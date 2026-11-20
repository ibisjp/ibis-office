// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Columns (段組み, `w:cols`) and column breaks read from the file, and
//! saved back as they were.

use ibis_doc_model::{Block, FlowRunKind, PageColumns, SectionBreak};

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)
}

/// The report with `body` (paragraphs) before its last section, whose
/// `w:cols` becomes `cols`.
fn with(body: &str, cols: &str) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(std::fs::read(path("editing/docx-report.docx")).unwrap()).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.rfind("<w:sectPr").unwrap();
    xml.insert_str(at, body);
    let sect = xml.rfind("<w:sectPr").unwrap();
    let tail = xml[sect..].to_string();
    let tail = match (tail.find("<w:cols"), tail.find("</w:sectPr>")) {
        (Some(c), _) => {
            let close = c + tail[c..].find('>').unwrap();
            let end = if tail[..close].ends_with('/') { close + 1 } else { tail.find("</w:cols>").unwrap() + "</w:cols>".len() };
            format!("{}{}{}", &tail[..c], cols, &tail[end..])
        }
        (None, Some(e)) => format!("{}{}{}", &tail[..e], cols, &tail[e..]),
        _ => unreachable!(),
    };
    xml.replace_range(sect.., &tail);
    package.set_bytes("word/document.xml", xml.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

#[test]
fn columns_and_column_breaks() {
    let data = with(r#"<w:p><w:r><w:t>左</w:t></w:r><w:r><w:br w:type="column"/></w:r><w:r><w:t>右</w:t></w:r></w:p>"#, r#"<w:cols w:num="2" w:space="720" w:sep="1"/>"#);
    let opened = ibis_formats::open(data.clone()).unwrap();
    let flow = opened.presentation.flow.as_deref().unwrap();
    let c = flow.page.columns;
    assert_eq!((c.count, c.space, c.separator, c.widths), (2, 720 * 635, true, None));
    // Two equal columns with 36 pt between.
    let width = flow.page.size.width - flow.page.left - flow.page.right;
    let cols = c.layout(width);
    assert_eq!(cols[1].0, cols[0].1 + 720 * 635);
    let p = flow.body.iter().rev().find_map(|b| if let Block::Paragraph(p) = b && p.text().contains('左') { Some(p) } else { None }).unwrap();
    assert!(p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::ColumnBreak)));
    // Saved unchanged: the same file.
    assert_eq!(opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap(), data);
}

#[test]
fn columns_of_their_own_widths() {
    let data = with("", r#"<w:cols w:num="2" w:equalWidth="0" w:space="425"><w:col w:w="3000" w:space="425"/><w:col w:w="6000"/></w:cols>"#);
    let flow = ibis_formats::read(&data).unwrap().flow.unwrap();
    let c = flow.page.columns;
    assert_eq!(c.count, 2);
    assert_eq!(c.layout(0), vec![(0, 3000 * 635), ((3000 + 425) * 635, 6000 * 635)]);
    assert_eq!(PageColumns::default().layout(1000), vec![(0, 1000)]);
}

#[test]
fn next_column_section_break() {
    // The break after a section is how the next one starts (its w:type).
    let data = with(r#"<w:p><w:pPr><w:sectPr><w:type w:val="continuous"/><w:cols w:num="2"/></w:sectPr></w:pPr><w:r><w:t>段</w:t></w:r></w:p>"#, r#"<w:type w:val="nextColumn"/><w:cols w:num="2"/>"#);
    let flow = ibis_formats::read(&data).unwrap().flow.unwrap();
    assert!(flow.body.iter().any(|b| matches!(b, Block::Paragraph(p) if p.section_break == SectionBreak::NextColumn)));
}


/// A chart in a document (`c:chart` in a drawing, the chart part related
/// from the document): read as a chart shape in its extent; saved
/// unchanged, the same file.
#[test]
fn charts_in_documents() {
    let source = ibis_ooxml::Package::open(std::fs::read(path("editing/pptx-chart-inserted-by-powerpoint.pptx")).unwrap()).unwrap();
    let chart_name = source.names().find(|n| n.starts_with("ppt/charts/chart") && n.ends_with(".xml")).unwrap().to_string();
    let chart = source.read(&chart_name).unwrap();
    let mut package = ibis_ooxml::Package::open(std::fs::read(path("editing/docx-report.docx")).unwrap()).unwrap();
    package.set_bytes("word/charts/chart1.xml", chart);
    let rels = String::from_utf8(package.read("word/_rels/document.xml.rels").unwrap()).unwrap();
    let at = rels.rfind("</Relationships>").unwrap();
    package.set_bytes("word/_rels/document.xml.rels", format!(r#"{}<Relationship Id="rIdChart1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="charts/chart1.xml"/>{}"#, &rels[..at], &rels[at..]).into_bytes());
    let types = String::from_utf8(package.read("[Content_Types].xml").unwrap()).unwrap();
    let at = types.rfind("</Types>").unwrap();
    package.set_bytes("[Content_Types].xml", format!(r#"{}<Override PartName="/word/charts/chart1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/>{}"#, &types[..at], &types[at..]).into_bytes());
    let run = r#"<w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><wp:extent cx="3200400" cy="1828800"/><wp:docPr id="77" name="グラフ 1"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rIdChart1"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#;
    let data = with_package(package, &format!("<w:p>{run}</w:p>"));
    let opened = ibis_formats::open(data.clone()).unwrap();
    let flow = opened.presentation.flow.as_deref().unwrap();
    let shape = flow.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).flat_map(|p| &p.runs).find_map(|r| if let FlowRunKind::Shape(s) = &r.kind { Some(s.clone()) } else { None }).expect("a chart");
    assert!(matches!(shape.kind, ibis_doc_model::ShapeKind::Chart(_)));
    assert_eq!((shape.frame.width, shape.frame.height), (3200400, 1828800));
    assert_eq!(opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap(), data);
}

fn with_package(mut package: ibis_ooxml::Package, body: &str) -> Vec<u8> {
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml", format!("{}{body}{}", &xml[..at], &xml[at..]).into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// 行番号 (w:lnNumType) and w:suppressLineNumbers read.
#[test]
fn line_numbers_read() {
    let data = with(r#"<w:p><w:pPr><w:suppressLineNumbers/></w:pPr><w:r><w:t>番号なし</w:t></w:r></w:p>"#, r#"<w:lnNumType w:countBy="5" w:start="9" w:distance="720" w:restart="newSection"/><w:cols w:space="425"/>"#);
    let flow = ibis_formats::read(&data).unwrap().flow.unwrap();
    let n = flow.page.line_numbers.expect("line numbers");
    assert_eq!((n.count_by, n.start, n.distance, n.restart), (5, 10, 720 * 635, ibis_doc_model::LineNumberRestart::NewSection));
    assert!(flow.body.iter().any(|b| matches!(b, Block::Paragraph(p) if p.suppress_line_numbers && p.text() == "番号なし")));
}

/// ページの色 (w:background) read when the settings show it
/// (displayBackgroundShape), and saved unchanged.
#[test]
fn page_color_read() {
    let mut package = ibis_ooxml::Package::open(std::fs::read(path("editing/docx-report.docx")).unwrap()).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.find("<w:body").unwrap();
    package.set_bytes("word/document.xml", format!(r#"{}<w:background w:color="FFF2CC"/>{}"#, &xml[..at], &xml[at..]).into_bytes());
    let plain = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    // Not shown without displayBackgroundShape.
    assert_eq!(ibis_formats::read(&plain).unwrap().flow.unwrap().page_color, None);
    let mut package = ibis_ooxml::Package::open(plain).unwrap();
    let settings = String::from_utf8(package.read("word/settings.xml").unwrap()).unwrap();
    let at = settings.find('>').map(|_| settings.find("<w:zoom").unwrap_or(settings.rfind("</w:settings>").unwrap())).unwrap();
    package.set_bytes("word/settings.xml", format!("{}<w:displayBackgroundShape/>{}", &settings[..at], &settings[at..]).into_bytes());
    let data = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    let opened = ibis_formats::open(data.clone()).unwrap();
    assert_eq!(opened.presentation.flow.as_deref().unwrap().page_color, Some(ibis_doc_model::Rgb(0xFFF2CC)));
    assert_eq!(opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap(), data);
}

#[test]
fn page_color_change_save_reopen() {
    let plain_bytes = std::fs::read(path("editing/docx-report.docx")).unwrap();
    let opened = ibis_formats::open(plain_bytes).unwrap();
    assert_eq!(opened.presentation.flow.as_deref().unwrap().page_color, None);

    // Set page color to light blue (#DDEBF7) via presentation flow
    let blue = ibis_doc_model::Rgb(0xDDEBF7);
    let mut p = opened.presentation.clone();
    let flow = p.flow.as_mut().unwrap();
    std::sync::Arc::make_mut(flow).page_color = Some(blue);
    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();

    // Reopen and verify page_color is Some(blue)
    let reloaded = ibis_formats::open(saved.clone()).unwrap();
    assert_eq!(reloaded.presentation.flow.as_deref().unwrap().page_color, Some(blue));

    // Now clear page color to None
    let mut p2 = reloaded.presentation.clone();
    let flow2 = p2.flow.as_mut().unwrap();
    std::sync::Arc::make_mut(flow2).page_color = None;
    let saved_none = reloaded.source.save(&p2, ibis_formats::SaveOptions::default()).unwrap();

    // Reopen and verify page_color is None
    let reloaded_none = ibis_formats::open(saved_none).unwrap();
    assert_eq!(reloaded_none.presentation.flow.as_deref().unwrap().page_color, None);
}

/// 縦書き (w:textDirection in the section) read.
#[test]
fn section_text_direction_read() {
    let data = with("", r#"<w:textDirection w:val="tbRl"/><w:cols w:space="425"/>"#);
    let flow = ibis_formats::read(&data).unwrap().flow.unwrap();
    assert_eq!(flow.page.direction, ibis_doc_model::CellDirection::TbRl);
}

#[test]
fn page_number_start_save_reopen() {
    let plain_bytes = std::fs::read(path("editing/docx-report.docx")).unwrap();
    let opened = ibis_formats::open(plain_bytes).unwrap();
    assert_eq!(opened.presentation.flow.as_deref().unwrap().page.first_page, 1);
    assert!(!opened.presentation.flow.as_deref().unwrap().restart_numbering);

    // Set starting page number to 5
    let mut p = opened.presentation.clone();
    let flow = p.flow.as_mut().unwrap();
    let m = std::sync::Arc::make_mut(flow);
    m.restart_numbering = true;
    m.page.first_page = 5;
    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();

    // Reopen and verify first_page is 5 and restart_numbering is true
    let reloaded = ibis_formats::open(saved.clone()).unwrap();
    let flow = reloaded.presentation.flow.as_deref().unwrap();
    assert_eq!(flow.page.first_page, 5);
    assert!(flow.restart_numbering);

    // Now clear restart_numbering
    let mut p2 = reloaded.presentation.clone();
    let flow2 = p2.flow.as_mut().unwrap();
    let m2 = std::sync::Arc::make_mut(flow2);
    m2.restart_numbering = false;
    m2.page.first_page = 1;
    let saved_reset = reloaded.source.save(&p2, ibis_formats::SaveOptions::default()).unwrap();

    let reloaded_reset = ibis_formats::open(saved_reset).unwrap();
    let flow_reset = reloaded_reset.presentation.flow.as_deref().unwrap();
    assert_eq!(flow_reset.page.first_page, 1);
    assert!(!flow_reset.restart_numbering);
}

#[test]
fn columns_change_save_reopen() {
    let plain_bytes = std::fs::read(path("editing/docx-report.docx")).unwrap();
    let opened = ibis_formats::open(plain_bytes).unwrap();
    let initial_cols = opened.presentation.flow.as_deref().unwrap().page.columns;
    assert_eq!(initial_cols.count, 1);

    // 1. Change to 2 columns with separator
    let mut p = opened.presentation.clone();
    let flow = p.flow.as_mut().unwrap();
    let m = std::sync::Arc::make_mut(flow);
    m.page.columns.count = 2;
    m.page.columns.separator = true;
    m.page.columns.space = 720 * 635;
    let saved_2cols = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();

    let reloaded_2cols = ibis_formats::open(saved_2cols.clone()).unwrap();
    let cols_2 = reloaded_2cols.presentation.flow.as_deref().unwrap().page.columns;
    assert_eq!(cols_2.count, 2);
    assert!(cols_2.separator);
    assert_eq!(cols_2.space, 720 * 635);

    // 2. Change to 3 columns without separator
    let mut p3 = reloaded_2cols.presentation.clone();
    let flow3 = p3.flow.as_mut().unwrap();
    let m3 = std::sync::Arc::make_mut(flow3);
    m3.page.columns.count = 3;
    m3.page.columns.separator = false;
    let saved_3cols = reloaded_2cols.source.save(&p3, ibis_formats::SaveOptions::default()).unwrap();

    let reloaded_3cols = ibis_formats::open(saved_3cols.clone()).unwrap();
    let cols_3 = reloaded_3cols.presentation.flow.as_deref().unwrap().page.columns;
    assert_eq!(cols_3.count, 3);
    assert!(!cols_3.separator);

    // 3. Reset back to 1 column
    let mut p1 = reloaded_3cols.presentation.clone();
    let flow1 = p1.flow.as_mut().unwrap();
    let m1 = std::sync::Arc::make_mut(flow1);
    m1.page.columns.count = 1;
    m1.page.columns.separator = false;
    let saved_1col = reloaded_3cols.source.save(&p1, ibis_formats::SaveOptions::default()).unwrap();

    let reloaded_1col = ibis_formats::open(saved_1col).unwrap();
    let cols_1 = reloaded_1col.presentation.flow.as_deref().unwrap().page.columns;
    assert_eq!(cols_1.count, 1);
    assert!(!cols_1.separator);
}

