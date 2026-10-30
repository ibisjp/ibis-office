// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Logical functions, and LET and LAMBDA with the functions that call a
//! LAMBDA for each element.

use std::rc::Rc;

use super::{Func, arg_array, array_value, err};
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
                    Ok(true) => return empty_as_zero(c.eval_scalar_or_array(&pair[1])),
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
                    return empty_as_zero(c.eval_scalar_or_array(&rest[2 * i + 1]));
                }
            }
            if rest.len() % 2 == 1 { empty_as_zero(c.eval_scalar_or_array(&rest[rest.len() - 1])) } else { err(CellError::NA) }
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
                if keep_ref && !c.in_array_context {
                    match c.eval(e) {
                        Value::Ref(r) => return Value::Ref(r),
                        _ => {}
                    }
                }
                empty_as_zero(c.eval_scalar_or_array(e))
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

fn wrong_lambda_arity(f: &Value, count: usize) -> bool {
    matches!(f, Value::Lambda(l) if l.eta.is_none() && (count > l.params.len() || count < l.params.len() - l.optional))
}

fn map(c: &mut Ctx, a: &[Expr]) -> Value {
    let (arrays, f) = a.split_at(a.len() - 1);
    let f = c.eval(&f[0]);
    let mut xs = Vec::new();
    for (i, e) in arrays.iter().enumerate() {
        match c.eval_array(e) {
            Value::Array(x) => xs.push(x),
            Value::Error(e) => return err(e),
            Value::Lambda(_) => return err(CellError::Value),
            v if i == 0 => xs.push(Rc::new(Array::new(1, 1, vec![v]))),
            _ => return err(CellError::Value),
        }
    }
    if wrong_lambda_arity(&f, arrays.len()) { return err(CellError::Value); }
    let rows = xs.iter().map(|x| x.rows).max().unwrap_or(1);
    let columns = xs.iter().map(|x| x.columns).max().unwrap_or(1);
    let mut data = Vec::with_capacity(rows * columns);
    for r in 0..rows {
        for col in 0..columns {
            // MAP pairs cells at the same coordinates; single rows/columns do not repeat.
            let args: Vec<Value> = xs.iter().map(|x| if r < x.rows && col < x.columns { x.get(r, col).clone() } else { err(CellError::NA) }).collect();
            data.push(scalar_of(c.invoke_values(&f, args)));
        }
    }
    array_value(rows, columns, data)
}

fn scalar_of(v: Value) -> Value {
    match v {
        Value::Array(a) => a.data.first().cloned().unwrap_or(Value::Empty),
        Value::Empty | Value::Omitted => Value::Number(0.0),
        v => v,
    }
}

fn reduce(c: &mut Ctx, a: &[Expr], scan: bool) -> Value {
    if a.len() != 3 {
        return err(CellError::Value);
    }
    let mut acc = match &a[0] {
        Expr::Missing => Value::Number(0.0),
        e => match c.eval(e) {
            Value::Ref(r) => c.deref_implicit(&r),
            Value::Array(a) => a.data.first().cloned().unwrap_or(Value::Empty),
            v => v,
        },
    };
    let arr = match arg_array(c, &a[1]) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    let f = c.eval(&a[2]);
    if wrong_lambda_arity(&f, 2) { return err(CellError::Value); }
    let mut steps = Vec::with_capacity(arr.data.len());
    for v in arr.data.iter() {
        acc = c.invoke_values(&f, vec![acc, v.clone()]);
        if scan {
            steps.push(scalar_of(acc.clone()));
        }
    }
    if scan { array_value(arr.rows, arr.columns, steps) } else { acc }
}

fn by(c: &mut Ctx, a: &[Expr], rows: bool) -> Value {
    let arr = match arg_array(c, &a[0]) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    let f = c.eval(&a[1]);
    if wrong_lambda_arity(&f, 1) { return err(CellError::Value); }
    let count = if rows { arr.rows } else { arr.columns };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let slice: Vec<Value> = if rows { (0..arr.columns).map(|j| arr.get(i, j).clone()).collect() } else { (0..arr.rows).map(|j| arr.get(j, i).clone()).collect() };
        let (r, cl) = if rows { (1, arr.columns) } else { (arr.rows, 1) };
        let v = c.invoke_values(&f, vec![Value::Array(Rc::new(Array::new(r, cl, slice)))]);
        out.push(match v {
            Value::Array(x) if x.data.len() != 1 => err(CellError::Calc),
            v => scalar_of(v),
        });
    }
    if rows { array_value(count, 1, out) } else { array_value(1, count, out) }
}

fn makearray(c: &mut Ctx, a: &[Expr]) -> Value {
    let rows = super::arg_num(c, a, 0);
    let columns = super::arg_num(c, a, 1);
    let (rows, columns) = match (rows, columns) {
        (Ok(r), Ok(cl)) => (r, cl),
        (Err(e), _) | (_, Err(e)) => return err(e),
    };
    if rows < 1.0 || columns < 1.0 || rows.fract() != 0.0 || columns.fract() != 0.0 {
        return err(CellError::Value);
    }
    let f = c.eval(&a[2]);
    if wrong_lambda_arity(&f, 2) { return err(CellError::Value); }
    let (rows, columns) = (rows as usize, columns as usize);
    let Some(len) = rows.checked_mul(columns).filter(|&n| n <= isize::MAX as usize / std::mem::size_of::<Value>()) else { return err(CellError::Num) };
    let mut data = Vec::with_capacity(len);
    for r in 0..rows {
        for col in 0..columns {
            let v = c.invoke_values(&f, vec![Value::Number((r + 1) as f64), Value::Number((col + 1) as f64)]);
            if matches!(v, Value::Array(_)) {
                return err(CellError::Value);
            }
            data.push(scalar_of(v));
        }
    }
    array_value(rows, columns, data)
}
