// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A formula (`a14:m`) in a paragraph is read as text, and kept when the file is saved.
use ibis_doc_model::{Command, Document};
use ibis_ooxml::{Package, SaveOptions};

const FORMULA: &str = r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" Requires="a14"><a14:m><m:oMathPara xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:oMath><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:sSup><m:e><m:r><m:t>b</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:den></m:f></m:oMath></m:oMathPara></a14:m></mc:Choice><mc:Fallback><a:r><a:t>fallback</a:t></a:r></mc:Fallback></mc:AlternateContent>"#;

fn fixture() -> Vec<u8> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join("pptx-plan.pptx")).unwrap()).unwrap();
    let slide = "ppt/slides/slide1.xml";
    let xml = String::from_utf8(package.read(slide).unwrap()).unwrap();
    let shape = format!(r#"<p:sp><p:nvSpPr><p:cNvPr id="901" name="formula"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="3000000" cy="800000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="en-US"/><a:t>x </a:t></a:r>{FORMULA}</a:p></p:txBody></p:sp>"#);
    package.set_bytes(slide, xml.replacen("</p:spTree>", &(shape + "</p:spTree>"), 1).into_bytes());
    package.save(SaveOptions::default()).unwrap()
}

#[test]
fn a_formula_is_read_as_text_and_kept() {
    let data = fixture();
    let opened = ibis_formats::open(data.clone()).unwrap();
    let slide = &opened.presentation.slides[0];
    let shape = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| b.text() == "x a/b²")).expect("the formula's shape");
    assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);

    // An edit elsewhere leaves the formula's markup as it was.
    let mut doc = Document::new(opened.presentation.clone());
    let other = slide.shapes.iter().find(|s| s.id != shape.id && s.text_body().is_some_and(|b| !b.text().is_empty())).unwrap();
    doc.execute(vec![Command::ReplaceTextRange { slide: slide.id, shape: other.id, at: 0, remove: 0, insert: "Z".into() }]).unwrap();
    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let xml = String::from_utf8(Package::open(saved).unwrap().read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("<m:oMath>") && xml.contains("a14:m"), "the formula is kept");
}

#[test]
fn a_formula_stays_when_the_text_round_it_is_edited() {
    let data = fixture();
    let opened = ibis_formats::open(data).unwrap();
    let slide = &opened.presentation.slides[0];
    let shape = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| b.text() == "x a/b²")).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    doc.execute(vec![Command::ReplaceTextRange { slide: slide.id, shape: shape.id, at: 0, remove: 0, insert: "Z".into() }]).unwrap();
    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let xml = String::from_utf8(Package::open(saved.clone()).unwrap().read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("Zx ") && xml.contains("<m:oMath>"), "the typed text and the formula are both there");
    let again = ibis_formats::read(&saved).unwrap();
    assert!(again.slides[0].shapes.iter().any(|s| s.text_body().is_some_and(|b| b.text() == "Zx a/b²")));
}

#[test]
fn a_formula_edited_in_its_text_is_read_again_and_saved_as_a_formula() {
    let data = fixture();
    let opened = ibis_formats::open(data).unwrap();
    let slide = &opened.presentation.slides[0];
    let shape = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| b.text() == "x a/b²")).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    // "a/b²" becomes "a+b²" (inside the formula's text).
    doc.execute(vec![Command::ReplaceTextRange { slide: slide.id, shape: shape.id, at: 3, remove: 1, insert: "+".into() }]).unwrap();
    let edited = doc.presentation().slides[0].shapes.iter().find(|s| s.id == shape.id).unwrap().text_body().unwrap().clone();
    assert!(edited.paragraphs[0].runs.iter().any(|r| r.style.math.as_ref().is_some_and(|m| m.linear() == "a+b²")), "the formula is read again");
    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let xml = String::from_utf8(Package::open(saved.clone()).unwrap().read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(xml.contains("a14:m") && !xml.contains("<m:f>"), "a formula without the fraction");
    let again = ibis_formats::read(&saved).unwrap();
    assert!(again.slides[0].shapes.iter().any(|s| s.text_body().is_some_and(|b| b.text() == "x a+b²")));
}
