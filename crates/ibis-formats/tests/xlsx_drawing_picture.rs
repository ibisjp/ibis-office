// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A picture in a sheet's drawing whose `xdr:spPr` has no `a:xfrm` (some makers leave it out) is placed by its anchor.
use ibis_doc_model::ShapeKind;
use ibis_ooxml::{Package, SaveOptions};

/// A 1 x 1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[test]
fn a_one_cell_anchored_picture_without_xfrm_is_on_the_sheet() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join("xlsx-table.xlsx")).unwrap()).unwrap();
    let sheet = "xl/worksheets/sheet1.xml";
    package.set_bytes("xl/media/probe.png", PNG.to_vec());
    package.set_override("xl/media/probe.png", "image/png").unwrap();
    let drawing = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:oneCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>4</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx="1905000" cy="952500"/><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="3" name="Image 2"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:oneCellAnchor></xdr:wsDr>"#;
    package.set_bytes("xl/drawings/probe.xml", drawing.as_bytes().to_vec());
    package.set_override("xl/drawings/probe.xml", "application/vnd.openxmlformats-officedocument.drawing+xml").unwrap();
    package.add_relationship("xl/drawings/probe.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image", "../media/probe.png", false).unwrap();
    let rid = package.add_relationship(sheet, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing", "../drawings/probe.xml", false).unwrap();
    let xml = String::from_utf8(package.read(sheet).unwrap()).unwrap();
    let xml = xml.replacen("</worksheet>", &format!(r#"<drawing xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="{rid}"/></worksheet>"#), 1);
    package.set_bytes(sheet, xml.into_bytes());
    let data = package.save(SaveOptions::default()).unwrap();
    let p = ibis_formats::read(&data).unwrap();
    let pictures: Vec<_> = p.slides.iter().flat_map(|s| &s.shapes).filter(|s| matches!(s.kind, ShapeKind::Picture(_))).collect();
    assert_eq!(pictures.len(), 1, "the picture is on the sheet");
    assert_eq!((pictures[0].frame.width, pictures[0].frame.height), (1_905_000, 952_500));
}

/// A chart written without `c:numCache` and `c:strCache` draws the values of the cells it names.
#[test]
fn a_sheet_chart_without_caches_reads_its_cells() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let bytes = std::fs::read(root.join("xlsx-sales.xlsx")).unwrap();
    let series_of = |p: &ibis_doc_model::Presentation| -> Vec<(Vec<Option<f64>>, Vec<String>)> {
        p.slides
            .iter()
            .flat_map(|s| &s.shapes)
            .filter_map(|s| match &s.kind {
                ShapeKind::Chart(c) => Some(c),
                _ => None,
            })
            .flat_map(|c| c.groups.iter().flat_map(|g| g.series.iter().map(|s| (s.values.iter().map(|v| v.map(|n| n.0)).collect(), s.categories.clone()))))
            .collect()
    };
    let with_caches = series_of(&ibis_formats::read(&bytes).unwrap());
    assert!(with_caches.iter().any(|(v, _)| v.iter().any(Option::is_some)), "the file's chart has data");
    let mut package = Package::open(bytes).unwrap();
    let part = "xl/charts/chart1.xml";
    let mut xml = String::from_utf8(package.read(part).unwrap()).unwrap();
    for tag in ["numCache", "strCache", "multiLvlStrCache"] {
        while let (Some(a), Some(b)) = (xml.find(&format!("<c:{tag}>")), xml.find(&format!("</c:{tag}>"))) {
            xml.replace_range(a..b + format!("</c:{tag}>").len(), "");
        }
    }
    package.set_bytes(part, xml.into_bytes());
    let without = series_of(&ibis_formats::read(&package.save(SaveOptions::default()).unwrap()).unwrap());
    assert_eq!(without, with_caches, "the cells give the same data the caches did");
}
