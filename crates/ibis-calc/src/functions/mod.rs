// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Excel's functions, by category, and what they share: taking arguments
//! as Excel takes them.
//!
//! A function gets the argument expressions and the evaluation context,
//! so that it evaluates what it needs as it needs it: IF only its chosen
//! branch, SUM its ranges as ranges, ROW its reference as a reference.

pub mod table;

mod database;
pub(crate) mod date;
mod dist;
mod engineering;
mod ets;
mod financial;
mod groupby;
mod info;
mod logical;
mod lookup;
pub(crate) mod math;
pub(crate) mod stat;
mod text;
pub use text::to_half_width;
pub(crate) mod xml;

use std::rc::Rc;

use crate::eval::{Array, Ctx, Res, Value, num, to_bool, to_number, to_text};
use crate::formula::Expr;
use crate::value::CellError;

pub type Func = fn(&mut Ctx, &[Expr]) -> Value;

/// The function named `name` (upper case, no prefix), if the engine has it.
pub fn lookup(name: &str) -> Option<Func> {
    math::get(name).or_else(|| logical::get(name)).or_else(|| info::get(name)).or_else(|| text::get(name)).or_else(|| stat::get(name)).or_else(|| lookup::get(name)).or_else(|| date::get(name)).or_else(|| financial::get(name)).or_else(|| groupby::get(name)).or_else(|| ets::get(name)).or_else(|| engineering::get(name)).or_else(|| database::get(name))
}

/// Functions of the Analysis ToolPak, which give `#VALUE!` for a logical
/// value where they want a number or date.
pub(crate) fn refuses_logical(name: &str) -> bool {
    matches!(
        name,
        "ACCRINT" | "ACCRINTM" | "AMORDEGRC" | "AMORLINC" | "COUPDAYBS" | "COUPDAYS" | "COUPDAYSNC" | "COUPNCD" | "COUPNUM" | "COUPPCD" | "CUMIPMT"
            | "CUMPRINC" | "DISC" | "DOLLARDE" | "DOLLARFR" | "DURATION" | "EFFECT" | "FVSCHEDULE" | "INTRATE" | "MDURATION" | "NOMINAL" | "ODDFPRICE"
            | "ODDFYIELD" | "ODDLPRICE" | "ODDLYIELD" | "PRICE" | "PRICEDISC" | "PRICEMAT" | "RECEIVED" | "TBILLEQ" | "TBILLPRICE" | "TBILLYIELD"
            | "XIRR" | "XNPV" | "YIELD" | "YIELDDISC" | "YIELDMAT"
            | "BESSELI" | "BESSELJ" | "BESSELK" | "BESSELY" | "COMPLEX" | "CONVERT" | "DELTA" | "ERF" | "ERF.PRECISE" | "ERFC" | "ERFC.PRECISE"
            | "GESTEP" | "IMABS" | "IMAGINARY" | "IMARGUMENT" | "IMCONJUGATE" | "IMCOS" | "IMCOSH" | "IMCOT" | "IMCSC" | "IMCSCH" | "IMDIV" | "IMEXP"
            | "IMLN" | "IMLOG10" | "IMLOG2" | "IMPOWER" | "IMREAL" | "IMSEC" | "IMSECH" | "IMSIN" | "IMSINH" | "IMSQRT" | "IMSUB"
            | "IMTAN"
    )
}

/// Functions whose arguments (those of the returned bit mask: bit i is argument i) are scalars: given an array, or a range of several cells
/// that is read as one, for one of them they run once for each element and give an array (Excel 365 does; measured on Excel 16.0.20527 with
/// `Formula2` and the spill range, 2026-11-09, #141: `=BIN2DEC({"1","10",3})`, `=IMABS({1,2,3})`, `=ADDRESS({1,2,3},2)`,
/// `=PERCENTILE(2,{1,2,3})`...). The arguments that are arrays by nature (the data of PERCENTILE, the holidays of WORKDAY, the schedule of
/// FVSCHEDULE) are not in the mask. The other functions lift in their own code (`numeric`, `texts`, `map_args`).
pub(crate) fn scalar_args_lifted(name: &str) -> u32 {
    match name {
        "ARABIC" | "ROMAN" | "ERROR.TYPE" | "HYPERLINK" | "IMAGE" | "ADDRESS" | "COMPLEX" | "DECIMAL" | "REGEXEXTRACT" => 0xFF,
        "BIN2DEC" | "BIN2HEX" | "BIN2OCT" | "DEC2BIN" | "DEC2HEX" | "DEC2OCT" | "HEX2BIN" | "HEX2DEC" | "HEX2OCT" | "OCT2BIN" | "OCT2DEC" | "OCT2HEX" => 0xFF,
        "IMABS" | "IMAGINARY" | "IMARGUMENT" | "IMCONJUGATE" | "IMCOS" | "IMCOSH" | "IMCOT" | "IMCSC" | "IMCSCH" | "IMDIV" | "IMEXP" | "IMLN" | "IMLOG10"
        | "IMLOG2" | "IMPOWER" | "IMREAL" | "IMSEC" | "IMSECH" | "IMSIN" | "IMSINH" | "IMSQRT" | "IMSUB" | "IMTAN" => 0xFF,
        // Start and end dates (and the weekend); the holidays are an array.
        "NETWORKDAYS" | "WORKDAY" => 0b011,
        "NETWORKDAYS.INTL" | "WORKDAY.INTL" => 0b111,
        // The point asked for; the data are an array.
        "PERCENTILE" | "PERCENTILE.INC" | "PERCENTILE.EXC" | "QUARTILE" | "QUARTILE.INC" | "QUARTILE.EXC" => 0b10,
        "PERCENTRANK" | "PERCENTRANK.INC" | "PERCENTRANK.EXC" => 0b100 | 0b010,
        "FVSCHEDULE" | "NPV" => 0b1,
        "TEXTBEFORE" | "TEXTAFTER" => 0b101,
        "VALUETOTEXT" | "ARRAYTOTEXT" => 0b10,
        _ => 0,
    }
}

/// The names of the functions the engine has.
pub fn implemented() -> Vec<&'static str> {
    table::FUNCTIONS.iter().map(|f| f.name).filter(|n| lookup(n).is_some()).collect()
}

// ----- Taking arguments ------------------------------------------------

/// Argument `i` as a number (one value; text that reads as a number is one).
pub(crate) fn arg_num(ctx: &mut Ctx, args: &[Expr], i: usize) -> Res<f64> {
    let v = ctx.eval_scalar(&args[i]);
    to_number(&v, ctx.date1904())
}

pub(crate) fn arg_text(ctx: &mut Ctx, args: &[Expr], i: usize) -> Res<String> {
    let v = ctx.eval_scalar(&args[i]);
    to_text(&v)
}

/// A function of numbers, element by element over arrays: each argument
/// taken as a number (`#VALUE!` for text that is not one).
pub(crate) fn numeric(ctx: &mut Ctx, args: &[Expr], f: impl Fn(&[f64]) -> Value) -> Value {
    let date1904 = ctx.date1904();
    ctx.map_args(args, |_, vals| {
        let mut ns = Vec::with_capacity(vals.len());
        for v in vals {
            match to_number(v, date1904) {
                Ok(n) => ns.push(n),
                Err(e) => return Value::Error(e),
            }
        }
        f(&ns)
    })
}

/// Like [`numeric`], with the arguments not given filled from `defaults`
/// (by position; `defaults.len()` is the most arguments).
pub(crate) fn numeric_defaults(ctx: &mut Ctx, args: &[Expr], defaults: &[f64], f: impl Fn(&[f64]) -> Value) -> Value {
    let given = args.len();
    numeric(ctx, args, |ns| {
        let mut all: Vec<f64> = ns.to_vec();
        for d in defaults.iter().skip(given) {
            all.push(*d);
        }
        f(&all)
    })
}

pub(crate) fn numeric_with_bool(ctx: &mut Ctx, args: &[Expr], bool_arg_idx: usize, f: impl Fn(&[f64]) -> Value) -> Value {
    let date1904 = ctx.date1904();
    ctx.map_args(args, |_, vals| {
        let mut ns = Vec::with_capacity(vals.len());
        for (i, v) in vals.iter().enumerate() {
            if i == bool_arg_idx {
                match to_bool(v) {
                    Ok(b) => ns.push(if b { 1.0 } else { 0.0 }),
                    Err(e) => return Value::Error(e),
                }
            } else {
                match to_number(v, date1904) {
                    Ok(n) => ns.push(n),
                    Err(e) => return Value::Error(e),
                }
            }
        }
        f(&ns)
    })
}

pub(crate) fn numeric_defaults_with_bool(
    ctx: &mut Ctx,
    args: &[Expr],
    defaults: &[f64],
    bool_arg_idx: usize,
    f: impl Fn(&[f64]) -> Value,
) -> Value {
    let given = args.len();
    numeric_with_bool(ctx, args, bool_arg_idx, |ns| {
        let mut all: Vec<f64> = ns.to_vec();
        for d in defaults.iter().skip(given) {
            all.push(*d);
        }
        f(&all)
    })
}

/// How an aggregate takes the values it is given.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Collect {
    /// SUM, AVERAGE, MAX…: numbers from ranges and arrays; typed-in text
    /// that reads as a number and logical values count, other text is
    /// `#VALUE!`.
    Numbers,
    /// AVERAGEA, MAXA…: text in ranges counts as 0, TRUE as 1.
    NumbersA,
}

/// The numbers of the arguments of an aggregate, or the first error.
pub(crate) fn collect_numbers(ctx: &mut Ctx, args: &[Expr], mode: Collect) -> Res<Vec<f64>> {
    let mut out = Vec::new();
    for a in args {
        collect_one(ctx, a, mode, &mut out)?;
    }
    Ok(out)
}

pub(crate) fn collect_one(ctx: &mut Ctx, a: &Expr, mode: Collect, out: &mut Vec<f64>) -> Res<()> {
    let date1904 = ctx.date1904();
    match ctx.eval(a) {
        Value::Ref(areas) => {
            let mut err = None;
            for area in areas.iter() {
                ctx.for_each(area, &mut |_, _, _, v| {
                    if err.is_some() {
                        return;
                    }
                    match v {
                        Value::Number(n) => out.push(n),
                        Value::Error(e) => err = Some(e),
                        Value::Text(_) if mode == Collect::NumbersA => out.push(0.0),
                        Value::Bool(b) if mode == Collect::NumbersA => out.push(if b { 1.0 } else { 0.0 }),
                        _ => {}
                    }
                });
            }
            if let Some(e) = err {
                return Err(e);
            }
        }
        Value::Array(arr) => {
            for v in arr.data.iter() {
                match v {
                    Value::Number(n) => out.push(*n),
                    Value::Error(e) => return Err(*e),
                    // A function value is no number.
                    Value::Lambda(_) => return Err(CellError::Value),
                    Value::Text(_) if mode == Collect::NumbersA => out.push(0.0),
                    Value::Bool(b) if mode == Collect::NumbersA => out.push(if *b { 1.0 } else { 0.0 }),
                    _ => {}
                }
            }
        }
        Value::Empty if matches!(a, Expr::Missing) => out.push(0.0),
        // A bound omitted LAMBDA value keeps the former ordinary-empty aggregate behavior.
        Value::Empty | Value::Omitted => {}
        v => out.push(to_number(&v, date1904)?),
    }
    Ok(())
}

/// Every value of the arguments, flattened: ranges' cells (empty ones
/// left out unless `with_empty`), arrays' elements, and single values.
/// Each value comes with whether it was typed in directly.
pub(crate) fn collect_values(ctx: &mut Ctx, args: &[Expr], with_empty: bool) -> Vec<(Value, bool)> {
    let mut out = Vec::new();
    for a in args {
        match ctx.eval(a) {
            Value::Ref(areas) => {
                for area in areas.iter() {
                    if with_empty {
                        if let Value::Array(arr) = ctx.ref_to_array(&[*area]) {
                            out.extend(arr.data.iter().cloned().map(|v| (v, false)));
                        }
                    } else {
                        ctx.for_each(area, &mut |_, _, _, v| {
                            if !matches!(v, Value::Empty) {
                                out.push((v, false));
                            }
                        });
                    }
                }
            }
            Value::Array(arr) => out.extend(arr.data.iter().cloned().map(|v| (v, false))),
            // ISOMITTED uses raw evaluation; ordinary aggregates consume empty values.
            Value::Omitted => out.push((Value::Empty, true)),
            v => out.push((v, true)),
        }
    }
    out
}

/// An argument as an array (a range's cells, an array, or one value).
pub(crate) fn arg_array(ctx: &mut Ctx, e: &Expr) -> Res<Rc<Array>> {
    match ctx.eval_array(e) {
        Value::Array(a) => Ok(a),
        Value::Error(e) => Err(e),
        // A function as a value (`HSTACK(SUM,MAX)`) is an element like any other.
        v => Ok(Rc::new(Array::new(1, 1, vec![v]))),
    }
}

pub(crate) fn array_value(rows: usize, columns: usize, data: Vec<Value>) -> Value {
    if rows == 0 || columns == 0 {
        return Value::Error(CellError::Calc);
    }
    Value::Array(Rc::new(Array::new(rows, columns, data)))
}

pub(crate) fn err(e: CellError) -> Value {
    Value::Error(e)
}

pub(crate) fn number(n: f64) -> Value {
    num(n)
}

// ----- Wildcards ---------------------------------------------------------

/// A pattern with Excel's wildcards (`?` one character, `*` any, `~`
/// before one of them for itself), over case-folded units of the chosen type.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pat<U> {
    Unit(U),
    One,
    Any,
}

fn fold_units(s: &str) -> Vec<u16> {
    s.chars().flat_map(|c| {
        let l = c.to_lowercase().next().unwrap_or(c);
        let mut buf = [0u16; 2];
        l.encode_utf16(&mut buf).to_vec()
    })
    .collect()
}

fn pattern(p: &str) -> Vec<Pat<u16>> {
    pattern_units(&fold_units(p), b'~' as u16, b'?' as u16, b'*' as u16)
}

fn pattern_units<U: Copy + PartialEq>(units: &[U], escape: U, one: U, any: U) -> Vec<Pat<U>> {
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if u == escape && i + 1 < units.len() && (units[i + 1] == one || units[i + 1] == any || units[i + 1] == escape) {
            out.push(Pat::Unit(units[i + 1]));
            i += 2;
            continue;
        }
        out.push(if u == one { Pat::One } else if u == any { Pat::Any } else { Pat::Unit(u) });
        i += 1;
    }
    out
}

/// Whether `p` matches all of `h` (`prefix`: a start of `h`).
fn matches_at<U: Copy + PartialEq>(p: &[Pat<U>], h: &[U], prefix: bool) -> bool {
    // Iterative matching with backtracking to the last `*`.
    let (mut pi, mut hi) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    loop {
        if pi == p.len() && (prefix || hi == h.len()) {
            return true;
        }
        if pi < p.len() {
            match p[pi] {
                Pat::Any => {
                    star = Some((pi, hi));
                    pi += 1;
                    continue;
                }
                Pat::One if hi < h.len() => {
                    pi += 1;
                    hi += 1;
                    continue;
                }
                Pat::Unit(u) if hi < h.len() && h[hi] == u => {
                    pi += 1;
                    hi += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some((sp, sh)) if sh < h.len() => {
                star = Some((sp, sh + 1));
                pi = sp + 1;
                hi = sh + 1;
            }
            _ => return false,
        }
    }
}

/// A pattern ready to match, and its units when it has no wildcard.
struct Compiled {
    pat: Vec<Pat<u16>>,
    plain: Option<Vec<u16>>,
}

thread_local! {
    /// The pattern last used: a criterion is matched against every cell of a
    /// range, so it is read once.
    static LAST_PATTERN: std::cell::RefCell<Option<(String, Rc<Compiled>)>> = const { std::cell::RefCell::new(None) };
}

fn compiled(pattern_text: &str) -> Rc<Compiled> {
    LAST_PATTERN.with(|cache| {
        if let Some((text, c)) = cache.borrow().as_ref()
            && text == pattern_text
        {
            return c.clone();
        }
        let pat = pattern(pattern_text);
        let plain = pat.iter().map(|p| match p { Pat::Unit(u) => Some(*u), _ => None }).collect::<Option<Vec<u16>>>();
        let c = Rc::new(Compiled { pat, plain });
        *cache.borrow_mut() = Some((pattern_text.to_string(), c.clone()));
        c
    })
}

/// Whether `text`, folded to lowercase, is exactly `units` (no copy made).
fn folded_equals(text: &str, units: &[u16]) -> bool {
    let mut i = 0;
    for ch in text.chars() {
        let l = ch.to_lowercase().next().unwrap_or(ch);
        let mut buf = [0u16; 2];
        for u in l.encode_utf16(&mut buf).iter() {
            if units.get(i) != Some(u) {
                return false;
            }
            i += 1;
        }
    }
    i == units.len()
}

/// Whether `text` matches `pattern` (wildcards, without regard to case).
pub(crate) fn wildcard_match(pattern_text: &str, text: &str) -> bool {
    let c = compiled(pattern_text);
    match &c.plain {
        Some(units) => folded_equals(text, units),
        None => matches_at(&c.pat, &fold_units(text), false),
    }
}

/// Where `needle` (with wildcards) first matches in `hay`, at or after
/// UTF-16 position `from`, without regard to case (SEARCH).
pub(crate) fn wildcard_search(needle: &str, hay: &str, from: usize) -> Option<usize> {
    let p = pattern(needle);
    let h = fold_units(hay);
    (from..=h.len()).find(|&i| matches_at(&p, &h[i..], true))
}

/// SEARCH in a version-2 workbook: positions and `?` count Unicode scalars.
/// The legacy UTF-16 matcher used by lookup/criteria functions is unchanged.
pub(crate) fn wildcard_search_unicode(needle: &str, hay: &str, from: usize) -> Option<usize> {
    let fold = |s: &str| s.chars().map(|c| c.to_lowercase().next().unwrap_or(c)).collect::<Vec<_>>();
    let p = pattern_units(&fold(needle), '~', '?', '*');
    let h = fold(hay);
    (from..=h.len()).find(|&i| matches_at(&p, &h[i..], true))
}

/// Whether text has wildcards to match with.
pub(crate) fn has_wildcards(s: &str) -> bool {
    s.contains(['?', '*', '~'])
}
