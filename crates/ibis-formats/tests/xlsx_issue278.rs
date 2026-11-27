// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #278 (miz9): `isTEST2.xlsx` tries 13 IS-type functions (and TYPE, LET with ISFORMULA, ISOMITTED, ...) on many arguments; `C3:V45` are the results of the
//! formulas in `AC3:AV45` after F9, and `BC3:BV45` are Excel's raw values (Microsoft 365, build 16.0 MSO 2608). The attachment is `test/github-issue/278/isTEST2.xlsx` (not in the
//! repository: the test passes at once without it).

use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::{CellValue, Command, Document, SheetCommand};

fn show(v: &CellValue) -> String {
    match v {
        CellValue::Empty => String::new(),
        CellValue::Number(n) => format!("{n}"),
        CellValue::Text(t) => t.clone(),
        CellValue::Bool(b) => if *b { "1".into() } else { "0".into() },
        CellValue::Error(e) => format!("{e:?}"),
    }
}

#[test]
fn is_test2_with_rows_46_47_through_if_is_excels() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/github-issue/278/isTEST2.xlsx");
    let Ok(data) = std::fs::read(&path) else { return };
    let opened = ibis_formats::open(data).unwrap();
    let mut d = Document::new(opened.presentation);
    d.execute(vec![Command::Sheet(SheetCommand::CalculateNow)]).unwrap();
    let p = d.presentation();
    let slide = p.slides[0].id;
    let sheet = sheet_of_slide(p.slide(slide).unwrap()).unwrap();
    let get = |row: u32, column: u32| sheet.cell(row, column).map(|c| c.value.clone()).unwrap_or(CellValue::Empty);
    let mut diffs = Vec::new();
    // C = column 2 (0-based), V = 21; BC = 54, BV = 73; rows 3..=47 (0-based 2..=46); rows 46 and 47 were added for #278.
    for row in 2..=46u32 {
        for k in 0..20u32 {
            let ours = get(row, 2 + k);
            let excel = get(row, 54 + k);
            let (a, b) = (show(&ours), show(&excel));
            if a != b {
                diffs.push(format!("row {} col {}: ours {a:?}, Excel {b:?}", row + 1, k));
            }
        }
    }
    assert!(diffs.is_empty(), "{} cells differ:\n{}", diffs.len(), diffs.join("\n"));
}
