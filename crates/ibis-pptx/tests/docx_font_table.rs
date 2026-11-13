// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A Word file's font table (`word/fontTable.xml`, ECMA-376 Part 1 §17.8.3) is read (alternate names, character set, family,
//! pitch, embedded fonts) and is saved as it was.

use ibis_doc_model::{Command, Document, FlowCommand, FlowPos};
use ibis_ooxml::{Package, SaveOptions as PackageOptions};
use ibis_pptx::SaveOptions;

const KEY: &str = "{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9}";

/// The bytes of an embedded font. The file is saved as it was and the obfuscation is undone, which does not look at what the bytes
/// are, so a few thousand bytes of a fixed pattern stand for a font.
fn font_file() -> Vec<u8> {
    (0..6000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect()
}

/// What Word writes: the first 32 bytes XORed with the GUID's bytes taken from the last to the first.
fn obfuscate(mut data: Vec<u8>) -> Vec<u8> {
    let hex: Vec<u8> = KEY.bytes().filter(u8::is_ascii_hexdigit).collect();
    let key: Vec<u8> = hex.chunks(2).map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap()).rev().collect();
    for i in 0..32 {
        data[i] ^= key[i % 16];
    }
    data
}

fn docx_with_font_table() -> Vec<u8> {
    let mut package = Package::open(std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/docx-paste-source.docx")).unwrap()).unwrap();
    let w = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
    package.set_bytes("word/fontTable.xml", format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:fonts {w}><w:font w:name="Mincho Test"><w:altName w:val="ＭＳ 明朝, Hiragino Mincho ProN"/><w:panose1 w:val="02020609040205080304"/><w:charset w:val="80"/><w:family w:val="roman"/><w:pitch w:val="fixed"/></w:font><w:font w:name="Embedded Test"><w:charset w:val="00"/><w:family w:val="swiss"/><w:pitch w:val="variable"/><w:embedRegular r:id="rId1" w:fontKey="{KEY}"/></w:font></w:fonts>"#
    ).into_bytes());
    package.set_bytes("word/_rels/fontTable.xml.rels", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/font" Target="fonts/font1.odttf"/></Relationships>"#.to_vec());
    package.set_bytes("word/fonts/font1.odttf", obfuscate(font_file()));
    let mut types = String::from_utf8(package.read("[Content_Types].xml").unwrap()).unwrap();
    if !types.contains("odttf") {
        types = types.replace("<Override", r#"<Default Extension="odttf" ContentType="application/vnd.openxmlformats-officedocument.obfuscatedFont"/><Override"#);
    }
    package.set_bytes("[Content_Types].xml", types.into_bytes());
    let mut rels = String::from_utf8(package.read("word/_rels/document.xml.rels").unwrap()).unwrap();
    if !rels.contains("/fontTable") {
        rels = rels.replace("</Relationships>", r#"<Relationship Id="rIdFT" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/></Relationships>"#);
    }
    package.set_bytes("word/_rels/document.xml.rels", rels.into_bytes());
    package.save(PackageOptions::default()).unwrap()
}

#[test]
fn the_font_table_is_read() {
    let opened = ibis_pptx::open(docx_with_font_table()).unwrap();
    let flow = opened.presentation.flow.as_deref().unwrap();
    let mincho = &flow.font_table["Mincho Test"];
    assert_eq!(mincho.alt_names, ["ＭＳ 明朝", "Hiragino Mincho ProN"]);
    assert_eq!(mincho.charset, Some(0x80));
    assert_eq!((mincho.family.as_str(), mincho.pitch.as_str(), mincho.panose.as_str()), ("roman", "fixed", "02020609040205080304"));
    assert_eq!(flow.font_table["Embedded Test"].family, "swiss");
}

#[test]
fn the_embedded_font_is_restored() {
    let opened = ibis_pptx::open(docx_with_font_table()).unwrap();
    let fonts = &opened.presentation.fonts;
    assert_eq!(fonts.len(), 1);
    assert_eq!(fonts[0].typeface, "Embedded Test");
    assert!(!fonts[0].bold && !fonts[0].italic);
    assert_eq!(&fonts[0].data[..], &font_file()[..], "the obfuscation is undone");
}

#[test]
fn saving_keeps_the_font_table_and_its_fonts() {
    let data = docx_with_font_table();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let para = match &d.presentation().flow.as_deref().unwrap().body[0] {
        ibis_doc_model::Block::Paragraph(p) => p.id,
        _ => panic!("a paragraph first"),
    };
    d.execute(vec![Command::Flow(FlowCommand::InsertText { at: FlowPos::new(para, 0), text: "x".into(), format: None })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let (before, after) = (Package::open(data).unwrap(), Package::open(saved.clone()).unwrap());
    for name in ["word/fontTable.xml", "word/_rels/fontTable.xml.rels", "word/fonts/font1.odttf"] {
        assert_eq!(before.read(name).unwrap(), after.read(name).unwrap(), "{name} is saved as it was");
    }
    let again = ibis_pptx::open(saved).unwrap();
    assert_eq!(again.presentation.fonts.len(), 1);
    assert_eq!(again.presentation.flow.as_deref().unwrap().font_table["Mincho Test"].alt_names.len(), 2);
}
