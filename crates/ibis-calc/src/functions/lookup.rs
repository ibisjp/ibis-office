// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Lookup and reference functions, and the dynamic array functions.

use std::cmp::Ordering;
use std::rc::Rc;

use super::{Func, arg_array, arg_num, arg_text, array_value, err, number, wildcard_match};
use crate::eval::{Area, Array, Ctx, Value, compare, to_bool, to_number};
use crate::formula::{AreaRef, Corner, Expr, MAX_COLUMN, MAX_ROW, quote_sheet, write_reference};
use crate::value::CellError;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "ROW" => |c, a| row_column(c, a, true),
        "COLUMN" => |c, a| row_column(c, a, false),
        "ROWS" => |c, a| match c.eval(&a[0]) {
            Value::Ref(r) if r.len() == 1 => number(r[0].rows() as f64),
            Value::Array(x) => number(x.rows as f64),
            Value::Error(e) => err(e),
            Value::Ref(_) => err(CellError::Ref),
            _ => number(1.0),
        },
        "COLUMNS" => |c, a| match c.eval(&a[0]) {
            Value::Ref(r) if r.len() == 1 => number(r[0].columns() as f64),
            Value::Array(x) => number(x.columns as f64),
            Value::Error(e) => err(e),
            Value::Ref(_) => err(CellError::Ref),
            _ => number(1.0),
        },
        "AREAS" => |c, a| match c.eval(&a[0]) {
            Value::Ref(r) => number(r.len() as f64),
            Value::Error(e) => err(e),
            _ => err(CellError::Value),
        },
        "ADDRESS" => address,
        "CHOOSE" => |c, a| {
            let i = c.eval_scalar_or_array(&a[0]);
            let date1904 = c.date1904();
            match i {
                Value::Array(arr) => {
                    let data = arr
                        .data
                        .iter()
                        .map(|v| match to_number(v, date1904) {
                            Ok(n) if n >= 1.0 && (n as usize) < a.len() => {
                                let v = c.eval_scalar_or_array(&a[n as usize]);
                                match v {
                                    Value::Array(x) => x.data.first().cloned().unwrap_or(Value::Empty),
                                    v => v,
                                }
                            }
                            Ok(_) => err(CellError::Value),
                            Err(e) => err(e),
                        })
                        .collect();
                    array_value(arr.rows, arr.columns, data)
                }
                v => match to_number(&v, date1904) {
                    Ok(n) if n >= 1.0 && (n.trunc() as usize) < a.len() => match c.eval(&a[n.trunc() as usize]) {
                        Value::Empty => number(0.0),
                        v => v,
                    },
                    Ok(_) => err(CellError::Value),
                    Err(e) => err(e),
                },
            }
        },
        "INDEX" => index,
        "OFFSET" => offset,
        "INDIRECT" => indirect,
        "VLOOKUP" => |c, a| vhlookup(c, a, true),
        "HLOOKUP" => |c, a| vhlookup(c, a, false),
        "LOOKUP" => lookup_fn,
        "MATCH" => match_fn,
        "XLOOKUP" => xlookup,
        "XMATCH" => xmatch,
        "GETPIVOTDATA" => getpivotdata,
        "TRANSPOSE" => |c, a| {
            let val = c.eval(&a[0]);
            let x = match val {
                Value::Ref(areas) => {
                    let v = c.deref_implicit(&areas);
                    match v {
                        Value::Error(e) => return err(e),
                        other => Rc::new(Array::new(1, 1, vec![other])),
                    }
                }
                Value::Array(a) => a,
                Value::Error(e) => return err(e),
                other => Rc::new(Array::new(1, 1, vec![other])),
            };
            let mut data = Vec::with_capacity(x.data.len());
            for col in 0..x.columns {
                for r in 0..x.rows {
                    data.push(x.get(r, col).clone());
                }
            }
            array_value(x.columns, x.rows, data)
        },
        "FORMULATEXT" => |c, a| match c.eval(&a[0]) {
            Value::Ref(r) => match c.book.formula(r[0].sheet, r[0].first_row, r[0].first_column) {
                Some(f) => Value::Text(format!("={f}")),
                None => err(CellError::NA),
            },
            Value::Error(e) => err(e),
            _ => err(CellError::NA),
        },
        "HYPERLINK" => |c, a| {
            let link = c.eval_scalar_or_array(&a[0]);
            if let Value::Error(e) = link {
                return err(e);
            }
            match a.get(1) {
                Some(e) => empty_zero(c.eval_scalar_or_array(e)),
                None => match crate::eval::to_text(&link) {
                    Ok(t) => Value::Text(t),
                    Err(e) => err(e),
                },
            }
        },
        "IMAGE" => |c, a| {
            let v = c.eval_scalar(&a[0]);
            let url = match crate::eval::to_text(&v) {
                Ok(u) => u,
                Err(e) => return err(e),
            };
            if url.is_empty() {
                return err(CellError::Value);
            }
            let alt = match a.get(1) {
                Some(e) if !matches!(e, Expr::Missing) => match crate::eval::to_text(&c.eval_scalar(e)) {
                    Ok(t) => Some(t),
                    Err(e) => return err(e),
                },
                _ => None,
            };
            let sizing = match a.get(2) {
                Some(e) if !matches!(e, Expr::Missing) => match to_number(&c.eval_scalar(e), c.date1904()) {
                    Ok(n) => {
                        let k = n.trunc() as i32;
                        if (0..=3).contains(&k) {
                            k
                        } else {
                            return err(CellError::Value);
                        }
                    }
                    Err(e) => return err(e),
                },
                _ => 0,
            };
            let (mut height, mut width) = (0u32, 0u32);
            if sizing == 3 {
                let h = match a.get(3) {
                    Some(e) if !matches!(e, Expr::Missing) => match to_number(&c.eval_scalar(e), c.date1904()) {
                        Ok(n) if n > 0.0 => n,
                        _ => return err(CellError::Value),
                    },
                    _ => return err(CellError::Value),
                };
                let w = match a.get(4) {
                    Some(e) if !matches!(e, Expr::Missing) => match to_number(&c.eval_scalar(e), c.date1904()) {
                        Ok(n) if n > 0.0 => n,
                        _ => return err(CellError::Value),
                    },
                    _ => return err(CellError::Value),
                };
                height = h.round().min(u32::MAX as f64) as u32;
                width = w.round().min(u32::MAX as f64) as u32;
            }
            c.image = Some(crate::eval::ImageSpec { source: url, alt_text: alt.clone(), sizing: sizing as u8, height, width });
            c.unsupported = true;
            match alt {
                Some(t) => Value::Text(t),
                None => Value::Empty,
            }
        },
        "SORT" => sort,
        "SORTBY" => sortby,
        "UNIQUE" => unique,
        "FILTER" => filter,
        "TAKE" => |c, a| take_drop(c, a, true),
        "DROP" => |c, a| take_drop(c, a, false),
        "EXPAND" => expand,
        "HSTACK" => |c, a| stack(c, a, false),
        "VSTACK" => |c, a| stack(c, a, true),
        "TOCOL" => |c, a| to_line(c, a, true),
        "TOROW" => |c, a| to_line(c, a, false),
        "WRAPROWS" => |c, a| wrap(c, a, true),
        "WRAPCOLS" => |c, a| wrap(c, a, false),
        "CHOOSEROWS" => |c, a| choose_lines(c, a, true),
        "CHOOSECOLS" => |c, a| choose_lines(c, a, false),
        "TRIMRANGE" => trimrange,
        "SINGLE" => |c, a| match c.eval(&a[0]) {
            Value::Ref(r) => c.deref_implicit(&r),
            Value::Array(x) => x.data.first().cloned().unwrap_or(Value::Empty),
            v => v,
        },
        "ANCHORARRAY" => |c, a| match c.eval(&a[0]) {
            Value::Ref(r) if r.len() == 1 && r[0].first_row == r[0].last_row && r[0].first_column == r[0].last_column => {
                let a = r[0];
                if let Value::Error(e) = c.cell_value(a.sheet, a.first_row, a.first_column) {
                    return Value::Error(e);
                }
                match c.spill_area(a.sheet, a.first_row, a.first_column) {
                    Some((r0, c0, r1, c1)) => Value::Ref(std::rc::Rc::new(vec![Area { sheet: a.sheet, last_sheet: a.sheet, first_row: r0, first_column: c0, last_row: r1, last_column: c1 }])),
                    None => Value::Error(CellError::Ref),
                }
            }
            Value::Error(e) => Value::Error(e),
            _ => Value::Error(CellError::Ref),
        },
        _ => return None,
    })
}

fn empty_zero(v: Value) -> Value {
    if matches!(v, Value::Empty) { number(0.0) } else { v }
}

fn row_column(c: &mut Ctx, a: &[Expr], row: bool) -> Value {
    let area = match a.first() {
        None | Some(Expr::Missing) => Area::cell(c.sheet, c.row, c.column),
        Some(e) => match c.eval(e) {
            Value::Ref(r) if r.len() == 1 => r[0],
            Value::Error(e) => return err(e),
            _ => return err(CellError::Value),
        },
    };
    // A range gives an array (the first shows).
    let (first, count) = if row { (area.first_row, area.rows()) } else { (area.first_column, area.columns()) };
    if count == 1 {
        return number(first as f64 + 1.0);
    }
    let count = count.min(1_048_576) as usize;
    let data = (0..count).map(|i| number((first as usize + i + 1) as f64)).collect();
    if row { array_value(count, 1, data) } else { array_value(1, count, data) }
}

fn address(c: &mut Ctx, a: &[Expr]) -> Value {
    let (row, column) = match (arg_num(c, a, 0), arg_num(c, a, 1)) {
        (Ok(r), Ok(col)) => (r.trunc(), col.trunc()),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    let abs = match a.get(2) {
        None | Some(Expr::Missing) => 1.0,
        Some(_) => match arg_num(c, a, 2) {
            Ok(v) => v.trunc(),
            Err(e) => return err(e),
        },
    };
    let a1 = match a.get(3) {
        None | Some(Expr::Missing) => true,
        Some(e) => match to_bool(&c.eval_scalar(e)) {
            Ok(b) => b,
            Err(e) => return err(e),
        },
    };
    let sheet = match a.get(4) {
        None | Some(Expr::Missing) => None,
        Some(_) => match arg_text(c, a, 4) {
            Ok(t) => Some(t),
            Err(e) => return err(e),
        },
    };
    if !(1.0..=4.0).contains(&abs) {
        return err(CellError::Value);
    }
    let (row_fixed_pre, column_fixed_pre) = match abs as u32 {
        1 => (true, true),
        2 => (true, false),
        3 => (false, true),
        _ => (false, false),
    };
    // A fixed (or A1) row or column must be on the sheet.
    let row_bad = row > (MAX_ROW + 1) as f64 || (row < 1.0 && (a1 || row_fixed_pre));
    let column_bad = column > (MAX_COLUMN + 1) as f64 || (column < 1.0 && (a1 || column_fixed_pre));
    if row_bad || column_bad {
        return err(CellError::Value);
    }
    let (row_fixed, column_fixed) = match abs as u32 {
        1 => (true, true),
        2 => (true, false),
        3 => (false, true),
        _ => (false, false),
    };
    let text = if a1 {
        let corner = Corner { row: row as u32 - 1, column: column as u32 - 1, row_fixed, column_fixed };
        write_reference(&AreaRef { sheet: None, start: corner, end: corner, whole_columns: false, whole_rows: false })
    } else {
        let part = |letter: char, v: f64, fixed: bool| if fixed { format!("{letter}{v}") } else if v == 0.0 { letter.to_string() } else { format!("{letter}[{v}]") };
        format!("{}{}", part('R', row, row_fixed), part('C', column, column_fixed))
    };
    Value::Text(match sheet {
        Some(s) if s.is_empty() => format!("!{text}"),
        Some(s) => format!("{}!{text}", quote_sheet(&s)),
        None => text,
    })
}

fn index(c: &mut Ctx, a: &[Expr]) -> Value {
    let num = |c: &mut Ctx, i: usize| -> Result<f64, CellError> {
        match a.get(i) {
            None | Some(Expr::Missing) => Ok(0.0),
            Some(_) => arg_num(c, a, i).map(f64::trunc),
        }
    };
    let (row, column) = match (num(c, 1), num(c, 2)) {
        (Ok(r), Ok(col)) => (r, col),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    if row < 0.0 || column < 0.0 {
        return err(CellError::Value);
    }
    match c.eval(&a[0]) {
        Value::Ref(areas) => {
            let which = match a.get(3) {
                None => 1.0,
                Some(_) => match num(c, 3) {
                    Ok(n) if n >= 1.0 => n,
                    Ok(_) => return err(CellError::Value),
                    Err(e) => return err(e),
                },
            };
            let Some(area) = areas.get(which as usize - 1).copied() else { return err(CellError::Ref) };
            // A single row or column: one number picks along it.
            let (row, column) = if a.len() == 2 && area.rows() == 1 { (1.0, row) } else { (row, column) };
            // One number for a range of several rows and columns.
            if a.len() == 2 && area.rows() > 1 && area.columns() > 1 {
                return err(CellError::Ref);
            }
            if row > area.rows() as f64 || column > area.columns() as f64 {
                return err(CellError::Ref);
            }
            let (r0, r1) = if row == 0.0 { (area.first_row, area.last_row) } else { (area.first_row + row as u32 - 1, area.first_row + row as u32 - 1) };
            let (c0, c1) = if column == 0.0 { (area.first_column, area.last_column) } else { (area.first_column + column as u32 - 1, area.first_column + column as u32 - 1) };
            Value::Ref(Rc::new(vec![Area { first_row: r0, last_row: r1, first_column: c0, last_column: c1, ..area }]))
        }
        Value::Error(e) => err(e),
        v => {
            let arr = match v {
                Value::Array(x) => x,
                other => Rc::new(Array::new(1, 1, vec![other])),
            };
            let (row, column) = if a.len() == 2 && arr.rows == 1 { (1.0, row) } else { (row, column) };
            if row > arr.rows as f64 || column > arr.columns as f64 {
                return err(CellError::Ref);
            }
            match (row as usize, column as usize) {
                (0, 0) => Value::Array(arr),
                (0, col) => array_value(arr.rows, 1, (0..arr.rows).map(|r| arr.get(r, col - 1).clone()).collect()),
                (r, 0) => array_value(1, arr.columns, (0..arr.columns).map(|col| arr.get(r - 1, col).clone()).collect()),
                (r, col) => empty_zero(arr.get(r - 1, col - 1).clone()),
            }
        }
    }
}

fn offset(c: &mut Ctx, a: &[Expr]) -> Value {
    let area = match c.eval(&a[0]) {
        Value::Ref(r) if r.len() == 1 => r[0],
        Value::Error(e) => return err(e),
        _ => return err(CellError::Value),
    };
    let num = |c: &mut Ctx, i: usize, d: f64| -> Result<f64, CellError> {
        match a.get(i) {
            None | Some(Expr::Missing) => Ok(d),
            Some(_) => arg_num(c, a, i).map(f64::trunc),
        }
    };
    let (rows, columns, height, width) = match (num(c, 1, 0.0), num(c, 2, 0.0), num(c, 3, area.rows() as f64), num(c, 4, area.columns() as f64)) {
        (Ok(r), Ok(col), Ok(h), Ok(w)) => (r, col, h, w),
        (Err(e), ..) | (_, Err(e), ..) | (_, _, Err(e), _) | (.., Err(e)) => return err(e),
    };
    if height == 0.0 || width == 0.0 {
        return err(CellError::Ref);
    }
    // A negative height or width reaches up or left.
    let r0 = area.first_row as f64 + rows + if height < 0.0 { height + 1.0 } else { 0.0 };
    let c0 = area.first_column as f64 + columns + if width < 0.0 { width + 1.0 } else { 0.0 };
    let r1 = r0 + height.abs() - 1.0;
    let c1 = c0 + width.abs() - 1.0;
    if r0 < 0.0 || c0 < 0.0 || r1 > MAX_ROW as f64 || c1 > MAX_COLUMN as f64 {
        return err(CellError::Ref);
    }
    Value::Ref(Rc::new(vec![Area { first_row: r0 as u32, first_column: c0 as u32, last_row: r1 as u32, last_column: c1 as u32, ..area }]))
}

fn indirect(c: &mut Ctx, a: &[Expr]) -> Value {
    let text = match arg_text(c, a, 0) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let a1 = match a.get(1) {
        None | Some(Expr::Missing) => true,
        Some(e) => match to_bool(&c.eval_scalar(e)) {
            Ok(b) => b,
            Err(e) => return err(e),
        },
    };
    let formula = if a1 { text.trim().to_string() } else { match r1c1_to_a1(text.trim(), c.row, c.column) {
        Some(t) => t,
        None => return err(CellError::Ref),
    } };
    let Ok(expr) = crate::formula::parse(&formula) else { return err(CellError::Ref) };
    match &expr {
        Expr::Ref(_) | Expr::Name { .. } | Expr::Range(..) => match c.eval(&expr) {
            v @ Value::Ref(_) => v,
            _ => err(CellError::Ref),
        },
        _ => err(CellError::Ref),
    }
}

/// An R1C1 reference (`R2C3`, `R[-1]C`, `Sheet1!R1C1:R2C2`) in A1 form.
fn r1c1_to_a1(text: &str, row: u32, column: u32) -> Option<String> {
    let (sheet, rest) = match text.rsplit_once('!') {
        Some((s, r)) => (Some(s), r),
        None => (None, text),
    };
    let corner = |part: &str| -> Option<String> {
        let upper = part.to_ascii_uppercase();
        let b = upper.as_str();
        let rest = b.strip_prefix('R')?;
        let (r, rest) = parse_offset(rest, row)?;
        let rest = rest.strip_prefix('C')?;
        let (col, rest) = parse_offset(rest, column)?;
        if !rest.is_empty() {
            return None;
        }
        let corner = Corner { row: r.0, column: col.0, row_fixed: r.1, column_fixed: col.1 };
        Some(write_reference(&AreaRef { sheet: None, start: corner, end: corner, whole_columns: false, whole_rows: false }))
    };
    let body = match rest.split_once(':') {
        Some((x, y)) => format!("{}:{}", corner(x)?, corner(y)?),
        None => corner(rest)?,
    };
    Some(match sheet {
        Some(s) => format!("{s}!{body}"),
        None => body,
    })
}

/// `5`, `[-1]` or nothing after R or C: the 0-based index and whether fixed.
fn parse_offset(s: &str, base: u32) -> Option<((u32, bool), &str)> {
    if let Some(rest) = s.strip_prefix('[') {
        let end = rest.find(']')?;
        let n: i64 = rest[..end].parse().ok()?;
        let v = base as i64 + n;
        if v < 0 {
            return None;
        }
        return Some(((v as u32, false), &rest[end + 1..]));
    }
    let digits = s.bytes().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 {
        return Some(((base, false), s));
    }
    let n: u32 = s[..digits].parse().ok()?;
    if n == 0 {
        return None;
    }
    Some(((n - 1, true), &s[digits..]))
}

/// Excel's equality for lookups: case-insensitive, wildcards when asked.
/// Text equal without regard to case, without making lowercase copies
/// (a lookup asks this of every cell it passes).
fn text_equal(a: &str, b: &str) -> bool {
    if a.is_ascii() && b.is_ascii() {
        return a.eq_ignore_ascii_case(b);
    }
    a == b || a.chars().flat_map(char::to_lowercase).eq(b.chars().flat_map(char::to_lowercase))
}

fn lookup_equal(key: &Value, candidate: &Value, wildcards: bool) -> bool {
    match (key, candidate) {
        (Value::Text(k), Value::Text(s)) if wildcards && super::has_wildcards(k) => wildcard_match(k, s),
        (Value::Text(k), Value::Text(s)) => text_equal(k, s),
        (Value::Number(a), Value::Number(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Empty, Value::Empty) => true,
        _ => false,
    }
}

fn same_kind(a: &Value, b: &Value) -> bool {
    matches!((a, b), (Value::Number(_), Value::Number(_)) | (Value::Text(_), Value::Text(_)) | (Value::Bool(_), Value::Bool(_)))
}

/// Excel's binary search for an approximate match (as its results on
/// unsorted data show): bounds inclusive, the middle rounded down, a value
/// of another kind skipped towards the start, an equal value ending the
/// search at once. The last position whose value is ≤ the key (≥ when
/// `descending`).
fn binary_search(keys: &[Value], key: &Value, descending: bool) -> Option<usize> {
    if keys.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (0i64, keys.len() as i64 - 1);
    let mut found = None;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        // A value of the key's kind at or before the middle.
        let mut m = mid;
        while m >= lo && !same_kind(&keys[m as usize], key) {
            m -= 1;
        }
        if m < lo {
            // None there: search after the middle.
            lo = mid + 1;
            continue;
        }
        let ord = compare(&keys[m as usize], key);
        if ord == Ordering::Equal {
            return Some(m as usize);
        }
        let ok = if descending { ord == Ordering::Greater } else { ord == Ordering::Less };
        if ok {
            found = Some(m as usize);
            lo = m + 1;
        } else {
            hi = m - 1;
        }
    }
    found
}

/// A lookup value: one value (the first of an array), empty as 0.
fn lookup_key(c: &mut Ctx, e: &Expr) -> Result<Value, CellError> {
    match c.eval_scalar(e) {
        Value::Error(e) => Err(e),
        Value::Empty => Ok(Value::Number(0.0)),
        v => Ok(v),
    }
}

/// A table argument: a range or an array (text or a logical value typed
/// in is `#VALUE!`).
fn table_arg(c: &mut Ctx, e: &Expr) -> Result<Rc<Array>, CellError> {
    match c.eval_array(e) {
        Value::Array(a) => Ok(a),
        Value::Error(e) => Err(e),
        Value::Number(n) => Ok(Rc::new(Array::new(1, 1, vec![Value::Number(n)]))),
        _ => Err(CellError::Value),
    }
}

fn vhlookup(c: &mut Ctx, a: &[Expr], vertical: bool) -> Value {
    let key = match lookup_key(c, &a[0]) {
        Ok(k) => k,
        Err(e) => return err(e),
    };
    let table = match table_arg(c, &a[1]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let index = match arg_num(c, a, 2) {
        Ok(n) => n.trunc(),
        Err(e) => return err(e),
    };
    let approx = match a.get(3) {
        None => true,
        Some(Expr::Missing) => false,
        Some(e) => match to_bool(&c.eval_scalar(e)) {
            Ok(b) => b,
            Err(e) => return err(e),
        },
    };
    let (count, width) = if vertical { (table.rows, table.columns) } else { (table.columns, table.rows) };
    if index < 1.0 {
        return err(CellError::Value);
    }
    let at = |i: usize, j: usize| if vertical { table.get(i, j).clone() } else { table.get(j, i).clone() };
    let keys: Vec<Value> = (0..count).map(|i| at(i, 0)).collect();
    let found = if approx { binary_search(&keys, &key, false) } else { keys.iter().position(|k| lookup_equal(&key, k, true)) };
    match found {
        Some(_) if index as usize > width => err(CellError::Ref),
        Some(i) => empty_zero(at(i, index as usize - 1)),
        None => err(CellError::NA),
    }
}

fn lookup_fn(c: &mut Ctx, a: &[Expr]) -> Value {
    let key = match lookup_key(c, &a[0]) {
        Ok(k) => k,
        Err(e) => return err(e),
    };
    let first = match c.eval_array(&a[1]) {
        Value::Array(a) => a,
        Value::Error(e) => return err(e),
        v => Rc::new(Array::new(1, 1, vec![v])),
    };
    let (keys, results): (Vec<Value>, Vec<Value>) = match a.get(2) {
        Some(e) => {
            let r = match arg_array(c, e) {
                Ok(t) => t,
                Err(e) => return err(e),
            };
            if r.rows > 1 && r.columns > 1 {
                return err(CellError::NA);
            }
            // A two-dimensional lookup vector: its first column (or row).
            let keys = if first.rows > 1 && first.columns > 1 {
                if first.rows >= first.columns { (0..first.rows).map(|i| first.get(i, 0).clone()).collect() } else { (0..first.columns).map(|j| first.get(0, j).clone()).collect() }
            } else {
                first.data.to_vec()
            };
            (keys, r.data.to_vec())
        }
        None => {
            // Array form: search the first row or column (the longer side), return the last.
            if first.columns > first.rows {
                ((0..first.columns).map(|j| first.get(0, j).clone()).collect(), (0..first.columns).map(|j| first.get(first.rows - 1, j).clone()).collect())
            } else {
                ((0..first.rows).map(|i| first.get(i, 0).clone()).collect(), (0..first.rows).map(|i| first.get(i, first.columns - 1).clone()).collect())
            }
        }
    };
    match binary_search(&keys, &key, false) {
        Some(i) => match results.get(i) {
            Some(v) => v.clone(),
            None => err(CellError::NA),
        },
        None => err(CellError::NA),
    }
}

fn match_fn(c: &mut Ctx, a: &[Expr]) -> Value {
    let key = match lookup_key(c, &a[0]) {
        Ok(k) => k,
        Err(e) => return err(e),
    };
    let arr = match table_arg(c, &a[1]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    if arr.rows > 1 && arr.columns > 1 {
        return err(CellError::NA);
    }
    let kind = match a.get(2) {
        None => 1.0,
        Some(Expr::Missing) => 0.0,
        Some(_) => match arg_num(c, a, 2) {
            Ok(n) => n.signum() * if n == 0.0 { 0.0 } else { 1.0 },
            Err(e) => return err(e),
        },
    };
    let keys = &arr.data;
    let found = match kind {
        k if k > 0.0 => binary_search(keys, &key, false),
        // Smallest value ≥ the key in a descending list: read in order
        // until a smaller value.
        k if k < 0.0 => match keys.iter().position(|k| same_kind(k, &key) && compare(k, &key) == Ordering::Less) {
            Some(0) => None,
            Some(i) => Some(i - 1),
            None => keys.iter().rposition(|k| same_kind(k, &key)),
        },
        _ => keys.iter().position(|k| lookup_equal(&key, k, true)),
    };
    found.map_or(err(CellError::NA), |i| number(i as f64 + 1.0))
}

fn binary_search_x(keys: &[Value], key: &Value, descending: bool, mode: i64) -> Option<usize> {
    if keys.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = (0i64, keys.len() as i64 - 1);
    let mut found = None;
    while lo <= hi {
        let mid = (lo + hi) / 2;
        let mut m = mid;
        while m >= lo && !same_kind(&keys[m as usize], key) {
            m -= 1;
        }
        if m < lo {
            lo = mid + 1;
            continue;
        }
        let ord = compare(&keys[m as usize], key);
        if ord == Ordering::Equal {
            return Some(m as usize);
        }
        if !descending {
            if ord == Ordering::Less {
                if mode == -1 {
                    found = Some(m as usize);
                }
                lo = m + 1;
            } else {
                if mode == 1 {
                    found = Some(m as usize);
                }
                hi = m - 1;
            }
        } else {
            if ord == Ordering::Greater {
                if mode == 1 {
                    found = Some(m as usize);
                }
                lo = m + 1;
            } else {
                if mode == -1 {
                    found = Some(m as usize);
                }
                hi = m - 1;
            }
        }
    }
    found
}

/// XLOOKUP and XMATCH's search: the position of `key` in `keys`.
fn xsearch(keys: &[Value], key: &Value, mode: i64, search: i64) -> Result<Option<usize>, CellError> {
    if !matches!(mode, -1..=3) || !matches!(search, -2 | -1 | 1 | 2) {
        return Err(CellError::Value);
    }
    // Wildcards and patterns cannot be searched for by halves.
    if search.abs() == 2 && mode >= 2 {
        return Err(CellError::Value);
    }
    if search.abs() == 2 {
        // Binary search on sorted keys.
        let descending = search == -2;
        let exact = keys.iter().position(|k| lookup_equal(key, k, false));
        return Ok(match mode {
            0 | 2 => {
                let i = binary_search(keys, key, descending);
                i.filter(|&i| lookup_equal(key, &keys[i], false)).or(exact.filter(|_| false))
            }
            -1 => binary_search_x(keys, key, descending, -1),
            _ => binary_search_x(keys, key, descending, 1),
        });
    }
    let order: Vec<usize> = if search > 0 { (0..keys.len()).collect() } else { (0..keys.len()).rev().collect() };
    if mode == 3 {
        let pat_str = match key {
            Value::Text(t) => t.clone(),
            Value::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    (*n as i64).to_string()
                } else {
                    n.to_string()
                }
            }
            Value::Bool(b) => if *b { "TRUE".to_string() } else { "FALSE".to_string() },
            _ => return Ok(None),
        };
        let re = match regex::RegexBuilder::new(&pat_str).case_insensitive(true).build() {
            Ok(re) => re,
            Err(_) => return Err(CellError::Value),
        };
        for &i in &order {
            let text = match &keys[i] {
                Value::Text(t) => t.clone(),
                Value::Number(n) => {
                    if n.fract() == 0.0 && n.abs() < 1e15 {
                        (*n as i64).to_string()
                    } else {
                        n.to_string()
                    }
                }
                Value::Bool(b) => if *b { "TRUE".to_string() } else { "FALSE".to_string() },
                _ => continue,
            };
            if re.is_match(&text) {
                return Ok(Some(i));
            }
        }
        return Ok(None);
    }
    let wild = mode == 2;
    if let Some(&i) = order.iter().find(|&&i| lookup_equal(key, &keys[i], wild)) {
        return Ok(Some(i));
    }
    if mode == 0 || mode == 2 {
        return Ok(None);
    }
    // The next smaller (-1) or larger (1) value, across kinds in Excel's
    // order (numbers, text, logical values), an empty cell as empty text.
    let as_text = |v: &Value| if matches!(v, Value::Empty) { Value::Text(String::new()) } else { v.clone() };
    let key = &as_text(key);
    let mut best: Option<usize> = None;
    for &i in &order {
        let k = &as_text(&keys[i]);
        if k.is_error() {
            continue;
        }
        let ord = compare(k, key);
        let fits = if mode < 0 { ord == Ordering::Less } else { ord == Ordering::Greater };
        if !fits {
            continue;
        }
        best = match best {
            None => Some(i),
            Some(b) => {
                let kb = as_text(&keys[b]);
                let better = if mode < 0 { compare(k, &kb) == Ordering::Greater } else { compare(k, &kb) == Ordering::Less };
                if better { Some(i) } else { Some(b) }
            }
        };
    }
    Ok(best)
}

fn opt_int(c: &mut Ctx, a: &[Expr], i: usize, d: i64) -> Result<i64, CellError> {
    match a.get(i) {
        None | Some(Expr::Missing) => Ok(d),
        Some(_) => arg_num(c, a, i).map(|n| n.trunc() as i64),
    }
}

/// A value as a table key for an exact search: text without regard to case; `None` for what no key equals.
fn exact_key(v: &Value) -> Option<String> {
    match v {
        Value::Text(t) => Some(format!("t{}", t.chars().flat_map(char::to_lowercase).collect::<String>())),
        Value::Number(n) => Some(format!("n{}", if *n == 0.0 { 0.0f64 } else { *n }.to_bits())),
        Value::Bool(b) => Some(format!("b{}", u8::from(*b))),
        Value::Empty => Some("e".into()),
        _ => None,
    }
}

/// Where each value of `keys` first is, by [`exact_key`].
fn exact_index(keys: &[Value]) -> std::collections::HashMap<String, usize> {
    let mut map = std::collections::HashMap::with_capacity(keys.len());
    for (i, k) in keys.iter().enumerate() {
        if let Some(e) = exact_key(k) {
            map.entry(e).or_insert(i);
        }
    }
    map
}

fn xlookup(c: &mut Ctx, a: &[Expr]) -> Value {
    let lookup_value = c.eval_array(&a[1]);
    let lookup_is_array = matches!(lookup_value, Value::Array(_));
    let keys = match lookup_value {
        Value::Array(t) => t,
        Value::Error(e) => return err(e),
        Value::Lambda(_) => return err(CellError::Value),
        v => Rc::new(Array::new(1, 1, vec![v])),
    };
    let results = match arg_array(c, &a[2]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let (mode, search) = match (opt_int(c, a, 4, 0), opt_int(c, a, 5, 1)) {
        (Ok(m), Ok(s)) => (m, s),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    let vertical = keys.columns == 1;
    if keys.rows > 1 && keys.columns > 1 {
        return err(CellError::Value);
    }
    let mismatch = (vertical && results.rows != keys.rows) || (!vertical && results.columns != keys.columns);
    if lookup_is_array && mismatch {
        return err(CellError::Value);
    }
    let key_value = c.eval_scalar_or_array(&a[0]);
    let not_found = a.get(3).filter(|e| !matches!(e, Expr::Missing)).cloned();
    let one = |c: &mut Ctx, key: &Value| -> Value {
        if let Value::Error(e) = key {
            return err(*e);
        }
        match xsearch(&keys.data, key, mode, search) {
            Err(e) => err(e),
            Ok(Some(_)) if mismatch => err(CellError::Value),
            Ok(Some(i)) => {
                if vertical {
                    if results.columns == 1 {
                        empty_zero(results.get(i, 0).clone())
                    } else {
                        array_value(1, results.columns, (0..results.columns).map(|j| results.get(i, j).clone()).collect())
                    }
                } else if results.rows == 1 {
                    empty_zero(results.get(0, i).clone())
                } else {
                    array_value(results.rows, 1, (0..results.rows).map(|r| results.get(r, i).clone()).collect())
                }
            }
            Ok(None) => match &not_found {
                Some(e) => empty_zero(c.eval_scalar_or_array(e)),
                None => err(CellError::NA),
            },
        }
    };
    // Many keys looked up exactly, first match first: each key's place read from a table made once.
    let index = match &key_value {
        Value::Array(ks) if ks.data.len() >= 16 && mode == 0 && search == 1 => Some(exact_index(&keys.data)),
        _ => None,
    };
    match key_value {
        Value::Array(ks) if index.is_some() => {
            let index = index.unwrap_or_default();
            let data = ks.data.iter().map(|k| match (k, exact_key(k).and_then(|e| index.get(&e))) {
                (Value::Error(e), _) => err(*e),
                (_, Some(&i)) if !mismatch => {
                    if vertical {
                        if results.columns == 1 { empty_zero(results.get(i, 0).clone()) } else { results.get(i, 0).clone() }
                    } else if results.rows == 1 {
                        empty_zero(results.get(0, i).clone())
                    } else {
                        results.get(0, i).clone()
                    }
                }
                (_, Some(_)) => err(CellError::Value),
                _ => match &not_found {
                    Some(e) => match empty_zero(c.eval_scalar_or_array(e)) {
                        Value::Array(x) => x.data.first().cloned().unwrap_or(Value::Empty),
                        v => v,
                    },
                    None => err(CellError::NA),
                },
            }).collect();
            array_value(ks.rows, ks.columns, data)
        }
        Value::Array(ks) => {
            let data = ks.data.iter().map(|k| match one(c, k) {
                Value::Array(x) => x.data.first().cloned().unwrap_or(Value::Empty),
                v => v,
            }).collect();
            array_value(ks.rows, ks.columns, data)
        }
        k => one(c, &k),
    }
}

fn xmatch(c: &mut Ctx, a: &[Expr]) -> Value {
    let keys = match arg_array(c, &a[1]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    if keys.rows > 1 && keys.columns > 1 {
        return err(CellError::Value);
    }
    let (mode, search) = match (opt_int(c, a, 2, 0), opt_int(c, a, 3, 1)) {
        (Ok(m), Ok(s)) => (m, s),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    let key = c.eval_scalar_or_array(&a[0]);
    c.map1(key, |_, k| {
        if let Value::Error(e) = k {
            return err(e);
        }
        match xsearch(&keys.data, &k, mode, search) {
            Ok(Some(i)) => number(i as f64 + 1.0),
            Ok(None) => err(CellError::NA),
            Err(e) => err(e),
        }
    })
}

/// Rows of an array as vectors.
fn rows_of(x: &Array) -> Vec<Vec<Value>> {
    (0..x.rows).map(|r| (0..x.columns).map(|col| x.get(r, col).clone()).collect()).collect()
}

fn from_rows(rows: Vec<Vec<Value>>) -> Value {
    let height = rows.len();
    let width = rows.first().map_or(0, |r| r.len());
    array_value(height, width, rows.into_iter().flatten().collect())
}

fn transpose_rows(rows: Vec<Vec<Value>>) -> Vec<Vec<Value>> {
    let width = rows.first().map_or(0, |r| r.len());
    (0..width).map(|j| rows.iter().map(|r| r[j].clone()).collect()).collect()
}

fn sort_compare(a: &Value, b: &Value) -> Ordering {
    // Errors and empties last; otherwise Excel's order.
    match (a, b) {
        (Value::Error(_), Value::Error(_)) => Ordering::Equal,
        (Value::Error(_), _) => Ordering::Greater,
        (_, Value::Error(_)) => Ordering::Less,
        (Value::Empty, Value::Empty) => Ordering::Equal,
        (Value::Empty, _) => Ordering::Greater,
        (_, Value::Empty) => Ordering::Less,
        _ => compare(a, b),
    }
}

fn sort(c: &mut Ctx, a: &[Expr]) -> Value {
    let x = match arg_array(c, &a[0]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let (index, order, by_column) = match (opt_int(c, a, 1, 1), opt_int(c, a, 2, 1), a.get(3)) {
        (Ok(i), Ok(o), by) => {
            let by_col = match by {
                None | Some(Expr::Missing) => false,
                Some(e) => match to_bool(&c.eval_scalar(e)) {
                    Ok(value) => value,
                    Err(error) => return err(error),
                },
            };
            (i, o, by_col)
        }
        (Err(e), _, _) | (_, Err(e), _) => return err(e),
    };
    if !(order == 1 || order == -1) {
        return err(CellError::Value);
    }
    let mut rows = rows_of(&x);
    if by_column {
        rows = transpose_rows(rows);
    }
    if index < 1 || rows.first().is_some_and(|r| index as usize > r.len()) {
        return err(CellError::Value);
    }
    let k = index as usize - 1;
    rows.sort_by(|p, q| {
        let o = match (&p[k], &q[k]) {
            // The observed ascending SORT order follows ERROR.TYPE codes.
            // Descending and SORTBY keep their existing behaviour until
            // their Office references can be checked independently.
            (Value::Error(a), Value::Error(b)) if order > 0 => a.type_number().unwrap_or(f64::INFINITY).total_cmp(&b.type_number().unwrap_or(f64::INFINITY)),
            _ => sort_compare(&p[k], &q[k]),
        };
        if order < 0 && !matches!((&p[k], &q[k]), (Value::Empty | Value::Error(_), _) | (_, Value::Empty | Value::Error(_))) { o.reverse() } else { o }
    });
    if by_column {
        rows = transpose_rows(rows);
    }
    from_rows(rows)
}

fn sortby(c: &mut Ctx, a: &[Expr]) -> Value {
    let x = match arg_array(c, &a[0]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let mut keys: Vec<(Rc<Array>, i64)> = Vec::new();
    let mut i = 1;
    while i < a.len() {
        let k = match arg_array(c, &a[i]) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let o = match opt_int(c, a, i + 1, 1) {
            Ok(o) if o == 1 || o == -1 => o,
            Ok(_) => return err(CellError::Value),
            Err(e) => return err(e),
        };
        keys.push((k, o));
        i += 2;
    }
    let by_row = keys.first().is_some_and(|(k, _)| k.columns == 1 && k.rows == x.rows);
    let count = if by_row { x.rows } else { x.columns };
    if keys.iter().any(|(k, _)| if by_row { k.columns != 1 || k.rows != x.rows } else { k.rows != 1 || k.columns != x.columns }) {
        return err(CellError::Value);
    }
    let mut idx: Vec<usize> = (0..count).collect();
    idx.sort_by(|&p, &q| {
        for (k, o) in &keys {
            let ord = sort_compare(&k.data[p], &k.data[q]);
            let ord = if *o < 0 { ord.reverse() } else { ord };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    });
    let rows = rows_of(&x);
    if by_row {
        from_rows(idx.into_iter().map(|i| rows[i].clone()).collect())
    } else {
        let cols = transpose_rows(rows);
        from_rows(transpose_rows(idx.into_iter().map(|i| cols[i].clone()).collect()))
    }
}

fn unique(c: &mut Ctx, a: &[Expr]) -> Value {
    let x = match arg_array(c, &a[0]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let flag = |c: &mut Ctx, i: usize| match a.get(i) {
        None | Some(Expr::Missing) => Ok(false),
        Some(e) => to_bool(&c.eval_scalar(e)),
    };
    let (by_col, exactly_once) = match (flag(c, 1), flag(c, 2)) {
        (Ok(p), Ok(q)) => (p, q),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    let mut rows = rows_of(&x);
    if by_col {
        rows = transpose_rows(rows);
    }
    let same = |p: &Vec<Value>, q: &Vec<Value>| p.iter().zip(q).all(|(a, b)| lookup_equal(a, b, false) || (matches!(a, Value::Error(_)) && a == b));
    let mut out: Vec<(Vec<Value>, usize)> = Vec::new();
    for r in rows {
        match out.iter_mut().find(|(o, _)| same(o, &r)) {
            Some(e) => e.1 += 1,
            None => out.push((r, 1)),
        }
    }
    let mut rows: Vec<Vec<Value>> = out.into_iter().filter(|(_, n)| !exactly_once || *n == 1).map(|(r, _)| r).collect();
    if rows.is_empty() {
        return err(CellError::Calc);
    }
    if by_col {
        rows = transpose_rows(rows);
    }
    from_rows(rows)
}

fn filter(c: &mut Ctx, a: &[Expr]) -> Value {
    let x = match arg_array(c, &a[0]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let include = match arg_array(c, &a[1]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let by_row = include.columns == 1 && include.rows == x.rows;
    let by_col = include.rows == 1 && include.columns == x.columns;
    if !by_row && !by_col {
        return err(CellError::Value);
    }
    let mut keep = Vec::new();
    for v in include.data.iter() {
        match v {
            Value::Error(e) => return err(*e),
            Value::Bool(b) => keep.push(*b),
            Value::Number(n) => keep.push(*n != 0.0),
            Value::Empty => keep.push(false),
            _ => return err(CellError::Value),
        }
    }
    let rows = rows_of(&x);
    let out: Vec<Vec<Value>> = if by_row {
        rows.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(r, _)| r).collect()
    } else {
        let cols = transpose_rows(rows);
        let kept: Vec<Vec<Value>> = cols.into_iter().zip(&keep).filter(|(_, k)| **k).map(|(col, _)| col).collect();
        if kept.is_empty() { Vec::new() } else { transpose_rows(kept) }
    };
    if out.is_empty() || out[0].is_empty() {
        return match a.get(2) {
            Some(e) if !matches!(e, Expr::Missing) => c.eval_scalar_or_array(e),
            _ => err(CellError::Calc),
        };
    }
    from_rows(out)
}

fn take_drop(c: &mut Ctx, a: &[Expr], take: bool) -> Value {
    let previous=c.in_array_context;
    c.in_array_context=true;
    let value=c.eval(&a[0]);
    c.in_array_context=previous;
    let area = match (take,&value) {
        (true,Value::Ref(r)) if r.len() == 1 && r[0].sheet == r[0].last_sheet => Some(r[0]),
        _ => None,
    };
    let x=if area.is_some(){None}else{
        let value=match value{Value::Ref(r)=>c.ref_to_array(&r),Value::Omitted=>Value::Empty,v=>v};
        Some(match value{
            Value::Array(a)=>a,
            Value::Error(e)=>return err(e),
            Value::Lambda(_)=>return err(CellError::Value),
            v=>Rc::new(Array::new(1,1,vec![v])),
        })
    };
    let (rows, columns) = match area {
        Some(a) => (a.rows() as usize, a.columns() as usize),
        None => { let x=x.as_ref().unwrap(); (x.rows,x.columns) },
    };
    let rows_arg = match a.get(1) {
        None | Some(Expr::Missing) => None,
        Some(_) => match arg_num(c, a, 1) {
            Ok(n) => Some(n.trunc() as i64),
            Err(e) => return err(e),
        },
    };
    let cols_arg = match a.get(2) {
        None | Some(Expr::Missing) => None,
        Some(_) => match arg_num(c, a, 2) {
            Ok(n) => Some(n.trunc() as i64),
            Err(e) => return err(e),
        },
    };
    let range = |n: Option<i64>, len: usize| -> Option<(usize, usize)> {
        let len = len as i64;
        let (s, e) = match n {
            None => (0, len),
            Some(n) if take => {
                if n >= 0 { (0, n.min(len)) } else { ((len + n).max(0), len) }
            }
            Some(n) => {
                if n >= 0 { (n.min(len), len) } else { (0, (len + n).max(0)) }
            }
        };
        (e > s).then_some((s as usize, e as usize))
    };
    if take && (rows_arg == Some(0) || cols_arg == Some(0)) {
        return err(CellError::Value);
    }
    let (Some((r0, r1)), Some((c0, c1))) = (range(rows_arg, rows), range(cols_arg, columns)) else {
        return err(CellError::Value);
    };
    if let Some(area) = area {
        return Value::Ref(Rc::new(vec![Area {
            first_row: area.first_row+r0 as u32, last_row: area.first_row+r1 as u32-1,
            first_column: area.first_column+c0 as u32, last_column: area.first_column+c1 as u32-1,
            ..area
        }]));
    }
    let x=x.unwrap();
    let mut data = Vec::new();
    for r in r0..r1 {
        for col in c0..c1 {
            data.push(x.get(r, col).clone());
        }
    }
    if r1 - r0 == 1 && c1 - c0 == 1 {
        if let Some(Value::Error(e)) = data.first() {
            return Value::Error(*e);
        }
    }
    array_value(r1 - r0, c1 - c0, data)
}

fn expand(c: &mut Ctx, a: &[Expr]) -> Value {
    let x = match arg_array(c, &a[0]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let dim = |c: &mut Ctx, i: usize, d: usize| -> Result<usize, CellError> {
        match a.get(i) {
            None | Some(Expr::Missing) => Ok(d),
            Some(_) => {
                let n = arg_num(c, a, i)?.trunc();
                if n < d as f64 { Err(CellError::Value) } else { Ok(n as usize) }
            }
        }
    };
    let (rows, columns) = match (dim(c, 1, x.rows), dim(c, 2, x.columns)) {
        (Ok(r), Ok(col)) => (r, col),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    let pad = match a.get(3) {
        None | Some(Expr::Missing) => err(CellError::NA),
        Some(e) => match c.eval(e) {
            Value::Array(_) => return err(CellError::Value),
            Value::Ref(r) => {
                if r.len() == 1 && r[0].is_cell() {
                    c.cell_value(r[0].sheet, r[0].first_row, r[0].first_column)
                } else {
                    c.deref_implicit(&r)
                }
            }
            v => v,
        },
    };
    let Some(len) = rows.checked_mul(columns).filter(|&len| len <= isize::MAX as usize / std::mem::size_of::<Value>()) else {
        return err(CellError::Num);
    };
    let mut data = Vec::with_capacity(len);
    for r in 0..rows {
        for col in 0..columns {
            data.push(if r < x.rows && col < x.columns { x.get(r, col).clone() } else { pad.clone() });
        }
    }
    array_value(rows, columns, data)
}

fn stack(c: &mut Ctx, a: &[Expr], vertical: bool) -> Value {
    let mut parts = Vec::new();
    for e in a {
        match arg_array(c, e) {
            Ok(x) => parts.push(x),
            Err(e) => return err(e),
        }
    }
    let (rows, columns) = if vertical {
        (parts.iter().map(|p| p.rows).sum(), parts.iter().map(|p| p.columns).max().unwrap_or(0))
    } else {
        (parts.iter().map(|p| p.rows).max().unwrap_or(0), parts.iter().map(|p| p.columns).sum())
    };
    let mut data = vec![err(CellError::NA); rows * columns];
    let mut offset = 0;
    for p in &parts {
        for r in 0..p.rows {
            for col in 0..p.columns {
                let (rr, cc) = if vertical { (offset + r, col) } else { (r, offset + col) };
                data[rr * columns + cc] = p.get(r, col).clone();
            }
        }
        offset += if vertical { p.rows } else { p.columns };
    }
    array_value(rows, columns, data)
}

fn to_line(c: &mut Ctx, a: &[Expr], column: bool) -> Value {
    let x = match arg_array(c, &a[0]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let ignore = match opt_int(c, a, 1, 0) {
        Ok(i) if (0..=3).contains(&i) => i,
        Ok(_) => return err(CellError::Value),
        Err(e) => return err(e),
    };
    let by_column = match a.get(2) {
        None | Some(Expr::Missing) => false,
        Some(e) => match to_bool(&c.eval_scalar(e)) {
            Ok(value) => value,
            Err(error) => return err(error),
        },
    };
    let mut values = Vec::new();
    let (outer, inner) = if by_column { (x.columns, x.rows) } else { (x.rows, x.columns) };
    for i in 0..outer {
        for j in 0..inner {
            let v = if by_column { x.get(j, i) } else { x.get(i, j) };
            let skip = match v {
                Value::Empty => ignore == 1 || ignore == 3,
                Value::Error(_) => ignore == 2 || ignore == 3,
                _ => false,
            };
            if !skip {
                values.push(v.clone());
            }
        }
    }
    if values.is_empty() {
        return err(CellError::Value);
    }
    let n = values.len();
    if column { array_value(n, 1, values) } else { array_value(1, n, values) }
}

fn wrap(c: &mut Ctx, a: &[Expr], rows: bool) -> Value {
    let x = match arg_array(c, &a[0]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    if x.rows > 1 && x.columns > 1 {
        return err(CellError::Value);
    }
    let count = match arg_num(c, a, 1) {
        Ok(n) if n.trunc() >= 1.0 => n.trunc() as usize,
        Ok(_) => return err(CellError::Num),
        Err(e) => return err(e),
    };
    let pad = match a.get(2) {
        None | Some(Expr::Missing) => err(CellError::NA),
        Some(e) => c.eval_scalar(e),
    };
    let n = x.data.len();
    if n == 0 {
        return err(CellError::Calc);
    }
    // A count beyond the vector returns one line, without padding it out.
    let count = count.min(n);
    let lines = n.div_ceil(count);
    let mut data = Vec::with_capacity(lines * count);
    if rows {
        for i in 0..lines * count {
            data.push(x.data.get(i).cloned().unwrap_or(pad.clone()));
        }
        array_value(lines, count, data)
    } else {
        for r in 0..count {
            for col in 0..lines {
                let i = col * count + r;
                data.push(x.data.get(i).cloned().unwrap_or(pad.clone()));
            }
        }
        array_value(count, lines, data)
    }
}

fn choose_lines(c: &mut Ctx, a: &[Expr], rows: bool) -> Value {
    let x = match arg_array(c, &a[0]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    // Every index first (an error among them wins over a bad index).
    let mut indexes = Vec::new();
    for e in &a[1..] {
        let idx = match arg_array(c, e) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        for v in idx.data.iter() {
            match to_number(v, false) {
                Ok(n) => indexes.push(n.trunc() as i64),
                Err(e) => return err(e),
            }
        }
    }
    let lines = rows_of(&x);
    let lines = if rows { lines } else { transpose_rows(lines) };
    let mut picked = Vec::new();
    for n in indexes {
        let len = lines.len() as i64;
        let i = if n > 0 { n - 1 } else { len + n };
        if n == 0 || i < 0 || i >= len {
            return err(CellError::Value);
        }
        picked.push(lines[i as usize].clone());
    }
    let out = if rows { picked } else { transpose_rows(picked) };
    from_rows(out)
}

fn trimrange(c: &mut Ctx, a: &[Expr]) -> Value {
    let x = match arg_array(c, &a[0]) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let mut get_mode = |idx: usize| -> Result<i32, CellError> {
        match a.get(idx) {
            None | Some(Expr::Missing) => Ok(3),
            Some(e) => {
                let v = c.eval_scalar(e);
                match to_number(&v, c.date1904()) {
                    Ok(n) => {
                        let k = n.trunc() as i32;
                        if (0..=3).contains(&k) {
                            Ok(k)
                        } else {
                            Err(CellError::Value)
                        }
                    }
                    Err(e) => Err(e),
                }
            }
        }
    };
    let trim_rows = match get_mode(1) {
        Ok(m) => m,
        Err(e) => return err(e),
    };
    let trim_cols = match get_mode(2) {
        Ok(m) => m,
        Err(e) => return err(e),
    };

    let row_empty = |r: usize| -> bool {
        (0..x.columns).all(|col| x.data[r * x.columns + col] == Value::Empty)
    };
    let col_empty = |col: usize| -> bool {
        (0..x.rows).all(|r| x.data[r * x.columns + col] == Value::Empty)
    };

    let mut r_start = 0;
    let mut r_end = x.rows;
    if trim_rows == 1 || trim_rows == 3 {
        while r_start < r_end && row_empty(r_start) {
            r_start += 1;
        }
    }
    if trim_rows == 2 || trim_rows == 3 {
        while r_end > r_start && row_empty(r_end - 1) {
            r_end -= 1;
        }
    }

    let mut c_start = 0;
    let mut c_end = x.columns;
    if trim_cols == 1 || trim_cols == 3 {
        while c_start < c_end && col_empty(c_start) {
            c_start += 1;
        }
    }
    if trim_cols == 2 || trim_cols == 3 {
        while c_end > c_start && col_empty(c_end - 1) {
            c_end -= 1;
        }
    }

    if r_start >= r_end || c_start >= c_end {
        return err(CellError::Ref);
    }

    let rows = r_end - r_start;
    let cols = c_end - c_start;
    let mut data = Vec::with_capacity(rows * cols);
    for r in r_start..r_end {
        for col in c_start..c_end {
            data.push(x.data[r * x.columns + col].clone());
        }
    }
    array_value(rows, cols, data)
}

fn getpivotdata(c: &mut Ctx, args: &[Expr]) -> Value {
    if args.len() < 2 {
        return err(CellError::Value);
    }
    let first = c.eval(&args[0]);
    if let Value::Error(e) = first {
        return err(e);
    }

    let (data_field, sheet, row, col, field_pairs) = match first {
        Value::Ref(areas) => {
            let Some(area) = areas.first() else { return err(CellError::Ref); };
            let desc_val = c.eval_scalar(&args[1]);
            let descriptor = match crate::eval::to_text(&desc_val) {
                Ok(s) => s,
                Err(e) => return err(e),
            };
            let (df, pairs) = parse_pivot_descriptor(&descriptor);
            (df, area.sheet, area.first_row, area.first_column, pairs)
        }
        _ => {
            let df = match crate::eval::to_text(&first) {
                Ok(s) => s,
                Err(e) => return err(e),
            };
            let table_ref = c.eval(&args[1]);
            let Value::Ref(areas) = table_ref else { return err(CellError::Ref); };
            let Some(area) = areas.first() else { return err(CellError::Ref); };

            let pair_args = &args[2..];
            if pair_args.len() % 2 != 0 {
                return err(CellError::Ref);
            }
            let mut pairs = Vec::with_capacity(pair_args.len() / 2);
            for i in 0..(pair_args.len() / 2) {
                let f_val = c.eval_scalar(&pair_args[2 * i]);
                let f_str = match crate::eval::to_text(&f_val) {
                    Ok(s) => s,
                    Err(e) => return err(e),
                };
                let it_val = c.eval_scalar(&pair_args[2 * i + 1]);
                let it_str = match crate::eval::to_text(&it_val) {
                    Ok(s) => s,
                    Err(e) => return err(e),
                };
                pairs.push((f_str, it_str));
            }
            (df, area.sheet, area.first_row, area.first_column, pairs)
        }
    };

    let field_refs: Vec<(&str, &str)> = field_pairs.iter().map(|(f, it)| (f.as_str(), it.as_str())).collect();
    match c.book.pivot_data(sheet, row, col, &data_field, &field_refs) {
        Some(val) => Value::from(val),
        None => err(CellError::Ref),
    }
}

fn parse_pivot_descriptor(desc: &str) -> (String, Vec<(String, String)>) {
    let trimmed = desc.trim();
    if trimmed.contains('[') && trimmed.contains(']') {
        let mut data_field = String::new();
        let mut pairs = Vec::new();
        let mut rest = trimmed;
        while let Some(start) = rest.find('[') {
            let prefix = rest[..start].trim();
            if !prefix.is_empty() && data_field.is_empty() {
                data_field = prefix.to_string();
            }
            let after = &rest[start + 1..];
            let Some(end) = after.find(']') else { break };
            let first_token = &after[..end];
            let remainder = &after[end + 1..];
            if remainder.starts_with(".[") {
                let after2 = &remainder[2..];
                if let Some(end2) = after2.find(']') {
                    let second_token = &after2[..end2];
                    if first_token.eq_ignore_ascii_case("Measures") {
                        data_field = second_token.to_string();
                    } else {
                        pairs.push((first_token.to_string(), second_token.to_string()));
                    }
                    rest = &after2[end2 + 1..];
                    continue;
                }
            } else if data_field.is_empty() {
                data_field = first_token.to_string();
            }
            rest = remainder;
        }
        if data_field.is_empty() && !pairs.is_empty() {
            data_field = pairs.remove(0).1;
        }
        (data_field, pairs)
    } else {
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        if words.is_empty() {
            (String::new(), Vec::new())
        } else if words.len() == 1 {
            (words[0].to_string(), Vec::new())
        } else {
            let data_field = words[0].to_string();
            let mut pairs = Vec::new();
            let rest_words = &words[1..];
            let num_pairs = rest_words.len() / 2;
            for i in 0..num_pairs {
                pairs.push((rest_words[2 * i].to_string(), rest_words[2 * i + 1].to_string()));
            }
            (data_field, pairs)
        }
    }
}


