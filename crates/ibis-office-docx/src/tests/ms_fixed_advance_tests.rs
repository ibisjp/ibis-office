// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A character of ＭＳ 明朝 / ＭＳ ゴシック takes the advance its font file gives it (GitHub issue #167, Word's PDF of `ms-fixed-width.docx`): the Greek and
//! Cyrillic letters and Ǎ one em, the dashes, •, − and the arrows half an em, not by the Unicode range.

use super::flow_layout::Item;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::ms_fixed_advance_follows_the_font_file", ms_fixed_advance_follows_the_font_file),
        ("docx::ms_fixed_advance_in_the_layout_of_ms_fixed_width", ms_fixed_advance_in_the_layout_of_ms_fixed_width),
        ("docx::combining_marks_of_ms_mincho_take_a_cell_each", combining_marks_of_ms_mincho_take_a_cell_each),
        ("docx::vertical_copyright_and_registered_are_turned_half_cells", vertical_copyright_and_registered_are_turned_half_cells),
        ("docx::vertical_literal_soft_hyphen_is_a_hyphen_cell", vertical_literal_soft_hyphen_is_a_hyphen_cell),
        ("docx::marks_without_a_base_advance_as_words_probes_do", marks_without_a_base_advance_as_words_probes_do),
    ]
}

fn ms_fixed_advance_follows_the_font_file() {
    let mut s = Scenario::open("test/editing/ms-fixed-width.docx");
    s.with(|e, _| {
    let res = &e.resources;
    for face in ["\u{ff2d}\u{ff33} \u{660e}\u{671d}", "\u{ff2d}\u{ff33} \u{30b4}\u{30b7}\u{30c3}\u{30af}"] {
        // Greek, Cyrillic, Ǎ, ‰, ×, §: one em.
        for c in ['\u{391}', '\u{3b1}', '\u{430}', '\u{1cd}', '\u{2030}', '\u{d7}', '\u{a7}'] {
            assert_eq!(super::flow_layout::advance_em(res, Some(face), c), 1.0, "U+{:04X} in {face}", c as u32);
        }
        // The dashes, •, ‼, −, ∅, ↔, ▶, ♠, €, ASCII: half an em.
        for c in ['\u{2014}', '\u{2013}', '\u{2011}', '\u{2022}', '\u{203c}', '\u{2212}', '\u{2205}', '\u{2194}', '\u{25b6}', '\u{2660}', '\u{20ac}', 'a', '1'] {
            assert_eq!(super::flow_layout::advance_em(res, Some(face), c), 0.5, "U+{:04X} in {face}", c as u32);
        }
    }
    });
}

fn ms_fixed_advance_in_the_layout_of_ms_fixed_width() {
    let mut s = Scenario::open("test/editing/ms-fixed-width.docx");
    // The text of each line, with each character's x.
    let lines: Vec<Vec<(char, f32)>> = s.with(|e, _| {
        let pages = super::flow_layout::pages(e.flow().expect("a document"), &e.resources);
        let mut lines = Vec::new();
        for page in &pages.pages {
            for it in &page.items {
                if let Item::Text { x, text, offsets, .. } = it {
                    lines.push(text.chars().enumerate().map(|(k, c)| (c, x + offsets[k])).collect());
                }
            }
        }
        lines
    });
    let size = 10.5_f32;
    let box_to_box = |line: &[(char, f32)]| -> Vec<f32> {
        let boxes: Vec<f32> = line.iter().filter(|(c, _)| *c == '\u{53e3}').map(|(_, x)| *x).collect();
        boxes.windows(2).map(|w| w[1] - w[0]).collect()
    };
    let with = |c: char| lines.iter().filter(|l| l.iter().any(|(k, _)| *k == c)).collect::<Vec<_>>();
    // Greek, Cyrillic and Ǎ: the box (1 em) and the letter (1 em), and the quarter em beside the letter on each side (#166): about 26.25 pt (Word: 26.28).
    for l in with('\u{391}') {
        for d in box_to_box(l) {
            assert!((d - 26.25).abs() < 0.4, "box to box with a letter of 1 em: {d}");
        }
    }
    // The dashes, •, −, ↔, €: the box and the half em letter, no space: 15.75 pt (Word: 15.72 to 15.86).
    for l in with('\u{2014}') {
        for d in box_to_box(l) {
            assert!((d - (size + size / 2.0)).abs() < 0.4, "box to box with a letter of half an em: {d}");
        }
    }
}

/// The combining marks of ＭＳ 明朝 (U+0300..036F, 0.5 em in the font file) are laid out one to a cell, not joined to the character before them (#M125, Word's PDF of
/// `fullwidth-symbols-all.docx`: after έ the marks of JIS X 0213 row 11 each advance 5.28 pt of the 10.56 pt text): a mark is an item of its own, the next
/// character half an em on.
fn combining_marks_of_ms_mincho_take_a_cell_each() {
    let mut s = Scenario::open("test/editing/fullwidth-symbols-all.docx");
    let marks: Vec<(char, f32)> = s.with(|e, _| {
        let pages = super::flow_layout::pages(e.flow().expect("a document"), &e.resources);
        let mut found = Vec::new();
        for page in pages.pages.iter().take(2) {
            for it in &page.items {
                if let Item::Text { text, offsets, size, .. } = it
                    && let Some(first) = text.chars().next()
                    && matches!(first, '\u{361}' | '\u{306}')
                {
                    // The item's last offset is where its next cell begins: the sum of the advances of its characters.
                    found.push((first, (offsets[offsets.len() - 1] - offsets[0]) / size / text.chars().count() as f32));
                }
            }
        }
        found
    });
    assert!(!marks.is_empty(), "the marks are items of their own");
    for (c, em) in marks {
        // (The next character's own inset, where its glyph is narrower than its cell, is within a quarter em.)
        assert!((em - 0.5).abs() < 0.26, "U+{:04X} takes half an em: {em}", c as u32);
    }
}

/// The turned items (x, y, character) and the upright ones (x, baseline y, character) of the vertical ＭＳ 明朝 section of the marked copy (page 4 of the 9).
fn vertical_items() -> (Vec<(f32, f32, char)>, Vec<(f32, f32, char)>) {
    let mut s = Scenario::open("test/editing/fullwidth-symbols-all-flagged.docx");
    s.with(|e, _| {
        let pages = super::flow_layout::pages(e.flow().expect("a document"), &e.resources);
        let (mut turned, mut upright) = (Vec::new(), Vec::new());
        for it in &pages.pages[3].items {
            match it {
                Item::Turned { x, y, text, .. } => turned.extend(text.chars().map(|c| (*x, *y, c))),
                Item::Text { x, y, text, .. } => upright.extend(text.chars().map(|c| (*x, *y, c))),
                _ => {}
            }
        }
        (turned, upright)
    })
}

/// Word turns © and ® in vertical ＭＳ 明朝 and gives each the half em of the font (Word's PDF of the marked copy, #M136: both `R`, 5.28 pt of 10.56): the next
/// character of the column is half an em on (the glyph of the substitute typeface may sit a little off the middle of its cell).
fn vertical_copyright_and_registered_are_turned_half_cells() {
    let (turned, _) = vertical_items();
    let size = 10.5_f32;
    for (c, next) in [('\u{a9}', '\u{aa}'), ('\u{ae}', '\u{af}')] {
        let (x, y, _) = *turned.iter().find(|t| t.2 == c).expect("the character is turned");
        let (_, y2, _) = *turned.iter().find(|t| t.2 == next && (t.0 - x).abs() < 1.0).expect("the next character is turned in the same column");
        assert!((y2 - y - size / 2.0).abs() < 1.5, "U+{:04X} to the next: {}", c as u32, y2 - y);
    }
}

/// A U+00AD in `w:t` is no optional hyphen (the `w:softHyphen` element is; the model keeps them apart): Word draws a turned hyphen that takes the half em of the font, so « - ® are one half em apart each (Word's PDF of the
/// marked copy, #M136: « 769.92, the hyphen 764.64, ® 759.36 on the page).
fn vertical_literal_soft_hyphen_is_a_hyphen_cell() {
    let (turned, _) = vertical_items();
    let size = 10.5_f32;
    let (x, y_open, _) = *turned.iter().find(|t| t.2 == '\u{ab}').expect("the guillemet is turned");
    let (_, y_hyphen, _) = *turned.iter().find(|t| t.2 == '-' && (t.0 - x).abs() < 1.0).expect("the soft hyphen is drawn");
    let (_, y_reg, _) = *turned.iter().find(|t| t.2 == '\u{ae}' && (t.0 - x).abs() < 1.0).expect("the next character");
    assert!((y_hyphen - y_open - size / 2.0).abs() < 1.5 && (y_reg - y_hyphen - size / 2.0).abs() < 1.5, "{y_open} {y_hyphen} {y_reg}");
}

/// Where the character after the marks of each paragraph of one of Word's probe documents ends: the position along the line (x in a line, y in a vertical column) of the
/// character that follows it (the paragraph is marks, a character, 亜, ＃ and two digits), by (vertical, paragraph number).
fn after_next_positions(file: &str) -> std::collections::HashMap<(bool, u32), f32> {
    let mut s = Scenario::open(file);
    s.with(|e, _| {
        let pages = super::flow_layout::pages(e.flow().expect("a document"), &e.resources);
        let mut found = std::collections::HashMap::new();
        for page in &pages.pages {
            let vertical = page.items.iter().any(|it| matches!(it, Item::Turned { .. }));
            let mut glyphs: Vec<(char, f32)> = Vec::new();
            for it in &page.items {
                match it {
                    Item::Turned { x, y, text, .. } => glyphs.extend(text.chars().map(|c| (c, if vertical { *y } else { *x }))),
                    Item::Text { x, y, text, offsets, .. } => glyphs.extend(text.chars().zip(offsets.iter()).map(|(c, o)| (c, if vertical { *y } else { x + o }))),
                    _ => {}
                }
            }
            for w in glyphs.windows(4) {
                let digit = |c: char| (0xff10..=0xff19).contains(&(c as u32)).then(|| c as u32 - 0xff10);
                if w[1].0 == '\u{ff03}' && let (Some(a), Some(b)) = (digit(w[2].0), digit(w[3].0)) {
                    // An upright character's y is its baseline, 0.88 em under the top of its cell (as in the layout's own rule); a turned one's is its cell's top.
                    let at = if vertical { w[0].1 - 10.5 * 0.88 } else { w[0].1 };
                    found.insert((vertical, a * 10 + b), at);
                }
            }
        }
        found
    })
}

/// Word's cells for the marks of #M138's three probe documents (`tasks/doc/m54-marked/m138/`: `m138-results.tsv`, the cell of each run of marks): in each group of
/// paragraphs with the same place (paragraph start, after a line break, after a tab, after 亜), the same character after the marks and the same direction, the
/// character after the marks stands as much further along the line than in the group's first paragraph as Word's cell for the marks says (within 0.3 pt of 10.5 pt
/// text; Word sets 10.56). This is the horizontal cell of every group, and the vertical cell of every one but the line-start marks before the symbols that stay
/// upright, which share one em (`baseless_marks_share_a_cell`: ❶ ① ○ ★ ■ ※ ✓, one em for 1 to 4 marks, in the TSV) and are in the same test. The two cases ours does
/// not follow (the Hangul tone marks U+302E and U+302F in a line: Word's 1.24 em, ours one em) are listed in `OPEN`.
fn marks_without_a_base_advance_as_words_probes_do() {
    // (probe number, direction, case number) of the rows whose cell ours does not follow.
    const OPEN: &[(u32, char, u32)] = &[(1, 'H', 38), (1, 'H', 39), (1, 'H', 44), (1, 'H', 45), (1, 'H', 48), (1, 'H', 49), (1, 'H', 51), (1, 'H', 54)];
    let tsv = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tasks/doc/m54-marked/m138/m138-results.tsv")).expect("the TSV of Word's cells");
    let mut rows: Vec<(u32, char, u32, String, String, f32)> = Vec::new();
    for line in tsv.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        // The rows with no marks are the references of the paragraphs' places, not groups of their own.
        if f[3] == "ref" || f[3].ends_with("ref") {
            continue;
        }
        rows.push((f[0].parse().unwrap(), f[2].chars().next().unwrap(), f[1].parse().unwrap(), f[3].to_string(), f[5].to_string(), f[6].parse().unwrap()));
    }
    let mut wrong = Vec::new();
    for probe in 1..=3u32 {
        let file = if probe == 1 { "m138-nobase-probe".to_string() } else { format!("m138-nobase-probe{probe}") };
        let ours = after_next_positions(&format!("tasks/doc/m54-marked/m138/{file}.docx"));
        for dir in ['H', 'V'] {
            let mut first: std::collections::HashMap<(String, String), (u32, f32)> = std::collections::HashMap::new();
            for (p, d, case, place, next, word) in rows.iter().filter(|r| r.0 == probe && r.1 == dir) {
                let _ = (p, d);
                let Some(&mine) = ours.get(&(dir == 'V', *case)) else { wrong.push(format!("probe {probe} {dir} #{case}: not found")); continue };
                let Some(&(case0, word0)) = first.get(&(place.clone(), next.clone())) else {
                    first.insert((place.clone(), next.clone()), (*case, *word));
                    continue;
                };
                let mine0 = ours[&(dir == 'V', case0)];
                // A paragraph's place is where the line starts: the same in every paragraph of the group, so the distance from the group's first is the difference of the cells.
                let (mine_d, word_d) = (mine - mine0, word - word0);
                let open = OPEN.contains(&(probe, dir, *case));
                if open && (mine_d - word_d).abs() <= 0.3 {
                    wrong.push(format!("probe {probe} {dir} #{case} now follows Word: take it out of OPEN"));
                }
                if (mine_d - word_d).abs() > 0.3 && !open {
                    wrong.push(format!("probe {probe} {dir} #{case} ({place}): ours {mine_d:.2} pt further than #{case0}, Word {word_d:.2}"));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}
