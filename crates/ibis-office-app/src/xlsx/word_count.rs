// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > 文字カウント…: the number of cells with data, of words and of characters (with and without spaces) of the
//! text the cells show, for the selected range, or the whole sheet when one cell is selected.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// What the cells hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SheetStatistics {
    pub(crate) is_selection: bool,
    pub(crate) cells: usize,
    pub(crate) words: usize,
    pub(crate) characters_no_spaces: usize,
    pub(crate) characters_with_spaces: usize,
}

pub(crate) struct WordCountPanel {
    pub(crate) stats: SheetStatistics,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    /// The text of the selected range's cells, or of the sheet's when one cell is selected (what the cells show).
    pub(crate) fn sheet_statistics(&self) -> SheetStatistics {
        let Some((_, sheet)) = self.current_sheet() else { return SheetStatistics::default() };
        let range = self.selected_range().filter(|r| (r.first_row, r.first_column) != (r.last_row, r.last_column));
        let mut stats = SheetStatistics { is_selection: range.is_some(), ..Default::default() };
        for cell in sheet.cells.iter() {
            if range.is_some_and(|r| !((r.first_row..=r.last_row).contains(&cell.row) && (r.first_column..=r.last_column).contains(&cell.column))) {
                continue;
            }
            let shown = super::sheet_view::cell_shown_text(sheet, cell);
            if shown.is_empty() {
                continue;
            }
            let (words, no_spaces, with_spaces) = crate::docx::help_panels::count_words_and_chars(&shown);
            stats.cells += 1;
            stats.words += words;
            stats.characters_no_spaces += no_spaces;
            stats.characters_with_spaces += with_spaces;
        }
        stats
    }

    pub(crate) fn open_sheet_word_count(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let panel = WordCountPanel { stats: self.sheet_statistics(), ui: Default::default() };
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
        s.panels.word_count = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn sheet_word_count_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.word_count.as_ref()?;
        let stats = p.stats.clone();
        let ui = p.ui.clone();
        let theme = cx.theme().clone();
        let mark = move |id: &str, child: AnyElement| {
            let ui = ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let row = |id: &'static str, label: &'static str, value: usize| {
            let theme = theme.clone();
            mark(
                id,
                h_flex()
                    .justify_between()
                    .items_center()
                    .py(px(5.))
                    .border_b_1()
                    .border_color(theme.title_bar_border)
                    .child(div().text_sm().text_color(theme.foreground).child(text(label)))
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(crate::shell::ribbon::thousands(value)))
                    .into_any_element(),
            )
        };
        let target = if stats.is_selection { text("xlsx.word-count.selection") } else { text("xlsx.word-count.sheet") };
        let body = v_flex()
            .gap_1()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .pb(px(4.))
                    .child(div().text_sm().text_color(theme.muted_foreground).child(target))
,
            )
            .child(row("xlsx.word-count.cells", "xlsx.word-count.cells", stats.cells))
            .child(row("xlsx.word-count.words", "docx.help-panels.words", stats.words))
            .child(row("xlsx.word-count.chars-no-spaces", "docx.help-panels.characters-no-spaces", stats.characters_no_spaces))
            .child(row("xlsx.word-count.chars-with-spaces", "docx.help-panels.characters-spaces", stats.characters_with_spaces));
        Some(SidebarPanel::with_footer(text("docx.help-panels.word-count"),body.into_any_element(),crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("sheet-word-count-close", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_shell.close")).on_click(cx.listener(|e,_,w,cx|e.close_panel_dialog(w,cx))))))
    }
}
