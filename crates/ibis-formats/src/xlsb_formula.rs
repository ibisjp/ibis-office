// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A formula of a binary workbook (`.xlsb`, BIFF12) as text: its tokens
//! (`rgce`, MS-XLSB §2.5.98) read back into the formula a person typed.
//! Like the old `.xls` ones (see `xls_formula`) but with 32-bit rows, 16-bit
//! strings and the extra data (`rgcb`) after the tokens. A token this does not
//! know makes the whole formula unreadable (`None`).

use crate::xls_formula::{Context, column_letters, function, sheet_prefix};

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

const ROWS: i64 = 1_048_576;
const COLS: i64 = 16_384;

fn cell_text(row: i64, col: i64, row_abs: bool, col_abs: bool) -> String {
    format!("{}{}{}{}", if col_abs { "$" } else { "" }, column_letters(col.max(0) as u32), if row_abs { "$" } else { "" }, row + 1)
}

/// One cell of a reference token: `row` and the column field (14 bits of column, bit 14 relative column, bit 15 relative row).
/// With `relative` (the `…N` tokens of a shared formula) the relative parts are offsets from `base`.
fn corner(row: u32, col: u16, relative: bool, base: (u32, u32)) -> String {
    let (row_rel, col_rel) = (col & 0x8000 != 0, col & 0x4000 != 0);
    let c = (col & 0x3FFF) as i64;
    if relative {
        let r = if row_rel { (base.0 as i64 + row as i32 as i64).rem_euclid(ROWS) } else { row as i64 };
        let c = if col_rel {
            // A 14-bit signed offset.
            let off = if c >= COLS / 2 { c - COLS } else { c };
            (base.1 as i64 + off).rem_euclid(COLS)
        } else {
            c
        };
        cell_text(r, c, !row_rel, !col_rel)
    } else {
        cell_text(row as i64, c, !row_rel, !col_rel)
    }
}

/// The formula `rgce` (with its extra data `rgcb`), for the cell at `base` (row, column).
pub(crate) fn decompile(rgce: &[u8], rgcb: &[u8], base: (u32, u32), cx: &Context) -> Option<String> {
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    let mut extra = 0usize;
    let bin = |stack: &mut Vec<String>, op: &str| -> Option<()> {
        let b = stack.pop()?;
        let a = stack.pop()?;
        stack.push(format!("{a}{op}{b}"));
        Some(())
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
            0x16 => stack.push(String::new()),
            0x17 => {
                let n = u16_at(rgce, i)? as usize;
                i += 2;
                let units: Vec<u16> = (0..n).map(|k| u16_at(rgce, i + k * 2)).collect::<Option<_>>()?;
                i += n * 2;
                stack.push(format!("\"{}\"", String::from_utf16_lossy(&units).replace('"', "\"\"")));
            }
            0x19 => {
                let flags = *rgce.get(i)?;
                let data = u16_at(rgce, i + 1)?;
                i += 3;
                if flags & 0x04 != 0 {
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
                // ptgArray: 14 unused bytes; the values are in the extra data.
                0x20 => {
                    i += 14;
                    let rows = u32_at(rgcb, extra)? as usize;
                    let cols = u32_at(rgcb, extra + 4)? as usize;
                    extra += 8;
                    if rows == 0 || cols == 0 || rows * cols > 100_000 {
                        return None;
                    }
                    let mut lines: Vec<String> = Vec::new();
                    for _ in 0..rows {
                        let mut cells: Vec<String> = Vec::new();
                        for _ in 0..cols {
                            match *rgcb.get(extra)? {
                                0 => {
                                    let v = f64::from_le_bytes(rgcb.get(extra + 1..extra + 9)?.try_into().ok()?);
                                    cells.push(ibis_calc::number_to_xml(v));
                                    extra += 9;
                                }
                                1 => {
                                    let n = u16_at(rgcb, extra + 1)? as usize;
                                    let units: Vec<u16> = (0..n).map(|k| u16_at(rgcb, extra + 3 + k * 2)).collect::<Option<_>>()?;
                                    cells.push(format!("\"{}\"", String::from_utf16_lossy(&units).replace('"', "\"\"")));
                                    extra += 3 + n * 2;
                                }
                                2 => {
                                    cells.push(if *rgcb.get(extra + 1)? != 0 { "TRUE" } else { "FALSE" }.to_string());
                                    extra += 2;
                                }
                                4 => {
                                    cells.push(
                                        match *rgcb.get(extra + 1)? {
                                            0x00 => "#NULL!",
                                            0x07 => "#DIV/0!",
                                            0x0F => "#VALUE!",
                                            0x17 => "#REF!",
                                            0x1D => "#NAME?",
                                            0x24 => "#NUM!",
                                            0x2A => "#N/A",
                                            _ => return None,
                                        }
                                        .to_string(),
                                    );
                                    extra += 5;
                                }
                                _ => return None,
                            }
                        }
                        lines.push(cells.join(","));
                    }
                    stack.push(format!("{{{}}}", lines.join(";")));
                }
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
                    let count = *rgce.get(i)? as usize;
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
                    let index = u32_at(rgce, i)? as usize;
                    i += 4;
                    stack.push(cx.names.get(index.checked_sub(1)?)?.clone());
                }
                0x24 | 0x2C => {
                    stack.push(corner(u32_at(rgce, i)?, u16_at(rgce, i + 4)?, base_t == 0x2C, base));
                    i += 6;
                }
                0x25 | 0x2D => {
                    let (r1, r2, c1, c2) = (u32_at(rgce, i)?, u32_at(rgce, i + 4)?, u16_at(rgce, i + 8)?, u16_at(rgce, i + 10)?);
                    let rel = base_t == 0x2D;
                    stack.push(format!("{}:{}", corner(r1, c1, rel, base), corner(r2, c2, rel, base)));
                    i += 12;
                }
                // ptgMemArea: the tokens of the sub-expression follow and carry the text; the extra data lists its ranges.
                0x26 => {
                    i += 6;
                    let count = u32_at(rgcb, extra)? as usize;
                    extra += 4 + count * 16;
                }
                0x29 => i += 2,
                0x27 | 0x28 => return None,
                0x2A => {
                    stack.push("#REF!".to_string());
                    i += 6;
                }
                0x2B => {
                    stack.push("#REF!".to_string());
                    i += 12;
                }
                0x3A => {
                    let prefix = sheets_of(u16_at(rgce, i)?)?;
                    stack.push(format!("{prefix}{}", corner(u32_at(rgce, i + 2)?, u16_at(rgce, i + 6)?, false, base)));
                    i += 8;
                }
                0x3B => {
                    let prefix = sheets_of(u16_at(rgce, i)?)?;
                    let (r1, r2, c1, c2) = (u32_at(rgce, i + 2)?, u32_at(rgce, i + 6)?, u16_at(rgce, i + 10)?, u16_at(rgce, i + 12)?);
                    stack.push(format!("{prefix}{}:{}", corner(r1, c1, false, base), corner(r2, c2, false, base)));
                    i += 14;
                }
                0x3C => {
                    stack.push("#REF!".to_string());
                    i += 8;
                }
                0x3D => {
                    stack.push("#REF!".to_string());
                    i += 14;
                }
                _ => return None,
            },
        }
    }
    if stack.len() == 1 { stack.pop() } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_become_the_formula() {
        let cx = Context { sheets: vec!["Sheet1".into(), "売上 表".into()], xti: vec![(0, 1, 1)], internal_book: vec![true], names: vec![] };
        // SUM(A1:B2) as ptgArea then ptgFuncVar SUM.
        let mut f = vec![0x25];
        f.extend([0, 0, 0, 0, 1, 0, 0, 0, 0, 0xC0, 1, 0xC0]);
        f.extend([0x22, 1, 4, 0]);
        assert_eq!(decompile(&f, &[], (5, 5), &cx).as_deref(), Some("SUM(A1:B2)"));
        // '売上 表'!$C$3
        let mut f = vec![0x3A, 0, 0];
        f.extend([2, 0, 0, 0, 2, 0]);
        assert_eq!(decompile(&f, &[], (0, 0), &cx).as_deref(), Some("'売上 表'!$C$3"));
        // One row up, same column, from (4, 2).
        let f = vec![0x2C, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0xC0];
        assert_eq!(decompile(&f, &[], (4, 2), &cx).as_deref(), Some("C4"));
        // A string.
        let f = vec![0x17, 2, 0, b'h', 0, b'"', 0];
        assert_eq!(decompile(&f, &[], (0, 0), &cx).as_deref(), Some("\"h\"\"\""));
    }
}
