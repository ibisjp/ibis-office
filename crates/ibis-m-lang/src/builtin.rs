// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The built-in library. Stage 1 has the constructors (`#table`, `#date`...), the type constants (`Int64.Type`...) and a few
//! `List`/`Text`/`Number`/`Record` functions that the language tests need; the rest comes by the order of
//! `tasks/doc/design/k0039-m-function-frequency.tsv`.
use crate::ast::{Ty, TypeSpec};
use crate::error::MError;
use crate::eval::{Interp, MAX_LIST};
use crate::value::*;
use std::collections::HashMap;
use std::rc::Rc;

type R = Result<Value, MError>;

fn ty(t: Ty) -> Value {
    Value::Type(TypeSpec { ty: t, nullable: matches!(t, Ty::Any | Ty::Null) })
}

pub fn list_items(it: &Interp, v: &Value) -> Result<Vec<Value>, MError> {
    match v {
        Value::List(items) => items.iter().map(|l| l.force(it)).collect(),
        Value::Gen(g) => g.all(it),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type List.", v.type_name()))),
    }
}

pub fn as_number(v: &Value) -> Result<f64, MError> {
    match v {
        Value::Number(n) => Ok(*n),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Number.", v.type_name()))),
    }
}

pub fn as_text(v: &Value) -> Result<Rc<str>, MError> {
    match v {
        Value::Text(s) => Ok(s.clone()),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Text.", v.type_name()))),
    }
}

/// The text of a number as `Text.From` gives it: no exponent below 1e15, shortest digits that read back the same.
pub fn number_to_text(n: f64) -> String {
    crate::culture::number_text(n)
}

macro_rules! func {
    ($lib:ident, $name:literal, $min:expr, $max:expr, $f:expr) => {
        $lib.insert($name, Value::Function(Rc::new(Func::Builtin { name: $name, min: $min, max: $max, f: $f })));
    };
}

pub fn library() -> HashMap<&'static str, Value> {
    let mut l: HashMap<&'static str, Value> = HashMap::new();
    for (n, t) in [
        ("Int64.Type", Ty::Int64),
        ("Int32.Type", Ty::Int32),
        ("Int16.Type", Ty::Int64),
        ("Int8.Type", Ty::Int64),
        ("Number.Type", Ty::Number),
        ("Double.Type", Ty::Number),
        ("Decimal.Type", Ty::Number),
        ("Single.Type", Ty::Number),
        ("Currency.Type", Ty::Currency),
        ("Percentage.Type", Ty::Percentage),
        ("Text.Type", Ty::Text),
        ("Logical.Type", Ty::Logical),
        ("Date.Type", Ty::Date),
        ("DateTime.Type", Ty::DateTime),
        ("DateTimeZone.Type", Ty::DateTimeZone),
        ("Time.Type", Ty::Time),
        ("Duration.Type", Ty::Duration),
        ("List.Type", Ty::List),
        ("Record.Type", Ty::Record),
        ("Table.Type", Ty::Table),
        ("Function.Type", Ty::Function),
        ("Any.Type", Ty::Any),
        ("Binary.Type", Ty::Binary),
    ] {
        l.insert(n, ty(t));
    }
    crate::table::register(&mut l);
    crate::table_more::register(&mut l);
    crate::lib_text::register(&mut l);
    crate::lib_list::register(&mut l);
    crate::lib_io::register(&mut l);
    crate::lib_date::register(&mut l);
    crate::lib_date::register_more(&mut l);
    crate::lib_date::register_clock(&mut l);
    func!(l, "#table", 2, 2, |it, a| {
        let cols_v = list_items(it, &a[0])?;
        let columns: Vec<String> = match &a[0] {
            Value::List(_) => cols_v.iter().map(|c| as_text(c).map(|s| s.to_string())).collect::<Result<_, _>>()?,
            _ => unreachable!(),
        };
        let mut rows = Vec::new();
        for r in list_items(it, &a[1])? {
            let r = list_items(it, &r)?;
            if r.len() != columns.len() {
                return Err(MError::expr("There are more or fewer values in a row than columns in the #table."));
            }
            rows.push(r);
        }
        let n = columns.len();
        Ok(Value::Table(Rc::new(Table { columns, types: vec![TypeSpec { ty: Ty::Any, nullable: true }; n], rows })))
    });
    func!(l, "#date", 3, 3, |_, a| {
        let (y, m, d) = (as_number(&a[0])? as i64, as_number(&a[1])? as i64, as_number(&a[2])? as i64);
        if !(1..=9999).contains(&y) || !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
            return Err(MError::expr("The date is not valid."));
        }
        Ok(Value::Date(days_from_civil(y, m, d)))
    });
    func!(l, "#time", 3, 3, |_, a| Ok(Value::Time(as_number(&a[0])? * 3600.0 + as_number(&a[1])? * 60.0 + as_number(&a[2])?)));
    func!(l, "#datetime", 6, 6, |_, a| {
        let (y, m, d) = (as_number(&a[0])? as i64, as_number(&a[1])? as i64, as_number(&a[2])? as i64);
        if !(1..=9999).contains(&y) || !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
            return Err(MError::expr("The datetime is not valid."));
        }
        Ok(Value::DateTime(days_from_civil(y, m, d), as_number(&a[3])? * 3600.0 + as_number(&a[4])? * 60.0 + as_number(&a[5])?))
    });
    func!(l, "#duration", 4, 4, |_, a| Ok(Value::Duration(as_number(&a[0])? * 86400.0 + as_number(&a[1])? * 3600.0 + as_number(&a[2])? * 60.0 + as_number(&a[3])?)));

    func!(l, "Excel.CurrentWorkbook", 0, 0, |it, _| it.global("#workbook").ok_or_else(|| MError::expr("The workbook is not available.")));
    
    func!(l, "Text.Clean", 1, 1, |_, a| text_map(&a[0], |s| s.chars().filter(|c| !c.is_control()).collect()));
    // The first letter after anything that is not a letter is upper, the other letters lower; each letter by its simple mapping (as Excel
    // does: "o'neil" is "O'Neil", "1st" is "1St", "ghi_jkl" is "Ghi_Jkl", a combining mark ends a word).
    func!(l, "Text.Proper", 1, 2, |_, a| text_map(&a[0], |s| {
        let mut out = String::new();
        let mut start = true;
        for c in s.chars() {
            if c.is_alphabetic() {
                out.push(if start { crate::culture::simple_upper(c) } else { crate::culture::simple_lower(c) });
                start = false;
            } else {
                out.push(c);
                start = true;
            }
        }
        out
    }));
    func!(l, "Number.Abs", 1, 1, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => Ok(Value::Number(as_number(v)?.abs())),
    });
    func!(l, "Number.Sqrt", 1, 1, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => Ok(Value::Number(as_number(v)?.sqrt())),
    });
    func!(l, "List.Distinct", 1, 2, |it, a| {
        let mut out: Vec<Value> = Vec::new();
        'item: for v in list_items(it, &a[0])? {
            for k in &out {
                if it.equals(k, &v)? {
                    continue 'item;
                }
            }
            out.push(v);
        }
        Ok(Value::list(out))
    });
    func!(l, "List.Contains", 2, 3, |it, a| {
        for v in list_items(it, &a[0])? {
            if it.equals(&v, &a[1])? {
                return Ok(Value::Logical(true));
            }
        }
        Ok(Value::Logical(false))
    });
    func!(l, "List.Difference", 2, 3, |it, a| {
        let drop = list_items(it, &a[1])?;
        let mut out = Vec::new();
        'item: for v in list_items(it, &a[0])? {
            for d in &drop {
                if it.equals(&v, d)? {
                    continue 'item;
                }
            }
            out.push(v);
        }
        Ok(Value::list(out))
    });
    func!(l, "List.Reverse", 1, 1, |it, a| Ok(Value::list(list_items(it, &a[0])?.into_iter().rev().collect())));
    func!(l, "List.First", 1, 2, |it, a| {
        let first = match &a[0] {
            Value::Gen(g) => g.take(it, 1)?.into_iter().next(),
            v => list_items(it, v)?.into_iter().next(),
        };
        Ok(first.unwrap_or_else(|| a.get(1).cloned().unwrap_or(Value::Null)))
    });
    func!(l, "List.Last", 1, 2, |it, a| Ok(list_items(it, &a[0])?.pop().unwrap_or_else(|| a.get(1).cloned().unwrap_or(Value::Null))));
    func!(l, "List.Average", 1, 1, |it, a| {
        let mut sum = 0.0;
        let mut n = 0usize;
        for v in list_items(it, &a[0])? {
            if !matches!(v, Value::Null) {
                sum += as_number(&v)?;
                n += 1;
            }
        }
        Ok(if n == 0 { Value::Null } else { Value::Number(sum / n as f64) })
    });
    func!(l, "List.Combine", 1, 1, |it, a| {
        let mut out = Vec::new();
        for part in list_items(it, &a[0])? {
            out.extend(list_items(it, &part)?);
        }
        Ok(Value::list(out))
    });
    func!(l, "List.RemoveNulls", 1, 1, |it, a| Ok(Value::list(list_items(it, &a[0])?.into_iter().filter(|v| !matches!(v, Value::Null)).collect())));
    func!(l, "List.Count", 1, 1, |it, a| Ok(Value::Number(list_items(it, &a[0])?.len() as f64)));
    func!(l, "List.Sum", 1, 1, |it, a| {
        let items = list_items(it, &a[0])?;
        // No value (an empty list, or only nulls) gives null (see the request, item 13).
        let mut s = None;
        for v in items {
            match v {
                Value::Null => {}
                v => *s.get_or_insert(0.0) += as_number(&v)?,
            }
        }
        Ok(s.map_or(Value::Null, Value::Number))
    });
    func!(l, "List.Max", 1, 2, |it, a| extreme(it, a, true));
    func!(l, "List.Min", 1, 2, |it, a| extreme(it, a, false));
    func!(l, "List.Transform", 2, 2, |it, a| {
        let mut out = Vec::new();
        for v in list_items(it, &a[0])? {
            out.push(it.call(&a[1], vec![v])?);
        }
        Ok(Value::list(out))
    });
    func!(l, "List.Select", 2, 2, |it, a| {
        let mut out = Vec::new();
        for v in list_items(it, &a[0])? {
            match it.call(&a[1], vec![v.clone()])? {
                Value::Logical(true) => out.push(v),
                Value::Logical(false) => {}
                r => return Err(MError::expr(format!("We cannot convert a value of type {} to type Logical.", r.type_name()))),
            }
        }
        Ok(Value::list(out))
    });
    func!(l, "List.Numbers", 2, 3, |_, a| {
        let start = as_number(&a[0])?;
        let count = as_number(&a[1])?;
        let inc = if a.len() > 2 { as_number(&a[2])? } else { 1.0 };
        if count > MAX_LIST as f64 {
            return Err(MError::limit("The list is too long."));
        }
        Ok(Value::list((0..count.max(0.0) as usize).map(|i| Value::Number(start + inc * i as f64)).collect()))
    });
    func!(l, "List.Accumulate", 3, 3, |it, a| {
        let mut acc = a[1].clone();
        for v in list_items(it, &a[0])? {
            acc = it.call(&a[2], vec![acc, v])?;
        }
        Ok(acc)
    });
    // Lazy: the items are made as they are asked for, so `List.FirstN(List.Generate(…endless…), 3)` ends.
    func!(l, "List.Generate", 3, 4, |_, a| Ok(Value::Gen(Rc::new(Generator::new(a[0].clone(), a[1].clone(), a[2].clone(), a.get(3).filter(|v| !matches!(v, Value::Null)).cloned())))));
    func!(l, "Text.Upper", 1, 2, |_, a| text_map(&a[0], |s| s.chars().map(crate::culture::simple_upper).collect()));
    func!(l, "Text.Lower", 1, 2, |_, a| text_map(&a[0], |s| s.chars().map(crate::culture::simple_lower).collect()));
    func!(l, "Text.Length", 1, 1, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => Ok(Value::Number(as_text(v)?.encode_utf16().count() as f64)),
    });
    func!(l, "Text.From", 1, 2, |it, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => {
            let c = culture_arg(it, a.get(1))?;
            crate::table::scalar_text_in(v, c).map(Value::text).ok_or_else(|| MError::expr(format!("We cannot convert a value of type {} to type Text.", v.type_name())))
        }
    });
    func!(l, "Number.From", 1, 2, |_, a| number_from(&a[0]));
    func!(l, "Number.FromText", 1, 2, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        Value::Text(s) => crate::culture::parse_number(s).map(Value::Number).ok_or_else(|| MError::format(format!("We couldn't convert to Number: \"{s}\"."))),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Text.", v.type_name()))),
    });
    func!(l, "Number.ToText", 1, 3, |it, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        Value::Number(n) => {
            let c = culture_arg(it, a.get(2))?;
            match a.get(1) {
                None | Some(Value::Null) => Ok(Value::text(crate::culture::number_text(*n))),
                Some(Value::Text(f)) if f.is_empty() => Ok(Value::text(crate::culture::number_text(*n))),
                Some(Value::Text(f)) => crate::format::format_number(*n, f, c).map(Value::text),
                Some(v) => Err(MError::expr(format!("We cannot convert a value of type {} to type Text.", v.type_name()))),
            }
        }
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Number.", v.type_name()))),
    });
    // `Text.Format("#{0} is #[x]", {1}, "en-US")`.
    func!(l, "Text.Format", 2, 3, |it, a| {
        let c = culture_arg(it, a.get(2))?;
        let text_of = |v: &Value| -> String {
            match v {
                Value::Null => "null".into(),
                Value::List(_) | Value::Gen(_) => "list".into(),
                Value::Record(_) => "record".into(),
                v => crate::table::scalar_text_in(v, c).unwrap_or_else(|| v.type_name().into()),
            }
        };
        let template = as_text(&a[0])?;
        let (mut positional, mut named) = (Vec::new(), Vec::new());
        match &a[1] {
            Value::Record(fields) => {
                for (n, l) in fields.iter() {
                    let v = l.force(it)?;
                    positional.push(text_of(&v));
                    named.push((n.clone(), text_of(&v)));
                }
            }
            v => {
                for x in list_items(it, v)? {
                    positional.push(text_of(&x));
                }
            }
        }
        crate::format::text_format(&template, &positional, &named).map(Value::text)
    });
    func!(l, "Record.Field", 2, 2, |it, a| match &a[0] {
        r @ Value::Record(_) => it.field(r, &as_text(&a[1])?, false),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Record.", v.type_name()))),
    });
    func!(l, "Record.FieldNames", 1, 1, |_, a| match &a[0] {
        Value::Record(f) => Ok(Value::list(f.iter().map(|(n, _)| Value::text(n)).collect())),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Record.", v.type_name()))),
    });
    l
}

fn text_map(v: &Value, f: impl Fn(&str) -> String) -> R {
    match v {
        Value::Null => Ok(Value::Null),
        v => Ok(Value::text(f(&as_text(v)?))),
    }
}

/// `List.Max` / `List.Min`: nulls are left out (NaN is the smallest, so `List.Min({#nan, 1})` is NaN and `List.Max({#nan, 1})` is 1), values of different types are ordered by type (text is the greatest; Excel:
/// `List.Max({1,"a"})` is "a"); an empty list gives the default (the second argument, else null).
fn extreme(it: &Interp, a: &[Value], max: bool) -> R {
    let mut best: Option<Value> = None;
    for v in list_items(it, &a[0])? {
        if matches!(v, Value::Null) {
            continue;
        }
        best = match best {
            None => Some(v),
            Some(b) => {
                let o = crate::table::order(it, &v, &b);
                Some(if (o == std::cmp::Ordering::Greater) == max && o != std::cmp::Ordering::Equal { v } else { b })
            }
        };
    }
    Ok(best.unwrap_or_else(|| a.get(1).cloned().unwrap_or(Value::Null)))
}

pub fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// The culture of a call: its argument (`"ja-JP"`, `"en-US"`, or a record `[Culture="en-US"]`), else the interpreter's.
pub fn culture_arg(it: &Interp, v: Option<&Value>) -> Result<crate::culture::Culture, MError> {
    match v {
        None | Some(Value::Null) => Ok(it.culture()),
        Some(Value::Text(s)) => crate::culture::Culture::parse(s),
        Some(r @ Value::Record(_)) => match it.field(r, "Culture", true)? {
            Value::Text(s) => crate::culture::Culture::parse(&s),
            _ => Ok(it.culture()),
        },
        Some(v) => Err(MError::expr(format!("We cannot convert a value of type {} to a culture name.", v.type_name()))),
    }
}

/// `Number.From`: a number, a logical (1 or 0), a text read as a number, a date or datetime as its serial number (days since 1899-12-30),
/// a time as the part of a day.
pub fn number_from(v: &Value) -> R {
    let frac = |s: f64| s / 86400.0;
    match v {
        Value::Null => Ok(Value::Null),
        Value::Number(n) => Ok(Value::Number(*n)),
        Value::Logical(b) => Ok(Value::Number(if *b { 1.0 } else { 0.0 })),
        Value::Text(s) => match crate::culture::parse_number(s) {
            Some(n) => Ok(Value::Number(n)),
            None if s.trim().is_empty() => Ok(Value::Null),
            None => Err(MError::format(format!("We couldn't convert to Number: \"{s}\"."))),
        },
        Value::Date(d) => Ok(Value::Number((*d - crate::table::serial_epoch()) as f64)),
        Value::DateTime(d, s) => Ok(Value::Number((*d - crate::table::serial_epoch()) as f64 + frac(*s))),
        Value::Time(s) => Ok(Value::Number(frac(*s))),
        Value::Duration(s) => Ok(Value::Number(frac(*s))),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Number.", v.type_name()))),
    }
}
