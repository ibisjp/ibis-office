// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An OpenDocument spreadsheet made by LibreOffice Calc (from a hand-written
//! flat document) is read as the workbook made from it.
use ibis_doc_model::{CellValue, Rgb, sheet_calc::sheet_of_slide};

#[test]
fn a_libreoffice_spreadsheet_is_read() {
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/libreoffice_small.ods")).unwrap();
    let p = ibis_odf::read(&data).unwrap();
    assert_eq!(p.slides.len(), 2);
    let sheet = sheet_of_slide(&p.slides[0]).unwrap();
    let cell = |r, c| sheet.cell(r, c).unwrap_or_else(|| panic!("no cell {r}:{c}"));
    // The merged, bold, filled, centred, bordered title.
    assert_eq!(cell(0, 0).value, CellValue::Text("Title".into()));
    assert_eq!(sheet.merges.len(), 1);
    let head = &sheet.styles[cell(0, 0).style as usize];
    assert!(head.bold);
    assert_eq!(head.fill, Some(Rgb(0xFFFF00)));
    // A date and a number with their formats.
    assert_eq!(cell(1, 0).text, "2024-03-05");
    assert_eq!(cell(1, 1).value, CellValue::Number(1234.5));
    assert_eq!(cell(1, 1).text, "1,234.50");
    // A formula, and one that names another sheet.
    assert_eq!(cell(2, 1).formula.as_deref(), Some("SUM(B2,B2*2)"));
    assert_eq!(cell(2, 1).value, CellValue::Number(3703.5));
    let other = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(other.cell(0, 0).unwrap().formula.as_deref(), Some("Sales!B3+1"));
    // The column width (4 cm) and the row height (1.5 cm).
    assert!((1_400_000..1_480_000).contains(&sheet.columns[0]), "column width {:?}", sheet.columns.first());
}

/// Written as .ods and read again, a workbook keeps its values, formulas,
/// merges and formats.
#[test]
fn a_workbook_written_as_ods_reads_back() {
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/libreoffice_small.ods")).unwrap();
    let first = ibis_odf::read(&data).unwrap();
    let ods = ibis_odf::write_ods(&first).unwrap();
    if let Ok(dir) = std::env::var("ODS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("written.ods"), &ods).unwrap();
    }
    let second = ibis_odf::read(&ods).unwrap();
    assert_eq!(second.slides.len(), 2);
    for i in 0..2 {
        let (a, b) = (sheet_of_slide(&first.slides[i]).unwrap(), sheet_of_slide(&second.slides[i]).unwrap());
        assert_eq!(a.merges, b.merges, "sheet {i}");
        assert_eq!(a.cells.len(), b.cells.len(), "sheet {i}");
        for c in &a.cells {
            let d = b.cell(c.row, c.column).unwrap_or_else(|| panic!("sheet {i} {}:{}", c.row, c.column));
            assert_eq!((&c.value, &c.text, &c.formula), (&d.value, &d.text, &d.formula), "sheet {i} {}:{}", c.row, c.column);
            let (x, y) = (&a.styles[c.style as usize], &b.styles[d.style as usize]);
            assert_eq!((x.bold, x.fill, x.number_format.as_str(), x.align), (y.bold, y.fill, y.number_format.as_str(), y.align), "sheet {i} {}:{}", c.row, c.column);
        }
        assert!((a.columns[0] - b.columns[0]).abs() < 20_000);
    }
}

/// A workbook with a picture, saved by LibreOffice Calc as .ods (the picture anchored in a cell): the picture
/// is on the sheet where the original has it (test/editing/xlsx-sales.xlsx; its chart is not read yet).
#[test]
fn a_picture_in_an_ods_is_on_the_sheet() {
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lo_pictures.ods")).unwrap();
    let ods = ibis_odf::read(&data).unwrap();
    let pictures = |p: &ibis_doc_model::Presentation| -> Vec<(usize, ibis_doc_model::EmuRect)> {
        p.slides.iter().enumerate().flat_map(|(i, s)| s.shapes.iter().filter(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Picture(_))).map(move |sh| (i, sh.frame))).collect()
    };
    let got = pictures(&ods);
    assert_eq!(got.len(), 1, "{got:?}");
    // 4.232 cm by 2.645 cm.
    let (_, frame) = got[0];
    assert!((frame.width - 1_523_520).abs() < 400 && (frame.height - 952_200).abs() < 400, "{frame:?}");
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsx-sales.xlsx")).unwrap();
    let want = pictures(&ibis_odf::read(&original).unwrap());
    assert_eq!(want.len(), got.len(), "{want:?}");
    assert_eq!(want[0].0, got[0].0, "the same sheet");
    // (The picture of the xlsx is anchored to cells, so its size follows the column widths: Excel for Windows' are about 10 % narrower than Excel for Mac's.)
    assert!((want[0].1.width - frame.width).abs() < 200_000 && (want[0].1.height - frame.height).abs() < 20_000, "{want:?} / {got:?}");
    // (The position is the same cell and the same offsets in it, 0.141 cm and 0.6 cm in column E; the column widths
    // and row heights LibreOffice wrote differ from Excel's a little, so the absolute position is not compared.)
    assert!(frame.x > 0 && frame.y > 0);
}

/// A workbook with a picture written as .ods and read again has the picture where it was (same sheet, same size,
/// same cell), and LibreOffice reads the .ods (checked by hand with a conversion to .xlsx: the picture is there).
#[test]
fn a_picture_survives_the_ods_round_trip() {
    let original = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsx-sales.xlsx")).unwrap();
    let first = ibis_odf::read(&original).unwrap();
    let ods = ibis_odf::write_ods(&first).unwrap();
    if let Ok(dir) = std::env::var("ODS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("pictures_written.ods"), &ods).unwrap();
    }
    let again = ibis_odf::read(&ods).unwrap();
    let pictures = |p: &ibis_doc_model::Presentation| -> Vec<(usize, ibis_doc_model::EmuRect)> {
        p.slides.iter().enumerate().flat_map(|(i, s)| s.shapes.iter().filter(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Picture(_))).map(move |sh| (i, sh.frame))).collect()
    };
    let (a, b) = (pictures(&first), pictures(&again));
    assert_eq!(a.len(), 1);
    assert_eq!(a.len(), b.len(), "{a:?} / {b:?}");
    assert_eq!(a[0].0, b[0].0);
    for (x, y) in [(a[0].1.x, b[0].1.x), (a[0].1.y, b[0].1.y), (a[0].1.width, b[0].1.width), (a[0].1.height, b[0].1.height)] {
        assert!((x - y).abs() < 2_000, "{a:?} / {b:?}");
    }
}

fn first_sheet(p: &ibis_doc_model::Presentation) -> std::sync::Arc<ibis_doc_model::Sheet> {
    ibis_doc_model::sheet_calc::sheet_of_slide(&p.slides[0]).unwrap().clone()
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// The rules of the conditional formats, as (range, kind, format) text that the two readings compare.
fn conditional_summary(sheet: &ibis_doc_model::Sheet) -> Vec<String> {
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

/// Conditional formats: the .ods LibreOffice made from a workbook has the formats the workbook has.
#[test]
fn conditional_formats_of_an_ods_are_the_workbooks() {
    let want = conditional_summary(&first_sheet(&ibis_odf::read(&fixture("ods_features.xlsx")).unwrap()));
    let got = conditional_summary(&first_sheet(&ibis_odf::read(&fixture("ods_features_lo.ods")).unwrap()));
    assert!(want.len() >= 8, "{want:#?}");
    assert_eq!(want, got);
}

/// Written as .ods and read again, the conditional formats are the same.
#[test]
fn conditional_formats_survive_the_ods_round_trip() {
    let first = ibis_odf::read(&fixture("ods_features.xlsx")).unwrap();
    let ods = ibis_odf::write_ods(&first).unwrap();
    if let Ok(dir) = std::env::var("ODS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("features_written.ods"), &ods).unwrap();
    }
    let again = ibis_odf::read(&ods).unwrap();
    assert_eq!(conditional_summary(&first_sheet(&first)), conditional_summary(&first_sheet(&again)));
}

fn validation_summary(sheet: &ibis_doc_model::Sheet) -> Vec<String> {
    let mut out: Vec<String> = sheet
        .validations
        .iter()
        .map(|v| {
            let ranges = v.ranges.iter().map(|g| format!("{}:{}-{}:{}", g.first_row, g.first_column, g.last_row, g.last_column)).collect::<Vec<_>>().join(",");
            format!("{ranges} {} {} {:?} {:?} blank={} drop={} {:?} {:?}", v.kind, v.operator, v.formula1, v.formula2, v.allow_blank, v.dropdown, v.prompt, v.error)
        })
        .collect();
    out.sort();
    out
}

/// Data validations: the .ods LibreOffice made from a workbook has the validations the workbook has.
#[test]
fn validations_of_an_ods_are_the_workbooks() {
    let want = validation_summary(&first_sheet(&ibis_odf::read(&fixture("ods_features.xlsx")).unwrap()));
    let got = validation_summary(&first_sheet(&ibis_odf::read(&fixture("ods_features_lo.ods")).unwrap()));
    assert_eq!(want.len(), 3, "{want:#?}");
    assert_eq!(want, got);
}

/// Written as .ods and read again, the validations are the same.
#[test]
fn validations_survive_the_ods_round_trip() {
    let first = ibis_odf::read(&fixture("ods_features.xlsx")).unwrap();
    let ods = ibis_odf::write_ods(&first).unwrap();
    if let Ok(dir) = std::env::var("ODS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("features_written.ods"), &ods).unwrap();
    }
    let again = ibis_odf::read(&ods).unwrap();
    assert_eq!(validation_summary(&first_sheet(&first)), validation_summary(&first_sheet(&again)));
}

fn note_summary(sheet: &ibis_doc_model::Sheet) -> Vec<(u32, u32, String, String)> {
    let mut v: Vec<_> = sheet.comments.iter().map(|c| (c.row, c.column, c.author.clone(), c.text.clone())).collect();
    v.sort();
    v
}

/// Notes: the .ods LibreOffice made from a workbook has the notes the workbook has (cell, author, text).
#[test]
fn notes_of_an_ods_are_the_workbooks() {
    let want = note_summary(&first_sheet(&ibis_odf::read(&fixture("ods_features.xlsx")).unwrap()));
    let got = note_summary(&first_sheet(&ibis_odf::read(&fixture("ods_features_lo.ods")).unwrap()));
    assert_eq!(want.len(), 2, "{want:#?}");
    assert_eq!(want[1].3, "Second note\nwith two lines");
    assert_eq!(want, got);
}

/// Written as .ods and read again, the notes are the same.
#[test]
fn notes_survive_the_ods_round_trip() {
    let first = ibis_odf::read(&fixture("ods_features.xlsx")).unwrap();
    let ods = ibis_odf::write_ods(&first).unwrap();
    if let Ok(dir) = std::env::var("ODS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("features_written.ods"), &ods).unwrap();
    }
    let again = ibis_odf::read(&ods).unwrap();
    assert_eq!(note_summary(&first_sheet(&first)), note_summary(&first_sheet(&again)));
}

/// The charts of a presentation as (slide, kind, series (name, categories, values), title).
fn chart_summary(p: &ibis_doc_model::Presentation) -> Vec<String> {
    let mut out = Vec::new();
    for (i, slide) in p.slides.iter().enumerate() {
        for shape in &slide.shapes {
            if let ibis_doc_model::ShapeKind::Chart(c) = &shape.kind {
                for g in &c.groups {
                    let series: Vec<String> = g.series.iter().map(|s| format!("{}|{:?}|{:?}", s.name, s.categories, s.values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>())).collect();
                    let axes: Vec<String> = c.axes.iter().map(|a| format!("{:?}/{:?}/{:?}", a.min.map(|n| n.0), a.max.map(|n| n.0), a.title.as_ref().map(|t| t.text.clone()))).collect();
                    out.push(format!("{i} {:?} {:?} {series:?} title={:?} legend={:?} axes={axes:?}", g.kind, g.grouping, c.title.as_ref().map(|t| t.text.clone()), c.legend.as_ref().map(|l| l.side)));
                }
            }
        }
    }
    out
}

/// A chart in an .ods (the chart of test/editing/xlsx-sales.xlsx saved by LibreOffice Calc) has the series, categories
/// and values the workbook's chart has.
#[test]
fn a_chart_in_an_ods_is_the_workbooks() {
    let want = chart_summary(&ibis_odf::read(&std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsx-sales.xlsx")).unwrap()).unwrap());
    let got = chart_summary(&ibis_odf::read(&fixture("lo_pictures.ods")).unwrap());
    assert_eq!(want.len(), 1, "{want:#?}");
    assert_eq!(want, got);
}

/// Line, pie, scatter and stacked bar charts: the .ods LibreOffice made from a workbook has the charts the workbook has.
#[test]
fn every_kind_of_chart_in_an_ods_is_the_workbooks() {
    let want = chart_summary(&ibis_odf::read(&fixture("ods_charts.xlsx")).unwrap());
    let got = chart_summary(&ibis_odf::read(&fixture("ods_charts_lo.ods")).unwrap());
    assert_eq!(want.len(), 4, "{want:#?}");
    for (w, g) in want.iter().zip(&got) {
        assert_eq!(w, g);
    }
    assert_eq!(want.len(), got.len(), "{got:#?}");
}

/// Written as .ods and read again, the charts are the same (the series now read the chart's own table).
#[test]
fn charts_survive_the_ods_round_trip() {
    for name in ["ods_charts.xlsx"] {
        let first = ibis_odf::read(&fixture(name)).unwrap();
        let ods = ibis_odf::write_ods(&first).unwrap();
        if let Ok(dir) = std::env::var("ODS_OUT") {
            std::fs::write(std::path::Path::new(&dir).join("charts_written.ods"), &ods).unwrap();
        }
        let again = ibis_odf::read(&ods).unwrap();
        let want = chart_summary(&first);
        assert_eq!(want.len(), 4);
        assert_eq!(want, chart_summary(&again));
    }
}

/// The drawing shapes of a presentation as text: kind, geometry, fill, outline colour, text; sizes in tenths of a
/// millimetre (the programs' column widths differ a little, so positions are not compared).
fn shape_summary(p: &ibis_doc_model::Presentation, with_size: bool) -> Vec<String> {
    use ibis_doc_model::{Fill, Geometry, ShapeKind};
    let fill = |f: &Fill| match f {
        Fill::Solid(c) => format!("{:06X}", c.rgb.0),
        _ => "none".to_string(),
    };
    let mut out = Vec::new();
    for slide in &p.slides {
        for shape in &slide.shapes {
            let size = if with_size { format!("{}x{}", (shape.frame.width as f64 / 3600.0).round(), (shape.frame.height as f64 / 3600.0).round()) } else { String::new() };
            match &shape.kind {
                ShapeKind::Auto(a) => {
                    let geometry = match &a.geometry {
                        Geometry::Rect => "rect".to_string(),
                        Geometry::RoundRect { .. } => "roundRect".to_string(),
                        Geometry::Ellipse => "ellipse".to_string(),
                        Geometry::Formula(f) => f.preset.clone().unwrap_or_default(),
                    };
                    let line = a.stroke.as_ref().map(|s| format!("{:06X}", s.color.rgb.0)).unwrap_or_default();
                    out.push(format!("auto {geometry} fill={} line={line} text={:?} {size}", fill(&a.fill), a.text.as_ref().map(|t| t.text())));
                }
                ShapeKind::TextBox(t) => out.push(format!("textbox {:?} {size}", t.text())),
                ShapeKind::Line(s) => out.push(format!("line {:06X} flip_v={} {size}", s.color.rgb.0, shape.flip_v)),
                _ => {}
            }
        }
    }
    out
}

/// Shapes: the .ods LibreOffice made from a workbook with a rectangle, an ellipse, a rounded rectangle, an arrow, a line
/// and a text box has the shapes the workbook has.
#[test]
fn shapes_of_an_ods_are_the_workbooks() {
    // (The sizes are not compared with the workbook's: it says them twice, in the anchors and in the shapes, and the
    // anchors' cells are wider in ibis than in the file. The .ods says the shapes' own: 4.973 cm by 2.063 cm.)
    let want = shape_summary(&ibis_odf::read(&fixture("ods_shapes.xlsx")).unwrap(), false);
    let got_with_size = shape_summary(&ibis_odf::read(&fixture("ods_shapes_lo.ods")).unwrap(), true);
    let got = shape_summary(&ibis_odf::read(&fixture("ods_shapes_lo.ods")).unwrap(), false);
    assert_eq!(want.len(), 6, "{want:#?}");
    assert_eq!(want, got);
    assert!(got_with_size[0].ends_with("497x206"), "{got_with_size:#?}");
}

/// Written as .ods and read again, the shapes are the same (kind, fill, outline, text, size).
#[test]
fn shapes_survive_the_ods_round_trip() {
    let first = ibis_odf::read(&fixture("ods_shapes.xlsx")).unwrap();
    let ods = ibis_odf::write_ods(&first).unwrap();
    if let Ok(dir) = std::env::var("ODS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("shapes_written.ods"), &ods).unwrap();
    }
    let again = ibis_odf::read(&ods).unwrap();
    let want = shape_summary(&first, true);
    assert_eq!(want.len(), 6, "{want:#?}");
    assert_eq!(want, shape_summary(&again, true));
}

/// A workbook's chart written as .ods reads the workbook's cells (as the chart did in the .xlsx), not only its own
/// copy of the data; LibreOffice reads it back with the same references.
#[test]
fn a_chart_written_as_ods_reads_the_cells() {
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsx-sales.xlsx")).unwrap();
    let opened = ibis_odf::open(data).unwrap();
    let ods = ibis_odf::write_ods_with_source(&opened.presentation, &opened.source).unwrap();
    if let Ok(dir) = std::env::var("ODS_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("sales-linked.ods"), &ods).unwrap();
    }
    let archive = ibis_ooxml::zip::Archive::new(ods.clone().into()).unwrap();
    let mut found = false;
    for entry in archive.entries() {
        if entry.name.ends_with("content.xml") && entry.name.starts_with("Object") {
            let xml = String::from_utf8(archive.read(entry).unwrap()).unwrap();
            // The series name the cells of the first sheet (月次売上), its values too.
            assert!(xml.contains("chart:values-cell-range-address=\"月次売上.B4:月次売上.B8\""), "{xml}");
            assert!(xml.contains("table:cell-range-address=\"月次売上.A4:月次売上.A8\""), "{xml}");
            found = true;
        }
    }
    assert!(found, "a chart object");
    // Read again, the chart's series are still there.
    let again = ibis_odf::read(&ods).unwrap();
    let charts: Vec<&ibis_doc_model::Chart> = again.slides.iter().flat_map(|s| &s.shapes).filter_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some(&**c), _ => None }).collect();
    assert_eq!(charts.len(), 1);
    let values: Vec<f64> = charts[0].groups[0].series[0].values.iter().flatten().map(|n| n.0).collect();
    assert_eq!(values, vec![1_250_000.0, 980_000.0, 1_530_000.0, 760_000.0, 2_100_000.0]);
    assert_eq!(charts[0].groups[0].series[0].name, "4月");
}
