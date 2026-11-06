// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pages of the print export (outline, notes pages, handouts) drawn over a headless window. GPUI's macOS platform can be created only
//! on the main thread, so macOS and Linux run them with the scenario runner (`--test scenarios -- pptx::export_print`); on Windows they
//! are `#[test]`s of `cargo test --lib`.

use super::*;

/// The checks, run by the scenario runner.
#[cfg(not(windows))]
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::export_print::the_outline_is_text_on_pages_of_its_own", the_outline_is_text_on_pages_of_its_own),
        ("pptx::export_print::notes_pages_have_the_slide_above_and_its_notes_below", notes_pages_have_the_slide_above_and_its_notes_below),
        ("pptx::export_print::handout_pages", handout_pages),
        ("pptx::export_print::marks_go_where_the_handout_master_puts_them", marks_go_where_the_handout_master_puts_them),
    ]
}

/// The layer the export draws text with, over a headless window (the same on every platform).
fn with_layer<R>(f: impl FnOnce(&mut dyn TextLayer) -> R) -> R {
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let presentation = ibis_doc_model::Presentation::new();
    let resources = std::rc::Rc::new(cx.update(|app| crate::render::shape_view::Resources::new(&presentation, app)));
    let density = crate::export::pixel_density(&mut cx).expect("a density");
    f(&mut crate::export::SlideTextLayer { cx: &mut cx, resources, density })
}

fn solid(w: u32, h: u32, rgb: [u8; 3]) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba([rgb[0], rgb[1], rgb[2], 255]))
}

/// アウトライン: the titles and bodies as dark text from the top of an A4 page, on more pages when they do not fit.
#[cfg_attr(windows, test)]
fn the_outline_is_text_on_pages_of_its_own() {
    let mut paragraphs = Vec::new();
    for i in 1..=120 {
        paragraphs.push(NotePara { level: 0, bullet: None, runs: vec![NoteRun { text: format!("{i}  Title {i}"), bold: true, size_pt: Some(14.0), ..Default::default() }] });
        paragraphs.push(NotePara { level: 1, bullet: Some("•".into()), runs: vec![NoteRun { text: format!("Point of slide {i}"), size_pt: Some(12.0), ..Default::default() }] });
    }
    let pages = with_layer(|layer| outline_pages(&NoteText { paragraphs }, &HandoutMarks::default(), layer));
    assert!(pages.len() >= 3, "{}", pages.len());
    for (page, size) in &pages {
        assert_eq!(*size, HANDOUT_PAGE);
        assert!(page.pixels().filter(|q| q[0] < 100 && q[1] < 100 && q[2] < 100).count() > 300, "text on every page");
    }
}

/// ノート ページ: the slide on top, framed, with the notes' text below it.
#[cfg_attr(windows, test)]
fn notes_pages_have_the_slide_above_and_its_notes_below() {
    let slides = vec![solid(160, 90, [200, 0, 0]), solid(160, 90, [0, 0, 200]), solid(160, 90, [0, 200, 0])];
    let long: String = (1..=90).map(|i| format!("Line {i} of very long notes")).collect::<Vec<_>>().join("\n");
    let notes = vec![NoteText::plain("Speaker notes for the first slide.\n二行目のメモ"), NoteText::default(), NoteText::plain(&long)]; // i18n-ok（試験・ファイルの中の日本語のデータ）
    let pages = with_layer(|layer| notes_pages(&slides, &notes, &HandoutMarks::default(), layer));
    // The third slide's notes take more pages, with nothing lost: text on each, and no slide on the later ones.
    assert!(pages.len() >= 5, "{}", pages.len());
    let green = |i: usize| pages[i].0.pixels().filter(|p| p[1] == 200 && p[0] == 0).count();
    assert!(green(2) > 10_000 && green(3) == 0);
    assert!(pages[3..].iter().all(|(p, _)| p.pixels().filter(|q| q[0] < 100 && q[1] < 100 && q[2] < 100).count() > 300));
    let (page, size) = &pages[0];
    assert_eq!(*size, HANDOUT_PAGE);
    let (w, h) = (page.width(), page.height());
    // The slide's colour in the upper half only; dark text in the lower half.
    let red = |y0: u32, y1: u32| (y0..y1).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| page.get_pixel(x, y)[0] == 200 && page.get_pixel(x, y)[2] == 0).count();
    // The slide ends 627 px down (60 pt and 253.7 pt, 2 px a point).
    assert!(red(0, 640) > 10_000 && red(640, h) == 0, "the slide above");
    let dark = |y0: u32, y1: u32| (y0..y1).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| page.get_pixel(x, y).0[..3].iter().all(|c| *c < 100)).count();
    assert!(dark(640, h) > 300, "the notes below");
    // A slide without notes has nothing below it (but the page's blank).
    assert_eq!(pages[1].0.pixels().filter(|p| p[0] < 100 && p[1] < 100 && p[2] > 150).count() > 10_000, true);
    let below = (640..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| pages[1].0.get_pixel(x, y)[0] < 100).count();
    assert_eq!(below, 0);
}

#[cfg_attr(windows, test)]
fn handout_pages() {
    let slides: Vec<RgbaImage> = (0..7).map(|_| solid(160, 90, [200, 0, 0])).collect();
    // 7 slides, 3 to a page: 3 pages (3, 3, 1).
    let pages = with_layer(|layer| handouts(&slides, 3, true, false, &HandoutMarks::default(), layer));
    assert_eq!(pages.len(), 3);
    let (page, size) = &pages[0];
    assert_eq!(*size, HANDOUT_PAGE);
    assert_eq!((page.width(), page.height()), (1190, 1684));
    // The page is white at its corner, and has the slides' colour (or the frame) inside.
    assert_eq!(page.get_pixel(0, 0), &Rgba([255, 255, 255, 255]));
    assert!(page.pixels().any(|p| p[0] == 200 && p[1] == 0));
    assert!(page.pixels().any(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
    // Four to a page: 2 pages, and the second holds three slides (of 4).
    let four = with_layer(|layer| handouts(&slides, 4, false, false, &HandoutMarks::default(), layer));
    assert_eq!(four.len(), 2);
    let second = &four[1].0;
    let red = second.pixels().filter(|p| p[0] == 200 && p[1] == 0).count();
    let first_red = four[0].0.pixels().filter(|p| p[0] == 200 && p[1] == 0).count();
    assert!(red * 4 > first_red * 2 && red * 4 < first_red * 4, "three slides, not four: {red} of {first_red}");
    assert_eq!(handout_grid(6, false), (2, 3));
    assert_eq!(handout_grid(9, false), (3, 3));
    // On its side.
    assert_eq!(handout_grid(6, true), (3, 2));
    let wide = with_layer(|layer| handouts(&slides, 6, false, true, &HandoutMarks::default(), layer));
    assert_eq!((wide[0].0.width(), wide[0].0.height()), (1684, 1190));
    assert_eq!(wide[0].1, (842.0, 595.0));
    assert_eq!(wide.len(), 2);
}

/// The handout master's places: the header where the master puts it, big, the footer gone.
#[cfg_attr(windows, test)]
fn marks_go_where_the_handout_master_puts_them() {
    use ibis_pptx::handout_master::{HandoutLayout, Mark, Place};
    let slides = vec![solid(160, 90, [255, 255, 255])];
    let layout = HandoutLayout {
        page: (6_858_000, 9_144_000),
        // The header in the middle of the page, 24 pt, centred; the date bottom right.
        places: vec![
            Place { mark: Mark::Header, frame: (1_714_500, 4_000_000, 3_429_000, 600_000), size_pt: 24.0, align: "ctr".into(), bottom: false },
            Place { mark: Mark::Date, frame: (3_884_613, 8_685_213, 2_971_800, 458_787), size_pt: 12.0, align: "r".into(), bottom: true },
        ],
    };
    let marks = HandoutMarks { header: Some("Header".into()), date: Some("2026/10/11".into()), footer: Some("Footer".into()), number: true, layout: Some(layout) };
    let pages = with_layer(|layer| handouts(&slides, 6, false, false, &marks, layer));
    let page = &pages[0].0;
    let dark = |x0: u32, y0: u32, x1: u32, y1: u32| (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| page.get_pixel(x, y)[0] < 128).count();
    let (w, h) = (page.width(), page.height());
    // The header: in the middle band (4.0M to 4.6M of 9.144M EMU), across the middle of the page.
    let band = (h as f32 * 4_000_000.0 / 9_144_000.0) as u32..(h as f32 * 4_700_000.0 / 9_144_000.0) as u32;
    assert!(dark(w / 3, band.start, w * 2 / 3, band.end) > 100, "the header in its place");
    // Not in the corners it used to be in, and no footer (the master has none): the bottom left is clean.
    assert_eq!(dark(0, h * 9 / 10, w / 2, h), 0, "no footer");
    // The date: bottom right.
    assert!(dark(w / 2, h * 9 / 10, w, h) > 50, "the date at the bottom right");
}
