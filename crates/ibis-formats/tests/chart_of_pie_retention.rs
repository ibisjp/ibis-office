// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OfPie source retention only; native splits/drawing/editable chart save unproven.
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, Presentation, ShapeKind, SheetCommand, ChartType, OfPieKind, OfPieSplit, OfPieSettings, ChartGroup, Number, Stroke, Color, Rgb};
use ibis_ooxml::{Package, SaveOptions};

fn chart_xml(settings: &str) -> String {
    format!(r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:ofPieChart>{settings}<c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>S1</c:v></c:tx><c:cat><c:strLit><c:ptCount val="1"/><c:pt idx="0"><c:v>A</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="1"/><c:pt idx="0"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser></c:ofPieChart></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend></c:chart><c:extLst><c:ext uri="urn:ibisOffice:retention-test"><keep:opaque xmlns:keep="urn:ibisOffice:test" keep:attribute="unchanged">unknown payload</keep:opaque></c:ext></c:extLst></c:chartSpace>"#)
}


fn fixture(xlsx: bool, xml: &str) -> (Vec<u8>, &'static str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join(if xlsx { "xlsx-sales.xlsx" } else { "pptx-plan.pptx" })).unwrap()).unwrap();
    let part = if xlsx { "xl/charts/chart1.xml" } else { "ppt/charts/of-pie-retention.xml" };
    package.set_bytes(part, xml.as_bytes().to_vec());
    if !xlsx {
        package.set_override(part, "application/vnd.openxmlformats-officedocument.drawingml.chart+xml").unwrap();
        let slide = "ppt/slides/slide1.xml";
        let rid = package.add_relationship(slide, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart", "../charts/of-pie-retention.xml", false).unwrap();
        let xml = String::from_utf8(package.read(slide).unwrap()).unwrap();
        let frame = format!(r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="900" name="retention chart"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="3000000" cy="2000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#);
        assert!(xml.contains("</p:spTree>"));
        package.set_bytes(slide, xml.replacen("</p:spTree>", &(frame + "</p:spTree>"), 1).into_bytes());
    }
    (package.save(SaveOptions::default()).unwrap(), part)
}



fn group(p: &Presentation) -> &ChartGroup {
    p.slides.iter().flat_map(|s| &s.shapes).find_map(|s|match &s.kind {
        ShapeKind::Chart(c)=>c.groups.iter().find(|g|matches!(g.kind,ChartType::OfPie {..})),_=>None,
    }).expect("retained OfPie")
}
fn empty() -> OfPieSettings { OfPieSettings { split:None,split_position:None,second_size:None,gap_width:None,vary_colors:None,custom_points:None,connector:None } }
fn cases() -> Vec<(String,Option<OfPieKind>,OfPieSettings)> {
    let mut cases=Vec::new();
    for (kind,k) in [("pie",OfPieKind::Pie),("bar",OfPieKind::Bar)] {
        for (split,s) in [("cust",OfPieSplit::Custom),("percent",OfPieSplit::Percent),("pos",OfPieSplit::Position),("val",OfPieSplit::Value)] {
            let xml=format!(r#"<c:ofPieType val="{kind}"/><c:splitType val="{split}"/><c:splitPos val="2.5"/><c:secondPieSize val="75"/><c:gapWidth val="100"/><c:varyColors val="1"/><c:custSplit><c:secondPiePt val="0"/><c:secondPiePt val="2"/></c:custSplit><c:serLines><c:spPr><a:ln w="19050"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:ln></c:spPr></c:serLines>"#);
            let value=OfPieSettings {split:Some(s),split_position:Some(Number(2.5)),second_size:Some(75),gap_width:Some(100),vary_colors:Some(true),custom_points:Some(vec![Some(0),Some(2)]),connector:Some(Some(Stroke::solid(Color::opaque(Rgb(0xFF0000)),19050)))};
            cases.push((xml,Some(k),value));
        }
    }
    cases.push(("".into(),None,empty()));
    cases.push((r#"<c:ofPieType/><c:splitType/><c:splitPos val="0"/><c:gapWidth val="0"/><c:secondPieSize val="0"/><c:varyColors/><c:custSplit/><c:serLines/>"#.into(),None,OfPieSettings {split_position:Some(Number(0.0)),gap_width:Some(0),second_size:Some(0),vary_colors:Some(true),custom_points:Some(vec![]),connector:Some(None),..empty()}));
    cases.push((r#"<c:ofPieType val="future"/><c:splitType val="auto"/><c:splitPos val="broken"/><c:gapWidth val="65536"/><c:secondPieSize val="-1"/><c:varyColors val="invalid"/><c:custSplit><c:secondPiePt val="bad"/><c:secondPiePt/><c:secondPiePt val="4294967296"/></c:custSplit><c:serLines><c:spPr><a:ln><a:noFill/></a:ln></c:spPr></c:serLines>"#.into(),None,OfPieSettings {custom_points:Some(vec![None,None,None]),connector:Some(None),..empty()}));
    cases.push((r#"<c:ofPieType val="pie"/><c:varyColors val="0"/>"#.into(),Some(OfPieKind::Pie),OfPieSettings {vary_colors:Some(false),..empty()}));
    cases
}
fn check(xlsx: bool) {
    for (settings,kind,expected) in cases() {
        let (data,part)=fixture(xlsx,&chart_xml(&settings));let original=Package::open(data.clone()).unwrap().read(part).unwrap();let opened=ibis_formats::open(data.clone()).unwrap();
        let g=group(&opened.presentation);assert_eq!(g.kind,ChartType::OfPie {kind});assert_eq!(g.of_pie.as_ref(),Some(&expected));assert_eq!(g.series[0].values,vec![Some(Number(10.0))]);assert_eq!(g.series[0].categories,vec!["A"]);let retained=g.clone();
        assert_eq!(opened.source.save(&opened.presentation,SaveOptions::default()).unwrap(),data);
        let regenerated=opened.source.save(&opened.presentation,SaveOptions {regenerate_xml:true}).unwrap();assert!(ibis_ooxml::canon::diff_packages(&data,&regenerated,&[]).unwrap().is_same());assert_eq!(group(&ibis_formats::read(&regenerated).unwrap()),&retained);
        let mut doc=Document::new(opened.presentation.clone());
        let command=if xlsx {
            let slide=doc.presentation().slides.iter().find(|s|s.shapes.iter().any(|s|matches!(s.kind,ShapeKind::Chart(_)))).unwrap();
            Command::Sheet(SheetCommand::SetCells {sheet:slide.id,edits:vec![CellEdit {row:40,column:20,input:CellInput::Value(CellValue::Number(42.0))}]})
        } else {let slide=&doc.presentation().slides[0];let shape=slide.shapes.iter().find(|s|s.text_body().is_some_and(|b|!b.text().is_empty())).unwrap();Command::ReplaceTextRange {slide:slide.id,shape:shape.id,at:0,remove:1,insert:"Z".into()}};
        doc.execute(vec![command]).unwrap();let saved=opened.source.save(doc.presentation(),SaveOptions::default()).unwrap();assert_ne!(saved,data);assert_eq!(Package::open(saved.clone()).unwrap().read(part).unwrap(),original);assert_eq!(group(&ibis_formats::read(&saved).unwrap()),&retained);
        assert!(doc.undo());assert_eq!(group(doc.presentation()),&retained);assert_eq!(opened.source.save(doc.presentation(),SaveOptions::default()).unwrap(),data);assert!(doc.redo());let redone=opened.source.save(doc.presentation(),SaveOptions::default()).unwrap();assert_eq!(group(&ibis_formats::read(&redone).unwrap()),&retained);assert_eq!(Package::open(redone).unwrap().read(part).unwrap(),original);
    }
}
#[test] fn pptx_of_pie_all_enum_values_presence_and_opaque_source_survive_edit_undo_save() {check(false);}
#[test] fn xlsx_of_pie_all_enum_values_presence_and_opaque_source_survive_edit_undo_save() {check(true);}
#[test] fn changed_of_pie_settings_pptx_rejected_and_xlsx_saved() {
    // 1. PPTX: ofPie edits are rejected
    let (data, _) = fixture(false, &chart_xml("<c:ofPieType val=\"pie\"/>"));
    let opened = ibis_formats::open(data.clone()).unwrap();
    let mut current = opened.presentation.clone();
    let chart = current.slides.iter_mut().flat_map(|s| &mut s.shapes).find_map(|s| match &mut s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap();
    chart.groups[0].of_pie.as_mut().unwrap().split = Some(OfPieSplit::Custom);
    assert!(opened.source.save(&current, SaveOptions::default()).is_err());
    assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);

    // 2. XLSX: ofPie edits (kind and split settings) are saved, roundtripped, and support undo/redo
    let (data, part) = fixture(true, &chart_xml("<c:ofPieType val=\"pie\"/><c:splitType val=\"pos\"/><c:splitPos val=\"2\"/><c:secondPieSize val=\"75\"/><c:gapWidth val=\"100\"/>"));
    let opened = ibis_formats::open(data.clone()).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    let (slide_id, shape_id, mut modified) = doc.presentation().slides.iter().flat_map(|s| s.shapes.iter().map(move |sh| (s.id, sh.id, sh.kind.clone()))).find_map(|(s, id, kind)| match kind {
        ShapeKind::Chart(c) => Some((s, id, (*c).clone())),
        _ => None,
    }).unwrap();

    modified.groups[0].kind = ChartType::OfPie { kind: Some(OfPieKind::Bar) };
    if let Some(op) = modified.groups[0].of_pie.as_mut() {
        op.split = Some(OfPieSplit::Value);
        op.split_position = Some(Number(15.0));
        op.second_size = Some(60);
        op.gap_width = Some(150);
    }
    doc.execute(vec![Command::SetShapeKind { slide: slide_id, shape: shape_id, kind: Box::new(ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();

    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let xml = String::from_utf8(Package::open(saved.clone()).unwrap().read(part).unwrap()).unwrap();
    assert!(xml.contains(r#"<c:ofPieType val="bar"/>"#), "saved xml: {xml}");
    assert!(xml.contains(r#"<c:splitType val="val"/>"#), "saved xml: {xml}");
    assert!(xml.contains(r#"<c:splitPos val="15"/>"#), "saved xml: {xml}");
    assert!(xml.contains(r#"<c:secondPieSize val="60"/>"#), "saved xml: {xml}");
    assert!(xml.contains(r#"<c:gapWidth val="150"/>"#), "saved xml: {xml}");

    let reopened = ibis_formats::read(&saved).unwrap();
    let rg = group(&reopened);
    assert_eq!(rg.kind, ChartType::OfPie { kind: Some(OfPieKind::Bar) });
    let rop = rg.of_pie.as_ref().unwrap();
    assert_eq!(rop.split, Some(OfPieSplit::Value));
    assert_eq!(rop.split_position, Some(Number(15.0)));
    assert_eq!(rop.second_size, Some(60));
    assert_eq!(rop.gap_width, Some(150));

    // Undo restores original
    assert!(doc.undo());
    let restored = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let r_xml = String::from_utf8(Package::open(restored).unwrap().read(part).unwrap()).unwrap();
    assert!(r_xml.contains(r#"<c:ofPieType val="pie"/>"#));
    assert!(r_xml.contains(r#"<c:splitType val="pos"/>"#));
}
#[test] fn retained_of_pie_does_not_advance_supported_palette_or_name() {
    let ofpie=chart_xml("<c:ofPieType val=\"pie\"/>");let source=ofpie.split("<c:plotArea>").nth(1).unwrap().split("</c:plotArea>").next().unwrap();let supported=source.replace("ofPieChart","barChart").replace("<c:ofPieType val=\"pie\"/>","<c:barDir val=\"col\"/>").replace("<c:idx val=\"0\"/>","").replace("<c:tx><c:v>S1</c:v></c:tx>","");
    for xlsx in [false,true] {
        let find=|p:Presentation|p.slides.iter().flat_map(|s|&s.shapes).find_map(|s|match &s.kind {ShapeKind::Chart(c)=>c.groups.iter().find(|g|matches!(g.kind,ChartType::Bar {..})).cloned(),_=>None}).unwrap();let pure=find(ibis_formats::read(&fixture(xlsx,&ofpie.replace(source,&supported)).0).unwrap());let mixed=find(ibis_formats::read(&fixture(xlsx,&ofpie.replace(source,&format!("{source}{supported}"))).0).unwrap());assert_eq!(pure,mixed);
    }
}
#[test]
#[ignore="Requires existing native corpus; set IBIS_CHART_CORPUS and run with --include-ignored"]
fn existing_native_of_pie_corpus_twelve_groups_hold_source_settings() {
    let root=std::path::PathBuf::from(std::env::var("IBIS_CHART_CORPUS").expect("existing corpus directory"));
    for file in ["pptx-PCHT.pptx","xlsx-XCHT.xlsx"] {
        let opened=ibis_formats::open(std::fs::read(root.join(file)).unwrap()).unwrap();let groups:Vec<_>=opened.presentation.slides.iter().flat_map(|s|&s.shapes).filter_map(|s|match &s.kind {ShapeKind::Chart(c)=>Some(c.groups.iter().filter(|g|matches!(g.kind,ChartType::OfPie {..}))),_=>None}).flatten().collect();assert_eq!(groups.len(),6);
        for (g,(kind,split)) in groups.iter().zip([(OfPieKind::Pie,OfPieSplit::Percent),(OfPieKind::Pie,OfPieSplit::Position),(OfPieKind::Pie,OfPieSplit::Value),(OfPieKind::Bar,OfPieSplit::Percent),(OfPieKind::Bar,OfPieSplit::Position),(OfPieKind::Bar,OfPieSplit::Value)]) {
            assert_eq!(g.kind,ChartType::OfPie {kind:Some(kind)});assert_eq!(g.of_pie.as_ref(),Some(&OfPieSettings {split:Some(split),second_size:Some(75),gap_width:Some(100),vary_colors:Some(true),connector:Some(None),..empty()}));assert_eq!(g.series.len(),1);assert_eq!(g.series[0].values,vec![Some(Number(4.3)),Some(Number(2.5)),Some(Number(3.5)),Some(Number(4.5))]);assert_eq!(g.series[0].categories.len(),4);
        }
    }
}
