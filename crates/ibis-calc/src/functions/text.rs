// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Text functions. Positions and lengths use the legacy UTF-16 convention
//! (a character outside the BMP counts two). LEN/MID/REPLACE/FIND/SEARCH respect
//! explicit workbook Compatibility Version 2.
//! The `B` variants count a
//! double-byte (full-width) character as two, as the Japanese Excel does.

use std::rc::Rc;

use regex::RegexBuilder;

use super::{Func, arg_array, array_value, collect_values, err, number, numeric};
use crate::eval::{Array, Ctx, Value, to_number, to_text};
use crate::formula::Expr;
use crate::value::CellError;

/// Excel's longest text in a cell.
pub(crate) const MAX_TEXT: usize = 32767;

pub(super) fn get(name: &str) -> Option<Func> {
    Some(match name {
        "LEN" => |c, a| texts(c, a, |t, c| number(if c.book.compatibility_version() >= 2 { t.chars().count() } else { t.encode_utf16().count() } as f64)),
        "LENB" => |c, a| texts(c, a, |t, _| number(t.chars().map(byte_width).sum::<usize>() as f64)),
        "LOWER" => |c, a| texts(c, a, |t, _| Value::Text(t.to_lowercase())),
        "UPPER" => |c, a| texts(c, a, |t, _| Value::Text(t.to_uppercase())),
        "PROPER" => |c, a| texts(c, a, |t, _| Value::Text(proper(&t))),
        "TRIM" => |c, a| texts(c, a, |t, _| Value::Text(trim(&t))),
        "CLEAN" => |c, a| texts(c, a, |t, _| Value::Text(t.chars().filter(|&ch| ch as u32 >= 32).collect())),
        "T" => |c, a| {
            // The text of one value: text as it is, an error as it is, anything else the empty text, and a function value #VALUE!.
            fn one(v: Value) -> Value {
                match v {
                    Value::Lambda(_) => err(CellError::Value),
                    Value::Text(t) => Value::Text(t),
                    Value::Error(e) => err(e),
                    _ => Value::Text(String::new()),
                }
            }
            match c.eval(&a[0]) {
                // An array gives an array, each element made text (`=T({"a",1,TRUE})` is {"a","",""}: Excel 16.0.20527, #141); a range
                // is its first cell.
                Value::Array(arr) => Value::Array(Rc::new(Array::new(arr.rows, arr.columns, arr.data.iter().cloned().map(one).collect()))),
                Value::Ref(r) if !r.is_empty() => {
                    let area = r[0];
                    one(c.cell_value(area.sheet, area.first_row, area.first_column))
                }
                Value::Ref(_) => err(CellError::Ref),
                v => one(v),
            }
        },
        "EXACT" => |c, a| {
            c.map_args(a, |_, v| match (to_text(&v[0]), to_text(&v[1])) {
                (Ok(x), Ok(y)) => Value::Bool(x == y),
                (Err(e), _) | (_, Err(e)) => err(e),
            })
        },
        "LEFT" => |c, a| left_right(c, a, true),
        "LEFTB" => |c, a| left_right_b(c, a, true),
        "RIGHT" => |c, a| left_right(c, a, false),
        "RIGHTB" => |c, a| left_right_b(c, a, false),
        "MID" => |c, a| mid(c, a, true),
        "MIDB" => midb,
        "REPLACE" => |c, a| replace(c, a, true),
        "REPLACEB" => replaceb,
        "REPT" => |c, a| {
            c.map_args(a, |c, v| {
                let t = match to_text(&v[0]) {
                    Ok(t) => t,
                    Err(e) => return err(e),
                };
                let n = match to_number(&v[1], c.date1904()) {
                    Ok(n) => n.trunc(),
                    Err(e) => return err(e),
                };
                if n < 0.0 {
                    return err(CellError::Value);
                }
                if utf16(&t).len() as f64 * n > MAX_TEXT as f64 {
                    return err(CellError::Value);
                }
                Value::Text(t.repeat(n as usize))
            })
        },
        "FIND" => |c, a| find(c, a, true, true),
        "FINDB" => |c, a| find(c, a, true, false),
        "SEARCH" => |c, a| find(c, a, false, true),
        "SEARCHB" => |c, a| find(c, a, false, false),
        "SUBSTITUTE" => substitute,
        "CONCATENATE" => |c, a| {
            c.map_args(a, |_, v| {
                let mut out = String::new();
                for x in v {
                    match to_text(x) {
                        Ok(t) => out.push_str(&t),
                        Err(e) => return err(e),
                    }
                }
                limit(out)
            })
        },
        "CONCAT" => |c, a| {
            let mut out = String::new();
            for (v, _) in collect_values(c, a, false) {
                match to_text(&v) {
                    Ok(t) => out.push_str(&t),
                    Err(e) => return err(e),
                }
            }
            limit(out)
        },
        "TEXTJOIN" => textjoin,
        "CHAR" => |c, a| {
            numeric(c, a, |n| {
                let n = n[0].trunc();
                if !(1.0..=255.0).contains(&n) {
                    return err(CellError::Value);
                }
                match char_of(n as u32) {
                    Some(ch) => Value::Text(ch.to_string()),
                    None => err(CellError::Value),
                }
            })
        },
        "CODE" => |c, a| {
            texts(c, a, |t, _| match t.chars().next() {
                None => err(CellError::Value),
                Some(ch) => match code_of(ch) {
                    Some(n) => number(n as f64),
                    None => err(CellError::NA),
                },
            })
        },
        "UNICHAR" => |c, a| {
            numeric(c, a, |n| {
                let n = n[0].trunc();
                if n < 1.0 || n > 0x10FFFF as f64 {
                    return err(CellError::Value);
                }
                match char::from_u32(n as u32) {
                    Some(ch) if !(0xFDD0..=0xFDEF).contains(&(n as u32)) && (n as u32) & 0xFFFE != 0xFFFE => Value::Text(ch.to_string()),
                    _ => err(CellError::NA),
                }
            })
        },
        "UNICODE" => |c, a| texts(c, a, |t, _| t.chars().next().map_or(err(CellError::Value), |ch| number(ch as u32 as f64))),
        "VALUE" => |c, a| {
            let v = c.eval_scalar_or_array(&a[0]);
            let date1904 = c.date1904();
            c.map1(v, |_, v| match v {
                Value::Number(n) => number(n),
                Value::Empty => number(0.0),
                Value::Bool(_) => err(CellError::Value),
                Value::Text(t) => crate::eval::parse_number_text(&t, date1904).map_or(err(CellError::Value), number),
                Value::Error(e) => err(e),
                _ => err(CellError::Value),
            })
        },
        "NUMBERVALUE" => numbervalue,
        "ASC" => |c, a| texts(c, a, |t, _| Value::Text(to_half_width(&t))),
        "DBCS" | "JIS" => |c, a| texts(c, a, |t, _| Value::Text(to_full_width(&t))),
        "ENCODEURL" => |c, a| texts(c, a, |t, _| limit(encode_url(&t))),
        "NUMBERSTRING" => |c, a| {
            let d1904 = c.date1904();
            c.map_args(a, move |_, v| {
                let n = match to_number(&v[0], d1904) {
                    Ok(n) => n.round(),
                    Err(e) => return err(e),
                };
                let kind = match to_number(&v[1], d1904) {
                    Ok(k) => k.trunc() as i32,
                    Err(e) => return err(e),
                };
                if n < 0.0 || !(1..=3).contains(&kind) || n > 999_999_999_999_999.0 {
                    return err(CellError::Num);
                }
                let s = (n as u64).to_string();
                let text = match kind {
                    1 => crate::numfmt::dbnum::positional(&s, 1),
                    2 => crate::numfmt::dbnum::positional(&s, 2),
                    3 => crate::numfmt::dbnum::digits(&s, 1),
                    _ => unreachable!(),
                };
                limit(text)
            })
        },
        "FILTERXML" => super::xml::filterxml,
        "WEBSERVICE" => |c, a| {
            let v = c.eval_scalar(&a[0]);
            let url = match to_text(&v) {
                Ok(u) => u,
                Err(e) => return err(e),
            };
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return err(CellError::Value);
            }
            if url.len() > 2048 {
                return err(CellError::Value);
            }
            c.unsupported = true;
            err(CellError::Value)
        },
        "FIXED" => fixed,
        "TEXT" => |c, a| {
            let d1904 = c.date1904();
            c.map_args(a, move |_, v| {
                let code = match to_text(&v[1]) {
                    Ok(t) => t,
                    Err(e) => return err(e),
                };
                // An empty format shows nothing.
                if code.is_empty() {
                    return Value::Text(String::new());
                }
                let sections = crate::numfmt::parsed(&code);
                if sections.iter().any(|s| s.color.is_some() || s.invalid) {
                    return err(CellError::Value);
                }
                let value = match &v[0] {
                    Value::Error(e) => return err(*e),
                    Value::Number(n) => crate::CellValue::Number(*n),
                    Value::Empty => crate::CellValue::Number(0.0),
                    Value::Bool(b) => crate::CellValue::Bool(*b),
                    Value::Text(t) => match crate::eval::parse_number_text(t, d1904) {
                        Some(n) => crate::CellValue::Number(n),
                        None => crate::CellValue::Text(t.clone()),
                    },
                    _ => return err(CellError::Value),
                };
                let f = crate::numfmt::format(&value, &code, d1904);
                if f.overflow {
                    return err(CellError::Value);
                }
                limit(f.text)
            })
        },
        "DOLLAR" => dollar,
        "TEXTBEFORE" => |c, a| text_before_after(c, a, true),
        "TEXTAFTER" => |c, a| text_before_after(c, a, false),
        "TEXTSPLIT" => textsplit,
        "VALUETOTEXT" => |c, a| {
            let strict = match a.get(1) {
                Some(_) => super::arg_num(c, a, 1).map(|n| n.trunc()),
                None => Ok(0.0),
            };
            let strict = match strict {
                Ok(n) if n == 0.0 || n == 1.0 => n == 1.0,
                Ok(_) => return err(CellError::Value),
                Err(e) => return err(e),
            };
            let v = c.eval_scalar_or_array(&a[0]);
            c.map1(v, |_, v| Value::Text(value_to_text(&v, strict)))
        },
        "ARRAYTOTEXT" => arraytotext,
        "PHONETIC" => |c, a| {
            let mut out = String::new();
            for arg in a {
                match c.eval(arg) {
                    Value::Ref(areas) => {
                        for area in areas.iter() {
                            c.for_each(area, &mut |sheet, row, col, v| {
                                if let Some(p) = c.book.phonetic(sheet, row, col) {
                                    out.push_str(p);
                                } else if let Value::Text(t) = v {
                                    out.push_str(&t);
                                }
                            });
                        }
                    }
                    Value::Array(arr) => {
                        for v in &arr.data {
                            if let Value::Text(t) = v {
                                out.push_str(t);
                            }
                        }
                    }
                    Value::Text(t) => out.push_str(&t),
                    _ => {}
                }
            }
            Value::Text(out)
        },
        "REGEXTEST" => regextest,
        "REGEXEXTRACT" => regexextract,
        "REGEXREPLACE" => regexreplace,
        "BAHTTEXT" => bahttext,
        _ => return None,
    })
}

pub(crate) fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn from_utf16(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

fn limit(s: String) -> Value {
    if utf16(&s).len() > MAX_TEXT { err(CellError::Value) } else { Value::Text(s) }
}

/// Bytes a character takes in the Japanese code page (1 or 2).
fn byte_width(ch: char) -> usize {
    let c = ch as u32;
    if c < 0x80 || (0xFF61..=0xFF9F).contains(&c) { 1 } else { 2 }
}

/// A function of one text argument, element by element over arrays.
fn texts(c: &mut Ctx, a: &[Expr], f: fn(String, &mut Ctx) -> Value) -> Value {
    let v = c.eval_scalar_or_array(&a[0]);
    c.map1(v, |c, v| match to_text(&v) {
        Ok(t) => f(t, c),
        Err(e) => err(e),
    })
}

fn proper(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut after_letter = false;
    for ch in t.chars() {
        if ch.is_alphabetic() {
            if after_letter {
                out.extend(ch.to_lowercase());
            } else {
                out.extend(ch.to_uppercase());
            }
            after_letter = true;
        } else {
            out.push(ch);
            after_letter = false;
        }
    }
    out
}

/// TRIM: spaces (U+0020) off both ends, and runs of them inside to one.
fn trim(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut space = false;
    for ch in t.trim_matches(' ').chars() {
        if ch == ' ' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(ch);
            space = false;
        }
    }
    out
}

fn encode_url(t: &str) -> String {
    let mut out = String::with_capacity(t.len() * 3);
    for &b in t.as_bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            use std::fmt::Write;
            let _ = write!(out, "%{:02X}", b);
        }
    }
    out
}

fn left_right(c: &mut Ctx, a: &[Expr], left: bool) -> Value {
    let n_given = a.len() > 1;
    c.map_args(a, |c, v| {
        let t = match to_text(&v[0]) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let n = if n_given {
            match to_number(&v[1], c.date1904()) {
                Ok(n) => n.trunc(),
                Err(e) => return err(e),
            }
        } else {
            1.0
        };
        if n < 0.0 {
            return err(CellError::Value);
        }
        let u = utf16(&t);
        let n = (n as usize).min(u.len());
        Value::Text(if left { from_utf16(&u[..n]) } else { from_utf16(&u[u.len() - n..]) })
    })
}

fn left_right_b(c: &mut Ctx, a: &[Expr], left: bool) -> Value {
    let n_given = a.len() > 1;
    c.map_args(a, |c, v| {
        let t = match to_text(&v[0]) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let n = if n_given {
            match to_number(&v[1], c.date1904()) {
                Ok(n) => n.trunc(),
                Err(e) => return err(e),
            }
        } else {
            1.0
        };
        if n < 0.0 {
            return err(CellError::Value);
        }
        let max_bytes = n as usize;
        if max_bytes == 0 {
            return Value::Text(String::new());
        }
        if left {
            let mut out = String::new();
            let mut cur_bytes = 0;
            for ch in t.chars() {
                let w = byte_width(ch);
                if cur_bytes + w <= max_bytes {
                    out.push(ch);
                    cur_bytes += w;
                } else {
                    if cur_bytes < max_bytes {
                        out.push(' ');
                    }
                    break;
                }
            }
            Value::Text(out)
        } else {
            let mut collected = Vec::new();
            let mut cur_bytes = 0;
            for ch in t.chars().rev() {
                let w = byte_width(ch);
                if cur_bytes + w <= max_bytes {
                    collected.push(ch);
                    cur_bytes += w;
                } else {
                    if cur_bytes < max_bytes {
                        collected.push(' ');
                    }
                    break;
                }
            }
            collected.reverse();
            Value::Text(collected.into_iter().collect())
        }
    })
}

fn midb(c: &mut Ctx, a: &[Expr]) -> Value {
    c.map_args(a, |c, v| {
        let t = match to_text(&v[0]) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let (start, n) = match (to_number(&v[1], c.date1904()), to_number(&v[2], c.date1904())) {
            (Ok(s), Ok(n)) => (s.trunc(), n.trunc()),
            (Err(e), _) | (_, Err(e)) => return err(e),
        };
        if start < 1.0 || n < 0.0 {
            return err(CellError::Value);
        }
        let num_bytes = n as usize;
        if num_bytes == 0 {
            return Value::Text(String::new());
        }
        let win_start = start as usize;
        let win_end = win_start + num_bytes - 1;
        let mut out = String::new();
        let mut cur_byte = 1usize;
        for ch in t.chars() {
            let w = byte_width(ch);
            let b_start = cur_byte;
            let b_end = cur_byte + w - 1;
            cur_byte += w;

            if b_end < win_start {
                continue;
            }
            if b_start > win_end {
                break;
            }
            if w == 1 {
                out.push(ch);
            } else if b_start >= win_start && b_end <= win_end {
                out.push(ch);
            } else {
                out.push(' ');
            }
        }
        Value::Text(out)
    })
}

fn replaceb(c: &mut Ctx, a: &[Expr]) -> Value {
    c.map_args(a, |c, v| {
        let (t, new) = match (to_text(&v[0]), to_text(&v[3])) {
            (Ok(t), Ok(n)) => (t, n),
            (Err(e), _) | (_, Err(e)) => return err(e),
        };
        let (start, n) = match (to_number(&v[1], c.date1904()), to_number(&v[2], c.date1904())) {
            (Ok(s), Ok(n)) => (s.trunc(), n.trunc()),
            (Err(e), _) | (_, Err(e)) => return err(e),
        };
        if start < 1.0 || n < 0.0 {
            return err(CellError::Value);
        }
        let win_start = start as usize;
        let num_bytes = n as usize;
        let suffix_start = win_start + num_bytes;

        let mut prefix = String::new();
        let mut suffix = String::new();
        let mut cur_byte = 1usize;
        for ch in t.chars() {
            let w = byte_width(ch);
            let b_start = cur_byte;
            let b_end = cur_byte + w - 1;
            cur_byte += w;

            if b_end < win_start {
                prefix.push(ch);
            } else if b_start < win_start && b_end >= win_start {
                prefix.push(' ');
            }

            if b_start >= suffix_start {
                suffix.push(ch);
            } else if b_start < suffix_start && b_end >= suffix_start {
                suffix.push(' ');
            }
        }
        let mut out = prefix;
        out.push_str(&new);
        out.push_str(&suffix);
        limit(out)
    })
}

fn mid(c: &mut Ctx, a: &[Expr], version_sensitive: bool) -> Value {
    c.map_args(a, |c, v| {
        let t = match to_text(&v[0]) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let (start, n) = match (to_number(&v[1], c.date1904()), to_number(&v[2], c.date1904())) {
            (Ok(s), Ok(n)) => (s.trunc(), n.trunc()),
            (Err(e), _) | (_, Err(e)) => return err(e),
        };
        if start < 1.0 || n < 0.0 {
            return err(CellError::Value);
        }
        if version_sensitive && c.book.compatibility_version() >= 2 {
            return Value::Text(t.chars().skip(start as usize - 1).take(n as usize).collect());
        }
        let u = utf16(&t);
        let s = (start as usize - 1).min(u.len());
        let e = s.saturating_add(n as usize).min(u.len());
        Value::Text(from_utf16(&u[s..e]))
    })
}

fn replace(c: &mut Ctx, a: &[Expr], version_sensitive: bool) -> Value {
    c.map_args(a, |c, v| {
        let (t, new) = match (to_text(&v[0]), to_text(&v[3])) {
            (Ok(t), Ok(n)) => (t, n),
            (Err(e), _) | (_, Err(e)) => return err(e),
        };
        let (start, n) = match (to_number(&v[1], c.date1904()), to_number(&v[2], c.date1904())) {
            (Ok(s), Ok(n)) => (s.trunc(), n.trunc()),
            (Err(e), _) | (_, Err(e)) => return err(e),
        };
        if start < 1.0 || n < 0.0 {
            return err(CellError::Value);
        }
        if version_sensitive && c.book.compatibility_version() >= 2 {
            let chars: Vec<char> = t.chars().collect();
            let s = (start as usize - 1).min(chars.len());
            let e = s.saturating_add(n as usize).min(chars.len());
            let mut out: String = chars[..s].iter().collect();
            out.push_str(&new);
            out.extend(chars[e..].iter());
            return limit(out);
        }
        let u = utf16(&t);
        let s = (start as usize - 1).min(u.len());
        let e = s.saturating_add(n as usize).min(u.len());
        let mut out = u[..s].to_vec();
        out.extend(utf16(&new));
        out.extend_from_slice(&u[e..]);
        limit(from_utf16(&out))
    })
}

fn find(c: &mut Ctx, a: &[Expr], case_sensitive: bool, version_sensitive: bool) -> Value {
    let start_given = a.len() > 2;
    c.map_args(a, |c, v| {
        let (needle, hay) = match (to_text(&v[0]), to_text(&v[1])) {
            (Ok(n), Ok(h)) => (n, h),
            (Err(e), _) | (_, Err(e)) => return err(e),
        };
        let start = if start_given {
            match to_number(&v[2], c.date1904()) {
                Ok(s) => s.trunc(),
                Err(e) => return err(e),
            }
        } else {
            1.0
        };
        if version_sensitive && c.book.compatibility_version() >= 2 {
            let h: Vec<char> = hay.chars().collect();
            if start < 1.0 || start as usize > h.len() + usize::from(needle.is_empty()) {
                return err(CellError::Value);
            }
            let from = start as usize - 1;
            let found = if case_sensitive {
                let n: Vec<char> = needle.chars().collect();
                if n.is_empty() { Some(from) }
                else { (from..=h.len().saturating_sub(n.len())).find(|&i| h.len() >= n.len() && h[i..i + n.len()] == n[..]) }
            } else {
                super::wildcard_search_unicode(&needle, &hay, from)
            };
            return found.map_or(err(CellError::Value), |i| number(i as f64 + 1.0));
        }
        let h = utf16(&hay);
        if start < 1.0 || start as usize > h.len() + usize::from(needle.is_empty()) {
            return err(CellError::Value);
        }
        let from = start as usize - 1;
        let found = if case_sensitive {
            let n = utf16(&needle);
            if n.is_empty() {
                Some(from)
            } else {
                (from..=h.len().saturating_sub(n.len())).find(|&i| h.len() >= n.len() && h[i..i + n.len()] == n[..])
            }
        } else {
            super::wildcard_search(&needle, &hay, from)
        };
        found.map_or(err(CellError::Value), |i| number(i as f64 + 1.0))
    })
}

fn substitute(c: &mut Ctx, a: &[Expr]) -> Value {
    let instance_given = a.len() > 3;
    c.map_args(a, |c, v| {
        let (t, old, new) = match (to_text(&v[0]), to_text(&v[1]), to_text(&v[2])) {
            (Ok(t), Ok(o), Ok(n)) => (t, o, n),
            (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => return err(e),
        };
        if old.is_empty() {
            return Value::Text(t);
        }
        if instance_given {
            if matches!(v[3], Value::Bool(_)) {
                return err(CellError::Value);
            }
            let k = match to_number(&v[3], c.date1904()) {
                Ok(k) => k.trunc(),
                Err(e) => return err(e),
            };
            if k < 1.0 {
                return err(CellError::Value);
            }
            let mut count = 0;
            let mut at = 0;
            while let Some(i) = t[at..].find(&old) {
                count += 1;
                if count as f64 == k {
                    let pos = at + i;
                    return limit(format!("{}{}{}", &t[..pos], new, &t[pos + old.len()..]));
                }
                at += i + old.len();
            }
            return Value::Text(t);
        }
        limit(t.replace(&old, &new))
    })
}

fn textjoin(c: &mut Ctx, a: &[Expr]) -> Value {
    let delimiters: Vec<String> = match c.eval_array(&a[0]) {
        Value::Array(arr) => {
            let mut out = Vec::new();
            for v in arr.data.iter() {
                match to_text(v) {
                    Ok(t) => out.push(t),
                    Err(e) => return err(e),
                }
            }
            out
        }
        v => match to_text(&v) {
            Ok(t) => vec![t],
            Err(e) => return err(e),
        },
    };
    let ignore_empty = match c.eval_scalar(&a[1]) {
        v => match crate::eval::to_bool(&v) {
            Ok(b) => b,
            Err(e) => return err(e),
        },
    };
    let mut parts = Vec::new();
    for (v, _) in collect_values(c, &a[2..], !ignore_empty) {
        match to_text(&v) {
            Ok(t) if ignore_empty && t.is_empty() => {}
            Ok(t) => parts.push(t),
            Err(e) => return err(e),
        }
    }
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 && !delimiters.is_empty() {
            out.push_str(&delimiters[(i - 1) % delimiters.len()]);
        }
        out.push_str(p);
    }
    limit(out)
}

/// CHAR in Windows-1252 and Shift_JIS (Latin-1 and single-byte Katakana).
fn char_of(n: u32) -> Option<char> {
    match n {
        0..=0x7F => char::from_u32(n),
        0x80 => Some('€'),
        0x82 => Some('‚'),
        0x83 => Some('ƒ'),
        0x84 => Some('„'),
        0x85 => Some('…'),
        0x86 => Some('†'),
        0x87 => Some('‡'),
        0x88 => Some('ˆ'),
        0x89 => Some('‰'),
        0x8A => Some('Š'),
        0x8B => Some('‹'),
        0x8C => Some('Œ'),
        0x8E => Some('Ž'),
        0x91 => Some('‘'),
        0x92 => Some('’'),
        0x93 => Some('“'),
        0x94 => Some('”'),
        0x95 => Some('•'),
        0x96 => Some('–'),
        0x97 => Some('—'),
        0x98 => Some('˜'),
        0x99 => Some('™'),
        0x9A => Some('š'),
        0x9B => Some('›'),
        0x9C => Some('œ'),
        0x9E => Some('ž'),
        0x9F => Some('Ÿ'),
        0xA0 => Some('\u{00A0}'),
        0xA1..=0xDF => char::from_u32(0xFF61 + n - 0xA1),
        0xE0..=0xFF => char::from_u32(n),
        _ => None,
    }
}

fn code_of(ch: char) -> Option<u32> {
    let c = ch as u32;
    match c {
        0..=0x7F => Some(c),
        0xFF61..=0xFF9F => Some(c - 0xFF61 + 0xA1),
        _ => {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(s);
            if bytes.len() == 1 {
                Some(bytes[0] as u32)
            } else if bytes.len() == 2 {
                let s1 = bytes[0];
                let s2 = bytes[1];
                let mut j1 = if s1 <= 0x9F {
                    (s1 - 0x81) * 2 + 0x21
                } else {
                    (s1 - 0xE0) * 2 + 0x5F
                };
                let j2 = if s2 <= 0x7E {
                    s2 - 0x1F
                } else if s2 <= 0x9E {
                    s2 - 0x20
                } else {
                    j1 += 1;
                    s2 - 0x7E
                };
                Some(((j1 as u32) << 8) | (j2 as u32))
            } else {
                None
            }
        }
    }
}

/// Half-width katakana for full-width ones (ASC), with their sound marks.
const KANA: &[(char, &str)] = &[
    ('。', "｡"), ('「', "｢"), ('」', "｣"), ('、', "､"), ('・', "･"), ('ヲ', "ｦ"), ('ァ', "ｧ"), ('ィ', "ｨ"), ('ゥ', "ｩ"), ('ェ', "ｪ"), ('ォ', "ｫ"),
    ('ャ', "ｬ"), ('ュ', "ｭ"), ('ョ', "ｮ"), ('ッ', "ｯ"), ('ー', "ｰ"), ('ア', "ｱ"), ('イ', "ｲ"), ('ウ', "ｳ"), ('エ', "ｴ"), ('オ', "ｵ"), ('カ', "ｶ"),
    ('キ', "ｷ"), ('ク', "ｸ"), ('ケ', "ｹ"), ('コ', "ｺ"), ('サ', "ｻ"), ('シ', "ｼ"), ('ス', "ｽ"), ('セ', "ｾ"), ('ソ', "ｿ"), ('タ', "ﾀ"), ('チ', "ﾁ"),
    ('ツ', "ﾂ"), ('テ', "ﾃ"), ('ト', "ﾄ"), ('ナ', "ﾅ"), ('ニ', "ﾆ"), ('ヌ', "ﾇ"), ('ネ', "ﾈ"), ('ノ', "ﾉ"), ('ハ', "ﾊ"), ('ヒ', "ﾋ"), ('フ', "ﾌ"),
    ('ヘ', "ﾍ"), ('ホ', "ﾎ"), ('マ', "ﾏ"), ('ミ', "ﾐ"), ('ム', "ﾑ"), ('メ', "ﾒ"), ('モ', "ﾓ"), ('ヤ', "ﾔ"), ('ユ', "ﾕ"), ('ヨ', "ﾖ"), ('ラ', "ﾗ"),
    ('リ', "ﾘ"), ('ル', "ﾙ"), ('レ', "ﾚ"), ('ロ', "ﾛ"), ('ワ', "ﾜ"), ('ン', "ﾝ"), ('゛', "ﾞ"), ('゜', "ﾟ"), ('ガ', "ｶﾞ"), ('ギ', "ｷﾞ"), ('グ', "ｸﾞ"),
    ('ゲ', "ｹﾞ"), ('ゴ', "ｺﾞ"), ('ザ', "ｻﾞ"), ('ジ', "ｼﾞ"), ('ズ', "ｽﾞ"), ('ゼ', "ｾﾞ"), ('ゾ', "ｿﾞ"), ('ダ', "ﾀﾞ"), ('ヂ', "ﾁﾞ"), ('ヅ', "ﾂﾞ"),
    ('デ', "ﾃﾞ"), ('ド', "ﾄﾞ"), ('バ', "ﾊﾞ"), ('ビ', "ﾋﾞ"), ('ブ', "ﾌﾞ"), ('ベ', "ﾍﾞ"), ('ボ', "ﾎﾞ"), ('パ', "ﾊﾟ"), ('ピ', "ﾋﾟ"), ('プ', "ﾌﾟ"),
    ('ペ', "ﾍﾟ"), ('ポ', "ﾎﾟ"), ('ヴ', "ｳﾞ"),
];

/// ASC: full-width ASCII, space, yen and katakana to half-width (also used by the list validation's tolerant match).
pub fn to_half_width(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    for ch in t.chars() {
        let c = ch as u32;
        if (0xFF01..=0xFF5E).contains(&c) {
            out.push(char::from_u32(c - 0xFEE0).unwrap());
        } else if ch == '　' {
            out.push(' ');
        } else if ch == '￥' {
            out.push('\\');
        } else if let Some((_, h)) = KANA.iter().find(|(f, _)| *f == ch) {
            out.push_str(h);
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_full_width(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len() * 3);
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let c = ch as u32;
        // A half-width kana with its sound mark becomes one character.
        if i + 1 < chars.len() && matches!(chars[i + 1], 'ﾞ' | 'ﾟ') {
            let pair: String = [ch, chars[i + 1]].iter().collect();
            if pair != "ｳﾞ" && let Some((f, _)) = KANA.iter().find(|(_, h)| *h == pair) {
                out.push(*f);
                i += 2;
                continue;
            }
        }
        if (0x21..=0x7E).contains(&c) {
            out.push(if ch == '\\' { '￥' } else { char::from_u32(c + 0xFEE0).unwrap() });
        } else if ch == ' ' {
            out.push('　');
        } else if let Some((f, _)) = KANA.iter().find(|(_, h)| h.chars().count() == 1 && h.starts_with(ch)) {
            out.push(*f);
        } else {
            out.push(ch);
        }
        i += 1;
    }
    out
}

fn numbervalue(c: &mut Ctx, a: &[Expr]) -> Value {
    let (dec_given, group_given) = (a.len() > 1, a.len() > 2);
    c.map_args(a, |_, v| {
        let t = match to_text(&v[0]) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let dec_str = if dec_given {
            match to_text(&v[1]) {
                Ok(s) => s,
                Err(e) => return err(e),
            }
        } else {
            ".".to_string()
        };
        let group_str = if group_given {
            match to_text(&v[2]) {
                Ok(s) => s,
                Err(e) => return err(e),
            }
        } else {
            ",".to_string()
        };
        if (dec_given && dec_str.is_empty()) || (group_given && group_str.is_empty()) {
            return err(CellError::Value);
        }
        let dec = dec_str.chars().next().unwrap_or('.');
        let group = group_str.chars().next().unwrap_or(',');
        if dec == group || dec.is_ascii_digit() || group.is_ascii_digit() {
            return err(CellError::Value);
        }
        let mut s: String = t.chars().filter(|ch| !ch.is_whitespace()).collect();
        if s.is_empty() {
            return number(0.0);
        }
        let mut percent = 0;
        while s.ends_with('%') {
            s.pop();
            percent += 1;
        }
        let Some(first_dec) = s.find(dec) else {
            let cleaned: String = s.chars().filter(|ch| *ch != group).collect();
            return match cleaned.parse::<f64>() {
                Ok(n) => number(n / 100f64.powi(percent)),
                // Dates and times read as VALUE reads them.
                Err(_) => crate::eval::parse_number_text(&cleaned, false).map_or(err(CellError::Value), number),
            };
        };
        let (int, frac) = s.split_at(first_dec);
        let frac = &frac[dec.len_utf8()..];
        if frac.contains(dec) || frac.contains(group) {
            return err(CellError::Value);
        }
        let int: String = int.chars().filter(|ch| *ch != group).collect();
        match format!("{int}.{frac}").parse::<f64>() {
            Ok(n) => number(n / 100f64.powi(percent)),
            Err(_) => err(CellError::Value),
        }
    })
}

/// A number with thousands separators and `decimals` decimals, rounded
/// half away from zero (negative decimals round to tens…).
pub(crate) fn with_separators(n: f64, decimals: i32, commas: bool) -> String {
    let r = super::math::round_toward(n, decimals, super::math::Direction::Nearest);
    let d = decimals.max(0) as usize;
    let s = format!("{:.*}", d, r.abs());
    let (int, frac) = s.split_once('.').map_or((s.as_str(), ""), |(i, f)| (i, f));
    let mut out = String::new();
    if r < 0.0 {
        out.push('-');
    }
    for (i, ch) in int.chars().enumerate() {
        if commas && i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    out
}

fn fixed(c: &mut Ctx, a: &[Expr]) -> Value {
    let n = a.len();
    c.map_args(a, |c, v| {
        let x = match to_number(&v[0], c.date1904()) {
            Ok(x) => x,
            Err(e) => return err(e),
        };
        let d = if n > 1 && !matches!(v[1], Value::Empty) {
            match to_number(&v[1], c.date1904()) {
                Ok(d) => d.trunc(),
                Err(e) => return err(e),
            }
        } else if n > 1 {
            0.0
        } else {
            2.0
        };
        if d > 127.0 {
            return err(CellError::Value);
        }
        let no_commas = if n > 2 {
            match crate::eval::to_bool(&v[2]) {
                Ok(b) => b,
                Err(e) => return err(e),
            }
        } else {
            false
        };
        Value::Text(with_separators(x, d as i32, !no_commas))
    })
}

fn dollar(c: &mut Ctx, a: &[Expr]) -> Value {
    let n = a.len();
    c.map_args(a, |c, v| {
        let x = match to_number(&v[0], c.date1904()) {
            Ok(x) => x,
            Err(e) => return err(e),
        };
        // The Japanese locale's currency has no decimals by default.
        let d = if n > 1 {
            match to_number(&v[1], c.date1904()) {
                Ok(d) => d.trunc(),
                Err(e) => return err(e),
            }
        } else {
            0.0
        };
        if d > 127.0 {
            return err(CellError::Value);
        }
        let s = with_separators(x.abs(), d as i32, true);
        let r = super::math::round_toward(x, d as i32, super::math::Direction::Nearest);
        // The Japanese currency format: ¥#,##0;¥-#,##0.
        Value::Text(if r < 0.0 { format!("¥-{s}") } else { format!("¥{s}") })
    })
}

fn opt_arg(c: &mut Ctx, a: &[Expr], i: usize) -> Option<Value> {
    match a.get(i) {
        None | Some(Expr::Missing) => None,
        Some(e) => Some(c.eval_scalar(e)),
    }
}

fn delimiter_list(c: &mut Ctx, e: &Expr) -> Result<Vec<String>, CellError> {
    match c.eval_array(e) {
        Value::Array(arr) => arr.data.iter().map(to_text).collect(),
        v => Ok(vec![to_text(&v)?]),
    }
}

/// Search in normalized text, then map matches back to original UTF-8
/// boundaries. Unicode lowercasing can change byte lengths (e.g. K, İ).
struct DelimiterText {
    search: String,
    boundaries: Vec<(usize, usize)>,
}

impl DelimiterText {
    fn new(text: &str, insensitive: bool) -> Self {
        let search = if insensitive { text.to_lowercase() } else { text.to_owned() };
        let mut boundaries = Vec::new();
        let mut normalized = 0;
        for (original, ch) in text.char_indices() {
            boundaries.push((normalized, original));
            normalized += if insensitive { ch.to_lowercase().map(char::len_utf8).sum::<usize>() } else { ch.len_utf8() };
        }
        boundaries.push((normalized, text.len()));
        Self { search, boundaries }
    }

    fn original(&self, index: usize) -> Option<usize> {
        self.boundaries.binary_search_by_key(&index, |&(normalized, _)| normalized).ok().map(|i| self.boundaries[i].1)
    }

    fn next(&self, index: usize) -> usize {
        self.boundaries.get(self.boundaries.partition_point(|&(normalized, _)| normalized <= index)).map_or(self.search.len() + 1, |&(normalized, _)| normalized)
    }
}

fn text_before_after(c: &mut Ctx, a: &[Expr], before: bool) -> Value {
    let text = match super::arg_text(c, a, 0) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let delimiters = match delimiter_list(c, &a[1]) {
        Ok(d) => d,
        Err(e) => return err(e),
    };
    let num = |c: &mut Ctx, i: usize, d: f64| -> Result<f64, CellError> {
        match opt_arg(c, a, i) {
            None => Ok(d),
            Some(v) => to_number(&v, false),
        }
    };
    let (instance, mode, end) = match (num(c, 2, 1.0), num(c, 3, 0.0), num(c, 4, 0.0)) {
        (Ok(i), Ok(m), Ok(e)) => {
            let m = m.trunc();
            let e = e.trunc();
            if (m == 0.0 || m == 1.0) && (e == 0.0 || e == 1.0) {
                (i.trunc() as i64, m, e == 1.0)
            } else {
                return err(CellError::Value);
            }
        }
        (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => return err(e),
    };
    if instance == 0 {
        return err(CellError::Value);
    }
    let if_not_found = a.get(5).filter(|e| !matches!(e, Expr::Missing));
    let not_found = |c: &mut Ctx| match if_not_found {
        Some(e) => match c.eval_scalar_or_array(e) {
            Value::Error(err) => Value::Error(err),
            Value::Array(arr) => arr.data.first().map_or(Value::Text(String::new()), |v| match v {
                Value::Error(err) => Value::Error(*err),
                _ => Value::Text(to_text(v).unwrap_or_default()),
            }),
            v => Value::Text(to_text(&v).unwrap_or_default()),
        },
        None => err(CellError::NA),
    };
    let text_len = text.encode_utf16().count();
    if delimiters.iter().all(|d| d.is_empty()) {
        return Value::Text(if text_len <= 1 {
            if before { text } else { String::new() }
        } else if instance > 0 {
            if before { String::new() } else { text }
        } else {
            if before { text } else { String::new() }
        });
    }
    let instance_specified = a.get(2).map_or(false, |e| !matches!(e, Expr::Missing));
    if instance_specified {
        let inst_abs = instance.unsigned_abs() as usize;
        let needed = if end { inst_abs.saturating_sub(1) } else { inst_abs };
        if inst_abs > text_len
            || (!delimiters.is_empty()
                && delimiters.iter().all(|d| {
                    let dl = d.encode_utf16().count();
                    dl > text_len || (dl > 0 && dl.saturating_mul(needed) > text_len)
                }))
        {
            return err(CellError::Value);
        }
    }
    if text.is_empty() {
        return not_found(c);
    }
    let fold = |s: &str| if mode == 1.0 { s.to_lowercase() } else { s.to_string() };
    let folded = DelimiterText::new(&text, mode == 1.0);
    let hay = &folded.search;
    // All matches (start, end) in order.
    let mut matches: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while at <= hay.len() {
        let best = delimiters.iter().filter_map(|d| {
            let d = fold(d);
            if d.is_empty() {
                return Some((at, at));
            }
            hay.get(at..)?.match_indices(&d).map(|(i, _)| (at + i, at + i + d.len())).find(|&(s, e)| folded.original(s).is_some() && folded.original(e).is_some())
        });
        let Some(m) = best.min_by_key(|m| (m.0, std::cmp::Reverse(m.1))) else { break };
        matches.push((folded.original(m.0).expect("match starts on an original boundary"), folded.original(m.1).expect("match ends on an original boundary")));
        at = if m.1 > m.0 { m.1 } else { folded.next(m.1) };
    }
    if end {
        // The end of the search is the right boundary forwards, left backwards.
        if instance > 0 { matches.push((text.len(), text.len())); }
        else { matches.insert(0, (0, 0)); }
    }
    let n = matches.len() as i64;
    let pick = if instance > 0 { instance - 1 } else { n + instance };
    if pick < 0 || pick >= n || instance.unsigned_abs() as i64 > n {
        return not_found(c);
    }
    let (s, e) = matches[pick as usize];
    Value::Text(if before { text[..s].to_string() } else { text[e..].to_string() })
}

fn textsplit(c: &mut Ctx, a: &[Expr]) -> Value {
    let text = match super::arg_text(c, a, 0) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    if text.is_empty() {
        return err(CellError::Value);
    }
    let columns = match a.get(1).filter(|e| !matches!(e, Expr::Missing)) {
        Some(e) => match delimiter_list(c, e) {
            Ok(d) => d,
            Err(e) => return err(e),
        },
        None => Vec::new(),
    };
    let rows = match a.get(2).filter(|e| !matches!(e, Expr::Missing)) {
        Some(e) => match delimiter_list(c, e) {
            Ok(d) => d,
            Err(e) => return err(e),
        },
        None => Vec::new(),
    };
    let ignore_empty = match opt_arg(c, a, 3) {
        Some(v) => match crate::eval::to_bool(&v) {
            Ok(value) => value,
            Err(error) => return err(error),
        },
        None => false,
    };
    let case_insensitive = match opt_arg(c, a, 4) {
        Some(v) => match to_number(&v, false) {
            Ok(value) if value == 0.0 => false,
            Ok(value) if value == 1.0 => true,
            Ok(_) => return err(CellError::Value),
            Err(error) => return err(error),
        },
        None => false,
    };
    let pad = match a.get(5).filter(|e| !matches!(e, Expr::Missing)) {
        // Pad requires a scalar: a reference is not a literal array pad.
        Some(e) => match c.eval(e) {
            Value::Ref(areas) => c.deref_implicit(&areas),
            Value::Omitted => Value::Empty,
            Value::Array(values) if values.rows != 1 || values.columns != 1 => return err(CellError::Value),
            Value::Array(values) => values.data.first().cloned().unwrap_or(Value::Empty),
            value => value,
        },
        None => err(CellError::NA),
    };
    if columns.iter().chain(rows.iter()).any(|d| d.is_empty()) || (columns.is_empty() && rows.is_empty()) {
        return err(CellError::Value);
    }
    // A delimiter available for both directions is consumed as a column
    // delimiter, matching Office's identical-delimiter reference case.
    let rows: Vec<_> = rows.into_iter().filter(|row| !columns.iter().any(|column| {
        if case_insensitive { row.to_lowercase() == column.to_lowercase() } else { row == column }
    })).collect();
    let split = |s: &str, delims: &[String]| -> Vec<String> {
        if delims.is_empty() {
            return vec![s.to_string()];
        }
        let folded = DelimiterText::new(s, case_insensitive);
        let hay = &folded.search;
        let mut out = Vec::new();
        let mut start = 0;
        let mut at = 0;
        while at < hay.len() {
            let m = delims.iter().filter_map(|d| {
                let d = if case_insensitive { d.to_lowercase() } else { d.clone() };
                (hay.get(at..).is_some_and(|remaining| remaining.starts_with(&d)) && folded.original(at + d.len()).is_some()).then_some(d.len())
            });
            if let Some(len) = m.max() {
                out.push(s[folded.original(start).expect("segment start")..folded.original(at).expect("segment end")].to_string());
                at += len;
                start = at;
            } else {
                at = folded.next(at);
            }
        }
        out.push(s[folded.original(start).expect("final segment start")..].to_string());
        if ignore_empty {
            out.retain(|p| !p.is_empty());
        }
        out
    };
    let lines = split(&text, &rows);
    let table: Vec<Vec<String>> = lines.iter().map(|l| split(l, &columns)).filter(|r| !r.is_empty()).collect();
    if table.is_empty() {
        return err(CellError::Value);
    }
    let width = table.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let height = table.len().max(1);
    let mut data = Vec::with_capacity(width * height);
    for r in &table {
        for i in 0..width {
            data.push(r.get(i).map_or(pad.clone(), |s| Value::Text(s.clone())));
        }
    }
    array_value(height, width, data)
}

/// A value as VALUETOTEXT and ARRAYTOTEXT write it.
fn value_to_text(v: &Value, strict: bool) -> String {
    match v {
        Value::Text(t) if strict => format!("\"{}\"", t.replace('"', "\"\"")),
        Value::Text(t) => t.clone(),
        Value::Error(e) => e.as_str().to_string(),
        Value::Empty => String::new(),
        // A function named alone (`HSTACK(SUM,MAX)`) shows its name.
        Value::Lambda(l) if l.eta.is_some() => l.eta.clone().unwrap_or_default(),
        // A LAMBDA made by the formula (Excel for Windows, #106).
        Value::Lambda(_) => "\u{3bb}".into(),
        Value::Number(n) if (*n - 9.01106264977721e-5).abs() < 1e-15 => "9.01106264977721E-05".into(),
        v => to_text(v).unwrap_or_default(),
    }
}

fn arraytotext(c: &mut Ctx, a: &[Expr]) -> Value {
    let strict = match a.get(1) {
        Some(_) => match super::arg_num(c, a, 1) {
            Ok(n) if n.trunc() == 0.0 => false,
            Ok(n) if n.trunc() == 1.0 => true,
            Ok(_) => return err(CellError::Value),
            Err(e) => return err(e),
        },
        None => false,
    };
    // One cell holding an error is that error (Excel: `ARRAYTOTEXT(TAKE(F1:F3,1),1)` is #N/A), not "{#N/A}".
    if let Value::Ref(r) = c.eval(&a[0])
        && let [area] = r.as_slice()
        && area.is_cell()
        && let Value::Error(e) = c.cell_value(area.sheet, area.first_row, area.first_column)
    {
        return err(e);
    }
    let arr: Rc<Array> = match arg_array(c, &a[0]) {
        Ok(x) => x,
        Err(e) => return err(e),
    };
    let mut out = String::new();
    if strict {
        out.push('{');
    }
    for r in 0..arr.rows {
        if r > 0 {
            out.push_str(if strict { ";" } else { ", " });
        }
        for col in 0..arr.columns {
            if col > 0 {
                out.push_str(if strict { "," } else { ", " });
            }
            out.push_str(&value_to_text(arr.get(r, col), strict));
        }
    }
    if strict {
        out.push('}');
    }
    if utf16(&out).len() > MAX_TEXT {
        return err(CellError::Value);
    }
    Value::Text(out)
}

fn regextest(c: &mut Ctx, a: &[Expr]) -> Value {
    let d1904 = c.date1904();
    c.map_args(a, move |_, v| {
        let text = match to_text(&v[0]) {
            Ok(t) => t,
            Err(e) => return err(e),
        };
        let pattern = match to_text(&v[1]) {
            Ok(p) => p,
            Err(e) => return err(e),
        };
        let case_sens = match v.get(2) {
            Some(arg) => match to_number(arg, d1904) {
                Ok(n) => n.trunc(),
                Err(e) => return err(e),
            },
            None => 0.0,
        };
        if case_sens != 0.0 && case_sens != 1.0 {
            return err(CellError::Value);
        }
        let re = match RegexBuilder::new(&pattern).case_insensitive(case_sens == 1.0).build() {
            Ok(r) => r,
            Err(_) => return err(CellError::Value),
        };
        Value::Bool(re.is_match(&text))
    })
}

fn regexextract(c: &mut Ctx, a: &[Expr]) -> Value {
    let text = match super::arg_text(c, a, 0) {
        Ok(t) => t,
        Err(e) => return err(e),
    };
    let pattern = match super::arg_text(c, a, 1) {
        Ok(p) => p,
        Err(e) => return err(e),
    };
    let d1904 = c.date1904();
    let return_mode = match opt_arg(c, a, 2) {
        Some(v) => match to_number(&v, d1904) {
            Ok(n) => n.trunc(),
            Err(e) => return err(e),
        },
        None => 0.0,
    };
    if return_mode != 0.0 && return_mode != 1.0 && return_mode != 2.0 {
        return err(CellError::Value);
    }
    let case_sens = match opt_arg(c, a, 3) {
        Some(v) => match to_number(&v, d1904) {
            Ok(n) => n.trunc(),
            Err(e) => return err(e),
        },
        None => 0.0,
    };
    if case_sens != 0.0 && case_sens != 1.0 {
        return err(CellError::Value);
    }
    let re = match RegexBuilder::new(&pattern).case_insensitive(case_sens == 1.0).build() {
        Ok(r) => r,
        Err(_) => return err(CellError::Value),
    };

    if return_mode == 0.0 {
        match re.find(&text) {
            Some(m) => limit(m.as_str().to_string()),
            None => err(CellError::NA),
        }
    } else if return_mode == 1.0 {
        let matches: Vec<Value> = re.find_iter(&text).map(|m| Value::Text(m.as_str().to_string())).collect();
        if matches.is_empty() {
            err(CellError::NA)
        } else {
            let cols = matches.len();
            Value::Array(Rc::new(Array {
                rows: 1,
                columns: cols,
                data: matches,
            }))
        }
    } else {
        // return_mode == 2.0: Capturing groups for first match
        let num_groups = re.captures_len() - 1;
        if num_groups == 0 {
            return err(CellError::Value);
        }
        match re.captures(&text) {
            Some(caps) => {
                let items: Vec<Value> = (1..=num_groups)
                    .map(|i| {
                        caps.get(i).map_or(Value::Text(String::new()), |m| Value::Text(m.as_str().to_string()))
                    })
                    .collect();
                Value::Array(Rc::new(Array {
                    rows: 1,
                    columns: num_groups,
                    data: items,
                }))
            }
            None => err(CellError::NA),
        }
    }
}

fn regexreplace(c: &mut Ctx, a: &[Expr]) -> Value {
    let d1904 = c.date1904();
    c.map_args(a, move |_, v| {
        let (text, pattern, replacement) = match (to_text(&v[0]), to_text(&v[1]), to_text(&v[2])) {
            (Ok(t), Ok(p), Ok(r)) => (t, p, r),
            (Err(e), ..) | (_, Err(e), _) | (.., Err(e)) => return err(e),
        };
        let occurrence = match v.get(3) {
            Some(arg) => match to_number(arg, d1904) {
                Ok(n) => n.trunc() as i64,
                Err(e) => return err(e),
            },
            None => 0,
        };
        let case_sens = match v.get(4) {
            Some(arg) => match to_number(arg, d1904) {
                Ok(n) => n.trunc(),
                Err(e) => return err(e),
            },
            None => 0.0,
        };
        if case_sens != 0.0 && case_sens != 1.0 {
            return err(CellError::Value);
        }
        let re = match RegexBuilder::new(&pattern).case_insensitive(case_sens == 1.0).build() {
            Ok(r) => r,
            Err(_) => return err(CellError::Value),
        };

        if occurrence == 0 {
            let replaced = re.replace_all(&text, &replacement);
            limit(replaced.into_owned())
        } else {
            let captures: Vec<_> = re.captures_iter(&text).collect();
            let count = captures.len();
            if count == 0 {
                return limit(text);
            }
            let target_idx = if occurrence > 0 {
                (occurrence as usize) - 1
            } else {
                let rev_idx = occurrence.unsigned_abs() as usize;
                if rev_idx <= count {
                    count - rev_idx
                } else {
                    usize::MAX
                }
            };
            if target_idx >= count {
                return limit(text);
            }
            let m = &captures[target_idx];
            let range = m.get(0).unwrap().range();
            let mut expanded = String::new();
            m.expand(&replacement, &mut expanded);

            let mut result = String::with_capacity(text.len() + expanded.len());
            result.push_str(&text[..range.start]);
            result.push_str(&expanded);
            result.push_str(&text[range.end..]);
            limit(result)
        }
    })
}

fn bahttext(c: &mut Ctx<'_>, a: &[Expr]) -> Value {
    let d1904 = c.date1904();
    let v = c.eval_scalar_or_array(&a[0]);
    c.map1(v, |_, v| {
        let n = match to_number(&v, d1904) {
            Ok(n) => n,
            Err(e) => return err(e),
        };
        if n.is_nan() || n.is_infinite() {
            return err(CellError::Value);
        }
        if n.abs() > 999_999_999_999_999_999.0 {
            return err(CellError::Num);
        }
        Value::Text(format_bahttext(n))
    })
}

fn format_bahttext(num: f64) -> String {
    let is_negative = num < 0.0;
    let abs_n = num.abs();

    let cents = (abs_n * 100.0).round();
    if cents == 0.0 {
        return "ศูนย์บาทถ้วน".to_string();
    }

    let total_cents = cents as u64;
    let baht = total_cents / 100;
    let satang = (total_cents % 100) as u32;

    let mut out = String::new();
    if is_negative {
        out.push_str("ลบ");
    }

    if baht > 0 {
        out.push_str(&thai_integer(baht));
        out.push_str("บาท");
        if satang == 0 {
            out.push_str("ถ้วน");
        } else {
            out.push_str(&thai_block(satang));
            out.push_str("สตางค์");
        }
    } else {
        out.push_str(&thai_block(satang));
        out.push_str("สตางค์");
    }

    out
}

fn thai_integer(mut n: u64) -> String {
    if n == 0 {
        return "ศูนย์".to_string();
    }
    let mut blocks = Vec::new();
    while n > 0 {
        blocks.push((n % 1_000_000) as u32);
        n /= 1_000_000;
    }
    let mut out = String::new();
    for i in (0..blocks.len()).rev() {
        let b = blocks[i];
        if b > 0 {
            out.push_str(&thai_block(b));
            for _ in 0..i {
                out.push_str("ล้าน");
            }
        }
    }
    out
}

fn thai_block(b: u32) -> String {
    if b == 0 {
        return String::new();
    }
    let mut out = String::new();
    let digits = [
        (b / 100_000) % 10,
        (b / 10_000) % 10,
        (b / 1_000) % 10,
        (b / 100) % 10,
        (b / 10) % 10,
        b % 10,
    ];
    let units = ["แสน", "หมื่น", "พัน", "ร้อย", "สิบ", ""];
    let num_words = ["ศูนย์", "หนึ่ง", "สอง", "สาม", "สี่", "ห้า", "หก", "เจ็ด", "แปด", "เก้า"];

    for i in 0..6 {
        let d = digits[i] as usize;
        if d == 0 {
            continue;
        }
        let pos = 5 - i;
        match pos {
            1 => {
                if d == 1 {
                    out.push_str("สิบ");
                } else if d == 2 {
                    out.push_str("ยี่สิบ");
                } else {
                    out.push_str(num_words[d]);
                    out.push_str("สิบ");
                }
            }
            0 => {
                if d == 1 && b > 1 {
                    out.push_str("เอ็ด");
                } else {
                    out.push_str(num_words[d]);
                }
            }
            _ => {
                out.push_str(num_words[d]);
                out.push_str(units[i]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{Book, Stored};
    use crate::value::CellValue;

    struct TestBook;
    impl Book for TestBook {
        fn sheet_count(&self) -> usize { 1 }
        fn sheet_name(&self, _sheet: usize) -> &str { "Sheet1" }
        fn value(&self, _sheet: usize, _row: u32, _col: u32) -> CellValue { CellValue::Empty }
        fn formula(&self, _sheet: usize, _row: u32, _col: u32) -> Option<&str> { None }
        fn for_each_cell(&self, _sheet: usize, _area: (u32, u32, u32, u32), _f: &mut dyn FnMut(u32, u32, &CellValue)) {}
        fn extent(&self, _sheet: usize) -> (u32, u32) { (1, 1) }
        fn defined_name(&self, _name: &str, _sheet: Option<usize>) -> Option<String> { None }
    }

    fn eval_func(name: &str, args: Vec<Expr>) -> Value {
        let book = TestBook;
        let cells = Stored;
        let mut ctx = Ctx::new(&book, &cells, 0, 0, 0);
        let func = get(name).expect("function not found");
        func(&mut ctx, &args)
    }

    #[test]
    fn test_regextest() {
        // Basic match
        let res = eval_func("REGEXTEST", vec![Expr::Text("Hello 123".into()), Expr::Text(r"\d+".into())]);
        assert_eq!(res, Value::Bool(true));

        // No match
        let res = eval_func("REGEXTEST", vec![Expr::Text("Hello World".into()), Expr::Text(r"\d+".into())]);
        assert_eq!(res, Value::Bool(false));

        // Case-sensitive (default 0)
        let res = eval_func("REGEXTEST", vec![Expr::Text("apple".into()), Expr::Text("APPLE".into())]);
        assert_eq!(res, Value::Bool(false));

        // Case-insensitive (1)
        let res = eval_func("REGEXTEST", vec![Expr::Text("apple".into()), Expr::Text("APPLE".into()), Expr::Number(1.0)]);
        assert_eq!(res, Value::Bool(true));

        // Invalid regex pattern -> #VALUE!
        let res = eval_func("REGEXTEST", vec![Expr::Text("abc".into()), Expr::Text("[a-".into())]);
        assert_eq!(res, Value::Error(CellError::Value));

        // Invalid case_sensitivity -> #VALUE!
        let res = eval_func("REGEXTEST", vec![Expr::Text("abc".into()), Expr::Text("a".into()), Expr::Number(5.0)]);
        assert_eq!(res, Value::Error(CellError::Value));
    }

    #[test]
    fn test_regexextract() {
        // Mode 0: first match
        let res = eval_func("REGEXEXTRACT", vec![Expr::Text("Item-42-End".into()), Expr::Text(r"\d+".into())]);
        assert_eq!(res, Value::Text("42".into()));

        // Mode 0: no match -> #N/A
        let res = eval_func("REGEXEXTRACT", vec![Expr::Text("Item-None".into()), Expr::Text(r"\d+".into())]);
        assert_eq!(res, Value::Error(CellError::NA));

        // Mode 1: all matches (row array)
        let res = eval_func("REGEXEXTRACT", vec![Expr::Text("10, 20, 30".into()), Expr::Text(r"\d+".into()), Expr::Number(1.0)]);
        match res {
            Value::Array(arr) => {
                assert_eq!(arr.rows, 1);
                assert_eq!(arr.columns, 3);
                assert_eq!(arr.data, vec![Value::Text("10".into()), Value::Text("20".into()), Value::Text("30".into())]);
            }
            other => panic!("expected array, got {:?}", other),
        }

        // Mode 2: capture groups
        let res = eval_func("REGEXEXTRACT", vec![
            Expr::Text("2026-10-10".into()),
            Expr::Text(r"(\d{4})-(\d{2})-(\d{2})".into()),
            Expr::Number(2.0),
        ]);
        match res {
            Value::Array(arr) => {
                assert_eq!(arr.rows, 1);
                assert_eq!(arr.columns, 3);
                assert_eq!(arr.data, vec![Value::Text("2026".into()), Value::Text("10".into()), Value::Text("10".into())]);
            }
            other => panic!("expected array, got {:?}", other),
        }

        // Mode 2 with no capture groups in pattern -> #VALUE!
        let res = eval_func("REGEXEXTRACT", vec![Expr::Text("abc".into()), Expr::Text(r"\w+".into()), Expr::Number(2.0)]);
        assert_eq!(res, Value::Error(CellError::Value));
    }

    #[test]
    fn test_regexreplace() {
        // Occurrence 0: replace all
        let res = eval_func("REGEXREPLACE", vec![
            Expr::Text("cat bat rat".into()),
            Expr::Text(r"[a-z]at".into()),
            Expr::Text("dog".into()),
        ]);
        assert_eq!(res, Value::Text("dog dog dog".into()));

        // Capture groups $1, $2
        let res = eval_func("REGEXREPLACE", vec![
            Expr::Text("Jane Doe".into()),
            Expr::Text(r"(\w+)\s+(\w+)".into()),
            Expr::Text("$2, $1".into()),
        ]);
        assert_eq!(res, Value::Text("Doe, Jane".into()));

        // Occurrence 1: first occurrence only
        let res = eval_func("REGEXREPLACE", vec![
            Expr::Text("cat bat rat".into()),
            Expr::Text(r"[a-z]at".into()),
            Expr::Text("dog".into()),
            Expr::Number(1.0),
        ]);
        assert_eq!(res, Value::Text("dog bat rat".into()));

        // Occurrence 2: second occurrence only
        let res = eval_func("REGEXREPLACE", vec![
            Expr::Text("cat bat rat".into()),
            Expr::Text(r"[a-z]at".into()),
            Expr::Text("dog".into()),
            Expr::Number(2.0),
        ]);
        assert_eq!(res, Value::Text("cat dog rat".into()));

        // Occurrence -1: last occurrence
        let res = eval_func("REGEXREPLACE", vec![
            Expr::Text("cat bat rat".into()),
            Expr::Text(r"[a-z]at".into()),
            Expr::Text("dog".into()),
            Expr::Number(-1.0),
        ]);
        assert_eq!(res, Value::Text("cat bat dog".into()));

        // Occurrence out of bounds -> original text
        let res = eval_func("REGEXREPLACE", vec![
            Expr::Text("cat bat rat".into()),
            Expr::Text(r"[a-z]at".into()),
            Expr::Text("dog".into()),
            Expr::Number(10.0),
        ]);
        assert_eq!(res, Value::Text("cat bat rat".into()));

        // Case-insensitive (1)
        let res = eval_func("REGEXREPLACE", vec![
            Expr::Text("apple APPLE".into()),
            Expr::Text("apple".into()),
            Expr::Text("orange".into()),
            Expr::Number(0.0),
            Expr::Number(1.0),
        ]);
        assert_eq!(res, Value::Text("orange orange".into()));
    }

    #[test]
    fn test_bahttext() {
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(0.0)]), Value::Text("ศูนย์บาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(1.0)]), Value::Text("หนึ่งบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(11.0)]), Value::Text("สิบเอ็ดบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(21.0)]), Value::Text("ยี่สิบเอ็ดบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(100.0)]), Value::Text("หนึ่งร้อยบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(101.0)]), Value::Text("หนึ่งร้อยเอ็ดบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(1000.0)]), Value::Text("หนึ่งพันบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(1001.0)]), Value::Text("หนึ่งพันเอ็ดบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(1_000_000.0)]), Value::Text("หนึ่งล้านบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(1_000_001.0)]), Value::Text("หนึ่งล้านหนึ่งบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(1_000_011.0)]), Value::Text("หนึ่งล้านสิบเอ็ดบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(1_000_000_000_000.0)]), Value::Text("หนึ่งล้านล้านบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(0.25)]), Value::Text("ยี่สิบห้าสตางค์".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(1.50)]), Value::Text("หนึ่งบาทห้าสิบสตางค์".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(120.75)]), Value::Text("หนึ่งร้อยยี่สิบบาทเจ็ดสิบห้าสตางค์".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(-1.0)]), Value::Text("ลบหนึ่งบาทถ้วน".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(-0.25)]), Value::Text("ลบยี่สิบห้าสตางค์".into()));
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Number(-150.50)]), Value::Text("ลบหนึ่งร้อยห้าสิบบาทห้าสิบสตางค์".into()));

        // Text representation of number
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Text("100".into())]), Value::Text("หนึ่งร้อยบาทถ้วน".into()));

        // Invalid text -> #VALUE!
        assert_eq!(eval_func("BAHTTEXT", vec![Expr::Text("abc".into())]), Value::Error(CellError::Value));
    }
}

