// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #54, how a justified line of mode 15 shares out the squeeze that takes in one more character (Windows Word, `tasks/doc/requests/2026-11-18-54-squeeze-share-result.md`): with
//! no hanging (`overflowPunct` off) the stop the line ends with takes `min(need, pitch / 2)` first, and the rest is shared out by the stops of the line, a unit each; the characters that
//! are not stops are not squeezed. ＭＳ 明朝 11 pt on a grid of lines and characters (charSpace 4290: a pitch of 12.047 pt), a column of 451.3 pt.

use super::flow_layout::{Item, Pagination};
use super::issue191_tests::{make_control, near, pages_of};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::issue54_squeeze_share_the_end_stop_takes_half_the_pitch_first_and_the_stops_share_the_rest", the_end_stop_takes_half_the_pitch_first)]
}

/// The x of each character of the first line of page 1.
fn first_line(pg: &Pagination) -> Vec<(char, f32)> {
    let mut out: Vec<(char, f32, f32)> = Vec::new();
    for item in &pg.pages[0].items {
        if let Item::Text { x, y, text, offsets, .. } = item {
            out.extend(text.chars().zip(offsets.iter()).map(|(c, dx)| (c, x + dx, *y)));
        }
    }
    let y0 = out.first().map_or(0.0, |c| c.2);
    out.into_iter().filter(|c| (c.2 - y0).abs() < 0.01).map(|c| (c.0, c.1)).collect()
}

fn the_end_stop_takes_half_the_pitch_first() {
    const PITCH: f32 = 11.0 + 4290.0 / 4096.0;
    let rpr = r#"<w:rFonts w:ascii="ＭＳ 明朝" w:hAnsi="ＭＳ 明朝" w:eastAsia="ＭＳ 明朝"/><w:sz w:val="22"/><w:szCs w:val="22"/>"#; // i18n-ok（書体名）
    // 39 characters in a column of 37.46 pitches: the stops are 、 (the 18th), 「 (the 29th), 」 (the 34th) and the end 。.
    let text: String = (0..39)
        .map(|i| match i {
            17 => '、',
            28 => '「',
            33 => '」',
            38 => '。',
            _ => 'あ',
        })
        .collect(); // i18n-ok（試験の文字列）
    let body = format!(r#"<w:p><w:pPr><w:overflowPunct w:val="0"/><w:jc w:val="both"/></w:pPr><w:r><w:rPr>{rpr}</w:rPr><w:t>{text}</w:t></w:r></w:p>"#);
    let file = make_control("squeeze-share", &body, r#"<w:docGrid w:type="linesAndChars" w:linePitch="360" w:charSpace="4290"/>"#, 15, "compressPunctuation");
    let line = first_line(&pages_of(&file));
    assert_eq!(line.len(), 39, "the line took the 39th character in: {}", line.iter().map(|c| c.0).collect::<String>());
    let gap = |i: usize| line[i + 1].1 - line[i].1;
    // What the line needs: 39 pitches over 451.3 pt. The end stop takes half the pitch, the three stops of the line share the rest, 、 and 」 after themselves, 「 before itself.
    let need = 39.0 * PITCH - 451.3;
    let rest = (need - PITCH / 2.0) / 3.0;
    assert!(need > PITCH / 2.0 && rest > 0.0, "the line needs {need}");
    near(gap(17), PITCH - rest, 0.3, "after 、");
    near(gap(27), PITCH - rest, 0.3, "before 「");
    near(gap(33), PITCH - rest, 0.3, "after 」");
    for i in [0, 5, 16, 18, 26, 28, 32, 34, 36] {
        near(gap(i), PITCH, 0.3, &format!("kana {i} is not squeezed"));
    }
    // The end stop's own squeeze: the line ends at the column's right edge, so the last character starts where the squeezes put it.
    let last_start = line[38].1;
    near(last_start, line[0].1 + 38.0 * PITCH - 3.0 * rest, 0.4, "the end stop starts after 38 pitches less the stops' squeezes");
}
