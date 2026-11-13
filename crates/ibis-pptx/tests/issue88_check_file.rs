// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #88: the user's 150-slide check file (`test/github-issue/88/transition-check.pptx`, not in the repository): every slide's
//! effect, option and duration are read, and written back as the attributes the file has.
use ibis_doc_model::Transition;
use ibis_pptx::SaveOptions;
use std::collections::BTreeMap;

fn file() -> Option<Vec<u8>> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/github-issue/88/transition-check.pptx")).ok()
}

/// Per slide (in show order): the effect element's name and its attributes, as written in `mc:Choice` (or plain).
fn effects(bytes: &[u8]) -> Vec<(String, BTreeMap<String, String>)> {
    let package = ibis_ooxml::Package::open(bytes.to_vec()).unwrap();
    let mut out = Vec::new();
    for n in 1..=150 {
        let text = String::from_utf8(package.xml(&format!("ppt/slides/slide{n}.xml")).unwrap().to_bytes()).unwrap();
        let doc = roxmltree::Document::parse(&text).unwrap();
        let t = doc.descendants().find(|e| e.has_tag_name(("http://schemas.openxmlformats.org/presentationml/2006/main", "transition"))).and_then(|t| t.children().find(|c| c.is_element() && c.tag_name().name() != "sndAc"));
        let Some(e) = t else {
            out.push(("none".to_string(), BTreeMap::new()));
            continue;
        };
        let attrs = e.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect();
        out.push((e.tag_name().name().to_string(), attrs));
    }
    out
}

#[test]
fn the_check_file_keeps_every_effect_and_option_when_written() {
    let Some(bytes) = file() else { return };
    let opened = ibis_pptx::open(bytes.clone()).unwrap();
    let original: Vec<Option<Transition>> = opened.presentation.slides.iter().map(|s| s.transition.clone()).collect();
    // First every slide a different effect, then the read ones back: the writer makes the elements.
    let mut step = opened.presentation.clone();
    for s in &mut step.slides {
        s.transition = Some(Transition::new("cut"));
    }
    let saved = opened.source.save(&step, SaveOptions::default()).unwrap();
    let reopened = ibis_pptx::open(saved).unwrap();
    let mut back = reopened.presentation.clone();
    for (s, t) in back.slides.iter_mut().zip(&original) {
        s.transition = t.clone();
    }
    let saved = reopened.source.save(&back, SaveOptions::default()).unwrap();
    let now = ibis_pptx::read(&saved).unwrap();
    let mut bad = Vec::new();
    for (i, (s, t)) in now.slides.iter().zip(&original).enumerate() {
        if &s.transition != t {
            bad.push(format!("slide {} read back {:?} not {:?}", i + 1, s.transition, t));
        }
    }
    // The written elements are the same as the file's (default attributes may be left out).
    let (a, b) = (effects(&bytes), effects_in_order(&saved));
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        if x != y {
            bad.push(format!("slide {}: file {:?} written {:?}", i + 1, x, y));
        }
    }
    assert!(bad.is_empty(), "{}\n{}", bad.len(), bad.join("\n"));
}

fn effects_in_order(bytes: &[u8]) -> Vec<(String, BTreeMap<String, String>)> {
    effects(bytes)
}
