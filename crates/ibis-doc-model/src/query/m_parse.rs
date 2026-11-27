// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The M code of a query read back into its steps (GitHub issue #173: the 詳細エディター lets the code be edited).
//! It reads the code `query_to_m` writes (`let`, a chain of steps each made from the one before, `in` the last one): the
//! functions and the argument shapes of `step_m`, with spaces, line breaks and comments free. Code that uses anything else
//! is refused with the reason; the query stays as it was.

use super::{Agg, Clause, Cond, Culture, DateFilter, DatePart, DateUnit, DateWhen, JoinKind, Missing, Op, QType, Query, SplitBy, Source, Step, Transform};

/// Why code was refused: a key of the screen's text table (`query.code-error.<key>`) and the text it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeError {
    pub key: &'static str,
    pub detail: String,
    /// The line of the code (1 = the first) where it cannot be read; 0 when no line is known.
    pub line: usize,
}

fn fail<T>(key: &'static str, detail: impl Into<String>) -> Result<T, CodeError> {
    Err(CodeError { key, detail: detail.into(), line: 0 })
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

pub(super) fn unquote(s: &str) -> Option<String> {
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

/// The pairs of a list of pairs `{{a, b}, {c, d}}`; one pair may be written bare, `{a, b}` (M accepts both: the list of one pair).
fn pairs_or_bare(s: &str) -> Option<Vec<Vec<&str>>> {
    let list = items(s)?;
    if list.iter().all(|x| x.trim_start().starts_with('{')) && !list.is_empty() {
        return list.into_iter().map(items).collect();
    }
    Some(vec![list])
}

/// A text or a number written in a step, as its value.
fn value_of(s: &str) -> Option<String> {
    let s = s.trim();
    if let Some(t) = unquote(s) {
        return Some(t);
    }
    if s.starts_with("#date(") || s.starts_with("#datetime(") {
        // Kept as written (spaces after the commas), so that the code reads the same when written back.
        super::cond::date_literal(s)?;
        let (name, args) = s.split_once('(')?;
        let parts: Vec<&str> = args.strip_suffix(')')?.split(',').map(str::trim).collect();
        return Some(format!("{name}({})", parts.join(", ")));
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

/// A column read in a row: `[Name]` (any text without brackets), or `[#"Any name"]`.
fn column_of(s: &str) -> Option<String> {
    let inner = s.trim().strip_prefix('[')?.strip_suffix(']')?.trim();
    if let Some(quoted) = inner.strip_prefix('#') {
        return unquote(quoted);
    }
    (!inner.is_empty() && !inner.contains(['[', ']', '"'])).then(|| inner.to_string())
}

/// The text inside a pair of brackets that opens the text and closes it at its very end.
fn outer_parens(s: &str) -> Option<&str> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.first() != Some(&b'(') || b.last() != Some(&b')') {
        return None;
    }
    let (mut depth, mut i) = (0i32, 0);
    while i < b.len() {
        match b[i] {
            b'"' => i = string_end(b, i)?,
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return (i == b.len() - 1).then(|| &s[1..i]);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `left op right` cut at the first comparison outside strings and brackets.
fn comparison(s: &str) -> Option<(&str, &'static str, &str)> {
    let b = s.as_bytes();
    let (mut depth, mut i) = (0i32, 0);
    while i < b.len() {
        match b[i] {
            b'"' => i = string_end(b, i)?,
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            b'<' | b'>' | b'=' if depth == 0 => {
                let two = &s[i..(i + 2).min(s.len())];
                let (op, len) = match two {
                    "<>" => ("<>", 2),
                    ">=" => (">=", 2),
                    "<=" => ("<=", 2),
                    _ => (
                        match b[i] {
                            b'<' => "<",
                            b'>' => ">",
                            _ => "=",
                        },
                        1,
                    ),
                };
                return Some((s[..i].trim(), op, s[i + len..].trim()));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `s` cut at the word (`and`, `or`) between spaces outside strings and brackets.
fn split_word<'a>(s: &'a str, word: &str) -> Vec<&'a str> {
    let b = s.as_bytes();
    let (mut out, mut depth, mut start, mut i) = (Vec::new(), 0i32, 0, 0);
    while i < b.len() {
        match b[i] {
            b'"' => match string_end(b, i) {
                Some(e) => i = e,
                None => break,
            },
            b'(' | b'{' | b'[' => depth += 1,
            b')' | b'}' | b']' => depth -= 1,
            c if depth == 0 && c.is_ascii_whitespace() => {
                let rest = &s[i + 1..];
                if rest.starts_with(word) && rest[word.len()..].starts_with(|c: char| c.is_whitespace() || c == '(') {
                    out.push(&s[start..i]);
                    start = i + 1 + word.len();
                    i = start;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
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

/// The inverse of the comparisons `step_m` writes.
fn condition(s: &str) -> Option<(String, Op, String)> {
    let mut s = s.trim();
    while let Some(inner) = outer_parens(s) {
        s = inner.trim();
    }
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
            ("Text.StartsWith", true) => Op::DoesNotBeginWith,
            ("Text.EndsWith", false) => Op::EndsWith,
            ("Text.EndsWith", true) => Op::DoesNotEndWith,
            _ => return None,
        };
        if parts.len() != 2 {
            return None;
        }
        return Some((column_of(parts[0])?, op, unquote(parts[1])?));
    }
    if negated {
        return None;
    }
    let (left, symbol, value) = comparison(body)?;
    let column = column_of(left)?;
    let op = match symbol {
        "<>" => Op::NotEquals,
        ">=" => Op::GreaterOrEqual,
        "<=" => Op::LessOrEqual,
        "=" => Op::Equals,
        ">" => Op::Greater,
        _ => Op::Less,
    };
    if value == "null" {
        return match op {
            Op::Equals => Some((column, Op::IsNull, String::new())),
            Op::NotEquals => Some((column, Op::IsNotNull, String::new())),
            _ => None,
        };
    }
    Some((column, op, value_of(value)?))
}

/// A window of dates (`Date.IsInPreviousNDays([d], 7)`, `Date.IsInCurrentMonth([d])`) as the column and the window.
fn date_window(name: &str, args: &[&str]) -> Option<(String, DateFilter)> {
    let rest = name.strip_prefix("Date.IsIn")?;
    let column = column_of(args.first()?)?;
    if rest == "YearToDate" {
        return (args.len() == 1).then_some((column, DateFilter { when: DateWhen::YearToDate, unit: DateUnit::Year, count: None }));
    }
    for (prefix, when) in [("Current", DateWhen::Current), ("Previous", DateWhen::Previous), ("Next", DateWhen::Next)] {
        let Some(tail) = rest.strip_prefix(prefix) else { continue };
        for unit in DateUnit::ALL {
            if tail == unit.m_name() && args.len() == 1 {
                return Some((column, DateFilter { when, unit, count: None }));
            }
            // `PreviousNDays`: N units, the count the second argument.
            if when != DateWhen::Current && tail.strip_prefix('N').and_then(|t| t.strip_suffix('s')) == Some(unit.m_name()) && args.len() == 2 {
                let count: u32 = number(args[1])?;
                return Some((column, DateFilter { when, unit, count: Some(count) }));
            }
        }
    }
    None
}

/// A row condition: comparisons joined by `and` / `or` (brackets as in M: `and` before `or`), `not`, a list of values the column is in
/// (`List.Contains({1, 2}, [a])`), a part of a date compared (`Date.Year([d]) = 2024`), a window of dates.
pub(super) fn cond_expr(s: &str) -> Option<Cond> {
    let s = s.trim();
    // Brackets round the whole are kept (`Cond::Group`): Excel writes the checkboxes' choices so.
    if let Some(inner) = outer_parens(s) {
        return cond_expr(inner).map(|c| Cond::Group(Box::new(c)));
    }
    let ors = split_word(s, "or");
    if ors.len() > 1 {
        return Some(Cond::Or(ors.into_iter().map(cond_expr).collect::<Option<_>>()?));
    }
    let ands = split_word(s, "and");
    if ands.len() > 1 {
        return Some(Cond::And(ands.into_iter().map(cond_expr).collect::<Option<_>>()?));
    }
    if let Some((column, op, value)) = condition(s) {
        return Some(Cond::Leaf { column, op, value });
    }
    if let Some((name, args)) = call(s) {
        let parts = split_top(args);
        if name == "List.Contains" && parts.len() == 2 {
            let column = column_of(parts[1])?;
            let values = items(parts[0])?.into_iter().map(value_of).collect::<Option<Vec<_>>>()?;
            let mut leaves: Vec<Cond> = values.into_iter().map(|value| Cond::Leaf { column: column.clone(), op: Op::Equals, value }).collect();
            return match leaves.len() {
                0 => None,
                1 => leaves.pop(),
                _ => Some(Cond::Or(leaves)),
            };
        }
        if let Some((column, filter)) = date_window(name, &parts) {
            return Some(Cond::Date { column, filter });
        }
    }
    if let Some((left, symbol, value)) = comparison(s) {
        let (name, args) = call(left)?;
        let part = DatePart::ALL.into_iter().find(|p| p.m_name() == name)?;
        let op = match symbol {
            "<>" => Op::NotEquals,
            ">=" => Op::GreaterOrEqual,
            "<=" => Op::LessOrEqual,
            "=" => Op::Equals,
            ">" => Op::Greater,
            _ => Op::Less,
        };
        let value = value.trim();
        if value.parse::<f64>().is_err() {
            return None;
        }
        return Some(Cond::Part { column: column_of(args)?, part, op, value: value.to_string() });
    }
    let rest = s.strip_prefix("not")?;
    // `not (…)` is the `not` of what is inside (the brackets are the ones `not` takes, not a group).
    let inner = match outer_parens(rest) {
        Some(inner) => inner,
        None => rest,
    };
    rest.starts_with(|c: char| c.is_whitespace() || c == '(').then(|| cond_expr(inner).map(|c| Cond::Not(Box::new(c)))).flatten()
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

/// The options of `Table.TransformColumnTypes`: a culture text, or a record with `Culture` and / or `MissingField`.
fn culture_arg(s: &str) -> Option<(Culture, Missing)> {
    let s = s.trim();
    let (mut name, mut missing) = (None, Missing::Error);
    match unquote(s) {
        Some(c) => name = Some(c),
        None => {
            let inner = s.strip_prefix('[')?.strip_suffix(']')?;
            for field in split_top(inner) {
                let (key, value) = field.split_once('=')?;
                match key.trim() {
                    "Culture" => name = Some(unquote(value)?),
                    "MissingField" => missing = Missing::from_m(value)?,
                    _ => return None,
                }
            }
        }
    }
    let culture = match name {
        Some(n) => Culture::ALL.into_iter().find(|k| k.name() == n)?,
        None => Culture::Invariant,
    };
    Some((culture, missing))
}

/// `step`, as it is when columns that are missing are an error, else held in `WithMissing`.
fn with_missing(step: Step, missing: Missing) -> Step {
    if missing == Missing::Error { step } else { Step::WithMissing { inner: Box::new(step), missing } }
}

/// The columns named by a list, or by one text.
fn column_names(s: &str) -> Option<Vec<String>> {
    strings(s).or_else(|| unquote(s).map(|c| vec![c]))
}

/// A list of whole numbers `{0, 3}`.
fn numbers(s: &str) -> Option<Vec<usize>> {
    items(s)?.into_iter().map(number).collect()
}

/// An optional logical argument (`startAtEnd`): absent or `null` is false.
fn flag(s: Option<&&str>) -> Option<bool> {
    match s.map(|x| x.trim()) {
        None | Some("null") | Some("false") => Some(false),
        Some("true") => Some(true),
        _ => None,
    }
}

/// What is wrong in the arguments of a call that is known but cannot be read, when it can be told.
fn hint(name: &str, a: &[&str]) -> String {
    match name {
        "Table.TransformColumnTypes" => {
            if let Some(list) = a.get(1).and_then(|x| pairs_or_bare(x)) {
                for pair in list {
                    if let [_, ty] = &pair[..] {
                        if QType::from_m(ty).is_none() {
                            return format!(" (type {})", ty.trim());
                        }
                    }
                }
            }
            String::new()
        }
        "Table.SelectRows" => match a.get(1).and_then(|x| x.trim().strip_prefix("each ")) {
            Some(text) => format!(" (condition {})", text.trim()),
            None => String::new(),
        },
        // A column of tables (the rows of each group) cannot be held in a cell.
        "Table.Group" if a.get(2).is_some_and(|x| x.contains("each _")) => " (all rows: a column of tables)".to_string(),
        _ => String::new(),
    }
}

/// `Table.NestedJoin(prev, left keys, right, right keys, "nested column", JoinKind.X)` as the right table, the keys, the kind and the
/// name of the nested column.
fn nested_join(body: &str, prev: &str) -> Option<(String, Vec<String>, Vec<String>, JoinKind, String)> {
    let (join, inner) = call(body)?;
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
    Some((parse_id(j[2])?, strings(j[1])?, strings(j[3])?, kind, unquote(j[4])?))
}

/// The columns `Table.ExpandTableColumn(table, "right", {columns}[, {new names}])` takes out of the nested table `right`; the new names
/// Power Query writes (`Right.Column`) are the ones this program gives, other names are refused.
fn expand_columns(a: &[&str], right: &str) -> Option<Vec<String>> {
    let columns = strings(a.get(2)?)?;
    if let Some(names) = a.get(3) {
        let names = strings(names)?;
        if names.len() != columns.len() || names.iter().zip(&columns).any(|(n, c)| *n != format!("{right}.{c}")) {
            return None;
        }
    }
    Some(columns)
}

/// The steps the call `body` makes from the step `prev` (one, except when a call does the same to several columns).
fn step_of(body: &str, prev: &str) -> Result<Vec<Step>, CodeError> {
    let Some((name, args)) = call(body) else { return fail("syntax", body.trim()) };
    let a = split_top(args);
    let bad = || CodeError { key: "arguments", detail: format!("{name}{}", hint(name, &a)), line: 0 };
    if let Some(steps) = (|| -> Option<Vec<Step>> {
        let n = a.len();
        let first = a.first().is_some_and(|x| is_prev(x, prev));
        match name {
            // Several columns at once: Power Query writes one call for all of them; each column is a step here (they do not depend on
            // one another).
            "Table.TransformColumnTypes" if first && (n == 2 || n == 3) => {
                let (culture, missing) = match a.get(2) {
                    Some(c) => culture_arg(c)?,
                    None => (Culture::Invariant, Missing::Error),
                };
                let mut out = Vec::new();
                for pair in pairs_or_bare(a[1])? {
                    let [column, ty] = &pair[..] else { return None };
                    let ty = QType::from_m(ty)?;
                    out.push(with_missing(Step::ChangeType { column: unquote(column)?, ty, culture, apart: false }, missing));
                }
                (!out.is_empty()).then_some(out)
            }
            // Several columns, each its own function: one step for each (the same function on all of them is the step of that function).
            "Table.TransformColumns" if first && n == 2 => {
                let mut done: Vec<(String, Transform)> = Vec::new();
                for pair in pairs(a[1])? {
                    let (column, function) = match &pair[..] {
                        [column, function] | [column, function, _] => (column, function),
                        _ => return None,
                    };
                    done.push((unquote(column)?, Transform::from_m(function)?));
                }
                let (_, f) = done.first()?;
                if done.iter().all(|(_, g)| g == f) && matches!(f, Transform::Trim | Transform::Upper | Transform::Lower) {
                    let columns = done.iter().map(|(c, _)| c.clone()).collect();
                    return Some(vec![match f {
                        Transform::Trim => Step::Trim(columns),
                        Transform::Upper => Step::Upper(columns),
                        _ => Step::Lower(columns),
                    }]);
                }
                Some(done.into_iter().map(|(column, function)| Step::Transform { column, function }).collect())
            }
            "Table.ReplaceValue" if first && n == 5 && a[1].trim() != "null" => {
                let whole_cell = match a[3].trim() {
                    "Replacer.ReplaceValue" => true,
                    "Replacer.ReplaceText" => false,
                    _ => return None,
                };
                let columns = strings(a[4])?;
                let (find, with) = (value_of(a[1])?, value_of(a[2])?);
                (!columns.is_empty()).then(|| columns.into_iter().map(|column| Step::ReplaceValue { column, find: find.clone(), with: with.clone(), whole_cell }).collect())
            }
            _ => None,
        }
    })() {
        return Ok(steps);
    }
    let step = (|| -> Option<Step> {
        // Every step but a few is made from the step before it, as its first argument.
        let n = a.len();
        let first = a.first().is_some_and(|x| is_prev(x, prev));
        Some(match name {
            "Table.PromoteHeaders" if first && n == 2 => Step::PromoteHeaders,
            "Table.DemoteHeaders" if first && n == 1 => Step::DemoteHeaders,
            "Table.SelectColumns" if first && (n == 2 || n == 3) => with_missing(Step::SelectColumns(column_names(a[1])?), missing_arg(&a, 2)?),
            "Table.RemoveColumns" if first && (n == 2 || n == 3) => with_missing(Step::RemoveColumns(column_names(a[1])?), missing_arg(&a, 2)?),
            "Table.RenameColumns" if first && (n == 2 || n == 3) => {
                let mut renames = Vec::new();
                for pair in pairs_or_bare(a[1])? {
                    let [from, to] = &pair[..] else { return None };
                    renames.push((unquote(from)?, unquote(to)?));
                }
                with_missing((!renames.is_empty()).then_some(Step::RenameColumns(renames))?, missing_arg(&a, 2)?)
            }
            "Table.ReorderColumns" if first && (n == 2 || n == 3) => with_missing(Step::ReorderColumns(column_names(a[1])?), missing_arg(&a, 2)?),
            "Table.SelectRows" if first && n == 2 => {
                let text = a[1].trim().strip_prefix("each ")?;
                if text.trim() == "not List.IsEmpty(List.RemoveMatchingItems(Record.FieldValues(_), {\"\", null}))" {
                    Step::RemoveBlankRows
                } else {
                    match cond_expr(text)? {
                        Cond::Leaf { column, op, value } => Step::FilterRows { column, op, value },
                        other => Step::FilterCondition(other),
                    }
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
            "Table.SplitColumn" if first && (n == 3 || n == 4) => {
                // The new column names Power Query writes (`Name.1`, `Name.2`, …) are the ones this program gives; other names are refused.
                let mut columns = 0;
                if let Some(names) = a.get(3) {
                    let column = unquote(a[1])?;
                    let names = strings(names)?;
                    if names.is_empty() || names.iter().enumerate().any(|(k, x)| *x != format!("{column}.{}", k + 1)) {
                        return None;
                    }
                    columns = names.len();
                }
                let (splitter, inner) = call(a[2])?;
                let parts = split_top(inner);
                let by = match splitter {
                    "Splitter.SplitTextByDelimiter" => SplitBy::Delimiter(unquote(parts.first()?)?),
                    "Splitter.SplitTextByRepeatedLengths" if parts.len() == 1 => SplitBy::Length(number(inner)?),
                    "Splitter.SplitTextByAnyDelimiter" => SplitBy::AnyDelimiter(strings(parts.first()?)?),
                    "Splitter.SplitTextByEachDelimiter" => SplitBy::EachDelimiter { delimiters: strings(parts.first()?)?, from_end: flag(parts.get(2))? },
                    // `{0, n}` with the end flag written is what 文字数 with 1 回 writes; the 位置 dialog writes no flag.
                    "Splitter.SplitTextByPositions" if parts.len() == 2 && numbers(parts[0]).is_some_and(|p| p.len() == 2 && p[0] == 0) => {
                        let n = numbers(parts[0])?;
                        SplitBy::Once { length: n[1], from_end: flag(parts.get(1))? }
                    }
                    "Splitter.SplitTextByPositions" => SplitBy::Positions { positions: numbers(parts.first()?)?, from_end: flag(parts.get(1))? },
                    "Splitter.SplitTextByCharacterTransition" if parts.len() == 2 => SplitBy::Transition { from: super::text_m::class_of(parts[0])?, to: super::text_m::class_of(parts[1])? },
                    "Splitter.SplitTextByLengths" => SplitBy::Lengths { lengths: numbers(parts.first()?)?, from_end: flag(parts.get(1))? },
                    _ => return None,
                };
                Step::SplitColumn { column: unquote(a[1])?, by, columns }
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
            "Table.Pivot" if first && n == 5 => Step::Pivot { attribute: unquote(a[2])?, values: unquote(a[3])?, agg: agg_of(a[4])? },
            "Table.Unpivot" if first && n == 4 => Step::Unpivot { columns: strings(a[1])?, attribute: unquote(a[2])?, value: unquote(a[3])? },
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
            "Table.Group" if first && (n == 3 || n == 4) => {
                // `GroupKind.Global` is the default; `GroupKind.Local` groups only the rows that come one after another.
                let local = match a.get(3).map(|x| x.trim()) {
                    None | Some("GroupKind.Global") => false,
                    Some("GroupKind.Local") => true,
                    _ => return None,
                };
                let mut aggregations = Vec::new();
                for pair in pairs(a[2])? {
                    let [name, function, _] = &pair[..] else { return None };
                    let function = function.trim().strip_prefix("each ")?.trim();
                    if function == "Table.RowCount(_)" {
                        aggregations.push((unquote(name)?, Agg::Count, None));
                    } else if let Some(inner) = function.strip_prefix("List.Count(List.Distinct(").and_then(|x| x.strip_suffix("))")) {
                        aggregations.push((unquote(name)?, Agg::CountDistinct, Some(column_of(inner)?)));
                    } else {
                        let (f, column) = call(function)?;
                        aggregations.push((unquote(name)?, agg_of(f)?, Some(column_of(column)?)));
                    }
                }
                let keys = column_names(a[1])?;
                if local { Step::GroupByLocal { keys, aggregations } } else { Step::GroupBy { keys, aggregations } }
            }
            "Table.AlternateRows" if first && n == 4 => Step::AlternateRows { offset: number(a[1])?, skip: number(a[2])?, take: number(a[3])? },
            "Table.Range" if first && (n == 2 || n == 3) => Step::KeepRange { offset: number(a[1])?, count: match a.get(2) {
                Some(c) => Some(number(c)?),
                None => None,
            } },
            "Table.DuplicateColumn" if first && (n == 3 || n == 4) => Step::DuplicateColumn { column: unquote(a[1])?, new_name: unquote(a[2])? },
            "Table.ExpandTableColumn" if n == 3 || (n == 4 && !first) => {
                // The merge written in one call, as this program writes it.
                let (right, left_keys, right_keys, kind, _) = nested_join(a[0], prev)?;
                let expand = expand_columns(&a, &right)?;
                Step::Merge { right, left_keys, right_keys, kind, expand }
            }
            "Table.Combine" if n == 1 => {
                let parts = items(a[0])?;
                let [from, other] = &parts[..] else { return None };
                if !is_prev(from, prev) {
                    return None;
                }
                Step::Append { other: parse_id(other)? }
            }
            "Table.AddColumn" if first && (n == 3 || n == 4) => {
                let formula = a[2].trim().strip_prefix("each ")?.trim();
                let column = unquote(a[1])?;
                // The type after the formula (`Int64.Type`).
                let ty = match a.get(3) {
                    Some(t) => Some(QType::from_m(t)?),
                    None => None,
                };
                match clauses(formula) {
                    Some((clauses, otherwise)) if ty.is_none() => Step::AddConditionalColumn { name: column, clauses, otherwise },
                    _ => Step::AddColumn { name: column, formula: formula.to_string(), ty },
                }
            }
            "Table.AddIndexColumn" if first && n == 5 => Step::AddIndexColumn { name: unquote(a[1])?, start: number(a[2])?, step: number(a[3])? },
            _ => return None,
        })
    })();
    match step {
        Some(step) => Ok(vec![step]),
        None if is_known(name) => Err(bad()),
        None => fail("function", name),
    }
}

/// The `MissingField.*` argument at `at` (the default, an error, when it is not there).
fn missing_arg(a: &[&str], at: usize) -> Option<Missing> {
    match a.get(at) {
        Some(m) => Missing::from_m(m),
        None => Some(Missing::Error),
    }
}

fn is_known(name: &str) -> bool {
    matches!(
        name,
        "Table.PromoteHeaders" | "Table.DemoteHeaders" | "Table.TransformColumnTypes" | "Table.SelectColumns" | "Table.RemoveColumns" | "Table.RenameColumns"
            | "Table.ReorderColumns" | "Table.SelectRows" | "Table.FirstN" | "Table.LastN" | "Table.Skip" | "Table.RemoveLastN" | "Table.Distinct"
            | "Table.RemoveRowsWithErrors" | "Table.ReplaceErrorValues" | "Table.Sort" | "Table.SplitColumn" | "Table.CombineColumns" | "Table.ReplaceValue"
            | "Table.FillDown" | "Table.FillUp" | "Table.Transpose" | "Table.ReverseRows" | "Table.TransformColumns" | "Table.Pivot" | "Table.UnpivotOtherColumns"
            | "Table.Group" | "Table.ExpandTableColumn" | "Table.Combine" | "Table.AddColumn" | "Table.AddIndexColumn" | "Table.Unpivot" | "Table.NestedJoin"
            | "Table.AlternateRows" | "Table.Range" | "Table.DuplicateColumn"
    )
}

/// `Csv.Document(File.Contents("path"),[Delimiter=",", Columns=7, Encoding=65001, QuoteStyle=QuoteStyle.None])`.
fn csv_source(body: &str) -> Option<Source> {
    let (name, args) = call(body)?;
    if name != "Csv.Document" {
        return None;
    }
    let a = split_top(args);
    let [file, options] = &a[..] else { return None };
    let (function, path) = call(file)?;
    if function != "File.Contents" {
        return None;
    }
    let (mut delimiter, mut columns, mut encoding, mut quote) = (",".to_string(), 0, 65001, "None".to_string());
    for field in split_top(options.trim().strip_prefix('[')?.strip_suffix(']')?) {
        let (key, value) = field.split_once('=')?;
        match key.trim() {
            "Delimiter" => delimiter = unquote(value)?,
            "Columns" => columns = number(value)?,
            "Encoding" => encoding = number(value)?,
            "QuoteStyle" => quote = value.trim().strip_prefix("QuoteStyle.")?.to_string(),
            _ => return None,
        }
    }
    Some(Source::Csv { path: unquote(path)?, delimiter, columns, encoding, quote })
}

/// `Excel.Workbook(File.Contents("path"), null, true)`; the item is told by the step after it (`workbook_item`).
fn workbook_source(body: &str) -> Option<Source> {
    let (name, args) = call(body)?;
    if name != "Excel.Workbook" {
        return None;
    }
    let a = split_top(args);
    let [file, null, lazy] = &a[..] else { return None };
    let (function, path) = call(file)?;
    (function == "File.Contents" && null.trim() == "null" && lazy.trim() == "true").then(|| Source::Workbook { path: unquote(path).unwrap_or_default(), item: String::new(), sheet: false })
}

/// `Source{[Item="Data",Kind="Sheet"]}[Data]`: the item and whether it is a sheet (else a table).
fn workbook_item(body: &str, source: &str) -> Option<(String, bool)> {
    let rest = body.trim();
    let rest = rest.strip_prefix(source).or_else(|| rest.strip_prefix(format!("#{}", super::text_m::q(source)).as_str()))?;
    let inner = rest.trim().strip_prefix("{[")?.strip_suffix("[Data]")?.trim_end().strip_suffix("]}")?;
    let (mut item, mut kind) = (None, None);
    for field in split_top(inner) {
        let (key, value) = field.split_once('=')?;
        match key.trim() {
            "Item" => item = Some(unquote(value)?),
            "Kind" => kind = Some(unquote(value)?),
            _ => return None,
        }
    }
    let sheet = match kind?.as_str() {
        "Sheet" => true,
        "Table" => false,
        _ => return None,
    };
    Some((item?, sheet))
}

fn source_of(body: &str) -> Result<Source, CodeError> {
    let compact: String = body.split_whitespace().collect();
    if compact == "#table({},{})" {
        return Ok(Source::Blank);
    }
    if let Some(source) = csv_source(strip_comments(body).trim()).or_else(|| workbook_source(strip_comments(body).trim())) {
        return Ok(source);
    }
    let b = body.as_bytes();
    // `{[Name="T"]}`, with spaces allowed inside.
    let Some(at) = body.find("{[") else { return fail("source", body.trim()) };
    let after = body[at + 2..].trim_start();
    let Some(after) = after.strip_prefix("Name").map(str::trim_start).and_then(|x| x.strip_prefix('=')) else { return fail("source", body.trim()) };
    let eq_end = body.len() - after.len();
    let start = body[eq_end..].find('"').map(|k| eq_end + k);
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
    let Some(rest) = text.strip_prefix("let") else { return Err(CodeError { key: "syntax", detail: "let".into(), line: 1 }) };
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
    // The line (1 = the first) of the text that `part` is a slice of.
    let line_of = |part: &str| code[..(part.as_ptr() as usize).saturating_sub(code.as_ptr() as usize).min(code.len())].matches('\n').count() + 1;
    let at_line = |mut e: CodeError, line: usize| {
        e.line = line;
        e
    };
    // Each binding as its name, its body and its line.
    let mut bound: Vec<(String, String, usize, String)> = Vec::new();
    for binding in split_top(bindings) {
        // Comments before the name are skipped; the first body keeps its own (a range is written in one).
        let mut binding = binding.trim_start();
        while let Some(e) = comment_end(binding.as_bytes(), 0) {
            binding = binding[e..].trim_start();
        }
        let line = line_of(binding);
        // `name = body`: the first `=` outside strings.
        let bb = binding.as_bytes();
        let mut k = 0;
        let mut eq = None;
        while k < bb.len() {
            if bb[k] == b'"' {
                match string_end(bb, k) {
                    Some(e) => k = e + 1,
                    None => return Err(at_line(CodeError { key: "syntax", detail: binding.trim().to_string(), line: 0 }, line)),
                }
                continue;
            }
            if bb[k] == b'=' {
                eq = Some(k);
                break;
            }
            k += 1;
        }
        let Some(eq) = eq else { return Err(at_line(CodeError { key: "syntax", detail: binding.trim().to_string(), line: 0 }, line)) };
        let Some(name) = parse_id(&binding[..eq]) else { return Err(at_line(CodeError { key: "syntax", detail: binding[..eq].trim().to_string(), line: 0 }, line)) };
        bound.push((name, binding[eq + 1..].trim().to_string(), line, binding.trim().to_string()));
    }
    let mut at = 0;
    while at < bound.len() {
        let (name, body, line) = (bound[at].0.clone(), bound[at].1.clone(), bound[at].2);
        let with_line = |e: CodeError| at_line(e, line);
        if names.contains(&name) {
            return Err(with_line(CodeError { key: "syntax", detail: name, line: 0 }));
        }
        if names.is_empty() {
            let mut from = if matches!((body.as_str(), &base.source),
                ("Ibis.ParquetSource()", Source::Parquet { .. })
                | ("Ibis.JsonLinesSource()", Source::JsonLines { .. })
                | ("Ibis.JsonArraySource()", Source::JsonArray { .. })
                | ("Ibis.DelimitedSource()", Source::Delimited { .. })
                | ("Ibis.ParquetFolderSource()", Source::ParquetFolder { .. })
                | ("Ibis.SqliteSource()", Source::Sqlite { .. })
                | ("Ibis.AvroSource()", Source::Avro { .. })
                | ("Ibis.ArrowIpcSource()", Source::ArrowIpc { .. })
            ) {
                base.source.clone()
            } else {
                source_of(&body).map_err(with_line)?
            };
            names.push(name.clone());
            at += 1;
            // A workbook file is followed by the step that picks its sheet or table.
            if let Source::Workbook { item, sheet, .. } = &mut from {
                let Some((nav, nav_body, nav_line, _)) = bound.get(at) else { return Err(with_line(CodeError { key: "source", detail: body.trim().to_string(), line: 0 })) };
                let Some((picked, is_sheet)) = workbook_item(&strip_comments(nav_body), &name) else {
                    return Err(at_line(CodeError { key: "source", detail: nav_body.trim().to_string(), line: 0 }, *nav_line));
                };
                (*item, *sheet) = (picked, is_sheet);
                names.push(nav.clone());
                at += 1;
            }
            source = Some(from);
            continue;
        }
        let prev = names.last().cloned().unwrap_or_default();
        let plain = strip_comments(&body);
        // Power Query writes a merge as two steps: `Table.NestedJoin`, then `Table.ExpandTableColumn` of its nested column.
        if let Some((right, left_keys, right_keys, kind, nested)) = nested_join(&plain, &prev) {
            let next = bound.get(at + 1).map(|(n, b, l, _)| (n.clone(), strip_comments(b), *l));
            let expanded = next.as_ref().and_then(|(_, b, _)| {
                let (function, inner) = call(b)?;
                let x = split_top(inner);
                (function == "Table.ExpandTableColumn" && x.len() >= 3 && is_prev(x[0], &name) && unquote(x[1]).as_deref() == Some(nested.as_str()) && nested == right)
                    .then(|| expand_columns(&x, &right))
                    .flatten()
            });
            let Some(expand) = expanded else {
                return Err(with_line(CodeError { key: "merge", detail: name, line: 0 }));
            };
            names.push(name);
            let (next_name, _, _) = next.unwrap_or_default();
            if names.contains(&next_name) {
                return Err(at_line(CodeError { key: "syntax", detail: next_name, line: 0 }, bound[at + 1].2));
            }
            steps.push(Step::Merge { right, left_keys, right_keys, kind, expand });
            names.push(next_name);
            at += 2;
            continue;
        }
        let made = match step_of(&plain, &prev) {
            Ok(made) => made,
            // Anything the steps cannot hold (a function they do not have, arguments of another shape, a value that is not a step): from this binding on the code is kept as written and run by the M
            // interpreter (K0039), if it can at least be read as M.
            Err(_) => {
                let tail = bound[at..].iter().map(|b| b.3.as_str()).collect::<Vec<_>>().join(",\n    ");
                if let Err(err) = ibis_m_lang::check_syntax(&format!("let\n{tail}\nin\n    {result}")) {
                    let mut e = with_line(CodeError { key: "syntax", detail: format!("{name}: {}", err.message), line: 0 });
                    e.line = line + (err.line as usize).saturating_sub(2);
                    return Err(e);
                }
                let Some(source) = source else { return fail("syntax", "Source") };
                steps.push(Step::Code { names, bindings: tail, result: result.to_string() });
                return Ok(Query { name: base.name.clone(), source, steps, output: base.output.clone() });
            }
        };
        // A type change that Excel would have written into the step before is marked as a step of its own.
        let mut made = made;
        for k in 0..made.len() {
            let apart = match &made[k] {
                Step::ChangeType { column, culture, .. } if k == 0 => super::text_m::joins_type_group(&steps, column, *culture),
                Step::ChangeType { column, .. } => made[..k].iter().any(|m| matches!(m, Step::ChangeType { column: c, .. } if c == column)),
                _ => continue,
            };
            if let Step::ChangeType { apart: a, .. } = &mut made[k] {
                *a = apart;
            }
        }
        steps.extend(made);
        names.push(name);
        at += 1;
    }
    let Some(source) = source else { return fail("syntax", "Source") };
    match parse_id(result) {
        Some(last) if names.last() == Some(&last) => {}
        _ => return Err(CodeError { key: "result", detail: result.to_string(), line: line_of(result) }),
    }
    Ok(Query { name: base.name.clone(), source, steps, output: base.output.clone() })
}
