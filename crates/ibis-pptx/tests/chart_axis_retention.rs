// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Axis source retention only: no native crossing/layout support claim.
use ibis_doc_model::{AxisCrosses, AxisKind, AxisSourceKind, CellEdit, CellInput, CellValue, Command, Document, Presentation, ShapeKind, SheetCommand, Chart, Number};
use ibis_ooxml::{Package, SaveOptions};
fn chart_xml(crossing: &str) -> String {
    format!(r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/><c:ser><c:idx val="0"/><c:val><c:numLit><c:ptCount val="1"/><c:pt idx="0"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="111"/><c:axId val="222"/></c:barChart><c:catAx><c:axId val="111"/><c:crossAx val="222"/>{crossing}</c:catAx><c:valAx><c:axId val="222"/><c:crossAx val="111"/></c:valAx><c:dateAx><c:axId val="333"/></c:dateAx><c:serAx><c:axId val="444"/><c:crossAx val="111"/></c:serAx><foreign:serAx xmlns:foreign="urn:foreign"><c:axId val="555"/></foreign:serAx></c:plotArea></c:chart></c:chartSpace>"#)
}
fn chart(p: &Presentation) -> &Chart {
    p.slides.iter().flat_map(|s| &s.shapes).find_map(|s| match &s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap()
}
fn fixture(xlsx: bool, xml: &str) -> (Vec<u8>, &'static str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join(if xlsx { "xlsx-sales.xlsx" } else { "pptx-plan.pptx" })).unwrap()).unwrap();
    let part = if xlsx { "xl/charts/chart1.xml" } else { "ppt/charts/axis-retention.xml" };
    package.set_bytes(part, xml.as_bytes().to_vec());
    if !xlsx {
        package.set_override(part, "application/vnd.openxmlformats-officedocument.drawingml.chart+xml").unwrap();
        let slide = "ppt/slides/slide1.xml";
        let rid = package.add_relationship(slide, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart", "../charts/axis-retention.xml", false).unwrap();
        let xml = String::from_utf8(package.read(slide).unwrap()).unwrap();
        let frame = format!(r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="900" name="retention chart"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="3000000" cy="2000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#);
        assert!(xml.contains("</p:spTree>"));
        package.set_bytes(slide, xml.replacen("</p:spTree>", &(frame + "</p:spTree>"), 1).into_bytes());
    }
    (package.save(SaveOptions::default()).unwrap(), part)
}


fn check_retention(xlsx: bool) {
    for element in ["", "<c:crosses val=\"autoZero\"/>", "<c:crosses val=\"max\"/>", "<c:crosses val=\"min\"/>", "<c:crossesAt val=\"0\"/>", "<c:crossesAt val=\"-2.5\"/>", "<c:crosses/>", "<c:crossesAt/>", "<c:crossesAt val=\"INF\"/>", "<c:crosses val=\"max\"/><c:crossesAt val=\"bad\"/>", "<c:crosses val=\"max\"/><c:crossesAt val=\"3.25\"/>", "<c:crosses val=\"future\"/><c:crossesAt val=\"NaN\"/>", "<foreign:crosses xmlns:foreign=\"urn:foreign\" val=\"max\"/>"] {
        let xml = chart_xml(element);
        let (data, part) = fixture(xlsx, &xml);
        let original = Package::open(data.clone()).unwrap().read(part).unwrap();
        let opened = ibis_pptx::open(data.clone()).unwrap();
        let retained = chart(&opened.presentation).clone();
        assert_eq!(retained.axes.len(), 4);
        assert_eq!(retained.axes.iter().map(|a| a.source_kind).collect::<Vec<_>>(), vec![AxisSourceKind::Category, AxisSourceKind::Value, AxisSourceKind::Date, AxisSourceKind::Series]);
        assert_eq!(retained.axes[3].kind, AxisKind::Series);
        assert_eq!(retained.axes[0].source_id, Some(111));
        assert_eq!(retained.axes[0].cross_axis_id, Some(222));
        let expected_crosses = if element.contains("foreign:") { None } else if element.contains("autoZero") { Some(AxisCrosses::AutoZero) } else if element.contains("max") { Some(AxisCrosses::Maximum) } else if element.contains("min") { Some(AxisCrosses::Minimum) } else { None };
        assert_eq!(retained.axes[0].crosses, expected_crosses);
        let expected_at = if element.contains("3.25") {Some(Number(3.25))} else if element.contains("-2.5") {Some(Number(-2.5))} else if element.contains("crossesAt val=\"0") {Some(Number(0.0))} else {None};
        assert_eq!(retained.axes[0].crosses_at, expected_at);
        assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);
        let regenerated = opened.source.save(&opened.presentation, SaveOptions { regenerate_xml: true }).unwrap();
        assert!(ibis_ooxml::canon::diff_packages(&data, &regenerated, &[]).unwrap().is_same());
        assert_eq!(chart(&ibis_pptx::read(&regenerated).unwrap()), &retained);
        let mut doc = Document::new(opened.presentation.clone());
        let command = if xlsx {
            let slide = doc.presentation().slides.iter().find(|s| s.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
            Command::Sheet(SheetCommand::SetCells { slide: slide.id, edits: vec![CellEdit { row: 40, column: 20, input: CellInput::Value(CellValue::Number(42.0)) }] })
        } else {
            let slide = &doc.presentation().slides[0];
            let shape = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| !b.text().is_empty())).unwrap();
            Command::EditText { slide: slide.id, shape: shape.id, at: 0, remove: 1, insert: "Z".into() }
        };
        doc.execute(vec![command]).unwrap();
        let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
        assert_ne!(saved, data);
        assert_eq!(Package::open(saved.clone()).unwrap().read(part).unwrap(), original);
        assert_eq!(chart(&ibis_pptx::read(&saved).unwrap()), &retained);
        assert!(doc.undo());
        assert_eq!(chart(doc.presentation()), &retained);
        assert_eq!(opened.source.save(doc.presentation(), SaveOptions::default()).unwrap(), data);
        assert!(doc.redo());
        let redone = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
        assert_eq!(chart(&ibis_pptx::read(&redone).unwrap()), &retained);
        assert_eq!(Package::open(redone).unwrap().read(part).unwrap(), original);
    }
}


#[test]
fn pptx_crossing_source_survives_save_unrelated_edit_undo_redo() {check_retention(false);}
#[test]
fn xlsx_crossing_source_survives_save_unrelated_edit_undo_redo() {check_retention(true);}
#[test]
fn changed_crossing_is_saved_or_rejected_never_dropped() {
 for xlsx in [false,true] {
  let (data,_) = fixture(xlsx, &chart_xml("")); let opened=ibis_pptx::open(data).unwrap();
  for field in 0..3 {
   let mut p=opened.presentation.clone();
   let c=p.slides.iter_mut().flat_map(|s| &mut s.shapes).find_map(|s|match &mut s.kind {ShapeKind::Chart(c)=>Some(c),_=>None}).unwrap();
   match field {0=>c.axes[0].crosses=Some(AxisCrosses::Minimum),1=>c.axes[0].crosses_at=Some(Number(7.0)),_=>c.axes[0].cross_axis_id=Some(999)}
   if field<2 {
    // The shared chart writer supports explicit crossing values in both formats.
    if let Ok(saved)=opened.source.save(&p,SaveOptions::default()) {
     let restored=ibis_pptx::read(&saved).unwrap();
     assert_eq!(chart(&restored).axes[0].crosses,chart(&p).axes[0].crosses);
     assert_eq!(chart(&restored).axes[0].crosses_at,chart(&p).axes[0].crosses_at);
    }
   }else{assert!(opened.source.save(&p,SaveOptions::default()).is_err());}
  }
 }
}
#[test]
fn invalid_and_foreign_links_do_not_become_zero_or_typed_links() {
 for xlsx in [false,true] {
  for value in ["-1","4294967296","bad",""] {
   let xml=chart_xml("").replace("<c:axId val=\"111\"/>",&format!("<c:axId val=\"{value}\"/>" )).replace("<c:crossAx val=\"222\"/>",&format!("<c:crossAx val=\"{value}\"/>"));
   let p=ibis_pptx::read(&fixture(xlsx,&xml).0).unwrap(); assert_eq!(chart(&p).axes[0].source_id,None); assert_eq!(chart(&p).axes[0].cross_axis_id,None);
  }
  let xml=chart_xml("").replace("<c:crossAx val=\"222\"/>","<foreign:crossAx xmlns:foreign=\"urn:foreign\" val=\"999\"/>");
  let p=ibis_pptx::read(&fixture(xlsx,&xml).0).unwrap(); assert_eq!(chart(&p).axes[0].cross_axis_id,None);
 }
}

#[test]
#[ignore = "Requires existing native corpus; set IBIS_CHART_CORPUS"]
fn original_corpus_axis_kinds_crossings_and_links_are_retained() {
 let root=std::path::PathBuf::from(std::env::var("IBIS_CHART_CORPUS").unwrap());
 for (file,prefix) in [("pptx-PCHT.pptx","ppt/slides/charts"),("xlsx-XCHT.xlsx","xl/drawings/charts")] {
  let data=std::fs::read(root.join(file)).unwrap(); let package=Package::open(data.clone()).unwrap();
  let mut expected=Vec::new();
  for i in 1..=221 {
   let bytes=package.read(&format!("{prefix}/chart{i}.xml")).unwrap(); let xml=std::str::from_utf8(&bytes).unwrap(); let tree=roxmltree::Document::parse(xml).unwrap();
   for node in tree.descendants().filter(|n|n.is_element() && n.tag_name().namespace()==Some("http://schemas.openxmlformats.org/drawingml/2006/chart") && matches!(n.tag_name().name(),"catAx"|"valAx"|"dateAx"|"serAx")) {
    let value=|name|node.children().find(|n|n.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/chart",name))).and_then(|n|n.attribute("val"));
    let kind=match node.tag_name().name() {"valAx"=>AxisSourceKind::Value,"dateAx"=>AxisSourceKind::Date,"serAx"=>AxisSourceKind::Series,_=>AxisSourceKind::Category};
    let crosses=match value("crosses") {Some("autoZero")=>Some(AxisCrosses::AutoZero),Some("max")=>Some(AxisCrosses::Maximum),Some("min")=>Some(AxisCrosses::Minimum),_=>None};
    expected.push(format!("{:?}", (kind,value("axId").and_then(|v|v.parse::<u32>().ok()),crosses,value("crossesAt").and_then(|v|v.parse::<f64>().ok()).filter(|v|v.is_finite()).map(Number),value("crossAx").and_then(|v|v.parse::<u32>().ok()))));
   }
  }
  let p=ibis_pptx::read(&data).unwrap();
  let mut actual:Vec<_>=p.slides.iter().flat_map(|s|&s.shapes).filter_map(|s|match &s.kind {ShapeKind::Chart(c)=>Some(c),_=>None}).flat_map(|c|&c.axes).map(|a|format!("{:?}",(a.source_kind,a.source_id,a.crosses,a.crosses_at,a.cross_axis_id))).collect();
  expected.sort();actual.sort();assert_eq!(actual,expected,"all original axis source fields including series axes");
  assert!(actual.iter().any(|a|a.contains("Maximum")));assert!(actual.iter().any(|a|a.contains("Minimum")));assert!(actual.iter().any(|a|a.contains("Series")));
 }
}

fn assert_foreign_source_survives(xlsx: bool, xml: &str, baseline: &Chart) {
    let (data, part) = fixture(xlsx, xml);
    let original = Package::open(data.clone()).unwrap().read(part).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    assert_eq!(chart(&opened.presentation), baseline);
    assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);
    let regenerated = opened.source.save(&opened.presentation, SaveOptions {regenerate_xml: true}).unwrap();
    assert!(ibis_ooxml::canon::diff_packages(&data, &regenerated, &[]).unwrap().is_same());
    let mut doc = Document::new(opened.presentation.clone());
        let command = if xlsx {
            let slide = doc.presentation().slides.iter().find(|s| s.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap();
            Command::Sheet(SheetCommand::SetCells { slide: slide.id, edits: vec![CellEdit { row: 40, column: 20, input: CellInput::Value(CellValue::Number(42.0)) }] })
        } else {
            let slide = &doc.presentation().slides[0];
            let shape = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| !b.text().is_empty())).unwrap();
            Command::EditText { slide: slide.id, shape: shape.id, at: 0, remove: 1, insert: "Z".into() }
        };
    doc.execute(vec![command]).unwrap();
    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(Package::open(saved.clone()).unwrap().read(part).unwrap(), original);
    assert_eq!(chart(&ibis_pptx::read(&saved).unwrap()), baseline);
    assert!(doc.undo());
    assert_eq!(opened.source.save(doc.presentation(), SaveOptions::default()).unwrap(), data);
    assert!(doc.redo());
    let redone = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(Package::open(redone.clone()).unwrap().read(part).unwrap(), original);
    assert_eq!(chart(&ibis_pptx::read(&redone).unwrap()), baseline);
}

#[test]
fn foreign_axis_nodes_and_ids_cannot_change_supported_cache_or_links() {
 for xlsx in [false,true] {
  let xml=chart_xml("").replace("<c:val><c:numLit>","<c:cat><c:strLit><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>A</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit>");
  let base=ibis_pptx::read(&fixture(xlsx,&xml).0).unwrap();
  for tag in ["catAx","dateAx"] {
   let foreign=format!(r#"<f:{tag} xmlns:f="urn:foreign"><c:axId val="111"/><c:scaling><c:min val="2"/><c:max val="2"/></c:scaling></f:{tag}>"#);
   let injected=xml.replace("</c:plotArea>",&format!("{foreign}</c:plotArea>"));
   let p=ibis_pptx::read(&fixture(xlsx,&injected).0).unwrap();assert_eq!(chart(&p),chart(&base),"foreign {tag} must not clip supported series cache");assert_foreign_source_survives(xlsx,&injected,chart(&base));
  }
  let foreign_id=r#"<f:axId xmlns:f="urn:foreign" val="999"/>"#;
  let injected=xml.replace("<c:axId val=\"111\"/>",&format!("{foreign_id}<c:axId val=\"111\"/>"));
  let p=ibis_pptx::read(&fixture(xlsx,&injected).0).unwrap();assert_eq!(chart(&p),chart(&base),"foreign axis IDs must not supply legacy/source/group linkage");assert_foreign_source_survives(xlsx,&injected,chart(&base));
 }
}

#[test]
fn foreign_val_attributes_do_not_supply_crossings_axis_ids_or_group_ids() {
 for xlsx in [false,true] {
  let xml=chart_xml(r#"<c:crosses xmlns:f="urn:foreign" f:val="max"/><c:crossesAt xmlns:f="urn:foreign" f:val="7"/>"#)
   .replace("<c:crossAx val=\"222\"/>",r#"<c:crossAx xmlns:f="urn:foreign" f:val="222"/>"#)
   .replace("<c:axId val=\"111\"/>",r#"<c:axId xmlns:f="urn:foreign" f:val="111"/>"#);
  let(data,part)=fixture(xlsx,&xml);let opened=ibis_pptx::open(data.clone()).unwrap();let c=chart(&opened.presentation);
  assert_eq!(c.axes[0].source_id,None);assert_eq!(c.axes[0].crosses,None);assert_eq!(c.axes[0].crosses_at,None);assert_eq!(c.axes[0].cross_axis_id,None);assert_eq!(c.groups[0].axes,vec![222]);
  assert_eq!(opened.source.save(&opened.presentation,SaveOptions::default()).unwrap(),data);
  let regenerated=opened.source.save(&opened.presentation,SaveOptions{regenerate_xml:true}).unwrap();assert!(ibis_ooxml::canon::diff_packages(&data,&regenerated,&[]).unwrap().is_same());assert_eq!(chart(&ibis_pptx::read(&regenerated).unwrap()),c);assert_eq!(Package::open(regenerated).unwrap().read(part).unwrap(),Package::open(data).unwrap().read(part).unwrap());
 }
}
