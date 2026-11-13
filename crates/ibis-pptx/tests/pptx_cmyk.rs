// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The CMYK mode of a file (ibisOffice's own): the flag and the profile's name are saved in an extension PowerPoint ignores
//! and come back when the file is read; the body keeps ordinary sRGB colours; an older extension with colour entries is ignored.
use ibis_doc_model::{Color, Command, Fill, Rgb};
use ibis_pptx::SaveOptions;

fn test_file(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)
}

#[test]
fn the_cmyk_mode_round_trips_and_the_body_keeps_srgb() {
    let bytes = std::fs::read(test_file("benchmark/earnings-presentation.pptx")).expect("read the benchmark file");
    let opened = ibis_pptx::open(bytes).expect("open");
    assert!(!opened.presentation.cmyk.enabled);

    let mut modified = opened.presentation.clone();
    modified.cmyk.enabled = true;
    modified.cmyk.profile = Some("Test & <profile>|abc".into());
    let slide = modified.slides[0].id;
    let shape = modified.slides[0].shapes[0].id;
    let colour = Rgb::from_rgb(12, 34, 56);
    modified.apply(Command::SetFill { slide, shape, fill: Fill::Solid(Color::opaque(colour)) }).expect("set the fill");

    let saved = opened.source.save_presentation(&modified, SaveOptions::default()).expect("save");
    let pkg = ibis_ooxml::Package::open(saved.clone()).expect("package");
    let xml = String::from_utf8(pkg.read("ppt/presentation.xml").unwrap()).unwrap();
    assert!(xml.contains("1B15C0DE-0003"), "the extension is in presentation.xml");
    assert!(xml.contains("Test &amp; &lt;profile&gt;|abc"), "the profile's name is escaped");
    let slide_xml = String::from_utf8(pkg.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    assert!(slide_xml.contains(&format!("srgbClr val=\"{:06X}\"", colour.0)), "the body has an ordinary srgbClr");
    assert!(!slide_xml.to_lowercase().contains("cmyk"), "the body does not mention CMYK");

    let reopened = ibis_pptx::open(saved).expect("reopen");
    assert!(reopened.presentation.cmyk.enabled);
    assert_eq!(reopened.presentation.cmyk.profile.as_deref(), Some("Test & <profile>|abc"));

    // Off again: the extension goes.
    let mut off = reopened.presentation.clone();
    off.cmyk = Default::default();
    let cleared = reopened.source.save_presentation(&off, SaveOptions::default()).expect("save cleared");
    let pkg = ibis_ooxml::Package::open(cleared.clone()).unwrap();
    assert!(!String::from_utf8(pkg.read("ppt/presentation.xml").unwrap()).unwrap().contains("1B15C0DE-0003"));
    assert!(!ibis_pptx::open(cleared).unwrap().presentation.cmyk.enabled);
}

#[test]
fn an_older_extension_with_colour_entries_is_read_without_them() {
    let bytes = std::fs::read(test_file("benchmark/earnings-presentation.pptx")).expect("read");
    let opened = ibis_pptx::open(bytes).expect("open");
    let mut m = opened.presentation.clone();
    m.cmyk.enabled = true;
    let saved = opened.source.save_presentation(&m, SaveOptions::default()).expect("save");
    // Make the extension look like the first builds': version 1 with colour children.
    let mut pkg = ibis_ooxml::Package::open(saved).expect("package");
    let xml = String::from_utf8(pkg.read("ppt/presentation.xml").unwrap()).unwrap();
    let old = xml.replace("version=\"2\"/>", "version=\"1\"><ibis:color rgb=\"ED1C24\" c=\"0\" m=\"100\" y=\"100\" k=\"0\"/></ibis:cmyk>");
    assert_ne!(old, xml);
    pkg.set_bytes("ppt/presentation.xml", old.into_bytes());
    let bytes = pkg.save(SaveOptions::default()).expect("package bytes");
    let again = ibis_pptx::open(bytes).expect("an old extension does not stop the file opening");
    assert!(again.presentation.cmyk.enabled);
}
