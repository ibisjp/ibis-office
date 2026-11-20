// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A formula as the tokens (`rgce`, the "Ptg"s of MS-XLS 2.5.198) of an old workbook (BIFF8), from the
//! expression tree the formula parser makes. A formula that has what the format cannot hold (a function it has no
//! number for, a reference past row 65 536 or column IV, a name or book it cannot point at, structured references,
//! lambdas…) is `None`: the cell then keeps its value only.
//!
//! Each operand token has a class: a reference (`R`) where the function takes cells, a value (`V`) as an operator's
//! operand, an array (`A`) inside the functions that compute with arrays.

use std::collections::HashMap;

use ibis_calc::CellError;
use ibis_calc::formula::parser::{AreaRef, BinaryOp, Corner, Expr, SheetRef, parse};

/// What the tokens of formulas point at by number.
pub(super) struct Refs {
    /// The sheets' names (lower case) and their index.
    pub sheets: HashMap<String, u16>,
    /// The defined names: (name lower case, sheet scope) → index from 1.
    pub names: HashMap<(String, Option<u16>), u32>,
    /// The `EXTERNSHEET` entries (first sheet, last sheet) of the own workbook, as the formulas ask.
    pub xti: Vec<(u16, u16)>,
}

impl Refs {
    fn xti_of(&mut self, first: u16, last: u16) -> u16 {
        match self.xti.iter().position(|&x| x == (first, last)) {
            Some(i) => i as u16,
            None => {
                self.xti.push((first, last));
                (self.xti.len() - 1) as u16
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Reference,
    Value,
    Array,
}

/// The number of a function and its fixed number of arguments (`None`: any number).
fn function_id(name: &str) -> Option<(u16, Option<usize>)> {
    static TABLE: std::sync::OnceLock<HashMap<&'static str, (u16, Option<usize>)>> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut m = HashMap::new();
        for id in 0..400u16 {
            if let Some((name, fixed)) = crate::xls_formula::function(id) {
                m.insert(name, (id, fixed));
            }
        }
        // More functions of Excel 97-2003 (their numbers are in the file format's table; the argument counts of the
        // ones that take a fixed number are left out, so they are written with the count).
        for (name, id) in [
            ("GAMMALN", 271), ("FISHER", 283), ("FISHERINV", 284), ("STANDARDIZE", 297), ("NORMSDIST", 294), ("NORMSINV", 296), ("NORMDIST", 293), ("NORMINV", 295), ("LOGINV", 291),
            ("LOGNORMDIST", 290), ("EXPONDIST", 280), ("POISSON", 300), ("WEIBULL", 302), ("BINOMDIST", 273), ("CRITBINOM", 278), ("CHIDIST", 274), ("CHIINV", 275), ("FDIST", 281),
            ("FINV", 282), ("TDIST", 301), ("TINV", 332), ("GAMMADIST", 286), ("GAMMAINV", 287), ("BETADIST", 270), ("BETAINV", 272), ("HYPGEOMDIST", 289), ("NEGBINOMDIST", 292),
            ("CONFIDENCE", 277), ("SUMXMY2", 303), ("SUMX2MY2", 304), ("SUMX2PY2", 305), ("CHITEST", 306), ("COVAR", 308), ("FORECAST", 309), ("FTEST", 310), ("PEARSON", 312),
            ("STEYX", 314), ("TTEST", 316), ("PROB", 317), ("DEVSQ", 318), ("GEOMEAN", 319), ("HARMEAN", 320), ("SUMSQ", 321), ("KURT", 322), ("SKEW", 323), ("ZTEST", 324),
            ("PERCENTRANK", 329), ("TRIMMEAN", 331), ("ISPMT", 350), ("DATEDIF", 351), ("ROMAN", 354), ("PHONETIC", 360), ("ASC", 214), ("JIS", 215), ("FINDB", 205),
            ("SEARCHB", 206), ("REPLACEB", 207), ("LEFTB", 208), ("RIGHTB", 209), ("MIDB", 210), ("LENB", 211), ("USDOLLAR", 204), ("GETPIVOTDATA", 358),
        ] {
            m.entry(name).or_insert((id, None));
        }
        m
    });
    // How many parameters it takes if fixed: the file format's own table (the functions with no parameters apart).
    table.get(name).map(|&(id, _)| {
        let fixed = match id {
            10 | 19 | 34 | 35 | 63 | 74 | 221 => Some(0),
            _ => super::function_params::FIXED.iter().find(|(i, _)| *i == id).map(|&(_, n)| n as usize),
        };
        (id, fixed)
    })
}

/// The kind of parameter `i` of function `id`: `V`, `R` or `X` (see `function_params`).
fn param_kind(id: u16, i: usize) -> char {
    static MAP: std::sync::OnceLock<HashMap<u16, &'static str>> = std::sync::OnceLock::new();
    let map = MAP.get_or_init(|| super::function_params::KINDS.iter().copied().collect());
    match map.get(&id) {
        Some(k) => k.chars().nth(i).or_else(|| k.chars().last()).unwrap_or('X'),
        None => 'X',
    }
}

/// Whether an expression is a reference to cells (rather than a value).
fn is_reference(e: &Expr) -> bool {
    match e {
        Expr::Ref(_) | Expr::Name { .. } | Expr::Range(..) | Expr::Intersect(..) | Expr::Union(_) => true,
        Expr::Call { name, .. } => matches!(name.as_str(), "INDEX" | "OFFSET" | "INDIRECT" | "IF" | "CHOOSE"),
        _ => false,
    }
}

/// Functions whose arguments are computed as arrays (outside an array formula too).
fn takes_arrays(name: &str) -> bool {
    matches!(name, "SUMPRODUCT" | "MMULT" | "MINVERSE" | "MDETERM" | "TRANSPOSE" | "LINEST" | "LOGEST" | "TREND" | "GROWTH" | "FREQUENCY" | "LOOKUP" | "SUMXMY2" | "SUMX2MY2" | "SUMX2PY2")
}

fn error_code(e: CellError) -> u8 {
    match e {
        CellError::Null => 0x00,
        CellError::Div0 => 0x07,
        CellError::Value => 0x0F,
        CellError::Ref => 0x17,
        CellError::Name => 0x1D,
        CellError::Num => 0x24,
        CellError::NA => 0x2A,
        _ => 0x0F,
    }
}

/// The tokens of the formula `text` (without `=`) in the cell at (`row`, `column`) of the sheet numbered `sheet`.
pub(super) fn compile(text: &str, sheet: u16, refs: &mut Refs) -> Option<Vec<u8>> {
    compile_as(text, sheet, refs, Class::Value, None)
}

/// A formula of a conditional format or a validation: references that are not fixed are offsets from `base` (the
/// first cell of the range, as `(row, column)`); `class` is what the formula is read as (`Value`, `Reference` for
/// a list's cells).
pub(super) fn compile_relative(text: &str, sheet: u16, refs: &mut Refs, base: (u32, u32), as_reference: bool) -> Option<Vec<u8>> {
    compile_as(text, sheet, refs, if as_reference { Class::Reference } else { Class::Value }, Some(base))
}

fn compile_as(text: &str, sheet: u16, refs: &mut Refs, class: Class, base: Option<(u32, u32)>) -> Option<Vec<u8>> {
    let expr = parse(text).ok()?;
    let mut out = Vec::new();
    let cx = Cx { own: sheet, base };
    expr_tokens(&expr, class, &cx, refs, &mut out)?;
    (out.len() <= 8000).then_some(out)
}

struct Cx {
    own: u16,
    /// Where relative references are counted from (a conditional format's or validation's first cell).
    base: Option<(u32, u32)>,
}

fn class_bits(base: u8, class: Class) -> u8 {
    // The operand tokens: reference class 0x20, value 0x40, array 0x60 (added to the token's number 0x04…0x1F).
    base
        + match class {
            Class::Reference => 0x20,
            Class::Value => 0x40,
            Class::Array => 0x60,
        }
}

fn corner_ok(c: &Corner) -> bool {
    c.row <= 65535 && c.column <= 255
}

fn col_word(c: &Corner) -> u16 {
    c.column as u16 | if c.column_fixed { 0 } else { 0x4000 } | if c.row_fixed { 0 } else { 0x8000 }
}

/// The row of a corner as the file has it: the row, or (in a formula counted from `base`) the offset of a relative one.
fn row_word(c: &Corner, base: Option<(u32, u32)>) -> u16 {
    match base {
        Some((r, _)) if !c.row_fixed => (c.row as i64 - r as i64) as u16,
        _ => c.row as u16,
    }
}

/// The column word of a corner: the column (the offset, if counted from `base`) and the two "relative" flags.
fn col_word_at(c: &Corner, base: Option<(u32, u32)>) -> u16 {
    let col = match base {
        Some((_, b)) if !c.column_fixed => ((c.column as i64 - b as i64) & 0xFF) as u16,
        _ => c.column as u16,
    };
    col | if c.column_fixed { 0 } else { 0x4000 } | if c.row_fixed { 0 } else { 0x8000 }
}

fn sheet_numbers(sheet: &Option<SheetRef>, _own: u16, refs: &Refs) -> Option<Option<(u16, u16)>> {
    match sheet {
        None => Some(None),
        Some(s) => {
            if s.book.is_some() {
                return None;
            }
            let first = *refs.sheets.get(&s.first.to_lowercase())?;
            let last = match &s.last {
                Some(l) => *refs.sheets.get(&l.to_lowercase())?,
                None => first,
            };
            Some(Some((first, last)))
        }
    }
}

fn area_tokens(r: &AreaRef, class: Class, cx: &Cx, refs: &mut Refs, out: &mut Vec<u8>) -> Option<()> {
    let own = cx.own;
    let mut start = r.start;
    let mut end = r.end;
    if r.whole_columns {
        start.row = 0;
        end.row = 65535;
        start.row_fixed = true;
        end.row_fixed = true;
    }
    if r.whole_rows {
        start.column = 0;
        end.column = 255;
        start.column_fixed = true;
        end.column_fixed = true;
    }
    if !corner_ok(&start) || !corner_ok(&end) {
        return None;
    }
    let sheet = sheet_numbers(&r.sheet, own, refs)?;
    let single = r.is_cell();
    match (sheet, single) {
        (None, true) => {
            out.push(class_bits(if cx.base.is_some() { 0x0C } else { 0x04 }, class));
            out.extend(row_word(&start, cx.base).to_le_bytes());
            out.extend(col_word_at(&start, cx.base).to_le_bytes());
        }
        (None, false) => {
            out.push(class_bits(if cx.base.is_some() { 0x0D } else { 0x05 }, class));
            out.extend(row_word(&start, cx.base).to_le_bytes());
            out.extend(row_word(&end, cx.base).to_le_bytes());
            out.extend(col_word_at(&start, cx.base).to_le_bytes());
            out.extend(col_word_at(&end, cx.base).to_le_bytes());
        }
        (Some((first, last)), true) => {
            let ixti = refs.xti_of(first, last);
            out.push(class_bits(0x1A, class));
            out.extend(ixti.to_le_bytes());
            out.extend((start.row as u16).to_le_bytes());
            out.extend(col_word(&start).to_le_bytes());
        }
        (Some((first, last)), false) => {
            let ixti = refs.xti_of(first, last);
            out.push(class_bits(0x1B, class));
            out.extend(ixti.to_le_bytes());
            out.extend((start.row as u16).to_le_bytes());
            out.extend((end.row as u16).to_le_bytes());
            out.extend(col_word(&start).to_le_bytes());
            out.extend(col_word(&end).to_le_bytes());
        }
    }
    Some(())
}

fn operand_class(class: Class) -> Class {
    if class == Class::Array { Class::Array } else { Class::Value }
}

fn expr_tokens(e: &Expr, class: Class, own: &Cx, refs: &mut Refs, out: &mut Vec<u8>) -> Option<()> {
    match e {
        Expr::Number(n) => {
            if n.fract() == 0.0 && (0.0..=65535.0).contains(n) && !(*n == 0.0 && n.is_sign_negative()) {
                out.push(0x1E);
                out.extend((*n as u16).to_le_bytes());
            } else {
                out.push(0x1F);
                out.extend(n.to_le_bytes());
            }
        }
        Expr::Text(t) => {
            let units: Vec<u16> = t.encode_utf16().collect();
            if units.len() > 255 {
                return None;
            }
            out.push(0x17);
            out.push(units.len() as u8);
            if t.chars().all(|c| (c as u32) < 0x100) {
                out.push(0);
                out.extend(t.chars().map(|c| c as u8));
            } else {
                out.push(1);
                out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
            }
        }
        Expr::Bool(b) => {
            out.push(0x1D);
            out.push(*b as u8);
        }
        Expr::Error(e) => {
            out.push(0x1C);
            out.push(error_code(*e));
        }
        Expr::Missing => out.push(0x16),
        Expr::Ref(r) => area_tokens(r, class, own, refs, out)?,
        Expr::Name { sheet, name } => {
            let scope = match sheet {
                Some(s) => Some(*refs.sheets.get(&s.first.to_lowercase())?),
                None => None,
            };
            let key = name.to_lowercase();
            let index = refs.names.get(&(key.clone(), scope)).or_else(|| refs.names.get(&(key, None)))?;
            out.push(class_bits(0x03, class));
            out.extend(index.to_le_bytes());
        }
        Expr::Neg(a) => {
            expr_tokens(a, operand_class(class), own, refs, out)?;
            out.push(0x13);
        }
        Expr::Plus(a) => {
            expr_tokens(a, operand_class(class), own, refs, out)?;
            out.push(0x12);
        }
        Expr::Percent(a) => {
            expr_tokens(a, operand_class(class), own, refs, out)?;
            out.push(0x14);
        }
        Expr::Binary(op, a, b) => {
            let c = operand_class(class);
            expr_tokens(a, c, own, refs, out)?;
            expr_tokens(b, c, own, refs, out)?;
            out.push(match op {
                BinaryOp::Add => 0x03,
                BinaryOp::Sub => 0x04,
                BinaryOp::Mul => 0x05,
                BinaryOp::Div => 0x06,
                BinaryOp::Pow => 0x07,
                BinaryOp::Concat => 0x08,
                BinaryOp::Lt => 0x09,
                BinaryOp::Le => 0x0A,
                BinaryOp::Eq => 0x0B,
                BinaryOp::Ge => 0x0C,
                BinaryOp::Gt => 0x0D,
                BinaryOp::Ne => 0x0E,
            });
        }
        Expr::Range(a, b) => {
            expr_tokens(a, Class::Reference, own, refs, out)?;
            expr_tokens(b, Class::Reference, own, refs, out)?;
            out.push(0x11);
        }
        Expr::Intersect(a, b) => {
            expr_tokens(a, Class::Reference, own, refs, out)?;
            expr_tokens(b, Class::Reference, own, refs, out)?;
            out.push(0x0F);
        }
        Expr::Union(parts) => {
            for (i, p) in parts.iter().enumerate() {
                expr_tokens(p, Class::Reference, own, refs, out)?;
                if i > 0 {
                    out.push(0x10);
                }
            }
        }
        Expr::Call { name, args } => {
            let (id, fixed) = function_id(name)?;
            let arrays = takes_arrays(name);
            let class_of = |i: usize, arg: &Expr| -> Class {
                if arrays {
                    return Class::Array;
                }
                match param_kind(id, i) {
                    'V' => Class::Value,
                    'R' => Class::Reference,
                    _ if is_reference(arg) => Class::Reference,
                    _ => Class::Value,
                }
            };
            // IF: the branches are skipped by the tokens `ptgAttrIf` and `ptgAttrGoto`, as Excel writes it.
            if name == "IF" && (args.len() == 2 || args.len() == 3) {
                expr_tokens(&args[0], Class::Value, own, refs, out)?;
                let mut when_true = Vec::new();
                expr_tokens(&args[1], class_of(1, &args[1]), own, refs, &mut when_true)?;
                let mut when_false = Vec::new();
                if args.len() == 3 {
                    expr_tokens(&args[2], class_of(2, &args[2]), own, refs, &mut when_false)?;
                }
                let func = 4usize;
                // ptgAttrIf: to the false branch, past the goto after the true one.
                out.extend([0x19, 0x02]);
                out.extend(((when_true.len() + 4) as u16).to_le_bytes());
                out.extend(when_true);
                // ptgAttrGoto: to the last byte of the function token.
                // One less than what follows the goto: the false branch, its own goto (4) and the function (4).
                let skip = if args.len() == 3 { when_false.len() + 4 + func - 1 } else { func - 1 };
                out.extend([0x19, 0x08]);
                out.extend((skip as u16).to_le_bytes());
                if args.len() == 3 {
                    out.extend(when_false);
                    out.extend([0x19, 0x08]);
                    out.extend(((func - 1) as u16).to_le_bytes());
                }
                out.push(class_bits(0x02, class));
                out.push(args.len() as u8);
                out.extend(id.to_le_bytes());
                return Some(());
            }
            for (i, a) in args.iter().enumerate() {
                expr_tokens(a, class_of(i, a), own, refs, out)?;
            }
            match fixed {
                Some(n) if n == args.len() => {
                    out.push(class_bits(0x01, class));
                    out.extend(id.to_le_bytes());
                }
                _ => {
                    if args.len() > 255 {
                        return None;
                    }
                    out.push(class_bits(0x02, class));
                    out.push(args.len() as u8);
                    out.extend(id.to_le_bytes());
                }
            }
        }
        Expr::Array(rows) => {
            // The values go after the formula's tokens (`ptgExtraArray`): handled by the caller through `array`.
            let _ = rows;
            return None;
        }
        _ => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs() -> Refs {
        Refs { sheets: [("sheet1".to_string(), 0u16)].into_iter().collect(), names: HashMap::new(), xti: Vec::new() }
    }

    #[test]
    fn an_if_has_its_attribute_tokens_as_the_specification_says() {
        let t = compile("IF(A1,1,2)", 0, &mut refs()).unwrap();
        // A1 as a value; PtgAttrIf past the true branch and the goto after it; the goto's offset one less than
        // what remains (the false branch, its own goto and the function); the last goto; IF with 3 parameters.
        assert_eq!(t, [0x44, 0, 0, 0, 0xC0, 0x19, 0x02, 7, 0, 0x1E, 1, 0, 0x19, 0x08, 10, 0, 0x1E, 2, 0, 0x19, 0x08, 3, 0, 0x42, 3, 1, 0]);
        // Two parameters: one goto.
        let t = compile("IF(A1,1)", 0, &mut refs()).unwrap();
        assert_eq!(t, [0x44, 0, 0, 0, 0xC0, 0x19, 0x02, 7, 0, 0x1E, 1, 0, 0x19, 0x08, 3, 0, 0x42, 2, 1, 0]);
    }

    #[test]
    fn the_arguments_have_the_kind_their_function_takes() {
        // VLOOKUP(value, reference-or-value, reference-or-value, value): G2 a value, A2:C6 a reference.
        let t = compile("VLOOKUP(G2,A2:C6,2,FALSE)", 0, &mut refs()).unwrap();
        assert_eq!(t[0], 0x44, "the lookup value is a value token");
        assert_eq!(t[5], 0x25, "the table is a reference token");
        // SUM(A1:A3): references; ROUND(A1,0): values.
        assert_eq!(compile("SUM(A1:A3)", 0, &mut refs()).unwrap()[0], 0x25);
        assert_eq!(compile("ROUND(A1,0)", 0, &mut refs()).unwrap()[0], 0x44);
    }
}
