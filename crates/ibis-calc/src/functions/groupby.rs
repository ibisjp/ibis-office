// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GROUPBY and PIVOTBY: values grouped by the values of row fields (and,
//! for PIVOTBY, of column fields), each group given to a function (SUM, a
//! LAMBDA…), as a spilled table with totals. Written from Excel's output
//! on the coverage tests (the totals' label is Excel's, in Japanese).

use std::cmp::Ordering;
use std::rc::Rc;

use super::{Func, arg_array, array_value, err};
use crate::eval::{Array, Ctx, Value, compare, to_text};
use crate::formula::Expr;
use crate::value::CellError;

type Res<T> = Result<T, CellError>;

const TOTAL: &str = "合計";
const GRAND_TOTAL: &str = "総計";

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "GROUPBY" => groupby,
        "PIVOTBY" => pivotby,
        _ => return None,
    })
}

// ----- Order and equality of keys ------------------------------------------

fn rank(v: &Value) -> u8 {
    match v {
        Value::Number(_) => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        Value::Error(_) => 3,
        _ => 4,
    }
}

/// Excel's order: numbers, text, logical values, errors (by kind), blanks.
fn order(a: &Value, b: &Value) -> Ordering {
    match (a, b) {
        (Value::Error(x), Value::Error(y)) => x.type_number().unwrap_or(f64::INFINITY).total_cmp(&y.type_number().unwrap_or(f64::INFINITY)),
        _ if rank(a) != rank(b) => rank(a).cmp(&rank(b)),
        (Value::Empty | Value::Omitted, _) => Ordering::Equal,
        _ => compare(a, b),
    }
}

fn same(a: &Value, b: &Value) -> bool {
    order(a, b) == Ordering::Equal
}

fn same_keys(a: &[Value], b: &[Value]) -> bool {
    a.iter().zip(b).all(|(x, y)| same(x, y))
}

// ----- Arguments ------------------------------------------------------------

fn given(a: &[Expr], i: usize) -> Option<&Expr> {
    a.get(i).filter(|e| !matches!(e, Expr::Missing))
}

/// An integer option: `None` when left out; text, logical values, blanks and fractions are `#VALUE!`.
fn int_option(c: &mut Ctx, a: &[Expr], i: usize) -> Res<Option<i64>> {
    let Some(e) = given(a, i) else { return Ok(None) };
    match c.eval_scalar(e) {
        Value::Number(n) if n.fract() == 0.0 && n.abs() < 1e9 => Ok(Some(n as i64)),
        Value::Error(e) => Err(e),
        _ => Err(CellError::Value),
    }
}

/// The columns a sort goes by (1-based, negative for descending): a number or an array of them.
fn sort_option(c: &mut Ctx, a: &[Expr], i: usize, columns: usize) -> Res<Option<Vec<i64>>> {
    let Some(e) = given(a, i) else { return Ok(None) };
    let arr = arg_array(c, e)?;
    let mut out = Vec::new();
    for v in &arr.data {
        match v {
            Value::Number(n) if n.fract() == 0.0 && *n != 0.0 && (n.abs() as usize) <= columns => out.push(*n as i64),
            Value::Error(e) => return Err(*e),
            _ => return Err(CellError::Value),
        }
    }
    Ok(Some(out))
}

/// The rows to take: logical values or numbers (not 0), one for each, or one for all.
fn filter_option(c: &mut Ctx, a: &[Expr], i: usize, rows: usize) -> Res<Vec<bool>> {
    let Some(e) = given(a, i) else { return Ok(vec![true; rows]) };
    let arr = arg_array(c, e)?;
    let one = |v: &Value| -> Res<bool> {
        match v {
            Value::Number(n) => Ok(*n != 0.0),
            Value::Bool(b) => Ok(*b),
            Value::Error(e) => Err(*e),
            _ => Err(CellError::Value),
        }
    };
    if arr.data.len() == 1 {
        return Ok(vec![one(&arr.data[0])?; rows]);
    }
    if arr.columns != 1 || arr.rows != rows {
        return Err(CellError::Value);
    }
    arr.data.iter().map(one).collect()
}

/// The function (or functions) to give each group to: one, or a row (`HSTACK(SUM,MAX)`)
/// or column (`VSTACK(SUM,MAX)`) of them, as function names or LAMBDAs.
struct Funcs {
    list: Vec<Value>,
    /// A row of functions (`Some(false)`) or a column (`Some(true)`), not one.
    array: Option<bool>,
}

impl Funcs {
    /// The name Excel shows over a column of a function's results: the function's name, empty for a LAMBDA.
    fn name(&self, i: usize) -> Value {
        match &self.list[i] {
            Value::Lambda(l) => Value::Text(l.eta.clone().unwrap_or_default()),
            _ => Value::Text(String::new()),
        }
    }
}

fn function_arg(c: &mut Ctx, e: &Expr) -> Res<Funcs> {
    match c.eval(e) {
        f @ Value::Lambda(_) => Ok(Funcs { list: vec![f], array: None }),
        Value::Array(x) if x.rows == 1 || x.columns == 1 => {
            if !x.data.iter().all(|v| matches!(v, Value::Lambda(_))) {
                return Err(x.data.iter().find_map(|v| if let Value::Error(e) = v { Some(*e) } else { None }).unwrap_or(CellError::Value));
            }
            Ok(Funcs { list: x.data.clone(), array: Some(x.columns == 1 && x.rows > 1) })
        }
        Value::Error(e) => Err(e),
        _ => Err(CellError::Value),
    }
}

/// Which function takes which column of values: one function takes every column;
/// several take one column each, or all the one column there is; otherwise none (#VALUE!).
fn function_plan(funcs: &Funcs, columns: usize) -> Res<Vec<(usize, usize)>> {
    let n = funcs.list.len();
    if n == 1 {
        Ok((0..columns).map(|j| (0, j)).collect())
    } else if columns == 1 {
        Ok((0..n).map(|i| (i, 0)).collect())
    } else if n == columns {
        Ok((0..n).map(|i| (i, i)).collect())
    } else {
        Err(CellError::Value)
    }
}

/// `f` of one column of values.
fn aggregate(c: &mut Ctx, f: &Value, column: Vec<Value>) -> Value {
    let all = column.clone();
    aggregate_in(c, f, column, all)
}

/// `f` of one column of values, `all` being the column it is a part of (for PERCENTOF).
fn aggregate_in(c: &mut Ctx, f: &Value, column: Vec<Value>, all: Vec<Value>) -> Value {
    let array_of = |column: Vec<Value>| {
        let n = column.len().max(1);
        Value::Array(Rc::new(Array::new(n, 1, if column.is_empty() { vec![Value::Empty] } else { column })))
    };
    let mut args = vec![array_of(column)];
    let mut eta = false;
    if let Value::Lambda(l) = f {
        if l.eta.is_none() && l.params.len() != 1 {
            return Value::Error(CellError::Value);
        }
        eta = l.eta.is_some();
        if l.eta.as_deref() == Some("PERCENTOF") {
            args.push(array_of(all));
        }
    }
    match c.invoke_values(f, args) {
        // A function that gives an array for an array (SIN) is not one that adds up a group.
        Value::Array(_) if eta => Value::Error(CellError::Value),
        Value::Array(x) if x.data.len() == 1 => x.data[0].clone(),
        Value::Array(_) => Value::Error(CellError::Calc),
        Value::Ref(_) => Value::Error(CellError::Value),
        v => v,
    }
}

fn column_of(arr: &Array, col: usize, rows: &[usize]) -> Vec<Value> {
    rows.iter().map(|&r| arr.get(r, col).clone()).collect()
}

/// A total depth: 0 none, 1 at the end, -1 at the start; 2 and -2 add subtotals, with more than one field.
fn total_depth(c: &mut Ctx, a: &[Expr], i: usize, fields: usize) -> Res<i64> {
    let d = int_option(c, a, i)?.unwrap_or(1);
    match d {
        0 | 1 | -1 => Ok(d),
        2 | -2 if fields >= 2 => Ok(d),
        _ => Err(CellError::Value),
    }
}

/// Groups of rows by their keys, in the order of their first row.
fn group_rows(keys: &Array, rows: &[usize]) -> Vec<(Vec<Value>, Vec<usize>)> {
    let mut groups: Vec<(Vec<Value>, Vec<usize>)> = Vec::new();
    for &r in rows {
        let key: Vec<Value> = (0..keys.columns).map(|k| keys.get(r, k).clone()).collect();
        match groups.iter_mut().find(|(k, _)| same_keys(k, &key)) {
            Some((_, members)) => members.push(r),
            None => groups.push((key, vec![r])),
        }
    }
    groups
}

fn by_keys(a: &[Value], b: &[Value]) -> Ordering {
    a.iter().zip(b).map(|(x, y)| order(x, y)).find(|o| *o != Ordering::Equal).unwrap_or(Ordering::Equal)
}

/// A sort of columns of values (negative: down, the blanks and errors still last).
fn by_columns(spec: &[i64], a: &[Value], b: &[Value]) -> Ordering {
    for &s in spec {
        let col = s.unsigned_abs() as usize - 1;
        let o = order(&a[col], &b[col]);
        let o = if s < 0 && !matches!((&a[col], &b[col]), (Value::Empty | Value::Error(_), _) | (_, Value::Empty | Value::Error(_))) { o.reverse() } else { o };
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// A header's text: a number as text.
fn header_text(v: &Value) -> Value {
    match v {
        Value::Number(_) => to_text(v).map(Value::Text).unwrap_or_else(|e| Value::Error(e)),
        other => other.clone(),
    }
}

/// How the header rows of the data are taken: not there, there and not shown, generated, or there and shown.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Headers {
    None,
    Hidden,
    Generated,
    Shown,
}

fn headers_option(c: &mut Ctx, a: &[Expr], i: usize, first_row_is_text: impl Fn() -> bool) -> Res<Headers> {
    Ok(match int_option(c, a, i)? {
        None => if first_row_is_text() { Headers::Shown } else { Headers::None },
        Some(0) => Headers::None,
        Some(1) => Headers::Hidden,
        Some(2) => Headers::Generated,
        Some(3) => Headers::Shown,
        _ => return Err(CellError::Value),
    })
}

// ----- GROUPBY --------------------------------------------------------------

fn groupby(c: &mut Ctx, a: &[Expr]) -> Value {
    match groupby_of(c, a) {
        Ok(v) => v,
        Err(e) => err(e),
    }
}

fn groupby_of(c: &mut Ctx, a: &[Expr]) -> Res<Value> {
    let rf = arg_array(c, &a[0])?;
    let vals = arg_array(c, &a[1])?;
    let funcs = function_arg(c, &a[2])?;
    if rf.rows != vals.rows {
        return Err(CellError::Value);
    }
    let (k, v) = (rf.columns, vals.columns);
    let plan = function_plan(&funcs, v)?;
    let w = plan.len();
    let text_row = || rf.rows >= 2 && (0..v).all(|j| matches!(vals.get(0, j), Value::Text(_))) && (0..v).all(|j| !matches!(vals.get(1, j), Value::Text(_)));
    let headers = headers_option(c, a, 3, text_row)?;
    let depth = total_depth(c, a, 4, k)?;
    // A column of functions has one column of values, so the sort goes by the keys and that one.
    let spec = sort_option(c, a, 5, if funcs.array == Some(true) { k + 1 } else { k + w })?;
    let filter = filter_option(c, a, 6, rf.rows)?;
    let table = match int_option(c, a, 7)? {
        None | Some(0) => false,
        Some(1) => true,
        _ => return Err(CellError::Value),
    };
    // Subtotals are of a hierarchy only.
    if table && depth.abs() == 2 {
        return Err(CellError::Value);
    }
    let skip = usize::from(matches!(headers, Headers::Hidden | Headers::Shown));
    let rows: Vec<usize> = (skip..rf.rows).filter(|&r| filter[r]).collect();
    if rows.is_empty() {
        return Err(CellError::Calc);
    }
    let mut groups = group_rows(&rf, &rows);
    // Each group's values.
    let mut table_rows: Vec<(Vec<Value>, Vec<Value>, Vec<usize>)> = Vec::new();
    for (key, members) in groups.drain(..) {
        let values: Vec<Value> = plan.iter().map(|&(i, j)| aggregate_in(c, &funcs.list[i], column_of(&vals, j, &members), column_of(&vals, j, &rows))).collect();
        table_rows.push((key, values, members));
    }
    match &spec {
        Some(spec) => table_rows.sort_by(|x, y| {
            let (p, q): (Vec<Value>, Vec<Value>) = (x.0.iter().chain(&x.1).cloned().collect(), y.0.iter().chain(&y.1).cloned().collect());
            by_columns(spec, &p, &q)
        }),
        None => table_rows.sort_by(|x, y| by_keys(&x.0, &y.0)),
    }
    let mut out: Vec<Vec<Value>> = Vec::new();
    // A row of functions: the names over the columns of their results (Excel).
    if funcs.array == Some(false) {
        let mut row: Vec<Value> = (0..k).map(|_| Value::Text(String::new())).collect();
        row.extend((0..w).map(|n| funcs.name(plan[n].0)));
        out.push(row);
    }
    match headers {
        Headers::Generated => {
            let mut row: Vec<Value> = (1..=k).map(|i| Value::Text(format!("行フィールド {i}"))).collect();
            row.extend((1..=w).map(|j| Value::Text(format!("値 {j}"))));
            out.push(row);
        }
        Headers::Shown => {
            let mut row: Vec<Value> = (0..k).map(|i| rf.get(0, i).clone()).collect();
            row.extend(plan.iter().map(|&(_, j)| vals.get(0, j).clone()));
            out.push(row);
        }
        _ => {}
    }
    let blank = || Value::Text(String::new());
    let total = |c: &mut Ctx, label: Value, members: &[usize]| -> Vec<Value> {
        let mut row = vec![label];
        row.extend((1..k).map(|_| blank()));
        row.extend(plan.iter().map(|&(i, j)| aggregate_in(c, &funcs.list[i], column_of(&vals, j, members), column_of(&vals, j, &rows))));
        row
    };
    // The grand total is "総計" where subtotals are, "合計" otherwise.
    let grand_label = if depth.abs() == 2 { GRAND_TOTAL } else { TOTAL };
    let grand = (depth != 0).then(|| total(c, Value::Text(grand_label.into()), &rows));
    if let (Some(g), true) = (&grand, depth < 0) {
        out.push(g.clone());
    }
    let subtotals = depth.abs() == 2;
    let mut i = 0;
    while i < table_rows.len() {
        // A subtotal covers the rows of one value of the first field; it comes before them when the totals do.
        let top = table_rows[i].0[0].clone();
        let end = (i..table_rows.len()).find(|&j| !same(&table_rows[j].0[0], &top)).unwrap_or(table_rows.len());
        if subtotals && depth < 0 {
            let members: Vec<usize> = table_rows[i..end].iter().flat_map(|r| r.2.iter().copied()).collect();
            out.push(total(c, top.clone(), &members));
        }
        for (key, values, _) in &table_rows[i..end] {
            let mut row: Vec<Value> = key.clone();
            row.extend(values.iter().cloned());
            out.push(row);
        }
        if subtotals && depth > 0 {
            let members: Vec<usize> = table_rows[i..end].iter().flat_map(|r| r.2.iter().copied()).collect();
            out.push(total(c, top.clone(), &members));
        }
        i = end;
    }
    if let (Some(g), true) = (&grand, depth > 0) {
        out.push(g.clone());
    }
    // A column of functions: a row for each function, after the keys, with its name.
    if funcs.array == Some(true) {
        // The row of headers (if any) stays one row, with an empty name.
        let header_rows = usize::from(matches!(headers, Headers::Generated | Headers::Shown));
        out = out
            .into_iter()
            .enumerate()
            .flat_map(|(at, row)| {
                if at < header_rows {
                    let mut r: Vec<Value> = row[..k].to_vec();
                    r.push(Value::Text(String::new()));
                    r.push(row[k].clone());
                    return vec![r];
                }
                (0..w).map(|n| {
                    let mut r: Vec<Value> = row[..k].to_vec();
                    r.push(funcs.name(plan[n].0));
                    r.push(row[k + n].clone());
                    r
                }).collect::<Vec<_>>()
            })
            .collect();
    }
    Ok(rows_to_array(out))
}

type Group = (Vec<Value>, Vec<usize>, Vec<Value>);

/// Groups in the order of a sort: of the keys, or of the columns of keys and the group's total (`spec`). Where the groups
/// have subtotals by the first key (`nested`), Excel orders those groups by the first key and the subtotal and, within each,
/// the others by the remaining keys and their totals.
fn sorted_groups(c: &mut Ctx, funcs: &[Value], vals: &Array, mut groups: Vec<Group>, spec: &Option<Vec<i64>>, nested: bool) -> Vec<Group> {
    let flat = |g: &Group| g.0.iter().cloned().chain(g.2.iter().cloned()).collect::<Vec<_>>();
    let Some(spec) = spec else {
        groups.sort_by(|x, y| by_keys(&x.0, &y.0));
        return groups;
    };
    if !nested || groups.first().is_none_or(|g| g.0.len() < 2) {
        groups.sort_by(|x, y| by_columns(spec, &flat(x), &flat(y)));
        return groups;
    }
    let mut tops: Vec<(Vec<Value>, Vec<Group>)> = Vec::new();
    for g in groups {
        match tops.iter_mut().find(|(k, _)| same(&k[0], &g.0[0])) {
            Some((_, members)) => members.push(g),
            None => tops.push((vec![g.0[0].clone()], vec![g])),
        }
    }
    let mut keyed: Vec<(Vec<Value>, Vec<Group>)> = tops
        .into_iter()
        .map(|(mut key, mut members)| {
            let all: Vec<usize> = members.iter().flat_map(|g| g.1.iter().copied()).collect();
            key.extend(std::iter::repeat_n(Value::Empty, members[0].0.len() - 1));
            key.extend(funcs.iter().map(|f| aggregate(c, f, column_of(vals, 0, &all))));
            members.sort_by(|x, y| by_columns(spec, &flat(x), &flat(y)));
            (key, members)
        })
        .collect();
    keyed.sort_by(|x, y| by_columns(spec, &x.0, &y.0));
    keyed.into_iter().flat_map(|(_, members)| members).collect()
}

fn rows_to_array(rows: Vec<Vec<Value>>) -> Value {
    let columns = rows.first().map_or(0, Vec::len);
    let n = rows.len();
    array_value(n, columns, rows.into_iter().flatten().collect())
}

// ----- PIVOTBY --------------------------------------------------------------

fn pivotby(c: &mut Ctx, a: &[Expr]) -> Value {
    match pivotby_of(c, a) {
        Ok(v) => v,
        Err(e) => err(e),
    }
}

fn pivotby_of(c: &mut Ctx, a: &[Expr]) -> Res<Value> {
    let funcs = function_arg(c, &a[3])?;
    let mut tables = Vec::new();
    let mut names_at = 1;
    let mut generated = false;
    for f in &funcs.list {
        let (t, at, g) = pivot_table(c, a, f, &funcs)?;
        names_at = at;
        generated = g;
        tables.push(t);
    }
    if tables.len() == 1 {
        return Ok(rows_to_array(tables.into_iter().next().unwrap_or_default()));
    }
    // A row of functions: each column of results is a column for each function, with the functions' names under the labels.
    let k = arg_array(c, &a[0])?.columns;
    let n = tables.len();
    let mut out: Vec<Vec<Value>> = Vec::new();
    if funcs.array == Some(true) {
        // A column of functions: the rows of headers get an empty column after the keys; every other row is a row for each function, with its name there.
        let header_rows = if generated { 3 } else { names_at };
        for r in 0..tables[0].len() {
            if r < header_rows {
                let mut row = tables[0][r].clone();
                row.insert(k, Value::Text(String::new()));
                out.push(row);
            } else {
                for (i, t) in tables.iter().enumerate() {
                    let Some(src) = t.get(r) else { continue };
                    let mut row: Vec<Value> = src[..k].to_vec();
                    row.push(funcs.name(i));
                    row.extend(src[k..].iter().cloned());
                    out.push(row);
                }
            }
        }
        return Ok(rows_to_array(out));
    }
    for r in 0..tables[0].len() {
        if r == names_at {
            let mut names: Vec<Value> = (0..k).map(|_| Value::Text(String::new())).collect();
            for _ in k..tables[0][r].len() {
                names.extend((0..n).map(|i| funcs.name(i)));
            }
            out.push(names);
        }
        let mut row: Vec<Value> = tables[0][r][..k].to_vec();
        for j in k..tables[0][r].len() {
            // The field's name over the columns (the first of the rows of headers) is said once.
            let once = generated && r == 0;
            row.extend(tables.iter().enumerate().map(|(i, t)| if once && i > 0 { Value::Text(String::new()) } else { t.get(r).and_then(|x| x.get(j)).cloned().unwrap_or(Value::Empty) }));
        }
        out.push(row);
    }
    Ok(rows_to_array(out))
}

/// One table of PIVOTBY for one function, and where the names of several functions go (after the column labels).
fn pivot_table(c: &mut Ctx, a: &[Expr], f: &Value, funcs: &Funcs) -> Res<(Vec<Vec<Value>>, usize, bool)> {
    let rf = arg_array(c, &a[0])?;
    let cf = arg_array(c, &a[1])?;
    let vals = arg_array(c, &a[2])?;
    let f = f.clone();
    if rf.rows != vals.rows || cf.rows != vals.rows || vals.columns != 1 {
        return Err(CellError::Value);
    }
    let k = rf.columns;
    let text_row = || rf.rows >= 2 && matches!(vals.get(0, 0), Value::Text(_)) && !matches!(vals.get(1, 0), Value::Text(_));
    let headers = headers_option(c, a, 4, text_row)?;
    let row_depth = total_depth(c, a, 5, k)?;
    // A sort by the totals counts every function's total where the functions go along the sorted side (a row of functions
    // for the rows' totals, a column for the columns'), else the first function's.
    let n = funcs.list.len();
    let (row_totals, col_totals) = (if funcs.array == Some(false) { n } else { 1 }, if funcs.array == Some(true) { n } else { 1 });
    let row_sort = sort_option(c, a, 6, k + row_totals)?;
    let col_depth = total_depth(c, a, 7, 1.max(cf.columns))?;
    let col_sort = sort_option(c, a, 8, cf.columns + col_totals)?;
    let filter = filter_option(c, a, 9, rf.rows)?;
    // How PERCENTOF is relative (0 to 4); the other functions do not mind.
    let rel = match int_option(c, a, 10)? {
        None => 0,
        Some(n @ 0..=4) => n,
        Some(_) => return Err(CellError::Value),
    };
    let percent = matches!(&f, Value::Lambda(l) if l.eta.as_deref() == Some("PERCENTOF"));
    let skip = usize::from(matches!(headers, Headers::Hidden | Headers::Shown));
    // The header rows with more than one column field are not measured (the names over the columns).
    if cf.columns > 1 && matches!(headers, Headers::Generated | Headers::Shown) {
        return Err(CellError::Value);
    }
    let kc = cf.columns;
    let rows: Vec<usize> = (skip..rf.rows).filter(|&r| filter[r]).collect();
    if rows.is_empty() {
        return Err(CellError::Calc);
    }
    // Row keys and column keys, each with its rows and total.
    let (row_funcs, col_funcs) = (&funcs.list[..row_totals], &funcs.list[..col_totals]);
    let mut row_groups: Vec<Group> = Vec::new();
    for (key, members) in group_rows(&rf, &rows) {
        let totals = row_funcs.iter().map(|g| aggregate(c, g, column_of(&vals, 0, &members))).collect();
        row_groups.push((key, members, totals));
    }
    let mut col_groups: Vec<Group> = Vec::new();
    for (key, members) in group_rows(&cf, &rows) {
        let totals = col_funcs.iter().map(|g| aggregate(c, g, column_of(&vals, 0, &members))).collect();
        col_groups.push((key, members, totals));
    }
    // A sort of the rows by the totals counts only where the totals' column is shown (Excel's columns sort by their totals either way).
    let row_sort = row_sort.filter(|spec| col_depth != 0 || spec.iter().all(|s| s.unsigned_abs() as usize <= k));
    row_groups = sorted_groups(c, row_funcs, &vals, row_groups, &row_sort, row_depth.abs() == 2);
    col_groups = sorted_groups(c, col_funcs, &vals, col_groups, &col_sort, col_depth.abs() == 2);
    let blank = || Value::Text(String::new());
    // The members of the groups one level up (the same keys but the last): all the rows with one field.
    let parent_of = |groups: &[Group], key: &[Value]| -> Vec<usize> {
        if key.len() < 2 {
            return rows.clone();
        }
        let mut m: Vec<usize> = groups.iter().filter(|g| same_keys(&g.0[..key.len() - 1], &key[..key.len() - 1])).flat_map(|g| g.1.iter().copied()).collect();
        m.sort_unstable();
        m
    };
    // The columns: each leaf group, with the subtotal of the first field's value after (before) its groups where the
    // depth is 2 (-2), then the total; a column has the rows it covers and the rows of its parent group.
    struct Column {
        label: Vec<Value>,
        members: Vec<usize>,
        parent: Vec<usize>,
    }
    let mut columns: Vec<Column> = Vec::new();
    {
        let sub = col_depth.abs() == 2;
        let mut i = 0;
        while i < col_groups.len() {
            let top = col_groups[i].0[0].clone();
            let end = if sub { (i..col_groups.len()).find(|&j| !same(&col_groups[j].0[0], &top)).unwrap_or(col_groups.len()) } else { i + 1 };
            let mut group: Vec<usize> = col_groups[i..end].iter().flat_map(|g| g.1.iter().copied()).collect();
            group.sort_unstable();
            let subtotal = || Column { label: std::iter::once(top.clone()).chain((1..kc).map(|_| blank())).collect(), members: group.clone(), parent: rows.clone() };
            if sub && col_depth < 0 {
                columns.push(subtotal());
            }
            for g in &col_groups[i..end] {
                columns.push(Column { label: g.0.clone(), members: g.1.clone(), parent: parent_of(&col_groups, &g.0) });
            }
            if sub && col_depth > 0 {
                columns.push(subtotal());
            }
            i = end;
        }
    }
    let total_column = || Column { label: std::iter::once(Value::Text(if col_depth.abs() == 2 { GRAND_TOTAL } else { TOTAL }.into())).chain((1..kc).map(|_| blank())).collect(), members: rows.clone(), parent: rows.clone() };
    match col_depth {
        1 | 2 => columns.push(total_column()),
        -1 | -2 => columns.insert(0, total_column()),
        _ => {}
    }
    let within = |set: &[usize], of: &[usize]| -> Vec<usize> { set.iter().copied().filter(|r| of.contains(r)).collect() };
    // What PERCENTOF is a share of, by `relative_to`: the column's total (0), the row's total (1), the grand total (2),
    // the row's total within the column's parent group (3), or the column's total within the row's parent group (4).
    // Parent groups are the groups of the keys but the last (all the rows for one field, and for a subtotal or total).
    let dens = |row_m: &[usize], row_parent: &[usize], col: &Column| -> Vec<usize> {
        match rel {
            1 => row_m.to_vec(),
            2 => rows.clone(),
            3 => within(row_m, &col.parent),
            4 => within(row_parent, &col.members),
            _ => within(&rows, &col.members),
        }
    };
    let cell_of = |c: &mut Ctx, members: &[usize], den: Vec<usize>| -> Value {
        let all = if percent { column_of(&vals, 0, &den) } else { Vec::new() };
        aggregate_in(c, &f, column_of(&vals, 0, members), all)
    };
    // The body: a row for each row key, with the subtotals of the first field's values (the hierarchy).
    let cells_of = |c: &mut Ctx, members: &[usize], row_parent: &[usize]| -> Vec<Value> {
        columns
            .iter()
            .map(|col| {
                let both = within(members, &col.members);
                if both.is_empty() { blank() } else { cell_of(c, &both, dens(members, row_parent, col)) }
            })
            .collect()
    };
    let mut body: Vec<Vec<Value>> = Vec::new();
    let subtotals = row_depth.abs() == 2;
    let mut i = 0;
    while i < row_groups.len() {
        let top = row_groups[i].0[0].clone();
        let end = if subtotals { (i..row_groups.len()).find(|&j| !same(&row_groups[j].0[0], &top)).unwrap_or(row_groups.len()) } else { i + 1 };
        let group: Vec<usize> = row_groups[i..end].iter().flat_map(|g| g.1.iter().copied()).collect();
        let subtotal = |c: &mut Ctx| -> Vec<Value> {
            let mut row = vec![top.clone()];
            row.extend((1..k).map(|_| blank()));
            row.extend(cells_of(c, &group, &rows));
            row
        };
        if subtotals && row_depth < 0 {
            body.push(subtotal(c));
        }
        for (key, members, _) in &row_groups[i..end] {
            let mut row = key.clone();
            row.extend(cells_of(c, members, &parent_of(&row_groups, key)));
            body.push(row);
        }
        if subtotals && row_depth > 0 {
            body.push(subtotal(c));
        }
        i = end;
    }
    // The column totals' row.
    if row_depth != 0 {
        let cells: Vec<Value> = columns.iter().map(|col| cell_of(c, &col.members, dens(&rows, &rows, col))).collect();
        let mut row = vec![Value::Text(if subtotals { GRAND_TOTAL } else { TOTAL }.into())];
        row.extend((1..k).map(|_| blank()));
        row.extend(cells);
        if row_depth > 0 { body.push(row) } else { body.insert(0, row) }
    }
    // The header rows.
    let col_labels: Vec<Value> = columns.iter().map(|col| col.label[0].clone()).collect();
    let mut out: Vec<Vec<Value>> = Vec::new();
    let names_at = if matches!(headers, Headers::Generated | Headers::Shown) { 2 } else { kc };
    let lead = |first: Value| -> Vec<Value> {
        let mut row = vec![first];
        row.extend((1..k).map(|_| blank()));
        row
    };
    match headers {
        Headers::Generated | Headers::Shown => {
            let (col_header, row_headers, value_header): (Value, Vec<Value>, Value) = if headers == Headers::Shown {
                (header_text(cf.get(0, 0)), (0..k).map(|i| rf.get(0, i).clone()).collect(), vals.get(0, 0).clone())
            } else {
                (Value::Text("列フィールド 1".into()), (1..=k).map(|i| Value::Text(format!("行フィールド {i}"))).collect(), Value::Text("値 1".into()))
            };
            let mut first = lead(blank());
            first.push(col_header);
            first.extend((1..col_labels.len()).map(|_| blank()));
            out.push(first);
            let mut second = lead(blank());
            second.extend(col_labels.iter().cloned());
            out.push(second);
            let mut third = row_headers;
            third.extend(col_labels.iter().map(|_| value_header.clone()));
            out.push(third);
        }
        _ => {
            // A row of headers for each column field.
            for level in 0..kc {
                let mut header = lead(blank());
                header.extend(columns.iter().map(|col| col.label[level].clone()));
                out.push(header);
            }
        }
    }
    out.extend(body);
    Ok((out, names_at, matches!(headers, Headers::Generated | Headers::Shown)))
}

