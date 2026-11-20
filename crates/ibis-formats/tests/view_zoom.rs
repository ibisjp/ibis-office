// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #237: the zoom a file was saved at is read (a document's `w:zoom/@w:percent`, a presentation's slide view scale) and written back
//! when it was changed; a file whose zoom was not changed is saved as it was.
use ibis_doc_model::Document;
use ibis_formats::SaveOptions;

fn file(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)).unwrap()
}

fn zoom_roundtrip(name: &str, to: u16) {
    let data = file(name);
    let opened = ibis_formats::open(data.clone()).unwrap();
    // Unchanged: the file as it was.
    let same = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    assert_eq!(same, data, "{name}: not changed, not rewritten");
    let mut doc = Document::new(opened.presentation.clone());
    doc.presentation_mut().view_zoom = Some(to);
    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let back = ibis_formats::read(&saved).unwrap();
    assert_eq!(back.view_zoom, Some(to), "{name}: the zoom is the file's");
    // And a second time over the saved file, another zoom.
    let again = ibis_formats::open(saved).unwrap();
    let mut doc = Document::new(again.presentation.clone());
    doc.presentation_mut().view_zoom = Some(to / 2 + 10);
    let saved = again.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(ibis_formats::read(&saved).unwrap().view_zoom, Some(to / 2 + 10), "{name}: changed again");
}

#[test]
fn a_documents_zoom_is_written_and_read_back() {
    zoom_roundtrip("autospace-letters.docx", 175);
}

/// A presentation is opened fitted to the window whatever its slide view says (PowerPoint for Windows, `tasks/doc/requests/2026-11-17-237-varscale-result.md`: `varScale`
/// 1, 0 or none, `scale` 119 % or 60 %, a window of 1000 x 700 pt or of 700 x 500: always fitted): no zoom is read. The file saved unchanged is as it was; a zoom put
/// on it is written as PowerPoint writes the zoom a user set (`varScale="0"` and the scale), the window fitted again as `varScale="1"`, and read as fitted anyway.
#[test]
fn a_presentation_is_opened_fitted_and_the_zoom_put_on_it_is_written() {
    let data = file("pptx-plan.pptx");
    let opened = ibis_formats::open(data.clone()).unwrap();
    assert_eq!(opened.presentation.view_zoom, None, "varScale=1, scale 119%: fitted");
    let view = |bytes: &[u8]| {
        let mut zip = zip_text(bytes, "ppt/viewProps.xml");
        zip.truncate(zip.find("<p:notesTextViewPr").unwrap());
        zip
    };
    assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), data);
    let mut doc = Document::new(opened.presentation.clone());
    doc.presentation_mut().view_zoom = Some(150);
    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    assert!(view(&saved).contains("varScale=\"0\"") && view(&saved).contains("n=\"150\""));
    assert_eq!(ibis_formats::read(&saved).unwrap().view_zoom, None, "a file with varScale=0 and scale 150 opens fitted too");
    let again = ibis_formats::open(saved).unwrap();
    let mut doc = Document::new(again.presentation.clone());
    doc.presentation_mut().view_zoom = Some(80);
    let saved = again.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    assert!(view(&saved).contains("n=\"80\""), "changed again: {}", view(&saved));
    // The window fitted again (the zoom taken off a file that had one written): `varScale="1"`.
    let again = ibis_formats::open(saved).unwrap();
    let mut doc = Document::new(again.presentation.clone());
    doc.presentation_mut().view_zoom = Some(80);
    doc.presentation_mut().view_zoom = None;
    let fitted = again.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(ibis_formats::read(&fitted).unwrap().view_zoom, None);
}

fn zip_text(bytes: &[u8], name: &str) -> String {
    let package = ibis_ooxml::Package::open(bytes.to_vec()).unwrap();
    String::from_utf8(package.read(&format!("/{name}")).or_else(|_| package.read(name)).unwrap()).unwrap()
}
