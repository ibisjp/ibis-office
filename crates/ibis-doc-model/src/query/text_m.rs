// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The M code of a query, as the 詳細エディター shows it: each step a line of `let`, written as Excel writes it (K0039: the strings
//! are from a Microsoft 365 Excel's own 詳細エディター, `tasks/doc/requests/2026-11-16-k0039-excel-m-result.md`).
//!
//! The steps are named as Power Query names them, in the language of the screen: the caller gives the names by an id of the step
//! (`changed-type`); `english_step_name` is the English of each.
use super::{Agg, CharClass, Clause, Cond, Culture, DateFilter, DateWhen, JoinKind, Op, Query, SplitBy, Source, Step, Transform};
use std::collections::HashMap;

pub(super) fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn list(items: &[String]) -> String {
    format!("{{{}}}", items.iter().map(|s| q(s)).collect::<Vec<_>>().join(", "))
}

const KEYWORDS: [&str; 15] = ["and", "as", "each", "else", "error", "false", "if", "in", "is", "let", "meta", "not", "null", "or", "then"];

/// A name used as a step: `#"Changed Type"` unless it is a plain identifier (letters, digits and `_`, the first not a digit, and not
/// a keyword); a name with a space, even one at its end, is quoted.
fn id(name: &str) -> String {
    let plain = !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
        && !KEYWORDS.contains(&name);
    if plain { name.to_string() } else { format!("#\"{}\"", name.replace('"', "\"\"")) }
}

/// A value typed in a step, as an M literal: a number stays one.
fn lit(s: &str) -> String {
    if s.trim().parse::<f64>().is_ok() || super::cond::date_literal(s).is_some() { s.trim().to_string() } else { q(s) }
}

/// A column read in a row: `[Name]`, or `[#"Name"]` when the name has anything but letters, digits, `_` and single spaces and dots inside.
pub(super) fn col_ref(name: &str) -> String {
    let plain = !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | ' ' | '.'))
        && !name.starts_with([' ', '.'])
        && !name.ends_with([' ', '.'])
        && !name.contains("  ")
        && !name.contains("..");
    if plain { format!("[{name}]") } else { format!("[#{}]", q(name)) }
}

fn agg_m(a: Agg) -> &'static str {
    match a {
        Agg::Sum => "List.Sum",
        Agg::Average => "List.Average",
        Agg::Min => "List.Min",
        Agg::Max => "List.Max",
        Agg::Count => "List.Count",
        Agg::CountDistinct => "each List.Count(List.Distinct(_))",
        Agg::Median => "List.Median",
        Agg::First => "List.First",
    }
}

fn cond_m(column: &str, op: Op, value: &str) -> String {
    let c = col_ref(column);
    match op {
        Op::Equals => format!("{c} = {}", lit(value)),
        Op::NotEquals => format!("{c} <> {}", lit(value)),
        Op::Greater => format!("{c} > {}", lit(value)),
        Op::GreaterOrEqual => format!("{c} >= {}", lit(value)),
        Op::Less => format!("{c} < {}", lit(value)),
        Op::LessOrEqual => format!("{c} <= {}", lit(value)),
        Op::Contains => format!("Text.Contains({c}, {})", q(value)),
        Op::DoesNotContain => format!("not Text.Contains({c}, {})", q(value)),
        Op::BeginsWith => format!("Text.StartsWith({c}, {})", q(value)),
        Op::EndsWith => format!("Text.EndsWith({c}, {})", q(value)),
        Op::DoesNotBeginWith => format!("not Text.StartsWith({c}, {})", q(value)),
        Op::DoesNotEndWith => format!("not Text.EndsWith({c}, {})", q(value)),
        Op::IsNull => format!("{c} = null"),
        Op::IsNotNull => format!("{c} <> null"),
    }
}

fn date_filter_m(column: &str, f: &DateFilter) -> String {
    let c = col_ref(column);
    let unit = f.unit.m_name();
    match (f.when, f.count) {
        (DateWhen::YearToDate, _) => format!("Date.IsInYearToDate({c})"),
        (DateWhen::Current, _) => format!("Date.IsInCurrent{unit}({c})"),
        (DateWhen::Previous, None) => format!("Date.IsInPrevious{unit}({c})"),
        (DateWhen::Next, None) => format!("Date.IsInNext{unit}({c})"),
        (DateWhen::Previous, Some(n)) => format!("Date.IsInPreviousN{unit}s({c}, {n})"),
        (DateWhen::Next, Some(n)) => format!("Date.IsInNextN{unit}s({c}, {n})"),
    }
}

/// A condition as M. Brackets are put as M needs them (an `or` in an `and`, the same word in itself) and where the condition says
/// (`Cond::Group`): `and` binds before `or`, so `a and b or c` is written without any, as Excel writes the 詳細設定 of a filter.
pub(super) fn cond_text(cond: &Cond) -> String {
    let nested = |c: &Cond| format!("({})", cond_text(c));
    match cond {
        Cond::Leaf { column, op, value } => cond_m(column, *op, value),
        Cond::Part { column, part, op, value } => format!("{}({}) {} {}", part.m_name(), col_ref(column), cmp_symbol(*op), lit(value)),
        Cond::Date { column, filter } => date_filter_m(column, filter),
        Cond::And(v) => v.iter().map(|c| match c {
            Cond::And(w) | Cond::Or(w) if w.len() > 1 => nested(c),
            other => cond_text(other),
        }).collect::<Vec<_>>().join(" and "),
        Cond::Or(v) => v.iter().map(|c| match c {
            Cond::Or(w) if w.len() > 1 => nested(c),
            other => cond_text(other),
        }).collect::<Vec<_>>().join(" or "),
        Cond::Not(c) => format!("not ({})", cond_text(c)),
        Cond::Group(c) => nested(c),
    }
}

fn cmp_symbol(op: Op) -> &'static str {
    match op {
        Op::NotEquals => "<>",
        Op::Greater => ">",
        Op::GreaterOrEqual => ">=",
        Op::Less => "<",
        Op::LessOrEqual => "<=",
        _ => "=",
    }
}

/// The characters of a kind as Excel writes them in `Splitter.SplitTextByCharacterTransition`.
fn class_m(c: CharClass) -> &'static str {
    match c {
        CharClass::Lower => "{\"a\"..\"z\"}",
        CharClass::Upper => "{\"A\"..\"Z\"}",
        CharClass::Digit => "{\"0\"..\"9\"}",
        CharClass::NonDigit => "(c) => not List.Contains({\"0\"..\"9\"}, c)",
    }
}

pub(super) fn class_of(text: &str) -> Option<CharClass> {
    let compact: String = text.split_whitespace().collect();
    Some(match compact.as_str() {
        "{\"a\"..\"z\"}" => CharClass::Lower,
        "{\"A\"..\"Z\"}" => CharClass::Upper,
        "{\"0\"..\"9\"}" => CharClass::Digit,
        "(c)=>notList.Contains({\"0\"..\"9\"},c)" => CharClass::NonDigit,
        _ => return None,
    })
}

fn splitter_m(by: &SplitBy) -> String {
    let nums = |v: &[usize]| format!("{{{}}}", v.iter().map(usize::to_string).collect::<Vec<_>>().join(", "));
    let end = |from_end: bool| if from_end { ", true" } else { "" };
    match by {
        SplitBy::Delimiter(d) => format!("Splitter.SplitTextByDelimiter({}, QuoteStyle.Csv)", q(d)),
        SplitBy::Length(n) => format!("Splitter.SplitTextByRepeatedLengths({n})"),
        SplitBy::AnyDelimiter(ds) => format!("Splitter.SplitTextByAnyDelimiter({}, QuoteStyle.Csv)", list(ds)),
        SplitBy::EachDelimiter { delimiters, from_end } => format!("Splitter.SplitTextByEachDelimiter({}, QuoteStyle.Csv, {from_end})", list(delimiters)),
        SplitBy::Positions { positions, from_end } => format!("Splitter.SplitTextByPositions({}{})", nums(positions), end(*from_end)),
        SplitBy::Lengths { lengths, from_end } => format!("Splitter.SplitTextByLengths({}{})", nums(lengths), end(*from_end)),
        SplitBy::Once { length, from_end } => format!("Splitter.SplitTextByPositions({{0, {length}}}, {from_end})"),
        SplitBy::Transition { from, to } => format!("Splitter.SplitTextByCharacterTransition({}, {})", class_m(*from), class_m(*to)),
    }
}

/// The English name of a step, by its id (the names Power Query gives in English).
pub fn english_step_name(id: &str) -> &'static str {
    match id {
        "source" => "Source",
        "promoted-headers" => "Promoted Headers",
        "demoted-headers" => "Demoted Headers",
        "changed-type" => "Changed Type",
        "changed-type-locale" => "Changed Type with Locale",
        "removed-other-columns" => "Removed Other Columns",
        "removed-columns" => "Removed Columns",
        "renamed-columns" => "Renamed Columns",
        "reordered-columns" => "Reordered Columns",
        "filtered-rows" => "Filtered Rows",
        "sorted-rows" => "Sorted Rows",
        "grouped-rows" => "Grouped Rows",
        "split-delimiter" => "Split Column by Delimiter",
        "split-positions" => "Split Column by Position",
        "split-lengths" => "Split Column by Position",
        "split-transition" => "Split Column by Character Transition",
        "lowercased-text" => "Lowercased Text",
        "uppercased-text" => "Uppercased Text",
        "capitalized-each-word" => "Capitalized Each Word",
        "trimmed-text" => "Trimmed Text",
        "cleaned-text" => "Cleaned Text",
        "added-prefix" => "Added Prefix",
        "added-suffix" => "Added Suffix",
        "text-length" => "Calculated Text Length",
        "reversed-rows" => "Reversed Rows",
        "rounded-off" => "Rounded Off",
        "rounded-up" => "Rounded Up",
        "rounded-down" => "Rounded Down",
        "absolute-value" => "Calculated Absolute Value",
        "square-root" => "Calculated Square Root",
        "extracted-year" => "Extracted Year",
        "extracted-month" => "Extracted Month",
        "extracted-day" => "Extracted Day",
        "inserted-year" => "Inserted Year",
        "inserted-month" => "Inserted Month",
        "inserted-day" => "Inserted Day",
        "duplicated-column" => "Duplicated Column",
        "removed-alternate-rows" => "Removed Alternate Rows",
        "kept-range" => "Kept Range of Rows",
        "kept-first-rows" => "Kept First Rows",
        "kept-last-rows" => "Kept Last Rows",
        "removed-top-rows" => "Removed Top Rows",
        "removed-bottom-rows" => "Removed Bottom Rows",
        "removed-duplicates" => "Removed Duplicates",
        "removed-blank-rows" => "Removed Blank Rows",
        "removed-errors" => "Removed Errors",
        "replaced-errors" => "Replaced Errors",
        "merged-columns" => "Merged Columns",
        "replaced-value" => "Replaced Value",
        "filled-down" => "Filled Down",
        "filled-up" => "Filled Up",
        "transposed-table" => "Transposed Table",
        "pivoted-column" => "Pivoted Column",
        "unpivoted-columns" => "Unpivoted Columns",
        "unpivoted-other-columns" => "Unpivoted Other Columns",
        "merged-queries" => "Merged Queries",
        "appended-query" => "Appended Query",
        "added-custom" => "Added Custom",
        "added-conditional-column" => "Added Conditional Column",
        "added-index" => "Added Index",
        "inserted-text-length" => "Inserted Text Length",
        "inserted-absolute-value" => "Inserted Absolute Value",
        "inserted-square-root" => "Inserted Square Root",
        _ => "Step",
    }
}

/// The id of the name a step is given (the language's name of it).
fn step_id(step: &Step) -> &'static str {
    match step {
        Step::Code { .. } => "m-code",
        Step::PromoteHeaders => "promoted-headers",
        Step::DemoteHeaders => "demoted-headers",
        Step::ChangeType { culture, .. } => if culture.name().is_empty() { "changed-type" } else { "changed-type-locale" },
        Step::SelectColumns(_) => "removed-other-columns",
        Step::RemoveColumns(_) => "removed-columns",
        Step::RenameColumns(_) => "renamed-columns",
        Step::ReorderColumns(_) => "reordered-columns",
        Step::FilterRows { .. } | Step::FilterCondition(_) => "filtered-rows",
        Step::WithMissing { inner, .. } => step_id(inner),
        Step::Transform { function, .. } => match function {
            Transform::Trim => "trimmed-text",
            Transform::Upper => "uppercased-text",
            Transform::Lower => "lowercased-text",
            Transform::Proper => "capitalized-each-word",
            Transform::Clean => "cleaned-text",
            Transform::Length => "text-length",
            Transform::Round(_) => "rounded-off",
            Transform::RoundUp => "rounded-up",
            Transform::RoundDown => "rounded-down",
            Transform::Abs => "absolute-value",
            Transform::Sqrt => "square-root",
            Transform::Year => "extracted-year",
            Transform::Month => "extracted-month",
            Transform::Day => "extracted-day",
            Transform::Prefix(_) => "added-prefix",
            Transform::Suffix(_) => "added-suffix",
        },
        Step::GroupByLocal { .. } | Step::GroupBy { .. } => "grouped-rows",
        Step::AlternateRows { .. } => "removed-alternate-rows",
        Step::KeepRange { .. } => "kept-range",
        Step::DuplicateColumn { .. } => "duplicated-column",
        Step::KeepTopRows(_) => "kept-first-rows",
        Step::KeepBottomRows(_) => "kept-last-rows",
        Step::RemoveTopRows(_) => "removed-top-rows",
        Step::RemoveBottomRows(_) => "removed-bottom-rows",
        Step::RemoveDuplicates(_) => "removed-duplicates",
        Step::RemoveBlankRows => "removed-blank-rows",
        Step::RemoveErrors(_) => "removed-errors",
        Step::ReplaceErrors { .. } => "replaced-errors",
        Step::Sort(_) => "sorted-rows",
        Step::SplitColumn { by, .. } => match by {
            SplitBy::Delimiter(_) | SplitBy::AnyDelimiter(_) | SplitBy::EachDelimiter { .. } => "split-delimiter",
            // 文字数 (the dialog of a number of characters) and 位置 (the dialog of positions) are named differently.
            SplitBy::Length(_) | SplitBy::Once { .. } | SplitBy::Lengths { .. } => "split-lengths",
            SplitBy::Positions { .. } => "split-positions",
            SplitBy::Transition { .. } => "split-transition",
        },
        Step::MergeColumns { .. } => "merged-columns",
        Step::ReplaceValue { .. } | Step::ReplaceNull { .. } => "replaced-value",
        Step::FillDown(_) => "filled-down",
        Step::FillUp(_) => "filled-up",
        Step::Transpose => "transposed-table",
        Step::ReverseRows => "reversed-rows",
        Step::Trim(_) => "trimmed-text",
        Step::Upper(_) => "uppercased-text",
        Step::Lower(_) => "lowercased-text",
        Step::Pivot { .. } => "pivoted-column",
        Step::Unpivot { .. } => "unpivoted-columns",
        Step::UnpivotOther { .. } => "unpivoted-other-columns",
        Step::Merge { .. } => "merged-queries",
        Step::Append { .. } => "appended-query",
        // The columns of a date Excel adds by 列の追加 → 日付 are named by what they hold.
        Step::AddColumn { formula, .. } => match formula.split('(').next() {
            Some("Date.Year") if formula.ends_with(')') => "inserted-year",
            Some("Date.Month") if formula.ends_with(')') => "inserted-month",
            Some("Date.Day") if formula.ends_with(')') => "inserted-day",
            // 列の追加 → 抽出 → 長さ, and 列の追加 → 指数 → 絶対値・平方根 (Excel names them by what they hold; the 変換 tab's are the Transform ones).
            Some("Text.Length") if formula.ends_with(')') => "inserted-text-length",
            Some("Number.Abs") if formula.ends_with(')') => "inserted-absolute-value",
            Some("Number.Sqrt") if formula.ends_with(')') => "inserted-square-root",
            _ => "added-custom",
        },
        Step::AddConditionalColumn { .. } => "added-conditional-column",
        Step::AddIndexColumn { .. } => "added-index",
    }
}

/// How many type changes from `at` are written in one `Table.TransformColumnTypes` (Excel adds the change of a column the step does
/// not hold yet to the step before; one that does, or of another locale, or one the step marks `apart`, begins another).
pub(super) fn type_group_len(steps: &[Step], at: usize) -> usize {
    let Step::ChangeType { culture, .. } = &steps[at] else { return 1 };
    let mut columns: Vec<&str> = Vec::new();
    let mut n = 0;
    for step in &steps[at..] {
        let Step::ChangeType { column, culture: c, apart, .. } = step else { break };
        if c != culture || (n > 0 && (*apart || columns.contains(&column.as_str()))) {
            break;
        }
        columns.push(column);
        n += 1;
    }
    n
}

/// Whether a type change of `column` (in `culture`) would be written into the group the steps end with.
pub(super) fn joins_type_group(steps: &[Step], column: &str, culture: Culture) -> bool {
    let mut at = 0;
    let mut last: Option<(usize, usize)> = None;
    while at < steps.len() {
        let n = type_group_len(steps, at);
        if matches!(steps[at], Step::ChangeType { .. }) {
            last = Some((at, n));
        }
        at += n;
    }
    let Some((from, n)) = last else { return false };
    if from + n != steps.len() {
        return false;
    }
    steps[from..].iter().all(|s| matches!(s, Step::ChangeType { culture: c, column: k, .. } if *c == culture && k != column))
}

/// `Table.TransformColumnTypes` of a group of type changes (all of one locale): no space after the table, and with a locale a space
/// before the list and the locale after it.
fn type_change(p: &str, group: &[Step]) -> String {
    let items: Vec<String> = group
        .iter()
        .filter_map(|s| match s {
            Step::ChangeType { column, ty, .. } => Some(format!("{{{}, {}}}", q(column), ty.m_name())),
            _ => None,
        })
        .collect();
    let types = format!("{{{}}}", items.join(", "));
    match group.first() {
        Some(Step::ChangeType { culture, .. }) if !culture.name().is_empty() => format!("Table.TransformColumnTypes({p}, {types}, {})", q(culture.name())),
        _ => format!("Table.TransformColumnTypes({p},{types})"),
    }
}

/// `Table.TransformColumns` of a text function over columns.
fn text_function(p: &str, columns: &[String], function: &str) -> String {
    format!("Table.TransformColumns({p},{{{}}})", columns.iter().map(|c| format!("{{{}, {function}, type text}}", q(c))).collect::<Vec<_>>().join(", "))
}

/// The M that makes a step from `prev`; `types` are the types the columns have been given (the type a function leaves a column in
/// follows from them).
fn step_body(step: &Step, prev: &str, types: &HashMap<String, &'static str>) -> String {
    let p = id(prev);
    match step {
        // The code step is written by `query_to_m_named` itself (as it was written).
        Step::Code { .. } => String::new(),
        Step::PromoteHeaders => format!("Table.PromoteHeaders({p}, [PromoteAllScalars=true])"),
        Step::DemoteHeaders => format!("Table.DemoteHeaders({p})"),
        Step::ChangeType { .. } => type_change(&p, std::slice::from_ref(step)),
        Step::SelectColumns(c) => format!("Table.SelectColumns({p},{})", list(c)),
        Step::RemoveColumns(c) => format!("Table.RemoveColumns({p},{})", list(c)),
        Step::RenameColumns(renames) => format!("Table.RenameColumns({p},{{{}}})", renames.iter().map(|(from, to)| format!("{{{}, {}}}", q(from), q(to))).collect::<Vec<_>>().join(", ")),
        Step::ReorderColumns(c) => format!("Table.ReorderColumns({p},{})", list(c)),
        Step::FilterRows { column, op, value } => format!("Table.SelectRows({p}, each {})", cond_m(column, *op, value)),
        Step::FilterCondition(cond) => format!("Table.SelectRows({p}, each {})", cond_text(cond)),
        Step::WithMissing { inner, missing } => match &**inner {
            // The type change takes it in the record of its options.
            Step::ChangeType { column, ty, culture, .. } => {
                let options = if culture.name().is_empty() { format!("[MissingField={}]", missing.m_name()) } else { format!("[Culture={}, MissingField={}]", q(culture.name()), missing.m_name()) };
                format!("Table.TransformColumnTypes({p}, {{{{{}, {}}}}}, {options})", q(column), ty.m_name())
            }
            other => {
                let body = step_body(other, prev, types);
                format!("{}, {})", &body[..body.len() - 1], missing.m_name())
            }
        },
        Step::Transform { column, function } => {
            let ty = result_type(column, function, types);
            // The text put before or after is written with a space after the table (Excel does).
            let gap = if matches!(function, Transform::Prefix(_) | Transform::Suffix(_)) { " " } else { "" };
            format!("Table.TransformColumns({p},{gap}{{{{{}, {}, {ty}}}}})", q(column), function.to_m())
        }
        Step::GroupByLocal { keys, aggregations } => {
            let body = step_body(&Step::GroupBy { keys: keys.clone(), aggregations: aggregations.clone() }, prev, types);
            format!("{}, GroupKind.Local)", &body[..body.len() - 1])
        }
        Step::AlternateRows { offset, skip, take } => format!("Table.AlternateRows({p},{offset},{skip},{take})"),
        Step::KeepRange { offset, count } => match count {
            Some(c) => format!("Table.Range({p},{offset},{c})"),
            None => format!("Table.Range({p},{offset})"),
        },
        Step::DuplicateColumn { column, new_name } => format!("Table.DuplicateColumn({p}, {}, {})", q(column), q(new_name)),
        Step::KeepTopRows(n) => format!("Table.FirstN({p}, {n})"),
        Step::KeepBottomRows(n) => format!("Table.LastN({p}, {n})"),
        Step::RemoveTopRows(n) => format!("Table.Skip({p}, {n})"),
        Step::RemoveBottomRows(n) => format!("Table.RemoveLastN({p}, {n})"),
        Step::RemoveDuplicates(c) => if c.is_empty() { format!("Table.Distinct({p})") } else { format!("Table.Distinct({p}, {})", list(c)) },
        Step::RemoveBlankRows => format!("Table.SelectRows({p}, each not List.IsEmpty(List.RemoveMatchingItems(Record.FieldValues(_), {{\"\", null}})))"),
        Step::RemoveErrors(c) => if c.is_empty() { format!("Table.RemoveRowsWithErrors({p})") } else { format!("Table.RemoveRowsWithErrors({p}, {})", list(c)) },
        Step::ReplaceErrors { columns, with } => format!("Table.ReplaceErrorValues({p}, {{{}}})", columns.iter().map(|c| format!("{{{}, {}}}", q(c), lit(with))).collect::<Vec<_>>().join(", ")),
        Step::Sort(keys) => format!("Table.Sort({p},{{{}}})", keys.iter().map(|(c, asc)| format!("{{{}, Order.{}}}", q(c), if *asc { "Ascending" } else { "Descending" })).collect::<Vec<_>>().join(", ")),
        Step::SplitColumn { column, by, columns } => {
            // The names of the new columns: Excel writes as many as the pieces it saw.
            let names = if *columns > 0 { format!(", {}", list(&(1..=*columns).map(|k| format!("{column}.{k}")).collect::<Vec<_>>())) } else { String::new() };
            format!("Table.SplitColumn({p}, {}, {}{names})", q(column), splitter_m(by))
        }
        Step::MergeColumns { columns, separator, name } => format!("Table.CombineColumns({p}, {}, Combiner.CombineTextByDelimiter({}, QuoteStyle.None), {})", list(columns), q(separator), q(name)),
        Step::ReplaceValue { column, find, with, whole_cell } => format!("Table.ReplaceValue({p}, {}, {}, Replacer.{}, {{{}}})", lit(find), lit(with), if *whole_cell { "ReplaceValue" } else { "ReplaceText" }, q(column)),
        Step::ReplaceNull { columns, with } => format!("Table.ReplaceValue({p}, null, {}, Replacer.ReplaceValue, {})", lit(with), list(columns)),
        Step::FillDown(c) => format!("Table.FillDown({p}, {})", list(c)),
        Step::FillUp(c) => format!("Table.FillUp({p}, {})", list(c)),
        Step::Transpose => format!("Table.Transpose({p})"),
        Step::ReverseRows => format!("Table.ReverseRows({p})"),
        Step::Trim(c) => text_function(&p, c, "Text.Trim"),
        Step::Upper(c) => text_function(&p, c, "Text.Upper"),
        Step::Lower(c) => text_function(&p, c, "Text.Lower"),
        Step::Pivot { attribute, values, agg } => format!("Table.Pivot({p}, List.Distinct({p}{}), {}, {}, {})", col_ref(attribute), q(attribute), q(values), agg_m(*agg)),
        Step::Unpivot { columns, attribute, value } => format!("Table.UnpivotOtherColumns({p}, List.Difference(Table.ColumnNames({p}), {}), {}, {})", list(columns), q(attribute), q(value)),
        Step::UnpivotOther { keep, attribute, value } => format!("Table.UnpivotOtherColumns({p}, {}, {}, {})", list(keep), q(attribute), q(value)),
        Step::GroupBy { keys, aggregations } => format!(
            "Table.Group({p}, {}, {{{}}})",
            list(keys),
            aggregations
                .iter()
                .map(|(name, agg, col)| match (agg, col) {
                    (Agg::Count, _) => format!("{{{}, each Table.RowCount(_), Int64.Type}}", q(name)),
                    (Agg::CountDistinct, Some(c)) => format!("{{{}, each List.Count(List.Distinct({})), Int64.Type}}", q(name), col_ref(c)),
                    (_, Some(c)) => format!("{{{}, each {}({}), type nullable number}}", q(name), agg_m(*agg).trim_start_matches("each "), col_ref(c)),
                    _ => format!("{{{}, each Table.RowCount(_), Int64.Type}}", q(name)),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Step::Merge { right, left_keys, right_keys, kind, expand } => {
            let k = match kind {
                JoinKind::LeftOuter => "LeftOuter",
                JoinKind::RightOuter => "RightOuter",
                JoinKind::FullOuter => "FullOuter",
                JoinKind::Inner => "Inner",
                JoinKind::LeftAnti => "LeftAnti",
                JoinKind::RightAnti => "RightAnti",
            };
            format!("Table.ExpandTableColumn(Table.NestedJoin({p}, {}, {}, {}, {}, JoinKind.{k}), {}, {})", list(left_keys), id(right), list(right_keys), q(right), q(right), list(expand))
        }
        Step::Append { other } => format!("Table.Combine({{{p}, {}}})", id(other)),
        Step::AddColumn { name, formula, ty } => match ty {
            Some(ty) => format!("Table.AddColumn({p}, {}, each {formula}, {})", q(name), ty.m_name()),
            None => format!("Table.AddColumn({p}, {}, each {formula})", q(name)),
        },
        Step::AddConditionalColumn { name, clauses, otherwise } => {
            let mut text = String::new();
            for Clause { column, op, value, output } in clauses {
                text.push_str(&format!("if {} then {} else ", cond_m(column, *op, value), lit(output)));
            }
            text.push_str(&lit(otherwise));
            format!("Table.AddColumn({p}, {}, each {text})", q(name))
        }
        Step::AddIndexColumn { name, start, step } => format!("Table.AddIndexColumn({p}, {}, {start}, {step}, Int64.Type)", q(name)),
    }
}

/// The type a column is left in by a function: `Number.Abs` leaves the type it had.
fn result_type(column: &str, function: &Transform, types: &HashMap<String, &'static str>) -> &'static str {
    match function {
        Transform::Abs => types.get(column).copied().unwrap_or("type number"),
        f => f.result_type_m(),
    }
}

/// The M code of `query`: `let Source = …, step = …  in step`, the steps named in English.
pub fn query_to_m(query: &Query) -> String {
    query_to_m_named(query, &|id| english_step_name(id).to_string())
}

/// The source as the lines of M that make it, and the name of the step that holds the table to go on from.
fn source_lines(source: &Source, first: &str) -> (Vec<String>, String) {
    let line = |name: &str, body: String| format!("    {} = {body}", id(name));
    match source {
        Source::Blank => (vec![line(first, "#table({}, {})".to_string())], first.to_string()),
        Source::Table(name) => (vec![line(first, format!("Excel.CurrentWorkbook(){{[Name={}]}}[Content]", q(name)))], first.to_string()),
        Source::Range { sheet, range } => (vec![line(first, format!("Excel.CurrentWorkbook(){{[Name={}]}}[Content] /* {range} */", q(sheet)))], first.to_string()),
        Source::Csv { path, delimiter, columns, encoding, quote } => (
            vec![line(first, format!("Csv.Document(File.Contents({}),[Delimiter={}, Columns={columns}, Encoding={encoding}, QuoteStyle=QuoteStyle.{quote}])", q(path), q(delimiter)))],
            first.to_string(),
        ),
        Source::Workbook { path, item, sheet } => {
            let kind = if *sheet { "Sheet" } else { "Table" };
            let nav = format!("{item}_{kind}");
            (
                vec![
                    line(first, format!("Excel.Workbook(File.Contents({}), null, true)", q(path))),
                    line(&nav, format!("{}{{[Item={},Kind={}]}}[Data]", id(first), q(item), q(kind))),
                ],
                nav,
            )
        }
    }
}

/// The M code of `query`, its steps named by `name_of` (the name of a step by its id, in the language of the screen).
///
/// A query that ends with a code step is written with the names of its other steps as the code spells them (the code refers to them),
/// when they are as many as the steps make; then the code as it was written, and `in` its result.
pub fn query_to_m_named(query: &Query, name_of: &dyn Fn(&str) -> String) -> String {
    let code = match query.steps.last() {
        Some(Step::Code { names, bindings, result }) => Some((names, bindings, result)),
        _ => None,
    };
    if let Some((names, ..)) = code {
        let (text, made) = m_text(query, name_of, Some(names));
        if made.len() == names.len() {
            return text;
        }
    }
    m_text(query, name_of, None).0
}

fn m_text(query: &Query, name_of: &dyn Fn(&str) -> String, forced: Option<&Vec<String>>) -> (String, Vec<String>) {
    let first = forced.and_then(|f| f.first().cloned()).unwrap_or_else(|| name_of("source"));
    let (mut lines, mut prev) = source_lines(&query.source, &first);
    let mut used: Vec<String> = vec![first];
    if prev != used[0] {
        used.push(prev.clone());
    }
    let mut types: HashMap<String, &'static str> = HashMap::new();
    let steps = &query.steps;
    let mut at = 0;
    let mut tail = None;
    while at < steps.len() {
        let step = &steps[at];
        if let Step::Code { bindings, result, .. } = step {
            tail = Some((bindings, result));
            break;
        }
        let mut taken = 1;
        let body = match step {
            Step::ChangeType { .. } => {
                taken = type_group_len(steps, at);
                let group = &steps[at..at + taken];
                for s in group {
                    if let Step::ChangeType { column, ty, .. } = s {
                        types.insert(column.clone(), ty.m_name());
                    }
                }
                type_change(&id(&prev), group)
            }
            other => {
                match other {
                    Step::Transform { column, function } => {
                        types.insert(column.clone(), result_type(column, function, &types));
                    }
                    Step::Trim(c) | Step::Upper(c) | Step::Lower(c) => {
                        for column in c {
                            types.insert(column.clone(), "type text");
                        }
                    }
                    _ => {}
                }
                step_body(other, &prev, &types)
            }
        };
        let name = match forced.and_then(|f| f.get(used.len())) {
            Some(kept) => kept.clone(),
            None => {
                let base = name_of(step_id(step));
                if used.contains(&base) { (1..).map(|i| format!("{base}{i}")).find(|n| !used.contains(n)).unwrap_or(base.clone()) } else { base }
            }
        };
        lines.push(format!("    {} = {body}", id(&name)));
        used.push(name.clone());
        prev = name;
        at += taken;
    }
    match tail {
        Some((bindings, result)) => {
            let made = used.clone();
            lines.push(format!("    {}", bindings.trim()));
            (format!("let\n{}\nin\n    {}", lines.join(",\n"), result.trim()), made)
        }
        None => (format!("let\n{}\nin\n    {}", lines.join(",\n"), id(&prev)), used),
    }
}
