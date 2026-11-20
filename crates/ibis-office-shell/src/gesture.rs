// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A touch that a second finger cancels (#209; design: `tasks/doc/design/touch-gesture-transaction.md`).
//!
//! The first finger of a touch screen works like the mouse: a press selects, starts a drag, moves the caret. When a
//! second finger comes down, the two fingers are a pinch and **what the first finger did must not stay**. gpui tells
//! the window about it with `TouchSessionEvent` (`Started` when the first finger goes down, `Ended` when it lifts,
//! `Cancelled` when it was taken back). This module is the **one place** that answers:
//!
//! * `Started`: [`GestureTransaction`] remembers the document's history mark (`Document::begin_gesture`) and a
//!   [`ViewSnapshot`] of what a touch on the page can change (selection, page, drag, text being edited, zoom, scroll,
//!   the format's own state: `GestureView`).
//! * `Cancelled`: the document's edits since are taken back (not redoable), the snapshot is put back, and the work
//!   queued with [`SlideEditor::after_gesture`] is dropped.
//! * `Ended`: the edits become one undo step (Undo once takes all of the touch back) and the queued work runs.
//!
//! Nothing in the handlers of the page has to know: they change the document with `execute` and the view as they
//! always did, and it is all taken back. What the transaction cannot take back must wait for `Ended`: **an effect
//! outside the document and the view (the clipboard, a file, the network, a dialog) goes through
//! [`SlideEditor::after_gesture`]** (`tests/gesture_rule.rs` fails when a pointer handler calls the clipboard or the
//! file system directly).

use std::any::Any;

use gpui_kit::*;
use ibis_doc_model::{GestureMark, ShapeId, SlideId};

use crate::editor::{Drag, SlideEditor, Tool};
use crate::format::with_format;
use crate::text_edit::TextEditing;

/// Work for after the touch is final (see [`SlideEditor::after_gesture`]).
type Later = Box<dyn FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>)>;

/// The open touch gesture: kept in `SlideEditor::gesture` from the first finger down until it lifts or is cancelled.
pub(crate) struct GestureTransaction {
    mark: GestureMark,
    view: ViewSnapshot,
    later: Vec<Later>,
}

/// What a touch on the page can change in the window, as it was when the finger went down.
///
/// `ViewSnapshot::take` destructures `SlideEditor` and `ShapeEditState` **without `..`**: a field added to either does not compile
/// until its author decides whether a touch can change it (put it in the snapshot) or not (`field: _`, with the reason in the
/// comment above the list). The format's own state is `FormatState::gesture_view` (the same rule, `GestureView`).
pub(crate) struct ViewSnapshot {
    pub(crate) selection: Option<ShapeId>,
    pub(crate) current_slide: SlideId,
    pub(crate) tool: Tool,
    pub(crate) drag: Option<Drag>,
    pub(crate) shape_selection: Vec<ShapeId>,
    pub(crate) shape_tool: crate::shape_state::Tool,
    pub(crate) shape_drag: Option<crate::shape_state::Drag>,
    pub(crate) shape_shift: bool,
    pub(crate) shape_edit: Option<TextEditing>,
    pub(crate) shape_goal_x: Option<(usize, f32)>,
    pub(crate) shape_pending_click: Option<Point<Pixels>>,
    pub(crate) zoom: Option<f32>,
    pub(crate) scroll: Point<Pixels>,
    pub(crate) format: Box<dyn Any>,
}

impl SlideEditor {
    /// A touch session of the window (the first finger down, lifted, or taken back): see the module docs.
    pub(crate) fn on_touch_session(&mut self, event: &TouchSessionEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.phase {
            TouchPhase::Started => {
                // (A session still open means the end was lost: it is final as it stands.)
                self.end_touch_session(true, window, cx);
                let in_page = self.workspace_bounds.is_some_and(|bounds| bounds.contains(&event.position));
                if self.has_document && in_page && !self.dialog_is_modal() {
                    self.begin_touch_session();
                }
            }
            TouchPhase::Ended => self.end_touch_session(true, window, cx),
            TouchPhase::Cancelled => self.end_touch_session(false, window, cx),
            TouchPhase::Moved => {}
        }
    }

    fn begin_touch_session(&mut self) {
        let mark = self.document.begin_gesture();
        self.gesture = Some(GestureTransaction { mark, view: self.take_view_snapshot(), later: Vec::new() });
    }

    /// Whether a touch gesture is open (its changes can still be taken back).
    pub fn gesture_is_open(&self) -> bool {
        self.gesture.is_some()
    }

    /// Run `effect` once the touch gesture is final: at once when no touch gesture is open, at the finger's lift when one is, never
    /// when it is cancelled. **The way to do what cannot be taken back** (the clipboard, a file, the network, a dialog) from a
    /// handler of a press or a drag.
    pub fn after_gesture(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        effect: impl FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static,
    ) {
        match self.gesture.as_mut() {
            Some(open) => open.later.push(Box::new(effect)),
            None => effect(self, window, cx),
        }
    }

    /// End the open touch gesture: `keep` makes it final (one undo step, the queued work runs), else it is taken back.
    pub(crate) fn end_touch_session(&mut self, keep: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.gesture.take() else { return };
        let GestureTransaction { mark, view, later } = open;
        if keep {
            self.document.commit_gesture(mark);
            for effect in later {
                effect(self, window, cx);
            }
            return;
        }
        drop(later);
        if !self.document.rollback_gesture(mark) {
            // The user's own Undo or Redo came in between: the history is theirs, and the view is where they left it.
            return;
        }
        // What an undo does to the window after the document changed (`SlideEditor::undo`), then the view as it was.
        with_format!(self, history_applied_early(self, window, cx));
        with_format!(self, view_from_model(self));
        self.restore_view(view);
        self.fix_selection();
        with_format!(self, history_applied(self, cx));
        self.sync_peers(cx);
        cx.notify();
    }

    fn restore_view(&mut self, view: ViewSnapshot) {
        let ViewSnapshot {
            selection,
            current_slide,
            tool,
            drag,
            shape_selection,
            shape_tool,
            shape_drag,
            shape_shift,
            shape_edit,
            shape_goal_x,
            shape_pending_click,
            zoom,
            scroll,
            format,
        } = view;
        if self.presentation().slide(current_slide).is_some() {
            self.set_current_slide(current_slide);
        }
        self.selection = selection;
        self.tool = tool;
        self.drag = drag;
        self.shape_edit.selection = shape_selection;
        self.shape_edit.tool = shape_tool;
        self.shape_edit.drag = shape_drag;
        self.shape_edit.shift = shape_shift;
        self.shape_edit.edit = shape_edit;
        self.shape_edit.goal_x = shape_goal_x;
        self.shape_edit.pending_click = shape_pending_click;
        self.zoom = zoom;
        self.workspace_scroll.set_offset(scroll);
        self.format.restore_gesture_view(format);
    }
}

/// An element (put once in the window's root) that hands the touch sessions of the window to the editor, before anything else
/// of the touch is handled: see the module docs.
pub fn touch_session_listener(cx: &Context<SlideEditor>) -> impl IntoElement {
    let editor = cx.entity().downgrade();
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let editor = editor.clone();
            window.on_mouse_event(move |event: &TouchSessionEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    editor.update(cx, |this, cx| this.on_touch_session(event, window, cx)).ok();
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
