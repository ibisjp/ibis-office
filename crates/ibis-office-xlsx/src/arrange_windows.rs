// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示 > ウィンドウの整列 (Arrange Windows):
//! Arrange multiple workbook windows (Tiled, Horizontal, Vertical, Cascade)
//! with option for windows of active workbook (XLSX-SHEET-017). The panel is the
//! workbook's; the arranging itself is the window's (shell/arrange_windows.rs).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::v_flex;
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

// Shared with the window: shell/arrange_kind.rs.
pub use crate::shell::arrange_kind::ArrangeKind;
use crate::ext::*;

pub struct ArrangeWindowsPanel {
    pub(crate) kind: ArrangeKind,
    pub(crate) active_workbook_only: bool,
    pub ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

#[ibis_office_macros::editor_ext(ArrangeWindows)]
impl SlideEditor {
    pub(crate) fn toggle_arrange_windows_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.wb().panels.arrange_windows.is_some() {
            self.wb_mut().panels.arrange_windows = None;
            self.sidebar_open = false;
            cx.notify();
        } else {
            self.open_arrange_windows_panel(cx);
        }
    }

    pub fn open_arrange_windows_panel(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        self.wb_mut().close_settings();
        self.wb_mut().panels.arrange_windows = Some(ArrangeWindowsPanel {
            kind: ArrangeKind::Tiled,
            active_workbook_only: false,
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        });
        self.sidebar_open = true;
        cx.notify();
    }

    pub fn set_arrange_kind(&mut self, kind: ArrangeKind, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.arrange_windows.as_mut() {
            p.kind = kind;
        }
        cx.notify();
    }

    pub(crate) fn toggle_arrange_active_workbook_only(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.arrange_windows.as_mut() {
            p.active_workbook_only = !p.active_workbook_only;
        }
        cx.notify();
    }

    pub fn apply_arrange_windows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (kind, active_only) = match self.wb().panels.arrange_windows.as_ref() {
            Some(p) => (p.kind, p.active_workbook_only),
            None => (ArrangeKind::Tiled, false),
        };
        self.wb_mut().panels.arrange_windows = None;
        self.sidebar_open = false;
        cx.notify();

        let current_handle = window.window_handle();
        // 作業中のブックのウィンドウを整列する: the windows of this workbook alone.
        let only = active_only.then(|| self.window_file());

        Self::arrange_windows_impl(kind, Some((window, current_handle)), only, cx);
    }

    pub fn arrange_windows_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.arrange_windows.as_ref()?;
        let kind = p.kind;
        let active_only = p.active_workbook_only;

        let ui = p.ui.clone();
        let mark = move |id: &str, el: AnyElement| {
            let id = id.to_string();
            let ui = ui.clone();
            div()
                .relative()
                .child(el)
                .child(
                    canvas(|_, _, _| {}, move |bounds, _, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let id = id.to_string();
            let ui = p.ui.clone();
            action.on_bounds(move |bounds, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
        };

        let mut body = v_flex().gap_1().px(px(16.)).py(px(8.)).p_0();

        body = body.child(
            div()
                .text_sm()
                .font_weight(FontWeight::BOLD)
                .child(text("fix.arrange-windows.arrange")),
        );

        // Shared radio buttons, one option per row.
        let radio_btn = |mode: ArrangeKind, label_key: &'static str, id_btn: &'static str, id_mark: &'static str, cx: &mut Context<Self>| {
            let selected = kind == mode;
            mark(
                id_mark,
                crate::shell::chrome::radio_button(
                    id_btn,
                    selected,
                    false,
                    text(label_key),
                    cx.listener(move |this, _, _, cx| {
                        this.set_arrange_kind(mode, cx);
                    }),
                )
                .into_any_element(),
            )
        };

        body = body.child(crate::shell::chrome::radio_group()
            .child(radio_btn(ArrangeKind::Tiled, "fix.arrange-windows.tiled", "btn-arrange-tiled", "arrange-tiled", cx))
            .child(radio_btn(ArrangeKind::Horizontal, "fix.arrange-windows.horizontal", "btn-arrange-horizontal", "arrange-horizontal", cx))
            .child(radio_btn(ArrangeKind::Vertical, "fix.arrange-windows.vertical", "btn-arrange-vertical", "arrange-vertical", cx))
            .child(radio_btn(ArrangeKind::Cascade, "fix.arrange-windows.cascade", "btn-arrange-cascade", "arrange-cascade", cx))
        );

        body = body.child(div().h(px(6.)));

        // Checkbox: active workbook only (shared toggle_row component, left-aligned)
        body = body.child(mark(
            "arrange-active-only",
            crate::toggle::toggle_row(
                "btn-arrange-active-only",
                active_only,
                text("fix.arrange-windows.windows-active-workbook"),
                cx.listener(|this, _, _, cx| {
                    this.toggle_arrange_active_workbook_only(cx);
                }),
            )
            .into_any_element(),
        ));

        // Execute button ("実行") in sticky bottom footer
        let footer = crate::shell::chrome::action_bar().action(mark_action("arrange-ok", crate::shell::chrome::ActionDefinition::new("btn-arrange-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(text("_xlsx.run"))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.apply_arrange_windows(window, cx);
                }))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));

        Some(SidebarPanel::with_footer(
            text("fix.arrange-windows.arrange-windows"),
            body.into_any_element(),
            footer,
        ))
    }
}
