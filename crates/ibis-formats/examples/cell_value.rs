// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Print the value ibisOffice calculates for every formula cell of a
//! workbook, for the coverage tests (scripts/coverage/compare.py compares it
//! with the value Excel calculated and saved):
//!
//!   cargo run --release -p ibis-formats --example cell_value -- <file.xlsx>
//!
//! Every formula of the workbook is calculated again (ibis_calc's
//! recalculation, each after the cells it refers to), not taken from the
//! values saved in the file. A dynamic array's result spills over the cells
//! next to it. One line per cell:
//! `sheet  A1  type  value  unsupported`, where type is n (number), str
//! (text), b (logical, 1 or 0), e (error) or empty, and unsupported is 1
//! when the formula used a function the engine does not have (the value is
//! then only a stand-in). Backslash, tab, CR and LF in a value are written
//! as \\, \t, \r, \n.

use ibis_calc::eval::Book;
use ibis_doc_model::sheet_calc::BookView;
use ibis_doc_model::{CellValue, cell_name};

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let code = ch as u32;
        if (code <= 0x1f && ch != '\t' && ch != '\r' && ch != '\n') || code == 0x7f {
            out.push_str(&format!("_x{:04x}_", code));
        } else {
            match ch {
                '\\' => out.push_str("\\\\"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                '\n' => out.push_str("\\n"),
                _ => out.push(ch),
            }
        }
    }
    out
}

fn typed(v: &CellValue) -> (&'static str, String) {
    match v {
        CellValue::Empty => ("empty", String::new()),
        CellValue::Number(n) => ("n", format!("{n}")),
        CellValue::Text(t) => ("str", escape(t)),
        CellValue::Bool(b) => ("b", if *b { "1" } else { "0" }.into()),
        CellValue::Error(e) => ("e", e.as_str().into()),
    }
}

fn main() {
    // Excel's random numbers cannot be compared: the stand-ins, marked as not calculated.
    ibis_calc::random::use_stand_ins(true);
    let path = std::env::args().nth(1).expect("usage: cell_value <file.xlsx> [file name the workbook is saved under]");
    // The name Excel saw the workbook under (the coverage tests' truth was made from a copy of another
    // name), else its own path: for CELL("filename") and the sheet names of CELL("address").
    ibis_calc::eval::convert::set_workbook_file(Some(std::env::args().nth(2).unwrap_or_else(|| std::fs::canonicalize(&path).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| path.clone()))));
    let data = std::fs::read(&path).expect("readable file");
    let presentation = ibis_formats::read_xlsx(&data).expect("a workbook");
    let book = BookView::new(&presentation);
    let formulas = book.formula_cells();
    let mut out: std::collections::BTreeMap<(usize, u32, u32), (CellValue, bool)> = std::collections::BTreeMap::new();
    for (sheet_idx, s) in presentation.slides.iter().filter_map(ibis_doc_model::sheet_calc::sheet_of_slide).enumerate() {
        for c in &s.cells {
            if !c.value.is_empty() {
                out.insert((sheet_idx, c.row, c.column), (c.value.clone(), false));
            }
        }
    }
    for r in book.recalculate(&formulas) {
        let (sheet, row, column) = r.cell;
        if let Some((rows, columns, values)) = &r.spill {
            let mut blocked = false;
            for dr in 0..*rows as u32 {
                for dc in 0..*columns as u32 {
                    if (dr, dc) == (0, 0) {
                        continue;
                    }
                    let tr = row + dr;
                    let tc = column + dc;
                    let in_old = book.spill_area(sheet, row, column).is_some_and(|(r0, c0, r1, c1)| tr >= r0 && tr <= r1 && tc >= c0 && tc <= c1);
                    if !in_old && (!book.value(sheet, tr, tc).is_empty() || book.formula(sheet, tr, tc).is_some()) {
                        blocked = true;
                        break;
                    }
                }
                if blocked {
                    break;
                }
            }
            if blocked {
                out.insert(r.cell, (CellValue::Error(ibis_doc_model::CellError::Value), r.unsupported));
            } else {
                for (i, v) in values.iter().enumerate() {
                    let (dr, dc) = ((i / columns) as u32, (i % columns) as u32);
                    if (i / columns) < *rows {
                        out.insert((sheet, row + dr, column + dc), (v.clone(), r.unsupported));
                    }
                }
            }
        } else {
            out.insert(r.cell, (r.value.clone(), r.unsupported));
        }
    }
    for ((sheet, row, column), (value, unsupported)) in out {
        let (t, v) = typed(&value);
        println!("{}\t{}\t{t}\t{v}\t{}", book.sheet_name(sheet), cell_name(column, row), u8::from(unsupported));
    }
}
