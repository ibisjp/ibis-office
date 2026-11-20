// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The data validations (`DVAL`, `DV`) and conditional formats (`CFHEADER`, `CF`) of a sheet of an old workbook,
//! written from MS-XLS. A conditional format of the file can say a value's comparison or a formula, with the font,
//! border and pattern it gives: the other rules (colour scales, data bars, icon sets) are counted as left out; the
//! top and bottom, average and duplicate rules are made formulas.

use ibis_doc_model::{CellRange, CellUnderline, CfKind, CfRule, CfStyle, ConditionalFormat, Sheet, Validation};

use super::formula::{self, Refs};
use super::{Out, Palette, Report, border_style, pattern_number};

fn ref8(r: &CellRange) -> [u8; 8] {
    let mut d = [0u8; 8];
    d[0..2].copy_from_slice(&(r.first_row.min(65535) as u16).to_le_bytes());
    d[2..4].copy_from_slice(&(r.last_row.min(65535) as u16).to_le_bytes());
    d[4..6].copy_from_slice(&(r.first_column.min(255) as u16).to_le_bytes());
    d[6..8].copy_from_slice(&(r.last_column.min(255) as u16).to_le_bytes());
    d
}

fn in_range(r: &CellRange) -> bool {
    r.first_row <= 65535 && r.first_column <= 255
}

/// A message of a validation: its text, or one NUL character for none.
fn message(text: &str, max: usize) -> Vec<u8> {
    let t: String = text.chars().take(max).collect();
    if t.is_empty() {
        return vec![1, 0, 0, 0];
    }
    super::unicode_string(&t)
}

fn operator_number(op: &str) -> u32 {
    match op {
        "notBetween" => 1,
        "equal" => 2,
        "notEqual" => 3,
        "greaterThan" => 4,
        "lessThan" => 5,
        "greaterThanOrEqual" => 6,
        "lessThanOrEqual" => 7,
        _ => 0,
    }
}

/// The tokens of a list typed in (`"A,B,C"`): one string, its items apart by NUL.
fn list_literal(text: &str) -> Option<Vec<u8>> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    let joined = inner.split(',').collect::<Vec<_>>().join("\0");
    let units: Vec<u16> = joined.encode_utf16().collect();
    if units.len() > 255 {
        return None;
    }
    let mut t = vec![0x17, units.len() as u8];
    if units.iter().all(|&u| u < 0x100) {
        t.push(0);
        t.extend(units.iter().map(|&u| u as u8));
    } else {
        t.push(1);
        t.extend(units.iter().flat_map(|u| u.to_le_bytes()));
    }
    Some(t)
}

fn validation(v: &Validation, number: u16, refs: &mut Refs) -> Option<Vec<u8>> {
    let ranges: Vec<&CellRange> = v.ranges.iter().filter(|r| in_range(r)).collect();
    let first = ranges.first()?;
    let base = (first.first_row, first.first_column);
    let type_code: u32 = match v.kind.as_str() {
        "whole" => 1,
        "decimal" => 2,
        "list" => 3,
        "date" => 4,
        "time" => 5,
        "textLength" => 6,
        "custom" => 7,
        _ => 0,
    };
    let (f1, f2, literal) = match v.kind.as_str() {
        "list" => {
            let text = v.formula1.as_deref()?;
            match list_literal(text) {
                Some(t) => (t, Vec::new(), true),
                None => (formula::compile_relative(text, number, refs, base, true)?, Vec::new(), false),
            }
        }
        "custom" => (formula::compile_relative(v.formula1.as_deref()?, number, refs, base, false)?, Vec::new(), false),
        _ => {
            let a = formula::compile_relative(v.formula1.as_deref()?, number, refs, base, false)?;
            let b = match &v.formula2 {
                Some(f) => formula::compile_relative(f, number, refs, base, false)?,
                None => Vec::new(),
            };
            (a, b, false)
        }
    };
    let err_style = match v.error.as_ref().map(|e| e.0.as_str()) {
        Some("warning") => 1,
        Some("information") => 2,
        _ => 0,
    };
    let flags: u32 = type_code
        | err_style << 4
        | u32::from(literal) << 7
        | u32::from(v.allow_blank) << 8
        | u32::from(v.kind == "list" && !v.dropdown) << 9
        | u32::from(v.prompt.is_some()) << 18
        | u32::from(v.error.is_some()) << 19
        | operator_number(&v.operator) << 20;
    let mut d = flags.to_le_bytes().to_vec();
    let (pt, pm) = v.prompt.clone().unwrap_or_default();
    let (_, et, em) = v.error.clone().unwrap_or_default();
    d.extend(message(&pt, 32));
    d.extend(message(&et, 32));
    d.extend(message(&pm, 255));
    d.extend(message(&em, 255));
    d.extend((f1.len() as u16).to_le_bytes());
    d.extend(0u16.to_le_bytes());
    d.extend(f1);
    d.extend((f2.len() as u16).to_le_bytes());
    d.extend(0u16.to_le_bytes());
    d.extend(f2);
    d.extend((ranges.len() as u16).to_le_bytes());
    for r in ranges {
        d.extend(ref8(r));
    }
    (d.len() <= 8224).then_some(d)
}

pub(super) fn validations(sheet: &Sheet, number: u16, refs: &mut Refs, o: &mut Out, report: &mut Report) {
    let records: Vec<Vec<u8>> = sheet.validations.iter().filter_map(|v| validation(v, number, refs)).collect();
    report.other += sheet.validations.len() - records.len();
    if records.is_empty() {
        return;
    }
    let mut d = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF];
    d.extend((records.len() as u32).to_le_bytes());
    d.truncate(18);
    o.rec(0x1B2, &d);
    for r in &records {
        o.rec(0x1BE, r);
    }
}

// ---------------------------------------------------------------- conditional

/// The `DXFN` of a format (the part of a `CF` record before the formulas).
fn dxfn(s: &CfStyle, palette: &mut Palette) -> Vec<u8> {
    let has_font = s.color.is_some() || s.bold.is_some() || s.italic.is_some() || s.underline.is_some_and(|u| u != CellUnderline::None) || s.strike.is_some();
    let has_border = s.borders.iter().any(Option::is_some);
    let fill = s.fill.map(|c| (1u8, c.0, c.0)).or_else(|| s.pattern.as_ref().filter(|(n, ..)| pattern_number(n) > 0).map(|(n, fg, bg)| (pattern_number(n), fg.0, bg.0)));
    let has_pattern = fill.is_some();
    let mut flags: u32 = 0xFFFF;
    for (k, b) in s.borders.iter().enumerate() {
        if b.is_some() {
            flags &= !(1 << (10 + k));
        }
    }
    flags |= 0x38 << 16;
    if !has_pattern {
        flags |= 0x7 << 16;
    }
    flags |= u32::from(has_font) << 26 | u32::from(has_border) << 28 | u32::from(has_pattern) << 29;
    let mut d = flags.to_le_bytes().to_vec();
    d.extend([0, 0]);
    if has_font {
        let mut f = [0u8; 118];
        f[64..68].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        let ts: u32 = u32::from(s.italic == Some(true)) << 1 | u32::from(s.strike == Some(true)) << 3;
        f[68..72].copy_from_slice(&ts.to_le_bytes());
        f[72..74].copy_from_slice(&(if s.bold == Some(true) { 700u16 } else { 400 }).to_le_bytes());
        let uls: u8 = match s.underline {
            Some(CellUnderline::Single) => 1,
            Some(CellUnderline::Double) => 2,
            Some(CellUnderline::SingleAccounting) => 0x21,
            Some(CellUnderline::DoubleAccounting) => 0x22,
            _ => 0,
        };
        f[76] = uls;
        let icv: u32 = s.color.map_or(0xFFFF_FFFF, |c| u32::from(palette.index(c.0)));
        f[80..84].copy_from_slice(&icv.to_le_bytes());
        let mut ninch: u32 = 0x98;
        if s.italic.is_none() {
            ninch |= 0x2;
        }
        if s.strike.is_some() {
            ninch &= !0x8;
        }
        f[88..92].copy_from_slice(&ninch.to_le_bytes());
        f[92..96].copy_from_slice(&1u32.to_le_bytes());
        f[96..100].copy_from_slice(&u32::from(uls == 0).to_le_bytes());
        f[100..104].copy_from_slice(&u32::from(s.bold.is_none()).to_le_bytes());
        f[116..118].copy_from_slice(&1u16.to_le_bytes());
        d.extend(f);
    }
    if has_border {
        let side = |b: &Option<ibis_doc_model::Border>, pal: &mut Palette| b.as_ref().map(|b| (border_style(b.line) as u32, pal.index(b.color.0) as u32)).unwrap_or((0, 0));
        let ((sl, cl), (sr, cr), (st, ct), (sb, cb)) = (side(&s.borders[0], palette), side(&s.borders[1], palette), side(&s.borders[2], palette), side(&s.borders[3], palette));
        let w0 = sl | sr << 4 | st << 8 | sb << 12 | cl << 16 | cr << 23;
        let w1 = ct | cb << 7;
        d.extend(w0.to_le_bytes());
        d.extend(w1.to_le_bytes());
    }
    if let Some((fls, fg, bg)) = fill {
        let v: u32 = u32::from(fls) << 10 | u32::from(palette.index(fg)) << 16 | u32::from(palette.index(bg)) << 23;
        d.extend(v.to_le_bytes());
    }
    d
}

fn letters(mut n: u32) -> String {
    let mut out = String::new();
    loop {
        out.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    out
}

fn range_text(r: &CellRange) -> String {
    format!("${}${}:${}${}", letters(r.first_column), r.first_row + 1, letters(r.last_column), r.last_row + 1)
}

/// A rule as (`ct`, `cp`, first formula, second formula), `None` for one the file cannot say.
fn rule_parts(rule: &CfRule, first: &CellRange, number: u16, refs: &mut Refs) -> Option<(u8, u8, Vec<u8>, Vec<u8>)> {
    let base = (first.first_row, first.first_column);
    let corner = format!("{}{}", letters(first.first_column), first.first_row + 1);
    let area = range_text(first);
    let formula = |text: &str, refs: &mut Refs| formula::compile_relative(text, number, refs, base, false);
    let expression = |text: &str, refs: &mut Refs| Some((2u8, 0u8, formula(text, refs)?, Vec::new()));
    match &rule.kind {
        CfKind::CellIs { operator, formulas } => {
            let cp = match operator.as_str() {
                "between" => 1,
                "notBetween" => 2,
                "equal" => 3,
                "notEqual" => 4,
                "greaterThan" => 5,
                "lessThan" => 6,
                "greaterThanOrEqual" => 7,
                "lessThanOrEqual" => 8,
                _ => return None,
            };
            let a = formula(formulas.first()?, refs)?;
            let b = match formulas.get(1) {
                Some(f) => formula(f, refs)?,
                None => Vec::new(),
            };
            Some((1, cp, a, b))
        }
        CfKind::Expression(f) => expression(f, refs),
        CfKind::Duplicate => expression(&format!("COUNTIF({area},{corner})>1"), refs),
        CfKind::Unique => expression(&format!("COUNTIF({area},{corner})=1"), refs),
        CfKind::Top10 { rank, percent: false, bottom } => expression(&format!("RANK({corner},{area},{})<={rank}", u8::from(*bottom)), refs),
        CfKind::AboveAverage { above, equal, std_dev: 0 } => {
            let op = match (above, equal) {
                (true, false) => ">",
                (true, true) => ">=",
                (false, false) => "<",
                (false, true) => "<=",
            };
            expression(&format!("{corner}{op}AVERAGE({area})"), refs)
        }
        _ => None,
    }
}

pub(super) fn conditional(sheet: &Sheet, number: u16, refs: &mut Refs, palette: &mut Palette, o: &mut Out, report: &mut Report) {
    for cf in &sheet.conditional {
        write_group(cf, number, refs, palette, o, report);
    }
}

fn write_group(cf: &ConditionalFormat, number: u16, refs: &mut Refs, palette: &mut Palette, o: &mut Out, report: &mut Report) {
    let ranges: Vec<&CellRange> = cf.ranges.iter().filter(|r| in_range(r)).collect();
    let Some(first) = ranges.first().copied() else { return };
    let mut rules: Vec<&CfRule> = cf.rules.iter().collect();
    rules.sort_by_key(|r| r.priority);
    let mut parts: Vec<(Vec<u8>, &CfRule, (u8, u8, Vec<u8>, Vec<u8>))> = Vec::new();
    for rule in rules {
        match rule_parts(rule, first, number, refs) {
            Some(p) => parts.push((rule.style.as_ref().map(|s| dxfn(s, palette)).unwrap_or_else(|| dxfn(&CfStyle::default(), palette)), rule, p)),
            None => report.other += 1,
        }
    }
    // Excel 97-2003 holds three rules for a range: more are in further groups of the same range.
    for chunk in parts.chunks(3) {
        let mut bound = *first;
        for r in &ranges {
            bound.first_row = bound.first_row.min(r.first_row);
            bound.first_column = bound.first_column.min(r.first_column);
            bound.last_row = bound.last_row.max(r.last_row);
            bound.last_column = bound.last_column.max(r.last_column);
        }
        let mut h = (chunk.len() as u16).to_le_bytes().to_vec();
        h.extend(1u16.to_le_bytes());
        h.extend(ref8(&bound));
        h.extend((ranges.len() as u16).to_le_bytes());
        for r in &ranges {
            h.extend(ref8(r));
        }
        o.rec(0x1B0, &h);
        for (dxf, _, (ct, cp, f1, f2)) in chunk {
            let mut d = vec![*ct, *cp];
            d.extend((f1.len() as u16).to_le_bytes());
            d.extend((f2.len() as u16).to_le_bytes());
            d.extend(dxf);
            d.extend(f1);
            d.extend(f2);
            o.rec(0x1B1, &d);
        }
    }
}
