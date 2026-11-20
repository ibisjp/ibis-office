// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0039: the M interpreter (`ibis-m-lang`) and the query's table (`QTable`): the table of the steps as an M value, and an M table
//! value as a `QTable`. Dates, times and durations are serial numbers in a `QTable` (the column's `QType` says which); in M they are
//! values of their own types, so they are turned into those on the way in and back into serial numbers on the way out.
use super::{QColumn, QTable, QType, Resolver};
use ibis_calc::{CellError, CellValue};
use ibis_m_lang::ast::{Ty, TypeSpec};
use ibis_m_lang::{CsvOptions, ErrKind, Host, Interp, Table, Value};
use std::rc::Rc;

/// Why an M text did not give a table. The application words these (with the line) in its own language.
#[derive(Clone, Debug, PartialEq)]
pub enum MEvalError {
    Syntax { line: u32, message: String },
    /// A library function the interpreter does not have yet (design 9.4), and the line of its call.
    Unsupported { name: String, line: u32 },
    /// An M error that was raised (`error "…"`, a type that does not fit...).
    Error { reason: String, message: String },
    /// The result is not a table (its type name).
    NotATable(String),
    /// The evaluation went on too long or too deep.
    Limit(String),
}

/// 1899-12-30 as days since 0001-01-01: serial `n` of a workbook is that day plus `n` (the serials of 1900-03-01 and later; the
/// nonexistent 1900-02-29 of Excel's calendar is before them).
fn epoch() -> i64 {
    ibis_m_lang::value::days_from_civil(1899, 12, 30)
}

pub fn type_of(t: QType) -> TypeSpec {
    let ty = match t {
        QType::Any => Ty::Any,
        QType::Text => Ty::Text,
        QType::Whole => Ty::Int64,
        QType::Decimal => Ty::Number,
        QType::Date => Ty::Date,
        QType::DateTime => Ty::DateTime,
        QType::Time => Ty::Time,
        QType::Logical => Ty::Logical,
        QType::Currency => Ty::Currency,
        QType::Percentage => Ty::Percentage,
        QType::Int32 => Ty::Int32,
        QType::DateTimeZone => Ty::DateTimeZone,
        QType::Duration => Ty::Duration,
    };
    TypeSpec { ty, nullable: true }
}

pub fn qtype_of(t: &TypeSpec) -> QType {
    match t.ty {
        Ty::Text => QType::Text,
        Ty::Int64 => QType::Whole,
        Ty::Number => QType::Decimal,
        Ty::Date => QType::Date,
        Ty::DateTime => QType::DateTime,
        Ty::Time => QType::Time,
        Ty::Logical => QType::Logical,
        Ty::Currency => QType::Currency,
        Ty::Percentage => QType::Percentage,
        Ty::Int32 => QType::Int32,
        Ty::DateTimeZone => QType::DateTimeZone,
        Ty::Duration => QType::Duration,
        _ => QType::Any,
    }
}

fn seconds(frac_of_day: f64) -> f64 {
    // To the millisecond: a serial number cannot hold more.
    (frac_of_day * 86400.0 * 1000.0).round() / 1000.0
}

fn cell_to_value(c: &CellValue, ty: QType) -> Value {
    match c {
        CellValue::Empty => Value::Null,
        CellValue::Text(s) => Value::text(s),
        CellValue::Bool(b) => Value::Logical(*b),
        CellValue::Error(_) => Value::Failed(Rc::new(ibis_m_lang::MError::expr("An error value of the cell."))),
        CellValue::Number(n) => match ty {
            QType::Date => Value::Date(epoch() + n.floor() as i64),
            QType::DateTime | QType::DateTimeZone => {
                let day = n.floor();
                Value::DateTime(epoch() + day as i64, seconds(n - day))
            }
            QType::Time => Value::Time(seconds(n - n.floor())),
            QType::Duration => Value::Duration(seconds(*n)),
            _ => Value::Number(*n),
        },
    }
}

pub fn qtable_to_value(t: &QTable) -> Value {
    let rows = t.rows.iter().map(|r| r.iter().zip(&t.columns).map(|(c, col)| cell_to_value(c, col.ty)).collect()).collect();
    Value::Table(Rc::new(Table { columns: t.columns.iter().map(|c| c.name.clone()).collect(), types: t.columns.iter().map(|c| type_of(c.ty)).collect(), rows }))
}

fn value_to_cell(v: &Value) -> Result<CellValue, String> {
    Ok(match v {
        Value::Null => CellValue::Empty,
        Value::Logical(b) => CellValue::Bool(*b),
        // A number that is not one (NaN, infinity) is the error #NUM! in a sheet (Excel).
        Value::Number(n) if !n.is_finite() => CellValue::Error(CellError::Num),
        Value::Number(n) => CellValue::Number(*n),
        Value::Text(s) => CellValue::Text(s.to_string()),
        Value::Date(d) => CellValue::Number((*d - epoch()) as f64),
        Value::DateTime(d, s) => CellValue::Number((*d - epoch()) as f64 + s / 86400.0),
        Value::Time(s) => CellValue::Number(s / 86400.0),
        Value::Duration(s) => CellValue::Number(s / 86400.0),
        Value::Failed(_) => CellValue::Error(CellError::Value),
        other => return Err(other.type_name().to_string()),
    })
}

/// A table value as the query's table. A cell that holds a list, a record, a table or a function cannot be shown in a sheet: the
/// error names its type.
pub fn value_to_qtable(v: &Value) -> Result<QTable, MEvalError> {
    let Value::Table(t) = v else { return Err(MEvalError::NotATable(v.type_name().to_string())) };
    let columns: Vec<QColumn> = t.columns.iter().zip(&t.types).map(|(n, ty)| QColumn { name: n.clone(), ty: qtype_of(ty) }).collect();
    let mut rows = Vec::with_capacity(t.rows.len());
    for r in &t.rows {
        let mut row = Vec::with_capacity(r.len());
        for c in r {
            row.push(value_to_cell(c).map_err(|ty| MEvalError::Error { reason: "Expression.Error".into(), message: format!("A cell of type {ty} cannot be shown in a sheet.") })?);
        }
        rows.push(row);
    }
    Ok(QTable { columns, rows })
}

/// Run M text over the tables of the book (`Excel.CurrentWorkbook()`, by name) and give the table it ends with.
pub fn eval_m(code: &str, book: &[(String, QTable)]) -> Result<QTable, MEvalError> {
    let it = interp();
    let rows = book.iter().map(|(n, t)| vec![Value::text(n), qtable_to_value(t)]).collect();
    it.set_global(
        "#workbook",
        Value::Table(Rc::new(Table {
            columns: vec!["Name".into(), "Content".into()],
            types: vec![TypeSpec { ty: Ty::Text, nullable: false }, TypeSpec { ty: Ty::Table, nullable: false }],
            rows,
        })),
    );
    let v = it.run(code).map_err(|e| match e.kind {
        ErrKind::Syntax => MEvalError::Syntax { line: e.line, message: e.message },
        ErrKind::Unsupported { name, line } => MEvalError::Unsupported { name, line },
        ErrKind::Limit => MEvalError::Limit(e.message),
        ErrKind::Raised => MEvalError::Error { reason: e.reason, message: e.message },
    })?;
    value_to_qtable(&v)
}

/// How a run error of a code step is told to the screen: the error is a text that begins with this mark, then the key of the screen's
/// text table (`query.m-error.<key>`) and its values, each after the mark (`parse_run_error` takes it apart). Words are the screen's, in
/// its language.
const MARK: char = '\u{1}';

fn run_error(key: &str, args: &[(&str, String)]) -> String {
    let mut out = String::new();
    out.push(MARK);
    out.push_str(key);
    for (n, v) in args {
        out.push(MARK);
        out.push_str(n);
        out.push('=');
        out.push_str(v);
    }
    out
}

/// The key and the values of a run error made by a code step, or None for an ordinary message.
pub fn parse_run_error(message: &str) -> Option<(String, Vec<(String, String)>)> {
    let rest = message.strip_prefix(MARK)?;
    let mut parts = rest.split(MARK);
    let key = parts.next()?.to_string();
    let args = parts.filter_map(|p| p.split_once('=').map(|(n, v)| (n.to_string(), v.to_string()))).collect();
    Some((key, args))
}

/// The tables of the book the code names with `Excel.CurrentWorkbook(){[Name="X"]}`: those `resolver` gives.
fn named_tables(code: &str, resolver: &dyn Resolver) -> Vec<(String, QTable)> {
    let mut out: Vec<(String, QTable)> = Vec::new();
    let mut rest = code;
    while let Some(i) = rest.find("[Name=\"") {
        rest = &rest[i + 7..];
        let Some(end) = rest.find('"') else { break };
        let name = rest[..end].to_string();
        if !out.iter().any(|(n, _)| *n == name)
            && let Some(t) = resolver.table(&name)
        {
            out.push((name, t));
        }
    }
    out
}

/// Run a code step: the tables of the steps before it are the names of those steps, and the code is `let bindings in result`.
/// `tables[0]` is the source and `tables[k]` the table after the step `k - 1`.
pub fn run_code(names: &[String], bindings: &str, result: &str, tables: &[QTable], first_line: usize, resolver: &dyn Resolver) -> Result<QTable, String> {
    let it = interp();
    // One name for each table when they are as many; else the source and the last step are the names that are known.
    if names.len() == tables.len() {
        for (n, t) in names.iter().zip(tables) {
            it.set_global(n, qtable_to_value(t));
        }
    } else if let (Some(first), Some(last), Some(t0), Some(tl)) = (names.first(), names.last(), tables.first(), tables.last()) {
        it.set_global(first, qtable_to_value(t0));
        it.set_global(last, qtable_to_value(tl));
    }
    let book = named_tables(bindings, resolver);
    let rows = book.iter().map(|(n, t)| vec![Value::text(n), qtable_to_value(t)]).collect();
    it.set_global(
        "#workbook",
        Value::Table(Rc::new(Table {
            columns: vec!["Name".into(), "Content".into()],
            types: vec![TypeSpec { ty: Ty::Text, nullable: false }, TypeSpec { ty: Ty::Table, nullable: false }],
            rows,
        })),
    );
    let text = format!("let\n{bindings}\nin\n    {result}");
    // Line 1 of the text is `let`; the first binding is line 2, which is `first_line` of the code the user sees.
    let at = |l: u32| (first_line + l.saturating_sub(2) as usize).to_string();
    let v = it.run(&text).map_err(|e| match e.kind {
        ErrKind::Syntax => run_error("syntax", &[("line", at(e.line)), ("message", e.message)]),
        ErrKind::Unsupported { name, line } => run_error("unsupported", &[("line", at(line)), ("name", name)]),
        ErrKind::Limit => run_error("limit", &[("message", e.message)]),
        ErrKind::Raised => run_error("raised", &[("reason", e.reason), ("message", e.message)]),
    })?;
    value_to_qtable(&v).map_err(|e| match e {
        MEvalError::NotATable(t) => run_error("not-table", &[("type", t)]),
        MEvalError::Error { message, .. } => run_error("raised", &[("reason", "Expression.Error".to_string()), ("message", message)]),
        other => run_error("raised", &[("reason", "Expression.Error".to_string()), ("message", format!("{other:?}"))]),
    })
}

/// What reads the files M names: a text file as the source of `Csv.Document` is read, a workbook as `Excel.Workbook` gives it (the reading of
/// a workbook file needs the loader the app gave the model, `set_workbook_loader`).
struct QueryHost;

impl Host for QueryHost {
    fn read_csv(&self, path: &str, o: &CsvOptions) -> Result<Value, String> {
        let table = super::book::read_csv(path, &o.delimiter, o.columns, o.encoding, if o.quoted { "Csv" } else { "None" })?;
        Ok(qtable_to_value(&table))
    }

    fn read_workbook(&self, path: &str) -> Result<Value, String> {
        let items = super::book::workbook_items(path)?;
        let text = |ty: Ty| TypeSpec { ty, nullable: true };
        let rows = items
            .into_iter()
            .map(|(name, sheet, table)| vec![Value::text(&name), qtable_to_value(&table), Value::text(&name), Value::text(if sheet { "Sheet" } else { "Table" }), Value::Logical(false)])
            .collect();
        Ok(Value::Table(Rc::new(Table {
            columns: vec!["Name".into(), "Data".into(), "Item".into(), "Kind".into(), "Hidden".into()],
            types: vec![text(Ty::Text), text(Ty::Table), text(Ty::Text), text(Ty::Text), text(Ty::Logical)],
            rows,
        })))
    }
}

/// An interpreter with the file reading and the clock of the app: `DateTime.LocalNow()` and the `Date.IsIn…` functions use the clock
/// (the same moment all through a run, `DateTime.FixedLocalNow`).
fn interp() -> Interp {
    let it = Interp::new();
    it.set_host(Rc::new(QueryHost));
    if let Some(serial) = ibis_calc::eval::convert::now_serial(false) {
        let day = serial.floor();
        it.set_now(epoch() + day as i64, seconds(serial - day));
    }
    it
}
