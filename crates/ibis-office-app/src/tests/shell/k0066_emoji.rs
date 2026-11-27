// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0066: emoji show (in colour) on the iPhone simulator, and wherever else the app runs (design: `tasks/doc/test-design/K0066.md`; the numbers
//! E1... are that document's). These run on the Mac: what decides a character is an emoji and what is drawn with it (the cluster rules, the colour
//! of what is on the screen and in a PDF); the iOS simulator and device are checked by the user (D1... of the design). Each is `#[ignore]`d (listed in
//! `ignored()` only) until the build shows whether it fails now; the ones that pass move to `all()` as regression tests.
use crate::docx::ext::*;
use crate::render::rich_text::is_emoji_cluster;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0066_e1_which_clusters_are_emoji", e1_cluster_rules),
        ("shell::k0066_e2_an_emoji_sequence_is_one_picture_not_its_parts", e2_one_picture_per_sequence),
        ("shell::k0066_e3_the_variation_selector_decides_the_presentation", e3_selector),
        ("shell::k0066_e4_emoji_are_in_colour_in_a_cell_a_shape_and_a_document", e4_three_screens),
        ("shell::k0066_e5_an_emoji_committed_by_the_input_method_is_in_colour", e5_ime),
        ("shell::k0066_e6_an_exported_pdf_has_the_emoji_in_colour", e6_pdf),
        ("shell::k0066_e7_one_picture_is_one_character_for_the_caret_and_backspace", e7_caret),
    ]
}

/// A pixel with colour: the channels differ a good deal (black text, white paper and grey UI are not).
fn coloured(p: &image::Rgba<u8>) -> bool {
    let (max, min) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
    max as i32 - min as i32 > 60
}

fn colour_count(img: &image::RgbaImage) -> usize {
    img.pixels().filter(|p| coloured(p)).count()
}

/// The picture of the document area of the scenario's window (the screenshot cropped to the workspace: the chrome has coloured parts of its own).
fn shot(s: &mut Scenario, label: &str) -> image::RgbaImage {
    let img = image::open(s.screenshot(label)).unwrap().to_rgba8();
    let (ws, window_width) = s.with(|e, _| (e.workspace_bounds, f32::from(e.workspace_bounds.map_or(gpui_kit::px(1200.), |b| b.right() + gpui_kit::px(1.)))));
    let Some(ws) = ws else { return img };
    let scale = img.width() as f32 / window_width.max(1.0);
    let (x, y) = ((f32::from(ws.left()) * scale) as u32, (f32::from(ws.top()) * scale) as u32);
    let (w, h) = (((f32::from(ws.size.width) * scale) as u32).min(img.width() - x), ((f32::from(ws.size.height) * scale) as u32).min(img.height() - y));
    image::imageops::crop_imm(&img, x, y, w, h).to_image()
}

/// For each of the ten big lines of the document (found by the dark rows of the letters' picture): the marked pixels of the emoji (coloured with the emoji, not
/// with letters in the same place: the chrome's colours and the text's fringes are in both) as (count, min x, max x), `None` when the line has none.
fn emoji_lines() -> Vec<Option<(u32, u32, u32)>> {
    let mut pictures = Vec::new();
    for (name, label) in [("k0066-emoji-big", "k0066-mask-emoji"), ("k0066-letters-big", "k0066-mask-letters")] {
        let mut s = Scenario::open(&format!("test/editing/{name}.docx"));
        s.size(1200., 1500.);
        s.settle();
        pictures.push(shot(&mut s, label));
    }
    let (with, without) = (&pictures[0], &pictures[1]);
    let (w, h) = (with.width().min(without.width()), with.height().min(without.height()));
    // The lines: runs of rows that have dark pixels in the letters' picture.
    let dark = |p: &image::Rgba<u8>| (p[0] as u32 + p[1] as u32 + p[2] as u32) < 240;
    let mut lines: Vec<(u32, u32)> = Vec::new();
    let mut open: Option<(u32, u32)> = None;
    for y in 0..h {
        let any = (0..w).filter(|x| dark(without.get_pixel(*x, y))).count() > 10;
        match (any, open) {
            (true, None) => open = Some((y, y)),
            (true, Some((a, _))) => open = Some((a, y)),
            (false, Some((a, b))) if y > b + 12 => {
                lines.push((a, b));
                open = None;
            }
            _ => {}
        }
    }
    lines.extend(open);
    assert!(lines.len() >= 10, "the ten lines of the letters' document are found: {lines:?}");
    let strong = |p: &image::Rgba<u8>| p[0].max(p[1]).max(p[2]) as i32 - p[0].min(p[1]).min(p[2]) as i32 > 45;
    lines
        .iter()
        .take(10)
        .map(|(a, b)| {
            let (mut n, mut lo, mut hi) = (0u32, u32::MAX, 0u32);
            // Each line's rows, and a margin above and below for the picture that is taller than the letters.
            for y in a.saturating_sub(20)..(*b + 20).min(h) {
                for x in 0..w {
                    if strong(with.get_pixel(x, y)) && !strong(without.get_pixel(x, y)) {
                        n += 1;
                        lo = lo.min(x);
                        hi = hi.max(x);
                    }
                }
            }
            (n > 800).then_some((n, lo, hi))
        })
        .collect()
}

/// E1: the clusters that are drawn as an emoji and the ones that are not (Unicode's emoji-data: `Emoji_Presentation`; the selectors U+FE0F and U+FE0E;
/// joiners; flags; keycaps; skin tones).
fn e1_cluster_rules() {
    let yes = [
        "😀", "🎉", "🍣", "🚀", "👍", "👍🏽", "👨\u{200d}👩\u{200d}👧", "👩\u{200d}💻", "🏳\u{fe0f}\u{200d}🌈", "🇯🇵", "🇺🇸", "🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
        "1\u{fe0f}\u{20e3}", "#\u{fe0f}\u{20e3}", "*\u{fe0f}\u{20e3}", "🔟", "❤\u{fe0f}", "☺\u{fe0f}", "©\u{fe0f}", "✨", "⌛", "⭐", "⚽", "❌", "🀄", "🃏", "🆗", "🈁",
    ];
    let no = ["A", "a", "1", "#", "*", "漢", "あ", "ア", "©", "™", "❤", "☺", "❤\u{fe0e}", "⌛\u{fe0e}", "♠", "☎", "→", "★"];
    let mut bad = Vec::new();
    for s in yes {
        if !is_emoji_cluster(&s.chars().collect::<Vec<_>>()) {
            bad.push(format!("{s:?} should be an emoji"));
        }
    }
    for s in no {
        if is_emoji_cluster(&s.chars().collect::<Vec<_>>()) {
            bad.push(format!("{s:?} should not be an emoji"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// E2: in the document of ten big lines (😀, family, flag, skin tone, keycap, the heart in three presentations, the sparkles, the hourglass in text presentation),
/// the pictures of the sequences that are one emoji are as wide as the smiley (a family or a flag that came out as its parts would be two or three times as wide;
/// a flag that came out as two boxed letters has no colour at all).
fn e2_one_picture_per_sequence() {
    let lines = emoji_lines();
    let width = |l: Option<(u32, u32, u32)>| l.map(|(_, lo, hi)| (hi - lo + 1) as f32);
    let smiley = width(lines[0]).expect("the smiley is coloured");
    let mut bad = Vec::new();
    // Windows has no flag pictures: Segoe UI Emoji draws the two regional indicators as the plain letters "JP", in Word's PDF as well (black, no colour:
    // tasks/doc/reports/2026-11-26-k0066-windows-word-emoji.md). So on Windows the flag line must have no colour; elsewhere it is a coloured picture.
    #[cfg(windows)]
    if lines[2].is_some() {
        bad.push("flag: coloured, where Windows draws the letters JP".to_string());
    }
    #[cfg(windows)]
    let sequences = [("family", 1usize), ("skin tone", 3), ("keycap", 4)];
    #[cfg(not(windows))]
    let sequences = [("family", 1usize), ("flag", 2), ("skin tone", 3), ("keycap", 4)];
    for (name, index) in sequences {
        match width(lines[index]) {
            None => bad.push(format!("{name}: no coloured picture")),
            Some(w) if !(0.6..=1.45).contains(&(w / smiley)) => bad.push(format!("{name}: {:.2} times the smiley's width", w / smiley)),
            Some(_) => {}
        }
    }
    assert!(bad.is_empty(), "{bad:#?} (lines: {lines:?})");
}

/// E3: with U+FE0F after it the heart is an emoji (colour), bare or with U+FE0E it is a plain glyph (no colour); the sparkles (an emoji by default) are in
/// colour; the hourglass with U+FE0E is plain.
fn e3_selector() {
    let lines = emoji_lines();
    let coloured: Vec<bool> = lines.iter().map(|l| l.is_some()).collect();
    // ❤️ (line 6) coloured, ❤ (7) and ❤︎ (8) plain, ✨ (9) coloured, ⌛︎ (10) plain.
    assert_eq!(&coloured[5..10], &[true, false, false, true, false], "the heart with U+FE0F, the bare heart, the heart with U+FE0E, the sparkles, the text hourglass: {lines:?}");
}

/// E4: an emoji is in colour on the three screens: in a cell of the sheet (typed in), in a text box of a slide (a file), and in the document (against the same
/// document with letters: the colour has to come from the emoji).
fn e4_three_screens() {
    // The document: the same ten lines, with the emoji and with letters.
    let mut s = Scenario::open("test/editing/k0066-emoji-big.docx");
    s.size(1200., 1000.);
    s.settle();
    let with = colour_count(&shot(&mut s, "k0066-e4-docx"));
    drop(s);
    let mut s = Scenario::open("test/editing/k0066-letters-big.docx");
    s.size(1200., 1000.);
    s.settle();
    let without = colour_count(&shot(&mut s, "k0066-e4-docx-letters"));
    assert!(with > without + 2000, "the document: {with} coloured pixels with the emoji against {without} with letters");
    drop(s);
    // The slide: the plan deck's title with emoji against the plan deck.
    let mut s = Scenario::open("test/editing/k0066-emoji.pptx");
    s.size(1200., 900.);
    s.settle();
    let with = colour_count(&shot(&mut s, "k0066-e4-pptx"));
    drop(s);
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 900.);
    s.settle();
    let without = colour_count(&shot(&mut s, "k0066-e4-pptx-plan"));
    assert!(with > without + 500, "the slide: {with} against {without}");
    drop(s);
    // The sheet: an emoji typed in a cell of a new sheet.
    let mut s = Scenario::new_document(ibis_formats::Kind::Workbook);
    s.size(1200., 900.);
    s.settle();
    let without = colour_count(&shot(&mut s, "k0066-e4-xlsx-before"));
    s.type_text("😀");
    s.key("enter");
    let with = colour_count(&shot(&mut s, "k0066-e4-xlsx-after"));
    assert!(with > without + 40, "the cell: {with} against {without}");
}

/// E5: an emoji committed by the input method (composing, then the emoji) is in colour in the document.
fn e5_ime() {
    use crate::docx::scenarios::DocSteps;
    let mut s = Scenario::open("test/editing/k0066-letters-big.docx");
    s.size(1200., 900.);
    s.settle();
    // The caret in the first line.
    let (id, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    let without = colour_count(&shot(&mut s, "k0066-e5-before"));
    s.ime("りんご", "🍎");
    let text = s.paragraph(0).1;
    assert!(text.contains('🍎'), "the committed emoji is in the text: {text:?}");
    let with = colour_count(&shot(&mut s, "k0066-e5-after"));
    assert!(with > without + 40, "{with} coloured pixels against {without}");
}

/// E6: the document exported as a PDF shows the emoji in colour when the PDF is opened again (against the PDF of the same document with letters).
fn e6_pdf() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = root.join("target/k0066-pdf");
    let _ = std::fs::remove_dir_all(&out);
    let mut counts = Vec::new();
    for name in ["k0066-emoji-big", "k0066-letters-big"] {
        let args = crate::parse_args_from(vec!["--export".into(), "pdf".into(), "--out-dir".into(), out.display().to_string(), root.join(format!("test/editing/{name}.docx")).display().to_string()]).unwrap();
        crate::run_export(&args, "pdf").unwrap();
        let pdf = out.join(format!("{name}.pdf"));
        assert!(pdf.exists(), "{name}.pdf is written");
        let mut s = Scenario::open(pdf.to_str().unwrap());
        s.size(1200., 1000.);
        s.settle();
        counts.push(colour_count(&shot(&mut s, &format!("k0066-e6-{name}"))));
    }
    assert!(counts[0] > counts[1] + 2000, "the PDF with the emoji has {} coloured pixels, the one with letters {}", counts[0], counts[1]);
}

/// E7: a sequence that is one picture (a family, a flag, a skin tone, a keycap, a tag flag) is one character for the caret and Backspace: after the input method puts one
/// in the text, one Backspace takes the whole picture away, and ← moves over the whole picture (a typed "x" lands before it).
fn e7_caret() {
    use crate::docx::scenarios::DocSteps;
    let sequences = ["\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}", "\u{1F1EF}\u{1F1F5}", "\u{1F44D}\u{1F3FD}", "1\u{FE0F}\u{20E3}", "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}"];
    for sequence in sequences {
        let mut s = Scenario::open("test/editing/k0066-letters-big.docx");
        s.size(1200., 900.);
        s.settle();
        let (id, _) = s.paragraph(0);
        s.click_text(ibis_doc_model::FlowPos::new(id, 0));
        let before = s.paragraph(0).1;
        s.ime("あ", sequence);
        assert_eq!(s.paragraph(0).1, format!("{sequence}{before}"), "the picture is in the text");
        s.key("backspace");
        assert_eq!(s.paragraph(0).1, before, "one Backspace takes the whole picture of {sequence:?} away");
        s.ime("あ", sequence);
        s.key("left");
        s.type_text("x");
        assert_eq!(s.paragraph(0).1, format!("x{sequence}{before}"), "one \u{2190} moves over the whole picture of {sequence:?}");
        drop(s);
    }
}
