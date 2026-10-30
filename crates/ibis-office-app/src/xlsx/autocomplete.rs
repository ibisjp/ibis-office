// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 数式オートコンプリート: while a formula is typed, the functions, names
//! and tables whose names start with the word before the caret, listed
//! under the cell (↑ ↓ to choose, Tab to put it in, Esc to close), as
//! Excel lists them; inside a function's parentheses, how many arguments
//! it takes.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::v_flex;
use gpui_kit::*;

use crate::shell::editor::SlideEditor;

/// Something the list offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) text: String,
    pub(crate) kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Function,
    Name,
    Table,
}

/// The list's state: the item chosen, and the text it was closed (Esc) for.
#[derive(Default)]
pub(crate) struct Completion {
    pub(crate) index: usize,
    pub(crate) prefix: String,
    pub(crate) dismissed: Option<String>,
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '\\')
}

/// The word of a formula (`=…`) the caret (a byte offset) is at the end of,
/// and where it starts; `None` outside a formula, inside a string, after a
/// sheet's or table's name, or when the word is not a name's start.
pub(crate) fn word_at(text: &str, caret: usize) -> Option<(usize, &str)> {
    if !super::formula_reference::is_formula(text) || caret > text.len() || !text.is_char_boundary(caret) {
        return None;
    }
    let before = &text[..caret];
    if before.chars().filter(|&c| c == '"').count() % 2 == 1 {
        return None;
    }
    let start = before.char_indices().rev().take_while(|(_, c)| word_char(*c)).last().map_or(caret, |(i, _)| i);
    let word = &text[start..caret];
    let first = word.chars().next()?;
    if !(first.is_alphabetic() || first == '_' || first == '\\') {
        return None;
    }
    // A cell's reference (A1, AB12) is not completed.
    let letters = word.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    if letters > 0 && letters <= 3 && word.len() > letters && word[letters..].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let prev = text[..start].chars().next_back();
    if matches!(prev, Some('!' | ']' | '[' | '\'' | '$')) {
        return None;
    }
    // Not when the next character goes on with the word.
    if text[caret..].chars().next().is_some_and(word_char) {
        return None;
    }
    Some((start, word))
}

/// The functions (Excel's list), names and tables starting with `prefix`
/// (without regard to case), names and tables first, each in order.
pub(crate) fn candidates(prefix: &str, names: &[String], tables: &[String]) -> Vec<Candidate> {
    let p = prefix.to_uppercase();
    let starts = |s: &str| s.to_uppercase().starts_with(&p);
    let mut out: Vec<Candidate> = Vec::new();
    let mut named: Vec<Candidate> = names.iter().filter(|n| starts(n) && !n.starts_with("_xlnm")).map(|n| Candidate { text: n.clone(), kind: Kind::Name }).collect();
    named.extend(tables.iter().filter(|t| starts(t)).map(|t| Candidate { text: t.clone(), kind: Kind::Table }));
    named.sort_by(|a, b| a.text.to_uppercase().cmp(&b.text.to_uppercase()));
    named.dedup_by(|a, b| a.text.eq_ignore_ascii_case(&b.text));
    let mut functions: Vec<Candidate> = ibis_calc::functions::table::FUNCTIONS.iter().filter(|f| f.name.starts_with(&p)).map(|f| Candidate { text: f.name.to_string(), kind: Kind::Function }).collect();
    functions.sort_by(|a, b| a.text.cmp(&b.text));
    out.extend(named);
    out.extend(functions);
    out
}

/// The function whose parentheses the caret (a byte offset) is in, and
/// which of its arguments (from 0) it is at.
pub(crate) fn call_at(text: &str, caret: usize) -> Option<(String, usize)> {
    if !super::formula_reference::is_formula(text) || caret > text.len() {
        return None;
    }
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut in_string = false;
    let mut word = String::new();
    for c in text[..caret].chars() {
        if in_string {
            if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                word.clear();
            }
            '(' => {
                stack.push((std::mem::take(&mut word).to_uppercase(), 0));
            }
            ')' => {
                stack.pop();
                word.clear();
            }
            ',' => {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
                word.clear();
            }
            c if word_char(c) => word.push(c),
            _ => word.clear(),
        }
    }
    let (name, arg) = stack.pop()?;
    let name = name.trim_start_matches("_XLFN.").to_string();
    ibis_calc::functions::table::FUNCTIONS.iter().any(|f| f.name == name).then_some((name, arg))
}

impl SlideEditor {
    fn completion_names(&self) -> (Vec<String>, Vec<String>) {
        let names = self.presentation().workbook.as_ref().map(|w| w.names.iter().map(|n| n.name.clone()).collect()).unwrap_or_default();
        let tables = self.presentation().slides.iter().filter_map(ibis_doc_model::sheet_calc::sheet_of_slide).flat_map(|s| s.tables.iter().map(|t| t.name.clone())).collect();
        (names, tables)
    }

    /// The list as it is now: the word's byte span and what it offers.
    pub(crate) fn completion_list(&self) -> Option<(usize, usize, Vec<Candidate>)> {
        let edit = self.cell_edit().filter(|e| e.slide == self.current_slide && e.marked.is_none())?;
        let caret = edit.caret_byte();
        let (start, word) = word_at(&edit.text, caret)?;
        if self.sheet_state().and_then(|s| s.panels.completion.dismissed.as_deref()) == Some(edit.text.as_str()) {
            return None;
        }
        let (names, tables) = self.completion_names();
        let list = candidates(word, &names, &tables);
        (!list.is_empty()).then_some((start, caret, list))
    }

    fn completion_index(&self, len: usize) -> usize {
        let Some(s) = self.sheet_state() else { return 0 };
        let word = self.cell_edit().and_then(|e| word_at(&e.text, e.caret_byte()).map(|(_, w)| w.to_string())).unwrap_or_default();
        if s.panels.completion.prefix == word { s.panels.completion.index.min(len.saturating_sub(1)) } else { 0 }
    }

    /// ↑ ↓ while the list shows: the item before or after.
    pub(crate) fn completion_move(&mut self, dy: i32, cx: &mut Context<Self>) -> bool {
        let Some((_, _, list)) = self.completion_list() else { return false };
        let i = self.completion_index(list.len()) as i32;
        let next = (i + dy).clamp(0, list.len() as i32 - 1) as usize;
        let word = self.cell_edit().and_then(|e| word_at(&e.text, e.caret_byte()).map(|(_, w)| w.to_string())).unwrap_or_default();
        let c = &mut self.wb_mut().panels.completion;
        c.index = next;
        c.prefix = word;
        cx.notify();
        true
    }

    /// Tab while the list shows: the item chosen put in place of the word
    /// (a function with its opening parenthesis).
    pub(crate) fn completion_accept(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, end, list)) = self.completion_list() else { return false };
        let i = self.completion_index(list.len());
        let chosen = &list[i];
        let text = if chosen.kind == Kind::Function { format!("{}(", chosen.text) } else { chosen.text.clone() };
        self.replace_cell_text(start, end, &text, cx);
        self.wb_mut().panels.completion = Completion::default();
        cx.notify();
        true
    }

    /// Esc while the list shows: the list closed (the entry goes on).
    pub(crate) fn completion_dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.completion_list().is_none() {
            return false;
        }
        let text = self.cell_edit().map(|e| e.text.clone());
        self.wb_mut().panels.completion.dismissed = text;
        cx.notify();
        true
    }

    /// The list, or the function's hint, under the cell being typed into.
    pub(crate) fn render_completion(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let edit = self.cell_edit().filter(|e| e.slide == self.current_slide)?;
        let rect = edit.rect.get()?;
        let theme = cx.theme().clone();
        let at = point(rect.origin.x, rect.origin.y + rect.size.height + px(2.));
        let body = if let Some((_, _, list)) = self.completion_list() {
            let index = self.completion_index(list.len());
            let first = index.saturating_sub(9);
            let mut column = v_flex();
            for (i, c) in list.iter().enumerate().skip(first).take(10) {
                let kind = match c.kind {
                    Kind::Function => "fx",
                    Kind::Name => "_shell.name",
                    Kind::Table => "docx.common.table",
                };
                column = column.child(
                    h_flex_row(i == index, &theme)
                        .child(div().w(px(32.)).text_color(theme.muted_foreground).child(crate::shell::i18n::text(kind)))
                        .child(SharedString::from(c.text.clone())),
                );
            }
            column.into_any_element()
        } else {
            let (name, arg) = call_at(&edit.text, edit.caret_byte())?;
            let f = ibis_calc::functions::table::FUNCTIONS.iter().find(|f| f.name == name)?;
            let count = match (f.min_args, f.max_args) {
                (a, b) if a == b => crate::shell::i18n::format("_xlsx.n-arguments", &[("n", &a)]),
                (a, 255) => crate::shell::i18n::format("_xlsx.min-or-more-arguments", &[("min", &a)]),
                (a, b) => crate::shell::i18n::format("_xlsx.min-to-max-arguments", &[("min", &a), ("max", &b)]),
            };
            let here = crate::shell::i18n::format("_xlsx.argument-n", &[("n", &(arg + 1))]);
            div().px_2().py_1().child(SharedString::from(crate::shell::i18n::format("_xlsx.name-count-here", &[("name", &name), ("count", &count), ("here", &here)]))).into_any_element()
        };
        let panel = div().min_w(px(220.)).bg(theme.background).border_1().border_color(theme.border).rounded_md().shadow_lg().text_size(px(12.)).child(body);
        Some(deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(panel)).with_priority(2).into_any_element())
    }
}

fn h_flex_row(selected: bool, theme: &gpui_kit::component::Theme) -> Div {
    let row = div().flex().flex_row().gap_2().px_2().py(px(2.));
    if selected { row.bg(theme.accent) } else { row }
}

#[cfg(test)]
mod tests {
    use super::{Kind, call_at, candidates, word_at};

    #[test]
    fn the_word_before_the_caret() {
        let t = "=SU";
        assert_eq!(word_at(t, t.len()), Some((1, "SU")));
        let t = "=A1+vlo";
        assert_eq!(word_at(t, t.len()), Some((4, "vlo")));
        assert_eq!(word_at("=A1", 3), None, "a cell's reference");
        assert_eq!(word_at("=\"SU", 4), None, "in a string");
        assert_eq!(word_at("SU", 2), None, "not a formula");
        assert_eq!(word_at("=Sheet1!SU", 10), None, "after a sheet's name");
        assert_eq!(word_at("=1", 2), None);
    }

    #[test]
    fn functions_names_and_tables() {
        let list = candidates("su", &["Sum_area".into()], &["Sales".into()]);
        assert_eq!(list[0].kind, Kind::Name);
        assert!(list.iter().any(|c| c.text == "SUM" && c.kind == Kind::Function));
        assert!(list.iter().any(|c| c.text == "SUMIFS"));
        assert!(!list.iter().any(|c| c.text == "Sales"));
        assert!(candidates("zzz", &[], &[]).is_empty());
    }

    #[test]
    fn the_call_the_caret_is_in() {
        let t = "=SUM(A1,IF(B1>0,";
        assert_eq!(call_at(t, t.len()), Some(("IF".into(), 1)));
        let t = "=SUM(A1,IF(B1>0,1,2),";
        assert_eq!(call_at(t, t.len()), Some(("SUM".into(), 2)));
        assert_eq!(call_at("=A1+", 4), None);
        assert_eq!(call_at("=SUM(\"(,\"", 9), Some(("SUM".into(), 0)));
    }
}
