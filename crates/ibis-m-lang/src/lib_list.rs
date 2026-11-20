// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `List.*` functions (the order 2 of `tasks/doc/design/k0039-m-function-frequency.tsv`). Left for the Excel answers (the request, item 13):
//! `List.Mode` (which of equal counts), the order of nulls and mixed types in `List.Sort`, `List.Max` of text.
use crate::builtin::{as_number, list_items};
use crate::error::MError;
use crate::eval::Interp;
use crate::table::{order, reg};
use crate::value::*;
use std::collections::HashMap;

type R = Result<Value, MError>;

fn count(v: &Value) -> Result<usize, MError> {
    let n = as_number(v)?;
    if n < 0.0 || n.fract() != 0.0 {
        return Err(MError::expr("The count must be a non-negative whole number."));
    }
    Ok(n as usize)
}

fn contains(it: &Interp, list: &[Value], v: &Value) -> Result<bool, MError> {
    for x in list {
        if it.equals(x, v)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn test(it: &Interp, f: &Value, v: &Value) -> Result<bool, MError> {
    match it.call(f, vec![v.clone()])? {
        Value::Logical(b) => Ok(b),
        r => Err(MError::expr(format!("We cannot convert a value of type {} to type Logical.", r.type_name()))),
    }
}

fn distinct(it: &Interp, items: Vec<Value>) -> Result<Vec<Value>, MError> {
    let mut out: Vec<Value> = Vec::new();
    for v in items {
        if !contains(it, &out, &v)? {
            out.push(v);
        }
    }
    Ok(out)
}

fn numbers(it: &Interp, v: &Value) -> Result<Vec<f64>, MError> {
    let mut out = Vec::new();
    for x in list_items(it, v)? {
        match x {
            Value::Null => {}
            x => out.push(as_number(&x)?),
        }
    }
    Ok(out)
}

pub fn register(l: &mut HashMap<&'static str, Value>) {
    reg(l, "List.Sort", 1, 2, |it, a| {
        let mut items = list_items(it, &a[0])?;
        let desc = matches!(a.get(1), Some(Value::Number(n)) if *n == 1.0);
        // A comparer (`Comparer.OrdinalIgnoreCase`) orders the items itself; the sort is stable.
        if let Some(f @ Value::Function(_)) = a.get(1) {
            items.sort_by(|x, y| match it.call(f, vec![x.clone(), y.clone()]) {
                Ok(Value::Number(n)) if n < 0.0 => std::cmp::Ordering::Less,
                Ok(Value::Number(n)) if n > 0.0 => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            });
            return Ok(Value::list(items));
        }
        items.sort_by(|x, y| if desc { order(it, y, x) } else { order(it, x, y) });
        Ok(Value::list(items))
    });
    // The mode: the value that occurs most; of equal counts the one whose group comes last (Excel: `List.Mode({1,1,2,2})` is 2, `{2,2,1,1}` is 1).
    reg(l, "List.Mode", 1, 2, |it, a| {
        let modes = modes(it, &a[0])?;
        Ok(modes.last().cloned().unwrap_or(Value::Null))
    });
    reg(l, "List.Modes", 1, 2, |it, a| Ok(Value::list(modes(it, &a[0])?)));
    reg(l, "List.Median", 1, 2, |it, a| {
        let mut v = numbers(it, &a[0])?;
        if v.is_empty() {
            return Ok(Value::Null);
        }
        v.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        let n = v.len();
        Ok(Value::Number(if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }))
    });
    reg(l, "List.StandardDeviation", 1, 1, |it, a| {
        let v = numbers(it, &a[0])?;
        match v.len() {
            0 => return Ok(Value::Null),
            1 => return Ok(Value::Number(0.0)),
            _ => {}
        }
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        Ok(Value::Number((v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt()))
    });
    reg(l, "List.Product", 1, 1, |it, a| {
        let v = numbers(it, &a[0])?;
        Ok(if v.is_empty() { Value::Null } else { Value::Number(v.iter().product()) })
    });
    reg(l, "List.NonNullCount", 1, 1, |it, a| Ok(Value::Number(list_items(it, &a[0])?.iter().filter(|v| !matches!(v, Value::Null)).count() as f64)));
    reg(l, "List.FirstN", 2, 2, |it, a| {
        if let Value::Function(_) = &a[1] {
            // Items are asked for one at a time, so an endless list ends when the test fails.
            let mut out = Vec::new();
            if let Value::Gen(g) = &a[0] {
                let mut k = 0;
                loop {
                    let got = g.take(it, k + 1)?;
                    let Some(v) = got.get(k) else { break };
                    if !test(it, &a[1], v)? {
                        break;
                    }
                    out.push(v.clone());
                    k += 1;
                }
                return Ok(Value::list(out));
            }
            for v in list_items(it, &a[0])? {
                if !test(it, &a[1], &v)? {
                    break;
                }
                out.push(v);
            }
            return Ok(Value::list(out));
        }
        let n = count(&a[1])?;
        if let Value::Gen(g) = &a[0] {
            return Ok(Value::list(g.take(it, n)?));
        }
        Ok(Value::list(list_items(it, &a[0])?.into_iter().take(n).collect()))
    });
    reg(l, "List.LastN", 2, 2, |it, a| {
        let items = list_items(it, &a[0])?;
        let skip = items.len().saturating_sub(count(&a[1])?);
        Ok(Value::list(items.into_iter().skip(skip).collect()))
    });
    reg(l, "List.Skip", 2, 2, |it, a| Ok(Value::list(list_items(it, &a[0])?.into_iter().skip(count(&a[1])?).collect())));
    reg(l, "List.RemoveFirstN", 1, 2, |it, a| {
        let n = a.get(1).map(count).transpose()?.unwrap_or(1);
        Ok(Value::list(list_items(it, &a[0])?.into_iter().skip(n).collect()))
    });
    reg(l, "List.RemoveLastN", 1, 2, |it, a| {
        let items = list_items(it, &a[0])?;
        let keep = items.len().saturating_sub(a.get(1).map(count).transpose()?.unwrap_or(1));
        Ok(Value::list(items.into_iter().take(keep).collect()))
    });
    reg(l, "List.Range", 2, 3, |it, a| {
        let off = count(&a[1])?;
        // A range of an endless list takes only what it needs.
        let items = match (&a[0], a.get(2)) {
            (Value::Gen(g), Some(c)) => g.take(it, off + count(c)?)?,
            _ => list_items(it, &a[0])?,
        };
        let end = match a.get(2) {
            Some(n) => off + count(n)?,
            None => items.len(),
        };
        if off > items.len() || end > items.len() {
            return Err(MError::expr("There weren't enough elements in the enumeration to complete the operation."));
        }
        Ok(Value::list(items[off..end].to_vec()))
    });
    reg(l, "List.Union", 1, 2, |it, a| {
        let mut all = Vec::new();
        for part in list_items(it, &a[0])? {
            all.extend(list_items(it, &part)?);
        }
        Ok(Value::list(distinct(it, all)?))
    });
    reg(l, "List.Intersect", 1, 2, |it, a| {
        let parts: Vec<Vec<Value>> = list_items(it, &a[0])?.iter().map(|p| list_items(it, p)).collect::<Result<_, _>>()?;
        let Some((first, rest)) = parts.split_first() else { return Ok(Value::list(vec![])) };
        let mut out = Vec::new();
        for v in distinct(it, first.clone())? {
            let mut all = true;
            for r in rest {
                if !contains(it, r, &v)? {
                    all = false;
                    break;
                }
            }
            if all {
                out.push(v);
            }
        }
        Ok(Value::list(out))
    });
    reg(l, "List.Zip", 1, 1, |it, a| {
        let lists: Vec<Vec<Value>> = list_items(it, &a[0])?.iter().map(|p| list_items(it, p)).collect::<Result<_, _>>()?;
        let n = lists.iter().map(Vec::len).max().unwrap_or(0);
        Ok(Value::list((0..n).map(|i| Value::list(lists.iter().map(|l| l.get(i).cloned().unwrap_or(Value::Null)).collect())).collect()))
    });
    reg(l, "List.PositionOf", 2, 4, |it, a| {
        let items = list_items(it, &a[0])?;
        let mode = match a.get(2) {
            Some(Value::Number(n)) => *n as i32,
            _ => 0,
        };
        let mut hits = Vec::new();
        for (i, v) in items.iter().enumerate() {
            if it.equals(v, &a[1])? {
                hits.push(i);
            }
        }
        Ok(match mode {
            2 => Value::list(hits.into_iter().map(|i| Value::Number(i as f64)).collect()),
            1 => Value::Number(hits.last().map_or(-1.0, |&i| i as f64)),
            _ => Value::Number(hits.first().map_or(-1.0, |&i| i as f64)),
        })
    });
    reg(l, "List.Positions", 1, 1, |it, a| Ok(Value::list((0..list_items(it, &a[0])?.len()).map(|i| Value::Number(i as f64)).collect())));
    reg(l, "List.ReplaceValue", 4, 4, |it, a| {
        let mut out = Vec::new();
        for v in list_items(it, &a[0])? {
            out.push(it.call(&a[3], vec![v, a[1].clone(), a[2].clone()])?);
        }
        Ok(Value::list(out))
    });
    reg(l, "List.IsEmpty", 1, 1, |it, a| Ok(Value::Logical(list_items(it, &a[0])?.is_empty())));
    reg(l, "List.AllTrue", 1, 1, |it, a| logical(it, &a[0], true));
    reg(l, "List.AnyTrue", 1, 1, |it, a| logical(it, &a[0], false));
    reg(l, "List.MatchesAll", 2, 2, |it, a| {
        for v in list_items(it, &a[0])? {
            if !test(it, &a[1], &v)? {
                return Ok(Value::Logical(false));
            }
        }
        Ok(Value::Logical(true))
    });
    reg(l, "List.MatchesAny", 2, 2, |it, a| {
        for v in list_items(it, &a[0])? {
            if test(it, &a[1], &v)? {
                return Ok(Value::Logical(true));
            }
        }
        Ok(Value::Logical(false))
    });
    reg(l, "List.Buffer", 1, 1, |it, a| Ok(Value::list(list_items(it, &a[0])?)));
    reg(l, "List.Single", 1, 1, |it, a| {
        let mut items = list_items(it, &a[0])?;
        if items.len() == 1 { Ok(items.remove(0)) } else { Err(MError::expr("The list must have exactly one element.")) }
    });
    reg(l, "List.SingleOrDefault", 1, 2, |it, a| {
        let mut items = list_items(it, &a[0])?;
        match items.len() {
            0 => Ok(a.get(1).cloned().unwrap_or(Value::Null)),
            1 => Ok(items.remove(0)),
            _ => Err(MError::expr("The list must have at most one element.")),
        }
    });
    reg(l, "List.Repeat", 2, 2, |it, a| {
        let items = list_items(it, &a[0])?;
        let n = count(&a[1])?;
        if items.len().saturating_mul(n) > crate::eval::MAX_LIST {
            return Err(MError::limit("The list is too long."));
        }
        Ok(Value::list((0..n).flat_map(|_| items.clone()).collect()))
    });
    reg(l, "List.ContainsAny", 2, 3, |it, a| {
        let items = list_items(it, &a[0])?;
        for v in list_items(it, &a[1])? {
            if contains(it, &items, &v)? {
                return Ok(Value::Logical(true));
            }
        }
        Ok(Value::Logical(false))
    });
    reg(l, "List.ContainsAll", 2, 3, |it, a| {
        let items = list_items(it, &a[0])?;
        for v in list_items(it, &a[1])? {
            if !contains(it, &items, &v)? {
                return Ok(Value::Logical(false));
            }
        }
        Ok(Value::Logical(true))
    });
    reg(l, "List.RemoveItems", 2, 2, |it, a| {
        let drop = list_items(it, &a[1])?;
        let mut out = Vec::new();
        for v in list_items(it, &a[0])? {
            if !contains(it, &drop, &v)? {
                out.push(v);
            }
        }
        Ok(Value::list(out))
    });
    reg(l, "List.Dates", 3, 3, |_, a| {
        let (Value::Date(start), Value::Duration(step)) = (&a[0], &a[2]) else { return Err(MError::expr("List.Dates needs a date, a count and a duration.")) };
        let n = count(&a[1])?;
        if n > crate::eval::MAX_LIST {
            return Err(MError::limit("The list is too long."));
        }
        let step_days = (step / 86400.0).floor() as i64;
        Ok(Value::list((0..n).map(|i| Value::Date(start + step_days * i as i64)).collect()))
    });
}

fn logical(it: &Interp, v: &Value, all: bool) -> R {
    for x in list_items(it, v)? {
        match x {
            Value::Logical(b) if b != all => return Ok(Value::Logical(b)),
            Value::Logical(_) => {}
            x => return Err(MError::expr(format!("We cannot convert a value of type {} to type Logical.", x.type_name()))),
        }
    }
    Ok(Value::Logical(all))
}

/// The values with the highest count, in the order of their first appearance.
fn modes(it: &Interp, v: &Value) -> Result<Vec<Value>, MError> {
    let mut groups: Vec<(Value, usize)> = Vec::new();
    'item: for x in list_items(it, v)? {
        for g in groups.iter_mut() {
            if it.equals(&g.0, &x)? {
                g.1 += 1;
                continue 'item;
            }
        }
        groups.push((x, 1));
    }
    let top = groups.iter().map(|g| g.1).max().unwrap_or(0);
    Ok(groups.into_iter().filter(|g| g.1 == top).map(|g| g.0).collect())
}
