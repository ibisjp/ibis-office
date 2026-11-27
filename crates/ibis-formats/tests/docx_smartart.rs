// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #208: a document's SmartArt (図表) is read from the drawing Word keeps of it (`word/diagrams/drawing*.xml`), as a presentation's is: a frame and the shapes over it.

use ibis_doc_model::{Block, FlowRunKind, GroupMember, Placeholder, Shape, ShapeKind};

fn base() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/docx-report.docx")).unwrap()
}

fn shapes_of(doc: &ibis_doc_model::FlowDocument) -> Vec<Shape> {
    doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).flat_map(|p| p.runs.iter()).filter_map(|r| if let FlowRunKind::Shape(s) = &r.kind { Some((**s).clone()) } else { None }).collect()
}

#[test]
fn a_smartart_graphic_is_a_frame_with_the_shapes_of_its_drawing() {
    let opened = ibis_formats::open(ibis_formats::test_files::docx_with_smartart(&base())).unwrap();
    let doc = opened.presentation.flow.clone().expect("a document");
    let shapes = shapes_of(&doc);
    let frame = shapes.iter().find(|s| matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram))).expect("the graphic's frame");
    assert_eq!((frame.frame.width, frame.frame.height), (4_320_000, 2_160_000));
    assert!(frame.floating.is_none(), "an inline graphic");
    let group = doc.shape_groups.iter().find(|g| g.children.first() == Some(&GroupMember::Shape(frame.id))).expect("the frame and its shapes are a group");
    let members = ibis_formats::docx_convert::shapes_needing_pictures(&doc);
    assert_eq!(members.len(), 1, "the frame is the one shape that is drawn as a picture");
    let texts: Vec<String> = group
        .children
        .iter()
        .skip(1)
        .map(|m| {
            let GroupMember::Shape(id) = m else { panic!("a shape") };
            let shape = shapes.iter().find(|s| s.id == *id).expect("the shape is in the body");
            let at = shape.floating.as_ref().expect("floating over the frame");
            assert_eq!((at.offset_x, at.offset_y), (shape.frame.x, shape.frame.y));
            assert!(shape.frame.x >= frame.frame.x && shape.frame.right() <= frame.frame.right() && shape.frame.bottom() <= frame.frame.bottom(), "{:?} inside {:?}", shape.frame, frame.frame);
            shape.text_body().expect("a text").text()
        })
        .collect();
    assert_eq!(texts, vec!["計画", "設計", "実装"]);
}

#[test]
fn a_document_without_a_drawing_part_reads_as_before() {
    let opened = ibis_formats::open(base()).unwrap();
    let doc = opened.presentation.flow.clone().unwrap();
    assert!(shapes_of(&doc).iter().all(|s| !matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram))));
}

#[test]
fn the_pages_picture_of_a_smartart_graphic_has_its_texts_as_alt_and_is_made_from_its_shapes() {
    let opened = ibis_formats::open(ibis_formats::test_files::docx_with_smartart(&base())).unwrap();
    let doc = opened.presentation.flow.clone().unwrap();
    let html = ibis_formats::docx_convert::export_html_with(&doc, &|shape, members| {
        assert!(matches!(shape.kind, ShapeKind::Placeholder(Placeholder::Diagram)));
        assert_eq!(members.len(), 3, "the shapes of the drawing are given to the picture");
        Some(vec![0x89, b'P', b'N', b'G'])
    });
    assert!(html.contains("<img alt=\"計画, 設計, 実装\" src=\"data:image/png;base64,iVBORw==\">"), "{}", &html[html.len().saturating_sub(600)..]);
    assert!(!html.contains(">計画<"), "the page does not write the shapes' texts again");
}

#[test]
fn a_chart_picture_has_the_charts_title_as_alt() {
    use ibis_doc_model::{EmuRect, ShapeId};
    let opened = ibis_formats::open(base()).unwrap();
    let mut doc = opened.presentation.flow.clone().unwrap();
    let doc = std::sync::Arc::make_mut(&mut doc);
    let mut chart = ibis_doc_model::Chart::empty();
    chart.title = Some(ibis_doc_model::chart::ChartTitle { text: "売上の推移".into(), style: Default::default(), at: None });
    let shape = Shape::new(ShapeId(9002), EmuRect::new(0, 0, 3_000_000, 2_000_000), ShapeKind::Chart(Box::new(chart)));
    let Some(Block::Paragraph(p)) = doc.body.iter_mut().find(|b| matches!(b, Block::Paragraph(p) if !p.runs.is_empty())) else { panic!("a paragraph") };
    let mut run = p.runs[0].clone();
    run.kind = FlowRunKind::Shape(Box::new(shape));
    p.runs.insert(0, run);
    let html = ibis_formats::docx_convert::export_html_with(doc, &|_, _| Some(vec![0x89, b'P', b'N', b'G']));
    assert!(html.contains("<img alt=\"売上の推移\" src="), "{}", &html[..html.len().min(1200)]);
}

/// Writes `test/editing/docx-smartart.docx` (the file the app's tests open): `cargo test --release -p ibis-formats --test docx_smartart -- --ignored`.
#[test]
#[ignore = "writes test/editing/docx-smartart.docx"]
fn write_the_sample_file() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/docx-smartart.docx");
    std::fs::write(path, ibis_formats::test_files::docx_with_smartart(&base())).unwrap();
}

/// Editing another paragraph and saving keeps the SmartArt as it was in the file (its `dgm:relIds` and its parts), writes no shape for the drawing's shapes, and reads back as the same group.
#[test]
fn saving_after_an_edit_keeps_the_smartart_and_writes_no_shapes_for_it() {
    use ibis_doc_model::{Command, Document, FlowCommand, FlowPos};
    let data = ibis_formats::test_files::docx_with_smartart(&base());
    let opened = ibis_formats::open(data.clone()).unwrap();
    let before = opened.presentation.flow.clone().unwrap();
    let p = before.body.iter().find_map(|b| if let Block::Paragraph(p) = b { p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Text(_))).then_some(p) } else { None }).expect("a text paragraph");
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p.id, 1), text: "追加".into(), format: None })]).unwrap();
    let saved = opened.source.save(document.presentation(), ibis_formats::SaveOptions::default()).unwrap();
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    assert!(package.contains("word/diagrams/data1.xml") && package.contains("word/diagrams/drawing1.xml"), "the diagram parts are kept");
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    assert!(xml.contains("drawingml/2006/diagram\"") && xml.contains("dgm:relIds"), "the graphic frame is kept");
    let original = String::from_utf8(ibis_ooxml::Package::open(data).unwrap().read("word/document.xml").unwrap()).unwrap();
    for tag in ["<wp:anchor", "<wp:inline", "<wps:wsp", "<w:drawing"] {
        assert_eq!(xml.matches(tag).count(), original.matches(tag).count(), "{tag}: no drawing is added (the drawing's shapes are not written) or lost");
    }
    assert!(!xml.contains("計画"), "their texts are not written");
    let again = ibis_formats::open(saved).unwrap().presentation.flow.clone().unwrap();
    assert_eq!(again.shape_groups.len(), before.shape_groups.len());
    assert_eq!(again.shape_groups[0].children.len(), 4, "the frame and its three shapes");
}

/// Typing in the SmartArt's own paragraph, before it, saves it still once.
#[test]
fn saving_after_typing_in_the_smartart_paragraph_keeps_it_once() {
    use ibis_doc_model::{Command, Document, FlowCommand, FlowPos};
    let data = ibis_formats::test_files::docx_with_smartart(&base());
    let opened = ibis_formats::open(data.clone()).unwrap();
    let flow = opened.presentation.flow.clone().unwrap();
    let p = flow.body.iter().find_map(|b| if let Block::Paragraph(p) = b { p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Shape(s) if matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram)))).then_some(p) } else { None }).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p.id, 0), text: "前".into(), format: None })]).unwrap();
    let saved = opened.source.save(document.presentation(), ibis_formats::SaveOptions::default()).unwrap();
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    assert_eq!(xml.matches("dgm:relIds").count(), 1, "the graphic frame once");
    assert!(package.contains("word/diagrams/drawing1.xml"));
    assert!(!xml.contains("計画"), "the drawing's shapes are not written");
    let again = ibis_formats::open(saved).unwrap().presentation.flow.clone().unwrap();
    assert_eq!(again.shape_groups.len(), 1);
    assert_eq!(again.shape_groups[0].children.len(), 4);
}

/// #208: a SmartArt shape's text has no typeface of its own; its `fontRef idx="minor"` is the theme's minor font (Yu Mincho in this document's theme, its Jpan font for Japanese text).
#[test]
fn a_smartart_shapes_text_is_in_the_themes_minor_font() {
    let opened = ibis_formats::open(ibis_formats::test_files::docx_with_smartart(&base())).unwrap();
    let doc = opened.presentation.flow.clone().unwrap();
    let shapes = shapes_of(&doc);
    let member = shapes.iter().find(|s| s.text_body().is_some_and(|t| t.text() == "計画")).expect("a shape of the drawing");
    let run = &member.text_body().unwrap().paragraphs[0].runs[0];
    assert_eq!(run.style.font.as_deref(), Some("Yu Mincho"));
}

/// #208: a graphic placed by `wp:align` (centred on the page's column, from the paragraph's top) has its shapes follow it: they keep the frame's alignment and size and their
/// offsets are from its corner.
#[test]
fn the_shapes_of_a_smartart_graphic_placed_by_an_alignment_follow_its_frame() {
    let data = ibis_formats::test_files::docx_with_smartart(&base());
    let mut package = ibis_ooxml::Package::open(data).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let from = xml.rfind("<wp:inline").unwrap();
    let head_end = xml[from..].find('>').unwrap() + from + 1;
    let anchor_head = r#"<wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="margin"><wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV>"#;
    let rest = &xml[head_end..];
    let rest = rest.replacen("<wp:docPr", "<wp:wrapNone/><wp:docPr", 1).replacen("</wp:inline>", "</wp:anchor>", 1);
    let xml = format!("{}{}{}", &xml[..from], anchor_head, rest);
    package.set_bytes("word/document.xml", xml.into_bytes());
    let opened = ibis_formats::open(package.save(ibis_ooxml::SaveOptions::default()).unwrap()).unwrap();
    let doc = opened.presentation.flow.clone().unwrap();
    let shapes = shapes_of(&doc);
    let frame = shapes.iter().find(|s| matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram))).expect("the frame");
    assert_eq!(frame.floating.as_ref().and_then(|f| f.align_h.as_deref()), Some("center"));
    let member = shapes.iter().find(|s| s.text_body().is_some_and(|t| t.text() == "設計")).unwrap();
    let at = member.floating.as_ref().unwrap();
    assert_eq!(at.align_h.as_deref(), Some("center"));
    assert_eq!(at.frame_size, Some((4_320_000, 2_160_000)));
    assert_eq!(at.offset_x, 1_584_000, "from the frame's corner");
    assert_eq!(at.align_v, None);
}
