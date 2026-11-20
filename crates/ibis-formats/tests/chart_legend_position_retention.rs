// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Legend source position only; TopRight painting/native meaning is separate.
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, Presentation, ShapeKind, SheetCommand, Chart, LegendPosition as P, Side};
use ibis_ooxml::{Package, SaveOptions};
fn chart_xml(legend: &str) -> String {format!(r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:barChart><c:ser><c:idx val="0"/><c:val><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser></c:barChart></c:plotArea>{legend}</c:chart></c:chartSpace>"#)}
fn chart(p: &Presentation) -> &Chart {p.slides.iter().flat_map(|s|&s.shapes).find_map(|s|match &s.kind {ShapeKind::Chart(c)=>Some(c),_=>None}).unwrap()}
fn fixture(xlsx: bool, xml: &str) -> (Vec<u8>, &'static str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join(if xlsx { "xlsx-sales.xlsx" } else { "pptx-plan.pptx" })).unwrap()).unwrap();
    let part = if xlsx { "xl/charts/chart1.xml" } else { "ppt/charts/legend-position-retention.xml" };
    package.set_bytes(part, xml.as_bytes().to_vec());
    if !xlsx {
        package.set_override(part, "application/vnd.openxmlformats-officedocument.drawingml.chart+xml").unwrap();
        let slide = "ppt/slides/slide1.xml";
        let rid = package.add_relationship(slide, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart", "../charts/legend-position-retention.xml", false).unwrap();
        let xml = String::from_utf8(package.read(slide).unwrap()).unwrap();
        let frame = format!(r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="900" name="retention chart"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="3000000" cy="2000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#);
        assert!(xml.contains("</p:spTree>"));
        package.set_bytes(slide, xml.replacen("</p:spTree>", &(frame + "</p:spTree>"), 1).into_bytes());
    }
    (package.save(SaveOptions::default()).unwrap(), part)
}


fn check(xlsx: bool, legend: &str, expected: Option<Option<P>>) {
 let (data,part)=fixture(xlsx,&chart_xml(legend));let original=Package::open(data.clone()).unwrap().read(part).unwrap();let opened=ibis_formats::open(data.clone()).unwrap();let retained=chart(&opened.presentation).clone();
 assert_eq!(retained.legend.as_ref().map(|l|l.source_position),expected);
 assert_eq!(opened.source.save(&opened.presentation,SaveOptions::default()).unwrap(),data);let regenerated=opened.source.save(&opened.presentation,SaveOptions{regenerate_xml:true}).unwrap();assert!(ibis_ooxml::canon::diff_packages(&data,&regenerated,&[]).unwrap().is_same());assert_eq!(chart(&ibis_formats::read(&regenerated).unwrap()),&retained);
 let mut doc=Document::new(opened.presentation.clone());
        let command = if xlsx {
            let slide = doc.presentation().slides.iter().find(|s| s.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
            Command::Sheet(SheetCommand::SetCells { sheet: slide.id, edits: vec![CellEdit { row: 40, column: 20, input: CellInput::Value(CellValue::Number(42.0)) }] })
        } else {
            let slide = &doc.presentation().slides[0];
            let shape = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| !b.text().is_empty())).unwrap();
            Command::ReplaceTextRange { slide: slide.id, shape: shape.id, at: 0, remove: 1, insert: "Z".into() }
        };
 doc.execute(vec![command]).unwrap();let saved=opened.source.save(doc.presentation(),SaveOptions::default()).unwrap();assert_eq!(Package::open(saved.clone()).unwrap().read(part).unwrap(),original);assert_eq!(chart(&ibis_formats::read(&saved).unwrap()),&retained);
 assert!(doc.undo());assert_eq!(opened.source.save(doc.presentation(),SaveOptions::default()).unwrap(),data);assert!(doc.redo());let saved=opened.source.save(doc.presentation(),SaveOptions::default()).unwrap();assert_eq!(chart(&ibis_formats::read(&saved).unwrap()),&retained);assert_eq!(Package::open(saved).unwrap().read(part).unwrap(),original);
}
fn all_values(xlsx: bool) {
 for (value,position) in [("b",P::Bottom),("l",P::Left),("r",P::Right),("t",P::Top),("tr",P::TopRight)] {
  for overlay in ["0","1"] {check(xlsx,&format!("<c:legend><c:legendPos val=\"{value}\"/><c:overlay val=\"{overlay}\"/></c:legend>"),Some(Some(position)));}
 }
 for child in ["","<c:legendPos/>","<c:legendPos val=\"future\"/>","<f:legendPos xmlns:f=\"urn:foreign\" val=\"tr\"/>","<c:legendPos xmlns:f=\"urn:foreign\" f:val=\"tr\"/>"] {check(xlsx,&format!("<c:legend>{child}</c:legend>"),Some(None));}
 check(xlsx,"",None);check(xlsx,"<f:legend xmlns:f=\"urn:foreign\"><c:legendPos val=\"tr\"/></f:legend>",None);
}
#[test] fn pptx_all_positions_overlay_presence_opaque_xml_save_undo() {all_values(false);}
#[test] fn xlsx_all_positions_overlay_presence_opaque_xml_save_undo() {all_values(true);}
#[test] fn changed_source_position_save_is_rejected_without_dropping_it() {
 for xlsx in [false,true] {let(data,_)=fixture(xlsx,&chart_xml("<c:legend><c:legendPos val=\"tr\"/></c:legend>"));let opened=ibis_formats::open(data).unwrap();assert_eq!(chart(&opened.presentation).legend.as_ref().unwrap().side,Side::Top,"legacy drawing remains unchanged until native calibration");let mut p=opened.presentation.clone();let c=p.slides.iter_mut().flat_map(|s|&mut s.shapes).find_map(|s|match &mut s.kind {ShapeKind::Chart(c)=>Some(c),_=>None}).unwrap();c.legend.as_mut().unwrap().source_position=Some(P::Right);
  // A workbook's chart takes the change (グラフの書式, 2026-10-07); a presentation's still refuses it.
  if xlsx {let saved=opened.source.save(&p,SaveOptions::default()).unwrap();assert_eq!(chart(&ibis_formats::read(&saved).unwrap()).legend.as_ref().unwrap().source_position,Some(P::Right));} else {assert!(opened.source.save(&p,SaveOptions::default()).is_err());}}
}

#[test]
#[ignore = "Requires existing native corpus; set IBIS_CHART_CORPUS"]
fn original_corpus_legend_positions_preserve_all_five_source_values() {
 let root=std::path::PathBuf::from(std::env::var("IBIS_CHART_CORPUS").unwrap());
 for (file,prefix) in [("pptx-PCHT.pptx","ppt/slides/charts"),("xlsx-XCHT.xlsx","xl/drawings/charts")] {
  let data=std::fs::read(root.join(file)).unwrap();let package=Package::open(data.clone()).unwrap();let mut expected=Vec::new();
  for i in 1..=221 {let bytes=package.read(&format!("{prefix}/chart{i}.xml")).unwrap();let tree=roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
   for legend in tree.descendants().filter(|n|n.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/chart","legend"))) {
    let value=legend.children().find(|n|n.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/chart","legendPos"))).and_then(|n|n.attributes().find(|a|a.name()=="val" && a.namespace().is_none()).map(|a|a.value()));
    let position=match value {Some("b")=>Some(P::Bottom),Some("l")=>Some(P::Left),Some("r")=>Some(P::Right),Some("t")=>Some(P::Top),Some("tr")=>Some(P::TopRight),_=>None};expected.push(format!("{position:?}"));
   }
  }
  let p=ibis_formats::read(&data).unwrap();let mut actual:Vec<_>=p.slides.iter().flat_map(|s|&s.shapes).filter_map(|s|match &s.kind {ShapeKind::Chart(c)=>c.legend.as_ref(),_=>None}).map(|l|format!("{:?}",l.source_position)).collect();actual.sort();expected.sort();assert_eq!(actual,expected);
  for position in [P::Bottom,P::Left,P::Right,P::Top,P::TopRight] {assert!(actual.iter().any(|p|p==&format!("Some({position:?})")));}
 }
}
