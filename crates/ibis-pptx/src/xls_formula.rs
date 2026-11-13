// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A formula of an old workbook (`.xls`, BIFF8) as text: its tokens
//! (`rgce`, the "Ptg"s of MS-XLS §2.5.198) read back into the formula a
//! person typed. A token this does not know makes the whole formula
//! unreadable (`None`): the cell then keeps its cached value only.

/// What the formulas of a workbook refer to by number.
#[derive(Default)]
pub(crate) struct Context {
    /// The sheets' names, in the workbook's order.
    pub sheets: Vec<String>,
    /// `EXTERNSHEET`'s entries: (supporting book, first sheet, last sheet).
    pub xti: Vec<(u16, u16, u16)>,
    /// For each supporting book: whether it is this workbook (`SUPBOOK` that refers to itself).
    pub internal_book: Vec<bool>,
    /// The defined names' texts, in order (1-based in `ptgName`).
    pub names: Vec<String>,
}

/// The name and the number of arguments of function `id` (`None` for the
/// ones taking a variable number: their count is in the token).
pub(crate) fn function(id: u16) -> Option<(&'static str, Option<usize>)> {
    let var = None;
    Some(match id {
        0 => ("COUNT", var), 1 => ("IF", var), 2 => ("ISNA", Some(1)), 3 => ("ISERROR", Some(1)), 4 => ("SUM", var), 5 => ("AVERAGE", var),
        6 => ("MIN", var), 7 => ("MAX", var), 8 => ("ROW", var), 9 => ("COLUMN", var), 10 => ("NA", Some(0)), 11 => ("NPV", var), 12 => ("STDEV", var),
        13 => ("DOLLAR", var), 14 => ("FIXED", var), 15 => ("SIN", Some(1)), 16 => ("COS", Some(1)), 17 => ("TAN", Some(1)), 18 => ("ATAN", Some(1)),
        19 => ("PI", Some(0)), 20 => ("SQRT", Some(1)), 21 => ("EXP", Some(1)), 22 => ("LN", Some(1)), 23 => ("LOG10", Some(1)), 24 => ("ABS", Some(1)),
        25 => ("INT", Some(1)), 26 => ("SIGN", Some(1)), 27 => ("ROUND", Some(2)), 28 => ("LOOKUP", var), 29 => ("INDEX", var), 30 => ("REPT", Some(2)),
        31 => ("MID", Some(3)), 32 => ("LEN", Some(1)), 33 => ("VALUE", Some(1)), 34 => ("TRUE", Some(0)), 35 => ("FALSE", Some(0)), 36 => ("AND", var),
        37 => ("OR", var), 38 => ("NOT", Some(1)), 39 => ("MOD", Some(2)), 40 => ("DCOUNT", Some(3)), 41 => ("DSUM", Some(3)), 42 => ("DAVERAGE", Some(3)),
        43 => ("DMIN", Some(3)), 44 => ("DMAX", Some(3)), 45 => ("DSTDEV", Some(3)), 46 => ("VAR", var), 47 => ("DVAR", Some(3)), 48 => ("TEXT", Some(2)),
        49 => ("LINEST", var), 50 => ("TREND", var), 51 => ("LOGEST", var), 52 => ("GROWTH", var), 56 => ("PV", var), 57 => ("FV", var), 58 => ("NPER", var),
        59 => ("PMT", var), 60 => ("RATE", var), 61 => ("MIRR", Some(3)), 62 => ("IRR", var), 63 => ("RAND", Some(0)), 64 => ("MATCH", var), 65 => ("DATE", Some(3)),
        66 => ("TIME", Some(3)), 67 => ("DAY", Some(1)), 68 => ("MONTH", Some(1)), 69 => ("YEAR", Some(1)), 70 => ("WEEKDAY", var), 71 => ("HOUR", Some(1)),
        72 => ("MINUTE", Some(1)), 73 => ("SECOND", Some(1)), 74 => ("NOW", Some(0)), 75 => ("AREAS", Some(1)), 76 => ("ROWS", Some(1)), 77 => ("COLUMNS", Some(1)),
        78 => ("OFFSET", var), 82 => ("SEARCH", var), 83 => ("TRANSPOSE", Some(1)), 86 => ("TYPE", Some(1)), 97 => ("ATAN2", Some(2)), 98 => ("ASIN", Some(1)),
        99 => ("ACOS", Some(1)), 100 => ("CHOOSE", var), 101 => ("HLOOKUP", var), 102 => ("VLOOKUP", var), 105 => ("ISREF", Some(1)), 109 => ("LOG", var),
        111 => ("CHAR", Some(1)), 112 => ("LOWER", Some(1)), 113 => ("UPPER", Some(1)), 114 => ("PROPER", Some(1)), 115 => ("LEFT", var), 116 => ("RIGHT", var),
        117 => ("EXACT", Some(2)), 118 => ("TRIM", Some(1)), 119 => ("REPLACE", Some(4)), 120 => ("SUBSTITUTE", var), 121 => ("CODE", Some(1)), 124 => ("FIND", var),
        125 => ("CELL", var), 126 => ("ISERR", Some(1)), 127 => ("ISTEXT", Some(1)), 128 => ("ISNUMBER", Some(1)), 129 => ("ISBLANK", Some(1)), 130 => ("T", Some(1)),
        131 => ("N", Some(1)), 140 => ("DATEVALUE", Some(1)), 141 => ("TIMEVALUE", Some(1)), 142 => ("SLN", Some(3)), 143 => ("SYD", Some(4)), 144 => ("DDB", var),
        148 => ("INDIRECT", var), 162 => ("CLEAN", Some(1)), 163 => ("MDETERM", Some(1)), 164 => ("MINVERSE", Some(1)), 165 => ("MMULT", Some(2)), 167 => ("IPMT", var),
        168 => ("PPMT", var), 169 => ("COUNTA", var), 183 => ("PRODUCT", var), 184 => ("FACT", Some(1)), 189 => ("DPRODUCT", Some(3)), 190 => ("ISNONTEXT", Some(1)),
        193 => ("STDEVP", var), 194 => ("VARP", var), 195 => ("DSTDEVP", Some(3)), 196 => ("DVARP", Some(3)), 197 => ("TRUNC", var), 198 => ("ISLOGICAL", Some(1)),
        199 => ("DCOUNTA", Some(3)), 212 => ("ROUNDUP", Some(2)), 213 => ("ROUNDDOWN", Some(2)), 216 => ("RANK", var), 219 => ("ADDRESS", var), 220 => ("DAYS360", var),
        221 => ("TODAY", Some(0)), 222 => ("VDB", var), 227 => ("MEDIAN", var), 228 => ("SUMPRODUCT", var), 229 => ("SINH", Some(1)), 230 => ("COSH", Some(1)),
        231 => ("TANH", Some(1)), 232 => ("ASINH", Some(1)), 233 => ("ACOSH", Some(1)), 234 => ("ATANH", Some(1)), 235 => ("DGET", Some(3)), 244 => ("INFO", Some(1)),
        247 => ("DB", var), 252 => ("FREQUENCY", Some(2)), 261 => ("ERROR.TYPE", Some(1)), 269 => ("AVEDEV", var), 276 => ("COMBIN", Some(2)), 279 => ("EVEN", Some(1)),
        285 => ("FLOOR", Some(2)), 288 => ("CEILING", Some(2)), 298 => ("ODD", Some(1)), 299 => ("PERMUT", Some(2)), 307 => ("CORREL", Some(2)), 311 => ("INTERCEPT", Some(2)),
        313 => ("RSQ", Some(2)), 315 => ("SLOPE", Some(2)), 325 => ("LARGE", Some(2)), 326 => ("SMALL", Some(2)), 327 => ("QUARTILE", Some(2)), 328 => ("PERCENTILE", Some(2)),
        330 => ("MODE", var), 336 => ("CONCATENATE", var), 337 => ("POWER", Some(2)), 342 => ("RADIANS", Some(1)), 343 => ("DEGREES", Some(1)), 344 => ("SUBTOTAL", var),
        345 => ("SUMIF", var), 346 => ("COUNTIF", Some(2)), 347 => ("COUNTBLANK", Some(1)), 359 => ("HYPERLINK", var), 361 => ("AVERAGEA", var), 362 => ("MAXA", var),
        363 => ("MINA", var), 364 => ("STDEVPA", var), 365 => ("VARPA", var), 366 => ("STDEVA", var), 367 => ("VARA", var),
        _ => return None,
    })
}

/// A column number (0-based) as its letters.
pub(crate) fn column_letters(mut c: u32) -> String {
    let mut s = String::new();
    c += 1;
    while c > 0 {
        let r = (c - 1) % 26;
        s.insert(0, (b'A' + r as u8) as char);
        c = (c - 1) / 26;
    }
    s
}

/// A sheet's name as a formula writes it before `!`.
pub(crate) fn sheet_prefix(name: &str) -> String {
    let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') && !name.chars().next().is_some_and(|c| c.is_ascii_digit()) && ibis_calc::formula::parse(&format!("{name}1")).is_ok_and(|e| !matches!(e, ibis_calc::formula::Expr::Ref(_)));
    if plain { format!("{name}!") } else { format!("'{}'!", name.replace('\'', "''")) }
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn cell_text(row: i64, col: i64, row_abs: bool, col_abs: bool) -> String {
    format!("{}{}{}{}", if col_abs { "$" } else { "" }, column_letters(col.max(0) as u32), if row_abs { "$" } else { "" }, row + 1)
}

/// The formula `rgce`, for the cell at `base` (row, column) (where a
/// relative token, in a shared formula, is counted from).
pub(crate) fn decompile(rgce: &[u8], base: (u32, u32), cx: &Context) -> Option<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    let bin = |stack: &mut Vec<String>, op: &str| -> Option<()> {
        let b = stack.pop()?;
        let a = stack.pop()?;
        stack.push(format!("{a}{op}{b}"));
        Some(())
    };
    // A reference: `rel` is whether the token counts from the cell (shared formulas, `…N` tokens).
    let reference = |row: u16, col: u16, relative: bool| -> String {
        let (row_rel, col_rel) = (col & 0x8000 != 0, col & 0x4000 != 0);
        let c = (col & 0x3FFF) as i64;
        if relative {
            let r = if row_rel { base.0 as i64 + row as i16 as i64 } else { row as i64 };
            let c = if col_rel { base.1 as i64 + (c as u8 as i8) as i64 } else { c };
            // Wrapping in the 65 536 rows and 256 columns of the old sheets.
            cell_text(r.rem_euclid(65_536), c.rem_euclid(256), !row_rel, !col_rel)
        } else {
            cell_text(row as i64, c, !row_rel, !col_rel)
        }
    };
    let sheets_of = |ixti: u16| -> Option<String> {
        let &(book, first, last) = cx.xti.get(ixti as usize)?;
        if !cx.internal_book.get(book as usize).copied().unwrap_or(false) {
            return None;
        }
        let name = |s: u16| cx.sheets.get(s as usize).cloned();
        let (a, b) = (name(first)?, name(last)?);
        Some(if first == last { sheet_prefix(&a) } else { format!("{}:{}", sheet_prefix(&a).trim_end_matches('!'), b) + "!" })
    };
    while i < rgce.len() {
        let t = rgce[i];
        i += 1;
        // The class bits (reference, value, array) do not change the text.
        let base_t = if t >= 0x20 { (t & 0x1F) | 0x20 } else { t };
        match t {
            0x01 => return None, // ptgExp: the caller resolves shared and array formulas
            0x03 => bin(&mut stack, "+")?,
            0x04 => bin(&mut stack, "-")?,
            0x05 => bin(&mut stack, "*")?,
            0x06 => bin(&mut stack, "/")?,
            0x07 => bin(&mut stack, "^")?,
            0x08 => bin(&mut stack, "&")?,
            0x09 => bin(&mut stack, "<")?,
            0x0A => bin(&mut stack, "<=")?,
            0x0B => bin(&mut stack, "=")?,
            0x0C => bin(&mut stack, ">=")?,
            0x0D => bin(&mut stack, ">")?,
            0x0E => bin(&mut stack, "<>")?,
            0x0F => bin(&mut stack, " ")?,
            0x10 => bin(&mut stack, ",")?,
            0x11 => bin(&mut stack, ":")?,
            0x12 => {
                let a = stack.pop()?;
                stack.push(format!("+{a}"));
            }
            0x13 => {
                let a = stack.pop()?;
                stack.push(format!("-{a}"));
            }
            0x14 => {
                let a = stack.pop()?;
                stack.push(format!("{a}%"));
            }
            0x15 => {
                let a = stack.pop()?;
                stack.push(format!("({a})"));
            }
            0x16 => stack.push(String::new()), // a missing argument
            0x17 => {
                let n = *rgce.get(i)? as usize;
                let wide = rgce.get(i + 1)? & 1 == 1;
                i += 2;
                let text = if wide {
                    let units: Vec<u16> = (0..n).map(|k| u16_at(rgce, i + k * 2)).collect::<Option<_>>()?;
                    i += n * 2;
                    String::from_utf16_lossy(&units)
                } else {
                    let s: String = rgce.get(i..i + n)?.iter().map(|&b| b as char).collect();
                    i += n;
                    s
                };
                stack.push(format!("\"{}\"", text.replace('"', "\"\"")));
            }
            0x19 => {
                // ptgAttr: only the ones that matter for the text.
                let flags = *rgce.get(i)?;
                let data = u16_at(rgce, i + 1)?;
                i += 3;
                if flags & 0x04 != 0 {
                    // ptgAttrChoose: a jump table follows.
                    i += (data as usize + 1) * 2;
                } else if flags & 0x10 != 0 {
                    let a = stack.pop()?;
                    stack.push(format!("SUM({a})"));
                }
            }
            0x1C => {
                let e = match *rgce.get(i)? {
                    0x00 => "#NULL!",
                    0x07 => "#DIV/0!",
                    0x0F => "#VALUE!",
                    0x17 => "#REF!",
                    0x1D => "#NAME?",
                    0x24 => "#NUM!",
                    0x2A => "#N/A",
                    _ => return None,
                };
                i += 1;
                stack.push(e.to_string());
            }
            0x1D => {
                stack.push(if *rgce.get(i)? != 0 { "TRUE" } else { "FALSE" }.to_string());
                i += 1;
            }
            0x1E => {
                stack.push(u16_at(rgce, i)?.to_string());
                i += 2;
            }
            0x1F => {
                let v = f64::from_le_bytes(rgce.get(i..i + 8)?.try_into().ok()?);
                stack.push(ibis_calc::number_to_xml(v));
                i += 8;
            }
            _ => match base_t {
                0x21 => {
                    let (name, n) = function(u16_at(rgce, i)?)?;
                    i += 2;
                    let n = n?;
                    if stack.len() < n {
                        return None;
                    }
                    let args = stack.split_off(stack.len() - n);
                    stack.push(format!("{name}({})", args.join(",")));
                }
                0x22 => {
                    let count = (*rgce.get(i)? & 0x7F) as usize;
                    let id = u16_at(rgce, i + 1)? & 0x7FFF;
                    i += 3;
                    let (name, _) = function(id)?;
                    if stack.len() < count {
                        return None;
                    }
                    let args = stack.split_off(stack.len() - count);
                    stack.push(format!("{name}({})", args.join(",")));
                }
                0x23 => {
                    let index = u16_at(rgce, i)? as usize;
                    i += 4;
                    stack.push(cx.names.get(index.checked_sub(1)?)?.clone());
                }
                0x24 => {
                    stack.push(reference(u16_at(rgce, i)?, u16_at(rgce, i + 2)?, false));
                    i += 4;
                }
                0x25 => {
                    let (r1, r2, c1, c2) = (u16_at(rgce, i)?, u16_at(rgce, i + 2)?, u16_at(rgce, i + 4)?, u16_at(rgce, i + 6)?);
                    // The columns' flags are in each corner's column field; the rows' too.
                    stack.push(area_text(r1, r2, c1, c2, base));
                    i += 8;
                }
                // ptgMemFunc: the size of the sub-expression whose tokens follow (they carry the text).
                0x29 => i += 2,
                0x26..=0x28 => return None,
                0x2A => {
                    stack.push("#REF!".to_string());
                    i += 4;
                }
                0x2B => {
                    stack.push("#REF!".to_string());
                    i += 8;
                }
                0x2C => {
                    stack.push(reference(u16_at(rgce, i)?, u16_at(rgce, i + 2)?, true));
                    i += 4;
                }
                0x2D => {
                    let (r1, r2, c1, c2) = (u16_at(rgce, i)?, u16_at(rgce, i + 2)?, u16_at(rgce, i + 4)?, u16_at(rgce, i + 6)?);
                    stack.push(format!("{}:{}", area_corner(r1, c1, true, base), area_corner(r2, c2, true, base)));
                    i += 8;
                }
                0x3A => {
                    let prefix = sheets_of(u16_at(rgce, i)?)?;
                    stack.push(format!("{prefix}{}", reference(u16_at(rgce, i + 2)?, u16_at(rgce, i + 4)?, false)));
                    i += 6;
                }
                0x3B => {
                    let prefix = sheets_of(u16_at(rgce, i)?)?;
                    let (r1, r2, c1, c2) = (u16_at(rgce, i + 2)?, u16_at(rgce, i + 4)?, u16_at(rgce, i + 6)?, u16_at(rgce, i + 8)?);
                    stack.push(format!("{prefix}{}", area_text(r1, r2, c1, c2, base)));
                    i += 10;
                }
                // ptgRefErr3d, ptgAreaErr3d: a reference of a sheet that was deleted, still of that sheet ('Sheet'!#REF!, as
                // Excel writes it: a print area so is of its sheet, corpus 076).
                0x3C | 0x3D => {
                    let prefix = u16_at(rgce, i).and_then(sheets_of).unwrap_or_default();
                    stack.push(format!("{prefix}#REF!"));
                    i += if base_t == 0x3C { 6 } else { 10 };
                }
                _ => return None,
            },
        }
    }
    if stack.len() == 1 { stack.pop() } else { None }
}

/// An area token's text. An area of every row of the sheet (1 to 65536, all a BIFF8 sheet has) is whole columns
/// (`$A:$X`), and one of every column (A to IV) whole rows (`$4:$4`), as Excel reads them: a workbook has more
/// rows and columns than an old one, and the area is the whole of them (a print area of whole columns, corpus 161).
fn area_text(r1: u16, r2: u16, c1: u16, c2: u16, base: (u32, u32)) -> String {
    let abs = |field: u16, bit: u16| if field & bit == 0 { "$" } else { "" };
    if r1 == 0 && r2 == 0xFFFF {
        return format!("{}{}:{}{}", abs(c1, 0x4000), column_letters((c1 & 0x3FFF) as u32), abs(c2, 0x4000), column_letters((c2 & 0x3FFF) as u32));
    }
    if c1 & 0x3FFF == 0 && c2 & 0x3FFF == 0xFF {
        return format!("{}{}:{}{}", abs(c1, 0x8000), r1 as u32 + 1, abs(c2, 0x8000), r2 as u32 + 1);
    }
    format!("{}:{}", area_corner(r1, c1, false, base), area_corner(r2, c2, false, base))
}

/// One corner of an area token: its row and its column field.
fn area_corner(row: u16, col: u16, relative: bool, base: (u32, u32)) -> String {
    let (row_rel, col_rel) = (col & 0x8000 != 0, col & 0x4000 != 0);
    let c = (col & 0x3FFF) as i64;
    if relative {
        let r = if row_rel { base.0 as i64 + row as i16 as i64 } else { row as i64 };
        let c = if col_rel { base.1 as i64 + (c as u8 as i8) as i64 } else { c };
        cell_text(r.rem_euclid(65_536), c.rem_euclid(256), !row_rel, !col_rel)
    } else {
        cell_text(row as i64, c, !row_rel, !col_rel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: u16) -> Vec<u8> {
        let mut v = vec![0x1E];
        v.extend(n.to_le_bytes());
        v
    }

    #[test]
    fn tokens_become_the_formula() {
        let cx = Context { sheets: vec!["Sheet1".into(), "売上 表".into()], xti: vec![(0, 1, 1)], internal_book: vec![true], names: vec![] };
        // 1+2*3
        let mut f = num(1);
        f.extend(num(2));
        f.extend(num(3));
        f.extend([0x05, 0x03]);
        assert_eq!(decompile(&f, (0, 0), &cx).as_deref(), Some("1+2*3"));
        // SUM(A1:B2) as ptgArea (rows 0..1, columns A..B, both relative) then ptgFuncVar SUM
        let mut f = vec![0x25];
        f.extend([0, 0, 1, 0, 0, 0xC0, 1, 0xC0]);
        f.extend([0x22, 1, 4, 0]);
        assert_eq!(decompile(&f, (5, 5), &cx).as_deref(), Some("SUM(A1:B2)"));
        // '売上 表'!$C$3 and (the first corner of) IF(A1>0,"x","y") with ptgAttr tokens between.
        let mut f = vec![0x3A, 0, 0];
        f.extend([2, 0, 2, 0]);
        assert_eq!(decompile(&f, (0, 0), &cx).as_deref(), Some("'売上 表'!$C$3"));
        let mut f = vec![0x44, 0, 0, 0, 0xC0];
        f.extend(num(0));
        f.push(0x0D);
        f.extend([0x19, 0x02, 0, 0]);
        f.extend([0x17, 1, 0, b'x']);
        f.extend([0x19, 0x08, 0, 0]);
        f.extend([0x17, 1, 0, b'y']);
        f.extend([0x22, 3, 1, 0]);
        assert_eq!(decompile(&f, (0, 0), &cx).as_deref(), Some("IF(A1>0,\"x\",\"y\")"));
        // A shared formula's relative reference: one row up, same column, from (4, 2).
        let f = vec![0x2C, 0xFF, 0xFF, 0x00, 0xC0];
        assert_eq!(decompile(&f, (4, 2), &cx).as_deref(), Some("C4"));
    }

    #[test]
    fn an_area_of_every_row_or_column_is_whole_columns_or_rows() {
        let cx = Context { sheets: vec!["SL".into()], xti: vec![(0, 0, 0)], internal_book: vec![true], names: vec![] };
        // 'SL'!$A$1:$X$65536 (a print area) is 'SL'!$A:$X (SL1 is a cell, so the name is quoted); $A$4:$IV$4 (print titles) is $4:$4.
        let f = vec![0x3B, 0, 0, 0, 0, 0xFF, 0xFF, 0, 0, 23, 0];
        assert_eq!(decompile(&f, (0, 0), &cx).as_deref(), Some("'SL'!$A:$X"));
        let f = vec![0x25, 3, 0, 3, 0, 0, 0, 0xFF, 0];
        assert_eq!(decompile(&f, (0, 0), &cx).as_deref(), Some("$4:$4"));
        // ptgAreaErr3d of the sheet: 'SL'!#REF!.
        let f = vec![0x3D, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(decompile(&f, (0, 0), &cx).as_deref(), Some("'SL'!#REF!"));
    }
}
