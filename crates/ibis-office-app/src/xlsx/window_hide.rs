// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示 > ウィンドウの非表示 / ウィンドウの再表示…:
//! Hide and Unhide Workbook Window (`SheetCommand::SetWindowHidden`).
//! ECMA-376 Part 1 §18.2.30 `<bookViews><workbookView visibility="hidden"/>`.
//! Restriction: when workbook windows are locked (`<workbookProtection lockWindows="1"/>`),
//! hiding and unhiding are disallowed (XLSX-VIEW-015).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct UnhideWindowPanel {
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    pub(crate) fn is_workbook_window_hidden(&self) -> bool {
        self.document.presentation().is_workbook_window_hidden()
    }

    pub(crate) fn is_workbook_windows_locked(&self) -> bool {
        self.document.presentation().is_workbook_windows_locked()
    }

    pub(crate) fn workbook_display_name(&self) -> String {
        self.file_name
            .as_ref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| text("Book1").to_string())
    }

    pub(crate) fn hide_workbook_window(&mut self, cx: &mut Context<Self>) {
        if self.is_workbook_windows_locked() {
            return;
        }
        self.commit_cell_edit(cx);
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetWindowHidden { hidden: true })], cx);
        self.wb_mut().panels.unhide_window = None;
        cx.notify();
    }

    pub(crate) fn unhide_workbook_window(&mut self, cx: &mut Context<Self>) {
        if self.is_workbook_windows_locked() {
            return;
        }
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetWindowHidden { hidden: false })], cx);
        self.wb_mut().panels.unhide_window = None;
        self.sidebar_open = false;
        cx.notify();
    }

    pub(crate) fn toggle_unhide_window_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.wb().panels.unhide_window.is_some() {
            self.wb_mut().panels.unhide_window = None;
            self.sidebar_open = false;
            cx.notify();
        } else {
            self.open_unhide_window_panel(cx);
        }
    }

    pub(crate) fn open_unhide_window_panel(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        self.wb_mut().close_settings();
        let message = if self.is_workbook_windows_locked() {
            text("_xlsx.this-can-t-be-done-because-the-workbook-window-i").to_string()
        } else {
            String::new()
        };
        self.wb_mut().panels.unhide_window = Some(UnhideWindowPanel {
            message,
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        });
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn unhide_window_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.unhide_window.as_ref()?;
        let is_locked = self.is_workbook_windows_locked();
        let is_hidden = self.is_workbook_window_hidden();
        let wb_name = self.workbook_display_name();

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

        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();

        if is_locked {
            body = body.child(
                div()
                    .p(px(8.))
                    .rounded_md()
                    .bg(cx.theme().danger.opacity(0.1))
                    .border_1()
                    .border_color(cx.theme().danger)
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(text("_xlsx.this-can-t-be-done-because-the-workbook-window-i")),
            );
        } else if !p.message.is_empty() {
            body = body.child(
                div()
                    .p(px(6.))
                    .rounded_md()
                    .bg(cx.theme().danger.opacity(0.1))
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(SharedString::from(p.message.clone())),
            );
        }

        body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.unhide-workbook")));

        let mut list_box = v_flex().gap_1().p(px(4.)).border_1().border_color(cx.theme().border).rounded_md().min_h(px(80.));
        if !is_hidden {
            list_box = list_box.child(div().text_sm().text_color(cx.theme().muted).child(text("_xlsx.no-hidden-workbooks")));
        } else {
            let item_el = div()
                .px(px(6.))
                .py(px(4.))
                .rounded_sm()
                .text_sm()
                .bg(cx.theme().accent.opacity(0.2))
                .child(SharedString::from(wb_name));
            list_box = list_box.child(mark(
                "unhide-window-item-0",
                Button::new("unhide-item-0")
                    .ghost()
                    .w_full()
                    .child(item_el)
                    .into_any_element(),
            ));
        }
        body = body.child(list_box);

        // Buttons
        let can_unhide = is_hidden && !is_locked;
        let mut buttons_row = crate::shell::chrome::action_bar();
        buttons_row = buttons_row.action(mark_action("unhide-window-ok", crate::shell::chrome::ActionDefinition::new("btn-unhide-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                .disabled(!can_unhide)
                .label(text("OK"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.unhide_workbook_window(cx);
                }))));
        buttons_row = buttons_row.action(mark_action("unhide-window-cancel", crate::shell::chrome::ActionDefinition::new("btn-unhide-cancel", "", crate::shell::chrome::ActionButtonKind::Normal)
                .label(text("_shell.cancel"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.wb_mut().panels.unhide_window = None;
                    this.sidebar_open = false;
                    cx.notify();
                }))));


        Some(SidebarPanel::with_footer(text("_xlsx.unhide-window"), body.into_any_element(), buttons_row))
    }

    /// When workbook window is hidden, render an empty workspace placeholder.
    pub(crate) fn render_hidden_workbook_view(&mut self, _window: &mut Window, cx: &mut Context<SlideEditor>) -> AnyElement {
        let is_locked = self.is_workbook_windows_locked();
        let placeholder = v_flex()
            .id("hidden-workspace")
            .key_context(crate::shell::editor::SHEET_CONTEXT)
            .track_focus(&self.focus_handle)
            .flex_1()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .bg(cx.theme().background)
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(cx.theme().muted)
                    .child(text("_xlsx.the-workbook-is-hidden")),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(cx.theme().muted)
                    .child(text("_xlsx.to-show-the-workbook-again-choose-unhide-window")),
            )
            .child(
                crate::shell::chrome::action_button("btn-hidden-unhide")
                    .small()
                    .disabled(is_locked)
                    .label(text("_xlsx.unhide"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_unhide_window_panel(cx);
                    })),
            );

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(placeholder)
            .into_any_element()
    }
}
