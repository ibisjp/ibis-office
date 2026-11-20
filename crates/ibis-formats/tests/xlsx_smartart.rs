// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! XLSX-CHEDIT-023: a SmartArt graphic on a sheet shows the shapes of the
//! drawing Excel keeps of it (`dsp:drawing`), placed in its frame and
//! attached to it, and the graphic is kept when the workbook is saved.
//!
//! The file is `xlsx-sales.xlsx` with a hand-written SmartArt (the parts are
//! written the way Excel names them; not a file Excel saved).
use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, Placeholder, SheetCommand, ShapeKind};
use ibis_formats::SaveOptions;

const DGM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn with_smartart() -> Vec<u8> {
    let stock = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let mut package = ibis_ooxml::Package::open(stock).unwrap();
    let drawing = "xl/drawings/drawing1.xml";
    // The drawing the graphic is made of.
    let shapes: String = ["計画", "設計", "実装"]
        .iter()
        .enumerate()
        .map(|(i, t)| {
            format!(
                "<dsp:sp modelId=\"{{0000000{i}-0000-0000-0000-000000000000}}\"><dsp:nvSpPr><dsp:cNvPr id=\"0\" name=\"\"/><dsp:cNvSpPr/></dsp:nvSpPr><dsp:spPr><a:xfrm><a:off x=\"{}\" y=\"100000\"/><a:ext cx=\"900000\" cy=\"500000\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></dsp:spPr><dsp:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"ja-JP\"/><a:t>{t}</a:t></a:r></a:p></dsp:txBody></dsp:sp>",
                i as i64 * 1_000_000
            )
        })
        .collect();
    let drawing_xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><dsp:drawing xmlns:dgm=\"{DGM}\" xmlns:dsp=\"http://schemas.microsoft.com/office/drawing/2008/diagram\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><dsp:spTree><dsp:nvGrpSpPr><dsp:cNvPr id=\"0\" name=\"\"/><dsp:cNvGrpSpPr/></dsp:nvGrpSpPr><dsp:grpSpPr/>{shapes}</dsp:spTree></dsp:drawing>");
    package.set_bytes("xl/diagrams/drawing1.xml", drawing_xml.into_bytes());
    package.set_override("xl/diagrams/drawing1.xml", "application/vnd.ms-office.drawingml.diagramDrawing+xml").unwrap();
    let drawing_id = package.add_relationship(drawing, "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing", "../diagrams/drawing1.xml", false).unwrap();
    // Its data, naming the drawing's relationship.
    let data_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
        <dgm:dataModel xmlns:dgm=\"{DGM}\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
        <dgm:ptLst>\
        <dgm:pt modelId=\"{{00000000-0000-4000-8000-000000000000}}\" type=\"doc\">\
        <dgm:prSet loTypeId=\"urn:microsoft.com/office/officeart/2005/8/layout/default\" loCatId=\"list\" qsTypeId=\"urn:microsoft.com/office/officeart/2005/8/quickstyle/simple1\" qsCatId=\"simple\" csTypeId=\"urn:microsoft.com/office/officeart/2005/8/colors/accent1_2\" csCatId=\"accent1\"/>\
        <dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"ja-JP\" altLang=\"en-US\"/></a:p></dgm:t>\
        </dgm:pt>\
        <dgm:pt modelId=\"{{00000001-0000-0000-0000-000000000000}}\"><dgm:prSet phldrT=\"[テキスト]\"/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"ja-JP\" altLang=\"en-US\"/><a:t>計画</a:t></a:r></a:p></dgm:t></dgm:pt>\
        <dgm:pt modelId=\"{{00000002-0000-0000-0000-000000000000}}\"><dgm:prSet phldrT=\"[テキスト]\"/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"ja-JP\" altLang=\"en-US\"/><a:t>設計</a:t></a:r></a:p></dgm:t></dgm:pt>\
        <dgm:pt modelId=\"{{00000003-0000-0000-0000-000000000000}}\"><dgm:prSet phldrT=\"[テキスト]\"/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"ja-JP\" altLang=\"en-US\"/><a:t>実装</a:t></a:r></a:p></dgm:t></dgm:pt>\
        </dgm:ptLst>\
        <dgm:cxnLst/><dgm:bg/><dgm:whole/>\
        <dgm:extLst><a:ext uri=\"http://schemas.microsoft.com/office/drawing/2008/diagram\">\
        <dsp:dataModelExt xmlns:dsp=\"http://schemas.microsoft.com/office/drawing/2008/diagram\" relId=\"{drawing_id}\" minVer=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>\
        </a:ext></dgm:extLst></dgm:dataModel>"
    );
    package.set_bytes("xl/diagrams/data1.xml", data_xml.into_bytes());
    package.set_override("xl/diagrams/data1.xml", "application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml").unwrap();
    let data_id = package.add_relationship(drawing, &format!("{REL}/diagramData"), "../diagrams/data1.xml", false).unwrap();
    // Layout, QuickStyle, Colors parts for full ECMA-376 compliance.
    let layout_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><dgm:layoutDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" uniqueId=\"urn:microsoft.com/office/officeart/2005/8/layout/default\"><dgm:layoutNode/></dgm:layoutDef>";
    let style_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><dgm:styleDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" uniqueId=\"urn:microsoft.com/office/officeart/2005/8/quickstyle/simple1\"><dgm:scene3d><a:camera prst=\"orthographicFront\"/><a:lightRig rig=\"threePt\" dir=\"t\"/></dgm:scene3d><dgm:styleLbl name=\"node0\"/></dgm:styleDef>";
    let colors_xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><dgm:colorsDef xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" uniqueId=\"urn:microsoft.com/office/officeart/2005/8/colors/accent1_2\"><dgm:styleLbl name=\"node0\"/></dgm:colorsDef>";
    package.set_bytes("xl/diagrams/layout1.xml", layout_xml.as_bytes().to_vec());
    package.set_bytes("xl/diagrams/quickStyle1.xml", style_xml.as_bytes().to_vec());
    package.set_bytes("xl/diagrams/colors1.xml", colors_xml.as_bytes().to_vec());
    package.set_override("xl/diagrams/layout1.xml", "application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml").unwrap();
    package.set_override("xl/diagrams/quickStyle1.xml", "application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml").unwrap();
    package.set_override("xl/diagrams/colors1.xml", "application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml").unwrap();
    let layout_id = package.add_relationship(drawing, &format!("{REL}/diagramLayout"), "../diagrams/layout1.xml", false).unwrap();
    let style_id = package.add_relationship(drawing, &format!("{REL}/diagramQuickStyle"), "../diagrams/quickStyle1.xml", false).unwrap();
    let colors_id = package.add_relationship(drawing, &format!("{REL}/diagramColors"), "../diagrams/colors1.xml", false).unwrap();
    // The anchor with the graphic frame.
    let anchor = format!("<xdr:twoCellAnchor><xdr:from><xdr:col>6</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>12</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>9</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"5\" name=\"図表 2\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{DGM}\"><dgm:relIds xmlns:dgm=\"{DGM}\" xmlns:r=\"{REL}\" r:dm=\"{data_id}\" r:lo=\"{layout_id}\" r:qs=\"{style_id}\" r:cs=\"{colors_id}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>");
    // A picture with an SVG version (Excel keeps a PNG beside it).
    package.set_bytes("xl/media/vector1.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"20\" viewBox=\"0 0 40 20\"><rect width=\"40\" height=\"20\" fill=\"#336699\"/></svg>".to_vec());
    let svg_id = package.add_relationship(drawing, &format!("{REL}/image"), "../media/vector1.svg", false).unwrap();
    let png_id = package.add_relationship(drawing, &format!("{REL}/image"), "../media/image1.png", false).unwrap();
    // Ensure svg content type is registered in [Content_Types].xml.
    {
        let doc = package.xml_mut(ibis_ooxml::package::CONTENT_TYPES).unwrap();
        let root = doc.root_element().unwrap();
        let first_override = doc.children_named(root, "Override").next();
        let index = first_override.and_then(|o| doc.children(root).iter().position(|&c| c == o)).unwrap_or(doc.children(root).len());
        let d = doc.insert_element(root, index, "Default");
        doc.set_attr(d, "Extension", "svg");
        doc.set_attr(d, "ContentType", "image/svg+xml");
    }
    let picture = format!("<xdr:twoCellAnchor><xdr:from><xdr:col>6</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>12</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>9</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>16</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"6\" name=\"グラフィックス 3\"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r=\"{REL}\" r:embed=\"{png_id}\"><a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{svg_id}\"/></a:ext></a:extLst></a:blip><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"100\" cy=\"50\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:twoCellAnchor>");
    let xml = String::from_utf8(package.read(drawing).unwrap()).unwrap().replace("</xdr:wsDr>", &format!("{anchor}{picture}</xdr:wsDr>"));
    package.set_bytes(drawing, xml.into_bytes());
    package.save(SaveOptions::default()).unwrap()
}

#[test]
fn a_smartart_graphic_on_a_sheet_shows_its_shapes_and_is_kept() {
    let data = with_smartart();
    // The file, for the scenario that opens it (test/editing/xlsx-smartart.xlsx).
    let edit_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-smartart.xlsx");
    std::fs::write(&edit_path, &data).unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("xlsx-smartart.xlsx"), &data).unwrap();
    let opened = ibis_formats::open(data.clone()).unwrap();
    let slide = &opened.presentation.slides[0];
    let graphic = slide.shapes.iter().find(|s| matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram))).expect("the graphic");
    let parts = slide.attached.get(&graphic.id).cloned().unwrap_or_default();
    assert_eq!(parts.len(), 3, "its shapes are attached to it");
    let texts: Vec<String> = parts.iter().filter_map(|id| slide.shape(*id)?.text_body().map(|b| b.text())).collect();
    assert_eq!(texts, vec!["計画", "設計", "実装"]);
    for id in &parts {
        let (f, g) = (graphic.frame, slide.shape(*id).unwrap().frame);
        assert!(g.x >= f.x && g.y >= f.y && g.right() <= f.right() + 1 && g.bottom() <= f.bottom() + 1, "{g:?} in {f:?}");
    }
    // The SVG picture is shown from its SVG.
    let svgs = slide.shapes.iter().filter(|s| matches!(&s.kind, ShapeKind::Picture(p) if opened.presentation.media.iter().any(|m| m.id == p.media && m.format == ibis_doc_model::MediaFormat::Svg))).count();
    assert_eq!(svgs, 1, "one picture is an SVG");
    // An edit and a save keep the graphic's parts.
    let mut d = Document::new(opened.presentation.clone());
    let id = d.presentation().slides[0].id;
    d.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: id, edits: vec![CellEdit { row: 40, column: 0, input: CellInput::Value(CellValue::Number(1.0)) }] })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let after = ibis_ooxml::Package::open(saved.clone()).unwrap();
    assert!(after.contains("xl/diagrams/data1.xml") && after.contains("xl/diagrams/drawing1.xml") && after.contains("xl/media/vector1.svg"));
    assert!(String::from_utf8(after.read("xl/drawings/drawing1.xml").unwrap()).unwrap().contains(DGM));
    let again = ibis_formats::read(&saved).unwrap();
    assert!(again.slides[0].shapes.iter().any(|s| matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram))));
    assert!(sheet_of_slide(&again.slides[0]).is_some());
}
