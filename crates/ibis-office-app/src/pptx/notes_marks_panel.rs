// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ノートと配布資料のヘッダーとフッター (挿入 > ノートのヘッダーとフッター…):
//! the header, date, footer and page number the notes pages carry.
use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, NotesDate, NotesMarks};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct NotesMarksPanel {
    pub(crate) header: bool,
    pub(crate) header_text: Entity<InputState>,
    pub(crate) date: bool,
    /// 自動的に更新: the date a field, else the text typed.
    pub(crate) auto_date: bool,
    pub(crate) date_text: Entity<InputState>,
    pub(crate) footer: bool,
    pub(crate) footer_text: Entity<InputState>,
    pub(crate) number: bool,
}

impl SlideEditor {
    pub(crate) fn open_notes_marks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let marks = self.presentation().notes_marks.clone();
        let (auto_date, date_value) = match &marks.date {
            Some(NotesDate::Auto(_)) | None => (true, super::header_footer::today()),
            Some(NotesDate::Fixed(t)) => (false, t.clone()),
        };
        let header_text = cx.new(|cx| InputState::new(window, cx).default_value(marks.header.clone().unwrap_or_default()));
        let footer_text = cx.new(|cx| InputState::new(window, cx).default_value(marks.footer.clone().unwrap_or_default()));
        let date_text = cx.new(|cx| InputState::new(window, cx).default_value(date_value));
        if let Some(s) = self.pptx_mut() {
            s.section_panel = None;
            s.layout_panel = None;
            s.print_panel = None;
            s.header_footer = None;
            s.notes_marks_panel = Some(NotesMarksPanel { header: marks.header.is_some(), header_text, date: marks.date.is_some(), auto_date, date_text, footer: marks.footer.is_some(), footer_text, number: marks.number });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_notes_marks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.notes_marks_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn toggle_notes_marks(&mut self, which: &str, on: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.notes_marks_panel.as_mut()) {
            match which {
                "header" => p.header = on,
                "date" => p.date = on,
                "auto" => p.auto_date = on,
                "footer" => p.footer = on,
                _ => p.number = on,
            }
        }
        cx.notify();
    }

    pub(crate) fn clear_notes_marks(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.notes_marks_panel.as_mut()) {
            p.header = false;
            p.date = false;
            p.footer = false;
            p.number = false;
        }
        cx.notify();
    }

    /// すべてに適用: the panel's choices on every notes page.
    pub(crate) fn apply_notes_marks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.notes_marks_panel.as_ref()) else { return };
        let date_text = p.date_text.read(cx).value().to_string();
        let marks = NotesMarks {
            header: p.header.then(|| p.header_text.read(cx).value().to_string()),
            date: p.date.then(|| if p.auto_date { NotesDate::Auto(super::header_footer::today()) } else { NotesDate::Fixed(date_text) }),
            footer: p.footer.then(|| p.footer_text.read(cx).value().to_string()),
            number: p.number,
        };
        if marks == self.presentation().notes_marks || self.apply_pptx_commands_checked(vec![Command::SetNotesMarks { marks }], cx) {
            self.close_notes_marks(window, cx);
        }
    }

    pub(crate) fn notes_marks_context_menu_entries(&self) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(p) = self.pptx().and_then(|s| s.notes_marks_panel.as_ref()) else {
            return Vec::new();
        };
        let header = p.header;
        let date = p.date;
        let auto_date = p.auto_date;
        let number = p.number;
        let footer = p.footer;

        vec![
            MenuItem::tr("pptx.notes-marks-panel.apply-all")
                .icon("done_all")
                .on(|this, w, cx| this.apply_notes_marks(w, cx))
                .into(),
            MenuItem::tr("pptx.find.clear")
                .icon("clear")
                .on(|this, _, cx| this.clear_notes_marks(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.header")
                .checked(header)
                .on(move |this, _, cx| this.toggle_notes_marks("header", !header, cx))
                .into(),
            MenuItem::tr("_shell.date-time")
                .checked(date)
                .on(move |this, _, cx| this.toggle_notes_marks("date", !date, cx))
                .into(),
            MenuItem::tr("pptx.notes-marks-panel.update-automatically")
                .checked(auto_date)
                .on(move |this, _, cx| this.toggle_notes_marks("auto", !auto_date, cx))
                .into(),
            MenuItem::tr("_xlsx.page-number")
                .checked(number)
                .on(move |this, _, cx| this.toggle_notes_marks("number", !number, cx))
                .into(),
            MenuItem::tr("_shell.footer")
                .checked(footer)
                .on(move |this, _, cx| this.toggle_notes_marks("footer", !footer, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.header-footer")
                .on(|this, w, cx| this.open_header_footer(w, cx))
                .into(),
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|this, w, cx| this.close_notes_marks(w, cx))
                .into(),
        ]
    }

    pub(crate) fn open_notes_marks_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_text_edit(window, cx);
        let items = self.notes_marks_context_menu_entries();
        if items.is_empty() {
            return;
        }
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(items);
        let editor = cx.entity().downgrade();
        let menu = gpui_kit::component::menu::PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(s) = this.pptx_mut() {
                s.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(s) = self.pptx_mut() {
            s.slide_popup = Some(super::SlidePopup { menu, position, _dismiss: dismiss });
        }
        cx.notify();
    }

    pub(crate) fn notes_marks_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.notes_marks_panel.as_ref()?;
        let t = crate::shell::i18n::text;
        let check = |id: &'static str, label: &'static str, on: bool, which: &'static str| {
            Checkbox::new(id).label(t(label)).checked(on).on_click(cx.listener(move |this, on: &bool, _, cx| this.toggle_notes_marks(which, *on, cx)))
        };
        let body = v_flex()
            .id("pptx-notes-marks-panel")
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_notes_marks_context_menu(event.position, window, cx);
            }))
            .child(check("pptx-nm-header", "_shell.header", p.header, "header"))
            .child(crate::shell::chrome::text_field(&p.header_text).full_width())
            .child(check("pptx-nm-date", "_shell.date-time", p.date, "date"))
            .child(check("pptx-nm-auto", "pptx.notes-marks-panel.update-automatically", p.auto_date, "auto"))
            .child(crate::shell::chrome::text_field(&p.date_text).full_width().disabled(p.auto_date))
            .child(check("pptx-nm-number", "_xlsx.page-number", p.number, "number"))
            .child(check("pptx-nm-footer", "_shell.footer", p.footer, "footer"))
            .child(crate::shell::chrome::text_field(&p.footer_text).full_width())
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("pptx-nm-all", t("pptx.notes-marks-panel.apply-all"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_notes_marks(w, cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("pptx-nm-cancel", t("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_notes_marks(w, cx))));
        Some(SidebarPanel::with_footer(t("pptx.notes-marks-panel.notes-header-footer"), body.into_any_element(), panel_actions))
    }
}
