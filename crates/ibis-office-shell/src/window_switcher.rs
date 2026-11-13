// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The quick switcher between the open documents (menu bar redesign, stage 4; `tasks/doc/menu-redesign/implementation-spec.md`
//! section 3), the Window menu's 「ウィンドウ一覧…」 (Ctrl+Tab), and the next and previous window.
//!
//! The list is the shared popup menu (`PopupMenu` filled by `fill_scrollable_popup_menu`, with the shared scrolling), one row per
//! open document, the most recently used first, in a popup at the top centre of the window under the menu bar. It is not a
//! dialog and has no backdrop. The row after the current one (the window used just before) is the one the list opens on;
//! ↑↓ and Tab move (wrapping at the ends), Enter or a click brings the window forward, Esc and a click outside close
//! it. Confirming by letting go of the modifier keys is a later stage.
//! This file is built for the desktop only: the browser has one document per tab and no windows to switch between.
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::*;

use crate::chrome::{menu_width_for_entries, MenuEntry, MenuItem};
use crate::editor::SlideEditor;
use crate::menu_bar::BAR_BOTTOM;
use crate::menu_context::format_color;
use crate::open_documents::{OpenDocument, OpenDocuments};

/// The list is this wide at least and at most (the spec: 360 to 420 px).
const MIN_WIDTH: f32 = 360.0;
const MAX_WIDTH: f32 = 420.0;

/// The open switcher: its popup, and the documents in the order of its rows.
pub struct WindowSwitcher {
    popup: Entity<PopupMenu>,
    /// The documents of the rows, most recently used first.
    pub rows: Vec<EntityId>,
    width: f32,
    /// The row to put the current mark on once the popup has been drawn (`true`: the last row, else the second).
    pending: Option<bool>,
    frames: u8,
    _dismiss: Subscription,
}

/// A document's line in a list: its name (with ● when it has unsaved changes); the row's mark is the format's icon.
pub(crate) fn document_label(doc: &OpenDocument) -> SharedString {
    if doc.dirty { SharedString::from(format!("{} ●", doc.name)) } else { doc.name.clone() }
}

impl SlideEditor {
    /// Open the list of windows, on the window used just before this one (the last row with `from_end`).
    pub fn open_window_switcher(&mut self, from_end: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menu(cx);
        let docs = OpenDocuments::mru(cx);
        if docs.is_empty() {
            return;
        }
        let me = self.entity_id;
        let entries: Vec<MenuEntry> = docs
            .iter()
            .map(|doc| {
                let id = doc.id;
                let mut item = MenuItem::new("window.open.switch", document_label(doc))
                    .format_mark(format_color(doc.kind), crate::format::info(doc.kind).icon)
                    .checked(id == me)
                    .on(move |this, window, cx| this.activate_open_document(id, window, cx));
                if id == me {
                    item = item.note(crate::i18n::text("window.current"));
                }
                MenuEntry::Item(item)
            })
            .collect();
        let width = menu_width_for_entries(&entries).clamp(MIN_WIDTH, MAX_WIDTH);
        let editor = cx.entity().downgrade();
        let rows: Vec<EntityId> = docs.iter().map(|d| d.id).collect();
        let popup = PopupMenu::build(window, cx, {
            let entries = entries.clone();
            move |menu, window, cx| crate::menu_bar::fill_scrollable_popup_menu(menu, entries.clone(), width, &editor, window, cx)
        });
        popup.focus_handle(cx).focus(window, cx);
        let dismiss = cx.subscribe_in(&popup, window, |this, _, _: &DismissEvent, window, cx| {
            this.window_switcher = None;
            this.focus_handle.focus(window, cx);
            cx.notify();
        });
        self.window_switcher = Some(WindowSwitcher { popup, rows, width, pending: Some(from_end), frames: 0, _dismiss: dismiss });
        // The current row is the second (the window used before this one), or the last when asked; with one window, the first.
        cx.notify();
    }

    /// Once a frame: the popup can take the arrow keys only after a frame has drawn it, so the first frame waits and the next
    /// puts the current mark on its row (the second, or the last).
    pub(crate) fn advance_window_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(switcher) = self.window_switcher.as_mut() else { return };
        let Some(from_end) = switcher.pending else { return };
        if switcher.frames < 1 {
            switcher.frames += 1;
            cx.notify();
            return;
        }
        switcher.pending = None;
        let focus = switcher.popup.focus_handle(cx);
        let keys: &[bool] = if from_end { &[false] } else { &[true, true] };
        for down in keys {
            if *down {
                focus.dispatch_action(&gpui_kit::base::actions::SelectDown, window, cx);
            } else {
                focus.dispatch_action(&gpui_kit::base::actions::SelectUp, window, cx);
            }
        }
    }

    pub fn close_window_switcher(&mut self, cx: &mut Context<Self>) {
        if self.window_switcher.take().is_some() {
            cx.notify();
        }
    }

    /// Bring the window that shows document `id` to the front (and make it the most recently used).
    pub fn activate_open_document(&mut self, id: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        // The list's popup had the focus: it goes back to the editor, so that its keys work again.
        if self.window_switcher.is_some() {
            self.focus_handle.focus(window, cx);
        }
        self.close_window_switcher(cx);
        if id == self.entity_id {
            return;
        }
        let Some(doc) = OpenDocuments::read(cx).into_iter().find(|d| d.id == id) else { return };
        OpenDocuments::activate(cx, id);
        let _ = doc.window.update(cx, |_, window, _| window.activate_window());
    }

    /// Arrange the open windows (the Window menu's 左右に並べて表示 and 上下に並べて表示: the same arranging the sheet's panel offers).
    pub(crate) fn arrange_windows_of(kind: crate::arrange_kind::ArrangeKind, window: &mut Window, cx: &mut App) {
        let handle = window.window_handle();
        Self::arrange_windows_impl(kind, Some((window, handle)), None, cx);
    }

    /// すべてを手前に移動: every document window is brought forward, this one last (so it stays in front).
    pub(crate) fn bring_all_windows_front(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = self.entity_id;
        for doc in OpenDocuments::by_open_order(cx) {
            if doc.id != me {
                let _ = doc.window.update(cx, |_, w, _| w.activate_window());
            }
        }
        window.activate_window();
        OpenDocuments::activate(cx, me);
    }

    /// すべての書類を閉じる: each window closes as 閉じる does (a document with changes asks to save first, in its own window). This window
    /// closes last.
    pub(crate) fn close_all_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = self.entity_id;
        for doc in OpenDocuments::by_open_order(cx) {
            if doc.id != me {
                let editor = doc.editor.clone();
                let _ = doc.window.update(cx, |_, w, cx| {
                    let _ = editor.update(cx, |e, cx| e.close_file(&crate::actions::CloseFile, w, cx));
                });
            }
        }
        self.close_file(&crate::actions::CloseFile, window, cx);
    }

    /// The next (or previous) window in the order the documents were opened, wrapping round at the ends.
    pub fn step_window(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(doc) = OpenDocuments::neighbour(cx, self.entity_id, forward) {
            self.activate_open_document(doc.id, window, cx);
        }
    }

    /// The switcher's popup, drawn over the window at its top centre under the menu bar.
    pub(crate) fn render_window_switcher(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let switcher = self.window_switcher.as_ref()?;
        let left = ((f32::from(window.viewport_size().width) - switcher.width) / 2.0).max(8.0);
        Some(
            deferred(
                anchored().position(point(px(left), px(BAR_BOTTOM))).anchor(Anchor::TopLeft).snap_to_window_with_margin(px(8.)).child(
                    div()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        // Tab and Shift+Tab move the current row as the arrow keys do.
                        .on_key_down(cx.listener(|_, event: &KeyDownEvent, window, cx| {
                            if event.keystroke.key == "tab" {
                                if event.keystroke.modifiers.shift {
                                    window.dispatch_action(Box::new(gpui_kit::base::actions::SelectUp), cx);
                                } else {
                                    window.dispatch_action(Box::new(gpui_kit::base::actions::SelectDown), cx);
                                }
                                cx.stop_propagation();
                            }
                        }))
                        .child(switcher.popup.clone()),
                ),
            )
            .with_priority(gpui_kit::base::POPUP_PRIORITY)
            .into_any_element(),
        )
    }

    /// Whether the list of windows is open (for the scenarios).
    #[allow(dead_code)]
    pub fn window_switcher_open(&self) -> bool {
        self.window_switcher.is_some()
    }
}
