// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The queries of a workbook as text, to keep in the file (`xl/ibisQueries.txt`):
//! one line a query header or a step, fields separated by tabs.
use super::{Agg, CharClass, Clause, Culture, JoinKind, Missing, Op, Parameter, QType, Query, Source, SplitBy, Step, Transform};

/// Separators the text can hold in XML (the control characters cannot be written there): `␟` between the items
/// of a list and `␞` between the parts of an item.
const LIST: char = '\u{241f}';
const PART: char = '\u{241e}';

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            LIST => out.push_str("\\u"),
            PART => out.push_str("\\v"),
            c => out.push(c),
        }
    }
    out
}

fn unesc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('u') => out.push(LIST),
                Some('v') => out.push(PART),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn list(items: &[String]) -> String {
    items.iter().map(|s| esc(s)).collect::<Vec<_>>().join(&LIST.to_string())
}

fn unlist(s: &str) -> Vec<String> {
    if s.is_empty() { Vec::new() } else { s.split(LIST).map(unesc).collect() }
}

fn op_name(o: Op) -> &'static str {
    match o {
        Op::Equals => "eq",
        Op::NotEquals => "ne",
        Op::Greater => "gt",
        Op::GreaterOrEqual => "ge",
        Op::Less => "lt",
        Op::LessOrEqual => "le",
        Op::Contains => "contains",
        Op::DoesNotContain => "notcontains",
        Op::BeginsWith => "begins",
        Op::EndsWith => "ends",
        Op::IsNull => "null",
        Op::IsNotNull => "notnull",
        Op::DoesNotBeginWith => "notbegins",
        Op::DoesNotEndWith => "notends",
    }
}

fn op_of(s: &str) -> Option<Op> {
    Op::ALL.into_iter().find(|o| op_name(*o) == s)
}

fn agg_name(a: Agg) -> &'static str {
    match a {
        Agg::Sum => "sum",
        Agg::Average => "avg",
        Agg::Min => "min",
        Agg::Max => "max",
        Agg::Count => "count",
        Agg::CountDistinct => "distinct",
        Agg::Median => "median",
        Agg::First => "first",
    }
}

fn agg_of(s: &str) -> Option<Agg> {
    Agg::ALL.into_iter().find(|a| agg_name(*a) == s)
}

fn join_name(k: JoinKind) -> &'static str {
    match k {
        JoinKind::LeftOuter => "left",
        JoinKind::RightOuter => "right",
        JoinKind::FullOuter => "full",
        JoinKind::Inner => "inner",
        JoinKind::LeftAnti => "leftanti",
        JoinKind::RightAnti => "rightanti",
    }
}

fn join_of(s: &str) -> Option<JoinKind> {
    JoinKind::ALL.into_iter().find(|k| join_name(*k) == s)
}

fn class_name(c: CharClass) -> char {
    match c {
        CharClass::Lower => 'l',
        CharClass::Upper => 'u',
        CharClass::Digit => 'd',
        CharClass::NonDigit => 'n',
    }
}

fn class_of(c: char) -> Option<CharClass> {
    Some(match c {
        'l' => CharClass::Lower,
        'u' => CharClass::Upper,
        'd' => CharClass::Digit,
        'n' => CharClass::NonDigit,
        _ => return None,
    })
}

fn type_name(t: QType) -> &'static str {
    match t {
        QType::Any => "any",
        QType::Text => "text",
        QType::Whole => "whole",
        QType::Decimal => "decimal",
        QType::Date => "date",
        QType::DateTime => "datetime",
        QType::Time => "time",
        QType::Logical => "logical",
        QType::Currency => "currency",
        QType::Percentage => "percentage",
        QType::Int32 => "int32",
        QType::DateTimeZone => "datetimezone",
        QType::Duration => "duration",
    }
}

fn type_of(s: &str) -> Option<QType> {
    QType::ALL.into_iter().find(|t| type_name(*t) == s)
}

fn culture_of(s: &str) -> Culture {
    Culture::ALL.into_iter().find(|c| c.name() == s).unwrap_or_default()
}

fn clauses(c: &[Clause]) -> String {
    c.iter().map(|c| format!("{}\u{241e}{}\u{241e}{}\u{241e}{}", esc(&c.column), op_name(c.op), esc(&c.value), esc(&c.output))).collect::<Vec<_>>().join(&LIST.to_string())
}

fn unclauses(s: &str) -> Option<Vec<Clause>> {
    if s.is_empty() {
        return Some(Vec::new());
    }
    s.split(LIST)
        .map(|part| {
            let f: Vec<&str> = part.split('\u{241e}').collect();
            (f.len() == 4).then(|| Some(Clause { column: unesc(f[0]), op: op_of(f[1])?, value: unesc(f[2]), output: unesc(f[3]) })).flatten()
        })
        .collect()
}

fn pairs(p: &[(String, bool)]) -> String {
    p.iter().map(|(c, asc)| format!("{}\u{241e}{}", esc(c), if *asc { 1 } else { 0 })).collect::<Vec<_>>().join(&LIST.to_string())
}

fn aggregations(a: &[(String, Agg, Option<String>)]) -> String {
    a.iter().map(|(n, g, c)| format!("{}\u{241e}{}\u{241e}{}", esc(n), agg_name(*g), c.as_deref().map(esc).unwrap_or_default())).collect::<Vec<_>>().join(&LIST.to_string())
}

fn numbers(v: &[usize]) -> String {
    v.iter().map(usize::to_string).collect::<Vec<_>>().join(",")
}

fn unnumbers(s: &str) -> Option<Vec<usize>> {
    if s.is_empty() { Some(Vec::new()) } else { s.split(',').map(|x| x.parse().ok()).collect() }
}

fn aggregations_of(f: &str) -> Option<Vec<(String, Agg, Option<String>)>> {
    if f.is_empty() {
        return Some(Vec::new());
    }
    f.split(LIST)
        .map(|p| {
            let parts: Vec<&str> = p.split('\u{241e}').collect();
            (parts.len() == 3).then(|| Some((unesc(parts[0]), agg_of(parts[1])?, if parts[2].is_empty() { None } else { Some(unesc(parts[2])) }))).flatten()
        })
        .collect()
}

fn step_fields(step: &Step) -> Vec<String> {
    let s = |x: &str| esc(x);
    match step {
        Step::PromoteHeaders => vec!["promote".into()],
        Step::DemoteHeaders => vec!["demote".into()],
        Step::ChangeType { column, ty, culture, apart } => vec!["type".into(), s(column), type_name(*ty).into(), s(culture.name()), if *apart { "1" } else { "0" }.into()],
        Step::SelectColumns(c) => vec!["select".into(), list(c)],
        Step::RemoveColumns(c) => vec!["remove".into(), list(c)],
        Step::RenameColumns(renames) => std::iter::once("rename".to_string()).chain(renames.iter().flat_map(|(from, to)| [s(from), s(to)])).collect(),
        Step::ReorderColumns(c) => vec!["reorder".into(), list(c)],
        Step::FilterRows { column, op, value } => vec!["filter".into(), s(column), op_name(*op).into(), s(value)],
        Step::KeepTopRows(n) => vec!["keeptop".into(), n.to_string()],
        Step::KeepBottomRows(n) => vec!["keepbottom".into(), n.to_string()],
        Step::RemoveTopRows(n) => vec!["removetop".into(), n.to_string()],
        Step::RemoveBottomRows(n) => vec!["removebottom".into(), n.to_string()],
        Step::RemoveDuplicates(c) => vec!["distinct".into(), list(c)],
        Step::RemoveBlankRows => vec!["removeblank".into()],
        Step::RemoveErrors(c) => vec!["removeerrors".into(), list(c)],
        Step::ReplaceErrors { columns, with } => vec!["replaceerrors".into(), list(columns), s(with)],
        Step::Sort(k) => vec!["sort".into(), pairs(k)],
        Step::SplitColumn { column, by, columns } => {
            let mut f = match by {
                SplitBy::Delimiter(d) => vec!["split".into(), s(column), "delimiter".into(), s(d)],
                SplitBy::Length(n) => vec!["split".into(), s(column), "length".into(), n.to_string()],
                SplitBy::AnyDelimiter(ds) => vec!["split".into(), s(column), "any".into(), list(ds)],
                SplitBy::EachDelimiter { delimiters, from_end } => vec!["split".into(), s(column), if *from_end { "eachend" } else { "each" }.into(), list(delimiters)],
                SplitBy::Positions { positions, from_end } => vec!["split".into(), s(column), if *from_end { "positionsend" } else { "positions" }.into(), numbers(positions)],
                SplitBy::Lengths { lengths, from_end } => vec!["split".into(), s(column), if *from_end { "lengthsend" } else { "lengths" }.into(), numbers(lengths)],
                SplitBy::Once { length, from_end } => vec!["split".into(), s(column), if *from_end { "onceend" } else { "once" }.into(), length.to_string()],
                SplitBy::Transition { from, to } => vec!["split".into(), s(column), "transition".into(), format!("{}{}", class_name(*from), class_name(*to))],
            };
            // The number of columns it makes, when Power Query wrote their names.
            f.push(columns.to_string());
            f
        }
        Step::MergeColumns { columns, separator, name } => vec!["mergecolumns".into(), list(columns), s(separator), s(name)],
        Step::ReplaceValue { column, find, with, whole_cell } => vec!["replace".into(), s(column), s(find), s(with), if *whole_cell { "1" } else { "0" }.into()],
        Step::ReplaceNull { columns, with } => vec!["replacenull".into(), list(columns), s(with)],
        Step::FillDown(c) => vec!["filldown".into(), list(c)],
        Step::FillUp(c) => vec!["fillup".into(), list(c)],
        Step::Transpose => vec!["transpose".into()],
        Step::ReverseRows => vec!["reverse".into()],
        Step::Trim(c) => vec!["trim".into(), list(c)],
        Step::Upper(c) => vec!["upper".into(), list(c)],
        Step::Lower(c) => vec!["lower".into(), list(c)],
        Step::Pivot { attribute, values, agg } => vec!["pivot".into(), s(attribute), s(values), agg_name(*agg).into()],
        Step::Unpivot { columns, attribute, value } => vec!["unpivot".into(), list(columns), s(attribute), s(value)],
        Step::UnpivotOther { keep, attribute, value } => vec!["unpivotother".into(), list(keep), s(attribute), s(value)],
        Step::GroupBy { keys, aggregations: a } => vec!["group".into(), list(keys), aggregations(a)],
        Step::Merge { right, left_keys, right_keys, kind, expand } => vec!["merge".into(), s(right), list(left_keys), list(right_keys), join_name(*kind).into(), list(expand)],
        Step::Append { other } => vec!["append".into(), s(other)],
        Step::AddColumn { name, formula, ty } => vec!["addcolumn".into(), s(name), s(formula), ty.map(type_name).unwrap_or_default().into()],
        Step::AddConditionalColumn { name, clauses: c, otherwise } => vec!["addconditional".into(), s(name), clauses(c), s(otherwise)],
        Step::AddIndexColumn { name, start, step } => vec!["addindex".into(), s(name), start.to_string(), step.to_string()],
        // The condition as its M text.
        Step::FilterCondition(cond) => vec!["filtercond".into(), s(&super::text_m::cond_text(cond))],
        Step::WithMissing { inner, missing } => {
            let mut f = vec!["missing".into(), if *missing == Missing::Ignore { "ignore" } else { "usenull" }.into()];
            f.extend(step_fields(inner));
            f
        }
        Step::Transform { column, function } => vec!["transform".into(), s(column), s(&function.to_m())],
        Step::GroupByLocal { keys, aggregations: a } => vec!["grouplocal".into(), list(keys), aggregations(a)],
        Step::AlternateRows { offset, skip, take } => vec!["alternate".into(), offset.to_string(), skip.to_string(), take.to_string()],
        Step::KeepRange { offset, count } => vec!["range".into(), offset.to_string(), count.map(|c| c.to_string()).unwrap_or_default()],
        Step::DuplicateColumn { column, new_name } => vec!["duplicate".into(), s(column), s(new_name)],
        Step::Code { names, bindings, result } => vec!["code".into(), list(names), s(bindings), s(result)],
    }
}

fn step_of(f: &[&str]) -> Option<Step> {
    let a = |i: usize| f.get(i).map(|x| unesc(x));
    let l = |i: usize| f.get(i).map(|x| unlist(x));
    let n = |i: usize| f.get(i)?.parse::<usize>().ok();
    Some(match *f.first()? {
        "promote" => Step::PromoteHeaders,
        "demote" => Step::DemoteHeaders,
        "type" => Step::ChangeType { column: a(1)?, ty: type_of(f.get(2)?)?, culture: culture_of(&a(3)?), apart: f.get(4) == Some(&"1") },
        "select" => Step::SelectColumns(l(1)?),
        "remove" => Step::RemoveColumns(l(1)?),
        // `rename`, then the old and the new name of each column (a step of one column, from before, is the same: two fields).
        "rename" => {
            let names: Vec<String> = f[1..].iter().map(|x| unesc(x)).collect();
            if names.is_empty() || names.len() % 2 != 0 {
                return None;
            }
            Step::RenameColumns(names.chunks(2).map(|c| (c[0].clone(), c[1].clone())).collect())
        }
        "reorder" => Step::ReorderColumns(l(1)?),
        "filter" => Step::FilterRows { column: a(1)?, op: op_of(f.get(2)?)?, value: a(3)? },
        "keeptop" => Step::KeepTopRows(n(1)?),
        "keepbottom" => Step::KeepBottomRows(n(1)?),
        "removetop" => Step::RemoveTopRows(n(1)?),
        "removebottom" => Step::RemoveBottomRows(n(1)?),
        "distinct" => Step::RemoveDuplicates(l(1)?),
        "removeblank" => Step::RemoveBlankRows,
        "removeerrors" => Step::RemoveErrors(l(1)?),
        "replaceerrors" => Step::ReplaceErrors { columns: l(1)?, with: a(2)? },
        "sort" => Step::Sort(if f.get(1)?.is_empty() {
            Vec::new()
        } else {
            f[1].split(LIST)
                .map(|p| {
                    let (c, asc) = p.split_once('\u{241e}')?;
                    Some((unesc(c), asc == "1"))
                })
                .collect::<Option<_>>()?
        }),
        "split" => Step::SplitColumn {
            column: a(1)?,
            columns: f.get(4).and_then(|x| x.parse().ok()).unwrap_or(0),
            by: match *f.get(2)? {
                "once" | "onceend" => SplitBy::Once { length: n(3)?, from_end: *f.get(2)? == "onceend" },
                "transition" => {
                    let c: Vec<char> = f.get(3)?.chars().collect();
                    SplitBy::Transition { from: class_of(*c.first()?)?, to: class_of(*c.get(1)?)? }
                }
                "length" => SplitBy::Length(n(3)?),
                "any" => SplitBy::AnyDelimiter(l(3)?),
                "each" | "eachend" => SplitBy::EachDelimiter { delimiters: l(3)?, from_end: *f.get(2)? == "eachend" },
                "positions" | "positionsend" => SplitBy::Positions { positions: unnumbers(f.get(3)?)?, from_end: *f.get(2)? == "positionsend" },
                "lengths" | "lengthsend" => SplitBy::Lengths { lengths: unnumbers(f.get(3)?)?, from_end: *f.get(2)? == "lengthsend" },
                _ => SplitBy::Delimiter(a(3)?),
            },
        },
        "mergecolumns" => Step::MergeColumns { columns: l(1)?, separator: a(2)?, name: a(3)? },
        "replace" => Step::ReplaceValue { column: a(1)?, find: a(2)?, with: a(3)?, whole_cell: *f.get(4)? == "1" },
        "replacenull" => Step::ReplaceNull { columns: l(1)?, with: a(2)? },
        "filldown" => Step::FillDown(l(1)?),
        "fillup" => Step::FillUp(l(1)?),
        "transpose" => Step::Transpose,
        "reverse" => Step::ReverseRows,
        "trim" => Step::Trim(l(1)?),
        "upper" => Step::Upper(l(1)?),
        "lower" => Step::Lower(l(1)?),
        "pivot" => Step::Pivot { attribute: a(1)?, values: a(2)?, agg: agg_of(f.get(3)?)? },
        "unpivot" => Step::Unpivot { columns: l(1)?, attribute: a(2)?, value: a(3)? },
        "unpivotother" => Step::UnpivotOther { keep: l(1)?, attribute: a(2)?, value: a(3)? },
        "group" => Step::GroupBy {
            keys: l(1)?,
            aggregations: if f.get(2)?.is_empty() {
                Vec::new()
            } else {
                f[2].split(LIST)
                    .map(|p| {
                        let parts: Vec<&str> = p.split('\u{241e}').collect();
                        (parts.len() == 3).then(|| Some((unesc(parts[0]), agg_of(parts[1])?, if parts[2].is_empty() { None } else { Some(unesc(parts[2])) }))).flatten()
                    })
                    .collect::<Option<_>>()?
            },
        },
        "merge" => Step::Merge { right: a(1)?, left_keys: l(2)?, right_keys: l(3)?, kind: join_of(f.get(4)?)?, expand: l(5)? },
        "append" => Step::Append { other: a(1)? },
        "addcolumn" => Step::AddColumn { name: a(1)?, formula: a(2)?, ty: match f.get(3) {
            Some(t) if !t.is_empty() => Some(type_of(t)?),
            _ => None,
        } },
        "addconditional" => Step::AddConditionalColumn { name: a(1)?, clauses: unclauses(f.get(2)?)?, otherwise: a(3)? },
        "addindex" => Step::AddIndexColumn { name: a(1)?, start: f.get(2)?.parse().ok()?, step: f.get(3)?.parse().ok()? },
        "filtercond" => Step::FilterCondition(super::m_parse::cond_expr(&a(1)?)?),
        "missing" => Step::WithMissing { inner: Box::new(step_of(&f[2..])?), missing: if *f.get(1)? == "ignore" { Missing::Ignore } else { Missing::UseNull } },
        "transform" => Step::Transform { column: a(1)?, function: Transform::from_m(&a(2)?)? },
        "grouplocal" => Step::GroupByLocal { keys: l(1)?, aggregations: aggregations_of(f.get(2)?)? },
        "alternate" => Step::AlternateRows { offset: n(1)?, skip: n(2)?, take: n(3)? },
        "range" => Step::KeepRange { offset: n(1)?, count: if f.get(2)?.is_empty() { None } else { Some(n(2)?) } },
        "duplicate" => Step::DuplicateColumn { column: a(1)?, new_name: a(2)? },
        "code" => Step::Code { names: unlist(f.get(1)?), bindings: a(2)?, result: a(3)? },
        _ => return None,
    })
}

/// The queries and parameters as text.
pub fn encode_queries(queries: &[Query], parameters: &[Parameter]) -> String {
    let mut out = String::from("ibis-queries 1\n");
    for p in parameters {
        out.push_str(&format!("parameter\t{}\t{}\n", esc(&p.name), esc(&p.value)));
    }
    for q in queries {
        out.push_str(&format!("query\t{}\t{}\n", esc(&q.name), q.output.as_deref().map(esc).unwrap_or_default()));
        match &q.source {
            Source::Blank => out.push_str("source\tblank\n"),
            Source::Table(t) => out.push_str(&format!("source\ttable\t{}\n", esc(t))),
            Source::Range { sheet, range } => out.push_str(&format!("source\trange\t{}\t{}\n", esc(sheet), esc(range))),
            Source::Csv { path, delimiter, columns, encoding, quote } => out.push_str(&format!("source\tcsv\t{}\t{}\t{columns}\t{encoding}\t{}\n", esc(path), esc(delimiter), esc(quote))),
            Source::Workbook { path, item, sheet } => out.push_str(&format!("source\tworkbook\t{}\t{}\t{}\n", esc(path), esc(item), if *sheet { "sheet" } else { "table" })),
        }
        for step in &q.steps {
            out.push_str(&format!("step\t{}\n", step_fields(step).join("\t")));
        }
    }
    out
}

/// The queries and parameters from text written by [`encode_queries`]; lines not understood are left out.
pub fn decode_queries(text: &str) -> (Vec<Query>, Vec<Parameter>) {
    let mut queries: Vec<Query> = Vec::new();
    let mut parameters = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        match f.first().copied() {
            Some("parameter") if f.len() >= 3 => parameters.push(Parameter { name: unesc(f[1]), value: unesc(f[2]) }),
            Some("query") if f.len() >= 2 => {
                let output = f.get(2).filter(|o| !o.is_empty()).map(|o| unesc(o));
                queries.push(Query { name: unesc(f[1]), source: Source::Table(String::new()), steps: Vec::new(), output });
            }
            Some("source") => {
                if let Some(q) = queries.last_mut() {
                    q.source = match f.get(1).copied() {
                        Some("blank") => Source::Blank,
                        Some("range") if f.len() >= 4 => Source::Range { sheet: unesc(f[2]), range: unesc(f[3]) },
                        Some("csv") if f.len() >= 7 => Source::Csv { path: unesc(f[2]), delimiter: unesc(f[3]), columns: f[4].parse().unwrap_or(0), encoding: f[5].parse().unwrap_or(65001), quote: unesc(f[6]) },
                        Some("workbook") if f.len() >= 5 => Source::Workbook { path: unesc(f[2]), item: unesc(f[3]), sheet: f[4] == "sheet" },
                        _ => Source::Table(f.get(2).map(|t| unesc(t)).unwrap_or_default()),
                    };
                }
            }
            Some("step") => {
                if let (Some(q), Some(step)) = (queries.last_mut(), step_of(&f[1..])) {
                    q.steps.push(step);
                }
            }
            _ => {}
        }
    }
    (queries, parameters)
}
