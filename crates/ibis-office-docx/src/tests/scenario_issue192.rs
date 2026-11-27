// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #192 (the vertical position of text `w:position`, the border `w:bdr`, the underline and the strike), the document (DOCX) side. The paragraphs are the issue's
//! (「あいう」+ 対象「かきく」+「たちつ」, MS Mincho 10.5 pt, A4 with 72 pt margins, compatibility mode 15, a marker of Arial 8 pt before each); the values are the issue's measurements
//! of Word's PDFs (tolerance 0.25 pt; Word's PDFs have 0.12 pt steps), as differences from the paragraph without the property where the issue gives them so.

use super::flow_layout::{Item, Pagination};
use super::issue191_tests::{make, near, pages_of};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue192_position_moves_the_text_by_half_the_value_in_points", position_moves_the_text_by_half_the_value_in_points),
        ("docx::issue192_position_grows_the_line_unless_it_is_exact", position_grows_the_line_unless_it_is_exact),
        ("docx::issue192_border_takes_room_and_its_box_is_the_lines_box_grown_by_the_border", border_takes_room_and_its_box_is_the_lines_box_grown_by_the_border),
        ("docx::issue192_underline_of_a_latin_typeface_follows_its_post_table", underline_of_a_latin_typeface_follows_its_post_table),
        ("docx::issue192_underline_of_an_east_asian_typeface_is_a_twentieth_of_the_size_thick", underline_of_an_east_asian_typeface_is_a_twentieth_of_the_size_thick),
        ("docx::issue192_underline_of_following_runs_is_one_line_at_the_width_weighted_mean", underline_of_following_runs_is_one_line_at_the_width_weighted_mean),
        ("docx::issue192_strike_follows_the_os2_table_run_by_run", strike_follows_the_os2_table_run_by_run),
        ("docx::issue192_strike_of_the_typefaces_office_ships_follows_their_os2_values", strike_of_the_typefaces_office_ships_follows_their_os2_values),
        ("docx::issue192_position_of_a_whole_line_takes_the_rooms_above_and_below_by_the_raise", position_of_a_whole_line_takes_the_rooms_above_and_below_by_the_raise),
        ("docx::issue192_position_of_a_whole_line_with_a_line_rule", position_of_a_whole_line_with_a_line_rule),
        ("docx::issue192_border_wraps_by_the_characters_that_fit_with_its_room_at_both_ends", border_wraps_by_the_characters_that_fit_with_its_room_at_both_ends),
        ("docx::issue192_border_box_of_a_raised_run_holds_the_line_baseline_band", border_box_of_a_raised_run_holds_the_line_baseline_band),
        ("docx::issue192_border_run_from_the_line_start_wraps_with_its_room_on_each_line", border_run_from_the_line_start_wraps_with_its_room_on_each_line),
        ("docx::issue192_border_run_moves_whole_to_the_next_line_when_no_character_fits", border_run_moves_whole_to_the_next_line_when_no_character_fits),
        ("docx::issue192_multiple_line_of_ms_mincho_keeps_the_baseline_and_adds_the_lines_below", multiple_line_of_ms_mincho_keeps_the_baseline_and_adds_the_lines_below),
        ("docx::issue192_border_box_of_a_script_run_is_the_full_size_box_on_the_line_baseline", border_box_of_a_script_run_is_the_full_size_box_on_the_line_baseline),
        ("docx::issue192_border_grows_the_line_above_and_below_by_its_room", border_grows_the_line_above_and_below_by_its_room),
        ("docx::issue192_underline_of_an_east_asian_typeface_is_seven_tenths_of_its_win_descent_and_half_a_point_down", underline_of_an_east_asian_typeface_is_seven_tenths_of_its_win_descent_and_half_a_point_down),
        ("docx::issue192_dump", dump),
    ]
}

const ARIAL: &str = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="Arial" w:cs="Arial"/><w:sz w:val="16"/><w:szCs w:val="16"/>"#;

/// The issue's scene: a marker paragraph `#<name>` and the paragraph of 前 + 対象 (with `rpr`) + 後, with `ppr` (XML) in its properties.
fn scene(name: &str, ppr: &str, rpr: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="240" w:after="120"/></w:pPr><w:r><w:rPr>{ARIAL}</w:rPr><w:t>#{name}</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="0" w:after="240"{ppr}/></w:pPr><w:r><w:t>あいう</w:t></w:r><w:r><w:rPr>{rpr}</w:rPr><w:t>かきく</w:t></w:r><w:r><w:t>たちつ</w:t></w:r></w:p>"#
    )
}

/// The baseline of the first Text item of page 0 whose text starts with `start`, and the x of its first character.
fn at(p: &Pagination, start: &str) -> (f32, f32) {
    for it in &p.pages[0].items {
        if let Item::Text { x, y, text, .. } = it && text.starts_with(start) {
            return (*y, *x);
        }
    }
    panic!("no text {start} on page 0");
}

/// Item 1: `w:position` is in half-points, up above the baseline (Word's PDF: `S5a` 24 gives -12.00, `S5b` -24 +12.00, `S5c` 6 -3.00, `S5d` -6 +3.00, `S5e` 1 -0.48, `S5f` -1 +0.48:
/// the baseline of 対象 less that of 前). The line's other text stays where it is.
fn position_moves_the_text_by_half_the_value_in_points() {
    let cases = [("S5a", 24, -12.0), ("S5b", -24, 12.0), ("S5c", 6, -3.0), ("S5d", -6, 3.0), ("S5e", 1, -0.5), ("S5f", -1, 0.5)];
    let mut body = String::new();
    for (name, value, _) in cases {
        body += &scene(name, "", &format!(r#"<w:position w:val="{value}"/>"#));
    }
    body += &scene("S5g", "", "");
    let pg = pages_of(&make("position", &body));
    for (i, (name, _, delta)) in cases.iter().enumerate() {
        // The three runs are drawn as items of one line; the target's is the second.
        let ys: Vec<f32> = pg.pages[0].items.iter().filter_map(|it| if let Item::Text { y, text, .. } = it && (text.starts_with("あいう") || text.starts_with("かきく") || text.starts_with("たちつ")) { Some(*y) } else { None }).collect();
        let (before, target, after) = (ys[i * 3], ys[i * 3 + 1], ys[i * 3 + 2]);
        near(target - before, *delta, 0.25, &format!("{name}: the target against the text before it"));
        near(after - before, 0.0, 0.01, &format!("{name}: the text after it stays"));
    }
}

/// Item 1, the line: with an automatic line the room above the line grows by the raise (S5a: 18.24 -> 30.24 from the marker), and the room below by the lowering (S5b: the next
/// marker at 22.56 -> 34.56); with a line of exactly 12 pt (`S5h`, `S5i`) the lines do not change and the text moves.
fn position_grows_the_line_unless_it_is_exact() {
    let mut body = String::new();
    body += &scene("S5a", "", r#"<w:position w:val="24"/>"#);
    body += &scene("S5b", "", r#"<w:position w:val="-24"/>"#);
    body += &scene("S5g", "", "");
    body += &scene("S5h", r#" w:line="240" w:lineRule="exact""#, r#"<w:position w:val="24"/>"#);
    body += &scene("S5h0", r#" w:line="240" w:lineRule="exact""#, "");
    body += &scene("S5end", "", "");
    let pg = pages_of(&make("position-line", &body));
    // The room above a line: the marker's baseline to the baseline of the text before. The room below: the text's baseline to the next marker's.
    let above = |marker: &str, first: bool| {
        let _ = first;
        let (m, _) = at(&pg, marker);
        let ys: Vec<f32> = pg.pages[0].items.iter().filter_map(|it| if let Item::Text { y, text, .. } = it && *y > m && text.starts_with("あいう") { Some(*y) } else { None }).collect();
        ys[0] - m
    };
    let below = |marker: &str, next: &str| at(&pg, next).0 - (at(&pg, marker).0 + above(marker, true));
    let control_above = above("#S5g", true);
    let control_below = below("#S5g", "#S5h");
    near(above("#S5a", true) - control_above, 12.0, 0.25, "S5a: the room above the line grows by 12 pt");
    near(below("#S5a", "#S5b") - control_below, 0.0, 0.25, "S5a: the room below does not");
    near(above("#S5b", true) - control_above, 0.0, 0.25, "S5b: the room above does not");
    near(below("#S5b", "#S5g") - control_below, 12.0, 0.25, "S5b: the room below grows by 12 pt");
    // Exact lines: the same as the exact line without the raise.
    near(above("#S5h", true), above("#S5h0", true), 0.25, "S5h: the exact line keeps its room above");
    near(below("#S5h", "#S5h0"), below("#S5h0", "#S5end"), 0.25, "S5h: and its room below");
}

/// Item 2: a run's border (`w:bdr`). Word's PDFs, ＭＳ 明朝 10.5 pt: a single line of 0.5 pt, 0 pt from the text (`S6a`): the box begins 0.48 pt left of the target's first character and the text
/// after it is 0.48 pt from its last; 4 pt from the text (`S6c`) 4.44 pt both; a 3 pt line (`S6d`) 3.00. The box is 11.16 pt above the baseline and 3.48 pt below (`S6a`), a double line
/// (`S6f`) 12.12 above and 4.44 below.
fn border_takes_room_and_its_box_is_the_lines_box_grown_by_the_border() {
    let bdr = |kind: &str, sz: u32, space: u32| format!(r#"<w:bdr w:val="{kind}" w:sz="{sz}" w:space="{space}" w:color="auto"/>"#);
    let cases = [("S6a", bdr("single", 4, 0), 0.48, 0.48), ("S6c", bdr("single", 4, 4), 4.44, 4.44), ("S6d", bdr("single", 24, 0), 3.0, 3.0)];
    let mut body = String::new();
    for (name, rpr, _, _) in &cases {
        body += &scene(name, "", rpr);
    }
    body += &scene("S6f", "", &bdr("double", 4, 0));
    body += &scene("S6h", "", "");
    let pg = pages_of(&make("border", &body));
    // The three runs' text items in order: 前, 対象, 後.
    let texts: Vec<(f32, f32, f32)> = pg.pages[0]
        .items
        .iter()
        .filter_map(|it| if let Item::Text { x, y, text, offsets, size, .. } = it && (text.starts_with("あいう") || text.starts_with("かきく") || text.starts_with("たちつ")) { Some((*x + offsets.first().copied().unwrap_or(0.0), *y, *x + offsets.last().copied().unwrap_or(0.0) + *size)) } else { None })
        .collect();
    for (i, (name, _, left, right)) in cases.iter().enumerate() {
        let (before, target, after) = (texts[i * 3], texts[i * 3 + 1], texts[i * 3 + 2]);
        // The text before ends where the box begins, the target begins `left` on, and the text after begins `right` after the target's last character.
        near(target.0 - before.2, *left, 0.25, &format!("{name}: room before the target"));
        near(after.0 - target.2, *right, 0.25, &format!("{name}: room after the target"));
    }
    // The boxes: the rectangles drawn (the outer edges). Top and bottom of the first (`S6a`) and of the double one (`S6f`), against the baseline of its text.
    let rects = |from_y: f32, to_y: f32| -> (f32, f32) {
        let ys: Vec<(f32, f32)> = pg.pages[0].items.iter().filter_map(|it| if let Item::Rect { y, h, .. } = it && *y > from_y && *y < to_y { Some((*y, *y + *h)) } else { None }).collect();
        (ys.iter().map(|v| v.0).fold(f32::MAX, f32::min), ys.iter().map(|v| v.1).fold(f32::MIN, f32::max))
    };
    let base_a = texts[1].1;
    let (top, bottom) = rects(base_a - 20.0, base_a + 10.0);
    near(base_a - top, 11.16, 0.25, "S6a: the box's top");
    near(bottom - base_a, 3.48, 0.25, "S6a: the box's bottom");
    let base_f = texts[3 * 3 + 1].1;
    let (top, bottom) = rects(base_f - 20.0, base_f + 10.0);
    near(base_f - top, 12.12, 0.25, "S6f: the double box's top");
    near(bottom - base_f, 4.44, 0.25, "S6f: the double box's bottom");
}

/// Item 2, the line: a line with a bordered run has the border's room `W` more above and below (Word's PDFs, ＭＳ 明朝 10.5 pt; the room above the text, from the marker's baseline, and
/// the room below, to the next marker's, against the paragraph without the border: S6a 0.62 and 0.36, S6c 4.44 and 4.56, S6d 2.88 and 3.15, S6f 1.44 and 1.56; 游ゴシック S6g 0.45
/// and 0.50).
fn border_grows_the_line_above_and_below_by_its_room() {
    let bdr = |kind: &str, sz: u32, space: u32| format!(r#"<w:bdr w:val="{kind}" w:sz="{sz}" w:space="{space}" w:color="auto"/>"#);
    let yu = r#"<w:rFonts w:ascii="游ゴシック" w:hAnsi="游ゴシック" w:eastAsia="游ゴシック"/>"#;
    let mut body = String::new();
    body += &scene("S6h", "", "");
    body += &scene("S6a", "", &bdr("single", 4, 0));
    body += &scene("S6c", "", &bdr("single", 4, 4));
    body += &scene("S6d", "", &bdr("single", 24, 0));
    body += &scene("S6f", "", &bdr("double", 4, 0));
    body += &scene("S6g", "", &format!("{yu}{}", bdr("single", 4, 0)));
    body += &scene("S6i", "", yu);
    body += &scene("S6end", "", "");
    let pg = pages_of(&make("border-line", &body));
    let above = |marker: &str| {
        let (m, _) = at(&pg, marker);
        pg.pages[0].items.iter().find_map(|it| if let Item::Text { y, text, .. } = it && *y > m && text.starts_with("あいう") { Some(*y - m) } else { None }).unwrap()
    };
    let below = |marker: &str, next: &str| at(&pg, next).0 - at(&pg, marker).0 - above(marker);
    let (c_above, c_below) = (above("#S6h"), below("#S6h", "#S6a"));
    for (m, next, a, b) in [("#S6a", "#S6c", 0.62, 0.36), ("#S6c", "#S6d", 4.44, 4.56), ("#S6d", "#S6f", 2.88, 3.15), ("#S6f", "#S6g", 1.44, 1.56)] {
        near(above(m) - c_above, a, 0.25, &format!("{m}: the room above"));
        near(below(m, next) - c_below, b, 0.25, &format!("{m}: the room below"));
    }
    near(above("#S6g") - above("#S6i"), 0.45, 0.25, "S6g: the room above");
    near(below("#S6g", "#S6i") - below("#S6i", "#S6end"), 0.50, 0.25, "S6g: the room below");
}

/// One paragraph of runs (`rpr`, text), each with its properties, alone on the page; the rectangles drawn (y, h, x, w) and the first baseline.
fn runs_page(name: &str, runs: &[(&str, &str)]) -> (f32, Vec<(f32, f32, f32, f32)>) {
    let mut body = String::from("<w:p>");
    for (rpr, text) in runs {
        body += &format!("<w:r><w:rPr>{rpr}</w:rPr><w:t>{text}</w:t></w:r>");
    }
    body += "</w:p>";
    let pg = pages_of(&make(name, &body));
    let rects = pg.pages[0].items.iter().filter_map(|it| if let Item::Rect { x, y, w, h, .. } = it { Some((*y, *h, *x, *w)) } else { None }).collect();
    let base = pg.pages[0].items.iter().find_map(|it| match it { Item::Text { y, .. } | Item::Scaled { y, .. } => Some(*y), _ => None }).unwrap_or_else(|| panic!("a text item: {:?}", pg.pages[0].items.iter().map(|i| std::mem::discriminant(i)).count()));
    (base, rects)
}

const ARIAL_72: &str = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="Arial" w:cs="Arial"/><w:sz w:val="144"/>"#;
const MINCHO_24: &str = r#"<w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝"/><w:sz w:val="48"/>"#;

/// A typeface that is not East Asian: the line is `post`'s underlineThickness thick at its underlinePosition below the baseline (Word's XPS, Arial 72 pt: a double line
/// 3.84 to 6.48 and 10.32 to 12.96, so a single line 7.68 down, 5.28 thick: Arial's 217 and 150 units of 2048).
fn underline_of_a_latin_typeface_follows_its_post_table() {
    let (base, rects) = runs_page("underline-arial", &[(&format!(r#"{ARIAL_72}<w:u w:val="single"/>"#), "H")]);
    assert_eq!(rects.len(), 1, "{rects:?}");
    near(rects[0].0 - base, 7.63, 0.25, "Arial 72 pt: the line's top");
    near(rects[0].1, 5.27, 0.25, "Arial 72 pt: its thickness");
    let (base, rects) = runs_page("underline-arial-double", &[(&format!(r#"{ARIAL_72}<w:u w:val="double"/>"#), "H")]);
    assert_eq!(rects.len(), 2, "{rects:?}");
    near(rects[0].0 - base, 3.84, 0.25, "double: the upper line's top");
    near(rects[0].1, 2.64, 0.25, "double: each line half the thickness");
    near(rects[1].0 - base, 10.32, 0.25, "double: the lower line's top");
}

/// An East Asian typeface (ＭＳ 明朝 here, told by its name where the PC lacks it): `size / 20` thick, the top where it was. Word's XPS: 24 pt 0.96 to 1.2.
fn underline_of_an_east_asian_typeface_is_a_twentieth_of_the_size_thick() {
    let (_, rects) = runs_page("underline-mincho", &[(&format!(r#"{MINCHO_24}<w:u w:val="single"/>"#), "あい")]);
    assert_eq!(rects.len(), 1, "{rects:?}");
    near(rects[0].1, 1.2, 0.25, "ＭＳ 明朝 24 pt: its thickness");
}

/// An East Asian typeface's underline: the top is `0.7 usWinDescent` and half a point below the baseline (a measured value the user allowed, implimentationRule.md; Word's PDFs of
/// 「あいう」 underlined, the top below the baseline, 8 typefaces at 10.5, 14, 24 and 48 pt, 32 points). The PDF's steps are 0.12 pt.
fn underline_of_an_east_asian_typeface_is_seven_tenths_of_its_win_descent_and_half_a_point_down() {
    let cases = [
        ("ＭＳ 明朝", [1.44, 1.80, 2.88, 5.16]), ("ＭＳ ゴシック", [1.44, 1.80, 2.88, 5.16]), ("ＭＳ Ｐ明朝", [1.44, 1.80, 2.88, 5.16]),
        ("游ゴシック", [2.76, 3.48, 5.52, 10.68]), ("游明朝", [2.64, 3.36, 5.40, 10.32]), ("メイリオ", [3.72, 4.80, 7.92, 15.24]),
        ("BIZ UDゴシック", [1.44, 1.68, 2.52, 4.56]), ("BIZ UD明朝 Medium", [1.44, 1.68, 2.52, 4.56]),
    ];
    for (i, (face, tops)) in cases.into_iter().enumerate() {
        for (j, (half_points, top)) in [21, 28, 48, 96].into_iter().zip(tops).enumerate() {
            let rpr = format!(r#"<w:rFonts w:ascii="{face}" w:hAnsi="{face}" w:eastAsia="{face}" w:cs="{face}"/><w:sz w:val="{half_points}"/><w:u w:val="single"/>"#);
            let (base, rects) = runs_page(&format!("underline-ea-{i}-{j}"), &[(&rpr, "あいう")]);
            assert_eq!(rects.len(), 1, "{face}: {rects:?}");
            near(rects[0].0 - base, top, 0.25, &format!("{face} {half_points}: the line's top below the baseline"));
        }
    }
}

/// Runs that follow one another with the same underline are one line, its thickness the mean of theirs weighted by the advances (Word's XPS: ＭＳ 明朝 10.5 pt 「あいう」 and 24 pt
/// 「い」: 0.96 where the mean over characters is lower).
fn underline_of_following_runs_is_one_line_at_the_width_weighted_mean() {
    let small = r#"<w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝"/><w:sz w:val="21"/><w:u w:val="single"/>"#;
    let (_, rects) = runs_page("underline-mixed", &[(small, "あいう"), (&format!(r#"{MINCHO_24}<w:u w:val="single"/>"#), "い")]);
    assert_eq!(rects.len(), 1, "one line: {rects:?}");
    // (0.525 x 31.5 + 1.2 x 24) / 55.5
    near(rects[0].1, 0.817, 0.1, "the thickness at the weighted mean");
    near(rects[0].3, 55.5, 0.5, "all the width");
}

/// A strike is `OS/2`'s yStrikeoutPosition and yStrikeoutSize as they are (Arial's file: 530 and 102 of 2048), a double one two lines a thickness above and below, a line to each run
/// (Word's XPS: a double one a thickness above and below the single one).
fn strike_follows_the_os2_table_run_by_run() {
    let arial = |half_points: u32| format!(r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="Arial" w:cs="Arial"/><w:sz w:val="{half_points}"/>"#);
    let (base, rects) = runs_page("strike-arial", &[(&format!("{}<w:strike/>", arial(144)), "H")]);
    assert_eq!(rects.len(), 1, "{rects:?}");
    near(rects[0].0 - base, -18.63, 0.25, "the line's top above the baseline");
    near(rects[0].1, 3.59, 0.25, "its thickness");
    let (base, rects) = runs_page("dstrike-arial", &[(&format!("{}<w:dstrike/>", arial(144)), "H")]);
    assert_eq!(rects.len(), 2, "{rects:?}");
    near(rects[0].0 - base, -18.63 - 3.59, 0.25, "the upper line");
    near(rects[1].0 - base, -18.63 + 3.59, 0.25, "the lower line");
    // Two sizes, two lines, each of its own size.
    let (_, rects) = runs_page("strike-two-sizes", &[(&format!("{}<w:strike/>", arial(48)), "H"), (&format!("{}<w:strike/>", arial(144)), "H")]);
    assert_eq!(rects.len(), 2, "{rects:?}");
    assert!(rects[1].1 > rects[0].1 * 2.0);
}

/// The typefaces Office ships (a PC may lack them: the table of the values read from their files in Windows): the strike's top above the baseline is yStrikeoutPosition / unitsPerEm
/// x size, its thickness yStrikeoutSize / unitsPerEm x size (ＭＳ 明朝 66 and 13 of 256, メイリオ 760 and 108 of 2048, bold 722 and 184, 游ゴシック 858 and 94).
fn strike_of_the_typefaces_office_ships_follows_their_os2_values() {
    let cases: [(&str, bool, f32, f32); 4] = [
        ("ＭＳ 明朝", false, 66.0 / 256.0, 13.0 / 256.0),
        ("ＭＳ ゴシック", true, 66.0 / 256.0, 13.0 / 256.0),
        ("メイリオ", false, 760.0 / 2048.0, 108.0 / 2048.0),
        ("メイリオ", true, 722.0 / 2048.0, 184.0 / 2048.0),
    ];
    for (i, (face, bold, pos, size)) in cases.into_iter().enumerate() {
        let b = if bold { "<w:b/>" } else { "" };
        let rpr = format!(r#"<w:rFonts w:ascii="{face}" w:hAnsi="{face}" w:eastAsia="{face}"/><w:sz w:val="48"/>{b}<w:strike/>"#);
        let (base, rects) = runs_page(&format!("strike-table-{i}"), &[(&rpr, "あい")]);
        assert_eq!(rects.len(), 1, "{face}: {rects:?}");
        near(base - rects[0].0, pos * 24.0, 0.02, &format!("{face}: the strike's top above the baseline"));
        near(rects[0].1, size * 24.0, 0.02, &format!("{face}: its thickness"));
    }
    // 游ゴシック and 游明朝 may be installed (a Mac has its own files for them, which win over the table), so their table is read directly.
    use crate::render::font_metrics::office_decoration_metrics as table;
    for (face, bold) in [("游ゴシック", false), ("游ゴシック", true), ("游明朝", false), ("游明朝", true)] {
        let m = table(face, bold).expect("in the table");
        near(m.strike_position, 858.0 / 2048.0, 1e-6, face);
        near(m.strike_size, 94.0 / 2048.0, 1e-6, face);
        near(m.underline_thickness, 104.0 / 2048.0, 1e-6, face);
    }
    near(table("游ゴシック", true).unwrap().win_descent, 612.0 / 2048.0, 1e-6, "Yu Gothic Bold winDescent");
}

/// A whole line raised or lowered (`w:position` on its only run; Word's XPS, ＭＳ 明朝 10.5 pt, the line's room above `A' = max(0, A + r)` and below `D' = max(0, D - r)`): the
/// text's baseline from the top of the page and the next line's from it. Plain: 10.56 and 13.58; raised 12 pt: 10.56 and 22.58; lowered 12 pt: 12.02 and 13.56; by 3 pt as plain.
fn position_of_a_whole_line_takes_the_rooms_above_and_below_by_the_raise() {
    let cases = [(0, 10.56, 13.58), (24, 10.56, 22.58), (-24, 12.02, 13.56), (6, 10.56, 13.58), (-6, 10.56, 13.58)];
    for (value, base, pitch) in cases {
        let rpr = if value == 0 { String::new() } else { format!(r#"<w:position w:val="{value}"/>"#) };
        let body = format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t>かきく</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:t>ｚ</w:t></w:r></w:p>"#);
        let pg = pages_of(&make("position-whole", &body));
        let ys: Vec<f32> = pg.pages[0].items.iter().filter_map(|it| if let Item::Text { y, .. } = it { Some(*y) } else { None }).collect();
        near(ys[0] - 72.0, base, 0.25, &format!("position {value}: the text's baseline"));
        near(ys[1] - ys[0], pitch, 0.25, &format!("position {value}: the next line"));
    }
}

/// The same with a line rule (Word's XPS, the measured text's baseline from the page's top and the next line's from it): at least 30 pt
/// (the shortfall above), exactly 20 pt (the line stays, the text moves). (A multiple of 1.5 without a grid is in `multiple_line_of_ms_mincho_keeps_the_baseline_and_adds_the_lines_below`; on a 14.5 pt grid it is the grid's 21.75 pt, Word's `gridmult` PDFs.)
fn position_of_a_whole_line_with_a_line_rule() {
    // (line attributes, position, baseline, pitch)
    let cases = [
        (r#"w:line="600" w:lineRule="atLeast""#, 0, 27.02, 13.56),
        (r#"w:line="600" w:lineRule="atLeast""#, 24, 18.02, 22.56),
        (r#"w:line="600" w:lineRule="atLeast""#, -24, 27.02, 13.56),
        (r#"w:line="400" w:lineRule="exact""#, 0, 16.10, 14.52),
        (r#"w:line="400" w:lineRule="exact""#, 24, 4.08, 26.54),
        (r#"w:line="400" w:lineRule="exact""#, -24, 28.10, 2.52),
    ];
    let mut wrong = Vec::new();
    for (line, value, base, pitch) in cases {
        let rpr = if value == 0 { String::new() } else { format!(r#"<w:position w:val="{value}"/>"#) };
        let body = format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" {line}/></w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t>かきく</w:t></w:r></w:p><w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:t>ｚ</w:t></w:r></w:p>"#);
        let pg = pages_of(&make("position-rule", &body));
        let ys: Vec<f32> = pg.pages[0].items.iter().filter_map(|it| if let Item::Text { y, .. } = it { Some(*y) } else { None }).collect();
        if (ys[0] - 72.0 - base).abs() > 0.25 || (ys[1] - ys[0] - pitch).abs() > 0.25 {
            wrong.push(format!("{line} position {value}: baseline {} against {base}, pitch {} against {pitch}", ys[0] - 72.0, ys[1] - ys[0]));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// A run's border wraps by the characters that fit (Word's XPS: 「あ」 x 40 and a bordered 「か」 x 2, then 「た」, the text 451.3 pt wide): each piece of the run takes `W` at both ends, so with
/// a 6 pt room (`sz` 48) the second 「か」 does not fit (33.0 pt against 31.3 left) and goes to the next line, where it begins `W` in (78.02); with 0.48 pt both stay.
fn border_wraps_by_the_characters_that_fit_with_its_room_at_both_ends() {
    let mut wrong = Vec::new();
    // (sz 96, a 12 pt room: Word puts both on the next line, 1 + 2W = 34.5 pt against the 31.3 left.)
    for (sz, second_on_next) in [(4, false), (12, false), (48, true), (96, true)] {
        let body = format!(r#"<w:p><w:r><w:t>{}</w:t></w:r><w:r><w:rPr><w:bdr w:val="single" w:sz="{sz}" w:space="0" w:color="auto"/></w:rPr><w:t>かか</w:t></w:r><w:r><w:t>た</w:t></w:r></w:p>"#, "あ".repeat(40));
        let pg = pages_of(&make("border-wrap", &body));
        let texts: Vec<(f32, f32, String)> = pg.pages[0].items.iter().filter_map(|it| if let Item::Text { x, y, text, .. } = it { Some((*x, *y, text.to_string())) } else { None }).collect();
        let ka_lines: std::collections::BTreeSet<i32> = texts.iter().filter(|t| t.2.contains('か')).map(|t| (t.1 * 10.0) as i32).collect();
        // The number of characters on the second line of the run's: 0 or 1 (sz 4, 12: none; sz 48: one; sz 96: two).
        let first_line_ka: usize = texts.iter().filter(|t| t.1 < texts[0].1 + 5.0).map(|t| t.2.chars().filter(|c| *c == 'か').count()).sum();
        let want_first = match sz { 4 | 12 => 2, 48 => 1, _ => 0 };
        // sz 96 puts both on the second line: the lines of the か are the one.
        let moved = if sz == 96 { ka_lines.len() == 1 && first_line_ka == 0 } else { (ka_lines.len() > 1) == second_on_next };
        if first_line_ka != want_first || !moved {
            wrong.push(format!("sz {sz}: か on the first line {first_line_ka} against {want_first}, lines {ka_lines:?}; {texts:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// A run of 60 「か」 from the start of a line (Word's PDFs, the line 451.3 pt wide): each line's fragment has `W` at both ends, so a line holds fewer characters than without the border:
/// sz 4 (W 0.48) 42, sz 12 space 2 (W 3.36) 42, sz 48 (W 6) 41, sz 96 (W 12) 40 on the first line, the left line of the box at the line's start and the first character `W` on.
fn border_run_from_the_line_start_wraps_with_its_room_on_each_line() {
    let mut wrong = Vec::new();
    for (sz, space, count, w) in [(4, 0, 42, 0.48), (12, 2, 42, 3.36), (48, 0, 41, 6.0), (96, 0, 40, 12.0)] {
        let body = format!(r#"<w:p><w:r><w:rPr><w:bdr w:val="single" w:sz="{sz}" w:space="{space}" w:color="auto"/></w:rPr><w:t>{}</w:t></w:r><w:r><w:t>た</w:t></w:r></w:p>"#, "か".repeat(60));
        let pg = pages_of(&make("border-run-wrap", &body));
        let lines: Vec<(f32, f32, usize)> = pg.pages[0].items.iter().filter_map(|it| if let Item::Text { x, y, text, offsets, .. } = it && text.contains('か') { Some((*y, *x + offsets.first().copied().unwrap_or(0.0), text.chars().filter(|c| *c == 'か').count())) } else { None }).collect();
        let mut ys: Vec<i32> = lines.iter().map(|l| (l.0 * 10.0) as i32).collect();
        ys.dedup();
        let first: usize = lines.iter().filter(|l| (l.0 * 10.0) as i32 == ys[0]).map(|l| l.2).sum();
        let second = lines.iter().find(|l| (l.0 * 10.0) as i32 == *ys.get(1).unwrap_or(&0)).map(|l| l.1);
        let ok_first = first == count;
        let ok_x = second.is_some_and(|x| (x - (72.0 + w)).abs() < 0.3) && (lines[0].1 - (72.0 + w)).abs() < 0.3;
        if !ok_first || !ok_x {
            wrong.push(format!("sz {sz}: {first} characters on the first line against {count}; starts {:?} / {second:?} against {}", lines.first().map(|l| l.1), 72.0 + w));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// 「あ」 x 41 + a border run of one 「か」 (Word's PDFs): sz 4 and sz 12 space 2 keep it on the first line, sz 48 (1 + 2W = 22.5 pt against the 20.8 left) and sz 96 move it to the second.
fn border_run_moves_whole_to_the_next_line_when_no_character_fits() {
    let mut wrong = Vec::new();
    for (sz, space, on_first) in [(4, 0, true), (12, 2, true), (48, 0, false), (96, 0, false)] {
        let body = format!(r#"<w:p><w:r><w:t>{}</w:t></w:r><w:r><w:rPr><w:bdr w:val="single" w:sz="{sz}" w:space="{space}" w:color="auto"/></w:rPr><w:t>か</w:t></w:r></w:p>"#, "あ".repeat(41));
        let pg = pages_of(&make("border-move", &body));
        let ys: Vec<(f32, bool)> = pg.pages[0].items.iter().filter_map(|it| if let Item::Text { y, text, .. } = it { Some((*y, text.contains('か'))) } else { None }).collect();
        let first_y = ys[0].0;
        let ka_first = ys.iter().any(|t| t.1 && (t.0 - first_y).abs() < 1.0);
        if ka_first != on_first {
            wrong.push(format!("sz {sz}: か on the first line {ka_first} against {on_first}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// The box of a run raised (`w:position`) holds the band of the line's baseline plus and minus `W` (Word's XPS, ＭＳ 明朝 10.5 pt, a 0.5 pt line, `W` 0.48): raised 12 pt the box is
/// from the raised text's own top (12 + 11.16 above the line's baseline) to 0.48 below the line's baseline; lowered 12 pt from 0.48 above it to 12 + 3.50 below.
fn border_box_of_a_raised_run_holds_the_line_baseline_band() {
    for (value, top, bottom) in [(24, -23.16, 0.48), (-24, -0.48, 15.50)] {
        let body = format!(r#"<w:p><w:r><w:t>あいう</w:t></w:r><w:r><w:rPr><w:bdr w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:position w:val="{value}"/></w:rPr><w:t>かきく</w:t></w:r></w:p>"#);
        let pg = pages_of(&make("border-raised", &body));
        let base = pg.pages[0].items.iter().find_map(|it| if let Item::Text { y, .. } = it { Some(*y) } else { None }).unwrap();
        let rects: Vec<(f32, f32)> = pg.pages[0].items.iter().filter_map(|it| if let Item::Rect { y, h, .. } = it { Some((*y, *y + *h)) } else { None }).collect();
        let (t, b) = (rects.iter().map(|r| r.0).fold(f32::MAX, f32::min), rects.iter().map(|r| r.1).fold(f32::MIN, f32::max));
        near(t - base, top, 0.3, &format!("position {value}: the box's top"));
        near(b - base, bottom, 0.3, &format!("position {value}: the box's bottom"));
    }
}

/// A superscript or subscript (`w:vertAlign`) with a border: the text shrinks but the box does not, and it is not lifted: the box of the run's full size on the line's baseline, the
/// same for both (Word's XPS of GitHub issue #192, 0.5 pt line: ＭＳ 明朝 10.5 pt -11.16 / +3.50 above and below the baseline, 24 pt -24.74 / +7.44; sz 12 space 2 the same
/// as sz 4 space 0 grown by its W: 3.36 instead of 0.48 each way).
fn border_box_of_a_script_run_is_the_full_size_box_on_the_line_baseline() {
    for (align, half, border, top, bottom) in [
        ("superscript", 21, (4, 0), -11.16, 3.50),
        ("subscript", 21, (4, 0), -11.16, 3.50),
        ("superscript", 48, (4, 0), -24.74, 7.44),
        ("subscript", 48, (4, 0), -24.74, 7.44),
        ("superscript", 21, (12, 2), -11.16 - 2.88, 3.50 + 2.88),
    ] {
        let body = format!(
            r#"<w:p><w:r><w:rPr><w:sz w:val="{half}"/></w:rPr><w:t>あいう</w:t></w:r><w:r><w:rPr><w:sz w:val="{half}"/><w:bdr w:val="single" w:sz="{}" w:space="{}" w:color="auto"/><w:vertAlign w:val="{align}"/></w:rPr><w:t>かきく</w:t></w:r></w:p>"#,
            border.0, border.1
        );
        let pg = pages_of(&make("border-script", &body));
        let base = pg.pages[0].items.iter().find_map(|it| if let Item::Text { y, .. } = it { Some(*y) } else { None }).unwrap();
        let rects: Vec<(f32, f32)> = pg.pages[0].items.iter().filter_map(|it| if let Item::Rect { y, h, .. } = it { Some((*y, *y + *h)) } else { None }).collect();
        let (t, b) = (rects.iter().map(|r| r.0).fold(f32::MAX, f32::min), rects.iter().map(|r| r.1).fold(f32::MIN, f32::max));
        near(t - base, top, 0.3, &format!("{align} {half}: the box's top"));
        near(b - base, bottom, 0.3, &format!("{align} {half}: the box's bottom"));
    }
}

/// A line spacing of several lines (`w:lineRule="auto"`) of ＭＳ 明朝 10.5 pt, no grid (Word's PDFs, the baselines of three lines from the paragraph's top): 1.5 lines 10.56 / 30.98 / 51.38
/// (feeds 20.42 / 20.40), 2 lines 10.68 / 37.94 / 65.18 (27.26 / 27.24), 3 lines 10.56 / 51.38 / 92.30 (40.82 / 40.92): the baseline is the single line's, the extra is below.
/// (This layout's single line is 13.5 pt where Word's is 13.6: `ms_room`, kept; tolerance 0.25 pt.)
fn multiple_line_of_ms_mincho_keeps_the_baseline_and_adds_the_lines_below() {
    let mut wrong = Vec::new();
    for (line, want) in [(360, [10.56, 30.98, 51.38]), (480, [10.68, 37.94, 65.18]), (720, [10.56, 51.38, 92.30])] {
        let body = format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="{line}" w:lineRule="auto"/></w:pPr><w:r><w:t>あ</w:t><w:br/><w:t>あ</w:t><w:br/><w:t>あ</w:t></w:r></w:p>"#);
        let pg = pages_of(&make("multiple-ms", &body));
        let ys: Vec<f32> = pg.pages[0].items.iter().filter_map(|it| if let Item::Text { y, .. } = it { Some(*y - 72.0) } else { None }).collect();
        if ys.len() != 3 || ys.iter().zip(want).any(|(a, b)| (a - b).abs() > 0.25) {
            wrong.push(format!("line {line}: baselines {ys:?} against {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// Not a test: prints what a file's pages hold (I192_FILE).
fn dump() {
    let Ok(file) = std::env::var("I192_FILE") else { return };
    let p = pages_of(&file);
    for (n, page) in p.pages.iter().enumerate() {
        println!("--- page {n}");
        for it in &page.items {
            match it {
                Item::Text { x, y, text, size, .. } => println!("T y={y:.2} x={x:.2} sz={size:.2} {text}"),
                Item::Rect { x, y, w, h, .. } => println!("RECT x={x:.2} y={y:.2} w={w:.2} h={h:.2}"),
                Item::Line { .. } => println!("LINE {it:?}"),
                _ => println!("OTHER {:?}", std::mem::discriminant(it)),
            }
        }
    }
}
