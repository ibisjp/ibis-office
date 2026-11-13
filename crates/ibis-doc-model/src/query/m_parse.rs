// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The M code of a query read back into its steps (GitHub issue #173: the 詳細エディター lets the code be edited).
//! It reads the code `query_to_m` writes (`let`, a chain of steps each made from the one before, `in` the last one): the
//! functions and the argument shapes of `step_m`, with spaces, line breaks and comments free. Code that uses anything else
//! is refused with the reason; the query stays as it was.

use super::{Agg, Clause, Culture, JoinKind, Op, QType, Query, SplitBy, Source, Step};

/// Why code was refused: a key of the screen's text table (`query.code-error.<key>`) and the text it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeError {
    pub key: &'static str,
    pub detail: String,
}

fn fail<T>(key: &'static str, detail: impl Into<String>) -> Result<T, CodeError> {
    Err(CodeError { key, detail: detail.into() })
}

/// The end (index of the closing character) of the string literal opening at `i`.
fn string_end(b: &[u8], i: usize) -> Option<usize> {
    let mut k = i + 1;
    while k < b.len() {
        if b[k] == b'"' {
            if b.get(k + 1) == Some(&b'"') {
                k += 2;
                continue;
            }
            return Some(k);
        }
        k += 1;
    }
    None
}

/// Where a comment opening at `i` ends (the index after it).
fn comment_end(b: &[u8], i: usize) -> Option<usize> {
    if b.get(i) == Some(&b'/') && b.get(i + 1) == Some(&b'*') {
        let mut k = i + 2;
        while k + 1 < b.len() {
            if b[k] == b'*' && b[k + 1] == b'/' {
                return Some(k + 2);
            }
            k += 1;
        }
        return Some(b.len());
    }
    if b.get(i) == Some(&b'/') && b.get(i + 1) == Some(&b'/') {
        let mut k = i;
        while k < b.len() && b[k] != b'\n' {
            k += 1;
        }
        return Some(k);
    }
    None
}

/// The text cut at the commas that are not inside brackets, strings or comments.
fn split_top(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let (mut out, mut depth, mut start, mut i) = (Vec::new(), 0i32, 0, 0);
    while i < b.len() {
        match b[i] {
            b'"' => match string_end(b, i) {
                Some(e) => i = e,
                None => break,
            },
            b'/' => {
                if let Some(e) = comment_end(b, i) {
                    i = e;
                    continue;
                }
            }
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            b',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

/// The text without its comments (strings are kept as they are).
fn strip_comments(s: &str) -> String {
    let b = s.as_bytes();
    let (mut out, mut i, mut from) = (String::new(), 0, 0);
    while i < b.len() {
        if b[i] == b'"' {
            match string_end(b, i) {
                Some(e) => i = e + 1,
                None => break,
            }
            continue;
        }
        if let Some(e) = comment_end(b, i) {
            out.push_str(&s[from..i]);
            out.push(' ');
            i = e;
            from = e;
            continue;
        }
        i += 1;
    }
    out.push_str(&s[from..]);
    out
}

fn unquote(s: &str) -> Option<String> {
    let s = s.trim();
    let inner = s.strip_prefix('"')?.strip_suffix('"')?;
    // The closing quote must be the string's own.
    if string_end(s.as_bytes(), 0)? != s.len() - 1 {
        return None;
    }
    Some(inner.replace("\"\"", "\""))
}

/// A step or query name: `Name` or `#"Any name"`.
fn parse_id(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('#') {
        return unquote(rest);
    }
    (!s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_')).then(|| s.to_string())
}

fn braces(s: &str) -> Option<&str> {
    s.trim().strip_prefix('{')?.strip_suffix('}')
}

fn items(s: &str) -> Option<Vec<&str>> {
    let inner = braces(s)?;
    Some(if inner.trim().is_empty() { Vec::new() } else { split_top(inner) })
}

fn strings(s: &str) -> Option<Vec<String>> {
    items(s)?.into_iter().map(unquote).collect()
}

/// A list of pairs `{{a, b}, {c, d}}` as the parts of each pair.
fn pairs(s: &str) -> Option<Vec<Vec<&str>>> {
    items(s)?.into_iter().map(|p| items(p)).collect()
}

/// A text or a number written in a step, as its value.
fn value_of(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(t) = unquote(s) {
        return Some(t);
    }
    s.parse::<f64>().is_ok().then(|| s.to_string())
}

/// `Name(args)` as the name and the text between the brackets, when the call is all of `s`.
fn call(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    let open = s.find('(')?;
    let name = &s[..open];
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_') {
        return None;
    }
    // The bracket opened at `open` must close at the very end.
    let b = s.as_bytes();
    let (mut depth, mut i) = (0i32, open);
    while i < b.len() {
        match b[i] {
            b'"' => i = string_end(b, i)?,
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return (i == s.len() - 1).then(|| (name, &s[open + 1..i]));
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn number<T: std::str::FromStr>(s: &str) -> Option<T> {
    s.trim().parse().ok()
}

fn column_of(s: &str) -> Option<String> {
    let s = s.trim();
    s.strip_prefix('[')?.strip_suffix(']').map(str::to_string)
}

fn agg_of(s: &str) -> Option<Agg> {
    Some(match s.trim() {
        "List.Sum" => Agg::Sum,
        "List.Average" => Agg::Average,
        "List.Min" => Agg::Min,
        "List.Max" => Agg::Max,
        "List.Count" => Agg::Count,
        "each List.Count(List.Distinct(_))" => Agg::CountDistinct,
        "List.Median" => Agg::Median,
        "List.First" => Agg::First,
        _ => return None,
    })
}

/// The inverse of the conditions `step_m` writes.
fn condition(s: &str) -> Option<(String, Op, String)> {
    let s = s.trim();
    let (negated, body) = match s.strip_prefix("not ") {
        Some(rest) => (true, rest.trim()),
        None => (false, s),
    };
    if let Some((name, args)) = call(body) {
        let parts = split_top(args);
        let op = match (name, negated) {
            ("Text.Contains", false) => Op::Contains,
            ("Text.Contains", true) => Op::DoesNotContain,
            ("Text.StartsWith", false) => Op::BeginsWith,
            ("Text.EndsWith", false) => Op::EndsWith,
            _ => return None,
        };
        if parts.len() != 2 {
            return None;
        }
        return Some((column_of(parts[0])?, op, unquote(parts[1])?));
    }
    if negated || !body.starts_with('[') {
        return None;
    }
    let close = body.find(']')?;
    let column = body[1..close].to_string();
    let rest = body[close + 1..].trim();
    for (text, op) in [("<>", Op::NotEquals), (">=", Op::GreaterOrEqual), ("<=", Op::LessOrEqual), ("=", Op::Equals), (">", Op::Greater), ("<", Op::Less)] {
        if let Some(value) = rest.strip_prefix(text) {
            let value = value.trim();
            if value == "null" {
                return match op {
                    Op::Equals => Some((column, Op::IsNull, String::new())),
                    Op::NotEquals => Some((column, Op::IsNotNull, String::new())),
                    _ => None,
                };
            }
            return Some((column, op, value_of(value)?));
        }
    }
    None
}

/// `if c then o else if … else x` as the clauses and the last output.
fn clauses(s: &str) -> Option<(Vec<Clause>, String)> {
    let mut rest = s.trim();
    let mut out = Vec::new();
    while let Some(after) = rest.strip_prefix("if ") {
        let then = find_word(after, " then ")?;
        let (condition_text, after) = (&after[..then], &after[then + 6..]);
        let other = find_word(after, " else ")?;
        let (output, tail) = (&after[..other], &after[other + 6..]);
        let (column, op, value) = condition(condition_text)?;
        out.push(Clause { column, op, value, output: value_of(output)? });
        rest = tail.trim();
    }
    if out.is_empty() {
        return None;
    }
    Some((out, value_of(rest)?))
}

/// The first place of `word` outside strings.
fn find_word(s: &str, word: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'"' {
            i = string_end(b, i)? + 1;
            continue;
        }
        if b[i..].starts_with(word.as_bytes()) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn is_prev(arg: &str, prev: &str) -> bool {
    parse_id(arg).as_deref() == Some(prev)
}

/// The step the call `body` makes from the step `prev`.
fn step_of(body: &str, prev: &str) -> Result<Step, CodeError> {
    let Some((name, args)) = call(body) else { return fail("syntax", body.trim()) };
    let a = split_top(args);
    let bad = || CodeError { key: "arguments", detail: name.to_string() };
    let step = (|| -> Option<Step> {
        // Every step but a few is made from the step before it, as its first argument.
        let n = a.len();
        let first = a.first().is_some_and(|x| is_prev(x, prev));
        Some(match name {
            "Table.PromoteHeaders" if first && n == 2 => Step::PromoteHeaders,
            "Table.DemoteHeaders" if first && n == 1 => Step::DemoteHeaders,
            "Table.TransformColumnTypes" if first && (n == 2 || n == 3) => {
                let p = pairs(a[1])?;
                let [one] = &p[..] else { return None };
                let [column, ty] = &one[..] else { return None };
                let ty = QType::ALL.into_iter().find(|t| t.m_name() == ty.trim())?;
                let culture = match a.get(2) {
                    Some(c) => {
                        let c = unquote(c)?;
                        Culture::ALL.into_iter().find(|k| k.name() == c)?
                    }
                    None => Culture::Invariant,
                };
                Step::ChangeType { column: unquote(column)?, ty, culture }
            }
            "Table.SelectColumns" if first && n == 2 => Step::SelectColumns(strings(a[1])?),
            "Table.RemoveColumns" if first && n == 2 => Step::RemoveColumns(strings(a[1])?),
            "Table.RenameColumns" if first && n == 2 => {
                let p = pairs(a[1])?;
                let [one] = &p[..] else { return None };
                let [from, to] = &one[..] else { return None };
                Step::RenameColumn { from: unquote(from)?, to: unquote(to)? }
            }
            "Table.ReorderColumns" if first && n == 2 => Step::ReorderColumns(strings(a[1])?),
            "Table.SelectRows" if first && n == 2 => {
                let text = a[1].trim().strip_prefix("each ")?;
                if text.trim() == "not List.IsEmpty(List.RemoveMatchingItems(Record.FieldValues(_), {\"\", null}))" {
                    Step::RemoveBlankRows
                } else {
                    let (column, op, value) = condition(text)?;
                    Step::FilterRows { column, op, value }
                }
            }
            "Table.FirstN" if first && n == 2 => Step::KeepTopRows(number(a[1])?),
            "Table.LastN" if first && n == 2 => Step::KeepBottomRows(number(a[1])?),
            "Table.Skip" if first && n == 2 => Step::RemoveTopRows(number(a[1])?),
            "Table.RemoveLastN" if first && n == 2 => Step::RemoveBottomRows(number(a[1])?),
            "Table.Distinct" if first && n <= 2 => Step::RemoveDuplicates(if n == 2 { strings(a[1])? } else { Vec::new() }),
            "Table.RemoveRowsWithErrors" if first && n <= 2 => Step::RemoveErrors(if n == 2 { strings(a[1])? } else { Vec::new() }),
            "Table.ReplaceErrorValues" if first && n == 2 => {
                let p = pairs(a[1])?;
                let mut columns = Vec::new();
                let mut with = String::new();
                for pair in &p {
                    let [column, value] = &pair[..] else { return None };
                    columns.push(unquote(column)?);
                    with = value_of(value)?;
                }
                Step::ReplaceErrors { columns, with }
            }
            "Table.Sort" if first && n == 2 => {
                let mut keys = Vec::new();
                for pair in pairs(a[1])? {
                    let [column, order] = &pair[..] else { return None };
                    keys.push((unquote(column)?, match order.trim() {
                        "Order.Ascending" => true,
                        "Order.Descending" => false,
                        _ => return None,
                    }));
                }
                Step::Sort(keys)
            }
            "Table.SplitColumn" if first && n == 3 => {
                let (splitter, inner) = call(a[2])?;
                let by = match splitter {
                    "Splitter.SplitTextByDelimiter" => SplitBy::Delimiter(unquote(split_top(inner).first()?)?),
                    "Splitter.SplitTextByRepeatedLengths" => SplitBy::Length(number(inner)?),
                    _ => return None,
                };
                Step::SplitColumn { column: unquote(a[1])?, by }
            }
            "Table.CombineColumns" if first && n == 4 => {
                let (combiner, inner) = call(a[2])?;
                if combiner != "Combiner.CombineTextByDelimiter" {
                    return None;
                }
                Step::MergeColumns { columns: strings(a[1])?, separator: unquote(split_top(inner).first()?)?, name: unquote(a[3])? }
            }
            "Table.ReplaceValue" if first && n == 5 => {
                let whole_cell = match a[3].trim() {
                    "Replacer.ReplaceValue" => true,
                    "Replacer.ReplaceText" => false,
                    _ => return None,
                };
                if a[1].trim() == "null" {
                    if !whole_cell {
                        return None;
                    }
                    Step::ReplaceNull { columns: strings(a[4])?, with: value_of(a[2])? }
                } else {
                    let columns = strings(a[4])?;
                    let [column] = &columns[..] else { return None };
                    Step::ReplaceValue { column: column.clone(), find: value_of(a[1])?, with: value_of(a[2])?, whole_cell }
                }
            }
            "Table.FillDown" if first && n == 2 => Step::FillDown(strings(a[1])?),
            "Table.FillUp" if first && n == 2 => Step::FillUp(strings(a[1])?),
            "Table.Transpose" if first && n == 1 => Step::Transpose,
            "Table.ReverseRows" if first && n == 1 => Step::ReverseRows,
            "Table.TransformColumns" if first && n == 2 => {
                let mut kind = None;
                let mut columns = Vec::new();
                for pair in pairs(a[1])? {
                    let [column, function, ty] = &pair[..] else { return None };
                    if ty.trim() != "type text" {
                        return None;
                    }
                    let k = match function.trim() {
                        "Text.Trim" => 0,
                        "Text.Upper" => 1,
                        "Text.Lower" => 2,
                        _ => return None,
                    };
                    if kind.is_some_and(|x| x != k) {
                        return None;
                    }
                    kind = Some(k);
                    columns.push(unquote(column)?);
                }
                match kind? {
                    0 => Step::Trim(columns),
                    1 => Step::Upper(columns),
                    _ => Step::Lower(columns),
                }
            }
            "Table.Pivot" if first && n == 5 => Step::Pivot { attribute: unquote(a[2])?, values: unquote(a[3])?, agg: agg_of(a[4])? },
            "Table.UnpivotOtherColumns" if first && n == 4 => {
                let (attribute, value) = (unquote(a[2])?, unquote(a[3])?);
                match call(a[1]) {
                    Some(("List.Difference", inner)) => {
                        let parts = split_top(inner);
                        let [names, columns] = &parts[..] else { return None };
                        let (function, of) = call(names)?;
                        if function != "Table.ColumnNames" || !is_prev(of, prev) {
                            return None;
                        }
                        Step::Unpivot { columns: strings(columns)?, attribute, value }
                    }
                    _ => Step::UnpivotOther { keep: strings(a[1])?, attribute, value },
                }
            }
            "Table.Group" if first && n == 3 => {
                let mut aggregations = Vec::new();
                for pair in pairs(a[2])? {
                    let [name, function, _] = &pair[..] else { return None };
                    let function = function.trim().strip_prefix("each ")?.trim();
                    if function == "Table.RowCount(_)" {
                        aggregations.push((unquote(name)?, Agg::Count, None));
                    } else {
                        let (f, column) = call(function)?;
                        aggregations.push((unquote(name)?, agg_of(f)?, Some(column_of(column)?)));
                    }
                }
                Step::GroupBy { keys: strings(a[1])?, aggregations }
            }
            "Table.ExpandTableColumn" if n == 3 => {
                let (join, inner) = call(a[0])?;
                if join != "Table.NestedJoin" {
                    return None;
                }
                let j = split_top(inner);
                if j.len() != 6 || !is_prev(j[0], prev) {
                    return None;
                }
                let kind = match j[5].trim().strip_prefix("JoinKind.")? {
                    "LeftOuter" => JoinKind::LeftOuter,
                    "RightOuter" => JoinKind::RightOuter,
                    "FullOuter" => JoinKind::FullOuter,
                    "Inner" => JoinKind::Inner,
                    "LeftAnti" => JoinKind::LeftAnti,
                    "RightAnti" => JoinKind::RightAnti,
                    _ => return None,
                };
                Step::Merge { right: parse_id(j[2])?, left_keys: strings(j[1])?, right_keys: strings(j[3])?, kind, expand: strings(a[2])? }
            }
            "Table.Combine" if n == 1 => {
                let parts = items(a[0])?;
                let [from, other] = &parts[..] else { return None };
                if !is_prev(from, prev) {
                    return None;
                }
                Step::Append { other: parse_id(other)? }
            }
            "Table.AddColumn" if first && n == 3 => {
                let formula = a[2].trim().strip_prefix("each ")?.trim();
                let column = unquote(a[1])?;
                match clauses(formula) {
                    Some((clauses, otherwise)) => Step::AddConditionalColumn { name: column, clauses, otherwise },
                    None => Step::AddColumn { name: column, formula: formula.to_string() },
                }
            }
            "Table.AddIndexColumn" if first && n == 5 => Step::AddIndexColumn { name: unquote(a[1])?, start: number(a[2])?, step: number(a[3])? },
            _ => return None,
        })
    })();
    match step {
        Some(step) => Ok(step),
        None if is_known(name) => Err(bad()),
        None => fail("function", name),
    }
}

fn is_known(name: &str) -> bool {
    matches!(
        name,
        "Table.PromoteHeaders" | "Table.DemoteHeaders" | "Table.TransformColumnTypes" | "Table.SelectColumns" | "Table.RemoveColumns" | "Table.RenameColumns"
            | "Table.ReorderColumns" | "Table.SelectRows" | "Table.FirstN" | "Table.LastN" | "Table.Skip" | "Table.RemoveLastN" | "Table.Distinct"
            | "Table.RemoveRowsWithErrors" | "Table.ReplaceErrorValues" | "Table.Sort" | "Table.SplitColumn" | "Table.CombineColumns" | "Table.ReplaceValue"
            | "Table.FillDown" | "Table.FillUp" | "Table.Transpose" | "Table.ReverseRows" | "Table.TransformColumns" | "Table.Pivot" | "Table.UnpivotOtherColumns"
            | "Table.Group" | "Table.ExpandTableColumn" | "Table.Combine" | "Table.AddColumn" | "Table.AddIndexColumn"
    )
}

fn source_of(body: &str) -> Result<Source, CodeError> {
    let compact: String = body.split_whitespace().collect();
    if compact == "#table({},{})" {
        return Ok(Source::Blank);
    }
    let b = body.as_bytes();
    let Some(at) = body.find("[Name=") else { return fail("source", body.trim()) };
    let start = body[at + 6..].find('"').map(|k| at + 6 + k);
    let name = start.and_then(|s| string_end(b, s).map(|e| (body[s..=e].to_string(), e)));
    let Some((literal, end)) = name else { return fail("source", body.trim()) };
    let Some(name) = unquote(&literal) else { return fail("source", body.trim()) };
    let tail = &body[end + 1..];
    if !compact.starts_with("Excel.CurrentWorkbook(){[Name=") || !tail.trim_start().starts_with("]}[Content]") && !tail.trim_start().starts_with("]") {
        return fail("source", body.trim());
    }
    // A range of a sheet is written with its A1 text in a comment after the table.
    if let Some(open) = tail.find("/*") {
        let range = tail[open + 2..].split("*/").next().unwrap_or("").trim().to_string();
        if !range.is_empty() {
            return Ok(Source::Range { sheet: name, range });
        }
    }
    Ok(Source::Table(name))
}

/// The query whose M code is `code`; its name and the sheet it was loaded to are `base`'s.
pub fn m_to_query(code: &str, base: &Query) -> Result<Query, CodeError> {
    let text = code.trim();
    let Some(rest) = text.strip_prefix("let") else { return fail("syntax", "let") };
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return fail("syntax", "let");
    }
    // The first `in` outside strings, brackets and comments.
    let b = rest.as_bytes();
    let (mut depth, mut i, mut at) = (0i32, 0, None);
    while i < b.len() {
        match b[i] {
            b'"' => match string_end(b, i) {
                Some(e) => i = e,
                None => return fail("syntax", "\""),
            },
            b'/' => {
                if let Some(e) = comment_end(b, i) {
                    i = e;
                    continue;
                }
            }
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            b'i' if depth == 0
                && b.get(i + 1) == Some(&b'n')
                && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] == b'.'))
                && b.get(i + 2).is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_')) =>
            {
                at = Some(i);
                break;
            }
            _ => {}
        }
        i += 1;
    }
    let Some(at) = at else { return fail("syntax", "in") };
    let (bindings, result) = (&rest[..at], rest[at + 2..].trim());
    let mut names: Vec<String> = Vec::new();
    let mut source = None;
    let mut steps = Vec::new();
    for binding in split_top(bindings) {
        // Comments before the name are skipped; the first body keeps its own (a range is written in one).
        let mut binding = binding.trim_start();
        while let Some(e) = comment_end(binding.as_bytes(), 0) {
            binding = binding[e..].trim_start();
        }
        // `name = body`: the first `=` outside strings.
        let bb = binding.as_bytes();
        let mut k = 0;
        let mut eq = None;
        while k < bb.len() {
            if bb[k] == b'"' {
                match string_end(bb, k) {
                    Some(e) => k = e + 1,
                    None => return fail("syntax", binding.trim()),
                }
                continue;
            }
            if bb[k] == b'=' {
                eq = Some(k);
                break;
            }
            k += 1;
        }
        let Some(eq) = eq else { return fail("syntax", binding.trim()) };
        let Some(name) = parse_id(&binding[..eq]) else { return fail("syntax", binding[..eq].trim()) };
        let body = binding[eq + 1..].trim();
        if names.is_empty() {
            source = Some(source_of(body)?);
        } else {
            let prev = names.last().cloned().unwrap_or_default();
            steps.push(step_of(&strip_comments(body), &prev).map_err(|mut e| {
                if e.key == "syntax" || e.key == "function" || e.key == "arguments" {
                    e.detail = format!("{name}: {}", e.detail);
                }
                e
            })?);
        }
        if names.contains(&name) {
            return fail("syntax", name);
        }
        names.push(name);
    }
    let Some(source) = source else { return fail("syntax", "Source") };
    match parse_id(result) {
        Some(last) if names.last() == Some(&last) => {}
        _ => return fail("result", result),
    }
    Ok(Query { name: base.name.clone(), source, steps, output: base.output.clone() })
}
