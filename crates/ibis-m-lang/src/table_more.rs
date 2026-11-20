// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `Table.*` functions whose results were measured in Excel (`tasks/doc/requests/2026-11-17-k0039-m-semantics-result.md`, sections 14-29):
//! headers, the order of columns, distinct, group, pivot, unpivot, joins, split and combine of columns, transpose, combine, replacing
//! values, and the functions of error cells. Where the measured result is odd (the full outer nested join) the plain reading stands and the
//! difference is written at the function.
use crate::ast::{Ty, TypeSpec};
use crate::builtin::{as_number, as_text, culture_arg, list_items};
use crate::culture;
use crate::error::{ErrKind, MError};
use crate::eval::Interp;
use crate::lib_text::ignore_case;
use crate::table::*;
use crate::value::*;
use std::collections::HashMap;
use std::rc::Rc;

type R = Result<Value, MError>;

/// Rows of cells as lists that read their cells lazily: a cell that is an error fails when it is read.
fn cell_list(row: &[Value]) -> Value {
    Value::List(Rc::new(row.iter().cloned().map(Lazy::of_cell).collect()))
}

fn raised(e: MError) -> Result<Value, MError> {
    if matches!(e.kind, ErrKind::Raised) { Ok(Value::Failed(Rc::new(e))) } else { Err(e) }
}

/// Whether two cells are equal for a key, the text compared by `cmp` when it ignores case.
fn key_equal(it: &Interp, a: &Value, b: &Value, ic: bool) -> Result<bool, MError> {
    if ic && let (Value::Text(x), Value::Text(y)) = (a, b) {
        return Ok(x.chars().map(culture::simple_upper).eq(y.chars().map(culture::simple_upper)));
    }
    it.equals(a, b)
}

fn rows_equal(it: &Interp, a: &[Value], b: &[Value], idx: &[usize], ic: bool) -> Result<bool, MError> {
    for &i in idx {
        if !key_equal(it, &a[i], &b[i], ic)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The specs of `{{"col", x}, ...}` or a single `{"col", x}` as lists.
fn specs(it: &Interp, v: &Value) -> Result<Vec<Vec<Value>>, MError> {
    match v {
        Value::List(items) if items.first().is_some_and(|x| matches!(x.force(it), Ok(Value::Text(_)))) => Ok(vec![list_items(it, v)?]),
        v => list_items(it, v)?.iter().map(|p| list_items(it, p)).collect(),
    }
}

fn func_name(v: &Value) -> Option<&'static str> {
    match v {
        Value::Function(f) => match &**f {
            Func::Builtin { name, .. } | Func::Native { name, .. } => Some(name),
            _ => None,
        },
        _ => None,
    }
}

fn dup_error(name: &str) -> MError {
    MError::expr(format!("The field '{name}' already exists in the record."))
}

pub fn register(l: &mut HashMap<&'static str, Value>) {
    for (n, v) in [("JoinAlgorithm.Dynamic", 0.0), ("JoinAlgorithm.PairwiseHash", 1.0), ("JoinAlgorithm.SortMerge", 2.0), ("JoinAlgorithm.LeftHash", 3.0), ("JoinAlgorithm.RightHash", 4.0), ("JoinAlgorithm.LeftIndex", 5.0), ("JoinAlgorithm.RightIndex", 6.0)] {
        l.insert(n, Value::Number(v));
    }
    // Headers: text and numbers (as text) are names, anything else (and null) is `Column<n>` unless PromoteAllScalars; a repeated name gets
    // `_1`, `_2`...; an empty text stays empty; the types of the columns are kept.
    reg(l, "Table.PromoteHeaders", 1, 2, |it, a| {
        let t = tbl(&a[0])?;
        let all = match a.get(1) {
            Some(opts @ Value::Record(_)) => matches!(it.field(opts, "PromoteAllScalars", true)?, Value::Logical(true)),
            _ => false,
        };
        let Some(first) = t.rows.first() else { return Ok(Value::Table(t)) };
        let mut cols: Vec<String> = Vec::new();
        for (i, v) in first.iter().enumerate() {
            let fallback = format!("Column{}", i + 1);
            let base = match v {
                Value::Text(s) => s.to_string(),
                Value::Number(n) => culture::number_text(*n),
                Value::Null | Value::Failed(_) => fallback,
                v if all => scalar_text_in(v, it.culture()).unwrap_or(fallback),
                _ => fallback,
            };
            let mut name = base.clone();
            let mut k = 1;
            while cols.contains(&name) {
                name = format!("{base}_{k}");
                k += 1;
            }
            cols.push(name);
        }
        Ok(make(cols, t.types.clone(), t.rows[1..].to_vec()))
    });
    // The listed columns take, in the order listed, the places the listed columns have (`{"c","a"}` on a,b,c,d gives c,b,a,d).
    reg(l, "Table.ReorderColumns", 2, 3, |it, a| {
        let t = tbl(&a[0])?;
        let miss = missing_of(a, 2);
        let mut listed: Vec<usize> = Vec::new();
        let mut null_cols: Vec<String> = Vec::new();
        for n in names(it, &a[1])? {
            match t.columns.iter().position(|c| *c == n) {
                Some(i) => {
                    if listed.contains(&i) {
                        return Err(dup_error(&n));
                    }
                    listed.push(i);
                }
                None if miss == 0 => return Err(MError::expr(format!("The column '{n}' of the table wasn't found."))),
                None if miss == 2 => null_cols.push(n),
                None => {}
            }
        }
        let mut slots = listed.clone();
        slots.sort_unstable();
        let mut order: Vec<usize> = (0..t.columns.len()).collect();
        for (slot, col) in slots.iter().zip(&listed) {
            order[*slot] = *col;
        }
        let Value::Table(p) = project(&t, &order) else { unreachable!() };
        if null_cols.is_empty() {
            return Ok(Value::Table(p));
        }
        let (mut cols, mut types, mut rows) = (p.columns.clone(), p.types.clone(), p.rows.clone());
        for n in null_cols {
            cols.push(n);
            types.push(TypeSpec { ty: Ty::Null, nullable: true });
            for r in &mut rows {
                r.push(Value::Null);
            }
        }
        Ok(make(cols, types, rows))
    });
    // Distinct: the first of equal rows stays; the criteria are columns (a text, a list) or `{column, comparer}` pairs.
    reg(l, "Table.Distinct", 1, 2, |it, a| {
        let t = tbl(&a[0])?;
        let mut idx: Vec<usize> = Vec::new();
        let mut ic = false;
        match a.get(1) {
            None | Some(Value::Null) => idx = (0..t.columns.len()).collect(),
            Some(Value::Text(n)) => idx.push(find_col(&t, n).map_err(|_| MError::expr("The distinct criteria are not valid."))?),
            Some(v) => {
                for item in list_items(it, v)? {
                    let (name, cmp) = match &item {
                        Value::Text(n) => (n.to_string(), None),
                        p => {
                            let parts = list_items(it, p)?;
                            (as_text(&parts[0])?.to_string(), parts.get(1).cloned())
                        }
                    };
                    idx.push(find_col(&t, &name).map_err(|_| MError::expr("The distinct criteria are not valid."))?);
                    ic |= ignore_case(it, cmp.as_ref())?;
                }
            }
        }
        let mut kept: Vec<Vec<Value>> = Vec::new();
        for r in &t.rows {
            for c in r {
                if let Value::Failed(e) = c {
                    return Err((**e).clone());
                }
            }
            let mut seen = false;
            for k in &kept {
                if rows_equal(it, r, k, &idx, ic)? {
                    seen = true;
                    break;
                }
            }
            if !seen {
                kept.push(r.clone());
            }
        }
        Ok(same_shape(&t, kept))
    });
    reg(l, "Table.AddIndexColumn", 2, 5, |_, a| {
        let t = tbl(&a[0])?;
        let name = as_text(&a[1])?.to_string();
        if t.columns.contains(&name) {
            return Err(MError::expr(format!("The column '{name}' already exists in the table.")));
        }
        let start = a.get(2).map(as_number).transpose()?.unwrap_or(0.0);
        let inc = a.get(3).map(as_number).transpose()?.unwrap_or(1.0);
        let rows = t.rows.iter().enumerate().map(|(i, r)| r.iter().cloned().chain([Value::Number(start + inc * i as f64)]).collect()).collect();
        let mut cols = t.columns.clone();
        cols.push(name);
        let mut types = t.types.clone();
        types.push(match a.get(4) {
            Some(Value::Type(ts)) => ts.clone(),
            _ => TypeSpec { ty: Ty::Number, nullable: false },
        });
        Ok(make(cols, types, rows))
    });
    reg(l, "Table.TransformColumnTypes", 2, 3, |it, a| {
        let t = tbl(&a[0])?;
        let c = culture_arg(it, a.get(2))?;
        let mut rows = t.rows.clone();
        let mut types = t.types.clone();
        for s in &specs(it, &a[1])? {
            let i = find_col(&t, &as_text(&s[0])?)?;
            let Some(Value::Type(ts)) = s.get(1) else { return Err(MError::expr("A type change needs a type.")) };
            types[i] = ts.clone();
            for r in &mut rows {
                r[i] = convert_value(&r[i], ts, c);
            }
        }
        Ok(make(t.columns.clone(), types, rows))
    });
    // ReplaceValue: each cell by the replacer; a cell the replacer fails on is an error cell, the rest of the table is made.
    reg(l, "Table.ReplaceValue", 5, 5, |it, a| {
        let t = tbl(&a[0])?;
        let mut rows = t.rows.clone();
        for n in names(it, &a[4])? {
            let i = find_col(&t, &n)?;
            for r in &mut rows {
                if matches!(r[i], Value::Failed(_)) {
                    continue;
                }
                r[i] = match it.call(&a[3], vec![r[i].clone(), a[1].clone(), a[2].clone()]) {
                    Ok(v) => v,
                    Err(e) => return_cell(e)?,
                };
            }
        }
        Ok(same_shape(&t, rows))
    });
    reg(l, "Replacer.ReplaceText", 3, 3, |_, a| {
        let text = |v: &Value| match v {
            Value::Text(s) => Ok(s.clone()),
            v => Err(MError::expr(format!("We cannot convert the value {} to type Text.", scalar_text(v).unwrap_or_else(|| "null".into())))),
        };
        // The old and the new text are checked before the value (a null among them fails every cell, even a null cell).
        let (old, new) = (text(&a[1])?, text(&a[2])?);
        match &a[0] {
            Value::Null => Ok(Value::Null),
            v => {
                let s = text(v)?;
                Ok(Value::text(if old.is_empty() { s.to_string() } else { s.replace(&*old, &new) }))
            }
        }
    });
    reg(l, "Table.Combine", 1, 2, |it, a| {
        let ts: Vec<Rc<Table>> = list_items(it, &a[0])?.iter().map(tbl).collect::<Result<_, _>>()?;
        let mut cols: Vec<String> = Vec::new();
        let mut types: Vec<TypeSpec> = Vec::new();
        for t in &ts {
            for (c, ty) in t.columns.iter().zip(&t.types) {
                match cols.iter().position(|x| x == c) {
                    Some(p) => types[p] = merge_type(&types[p], ty),
                    None => {
                        cols.push(c.clone());
                        types.push(ty.clone());
                    }
                }
            }
        }
        // A second argument names the columns of the result and their order.
        if let Some(v) = a.get(1).filter(|v| !matches!(v, Value::Null)) {
            let want = names(it, v)?;
            types = want.iter().map(|n| cols.iter().position(|c| c == n).map_or_else(any_ty, |p| types[p].clone())).collect();
            cols = want;
        }
        let mut rows = Vec::new();
        for t in &ts {
            for r in &t.rows {
                rows.push(cols.iter().map(|c| t.columns.iter().position(|x| x == c).map_or(Value::Null, |i| r[i].clone())).collect());
            }
        }
        Ok(make(cols, types, rows))
    });
    reg(l, "Table.ToRows", 1, 1, |_, a| {
        let t = tbl(&a[0])?;
        Ok(Value::List(Rc::new(t.rows.iter().map(|r| Lazy::done(cell_list(r))).collect())))
    });
    // Transpose: the rows become the columns, named Column1...; the types are not kept; a table without rows or columns is empty.
    reg(l, "Table.Transpose", 1, 2, |it, a| {
        let t = tbl(&a[0])?;
        if t.rows.is_empty() || t.columns.is_empty() {
            return Ok(make(vec![], vec![], vec![]));
        }
        let n = t.rows.len();
        let cols = match a.get(1) {
            Some(Value::Null) | None => (1..=n).map(|i| format!("Column{i}")).collect(),
            Some(v) => {
                let c = names(it, v)?;
                if c.len() != n {
                    return Err(MError::expr(format!("The number of columns ({n}) does not match the number of column names ({}).", c.len())));
                }
                c
            }
        };
        let rows: Vec<Vec<Value>> = (0..t.columns.len()).map(|c| t.rows.iter().map(|r| r[c].clone()).collect()).collect();
        Ok(make(cols, vec![any_ty(); n], rows))
    });
    reg(l, "Table.Unpivot", 4, 4, |it, a| unpivot(it, a, false));
    reg(l, "Table.UnpivotOtherColumns", 4, 4, |it, a| unpivot(it, a, true));
    reg(l, "Table.Pivot", 4, 5, |it, a| {
        let t = tbl(&a[0])?;
        let mut pv: Vec<String> = Vec::new();
        for v in list_items(it, &a[1])? {
            match v {
                Value::Text(s) => pv.push(s.to_string()),
                v => return Err(MError::expr(format!("We cannot convert the value {} to type Text.", scalar_text(&v).unwrap_or_else(|| "null".into())))),
            }
        }
        let (ai, vi) = (find_col(&t, &as_text(&a[2])?)?, find_col(&t, &as_text(&a[3])?)?);
        let keep: Vec<usize> = (0..t.columns.len()).filter(|i| *i != ai && *i != vi).collect();
        for p in &pv {
            if keep.iter().any(|&i| t.columns[i] == *p) {
                return Err(dup_error(p));
            }
        }
        let mut groups: Vec<(Vec<Value>, Vec<usize>)> = Vec::new();
        for (ri, r) in t.rows.iter().enumerate() {
            let key: Vec<Value> = keep.iter().map(|&i| r[i].clone()).collect();
            let mut at = None;
            for (gi, (k, _)) in groups.iter().enumerate() {
                let mut eq = true;
                for (x, y) in k.iter().zip(&key) {
                    if !it.equals(x, y)? {
                        eq = false;
                        break;
                    }
                }
                if eq {
                    at = Some(gi);
                    break;
                }
            }
            match at {
                Some(g) => groups[g].1.push(ri),
                None => groups.push((key, vec![ri])),
            }
        }
        let mut rows = Vec::new();
        for (key, members) in &groups {
            let mut row = key.clone();
            for p in &pv {
                let vals: Vec<Value> = members.iter().filter(|&&ri| matches!(&t.rows[ri][ai], Value::Text(s) if **s == **p)).map(|&ri| t.rows[ri][vi].clone()).collect();
                row.push(match (vals.len(), a.get(4)) {
                    (_, Some(f)) => match it.call(f, vec![Value::list(vals)]) {
                        Ok(v) => v,
                        Err(e) => return_cell(e)?,
                    },
                    (0, None) => Value::Null,
                    (1, None) => vals[0].clone(),
                    (_, None) => return_cell(MError::expr("There were too many elements in the enumeration to complete the operation."))?,
                });
            }
            rows.push(row);
        }
        let value_type = if a.get(4).and_then(func_name) == Some("List.Count") { TypeSpec { ty: Ty::Number, nullable: false } } else { t.types[vi].clone() };
        let mut cols: Vec<String> = keep.iter().map(|&i| t.columns[i].clone()).collect();
        let mut types: Vec<TypeSpec> = keep.iter().map(|&i| t.types[i].clone()).collect();
        for p in &pv {
            cols.push(p.clone());
            types.push(value_type.clone());
        }
        Ok(make(cols, types, rows))
    });
    reg(l, "Table.Group", 3, 5, |it, a| {
        let t = tbl(&a[0])?;
        let keys: Vec<usize> = names(it, &a[1])?.iter().map(|n| find_col(&t, n)).collect::<Result<_, _>>()?;
        let local = matches!(a.get(3), Some(Value::Number(n)) if *n == 1.0);
        let ic = ignore_case(it, a.get(4))?;
        let aggs = specs(it, &a[2])?;
        let mut groups: Vec<(Vec<Value>, Vec<Vec<Value>>)> = Vec::new();
        for r in &t.rows {
            let key: Vec<Value> = keys.iter().map(|&i| r[i].clone()).collect();
            let candidates: Box<dyn Iterator<Item = usize>> = if local { Box::new(groups.len().checked_sub(1).into_iter()) } else { Box::new(0..groups.len()) };
            let mut at = None;
            for gi in candidates {
                let mut eq = true;
                for (x, y) in groups[gi].0.iter().zip(&key) {
                    if !key_equal(it, x, y, ic)? {
                        eq = false;
                        break;
                    }
                }
                if eq {
                    at = Some(gi);
                    break;
                }
            }
            match at {
                Some(g) => groups[g].1.push(r.clone()),
                None => groups.push((key, vec![r.clone()])),
            }
        }
        let mut cols: Vec<String> = keys.iter().map(|&i| t.columns[i].clone()).collect();
        let mut types: Vec<TypeSpec> = keys.iter().map(|&i| t.types[i].clone()).collect();
        for s in &aggs {
            let name = as_text(&s[0])?.to_string();
            if cols.contains(&name) {
                return Err(dup_error(&name));
            }
            cols.push(name);
            types.push(match s.get(2) {
                Some(Value::Type(ts)) => ts.clone(),
                _ => any_ty(),
            });
        }
        let mut rows = Vec::new();
        for (key, members) in groups {
            let sub = Value::Table(Rc::new(Table { columns: t.columns.clone(), types: t.types.clone(), rows: members }));
            let mut row = key;
            for s in &aggs {
                let f = s.get(1).ok_or_else(|| MError::expr("An aggregation needs a function."))?;
                row.push(match it.call(f, vec![sub.clone()]) {
                    Ok(v) => v,
                    Err(e) => return_cell(e)?,
                });
            }
            rows.push(row);
        }
        Ok(make(cols, types, rows))
    });
    reg(l, "Table.NestedJoin", 5, 7, |it, a| nested_join(it, a));
    reg(l, "Table.Join", 4, 6, |it, a| join(it, a));
    reg(l, "Table.ExpandTableColumn", 3, 4, |it, a| {
        let t = tbl(&a[0])?;
        let i = find_col(&t, &as_text(&a[1])?)?;
        let inner = names(it, &a[2])?;
        let new = match a.get(3) {
            Some(Value::Null) | None => inner.clone(),
            Some(v) => names(it, v)?,
        };
        let others: Vec<String> = t.columns.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, c)| c.clone()).collect();
        for (k, n) in new.iter().enumerate() {
            if others.contains(n) || new[..k].contains(n) {
                return Err(dup_error(n));
            }
        }
        // The type of an expanded column is that of the nested column of the first nested table that has it.
        let inner_types: Vec<TypeSpec> = inner
            .iter()
            .map(|n| t.rows.iter().find_map(|r| if let Value::Table(sub) = &r[i] { sub.columns.iter().position(|c| c == n).map(|p| sub.types[p].clone()) } else { None }).unwrap_or_else(any_ty))
            .collect();
        let mut cols = t.columns[..i].to_vec();
        cols.extend(new.clone());
        cols.extend_from_slice(&t.columns[i + 1..]);
        let mut types = t.types[..i].to_vec();
        types.extend(inner_types);
        types.extend_from_slice(&t.types[i + 1..]);
        let mut rows = Vec::new();
        for r in &t.rows {
            let (before, after) = (r[..i].to_vec(), r[i + 1..].to_vec());
            match &r[i] {
                Value::Table(sub) if !sub.rows.is_empty() => {
                    for sr in &sub.rows {
                        let mut row = before.clone();
                        for n in &inner {
                            row.push(sub.columns.iter().position(|c| c == n).map_or(Value::Null, |ci| sr[ci].clone()));
                        }
                        row.extend(after.clone());
                        rows.push(row);
                    }
                }
                _ => {
                    let mut row = before;
                    row.extend(vec![Value::Null; inner.len()]);
                    row.extend(after);
                    rows.push(row);
                }
            }
        }
        Ok(make(cols, types, rows))
    });
    // SplitColumn: the pieces of the splitter; the new columns are named `col.1`, `col.2`... unless names (or a number of columns) are given;
    // fewer pieces are null, more are dropped; a null cell gives nulls; a name of another column is an error; the new columns are text.
    reg(l, "Table.SplitColumn", 3, 5, |it, a| {
        let t = tbl(&a[0])?;
        let i = find_col(&t, &as_text(&a[1])?).map_err(|_| MError::expr(format!("The field '{}' of the record wasn't found.", scalar_text(&a[1]).unwrap_or_default())))?;
        let mut parts: Vec<Vec<Value>> = Vec::new();
        for r in &t.rows {
            parts.push(match &r[i] {
                Value::Null => Vec::new(),
                v @ Value::Text(_) => list_items(it, &it.call(&a[2], vec![v.clone()])?)?,
                v => return Err(MError::expr(format!("We cannot convert the value {} to type Text.", scalar_text(v).unwrap_or_default()))),
            });
        }
        let others: Vec<String> = t.columns.iter().enumerate().filter(|(k, _)| *k != i).map(|(_, c)| c.clone()).collect();
        let new_names: Vec<String> = match a.get(3) {
            Some(Value::Text(_)) | Some(Value::List(_)) => names(it, &a[3])?,
            other_arg => {
                let width = match other_arg {
                    Some(Value::Number(n)) => *n as usize,
                    _ => parts.iter().map(Vec::len).max().unwrap_or(1).max(1),
                };
                (1..=width).map(|k| format!("{}.{k}", t.columns[i])).collect()
            }
        };
        for (k, n) in new_names.iter().enumerate() {
            if others.contains(n) || new_names[..k].contains(n) {
                return Err(dup_error(n));
            }
        }
        let width = new_names.len();
        let mut cols = t.columns.clone();
        let mut types = t.types.clone();
        cols.splice(i..=i, new_names);
        types.splice(i..=i, vec![TypeSpec { ty: Ty::Text, nullable: true }; width]);
        let rows = t
            .rows
            .iter()
            .zip(&parts)
            .map(|(r, ps)| {
                let mut out = r[..i].to_vec();
                for k in 0..width {
                    out.push(ps.get(k).cloned().unwrap_or(Value::Null));
                }
                out.extend_from_slice(&r[i + 1..]);
                out
            })
            .collect();
        Ok(make(cols, types, rows))
    });
    reg(l, "Table.CombineColumns", 4, 5, |it, a| {
        let t = tbl(&a[0])?;
        let idx: Vec<usize> = names(it, &a[1])?.iter().map(|n| find_col(&t, n)).collect::<Result<_, _>>()?;
        let first = *idx.first().ok_or_else(|| MError::expr("No columns were chosen."))?;
        let name = as_text(&a[3])?.to_string();
        let mut rows = Vec::new();
        for r in &t.rows {
            let mut row = r.clone();
            row[first] = match it.call(&a[2], vec![cell_list(&idx.iter().map(|&i| r[i].clone()).collect::<Vec<_>>())]) {
                Ok(v) => v,
                Err(e) => return_cell(e)?,
            };
            rows.push(row);
        }
        let mut cols = t.columns.clone();
        let mut types = t.types.clone();
        let rest: Vec<String> = cols.iter().enumerate().filter(|(k, _)| !idx.contains(k)).map(|(_, c)| c.clone()).collect();
        if rest.contains(&name) {
            return Err(dup_error(&name));
        }
        cols[first] = name;
        types[first] = TypeSpec { ty: Ty::Text, nullable: true };
        let keep: Vec<usize> = (0..cols.len()).filter(|k| *k == first || !idx.contains(k)).collect();
        let t2 = Table { columns: cols, types, rows };
        Ok(project(&t2, &keep))
    });
    // Error cells: rows with an error in the given columns (all, by default) are removed, selected, or their errors are replaced.
    reg(l, "Table.RemoveRowsWithErrors", 1, 2, |it, a| rows_with_errors(it, a, false));
    reg(l, "Table.SelectRowsWithErrors", 1, 2, |it, a| rows_with_errors(it, a, true));
    reg(l, "Table.ReplaceErrorValues", 2, 2, |it, a| {
        let t = tbl(&a[0])?;
        let mut rows = t.rows.clone();
        for s in &specs(it, &a[1])? {
            let i = find_col(&t, &as_text(&s[0])?)?;
            let with = s.get(1).cloned().unwrap_or(Value::Null);
            for r in &mut rows {
                if matches!(r[i], Value::Failed(_)) {
                    r[i] = with.clone();
                }
            }
        }
        Ok(same_shape(&t, rows))
    });
}

/// An error raised while making one cell is that cell's error; an error that is not one of M's (an unsupported function, a limit) stops.
fn return_cell(e: MError) -> R {
    raised(e)
}

fn rows_with_errors(it: &Interp, a: &[Value], select: bool) -> R {
    let t = tbl(&a[0])?;
    let idx: Vec<usize> = match a.get(1) {
        None | Some(Value::Null) => (0..t.columns.len()).collect(),
        Some(v) => names(it, v)?.iter().map(|n| find_col(&t, n)).collect::<Result<_, _>>()?,
    };
    let rows = t.rows.iter().filter(|r| idx.iter().any(|&i| matches!(r[i], Value::Failed(_))) == select).cloned().collect();
    Ok(same_shape(&t, rows))
}

/// The type of the values of columns that are unpivoted into one: the same type stays, numbers are a number, anything else is any.
fn common_type(types: &[&TypeSpec]) -> TypeSpec {
    types.iter().skip(1).fold(types.first().map_or_else(any_ty, |t| (*t).clone()), |acc, t| merge_type(&acc, t))
}

fn unpivot(it: &Interp, a: &[Value], other: bool) -> R {
    let t = tbl(&a[0])?;
    let given: Vec<usize> = names(it, &a[1])?.iter().map(|n| find_col(&t, n)).collect::<Result<_, _>>()?;
    // The columns are taken in the table's order, whatever the order given.
    let (keep, spin): (Vec<usize>, Vec<usize>) = if other {
        ((0..t.columns.len()).filter(|i| given.contains(i)).collect(), (0..t.columns.len()).filter(|i| !given.contains(i)).collect())
    } else {
        ((0..t.columns.len()).filter(|i| !given.contains(i)).collect(), (0..t.columns.len()).filter(|i| given.contains(i)).collect())
    };
    if !other && spin.is_empty() {
        return Err(MError::expr("There weren't enough elements in the enumeration to complete the operation."));
    }
    let (an, vn) = (as_text(&a[2])?.to_string(), as_text(&a[3])?.to_string());
    for n in [&an, &vn] {
        if keep.iter().any(|&i| t.columns[i] == **n) {
            return Err(dup_error(n));
        }
    }
    let mut rows = Vec::new();
    for r in &t.rows {
        for &s in &spin {
            // A cell of null makes no row.
            if matches!(r[s], Value::Null) {
                continue;
            }
            let mut row: Vec<Value> = keep.iter().map(|&i| r[i].clone()).collect();
            row.push(Value::text(&t.columns[s]));
            row.push(r[s].clone());
            rows.push(row);
        }
    }
    let mut cols: Vec<String> = keep.iter().map(|&i| t.columns[i].clone()).collect();
    let mut types: Vec<TypeSpec> = keep.iter().map(|&i| t.types[i].clone()).collect();
    cols.push(an);
    cols.push(vn);
    types.push(TypeSpec { ty: Ty::Text, nullable: false });
    types.push(common_type(&spin.iter().map(|&i| &t.types[i]).collect::<Vec<_>>()));
    Ok(make(cols, types, rows))
}

struct Matches {
    /// For each left row, the right rows that match.
    left: Vec<Vec<usize>>,
    right_used: Vec<bool>,
}

fn matches_of(it: &Interp, lt: &Table, rt: &Table, lki: &[usize], rki: &[usize]) -> Result<Matches, MError> {
    let mut left = Vec::new();
    let mut right_used = vec![false; rt.rows.len()];
    for lr in &lt.rows {
        let mut ms = Vec::new();
        for (ri, rr) in rt.rows.iter().enumerate() {
            let mut eq = true;
            for (&x, &y) in lki.iter().zip(rki) {
                if !it.equals(&lr[x], &rr[y])? {
                    eq = false;
                    break;
                }
            }
            if eq {
                ms.push(ri);
                right_used[ri] = true;
            }
        }
        left.push(ms);
    }
    Ok(Matches { left, right_used })
}

fn join_keys(it: &Interp, lt: &Table, lk: &Value, rt: &Table, rk: &Value) -> Result<(Vec<usize>, Vec<usize>), MError> {
    let lki: Vec<usize> = names(it, lk)?.iter().map(|n| find_col(lt, n)).collect::<Result<_, _>>()?;
    let rki: Vec<usize> = names(it, rk)?.iter().map(|n| find_col(rt, n)).collect::<Result<_, _>>()?;
    if lki.len() != rki.len() {
        return Err(MError::expr("A join needs the same number of key columns on both sides."));
    }
    Ok((lki, rki))
}

fn kind_of(v: Option<&Value>, default: i32) -> Result<i32, MError> {
    match v {
        None | Some(Value::Null) => Ok(default),
        Some(v) => Ok(as_number(v)? as i32),
    }
}

/// `Table.NestedJoin`: the left rows with the matching right rows as a table in a new column. Left outer is the default.
/// (Excel's full outer nested join gave misplaced nested tables; here it is the left outer rows and then the right rows that match nothing.)
fn nested_join(it: &Interp, a: &[Value]) -> R {
    if a.len() > 6 && !matches!(a[6], Value::Null) {
        return Err(MError::unsupported("Table.NestedJoin with a key comparer", 0));
    }
    let (lt, rt) = (tbl(&a[0])?, tbl(&a[2])?);
    let (lki, rki) = join_keys(it, &lt, &a[1], &rt, &a[3])?;
    let name = as_text(&a[4])?.to_string();
    if lt.columns.contains(&name) {
        return Err(MError::expr(format!("The column '{name}' already exists in the table.")));
    }
    let kind = kind_of(a.get(5), 1)?;
    let m = matches_of(it, &lt, &rt, &lki, &rki)?;
    let sub = |rows: Vec<usize>| Value::Table(Rc::new(Table { columns: rt.columns.clone(), types: rt.types.clone(), rows: rows.into_iter().map(|r| rt.rows[r].clone()).collect() }));
    let null_sub = || Value::Table(Rc::new(Table { columns: rt.columns.clone(), types: rt.types.clone(), rows: vec![vec![Value::Null; rt.columns.len()]] }));
    let mut rows: Vec<Vec<Value>> = Vec::new();
    for (lr, ms) in lt.rows.iter().zip(&m.left) {
        let mut row = lr.clone();
        match kind {
            0 | 2 if ms.is_empty() => continue,
            4 if !ms.is_empty() => continue,
            5 => continue,
            4 => row.push(null_sub()),
            _ => row.push(sub(ms.clone())),
        }
        rows.push(row);
    }
    // The right rows that match nothing are one row: the left columns null, the nested table all of them (Excel, measured).
    if matches!(kind, 2 | 3 | 5) {
        let only: Vec<usize> = m.right_used.iter().enumerate().filter(|(_, used)| !**used).map(|(ri, _)| ri).collect();
        if !only.is_empty() {
            let mut row = vec![Value::Null; lt.columns.len()];
            row.push(sub(only));
            rows.push(row);
        }
    }
    let mut cols = lt.columns.clone();
    cols.push(name);
    let mut types = lt.types.clone();
    types.push(TypeSpec { ty: Ty::Table, nullable: false });
    Ok(make(cols, types, rows))
}

/// `Table.Join`: the rows side by side. Inner is the default. The order: the matching rows in the order of the left rows (and the right rows
/// for each), then the right rows that match nothing, then the left rows that match nothing (outer joins); an anti join gives only those.
/// A key column of the right table with the name of its left key is not repeated (an inner join only); any other repeated name is an error.
fn join(it: &Interp, a: &[Value]) -> R {
    let (lt, rt) = (tbl(&a[0])?, tbl(&a[2])?);
    let (lki, rki) = join_keys(it, &lt, &a[1], &rt, &a[3])?;
    let kind = kind_of(a.get(4), 0)?;
    let drop: Vec<usize> = lki.iter().zip(&rki).filter(|(l, r)| lt.columns[**l] == rt.columns[**r]).map(|(_, r)| *r).collect();
    // Only an inner join may have a key of the same name on both sides (it is one column then); an outer or anti join with one is an error.
    if kind != 0 && !drop.is_empty() {
        return Err(MError::expr(format!("A join cannot make a table with a repeated column name (\"{}\").", rt.columns[drop[0]])));
    }
    let right_cols: Vec<usize> = (0..rt.columns.len()).filter(|i| !drop.contains(i)).collect();
    for &i in &right_cols {
        if lt.columns.contains(&rt.columns[i]) {
            return Err(MError::expr(format!("A join cannot make a table with a repeated column name (\"{}\").", rt.columns[i])));
        }
    }
    let m = matches_of(it, &lt, &rt, &lki, &rki)?;
    let mut cols = lt.columns.clone();
    let mut types = lt.types.clone();
    for &i in &right_cols {
        cols.push(rt.columns[i].clone());
        types.push(rt.types[i].clone());
    }
    let right_part = |ri: usize| -> Vec<Value> { right_cols.iter().map(|&i| rt.rows[ri][i].clone()).collect() };
    let mut matched = Vec::new();
    let mut left_only = Vec::new();
    for (lr, ms) in lt.rows.iter().zip(&m.left) {
        if ms.is_empty() {
            let mut row = lr.clone();
            row.extend(vec![Value::Null; right_cols.len()]);
            left_only.push(row);
        } else {
            for &ri in ms {
                let mut row = lr.clone();
                row.extend(right_part(ri));
                matched.push(row);
            }
        }
    }
    let mut right_only = Vec::new();
    for (ri, used) in m.right_used.iter().enumerate() {
        if !used {
            let mut row = vec![Value::Null; lt.columns.len()];
            // The key of the left side is filled from the right when the names are the same (they are one column then).
            row.extend(right_part(ri));
            right_only.push(row);
        }
    }
    let rows = match kind {
        0 => matched,
        1 => [matched, left_only].concat(),
        2 => [matched, right_only].concat(),
        3 => [matched, right_only, left_only].concat(),
        4 => left_only,
        5 => right_only,
        _ => return Err(MError::expr("The join kind is not valid.")),
    };
    Ok(make(cols, types, rows))
}
