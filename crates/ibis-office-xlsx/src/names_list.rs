// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 名前の管理の一覧 (issue #102): what the list of defined names knows about a name without the screen: its kind,
//! the parameters of a LAMBDA, whether it is an error, the search, the filter and the sort, the checks on a name
//! typed in the dialog, and the table that 一覧を貼り付け writes into the sheet.

use super::sheet_ui::{Target, parse_reference};

/// What a name stands for. The order is the order of the filter menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NameKind {
    /// `Sheet1!$A$1:$B$3`.
    Range,
    /// A number, text, logical value or array constant.
    Constant,
    /// Any other formula.
    Formula,
    /// `LAMBDA(...)`.
    Lambda,
}

impl NameKind {
    pub(crate) const ALL: [NameKind; 4] = [NameKind::Range, NameKind::Constant, NameKind::Formula, NameKind::Lambda];

    pub(crate) fn icon(self) -> &'static str {
        match self {
            NameKind::Range => "grid_on",
            NameKind::Constant => "pin",
            NameKind::Formula => "calculate",
            NameKind::Lambda => "function",
        }
    }

    /// The id of the kind's name in the i18n tables.
    pub(crate) fn label_id(self) -> &'static str {
        match self {
            NameKind::Range => "xlsx.names.filter.kind-range",
            NameKind::Constant => "xlsx.names.filter.kind-constant",
            NameKind::Formula => "xlsx.names.filter.kind-formula",
            NameKind::Lambda => "xlsx.names.filter.kind-lambda",
        }
    }

    /// The position in [`NameKind::ALL`], and in the order of sorting by kind.
    pub(crate) fn order(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

/// One parameter of a LAMBDA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LambdaParam {
    pub name: String,
    /// Written `[name]`.
    pub optional: bool,
}

/// `_xlfn.` and `_xlpm.` are how the file spells functions and LAMBDA parameters; the screen shows neither.
pub(crate) fn plain_formula(formula: &str) -> String {
    formula.replace("_xlfn.", "").replace("_xlpm.", "")
}

/// The top-level arguments of the call `LAMBDA(...)` (up to the closing parenthesis), or `None` if `formula` is
/// not a LAMBDA call.
fn lambda_arguments(formula: &str) -> Option<Vec<String>> {
    let plain = plain_formula(formula);
    let text = plain.trim().trim_start_matches('=').trim();
    let head = text.get(..7)?;
    if !head.eq_ignore_ascii_case("LAMBDA(") {
        return None;
    }
    let body = &text[7..];
    let mut args = Vec::new();
    let (mut depth, mut quoted, mut start) = (0usize, false, 0usize);
    for (i, c) in body.char_indices() {
        match c {
            '"' => quoted = !quoted,
            _ if quoted => {}
            '(' | '{' | '[' => depth += 1,
            ')' if depth == 0 => {
                args.push(body[start..i].trim().to_string());
                return Some(args);
            }
            ')' | '}' | ']' => depth = depth.saturating_sub(1),
            ',' | ';' if depth == 0 => {
                args.push(body[start..i].trim().to_string());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    // The closing parenthesis is missing: what there is still shows.
    args.push(body[start..].trim().to_string());
    Some(args)
}

/// The parameters and the body of a LAMBDA: `LAMBDA(金額, [税率], 金額*(1+税率))` has 金額, [税率] and `金額*(1+税率)`.
pub(crate) fn lambda_parts(formula: &str) -> Option<(Vec<LambdaParam>, String)> {
    let mut args = lambda_arguments(formula)?;
    let body = args.pop().unwrap_or_default();
    let params = args
        .into_iter()
        .map(|a| match a.strip_prefix('[').and_then(|a| a.strip_suffix(']')) {
            Some(inner) => LambdaParam { name: inner.trim().to_string(), optional: true },
            None => LambdaParam { name: a, optional: false },
        })
        .collect();
    Some((params, body))
}

/// A LAMBDA as the list shows it: `(金額, [税率]) → 金額*(1+税率)`.
pub(crate) fn lambda_summary(formula: &str) -> Option<String> {
    let (params, body) = lambda_parts(formula)?;
    let names: Vec<String> = params.iter().map(|p| if p.optional { format!("[{}]", p.name) } else { p.name.clone() }).collect();
    Some(format!("({}) → {}", names.join(", "), body))
}

/// A number, a quoted text, TRUE/FALSE or an array constant.
fn is_constant(formula: &str) -> bool {
    let t = formula.trim().trim_start_matches('=').trim();
    if t.is_empty() {
        return false;
    }
    if t.parse::<f64>().is_ok() {
        return true;
    }
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        return true;
    }
    if t.len() >= 2 && t.starts_with('{') && t.ends_with('}') {
        return true;
    }
    t.eq_ignore_ascii_case("TRUE") || t.eq_ignore_ascii_case("FALSE")
}

/// The kind of a name, and whether it is an error (a `#REF!` in it, or a range on a sheet the book no longer has).
pub(crate) fn classify(formula: &str, sheet_names: &[String]) -> (NameKind, bool) {
    let plain = plain_formula(formula);
    let text = plain.trim().trim_start_matches('=').trim();
    let ref_error = text.to_ascii_uppercase().contains("#REF!");
    if lambda_arguments(text).is_some() {
        return (NameKind::Lambda, ref_error);
    }
    if is_constant(text) {
        return (NameKind::Constant, false);
    }
    if let Some(reference) = parse_reference(text)
        && let Target::Cells(_) = reference.target
    {
        let missing = reference.sheet.as_ref().is_some_and(|s| !sheet_names.iter().any(|n| n.eq_ignore_ascii_case(s)));
        return (NameKind::Range, ref_error || missing);
    }
    (NameKind::Formula, ref_error)
}

/// A name for the book's own helpers: `_` or `¥` (`\`) first. Excel hides none of these, but a book full of LAMBDA
/// helpers has hundreds, and the list can leave them out.
pub(crate) fn is_helper(name: &str) -> bool {
    name.starts_with('_') || name.starts_with('\\') || name.starts_with('¥')
}

/// One name of the list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NameRow {
    /// The position in the workbook's names (the order they were defined in).
    pub index: usize,
    pub name: String,
    pub sheet: Option<usize>,
    /// Without `=`.
    pub formula: String,
    pub kind: NameKind,
    pub error: bool,
    /// What the list shows in the column 参照範囲または数式: the formula, or a LAMBDA's parameters and body.
    pub summary: String,
    /// The result of the name, as text.
    pub value: String,
    /// The scope's name: the sheet's name, or empty for the whole book.
    pub scope: String,
}

impl NameRow {
    /// The key that tells this name from another of the same name in another scope.
    pub(crate) fn key(&self) -> (String, Option<usize>) {
        (self.name.to_lowercase(), self.sheet)
    }
}

/// Which scope the list shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ScopeFilter {
    #[default]
    All,
    Workbook,
    Sheet(usize),
}

/// The filter menu's conditions, all of which must hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NameFilter {
    pub scope: ScopeFilter,
    /// The kinds shown; none chosen shows all.
    pub kinds: Vec<NameKind>,
    pub(crate) errors_only: bool,
    pub(crate) hide_helpers: bool,
}

impl NameFilter {
    /// How many conditions are on (the number on the filter button).
    pub(crate) fn active(&self) -> usize {
        usize::from(self.scope != ScopeFilter::All) + usize::from(!self.kinds.is_empty()) + usize::from(self.errors_only) + usize::from(self.hide_helpers)
    }

    pub(crate) fn matches(&self, row: &NameRow) -> bool {
        let scope = match self.scope {
            ScopeFilter::All => true,
            ScopeFilter::Workbook => row.sheet.is_none(),
            ScopeFilter::Sheet(i) => row.sheet == Some(i),
        };
        scope && (self.kinds.is_empty() || self.kinds.contains(&row.kind)) && (!self.errors_only || row.error) && (!self.hide_helpers || !is_helper(&row.name))
    }
}

/// The columns the list can be sorted by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SortColumn {
    Kind,
    Name,
    Reference,
    Value,
    Scope,
}

/// 昇順 → 降順 → 元の順 on each press of a heading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Sort {
    pub column: Option<(SortColumn, bool)>,
}

impl Sort {
    /// The sort after the heading of `column` is pressed.
    pub(crate) fn pressed(self, column: SortColumn) -> Sort {
        Sort {
            column: match self.column {
                Some((c, true)) if c == column => Some((column, false)),
                Some((c, false)) if c == column => None,
                _ => Some((column, true)),
            },
        }
    }

    /// `Some(true)`: ascending.
    pub(crate) fn direction(self, column: SortColumn) -> Option<bool> {
        self.column.filter(|(c, _)| *c == column).map(|(_, ascending)| ascending)
    }
}

/// Whether `row` contains `needle` (case-insensitive) in its name, formula or value.
pub(crate) fn matches_search(row: &NameRow, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    needle.is_empty() || row.name.to_lowercase().contains(&needle) || row.formula.to_lowercase().contains(&needle) || row.summary.to_lowercase().contains(&needle) || row.scope.to_lowercase().contains(&needle)
}

/// The rows that pass the search and the filter, in the order the sort asks (the order of definition when none):
/// the default is the name, ascending.
pub(crate) fn shown_rows(rows: &[NameRow], needle: &str, filter: &NameFilter, sort: Sort) -> Vec<NameRow> {
    let mut out: Vec<NameRow> = rows.iter().filter(|r| filter.matches(r) && matches_search(r, needle)).cloned().collect();
    match sort.column {
        None => {}
        Some((column, ascending)) => {
            out.sort_by(|a, b| {
                let order = match column {
                    SortColumn::Kind => (a.error, a.kind.order()).cmp(&(b.error, b.kind.order())),
                    SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortColumn::Reference => a.summary.to_lowercase().cmp(&b.summary.to_lowercase()),
                    SortColumn::Value => a.value.to_lowercase().cmp(&b.value.to_lowercase()),
                    SortColumn::Scope => a.scope.to_lowercase().cmp(&b.scope.to_lowercase()),
                };
                let order = order.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then(a.index.cmp(&b.index));
                if ascending { order } else { order.reverse() }
            });
        }
    }
    out
}

/// What is wrong with a name typed in the dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NameProblem {
    Empty,
    /// `A1`, `R1C1`, `R`, `C` and the like.
    CellReference,
    FirstCharacter,
    Characters,
    /// The book has the name already.
    DuplicateBook,
    /// The sheet has the name already.
    DuplicateSheet(usize),
    /// The reference or formula is empty.
    ReferenceEmpty,
}

/// Why `name` cannot be used, if it cannot. `taken` tells whether another name of the same scope has it.
pub(crate) fn name_problem(name: &str, sheet: Option<usize>, taken: bool) -> Option<NameProblem> {
    if name.is_empty() {
        return Some(NameProblem::Empty);
    }
    let first = name.chars().next()?;
    if !(first.is_alphabetic() || matches!(first, '_' | '\\' | '¥')) {
        return Some(NameProblem::FirstCharacter);
    }
    if name.chars().count() > 255 || !name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?' | '¥')) {
        return Some(NameProblem::Characters);
    }
    if !super::names::valid_name(&name.replace('¥', "\\")) {
        return Some(NameProblem::CellReference);
    }
    if taken {
        return Some(match sheet {
            Some(i) => NameProblem::DuplicateSheet(i),
            None => NameProblem::DuplicateBook,
        });
    }
    None
}

/// The table that 一覧を貼り付け writes: a heading row, then a row per name (name, reference or formula, scope, value).
/// The reference or formula starts with `=`; it is written as text, not as a formula.
pub(crate) fn paste_table(rows: &[NameRow], headings: [String; 4], book_scope: &str) -> Vec<[String; 4]> {
    let mut out = vec![headings];
    for r in rows {
        let scope = if r.scope.is_empty() { book_scope.to_string() } else { r.scope.clone() };
        out.push([r.name.clone(), format!("={}", r.formula), scope, r.value.clone()]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheets() -> Vec<String> {
        vec!["Sheet1".to_string(), "売上".to_string()]
    }

    fn row(index: usize, name: &str, sheet: Option<usize>, formula: &str) -> NameRow {
        let (kind, error) = classify(formula, &sheets());
        NameRow { index, name: name.into(), sheet, formula: formula.into(), kind, error, summary: lambda_summary(formula).unwrap_or_else(|| formula.into()), value: String::new(), scope: sheet.map(|i| sheets()[i].clone()).unwrap_or_default() }
    }

    #[test]
    fn kinds() {
        assert_eq!(classify("Sheet1!$A$1:$B$3", &sheets()), (NameKind::Range, false));
        assert_eq!(classify("'売上'!$E$2:$E$9", &sheets()), (NameKind::Range, false));
        assert_eq!(classify("Sheet9!$A$1", &sheets()), (NameKind::Range, true));
        assert_eq!(classify("Sheet1!#REF!", &sheets()), (NameKind::Formula, true));
        assert_eq!(classify("0.1", &sheets()), (NameKind::Constant, false));
        assert_eq!(classify("\"税込\"", &sheets()), (NameKind::Constant, false));
        assert_eq!(classify("{1,2;3,4}", &sheets()), (NameKind::Constant, false));
        assert_eq!(classify("TRUE", &sheets()), (NameKind::Constant, false));
        assert_eq!(classify("SUM(Sheet1!$A$1:$A$3)", &sheets()), (NameKind::Formula, false));
        assert_eq!(classify("LAMBDA(x, x*2)", &sheets()), (NameKind::Lambda, false));
        assert_eq!(classify("_xlfn.LAMBDA(_xlpm.x, _xlpm.x*2)", &sheets()), (NameKind::Lambda, false));
        assert_eq!(classify("LAMBDA(x, Sheet1!#REF!)", &sheets()), (NameKind::Lambda, true));
    }

    #[test]
    fn lambda() {
        let (params, body) = lambda_parts("LAMBDA(金額, [税率], 金額*(1+税率))").unwrap();
        assert_eq!(params, vec![LambdaParam { name: "金額".into(), optional: false }, LambdaParam { name: "税率".into(), optional: true }]);
        assert_eq!(body, "金額*(1+税率)");
        assert_eq!(lambda_summary("LAMBDA(金額, [税率], 金額*(1+税率))").as_deref(), Some("(金額, [税率]) → 金額*(1+税率)"));
        // Commas inside the body's own calls and texts do not split.
        let (params, body) = lambda_parts("LAMBDA(a, b, IF(a>b, \"x,y\", SUM(a, b)))").unwrap();
        assert_eq!(params.len(), 2);
        assert_eq!(body, "IF(a>b, \"x,y\", SUM(a, b))");
        // No parameters.
        assert_eq!(lambda_summary("LAMBDA(42)").as_deref(), Some("() → 42"));
        assert!(lambda_parts("SUM(1,2)").is_none());
    }

    #[test]
    fn filter_search_sort() {
        let rows = vec![
            row(0, "税率", None, "0.1"),
            row(1, "売上金額", None, "Sheet1!$E$2:$E$9"),
            row(2, "_Helper", None, "LAMBDA(x, x)"),
            row(3, "対象月", Some(0), "Sheet1!$B$1"),
            row(4, "対象月", Some(1), "売上!$B$1"),
            row(5, "壊れた", None, "Sheet1!#REF!"),
        ];
        let all = shown_rows(&rows, "", &NameFilter::default(), Sort::default());
        assert_eq!(all.len(), 6);
        let by_name = shown_rows(&rows, "", &NameFilter::default(), Sort::default().pressed(SortColumn::Name));
        assert_eq!(by_name.first().map(|r| r.name.as_str()), Some("_Helper"));
        let desc = shown_rows(&rows, "", &NameFilter::default(), Sort::default().pressed(SortColumn::Name).pressed(SortColumn::Name));
        assert_eq!(desc.first().map(|r| r.name.as_str()), Some("税率"));
        assert_eq!(Sort::default().pressed(SortColumn::Name).pressed(SortColumn::Name).pressed(SortColumn::Name), Sort::default());
        // The search looks at the formula too, ignoring case.
        assert_eq!(shown_rows(&rows, "sheet1!$e", &NameFilter::default(), Sort::default()).len(), 1);
        let scope = NameFilter { scope: ScopeFilter::Sheet(1), ..Default::default() };
        assert_eq!(shown_rows(&rows, "", &scope, Sort::default()).len(), 1);
        let book = NameFilter { scope: ScopeFilter::Workbook, ..Default::default() };
        assert_eq!(shown_rows(&rows, "", &book, Sort::default()).len(), 4);
        let kinds = NameFilter { kinds: vec![NameKind::Lambda, NameKind::Constant], ..Default::default() };
        assert_eq!(shown_rows(&rows, "", &kinds, Sort::default()).len(), 2);
        let errors = NameFilter { errors_only: true, ..Default::default() };
        assert_eq!(shown_rows(&rows, "", &errors, Sort::default()).iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["壊れた"]);
        let helpers = NameFilter { hide_helpers: true, ..Default::default() };
        assert_eq!(shown_rows(&rows, "", &helpers, Sort::default()).len(), 5);
        assert_eq!(NameFilter { scope: ScopeFilter::Workbook, kinds: vec![NameKind::Range], errors_only: true, hide_helpers: true }.active(), 4);
    }

    #[test]
    fn problems() {
        assert_eq!(name_problem("", None, false), Some(NameProblem::Empty));
        assert_eq!(name_problem("A1", None, false), Some(NameProblem::CellReference));
        assert_eq!(name_problem("R1C1", None, false), Some(NameProblem::CellReference));
        assert_eq!(name_problem("1st", None, false), Some(NameProblem::FirstCharacter));
        assert_eq!(name_problem("my name", None, false), Some(NameProblem::Characters));
        assert_eq!(name_problem("売上", None, true), Some(NameProblem::DuplicateBook));
        assert_eq!(name_problem("売上", Some(2), true), Some(NameProblem::DuplicateSheet(2)));
        assert_eq!(name_problem("売上", None, false), None);
        assert_eq!(name_problem("_helper.1", None, false), None);
    }

    #[test]
    fn paste() {
        let rows = vec![row(0, "税率", None, "0.1"), row(1, "対象月", Some(0), "Sheet1!$B$1")];
        let table = paste_table(&rows, ["名前".into(), "参照".into(), "スコープ".into(), "値".into()], "ブック");
        assert_eq!(table.len(), 3);
        assert_eq!(table[1], ["税率".to_string(), "=0.1".into(), "ブック".into(), String::new()]);
        assert_eq!(table[2][2], "Sheet1");
    }
}
