// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 縦中横 (`w:eastAsianLayout w:vert`) in a section of vertical writing, checked without a window against Word's PDFs (GitHub issue
//! #94; Word 16.0 of Microsoft 365 on Windows, `ExportAsFixedFormat`, positions by `pdftotext -bbox`): a small file of
//! `test/corpus-report/docx/` (`tate-chu-yoko-*.docx`: three paragraphs of A4 `tbRl`, the middle one 「あ」+ digits + 「あああ」, the
//! digits 縦中横 with 行の幅に合わせる (`fit`, `w:w` stored as Word's dialog computes it) or without (`off`), 12 pt, no grid unless the
//! name says) is laid out by a scenario runner (macOS and Linux, `--test scenarios -- tate_chu_yoko`).
//!
//! What Word's PDFs show, whatever the digits (2 to 8) and the size (9 to 24 pt):
//! - the run goes as far down the line as a line of its typeface is high (12 pt: ＭＳ 15.62 pt, 游明朝・游ゴシック 20.06, メイリオ 23.42, Arial
//!   13.82; 9 to 24 pt in ＭＳ and 游明朝 the same in em), not the 1 em of a kana;
//! - its digits are horizontal text on the middle of the kana's box, and with 行の幅に合わせる off a line as thick as the digits are wide
//!   (n digits of 0.5 em) when that is more than the line: the lines before and after move apart by the difference.

use std::sync::Arc;

use super::flow_layout::{Item, Pagination};
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::tate_chu_yoko_run_goes_down_the_line_as_far_as_a_line_is_high", a_tate_chu_yoko_run_goes_down_the_line_as_far_as_a_line_is_high),
        ("docx::tate_chu_yoko_digits_are_in_the_middle_of_the_kana_box", the_digits_of_a_tate_chu_yoko_run_are_in_the_middle_of_the_kana_box),
        ("docx::tate_chu_yoko_fit_squeezes_the_digits_and_keeps_the_line", fit_squeezes_the_digits_and_keeps_the_line),
        ("docx::tate_chu_yoko_wide_run_widens_its_line_when_it_is_not_fitted", a_wide_run_widens_its_line_when_it_is_not_fitted),
    ]
}

fn laid_out(file: &str) -> Arc<Pagination> {
    let mut s = Scenario::open(&format!("test/corpus-report/docx/{file}"));
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

const SIZE: f32 = 12.0;

/// A glyph of the first page: its box's left, baseline and squeeze.
struct Drawn {
    c: char,
    x: f32,
    y: f32,
    xscale: f32,
}

fn drawn(file: &str) -> Vec<Drawn> {
    let pages = laid_out(file);
    let mut out = Vec::new();
    for item in &pages.pages[0].items {
        match item {
            Item::Text { x, y, text, offsets, .. } => out.extend(text.chars().zip(offsets.iter()).map(|(c, dx)| Drawn { c, x: x + dx, y: *y, xscale: 1.0 })),
            Item::Scaled { x, y, text, xscale, .. } => out.extend(text.chars().map(|c| Drawn { c, x: *x, y: *y, xscale: *xscale })),
            _ => {}
        }
    }
    out
}

/// The three columns' kana (right to left) and the digits.
struct Columns {
    /// x of the left of each column's kana, right to left.
    x: [f32; 3],
    /// The middle column's kana, top to bottom (the first one before the digits, the second one after them).
    middle: Vec<(f32, f32)>,
    digits: Vec<Drawn>,
}

fn columns(file: &str) -> Columns {
    let all = drawn(file);
    let mut xs: Vec<f32> = Vec::new();
    for g in all.iter().filter(|g| g.c == 'あ') {
        if !xs.iter().any(|x| (x - g.x).abs() < 0.5) {
            xs.push(g.x);
        }
    }
    xs.sort_by(|a, b| b.total_cmp(a));
    assert_eq!(xs.len(), 3, "{file}: three columns, {xs:?}");
    let mut middle: Vec<(f32, f32)> = all.iter().filter(|g| g.c == 'あ' && (g.x - xs[1]).abs() < 0.5).map(|g| (g.x, g.y)).collect();
    middle.sort_by(|a, b| a.1.total_cmp(&b.1));
    let digits = all.into_iter().filter(|g| g.c.is_ascii_digit()).collect();
    Columns { x: [xs[0], xs[1], xs[2]], middle, digits }
}

fn near(actual: f32, expected: f32, tolerance: f32) -> bool {
    (actual - expected).abs() <= tolerance
}

/// The run goes down the line as far as a line of its typeface is high, whatever the digits and 行の幅に合わせる (Word: kana after it 15.62 pt
/// below the baseline of the one before it, 12 pt + 3.62 pt; 游明朝 20.06 + 12 = 32.06; ibis's ＭＳ line is 1.2857 em, 0.19 pt short at 12 pt).
fn a_tate_chu_yoko_run_goes_down_the_line_as_far_as_a_line_is_high() {
    for (file, word_cell) in [
        ("tate-chu-yoko-msg-fit-3.docx", 15.62),
        ("tate-chu-yoko-msg-off-3.docx", 15.62),
        ("tate-chu-yoko-msg-off-6.docx", 15.62),
        ("tate-chu-yoko-msg-off-8.docx", 15.62),
        ("tate-chu-yoko-msg-grid-off-6.docx", 15.62),
        ("tate-chu-yoko-msm-off-6.docx", 15.62),
        ("tate-chu-yoko-yum-fit-3.docx", 20.06),
        ("tate-chu-yoko-yum-off-4.docx", 20.06),
        ("tate-chu-yoko-yum-off-8.docx", 20.06),
        // The others' own line heights: 游ゴシック 1.672 em, メイリオ 1.95 em (Arial 1.152 em: this layout's Arial line leaves out the font's
        // line gap, 1.117 em, so it is not checked).
        ("tate-chu-yoko-yug-fit-3.docx", 20.06),
        ("tate-chu-yoko-mei-fit-3.docx", 23.42),
    ] {
        let c = columns(file);
        let (before, after) = (c.middle[0].1, c.middle[1].1);
        let cell = after - before - SIZE;
        assert!(near(cell, word_cell, 0.25), "{file}: the run takes {cell:.2} pt down the line; Word: {word_cell} pt");
    }
}

/// The digits lie on the middle of the kana's box, in the middle of their cell down the line (Word's bboxes: the kana's left plus half of
/// what it is wider than the digits; 491.62 + 7.74 = 499.36 for 15.48 pt of squeezed digits on the kana at 493.3 + 6).
fn the_digits_of_a_tate_chu_yoko_run_are_in_the_middle_of_the_kana_box() {
    // (file, digits' width across the line in Word's PDF, digits' top below the kana's top). ＭＳ only: the width of 游明朝's digits is its
    // file's (0.5405 em in Word's PDF), and the PCs' files differ.
    for (file, width, below) in [
        ("tate-chu-yoko-msg-fit-3.docx", 15.48, 13.82),
        ("tate-chu-yoko-msg-off-3.docx", 18.0, 13.82),
        ("tate-chu-yoko-msg-off-6.docx", 36.0, 13.82),
        ("tate-chu-yoko-msg-off-8.docx", 48.0, 13.82),
    ] {
        let c = columns(file);
        let kana_mid = c.middle[0].0 + SIZE / 2.0;
        let first = c.digits.first().unwrap_or_else(|| panic!("{file}: the digits are drawn"));
        let mid = first.x + width / 2.0;
        let xs: Vec<f32> = c.digits.iter().map(|d| d.x).collect();
        assert!(near(mid, kana_mid, 0.4), "{file}: the digits' middle is {mid:.2}; the kana's is {kana_mid:.2} (digits at {xs:?}, kana column {:?}, columns {:?})", c.middle, c.x);
        let down = first.y - c.middle[0].1;
        assert!(near(down, below, 0.5), "{file}: the digits' baseline is {down:.2} pt below the kana's; Word {below} pt");
    }
}

/// 行の幅に合わせる: the digits are squeezed by `w:w` (one glyph each, each moving on by its squeezed advance), and do not widen the line.
fn fit_squeezes_the_digits_and_keeps_the_line() {
    let fit = columns("tate-chu-yoko-msg-fit-3.docx");
    let plain = columns("tate-chu-yoko-msg-plain-0.docx");
    assert_eq!(fit.digits.len(), 3);
    // `w:w` 86 (floor(100 x 1.29 / 1.5)).
    assert!(fit.digits.iter().all(|d| near(d.xscale, 0.86, 0.001)), "squeezed to 86 %");
    let advance = fit.digits[1].x - fit.digits[0].x;
    assert!(near(advance, 6.0 * 0.86, 0.05), "each digit moves on by {advance:.2} pt; Word: 0.5 em x 86 % = 5.16");
    assert!(near(fit.x[0] - fit.x[2], plain.x[0] - plain.x[2], 0.05), "the three lines are as far apart as without it");
}

/// 行を広げる: the line holding the digits is as thick as they are wide when that is more than a line, the kana in the middle of it, so
/// that the lines before and after it move apart by half of the difference each (Word, 12 pt ＭＳ: 3 / 6 / 8 digits, 18 / 36 / 48 pt wide, the
/// three lines 33.96 / 51.96 / 63.96 pt apart where lines of kana alone are 32.16; at 9 to 24 pt, 3 and 6 digits and 游明朝 the same).
/// The thickness of the other lines is what this layout gives a line, so the check is that the middle line is `W` thick: the three lines
/// are `W / 2` + `W` + ... apart, i.e. `x1 - x3 = pitch + W` with `pitch` of a line of kana alone.
fn a_wide_run_widens_its_line_when_it_is_not_fitted() {
    let plain = columns("tate-chu-yoko-msg-plain-0.docx");
    let pitch = (plain.x[0] - plain.x[2]) / 2.0;
    for (file, width) in [("tate-chu-yoko-msg-off-3.docx", 18.0), ("tate-chu-yoko-msg-off-6.docx", 36.0), ("tate-chu-yoko-msg-off-8.docx", 48.0), ("tate-chu-yoko-msm-off-6.docx", 36.0)] {
        let c = columns(file);
        let apart = c.x[0] - c.x[2];
        assert!(near(apart, pitch + width, 0.3), "{file}: the lines are {apart:.2} pt apart; the line {pitch:.2} pt and the digits {width} pt wide make {:.2}", pitch + width);
        // The kana of the line stay in the middle of it: the line before is as far from them as the line after.
        assert!(near(c.x[0] - c.x[1], c.x[1] - c.x[2], 0.3), "{file}: the middle line's kana are in the middle of its width");
    }
}
