// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 傍点 (`w:em`): read, saved back as it was, and written for formatting
//! that gives it.

use ibis_doc_model::{Block, EmphasisMark, FlowParagraph};

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)
}

fn with_paragraph(runs: &str) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(std::fs::read(path("editing/docx-report.docx")).unwrap()).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml", format!("{}<w:p>{runs}</w:p>{}", &xml[..at], &xml[at..]).into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

fn last(flow: &ibis_doc_model::FlowDocument) -> FlowParagraph {
    flow.body.iter().rev().find_map(|b| if let Block::Paragraph(p) = b { Some(p.clone()) } else { None }).unwrap()
}

#[test]
fn emphasis_marks_read_and_kept() {
    for (name, mark) in [("dot", EmphasisMark::Dot), ("comma", EmphasisMark::Comma), ("circle", EmphasisMark::Circle), ("underDot", EmphasisMark::UnderDot), ("none", EmphasisMark::None)] {
        let data = with_paragraph(&format!(r#"<w:r><w:rPr><w:em w:val="{name}"/></w:rPr><w:t>傍点</w:t></w:r>"#));
        let opened = ibis_formats::open(data.clone()).unwrap();
        let p = last(opened.presentation.flow.as_deref().unwrap());
        assert_eq!(p.runs[0].style.emphasis, mark, "{name}");
        assert_eq!(opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap(), data, "{name}: saved unchanged");
    }
}

#[test]
fn emphasis_written_when_set() {
    use ibis_doc_model::{Command, Document, FlowCommand, FlowPos, InsertFormat, RunFormat};
    let data = with_paragraph(r#"<w:r><w:t>前</w:t></w:r>"#);
    let opened = ibis_formats::open(data).unwrap();
    let p = last(opened.presentation.flow.as_deref().unwrap());
    let mut doc = Document::new(opened.presentation.clone());
    let format = InsertFormat { format: RunFormat { emphasis: Some(EmphasisMark::Comma), ..RunFormat::default() }, char_style: None };
    doc.execute(vec![Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p.id, 1), text: "点".into(), format: Some(format) })]).unwrap();
    let saved = opened.source.save(doc.presentation(), ibis_formats::SaveOptions::default()).unwrap();
    let reread = ibis_formats::read(&saved).unwrap();
    let p = last(reread.flow.as_deref().unwrap());
    assert!(p.runs.iter().any(|r| r.style.emphasis == EmphasisMark::Comma && matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains('点'))), "{:?}", p.runs);
}

#[test]
fn ruby_read() {
    let data = with_paragraph(r#"<w:r><w:ruby><w:rubyPr><w:rubyAlign w:val="distributeSpace"/><w:hps w:val="10"/><w:hpsRaise w:val="26"/><w:hpsBaseText w:val="28"/><w:lid w:val="ja-JP"/></w:rubyPr><w:rt><w:r><w:rPr><w:sz w:val="10"/></w:rPr><w:t>かんじ</w:t></w:r></w:rt><w:rubyBase><w:r><w:rPr><w:sz w:val="28"/></w:rPr><w:t>漢字</w:t></w:r></w:rubyBase></w:ruby></w:r>"#);
    let opened = ibis_formats::open(data.clone()).unwrap();
    let p = last(opened.presentation.flow.as_deref().unwrap());
    assert_eq!(p.runs.len(), 1, "{:?}", p.runs);
    assert_eq!(p.runs[0].kind, ibis_doc_model::FlowRunKind::Ruby { base: "漢字".into(), text: "かんじ".into(), size: 500, raise: 1300, align: ibis_doc_model::RubyAlign::DistributeSpace });
    assert_eq!(p.runs[0].style.size, 1400);
    assert_eq!(opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap(), data, "saved unchanged");
}

#[test]
fn combine_read_and_kept() {
    let data = with_paragraph(r#"<w:r><w:rPr><w:eastAsianLayout w:id="7" w:combine="1" w:combineBrackets="angle"/></w:rPr><w:t>組み文字</w:t></w:r>"#);
    let opened = ibis_formats::open(data.clone()).unwrap();
    let p = last(opened.presentation.flow.as_deref().unwrap());
    assert_eq!(p.runs[0].style.combine, Some(ibis_doc_model::Combine::Angle));
    assert_eq!(opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap(), data, "saved unchanged");
}

#[test]
fn tate_chu_yoko_read_and_kept() {
    let data = with_paragraph(r#"<w:r><w:rPr><w:eastAsianLayout w:id="1" w:vert="1"/></w:rPr><w:t>12</w:t></w:r>"#);
    let opened = ibis_formats::open(data.clone()).unwrap();
    let p = last(opened.presentation.flow.as_deref().unwrap());
    assert!(p.runs[0].style.tate_chu_yoko, "tate_chu_yoko read");
    assert_eq!(opened.source.save(&opened.presentation, ibis_formats::SaveOptions::default()).unwrap(), data, "saved unchanged");
}

#[test]
fn tate_chu_yoko_written_when_set() {
    use ibis_doc_model::{Command, Document, FlowCommand, FlowPos, InsertFormat, RunFormat};
    let data = with_paragraph(r#"<w:r><w:t>前</w:t></w:r>"#);
    let opened = ibis_formats::open(data).unwrap();
    let p = last(opened.presentation.flow.as_deref().unwrap());
    let mut doc = Document::new(opened.presentation.clone());
    let format = InsertFormat { format: RunFormat { tate_chu_yoko: Some(true), ..RunFormat::default() }, char_style: None };
    doc.execute(vec![Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p.id, 1), text: "12".into(), format: Some(format) })]).unwrap();
    let saved = opened.source.save(doc.presentation(), ibis_formats::SaveOptions::default()).unwrap();
    let reread = ibis_formats::read(&saved).unwrap();
    let p = last(reread.flow.as_deref().unwrap());
    assert!(p.runs.iter().any(|r| r.style.tate_chu_yoko && matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("12"))), "{:?}", p.runs);
}

