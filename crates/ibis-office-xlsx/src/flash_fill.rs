// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > フラッシュ フィル (⌃E): the rest of a column filled after the
//! way its first cells were typed from the cells beside them, as Excel's
//! Flash Fill: each example is read as pieces, each piece either a part of
//! one of the row's cells (a run of letters, of digits or of other
//! characters, counted from the start or from the end; as it is, in
//! capitals, in small letters or with a capital first) or text of its own;
//! the simplest way that makes every example is used for the rows below.

use crate::ext::*;

/// A part of a cell: its runs of the same kind of character.
fn tokens(text: &str) -> Vec<String> {
    #[derive(PartialEq, Clone, Copy)]
    enum Kind {
        Letter,
        Digit,
        Space,
        Other,
    }
    let kind = |c: char| {
        if c.is_ascii_digit() || c.is_numeric() && !c.is_alphabetic() {
            Kind::Digit
        } else if c.is_alphabetic() {
            Kind::Letter
        } else if c.is_whitespace() {
            Kind::Space
        } else {
            Kind::Other
        }
    };
    let mut out: Vec<String> = Vec::new();
    let mut last: Option<Kind> = None;
    for c in text.chars() {
        let k = kind(c);
        // Other characters each a piece (`-`, `@`, `.`); the others in runs.
        if last == Some(k) && k != Kind::Other {
            out.last_mut().expect("a piece").push(c);
        } else {
            out.push(c.to_string());
        }
        last = Some(k);
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Transform {
    AsIs,
    Upper,
    Lower,
    Proper,
    TrimZeros,
    PadZero2,
    PadZero3,
    PadZero4,
    ThousandsComma,
    MonthShort,
    MonthFull,
    MonthKanji,
    DayKanji,
    YearKanji,
}

impl Transform {
    fn cost(&self) -> usize {
        match self {
            Transform::AsIs => 0,
            Transform::Upper | Transform::Lower | Transform::Proper => 1,
            Transform::TrimZeros | Transform::PadZero2 | Transform::PadZero3 | Transform::PadZero4 => 1,
            Transform::ThousandsComma => 1,
            Transform::MonthShort | Transform::MonthFull | Transform::MonthKanji | Transform::DayKanji | Transform::YearKanji => 2,
        }
    }
}

fn apply_transform(text: &str, transform: Transform) -> Option<String> {
    match transform {
        Transform::AsIs => Some(text.to_string()),
        Transform::Upper => Some(text.to_uppercase()),
        Transform::Lower => Some(text.to_lowercase()),
        Transform::Proper => {
            let mut chars = text.chars();
            Some(match chars.next() {
                Some(first) => first.to_uppercase().chain(chars.flat_map(|c| c.to_lowercase())).collect(),
                None => String::new(),
            })
        }
        Transform::TrimZeros => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let trimmed = text.trim_start_matches('0');
            Some(if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() })
        }
        Transform::PadZero2 => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            Some(if text.len() < 2 { format!("0{text}") } else { text.to_string() })
        }
        Transform::PadZero3 => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            Some(if text.len() < 3 { format!("{:0>3}", text) } else { text.to_string() })
        }
        Transform::PadZero4 => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            Some(if text.len() < 4 { format!("{:0>4}", text) } else { text.to_string() })
        }
        Transform::ThousandsComma => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) || text.len() <= 3 {
                return None;
            }
            let mut result = String::new();
            let len = text.len();
            for (i, c) in text.chars().enumerate() {
                if i > 0 && (len - i) % 3 == 0 {
                    result.push(',');
                }
                result.push(c);
            }
            Some(result)
        }
        Transform::MonthShort => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let n: u32 = text.parse().ok()?;
            const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            MONTHS.get(n.checked_sub(1)? as usize).map(|&s| s.to_string())
        }
        Transform::MonthFull => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let n: u32 = text.parse().ok()?;
            const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
            MONTHS.get(n.checked_sub(1)? as usize).map(|&s| s.to_string())
        }
        Transform::MonthKanji => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let n: u32 = text.parse().ok()?;
            if (1..=12).contains(&n) {
                Some(format!("{n}月")) // i18n-ok（入力例の書式に合わせて作る日本語の値）
            } else {
                None
            }
        }
        Transform::DayKanji => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let n: u32 = text.parse().ok()?;
            if (1..=31).contains(&n) {
                Some(format!("{n}日")) // i18n-ok（入力例の書式に合わせて作る日本語の値）
            } else {
                None
            }
        }
        Transform::YearKanji => {
            if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let n: u32 = text.parse().ok()?;
            if (1900..=2100).contains(&n) {
                Some(format!("{n}年")) // i18n-ok（入力例の書式に合わせて作る日本語の値）
            } else {
                None
            }
        }
    }
}

/// One piece of an answer.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Piece {
    /// The `index`th part of cell `column` (from the end if `from_end`).
    Part { column: usize, index: usize, from_end: bool, transform: Transform },
    /// The whole cell.
    Cell { column: usize, transform: Transform },
    /// The first character of the `index`th part (an initial).
    Initial { column: usize, index: usize, from_end: bool, transform: Transform },
    /// A substring slice of the `index`th part: character range `start..(start + len)`.
    Slice { column: usize, index: usize, from_end: bool, start: usize, len: usize, transform: Transform },
    Text(String),
}

impl Piece {
    fn eval(&self, cells: &[Vec<String>], whole: &[String]) -> Option<String> {
        match self {
            Piece::Text(t) => Some(t.clone()),
            Piece::Cell { column, transform } => apply_transform(whole.get(*column)?, *transform),
            Piece::Initial { column, index, from_end, transform } => {
                let parts = cells.get(*column)?;
                let i = if *from_end { parts.len().checked_sub(index + 1)? } else { *index };
                let initial = parts.get(i)?.chars().next()?.to_string();
                apply_transform(&initial, *transform)
            }
            Piece::Part { column, index, from_end, transform } => {
                let parts = cells.get(*column)?;
                let i = if *from_end { parts.len().checked_sub(index + 1)? } else { *index };
                apply_transform(parts.get(i)?, *transform)
            }
            Piece::Slice { column, index, from_end, start, len, transform } => {
                let parts = cells.get(*column)?;
                let i = if *from_end { parts.len().checked_sub(index + 1)? } else { *index };
                let text = parts.get(i)?;
                let chars: Vec<char> = text.chars().collect();
                let s = if *from_end {
                    chars.len().checked_sub(start + len)?
                } else {
                    *start
                };
                if s + len > chars.len() {
                    return None;
                }
                let s_str: String = chars[s..s + len].iter().collect();
                apply_transform(&s_str, *transform)
            }
        }
    }

    /// Lower is simpler: a whole cell, then parts, then text.
    fn cost(&self) -> usize {
        match self {
            Piece::Cell { transform, .. } => 1 + transform.cost(),
            Piece::Part { from_end, transform, .. } => 2 + usize::from(*from_end) + transform.cost(),
            Piece::Initial { from_end, transform, .. } => 3 + usize::from(*from_end) + transform.cost(),
            Piece::Slice { from_end, transform, len, .. } => 3 + usize::from(*from_end) + transform.cost() + usize::from(*len < 2),
            // Letters and digits of its own are rarely meant: they come from the cells.
            Piece::Text(t) => 2 + t.chars().count() + 10 * t.chars().filter(|c| c.is_alphanumeric()).count(),
        }
    }
}

/// The ways (up to a limit) the pieces of one row make `output`.
fn programs(cells: &[Vec<String>], whole: &[String], output: &str) -> Vec<Vec<Piece>> {
    let chars: Vec<char> = output.chars().collect();
    let n = chars.len();
    // Candidate pieces: what each makes.
    let mut candidates: Vec<(Piece, String)> = Vec::new();

    const GENERAL_TRANSFORMS: [Transform; 4] = [Transform::AsIs, Transform::Upper, Transform::Lower, Transform::Proper];
    const NUMBER_TRANSFORMS: [Transform; 10] = [
        Transform::TrimZeros, Transform::PadZero2, Transform::PadZero3, Transform::PadZero4,
        Transform::ThousandsComma, Transform::MonthShort, Transform::MonthFull,
        Transform::MonthKanji, Transform::DayKanji, Transform::YearKanji,
    ];

    for (column, text) in whole.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        for transform in GENERAL_TRANSFORMS {
            if let Some(made) = apply_transform(text, transform)
                && !made.is_empty()
            {
                candidates.push((Piece::Cell { column, transform }, made));
            }
        }
        if text.chars().all(|c| c.is_ascii_digit()) {
            for transform in NUMBER_TRANSFORMS {
                if let Some(made) = apply_transform(text, transform)
                    && !made.is_empty()
                    && made != *text
                {
                    candidates.push((Piece::Cell { column, transform }, made));
                }
            }
        }
    }

    for (column, parts) in cells.iter().enumerate() {
        for index in 0..parts.len() {
            let part_text = &parts[index];
            let is_all_digits = !part_text.is_empty() && part_text.chars().all(|c| c.is_ascii_digit());
            for from_end in [false, true] {
                for transform in GENERAL_TRANSFORMS {
                    for piece in [Piece::Part { column, index, from_end, transform }, Piece::Initial { column, index, from_end, transform }] {
                        if let Some(made) = piece.eval(cells, whole)
                            && !made.is_empty()
                        {
                            candidates.push((piece, made));
                        }
                    }
                }
                if is_all_digits {
                    for transform in NUMBER_TRANSFORMS {
                        let piece = Piece::Part { column, index, from_end, transform };
                        if let Some(made) = piece.eval(cells, whole)
                            && !made.is_empty()
                            && made != *part_text
                        {
                            candidates.push((piece, made));
                        }
                    }
                }
            }

            let part_len = part_text.chars().count();
            if part_len >= 3 && part_len <= 20 {
                for len in 1..part_len {
                    for start in 0..=(part_len - len) {
                        for from_end in [false, true] {
                            let piece = Piece::Slice { column, index, from_end, start, len, transform: Transform::AsIs };
                            if let Some(made) = piece.eval(cells, whole)
                                && !made.is_empty()
                            {
                                candidates.push((piece, made));
                            }
                        }
                    }
                }
            }
        }
    }

    // The cheapest ways to make each end of the output (a few each).
    const KEEP: usize = 32;
    let mut best: Vec<Vec<(usize, Vec<Piece>)>> = vec![Vec::new(); n + 1];
    best[0].push((0, Vec::new()));
    for at in 0..n {
        if best[at].is_empty() {
            continue;
        }
        let here = best[at].clone();
        let rest: String = chars[at..].iter().collect();
        let mut steps: Vec<(Piece, usize)> = candidates.iter().filter(|(_, made)| rest.starts_with(made.as_str())).map(|(p, made)| (p.clone(), made.chars().count())).collect();
        // Text of its own: up to the next character a part could start.
        for len in 1..=(n - at).min(8) {
            steps.push((Piece::Text(chars[at..at + len].iter().collect()), len));
        }
        for (piece, len) in steps {
            for (cost, list) in &here {
                let mut list = list.clone();
                let cost = cost + piece.cost();
                // Text after text joins it.
                if let (Piece::Text(t), Some(Piece::Text(last))) = (&piece, list.last_mut()) {
                    last.push_str(t);
                } else {
                    list.push(piece.clone());
                }
                let slot = &mut best[at + len];
                if !slot.iter().any(|(_, l)| *l == list) {
                    slot.push((cost, list));
                    slot.sort_by_key(|(c, _)| *c);
                    slot.truncate(KEEP);
                }
            }
        }
    }
    best[n].iter().map(|(_, l)| l.clone()).collect()
}

/// The answers for `rows` (each row's cells beside the column), learnt
/// from `examples` (the first rows' cells and what was typed for them).
/// `None` when no one way makes every example.
pub(crate) fn flash_fill(examples: &[(Vec<String>, String)], rows: &[Vec<String>]) -> Option<Vec<String>> {
    let (first, rest) = examples.split_first()?;
    let split = |row: &[String]| row.iter().map(|c| tokens(c)).collect::<Vec<_>>();
    let candidates = programs(&split(&first.0), &first.0, &first.1);
    let works = |program: &Vec<Piece>, row: &[String], out: &str| -> bool {
        let cells = split(row);
        program.iter().map(|p| p.eval(&cells, row)).collect::<Option<String>>().as_deref() == Some(out)
    };
    let program = candidates.into_iter().find(|program| {
        // A piece of text only would not use the row at all.
        program.iter().any(|p| !matches!(p, Piece::Text(_))) && rest.iter().all(|(row, out)| works(program, row, out))
    })?;
    Some(
        rows.iter()
            .map(|row| {
                let cells = split(row);
                program.iter().map(|p| p.eval(&cells, row)).collect::<Option<String>>().unwrap_or_default()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::flash_fill;

    fn row(cells: &[&str]) -> Vec<String> {
        cells.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn first_names_from_full_names() {
        let examples = [(row(&["Nancy Davolio"]), "Nancy".to_string())];
        let rows = [row(&["Andrew Fuller"]), row(&["Janet Leverling"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["Andrew", "Janet"]);
    }

    #[test]
    fn last_name_comma_first_initial() {
        let examples = [(row(&["Nancy", "Davolio"]), "DAVOLIO, N.".to_string())];
        let rows = [row(&["Andrew", "Fuller"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["FULLER, A."]);
    }

    #[test]
    fn user_from_email_and_joined_columns() {
        let examples = [(row(&["nancy.davolio@example.com"]), "nancy".to_string())];
        assert_eq!(flash_fill(&examples, &[row(&["a.fuller@example.com"])]).unwrap(), ["a"]);
        let examples = [(row(&["東京都", "千代田区"]), "東京都千代田区".to_string())];
        assert_eq!(flash_fill(&examples, &[row(&["大阪府", "北区"])]).unwrap(), ["大阪府北区"]);
    }

    #[test]
    fn two_examples_pick_the_way_both_fit() {
        // "03" alone could be the third digits or the last; the second
        // example says the last.
        let examples = [(row(&["2026-10-03"]), "03".to_string()), (row(&["2025-1-15"]), "15".to_string())];
        assert_eq!(flash_fill(&examples, &[row(&["2024-12-31"])]).unwrap(), ["31"]);
    }

    #[test]
    fn date_format_leading_zero_trim() {
        let examples = [(row(&["2024-05-09"]), "2024/5/9".to_string())];
        let rows = [row(&["2025-12-03"]), row(&["2026-01-08"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["2025/12/3", "2026/1/8"]);
    }

    #[test]
    fn date_format_leading_zero_pad() {
        let examples = [(row(&["2024/5/9"]), "2024-05-09".to_string())];
        let rows = [row(&["2025/11/3"]), row(&["2026/1/15"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["2025-11-03", "2026-01-15"]);
    }

    #[test]
    fn date_to_japanese_kanji() {
        let examples = [(row(&["2024-05-09"]), "2024年5月9日".to_string())];
        let rows = [row(&["2025-11-20"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["2025年11月20日"]);
    }

    #[test]
    fn date_to_english_month() {
        let examples = [(row(&["2024-05-09"]), "May 9, 2024".to_string())];
        let rows = [row(&["2025-01-15"]), row(&["2026-12-25"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["Jan 15, 2025", "Dec 25, 2026"]);
    }

    #[test]
    fn number_thousands_separator() {
        let examples = [(row(&["1234567"]), "1,234,567".to_string())];
        let rows = [row(&["987654321"]), row(&["1000"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["987,654,321", "1,000"]);
    }

    #[test]
    fn telephone_split_with_slices() {
        let examples = [(row(&["09012345678"]), "090-1234-5678".to_string())];
        let rows = [row(&["08098765432"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["080-9876-5432"]);
    }

    #[test]
    fn postal_code_split_with_slices() {
        let examples = [(row(&["1234567"]), "123-4567".to_string())];
        let rows = [row(&["9870001"])];
        assert_eq!(flash_fill(&examples, &rows).unwrap(), ["987-0001"]);
    }
}

#[ibis_office_macros::editor_ext(FlashFill)]
impl crate::shell::editor::SlideEditor {
    /// Compute inline Flash Fill ghost predictions `(row, predicted_text)` for `column` starting at `row`
    /// while the user is typing `typed` into an empty cell `(row, column)`.
    pub(crate) fn compute_flash_fill_preview(sheet: &ibis_doc_model::Sheet, column: u32, row: u32, typed: &str) -> Vec<(u32, String)> {
        use ibis_doc_model::CellValue;
        if typed.is_empty() || typed.starts_with('=') || typed.starts_with('\'') || typed.contains('\n') {
            return Vec::new();
        }
        if sheet.cell(row, column).is_some_and(|c| !c.value.is_empty() || c.formula.is_some()) {
            return Vec::new();
        }
        let region = super::goto_special::find(
            sheet,
            super::goto_special::Special::CurrentRegion,
            super::goto_special::Types::ALL,
            super::goto_special::Scope::All,
            super::goto_special::TraceLevel::DirectOnly,
            ibis_doc_model::CellRange { first_row: row, last_row: row, first_column: column, last_column: column },
            (column, row),
            None,
        )[0];
        let sources: Vec<u32> = (region.first_column..=region.last_column).filter(|&c| c != column).collect();
        if sources.is_empty() {
            return Vec::new();
        }
        let text = |r: u32, c: u32| {
            sheet
                .cell(r, c)
                .map(|x| {
                    if !x.text.is_empty() {
                        x.text.clone()
                    } else {
                        match &x.value {
                            CellValue::Text(t) => t.clone(),
                            CellValue::Number(n) => n.to_string(),
                            CellValue::Bool(b) => {
                                if *b { "TRUE".into() } else { "FALSE".into() }
                            }
                            _ => String::new(),
                        }
                    }
                })
                .unwrap_or_default()
        };
        let source_row = |r: u32| sources.iter().map(|&c| text(r, c)).collect::<Vec<_>>();
        let rows: Vec<u32> = (region.first_row..=region.last_row)
            .filter(|&r| source_row(r).iter().any(|t| !t.is_empty()))
            .collect();
        if !rows.contains(&row) {
            return Vec::new();
        }
        let examples: Vec<(u32, Vec<String>, String)> = rows
            .iter()
            .filter(|&&r| r != row && !text(r, column).is_empty())
            .map(|&r| (r, source_row(r), text(r, column)))
            .collect();
        let targets: Vec<u32> = rows
            .iter()
            .copied()
            .filter(|&r| r == row || (r > row && text(r, column).is_empty()))
            .collect();
        if examples.is_empty() || targets.len() < 2 {
            return Vec::new();
        }
        let learn = |skip: usize| -> Option<Vec<String>> {
            let ex_rows: Vec<&(u32, Vec<String>, String)> = examples.iter().skip(skip).collect();
            if !ex_rows.iter().any(|&&(r, _, _)| r < row) {
                return None;
            }
            let ex: Vec<(Vec<String>, String)> = ex_rows.into_iter().map(|(_, s, o)| (s.clone(), o.clone())).collect();
            flash_fill(&ex, &targets.iter().map(|&r| source_row(r)).collect::<Vec<_>>())
        };
        let Some(filled) = learn(0).or_else(|| learn(1)) else {
            return Vec::new();
        };
        let Some(first) = filled.first() else {
            return Vec::new();
        };
        if !first.starts_with(typed) || first.chars().count() <= typed.chars().count() {
            return Vec::new();
        }
        targets
            .into_iter()
            .zip(filled)
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }

    /// Inline Flash Fill ghost predictions `(row, predicted_text)` for the cell currently being typed into.
    pub(crate) fn flash_fill_preview(&self) -> Vec<(u32, String)> {
        let Some(edit) = self.cell_edit().filter(|e| e.slide == self.current_slide) else {
            return Vec::new();
        };
        if edit.mode != super::cell_edit::EditMode::Enter || edit.marked.is_some() {
            return Vec::new();
        }
        let Some((_, sheet)) = self.current_sheet() else {
            return Vec::new();
        };
        Self::compute_flash_fill_preview(sheet, edit.column, edit.row, &edit.text)
    }

    /// If an inline Flash Fill ghost preview is active, accept it and fill the column in one Undo step.
    pub(crate) fn accept_flash_fill_preview(&mut self, cx: &mut gpui_kit::Context<Self>) -> bool {
        let preview = self.flash_fill_preview();
        if preview.is_empty() {
            return false;
        }
        let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.take()) else {
            return false;
        };
        use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};
        let edits: Vec<CellEdit> = preview
            .into_iter()
            .map(|(r, v)| CellEdit {
                row: r,
                column: edit.column,
                input: CellInput::Value(CellValue::Text(v)),
            })
            .collect();
        let count = edits.len();
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide: edit.slide, edits })]) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
            cx.notify();
            return true;
        }
        self.status = Some(crate::shell::i18n::format("flash-fill.done", &[("count", &count)]).into());
        self.sync_shape_formulas(cx);
        self.sync_peers(cx);
        cx.notify();
        true
    }

    /// フラッシュ フィル on the selected cell's column: learnt from the
    /// filled cells of the column in the current region, the empty ones
    /// filled (one step), the number filled told in the status bar.
    pub(crate) fn flash_fill_column(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if self.protection_refuses(crate::protection::Guarded::Cells, cx) {
            return;
        }
        use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};
        self.commit_cell_edit(cx);
        let (Some((column, row)), Some((_, sheet))) = (self.current_cell(), self.current_sheet()) else { return };
        let region = super::goto_special::find(sheet, super::goto_special::Special::CurrentRegion, super::goto_special::Types::ALL, super::goto_special::Scope::All, super::goto_special::TraceLevel::DirectOnly, ibis_doc_model::CellRange { first_row: row, last_row: row, first_column: column, last_column: column }, (column, row), None)[0];
        let sources: Vec<u32> = (region.first_column..=region.last_column).filter(|&c| c != column).collect();
        let text = |r: u32, c: u32| {
            sheet.cell(r, c).map(|x| {
                if !x.text.is_empty() {
                    x.text.clone()
                } else {
                    match &x.value {
                        CellValue::Text(t) => t.clone(),
                        CellValue::Number(n) => n.to_string(),
                        CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
                        _ => String::new(),
                    }
                }
            }).unwrap_or_default()
        };
        let source_row = |r: u32| sources.iter().map(|&c| text(r, c)).collect::<Vec<_>>();
        let rows: Vec<u32> = (region.first_row..=region.last_row).filter(|&r| source_row(r).iter().any(|t| !t.is_empty())).collect();
        let examples: Vec<(u32, Vec<String>, String)> = rows.iter().filter(|&&r| !text(r, column).is_empty()).map(|&r| (r, source_row(r), text(r, column))).collect();
        let targets: Vec<u32> = rows.iter().copied().filter(|&r| text(r, column).is_empty()).collect();
        let learn = |skip: usize| {
            let ex: Vec<(Vec<String>, String)> = examples.iter().skip(skip).map(|(_, s, o)| (s.clone(), o.clone())).collect();
            flash_fill(&ex, &targets.iter().map(|&r| source_row(r)).collect::<Vec<_>>())
        };
        // The first row may be the headings, not an example.
        let filled = if examples.is_empty() || targets.is_empty() { None } else { learn(0).or_else(|| learn(1)) };
        let Some(values) = filled else {
            self.status = Some(crate::shell::i18n::text("_xlsx.flash-fill-could-not-determine-values"));
            return cx.notify();
        };
        let edits: Vec<CellEdit> = targets.iter().zip(&values).filter(|(_, v)| !v.is_empty()).map(|(&r, v)| CellEdit { row: r, column, input: CellInput::Value(CellValue::Text(v.clone())) }).collect();
        let count = edits.len();
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })]) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
            return cx.notify();
        }
        self.status = Some(crate::shell::i18n::format("flash-fill.done", &[("count", &count)]).into());
        cx.notify();
    }
}
