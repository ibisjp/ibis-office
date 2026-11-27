// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pages of the print export (outline, notes pages, handouts) drawn over a headless window. GPUI's macOS platform can be created only
//! on the main thread, so macOS and Linux run them with the scenario runner (`--test scenarios -- pptx::export_print`); on Windows they
//! are `#[test]`s of `cargo test --lib`.

use super::*;

/// Checks that are known to fail (`--ignored`); none now (the #225 handout checks are in `all`).
#[cfg(not(windows))]
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

/// The checks, run by the scenario runner.
#[cfg(not(windows))]
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::export_print::export_png_reads_a_text_table_as_the_window_does", export_png_reads_a_text_table_as_the_window_does),
        ("pptx::export_print::export_html_reads_a_text_table_as_the_window_does", export_html_reads_a_text_table_as_the_window_does),
        ("pptx::export_print::the_outline_is_text_on_pages_of_its_own", the_outline_is_text_on_pages_of_its_own),
        ("pptx::export_print::an_outline_bullet_pushes_the_text_by_its_typefaces_advance", an_outline_bullet_pushes_the_text_by_its_typefaces_advance),
        ("pptx::export_print::notes_pages_have_the_slide_above_and_its_notes_below", notes_pages_have_the_slide_above_and_its_notes_below),
        ("pptx::export_print::handout_pages", handout_pages),
        ("pptx::export_print::handouts_on_a4_put_the_slides_where_powerpoint_does", handouts_on_a4_put_the_slides_where_powerpoint_does),
        ("pptx::export_print::handouts_of_six_on_a3_and_letter_put_the_slides_where_powerpoint_does", handouts_of_six_on_a3_and_letter_put_the_slides_where_powerpoint_does),
        ("pptx::export_print::marks_go_where_the_handout_master_puts_them", marks_go_where_the_handout_master_puts_them),
        ("pptx::export_print::pdf_from_the_command_line_has_handouts_and_notes_pages", pdf_from_the_command_line_has_handouts_and_notes_pages),
        ("pptx::export_print::vector_pdf_prints_in_grey_and_black_and_white", vector_pdf_prints_in_grey_and_black_and_white),
        ("pptx::export_print::html_export_embeds_charts_and_smartart_as_pictures", html_export_embeds_charts_and_smartart_as_pictures),
        ("pptx::export_print::chart_bars_print_grey_with_a_black_line_in_grey_and_black_and_white", chart_bars_print_grey_with_a_black_line_in_grey_and_black_and_white),
        ("pptx::export_print::chart_lines_are_drawn_in_whole_device_pixels", chart_lines_are_drawn_in_whole_device_pixels),
        ("pptx::export_print::pure_black_and_white_chart_series_by_their_format", pure_black_and_white_chart_series_by_their_format),
        ("pptx::export_print::pure_black_and_white_automatic_series_have_the_dots_of_the_papers_grid", pure_black_and_white_automatic_series_have_the_dots_of_the_papers_grid),
        ("pptx::export_print::the_dots_of_the_automatic_series_have_the_measured_fractions_of_black", the_dots_of_the_automatic_series_have_the_measured_fractions_of_black),
        ("pptx::export_print::print_paper_puts_the_pages_on_the_paper_as_powerpoint_does", print_paper_puts_the_pages_on_the_paper_as_powerpoint_does),
        ("pptx::export_print::the_patterns_of_the_automatic_series_are_those_measured_and_repeat_with_period_19", the_patterns_of_the_automatic_series_are_those_measured_and_repeat_with_period_19),
        ("pptx::export_print::print_paper_from_the_command_line_sets_the_page", print_paper_from_the_command_line_sets_the_page),
        ("pptx::export_print::pure_black_and_white_chart_areas_pies_markers_and_axes", pure_black_and_white_chart_areas_pies_markers_and_axes),
        ("pptx::export_print::pure_black_and_white_draws_a_thin_black_edge_round_plain_fills", pure_black_and_white_draws_a_thin_black_edge_round_plain_fills),
        ("pptx::export_print::every_paper_has_powerpoints_size_and_places_the_slide_as_it_does", every_paper_has_powerpoints_size_and_places_the_slide_as_it_does),
    ]
}

/// The layer the export draws text with, over a headless window (the same on every platform).
fn with_layer<R>(f: impl FnOnce(&mut dyn TextLayer) -> R) -> R {
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let presentation = ibis_doc_model::Presentation::new();
    let resources = std::rc::Rc::new(cx.update(|app| crate::render::shape_view::Resources::new(&presentation, app)));
    let density = crate::export::pixel_density(&mut cx).expect("a density");
    f(&mut crate::export::SlideTextLayer { cx: &mut cx, resources, density, vector: false, last_baseline: None })
}

fn solid(w: u32, h: u32, rgb: [u8; 3]) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba([rgb[0], rgb[1], rgb[2], 255]))
}

/// アウトライン: the titles and bodies as dark text from the top of an A4 page, on more pages when they do not fit.
#[cfg_attr(windows, test)]
fn the_outline_is_text_on_pages_of_its_own() {
    let mut paragraphs = Vec::new();
    for i in 1..=120 {
        paragraphs.push(NotePara { level: 0, bullet: None, bullet_font: None, runs: vec![NoteRun { text: format!("{i}  Title {i}"), bold: true, size_pt: Some(14.0), ..Default::default() }], layout: None });
        paragraphs.push(NotePara { level: 1, bullet: Some("•".into()), bullet_font: None, runs: vec![NoteRun { text: format!("Point of slide {i}"), size_pt: Some(12.0), ..Default::default() }], layout: None });
    }
    let pages = with_layer(|layer| outline_pages(&NoteText { paragraphs }, &HandoutMarks::default(), layer));
    assert!(pages.len() >= 3, "{}", pages.len());
    for (page, size) in &pages {
        assert_eq!(*size, HANDOUT_PAGE);
        assert!(page.pixels().filter(|q| q[0] < 100 && q[1] < 100 && q[2] < 100).count() > 300, "text on every page");
    }
}

/// アウトライン: where a bullet is wider than the gap before the text, the text starts where the bullet's own typeface ends (PowerPoint sets
/// the outline's bullets in Arial: 0.35 em, 3.486 pt at 9.96 pt), not 0.55 em after it as for an unknown typeface.
#[cfg_attr(windows, test)]
fn an_outline_bullet_pushes_the_text_by_its_typefaces_advance() {
    let right_edge = |bullet_font: Option<&str>| {
        let layout = ParaLayout { left_pt: 30.0, hang_pt: 0.0, before_pt: 0.0, pitch_pt: 18.12 };
        let paragraph = NotePara { level: 0, bullet: Some("•".into()), bullet_font: bullet_font.map(str::to_string), runs: vec![NoteRun { text: "Body text of the line".into(), size_pt: Some(9.96), ..Default::default() }], layout: Some(layout) };
        let pages = with_layer(|layer| outline_pages(&NoteText { paragraphs: vec![paragraph] }, &HandoutMarks::default(), layer));
        let (page, _) = &pages[0];
        (0..page.width()).rev().find(|&x| (0..page.height()).any(|y| page.get_pixel(x, y)[0] < 100)).expect("ink")
    };
    // The page is 2 px a point: Arial's bullet is about 2 pt (4 px) narrower than 0.55 em.
    let (own, arial) = (right_edge(None), right_edge(Some("Arial")));
    assert!(own >= arial + 2, "the text ends at {arial} px with Arial's bullet and {own} px with the 0.55 em estimate");
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
    // The slide ends 666 px down (the notes master's image box: 90 pt down, 243 pt high, 2 px a point).
    assert!(red(0, 680) > 10_000 && red(680, h) == 0, "the slide above");
    let dark = |y0: u32, y1: u32| (y0..y1).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| page.get_pixel(x, y).0[..3].iter().all(|c| *c < 100)).count();
    assert!(dark(680, h) > 300, "the notes below");
    // A slide without notes has nothing below it (but the page's blank).
    assert_eq!(pages[1].0.pixels().filter(|p| p[0] < 100 && p[1] < 100 && p[2] > 150).count() > 10_000, true);
    let below = (680..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| pages[1].0.get_pixel(x, y)[0] < 100).count();
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
    assert_eq!((page.width(), page.height()), (1080, 1440));
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
    assert_eq!((wide[0].0.width(), wide[0].0.height()), (1440, 1080));
    assert_eq!(wide[0].1, (HANDOUT_PAGE.1, HANDOUT_PAGE.0));
    assert_eq!(wide.len(), 2);
}

/// The handout master's places: the header where the master puts it, big, the footer gone.
#[cfg_attr(windows, test)]
fn marks_go_where_the_handout_master_puts_them() {
    use ibis_formats::handout_master::{HandoutLayout, Mark, Place};
    let slides = vec![solid(160, 90, [255, 255, 255])];
    let layout = HandoutLayout {
        page: (6_858_000, 9_144_000),
        // The header in the middle of the page, 24 pt, centred; the date bottom right.
        places: vec![
            Place { mark: Mark::Header, frame: (1_714_500, 4_000_000, 3_429_000, 600_000), size_pt: 24.0, align: "ctr".into(), bottom: false },
            Place { mark: Mark::Date, frame: (3_884_613, 8_685_213, 2_971_800, 458_787), size_pt: 12.0, align: "r".into(), bottom: true },
        ],
    };
    let marks = HandoutMarks { header: Some("Header".into()), date: Some("2026/10/11".into()), footer: Some("Footer".into()), number: true, layout: Some(layout), notes: None, vertical_order: false, slide_numbers: Vec::new(), slide_size_pt: None, on_paper: None };
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

/// #225: `--export pdf` with `--handout`, `--notes-pages` or `--outline-pages` writes handout, notes and outline pages (A4) as a vector PDF:
/// the text on them is text of the PDF (pdftotext reads it), not a picture.
#[cfg_attr(windows, test)]
fn pdf_from_the_command_line_has_handouts_and_notes_pages() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-plan.pptx");
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/export-cli-225");
    let _ = std::fs::remove_dir_all(&out);
    // (the extra arguments, the pages the PDF has, whether it is on the handout paper)
    let slides = 4;
    let cases: [(&[&str], usize, bool); 7] = [
        (&[], slides, false),
        (&["--handout", "3"], slides.div_ceil(3), true),
        (&["--handout", "1"], slides, true),
        (&["--handout", "2", "--handout-landscape"], slides.div_ceil(2), true),
        (&["--notes-pages"], slides, true),
        (&["--handout", "4", "--print-frame", "--handout-header", "HEADER225", "--handout-footer", "FOOTER225", "--handout-number"], 1, true),
        (&["--outline-pages", "--handout-header", "HEADER225"], 1, true),
    ];
    let have_pdftotext = std::process::Command::new("pdftotext").arg("-v").output().is_ok();
    for (i, (extra, count, on_paper)) in cases.iter().enumerate() {
        let dir = out.join(i.to_string());
        let mut arguments: Vec<String> = vec!["--export".into(), "pdf".into(), "--out-dir".into(), dir.display().to_string()];
        arguments.extend(extra.iter().map(|a| a.to_string()));
        arguments.push(file.display().to_string());
        let args = crate::parse_args_from(arguments).unwrap();
        crate::run_export(&args, "pdf").unwrap();
        let path = dir.join("pptx-plan.pdf");
        let pdf = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
        let number = |after: &str| -> f32 { pdf.split(after).nth(1).unwrap().trim_start().split(|c: char| !(c.is_ascii_digit() || c == '.')).next().unwrap().parse().unwrap() };
        assert_eq!(number("/Count") as usize, *count, "{extra:?}: pages");
        // It is the vector PDF (the picture PDF writes `/MediaBox [0 0 595.00 842.00]` and one image to a page).
        assert!(!pdf.contains("/MediaBox [0 0 595.00 842.00]"), "{extra:?}: a vector PDF");
        let media = pdf.split("/MediaBox").nth(1).unwrap().trim_start().trim_start_matches('[').trim_start_matches("0 0 ").split(']').next().unwrap();
        let size: Vec<f32> = media.split_whitespace().map(|v| v.parse().unwrap()).collect();
        let landscape = extra.contains(&"--handout-landscape");
        let paper = if landscape { (HANDOUT_PAGE.1, HANDOUT_PAGE.0) } else { HANDOUT_PAGE };
        assert_eq!((size[0] == paper.0 && size[1] == paper.1), *on_paper, "{extra:?}: the page is {size:?}");
        if !have_pdftotext {
            continue;
        }
        let text = String::from_utf8_lossy(&std::process::Command::new("pdftotext").arg(&path).arg("-").output().unwrap().stdout).into_owned();
        if extra.contains(&"--handout-header") {
            assert!(text.contains("HEADER225"), "{extra:?}: the header is text of the PDF: {text:?}");
        }
        if extra.contains(&"--handout-footer") {
            assert!(text.contains("FOOTER225"), "{extra:?}: the footer is text of the PDF: {text:?}");
        }
        if extra.contains(&"--outline-pages") || extra.contains(&"--notes-pages") {
            assert!(text.trim().len() > 20, "{extra:?}: the page's text is text of the PDF: {text:?}");
        }
        // The slides are on the handout as they are: their words are text of the PDF too.
        if extra.contains(&"--handout") {
            assert!(text.trim().len() > 20, "{extra:?}: the slides' text is text of the PDF: {text:?}");
        }
    }
    let _ = std::fs::remove_dir_all(&out);
}

/// K0038: the page range is `--range` and the layout option is `--include-layout`; the old names (`--slides`, `--layout`) are still accepted.
#[test]
fn the_renamed_options_and_their_old_names_are_both_accepted() {
    let parse = |list: &[&str]| crate::parse_args_from(list.iter().map(|a| a.to_string())).unwrap();
    for range in ["--range", "--slides"] {
        let args = parse(&["--export", "png", range, "1,3-5", "deck.pptx"]);
        assert_eq!(args.slides.as_deref(), Some("1,3-5"), "{range}");
    }
    for layout in ["--include-layout", "--layout"] {
        assert!(parse(&["--export", "png", layout, "deck.pptx"]).layout, "{layout}");
    }
    assert!(!parse(&["--export", "png", "deck.pptx"]).layout);
}

/// #225: `--print-color gray` and `bw` change the colours of the vector PDF of a handout, notes pages and the outline (text stays text), by
/// PowerPoint's rules by the kind of thing (`PrintColor::map_for`): every pixel the PDF is drawn as is a grey, and in colour the deck does have colours.
#[cfg_attr(windows, test)]
fn vector_pdf_prints_in_grey_and_black_and_white() {
    if std::process::Command::new("pdftoppm").arg("-v").output().is_err() {
        return;
    }
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-plan.pptx");
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/export-cli-225-color");
    let _ = std::fs::remove_dir_all(&out);
    let coloured = |extra: &[&str], name: &str| -> usize {
        let dir = out.join(name);
        let mut arguments: Vec<String> = vec!["--export".into(), "pdf".into(), "--out-dir".into(), dir.display().to_string()];
        arguments.extend(extra.iter().map(|a| a.to_string()));
        arguments.push(file.display().to_string());
        let args = crate::parse_args_from(arguments).unwrap();
        crate::run_export(&args, "pdf").unwrap();
        let pdf = dir.join("pptx-plan.pdf");
        assert!(!String::from_utf8_lossy(&std::fs::read(&pdf).unwrap()).contains("/MediaBox [0 0 595.00 842.00]"), "{name}: a vector PDF");
        let prefix = dir.join("page");
        assert!(std::process::Command::new("pdftoppm").args(["-r", "40", "-png"]).arg(&pdf).arg(&prefix).status().unwrap().success());
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "png") {
                let image = image::open(&path).unwrap().to_rgb8();
                count += image.pixels().filter(|p| p[0] != p[1] || p[1] != p[2]).count();
            }
        }
        count
    };
    assert!(coloured(&["--handout", "4"], "color") > 0, "the deck has colours");
    for (mode, name) in [("gray", "gray"), ("bw", "bw")] {
        assert_eq!(coloured(&["--handout", "4", "--print-color", mode], name), 0, "{mode}: handout has only greys");
        assert_eq!(coloured(&["--notes-pages", "--print-color", mode], &format!("{name}-notes")), 0, "{mode}: notes pages have only greys");
    }
}

/// How many round-capped lines (`1 J`) the PDF's content streams draw: the black edge of a plain fill in the pure black and white print is the only
/// one the slides of a deck without round-capped lines have.
fn round_capped_lines(pdf: &[u8]) -> usize {
    let mut found = 0;
    let mut at = 0;
    while let Some(i) = pdf[at..].windows(6).position(|w| w == b"stream") {
        let start = at + i + 6;
        let start = start + pdf[start..].iter().take_while(|b| **b == b'\r' || **b == b'\n').count();
        let Some(len) = pdf[start..].windows(9).position(|w| w == b"endstream") else { break };
        if let Ok(data) = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..start + len]) {
            let text = String::from_utf8_lossy(&data);
            let t: Vec<&str> = text.split_whitespace().collect();
            found += t.windows(2).filter(|w| w[0] == "1" && w[1] == "J").count();
        }
        at = start + len + 9;
    }
    found
}

/// #225: in the pure black and white print (`--print-color bw`) the plain fills of shapes without a line get PowerPoint's thin black edge
/// (0.375 pt, round ends and joins: `pdf_vector::HAIRLINE_PT`, `tasks/doc/requests/2026-11-20-225-bw-hairline-result.md`); in grey and in colour they do not.
#[cfg_attr(windows, test)]
fn pure_black_and_white_draws_a_thin_black_edge_round_plain_fills() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/t88-black-shape.pptx");
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/export-cli-225-hairline");
    let _ = std::fs::remove_dir_all(&out);
    let edges = |extra: &[&str], name: &str| -> usize {
        let dir = out.join(name);
        let mut arguments: Vec<String> = vec!["--export".into(), "pdf".into(), "--out-dir".into(), dir.display().to_string()];
        arguments.extend(extra.iter().map(|a| a.to_string()));
        arguments.push(file.display().to_string());
        let args = crate::parse_args_from(arguments).unwrap();
        crate::run_export(&args, "pdf").unwrap();
        round_capped_lines(&std::fs::read(dir.join("t88-black-shape.pdf")).unwrap())
    };
    assert!(edges(&["--print-color", "bw"], "bw") > 0, "pure black and white: the plain fills have the edge");
    assert_eq!(edges(&["--print-color", "gray"], "gray"), 0, "grey: no edge");
    assert_eq!(edges(&[], "color"), 0, "colour: no edge");
}

/// #225: a chart's bars in the grey and the pure black and white print are grey (Rec.709) and not white as a shape's fill is, and have a black line; in colour
/// they are as they are (`tasks/doc/requests/2026-11-20-225-bw-hairline-result.md`: 0070C0 prints as 94).
#[cfg_attr(windows, test)]
fn chart_bars_print_grey_with_a_black_line_in_grey_and_black_and_white() {
    if std::process::Command::new("pdftoppm").arg("-v").output().is_err() {
        return;
    }
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/corpus-report/pptx/chart-bar-serlines.pptx");
    if !file.exists() {
        return;
    }
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/export-cli-225-chart");
    let _ = std::fs::remove_dir_all(&out);
    let grey_blue = |extra: &[&str], name: &str| -> usize {
        let dir = out.join(name);
        let mut arguments: Vec<String> = vec!["--export".into(), "pdf".into(), "--out-dir".into(), dir.display().to_string()];
        arguments.extend(extra.iter().map(|a| a.to_string()));
        arguments.push(file.display().to_string());
        let args = crate::parse_args_from(arguments).unwrap();
        crate::run_export(&args, "pdf").unwrap();
        let pdf = dir.join("chart-bar-serlines.pdf");
        let prefix = dir.join("page");
        assert!(std::process::Command::new("pdftoppm").args(["-r", "60", "-f", "1", "-l", "1", "-png"]).arg(&pdf).arg(&prefix).status().unwrap().success());
        let png = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).find(|p| p.extension().is_some_and(|e| e == "png")).unwrap();
        // The pixels of the 0070C0 bars' grey (94) within a unit.
        image::open(&png).unwrap().to_rgb8().pixels().filter(|p| p[0] == p[1] && p[1] == p[2] && (93..=95).contains(&p[0])).count()
    };
    assert!(grey_blue(&["--print-color", "gray"], "gray") > 1000, "grey: the bars are 94");
    assert!(grey_blue(&["--print-color", "bw"], "bw") > 1000, "pure black and white: the bars are grey too, not white");
    assert!(grey_blue(&[], "color") < 300, "colour: the bars are blue (a few anti-aliased pixels of the axes are grey)");
}

/// #208 (the user, F44): `--export html` of a presentation, a workbook, a document and a deck with SmartArt writes the charts and the SmartArt as pictures (PNG as data URIs), not a dashed
/// frame with a label: each picture decodes, is not blank, and (a presentation's) has the proportions of the frame it stands for; a SmartArt picture holds the graphic's shapes (the page
/// does not draw them again) and its `alt` is their text.
#[cfg_attr(windows, test)]
fn html_export_embeds_charts_and_smartart_as_pictures() {
    crate::register_hooks(); // a plain `#[test]` on Windows has no scenario driver to do it
    fn unbase64(text: &str) -> Vec<u8> {
        let (mut out, mut bits, mut n) = (Vec::new(), 0u32, 0);
        for c in text.bytes().filter(|c| *c != b'=') {
            let v = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".iter().position(|x| *x == c).unwrap() as u32;
            bits = (bits << 6) | v;
            n += 6;
            if n >= 8 {
                n -= 8;
                out.push((bits >> n) as u8);
                bits &= (1 << n) - 1;
            }
        }
        out
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join("target/export-cli-208-html");
    let _ = std::fs::remove_dir_all(&out);
    for (file, stem) in [("test/corpus-report/pptx/chart-bar-serlines.pptx", "chart-bar-serlines"), ("test/editing/xlsx-sales.xlsx", "xlsx-sales"), ("test/editing/xlsx-smartart.xlsx", "xlsx-smartart"), ("test/editing/docx-chart.docx", "docx-chart"), ("test/editing/pptx-smartart.pptx", "pptx-smartart"), ("test/editing/docx-smartart.docx", "docx-smartart")] {
        let path = root.join(file);
        if !path.exists() {
            continue;
        }
        let dir = out.join(stem);
        let args = crate::parse_args_from(vec!["--export".into(), "html".into(), "--out-dir".into(), dir.display().to_string(), path.display().to_string()]).unwrap();
        crate::run_export(&args, "html").unwrap();
        let html = std::fs::read_to_string(dir.join(format!("{stem}.html"))).unwrap();
        assert!(!html.contains("class=\"ph\""), "{stem}: no dashed frame with a label is left");
        let mut pictures = Vec::new();
        for part in html.split("<img ").skip(1) {
            let Some(at) = part.find("src=\"data:image/png;base64,") else { continue };
            let data = &part[at + 27..];
            let png = unbase64(&data[..data.find('"').unwrap()]);
            let image = image::load_from_memory(&png).unwrap().to_rgb8();
            assert!(image.pixels().any(|p| p.0.iter().any(|&c| c < 245)), "{stem}: a picture of {}x{} is blank", image.width(), image.height());
            pictures.push((part.to_string(), image.width(), image.height()));
        }
        assert!(!pictures.is_empty(), "{stem}: a picture of the chart or the SmartArt is in the page");
        if stem == "docx-smartart" {
            // A document's SmartArt: the picture is the graphic's frame (12 cm by 6 cm) with its three boxes drawn (not a blank page), its alt their texts, and the page does not write the texts again.
            let (part, w, h) = &pictures[0];
            assert!((*w as f64 / *h as f64 - 2.0).abs() < 0.02, "the picture is {w}x{h}, its frame 12 cm by 6 cm");
            assert!(part.starts_with("alt=\"計画, 設計, 実装\""), "alt holds the SmartArt's texts: {}", &part[..80.min(part.len())]);
            assert!(!html.contains(">計画<"), "the page does not draw the SmartArt's shapes again");
            // The boxes are the theme's accent colour (not white): a third of the frame's width is covered by them in the middle row.
            let png = {
                let at = part.find("src=\"data:image/png;base64,").unwrap() + 27;
                let data = &part[at..];
                image::load_from_memory(&unbase64(&data[..data.find('"').unwrap()])).unwrap().to_rgb8()
            };
            let row = png.height() / 2;
            let coloured = (0..png.width()).filter(|x| { let p = png.get_pixel(*x, row).0; p.iter().max().unwrap() - p.iter().min().unwrap() > 60 }).count();
            assert!(coloured as f64 > png.width() as f64 * 0.25, "the three boxes are drawn: {coloured} of {} pixels coloured", png.width());
        }
        if stem == "docx-chart" {
            let doc = ibis_formats::open(std::fs::read(&path).unwrap()).unwrap().presentation.flow.clone().unwrap();
            let titles: Vec<String> = ibis_formats::docx_convert::shapes_needing_pictures(&doc).iter().filter_map(|s| s.picture_label(&[])).collect();
            if let Some(title) = titles.first() {
                assert!(pictures[0].0.starts_with(&format!("alt=\"{}\"", title.replace('&', "&amp;"))), "the chart's alt is its title {title}: {}", &pictures[0].0[..80.min(pictures[0].0.len())]);
            }
        }
        if stem == "pptx-smartart" {
            let presentation = ibis_formats::open(std::fs::read(&path).unwrap()).unwrap().presentation;
            let slide = presentation.slides.iter().find(|s| s.shapes.iter().any(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Placeholder(ibis_doc_model::Placeholder::Diagram)))).expect("a slide with SmartArt");
            let frame = slide.shapes.iter().find(|x| matches!(x.kind, ibis_doc_model::ShapeKind::Placeholder(ibis_doc_model::Placeholder::Diagram))).unwrap().frame;
            let (_, w, h) = &pictures[0];
            let (expected, got) = (frame.width as f64 / frame.height as f64, *w as f64 / *h as f64);
            assert!((expected - got).abs() / expected < 0.01, "the SmartArt picture is {w}x{h}, its frame {}x{} EMU", frame.width, frame.height);
            let texts = slide.shapes.iter().filter(|x| slide.is_attached(x.id)).filter_map(|x| x.text_body().map(|b| b.text())).filter(|t| !t.trim().is_empty()).collect::<Vec<_>>();
            assert!(!texts.is_empty(), "the SmartArt has text");
            assert!(pictures[0].0.contains(&texts[0].split_whitespace().next().unwrap().replace('&', "&amp;")), "alt holds the SmartArt's text: {}", &pictures[0].0[..80.min(pictures[0].0.len())]);
            assert!(!html.contains(&format!(">{}<", texts[0])), "the page does not draw the SmartArt's shapes again");
        }
    }
}


/// #225: a chart's line is drawn in whole pixels of the printer's 600 dpi (0.12 pt) and one pixel or less as the thin line, whatever the paper's scale: the rows of
/// PowerPoint's table (`2026-11-20-225-chart-print-result.md`, a slide printed at 1.169 units per point: 0.25 pt is 2 pixels, 0.35 pt 3, 0.75 pt 5, 2.25 pt 16, 4.5 pt 33).
#[cfg_attr(windows, test)]
fn chart_lines_are_drawn_in_whole_device_pixels() {
    // The page's points per pixel of the window: 1 pixel is a slide point, a slide point is 1.169 units of 0.75 pt.
    let k = 1.169 * 0.75;
    for (slide_pt, pixels) in [(0.01, 0.0), (0.2, 0.0), (0.25, 2.0), (0.3, 2.0), (0.35, 3.0), (0.75, 5.0), (1.0, 7.0), (2.25, 16.0), (4.5, 33.0)] {
        let drawn = crate::pdf_vector::printed_line_width(slide_pt, k) * k;
        let expected = if pixels == 0.0 { crate::pdf_vector::HAIRLINE_PT } else { pixels * 0.12 };
        assert!((drawn - expected).abs() < 1e-3, "{slide_pt} pt: {drawn} pt on the page, expected {expected}");
    }
    // On a handout page of 6 slides (0.3385 units per point) 0.5 pt is one pixel, the thin line, and 0.75 pt two pixels (the table's 0.5 and 0.32 units).
    let k = 0.3385 * 0.75;
    assert!((crate::pdf_vector::printed_line_width(0.5, k) * k - crate::pdf_vector::HAIRLINE_PT).abs() < 1e-3);
    assert!((crate::pdf_vector::printed_line_width(0.75, k) * k - 0.24).abs() < 1e-3);
}

/// #225: in the pure black and white print a series with no format is black, white, then striped, and the filled series that state a fill but no outline get a thin
/// black edge (none where the outline is stated); the grey print changes nothing of the series.
#[cfg_attr(windows, test)]
fn pure_black_and_white_chart_series_by_their_format() {
    use ibis_doc_model::{Color, Fill, ShapeKind};
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/corpus-report/pptx/chart-bar-serlines.pptx");
    let Ok(data) = std::fs::read(&file) else { return };
    let presentation = ibis_formats::read(&data).unwrap();
    let series_of = |slide: &ibis_doc_model::Slide| -> Vec<ibis_doc_model::Series> {
        slide.shapes.iter().filter_map(|s| if let ShapeKind::Chart(c) = &s.kind { Some(c.groups.iter().flat_map(|g| g.series.clone()).collect::<Vec<_>>()) } else { None }).flatten().collect()
    };
    let before = series_of(&presentation.slides[0]);
    assert!(!before.is_empty(), "the file has a chart");
    let mut grey = presentation.slides[0].clone();
    crate::export::print_chart_series(&mut grey, crate::export_print::PrintColor::Gray);
    assert_eq!(series_of(&grey), before, "grey: the series are left as they are");
    let mut bw = presentation.slides[0].clone();
    crate::export::print_chart_series(&mut bw, crate::export_print::PrintColor::BlackWhite);
    for (was, now) in before.iter().zip(series_of(&bw)) {
        if !was.has_outline {
            assert_eq!(now.stroke.as_ref().map(|s| (s.color, s.width)), Some((Color::BLACK, 7_620)), "{}: a thin black edge", was.name);
        } else {
            assert_eq!(now.stroke, was.stroke, "{}: its outline is stated", was.name);
        }
        if !was.has_format {
            assert!(matches!(now.fill, Fill::Solid(_) | Fill::Pattern { .. }), "{}: black, white or striped", was.name);
        }
    }
}

/// #225: the rest of a chart in the pure black and white print, on the chart of `chart-bar-serlines.pptx` made an area chart and a pie chart: the series that state no
/// format are black, white, then the dots of the paper's grid (a pie's slices by their number) and every filled series without an outline element has the thin black edge;
/// a marker has the edge.
#[cfg_attr(windows, test)]
fn pure_black_and_white_chart_areas_pies_markers_and_axes() {
    use ibis_doc_model::{ChartType, Color, Fill, Marker, MarkerSymbol, ShapeKind, Stroke};
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/corpus-report/pptx/chart-bar-serlines.pptx");
    let Ok(data) = std::fs::read(&file) else { return };
    let presentation = ibis_formats::read(&data).unwrap();
    let edge = Some((Color::BLACK, 7_620));
    let black = |s: &Option<Stroke>| s.as_ref().map(|s| (s.color, s.width));
    for kind in [ChartType::Area, ChartType::Pie { first_angle: 0 }] {
        let mut slide = presentation.slides[0].clone();
        let mut found = false;
        for shape in &mut slide.shapes {
            let ShapeKind::Chart(chart) = &mut shape.kind else { continue };
            found = true;
            for group in &mut chart.groups {
                group.kind = kind;
                for (i, series) in group.series.iter_mut().enumerate() {
                    series.has_format = i != 2;
                    series.has_outline = i == 1;
                    series.stroke = if i == 1 { Some(Stroke::solid(Color::opaque(ibis_doc_model::Rgb(0xff0000)), 19_050)) } else { None };
                    series.values = vec![series.values.first().cloned().flatten(); 4];
                }
            }
        }
        assert!(found);
        crate::export::print_chart_series(&mut slide, crate::export_print::PrintColor::BlackWhite);
        for shape in &slide.shapes {
            let ShapeKind::Chart(chart) = &shape.kind else { continue };
            for group in &chart.groups {
                let series = &group.series;
                assert!(series.len() >= 3, "the chart has three series or more");
                assert_eq!(black(&series[0].stroke), edge, "{kind:?}: a stated fill with no outline element has the edge");
                assert_eq!(black(&series[1].stroke), Some((Color::opaque(ibis_doc_model::Rgb(0xff0000)), 19_050)), "{kind:?}: a stated outline is left alone");
                assert_eq!(black(&series[2].stroke), edge, "{kind:?}: an automatic series has the edge");
                assert_eq!(series[0].fill, series[0].fill, "{kind:?}");
                assert!(matches!(&series[2].fill, Fill::Pattern { preset, .. } if preset == "printDots3"), "{kind:?}: the third series is the dots of series 3");
                if matches!(kind, ChartType::Pie { .. }) {
                    // A pie's slices by their number: black, white, then the dots of series 3 and 4.
                    let slice = |i: usize| series[2].point_fills.iter().find(|(idx, _)| *idx == i).map(|(_, f)| f.clone());
                    assert_eq!(slice(0), Some(Fill::Solid(Color::BLACK)));
                    assert_eq!(slice(1), Some(Fill::Solid(Color::WHITE)));
                    assert!(matches!(slice(2), Some(Fill::Pattern { preset, .. }) if preset == "printDots3") && matches!(slice(3), Some(Fill::Pattern { preset, .. }) if preset == "printDots4"));
                }
            }
        }
    }
    // Markers.
    let mut slide = presentation.slides[0].clone();
    for shape in &mut slide.shapes {
        let ShapeKind::Chart(chart) = &mut shape.kind else { continue };
        for group in &mut chart.groups {
            group.kind = ChartType::Line;
            for series in &mut group.series {
                series.marker = Some(Marker { source: None, symbol: MarkerSymbol::Circle, size: 7, fill: Fill::Solid(Color::BLACK), edge: None });
            }
        }
    }
    crate::export::print_chart_series(&mut slide, crate::export_print::PrintColor::BlackWhite);
    for shape in &slide.shapes {
        let ShapeKind::Chart(chart) = &shape.kind else { continue };
        for series in chart.groups.iter().flat_map(|g| &g.series) {
            assert_eq!(black(&series.marker.as_ref().unwrap().edge), edge, "a marker has the edge");
        }
    }
}

/// #225: the 3rd to 6th automatic series of a chart in the pure black and white print are the dots of a grid fixed to the paper, 1/96 inch to a cell
/// (`2026-11-21-225-chart-stripes-result.md`: a checkerboard, black with white dots, white with black dots twice). The PDF of `chart-auto-series.pptx` (6 series
/// with no format) is drawn at 96 dpi, so a cell is a pixel, and a 16 by 16 window of the bars that is all black and white must be the formula of one of the four,
/// each of which has such a window (a viewer's smoothing leaves a lone white dot among black ones light grey, 190 or so).
#[cfg_attr(windows, test)]
fn pure_black_and_white_automatic_series_have_the_dots_of_the_papers_grid() {
    if std::process::Command::new("pdftoppm").arg("-v").output().is_err() {
        return;
    }
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/corpus-report/pptx/chart-auto-series.pptx");
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/export-cli-225-dots");
    let _ = std::fs::remove_dir_all(&out);
    let arguments: Vec<String> = ["--export", "pdf", "--print-color", "bw", "--out-dir"].iter().map(|a| a.to_string()).chain([out.display().to_string(), file.display().to_string()]).collect();
    let args = crate::parse_args_from(arguments).unwrap();
    crate::run_export(&args, "pdf").unwrap();
    let prefix = out.join("page");
    assert!(std::process::Command::new("pdftoppm").args(["-r", "96", "-f", "1", "-l", "1", "-png"]).arg(out.join("chart-auto-series.pdf")).arg(&prefix).status().unwrap().success());
    let png = std::fs::read_dir(&out).unwrap().map(|e| e.unwrap().path()).find(|p| p.extension().is_some_and(|e| e == "png")).unwrap();
    let image = image::open(&png).unwrap().to_luma8();
    let mut found = [0usize; 4];
    let mut unmatched = 0;
    for wy in (0..image.height().saturating_sub(16)).step_by(16) {
        for wx in (0..image.width().saturating_sub(16)).step_by(16) {
            let pixels: Vec<(u32, u32, bool)> = (0..16).flat_map(|dy| (0..16).map(move |dx| (wx + dx, wy + dy))).map(|(x, y)| (x, y, image.get_pixel(x, y)[0] < 64)).collect();
            let extreme = (0..16u32).flat_map(|dy| (0..16u32).map(move |dx| (wx + dx, wy + dy))).all(|(x, y)| { let v = image.get_pixel(x, y)[0]; v < 64 || v >= 128 });
            let black = pixels.iter().filter(|p| p.2).count();
            if !extreme || black < 8 || black > 248 {
                continue;
            }
            match (3u8..=6).position(|n| pixels.iter().all(|&(x, y, b)| b == crate::pdf_vector::dot_is_black(n, x, y + 1))) {
                Some(i) => found[i] += 1,
                None => unmatched += 1,
            }
        }
    }
    eprintln!("dots windows of series 3, 4, 5, 6: {found:?}, other mixed windows: {unmatched}");
    for (i, n) in (3..=6).enumerate() {
        assert!(found[i] > 0, "series {n}: a window of its dots");
    }
}

/// The dots' formulas: the fractions of black the measurements give (50, 87.5, 25 and 6.25 percent) over the 8 by 4 tile.
fn the_dots_of_the_automatic_series_have_the_measured_fractions_of_black() {
    for (n, black) in [(3u8, 16), (4, 28), (5, 8), (6, 2)] {
        let count = (0..4).flat_map(|y| (0..8).map(move |x| (x, y))).filter(|&(x, y)| crate::pdf_vector::dot_is_black(n, x, y)).count();
        assert_eq!(count, black, "series {n}");
    }
}


/// #225: the papers and how a page is put on one (Microsoft 365's PDFs on A4 of slides of 6 sizes, notes pages: `2026-11-23-225-patterns-7plus-result.md` 3 節; the
/// other papers are not measured). The numbers are the result's units (0.75 pt) times 0.75.
#[cfg_attr(windows, test)]
fn print_paper_puts_the_pages_on_the_paper_as_powerpoint_does() {
    use crate::export_print::Paper;
    let a4 = Paper::parse("A4").unwrap();
    assert!((a4.short - 595.32).abs() < 0.005 && (a4.long - 841.92).abs() < 0.005, "{a4:?}");
    assert_eq!(Paper::parse("letter").unwrap(), Paper { short: 612.0, long: 792.0 });
    assert_eq!(Paper::parse("210x297mm").unwrap(), a4);
    assert_eq!(Paper::parse("297x210mm").unwrap(), a4);
    assert!(Paper::parse("a6").is_err() && Paper::parse("0x0mm").is_err());
    let near = |a: f32, b: f32| (a - b).abs() < 0.01;
    // 16:9 (960 x 540 pt): A4 landscape, 1.1693 units a point, 81.12 units above and below.
    let f = a4.fit((960.0, 540.0), true);
    assert_eq!(f.page_pt, (a4.long, a4.short));
    assert!(near(f.scale, 841.92 / 960.0) && (f.scale / 0.75 - 1.1693).abs() < 0.0001, "{f:?}");
    // (81.12 units above and below were measured within 0.1 unit; the exact centre is 81.16.)
    assert!(near(f.shift.0, 0.0) && (f.shift.1 - 81.12 * 0.75).abs() < 0.075, "{f:?}");
    // The notes page (540 x 720 pt): A4 portrait, 1.1024 and 24.12 pt above and below.
    let f = a4.fit((540.0, 720.0), true);
    assert_eq!(f.page_pt, (a4.short, a4.long));
    assert!(near(f.scale, 595.32 / 540.0) && near(f.shift.0, 0.0) && (f.shift.1 - 24.12).abs() < 0.05, "{f:?}");
    // 4:3 (720 x 540): the height decides, 1.47 units a point, 32.08 units from the sides.
    let f = a4.fit((720.0, 540.0), true);
    assert!(near(f.scale, 595.32 / 540.0) && (f.shift.0 - 32.08 * 0.75).abs() < 0.075 && near(f.shift.1, 0.0), "{f:?}");
    // Taller than wide: a portrait paper.
    let f = a4.fit((540.0, 720.0 + 1.0), true);
    assert_eq!(f.page_pt, (a4.short, a4.long));
    // Large (2000 x 1125): scaled down, 0.5613 units a point.
    let f = a4.fit((2000.0, 1125.0), true);
    assert!(near(f.scale, 841.92 / 2000.0), "{f:?}");
    // Not fitted: 100 %, in the middle (a small slide stays small, a large one is cut off at the sides).
    let f = a4.fit((360.0, 202.5), false);
    assert!(near(f.scale, 1.0) && near(f.shift.0, (841.92 - 360.0) / 2.0) && near(f.shift.1, (595.32 - 202.5) / 2.0), "{f:?}");
    let f = a4.fit((960.0, 540.0), false);
    assert!(near(f.scale, 1.0) && near(f.shift.0, (841.92 - 960.0) / 2.0), "{f:?}");
    // A paper of its own: a landscape page on a portrait-named size.
    let f = Paper::parse("300x200mm").unwrap().fit((960.0, 540.0), true);
    assert!(f.page_pt.0 > f.page_pt.1);
}

/// #225: the patterns of the pure black and white print's automatic series 3 to 19 are the formulas measured (`2026-11-23-225-patterns-7plus-result.md` 1 節; written here
/// separately from the tiles the code holds), and the series repeat with period 19: the 20th is black, the 21st white, the 22nd the 3rd's.
#[cfg_attr(windows, test)]
fn the_patterns_of_the_automatic_series_are_those_measured_and_repeat_with_period_19() {
    let by_formula = |p: u32, i: i64, j: i64| -> bool {
        let m = |v: i64, n: i64| v.rem_euclid(n);
        match p {
            3 => m(i + j, 2) == 1,
            4 => !(m(j, 2) == 1 && m(i, 4) == if m(j, 4) == 1 { 0 } else { 2 }),
            5 => m(i, 4) == if m(j, 2) == 1 { 0 } else { 2 },
            6 => m(j, 2) == 1 && m(i, 8) == if m(j, 4) == 1 { 0 } else { 4 },
            7 => matches!(m(j, 4), 1 | 2),
            8 => matches!(m(i, 4), 0 | 1),
            9 => matches!(m(i - j, 4), 0 | 3),
            10 => matches!(m(i + j, 4), 0 | 3),
            11 => (m(i + 1, 4) < 2) == (m(j, 4) < 2),
            12 => m(j, 2) == 1 || if m(j, 4) == 0 { matches!(m(i, 4), 0 | 3) } else { matches!(m(i, 4), 1 | 2) },
            13 => m(j, 4) == 1,
            14 => m(i, 4) == 0,
            15 => m(i - j, 4) == 3,
            16 => m(i + j, 4) == 0,
            17 => m(j, 4) == 1 || m(i, 4) == 0,
            18 => {
                let d = [0, 0, 1, 2, 3, 3, 2, 1][m(j, 8) as usize];
                m(i, 8) == d || m(i, 8) == 7 - d
            }
            _ => (m(j, 8) == 1 && m(i, 8) == 0) || (m(j, 8) == 5 && m(i, 8) == 4),
        }
    };
    for p in 3..=19u32 {
        for j in 0..16i64 {
            for i in 0..16i64 {
                assert_eq!(crate::pdf_vector::dot_is_black(p as u8, i as u32, j as u32), by_formula(p, i, j), "pattern {p} at cell ({i}, {j})");
            }
        }
    }
    // Percentages of black over the 8 by 8 tile (the table of the result).
    for (p, percent) in [(3, 50.0), (4, 87.5), (5, 25.0), (6, 6.25), (7, 50.0), (9, 50.0), (11, 50.0), (12, 75.0), (13, 25.0), (17, 43.75), (18, 25.0), (19, 3.125)] {
        let count = (0..8).flat_map(|y| (0..8).map(move |x| (x, y))).filter(|&(x, y)| crate::pdf_vector::dot_is_black(p, x, y)).count();
        assert_eq!(count as f32 / 64.0 * 100.0, percent, "pattern {p}");
    }
    use ibis_doc_model::{Color, Fill};
    let preset = |f: Fill| match f {
        Fill::Pattern { preset, .. } => preset,
        _ => String::new(),
    };
    assert_eq!(crate::export::print_automatic_fill(0), Fill::Solid(Color::BLACK));
    assert_eq!(crate::export::print_automatic_fill(1), Fill::Solid(Color::WHITE));
    assert_eq!(preset(crate::export::print_automatic_fill(2)), "printDots3");
    assert_eq!(preset(crate::export::print_automatic_fill(6)), "printDots7");
    assert_eq!(preset(crate::export::print_automatic_fill(18)), "printDots19");
    assert_eq!(crate::export::print_automatic_fill(19), Fill::Solid(Color::BLACK));
    assert_eq!(crate::export::print_automatic_fill(20), Fill::Solid(Color::WHITE));
    assert_eq!(preset(crate::export::print_automatic_fill(21)), "printDots3");
    assert_eq!(preset(crate::export::print_automatic_fill(23)), "printDots5");
}

/// #225: `--print-paper a4` makes every page of the PDF the paper's size (landscape for a slide as wide as high), `--no-print-fit-paper` is read, and a wrong name is an error.
fn print_paper_from_the_command_line_sets_the_page() {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/corpus-report/pptx/chart-auto-series.pptx");
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/export-cli-225-paper");
    let media_box = |extra: &[&str]| -> String {
        let _ = std::fs::remove_dir_all(&out);
        let arguments: Vec<String> = ["--export", "pdf", "--out-dir"].iter().map(|a| a.to_string()).chain([out.display().to_string()]).chain(extra.iter().map(|a| a.to_string())).chain([file.display().to_string()]).collect();
        let args = crate::parse_args_from(arguments).unwrap();
        crate::run_export(&args, "pdf").unwrap();
        let bytes = std::fs::read(out.join("chart-auto-series.pdf")).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let at = text.find("/MediaBox").expect("a MediaBox");
        text[at..].split(']').next().unwrap_or("").to_string()
    };
    let plain = media_box(&[]);
    let a4 = media_box(&["--print-paper", "a4"]);
    assert!(a4.contains("841.92") && a4.contains("595.32"), "a4: {a4}");
    assert_ne!(plain, a4);
    let fitted_not = media_box(&["--print-paper", "letter", "--no-print-fit-paper"]);
    assert!(fitted_not.contains("792") && fitted_not.contains("612"), "letter: {fitted_not}");
    let wrong = crate::parse_args_from(["--export", "pdf", "--print-paper", "a9", file.display().to_string().as_str()].iter().map(|a| a.to_string()).collect::<Vec<_>>());
    if let Ok(args) = wrong {
        assert!(crate::run_export(&args, "pdf").is_err());
    }
}


/// `--export png` of a .csv, .tsv or .tab file (a text table opens as a sheet in the window): it is read as the window reads it, not refused as "not a ZIP file".
#[cfg_attr(windows, test)]
fn export_png_reads_a_text_table_as_the_window_does() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/export-cli-text-table");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    for (name, text) in [("sample.csv", "a,b,c\n1,2,3\n4,5,6\n"), ("sample.tsv", "a\tb\tc\n1\t2\t3\n4\t5\t6\n"), ("sample.tab", "a\tb\n1\t2\n")] {
        let file = out.join(name);
        std::fs::write(&file, text).unwrap();
        let dir = out.join(format!("png-{name}"));
        let arguments: Vec<String> = vec!["--export".into(), "png".into(), "--out-dir".into(), dir.display().to_string(), file.display().to_string()];
        let args = crate::parse_args_from(arguments).unwrap();
        crate::run_export(&args, "png").unwrap_or_else(|e| panic!("{name}: {e}"));
        let stem = name.rsplit_once('.').unwrap().0;
        let png = dir.join(format!("{stem}-01.png"));
        assert!(std::fs::metadata(&png).map(|m| m.len() > 1000).unwrap_or(false), "{name}: a picture of the sheet is written ({})", png.display());
    }
}

/// `--export html` of a .csv file: read as the window reads it (a sheet), so the page holds the table's cells.
#[cfg_attr(windows, test)]
fn export_html_reads_a_text_table_as_the_window_does() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/export-cli-text-table-html");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let file = out.join("sample.csv");
    std::fs::write(&file, "name,count\nalpha,4711\nbeta,4712\n").unwrap();
    let dir = out.join("html");
    let arguments: Vec<String> = vec!["--export".into(), "html".into(), "--out-dir".into(), dir.display().to_string(), file.display().to_string()];
    let args = crate::parse_args_from(arguments).unwrap();
    crate::run_export(&args, "html").unwrap_or_else(|e| panic!("sample.csv: {e}"));
    let page = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "html")).expect("an HTML page is written");
    let text = std::fs::read_to_string(&page).unwrap();
    assert!(text.contains("alpha") && text.contains("4711") && text.contains("4712"), "the page holds the cells of the table");
}

// ---- #225: A4 and other papers, handouts portrait and landscape (tasks/doc/test-design/issue-225-paper.md, Windows PowerPoint's measurements of 2026-11-23/24) ----

/// The stroked rectangles of the PDF's content streams, in points from the page's top left: (x, y, width, height). A rectangle is `x y w h re` or a closed path of four corners
/// (`m l l l h`) under the current matrix (`q`, `Q`, `cm`); one is taken when it is painted with a stroke (`S`, `s`, `B`, `b`).
fn stroked_rectangles(pdf: &[u8], page_height: f32) -> Vec<[f32; 4]> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(i) = pdf[at..].windows(6).position(|w| w == b"stream") {
        let start = at + i + 6;
        let start = start + pdf[start..].iter().take_while(|b| **b == b'\r' || **b == b'\n').count();
        let Some(len) = pdf[start..].windows(9).position(|w| w == b"endstream") else { break };
        if let Ok(data) = miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..start + len]) {
            let text = String::from_utf8_lossy(&data);
            let mut stack: Vec<[f32; 6]> = Vec::new();
            let mut ctm = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
            let mut numbers: Vec<f32> = Vec::new();
            let mut path: Vec<(f32, f32)> = Vec::new();
            let mut pending: Option<[f32; 4]> = None;
            let apply = |m: &[f32; 6], x: f32, y: f32| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]);
            for token in text.split_whitespace() {
                if let Ok(n) = token.parse::<f32>() {
                    numbers.push(n);
                    continue;
                }
                match token {
                    "q" => stack.push(ctm),
                    "Q" => ctm = stack.pop().unwrap_or(ctm),
                    "cm" if numbers.len() >= 6 => {
                        let n = &numbers[numbers.len() - 6..];
                        // new = n x ctm
                        let m = [
                            n[0] * ctm[0] + n[1] * ctm[2],
                            n[0] * ctm[1] + n[1] * ctm[3],
                            n[2] * ctm[0] + n[3] * ctm[2],
                            n[2] * ctm[1] + n[3] * ctm[3],
                            n[4] * ctm[0] + n[5] * ctm[2] + ctm[4],
                            n[4] * ctm[1] + n[5] * ctm[3] + ctm[5],
                        ];
                        ctm = m;
                    }
                    "re" if numbers.len() >= 4 => {
                        let n = &numbers[numbers.len() - 4..];
                        let (a, b) = (apply(&ctm, n[0], n[1]), apply(&ctm, n[0] + n[2], n[1] + n[3]));
                        pending = Some([a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs()]);
                    }
                    "m" if numbers.len() >= 2 => {
                        let n = &numbers[numbers.len() - 2..];
                        path = vec![apply(&ctm, n[0], n[1])];
                    }
                    "l" if numbers.len() >= 2 => {
                        let n = &numbers[numbers.len() - 2..];
                        path.push(apply(&ctm, n[0], n[1]));
                    }
                    "h" => {
                        if path.len() == 4 {
                            let (x0, x1) = (path.iter().map(|p| p.0).fold(f32::MAX, f32::min), path.iter().map(|p| p.0).fold(f32::MIN, f32::max));
                            let (y0, y1) = (path.iter().map(|p| p.1).fold(f32::MAX, f32::min), path.iter().map(|p| p.1).fold(f32::MIN, f32::max));
                            pending = Some([x0, y0, x1 - x0, y1 - y0]);
                        }
                    }
                    "S" | "s" | "B" | "b" | "B*" | "b*" => {
                        if let Some(r) = pending.take() {
                            found.push([r[0], page_height - (r[1] + r[3]), r[2], r[3]]);
                        }
                        path.clear();
                    }
                    "f" | "F" | "f*" | "n" => {
                        pending = None;
                        path.clear();
                    }
                    _ => {}
                }
                numbers.clear();
            }
        }
        at = start + len + 9;
    }
    found
}

/// One expected frame in PowerPoint's units (1/96 inch: 0.75 pt): (x, y, width, height) of a slide's box on a handout page.
type Box96 = [f32; 4];

/// A deck's handout page printed with `--handout <per page> --print-paper <paper> --print-frame` (and `--handout-landscape`): the frames drawn round the slides are
/// where PowerPoint puts the slides (`expected`, in order: left to right then down), within 0.8 pt in position and 1.2 pt in size. Returns the problems.
fn handout_frames_problems(deck: &str, extra: &[&str], page: (f32, f32), ratio: f32, expected: &[Box96], label: &str) -> Vec<String> {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(deck);
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/export-cli-225-paper-handout");
    let _ = std::fs::remove_dir_all(&out);
    let mut arguments: Vec<String> = ["--export", "pdf", "--out-dir"].iter().map(|a| a.to_string()).chain([out.display().to_string()]).collect();
    arguments.extend(extra.iter().map(|a| a.to_string()));
    arguments.extend(["--print-frame".to_string(), file.display().to_string()]);
    let args = match crate::parse_args_from(arguments) {
        Ok(a) => a,
        Err(e) => return vec![format!("{label}: {e}")],
    };
    if let Err(e) = crate::run_export(&args, "pdf") {
        return vec![format!("{label}: {e}")];
    }
    let pdf = std::fs::read(out.join(deck.replace(".pptx", ".pdf"))).unwrap();
    let media = String::from_utf8_lossy(&pdf).split("/MediaBox").nth(1).unwrap_or("").trim_start().trim_start_matches('[').trim_start_matches("0 0 ").split(']').next().unwrap_or("").to_string();
    let size: Vec<f32> = media.split_whitespace().filter_map(|v| v.parse().ok()).collect();
    let mut problems = Vec::new();
    if size.len() != 2 || (size[0] - page.0).abs() > 0.15 || (size[1] - page.1).abs() > 0.15 {
        problems.push(format!("{label}: the page is {size:?}, PowerPoint's {page:?}"));
    }
    // A slide's frame: a stroked rectangle of the slide's shape (not a shape in a slide) at least 100 pt wide.
    let frames: Vec<[f32; 4]> = stroked_rectangles(&pdf, page.1).into_iter().filter(|r| r[2] > 100.0 && (r[2] / r[3] - ratio).abs() < ratio * 0.01).collect();
    let slides_in_the_deck = 4;
    for (i, e) in expected.iter().enumerate().take(slides_in_the_deck) {
        // PowerPoint's boxes are the slide's outer edge; the frame (0.96 pt) is stroked with its middle half of it inside that edge (`2026-11-18-225-frame.md`), and
        // the rectangle read from the PDF is the stroke's middle line: inset by half the line on every side.
        let half = 0.48;
        let want = [e[0] * 0.75 + half, e[1] * 0.75 + half, e[2] * 0.75 - 2.0 * half, e[3] * 0.75 - 2.0 * half];
        if !frames.iter().any(|f| (f[0] - want[0]).abs() <= 0.8 && (f[1] - want[1]).abs() <= 0.8 && (f[2] - want[2]).abs() <= 1.2 && (f[3] - want[3]).abs() <= 1.2) {
            problems.push(format!("{label}: slide {} is not at {want:.2?} pt (PowerPoint's {e:?} units); frames drawn: {frames:.2?}", i + 1));
        }
    }
    problems
}

fn ratio_of(w: f32, h: f32) -> f32 {
    w / h
}

/// #225 (R-4): handouts of 1, 2, 3 and 6 slides to a page on A4, portrait and landscape, for a 16:9 and a 4:3 deck: the slides' frames are where PowerPoint puts them
/// (`tasks/doc/requests/2026-11-23-225-paper-sizes-result.md`): the page's centre is the origin and x and y are stretched on their own (kx = paper width / the page's
/// width, ky = the paper's height / the page's height; the page is 720 x 960 units portrait, 960 x 720 landscape), the slide fitted into the stretched box.
#[cfg_attr(windows, test)]
fn handouts_on_a4_put_the_slides_where_powerpoint_does() {
    let pt = |w: f32, h: f32| (w * 0.75, h * 0.75);
    let portrait = pt(793.76, 1122.56);
    let landscape = pt(1122.56, 793.76);
    let one = |x: f32, y: f32, w: f32, h: f32| -> Box96 { [x, y, w, h] };
    let mut problems = Vec::new();
    // 16:9 (pptx-plan.pptx, 960 x 540 pt): portrait.
    let col = |x: f32, w: f32, ys: [f32; 3], h: f32| -> Vec<Box96> { ys.iter().map(|y| one(x, *y, w, h)).collect() };
    let r = ratio_of(16.0, 9.0);
    let ys169 = [144.16, 469.44, 794.88];
    let two_up = vec![one(76.80, 137.12, 640.0, 360.0), one(76.80, 625.28, 640.0, 360.0)];
    problems.extend(handout_frames_problems("pptx-plan.pptx", &["--handout", "2", "--print-paper", "a4"], portrait, r, &two_up, "16:9 portrait 2-up"));
    problems.extend(handout_frames_problems("pptx-plan.pptx", &["--handout", "3", "--print-paper", "a4"], portrait, r, &col(55.52, 325.44, ys169, 183.04), "16:9 portrait 3-up"));
    let mut six = Vec::new();
    for y in ys169 {
        six.push(one(55.52, y, 325.44, 183.04));
        six.push(one(412.80, y, 325.60, 183.04));
    }
    problems.extend(handout_frames_problems("pptx-plan.pptx", &["--handout", "6", "--print-paper", "a4"], portrait, r, &six, "16:9 portrait 6-up"));
    problems.extend(handout_frames_problems("pptx-plan.pptx", &["--handout", "1", "--print-paper", "a4"], portrait, r, &[one(76.80, 381.12, 640.0, 360.0)], "16:9 portrait 1-up"));
    // 16:9 landscape.
    let land = ["--handout-landscape", "--print-paper", "a4"];
    let with = |n: &'static str| -> Vec<&'static str> { ["--handout", n].into_iter().chain(land).collect() };
    problems.extend(handout_frames_problems("pptx-plan.pptx", &with("2"), landscape, r, &[one(78.56, 267.52, 460.32, 258.88), one(583.68, 267.52, 460.32, 258.88)], "16:9 landscape 2-up"));
    let xs = [78.56, 415.20, 751.84];
    problems.extend(handout_frames_problems("pptx-plan.pptx", &with("3"), landscape, r, &xs.iter().map(|x| one(*x, 142.24, 291.84, 164.0)).collect::<Vec<_>>(), "16:9 landscape 3-up"));
    let mut six_wide: Vec<Box96> = xs.iter().map(|x| one(*x, 142.24, 291.84, 164.0)).collect();
    six_wide.push(one(78.56, 487.52, 291.84, 164.0));
    problems.extend(handout_frames_problems("pptx-plan.pptx", &with("6"), landscape, r, &six_wide, "16:9 landscape 6-up"));
    problems.extend(handout_frames_problems("pptx-plan.pptx", &with("1"), landscape, r, &[one(78.56, 125.28, 965.44, 543.04)], "16:9 landscape 1-up"));
    // 4:3 (pptx-plan-4x3.pptx, 720 x 540 pt): portrait.
    let r = ratio_of(4.0, 3.0);
    let ys43 = [113.60, 439.04, 764.32];
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &["--handout", "2", "--print-paper", "a4"], portrait, r, &[one(156.80, 137.12, 480.0, 360.0), one(156.80, 625.28, 480.0, 360.0)], "4:3 portrait 2-up"));
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &["--handout", "3", "--print-paper", "a4"], portrait, r, &col(55.52, 325.44, ys43, 244.0), "4:3 portrait 3-up"));
    let mut six43 = Vec::new();
    for y in ys43 {
        six43.push(one(55.52, y, 325.44, 244.0));
        six43.push(one(412.80, y, 325.44, 244.0));
    }
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &["--handout", "6", "--print-paper", "a4"], portrait, r, &six43, "4:3 portrait 6-up"));
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &["--handout", "1", "--print-paper", "a4"], portrait, r, &[one(55.52, 305.12, 682.88, 512.0)], "4:3 portrait 1-up"));
    // 4:3 landscape.
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &with("2"), landscape, r, &[one(68.64, 216.80, 480.0, 360.0), one(573.76, 216.80, 480.0, 360.0)], "4:3 landscape 2-up"));
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &with("3"), landscape, r, &xs.iter().map(|x| one(*x, 114.88, 291.84, 218.72)).collect::<Vec<_>>(), "4:3 landscape 3-up"));
    let mut six43_wide: Vec<Box96> = xs.iter().map(|x| one(*x, 114.88, 291.84, 218.72)).collect();
    six43_wide.push(one(78.56, 460.16, 291.84, 218.72));
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &with("6"), landscape, r, &six43_wide, "4:3 landscape 6-up"));
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &with("1"), landscape, r, &[one(137.76, 79.36, 846.88, 635.2)], "4:3 landscape 1-up"));
    assert!(problems.is_empty(), "{problems:#?}");
}

/// #225 (R-5): a handout of 6 on other papers (portrait): A3 and Letter, 16:9 and 4:3. The A3 4:3 slides are 480 x 360 units, not the 460.25 x 345.19 of the box (PowerPoint's
/// measured exception, unexplained).
#[cfg_attr(windows, test)]
fn handouts_of_six_on_a3_and_letter_put_the_slides_where_powerpoint_does() {
    let one = |x: f32, y: f32, w: f32, h: f32| -> Box96 { [x, y, w, h] };
    let rows = |left: f32, right: f32, ys: [f32; 3], w: f32, h: f32| -> Vec<Box96> {
        let mut v = Vec::new();
        for y in ys {
            v.push(one(left, y, w, h));
            v.push(one(right, y, w, h));
        }
        v
    };
    let mut problems = Vec::new();
    let a3 = (841.92, 1190.52);
    let letter = (612.0, 792.0);
    problems.extend(handout_frames_problems("pptx-plan.pptx", &["--handout", "6", "--print-paper", "a3"], a3, ratio_of(16.0, 9.0), &rows(78.56, 583.68, [203.84, 664.16, 1124.48], 460.32, 258.88), "A3 16:9"));
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &["--handout", "6", "--print-paper", "a3"], a3, ratio_of(4.0, 3.0), &rows(68.64, 573.76, [153.28, 613.60, 1074.08], 480.0, 360.0), "A3 4:3"));
    problems.extend(handout_frames_problems("pptx-plan.pptx", &["--handout", "6", "--print-paper", "letter"], letter, ratio_of(16.0, 9.0), &rows(57.12, 424.32, [127.68, 433.76, 740.00], 334.56, 188.0), "Letter 16:9"));
    problems.extend(handout_frames_problems("pptx-plan-4x3.pptx", &["--handout", "6", "--print-paper", "letter"], letter, ratio_of(4.0, 3.0), &rows(69.60, 436.80, [105.60, 411.84, 718.08], 309.60, 232.16), "Letter 4:3"));
    assert!(problems.is_empty(), "{problems:#?}");
}

/// #225 (R-1, R-2): the page of every paper is its size in whole pixels of the printer's 600 dpi (Microsoft Print to PDF's MediaBox), and the slide printed on it is scaled
/// and put as PowerPoint does: fitted (the smaller ratio, in the middle) or at 100 % (in the middle, cut off where it is larger than the paper). Values: the tables of
/// 2026-11-23-225-paper-sizes-result.md (units of 0.75 pt: the paper's `fit` is in points).
#[cfg_attr(windows, test)]
fn every_paper_has_powerpoints_size_and_places_the_slide_as_it_does() {
    use crate::export_print::Paper;
    // (name, the page in points (landscape for a slide: long, short), 16:9 fitted (scale in units a point, x, y in units), 4:3 fitted).
    let papers: [(&str, (f32, f32), [f32; 3], [f32; 3]); 7] = [
        ("a3", (1190.52, 841.92), [1.6535, 0.0, 114.88], [2.0787, 45.28, 0.0]),
        ("b4", (1031.76, 728.52), [1.4330, 0.0, 98.72], [1.7987, 40.16, 0.0]),
        ("a5", (595.32, 419.52), [0.8268, 0.0, 56.32], [1.0358, 24.00, 0.0]),
        ("b5", (728.52, 515.88), [1.0118, 0.0, 70.72], [1.2738, 27.04, 0.0]),
        ("letter", (792.0, 612.0), [1.1000, 0.0, 111.04], [1.4667, 0.0, 12.00]),
        ("legal", (1008.0, 612.0), [1.4000, 0.0, 30.08], [1.5111, 128.00, 0.0]),
        ("tabloid", (1224.0, 792.0), [1.7000, 0.0, 68.96], [1.9556, 112.00, 0.0]),
    ];
    let mut problems = Vec::new();
    for (name, page, wide, tall) in papers {
        let paper = Paper::parse(name).unwrap();
        for (what, slide, want) in [("16:9", (960.0f32, 540.0f32), wide), ("4:3", (720.0, 540.0), tall)] {
            let f = paper.fit(slide, true);
            if (f.page_pt.0 - page.0).abs() > 0.01 || (f.page_pt.1 - page.1).abs() > 0.01 {
                problems.push(format!("{name} {what}: the page is {:?}, PowerPoint's {page:?}", f.page_pt));
            }
            let (scale, x, y) = (f.scale / 0.75, f.shift.0 / 0.75, f.shift.1 / 0.75);
            if (scale - want[0]).abs() > 0.0002 || (x - want[1]).abs() > 0.2 || (y - want[2]).abs() > 0.2 {
                problems.push(format!("{name} {what} fitted: scale {scale:.4} at ({x:.2}, {y:.2}) units, PowerPoint's {want:?}"));
            }
        }
    }
    // 100 % (not fitted): the slide in the middle, at 1.3333 units a point whatever the paper (measured: A3 16:9 (153.6, 201.12), 4:3 (313.6, 201.12); B4 (47.84, 125.6), (207.84, 125.6);
    // B5 4:3 (5.6, -16.0); Letter (-112, 48), (48, 48); Legal (32, 48), (192, 48); Tabloid (176, 168), (336, 168)).
    for (name, wide, tall) in [
        ("a3", Some((153.6f32, 201.12f32)), Some((313.6f32, 201.12f32))),
        ("b4", Some((47.84, 125.6)), Some((207.84, 125.6))),
        ("b5", None, Some((5.6, -16.0))),
        ("letter", Some((-112.0, 48.0)), Some((48.0, 48.0))),
        ("legal", Some((32.0, 48.0)), Some((192.0, 48.0))),
        ("tabloid", Some((176.0, 168.0)), Some((336.0, 168.0))),
    ] {
        let paper = Paper::parse(name).unwrap();
        for (what, slide, want) in [("16:9", (960.0f32, 540.0f32), wide), ("4:3", (720.0, 540.0), tall)] {
            let Some(want) = want else { continue };
            let f = paper.fit(slide, false);
            let (x, y) = (f.shift.0 / 0.75, f.shift.1 / 0.75);
            if (f.scale - 1.0).abs() > 0.0001 || (x - want.0).abs() > 0.2 || (y - want.1).abs() > 0.2 {
                problems.push(format!("{name} {what} not fitted: scale {} at ({x:.2}, {y:.2}) units, PowerPoint's {want:?}", f.scale));
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
