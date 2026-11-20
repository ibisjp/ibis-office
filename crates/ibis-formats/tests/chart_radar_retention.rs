// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Retention-only Radar tests: no native drawing or editable chart support claim.
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, Presentation, ShapeKind, SheetCommand, ChartType, RadarStyle, ChartGroup, Number};
use ibis_ooxml::{Package, SaveOptions};

fn chart_xml(style: &str) -> String {
    format!(r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea><c:radarChart>{style}<c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:v>S1</c:v></c:tx><c:cat><c:strLit><c:ptCount val="1"/><c:pt idx="0"><c:v>A</c:v></c:pt></c:strLit></c:cat><c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val="1"/><c:pt idx="0"><c:v>10</c:v></c:pt></c:numLit></c:val></c:ser><c:axId val="111"/><c:axId val="222"/></c:radarChart></c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend></c:chart><c:extLst><c:ext uri="urn:ibisOffice:retention-test"><keep:opaque xmlns:keep="urn:ibisOffice:test" keep:attribute="unchanged">unknown payload</keep:opaque></c:ext></c:extLst></c:chartSpace>"#)
}


fn fixture(xlsx: bool, xml: &str) -> (Vec<u8>, &'static str) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join(if xlsx { "xlsx-sales.xlsx" } else { "pptx-plan.pptx" })).unwrap()).unwrap();
    let part = if xlsx { "xl/charts/chart1.xml" } else { "ppt/charts/radar-retention.xml" };
    package.set_bytes(part, xml.as_bytes().to_vec());
    if !xlsx {
        package.set_override(part, "application/vnd.openxmlformats-officedocument.drawingml.chart+xml").unwrap();
        let slide = "ppt/slides/slide1.xml";
        let rid = package.add_relationship(slide, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart", "../charts/radar-retention.xml", false).unwrap();
        let xml = String::from_utf8(package.read(slide).unwrap()).unwrap();
        let frame = format!(r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="900" name="retention chart"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="3000000" cy="2000000"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#);
        assert!(xml.contains("</p:spTree>"));
        package.set_bytes(slide, xml.replacen("</p:spTree>", &(frame + "</p:spTree>"), 1).into_bytes());
    }
    (package.save(SaveOptions::default()).unwrap(), part)
}


fn radar_group(p: &Presentation) -> &ChartGroup {
    p.slides.iter().flat_map(|s| &s.shapes).find_map(|s| match &s.kind {
        ShapeKind::Chart(c) => c.groups.iter().find(|g| matches!(g.kind, ChartType::Radar { .. })),
        _ => None,
    }).expect("retained Radar group")
}

fn check_retention(xlsx: bool) {
    for (element, expected) in [("", None), ("<c:radarStyle val=\"standard\"/>", Some(RadarStyle::Standard)), ("<c:radarStyle val=\"marker\"/>", Some(RadarStyle::Marker)), ("<c:radarStyle val=\"filled\"/>", Some(RadarStyle::Filled)), ("<c:radarStyle/>", None), ("<c:radarStyle val=\"future\"/>", None)] {
        let xml = chart_xml(element);
        let (data, part) = fixture(xlsx, &xml);
        let original = Package::open(data.clone()).unwrap().read(part).unwrap();
        let opened = ibis_formats::open(data.clone()).unwrap();
        let group = radar_group(&opened.presentation);
        assert_eq!(group.kind, ChartType::Radar { style: expected });
        assert_eq!(group.axes, vec![111,222]);
        assert_eq!(group.series.len(), 1);
        assert_eq!(group.series[0].values, vec![Some(Number(10.0))]);
        assert_eq!(group.series[0].categories, vec!["A"]);
        let retained = group.clone();
        assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);
        let regenerated = opened.source.save(&opened.presentation, SaveOptions { regenerate_xml: true }).unwrap();
        assert!(ibis_ooxml::canon::diff_packages(&data, &regenerated, &[]).unwrap().is_same());
        assert_eq!(radar_group(&ibis_formats::read(&regenerated).unwrap()), &retained);
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
        assert_ne!(saved, data);
        assert_eq!(Package::open(saved.clone()).unwrap().read(part).unwrap(), original);
        assert_eq!(radar_group(&ibis_formats::read(&saved).unwrap()), &retained);
        assert!(doc.undo());
        assert_eq!(radar_group(doc.presentation()), &retained);
        assert_eq!(opened.source.save(doc.presentation(), SaveOptions::default()).unwrap(), data);
        assert!(doc.redo());
        let redone = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
        assert_eq!(radar_group(&ibis_formats::read(&redone).unwrap()), &retained);
        assert_eq!(Package::open(redone).unwrap().read(part).unwrap(), original);
    }
}

#[test]
fn pptx_radar_values_caches_and_opaque_xml_survive_unrelated_edit_undo_save() { check_retention(false); }
#[test]
fn xlsx_radar_values_caches_and_opaque_xml_survive_unrelated_edit_undo_save() { check_retention(true); }

#[test]
fn changed_radar_style_pptx_rejected_and_xlsx_saved() {
    // PPTX: editing radar style is not supported yet, rejected.
    let (data, _) = fixture(false, &chart_xml("<c:radarStyle val=\"standard\"/>"));
    let opened = ibis_formats::open(data.clone()).unwrap();
    let mut current = opened.presentation.clone();
    let chart = current.slides.iter_mut().flat_map(|s| &mut s.shapes).find_map(|s| match &mut s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap();
    chart.groups[0].kind = ChartType::Radar { style: Some(RadarStyle::Filled) };
    assert!(opened.source.save(&current, SaveOptions::default()).is_err());
    assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);

    // XLSX: editing radar style is supported and updates <c:radarStyle val="filled"/>
    let (data_xlsx, part) = fixture(true, &chart_xml("<c:radarStyle val=\"standard\"/>"));
    let opened_xlsx = ibis_formats::open(data_xlsx.clone()).unwrap();
    let initial_chart = opened_xlsx.presentation.slides.iter().flat_map(|s| &s.shapes).find_map(|s| match &s.kind { ShapeKind::Chart(c) => Some((**c).clone()), _ => None }).unwrap();
    let mut current_xlsx = opened_xlsx.presentation.clone();
    {
        let chart_xlsx = current_xlsx.slides.iter_mut().flat_map(|s| &mut s.shapes).find_map(|s| match &mut s.kind { ShapeKind::Chart(c) => Some(c), _ => None }).unwrap();
        chart_xlsx.groups[0].kind = ChartType::Radar { style: Some(RadarStyle::Filled) };
    }
    let saved_xlsx = opened_xlsx.source.save(&current_xlsx, SaveOptions::default()).expect("xlsx radar style save succeeds");
    let pkg = Package::open(saved_xlsx.clone()).unwrap();
    let xml = String::from_utf8(pkg.read(part).unwrap()).unwrap();
    assert!(xml.contains(r#"<c:radarStyle val="filled"/>"#), "XML should contain filled radar style: {xml}");
    let reopened = ibis_formats::open(saved_xlsx).unwrap();
    let re_group = radar_group(&reopened.presentation);
    assert_eq!(re_group.kind, ChartType::Radar { style: Some(RadarStyle::Filled) });

    // Test undo/redo with Document
    let mut doc = Document::new(opened_xlsx.presentation.clone());
    let (shape_id, _) = doc.presentation().slides.iter().flat_map(|s| &s.shapes).find_map(|s| match &s.kind { ShapeKind::Chart(c) => Some((s.id, (**c).clone())), _ => None }).unwrap();
    let slide_id = doc.presentation().slides.iter().find(|s| s.shapes.iter().any(|sh| sh.id == shape_id)).unwrap().id;
    let mut modified = initial_chart.clone();
    modified.groups[0].kind = ChartType::Radar { style: Some(RadarStyle::Marker) };
    doc.execute(vec![Command::SetShapeKind { slide: slide_id, shape: shape_id, kind: Box::new(ShapeKind::Chart(Box::new(modified.clone()))) }]).unwrap();
    assert_eq!(radar_group(doc.presentation()).kind, ChartType::Radar { style: Some(RadarStyle::Marker) });
    let saved_marker = opened_xlsx.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let pkg_marker = Package::open(saved_marker.clone()).unwrap();
    let xml_marker = String::from_utf8(pkg_marker.read(part).unwrap()).unwrap();
    assert!(xml_marker.contains(r#"<c:radarStyle val="marker"/>"#));

    assert!(doc.undo());
    assert_eq!(radar_group(doc.presentation()).kind, ChartType::Radar { style: Some(RadarStyle::Standard) });
    assert!(doc.redo());
    assert_eq!(radar_group(doc.presentation()).kind, ChartType::Radar { style: Some(RadarStyle::Marker) });
}

#[test]
fn retained_radar_does_not_advance_supported_series_fallback_palette_or_name() {
    let radar=chart_xml("<c:radarStyle val=\"marker\"/>");
    let group=radar.split("<c:plotArea>").nth(1).unwrap().split("</c:plotArea>").next().unwrap();
    let supported=group.replace("radarChart","barChart").replace("<c:radarStyle val=\"marker\"/>","<c:barDir val=\"col\"/>").replace("<c:idx val=\"0\"/>","").replace("<c:tx><c:v>S1</c:v></c:tx>","");
    for xlsx in [false,true] {
        let pure=radar.replace(group,&supported);
        let mixed=radar.replace(group,&format!("{group}{supported}"));
        let find=|p:Presentation|p.slides.iter().flat_map(|s|&s.shapes).find_map(|s|match &s.kind {ShapeKind::Chart(c)=>c.groups.iter().find(|g|matches!(g.kind,ChartType::Bar {..})).cloned(),_=>None}).unwrap();
        let pure=find(ibis_formats::read(&fixture(xlsx,&pure).0).unwrap());
        let mixed=find(ibis_formats::read(&fixture(xlsx,&mixed).0).unwrap());
        assert_eq!(mixed,pure,"supported group remains exactly unchanged");
    }
}

#[test]
#[ignore = "Requires existing native corpus; set IBIS_CHART_CORPUS and run with --include-ignored"]
fn existing_native_corpus_all_three_styles_have_two_series_and_real_axis_links() {
    let root=std::path::PathBuf::from(std::env::var("IBIS_CHART_CORPUS").expect("existing corpus directory"));
    for file in ["pptx-PCHT.pptx","xlsx-XCHT.xlsx"] {
        let opened=ibis_formats::open(std::fs::read(root.join(file)).unwrap()).unwrap();
        let charts:Vec<_>=opened.presentation.slides.iter().flat_map(|s|&s.shapes).filter_map(|s|match &s.kind {ShapeKind::Chart(c) if c.groups.iter().any(|g|matches!(g.kind,ChartType::Radar {..}))=>Some(c),_=>None}).collect();
        assert_eq!(charts.len(),3);
        for (chart,style) in charts.iter().zip([RadarStyle::Standard,RadarStyle::Marker,RadarStyle::Filled]) {
            let g=chart.groups.iter().find(|g|matches!(g.kind,ChartType::Radar {..})).unwrap();
            assert_eq!(g.kind,ChartType::Radar {style:Some(style)});assert_eq!(g.series.len(),2);
            assert_eq!(g.axes,vec![111,222]);assert!(g.axes.iter().all(|id|chart.axis(*id).is_some()));
            assert!(g.series.iter().all(|s|s.values.len()==4&&s.categories.len()==4));
        }
    }
}

#[test]
fn retained_radar_cache_is_not_cut_by_cartesian_category_viewport() {
    let axes = r#"<c:catAx><c:axId val="111"/><c:scaling><c:min val="2"/><c:max val="2"/></c:scaling><c:crossAx val="222"/></c:catAx><c:valAx><c:axId val="222"/><c:crossAx val="111"/></c:valAx>"#;
    let xml=chart_xml("<c:radarStyle val=\"standard\"/>").replace("</c:plotArea>",&format!("{axes}</c:plotArea>"));
    for xlsx in [false,true] {
        let presentation=ibis_formats::read(&fixture(xlsx,&xml).0).unwrap();
        let group=radar_group(&presentation);
        assert_eq!(group.series[0].values,vec![Some(Number(10.0))]);
        assert_eq!(group.series[0].categories,vec!["A"]);
    }
}
