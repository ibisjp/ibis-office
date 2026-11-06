// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the user changes is saved, and what the user leaves alone is saved as it was (A1 to A4 of the 0.8.5 review): a document's
//! variables, a presentation's photo album, the inside of a moved text box, the kinds of tab stops and their leaders.

use ibis_doc_model::{Block, Command, Document, FlowCommand, FlowDocument, FlowPos, FlowRunKind, ParaChange, PhotoAlbum, TabAlign, TabChange, TabLeader, TabStop};
use ibis_ooxml::{Package, SaveOptions as PackageOptions};
use ibis_pptx::SaveOptions;

fn base_docx() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/docx-paste-source.docx")).unwrap()
}

/// `base_docx` with `paragraph` (WordprocessingML) at the start of the body.
fn docx_with_paragraph(paragraph: &str) -> Vec<u8> {
    let mut package = Package::open(base_docx()).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.find("<w:body>").expect("a body") + "<w:body>".len();
    xml.insert_str(at, paragraph);
    package.set_bytes("word/document.xml", xml.into_bytes());
    package.save(PackageOptions::default()).unwrap()
}

fn part(bytes: &[u8], name: &str) -> String {
    String::from_utf8(Package::open(bytes.to_vec()).unwrap().read(name).unwrap().to_vec()).unwrap()
}

fn flow(p: &ibis_doc_model::Presentation) -> &FlowDocument {
    p.flow.as_deref().unwrap()
}

#[test]
fn document_variables_alone_are_saved() {
    let data = base_docx();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let mut vars = flow(d.presentation()).doc_variables.clone();
    vars.insert("資料文献".into(), "1件目".into());
    d.execute(vec![Command::Flow(FlowCommand::SetDocVariables(vars.clone()))]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    assert!(saved != data, "a change of the document variables alone is written");
    let again = ibis_pptx::read(&saved).unwrap();
    assert_eq!(flow(&again).doc_variables, vars);
}

#[test]
fn photo_album_alone_is_saved() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let Some(file) = std::fs::read_dir(&path).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "pptx")) else { return };
    let data = std::fs::read(file).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let album = PhotoAlbum { bw: true, show_captions: true, ..PhotoAlbum::default() };
    d.execute(vec![Command::SetPhotoAlbum { album: Some(album.clone()) }]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    assert!(saved != data, "a change of the photo album alone is written");
    let again = ibis_pptx::read(&saved).unwrap();
    assert_eq!(again.photo_album, Some(album));
}

const RICH_BOX: &str = concat!(
    r#"<w:txbxContent><w:p><w:r><w:t>通常の文字と</w:t></w:r><w:r><w:rPr><w:b/><w:color w:val="C00000"/></w:rPr><w:t>赤い太字</w:t></w:r>"#,
    r#"<w:r><w:t>が混ざった1行目</w:t></w:r><w:r><w:br/></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>改行のあとの斜体の2行目</w:t></w:r></w:p></w:txbxContent>"#
);

/// A document whose text box holds `RICH_BOX`.
fn docx_with_rich_box() -> Vec<u8> {
    // A text box made by the editor, its content then replaced by one with a bold red part and a line break inside a paragraph.
    let data = base_docx();
    let opened = ibis_pptx::open(data).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let para = flow(d.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap();
    d.execute(vec![Command::Flow(FlowCommand::InsertTextBox { at: FlowPos::new(para, 0), size: ibis_doc_model::EmuSize::new(1_828_800, 914_400), text: "x".into() })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let mut package = Package::open(saved).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap().to_vec()).unwrap();
    let (a, b) = (xml.find("<w:txbxContent>").expect("the box"), xml.find("</w:txbxContent>").unwrap() + "</w:txbxContent>".len());
    package.set_bytes("word/document.xml", format!("{}{}{}", &xml[..a], RICH_BOX, &xml[b..]).into_bytes());
    let rich = package.save(PackageOptions::default()).unwrap();
    rich
}

#[test]
fn moving_a_text_box_keeps_what_is_inside() {
    let rich = docx_with_rich_box();
    let opened = ibis_pptx::open(rich.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let at = flow(d.presentation())
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) => {
                let mut off = 0;
                p.runs.iter().find_map(|r| {
                    let here = off;
                    off += r.len();
                    matches!(r.kind, FlowRunKind::Shape(_)).then_some(FlowPos::new(p.id, here))
                })
            }
            _ => None,
        })
        .expect("the text box");
    d.execute(vec![Command::Flow(FlowCommand::SetShapeGeometry { at, size: None, offset: Some((360_000, 360_000)) })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let content = |bytes: &[u8]| {
        let xml = part(bytes, "word/document.xml");
        let a = xml.find("<w:txbxContent>").unwrap();
        xml[a..xml.find("</w:txbxContent>").unwrap()].to_string()
    };
    assert_eq!(content(&saved), content(&rich), "moving the box leaves its text alone");
}


fn box_position(p: &ibis_doc_model::Presentation) -> FlowPos {
    flow(p)
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) => {
                let mut off = 0;
                p.runs.iter().find_map(|r| {
                    let here = off;
                    off += r.len();
                    matches!(r.kind, FlowRunKind::Shape(_)).then_some(FlowPos::new(p.id, here))
                })
            }
            _ => None,
        })
        .expect("the text box")
}

#[test]
fn editing_the_words_of_a_text_box_keeps_its_bold_colour_and_line_break() {
    let rich = docx_with_rich_box();
    let opened = ibis_pptx::open(rich).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let at = box_position(d.presentation());
    // Words added in the middle of the red bold part, and at the end of the second line.
    d.execute(vec![Command::Flow(FlowCommand::SpliceShapeText { at, start: 8, remove: 0, insert: "【追加】".into() })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let xml = part(&saved, "word/document.xml");
    let a = xml.find("<w:txbxContent>").unwrap();
    let content = &xml[a..xml.find("</w:txbxContent>").unwrap()];
    assert!(content.contains("追加"), "{content}");
    assert!(content.contains("<w:b/>") && content.contains("C00000"), "the bold red part stays bold and red: {content}");
    assert!(content.contains("<w:br/>"), "the line break stays: {content}");
    assert!(content.contains("<w:i/>"), "the italic second line stays italic: {content}");
    assert!(content.contains("通常の文字と"), "the plain start stays: {content}");
    let again = ibis_pptx::open(saved).unwrap();
    let text = |p: &ibis_doc_model::Presentation| {
        let at = box_position(p);
        let _ = at;
        match &flow(p).body.iter().find_map(|b| if let Block::Paragraph(p) = b { p.runs.iter().find_map(|r| if let FlowRunKind::Shape(s) = &r.kind { Some(s.clone()) } else { None }) } else { None }).unwrap().kind {
            ibis_doc_model::ShapeKind::TextBox(t) => t.text(),
            ibis_doc_model::ShapeKind::Auto(a) => a.text.as_ref().unwrap().text(),
            _ => panic!("a box"),
        }
    };
    assert_eq!(text(&again.presentation), text(d.presentation()), "reading the saved file gives the edited text");
}

#[test]
fn tab_kinds_and_leaders_are_kept_when_a_tab_is_added() {
    let paragraph = r#"<w:p><w:pPr><w:tabs><w:tab w:val="bar" w:pos="1701"/><w:tab w:val="right" w:leader="hyphen" w:pos="5103"/><w:tab w:val="left" w:pos="6804"/></w:tabs></w:pPr><w:r><w:t>価格</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>1,000円</w:t></w:r></w:p>"#;
    let opened = ibis_pptx::open(docx_with_paragraph(paragraph)).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let (id, existing) = flow(d.presentation()).body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some((p.id, p.direct.tabs.clone())) } else { None }).unwrap();
    assert_eq!(existing.len(), 3);
    assert_eq!(existing[0].stop.map(|s| s.align), Some(TabAlign::Bar), "a bar tab is read as one");
    assert_eq!(existing[1].stop.map(|s| s.leader), Some(TabLeader::Hyphen));
    let mut changes = existing;
    changes.push(TabChange { position: 3401 * 635, stop: Some(TabStop { position: 3401 * 635, align: TabAlign::Left, leader: TabLeader::None }) });
    d.execute(vec![Command::Flow(FlowCommand::SetParagraphFormat { first: id, last: id, change: ParaChange::Tabs(changes) })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let xml = part(&saved, "word/document.xml");
    let a = xml.find("<w:tabs>").unwrap();
    let tabs = &xml[a..xml.find("</w:tabs>").unwrap()];
    assert!(tabs.contains(r#"w:val="bar" w:pos="1701""#), "{tabs}");
    assert!(tabs.contains(r#"w:val="right" w:leader="hyphen" w:pos="5103""#), "{tabs}");
    assert!(tabs.contains(r#"w:val="left" w:pos="3401""#), "{tabs}");
}
