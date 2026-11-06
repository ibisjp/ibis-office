// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Logical functions, and LET and LAMBDA with the functions that call a
//! LAMBDA for each element.

use std::rc::Rc;

use super::{Func, array_value, err};
use crate::eval::{Array, Ctx, Lambda, Value, compare, to_bool};
use crate::formula::Expr;
use crate::value::CellError;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "TRUE" => |_, _| Value::Bool(true),
        "FALSE" => |_, _| Value::Bool(false),
        "IF" => if_,
        "IFERROR" => |c, a| {
            let v = c.eval_scalar_or_array(&a[0]);
            match v {
                Value::Error(_) => c.eval_scalar_or_array(&a[1]),
                Value::Array(arr) if arr.data.iter().any(|v| v.is_error()) => {
                    let fallback = c.eval_scalar_or_array(&a[1]);
                    let arr = arr.clone();
                    c.map2(Value::Array(arr), fallback, |_, x, y| if x.is_error() { if matches!(y, Value::Empty) { Value::Number(0.0) } else { y } } else { x })
                }
                Value::Empty => Value::Number(0.0),
                v => v,
            }
        },
        "IFNA" => |c, a| {
            let v = c.eval_scalar_or_array(&a[0]);
            match v {
                Value::Error(CellError::NA) => c.eval_scalar_or_array(&a[1]),
                Value::Array(arr) if arr.data.iter().any(|v| *v == Value::Error(CellError::NA)) => {
                    let fallback = c.eval_scalar_or_array(&a[1]);
                    c.map2(Value::Array(arr), fallback, |_, x, y| if x == Value::Error(CellError::NA) { y } else { x })
                }
                Value::Empty => Value::Number(0.0),
                v => v,
            }
        },
        "IFS" => |c, a| {
            if a.len() % 2 != 0 {
                return err(CellError::Value);
            }
            for pair in a.chunks(2) {
                let cond = c.eval_scalar(&pair[0]);
                match to_bool(&cond) {
                    Ok(true) => return selected(c, &pair[1]),
                    Ok(false) => {}
                    Err(e) => return err(e),
                }
            }
            err(CellError::NA)
        },
        "SWITCH" => |c, a| {
            let v = c.eval_scalar_or_array(&a[0]);
            if let Value::Error(e) = v {
                return err(e);
            }
            let rest = &a[1..];
            let pairs = rest.len() / 2;
            for i in 0..pairs {
                let candidate = c.eval_scalar_or_array(&rest[2 * i]);
                if let Value::Error(e) = candidate {
                    return err(e);
                }
                if compare(&v, &candidate) == std::cmp::Ordering::Equal && std::mem::discriminant(&v) == std::mem::discriminant(&candidate)
                    || matches!((&v, &candidate), (Value::Empty, _) | (_, Value::Empty)) && compare(&v, &candidate) == std::cmp::Ordering::Equal
                {
                    return selected(c, &rest[2 * i + 1]);
                }
            }
            if rest.len() % 2 == 1 { selected(c, &rest[rest.len() - 1]) } else { err(CellError::NA) }
        },
        "NOT" => |c, a| {
            let v = c.eval_scalar_or_array(&a[0]);
            c.map1(v, |_, v| match to_bool(&v) {
                Ok(b) => Value::Bool(!b),
                Err(e) => err(e),
            })
        },
        "AND" => |c, a| logical_fold(c, a, |acc, b| acc && b, true),
        "OR" => |c, a| logical_fold(c, a, |acc, b| acc || b, false),
        "XOR" => |c, a| logical_fold(c, a, |acc, b| acc ^ b, false),
        "LET" => let_,
        "LAMBDA" => lambda,
        "MAP" => map,
        "REDUCE" => |c, a| reduce(c, a, false),
        "SCAN" => |c, a| reduce(c, a, true),
        "BYROW" => |c, a| by(c, a, true),
        "BYCOL" => |c, a| by(c, a, false),
        "MAKEARRAY" => makearray,
        _ => return None,
    })
}

/// The result chosen by IFS or SWITCH: a reference stays a reference (as IF's).
fn selected(c: &mut Ctx, e: &Expr) -> Value {
    let v = c.eval(e);
    match v {
        Value::Ref(r) if !c.in_array_context => Value::Ref(r),
        v => empty_as_zero(c.scalar_or_array_of(v)),
    }
}

fn empty_as_zero(v: Value) -> Value {
    if matches!(v, Value::Empty) { Value::Number(0.0) } else { v }
}

fn if_(c: &mut Ctx, a: &[Expr]) -> Value {
    let cond = c.eval_scalar_or_array(&a[0]);
    let branch = |c: &mut Ctx, i: usize, keep_ref: bool| -> Value {
        match a.get(i) {
            // IF(FALSE, 1) is FALSE; IF(FALSE, 1,) is 0.
            None => Value::Bool(i == 1),
            Some(Expr::Missing) => Value::Number(0.0),
            Some(e) => {
                // Evaluated once: a branch that calls itself (a recursive LAMBDA) would otherwise double its work at every level.
                let v = c.eval(e);
                if keep_ref && !c.in_array_context
                    && let Value::Ref(r) = v
                {
                    return Value::Ref(r);
                }
                empty_as_zero(c.scalar_or_array_of(v))
            }
        }
    };
    match cond {
        Value::Array(arr) => {
            // Both branches, chosen element by element.
            let yes = branch(c, 1, false);
            let no = branch(c, 2, false);
            let values = vec![Value::Array(arr), yes, no];
            c.map_values(values, &mut |_, v| match to_bool(&v[0]) {
                Ok(true) => v[1].clone(),
                Ok(false) => v[2].clone(),
                Err(e) => err(e),
            })
        }
        v => match to_bool(&v) {
            Ok(true) => branch(c, 1, true),
            Ok(false) => branch(c, 2, true),
            Err(e) => err(e),
        },
    }
}

/// AND/OR/XOR: logical values and numbers count; text in ranges is left
/// out, typed text is `#VALUE!`; nothing to count is `#VALUE!`.
fn logical_fold(c: &mut Ctx, a: &[Expr], f: impl Fn(bool, bool) -> bool, start: bool) -> Value {
    let mut acc = start;
    let mut any = false;
    for (v, direct) in super::collect_values(c, a, false) {
        match v {
            Value::Error(e) => return err(e),
            Value::Bool(b) => {
                acc = f(acc, b);
                any = true;
            }
            Value::Number(n) => {
                acc = f(acc, n != 0.0);
                any = true;
            }
            Value::Text(t) if direct => {
                if let Ok(b) = to_bool(&Value::Text(t)) {
                    acc = f(acc, b);
                    any = true;
                }
            }
            Value::Empty if direct => {
                acc = f(acc, false);
                any = true;
            }
            _ => {}
        }
    }
    if any { Value::Bool(acc) } else { err(CellError::Value) }
}

fn name_of(e: &Expr) -> Option<String> {
    match e {
        Expr::Name { sheet: None, name } => Some(name.to_string()),
        _ => None,
    }
}

fn let_(c: &mut Ctx, a: &[Expr]) -> Value {
    if a.len() < 3 || a.len() % 2 == 0 {
        return err(CellError::Value);
    }
    let saved = c.locals.len();
    for pair in a[..a.len() - 1].chunks(2) {
        let Some(name) = name_of(&pair[0]) else {
            c.locals.truncate(saved);
            return err(CellError::Name);
        };
        let v = c.eval(&pair[1]);
        c.locals.push((name, v));
    }
    let v = c.eval(&a[a.len() - 1]);
    c.locals.truncate(saved);
    v
}

fn lambda(c: &mut Ctx, a: &[Expr]) -> Value {
    let (params, body) = a.split_at(a.len() - 1);
    let mut names = Vec::new();
    let mut optional = 0;
    for p in params {
        if let Expr::Structured { table, spec } = p
            && table.is_empty()
            && let Some(name) = spec.strip_prefix('[').and_then(|s|s.strip_suffix(']'))
            && matches!(crate::formula::parse(name), Ok(Expr::Name { sheet:None, .. }))
        {
            names.push(name.to_string());
            optional += 1;
            continue;
        }
        if optional > 0 { return err(CellError::Value); }
        match name_of(p) {
            Some(n) => names.push(n),
            None => return err(CellError::Value),
        }
    }
    Value::Lambda(Rc::new(Lambda { params: names, optional, body: body[0].clone(), captured: c.locals.clone(), eta: None }))
}

/// Whether `f` cannot be called with `count` arguments (or is no function at all): the whole result is then #VALUE!.
fn wrong_lambda_arity(f: &Value, count: usize) -> bool {
    match f {
        Value::Lambda(l) => match &l.eta {
            None => count > l.params.len() || count < l.params.len() - l.optional,
            Some(name) => super::table::function_info(name).is_some_and(|i| count < i.min_args as usize || count > i.max_args as usize),
        },
        Value::Error(CellError::Name) => true,
        _ => false,
    }
}

/// An array argument of MAP, BYROW, BYCOL, REDUCE and SCAN. A range is kept
/// as a reference, so the LAMBDA gets references (ISREF, ROW, CELL, ISFORMULA
/// and the like work on its parameters, as in Excel; GitHub #98): `origin` is
/// the range's area, and `arr` holds its cells' values (and the size).
struct Source {
    arr: Rc<Array>,
    origin: Option<crate::eval::Area>,
    /// The argument was one value, not a range or an array.
    scalar: bool,
}

impl Source {
    fn of(c: &mut Ctx, e: &Expr) -> Result<Source, CellError> {
        let v = c.eval(e);
        let (v, origin) = match v {
            Value::Ref(r) if r.len() == 1 && r[0].sheet == r[0].last_sheet => {
                let origin = r[0];
                (c.ref_to_array(&r), Some(origin))
            }
            Value::Ref(r) => (c.ref_to_array(&r), None),
            Value::Omitted => (Value::Empty, None),
            v => (v, None),
        };
        match v {
            Value::Array(arr) => Ok(Source { arr, origin, scalar: false }),
            Value::Error(e) => Err(e),
            // A function value is a value like any other here: one element that the LAMBDA gets (#106).
            v => Ok(Source { arr: Rc::new(Array::new(1, 1, vec![v])), origin: None, scalar: true }),
        }
    }

    /// The cell at (r, col): a reference to it for a range, else its value.
    fn cell(&self, r: usize, col: usize) -> Value {
        match &self.origin {
            Some(a) => Value::Ref(Rc::new(vec![crate::eval::Area::cell(a.sheet, a.first_row + r as u32, a.first_column + col as u32)])),
            None => self.arr.get(r, col).clone(),
        }
    }

    /// Row `i` (or column `i`) as one argument: a reference for a range, else an array.
    fn line(&self, i: usize, rows: bool) -> Value {
        let (rows_n, columns_n) = if rows { (1, self.arr.columns) } else { (self.arr.rows, 1) };
        if let Some(a) = &self.origin {
            let area = if rows {
                crate::eval::Area { first_row: a.first_row + i as u32, last_row: a.first_row + i as u32, ..*a }
            } else {
                crate::eval::Area { first_column: a.first_column + i as u32, last_column: a.first_column + i as u32, ..*a }
            };
            return Value::Ref(Rc::new(vec![area]));
        }
        let data = (0..rows_n * columns_n).map(|k| if rows { self.arr.get(i, k).clone() } else { self.arr.get(k, i).clone() }).collect();
        Value::Array(Rc::new(Array::new(rows_n, columns_n, data)))
    }
}

/// An array holding function values given where a helper wants its LAMBDA (`HSTACK(SUM,MAX)`, `IF({1},SUM)`, `CHOOSE({1,2},SUM,MAX)`).
/// Measured in Excel (#106, 2026-11-04): REDUCE, SCAN, BYROW and BYCOL run once for each element and give the array of the
/// results (the first value of each result when it has several; an element that is no function is #VALUE!); MAP and
/// MAKEARRAY give #VALUE! even for an array of one function.
fn function_array(f: &Value) -> Option<&Rc<Array>> {
    match f {
        Value::Array(x) if x.data.iter().any(|v| matches!(v, Value::Lambda(_))) => Some(x),
        _ => None,
    }
}

/// Run `run` for each function of the array `x` (see `function_array`).
fn lift_functions(c: &mut Ctx, x: &Array, mut run: impl FnMut(&mut Ctx, &Value) -> Value) -> Value {
    let data = x
        .data
        .iter()
        .map(|g| match g {
            Value::Lambda(_) => {
                let v = run(c, g);
                scalar_of(c, v)
            }
            _ => err(CellError::Value),
        })
        .collect();
    array_value(x.rows, x.columns, data)
}

fn map(c: &mut Ctx, a: &[Expr]) -> Value {
    let (arrays, f) = a.split_at(a.len() - 1);
    let f = c.eval(&f[0]);
    let mut xs = Vec::new();
    for (i, e) in arrays.iter().enumerate() {
        match Source::of(c, e) {
            // Only the first argument may be a single value.
            Ok(x) if i > 0 && x.scalar => return err(CellError::Value),
            Ok(x) => xs.push(x),
            Err(e) => return err(e),
        }
    }
    if wrong_lambda_arity(&f, arrays.len()) || function_array(&f).is_some() { return err(CellError::Value); }
    let rows = xs.iter().map(|x| x.arr.rows).max().unwrap_or(1);
    let columns = xs.iter().map(|x| x.arr.columns).max().unwrap_or(1);
    let mut data = Vec::with_capacity(rows * columns);
    for r in 0..rows {
        for col in 0..columns {
            // MAP pairs cells at the same coordinates; single rows/columns do not repeat.
            // Where an argument has no cell, the result is #N/A without calling the LAMBDA.
            if xs.iter().any(|x| r >= x.arr.rows || col >= x.arr.columns) {
                data.push(err(CellError::NA));
                continue;
            }
            let args: Vec<Value> = xs.iter().map(|x| x.cell(r, col)).collect();
            let v = c.invoke_values(&f, args);
            let v = value_of(c, v);
            // A result of several values is #CALC! for the whole (Excel).
            if matches!(&v, Value::Array(x) if x.data.len() != 1) {
                return err(CellError::Calc);
            }
            data.push(scalar_of(c, v));
        }
    }
    array_value(rows, columns, data)
}

/// What a LAMBDA's result is as one element: a reference is its cell's value
/// (the first cell of a range), an array its first element, empty is 0.
fn scalar_of(c: &mut Ctx, v: Value) -> Value {
    match v {
        Value::Ref(r) => {
            let v = c.ref_to_array(&r);
            scalar_of(c, v)
        }
        Value::Array(a) => a.data.first().cloned().unwrap_or(Value::Empty),
        Value::Empty | Value::Omitted => Value::Number(0.0),
        v => v,
    }
}

/// A LAMBDA's result as a value (a reference becomes the cells' values).
fn value_of(c: &mut Ctx, v: Value) -> Value {
    match v {
        Value::Ref(r) => c.ref_to_array(&r),
        v => v,
    }
}

fn reduce(c: &mut Ctx, a: &[Expr], scan: bool) -> Value {
    if a.len() != 3 {
        return err(CellError::Value);
    }
    // The first accumulator is the argument as it is: a range stays a reference.
    let acc = match &a[0] {
        Expr::Missing => Value::Number(0.0),
        e => c.eval(e),
    };
    let src = match Source::of(c, &a[1]) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    let f = c.eval(&a[2]);
    if let Some(x) = function_array(&f) {
        let x = x.clone();
        return lift_functions(c, &x, |c, g| reduce_with(c, acc.clone(), &src, g, scan));
    }
    reduce_with(c, acc, &src, &f, scan)
}

fn reduce_with(c: &mut Ctx, mut acc: Value, src: &Source, f: &Value, scan: bool) -> Value {
    if wrong_lambda_arity(f, 2) { return err(CellError::Value); }
    let (rows, columns) = (src.arr.rows, src.arr.columns);
    let mut steps = Vec::with_capacity(rows * columns);
    for r in 0..rows {
        for col in 0..columns {
            acc = c.invoke_values(f, vec![acc, src.cell(r, col)]);
            if scan {
                // A step of several values is #CALC! for the whole (Excel); REDUCE keeps an array as its accumulator.
                let v = value_of(c, acc.clone());
                if matches!(&v, Value::Array(x) if x.data.len() != 1) {
                    return err(CellError::Calc);
                }
                steps.push(scalar_of(c, v));
            }
        }
    }
    if scan { array_value(rows, columns, steps) } else { acc }
}

fn by(c: &mut Ctx, a: &[Expr], rows: bool) -> Value {
    let src = match Source::of(c, &a[0]) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    let f = c.eval(&a[1]);
    if let Some(x) = function_array(&f) {
        let x = x.clone();
        return lift_functions(c, &x, |c, g| by_with(c, &src, g, rows));
    }
    by_with(c, &src, &f, rows)
}

fn by_with(c: &mut Ctx, src: &Source, f: &Value, rows: bool) -> Value {
    if wrong_lambda_arity(f, 1) { return err(CellError::Value); }
    let count = if rows { src.arr.rows } else { src.arr.columns };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let v = c.invoke_values(f, vec![src.line(i, rows)]);
        let v = value_of(c, v);
        out.push(match v {
            // A row or column of results is not one value: the whole is #CALC! (Excel).
            Value::Array(x) if x.data.len() != 1 => return err(CellError::Calc),
            v => scalar_of(c, v),
        });
    }
    if rows { array_value(count, 1, out) } else { array_value(1, count, out) }
}

fn makearray(c: &mut Ctx, a: &[Expr]) -> Value {
    let rows = c.eval_scalar_or_array(&a[0]);
    let columns = c.eval_scalar_or_array(&a[1]);
    let f = c.eval(&a[2]);
    if wrong_lambda_arity(&f, 2) || function_array(&f).is_some() { return err(CellError::Value); }
    // Where a size is an array, MAKEARRAY runs once for each element and gives the first value of each (Excel, #106);
    // an element that is no number (a function value) is #VALUE!.
    if matches!(rows, Value::Array(_)) || matches!(columns, Value::Array(_)) {
        return c.map_values(vec![rows, columns], &mut |c, vs| match makearray_of(c, &vs[0], &vs[1], &f) {
            Value::Array(x) => x.data.first().cloned().unwrap_or(Value::Empty),
            v => v,
        });
    }
    makearray_of(c, &rows, &columns, &f)
}

fn makearray_of(c: &mut Ctx, rows: &Value, columns: &Value, f: &Value) -> Value {
    let date1904 = c.date1904();
    let (rows, columns) = match (crate::functions::to_number(rows, date1904), crate::functions::to_number(columns, date1904)) {
        (Ok(r), Ok(cl)) => (r, cl),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    if rows < 1.0 || columns < 1.0 || rows.fract() != 0.0 || columns.fract() != 0.0 {
        return err(CellError::Value);
    }
    let (rows, columns) = (rows as usize, columns as usize);
    let Some(len) = rows.checked_mul(columns).filter(|&n| n <= isize::MAX as usize / std::mem::size_of::<Value>()) else { return err(CellError::Num) };
    let mut data = Vec::with_capacity(len);
    for r in 0..rows {
        for col in 0..columns {
            let v = c.invoke_values(f, vec![Value::Number((r + 1) as f64), Value::Number((col + 1) as f64)]);
            // A result of several values is #CALC! (Excel).
            let v = value_of(c, v);
            if matches!(&v, Value::Array(x) if x.data.len() != 1) {
                return err(CellError::Calc);
            }
            data.push(scalar_of(c, v));
        }
    }
    array_value(rows, columns, data)
}
