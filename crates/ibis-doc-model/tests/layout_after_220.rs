// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #220: a slide added after a title slide follows the 2nd layout of the same master, by position (measured on Microsoft PowerPoint:
//! renaming does not change it, moving or deleting layouts does, with two masters each master's own 2nd); after any other slide, the same layout.

use ibis_doc_model::{Layout, LayoutId, LayoutPlaceholder, MasterId, PlaceholderRef, Presentation};

fn layout(id: u64, master: u64, name: &str, kinds: &[&str]) -> Layout {
    Layout {
        id: LayoutId(id),
        name: name.into(),
        background: Default::default(),
        backdrop: Vec::new(),
        placeholders: kinds.iter().map(|k| LayoutPlaceholder { placeholder: PlaceholderRef { kind: Some((*k).into()), index: None }, placement: None, prompt: None }).collect(),
        master_placeholders: Vec::new(),
        copy_of: None,
        master: Some(MasterId(master)),
        guides: Vec::new(),
        master_guides: Vec::new(),
    }
}

fn pres(layouts: Vec<Layout>) -> Presentation {
    let mut p = Presentation::default();
    p.layouts = layouts;
    p
}

#[test]
fn after_a_title_slide_the_second_layout_of_the_master_by_position() {
    let title = layout(1, 1, "T", &["ctrTitle", "subTitle"]);
    let content = layout(2, 1, "C", &["title", "obj"]);
    let section = layout(3, 1, "S", &["title", "body"]);
    let p = pres(vec![title.clone(), content.clone(), section.clone()]);
    assert_eq!(p.layout_after(LayoutId(1)), LayoutId(2));
    // The name does not matter.
    let mut renamed = content.clone();
    renamed.name = "Foo".into();
    assert_eq!(pres(vec![title.clone(), renamed, section.clone()]).layout_after(LayoutId(1)), LayoutId(2));
    // Moved to the end: the new 2nd, though the moved one has a title and a content placeholder.
    assert_eq!(pres(vec![title.clone(), section.clone(), content.clone()]).layout_after(LayoutId(1)), LayoutId(3));
    // Deleted: the new 2nd.
    assert_eq!(pres(vec![title.clone(), section.clone()]).layout_after(LayoutId(1)), LayoutId(3));
    // No 2nd: the same layout.
    assert_eq!(pres(vec![title.clone()]).layout_after(LayoutId(1)), LayoutId(1));
}

#[test]
fn each_master_has_its_own_second_layout_and_other_slides_keep_theirs() {
    let t1 = layout(1, 1, "T", &["ctrTitle"]);
    let c1 = layout(2, 1, "C", &["title", "obj"]);
    let t2 = layout(3, 2, "T", &["ctrTitle"]);
    let s2 = layout(4, 2, "S", &["title", "body"]);
    let c2 = layout(5, 2, "C", &["title", "obj"]);
    let p = pres(vec![t1, c1, t2, s2, c2]);
    assert_eq!(p.layout_after(LayoutId(3)), LayoutId(4));
    assert_eq!(p.layout_after(LayoutId(1)), LayoutId(2));
    assert_eq!(p.layout_after(LayoutId(5)), LayoutId(5));
    assert_eq!(p.layout_after(LayoutId(4)), LayoutId(4));
}
