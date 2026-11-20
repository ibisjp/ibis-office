// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Paragraph details found on real files: a bullet sized in points, a run of one space between two others.
use ibis_doc_model::Presentation;
use ibis_ooxml::{Package, SaveOptions};

fn pptx_with(shape_paragraphs: &str) -> Presentation {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join("pptx-plan.pptx")).unwrap()).unwrap();
    let slide = "ppt/slides/slide1.xml";
    let s = String::from_utf8(package.read(slide).unwrap()).unwrap();
    let shape = format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="901" name="probe"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="1000000" y="1000000"/><a:ext cx="5000000" cy="2000000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{shape_paragraphs}</p:txBody></p:sp>"#
    );
    package.set_bytes(slide, s.replacen("</p:spTree>", &(shape + "</p:spTree>"), 1).into_bytes());
    ibis_formats::read(&package.save(SaveOptions::default()).unwrap()).unwrap()
}

fn probe(p: &Presentation) -> &ibis_doc_model::TextBody {
    let slide = &p.slides[0];
    let id = slide.shape_names.iter().find(|(_, n)| n.as_str() == "probe").map(|(id, _)| *id).expect("the probe shape");
    slide.shape(id).and_then(|s| s.text_body()).expect("its text")
}

/// `a:buSzPts` is a size in points: 20 pt over 40 pt text is half the text's size.
#[test]
fn bullet_size_in_points_becomes_a_share_of_the_text() {
    let p = pptx_with(r#"<a:p><a:pPr marL="342900" indent="-342900"><a:buSzPts val="2000"/><a:buAutoNum type="arabicPeriod"/></a:pPr><a:r><a:rPr lang="en-US" sz="4000" b="1"/><a:t>Heading</a:t></a:r></a:p>"#);
    let bullet = probe(&p).paragraphs[0].bullet.as_ref().expect("a numbered bullet");
    assert_eq!(bullet.size, 50_000, "20 pt of 40 pt");
}

/// A run of a single space between two others keeps its space.
#[test]
fn a_run_of_one_space_is_kept() {
    let p = pptx_with(r#"<a:p><a:r><a:rPr lang="en-US"/><a:t>soon</a:t></a:r><a:r><a:rPr lang="en-US"/><a:t> </a:t></a:r><a:r><a:rPr lang="en-US" b="1"/><a:t>as</a:t></a:r></a:p>"#);
    assert_eq!(probe(&p).paragraphs[0].runs.iter().map(|r| r.text.as_str()).collect::<String>(), "soon as");
}
