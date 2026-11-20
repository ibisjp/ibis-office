// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A picture of the old (VML) kind in the text of a Word document: `w:pict/v:shape` with a `v:imagedata`, such as the
//! metafile of a form drawn in Excel and pasted in (a table of 15 rows that was not drawn at all).
use ibis_doc_model::{Block, EmuSize, FlowRunKind, MediaFormat};
use ibis_ooxml::{Package, SaveOptions};

/// A 1 x 1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// A metafile of nothing: its header (88 bytes) and its end record, with a frame of 10 mm square.
fn empty_emf() -> Vec<u8> {
    let mut d = Vec::new();
    let mut put = |v: i32| d.extend(v.to_le_bytes());
    for v in [1, 88, 0, 0, 99, 99, 0, 0, 1000, 1000, 0x464d_4520, 0x0001_0000, 108, 2, 1, 0, 0, 0, 0, 800, 600, 211, 158] {
        put(v);
    }
    for v in [14, 20, 0, 16, 20] {
        put(v);
    }
    d
}

/// A Word document with `media` in it and a paragraph with a VML picture whose `v:imagedata` names it, in `style`
/// (`imagedata` is the element, with `{rid}` for the relationship the media got).
fn docx_with(media: (&str, &[u8], &str), imagedata: &str, style: &str) -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join("docx-paste-source.docx")).unwrap()).unwrap();
    let (name, bytes, content_type) = media;
    let part = format!("word/media/{name}");
    package.set_bytes(&part, bytes.to_vec());
    package.set_override(&part, content_type).unwrap();
    let rid = package.add_relationship("word/document.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image", &format!("media/{name}"), false).unwrap();
    let paragraph = format!(
        r##"<w:p xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:r><w:pict><v:shape id="_x0000_i1026" type="#_x0000_t75" style="{style}">{}</v:shape></w:pict></w:r></w:p>"##,
        imagedata.replace("{rid}", &rid)
    );
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.find("<w:body>").expect("a body") + "<w:body>".len();
    xml.insert_str(at, &paragraph);
    package.set_bytes("word/document.xml", xml.into_bytes());
    package.save(SaveOptions::default()).unwrap()
}

/// The pictures in the text: their sizes and the formats of their media.
fn pictures(data: &[u8]) -> Vec<(EmuSize, MediaFormat)> {
    let presentation = ibis_formats::read(data).unwrap();
    let flow = presentation.flow.clone().expect("a Word document");
    flow.body
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p),
            _ => None,
        })
        .flat_map(|p| &p.runs)
        .filter_map(|r| match &r.kind {
            FlowRunKind::Picture { media, size, .. } => Some((*size, presentation.media(*media).expect("the media").format)),
            _ => None,
        })
        .collect()
}

/// The pictures a document has besides those of the file it is made from (it has one of its own).
fn added(data: &[u8], media: (&str, &[u8], &str)) -> Vec<(EmuSize, MediaFormat)> {
    let mut base = pictures(&docx_with(media, "", ""));
    let mut all = pictures(data);
    for own in base.drain(..) {
        let at = all.iter().position(|p| *p == own).expect("the file's own picture stays");
        all.remove(at);
    }
    all
}

/// A VML picture in the text is a picture of the size its style gives, whatever the format of its part (a bitmap, or
/// a metafile as Word keeps a table pasted in from Excel).
#[test]
fn a_vml_picture_in_the_text_is_a_picture_of_its_styles_size() {
    for (media, format) in [(("probe.png", PNG, "image/png"), MediaFormat::Png), (("probe.emf", empty_emf().leak() as &[u8], "image/x-emf"), MediaFormat::Emf)] {
        let name = media.0;
        let data = docx_with(media, r#"<v:imagedata r:id="{rid}" o:title=""/>"#, "width:450.75pt;height:258.75pt");
        let found = added(&data, media);
        assert_eq!(found.len(), 1, "{name}: one picture");
        assert_eq!(found[0].0, EmuSize::new(5_724_525, 3_286_125), "{name}: 450.75 pt by 258.75 pt");
        assert_eq!(found[0].1, format, "{name}: its format");
    }
}

/// What is not a picture in the text is left alone: one placed at a place of its own (`position:absolute`), one
/// with no width or height, and one that names a relationship that is not there.
#[test]
fn a_vml_picture_at_a_place_of_its_own_with_no_size_or_with_no_part_is_not_one_in_the_text() {
    let png = ("probe.png", PNG, "image/png");
    for (what, style) in [("absolute", "position:absolute;width:100pt;height:50pt"), ("no size", "margin-left:3pt"), ("no height", "width:100pt")] {
        assert!(added(&docx_with(png, r#"<v:imagedata r:id="{rid}"/>"#, style), png).is_empty(), "{what}");
    }
    assert!(added(&docx_with(png, r#"<v:imagedata r:id="rId404"/>"#, "width:10pt;height:10pt"), png).is_empty(), "a relationship that is not there");
}
