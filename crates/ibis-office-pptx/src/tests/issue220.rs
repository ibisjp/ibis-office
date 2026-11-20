// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! issue #220: a slide added after a title slide follows the 2nd layout of its master by position (as PowerPoint's 新しいスライド does); after any other
//! slide it follows that slide's layout. Every way of adding a slide (menu, ⌘M / Ctrl+M, Enter in the slide list) goes the same way.
use crate::shell::scenario::Scenario;
use ibis_doc_model::LayoutId;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue220_a_slide_after_a_title_slide_follows_the_second_layout", after_title_slide),
        ("pptx::issue220_a_slide_after_another_layout_follows_it", after_other_layout),
    ]
}

fn layout_with(s: &mut Scenario, ctr_title: bool) -> LayoutId {
    s.with(|e, _| {
        let p = e.presentation();
        let l = p.layouts.iter().find(|l| l.placeholders.iter().any(|x| x.placeholder.kind.as_deref() == Some("ctrTitle")) == ctr_title && !l.placeholders.is_empty());
        l.expect("a layout").id
    })
}

fn name_of(s: &mut Scenario, id: Option<LayoutId>) -> String {
    s.with(|e, _| e.presentation().layout(id.expect("laid out")).map(|l| l.name.clone()).unwrap())
}

fn after_title_slide() {
    for how in ["menu", "key", "list-enter"] {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        let title = layout_with(&mut s, true);
        s.update(|e, w, cx| e.apply_layout(title, w, cx));
        match how {
            "menu" => s.menu("pptx.new-slide"),
            "key" => s.key("ctrl-m"),
            _ => s.update(|e, w, cx| e.add_slide(w, cx)),
        }
        let new = s.with(|e, _| e.slide().layout);
        let name = name_of(&mut s, new);
        eprintln!("{how}: {name}");
        assert_ne!(new, Some(title), "{how}: not the title layout again");
        // The 2nd layout of the title layout's master, by position (Microsoft PowerPoint, measured).
        let second = s.with(|e, _| {
            let p = e.presentation();
            let master = p.layout(title).unwrap().master;
            p.layouts.iter().filter(|l| l.master == master).nth(1).map(|l| l.id)
        });
        assert_eq!(new, second, "{how}: {name}");
        // The same layout is taken again by the slide after that one.
        s.menu("pptx.new-slide");
        assert_eq!(s.with(|e, _| e.slide().layout), new, "{how}");
    }
}

fn after_other_layout() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let other = layout_with(&mut s, false);
    s.update(|e, w, cx| e.apply_layout(other, w, cx));
    s.menu("pptx.new-slide");
    assert_eq!(s.with(|e, _| e.slide().layout), Some(other));
}
