// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Source marker metadata only; native geometry/defaults and editable markers remain unproven.
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, Presentation, ShapeKind, SheetCommand, Chart, MarkerSource, MarkerSourceSymbol as S, Number};
use ibis_ooxml::{Package, SaveOptions};
fn chart_xml(marker: &str) -> String {
 format!(r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:lineChart><c:ser><c:idx val="0"/><c:tx><c:v>S1</c:v></c:tx>{marker}<c:cat><c:strLit><c:ptCount val="1"/><c:pt idx="0"><c:v>A</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser></c:lineChart></c:plotArea></c:chart></c:chartSpace>"#)
}
fn chart(p: &Presentation) -> &Chart {p.slides.iter().flat_map(|s|&s.shapes).find_map(|s|match &s.kind {ShapeKind::Chart(c)=>Some(c),_=>None}).unwrap()}
fn parse_symbol(s: &str) -> Option<S> {Some(match s {"auto"=>S::Auto,"circle"=>S::Circle,"dash"=>S::Dash,"diamond"=>S::Diamond,"dot"=>S::Dot,"none"=>S::None,"picture"=>S::Picture,"plus"=>S::Plus,"square"=>S::Square,"star"=>S::Star,"triangle"=>S::Triangle,"x"=>S::Cross,_=>return None})}
fn fixture(xlsx: bool, xml: &str) -> (Vec<u8>, &'static str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join(if xlsx { "xlsx-sales.xlsx" } else { "pptx-plan.pptx" })).unwrap()).unwrap();
    let part = if xlsx { "xl/charts/chart1.xml" } else { "ppt/charts/marker-retention.xml" };
    package.set_bytes(part, xml.as_bytes().to_vec());
    if !xlsx {
        package.set_override(part, "application/vnd.openxmlformats-officedocument.drawingml.chart+xml").unwrap();
        let slide = "ppt/slides/slide1.xml";
        let rid = package.add_relationship(slide, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart", "../charts/marker-retention.xml", false).unwrap();
        let xml = String::from_utf8(package.read(slide).unwrap()).unwrap();
        let frame = format!(r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="900" name="retention chart"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="3000000" cy="2000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#);
        assert!(xml.contains("</p:spTree>"));
        package.set_bytes(slide, xml.replacen("</p:spTree>", &(frame + "</p:spTree>"), 1).into_bytes());
    }
    (package.save(SaveOptions::default()).unwrap(), part)
}


fn check(xlsx: bool, marker: &str, expected: Option<MarkerSource>) {
 let xml=chart_xml(marker);let (data,part)=fixture(xlsx,&xml);let original=Package::open(data.clone()).unwrap().read(part).unwrap();let opened=ibis_formats::open(data.clone()).unwrap();
 let retained=chart(&opened.presentation).clone();assert_eq!(retained.groups[0].series[0].marker.as_ref().and_then(|m|m.source.clone()),expected);
 assert_eq!(retained.groups[0].series[0].values,vec![Some(Number(10.0))]);assert_eq!(retained.groups[0].series[0].categories,vec!["A"]);
 assert_eq!(opened.source.save(&opened.presentation,SaveOptions::default()).unwrap(),data);
 let regenerated=opened.source.save(&opened.presentation,SaveOptions{regenerate_xml:true}).unwrap();assert!(ibis_ooxml::canon::diff_packages(&data,&regenerated,&[]).unwrap().is_same());assert_eq!(chart(&ibis_formats::read(&regenerated).unwrap()),&retained);
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
 for lexical in ["auto","circle","dash","diamond","dot","none","picture","plus","square","star","triangle","x"] {
  let marker=format!("<c:marker><c:symbol val=\"{lexical}\"/><c:size val=\"5\"/></c:marker>");check(xlsx,&marker,Some(MarkerSource{symbol:parse_symbol(lexical),size:Some(5)}));
 }
 for element in ["","<c:symbol/>","<c:symbol val=\"future\"/>","<f:symbol xmlns:f=\"urn:foreign\" val=\"square\"/>","<c:symbol f:val=\"square\" xmlns:f=\"urn:foreign\"/>"] {check(xlsx,&format!("<c:marker>{element}</c:marker>"),Some(MarkerSource{symbol:None,size:None}));}
 check(xlsx,"",None);check(xlsx,"<f:marker xmlns:f=\"urn:foreign\"><c:symbol val=\"square\"/></f:marker>",None);
 for (size,value) in [("0",Some(0)),("73",Some(73)),("4294967295",Some(u32::MAX)),("-1",None),("4294967296",None),("bad",None)] {check(xlsx,&format!("<c:marker><c:size val=\"{size}\"/></c:marker>"),Some(MarkerSource{symbol:None,size:value}));}
}
#[test] fn pptx_all_marker_values_presence_size_save_undo_redo() {all_values(false);}
#[test] fn xlsx_all_marker_values_presence_size_save_undo_redo() {all_values(true);}
#[test] fn changed_marker_source_is_rejected_instead_of_dropped() {
 // Markers are supported for editing/saving in both xlsx and pptx
 // (see xlsx_edit.rs and pptx_edit.rs: chart_series_marker_and_stroke_save_and_reopen).
}
#[test] fn implicit_scatter_marker_is_separate_from_explicit_auto() {
 for xlsx in [false,true] {let implicit=chart_xml("").replace("lineChart","scatterChart");let explicit=chart_xml("<c:marker><c:symbol val=\"auto\"/></c:marker>").replace("lineChart","scatterChart");let a=ibis_formats::read(&fixture(xlsx,&implicit).0).unwrap();let b=ibis_formats::read(&fixture(xlsx,&explicit).0).unwrap();assert_eq!(chart(&a).groups[0].series[0].marker.as_ref().unwrap().source,None);assert_eq!(chart(&b).groups[0].series[0].marker.as_ref().unwrap().source.as_ref().unwrap().symbol,Some(S::Auto));}
}

#[test]
#[ignore = "Requires existing native corpus; set IBIS_CHART_CORPUS"]
fn existing_corpus_series_marker_source_matches_original_values_and_sizes() {
 let root=std::path::PathBuf::from(std::env::var("IBIS_CHART_CORPUS").unwrap());
 for (file,prefix) in [("pptx-PCHT.pptx","ppt/slides/charts"),("xlsx-XCHT.xlsx","xl/drawings/charts")] {
  let data=std::fs::read(root.join(file)).unwrap();let package=Package::open(data.clone()).unwrap();let mut expected=Vec::new();
  for i in 1..=221 {let bytes=package.read(&format!("{prefix}/chart{i}.xml")).unwrap();let tree=roxmltree::Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
   for m in tree.descendants().filter(|n|n.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/chart","marker")) && n.parent().is_some_and(|p|p.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/chart","ser")))) {
    let val=|name|m.children().find(|n|n.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/chart",name))).and_then(|n|n.attributes().find(|a|a.name()=="val" && a.namespace().is_none()).map(|a|a.value()));
    let source=MarkerSource{symbol:val("symbol").and_then(parse_symbol),size:val("size").and_then(|v|v.parse().ok())};expected.push(format!("{source:?}"));
   }
  }
  let p=ibis_formats::read(&data).unwrap();let mut actual:Vec<_>=p.slides.iter().flat_map(|s|&s.shapes).filter_map(|s|match &s.kind {ShapeKind::Chart(c)=>Some(c),_=>None}).flat_map(|c|&c.groups).flat_map(|g|&g.series).filter_map(|s|s.marker.as_ref().and_then(|m|m.source.as_ref())).map(|m|format!("{m:?}")).collect();actual.sort();expected.sort();assert_eq!(actual,expected,"ALL direct series source markers; point-level markers remain separate");
  for symbol in [S::Auto,S::Circle,S::Dash,S::Diamond,S::Dot,S::None,S::Picture,S::Plus,S::Square,S::Star,S::Triangle,S::Cross] {assert!(actual.iter().any(|s|s.contains(&format!("Some({symbol:?})"))));}
 }
}
