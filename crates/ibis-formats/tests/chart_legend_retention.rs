// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Finite source retention tests; no native Office or renderer equivalence claim.
//! Overlay edits are still rejected by the existing format writers.
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, Presentation, ShapeKind, SheetCommand};
use ibis_ooxml::{Package, SaveOptions};

fn chart_xml(overlay: &str) -> String {
    format!(r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>S1</c:v></c:tx><c:cat><c:strLit><c:ptCount val="1"/><c:pt idx="0"><c:v>A</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="1"/><c:pt idx="0"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart></c:plotArea><c:legend><c:legendPos val="b"/>{overlay}</c:legend></c:chart><c:extLst><c:ext uri="urn:ibisOffice:retention-test"><keep:opaque xmlns:keep="urn:ibisOffice:test" keep:attribute="unchanged">unknown payload</keep:opaque></c:ext></c:extLst></c:chartSpace>"#)
}

fn fixture(xlsx: bool, overlay: &str) -> (Vec<u8>, &'static str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join(if xlsx { "xlsx-sales.xlsx" } else { "pptx-plan.pptx" })).unwrap()).unwrap();
    let part = if xlsx { "xl/charts/chart1.xml" } else { "ppt/charts/legend-retention.xml" };
    package.set_bytes(part, chart_xml(overlay).into_bytes());
    if !xlsx {
        package.set_override(part, "application/vnd.openxmlformats-officedocument.drawingml.chart+xml").unwrap();
        let slide = "ppt/slides/slide1.xml";
        let rid = package.add_relationship(slide, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart", "../charts/legend-retention.xml", false).unwrap();
        let xml = String::from_utf8(package.read(slide).unwrap()).unwrap();
        let frame = format!(r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="900" name="retention chart"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="3000000" cy="2000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#);
        assert!(xml.contains("</p:spTree>"));
        package.set_bytes(slide, xml.replacen("</p:spTree>", &(frame + "</p:spTree>"), 1).into_bytes());
    }
    (package.save(SaveOptions::default()).unwrap(), part)
}

fn overlay(p: &Presentation) -> Option<bool> {
    p.slides.iter().flat_map(|s| &s.shapes).find_map(|s| match &s.kind {
        ShapeKind::Chart(c) => Some(c.legend.as_ref().expect("legend").overlay),
        _ => None,
    }).expect("a chart")
}

fn check_retention(xlsx: bool) {
    for (xml, expected) in [("", None), ("<c:overlay val=\"0\"/>", Some(false)), ("<c:overlay val=\"1\"/>", Some(true)), ("<c:overlay val=\"false\"/>", Some(false)), ("<c:overlay val=\"true\"/>", Some(true)), ("<c:overlay/>", Some(true)), ("<c:overlay val=\"invalid\"/>", None)] {
        let (data, part) = fixture(xlsx, xml);
        let original_part = Package::open(data.clone()).unwrap().read(part).unwrap();
        let opened = ibis_formats::open(data.clone()).unwrap();
        assert_eq!(overlay(&opened.presentation), expected, "{xml}");
        assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);
        let regenerated = opened.source.save(&opened.presentation, SaveOptions { regenerate_xml: true }).unwrap();
        assert!(ibis_ooxml::canon::diff_packages(&data, &regenerated, &[]).unwrap().is_same());
        assert_eq!(overlay(&ibis_formats::read(&regenerated).unwrap()), expected);

        let mut doc = Document::new(opened.presentation.clone());
        let command = if xlsx {
            let slide = doc.presentation().slides.iter().find(|s| s.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
            Command::Sheet(SheetCommand::SetCells { sheet: slide.id, edits: vec![CellEdit { row: 40, column: 20, input: CellInput::Value(CellValue::Number(42.0)) }] })
        } else {
            let slide = &doc.presentation().slides[0];
            let shape = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| !b.text().is_empty())).unwrap();
            Command::ReplaceTextRange { slide: slide.id, shape: shape.id, at: 0, remove: 1, insert: "Z".into() }
        };
        doc.execute(vec![command]).unwrap();
        let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
        assert_ne!(saved, data, "unrelated edit must be saved");
        assert_eq!(Package::open(saved.clone()).unwrap().read(part).unwrap(), original_part, "including unknown XML and original overlay lexical form");
        assert_eq!(overlay(&ibis_formats::read(&saved).unwrap()), expected);
        assert!(doc.undo());
        assert_eq!(overlay(doc.presentation()), expected);
        assert_eq!(opened.source.save(doc.presentation(), SaveOptions::default()).unwrap(), data);
        assert!(doc.redo());
        assert_eq!(overlay(doc.presentation()), expected);
        let redone = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
        assert_eq!(overlay(&ibis_formats::read(&redone).unwrap()), expected);
        assert_eq!(Package::open(redone).unwrap().read(part).unwrap(), original_part);
    }
}

#[test]
fn pptx_overlay_presence_and_lexical_forms_survive_unrelated_edits() { check_retention(false); }

#[test]
fn xlsx_overlay_presence_and_lexical_forms_survive_unrelated_edits() { check_retention(true); }

#[test]
fn changed_overlay_is_rejected_not_silently_dropped() {
    for xlsx in [false, true] {
        let (data, _) = fixture(xlsx, "<c:overlay val=\"1\"/>");
        let opened = ibis_formats::open(data.clone()).unwrap();
        let mut current = opened.presentation.clone();
        let chart = current.slides.iter_mut().flat_map(|s| &mut s.shapes).find_map(|s| match &mut s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap();
        chart.legend.as_mut().unwrap().overlay = Some(false);
        assert!(opened.source.save(&current, SaveOptions::default()).is_err());
        assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);
    }
}
