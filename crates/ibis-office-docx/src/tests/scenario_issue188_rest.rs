// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #188, the rest (design: `tasks/doc/test-design/issue-188-rest.md`; the numbers A1... and B1... are that document's):
//! (A) running heads and feet and page numbers of a vertical (`tbRl`) section, (B) the advances of the sixteen space characters in Meiryo, Yu Mincho
//! and MS P Mincho. The values are Word for Windows' (tasks/doc/requests/2026-11-14-188-word-result.md, sections 3 and 6; tolerance 0.25 pt: Word's
//! PDFs have 0.12 pt steps). The (A) tests run with the others (`all()`); the (B) ones are `#[ignore]`d (`ignored()`) until the measurement they wait for arrives.
use super::flow_layout::Item;
use super::issue188_tests::{Margins, drawn, make_general, near, pages_of, texts};

/// The tests of (A) running heads and feet: they pass, so they run with the others.
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue188_a1_running_head_and_foot_boxes_of_a_vertical_section_in_eight_margin_sets", a1_boxes),
        ("docx::issue188_a2_gutter_does_not_move_a_vertical_sections_head_but_shrinks_its_box", a2_gutter),
        ("docx::issue188_a3_page_number_field_sits_in_the_middle_of_the_foot_across", a3_page_number),
        ("docx::issue188_a4_a_paragraph_direction_in_a_head_is_ignored", a4_paragraph_direction),
        ("docx::issue188_a5_cell_directions_in_a_head", a5_cell_directions),
        ("docx::issue188_a6_a_table_in_a_head_starts_a_cell_margin_in", a6_table_in_head),
        ("docx::issue188_a6b_a_table_in_a_head_has_no_cell_margin_without_the_default_table_style", a6b_table_without_style),
        ("docx::issue188_a7_a_document_grid_does_not_move_a_vertical_sections_head", a7_doc_grid),
        ("docx::issue188_a8_two_digit_page_number_moves_by_half_a_digit_across_alignments", a8_two_digit_page_numbers),
        ("docx::issue188_a9_indents_of_the_page_number_paragraph_move_its_box", a9_page_number_indents),
        ("docx::issue188_a10_even_odd_and_first_page_feet_sit_at_the_same_places", a10_even_odd_first),
        ("docx::issue188_a11_mirror_margins_swap_the_feet_box_by_page_in_a_vertical_section", a11_mirror),
        ("docx::issue188_a12_cell_margins_of_a_table_in_a_head_move_the_text_one_for_one", a12_cell_margins),
        ("docx::issue188_b1_space_advances_in_meiryo", b1_meiryo),
        ("docx::issue188_b2_space_advances_in_yu_mincho", b2_yu_mincho),
        ("docx::issue188_b3_space_advances_in_ms_p_mincho", b3_ms_p_mincho),
        ("docx::issue188_b4_the_two_fixed_spaces_are_the_same_in_every_face", b4_fixed_spaces),
        ("docx::issue188_b5_u2002_to_u2006_follow_the_en_and_em_dash_widths_of_the_face", b5_dash_rule),
    ]
}

/// The tests of (B) the spaces' advances in Meiryo, Yu Mincho and MS P Mincho: they wait for Windows' measurement (`tasks/doc/requests/2026-11-25-188-space-advance.md`).
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
    ]
}

const LEFT: &str = "left";
const CENTER: &str = "center";
const RIGHT: &str = "right";

/// A paragraph of `text` (ＭＳ 明朝 10.5 pt, from the document's defaults) aligned `jc`.
fn aligned(text: &str, jc: &str) -> String {
    format!(r#"<w:p><w:pPr><w:jc w:val="{jc}"/></w:pPr><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

/// The width of the head's text あいうABC: あいう 31.56 pt, the gap between the Japanese and the Latin 2.64 pt, ABC 15.78 pt.
const HEAD_WIDTH: f32 = 49.94;

/// (name, margins in twips, left, right, gutter in pt, the head's baseline, the foot's baseline) of the eight margin sets of Word's test (D6a): the head's baseline is
/// `header` + 10.56 pt, the foot's 841.92 - `footer` - 3.00 pt; the left aligned start `left` + 0.02 pt; the centre and the right aligned text in the box
/// [`left`, 595.32 - `right` - `gutter`] (a vertical section puts the gutter on the right).
struct Set {
    name: &'static str,
    m: Margins,
    left: f32,
    right: f32,
    gutter: f32,
    head_y: f32,
    foot_y: f32,
    /// Measured by Word (true) or from the rule above (false).
    measured: bool,
}

fn sets() -> Vec<Set> {
    let m = |left, right, header, footer, gutter| Margins { left, right, header, footer, gutter, ..Margins::default() };
    vec![
        Set { name: "l72-r72", m: m(1440, 1440, 720, 720, 0), left: 72.0, right: 72.0, gutter: 0.0, head_y: 46.56, foot_y: 802.92, measured: true },
        Set { name: "l72-r108", m: m(1440, 2160, 720, 720, 0), left: 72.0, right: 108.0, gutter: 0.0, head_y: 46.56, foot_y: 802.92, measured: true },
        Set { name: "l72-r72-g36", m: m(1440, 1440, 720, 720, 720), left: 72.0, right: 72.0, gutter: 36.0, head_y: 46.56, foot_y: 802.92, measured: true },
        Set { name: "l50-r90-g15", m: m(1000, 1800, 720, 720, 300), left: 50.0, right: 90.0, gutter: 15.0, head_y: 46.56, foot_y: 802.92, measured: true },
        Set { name: "l108-r72", m: m(2160, 1440, 720, 720, 0), left: 108.0, right: 72.0, gutter: 0.0, head_y: 46.56, foot_y: 802.92, measured: false },
        Set { name: "h18", m: m(1440, 1440, 360, 360, 0), left: 72.0, right: 72.0, gutter: 0.0, head_y: 28.56, foot_y: 820.92, measured: true },
        Set { name: "h54", m: m(1440, 1440, 1080, 1080, 0), left: 72.0, right: 72.0, gutter: 0.0, head_y: 64.56, foot_y: 784.92, measured: true },
        Set { name: "h25-f45", m: m(1440, 1440, 500, 900, 0), left: 72.0, right: 72.0, gutter: 0.0, head_y: 35.56, foot_y: 793.92, measured: true },
    ]
}

/// The x of the first character of the drawn あ near the baseline `y` (the head's or the foot's text), if any.
fn start_of_a_near(d: &[(f32, f32, String)], y: f32) -> Option<f32> {
    d.iter().find(|i| i.2 == "あ" && (i.1 - y).abs() < 1.5).map(|i| i.0)
}

/// A1: a running head and a running foot of a vertical section stay across, at the places Word puts them, for left, centre and right aligned paragraphs in
/// eight margin sets. The body (and so the margins) is vertical, the head's text あいうABC.
fn a1_boxes() {
    let mut bad = Vec::new();
    for set in sets() {
        for jc in [LEFT, CENTER, RIGHT] {
            let head = aligned("あいうABC", jc);
            let file = make_general(&format!("a1-{}-{jc}", set.name), &aligned("漢字", LEFT), true, "ＭＳ 明朝", set.m, Some(&head), Some(&head));
            let d = drawn(&pages_of(&file));
            let box_left = set.left;
            let box_right = 595.32 - set.right - set.gutter;
            let expected_x = match jc {
                LEFT => set.left + 0.02,
                CENTER => box_left + (box_right - box_left - HEAD_WIDTH) / 2.0,
                _ => box_right - HEAD_WIDTH,
            };
            let tolerance = if set.measured { 0.25 } else { 0.35 };
            for (what, y) in [("head", set.head_y), ("foot", set.foot_y)] {
                match start_of_a_near(&d, y) {
                    Some(x) if (x - expected_x).abs() <= tolerance => {}
                    Some(x) => bad.push(format!("{} {jc} {what}: starts at x {x:.2}, Word {expected_x:.2}", set.name)),
                    None => bad.push(format!("{} {jc} {what}: no あ on the baseline {y}", set.name)),
                }
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// A2: the gutter (`w:gutter`) does not move a vertical section's head from its left margin (it goes on the right), and shrinks the centre and the right aligned
/// box on that side; in an across section the head starts `left` + `gutter` (the control).
fn a2_gutter() {
    let m = Margins { gutter: 720, ..Margins::default() };
    let head = aligned("あいうABC", LEFT);
    let d = drawn(&pages_of(&make_general("a2-v", &aligned("漢字", LEFT), true, "ＭＳ 明朝", m, Some(&head), None)));
    near(start_of_a_near(&d, 46.56).expect("the head"), 72.02, 0.25, "a vertical section: the gutter does not move the head");
    let d = drawn(&pages_of(&make_general("a2-h", &aligned("漢字", LEFT), false, "ＭＳ 明朝", m, Some(&head), None)));
    near(start_of_a_near(&d, 46.56).expect("the head"), 72.02 + 36.0, 0.25, "an across section: the head starts at left + gutter");
}

/// A3: `- PAGE -` centred in the foot of a vertical section: across, in the middle of the foot's box (page 1: the hyphens at x 284.57 and 305.69, the number
/// at 295.13, baseline 802.92), the number upright (not turned); on page 2 the same places for the number 2.
fn a3_page_number() {
    let foot = r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t xml:space="preserve">- </w:t></w:r><w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple><w:r><w:t xml:space="preserve"> -</w:t></w:r></w:p>"#;
    let two_pages = format!(r#"{}<w:p><w:r><w:br w:type="page"/></w:r></w:p>{}"#, aligned("漢字", LEFT), aligned("漢字", LEFT));
    let pagination = pages_of(&make_general("a3", &two_pages, true, "ＭＳ 明朝", Margins::default(), None, Some(foot)));
    assert!(pagination.pages.len() >= 2, "two pages");
    let installed = typeface_installed("a3", "ＭＳ 明朝");
    for (index, number, hyphens_x, number_x) in [(0usize, "1", [284.57, 305.69], 295.13), (1, "2", [284.57, 305.69], 295.13)] {
        let mut items = Vec::new();
        for it in &pagination.pages[index].items {
            match it {
                Item::Text { x, y, text, offsets, .. } if (*y - 802.92).abs() < 1.5 => {
                    for (c, o) in text.chars().zip(offsets.iter()) {
                        items.push((x + o, c));
                    }
                }
                Item::Scaled { x, y, text, .. } if (*y - 802.92).abs() < 1.5 => items.push((*x, text.chars().next().unwrap_or(' '))),
                Item::Turned { y, text, .. } if (*y - 802.92).abs() < 1.5 && text.as_ref() == number => panic!("page {}: the number is turned", index + 1),
                _ => {}
            }
        }
        let find = |c: char| items.iter().filter(|i| i.1 == c).map(|i| i.0).collect::<Vec<_>>();
        let hyphens = find('-');
        assert_eq!(hyphens.len(), 2, "page {}: two hyphens in the foot: {items:?}", index + 1);
        // Where the line stands is read from the spaces and the number, which are drawn at the start of their cell of 5.28 pt in any face. The hyphens are checked only where ＭＳ 明朝
        // is installed: its hyphen fills the half-width cell, so its origin is the cell's; the stand-in of a machine without it (Hiragino Mincho) has a narrower hyphen, centred in the
        // cell, which moves its origin by half the difference (+0.88 pt on a Mac), a difference of the stand-in's glyph and not of the line's place.
        let spaces = find(' ');
        assert_eq!(spaces.len(), 2, "page {}: two spaces in the foot: {items:?}", index + 1);
        near(spaces[0], hyphens_x[0] + 5.28, 0.25, &format!("page {}: the space after the first hyphen", index + 1));
        near(spaces[1], hyphens_x[1] - 5.28, 0.25, &format!("page {}: the space before the second hyphen", index + 1));
        // In any face a hyphen's origin is inside its cell: [cell start - 0.25, cell start + cell/2 + 0.25] (a narrower glyph is centred in it).
        for (k, (got, cell)) in hyphens.iter().zip(hyphens_x).enumerate() {
            assert!(*got >= cell - 0.25 && *got <= cell + 5.28 / 2.0 + 0.25, "page {}: hyphen {} is at {got}, outside its cell [{}, {}]", index + 1, k + 1, cell - 0.25, cell + 5.28 / 2.0 + 0.25);
        }
        if installed {
            near(hyphens[0], hyphens_x[0], 0.25, &format!("page {}: the first hyphen", index + 1));
            near(hyphens[1], hyphens_x[1], 0.25, &format!("page {}: the second hyphen", index + 1));
        } else {
            eprintln!("page {}: ＭＳ 明朝 is not installed, so the hyphens' exact origins (Word's {} and {}) are not checked (their cells are)", index + 1, hyphens_x[0], hyphens_x[1]);
        }
        let n = find(number.chars().next().unwrap());
        assert_eq!(n.len(), 1, "page {}: the number {number}: {items:?}", index + 1);
        near(n[0], number_x, 0.25, &format!("page {}: the number", index + 1));
    }
}

/// A7: `w:docGrid` (lines, linesAndChars, snapToChars) does not move a head or foot of a vertical section: they stay at the places of A1's l72-r72 set (Word's
/// body moves, the head does not).
fn a7_doc_grid() {
    for (name, grid) in [("lines", r#" w:type="lines""#), ("lines-and-chars", r#" w:type="linesAndChars" w:charSpace="4096""#), ("snap-to-chars", r#" w:type="snapToChars""#)] {
        let head = aligned("あいうABC", LEFT);
        let m = Margins { grid, ..Margins::default() };
        let d = drawn(&pages_of(&make_general(&format!("a7-{name}"), &aligned("漢字", LEFT), true, "ＭＳ 明朝", m, Some(&head), Some(&head))));
        for (what, y) in [("head", 46.56f32), ("foot", 802.92)] {
            near(start_of_a_near(&d, y).unwrap_or_else(|| panic!("{name}: no {what} on the baseline {y}: {d:?}")), 72.02, 0.25, &format!("{name}: the {what}'s start"));
        }
    }
}

/// Whether `face` is installed on this machine (the layout draws the stand-in of a face that is not).
fn typeface_installed(name: &str, face: &str) -> bool {
    let mut s = crate::shell::scenario::Scenario::open(&make_general(name, &aligned("漢字", LEFT), true, face, Margins::default(), None, None));
    s.with(|e, _| e.resources.has_typeface(face))
}

/// A4: a paragraph of the head with `w:textDirection` (`tbRl` or `lrTb`) stays across, at the places it has without it (Word ignores it).
fn a4_paragraph_direction() {
    for direction in ["tbRl", "lrTb"] {
        let head = format!(r#"<w:p><w:pPr><w:jc w:val="center"/><w:textDirection w:val="{direction}"/></w:pPr><w:r><w:t>あいうABC</w:t></w:r></w:p>"#);
        let d = drawn(&pages_of(&make_general(&format!("a4-{direction}"), &aligned("漢字", LEFT), true, "ＭＳ 明朝", Margins::default(), Some(&head), None)));
        near(start_of_a_near(&d, 46.56).unwrap_or_else(|| panic!("{direction}: the head is not across: {d:?}")), 272.69, 0.25, &format!("{direction}: the centred head"));
    }
}

/// A head that is a one-cell table of `text` with the cell direction `direction`.
fn table_head(direction: &str) -> String {
    let dir = if direction.is_empty() { String::new() } else { format!(r#"<w:textDirection w:val="{direction}"/>"#) };
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="2000"/></w:trPr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/>{dir}</w:tcPr><w:p><w:r><w:t>あいうABC</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"#
    )
}

/// A5: the text of a cell in a head: `tbRl` lays all of it (the Japanese too) on its side to the right, `tbRlV` keeps the Japanese upright and lays the Latin
/// on its side, `btLr` lays all of it on its side to the left (anticlockwise).
fn a5_cell_directions() {
    for (direction, japanese_upright, japanese_turns, latin_turns) in [("tbRl", false, 1i8, 1i8), ("tbRlV", true, 0, 1), ("btLr", false, -1, -1)] {
        let d = pages_of(&make_general(&format!("a5-{direction}"), &aligned("漢字", LEFT), true, "ＭＳ 明朝", Margins::default(), Some(&table_head(direction)), None));
        let items = &d.pages[0].items;
        let turns_of = |s: &str| items.iter().find_map(|it| match it {
            Item::Turned { text, turns, .. } if text.as_ref() == s => Some(*turns),
            _ => None,
        });
        let upright_in_a_text = items.iter().any(|it| matches!(it, Item::Text { text, y, .. } if text.contains('あ') && *y < 200.0));
        if japanese_upright {
            assert!(upright_in_a_text && turns_of("あ").is_none(), "{direction}: the あ stands");
        } else {
            assert_eq!(turns_of("あ"), Some(japanese_turns), "{direction}: the あ lies on its side (turns)");
        }
        assert_eq!(turns_of("A"), Some(latin_turns), "{direction}: the A lies on its side (turns)");
    }
}

/// A6: a table (no cell direction) in the head puts the text a cell margin (108 twips = 5.4 pt) in from the left margin: x 77.42, baseline 46.56.
fn a6_table_in_head() {
    let d = drawn(&pages_of(&make_general("a6", &aligned("漢字", LEFT), true, "ＭＳ 明朝", Margins::default(), Some(&table_head("")), None)));
    near(start_of_a_near(&d, 46.56).expect("the head's あ on the baseline"), 77.42, 0.3, "the table's text starts a cell margin in");
}

/// A6b: without Word's default table style in styles.xml a table's cells have no margin (#227, Windows Word): the text starts at the left margin, x 72.00 to 72.02.
fn a6b_table_without_style() {
    let m = Margins { default_table_style: false, ..Margins::default() };
    let d = drawn(&pages_of(&make_general("a6b", &aligned("漢字", LEFT), true, "ＭＳ 明朝", m, Some(&table_head("")), None)));
    near(start_of_a_near(&d, 46.56).expect("the head's あ on the baseline"), 72.01, 0.3, "no default table style: the table's text starts at the left margin");
}

/// Word's advance (pt, 10.5 pt text) of each of the sixteen spaces, vertical and across alike, from the section 3 table of the Word result: columns Meiryo,
/// Yu Mincho, MS P Mincho.
const SPACES: [(char, [f32; 3]); 16] = [
    ('\u{20}', [3.564, 2.724, 3.192]),
    ('\u{a0}', [3.564, 2.724, 3.192]),
    ('\u{2000}', [5.256, 5.256, 5.244]),
    ('\u{2001}', [10.503, 10.503, 10.491]),
    ('\u{2002}', [6.528, 5.256, 5.280]),
    ('\u{2003}', [10.503, 10.503, 7.008]),
    ('\u{2004}', [3.480, 3.480, 2.328]),
    ('\u{2005}', [2.628, 2.628, 1.752]),
    ('\u{2006}', [1.752, 1.752, 1.164]),
    ('\u{2007}', [5.736, 5.256, 6.552]),
    ('\u{2008}', [3.180, 2.628, 3.276]),
    ('\u{2009}', [2.100, 2.100, 1.308]),
    ('\u{200a}', [0.864, 0.864, 0.648]),
    ('\u{202f}', [2.100, 2.100, 2.100]),
    ('\u{205f}', [2.328, 2.328, 2.328]),
    ('\u{3000}', [10.503, 10.503, 6.960]),
];

/// The advance of each space in `face`, vertical and across, against Word's `column` of `SPACES`: the distance between え and お with the space between them, less
/// the distance without it (a proportional face sets え narrower than 10.5 pt across, so 10.5 is not taken off).
fn space_advances(face: &str, column: usize, name: &str) {
    let gap = |vertical: bool, middle: &str| -> Option<f32> {
        let body = format!(r#"<w:p><w:r><w:t xml:space="preserve">あ</w:t></w:r><w:r><w:t xml:space="preserve">え{middle}お</w:t></w:r></w:p>"#);
        let d = drawn(&pages_of(&make_general(&format!("b-{name}"), &body, vertical, face, Margins::default(), None, None)));
        let (e, o) = (d.iter().find(|i| i.2 == "え")?, d.iter().find(|i| i.2 == "お")?);
        Some(if vertical { o.1 - e.1 } else { o.0 - e.0 })
    };
    let mut bad = Vec::new();
    for vertical in [false, true] {
        let Some(base) = gap(vertical, "") else { panic!("{face}: no え and お") };
        for (c, word) in SPACES {
            let Some(with) = gap(vertical, &c.to_string()) else { bad.push(format!("U+{:04X} lost", c as u32)); continue };
            let adv = with - base;
            let want = word[column];
            if (adv - want).abs() > 0.25 {
                bad.push(format!("U+{:04X} vertical={vertical}: {adv:.3} against Word {want}", c as u32));
            }
        }
    }
    assert!(bad.is_empty(), "{face}: {bad:#?}");
}

/// B1: Meiryo. Its own widths (`hmtx`) for most; U+2004 (4.19: not the face's 3.52) and U+2006 (2.63: not 1.76) are Word's own; U+202F and U+205F are fixed.
fn b1_meiryo() {
    space_advances("メイリオ", 0, "meiryo");
}

/// B2: Yu Mincho has no glyph for most spaces: Word sets them by em fractions (U+2004 1/3, U+2005 1/4, U+2006 1/6, U+2009 1/5, U+200A 1/12 em).
fn b2_yu_mincho() {
    space_advances("游明朝", 1, "yumincho");
}

/// B3: MS P Mincho (proportional): its own widths, the ideographic space 6.926 and U+2003 6.986 included (not the full 10.5 pt).
fn b3_ms_p_mincho() {
    space_advances("ＭＳ Ｐ明朝", 2, "msp");
}

/// B4: U+202F is 0.2 em (2.102 pt of 10.5 pt text) and U+205F 4/18 em (2.330 pt) in every face and both directions (not by the face's own width).
fn b4_fixed_spaces() {
    for face in ["メイリオ", "游明朝", "ＭＳ 明朝"] {
        for vertical in [false, true] {
            let gap = |middle: &str| {
                let body = format!(r#"<w:p><w:r><w:t xml:space="preserve">あえ{middle}お</w:t></w:r></w:p>"#);
                let d = drawn(&pages_of(&make_general("b4", &body, vertical, face, Margins::default(), None, None)));
                let (e, o) = (d.iter().find(|i| i.2 == "え").unwrap(), d.iter().find(|i| i.2 == "お").unwrap());
                if vertical { o.1 - e.1 } else { o.0 - e.0 }
            };
            let base = gap("");
            for (c, want) in [('\u{202f}', 2.102f32), ('\u{205f}', 2.330)] {
                near(gap(&c.to_string()) - base, want, 0.25, &format!("{face} U+{:04X} vertical={vertical}", c as u32));
            }
        }
    }
    let _ = texts;
}


// ---- the tests added with Windows' later measurements (tasks/doc/requests/2026-11-25-188-rest-result.md W-A1..W-A3, 2026-11-25-188-mechanism-result.md) ----

/// Per-document options of `make_pages`.
#[derive(Clone, Copy, Default)]
struct PageOpts {
    even_odd: bool,
    title_pg: bool,
    mirror: bool,
    gutter: u32,
    /// The right margin in twips (the left is 1440); 0 means 1440.
    right: u32,
}

/// A document of `pages` pages (a paragraph 漢字 each, a page break between) of a vertical `tbRl` section (ＭＳ 明朝 10.5 pt, A4, the left margin 72 pt) whose feet are
/// `feet` = (`w:type` of the reference: default, even or first, the XML of the paragraphs). The path of the file.
fn make_pages(name: &str, pages: usize, feet: &[(&str, &str)], o: PageOpts, vertical: bool) -> String {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let r = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let right = if o.right == 0 { 1440 } else { o.right };
    let body: String = (0..pages).map(|i| format!("{}{}", if i == 0 { "" } else { r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"# }, aligned("漢字", LEFT))).collect();
    let mut ct = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>"#);
    let mut rels = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{r}/settings" Target="settings.xml"/><Relationship Id="rId2" Type="{r}/styles" Target="styles.xml"/>"#);
    let mut refs = String::new();
    for (i, (kind, _)) in feet.iter().enumerate() {
        ct += &format!(r#"<Override PartName="/word/footer{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>"#);
        rels += &format!(r#"<Relationship Id="rIdF{i}" Type="{r}/footer" Target="footer{i}.xml"/>"#);
        refs += &format!(r#"<w:footerReference w:type="{kind}" r:id="rIdF{i}" xmlns:r="{r}"/>"#);
    }
    ct += "</Types>";
    rels += "</Relationships>";
    let root = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{r}/officeDocument" Target="word/document.xml"/></Relationships>"#);
    let settings = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{w}">{}{}<w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#,
        if o.mirror { "<w:mirrorMargins/>" } else { "" },
        if o.even_odd { "<w:evenAndOddHeaders/>" } else { "" }
    );
    let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{w}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝" w:cs="ＭＳ 明朝"/><w:sz w:val="21"/><w:szCs w:val="21"/><w:lang w:val="en-US" w:eastAsia="ja-JP" w:bidi="ar-SA"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#);
    let direction = if vertical { r#"<w:textDirection w:val="tbRl"/>"# } else { "" };
    let title = if o.title_pg { "<w:titlePg/>" } else { "" };
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}"><w:body>{body}<w:sectPr>{refs}<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="{right}" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="{}"/>{title}{direction}<w:docGrid w:linePitch="360"/></w:sectPr></w:body></w:document>"#,
        o.gutter
    );
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), root.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/settings.xml", EntryMeta::new(), settings.as_bytes()).unwrap();
    z.add("word/styles.xml", EntryMeta::new(), styles.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    for (i, (_, xml)) in feet.iter().enumerate() {
        let part = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:ftr xmlns:w="{w}">{xml}</w:ftr>"#);
        z.add(&format!("word/footer{i}.xml"), EntryMeta::new(), part.as_bytes()).unwrap();
    }
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("issue188-{name}.docx")), z.finish().unwrap()).unwrap();
    format!("target/scenario-input/issue188-{name}.docx")
}

/// The characters drawn on page `index` (0-based) on the foot's baseline 802.92: (x, character), in the order of the line.
fn foot_chars(p: &super::flow_layout::Pagination, index: usize) -> Vec<(f32, char)> {
    let mut v = Vec::new();
    for it in &p.pages[index].items {
        match it {
            Item::Text { x, y, text, offsets, .. } if (*y - 802.92).abs() < 1.5 => v.extend(text.chars().zip(offsets.iter()).map(|(c, o)| (x + o, c))),
            Item::Scaled { x, y, text, .. } if (*y - 802.92).abs() < 1.5 => v.push((*x, text.chars().next().unwrap_or(' '))),
            Item::Turned { y, text, .. } if (*y - 802.92).abs() < 1.5 => panic!("page {}: {text:?} in the foot is turned", index + 1),
            _ => {}
        }
    }
    v
}

/// A foot paragraph `- PAGE -` aligned `jc` with the indents `ind` (the attributes of `w:ind`).
fn page_foot(jc: &str, ind: &str) -> String {
    format!(r#"<w:p><w:pPr><w:ind {ind}/><w:jc w:val="{jc}"/></w:pPr><w:r><w:t xml:space="preserve">- </w:t></w:r><w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple><w:r><w:t xml:space="preserve"> -</w:t></w:r></w:p>"#)
}

/// A8 (W-A1): `- PAGE -` of a vertical section's foot: from one digit to two (page 10) the line is 5.28 pt wider, so a centred line starts 2.64 pt left, a right aligned one
/// 5.28 pt left (its end stays at 518.26) and a left aligned one where it was. Word: page 1 / page 10 digits: centre 295.13 / 292.49 and 297.77, left 82.58 / 82.58 and
/// 87.86, right 507.70 / 502.42 and 507.70 (the hyphens, where ＭＳ 明朝 is installed: centre 284.57 and 281.93 for the first, left 72.02, right 497.14 and 491.86).
fn a8_two_digit_page_numbers() {
    let installed = typeface_installed("a8", "ＭＳ 明朝");
    let mut bad = Vec::new();
    for (jc, one, ten, first_hyphen) in [
        (CENTER, vec![295.13f32], vec![292.49f32, 297.77], [284.57f32, 281.93]),
        (LEFT, vec![82.58], vec![82.58, 87.86], [72.02, 72.02]),
        (RIGHT, vec![507.70], vec![502.42, 507.70], [497.14, 491.86]),
    ] {
        let foot = page_foot(jc, "");
        let file = make_pages(&format!("a8-{jc}"), 12, &[("default", &foot)], PageOpts::default(), true);
        let p = pages_of(&file);
        if p.pages.len() < 10 {
            bad.push(format!("{jc}: only {} pages", p.pages.len()));
            continue;
        }
        for (page, digits, hyphen) in [(0usize, one, first_hyphen[0]), (9, ten, first_hyphen[1])] {
            let chars = foot_chars(&p, page);
            let got: Vec<f32> = chars.iter().filter(|c| c.1.is_ascii_digit()).map(|c| c.0).collect();
            if got.len() != digits.len() || got.iter().zip(&digits).any(|(g, w)| (g - w).abs() > 0.25) {
                bad.push(format!("{jc} page {}: digits at {got:?}, Word {digits:?}", page + 1));
            }
            if installed {
                match chars.iter().find(|c| c.1 == '-') {
                    Some(h) if (h.0 - hyphen).abs() <= 0.25 => {}
                    other => bad.push(format!("{jc} page {}: first hyphen {other:?}, Word {hyphen}", page + 1)),
                }
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// A9 (W-A1): the indents of the foot paragraph move the box the line is aligned in: right aligned with `w:right` 720 ends 36 pt left (the number at 471.70 on page 1),
/// left aligned with `w:left` 720 starts 36 pt right (number 118.58), centred with left 720 and right 360 is centred in the box [108.02, 505.32] (number 304.13).
fn a9_page_number_indents() {
    let mut bad = Vec::new();
    for (name, jc, ind, number_x) in [
        ("right", RIGHT, r#"w:right="720""#, 471.70f32),
        ("left", LEFT, r#"w:left="720""#, 118.58),
        ("center", CENTER, r#"w:left="720" w:right="360""#, 304.13),
    ] {
        let foot = page_foot(jc, ind);
        let p = pages_of(&make_pages(&format!("a9-{name}"), 2, &[("default", &foot)], PageOpts::default(), true));
        let n: Vec<f32> = foot_chars(&p, 0).iter().filter(|c| c.1 == '1').map(|c| c.0).collect();
        match n.as_slice() {
            [x] if (x - number_x).abs() <= 0.25 => {}
            _ => bad.push(format!("{name}: the number at {n:?}, Word {number_x}")),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// A10 (W-A3): `w:evenAndOddHeaders` and `w:titlePg` pick the foot of the page (first page: the first foot, with both settings the first wins over the even one; even
/// pages: the even foot; the others: the default) and every foot is at the same places: left aligned starts 72.02, centred in [72, 487.32] (the right margin 108).
/// The feet are the letters F, E and D (three each) so that the page's foot is told by its letters.
fn a10_even_odd_first() {
    let foot = |letter: &str, jc: &str| aligned(&letter.repeat(3), jc);
    let mut bad = Vec::new();
    for (name, o, expected) in [
        ("even-odd", PageOpts { even_odd: true, right: 2160, ..PageOpts::default() }, ['D', 'E', 'D', 'E']),
        ("title", PageOpts { title_pg: true, right: 2160, ..PageOpts::default() }, ['F', 'D', 'D', 'D']),
        ("both", PageOpts { even_odd: true, title_pg: true, right: 2160, ..PageOpts::default() }, ['F', 'E', 'D', 'E']),
    ] {
        for (jc, start) in [(LEFT, 72.02f32), (CENTER, 72.0 + (415.32 - 3.0 * 5.26) / 2.0)] {
            let feet = [("default", foot("D", jc)), ("even", foot("E", jc)), ("first", foot("F", jc))];
            let feet_ref: Vec<(&str, &str)> = feet.iter().map(|f| (f.0, f.1.as_str())).collect();
            let p = pages_of(&make_pages(&format!("a10-{name}-{jc}"), 4, &feet_ref, o, true));
            for (page, want) in expected.iter().enumerate() {
                let chars = foot_chars(&p, page);
                match chars.first() {
                    Some((x, c)) if c == want && (x - start).abs() <= 0.35 => {}
                    other => bad.push(format!("{name} {jc} page {}: first foot character {other:?}, wanted {want} at {start:.2}", page + 1)),
                }
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// A11 (W-A3): `w:mirrorMargins` in a vertical section (left 72, right 108, gutter 0 or 36): odd pages have the box [72, 595.32 - 108 - gutter], even pages swap the
/// margins: [108 + gutter, 595.32 - 72] (the gutter goes right on odd pages and left on even ones). Left aligned feet start at the box's left (Word: 72.02 / 108.02 and
/// 72.02 / 144.02), centred and right aligned ones are in the box (a derived value: the box's middle and its right edge less the line's width 15.78 pt of ABC).
/// In an across section the gutter goes to the inside (Word: gutter 36, both pages [108, 487.32]: 108.02 on both).
fn a11_mirror() {
    let mut bad = Vec::new();
    for gutter in [0u32, 720] {
        let g = gutter as f32 / 20.0;
        let (odd, even) = ((72.0f32, 595.32 - 108.0 - g), (108.0 + g, 595.32 - 72.0));
        for (jc, at) in [(LEFT, (odd.0 + 0.02, even.0 + 0.02)), (CENTER, ((odd.0 + odd.1 - 15.78) / 2.0, (even.0 + even.1 - 15.78) / 2.0)), (RIGHT, (odd.1 - 15.78, even.1 - 15.78))] {
            let foot = aligned("ABC", jc);
            let o = PageOpts { mirror: true, gutter, right: 2160, ..PageOpts::default() };
            let p = pages_of(&make_pages(&format!("a11-v-{gutter}-{jc}"), 4, &[("default", &foot)], o, true));
            for (page, want) in [(0usize, at.0), (1, at.1), (2, at.0), (3, at.1)] {
                match foot_chars(&p, page).first() {
                    Some((x, _)) if (x - want).abs() <= 0.35 => {}
                    other => bad.push(format!("vertical gutter {g} {jc} page {}: starts {other:?}, wanted {want:.2}", page + 1)),
                }
            }
        }
    }
    let foot = aligned("ABC", LEFT);
    let o = PageOpts { mirror: true, gutter: 720, right: 2160, ..PageOpts::default() };
    let p = pages_of(&make_pages("a11-h", 2, &[("default", &foot)], o, false));
    for page in 0..2 {
        match foot_chars(&p, page).first() {
            Some((x, _)) if (x - 108.02).abs() <= 0.35 => {}
            other => bad.push(format!("across gutter 36 page {}: starts {other:?}, wanted 108.02", page + 1)),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// A12 (W-A2): the cell margins of a table in a head move its text one for one: the left margin along the line (x), the top margin down the cell. Four cells of 75 pt
/// side by side (text あ い う え): with the margins left and right 108, 0 or 216 twips the first character of cell k is at 72.02 + 75 (k - 1) + the left margin
/// (5.4, 0 or 10.8) on the baseline 46.56 + the top margin (0, or 3.6 pt for the top margin 72 twips). Word: 77.42, 72.02, 82.82 (cell 1) and 152.42 (cell 2, 108 twips).
fn a12_cell_margins() {
    let mut bad = Vec::new();
    for (name, mar, left_pt, top_pt) in [
        ("108", r#"<w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/>"#, 5.4f32, 0.0f32),
        ("0", r#"<w:left w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/>"#, 0.0, 0.0),
        ("216-72", r#"<w:top w:w="72" w:type="dxa"/><w:left w:w="216" w:type="dxa"/><w:right w:w="216" w:type="dxa"/>"#, 10.8, 3.6),
    ] {
        let cell = |t: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#);
        let head = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="6000" w:type="dxa"/><w:tblCellMar>{mar}</w:tblCellMar></w:tblPr><w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/></w:tblGrid><w:tr><w:trPr><w:trHeight w:val="2000"/></w:trPr>{}{}{}{}</w:tr></w:tbl><w:p/>"#,
            cell("あ"), cell("い"), cell("う"), cell("え")
        );
        let d = drawn(&pages_of(&make_general(&format!("a12-{name}"), &aligned("漢字", LEFT), true, "ＭＳ 明朝", Margins::default(), Some(&head), None)));
        for (k, ch) in ["あ", "い", "う", "え"].iter().enumerate() {
            let want_x = 72.02 + 75.0 * k as f32 + left_pt;
            match d.iter().find(|i| i.2 == *ch && i.1 < 200.0) {
                Some(i) if (i.0 - want_x).abs() <= 0.3 && (i.1 - (46.56 + top_pt)).abs() <= 0.3 => {}
                other => bad.push(format!("margins {name} cell {}: {other:?}, wanted x {want_x:.2} baseline {:.2}", k + 1, 46.56 + top_pt)),
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// B5 (mechanism result, 2026-11-25-188-mechanism-result.md): Word sets U+2002 to the face's U+2013 (EN DASH) advance, U+2003 to its U+2014 (EM DASH) advance (1 em in
/// ＭＳ 明朝 and ＭＳ ゴシック), and U+2004, U+2005, U+2006 to a third, a quarter and a sixth of the U+2014 advance, in every face (36 pt; em values of Word's XPS:
/// Arial 0.5561 / 1.0003, Calibri 0.4974 / 0.9063, Tahoma 0.5454 / 0.9096, メイリオ 0.6208 / 1.0003, ＭＳ Ｐ明朝 0.5041 / 0.6674, ＭＳ 明朝 0.5001 / U+2003 1.0003 with
/// its dash 0.5). Tolerance 0.005 em (the XPS's 600 dpi grid is 0.001 em at 36 pt). A face not installed on this machine is skipped, and said so.
fn b5_dash_rule() {
    // (face, em of U+2002, em of U+2014 (the base of U+2003 to U+2006), em of U+2003 where it is not the dash's)
    let faces: [(&str, f32, f32, Option<f32>); 6] = [
        ("Arial", 0.5561, 1.0003, None),
        ("Calibri", 0.4974, 0.9063, None),
        ("Tahoma", 0.5454, 0.9096, None),
        ("メイリオ", 0.6208, 1.0003, None),
        ("ＭＳ Ｐ明朝", 0.5041, 0.6674, None),
        ("ＭＳ 明朝", 0.5001, 0.5, Some(1.0003)),
    ];
    let size = 36.0f32;
    let mut bad = Vec::new();
    for (face, en, em, em_space) in faces {
        if !typeface_installed("b5", face) {
            eprintln!("b5: {face} is not installed on this machine; skipped");
            continue;
        }
        let gap = |middle: &str| -> f32 {
            let body = format!(r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:rPr><w:sz w:val="72"/></w:rPr><w:t xml:space="preserve">え{middle}お</w:t></w:r></w:p>"#);
            let d = drawn(&pages_of(&make_general("b5", &body, false, face, Margins::default(), None, None)));
            let (e, o) = (d.iter().find(|i| i.2 == "え").unwrap(), d.iter().find(|i| i.2 == "お").unwrap());
            o.0 - e.0
        };
        let base = gap("");
        for (c, want_em) in [
            ('\u{2002}', en),
            ('\u{2003}', em_space.unwrap_or(em)),
            ('\u{2004}', em / 3.0),
            ('\u{2005}', em / 4.0),
            ('\u{2006}', em / 6.0),
        ] {
            let adv = (gap(&c.to_string()) - base) / size;
            if (adv - want_em).abs() > 0.005 {
                bad.push(format!("{face} U+{:04X}: {adv:.4} em, Word {want_em:.4} em", c as u32));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
