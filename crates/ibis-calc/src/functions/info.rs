// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Information functions.

use super::{Func, err, number};
use crate::eval::{Array, Ctx, Value, to_number};
use crate::formula::Expr;
use crate::value::CellError;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "NA" => |_, _| err(CellError::NA),
        "ISBLANK" => |c, a| is(c, a, |v| matches!(v, Value::Empty)),
        "ISERR" => |c, a| is(c, a, |v| matches!(v, Value::Error(e) if *e != CellError::NA)),
        "ISERROR" => |c, a| is(c, a, |v| matches!(v, Value::Error(_))),
        "ISNA" => |c, a| is(c, a, |v| matches!(v, Value::Error(CellError::NA))),
        "ISLOGICAL" => |c, a| is(c, a, |v| matches!(v, Value::Bool(_))),
        "ISNUMBER" => |c, a| is(c, a, |v| matches!(v, Value::Number(_))),
        "ISTEXT" => |c, a| is(c, a, |v| matches!(v, Value::Text(_))),
        "ISNONTEXT" => |c, a| is(c, a, |v| !matches!(v, Value::Text(_))),
        "ISEVEN" => |c, a| parity(c, a, true),
        "ISODD" => |c, a| parity(c, a, false),
        "ISREF" => |c, a| Value::Bool(matches!(c.eval(&a[0]), Value::Ref(_))),
        "ISOMITTED" => |c, a| Value::Bool(matches!(a[0], Expr::Missing) || matches!(c.eval(&a[0]), Value::Omitted)),
        "ISFORMULA" => |c, a| match c.eval(&a[0]) {
            // A range gives one answer per cell (Excel).
            Value::Ref(r) => {
                let area = r[0];
                let (rows, columns) = (area.rows().min(1_048_576) as usize, area.columns().min(16_384) as usize);
                let data: Vec<Value> = (0..rows * columns)
                    .map(|k| Value::Bool(c.book.formula(area.sheet, area.first_row + (k / columns) as u32, area.first_column + (k % columns) as u32).is_some()))
                    .collect();
                if data.len() == 1 { data[0].clone() } else { Value::Array(std::rc::Rc::new(crate::eval::Array::new(rows, columns, data))) }
            }
            Value::Error(e) => err(e),
            _ => err(CellError::Value),
        },
        "N" => |c, a| {
            // A number, as Excel's N makes it of one value: a number as it is, TRUE 1, FALSE 0, an error as it is, anything else (text,
            // an empty cell) 0, and a function value #VALUE!.
            fn one(v: Value) -> Value {
                match v {
                    Value::Number(n) => number(n),
                    Value::Bool(b) => number(if b { 1.0 } else { 0.0 }),
                    Value::Error(e) => err(e),
                    Value::Lambda(_) => err(CellError::Value),
                    _ => number(0.0),
                }
            }
            match c.eval(&a[0]) {
                // An array gives an array, each element made a number (Excel 365 and `=N({TRUE,FALSE,"a",5})` is {1,0,0,5}, measured on
                // Excel 16.0.20527: #141); a range is its first cell.
                Value::Array(arr) => Value::Array(std::rc::Rc::new(crate::eval::Array::new(arr.rows, arr.columns, arr.data.iter().cloned().map(one).collect()))),
                Value::Ref(r) if !r.is_empty() => {
                    let area = r[0];
                    one(c.cell_value(area.sheet, area.first_row, area.first_column))
                }
                Value::Ref(_) => err(CellError::Ref),
                v => one(v),
            }
        },
        "TYPE" => |c, a| {
            // The argument is taken as an array (Excel: `TYPE(A1:A2+0)` is 64 in every row).
            let previous = c.in_array_context;
            c.in_array_context = true;
            let evaluated = c.eval(&a[0]);
            c.in_array_context = previous;
            let v = match evaluated {
                // Checked against Excel for Mac 16.114 (dynamic arrays), #69: a range of
                // more than one cell is 64 wherever the formula is (no implicit
                // intersection); a union such as (A1,A2) is an error (16).
                Value::Ref(r) if r.len() > 1 => return number(16.0),
                Value::Ref(r) if r.first().is_some_and(|a| !a.is_cell()) => return number(64.0),
                Value::Ref(r) => c.deref_implicit(&r),
                v => v,
            };
            number(match v {
                Value::Number(_) | Value::Empty | Value::Omitted => 1.0,
                Value::Text(_) => 2.0,
                Value::Bool(_) => 4.0,
                Value::Error(_) => 16.0,
                Value::Array(_) => 64.0,
                Value::Lambda(_) => 128.0,
                Value::Ref(_) => 64.0,
            })
        },
        "ERROR.TYPE" => |c, a| match c.eval_scalar(&a[0]) {
            Value::Error(e) => e.type_number().map_or(err(CellError::NA), number),
            _ => err(CellError::NA),
        },
        "SHEET" => |c, a| {
            if a.is_empty() {
                return number(c.sheet as f64 + 1.0);
            }
            match c.eval(&a[0]) {
                Value::Ref(r) => number(r[0].sheet as f64 + 1.0),
                Value::Text(t) => match c.book.sheet_index(&t) {
                    Some(i) => number(i as f64 + 1.0),
                    None => err(CellError::NA),
                },
                Value::Error(e) => err(e),
                _ => err(CellError::NA),
            }
        },
        "SHEETS" => |c, a| {
            if a.is_empty() {
                return number(c.book.sheet_count() as f64);
            }
            match c.eval(&a[0]) {
                Value::Ref(r) => number(r.iter().map(|x| x.last_sheet - x.sheet + 1).sum::<usize>() as f64),
                Value::Error(e) => err(e),
                _ => err(CellError::NA),
            }
        },
        "INFO" => |c, a| {
            let t = match super::arg_text(c, a, 0) {
                Ok(t) => t.to_lowercase(),
                Err(e) => return err(e),
            };
            match t.as_str() {
                "numfile" => number(c.book.sheet_count() as f64),
                "recalc" => Value::Text("自動".into()),
                "system" => Value::Text("mac".into()),
                "memavail" | "memused" | "totmem" => err(CellError::NA),
                "origin" => Value::Text("$A:$A$1".into()),
                "directory" => {
                    c.unsupported = true;
                    Value::Text(match crate::eval::convert::workbook_file() {
                        Some(path) => match path.rfind(['/', '\\']) {
                            Some(at) => path[..=at].to_string(),
                            None => String::new(),
                        },
                        None => String::new(),
                    })
                }
                "osversion" => {
                    c.unsupported = true;
                    Value::Text(if cfg!(target_os = "windows") {
                        "Windows (64-bit)".into()
                    } else if cfg!(target_os = "macos") {
                        "macOS".into()
                    } else {
                        "Unix".into()
                    })
                }
                "release" => {
                    c.unsupported = true;
                    Value::Text("16.0".into())
                }
                _ => err(CellError::Value),
            }
        },
        "CELL" => cell,
        _ => return None,
    })
}

fn is(c: &mut Ctx, a: &[Expr], f: fn(&Value) -> bool) -> Value {
    let v = c.eval_scalar_or_array(&a[0]);
    c.map1(v, |_, v| Value::Bool(f(&v)))
}

fn parity(c: &mut Ctx, a: &[Expr], even: bool) -> Value {
    let v = c.eval_scalar_or_array(&a[0]);
    let date1904 = c.date1904();
    c.map1(v, |_, v| {
        if let Value::Bool(_) = v {
            return err(CellError::Value);
        }
        match to_number(&v, date1904) {
            Ok(n) => Value::Bool((n.trunc() % 2.0 == 0.0) == even),
            Err(e) => err(e),
        }
    })
}

fn cell(c: &mut Ctx, a: &[Expr]) -> Value {
    let what = match super::arg_text(c, a, 0) {
        Ok(t) => t.to_lowercase(),
        Err(e) => return err(e),
    };
    match a.get(1).map(|e| c.eval(e)) {
        Some(Value::Ref(r)) => cell_of(c, &what, &r),
        Some(Value::Error(e)) => err(e),
        // Each element of an array that is a reference is read as one; the others are #VALUE!.
        Some(Value::Array(x)) => {
            let data: Vec<Value> = x.data.iter().map(|v| match v {
                Value::Ref(r) => cell_of(c, &what, r),
                _ => err(CellError::Value),
            }).collect();
            Value::Array(std::rc::Rc::new(Array::new(x.rows, x.columns, data)))
        }
        Some(_) => err(CellError::Value),
        None => cell_of(c, &what, &[crate::eval::Area::cell(c.sheet, c.row, c.column)]),
    }
}

/// CELL of the reference `areas` (the first area's cell for most infos; the address of a union lists them all).
fn cell_of(c: &mut Ctx, what: &str, areas: &[crate::eval::Area]) -> Value {
    let Some(&area) = areas.first() else { return err(CellError::Ref) };
    match what {
        "row" => number(area.first_row as f64 + 1.0),
        "col" => number(area.first_column as f64 + 1.0),
        "address" => {
            // The first area is only its first cell, the others are whole (Excel: `$A$1,$C$1:$C$2` for `(A1:A2,C1:C2)`).
            let texts: Vec<String> = areas
                .iter()
                .enumerate()
                .map(|(n, area)| {
                    let (last_row, last_column) = if n == 0 { (area.first_row, area.first_column) } else { (area.last_row, area.last_column) };
                    let r = crate::formula::AreaRef {
                        sheet: None,
                        start: crate::formula::Corner { row: area.first_row, column: area.first_column, row_fixed: true, column_fixed: true },
                        end: crate::formula::Corner { row: last_row, column: last_column, row_fixed: true, column_fixed: true },
                        whole_columns: false,
                        whole_rows: false,
                    };
                    let mut text = crate::formula::write_reference(&r);
                    if area.sheet != c.sheet {
                        // Another sheet's cell: its book and sheet in quotes, as `'[Book1.xlsx]Sheet1'!$B$2`.
                        let book = crate::eval::convert::workbook_file().and_then(|p| p.rsplit(['/', '\\']).next().map(str::to_string)).unwrap_or_else(|| "Book1".to_string());
                        let sheet = c.book.sheet_name(area.sheet).replace('\'', "''");
                        text = format!("'[{book}]{sheet}'!{text}");
                    }
                    text
                })
                .collect();
            Value::Text(texts.join(","))
        }
        "filename" => {
            // The path, the book's name in brackets and the sheet's: empty before the workbook is saved.
            let sheet = c.book.sheet_name(area.sheet);
            Value::Text(match crate::eval::convert::workbook_file() {
                Some(path) => match path.rfind(['/', '\\']) {
                    Some(at) => format!("{}[{}]{sheet}", &path[..=at], &path[at + 1..]),
                    None => format!("[{path}]{sheet}"),
                },
                None => String::new(),
            })
        }
        "contents" => match c.cell_value(area.sheet, area.first_row, area.first_column) {
            Value::Empty => number(0.0),
            v => v,
        },
        "type" => Value::Text(
            if c.book.formula(area.sheet, area.first_row, area.first_column).is_some() {
                "v"
            } else {
                match c.cell_value(area.sheet, area.first_row, area.first_column) {
                    Value::Empty => "b",
                    Value::Text(_) => "l",
                    _ => "v",
                }
            }
            .into(),
        ),
        "color" | "format" | "parentheses" | "prefix" | "protect" | "width" => {
            let Some(info) = c.book.cell_info(area.sheet, area.first_row, area.first_column) else {
                c.unsupported = true;
                return err(CellError::Value);
            };
            match what {
                "color" => number(f64::from(format_flags(&info.number_format).1)),
                "parentheses" => number(f64::from(format_flags(&info.number_format).2)),
                "format" => Value::Text(format_code(&info.number_format)),
                "protect" => number(f64::from(info.locked)),
                "width" => number(info.column_width.floor()),
                _ => {
                    // The prefix of a text cell by its alignment; nothing for the others.
                    let is_text = matches!(c.cell_value(area.sheet, area.first_row, area.first_column), Value::Text(_));
                    Value::Text(if is_text { info.prefix.map_or(String::new(), |p| p.to_string()) } else { String::new() })
                }
            }
        }
        _ => err(CellError::Value),
    }
}


/// (decimal places, negatives in colour, positives or all in parentheses) of a number format's code.
fn format_flags(code: &str) -> (usize, u8, u8) {
    let mut sections = code.split(';');
    let first = sections.next().unwrap_or("");
    let negative = sections.next().unwrap_or("");
    let colour = negative.to_ascii_lowercase().contains("[red]") || negative.to_ascii_lowercase().contains("[color");
    let parentheses = first.contains('(');
    let decimals = first.split_once('.').map_or(0, |(_, rest)| rest.chars().take_while(|c| matches!(c, '0' | '#' | '?')).count());
    (decimals, u8::from(colour), u8::from(parentheses))
}

/// `CELL("format")`: Excel's code for a number format.
fn format_code(code: &str) -> String {
    let first = code.split(';').next().unwrap_or("").trim();
    let lower = first.to_ascii_lowercase();
    if lower.is_empty() || lower == "general" || lower == "@" {
        return "G".into();
    }
    let (decimals, colour, _) = format_flags(code);
    let dash = if colour == 1 { "-" } else { "" };
    match lower.replace(' ', "").as_str() {
        "d-mmm-yy" | "dd-mmm-yy" => return "D1".into(),
        "d-mmm" | "dd-mmm" => return "D2".into(),
        "mmm-yy" => return "D3".into(),
        "m/d/yy" | "mm/dd/yy" | "m/d/yyyy" | "mm/dd/yyyy" | "yyyy/m/d" | "yyyy/mm/dd" | "yyyy-mm-dd" | "m/d/yy h:mm" | "yyyy/m/dh:mm" => return "D4".into(),
        "mm/dd" | "m/d" => return "D5".into(),
        "h:mm:ssam/pm" => return "D6".into(),
        "h:mmam/pm" => return "D7".into(),
        "h:mm:ss" | "hh:mm:ss" => return "D8".into(),
        "h:mm" | "hh:mm" => return "D9".into(),
        _ => {}
    }
    if lower.contains("e+") || lower.contains("e-") {
        return format!("S{decimals}");
    }
    if lower.contains('%') {
        return format!("P{decimals}");
    }
    if first.contains('$') || first.contains('¥') || first.contains('€') || first.contains('£') {
        return format!("C{decimals}{dash}");
    }
    if lower.contains('/') && lower.contains('?') {
        return "G".into();
    }
    if lower.chars().any(|c| matches!(c, 'y' | 'd' | 'h' | 's' | 'm')) && !lower.contains('"') {
        return "G".into();
    }
    if first.contains('0') || first.contains('#') {
        return if first.contains("#,##") || first.contains("#,#") { format!(",{decimals}{dash}") } else { format!("F{decimals}{dash}") };
    }
    "G".into()
}
