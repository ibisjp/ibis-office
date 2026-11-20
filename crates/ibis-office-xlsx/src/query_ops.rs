// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The operations of the クエリ エディター as forms: for each operation the
//! fields it asks, the values it starts with (from the columns selected),
//! the step the values make, and the values a step already in the list gives
//! back when it is edited.
use std::collections::HashMap;

use ibis_doc_model::query::{Agg, Clause, Cond, Culture, DateFilter, DateUnit, DateWhen, JoinKind, Op, QTable, QType, SplitBy, Step, Transform};

/// An operation of the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum QOp {
    PromoteHeaders,
    DemoteHeaders,
    ChangeType,
    SelectColumns,
    RemoveColumns,
    Rename,
    Reorder,
    Filter,
    KeepTop,
    KeepBottom,
    RemoveTop,
    RemoveBottom,
    RemoveDuplicates,
    RemoveBlankRows,
    RemoveErrors,
    ReplaceErrors,
    Sort,
    Split,
    MergeColumns,
    Replace,
    ReplaceNull,
    FillDown,
    FillUp,
    Transpose,
    Reverse,
    Trim,
    Upper,
    Lower,
    Pivot,
    Unpivot,
    UnpivotOther,
    GroupBy,
    Merge,
    Append,
    AddColumn,
    AddConditional,
    AddIndex,
    FilterDate,
    Transform,
    AlternateRows,
    KeepRange,
    DuplicateColumn,
    /// A step the code writes that no form edits (its name is shown in the list; the 詳細エディター edits it).
    FilterCode,
    SplitCode,
    MissingField,
    GroupLocal,
    /// The code step: the M the steps cannot hold, run by the M interpreter.
    MCode,
}

/// The ribbon's groups of operations.
pub(crate) const TABS: [(&str, &[QOp]); 4] = [
    (
        "query.tab.home",
        &[
            QOp::PromoteHeaders,
            QOp::DemoteHeaders,
            QOp::ChangeType,
            QOp::SelectColumns,
            QOp::RemoveColumns,
            QOp::Rename,
            QOp::KeepTop,
            QOp::KeepBottom,
            QOp::RemoveTop,
            QOp::RemoveBottom,
            QOp::RemoveDuplicates,
            QOp::RemoveBlankRows,
            QOp::RemoveErrors,
            QOp::Sort,
            QOp::Filter,
            QOp::FilterDate,
            QOp::KeepRange,
            QOp::AlternateRows,
        ],
    ),
    (
        "query.tab.transform",
        &[
            QOp::Split,
            QOp::MergeColumns,
            QOp::Replace,
            QOp::ReplaceNull,
            QOp::ReplaceErrors,
            QOp::FillDown,
            QOp::FillUp,
            QOp::Trim,
            QOp::Upper,
            QOp::Lower,
            QOp::Transform,
            QOp::Reorder,
            QOp::Transpose,
            QOp::Reverse,
            QOp::Pivot,
            QOp::Unpivot,
            QOp::UnpivotOther,
            QOp::GroupBy,
        ],
    ),
    ("query.tab.add", &[QOp::AddColumn, QOp::AddConditional, QOp::AddIndex, QOp::DuplicateColumn]),
    ("query.tab.combine", &[QOp::Merge, QOp::Append]),
];

/// What a field is asked as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Text,
    /// One of these (their i18n ids); the value is the index.
    Choice(&'static [&'static str]),
    Flag,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Field {
    pub id: &'static str,
    pub kind: Kind,
}

pub(crate) type Values = HashMap<String, String>;

const fn text(id: &'static str) -> Field {
    Field { id, kind: Kind::Text }
}

const fn choice(id: &'static str, options: &'static [&'static str]) -> Field {
    Field { id, kind: Kind::Choice(options) }
}

pub(crate) const TYPES: [&str; 13] = [
    "query.type.any",
    "query.type.text",
    "query.type.whole",
    "query.type.decimal",
    "query.type.date",
    "query.type.datetime",
    "query.type.time",
    "query.type.logical",
    "query.type.currency",
    "query.type.percentage",
    "query.type.int32",
    "query.type.datetimezone",
    "query.type.duration",
];
pub(crate) const CULTURES: [&str; 6] = ["query.culture.invariant", "query.culture.en-us", "query.culture.ja-jp", "query.culture.de-de", "query.culture.fr-fr", "query.culture.zh-cn"];
pub(crate) const COMPARISONS: [&str; 14] = [
    "query.cmp.equals",
    "query.cmp.not-equals",
    "query.cmp.greater",
    "query.cmp.greater-or-equal",
    "query.cmp.less",
    "query.cmp.less-or-equal",
    "query.cmp.contains",
    "query.cmp.does-not-contain",
    "query.cmp.begins-with",
    "query.cmp.ends-with",
    "query.cmp.is-null",
    "query.cmp.is-not-null",
    "query.cmp.does-not-begin-with",
    "query.cmp.does-not-end-with",
];
pub(crate) const LOGICS: [&str; 2] = ["query.logic.and", "query.logic.or"];
pub(crate) const WHENS: [&str; 4] = ["query.when.previous", "query.when.current", "query.when.next", "query.when.year-to-date"];
pub(crate) const UNITS: [&str; 5] = ["query.unit.day", "query.unit.week", "query.unit.month", "query.unit.quarter", "query.unit.year"];
pub(crate) const FUNCTIONS: [&str; 16] = [
    "query.fn.trim",
    "query.fn.upper",
    "query.fn.lower",
    "query.fn.proper",
    "query.fn.clean",
    "query.fn.length",
    "query.fn.round",
    "query.fn.round-up",
    "query.fn.round-down",
    "query.fn.abs",
    "query.fn.sqrt",
    "query.fn.year",
    "query.fn.month",
    "query.fn.day",
    "query.fn.prefix",
    "query.fn.suffix",
];
pub(crate) const AGGREGATES: [&str; 8] = ["query.agg.sum", "query.agg.average", "query.agg.min", "query.agg.max", "query.agg.count", "query.agg.distinct", "query.agg.median", "query.agg.first"];
pub(crate) const JOINS: [&str; 6] = ["query.join.left", "query.join.right", "query.join.full", "query.join.inner", "query.join.left-anti", "query.join.right-anti"];
pub(crate) const ORDERS: [&str; 2] = ["query.order.ascending", "query.order.descending"];
pub(crate) const SPLITS: [&str; 3] = ["query.split.delimiter", "query.split.length", "query.split.positions"];

impl QOp {
    /// The operation's name id (`query.op.<this>`).
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::PromoteHeaders => "promote",
            Self::DemoteHeaders => "demote",
            Self::ChangeType => "change-type",
            Self::SelectColumns => "select-columns",
            Self::RemoveColumns => "remove-columns",
            Self::Rename => "rename",
            Self::Reorder => "reorder",
            Self::Filter => "filter",
            Self::KeepTop => "keep-top",
            Self::KeepBottom => "keep-bottom",
            Self::RemoveTop => "remove-top",
            Self::RemoveBottom => "remove-bottom",
            Self::RemoveDuplicates => "remove-duplicates",
            Self::RemoveBlankRows => "remove-blank-rows",
            Self::RemoveErrors => "remove-errors",
            Self::ReplaceErrors => "replace-errors",
            Self::Sort => "sort",
            Self::Split => "split",
            Self::MergeColumns => "merge-columns",
            Self::Replace => "replace",
            Self::ReplaceNull => "replace-null",
            Self::FillDown => "fill-down",
            Self::FillUp => "fill-up",
            Self::Transpose => "transpose",
            Self::Reverse => "reverse",
            Self::Trim => "trim",
            Self::Upper => "upper",
            Self::Lower => "lower",
            Self::Pivot => "pivot",
            Self::Unpivot => "unpivot",
            Self::UnpivotOther => "unpivot-other",
            Self::GroupBy => "group-by",
            Self::Merge => "merge",
            Self::Append => "append",
            Self::AddColumn => "add-column",
            Self::AddConditional => "add-conditional",
            Self::AddIndex => "add-index",
            Self::FilterDate => "filter-date",
            Self::Transform => "transform",
            Self::AlternateRows => "alternate-rows",
            Self::KeepRange => "keep-range",
            Self::DuplicateColumn => "duplicate-column",
            Self::FilterCode => "filter-code",
            Self::SplitCode => "split-code",
            Self::MissingField => "missing-field",
            Self::GroupLocal => "group-local",
            Self::MCode => "m-code",
        }
    }

    /// The fields the operation asks (none: it needs nothing more).
    pub(crate) fn fields(self) -> Vec<Field> {
        match self {
            Self::PromoteHeaders | Self::DemoteHeaders | Self::RemoveBlankRows | Self::Transpose | Self::Reverse | Self::FilterCode | Self::SplitCode | Self::MissingField | Self::GroupLocal | Self::MCode => vec![],
            Self::ChangeType => vec![text("column"), choice("type", &TYPES), choice("culture", &CULTURES)],
            Self::SelectColumns | Self::RemoveColumns | Self::Reorder | Self::RemoveDuplicates | Self::RemoveErrors | Self::FillDown | Self::FillUp | Self::Trim | Self::Upper | Self::Lower => vec![text("columns")],
            Self::Rename => vec![text("from"), text("to")],
            // The second comparison is optional (an empty column leaves it out); `logic` joins the two.
            Self::Filter => vec![text("column"), choice("comparison", &COMPARISONS), text("value"), choice("logic", &LOGICS), text("column2"), choice("comparison2", &COMPARISONS), text("value2")],
            Self::FilterDate => vec![text("column"), choice("when", &WHENS), choice("unit", &UNITS), text("count")],
            Self::Transform => vec![text("column"), choice("function", &FUNCTIONS), text("value")],
            Self::AlternateRows => vec![text("offset"), text("skip"), text("take")],
            Self::KeepRange => vec![text("first-row"), text("count")],
            Self::DuplicateColumn => vec![text("column"), text("new-name")],
            Self::KeepTop | Self::KeepBottom | Self::RemoveTop | Self::RemoveBottom => vec![text("count")],
            Self::ReplaceErrors => vec![text("columns"), text("with")],
            Self::Sort => vec![text("column"), choice("order", &ORDERS), text("column2"), choice("order2", &ORDERS)],
            Self::Split => vec![text("column"), choice("by", &SPLITS), text("value")],
            Self::MergeColumns => vec![text("columns"), text("separator"), text("new-name")],
            Self::Replace => vec![text("column"), text("find"), text("with"), Field { id: "whole", kind: Kind::Flag }],
            Self::ReplaceNull => vec![text("columns"), text("with")],
            Self::Pivot => vec![text("attribute"), text("values"), choice("aggregate", &AGGREGATES)],
            Self::Unpivot => vec![text("columns"), text("attribute-name"), text("value-name")],
            Self::UnpivotOther => vec![text("columns"), text("attribute-name"), text("value-name")],
            Self::GroupBy => vec![text("columns"), text("name1"), choice("aggregate1", &AGGREGATES), text("column1"), text("name2"), choice("aggregate2", &AGGREGATES), text("column2")],
            Self::Merge => vec![text("right"), text("left-keys"), text("right-keys"), choice("join", &JOINS), text("expand")],
            Self::Append => vec![text("other")],
            Self::AddColumn => vec![text("new-name"), text("formula")],
            Self::AddConditional => vec![
                text("new-name"),
                text("column"),
                choice("comparison", &COMPARISONS),
                text("value"),
                text("output"),
                text("column2"),
                choice("comparison2", &COMPARISONS),
                text("value2"),
                text("output2"),
                text("otherwise"),
            ],
            Self::AddIndex => vec![text("new-name"), text("start"), text("step")],
        }
    }

    /// Whether the operation asks the columns only: it is made at once when some are selected.
    pub(crate) fn takes_columns_only(self) -> bool {
        let fields = self.fields();
        fields.len() == 1 && fields[0].id == "columns"
    }
}

/// The names in a list field: separated by commas.
pub(crate) fn names(text: &str) -> Vec<String> {
    text.split([',', '、']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn get<'a>(v: &'a Values, id: &str) -> &'a str {
    v.get(id).map(String::as_str).unwrap_or("")
}

fn index(v: &Values, id: &str) -> usize {
    get(v, id).parse().unwrap_or(0)
}

fn need(v: &Values, id: &str, what: &str) -> Result<String, String> {
    let t = get(v, id).trim().to_string();
    if t.is_empty() { Err(what.to_string()) } else { Ok(t) }
}

fn count(v: &Values) -> Result<usize, String> {
    get(v, "count").trim().parse::<usize>().map_err(|_| "count".to_string())
}

fn clause(v: &Values, suffix: &str) -> Option<Clause> {
    let column = get(v, &format!("column{suffix}")).trim().to_string();
    if column.is_empty() {
        return None;
    }
    Some(Clause { column, op: Op::ALL[index(v, &format!("comparison{suffix}")).min(Op::ALL.len() - 1)], value: get(v, &format!("value{suffix}")).to_string(), output: get(v, &format!("output{suffix}")).to_string() })
}

/// The step the values of the operation's form make; the error is the id of the field to fill (`query.error.<it>`).
pub(crate) fn build(op: QOp, v: &Values) -> Result<Step, String> {
    let columns = |id: &str| -> Result<Vec<String>, String> {
        let list = names(get(v, id));
        if list.is_empty() { Err(id.to_string()) } else { Ok(list) }
    };
    Ok(match op {
        QOp::PromoteHeaders => Step::PromoteHeaders,
        QOp::DemoteHeaders => Step::DemoteHeaders,
        QOp::ChangeType => Step::ChangeType { column: need(v, "column", "column")?, ty: QType::ALL[index(v, "type").min(QType::ALL.len() - 1)], culture: Culture::ALL[index(v, "culture").min(Culture::ALL.len() - 1)], apart: false },
        QOp::SelectColumns => Step::SelectColumns(columns("columns")?),
        QOp::RemoveColumns => Step::RemoveColumns(columns("columns")?),
        QOp::Rename => {
            // Several columns at once: the old names and the new names are lists of the same length, in the same order.
            let (from, to) = (columns("from")?, columns("to")?);
            if from.len() != to.len() {
                return Err("to".into());
            }
            Step::RenameColumns(from.into_iter().zip(to).collect())
        }
        QOp::Reorder => Step::ReorderColumns(columns("columns")?),
        QOp::Filter => {
            let op = Op::ALL[index(v, "comparison").min(Op::ALL.len() - 1)];
            if op.takes_value() && get(v, "value").is_empty() {
                return Err("value".into());
            }
            let (column, value) = (need(v, "column", "column")?, get(v, "value").to_string());
            let second = get(v, "column2").trim();
            if second.is_empty() {
                Step::FilterRows { column, op, value }
            } else {
                let op2 = Op::ALL[index(v, "comparison2").min(Op::ALL.len() - 1)];
                if op2.takes_value() && get(v, "value2").is_empty() {
                    return Err("value2".into());
                }
                let leaves = vec![Cond::Leaf { column, op, value }, Cond::Leaf { column: second.to_string(), op: op2, value: get(v, "value2").to_string() }];
                Step::FilterCondition(if index(v, "logic") == 1 { Cond::Or(leaves) } else { Cond::And(leaves) })
            }
        }
        QOp::FilterDate => {
            let when = [DateWhen::Previous, DateWhen::Current, DateWhen::Next, DateWhen::YearToDate][index(v, "when").min(3)];
            let unit = DateUnit::ALL[index(v, "unit").min(4)];
            let count = match get(v, "count").trim() {
                "" => None,
                n => Some(n.parse::<u32>().ok().filter(|n| *n > 0).ok_or("count")?),
            };
            // The current unit and the year to date have no count.
            let count = if matches!(when, DateWhen::Current | DateWhen::YearToDate) { None } else { count };
            Step::FilterCondition(Cond::Date { column: need(v, "column", "column")?, filter: DateFilter { when, unit, count } })
        }
        QOp::Transform => {
            let value = get(v, "value");
            let function = match index(v, "function").min(FUNCTIONS.len() - 1) {
                0 => Transform::Trim,
                1 => Transform::Upper,
                2 => Transform::Lower,
                3 => Transform::Proper,
                4 => Transform::Clean,
                5 => Transform::Length,
                6 => Transform::Round(if value.trim().is_empty() { 0 } else { value.trim().parse().map_err(|_| "value".to_string())? }),
                7 => Transform::RoundUp,
                8 => Transform::RoundDown,
                9 => Transform::Abs,
                10 => Transform::Sqrt,
                11 => Transform::Year,
                12 => Transform::Month,
                13 => Transform::Day,
                14 => Transform::Prefix(value.to_string()),
                _ => Transform::Suffix(value.to_string()),
            };
            Step::Transform { column: need(v, "column", "column")?, function }
        }
        QOp::AlternateRows => {
            let number = |id: &str| get(v, id).trim().parse::<usize>().map_err(|_| id.to_string());
            Step::AlternateRows { offset: number("offset")?, skip: number("skip")?, take: number("take")? }
        }
        QOp::KeepRange => {
            let first = get(v, "first-row").trim().parse::<usize>().ok().filter(|n| *n >= 1).ok_or("first-row")?;
            let count = match get(v, "count").trim() {
                "" => None,
                n => Some(n.parse::<usize>().map_err(|_| "count".to_string())?),
            };
            Step::KeepRange { offset: first - 1, count }
        }
        QOp::DuplicateColumn => Step::DuplicateColumn { column: need(v, "column", "column")?, new_name: need(v, "new-name", "new-name")? },
        // These are not made by a form.
        QOp::FilterCode | QOp::SplitCode | QOp::MissingField | QOp::GroupLocal | QOp::MCode => return Err("column".into()),
        QOp::KeepTop => Step::KeepTopRows(count(v)?),
        QOp::KeepBottom => Step::KeepBottomRows(count(v)?),
        QOp::RemoveTop => Step::RemoveTopRows(count(v)?),
        QOp::RemoveBottom => Step::RemoveBottomRows(count(v)?),
        QOp::RemoveDuplicates => Step::RemoveDuplicates(names(get(v, "columns"))),
        QOp::RemoveBlankRows => Step::RemoveBlankRows,
        QOp::RemoveErrors => Step::RemoveErrors(names(get(v, "columns"))),
        QOp::ReplaceErrors => Step::ReplaceErrors { columns: columns("columns")?, with: get(v, "with").to_string() },
        QOp::Sort => {
            let mut keys = vec![(need(v, "column", "column")?, index(v, "order") == 0)];
            let second = get(v, "column2").trim();
            if !second.is_empty() {
                keys.push((second.to_string(), index(v, "order2") == 0));
            }
            Step::Sort(keys)
        }
        QOp::Split => {
            let by = if index(v, "by") == 2 {
                // Positions from the start, separated by commas (`0, 3, 5`).
                let positions: Vec<usize> = names(get(v, "value")).iter().map(|x| x.parse().ok()).collect::<Option<_>>().filter(|p: &Vec<usize>| !p.is_empty()).ok_or("value")?;
                SplitBy::Positions { positions, from_end: false }
            } else if index(v, "by") == 1 {
                SplitBy::Length(get(v, "value").trim().parse::<usize>().ok().filter(|n| *n > 0).ok_or("value")?)
            } else {
                if get(v, "value").is_empty() {
                    return Err("value".into());
                }
                SplitBy::Delimiter(get(v, "value").to_string())
            };
            Step::SplitColumn { column: need(v, "column", "column")?, by, columns: 0 }
        }
        QOp::MergeColumns => Step::MergeColumns { columns: columns("columns")?, separator: get(v, "separator").to_string(), name: need(v, "new-name", "new-name")? },
        QOp::Replace => Step::ReplaceValue { column: need(v, "column", "column")?, find: need(v, "find", "find")?, with: get(v, "with").to_string(), whole_cell: get(v, "whole") == "1" },
        QOp::ReplaceNull => Step::ReplaceNull { columns: columns("columns")?, with: get(v, "with").to_string() },
        QOp::FillDown => Step::FillDown(columns("columns")?),
        QOp::FillUp => Step::FillUp(columns("columns")?),
        QOp::Transpose => Step::Transpose,
        QOp::Reverse => Step::ReverseRows,
        QOp::Trim => Step::Trim(columns("columns")?),
        QOp::Upper => Step::Upper(columns("columns")?),
        QOp::Lower => Step::Lower(columns("columns")?),
        QOp::Pivot => Step::Pivot { attribute: need(v, "attribute", "attribute")?, values: need(v, "values", "values")?, agg: Agg::ALL[index(v, "aggregate").min(7)] },
        QOp::Unpivot => Step::Unpivot { columns: columns("columns")?, attribute: need(v, "attribute-name", "attribute-name")?, value: need(v, "value-name", "value-name")? },
        QOp::UnpivotOther => Step::UnpivotOther { keep: columns("columns")?, attribute: need(v, "attribute-name", "attribute-name")?, value: need(v, "value-name", "value-name")? },
        QOp::GroupBy => {
            let mut aggregations = Vec::new();
            for n in ["1", "2"] {
                let name = get(v, &format!("name{n}")).trim().to_string();
                if name.is_empty() {
                    continue;
                }
                let agg = Agg::ALL[index(v, &format!("aggregate{n}")).min(7)];
                let column = get(v, &format!("column{n}")).trim().to_string();
                if agg != Agg::Count && column.is_empty() {
                    return Err(format!("column{n}"));
                }
                aggregations.push((name, agg, if column.is_empty() { None } else { Some(column) }));
            }
            if aggregations.is_empty() {
                return Err("name1".into());
            }
            Step::GroupBy { keys: columns("columns")?, aggregations }
        }
        QOp::Merge => {
            let (left, right) = (columns("left-keys")?, columns("right-keys")?);
            if left.len() != right.len() {
                return Err("right-keys".into());
            }
            Step::Merge { right: need(v, "right", "right")?, left_keys: left, right_keys: right, kind: JoinKind::ALL[index(v, "join").min(5)], expand: columns("expand")? }
        }
        QOp::Append => Step::Append { other: need(v, "other", "other")? },
        QOp::AddColumn => Step::AddColumn { name: need(v, "new-name", "new-name")?, formula: need(v, "formula", "formula")?, ty: None },
        QOp::AddConditional => {
            let mut clauses = Vec::new();
            clauses.extend(clause(v, ""));
            clauses.extend(clause(v, "2"));
            if clauses.is_empty() {
                return Err("column".into());
            }
            Step::AddConditionalColumn { name: need(v, "new-name", "new-name")?, clauses, otherwise: get(v, "otherwise").to_string() }
        }
        QOp::AddIndex => Step::AddIndexColumn {
            name: need(v, "new-name", "new-name")?,
            start: get(v, "start").trim().parse().map_err(|_| "start".to_string())?,
            step: get(v, "step").trim().parse().map_err(|_| "step".to_string())?,
        },
    })
}

fn join(list: &[String]) -> String {
    list.join(", ")
}

fn position<T: PartialEq + Copy>(all: &[T], x: T) -> String {
    all.iter().position(|a| *a == x).unwrap_or(0).to_string()
}

/// The operation and the form values of a step in the list (to edit it).
pub(crate) fn values_of(step: &Step) -> (QOp, Values) {
    let mut v = Values::new();
    let mut set = |id: &str, value: String| {
        v.insert(id.to_string(), value);
    };
    let op = match step {
        Step::PromoteHeaders => QOp::PromoteHeaders,
        Step::DemoteHeaders => QOp::DemoteHeaders,
        Step::ChangeType { column, ty, culture, .. } => {
            set("column", column.clone());
            set("type", position(&QType::ALL, *ty));
            set("culture", position(&Culture::ALL, *culture));
            QOp::ChangeType
        }
        Step::SelectColumns(c) => {
            set("columns", join(c));
            QOp::SelectColumns
        }
        Step::RemoveColumns(c) => {
            set("columns", join(c));
            QOp::RemoveColumns
        }
        Step::RenameColumns(renames) => {
            set("from", join(&renames.iter().map(|(from, _)| from.clone()).collect::<Vec<_>>()));
            set("to", join(&renames.iter().map(|(_, to)| to.clone()).collect::<Vec<_>>()));
            QOp::Rename
        }
        Step::ReorderColumns(c) => {
            set("columns", join(c));
            QOp::Reorder
        }
        Step::FilterRows { column, op, value } => {
            set("column", column.clone());
            set("comparison", position(&Op::ALL, *op));
            set("value", value.clone());
            set("logic", "0".into());
            set("column2", String::new());
            set("comparison2", "0".into());
            set("value2", String::new());
            QOp::Filter
        }
        Step::FilterCondition(cond) => match cond {
            Cond::Date { column, filter } => {
                set("column", column.clone());
                set("when", position(&[DateWhen::Previous, DateWhen::Current, DateWhen::Next, DateWhen::YearToDate], filter.when));
                set("unit", position(&DateUnit::ALL, filter.unit));
                set("count", filter.count.map(|n| n.to_string()).unwrap_or_default());
                QOp::FilterDate
            }
            Cond::And(parts) | Cond::Or(parts) if matches!(&parts[..], [Cond::Leaf { .. }, Cond::Leaf { .. }]) => {
                let (Cond::Leaf { column, op, value }, Cond::Leaf { column: column2, op: op2, value: value2 }) = (&parts[0], &parts[1]) else { return (QOp::FilterCode, v) };
                set("column", column.clone());
                set("comparison", position(&Op::ALL, *op));
                set("value", value.clone());
                set("logic", if matches!(cond, Cond::Or(_)) { "1" } else { "0" }.into());
                set("column2", column2.clone());
                set("comparison2", position(&Op::ALL, *op2));
                set("value2", value2.clone());
                QOp::Filter
            }
            _ => return (QOp::FilterCode, v),
        },
        Step::WithMissing { .. } => return (QOp::MissingField, v),
        Step::Transform { column, function } => {
            set("column", column.clone());
            let (index, value) = match function {
                Transform::Trim => (0, String::new()),
                Transform::Upper => (1, String::new()),
                Transform::Lower => (2, String::new()),
                Transform::Proper => (3, String::new()),
                Transform::Clean => (4, String::new()),
                Transform::Length => (5, String::new()),
                Transform::Round(n) => (6, n.to_string()),
                Transform::RoundUp => (7, String::new()),
                Transform::RoundDown => (8, String::new()),
                Transform::Abs => (9, String::new()),
                Transform::Sqrt => (10, String::new()),
                Transform::Year => (11, String::new()),
                Transform::Month => (12, String::new()),
                Transform::Day => (13, String::new()),
                Transform::Prefix(s) => (14, s.clone()),
                Transform::Suffix(s) => (15, s.clone()),
            };
            set("function", index.to_string());
            set("value", value);
            QOp::Transform
        }
        Step::GroupByLocal { .. } => return (QOp::GroupLocal, v),
        Step::Code { .. } => return (QOp::MCode, v),
        Step::AlternateRows { offset, skip, take } => {
            set("offset", offset.to_string());
            set("skip", skip.to_string());
            set("take", take.to_string());
            QOp::AlternateRows
        }
        Step::KeepRange { offset, count } => {
            set("first-row", (offset + 1).to_string());
            set("count", count.map(|c| c.to_string()).unwrap_or_default());
            QOp::KeepRange
        }
        Step::DuplicateColumn { column, new_name } => {
            set("column", column.clone());
            set("new-name", new_name.clone());
            QOp::DuplicateColumn
        }
        Step::KeepTopRows(n) => {
            set("count", n.to_string());
            QOp::KeepTop
        }
        Step::KeepBottomRows(n) => {
            set("count", n.to_string());
            QOp::KeepBottom
        }
        Step::RemoveTopRows(n) => {
            set("count", n.to_string());
            QOp::RemoveTop
        }
        Step::RemoveBottomRows(n) => {
            set("count", n.to_string());
            QOp::RemoveBottom
        }
        Step::RemoveDuplicates(c) => {
            set("columns", join(c));
            QOp::RemoveDuplicates
        }
        Step::RemoveBlankRows => QOp::RemoveBlankRows,
        Step::RemoveErrors(c) => {
            set("columns", join(c));
            QOp::RemoveErrors
        }
        Step::ReplaceErrors { columns, with } => {
            set("columns", join(columns));
            set("with", with.clone());
            QOp::ReplaceErrors
        }
        Step::Sort(keys) => {
            if let Some((c, asc)) = keys.first() {
                set("column", c.clone());
                set("order", if *asc { "0" } else { "1" }.into());
            }
            if let Some((c, asc)) = keys.get(1) {
                set("column2", c.clone());
                set("order2", if *asc { "0" } else { "1" }.into());
            }
            QOp::Sort
        }
        Step::SplitColumn { column, by, .. } => {
            set("column", column.clone());
            match by {
                SplitBy::Delimiter(d) => {
                    set("by", "0".into());
                    set("value", d.clone());
                }
                SplitBy::Length(n) => {
                    set("by", "1".into());
                    set("value", n.to_string());
                }
                SplitBy::Positions { positions, from_end: false } => {
                    set("by", "2".into());
                    set("value", positions.iter().map(usize::to_string).collect::<Vec<_>>().join(", "));
                }
                _ => return (QOp::SplitCode, v),
            }
            QOp::Split
        }
        Step::MergeColumns { columns, separator, name } => {
            set("columns", join(columns));
            set("separator", separator.clone());
            set("new-name", name.clone());
            QOp::MergeColumns
        }
        Step::ReplaceValue { column, find, with, whole_cell } => {
            set("column", column.clone());
            set("find", find.clone());
            set("with", with.clone());
            set("whole", if *whole_cell { "1" } else { "0" }.into());
            QOp::Replace
        }
        Step::ReplaceNull { columns, with } => {
            set("columns", join(columns));
            set("with", with.clone());
            QOp::ReplaceNull
        }
        Step::FillDown(c) => {
            set("columns", join(c));
            QOp::FillDown
        }
        Step::FillUp(c) => {
            set("columns", join(c));
            QOp::FillUp
        }
        Step::Transpose => QOp::Transpose,
        Step::ReverseRows => QOp::Reverse,
        Step::Trim(c) => {
            set("columns", join(c));
            QOp::Trim
        }
        Step::Upper(c) => {
            set("columns", join(c));
            QOp::Upper
        }
        Step::Lower(c) => {
            set("columns", join(c));
            QOp::Lower
        }
        Step::Pivot { attribute, values, agg } => {
            set("attribute", attribute.clone());
            set("values", values.clone());
            set("aggregate", position(&Agg::ALL, *agg));
            QOp::Pivot
        }
        Step::Unpivot { columns, attribute, value } => {
            set("columns", join(columns));
            set("attribute-name", attribute.clone());
            set("value-name", value.clone());
            QOp::Unpivot
        }
        Step::UnpivotOther { keep, attribute, value } => {
            set("columns", join(keep));
            set("attribute-name", attribute.clone());
            set("value-name", value.clone());
            QOp::UnpivotOther
        }
        Step::GroupBy { keys, aggregations } => {
            set("columns", join(keys));
            for (k, (name, agg, column)) in aggregations.iter().take(2).enumerate() {
                let n = k + 1;
                set(&format!("name{n}"), name.clone());
                set(&format!("aggregate{n}"), position(&Agg::ALL, *agg));
                set(&format!("column{n}"), column.clone().unwrap_or_default());
            }
            QOp::GroupBy
        }
        Step::Merge { right, left_keys, right_keys, kind, expand } => {
            set("right", right.clone());
            set("left-keys", join(left_keys));
            set("right-keys", join(right_keys));
            set("join", position(&JoinKind::ALL, *kind));
            set("expand", join(expand));
            QOp::Merge
        }
        Step::Append { other } => {
            set("other", other.clone());
            QOp::Append
        }
        Step::AddColumn { name, formula, .. } => {
            set("new-name", name.clone());
            set("formula", formula.clone());
            QOp::AddColumn
        }
        Step::AddConditionalColumn { name, clauses, otherwise } => {
            set("new-name", name.clone());
            set("otherwise", otherwise.clone());
            for (k, c) in clauses.iter().take(2).enumerate() {
                let s = if k == 0 { "" } else { "2" };
                set(&format!("column{s}"), c.column.clone());
                set(&format!("comparison{s}"), position(&Op::ALL, c.op));
                set(&format!("value{s}"), c.value.clone());
                set(&format!("output{s}"), c.output.clone());
            }
            QOp::AddConditional
        }
        Step::AddIndexColumn { name, start, step } => {
            set("new-name", name.clone());
            set("start", start.to_string());
            set("step", step.to_string());
            QOp::AddIndex
        }
    };
    (op, v)
}

/// The values a form starts with for a new step: the selected columns where the operation asks for columns.
pub(crate) fn defaults(op: QOp, selected: &[String], table: &QTable) -> Values {
    let mut v = Values::new();
    let first = selected.first().cloned().or_else(|| table.columns.first().map(|c| c.name.clone())).unwrap_or_default();
    for f in op.fields() {
        let value = match (f.id, f.kind) {
            ("columns", _) | ("left-keys", _) => join(selected),
            ("from", _) if !selected.is_empty() => join(selected),
            ("column", _) | ("from", _) | ("attribute", _) | ("column1", _) => first.clone(),
            ("values", _) => table.columns.iter().rev().find(|c| !selected.contains(&c.name)).map(|c| c.name.clone()).unwrap_or_default(),
            ("count", _) if op == QOp::FilterDate => String::new(),
            ("count", _) => "10".into(),
            ("first-row", _) => "1".into(),
            ("offset", _) | ("skip", _) | ("take", _) => "1".into(),
            ("separator", _) => " ".into(),
            ("new-name", _) => match op {
                QOp::MergeColumns => "Merged".into(),
                QOp::AddColumn => "Custom".into(),
                QOp::AddConditional => "Conditional".into(),
                QOp::AddIndex => "Index".into(),
                QOp::DuplicateColumn => format!("Copy of {first}"),
                _ => String::new(),
            },
            ("attribute-name", _) => "Attribute".into(),
            ("value-name", _) => "Value".into(),
            ("start", _) => "0".into(),
            ("step", _) => "1".into(),
            ("name1", _) => "Count".into(),
            (_, Kind::Choice(_)) => "0".into(),
            (_, Kind::Flag) => "0".into(),
            _ => String::new(),
        };
        v.insert(f.id.to_string(), value);
    }
    v
}

/// The step's name id (`query.step.<this>`).
pub(crate) fn step_id(step: &Step) -> &'static str {
    values_of(step).0.id()
}

#[cfg(test)]
mod tests {
    use super::{build, defaults, step_id, values_of, QOp, TABS};
    use ibis_doc_model::query::{QTable, Step};

    #[test]
    fn every_operation_builds_the_step_it_started_with() {
        let table = QTable::from_rows(vec!["a".into(), "b".into(), "c".into()], vec![]);
        let selected = vec!["a".to_string(), "b".to_string()];
        let mut seen = 0;
        for (_, ops) in TABS {
            for &op in ops {
                let mut v = defaults(op, &selected, &table);
                // Fill what the defaults leave empty.
                for (k, value) in [("find", "x"), ("first-row", "1"), ("formula", "[a] + 1"), ("right", "T2"), ("right-keys", "a, b"), ("expand", "c"), ("other", "T2"), ("name2", ""), ("with", "0"), ("value", "1"), ("to", "z, y"), ("new-name", "N"), ("attribute-name", "A"), ("value-name", "V"), ("column1", "c")] {
                    if v.get(k).is_none_or(String::is_empty) && op.fields().iter().any(|f| f.id == k) {
                        v.insert(k.into(), value.into());
                    }
                }
                let step = build(op, &v).unwrap_or_else(|e| panic!("{:?} gives {e}", op));
                let (back, again) = values_of(&step);
                assert_eq!(back, op, "{step:?}");
                assert_eq!(build(op, &again).unwrap(), step, "{op:?}");
                assert_eq!(step_id(&step), op.id());
                seen += 1;
            }
        }
        assert_eq!(seen, 42);
        assert!(matches!(build(QOp::SelectColumns, &defaults(QOp::SelectColumns, &[], &table)), Err(e) if e == "columns"));
        // Several columns are renamed in one step, the old and the new names in the same order; a different number is refused.
        let mut rename = defaults(QOp::Rename, &selected, &table);
        rename.insert("to".into(), "x, y".into());
        assert_eq!(build(QOp::Rename, &rename).unwrap(), Step::RenameColumns(vec![("a".into(), "x".into()), ("b".into(), "y".into())]));
        rename.insert("to".into(), "x".into());
        assert!(matches!(build(QOp::Rename, &rename), Err(e) if e == "to"));
        assert_eq!(build(QOp::KeepTop, &defaults(QOp::KeepTop, &[], &table)).unwrap(), Step::KeepTopRows(10));
    }
}
