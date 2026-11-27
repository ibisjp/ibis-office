// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #208: text beside a SmartArt anchored with `wrapSquare` (Word's own sample `test/editing/docx-smartart-word-square.docx`, its PDF beside it): a word the
//! narrow gap left of the figure cannot hold goes to the gap on its right whole, the next word that fits goes to the left gap.

use crate::docx::flow_layout::Item;

/// A sample of `test/editing/` laid out by a headless app as the export does it: the text items of the first page (x, y, text).
fn texts_of(file: &str) -> Vec<(f32, f32, String)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(file);
    let mut presentation = ibis_formats::read(&std::fs::read(path).expect("the file")).expect("a document");
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let resources = cx.update(|app| {
        let mut resources = crate::render::shape_view::Resources::new(&presentation, app);
        resources.print = true;
        resources.exact = true;
        resources.preload.wait_all();
        resources
    });
    crate::docx::flow_layout::update_fields_on_open(&mut presentation, &resources);
    let pages = crate::docx::flow_layout::pages(presentation.flow.as_ref().expect("a flowing document"), &resources);
    let mut out = Vec::new();
    fn walk(items: &[Item], out: &mut Vec<(f32, f32, String)>) {
        for i in items {
            match i {
                Item::Text { x, y, text, .. } => out.push((*x, *y, text.to_string())),
                _ => {}
            }
        }
    }
    walk(&pages.pages[0].items, &mut out);
    out
}

#[test]
fn a_word_too_wide_for_the_gap_beside_a_figure_goes_to_the_next_gap_whole() {
    let texts = texts_of("docx-smartart-word-square.docx");
    let at = |word: &str| texts.iter().find(|(_, _, t)| t.trim() == word).map(|(x, _, _)| *x).unwrap_or_else(|| panic!("{word:?} is drawn whole: {texts:?}"));
    // Word's PDF (pdftotext -bbox, 72 pt = 1 in): "process" 86 pt from the page's left, "SmartArt" and "(square)" at 444 pt.
    assert!((at("process") - 86.0).abs() < 6.0, "process: {}", at("process"));
    assert!((at("SmartArt") - 444.0).abs() < 6.0, "SmartArt: {}", at("SmartArt"));
    assert!((at("(square)") - 444.0).abs() < 6.0, "(square): {}", at("(square)"));
}

/// Three SmartArt frames anchored by `wp:align` (center of the margins, right of the page / middle of the page, left of the column / bottom of the margins: Word's PDF
/// `docx-smartart-word-align.pdf`, made from `docx-smartart-word-square.docx` with the offsets replaced by alignments): the shapes' centres are where the centres of the
/// texts Word draws in them are, within 1 pt across.
#[test]
fn smartart_frames_placed_by_an_alignment_have_their_shapes_where_word_puts_them() {
    let shapes = shapes_of("docx-smartart-word-align.docx");
    // (the text, the centre of Word's text across: pdftotext -bbox of the PDF)
    for (word, word_x) in [("春", 297.62f32), ("部長", 445.37), ("計画", 124.75), ("設計", 235.11)] {
        let (x, w, _) = shapes.iter().find(|(_, _, t)| t.contains(word)).unwrap_or_else(|| panic!("{word:?} is in a shape: {}", shapes.len()));
        assert!((x + w / 2.0 - word_x).abs() < 1.0, "{word}: {} against Word {word_x}", x + w / 2.0);
    }
}

/// An inline SmartArt of 248.04 pt on an 18 pt line grid (1.079 lines) is a line of 14 grids, 252 pt, in Word (its PDF `docx-smartart-word-inline.pdf`): the line of text after it is 304.01 pt
/// below the one before it (the two lines of 11 pt text, their two paragraph spaces of 8 pt and the paragraph of the figure).
#[test]
fn an_inline_smartart_on_a_grid_takes_whole_grids() {
    let texts = texts_of("docx-smartart-word-inline.docx");
    let y_of = |start: &str| texts.iter().find(|(_, _, t)| t.trim_start().starts_with(start)).map(|(_, y, _)| *y).unwrap_or_else(|| panic!("{start:?} is drawn: {texts:?}"));
    let pitch = y_of("SmartArt org") - y_of("SmartArt process");
    assert!((pitch - 304.01).abs() < 0.2, "the figure's paragraph: {pitch} against Word 304.01");
}

/// The shapes of the first page: (x, width, the debug text of the shape).
fn shapes_of(file: &str) -> Vec<(f32, f32, String)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(file);
    let mut presentation = ibis_formats::read(&std::fs::read(path).expect("the file")).expect("a document");
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let resources = cx.update(|app| {
        let mut resources = crate::render::shape_view::Resources::new(&presentation, app);
        resources.print = true;
        resources.exact = true;
        resources.preload.wait_all();
        resources
    });
    crate::docx::flow_layout::update_fields_on_open(&mut presentation, &resources);
    let pages = crate::docx::flow_layout::pages(presentation.flow.as_ref().expect("a flowing document"), &resources);
    pages.pages[0].items.iter().filter_map(|i| match i {
        Item::Shape { shape, x, w, .. } => Some((*x, *w, format!("{:?}", shape.kind))),
        _ => None,
    }).collect()
}

/// The shapes of the first page that hold text: (the debug text of the shape, the centre of the shape across the page).
fn shape_centres(file: &str) -> Vec<(String, f32)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(file);
    let mut presentation = ibis_formats::read(&std::fs::read(path).expect("the file")).expect("a document");
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let resources = cx.update(|app| {
        let mut resources = crate::render::shape_view::Resources::new(&presentation, app);
        resources.print = true;
        resources.exact = true;
        resources.preload.wait_all();
        resources
    });
    crate::docx::flow_layout::update_fields_on_open(&mut presentation, &resources);
    let pages = crate::docx::flow_layout::pages(presentation.flow.as_ref().expect("a flowing document"), &resources);
    pages.pages[0].items.iter().filter_map(|i| match i {
        Item::Shape { shape, y, h, .. } => Some((format!("{:?}", shape.kind), *y + *h / 2.0)),
        _ => None,
    }).collect()
}

/// The top of the words of the PDF `file` exports to (`pdftotext -bbox`, points from the page's top), by the word.
fn pdf_word_tops(file: &str) -> std::collections::HashMap<String, f32> {
    use crate::export::{Format, Options, export};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data = std::fs::read(root.join("test/editing").join(file)).expect("the file");
    let presentation = ibis_formats::read(&data).expect("a document");
    let out_dir = root.join("target/scenario/smartart_text");
    std::fs::create_dir_all(&out_dir).unwrap();
    let name = std::path::Path::new(file).file_stem().unwrap().to_string_lossy().into_owned();
    let options = Options {
        format: Format::Pdf,
        out_dir: out_dir.clone(),
        name: name.clone(),
        width: 1920,
        slides: Vec::new(),
        layout: false,
        file_name: Some(file.into()),
        print_backgrounds: false,
        print_hidden_text: false,
        print_markup: true,
        color: Default::default(),
        handout: None,
        frame: false,
        handout_landscape: false,
        paper: None,
        fit_paper: true,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    export(presentation, &options).expect("the PDF");
    let pdf = out_dir.join(format!("{name}.pdf"));
    let out = std::process::Command::new("pdftotext").args(["-f", "1", "-l", "1", "-bbox"]).arg(&pdf).arg("-").output().expect("pdftotext (poppler) is needed");
    let xml = String::from_utf8_lossy(&out.stdout).into_owned();
    let mut tops = std::collections::HashMap::new();
    for line in xml.lines().map(str::trim).filter(|l| l.starts_with("<word ")) {
        let top: f32 = line.split("yMin=\"").nth(1).and_then(|s| s.split('"').next()).and_then(|s| s.parse().ok()).unwrap();
        let word = line.split_once('>').unwrap().1.trim_end_matches("</word>").to_string();
        tops.entry(word).or_insert(top);
    }
    tops
}

/// The text in the shapes of a SmartArt is where Word puts it in them (Word's PDFs `docx-smartart-word-inline.pdf`, `docx-smartart-word-align.pdf`; the text's top, as
/// `pdftotext -bbox` gives it, from the centre of the shape it is in, the shape's from the PDF's own drawing of it: 計画 29 pt 17.105 pt above the centre of its shape, 部長 58 pt
/// 34.159). Word's text is a line of the font's own height (ascent plus descent, 90 % of it) with a quarter of it below the baseline, the line centred, not on the document's line grid
/// (`linePitch`): the text was 1 to 9 pt lower here.
#[test]
fn the_text_of_a_smartart_shape_is_where_word_puts_it_in_the_shape() {
    // (file, word, Word's distance of the text's top from the shape's centre)
    for (file, word, word_dy) in [
        ("docx-smartart-word-inline.docx", "計画", -17.105f32),
        ("docx-smartart-word-inline.docx", "部長", -34.159),
        ("docx-smartart-word-inline.docx", "課長A", -34.108),
        ("docx-smartart-word-align.docx", "計画", -11.765),
        ("docx-smartart-word-align.docx", "部長", -21.734),
        ("docx-smartart-word-align.docx", "春", -11.2),
    ] {
        let tops = pdf_word_tops(file);
        let centres = shape_centres(file);
        let top = tops.get(word).unwrap_or_else(|| panic!("{word:?} is in the PDF of {file}: {tops:?}"));
        let first = word.chars().next().unwrap().to_string();
        let (_, centre) = centres.iter().find(|(t, _)| t.contains(&first)).unwrap_or_else(|| panic!("{word:?} is in a shape of {file}"));
        assert!((top - centre - word_dy).abs() < 0.6, "{file} {word}: {} from the centre of its shape, Word {word_dy}", top - centre);
    }
}
