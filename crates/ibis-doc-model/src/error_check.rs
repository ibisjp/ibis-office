// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! エラー チェック: the cells Excel marks with a green triangle at their
//! top left, and the errors a person tells it to ignore
//! (`ignoredErrors`, ECMA-376 Part 1 §18.3.1.51).
//!
//! The rules checked: 数値が文字列として保存されている (a text constant that
//! reads as a number), 数式にエラーがある (a formula's result is an error
//! value) and 不整合な数式 (a formula unlike the ones above and below it,
//! or left and right of it, which agree with each other).

use crate::sheet::{CellRange, CellValue, Sheet, SheetCell};

/// A kind of error a cell can be marked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// 数値が文字列として保存されています (`numberStoredAsText`).
    NumberAsText,
    /// この数式の結果はエラーです (`evalError`).
    EvalError,
    /// 不整合な数式 (`formula`).
    InconsistentFormula,
    /// 2 桁の年で表された文字列形式の日付 (`twoDigitTextYear`).
    TwoDigitYear,
    /// ロックされていないセルを含む数式 (`unlockedFormula`).
    UnlockedFormula,
    /// 空のセルを参照する数式 (`emptyCellReference`).
    EmptyCellReference,
    /// 隣接するセルを含まない数式 (`formulaRange`).
    FormulaRange,
    /// テーブルの不整合な計算列 (`calculatedColumn`).
    CalculatedColumn,
    /// データの入力規則に違反する値 (`listDataValidation` / `dataValidation`).
    ListDataValidation,
}

/// Which rules are checked (Excel's エラー チェック settings).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    /// Check for errors in the background (off: no cell is marked).
    pub background: bool,
    pub number_as_text: bool,
    pub eval_error: bool,
    pub inconsistent_formula: bool,
    pub two_digit_year: bool,
    pub unlocked_formula: bool,
    pub empty_cell_reference: bool,
    pub formula_range: bool,
    pub calculated_column: bool,
    pub list_data_validation: bool,
}

impl Default for Rules {
    /// Excel's: all on but the formulas that refer to empty cells.
    fn default() -> Self {
        Self {
            background: true,
            number_as_text: true,
            eval_error: true,
            inconsistent_formula: true,
            two_digit_year: true,
            unlocked_formula: true,
            empty_cell_reference: false,
            formula_range: true,
            calculated_column: true,
            list_data_validation: true,
        }
    }
}

impl Rules {
    fn checks(&self, kind: ErrorKind) -> bool {
        self.background
            && match kind {
                ErrorKind::NumberAsText => self.number_as_text,
                ErrorKind::EvalError => self.eval_error,
                ErrorKind::InconsistentFormula => self.inconsistent_formula,
                ErrorKind::TwoDigitYear => self.two_digit_year,
                ErrorKind::UnlockedFormula => self.unlocked_formula,
                ErrorKind::EmptyCellReference => self.empty_cell_reference,
                ErrorKind::FormulaRange => self.formula_range,
                ErrorKind::CalculatedColumn => self.calculated_column,
                ErrorKind::ListDataValidation => self.list_data_validation,
            }
    }
}

static RULES: std::sync::Mutex<Option<Rules>> = std::sync::Mutex::new(None);

/// The rules in force.
pub fn rules() -> Rules {
    RULES.lock().unwrap_or_else(|e| e.into_inner()).unwrap_or_default()
}

/// Set the rules in force (the app's settings).
pub fn set_rules(rules: Rules) {
    *RULES.lock().unwrap_or_else(|e| e.into_inner()) = Some(rules);
}

impl ErrorKind {
    /// The attribute of `ignoredError` that ignores it.
    pub fn attribute(self) -> &'static str {
        match self {
            Self::NumberAsText => "numberStoredAsText",
            Self::EvalError => "evalError",
            Self::InconsistentFormula => "formula",
            Self::TwoDigitYear => "twoDigitTextYear",
            Self::UnlockedFormula => "unlockedFormula",
            Self::EmptyCellReference => "emptyCellReference",
            Self::FormulaRange => "formulaRange",
            Self::CalculatedColumn => "calculatedColumn",
            Self::ListDataValidation => "listDataValidation",
        }
    }

    pub fn from_attribute(name: &str) -> Option<Self> {
        if name == "dataValidation" {
            return Some(Self::ListDataValidation);
        }
        Self::ALL.into_iter().find(|k| k.attribute() == name)
    }

    pub const ALL: [Self; 9] = [
        Self::NumberAsText,
        Self::EvalError,
        Self::InconsistentFormula,
        Self::TwoDigitYear,
        Self::UnlockedFormula,
        Self::EmptyCellReference,
        Self::FormulaRange,
        Self::CalculatedColumn,
        Self::ListDataValidation,
    ];
}

/// Errors ignored in cells (`ignoredError`): the ranges, and the kinds
/// (attributes that are 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IgnoredError {
    pub ranges: Vec<CellRange>,
    /// The attributes set, such as `numberStoredAsText`; kinds this program
    /// does not check (`formulaRange`, `unlockedFormula`, …) are kept too.
    pub attributes: Vec<String>,
}

impl IgnoredError {
    fn covers(&self, row: u32, column: u32) -> bool {
        self.ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column))
    }
}

fn ignored(sheet: &Sheet, row: u32, column: u32, kind: ErrorKind) -> bool {
    sheet.ignored_errors.iter().any(|i| i.covers(row, column) && i.attributes.iter().any(|a| a == kind.attribute()))
}

/// Whether the text of a text constant reads as a number (not a date or a
/// time), as typing it would: it is stored as text, which Excel flags.
fn reads_as_number(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    match ibis_calc::input::parse_input(t, false) {
        ibis_calc::input::Typed::Value(CellValue::Number(_), format) => !format.is_some_and(ibis_calc::numfmt::is_date_format),
        _ => false,
    }
}

/// The formula of the cell at (row, column), as it would be in (to_row, to_column).
fn formula_moved(sheet: &Sheet, row: u32, column: u32, to_row: u32, to_column: u32) -> Option<String> {
    let f = sheet.cell(row, column)?.formula.as_deref()?;
    Some(ibis_calc::formula::shift(f, to_row as i64 - row as i64, to_column as i64 - column as i64))
}

fn inconsistent(sheet: &Sheet, cell: &SheetCell) -> bool {
    let (row, column) = (cell.row, cell.column);
    let Some(own) = cell.formula.as_deref() else { return false };
    let pairs = [(row.checked_sub(1).map(|r| (r, column)), Some((row + 1, column))), (column.checked_sub(1).map(|c| (row, c)), Some((row, column + 1)))];
    pairs.into_iter().any(|(before, after)| {
        let (Some((br, bc)), Some((ar, ac))) = (before, after) else { return false };
        match (formula_moved(sheet, br, bc, row, column), formula_moved(sheet, ar, ac, row, column)) {
            (Some(a), Some(b)) => a == b && a != own,
            _ => false,
        }
    })
}

/// The errors of the cell at (row, column) that are not ignored.
pub fn errors_at(sheet: &Sheet, row: u32, column: u32) -> Vec<ErrorKind> {
    let Some(cell) = sheet.cell(row, column) else { return Vec::new() };
    errors_of(sheet, cell)
}

/// Whether a text constant is a date written with a 2-digit year (`1/2/26`).
fn two_digit_year(text: &str) -> bool {
    let t = text.trim();
    let parts: Vec<&str> = t.split(['/', '-', '.']).collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) && parts[2].len() == 2 && parts[0].len() <= 2 && parts[1].len() <= 2 && parts[0].parse::<u32>().is_ok_and(|v| (1..=31).contains(&v)) && parts[1].parse::<u32>().is_ok_and(|v| (1..=31).contains(&v))
}

/// The cell references (same sheet) a formula has.
fn references(formula: &str) -> Vec<ibis_calc::formula::AreaRef> {
    let mut out = Vec::new();
    ibis_calc::formula::rewrite_references(formula, |a| {
        if a.sheet.is_none() && !a.whole_columns && !a.whole_rows {
            out.push(a.clone());
        }
        Some(a.clone())
    });
    out
}

fn is_number(sheet: &Sheet, row: Option<u32>, column: Option<u32>, own: (u32, u32)) -> bool {
    let (Some(r), Some(c)) = (row, column) else { return false };
    (r, c) != own && sheet.cell(r, c).is_some_and(|x| matches!(x.value, CellValue::Number(_)) && x.formula.is_none())
}

/// A formula's range left out the numbers next to it (`=SUM(B2:B5)` with a number in B6).
fn omits_adjacent(sheet: &Sheet, cell: &SheetCell) -> bool {
    let Some(formula) = cell.formula.as_deref() else { return false };
    let own = (cell.row, cell.column);
    references(formula).into_iter().filter(|a| !a.is_cell()).any(|a| {
        let (r0, r1, c0, c1) = (a.start.row.min(a.end.row), a.start.row.max(a.end.row), a.start.column.min(a.end.column), a.start.column.max(a.end.column));
        if c0 == c1 {
            is_number(sheet, r0.checked_sub(1), Some(c0), own) || is_number(sheet, Some(r1 + 1), Some(c0), own)
        } else if r0 == r1 {
            is_number(sheet, Some(r0), c0.checked_sub(1), own) || is_number(sheet, Some(r0), Some(c1 + 1), own)
        } else {
            false
        }
    })
}

/// A formula with a single-cell reference to an empty cell.
fn refers_to_empty(sheet: &Sheet, cell: &SheetCell) -> bool {
    let Some(formula) = cell.formula.as_deref() else { return false };
    references(formula).into_iter().filter(|a| a.is_cell()).any(|a| sheet.cell(a.start.row, a.start.column).is_none_or(|c| matches!(c.value, CellValue::Empty) && c.formula.is_none()))
}

/// `A1` or `A1:B3` as (first row, first column, last row, last column).
fn parse_validation_area(text: &str) -> Option<(u32, u32, u32, u32)> {
    let one = |t: &str| -> Option<(u32, u32)> {
        let split = t.find(|c: char| c.is_ascii_digit())?;
        let column = ibis_calc::formula::parser::column_index(&t[..split])?;
        let row: u32 = t[split..].parse().ok()?;
        Some((row.checked_sub(1)?, column))
    };
    let (a, b) = text.split_once(':').unwrap_or((text, text));
    let (r0, c0) = one(a)?;
    let (r1, c1) = one(b)?;
    Some((r0.min(r1), c0.min(c1), r0.max(r1), c0.max(c1)))
}

fn validation_list_items(sheet: &Sheet, rule: &crate::sheet::Validation) -> Vec<String> {
    let Some(f) = rule.formula1.as_deref() else { return Vec::new() };
    let f = f.trim();
    if let Some(inner) = f.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        return inner.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    }
    let area = f.rsplit('!').next().unwrap_or(f).replace('$', "");
    let Some(r) = parse_validation_area(&area) else { return Vec::new() };
    let mut out = Vec::new();
    for row in r.0..=r.2 {
        for column in r.1..=r.3 {
            if let Some(c) = sheet.cell(row, column) && !c.text.is_empty() {
                out.push(c.text.clone());
            }
        }
    }
    out
}

fn validation_keeps(sheet: &Sheet, rule: &crate::sheet::Validation, value: &CellValue) -> bool {
    if value.is_empty() {
        return rule.allow_blank || rule.kind == "none";
    }
    let bound = |f: &Option<String>| -> Option<f64> {
        let f = f.as_deref()?;
        f.trim().parse::<f64>().ok().or_else(|| {
            let area = f.rsplit('!').next().unwrap_or(f).replace('$', "");
            let (r, c, _, _) = parse_validation_area(&area)?;
            match sheet.cell(r, c).map(|x| &x.value) {
                Some(CellValue::Number(n)) => Some(*n),
                _ => None,
            }
        })
    };
    let compare = |v: f64| -> bool {
        let (a, b) = (bound(&rule.formula1), bound(&rule.formula2));
        match (rule.operator.as_str(), a) {
            ("between", Some(a)) => b.is_some_and(|b| v >= a.min(b) && v <= a.max(b)),
            ("notBetween", Some(a)) => b.is_some_and(|b| v < a.min(b) || v > a.max(b)),
            ("equal", Some(a)) => v == a,
            ("notEqual", Some(a)) => v != a,
            ("greaterThan", Some(a)) => v > a,
            ("lessThan", Some(a)) => v < a,
            ("greaterThanOrEqual", Some(a)) => v >= a,
            ("lessThanOrEqual", Some(a)) => v <= a,
            _ => true,
        }
    };
    match rule.kind.as_str() {
        "list" => {
            let text = match value {
                CellValue::Number(n) => ibis_calc::eval::number_to_text(*n),
                CellValue::Text(t) => t.clone(),
                CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
                CellValue::Error(e) => e.as_str().to_string(),
                CellValue::Empty => String::new(),
            };
            { let items = validation_list_items(sheet, rule); let half = ibis_calc::functions::to_half_width(&text); items.iter().any(|i| i.eq_ignore_ascii_case(&text) || i.eq_ignore_ascii_case(&half)) }
        }
        "whole" => matches!(value, CellValue::Number(n) if n.fract() == 0.0 && compare(*n)),
        "decimal" | "date" | "time" => matches!(value, CellValue::Number(n) if compare(*n)),
        "textLength" => {
            let len = match value {
                CellValue::Text(t) => t.chars().count(),
                CellValue::Number(n) => ibis_calc::eval::number_to_text(*n).chars().count(),
                _ => 0,
            };
            compare(len as f64)
        }
        _ => true,
    }
}

fn violates_validation(sheet: &Sheet, cell: &SheetCell) -> bool {
    let (row, column) = (cell.row, cell.column);
    let Some(rule) = sheet.validations.iter().find(|v| v.ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column))) else {
        return false;
    };
    !validation_keeps(sheet, rule, &cell.value)
}

/// Whether `cell` is inside a table and its formula (or lack of one) is
/// inconsistent with the calculated column formula established by other rows.
fn inconsistent_calculated_column(sheet: &Sheet, cell: &SheetCell) -> bool {
    let (row, column) = (cell.row, cell.column);
    let Some(table) = sheet.tables.iter().find(|t| {
        let first_row = t.range.first_row + t.header_rows;
        let last_row = t.range.last_row.saturating_sub(t.totals_rows);
        first_row <= row && row <= last_row && t.range.first_column <= column && column <= t.range.last_column
    }) else {
        return false;
    };

    let start_row = table.range.first_row + table.header_rows;
    let end_row = table.range.last_row.saturating_sub(table.totals_rows);
    if end_row <= start_row {
        return false;
    }

    // Inspect formulas in the same column across all data rows of this table
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in start_row..=end_row {
        if let Some(f) = sheet.cell(r, column).and_then(|c| c.formula.as_deref()) {
            let norm = ibis_calc::formula::shift(f, start_row as i64 - r as i64, 0);
            *counts.entry(norm).or_insert(0) += 1;
        }
    }

    let Some((majority_formula, &count)) = counts.iter().max_by_key(|e| *e.1) else {
        return false;
    };

    let total_data_rows = (end_row - start_row + 1) as usize;
    // Established calculated column pattern: at least 2 rows, or majority of data rows
    if (count >= 2 || (total_data_rows == 2 && count == 1)) && (count * 2 >= total_data_rows || count >= 2) {
        let own_norm = cell.formula.as_deref().map(|f| ibis_calc::formula::shift(f, start_row as i64 - row as i64, 0));
        return own_norm.as_ref() != Some(majority_formula);
    }

    false
}

/// Whether the cell holds a value of a dynamic array formula's spill (not the formula's own cell): like any formula result,
/// Excel marks no error on it (not "number stored as text", nor the others of the constants), and an action on it
/// (数値に変換) would write over the spill (`#SPILL!`).
pub fn spilled_into(sheet: &Sheet, row: u32, column: u32) -> bool {
    sheet.spills.iter().any(|(&anchor, r)| anchor != (row, column) && r.first_row <= row && row <= r.last_row && r.first_column <= column && column <= r.last_column)
}

fn errors_of(sheet: &Sheet, cell: &SheetCell) -> Vec<ErrorKind> {
    let mut out = Vec::new();
    let rules = rules();
    if !rules.background || spilled_into(sheet, cell.row, cell.column) {
        return out;
    }
    let mut add = |kind: ErrorKind, found: bool| {
        if found && rules.checks(kind) && !ignored(sheet, cell.row, cell.column, kind) {
            out.push(kind);
        }
    };
    add(ErrorKind::NumberAsText, cell.formula.is_none() && matches!(&cell.value, CellValue::Text(t) if reads_as_number(t)));
    add(ErrorKind::EvalError, cell.formula.is_some() && matches!(cell.value, CellValue::Error(_)));
    let is_calc_col = inconsistent_calculated_column(sheet, cell);
    add(ErrorKind::CalculatedColumn, is_calc_col);
    add(ErrorKind::InconsistentFormula, !is_calc_col && inconsistent(sheet, cell));
    add(ErrorKind::TwoDigitYear, cell.formula.is_none() && matches!(&cell.value, CellValue::Text(t) if two_digit_year(t)));
    add(ErrorKind::UnlockedFormula, cell.formula.is_some() && sheet.styles.get(cell.style as usize).is_some_and(|s| s.unlocked));
    add(ErrorKind::EmptyCellReference, refers_to_empty(sheet, cell));
    add(ErrorKind::FormulaRange, omits_adjacent(sheet, cell));
    add(ErrorKind::ListDataValidation, violates_validation(sheet, cell));
    out
}

/// The cells among `cells` (of `sheet`) with an error that is not ignored,
/// with their first error, in order.
pub fn flagged_among(sheet: &Sheet, cells: &[SheetCell]) -> Vec<(u32, u32, ErrorKind)> {
    cells.iter().filter_map(|c| errors_of(sheet, c).first().map(|&k| (c.row, c.column, k))).collect()
}

/// Every cell with an error that is not ignored, with its first error, in order.
pub fn flagged_cells(sheet: &Sheet) -> Vec<(u32, u32, ErrorKind)> {
    sheet.cells.iter().filter_map(|c| errors_of(sheet, c).first().map(|&k| (c.row, c.column, k))).collect()
}

/// The message Excel shows for an error (in Japanese; the app has its own).
pub fn message(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::TwoDigitYear => "このセルの日付は 2 桁の年で表されています",
        ErrorKind::UnlockedFormula => "この数式を含むセルはロックされていません",
        ErrorKind::EmptyCellReference => "この数式は空のセルを参照しています",
        ErrorKind::FormulaRange => "この数式は、隣接するセルを含んでいません",
        ErrorKind::NumberAsText => "このセルの数値は、文字列形式で保存されています",
        ErrorKind::EvalError => "このセルの数式の結果はエラーです",
        ErrorKind::InconsistentFormula => "この数式は、周囲の数式と異なっています",
        ErrorKind::CalculatedColumn => "この数式は、この列の他の数式と矛盾しています",
        ErrorKind::ListDataValidation => "このセルの値は、データの入力規則に違反しています",
    }
}

impl Sheet {
    /// Ignore errors of `kind` in `range` from now on.
    pub fn ignore_errors(&mut self, range: CellRange, kind: ErrorKind) {
        let attribute = kind.attribute().to_string();
        match self.ignored_errors.iter_mut().find(|i| i.ranges == [range]) {
            Some(i) => {
                if !i.attributes.contains(&attribute) {
                    i.attributes.push(attribute);
                }
            }
            None => self.ignored_errors.push(IgnoredError { ranges: vec![range], attributes: vec![attribute] }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::{CellKind, SheetCell};

    /// The rules are the process's: the tests that set them take turns.
    static GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn cell(row: u32, column: u32, value: CellValue, formula: Option<&str>) -> SheetCell {
        let text = match &value {
            CellValue::Text(t) => t.clone(),
            CellValue::Number(n) => n.to_string(),
            _ => String::new(),
        };
        SheetCell { runs: None, phonetic: None, row, column, value, text, kind: CellKind::Text, formula: formula.map(str::to_string), style: 0 }
    }

    fn sheet(cells: Vec<SheetCell>) -> Sheet {
        let mut s = Sheet::default();
        for c in cells {
            s.put_cell(c.row, c.column, Some(c));
        }
        s
    }

    #[test]
    fn numbers_stored_as_text_errors_and_inconsistent_formulas_are_marked() {
        let _turn = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        set_rules(Rules::default());
        let mut s = sheet(vec![
            cell(0, 0, CellValue::Text("123".into()), None),
            cell(1, 0, CellValue::Text("abc".into()), None),
            cell(2, 0, CellValue::Text("2026/1/5".into()), None),
            cell(3, 0, CellValue::Number(5.0), None),
            cell(0, 1, CellValue::Number(1.0), Some("A1+1")),
            cell(1, 1, CellValue::Number(2.0), Some("A2+1")),
            cell(2, 1, CellValue::Number(9.0), Some("A3*2")),
            cell(3, 1, CellValue::Number(4.0), Some("A4+1")),
            cell(0, 2, CellValue::Error(crate::sheet::CellError::Div0), Some("1/0")),
        ]);
        let flagged = flagged_cells(&s);
        assert_eq!(flagged, vec![(0, 0, ErrorKind::NumberAsText), (0, 2, ErrorKind::EvalError), (2, 1, ErrorKind::InconsistentFormula)]);
        // Ignored in a range: gone there.
        s.ignore_errors(CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 2 }, ErrorKind::NumberAsText);
        s.ignore_errors(CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 2 }, ErrorKind::EvalError);
        assert_eq!(flagged_cells(&s), vec![(2, 1, ErrorKind::InconsistentFormula)]);
        assert_eq!(s.ignored_errors.len(), 1, "one entry with both kinds");
    }

    #[test]
    fn the_other_rules_and_the_settings() {
        let _turn = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let mut locked = Sheet::default();
        let plain = locked.style_index(crate::sheet::CellStyle::default());
        let open = locked.style_index(crate::sheet::CellStyle { unlocked: true, ..Default::default() });
        let mut cells = vec![
            cell(0, 0, CellValue::Text("1/2/26".into()), None),
            cell(1, 0, CellValue::Text("1/2/2026".into()), None),
            cell(0, 2, CellValue::Number(3.0), None),
            cell(1, 2, CellValue::Number(4.0), None),
            cell(2, 2, CellValue::Number(5.0), None),
            cell(3, 2, CellValue::Number(6.0), None),
            cell(0, 3, CellValue::Number(7.0), Some("SUM(C1:C3)")),
            cell(1, 3, CellValue::Number(7.0), Some("SUM(C2:C4)")),
            cell(2, 3, CellValue::Number(1.0), Some("Z9+1")),
            cell(3, 3, CellValue::Number(1.0), Some("C1+1")),
        ];
        cells[9].style = open;
        for c in cells.drain(..) {
            locked.put_cell(c.row, c.column, Some(c));
        }
        let _ = plain;
        set_rules(Rules::default());
        let flagged = flagged_cells(&locked);
        // A1 is a text date with a 2-digit year; D1 omits C4 and D2 omits C1 beside their ranges; D4 is unlocked.
        assert_eq!(flagged, vec![(0, 0, ErrorKind::TwoDigitYear), (0, 3, ErrorKind::FormulaRange), (1, 3, ErrorKind::FormulaRange), (3, 3, ErrorKind::UnlockedFormula)]);
        // A reference to an empty cell is checked only when it is turned on.
        set_rules(Rules { empty_cell_reference: true, ..Rules::default() });
        assert!(flagged_cells(&locked).contains(&(2, 3, ErrorKind::EmptyCellReference)));
        // Turning the background check off marks nothing.
        set_rules(Rules { background: false, ..Rules::default() });
        assert!(flagged_cells(&locked).is_empty());
        set_rules(Rules::default());
    }

    #[test]
    fn calculated_column_and_data_validation_errors_are_marked() {
        use crate::sheet::{SheetTable, Validation};
        let _turn = GUARD.lock().unwrap_or_else(|e| e.into_inner());
        set_rules(Rules::default());

        let mut s = sheet(vec![
            // Table headers and data:
            // Table at rows 0..=3, columns 1..=2 (B1:C4). Header row = 0. Data rows = 1..=3.
            cell(0, 1, CellValue::Text("Price".into()), None),
            cell(0, 2, CellValue::Text("Total".into()), None),
            cell(1, 1, CellValue::Number(10.0), None),
            cell(1, 2, CellValue::Number(20.0), Some("B2*2")),
            cell(2, 1, CellValue::Number(20.0), None),
            // C3 has a constant value instead of the calculated column formula B3*2:
            cell(2, 2, CellValue::Number(99.0), None),
            cell(3, 1, CellValue::Number(30.0), None),
            cell(3, 2, CellValue::Number(60.0), Some("B4*2")),
            // Cell outside table with invalid data validation:
            cell(0, 0, CellValue::Text("Grape".into()), None),
            cell(4, 0, CellValue::Number(15.0), None),
        ]);

        s.tables.push(SheetTable {
            name: "Table1".into(),
            range: CellRange { first_row: 0, last_row: 3, first_column: 1, last_column: 2 },
            header_rows: 1,
            totals_rows: 0,
            columns: vec!["Price".into(), "Total".into()],
            style: None,
            filter: None,
            totals: Vec::new(),
        });

        // List validation on A1
        s.validations.push(Validation {
            ranges: vec![CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 }],
            kind: "list".into(),
            operator: "between".into(),
            formula1: Some(r#""Apple,Banana,Orange""#.into()),
            formula2: None,
            allow_blank: true,
            dropdown: true,
            prompt: None,
            error: None,
        });

        // Whole number validation on A5 (1 <= x <= 10)
        s.validations.push(Validation {
            ranges: vec![CellRange { first_row: 4, last_row: 4, first_column: 0, last_column: 0 }],
            kind: "whole".into(),
            operator: "between".into(),
            formula1: Some("1".into()),
            formula2: Some("10".into()),
            allow_blank: true,
            dropdown: false,
            prompt: None,
            error: None,
        });

        let flagged = flagged_cells(&s);
        assert!(flagged.contains(&(0, 0, ErrorKind::ListDataValidation)));
        assert!(flagged.contains(&(4, 0, ErrorKind::ListDataValidation)));
        assert!(flagged.contains(&(2, 2, ErrorKind::CalculatedColumn)));

        // Test ignoring calculatedColumn error
        s.ignore_errors(CellRange { first_row: 2, last_row: 2, first_column: 2, last_column: 2 }, ErrorKind::CalculatedColumn);
        assert!(!flagged_cells(&s).contains(&(2, 2, ErrorKind::CalculatedColumn)));

        // Test turning off rules
        set_rules(Rules { calculated_column: false, list_data_validation: false, ..Rules::default() });
        let filtered = flagged_cells(&s);
        assert!(!filtered.iter().any(|(_, _, k)| matches!(k, ErrorKind::CalculatedColumn | ErrorKind::ListDataValidation)));
        set_rules(Rules::default());
    }
}
