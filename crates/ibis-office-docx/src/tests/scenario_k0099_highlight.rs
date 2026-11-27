// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0099 (b): on the dark page (表示 > ダークモード（ページ）) the text on a yellow highlight keeps its own (dark) colour, the other text is light.
//! Pure yellow's HSL lightness is 0.5, so a lightness test missed it: the screenshot's pixels are measured.
use crate::ext::*;
use crate::shell::scenario::Scenario;
use ibis_doc_model::FlowPos;
use super::scenarios::DocSteps;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::k0099_text_on_a_yellow_highlight_is_dark_on_the_dark_page", highlighted_text_is_dark as fn())]
}

fn highlighted_text_is_dark() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 800.);
    let (_, id, _) = super::scenarios::text_paragraph(&mut s, 12);
    s.click_text(FlowPos::new(id, 1));
    for _ in 0..8 {
        s.key("shift-right");
    }
    s.toolbar_menu("highlight", "docx.highlight.yellow");
    s.key("right");
    s.menu("docx.view.dark-page");
    s.settle();
    let image = image::open(s.screenshot("k0099-highlight")).expect("the screenshot").to_rgba8();
    let (w, h) = image.dimensions();
    let is_yellow = |p: &image::Rgba<u8>| p.0[0] > 240 && p.0[1] > 240 && p.0[2] < 20;
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in (h / 5)..(h - h / 16) {
        for x in 0..w {
            if is_yellow(image.get_pixel(x, y)) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    assert!(x1 > x0 && y1 > y0, "a yellow highlight is drawn");
    // Over the highlight: dark glyph pixels, and no near-white ones.
    let (mut dark, mut white) = (0, 0);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = image.get_pixel(x, y).0;
            if p[0] < 90 && p[1] < 90 && p[2] < 90 {
                dark += 1;
            }
            if p[0] > 200 && p[1] > 200 && p[2] > 200 {
                white += 1;
            }
        }
    }
    assert!(dark >= 100, "the text on the highlight is dark ({dark} dark pixels)");
    assert!(white < dark / 20 + 3, "no white text on the highlight ({white} light pixels, {dark} dark)");
    // The text off the highlight is light on the dark paper.
    let mut light = 0;
    for y in (h / 5)..(h - h / 16) {
        for x in 0..w {
            if (y < y0.saturating_sub(4) || y > y1 + 4) && image.get_pixel(x, y).0.iter().all(|c| *c > 200) {
                light += 1;
            }
        }
    }
    assert!(light >= 50, "the other text is light ({light} pixels)");
}
