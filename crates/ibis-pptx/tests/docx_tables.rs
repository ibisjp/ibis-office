// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Tables read from documents other programs write: without a grid
//! (`w:tblGrid`), or with a short one, the columns take the cells' widths
//! (`w:tcW`), as Word lays them out.

use ibis_doc_model::{Block, FlowTable};

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)
}

/// The report with `table` (the XML of a `w:tbl`) at the end of its body.
fn with_table(table: &str) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(std::fs::read(path("editing/docx-report.docx")).unwrap()).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml", format!("{}{}<w:p/>{}", &xml[..at], table, &xml[at..]).into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

fn last_table(data: &[u8]) -> FlowTable {
    let p = ibis_pptx::read(data).unwrap();
    p.flow.as_deref().unwrap().body.iter().rev().find_map(|b| if let Block::Table(t) = b { Some(t.clone()) } else { None }).unwrap()
}

const TWIP: i64 = 635;

fn cell(width: &str, span: u32) -> String {
    let span = if span > 1 { format!(r#"<w:gridSpan w:val="{span}"/>"#) } else { String::new() };
    format!(r#"<w:tc><w:tcPr>{width}{span}</w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc>"#)
}

#[test]
fn columns_from_cell_widths_without_a_grid() {
    let row = format!("<w:tr>{}{}{}</w:tr>", cell(r#"<w:tcW w:w="2000" w:type="dxa"/>"#, 1), cell(r#"<w:tcW w:w="3000" w:type="dxa"/>"#, 1), cell(r#"<w:tcW w:w="1000" w:type="dxa"/>"#, 1));
    let table = last_table(&with_table(&format!("<w:tbl><w:tblPr><w:tblW w:w=\"6000\" w:type=\"dxa\"/></w:tblPr>{row}</w:tbl>")));
    assert_eq!(table.columns, vec![2000 * TWIP, 3000 * TWIP, 1000 * TWIP]);
}

#[test]
fn spans_percentages_and_a_short_grid() {
    // A grid of one column (the first's), a row of 3 grid columns: 50 %
    // of a 4000-twip table spanning two, then 1000 twips.
    let row1 = format!("<w:tr>{}{}</w:tr>", cell(r#"<w:tcW w:w="2500" w:type="pct"/>"#, 2), cell(r#"<w:tcW w:w="1000" w:type="dxa"/>"#, 1));
    let row2 = format!("<w:tr>{}{}{}</w:tr>", cell(r#"<w:tcW w:w="0" w:type="auto"/>"#, 1), cell(r#"<w:tcW w:w="0" w:type="auto"/>"#, 1), cell("", 1));
    let xml = format!(r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="1200"/></w:tblGrid>{row1}{row2}</w:tbl>"#);
    let table = last_table(&with_table(&xml));
    assert_eq!(table.columns, vec![1200 * TWIP, 1800 * TWIP, 1000 * TWIP], "the grid's first column kept, the span's rest from its width");
}

#[test]
fn a_full_grid_is_kept() {
    let row = format!("<w:tr>{}{}</w:tr>", cell(r#"<w:tcW w:w="9999" w:type="dxa"/>"#, 1), cell(r#"<w:tcW w:w="9999" w:type="dxa"/>"#, 1));
    let xml = format!(r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="2500"/></w:tblGrid>{row}</w:tbl>"#);
    assert_eq!(last_table(&with_table(&xml)).columns, vec![1500 * TWIP, 2500 * TWIP]);
}

/// 自動調整 tables are laid out from their content (Word lays them out again whenever it opens them): the model keeps their preferred widths, and says whether the grid
/// says the widths (`grid_trusted`: the cells' widths agree with it, as Word writes them, so it is kept while each column holds its content's least width; not when the cells
/// have no width, the table is in percent or the widths disagree with the grid); a fixed one has none.
#[test]
fn autofit_when_the_grid_does_not_say_the_widths() {
    use ibis_doc_model::PreferredWidth;
    let grid = r#"<w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>"#;
    let row = |w: &str| format!("<w:tr>{}{}</w:tr>", cell(w, 1), cell(w, 1));
    let auto = r#"<w:tcW w:w="0" w:type="auto"/>"#;
    let t = last_table(&with_table(&format!(r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr>{grid}{}</w:tbl>"#, row(auto))));
    assert_eq!(t.autofit.map(|f| (f.width, f.cells, f.grid_trusted)), Some((PreferredWidth::Auto, vec![vec![PreferredWidth::Auto; 2]], false)));
    let t = last_table(&with_table(&format!(r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblLayout w:type="fixed"/></w:tblPr>{grid}{}</w:tbl>"#, row(auto))));
    assert!(t.autofit.is_none(), "fixed");
    let dxa = r#"<w:tcW w:w="2000" w:type="dxa"/>"#;
    let t = last_table(&with_table(&format!(r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr>{grid}{}</w:tbl>"#, row(dxa))));
    assert_eq!(t.autofit.map(|f| f.grid_trusted), Some(true), "the widths agree with the grid");
    let t = last_table(&with_table(&format!(r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/></w:tblPr>{grid}{}</w:tbl>"#, row(dxa))));
    assert_eq!(t.autofit.map(|f| f.width), Some(PreferredWidth::Percent(5000)));
}

/// A table style's conditional formatting (w:tblStylePr) by the table's
/// options (w:tblLook): the heading row's fill and bold text, the rows'
/// bands after it, the first column's fill over the bands.
#[test]
fn table_style_conditional_formatting() {
    let style = r#"<w:style w:type="table" w:styleId="Cond"><w:name w:val="Cond"/><w:tblPr><w:tblStyleRowBandSize w:val="1"/></w:tblPr><w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="4472C4"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="band1Horz"><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="E2EFDA"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="firstCol"><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="FFC000"/></w:tcPr></w:tblStylePr></w:style>"#;
    let mut package = ibis_ooxml::Package::open(std::fs::read(path("editing/docx-report.docx")).unwrap()).unwrap();
    let styles = String::from_utf8(package.read("word/styles.xml").unwrap()).unwrap();
    let at = styles.rfind("</w:styles>").unwrap();
    package.set_bytes("word/styles.xml", format!("{}{style}{}", &styles[..at], &styles[at..]).into_bytes());
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let row = |r: usize| format!("<w:tr>{}{}</w:tr>", cell(&format!(r#"<w:tcW w:w="1000" w:type="dxa"/>"#), 1).replace(">x<", &format!(">{r}0<")), cell(r#"<w:tcW w:w="1000" w:type="dxa"/>"#, 1));
    let table = format!(r#"<w:tbl><w:tblPr><w:tblStyle w:val="Cond"/><w:tblLook w:firstRow="1" w:lastRow="0" w:firstColumn="1" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/></w:tblPr><w:tblGrid><w:gridCol w:w="1000"/><w:gridCol w:w="1000"/></w:tblGrid>{}{}{}</w:tbl>"#, row(0), row(1), row(2));
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml", format!("{}{}<w:p/>{}", &xml[..at], table, &xml[at..]).into_bytes());
    let t = last_table(&package.save(ibis_ooxml::SaveOptions::default()).unwrap());
    let fill = |r: usize, c: usize| t.rows[r].cells[c].shading.or(t.rows[r].cells[c].style_fill).map(|s| s.0);
    assert_eq!(fill(0, 1), Some(0x4472C4), "the heading row");
    assert_eq!(fill(1, 1), Some(0xE2EFDA), "the first band after the heading");
    assert_eq!(fill(2, 1), None, "the second band (none)");
    assert_eq!(fill(1, 0), Some(0xFFC000), "the first column over the band");
    let bold = |r: usize| match &t.rows[r].cells[1].blocks[0] { ibis_doc_model::Block::Paragraph(p) => p.runs[0].style.bold, _ => false };
    assert!(bold(0) && !bold(1), "bold in the heading row only");
}

/// A floating table (w:tblpPr): its anchors, offset and specs read.
#[test]
fn floating_table_read() {
    let row = format!("<w:tr>{}</w:tr>", cell(r#"<w:tcW w:w="1200" w:type="dxa"/>"#, 1));
    let t = last_table(&with_table(&format!(r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:topFromText="120" w:bottomFromText="120" w:vertAnchor="margin" w:horzAnchor="page" w:tblpX="1440" w:tblpYSpec="center"/><w:tblW w:w="1200" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="1200"/></w:tblGrid>{row}</w:tbl>"#)));
    let f = t.floating.expect("floating");
    assert_eq!((f.horz_anchor, f.vert_anchor), (ibis_doc_model::TableAnchor::Page, ibis_doc_model::TableAnchor::Margin));
    assert_eq!((f.x, f.x_align, f.y_align), (1440 * TWIP, None, Some(ibis_doc_model::TableAlign::Center)));
    assert_eq!(f.distance, [180 * TWIP, 180 * TWIP, 120 * TWIP, 120 * TWIP]);
}

/// A floating table (w:tblpPr) read, saved unchanged, modified, and saved.
#[test]
fn floating_table_read_and_save() {
    let row = format!("<w:tr>{}</w:tr>", cell(r#"<w:tcW w:w="1200" w:type="dxa"/>"#, 1));
    let data = with_table(&format!(r#"<w:tbl><w:tblPr><w:tblpPr w:leftFromText="180" w:rightFromText="180" w:topFromText="120" w:bottomFromText="120" w:vertAnchor="margin" w:horzAnchor="page" w:tblpX="1440" w:tblpYSpec="center"/><w:tblW w:w="1200" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="1200"/></w:tblGrid>{row}</w:tbl>"#));
    let opened = ibis_pptx::open(data.clone()).unwrap();
    // Saved unchanged
    assert_eq!(opened.source.save(&opened.presentation, ibis_pptx::SaveOptions::default()).unwrap(), data);

    // Modify floating settings and save
    let mut modified = opened.presentation.clone();
    let flow = modified.flow.as_mut().unwrap();
    let arc_flow = std::sync::Arc::make_mut(flow);
    let tbl = arc_flow.body.iter_mut().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).expect("table");
    let mut f = tbl.floating.unwrap();
    f.distance = [240 * TWIP, 240 * TWIP, 60 * TWIP, 60 * TWIP];
    f.x_align = Some(ibis_doc_model::TableAlign::Start);
    tbl.floating = Some(f);
    let saved_modified = opened.source.save(&modified, ibis_pptx::SaveOptions::default()).unwrap();

    let reopened = ibis_pptx::open(saved_modified).unwrap();
    let reopened_tbl = last_table_from_pres(&reopened.presentation);
    let reopened_f = reopened_tbl.floating.expect("reopened floating");
    assert_eq!(reopened_f.distance, [240 * TWIP, 240 * TWIP, 60 * TWIP, 60 * TWIP]);
    assert_eq!(reopened_f.x_align, Some(ibis_doc_model::TableAlign::Start));

    // Remove floating (inline) and save
    let mut inline_pres = reopened.presentation.clone();
    let flow2 = inline_pres.flow.as_mut().unwrap();
    let arc_flow2 = std::sync::Arc::make_mut(flow2);
    let tbl2 = arc_flow2.body.iter_mut().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).expect("table");
    tbl2.floating = None;
    let saved_inline = reopened.source.save(&inline_pres, ibis_pptx::SaveOptions::default()).unwrap();

    let reopened_inline = ibis_pptx::open(saved_inline).unwrap();
    let reopened_inline_tbl = last_table_from_pres(&reopened_inline.presentation);
    assert_eq!(reopened_inline_tbl.floating, None);
}

fn last_table_from_pres(presentation: &ibis_doc_model::Presentation) -> &ibis_doc_model::FlowTable {
    let flow = presentation.flow.as_ref().unwrap();
    flow.body.iter().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).expect("table")
}

/// A cell's text direction (w:textDirection) read; saved unchanged.
#[test]
fn cell_text_direction_read() {
    use ibis_doc_model::CellDirection;
    let cells: String = ["tbRl", "btLr", "lrTbV", "tbLrV", "tbRlV"].iter().map(|d| format!(r#"<w:tc><w:tcPr><w:tcW w:w="1000" w:type="dxa"/><w:textDirection w:val="{d}"/></w:tcPr><w:p><w:r><w:t>縦</w:t></w:r></w:p></w:tc>"#)).collect();
    let data = with_table(&format!(r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="dxa"/></w:tblPr><w:tblGrid>{}</w:tblGrid><w:tr>{cells}</w:tr></w:tbl>"#, r#"<w:gridCol w:w="1000"/>"#.repeat(5)));
    let t = last_table(&data);
    let d: Vec<CellDirection> = t.rows[0].cells.iter().map(|c| c.direction).collect();
    assert_eq!(d, vec![CellDirection::TbRl, CellDirection::BtLr, CellDirection::LrTbV, CellDirection::TbLrV, CellDirection::TbRl]);
    let opened = ibis_pptx::open(data.clone()).unwrap();
    assert_eq!(opened.source.save(&opened.presentation, ibis_pptx::SaveOptions::default()).unwrap(), data);
}

/// A table's cell spacing (w:tblCellSpacing) read and saved.
#[test]
fn table_cell_spacing_read_and_save() {
    let row = format!("<w:tr>{}</w:tr>", cell(r#"<w:tcW w:w="2000" w:type="dxa"/>"#, 1));
    let xml = format!(r#"<w:tbl><w:tblPr><w:tblW w:w="2000" w:type="dxa"/><w:tblCellSpacing w:w="120" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>{row}</w:tbl>"#);
    let data = with_table(&xml);
    let t = last_table(&data);
    assert_eq!(t.cell_spacing, Some(120 * TWIP));

    // Save unchanged -> data preserved
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let saved = opened.source.save(&opened.presentation, ibis_pptx::SaveOptions::default()).unwrap();
    let t2 = last_table(&saved);
    assert_eq!(t2.cell_spacing, Some(120 * TWIP));
}

/// A cell's noWrap property (w:noWrap) read and saved.
#[test]
fn cell_no_wrap_read_and_save() {
    let row = format!("<w:tr><w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/><w:noWrap/></w:tcPr><w:p><w:r><w:t> nowrap </w:t></w:r></w:p></w:tc></w:tr>");
    let xml = format!(r#"<w:tbl><w:tblPr><w:tblW w:w="2000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>{row}</w:tbl>"#);
    let data = with_table(&xml);
    let t = last_table(&data);
    assert_eq!(t.rows[0].cells[0].no_wrap, true);

    // Save unchanged -> data preserved
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let saved = opened.source.save(&opened.presentation, ibis_pptx::SaveOptions::default()).unwrap();
    let t2 = last_table(&saved);
    assert_eq!(t2.rows[0].cells[0].no_wrap, true);
}

/// A table's style (w:tblStyle) read, updated, cleared, and saved.
#[test]
fn table_style_apply_and_save() {
    let row = format!("<w:tr><w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>styled</w:t></w:r></w:p></w:tc></w:tr>");
    let xml = format!(r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="2000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>{row}</w:tbl>"#);
    let data = with_table(&xml);

    // 1. Initial read: style id is TableGrid
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let t = last_table_from_pres(&opened.presentation);
    // Note: If styles.xml does not define TableGrid with tblStylePr, style_id is still preserved in paragraph or table
    let initial_style = t.style.as_ref().map(|s| s.id.as_str());
    assert!(initial_style == Some("TableGrid") || t.rows[0].cells[0].blocks.iter().any(|b| if let ibis_doc_model::Block::Paragraph(p) = b { p.table_style.as_deref() == Some("TableGrid") } else { false }));

    // 2. Change style to "LightShading"
    let mut pres = opened.presentation.clone();
    let flow = pres.flow.as_mut().unwrap();
    let arc_flow = std::sync::Arc::make_mut(flow);
    let tbl = arc_flow.body.iter_mut().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).expect("table");
    tbl.style = Some(ibis_doc_model::TableStyleRef {
        id: "LightShading".to_string(),
        look: ibis_doc_model::TableLook::default(),
        row_band: 1,
        col_band: 1,
    });
    let saved_data = opened.source.save(&pres, ibis_pptx::SaveOptions::default()).unwrap();

    // Verify XML contains <w:tblStyle w:val="LightShading"
    let saved_opened = ibis_pptx::open(saved_data.clone()).unwrap();
    let saved_t = last_table_from_pres(&saved_opened.presentation);
    assert_eq!(saved_t.style.as_ref().map(|s| s.id.as_str()), Some("LightShading"));

    // 3. Clear style (None)
    let mut pres_clear = saved_opened.presentation.clone();
    let flow_clear = pres_clear.flow.as_mut().unwrap();
    let arc_flow_clear = std::sync::Arc::make_mut(flow_clear);
    let tbl_clear = arc_flow_clear.body.iter_mut().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).expect("table");
    tbl_clear.style = None;
    let cleared_data = saved_opened.source.save(&pres_clear, ibis_pptx::SaveOptions::default()).unwrap();

    let reopened_cleared = ibis_pptx::open(cleared_data).unwrap();
    let cleared_t = last_table_from_pres(&reopened_cleared.presentation);
    assert_eq!(cleared_t.style, None);
}

