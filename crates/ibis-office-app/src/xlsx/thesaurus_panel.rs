// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > 辞書 / 校閲 > 類義語辞典: the word of the selected cell looked up in the thesaurus the Word side has
//! (`docx/thesaurus.rs`), the synonyms listed under their kinds and meanings; a synonym chosen takes the place of the
//! cell's text (one step to undo).

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};

use crate::docx::thesaurus::{ThesaurusResult, lookup};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

pub(crate) struct SheetThesaurusPanel {
    pub(crate) query: Entity<InputState>,
    pub(crate) result: Option<ThesaurusResult>,
    pub(crate) searched: String,
    pub(crate) message: String,
    _subscription: Subscription,
}

impl SlideEditor {
    /// The text of the selected cell (what it shows).
    fn selected_cell_text(&self) -> String {
        let Some((column, row)) = self.current_cell() else { return String::new() };
        let Some((_, sheet)) = self.current_sheet() else { return String::new() };
        sheet.cell(row, column).map(|c| super::sheet_view::cell_shown_text(sheet, c)).unwrap_or_default().trim().to_string()
    }

    pub(crate) fn open_sheet_thesaurus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let word = self.selected_cell_text();
        let query = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(text("docx.thesaurus.search-placeholder"));
            if !word.is_empty() {
                input.set_value(&word, window, cx);
            }
            input.focus(window, cx);
            input
        });
        let subscription = cx.subscribe_in(&query, window, |this, state, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let word = state.read(cx).value().to_string();
                this.sheet_thesaurus_search(&word, cx);
            }
        });
        let result = lookup(&word);
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
        s.panels.thesaurus = Some(SheetThesaurusPanel { query, result, searched: word, message: String::new(), _subscription: subscription });
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn sheet_thesaurus_search(&mut self, word: &str, cx: &mut Context<Self>) {
        let word = word.trim().to_string();
        if let Some(p) = self.wb_mut().panels.thesaurus.as_mut() {
            p.result = lookup(&word);
            p.searched = word;
            p.message.clear();
        }
        cx.notify();
    }

    /// A synonym chosen: it takes the place of the selected cell's text.
    pub(crate) fn sheet_thesaurus_replace(&mut self, synonym: &str, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some((column, row)) = self.current_cell() else { return };
        let slide = self.current_slide;
        let edit = CellEdit { row, column, input: CellInput::Value(CellValue::Text(synonym.to_string())) };
        let message = match self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![edit] })]) {
            Ok(()) => format("docx.thesaurus.replaced-msg", &[("word", &synonym)]),
            Err(err) => format("edit.refused", &[("error", &err)]),
        };
        if let Some(p) = self.wb_mut().panels.thesaurus.as_mut() {
            p.message = message;
        }
        cx.notify();
    }

    pub(crate) fn sheet_thesaurus_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.thesaurus.as_ref()?;
        let theme = cx.theme().clone();
        let query = p.query.clone();
        let searched = p.searched.clone();
        let message = p.message.clone();
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0().child(crate::shell::chrome::text_field(&query).full_width());
        match &p.result {
            None if searched.is_empty() => {
                body = body.child(div().text_sm().text_color(theme.muted_foreground).child(text("docx.thesaurus.empty-prompt")));
            }
            None => {
                body = body
                    .child(div().text_sm().child(format("docx.thesaurus.no-results", &[("word", &searched)])))
                    .child(div().text_sm().text_color(theme.muted_foreground).child(text("docx.thesaurus.hint")));
            }
            Some(result) => {
                for (g, group) in result.groups.iter().enumerate() {
                    body = body.child(div().pt_1().text_sm().font_weight(FontWeight::SEMIBOLD).child(SharedString::from(format!("{} — {}", group.category, group.meaning))));
                    let mut row = h_flex().flex_wrap().gap_1();
                    for (i, synonym) in group.synonyms.iter().enumerate() {
                        let word = synonym.clone();
                        row = row.child(
                            crate::shell::chrome::action_button(SharedString::from(format!("synonym-{g}-{i}")))
                                .xsmall()
                                .label(SharedString::from(synonym.clone()))
                                .on_click(cx.listener(move |this, _, _, cx| this.sheet_thesaurus_replace(&word, cx))),
                        );
                    }
                    body = body.child(row);
                }
            }
        }
        body = body.child(div().text_sm().child(SharedString::from(message)));
        Some(SidebarPanel{title:text("docx.thesaurus.title").into(),body:body.into_any_element()})
    }
}
