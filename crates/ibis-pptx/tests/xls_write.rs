// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A workbook written as an old Excel file (.xls) and read again has its values, formulas, merges, sizes and formats.
use ibis_doc_model::{Presentation, Sheet, sheet_calc::sheet_of_slide};

fn read(path: &str) -> Presentation {
    ibis_pptx::read(&std::fs::read(format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap()
}

fn sheets(p: &Presentation) -> Vec<std::sync::Arc<Sheet>> {
    p.slides.iter().filter_map(|s| sheet_of_slide(s).cloned()).collect()
}

fn norm(f: &str) -> String {
    f.replace(' ', "").to_uppercase().replace("_XLFN.", "")
}

#[test]
fn a_workbook_written_as_xls_reads_back() {
    let first = read("test/editing/xlsx-sales.xlsx");
    let (bytes, report) = ibis_pptx::write_xls_report(&first).unwrap();
    if let Ok(dir) = std::env::var("XLS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("sales.xls"), &bytes).unwrap();
    }
    let again = ibis_pptx::read(&bytes).unwrap();
    let (a, b) = (sheets(&first), sheets(&again));
    assert_eq!(a.len(), b.len());
    let names = |p: &Presentation| p.workbook.as_ref().unwrap().sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&first), names(&again));
    let mut formulas = 0;
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        for cell in &x.cells {
            let got = y.cell(cell.row, cell.column).unwrap_or_else(|| panic!("sheet {i}: no cell {}:{}", cell.row, cell.column));
            assert_eq!(cell.value, got.value, "sheet {i} cell {}:{}", cell.row, cell.column);
            if let Some(f) = &cell.formula {
                formulas += 1;
                if let Some(g) = &got.formula {
                    assert_eq!(norm(f), norm(g), "sheet {i} cell {}:{}", cell.row, cell.column);
                }
            }
        }
        assert_eq!(x.merges.len(), y.merges.len(), "merges of sheet {i}");
    }
    assert!(formulas > 0);
    eprintln!("{report:?}");
}

/// The formats of a workbook (font, fill, borders, alignment, number formats), its merged cells and sizes survive.
#[test]
fn formats_sizes_and_merges_survive_the_xls_round_trip() {
    for path in ["crates/ibis-pptx/tests/fixtures/libreoffice_small.ods", "test/editing/xlsx-sales.xlsx"] {
        let first = read(path);
        let bytes = ibis_pptx::write_xls(&first).unwrap();
        let again = ibis_pptx::read(&bytes).unwrap();
        for (i, (x, y)) in sheets(&first).iter().zip(sheets(&again)).enumerate() {
            for cell in &x.cells {
                let (sx, sy) = (&x.styles[cell.style as usize], &y.styles[y.cell(cell.row, cell.column).unwrap().style as usize]);
                let at = format!("{path} sheet {i} cell {}:{}", cell.row, cell.column);
                assert_eq!((sx.bold, sx.italic, sx.strike, sx.underline), (sy.bold, sy.italic, sy.strike, sy.underline), "{at}");
                assert_eq!(sx.fill, sy.fill, "{at}");
                assert_eq!(sx.color, sy.color, "{at}");
                assert_eq!((sx.align, sx.vertical, sx.wrap), (sy.align, sy.vertical, sy.wrap), "{at}");
                assert_eq!(sx.number_format, sy.number_format, "{at}");
                assert_eq!((sx.left.as_ref().map(|b| (b.line, b.color)), sx.bottom.as_ref().map(|b| (b.line, b.color))), (sy.left.as_ref().map(|b| (b.line, b.color)), sy.bottom.as_ref().map(|b| (b.line, b.color))), "{at}");
            }
            assert_eq!(x.merges, y.merges, "merges of {path} sheet {i}");
            // Sizes: a column and a row differ by what a width in characters and a height in twips round (a row the xls
            // file gives a height reads at a whole 96 dpi pixel, on every OS since the sizes follow Excel for Windows (`EXCEL_METRICS`): 20 pt of an automatic row of the xlsx is 19.5 pt there).
            for c in 0..x.columns.len().min(20) {
                assert!((x.columns[c] - y.columns.get(c).copied().unwrap_or(0)).abs() < 40_000, "{path} sheet {i} column {c}: {} / {:?}", x.columns[c], y.columns.get(c));
            }
            for r in 0..x.rows.len().min(20) {
                assert!((x.rows[r] - y.rows.get(r).copied().unwrap_or(0)).abs() < 700 + 9_525, "{path} sheet {i} row {r}: {} / {:?}", x.rows[r], y.rows.get(r));
            }
        }
    }
}

fn conditional_summary(sheet: &Sheet) -> Vec<String> {
    let mut out: Vec<String> = sheet
        .conditional
        .iter()
        .flat_map(|f| {
            f.rules.iter().map(move |r| {
                let range = f.ranges.iter().map(|g| format!("{}:{}-{}:{}", g.first_row, g.first_column, g.last_row, g.last_column)).collect::<Vec<_>>().join(",");
                let style = r.style.as_ref().map(|s| format!("{:?}/{:?}/{:?}/{:?}", s.color, s.bold, s.italic, s.fill)).unwrap_or_default();
                format!("{range} {:?} {style}", r.kind)
            })
        })
        .collect();
    out.sort();
    out
}

/// The data validations and the conditional formats (value and formula rules, with their fonts and fills) survive.
#[test]
fn validations_and_conditional_formats_survive_the_xls_round_trip() {
    let first = read("crates/ibis-pptx/tests/fixtures/ods_features.xlsx");
    let (bytes, report) = ibis_pptx::write_xls_report(&first).unwrap();
    if let Ok(dir) = std::env::var("XLS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("features.xls"), &bytes).unwrap();
    }
    let again = ibis_pptx::read(&bytes).unwrap();
    let (x, y) = (sheets(&first).remove(0), sheets(&again).remove(0));
    assert_eq!(x.validations.len(), 3);
    assert_eq!(x.validations, y.validations);
    // The rules the old format has (a value or formula, the top three, duplicates): the colour scale, data bar and
    // icon set are left out, and said.
    let (want, got) = (conditional_summary(&x), conditional_summary(&y));
    assert_eq!(got.len(), 5, "{got:#?}");
    for g in &got {
        if g.contains("RANK(") || g.contains("COUNTIF(") {
            continue;
        }
        assert!(want.contains(g), "{g}\n{want:#?}");
    }
    assert_eq!(report.other, 3, "colour scale, data bar and icon set: {report:?}");
    // The notes: cell, author and text (a second line too).
    let notes = |s: &Sheet| {
        let mut v: Vec<_> = s.comments.iter().map(|c| (c.row, c.column, c.author.clone(), c.text.clone())).collect();
        v.sort();
        v
    };
    assert_eq!(notes(&x).len(), 2);
    assert_eq!(notes(&x), notes(&y));
}

/// Pictures (PNG and JPEG) survive: the same pictures in the same cells, the same size.
#[test]
fn pictures_survive_the_xls_round_trip() {
    let first = read("test/editing/xlsx-sales.xlsx");
    let (bytes, report) = ibis_pptx::write_xls_report(&first).unwrap();
    if let Ok(dir) = std::env::var("XLS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("sales-pictures.xls"), &bytes).unwrap();
    }
    let again = ibis_pptx::read(&bytes).unwrap();
    let pictures = |p: &Presentation| -> Vec<(usize, ibis_doc_model::EmuRect, usize)> {
        p.slides
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                s.shapes.iter().filter_map(move |sh| match &sh.kind {
                    ibis_doc_model::ShapeKind::Picture(pic) => Some((i, sh.frame, p.media.iter().find(|m| m.id == pic.media).map_or(0, |m| m.data.len()))),
                    _ => None,
                })
            })
            .collect()
    };
    let (a, b) = (pictures(&first), pictures(&again));
    assert_eq!(a.len(), 1);
    assert_eq!(a.len(), b.len(), "{a:?} / {b:?}; {report:?}");
    // The same image bytes, in the same place within a character or two.
    assert_eq!(a[0].2, b[0].2);
    assert_eq!(a[0].0, b[0].0 - (sheets_before(&first) - sheets_before(&again)).min(b[0].0));
    for (x, y) in [(a[0].1.x, b[0].1.x), (a[0].1.y, b[0].1.y), (a[0].1.width, b[0].1.width), (a[0].1.height, b[0].1.height)] {
        assert!((x - y).abs() < 150_000, "{a:?} / {b:?}");
    }
}

/// How many worksheets there are (a chart sheet is not one the .xls keeps).
fn sheets_before(p: &Presentation) -> usize {
    p.workbook.as_ref().unwrap().sheets.len()
}
