// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! メモ (Excel's notes, the comments of older versions): 挿入 > 新しいメモ
//! (or 編集 for a cell with one) opens the sidebar's panel with the
//! selected cell's note to write or change, by the user (the system's
//! name); メモの削除 and コメントとメモのクリア take them off the selected
//! cells. Shown by the cell's red mark and its card (comment_card.rs).
//! Threaded comments are another feature (the xlsx owner's).
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, SheetCommand, SheetComment};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct NotePanel {
    /// The cell (column, row).
    pub(crate) cell: (u32, u32),
    pub(crate) state: Entity<TextareaState>,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// Open the note of the selected cell (a new one if it has none).
    pub(crate) fn open_note_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let (Some((column, row)), Some((_, sheet))) = (self.current_cell(), self.current_sheet()) else { return };
        let note = sheet.comment_at(row, column).map(|c| c.text.clone()).unwrap_or_default();
        let state = cx.new(|cx| TextareaState::new(window, cx).placeholder(text("fix.notes.type-note")).default_value(note));
        state.update(cx, |s, cx| s.focus(window, cx));
        let panel = NotePanel { cell: (column, row), state, ui: Default::default() };
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.note = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Keep the panel's note on its cell (an empty one takes it off).
    pub(crate) fn save_note(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.note.as_ref()) else { return };
        let ((column, row), body) = (p.cell, p.state.read(cx).value().to_string());
        let slide = self.current_slide;
        let author = self.current_sheet().and_then(|(_, s)| s.comment_at(row, column).map(|c| c.author.clone())).filter(|a| !a.is_empty()).unwrap_or_else(|| crate::shell::user_name::user_name().to_string());
        let comment = (!body.trim().is_empty()).then(|| SheetComment { row, column, author, text: body });
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetComment { slide, row, column, comment })]) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
        }
        self.wb_mut().panels.note = None;
        cx.notify();
    }

    /// メモの削除 / コメントとメモのクリア: the notes of the selected cells
    /// (and of the ranges selected with them) taken off, one step.
    pub(crate) fn delete_notes(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let commands: Vec<Command> = sheet
            .comments
            .iter()
            .filter(|c| ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)))
            .map(|c| Command::Sheet(SheetCommand::SetComment { slide, row: c.row, column: c.column, comment: None }))
            .collect();
        if commands.is_empty() {
            return;
        }
        if let Err(err) = self.document.execute(commands) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
        }
        if let Some(p) = self.wb().panels.note.as_ref() {
            let (c, r) = p.cell;
            if ranges.iter().any(|rng| (rng.first_row..=rng.last_row).contains(&r) && (rng.first_column..=rng.last_column).contains(&c)) {
                self.wb_mut().panels.note = None;
            }
        }
        cx.notify();
    }

    pub(crate) fn note_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.note.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let (column, row) = p.cell;
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(div().text_sm().child(crate::shell::i18n::format("note.cell", &[("cell", &ibis_doc_model::cell_name(column, row))])))
            .child(mark("text", div().h(px(140.)).border_1().border_color(rgb(0xD0D0D0)).rounded_md().p_1().child(Textarea::new(&p.state).appearance(false).size_full()).into_any_element()))
            ;
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("save", crate::shell::chrome::ActionDefinition::new("note-save", "", crate::shell::chrome::ActionButtonKind::Primary)
                    .label(text("_shell.save"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.save_note(cx);
                        this.focus_handle.focus(window, cx);
                    })))).action(mark_action("delete", crate::shell::chrome::ActionDefinition::new("note-delete", "", crate::shell::chrome::ActionButtonKind::Destructive)
                    .label(text("fix.notes.delete-note"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let slide = this.current_slide;
                        if let Err(err) = this.document.execute(vec![Command::Sheet(SheetCommand::SetComment { slide, row, column, comment: None })]) {
                            log::warn!("deleting the note rejected: {err}");
                        }
                        this.wb_mut().panels.note = None;
                        this.focus_handle.focus(window, cx);
                        cx.notify();
                    }))));
        Some(SidebarPanel::with_footer("pptx.notes.note", body.into_any_element(), panel_terminal_actions))
    }
}
