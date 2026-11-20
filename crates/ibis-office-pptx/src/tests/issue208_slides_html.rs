// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #208: ファイル > 書き出し > Web ページ writes the shown slides as one HTML page: the menu item, a section for each shown
//! slide, the text of the slides, and nothing for a hidden slide.
use crate::shell::scenario::Scenario;
use ibis_doc_model::{EmuRect, Shape};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue208_menu_item", menu_item as fn()),
        ("pptx::issue208_page_has_the_shown_slides_and_their_text", page_has_the_shown_slides_and_their_text),
        ("pptx::issue208_all_hidden_says_so", all_hidden_says_so),
    ]
}

fn menu_item() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    assert!(s.menu_state("pptx.export-html").is_some(), "File > Export has the Web page item");
}

fn page_has_the_shown_slides_and_their_text() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        e.insert_shape(Shape::text_box(id, EmuRect::new(500_000, 500_000, 3_000_000, 900_000), "Issue 208 <text>"), cx);
    });
    let (html, shown) = s.with(|e, _| {
        let p = e.presentation();
        (crate::export_html::page_for(p, "plan", true).unwrap(), p.slides.iter().filter(|s| !s.hidden).count())
    });
    assert_eq!(html.matches("<section").count(), shown, "{html}");
    assert!(html.contains("Issue 208 &lt;text&gt;"), "the text of the slide is in the page");
    assert!(html.contains("<html lang=\"en\">") && html.contains("<title>plan</title>"));
}

fn all_hidden_says_so() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let message = s.with(|e, _| {
        let mut p = e.presentation().clone();
        for slide in &mut p.slides {
            slide.hidden = true;
        }
        crate::export_html::page_for(&p, "plan", true)
    });
    assert!(message.is_err());
}
