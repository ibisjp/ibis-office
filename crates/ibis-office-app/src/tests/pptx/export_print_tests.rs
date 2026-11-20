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
        ("pptx::export_print::an_outline_bullet_pushes_the_text_by_its_typefaces_advance", an_outline_bullet_pushes_the_text_by_its_typefaces_advance),
        ("pptx::export_print::notes_pages_have_the_slide_above_and_its_notes_below", notes_pages_have_the_slide_above_and_its_notes_below),
        ("pptx::export_print::handout_pages", handout_pages),
        ("pptx::export_print::marks_go_where_the_handout_master_puts_them", marks_go_where_the_handout_master_puts_them),
        ("pptx::export_print::pdf_from_the_command_line_has_handouts_and_notes_pages", pdf_from_the_command_line_has_handouts_and_notes_pages),
        ("pptx::export_print::vector_pdf_prints_in_grey_and_black_and_white", vector_pdf_prints_in_grey_and_black_and_white),
        ("pptx::export_print::chart_bars_print_grey_with_a_black_line_in_grey_and_black_and_white", chart_bars_print_grey_with_a_black_line_in_grey_and_black_and_white),
        ("pptx::export_print::pure_black_and_white_draws_a_thin_black_edge_round_plain_fills", pure_black_and_white_draws_a_thin_black_edge_round_plain_fills),
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
    let marks = HandoutMarks { header: Some("Header".into()), date: Some("2026/10/11".into()), footer: Some("Footer".into()), number: true, layout: Some(layout), notes: None, vertical_order: false, slide_numbers: Vec::new(), slide_size_pt: None };
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
