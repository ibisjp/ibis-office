// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #196: the lines of a paragraph that goes round a figure (`wrapSquare`, `bothSides`) are split, tabbed and indented as the same text set in the same width
//! without the figure (the paragraph's left indent narrowed by the figure's width and the 9 pt it keeps off the text). Each test sets a paragraph twice, beside a figure and
//! with the indent in its place, and compares the characters of each line and where they stand; the values are the issue's measurements of Word's PDFs
//! (ＭＳ 明朝 10.5 pt, A4, 72 pt margins, a column of 451.3 pt, compatibility mode 15; tolerance 0.25 pt, Word's PDFs have 0.12 pt steps).

use super::flow_layout::{Item, Pagination};
use super::issue191_tests::{make_control, near, pages_of, text_box};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue196_lines_beside_a_figure_break_as_the_same_text_without_it", lines_beside_a_figure_break_as_the_same_text_without_it),
        ("docx::issue196_stops_squeeze_beside_a_figure_as_without_it", stops_squeeze_beside_a_figure_as_without_it),
        ("docx::issue196_a_positional_tab_aligns_to_the_gap_beside_the_figure", a_positional_tab_aligns_to_the_gap_beside_the_figure),
        ("docx::issue196_the_first_line_indent_counts_from_the_gap_beside_the_figure", the_first_line_indent_counts_from_the_gap_beside_the_figure),
        ("docx::issue196_ptab_stops_of_each_alignment_beside_a_figure_on_the_left", ptab_stops_of_each_alignment_beside_a_figure_on_the_left),
        ("docx::issue196_ptab_stops_of_each_alignment_beside_a_figure_on_the_right", ptab_stops_of_each_alignment_beside_a_figure_on_the_right),
        ("docx::issue196_a_left_ptab_after_text_goes_to_the_next_line_and_the_others_stay", a_left_ptab_after_text_goes_to_the_next_line_and_the_others_stay),
        ("docx::issue196_hanging_and_first_line_beside_a_figure_on_the_left", hanging_and_first_line_beside_a_figure_on_the_left),
    ]
}

const PT: i64 = 12_700;
const SQUARE: &str = r#"<wp:wrapSquare wrapText="bothSides"/>"#;

fn run(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

fn ptab(relative_to: &str) -> String {
    format!(r#"<w:r><w:ptab w:relativeTo="{relative_to}" w:alignment="right" w:leader="none"/></w:r>"#)
}

/// The characters (not spaces) of each line of the paragraph on page 0 (the text items on one baseline), without the text of the figure's own box.
fn counts(p: &Pagination) -> Vec<usize> {
    let mut rows: Vec<(f32, usize)> = Vec::new();
    for it in &p.pages[0].items {
        if let Item::Text { y, text, .. } | Item::Scaled { y, text, .. } = it {
            if text.starts_with("アイウ") {
                continue;
            }
            let n = text.chars().filter(|c| !c.is_whitespace()).count();
            match rows.iter_mut().find(|r| (r.0 - *y).abs() < 0.01) {
                Some(r) => r.1 += n,
                None => rows.push((*y, n)),
            }
        }
    }
    rows.into_iter().map(|r| r.1).collect()
}

/// Where the text item that starts with `start` begins and ends (its offsets are those of the characters' left edges; the last is full-width).
fn span_of(p: &Pagination, start: &str) -> (f32, f32) {
    p.pages[0]
        .items
        .iter()
        .find_map(|it| match it {
            Item::Text { x, text, offsets, size, .. } if text.starts_with(start) => Some((*x, x + offsets.last().copied().unwrap_or(0.0) + size)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no text {start}"))
}

/// A figure `figure_w` pt wide and 400 pt high, its left edge `at` tenths of a point from the column's, then the paragraph's runs.
fn beside(figure_w: i64, at: i64, ppr: &str, runs: &str) -> String {
    format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="800"/>{ppr}</w:pPr>{}{runs}</w:p>"#, text_box(101, "column", at * PT / 10, SQUARE, figure_w * PT, 400 * PT))
}

/// The same text with no figure and `ind` (XML) in its place.
fn without(ppr: &str, ind: &str, runs: &str) -> String {
    format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="800"/>{ind}{ppr}</w:pPr>{runs}</w:p>"#)
}

/// The issue's four scenes of lines (150 pt figure on the left, a band of 292.3 pt = the column less 159 pt): a left aligned line that ends in a stop (P1), a justified one
/// that squeezes stops (H1), one with Latin letters and digits (L1) and one with many stops (Q1). Beside the figure the lines are those of the text without it, which are
/// Word's.
fn lines_beside_a_figure_break_as_the_same_text_without_it() {
    let scenes: [(&str, &str, &str, &str, &[usize]); 4] = [
        ("p1", "left", "doNotCompress", "かまべるごぜいむはや、ンパミドセグハやりわりじぺらじむ、にこちなぱれぴる、がんぴぐぐりりそかねぴさむぽずきへべらはとへもさねみえプヘロスゴデム人会田天182,943、島", &[26, 27, 31]),
        ("h1", "both", "compressPunctuation", "おぬちふいあじひいoovreofelv々わくやわぽぼぼりてきて、。「こぶぬうさちなぎいぜそ、をおもれおぽしがなび：くんろ、、ぴよつれひひぴせだよぼ「ヲロビペサ」3956えひこうひへげべるむ", &[33, 29, 30, 2]),
        ("l1", "both", "doNotCompress", "とやぺだにばにせそどをぽとぐろぞけぺymqgfdzxluyざだずべふずくせぽそきめへぶがねいぴぬにひめわはぱちやぬねまばぺわぎぽつゾタピタンゼガダー。々々952726.479,237.2ぽぷ", &[33, 27, 35]),
        ("q1", "both", "compressPunctuation", "とるつざずわどそご、、、もさぷはふつのゴフキハアミロフガユーヘゴフビ、、「う、金力」とびえ【トドミコビ】たずよみじぬん耳日耳糸事人生学生見入【ベジドバ】ぐおききはぎわす木水国出右糸耳。、カヲマペテヤまひいかるぎゴヲエミヨーユ。", &[28, 28, 28, 29]),
    ];
    for (name, jc, control, text, word) in scenes {
        let ppr = format!(r#"<w:jc w:val="{jc}"/>"#);
        let a = make_control(&format!("196-{name}a"), &beside(150, 0, &ppr, &run(text)), "", 15, control);
        let b = make_control(&format!("196-{name}b"), &without("", r#"<w:ind w:left="3180"/>"#, &run(text)).replace("<w:spacing", &format!("{ppr}<w:spacing")), "", 15, control);
        let (pa, pb) = (pages_of(&a), pages_of(&b));
        let (with_figure, plain) = (counts(&pa), counts(&pb));
        assert_eq!(plain, word, "{name}: the same text without the figure is Word's");
        assert_eq!(with_figure, plain, "{name}: beside the figure the lines are those of the text without it");
    }
}

/// Where each character stands, line by line (the text items on one baseline, without the figure's own box).
fn stands(p: &Pagination) -> Vec<(i64, f32)> {
    let mut out = Vec::new();
    for it in &p.pages[0].items {
        if let Item::Text { x, y, text, offsets, .. } = it && !text.starts_with("アイウ") {
            for (k, _) in text.chars().enumerate() {
                out.push(((*y * 100.0).round() as i64, *x + offsets.get(k).copied().unwrap_or(0.0)));
            }
        }
    }
    out
}

/// The stops a justified line takes in (約物を詰める) are chosen as they are in the same text without the figure, though the anchor of the figure is a unit of the line (Word's PDF of
/// the issue's `n6-wrap-c.docx`: H1a's first line ends 、。 with the 。 squeezed to half and the 、 whole, 10.44 pt apart; Q1a's three 、 of the first line 9.96, 9.84, 9.96 pt apart).
fn stops_squeeze_beside_a_figure_as_without_it() {
    let scenes = [
        ("h1", "おぬちふいあじひいoovreofelv々わくやわぽぼぼりてきて、。「こぶぬうさちなぎいぜそ、をおもれおぽしがなび：くんろ、、ぴよつれひひぴせだよぼ「ヲロビペサ」3956えひこうひへげべるむ"),
        ("q1", "とるつざずわどそご、、、もさぷはふつのゴフキハアミロフガユーヘゴフビ、、「う、金力」とびえ【トドミコビ】たずよみじぬん耳日耳糸事人生学生見入【ベジドバ】ぐおききはぎわす木水国出右糸耳。、カヲマペテヤまひいかるぎゴヲエミヨーユ。"),
    ];
    for (name, text) in scenes {
        let ppr = r#"<w:jc w:val="both"/>"#;
        let a = make_control(&format!("196-{name}c"), &beside(150, 0, ppr, &run(text)), "", 15, "compressPunctuation");
        let b = make_control(&format!("196-{name}d"), &without("", r#"<w:ind w:left="3180"/>"#, &run(text)).replace("<w:spacing", &format!("{ppr}<w:spacing")), "", 15, "compressPunctuation");
        let (xa, xb) = (stands(&pages_of(&a)), stands(&pages_of(&b)));
        assert_eq!(xa.len(), xb.len(), "{name}: the same characters");
        for (k, (pa, pb)) in xa.iter().zip(&xb).enumerate() {
            assert_eq!(pa.0, pb.0, "{name}: character {k} is on the same line");
            near(pa.1, pb.1, 0.25, &format!("{name}: character {k}"));
        }
    }
}

/// A right-aligned positional tab beside a figure on the right ends at the right edge of the gap (Word: 434.5 pt, the text after it from 381.8 pt), relative to the margin
/// and to the indent; relative to the margin it also ends at the right indent when there is no figure (an indent of 89 pt: 434.5 pt).
fn a_positional_tab_aligns_to_the_gap_beside_the_figure() {
    for (name, relative_to) in [("t3", "margin"), ("t4", "indent")] {
        let runs = format!("{}{}{}", run("かきくけこ"), ptab(relative_to), run("さしすせそ"));
        let a = make_control(&format!("196-{name}a"), &beside(80, 3713, "", &runs), "", 15, "doNotCompress");
        let b = make_control(&format!("196-{name}b"), &without("", r#"<w:ind w:right="1780"/>"#, &runs), "", 15, "doNotCompress");
        for (what, file) in [("beside the figure", a), ("with the right indent", b)] {
            let (x, end) = span_of(&pages_of(&file), "さしすせそ");
            near(x, 381.8, 0.25, &format!("{name} {what}: the text after the tab starts"));
            near(end, 434.5, 0.25, &format!("{name} {what}: the text after the tab ends"));
        }
    }
}

/// `firstLine` beside a figure on the left: the line starts at the gap's left edge and is indented from there (Word: 203.1 pt for a gap that starts at 161.1 pt, 42 pt in),
/// where the same paragraph without the figure and with the indent that narrows it starts at 203.0 pt.
fn the_first_line_indent_counts_from_the_gap_beside_the_figure() {
    let text = "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんあいうえおかきくけこさしすせ";
    let a = make_control("196-i1a", &beside(80, 0, r#"<w:ind w:firstLine="840"/>"#, &run(text)), "", 15, "doNotCompress");
    let b = make_control("196-i1b", &without(r#"<w:ind w:firstLine="840" w:left="1780"/>"#, "", &run(text)), "", 15, "doNotCompress");
    let (xa, _) = span_of(&pages_of(&a), "あいう");
    let (xb, _) = span_of(&pages_of(&b), "あいう");
    near(xb, 203.0, 0.25, "without the figure");
    near(xa, xb, 0.25, "beside the figure the first line starts where it does without it");
}

fn ptab_of(alignment: &str, relative_to: &str) -> String {
    format!(r#"<w:r><w:ptab w:relativeTo="{relative_to}" w:alignment="{alignment}" w:leader="none"/></w:r>"#)
}

/// Where the five characters after a tab of `alignment` start, for the indent XML `ind`, beside a figure 80 pt wide that is `at` tenths of a point from the column's left
/// edge, and where the same tab starts them with no figure when `plain` is set.
fn after_ptab(name: &str, at: i64, ind: &str, alignment: &str, relative_to: &str, before: &str) -> f32 {
    let runs = format!("{}{}{}", if before.is_empty() { String::new() } else { run(before) }, ptab_of(alignment, relative_to), run("あいうえお"));
    let ppr = format!("<w:jc w:val=\"left\"/>{ind}");
    let doc = make_control(name, &beside(80, at, &ppr, &runs), "", 15, "doNotCompress");
    span_of(&pages_of(&doc), "あいうえお").0
}

/// Word's XPS of the issue #196 file, a figure 80 pt wide and 400 pt high on the left (the gap starts at 161.06 pt), the text after a positional tab: a left tab stops at the
/// larger of the left indent and the gap's left edge (the margin's tab too); a centred one at the middle of the margin (or the indent) and the column's right edge, with no
/// regard to the right indent or the figure; a right one ends at the column's right edge less the right indent.
fn ptab_stops_of_each_alignment_beside_a_figure_on_the_left() {
    let left = |pt: i64| format!(r#"<w:ind w:left="{}"/>"#, pt * 20);
    let right89 = r#"<w:ind w:right="1780"/>"#.to_string();
    let none = String::new();
    // (alignment, relativeTo, indent XML, Word's x)
    let cases: Vec<(&str, &str, String, f32)> = vec![
        ("left", "margin", none.clone(), 161.06),
        ("left", "margin", left(40), 161.06),
        ("left", "margin", left(150), 222.05),
        ("left", "indent", none.clone(), 161.06),
        ("left", "indent", left(40), 161.06),
        ("left", "indent", left(150), 222.05),
        ("left", "indent", right89.clone(), 161.06),
        ("center", "margin", none.clone(), 271.37),
        ("center", "margin", left(40), 271.37),
        ("center", "margin", left(150), 271.37),
        ("center", "margin", right89.clone(), 271.37),
        ("center", "indent", none.clone(), 271.37),
        ("center", "indent", left(40), 291.41),
        ("center", "indent", left(150), 346.39),
        ("center", "indent", right89.clone(), 271.37),
        ("right", "margin", none.clone(), 470.86),
        ("right", "margin", left(40), 470.86),
        ("right", "margin", right89.clone(), 381.79),
        ("right", "indent", none, 470.86),
        ("right", "indent", left(40), 470.86),
        ("right", "indent", right89, 381.79),
    ];
    for (n, (alignment, relative_to, ind, word)) in cases.into_iter().enumerate() {
        let x = after_ptab(&format!("196-pl{n}"), 0, &ind, alignment, relative_to, "");
        near(x, word, 0.25, &format!("{alignment} to the {relative_to}, indent {ind}"));
    }
}

/// The same tabs beside a figure on the right (443.3 to 523.3 pt, 9 pt kept off the text, so the gap ends at 434.3 pt): the left and centred tabs are those of no figure;
/// a right tab ends at the gap's right edge.
fn ptab_stops_of_each_alignment_beside_a_figure_on_the_right() {
    let left40 = r#"<w:ind w:left="800"/>"#;
    let cases: [(&str, &str, &str, f32); 8] = [
        ("left", "margin", "", 72.02),
        ("left", "indent", left40, 111.98),
        ("center", "margin", "", 271.37),
        ("center", "margin", left40, 271.37),
        ("center", "indent", "", 271.37),
        ("center", "indent", left40, 291.41),
        ("right", "margin", "", 381.79),
        ("right", "indent", left40, 381.79),
    ];
    for (n, (alignment, relative_to, ind, word)) in cases.into_iter().enumerate() {
        let x = after_ptab(&format!("196-pr{n}"), 3713, ind, alignment, relative_to, "");
        near(x, word, 0.25, &format!("{alignment} to the {relative_to}, indent {ind}"));
    }
}

/// With characters before it ("ＡＢ" starts at the gap's left edge, 161.06 pt) a left tab goes to the next line, the text after it at the gap's left edge (Word); a centred
/// or a right tab stays on the line, the text after them where they are without the characters.
fn a_left_ptab_after_text_goes_to_the_next_line_and_the_others_stay() {
    let line_of = |name: &str, alignment: &str| {
        let doc = make_control(name, &beside(80, 0, "", &format!("{}{}{}", run("ＡＢ"), ptab_of(alignment, "margin"), run("あいうえお"))), "", 15, "doNotCompress");
        let p = pages_of(&doc);
        let y_of = |start: &str| {
            p.pages[0].items.iter().find_map(|it| match it {
                Item::Text { y, text, .. } if text.starts_with(start) => Some(*y),
                _ => None,
            })
        };
        (span_of(&p, "ＡＢ").0, span_of(&p, "あいうえお").0, y_of("ＡＢ").unwrap(), y_of("あいうえお").unwrap())
    };
    let (ab, x, ya, yb) = line_of("196-wl", "left");
    near(ab, 161.06, 0.25, "the characters before the tab start at the gap's left edge");
    near(x, 161.06, 0.25, "the text after a left tab starts the next line at the gap's left edge");
    assert!(yb > ya + 5.0, "the text after a left tab is on the next line ({ya} {yb})");
    let (_, x, ya, yb) = line_of("196-wc", "center");
    near(x, 271.37, 0.25, "centred");
    assert!((yb - ya).abs() < 0.01, "centred tab stays on the line");
    let (_, x, ya, yb) = line_of("196-wr", "right");
    near(x, 470.86, 0.25, "right");
    assert!((yb - ya).abs() < 0.01, "right tab stays on the line");
}

/// `hanging` 21 pt with a left indent of 150 pt and a left tab to the indent: the first line starts at 201.05 pt and the tab takes the text after it to 222.05 pt; with a
/// `firstLine` of 42 pt the tab changes nothing (203.09 pt: the gap's edge and the indent). A hanging first line at an indent that is in the figure's band starts at the gap's left
/// edge, and the lines after it too.
fn hanging_and_first_line_beside_a_figure_on_the_left() {
    let x = after_ptab("196-h1", 0, r#"<w:ind w:left="3000" w:hanging="420"/>"#, "left", "indent", "");
    near(x, 222.05, 0.25, "hanging, left tab to the indent");
    let x = after_ptab("196-h2", 0, r#"<w:ind w:firstLine="840"/>"#, "left", "indent", "");
    near(x, 203.09, 0.25, "firstLine, left tab");
    let text = "あいうえおかきくけこさしすせそたちつてとなにぬねのはひふへほまみむめもやゆよらりるれろわをんあいうえおかきくけこさしすせ";
    for (n, (ind, first, rest)) in [(r#"<w:ind w:left="0" w:hanging="420"/>"#, 161.06, 161.06), (r#"<w:ind w:left="1780" w:hanging="420"/>"#, 161.06, 161.06), (r#"<w:ind w:left="3000" w:hanging="420"/>"#, 201.05, 222.05)].into_iter().enumerate() {
        let doc = make_control(&format!("196-h3{n}"), &beside(80, 0, ind, &run(&format!("{text}あ"))), "", 15, "doNotCompress");
        let p = pages_of(&doc);
        let mut starts: Vec<(f32, f32)> = Vec::new();
        for it in &p.pages[0].items {
            if let Item::Text { x, y, text, .. } = it && !text.starts_with("アイウ") && !starts.iter().any(|s| (s.1 - *y).abs() < 0.01) {
                starts.push((*x, *y));
            }
        }
        near(starts[0].0, first, 0.25, &format!("hanging {ind}: the first line"));
        near(starts[1].0, rest, 0.25, &format!("hanging {ind}: the second line"));
    }
}
