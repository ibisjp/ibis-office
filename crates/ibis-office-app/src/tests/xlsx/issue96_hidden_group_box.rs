// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #96: a click on an option button chooses it; the hidden group boxes behind it (VML `visibility:hidden`)
//! take no click, select nothing and open no text editing.

use crate::shell::scenario::Scenario;
use ibis_doc_model::{ControlKind, ShapeId, ShapeKind};

const BOOK: &str = "test/github-issue/96/sim.xlsx";

fn radios(s: &mut Scenario) -> Vec<(ShapeId, String, u8)> {
    s.with(|e, _| e.slide().shapes.iter().filter_map(|sh| match &sh.kind {
        ShapeKind::Control(c) if c.kind == ControlKind::Radio => Some((sh.id, c.text.clone(), c.checked)),
        _ => None,
    }).collect())
}

fn center(s: &mut Scenario, id: ShapeId) -> (f32, f32) {
    s.with(|e, _| {
        let f = e.slide().shape(id).unwrap().frame;
        let l = e.slide_layout().expect("laid out");
        (f32::from(l.origin.x + l.to_px(f.x + f.width / 2)), f32::from(l.origin.y + l.to_px(f.y + f.height / 2)))
    })
}

pub(super) fn hidden_group_box_takes_no_click() {
    let path = crate::shell::scenario::repo_root().join(BOOK);
    if !path.exists() { eprintln!("skipped: {BOOK} is not in this checkout"); return; }
    let mut s = Scenario::open(BOOK);
    let target = s.with(|e, _| e.presentation().slides.iter().find(|sl| sl.name.as_deref() == Some("時刻入力シート")).map(|sl| sl.id)).expect("the sheet"); // i18n-ok（試験・ファイルの中の日本語のデータ）
    s.update(move |e, w, cx| e.select_slide(target, w, cx));
    s.settle();
    let hidden = s.with(|e, _| e.slide().hidden_shapes.len());
    assert!(hidden >= 3, "the hidden group boxes are read as hidden: {hidden}");
    let all = radios(&mut s);
    assert!(all.len() >= 4, "option buttons: {all:?}");
    for (id, name, _) in all.clone() {
        let at = center(&mut s, id);
        s.click(at);
        s.settle();
        let now = radios(&mut s);
        assert_eq!(now.iter().find(|r| r.0 == id).unwrap().2, 1, "{name} is chosen");
        assert!(s.with(|e, _| e.text_editing().is_none()), "{name}: no text editing");
        let picked_hidden = s.with(|e, _| e.selected_ids().iter().any(|x| e.slide().hidden_shapes.contains(x)));
        assert!(!picked_hidden, "{name}: a hidden shape is not selected");
    }
}
