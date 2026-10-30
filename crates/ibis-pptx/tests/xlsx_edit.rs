// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Editing workbooks and saving them (G2 of `tasks/doc/editing-plan.md`),
//! one scenario per feature: edit with commands, check the model, undo and
//! redo, save, check what changed in the file, read it back.
//!
//! Each saved file is also written to `target/xlsx-edit/` for the checks
//! with the real Excel (`scripts/office-check/check.sh`).

use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, Document, Presentation, SheetCommand, SlideId, StyleChange};
use ibis_ooxml::canon::diff_packages;
use ibis_pptx::SaveOptions;

fn open(name: &str) -> (Vec<u8>, ibis_pptx::Opened) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name);
    let data = std::fs::read(&path).unwrap();
    // The tests keep copies of what they save in `target/xlsx-edit/` (a fresh checkout has none).
    std::fs::create_dir_all(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit")).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    (data, opened)
}

fn slide(p: &Presentation, name: &str) -> SlideId {
    p.slides.iter().find(|s| s.name.as_deref() == Some(name)).unwrap().id
}

fn value(p: &Presentation, sheet: &str, row: u32, column: u32) -> CellValue {
    let s = p.slide(slide(p, sheet)).unwrap();
    sheet_of_slide(s).unwrap().cell(row, column).map(|c| c.value.clone()).unwrap_or_default()
}

fn cell(p: &Presentation, sheet: &str, row: u32, column: u32) -> ibis_doc_model::SheetCell {
    let s = p.slide(slide(p, sheet)).unwrap();
    sheet_of_slide(s).unwrap().cell(row, column).cloned().unwrap()
}

fn set(d: &mut Document, sheet: SlideId, edits: Vec<(u32, u32, CellInput)>) {
    let edits = edits.into_iter().map(|(row, column, input)| CellEdit { row, column, input }).collect();
    d.execute(vec![Command::Sheet(SheetCommand::SetCells { slide: sheet, edits })]).unwrap();
}

/// Save, keep a copy for the Excel checks, and return the bytes.
fn save(opened: &ibis_pptx::Opened, p: &Presentation, name: &str) -> Vec<u8> {
    let saved = opened.source.save(p, SaveOptions::default()).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(name), &saved).unwrap();
    saved
}

fn changed_parts(a: &[u8], b: &[u8]) -> Vec<String> {
    let report = diff_packages(a, b, &[]).unwrap();
    let mut parts: Vec<String> = report.differences.iter().map(|(p, _)| p.clone()).collect();
    parts.sort();
    parts
}

/// The model read back from a saved file equals the edited one (cells of
/// every sheet: values, formulas, displayed text and formats).
fn assert_reads_back(saved: &[u8], edited: &Presentation) {
    let again = ibis_pptx::read(saved).unwrap();
    for (a, b) in again.slides.iter().zip(&edited.slides) {
        let (Some(x), Some(y)) = (sheet_of_slide(a), sheet_of_slide(b)) else { continue };
        let cells = |s: &ibis_doc_model::Sheet| s.cells.iter().map(|c| (c.row, c.column, c.value.clone(), c.formula.clone(), c.text.clone(), { let mut st = s.styles[c.style as usize].clone(); st.xf = None; st })).collect::<Vec<_>>();
        let (cx, cy) = (cells(x), cells(y));
        for (p, q) in cx.iter().zip(&cy) {
            assert_eq!(p, q, "sheet {:?}", a.name);
        }
        assert_eq!(cx.len(), cy.len(), "sheet {:?}", a.name);
        assert_eq!(x.comments, y.comments, "sheet {:?} comments", a.name);
    }
}

#[test]
fn a_number_typed_in_is_saved_and_recalculated() {
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    // B4 feeds E4 (a shared formula SUM(B4:D4)) and the total below.
    assert_eq!(value(d.presentation(), "月次売上", 3, 4), CellValue::Number(3_750_000.0));
    let total_before = value(d.presentation(), "月次売上", 8, 4);
    set(&mut d, s, vec![(3, 1, CellInput::Value(CellValue::Number(1_300_000.0)))]);
    assert_eq!(value(d.presentation(), "月次売上", 3, 4), CellValue::Number(3_800_000.0));
    let CellValue::Number(t0) = total_before else { panic!() };
    assert_eq!(value(d.presentation(), "月次売上", 8, 4), CellValue::Number(t0 + 50_000.0));
    assert_eq!(cell(d.presentation(), "月次売上", 3, 1).text, "1,300,000");

    // The note sheet's TEXT(月次売上!E9,…) follows the total.
    assert_eq!(value(d.presentation(), "メモ", 3, 0), CellValue::Text(format!("{} 円", ibis_calc::numfmt::format_text_only(&CellValue::Number(t0 + 50_000.0), "#,##0", false))));
    let saved = save(&opened, d.presentation(), "number.xlsx");
    // (And the chart that plots B4: its cached values follow the cell, as Excel updates them.)
    assert_eq!(changed_parts(&data, &saved), vec!["xl/charts/chart1.xml".to_string(), "xl/worksheets/sheet1.xml".to_string(), "xl/worksheets/sheet3.xml".to_string()]);
    assert_reads_back(&saved, d.presentation());

    assert!(d.undo());
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap() == data);
    assert!(d.redo());
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap() == saved);
}

#[test]
fn text_goes_into_the_shared_strings() {
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "メモ");
    // A new string, and one the workbook already has (reused).
    let existing = match value(d.presentation(), "月次売上", 2, 1) {
        CellValue::Text(t) => t,
        other => panic!("{other:?}"),
    };
    set(&mut d, s, vec![(9, 0, CellInput::Value(CellValue::Text("新しいメモ".into()))), (10, 0, CellInput::Value(CellValue::Text(existing.clone())))]);
    let saved = save(&opened, d.presentation(), "text.xlsx");
    assert_eq!(changed_parts(&data, &saved), vec!["xl/sharedStrings.xml".to_string(), "xl/worksheets/sheet3.xml".to_string()]);
    assert_reads_back(&saved, d.presentation());
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sst = String::from_utf8(package.read("xl/sharedStrings.xml").unwrap()).unwrap();
    assert_eq!(sst.matches(&format!(">{existing}<")).count(), 1, "an existing string is used again");
    assert!(sst.contains(">新しいメモ</t></si>"));
    assert!(d.undo());
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn formulas_are_saved_with_their_values_and_the_calculation_chain() {
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    // A new formula, a newer function (written with _xlfn.), and a
    // formula cleared.
    set(&mut d, s, vec![(12, 1, CellInput::Formula("B4*2".into())), (13, 1, CellInput::Formula("XOR(TRUE,FALSE)".into()))]);
    assert_eq!(value(d.presentation(), "月次売上", 12, 1), CellValue::Number(2_500_000.0));
    assert_eq!(value(d.presentation(), "月次売上", 13, 1), CellValue::Bool(true));
    // E5 is a member of the shared formula E4:E8: clearing it writes the
    // group out cell by cell.
    set(&mut d, s, vec![(4, 4, CellInput::Clear)]);
    let saved = save(&opened, d.presentation(), "formula.xlsx");
    assert_eq!(changed_parts(&data, &saved), vec!["[Content_Types].xml".to_string(), "xl/_rels/workbook.xml.rels".to_string(), "xl/calcChain.xml".to_string(), "xl/metadata.xml".to_string(), "xl/worksheets/sheet1.xml".to_string(), "xl/worksheets/sheet3.xml".to_string()]);
    assert_reads_back(&saved, d.presentation());
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    let xml=package.xml("xl/worksheets/sheet1.xml").unwrap();let root=xml.root_element().unwrap();
    for(address,formula,cache)in[("B13","B4*2","2500000"),("B14","_xlfn.XOR(TRUE,FALSE)","1")] {
        let c=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="c" && xml.attr(n,"r")==Some(address)).unwrap();
        // One value, as Excel writes such a formula: plain (no t="array", no cm).
        let f=xml.child(c,"f").unwrap();assert_eq!(xml.text(f),formula);assert_eq!(xml.attr(f,"t"),None);assert_eq!(xml.attr(f,"ref"),None);assert!(xml.attr(c,"cm").is_none());
        assert_eq!(xml.text(xml.child(c,"v").unwrap()),cache);
    }
    let reopened=ibis_pptx::read(&saved).unwrap();let sh=ibis_doc_model::sheet_calc::sheet_of_slide(reopened.slide(slide(&reopened,"月次売上")).unwrap()).unwrap();
    assert!(!sh.spills.contains_key(&(12,1)) && !sh.spills.contains_key(&(13,1)),"plain formulas read back plain");
    assert!(sheet.contains("<f>SUM(B6:D6)</f>"), "the shared formula written out");
    assert!(!sheet.contains("si=\"0\""));
    let chain = String::from_utf8(package.read("xl/calcChain.xml").unwrap()).unwrap();
    assert!(chain.contains("<c r=\"B13\" i=\"1\"/>"));
    let before = String::from_utf8(ibis_ooxml::Package::open(data.clone()).unwrap().read("xl/calcChain.xml").unwrap()).unwrap();
    assert_eq!(chain.matches("r=\"E5\"").count() + 1, before.matches("r=\"E5\"").count(), "E5 of the first sheet leaves the chain");
    while d.undo() {}
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn formats_make_new_cell_formats() {
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let range = CellRange { first_row: 3, first_column: 1, last_row: 4, last_column: 2 };
    for change in [StyleChange::Bold(true), StyleChange::Fill(Some(ibis_doc_model::Rgb(0xFFEE99))), StyleChange::NumberFormat("#,##0.0\"円\"".into())] {
        d.execute(vec![Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![range], change })]).unwrap();
    }
    let c = cell(d.presentation(), "月次売上", 3, 1);
    assert_eq!(c.text, "1,250,000.0円");
    let saved = save(&opened, d.presentation(), "format.xlsx");
    assert_eq!(changed_parts(&data, &saved), vec!["xl/styles.xml".to_string(), "xl/worksheets/sheet1.xml".to_string()]);
    assert_reads_back(&saved, d.presentation());
    // The same format twice is one new cellXfs entry.
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let styles = String::from_utf8(package.read("xl/styles.xml").unwrap()).unwrap();
    let before = String::from_utf8(ibis_ooxml::Package::open(data.clone()).unwrap().read("xl/styles.xml").unwrap()).unwrap();
    let xfs = |s: &str| s.split("<cellXfs").nth(1).unwrap().split("</cellXfs>").next().unwrap().matches("<xf ").count();
    assert_eq!(xfs(&styles), xfs(&before) + 1);
    assert!(styles.contains("formatCode=\"#,##0.0&quot;円&quot;\""));
    while d.undo() {}
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap() == data);
}

fn formula(p: &Presentation, sheet: &str, row: u32, column: u32) -> Option<String> {
    let s = p.slide(slide(p, sheet)).unwrap();
    sheet_of_slide(s).unwrap().cell(row, column).and_then(|c| c.formula.clone())
}

#[test]
fn rows_and_columns_inserted_and_deleted_are_saved() {
    use ibis_doc_model::sheet_command::Axis;
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let total = formula(d.presentation(), "月次売上", 8, 4).unwrap();
    // A row inserted inside the data (before row 6): the totals' ranges grow.
    d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Rows, at: 5, count: 1 })]).unwrap();
    assert_eq!(formula(d.presentation(), "月次売上", 9, 4).as_deref().map(|f| f.contains("E9")), Some(true), "{total}");
    set(&mut d, s, vec![(5, 1, CellInput::Value(CellValue::Number(100.0))), (5, 2, CellInput::Value(CellValue::Number(200.0))), (5, 3, CellInput::Value(CellValue::Number(300.0))), (5, 4, CellInput::Formula("SUM(B6:D6)".into()))]);
    let saved = save(&opened, d.presentation(), "insert-row.xlsx");
    assert_reads_back(&saved, d.presentation());
    // Another sheet's formula refers to the moved total.
    assert!(formula(d.presentation(), "メモ", 3, 0).unwrap().contains("E10"));
    // A column deleted (C): the formulas lose it.
    d.execute(vec![Command::Sheet(SheetCommand::Delete { slide: s, axis: Axis::Columns, at: 2, count: 1 })]).unwrap();
    let saved = save(&opened, d.presentation(), "delete-column.xlsx");
    assert_reads_back(&saved, d.presentation());
    // The first row deleted.
    d.execute(vec![Command::Sheet(SheetCommand::Delete { slide: s, axis: Axis::Rows, at: 0, count: 1 })]).unwrap();
    let saved = save(&opened, d.presentation(), "delete-row.xlsx");
    assert_reads_back(&saved, d.presentation());
    while d.undo() {}
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn sizes_and_merges_are_saved() {
    use ibis_doc_model::sheet_command::Axis;
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let pt = ibis_doc_model::EMU_PER_POINT;
    d.execute(vec![Command::Sheet(SheetCommand::SetSize { slide: s, axis: Axis::Columns, first: 1, last: 2, size: 90 * pt })]).unwrap();
    d.execute(vec![Command::Sheet(SheetCommand::SetSize { slide: s, axis: Axis::Rows, first: 3, last: 3, size: 30 * pt })]).unwrap();
    d.execute(vec![Command::Sheet(SheetCommand::SetSize { slide: s, axis: Axis::Columns, first: 8, last: 8, size: 0 })]).unwrap();
    let range = CellRange { first_row: 10, first_column: 0, last_row: 10, last_column: 3 };
    set(&mut d, s, vec![(10, 0, CellInput::Value(CellValue::Text("注記".into()))), (10, 2, CellInput::Value(CellValue::Number(1.0)))]);
    d.execute(vec![Command::Sheet(SheetCommand::Merge { slide: s, range, center: true })]).unwrap();
    assert!(value(d.presentation(), "月次売上", 10, 2).is_empty());
    let saved = save(&opened, d.presentation(), "sizes-merge.xlsx");
    assert_reads_back(&saved, d.presentation());
    let again = ibis_pptx::read(&saved).unwrap();
    let sheet = sheet_of_slide(again.slides.iter().find(|x| x.name.as_deref() == Some("月次売上")).unwrap()).unwrap().clone();
    let edited = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap().clone();
    assert_eq!(sheet.columns[1], edited.columns[1]);
    assert_eq!(sheet.rows[3], edited.rows[3]);
    assert_eq!(sheet.columns[8], 0);
    assert!(sheet.merges.contains(&range));
    // Unmerge.
    d.execute(vec![Command::Sheet(SheetCommand::Unmerge { slide: s, range })]).unwrap();
    let saved = save(&opened, d.presentation(), "unmerge.xlsx");
    let again = ibis_pptx::read(&saved).unwrap();
    assert!(!sheet_of_slide(again.slides.iter().find(|x| x.name.as_deref() == Some("月次売上")).unwrap()).unwrap().merges.contains(&range));
    while d.undo() {}
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap() == data);
}

#[test]
fn sheets_added_renamed_and_deleted_are_saved() {
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (new_slide, new_shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "集計".into(), slide: new_slide, shape: new_shape })]).unwrap();
    assert_eq!(d.presentation().slides.iter().map(|s| s.name.clone().unwrap()).collect::<Vec<_>>(), ["月次売上", "集計", "商品", "メモ"]);
    set(&mut d, new_slide, vec![(0, 0, CellInput::Value(CellValue::Text("合計".into()))), (0, 1, CellInput::Formula("SUM(月次売上!E4:E8)".into()))]);
    assert_eq!(value(d.presentation(), "集計", 0, 1), value(d.presentation(), "月次売上", 8, 4));
    let saved = save(&opened, d.presentation(), "add-sheet.xlsx");
    assert_reads_back(&saved, d.presentation());
    let again = ibis_pptx::read(&saved).unwrap();
    assert_eq!(again.slides[1].name.as_deref(), Some("集計"));
    // Renaming the source sheet: the formulas follow.
    let s = slide(d.presentation(), "月次売上");
    d.execute(vec![Command::Sheet(SheetCommand::RenameSheet { slide: s, name: "売上 2026".into() })]).unwrap();
    assert_eq!(formula(d.presentation(), "集計", 0, 1).as_deref(), Some("SUM('売上 2026'!E4:E8)"));
    let saved = save(&opened, d.presentation(), "rename-sheet.xlsx");
    assert_reads_back(&saved, d.presentation());
    // Deleting a sheet another refers to: #REF!.
    let products = slide(d.presentation(), "商品");
    d.execute(vec![Command::Sheet(SheetCommand::DeleteSheet { slide: products })]).unwrap();
    assert!(formula(d.presentation(), "メモ", 4, 0).unwrap().contains("#REF!"));
    let saved = save(&opened, d.presentation(), "delete-sheet.xlsx");
    assert_reads_back(&saved, d.presentation());
    while d.undo() {}
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap() == data);
}

/// A formula the engine cannot calculate, whose input changes, makes Excel
/// calculate the workbook when it opens it; otherwise the flag is not set.
#[test]
fn stale_formulas_ask_excel_to_calculate() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let calc_flag = |bytes: &[u8]| String::from_utf8(ibis_ooxml::Package::open(bytes.to_vec()).unwrap().read("xl/workbook.xml").unwrap()).unwrap().contains("fullCalcOnLoad=\"1\"");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    set(&mut d, s, vec![(3, 1, CellInput::Value(CellValue::Number(5.0)))]);
    assert!(!calc_flag(&save(&opened, d.presentation(), "not-stale.xlsx")));
    // INFO("release") is not calculated by the engine.
    set(&mut d, s, vec![(20, 0, CellInput::Formula("INFO(\"release\")".into()))]);
    set(&mut d, s, vec![(3, 1, CellInput::Value(CellValue::Number(6.0)))]);
    assert!(calc_flag(&save(&opened, d.presentation(), "stale.xlsx")));
    // Undone: no longer.
    d.undo();
    d.undo();
    assert!(!calc_flag(&opened.source.save(d.presentation(), SaveOptions::default()).unwrap()));
}

fn conditional(p: &Presentation, s: SlideId) -> Vec<ibis_doc_model::ConditionalFormat> {
    sheet_of_slide(p.slide(s).unwrap()).unwrap().conditional.clone()
}

fn formula_rule(formula: &str, fill: u32, stop: bool) -> ibis_doc_model::ConditionalFormat {
    use ibis_doc_model::{ConditionalFormat, CfRule, CfKind, CfStyle, Rgb};
    ConditionalFormat {
        source_index: None,
        ranges: vec![CellRange { first_row: 3, last_row: 7, first_column: 1, last_column: 3 }],
        rules: vec![CfRule { kind: CfKind::Expression(formula.into()), priority: 99, stop,
            style: Some(CfStyle { fill: Some(Rgb(fill)), bold: Some(false), italic: Some(true), underline: Some(ibis_doc_model::CellUnderline::None), strike: Some(false), color: Some(Rgb(0x9C0006)), number_format: Some("#,##0.00".into()), borders: [Some(ibis_doc_model::Border { line: ibis_doc_model::BorderLine::Double, color: Rgb(0x123456) }), None, None, None], ..Default::default() }) }],
    }
}

#[test]
fn conditional_create_edit_delete_undo_redo_and_save() {
    let (original, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let start = conditional(d.presentation(), s).len();
    d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format: formula_rule("AND(B4>1000000,$B4>0,B$4>0,$B$4>0)", 0xFFC7CE, true) })]).unwrap();
    let first = conditional(d.presentation(), s);
    let first_bytes = save(&opened, d.presentation(), "conditional-create.xlsx");
    assert_eq!(conditional(&ibis_pptx::read(&first_bytes).unwrap(), s), first);
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
    assert!(d.redo());
    assert_eq!(save(&opened, d.presentation(), "conditional-redo.xlsx"), first_bytes);
    d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format: formula_rule("B4>0", 0xC6EFCE, false) })]).unwrap();
    let two = conditional(d.presentation(), s);
    assert!(two[start].rules[0].priority < two[start+1].rules[0].priority);
    assert!(two[start].rules[0].stop);
    let mut changed = two[start].clone();
    changed.ranges[0].last_row = 8;
    changed.rules[0].kind = ibis_doc_model::CfKind::Expression("B4>2000000".into());
    changed.rules[0].style.as_mut().unwrap().fill = Some(ibis_doc_model::Rgb(0xFFFF00));
    changed.rules[0].priority = 999;
    d.execute(vec![Command::Sheet(SheetCommand::UpdateConditional { slide: s, index: start, format: changed })]).unwrap();
    let edited = conditional(d.presentation(), s);
    assert_eq!(edited[start].rules[0].priority, two[start].rules[0].priority);
    let edited_bytes = save(&opened, d.presentation(), "conditional-edit.xlsx");
    assert_eq!(conditional(&ibis_pptx::read(&edited_bytes).unwrap(), s), edited);
    assert!(d.undo()); assert_eq!(conditional(d.presentation(), s), two);
    assert!(d.redo()); assert_eq!(conditional(d.presentation(), s), edited);
    let ranges = edited[start].ranges.clone();
    d.execute(vec![Command::Sheet(SheetCommand::ClearConditional { slide: s, ranges })]).unwrap();
    assert_eq!(conditional(d.presentation(), s).len(), start);
    let cleared = save(&opened, d.presentation(), "conditional-delete.xlsx");
    assert_eq!(conditional(&ibis_pptx::read(&cleared).unwrap(), s).len(), start);
    assert!(d.undo()); assert_eq!(conditional(d.presentation(), s), edited);
    assert_eq!(save(&opened, d.presentation(), "conditional-delete-undo.xlsx"), edited_bytes);
    assert!(d.redo()); assert_eq!(save(&opened, d.presentation(), "conditional-delete-redo.xlsx"), cleared);
}

#[test]
fn conditional_mixed_references_follow_insert_and_roundtrip() {
    use ibis_doc_model::sheet_command::Axis;
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format: formula_rule("AND(B4>0,$B4>0,B$4>0,$B$4>0)", 0xFFC7CE, true) })]).unwrap();
    // Reopen so the structural writer also exercises imported CF XML.
    let saved = save(&opened, d.presentation(), "conditional-before-insert.xlsx");
    let opened = ibis_pptx::open(saved).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let before = conditional(d.presentation(), s);
    d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Rows, at: 2, count: 1 })]).unwrap();
    let after = conditional(d.presentation(), s);
    assert_eq!(after.last().unwrap().ranges[0].first_row, 4);
    assert_eq!(after.last().unwrap().rules[0].kind, ibis_doc_model::CfKind::Expression("AND(B5>0,$B5>0,B$5>0,$B$5>0)".into()));
    let saved = save(&opened, d.presentation(), "conditional-insert.xlsx");
    assert_eq!(conditional(&ibis_pptx::read(&saved).unwrap(), s), after);
    assert!(d.undo()); assert_eq!(conditional(d.presentation(), s), before);
    assert!(d.redo()); assert_eq!(conditional(d.presentation(), s), after);
}

#[test]
fn adding_conditional_preserves_unknown_original_xml() {
    let (data, _) = open("xlsx-sales.xlsx");
    let mut package = ibis_ooxml::Package::open(data).unwrap();
    let part = "xl/worksheets/sheet1.xml";
    let doc = package.xml_mut(part).unwrap();
    let root = doc.root_element().unwrap();
    let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
    let cf = doc.append_element(root, &format!("{prefix}conditionalFormatting"));
    doc.set_attr(cf, "sqref", "B4:D8");
    doc.set_attr(cf, "pivot", "1");
    let rule = doc.append_element(cf, &format!("{prefix}cfRule"));
    doc.set_attr(rule, "type", "timePeriod");
    doc.set_attr(rule, "timePeriod", "last7Days");
    doc.set_attr(rule, "priority", "3");
    doc.set_attr(rule, "stopIfTrue", "1");
    // A recognised expression with unmodelled attributes must survive too.
    let cf2 = doc.append_element(root, &format!("{prefix}conditionalFormatting"));
    doc.set_attr(cf2, "sqref", "E4:E8");
    let r2 = doc.append_element(cf2, &format!("{prefix}cfRule"));
    doc.set_attr(r2, "type", "containsText"); doc.set_attr(r2, "text", "sale"); doc.set_attr(r2, "priority", "4");
    let f = doc.append_element(r2, &format!("{prefix}formula")); doc.set_text(f, "NOT(ISERROR(SEARCH(\"sale\",E4)))");
    let input = package.save(Default::default()).unwrap();
    let opened = ibis_pptx::open(input).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format: formula_rule("B4>0", 0xFFC7CE, false) })]).unwrap();
    let bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let package = ibis_ooxml::Package::open(bytes).unwrap();
    let xml = String::from_utf8(package.read(part).unwrap()).unwrap();
    for text in ["pivot=\"1\"", "type=\"timePeriod\"", "timePeriod=\"last7Days\"", "type=\"containsText\"", "text=\"sale\""] { assert!(xml.contains(text), "{text}"); }
    assert_eq!(conditional(d.presentation(), s)[2].rules[0].priority, 5);
}

/// Imported formula-valued CF thresholds follow sheet rename/delete as XML
/// attributes, with numeric thresholds and unmodelled CF attributes preserved.
#[test]
fn conditional_thresholds_follow_sheet_rename_delete_and_roundtrip() {
    use ibis_doc_model::{CfKind, CfValue, Rgb};
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let reference = "商品!$B4+商品!B$4+商品!$B$4";
    let point = |kind: &str, value: &str| CfValue { kind: kind.into(), value: Some(value.into()), gte: false };
    let numeric = point("num", "100");
    let formula = point("formula", reference);
    let kinds = vec![
        CfKind::ColorScale(vec![(formula.clone(), Rgb(0xFF0000)), (numeric.clone(), Rgb(0x00FF00))]),
        CfKind::data_bar(formula.clone(), numeric.clone(), Rgb(0x123456), true),
        CfKind::icon_set("3Arrows", vec![point("num", "0"), formula, numeric], true, false),
    ];
    for kind in kinds {
        let mut format = formula_rule("A1>0", 0xFFC7CE, false);
        format.rules[0].kind = kind;
        format.rules[0].style = None;
        d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format })]).unwrap();
    }
    let initial = save(&opened, d.presentation(), "conditional-thresholds-before.xlsx");
    let mut package = ibis_ooxml::Package::open(initial).unwrap();
    let part = "xl/worksheets/sheet1.xml";
    let doc = package.xml_mut(part).unwrap();
    let root = doc.root_element().unwrap();
    let formats: Vec<_> = doc.children_named(root, "conditionalFormatting").collect();
    for cf in formats { doc.set_attr(cf, "pivot", "1"); }
    let initial = package.save(Default::default()).unwrap();
    let opened = ibis_pptx::open(initial.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let before = conditional(d.presentation(), s);
    let products = slide(d.presentation(), "商品");
    d.execute(vec![Command::Sheet(SheetCommand::RenameSheet { slide: products, name: "商品 2026".into() })]).unwrap();
    let renamed = conditional(d.presentation(), s);
    let threshold = |c: &ibis_doc_model::ConditionalFormat| match &c.rules[0].kind {
        CfKind::ColorScale(points) => points[0].0.value.clone().unwrap(),
        CfKind::DataBar { min, .. } => min.value.clone().unwrap(),
        CfKind::IconSet { values, .. } => values[1].value.clone().unwrap(),
        _ => panic!("threshold rule"),
    };
    for cf in &renamed { assert_eq!(threshold(cf), "'商品 2026'!$B4+'商品 2026'!B$4+'商品 2026'!$B$4"); }
    let bytes = save(&opened, d.presentation(), "conditional-thresholds-rename.xlsx");
    assert_eq!(conditional(&ibis_pptx::read(&bytes).unwrap(), s), renamed);
    let saved_package = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let xml = saved_package.xml(part).unwrap();
    assert!(xml.children_named(xml.root_element().unwrap(), "conditionalFormatting").all(|cf| xml.attr(cf, "pivot") == Some("1")));
    assert!(d.undo()); assert_eq!(conditional(d.presentation(), s), before);
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), initial);
    assert!(d.redo()); assert_eq!(conditional(d.presentation(), s), renamed);
    assert_eq!(save(&opened, d.presentation(), "conditional-thresholds-rename-redo.xlsx"), bytes);
    // Reopen the renamed file before deleting, to test imported attributes again.
    let opened = ibis_pptx::open(bytes).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let products = slide(d.presentation(), "商品 2026");
    d.execute(vec![Command::Sheet(SheetCommand::DeleteSheet { slide: products })]).unwrap();
    let deleted = conditional(d.presentation(), s);
    for cf in &deleted { assert_eq!(threshold(cf), "#REF!+#REF!+#REF!"); }
    let bytes = save(&opened, d.presentation(), "conditional-thresholds-delete-sheet.xlsx");
    assert_eq!(conditional(&ibis_pptx::read(&bytes).unwrap(), s), deleted);
    assert!(d.undo()); assert_eq!(conditional(d.presentation(), s), renamed);
    assert!(d.redo()); assert_eq!(conditional(d.presentation(), s), deleted);
    assert_eq!(save(&opened, d.presentation(), "conditional-thresholds-delete-sheet-redo.xlsx"), bytes);
}

#[test]
fn conditional_management_preserves_imported_rule_xml_and_styles() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format: formula_rule("B4>0", 0xFFC7CE, true) })]).unwrap();
    let bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let mut package = ibis_ooxml::Package::open(bytes).unwrap();
    let part = "xl/worksheets/sheet1.xml";
    let doc = package.xml_mut(part).unwrap();
    let root = doc.root_element().unwrap();
    let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
    let cf = doc.children_named(root, "conditionalFormatting").next().unwrap();
    doc.set_attr(cf, "pivot", "1");
    let first = doc.children_named(cf, "cfRule").next().unwrap();
    doc.set_attr(first, "priority", "7");
    let unknown = doc.append_element(cf, &format!("{prefix}cfRule"));
    doc.set_attr(unknown, "type", "timePeriod"); doc.set_attr(unknown, "timePeriod", "last7Days");
    doc.set_attr(unknown, "priority", "10"); doc.set_attr(unknown, "stopIfTrue", "1");
    let input = package.save(Default::default()).unwrap();
    let original_package = ibis_ooxml::Package::open(input.clone()).unwrap();
    let styles = original_package.read("xl/styles.xml").unwrap();
    let opened = ibis_pptx::open(input.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let before = conditional(d.presentation(), s);
    assert!(d.execute(vec![Command::Sheet(SheetCommand::MoveConditionalRule { slide: s, index: 0, rule: 0, up: true })]).is_err());
    assert_eq!(conditional(d.presentation(), s), before);
    d.execute(vec![Command::Sheet(SheetCommand::MoveConditionalRule { slide: s, index: 0, rule: 1, up: true })]).unwrap();
    let moved = conditional(d.presentation(), s);
    assert_eq!(moved[0].rules[0].priority, 2); assert_eq!(moved[0].rules[1].priority, 1);
    let moved_bytes = save(&opened, d.presentation(), "conditional-manage-move.xlsx");
    assert_eq!(conditional(&ibis_pptx::read(&moved_bytes).unwrap(), s), moved);
    assert!(d.undo()); assert_eq!(conditional(d.presentation(), s), before);
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), input);
    assert!(d.redo()); assert_eq!(conditional(d.presentation(), s), moved);
    assert_eq!(save(&opened, d.presentation(), "conditional-manage-move-redo.xlsx"), moved_bytes);
    d.execute(vec![Command::Sheet(SheetCommand::SetConditionalStop { slide: s, index: 0, rule: 1, stop: false })]).unwrap();
    let stopped = conditional(d.presentation(), s);
    let bytes = save(&opened, d.presentation(), "conditional-manage-stop.xlsx");
    assert_eq!(conditional(&ibis_pptx::read(&bytes).unwrap(), s), stopped);
    let package = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    assert_eq!(package.read("xl/styles.xml").unwrap(), styles, "management must not duplicate dxfs");
    let xml = package.xml(part).unwrap();
    let cf = xml.children_named(xml.root_element().unwrap(), "conditionalFormatting").next().unwrap();
    assert_eq!(xml.attr(cf, "pivot"), Some("1"));
    let rules: Vec<_> = xml.children_named(cf, "cfRule").collect();
    assert_eq!(xml.attr(rules[1], "type"), Some("timePeriod"));
    assert_eq!(xml.attr(rules[1], "timePeriod"), Some("last7Days"));
    assert_eq!(xml.attr(rules[1], "stopIfTrue"), None);
    assert_eq!(xml.attr(rules[0], "dxfId"), Some("0"));
    assert!(d.undo()); assert_eq!(conditional(d.presentation(), s), moved);
    assert!(d.redo()); assert_eq!(conditional(d.presentation(), s), stopped);
    assert_eq!(save(&opened, d.presentation(), "conditional-manage-stop-redo.xlsx"), bytes);
}

#[test]
fn adding_identical_conditional_keeps_both_dxf_styles() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let format = formula_rule("B4>0", 0xFFC7CE, false);
    d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format: format.clone() })]).unwrap();
    let bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let opened = ibis_pptx::open(bytes).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format })]).unwrap();
    let expected = conditional(d.presentation(), s);
    assert_eq!(expected.len(), 2);
    let saved = save(&opened, d.presentation(), "conditional-identical.xlsx");
    assert_eq!(conditional(&ibis_pptx::read(&saved).unwrap(), s), expected);
}

#[test]
fn conditional_duplicate_and_delete_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let format = formula_rule("B4>1000", 0xFFC7CE, true);
    d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format })]).unwrap();
    let initial = conditional(d.presentation(), s);
    assert_eq!(initial.len(), 1);
    assert_eq!(initial[0].rules[0].priority, 1);

    // Duplicate rule 0 (priority 1)
    d.execute(vec![Command::Sheet(SheetCommand::DuplicateConditionalRule { slide: s, index: 0, rule: 0 })]).unwrap();
    let duplicated = conditional(d.presentation(), s);
    assert_eq!(duplicated.len(), 2);
    assert_eq!(duplicated[0].rules[0].priority, 1);
    assert_eq!(duplicated[1].rules[0].priority, 2);
    assert_eq!(duplicated[1].rules[0].kind, duplicated[0].rules[0].kind);
    assert_eq!(duplicated[1].rules[0].style, duplicated[0].rules[0].style);
    assert_eq!(duplicated[1].rules[0].stop, duplicated[0].rules[0].stop);
    assert_eq!(duplicated[1].ranges, duplicated[0].ranges);

    // Save with duplicated rule, reopen and verify
    let bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reloaded = ibis_pptx::read(&bytes).unwrap();
    let reloaded_cf = conditional(&reloaded, s);
    assert_eq!(reloaded_cf.len(), 2);
    assert_eq!(reloaded_cf[0].rules[0].priority, 1);
    assert_eq!(reloaded_cf[1].rules[0].priority, 2);
    assert_eq!(reloaded_cf[1].rules[0].kind, reloaded_cf[0].rules[0].kind);

    // Undo duplication
    assert!(d.undo());
    assert_eq!(conditional(d.presentation(), s), initial);

    // Redo duplication
    assert!(d.redo());
    assert_eq!(conditional(d.presentation(), s), duplicated);

    // Delete duplicated rule (index 1, rule 0)
    d.execute(vec![Command::Sheet(SheetCommand::DeleteConditionalRule { slide: s, index: 1, rule: 0 })]).unwrap();
    let after_delete = conditional(d.presentation(), s);
    assert_eq!(after_delete.len(), 1);
    assert_eq!(after_delete[0].rules[0].priority, 1);

    // Save after deletion, reopen and verify
    let bytes_del = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reloaded_del = ibis_pptx::read(&bytes_del).unwrap();
    assert_eq!(conditional(&reloaded_del, s), after_delete);

    // Undo delete
    assert!(d.undo());
    assert_eq!(conditional(d.presentation(), s), duplicated);

    // Redo delete
    assert!(d.redo());
    assert_eq!(conditional(d.presentation(), s), after_delete);
}

#[test]
fn managing_equal_unmodelled_rules_keeps_each_original_identity() {
    let (bytes, _) = open("xlsx-sales.xlsx");
    let mut package = ibis_ooxml::Package::open(bytes).unwrap();
    let part = "xl/worksheets/sheet1.xml";
    let doc = package.xml_mut(part).unwrap();
    let root = doc.root_element().unwrap();
    let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
    for (priority, period) in [(1, "last7Days"), (2, "yesterday")] {
        let cf = doc.append_element(root, &format!("{prefix}conditionalFormatting"));
        doc.set_attr(cf, "sqref", "B4:D8");
        let r = doc.append_element(cf, &format!("{prefix}cfRule"));
        doc.set_attr(r, "type", "timePeriod"); doc.set_attr(r, "timePeriod", period);
        doc.set_attr(r, "priority", &priority.to_string());
    }
    let opened = ibis_pptx::open(package.save(Default::default()).unwrap()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    d.execute(vec![Command::Sheet(SheetCommand::MoveConditionalRule { slide: s, index: 1, rule: 0, up: true })]).unwrap();
    let bytes = save(&opened, d.presentation(), "conditional-manage-identities.xlsx");
    let package = ibis_ooxml::Package::open(bytes).unwrap();
    let doc = package.xml(part).unwrap();
    let formats: Vec<_> = doc.children_named(doc.root_element().unwrap(), "conditionalFormatting").collect();
    for (cf, (priority, period)) in formats.into_iter().zip([("2", "last7Days"), ("1", "yesterday")]) {
        let r = doc.children_named(cf, "cfRule").next().unwrap();
        assert_eq!(doc.attr(r, "priority"), Some(priority)); assert_eq!(doc.attr(r, "timePeriod"), Some(period));
    }
}

#[test]
fn all_border_styles_colors_and_edges_save_and_undo() {
    use ibis_doc_model::{Border, BorderLine, Edge, Rgb};
    let (original, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    let edges = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom, Edge::DiagonalUp, Edge::DiagonalDown];
    let colors = [0x000000, 0xc00000, 0xff0000, 0xffc000, 0xffff00, 0x92d050, 0x00b050, 0x00b0f0, 0x0070c0, 0x002060, 0x7030a0, 0xffffff];
    let mut commands = vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "罫線全値".into(), slide: s, shape })];
    let mut expected = Vec::new();
    for line in BorderLine::ALL {
        for color in colors {
            for edge in edges {
                let row = expected.len() as u32 / 26;
                let column = expected.len() as u32 % 26;
                let range = CellRange { first_row: row, last_row: row, first_column: column, last_column: column };
                let border = Border { line, color: Rgb(color) };
                commands.push(Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![range], change: StyleChange::Border(edge, Some(border)) }));
                expected.push((row, column, edge, border));
            }
        }
    }
    assert_eq!(expected.len(), 13 * 12 * 6);
    d.execute(commands).unwrap();
    let bytes = save(&opened, d.presentation(), "border-styles-colors-edges.xlsx");
    let reloaded = ibis_pptx::read(&bytes).unwrap();
    let sheet = sheet_of_slide(reloaded.slide(slide(&reloaded, "罫線全値")).unwrap()).unwrap();
    for (row, column, edge, expected) in expected {
        let style = &sheet.styles[sheet.cell(row, column).unwrap().style as usize];
        let border = match edge { Edge::Left => style.left, Edge::Right => style.right, Edge::Top => style.top,
            Edge::Bottom => style.bottom, Edge::DiagonalUp => style.diagonal_up, Edge::DiagonalDown => style.diagonal_down };
        assert_eq!(border, Some(expected), "row {row} edge {edge:?}");
    }
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
    assert!(d.redo());
    assert_eq!(save(&opened, d.presentation(), "border-styles-colors-edges-redo.xlsx"), bytes);
}

#[test]
fn all_alignment_values_survive_format_save_undo_redo() {
    use ibis_doc_model::{CellAlign, CellVerticalAlign};
    let (original, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    let horizontal = [CellAlign::General, CellAlign::Left, CellAlign::Center, CellAlign::Right, CellAlign::Fill, CellAlign::Justify, CellAlign::CenterAcross, CellAlign::Distributed];
    let vertical = [CellVerticalAlign::Top, CellVerticalAlign::Center, CellVerticalAlign::Bottom, CellVerticalAlign::Justify, CellVerticalAlign::Distributed];
    let mut commands = vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "配置全値".into(), slide: s, shape })];
    for (row, h) in horizontal.into_iter().enumerate() {
        for (column, v) in vertical.into_iter().enumerate() {
            let range = CellRange { first_row: row as u32, last_row: row as u32, first_column: column as u32, last_column: column as u32 };
            commands.push(Command::Sheet(SheetCommand::SetCells { slide: s, edits: vec![CellEdit { row: row as u32, column: column as u32, input: CellInput::Value(CellValue::Text("配置確認".into())) }] }));
            for change in [StyleChange::Align(h), StyleChange::Vertical(v)] {
                commands.push(Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![range], change }));
            }
        }
    }
    d.execute(commands).unwrap();
    let bytes = save(&opened, d.presentation(), "alignment-all-values.xlsx");
    let reloaded = ibis_pptx::read(&bytes).unwrap();
    let sheet = sheet_of_slide(reloaded.slide(slide(&reloaded, "配置全値")).unwrap()).unwrap();
    for (row, h) in horizontal.into_iter().enumerate() {
        for (column, v) in vertical.into_iter().enumerate() {
            let style = &sheet.styles[sheet.cell(row as u32, column as u32).unwrap().style as usize];
            assert_eq!((style.align, style.vertical), (h, v));
        }
    }
    assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
    assert!(d.redo()); assert_eq!(save(&opened, d.presentation(), "alignment-all-values-redo.xlsx"), bytes);
}

#[test]
fn all_underline_kinds_save_and_undo_redo() {
    use ibis_doc_model::CellUnderline;
    let (original, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    let mut commands = vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "下線全値".into(), slide: s, shape })];
    for (column, u) in CellUnderline::ALL.into_iter().enumerate() {
        let column = column as u32;
        commands.push(Command::Sheet(SheetCommand::SetCells { slide: s, edits: vec![CellEdit { row: 0, column, input: CellInput::Value(CellValue::Text("gyp 下線".into())) }] }));
        commands.push(Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![CellRange { first_row: 0, last_row: 0, first_column: column, last_column: column }], change: StyleChange::UnderlineKind(u) }));
    }
    d.execute(commands).unwrap();
    let bytes = save(&opened, d.presentation(), "underline-all-kinds.xlsx");
    let reloaded = ibis_pptx::read(&bytes).unwrap();
    let sheet = sheet_of_slide(reloaded.slide(slide(&reloaded, "下線全値")).unwrap()).unwrap();
    for (column, u) in CellUnderline::ALL.into_iter().enumerate() {
        assert_eq!(sheet.styles[sheet.cell(0, column as u32).unwrap().style as usize].underline, u);
    }
    assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
    assert!(d.redo()); assert_eq!(save(&opened, d.presentation(), "underline-all-kinds-redo.xlsx"), bytes);
}

#[test]
fn implicit_and_spill_operators_evaluate_save_and_undo_redo() {
    let (original, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    let formulas = [(0,1,"@A1:A3"), (1,1,"@A1:A3"), (2,1,"@A1:A3"), (3,1,"@A1:A3"), (0,3,"SEQUENCE(3)"), (0,4,"SUM(D1#)"), (1,4,"@D1#"), (2,4,"SINGLE(D1#)"), (0,5,"@SEQUENCE(3)")];
    let mut edits: Vec<_> = (0..3).map(|row| CellEdit { row, column: 0, input: CellInput::Value(CellValue::Number((row + 1) as f64 * 10.)) }).collect();
    edits.extend(formulas.iter().map(|&(row,column,formula)| CellEdit { row,column,input: CellInput::Formula(formula.into()) }));
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "演算子".into(), slide: s, shape }), Command::Sheet(SheetCommand::SetCells { slide: s, edits })]).unwrap();
    let verify = |p: &Presentation| {
        for row in 0..3 { assert_eq!(value(p,"演算子",row,1), CellValue::Number((row+1) as f64*10.)); }
        assert_eq!(value(p,"演算子",3,1),CellValue::Error(ibis_doc_model::CellError::Value));
        for (row,col,expected) in [(0,4,6.),(1,4,2.),(2,4,3.),(0,5,1.)] { assert_eq!(value(p,"演算子",row,col),CellValue::Number(expected)); }
        assert_eq!(value(p,"演算子",1,5),CellValue::Empty,"implicit array does not spill");
    };
    verify(d.presentation());
    let bytes = save(&opened,d.presentation(),"implicit-spill-operators.xlsx");
    let reloaded = ibis_pptx::read(&bytes).unwrap(); verify(&reloaded);
    assert_eq!(cell(&reloaded,"演算子",1,1).formula.as_deref(),Some("@A1:A3"));
    assert_eq!(cell(&reloaded,"演算子",1,4).formula.as_deref(),Some("@D1#"));
    let package = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let xml: String = package.names().filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).collect();
    assert!(xml.contains("_xlfn.SINGLE(A1:A3)") && xml.contains("_xlfn.ANCHORARRAY(D1)"));
    assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(d.redo()); verify(d.presentation()); assert_eq!(save(&opened,d.presentation(),"implicit-spill-operators-redo.xlsx"),bytes);
}

#[test]
fn external_functions_keep_cached_values_and_opaque_parts() {
    let (_, stock) = open("xlsx-sales.xlsx");
    let mut initial = Document::new(stock.presentation.clone()); let s = slide(initial.presentation(), "月次売上");
    set(&mut initial,s,vec![(9,0,CellInput::Value(CellValue::Number(1.))),(9,1,CellInput::Formula("7".into())),(10,1,CellInput::Formula("11".into())),(11,1,CellInput::Formula("NA()".into())),(9,2,CellInput::Formula("SUM(B10:B11)".into()))]);
    let mut package=ibis_ooxml::Package::open(stock.source.save(initial.presentation(),SaveOptions::default()).unwrap()).unwrap();
    let part="xl/worksheets/sheet1.xml";
    let doc=package.xml_mut(part).unwrap(); let root=doc.root_element().unwrap(); doc.set_attr(root,"xmlns:ibis","urn:ibis:cache-test");
    let data=doc.child(root,"sheetData").unwrap();
    for (address,formula) in [("B10","_xlfn.FIELDVALUE(A10,\"Price\")"),("B11","_xlfn.FIELDVALUE(A10,\"Volume\")"),("B12","_xlfn.FIELDVALUE(A10,\"Missing\")")] {
        let c=doc.children_named(data,"row").flat_map(|r|doc.children_named(r,"c")).find(|&c|doc.attr(c,"r")==Some(address)).unwrap();
        let f=doc.child(c,"f").unwrap(); doc.set_text(f,formula); doc.set_attr(f,"ibis:sentinel",address);
        if address=="B12" {doc.set_attr(c,"t","str");let v=doc.child(c,"v").unwrap();doc.set_text(v,"#FIELD!");}
    }
    let opaque="xl/ibis-external-cache-test.bin"; let payload=b"IBIS synthetic cache payload\0\xff".to_vec();
    package.set_bytes(opaque,payload.clone()); package.set_override(opaque,"application/octet-stream").unwrap();
    let original=package.save(Default::default()).unwrap();
    std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit/external-cache-synthetic-original.xlsx"),&original).unwrap();
    let opened=ibis_pptx::open(original.clone()).unwrap(); let mut d=Document::new(opened.presentation.clone()); let s=slide(d.presentation(),"月次売上");
    assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
    let verify=|p:&Presentation| {
        for (row,expected) in [(9,CellValue::Number(7.)),(10,CellValue::Number(11.)),(11,CellValue::Error(ibis_doc_model::CellError::Field))] {assert_eq!(value(p,"月次売上",row,1),expected);}
        assert_eq!(value(p,"月次売上",9,2),CellValue::Number(18.),"dependents use the retained cache, not #NAME?");
    };
    set(&mut d,s,vec![(9,0,CellInput::Value(CellValue::Number(2.)))]); verify(d.presentation());
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat {slide:s,ranges:vec![CellRange {first_row:9,last_row:11,first_column:1,last_column:1}],change:StyleChange::Bold(true)})]).unwrap();
    let bytes=save(&opened,d.presentation(),"external-cache-preserved.xlsx"); verify(&ibis_pptx::read(&bytes).unwrap());
    let saved=ibis_ooxml::Package::open(bytes.clone()).unwrap(); assert_eq!(saved.read(opaque).unwrap(),payload);
    let xml=String::from_utf8(saved.read(part).unwrap()).unwrap();
    for address in ["B10","B11","B12"] {assert!(xml.contains(&format!("ibis:sentinel=\"{address}\"")));}
    assert!(xml.contains("_xlfn.FIELDVALUE"));
    assert!(d.undo()); assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(d.redo()); assert!(d.redo()); verify(d.presentation()); assert_eq!(save(&opened,d.presentation(),"external-cache-preserved-redo.xlsx"),bytes);
}

#[test]
fn flatten_array_options_save_and_undo_redo() {
    let (original, opened)=open("xlsx-sales.xlsx");
    let mut d=Document::new(opened.presentation.clone());
    let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let mut edits=vec![];
    for (row,column,input) in [(0,0,CellValue::Number(1.)),(0,2,CellValue::Error(ibis_doc_model::CellError::NA)),(1,0,CellValue::Number(2.)),(1,1,CellValue::Number(0.)),(1,2,CellValue::Text(String::new()))] {
        edits.push(CellEdit {row,column,input:CellInput::Value(input)});
    }
    for (f,function) in ["TOCOL","TOROW"].into_iter().enumerate() {
        for ignore in 0..=3 { for scan in 0..=1 {
            edits.push(CellEdit {row:5+(f*8+ignore*2+scan) as u32*8,column:0,input:CellInput::Formula(format!("{function}(A1:C2,{ignore},{})",scan==1))});
        }}
        edits.push(CellEdit {row:140+f as u32,column:0,input:CellInput::Formula(format!("{function}(A1:C2,0,NA())"))});
    }
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"平坦化".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"flatten-array-options.xlsx");
    assert_reads_back(&bytes,d.presentation());
    let p=ibis_pptx::read(&bytes).unwrap();
    for row in [140,141] {assert_eq!(value(&p,"平坦化",row,0),CellValue::Error(ibis_doc_model::CellError::NA));}
    let package=ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let xml:String=package.names().filter(|n|n.starts_with("xl/worksheets/sheet")&&n.ends_with(".xml")).map(|n|String::from_utf8(package.read(n).unwrap()).unwrap()).collect();
    assert!(xml.contains("_xlfn.TOCOL(")&&xml.contains("_xlfn.TOROW("));
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"flatten-array-options-redo.xlsx"),bytes);
}

#[test]
fn weekend_options_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());
    let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];
    for mask in 0u32..128 {
        let text:String=(0..7).map(|day|if mask&(1<<day)==0 {'0'}else{'1'}).collect();
        for (column,formula) in [(0,format!("NETWORKDAYS.INTL(DATE(2024,1,1),DATE(2024,1,14),\"{text}\")")),(1,format!("WORKDAY.INTL(DATE(2024,1,1),1,\"{text}\")"))] {edits.push(CellEdit {row:mask,column,input:CellInput::Formula(formula)});}
    }
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"週末指定".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"weekend-options.xlsx");assert_reads_back(&bytes,d.presentation());
    let p=ibis_pptx::read(&bytes).unwrap();assert_eq!(value(&p,"週末指定",127,0),CellValue::Number(0.));assert_eq!(value(&p,"週末指定",127,1),CellValue::Error(ibis_doc_model::CellError::Value));
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"weekend-options-redo.xlsx"),bytes);
}

#[test]
fn datedif_units_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());
    let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];
    let units=[("Y",1.),("M",14.),("D",440.),("MD",14.),("YM",2.),("YD",75.)];
    for (row,(unit,_)) in units.into_iter().enumerate() {edits.push(CellEdit {row:row as u32,column:0,input:CellInput::Formula(format!("DATEDIF(DATE(2001,6,1),DATE(2002,8,15),\"{unit}\")"))});}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"日付差".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"datedif-units.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();
    for (row,(_,expected)) in units.into_iter().enumerate(){assert_eq!(value(&p,"日付差",row as u32,0),CellValue::Number(expected));}
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"datedif-units-redo.xlsx"),bytes);
}

#[test]
fn information_options_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());
    let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let formulas=["INFO(1)","INFO(\"invalid\")","CELL(1,A1)","CELL(\"invalid\",A1)","INFO(\"memavail\")","INFO(\"memused\")","INFO(\"totmem\")"];
    let edits=formulas.iter().enumerate().map(|(row,f)|CellEdit {row:row as u32,column:1,input:CellInput::Formula((*f).into())}).collect();
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"情報引数".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"information-options.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();
    for row in 0..7 {assert_eq!(value(&p,"情報引数",row,1),CellValue::Error(if row<4 {ibis_doc_model::CellError::Value}else{ibis_doc_model::CellError::NA}));}
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"information-options-redo.xlsx"),bytes);
}

#[test]
fn valid_information_options_keep_imported_cache() {
    let (_,stock)=open("xlsx-sales.xlsx");let mut initial=Document::new(stock.presentation.clone());let s=slide(initial.presentation(),"月次売上");
    set(&mut initial,s,vec![(9,0,CellInput::Value(CellValue::Number(1.))),(9,1,CellInput::Formula("\"F2\"".into())),(10,1,CellInput::Formula("\"cached-version\"".into()))]);
    let mut package=ibis_ooxml::Package::open(stock.source.save(initial.presentation(),SaveOptions::default()).unwrap()).unwrap();
    let doc=package.xml_mut("xl/worksheets/sheet1.xml").unwrap();let data=doc.child(doc.root_element().unwrap(),"sheetData").unwrap();
    for (address,formula) in [("B10","INFO(\"directory\")"),("B11","INFO(\"release\")")] {
        let c=doc.children_named(data,"row").flat_map(|r|doc.children_named(r,"c")).find(|&c|doc.attr(c,"r")==Some(address)).unwrap();let f=doc.child(c,"f").unwrap();doc.set_text(f,formula);
    }
    let original=package.save(Default::default()).unwrap();
    std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit/info-valid-cache-original.xlsx"),&original).unwrap();
    let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let s=slide(d.presentation(),"月次売上");
    let book=ibis_doc_model::sheet_calc::BookView::new(d.presentation());
    for (row,formula) in [(9,"INFO(\"directory\")"),(10,"INFO(\"release\")")] {assert!(ibis_calc::recalc::evaluate(&book,(0,row,1),formula).1,"valid unsupported options must retain caches");}
    set(&mut d,s,vec![(9,0,CellInput::Value(CellValue::Number(2.)))]);
    let verify=|p:&Presentation|{assert_eq!(value(p,"月次売上",9,1),CellValue::Text("F2".into()));assert_eq!(value(p,"月次売上",10,1),CellValue::Text("cached-version".into()));};
    verify(d.presentation());let bytes=save(&opened,d.presentation(),"info-valid-cache-preserved.xlsx");verify(&ibis_pptx::read(&bytes).unwrap());
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"info-valid-cache-redo.xlsx"),bytes);
}

#[test]
fn sort_options_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];
    let mut row=0;for index in [1,2] {for order in [-1,1] {for by in [false,true] {
        edits.push(CellEdit {row,column:0,input:CellInput::Formula(format!("SORT({{4,1;2,3}},{index},{order},{by})"))});row+=3;
    }}}
    edits.push(CellEdit {row,column:0,input:CellInput::Formula("SORT({4,1;2,3},1,1,NA())".into())});
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"配列ソート".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"sort-options.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();assert_eq!(value(&p,"配列ソート",24,0),CellValue::Error(ibis_doc_model::CellError::NA));
    let package=ibis_ooxml::Package::open(bytes.clone()).unwrap();let xml:String=package.names().filter(|n|n.starts_with("xl/worksheets/sheet")&&n.ends_with(".xml")).map(|n|String::from_utf8(package.read(n).unwrap()).unwrap()).collect();assert!(xml.contains("_xlfn._xlws.SORT("));
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"sort-options-redo.xlsx"),bytes);
}

#[test]
fn sort_errors_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let edits=vec![CellEdit {row:0,column:0,input:CellInput::Formula("SORT({#N/A;#DIV/0!;#VALUE!})".into())},CellEdit {row:0,column:2,input:CellInput::Formula("SORT({#N/A;2;#DIV/0!;1;#VALUE!})".into())}];
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"エラー順".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"sort-errors.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();
    for (row,error) in [ibis_doc_model::CellError::Div0,ibis_doc_model::CellError::Value,ibis_doc_model::CellError::NA].into_iter().enumerate(){assert_eq!(value(&p,"エラー順",row as u32,0),CellValue::Error(error));}
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"sort-errors-redo.xlsx"),bytes);
}

#[test]
fn textsplit_options_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];let mut row=0;
    for ignore in [false,true] {for mode in [0,1] {for custom in [false,true] {
        let pad=if custom {",\"pad\""}else{""};edits.push(CellEdit {row,column:0,input:CellInput::Formula(format!("TEXTSPLIT(\"aXbxxc|dXe\",\"x\",\"|\",{ignore},{mode}{pad})"))});row+=3;
    }}}
    for formula in ["TEXTSPLIT(\"a,b\",\",\",,NA())","TEXTSPLIT(\"a,b\",\",\",,FALSE,-1)"] {edits.push(CellEdit {row,column:0,input:CellInput::Formula(formula.into())});row+=1;}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"分割引数".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"textsplit-options.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();assert_eq!(value(&p,"分割引数",24,0),CellValue::Error(ibis_doc_model::CellError::NA));assert_eq!(value(&p,"分割引数",25,0),CellValue::Error(ibis_doc_model::CellError::Value));
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"textsplit-options-redo.xlsx"),bytes);
}

#[test]
fn textsplit_shape_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let formulas=[(0,"TEXTSPLIT(\"Hello World\",\"Hello World\",\"Hello World\")"),(3,"TEXTSPLIT(\"a,b,c\",\",\",\",\")"),(6,"TEXTSPLIT(\"m\",\",\",,FALSE,0,{1,2;3,4})"),(9,"TEXTSPLIT(\"a,b|c\",\",\",\"|\",FALSE,0,{9})"),(12,"TEXTSPLIT(\"m\",\",\",,FALSE,0,C1:C3)")];
    let edits=formulas.into_iter().map(|(row,f)|CellEdit {row,column:0,input:CellInput::Formula(f.into())}).collect();d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"分割形状".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"textsplit-shape.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();assert_eq!(value(&p,"分割形状",0,1),CellValue::Text(String::new()));assert_eq!(value(&p,"分割形状",6,0),CellValue::Error(ibis_doc_model::CellError::Value));assert_eq!(value(&p,"分割形状",10,1),CellValue::Number(9.));assert_eq!(value(&p,"分割形状",12,0),CellValue::Text("m".into()));
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"textsplit-shape-redo.xlsx"),bytes);
}

#[test]
fn unicode_delimiters_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];
    let prefixes=["K","İ","日本","🙂","ΟΣ|"];
    for (i,prefix) in prefixes.into_iter().enumerate() {
        for (row,column,formula) in [(i as u32*4,0,format!("TEXTBEFORE(\"{prefix}Xtail\",\"x\",1,1)")),(i as u32*4,1,format!("TEXTAFTER(\"{prefix}Xtail\",\"x\",1,1)")),(i as u32*4+1,0,format!("TEXTSPLIT(\"{prefix}Xtail\",\"x\",,FALSE,1)"))] {edits.push(CellEdit {row,column,input:CellInput::Formula(formula)});}
    }
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"Unicode位置".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"unicode-delimiters.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();
    for (i,prefix) in prefixes.into_iter().enumerate(){for row in [i as u32*4,i as u32*4+1]{assert_eq!(value(&p,"Unicode位置",row,0),CellValue::Text(prefix.into()));assert_eq!(value(&p,"Unicode位置",row,1),CellValue::Text("tail".into()));}}
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"unicode-delimiters-redo.xlsx"),bytes);
}

#[test]
fn subtotal_literals_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let source=["LEN(\"SUBTOTAL(\")","LEN(\"AGGREGATE(\")","SUBTOTAL(9,A1:A2)","AGGREGATE(9,0,A1:A2)"];
    let checks=[("SUBTOTAL(9,A1:A4)",19.0),("SUBTOTAL(3,A1:A4)",2.0),("AGGREGATE(9,0,A1:A4)",19.0),("AGGREGATE(3,0,A1:A4)",2.0),("AGGREGATE(9,4,A1:A4)",57.0)];
    let mut edits:Vec<_>=source.into_iter().enumerate().map(|(row,f)|CellEdit {row:row as u32,column:0,input:CellInput::Formula(f.into())}).collect();
    edits.extend(checks.iter().enumerate().map(|(row,(f,_))|CellEdit {row:row as u32,column:1,input:CellInput::Formula((*f).into())}));
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"入れ子除外".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"subtotal-literals.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();
    for (row,(_,expected)) in checks.into_iter().enumerate(){assert_eq!(value(&p,"入れ子除外",row as u32,1),CellValue::Number(expected));}
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"subtotal-literals-redo.xlsx"),bytes);
}

#[test]
fn subtotal_count_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let mut edits=vec![CellEdit {row:0,column:0,input:CellInput::Value(CellValue::Error(ibis_doc_model::CellError::NA))},CellEdit {row:1,column:0,input:CellInput::Value(CellValue::Number(7.0))},CellEdit {row:2,column:0,input:CellInput::Value(CellValue::Text("text".into()))}];
    for (row,code) in [2,102,3,103,9].into_iter().enumerate(){edits.push(CellEdit {row:row as u32,column:1,input:CellInput::Formula(format!("SUBTOTAL({code},A1:A3)"))});}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"集計個数".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let bytes=save(&opened,d.presentation(),"subtotal-count.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();
    for (row,n) in [1.0,1.0,3.0,3.0].into_iter().enumerate(){assert_eq!(value(&p,"集計個数",row as u32,1),CellValue::Number(n));}assert_eq!(value(&p,"集計個数",4,1),CellValue::Error(ibis_doc_model::CellError::NA));
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"subtotal-count-redo.xlsx"),bytes);
}

#[test]
fn subtotal_hidden_save_and_undo_redo() {
    let (_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let mut edits:Vec<_>=[2.0,4.0,6.0].into_iter().enumerate().map(|(row,n)|CellEdit {row:row as u32,column:0,input:CellInput::Value(CellValue::Number(n))}).collect();
    for (i,code) in (1..=11).chain(101..=111).enumerate(){edits.push(CellEdit {row:(i%11) as u32,column:(1+i/11) as u32,input:CellInput::Formula(format!("SUBTOTAL({code},A1:A3)"))});}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"非表示集計".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let shown=save(&opened,d.presentation(),"subtotal-shown.xlsx");
    d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:ibis_doc_model::sheet_command::Axis::Rows,first:1,last:1,hidden:true})]).unwrap();
    let bytes=save(&opened,d.presentation(),"subtotal-hidden.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();assert_eq!(value(&p,"非表示集計",8,1),CellValue::Number(12.0));assert_eq!(value(&p,"非表示集計",8,2),CellValue::Number(8.0));assert_eq!(sheet_of_slide(p.slide(slide(&p,"非表示集計")).unwrap()).unwrap().rows[1],0);
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),shown);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"subtotal-hidden-redo.xlsx"),bytes);
}

#[test]
fn aggregate_enums_save_and_undo_redo() {
    let (_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let mut edits:Vec<_>=[2.0,4.0,4.0,6.0].into_iter().enumerate().map(|(row,n)|CellEdit {row:row as u32,column:0,input:CellInput::Value(CellValue::Number(n))}).collect();
    for function in 1..=19{for option in 0..8{let k=match function{14|15=>",2",16|18=>",0.25",17|19=>",1",_=>""};edits.push(CellEdit {row:function-1,column:option+1,input:CellInput::Formula(format!("AGGREGATE({function},{option},A1:A4{k})"))});}}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"集計全選択肢".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let shown=save(&opened,d.presentation(),"aggregate-enums-shown.xlsx");assert_reads_back(&shown,d.presentation());
    d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:ibis_doc_model::sheet_command::Axis::Rows,first:0,last:0,hidden:true})]).unwrap();
    let bytes=save(&opened,d.presentation(),"aggregate-enums-hidden.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for option in 0..8{assert_eq!(value(&p,"集計全選択肢",8,option+1),CellValue::Number(if option%2==1{14.0}else{16.0}));}
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),shown);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"aggregate-enums-redo.xlsx"),bytes);
}

#[test]
fn subtotal_filter_save_and_undo_redo() {
    let (_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let mut edits=vec![CellEdit {row:0,column:0,input:CellInput::Value(CellValue::Text("header".into()))}];for (row,n) in [(1,2.0),(2,4.0),(3,6.0)]{edits.push(CellEdit {row,column:0,input:CellInput::Value(CellValue::Number(n))});}
    for (row,code) in [9,109,2,102,3,103].into_iter().enumerate(){edits.push(CellEdit {row:row as u32,column:1,input:CellInput::Formula(format!("SUBTOTAL({code},A2:A4)"))});}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"フィルター集計".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let shown=save(&opened,d.presentation(),"subtotal-filter-shown.xlsx");
    d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:ibis_doc_model::sheet_command::Axis::Rows,first:2,last:2,hidden:true}),Command::Sheet(SheetCommand::SetAutoFilter {slide:s,filter:Some(ibis_doc_model::AutoFilter {range:CellRange {first_row:0,last_row:3,first_column:0,last_column:0},columns:vec![(0,ibis_doc_model::ColumnFilter::Values(vec!["4".into(),"6".into()]))]})})]).unwrap();
    let bytes=save(&opened,d.presentation(),"subtotal-filter.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for (row,n) in [10.0,6.0,2.0,1.0,2.0,1.0].into_iter().enumerate(){assert_eq!(value(&p,"フィルター集計",row as u32,1),CellValue::Number(n));}let sh=sheet_of_slide(p.slide(slide(&p,"フィルター集計")).unwrap()).unwrap();assert_eq!(sh.filtered_rows.iter().copied().collect::<Vec<_>>(),vec![1]);
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),shown);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"subtotal-filter-redo.xlsx"),bytes);
}

#[test]
fn jis_name_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];
    let cases=[("abc","ａｂｃ"),("ｱｲｳ ABC","アイウ　ＡＢＣ"),("ｶﾞﾊﾟｳﾞ","ガパウ゛"),("日本語ＡＢＣ","日本語ＡＢＣ"),("ｶA🙂","カＡ🙂"),("","")];
    for (row,(input,_)) in cases.into_iter().enumerate(){edits.push(CellEdit {row:row as u32,column:0,input:CellInput::Value(CellValue::Text(input.into()))});for (column,name) in [(1,"JIS"),(2,"DBCS")]{edits.push(CellEdit {row:row as u32,column,input:CellInput::Formula(format!("{name}(A{})",row+1))});}}
    for (row,formula) in [(6,"JIS(123)"),(7,"JIS(NA())"),(8,"JIS()"),(9,"JIS(\"a\",\"b\")")]{edits.push(CellEdit {row,column:1,input:CellInput::Formula(formula.into())});}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"全角名".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let bytes=save(&opened,d.presentation(),"jis-name.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();
    for (row,(_,expected)) in cases.into_iter().enumerate(){for column in [1,2]{assert_eq!(value(&p,"全角名",row as u32,column),CellValue::Text(expected.into()));}}assert_eq!(cell(&p,"全角名",0,1).formula.as_deref(),Some("JIS(A1)"));assert_eq!(cell(&p,"全角名",0,2).formula.as_deref(),Some("DBCS(A1)"));
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"jis-name-redo.xlsx"),bytes);
}

#[test]
fn aggregate_resize_save_and_undo_redo() {
    let (_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits:Vec<_>=[2.0,4.0,6.0].into_iter().enumerate().map(|(row,n)|CellEdit {row:row as u32,column:0,input:CellInput::Value(CellValue::Number(n))}).collect();
    for (row,f) in ["SUBTOTAL(9,A1:A3)","SUBTOTAL(109,A1:A3)","AGGREGATE(9,1,A1:A3)","AGGREGATE(9,4,A1:A3)","B2*2","SUM(A1:A3)"].into_iter().enumerate(){edits.push(CellEdit {row:row as u32,column:1,input:CellInput::Formula(f.into())});}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"高さ集計".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let shown=save(&opened,d.presentation(),"aggregate-resize-shown.xlsx");
    d.execute(vec![Command::Sheet(SheetCommand::SetSize {slide:s,axis:ibis_doc_model::sheet_command::Axis::Rows,first:1,last:1,size:0})]).unwrap();let bytes=save(&opened,d.presentation(),"aggregate-resize-hidden.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for (row,n) in [12.0,8.0,8.0,12.0,16.0,12.0].into_iter().enumerate(){assert_eq!(value(&p,"高さ集計",row as u32,1),CellValue::Number(n));}
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),shown);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"aggregate-resize-redo.xlsx"),bytes);
}

#[test]
fn wrap_count_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];let mut checks=vec![];let mut row=0;
    for function in ["WRAPROWS","WRAPCOLS"]{for vector in ["{1,2}","{1;2}"]{for pad in ["",",\"pad\""]{edits.push(CellEdit {row,column:0,input:CellInput::Formula(format!("{function}({vector},5{pad})"))});checks.push((row,if function=="WRAPROWS"{(row,1)}else{(row+1,0)}));row+=3;}}}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"折返し最大数".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let bytes=save(&opened,d.presentation(),"wrap-count.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for (row,(r,c)) in checks{assert_eq!(value(&p,"折返し最大数",row,0),CellValue::Number(1.0));assert_eq!(value(&p,"折返し最大数",r,c),CellValue::Number(2.0));}assert_eq!(sheet_of_slide(p.slide(slide(&p,"折返し最大数")).unwrap()).unwrap().cells.len(),16);
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"wrap-count-redo.xlsx"),bytes);
}

#[test]
fn expand_options_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];let mut row=0;
    for rows in [2,3]{for columns in [2,3]{for pad in ["",",0",",\"\"",",\"pad\""]{edits.push(CellEdit {row,column:0,input:CellInput::Formula(format!("EXPAND({{1,2;3,4}},{rows},{columns}{pad})"))});row+=4;}}}
    for f in ["EXPAND({1,2},1E20,2)","EXPAND({1;2},2,1E20)","EXPAND(1,1E20,1E20)"]{edits.push(CellEdit {row,column:0,input:CellInput::Formula(f.into())});row+=1;}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"配列拡張".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let bytes=save(&opened,d.presentation(),"expand-options.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for row in 64..67{assert_eq!(value(&p,"配列拡張",row,0),CellValue::Error(ibis_doc_model::CellError::Num));}
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"expand-options-redo.xlsx"),bytes);
}

#[test]
fn regex_lookup_keeps_imported_cache() {
    let (_,stock)=open("xlsx-sales.xlsx");let mut initial=Document::new(stock.presentation.clone());let s=slide(initial.presentation(),"月次売上");let mut edits=vec![(9,0,CellInput::Value(CellValue::Text("a.c".into())))];for row in 9..13{edits.push((row,1,CellInput::Formula("77".into())));}set(&mut initial,s,edits);
    let formulas=["XLOOKUP(A10,{\"abc\",\"a.c\"},{10,20},99,3,1)","XLOOKUP(A10,{\"abc\",\"a.c\"},{10,20},99,3,-1)","XMATCH(A10,{\"abc\",\"a.c\"},3,1)","XMATCH(A10,{\"abc\",\"a.c\"},3,-1)"];
    let mut package=ibis_ooxml::Package::open(stock.source.save(initial.presentation(),SaveOptions::default()).unwrap()).unwrap();let xml=package.xml_mut("xl/worksheets/sheet1.xml").unwrap();let data=xml.child(xml.root_element().unwrap(),"sheetData").unwrap();for (i,f) in formulas.iter().enumerate(){let address=format!("B{}",i+10);let c=xml.children_named(data,"row").flat_map(|r|xml.children_named(r,"c")).find(|&c|xml.attr(c,"r")==Some(address.as_str())).unwrap();let node=xml.child(c,"f").unwrap();xml.set_text(node,f);}
    let original=package.save(Default::default()).unwrap();let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let s=slide(d.presentation(),"月次売上");set(&mut d,s,vec![(9,0,CellInput::Value(CellValue::Text("ab.*".into())))]);
    let expected=[CellValue::Number(10.0),CellValue::Number(10.0),CellValue::Number(1.0),CellValue::Number(1.0)];
    let verify=|p:&Presentation|{for (i,exp) in expected.iter().enumerate(){let c=cell(p,"月次売上",i as u32+9,1);assert_eq!(&c.value,exp);assert_eq!(c.formula.as_deref(),Some(formulas[i]));}};verify(d.presentation());let bytes=save(&opened,d.presentation(),"regex-lookup-cache.xlsx");verify(&ibis_pptx::read(&bytes).unwrap());assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"regex-lookup-cache-redo.xlsx"),bytes);
}

#[test]
fn xlookup_dimension_mismatch_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];let mut row=0;for (keys,returns) in [("{1;2}","{10;20;30}"),("{1,2}","{10,20,30}")]{for key in [1,9]{for mode in [-1,0,1]{for search in [-1,1]{edits.push(CellEdit {row,column:0,input:CellInput::Formula(format!("XLOOKUP({key},{keys},{returns},99,{mode},{search})"))});row+=1;}}}}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"検索寸法".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let bytes=save(&opened,d.presentation(),"xlookup-shape.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for row in 0..24{assert_eq!(value(&p,"検索寸法",row,0),CellValue::Error(ibis_doc_model::CellError::Value));}assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"xlookup-shape-redo.xlsx"),bytes);
}

#[test]
fn xlookup_empty_key_save_and_undo_redo() {
    let (_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"空検索".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits:vec![CellEdit {row:0,column:1,input:CellInput::Formula("XLOOKUP(A1,{1;2;3},{2.1;3.9;6.2},99,1,1)".into())}]})]).unwrap();let blank=save(&opened,d.presentation(),"xlookup-empty-blank.xlsx");assert_eq!(value(&ibis_pptx::read(&blank).unwrap(),"空検索",0,1),CellValue::Number(99.0));
    set(&mut d,s,vec![(0,0,CellInput::Value(CellValue::Number(0.0)))]);let zero=save(&opened,d.presentation(),"xlookup-empty-zero.xlsx");assert_reads_back(&zero,d.presentation());assert_eq!(value(&ibis_pptx::read(&zero).unwrap(),"空検索",0,1),CellValue::Number(2.1));assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),blank);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"xlookup-empty-redo.xlsx"),zero);
}

#[test]
fn sortby_key_shape_and_orders_save() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];let mut checks=vec![];let mut row=0;
    for (source,valid,bad) in [("{1;2;3;4}","{1;2;3;4}","{1,2;3,4}"),("{1,2,3,4}","{1,2,3,4}","{1,2;3,4}"),("{1;2;3;4}","{1;2;3;4}","{1,2,3,4}"),("{1,2,3,4}","{1,2,3,4}","{1;2;3;4}")]{for position in 0..3{let keys=(0..3).map(|i|if i==position{bad}else{valid}).collect::<Vec<_>>();edits.push(CellEdit {row,column:0,input:CellInput::Formula(format!("SORTBY({source},{},1,{},1,{},1)",keys[0],keys[1],keys[2]))});checks.push((row,0,CellValue::Error(ibis_doc_model::CellError::Value)));row+=1;}}
    for sep in [";",","]{for (a,b,expected) in [(1,1,[4.,2.,1.,3.]),(1,-1,[2.,4.,3.,1.]),(-1,1,[1.,3.,4.,2.]),(-1,-1,[3.,1.,2.,4.])]{let src=format!("{{{}}}",["1","2","3","4"].join(sep));let k1=format!("{{{}}}",["2","1","2","1"].join(sep));let k2=format!("{{{}}}",["1","2","2","1"].join(sep));edits.push(CellEdit {row,column:0,input:CellInput::Formula(format!("SORTBY({src},{k1},{a},{k2},{b})"))});for (i,n) in expected.into_iter().enumerate(){checks.push((if sep==";"{row+i as u32}else{row},if sep==";"{0}else{i as u32},CellValue::Number(n)));}row+=5;}}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"複数検索順".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let bytes=save(&opened,d.presentation(),"sortby-shape-orders.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for (r,c,v) in checks{assert_eq!(value(&p,"複数検索順",r,c),v);}assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"sortby-shape-orders-redo.xlsx"),bytes);
}

#[test]
fn filter_empty_include_save_and_undo_redo() {
    let (_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"空条件".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits:vec![CellEdit {row:0,column:0,input:CellInput::Value(CellValue::Number(10.0))},CellEdit {row:1,column:0,input:CellInput::Value(CellValue::Number(20.0))},CellEdit {row:2,column:0,input:CellInput::Value(CellValue::Number(30.0))},CellEdit {row:0,column:2,input:CellInput::Formula("FILTER(A1:A3,B1:B3,99)".into())}]})]).unwrap();let blank=save(&opened,d.presentation(),"filter-empty-blank.xlsx");assert_eq!(value(&ibis_pptx::read(&blank).unwrap(),"空条件",0,2),CellValue::Number(99.0));set(&mut d,s,vec![(1,1,CellInput::Value(CellValue::Bool(true)))]);let selected=save(&opened,d.presentation(),"filter-empty-selected.xlsx");assert_reads_back(&selected,d.presentation());assert_eq!(value(&ibis_pptx::read(&selected).unwrap(),"空条件",0,2),CellValue::Number(20.0));assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),blank);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"filter-empty-redo.xlsx"),selected);
}

#[test]
fn arraytotext_empty_arrays_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];let mut row=0;for source in ["TAKE({1;2},0)","FILTER({1;2},{FALSE;FALSE})"]{edits.push(CellEdit {row,column:0,input:CellInput::Formula(source.into())});for (i,suffix) in ["",",0",",1"].into_iter().enumerate(){edits.push(CellEdit {row,column:i as u32+1,input:CellInput::Formula(format!("ARRAYTOTEXT({source}{suffix})"))});}row+=1;}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"空配列文字".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let bytes=save(&opened,d.presentation(),"arraytotext-empty.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for col in 0..4{assert_eq!(value(&p,"空配列文字",0,col),CellValue::Error(ibis_doc_model::CellError::Value));}assert_eq!(value(&p,"空配列文字",1,0),CellValue::Error(ibis_doc_model::CellError::Calc));for col in 1..4{assert_eq!(value(&p,"空配列文字",1,col),CellValue::Error(ibis_doc_model::CellError::Value));}assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"arraytotext-empty-redo.xlsx"),bytes);
}

#[test]
fn choose_lines_options_save_and_undo_redo() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];let mut checks=vec![];let mut row=0;
    let cases=[("1",vec![1]),("-1",vec![3]),("2.9",vec![2]),("{3,1,3}",vec![3,1,3]),("{3;1;3}",vec![3,1,3]),("{3,1;2,3}",vec![3,1,2,3])];for name in ["CHOOSEROWS","CHOOSECOLS"]{for (input,lines) in &cases{edits.push(CellEdit {row,column:0,input:CellInput::Formula(format!("{name}({{1,2,3;4,5,6;7,8,9}},{input})"))});for (out,line) in lines.iter().enumerate(){for cross in 0..3{let (r,c,n)=if name=="CHOOSEROWS"{(row+out as u32,cross,((*line-1)*3+cross+1) as f64)}else{(row+cross,out as u32,(cross*3+*line) as f64)};checks.push((r,c,CellValue::Number(n)));}}row+=5;}
    for (index,e) in [("0",ibis_doc_model::CellError::Value),("4",ibis_doc_model::CellError::Value),("-4",ibis_doc_model::CellError::Value),("NA()",ibis_doc_model::CellError::NA),("{0,#N/A}",ibis_doc_model::CellError::NA),("{#N/A,0}",ibis_doc_model::CellError::NA)]{edits.push(CellEdit {row,column:0,input:CellInput::Formula(format!("{name}({{1,2,3;4,5,6;7,8,9}},{index})"))});checks.push((row,0,CellValue::Error(e)));row+=1;}}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"選択引数".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let bytes=save(&opened,d.presentation(),"choose-lines-options.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for (r,c,v) in checks{assert_eq!(value(&p,"選択引数",r,c),v);}assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"choose-lines-options-redo.xlsx"),bytes);
}

#[test]
fn multi_column_sort_options_save_and_undo_redo() {
    for header in [false,true]{for ascending in [false,true]{for second in [false,true]{let (_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut edits=vec![];let data=[("Group",CellValue::Text("Score".into()),"ID"),("b",CellValue::Number(2.),"r1"),("a",CellValue::Number(2.),"r2"),("b",CellValue::Number(1.),"r3"),("a",CellValue::Number(1.),"r4")];for (row,(a,b,id)) in data.into_iter().enumerate(){for (column,v) in [CellValue::Text(a.into()),b,CellValue::Text(id.into())].into_iter().enumerate(){edits.push(CellEdit {row:row as u32,column:column as u32,input:CellInput::Value(v)});}}d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"多段並替".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();let before=opened.source.save(d.presentation(),SaveOptions::default()).unwrap();
        let mut expected=match (ascending,second){(true,true)=>vec!["r4","r2","r3","r1"],(true,false)=>vec!["r2","r4","r1","r3"],(false,true)=>vec!["r3","r1","r4","r2"],(false,false)=>vec!["r1","r3","r2","r4"]};if header || !ascending{expected.insert(0,"ID");}else{expected.push("ID");}
        d.execute(vec![Command::Sheet(SheetCommand::Sort {slide:s,range:CellRange {first_row:0,last_row:4,first_column:0,last_column:2},keys:vec![(0,ascending),(1,second)],header})]).unwrap();let name=format!("sort-manage-{header}-{ascending}-{second}.xlsx");let bytes=save(&opened,d.presentation(),&name);assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();for (row,id) in expected.iter().enumerate(){assert_eq!(value(&p,"多段並替",row as u32,2),CellValue::Text((*id).into()));}assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),before);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    }}}
}

#[test]
fn sort_filtered_rows_retains_applied_state_after_save() {
    let (_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"並替フィルター".into(),slide:s,shape})]).unwrap();set(&mut d,s,vec![(0,0,CellInput::Value(CellValue::Text("数".into()))),(1,0,CellInput::Value(CellValue::Number(2.))),(2,0,CellInput::Value(CellValue::Number(4.))),(3,0,CellInput::Value(CellValue::Number(6.))),(0,2,CellInput::Formula("SUBTOTAL(9,A2:A4)".into()))]);let range=CellRange {first_row:0,last_row:3,first_column:0,last_column:0};d.execute(vec![Command::Sheet(SheetCommand::SetAutoFilter {slide:s,filter:Some(ibis_doc_model::AutoFilter {range,columns:vec![(0,ibis_doc_model::ColumnFilter::Values(vec!["4".into(),"6".into()]))]})})]).unwrap();let unsorted=opened.source.save(d.presentation(),SaveOptions::default()).unwrap();d.execute(vec![Command::Sheet(SheetCommand::Sort {slide:s,range,keys:vec![(0,false)],header:true})]).unwrap();let bytes=save(&opened,d.presentation(),"sort-filter-state.xlsx");assert_eq!(value(d.presentation(),"並替フィルター",1,0),CellValue::Number(2.0));assert_eq!(value(d.presentation(),"並替フィルター",2,0),CellValue::Number(6.0));assert_eq!(value(d.presentation(),"並替フィルター",3,0),CellValue::Number(4.0));assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),unsorted);assert!(d.redo());assert_eq!(save(&opened,d.presentation(),"sort-filter-state-redo.xlsx"),bytes);let p=ibis_pptx::read(&bytes).unwrap();let before=sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();let after=sheet_of_slide(p.slide(slide(&p,"並替フィルター")).unwrap()).unwrap();assert_eq!(before.filtered_rows,after.filtered_rows,"applied filtering must not change merely by saving");
    let mut reopened=Document::new(p);let rs=slide(reopened.presentation(),"並替フィルター");set(&mut d,s,vec![(3,0,CellInput::Value(CellValue::Number(3.)))]);set(&mut reopened,rs,vec![(3,0,CellInput::Value(CellValue::Number(3.)))]);assert_eq!(value(d.presentation(),"並替フィルター",0,2),value(reopened.presentation(),"並替フィルター",0,2),"the same edit must calculate equally before/after save");
}

#[test]
fn spill_structure_roundtrip_from_saved_source() {
    let (_,stock)=open("xlsx-sales.xlsx");let mut initial=Document::new(stock.presentation.clone());let (id,shape)=(initial.ids().new_slide_id(),initial.ids().new_shape_id());initial.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"配列構造".into(),slide:id,shape})]).unwrap();set(&mut initial,id,vec![(1,1,CellInput::Formula("SEQUENCE(3,3)".into())),(5,5,CellInput::Formula("SUM($B$2#)".into()))]);initial.execute(vec![Command::Sheet(SheetCommand::SetFormat {slide:id,ranges:vec![CellRange {first_row:2,last_row:2,first_column:2,last_column:2}],change:StyleChange::Bold(true)})]).unwrap();let original=save(&stock,initial.presentation(),"spill-structure-original.xlsx");
    for axis in [ibis_doc_model::sheet_command::Axis::Rows,ibis_doc_model::sheet_command::Axis::Columns]{for (insert,at) in [(true,0),(true,2),(false,1)]{let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let slide=slide(d.presentation(),"配列構造");let command=if insert{SheetCommand::Insert {slide,axis,at,count:1}}else{SheetCommand::Delete {slide,axis,at,count:1}};d.execute(vec![Command::Sheet(command)]).unwrap();let name=format!("spill-structure-{axis:?}-{insert}-{at}.xlsx");let bytes=save(&opened,d.presentation(),&name);assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();let model=sheet_of_slide(d.presentation().slide(slide).unwrap()).unwrap();let read=sheet_of_slide(p.slide(self::slide(&p,"配列構造")).unwrap()).unwrap();assert_eq!(read.spills,model.spills,"{name}");if !insert{let (r,c)=if axis==ibis_doc_model::sheet_command::Axis::Rows{(1,2)}else{(2,1)};let styled=read.cell(r,c).unwrap();assert_eq!(styled.value,CellValue::Empty);assert!(read.styles[styled.style as usize].bold);for row in 1..4{for col in 1..4{assert!(read.cell(row,col).is_none_or(|c|c.value.is_empty()));}}}assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);}}
}

#[test]
fn surviving_spill_structure_roundtrip() {
    let (_,stock)=open("xlsx-sales.xlsx");let mut initial=Document::new(stock.presentation.clone());let (id,shape)=(initial.ids().new_slide_id(),initial.ids().new_shape_id());initial.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"配列構造".into(),slide:id,shape})]).unwrap();set(&mut initial,id,vec![(1,1,CellInput::Formula("SEQUENCE(3,3)".into())),(5,5,CellInput::Formula("SUM($B$2#)".into()))]);initial.execute(vec![Command::Sheet(SheetCommand::SetFormat {slide:id,ranges:vec![CellRange {first_row:2,last_row:2,first_column:2,last_column:2}],change:StyleChange::Bold(true)})]).unwrap();let original=save(&stock,initial.presentation(),"spill-partial-original.xlsx");
    for axis in [ibis_doc_model::sheet_command::Axis::Rows,ibis_doc_model::sheet_command::Axis::Columns] {
        for (insert,count) in [(false,1),(false,2),(true,2)] {
            let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let id=slide(d.presentation(),"配列構造");
            let command=if insert{SheetCommand::Insert {slide:id,axis,at:2,count}}else{SheetCommand::Delete {slide:id,axis,at:2,count}};
            d.execute(vec![Command::Sheet(command)]).unwrap();let bytes=save(&opened,d.presentation(),&format!("spill-partial-{axis:?}-{insert}-{count}.xlsx"));assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();let sh=sheet_of_slide(p.slide(slide(&p,"配列構造")).unwrap()).unwrap();assert_eq!(sh.spills.get(&(1,1)),Some(&CellRange {first_row:1,last_row:3,first_column:1,last_column:3}));
            for row in 0..3{for col in 0..3{assert_eq!(sh.cell(row+1,col+1).unwrap().value,CellValue::Number((row*3+col+1)as f64));}}
            assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
        }
    }
}

#[test]
fn match_options_roundtrip() {
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let (id,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let mut commands=vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"検索選択肢".into(),slide:id,shape})];
    let cases:Vec<_>=include_str!("../../../test/editing/xlsx-match-options.tsv").lines().skip(1).map(|line|line.split_once('\t').unwrap()).collect();commands.push(Command::Sheet(SheetCommand::SetCells {slide:id,edits:cases.iter().enumerate().map(|(row,(formula,_))|CellEdit {row:row as u32,column:0,input:CellInput::Formula((*formula).into())}).collect()}));d.execute(commands).unwrap();
    for (row,(_,expected))in cases.iter().enumerate(){let expected=expected.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(if *expected=="#N/A"{ibis_doc_model::CellError::NA}else{ibis_doc_model::CellError::Value}));assert_eq!(value(d.presentation(),"検索選択肢",row as u32,0),expected);}
    let bytes=save(&opened,d.presentation(),"match-options.xlsx");assert_reads_back(&bytes,d.presentation());assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
}

#[test]
fn unique_options_dependency_roundtrip() {
    for (case,line) in include_str!("../../../test/editing/xlsx-unique-options.tsv").lines().skip(1).enumerate(){let fields:Vec<_>=line.split('\t').collect();let (_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let(id,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"一意配列".into(),slide:id,shape})]).unwrap();let mut edits=Vec::new();for(row,values)in [[1.,10.,1.],[2.,20.,2.],[1.,10.,1.]].into_iter().enumerate(){for(column,n)in values.into_iter().enumerate(){edits.push((row as u32,column as u32,CellInput::Value(CellValue::Number(n))));}}edits.push((0,4,CellInput::Formula(fields[0].into())));set(&mut d,id,edits);
        let check=|p:&Presentation,text:&str|{let rows:Vec<Vec<f64>>=text.split(';').map(|r|r.split(',').map(|v|v.parse().unwrap()).collect()).collect();let sh=sheet_of_slide(p.slide(slide(p,"一意配列")).unwrap()).unwrap();assert_eq!(sh.spills.get(&(0,4)),Some(&CellRange {first_row:0,last_row:rows.len()as u32-1,first_column:4,last_column:4+rows[0].len()as u32-1}));for r in 0..3{for c in 0..3{assert_eq!(value(p,"一意配列",r,c+4),rows.get(r as usize).and_then(|x|x.get(c as usize)).map_or(CellValue::Empty,|n|CellValue::Number(*n)));}}};
        check(d.presentation(),fields[1]);let before=save(&opened,d.presentation(),&format!("unique-options-{case}-before.xlsx"));assert_reads_back(&before,d.presentation());check(&ibis_pptx::read(&before).unwrap(),fields[1]);set(&mut d,id,vec![(2,2,CellInput::Value(CellValue::Number(3.0)))]);check(d.presentation(),fields[2]);let after=save(&opened,d.presentation(),&format!("unique-options-{case}-after.xlsx"));assert_reads_back(&after,d.presentation());check(&ibis_pptx::read(&after).unwrap(),fields[2]);assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),before);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),after);
    }
}

#[test]
fn makearray_dimension_dependency_roundtrip() {
    for columns in [false,true]{let(_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let(id,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"配列寸法".into(),slide:id,shape})]).unwrap();let formula=if columns{"MAKEARRAY(3,A1,LAMBDA(r,c,r*10+c))"}else{"MAKEARRAY(A1,3,LAMBDA(r,c,r*10+c))"};set(&mut d,id,vec![(0,0,CellInput::Value(CellValue::Number(2.0))),(2,2,CellInput::Formula(formula.into()))]);let before=save(&opened,d.presentation(),&format!("makearray-dimension-{columns}-before.xlsx"));assert_reads_back(&before,d.presentation());set(&mut d,id,vec![(0,0,CellInput::Value(CellValue::Number(2.9)))]);assert_eq!(value(d.presentation(),"配列寸法",2,2),CellValue::Error(ibis_doc_model::CellError::Value));let after=save(&opened,d.presentation(),&format!("makearray-dimension-{columns}-after.xlsx"));assert_reads_back(&after,d.presentation());let p=ibis_pptx::read(&after).unwrap();for r in 2..5{for c in 2..5{if(r,c)!=(2,2){assert!(value(&p,"配列寸法",r,c).is_empty());}}}assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),before);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),after);}
}

#[test]
fn file_lambda_bindings_preserve_source_and_recalculate() {
    let(_,stock)=open("xlsx-sales.xlsx");let mut initial=Document::new(stock.presentation.clone());let(id,shape)=(initial.ids().new_slide_id(),initial.ids().new_shape_id());initial.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"束縛保持".into(),slide:id,shape})]).unwrap();set(&mut initial,id,vec![(0,0,CellInput::Value(CellValue::Number(1.0))),(1,0,CellInput::Value(CellValue::Number(2.0))),(0,1,CellInput::Value(CellValue::Number(3.0))),(1,1,CellInput::Value(CellValue::Number(4.0))),(0,2,CellInput::Formula("MAP(A1:A2,B1:B2,LAMBDA(first,second,first+second))".into()))]);
    let mut package=ibis_ooxml::Package::open(stock.source.save(initial.presentation(),SaveOptions::default()).unwrap()).unwrap();let part=package.names().filter(|n|n.starts_with("xl/worksheets/sheet")&&n.ends_with(".xml")).find(|n|String::from_utf8(package.read(n).unwrap()).unwrap().contains("_xlpm.first")).unwrap().to_string();let xml=package.xml_mut(&part).unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:ibis","urn:ibis:lambda-test");let data=xml.child(root,"sheetData").unwrap();let c=xml.children_named(data,"row").flat_map(|r|xml.children_named(r,"c")).find(|&c|xml.attr(c,"r")==Some("C1")).unwrap();let f=xml.child(c,"f").unwrap();xml.set_text(f,"_xlfn.MAP(A1:A2,B1:B2,_xlfn.LAMBDA(_xlpm.x0,_xlpm.x1,_xlpm.x0+_xlpm.x1))");xml.set_attr(f,"ibis:sentinel","binding");let opaque="xl/ibis-lambda-test.bin";let payload=b"IBIS own synthetic lambda probe".to_vec();package.set_bytes(opaque,payload.clone());package.set_override(opaque,"application/octet-stream").unwrap();let original=package.save(Default::default()).unwrap();std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit/lambda-binding-original.xlsx"),&original).unwrap();
    let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let id=slide(d.presentation(),"束縛保持");assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert_eq!(cell(d.presentation(),"束縛保持",0,2).formula.as_deref(),Some("MAP(A1:A2,B1:B2,LAMBDA(x0,_xlpm.x1,x0+_xlpm.x1))"));set(&mut d,id,vec![(0,0,CellInput::Value(CellValue::Number(10.0)))]);assert_eq!(value(d.presentation(),"束縛保持",0,2),CellValue::Number(13.0));assert_eq!(value(d.presentation(),"束縛保持",1,2),CellValue::Number(6.0));let bytes=save(&opened,d.presentation(),"lambda-binding-edited.xlsx");assert_reads_back(&bytes,d.presentation());let package=ibis_ooxml::Package::open(bytes.clone()).unwrap();assert_eq!(package.read(opaque).unwrap(),payload);let xml=String::from_utf8(package.read(&part).unwrap()).unwrap();assert!(xml.contains("ibis:sentinel=\"binding\""));assert!(xml.contains("_xlpm.x0,_xlpm.x1,_xlpm.x0+_xlpm.x1"));assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
}

#[test]
fn map_shape_options_roundtrip() {
    let(original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let(id,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let cases:Vec<_>=include_str!("../../../test/editing/xlsx-map-shapes.tsv").lines().skip(1).map(|line|line.split_once('\t').unwrap()).collect();d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"対応座標".into(),slide:id,shape}),Command::Sheet(SheetCommand::SetCells {slide:id,edits:std::iter::once(CellEdit {row:9,column:9,input:CellInput::Value(CellValue::Number(5.0))}).chain(cases.iter().enumerate().map(|(i,(formula,_))|CellEdit {row:i as u32*3,column:0,input:CellInput::Formula((*formula).into())})).collect()})]).unwrap();let check=|p:&Presentation|{for(i,(formula,expected))in cases.iter().enumerate(){assert_eq!(cell(p,"対応座標",i as u32*3,0).formula.as_deref(),Some(*formula));for(row,values)in expected.split(';').enumerate(){for(col,v)in values.split(',').enumerate(){let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==v).unwrap()));assert_eq!(value(p,"対応座標",i as u32*3+row as u32,col as u32),expected);}}}};check(d.presentation());let bytes=save(&opened,d.presentation(),"map-shape-options.xlsx");assert_reads_back(&bytes,d.presentation());check(&ibis_pptx::read(&bytes).unwrap());assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
}

#[test]
fn lambda_arity_options_roundtrip() {
    let(original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let(id,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let cases:Vec<_>=include_str!("../../../test/editing/xlsx-lambda-arity.tsv").lines().skip(1).map(|line|line.split_once('\t').unwrap()).collect();d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"引数数".into(),slide:id,shape}),Command::Sheet(SheetCommand::SetCells {slide:id,edits:cases.iter().enumerate().map(|(row,(formula,_))|CellEdit {row:row as u32,column:0,input:CellInput::Formula((*formula).into())}).collect()})]).unwrap();for(row,(_,v))in cases.iter().enumerate(){let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==*v).unwrap()));assert_eq!(value(d.presentation(),"引数数",row as u32,0),expected);}let bytes=save(&opened,d.presentation(),"lambda-arity-options.xlsx");assert_reads_back(&bytes,d.presentation());assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
}

#[test]
fn isomitted_binding_options_roundtrip() {
    let(original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let(id,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let cases:Vec<_>=include_str!("../../../test/editing/xlsx-isomitted.tsv").lines().skip(1).map(|line|line.split_once('\t').unwrap()).collect();d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"省略引数".into(),slide:id,shape}),Command::Sheet(SheetCommand::SetCells {slide:id,edits:std::iter::once(CellEdit {row:1,column:0,input:CellInput::Value(CellValue::Number(7.0))}).chain(cases.iter().enumerate().map(|(i,(formula,_))|CellEdit {row:i as u32*3,column:3,input:CellInput::Formula((*formula).into())})).collect()})]).unwrap();let check=|p:&Presentation|{for(i,(formula,expected))in cases.iter().enumerate(){assert_eq!(cell(p,"省略引数",i as u32*3,3).formula.as_deref(),Some(*formula));for(row,values)in expected.split(';').enumerate(){for(col,v)in values.split(',').enumerate(){assert_eq!(value(p,"省略引数",i as u32*3+row as u32,col as u32+3),CellValue::Number(v.parse().unwrap()),"{formula}");}}}};check(d.presentation());let bytes=save(&opened,d.presentation(),"isomitted-binding-options.xlsx");assert_reads_back(&bytes,d.presentation());check(&ibis_pptx::read(&bytes).unwrap());assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
}

#[test]
fn cross_sheet_spill_structure_roundtrip() {
    let (_,stock)=open("xlsx-sales.xlsx");
    let mut initial=Document::new(stock.presentation.clone());
    let source=initial.ids().new_slide_id(); let source_shape=initial.ids().new_shape_id();
    let observer=initial.ids().new_slide_id(); let observer_shape=initial.ids().new_shape_id();
    initial.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"Source".into(),slide:source,shape:source_shape}),Command::Sheet(SheetCommand::AddSheet {index:2,name:"Observer".into(),slide:observer,shape:observer_shape})]).unwrap();
    set(&mut initial,source,vec![(1,1,CellInput::Formula("SEQUENCE(3,3)".into()))]);
    set(&mut initial,observer,[(0,"SUM(Source!$B$2#)"),(1,"Source!B2+Source!$B2+Source!B$2+Source!$B$2"),(3,"MAP(Source!$B$2#,LAMBDA(value,value*2))"),(7,"SUM(A4#)")].into_iter().map(|(r,f)|(r,0,CellInput::Formula(f.into()))).collect());
    let original=save(&stock,initial.presentation(),"spill-cross-sheet-original.xlsx");
    for axis in [ibis_doc_model::sheet_command::Axis::Rows,ibis_doc_model::sheet_command::Axis::Columns] {
        for (insert,at) in [(true,0),(true,2),(false,1),(false,2)] {
            let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let source=slide(d.presentation(),"Source");
            let command=if insert {SheetCommand::Insert {slide:source,axis,at,count:1}} else {SheetCommand::Delete {slide:source,axis,at,count:1}};
            d.execute(vec![Command::Sheet(command)]).unwrap();
            let deleted=!insert&&at==1;
            for (row,n) in [(0,45.0),(1,4.0),(7,90.0)] {assert_eq!(value(d.presentation(),"Observer",row,0),if deleted{CellValue::Error(ibis_doc_model::CellError::Ref)}else{CellValue::Number(n)});}
            let bytes=save(&opened,d.presentation(),&format!("spill-cross-sheet-{axis:?}-{insert}-{at}.xlsx"));assert_reads_back(&bytes,d.presentation());
            let p=ibis_pptx::read(&bytes).unwrap();let sh=sheet_of_slide(p.slide(slide(&p,"Observer")).unwrap()).unwrap();
            for r in 0..3 {for c in 0..3 {let expected=if deleted{if(r,c)==(0,0){CellValue::Error(ibis_doc_model::CellError::Ref)}else{CellValue::Empty}}else{CellValue::Number(((r*3+c+1)*2)as f64)};assert_eq!(sh.cell(r+3,c).map(|x|x.value.clone()).unwrap_or_default(),expected);}}
            assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
        }
    }
}

#[test]
fn dynamic_reference_options_roundtrip() {
    let (_,stock)=open("xlsx-sales.xlsx");let mut initial=Document::new(stock.presentation.clone());let(id,shape)=(initial.ids().new_slide_id(),initial.ids().new_shape_id());initial.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"参照配列".into(),slide:id,shape})]).unwrap();
    let cases:Vec<_>=include_str!("../../../test/editing/xlsx-dynamic-reference.tsv").lines().skip(1).map(|line|line.split('\t').collect::<Vec<_>>()).collect();let mut edits=vec![(0,0,CellInput::Value(CellValue::Number(3.0))),(1,1,CellInput::Formula("SEQUENCE(A1)".into()))];edits.extend(cases.iter().enumerate().map(|(i,f)|(2,3+i as u32*3,CellInput::Formula(f[0].into()))));set(&mut initial,id,edits);
    let check=|p:&Presentation,col:usize|{for(i,f)in cases.iter().enumerate(){assert_eq!(cell(p,"参照配列",2,3+i as u32*3).formula.as_deref(),Some(f[0]));let expected:Vec<f64>=f[col].split(';').map(|v|v.parse().unwrap()).collect();for r in 0..3{let sh=sheet_of_slide(p.slide(slide(p,"参照配列")).unwrap()).unwrap();assert_eq!(sh.cell(r+2,3+i as u32*3).map(|x|x.value.clone()).unwrap_or_default(),expected.get(r as usize).map_or(CellValue::Empty,|n|CellValue::Number(*n)));}}};check(initial.presentation(),1);
    let original=save(&stock,initial.presentation(),"dynamic-reference-before.xlsx");let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let id=slide(d.presentation(),"参照配列");set(&mut d,id,vec![(0,0,CellInput::Value(CellValue::Number(2.0)))]);check(d.presentation(),2);let bytes=save(&opened,d.presentation(),"dynamic-reference-after.xlsx");assert_reads_back(&bytes,d.presentation());check(&ibis_pptx::read(&bytes).unwrap(),2);assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
}

#[test]
fn text_boundary_options_roundtrip() {
    let(original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let(id,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let cases:Vec<_>=include_str!("../../../test/editing/xlsx-text-boundary.tsv").lines().skip(1).map(|line|line.split_once('\t').unwrap()).collect();d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"文字終端".into(),slide:id,shape}),Command::Sheet(SheetCommand::SetCells {slide:id,edits:cases.iter().enumerate().map(|(row,(formula,_))|CellEdit {row:row as u32,column:0,input:CellInput::Formula((*formula).into())}).collect()})]).unwrap();for(row,(_,v))in cases.iter().enumerate(){let expected=v.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==*v).unwrap()));assert_eq!(value(d.presentation(),"文字終端",row as u32,0),expected);}let bytes=save(&opened,d.presentation(),"text-boundary-options.xlsx");assert_reads_back(&bytes,d.presentation());assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
}

#[test]
fn compatibility_version_len_recalculation_and_retention() {
    // The extension URI is present in the existing xlsx-sales.xlsx input.
    // Variants are synthetic; their native recalculation is not verified.
    const VERSION_NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2024/workbookCompatibilityVersion";
    for (label, version, namespace, modern) in [
        ("absent", None, VERSION_NS, false),
        ("legacy", Some(1), VERSION_NS, false),
        ("modern", Some(2), VERSION_NS, true),
        ("future", Some(7), VERSION_NS, true),
        ("foreign", Some(2), "urn:ibis:foreign-version-probe", false),
    ] {
        let (_, stock) = open("xlsx-sales.xlsx");
        let mut initial = Document::new(stock.presentation.clone());
        let id = slide(initial.presentation(), "月次売上");
        set(&mut initial, id, vec![(9,0,CellInput::Value(CellValue::Text("abc".into()))),(9,1,CellInput::Formula("LEN(A10)".into()))]);
        let mut package=ibis_ooxml::Package::open(stock.source.save(initial.presentation(),SaveOptions::default()).unwrap()).unwrap();
        let doc=package.xml_mut("xl/workbook.xml").unwrap();
        let root=doc.root_element().unwrap();
        // The stock input already declares version 2: remove it for each control.
        let old:Vec<_>=doc.descendants(root).into_iter().filter(|&n|doc.local_name(n)=="version" && doc.namespace(n)==Some(VERSION_NS)).collect();
        for node in old {let ext=doc.parent(node).unwrap();doc.detach(ext);}
        if let Some(version) = version {
            let ext_list=doc.child(root,"extLst").unwrap_or_else(||doc.append_element(root,"extLst"));
            let ext=doc.append_element(ext_list,"ext");
            doc.set_attr(ext,"uri","{D14903EA-33C4-47F7-8F05-3474C54BE107}");
            doc.set_attr(ext,"xmlns:ibisversion",namespace);
            let node=doc.append_element(ext,"ibisversion:version");
            doc.set_attr(node,"setVersion",&version.to_string());
            doc.set_attr(node,"warnBelowVersion","1");
        }
        let original=package.save(Default::default()).unwrap();
        let opened=ibis_pptx::open(original.clone()).unwrap();
        assert_eq!(opened.presentation.workbook.as_deref().unwrap().compatibility_version, if namespace==VERSION_NS { version } else {None});
        let mut d=Document::new(opened.presentation.clone());
        let id=slide(d.presentation(),"月次売上");
        assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
        for (text, legacy, modern_len) in [("😀x",3.,2.),("😀\u{fe0f}",3.,2.),("😀🏽",4.,2.)] {
            set(&mut d,id,vec![(9,0,CellInput::Value(CellValue::Text(text.into())))]);
            let expected=CellValue::Number(if modern {modern_len} else {legacy});
            assert_eq!(value(d.presentation(),"月次売上",9,1),expected,"{label}: {text}");
            let bytes=save(&opened,d.presentation(),&format!("compatibility-len-{label}-{legacy}.xlsx"));
            assert_reads_back(&bytes,d.presentation());
            let saved=ibis_ooxml::Package::open(bytes.clone()).unwrap();
            assert_eq!(saved.read("xl/workbook.xml").unwrap(),package.read("xl/workbook.xml").unwrap(),"version extension retained exactly");
            assert!(d.undo());assert_eq!(value(d.presentation(),"月次売上",9,1),CellValue::Number(3.));
            assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
            assert!(d.redo());assert_eq!(value(d.presentation(),"月次売上",9,1),expected);
            assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
            assert!(d.undo());
        }
    }
}

#[test]
fn compatibility_version_mid_replace_roundtrip() {
    for (path,version) in [("xlsx-compatibility-v1.xlsx",1),("xlsx-sales.xlsx",2)] {
        let (original,opened)=open(path);let mut d=Document::new(opened.presentation.clone());
        let(id,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
        let cases:Vec<_>=include_str!("../../../test/editing/xlsx-compatibility-slice.tsv").lines().skip(1).map(|line|line.split('\t').collect::<Vec<_>>()).collect();
        d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"版切取".into(),slide:id,shape}),Command::Sheet(SheetCommand::SetCells {slide:id,edits:cases.iter().enumerate().map(|(row,f)|CellEdit {row:row as u32,column:0,input:CellInput::Formula(f[0].into())}).collect()})]).unwrap();
        for(row,f)in cases.iter().enumerate() {
            let text=f[version as usize];let expected=ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==text).map_or_else(||CellValue::Text(text.into()),|e|CellValue::Error(*e));
            assert_eq!(value(d.presentation(),"版切取",row as u32,0),expected,"version {version}: {}",f[0]);
        }
        let bytes=save(&opened,d.presentation(),&format!("compatibility-slice-v{version}.xlsx"));assert_reads_back(&bytes,d.presentation());
        assert_eq!(ibis_pptx::read(&bytes).unwrap().workbook.as_deref().unwrap().compatibility_version,Some(version));
        let package=ibis_ooxml::Package::open(bytes.clone()).unwrap();let xml=String::from_utf8(package.read("xl/workbook.xml").unwrap()).unwrap();
        assert!(xml.contains(&format!("setVersion=\"{version}\"")));assert!(xml.contains("{D14903EA-33C4-47F7-8F05-3474C54BE107}"));
        assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
        assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    }
}

#[test]
fn compatibility_version_find_search_roundtrip() {
    for (path,version) in [("xlsx-compatibility-v1.xlsx",1),("xlsx-sales.xlsx",2)] {
        let (original,opened)=open(path);let mut d=Document::new(opened.presentation.clone());
        let(id,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
        let cases:Vec<_>=include_str!("../../../test/editing/xlsx-compatibility-search.tsv").lines().skip(1).map(|line|line.split('\t').collect::<Vec<_>>()).collect();
        d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"版検索".into(),slide:id,shape}),Command::Sheet(SheetCommand::SetCells {slide:id,edits:cases.iter().enumerate().map(|(row,f)|CellEdit {row:row as u32,column:0,input:CellInput::Formula(f[0].into())}).collect()})]).unwrap();
        for(row,f)in cases.iter().enumerate() {
            let text=f[version as usize];let expected=text.parse::<f64>().map(CellValue::Number).unwrap_or_else(|_|CellValue::Error(*ibis_doc_model::CellError::ALL.iter().find(|e|e.as_str()==text).unwrap()));
            assert_eq!(value(d.presentation(),"版検索",row as u32,0),expected,"version {version}: {}",f[0]);
        }
        let bytes=save(&opened,d.presentation(),&format!("compatibility-search-v{version}.xlsx"));assert_reads_back(&bytes,d.presentation());
        assert_eq!(ibis_pptx::read(&bytes).unwrap().workbook.as_deref().unwrap().compatibility_version,Some(version));
        let package=ibis_ooxml::Package::open(bytes.clone()).unwrap();let xml=String::from_utf8(package.read("xl/workbook.xml").unwrap()).unwrap();
        assert!(xml.contains(&format!("setVersion=\"{version}\"")));assert!(xml.contains("{D14903EA-33C4-47F7-8F05-3474C54BE107}"));
        assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
        assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    }
}

#[test]
fn compatibility_selection_save_and_undo_redo() {
    let(_,stock)=open("xlsx-sales.xlsx");let mut initial=Document::new(stock.presentation.clone());let(id,shape)=(initial.ids().new_slide_id(),initial.ids().new_shape_id());
    let cases:Vec<_>=include_str!("../../../test/editing/xlsx-compatibility-toggle.tsv").lines().skip(1).map(|line|line.split('\t').collect::<Vec<_>>()).collect();
    initial.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"版選択".into(),slide:id,shape}),Command::Sheet(SheetCommand::SetCells {slide:id,edits:cases.iter().enumerate().map(|(i,f)|CellEdit {row:i as u32*3,column:0,input:CellInput::Formula(f[0].into())}).collect()})]).unwrap();
    let original=save(&stock,initial.presentation(),"compatibility-toggle-before.xlsx");let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());
    d.execute(vec![Command::Sheet(SheetCommand::SetCompatibilityVersion {version:ibis_doc_model::CalculationCompatibilityVersion::V1})]).unwrap();
    for(i,f)in cases.iter().enumerate(){let text=f[2];let expected=if f[1]=="T" {vec![CellValue::Text(text.into())]} else {text.split(';').map(|v|CellValue::Number(v.parse().unwrap())).collect()};for r in 0..2{assert_eq!(value(d.presentation(),"版選択",i as u32*3+r,0),expected.get(r as usize).cloned().unwrap_or_default());}}
    let bytes=save(&opened,d.presentation(),"compatibility-toggle-after.xlsx");assert_reads_back(&bytes,d.presentation());assert_eq!(ibis_pptx::read(&bytes).unwrap().workbook.as_deref().unwrap().compatibility_version,Some(1));
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
}


#[test]
fn single_cell_dynamic_reopens_and_grows_without_promoting_legacy() {
    let (_, stock) = open("xlsx-sales.xlsx");
    let mut initial = Document::new(stock.presentation.clone());
    let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id());
    initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "再開配列".into(), slide: id, shape })]).unwrap();
    let cases: Vec<_> = include_str!("../../../test/editing/xlsx-dynamic-reopen.tsv").lines().skip(1).map(|line| line.split('\t').collect::<Vec<_>>()).collect();
    let mut edits = vec![(0, 0, CellInput::Value(CellValue::Number(1.))), (1, 1, CellInput::Formula("SEQUENCE(A1)".into()))];
    edits.extend(cases.iter().enumerate().map(|(i, f)| (2, 3 + i as u32 * 3, CellInput::Formula(f[0].into()))));
    edits.push((2, 30, CellInput::Formula("B2:B4*2".into())));
    set(&mut initial, id, edits);
    // An ordinary OOXML formula with no cell metadata is a synthetic legacy
    // control. It must not acquire the typed formula's dynamic flag.
    let mut package = ibis_ooxml::Package::open(stock.source.save(initial.presentation(), SaveOptions::default()).unwrap()).unwrap();
    let xml = package.xml_mut("xl/worksheets/sheet4.xml").unwrap();
    let root = xml.root_element().unwrap();
    let legacy = xml.descendants(root).into_iter().find(|&n| xml.local_name(n) == "c" && xml.attr(n, "r") == Some("AE3")).unwrap();
    xml.remove_attr(legacy, "cm");
    let f = xml.child(legacy, "f").unwrap();
    xml.remove_attr(f, "t"); xml.remove_attr(f, "ref");
    let old_tails: Vec<_> = xml.descendants(root).into_iter().filter(|&n| xml.local_name(n) == "c" && matches!(xml.attr(n, "r"), Some("AE4" | "AE5"))).collect();
    for tail in old_tails { xml.detach(tail); }
    let original = package.save(Default::default()).unwrap();
    std::fs::write(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit/dynamic-reopen-before.xlsx"), &original).unwrap();
    let opened = ibis_pptx::open(original.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let id = slide(d.presentation(), "再開配列");
    let check = |p: &Presentation, col: usize| {
        let sh = sheet_of_slide(p.slide(slide(p, "再開配列")).unwrap()).unwrap();
        for (i, f) in cases.iter().enumerate() {
            let c = 3 + i as u32 * 3;
            assert_eq!(sh.cell(2, c).unwrap().formula.as_deref(), Some(f[0]));
            // A formula that gives one value is saved plain (as Excel saves it).
            let dynamic = ibis_calc::formula::arrays::can_return_array(f[0], &|_| false);
            assert_eq!(sh.spills.contains_key(&(2, c)), dynamic, "dynamic marker {}", f[0]);
            let expected: Vec<f64> = f[col].split(';').map(|v| v.parse().unwrap()).collect();
            for r in 0..3 { assert_eq!(value(p, "再開配列", 2 + r, c), expected.get(r as usize).map_or(CellValue::Empty, |v| CellValue::Number(*v)), "{} at row {}", f[0], r); }
        }
        assert!(!sh.spills.contains_key(&(2, 30)), "legacy stays scalar");
        assert_eq!(value(p, "再開配列", 2, 30), CellValue::Number(if col == 1 { 0. } else { 4. }));
        assert_eq!(value(p, "再開配列", 3, 30), CellValue::Empty);
    };
    // Force recalculation of the synthetic legacy control, whose initial
    // cache was produced before stripping its metadata.
    set(&mut d, id, vec![(0, 0, CellInput::Value(CellValue::Number(1.)))]);
    check(d.presentation(), 1);
    let original = save(&opened, d.presentation(), "dynamic-reopen-one.xlsx");
    let opened = ibis_pptx::open(original.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    check(d.presentation(), 1);
    let id = slide(d.presentation(), "再開配列");
    set(&mut d, id, vec![(0, 0, CellInput::Value(CellValue::Number(3.)))]);
    check(d.presentation(), 2);
    let bytes = save(&opened, d.presentation(), "dynamic-reopen-three.xlsx");
    assert_reads_back(&bytes, d.presentation()); check(&ibis_pptx::read(&bytes).unwrap(), 2);
    assert!(d.undo()); check(d.presentation(), 1);
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
    assert!(d.redo()); check(d.presentation(), 2);
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), bytes);
}


#[test]
fn week_return_types_save_and_recalculate_after_reopening() {
    let (_, stock) = open("xlsx-sales.xlsx"); let mut initial = Document::new(stock.presentation.clone());
    let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id());
    initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "週番号引数".into(), slide: id, shape })]).unwrap();
    let cases: Vec<_> = include_str!("../../../test/editing/xlsx-week-return-types.tsv").lines().skip(1).map(|line| line.split('\t').collect::<Vec<_>>()).collect();
    let mut edits = vec![(0, 1, CellInput::Formula("DATE(2021,1,1)".into()))];
    edits.extend(cases.iter().enumerate().map(|(r, f)| (r as u32, 0, CellInput::Formula(f[0].into())))); set(&mut initial, id, edits);
    let check = |p: &Presentation, col: usize| { for (r, f) in cases.iter().enumerate() { let expected = f[col].parse::<f64>().map(CellValue::Number).unwrap_or(CellValue::Error(ibis_doc_model::CellError::Num)); assert_eq!(value(p, "週番号引数", r as u32, 0), expected, "{}", f[0]); assert_eq!(cell(p, "週番号引数", r as u32, 0).formula.as_deref(), Some(f[0])); } };
    check(initial.presentation(), 1); let original = save(&stock, initial.presentation(), "week-return-types-before.xlsx");
    let opened = ibis_pptx::open(original.clone()).unwrap(); let mut d = Document::new(opened.presentation.clone()); check(d.presentation(), 1);
    let id = slide(d.presentation(), "週番号引数"); set(&mut d, id, vec![(0, 1, CellInput::Formula("DATE(2021,1,4)".into()))]); check(d.presentation(), 2);
    let bytes = save(&opened, d.presentation(), "week-return-types-after.xlsx"); assert_reads_back(&bytes, d.presentation()); check(&ibis_pptx::read(&bytes).unwrap(), 2);
    assert!(d.undo()); check(d.presentation(), 1); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
    assert!(d.redo()); check(d.presentation(), 2); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), bytes);
}


#[test]
fn day_count_basis_and_method_save_in_both_date_bases() {
    for date1904 in [false, true] {
        let (input, _) = open("xlsx-sales.xlsx"); let mut package = ibis_ooxml::Package::open(input).unwrap();
        let xml = package.xml_mut("xl/workbook.xml").unwrap(); let root = xml.root_element().unwrap();
        let properties = xml.child(root, "workbookPr").unwrap(); xml.set_attr(properties, "date1904", if date1904 { "1" } else { "0" });
        let stock = ibis_pptx::open(package.save(Default::default()).unwrap()).unwrap();
        let mut initial = Document::new(stock.presentation.clone()); let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id());
        initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "日数基準".into(), slide: id, shape })]).unwrap();
        let cases: Vec<_> = include_str!("../../../test/editing/xlsx-day-count-options.tsv").lines().skip(1).map(|line| line.split('\t').collect::<Vec<_>>()).collect();
        let mut edits = vec![(0, 1, CellInput::Formula("DATE(2019,1,1)".into())), (0, 2, CellInput::Formula("DATE(2020,1,1)".into()))];
        edits.extend(cases.iter().enumerate().map(|(r, f)| (r as u32, 0, CellInput::Formula(f[0].into())))); set(&mut initial, id, edits);
        let check = |p: &Presentation, col: usize| { assert_eq!(p.workbook.as_deref().unwrap().date1904, date1904); for (r, f) in cases.iter().enumerate() { let expected = f[col].parse::<f64>().map(CellValue::Number).unwrap_or(CellValue::Error(ibis_doc_model::CellError::Num)); assert_eq!(value(p, "日数基準", r as u32, 0), expected, "{}", f[0]); assert_eq!(cell(p, "日数基準", r as u32, 0).formula.as_deref(), Some(f[0])); } };
        check(initial.presentation(), 1); let original = save(&stock, initial.presentation(), &format!("day-count-{date1904}-before.xlsx"));
        let opened = ibis_pptx::open(original.clone()).unwrap(); let mut d = Document::new(opened.presentation.clone()); check(d.presentation(), 1);
        let id = slide(d.presentation(), "日数基準"); set(&mut d, id, vec![(0, 1, CellInput::Formula("DATE(2020,1,1)".into())), (0, 2, CellInput::Formula("DATE(2021,1,1)".into()))]); check(d.presentation(), 2);
        let bytes = save(&opened, d.presentation(), &format!("day-count-{date1904}-after.xlsx")); assert_reads_back(&bytes, d.presentation()); check(&ibis_pptx::read(&bytes).unwrap(), 2);
        assert!(d.undo()); check(d.presentation(), 1); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
        assert!(d.redo()); check(d.presentation(), 2); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), bytes);
    }
}


#[test]
fn dynamic_growth_preserves_composite_metadata_and_unknown_formula_attributes() {
    for order in [[1,2], [2,1]] {
    for plain in [true, false] {
    let (_, stock) = open("xlsx-sales.xlsx"); let mut initial = Document::new(stock.presentation.clone());
    let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id());
    initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "複合保持".into(), slide: id, shape })]).unwrap();
    set(&mut initial, id, vec![(0,0,CellInput::Value(CellValue::Number(1.))), (1,1,CellInput::Formula("SEQUENCE(A1)".into())), (2,3,CellInput::Formula("B2#*2".into()))]);
    let mut package = ibis_ooxml::Package::open(stock.source.save(initial.presentation(), SaveOptions::default()).unwrap()).unwrap();
    let metadata = package.xml_mut("xl/metadata.xml").unwrap(); let root = metadata.root_element().unwrap();
    metadata.set_attr(root, "xmlns:ibis", "urn:ibis:composite-metadata-test");
    let types = metadata.child(root, "metadataTypes").unwrap(); metadata.set_attr(types, "count", "2");
    let extra = metadata.append_element(types, "metadataType"); metadata.set_attr(extra, "name", "IBIS_PROBE"); metadata.set_attr(extra, "minSupportedVersion", "0"); metadata.set_attr(extra, "cellMeta", "1");
    let future = metadata.append_element(root, "futureMetadata"); metadata.set_attr(future, "name", "IBIS_PROBE"); metadata.set_attr(future, "count", "1");
    let bk = metadata.append_element(future, "bk"); let list = metadata.append_element(bk, "extLst"); let ext = metadata.append_element(list, "ext"); metadata.set_attr(ext, "uri", "{IBIS-COMPOSITE-METADATA-PROBE}"); let payload = metadata.append_element(ext, "ibis:payload"); metadata.set_text(payload, "keep-unmodeled-metadata");
    let cells = metadata.child(root, "cellMetadata").unwrap(); metadata.insert_before(cells, future); metadata.set_attr(cells, "count", "2"); let composite = metadata.append_element(cells, "bk");
    if !plain { let first = metadata.child(cells,"bk").unwrap(); let rc = metadata.append_element(first,"rc"); metadata.set_attr(rc,"t","2"); metadata.set_attr(rc,"v","0"); }
    for t in order { let rc = metadata.append_element(composite, "rc"); metadata.set_attr(rc, "t", &t.to_string()); metadata.set_attr(rc, "v", "0"); }
    let xml = package.xml_mut("xl/worksheets/sheet4.xml").unwrap(); let root = xml.root_element().unwrap(); xml.set_attr(root, "xmlns:ibis", "urn:ibis:composite-metadata-test"); xml.set_attr(root,"xmlns:mc","http://schemas.openxmlformats.org/markup-compatibility/2006"); xml.set_attr(root,"mc:Ignorable","ibis");
    for address in ["B2", "D3"] {
        let c = xml.descendants(root).into_iter().find(|&n| xml.local_name(n)=="c" && xml.attr(n,"r")==Some(address)).unwrap();
        xml.set_attr(c, "cm", "2"); xml.set_attr(c, "ibis:sentinel", address);
        let f = xml.child(c,"f").unwrap(); xml.set_attr(f, "ibis:sentinel", "keep-formula-attribute"); xml.set_attr(f,"aca","1"); xml.set_attr(f,"ca","1");
    }
    let opaque = "xl/ibis-composite-probe.bin"; let payload = b"ibis own synthetic metadata payload".to_vec(); package.set_bytes(opaque,payload.clone()); package.set_override(opaque,"application/octet-stream").unwrap();
    let original = package.save(Default::default()).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit"); std::fs::create_dir_all(&dir).unwrap(); std::fs::write(dir.join(format!("dynamic-composite-{}-{plain}-source.xlsx",order[0])),&original).unwrap();
    let original_package = ibis_ooxml::Package::open(original.clone()).unwrap(); let metadata_before = original_package.read("xl/metadata.xml").unwrap();
    // Saving puts the dynamic array record first in a block that has it later (Excel does not open the other order).
    let metadata_saved = { let text = String::from_utf8(metadata_before.clone()).unwrap(); text.replace("<rc t=\"2\" v=\"0\"/><rc t=\"1\" v=\"0\"/>", "<rc t=\"1\" v=\"0\"/><rc t=\"2\" v=\"0\"/>").into_bytes() };
    let opened = ibis_pptx::open(original.clone()).unwrap(); let mut d = Document::new(opened.presentation.clone());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
    let id = slide(d.presentation(), "複合保持");
    let mut rewrite_only = Document::new(opened.presentation.clone());
    set(&mut rewrite_only,id,vec![(1,1,CellInput::Formula("SEQUENCE(A1)*3".into()))]);
    let rewritten = save(&opened,rewrite_only.presentation(),&format!("dynamic-composite-{}-{plain}-rewritten.xlsx",order[0]));
    assert_reads_back(&rewritten,rewrite_only.presentation());
    let rewritten_package = ibis_ooxml::Package::open(rewritten.clone()).unwrap();
    assert_eq!(rewritten_package.read("xl/metadata.xml").unwrap(),metadata_saved,"rewriting an existing dynamic formula retains metadata bytes");
    let rewritten_xml = rewritten_package.xml("xl/worksheets/sheet4.xml").unwrap(); let rewritten_root = rewritten_xml.root_element().unwrap();
    let rewritten_cell = rewritten_xml.descendants(rewritten_root).into_iter().find(|&n|rewritten_xml.local_name(n)=="c" && rewritten_xml.attr(n,"r")==Some("B2")).unwrap();
    assert_eq!(rewritten_xml.attr(rewritten_cell,"cm"),Some("2"),"formula rewrite must keep the composite record");
    let rewritten_f = rewritten_xml.child(rewritten_cell,"f").unwrap();
    assert_eq!(rewritten_xml.attr(rewritten_f,"ibis:sentinel"),Some("keep-formula-attribute"));
    assert_eq!(rewritten_xml.attr(rewritten_f,"aca"),Some("1")); assert_eq!(rewritten_xml.attr(rewritten_f,"ca"),Some("1"));
    assert_eq!(rewritten_package.read(opaque).unwrap(),payload);
    assert!(rewrite_only.undo()); assert_eq!(opened.source.save(rewrite_only.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(rewrite_only.redo()); assert_eq!(opened.source.save(rewrite_only.presentation(),SaveOptions::default()).unwrap(),rewritten);
    let mut literal_only = Document::new(opened.presentation.clone());
    set(&mut literal_only,id,vec![(1,1,CellInput::Value(CellValue::Number(42.)))]);
    let literal = save(&opened,literal_only.presentation(),&format!("dynamic-composite-{}-{plain}-literal.xlsx",order[0]));
    assert_reads_back(&literal,literal_only.presentation());
    let mut literal_package = ibis_ooxml::Package::open(literal.clone()).unwrap();
    let literal_xml = literal_package.xml("xl/worksheets/sheet4.xml").unwrap(); let literal_root = literal_xml.root_element().unwrap();
    let literal_cell = literal_xml.descendants(literal_root).into_iter().find(|&n|literal_xml.local_name(n)=="c" && literal_xml.attr(n,"r")==Some("B2")).unwrap();
    assert_eq!(literal_xml.attr(literal_cell,"cm"),Some("3"),"literal retains an unknown-only metadata reference");
    assert!(literal_xml.child(literal_cell,"f").is_none()); assert_eq!(literal_xml.attr(literal_cell,"ibis:sentinel"),Some("B2"));
    let metadata = literal_package.xml_mut("xl/metadata.xml").unwrap(); let root = metadata.root_element().unwrap(); let cells = metadata.child(root,"cellMetadata").unwrap(); let blocks:Vec<_>=metadata.children_named(cells,"bk").collect(); assert_eq!(blocks.len(),3);
    let records:Vec<_>=metadata.children_named(blocks[2],"rc").collect(); assert_eq!(records.len(),1); assert_eq!(metadata.attr(records[0],"t"),Some("2")); assert_eq!(metadata.attr(records[0],"v"),Some("0"));
    metadata.detach(blocks[2]); metadata.set_attr(cells,"count","2"); assert_eq!(literal_package.read("xl/metadata.xml").unwrap(),metadata_saved,"only unknown-only block/count are added");
    assert_eq!(literal_package.read(opaque).unwrap(),payload);
    assert!(literal_only.undo()); assert_eq!(opened.source.save(literal_only.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(literal_only.redo()); assert_eq!(opened.source.save(literal_only.presentation(),SaveOptions::default()).unwrap(),literal);
    {
        // Reactivating the literal as a dynamic formula: the XLDAPR record comes first in the new block (Excel does not open it otherwise).
        let reopened = ibis_pptx::open(literal.clone()).unwrap(); let mut r = Document::new(reopened.presentation.clone());
        let rid = slide(r.presentation(), "複合保持");
        set(&mut r, rid, vec![(1,1,CellInput::Formula("SEQUENCE(A1)".into()))]);
        let reactivated = reopened.source.save(r.presentation(), SaveOptions::default()).unwrap();
        std::fs::write(dir.join(format!("dynamic-composite-{}-{plain}-reactivated.xlsx",order[0])),&reactivated).unwrap();
        let pkg = ibis_ooxml::Package::open(reactivated.clone()).unwrap(); let m = pkg.xml("xl/metadata.xml").unwrap(); let mroot = m.root_element().unwrap(); let cells = m.child(mroot,"cellMetadata").unwrap();
        let blocks: Vec<_> = m.children_named(cells,"bk").collect(); let last = *blocks.last().unwrap();
        let first = m.children_named(last,"rc").next().unwrap(); assert_eq!(m.attr(first,"t"),Some("1"),"XLDAPR record first");
    }
    let mut grow_only = Document::new(opened.presentation.clone()); set(&mut grow_only,id,vec![(0,0,CellInput::Value(CellValue::Number(3.)))]);
    let grown_only = opened.source.save(grow_only.presentation(),SaveOptions::default()).unwrap();
    assert_eq!(ibis_ooxml::Package::open(grown_only).unwrap().read("xl/metadata.xml").unwrap(),metadata_saved,"existing-only growth need not append metadata");
    set(&mut d,id,vec![(0,0,CellInput::Value(CellValue::Number(3.))),(9,5,CellInput::Formula("SEQUENCE(1)".into()))]);
    let bytes = save(&opened,d.presentation(),&format!("dynamic-composite-{}-{plain}-grown.xlsx",order[0])); assert_reads_back(&bytes,d.presentation());
    for (r,n) in [(2,2.),(3,4.),(4,6.)] { assert_eq!(value(d.presentation(),"複合保持",r,3),CellValue::Number(n)); }
    let mut saved = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    { let m = saved.xml("xl/metadata.xml").unwrap(); let r = m.root_element().unwrap(); let cm = m.child(r,"cellMetadata").unwrap();
      for bk in m.children_named(cm,"bk") { let rcs: Vec<_> = m.children_named(bk,"rc").collect(); if let Some(i) = rcs.iter().position(|&rc| m.attr(rc,"t")==Some("1")) { assert_eq!(i,0,"XLDAPR record first in every block"); } } }
    if !plain { let metadata = saved.xml_mut("xl/metadata.xml").unwrap(); let root = metadata.root_element().unwrap(); let cells = metadata.child(root,"cellMetadata").unwrap(); assert_eq!(metadata.attr(cells,"count"),Some("3")); let blocks: Vec<_> = metadata.children_named(cells,"bk").collect(); assert_eq!(blocks.len(),3); let records: Vec<_> = metadata.children_named(blocks[2],"rc").collect(); assert_eq!(records.len(),1); assert_eq!(metadata.attr(records[0],"t"),Some("1")); metadata.detach(blocks[2]); metadata.set_attr(cells,"count","2"); }
    assert_eq!(saved.read("xl/metadata.xml").unwrap(),metadata_saved); assert_eq!(saved.read(opaque).unwrap(),payload);
    let xml = saved.xml("xl/worksheets/sheet4.xml").unwrap(); let root = xml.root_element().unwrap();
    for address in ["B2","D3"] { let c=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="c" && xml.attr(n,"r")==Some(address)).unwrap(); assert_eq!(xml.attr(c,"cm"),Some("2"),"{address} retains its composite metadata record"); assert_eq!(xml.attr(c,"ibis:sentinel"),Some(address)); let f=xml.child(c,"f").unwrap(); assert_eq!(xml.attr(f,"ibis:sentinel"),Some("keep-formula-attribute")); }
    let fresh = xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="c" && xml.attr(n,"r")==Some("F10")).unwrap(); assert_eq!(xml.attr(fresh,"cm"),Some(if plain {"1"} else {"3"}),"new cell uses plain record instead of inheriting opaque metadata");
    assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
    assert!(d.redo()); assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    }
    }
}


#[test]
fn dynamic_metadata_requires_standard_namespace_and_unambiguous_lists() {
    for variant in ["valid","prefixed","shadow-types","shadow-cells","foreign-root","foreign-types","foreign-cells","foreign-type","foreign-bk","foreign-rc","duplicate-types","missing-name"] {
        let (_,stock)=open("xlsx-sales.xlsx"); let mut initial=Document::new(stock.presentation.clone()); let(id,shape)=(initial.ids().new_slide_id(),initial.ids().new_shape_id());
        initial.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"metadata境界".into(),slide:id,shape})]).unwrap();
        set(&mut initial,id,vec![(0,0,CellInput::Value(CellValue::Number(1.))),(1,1,CellInput::Formula("SEQUENCE(A1)".into()))]);
        let mut package=ibis_ooxml::Package::open(stock.source.save(initial.presentation(),SaveOptions::default()).unwrap()).unwrap();
        let xml=package.xml_mut("xl/metadata.xml").unwrap();let root=xml.root_element().unwrap();let types=xml.child(root,"metadataTypes").unwrap();let cells=xml.child(root,"cellMetadata").unwrap();let kind=xml.child(types,"metadataType").unwrap();let bk=xml.child(cells,"bk").unwrap();let rc=xml.child(bk,"rc").unwrap();
        match variant {
            "foreign-root"=>xml.set_attr(root,"xmlns","urn:ibis:foreign-metadata"),
            "foreign-types"=>xml.set_attr(types,"xmlns","urn:ibis:foreign-metadata"),
            "foreign-cells"=>xml.set_attr(cells,"xmlns","urn:ibis:foreign-metadata"),
            "foreign-type"=>xml.set_attr(kind,"xmlns","urn:ibis:foreign-metadata"),
            "foreign-bk"=>xml.set_attr(bk,"xmlns","urn:ibis:foreign-metadata"),
            "foreign-rc"=>xml.set_attr(rc,"xmlns","urn:ibis:foreign-metadata"),
            "missing-name"=>xml.remove_attr(kind,"name"),
            "duplicate-types"=>{let copy=xml.deep_clone(types);xml.insert_before(types,copy);},
            "shadow-types"|"shadow-cells"=>{let target=if variant=="shadow-types"{types}else{cells};let copy=xml.deep_clone(target);xml.set_attr(copy,"xmlns","urn:ibis:foreign-metadata");xml.insert_before(target,copy);},
            "prefixed"=>{xml.set_attr(root,"xmlns:p","http://schemas.openxmlformats.org/spreadsheetml/2006/main");let nodes:Vec<_>=xml.descendants(root).into_iter().filter(|&n|xml.namespace(n)==Some("http://schemas.openxmlformats.org/spreadsheetml/2006/main")).collect();for n in nodes {let name=xml.local_name(n).to_string();xml.rename(n,&format!("p:{name}"));}xml.rename(root,"p:metadata");},
            _=>{},
        }
        let original=package.save(Default::default()).unwrap();let metadata=package.read("xl/metadata.xml").unwrap();let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let id=slide(d.presentation(),"metadata境界");
        assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original,"{variant} no-op preserves the original");
        set(&mut d,id,vec![(0,0,CellInput::Value(CellValue::Number(3.))),(9,4,CellInput::Formula("SEQUENCE(1)".into()))]);
        let result=opened.source.save(d.presentation(),SaveOptions::default());
        if matches!(variant,"valid"|"prefixed"|"shadow-types"|"shadow-cells") {
            let bytes=result.unwrap_or_else(|e|panic!("{variant}: {e}"));assert_reads_back(&bytes,d.presentation());assert_eq!(ibis_ooxml::Package::open(bytes.clone()).unwrap().read("xl/metadata.xml").unwrap(),metadata);
            assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
        } else {
            assert!(result.is_err(),"{variant} must not be treated as standard dynamic metadata");assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert!(opened.source.save(d.presentation(),SaveOptions::default()).is_err());
        }
    }
}


#[test]
fn dynamic_mode_requires_referenced_xldapr_future_flag() {
    for (variant, dynamic) in [("valid",true),("true",true),("cm-leading-zero",true),("false",false),("zero",false),("missing-flag",false),("bad-flag",false),("cm-zero",false),("cm-outside",false),("v-outside",false),("other-type",false),("foreign-root",false),("foreign-properties",false),("wrong-uri",false)] {
        let (_,stock)=open("xlsx-sales.xlsx");let mut initial=Document::new(stock.presentation.clone());let(id,shape)=(initial.ids().new_slide_id(),initial.ids().new_shape_id());initial.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"dynamic識別".into(),slide:id,shape})]).unwrap();
        set(&mut initial,id,vec![(0,0,CellInput::Value(CellValue::Number(1.))),(1,1,CellInput::Formula("SEQUENCE(A1)".into()))]);
        let mut package=ibis_ooxml::Package::open(stock.source.save(initial.presentation(),SaveOptions::default()).unwrap()).unwrap();
        let xml=package.xml_mut("xl/metadata.xml").unwrap();let root=xml.root_element().unwrap();let prop=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="dynamicArrayProperties").unwrap();
        match variant {
            "true"=>xml.set_attr(prop,"fDynamic","true"),"false"=>xml.set_attr(prop,"fDynamic","false"),"zero"=>xml.set_attr(prop,"fDynamic","0"),"missing-flag"=>xml.remove_attr(prop,"fDynamic"),"bad-flag"=>xml.set_attr(prop,"fDynamic","yes"),
            "foreign-root"=>xml.set_attr(root,"xmlns","urn:ibis:foreign-metadata"),
            "foreign-properties"=>{xml.set_attr(root,"xmlns:probe","urn:ibis:foreign-metadata");xml.rename(prop,"probe:dynamicArrayProperties");},
            "wrong-uri"=>{let ext=xml.parent(prop).unwrap();xml.set_attr(ext,"uri","{IBIS-OTHER-EXTENSION}");},
            "other-type"=>{let types=xml.child(root,"metadataTypes").unwrap();let kind=xml.child(types,"metadataType").unwrap();xml.set_attr(kind,"name","IBIS_OTHER");},
            "v-outside"=>{let cells=xml.child(root,"cellMetadata").unwrap();let bk=xml.child(cells,"bk").unwrap();let rc=xml.child(bk,"rc").unwrap();xml.set_attr(rc,"v","1");},
            _=>{},
        }
        if variant.starts_with("cm-") {let xml=package.xml_mut("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let c=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="c"&&xml.attr(n,"r")==Some("B2")).unwrap();xml.set_attr(c,"cm",match variant {"cm-zero"=>"0","cm-outside"=>"2",_=>"01"});}
        let original=package.save(Default::default()).unwrap();let opened=ibis_pptx::open(original.clone()).unwrap();let sh=sheet_of_slide(&opened.presentation.slides[1]).unwrap();assert_eq!(sh.spills.contains_key(&(1,1)),dynamic,"{variant} reader mode");
        assert_eq!(opened.source.save(&opened.presentation,SaveOptions::default()).unwrap(),original,"{variant} no-op retains original metadata");
        let mut d=Document::new(opened.presentation.clone());let id=slide(d.presentation(),"dynamic識別");set(&mut d,id,vec![(0,0,CellInput::Value(CellValue::Number(3.)))]);
        assert_eq!(value(d.presentation(),"dynamic識別",2,1),if dynamic {CellValue::Number(2.)} else {CellValue::Empty},"{variant} recomputed tail");assert!(d.undo());assert_eq!(*d.presentation(),opened.presentation);assert!(d.redo());assert_eq!(value(d.presentation(),"dynamic識別",2,1),if dynamic {CellValue::Number(2.)} else {CellValue::Empty},"{variant} redone tail");
    }
}


#[test]
fn newly_typed_dynamic_formulas_do_not_reuse_non_dynamic_future_records() {
    for (flag, dynamic) in [(Some("1"),true),(Some("0"),false),(Some("false"),false),(None,false)] {
        let label=flag.unwrap_or("absent");let (_,stock)=open("xlsx-sales.xlsx");let mut initial=Document::new(stock.presentation.clone());let(id,shape)=(initial.ids().new_slide_id(),initial.ids().new_shape_id());initial.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"新dynamic保持".into(),slide:id,shape})]).unwrap();
        set(&mut initial,id,vec![(0,0,CellInput::Value(CellValue::Number(1.))),(1,1,CellInput::Formula("SEQUENCE(A1)".into()))]);let mut package=ibis_ooxml::Package::open(stock.source.save(initial.presentation(),SaveOptions::default()).unwrap()).unwrap();
        let xml=package.xml_mut("xl/metadata.xml").unwrap();let root=xml.root_element().unwrap();let prop=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="dynamicArrayProperties").unwrap();if let Some(flag)=flag {xml.set_attr(prop,"fDynamic",flag);} else {xml.remove_attr(prop,"fDynamic");}
        if dynamic {let xml=package.xml_mut("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let c=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="c"&&xml.attr(n,"r")==Some("B2")).unwrap();xml.set_attr(c,"cm","01");let xml=package.xml_mut("xl/metadata.xml").unwrap();let root=xml.root_element().unwrap();let cells=xml.child(root,"cellMetadata").unwrap();let bk=xml.child(cells,"bk").unwrap();let rc=xml.child(bk,"rc").unwrap();xml.set_attr(rc,"t","01");xml.set_attr(rc,"v","00");}
        let original=package.save(Default::default()).unwrap();
        let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");std::fs::create_dir_all(&dir).unwrap();std::fs::write(dir.join(format!("new-dynamic-{label}-source.xlsx")),&original).unwrap();
        let original_metadata=package.read("xl/metadata.xml").unwrap();let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let id=slide(d.presentation(),"新dynamic保持");
        set(&mut d,id,vec![(9,5,CellInput::Formula("SEQUENCE(A1)".into())),(9,7,CellInput::Formula("F10#*2".into()))]);let one=save(&opened,d.presentation(),&format!("new-dynamic-{label}-one.xlsx"));assert_reads_back(&one,d.presentation());
        let one_opened=ibis_pptx::open(one.clone()).unwrap();let sh=sheet_of_slide(&one_opened.presentation.slides[1]).unwrap();assert_eq!(sh.spills.contains_key(&(1,1)),dynamic,"old {label} mode is retained");assert!(sh.spills.contains_key(&(9,5)),"new formula {label} must remain dynamic after reading its cache");assert!(sh.spills.contains_key(&(9,7)),"new reference {label} must remain dynamic");
        if dynamic {assert_eq!(ibis_ooxml::Package::open(one.clone()).unwrap().read("xl/metadata.xml").unwrap(),original_metadata);}
        assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),one);
        let mut d=Document::new(one_opened.presentation.clone());let id=slide(d.presentation(),"新dynamic保持");set(&mut d,id,vec![(0,0,CellInput::Value(CellValue::Number(3.)))]);
        for (r,n) in [(9,1.),(10,2.),(11,3.)] {assert_eq!(value(d.presentation(),"新dynamic保持",r,5),CellValue::Number(n));assert_eq!(value(d.presentation(),"新dynamic保持",r,7),CellValue::Number(n*2.));}
        assert_eq!(value(d.presentation(),"新dynamic保持",2,1),if dynamic {CellValue::Number(2.)} else {CellValue::Empty});let three=save(&one_opened,d.presentation(),&format!("new-dynamic-{label}-three.xlsx"));assert_reads_back(&three,d.presentation());
        assert!(d.undo());assert_eq!(one_opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),one);assert!(d.redo());assert_eq!(one_opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),three);
    }
}


#[test]
fn dynamic_metadata_attributes_are_unqualified_and_order_independent() {
    let (_, stock) = open("xlsx-sales.xlsx");
    let mut initial = Document::new(stock.presentation.clone());
    let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id());
    initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "属性境界".into(), slide: id, shape })]).unwrap();
    set(&mut initial, id, vec![(0, 0, CellInput::Value(CellValue::Number(1.))), (1, 1, CellInput::Formula("SEQUENCE(A1)".into()))]);
    let seed = stock.source.save(initial.presentation(), SaveOptions::default()).unwrap();
    let fields = [("type-name", "name", "XLDAPR", "OTHER"), ("future-name", "name", "XLDAPR", "OTHER"),
        ("uri", "uri", "{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}", "OTHER"),
        ("flag", "fDynamic", "1", "0"), ("rc-t", "t", "1", "9"), ("rc-v", "v", "0", "9"),
        ("cm", "cm", "1", "9"), ("formula-t", "t", "array", "shared"), ("formula-ref", "ref", "B2", "C3")];
    for (field, attribute, proper, foreign) in fields {
        for order in ["foreign-only", "foreign-first", "proper-first"] {
            let label = format!("{field}-{order}");
            let mut package = ibis_ooxml::Package::open(seed.clone()).unwrap();
            let part = if matches!(field, "cm" | "formula-t" | "formula-ref") { "xl/worksheets/sheet4.xml" } else { "xl/metadata.xml" };
            let xml = package.xml_mut(part).unwrap(); let root = xml.root_element().unwrap();
            xml.set_attr(root, "xmlns:probe", "urn:ibis:foreign-attribute");
            let target = xml.descendants(root).into_iter().find(|&n| match field {
                "type-name" => xml.local_name(n) == "metadataType", "future-name" => xml.local_name(n) == "futureMetadata",
                "uri" => xml.local_name(n) == "ext", "flag" => xml.local_name(n) == "dynamicArrayProperties",
                "rc-t" | "rc-v" => xml.local_name(n) == "rc", "cm" => xml.local_name(n) == "c" && xml.attr(n, "r") == Some("B2"),
                _ => xml.local_name(n) == "f",
            }).unwrap();
            xml.remove_attr(target, attribute);
            if order == "proper-first" { xml.set_attr(target, attribute, proper); }
            xml.set_attr(target, &format!("probe:{attribute}"), if order == "foreign-only" { proper } else { foreign });
            if order == "foreign-first" { xml.set_attr(target, attribute, proper); }
            let original = package.save(Default::default()).unwrap();
            let opened = ibis_pptx::open(original.clone()).unwrap(); let sh = sheet_of_slide(&opened.presentation.slides[1]).unwrap();
            let dynamic = order != "foreign-only";
            assert_eq!(sh.spills.contains_key(&(1, 1)), dynamic, "{label} mode");
            if dynamic { assert_eq!(sh.spills[&(1, 1)], CellRange { first_row: 1, last_row: 1, first_column: 1, last_column: 1 }, "{label} proper ref wins"); }
            assert_eq!(sh.cell(1, 1).unwrap().formula.as_deref(), Some("SEQUENCE(A1)"), "{label} formula retained");
            assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), original, "{label} no-op ZIP");
            let mut d = Document::new(opened.presentation.clone()); let id = slide(d.presentation(), "属性境界");
            set(&mut d, id, vec![(0, 0, CellInput::Value(CellValue::Number(3.))), (9, 5, CellInput::Formula("SEQUENCE(A1)".into()))]);
            assert_eq!(value(d.presentation(), "属性境界", 2, 1), if dynamic { CellValue::Number(2.) } else { CellValue::Empty }, "{label} tail");
            let result = opened.source.save(d.presentation(), SaveOptions::default());
            if order == "foreign-only" && matches!(field, "type-name" | "future-name") {
                assert!(result.is_err(), "{label} incomplete registration is unsupported");
            } else {
                let bytes = result.unwrap_or_else(|e| panic!("{label}: {e}")); assert_reads_back(&bytes, d.presentation());
                let reread = ibis_pptx::open(bytes.clone()).unwrap(); let sh = sheet_of_slide(&reread.presentation.slides[1]).unwrap();
                assert_eq!(sh.spills.contains_key(&(1, 1)), dynamic, "{label} saved old mode"); assert!(sh.spills.contains_key(&(9, 5)), "{label} allocated new mode");
                let saved_xml = ibis_ooxml::Package::open(bytes.clone()).unwrap().read(part).unwrap();
                assert!(String::from_utf8(saved_xml).unwrap().contains(&format!("probe:{attribute}=\"{}\"", if order == "foreign-only" { proper } else { foreign })), "{label} foreign attribute preserved");
                let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");
                std::fs::write(dir.join(format!("attribute-{label}-source.xlsx")), &original).unwrap(); std::fs::write(dir.join(format!("attribute-{label}-saved.xlsx")), &bytes).unwrap();
                assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original); assert!(d.redo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), bytes);
                continue;
            }
            assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original); assert!(d.redo()); assert!(opened.source.save(d.presentation(), SaveOptions::default()).is_err());
        }
    }
}

#[test]
fn dynamic_metadata_boolean_uses_xml_schema_lexical_values() {
    for order in ["plain", "foreign-first", "proper-first"] {
    for (flag, dynamic) in [(Some("1"), true), (Some("true"), true), (Some("0"), false), (Some("false"), false), (Some("on"), false), (Some("off"), false), (Some("yes"), false), (None, false), (Some(" true "), true), (Some(" 0 "), false), (Some("\u{a0}true\u{a0}"), false)] {
        let (_, stock) = open("xlsx-sales.xlsx"); let mut initial = Document::new(stock.presentation.clone());
        let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id());
        initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "Boolean境界".into(), slide: id, shape })]).unwrap();
        set(&mut initial, id, vec![(0, 0, CellInput::Value(CellValue::Number(1.))), (1, 1, CellInput::Formula("SEQUENCE(A1)".into()))]);
        let mut package = ibis_ooxml::Package::open(stock.source.save(initial.presentation(), SaveOptions::default()).unwrap()).unwrap();
        let xml = package.xml_mut("xl/metadata.xml").unwrap(); let root = xml.root_element().unwrap();
        let props = xml.descendants(root).into_iter().find(|&n| xml.local_name(n) == "dynamicArrayProperties").unwrap();
        xml.remove_attr(props, "fDynamic");
        if order != "plain" { xml.set_attr(root, "xmlns:probe", "urn:ibis:boolean-shadow"); }
        if order == "foreign-first" { xml.set_attr(props, "probe:fDynamic", if dynamic { "0" } else { "1" }); }
        if let Some(flag) = flag { xml.set_attr(props, "fDynamic", flag); }
        if order == "proper-first" { xml.set_attr(props, "probe:fDynamic", if dynamic { "0" } else { "1" }); }
        let original = package.save(Default::default()).unwrap(); let opened = ibis_pptx::open(original.clone()).unwrap();
        assert_eq!(sheet_of_slide(&opened.presentation.slides[1]).unwrap().spills.contains_key(&(1, 1)), dynamic, "{flag:?} {order}");
        assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), original);
        let mut d = Document::new(opened.presentation.clone()); let id = slide(d.presentation(), "Boolean境界");
        set(&mut d, id, vec![(0, 0, CellInput::Value(CellValue::Number(3.))), (9, 5, CellInput::Formula("SEQUENCE(A1)".into()))]);
        assert_eq!(value(d.presentation(), "Boolean境界", 2, 1), if dynamic { CellValue::Number(2.) } else { CellValue::Empty });
        let bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap(); assert_reads_back(&bytes, d.presentation());
        let reread = ibis_pptx::open(bytes.clone()).unwrap(); let sh = sheet_of_slide(&reread.presentation.slides[1]).unwrap();
        assert_eq!(sh.spills.contains_key(&(1, 1)), dynamic); assert!(sh.spills.contains_key(&(9, 5)));
        assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original); assert!(d.redo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), bytes);
    }
}
}


#[test]
fn dynamic_formula_elements_keep_foreign_shadows_and_hidden_modes() {
    let (_, stock) = open("xlsx-sales.xlsx"); let mut initial = Document::new(stock.presentation.clone());
    let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id());
    initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "要素境界".into(), slide: id, shape })]).unwrap();
    set(&mut initial, id, vec![(0, 0, CellInput::Value(CellValue::Number(1.))), (1, 1, CellInput::Formula("SEQUENCE(A1)".into()))]);
    let seed = stock.source.save(initial.presentation(), SaveOptions::default()).unwrap();
    for state in ["visible", "hidden", "veryHidden"] {
        for order in ["foreign-only", "foreign-first", "proper-first"] {
            let label = format!("{state}-{order}"); let dynamic = order != "foreign-only";
            let mut package = ibis_ooxml::Package::open(seed.clone()).unwrap();
            let xml = package.xml_mut("xl/worksheets/sheet4.xml").unwrap(); let root = xml.root_element().unwrap();
            xml.set_attr(root, "xmlns:probe", "urn:ibis:foreign-formula");
            let c = xml.descendants(root).into_iter().find(|&n| xml.local_name(n) == "c" && xml.attr(n, "r") == Some("B2")).unwrap();
            let f = xml.child(c, "f").unwrap(); let shadow = xml.deep_clone(f);
            xml.rename(shadow, "probe:f"); xml.set_text(shadow, "SEQUENCE(99)"); xml.set_attr(shadow, "probe:sentinel", "opaque-formula");
            if order == "proper-first" { xml.insert(c, 1, shadow); } else { xml.insert_before(f, shadow); }
            if order == "foreign-only" { xml.detach(f); }
            if state != "visible" { let xml = package.xml_mut("xl/workbook.xml").unwrap(); let root = xml.root_element().unwrap(); let sh = xml.descendants(root).into_iter().find(|&n| xml.local_name(n) == "sheet" && xml.attr(n, "name") == Some("要素境界")).unwrap(); xml.set_attr(sh, "state", state); }
            let original = package.save(Default::default()).unwrap(); let opened = ibis_pptx::open(original.clone()).unwrap();
            let workbook = opened.presentation.workbook.as_ref().unwrap();
            let entry = workbook.sheets.iter().find(|s| s.name == "要素境界").unwrap();
            let sh = if state == "visible" { sheet_of_slide(opened.presentation.slide(entry.slide.unwrap()).unwrap()).unwrap() } else { entry.hidden.as_deref().unwrap() };
            assert_eq!(sh.spills.contains_key(&(1, 1)), dynamic, "{label} mode");
            assert_eq!(sh.cell(1, 1).unwrap().formula.as_deref(), if dynamic { Some("SEQUENCE(A1)") } else { None }, "{label} ignores opaque formula");
            assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), original, "{label} no-op ZIP");
            if state != "visible" { continue; }
            let mut d = Document::new(opened.presentation.clone()); let id = slide(d.presentation(), "要素境界");
            let mut edits = vec![(0, 0, CellInput::Value(CellValue::Number(3.))), (9, 5, CellInput::Formula("SEQUENCE(A1)".into()))];
            if dynamic { edits.push((1, 1, CellInput::Formula("SEQUENCE(A1)*2".into()))); }
            set(&mut d, id, edits); let bytes = save(&opened, d.presentation(), &format!("formula-element-{label}-saved.xlsx")); assert_reads_back(&bytes, d.presentation());
            let saved = ibis_ooxml::Package::open(bytes.clone()).unwrap(); let xml = saved.xml("xl/worksheets/sheet4.xml").unwrap(); let root = xml.root_element().unwrap();
            let shadow = xml.descendants(root).into_iter().find(|&n| xml.name(n) == "probe:f").unwrap();
            assert_eq!(xml.text(shadow), "SEQUENCE(99)"); assert_eq!(xml.attr(shadow, "probe:sentinel"), Some("opaque-formula"));
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit"); std::fs::write(dir.join(format!("formula-element-{label}-source.xlsx")), &original).unwrap();
            assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original); assert!(d.redo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), bytes);
        }
    }
}


#[test]
fn dynamic_metadata_indices_accept_xml_whitespace_without_losing_references() {
    let (_, stock) = open("xlsx-sales.xlsx"); let mut initial = Document::new(stock.presentation.clone());
    let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id()); initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "数値境界".into(), slide: id, shape })]).unwrap();
    set(&mut initial, id, vec![(0, 0, CellInput::Value(CellValue::Number(1.))), (1, 1, CellInput::Formula("SEQUENCE(A1)".into()))]);
    let seed = stock.source.save(initial.presentation(), SaveOptions::default()).unwrap();
    for field in ["cm", "t", "v"] {
        for variant in ["plain", "leading", "space", "tab", "nbsp", "internal", "overflow", "outside", "negative"] {
            let zero = field == "v"; let base = if zero { "0" } else { "1" };
            let lexical = match variant { "leading" => format!("00{base}"), "space" => format!(" 00{base} "), "tab" => format!("\t{base}\n"), "nbsp" => format!("\u{a0}{base}\u{a0}"), "internal" => format!("0 {base}"), "overflow" => "4294967296".into(), "outside" => if zero { "1" } else { "2" }.into(), "negative" => "-1".into(), _ => base.into() };
            let dynamic = matches!(variant, "plain" | "leading" | "space" | "tab"); let label = format!("{field}-{variant}");
            let mut package = ibis_ooxml::Package::open(seed.clone()).unwrap(); let part = if field == "cm" { "xl/worksheets/sheet4.xml" } else { "xl/metadata.xml" };
            let xml = package.xml_mut(part).unwrap(); let root = xml.root_element().unwrap(); let target = xml.descendants(root).into_iter().find(|&n| if field == "cm" { xml.local_name(n) == "c" && xml.attr(n, "r") == Some("B2") } else { xml.local_name(n) == "rc" }).unwrap(); xml.set_attr(target, field, &lexical);
            let original = package.save(Default::default()).unwrap(); let metadata = package.read("xl/metadata.xml").unwrap(); let opened = ibis_pptx::open(original.clone()).unwrap();
            assert_eq!(sheet_of_slide(&opened.presentation.slides[1]).unwrap().spills.contains_key(&(1, 1)), dynamic, "{label} mode"); assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), original);
            let mut d = Document::new(opened.presentation.clone()); let id = slide(d.presentation(), "数値境界"); set(&mut d, id, vec![(0, 0, CellInput::Value(CellValue::Number(3.))), (9, 5, CellInput::Formula("SEQUENCE(A1)".into()))]);
            assert_eq!(value(d.presentation(), "数値境界", 2, 1), if dynamic { CellValue::Number(2.) } else { CellValue::Empty });
            if field == "v" && variant == "outside" {
                assert!(opened.source.save(d.presentation(), SaveOptions::default()).is_err(), "{label} must not resolve an existing orphan reference");
                assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
                assert!(d.redo()); assert!(opened.source.save(d.presentation(), SaveOptions::default()).is_err());
                continue;
            }
            let bytes = save(&opened, d.presentation(), &format!("metadata-index-{label}-saved.xlsx")); assert_reads_back(&bytes, d.presentation()); let saved = ibis_ooxml::Package::open(bytes.clone()).unwrap();
            if dynamic { assert_eq!(saved.read("xl/metadata.xml").unwrap(), metadata, "{label} existing record retained"); }
            if dynamic && field == "cm" {
                let before = package.xml("xl/worksheets/sheet4.xml").unwrap(); let before_root = before.root_element().unwrap();
                let after = saved.xml("xl/worksheets/sheet4.xml").unwrap(); let after_root = after.root_element().unwrap();
                let before_cell = before.descendants(before_root).into_iter().find(|&n| before.local_name(n)=="c" && before.attr(n,"r")==Some("B2")).unwrap();
                let after_cell = after.descendants(after_root).into_iter().find(|&n| after.local_name(n)=="c" && after.attr(n,"r")==Some("B2")).unwrap();
                assert_eq!(after.attr(after_cell,"cm"), before.attr(before_cell,"cm"), "{label} existing cm text kept");
            }

            let reread = ibis_pptx::open(bytes.clone()).unwrap(); let sh = sheet_of_slide(&reread.presentation.slides[1]).unwrap(); assert_eq!(sh.spills.contains_key(&(1, 1)), dynamic); assert!(sh.spills.contains_key(&(9, 5)));
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit"); std::fs::write(dir.join(format!("metadata-index-{label}-source.xlsx")), &original).unwrap();
            assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original); assert!(d.redo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), bytes);
        }
    }
}


#[test]
fn adding_metadata_does_not_resolve_orphan_cell_indices_in_any_sheet_state() {
    let (_, stock) = open("xlsx-sales.xlsx"); let mut initial = Document::new(stock.presentation.clone());
    let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id()); initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "cm孤立境界".into(), slide: id, shape })]).unwrap();
    set(&mut initial, id, vec![(0, 0, CellInput::Value(CellValue::Number(1.))), (1, 1, CellInput::Formula("SEQUENCE(A1)".into()))]);
    let seed = stock.source.save(initial.presentation(), SaveOptions::default()).unwrap();
    for metadata_present in [true, false] {
    for state in ["visible", "hidden", "veryHidden"] {
        for attribute in ["proper", "foreign"] {
            let label = format!("{}-{state}-{attribute}", if metadata_present { "with" } else { "without" }); let mut package = ibis_ooxml::Package::open(seed.clone()).unwrap();
            let xml = package.xml_mut("xl/metadata.xml").unwrap(); let root = xml.root_element().unwrap(); let props = xml.descendants(root).into_iter().find(|&n| xml.local_name(n)=="dynamicArrayProperties").unwrap(); xml.set_attr(props,"fDynamic","0");
            let xml = package.xml_mut("xl/worksheets/sheet4.xml").unwrap(); let root = xml.root_element().unwrap(); let c = xml.descendants(root).into_iter().find(|&n| xml.local_name(n)=="c" && xml.attr(n,"r")==Some("B2")).unwrap();
            if attribute=="proper" { xml.set_attr(c,"cm","2"); } else { xml.set_attr(root,"xmlns:probe","urn:ibis:orphan-cell-index"); xml.set_attr(c,"probe:cm","2"); }
            if state!="visible" { let xml=package.xml_mut("xl/workbook.xml").unwrap(); let root=xml.root_element().unwrap(); let sh=xml.descendants(root).into_iter().find(|&n| xml.local_name(n)=="sheet" && xml.attr(n,"name")==Some("cm孤立境界")).unwrap(); xml.set_attr(sh,"state",state); }
            if !metadata_present {
                let xml=package.xml_mut("xl/worksheets/sheet4.xml").unwrap(); let root=xml.root_element().unwrap(); let c=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="c" && xml.attr(n,"r")==Some("B2")).unwrap();
                xml.set_attr(c, if attribute=="proper" {"cm"} else {"probe:cm"}, "1");
                if attribute=="foreign" {xml.remove_attr(c,"cm");}
                let relationships=package.relationships("xl/workbook.xml").unwrap();
                for rel in relationships.into_iter().filter(|r|r.kind.ends_with("/sheetMetadata")) {package.remove_relationship("xl/workbook.xml",&rel.id).unwrap();}
                package.remove("xl/metadata.xml"); package.remove_override("xl/metadata.xml").unwrap();
            }
            let original=package.save(Default::default()).unwrap(); let opened=ibis_pptx::open(original.clone()).unwrap();
            let old_sheet = |p: &Presentation| -> bool { let entry=p.workbook.as_deref().unwrap().sheets.iter().find(|s|s.name=="cm孤立境界").unwrap(); if let Some(slide)=entry.slide { sheet_of_slide(p.slide(slide).unwrap()).unwrap().spills.contains_key(&(1,1)) } else { entry.hidden.as_deref().unwrap().spills.contains_key(&(1,1)) } };
            assert!(!old_sheet(&opened.presentation)); assert_eq!(opened.source.save(&opened.presentation,SaveOptions::default()).unwrap(),original);
            let mut d=Document::new(opened.presentation.clone()); let editing_slide=if state=="visible" { slide(d.presentation(),"cm孤立境界") } else { d.presentation().slides[0].id };
            set(&mut d,editing_slide,vec![(9,5,CellInput::Formula("SEQUENCE(3)".into()))]);
            let result=opened.source.save(d.presentation(),SaveOptions::default());
            if attribute=="proper" {
                if let Ok(ref bytes)=result { let reopened=ibis_pptx::read(bytes).unwrap(); assert!(!old_sheet(&reopened),"{label} old cm must not become the appended block"); }
                assert!(result.is_err(),"{label} orphan collision must be explicit");
                assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original); assert!(d.redo()); assert!(opened.source.save(d.presentation(),SaveOptions::default()).is_err());
            } else {
                let bytes=result.unwrap(); assert_reads_back(&bytes,d.presentation()); assert!(!old_sheet(&ibis_pptx::read(&bytes).unwrap()),"{label} foreign cm does not supply a standard index");
                let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit"); std::fs::write(dir.join(format!("orphan-cell-{label}-source.xlsx")),&original).unwrap(); std::fs::write(dir.join(format!("orphan-cell-{label}-saved.xlsx")),&bytes).unwrap();
                assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original); assert!(d.redo()); assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
            }
        }
    }
}
}


#[test]
fn literal_metadata_preserves_plain_control_and_refuses_opaque_or_colliding_blocks() {
    let seed=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-composite-metadata.xlsx")).unwrap();
    for variant in ["composite", "plain", "opaque-attribute", "opaque-child", "bad-rc", "orphan-cm"] {
        let mut package=ibis_ooxml::Package::open(seed.clone()).unwrap();
        let xml=package.xml_mut("xl/metadata.xml").unwrap(); let root=xml.root_element().unwrap(); let cells=xml.child(root,"cellMetadata").unwrap(); let blocks:Vec<_>=xml.children_named(cells,"bk").collect(); let block=blocks[1];
        match variant {
            "opaque-attribute"=>{let first=blocks[0];xml.set_attr(first,"ibis:sentinel","opaque-plain-block");},
            "opaque-child"=>{let child=xml.append_element(block,"ibis:payload");xml.set_text(child,"opaque cell metadata child");},
            "bad-rc"=>{let rc=xml.children_named(block,"rc").find(|&n|xml.attr(n,"t")==Some("2")).unwrap();xml.set_attr(rc,"v","not-an-index");},
            _=>{},
        }
        if matches!(variant,"plain"|"opaque-attribute") {
            let xml=package.xml_mut("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let cell=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="c"&&xml.attr(n,"r")==Some("B2")).unwrap();xml.set_attr(cell,"cm","1");
        }
        if variant=="orphan-cm" {
            let xml=package.xml_mut("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let data=xml.child(root,"sheetData").unwrap();let row=xml.append_element(data,"row");xml.set_attr(row,"r","12");let c=xml.append_element(row,"c");xml.set_attr(c,"r","C12");xml.set_attr(c,"cm","3");let v=xml.append_element(c,"v");xml.set_text(v,"7");
        }
        let original=package.save(Default::default()).unwrap();let metadata=package.read("xl/metadata.xml").unwrap();let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let id=slide(d.presentation(),"複合保持");
        assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
        set(&mut d,id,vec![(1,1,CellInput::Value(CellValue::Number(42.)))]);let result=opened.source.save(d.presentation(),SaveOptions::default());
        if matches!(variant,"composite"|"plain") {
            let bytes=result.unwrap_or_else(|e|panic!("{variant}: {e}"));assert_reads_back(&bytes,d.presentation());let saved=ibis_ooxml::Package::open(bytes.clone()).unwrap();let xml=saved.xml("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let c=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="c"&&xml.attr(n,"r")==Some("B2")).unwrap();assert_eq!(xml.attr(c,"cm"),if variant=="plain" {None}else{Some("3")});
            if variant=="plain" {assert_eq!(saved.read("xl/metadata.xml").unwrap(),metadata);}
            let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");std::fs::write(dir.join(format!("literal-boundary-{variant}-source.xlsx")),&original).unwrap();std::fs::write(dir.join(format!("literal-boundary-{variant}-saved.xlsx")),&bytes).unwrap();
            assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
        } else {
            assert!(result.is_err(),"{variant} must be refused without silently losing unrelated metadata");assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert!(opened.source.save(d.presentation(),SaveOptions::default()).is_err());
        }
    }
}

#[test]
fn reactivating_literal_formula_retains_its_unrelated_metadata() {
    let seed=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-composite-metadata.xlsx")).unwrap();
    let opened=ibis_pptx::open(seed).unwrap(); let mut d=Document::new(opened.presentation.clone()); let id=slide(d.presentation(),"複合保持");
    set(&mut d,id,vec![(1,1,CellInput::Value(CellValue::Number(42.)))]);
    let literal=opened.source.save(d.presentation(),SaveOptions::default()).unwrap();
    let opened=ibis_pptx::open(literal.clone()).unwrap(); let mut d=Document::new(opened.presentation.clone()); let id=slide(d.presentation(),"複合保持");
    set(&mut d,id,vec![(1,1,CellInput::Formula("SEQUENCE(A1)*5".into()))]);
    let bytes=opened.source.save(d.presentation(),SaveOptions::default()).unwrap(); assert_reads_back(&bytes,d.presentation());
    let pkg=ibis_ooxml::Package::open(bytes.clone()).unwrap(); let xml=pkg.xml("xl/worksheets/sheet4.xml").unwrap(); let root=xml.root_element().unwrap();
    let c=xml.descendants(root).into_iter().find(|&n|xml.local_name(n)=="c"&&xml.attr(n,"r")==Some("B2")).unwrap();
    let cm=xml.attr(c,"cm").unwrap().parse::<usize>().unwrap(); let xml=pkg.xml("xl/metadata.xml").unwrap();let root=xml.root_element().unwrap(); let cells=xml.child(root,"cellMetadata").unwrap(); let bk=xml.children_named(cells,"bk").nth(cm-1).unwrap();
    assert!(xml.children_named(bk,"rc").any(|n|xml.attr(n,"t")==Some("2")&&xml.attr(n,"v")==Some("0")),"reactivating formula must retain the same cell's unrelated record");
    assert!(xml.children_named(bk,"rc").any(|n|xml.attr(n,"t")==Some("1")&&xml.attr(n,"v")==Some("0")));
    assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),literal); assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");std::fs::write(dir.join("dynamic-reactivated-source.xlsx"),&literal).unwrap();std::fs::write(dir.join("dynamic-reactivated-saved.xlsx"),&bytes).unwrap();
    let reopened=ibis_pptx::open(bytes).unwrap();let mut grown=Document::new(reopened.presentation.clone());let id=slide(grown.presentation(),"複合保持");set(&mut grown,id,vec![(0,0,CellInput::Value(CellValue::Number(3.)))]);
    let sh=sheet_of_slide(grown.presentation().slides.iter().find(|s|s.id==id).unwrap()).unwrap();assert!(sh.spills.contains_key(&(1,1)));assert_eq!(sh.cell(3,1).unwrap().value,CellValue::Number(15.));
    let grown_bytes=reopened.source.save(grown.presentation(),SaveOptions::default()).unwrap();assert_reads_back(&grown_bytes,grown.presentation());std::fs::write(dir.join("dynamic-reactivated-grown.xlsx"),&grown_bytes).unwrap();
}

#[test]
fn reactivating_metadata_refuses_opaque_records_and_orphan_collisions() {
    let seed=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-composite-metadata.xlsx")).unwrap();
    let source=ibis_pptx::open(seed).unwrap();let mut d=Document::new(source.presentation.clone());let id=slide(d.presentation(),"複合保持");set(&mut d,id,vec![(1,1,CellInput::Value(CellValue::Number(42.)))]);
    let literal=source.source.save(d.presentation(),SaveOptions::default()).unwrap();
    for variant in ["opaque-child","bad-rc","existing-xldapr","orphan-cm"] {
        let mut pkg=ibis_ooxml::Package::open(literal.clone()).unwrap();let xml=pkg.xml_mut("xl/metadata.xml").unwrap();let root=xml.root_element().unwrap();let cells=xml.child(root,"cellMetadata").unwrap();let block=xml.children_named(cells,"bk").nth(2).unwrap();
        match variant {
            "opaque-child"=>{let child=xml.append_element(block,"ibis:payload");xml.set_text(child,"opaque");},
            "bad-rc"=>{let rc=xml.child(block,"rc").unwrap();xml.set_attr(rc,"v","bad");},
            "existing-xldapr"=>{let rc=xml.append_element(block,"rc");xml.set_attr(rc,"t","1");xml.set_attr(rc,"v","5");},
            _=>{},
        }
        if variant=="orphan-cm" {let xml=pkg.xml_mut("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let data=xml.child(root,"sheetData").unwrap();let row=xml.append_element(data,"row");xml.set_attr(row,"r","12");let c=xml.append_element(row,"c");xml.set_attr(c,"r","C12");xml.set_attr(c,"cm","4");let v=xml.append_element(c,"v");xml.set_text(v,"7");}
        let original=pkg.save(Default::default()).unwrap();let source=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(source.presentation.clone());let id=slide(d.presentation(),"複合保持");assert_eq!(source.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
        set(&mut d,id,vec![(1,1,CellInput::Formula("SEQUENCE(A1)*5".into()))]);assert!(source.source.save(d.presentation(),SaveOptions::default()).is_err(),"{variant}");assert!(d.undo());assert_eq!(source.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert!(source.source.save(d.presentation(),SaveOptions::default()).is_err());
    }
}

#[test]
fn aggregate_count_options_save_and_recalculate_after_reopening() {
    use ibis_calc::formula::Axis;
    let (original,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let(s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());
    let mut edits=vec![];for(row,input)in[(0,CellInput::Value(CellValue::Number(5.))),(1,CellInput::Formula("NA()".into())),(2,CellInput::Value(CellValue::Text("text".into()))),(3,CellInput::Formula("\"\"".into())),(4,CellInput::Formula("SUBTOTAL(3,A1:A4)".into())),(5,CellInput::Value(CellValue::Bool(true))),(7,CellInput::Value(CellValue::Number(8.)))] {edits.push(CellEdit {row,column:0,input});}
    for f in 2..=3 {for o in 0..8 {edits.push(CellEdit {row:o,column:f-1,input:CellInput::Formula(format!("AGGREGATE({f},{o},A1:A8)"))});}}
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"集計count options".into(),slide:s,shape}),Command::Sheet(SheetCommand::SetCells {slide:s,edits})]).unwrap();
    let check=|p:&Presentation,hidden:bool| {for o in 0..8 {let nested=o<4;let skip_errors=matches!(o,2|3|6|7);let hides=hidden&&o%2==1;let count=3-i32::from(nested)-i32::from(hides);let counta=7-i32::from(nested)-i32::from(skip_errors)-if hides{1+i32::from(!skip_errors)}else{0};assert_eq!(value(p,"集計count options",o,1),CellValue::Number(count as f64));assert_eq!(value(p,"集計count options",o,2),CellValue::Number(counta as f64));}};
    check(d.presentation(),false);let shown=save(&opened,d.presentation(),"aggregate-count-shown.xlsx");assert_reads_back(&shown,d.presentation());check(&ibis_pptx::read(&shown).unwrap(),false);assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),shown);
    d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:s,axis:Axis::Rows,first:1,last:1,hidden:true}),Command::Sheet(SheetCommand::Hide {slide:s,axis:Axis::Rows,first:7,last:7,hidden:true})]).unwrap();check(d.presentation(),true);let hidden=save(&opened,d.presentation(),"aggregate-count-hidden.xlsx");assert_reads_back(&hidden,d.presentation());check(&ibis_pptx::read(&hidden).unwrap(),true);assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),shown);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),hidden);
    let reopened=ibis_pptx::open(hidden.clone()).unwrap();let mut d=Document::new(reopened.presentation.clone());let id=slide(d.presentation(),"集計count options");d.execute(vec![Command::Sheet(SheetCommand::Hide {slide:id,axis:Axis::Rows,first:1,last:1,hidden:false}),Command::Sheet(SheetCommand::Hide {slide:id,axis:Axis::Rows,first:7,last:7,hidden:false})]).unwrap();check(d.presentation(),false);let restored=save(&reopened,d.presentation(),"aggregate-count-restored.xlsx");assert_reads_back(&restored,d.presentation());check(&ibis_pptx::read(&restored).unwrap(),false);assert!(d.undo());assert_eq!(reopened.source.save(d.presentation(),SaveOptions::default()).unwrap(),hidden);assert!(d.redo());assert_eq!(reopened.source.save(d.presentation(),SaveOptions::default()).unwrap(),restored);
}

#[test]
fn conditional_cell_is_operators_keep_mixed_references_through_row_insertion() {
    use ibis_doc_model::{ConditionalFormat,CfRule,CfKind,CfStyle,Rgb,sheet_command::Axis};
    let(_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let(s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"セル値比較参照".into(),slide:s,shape})]).unwrap();
    for(i,op)in ["lessThan","lessThanOrEqual","equal","notEqual","greaterThanOrEqual","greaterThan","between","notBetween"].into_iter().enumerate(){let mut formulas=vec!["B2+$B2+B$2+$B$2".into()];if i>=6 {formulas.push("B3+$B3+B$3+$B$3".into());}let format=ConditionalFormat {source_index:None,ranges:vec![CellRange {first_row:1,last_row:2,first_column:0,last_column:0}],rules:vec![CfRule {kind:CfKind::CellIs {operator:op.into(),formulas},priority:1,stop:i%2==0,style:Some(CfStyle {fill:Some(Rgb(0xFFC7CE)),..Default::default()})}]};d.execute(vec![Command::Sheet(SheetCommand::AddConditional {slide:s,format})]).unwrap();}
    let original=save(&opened,d.presentation(),"conditional-cellis-before-insert.xlsx");let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let s=slide(d.presentation(),"セル値比較参照");let before=conditional(d.presentation(),s);
    d.execute(vec![Command::Sheet(SheetCommand::Insert {slide:s,axis:Axis::Rows,at:1,count:1})]).unwrap();let changed=conditional(d.presentation(),s);assert_eq!(changed.len(),8);
    for(i,f)in changed.iter().enumerate(){assert_eq!(f.ranges,vec![CellRange {first_row:2,last_row:3,first_column:0,last_column:0}]);assert_eq!(f.rules[0].priority,before[i].rules[0].priority);assert_eq!(f.rules[0].stop,before[i].rules[0].stop);assert_eq!(f.rules[0].style,before[i].rules[0].style);let CfKind::CellIs {operator,formulas}=&f.rules[0].kind else{panic!()};let CfKind::CellIs {operator:old,..}=&before[i].rules[0].kind else{panic!()};assert_eq!(operator,old);assert_eq!(formulas[0],"B3+$B3+B$3+$B$3");if i>=6{assert_eq!(formulas[1],"B4+$B4+B$4+$B$4");}}
    let bytes=save(&opened,d.presentation(),"conditional-cellis-after-insert.xlsx");assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();assert_eq!(conditional(&p,slide(&p,"セル値比較参照")),changed);assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
}

#[test]
fn conditional_threshold_range_edit_retains_opaque_rule_and_unchanged_dxf() {
    use ibis_doc_model::{CfKind,ConditionalFormat,CfRule,CfStyle,Rgb};
    for kind in ["expression","cellIs"] {
        let(_,opened)=open("xlsx-sales.xlsx");let mut d=Document::new(opened.presentation.clone());let(s,shape)=(d.ids().new_slide_id(),d.ids().new_shape_id());let format=ConditionalFormat {source_index:None,ranges:vec![CellRange {first_row:0,last_row:2,first_column:0,last_column:0}],rules:vec![CfRule {kind:if kind=="expression" {CfKind::Expression("A1>10".into())}else{CfKind::CellIs {operator:"greaterThan".into(),formulas:vec!["10".into()]}},priority:1,stop:false,style:Some(CfStyle {fill:Some(Rgb(0xFFC7CE)),..Default::default()})}]};
        d.execute(vec![Command::Sheet(SheetCommand::AddSheet {index:1,name:"未知規則保持".into(),slide:s,shape}),Command::Sheet(SheetCommand::AddConditional {slide:s,format})]).unwrap();let mut pkg=ibis_ooxml::Package::open(opened.source.save(d.presentation(),SaveOptions::default()).unwrap()).unwrap();let xml=pkg.xml_mut("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:probe","urn:ibis:cf-opaque");let cf=xml.child(root,"conditionalFormatting").unwrap();xml.set_attr(cf,"probe:scope","keep-scope");let r=xml.child(cf,"cfRule").unwrap();xml.set_attr(r,"probe:token","keep-rule");let id=xml.attr(r,"dxfId").unwrap().parse::<usize>().unwrap();let f=xml.child(r,"formula").unwrap();xml.set_attr(f,"probe:formula","keep-formula");let payload=xml.append_element(r,"probe:payload");xml.set_text(payload,"owned unknown payload");
        let xml=pkg.xml_mut("xl/styles.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:probe","urn:ibis:cf-opaque");let list=xml.child(root,"dxfs").unwrap();let dxf=xml.children_named(list,"dxf").nth(id).unwrap();xml.set_attr(dxf,"probe:token","keep-dxf");let font=xml.append_element(dxf,"font");let va=xml.append_element(font,"vertAlign");xml.set_attr(va,"val","superscript");let styles=pkg.read("xl/styles.xml").unwrap();
        let original=pkg.save(Default::default()).unwrap();let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let s=slide(d.presentation(),"未知規則保持");let mut format=conditional(d.presentation(),s)[0].clone();format.ranges[0].first_row=1;format.rules[0].stop=true;format.rules[0].kind=if kind=="expression" {CfKind::Expression("A2>20".into())}else{CfKind::CellIs {operator:"greaterThanOrEqual".into(),formulas:vec!["20".into()]}};
        d.execute(vec![Command::Sheet(SheetCommand::UpdateConditional {slide:s,index:0,format})]).unwrap();let bytes=opened.source.save(d.presentation(),SaveOptions::default()).unwrap();let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");std::fs::write(dir.join(format!("conditional-opaque-{kind}-source.xlsx")),&original).unwrap();std::fs::write(dir.join(format!("conditional-opaque-{kind}-saved.xlsx")),&bytes).unwrap();
        let pkg=ibis_ooxml::Package::open(bytes.clone()).unwrap();let xml=pkg.xml("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let cf=xml.child(root,"conditionalFormatting").unwrap();let r=xml.child(cf,"cfRule").unwrap();assert_eq!(xml.attr(cf,"probe:scope"),Some("keep-scope"));assert_eq!(xml.attr(r,"probe:token"),Some("keep-rule"));assert_eq!(xml.attr(r,"dxfId").unwrap().parse::<usize>().unwrap(),id);let f=xml.child(r,"formula").unwrap();assert_eq!(xml.attr(f,"probe:formula"),Some("keep-formula"));assert!(xml.children_named(r,"payload").any(|n|xml.namespace(n)==Some("urn:ibis:cf-opaque")&&xml.text(n)=="owned unknown payload"));assert_eq!(pkg.read("xl/styles.xml").unwrap(),styles);
        assert_reads_back(&bytes,d.presentation());let p=ibis_pptx::read(&bytes).unwrap();assert_eq!(conditional(&p,slide(&p,"未知規則保持")),conditional(d.presentation(),s));assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    }
}

#[test]
fn conditional_origins_do_not_transfer_opaque_payload_to_new_rules() {
    let base=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit/conditional-opaque-expression-source.xlsx");
    // Generate independently, so this probe does not require another test's ordering.
    let(_,opened)=open("xlsx-sales.xlsx");let mut initial=Document::new(opened.presentation.clone());let id=slide(initial.presentation(),"月次売上");initial.execute(vec![Command::Sheet(SheetCommand::AddConditional {slide:id,format:formula_rule("B4>10",0xFFC7CE,false)})]).unwrap();let mut pkg=ibis_ooxml::Package::open(opened.source.save(initial.presentation(),SaveOptions::default()).unwrap()).unwrap();let xml=pkg.xml_mut("xl/worksheets/sheet1.xml").unwrap();let root=xml.root_element().unwrap();xml.set_attr(root,"xmlns:probe","urn:ibis:cf-origin");let cf=xml.children_named(root,"conditionalFormatting").last().unwrap();let rule=xml.child(cf,"cfRule").unwrap();xml.set_attr(rule,"probe:token","owned-original-only");let original=pkg.save(Default::default()).unwrap();
    for variant in ["add-clone","replace-identical","update-wrong-id"] {
        let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let id=slide(d.presentation(),"月次売上");let formats=conditional(d.presentation(),id);let at=formats.len()-1;let mut format=formats[at].clone();let imported=format.source_index;
        match variant {
            "add-clone"=>{d.execute(vec![Command::Sheet(SheetCommand::AddConditional {slide:id,format})]).unwrap();assert!(conditional(d.presentation(),id).last().unwrap().source_index.is_none());},
            "replace-identical"=>{d.execute(vec![Command::Sheet(SheetCommand::ClearConditional {slide:id,ranges:format.ranges.clone()}),Command::Sheet(SheetCommand::AddConditional {slide:id,format})]).unwrap();assert!(conditional(d.presentation(),id).last().unwrap().source_index.is_none());},
            _=>{format.source_index=Some(usize::MAX);format.rules[0].kind=ibis_doc_model::CfKind::Expression("B4>20".into());d.execute(vec![Command::Sheet(SheetCommand::UpdateConditional {slide:id,index:at,format})]).unwrap();assert_eq!(conditional(d.presentation(),id)[at].source_index,imported);},
        }
        let bytes=opened.source.save(d.presentation(),SaveOptions::default()).unwrap();assert_reads_back(&bytes,d.presentation());let pkg=ibis_ooxml::Package::open(bytes.clone()).unwrap();let xml=pkg.xml("xl/worksheets/sheet1.xml").unwrap();let root=xml.root_element().unwrap();let formats:Vec<_>=xml.children_named(root,"conditionalFormatting").collect();let last=xml.child(*formats.last().unwrap(),"cfRule").unwrap();assert_eq!(xml.attr(last,"probe:token"),if variant=="update-wrong-id" {Some("owned-original-only")}else{None});if variant=="add-clone" {let r=xml.child(formats[at],"cfRule").unwrap();assert_eq!(xml.attr(r,"probe:token"),Some("owned-original-only"));}
        let dir=base.parent().unwrap();std::fs::write(dir.join(format!("conditional-origin-{variant}-source.xlsx")),&original).unwrap();std::fs::write(dir.join(format!("conditional-origin-{variant}-saved.xlsx")),&bytes).unwrap();assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    }
    let opened=ibis_pptx::open(original).unwrap();let mut p=opened.presentation.clone();let id=slide(&p,"月次売上");let sh=std::sync::Arc::make_mut(p.slide_mut(id).unwrap().shapes.iter_mut().find_map(|s|match &mut s.kind {ibis_doc_model::ShapeKind::Sheet(sh)=>Some(sh),_=>None}).unwrap());let duplicate=sh.conditional.last().unwrap().clone();sh.conditional.push(duplicate);assert!(opened.source.save(&p,SaveOptions::default()).is_err());
}

#[test]
fn conditional_fill_edit_keeps_opaque_dxf_and_shared_original() {
    for color_node in ["bgColor", "fgColor"] {
        let source=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-conditional-opaque.xlsx")).unwrap();
        let mut pkg=ibis_ooxml::Package::open(source).unwrap();
        let xml=pkg.xml_mut("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let cf=xml.child(root,"conditionalFormatting").unwrap();let rule=xml.child(cf,"cfRule").unwrap();let old_id=xml.attr(rule,"dxfId").unwrap().parse::<usize>().unwrap();let copy=xml.deep_clone(cf);let pos=xml.children(root).iter().position(|&c| c==cf).unwrap();xml.insert(root,pos+1,copy);xml.set_attr(copy,"sqref","B1:B3");
        let xml=pkg.xml_mut("xl/styles.xml").unwrap();let root=xml.root_element().unwrap();let list=xml.child(root,"dxfs").unwrap();let dxf=xml.children_named(list,"dxf").nth(old_id).unwrap();let fill=xml.child(dxf,"fill").unwrap();let pat=xml.child(fill,"patternFill").unwrap();let bg=xml.child(pat,"bgColor").unwrap();if color_node=="fgColor" {xml.detach(bg);let fg=xml.append_element(pat,"fgColor");xml.set_attr(fg,"rgb","FFFFC7CE");}let color=xml.child(pat,color_node).unwrap();xml.set_attr(color,"probe:token","keep-color");xml.set_attr(fill,"probe:token","keep-fill");xml.set_attr(pat,"probe:token","keep-pattern");
        let original=pkg.save(Default::default()).unwrap();let opened=ibis_pptx::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());let id=slide(d.presentation(),"未知規則保持");let before=conditional(d.presentation(),id);assert_eq!(before.len(),2);let mut format=before[0].clone();format.rules[0].style.as_mut().unwrap().fill=Some(ibis_doc_model::Rgb(0x00B050));d.execute(vec![Command::Sheet(SheetCommand::UpdateConditional {slide:id,index:0,format})]).unwrap();
        let bytes=save(&opened,d.presentation(),&format!("conditional-dxf-fill-{color_node}-saved.xlsx"));let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");std::fs::write(dir.join(format!("conditional-dxf-fill-{color_node}-source.xlsx")),&original).unwrap();
        let pkg=ibis_ooxml::Package::open(bytes.clone()).unwrap();let xml=pkg.xml("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let ids=xml.children_named(root,"conditionalFormatting").map(|cf|xml.attr(xml.child(cf,"cfRule").unwrap(),"dxfId").unwrap().parse::<usize>().unwrap()).collect::<Vec<_>>();assert_ne!(ids[0],old_id);assert_eq!(ids[1],old_id);
        let xml=pkg.xml("xl/styles.xml").unwrap();let root=xml.root_element().unwrap();let list=xml.child(root,"dxfs").unwrap();let entries=xml.children_named(list,"dxf").collect::<Vec<_>>();let changed=entries[ids[0]];assert_eq!(xml.attr(changed,"probe:token"),Some("keep-dxf"));let font=xml.child(changed,"font").unwrap();assert_eq!(xml.attr(xml.child(font,"vertAlign").unwrap(),"val"),Some("superscript"));let fill=xml.child(changed,"fill").unwrap();let pat=xml.child(fill,"patternFill").unwrap();let color=xml.child(pat,color_node).unwrap();assert_eq!(xml.attr(fill,"probe:token"),Some("keep-fill"));assert_eq!(xml.attr(pat,"probe:token"),Some("keep-pattern"));assert_eq!(xml.attr(color,"probe:token"),Some("keep-color"));assert_eq!(xml.attr(color,"rgb"),Some("FF00B050"));let old=xml.child(xml.child(xml.child(entries[old_id],"fill").unwrap(),"patternFill").unwrap(),color_node).unwrap();assert_eq!(xml.attr(old,"rgb"),Some("FFFFC7CE"));
        assert_reads_back(&bytes,d.presentation());assert_eq!(conditional(&ibis_pptx::read(&bytes).unwrap(),id)[1],before[1]);assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    }
}

#[test]
fn conditional_fill_edit_refuses_unmodeled_fill_combinations() {
    for variant in ["gradient", "pattern", "two-colors"] {
        let source=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-conditional-opaque.xlsx")).unwrap();let mut pkg=ibis_ooxml::Package::open(source).unwrap();let xml=pkg.xml("xl/worksheets/sheet4.xml").unwrap();let root=xml.root_element().unwrap();let rule=xml.child(xml.child(root,"conditionalFormatting").unwrap(),"cfRule").unwrap();let id=xml.attr(rule,"dxfId").unwrap().parse::<usize>().unwrap();let xml=pkg.xml_mut("xl/styles.xml").unwrap();let root=xml.root_element().unwrap();let dxf=xml.children_named(xml.child(root,"dxfs").unwrap(),"dxf").nth(id).unwrap();let fill=xml.child(dxf,"fill").unwrap();let pat=xml.child(fill,"patternFill").unwrap();match variant {"gradient"=>{xml.detach(pat);let gradient=xml.append_element(fill,"gradientFill");xml.set_attr(gradient,"degree","45");},"pattern"=>xml.set_attr(pat,"patternType","darkGrid"),_=>{let fg=xml.append_element(pat,"fgColor");xml.set_attr(fg,"rgb","FF000000");}}
        let original=pkg.save(Default::default()).unwrap();let opened=ibis_pptx::open(original.clone()).unwrap();assert_eq!(opened.source.save(&opened.presentation,SaveOptions::default()).unwrap(),original);let mut d=Document::new(opened.presentation.clone());let s=slide(d.presentation(),"未知規則保持");let mut format=conditional(d.presentation(),s)[0].clone();format.rules[0].style.as_mut().unwrap().fill=Some(ibis_doc_model::Rgb(0x00B050));d.execute(vec![Command::Sheet(SheetCommand::UpdateConditional {slide:s,index:0,format})]).unwrap();assert!(opened.source.save(d.presentation(),SaveOptions::default()).is_err(),"{variant}");assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert!(opened.source.save(d.presentation(),SaveOptions::default()).is_err(),"{variant} redo");
    }
}

#[test]
fn conditional_fill_edit_refuses_missing_source_dxf_reference() {
    let source = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-conditional-opaque.xlsx")).unwrap();
    let mut pkg = ibis_ooxml::Package::open(source).unwrap();
    let xml = pkg.xml_mut("xl/worksheets/sheet4.xml").unwrap();
    let root = xml.root_element().unwrap();
    let rule = xml.child(xml.child(root, "conditionalFormatting").unwrap(), "cfRule").unwrap();
    xml.set_attr(rule, "dxfId", "-1");
    let original = pkg.save(Default::default()).unwrap();
    let opened = ibis_pptx::open(original.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "未知規則保持");
    let mut format = conditional(d.presentation(), s)[0].clone();
    format.rules[0].style.as_mut().unwrap().fill = Some(ibis_doc_model::Rgb(0x00B050));
    d.execute(vec![Command::Sheet(SheetCommand::UpdateConditional { slide: s, index: 0, format })]).unwrap();
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).is_err());
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
    assert!(d.redo());
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).is_err());
}

#[test]
fn conditional_fill_edit_inserts_fill_before_border_when_source_lacks_fill() {
    let source = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-conditional-opaque.xlsx")).unwrap();
    let mut pkg = ibis_ooxml::Package::open(source).unwrap();
    let xml = pkg.xml_mut("xl/styles.xml").unwrap();
    let root = xml.root_element().unwrap();
    let dxf = xml.children_named(xml.child(root, "dxfs").unwrap(), "dxf").next().unwrap();
    let fill = xml.child(dxf, "fill").unwrap();
    xml.detach(fill);
    let _border = xml.append_element(dxf, "border");
    let original = pkg.save(Default::default()).unwrap();
    let opened = ibis_pptx::open(original.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "未知規則保持");
    let mut format = conditional(d.presentation(), s)[0].clone();
    format.rules[0].style.as_mut().unwrap().fill = Some(ibis_doc_model::Rgb(0x00B050));
    d.execute(vec![Command::Sheet(SheetCommand::UpdateConditional { slide: s, index: 0, format })]).unwrap();
    let bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(bytes).unwrap();
    let xml = pkg.xml("xl/styles.xml").unwrap();
    let root = xml.root_element().unwrap();
    let dxf = xml.children_named(xml.child(root, "dxfs").unwrap(), "dxf").nth(1).unwrap();
    let names: Vec<_> = xml.elements(dxf).map(|n| xml.local_name(n).to_string()).collect();
    assert_eq!(names, vec!["font", "fill", "border"]);
}

#[test]
fn extended_calc_functions_save_and_reopen() {
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");

    set(&mut d, s, vec![
        (15, 1, CellInput::Formula("ENCODEURL(\"https://example.com/search?q=東京\")".into())),
        (16, 1, CellInput::Formula("NUMBERSTRING(12345, 1)".into())),
        (17, 1, CellInput::Formula("DATESTRING(DATE(2023, 1, 1))".into())),
        (18, 1, CellInput::Formula("RANDARRAY(2, 2)".into())),
        (20, 1, CellInput::Formula("SUM(B19#)".into())),
        (21, 1, CellInput::Formula("TRIMRANGE(B19:C20)".into())),
    ]);

    assert_eq!(value(d.presentation(), "月次売上", 15, 1), CellValue::Text("https%3A%2F%2Fexample.com%2Fsearch%3Fq%3D%E6%9D%B1%E4%BA%AC".into()));
    assert_eq!(value(d.presentation(), "月次売上", 16, 1), CellValue::Text("一万二千三百四十五".into()));
    assert_eq!(value(d.presentation(), "月次売上", 17, 1), CellValue::Text("令和5年1月1日".into()));
    assert!(matches!(value(d.presentation(), "月次売上", 18, 1), CellValue::Number(n) if (0.0..1.0).contains(&n)), "random number at 18,1");
    assert!(matches!(value(d.presentation(), "月次売上", 19, 2), CellValue::Number(n) if (0.0..1.0).contains(&n)), "random number at 19,2");
    assert!(matches!(value(d.presentation(), "月次売上", 20, 1), CellValue::Number(n) if (0.0..4.0).contains(&n)), "the sum of four random numbers");
    assert!(matches!(value(d.presentation(), "月次売上", 21, 1), CellValue::Number(n) if (0.0..1.0).contains(&n)), "random number at 21,1");

    let saved = save(&opened, d.presentation(), "extended-calc-functions.xlsx");
    assert_reads_back(&saved, d.presentation());

    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = package.xml("xl/worksheets/sheet1.xml").unwrap();
    let root = xml.root_element().unwrap();

    let check_formula = |addr: &str, expected: &str| {
        let c = xml.descendants(root).into_iter().find(|&n| xml.local_name(n) == "c" && xml.attr(n, "r") == Some(addr)).unwrap();
        let f = xml.child(c, "f").unwrap();
        assert_eq!(xml.text(f), expected, "formula at {addr}");
    };

    check_formula("B16", "_xlfn.ENCODEURL(\"https://example.com/search?q=東京\")");
    check_formula("B17", "NUMBERSTRING(12345, 1)");
    check_formula("B18", "DATESTRING(DATE(2023, 1, 1))");
    check_formula("B19", "_xlfn.RANDARRAY(2, 2)");
    check_formula("B21", "SUM(_xlfn.ANCHORARRAY(B19))");
    check_formula("B22", "_xlfn.TRIMRANGE(B19:C20)");

    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), data);
    assert!(d.redo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), saved);
}

#[test]
fn cell_comments_roundtrip_and_editing() {
    use ibis_doc_model::SheetComment;

    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");

    // Add comments to B2 and C3
    d.execute(vec![
        Command::Sheet(SheetCommand::SetComment {
            slide: s,
            row: 1,
            column: 1,
            comment: Some(SheetComment {
                row: 1,
                column: 1,
                author: "Reviewer".into(),
                text: "Please verify quarterly numbers".into(),
            }),
        }),
        Command::Sheet(SheetCommand::SetComment {
            slide: s,
            row: 2,
            column: 2,
            comment: Some(SheetComment {
                row: 2,
                column: 2,
                author: "Manager".into(),
                text: "Approved\nWith note".into(),
            }),
        }),
    ]).unwrap();

    let sheet = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sheet.comments.len(), 2);
    assert_eq!(sheet.comment_at(1, 1).unwrap().text, "Please verify quarterly numbers");
    assert_eq!(sheet.comment_at(2, 2).unwrap().text, "Approved\nWith note");

    let saved = save(&opened, d.presentation(), "comments-test.xlsx");
    assert_reads_back(&saved, d.presentation());

    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    assert!(pkg.contains("xl/comments1.xml"));
    let ct = pkg.content_type("xl/comments1.xml").unwrap();
    assert_eq!(ct.as_deref(), Some("application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml"));

    let rels = pkg.relationships("xl/worksheets/sheet1.xml").unwrap();
    let comment_rel = rels.iter().find(|r| r.kind == "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments").unwrap();
    assert_eq!(comment_rel.target_part("xl/worksheets/sheet1.xml").as_deref(), Some("xl/comments1.xml"));

    // Modify one comment and delete the other
    d.execute(vec![
        Command::Sheet(SheetCommand::SetComment {
            slide: s,
            row: 1,
            column: 1,
            comment: Some(SheetComment {
                row: 1,
                column: 1,
                author: "Reviewer".into(),
                text: "Updated comment text".into(),
            }),
        }),
        Command::Sheet(SheetCommand::SetComment {
            slide: s,
            row: 2,
            column: 2,
            comment: None,
        }),
    ]).unwrap();

    let sheet = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sheet.comments.len(), 1);
    assert_eq!(sheet.comment_at(1, 1).unwrap().text, "Updated comment text");
    assert!(sheet.comment_at(2, 2).is_none());

    let saved_modified = save(&opened, d.presentation(), "comments-modified.xlsx");
    assert_reads_back(&saved_modified, d.presentation());

    // Undo back to initial 2 comments
    assert!(d.undo());
    let sheet = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sheet.comments.len(), 2);
    assert_eq!(sheet.comment_at(1, 1).unwrap().text, "Please verify quarterly numbers");
    assert_eq!(sheet.comment_at(2, 2).unwrap().text, "Approved\nWith note");

    // Undo back to original without comments
    assert!(d.undo());
    let sheet = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sheet.comments.len(), 0);
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), data);

    // Redo back to 2 comments
    assert!(d.redo());
    let sheet = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sheet.comments.len(), 2);
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), saved);
}




/// Whether a cell's alignment names its vertical position is read (Excel
/// sets an explicitly bottom-aligned line on other measures), and saving
/// keeps it: a cell without one does not gain `vertical`, one with it keeps
/// it, and changing the position names it unless it is the default.
#[test]
fn whether_the_vertical_position_is_named_survives_saving() {
    let (_, opened) = open("xlsx-vertical.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "縦位置");
    let named = |p: &Presentation, row: u32| {
        let sheet = sheet_of_slide(p.slide(slide(p, "縦位置")).unwrap()).unwrap();
        sheet.styles[sheet.cell(row, 0).unwrap().style as usize].vertical_set
    };
    assert_eq!((named(d.presentation(), 0), named(d.presentation(), 1), named(d.presentation(), 2)), (false, true, true));
    // An unrelated change to each of the three, and the top one moved down.
    let all = CellRange { first_row: 0, first_column: 0, last_row: 2, last_column: 0 };
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![all], change: StyleChange::Bold(true) })]).unwrap();
    let third = CellRange { first_row: 2, first_column: 0, last_row: 2, last_column: 0 };
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![third], change: StyleChange::Vertical(ibis_doc_model::CellVerticalAlign::Bottom) })]).unwrap();
    assert_eq!((named(d.presentation(), 0), named(d.presentation(), 1), named(d.presentation(), 2)), (false, true, false));
    let saved = save(&opened, d.presentation(), "vertical.xlsx");
    let again = ibis_pptx::read(&saved).unwrap();
    assert_eq!((named(&again, 0), named(&again, 1), named(&again, 2)), (false, true, false));
    let styles = String::from_utf8(ibis_ooxml::Package::open(saved).unwrap().read("xl/styles.xml").unwrap()).unwrap();
    assert_eq!(styles.matches("vertical=\"bottom\"").count(), 2, "{styles}");
}

/// Structured references (`Table1[Qty]`, `[@Qty]`, `Table1[[#Headers],[Qty]]`)
/// name the table's cells: typing a new quantity recalculates the sum over
/// the column, the row's own `[@Qty]*2`, and the totals row's SUBTOTAL.
#[test]
fn structured_references_follow_the_table() {
    let (_, opened) = open("xlsx-table.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "表");
    set(&mut d, s, vec![(1, 1, CellInput::Value(CellValue::Number(10.0)))]);
    let p = d.presentation();
    assert_eq!(value(p, "表", 0, 4), CellValue::Number(21.0));
    assert_eq!(value(p, "表", 1, 2), CellValue::Number(20.0));
    assert_eq!(value(p, "表", 5, 1), CellValue::Number(21.0));
    assert_eq!(value(p, "表", 1, 4), CellValue::Text("Qty".into()));
    // A formula typed in: the table's name and column, case aside.
    set(&mut d, s, vec![(3, 4, CellInput::Formula("AVERAGE(table1[qty])".into()))]);
    assert_eq!(value(d.presentation(), "表", 3, 4), CellValue::Number(5.25));
    let saved = save(&opened, d.presentation(), "table.xlsx");
    assert_reads_back(&saved, d.presentation());
    // The row's own shorthand typed in is stored as the file writes it: `[@Qty]` does not open in Excel.
    set(&mut d, s, vec![(2, 2, CellInput::Formula("[@Qty]*3".into()))]);
    let saved = save(&opened, d.presentation(), "table-this-row.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet_xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet_xml.contains("Table1[[#This Row],[Qty]]*3"), "{sheet_xml}");
    assert!(!sheet_xml.contains("[@"), "{sheet_xml}");
}

/// Sparklines (`x14:sparklineGroups`) are read with their settings, the
/// data on the sheet itself made sheet-less, and survive saving an edit.
#[test]
fn sparklines_are_read_and_kept() {
    use ibis_doc_model::{Rgb, SparklineAxis, SparklineEmpty, SparklineKind};
    let (_, opened) = open("xlsx-sparkline.xlsx");
    let groups = |p: &Presentation| sheet_of_slide(p.slide(slide(p, "縦位置")).unwrap()).unwrap().sparklines.clone();
    let g = groups(&opened.presentation);
    assert_eq!(g.len(), 2);
    let line = &g[0];
    assert_eq!(line.kind, SparklineKind::Line);
    assert_eq!(line.line_weight, 150);
    assert!(line.markers && line.high && line.display_x_axis && !line.low);
    assert_eq!(line.empty, SparklineEmpty::Span);
    assert_eq!(line.min_axis, SparklineAxis::Custom("-20".into()));
    assert_eq!(line.max_axis, SparklineAxis::Individual);
    assert_eq!(line.series, Some(Rgb(0x1F497D)));
    assert_eq!(line.axis, Some(Rgb(0x808080)));
    assert_eq!(line.high_color, Some(Rgb(0x00B0F0)));
    assert_eq!(line.low_color, None);
    let (here, other) = (&line.sparklines[0], &line.sparklines[1]);
    assert_eq!((here.row, here.column, here.sheet.as_deref()), (0, 6, None));
    assert_eq!((here.data.first_column, here.data.last_column), (0, 4));
    assert_eq!((other.row, other.column, other.sheet.as_deref()), (0, 7, Some("Other")));
    assert_eq!((other.data.first_row, other.data.last_row), (0, 4));
    // Win/loss without colours: Excel draws nothing for it.
    assert_eq!(g[1].kind, SparklineKind::WinLoss);
    assert_eq!(g[1].series, None);

    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "縦位置");
    set(&mut d, s, vec![(0, 0, CellInput::Value(CellValue::Number(1.0)))]);
    let saved = save(&opened, d.presentation(), "sparkline.xlsx");
    assert_reads_back(&saved, d.presentation());
    assert_eq!(groups(&ibis_pptx::read(&saved).unwrap()), g);
}

/// Sparklines can be created, edited, cleared, and saved with roundtrip fidelity.
#[test]
fn sparklines_create_and_save_roundtrip() {
    use ibis_doc_model::{CellRange, Command, Rgb, SheetCommand, Sparkline, SparklineAxis, SparklineEmpty, SparklineGroup, SparklineKind};
    let (_, opened) = open("xlsx-sparkline.xlsx");
    let s = slide(&opened.presentation, "縦位置");
    let mut d = Document::new(opened.presentation.clone());

    // 1. Add a new Column sparkline group
    let new_group = SparklineGroup {
        kind: SparklineKind::Column,
        series: Some(Rgb(0x00B050)),
        negative: Some(Rgb(0xC00000)),
        axis: Some(Rgb(0x000000)),
        markers_color: None,
        first_color: None,
        last_color: None,
        high_color: Some(Rgb(0x0070C0)),
        low_color: Some(Rgb(0xFFC000)),
        line_weight: 125,
        markers: false,
        high: true,
        low: true,
        first: false,
        last: false,
        negative_points: true,
        display_x_axis: true,
        empty: SparklineEmpty::Zero,
        min_axis: SparklineAxis::Individual,
        max_axis: SparklineAxis::Custom("50".to_string()),
        right_to_left: false,
        display_hidden: false,
        date_axis: false,
        date_range: None,
        sparklines: vec![
            Sparkline {
                row: 2,
                column: 6,
                sheet: None,
                data: CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 4 },
            },
        ],
    };

    d.execute(vec![Command::Sheet(SheetCommand::AddSparklineGroup {
        slide: s,
        group: new_group.clone(),
    })]).unwrap();

    let groups = |p: &Presentation| sheet_of_slide(p.slide(s).unwrap()).unwrap().sparklines.clone();
    assert_eq!(groups(d.presentation()).len(), 3);

    let saved = save(&opened, d.presentation(), "sparkline_add.xlsx");
    assert_reads_back(&saved, d.presentation());

    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_groups = groups(&reread);
    assert_eq!(reread_groups.len(), 3);
    assert_eq!(reread_groups[2].kind, SparklineKind::Column);
    assert_eq!(reread_groups[2].series, Some(Rgb(0x00B050)));
    assert_eq!(reread_groups[2].negative, Some(Rgb(0xC00000)));
    assert_eq!(reread_groups[2].high_color, Some(Rgb(0x0070C0)));
    assert_eq!(reread_groups[2].low_color, Some(Rgb(0xFFC000)));
    assert_eq!(reread_groups[2].line_weight, 125);
    assert_eq!(reread_groups[2].high, true);
    assert_eq!(reread_groups[2].low, true);
    assert_eq!(reread_groups[2].negative_points, true);
    assert_eq!(reread_groups[2].display_x_axis, true);
    assert_eq!(reread_groups[2].empty, SparklineEmpty::Zero);
    assert_eq!(reread_groups[2].max_axis, SparklineAxis::Custom("50".to_string()));
    assert_eq!(reread_groups[2].sparklines.len(), 1);
    assert_eq!(reread_groups[2].sparklines[0].row, 2);
    assert_eq!(reread_groups[2].sparklines[0].column, 6);

    // 2. Clear sparkline at (0, 6) in the first group
    d.execute(vec![Command::Sheet(SheetCommand::ClearSparklines {
        slide: s,
        ranges: vec![CellRange { first_row: 0, last_row: 0, first_column: 6, last_column: 6 }],
        whole_group: false,
    })]).unwrap();
    assert_eq!(groups(d.presentation())[0].sparklines.len(), 1);
    let saved_clear = save(&opened, d.presentation(), "sparkline_clear.xlsx");
    let reread_clear = ibis_pptx::read(&saved_clear).unwrap();
    assert_eq!(groups(&reread_clear)[0].sparklines.len(), 1);

    // 3. Clear all remaining sparklines
    d.execute(vec![
        Command::Sheet(SheetCommand::DeleteSparklineGroup { slide: s, index: 2 }),
        Command::Sheet(SheetCommand::DeleteSparklineGroup { slide: s, index: 1 }),
        Command::Sheet(SheetCommand::DeleteSparklineGroup { slide: s, index: 0 }),
    ]).unwrap();
    assert_eq!(groups(d.presentation()).len(), 0);
    let saved_all_cleared = save(&opened, d.presentation(), "sparkline_cleared.xlsx");
    let reread_all_cleared = ibis_pptx::read(&saved_all_cleared).unwrap();
    assert!(groups(&reread_all_cleared).is_empty());

    // 4. Test Undo / Redo
    assert!(d.undo());
    assert_eq!(groups(d.presentation()).len(), 3);
    assert!(d.undo()); // undo clear (0, 6)
    assert_eq!(groups(d.presentation())[0].sparklines.len(), 2);
    assert!(d.undo()); // undo add group
    assert_eq!(groups(d.presentation()).len(), 2);
    assert!(d.redo()); // redo add group
    assert_eq!(groups(d.presentation()).len(), 3);
}

/// Sparkline styles, colors, and display_hidden setting can be edited, undone/redone,
/// and survive saving with full roundtrip fidelity (XLSX-SPARK-008, XLSX-SPARK-022).
#[test]
fn sparklines_styles_and_display_hidden_roundtrip() {
    use ibis_doc_model::{Command, Rgb, SheetCommand, SparklineEmpty};
    let (_, opened) = open("xlsx-sparkline.xlsx");
    let s = slide(&opened.presentation, "縦位置");
    let mut d = Document::new(opened.presentation.clone());
    let groups = |p: &Presentation| sheet_of_slide(p.slide(s).unwrap()).unwrap().sparklines.clone();

    let initial = groups(d.presentation());
    assert!(!initial[0].display_hidden);

    // 1. Edit group 0: change series color, markers color, weight, and display_hidden = true
    let mut edited = initial[0].clone();
    edited.series = Some(Rgb(0xC0504D)); // Accent 2 style
    edited.negative = Some(Rgb(0x9BBB59));
    edited.markers_color = Some(Rgb(0x4F81BD));
    edited.high_color = Some(Rgb(0x8064A2));
    edited.low_color = Some(Rgb(0x9BBB59));
    edited.line_weight = 225;
    edited.display_hidden = true;
    edited.empty = SparklineEmpty::Zero;

    d.execute(vec![Command::Sheet(SheetCommand::EditSparklineGroup { slide: s, index: 0, group: edited.clone() })]).unwrap();
    assert!(groups(d.presentation())[0].display_hidden);
    assert_eq!(groups(d.presentation())[0].series, Some(Rgb(0xC0504D)));
    assert_eq!(groups(d.presentation())[0].line_weight, 225);
    assert_eq!(groups(d.presentation())[0].empty, SparklineEmpty::Zero);

    // 2. Save and reread
    let saved = save(&opened, d.presentation(), "sparkline_style_hidden.xlsx");
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_g = groups(&reread);
    assert_eq!(reread_g.len(), 2);
    assert!(reread_g[0].display_hidden);
    assert_eq!(reread_g[0].series, Some(Rgb(0xC0504D)));
    assert_eq!(reread_g[0].negative, Some(Rgb(0x9BBB59)));
    assert_eq!(reread_g[0].markers_color, Some(Rgb(0x4F81BD)));
    assert_eq!(reread_g[0].high_color, Some(Rgb(0x8064A2)));
    assert_eq!(reread_g[0].low_color, Some(Rgb(0x9BBB59)));
    assert_eq!(reread_g[0].line_weight, 225);
    assert_eq!(reread_g[0].empty, SparklineEmpty::Zero);

    // 3. Test Undo / Redo
    assert!(d.undo());
    assert!(!groups(d.presentation())[0].display_hidden);
    assert_eq!(groups(d.presentation())[0].series, initial[0].series);
    assert_eq!(groups(d.presentation())[0].line_weight, initial[0].line_weight);

    assert!(d.redo());
    assert!(groups(d.presentation())[0].display_hidden);
    assert_eq!(groups(d.presentation())[0].series, Some(Rgb(0xC0504D)));
    assert_eq!(groups(d.presentation())[0].line_weight, 225);
}

/// Sparkline date axis and date range formula can be edited, undone/redone,
/// and survive saving with full roundtrip fidelity (XLSX-SPARK-015).
#[test]
fn sparklines_date_axis_roundtrip() {
    use ibis_doc_model::{Command, SheetCommand};
    let (_, opened) = open("xlsx-sparkline.xlsx");
    let s = slide(&opened.presentation, "縦位置");
    let mut d = Document::new(opened.presentation.clone());
    let groups = |p: &Presentation| sheet_of_slide(p.slide(s).unwrap()).unwrap().sparklines.clone();

    let initial = groups(d.presentation());
    assert!(!initial[0].date_axis);
    assert_eq!(initial[0].date_range, None);

    // 1. Edit group 0: set date_axis = true and date_range
    let mut edited = initial[0].clone();
    edited.date_axis = true;
    edited.date_range = Some("Sheet1!$A$1:$A$10".to_string());

    d.execute(vec![Command::Sheet(SheetCommand::EditSparklineGroup { slide: s, index: 0, group: edited.clone() })]).unwrap();
    assert!(groups(d.presentation())[0].date_axis);
    assert_eq!(groups(d.presentation())[0].date_range, Some("Sheet1!$A$1:$A$10".to_string()));

    // 2. Save and reread
    let saved = save(&opened, d.presentation(), "sparkline_date_axis.xlsx");
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_g = groups(&reread);
    assert_eq!(reread_g.len(), 2);
    assert!(reread_g[0].date_axis);
    assert_eq!(reread_g[0].date_range, Some("Sheet1!$A$1:$A$10".to_string()));

    // 2b. Unqualified date_range is automatically qualified with the current sheet name on save (real Excel requirement)
    let mut edited_unqual = initial[0].clone();
    edited_unqual.date_axis = true;
    edited_unqual.date_range = Some("A1:C1".to_string());
    d.execute(vec![Command::Sheet(SheetCommand::EditSparklineGroup { slide: s, index: 0, group: edited_unqual })]).unwrap();
    let saved_unqual = save(&opened, d.presentation(), "sparkline_date_axis_unqual.xlsx");
    let reread_unqual = ibis_pptx::read(&saved_unqual).unwrap();
    let reread_unqual_g = groups(&reread_unqual);
    assert!(reread_unqual_g[0].date_axis);
    assert_eq!(reread_unqual_g[0].date_range, Some("縦位置!A1:C1".to_string()));
    assert!(reread_unqual_g[0].date_range.as_deref().unwrap().contains('!'));
    assert!(d.undo());

    // 3. Test Undo / Redo
    assert!(d.undo());
    assert!(!groups(d.presentation())[0].date_axis);
    assert_eq!(groups(d.presentation())[0].date_range, None);

    assert!(d.redo());
    assert!(groups(d.presentation())[0].date_axis);
    assert_eq!(groups(d.presentation())[0].date_range, Some("Sheet1!$A$1:$A$10".to_string()));
}

/// Each kind of column filter is saved as Excel writes it and read back
/// the same: comparisons with wildcards, top 10, a date period, dates by
/// month, blanks, and colours (a `dxf` added to the styles).
#[test]
fn every_kind_of_filter_is_saved() {
    use ibis_doc_model::{AutoFilter, ColumnFilter, DateGroup};
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    let mut edits = Vec::new();
    for column in 0..8 {
        edits.push(CellEdit { row: 0, column, input: CellInput::Value(CellValue::Text(format!("列{column}"))) });
        for row in 1..5 {
            edits.push(CellEdit { row, column, input: CellInput::Value(CellValue::Number(46000.0 + row as f64 * 10.0)) });
        }
    }
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "絞り込み".into(), slide: s, shape }), Command::Sheet(SheetCommand::SetCells { slide: s, edits })]).unwrap();
    let columns = vec![
        (0, ColumnFilter::Custom { and: true, conditions: vec![("greaterThanOrEqual".into(), "46010".into()), ("lessThan".into(), "46030".into())] }),
        (1, ColumnFilter::Custom { and: false, conditions: vec![("equal".into(), "*店*".into()), ("notEqual".into(), "4602?".into())] }),
        (2, ColumnFilter::Top { top: false, percent: true, value: "50".into() }),
        (3, ColumnFilter::Period("thisMonth".into())),
        (4, ColumnFilter::Dates { values: vec!["".into()], groups: vec![DateGroup { grouping: "month".into(), year: 2025, month: 12, day: 0, hour: 0, minute: 0, second: 0 }] }),
        (5, ColumnFilter::Color { cell: true, color: Some(ibis_doc_model::Rgb(0xFFC000)) }),
        (6, ColumnFilter::Color { cell: false, color: None }),
        (7, ColumnFilter::Icon { icon_set: "3TrafficLights1".into(), icon_id: 1 }),
    ];
    let filter = AutoFilter { range: CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 7 }, columns };
    d.execute(vec![Command::Sheet(SheetCommand::SetAutoFilter { slide: s, filter: Some(filter.clone()) })]).unwrap();
    let bytes = save(&opened, d.presentation(), "every-filter.xlsx");
    let p = ibis_pptx::read(&bytes).unwrap();
    let again = sheet_of_slide(p.slide(slide(&p, "絞り込み")).unwrap()).unwrap();
    assert_eq!(again.auto_filter.as_ref(), Some(&filter));
    // Saved again unchanged but for a cell, the colour filters keep their dxfs.
    let mut d2 = Document::new(p.clone());
    set(&mut d2, slide(&p, "絞り込み"), vec![(6, 0, CellInput::Value(CellValue::Number(1.0)))]);
    let reopened = ibis_pptx::open(bytes.clone()).unwrap();
    let twice = reopened.source.save(d2.presentation(), SaveOptions::default()).unwrap();
    let count = |b: &[u8]| {
        let xml = String::from_utf8(ibis_ooxml::Package::open(b.to_vec()).unwrap().read("xl/styles.xml").unwrap()).unwrap();
        xml.matches("<dxf>").count() + xml.matches("<dxf ").count()
    };
    assert_eq!(count(&twice), count(&bytes));
}

/// Threaded comments are read with their people and replies; rows inserted
/// or deleted move them in the file too (a deleted cell's thread goes with
/// its replies).
#[test]
fn threaded_comments_are_read_and_follow_the_rows() {
    use ibis_doc_model::sheet_command::Axis;
    let (_, opened) = open("xlsx-threads.xlsx");
    let threads = |p: &Presentation| sheet_of_slide(p.slide(slide(p, "縦位置")).unwrap()).unwrap().threads.clone();
    let t = threads(&opened.presentation);
    assert_eq!(t.len(), 2);
    assert_eq!((t[0].row, t[0].column, t[0].done), (1, 1, false));
    let posts: Vec<(&str, &str)> = t[0].posts.iter().map(|p| (p.author.as_str(), p.text.as_str())).collect();
    assert_eq!(posts, vec![("山田 花子", "この値で合っていますか？"), ("鈴木 一郎", "はい、確認済みです。")]);
    assert_eq!(t[0].posts[0].date, "2026-10-07T09:30:00.00");
    assert_eq!((t[1].row, t[1].done), (3, true));
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "縦位置");
    d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Rows, at: 0, count: 1 })]).unwrap();
    d.execute(vec![Command::Sheet(SheetCommand::Delete { slide: s, axis: Axis::Rows, at: 4, count: 1 })]).unwrap();
    let moved = threads(d.presentation());
    assert_eq!(moved.iter().map(|t| t.row).collect::<Vec<_>>(), vec![2]);
    let saved = save(&opened, d.presentation(), "threads.xlsx");
    let again = threads(&ibis_pptx::read(&saved).unwrap());
    assert_eq!(again.len(), 1);
    assert_eq!((again[0].row, again[0].column, again[0].posts.len()), (2, 1, 2));
    let xml = String::from_utf8(ibis_ooxml::Package::open(saved).unwrap().read("xl/threadedComments/threadedComment1.xml").unwrap()).unwrap();
    assert_eq!(xml.matches("<threadedComment ").count(), 2, "{xml}");
}

/// Threaded comments can be created, replied to, resolved, and deleted;
/// changes survive saving and reopening, and undo/redo works cleanly.
#[test]
fn threaded_comments_create_reply_resolve_delete_roundtrip() {
    use ibis_doc_model::{ThreadPost, ThreadedComment};
    let (_, opened) = open("xlsx-threads.xlsx");
    let threads = |p: &Presentation| sheet_of_slide(p.slide(slide(p, "縦位置")).unwrap()).unwrap().threads.clone();
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "縦位置");

    // 1. Create a new thread at C3 (row: 2, column: 2)
    let new_thread = ThreadedComment {
        id: "{CCCCCCCC-0000-0000-0000-000000000001}".to_string(),
        row: 2,
        column: 2,
        posts: vec![ThreadPost {
            author: "佐藤 次郎".to_string(),
            date: "2026-10-09T01:00:00.00".to_string(),
            text: "新規の質問です。".to_string(),
        }],
        done: false,
    };
    d.execute(vec![Command::Sheet(SheetCommand::SetThreadedComment {
        slide: s,
        row: 2,
        column: 2,
        thread: Some(new_thread),
    })]).unwrap();

    // 2. Add reply to existing thread at B2 (row: 1, column: 1)
    let mut b2_thread = threads(d.presentation()).into_iter().find(|t| (t.row, t.column) == (1, 1)).unwrap();
    b2_thread.posts.push(ThreadPost {
        author: "佐藤 次郎".to_string(),
        date: "2026-10-09T02:00:00.00".to_string(),
        text: "追記の確認です。".to_string(),
    });
    d.execute(vec![Command::Sheet(SheetCommand::SetThreadedComment {
        slide: s,
        row: 1,
        column: 1,
        thread: Some(b2_thread),
    })]).unwrap();

    // 3. Resolve C3 thread
    let mut c3_thread = threads(d.presentation()).into_iter().find(|t| (t.row, t.column) == (2, 2)).unwrap();
    c3_thread.done = true;
    d.execute(vec![Command::Sheet(SheetCommand::SetThreadedComment {
        slide: s,
        row: 2,
        column: 2,
        thread: Some(c3_thread),
    })]).unwrap();

    // Save and verify roundtrip
    let saved = save(&opened, d.presentation(), "threads_edited.xlsx");
    let reloaded = ibis_pptx::read(&saved).unwrap();
    let r_threads = threads(&reloaded);
    assert_eq!(r_threads.len(), 3);

    let r_b2 = r_threads.iter().find(|t| (t.row, t.column) == (1, 1)).unwrap();
    assert_eq!(r_b2.posts.len(), 3);
    assert_eq!(r_b2.posts[2].author, "佐藤 次郎");
    assert_eq!(r_b2.posts[2].text, "追記の確認です。");
    assert!(!r_b2.done);

    let r_c3 = r_threads.iter().find(|t| (t.row, t.column) == (2, 2)).unwrap();
    assert_eq!(r_c3.posts.len(), 1);
    assert_eq!(r_c3.posts[0].text, "新規の質問です。");
    assert!(r_c3.done);

    // 4. Delete the C3 thread
    d.execute(vec![Command::Sheet(SheetCommand::SetThreadedComment {
        slide: s,
        row: 2,
        column: 2,
        thread: None,
    })]).unwrap();
    assert_eq!(threads(d.presentation()).len(), 2);

    let saved_del = save(&opened, d.presentation(), "threads_deleted.xlsx");
    let reloaded_del = ibis_pptx::read(&saved_del).unwrap();
    assert_eq!(threads(&reloaded_del).len(), 2);

    // 5. Undo delete
    assert!(d.undo());
    assert_eq!(threads(d.presentation()).len(), 3);
    let undone_c3 = threads(d.presentation()).into_iter().find(|t| (t.row, t.column) == (2, 2)).unwrap();
    assert_eq!(undone_c3.posts[0].text, "新規の質問です。");
    assert!(undone_c3.done);

    // 6. Redo delete
    assert!(d.redo());
    assert_eq!(threads(d.presentation()).len(), 2);
}

/// A table grown by a column is saved with its new range and column; a
/// new table gets its own part, and both read back.
#[test]
fn tables_made_and_grown_are_saved() {
    use ibis_doc_model::sheet_command::PutCell;
    use ibis_doc_model::{Rgb, SheetTable, TableStyleInfo};
    let (_, opened) = open("xlsx-table.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "表");
    let mut grown = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap().tables[0].clone();
    grown.range.last_column += 1;
    grown.columns.push("列4".into());
    if !grown.totals.is_empty() {
        grown.totals.push(Default::default());
    }
    if let Some(f) = grown.filter.as_mut() {
        f.columns.push((grown.range.first_column, ibis_doc_model::ColumnFilter::Color { cell: true, color: Some(Rgb(0xFFCC00)) }));
    }
    let mut grown_cells = vec![PutCell { row: 0, column: grown.range.last_column, input: CellInput::Value(CellValue::Text("列4".into())), style: None }];
    for r in 1..=4 {
        grown_cells.push(PutCell {
            row: r,
            column: grown.range.last_column,
            input: CellInput::Formula(format!("B{}*2", r + 1)),
            style: None,
        });
    }
    d.execute(vec![Command::Sheet(SheetCommand::SetTable { slide: s, index: 0, table: Some(grown.clone()), cells: grown_cells })]).unwrap();
    let made = SheetTable {
        name: "テーブル2".into(),
        range: CellRange { first_row: 9, last_row: 11, first_column: 0, last_column: 1 },
        header_rows: 1,
        totals_rows: 0,
        columns: vec!["甲".into(), "乙".into()],
        style: Some(TableStyleInfo { name: "TableStyleLight9".into(), row_stripes: true, ..Default::default() }),
        filter: Some(ibis_doc_model::AutoFilter {
            range: CellRange { first_row: 9, last_row: 11, first_column: 0, last_column: 1 },
            columns: vec![(0, ibis_doc_model::ColumnFilter::Color { cell: false, color: Some(Rgb(0x0088FF)) })],
        }),
        totals: Vec::new(),
    };
    let mut cells: Vec<PutCell> = ["甲", "乙"].iter().enumerate().map(|(i, t)| PutCell { row: 9, column: i as u32, input: CellInput::Value(CellValue::Text((*t).into())), style: None }).collect();
    for r in 10..=11 {
        cells.push(PutCell { row: r, column: 1, input: CellInput::Formula(format!("A{}+1", r + 1)), style: None });
    }
    d.execute(vec![Command::Sheet(SheetCommand::SetTable { slide: s, index: 1, table: Some(made.clone()), cells })]).unwrap();
    // (The cells' formats are left as they were here: the app lays the
    // style on them, `table_edit.rs`.)
    let saved = save(&opened, d.presentation(), "tables.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let t1_xml = String::from_utf8(pkg.read("xl/tables/table1.xml").unwrap()).unwrap();
    assert!(t1_xml.contains("<calculatedColumnFormula>B2*2</calculatedColumnFormula>"), "{t1_xml}");
    let t2_xml = String::from_utf8(pkg.read("xl/tables/table2.xml").unwrap()).unwrap();
    assert!(t2_xml.contains("<calculatedColumnFormula>A11+1</calculatedColumnFormula>"), "{t2_xml}");
    let p = ibis_pptx::read(&saved).unwrap();
    let tables = sheet_of_slide(p.slide(slide(&p, "表")).unwrap()).unwrap().tables.clone();
    // As the model has them (the grown table's filter covering its new column).
    let model = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap().tables.clone();
    assert_eq!(model[0].filter.as_ref().map(|f| f.range.last_column), Some(grown.range.last_column));
    assert_eq!(tables, vec![SheetTable { style: tables[0].style.clone(), ..model[0].clone() }, made]);
    // Undone, the file is as it was.
    assert!(d.undo() && d.undo());
    assert_eq!(sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap().tables.len(), 1);
}

/// 範囲に変換: the table goes, its formulas' structured references become
/// cell references (the values kept), and the saved file has no table.
#[test]
fn a_table_converted_to_a_range() {
    let (_, opened) = open("xlsx-table.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "表");
    let before: Vec<CellValue> = [(0, 4), (1, 2), (5, 1)].iter().map(|&(r, c)| value(d.presentation(), "表", r, c)).collect();
    d.execute(vec![Command::Sheet(SheetCommand::SetTable { slide: s, index: 0, table: None, cells: Vec::new() })]).unwrap();
    let p = d.presentation();
    assert_eq!(formula(p, "表", 0, 4).as_deref(), Some("SUM($B$2:$B$5)"));
    assert_eq!(formula(p, "表", 1, 2).as_deref(), Some("$B2*2"));
    let after: Vec<CellValue> = [(0, 4), (1, 2), (5, 1)].iter().map(|&(r, c)| value(p, "表", r, c)).collect();
    assert_eq!(after, before);
    let saved = save(&opened, p, "table-to-range.xlsx");
    assert_reads_back(&saved, p);
    let again = ibis_pptx::read(&saved).unwrap();
    assert!(sheet_of_slide(again.slide(slide(&again, "表")).unwrap()).unwrap().tables.is_empty());
    let package = ibis_ooxml::Package::open(saved).unwrap();
    assert!(!package.names().any(|n| n.starts_with("xl/tables/")));
}

#[cfg(target_os = "macos")]
#[test]
fn kanji_text_sorts_by_its_reading() {
    let (_, opened) = open("xlsx-sales.xlsx"); let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "読み順".into(), slide: s, shape })]).unwrap();
    let names = ["地域", "北海道", "九州", "東北", "関東", "関西"];
    set(&mut d, s, names.iter().enumerate().map(|(r, n)| (r as u32, 0, CellInput::Value(CellValue::Text((*n).into())))).collect());
    let range = CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 0 };
    d.execute(vec![Command::Sheet(SheetCommand::Sort { slide: s, range, keys: vec![(0, true)], header: true })]).unwrap();
    for (r, n) in ["地域", "関西", "関東", "九州", "東北", "北海道"].iter().enumerate() {
        assert_eq!(value(d.presentation(), "読み順", r as u32, 0), CellValue::Text((*n).into()));
    }
    d.execute(vec![Command::Sheet(SheetCommand::Sort { slide: s, range, keys: vec![(0, false)], header: true })]).unwrap();
    assert_eq!(value(d.presentation(), "読み順", 1, 0), CellValue::Text("北海道".into()));
}

#[test]
fn the_readings_of_shared_strings_are_read_and_sorting_uses_them() {
    let (_, stock) = open("xlsx-sales.xlsx");
    let bytes = stock.source.save(&stock.presentation, SaveOptions::default()).unwrap();
    let mut package = ibis_ooxml::Package::open(bytes).unwrap();
    let xml = package.xml_mut("xl/sharedStrings.xml").unwrap(); let root = xml.root_element().unwrap();
    let si = xml.child(root, "si").unwrap(); let rph = xml.append_element(si, "rPh"); xml.set_attr(rph, "sb", "0"); xml.set_attr(rph, "eb", "1");
    let t = xml.append_element(rph, "t"); xml.set_text(t, "ゼンゴ");
    let opened = ibis_pptx::open(package.save(Default::default()).unwrap()).unwrap();
    let sheet = opened.presentation.slides.iter().filter_map(|s| sheet_of_slide(s)).next().unwrap();
    let with: Vec<_> = sheet.cells.iter().filter(|c| c.phonetic.is_some()).collect();
    assert!(!with.is_empty()); assert!(with.iter().all(|c| c.phonetic.as_deref() == Some("ゼンゴ")));
    assert_eq!(ibis_calc::text_order::compare_for_sort_with("A", Some("ん"), "B", Some("あ")), std::cmp::Ordering::Greater);
}

#[test]
fn theme_colours_of_text_and_fill_are_saved_as_theme_and_tint() {
    use ibis_doc_model::ThemeColor;
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let range = CellRange { first_row: 3, first_column: 1, last_row: 3, last_column: 1 };
    let colors = ibis_pptx::workbook_theme_colors(Some(&opened.source));
    let (text, fill) = (ThemeColor { index: 5, tint: -250_000 }, ThemeColor { index: 4, tint: 800_000 });
    for change in [StyleChange::ColorTheme(ibis_pptx::tinted(colors[5], text.tint), text), StyleChange::FillTheme(ibis_pptx::tinted(colors[4], fill.tint), fill)] {
        d.execute(vec![Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![range], change })]).unwrap();
    }
    let bytes = save(&opened, d.presentation(), "theme-colours.xlsx");
    let styles = String::from_utf8(ibis_ooxml::Package::open(bytes.clone()).unwrap().read("xl/styles.xml").unwrap()).unwrap();
    assert!(styles.contains("theme=\"5\" tint=\"-0.249977111117893\""), "text colour as theme and tint, as Excel writes it");
    assert!(styles.contains("<fgColor theme=\"4\" tint=\"0.7999816888943144\""), "fill as theme and tint");
    let read = ibis_pptx::read(&bytes).unwrap();
    let c = cell(&read, "月次売上", 3, 1);
    let style = &sheet_of_slide(read.slide(slide(&read, "月次売上")).unwrap()).unwrap().styles[c.style as usize];
    let near = |a: Option<ThemeColor>, b: ThemeColor| a.is_some_and(|a| a.index == b.index && (a.tint - b.tint).abs() < 100);
    assert!(near(style.color_theme, text) && near(style.fill_theme, fill), "{:?}", (style.color_theme, style.fill_theme));
    // A plain colour afterwards drops the theme.
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![range], change: StyleChange::Color(ibis_doc_model::Rgb(0xFF0000)) })]).unwrap();
    let bytes = save(&opened, d.presentation(), "theme-colours-plain.xlsx");
    let read = ibis_pptx::read(&bytes).unwrap();
    let c = cell(&read, "月次売上", 3, 1);
    let style = &sheet_of_slide(read.slide(slide(&read, "月次売上")).unwrap()).unwrap().styles[c.style as usize];
    assert_eq!((style.color_theme, style.color), (None, ibis_doc_model::Rgb(0xFF0000)));
    assert!(near(style.fill_theme, fill));
}

#[test]
fn a_sheet_chart_switches_rows_and_columns_and_takes_a_new_range() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let refs = opened.source.sheet_chart_refs(shape, &chart).unwrap();
    assert_eq!(refs.len(), 3);
    let (sheet, range, by_columns) = ibis_pptx::refs_range(&refs).unwrap();
    assert_eq!((sheet.as_str(), range, by_columns), ("月次売上", CellRange { first_row: 2, first_column: 0, last_row: 7, last_column: 3 }, true));
    // 行/列の切り替え.
    let rows = ibis_pptx::refs_for_range(d.presentation(), &sheet, range, false).unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].values.as_deref(), Some("月次売上!$B$4:$D$4"));
    let target = ibis_pptx::sheet_chart_from_data(&chart, d.presentation(), &rows);
    assert_eq!(target.groups.iter().map(|g| g.series.len()).sum::<usize>(), 5);
    opened.source.set_chart_data(shape, target.clone(), ibis_pptx::ChartData { workbook: Vec::new(), series: rows.clone() });
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(target.clone()))) }]).unwrap();
    assert_eq!(opened.source.sheet_chart_refs(shape, &target).unwrap(), rows);
    let bytes = save(&opened, d.presentation(), "chart-switched.xlsx");
    let read = ibis_pptx::open(bytes).unwrap();
    let (rs, rc) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read.source.sheet_chart_refs(rs, &rc).unwrap(), rows, "the references are saved");
    let series: Vec<_> = rc.groups.iter().flat_map(|g| g.series.iter()).collect();
    assert_eq!(series.len(), 5);
    assert_eq!(series[0].values.len(), 3);
    assert_eq!(series[0].categories.len(), 3);
    // The same data in columns again, from a smaller range.
    let small = CellRange { first_row: 2, first_column: 0, last_row: 5, last_column: 2 };
    let columns = ibis_pptx::refs_for_range(d.presentation(), &sheet, small, true).unwrap();
    assert_eq!(columns.len(), 2);
    let numeric=ibis_pptx::refs_for_range(d.presentation(), &sheet, CellRange { first_row: 3, first_column: 1, last_row: 5, last_column: 3 }, true).unwrap();
    assert!(numeric.iter().all(|s|s.name.is_none()),"numeric first cells are values, not headings");
    assert_eq!(numeric[0].values.as_deref(),Some("月次売上!$B$4:$B$6"));
}

#[test]
fn chart_axis_titles_save_and_reopen() {
    use ibis_doc_model::chart::{AxisKind, ChartText, ChartTitle};
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let cat_idx = chart.axes.iter().position(|a| a.kind == AxisKind::Category && !a.deleted).unwrap();
    let val_idx = chart.axes.iter().position(|a| a.kind == AxisKind::Value && !a.deleted).unwrap();
    assert!(chart.axes[cat_idx].title.is_none());
    assert!(chart.axes[val_idx].title.is_none());

    // 1. 項目軸・値軸にタイトルを設定して保存・再読込
    let mut modified = chart.clone();
    modified.axes[cat_idx].title = Some(ChartTitle {
        text: "期間（月）".into(),
        style: ChartText { size: 1000, ..modified.axes[cat_idx].text.clone() },
        at: None,
    });
    modified.axes[val_idx].title = Some(ChartTitle {
        text: "売上高（円）".into(),
        style: ChartText { size: 1000, ..modified.axes[val_idx].text.clone() },
        at: None,
    });
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-axis-titles.xlsx");

    let read = ibis_pptx::open(bytes).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read_chart.axes[cat_idx].title.as_ref().map(|t| t.text.as_str()), Some("期間（月）"));
    assert_eq!(read_chart.axes[val_idx].title.as_ref().map(|t| t.text.as_str()), Some("売上高（円）"));

    // 2. 軸タイトルのテキストを変更して保存・再読込
    let mut edited = read_chart.clone();
    edited.axes[cat_idx].title.as_mut().unwrap().text = "対象月".into();
    edited.axes[val_idx].title.as_mut().unwrap().text = "金額（千円）".into();
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-axis-titles-edited.xlsx");

    let read2 = ibis_pptx::open(bytes2).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read2_chart.axes[cat_idx].title.as_ref().map(|t| t.text.as_str()), Some("対象月"));
    assert_eq!(read2_chart.axes[val_idx].title.as_ref().map(|t| t.text.as_str()), Some("金額（千円）"));

    // 3. 軸タイトルを削除（None）して保存・再読込（XML から <c:title> が消えていること）
    let mut cleared = read2_chart.clone();
    cleared.axes[cat_idx].title = None;
    cleared.axes[val_idx].title = None;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(cleared.clone()))) }]).unwrap();
    let bytes3 = save(&read2, d.presentation(), "chart-axis-titles-cleared.xlsx");

    let read3 = ibis_pptx::open(bytes3).unwrap();
    let (_, read3_chart) = read3.presentation.slide(slide(&read3.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(read3_chart.axes[cat_idx].title.is_none());
    assert!(read3_chart.axes[val_idx].title.is_none());

    // 4. Undo してタイトルが復元されること
    assert!(d.undo());
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart.axes[cat_idx].title.as_ref().map(|t| t.text.as_str()), Some("対象月"));
    assert_eq!(undo_chart.axes[val_idx].title.as_ref().map(|t| t.text.as_str()), Some("金額（千円）"));
}

#[test]
fn chart_data_labels_save_and_reopen() {
    use ibis_doc_model::chart::{DataLabels, LabelPosition};
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    // 元ファイルはデータラベルなし
    assert!(chart.groups.iter().flat_map(|g| &g.series).all(|ser| ser.labels.is_none()));

    // 1. 各系列にデータラベルを設定（表示：値、位置：OutsideEnd）
    let mut modified = chart.clone();
    for ser in modified.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        ser.labels = Some(DataLabels {
            show_value: true,
            position: Some(LabelPosition::OutsideEnd),
            ..Default::default()
        });
    }
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-data-labels.xlsx");

    let read = ibis_pptx::open(bytes).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    for ser in read_chart.groups.iter().flat_map(|g| &g.series) {
        let labels = ser.labels.as_ref().unwrap();
        assert!(labels.show_value);
        assert_eq!(labels.position, Some(LabelPosition::OutsideEnd));
    }

    // 2. データラベルの位置を Center に変更
    let mut edited = read_chart.clone();
    for ser in edited.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        let labels = ser.labels.as_mut().unwrap();
        labels.position = Some(LabelPosition::Center);
    }
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-data-labels-edited.xlsx");

    let read2 = ibis_pptx::open(bytes2).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    for ser in read2_chart.groups.iter().flat_map(|g| &g.series) {
        let labels = ser.labels.as_ref().unwrap();
        assert_eq!(labels.position, Some(LabelPosition::Center));
    }

    // 3. データラベルを削除（None）
    let mut cleared = read2_chart.clone();
    for ser in cleared.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
        ser.labels = None;
    }
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(cleared.clone()))) }]).unwrap();
    let bytes3 = save(&read2, d.presentation(), "chart-data-labels-cleared.xlsx");

    let read3 = ibis_pptx::open(bytes3).unwrap();
    let (_, read3_chart) = read3.presentation.slide(slide(&read3.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(read3_chart.groups.iter().flat_map(|g| &g.series).all(|ser| ser.labels.is_none()));

    // 4. Undo して Center のラベルが復元されること
    assert!(d.undo());
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    for ser in undo_chart.groups.iter().flat_map(|g| &g.series) {
        let labels = ser.labels.as_ref().unwrap();
        assert_eq!(labels.position, Some(LabelPosition::Center));
    }
}

#[test]
fn chart_axis_orientation_and_crosses_save_and_reopen() {
    use ibis_doc_model::chart::AxisCrosses;
    use ibis_doc_model::Number;
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let cat_idx = chart.axes.iter().position(|a| a.kind == ibis_doc_model::chart::AxisKind::Category).unwrap();
    let val_idx = chart.axes.iter().position(|a| a.kind == ibis_doc_model::chart::AxisKind::Value).unwrap();
    assert!(!chart.axes[cat_idx].reversed);
    assert!(!chart.axes[val_idx].reversed);

    // 1. 項目軸・値軸を逆順（reversed=true）にし、値軸の交点を最大値（Maximum）に設定
    let mut modified = chart.clone();
    modified.axes[cat_idx].reversed = true;
    modified.axes[val_idx].reversed = true;
    modified.axes[val_idx].crosses = Some(AxisCrosses::Maximum);
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-axis-reversed.xlsx");

    let read = ibis_pptx::open(bytes).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(read_chart.axes[cat_idx].reversed);
    assert!(read_chart.axes[val_idx].reversed);
    assert_eq!(read_chart.axes[val_idx].crosses, Some(AxisCrosses::Maximum));

    // 2. 値軸の交点を特定の値 (crosses_at=500.0) に変更
    let mut edited = read_chart.clone();
    edited.axes[val_idx].crosses = None;
    edited.axes[val_idx].crosses_at = Some(Number(500.0));
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-axis-crosses-at.xlsx");

    let read2 = ibis_pptx::open(bytes2).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read2_chart.axes[val_idx].crosses_at, Some(Number(500.0)));

    // 3. 軸を標準順 (reversed=false) と autoZero に戻す
    let mut cleared = read2_chart.clone();
    cleared.axes[cat_idx].reversed = false;
    cleared.axes[val_idx].reversed = false;
    cleared.axes[val_idx].crosses_at = None;
    cleared.axes[val_idx].crosses = Some(AxisCrosses::AutoZero);
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(cleared.clone()))) }]).unwrap();
    let bytes3 = save(&read2, d.presentation(), "chart-axis-restored.xlsx");

    let read3 = ibis_pptx::open(bytes3).unwrap();
    let (_, read3_chart) = read3.presentation.slide(slide(&read3.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(!read3_chart.axes[cat_idx].reversed);
    assert!(!read3_chart.axes[val_idx].reversed);
    assert_eq!(read3_chart.axes[val_idx].crosses, Some(AxisCrosses::AutoZero));

    // 4. Undo 検証
    assert!(d.undo());
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(undo_chart.axes[cat_idx].reversed);
    assert!(undo_chart.axes[val_idx].reversed);
    assert_eq!(undo_chart.axes[val_idx].crosses_at, Some(Number(500.0)));
}

#[test]
fn chart_minor_and_cat_gridlines_save_and_reopen() {
    use ibis_doc_model::{Color, Rgb, Stroke};
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let cat_idx = chart.axes.iter().position(|a| a.kind == ibis_doc_model::chart::AxisKind::Category).unwrap();
    let val_idx = chart.axes.iter().position(|a| a.kind == ibis_doc_model::chart::AxisKind::Value).unwrap();
    assert!(chart.axes[cat_idx].gridlines.is_none());
    assert!(chart.axes[val_idx].gridlines.is_some());
    assert!(chart.axes[val_idx].minor_gridlines.is_none());

    // 1. 項目軸に主目盛線、値軸に補助目盛線を設定して保存
    let mut modified = chart.clone();
    modified.axes[cat_idx].gridlines = Some(Stroke::solid(Color::opaque(Rgb(0xD9D9D9)), Stroke::DEFAULT_WIDTH));
    modified.axes[val_idx].minor_gridlines = Some(Stroke::solid(Color::opaque(Rgb(0xEBEBEB)), Stroke::DEFAULT_WIDTH * 3 / 4));
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-minor-gridlines.xlsx");

    let read = ibis_pptx::open(bytes.clone()).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(read_chart.axes[cat_idx].gridlines.is_some(), "cat axis should have gridlines");
    assert!(read_chart.axes[val_idx].minor_gridlines.is_some(), "val axis should have minor gridlines");

    let package = ibis_ooxml::Package::open(bytes).unwrap();
    let chart_xml = package.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml.contains("minorGridlines"), "chart xml should contain minorGridlines element");

    // 2. 項目軸の目盛線・値軸の補助目盛線を解除（None）して保存
    let mut cleared = read_chart.clone();
    cleared.axes[cat_idx].gridlines = None;
    cleared.axes[val_idx].minor_gridlines = None;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(cleared.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-minor-gridlines-cleared.xlsx");

    let read2 = ibis_pptx::open(bytes2).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(read2_chart.axes[cat_idx].gridlines.is_none());
    assert!(read2_chart.axes[val_idx].minor_gridlines.is_none());

    // 3. Undo 検証
    assert!(d.undo());
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(undo_chart.axes[cat_idx].gridlines.is_some());
    assert!(undo_chart.axes[val_idx].minor_gridlines.is_some());
}

#[test]
fn chart_axis_units_and_disp_units_save_and_reopen() {
    use ibis_doc_model::Number;
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let val_idx = chart.axes.iter().position(|a| a.kind == ibis_doc_model::chart::AxisKind::Value).unwrap();
    assert!(chart.axes[val_idx].minor_unit.is_none());
    assert!(chart.axes[val_idx].display_unit.is_none());

    // 1. 値軸に補助単位 (minor_unit=200.0) および表示単位 (display_unit=1000.0 / thousands) を設定
    let mut modified = chart.clone();
    modified.axes[val_idx].minor_unit = Some(Number(200.0));
    modified.axes[val_idx].display_unit = Some(Number(1000.0));
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-disp-units.xlsx");

    let read = ibis_pptx::open(bytes.clone()).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read_chart.axes[val_idx].minor_unit, Some(Number(200.0)));
    assert_eq!(read_chart.axes[val_idx].display_unit, Some(Number(1000.0)));

    let package = ibis_ooxml::Package::open(bytes).unwrap();
    let chart_xml = package.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml.contains(r#"builtInUnit val="thousands""#), "chart xml should contain builtInUnit thousands: {chart_xml}");
    assert!(chart_xml.contains(r#"minorUnit val="200""#), "chart xml should contain minorUnit 200: {chart_xml}");

    // 2. 表示単位を millions (1,000,000.0) に変更
    let mut edited = read_chart.clone();
    edited.axes[val_idx].display_unit = Some(Number(1_000_000.0));
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-disp-units-millions.xlsx");

    let read2 = ibis_pptx::open(bytes2.clone()).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read2_chart.axes[val_idx].display_unit, Some(Number(1_000_000.0)));
    let package2 = ibis_ooxml::Package::open(bytes2).unwrap();
    let chart_xml2 = package2.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package2.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml2.contains(r#"builtInUnit val="millions""#));

    // 3. 表示単位・補助単位を解除 (None)
    let mut cleared = read2_chart.clone();
    cleared.axes[val_idx].display_unit = None;
    cleared.axes[val_idx].minor_unit = None;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(cleared.clone()))) }]).unwrap();
    let bytes3 = save(&read2, d.presentation(), "chart-disp-units-cleared.xlsx");

    let read3 = ibis_pptx::open(bytes3.clone()).unwrap();
    let (_, read3_chart) = read3.presentation.slide(slide(&read3.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(read3_chart.axes[val_idx].display_unit.is_none());
    assert!(read3_chart.axes[val_idx].minor_unit.is_none());

    // 4. Undo 検証
    assert!(d.undo());
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart.axes[val_idx].display_unit, Some(Number(1_000_000.0)));
    assert_eq!(undo_chart.axes[val_idx].minor_unit, Some(Number(200.0)));
}

#[test]
fn chart_gap_width_and_overlap_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let initial_kind = &chart.groups[0].kind;
    let (h, initial_gap, initial_overlap) = match initial_kind {
        ibis_doc_model::ChartType::Bar { horizontal, gap_width, overlap } => (*horizontal, *gap_width, *overlap),
        _ => panic!("first group should be bar chart"),
    };

    // 1. gap_width = 80, overlap = 50 に設定して保存・再読込検証
    let mut modified = chart.clone();
    modified.groups[0].kind = ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 80, overlap: 50 };
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-gap-overlap.xlsx");

    let read = ibis_pptx::open(bytes.clone()).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read_chart.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 80, overlap: 50 });

    let package = ibis_ooxml::Package::open(bytes).unwrap();
    let chart_xml = package.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml.contains(r#"gapWidth val="80""#), "chart xml should contain gapWidth 80: {chart_xml}");
    assert!(chart_xml.contains(r#"overlap val="50""#), "chart xml should contain overlap 50: {chart_xml}");

    // 2. gap_width = 250, overlap = -50 に変更して保存・再読込検証
    let mut edited = read_chart.clone();
    edited.groups[0].kind = ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 250, overlap: -50 };
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-gap-overlap-2.xlsx");

    let read2 = ibis_pptx::open(bytes2.clone()).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read2_chart.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 250, overlap: -50 });

    let package2 = ibis_ooxml::Package::open(bytes2).unwrap();
    let chart_xml2 = package2.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package2.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml2.contains(r#"gapWidth val="250""#));
    assert!(chart_xml2.contains(r#"overlap val="-50""#));

    // 3. Undo 検証
    assert!(d.undo());
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: 80, overlap: 50 });

    assert!(d.undo());
    let (_, initial_restored) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(initial_restored.groups[0].kind, ibis_doc_model::ChartType::Bar { horizontal: h, gap_width: initial_gap, overlap: initial_overlap });
}

#[test]
fn chart_log_scale_axis_save_and_reopen() {
    use ibis_doc_model::Number;
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let val_idx = chart.axes.iter().position(|a| a.kind == ibis_doc_model::chart::AxisKind::Value).unwrap();
    assert!(chart.axes[val_idx].log_base.is_none());

    // 1. 値軸に対数目盛 (log_base=10.0) を設定
    let mut modified = chart.clone();
    modified.axes[val_idx].log_base = Some(Number(10.0));
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-log-scale.xlsx");
    let package = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let chart_xml = package.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml.contains(r#"logBase val="10""#), "chart xml should contain logBase 10: {chart_xml}");

    let read = ibis_pptx::open(bytes.clone()).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read_chart.axes[val_idx].log_base, Some(Number(10.0)));

    // 2. 底を 2.0 に変更
    let mut edited = read_chart.clone();
    edited.axes[val_idx].log_base = Some(Number(2.0));
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-log-scale-2.xlsx");

    let read2 = ibis_pptx::open(bytes2.clone()).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read2_chart.axes[val_idx].log_base, Some(Number(2.0)));

    let package2 = ibis_ooxml::Package::open(bytes2).unwrap();
    let chart_xml2 = package2.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package2.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml2.contains(r#"logBase val="2""#));

    // 3. 対数目盛を解除 (None)
    let mut cleared = read2_chart.clone();
    cleared.axes[val_idx].log_base = None;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(cleared.clone()))) }]).unwrap();
    let bytes3 = save(&read2, d.presentation(), "chart-log-scale-cleared.xlsx");

    let read3 = ibis_pptx::open(bytes3.clone()).unwrap();
    let (_, read3_chart) = read3.presentation.slide(slide(&read3.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert!(read3_chart.axes[val_idx].log_base.is_none());

    let package3 = ibis_ooxml::Package::open(bytes3).unwrap();
    let chart_xml3 = package3.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package3.read(n).unwrap()).unwrap()).unwrap();
    assert!(!chart_xml3.contains("logBase"));

    // 4. Undo 検証
    assert!(d.undo());
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart.axes[val_idx].log_base, Some(Number(2.0)));
}

#[test]
fn chart_disp_blanks_as_and_plot_vis_only_save_and_reopen() {
    use ibis_doc_model::DispBlanksAs;
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(chart.plot_vis_only, true);

    // 1. disp_blanks_as = Zero, plot_vis_only = false に変更
    let mut modified = chart.clone();
    modified.disp_blanks_as = Some(DispBlanksAs::Zero);
    modified.plot_vis_only = false;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-blanks-zero.xlsx");
    let package = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let chart_xml = package.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml.contains(r#"plotVisOnly val="0""#), "XML should contain plotVisOnly 0: {chart_xml}");
    assert!(chart_xml.contains(r#"dispBlanksAs val="zero""#), "XML should contain dispBlanksAs zero: {chart_xml}");

    let read = ibis_pptx::open(bytes.clone()).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read_chart.disp_blanks_as, Some(DispBlanksAs::Zero));
    assert_eq!(read_chart.plot_vis_only, false);

    // 2. disp_blanks_as = Span, plot_vis_only = true に変更
    let mut edited = read_chart.clone();
    edited.disp_blanks_as = Some(DispBlanksAs::Span);
    edited.plot_vis_only = true;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-blanks-span.xlsx");

    let read2 = ibis_pptx::open(bytes2.clone()).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read2_chart.disp_blanks_as, Some(DispBlanksAs::Span));
    assert_eq!(read2_chart.plot_vis_only, true);

    let package2 = ibis_ooxml::Package::open(bytes2).unwrap();
    let chart_xml2 = package2.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package2.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml2.contains(r#"plotVisOnly val="1""#));
    assert!(chart_xml2.contains(r#"dispBlanksAs val="span""#));

    // 3. disp_blanks_as = Gap に変更
    let mut edited3 = read2_chart.clone();
    edited3.disp_blanks_as = Some(DispBlanksAs::Gap);
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited3.clone()))) }]).unwrap();
    let bytes3 = save(&read2, d.presentation(), "chart-blanks-gap.xlsx");

    let read3 = ibis_pptx::open(bytes3.clone()).unwrap();
    let (_, read3_chart) = read3.presentation.slide(slide(&read3.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read3_chart.disp_blanks_as, Some(DispBlanksAs::Gap));

    let package3 = ibis_ooxml::Package::open(bytes3).unwrap();
    let chart_xml3 = package3.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package3.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml3.contains(r#"dispBlanksAs val="gap""#));

    // 4. disp_blanks_as = None (削除)
    let mut edited4 = read3_chart.clone();
    edited4.disp_blanks_as = None;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited4.clone()))) }]).unwrap();
    let bytes4 = save(&read3, d.presentation(), "chart-blanks-none.xlsx");

    let read4 = ibis_pptx::open(bytes4.clone()).unwrap();
    let (_, read4_chart) = read4.presentation.slide(slide(&read4.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read4_chart.disp_blanks_as, None);

    let package4 = ibis_ooxml::Package::open(bytes4).unwrap();
    let chart_xml4 = package4.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package4.read(n).unwrap()).unwrap()).unwrap();
    assert!(!chart_xml4.contains("dispBlanksAs"));

    // 5. Undo 検証
    assert!(d.undo()); // None -> Gap
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart.disp_blanks_as, Some(DispBlanksAs::Gap));

    assert!(d.undo()); // Gap -> Span (and plot_vis_only=true)
    let (_, undo_chart2) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart2.disp_blanks_as, Some(DispBlanksAs::Span));

    assert!(d.undo()); // Span -> Zero (and plot_vis_only=false)
    let (_, undo_chart3) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart3.disp_blanks_as, Some(DispBlanksAs::Zero));
    assert_eq!(undo_chart3.plot_vis_only, false);
}

#[test]
fn chart_series_marker_and_stroke_save_and_reopen() {
    use ibis_doc_model::chart::{Marker, MarkerSymbol};
    use ibis_doc_model::{Color, Fill, Rgb, Stroke};
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();

    // 1. 系列 0 にマーカー (ひし形, サイズ 7, 赤) と 線 (太さ 38100, 緑) を設定
    let mut modified = chart.clone();
    modified.groups[0].kind=ibis_doc_model::ChartType::Line;
    modified.groups[0].series[0].marker = Some(Marker {
        source: None,
        symbol: MarkerSymbol::Diamond,
        size: 7,
        fill: Fill::Solid(Color::opaque(Rgb(0xFF0000))),
    });
    modified.groups[0].series[0].stroke = Some(Stroke::solid(Color::opaque(Rgb(0x00FF00)), 38_100));

    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-series-marker-stroke.xlsx");
    let package = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let chart_xml = package.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml.contains(r#"symbol val="diamond""#), "XML should contain diamond symbol: {chart_xml}");
    assert!(chart_xml.contains(r#"size val="7""#), "XML should contain marker size 7: {chart_xml}");
    assert!(chart_xml.contains(r#"srgbClr val="FF0000""#), "XML should contain red marker: {chart_xml}");
    assert!(chart_xml.contains(r#"ln w="38100""#), "XML should contain stroke width 38100: {chart_xml}");
    assert!(chart_xml.contains(r#"srgbClr val="00FF00""#), "XML should contain green stroke: {chart_xml}");

    let read = ibis_pptx::open(bytes.clone()).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let ser0 = &read_chart.groups[0].series[0];
    assert_eq!(ser0.marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::Diamond));
    assert_eq!(ser0.marker.as_ref().map(|m| m.size), Some(7));
    assert_eq!(ser0.stroke.as_ref().map(|s| s.width), Some(38_100));

    // 2. マーカーを「なし」(symbol: None)、線を「細」(12700) に変更
    let mut edited = read_chart.clone();
    edited.groups[0].series[0].marker = Some(Marker {
        source: None,
        symbol: MarkerSymbol::None,
        size: 5,
        fill: Fill::None,
    });
    edited.groups[0].series[0].stroke = Some(Stroke::solid(Color::opaque(Rgb(0x0000FF)), 12_700));

    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-series-marker-none.xlsx");

    let read2 = ibis_pptx::open(bytes2.clone()).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let ser0_2 = &read2_chart.groups[0].series[0];
    assert_eq!(ser0_2.marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::None));
    assert_eq!(ser0_2.stroke.as_ref().map(|s| s.width), Some(12_700));

    let package2 = ibis_ooxml::Package::open(bytes2).unwrap();
    let chart_xml2 = package2.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package2.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml2.contains(r#"symbol val="none""#));
    assert!(chart_xml2.contains(r#"ln w="12700""#));

    // 3. マーカーを None、線を None (線なし) に変更
    let mut edited3 = read2_chart.clone();
    edited3.groups[0].series[0].marker = None;
    edited3.groups[0].series[0].stroke = None;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited3.clone()))) }]).unwrap();
    let bytes3 = save(&read2, d.presentation(), "chart-series-no-stroke.xlsx");

    let read3 = ibis_pptx::open(bytes3.clone()).unwrap();
    let (_, read3_chart) = read3.presentation.slide(slide(&read3.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let ser0_3 = &read3_chart.groups[0].series[0];
    // Removing the override restores the line group's automatic marker.
    assert!(ser0_3.marker.is_some());
    assert!(ser0_3.stroke.is_none());

    let package3 = ibis_ooxml::Package::open(bytes3).unwrap();
    let chart_xml3 = package3.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package3.read(n).unwrap()).unwrap()).unwrap();
    assert!(!chart_xml3.contains("<c:marker>"));
    assert!(chart_xml3.contains("<a:noFill/>"));

    // 4. Undo 検証
    assert!(d.undo()); // None/None -> None(symbol:None)/12700
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart.groups[0].series[0].marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::None));
    assert_eq!(undo_chart.groups[0].series[0].stroke.as_ref().map(|s| s.width), Some(12_700));

    assert!(d.undo()); // -> Diamond/38100
    let (_, undo_chart2) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart2.groups[0].series[0].marker.as_ref().map(|m| m.symbol), Some(MarkerSymbol::Diamond));
    assert_eq!(undo_chart2.groups[0].series[0].stroke.as_ref().map(|s| s.width), Some(38_100));
}

#[test]
fn rich_text_runs_are_read_kept_through_a_format_change_and_set() {
    use ibis_doc_model::{CellRun, Rgb};
    let (_, stock) = open("xlsx-sales.xlsx");
    let bytes = stock.source.save(&stock.presentation, SaveOptions::default()).unwrap();
    let mut package = ibis_ooxml::Package::open(bytes).unwrap();
    let xml = package.xml_mut("xl/sharedStrings.xml").unwrap(); let root = xml.root_element().unwrap();
    let si = xml.children_named(root, "si").nth(6).unwrap();
    for c in xml.children(si).to_vec() { xml.detach(c); }
    let r = xml.append_element(si, "r"); let p = xml.append_element(r, "rPr");
    xml.append_element(p, "b"); let sz = xml.append_element(p, "sz"); xml.set_attr(sz, "val", "11");
    let color = xml.append_element(p, "color"); xml.set_attr(color, "rgb", "FFFF0000");
    let font = xml.append_element(p, "rFont"); xml.set_attr(font, "val", "Arial");
    let t = xml.append_element(r, "t"); xml.set_text(t, "北町");
    let r2 = xml.append_element(si, "r"); let t2 = xml.append_element(r2, "t"); xml.set_text(t2, "店");
    let opened = ibis_pptx::open(package.save(Default::default()).unwrap()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let a4 = cell(d.presentation(), "月次売上", 3, 0);
    assert_eq!(a4.value, CellValue::Text("北町店".into()));
    let runs = a4.runs.clone().expect("runs");
    assert_eq!(runs.len(), 2);
    assert_eq!((runs[0].text.as_str(), runs[0].bold, runs[0].size, runs[0].color, runs[0].font.as_deref()), ("北町", Some(true), Some(1100), Some(Rgb(0xFF0000)), Some("Arial")));
    assert_eq!(runs[1], CellRun { text: "店".into(), ..Default::default() });
    // A format change rewrites the cell's string: the runs are kept.
    let range = CellRange { first_row: 3, first_column: 0, last_row: 3, last_column: 0 };
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat { slide: s, ranges: vec![range], change: StyleChange::Fill(Some(Rgb(0xFFEE99))) })]).unwrap();
    let bytes = save(&opened, d.presentation(), "rich-format.xlsx");
    let read = ibis_pptx::read(&bytes).unwrap();
    assert_eq!(cell(&read, "月次売上", 3, 0).runs, a4.runs);
    // New runs: the second part bold and blue.
    let new = vec![CellRun { text: "北町".into(), bold: Some(true), ..Default::default() }, CellRun { text: "店".into(), bold: Some(true), color: Some(Rgb(0x0000FF)), size: Some(1400), underline: Some(ibis_doc_model::CellUnderline::Double), ..Default::default() }];
    d.execute(vec![Command::Sheet(SheetCommand::SetRuns { slide: s, row: 3, column: 0, runs: new.clone() })]).unwrap();
    let bytes = save(&opened, d.presentation(), "rich-set.xlsx");
    let read = ibis_pptx::read(&bytes).unwrap();
    let got = cell(&read, "月次売上", 3, 0).runs.unwrap();
    assert_eq!(got[0].bold, Some(true));
    assert_eq!((got[1].color, got[1].size, got[1].underline), (Some(Rgb(0x0000FF)), Some(1400), Some(ibis_doc_model::CellUnderline::Double)));
    assert!(d.undo()); assert!(d.redo());
    // Other text in the cell: no runs.
    set(&mut d, s, vec![(3, 0, CellInput::Value(CellValue::Text("北町店2".into())))]);
    assert!(cell(d.presentation(), "月次売上", 3, 0).runs.is_none());
    // Runs that do not make the text are refused.
    assert!(d.execute(vec![Command::Sheet(SheetCommand::SetRuns { slide: s, row: 3, column: 0, runs: new })]).is_err());
}

#[test]
fn a_data_table_is_made_saved_as_a_data_table_and_read_back() {
    use ibis_doc_model::DataTable;
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "データテーブル".into(), slide: s, shape })]).unwrap();
    let num = |v: f64| CellInput::Value(CellValue::Number(v));
    // Inputs F1 (row), F2 (column); the formula in the corner A3; values across B3:D3 and down A4:A5.
    set(&mut d, s, vec![(0, 5, num(0.)), (1, 5, num(0.)), (2, 0, CellInput::Formula("F1*10+F2".into())), (2, 1, num(1.)), (2, 2, num(2.)), (2, 3, num(3.)), (3, 0, num(1.)), (4, 0, num(2.))]);
    let table = DataTable { range: CellRange { first_row: 3, last_row: 4, first_column: 1, last_column: 3 }, row_input: Some((0, 5)), column_input: Some((1, 5)) };
    d.execute(vec![Command::Sheet(SheetCommand::SetDataTable { slide: s, table: table.clone() })]).unwrap();
    let bytes = save(&opened, d.presentation(), "data-table.xlsx");
    let sheet_xml = |bytes: &[u8]| {
        let p = ibis_ooxml::Package::open(bytes.to_vec()).unwrap();
        p.names().filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml")).filter_map(|n| String::from_utf8(p.read(n).unwrap()).ok()).find(|t| t.contains("dataTable")).unwrap_or_default()
    };
    let xml = sheet_xml(&bytes);
    assert!(xml.contains("t=\"dataTable\"") && xml.contains("ref=\"B4:D5\"") && xml.contains("dt2D=\"1\"") && xml.contains("r1=\"F1\"") && xml.contains("r2=\"F2\""), "{xml}");
    let read = ibis_pptx::read(&bytes).unwrap();
    let sheet = sheet_of_slide(read.slide(slide(&read, "データテーブル")).unwrap()).unwrap();
    assert_eq!(sheet.data_tables, vec![table]);
    assert_eq!(value(&read, "データテーブル", 4, 3), CellValue::Number(32.0));
    // A header changed and saved again: the results are written, the table kept.
    let reopened = ibis_pptx::open(bytes).unwrap();
    let mut d2 = Document::new(reopened.presentation.clone());
    let s2 = slide(d2.presentation(), "データテーブル");
    set(&mut d2, s2, vec![(2, 1, num(7.))]);
    let bytes2 = save(&reopened, d2.presentation(), "data-table-edited.xlsx");
    assert!(sheet_xml(&bytes2).contains("t=\"dataTable\""));
    let read2 = ibis_pptx::read(&bytes2).unwrap();
    assert_eq!(value(&read2, "データテーブル", 3, 1), CellValue::Number(71.0));
    assert_eq!(sheet_of_slide(read2.slide(slide(&read2, "データテーブル")).unwrap()).unwrap().data_tables.len(), 1);
}

#[test]
fn one_input_data_tables_are_saved_and_read_back() {
    use ibis_doc_model::DataTable;
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "1入力テーブル".into(), slide: s, shape })]).unwrap();
    let num = |v: f64| CellInput::Value(CellValue::Number(v));
    // 1. Column-oriented table (Column Input Cell only, r1=F2):
    // Formula in B3 (=F2*2), inputs in A4:A6 (10, 20, 30), results in B4:B6 (20, 40, 60).
    set(&mut d, s, vec![(1, 5, num(0.)), (2, 1, CellInput::Formula("F2*2".into())), (3, 0, num(10.)), (4, 0, num(20.)), (5, 0, num(30.))]);
    let col_table = DataTable { range: CellRange { first_row: 3, last_row: 5, first_column: 1, last_column: 1 }, row_input: None, column_input: Some((1, 5)) };
    d.execute(vec![Command::Sheet(SheetCommand::SetDataTable { slide: s, table: col_table.clone() })]).unwrap();
    let bytes_col = save(&opened, d.presentation(), "data-table-col.xlsx");
    let sheet_xml = |bytes: &[u8]| {
        let p = ibis_ooxml::Package::open(bytes.to_vec()).unwrap();
        p.names().filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml")).filter_map(|n| String::from_utf8(p.read(n).unwrap()).ok()).find(|t| t.contains("dataTable")).unwrap_or_default()
    };
    let xml_col = sheet_xml(&bytes_col);
    assert!(xml_col.contains("t=\"dataTable\"") && xml_col.contains("ref=\"B4:B6\"") && xml_col.contains("r1=\"F2\"") && !xml_col.contains("dtr=") && !xml_col.contains("dt2D="), "{xml_col}");
    let read_col = ibis_pptx::read(&bytes_col).unwrap();
    let sheet_col = sheet_of_slide(read_col.slide(slide(&read_col, "1入力テーブル")).unwrap()).unwrap();
    assert_eq!(sheet_col.data_tables, vec![col_table]);

    // 2. Row-oriented table (Row Input Cell only, dtr="1", r1=F1):
    let mut d2 = Document::new(opened.presentation.clone());
    let (s2, shape2) = (d2.ids().new_slide_id(), d2.ids().new_shape_id());
    d2.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "1入力行テーブル".into(), slide: s2, shape: shape2 })]).unwrap();
    // Formula in A4 (=F1*3), inputs in B3:D3 (10, 20, 30), results in B4:D4 (30, 60, 90).
    set(&mut d2, s2, vec![(0, 5, num(0.)), (3, 0, CellInput::Formula("F1*3".into())), (2, 1, num(10.)), (2, 2, num(20.)), (2, 3, num(30.))]);
    let row_table = DataTable { range: CellRange { first_row: 3, last_row: 3, first_column: 1, last_column: 3 }, row_input: Some((0, 5)), column_input: None };
    d2.execute(vec![Command::Sheet(SheetCommand::SetDataTable { slide: s2, table: row_table.clone() })]).unwrap();
    let bytes_row = save(&opened, d2.presentation(), "data-table-row.xlsx");
    let xml_row = sheet_xml(&bytes_row);
    assert!(xml_row.contains("t=\"dataTable\"") && xml_row.contains("ref=\"B4:D4\"") && xml_row.contains("r1=\"F1\"") && xml_row.contains("dtr=\"1\"") && !xml_row.contains("dt2D="), "{xml_row}");
    let read_row = ibis_pptx::read(&bytes_row).unwrap();
    let sheet_row = sheet_of_slide(read_row.slide(slide(&read_row, "1入力行テーブル")).unwrap()).unwrap();
    assert_eq!(sheet_row.data_tables, vec![row_table]);
}

#[test]
fn custom_cell_style_save_and_reopen() {
    let (_, opened) = open("xlsx-formats.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let custom_style = ibis_doc_model::CustomCellStyle {
        name: "強調 1".into(),
        format: ibis_doc_model::CellStyle {
            bold: true,
            color: ibis_doc_model::Rgb(0xFF0000),
            fill: Some(ibis_doc_model::Rgb(0xFFFFCC)),
            number_format: "#,##0".into(),
            ..Default::default()
        },
        include_number_format: true,
        include_alignment: false,
        include_font: true,
        include_border: false,
        include_fill: true,
        include_protection: false,
    };
    d.execute(vec![Command::Sheet(SheetCommand::AddCustomCellStyle { style: custom_style.clone() })]).unwrap();
    let bytes = save(&opened, d.presentation(), "custom-cell-style.xlsx");
    let read = ibis_pptx::read(&bytes).unwrap();
    let wb = read.workbook.as_ref().unwrap();
    assert_eq!(wb.custom_cell_styles.len(), 1);
    assert_eq!(wb.custom_cell_styles[0].name, "強調 1");
    assert!(wb.custom_cell_styles[0].include_font);
    assert!(wb.custom_cell_styles[0].include_fill);
    assert!(wb.custom_cell_styles[0].include_number_format);
    assert!(!wb.custom_cell_styles[0].include_alignment);
    assert_eq!(wb.custom_cell_styles[0].format.color, ibis_doc_model::Rgb(0xFF0000));
    assert_eq!(wb.custom_cell_styles[0].format.fill, Some(ibis_doc_model::Rgb(0xFFFFCC)));
    assert_eq!(wb.custom_cell_styles[0].format.number_format, "#,##0");
}

/// XLSX-CF-003: timePeriod conditional formatting rule (today, yesterday, etc.)
/// saved with type="timePeriod" and timePeriod attribute and read back into CfKind::TimePeriod.
#[test]
fn time_period_conditional_save_and_reopen() {
    let (_, opened) = open("xlsx-formats.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "表示形式");
    let format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 1, last_row: 5, first_column: 0, last_column: 0 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::TimePeriod {
                period: "today".into(),
                formula: "FLOOR(A2,1)=TODAY()".into(),
            },
            priority: 1,
            stop: false,
            style: Some(ibis_doc_model::CfStyle {
                fill: Some(ibis_doc_model::Rgb(0xFFC7CE)),
                color: Some(ibis_doc_model::Rgb(0x9C0006)),
                ..Default::default()
            }),
        }],
    };
    d.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide: s, format })]).unwrap();
    let bytes = save(&opened, d.presentation(), "time-period-cf.xlsx");
    let read = ibis_pptx::read(&bytes).unwrap();
    let sheet = sheet_of_slide(&read.slides[0]).unwrap();
    let rule = sheet.conditional.iter().find_map(|cf| {
        cf.rules.iter().find(|r| matches!(&r.kind, ibis_doc_model::CfKind::TimePeriod { period, .. } if period == "today"))
    }).expect("the timePeriod rule");
    if let ibis_doc_model::CfKind::TimePeriod { period, formula } = &rule.kind {
        assert_eq!(period, "today");
        assert_eq!(formula, "FLOOR(A2,1)=TODAY()");
    } else {
        panic!("unexpected kind: {:?}", rule.kind);
    }
    assert_eq!(rule.style.as_ref().unwrap().fill, Some(ibis_doc_model::Rgb(0xFFC7CE)));
    assert_eq!(rule.style.as_ref().unwrap().color, Some(ibis_doc_model::Rgb(0x9C0006)));
}

/// XLSX-CF-011: conditional formatting threshold types (min, max, num, percent, percentile, formula)
/// for colorScale, dataBar, and iconSet rules saved into OOXML cfvo and read back faithfully.
#[test]
fn conditional_format_threshold_types_save_and_reopen() {
    let (_, opened) = open("xlsx-formats.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "表示形式");

    // 1. ColorScale rule with min, percentile(50), and max
    let color_scale_format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 1, last_row: 5, first_column: 0, last_column: 0 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::ColorScale(vec![
                (ibis_doc_model::CfValue { kind: "min".into(), value: None, gte: true }, ibis_doc_model::Rgb(0xF8696B)),
                (ibis_doc_model::CfValue { kind: "percentile".into(), value: Some("50".into()), gte: true }, ibis_doc_model::Rgb(0xFFEB84)),
                (ibis_doc_model::CfValue { kind: "max".into(), value: None, gte: true }, ibis_doc_model::Rgb(0x63BE7B)),
            ]),
            priority: 1,
            stop: false,
            style: None,
        }],
    };

    // 2. IconSet rule with percent(0), percentile(33), and formula("AVERAGE(A1:A5)")
    let icon_set_format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 1, last_row: 5, first_column: 1, last_column: 1 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::icon_set(
                "3TrafficLights1",
                vec![
                    ibis_doc_model::CfValue { kind: "percent".into(), value: Some("0".into()), gte: true },
                    ibis_doc_model::CfValue { kind: "percentile".into(), value: Some("33".into()), gte: true },
                    ibis_doc_model::CfValue { kind: "formula".into(), value: Some("AVERAGE(A1:A5)".into()), gte: true },
                ],
                false,
                true,
            ),
            priority: 2,
            stop: false,
            style: None,
        }],
    };

    // 3. DataBar rule with num(10) and formula("B1")
    let data_bar_format = ibis_doc_model::ConditionalFormat {
        source_index: None,
        ranges: vec![ibis_doc_model::CellRange { first_row: 1, last_row: 5, first_column: 2, last_column: 2 }],
        rules: vec![ibis_doc_model::CfRule {
            kind: ibis_doc_model::CfKind::data_bar(
                ibis_doc_model::CfValue { kind: "num".into(), value: Some("10".into()), gte: true },
                ibis_doc_model::CfValue { kind: "formula".into(), value: Some("B1".into()), gte: true },
                ibis_doc_model::Rgb(0x638EC6),
                true,
            ),
            priority: 3,
            stop: false,
            style: None,
        }],
    };

    d.execute(vec![
        Command::Sheet(SheetCommand::AddConditional { slide: s, format: color_scale_format }),
        Command::Sheet(SheetCommand::AddConditional { slide: s, format: icon_set_format }),
        Command::Sheet(SheetCommand::AddConditional { slide: s, format: data_bar_format }),
    ]).unwrap();

    let bytes = save(&opened, d.presentation(), "threshold-types-cf.xlsx");

    // Verify low-level XML output
    let pkg = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let xml = pkg.xml("xl/worksheets/sheet1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let cf_nodes: Vec<_> = xml.children_named(root, "conditionalFormatting").collect();
    assert!(!cf_nodes.is_empty(), "expected conditionalFormatting nodes");

    // Verify read-back model
    let read = ibis_pptx::read(&bytes).unwrap();
    let sheet = sheet_of_slide(&read.slides[0]).unwrap();

    // 1. Verify ColorScale
    let cs_rule = sheet.conditional.iter().find_map(|cf| {
        cf.rules.iter().find(|r| matches!(&r.kind, ibis_doc_model::CfKind::ColorScale(_)))
    }).expect("colorScale rule");
    if let ibis_doc_model::CfKind::ColorScale(pts) = &cs_rule.kind {
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[0].0.kind, "min");
        assert_eq!(pts[0].0.value, None);
        assert_eq!(pts[1].0.kind, "percentile");
        assert_eq!(pts[1].0.value.as_deref(), Some("50"));
        assert_eq!(pts[2].0.kind, "max");
        assert_eq!(pts[2].0.value, None);
    } else {
        panic!("unexpected rule kind");
    }

    // 2. Verify IconSet
    let is_rule = sheet.conditional.iter().find_map(|cf| {
        cf.rules.iter().find(|r| matches!(&r.kind, ibis_doc_model::CfKind::IconSet { .. }))
    }).expect("iconSet rule");
    if let ibis_doc_model::CfKind::IconSet { set, values, .. } = &is_rule.kind {
        assert_eq!(set, "3TrafficLights1");
        assert_eq!(values.len(), 3);
        assert_eq!(values[0].kind, "percent");
        assert_eq!(values[0].value.as_deref(), Some("0"));
        assert_eq!(values[1].kind, "percentile");
        assert_eq!(values[1].value.as_deref(), Some("33"));
        assert_eq!(values[2].kind, "formula");
        assert_eq!(values[2].value.as_deref(), Some("AVERAGE(A1:A5)"));
    } else {
        panic!("unexpected rule kind");
    }

    // 3. Verify DataBar
    let db_rule = sheet.conditional.iter().find_map(|cf| {
        cf.rules.iter().find(|r| matches!(&r.kind, ibis_doc_model::CfKind::DataBar { .. }))
    }).expect("dataBar rule");
    if let ibis_doc_model::CfKind::DataBar { min, max, gradient, .. } = &db_rule.kind {
        assert!(*gradient);
        assert_eq!(min.kind, "num");
        assert_eq!(min.value.as_deref(), Some("10"));
        assert_eq!(max.kind, "formula");
        assert_eq!(max.value.as_deref(), Some("B1"));
    } else {
        panic!("unexpected rule kind");
    }

    // Test Undo/Redo
    assert!(d.undo());
    let undo_bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let undo_read = ibis_pptx::read(&undo_bytes).unwrap();
    let undo_sheet = sheet_of_slide(&undo_read.slides[0]).unwrap();
    assert!(!undo_sheet.conditional.iter().any(|cf| {
        cf.rules.iter().any(|r| matches!(&r.kind, ibis_doc_model::CfKind::ColorScale(_) | ibis_doc_model::CfKind::IconSet { .. } | ibis_doc_model::CfKind::DataBar { .. }))
    }));

    assert!(d.redo());
    let redo_bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let redo_read = ibis_pptx::read(&redo_bytes).unwrap();
    let redo_sheet = sheet_of_slide(&redo_read.slides[0]).unwrap();
    assert!(redo_sheet.conditional.iter().any(|cf| {
        cf.rules.iter().any(|r| matches!(&r.kind, ibis_doc_model::CfKind::ColorScale(_)))
    }));
}

#[test]
fn case_sensitive_sort_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "大小区別並替".into(), slide: s, shape })]).unwrap();
    let rows = [("Item", "0"), ("b", "1"), ("B", "2"), ("a", "3"), ("A", "4")];
    set(&mut d, s, rows.iter().enumerate().map(|(r, (item, _))| (r as u32, 0, CellInput::Value(CellValue::Text((*item).into())))).collect());
    let range = CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 0 };

    // Case-sensitive sort ascending
    d.execute(vec![Command::Sheet(SheetCommand::SortBy {
        slide: s,
        range,
        keys: vec![(0, ibis_doc_model::SortBy::Value(true))],
        header: true,
        match_case: true,
    })]).unwrap();

    let bytes = save(&opened, d.presentation(), "case-sensitive-sort.xlsx");
    assert_reads_back(&bytes, d.presentation());
    let p = ibis_pptx::read(&bytes).unwrap();
    let expected = ["Item", "A", "a", "B", "b"];
    for (r, exp) in expected.iter().enumerate() {
        assert_eq!(value(&p, "大小区別並替", r as u32, 0), CellValue::Text((*exp).into()));
    }

    // Undo restores original order
    assert!(d.undo());
    let original = ["Item", "b", "B", "a", "A"];
    for (r, exp) in original.iter().enumerate() {
        assert_eq!(value(d.presentation(), "大小区別並替", r as u32, 0), CellValue::Text((*exp).into()));
    }

    // Redo restores sorted order
    assert!(d.redo());
    for (r, exp) in expected.iter().enumerate() {
        assert_eq!(value(d.presentation(), "大小区別並替", r as u32, 0), CellValue::Text((*exp).into()));
    }
}

#[test]
fn custom_list_sort_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (s, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "カスタム並替".into(), slide: s, shape })]).unwrap();

    let rows = [("優先度", "ID"), ("低", "1"), ("高", "2"), ("緊急", "3"), ("中", "4"), ("未設定", "5")];
    set(&mut d, s, rows.iter().enumerate().map(|(r, (item, _))| (r as u32, 0, CellInput::Value(CellValue::Text((*item).into())))).collect());
    set(&mut d, s, rows.iter().enumerate().map(|(r, (_, id))| (r as u32, 1, CellInput::Value(CellValue::Text((*id).into())))).collect());
    let range = CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 1 };

    let priorities = vec!["緊急".into(), "高".into(), "中".into(), "低".into()];

    // Custom list ascending: 緊急 (ID 3), 高 (ID 2), 中 (ID 4), 低 (ID 1), 未設定 (ID 5)
    d.execute(vec![Command::Sheet(SheetCommand::SortBy {
        slide: s,
        range,
        keys: vec![(0, ibis_doc_model::SortBy::CustomList { list: priorities.clone(), ascending: true })],
        header: true,
        match_case: false,
    })]).unwrap();

    let bytes = save(&opened, d.presentation(), "custom-list-sort.xlsx");
    assert_reads_back(&bytes, d.presentation());
    let p = ibis_pptx::read(&bytes).unwrap();
    let expected = [("優先度", "ID"), ("緊急", "3"), ("高", "2"), ("中", "4"), ("低", "1"), ("未設定", "5")];
    for (r, (prio, id)) in expected.iter().enumerate() {
        assert_eq!(value(&p, "カスタム並替", r as u32, 0), CellValue::Text((*prio).into()));
        assert_eq!(value(&p, "カスタム並替", r as u32, 1), CellValue::Text((*id).into()));
    }

    // Undo restores original order
    assert!(d.undo());
    for (r, (prio, id)) in rows.iter().enumerate() {
        assert_eq!(value(d.presentation(), "カスタム並替", r as u32, 0), CellValue::Text((*prio).into()));
        assert_eq!(value(d.presentation(), "カスタム並替", r as u32, 1), CellValue::Text((*id).into()));
    }

    // Redo restores custom list sorted order
    assert!(d.redo());
    for (r, (prio, id)) in expected.iter().enumerate() {
        assert_eq!(value(d.presentation(), "カスタム並替", r as u32, 0), CellValue::Text((*prio).into()));
        assert_eq!(value(d.presentation(), "カスタム並替", r as u32, 1), CellValue::Text((*id).into()));
    }
}

#[test]
fn calc_mode_roundtrip_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    assert_eq!(d.presentation().workbook.as_deref().unwrap().calc_mode, ibis_doc_model::CalcMode::Auto);

    // Switch to Manual mode
    d.execute(vec![Command::Sheet(SheetCommand::SetCalcMode { mode: ibis_doc_model::CalcMode::Manual })]).unwrap();
    let bytes = save(&opened, d.presentation(), "calc-mode-manual.xlsx");
    assert_reads_back(&bytes, d.presentation());
    let p = ibis_pptx::read(&bytes).unwrap();
    assert_eq!(p.workbook.as_deref().unwrap().calc_mode, ibis_doc_model::CalcMode::Manual);

    // Switch to AutoNoTable mode
    d.execute(vec![Command::Sheet(SheetCommand::SetCalcMode { mode: ibis_doc_model::CalcMode::AutoNoTable })]).unwrap();
    let bytes = save(&opened, d.presentation(), "calc-mode-auto-no-table.xlsx");
    assert_reads_back(&bytes, d.presentation());
    let p = ibis_pptx::read(&bytes).unwrap();
    assert_eq!(p.workbook.as_deref().unwrap().calc_mode, ibis_doc_model::CalcMode::AutoNoTable);

    // Switch back to Auto
    d.execute(vec![Command::Sheet(SheetCommand::SetCalcMode { mode: ibis_doc_model::CalcMode::Auto })]).unwrap();
    let bytes = save(&opened, d.presentation(), "calc-mode-auto.xlsx");
    assert_reads_back(&bytes, d.presentation());
    let p = ibis_pptx::read(&bytes).unwrap();
    assert_eq!(p.workbook.as_deref().unwrap().calc_mode, ibis_doc_model::CalcMode::Auto);
}

#[test]
fn precision_as_displayed_save_and_reopen_roundtrip() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    assert!(d.presentation().workbook.as_deref().unwrap().full_precision);

    // Turn off full_precision (precision as displayed)
    d.execute(vec![Command::Sheet(SheetCommand::SetFullPrecision { full_precision: false })]).unwrap();
    let bytes = save(&opened, d.presentation(), "precision-as-displayed.xlsx");
    assert_reads_back(&bytes, d.presentation());
    let p = ibis_pptx::read(&bytes).unwrap();
    assert!(!p.workbook.as_deref().unwrap().full_precision);

    // Turn full_precision back on
    let opened2 = ibis_pptx::open(bytes.clone()).unwrap();
    let mut d2 = Document::new(opened2.presentation.clone());
    d2.execute(vec![Command::Sheet(SheetCommand::SetFullPrecision { full_precision: true })]).unwrap();
    let bytes2 = save(&opened2, d2.presentation(), "full-precision-restored.xlsx");
    assert_reads_back(&bytes2, d2.presentation());
    let p2 = ibis_pptx::read(&bytes2).unwrap();
    assert!(p2.workbook.as_deref().unwrap().full_precision);
}

#[test]
fn iterative_calculation_save_and_reopen_roundtrip() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    assert!(!d.presentation().workbook.as_deref().unwrap().iterate);

    // Turn on iterative calculation (iterate: true, max_iterations: 50, max_change: 0.005)
    d.execute(vec![Command::Sheet(SheetCommand::SetIterativeCalculation {
        iterate: true,
        max_iterations: 50,
        max_change: 0.005.into(),
    })]).unwrap();
    let bytes = save(&opened, d.presentation(), "iterative-calculation.xlsx");
    assert_reads_back(&bytes, d.presentation());
    let p = ibis_pptx::read(&bytes).unwrap();
    let wb = p.workbook.as_deref().unwrap();
    assert!(wb.iterate);
    assert_eq!(wb.max_iterations, 50);
    assert!((wb.max_change.0 - 0.005).abs() < 1e-6);

    // Turn iterative calculation back off
    let opened2 = ibis_pptx::open(bytes.clone()).unwrap();
    let mut d2 = Document::new(opened2.presentation.clone());
    d2.execute(vec![Command::Sheet(SheetCommand::SetIterativeCalculation {
        iterate: false,
        max_iterations: 100,
        max_change: 0.001.into(),
    })]).unwrap();
    let bytes2 = save(&opened2, d2.presentation(), "iterative-off.xlsx");
    assert_reads_back(&bytes2, d2.presentation());
    let p2 = ibis_pptx::read(&bytes2).unwrap();
    assert!(!p2.workbook.as_deref().unwrap().iterate);
}

#[test]
fn named_formulas_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");

    // Define constant name TaxRate = "=0.08"
    d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "TaxRate".into(), sheet: None, formula: Some("=0.08".into()) })]).unwrap();
    // Define formula name EastRegionTotal = "SUM(月次売上!$B$4:$B$5)"
    d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "EastRegionTotal".into(), sheet: None, formula: Some("SUM(月次売上!$B$4:$B$5)".into()) })]).unwrap();
    // Define composite formula name TotalWithTax = "EastRegionTotal * (1 + TaxRate)"
    d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "TotalWithTax".into(), sheet: None, formula: Some("EastRegionTotal * (1 + TaxRate)".into()) })]).unwrap();

    // In cell G10 = =TotalWithTax
    set(&mut d, s, vec![(9, 6, CellInput::Formula("TotalWithTax".into()))]);
    // B4 is 1200000, B5 is 1030000 -> EastRegionTotal is 2230000
    // TotalWithTax is 2230000 * 1.08 = 2408400.0
    assert_eq!(cell(d.presentation(), "月次売上", 9, 6).value, CellValue::Number(2408400.0));

    // Save and reopen
    let bytes = save(&opened, d.presentation(), "named-formulas.xlsx");
    assert_reads_back(&bytes, d.presentation());
    let p = ibis_pptx::read(&bytes).unwrap();
    let names = &p.workbook.as_deref().unwrap().names;
    assert_eq!(names.iter().find(|n| n.name == "TaxRate").unwrap().formula, "0.08");
    assert_eq!(names.iter().find(|n| n.name == "EastRegionTotal").unwrap().formula, "SUM(月次売上!$B$4:$B$5)");
    assert_eq!(names.iter().find(|n| n.name == "TotalWithTax").unwrap().formula, "EastRegionTotal*(1+TaxRate)");
    assert_eq!(cell(&p, "月次売上", 9, 6).value, CellValue::Number(2408400.0));

    // Update TaxRate to 0.10, save and reopen
    d.execute(vec![Command::Sheet(SheetCommand::SetName { name: "TaxRate".into(), sheet: None, formula: Some("0.10".into()) })]).unwrap();
    assert_eq!(cell(d.presentation(), "月次売上", 9, 6).value, CellValue::Number(2453000.0));
    let bytes2 = save(&opened, d.presentation(), "named-formulas-updated.xlsx");
    assert_reads_back(&bytes2, d.presentation());
    let p2 = ibis_pptx::read(&bytes2).unwrap();
    assert_eq!(cell(&p2, "月次売上", 9, 6).value, CellValue::Number(2453000.0));
}

#[test]
fn scenarios_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");

    let sc1 = ibis_doc_model::SheetScenario {
        name: "通常売上".into(),
        comment: Some("ベースライン".into()),
        user: Some("ibis".into()),
        locked: false,
        hidden: false,
        input_cells: vec![
            ibis_doc_model::ScenarioInputCell { r: "B4".into(), val: "1200000".into(), num_fmt_id: None, deleted: false },
            ibis_doc_model::ScenarioInputCell { r: "B5".into(), val: "1030000".into(), num_fmt_id: None, deleted: false },
        ],
    };
    let sc2 = ibis_doc_model::SheetScenario {
        name: "好況売上".into(),
        comment: Some("好況時1.5倍".into()),
        user: Some("ibis".into()),
        locked: true,
        hidden: false,
        input_cells: vec![
            ibis_doc_model::ScenarioInputCell { r: "B4".into(), val: "1800000".into(), num_fmt_id: None, deleted: false },
            ibis_doc_model::ScenarioInputCell { r: "B5".into(), val: "1545000".into(), num_fmt_id: None, deleted: false },
        ],
    };

    // Add scenarios
    d.execute(vec![Command::Sheet(SheetCommand::AddScenario { slide: s, scenario: sc1 })]).unwrap();
    d.execute(vec![Command::Sheet(SheetCommand::AddScenario { slide: s, scenario: sc2 })]).unwrap();

    // Show scenario 1 (好況売上)
    d.execute(vec![Command::Sheet(SheetCommand::ShowScenario { slide: s, index: 1 })]).unwrap();
    assert_eq!(cell(d.presentation(), "月次売上", 3, 1).value, CellValue::Number(1800000.0));
    assert_eq!(cell(d.presentation(), "月次売上", 4, 1).value, CellValue::Number(1545000.0));

    // Save and reopen
    let bytes = save(&opened, d.presentation(), "scenarios.xlsx");
    assert_reads_back(&bytes, d.presentation());
    let p = ibis_pptx::read(&bytes).unwrap();
    let sh = sheet_of_slide(&p.slides[0]).unwrap();
    let scenarios = sh.scenarios.as_ref().expect("scenarios loaded");
    assert_eq!(scenarios.scenarios.len(), 2);
    assert_eq!(scenarios.scenarios[0].name, "通常売上");
    assert_eq!(scenarios.scenarios[0].comment.as_deref(), Some("ベースライン"));
    assert_eq!(scenarios.scenarios[1].name, "好況売上");
    assert!(scenarios.scenarios[1].locked);
    assert_eq!(scenarios.scenarios[1].input_cells[0].val, "1800000");

    // Check OOXML <scenarios> xml content
    let pkg = ibis_ooxml::Package::open(bytes).unwrap();
    let xml = pkg.xml("xl/worksheets/sheet1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let sc_node = xml.descendants(root).into_iter().find(|&n| xml.local_name(n) == "scenarios").expect("scenarios XML element");
    assert!(xml.descendants(sc_node).into_iter().any(|n| xml.local_name(n) == "scenario" && xml.attr(n, "name") == Some("好況売上")));
}

#[test]
fn subtotals_apply_and_save_roundtrip() {
    let (_, stock) = open("xlsx-sales.xlsx");
    let mut initial = Document::new(stock.presentation.clone());
    let (id, shape) = (initial.ids().new_slide_id(), initial.ids().new_shape_id());
    initial.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "小計テスト".into(), slide: id, shape })]).unwrap();

    set(&mut initial, id, vec![
        (0, 0, CellInput::Value(CellValue::Text("部門".into()))),
        (0, 1, CellInput::Value(CellValue::Text("金額".into()))),
        (1, 0, CellInput::Value(CellValue::Text("A".into()))),
        (1, 1, CellInput::Value(CellValue::Number(100.0))),
        (2, 0, CellInput::Value(CellValue::Text("A".into()))),
        (2, 1, CellInput::Value(CellValue::Number(200.0))),
        (3, 0, CellInput::Value(CellValue::Text("B".into()))),
        (3, 1, CellInput::Value(CellValue::Number(300.0))),
    ]);

    let params = ibis_doc_model::SubtotalParams {
        range: CellRange { first_row: 0, last_row: 3, first_column: 0, last_column: 1 },
        group_by_column: 0,
        function_code: 9, // SUM
        subtotal_columns: vec![1],
        replace_existing: true,
        summary_below: true,
        page_breaks: false,
    };
    initial.execute(vec![Command::Sheet(SheetCommand::ApplySubtotal { slide: id, params })]).unwrap();

    // Verify subtotal values
    assert_eq!(value(initial.presentation(), "小計テスト", 3, 0), CellValue::Text("A 集計".into()));
    assert_eq!(value(initial.presentation(), "小計テスト", 3, 1), CellValue::Number(300.0));
    assert_eq!(value(initial.presentation(), "小計テスト", 5, 0), CellValue::Text("B 集計".into()));
    assert_eq!(value(initial.presentation(), "小計テスト", 5, 1), CellValue::Number(300.0));
    assert_eq!(value(initial.presentation(), "小計テスト", 6, 0), CellValue::Text("総計".into()));
    assert_eq!(value(initial.presentation(), "小計テスト", 6, 1), CellValue::Number(600.0));

    // Save and reopen
    let bytes = save(&stock, initial.presentation(), "subtotals-roundtrip.xlsx");
    let p = ibis_pptx::read(&bytes).unwrap();
    assert_eq!(value(&p, "小計テスト", 3, 0), CellValue::Text("A 集計".into()));
    assert_eq!(value(&p, "小計テスト", 3, 1), CellValue::Number(300.0));
    assert_eq!(value(&p, "小計テスト", 6, 0), CellValue::Text("総計".into()));
    assert_eq!(value(&p, "小計テスト", 6, 1), CellValue::Number(600.0));

    let sh = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(sh.outline_rows.get(&3).copied(), Some(1));
    assert_eq!(sh.outline_rows.get(&5).copied(), Some(1));
}

#[test]
fn workbook_protection_save_and_reopen_roundtrip() {
    let (_, stock) = open("xlsx-sales.xlsx");
    let mut initial = Document::new(stock.presentation.clone());

    // 1. Initially unprotected
    assert!(initial.presentation().workbook.as_ref().unwrap().protection.is_none());

    // 2. Protect workbook with structure=true, windows=true, and password hash
    let hash = ibis_doc_model::PasswordHash {
        algorithm: "SHA-512".into(),
        hash: "testHashBase64==".into(),
        salt: "testSaltBase64==".into(),
        spin_count: 100000,
    };
    initial.execute(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
        protection: Some(ibis_doc_model::WorkbookProtection {
            lock_structure: true,
            lock_windows: true,
            lock_revision: false,
            hash: Some(hash),
            legacy: None,
        }),
    })]).unwrap();

    // 3. Save to bytes and verify XML
    let bytes = save(&stock, initial.presentation(), "workbook-protection.xlsx");
    let pkg = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let wb_xml = String::from_utf8(pkg.read("xl/workbook.xml").unwrap()).unwrap();
    assert!(wb_xml.contains("<workbookProtection"));
    assert!(wb_xml.contains("lockStructure=\"1\""));
    assert!(wb_xml.contains("lockWindows=\"1\""));
    assert!(wb_xml.contains("workbookAlgorithmName=\"SHA-512\""));
    assert!(wb_xml.contains("workbookHashValue=\"testHashBase64==\""));

    // 4. Reopen package and verify WorkbookProtection restored
    let reopened_opened = ibis_pptx::open(bytes.clone()).unwrap();
    let prot = reopened_opened.presentation.workbook.as_ref().unwrap().protection.as_ref().unwrap();
    assert!(prot.lock_structure);
    assert!(prot.lock_windows);
    assert_eq!(prot.hash.as_ref().unwrap().algorithm, "SHA-512");
    assert_eq!(prot.hash.as_ref().unwrap().hash, "testHashBase64==");
    assert_eq!(prot.hash.as_ref().unwrap().spin_count, 100000);

    // 5. Unprotect, save again, verify removed
    let mut doc2 = Document::new(reopened_opened.presentation.clone());
    doc2.execute(vec![Command::Sheet(SheetCommand::SetWorkbookProtection { protection: None })]).unwrap();
    let bytes2 = save(&reopened_opened, doc2.presentation(), "workbook-unprotected.xlsx");
    let pkg2 = ibis_ooxml::Package::open(bytes2.clone()).unwrap();
    let wb_xml2 = String::from_utf8(pkg2.read("xl/workbook.xml").unwrap()).unwrap();
    assert!(!wb_xml2.contains("<workbookProtection"));
    let reopened2 = ibis_pptx::open(bytes2).unwrap().presentation;
    assert!(reopened2.workbook.as_ref().unwrap().protection.is_none());
}

#[test]
fn protected_ranges_save_and_reopen_roundtrip() {
    let (_, stock) = open("xlsx-sales.xlsx");
    let mut initial = Document::new(stock.presentation.clone());
    let id = slide(initial.presentation(), "月次売上");

    // 1. Initially no protected ranges
    let s = initial.presentation().slide(id).unwrap();
    let sh = sheet_of_slide(s).unwrap();
    assert!(sh.protected_ranges.is_empty());

    // 2. Add protected range with B2:D10 and password hash
    let hash = ibis_doc_model::PasswordHash {
        algorithm: "SHA-512".into(),
        hash: "rangeHashBase64==".into(),
        salt: "rangeSaltBase64==".into(),
        spin_count: 100000,
    };
    let pr = ibis_doc_model::ProtectedRange {
        name: "AllowedRange1".into(),
        ranges: vec![CellRange { first_row: 1, first_column: 1, last_row: 9, last_column: 3 }],
        hash: Some(hash),
        legacy: None,
    };
    initial.execute(vec![Command::Sheet(SheetCommand::SetProtectedRanges {
        slide: id,
        ranges: vec![pr],
    })]).unwrap();

    // 3. Save to bytes and verify XML
    let bytes = save(&stock, initial.presentation(), "protected-ranges.xlsx");
    let pkg = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let sheet_xml = pkg.names().filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .filter_map(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .find(|t| t.contains("<protectedRanges>"))
        .expect("sheet xml contains <protectedRanges>");
    assert!(sheet_xml.contains("name=\"AllowedRange1\""), "contains name");
    assert!(sheet_xml.contains("sqref=\"B2:D10\""), "contains sqref B2:D10");
    assert!(sheet_xml.contains("algorithmName=\"SHA-512\""), "contains algorithmName");
    assert!(sheet_xml.contains("hashValue=\"rangeHashBase64==\""), "contains hashValue");

    // 4. Reopen package and verify ProtectedRange restored
    let reopened = ibis_pptx::open(bytes.clone()).unwrap();
    let s2 = reopened.presentation.slide(slide(&reopened.presentation, "月次売上")).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();
    assert_eq!(sh2.protected_ranges.len(), 1);
    assert_eq!(sh2.protected_ranges[0].name, "AllowedRange1");
    assert_eq!(sh2.protected_ranges[0].ranges, vec![CellRange { first_row: 1, first_column: 1, last_row: 9, last_column: 3 }]);
    assert_eq!(sh2.protected_ranges[0].hash.as_ref().unwrap().algorithm, "SHA-512");
    assert_eq!(sh2.protected_ranges[0].hash.as_ref().unwrap().hash, "rangeHashBase64==");

    // 5. Remove protected ranges, save again, verify removed
    let mut doc2 = Document::new(reopened.presentation.clone());
    doc2.execute(vec![Command::Sheet(SheetCommand::SetProtectedRanges { slide: id, ranges: Vec::new() })]).unwrap();
    let bytes2 = save(&reopened, doc2.presentation(), "protected-ranges-cleared.xlsx");
    let pkg2 = ibis_ooxml::Package::open(bytes2.clone()).unwrap();
    let any_has_pr = pkg2.names().filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .filter_map(|n| String::from_utf8(pkg2.read(n).unwrap()).ok())
        .any(|t| t.contains("<protectedRanges>"));
    assert!(!any_has_pr, "protectedRanges removed");
    let reopened2 = ibis_pptx::open(bytes2).unwrap().presentation;
    let s3 = reopened2.slide(slide(&reopened2, "月次売上")).unwrap();
    let sh3 = sheet_of_slide(s3).unwrap();
    assert!(sh3.protected_ranges.is_empty());
}

#[test]
fn pivot_options_save_and_reopen_roundtrip() {
    let (_data, opened) = open("xlsx-pivot-not-refreshed.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let id = opened.presentation.slides[0].id;
    let s = d.presentation().slide(id).unwrap();
    let mut sh = (**sheet_of_slide(s).unwrap()).clone();
    assert!(!sh.pivots.is_empty(), "has pivot table");

    // Modify pivot options
    sh.pivots[0].name = "売上ピボット".into();
    sh.pivots[0].merge_item = true;
    sh.pivots[0].auto_fit_columns = false;
    sh.pivots[0].preserve_formatting = false;
    sh.pivots[0].multiple_field_filters = true;
    sh.pivots[0].row_grand_totals = false;
    sh.pivots[0].col_grand_totals = true;
    sh.pivots[0].missing_caption = Some("(空白)".into());
    sh.pivots[0].error_caption = Some("ERR".into());

    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: id, sheet: Box::new(sh) })]).unwrap();
    let saved = save(&opened, d.presentation(), "pivot-options-roundtrip.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let pivot_xml = pkg.names().filter(|n| n.starts_with("xl/pivotTables/pivotTable") && n.ends_with(".xml"))
        .find_map(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("pivot table xml exists");

    assert!(pivot_xml.contains("name=\"売上ピボット\""), "xml has name");
    assert!(pivot_xml.contains("mergeItem=\"1\""), "xml has mergeItem");
    assert!(pivot_xml.contains("applyWidthHeightFormats=\"0\""), "xml has applyWidthHeightFormats 0");
    assert!(pivot_xml.contains("useAutoFormatting=\"0\""), "xml has useAutoFormatting 0");
    assert!(pivot_xml.contains("multipleFieldFilters=\"1\""), "xml has multipleFieldFilters 1");
    assert!(pivot_xml.contains("rowGrandTotals=\"0\""), "xml has rowGrandTotals 0");
    assert!(pivot_xml.contains("missingCaption=\"(空白)\""), "xml has missingCaption");
    assert!(pivot_xml.contains("errorCaption=\"ERR\""), "xml has errorCaption");

    // Reopen and verify restored in model
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let s2 = reopened.slide(id).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();
    let p2 = &sh2.pivots[0];
    assert_eq!(p2.name, "売上ピボット");
    assert!(p2.merge_item);
    assert!(!p2.auto_fit_columns);
    assert!(!p2.preserve_formatting);
    assert!(p2.multiple_field_filters);
    assert!(!p2.row_grand_totals);
    assert!(p2.col_grand_totals);
    assert_eq!(p2.missing_caption.as_deref(), Some("(空白)"));
    assert_eq!(p2.error_caption.as_deref(), Some("ERR"));
}

#[test]
fn pivot_calculated_field_save_and_reopen_roundtrip() {
    let (_data, opened) = open("xlsx-pivot-not-refreshed.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let id = opened.presentation.slides[0].id;
    let s = d.presentation().slide(id).unwrap();
    let mut sh = (**sheet_of_slide(s).unwrap()).clone();
    assert!(!sh.pivots.is_empty(), "has pivot table");

    // Add calculated field: 消費税 = 金額 * 0.1
    let idx = sh.pivots[0].add_calculated_field("消費税", "= 金額 * 0.1").expect("add calculated field");
    assert!(sh.pivots[0].is_calculated_field(idx));

    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: id, sheet: Box::new(sh) })]).unwrap();
    let saved = save(&opened, d.presentation(), "pivot-calc-field-roundtrip.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();

    // Verify cache XML contains formula
    let cache_xml = pkg.names().filter(|n| n.starts_with("xl/pivotCache/pivotCacheDefinition") && n.ends_with(".xml"))
        .find_map(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("pivot cache definition xml exists");

    assert!(cache_xml.contains("name=\"消費税\""), "cache xml has field name");
    assert!(cache_xml.contains("formula=\"金額 * 0.1\""), "cache xml has formula");

    // Reopen and verify restored in model
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let s2 = reopened.slide(id).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();
    let p2 = &sh2.pivots[0];
    let calcs = p2.calculated_fields();
    assert_eq!(calcs.len(), 1, "has one calculated field restored");
    assert_eq!(calcs[0].1, "消費税");
    assert_eq!(calcs[0].2, "金額 * 0.1");
}

#[test]
fn pivot_calculated_item_save_and_reopen_roundtrip() {
    let (_, opened) = open("xlsx-pivot-not-refreshed.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let id = slide(d.presentation(), "ピボット");
    let mut sheet = (**sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap()).clone();

    // Add calculated item "北日本" = "北海道" to field 2 (地域)
    let p = &mut sheet.pivots[0];
    p.add_calculated_item(2, "北日本", "= 北海道").expect("add calculated item");

    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: id, sheet: Box::new(sheet) })]).unwrap();

    let saved = save(&opened, d.presentation(), "pivot-calc-item-roundtrip.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();

    // Verify cache XML contains calculatedItems
    let cache_xml = pkg.names().filter(|n| n.starts_with("xl/pivotCache/pivotCacheDefinition") && n.ends_with(".xml"))
        .find_map(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("pivot cache definition xml exists");

    assert!(cache_xml.contains("<calculatedItems"), "cache xml has calculatedItems");
    assert!(cache_xml.contains("v=\"北日本\" f=\"1\""), "cache xml has item name");
    assert!(cache_xml.contains("formula=\"北海道\""), "cache xml has formula");

    // Reopen and verify restored in model
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let s2 = reopened.slide(id).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();
    let p2 = &sh2.pivots[0];
    let items = p2.calculated_items_for_field(2);
    assert_eq!(items.len(), 1, "has one calculated item restored");
    assert_eq!(items[0].0, "北日本");
    assert_eq!(items[0].1, "北海道");
}

#[test]
fn pivot_manual_grouping_save_and_reopen_roundtrip() {
    let (_, opened) = open("xlsx-pivot-not-refreshed.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let id = slide(d.presentation(), "ピボット");
    let mut sheet = (**sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap()).clone();

    // Group "関西" and "九州" in field 2 (地域)
    let p = &mut sheet.pivots[0];
    let g_name = p.group_manual(2, &["関西".into(), "九州".into()], None).expect("group manual");
    assert_eq!(g_name, "グループ1");

    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: id, sheet: Box::new(sheet) })]).unwrap();

    let saved = save(&opened, d.presentation(), "pivot-manual-group-roundtrip.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();

    // Verify cache XML contains discretePr and groupItems (ECMA-376 compliant)
    let cache_xml = pkg.names().filter(|n| n.starts_with("xl/pivotCache/pivotCacheDefinition") && n.ends_with(".xml"))
        .find_map(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("pivot cache definition xml exists");

    assert!(cache_xml.contains("<discretePr"), "cache xml has discretePr");
    assert!(cache_xml.contains("<groupItems"), "cache xml has groupItems");
    assert!(cache_xml.contains(r#"<s v="グループ1"/>"#), "cache xml has group name");

    // Reopen and verify restored in model
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let s2 = reopened.slide(id).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();
    let p2 = &sh2.pivots[0];
    let groups = p2.manual_groups_for_field(2).expect("has manual groups");
    assert_eq!(groups.get("関西").map(String::as_str), Some("グループ1"));
    assert_eq!(groups.get("九州").map(String::as_str), Some("グループ1"));
}

#[test]
fn pivot_sort_by_value_save_and_reopen_roundtrip() {
    let (_, opened) = open("xlsx-pivot-not-refreshed.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let id = slide(d.presentation(), "ピボット");
    let mut sheet = (**sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap()).clone();

    // Sort field 2 (地域) by dataField 0 (金額) descending
    let p = &mut sheet.pivots[0];
    p.sort_field_by_value(2, 0, false);
    assert_eq!(p.fields[2].sort, ibis_doc_model::PivotSort::Descending);
    assert_eq!(p.fields[2].sort_data_field, Some(0));

    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: id, sheet: Box::new(sheet) })]).unwrap();

    let saved = save(&opened, d.presentation(), "pivot-sort-by-val-roundtrip.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();

    // Verify pivotTable XML contains sortType="descending" and dataField="0"
    let pt_xml = pkg.names().filter(|n| n.starts_with("xl/pivotTables/pivotTable") && n.ends_with(".xml"))
        .find_map(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("pivot table xml exists");

    assert!(pt_xml.contains(r#"sortType="descending""#), "table xml has sortType descending");
    assert!(pt_xml.contains(r#"dataField="0""#), "table xml has dataField attribute");

    // Reopen and verify restored in model
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let s2 = reopened.slide(id).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();
    let p2 = &sh2.pivots[0];
    assert_eq!(p2.fields[2].sort, ibis_doc_model::PivotSort::Descending);
    assert_eq!(p2.fields[2].sort_data_field, Some(0));
}

#[test]
fn pivot_table_source_and_defined_name_source_roundtrip() {
    let (_data, opened) = open("xlsx-pivot-not-refreshed.xlsx");
    let mut d = Document::new(opened.presentation.clone());

    let pivot_slide_id = d.presentation().slides.iter().find(|s| {
        s.shapes.iter().any(|sh| if let ibis_doc_model::ShapeKind::Sheet(sheet) = &sh.kind { !sheet.pivots.is_empty() } else { false })
    }).unwrap().id;

    let s = d.presentation().slide(pivot_slide_id).unwrap();
    let pivot_sheet = (**sheet_of_slide(s).unwrap()).clone();
    let original_source = pivot_sheet.pivots[0].source.clone();

    let source_slide_id = if let Some(src_name) = &original_source.sheet {
        d.presentation().slides.iter().find(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(src_name))).unwrap().id
    } else {
        pivot_slide_id
    };
    let src_s = d.presentation().slide(source_slide_id).unwrap();
    let mut src_sheet = (**sheet_of_slide(src_s).unwrap()).clone();

    let table_range = original_source.range.unwrap_or(CellRange { first_row: 0, last_row: 10, first_column: 0, last_column: 3 });
    src_sheet.tables.push(ibis_doc_model::SheetTable {
        name: "MySalesTable".to_string(),
        range: table_range,
        header_rows: 1,
        totals_rows: 0,
        columns: vec!["A".into(), "B".into(), "C".into(), "D".into()],
        style: None,
        filter: None,
        totals: Vec::new(),
    });

    let mut wb = d.presentation().workbook.as_deref().cloned().unwrap_or_default();
    let src_sheet_name = d.presentation().slide(source_slide_id).and_then(|s| s.name.clone()).unwrap_or_else(|| "Sheet1".to_string());
    wb.names.push(ibis_doc_model::DefinedName {
        name: "MyNamedRange".to_string(),
        sheet: None,
        formula: format!("{}!$A${}:$D${}", src_sheet_name, table_range.first_row + 1, table_range.last_row + 1),
        hidden: false,
    });

    d.execute(vec![
        Command::Sheet(SheetCommand::ReplaceSheet { slide: source_slide_id, sheet: Box::new(src_sheet) }),
    ]).unwrap();
    let mut pres = d.presentation().clone();
    pres.workbook = Some(std::sync::Arc::new(wb));
    let mut d = Document::new(pres);

    // 1. Pivot with Table name source
    let s = d.presentation().slide(pivot_slide_id).unwrap();
    let mut pivot_sheet = (**sheet_of_slide(s).unwrap()).clone();
    pivot_sheet.pivots[0].source = ibis_doc_model::PivotSource {
        sheet: None,
        range: None,
        name: Some("MySalesTable".to_string()),
    };
    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: pivot_slide_id, sheet: Box::new(pivot_sheet) })]).unwrap();

    let data = ibis_pptx::pivot_data(d.presentation(), pivot_slide_id, 0);
    assert!(data.is_some(), "pivot_data should resolve data from table name");
    let data = data.unwrap();
    assert!(!data.headings.is_empty(), "data headings should not be empty");

    let saved = save(&opened, d.presentation(), "pivot-table-source-roundtrip.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let cache_xml = pkg.names().filter(|n| n.starts_with("xl/pivotCache/pivotCacheDefinition") && n.ends_with(".xml"))
        .find_map(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("pivot cache xml exists");

    assert!(cache_xml.contains(r#"name="MySalesTable""#), "cache xml has name attribute");
    assert!(!cache_xml.contains(r#"ref=""#), "cache xml should not have ref attribute when name is set");

    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let s2 = reopened.slide(pivot_slide_id).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();
    let p2 = &sh2.pivots[0];
    assert_eq!(p2.source.name.as_deref(), Some("MySalesTable"));

    let data2 = ibis_pptx::pivot_data(&reopened, pivot_slide_id, 0);
    assert!(data2.is_some(), "pivot_data should resolve on reopened document");

    // 2. Pivot with DefinedName source
    let mut d2 = Document::new(reopened);
    let s2 = d2.presentation().slide(pivot_slide_id).unwrap();
    let mut pivot_sheet2 = (**sheet_of_slide(s2).unwrap()).clone();
    pivot_sheet2.pivots[0].source = ibis_doc_model::PivotSource {
        sheet: None,
        range: None,
        name: Some("MyNamedRange".to_string()),
    };
    d2.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: pivot_slide_id, sheet: Box::new(pivot_sheet2) })]).unwrap();

    let data_dn = ibis_pptx::pivot_data(d2.presentation(), pivot_slide_id, 0);
    assert!(data_dn.is_some(), "pivot_data should resolve data from defined name");

    let saved_dn = save(&opened, d2.presentation(), "pivot-dn-source-roundtrip.xlsx");
    let pkg_dn = ibis_ooxml::Package::open(saved_dn.clone()).unwrap();
    let cache_xml_dn = pkg_dn.names().filter(|n| n.starts_with("xl/pivotCache/pivotCacheDefinition") && n.ends_with(".xml"))
        .find_map(|n| String::from_utf8(pkg_dn.read(n).unwrap()).ok())
        .expect("pivot cache xml exists");
    assert!(cache_xml_dn.contains(r#"name="MyNamedRange""#));

    let reopened_dn = ibis_pptx::open(saved_dn).unwrap().presentation;
    let data_dn2 = ibis_pptx::pivot_data(&reopened_dn, pivot_slide_id, 0);
    assert!(data_dn2.is_some(), "pivot_data should resolve for defined name on reopened document");
}

/// A sheet with a table: inserting and deleting rows and columns adjusts the
/// table's range, autoFilter, columns and formulas, saves to OOXML, and
/// reads back correctly.
#[test]
fn tables_with_row_and_column_inserts_and_deletes_save_and_reopen() {
    use ibis_doc_model::sheet_command::Axis;
    let (original, opened) = open("xlsx-table.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "表");

    // Initial state: Table1 at A1:C6 (0..=5, 0..=2)
    let initial_table = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap().tables[0].clone();
    assert_eq!(initial_table.range, CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 2 });
    assert_eq!(initial_table.columns.len(), 3);

    // 1. Insert 1 row at row 0 (above the table): table shifts to A2:C7 (1..=6, 0..=2)
    d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Rows, at: 0, count: 1 })]).unwrap();
    let saved1 = save(&opened, d.presentation(), "table-shift-row.xlsx");
    let p1 = ibis_pptx::read(&saved1).unwrap();
    let t1 = sheet_of_slide(p1.slide(slide(&p1, "表")).unwrap()).unwrap().tables[0].clone();
    assert_eq!(t1.range, CellRange { first_row: 1, last_row: 6, first_column: 0, last_column: 2 });
    assert_eq!(t1.columns.len(), 3);
    assert_eq!(t1.filter.as_ref().map(|f| f.range), Some(CellRange { first_row: 1, last_row: 5, first_column: 0, last_column: 2 }));
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);

    // 2. Insert 1 row at row 2 (inside data rows): table expands to A1:C7 (0..=6, 0..=2)
    d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Rows, at: 2, count: 1 })]).unwrap();
    let saved2 = save(&opened, d.presentation(), "table-expand-row.xlsx");
    let p2 = ibis_pptx::read(&saved2).unwrap();
    let t2 = sheet_of_slide(p2.slide(slide(&p2, "表")).unwrap()).unwrap().tables[0].clone();
    assert_eq!(t2.range, CellRange { first_row: 0, last_row: 6, first_column: 0, last_column: 2 });
    assert_eq!(t2.columns.len(), 3);
    assert_eq!(t2.filter.as_ref().map(|f| f.range), Some(CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 2 }));
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);

    // 3. Delete 1 row at row 2 (inside data rows): table shrinks to A1:C5 (0..=4, 0..=2)
    d.execute(vec![Command::Sheet(SheetCommand::Delete { slide: s, axis: Axis::Rows, at: 2, count: 1 })]).unwrap();
    let saved3 = save(&opened, d.presentation(), "table-shrink-row.xlsx");
    let p3 = ibis_pptx::read(&saved3).unwrap();
    let t3 = sheet_of_slide(p3.slide(slide(&p3, "表")).unwrap()).unwrap().tables[0].clone();
    assert_eq!(t3.range, CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 2 });
    assert_eq!(t3.columns.len(), 3);
    assert_eq!(t3.filter.as_ref().map(|f| f.range), Some(CellRange { first_row: 0, last_row: 3, first_column: 0, last_column: 2 }));
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);

    // 4. Insert 1 column at col 0 (before table): table shifts to B1:D6 (0..=5, 1..=3)
    d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Columns, at: 0, count: 1 })]).unwrap();
    let saved4 = save(&opened, d.presentation(), "table-shift-col.xlsx");
    let p4 = ibis_pptx::read(&saved4).unwrap();
    let t4 = sheet_of_slide(p4.slide(slide(&p4, "表")).unwrap()).unwrap().tables[0].clone();
    assert_eq!(t4.range, CellRange { first_row: 0, last_row: 5, first_column: 1, last_column: 3 });
    assert_eq!(t4.columns.len(), 3);
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);

    // 5. Insert 1 column at col 1 (inside table columns): table expands to A1:D6 (0..=5, 0..=3)
    d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Columns, at: 1, count: 1 })]).unwrap();
    let saved5 = save(&opened, d.presentation(), "table-expand-col.xlsx");
    let p5 = ibis_pptx::read(&saved5).unwrap();
    let t5 = sheet_of_slide(p5.slide(slide(&p5, "表")).unwrap()).unwrap().tables[0].clone();
    assert_eq!(t5.range, CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 3 });
    assert_eq!(t5.columns.len(), 4);
    assert_eq!(t5.columns[1], "列1");
    assert_eq!(value(&p5, "表", 0, 1), CellValue::Text("列1".into()));
    assert_eq!(value(d.presentation(), "表", 0, 1), CellValue::Text("列1".into()));
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);

    // 6. Delete 1 column at col 1 (inside table columns): table shrinks to A1:B6 (0..=5, 0..=1)
    d.execute(vec![Command::Sheet(SheetCommand::Delete { slide: s, axis: Axis::Columns, at: 1, count: 1 })]).unwrap();
    let saved6 = save(&opened, d.presentation(), "table-shrink-col.xlsx");
    let p6 = ibis_pptx::read(&saved6).unwrap();
    let t6 = sheet_of_slide(p6.slide(slide(&p6, "表")).unwrap()).unwrap().tables[0].clone();
    assert_eq!(t6.range, CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 1 });
    assert_eq!(t6.columns.len(), 2);
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);

    // 7. Delete all rows of the table (rows 0..=5): table is completely deleted
    d.execute(vec![Command::Sheet(SheetCommand::Delete { slide: s, axis: Axis::Rows, at: 0, count: 6 })]).unwrap();
    let saved7 = save(&opened, d.presentation(), "table-delete-all-rows.xlsx");
    let p7 = ibis_pptx::read(&saved7).unwrap();
    let t7 = sheet_of_slide(p7.slide(slide(&p7, "表")).unwrap()).unwrap().tables.clone();
    assert!(t7.is_empty(), "table should be removed when all rows deleted");
    let pkg7 = ibis_ooxml::Package::open(saved7).unwrap();
    assert!(!pkg7.names().any(|n| n.starts_with("xl/tables/")), "table part should be removed");
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original);
}
/// XLSX-FILE-017: ブックのプロパティ（docProps/core.xml, docProps/app.xml）の編集、
/// 保存、再読込、Undo/Redo 検証。
#[test]
fn test_workbook_doc_properties_edit_and_roundtrip() {
    use ibis_doc_model::DocProperties;

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());

    let mut props = DocProperties::default();
    props.title = Some("四半期売上報告書".to_string());
    props.creator = Some("アイビス営業部".to_string());
    props.subject = Some("売上実績".to_string());
    props.keywords = Some("売上, 2026, 会計".to_string());
    props.description = Some("社外秘データです".to_string());
    props.company = Some("株式会社アイビス".to_string());
    props.manager = Some("山田".to_string());
    props.category = Some("営業報告".to_string());

    // 1. コマンド実行
    d.execute(vec![Command::Sheet(SheetCommand::SetDocProperties(props.clone()))]).unwrap();
    assert_eq!(d.presentation().workbook.as_ref().unwrap().properties, props);

    // 2. 保存とファイル内容検証
    let saved = save(&opened, d.presentation(), "workbook-props.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();

    let core_xml = pkg.names().find(|n| n.ends_with("core.xml"))
        .and_then(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("core.xml exists");
    assert!(core_xml.contains("<dc:title>四半期売上報告書</dc:title>"));
    assert!(core_xml.contains("<dc:creator>アイビス営業部</dc:creator>"));
    assert!(core_xml.contains("<dc:subject>売上実績</dc:subject>"));
    assert!(core_xml.contains("<cp:keywords>売上, 2026, 会計</cp:keywords>"));
    assert!(core_xml.contains("<dc:description>社外秘データです</dc:description>"));
    assert!(core_xml.contains("<cp:category>営業報告</cp:category>"));

    let app_xml = pkg.names().find(|n| n.ends_with("app.xml"))
        .and_then(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("app.xml exists");
    assert!(app_xml.contains("<Company>株式会社アイビス</Company>"));
    assert!(app_xml.contains("<Manager>山田</Manager>"));

    // 3. 再読込での完全復元検証
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let reopened_props = &reopened.workbook.as_ref().unwrap().properties;
    assert_eq!(reopened_props.title.as_deref(), Some("四半期売上報告書"));
    assert_eq!(reopened_props.creator.as_deref(), Some("アイビス営業部"));
    assert_eq!(reopened_props.subject.as_deref(), Some("売上実績"));
    assert_eq!(reopened_props.keywords.as_deref(), Some("売上, 2026, 会計"));
    assert_eq!(reopened_props.description.as_deref(), Some("社外秘データです"));
    assert_eq!(reopened_props.company.as_deref(), Some("株式会社アイビス"));
    assert_eq!(reopened_props.manager.as_deref(), Some("山田"));
    assert_eq!(reopened_props.category.as_deref(), Some("営業報告"));

    // 4. Undo 検証
    assert!(d.undo());
    assert_ne!(d.presentation().workbook.as_ref().unwrap().properties, props);

    // 5. Redo 検証
    assert!(d.redo());
    assert_eq!(d.presentation().workbook.as_ref().unwrap().properties, props);
}
#[test]
fn chart_date_axis_save_and_reopen() {
    use ibis_doc_model::chart::{AxisKind, AxisSourceKind};
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let cat_idx = chart.axes.iter().position(|a| a.kind == AxisKind::Category && !a.deleted).unwrap();

    // 初期状態: date: false, source_kind: AxisSourceKind::Category
    assert_eq!(chart.axes[cat_idx].date, false);
    assert_eq!(chart.axes[cat_idx].source_kind, AxisSourceKind::Category);

    // 1. 日付軸（date: true, source_kind: AxisSourceKind::Date）に変更して保存
    let mut modified = chart.clone();
    modified.axes[cat_idx].date = true;
    modified.axes[cat_idx].source_kind = AxisSourceKind::Date;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    let bytes = save(&opened, d.presentation(), "chart-date-axis.xlsx");

    // XML に <c:dateAx> があり、<c:catAx> がないことを検証
    let package = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let chart_xml = package.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml.contains("<c:dateAx>"), "XML should contain <c:dateAx>: {chart_xml}");
    assert!(!chart_xml.contains("<c:catAx>"), "XML should not contain <c:catAx>: {chart_xml}");

    // 開き直してモデルを検証
    let read = ibis_pptx::open(bytes.clone()).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let read_cat = &read_chart.axes[cat_idx];
    assert_eq!(read_cat.date, true);
    assert_eq!(read_cat.source_kind, AxisSourceKind::Date);

    // 2. 再びテキスト軸（date: false, source_kind: AxisSourceKind::Category）に戻して保存
    let mut edited = read_chart.clone();
    edited.axes[cat_idx].date = false;
    edited.axes[cat_idx].source_kind = AxisSourceKind::Category;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();
    let bytes2 = save(&read, d.presentation(), "chart-text-axis.xlsx");

    let package2 = ibis_ooxml::Package::open(bytes2.clone()).unwrap();
    let chart_xml2 = package2.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package2.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml2.contains("<c:catAx>"), "XML should contain <c:catAx>: {chart_xml2}");
    assert!(!chart_xml2.contains("<c:dateAx>"), "XML should not contain <c:dateAx>: {chart_xml2}");

    let read2 = ibis_pptx::open(bytes2).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let read2_cat = &read2_chart.axes[cat_idx];
    assert_eq!(read2_cat.date, false);
    assert_eq!(read2_cat.source_kind, AxisSourceKind::Category);

    // 3. Undo 検証
    assert!(d.undo()); // date: false -> date: true
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart.axes[cat_idx].date, true);

    assert!(d.undo()); // date: true -> date: false (初期状態)
    let (_, undo2_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo2_chart.axes[cat_idx].date, false);
}

/// XLSX-FILE-018: カスタムプロパティ（docProps/custom.xml）の編集、
/// 保存、再読込、Undo/Redo 検証。
#[test]
fn test_workbook_custom_properties_edit_and_roundtrip() {
    use ibis_doc_model::{CustomProperty, CustomPropertyValue, DocProperties};

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());

    let mut props = DocProperties::default();
    props.title = Some("カスタムプロパティテスト".to_string());
    props.custom = vec![
        CustomProperty {
            name: "プロジェクト名".to_string(),
            value: CustomPropertyValue::Text("次世代オフィス".to_string()),
        },
        CustomProperty {
            name: "予算".to_string(),
            value: CustomPropertyValue::Number("12000000".to_string()),
        },
        CustomProperty {
            name: "承認済み".to_string(),
            value: CustomPropertyValue::Bool(true),
        },
        CustomProperty {
            name: "目標完了日".to_string(),
            value: CustomPropertyValue::DateTime("2026-12-31T00:00:00Z".to_string()),
        },
    ];

    // 1. コマンド実行
    d.execute(vec![Command::Sheet(SheetCommand::SetDocProperties(props.clone()))]).unwrap();
    assert_eq!(d.presentation().workbook.as_ref().unwrap().properties, props);

    // 2. 保存と XML 検証
    let saved = save(&opened, d.presentation(), "workbook-custom-props.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();

    let custom_xml = pkg.names().find(|n| n.ends_with("custom.xml"))
        .and_then(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("custom.xml exists");
    assert!(custom_xml.contains("name=\"プロジェクト名\""));
    assert!(custom_xml.contains("<vt:lpwstr>次世代オフィス</vt:lpwstr>"));
    assert!(custom_xml.contains("name=\"予算\""));
    assert!(custom_xml.contains("<vt:r8>12000000</vt:r8>"));
    assert!(custom_xml.contains("name=\"承認済み\""));
    assert!(custom_xml.contains("<vt:bool>true</vt:bool>"));
    assert!(custom_xml.contains("name=\"目標完了日\""));
    assert!(custom_xml.contains("<vt:filetime>2026-12-31T00:00:00Z</vt:filetime>"));

    // 3. 再読込での完全復元検証
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let reopened_props = &reopened.workbook.as_ref().unwrap().properties;
    assert_eq!(reopened_props.custom.len(), 4);
    assert_eq!(reopened_props.custom[0].name, "プロジェクト名");
    assert_eq!(reopened_props.custom[0].value, CustomPropertyValue::Text("次世代オフィス".to_string()));
    assert_eq!(reopened_props.custom[1].name, "予算");
    assert_eq!(reopened_props.custom[1].value, CustomPropertyValue::Number("12000000".to_string()));
    assert_eq!(reopened_props.custom[2].name, "承認済み");
    assert_eq!(reopened_props.custom[2].value, CustomPropertyValue::Bool(true));
    assert_eq!(reopened_props.custom[3].name, "目標完了日");
    assert_eq!(reopened_props.custom[3].value, CustomPropertyValue::DateTime("2026-12-31T00:00:00Z".to_string()));

    // 4. Undo 検証
    assert!(d.undo());
    assert_ne!(d.presentation().workbook.as_ref().unwrap().properties, props);

    // 5. Redo 検証
    assert!(d.redo());
    assert_eq!(d.presentation().workbook.as_ref().unwrap().properties, props);
}

#[test]
fn chart_secondary_and_series_axis_save_and_reopen() {
    use ibis_doc_model::chart::{AxisKind, ChartTitle, ChartText};
    use ibis_doc_model::{Number, Stroke, Color, Rgb};

    let (_, stock) = open("xlsx-sales.xlsx");
    let bytes = stock.source.save(&stock.presentation, SaveOptions::default()).unwrap();
    let mut package = ibis_ooxml::Package::open(bytes).unwrap();
    let mut xml = String::from_utf8(package.read("xl/charts/chart1.xml").unwrap()).unwrap();

    // Convert 2D barChart into two bar3DChart groups (one for primary valAx, one for secondary valAx)
    // with view3D, secondary valAx and serAx so that all axes are properly referenced and schema-valid for real Excel.
    let ser2_pos = xml.find(r#"<c:ser><c:idx val="2""#).unwrap();
    let ser2_end = xml[ser2_pos..].find("</c:ser>").unwrap() + ser2_pos + "</c:ser>".len();
    let ser2 = xml[ser2_pos..ser2_end].to_string();
    xml.replace_range(ser2_pos..ser2_end, "");

    xml = xml.replace("<c:barChart>", "<c:bar3DChart>");
    xml = xml.replace("</c:barChart>", "<c:axId val=\"1642411137\"/></c:bar3DChart>");

    let bar2 = format!(r#"<c:bar3DChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/>{ser2}<c:gapWidth val="219"/><c:gapDepth val="150"/><c:axId val="1658744191"/><c:axId val="1642411136"/><c:axId val="1642411137"/></c:bar3DChart>"#);
    xml = xml.replace("</c:bar3DChart>", &format!("</c:bar3DChart>{bar2}"));

    let view3d = r#"<c:view3D><c:rotX val="15"/><c:rotY val="20"/><c:rAngAx val="1"/></c:view3D>"#;
    xml = xml.replace("<c:plotArea>", &format!("{view3d}<c:plotArea>"));

    let extra_axes = r#"<c:valAx><c:axId val="1642411136"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="r"/><c:crossAx val="1658744191"/><c:crosses val="autoZero"/></c:valAx><c:serAx><c:axId val="1642411137"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="1658744191"/><c:crosses val="autoZero"/></c:serAx>"#;
    xml = xml.replace("</c:plotArea>", &format!("{extra_axes}</c:plotArea>"));
    package.set_bytes("xl/charts/chart1.xml", xml.into_bytes());
    let fixture_bytes = package.save(SaveOptions::default()).unwrap();

    let opened = ibis_pptx::open(fixture_bytes).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();

    let sec_val_idx = chart.axes.iter().position(|a| a.kind == AxisKind::Value && a.source_id == Some(1642411136)).expect("secondary value axis exists");
    let ser_idx = chart.axes.iter().position(|a| a.kind == AxisKind::Series && a.source_id == Some(1642411137)).expect("series axis exists");

    let mut modified = chart.clone();
    modified.axes[sec_val_idx].title = Some(ChartTitle {
        text: "第2軸売上".to_string(),
        style: ChartText::default(),
        at: None,
    });
    modified.axes[sec_val_idx].gridlines = Some(Stroke::solid(Color::opaque(Rgb(0xD9D9D9)), 9525));
    modified.axes[sec_val_idx].minor_gridlines = Some(Stroke::solid(Color::opaque(Rgb(0xEBEBEB)), 7143));
    modified.axes[sec_val_idx].reversed = true;
    modified.axes[sec_val_idx].log_base = Some(Number(10.0));
    modified.axes[sec_val_idx].display_unit = Some(Number(1000.0));

    modified.axes[ser_idx].title = Some(ChartTitle {
        text: "系列軸タイトル".to_string(),
        style: ChartText::default(),
        at: None,
    });
    modified.axes[ser_idx].gridlines = Some(Stroke::solid(Color::opaque(Rgb(0xD9D9D9)), 9525));
    modified.axes[ser_idx].reversed = true;

    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();

    let saved = save(&opened, d.presentation(), "chart-sec-and-ser-axis.xlsx");

    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let chart_xml = String::from_utf8(pkg.read("xl/charts/chart1.xml").unwrap()).unwrap();
    assert!(chart_xml.contains("第2軸売上"), "XML should contain secondary axis title: {chart_xml}");
    assert!(chart_xml.contains("系列軸タイトル"), "XML should contain series axis title: {chart_xml}");
    assert!(chart_xml.contains("<c:dispUnits>"), "XML should contain dispUnits: {chart_xml}");
    assert!(chart_xml.contains("logBase"), "XML should contain logBase: {chart_xml}");

    let reopened = ibis_pptx::open(saved).unwrap();
    let (_, reopened_chart) = reopened.presentation.slide(slide(&reopened.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();

    let re_sec = &reopened_chart.axes[sec_val_idx];
    assert_eq!(re_sec.title.as_ref().map(|t| t.text.as_str()), Some("第2軸売上"));
    assert!(re_sec.gridlines.is_some());
    assert!(re_sec.minor_gridlines.is_some());
    assert_eq!(re_sec.reversed, true);
    assert_eq!(re_sec.log_base, Some(Number(10.0)));
    assert_eq!(re_sec.display_unit, Some(Number(1000.0)));

    let re_ser = &reopened_chart.axes[ser_idx];
    assert_eq!(re_ser.title.as_ref().map(|t| t.text.as_str()), Some("系列軸タイトル"));
    assert!(re_ser.gridlines.is_some());
    assert_eq!(re_ser.reversed, true);

    assert!(d.undo());
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let undo_sec = &undo_chart.axes[sec_val_idx];
    assert!(undo_sec.title.is_none());
    assert!(undo_sec.gridlines.is_none());
    assert!(undo_sec.minor_gridlines.is_none());
    assert_eq!(undo_sec.reversed, false);
    assert_eq!(undo_sec.log_base, None);
    assert_eq!(undo_sec.display_unit, None);

    let undo_ser = &undo_chart.axes[ser_idx];
    assert!(undo_ser.title.is_none());
    assert!(undo_ser.gridlines.is_none());
    assert_eq!(undo_ser.reversed, false);

    assert!(d.redo());
    let (_, redo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(redo_chart.axes[sec_val_idx].title.as_ref().map(|t| t.text.as_str()), Some("第2軸売上"));
    assert_eq!(redo_chart.axes[ser_idx].title.as_ref().map(|t| t.text.as_str()), Some("系列軸タイトル"));
}

#[test]
fn test_sheet_shape_alt_text_edit_and_roundtrip() {
    use ibis_doc_model::AltText;

    let (_original, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());

    // シート1（グラフ）とシート2（画像）を取得
    let s0 = d.presentation().slides[0].id;
    let s1 = d.presentation().slides[1].id;

    let chart_shape = d.presentation().slide(s0).unwrap().shapes.iter().find(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_))).unwrap().id;
    let pic_shape = d.presentation().slide(s1).unwrap().shapes.iter().find(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_))).unwrap().id;

    // 1. 代替テキスト（タイトル・説明文）を設定
    let chart_alt = AltText {
        title: "売上グラフ".to_string(),
        description: "月次売上推移を表す棒グラフ".to_string(),
    };
    let pic_alt = AltText {
        title: "会社ロゴ".to_string(),
        description: "シート上部に配置された会社ロゴ画像".to_string(),
    };

    d.execute(vec![
        Command::SetShapeAltText {
            slide: s0,
            shape: chart_shape,
            alt_text: chart_alt.clone(),
        },
        Command::SetShapeAltText {
            slide: s1,
            shape: pic_shape,
            alt_text: pic_alt.clone(),
        },
    ]).unwrap();

    // 2. 保存して OOXML (drawing1.xml, drawing2.xml) を直接検証
    let saved1 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let pkg1 = ibis_ooxml::Package::open(saved1.clone()).unwrap();

    let d1_xml = pkg1.xml("xl/drawings/drawing1.xml").unwrap();
    let d1_root = d1_xml.root_element().unwrap();
    let c1 = d1_xml.descendants(d1_root).into_iter().find(|&n| d1_xml.is_element(n) && d1_xml.local_name(n) == "cNvPr" && d1_xml.attr(n, "id") == Some("2")).expect("drawing1 cNvPr");
    assert_eq!(d1_xml.attr(c1, "title"), Some("売上グラフ"));
    assert_eq!(d1_xml.attr(c1, "descr"), Some("月次売上推移を表す棒グラフ"));

    let d2_xml = pkg1.xml("xl/drawings/drawing2.xml").unwrap();
    let d2_root = d2_xml.root_element().unwrap();
    let c2 = d2_xml.descendants(d2_root).into_iter().find(|&n| d2_xml.is_element(n) && d2_xml.local_name(n) == "cNvPr" && d2_xml.attr(n, "id") == Some("3")).expect("drawing2 cNvPr");
    assert_eq!(d2_xml.attr(c2, "title"), Some("会社ロゴ"));
    assert_eq!(d2_xml.attr(c2, "descr"), Some("シート上部に配置された会社ロゴ画像"));

    // 3. 再読込して AltText が復元されることを検証
    let reopened1 = ibis_pptx::open(saved1.clone()).unwrap().presentation;
    assert_eq!(reopened1.slides[0].shape_alt_texts.get(&chart_shape), Some(&chart_alt));
    assert_eq!(reopened1.slides[1].shape_alt_texts.get(&pic_shape), Some(&pic_alt));

    // 4. 代替テキストを空にして保存 -> 属性が削除されることを検証
    d.execute(vec![
        Command::SetShapeAltText {
            slide: s0,
            shape: chart_shape,
            alt_text: AltText::default(),
        },
    ]).unwrap();

    let saved2 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let pkg2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let d1_xml2 = pkg2.xml("xl/drawings/drawing1.xml").unwrap();
    let d1_root2 = d1_xml2.root_element().unwrap();
    let c1_2 = d1_xml2.descendants(d1_root2).into_iter().find(|&n| d1_xml2.is_element(n) && d1_xml2.local_name(n) == "cNvPr" && d1_xml2.attr(n, "id") == Some("2")).expect("drawing1 cNvPr");
    assert_eq!(d1_xml2.attr(c1_2, "title"), None);
    assert_eq!(d1_xml2.attr(c1_2, "descr"), None);

    let reopened2 = ibis_pptx::open(saved2).unwrap().presentation;
    assert_eq!(reopened2.slides[0].shape_alt_texts.get(&chart_shape), None);

    // 5. Undo 検証
    assert!(d.undo()); // empty alt
    assert_eq!(d.presentation().slides[0].shape_alt_texts.get(&chart_shape), Some(&chart_alt));

    assert!(d.undo()); // initial set
    assert_eq!(d.presentation().slides[0].shape_alt_texts.get(&chart_shape), None);
    assert_eq!(d.presentation().slides[1].shape_alt_texts.get(&pic_shape), None);

    // 6. Redo 検証
    assert!(d.redo()); // restore initial set
    assert_eq!(d.presentation().slides[0].shape_alt_texts.get(&chart_shape), Some(&chart_alt));
    assert_eq!(d.presentation().slides[1].shape_alt_texts.get(&pic_shape), Some(&pic_alt));
}

#[test]
fn test_sheet_shape_placement_and_print_roundtrip() {
    use ibis_doc_model::{ObjectPlacement, ShapeKind};

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s0 = d.presentation().slides[0].id;
    let s1 = d.presentation().slides[1].id;

    let chart_shape = d.presentation().slide(s0).unwrap().shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Chart(_))).unwrap().id;
    let pic_shape = d.presentation().slide(s1).unwrap().shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Picture(_))).unwrap().id;

    // 1. Placement (配置) と Print (印刷) を変更して実行
    d.execute(vec![
        Command::SetShapePlacement {
            slide: s0,
            shape: chart_shape,
            placement: ObjectPlacement::MoveOnly,
        },
        Command::SetShapePrint {
            slide: s0,
            shape: chart_shape,
            print: false,
        },
        Command::SetShapePlacement {
            slide: s1,
            shape: pic_shape,
            placement: ObjectPlacement::Free,
        },
        Command::SetShapePrint {
            slide: s1,
            shape: pic_shape,
            print: true,
        },
    ]).unwrap();

    assert_eq!(d.presentation().slides[0].shape_placements.get(&chart_shape), Some(&ObjectPlacement::MoveOnly));
    assert_eq!(d.presentation().slides[0].shape_prints.get(&chart_shape), Some(&false));
    assert_eq!(d.presentation().slides[1].shape_placements.get(&pic_shape), Some(&ObjectPlacement::Free));
    assert_eq!(d.presentation().slides[1].shape_prints.get(&pic_shape), Some(&true));

    // 2. 保存して XML 検証
    let saved1 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let pkg1 = ibis_ooxml::Package::open(saved1.clone()).unwrap();

    let d1_xml = pkg1.xml("xl/drawings/drawing1.xml").unwrap();
    let d1_root = d1_xml.root_element().unwrap();
    let anchor1 = d1_xml.descendants(d1_root).into_iter().find(|&n| d1_xml.is_element(n) && matches!(d1_xml.local_name(n), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor")).expect("drawing1 anchor");
    assert_eq!(d1_xml.attr(anchor1, "editAs"), Some("oneCell"));
    let cd1 = d1_xml.children(anchor1).into_iter().copied().find(|&c| d1_xml.local_name(c) == "clientData").expect("drawing1 clientData");
    assert_eq!(d1_xml.attr(cd1, "fPrintsWithSheet"), Some("0"));

    let d2_xml = pkg1.xml("xl/drawings/drawing2.xml").unwrap();
    let d2_root = d2_xml.root_element().unwrap();
    let anchor2 = d2_xml.descendants(d2_root).into_iter().find(|&n| d2_xml.is_element(n) && matches!(d2_xml.local_name(n), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor")).expect("drawing2 anchor");
    assert_eq!(d2_xml.attr(anchor2, "editAs"), Some("absolute"));

    // 3. 再読込してモデル復元を検証
    let reopened1 = ibis_pptx::open(saved1).unwrap().presentation;
    assert_eq!(reopened1.slides[0].shape_placements.get(&chart_shape), Some(&ObjectPlacement::MoveOnly));
    assert_eq!(reopened1.slides[0].shape_prints.get(&chart_shape), Some(&false));
    assert_eq!(reopened1.slides[1].shape_placements.get(&pic_shape), Some(&ObjectPlacement::Free));
    assert_eq!(reopened1.slides[1].shape_prints.get(&pic_shape), Some(&true));

    // 4. Undo / Redo 検証
    assert!(d.undo());
    assert_eq!(d.presentation().slides[0].shape_placements.get(&chart_shape), Some(&ObjectPlacement::MoveAndSize));
    assert_eq!(d.presentation().slides[0].shape_prints.get(&chart_shape), Some(&true));

    assert!(d.redo());
    assert_eq!(d.presentation().slides[0].shape_placements.get(&chart_shape), Some(&ObjectPlacement::MoveOnly));
    assert_eq!(d.presentation().slides[0].shape_prints.get(&chart_shape), Some(&false));
}

#[test]
fn chart_series_fill_and_outline_save_and_reopen() {
    use ibis_doc_model::{Color, Fill, Rgb, Stroke};

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();

    // 1. 系列 0 の塗りつぶしを赤、枠線を黒 (28575 = 2.25pt) に変更して保存
    let mut modified = chart.clone();
    modified.groups[0].series[0].fill = Fill::Solid(Color::opaque(Rgb(0xC00000)));
    modified.groups[0].series[0].stroke = Some(Stroke::solid(Color::opaque(Rgb(0x000000)), 28_575));

    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();

    let saved = save(&opened, d.presentation(), "chart-series-fill-and-outline.xlsx");

    // XML 検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let chart_xml = String::from_utf8(pkg.read("xl/charts/chart1.xml").unwrap()).unwrap();
    assert!(chart_xml.contains("val=\"C00000\""), "XML should contain solidFill C00000: {chart_xml}");
    assert!(chart_xml.contains("w=\"28575\""), "XML should contain stroke width 28575: {chart_xml}");

    // 再読込してモデル検証
    let reopened = ibis_pptx::open(saved).unwrap();
    let (_, reopened_chart) = reopened.presentation.slide(slide(&reopened.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let ser0 = &reopened_chart.groups[0].series[0];
    assert_eq!(ser0.fill, Fill::Solid(Color::opaque(Rgb(0xC00000))));
    assert_eq!(ser0.stroke.as_ref().map(|st| st.color.rgb), Some(Rgb(0x000000)));
    assert_eq!(ser0.stroke.as_ref().map(|st| st.width), Some(28_575));

    // 2. 塗りつぶし「なし」(Fill::None)、枠線「なし」(None) に変更して保存
    let mut edited = reopened_chart.clone();
    edited.groups[0].series[0].fill = Fill::None;
    edited.groups[0].series[0].stroke = None;

    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(edited.clone()))) }]).unwrap();

    let saved2 = save(&reopened, d.presentation(), "chart-series-fill-none.xlsx");
    let pkg2 = ibis_ooxml::Package::open(saved2.clone()).unwrap();
    let chart_xml2 = String::from_utf8(pkg2.read("xl/charts/chart1.xml").unwrap()).unwrap();
    assert!(chart_xml2.contains("<a:noFill/>"), "XML should contain <a:noFill/>: {chart_xml2}");

    let reopened2 = ibis_pptx::open(saved2).unwrap();
    let (_, reopened2_chart) = reopened2.presentation.slide(slide(&reopened2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let re_ser0 = &reopened2_chart.groups[0].series[0];
    assert_eq!(re_ser0.fill, Fill::None);
    assert!(re_ser0.stroke.is_none());

    // 3. Undo 検証
    assert!(d.undo()); // None -> Red
    let (_, undo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo_chart.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0xC00000))));
    assert_eq!(undo_chart.groups[0].series[0].stroke.as_ref().map(|st| st.color.rgb), Some(Rgb(0x000000)));

    assert!(d.undo()); // Red -> Initial
    let (_, undo2_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(undo2_chart.groups[0].series[0].fill, chart.groups[0].series[0].fill);

    // 4. Redo 検証
    assert!(d.redo()); // Initial -> Red
    let (_, redo_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(redo_chart.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0xC00000))));

    assert!(d.redo()); // Red -> None
    let (_, redo2_chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(redo2_chart.groups[0].series[0].fill, Fill::None);
}

#[test]
fn formula_array_constant_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (id, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet {
        index: 1,
        name: "配列定数".into(),
        slide: id,
        shape,
    })]).unwrap();

    // 1. 配列定数および各種関数引数としての設定
    set(&mut d, id, vec![
        (0, 0, CellInput::Formula("{1, 2; 3, 4}".into())),
        (0, 3, CellInput::Formula("SUM({10, 20; 30, 40})".into())),
        (1, 3, CellInput::Formula("INDEX({100, 200; 300, 400}, 2, 2)".into())),
        (2, 3, CellInput::Formula("AVERAGE({2, 4, 6})".into())),
        (3, 3, CellInput::Formula("VLOOKUP(2, {1, \"A\"; 2, \"B\"; 3, \"C\"}, 2, FALSE)".into())),
    ]);

    // 実行結果・スピル・値の検証
    let sh = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    // A1 = {1, 2; 3, 4} が A1:B2 にスピル
    assert_eq!(sh.cell(0, 0).unwrap().value, CellValue::Number(1.0));
    assert_eq!(sh.cell(0, 1).unwrap().value, CellValue::Number(2.0));
    assert_eq!(sh.cell(1, 0).unwrap().value, CellValue::Number(3.0));
    assert_eq!(sh.cell(1, 1).unwrap().value, CellValue::Number(4.0));
    assert_eq!(sh.spills.get(&(0, 0)), Some(&ibis_doc_model::CellRange {
        first_row: 0,
        last_row: 1,
        first_column: 0,
        last_column: 1,
    }));

    // D1 = SUM({10, 20; 30, 40}) = 100
    assert_eq!(sh.cell(0, 3).unwrap().value, CellValue::Number(100.0));
    // D2 = INDEX({100, 200; 300, 400}, 2, 2) = 400
    assert_eq!(sh.cell(1, 3).unwrap().value, CellValue::Number(400.0));
    // D3 = AVERAGE({2, 4, 6}) = 4
    assert_eq!(sh.cell(2, 3).unwrap().value, CellValue::Number(4.0));
    // D4 = VLOOKUP(2, {1, "A"; 2, "B"; 3, "C"}, 2, FALSE) = "B"
    assert_eq!(sh.cell(3, 3).unwrap().value, CellValue::Text("B".into()));

    // 2. 保存と再読込
    let saved = save(&opened, d.presentation(), "formula-array-constants.xlsx");
    assert_reads_back(&saved, d.presentation());

    // OOXML XML検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet_xml = pkg.names()
        .filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .find_map(|n| {
            let s = String::from_utf8(pkg.read(n).unwrap()).ok()?;
            if s.contains("{1, 2; 3, 4}") || s.contains("{1,2;3,4}") {
                Some(s)
            } else {
                None
            }
        })
        .expect("Should find worksheet XML containing array constant formula");
    assert!(sheet_xml.contains("SUM({10, 20; 30, 40})") || sheet_xml.contains("SUM({10,20;30,40})"), "XML should contain SUM array formula: {sheet_xml}");

    // 再読込後のモデル検証
    let reopened = ibis_pptx::open(saved).unwrap();
    let re_sh = sheet_of_slide(reopened.presentation.slide(slide(&reopened.presentation, "配列定数")).unwrap()).unwrap();
    assert_eq!(re_sh.cell(0, 0).unwrap().value, CellValue::Number(1.0));
    assert_eq!(re_sh.cell(0, 1).unwrap().value, CellValue::Number(2.0));
    assert_eq!(re_sh.cell(1, 0).unwrap().value, CellValue::Number(3.0));
    assert_eq!(re_sh.cell(1, 1).unwrap().value, CellValue::Number(4.0));
    assert_eq!(re_sh.spills.get(&(0, 0)), Some(&ibis_doc_model::CellRange {
        first_row: 0,
        last_row: 1,
        first_column: 0,
        last_column: 1,
    }));
    assert_eq!(re_sh.cell(0, 3).unwrap().value, CellValue::Number(100.0));
    assert_eq!(re_sh.cell(1, 3).unwrap().value, CellValue::Number(400.0));
    assert_eq!(re_sh.cell(2, 3).unwrap().value, CellValue::Number(4.0));
    assert_eq!(re_sh.cell(3, 3).unwrap().value, CellValue::Text("B".into()));

    // 3. Undo / Redo 検証
    assert!(d.undo()); // SetCells を取り消し
    let undo_sh = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert!(undo_sh.cell(0, 0).is_none() || undo_sh.cell(0, 0).unwrap().value.is_empty());
    assert!(!undo_sh.spills.contains_key(&(0, 0)));

    assert!(d.redo()); // SetCells をやり直し
    let redo_sh = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(redo_sh.cell(0, 0).unwrap().value, CellValue::Number(1.0));
    assert_eq!(redo_sh.cell(1, 1).unwrap().value, CellValue::Number(4.0));
    assert_eq!(redo_sh.cell(0, 3).unwrap().value, CellValue::Number(100.0));
    assert_eq!(redo_sh.spills.get(&(0, 0)), Some(&ibis_doc_model::CellRange {
        first_row: 0,
        last_row: 1,
        first_column: 0,
        last_column: 1,
    }));
}

#[test]
fn pivot_field_layout_save_and_reopen_roundtrip() {
    use ibis_doc_model::PivotAxis;

    let (_data, opened) = open("xlsx-pivot-not-refreshed.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let id = opened.presentation.slides[0].id;
    let s = d.presentation().slide(id).unwrap();
    let mut sh = (**sheet_of_slide(s).unwrap()).clone();
    assert!(!sh.pivots.is_empty(), "has pivot table");

    let initial_p = sh.pivots[0].clone();

    // 1. フィールド配置の変更:
    // - 商品 (field 3) を行から列へ移動
    sh.pivots[0].place(3, Some(PivotAxis::Column), 0);
    // - 四半期 (field 1) を列からフィルター（ページ）へ移動
    sh.pivots[0].place(1, Some(PivotAxis::Page), 0);
    // - 商品 (field 3) を列から除外
    sh.pivots[0].place(3, None, 0);
    // - 年 (field 0) を行の先頭へ追加（行: [0, 2]）
    sh.pivots[0].place(0, Some(PivotAxis::Row), 0);
    // - 数量 (field 5) を値エリアに追加（値エリア: [金額(6), 数量(5)]）
    sh.pivots[0].place(5, Some(PivotAxis::Values), 1);

    assert_eq!(sh.pivots[0].row_fields, vec![0, 2]);
    // 値フィールドが2つのため、Excel仕様に準拠して Σ値(-2) が col_fields に自動配置される
    assert_eq!(sh.pivots[0].col_fields, vec![-2]);
    assert_eq!(sh.pivots[0].page_fields.len(), 1);
    assert_eq!(sh.pivots[0].page_fields[0].field, 1);
    assert_eq!(sh.pivots[0].data_fields.len(), 2);
    assert_eq!(sh.pivots[0].data_fields[0].field, 6);
    assert_eq!(sh.pivots[0].data_fields[1].field, 5);

    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: id, sheet: Box::new(sh) })]).unwrap();

    // 2. OOXML 保存と XML 検証
    let saved = save(&opened, d.presentation(), "pivot-field-layout-roundtrip.xlsx");
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let pivot_xml = pkg.names()
        .filter(|n| n.starts_with("xl/pivotTables/pivotTable") && n.ends_with(".xml"))
        .find_map(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("pivot table xml exists");

    assert!(pivot_xml.contains(r#"<rowFields count="2"><field x="0"/><field x="2"/></rowFields>"#), "xml has rowFields: {pivot_xml}");
    assert!(pivot_xml.contains(r#"<colFields count="1"><field x="-2"/></colFields>"#), "xml has colFields -2: {pivot_xml}");
    assert!(pivot_xml.contains(r#"<pageFields count="1"><pageField fld="1""#), "xml has pageFields: {pivot_xml}");
    assert!(pivot_xml.contains(r#"<dataFields count="2">"#), "xml has dataFields: {pivot_xml}");
    assert!(pivot_xml.contains(r#"fld="6""#) && pivot_xml.contains(r#"fld="5""#), "xml has dataField fields: {pivot_xml}");

    // 3. 再読込後のモデル検証
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let s2 = reopened.slide(id).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();
    let p2 = &sh2.pivots[0];

    assert_eq!(p2.row_fields, vec![0, 2]);
    assert_eq!(p2.col_fields, vec![-2]);
    assert_eq!(p2.page_fields.len(), 1);
    assert_eq!(p2.page_fields[0].field, 1);
    assert_eq!(p2.data_fields.len(), 2);
    assert_eq!(p2.data_fields[0].field, 6);
    assert_eq!(p2.data_fields[1].field, 5);

    // 4. Undo / Redo 検証
    assert!(d.undo());
    let undo_s = d.presentation().slide(id).unwrap();
    let undo_sh = sheet_of_slide(undo_s).unwrap();
    assert_eq!(undo_sh.pivots[0].row_fields, initial_p.row_fields);
    assert_eq!(undo_sh.pivots[0].col_fields, initial_p.col_fields);
    assert_eq!(undo_sh.pivots[0].page_fields.len(), initial_p.page_fields.len());
    assert_eq!(undo_sh.pivots[0].data_fields.len(), initial_p.data_fields.len());

    assert!(d.redo());
    let redo_s = d.presentation().slide(id).unwrap();
    let redo_sh = sheet_of_slide(redo_s).unwrap();
    assert_eq!(redo_sh.pivots[0].row_fields, vec![0, 2]);
    assert_eq!(redo_sh.pivots[0].col_fields, vec![-2]);
    assert_eq!(redo_sh.pivots[0].page_fields.len(), 1);
    assert_eq!(redo_sh.pivots[0].data_fields.len(), 2);
}

#[test]
fn formula_3d_reference_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());

    // 1. 3枚の連続するシートを追加
    let (id1, shape1) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    let (id2, shape2) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    let (id3, shape3) = (d.ids().new_slide_id(), d.ids().new_shape_id());

    d.execute(vec![
        Command::Sheet(SheetCommand::AddSheet { index: 1, name: "Data1".into(), slide: id1, shape: shape1 }),
        Command::Sheet(SheetCommand::AddSheet { index: 2, name: "Data2".into(), slide: id2, shape: shape2 }),
        Command::Sheet(SheetCommand::AddSheet { index: 3, name: "Data3".into(), slide: id3, shape: shape3 }),
    ]).unwrap();

    // 2. 各シートに値を設定
    set(&mut d, id1, vec![
        (0, 0, CellInput::Value(CellValue::Number(100.0))),
        (0, 1, CellInput::Value(CellValue::Number(10.0))),
    ]);
    set(&mut d, id2, vec![
        (0, 0, CellInput::Value(CellValue::Number(200.0))),
        (0, 1, CellInput::Value(CellValue::Number(20.0))),
    ]);
    set(&mut d, id3, vec![
        (0, 0, CellInput::Value(CellValue::Number(300.0))),
        (0, 1, CellInput::Value(CellValue::Number(30.0))),
    ]);

    // 3. Data1 に 3D 参照数式を設定
    set(&mut d, id1, vec![
        (0, 2, CellInput::Formula("SUM(Data1:Data3!A1)".into())),
        (1, 2, CellInput::Formula("AVERAGE(Data1:Data3!A1)".into())),
        (2, 2, CellInput::Formula("MAX(Data1:Data3!A1)".into())),
        (3, 2, CellInput::Formula("MIN(Data1:Data3!A1)".into())),
        (4, 2, CellInput::Formula("SUM(Data1:Data3!A1:B1)".into())),
    ]);

    // 計算値の検証
    let sh1 = sheet_of_slide(d.presentation().slide(id1).unwrap()).unwrap();
    assert_eq!(sh1.cell(0, 2).unwrap().value, CellValue::Number(600.0), "SUM(Data1:Data3!A1) = 600");
    assert_eq!(sh1.cell(1, 2).unwrap().value, CellValue::Number(200.0), "AVERAGE(Data1:Data3!A1) = 200");
    assert_eq!(sh1.cell(2, 2).unwrap().value, CellValue::Number(300.0), "MAX(Data1:Data3!A1) = 300");
    assert_eq!(sh1.cell(3, 2).unwrap().value, CellValue::Number(100.0), "MIN(Data1:Data3!A1) = 100");
    assert_eq!(sh1.cell(4, 2).unwrap().value, CellValue::Number(660.0), "SUM(Data1:Data3!A1:B1) = 660");

    // 4. 保存と XML 検証
    let saved = save(&opened, d.presentation(), "formula-3d-reference.xlsx");
    assert_reads_back(&saved, d.presentation());

    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet_xml = pkg.names()
        .filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .find_map(|n| {
            let s = String::from_utf8(pkg.read(n).unwrap()).ok()?;
            if s.contains("SUM(Data1:Data3!A1)") {
                Some(s)
            } else {
                None
            }
        })
        .expect("Should find worksheet XML containing 3D reference formula");
    assert!(sheet_xml.contains("AVERAGE(Data1:Data3!A1)"), "xml has AVERAGE: {sheet_xml}");
    assert!(sheet_xml.contains("SUM(Data1:Data3!A1:B1)"), "xml has range 3D: {sheet_xml}");

    // 5. 再読込後のモデル検証
    let reopened = ibis_pptx::open(saved).unwrap();
    let re_id1 = slide(&reopened.presentation, "Data1");
    let re_sh1 = sheet_of_slide(reopened.presentation.slide(re_id1).unwrap()).unwrap();
    assert_eq!(re_sh1.cell(0, 2).unwrap().value, CellValue::Number(600.0));
    assert_eq!(re_sh1.cell(1, 2).unwrap().value, CellValue::Number(200.0));
    assert_eq!(re_sh1.cell(2, 2).unwrap().value, CellValue::Number(300.0));
    assert_eq!(re_sh1.cell(3, 2).unwrap().value, CellValue::Number(100.0));
    assert_eq!(re_sh1.cell(4, 2).unwrap().value, CellValue::Number(660.0));

    // 6. カスケード再計算と Undo / Redo 検証:
    // Data2 の A1 を 200 から 500 に変更
    set(&mut d, id2, vec![(0, 0, CellInput::Value(CellValue::Number(500.0)))]);
    let sh1_after = sheet_of_slide(d.presentation().slide(id1).unwrap()).unwrap();
    assert_eq!(sh1_after.cell(0, 2).unwrap().value, CellValue::Number(900.0), "C1 cascades from 600 to 900");

    assert!(d.undo()); // Data2 の A1 変更を取り消し
    let sh1_undo = sheet_of_slide(d.presentation().slide(id1).unwrap()).unwrap();
    assert_eq!(sh1_undo.cell(0, 2).unwrap().value, CellValue::Number(600.0), "C1 reverts to 600");

    assert!(d.redo()); // Data2 の A1 変更をやり直し
    let sh1_redo = sheet_of_slide(d.presentation().slide(id1).unwrap()).unwrap();
    assert_eq!(sh1_redo.cell(0, 2).unwrap().value, CellValue::Number(900.0), "C1 redos to 900");
}

#[test]
fn formula_operator_precedence_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());

    // 1. 新規シート「Precedence」を追加
    let (id, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "Precedence".into(), slide: id, shape })]).unwrap();

    // 2. 参照用セルを設定: A1=10, B1=2, C1=3
    set(&mut d, id, vec![
        (0, 0, CellInput::Value(CellValue::Number(10.0))),
        (0, 1, CellInput::Value(CellValue::Number(2.0))),
        (0, 2, CellInput::Value(CellValue::Number(3.0))),
    ]);

    // 3. 多彩な演算子優先順位の数式を設定 (D列: column index 3)
    set(&mut d, id, vec![
        // 四則演算と括弧
        (0, 3, CellInput::Formula("1+2*3".into())),
        (1, 3, CellInput::Formula("(1+2)*3".into())),
        (2, 3, CellInput::Formula("10-4/2".into())),
        // べき乗と符号
        (3, 3, CellInput::Formula("2*3^2".into())),
        (4, 3, CellInput::Formula("-2^2".into())),
        (5, 3, CellInput::Formula("-(2^2)".into())),
        (6, 3, CellInput::Formula("2^3^2".into())),
        // パーセント
        (7, 3, CellInput::Formula("50%*200".into())),
        (8, 3, CellInput::Formula("10+50%".into())),
        // 文字列連結
        (9, 3, CellInput::Formula("1+2&3+4".into())),
        // 比較演算子
        (10, 3, CellInput::Formula("1+2=3".into())),
        (11, 3, CellInput::Formula("1+2*3=7".into())),
        (12, 3, CellInput::Formula("1&2=\"12\"".into())),
        (13, 3, CellInput::Formula("1+2<2*2".into())),
        // セル参照連動
        (14, 3, CellInput::Formula("A1+B1*C1".into())),
        (15, 3, CellInput::Formula("(A1+B1)*C1".into())),
        (16, 3, CellInput::Formula("-B1^2".into())),
        (17, 3, CellInput::Formula("A1&B1".into())),
        (18, 3, CellInput::Formula("A1>B1*C1".into())),
    ]);

    // 計算値の検証
    let sh = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh.cell(0, 3).unwrap().value, CellValue::Number(7.0), "1+2*3 = 7");
    assert_eq!(sh.cell(1, 3).unwrap().value, CellValue::Number(9.0), "(1+2)*3 = 9");
    assert_eq!(sh.cell(2, 3).unwrap().value, CellValue::Number(8.0), "10-4/2 = 8");
    assert_eq!(sh.cell(3, 3).unwrap().value, CellValue::Number(18.0), "2*3^2 = 18");
    assert_eq!(sh.cell(4, 3).unwrap().value, CellValue::Number(4.0), "-2^2 = 4");
    assert_eq!(sh.cell(5, 3).unwrap().value, CellValue::Number(-4.0), "-(2^2) = -4");
    assert_eq!(sh.cell(6, 3).unwrap().value, CellValue::Number(64.0), "2^3^2 = 64");
    assert_eq!(sh.cell(7, 3).unwrap().value, CellValue::Number(100.0), "50%*200 = 100");
    assert_eq!(sh.cell(8, 3).unwrap().value, CellValue::Number(10.5), "10+50% = 10.5");
    assert_eq!(sh.cell(9, 3).unwrap().value, CellValue::Text("37".into()), "1+2&3+4 = '37'");
    assert_eq!(sh.cell(10, 3).unwrap().value, CellValue::Bool(true), "1+2=3 = true");
    assert_eq!(sh.cell(11, 3).unwrap().value, CellValue::Bool(true), "1+2*3=7 = true");
    assert_eq!(sh.cell(12, 3).unwrap().value, CellValue::Bool(true), "1&2='12' = true");
    assert_eq!(sh.cell(13, 3).unwrap().value, CellValue::Bool(true), "1+2<2*2 = true");
    assert_eq!(sh.cell(14, 3).unwrap().value, CellValue::Number(16.0), "A1+B1*C1 = 16");
    assert_eq!(sh.cell(15, 3).unwrap().value, CellValue::Number(36.0), "(A1+B1)*C1 = 36");
    assert_eq!(sh.cell(16, 3).unwrap().value, CellValue::Number(4.0), "-B1^2 = 4");
    assert_eq!(sh.cell(17, 3).unwrap().value, CellValue::Text("102".into()), "A1&B1 = '102'");
    assert_eq!(sh.cell(18, 3).unwrap().value, CellValue::Bool(true), "A1>B1*C1 = true");

    // 4. 保存と XML 検証
    let saved = save(&opened, d.presentation(), "formula-operator-precedence.xlsx");
    assert_reads_back(&saved, d.presentation());

    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet_xml = pkg.names()
        .filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .find_map(|n| {
            let s = String::from_utf8(pkg.read(n).unwrap()).ok()?;
            if s.contains("1+2*3") {
                Some(s)
            } else {
                None
            }
        })
        .expect("Should find worksheet XML containing precedence formulas");
    assert!(sheet_xml.contains("-2^2"), "xml contains -2^2: {sheet_xml}");
    assert!(sheet_xml.contains("2^3^2"), "xml contains 2^3^2: {sheet_xml}");

    // 5. 再読込後のモデル検証
    let reopened = ibis_pptx::open(saved).unwrap();
    let re_id = slide(&reopened.presentation, "Precedence");
    let re_sh = sheet_of_slide(reopened.presentation.slide(re_id).unwrap()).unwrap();
    assert_eq!(re_sh.cell(0, 3).unwrap().value, CellValue::Number(7.0));
    assert_eq!(re_sh.cell(4, 3).unwrap().value, CellValue::Number(4.0));
    assert_eq!(re_sh.cell(6, 3).unwrap().value, CellValue::Number(64.0));
    assert_eq!(re_sh.cell(14, 3).unwrap().value, CellValue::Number(16.0));
    assert_eq!(re_sh.cell(16, 3).unwrap().value, CellValue::Number(4.0));
    assert_eq!(re_sh.cell(18, 3).unwrap().value, CellValue::Bool(true));

    // 6. カスケード再計算と Undo / Redo 検証:
    // B1 を 2 から 5 に変更
    set(&mut d, id, vec![(0, 1, CellInput::Value(CellValue::Number(5.0)))]);
    let sh_after = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    // A1+B1*C1 = 10 + 5*3 = 25
    assert_eq!(sh_after.cell(14, 3).unwrap().value, CellValue::Number(25.0), "D15 cascades to 25");
    // (A1+B1)*C1 = (10+5)*3 = 45
    assert_eq!(sh_after.cell(15, 3).unwrap().value, CellValue::Number(45.0), "D16 cascades to 45");
    // -B1^2 = (-5)^2 = 25
    assert_eq!(sh_after.cell(16, 3).unwrap().value, CellValue::Number(25.0), "D17 cascades to 25");
    // A1>B1*C1 = 10 > 15 = false
    assert_eq!(sh_after.cell(18, 3).unwrap().value, CellValue::Bool(false), "D19 cascades to false");

    // Undo
    assert!(d.undo());
    let sh_undo = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_undo.cell(14, 3).unwrap().value, CellValue::Number(16.0), "D15 reverts to 16");
    assert_eq!(sh_undo.cell(16, 3).unwrap().value, CellValue::Number(4.0), "D17 reverts to 4");
    assert_eq!(sh_undo.cell(18, 3).unwrap().value, CellValue::Bool(true), "D19 reverts to true");

    // Redo
    assert!(d.redo());
    let sh_redo = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_redo.cell(14, 3).unwrap().value, CellValue::Number(25.0), "D15 redos to 25");
    assert_eq!(sh_redo.cell(16, 3).unwrap().value, CellValue::Number(25.0), "D17 redos to 25");
    assert_eq!(sh_redo.cell(18, 3).unwrap().value, CellValue::Bool(false), "D19 redos to false");
}

/// XLSX-FILE-007: .xltx テンプレート保存と ContentType 設定、および再読込の検証
#[test]
fn test_save_template_xltx() {
    assert!(ibis_pptx::is_template("xltx"));
    assert!(ibis_pptx::is_template("XLTM"));
    assert_eq!(ibis_pptx::Kind::from_extension("xltx"), Some(ibis_pptx::Kind::Workbook));
    assert_eq!(ibis_pptx::Kind::from_extension("xltm"), Some(ibis_pptx::Kind::Workbook));
    assert_eq!(ibis_pptx::Kind::from_extension("xlk"), Some(ibis_pptx::Kind::Workbook));
    assert_eq!(ibis_pptx::Kind::from_extension("XLK"), Some(ibis_pptx::Kind::Workbook));

    let (_, opened) = open("xlsx-formats.xlsx");
    assert_eq!(ibis_pptx::Kind::of(&opened.presentation), ibis_pptx::Kind::Workbook);

    let saved = opened.source.save_template(&opened.presentation, SaveOptions::default()).expect("save_template succeeds");

    // [Content_Types].xml を検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("valid ooxml package");
    let ct_bytes = pkg.read("[Content_Types].xml").expect("[Content_Types].xml exists");
    let ct_xml = String::from_utf8(ct_bytes).unwrap();

    assert!(
        ct_xml.contains("application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml"),
        "Content_Types must have spreadsheetml.template: {ct_xml}"
    );
    assert!(
        !ct_xml.contains("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"),
        "Content_Types must replace sheet.main with template.main: {ct_xml}"
    );

    // 再読込して Workbook が復元されることを検証
    let reloaded = ibis_pptx::read(&saved).expect("read xltx succeeds");
    assert!(reloaded.workbook.is_some(), "reloaded must be a workbook");
    assert_eq!(reloaded.slides.len(), opened.presentation.slides.len());
}

#[test]
fn formula_implicit_intersection_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());

    // 1. 新規シート「Implicit」を追加
    let (id, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "Implicit".into(), slide: id, shape })]).unwrap();

    // 2. データセルの設定
    // 縦方向データ A1:A5 (10, 20, 30, 40, 50)
    // 横方向データ C1:E1 (100, 200, 300)
    set(&mut d, id, vec![
        (0, 0, CellInput::Value(CellValue::Number(10.0))),
        (1, 0, CellInput::Value(CellValue::Number(20.0))),
        (2, 0, CellInput::Value(CellValue::Number(30.0))),
        (3, 0, CellInput::Value(CellValue::Number(40.0))),
        (4, 0, CellInput::Value(CellValue::Number(50.0))),
        (0, 2, CellInput::Value(CellValue::Number(100.0))),
        (0, 3, CellInput::Value(CellValue::Number(200.0))),
        (0, 4, CellInput::Value(CellValue::Number(300.0))),
    ]);

    // 3. 暗黙の交差数式を設定
    set(&mut d, id, vec![
        // 列範囲の交差 (B列: column 1)
        (0, 1, CellInput::Formula("@A1:A5".into())),
        (1, 1, CellInput::Formula("@A1:A5".into())),
        (2, 1, CellInput::Formula("@A1:A5".into())),
        (4, 1, CellInput::Formula("@A1:A5".into())),
        (9, 1, CellInput::Formula("@A1:A5".into())), // 10行目は交差なし -> #VALUE!
        // 行範囲の交差 (3行目: row 2)
        (2, 2, CellInput::Formula("@C1:E1".into())),
        (2, 3, CellInput::Formula("@C1:E1".into())),
        (2, 4, CellInput::Formula("@C1:E1".into())),
        (2, 5, CellInput::Formula("@C1:E1".into())), // F列は交差なし -> #VALUE!
        // 配列定数 (スピル抑制・先頭要素)
        (0, 6, CellInput::Formula("@{1000, 2000; 3000, 4000}".into())),
        // 算術連動
        (1, 7, CellInput::Formula("10+@A1:A5".into())),
        (2, 7, CellInput::Formula("@A1:A5*2".into())),
    ]);

    // 計算値の検証
    let sh = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh.cell(0, 1).unwrap().value, CellValue::Number(10.0), "B1: @A1:A5 row 0 = 10");
    assert_eq!(sh.cell(1, 1).unwrap().value, CellValue::Number(20.0), "B2: @A1:A5 row 1 = 20");
    assert_eq!(sh.cell(2, 1).unwrap().value, CellValue::Number(30.0), "B3: @A1:A5 row 2 = 30");
    assert_eq!(sh.cell(4, 1).unwrap().value, CellValue::Number(50.0), "B5: @A1:A5 row 4 = 50");
    assert_eq!(sh.cell(9, 1).unwrap().value, CellValue::Error(ibis_doc_model::CellError::Value), "B10: no intersection -> #VALUE!");

    assert_eq!(sh.cell(2, 2).unwrap().value, CellValue::Number(100.0), "C3: @C1:E1 col 2 = 100");
    assert_eq!(sh.cell(2, 3).unwrap().value, CellValue::Number(200.0), "D3: @C1:E1 col 3 = 200");
    assert_eq!(sh.cell(2, 4).unwrap().value, CellValue::Number(300.0), "E3: @C1:E1 col 4 = 300");
    assert_eq!(sh.cell(2, 5).unwrap().value, CellValue::Error(ibis_doc_model::CellError::Value), "F3: no intersection -> #VALUE!");

    assert_eq!(sh.cell(0, 6).unwrap().value, CellValue::Number(1000.0), "G1: @array = 1000");
    assert_eq!(sh.cell(1, 7).unwrap().value, CellValue::Number(30.0), "H2: 10+@A1:A5 = 30");
    assert_eq!(sh.cell(2, 7).unwrap().value, CellValue::Number(60.0), "H3: @A1:A5*2 = 60");

    // 4. 保存と XML 検証 (_xlfn.SINGLE として保存されること)
    let saved = save(&opened, d.presentation(), "formula-implicit-intersection.xlsx");
    assert_reads_back(&saved, d.presentation());

    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet_xml = pkg.names()
        .filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .find_map(|n| {
            let s = String::from_utf8(pkg.read(n).unwrap()).ok()?;
            if s.contains("SINGLE(A1:A5)") {
                Some(s)
            } else {
                None
            }
        })
        .expect("Should find worksheet XML containing SINGLE formula");
    assert!(sheet_xml.contains("_xlfn.SINGLE(A1:A5)"), "xml has _xlfn.SINGLE: {sheet_xml}");
    assert!(sheet_xml.contains("_xlfn.SINGLE(C1:E1)"), "xml has _xlfn.SINGLE for row: {sheet_xml}");

    // 5. 再読込後のモデル検証 (@ として復元され、値も一致すること)
    let reopened = ibis_pptx::open(saved).unwrap();
    let re_id = slide(&reopened.presentation, "Implicit");
    let re_sh = sheet_of_slide(reopened.presentation.slide(re_id).unwrap()).unwrap();
    assert_eq!(re_sh.cell(0, 1).unwrap().formula.as_deref(), Some("@A1:A5"));
    assert_eq!(re_sh.cell(0, 1).unwrap().value, CellValue::Number(10.0));
    assert_eq!(re_sh.cell(1, 1).unwrap().value, CellValue::Number(20.0));
    assert_eq!(re_sh.cell(2, 2).unwrap().formula.as_deref(), Some("@C1:E1"));
    assert_eq!(re_sh.cell(2, 2).unwrap().value, CellValue::Number(100.0));
    assert_eq!(re_sh.cell(0, 6).unwrap().value, CellValue::Number(1000.0));

    // 6. カスケード再計算と Undo / Redo 検証:
    // A2 を 20 から 99 に変更
    set(&mut d, id, vec![(1, 0, CellInput::Value(CellValue::Number(99.0)))]);
    let sh_after = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_after.cell(1, 1).unwrap().value, CellValue::Number(99.0), "B2 cascades to 99");
    assert_eq!(sh_after.cell(1, 7).unwrap().value, CellValue::Number(109.0), "H2 cascades to 109");

    // Undo
    assert!(d.undo());
    let sh_undo = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_undo.cell(1, 1).unwrap().value, CellValue::Number(20.0), "B2 reverts to 20");
    assert_eq!(sh_undo.cell(1, 7).unwrap().value, CellValue::Number(30.0), "H2 reverts to 30");

    // Redo
    assert!(d.redo());
    let sh_redo = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_redo.cell(1, 1).unwrap().value, CellValue::Number(99.0), "B2 redos to 99");
    assert_eq!(sh_redo.cell(1, 7).unwrap().value, CellValue::Number(109.0), "H2 redos to 109");
}

/// XLSX-CELL-003: 15-digit precision rule for long numbers, zero-padding beyond 15 digits,
/// preserving quoted text strings, cascade recalculation, OOXML save and reopen roundtrip, Undo/Redo.
#[test]
fn cell_long_number_precision_save_and_reopen() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());

    // 1. 新規シートを追加
    let (id, shape) = (d.ids().new_slide_id(), d.ids().new_shape_id());
    d.execute(vec![Command::Sheet(SheetCommand::AddSheet { index: 1, name: "長数値精度".into(), slide: id, shape })]).unwrap();

    // 2. parse_input を用いて長い数値・小数を設定
    let edits = vec![
        // 15桁整数（そのまま保持）
        (0, 0, ibis_calc::input::parse_input("123456789012345", false)),
        // 16桁整数（16桁目が0に丸められる）
        (1, 0, ibis_calc::input::parse_input("1234567890123456", false)),
        // 19桁整数（16〜19桁目が0に丸められる）
        (2, 0, ibis_calc::input::parse_input("1234567890123456789", false)),
        // 16桁小数（16桁目が切り捨て）
        (3, 0, ibis_calc::input::parse_input("1.234567890123456", false)),
        // クォート付き長い文字列（テキストとして全桁完全保持）
        (4, 0, ibis_calc::input::parse_input("'1234567890123456789", false)),
        // カンマ区切りの長い数値
        (5, 0, ibis_calc::input::parse_input("1,234,567,890,123,456", false)),
        // 指数表記の長い数値
        (6, 0, ibis_calc::input::parse_input("1.234567890123456E+15", false)),
    ];

    let cell_inputs = edits.into_iter().map(|(r, c, typed)| {
        let input = match typed {
            ibis_calc::input::Typed::Value(v, _) => CellInput::Value(v),
            ibis_calc::input::Typed::Formula(f) => CellInput::Formula(f),
        };
        (r, c, input)
    }).collect();
    set(&mut d, id, cell_inputs);

    // 数式セル B1, B2 を設定
    set(&mut d, id, vec![
        (0, 1, CellInput::Formula("A1+1".into())),
        (1, 1, CellInput::Formula("A2+1".into())),
    ]);

    // 値の検証
    let sh = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh.cell(0, 0).unwrap().value, CellValue::Number(123456789012345.0), "15 digits intact");
    assert_eq!(sh.cell(1, 0).unwrap().value, CellValue::Number(1234567890123450.0), "16th digit becomes 0");
    assert_eq!(sh.cell(2, 0).unwrap().value, CellValue::Number(1234567890123450000.0), "16-19th digits become 0");
    assert_eq!(sh.cell(3, 0).unwrap().value, CellValue::Number(1.23456789012345), "fraction truncated to 15 digits");
    assert_eq!(sh.cell(4, 0).unwrap().value, CellValue::Text("1234567890123456789".into()), "quoted text preserved completely");
    assert_eq!(sh.cell(5, 0).unwrap().value, CellValue::Number(1234567890123450.0), "comma separated long number");
    assert_eq!(sh.cell(6, 0).unwrap().value, CellValue::Number(1234567890123450.0), "scientific long number");

    // 計算値の検証
    assert_eq!(sh.cell(0, 1).unwrap().value, CellValue::Number(123456789012346.0));
    assert_eq!(sh.cell(1, 1).unwrap().value, CellValue::Number(1234567890123451.0));

    // 4. 保存と XML 検証
    let saved = save(&opened, d.presentation(), "cell-long-number-precision.xlsx");
    assert_reads_back(&saved, d.presentation());

    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet_xml = pkg.names()
        .filter(|n| n.starts_with("xl/worksheets/sheet") && n.ends_with(".xml"))
        .find_map(|n| {
            let s = String::from_utf8(pkg.read(n).unwrap()).ok()?;
            if s.contains("1234567890123450") {
                Some(s)
            } else {
                None
            }
        })
        .expect("Should find worksheet XML containing long numbers");
    assert!(sheet_xml.contains("1234567890123450"), "XML contains 15-digit rounded number");

    // 5. 再読込後のモデル検証
    let reopened = ibis_pptx::open(saved).unwrap();
    let re_id = slide(&reopened.presentation, "長数値精度");
    let re_sh = sheet_of_slide(reopened.presentation.slide(re_id).unwrap()).unwrap();
    assert_eq!(re_sh.cell(0, 0).unwrap().value, CellValue::Number(123456789012345.0));
    assert_eq!(re_sh.cell(1, 0).unwrap().value, CellValue::Number(1234567890123450.0));
    assert_eq!(re_sh.cell(2, 0).unwrap().value, CellValue::Number(1234567890123450000.0));
    assert_eq!(re_sh.cell(3, 0).unwrap().value, CellValue::Number(1.23456789012345));
    assert_eq!(re_sh.cell(4, 0).unwrap().value, CellValue::Text("1234567890123456789".into()));
    assert_eq!(re_sh.cell(0, 1).unwrap().value, CellValue::Number(123456789012346.0));
    assert_eq!(re_sh.cell(1, 1).unwrap().value, CellValue::Number(1234567890123451.0));

    // 6. カスケード再計算と Undo / Redo 検証:
    // A2 を別の16桁数値 9876543210987654 に変更 (-> 9876543210987650)
    let new_input = match ibis_calc::input::parse_input("9876543210987654", false) {
        ibis_calc::input::Typed::Value(v, _) => CellInput::Value(v),
        _ => unreachable!(),
    };
    set(&mut d, id, vec![(1, 0, new_input)]);
    let sh_after = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_after.cell(1, 0).unwrap().value, CellValue::Number(9876543210987650.0));
    assert_eq!(sh_after.cell(1, 1).unwrap().value, CellValue::Number(9876543210987651.0), "B2 cascades");

    // Undo
    assert!(d.undo());
    let sh_undo = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_undo.cell(1, 0).unwrap().value, CellValue::Number(1234567890123450.0));
    assert_eq!(sh_undo.cell(1, 1).unwrap().value, CellValue::Number(1234567890123451.0));

    // Redo
    assert!(d.redo());
    let sh_redo = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_redo.cell(1, 0).unwrap().value, CellValue::Number(9876543210987650.0));
    assert_eq!(sh_redo.cell(1, 1).unwrap().value, CellValue::Number(9876543210987651.0));
}

#[test]
fn pivot_options_and_field_settings_roundtrip() {
    let (_data, opened) = open("xlsx-pivot-not-refreshed.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let id = opened.presentation.slides[0].id;
    let s = d.presentation().slide(id).unwrap();
    let mut sh = (**sheet_of_slide(s).unwrap()).clone();
    assert!(!sh.pivots.is_empty(), "has pivot table");

    // 1. ピボットテーブルオプション（表示名キャプション・データ更新）を設定
    sh.pivots[0].grand_total_caption = Some("総合計".into());
    sh.pivots[0].row_header_caption = Some("部門".into());
    sh.pivots[0].col_header_caption = Some("年度".into());
    sh.pivots[0].refresh_on_load = true;

    // 2. フィールド設定（小計の有無、グループ先頭/末尾、レイアウト形式）を設定
    if let Some(f0) = sh.pivots[0].fields.get_mut(0) {
        f0.default_subtotal = false; // 小計非表示
        f0.compact = false;
        f0.outline = true; // アウトライン形式
    }
    if let Some(f1) = sh.pivots[0].fields.get_mut(1) {
        f1.default_subtotal = true;
        f1.subtotal_top = false; // 小計をグループの下部に表示
    }

    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: id, sheet: Box::new(sh) })]).unwrap();
    let saved = save(&opened, d.presentation(), "pivot-options-field-settings.xlsx");

    // 3. XML 検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let pivot_xml = pkg.names().filter(|n| n.starts_with("xl/pivotTables/pivotTable") && n.ends_with(".xml"))
        .find_map(|n| String::from_utf8(pkg.read(n).unwrap()).ok())
        .expect("pivot table xml exists");

    assert!(pivot_xml.contains(r#"grandTotalCaption="総合計""#), "XML contains grandTotalCaption: {pivot_xml}");
    assert!(pivot_xml.contains(r#"rowHeaderCaption="部門""#), "XML contains rowHeaderCaption: {pivot_xml}");
    assert!(pivot_xml.contains(r#"colHeaderCaption="年度""#), "XML contains colHeaderCaption: {pivot_xml}");
    assert!(pivot_xml.contains(r#"defaultSubtotal="0""#), "XML contains defaultSubtotal=0: {pivot_xml}");
    assert!(pivot_xml.contains(r#"compact="0""#), "XML contains compact=0: {pivot_xml}");
    assert!(pivot_xml.contains(r#"subtotalTop="0""#), "XML contains subtotalTop=0: {pivot_xml}");

    // 4. 再読込後のモデル検証
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let s2 = reopened.slide(id).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();
    let p2 = &sh2.pivots[0];

    assert_eq!(p2.grand_total_caption.as_deref(), Some("総合計"));
    assert_eq!(p2.row_header_caption.as_deref(), Some("部門"));
    assert_eq!(p2.col_header_caption.as_deref(), Some("年度"));

    assert!(!p2.fields[0].default_subtotal, "field 0 defaultSubtotal restored to false");
    assert!(!p2.fields[0].compact, "field 0 compact restored to false");
    assert!(p2.fields[0].outline, "field 0 outline restored to true");

    assert!(p2.fields[1].default_subtotal, "field 1 defaultSubtotal restored to true");
    assert!(!p2.fields[1].subtotal_top, "field 1 subtotalTop restored to false");

    // 5. Undo / Redo 検証
    assert!(d.undo());
    let sh_undo = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_undo.pivots[0].grand_total_caption, None);
    assert!(sh_undo.pivots[0].fields[0].default_subtotal);

    assert!(d.redo());
    let sh_redo = sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap();
    assert_eq!(sh_redo.pivots[0].grand_total_caption.as_deref(), Some("総合計"));
    assert!(!sh_redo.pivots[0].fields[0].default_subtotal);
}

#[test]
fn cell_pattern_color_and_gradient_fill_save_and_reopen_roundtrip() {
    use ibis_doc_model::{CellGradient, CellRange, GradientPath, Rgb, StyleChange};

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");

    let range_a1 = CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 };
    let range_b1 = CellRange { first_row: 0, first_column: 1, last_row: 0, last_column: 1 };
    let range_c1 = CellRange { first_row: 0, first_column: 2, last_row: 0, last_column: 2 };

    // 1. A1 に背景色黄色 + ltUpDiag パターン + 前景色赤を設定
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat {
        slide: s,
        ranges: vec![range_a1],
        change: StyleChange::Fill(Some(Rgb(0xFFFF00))),
    })]).unwrap();
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat {
        slide: s,
        ranges: vec![range_a1],
        change: StyleChange::Pattern(Some(("ltUpDiag".into(), Rgb(0xC00000), Rgb(0xFFFF00)))),
    })]).unwrap();

    // パターン色を青に変更 (StyleChange::PatternColor)
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat {
        slide: s,
        ranges: vec![range_a1],
        change: StyleChange::PatternColor(Rgb(0x4472C4)),
    })]).unwrap();

    // 2. B1 に線形グラデーション (Linear 90度: オレンジ→白)
    let linear_grad = CellGradient::Linear {
        degree: 90,
        stops: vec![(0, Rgb(0xED7D31)), (1000, Rgb(0xFFFFFF))],
    };
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat {
        slide: s,
        ranges: vec![range_b1],
        change: StyleChange::Gradient(Some(linear_grad.clone())),
    })]).unwrap();

    // 3. C1 にパスグラデーション (Path Rect: 緑→白)
    let path_grad = CellGradient::Path {
        path: GradientPath::Rect,
        focus: [50_000, 50_000, 50_000, 50_000],
        stops: vec![(0, Rgb(0x70AD47)), (1000, Rgb(0xFFFFFF))],
    };
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat {
        slide: s,
        ranges: vec![range_c1],
        change: StyleChange::Gradient(Some(path_grad.clone())),
    })]).unwrap();

    // モデル上の検証
    let sh = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    let st_a1 = &sh.styles[sh.cell(0, 0).unwrap().style as usize];
    assert_eq!(st_a1.pattern, Some(("ltUpDiag".into(), Rgb(0x4472C4), Rgb(0xFFFF00))));
    assert_eq!(st_a1.fill, Some(Rgb(0xFFFF00)));
    assert_eq!(st_a1.gradient, None);

    let st_b1 = &sh.styles[sh.cell(0, 1).unwrap().style as usize];
    assert_eq!(st_b1.gradient, Some(linear_grad.clone()));
    assert_eq!(st_b1.pattern, None);
    assert_eq!(st_b1.fill, None);

    let st_c1 = &sh.styles[sh.cell(0, 2).unwrap().style as usize];
    assert_eq!(st_c1.gradient, Some(path_grad.clone()));
    assert_eq!(st_c1.pattern, None);
    assert_eq!(st_c1.fill, None);

    // 4. OOXML 保存
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();

    // 5. styles.xml の XML 検証
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let styles_xml = String::from_utf8(pkg.read("xl/styles.xml").unwrap()).unwrap();

    assert!(styles_xml.contains(r#"patternType="lightUp""#), "XML contains patternType: {styles_xml}");
    assert!(styles_xml.contains(r#"rgb="FF4472C4""#), "XML contains pattern fgColor: {styles_xml}");
    assert!(styles_xml.contains(r#"rgb="FFFFFF00""#), "XML contains pattern bgColor: {styles_xml}");

    assert!(styles_xml.contains(r#"<gradientFill degree="90">"#), "XML contains linear gradient: {styles_xml}");
    assert!(styles_xml.contains(r#"rgb="FFED7D31""#), "XML contains linear gradient color: {styles_xml}");

    assert!(styles_xml.contains(r#"<gradientFill type="path""#), "XML contains path gradient: {styles_xml}");
    assert!(styles_xml.contains(r#"rgb="FF70AD47""#), "XML contains path gradient color: {styles_xml}");

    // 6. 再読込後のモデル復元検証
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let s2 = reopened.slide(s).unwrap();
    let sh2 = sheet_of_slide(s2).unwrap();

    let r_a1 = &sh2.styles[sh2.cell(0, 0).unwrap().style as usize];
    assert_eq!(r_a1.pattern, Some(("ltUpDiag".into(), Rgb(0x4472C4), Rgb(0xFFFF00))));

    let r_b1 = &sh2.styles[sh2.cell(0, 1).unwrap().style as usize];
    assert_eq!(r_b1.gradient, Some(linear_grad));

    let r_c1 = &sh2.styles[sh2.cell(0, 2).unwrap().style as usize];
    assert_eq!(r_c1.gradient, Some(path_grad));

    // 7. Undo / Redo 検証
    assert!(d.undo()); // undo C1
    let sh_u1 = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh_u1.styles[sh_u1.cell(0, 2).unwrap().style as usize].gradient, None);

    assert!(d.undo()); // undo B1
    let sh_u2 = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh_u2.styles[sh_u2.cell(0, 1).unwrap().style as usize].gradient, None);

    assert!(d.undo()); // undo A1 PatternColor
    let sh_u3 = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh_u3.styles[sh_u3.cell(0, 0).unwrap().style as usize].pattern, Some(("ltUpDiag".into(), Rgb(0xC00000), Rgb(0xFFFF00))));

    assert!(d.redo());
    let sh_r1 = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh_r1.styles[sh_r1.cell(0, 0).unwrap().style as usize].pattern, Some(("ltUpDiag".into(), Rgb(0x4472C4), Rgb(0xFFFF00))));
}

#[test]
fn workbook_theme_palette_change_and_save_roundtrip() {
    use ibis_doc_model::sheet_command::{SheetCommand, StyleChange};
    use ibis_doc_model::{CellRange, Command, Document, Rgb, ThemeColor};

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");

    // 1. 初期のテーマ色を確認
    let initial_colors = d.presentation().workbook.as_ref().unwrap().theme_colors.clone();
    let initial_accent1 = initial_colors.iter().find(|(n, _)| n == "accent1").unwrap().1;
    let initial_accent2 = initial_colors.iter().find(|(n, _)| n == "accent2").unwrap().1;

    // 2. セル A1 に accent1 のテーマ塗りつぶし、セル A2 に accent2 のテーマ文字色を設定
    let r1 = CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 };
    let r2 = CellRange { first_row: 1, first_column: 0, last_row: 1, last_column: 0 };
    d.execute(vec![
        Command::Sheet(SheetCommand::SetFormat {
            slide: s,
            ranges: vec![r1],
            change: StyleChange::FillTheme(initial_accent1, ThemeColor { index: 4, tint: 0 }),
        }),
        Command::Sheet(SheetCommand::SetFormat {
            slide: s,
            ranges: vec![r2],
            change: StyleChange::ColorTheme(initial_accent2, ThemeColor { index: 5, tint: 0 }),
        }),
    ]).unwrap();

    let sh = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    let style_a1 = &sh.styles[sh.cell(0, 0).unwrap().style as usize];
    assert_eq!(style_a1.fill, Some(initial_accent1));
    assert_eq!(style_a1.fill_theme, Some(ThemeColor { index: 4, tint: 0 }));

    let style_a2 = &sh.styles[sh.cell(1, 0).unwrap().style as usize];
    assert_eq!(style_a2.color, initial_accent2);
    assert_eq!(style_a2.color_theme, Some(ThemeColor { index: 5, tint: 0 }));

    // 3. Aspect テーマの配色（accent1: F0A22E, accent2: A56110）に変更
    let mut aspect_colors = initial_colors.clone();
    for (name, rgb) in [
        ("accent1", Rgb(0xF0A22E)),
        ("accent2", Rgb(0xA56110)),
        ("accent3", Rgb(0x90A437)),
        ("accent4", Rgb(0x43896A)),
        ("accent5", Rgb(0x427891)),
        ("accent6", Rgb(0x4E4273)),
    ] {
        if let Some(entry) = aspect_colors.iter_mut().find(|(n, _)| n == name) {
            entry.1 = rgb;
        }
    }
    d.execute(vec![Command::SetTheme {
        colors: aspect_colors.clone(),
        fonts: d.presentation().theme_fonts.clone(),
    }]).unwrap();

    // 4. モデル内のテーマ色およびセルの描画色が即座に Aspect 色に連動していることを確認
    assert_eq!(d.presentation().workbook.as_ref().unwrap().theme_colors, aspect_colors);
    let sh_aspect = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    let style_a1_aspect = &sh_aspect.styles[sh_aspect.cell(0, 0).unwrap().style as usize];
    assert_eq!(style_a1_aspect.fill, Some(Rgb(0xF0A22E)));
    assert_eq!(style_a1_aspect.fill_theme, Some(ThemeColor { index: 4, tint: 0 }));

    let style_a2_aspect = &sh_aspect.styles[sh_aspect.cell(1, 0).unwrap().style as usize];
    assert_eq!(style_a2_aspect.color, Rgb(0xA56110));
    assert_eq!(style_a2_aspect.color_theme, Some(ThemeColor { index: 5, tint: 0 }));

    // 5. 保存して開き直す（ラウンドトリップ検証）
    let bytes = save(&opened, d.presentation(), "theme-aspect.xlsx");
    let read = ibis_pptx::open(bytes).unwrap();
    let read_wb = read.presentation.workbook.as_ref().unwrap();
    assert_eq!(read_wb.theme_colors.iter().find(|(n, _)| n == "accent1").unwrap().1, Rgb(0xF0A22E));
    assert_eq!(read_wb.theme_colors.iter().find(|(n, _)| n == "accent2").unwrap().1, Rgb(0xA56110));

    let sh_read = sheet_of_slide(read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap()).unwrap();
    let style_a1_read = &sh_read.styles[sh_read.cell(0, 0).unwrap().style as usize];
    assert_eq!(style_a1_read.fill, Some(Rgb(0xF0A22E)));
    assert_eq!(style_a1_read.fill_theme, Some(ThemeColor { index: 4, tint: 0 }));

    let style_a2_read = &sh_read.styles[sh_read.cell(1, 0).unwrap().style as usize];
    assert_eq!(style_a2_read.color, Rgb(0xA56110));
    assert_eq!(style_a2_read.color_theme, Some(ThemeColor { index: 5, tint: 0 }));

    // 6. Undo 検証（元の初期テーマ色に戻り、セルの色も元に戻る）
    assert!(d.undo());
    assert_eq!(d.presentation().workbook.as_ref().unwrap().theme_colors, initial_colors);
    let sh_undo = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    let style_a1_undo = &sh_undo.styles[sh_undo.cell(0, 0).unwrap().style as usize];
    assert_eq!(style_a1_undo.fill, Some(initial_accent1));

    let style_a2_undo = &sh_undo.styles[sh_undo.cell(1, 0).unwrap().style as usize];
    assert_eq!(style_a2_undo.color, initial_accent2);

    // 7. Redo 検証
    assert!(d.redo());
    assert_eq!(d.presentation().workbook.as_ref().unwrap().theme_colors, aspect_colors);
    let sh_redo = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh_redo.styles[sh_redo.cell(0, 0).unwrap().style as usize].fill, Some(Rgb(0xF0A22E)));
    assert_eq!(sh_redo.styles[sh_redo.cell(1, 0).unwrap().style as usize].color, Rgb(0xA56110));
}

#[test]
fn chart_data_table_save_and_reopen_roundtrip() {
    use ibis_doc_model::{ChartDataTable, Command, ShapeKind};
    let (_, opened) = open("xlsx-sales.xlsx");
    let s = slide(&opened.presentation, "月次売上");
    let shape_id = opened.presentation.slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(_) => Some(sh.id),
        _ => None,
    }).expect("chart shape exists");

    let initial_chart = opened.presentation.slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(initial_chart.data_table, None);

    let mut d = Document::new(opened.presentation.clone());

    // 1. データテーブル（凡例キーあり、全罫線あり）を設定
    let mut chart_with_table = initial_chart.clone();
    let dt1 = ChartDataTable {
        show_horz_border: true,
        show_vert_border: true,
        show_outline: true,
        show_keys: true,
    };
    chart_with_table.data_table = Some(dt1);

    d.execute(vec![Command::SetKind {
        slide: s,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(chart_with_table.clone()))),
    }]).unwrap();

    // 2. OOXML 保存と再読込
    let bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reopened = ibis_pptx::read(&bytes).unwrap();
    let read_chart = reopened.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(read_chart.data_table, Some(dt1));

    // 3. データテーブルの設定変更（凡例キーなし、水平罫線なし、外枠なし）
    let dt2 = ChartDataTable {
        show_horz_border: false,
        show_vert_border: true,
        show_outline: false,
        show_keys: false,
    };
    let mut chart_with_table2 = initial_chart.clone();
    chart_with_table2.data_table = Some(dt2);
    d.execute(vec![Command::SetKind {
        slide: s,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(chart_with_table2.clone()))),
    }]).unwrap();

    let bytes2 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reopened2 = ibis_pptx::read(&bytes2).unwrap();
    let read_chart2 = reopened2.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(read_chart2.data_table, Some(dt2));

    // 4. データテーブルを削除 (None)
    d.execute(vec![Command::SetKind {
        slide: s,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(initial_chart.clone()))),
    }]).unwrap();

    let bytes3 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reopened3 = ibis_pptx::read(&bytes3).unwrap();
    let read_chart3 = reopened3.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(read_chart3.data_table, None);

    // 5. Undo 検証
    assert!(d.undo()); // back to dt2
    let ch_undo1 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo1.data_table, Some(dt2));

    assert!(d.undo()); // back to dt1
    let ch_undo2 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo2.data_table, Some(dt1));

    assert!(d.undo()); // back to initial (None)
    let ch_undo3 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo3.data_table, None);

    // 6. Redo 検証
    assert!(d.redo()); // dt1
    assert!(d.redo()); // dt2
    let ch_redo = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_redo.data_table, Some(dt2));
}

#[test]
fn chart_trendline_save_and_reopen_roundtrip() {
    use ibis_doc_model::chart::{Trendline, TrendlineType};
    use ibis_doc_model::{Command, ShapeKind};

    let (_, opened) = open("xlsx-sales.xlsx");
    let s = slide(&opened.presentation, "月次売上");
    let shape_id = opened.presentation.slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(_) => Some(sh.id),
        _ => None,
    }).expect("chart shape exists");

    let initial_chart = opened.presentation.slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();

    let mut d = Document::new(opened.presentation.clone());

    // 1. 初期状態では trendline は None
    assert!(initial_chart.groups.iter().flat_map(|g| &g.series).all(|ser| ser.trendline.is_none()));

    // 2. 線形近似曲線（数式表示・R-2乗値表示オン）を設定して保存・開き直し
    let tl1 = Trendline {
        kind: TrendlineType::Linear,
        order: 2,
        period: 2,
        disp_eq: true,
        disp_r_sqr: true,
        stroke: None,
    };
    let mut modified1 = initial_chart.clone();
    for g in modified1.groups.iter_mut() {
        for ser in g.series.iter_mut() {
            ser.trendline = Some(tl1.clone());
        }
    }

    d.execute(vec![Command::SetKind {
        slide: s,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(modified1.clone()))),
    }]).unwrap();

    let bytes1 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reopened1 = ibis_pptx::read(&bytes1).unwrap();
    let read_chart1 = reopened1.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();

    let first_ser1 = read_chart1.groups.first().and_then(|g| g.series.first()).unwrap();
    assert_eq!(first_ser1.trendline, Some(tl1.clone()));

    // 3. 移動平均近似曲線（期間3）に変更して保存・開き直し
    let tl2 = Trendline {
        kind: TrendlineType::MovingAverage,
        order: 2,
        period: 3,
        disp_eq: false,
        disp_r_sqr: false,
        stroke: None,
    };
    let mut modified2 = initial_chart.clone();
    for g in modified2.groups.iter_mut() {
        for ser in g.series.iter_mut() {
            ser.trendline = Some(tl2.clone());
        }
    }

    d.execute(vec![Command::SetKind {
        slide: s,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(modified2.clone()))),
    }]).unwrap();

    let bytes2 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reopened2 = ibis_pptx::read(&bytes2).unwrap();
    let read_chart2 = reopened2.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();

    let first_ser2 = read_chart2.groups.first().and_then(|g| g.series.first()).unwrap();
    assert_eq!(first_ser2.trendline, Some(tl2.clone()));

    // 4. 近似曲線を削除（None）にして保存・開き直し
    d.execute(vec![Command::SetKind {
        slide: s,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(initial_chart.clone()))),
    }]).unwrap();

    let bytes3 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reopened3 = ibis_pptx::read(&bytes3).unwrap();
    let read_chart3 = reopened3.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();

    let first_ser3 = read_chart3.groups.first().and_then(|g| g.series.first()).unwrap();
    assert_eq!(first_ser3.trendline, None);

    // 5. Undo 検証
    assert!(d.undo()); // back to tl2
    let ch_undo1 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo1.groups[0].series[0].trendline, Some(tl2.clone()));

    assert!(d.undo()); // back to tl1
    let ch_undo2 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo2.groups[0].series[0].trendline, Some(tl1.clone()));

    assert!(d.undo()); // back to initial (None)
    let ch_undo3 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo3.groups[0].series[0].trendline, None);

    // 6. Redo 検証
    assert!(d.redo()); // tl1
    assert!(d.redo()); // tl2
    let ch_redo = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_redo.groups[0].series[0].trendline, Some(tl2));
}

#[test]
fn chart_error_bars_save_and_reopen_roundtrip() {
    use ibis_doc_model::chart::{ErrorBarDirection, ErrorBarType, ErrorBarValueType, ErrorBars, Number};
    use ibis_doc_model::{Command, ShapeKind};

    let (_, opened) = open("xlsx-sales.xlsx");
    let s = slide(&opened.presentation, "月次売上");
    let shape_id = opened.presentation.slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(_) => Some(sh.id),
        _ => None,
    }).expect("chart shape exists");

    let initial_chart = opened.presentation.slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();

    let mut d = Document::new(opened.presentation.clone());

    // 1. 初期状態では error_bars は None
    assert!(initial_chart.groups.iter().flat_map(|g| &g.series).all(|ser| ser.error_bars.is_none()));

    // 2. 両方向・固定値（1.5）・末端キャップありを設定して保存・開き直し
    let eb1 = ErrorBars {
        direction: ErrorBarDirection::Y,
        bar_type: ErrorBarType::Both,
        val_type: ErrorBarValueType::FixedValue,
        no_end_cap: false,
        value: Some(Number(1.5)),
        stroke: None,
        ..Default::default()
    };
    let mut modified1 = initial_chart.clone();
    for g in modified1.groups.iter_mut() {
        for ser in g.series.iter_mut() {
            ser.error_bars = Some(eb1.clone());
        }
    }

    d.execute(vec![Command::SetKind {
        slide: s,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(modified1.clone()))),
    }]).unwrap();

    let bytes1 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reopened1 = ibis_pptx::read(&bytes1).unwrap();
    let read_chart1 = reopened1.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();

    let first_ser1 = read_chart1.groups.first().and_then(|g| g.series.first()).unwrap();
    assert_eq!(first_ser1.error_bars, Some(eb1.clone()));

    // 3. 正方向・パーセンテージ（5.0%）・末端キャップなしに変更して保存・開き直し
    let eb2 = ErrorBars {
        direction: ErrorBarDirection::Y,
        bar_type: ErrorBarType::Plus,
        val_type: ErrorBarValueType::Percentage,
        no_end_cap: true,
        value: Some(Number(5.0)),
        stroke: None,
        ..Default::default()
    };
    let mut modified2 = initial_chart.clone();
    for g in modified2.groups.iter_mut() {
        for ser in g.series.iter_mut() {
            ser.error_bars = Some(eb2.clone());
        }
    }

    d.execute(vec![Command::SetKind {
        slide: s,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(modified2.clone()))),
    }]).unwrap();

    let bytes2 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reopened2 = ibis_pptx::read(&bytes2).unwrap();
    let read_chart2 = reopened2.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();

    let first_ser2 = read_chart2.groups.first().and_then(|g| g.series.first()).unwrap();
    assert_eq!(first_ser2.error_bars, Some(eb2.clone()));

    // 4. 誤差範囲を削除（None）にして保存・開き直し
    let mut modified3 = initial_chart.clone();
    for g in modified3.groups.iter_mut() {
        for ser in g.series.iter_mut() {
            ser.error_bars = None;
        }
    }

    d.execute(vec![Command::SetKind {
        slide: s,
        shape: shape_id,
        kind: Box::new(ShapeKind::Chart(Box::new(modified3.clone()))),
    }]).unwrap();

    let bytes3 = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reopened3 = ibis_pptx::read(&bytes3).unwrap();
    let read_chart3 = reopened3.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();

    let first_ser3 = read_chart3.groups.first().and_then(|g| g.series.first()).unwrap();
    assert_eq!(first_ser3.error_bars, None);

    // 5. Undo 検証
    assert!(d.undo()); // back to eb2
    let ch_undo1 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo1.groups[0].series[0].error_bars, Some(eb2.clone()));

    assert!(d.undo()); // back to eb1
    let ch_undo2 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo2.groups[0].series[0].error_bars, Some(eb1.clone()));

    assert!(d.undo()); // back to initial (None)
    let ch_undo3 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo3.groups[0].series[0].error_bars, None);

    // 6. Redo 検証
    assert!(d.redo()); // eb1
    assert!(d.redo()); // eb2
    let ch_redo = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_redo.groups[0].series[0].error_bars, Some(eb2));
}

#[test]
fn chart_style_and_palette_save_and_reopen_roundtrip() {
    use ibis_doc_model::{Color, Fill, Rgb, ShapeKind, Stroke};

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
        _ => None,
    }).unwrap();

    assert_eq!(chart.style, Some(2));

    // 1. スタイル 3 を設定し、カラフル 1（#4472C4, #ED7D31）を系列に適用
    let mut modified = chart.clone();
    modified.style = Some(3);
    if let Some(ser) = modified.groups.get_mut(0).and_then(|g| g.series.get_mut(0)) {
        ser.fill = Fill::Solid(Color::opaque(Rgb(0x4472C4)));
        ser.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x4472C4)), 28_575));
    }
    if let Some(ser) = modified.groups.get_mut(0).and_then(|g| g.series.get_mut(1)) {
        ser.fill = Fill::Solid(Color::opaque(Rgb(0xED7D31)));
        ser.stroke = Some(Stroke::solid(Color::opaque(Rgb(0xED7D31)), 28_575));
    }
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();

    let bytes = save(&opened, d.presentation(), "chart-style-3.xlsx");
    let package = ibis_ooxml::Package::open(bytes.clone()).unwrap();
    let chart_xml = package.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml.contains(r#"style val="3""#), "XML should contain style 3: {chart_xml}");
    assert!(!chart_xml.contains("AlternateContent"), "AlternateContent should be removed when style updated");

    // 再読み込み検証
    let read = ibis_pptx::open(bytes.clone()).unwrap();
    let (_, read_chart) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
        _ => None,
    }).unwrap();
    assert_eq!(read_chart.style, Some(3));
    assert_eq!(read_chart.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x4472C4))));
    assert_eq!(read_chart.groups[0].series[1].fill, Fill::Solid(Color::opaque(Rgb(0xED7D31))));

    // 2. スタイル 5 に変更し、モノクロ 1（#2F5597, #4472C4）を適用
    let mut modified2 = read_chart.clone();
    modified2.style = Some(5);
    if let Some(ser) = modified2.groups.get_mut(0).and_then(|g| g.series.get_mut(0)) {
        ser.fill = Fill::Solid(Color::opaque(Rgb(0x2F5597)));
        ser.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x2F5597)), 28_575));
    }
    if let Some(ser) = modified2.groups.get_mut(0).and_then(|g| g.series.get_mut(1)) {
        ser.fill = Fill::Solid(Color::opaque(Rgb(0x4472C4)));
        ser.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x4472C4)), 28_575));
    }
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ShapeKind::Chart(Box::new(modified2.clone()))) }]).unwrap();

    let bytes2 = save(&read, d.presentation(), "chart-style-5.xlsx");
    let package2 = ibis_ooxml::Package::open(bytes2.clone()).unwrap();
    let chart_xml2 = package2.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package2.read(n).unwrap()).unwrap()).unwrap();
    assert!(chart_xml2.contains(r#"style val="5""#), "XML should contain style 5: {chart_xml2}");

    let read2 = ibis_pptx::open(bytes2.clone()).unwrap();
    let (_, read2_chart) = read2.presentation.slide(slide(&read2.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
        _ => None,
    }).unwrap();
    assert_eq!(read2_chart.style, Some(5));
    assert_eq!(read2_chart.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x2F5597))));
    assert_eq!(read2_chart.groups[0].series[1].fill, Fill::Solid(Color::opaque(Rgb(0x4472C4))));

    // 3. スタイルを削除（None）
    let mut modified3 = read2_chart.clone();
    modified3.style = None;
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ShapeKind::Chart(Box::new(modified3.clone()))) }]).unwrap();

    let bytes3 = save(&read2, d.presentation(), "chart-style-none.xlsx");
    let package3 = ibis_ooxml::Package::open(bytes3.clone()).unwrap();
    let chart_xml3 = package3.names().find(|n| n.starts_with("xl/charts/chart")).map(|n| String::from_utf8(package3.read(n).unwrap()).unwrap()).unwrap();
    assert!(!chart_xml3.contains("<c:style"), "XML should not contain style: {chart_xml3}");
    assert!(!chart_xml3.contains("AlternateContent"), "XML should not contain AlternateContent: {chart_xml3}");

    let read3 = ibis_pptx::open(bytes3.clone()).unwrap();
    let (_, read3_chart) = read3.presentation.slide(slide(&read3.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
        _ => None,
    }).unwrap();
    assert_eq!(read3_chart.style, None);

    // 4. Undo 検証
    assert!(d.undo()); // back to style 5
    let ch_undo1 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo1.style, Some(5));
    assert_eq!(ch_undo1.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x2F5597))));

    assert!(d.undo()); // back to style 3
    let ch_undo2 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo2.style, Some(3));
    assert_eq!(ch_undo2.groups[0].series[0].fill, Fill::Solid(Color::opaque(Rgb(0x4472C4))));

    assert!(d.undo()); // back to initial
    let ch_undo3 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_undo3.style, Some(2));

    // 5. Redo 検証
    assert!(d.redo()); // style 3
    let ch_redo1 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_redo1.style, Some(3));

    assert!(d.redo()); // style 5
    let ch_redo2 = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).unwrap();
    assert_eq!(ch_redo2.style, Some(5));
}

#[test]
fn test_sheet_shape_change_geometry_roundtrip() {
    use ibis_doc_model::{Geometry, ShapeKind};

    let (data, _) = open("xlsx-sales.xlsx");
    let mut pkg = ibis_ooxml::Package::open(data).unwrap();
    {
        let doc = pkg.xml_mut("xl/drawings/drawing1.xml").unwrap();
        let root = doc.root_element().unwrap();
        let anchor = doc.append_element(root, "xdr:twoCellAnchor");
        doc.set_attr(anchor, "editAs", "twoCell");
        let from = doc.append_element(anchor, "xdr:from");
        let fc = doc.append_element(from, "xdr:col"); doc.append_text(fc, "1");
        let fco = doc.append_element(from, "xdr:colOff"); doc.append_text(fco, "0");
        let fr = doc.append_element(from, "xdr:row"); doc.append_text(fr, "1");
        let fro = doc.append_element(from, "xdr:rowOff"); doc.append_text(fro, "0");
        let to = doc.append_element(anchor, "xdr:to");
        let tc = doc.append_element(to, "xdr:col"); doc.append_text(tc, "3");
        let tco = doc.append_element(to, "xdr:colOff"); doc.append_text(tco, "0");
        let tr = doc.append_element(to, "xdr:row"); doc.append_text(tr, "5");
        let tro = doc.append_element(to, "xdr:rowOff"); doc.append_text(tro, "0");
        let sp = doc.append_element(anchor, "xdr:sp");
        let nv = doc.append_element(sp, "xdr:nvSpPr");
        let c_nv = doc.append_element(nv, "xdr:cNvPr");
        doc.set_attr(c_nv, "id", "100");
        doc.set_attr(c_nv, "name", "四角形 1");
        let _c_nv_sp = doc.append_element(nv, "xdr:cNvSpPr");
        let sp_pr = doc.append_element(sp, "xdr:spPr");
        let xfrm = doc.append_element(sp_pr, "a:xfrm");
        let off = doc.append_element(xfrm, "a:off");
        doc.set_attr(off, "x", "0"); doc.set_attr(off, "y", "0");
        let ext = doc.append_element(xfrm, "a:ext");
        doc.set_attr(ext, "cx", "2000000"); doc.set_attr(ext, "cy", "1200000");
        let geom = doc.append_element(sp_pr, "a:prstGeom");
        doc.set_attr(geom, "prst", "rect");
        let _ = doc.append_element(anchor, "xdr:clientData");
    }
    let input = pkg.save(ibis_pptx::SaveOptions::default()).unwrap();
    let opened = ibis_pptx::open(input).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s0 = d.presentation().slides[0].id;
    let auto_shape = d.presentation().slide(s0).unwrap().shapes.iter().find(|sh| matches!(sh.kind, ShapeKind::Auto(_))).expect("auto shape found").id;

    // 初期状態の形状が Rect であることを確認
    let ShapeKind::Auto(initial_auto) = &d.presentation().slide(s0).unwrap().shape(auto_shape).unwrap().kind else { panic!() };
    assert_eq!(initial_auto.geometry, Geometry::Rect);
    assert_eq!(initial_auto.geometry.preset_name(), Some("rect"));

    // 1. 図形の形状を ellipse (楕円) に変更
    let ellipse_geom = ibis_pptx::preset_geometry("ellipse").unwrap();
    let mut changed_auto = initial_auto.clone();
    changed_auto.geometry = ellipse_geom;
    d.execute(vec![Command::SetKind {
        slide: s0,
        shape: auto_shape,
        kind: Box::new(ShapeKind::Auto(changed_auto)),
    }]).unwrap();

    // 2. 保存して XML 検証
    let saved = opened.source.save(d.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    let pkg_saved = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = pkg_saved.xml("xl/drawings/drawing1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let pg = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "prstGeom").expect("prstGeom element");
    assert_eq!(xml.attr(pg, "prst"), Some("ellipse"));

    // 3. 再読込してモデルを検証
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let ShapeKind::Auto(re_auto) = &reopened.slides[0].shape(auto_shape).unwrap().kind else { panic!() };
    assert_eq!(re_auto.geometry.preset_name(), Some("ellipse"));

    // 4. Undo / Redo
    assert!(d.undo());
    let ShapeKind::Auto(undone_auto) = &d.presentation().slide(s0).unwrap().shape(auto_shape).unwrap().kind else { panic!() };
    assert_eq!(undone_auto.geometry.preset_name(), Some("rect"));

    assert!(d.redo());
    let ShapeKind::Auto(redone_auto) = &d.presentation().slide(s0).unwrap().shape(auto_shape).unwrap().kind else { panic!() };
    assert_eq!(redone_auto.geometry.preset_name(), Some("ellipse"));
}

#[test]
fn test_sheet_shape_formula_textlink_roundtrip() {
    let (data, _) = open("xlsx-sales.xlsx");
    let mut pkg = ibis_ooxml::Package::open(data).unwrap();

    let sheet_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData>
  <drawing r:id="rId1"/>
</worksheet>"#;
    let rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/>
</Relationships>"#;
    let drawing_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <xdr:twoCellAnchor editAs="twoCell">
    <xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>
    <xdr:to><xdr:col>2</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
    <xdr:sp macro="" textlink="">
      <xdr:nvSpPr>
        <xdr:cNvPr id="2" name="Rectangle 1"/>
        <xdr:cNvSpPr/>
      </xdr:nvSpPr>
      <xdr:spPr>
        <a:xfrm><a:off x="0" y="0"/><a:ext cx="1000000" cy="1000000"/></a:xfrm>
        <a:prstGeom prst="rect"><a:avLst/></a:prstGeom>
      </xdr:spPr>
      <xdr:txBody>
        <a:bodyPr/>
        <a:lstStyle/>
        <a:p><a:r><a:t>KPI Card</a:t></a:r></a:p>
      </xdr:txBody>
    </xdr:sp>
    <xdr:clientData/>
  </xdr:twoCellAnchor>
</xdr:wsDr>"#;

    pkg.set_bytes("xl/worksheets/sheet1.xml", sheet_xml.as_bytes().to_vec());
    pkg.set_bytes("xl/worksheets/_rels/sheet1.xml.rels", rels_xml.as_bytes().to_vec());
    pkg.set_bytes("xl/drawings/drawing1.xml", drawing_xml.as_bytes().to_vec());

    let bytes = pkg.save(Default::default()).unwrap();
    let opened = ibis_pptx::open(bytes).unwrap();
    let s0 = opened.presentation.slides[0].id;
    let auto_shape = opened.presentation.slides[0].shapes.iter().find(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Auto(_))).expect("auto shape").id;

    let mut d = ibis_doc_model::Document::new(opened.presentation.clone());

    // 1. 式 Sheet1!$A$1 を図形に設定
    d.execute(vec![Command::SetShapeFormula {
        slide: s0,
        shape: auto_shape,
        formula: Some("Sheet1!$A$1".into()),
    }]).unwrap();
    assert_eq!(d.presentation().slide(s0).unwrap().shape_formulas.get(&auto_shape).map(|s| s.as_str()), Some("Sheet1!$A$1"));

    // 2. 保存して XML 検証（<f>Sheet1!$A$1</f> が存在）
    let saved = opened.source.save(d.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    let pkg_saved = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = pkg_saved.xml("xl/drawings/drawing1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let f_node = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "f").expect("f element");
    assert_eq!(xml.text(f_node), "Sheet1!$A$1");

    // 3. 再読込してモデルの shape_formulas を検証
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    assert_eq!(reopened.slides[0].shape_formulas.get(&auto_shape).map(|s| s.as_str()), Some("Sheet1!$A$1"));

    // 4. 式を解除（None）して保存
    d.execute(vec![Command::SetShapeFormula {
        slide: s0,
        shape: auto_shape,
        formula: None,
    }]).unwrap();
    assert_eq!(d.presentation().slide(s0).unwrap().shape_formulas.get(&auto_shape), None);

    let saved_cleared = opened.source.save(d.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    let pkg_cleared = ibis_ooxml::Package::open(saved_cleared.clone()).unwrap();
    let xml_cleared = pkg_cleared.xml("xl/drawings/drawing1.xml").unwrap();
    let root_cleared = xml_cleared.root_element().unwrap();
    let f_cleared = xml_cleared.descendants(root_cleared).into_iter().find(|&n| xml_cleared.is_element(n) && xml_cleared.local_name(n) == "f");
    assert!(f_cleared.is_none(), "f element should be removed");

    let reopened_cleared = ibis_pptx::open(saved_cleared).unwrap().presentation;
    assert_eq!(reopened_cleared.slides[0].shape_formulas.get(&auto_shape), None);

    // 5. Undo / Redo 検証
    assert!(d.undo());
    assert_eq!(d.presentation().slide(s0).unwrap().shape_formulas.get(&auto_shape).map(|s| s.as_str()), Some("Sheet1!$A$1"));

    assert!(d.undo());
    assert_eq!(d.presentation().slide(s0).unwrap().shape_formulas.get(&auto_shape), None);

    assert!(d.redo());
    assert_eq!(d.presentation().slide(s0).unwrap().shape_formulas.get(&auto_shape).map(|s| s.as_str()), Some("Sheet1!$A$1"));
}

#[test]
fn test_sheet_shape_warp_roundtrip() {
    let (data, _) = open("xlsx-sales.xlsx");
    let mut pkg = ibis_ooxml::Package::open(data).unwrap();

    let sheet_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheetData><row r="1"><c r="A1" t="s"><v>0</v></c></row></sheetData>
  <drawing r:id="rId1"/>
</worksheet>"#;
    let rels_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/>
</Relationships>"#;
    let drawing_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <xdr:twoCellAnchor editAs="twoCell">
    <xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>
    <xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
    <xdr:sp>
      <xdr:nvSpPr><xdr:cNvPr id="100" name="WordArt 1"/><xdr:cNvSpPr/></xdr:nvSpPr>
      <xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1200000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr>
      <xdr:txBody>
        <a:bodyPr/>
        <a:lstStyle/>
        <a:p><a:r><a:rPr sz="3600" b="1"/><a:t>ワードアート</a:t></a:r></a:p>
      </xdr:txBody>
    </xdr:sp>
    <xdr:clientData/>
  </xdr:twoCellAnchor>
</xdr:wsDr>"#;

    pkg.set_bytes("xl/worksheets/sheet1.xml", sheet_xml.as_bytes().to_vec());
    pkg.set_bytes("xl/worksheets/_rels/sheet1.xml.rels", rels_xml.as_bytes().to_vec());
    pkg.set_bytes("xl/drawings/drawing1.xml", drawing_xml.as_bytes().to_vec());

    let input = pkg.save(ibis_pptx::SaveOptions::default()).unwrap();
    let opened = ibis_pptx::open(input).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s0 = d.presentation().slides[0].id;
    let shape_id = d.presentation().slide(s0).unwrap().shapes.iter().find(|sh| sh.text_body().is_some()).expect("shape found").id;

    // 初期状態: warp is None
    assert_eq!(d.presentation().slide(s0).unwrap().shape(shape_id).unwrap().text_body().unwrap().warp, None);

    // 1. 変形を textWave1 に設定して保存
    let mut body1 = d.presentation().slide(s0).unwrap().shape(shape_id).unwrap().text_body().unwrap().clone();
    body1.warp = Some("textWave1".to_string());
    d.execute(vec![Command::SetTextBody { slide: s0, shape: shape_id, body: body1 }]).unwrap();

    let saved = opened.source.save(d.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    let pkg_saved = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = pkg_saved.xml("xl/drawings/drawing1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let warp_node = xml.descendants(root).into_iter().find(|&n| xml.is_element(n) && xml.local_name(n) == "prstTxWarp").expect("prstTxWarp element");
    assert_eq!(xml.attr(warp_node, "prst"), Some("textWave1"));

    // 2. 再読込してモデルの warp を検証
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    assert_eq!(reopened.slides[0].shape(shape_id).unwrap().text_body().unwrap().warp.as_deref(), Some("textWave1"));

    // 3. 変形を解除 (None) して保存
    let mut body2 = d.presentation().slide(s0).unwrap().shape(shape_id).unwrap().text_body().unwrap().clone();
    body2.warp = None;
    d.execute(vec![Command::SetTextBody { slide: s0, shape: shape_id, body: body2 }]).unwrap();

    let saved_cleared = opened.source.save(d.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    let pkg_cleared = ibis_ooxml::Package::open(saved_cleared.clone()).unwrap();
    let xml_cleared = pkg_cleared.xml("xl/drawings/drawing1.xml").unwrap();
    let root_cleared = xml_cleared.root_element().unwrap();
    let warp_cleared = xml_cleared.descendants(root_cleared).into_iter().find(|&n| xml_cleared.is_element(n) && xml_cleared.local_name(n) == "prstTxWarp");
    assert!(warp_cleared.is_none(), "prstTxWarp element should be removed");

    let reopened_cleared = ibis_pptx::open(saved_cleared).unwrap().presentation;
    assert_eq!(reopened_cleared.slides[0].shape(shape_id).unwrap().text_body().unwrap().warp, None);

    // 4. Undo 検証
    assert!(d.undo());
    assert_eq!(d.presentation().slide(s0).unwrap().shape(shape_id).unwrap().text_body().unwrap().warp.as_deref(), Some("textWave1"));
}

#[test]
fn conditional_databar_negative_and_axis_roundtrip() {
    use ibis_doc_model::{CellRange, CfKind, CfRule, CfValue, ConditionalFormat, DataBarAxisPosition, Rgb, SheetCommand};

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = opened.presentation.slides[0].id;
    let base_len = sheet_of_slide(&opened.presentation.slides[0]).unwrap().conditional.len();

    // 1. データバー規則 2 件（グラデーション負軸付き、単色Midpoint軸付き）を作成
    let rule_gradient = CfRule {
        kind: CfKind::DataBar {
            min: CfValue { kind: "autoMin".into(), value: None, gte: true },
            max: CfValue { kind: "autoMax".into(), value: None, gte: true },
            color: Rgb(0x638EC6),
            gradient: true,
            negative_color: Some(Rgb(0xFF0000)),
            axis_color: Some(Rgb(0x333333)),
            axis_position: Some(DataBarAxisPosition::Automatic),
            min_length: Some(10),
            max_length: Some(90),
            border_color: None,
            negative_border_color: None,
            direction: ibis_doc_model::DataBarDirection::Context,
        },
        priority: 1,
        stop: false,
        style: None,
    };
    let format1 = ConditionalFormat {
        source_index: None,
        ranges: vec![CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 0 }],
        rules: vec![rule_gradient],
    };

    let rule_solid = CfRule {
        kind: CfKind::DataBar {
            min: CfValue { kind: "min".into(), value: None, gte: true },
            max: CfValue { kind: "max".into(), value: None, gte: true },
            color: Rgb(0x63C384),
            gradient: false,
            negative_color: Some(Rgb(0xD00000)),
            axis_color: None,
            axis_position: Some(DataBarAxisPosition::Midpoint),
            min_length: None,
            max_length: None,
            border_color: None,
            negative_border_color: None,
            direction: ibis_doc_model::DataBarDirection::Context,
        },
        priority: 2,
        stop: false,
        style: None,
    };
    let format2 = ConditionalFormat {
        source_index: None,
        ranges: vec![CellRange { first_row: 0, last_row: 5, first_column: 1, last_column: 1 }],
        rules: vec![rule_solid],
    };

    d.execute(vec![
        Command::Sheet(SheetCommand::AddConditional { slide: s, format: format1 }),
        Command::Sheet(SheetCommand::AddConditional { slide: s, format: format2 }),
    ]).unwrap();

    // 2. 保存して XML 検証
    let saved = opened.source.save(d.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = pkg.xml("xl/worksheets/sheet1.xml").unwrap();
    let root = xml.root_element().unwrap();
    let dbs: Vec<_> = xml.descendants(root).into_iter().filter(|&n| xml.is_element(n) && xml.local_name(n) == "dataBar").collect();
    assert!(!dbs.is_empty(), "dataBar elements must exist");

    // 3. 再読込してモデル検証
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let re_sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(re_sheet.conditional.len(), base_len + 2);

    let cf1 = &re_sheet.conditional[base_len].rules[0];
    if let CfKind::DataBar { gradient, negative_color, axis_color, axis_position, min_length, max_length, .. } = &cf1.kind {
        assert!(gradient, "format 1 should be gradient");
        assert_eq!(*negative_color, Some(Rgb(0xFF0000)));
        assert_eq!(*axis_color, Some(Rgb(0x333333)));
        assert_eq!(*axis_position, Some(DataBarAxisPosition::Automatic));
        assert_eq!(*min_length, Some(10));
        assert_eq!(*max_length, Some(90));
    } else {
        panic!("expected DataBar for format 1");
    }

    let cf2 = &re_sheet.conditional[base_len + 1].rules[0];
    if let CfKind::DataBar { gradient, negative_color, axis_position, .. } = &cf2.kind {
        assert!(!gradient, "format 2 should be solid (not gradient)");
        assert_eq!(*negative_color, Some(Rgb(0xD00000)));
        assert_eq!(*axis_position, Some(DataBarAxisPosition::Midpoint));
    } else {
        panic!("expected DataBar for format 2");
    }

    // 4. Undo / Redo
    assert!(d.undo());
    let undone_sheet = sheet_of_slide(&d.presentation().slides[0]).unwrap();
    assert_eq!(undone_sheet.conditional.len(), base_len);

    assert!(d.redo());
    let redone_sheet = sheet_of_slide(&d.presentation().slides[0]).unwrap();
    assert_eq!(redone_sheet.conditional.len(), base_len + 2);
}

#[test]
fn conditional_iconset_custom_no_icon_and_reverse_roundtrip() {
    use ibis_doc_model::{CellRange, CfIcon, CfKind, CfRule, CfValue, Command, ConditionalFormat, Document, SheetCommand};

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = opened.presentation.slides[0].id;
    let base_len = sheet_of_slide(&opened.presentation.slides[0]).unwrap().conditional.len();

    // 1. 個別選択（custom = true）および NoIcon を含む IconSet ルール
    let custom_iconset = ConditionalFormat {
        source_index: None,
        ranges: vec![CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 0 }],
        rules: vec![CfRule {
            kind: CfKind::IconSet {
                set: "3TrafficLights1".into(),
                values: vec![
                    CfValue { kind: "percent".into(), value: Some("0".into()), gte: true },
                    CfValue { kind: "percent".into(), value: Some("33".into()), gte: true },
                    CfValue { kind: "percent".into(), value: Some("67".into()), gte: true },
                ],
                reverse: true,
                show_value: false,
                custom: true,
                icons: vec![
                    Some(CfIcon::new("3TrafficLights1", 0)),
                    Some(CfIcon::no_icon()), // アイコンなし！
                    Some(CfIcon::new("3Flags", 2)), // 別セットのアイコン
                ],
            },
            priority: 1,
            stop: false,
            style: None,
        }],
    };

    // 2. 標準 IconSet ルール（4ArrowsGray, reverse = false, show_value = true）
    let std_iconset = ConditionalFormat {
        source_index: None,
        ranges: vec![CellRange { first_row: 0, last_row: 5, first_column: 1, last_column: 1 }],
        rules: vec![CfRule {
            kind: CfKind::icon_set(
                "4ArrowsGray",
                vec![
                    CfValue { kind: "percent".into(), value: Some("0".into()), gte: true },
                    CfValue { kind: "percent".into(), value: Some("25".into()), gte: true },
                    CfValue { kind: "percent".into(), value: Some("50".into()), gte: true },
                    CfValue { kind: "percent".into(), value: Some("75".into()), gte: true },
                ],
                false,
                true,
            ),
            priority: 2,
            stop: false,
            style: None,
        }],
    };

    d.execute(vec![
        Command::Sheet(SheetCommand::AddConditional { slide: s, format: custom_iconset }),
        Command::Sheet(SheetCommand::AddConditional { slide: s, format: std_iconset }),
    ]).unwrap();

    // 3. 保存と再読み込み
    let saved = save(&opened, d.presentation(), "conditional-iconset-custom-roundtrip.xlsx");
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let re_sheet = sheet_of_slide(&reopened.slides[0]).unwrap();
    assert_eq!(re_sheet.conditional.len(), base_len + 2);

    let cf1 = &re_sheet.conditional[base_len].rules[0];
    if let CfKind::IconSet { set, reverse, show_value, custom, icons, .. } = &cf1.kind {
        assert_eq!(set, "3TrafficLights1");
        assert!(reverse, "expected reverse == true");
        assert!(!show_value, "expected show_value == false");
        assert!(custom, "expected custom == true");
        assert_eq!(icons.len(), 3);
        assert_eq!(icons[0], Some(CfIcon::new("3TrafficLights1", 0)));
        assert!(icons[1].as_ref().unwrap().is_no_icon(), "expected slot 1 to be NoIcon");
        assert_eq!(icons[2], Some(CfIcon::new("3Flags", 2)));
    } else {
        panic!("expected IconSet for format 1");
    }

    let cf2 = &re_sheet.conditional[base_len + 1].rules[0];
    if let CfKind::IconSet { set, reverse, show_value, custom, .. } = &cf2.kind {
        assert_eq!(set, "4ArrowsGray");
        assert!(!reverse);
        assert!(show_value);
        assert!(!custom);
    } else {
        panic!("expected IconSet for format 2");
    }

    // 4. Undo / Redo
    assert!(d.undo());
    let undone_sheet = sheet_of_slide(&d.presentation().slides[0]).unwrap();
    assert_eq!(undone_sheet.conditional.len(), base_len);

    assert!(d.redo());
    let redone_sheet = sheet_of_slide(&d.presentation().slides[0]).unwrap();
    assert_eq!(redone_sheet.conditional.len(), base_len + 2);
}

#[test]
fn csv_shift_jis_utf8_and_tsv_roundtrip() {
    use ibis_pptx::csv::{sheet_to_csv, sheet_to_csv_sjis, sheet_to_tsv, decode};
    use ibis_doc_model::CellValue;

    // 1. Shift_JIS CSV の読み込み・検証
    let sjis_src = "商品名,単価,数量,数式\r\nりんご,150,3,=B2*C2\r\n\"温州,みかん\",80,10,=B3*C3\r\n";
    let (sjis_bytes, _, _) = encoding_rs::SHIFT_JIS.encode(sjis_src);
    let opened = ibis_pptx::open(sjis_bytes.to_vec()).expect("Shift_JIS CSV open succeeds");
    let sheet = sheet_of_slide(&opened.presentation.slides[0]).expect("sheet exists");
    assert_eq!(sheet.cell(0, 0).unwrap().text, "商品名");
    assert_eq!(sheet.cell(1, 0).unwrap().text, "りんご");
    assert_eq!(sheet.cell(1, 1).unwrap().value, CellValue::Number(150.0));
    assert_eq!(sheet.cell(1, 2).unwrap().value, CellValue::Number(3.0));
    assert_eq!(sheet.cell(1, 3).unwrap().formula.as_deref(), Some("B2*C2"));
    assert_eq!(sheet.cell(2, 0).unwrap().text, "温州,みかん");

    // 2. 各種形式へのエクスポート
    // 2a. Shift_JIS CSV 書き出し
    let out_sjis = sheet_to_csv_sjis(&opened.presentation, 0);
    assert!(!out_sjis.starts_with(b"\xEF\xBB\xBF"));
    let decoded_sjis = decode(&out_sjis).expect("decode sjis");
    assert!(decoded_sjis.contains("商品名,単価,数量,数式\r\n"));
    assert!(decoded_sjis.contains("\"温州,みかん\",80,10"));

    // 2b. UTF-8 (BOM付き) CSV 書き出し
    let out_utf8 = sheet_to_csv(&opened.presentation, 0);
    assert!(out_utf8.starts_with(b"\xEF\xBB\xBF"));
    let decoded_utf8 = decode(&out_utf8).expect("decode utf8");
    assert!(decoded_utf8.contains("商品名,単価,数量,数式\r\n"));

    // 2c. TSV (タブ区切り) 書き出し
    let out_tsv = sheet_to_tsv(&opened.presentation, 0);
    let decoded_tsv = decode(&out_tsv).expect("decode tsv");
    assert!(decoded_tsv.contains("商品名\t単価\t数量\t数式\r\n"));
    assert!(decoded_tsv.contains("りんご\t150\t3"));

    // 3. TSV からの読み込み・再構築ラウンドトリップ
    let opened_tsv = ibis_pptx::open(out_tsv).expect("TSV open succeeds");
    let sheet_tsv = sheet_of_slide(&opened_tsv.presentation.slides[0]).expect("tsv sheet exists");
    assert_eq!(sheet_tsv.cell(0, 0).unwrap().text, "商品名");
    assert_eq!(sheet_tsv.cell(1, 0).unwrap().text, "りんご");
    assert_eq!(sheet_tsv.cell(1, 1).unwrap().value, CellValue::Number(150.0));
}

#[test]
fn custom_views_save_and_reopen_roundtrip() {
    use ibis_doc_model::{CustomSheetView, CustomWorkbookView};

    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = d.presentation().slides[0].id;

    let wb_view = CustomWorkbookView {
        name: "MyCustomView".into(),
        guid: "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}".into(),
        include_print_settings: true,
        include_hidden: true,
        active_sheet_index: 0,
    };

    let sheet_view = CustomSheetView {
        guid: "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}".into(),
        name: "MyCustomView".into(),
        zoom: Some(135),
        grid_lines: false,
        headings: false,
        show_ruler: true,
        page_break_preview: true,
        frozen: Some((2, 3)),
        active_cell: Some((1, 1)),
        selection_range: None,
        print: Some(ibis_doc_model::SheetPrint {
            landscape: true,
            scale: 120,
            ..Default::default()
        }),
        hidden_rows: None,
        hidden_columns: None,
        auto_filter: None,
        filtered_rows: None,
        ..Default::default()
    };

    d.execute(vec![Command::Sheet(SheetCommand::AddCustomView {
        view: wb_view.clone(),
        sheet_views: vec![(s, sheet_view.clone())],
    })]).unwrap();

    let saved = save(&opened, d.presentation(), "custom_views_roundtrip.xlsx");
    let reopened = ibis_pptx::open(saved).expect("reopen saved custom views workbook");

    // Check workbook custom views
    let wb = reopened.presentation.workbook.as_ref().expect("workbook exists");
    assert_eq!(wb.custom_views.len(), 1);
    assert_eq!(wb.custom_views[0].name, "MyCustomView");
    assert_eq!(wb.custom_views[0].guid, "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}");
    assert!(wb.custom_views[0].include_print_settings);
    assert!(wb.custom_views[0].include_hidden);

    // Check sheet custom views
    let sheet = sheet_of_slide(&reopened.presentation.slides[0]).expect("sheet exists");
    assert_eq!(sheet.custom_views.len(), 1);
    let sv = &sheet.custom_views[0];
    assert_eq!(sv.name, "MyCustomView");
    assert_eq!(sv.guid, "{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}");
    assert_eq!(sv.zoom, Some(135));
    assert_eq!(sv.grid_lines, false);
    assert_eq!(sv.headings, false);
    assert_eq!(sv.page_break_preview, true);
    assert_eq!(sv.frozen, Some((2, 3)));
    assert_eq!(sv.active_cell, Some((1, 1)));
    assert!(sv.print.as_ref().map_or(false, |p| p.landscape && p.scale == 120));
}

#[test]
fn window_hidden_save_and_reopen_roundtrip() {
    let (_, opened) = open("xlsx-sales.xlsx");
    assert!(!opened.presentation.is_workbook_window_hidden());

    let mut d = Document::new(opened.presentation.clone());

    // 1. Hide workbook window
    d.execute(vec![Command::Sheet(SheetCommand::SetWindowHidden { hidden: true })]).unwrap();
    assert!(d.presentation().is_workbook_window_hidden());

    // 2. Save and reopen
    let saved_hidden = save(&opened, d.presentation(), "window_hidden.xlsx");
    let reopened_hidden = ibis_pptx::open(saved_hidden.clone()).expect("reopen hidden workbook");
    assert!(reopened_hidden.presentation.is_workbook_window_hidden());

    // Verify XML has visibility="hidden" on workbookView
    let pkg = ibis_ooxml::Package::open(saved_hidden).unwrap();
    let xml = String::from_utf8(pkg.read("xl/workbook.xml").unwrap()).unwrap();
    assert!(xml.contains(r#"visibility="hidden""#));

    // 3. Unhide workbook window on reopened document
    let mut d2 = Document::new(reopened_hidden.presentation.clone());
    d2.execute(vec![Command::Sheet(SheetCommand::SetWindowHidden { hidden: false })]).unwrap();
    assert!(!d2.presentation().is_workbook_window_hidden());

    // 4. Save and reopen again
    let saved_unhidden = save(&reopened_hidden, d2.presentation(), "window_unhidden.xlsx");
    let reopened_unhidden = ibis_pptx::open(saved_unhidden.clone()).expect("reopen unhidden workbook");
    assert!(!reopened_unhidden.presentation.is_workbook_window_hidden());

    // Verify XML no longer has visibility="hidden"
    let pkg2 = ibis_ooxml::Package::open(saved_unhidden).unwrap();
    let xml2 = String::from_utf8(pkg2.read("xl/workbook.xml").unwrap()).unwrap();
    assert!(!xml2.contains(r#"visibility="hidden""#));
}

#[test]
fn chart_move_to_existing_sheet_and_new_sheet_roundtrip() {
    use ibis_doc_model::{ChartMoveTarget, ShapeId, ShapeKind};

    // --- Scenario 1: Move chart to existing sheet ---
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sales_slide = slide(d.presentation(), "月次売上");
    let memo_slide = slide(d.presentation(), "メモ");

    let (chart_shape_id, chart_obj) = d
        .presentation()
        .slide(sales_slide)
        .unwrap()
        .shapes
        .iter()
        .find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
            _ => None,
        })
        .expect("chart exists on 月次売上");

    // Move chart to "メモ" sheet
    d.execute(vec![Command::Sheet(SheetCommand::MoveChart {
        from_slide: sales_slide,
        shape: chart_shape_id,
        target: ChartMoveTarget::ExistingSheet { slide: memo_slide },
    })])
    .unwrap();

    // Verify in model before save
    let sales_has_chart = d.presentation().slide(sales_slide).unwrap().shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)));
    assert!(!sales_has_chart, "月次売上 should no longer have the chart");
    let memo_has_chart = d.presentation().slide(memo_slide).unwrap().shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)));
    assert!(memo_has_chart, "メモ should now have the chart");

    // Save and verify
    let saved_existing = save(&opened, d.presentation(), "chart-move-existing.xlsx");
    let reopened = ibis_pptx::open(saved_existing.clone()).expect("open saved chart-move-existing");
    let reopened_sales_chart = reopened.presentation.slide(slide(&reopened.presentation, "月次売上")).unwrap().shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)));
    assert!(!reopened_sales_chart, "reopened 月次売上 should have no chart");
    let (_reopened_memo_shape, reopened_memo_chart) = reopened
        .presentation
        .slide(slide(&reopened.presentation, "メモ"))
        .unwrap()
        .shapes
        .iter()
        .find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
            _ => None,
        })
        .expect("reopened メモ should have the chart");
    assert_eq!(reopened_memo_chart.title, chart_obj.title);

    // Test Undo / Redo
    assert!(d.undo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), data);
    assert!(d.redo());
    assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), saved_existing);

    // --- Scenario 2: Move chart to new sheet ---
    let (data2, opened2) = open("xlsx-sales.xlsx");
    let mut d2 = Document::new(opened2.presentation.clone());
    let sales_slide2 = slide(d2.presentation(), "月次売上");
    let (chart_shape_id2, chart_obj2) = d2
        .presentation()
        .slide(sales_slide2)
        .unwrap()
        .shapes
        .iter()
        .find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
            _ => None,
        })
        .expect("chart exists on 月次売上");

    let new_slide_id = SlideId(9999);
    let new_shape_id = ShapeId(9999);

    d2.execute(vec![Command::Sheet(SheetCommand::MoveChart {
        from_slide: sales_slide2,
        shape: chart_shape_id2,
        target: ChartMoveTarget::NewSheet {
            name: "新グラフシート".into(),
            slide: new_slide_id,
            sheet_shape: new_shape_id,
        },
    })])
    .unwrap();

    assert_eq!(d2.presentation().slides.len(), 4);
    let sales_has_chart2 = d2.presentation().slide(sales_slide2).unwrap().shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)));
    assert!(!sales_has_chart2);
    let new_slide_has_chart = d2.presentation().slide(new_slide_id).unwrap().shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)));
    assert!(new_slide_has_chart);

    let saved_new = save(&opened2, d2.presentation(), "chart-move-new-sheet.xlsx");
    let reopened2 = ibis_pptx::open(saved_new.clone()).expect("open saved chart-move-new-sheet");
    assert_eq!(reopened2.presentation.slides.len(), 4);
    let reopened_new_slide = reopened2.presentation.slide(slide(&reopened2.presentation, "新グラフシート")).expect("新グラフシート slide exists");
    let (_, reopened_new_chart) = reopened_new_slide
        .shapes
        .iter()
        .find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
            _ => None,
        })
        .expect("chart on 新グラフシート");
    assert_eq!(reopened_new_chart.title, chart_obj2.title);

    // Test Undo / Redo for new sheet move
    assert!(d2.undo());
    assert_eq!(d2.presentation().slides.len(), 3);
    assert_eq!(opened2.source.save(d2.presentation(), SaveOptions::default()).unwrap(), data2);
    assert!(d2.redo());
    assert_eq!(d2.presentation().slides.len(), 4);
    assert_eq!(opened2.source.save(d2.presentation(), SaveOptions::default()).unwrap(), saved_new);
}

#[test]
fn chart_series_add_delete_and_reorder_roundtrip() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (shape, chart) = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    let mut refs = opened.source.sheet_chart_refs(shape, &chart).unwrap();
    assert_eq!(refs.len(), 3);
    assert_eq!(chart.groups[0].series.len(), 3);
    assert_eq!(chart.groups[0].series[0].name, "4月");
    assert_eq!(chart.groups[0].series[1].name, "5月");
    assert_eq!(chart.groups[0].series[2].name, "6月");

    // 1. Add 4th series (column E: 合計)
    let new_ref = ibis_pptx::SeriesRefs {
        name: Some("月次売上!$E$3".into()),
        categories: refs[0].categories.clone(),
        values: Some("月次売上!$E$4:$E$8".into()),
        ..Default::default()
    };
    refs.push(new_ref);
    let target = ibis_pptx::sheet_chart_from_data(&chart, d.presentation(), &refs);
    assert_eq!(target.groups[0].series.len(), 4);
    assert_eq!(target.groups[0].series[3].name, "合計");
    opened.source.set_chart_data(shape, target.clone(), ibis_pptx::ChartData { workbook: Vec::new(), series: refs.clone() });
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(target.clone()))) }]).unwrap();

    // 2. Reorder: swap series 0 ("4月") and series 1 ("5月")
    refs.swap(0, 1);
    let mut swapped_chart = target.clone();
    swapped_chart.groups[0].series.swap(0, 1);
    let target_swapped = ibis_pptx::sheet_chart_from_data(&swapped_chart, d.presentation(), &refs);
    assert_eq!(target_swapped.groups[0].series[0].name, "5月");
    assert_eq!(target_swapped.groups[0].series[1].name, "4月");
    opened.source.set_chart_data(shape, target_swapped.clone(), ibis_pptx::ChartData { workbook: Vec::new(), series: refs.clone() });
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(target_swapped.clone()))) }]).unwrap();

    // 3. Delete 4th series
    refs.pop();
    let mut deleted_chart = target_swapped.clone();
    deleted_chart.groups[0].series.pop();
    let target_deleted = ibis_pptx::sheet_chart_from_data(&deleted_chart, d.presentation(), &refs);
    assert_eq!(target_deleted.groups[0].series.len(), 3);
    opened.source.set_chart_data(shape, target_deleted.clone(), ibis_pptx::ChartData { workbook: Vec::new(), series: refs.clone() });
    d.execute(vec![Command::SetKind { slide: s, shape, kind: Box::new(ibis_doc_model::ShapeKind::Chart(Box::new(target_deleted.clone()))) }]).unwrap();

    // 4. Save and read back
    let bytes = save(&opened, d.presentation(), "chart-series-modify.xlsx");
    let read = ibis_pptx::open(bytes).unwrap();
    let (rs, rc) = read.presentation.slide(slide(&read.presentation, "月次売上")).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())), _ => None }).unwrap();
    assert_eq!(read.source.sheet_chart_refs(rs, &rc).unwrap(), refs);
    assert_eq!(rc.groups[0].series.len(), 3);
    assert_eq!(rc.groups[0].series[0].name, "5月");
    assert_eq!(rc.groups[0].series[1].name, "4月");
    assert_eq!(rc.groups[0].series[2].name, "6月");

    // 5. Undo / Redo
    assert!(d.undo());
    let curr = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()), _ => None }).unwrap();
    assert_eq!(curr.groups[0].series.len(), 4);
    assert!(d.redo());
    let curr = d.presentation().slide(s).unwrap().shapes.iter().find_map(|sh| match &sh.kind { ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()), _ => None }).unwrap();
    assert_eq!(curr.groups[0].series.len(), 3);
}#[test]
fn test_chart_template_crtx_roundtrip() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let (_, chart) = d
        .presentation()
        .slide(s)
        .unwrap()
        .shapes
        .iter()
        .find_map(|sh| match &sh.kind {
            ibis_doc_model::ShapeKind::Chart(c) => Some((sh.id, (**c).clone())),
            _ => None,
        })
        .unwrap();

    // 1. グラフをカスタマイズ（スタイル、系列0の近似曲線、系列1の塗りつぶし、系列2の誤差範囲）
    let mut custom_chart = chart.clone();
    custom_chart.style = Some(12);
    custom_chart.groups[0].series[0].trendline = Some(ibis_doc_model::chart::Trendline {
        kind: ibis_doc_model::chart::TrendlineType::Linear,
        order: 2,
        period: 2,
        disp_eq: true,
        disp_r_sqr: true,
        stroke: None,
    });
    custom_chart.groups[0].series[1].fill = ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472C4)));
    custom_chart.groups[0].series[2].error_bars = Some(ibis_doc_model::chart::ErrorBars {
        direction: ibis_doc_model::chart::ErrorBarDirection::Y,
        bar_type: ibis_doc_model::chart::ErrorBarType::Both,
        val_type: ibis_doc_model::chart::ErrorBarValueType::Percentage,
        no_end_cap: false,
        value: Some(ibis_doc_model::Number(5.0)),
        stroke: None,
        ..Default::default()
    });

    // 2. .crtx パッケージを生成
    let crtx_bytes = ibis_pptx::chart_to_crtx(&custom_chart).expect("chart_to_crtx succeeds");
    assert!(!crtx_bytes.is_empty());
    assert_eq!(&crtx_bytes[..2], b"PK", "crtx must be a ZIP package");

    // 3. ZIP 構造検証
    let archive = ibis_ooxml::zip::Archive::new(std::sync::Arc::from(crtx_bytes.as_slice())).expect("valid zip");
    let names: Vec<_> = archive.entries().iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"[Content_Types].xml"));
    assert!(names.contains(&"_rels/.rels"));
    assert!(names.contains(&"chart/chart.xml"));

    // 4. .crtx を読み込んで Chart モデルに復元
    let template = ibis_pptx::read_crtx(&crtx_bytes).expect("read_crtx succeeds");
    assert_eq!(template.style, Some(12));
    assert!(template.groups[0].series[0].trendline.is_some());
    assert_eq!(template.groups[0].series[0].trendline.as_ref().unwrap().kind, ibis_doc_model::chart::TrendlineType::Linear);
    assert_eq!(template.groups[0].series[1].fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472C4))));
    assert!(template.groups[0].series[2].error_bars.is_some());

    // 5. 別のデータを持つ対象グラフにテンプレートを適用
    let mut other_chart = chart.clone();
    other_chart.style = Some(1);
    other_chart.groups[0].series[0].name = "東京本店".into();
    other_chart.groups[0].series[0].trendline = None;
    other_chart.groups[0].series[1].name = "大阪支店".into();
    other_chart.groups[0].series[2].name = "名古屋支店".into();

    ibis_pptx::apply_chart_template(&mut other_chart, &template);

    // テンプレートの書式・スタイルが適用され、対象グラフの系列名やデータは保持されることを検証
    assert_eq!(other_chart.style, Some(12));
    assert_eq!(other_chart.groups[0].series[0].name, "東京本店");
    assert!(other_chart.groups[0].series[0].trendline.is_some());
    assert_eq!(other_chart.groups[0].series[1].name, "大阪支店");
    assert_eq!(other_chart.groups[0].series[1].fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472C4))));
    assert_eq!(other_chart.groups[0].series[2].name, "名古屋支店");
    assert!(other_chart.groups[0].series[2].error_bars.is_some());
}

#[test]
fn test_surface_and_contour_charts_roundtrip() {
    // 1. ECMA-376 Part 1 §21.2.2.203/204 の surface3DChart と surfaceChart XML を作成
    let surface_3d_xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
        <c:chart>
            <c:view3D><c:rotX val="30"/><c:rotY val="30"/><c:rAngAx val="1"/></c:view3D>
            <c:plotArea>
                <c:surface3DChart>
                    <c:wireframe val="0"/>
                    <c:ser>
                        <c:idx val="0"/><c:order val="0"/>
                        <c:tx><c:v>系列1</c:v></c:tx>
                        <c:cat><c:strLit><c:ptCount val="3"/><c:pt idx="0"><c:v>X1</c:v></c:pt><c:pt idx="1"><c:v>X2</c:v></c:pt><c:pt idx="2"><c:v>X3</c:v></c:pt></c:strLit></c:cat>
                        <c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="3"/><c:pt idx="0"><c:v>10</c:v></c:pt><c:pt idx="1"><c:v>20</c:v></c:pt><c:pt idx="2"><c:v>30</c:v></c:pt></c:numLit></c:val>
                    </c:ser>
                    <c:ser>
                        <c:idx val="1"/><c:order val="1"/>
                        <c:tx><c:v>系列2</c:v></c:tx>
                        <c:cat><c:strLit><c:ptCount val="3"/><c:pt idx="0"><c:v>X1</c:v></c:pt><c:pt idx="1"><c:v>X2</c:v></c:pt><c:pt idx="2"><c:v>X3</c:v></c:pt></c:strLit></c:cat>
                        <c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="3"/><c:pt idx="0"><c:v>15</c:v></c:pt><c:pt idx="1"><c:v>25</c:v></c:pt><c:pt idx="2"><c:v>35</c:v></c:pt></c:numLit></c:val>
                    </c:ser>
                    <c:axId val="100000001"/>
                    <c:axId val="100000002"/>
                    <c:axId val="100000003"/>
                </c:surface3DChart>
                <c:catAx><c:axId val="100000001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:catAx>
                <c:valAx><c:axId val="100000002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="100000001"/></c:valAx>
                <c:serAx><c:axId val="100000003"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:serAx>
            </c:plotArea>
            <c:legend><c:legendPos val="r"/><c:overlay val="0"/></c:legend>
        </c:chart>
    </c:chartSpace>"#;

    let original = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let bytes = ibis_pptx::test_files::xlsx_with_chart_xml(&original, "xl/charts/chart1.xml", surface_3d_xml);

    // 2. 読込: surface3DChart が ChartType::Surface { is_3d: true, wireframe: false } としてパースされることを検証
    let opened = ibis_pptx::open(bytes).expect("open XLSX with surface3DChart");
    let chart = opened.presentation.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).expect("chart found");
    assert_eq!(chart.groups[0].kind, ibis_doc_model::chart::ChartType::Surface { is_3d: true, wireframe: false });
    assert_eq!(chart.groups[0].series.len(), 2);

    // 3. 編集: 等高線ワイヤーフレーム (2D, wireframe: true) に変更して保存
    let mut edited_p = opened.presentation.clone();
    for sh in &mut edited_p.slides[0].shapes {
        if let ibis_doc_model::ShapeKind::Chart(ref mut c) = sh.kind {
            let mut chart_clone = (**c).clone();
            chart_clone.groups[0].kind = ibis_doc_model::chart::ChartType::Surface { is_3d: false, wireframe: true };
            *c = Box::new(chart_clone);
        }
    }
    let saved_bytes = opened.source.save(&edited_p, ibis_pptx::SaveOptions::default()).expect("save succeeds");

    // 4. 再読込: 保存した XLSX から ChartType::Surface { is_3d: false, wireframe: true } として正しく復元されることを検証
    let reopened = ibis_pptx::open(saved_bytes).expect("reopen saved XLSX");
    let reopened_chart = reopened.presentation.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).expect("reopened chart found");
    assert_eq!(reopened_chart.groups[0].kind, ibis_doc_model::chart::ChartType::Surface { is_3d: false, wireframe: true });

    // 5. テンプレート XML シリアライズ検証 (chart_to_xml)
    let xml_3d = ibis_pptx::chart_to_xml(&chart).expect("chart_to_xml for 3D surface");
    assert!(xml_3d.contains("<c:surface3DChart><c:wireframe val=\"0\"/>"));
    let xml_contour = ibis_pptx::chart_to_xml(&reopened_chart).expect("chart_to_xml for contour wireframe");
    assert!(xml_contour.contains("<c:surfaceChart><c:wireframe val=\"1\"/>"));
}

#[test]
fn test_pivot_chart_source_and_roundtrip() {
    let pivot_chart_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
    <c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
        <c:pivotSource>
            <c:name>PivotTable1</c:name>
            <c:fmtId val="0"/>
        </c:pivotSource>
        <c:chart>
            <c:plotArea>
                <c:barChart>
                    <c:barDir val="col"/>
                    <c:grouping val="clustered"/>
                    <c:ser>
                        <c:idx val="0"/><c:order val="0"/>
                        <c:tx><c:v>売上</c:v></c:tx>
                        <c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>東京</c:v></c:pt><c:pt idx="1"><c:v>大阪</c:v></c:pt></c:strLit></c:cat>
                        <c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="2"/><c:pt idx="0"><c:v>100</c:v></c:pt><c:pt idx="1"><c:v>200</c:v></c:pt></c:numLit></c:val>
                    </c:ser>
                    <c:axId val="100000001"/>
                    <c:axId val="100000002"/>
                </c:barChart>
                <c:catAx><c:axId val="100000001"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="100000002"/></c:catAx>
                <c:valAx><c:axId val="100000002"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="100000001"/></c:valAx>
            </c:plotArea>
            <c:legend><c:legendPos val="r"/><c:overlay val="0"/></c:legend>
        </c:chart>
    </c:chartSpace>"#;

    let original = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let bytes = ibis_pptx::test_files::xlsx_with_chart_xml(&original, "xl/charts/chart1.xml", pivot_chart_xml);

    // 1. 読込: pivotSource がパースされ、chart.pivot_source に格納される
    let opened = ibis_pptx::open(bytes).expect("open XLSX with pivotSource");
    let chart = opened.presentation.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).expect("chart found");
    assert!(chart.is_pivot_chart());
    assert_eq!(chart.pivot_source.as_deref(), Some("PivotTable1"));

    // 2. 編集: pivot_source を変更して保存
    let mut edited_p = opened.presentation.clone();
    for sh in &mut edited_p.slides[0].shapes {
        if let ibis_doc_model::ShapeKind::Chart(ref mut c) = sh.kind {
            let mut chart_clone = (**c).clone();
            chart_clone.pivot_source = Some("PivotTable2".to_string());
            *c = Box::new(chart_clone);
        }
    }
    let saved_bytes = opened.source.save(&edited_p, ibis_pptx::SaveOptions::default()).expect("save succeeds");

    // 3. 再読込: 変更された pivotSource が保存・復元される
    let reopened = ibis_pptx::open(saved_bytes).expect("reopen saved XLSX");
    let reopened_chart = reopened.presentation.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).expect("reopened chart found");
    assert!(reopened_chart.is_pivot_chart());
    assert_eq!(reopened_chart.pivot_source.as_deref(), Some("PivotTable2"));

    // 4. pivot_source を None に変更して保存 (削除)
    let mut unpivot_p = reopened.presentation.clone();
    for sh in &mut unpivot_p.slides[0].shapes {
        if let ibis_doc_model::ShapeKind::Chart(ref mut c) = sh.kind {
            let mut chart_clone = (**c).clone();
            chart_clone.pivot_source = None;
            *c = Box::new(chart_clone);
        }
    }
    let unpivot_bytes = reopened.source.save(&unpivot_p, ibis_pptx::SaveOptions::default()).expect("save unpivot succeeds");
    let unpivot_opened = ibis_pptx::open(unpivot_bytes).expect("reopen unpivot XLSX");
    let unpivot_chart = unpivot_opened.presentation.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    }).expect("unpivot chart found");
    assert!(!unpivot_chart.is_pivot_chart());
    assert_eq!(unpivot_chart.pivot_source, None);

    // 5. テンプレート XML シリアライズ検証 (chart_to_xml)
    let xml_pivot = ibis_pptx::chart_to_xml(&chart).expect("chart_to_xml with pivotSource");
    assert!(xml_pivot.contains("<c:pivotSource><c:name>PivotTable1</c:name><c:fmtId val=\"0\"/></c:pivotSource>"));
    let xml_unpivot = ibis_pptx::chart_to_xml(&unpivot_chart).expect("chart_to_xml without pivotSource");
    assert!(!xml_unpivot.contains("<c:pivotSource>"));
}

#[test]
fn test_cell_phonetic_and_roundtrip() {
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = d.presentation().slides[0].id;

    // 1. セル A10 に "東京" を設定
    d.execute(vec![Command::Sheet(SheetCommand::SetCells {
        slide: s,
        edits: vec![CellEdit {
            row: 9,
            column: 0,
            input: CellInput::Value(CellValue::Text("東京".into())),
        }],
    })]).unwrap();

    // 2. A10 にふりがな "トウキョウ" を設定
    d.execute(vec![Command::Sheet(SheetCommand::SetCellPhonetic {
        slide: s,
        row: 9,
        column: 0,
        phonetic: Some("トウキョウ".into()),
    })]).unwrap();

    // 3. ふりがな表示を有効化
    d.execute(vec![Command::Sheet(SheetCommand::SetShowPhonetic {
        slide: s,
        show: true,
    })]).unwrap();

    // 4. phoneticPr を設定
    use ibis_doc_model::{PhoneticProperties, PhoneticType, PhoneticAlignment};
    let props = PhoneticProperties {
        font_id: 0,
        phonetic_type: PhoneticType::Hiragana,
        alignment: PhoneticAlignment::Center,
    };
    d.execute(vec![Command::Sheet(SheetCommand::SetPhoneticProperties {
        slide: s,
        properties: Some(props),
    })]).unwrap();

    // 5. 保存
    let saved = save(&opened, d.presentation(), "cell-phonetic.xlsx");

    // 6. 再読み込み
    let reopened = ibis_pptx::open(saved).unwrap();
    let re_slide = reopened.presentation.slide(s).unwrap();
    let re_sheet = sheet_of_slide(re_slide).unwrap();

    // 7. 検証
    let re_cell = re_sheet.cell(9, 0).unwrap();
    assert_eq!(re_cell.value, CellValue::Text("東京".into()));
    assert_eq!(re_cell.phonetic.as_deref(), Some("トウキョウ"));
    assert!(re_sheet.show_phonetic);
    assert_eq!(re_sheet.phonetic_pr, Some(props));
}



fn slicers(p: &Presentation) -> Vec<(ibis_doc_model::EmuRect, ibis_doc_model::slicer::Slicer)> {
    p.slides.iter().flat_map(|s| s.shapes.iter()).filter_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Slicer(s) => Some((sh.frame, (**s).clone())),
        _ => None,
    }).collect()
}

#[test]
fn slicers_made_by_excel_are_read_with_their_items_and_kept_by_the_save() {
    let (data, opened) = open("xlsx-slicers-excel.xlsx");
    let found = slicers(&opened.presentation);
    assert_eq!(found.len(), 2);
    for (frame, _) in &found {
        assert!(frame.width > 0 && frame.height > 0);
    }
    // The pivot table's slicer (an `a14` choice in its anchor) and the table's (`sle15`).
    let product = &found.iter().find(|(_, s)| s.name == "商品").expect("the pivot slicer").1;
    assert_eq!(product.source, ibis_doc_model::slicer::SlicerSource::Pivot { field: "商品".into(), pivots: vec!["ピボットテーブル1".into()] });
    assert_eq!(product.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    assert!(product.items.iter().all(|i| i.selected));
    let region = &found.iter().find(|(_, s)| s.name == "地域").expect("the table slicer").1;
    assert_eq!(region.source, ibis_doc_model::slicer::SlicerSource::Table { table: "売上表".into(), field: "地域".into() });
    let mut labels: Vec<&str> = region.items.iter().map(|i| i.label.as_str()).collect();
    labels.sort();
    assert_eq!(labels, ["北", "南", "東", "西"]);
    // Saved with nothing changed, the slicers' parts are as they were.
    let saved = save(&opened, &opened.presentation, "slicers-unchanged.xlsx");
    assert_eq!(changed_parts(&data, &saved), Vec::<String>::new());
    let again = ibis_pptx::open(saved).unwrap();
    assert_eq!(slicers(&again.presentation).len(), 2);
}

#[test]
fn deleted_slicers_leave_the_drawing_and_their_parts() {
    let (_data, opened) = open("xlsx-slicers-excel.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let remove = |d: &mut Document, name: &str| {
        let (slide, shape) = d.presentation().slides.iter().find_map(|s| s.shapes.iter().find(|sh| matches!(&sh.kind, ibis_doc_model::ShapeKind::Slicer(x) if x.name == name)).map(|sh| (s.id, sh.id))).unwrap();
        d.execute(vec![Command::RemoveShape { slide, shape }]).unwrap();
    };
    remove(&mut d, "商品");
    let saved = save(&opened, d.presentation(), "slicers-one-deleted.xlsx");
    let again = ibis_pptx::open(saved.clone()).unwrap();
    let left = slicers(&again.presentation);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].1.name, "地域");
    let package = ibis_ooxml::Package::open(saved).unwrap();
    assert!(package.contains("xl/slicerCaches/slicerCache2.xml"));
    assert!(!package.contains("xl/slicerCaches/slicerCache1.xml"), "the deleted slicer's cache goes");
    let workbook = String::from_utf8(package.read("xl/workbook.xml").unwrap()).unwrap();
    assert!(!workbook.contains("スライサー_商品") && workbook.contains("スライサー_地域"), "its name goes: {workbook}");
    // The last one too: no slicer part is left, and the extension lists with them.
    remove(&mut d, "地域");
    let saved = save(&opened, d.presentation(), "slicers-all-deleted.xlsx");
    let again = ibis_pptx::open(saved.clone()).unwrap();
    assert!(slicers(&again.presentation).is_empty());
    let package = ibis_ooxml::Package::open(saved).unwrap();
    assert!(package.names().all(|n| !n.contains("slicer")), "{:?}", package.names().collect::<Vec<_>>());
    let workbook = String::from_utf8(package.read("xl/workbook.xml").unwrap()).unwrap();
    assert!(!workbook.contains("licerCache") && !workbook.contains("スライサー_"), "{workbook}");
    for part in ["xl/worksheets/sheet1.xml", "xl/worksheets/sheet2.xml"] {
        let sheet = String::from_utf8(package.read(part).unwrap()).unwrap();
        assert!(!sheet.contains("slicerList"), "{part}: {sheet}");
    }
    let drawing = String::from_utf8(package.read("xl/drawings/drawing1.xml").unwrap()).unwrap();
    assert!(!drawing.contains("slicer") && !drawing.contains("AlternateContent"), "{drawing}");
}

#[test]
fn external_workbook_references_show_the_cache_and_save_by_number() {
    let (data, opened) = open("xlsx-external-link.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "ピボット");
    // Read: the formulas name the workbook as Excel shows it; the cached value stays.
    assert_eq!(formula(d.presentation(), "ピボット", 44, 9).as_deref(), Some("SUM([予算.xlsx]Data!$A$1:$A$3)"));
    assert_eq!(value(d.presentation(), "ピボット", 44, 9), CellValue::Number(60.0));
    assert_eq!(formula(d.presentation(), "ピボット", 44, 10).as_deref(), Some("[予算.xlsx]Data!B1"));
    assert_eq!(value(d.presentation(), "ピボット", 44, 10), CellValue::Text("北部".into()));
    // Typed in: calculated from the link's cached cells.
    set(&mut d, sheet, vec![
        (45, 9, CellInput::Formula("SUM([予算.xlsx]Data!$A$1:$A$3)+1".into())),
        (45, 10, CellInput::Formula("[予算.xlsx]Data!A2*2".into())),
        (45, 11, CellInput::Formula("[予算.xlsx]Data!B1&\"!\"".into())),
        (46, 9, CellInput::Formula("[予算.xlsx]Other!A1".into())),
        (46, 10, CellInput::Formula("[予算.xlsx]Data!Z9".into())),
    ]);
    assert_eq!(value(d.presentation(), "ピボット", 45, 9), CellValue::Number(61.0));
    assert_eq!(value(d.presentation(), "ピボット", 45, 10), CellValue::Number(40.0));
    assert_eq!(value(d.presentation(), "ピボット", 45, 11), CellValue::Text("北部!".into()));
    assert_eq!(value(d.presentation(), "ピボット", 46, 9), CellValue::Error(ibis_doc_model::CellError::Ref), "a sheet the link does not cache");
    assert_eq!(value(d.presentation(), "ピボット", 46, 10), CellValue::Number(0.0), "a cell it does not cache is empty");
    // Edited cells that depend on the cells of the link are calculated again.
    set(&mut d, sheet, vec![(44, 11, CellInput::Formula("J45*3".into()))]);
    assert_eq!(value(d.presentation(), "ピボット", 44, 11), CellValue::Number(180.0));
    // Saved: the formulas by number, the link's part as it was.
    let saved = save(&opened, d.presentation(), "external-link.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("<x:f>SUM([1]Data!$A$1:$A$3)+1</x:f>") || xml.contains("SUM([1]Data!$A$1:$A$3)+1"), "{xml}");
    assert!(xml.contains("[1]Data!A2*2") && !xml.contains("予算.xlsx"), "no workbook name in the file's formulas");
    let changed = changed_parts(&data, &saved);
    assert!(changed.contains(&"xl/worksheets/sheet1.xml".to_string()) && changed.iter().all(|p| !p.contains("externalLink")), "{changed:?}");
    let again = ibis_pptx::open(saved).unwrap();
    assert_eq!(formula(&again.presentation, "ピボット", 45, 10).as_deref(), Some("[予算.xlsx]Data!A2*2"));
    assert_eq!(value(&again.presentation, "ピボット", 45, 10), CellValue::Number(40.0));
    // A workbook the file has no link to cannot be saved yet.
    set(&mut d, sheet, vec![(47, 9, CellInput::Formula("[新規.xlsx]Data!A1".into()))]);
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).is_err());
}

#[test]
fn external_links_are_updated_and_broken_and_saved() {
    use ibis_doc_model::{ExternalSheet, SheetCommand};
    let (_data, opened) = open("xlsx-external-link.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let books = d.presentation().workbook.as_ref().unwrap().external_books.clone();
    assert_eq!((books[0].name.as_str(), books[1].name.as_str()), ("予算.xlsx", "actual.xlsx"));
    assert_eq!(books[0].path, "file:///C:/data/%E4%BA%88%E7%AE%97.xlsx");
    assert_eq!(formula(d.presentation(), "ピボット", 44, 12).as_deref(), Some("[actual.xlsx]Sales!A1*10"));
    // 値の更新: the cached cells replaced; the formulas reading them calculated again.
    let mut sheet = books[0].sheets[0].clone();
    sheet.cells.insert((0, 0), CellValue::Number(100.0));
    d.execute(vec![Command::Sheet(SheetCommand::RefreshExternalBook { index: 0, sheets: vec![ExternalSheet { ..sheet }] })]).unwrap();
    assert_eq!(value(d.presentation(), "ピボット", 44, 9), CellValue::Number(150.0));
    assert_eq!(value(d.presentation(), "ピボット", 44, 11), CellValue::Number(300.0), "what reads the formula follows");
    let saved = save(&opened, d.presentation(), "external-refreshed.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let link = String::from_utf8(package.read("xl/externalLinks/externalLink1.xml").unwrap()).unwrap();
    assert!(link.contains("<v>100</v>") && !link.contains("<v>10</v>"), "{link}");
    // リンクの解除 of the first: its formulas become values, the second is [1] now.
    d.execute(vec![Command::Sheet(SheetCommand::BreakExternalLink { index: 0 })]).unwrap();
    assert_eq!(formula(d.presentation(), "ピボット", 44, 9), None);
    assert_eq!(value(d.presentation(), "ピボット", 44, 9), CellValue::Number(150.0), "the value stays");
    assert_eq!(formula(d.presentation(), "ピボット", 44, 12).as_deref(), Some("[actual.xlsx]Sheet1!A1*10").filter(|_| false).or(Some("[actual.xlsx]Sales!A1*10")));
    let saved = save(&opened, d.presentation(), "external-broken.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    assert!(!package.contains("xl/externalLinks/externalLink1.xml"), "the broken link's part goes");
    assert!(package.contains("xl/externalLinks/externalLink2.xml"));
    let workbook = String::from_utf8(package.read("xl/workbook.xml").unwrap()).unwrap();
    assert_eq!(workbook.matches("externalReference ").count(), 1, "{workbook}");
    let sheet_xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet_xml.contains("[1]Sales!A1*10") && !sheet_xml.contains("[2]"), "the second link is [1] now: {sheet_xml}");
    assert!(!sheet_xml.contains("Data!"), "no formula reads the broken link");
    let again = ibis_pptx::open(saved).unwrap();
    assert_eq!(again.presentation.workbook.as_ref().unwrap().external_books.len(), 1);
    assert_eq!(formula(&again.presentation, "ピボット", 44, 12).as_deref(), Some("[actual.xlsx]Sales!A1*10"));
    assert_eq!(value(&again.presentation, "ピボット", 44, 9), CellValue::Number(150.0));
}

#[test]
fn ignored_errors_are_saved_and_read_back() {
    use ibis_doc_model::error_check::{ErrorKind, flagged_cells};
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    set(&mut d, sheet, vec![(20, 0, CellInput::Value(CellValue::Text("123".into()))), (21, 0, CellInput::Value(CellValue::Text("45".into())))]);
    let flagged = |p: &Presentation| flagged_cells(sheet_of_slide(p.slide(slide(p, "月次売上")).unwrap()).unwrap());
    assert_eq!(flagged(d.presentation()), vec![(20, 0, ErrorKind::NumberAsText), (21, 0, ErrorKind::NumberAsText)]);
    d.execute(vec![Command::Sheet(SheetCommand::IgnoreErrors { slide: sheet, range: CellRange { first_row: 20, last_row: 20, first_column: 0, last_column: 0 }, kind: ErrorKind::NumberAsText })]).unwrap();
    assert_eq!(flagged(d.presentation()), vec![(21, 0, ErrorKind::NumberAsText)]);
    let saved = save(&opened, d.presentation(), "ignored-errors.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains("<ignoredErrors><ignoredError sqref=\"A21\" numberStoredAsText=\"1\"/></ignoredErrors>"), "{xml}");
    let again = ibis_pptx::open(saved).unwrap();
    assert_eq!(flagged(&again.presentation), vec![(21, 0, ErrorKind::NumberAsText)]);
    // Undo: marked again.
    assert!(d.undo());
    assert_eq!(flagged(d.presentation()).len(), 2);
}

#[test]
fn a_pivot_slicers_selection_is_saved_in_its_cache() {
    let (_data, opened) = open("xlsx-slicers-excel.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let slide_id = d.presentation().slides.iter().find(|s| sheet_of_slide(s).is_some_and(|sh| !sh.pivots.is_empty())).unwrap().id;
    // A hidden in the pivot table (what the slicer's button does): the slicer follows.
    let mut sheet = (**sheet_of_slide(d.presentation().slide(slide_id).unwrap()).unwrap()).clone();
    sheet.pivots[0].fields[1].items[0].hidden = true;
    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: slide_id, sheet: Box::new(sheet) })]).unwrap();
    let product = slicers(d.presentation()).into_iter().find(|(_, s)| s.name == "商品").unwrap().1;
    assert_eq!(product.items.iter().map(|i| i.selected).collect::<Vec<_>>(), [false, true]);
    let saved = save(&opened, d.presentation(), "slicer-selection.xlsx");
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let cache = String::from_utf8(package.read("xl/slicerCaches/slicerCache1.xml").unwrap()).unwrap();
    assert!(cache.contains("<i x=\"0\"/>") && cache.contains("<i x=\"1\" s=\"1\"/>"), "{cache}");
    // The pivot cache is as it was (its records too): the slicers find it by the number in its extension.
    let definition = String::from_utf8(package.read("xl/pivotCache/pivotCacheDefinition1.xml").unwrap()).unwrap();
    assert!(definition.contains("pivotCacheId=\"505006826\"") && definition.contains("recordCount=\"8\""), "{definition}");
}

#[test]
fn a_rewritten_pivot_cache_keeps_its_extensions() {
    let (_data, opened) = open("xlsx-slicers-excel.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let slide_id = d.presentation().slides.iter().find(|s| sheet_of_slide(s).is_some_and(|sh| !sh.pivots.is_empty())).unwrap().id;
    // A shared item the cache did not have (and the table's field showing it): only added to the cache's, the records stay.
    let mut sheet = (**sheet_of_slide(d.presentation().slide(slide_id).unwrap()).unwrap()).clone();
    let field = &mut sheet.pivots[0].fields[1];
    field.shared.push(CellValue::Text("Z".into()));
    let at = field.items.len() - 1;
    field.items.insert(at, ibis_doc_model::PivotItem { x: Some(2), hidden: false, kind: None });
    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: slide_id, sheet: Box::new(sheet) })]).unwrap();
    let saved = save(&opened, d.presentation(), "pivot-cache-rewritten.xlsx");
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let definition = String::from_utf8(package.read("xl/pivotCache/pivotCacheDefinition1.xml").unwrap()).unwrap();
    assert!(definition.contains("505006826") && definition.contains("pivotCacheDefinition"), "{definition}");
    assert!(definition.contains("<s v=\"Z\"/>") && definition.contains("count=\"3\""), "the item is added: {definition}");
    assert!(definition.contains("recordCount=\"8\""), "the records stay: {definition}");
    // A cache whose items changed otherwise is written again, with the extensions it had.
    let mut sheet = (**sheet_of_slide(d.presentation().slide(slide_id).unwrap()).unwrap()).clone();
    sheet.pivots[0].fields[1].shared.reverse();
    sheet.pivots[0].fields[1].items.reverse();
    d.execute(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide: slide_id, sheet: Box::new(sheet) })]).unwrap();
    let saved = save(&opened, d.presentation(), "pivot-cache-rewritten-anew.xlsx");
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let definition = String::from_utf8(package.read("xl/pivotCache/pivotCacheDefinition1.xml").unwrap()).unwrap();
    assert!(definition.contains("505006826"), "{definition}");
}

#[test]
fn pictures_in_cells_are_drawn_in_their_cells() {
    // The workbook Excel made (セルに配置 twice); skipped while the file is not in the tree.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-cell-images-excel.xlsx");
    if !path.exists() {
        return;
    }
    let (data, opened) = open("xlsx-cell-images-excel.xlsx");
    let slide_ids: Vec<_> = opened.presentation.slides.iter().map(|s| s.id).collect();
    let mut found = 0;
    for id in slide_ids {
        let slide = opened.presentation.slide(id).unwrap();
        let Some(sheet) = sheet_of_slide(slide) else { continue };
        for image in &sheet.cell_images {
            let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
            let cell = ibis_doc_model::EmuRect::new(xs[image.column as usize], ys[image.row as usize], xs[image.column as usize + 1] - xs[image.column as usize], ys[image.row as usize + 1] - ys[image.row as usize]);
            let pictures: Vec<_> = slide.shapes.iter().filter(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_)) && cell.x <= s.frame.x + 1 && s.frame.right() <= cell.right() + 1 && cell.y <= s.frame.y + 1 && s.frame.bottom() <= cell.bottom() + 1).collect();
            assert_eq!(pictures.len(), 1, "a picture inside cell {image:?}");
            found += 1;
        }
    }
    assert!(found >= 1, "the file has pictures in cells");
    // The row of the second pair is 97 pt tall though nothing in it shows (the pictures' cells hold no text): its height
    // is kept.
    let slide = opened.presentation.slides.first().unwrap();
    let sheet = sheet_of_slide(slide).unwrap();
    // (Excel for Windows reads a custom height at a whole 96 dpi pixel: 97 pt is 129.33 px, 129 px = 96.75 pt.)
    assert_eq!(sheet.rows.get(1).copied(), Some((96.75 * 12_700.0) as i64));
    // Saved as it was: nothing changes.
    let saved = save(&opened, &opened.presentation, "cell-images-unchanged.xlsx");
    assert_eq!(changed_parts(&data, &saved), Vec::<String>::new());
}

/// An old workbook's AutoFilter (`AUTOFILTERINFO`, `AUTOFILTER`, the `_FilterDatabase` name): its range and
/// each column's conditions (a file made of fictional data by LibreOffice: Excel 97-2003 format).
#[test]
fn an_old_xls_autofilter_is_read() {
    use ibis_doc_model::{CellRange, ColumnFilter};
    let (_data, opened) = open("xlsx-legacy-autofilter.xls");
    let p = &opened.presentation;
    let sheet = sheet_of_slide(p.slide(slide(p, "Data")).unwrap()).unwrap();
    let filter = sheet.auto_filter.as_ref().expect("the filter");
    assert_eq!(filter.range, CellRange { first_row: 0, last_row: 7, first_column: 0, last_column: 2 });
    let column = |c: u32| filter.columns.iter().find(|(i, _)| *i == c).map(|(_, f)| f.clone());
    // (LibreOffice writes only the first of two chosen values; Excel's own multi-select uses a later record, not read.)
    assert_eq!(column(0), Some(ColumnFilter::Values(vec!["item3".into()])));
    assert_eq!(column(1), Some(ColumnFilter::Custom { and: true, conditions: vec![("greaterThanOrEqual".into(), "30".into()), ("lessThan".into(), "60".into())] }));
    // The manual page breaks: after row 4 and column 2 (0-based: the pages start at row 4 and column 2).
    assert_eq!(sheet.print.row_breaks, vec![4]);
    assert_eq!(sheet.print.column_breaks, vec![2]);
    // Saved as an .xlsx: the filter is kept.
    let saved = opened.source.save(p, SaveOptions::default()).unwrap();
    let again = ibis_pptx::open(saved).unwrap();
    let sheet = sheet_of_slide(again.presentation.slide(slide(&again.presentation, "Data")).unwrap()).unwrap();
    assert_eq!(sheet.auto_filter.as_ref().map(|f| f.columns.len()), Some(2));
}

/// An old workbook's pictures (the drawing group's BLIP store and the sheet's drawing): two PNG images with their
/// anchors (a file made of fictional data by LibreOffice: Excel 97-2003 format).
#[test]
fn an_old_xls_pictures_are_read() {
    use ibis_doc_model::ShapeKind;
    let (_data, opened) = open("xlsx-legacy-pictures.xls");
    let p = &opened.presentation;
    let pictures: Vec<_> = p.slide(slide(p, "Pic")).unwrap().shapes.iter().filter(|s| matches!(s.kind, ShapeKind::Picture(_))).collect();
    assert_eq!(pictures.len(), 2);
    for pic in &pictures {
        let ShapeKind::Picture(picture) = &pic.kind else { unreachable!() };
        let media = p.media(picture.media).expect("its image");
        assert_eq!(media.format, ibis_doc_model::MediaFormat::Png);
        assert!(media.data.starts_with(b"\x89PNG"));
    }
    // The first picture sits at B3 (column 1, row 2); the second at F3.
    assert!(pictures[0].frame.x < pictures[1].frame.x && pictures[0].frame.width > 0 && pictures[0].frame.height > 0, "{:?} {:?}", pictures[0].frame, pictures[1].frame);
    // Saved as an .xlsx: both again.
    let saved = opened.source.save(p, SaveOptions::default()).unwrap();
    let again = ibis_pptx::open(saved).unwrap();
    let n = again.presentation.slide(slide(&again.presentation, "Pic")).unwrap().shapes.iter().filter(|s| matches!(s.kind, ShapeKind::Picture(_))).count();
    assert_eq!(n, 2);
}

/// An old workbook's charts (the chart streams of the sheet): six charts of one sheet (columns, stacked bars,
/// lines, an area, a pie, a scatter) with their titles, series (the cells they name, with the values as the
/// cache), colours and legend (a file made of fictional data by LibreOffice: Excel 97-2003 format).
#[test]
fn an_old_xls_charts_are_read() {
    use ibis_doc_model::{ChartType, Grouping, ShapeKind};
    let (_data, opened) = open("xlsx-legacy-charts.xls");
    let p = &opened.presentation;
    let charts: Vec<_> = p.slide(slide(p, "Data")).unwrap().shapes.iter().filter_map(|s| if let ShapeKind::Chart(c) = &s.kind { Some(c) } else { None }).collect();
    assert_eq!(charts.len(), 6, "all six charts");
    let kinds: Vec<_> = charts.iter().map(|c| c.groups[0].kind.clone()).collect();
    assert!(matches!(kinds[0], ChartType::Bar { .. }) && matches!(kinds[2], ChartType::Line) && matches!(kinds[4], ChartType::Pie { .. }), "{kinds:?}");
    assert_eq!(charts[0].title.as_ref().map(|t| t.text.as_str()), Some("Sales by quarter"));
    let columns = &charts[0].groups[0];
    assert_eq!(columns.series.len(), 2);
    assert_eq!(columns.series[0].name, "East");
    assert_eq!(columns.series[1].name, "West");
    assert_eq!(columns.series[0].categories, vec!["Q1", "Q2", "Q3", "Q4"]);
    let east: Vec<f64> = columns.series[0].values.iter().map(|v| v.unwrap().0).collect();
    assert_eq!(east, vec![30.0, 45.0, 60.0, 80.0]);
    assert_eq!(charts[1].groups[0].grouping, Grouping::Stacked);
    // Saved as an .xlsx: all six again.
    let saved = opened.source.save(p, SaveOptions::default()).unwrap();
    let again = ibis_pptx::open(saved).unwrap();
    let n = again.presentation.slide(slide(&again.presentation, "Data")).unwrap().shapes.iter().filter(|s| matches!(s.kind, ShapeKind::Chart(_))).count();
    assert_eq!(n, 6);
}

/// Excel 2010's half of a conditional format (`x14:cfRule` in the sheet's `extLst`): a data bar of 2007 names it by
/// `x14:id` (solid, bordered, from the right, no axis, lengths 0 to 100), and an icon set only 2010 has (3Stars) is
/// there alone. A fictional file.
#[test]
fn the_2010_half_of_a_conditional_format_is_read() {
    use ibis_doc_model::{CfKind, DataBarAxisPosition, DataBarDirection, Rgb};
    let (_data, opened) = open("xlsx-databar-2010.xlsx");
    let p = &opened.presentation;
    let sheet = sheet_of_slide(p.slide(slide(p, "Sheet1")).unwrap()).unwrap();
    let CfKind::DataBar { gradient, axis_position, min_length, max_length, border_color, direction, .. } = &sheet.conditional[0].rules[0].kind else { panic!("a data bar") };
    assert!(!*gradient);
    assert_eq!((*axis_position, *min_length, *max_length), (Some(DataBarAxisPosition::None), Some(0), Some(100)));
    assert_eq!((*border_color, *direction), (Some(Rgb(0x1F4E79)), DataBarDirection::RightToLeft));
    // The icon set that has no rule of 2007: on its own list, not in the one that is written back.
    assert_eq!(sheet.conditional.len(), 1);
    assert_eq!(sheet.conditional_x14.len(), 1);
    let CfKind::IconSet { set, values, .. } = &sheet.conditional_x14[0].rules[0].kind else { panic!("an icon set") };
    assert_eq!(set, "3Stars");
    assert_eq!(values.iter().map(|v| v.value.clone().unwrap()).collect::<Vec<_>>(), ["0", "33", "67"]);
    // Saved again: the file keeps the 2010 half.
    let saved = opened.source.save(p, SaveOptions::default()).unwrap();
    let again = ibis_pptx::open(saved).unwrap();
    let sheet = sheet_of_slide(again.presentation.slide(slide(&again.presentation, "Sheet1")).unwrap()).unwrap();
    assert_eq!(sheet.conditional_x14.len(), 1);
}

#[test]
fn an_old_xls_workbook_is_read() {
    // Excel's own .xls (test 担当, 架空の内容): 2 sheets, formats, a merge, a frozen pane, formulas.
    let (_data, opened) = open("xlsx-legacy-sample.xls");
    let p = &opened.presentation;
    assert_eq!(p.slides.iter().filter_map(|s| s.name.as_deref()).collect::<Vec<_>>(), ["売上", "集計"]);
    assert_eq!(value(p, "売上", 0, 0), CellValue::Text("架空商店 月次売上表".into()));
    assert_eq!(value(p, "売上", 2, 0), CellValue::Number(46113.0));
    assert_eq!(cell(p, "売上", 2, 0).text, "2026/4/1");
    assert_eq!(cell(p, "売上", 2, 3).text, "¥120");
    assert_eq!(formula(p, "売上", 2, 4).as_deref(), Some("C3*D3"));
    assert_eq!(value(p, "売上", 2, 4), CellValue::Number(1440.0));
    assert_eq!(formula(p, "売上", 7, 4).as_deref(), Some("SUM(E3:E6)"));
    assert_eq!(cell(p, "売上", 8, 4).text, "85.0%");
    assert_eq!(formula(p, "集計", 1, 1).as_deref(), Some("売上!E8"));
    assert_eq!(formula(p, "集計", 2, 1).as_deref(), Some("AVERAGE(売上!C3:C6)"));
    assert_eq!(cell(p, "集計", 3, 1).text, "¥1,234,567");
    let sheet = sheet_of_slide(p.slide(slide(p, "売上")).unwrap()).unwrap();
    assert_eq!(sheet.merges, vec![CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 4 }]);
    assert_eq!(sheet.frozen, Some((1, 2)));
    // Saved as an .xlsx: opens again with the same cells.
    let saved = opened.source.save(p, SaveOptions::default()).unwrap();
    let again = ibis_pptx::open(saved).unwrap();
    assert_eq!(formula(&again.presentation, "売上", 2, 4).as_deref(), Some("C3*D3"));
    assert_eq!(cell(&again.presentation, "売上", 2, 0).text, "2026/4/1");
}

/// A PNG of a few pixels, to put in a sheet.
fn a_small_png() -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        out.extend(kind);
        out.extend(data);
        let mut crc = 0xffff_ffffu32;
        for &b in kind.iter().chain(data) {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 { 0xedb8_8320 ^ (crc >> 1) } else { crc >> 1 };
            }
        }
        out.extend((!crc).to_be_bytes());
    };
    chunk(&mut out, b"IHDR", &[0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0, 0]);
    // One pixel, stored (no compression) in a zlib stream.
    let raw = [0u8, 0x44, 0x72, 0xc4];
    let (mut s1, mut s2) = (1u32, 0u32);
    for b in raw {
        s1 = (s1 + b as u32) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    let mut z = vec![0x78, 0x01, 0x01, raw.len() as u8, 0, !(raw.len() as u8), 0xff];
    z.extend(raw);
    z.extend(((s2 << 16) | s1).to_be_bytes());
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn shapes_of(p: &Presentation, sheet: &str) -> Vec<ibis_doc_model::Shape> {
    p.slide(slide(p, sheet)).unwrap().shapes.iter().filter(|s| !matches!(s.kind, ibis_doc_model::ShapeKind::Sheet(_))).cloned().collect()
}

#[test]
fn shapes_added_to_a_sheet_are_saved_in_its_drawing() {
    use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Geometry, Picture, Rgb, Shape, ShapeKind, Stroke};
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    let before = shapes_of(d.presentation(), "月次売上").len();
    let media = d.ids().add_media(ibis_doc_model::MediaFormat::Png, a_small_png());
    let mut added = Vec::new();
    for (kind, frame) in [
        (ShapeKind::TextBox(ibis_doc_model::TextBody::plain("メモ", Default::default())), EmuRect::new(6_000_000, 300_000, 1_500_000, 400_000)),
        (ShapeKind::Auto(AutoShape::new(Geometry::Ellipse, Fill::Solid(Color::opaque(Rgb(0xed7d31))), Some(Stroke::solid(Color::opaque(Rgb(0x2f528f)), 12_700)))), EmuRect::new(6_000_000, 900_000, 1_000_000, 700_000)),
        (ShapeKind::Line(Stroke::solid(Color::opaque(Rgb(0x000000)), 19_050)), EmuRect::new(6_000_000, 1_800_000, 1_200_000, 300_000)),
        (ShapeKind::Picture(Picture::new(media)), EmuRect::new(6_000_000, 2_300_000, 600_000, 600_000)),
    ] {
        let id = d.ids().new_shape_id();
        let index = d.presentation().slide(sheet).unwrap().shapes.len();
        d.execute(vec![Command::InsertShape { slide: sheet, index, shape: Shape::new(id, frame, kind) }]).unwrap();
        added.push(id);
    }
    let saved = save(&opened, d.presentation(), "shapes-added.xlsx");
    let parts = changed_parts(&data, &saved);
    assert!(parts.contains(&"xl/drawings/drawing1.xml".to_string()), "{parts:?}");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let drawing = String::from_utf8(package.read("xl/drawings/drawing1.xml").unwrap()).unwrap();
    for name in ["xdr:twoCellAnchor", "xdr:oneCellAnchor", "xdr:cxnSp", "xdr:pic", "メモ"] {
        assert!(drawing.contains(name), "{name} in {drawing}");
    }
    assert!(!drawing.contains("<xdr:nvPr/>"), "{drawing}");
    assert!(package.names().any(|n| n.starts_with("xl/media/image") && n.ends_with(".png")));
    let again = ibis_pptx::open(saved.clone()).unwrap();
    let shapes = shapes_of(&again.presentation, "月次売上");
    assert_eq!(shapes.len(), before + 4);
    let kinds: Vec<&str> = shapes[before..].iter().map(|s| match &s.kind {
        ShapeKind::TextBox(_) => "text",
        ShapeKind::Auto(_) => "auto",
        ShapeKind::Line(_) => "line",
        ShapeKind::Picture(_) => "picture",
        _ => "other",
    }).collect();
    // A text box without a fill or line reads back as a text box, a connector line as a line.
    assert!(kinds.contains(&"picture") && kinds.iter().filter(|k| **k != "other").count() >= 3, "{kinds:?}");
    // Their places survive within a cell.
    for (a, b) in shapes[before..].iter().zip(&added) {
        let _ = b;
        assert!(a.frame.width > 0 && a.frame.height > 0);
    }
    let first = shapes[before].frame;
    assert!((first.x - 6_000_000).abs() < 12_700 && (first.y - 300_000).abs() < 12_700, "{first:?}");
    // Saved again unchanged, nothing differs.
    let resaved = again.source.save(&again.presentation, SaveOptions::default()).unwrap();
    assert_eq!(changed_parts(&saved, &resaved), Vec::<String>::new());
}

#[test]
fn shapes_and_charts_removed_from_a_sheet_leave_the_drawing() {
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    let all = shapes_of(d.presentation(), "月次売上");
    assert!(!all.is_empty(), "the sales sheet has a chart");
    let chart_count = all.iter().filter(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_))).count();
    assert!(chart_count >= 1);
    for s in &all {
        d.execute(vec![Command::RemoveShape { slide: sheet, shape: s.id }]).unwrap();
    }
    let saved = save(&opened, d.presentation(), "shapes-removed.xlsx");
    assert!(changed_parts(&data, &saved).contains(&"xl/drawings/drawing1.xml".to_string()));
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    assert!(!package.names().any(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")), "the chart part goes");
    let again = ibis_pptx::open(saved).unwrap();
    assert!(shapes_of(&again.presentation, "月次売上").is_empty());
}

#[test]
fn a_shape_added_to_a_sheet_without_a_drawing_makes_the_drawing() {
    let (data, opened) = open("xlsx-table.xlsx");
    assert!(!ibis_ooxml::Package::open(data.clone()).unwrap().names().any(|n| n.starts_with("xl/drawings/drawing")), "the file has no drawing");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = d.presentation().slides[0].id;
    let id = d.ids().new_shape_id();
    let shape = ibis_doc_model::Shape::text_box(id, ibis_doc_model::EmuRect::new(3_000_000, 2_000_000, 1_500_000, 400_000), "注");
    d.execute(vec![Command::InsertShape { slide: sheet, index: 1, shape }]).unwrap();
    let saved = save(&opened, d.presentation(), "shape-in-new-drawing.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    assert!(package.contains("xl/drawings/drawing1.xml"));
    let sheet_xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet_xml.contains("<drawing "), "{sheet_xml}");
    let again = ibis_pptx::open(saved).unwrap();
    let shapes: Vec<_> = again.presentation.slides[0].shapes.iter().filter(|s| !matches!(s.kind, ibis_doc_model::ShapeKind::Sheet(_))).collect();
    assert_eq!(shapes.len(), 1);
}

#[test]
fn a_chart_inserted_over_cells_is_saved_with_references_to_them() {
    use ibis_doc_model::{CellRange, EmuRect, ShapeKind};
    use ibis_pptx::NewChartKind;
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    // A3:D8: the stores down the side, the months across the top.
    let range = CellRange { first_row: 2, first_column: 0, last_row: 7, last_column: 3 };
    let refs = ibis_pptx::refs_for_range(d.presentation(), "月次売上", range, true).expect("headings in the first row and column");
    assert_eq!(refs.len(), 3);
    let charts_before = shapes_of(d.presentation(), "月次売上").iter().filter(|s| matches!(s.kind, ShapeKind::Chart(_))).count();
    let mut made = Vec::new();
    for kind in [NewChartKind::ClusteredColumn, NewChartKind::Line, NewChartKind::Pie] {
        let id = d.ids().new_shape_id();
        let shape = opened.source.new_sheet_chart(d.presentation(), sheet, id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), kind, refs.clone()).unwrap();
        let ShapeKind::Chart(c) = &shape.kind else { panic!("a chart") };
        let all: Vec<_> = c.groups.iter().flat_map(|g| g.series.iter()).collect();
        assert!(!all.is_empty());
        assert_eq!(all[0].name, "4月");
        assert_eq!(all[0].categories, ["北町店", "南町店", "東町店", "西町店", "駅前店"]);
        assert_eq!(all[0].values[0].map(|n| n.0), Some(1_250_000.0));
        let index = d.presentation().slide(sheet).unwrap().shapes.len();
        d.execute(vec![Command::InsertShape { slide: sheet, index, shape: shape.clone() }]).unwrap();
        made.push(id);
    }
    let saved = save(&opened, d.presentation(), "charts-added.xlsx");
    assert!(changed_parts(&data, &saved).contains(&"xl/drawings/drawing1.xml".to_string()));
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let new_parts: Vec<String> = package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(str::to_string).collect();
    assert_eq!(new_parts.len(), charts_before + 3, "{new_parts:?}");
    let all_charts: String = new_parts.iter().map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).collect();
    assert!(all_charts.contains("月次売上!$B$4:$B$8") && all_charts.contains("月次売上!$A$4:$A$8"), "{all_charts}");
    assert!(!all_charts.contains("externalData"), "a sheet's chart has no workbook of its own");
    let again = ibis_pptx::open(saved.clone()).unwrap();
    let charts: Vec<_> = shapes_of(&again.presentation, "月次売上").into_iter().filter(|s| matches!(s.kind, ShapeKind::Chart(_))).collect();
    assert_eq!(charts.len(), charts_before + 3);
    let ShapeKind::Chart(last) = &charts.last().unwrap().kind else { unreachable!() };
    assert_eq!(last.groups.iter().map(|g| g.series.len()).sum::<usize>(), 3);
    let first = last.groups[0].series[0].values.iter().map(|n| n.map(|n| n.0)).collect::<Vec<_>>();
    assert_eq!(first[0], Some(1_250_000.0), "{first:?}");
    let resaved = again.source.save(&again.presentation, SaveOptions::default()).unwrap();
    assert_eq!(changed_parts(&saved, &resaved), Vec::<String>::new());
}

#[test]
fn a_new_chart_edited_after_insertion_saves_the_edit() {
    use ibis_doc_model::chart::{AxisKind, ChartText, ChartTitle};
    use ibis_doc_model::{CellRange, EmuRect, ShapeKind};
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    let range = CellRange { first_row: 2, first_column: 0, last_row: 7, last_column: 3 };
    let refs = ibis_pptx::refs_for_range(d.presentation(), "月次売上", range, true).unwrap();
    let id = d.ids().new_shape_id();
    let shape = opened.source.new_sheet_chart(d.presentation(), sheet, id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), ibis_pptx::NewChartKind::ClusteredColumn, refs).unwrap();
    let index = d.presentation().slide(sheet).unwrap().shapes.len();
    d.execute(vec![Command::InsertShape { slide: sheet, index, shape: shape.clone() }]).unwrap();
    let ShapeKind::Chart(chart) = &shape.kind else { panic!() };
    let mut edited = (**chart).clone();
    let val = edited.axes.iter().position(|a| a.kind == AxisKind::Value && !a.deleted).unwrap();
    edited.axes[val].title = Some(ChartTitle { text: "売上（円）".into(), style: ChartText { size: 1000, ..edited.axes[val].text.clone() }, at: None });
    d.execute(vec![Command::SetKind { slide: sheet, shape: id, kind: Box::new(ShapeKind::Chart(Box::new(edited))) }]).unwrap();
    let saved = save(&opened, d.presentation(), "chart-new-edited.xlsx");
    let again = ibis_pptx::open(saved).unwrap();
    let last = shapes_of(&again.presentation, "月次売上").into_iter().rev().find_map(|s| match s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap();
    let val = last.axes.iter().position(|a| a.kind == AxisKind::Value && !a.deleted).unwrap();
    assert_eq!(last.axes[val].title.as_ref().map(|t| t.text.as_str()), Some("売上（円）"));
}

#[test]
fn a_scatter_chart_over_numbers_is_saved_with_x_and_y_references() {
    use ibis_doc_model::{CellRange, EmuRect, ShapeKind};
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    // B3:D8: the 4月 column as X, 5月 and 6月 as Y, headings in the first row.
    let range = CellRange { first_row: 2, first_column: 1, last_row: 7, last_column: 3 };
    let refs = ibis_pptx::scatter_refs_for_range(d.presentation(), "月次売上", range, true).expect("numbers under headings");
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].categories.as_deref(), Some("月次売上!$B$4:$B$8"));
    assert_eq!(refs[1].values.as_deref(), Some("月次売上!$D$4:$D$8"));
    let id = d.ids().new_shape_id();
    let shape = opened.source.new_sheet_chart(d.presentation(), sheet, id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), ibis_pptx::NewChartKind::Scatter, refs).unwrap();
    let ShapeKind::Chart(c) = &shape.kind else { panic!() };
    let series: Vec<_> = c.groups.iter().flat_map(|g| g.series.iter()).collect();
    assert_eq!(series.len(), 2);
    assert_eq!(series[0].name, "5月");
    let index = d.presentation().slide(sheet).unwrap().shapes.len();
    d.execute(vec![Command::InsertShape { slide: sheet, index, shape }]).unwrap();
    let saved = save(&opened, d.presentation(), "chart-scatter.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let all: String = package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).collect();
    assert!(all.contains("<c:xVal>") && all.contains("月次売上!$B$4:$B$8") && all.contains("月次売上!$D$4:$D$8"), "{all}");
    let again = ibis_pptx::open(saved).unwrap();
    let last = shapes_of(&again.presentation, "月次売上").into_iter().rev().find_map(|s| match s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap();
    let s0 = last.groups.iter().flat_map(|g| g.series.iter()).next().unwrap();
    assert_eq!(s0.x_values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(1_250_000.0), Some(980_000.0), Some(1_530_000.0), Some(760_000.0), Some(2_100_000.0)]);
}

#[test]
fn an_old_xls_keeps_conditional_formats_validations_links_and_notes() {
    // Excel's own .xls (test 担当, 架空の内容): 条件付き書式 (値・数式・データ バー), 入力規則 (リスト・整数), ハイパーリンク (Web・シート内), メモ 2 つ.
    let (_data, opened) = open("xlsx-legacy-features.xls");
    let p = &opened.presentation;
    let sheet = sheet_of_slide(p.slide(slide(p, "売上")).unwrap()).unwrap();
    assert_eq!(sheet.conditional.len(), 3);
    let rules: Vec<_> = sheet.conditional.iter().flat_map(|c| c.rules.iter()).collect();
    assert_eq!(rules.len(), 3);
    assert!(matches!(&rules[0].kind, ibis_doc_model::CfKind::CellIs { operator, formulas } if operator == "greaterThan" && formulas == &["60".to_string()]), "{:?}", rules[0].kind);
    assert!(matches!(&rules[1].kind, ibis_doc_model::CfKind::Expression(f) if f == "MOD(ROW(),2)=0"), "{:?}", rules[1].kind);
    assert!(matches!(&rules[2].kind, ibis_doc_model::CfKind::DataBar { .. }), "{:?}", rules[2].kind);
    // The format of the first rule: the real colours (FFC7CE, 9C0006), bold.
    let style = rules[0].style.as_ref().expect("a format");
    assert_eq!(style.fill, Some(ibis_doc_model::Rgb(0xFFC7CE)));
    assert_eq!(style.color, Some(ibis_doc_model::Rgb(0x9C0006)));
    assert_eq!(style.bold, Some(true));
    assert_eq!(sheet.validations.len(), 2);
    assert_eq!(sheet.validations[0].kind, "list");
    assert_eq!(sheet.validations[0].formula1.as_deref(), Some("\"はい,いいえ,保留\""));
    assert_eq!(sheet.validations[1].kind, "whole");
    assert_eq!(sheet.hyperlinks.len(), 2);
    assert_eq!(sheet.hyperlinks[0].target.as_deref(), Some("https://example.com/"));
    assert_eq!(sheet.hyperlinks[1].location.as_deref(), Some("集計!A1"));
    // The rows keep the height the file gives (15 pt = 190500 EMU), not one worked out from the text.
    assert_eq!(sheet.rows.first().copied(), Some(190_500), "{:?}", sheet.rows.first());
    assert_eq!(sheet.comments.len(), 2);
    assert_eq!(sheet.comments[0].text, "メモの内容です");
    assert_eq!((sheet.comments[0].row, sheet.comments[0].column), (1, 1));
    // Saved and opened again: all there.
    let saved = save(&opened, p, "legacy-features.xlsx");
    let again = ibis_pptx::open(saved).unwrap();
    let sheet = sheet_of_slide(again.presentation.slide(slide(&again.presentation, "売上")).unwrap()).unwrap();
    assert_eq!((sheet.conditional.len(), sheet.validations.len(), sheet.hyperlinks.len(), sheet.comments.len()), (3, 2, 2, 2));
}

#[test]
fn a_bubble_chart_over_three_columns_is_saved_with_x_y_and_size_references() {
    use ibis_doc_model::{CellRange, EmuRect, ShapeKind};
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    // B3:D8: 4月 as X, 5月 as Y, 6月 as the size, headings in the first row.
    let range = CellRange { first_row: 2, first_column: 1, last_row: 7, last_column: 3 };
    let refs = ibis_pptx::bubble_refs_for_range(d.presentation(), "月次売上", range, true).expect("three columns of numbers");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].sizes.as_deref(), Some("月次売上!$D$4:$D$8"));
    let id = d.ids().new_shape_id();
    let shape = opened.source.new_sheet_chart(d.presentation(), sheet, id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), ibis_pptx::NewChartKind::Bubble, refs).unwrap();
    let ShapeKind::Chart(c) = &shape.kind else { panic!() };
    let series = &c.groups[0].series[0];
    assert_eq!(series.name, "5月");
    assert_eq!(series.bubble_sizes.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>()[0], Some(1_180_000.0));
    let index = d.presentation().slide(sheet).unwrap().shapes.len();
    d.execute(vec![Command::InsertShape { slide: sheet, index, shape }]).unwrap();
    let saved = save(&opened, d.presentation(), "chart-bubble.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let all: String = package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).collect();
    assert!(all.contains("<c:bubbleSize>") && all.contains("月次売上!$D$4:$D$8") && all.contains("月次売上!$B$4:$B$8"), "{all}");
    let again = ibis_pptx::open(saved).unwrap();
    let last = shapes_of(&again.presentation, "月次売上").into_iter().rev().find_map(|s| match s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap();
    let s0 = &last.groups[0].series[0];
    assert_eq!(s0.bubble_sizes.len(), 5);
    assert_eq!(s0.x_values.len(), 5);
}

#[test]
fn a_scatter_chart_can_be_switched_between_columns_and_rows() {
    use ibis_doc_model::{CellInput, CellRange, CellValue, EmuRect, ShapeKind};
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    // Row 10 (r=9): A10: header, B10..D10: X values (10, 20, 30)
    // Row 11 (r=10): A11: "系列A", B11..D11: Y values (100, 200, 300)
    // Row 12 (r=11): A12: "系列B", B12..D12: Y values (150, 250, 350)
    set(
        &mut d,
        sheet,
        vec![
            (9, 0, CellInput::Value(CellValue::Text("X/Y".into()))),
            (9, 1, CellInput::Value(CellValue::Number(10.0))),
            (9, 2, CellInput::Value(CellValue::Number(20.0))),
            (9, 3, CellInput::Value(CellValue::Number(30.0))),
            (10, 0, CellInput::Value(CellValue::Text("系列A".into()))),
            (10, 1, CellInput::Value(CellValue::Number(100.0))),
            (10, 2, CellInput::Value(CellValue::Number(200.0))),
            (10, 3, CellInput::Value(CellValue::Number(300.0))),
            (11, 0, CellInput::Value(CellValue::Text("系列B".into()))),
            (11, 1, CellInput::Value(CellValue::Number(150.0))),
            (11, 2, CellInput::Value(CellValue::Number(250.0))),
            (11, 3, CellInput::Value(CellValue::Number(350.0))),
        ],
    );
    let range = CellRange { first_row: 9, first_column: 0, last_row: 11, last_column: 3 };
    // Rows mode: by_columns = false
    let row_refs = ibis_pptx::scatter_refs_for_range(d.presentation(), "月次売上", range, false).expect("scatter refs by rows");
    assert_eq!(row_refs.len(), 2);
    assert_eq!(row_refs[0].name.as_deref(), Some("月次売上!$A$11"));
    assert_eq!(row_refs[0].categories.as_deref(), Some("月次売上!$B$10:$D$10"));
    assert_eq!(row_refs[0].values.as_deref(), Some("月次売上!$B$11:$D$11"));
    assert_eq!(row_refs[1].name.as_deref(), Some("月次売上!$A$12"));
    assert_eq!(row_refs[1].values.as_deref(), Some("月次売上!$B$12:$D$12"));

    // refs_range should detect by_columns = false
    let (r_sheet, r_range, by_cols) = ibis_pptx::refs_range(&row_refs).unwrap();
    assert_eq!(r_sheet, "月次売上");
    assert_eq!(r_range, range);
    assert!(!by_cols);

    // Insert shape with row_refs
    let id = d.ids().new_shape_id();
    let shape = opened.source.new_sheet_chart(d.presentation(), sheet, id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), ibis_pptx::NewChartKind::Scatter, row_refs).unwrap();
    let ShapeKind::Chart(c) = &shape.kind else { panic!() };
    let series: Vec<_> = c.groups.iter().flat_map(|g| g.series.iter()).collect();
    assert_eq!(series.len(), 2);
    assert_eq!(series[0].name, "系列A");
    assert_eq!(series[0].x_values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(10.0), Some(20.0), Some(30.0)]);
    assert_eq!(series[0].values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(100.0), Some(200.0), Some(300.0)]);

    let index = d.presentation().slide(sheet).unwrap().shapes.len();
    d.execute(vec![Command::InsertShape { slide: sheet, index, shape }]).unwrap();
    let saved = save(&opened, d.presentation(), "chart-scatter-row.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let all: String = package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).collect();
    assert!(all.contains("<c:xVal>") && all.contains("月次売上!$B$10:$D$10") && all.contains("月次売上!$B$11:$D$11"), "{all}");

    let again = ibis_pptx::open(saved).unwrap();
    let last = shapes_of(&again.presentation, "月次売上").into_iter().rev().find_map(|s| match s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap();
    let s0 = last.groups.iter().flat_map(|g| g.series.iter()).next().unwrap();
    assert_eq!(s0.x_values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(10.0), Some(20.0), Some(30.0)]);
    assert_eq!(s0.values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(100.0), Some(200.0), Some(300.0)]);
}

#[test]
fn a_bubble_chart_can_be_switched_between_columns_and_rows() {
    use ibis_doc_model::{CellInput, CellRange, CellValue, EmuRect, ShapeKind};
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    // Row 13 (r=12): A13: header, B13..D13: X values (1, 2, 3)
    // Row 14 (r=13): A14: "系列1", B14..D14: Y values (10, 20, 30)
    // Row 15 (r=14): A15: "サイズ1", B15..D15: Size values (5, 8, 12)
    set(
        &mut d,
        sheet,
        vec![
            (12, 0, CellInput::Value(CellValue::Text("X/Y/Size".into()))),
            (12, 1, CellInput::Value(CellValue::Number(1.0))),
            (12, 2, CellInput::Value(CellValue::Number(2.0))),
            (12, 3, CellInput::Value(CellValue::Number(3.0))),
            (13, 0, CellInput::Value(CellValue::Text("バブルA".into()))),
            (13, 1, CellInput::Value(CellValue::Number(10.0))),
            (13, 2, CellInput::Value(CellValue::Number(20.0))),
            (13, 3, CellInput::Value(CellValue::Number(30.0))),
            (14, 0, CellInput::Value(CellValue::Text("サイズA".into()))),
            (14, 1, CellInput::Value(CellValue::Number(5.0))),
            (14, 2, CellInput::Value(CellValue::Number(8.0))),
            (14, 3, CellInput::Value(CellValue::Number(12.0))),
        ],
    );
    let range = CellRange { first_row: 12, first_column: 0, last_row: 14, last_column: 3 };
    // Rows mode: by_columns = false
    let row_refs = ibis_pptx::bubble_refs_for_range(d.presentation(), "月次売上", range, false).expect("bubble refs by rows");
    assert_eq!(row_refs.len(), 1);
    assert_eq!(row_refs[0].name.as_deref(), Some("月次売上!$A$14"));
    assert_eq!(row_refs[0].categories.as_deref(), Some("月次売上!$B$13:$D$13"));
    assert_eq!(row_refs[0].values.as_deref(), Some("月次売上!$B$14:$D$14"));
    assert_eq!(row_refs[0].sizes.as_deref(), Some("月次売上!$B$15:$D$15"));

    // refs_range should detect by_columns = false and include size row in range
    let (r_sheet, r_range, by_cols) = ibis_pptx::refs_range(&row_refs).unwrap();
    assert_eq!(r_sheet, "月次売上");
    assert_eq!(r_range, range);
    assert!(!by_cols);

    // Insert shape with row_refs
    let id = d.ids().new_shape_id();
    let shape = opened.source.new_sheet_chart(d.presentation(), sheet, id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), ibis_pptx::NewChartKind::Bubble, row_refs).unwrap();
    let ShapeKind::Chart(c) = &shape.kind else { panic!() };
    let series = &c.groups[0].series[0];
    assert_eq!(series.name, "バブルA");
    assert_eq!(series.x_values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(1.0), Some(2.0), Some(3.0)]);
    assert_eq!(series.values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(10.0), Some(20.0), Some(30.0)]);
    assert_eq!(series.bubble_sizes.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(5.0), Some(8.0), Some(12.0)]);

    let index = d.presentation().slide(sheet).unwrap().shapes.len();
    d.execute(vec![Command::InsertShape { slide: sheet, index, shape }]).unwrap();
    let saved = save(&opened, d.presentation(), "chart-bubble-row.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let all: String = package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).collect();
    assert!(all.contains("<c:bubbleSize>") && all.contains("月次売上!$B$13:$D$13") && all.contains("月次売上!$B$14:$D$14") && all.contains("月次売上!$B$15:$D$15"), "{all}");

    let again = ibis_pptx::open(saved).unwrap();
    let last = shapes_of(&again.presentation, "月次売上").into_iter().rev().find_map(|s| match s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap();
    let s0 = &last.groups[0].series[0];
    assert_eq!(s0.bubble_sizes.len(), 3);
    assert_eq!(s0.x_values.len(), 3);
    assert_eq!(s0.values.len(), 3);
}

#[test]
fn scatter_and_bubble_charts_support_headerless_ranges() {
    use ibis_doc_model::{CellInput, CellRange, CellValue, EmuRect, ShapeKind};
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");

    // Pure numeric scatter data (no headers): row 16..18, col 0..1 (A17:B19)
    // All 3 rows are numbers, so row 16 is NOT a header row.
    set(
        &mut d,
        sheet,
        vec![
            (16, 0, CellInput::Value(CellValue::Number(100.0))),
            (16, 1, CellInput::Value(CellValue::Number(10.0))),
            (17, 0, CellInput::Value(CellValue::Number(200.0))),
            (17, 1, CellInput::Value(CellValue::Number(20.0))),
            (18, 0, CellInput::Value(CellValue::Number(300.0))),
            (18, 1, CellInput::Value(CellValue::Number(30.0))),
        ],
    );
    let scatter_range = CellRange { first_row: 16, first_column: 0, last_row: 18, last_column: 1 };
    let scatter_refs = ibis_pptx::scatter_refs_for_range(d.presentation(), "月次売上", scatter_range, true).expect("scatter refs for headerless numbers");
    assert_eq!(scatter_refs.len(), 1);
    assert_eq!(scatter_refs[0].name, None, "headerless scatter series should have name: None");
    assert_eq!(scatter_refs[0].categories.as_deref(), Some("月次売上!$A$17:$A$19"), "all rows included in X");
    assert_eq!(scatter_refs[0].values.as_deref(), Some("月次売上!$B$17:$B$19"), "all rows included in Y");

    // Check refs_range on headerless refs
    let (r_sheet, r_range, by_cols) = ibis_pptx::refs_range(&scatter_refs).unwrap();
    assert_eq!(r_sheet, "月次売上");
    assert_eq!(r_range, scatter_range);
    assert!(by_cols);

    // Insert shape with headerless scatter_refs
    let id = d.ids().new_shape_id();
    let shape = opened.source.new_sheet_chart(d.presentation(), sheet, id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), ibis_pptx::NewChartKind::Scatter, scatter_refs).unwrap();
    let ShapeKind::Chart(c) = &shape.kind else { panic!() };
    let s0 = &c.groups[0].series[0];
    assert!(!s0.name.is_empty(), "default series name assigned");
    assert_eq!(s0.x_values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(100.0), Some(200.0), Some(300.0)]);
    assert_eq!(s0.values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(10.0), Some(20.0), Some(30.0)]);

    let index = d.presentation().slide(sheet).unwrap().shapes.len();
    d.execute(vec![Command::InsertShape { slide: sheet, index, shape }]).unwrap();
    let saved = save(&opened, d.presentation(), "chart-scatter-headerless.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let all: String = package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).collect();
    assert!(all.contains("<c:xVal>") && all.contains("月次売上!$A$17:$A$19") && all.contains("月次売上!$B$17:$B$19"), "{all}");
    // Headerless series must not have a <c:tx> with strRef pointing to cells
    assert!(!all.contains("月次売上!$B$17:$B$17"), "must not consume row 17 as header name: {all}");

    // Pure numeric bubble data (no headers): row 20..22, col 0..2 (A21:C23)
    set(
        &mut d,
        sheet,
        vec![
            (20, 0, CellInput::Value(CellValue::Number(1.0))),
            (20, 1, CellInput::Value(CellValue::Number(10.0))),
            (20, 2, CellInput::Value(CellValue::Number(5.0))),
            (21, 0, CellInput::Value(CellValue::Number(2.0))),
            (21, 1, CellInput::Value(CellValue::Number(20.0))),
            (21, 2, CellInput::Value(CellValue::Number(8.0))),
            (22, 0, CellInput::Value(CellValue::Number(3.0))),
            (22, 1, CellInput::Value(CellValue::Number(30.0))),
            (22, 2, CellInput::Value(CellValue::Number(12.0))),
        ],
    );
    let bubble_range = CellRange { first_row: 20, first_column: 0, last_row: 22, last_column: 2 };
    let bubble_refs = ibis_pptx::bubble_refs_for_range(d.presentation(), "月次売上", bubble_range, true).expect("bubble refs for headerless numbers");
    assert_eq!(bubble_refs.len(), 1);
    assert_eq!(bubble_refs[0].name, None, "headerless bubble series should have name: None");
    assert_eq!(bubble_refs[0].categories.as_deref(), Some("月次売上!$A$21:$A$23"));
    assert_eq!(bubble_refs[0].values.as_deref(), Some("月次売上!$B$21:$B$23"));
    assert_eq!(bubble_refs[0].sizes.as_deref(), Some("月次売上!$C$21:$C$23"));

    // Check refs_range on headerless bubble_refs
    let (b_sheet, b_range, b_by_cols) = ibis_pptx::refs_range(&bubble_refs).unwrap();
    assert_eq!(b_sheet, "月次売上");
    assert_eq!(b_range, bubble_range);
    assert!(b_by_cols);

    // Insert shape with headerless bubble_refs
    let b_id = d.ids().new_shape_id();
    let b_shape = opened.source.new_sheet_chart(d.presentation(), sheet, b_id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), ibis_pptx::NewChartKind::Bubble, bubble_refs).unwrap();
    let ShapeKind::Chart(bc) = &b_shape.kind else { panic!() };
    let bs0 = &bc.groups[0].series[0];
    assert!(!bs0.name.is_empty());
    assert_eq!(bs0.x_values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(1.0), Some(2.0), Some(3.0)]);
    assert_eq!(bs0.values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(10.0), Some(20.0), Some(30.0)]);
    assert_eq!(bs0.bubble_sizes.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(5.0), Some(8.0), Some(12.0)]);

    let b_index = d.presentation().slide(sheet).unwrap().shapes.len();
    d.execute(vec![Command::InsertShape { slide: sheet, index: b_index, shape: b_shape }]).unwrap();
    let b_saved = save(&opened, d.presentation(), "chart-bubble-headerless.xlsx");
    let b_package = ibis_ooxml::Package::open(b_saved).unwrap();
    let b_all: String = b_package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(|n| String::from_utf8(b_package.read(n).unwrap()).unwrap()).collect();
    assert!(b_all.contains("<c:bubbleSize>") && b_all.contains("月次売上!$C$21:$C$23"), "{b_all}");
}

#[test]
fn stock_charts_support_range_specification_and_roundtrip() {
    use ibis_doc_model::{CellInput, CellRange, CellValue, EmuRect, ShapeKind};
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");

    // 1. StockHLC with headers and date categories (4 columns: Date, High, Low, Close)
    set(
        &mut d,
        sheet,
        vec![
            (30, 0, CellInput::Value(CellValue::Text("日付".into()))),
            (30, 1, CellInput::Value(CellValue::Text("高値".into()))),
            (30, 2, CellInput::Value(CellValue::Text("安値".into()))),
            (30, 3, CellInput::Value(CellValue::Text("終値".into()))),
            (31, 0, CellInput::Value(CellValue::Text("2026/10/01".into()))),
            (31, 1, CellInput::Value(CellValue::Number(150.0))),
            (31, 2, CellInput::Value(CellValue::Number(100.0))),
            (31, 3, CellInput::Value(CellValue::Number(130.0))),
            (32, 0, CellInput::Value(CellValue::Text("2026/10/02".into()))),
            (32, 1, CellInput::Value(CellValue::Number(160.0))),
            (32, 2, CellInput::Value(CellValue::Number(110.0))),
            (32, 3, CellInput::Value(CellValue::Number(140.0))),
            (33, 0, CellInput::Value(CellValue::Text("2026/10/03".into()))),
            (33, 1, CellInput::Value(CellValue::Number(170.0))),
            (33, 2, CellInput::Value(CellValue::Number(120.0))),
            (33, 3, CellInput::Value(CellValue::Number(155.0))),
        ],
    );
    let hlc_range = CellRange { first_row: 30, first_column: 0, last_row: 33, last_column: 3 };
    let hlc_refs = ibis_pptx::stock_refs_for_range(d.presentation(), "月次売上", hlc_range, ibis_pptx::NewChartKind::StockHLC, true).expect("hlc refs");
    assert_eq!(hlc_refs.len(), 3);
    assert_eq!(hlc_refs[0].name.as_deref(), Some("月次売上!$B$31"));
    assert_eq!(hlc_refs[0].categories.as_deref(), Some("月次売上!$A$32:$A$34"));
    assert_eq!(hlc_refs[0].values.as_deref(), Some("月次売上!$B$32:$B$34"));
    assert_eq!(hlc_refs[1].name.as_deref(), Some("月次売上!$C$31"));
    assert_eq!(hlc_refs[2].name.as_deref(), Some("月次売上!$D$31"));

    let (s, r, by_cols) = ibis_pptx::refs_range(&hlc_refs).unwrap();
    assert_eq!(s, "月次売上");
    assert_eq!(r, hlc_range);
    assert!(by_cols);

    let hlc_id = d.ids().new_shape_id();
    let hlc_shape = opened.source.new_sheet_chart(d.presentation(), sheet, hlc_id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), ibis_pptx::NewChartKind::StockHLC, hlc_refs).unwrap();
    let ShapeKind::Chart(hlc_chart) = &hlc_shape.kind else { panic!() };
    assert_eq!(hlc_chart.groups[0].series.len(), 3);
    assert_eq!(hlc_chart.groups[0].series[0].name, "高値");
    assert_eq!(hlc_chart.groups[0].series[0].values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>(), [Some(150.0), Some(160.0), Some(170.0)]);

    // 2. Pure numeric / headerless HLC with 3 columns (no categories, just High, Low, Close)
    set(
        &mut d,
        sheet,
        vec![
            (35, 0, CellInput::Value(CellValue::Number(200.0))),
            (35, 1, CellInput::Value(CellValue::Number(150.0))),
            (35, 2, CellInput::Value(CellValue::Number(180.0))),
            (36, 0, CellInput::Value(CellValue::Number(210.0))),
            (36, 1, CellInput::Value(CellValue::Number(160.0))),
            (36, 2, CellInput::Value(CellValue::Number(190.0))),
            (37, 0, CellInput::Value(CellValue::Number(220.0))),
            (37, 1, CellInput::Value(CellValue::Number(170.0))),
            (37, 2, CellInput::Value(CellValue::Number(200.0))),
        ],
    );
    let hlc3_range = CellRange { first_row: 35, first_column: 0, last_row: 37, last_column: 2 };
    let hlc3_refs = ibis_pptx::stock_refs_for_range(d.presentation(), "月次売上", hlc3_range, ibis_pptx::NewChartKind::StockHLC, true).expect("hlc 3 cols pure numeric");
    assert_eq!(hlc3_refs.len(), 3);
    assert_eq!(hlc3_refs[0].name, None);
    assert_eq!(hlc3_refs[0].categories, None);
    assert_eq!(hlc3_refs[0].values.as_deref(), Some("月次売上!$A$36:$A$38"));

    // 3. StockOHLC with headers (5 columns: Date, Open, High, Low, Close)
    set(
        &mut d,
        sheet,
        vec![
            (40, 0, CellInput::Value(CellValue::Text("日付".into()))),
            (40, 1, CellInput::Value(CellValue::Text("始値".into()))),
            (40, 2, CellInput::Value(CellValue::Text("高値".into()))),
            (40, 3, CellInput::Value(CellValue::Text("安値".into()))),
            (40, 4, CellInput::Value(CellValue::Text("終値".into()))),
            (41, 0, CellInput::Value(CellValue::Text("10/1".into()))),
            (41, 1, CellInput::Value(CellValue::Number(100.0))),
            (41, 2, CellInput::Value(CellValue::Number(120.0))),
            (41, 3, CellInput::Value(CellValue::Number(90.0))),
            (41, 4, CellInput::Value(CellValue::Number(110.0))),
            (42, 0, CellInput::Value(CellValue::Text("10/2".into()))),
            (42, 1, CellInput::Value(CellValue::Number(110.0))),
            (42, 2, CellInput::Value(CellValue::Number(130.0))),
            (42, 3, CellInput::Value(CellValue::Number(105.0))),
            (42, 4, CellInput::Value(CellValue::Number(125.0))),
        ],
    );
    let ohlc_range = CellRange { first_row: 40, first_column: 0, last_row: 42, last_column: 4 };
    let ohlc_refs = ibis_pptx::stock_refs_for_range(d.presentation(), "月次売上", ohlc_range, ibis_pptx::NewChartKind::StockOHLC, true).expect("ohlc refs");
    assert_eq!(ohlc_refs.len(), 4);
    assert_eq!(ohlc_refs[0].name.as_deref(), Some("月次売上!$B$41"));
    assert_eq!(ohlc_refs[0].categories.as_deref(), Some("月次売上!$A$42:$A$43"));
    assert_eq!(ohlc_refs[0].values.as_deref(), Some("月次売上!$B$42:$B$43"));

    // 4. StockVHLC (2 groups: Bar for volume, Stock for HLC)
    set(
        &mut d,
        sheet,
        vec![
            (45, 0, CellInput::Value(CellValue::Text("日付".into()))),
            (45, 1, CellInput::Value(CellValue::Text("出来高".into()))),
            (45, 2, CellInput::Value(CellValue::Text("高値".into()))),
            (45, 3, CellInput::Value(CellValue::Text("安値".into()))),
            (45, 4, CellInput::Value(CellValue::Text("終値".into()))),
            (46, 0, CellInput::Value(CellValue::Text("10/1".into()))),
            (46, 1, CellInput::Value(CellValue::Number(10000.0))),
            (46, 2, CellInput::Value(CellValue::Number(250.0))),
            (46, 3, CellInput::Value(CellValue::Number(210.0))),
            (46, 4, CellInput::Value(CellValue::Number(240.0))),
            (47, 0, CellInput::Value(CellValue::Text("10/2".into()))),
            (47, 1, CellInput::Value(CellValue::Number(15000.0))),
            (47, 2, CellInput::Value(CellValue::Number(260.0))),
            (47, 3, CellInput::Value(CellValue::Number(220.0))),
            (47, 4, CellInput::Value(CellValue::Number(255.0))),
        ],
    );
    let vhlc_range = CellRange { first_row: 45, first_column: 0, last_row: 47, last_column: 4 };
    let vhlc_refs = ibis_pptx::stock_refs_for_range(d.presentation(), "月次売上", vhlc_range, ibis_pptx::NewChartKind::StockVHLC, true).expect("vhlc refs");
    assert_eq!(vhlc_refs.len(), 4);
    assert_eq!(vhlc_refs[0].name.as_deref(), Some("月次売上!$B$46"));
    assert_eq!(vhlc_refs[1].name.as_deref(), Some("月次売上!$C$46"));
    assert_eq!(vhlc_refs[2].name.as_deref(), Some("月次売上!$D$46"));
    assert_eq!(vhlc_refs[3].name.as_deref(), Some("月次売上!$E$46"));

    let vhlc_id = d.ids().new_shape_id();
    let vhlc_shape = opened.source.new_sheet_chart(d.presentation(), sheet, vhlc_id, EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000), ibis_pptx::NewChartKind::StockVHLC, vhlc_refs).unwrap();
    let ShapeKind::Chart(vhlc_chart) = &vhlc_shape.kind else { panic!() };
    assert_eq!(vhlc_chart.groups.len(), 2);
    assert_eq!(vhlc_chart.groups[0].series.len(), 1);
    assert_eq!(vhlc_chart.groups[0].series[0].name, "出来高");
    assert_eq!(vhlc_chart.groups[1].series.len(), 3);
    assert_eq!(vhlc_chart.groups[1].series[0].name, "高値");

    let v_idx = d.presentation().slide(sheet).unwrap().shapes.len();
    d.execute(vec![Command::InsertShape { slide: sheet, index: v_idx, shape: vhlc_shape }]).unwrap();
    let saved = save(&opened, d.presentation(), "chart-stock-vhlc.xlsx");
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let chart_xml: String = package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(|n| String::from_utf8(package.read(n).unwrap()).unwrap()).collect();
    assert!(chart_xml.contains("<c:barChart>") && chart_xml.contains("<c:stockChart>"), "{chart_xml}");
    assert!(chart_xml.contains("月次売上!$B$46") && chart_xml.contains("月次売上!$E$47:$E$48"), "{chart_xml}");
}

#[test]
fn groupby_and_pivotby_spill_and_a_function_by_its_name_is_saved_as_the_file_has_it() {
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    // 店舗 (A4:A8) with their 4月 (B4:B8): one group each, then the total.
    set(&mut d, s, vec![(0, 9, CellInput::Formula("GROUPBY(A4:A8,B4:B8,SUM)".into())), (0, 12, CellInput::Formula("BYROW(B4:D8,SUM)".into()))]);
    let p = d.presentation();
    // Excel for Mac's order of kanji (by reading: 駅 first, 北 last) where the system has it; else by code.
    let (first, first_sum, last) = if cfg!(any(target_os = "macos", windows)) { ("駅前店", 2_100_000.0, "北町店") } else { ("北町店", 1_250_000.0, "駅前店") };
    assert_eq!(value(p, "月次売上", 0, 9), CellValue::Text(first.into()));
    assert_eq!(value(p, "月次売上", 0, 10), CellValue::Number(first_sum));
    assert_eq!(value(p, "月次売上", 4, 9), CellValue::Text(last.into()));
    assert_eq!(value(p, "月次売上", 5, 9), CellValue::Text("合計".into()));
    assert_eq!(value(p, "月次売上", 5, 10), CellValue::Number(6_620_000.0));
    assert_eq!(value(p, "月次売上", 0, 12), CellValue::Number(3_750_000.0));
    let saved = save(&opened, d.presentation(), "groupby.xlsx");
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let sheet_xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(sheet_xml.contains("_xlfn.BYROW(B4:D8,_xleta.SUM)"), "{sheet_xml}");
    assert!(sheet_xml.contains("_xlfn.GROUPBY(A4:A8,B4:B8,SUM)") || sheet_xml.contains("_xlfn.GROUPBY(A4:A8,B4:B8,_xleta.SUM)"), "{sheet_xml}");
}

/// The クエリ エディター's queries are kept in the file and read back, and the sheet of a loaded result
/// is saved with them.
#[test]
fn queries_and_their_result_sheet_are_saved_and_read_back() {
    use ibis_doc_model::query::{Agg, QTable, Query, Source, Step};
    let (_, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let mut query = Query::new("集計", Source::Range { sheet: "月次売上".into(), range: "A1:C3".into() });
    query.steps = vec![Step::PromoteHeaders, Step::GroupBy { keys: vec!["Column1".into()], aggregations: vec![("合計".into(), Agg::Sum, Some("Column2".into()))] }];
    let table = QTable::from_rows(vec!["店".into(), "合計".into()], vec![vec![CellValue::Text("北".into()), CellValue::Number(3.0)]]);
    d.execute(vec![Command::Sheet(SheetCommand::LoadQuery { slide: s, query: query.clone(), table })]).unwrap();
    let saved = save(&opened, d.presentation(), "queries.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    // A Custom XML Data part with its properties, related from the workbook and listed in the content types.
    let text = String::from_utf8(package.read("customXml/item1.xml").unwrap()).unwrap();
    assert!(text.contains("https://ibis.ne.jp/ibisoffice/queries") && text.contains("query\t集計\t集計"), "{text}");
    let props = String::from_utf8(package.read("customXml/itemProps1.xml").unwrap()).unwrap();
    assert!(props.contains("ds:itemID") && props.contains("schemaRef"), "{props}");
    assert!(String::from_utf8(package.read("customXml/_rels/item1.xml.rels").unwrap()).unwrap().contains("customXmlProps"));
    assert!(String::from_utf8(package.read("[Content_Types].xml").unwrap()).unwrap().contains("/customXml/itemProps1.xml"));
    assert!(String::from_utf8(package.read("xl/_rels/workbook.xml.rels").unwrap()).unwrap().contains("../customXml/item1.xml"));
    let back = ibis_pptx::read(&saved).unwrap();
    let wb = back.workbook.as_ref().unwrap();
    assert_eq!(wb.queries.len(), 1);
    assert_eq!(wb.queries[0].steps, query.steps);
    assert_eq!(wb.queries[0].output.as_deref(), Some("集計"));
    assert!(wb.sheets.iter().any(|s| s.name == "集計"));
    let result = back.slides.iter().find(|s| s.name.as_deref() == Some("集計")).and_then(sheet_of_slide).unwrap();
    assert_eq!(result.cell(0, 0).map(|c| c.value.clone()), Some(CellValue::Text("店".into())));
    assert_eq!(result.cell(1, 1).map(|c| c.value.clone()), Some(CellValue::Number(3.0)));
}

#[test]
fn picture_transparent_color_and_effects_roundtrip() {
    use ibis_doc_model::{Rgb, ShapeKind};
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let (slide_id, shape_id, orig_pic) = d
        .presentation()
        .slides
        .iter()
        .find_map(|s| {
            s.shapes.iter().find_map(|sh| {
                if let ShapeKind::Picture(p) = &sh.kind {
                    Some((s.id, sh.id, p.clone()))
                } else {
                    None
                }
            })
        })
        .expect("xlsx-sales.xlsx has a picture");

    // 1. Set transparent color (background removal) to white (0xFFFFFF), duotone, and luminance
    let mut modified_pic = orig_pic.clone();
    modified_pic.clear = Some(Rgb(0xffffff));
    modified_pic.duotone = Some((Rgb(0x002060), Rgb(0xd9e1f2)));
    modified_pic.luminance = Some((20_000, 10_000));
    d.execute(vec![Command::SetKind {
        slide: slide_id,
        shape: shape_id,
        kind: Box::new(ShapeKind::Picture(modified_pic.clone())),
    }])
    .unwrap();

    let saved = save(&opened, d.presentation(), "picture-clear.xlsx");
    let parts = changed_parts(&data, &saved);
    assert!(parts.iter().any(|p| p.starts_with("xl/drawings/drawing")), "{parts:?}");

    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let drawing_part = parts.iter().find(|p| p.starts_with("xl/drawings/drawing")).unwrap();
    let drawing_xml = String::from_utf8(package.read(drawing_part).unwrap()).unwrap();
    assert!(drawing_xml.contains("clrChange"), "drawing must contain clrChange: {drawing_xml}");
    assert!(drawing_xml.contains("FFFFFF"), "drawing must contain clrFrom/clrTo with FFFFFF: {drawing_xml}");
    assert!(drawing_xml.contains("duotone"), "drawing must contain duotone: {drawing_xml}");
    assert!(drawing_xml.contains("lum"), "drawing must contain lum: {drawing_xml}");

    // 2. Read back and verify picture effects preserved in model
    let again = ibis_pptx::read(&saved).unwrap();
    let reloaded_pic = again
        .slide(slide_id)
        .unwrap()
        .shapes
        .iter()
        .find(|sh| sh.id == shape_id)
        .and_then(|sh| if let ShapeKind::Picture(p) = &sh.kind { Some(p) } else { None })
        .expect("reloaded picture exists");
    assert_eq!(reloaded_pic.clear, Some(Rgb(0xffffff)));
    assert_eq!(reloaded_pic.duotone, Some((Rgb(0x002060), Rgb(0xd9e1f2))));
    assert_eq!(reloaded_pic.luminance, Some((20_000, 10_000)));

    // 3. Reset effects back to None
    let mut reset_pic = reloaded_pic.clone();
    reset_pic.clear = None;
    reset_pic.duotone = None;
    reset_pic.luminance = None;
    d.execute(vec![Command::SetKind {
        slide: slide_id,
        shape: shape_id,
        kind: Box::new(ShapeKind::Picture(reset_pic)),
    }])
    .unwrap();

    let saved_reset = save(&opened, d.presentation(), "picture-clear-reset.xlsx");
    let package_reset = ibis_ooxml::Package::open(saved_reset.clone()).unwrap();
    let reset_xml = String::from_utf8(package_reset.read(drawing_part).unwrap()).unwrap();
    assert!(!reset_xml.contains("clrChange"), "clrChange should be removed: {reset_xml}");
    assert!(!reset_xml.contains("duotone"), "duotone should be removed: {reset_xml}");

    let again_reset = ibis_pptx::read(&saved_reset).unwrap();
    let back_pic = again_reset
        .slide(slide_id)
        .unwrap()
        .shapes
        .iter()
        .find(|sh| sh.id == shape_id)
        .and_then(|sh| if let ShapeKind::Picture(p) = &sh.kind { Some(p) } else { None })
        .expect("reloaded picture exists");
    assert_eq!(back_pic.clear, None);
    assert_eq!(back_pic.duotone, None);
    assert_eq!(back_pic.luminance, None);
}

#[test]
fn test_show_formulas_and_show_zeros_roundtrip() {
    let (bytes, _) = open("xlsx-sales.xlsx");
    let opened = ibis_pptx::open(bytes.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");

    // Initially show_formulas is false, show_zeros is true
    let sh0 = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh0.show_formulas, false);
    assert_eq!(sh0.show_zeros, true);

    // Toggle show_formulas to true, show_zeros to false
    d.execute(vec![
        Command::Sheet(SheetCommand::SetShowFormulas { slide: s, shown: true }),
    ]).unwrap();
    d.execute(vec![
        Command::Sheet(SheetCommand::SetShowZeros { slide: s, shown: false }),
    ]).unwrap();

    let sh1 = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh1.show_formulas, true);
    assert_eq!(sh1.show_zeros, false);

    // Save and verify XML
    let saved = save(&opened, d.presentation(), "show-formulas-zeros.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet_part = "xl/worksheets/sheet1.xml";
    let xml = String::from_utf8(package.read(sheet_part).unwrap()).unwrap();
    assert!(xml.contains(r#"showFormulas="1""#), "sheet xml must have showFormulas=1: {xml}");
    assert!(xml.contains(r#"showZeros="0""#), "sheet xml must have showZeros=0: {xml}");

    // Reopen and verify model
    let again = ibis_pptx::read(&saved).unwrap();
    let sh_again = sheet_of_slide(again.slide(s).unwrap()).unwrap();
    assert_eq!(sh_again.show_formulas, true);
    assert_eq!(sh_again.show_zeros, false);

    // Undo both and verify
    assert!(d.undo());
    let sh_undo1 = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh_undo1.show_formulas, true);
    assert_eq!(sh_undo1.show_zeros, true);

    assert!(d.undo());
    let sh_undone = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh_undone.show_formulas, false);
    assert_eq!(sh_undone.show_zeros, true);

    // Save and verify attributes are removed (absent = default)
    let saved_undone = save(&opened, d.presentation(), "show-formulas-zeros-undone.xlsx");
    let package_undone = ibis_ooxml::Package::open(saved_undone.clone()).unwrap();
    let xml_undone = String::from_utf8(package_undone.read(sheet_part).unwrap()).unwrap();
    assert!(!xml_undone.contains("showFormulas"), "showFormulas should be absent when false: {xml_undone}");
    assert!(!xml_undone.contains("showZeros"), "showZeros should be absent when true: {xml_undone}");
}

#[test]
fn split_panes_save_and_reopen_roundtrip() {
    let (bytes, _) = open("xlsx-sales.xlsx");
    let opened = ibis_pptx::open(bytes.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");

    // 1. Initially no split
    let sh0 = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh0.split, None);

    // 2. Execute SheetCommand::Split at (2, 3) (column 2, row 3)
    d.execute(vec![
        Command::Sheet(SheetCommand::Split { slide: s, split: Some((2, 3)) }),
    ]).unwrap();

    let sh1 = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh1.split, Some((2, 3)));
    assert_eq!(sh1.frozen, None);

    // 3. Save as .xlsx and verify XML has <pane state="split" xSplit="2" ySplit="3" topLeftCell="C4" activePane="bottomRight"/>
    let saved = save(&opened, d.presentation(), "split-panes-test.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet_part = "xl/worksheets/sheet1.xml";
    let xml = String::from_utf8(package.read(sheet_part).unwrap()).unwrap();
    assert!(xml.contains(r#"state="split""#), "sheet xml must have state=split: {xml}");
    assert!(xml.contains(r#"xSplit="2""#), "sheet xml must have xSplit=2: {xml}");
    assert!(xml.contains(r#"ySplit="3""#), "sheet xml must have ySplit=3: {xml}");
    assert!(xml.contains(r#"topLeftCell="C4""#), "sheet xml must have topLeftCell=C4: {xml}");
    assert!(xml.contains(r#"activePane="bottomRight""#), "sheet xml must have activePane=bottomRight: {xml}");

    // 4. Reopen and verify model restores sheet.split
    let again = ibis_pptx::read(&saved).unwrap();
    let sh_again = sheet_of_slide(again.slide(s).unwrap()).unwrap();
    assert_eq!(sh_again.split, Some((2, 3)));
    assert_eq!(sh_again.frozen, None);

    // 5. Undo and verify split is removed in model and XML
    assert!(d.undo());
    let sh_undo = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh_undo.split, None);

    let saved_undo = save(&opened, d.presentation(), "split-panes-undo.xlsx");
    let package_undo = ibis_ooxml::Package::open(saved_undo.clone()).unwrap();
    let xml_undo = String::from_utf8(package_undo.read(sheet_part).unwrap()).unwrap();
    assert!(!xml_undo.contains(r#"state="split""#), "state=split should be absent: {xml_undo}");

    let again_undo = ibis_pptx::read(&saved_undo).unwrap();
    let sh_again_undo = sheet_of_slide(again_undo.slide(s).unwrap()).unwrap();
    assert_eq!(sh_again_undo.split, None);

    // 6. Switch to Freeze panes and verify mutual exclusion and roundtrip
    d.execute(vec![
        Command::Sheet(SheetCommand::Freeze { slide: s, frozen: Some((1, 2)) }),
    ]).unwrap();
    let sh_freeze = sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
    assert_eq!(sh_freeze.frozen, Some((1, 2)));
    assert_eq!(sh_freeze.split, None);

    let saved_freeze = save(&opened, d.presentation(), "split-to-freeze.xlsx");
    let package_freeze = ibis_ooxml::Package::open(saved_freeze.clone()).unwrap();
    let xml_freeze = String::from_utf8(package_freeze.read(sheet_part).unwrap()).unwrap();
    assert!(xml_freeze.contains(r#"state="frozen""#), "sheet xml must have state=frozen: {xml_freeze}");
    assert!(!xml_freeze.contains(r#"state="split""#), "state=split should be absent: {xml_freeze}");

    let again_freeze = ibis_pptx::read(&saved_freeze).unwrap();
    let sh_again_freeze = sheet_of_slide(again_freeze.slide(s).unwrap()).unwrap();
    assert_eq!(sh_again_freeze.frozen, Some((1, 2)));
    assert_eq!(sh_again_freeze.split, None);
}

#[test]
fn new_sheet_chart_supports_radar_surface_combo_and_of_pie_roundtrip() {
    use ibis_doc_model::{CellRange, EmuRect, ShapeKind};
    use ibis_pptx::NewChartKind;
    let (_data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let sheet = slide(d.presentation(), "月次売上");
    let charts_before = shapes_of(d.presentation(), "月次売上").iter().filter(|s| matches!(s.kind, ShapeKind::Chart(_))).count();
    // A3:D8: stores down the side, months across top (3 value columns: 4月, 5月, 6月).
    let range = CellRange { first_row: 2, first_column: 0, last_row: 7, last_column: 3 };
    let refs = ibis_pptx::refs_for_range(d.presentation(), "月次売上", range, true).expect("refs");
    assert_eq!(refs.len(), 3);

    let kinds = [
        NewChartKind::PieOfPie,
        NewChartKind::BarOfPie,
        NewChartKind::Radar,
        NewChartKind::RadarFilled,
        NewChartKind::Surface3D,
        NewChartKind::Surface3DWireframe,
        NewChartKind::Contour,
        NewChartKind::ContourWireframe,
        NewChartKind::ComboColumnLine,
        NewChartKind::ComboColumnLineSecondaryAxis,
        NewChartKind::ComboAreaColumn,
    ];

    for kind in kinds {
        let id = d.ids().new_shape_id();
        let shape = opened.source.new_sheet_chart(
            d.presentation(),
            sheet,
            id,
            EmuRect::new(1_000_000, 4_000_000, 5_000_000, 3_000_000),
            kind,
            refs.clone(),
        ).unwrap_or_else(|e| panic!("failed to insert {kind:?}: {e}"));
        let ShapeKind::Chart(c) = &shape.kind else { panic!("a chart") };
        assert!(!c.groups.is_empty(), "{kind:?} must have at least one group");
        let total_series: usize = c.groups.iter().map(|g| g.series.len()).sum();
        assert_eq!(total_series, 3, "{kind:?} must distribute all 3 series");

        let index = d.presentation().slide(sheet).unwrap().shapes.len();
        d.execute(vec![Command::InsertShape { slide: sheet, index, shape: shape.clone() }]).unwrap();
    }

    let saved = save(&opened, d.presentation(), "advanced-sheet-charts.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    for name in package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")) {
        let chart_xml = String::from_utf8(package.read(&name).unwrap()).unwrap();
        assert!(!chart_xml.contains("externalData"), "sheet chart {name} must not contain externalData");
        assert!(chart_xml.contains("月次売上!$B$4:$B$8") || chart_xml.contains("月次売上!$B$3"), "chart {name} must reference sheet data");
    }

    let again = ibis_pptx::open(saved).unwrap();
    let charts: Vec<_> = shapes_of(&again.presentation, "月次売上").into_iter().filter(|s| matches!(s.kind, ShapeKind::Chart(_))).collect();
    assert_eq!(charts.len(), charts_before + kinds.len());
}

/// Non-ECMA-376 error values without formulas (e.g. #SPILL!, #CALC!) must be saved with t="str"
/// so that Excel for Mac opens them without hanging, and read back as CellValue::Error.
/// Legacy ECMA-376 errors (e.g. #DIV/0!) must still be saved with t="e".
#[test]
fn non_ecma376_cell_error_literal_saved_as_str_and_read_back() {
    use ibis_doc_model::CellError;
    // 1. Read a worksheet that has raw `<c t="e"><v>#SPILL!</v></c>` (and #FIELD!, #BLOCKED!, #CONNECT!, #UNKNOWN!)
    let base = ibis_pptx::blank_file(ibis_pptx::Kind::Workbook).unwrap();
    let mut pkg = ibis_ooxml::Package::open(base).unwrap();
    let raw_sheet = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="B1" t="e"><v>#SPILL!</v></c>
      <c r="C1" t="e"><v>#FIELD!</v></c>
      <c r="D1" t="e"><v>#BLOCKED!</v></c>
      <c r="E1" t="e"><v>#CONNECT!</v></c>
      <c r="F1" t="e"><v>#UNKNOWN!</v></c>
    </row>
  </sheetData>
</worksheet>"#;
    pkg.set_bytes("xl/worksheets/sheet1.xml", raw_sheet.as_bytes().to_vec());
    let raw_opened = ibis_pptx::open(pkg.save(SaveOptions::default()).unwrap()).unwrap();
    let expected = [
        (1, CellError::Spill, "#SPILL!"),
        (2, CellError::Field, "#FIELD!"),
        (3, CellError::Blocked, "#BLOCKED!"),
        (4, CellError::Connect, "#CONNECT!"),
        (5, CellError::Unknown, "#UNKNOWN!"),
    ];
    for &(col, err, _) in &expected {
        assert_eq!(cell(&raw_opened.presentation, "Sheet1", 0, col).value, CellValue::Error(err));
    }

    // 2. Writing those 5 errors as cell literals (no formula) saves them as t="str" and reads back as CellValue::Error.
    let (_, opened) = open("xlsx-sparkline.xlsx");
    let s = slide(&opened.presentation, "縦位置");
    let mut d = Document::new(opened.presentation.clone());

    let mut edits: Vec<(u32, u32, CellInput)> = expected
        .iter()
        .enumerate()
        .map(|(r, &(_, err, _))| (r as u32, 0, CellInput::Value(CellValue::Error(err))))
        .collect();
    edits.push((5, 0, CellInput::Value(CellValue::Error(CellError::Div0))));
    set(&mut d, s, edits);

    let saved = save(&opened, d.presentation(), "error_literals.xlsx");
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let sheet_xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();

    for (r, &(_, _, text)) in expected.iter().enumerate() {
        let cell_ref = format!("A{}", r + 1);
        assert!(
            sheet_xml.contains(&format!("r=\"{cell_ref}\" t=\"str\"")) && sheet_xml.contains(&format!("<v>{text}</v>")),
            "{cell_ref} ({text}) must be saved as t=\"str\": {sheet_xml}"
        );
    }
    assert!(
        sheet_xml.contains("r=\"A6\" t=\"e\"") && sheet_xml.contains("<v>#DIV/0!</v>"),
        "A6 (#DIV/0!) must be saved as t=\"e\": {sheet_xml}"
    );

    let reread = ibis_pptx::read(&saved).unwrap();
    for (r, &(_, err, _)) in expected.iter().enumerate() {
        assert_eq!(cell(&reread, "縦位置", r as u32, 0).value, CellValue::Error(err));
    }
    assert_eq!(cell(&reread, "縦位置", 5, 0).value, CellValue::Error(CellError::Div0));
}

#[test]
fn external_workbook_formula_reference_roundtrip() {
    let base = ibis_pptx::blank_file(ibis_pptx::Kind::Workbook).unwrap();
    let mut pkg = ibis_ooxml::Package::open(base).unwrap();
    let raw_sheet = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1"><v>10</v></c>
      <c r="B1"><f>'[External.xlsx]Sheet1'!$A$1</f><v>42</v></c>
    </row>
  </sheetData>
</worksheet>"#;
    pkg.set_bytes("xl/worksheets/sheet1.xml", raw_sheet.as_bytes().to_vec());
    let opened = ibis_pptx::open(pkg.save(SaveOptions::default()).unwrap()).unwrap();
    assert_eq!(formula(&opened.presentation, "Sheet1", 0, 1).as_deref(), Some("'[External.xlsx]Sheet1'!$A$1"));
    assert_eq!(value(&opened.presentation, "Sheet1", 0, 1), CellValue::Number(42.0));

    let s = slide(&opened.presentation, "Sheet1");
    let mut d = Document::new(opened.presentation.clone());
    set(&mut d, s, vec![(0, 0, CellInput::Value(CellValue::Number(20.0)))]);

    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let reread = ibis_pptx::open(saved).unwrap();
    assert_eq!(value(&reread.presentation, "Sheet1", 0, 0), CellValue::Number(20.0));
    assert_eq!(formula(&reread.presentation, "Sheet1", 0, 1).as_deref(), Some("'[External.xlsx]Sheet1'!$A$1"));
    assert_eq!(value(&reread.presentation, "Sheet1", 0, 1), CellValue::Number(42.0));
}

#[test]
fn chart_follows_cells_after_structure_edits_and_save() {
    use ibis_doc_model::sheet_command::Axis;

    let (_, opened) = open("xlsx-sales.xlsx");
    let s = slide(&opened.presentation, "月次売上");
    let s2 = opened.presentation.slides[2].id;
    let mut d = Document::new(opened.presentation.clone());
    let first_value = |p: &ibis_doc_model::Presentation| -> Option<f64> {
        p.slide(s)
            .unwrap()
            .shapes
            .iter()
            .find_map(|sh| match &sh.kind {
                ibis_doc_model::ShapeKind::Chart(c) => c.groups[0].series[0].values.first().copied().flatten().map(|n| n.0),
                _ => None,
            })
    };
    assert_eq!(first_value(d.presentation()), Some(1_250_000.0));

    // 1. Insert a row above row 1: B4 moves to B5 (row 4, col 1), and editing B5 updates the chart live.
    d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Rows, at: 0, count: 1 })]).unwrap();
    set(&mut d, s, vec![(4, 1, CellInput::Value(CellValue::Number(7_500_000.0)))]);
    assert_eq!(first_value(d.presentation()), Some(7_500_000.0), "chart follows cell after row insert");
    assert!(d.undo());
    assert!(d.undo());
    assert_eq!(first_value(d.presentation()), Some(1_250_000.0), "undo row insert restores chart");

    // 2. Delete row 4 (index 3), then edit the new B4 (row 3, col 1).
    d.execute(vec![Command::Sheet(SheetCommand::Delete { slide: s, axis: Axis::Rows, at: 3, count: 1 })]).unwrap();
    set(&mut d, s, vec![(3, 1, CellInput::Value(CellValue::Number(6_400_000.0)))]);
    assert_eq!(first_value(d.presentation()), Some(6_400_000.0), "chart follows cell after row delete");
    assert!(d.undo());
    assert!(d.undo());
    assert_eq!(first_value(d.presentation()), Some(1_250_000.0));

    // 3. Rename the sheet, move the sheet, and delete another sheet; then edit B4 and save.
    d.execute(vec![Command::Sheet(SheetCommand::RenameSheet { slide: s, name: "売上データ".into() })]).unwrap();
    d.execute(vec![Command::Sheet(SheetCommand::MoveSheet { slide: s, to: 1 })]).unwrap();
    d.execute(vec![Command::Sheet(SheetCommand::DeleteSheet { slide: s2 })]).unwrap();
    set(&mut d, s, vec![(3, 1, CellInput::Value(CellValue::Number(9_000_000.0)))]);
    assert_eq!(first_value(d.presentation()), Some(9_000_000.0), "chart follows cell after sheet rename/move/delete");

    let saved = save(&opened, d.presentation(), "chart-follows-structure.xlsx");
    let reread = ibis_pptx::read(&saved).unwrap();
    let reread_s = slide(&reread, "売上データ");
    let reread_first = reread.slide(reread_s).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Chart(c) => c.groups[0].series[0].values.first().copied().flatten().map(|n| n.0),
        _ => None,
    });
    assert_eq!(reread_first, Some(9_000_000.0), "saved chart keeps updated value after structure edits");

    // 4. A chartEx chart also continues following its cells after sheet rename and row insert.
    let (_, ex_opened) = open("xlsx-chartex-funnel.xlsx");
    let ex_s = ex_opened.presentation.slides[0].id;
    let mut ex_doc = Document::new(ex_opened.presentation.clone());
    ex_doc.execute(vec![Command::Sheet(SheetCommand::RenameSheet { slide: ex_s, name: "FunnelRenamed".into() })]).unwrap();
    ex_doc.execute(vec![Command::Sheet(SheetCommand::Insert { slide: ex_s, axis: Axis::Rows, at: 0, count: 1 })]).unwrap();
    set(&mut ex_doc, ex_s, vec![(2, 1, CellInput::Value(CellValue::Number(99_999.0)))]);
    let ex_vals: Vec<Option<f64>> = ex_doc.presentation().slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Chart(c) => Some(c.groups[0].series.iter().flat_map(|s| &s.values).map(|v| v.map(|n| n.0)).collect()),
        _ => None,
    }).unwrap();
    assert!(ex_vals.contains(&Some(99_999.0)), "chartEx follows cell edit after sheet rename and row insert: {ex_vals:?}");
    let ex_saved = save(&ex_opened, ex_doc.presentation(), "chartex-follows-structure.xlsx");
    let ex_reread = ibis_pptx::read(&ex_saved).unwrap();
    let ex_reread_vals: Vec<Option<f64>> = ex_reread.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Chart(c) => Some(c.groups[0].series.iter().flat_map(|s| &s.values).map(|v| v.map(|n| n.0)).collect()),
        _ => None,
    }).unwrap();
    assert!(ex_reread_vals.contains(&Some(99_999.0)), "saved chartEx keeps updated value: {ex_reread_vals:?}");
}

#[test]
fn pivot_chart_insert_sync_and_save_roundtrip() {
    use ibis_doc_model::{EmuRect, Shape, ShapeKind};

    let (_, opened) = open("xlsx-pivot-not-refreshed.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let id = opened.presentation.slides[0].id;
    let s = d.presentation().slide(id).unwrap();
    let sh = sheet_of_slide(s).unwrap();
    let pivot = sh.pivots[0].clone();
    let data = ibis_pptx::pivot_source(d.presentation(), sh, &pivot).unwrap();
    let built = pivot.build(&data);

    // 1. Create a PivotChart from the PivotTable and insert it on the sheet.
    let chart = ibis_doc_model::pivot::chart_from_pivot(&pivot, &built);
    assert_eq!(chart.groups[0].series.len(), 4);
    let initial_categories = chart.groups[0].series[0].categories.clone();
    let initial_values = chart.groups[0].series[0].values.clone();
    let shape_index = s.shapes.len();
    let shape_id = d.ids().new_shape_id();
    let shape = Shape {
        id: shape_id,
        frame: EmuRect::new(1_000_000, 500_000, 4_500_000, 3_000_000),
        rotation: 0,
        flip_h: false,
        flip_v: false,
        kind: ShapeKind::Chart(Box::new(chart)),
        floating: None,
    };
    d.execute(vec![Command::InsertShape { slide: id, index: shape_index, shape }]).unwrap();

    // 2. Save and verify that the new PivotChart round-trips with pivotSource and series data.
    let saved1 = save(&opened, d.presentation(), "pivot-chart-insert-roundtrip.xlsx");
    let reopened1 = ibis_pptx::open(saved1).unwrap();
    let chart1 = reopened1.presentation.slide(id).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) if c.is_pivot_chart() => Some((**c).clone()),
        _ => None,
    }).expect("saved workbook should contain the newly inserted PivotChart");
    assert_eq!(chart1.pivot_source.as_deref(), Some(pivot.name.as_str()));
    assert_eq!(chart1.groups[0].series.len(), 4);
    assert_eq!(chart1.groups[0].series[0].categories, initial_categories);
    assert_eq!(chart1.groups[0].series[0].values, initial_values);

    // 3. Reopen the saved workbook, change the PivotTable layout so series count changes from 4 to 1,
    // sync the PivotChart via update_chart_from_pivot, and save again.
    let mut d2 = Document::new(reopened1.presentation.clone());
    let mut p2 = sheet_of_slide(d2.presentation().slide(id).unwrap()).unwrap().pivots[0].clone();
    if let Some(pdata) = ibis_pptx::pivot_data(d2.presentation(), id, 0) {
        p2.fill_shared(&pdata);
    }
    p2.place(1, None, 0);
    let sh2 = ibis_pptx::rebuilt_pivot(d2.presentation(), id, 0, Some(p2)).unwrap();
    let data2 = ibis_pptx::pivot_source(d2.presentation(), &sh2, &sh2.pivots[0]).unwrap();
    let built2 = sh2.pivots[0].build(&data2);
    let mut updated_chart = chart1.clone();
    ibis_doc_model::pivot::update_chart_from_pivot(&mut updated_chart, &sh2.pivots[0], &built2);
    assert_eq!(updated_chart.groups[0].series.len(), 1);
    let expected_categories = updated_chart.groups[0].series[0].categories.clone();
    let expected_values = updated_chart.groups[0].series[0].values.clone();

    let chart_shape_id = d2.presentation().slide(id).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) if c.is_pivot_chart() => Some(sh.id),
        _ => None,
    }).unwrap();
    d2.execute(vec![
        Command::Sheet(SheetCommand::ReplaceSheet { slide: id, sheet: Box::new(sh2) }),
        Command::SetKind { slide: id, shape: chart_shape_id, kind: Box::new(ShapeKind::Chart(Box::new(updated_chart))) },
    ]).unwrap();

    let saved2 = reopened1.source.save(d2.presentation(), SaveOptions::default()).unwrap();
    let reopened2 = ibis_pptx::read(&saved2).unwrap();
    let chart2 = reopened2.slide(id).unwrap().shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) if c.is_pivot_chart() => Some((**c).clone()),
        _ => None,
    }).expect("second saved workbook should preserve the updated PivotChart");
    assert_eq!(chart2.pivot_source.as_deref(), Some(pivot.name.as_str()));
    assert_eq!(chart2.groups[0].series.len(), 1);
    assert_eq!(chart2.groups[0].series[0].categories, expected_categories);
    assert_eq!(chart2.groups[0].series[0].values, expected_values);
}

#[test]
fn timeline_insert_move_style_grouped_filter_and_delete_roundtrip() {
    use ibis_doc_model::timeline::Timeline;
    use ibis_doc_model::{Command, Document, EmuRect, Shape, ShapeId, ShapeKind};
    use ibis_pptx::SaveOptions;
    use std::sync::Arc;

    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/editing/xlsx-timeline-pivot.xlsx"),
    )
    .unwrap();
    let loaded = ibis_pptx::open(bytes).unwrap();
    let mut doc = Document::new(loaded.presentation.clone());
    let slide_id = doc.presentation().slides[0].id;

    // 1. Add a second Timeline (Years level = 0, TimeSlicerStyleDark3) and save
    let existing_tl = doc
        .presentation()
        .slide(slide_id)
        .unwrap()
        .shapes
        .iter()
        .find_map(|sh| match &sh.kind {
            ShapeKind::Timeline(tl) => Some((**tl).clone()),
            _ => None,
        })
        .unwrap();
    let new_tl_id = ShapeId(9901);
    let p0 = existing_tl.periods(0);
    let sel_year = (p0[0].start, p0[0].end);
    let new_tl = Timeline {
        name: "NativeTimeline2".into(),
        caption: "日付 (年)".into(),
        cache: String::new(),
        source: existing_tl.source.clone(),
        pivots: existing_tl.pivots.clone(),
        level: 0,
        selection_level: 0,
        scroll: existing_tl.bounds.0,
        bounds: existing_tl.bounds,
        selection: Some(sel_year),
        show_header: true,
        show_selection_label: true,
        show_time_level: false,
        show_scroll_bar: true,
        style: "TimeSlicerStyleDark3".into(),
        part: String::new(),
        cache_part: String::new(),
    };
    let shape = Shape::new(
        new_tl_id,
        EmuRect {
            x: 4572000,
            y: 2286000,
            width: 3200400,
            height: 1371600,
        },
        ShapeKind::Timeline(Arc::new(new_tl)),
    );
    let index = doc.presentation().slide(slide_id).unwrap().shapes.len();
    doc.execute(vec![Command::InsertShape {
        slide: slide_id,
        index,
        shape,
    }])
    .unwrap();

    let saved1 = loaded
        .source
        .save(doc.presentation(), SaveOptions::default())
        .unwrap();
    let reopened1 = ibis_pptx::open(saved1).unwrap();
    let timelines1: Vec<Timeline> = reopened1
        .presentation
        .slide(slide_id)
        .unwrap()
        .shapes
        .iter()
        .filter_map(|sh| match &sh.kind {
            ShapeKind::Timeline(tl) => Some((**tl).clone()),
            _ => None,
        })
        .collect();
    assert_eq!(timelines1.len(), 2, "newly added timeline should be saved and reopened");
    let added = timelines1
        .iter()
        .find(|t| t.caption == "日付 (年)")
        .expect("reopened second timeline");
    assert_eq!(added.style, "TimeSlicerStyleDark3");
    assert_eq!(added.level, 0);
    assert_eq!(added.selection_level, 0);
    assert_eq!(added.selection, Some(sel_year));
    assert!(!added.show_time_level);

    // 2. Delete both timelines and verify OOXML removal round-trip
    let mut doc2 = Document::new(reopened1.presentation.clone());
    let tl_ids: Vec<ShapeId> = doc2
        .presentation()
        .slide(slide_id)
        .unwrap()
        .shapes
        .iter()
        .filter_map(|sh| matches!(sh.kind, ShapeKind::Timeline(_)).then_some(sh.id))
        .collect();
    assert_eq!(tl_ids.len(), 2);
    for id in tl_ids {
        doc2.execute(vec![Command::RemoveShape {
            slide: slide_id,
            shape: id,
        }])
        .unwrap();
    }
    let saved2 = reopened1
        .source
        .save(doc2.presentation(), SaveOptions::default())
        .unwrap();
    let reopened2 = ibis_pptx::read(&saved2).unwrap();
    let remaining_timelines = reopened2
        .slide(slide_id)
        .unwrap()
        .shapes
        .iter()
        .filter(|sh| matches!(sh.kind, ShapeKind::Timeline(_)))
        .count();
    assert_eq!(remaining_timelines, 0, "deleted timelines should be removed from OOXML");

    // 3. Verify SelectionStart, SelectionEnd, ScrollThumb hit-testing and grouped date field filtering
    use ibis_doc_model::timeline::{
        CAPTION, CELL_WIDTH, GROUP, SCROLL, SUB, TimelineHit, first_shown, hit, period_at_x,
        thumb_rect,
    };
    let mut tl_sel = existing_tl.clone();
    let periods = tl_sel.periods(tl_sel.selection_level);
    let first = first_shown(&periods, tl_sel.scroll);
    tl_sel.selection = Some((periods[first].start, periods[first + 1].end));
    let x_start = SCROLL;
    let x_end = SCROLL + 2 * CELL_WIDTH;
    let width = 3200400;
    let height = 1371600;
    let body_y = (CAPTION + SUB + GROUP + height - SCROLL) / 2;
    assert_eq!(
        hit(&tl_sel, width, height, x_start, body_y),
        Some(TimelineHit::SelectionStart)
    );
    assert_eq!(
        hit(&tl_sel, width, height, x_end, body_y),
        Some(TimelineHit::SelectionEnd)
    );
    let (tx0, tx1) = thumb_rect(&tl_sel, &periods, width);
    assert_eq!(
        hit(&tl_sel, width, height, (tx0 + tx1) / 2, height - SCROLL / 2),
        Some(TimelineHit::ScrollThumb)
    );
    let p2_x = SCROLL + 2 * CELL_WIDTH + CELL_WIDTH / 2;
    assert_eq!(period_at_x(&tl_sel, &periods, width, p2_x), Some(first + 2));

    // 4. Verify grouped date field filtering via Timeline (date field placed on Row, grouped by Months)
    use ibis_doc_model::pivot::{DateGroupBy, PivotAxis, PivotFieldGrouping, PivotItem};
    let mut p = sheet_of_slide(loaded.presentation.slide(slide_id).unwrap())
        .unwrap()
        .pivots[0]
        .clone();
    if let Some(pdata) = ibis_pptx::pivot_data(&loaded.presentation, slide_id, 0) {
        p.fill_shared(&pdata);
    }
    p.place(0, Some(PivotAxis::Row), 0);
    p.place(1, None, 0);
    p.fields[0].grouping = Some(PivotFieldGrouping::Date {
        by: DateGroupBy::Months,
        start: None,
        end: None,
        auto_start: true,
        auto_end: true,
    });
    // Filter date field items by first period only
    let mut tl_first = existing_tl.clone();
    tl_first.selection = Some((periods[0].start, periods[0].end));
    if p.fields[0].items.len() < p.fields[0].shared.len() {
        p.fields[0].items = (0..p.fields[0].shared.len() as u32)
            .map(|i| PivotItem {
                x: Some(i),
                kind: None,
                hidden: false,
            })
            .collect();
    }
    let shared = p.fields[0].shared.clone();
    for item in &mut p.fields[0].items {
        if let Some(Some(&ibis_doc_model::CellValue::Number(n))) =
            item.x.map(|i| shared.get(i as usize))
        {
            item.hidden = !tl_first.keeps(n);
        }
    }
    let sh_grouped = ibis_pptx::rebuilt_pivot(&loaded.presentation, slide_id, 0, Some(p)).unwrap();
    let data_grouped =
        ibis_pptx::pivot_source(&loaded.presentation, &sh_grouped, &sh_grouped.pivots[0]).unwrap();
    let built_grouped = sh_grouped.pivots[0].build(&data_grouped);
    assert!(
        built_grouped.cells.len() >= 3,
        "Grouped date PivotTable should retain filtered month bucket"
    );
}

#[test]
fn form_control_insert_move_resize_format_and_delete_roundtrip() {
    use ibis_doc_model::{ControlKind, EmuRect, FormControl, Rgb, Shape, ShapeId, ShapeKind};
    use ibis_pptx::SaveOptions;
    use std::sync::Arc;

    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/editing/xlsx-form-controls.xlsx"),
    )
    .expect("read xlsx-form-controls.xlsx");
    let mut loaded = ibis_pptx::open(bytes).expect("load xlsx-form-controls");
    let slide_id = loaded.presentation.slides[0].id;

    // 1. Move, resize, and format the Button control, and toggle 3D on the CheckBox control
    let slide = loaded.presentation.slide_mut(slide_id).unwrap();
    let btn_shape_id = slide
        .shapes
        .iter()
        .find_map(|sh| match &sh.kind {
            ShapeKind::Control(c) if c.kind == ControlKind::Button => Some(sh.id),
            _ => None,
        })
        .expect("find button control");
    if let Some(sh) = slide.shapes.iter_mut().find(|sh| sh.id == btn_shape_id) {
        sh.frame.x += 914400; // +1 inch
        sh.frame.y += 457200; // +0.5 inch
        sh.frame.width += 457200;
        sh.frame.height += 228600;
        if let ShapeKind::Control(c) = &mut sh.kind {
            let mut c_mut = (**c).clone();
            c_mut.text = "実行更新".to_string();
            c_mut.font = Some("Yu Gothic".to_string());
            c_mut.font_size = Some(1400); // 14pt
            c_mut.bold = true;
            c_mut.italic = true;
            c_mut.text_color = Some(Rgb::from_rgb(0xCC, 0x22, 0x11));
            *c = Arc::new(c_mut);
        }
    }
    let orig_btn_x = slide
        .shapes
        .iter()
        .find(|sh| sh.id == btn_shape_id)
        .unwrap()
        .frame
        .x;

    // Toggle CheckBox three_d to false (noThreeD="1")
    let chk_shape_id = slide
        .shapes
        .iter()
        .find_map(|sh| match &sh.kind {
            ShapeKind::Control(c) if c.kind == ControlKind::CheckBox => Some(sh.id),
            _ => None,
        })
        .expect("find checkbox control");
    if let Some(sh) = slide.shapes.iter_mut().find(|sh| sh.id == chk_shape_id) {
        if let ShapeKind::Control(c) = &mut sh.kind {
            let mut c_mut = (**c).clone();
            c_mut.three_d = false;
            c_mut.checked = 0;
            *c = Arc::new(c_mut);
        }
    }

    // Delete the Label control
    let label_idx = slide
        .shapes
        .iter()
        .position(|sh| matches!(&sh.kind, ShapeKind::Control(c) if c.kind == ControlKind::Label))
        .expect("find label control");
    slide.shapes.remove(label_idx);

    // Insert a brand-new CheckBox control
    let mut new_ctrl = FormControl::new(ControlKind::CheckBox, "Check Box 99");
    new_ctrl.text = "新規チェック".to_string();
    new_ctrl.link = Some("$H$10".to_string());
    new_ctrl.checked = 1;
    new_ctrl.three_d = true;
    new_ctrl.font = Some("Meiryo".to_string());
    new_ctrl.font_size = Some(1100);
    new_ctrl.bold = true;
    new_ctrl.italic = false;
    new_ctrl.text_color = Some(Rgb::from_rgb(0x11, 0x44, 0x88));
    let new_sh = Shape::new(
        ShapeId(9999),
        EmuRect {
            x: 914400 * 5,
            y: 914400 * 6,
            width: 914400 * 2,
            height: 274320,
        },
        ShapeKind::Control(Arc::new(new_ctrl)),
    );
    slide.shapes.push(new_sh);

    // Save and reload
    let saved1 = loaded
        .source
        .save(&loaded.presentation, SaveOptions::default())
        .expect("save1");
    let reloaded1 = ibis_pptx::open(saved1).expect("reload1");
    let slide1 = &reloaded1.presentation.slides[0];
    let ctrls1: Vec<(&Shape, &FormControl)> = slide1
        .shapes
        .iter()
        .filter_map(|sh| match &sh.kind {
            ShapeKind::Control(c) => Some((sh, c.as_ref())),
            _ => None,
        })
        .collect();
    // 10 original - 1 deleted Label + 1 inserted CheckBox = 10 controls
    assert_eq!(ctrls1.len(), 10, "expected 10 controls after delete + insert");
    assert!(
        !ctrls1.iter().any(|(_, c)| c.kind == ControlKind::Label),
        "Deleted Label control should not remain after reload"
    );

    let (btn_sh, btn_c) = ctrls1
        .iter()
        .find(|(_, c)| c.kind == ControlKind::Button)
        .expect("reloaded button");
    assert!(
        (btn_sh.frame.x - orig_btn_x).abs() < 50000,
        "Button moved X should roundtrip: got {} vs {}",
        btn_sh.frame.x,
        orig_btn_x
    );
    assert_eq!(btn_c.text, "実行更新");
    assert_eq!(btn_c.font.as_deref(), Some("Yu Gothic"));
    assert_eq!(btn_c.font_size, Some(1400));
    assert!(btn_c.bold);
    assert!(btn_c.italic);
    assert_eq!(btn_c.text_color, Some(Rgb::from_rgb(0xCC, 0x22, 0x11)));

    let (_, chk_orig) = ctrls1
        .iter()
        .find(|(_, c)| c.kind == ControlKind::CheckBox && c.text != "新規チェック")
        .expect("reloaded original checkbox");
    assert!(!chk_orig.three_d, "CheckBox three_d=false (noThreeD=1) should roundtrip");

    let (_, chk_new) = ctrls1
        .iter()
        .find(|(_, c)| c.kind == ControlKind::CheckBox && c.text == "新規チェック")
        .expect("reloaded newly inserted checkbox");
    assert!(chk_new.three_d, "Inserted CheckBox three_d=true should roundtrip");
    assert_eq!(chk_new.link.as_deref(), Some("$H$10"));
    assert_eq!(chk_new.font.as_deref(), Some("Meiryo"));
    assert_eq!(chk_new.font_size, Some(1100));
    assert!(chk_new.bold);
    assert!(!chk_new.italic);
    assert_eq!(chk_new.text_color, Some(Rgb::from_rgb(0x11, 0x44, 0x88)));

    // Verify thumb_value helper for continuous scrollbar dragging
    let (_, scroll_c) = ctrls1
        .iter()
        .find(|(_, c)| c.kind == ControlKind::Scroll)
        .expect("reloaded scrollbar");
    let mid_val = ibis_doc_model::form_control::thumb_value(scroll_c, 1828800, 190500, 914400, 0);
    assert!(
        mid_val >= scroll_c.min && mid_val <= scroll_c.max,
        "thumb_value should compute value within [min, max]: got {mid_val}"
    );
}


/// Only a width the user fixed is saved, with `customWidth="1"`; a width the sheet chose (a number wider than its column) is not,
/// so the file opens with that column as it was (as Excel's), and the fixed one reads back as fixed.
#[test]
fn only_fixed_column_widths_are_saved() {
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut d = Document::new(opened.presentation.clone());
    let s = slide(d.presentation(), "月次売上");
    let pt = ibis_doc_model::EMU_PER_POINT;
    let sheet = |p: &Presentation| sheet_of_slide(p.slide(s).unwrap()).unwrap().clone();
    let before = sheet(d.presentation());
    // B fixed by the user; G widened by the sheet.
    d.execute(vec![Command::Sheet(SheetCommand::SetSize { slide: s, axis: ibis_doc_model::sheet_command::Axis::Columns, first: 1, last: 1, size: 90 * pt })]).unwrap();
    d.execute(vec![Command::Sheet(SheetCommand::SetColumnWidthAuto { slide: s, first: 6, last: 6, size: 120 * pt })]).unwrap();
    let edited = sheet(d.presentation());
    assert!(edited.custom_columns.contains(&1) && !edited.custom_columns.contains(&6));
    let saved = save(&opened, d.presentation(), "fixed-column-widths.xlsx");
    let again = ibis_pptx::read(&saved).unwrap();
    let read = sheet_of_slide(again.slides.iter().find(|x| x.name.as_deref() == Some("月次売上")).unwrap()).unwrap().clone();
    assert_eq!(read.columns[1], edited.columns[1]);
    assert!(read.custom_columns.contains(&1), "a fixed width reads back as fixed");
    assert_eq!(read.columns[6], before.columns[6], "a width the sheet chose is not saved");
    assert!(!read.custom_columns.contains(&6));
    // Undo takes the width and the mark back.
    assert!(d.undo() && d.undo());
    assert_eq!(sheet(d.presentation()).custom_columns, before.custom_columns);
    assert_eq!(sheet(d.presentation()).columns, before.columns);
    // Fixed, then given back to the sheet: the width stays, the mark goes, in the saved file too.
    d.execute(vec![Command::Sheet(SheetCommand::SetSize { slide: s, axis: ibis_doc_model::sheet_command::Axis::Columns, first: 1, last: 1, size: 90 * pt })]).unwrap();
    let saved = save(&opened, d.presentation(), "fixed-column-widths-2.xlsx");
    let reopened = ibis_pptx::open(saved).unwrap();
    let mut d2 = Document::new(reopened.presentation.clone());
    d2.execute(vec![Command::Sheet(SheetCommand::SetColumnWidthAuto { slide: s, first: 1, last: 1, size: 80 * pt })]).unwrap();
    let saved2 = reopened.source.save(d2.presentation(), SaveOptions::default()).unwrap();
    let read2 = sheet_of_slide(ibis_pptx::read(&saved2).unwrap().slides.iter().find(|x| x.name.as_deref() == Some("月次売上")).unwrap()).unwrap().clone();
    assert!(!read2.custom_columns.contains(&1));
    // The width is saved in characters to 1/256 and read back on whole pixels.
    assert!((read2.columns[1] - sheet(d2.presentation()).columns[1]).abs() < pt / 2);
    while d.undo() {}
    assert!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap() == data);
}

/// The rows the app measures by their fonts when it opens a file (`Sheet::measured_rows`, `fitted_rows`) are not written back
/// as `customHeight` rows: a file opened and saved without an edit is the file, and a row the person resizes is written.
#[test]
fn rows_the_app_measured_on_opening_are_not_saved_as_custom_heights() {
    let (data, opened) = open("xlsx-sales.xlsx");
    let mut p = opened.presentation.clone();
    let mut measured = 0;
    for slide in &mut p.slides {
        for shape in &mut slide.shapes {
            let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
            let sheet = std::sync::Arc::make_mut(sheet);
            for r in sheet.measured_rows.0.clone() {
                if let Some(h) = sheet.rows.get_mut(r as usize) && *h > 0 {
                    *h += 7 * ibis_doc_model::EMU_PER_POINT / 4;
                    sheet.fitted_rows.insert(r, *h);
                    measured += 1;
                }
            }
        }
    }
    assert!(measured > 0, "the file has rows that are measured");
    let unedited = opened.source.save(&p, SaveOptions::default()).unwrap();
    assert_eq!(changed_parts(&data, &unedited), Vec::<String>::new(), "a measured row was written back");
    // A row the person then resizes (one in each sheet) is written, with customHeight.
    let mut resized = 0;
    for slide in &mut p.slides {
        for shape in &mut slide.shapes {
            let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
            let sheet = std::sync::Arc::make_mut(sheet);
            if let Some(&r) = sheet.measured_rows.first() {
                sheet.rows[r as usize] += ibis_doc_model::EMU_PER_POINT;
                resized += 1;
            }
        }
    }
    let saved = opened.source.save(&p, SaveOptions::default()).unwrap();
    assert!(!changed_parts(&data, &saved).is_empty());
    let count = |bytes: &[u8]| {
        let package = ibis_ooxml::Package::open(bytes.to_vec()).unwrap();
        let names: Vec<String> = package.names().filter(|n| n.starts_with("xl/worksheets/sheet")).map(str::to_string).collect();
        names.iter().map(|n| String::from_utf8_lossy(&package.read(n).unwrap()).matches("customHeight=\"1\"").count()).sum::<usize>()
    };
    assert_eq!(count(&saved), count(&data) + resized, "the resized row is written with customHeight");
}

/// Rows inserted above the rows the app measured on opening: the measured rows are still not written as `customHeight` rows
/// (they keep their place in `fitted_rows`), and one the person resizes afterwards is.
#[test]
fn measured_rows_below_an_inserted_row_are_not_saved_as_custom_heights() {
    use ibis_doc_model::sheet_command::Axis;
    let (_, opened) = open("xlsx-sales.xlsx");
    let custom_heights = |bytes: &[u8]| {
        let package = ibis_ooxml::Package::open(bytes.to_vec()).unwrap();
        let names: Vec<String> = package.names().filter(|n| n.starts_with("xl/worksheets/sheet")).map(str::to_string).collect();
        names.iter().map(|n| String::from_utf8_lossy(&package.read(n).unwrap()).matches("customHeight=\"1\"").count()).sum::<usize>()
    };
    let insert = |p: &Presentation| {
        let mut d = Document::new(p.clone());
        let s = slide(d.presentation(), "月次売上");
        d.execute(vec![Command::Sheet(SheetCommand::Insert { slide: s, axis: Axis::Rows, at: 1, count: 1 })]).unwrap();
        (d, s)
    };
    fn sheet_mut(p: &mut Presentation, s: SlideId) -> &mut ibis_doc_model::Sheet {
        let shape = p.slide_mut(s).unwrap().shapes.iter_mut().find(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Sheet(_))).unwrap();
        let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind else { unreachable!() };
        std::sync::Arc::make_mut(sheet)
    }
    // The control: the same insertion in the file as it was read.
    let (control, _) = insert(&opened.presentation);
    let expected = custom_heights(&opened.source.save(control.presentation(), SaveOptions::default()).unwrap());
    // The app measured the rows of the sheet (taller than the file says), then the row is inserted.
    let mut p = opened.presentation.clone();
    let s = slide(&p, "月次売上");
    let sheet = sheet_mut(&mut p, s);
    assert!(!sheet.measured_rows.is_empty(), "the sheet has rows that are measured");
    for r in sheet.measured_rows.0.clone() {
        if let Some(h) = sheet.rows.get_mut(r as usize) && *h > 0 {
            *h += 7 * ibis_doc_model::EMU_PER_POINT / 4;
            sheet.fitted_rows.insert(r, *h);
        }
    }
    let (mut d, s) = insert(&p);
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(custom_heights(&saved), expected, "a measured row below the inserted row was written as a custom height");
    // A measured row (now one lower) that the person resizes is written.
    let mut p = d.presentation().clone();
    let sheet = sheet_mut(&mut p, s);
    let r = *sheet.measured_rows.iter().find(|&&r| sheet.rows.get(r as usize).is_some_and(|&h| h > 0)).unwrap();
    sheet.rows[r as usize] += ibis_doc_model::EMU_PER_POINT;
    let saved = opened.source.save(&p, SaveOptions::default()).unwrap();
    assert_eq!(custom_heights(&saved), expected + 1, "the resized row is written");
}
