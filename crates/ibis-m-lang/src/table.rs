// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `Table.*` functions (stage 2: the ones the screen's steps write, design 9.3 order 1), the `Replacer`, `Combiner` and
//! `Splitter` makers they take, and the value conversions of `Table.TransformColumnTypes`.
//!
//! Where Excel's behaviour is not established the function does the plain thing and the question is in
//! `tasks/doc/requests/2026-11-17-k0039-m-semantics.md` (the order of `Table.ReorderColumns` with a part of the columns, the sort of text,
//! the order of `Table.Group`, the rounding of `Int64.Type`, the text of a date...).
use crate::ast::{Ty, TypeSpec};
use crate::builtin::{as_number, as_text, list_items};
use crate::culture::{self, Culture};
use crate::error::MError;
use crate::eval::{Interp, row_record};
use crate::value::*;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;

type R = Result<Value, MError>;

pub(crate) fn tbl(v: &Value) -> Result<Rc<Table>, MError> {
    match v {
        Value::Table(t) => Ok(t.clone()),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Table.", v.type_name()))),
    }
}

pub(crate) fn any_ty() -> TypeSpec {
    TypeSpec { ty: Ty::Any, nullable: true }
}

pub(crate) fn make(columns: Vec<String>, types: Vec<TypeSpec>, rows: Vec<Vec<Value>>) -> Value {
    Value::Table(Rc::new(Table { columns, types, rows }))
}

pub(crate) fn same_shape(t: &Table, rows: Vec<Vec<Value>>) -> Value {
    make(t.columns.clone(), t.types.clone(), rows)
}

/// A text or a list of texts.
pub(crate) fn names(it: &Interp, v: &Value) -> Result<Vec<String>, MError> {
    match v {
        Value::Text(s) => Ok(vec![s.to_string()]),
        v => list_items(it, v)?.iter().map(|x| as_text(x).map(|s| s.to_string())).collect(),
    }
}

/// `MissingField.Error` (0, the default), `.Ignore` (1) or `.UseNull` (2).
pub(crate) fn missing_of(a: &[Value], at: usize) -> i32 {
    match a.get(at) {
        Some(Value::Number(n)) => *n as i32,
        _ => 0,
    }
}

pub(crate) fn find_col(t: &Table, name: &str) -> Result<usize, MError> {
    t.columns.iter().position(|c| c == name).ok_or_else(|| MError::expr(format!("The column '{name}' of the table wasn't found.")))
}

pub(crate) fn count_arg(v: &Value) -> Result<usize, MError> {
    let n = as_number(v)?;
    if n < 0.0 || n.fract() != 0.0 {
        return Err(MError::expr("The count must be a non-negative whole number."));
    }
    Ok(n as usize)
}

/// The text of a scalar as `Text.From` writes it in the culture (a column name, a text conversion); None for a value that has no text form.
pub fn scalar_text_in(v: &Value, c: Culture) -> Option<String> {
    Some(match v {
        Value::Text(s) => s.to_string(),
        Value::Number(n) => culture::number_text_in(*n, c),
        Value::Logical(b) => (if *b { "true" } else { "false" }).to_string(),
        Value::Date(d) => culture::date_text(*d, c),
        Value::DateTime(d, s) => culture::datetime_text(*d, *s, c),
        Value::DateTimeZone(d, s, o) => format!("{} {}", culture::datetime_text(*d, *s, c), crate::lib_zone::offset_text(*o, true)),
        Value::Time(s) => culture::time_text(*s, c),
        Value::Duration(s) => culture::duration_text(*s),
        _ => return None,
    })
}

/// `scalar_text_in` in the default culture (ja-JP).
pub fn scalar_text(v: &Value) -> Option<String> {
    scalar_text_in(v, Culture::Ja)
}

/// The rank of a value's type in an order of mixed values: null, time, date, datetime, datetimezone, duration, number, logical, text (measured in Excel:
/// `Table.Sort`, `List.Sort`, `Value.Compare`).
fn type_rank(v: &Value) -> u8 {
    match v {
        Value::Null => 0,
        Value::Time(_) => 1,
        Value::Date(_) => 2,
        Value::DateTime(..) => 3,
        Value::DateTimeZone(..) => 4,
        Value::Duration(_) => 5,
        Value::Number(_) => 6,
        Value::Logical(_) => 7,
        Value::Text(_) => 8,
        _ => 9,
    }
}

/// The order of sorting: values of one type by their order (text by UTF-16 code units, as Excel measured), values of different types by the
/// rank of their type (null first).
pub(crate) fn order(it: &Interp, a: &Value, b: &Value) -> Ordering {
    let (ra, rb) = (type_rank(a), type_rank(b));
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (Value::Null, Value::Null) => Ordering::Equal,
        _ => it.compare(a, b).ok().flatten().unwrap_or(Ordering::Equal),
    }
}

/// A value of a column as the type asks (`Table.TransformColumnTypes`), by the rules Excel gave (the request, sections 5 and 8): a whole
/// number is rounded half to even, a text is read as a number the way `culture::parse_number` says (the empty text is null), a currency
/// has four decimals; a value that cannot be is a failed cell, not a failed table.
/// `offset`: the local offset from UTC in minutes (a value made a datetimezone without an offset of its own gets it).
pub fn convert_value(v: &Value, t: &TypeSpec, c: Culture, offset: i32) -> Value {
    let fail = |reason: &str, msg: String, v: &Value| {
        let mut e = if reason == "DataFormat.Error" { MError::format(msg) } else { MError::expr(msg) };
        e.detail = v.clone();
        Value::Failed(Rc::new(e))
    };
    let bad = |what: &str, v: &Value| fail("DataFormat.Error", format!("We couldn't convert to {what}."), v);
    if matches!(v, Value::Null | Value::Failed(_)) {
        return v.clone();
    }
    // The empty text is null for every type but text.
    if matches!(v, Value::Text(s) if s.is_empty()) && !matches!(t.ty, Ty::Text | Ty::Any | Ty::AnyNonNull) {
        return Value::Null;
    }
    match t.ty {
        Ty::Any | Ty::AnyNonNull | Ty::None | Ty::Null | Ty::Binary | Ty::List | Ty::Record | Ty::Table | Ty::Function | Ty::Type => v.clone(),
        Ty::Text => match scalar_text_in(v, c) {
            Some(s) => Value::text(s),
            None => bad("Text", v),
        },
        Ty::Number | Ty::Currency | Ty::Percentage | Ty::Int64 | Ty::Int32 => {
            let n = match v {
                Value::Number(n) => *n,
                Value::Logical(b) => f64::from(*b),
                Value::Text(s) => match culture::parse_number_in(s, c) {
                    Some(n) => n,
                    None => return bad("Number", v),
                },
                _ => return bad("Number", v),
            };
            // Not a number (NaN, infinity) is nothing in a whole, currency or percentage column (Excel gives null, not an error).
            if !n.is_finite() && !matches!(t.ty, Ty::Number) {
                return Value::Null;
            }
            match t.ty {
                Ty::Int64 => {
                    let r = culture::round_scaled(n, 0, 4);
                    if r.abs() >= 9.223372036854775807e18 { fail("Expression.Error", "The number is outside the range of a 64-bit integer.".into(), v) } else { Value::Number(r) }
                }
                Ty::Int32 => {
                    let r = culture::round_scaled(n, 0, 4);
                    if r.abs() > f64::from(i32::MAX) { fail("Expression.Error", "The number is outside the range of a 32-bit integer.".into(), v) } else { Value::Number(r) }
                }
                Ty::Currency => {
                    if n.abs() > 922337203685477.5807 { fail("Expression.Error", "The number is outside the range of a currency.".into(), v) } else { Value::Number(culture::round_scaled(n, 4, 4)) }
                }
                _ => Value::Number(n),
            }
        }
        Ty::Logical => match v {
            Value::Logical(_) => v.clone(),
            Value::Number(n) => Value::Logical(*n != 0.0),
            Value::Text(s) => match s.trim().to_lowercase().as_str() {
                "true" => Value::Logical(true),
                "false" => Value::Logical(false),
                _ => fail("Expression.Error", "We couldn't convert to Logical.".into(), v),
            },
            _ => fail("Expression.Error", "We couldn't convert to Logical.".into(), v),
        },
        Ty::Date => match v {
            Value::Date(_) => v.clone(),
            Value::DateTime(d, _) | Value::DateTimeZone(d, ..) => Value::Date(*d),
            // A number is a serial: days since 1899-12-30 (`Date.From(46343)`).
            Value::Number(n) => Value::Date(serial_epoch() + n.floor() as i64),
            Value::Text(s) => culture::parse_date_in(s, c, true).map(Value::Date).unwrap_or_else(|| bad("Date", v)),
            _ => bad("Date", v),
        },
        Ty::DateTimeZone => crate::lib_zone::to_zone(v, offset).unwrap_or_else(|| bad("DateTimeZone", v)),
        Ty::DateTime => match v {
            Value::DateTime(..) => v.clone(),
            // The clock time as written (no offset taken off); a text ending in Z is UTC, made the local time.
            Value::DateTimeZone(d, s, _) => Value::DateTime(*d, *s),
            Value::Text(s) if crate::lib_zone::ends_in_z(s) => crate::lib_zone::to_zone(v, offset).map(|z| crate::lib_zone::local_of(&z, offset)).unwrap_or_else(|| bad("DateTime", v)),
            Value::Date(d) => Value::DateTime(*d, 0.0),
            Value::Number(n) => Value::DateTime(serial_epoch() + n.floor() as i64, ((n - n.floor()) * 86400.0 * 1000.0).round() / 1000.0),
            Value::Text(s) => culture::parse_datetime_in(s, c).map(|(d, secs)| Value::DateTime(d, secs)).unwrap_or_else(|| bad("DateTime", v)),
            _ => bad("DateTime", v),
        },
        // A number is a part of a day (0 up to, not including, 1); a date or a duration is not a time.
        Ty::Time => match v {
            Value::Time(_) => v.clone(),
            Value::DateTime(_, s) | Value::DateTimeZone(_, s, _) => Value::Time(*s),
            Value::Number(n) if (0.0..1.0).contains(n) => Value::Time(((n - n.floor()) * 86400.0 * 1000.0).round() / 1000.0),
            Value::Text(s) => culture::parse_time(s, c).map(Value::Time).unwrap_or_else(|| fail("Expression.Error", "The input could not be parsed as a Time.".into(), v)),
            _ => bad("Time", v),
        },
        Ty::Duration => match v {
            Value::Duration(_) => v.clone(),
            Value::Number(n) => Value::Duration(*n * 86400.0),
            Value::Text(s) => culture::parse_duration(s).map(Value::Duration).unwrap_or_else(|| bad("Duration", v)),
            _ => bad("Duration", v),
        },
    }
}

/// 1899-12-30 as days since 0001-01-01: the day of serial 0 of a number turned into a date.
pub(crate) fn serial_epoch() -> i64 {
    days_from_civil(1899, 12, 30)
}

/// The type of a column that holds both: the same type stays; two kinds of number are a number; anything else is any.
pub(crate) fn merge_type(a: &TypeSpec, b: &TypeSpec) -> TypeSpec {
    let numeric = |t: Ty| matches!(t, Ty::Number | Ty::Int64 | Ty::Int32 | Ty::Currency | Ty::Percentage);
    if a.ty == b.ty {
        a.clone()
    } else if numeric(a.ty) && numeric(b.ty) {
        TypeSpec { ty: Ty::Number, nullable: true }
    } else {
        any_ty()
    }
}

pub(crate) fn bool_arg(it: &Interp, f: &Value, row: &Value) -> Result<bool, MError> {
    match it.call(f, vec![row.clone()])? {
        Value::Logical(b) => Ok(b),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Logical.", v.type_name()))),
    }
}

pub(crate) fn reg(l: &mut HashMap<&'static str, Value>, name: &'static str, min: usize, max: usize, f: Builtin) {
    l.insert(name, Value::Function(Rc::new(Func::Builtin { name, min, max, f })));
}

pub(crate) fn native(name: &'static str, min: usize, max: usize, f: impl Fn(&Interp, &[Value]) -> R + 'static) -> Value {
    Value::Function(Rc::new(Func::Native { name, min, max, f: Rc::new(f) }))
}

fn num(n: i32) -> Value {
    Value::Number(f64::from(n))
}

pub fn register(l: &mut HashMap<&'static str, Value>) {
    for (n, v) in [
        ("MissingField.Error", 0),
        ("MissingField.Ignore", 1),
        ("MissingField.UseNull", 2),
        ("Order.Ascending", 0),
        ("Order.Descending", 1),
        ("JoinKind.Inner", 0),
        ("JoinKind.LeftOuter", 1),
        ("JoinKind.RightOuter", 2),
        ("JoinKind.FullOuter", 3),
        ("JoinKind.LeftAnti", 4),
        ("JoinKind.RightAnti", 5),
        ("GroupKind.Global", 0),
        ("GroupKind.Local", 1),
        ("QuoteStyle.None", 0),
        ("QuoteStyle.Csv", 1),
    ] {
        l.insert(n, num(v));
    }
    reg(l, "Table.DemoteHeaders", 1, 1, |_, a| {
        let t = tbl(&a[0])?;
        let mut rows = vec![t.columns.iter().map(|c| Value::text(c)).collect::<Vec<_>>()];
        rows.extend(t.rows.iter().cloned());
        let n = t.columns.len();
        Ok(make((1..=n).map(|i| format!("Column{i}")).collect(), vec![any_ty(); n], rows))
    });
    reg(l, "Table.SelectRows", 2, 2, |it, a| {
        let t = tbl(&a[0])?;
        let mut rows = Vec::new();
        for r in &t.rows {
            if bool_arg(it, &a[1], &row_record(&t, r))? {
                rows.push(r.clone());
            }
        }
        Ok(same_shape(&t, rows))
    });
    reg(l, "Table.SelectColumns", 2, 3, |it, a| pick(it, a, false));
    reg(l, "Table.RemoveColumns", 2, 3, |it, a| pick(it, a, true));
    reg(l, "Table.RenameColumns", 2, 3, |it, a| {
        let t = tbl(&a[0])?;
        let miss = missing_of(a, 2);
        let mut cols = t.columns.clone();
        let pairs: Vec<Vec<Value>> = match &a[1] {
            Value::List(items) if items.first().is_some_and(|x| matches!(x.force(it), Ok(Value::Text(_)))) => vec![list_items(it, &a[1])?],
            v => list_items(it, v)?.iter().map(|p| list_items(it, p)).collect::<Result<_, _>>()?,
        };
        let mut at = Vec::new();
        for p in &pairs {
            let (from, to) = (as_text(&p[0])?, as_text(p.get(1).ok_or_else(|| MError::expr("A rename needs an old and a new name."))?)?);
            match t.columns.iter().position(|c| **c == *from) {
                Some(i) => at.push((i, to.to_string())),
                None if miss == 0 => return Err(MError::expr(format!("The column '{from}' of the table wasn't found."))),
                None => {}
            }
        }
        for (i, to) in at {
            cols[i] = to;
        }
        for (i, c) in cols.iter().enumerate() {
            if cols[..i].contains(c) {
                return Err(MError::expr(format!("The column '{c}' already exists in the table.")));
            }
        }
        Ok(make(cols, t.types.clone(), t.rows.clone()))
    });
    // The order Excel sorts mixed values in, as -1, 0 or 1 (a table, list or record is not one of them: those are an error of the comparison).
    reg(l, "Value.Compare", 2, 3, |it, a| {
        if [&a[0], &a[1]].iter().any(|v| matches!(v, Value::List(_) | Value::Record(_) | Value::Table(_))) {
            return Err(MError::expr("The operator < cannot be applied to these values."));
        }
        Ok(Value::Number(match order(it, &a[0], &a[1]) {
            Ordering::Less => -1.0,
            Ordering::Equal => 0.0,
            Ordering::Greater => 1.0,
        }))
    });
    reg(l, "Table.Sort", 2, 2, |it, a| {
        let t = tbl(&a[0])?;
        let mut keys: Vec<(usize, bool)> = Vec::new();
        let specs: Vec<Value> = match &a[1] {
            Value::Text(_) => vec![a[1].clone()],
            v => {
                let items = list_items(it, v)?;
                // A single {"col", Order.Descending} pair is a spec of its own.
                if items.len() == 2 && matches!(items[0], Value::Text(_)) && matches!(items[1], Value::Number(_)) { vec![v.clone()] } else { items }
            }
        };
        for s in specs {
            match &s {
                Value::Text(n) => keys.push((find_col(&t, n)?, true)),
                v => {
                    let p = list_items(it, v)?;
                    let asc = match p.get(1) {
                        Some(Value::Number(n)) => *n == 0.0,
                        None => true,
                        Some(_) => return Err(MError::expr("The sort order must be Order.Ascending or Order.Descending.")),
                    };
                    keys.push((find_col(&t, &as_text(&p[0])?)?, asc));
                }
            }
        }
        let mut rows = t.rows.clone();
        rows.sort_by(|x, y| {
            for &(i, asc) in &keys {
                let o = order(it, &x[i], &y[i]);
                let o = if asc { o } else { o.reverse() };
                if o != Ordering::Equal {
                    return o;
                }
            }
            Ordering::Equal
        });
        Ok(same_shape(&t, rows))
    });
    reg(l, "Table.FillDown", 2, 2, |it, a| fill(it, a, false));
    reg(l, "Table.FillUp", 2, 2, |it, a| fill(it, a, true));
    reg(l, "Table.FirstN", 2, 2, |it, a| {
        let t = tbl(&a[0])?;
        if let Value::Function(_) = &a[1] {
            let mut rows = Vec::new();
            for r in &t.rows {
                if !bool_arg(it, &a[1], &row_record(&t, r))? {
                    break;
                }
                rows.push(r.clone());
            }
            return Ok(same_shape(&t, rows));
        }
        let n = count_arg(&a[1])?;
        Ok(same_shape(&t, t.rows.iter().take(n).cloned().collect()))
    });
    reg(l, "Table.LastN", 2, 2, |_, a| {
        let t = tbl(&a[0])?;
        let n = count_arg(&a[1])?;
        let skip = t.rows.len().saturating_sub(n);
        Ok(same_shape(&t, t.rows[skip..].to_vec()))
    });
    reg(l, "Table.Skip", 2, 2, |_, a| {
        let t = tbl(&a[0])?;
        let n = count_arg(&a[1])?.min(t.rows.len());
        Ok(same_shape(&t, t.rows[n..].to_vec()))
    });
    reg(l, "Table.RemoveFirstN", 1, 2, |_, a| {
        let t = tbl(&a[0])?;
        let n = a.get(1).map(count_arg).transpose()?.unwrap_or(1).min(t.rows.len());
        Ok(same_shape(&t, t.rows[n..].to_vec()))
    });
    reg(l, "Table.RemoveLastN", 1, 2, |_, a| {
        let t = tbl(&a[0])?;
        let n = a.get(1).map(count_arg).transpose()?.unwrap_or(1);
        let keep = t.rows.len().saturating_sub(n);
        Ok(same_shape(&t, t.rows[..keep].to_vec()))
    });
    reg(l, "Table.Range", 2, 3, |_, a| {
        let t = tbl(&a[0])?;
        let off = count_arg(&a[1])?;
        if off > t.rows.len() {
            return Err(MError::expr("There weren't enough rows in the table."));
        }
        let end = match a.get(2) {
            Some(c) => {
                let c = count_arg(c)?;
                if off + c > t.rows.len() {
                    return Err(MError::expr("There weren't enough rows in the table."));
                }
                off + c
            }
            None => t.rows.len(),
        };
        Ok(same_shape(&t, t.rows[off..end].to_vec()))
    });
    reg(l, "Table.AlternateRows", 4, 4, |_, a| {
        let t = tbl(&a[0])?;
        let (off, skip, take) = (count_arg(&a[1])?, count_arg(&a[2])?, count_arg(&a[3])?);
        let mut rows: Vec<Vec<Value>> = t.rows.iter().take(off).cloned().collect();
        let mut i = off;
        while i < t.rows.len() {
            i += skip;
            for _ in 0..take {
                if let Some(r) = t.rows.get(i) {
                    rows.push(r.clone());
                }
                i += 1;
            }
        }
        Ok(same_shape(&t, rows))
    });
    reg(l, "Table.ReverseRows", 1, 1, |_, a| {
        let t = tbl(&a[0])?;
        Ok(same_shape(&t, t.rows.iter().rev().cloned().collect()))
    });
    reg(l, "Table.RowCount", 1, 1, |_, a| Ok(Value::Number(tbl(&a[0])?.rows.len() as f64)));
    reg(l, "Table.ColumnCount", 1, 1, |_, a| Ok(Value::Number(tbl(&a[0])?.columns.len() as f64)));
    reg(l, "Table.ColumnNames", 1, 1, |_, a| Ok(Value::list(tbl(&a[0])?.columns.iter().map(Value::text).collect())));
    reg(l, "Table.Column", 2, 2, |it, a| {
        let t = tbl(&a[0])?;
        it.field(&Value::Table(t), &as_text(&a[1])?, false)
    });
    reg(l, "Table.AddColumn", 3, 4, |it, a| {
        let t = tbl(&a[0])?;
        let name = as_text(&a[1])?.to_string();
        if t.columns.contains(&name) {
            return Err(MError::expr(format!("The column '{name}' already exists in the table.")));
        }
        let ty = match a.get(3) {
            Some(Value::Type(ts)) => ts.clone(),
            _ => any_ty(),
        };
        let mut rows = Vec::new();
        for r in &t.rows {
            let mut r = r.clone();
            let v = match it.call(&a[2], vec![row_record(&t, &r)]) {
                Ok(v) => v,
                Err(e) if matches!(e.kind, crate::error::ErrKind::Raised) => Value::Failed(Rc::new(e)),
                Err(e) => return Err(e),
            };
            r.push(if matches!(ty.ty, Ty::Any) { v } else { convert_value(&v, &ty, it.culture(), it.utc_offset()) });
            rows.push(r);
        }
        let mut cols = t.columns.clone();
        cols.push(name);
        let mut types = t.types.clone();
        types.push(ty);
        Ok(make(cols, types, rows))
    });
    reg(l, "Table.DuplicateColumn", 3, 4, |_, a| {
        let t = tbl(&a[0])?;
        let i = find_col(&t, &as_text(&a[1])?)?;
        let name = as_text(&a[2])?.to_string();
        if t.columns.contains(&name) {
            return Err(MError::expr(format!("The column '{name}' already exists in the table.")));
        }
        let rows = t.rows.iter().map(|r| r.iter().cloned().chain([r[i].clone()]).collect()).collect();
        let mut cols = t.columns.clone();
        cols.push(name);
        let mut types = t.types.clone();
        types.push(t.types[i].clone());
        Ok(make(cols, types, rows))
    });
    reg(l, "Table.TransformColumns", 2, 4, |it, a| {
        let t = tbl(&a[0])?;
        let miss = missing_of(a, 3);
        let specs: Vec<Vec<Value>> = match &a[1] {
            Value::List(items) if items.first().is_some_and(|x| matches!(x.force(it), Ok(Value::Text(_)))) => vec![list_items(it, &a[1])?],
            v => list_items(it, v)?.iter().map(|p| list_items(it, p)).collect::<Result<_, _>>()?,
        };
        let mut rows = t.rows.clone();
        let mut types = t.types.clone();
        for s in &specs {
            let name = as_text(&s[0])?;
            let Some(i) = t.columns.iter().position(|c| **c == *name) else {
                if miss == 0 { return Err(MError::expr(format!("The column '{name}' of the table wasn't found."))) } else { continue }
            };
            let f = s.get(1).ok_or_else(|| MError::expr("A transformation needs a function."))?;
            for r in &mut rows {
                if matches!(r[i], Value::Failed(_)) {
                    continue;
                }
                r[i] = match it.call(f, vec![r[i].clone()]) {
                    Ok(v) => v,
                    Err(e) if matches!(e.kind, crate::error::ErrKind::Raised) => Value::Failed(Rc::new(e)),
                    Err(e) => return Err(e),
                };
            }
            if let Some(Value::Type(ts)) = s.get(2) {
                types[i] = ts.clone();
                for r in &mut rows {
                    r[i] = convert_value(&r[i], ts, it.culture(), it.utc_offset());
                }
            }
        }
        Ok(make(t.columns.clone(), types, rows))
    });
    reg(l, "Table.FromRows", 1, 2, |it, a| {
        let rows: Vec<Vec<Value>> = list_items(it, &a[0])?.iter().map(|r| list_items(it, r)).collect::<Result<_, _>>()?;
        let n = rows.iter().map(Vec::len).max().unwrap_or(0);
        let cols = match a.get(1) {
            Some(Value::Null) | None => (1..=n).map(|i| format!("Column{i}")).collect(),
            Some(v) => names(it, v)?,
        };
        let w = cols.len();
        let rows = rows.into_iter().map(|mut r| {
            r.resize(w, Value::Null);
            r
        });
        Ok(make(cols, vec![any_ty(); w], rows.collect()))
    });
    reg(l, "Table.FromRecords", 1, 2, |it, a| {
        let recs = list_items(it, &a[0])?;
        let mut cols: Vec<String> = Vec::new();
        for r in &recs {
            let Value::Record(f) = r else { return Err(MError::expr("Table.FromRecords needs records.")) };
            for (n, _) in f.iter() {
                if !cols.contains(n) {
                    cols.push(n.clone());
                }
            }
        }
        let mut rows = Vec::new();
        for r in &recs {
            let Value::Record(f) = r else { unreachable!() };
            let mut row = Vec::new();
            for c in &cols {
                row.push(match f.iter().find(|(n, _)| n == c) {
                    Some((_, l)) => l.force(it)?,
                    None => Value::Null,
                });
            }
            rows.push(row);
        }
        let n = cols.len();
        Ok(make(cols, vec![any_ty(); n], rows))
    });
    reg(l, "Table.FromColumns", 1, 2, |it, a| {
        let cs: Vec<Vec<Value>> = list_items(it, &a[0])?.iter().map(|c| list_items(it, c)).collect::<Result<_, _>>()?;
        let h = cs.iter().map(Vec::len).max().unwrap_or(0);
        let cols = match a.get(1) {
            Some(Value::Null) | None => (1..=cs.len()).map(|i| format!("Column{i}")).collect(),
            Some(v) => names(it, v)?,
        };
        let rows = (0..h).map(|r| cs.iter().map(|c| c.get(r).cloned().unwrap_or(Value::Null)).collect()).collect();
        let n = cols.len();
        Ok(make(cols, vec![any_ty(); n], rows))
    });
    reg(l, "Table.FromList", 1, 4, |it, a| {
        let items = list_items(it, &a[0])?;
        if a.len() > 1 && !matches!(a[1], Value::Null) {
            return Err(MError::unsupported("Table.FromList with a splitter", 0));
        }
        let cols = match a.get(2) {
            Some(Value::Null) | None => vec!["Column1".to_string()],
            Some(v) => names(it, v)?,
        };
        Ok(make(cols, vec![any_ty()], items.into_iter().map(|v| vec![v]).collect()))
    });
    reg(l, "Table.ToRecords", 1, 1, |_, a| {
        let t = tbl(&a[0])?;
        Ok(Value::list(t.rows.iter().map(|r| row_record(&t, r)).collect()))
    });
    reg(l, "Table.ToColumns", 1, 1, |_, a| {
        let t = tbl(&a[0])?;
        Ok(Value::list((0..t.columns.len()).map(|i| Value::list(t.rows.iter().map(|r| r[i].clone()).collect())).collect()))
    });
    reg(l, "Table.InsertRows", 3, 3, |it, a| {
        let t = tbl(&a[0])?;
        let off = count_arg(&a[1])?;
        if off > t.rows.len() {
            return Err(MError::expr("There weren't enough rows in the table."));
        }
        let mut rows = t.rows.clone();
        let new = record_rows(it, &t, &a[2])?;
        rows.splice(off..off, new);
        Ok(same_shape(&t, rows))
    });
    reg(l, "Table.ReplaceRows", 4, 4, |it, a| {
        let t = tbl(&a[0])?;
        let (off, cnt) = (count_arg(&a[1])?, count_arg(&a[2])?);
        if off + cnt > t.rows.len() {
            return Err(MError::expr("There weren't enough rows in the table."));
        }
        let mut rows = t.rows.clone();
        let new = record_rows(it, &t, &a[3])?;
        rows.splice(off..off + cnt, new);
        Ok(same_shape(&t, rows))
    });

    reg(l, "Replacer.ReplaceValue", 3, 3, |it, a| Ok(if it.equals(&a[0], &a[1])? { a[2].clone() } else { a[0].clone() }));
    reg(l, "Combiner.CombineTextByDelimiter", 1, 2, |_, a| {
        let d = as_text(&a[0])?;
        let csv = quote_csv(a.get(1));
        Ok(native("Combiner.CombineTextByDelimiter", 1, 1, move |it, args| {
            let parts: Vec<String> = list_items(it, &args[0])?
                .iter()
                .map(|v| {
                    let t = scalar_text(v).unwrap_or_default();
                    // QuoteStyle.Csv: a part that holds the delimiter, a quote or a line break is quoted.
                    if csv && (t.contains(&*d) || t.contains('"') || t.contains('\n') || t.contains('\r')) { format!("\"{}\"", t.replace('"', "\"\"")) } else { t }
                })
                .collect();
            Ok(Value::text(parts.join(&d)))
        }))
    });
    reg(l, "Splitter.SplitTextByDelimiter", 1, 2, |_, a| {
        let d = as_text(&a[0])?;
        let csv = quote_csv(a.get(1));
        Ok(native("Splitter.SplitTextByDelimiter", 1, 1, move |_, args| match &args[0] {
            Value::Null => Ok(Value::Null),
            v => {
                let s = as_text(v)?;
                if d.is_empty() {
                    return Ok(Value::list(vec![Value::Text(s)]));
                }
                Ok(Value::list(split_delimited(&s, &d, csv).into_iter().map(Value::text).collect()))
            }
        }))
    });
}

/// `QuoteStyle.Csv` (1, the default of the splitter and the combiner) or `QuoteStyle.None` (0).
fn quote_csv(v: Option<&Value>) -> bool {
    !matches!(v, Some(Value::Number(n)) if *n == 0.0)
}

/// Split at the delimiter; with CSV quoting a piece may be quoted (the delimiter inside it does not split, `""` is a quote) and the
/// quotes are taken off.
fn split_delimited(s: &str, d: &str, csv: bool) -> Vec<String> {
    if !csv {
        return s.split(d).map(str::to_string).collect();
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        let c = rest.chars().next().expect("a char");
        if in_q {
            if c == '"' {
                if rest[1..].starts_with('"') {
                    cur.push('"');
                    i += 2;
                } else {
                    in_q = false;
                    i += 1;
                }
            } else {
                cur.push(c);
                i += c.len_utf8();
            }
        } else if c == '"' && cur.is_empty() {
            in_q = true;
            i += 1;
        } else if rest.starts_with(d) {
            out.push(std::mem::take(&mut cur));
            i += d.len();
        } else {
            cur.push(c);
            i += c.len_utf8();
        }
    }
    out.push(cur);
    out
}

pub(crate) fn project(t: &Table, idx: &[usize]) -> Value {
    make(idx.iter().map(|&i| t.columns[i].clone()).collect(), idx.iter().map(|&i| t.types[i].clone()).collect(), t.rows.iter().map(|r| idx.iter().map(|&i| r[i].clone()).collect()).collect())
}

fn pick(it: &Interp, a: &[Value], remove: bool) -> R {
    let t = tbl(&a[0])?;
    let miss = missing_of(a, 2);
    let mut idx = Vec::new();
    let mut null_cols = Vec::new();
    for n in names(it, &a[1])? {
        match t.columns.iter().position(|c| *c == n) {
            Some(i) => idx.push(i),
            None if miss == 0 => return Err(MError::expr(format!("The column '{n}' of the table wasn't found."))),
            None if miss == 2 && !remove => null_cols.push(n),
            None => {}
        }
    }
    if remove {
        let keep: Vec<usize> = (0..t.columns.len()).filter(|i| !idx.contains(i)).collect();
        return Ok(project(&t, &keep));
    }
    let Value::Table(p) = project(&t, &idx) else { unreachable!() };
    if null_cols.is_empty() {
        return Ok(Value::Table(p));
    }
    let mut cols = p.columns.clone();
    let mut types = p.types.clone();
    let mut rows = p.rows.clone();
    for n in null_cols {
        cols.push(n);
        types.push(any_ty());
        for r in &mut rows {
            r.push(Value::Null);
        }
    }
    Ok(make(cols, types, rows))
}

fn fill(it: &Interp, a: &[Value], up: bool) -> R {
    let t = tbl(&a[0])?;
    let mut rows = t.rows.clone();
    for n in names(it, &a[1])? {
        let i = find_col(&t, &n)?;
        let order: Vec<usize> = if up { (0..rows.len()).rev().collect() } else { (0..rows.len()).collect() };
        let mut last = Value::Null;
        for k in order {
            if matches!(rows[k][i], Value::Null) {
                rows[k][i] = last.clone();
            } else {
                last = rows[k][i].clone();
            }
        }
    }
    Ok(same_shape(&t, rows))
}

fn record_rows(it: &Interp, t: &Table, v: &Value) -> Result<Vec<Vec<Value>>, MError> {
    let mut out = Vec::new();
    for r in list_items(it, v)? {
        let Value::Record(_) = &r else { return Err(MError::expr("The rows must be records.")) };
        let mut row = Vec::new();
        for c in &t.columns {
            row.push(it.field(&r, c, true)?);
        }
        out.push(row);
    }
    Ok(out)
}


