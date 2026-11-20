// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #245: the caret of the text the three formats draw themselves is black over a light background and white over a dark one (the Rec.709 luma
//! of the colour under it), whatever the appearance of the screen (macOS dark mode made the slide's caret white on a white slide).
use super::Scenario;
use crate::docx::ext::*;
use crate::shell::palette;
use ibis_doc_model::{Color, Fill, Rgb};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue245_the_caret_is_black_over_light_and_white_over_dark_by_the_luma", by_luma),
        ("shell::issue245_pptx_the_caret_follows_the_shape_fill_and_the_slide_background_in_dark_mode", pptx),
        ("shell::issue245_docx_the_caret_is_drawn_black_on_a_white_page_and_white_on_a_dark_one_in_dark_mode", docx),
        ("shell::issue245_xlsx_the_caret_follows_the_cell_fill_and_the_bar_in_dark_mode", xlsx),
    ]
}

fn dark(s: &mut Scenario) {
    s.with_app(|app| palette::follow(gpui_kit::WindowAppearance::Dark, None, app));
    assert!(palette::is_dark());
}

fn is(c: gpui_kit::Hsla, hex: u32) -> bool {
    let c: gpui_kit::Rgba = c.into();
    let byte = |v: f32| (v * 255.0).round() as u32;
    (byte(c.r) << 16 | byte(c.g) << 8 | byte(c.b)) == hex
}

fn by_luma() {
    use crate::shell::caret::color_over;
    let over = |hex: u32| color_over(Some(Rgb(hex)), Rgb::WHITE);
    for light in [0xffffff, 0xffffe1, 0x00b050, 0xffff00, 0xd9d9d9, 0x808080] {
        assert!(is(over(light), 0x000000), "{light:06x} is light");
    }
    for dark in [0x000000, 0x0070c0, 0x7f7f7f, 0x1f1f1f, 0x203864, 0xc00000] {
        assert!(is(over(dark), 0xffffff), "{dark:06x} is dark");
    }
    assert_eq!(Rgb(0x00b050).luma(), 132);
    assert_eq!(Rgb(0x0070c0).luma(), 94);
    assert!(is(color_over(None, Rgb::WHITE), 0x000000), "not known: the page's white");
}

fn pptx() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    dark(&mut s);
    s.double_click_shape(0);
    assert!(s.with(|e, _| e.text_editing().is_some()));
    assert!(is(s.with(|e, _| e.text_caret_color()), 0x000000), "a white slide in dark mode: a black caret");
    let (slide, shape) = s.with(|e, _| (e.slide().clone(), e.text_editing().unwrap().shape));
    let color = |s: &mut Scenario, slide: &ibis_doc_model::Slide| s.with(|e, _| crate::shell::editor::SlideEditor::caret_color_in(slide, Some(shape), &e.resources));
    let clear = |slide: &mut ibis_doc_model::Slide| {
        for sh in &mut slide.shapes {
            if let ibis_doc_model::ShapeKind::Auto(a) = &mut sh.kind {
                a.fill = Fill::None;
            }
        }
    };
    let mut black = slide.clone();
    black.background = Fill::Solid(Color::BLACK);
    clear(&mut black);
    assert!(is(color(&mut s, &black), 0xffffff), "a black slide: a white caret");
    // A white shape on the black slide: its own fill decides.
    let mut filled = black.clone();
    for sh in &mut filled.shapes {
        if sh.id == shape && let ibis_doc_model::ShapeKind::Auto(a) = &mut sh.kind {
            a.fill = Fill::Solid(Color::WHITE);
        }
    }
    if filled.shape(shape).is_some_and(|sh| sh.backdrop_rgb().is_some()) {
        assert!(is(color(&mut s, &filled), 0x000000), "a white shape on a black slide: black");
    }
    // No background of its own (a layout's or a master's): the full-slide shape of the backdrop is what is drawn on.
    let mut inherited = slide.clone();
    clear(&mut inherited);
    inherited.background = Fill::None;
    let mut cover = ibis_doc_model::Shape::new(ibis_doc_model::ShapeId(9_999_999), ibis_doc_model::EmuRect { x: 0, y: 0, width: 12_192_000, height: 6_858_000 }, ibis_doc_model::ShapeKind::Auto(ibis_doc_model::AutoShape { fill: Fill::Solid(Color::BLACK), ..first_auto(&slide) }));
    inherited.backdrop = vec![cover.clone()];
    assert!(is(color(&mut s, &inherited), 0xffffff), "a black rectangle of the layout under a slide with no background: white");
    cover = ibis_doc_model::Shape { kind: ibis_doc_model::ShapeKind::Auto(ibis_doc_model::AutoShape { fill: Fill::Solid(Color::WHITE), ..first_auto(&slide) }), ..cover };
    inherited.backdrop = vec![cover];
    assert!(is(color(&mut s, &inherited), 0x000000), "a white one: black");
    inherited.backdrop.clear();
    assert!(is(color(&mut s, &inherited), 0x000000), "nothing: the slide's white");
    // A picture's background: by its average colour.
    let media = s.with(|e, _| e.presentation().slides.iter().flat_map(|sl| sl.shapes.iter()).find_map(|sh| if let ibis_doc_model::ShapeKind::Picture(p) = &sh.kind { Some(p.media) } else { None }));
    if let Some(media) = media {
        s.settle();
        let mean = s.with(|e, _| e.resources.mean_color(media));
        let mean = mean.expect("the picture's average colour");
        let mut picture = slide.clone();
        clear(&mut picture);
        picture.background = Fill::Image(media);
        let expect = if mean.luma() >= 128 { 0x000000 } else { 0xffffff };
        assert!(is(color(&mut s, &picture), expect), "a picture background of average {mean:?}");
    }
    let mut white = slide;
    white.background = Fill::Solid(Color::WHITE);
    assert!(is(color(&mut s, &white), 0x000000));
}

fn first_auto(slide: &ibis_doc_model::Slide) -> ibis_doc_model::AutoShape {
    slide.shapes.iter().find_map(|s| if let ibis_doc_model::ShapeKind::Auto(a) = &s.kind { Some(a.clone()) } else { None }).expect("an auto shape")
}

/// The caret column's darkest and lightest luma in the window picture.
fn caret_column(s: &mut Scenario) -> (u8, u8) {
    let pos = s.with(|e, _| e.doc_state().and_then(|d| d.caret)).expect("a caret");
    let (x, top, bottom) = s.with(|e, _| {
        let (page, x, y, h) = e.caret_box(&e.flow_lines(), pos).expect("laid out");
        let (origin, k) = e.page_origin(page).expect("placed");
        (f32::from(origin.x) + x * k, f32::from(origin.y) + y * k, f32::from(origin.y) + (y + h) * k)
    });
    let path = s.screenshot("docx-issue245-caret");
    let image = image::open(&path).expect("the picture").to_rgb8();
    let scale = image.width() as f32 / 1200.0;
    let (mut lo, mut hi) = (255u8, 0u8);
    for py in ((top + 2.0) * scale) as u32..((bottom - 2.0) * scale) as u32 {
        for px in ((x - 1.0) * scale) as u32..=((x + 1.0) * scale) as u32 {
            let p = image.get_pixel(px, py);
            let l = ibis_doc_model::Rgb::from_rgb(p[0], p[1], p[2]).luma();
            lo = lo.min(l);
            hi = hi.max(l);
        }
    }
    (lo, hi)
}

fn docx() {
    let mut s = Scenario::start_view();
    s.menu("file.new.document");
    dark(&mut s);
    let first = s.with(|e, _| e.flow().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap());
    s.update(|e, _, cx| e.set_caret(ibis_doc_model::FlowPos::new(first, 0), false, cx));
    s.settle();
    let (lo, _) = caret_column(&mut s);
    assert!(lo < 60, "a black caret on the white page in dark mode: darkest {lo}");
    s.update(|e, _, cx| { e.run_flow(vec![ibis_doc_model::FlowCommand::SetPageColor(Some(Rgb(0x000000)))], cx); });
    s.settle();
    let color = s.with(|e, _| e.flow().and_then(|d| d.page_color));
    assert_eq!(color, Some(Rgb(0)));
    let (_, hi) = caret_column(&mut s);
    assert!(hi > 200, "a white caret on a black page: lightest {hi}");
}

fn xlsx() {
    use crate::shell::caret::{color_over, color_over_theme};
    // A cell's fill decides (the box is its fill, white when it has none); the formula bar follows the theme's background.
    assert!(is(color_over(None, Rgb::WHITE), 0x000000));
    assert!(is(color_over(Some(Rgb(0x203864)), Rgb::WHITE), 0xffffff));
    assert!(is(color_over(Some(Rgb(0xffffe1)), Rgb::WHITE), 0x000000), "a note");
    assert!(is(color_over_theme(gpui_kit::rgb(0x1e1e1e).into()), 0xffffff), "a dark bar");
    assert!(is(color_over_theme(gpui_kit::rgb(0xffffff).into()), 0x000000), "a light bar");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    dark(&mut s);
    s.key("f2");
    s.screenshot("xlsx-issue245-cell-edit");
}
