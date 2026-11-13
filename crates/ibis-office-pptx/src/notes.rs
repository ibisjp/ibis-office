// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スピーカー ノート: the notes pane under the slide (表示 > スピーカー ノート
//! を表示), a plain text field for the current slide's notes. What is typed
//! is put in the document as it is typed (`Command::SetNotes`; one undo
//! step while the field keeps the focus).

use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex, ActiveTheme as _, Icon};
use crate::shell::icons::Material;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, SlideId};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The notes field and the slide it shows.
pub(crate) struct NotesField {
    pub(crate) slide: SlideId,
    /// The UI language its prompt is in (a new field when it changes).
    language: crate::shell::i18n::Language,
    pub(crate) state: Entity<TextareaState>,
    /// The last command is this field's typing (more joins it).
    pub(crate) typing: bool,
    _subscription: Subscription,
}

/// The pane's height, pixels.
pub(crate) const PANE_HEIGHT: f32 = 110.0;
/// The least height the dividers let the notes pane have (#39), pixels: about two lines of the notes' text and the pane's padding.
const PANE_MIN_HEIGHT: f32 = 44.0;
/// The least height of the slide pane, pixels: the slide is still seen as a slide (a 16:9 slide of this height is about 213 pixels wide).
/// ibisOffice's own rule; PowerPoint's limit depends on its window (tasks/doc/m117-pptx-notes-pane-limits.md) and is not copied.
pub(crate) const SLIDE_PANE_MIN_HEIGHT: f32 = 120.0;
const LIST_MIN_WIDTH: f32 = 120.0;
const LIST_MAX_WIDTH: f32 = 480.0;
/// The tallest the notes pane can be of the two panes together `total`: the slide pane keeps its least height.
fn notes_pane_most(total: f32) -> f32 {
    (total - SLIDE_PANE_MIN_HEIGHT).max(PANE_MIN_HEIGHT)
}
/// A border taken down until the notes pane would be less than half its least height hides the pane (`maximized`), as the pointer goes
/// past the least height far enough to show it is meant (PowerPoint: under 12.5 pixels, half of its 25-pixel least at the time, tasks/doc/m117-pptx-notes-pane-limits.md).
const HIDE_BELOW: f32 = PANE_MIN_HEIGHT / 2.0;

/// What the user did to the notes pane (see `PptxState::notes_touched`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct NotesTouched {
    /// The border was moved (the share is read at the save).
    pub(crate) moved: bool,
    /// The border was taken to the top, as far as the slide pane's least height lets it go and further: PowerPoint's `minimized`.
    pub(crate) to_top: bool,
    /// The border is being taken down to where the notes pane would be under half its least height: the pane is hidden when the button is let go.
    pub(crate) to_bottom: bool,
    /// The slide pane's share (1/1000 %) the border stood at before the last move or the hiding, when it had been moved before (PowerPoint's
    /// `restoredTop` for `minimized` and `maximized`); None: the border was never moved, the file's own stays.
    pub(crate) restored: Option<u32>,
    /// The pane was hidden or shown from the menu or its button.
    pub(crate) toggled: bool,
}

impl NotesTouched {
    /// What is written to `normalViewPr`, None when the pane was not touched. `hidden`: the pane is hidden now; `share`: the slide pane's share
    /// in 1/1000 %.
    pub(crate) fn to_save(self, hidden: bool, share: Option<u32>) -> Option<ibis_doc_model::NotesPaneSave> {
        use ibis_doc_model::NotesPaneSave;
        if !(self.moved || self.toggled) {
            return None;
        }
        if hidden {
            return Some(NotesPaneSave::Maximized(self.restored));
        }
        if self.moved {
            if self.to_top {
                return Some(NotesPaneSave::Minimized(self.restored));
            }
            return share.map(NotesPaneSave::Moved).or(Some(NotesPaneSave::Shown));
        }
        Some(NotesPaneSave::Shown)
    }
}

/// The slide pane's share of the panes together in 1/1000 %, rounded to a pixel of the slide pane (`slide` pixels of `total`).
fn slide_share_thousandths(slide: f32, total: f32) -> Option<u32> {
    (total > 0.0).then(|| ((slide.round() / total) * 100_000.0).round().clamp(0.0, 100_000.0) as u32)
}

impl crate::State {
    /// The slide pane's share of the slide and notes panes together, in 1/1000 %, rounded to a pixel of the slide pane as PowerPoint writes it
    /// (the workspace's height is what is left for the slide pane while the notes pane has its `notes_height`).
    pub(crate) fn notes_slide_share_thousandths(&self, workspace_height: Option<f32>) -> Option<u32> {
        let slide = workspace_height?;
        slide_share_thousandths(slide, slide + self.notes_height)
    }
}

/// A divider being dragged: where the pointer was and how large the pane was then.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PaneDrag {
    List { x0: f32, w0: f32 },
    /// `touched0`: what had been done to the pane before this drag (a drag taken to the bottom leaves it so).
    Notes { y0: f32, h0: f32, max: f32, touched0: NotesTouched },
}

#[ibis_office_macros::editor_ext(NotesExt)]
impl SlideEditor {
    /// Whether the notes pane shows (it does unless hidden from the menu).
    pub(crate) fn notes_shown(&self) -> bool {
        self.pptx().is_some_and(|s| !s.notes_hidden)
    }

    /// The slide list's width now: dragged, or the default.
    pub(crate) fn pane_width_now(&self) -> f32 {
        // One width for the thumbnails and the outline: switching the tab does not move the slide beside it.
        self.pptx().and_then(|s| s.pane_width).unwrap_or(super::thumbnails::PANE_WIDTH)
    }

    /// A thumbnail's width: it fills the list less its margins.
    pub(crate) fn thumbnail_width(&self) -> f32 {
        (self.pane_width_now() - 48.0).clamp(72.0, 420.0)
    }

    /// The divider between the slide list and the slide (#39).
    pub(crate) fn render_pane_divider(&self, cx: &mut Context<Self>) -> AnyElement {
        let w0 = self.pane_width_now();
        div()
            .id("pane-divider")
            .w(px(4.))
            .h_full()
            .flex_shrink_0()
            .cursor_col_resize()
            .hover(|s| s.bg(cx.theme().border))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let x0 = f32::from(event.position.x);
                if let Some(state) = this.pptx_mut() {
                    state.pane_drag = Some(PaneDrag::List { x0, w0 });
                }
            }))
            .into_any_element()
    }

    /// The divider between the slide and the notes pane (#39); none while the pane is hidden.
    pub(crate) fn render_notes_divider(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.notes_shown() || !self.pages_are_slides() || !self.has_document {
            return None;
        }
        let h0 = self.pptx().map_or(PANE_HEIGHT, |s| s.notes_height);
        let total = self.workspace_bounds.map_or(f32::MAX, |b| f32::from(b.size.height)) + h0;
        let max = notes_pane_most(total);
        Some(
            div()
                .id("notes-divider")
                .h(px(4.))
                .w_full()
                .flex_shrink_0()
                .cursor_row_resize()
                .hover(|s| s.bg(cx.theme().border))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    let y0 = f32::from(event.position.y);
                    if let Some(state) = this.pptx_mut() {
                        // A border moved before and not at the top now stands at a position PowerPoint keeps as `restoredTop` while this drag
                        // takes it to the top or the bottom.
                        if state.notes_touched.moved && !state.notes_touched.to_top {
                            state.notes_touched.restored = slide_share_thousandths(total - h0, total);
                        }
                        let touched0 = state.notes_touched;
                        state.pane_drag = Some(PaneDrag::Notes { y0, h0, max, touched0 });
                    }
                }))
                .into_any_element(),
        )
    }

    /// The pointer moves while a divider is held.
    pub(crate) fn drag_pane(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(drag) = self.pptx().and_then(|s| s.pane_drag) else { return };
        // The button was let go outside: the drag is over.
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_pane_drag(cx);
            return;
        }
        if let Some(state) = self.pptx_mut() {
            match drag {
                PaneDrag::List { x0, w0 } => state.pane_width = Some((w0 + f32::from(event.position.x) - x0).clamp(LIST_MIN_WIDTH, LIST_MAX_WIDTH)),
                PaneDrag::Notes { y0, h0, max, .. } => {
                    let wanted = h0 - (f32::from(event.position.y) - y0);
                    state.notes_height = wanted.clamp(PANE_MIN_HEIGHT, max);
                    state.notes_touched.moved = true;
                    // The pointer at the least height of the slide pane or higher: the notes pane the largest; down to where the notes pane
                    // would be under half its least height: hide it.
                    state.notes_touched.to_top = wanted >= max;
                    state.notes_touched.to_bottom = wanted < HIDE_BELOW;
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn end_pane_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.pptx_mut() {
            if let Some(drag) = state.pane_drag.take() {
                // The border let go at the bottom hides the pane (PowerPoint's `maximized`); the pane keeps the height it had before the drag
                // and what was done before stays, so that showing it again brings it back as it was.
                if let PaneDrag::Notes { h0, touched0, .. } = drag
                    && state.notes_touched.to_bottom
                {
                    state.notes_height = h0;
                    state.notes_hidden = true;
                    state.notes_touched = NotesTouched { toggled: true, to_bottom: false, ..touched0 };
                }
                state.notes_touched.to_bottom = false;
                cx.notify();
            }
        }
    }

    pub(crate) fn toggle_notes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_notes(window, cx);
        let height = self.workspace_bounds.map(|b| f32::from(b.size.height));
        if let Some(state) = self.pptx_mut() {
            // Hiding a border that was moved and is not at the top: PowerPoint keeps where it stood as `restoredTop`.
            if !state.notes_hidden && state.notes_touched.moved && !state.notes_touched.to_top {
                state.notes_touched.restored = state.notes_slide_share_thousandths(height);
            }
            state.notes_hidden = !state.notes_hidden;
            state.notes_touched.toggled = true;
        }
        cx.notify();
    }

    /// Copy (or cut if `cut` is true) the current slide's speaker notes to the system clipboard.
    pub(crate) fn copy_notes_text(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_notes(window, cx);
        let slide = self.current_slide;
        let Some(notes) = self.presentation().slide(slide).map(|s| s.notes.clone()) else { return };
        if !notes.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(notes));
        }
        if cut && !self.slide().notes.is_empty() {
            self.end_notes_typing();
            self.run(vec![Command::SetNotes { slide, notes: String::new() }], cx);
            if let Some(field) = self.pptx().and_then(|s| s.notes_field.as_ref()).filter(|f| f.slide == slide).map(|f| f.state.clone()) {
                field.update(cx, |s, cx| s.set_value(String::new(), window, cx));
            }
        }
    }

    /// Paste text from the system clipboard into the current slide's speaker notes.
    pub(crate) fn paste_notes_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_notes(window, cx);
        let Some(pasted) = cx.read_from_clipboard().and_then(|item| item.text()) else { return };
        let pasted = pasted.replace("\r\n", "\n").replace('\r', "\n");
        if pasted.is_empty() {
            return;
        }
        let slide = self.current_slide;
        let old = self.presentation().slide(slide).map(|s| s.notes.clone()).unwrap_or_default();
        let next = format!("{old}{pasted}");
        self.end_notes_typing();
        self.run(vec![Command::SetNotes { slide, notes: next.clone() }], cx);
        if let Some(field) = self.pptx().and_then(|s| s.notes_field.as_ref()).filter(|f| f.slide == slide).map(|f| f.state.clone()) {
            field.update(cx, |s, cx| s.set_value(next, window, cx));
        }
    }

    /// Clear all speaker notes on the current slide.
    pub(crate) fn clear_notes_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_notes(window, cx);
        let slide = self.current_slide;
        if self.presentation().slide(slide).is_some_and(|s| !s.notes.is_empty()) {
            self.end_notes_typing();
            self.run(vec![Command::SetNotes { slide, notes: String::new() }], cx);
            if let Some(field) = self.pptx().and_then(|s| s.notes_field.as_ref()).filter(|f| f.slide == slide).map(|f| f.state.clone()) {
                field.update(cx, |s, cx| s.set_value(String::new(), window, cx));
            }
        }
    }

    /// Context menu entries for the speaker notes pane and Notes Page View (PPTX-NOTES-001..006, PPTX-NOTES-014).
    pub fn notes_context_menu_entries(&self, cx: &App) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let has_notes = !self.slide().notes.is_empty()
            || self.pptx().and_then(|s| s.notes_field.as_ref()).is_some_and(|f| !f.state.read(cx).value().is_empty());
        let has_clip = cx.read_from_clipboard().and_then(|item| item.text()).is_some_and(|t| !t.is_empty());

        vec![
            MenuItem::tr("edit.cut").icon("content_cut").shortcut("⌘X").disabled(!has_notes).on(|e, w, cx| e.copy_notes_text(true, w, cx)).into(),
            MenuItem::tr("edit.copy").icon("content_copy").shortcut("⌘C").disabled(!has_notes).on(|e, w, cx| e.copy_notes_text(false, w, cx)).into(),
            MenuItem::tr("edit.paste").icon("content_paste").shortcut("⌘V").disabled(!has_clip).on(|e, w, cx| e.paste_notes_text(w, cx)).into(),
            MenuItem::tr("edit.delete").icon("delete").disabled(!has_notes).on(|e, w, cx| e.clear_notes_text(w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.notes-format")
                .icon("format_bold")
                .checked(self.is_notes_format())
                .on(|e, w, cx| {
                    if e.is_notes_format() {
                        e.close_notes_format(w, cx);
                    } else {
                        e.open_notes_format(w, cx);
                    }
                })
                .into(),
            MenuItem::tr("pptx.notes-master")
                .icon("sticky_note_2")
                .checked(self.is_notes_master())
                .on(|e, w, cx| {
                    if e.is_notes_master() {
                        e.close_notes_format(w, cx);
                    } else {
                        e.open_notes_master(w, cx);
                    }
                })
                .into(),
            MenuItem::tr("pptx.handout-master")
                .icon("view_agenda")
                .checked(self.is_handout_master())
                .on(|e, w, cx| {
                    if e.is_handout_master() {
                        e.close_notes_format(w, cx);
                    } else {
                        e.open_handout_master(w, cx);
                    }
                })
                .into(),
            MenuItem::tr("pptx.notes-header-footer")
                .icon("view_quilt")
                .on(|e, w, cx| e.open_notes_marks(w, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.notes-page-view")
                .icon("sticky_note_2")
                .shortcut("⌃⌥3")
                .checked(self.is_notes_page_view())
                .on(|e, w, cx| e.toggle_notes_page_view(w, cx))
                .into(),
            MenuItem::tr("pptx.normal-view")
                .icon("view_compact")
                .checked(self.is_normal_view())
                .on(|e, w, cx| e.switch_to_normal_view(w, cx))
                .into(),
            MenuItem::tr("pptx.notes")
                .icon("notes")
                .checked(self.notes_shown())
                .on(|e, w, cx| e.toggle_notes(w, cx))
                .into(),
        ]
    }

    /// Open context menu on the speaker notes pane or Notes Page View at `position`.
    pub(crate) fn open_notes_context_menu(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = self.notes_context_menu_entries(cx);
        self.open_popup_entries(entries, position, window, cx);
    }

    /// Put what the field holds in the document, if it changed.
    pub(crate) fn commit_notes(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some((slide, text)) = self.pptx().and_then(|s| s.notes_field.as_ref()).map(|f| (f.slide, f.state.read(cx).value().to_string())) else { return };
        let Some(old) = self.presentation().slide(slide).map(|s| s.notes.clone()) else { return };
        // The field's own line ends are `\n`; a pasted `\r\n` is one too.
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text != old {
            let typing = self.pptx().and_then(|s| s.notes_field.as_ref()).is_some_and(|f| f.typing);
            let command = vec![Command::SetNotes { slide, notes: text }];
            let result = if typing { self.document.amend_last(command) } else { self.document.execute(command) };
            if let Err(err) = result {
                log::warn!("notes rejected: {err}");
            }
            if let Some(f) = self.pptx_mut().and_then(|s| s.notes_field.as_mut()) {
                f.typing = true;
            }
            cx.notify();
        }
    }

    /// The notes field is no longer being typed in (it lost the focus, or
    /// another command ran): the next typing is a new undo step.
    pub(crate) fn end_notes_typing(&mut self) {
        if let Some(f) = self.pptx_mut().and_then(|s| s.notes_field.as_mut()) {
            f.typing = false;
        }
    }

    /// The field for the current slide (made, or moved to it), showing its
    /// notes as they are in the document unless they are being typed.
    fn notes_field(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<TextareaState>> {
        let current = self.current_slide;
        let notes = self.presentation().slide(current)?.notes.clone();
        let language = crate::shell::i18n::language();
        let field = self.pptx().and_then(|s| s.notes_field.as_ref()).map(|f| (f.slide, f.language, f.state.clone()));
        match field {
            Some((slide, shown, state)) if slide == current && shown == language => {
                // Undo and redo change the notes under the field.
                let focused = state.read(cx).focus_handle(cx).is_focused(window);
                if !focused && state.read(cx).value().as_ref() != notes {
                    state.update(cx, |s, cx| s.set_value(notes, window, cx));
                }
                Some(state)
            }
            _ => {
                if field.is_some() {
                    self.commit_notes(window, cx);
                }
                let state = cx.new(|cx| TextareaState::new(window, cx).placeholder(crate::shell::i18n::text("pptx.notes.click-add-notes")).default_value(notes));
                let subscription = cx.subscribe_in(&state, window, |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => this.commit_notes(window, cx),
                    InputEvent::Blur => {
                        this.commit_notes(window, cx);
                        this.end_notes_typing();
                    }
                    _ => {}
                });
                if let Some(s) = self.pptx_mut() {
                    s.notes_field = Some(NotesField { slide: current, language, state: state.clone(), typing: false, _subscription: subscription });
                }
                Some(state)
            }
        }
    }

    /// The notes pane, when it shows.
    pub(crate) fn render_notes_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.notes_shown() || !self.pages_are_slides() || !self.has_document {
            return None;
        }
        // The first frame with the room known sizes a pane that opened with a share of the panes (the file's normal view).
        if let (Some(percent), Some(bounds)) = (self.pptx().and_then(|s| s.notes_open_percent), self.workspace_bounds) {
            let notes = self.pptx().map_or(PANE_HEIGHT, |s| s.notes_height);
            let total = f32::from(bounds.size.height) + notes;
            let max = notes_pane_most(total);
            if let Some(state) = self.pptx_mut() {
                state.notes_height = (total * percent / 100.0).clamp(PANE_MIN_HEIGHT, max);
                state.notes_open_percent = None;
            }
        }
        let state = self.notes_field(window, cx)?;
        let theme = cx.theme();
        Some(
            div()
                .id("speaker-notes")
                .h(px(self.pptx().map_or(PANE_HEIGHT, |s| s.notes_height)))
                .relative()
                .flex_shrink_0()
                .border_t_1()
                .border_color(theme.border)
                .bg(theme.background)
                .px_3()
                .py_2()
                .text_sm()
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_notes_context_menu(event.position, window, cx);
                    }),
                )
                .child(Textarea::new(&state).appearance(false).size_full())
                // The close button at the top right (#38): the toolbar's button follows.
                .child(
                    div()
                        .id("notes-pane-close")
                        .absolute()
                        .top(px(2.))
                        .right(px(6.))
                        .size(px(22.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.muted))
                        .child(Icon::new(Material("close")).size(px(16.)).text_color(theme.muted_foreground))
                        .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text("_shell.hide-notes-pane")).build(window, cx))
                        .on_click(cx.listener(|this, _, window, cx| this.toggle_notes(window, cx))),
                )
                .into_any_element(),
        )
    }

    /// Notes Page View (ノート表示): A full-page layout showing the slide at the top
    /// and its speaker notes editing area at the bottom, mimicking PowerPoint's Notes Page view.
    pub(crate) fn render_notes_page_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let slide = self.slide().clone();
        let size = self.presentation().size_of(&slide);
        let preview_w = 460.0f32;
        let scale = (preview_w / size.width.max(1) as f32).min(300.0 / size.height.max(1) as f32);
        let (width, height) = (size.width as f32 * scale, size.height as f32 * scale);

        let ready = self.resources.preload.slide_ready(&slide);
        let shapes: Vec<AnyElement> = if ready {
            slide
                .shown_shapes()
                .map(|shape| crate::render::shape_view::shape_element(shape, shape.frame, scale, &self.resources, None))
                .collect()
        } else {
            Vec::new()
        };

        // The notes page's own shapes (a picture or a shape put on it), at
        // the scale the page (520 px wide) has to the notes page.
        let notes_size = self.presentation().notes_size;
        let page_scale = 520.0 / notes_size.width.max(1) as f32;
        let page_height = notes_size.height as f32 * page_scale;
        let notes_shapes: Vec<AnyElement> = if ready {
            slide.notes_shapes.iter().map(|shape| crate::render::shape_view::shape_element(shape, shape.frame, page_scale, &self.resources, None)).collect()
        } else {
            Vec::new()
        };

        // The header, date, footer and number the notes pages carry, where the notes master puts them.
        let marks = self.presentation().notes_marks.clone();
        let number = self.presentation().slide_index(self.current_slide).unwrap_or(0) + self.presentation().first_slide_number;
        let mark_text = |kind: &str| -> Option<String> {
            match kind {
                "hdr" => marks.header.clone(),
                "dt" => marks.date.as_ref().map(|d| match d {
                    ibis_doc_model::NotesDate::Auto(t) | ibis_doc_model::NotesDate::Fixed(t) => t.clone(),
                }),
                "ftr" => marks.footer.clone(),
                _ => marks.number.then(|| number.to_string()),
            }
        };
        let mark_elements: Vec<AnyElement> = self
            .presentation()
            .notes_frames
            .iter()
            .filter_map(|(kind, frame)| {
                let text = mark_text(kind).filter(|t| !t.is_empty())?;
                let right = matches!(kind.as_str(), "dt" | "sldNum");
                let bottom = matches!(kind.as_str(), "ftr" | "sldNum");
                let mut d = div()
                    .absolute()
                    .left(px(frame.x as f32 * page_scale))
                    .top(px(frame.y as f32 * page_scale))
                    .w(px(frame.width as f32 * page_scale))
                    .h(px(frame.height as f32 * page_scale))
                    .flex()
                    .px_1()
                    .text_size(px(12.0 * 12_700.0 * page_scale))
                    .text_color(black());
                d = if bottom { d.items_end() } else { d.items_start() };
                d = if right { d.justify_end() } else { d.justify_start() };
                Some(d.child(text).into_any_element())
            })
            .collect();

        let state = self.notes_field(window, cx);
        let theme = cx.theme();

        let slide_index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let total_slides = self.presentation().slides.len();

        let header = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .px_6()
            .py_2()
            .bg(theme.sidebar)
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(crate::shell::i18n::text("pptx.notes-page-view")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(crate::shell::i18n::format("_pptx.slide-n-of-total", &[("n", &(slide_index + 1)), ("total", &total_slides)])),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .id("notes-format-edit")
                            .px_2p5()
                            .py_1()
                            .rounded_sm()
                            .border_1()
                            .border_color(theme.border)
                            .text_xs()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.accent))
                            .text_color(theme.foreground)
                            .on_click(cx.listener(|this, _, window, cx| this.open_notes_format(window, cx)))
                            .child(crate::shell::i18n::text("pptx.notes.edit-notes-formatting")),
                    )
                    .child(
                div()
                    .id("exit-notes-view")
                    .px_2p5()
                    .py_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.accent))
                    .text_color(theme.foreground)
                    .on_click(cx.listener(|this, _, window, cx| this.switch_to_normal_view(window, cx)))
                    .child(crate::shell::i18n::text("_pptx.return-to-normal-view")),
                    ),
            );

        let page = v_flex()
            .relative()
            .w(px(520.0))
            .h(px(page_height))
            .bg(crate::shell::palette::white())
            .rounded_md()
            .shadow_lg()
            .border_1()
            .border_color(theme.border)
            .p_6()
            .gap_4()
            .items_center()
            // 1. スライド縮小プレビュー
            .child(
                div()
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border)
                    .shadow_sm()
                    .bg(crate::shell::palette::white())
                    .child(crate::render::shape_view::slide_backdrop(&slide, scale, &self.resources))
                    .children(shapes),
            )
            // 2. ノート入力・表示領域
            .child(
                v_flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .border_1()
                    .border_color(theme.border)
                    .rounded_sm()
                    .p_3()
                    .bg(theme.background)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .pb_1()
                            .child(crate::shell::i18n::text("_pptx.speaker-notes")),
                    )
                    .when_some(state, |d, st| {
                        d.child(Textarea::new(&st).appearance(false).size_full())
                    }),
            )
            // 3. ノート ページ上の図・図形
            .children(notes_shapes)
            .children(mark_elements);

        v_flex()
            .id("notes-page-workspace")
            .key_context(crate::shell::editor::CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &crate::shell::actions::CancelTool, window, cx| this.switch_to_normal_view(window, cx)))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.switch_to_normal_view(window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_notes_context_menu(event.position, window, cx);
                }),
            )
            .size_full()
            .bg(theme.muted)
            .child(header)
            .child(
                v_flex()
                    .id("notes-page-scroll")
                    .flex_1()
                    .size_full()
                    .overflow_y_scroll()
                    .items_center()
                    .py_8()
                    .child(page),
            )
            .into_any_element()
    }
}
