// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Print every worksheet cell's display text as ibisOffice reads it, for the
//! coverage tests (scripts/coverage/compare.py compares it with Excel's):
//!
//!   cargo run --release -p ibis-formats --example cell_text -- <file.xlsx>
//!
//! One line per cell: `sheet  A1  text`, with backslash, tab, CR and LF
//! written as \\, \t, \r, \n.

use ibis_doc_model::{ShapeKind, cell_name};

fn main() {
    let path = std::env::args().nth(1).expect("usage: cell_text <file.xlsx>");
    let data = std::fs::read(&path).expect("readable file");
    let p = ibis_formats::read_xlsx(&data).expect("a workbook");
    for slide in &p.slides {
        let name = slide.name.clone().unwrap_or_default();
        for shape in &slide.shapes {
            let ShapeKind::Sheet(sheet) = &shape.kind else { continue };
            for c in &sheet.cells {
                let text = c.text.replace('\\', "\\\\").replace('\t', "\\t").replace('\r', "\\r").replace('\n', "\\n");
                println!("{name}\t{}\t{text}", cell_name(c.column, c.row));
            }
        }
    }
}
