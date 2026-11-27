// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #245 (reopened 2026-11-25): the caret the three formats draw themselves can be seen over what is really under it (design:
//! `tasks/doc/test-design/issue-245.md`; the numbers D1... are that document's). The 0.8.9 fix tests the colour the code picks
//! (`issue245_caret.rs`); these read the window's pixels: the caret's pixel (where the editor says the caret is) against the colour most of the
//! surrounding box has (the background under the text), as a WCAG contrast ratio (3:1, WCAG 1.4.11 non-text contrast). The caret blinks on the
//! real clock: a caret that is out of its visible half is a background-coloured pixel, so each case looks three times (moving the caret in between
//! starts a new visible half) and takes the best; a caret that is the colour of its background is 1:1 every time.
//! The reopened report: with 表示 > ダークモード（ページ） the page is dark and the caret stays black.
use super::Scenario;
use crate::docx::ext::*;
use crate::docx::scenarios::DocSteps;
use crate::shell::palette;
use crate::xlsx::ext::*;
use crate::xlsx::scenarios::SheetSteps;
use ibis_doc_model::{Block, Color, Fill, FlowPos, Rgb};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue245_d1_docx_page_light_and_dark_page_in_both_app_appearances", d1_page),
        ("shell::issue245_d2_docx_page_colour_with_and_without_dark_page", d2_page_colour),
        ("shell::issue245_d3_docx_table_cell_shading", d3_table_cell),
        ("shell::issue245_d4_docx_paragraph_shading", d4_paragraph),
        ("shell::issue245_d5_docx_text_box", d5_text_box),
        ("shell::issue245_p1_pptx_slide_background_and_shape_fill", p1_pptx),
        ("shell::issue245_x1_xlsx_cell_fill", x1_xlsx),
    ]
}

/// The smallest contrast ratio that counts as visible (WCAG 2.1 success criterion 1.4.11, non-text contrast: 3:1).
const MIN_CONTRAST: f64 = 3.0;

fn app_appearance(s: &mut Scenario, dark: bool) {
    let a = if dark { gpui_kit::WindowAppearance::Dark } else { gpui_kit::WindowAppearance::Light };
    s.with_app(|app| palette::follow(a, None, app));
    assert_eq!(palette::is_dark(), dark);
}

fn luminance(p: [u8; 3]) -> f64 {
    let c = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * c(p[0]) + 0.7152 * c(p[1]) + 0.0722 * c(p[2])
}

fn ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// A caret place in points: the strip to read the caret from (x0, x1, y0, y1) and the box whose commonest colour is the background.
#[derive(Clone, Copy)]
struct Place {
    strip: (f32, f32, f32, f32),
    around: (f32, f32, f32, f32),
}

/// The best contrast between the pixels of the strip and the background (the box's commonest colour) over three looks; `nudge` starts a new visible half.
fn measure(s: &mut Scenario, place: impl Fn(&mut Scenario) -> Place, nudge: impl Fn(&mut Scenario)) -> (f64, [u8; 3], [u8; 3]) {
    let width = s.update(|_, w, _| f32::from(w.viewport_size().width));
    let mut best = (0.0f64, [0u8; 3], [0u8; 3]);
    for _ in 0..3 {
        nudge(s);
        let place = place(s);
        let (w, img) = s.picture();
        let scale = w as f32 / width;
        let at = |x: f32, y: f32| {
            let i = (((y * scale) as u32 * w + (x * scale) as u32) * 4) as usize;
            [img[i], img[i + 1], img[i + 2]]
        };
        let mut counts = std::collections::HashMap::new();
        let (ax0, ax1, ay0, ay1) = place.around;
        let mut y = ay0;
        while y < ay1 {
            let mut x = ax0;
            while x < ax1 {
                *counts.entry(at(x, y)).or_insert(0usize) += 1;
                x += 1.0 / scale.max(1.0);
            }
            y += 1.0 / scale.max(1.0);
        }
        let bg = counts.into_iter().max_by_key(|(_, n)| *n).map(|(c, _)| c).unwrap_or([255, 255, 255]);
        let (sx0, sx1, sy0, sy1) = place.strip;
        let mut found = (0.0f64, bg, bg);
        let mut y = sy0;
        while y <= sy1 {
            let mut x = sx0;
            while x <= sx1 {
                let p = at(x, y);
                let r = ratio(p, bg);
                if r > found.0 {
                    found = (r, p, bg);
                }
                x += 1.0 / scale.max(1.0);
            }
            y += 1.0 / scale.max(1.0);
        }
        if found.0 > best.0 {
            best = found;
        }
        if best.0 >= MIN_CONTRAST {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(520));
    }
    best
}

fn check(label: &str, s: &mut Scenario, failures: &mut Vec<String>, place: impl Fn(&mut Scenario) -> Place, nudge: impl Fn(&mut Scenario)) {
    let (r, caret, bg) = measure(s, place, nudge);
    if r < MIN_CONTRAST {
        failures.push(format!("{label}: contrast {r:.2}:1 (the most different pixel of the caret's strip is {caret:?}, the background {bg:?})"));
    }
}

fn report(what: &str, failures: Vec<String>) {
    assert!(failures.is_empty(), "{what}: the caret is hard to see in {} case(s):\n{}", failures.len(), failures.join("\n"));
}

fn dark_page(s: &mut Scenario, want: bool) {
    if s.with(|e, _| e.resources.dark_page) != want {
        s.menu("docx.view.dark-page");
    }
    assert_eq!(s.with(|e, _| e.resources.dark_page), want);
}

/// A new document with some text and the caret at its end.
fn new_doc() -> Scenario {
    let mut s = Scenario::start_view();
    s.menu("file.new.document");
    s.size(1200., 900.);
    let first = s.with(|e, _| e.flow().unwrap().body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap());
    s.update(|e, _, cx| e.set_caret(FlowPos::new(first, 0), false, cx));
    s.type_text("TEST Cursor");
    s.update(|e, _, cx| e.set_caret(FlowPos::new(first, 0), false, cx));
    s
}


/// The place of the document's caret (not in a text box), in points.
fn docx_place(s: &mut Scenario) -> Place {
    let pos = s.with(|e, _| e.doc_state().and_then(|d| d.caret)).expect("a caret");
    let (x, top, bottom) = s.with(|e, _| {
        let (page, x, y, h) = e.caret_box(&e.flow_lines(), pos).expect("laid out");
        let (origin, k) = e.page_origin(page).expect("placed");
        (f32::from(origin.x) + x * k, f32::from(origin.y) + y * k, f32::from(origin.y) + (y + h) * k)
    });
    Place { strip: (x - 0.2, x + 0.4, top + 2.0, bottom - 2.0), around: (x - 3.0, x + 40.0, top, bottom) }
}

/// A new visible half of the blink: the caret goes one character on and back.
fn docx_nudge(s: &mut Scenario) {
    let pos = s.with(|e, _| e.doc_state().and_then(|d| d.caret)).expect("a caret");
    s.update(|e, _, cx| e.set_caret(FlowPos::new(pos.para, pos.offset + 1), false, cx));
    s.update(|e, _, cx| e.set_caret(pos, false, cx));
}

fn pptx_place(s: &mut Scenario) -> Place {
    let b = s.with(|e, _| e.caret_bounds()).expect("the caret has a place");
    let (x, y, w, h) = (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height));
    Place { strip: (x - 0.2, x + w + 0.4, y + 2.0, y + h - 2.0), around: (x - 4.0, x + 30.0, y, y + h) }
}

fn keys_nudge(s: &mut Scenario) {
    s.key("right");
    s.key("left");
}

/// D1: a white page, and the page 表示 > ダークモード（ページ） makes dark, in the light and the dark app (the reopened report).
fn d1_page() {
    let mut f = Vec::new();
    for app_dark in [false, true] {
        for page_dark in [false, true] {
            let mut s = new_doc();
            app_appearance(&mut s, app_dark);
            dark_page(&mut s, page_dark);
            check(&format!("docx, white page, dark page {page_dark}, dark app {app_dark}"), &mut s, &mut f, docx_place, docx_nudge);
        }
    }
    report("D1", f);
}

/// D2: the page's own colour (a black page, a light yellow one) with the dark page off and on: what is drawn is the dark paper when it is on.
fn d2_page_colour() {
    let mut f = Vec::new();
    for (name, color) in [("black", 0x000000), ("light yellow", 0xffffcc), ("dark blue", 0x203864)] {
        for page_dark in [false, true] {
            let mut s = new_doc();
            s.update(|e, _, cx| e.run_flow(vec![ibis_doc_model::FlowCommand::SetPageColor(Some(Rgb(color)))], cx));
            dark_page(&mut s, page_dark);
            check(&format!("docx, {name} page colour, dark page {page_dark}"), &mut s, &mut f, docx_place, docx_nudge);
        }
    }
    report("D2", f);
}

fn caret_in(s: &mut Scenario, para_of: impl Fn(&ibis_doc_model::FlowDocument) -> Option<u32>) {
    s.size(1200., 900.);
    let id = s.with(|e, _| para_of(e.flow().unwrap())).expect("a paragraph");
    s.update(|e, _, cx| e.set_caret(FlowPos::new(id, 0), false, cx));
    s.settle();
}

fn first_cell_paragraph(d: &ibis_doc_model::FlowDocument) -> Option<u32> {
    d.body.iter().find_map(|b| if let Block::Table(t) = b { t.rows[0].cells[0].blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }) } else { None })
}

fn first_paragraph(d: &ibis_doc_model::FlowDocument) -> Option<u32> {
    d.body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None })
}

/// D3: the text in a table cell that has a dark or a light shading, on the white page and the dark page.
fn d3_table_cell() {
    let mut f = Vec::new();
    for (name, file) in [("dark shading", "docx-table-cell-dark"), ("light shading", "docx-table-cell-light")] {
        for page_dark in [false, true] {
            let mut s = Scenario::open(&format!("test/editing/caret245/{file}.docx"));
            caret_in(&mut s, first_cell_paragraph);
            dark_page(&mut s, page_dark);
            check(&format!("docx table cell, {name}, dark page {page_dark}"), &mut s, &mut f, docx_place, docx_nudge);
        }
    }
    report("D3", f);
}

/// D4: the text of a paragraph that has a dark or a light shading, on the white page and the dark page.
fn d4_paragraph() {
    let mut f = Vec::new();
    for (name, file) in [("dark shading", "docx-paragraph-dark"), ("light shading", "docx-paragraph-light")] {
        for page_dark in [false, true] {
            let mut s = Scenario::open(&format!("test/editing/caret245/{file}.docx"));
            caret_in(&mut s, first_paragraph);
            dark_page(&mut s, page_dark);
            check(&format!("docx paragraph, {name}, dark page {page_dark}"), &mut s, &mut f, docx_place, docx_nudge);
        }
    }
    report("D4", f);
}

/// D5: the text in a text box that has no fill, on the white page and the dark page.
fn d5_text_box() {
    let mut f = Vec::new();
    for page_dark in [false, true] {
        let mut s = Scenario::start_view();
        s.menu("file.new.document");
        s.size(1200., 900.);
        dark_page(&mut s, page_dark);
        s.menu("insert.text-box");
        assert!(s.with(|e, _| e.doc_state().unwrap().box_edit.is_some()), "the caret is in the text box");
        let place = |s: &mut Scenario| {
            let (x, y, w) = s.with(|e, _| {
                let edit = e.doc_state().unwrap().box_edit.as_ref().unwrap();
                let (page, (x, y, w, _), _) = e.floating_text_box_frame_of(edit.at).unwrap();
                let (o, k) = e.page_origin(page).unwrap();
                (f32::from(o.x) + x * k, f32::from(o.y) + y * k, w * k)
            });
            // The empty box's first line, inside its border.
            Place { strip: (x + 4.0, x + w * 0.5, y + 4.0, y + 18.0), around: (x + 4.0, x + w * 0.5, y + 4.0, y + 18.0) }
        };
        check(&format!("docx text box without fill, dark page {page_dark}"), &mut s, &mut f, place, |s| {
            s.type_text("x");
            s.key("backspace");
        });
    }
    report("D5", f);
}

/// P1: a slide's text in a shape: the slide white or black, the shape without a fill or with a dark or a light one, in the light and dark app.
fn p1_pptx() {
    let mut f = Vec::new();
    for app_dark in [false, true] {
        for (name, background, fill) in [
            ("white slide, no fill", Color::WHITE, Fill::None),
            ("black slide, no fill", Color::BLACK, Fill::None),
            ("white slide, dark fill", Color::WHITE, Fill::Solid(Color::BLACK)),
            ("black slide, light fill", Color::BLACK, Fill::Solid(Color::WHITE)),
        ] {
            let mut s = Scenario::open("test/editing/pptx-plan.pptx");
            app_appearance(&mut s, app_dark);
            let (slide, shape) = s.with(|e, _| (e.slide().id, e.slide().shapes[0].id));
            s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetBackground { slide, fill: Fill::Solid(background) }, ibis_doc_model::Command::SetFill { slide, shape, fill }], cx));
            s.double_click_shape(0);
            assert!(s.with(|e, _| e.text_editing().is_some()), "the text is being edited");
            check(&format!("pptx {name}, dark app {app_dark}"), &mut s, &mut f, pptx_place, keys_nudge);
        }
    }
    report("P1", f);
}

/// X1: a cell being edited: no fill, a dark fill, a light fill, in the light and dark app.
fn x1_xlsx() {
    use ibis_doc_model::{CellRange, Command, SheetCommand, StyleChange};
    let mut f = Vec::new();
    for app_dark in [false, true] {
        for (name, fill) in [("no fill", None), ("dark fill", Some(Rgb(0x203864))), ("light fill", Some(Rgb(0xffff99)))] {
            let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
            s.size(1200., 900.);
            app_appearance(&mut s, app_dark);
            if let Some(fill) = fill {
                let sheet = s.with(|e, _| e.slide().id);
                let range = CellRange { first_row: 1, first_column: 1, last_row: 1, last_column: 1 };
                s.update(|e, _, cx| e.run_sheet(vec![Command::Sheet(SheetCommand::SetFormat { sheet, ranges: vec![range], change: StyleChange::Fill(Some(fill)) })], cx));
            }
            s.click_cell("B2");
            s.key("delete");
            s.key("f2");
            let place = |s: &mut Scenario| {
                let (b, c, row) = (s.header("B"), s.header("C"), s.header("2"));
                let (x, y, half) = (b.0, row.1, (c.0 - b.0) / 2.0);
                // The left end of the empty cell being edited, inside its edge (the caret is the only thing in it).
                Place { strip: (x - half + 2.0, x - half + 7.0, y - 4.0, y + 4.0), around: (x - half + 2.0, x + half - 4.0, y - 6.0, y + 6.0) }
            };
            check(&format!("xlsx cell, {name}, dark app {app_dark}"), &mut s, &mut f, place, |s| {
                s.key("escape");
                s.key("f2");
            });
        }
    }
    report("X1", f);
}
