// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #54, what is left: the pitch of the columns of vertical text without a line grid, and the line pitch of ＭＳ 明朝 in horizontal text
//! (Word's XPS of `issue54-symbols.docx`, Microsoft 365 on Windows, `tasks/doc/requests/2026-11-14-54-remaining-result.md`; the columns of its
//! page 2 are 14 pt ＭＳ 明朝 18.7 pt apart, ＭＳ ゴシック 18.3 to 18.9, 游明朝 and 游ゴシック 24.0; the horizontal lines of page 1 18.12 to 18.24 pt).
//!
//! ＭＳ 明朝's line is 332/256 em in Word (`ms_room_em`: horizontal 18.16 pt at 14 pt, an ordinary test now), and its vertical columns are 344/256 em
//! (`vertical_pitch_em`: 12/256 em more than the line, 18.81 pt at 14 pt; Word's own probe of plain text, `2026-11-14-54-vertical-pitch-result.md`, 18.812 pt).
//! The columns of this document are told by the mean x of their glyphs, which moves 0.3 to 0.4 pt from column to column with the symbols in it (upright and
//! turned glyphs sit at different offsets of the column): ibisOffice's single distances are 18.45 and 19.18 pt (ＭＳ 明朝) and Word's 18.5 to 18.8, so a test
//! of one distance measures the symbols, not the pitch. The ＭＳ tests take the pitch as the two distances' mean (the first column to the third, over two), which
//! the symbols' offsets do not move much, against Word's 344/256 em.

use std::sync::Arc;

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue54_pitch_vertical_columns_of_ms_mincho_are_18_8_apart", vertical_ms_mincho_columns as fn()),
        ("docx::issue54_pitch_vertical_columns_of_ms_gothic_are_18_8_apart", vertical_ms_gothic_columns),
        ("docx::issue54_pitch_vertical_columns_of_yu_mincho_are_24_apart", vertical_yu_mincho_columns),
        ("docx::issue54_pitch_vertical_columns_of_yu_gothic_are_24_apart", vertical_yu_gothic_columns),
        ("docx::issue54_pitch_horizontal_line_of_ms_mincho_14pt_is_18_2", horizontal_ms_mincho_line),
        ("docx::issue54_pitch_line_and_column_of_27_typefaces_are_s_plus_2_round_0_15_s", line_and_column_units),
    ]
}

fn laid_out() -> Arc<Pagination> {
    let mut s = Scenario::open("test/corpus-report/docx/issue54-symbols.docx");
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

/// The positions along the writing's cross axis (x of page 2's columns, y of page 1's lines) of the 14 pt text, one per column (of at least 8 glyphs, one item each) or per line (one item each) (the 11 pt headings are not counted). The document has four paragraphs of 71 symbols in ＭＳ 明朝, ＭＳ ゴシック, 游明朝 and
/// 游ゴシック, each three columns (or lines) long and in this order; the typeface is not told from the drawn font (a Mac draws them in its own).
fn clusters(vertical: bool) -> Vec<f32> {
    let pages = laid_out();
    let page = &pages.pages[if vertical { 1 } else { 0 }];
    let mut at: Vec<f32> = page
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Text { x, y, size, .. } if (*size - 14.0).abs() < 0.01 => Some(if vertical { *x } else { *y }),
            _ => None,
        })
        .collect();
    at.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // A column is where most of its glyphs stand (the median): the few symbols drawn at other offsets of the column (upright ones, from a
    // fallback typeface) are inside the 4 pt a group takes in, and a mean would move the column by them (a pair of them 3.95 pt off made
    // 游明朝's columns 23.75 apart where the glyphs' own lines are 24.00 apart).
    let mut groups: Vec<Vec<f32>> = Vec::new();
    for v in at {
        match groups.last_mut() {
            Some(g) if (v - g[0]).abs() < 4.0 => g.push(v),
            _ => groups.push(vec![v]),
        }
    }
    let mut out: Vec<f32> = groups.into_iter().filter(|g| g.len() >= if vertical { 8 } else { 1 }).map(|g| g[g.len() / 2]).collect();
    if vertical {
        out.reverse(); // the first column is on the right
    }
    out
}

/// The two distances between the three columns of paragraph `n` (0 to 3) of the vertical page.
fn pitches(n: usize) -> [f32; 2] {
    let c = clusters(true);
    assert!(c.len() >= 12, "four paragraphs of three columns: {c:?}");
    [c[n * 3] - c[n * 3 + 1], c[n * 3 + 1] - c[n * 3 + 2]]
}

fn expect_columns(n: usize, word: f32, tolerance: f32, name: &str) {
    for d in pitches(n) {
        assert!((d - word).abs() < tolerance, "the columns of {name} 14 pt are {d} apart; Word's are {word}");
    }
}

/// The mean of the two distances (the first column to the third, over two).
fn expect_mean_pitch(n: usize, word: f32, tolerance: f32, name: &str) {
    let p = pitches(n);
    let d = (p[0] + p[1]) / 2.0;
    assert!((d - word).abs() < tolerance, "the columns of {name} 14 pt are {d} apart on average; Word's are {word}");
}

fn vertical_ms_mincho_columns() {
    expect_mean_pitch(0, 344.0 / 256.0 * 14.0, 0.1, "ＭＳ 明朝");
}

fn vertical_ms_gothic_columns() {
    expect_mean_pitch(1, 344.0 / 256.0 * 14.0, 0.1, "ＭＳ ゴシック");
}

fn vertical_yu_mincho_columns() {
    expect_columns(2, 24.0, 0.2, "游明朝");
}

fn vertical_yu_gothic_columns() {
    expect_columns(3, 24.0, 0.2, "游ゴシック");
}

fn horizontal_ms_mincho_line() {
    let l = clusters(false);
    assert!(l.len() >= 3, "{l:?}");
    for d in [l[1] - l[0], l[2] - l[1]] {
        assert!((d - 18.2).abs() < 0.1, "the lines of ＭＳ 明朝 14 pt are {d} apart; Word's are 18.12 to 18.24");
    }
}

/// Word's line `H = S + 2 round(0.15 S)` and column `V = S' + 2 round(0.15 S')`, `S' = S + upem / 32` (font units; Windows Word's XPS, `2026-11-16-54-ms-pitch-result.md`): the (upem, S, H, V)
/// of its typefaces: ＭＳ / HG, けいふぉんと, BIZ UD, UD デジタル教科書体, メイリオ, Meiryo UI, 游ゴシック / 游明朝, Yu Gothic UI.
fn line_and_column_units() {
    use super::flow_layout::{cjk_extra_units_for_test, vertical_extra_units_for_test};
    for (upem, s, h, v) in [(256.0, 256.0, 332.0, 344.0), (1024.0, 1134.0, 1474.0, 1516.0), (2048.0, 2048.0, 2662.0, 2746.0), (2048.0, 2369.0, 3079.0, 3163.0), (2048.0, 3072.0, 3994.0, 4076.0),
        (2048.0, 2601.0, 3381.0, 3465.0), (2048.0, 2636.0, 3426.0, 3510.0), (2048.0, 2724.0, 3542.0, 3624.0)] {
        assert_eq!(s + cjk_extra_units_for_test(s), h, "H of S = {s} (upem {upem})");
        assert_eq!(s + cjk_extra_units_for_test(s) + vertical_extra_units_for_test(s, upem), v, "V of S = {s} (upem {upem})");
    }
}
