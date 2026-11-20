// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `c:explosion` of a pie series and of single slices is read, and a save that edits nothing in the chart keeps it.
use ibis_doc_model::{Chart, Presentation, ShapeKind};
use ibis_ooxml::{Package, SaveOptions};

fn chart(p: &Presentation) -> &Chart {
    p.slides.iter().flat_map(|s| &s.shapes).find_map(|s| match &s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap()
}

fn pptx_with(xml: &str) -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join("pptx-plan.pptx")).unwrap()).unwrap();
    let part = "ppt/charts/explosion.xml";
    package.set_bytes(part, xml.as_bytes().to_vec());
    package.set_override(part, "application/vnd.openxmlformats-officedocument.drawingml.chart+xml").unwrap();
    let slide = "ppt/slides/slide1.xml";
    let rid = package.add_relationship(slide, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart", "../charts/explosion.xml", false).unwrap();
    let s = String::from_utf8(package.read(slide).unwrap()).unwrap();
    let frame = format!(r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="900" name="pie"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="3000000" cy="2000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#);
    package.set_bytes(slide, s.replacen("</p:spTree>", &(frame + "</p:spTree>"), 1).into_bytes());
    package.save(SaveOptions::default()).unwrap()
}

#[test]
fn pie_explosion_of_series_and_points_is_read_and_kept() {
    let xml = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:pieChart><c:ser><c:idx val="0"/><c:tx><c:v>S1</c:v></c:tx><c:explosion val="10"/><c:dPt><c:idx val="1"/><c:explosion val="30"/></c:dPt><c:cat><c:strLit><c:ptCount val="2"/><c:pt idx="0"><c:v>A</c:v></c:pt><c:pt idx="1"><c:v>B</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt></c:numLit></c:val></c:ser></c:pieChart></c:plotArea></c:chart></c:chartSpace>"#;
    let data = pptx_with(xml);
    let opened = ibis_formats::open(data.clone()).unwrap();
    let series = &chart(&opened.presentation).groups[0].series[0];
    assert_eq!(series.explosion, 10);
    assert_eq!(series.point_explosions, vec![(1, 30)]);
    assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);
    let regenerated = opened.source.save(&opened.presentation, SaveOptions { regenerate_xml: true }).unwrap();
    assert_eq!(chart(&ibis_formats::read(&regenerated).unwrap()).groups[0].series[0].point_explosions, vec![(1, 30)]);
}
