// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/272)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The plain text of a page, from the boxes of its characters ([`PageTextIndex`]).
//!
//! A PDF draws glyphs at positions; it does not say where a line ends or where a space is. The text is
//! rebuilt from the geometry: characters on one baseline make a line, read left to right, and a gap between two characters
//! makes a space. Characters the page sets one below another (縦書き, vertical writing) make columns, read top to
//! bottom, the columns right to left; digits set upright in a column (縦中横) are read inside it.
//!
//! Two layouts, after `pdftotext`: [`TextLayout::Reading`] gives each line its words with single spaces;
//! [`TextLayout::Physical`] places each character in the column of a fixed grid (a half-width character one column,
//! a full-width two), so that the columns of a table stay aligned and a parser can cut the lines at fixed positions.

use crate::scene::{PageTextIndex, estimated_advance_em};

/// How the characters of a line are spaced.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextLayout {
    /// Words with single spaces between them, in reading order.
    #[default]
    Reading,
    /// Each character at the column its position on the page gives (like `pdftotext -layout`).
    Physical,
}

/// One drawn character, in page points (y grows downwards).
#[derive(Clone, Copy, Debug)]
struct Glyph {
    ch: char,
    x0: f32,
    x1: f32,
    top: f32,
    base: f32,
    op: usize,
    order: usize,
}

impl Glyph {
    fn height(&self) -> f32 {
        (self.base - self.top).max(1.0)
    }

    fn width(&self) -> f32 {
        let w = self.x1 - self.x0;
        if w > 0.0 { w } else { self.height() * estimated_advance_em(self.ch) }
    }

    fn center_x(&self) -> f32 {
        self.x0 + self.width() / 2.0
    }

    fn bottom(&self) -> f32 {
        self.top + self.height()
    }
}

/// Columns a character takes in the physical layout: a full-width character two, any other one.
fn columns_of(ch: char) -> usize {
    if estimated_advance_em(ch) >= 1.0 { 2 } else { 1 }
}

/// The text of a page. Lines end with `\n`; an empty page gives an empty string.
pub fn page_text(index: &PageTextIndex, layout: TextLayout) -> String {
    let glyphs: Vec<Glyph> = index
        .chars
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.ch.is_whitespace() && !c.ch.is_control())
        .map(|(order, c)| {
            let r = c.rect_pt;
            Glyph {
                ch: c.ch,
                x0: r.x0.min(r.x1),
                x1: r.x0.max(r.x1),
                top: r.y0.min(r.y1),
                base: r.y0.max(r.y1),
                op: c.stream_op_index,
                order,
            }
        })
        .collect();
    if glyphs.is_empty() {
        return String::new();
    }
    let flagged: Vec<bool> = index
        .chars
        .iter()
        .filter(|c| !c.ch.is_whitespace() && !c.ch.is_control())
        .map(|c| c.vertical)
        .collect();
    let vertical = vertical_runs(&glyphs, &flagged);

    let (mut upright, mut flat): (Vec<Glyph>, Vec<Glyph>) = (Vec::new(), Vec::new());
    for (g, v) in glyphs.iter().zip(&vertical) {
        if *v { upright.push(*g) } else { flat.push(*g) }
    }
    let mut segments = column_segments(upright);
    // Upright digits in a column (縦中横) belong to the column.
    flat.retain(|g| !absorb_into_column(&mut segments, *g));
    for s in &mut segments {
        s.glyphs.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.x0.total_cmp(&b.x0)));
    }

    let vertical_text = vertical_text(&segments, layout);
    let horizontal_text = horizontal_text(flat, layout);
    let vertical_count: usize = segments.iter().map(|s| s.glyphs.len()).sum();
    let horizontal_count = glyphs.len() - vertical_count;
    let (first, second) = if vertical_count > horizontal_count {
        (vertical_text, horizontal_text)
    } else {
        (horizontal_text, vertical_text)
    };
    match (first.is_empty(), second.is_empty()) {
        (_, true) => first,
        (true, false) => second,
        (false, false) => format!("{first}\n{second}"),
    }
}

/// Which characters stand in a vertical run: three or more drawn one after another, each about one character below
/// the last in the same column. Characters spaced out down a column (a name or a title set with gaps, 総　務　省)
/// may stand up to four characters apart where they keep exactly to the column. (A character the reader marked
/// vertical counts too.)
fn vertical_runs(glyphs: &[Glyph], flagged: &[bool]) -> Vec<bool> {
    let n = glyphs.len();
    let mut steps = vec![false; n.saturating_sub(1)];
    for i in 0..n.saturating_sub(1) {
        let (a, b) = (&glyphs[i], &glyphs[i + 1]);
        let h = a.height().max(b.height());
        let dx = (a.center_x() - b.center_x()).abs();
        let dy = b.top - a.top;
        let in_column = dx <= 0.35 * h && dy >= 0.6 * h && dy <= 1.8 * h;
        let spaced_in_column = dx <= 0.1 * h && dy >= 0.6 * h && dy <= 4.0 * h;
        steps[i] = in_column || spaced_in_column;
    }
    let mut out: Vec<bool> = flagged.to_vec();
    out.resize(n, false);
    // The runs as `start..=end` (the characters), with their number of steps.
    let mut runs = Vec::new();
    let mut i = 0;
    while i < steps.len() {
        if !steps[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < steps.len() && steps[i] {
            i += 1;
        }
        runs.push((start, i));
    }
    for &(start, end) in &runs {
        if end - start >= 2 {
            for v in &mut out[start..=end] {
                *v = true;
            }
        }
    }
    // On a page set mostly in columns, two characters one below the other on one column are a column too: a
    // heading of two characters set apart (附　則), the end of a word carried to the next column.
    if out.iter().filter(|v| **v).count() * 2 > n {
        for &(start, end) in &runs {
            if end - start == 1 {
                let (a, b) = (&glyphs[start], &glyphs[end]);
                if (a.center_x() - b.center_x()).abs() <= 0.1 * a.height().max(b.height()) {
                    out[start] = true;
                    out[end] = true;
                }
            }
        }
    }
    out
}

/// The characters of one stretch of a column, top to bottom.
#[derive(Debug)]
struct Segment {
    center_x: f32,
    width: f32,
    top: f32,
    bottom: f32,
    glyphs: Vec<Glyph>,
}

/// Vertical characters gathered into columns, and the columns cut where a long gap divides them (the tiers of a page).
fn column_segments(mut upright: Vec<Glyph>) -> Vec<Segment> {
    upright.sort_by(|a, b| b.center_x().total_cmp(&a.center_x()).then(a.top.total_cmp(&b.top)));
    let mut columns: Vec<(f32, f32, Vec<Glyph>)> = Vec::new();
    for g in upright {
        // A character's size across the column: its height, or for a glyph turned a quarter (a Latin letter set
        // sideways, as tall as its advance) its width. The column is as wide as its widest character.
        let h = g.height().max(g.x1 - g.x0);
        match columns.iter_mut().find(|(cx, ch, _)| (*cx - g.center_x()).abs() <= 0.5 * ch.max(h)) {
            Some((_, ch, glyphs)) => {
                *ch = ch.max(h);
                glyphs.push(g);
            }
            None => columns.push((g.center_x(), h, vec![g])),
        }
    }
    let mut segments = Vec::new();
    for (center_x, h, mut glyphs) in columns {
        glyphs.sort_by(|a, b| a.top.total_cmp(&b.top));
        let mut current: Vec<Glyph> = Vec::new();
        for g in glyphs {
            if let Some(last) = current.last()
                && g.top - last.bottom() > 3.0 * h
            {
                segments.push(segment(center_x, h, std::mem::take(&mut current)));
            }
            current.push(g);
        }
        if !current.is_empty() {
            segments.push(segment(center_x, h, current));
        }
    }
    segments
}

fn segment(center_x: f32, width: f32, glyphs: Vec<Glyph>) -> Segment {
    let top = glyphs.iter().map(|g| g.top).fold(f32::INFINITY, f32::min);
    let bottom = glyphs.iter().map(Glyph::bottom).fold(f32::NEG_INFINITY, f32::max);
    Segment { center_x, width, top, bottom, glyphs }
}

/// Put `g` into the column stretch it stands inside, if any.
fn absorb_into_column(segments: &mut [Segment], g: Glyph) -> bool {
    let (cx, cy) = (g.center_x(), (g.top + g.base) / 2.0);
    for s in segments.iter_mut() {
        if (cx - s.center_x).abs() <= 0.6 * s.width && cy >= s.top - 0.5 * s.width && cy <= s.bottom + 0.5 * s.width {
            s.glyphs.push(g);
            return true;
        }
    }
    false
}

/// The columns as lines: tiers top to bottom, in each tier the columns right to left.
fn vertical_text(segments: &[Segment], layout: TextLayout) -> String {
    if segments.is_empty() {
        return String::new();
    }
    // Tiers: the stretches whose vertical extents overlap, one with another in a chain. Only a band no stretch
    // crosses (the gutter between the tiers of a gazette page) divides two tiers; a column running the length of the
    // page keeps the page one tier.
    let mut order: Vec<usize> = (0..segments.len()).collect();
    order.sort_by(|&a, &b| segments[a].top.total_cmp(&segments[b].top));
    let mut tiers: Vec<(f32, f32, Vec<usize>)> = Vec::new();
    for i in order {
        let s = &segments[i];
        match tiers.last_mut() {
            Some((_, bottom, members)) if s.top < *bottom => {
                *bottom = bottom.max(s.bottom);
                members.push(i);
            }
            _ => tiers.push((s.top, s.bottom, vec![i])),
        }
    }
    // Tiers stand one above the other across the page. Two bands of columns that do not share any width are side
    // by side (the columns of a signature set low at the right, the next article high at the left): one tier.
    let span = |members: &[usize]| {
        members.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), &i| {
            let s = &segments[i];
            (l.min(s.center_x - s.width / 2.0), r.max(s.center_x + s.width / 2.0))
        })
    };
    let mut merged: Vec<(f32, f32, Vec<usize>)> = Vec::new();
    for tier in tiers {
        if let Some(last) = merged.last_mut() {
            let ((l0, r0), (l1, r1)) = (span(&last.2), span(&tier.2));
            if r0 <= l1 || r1 <= l0 {
                last.1 = last.1.max(tier.1);
                last.2.extend(tier.2);
                continue;
            }
        }
        merged.push(tier);
    }
    let mut tiers = merged;
    let mut out = String::new();
    for (t, (tier_top, _, members)) in tiers.iter_mut().enumerate() {
        if t > 0 {
            out.push('\n');
        }
        // Right to left; the stretches of one column top to bottom.
        members.sort_by(|&a, &b| {
            segments[b].center_x.total_cmp(&segments[a].center_x).then(segments[a].top.total_cmp(&segments[b].top))
        });
        // The stretches of one column in a tier are one line, the gap between them kept as the gap inside a stretch
        // (characters the page draws as figures leave a long one).
        let mut k = 0;
        while k < members.len() {
            let first = &segments[members[k]];
            let mut end = k + 1;
            while end < members.len() && segments[members[end]].center_x == first.center_x {
                end += 1;
            }
            let width = members[k..end].iter().map(|&i| segments[i].width).fold(1.0f32, f32::max);
            // Physical: the column starts as far down the tier as it stands (a paragraph's first column one character
            // down, a column whose first characters the page draws as figures by that much).
            if layout == TextLayout::Physical {
                for _ in 0..((first.top - *tier_top) / width).round().max(0.0) as usize {
                    out.push('\u{3000}');
                }
            }
            let mut prev: Option<&Glyph> = None;
            for g in members[k..end].iter().flat_map(|&i| segments[i].glyphs.iter()) {
                if let Some(p) = prev {
                    if same_glyph(p, g) {
                        continue;
                    }
                    let gap = g.top - p.bottom();
                    let h = p.height().max(g.height());
                    match layout {
                        TextLayout::Reading if gap > 0.5 * h => out.push(' '),
                        TextLayout::Physical if gap > 0.5 * h => {
                            for _ in 0..((gap / width).round() as usize).max(1) {
                                out.push('\u{3000}');
                            }
                        }
                        _ => {}
                    }
                }
                out.push(g.ch);
                prev = Some(g);
            }
            k = end;
            out.push('\n');
        }
    }
    out
}

/// A character drawn twice over itself (overprinted for a bold look): the second drawing is not text.
fn same_glyph(a: &Glyph, b: &Glyph) -> bool {
    a.ch == b.ch && a.op != b.op && (a.x0 - b.x0).abs() < 0.2 * a.height() && (a.top - b.top).abs() < 0.2 * a.height()
}

/// The horizontal characters as lines, top to bottom.
fn horizontal_text(mut flat: Vec<Glyph>, layout: TextLayout) -> String {
    if flat.is_empty() {
        return String::new();
    }
    flat.sort_by(|a, b| a.base.total_cmp(&b.base).then(a.order.cmp(&b.order)));
    // Lines: characters whose baselines are within 0.4 of the smaller character's height of the line's.
    let mut lines: Vec<(f32, f32, Vec<Glyph>)> = Vec::new();
    for g in flat {
        let h = g.height();
        match lines.last_mut() {
            Some((base, lh, glyphs)) if (g.base - *base).abs() <= 0.4 * lh.min(h) => {
                *lh = lh.max(h);
                glyphs.push(g);
            }
            _ => lines.push((g.base, h, vec![g])),
        }
    }
    let left = lines
        .iter()
        .flat_map(|(_, _, glyphs)| glyphs.iter().map(|g| g.x0))
        .fold(f32::INFINITY, f32::min);
    let unit = grid_unit(&lines);

    let mut out = String::new();
    let mut prev: Option<(f32, f32)> = None;
    for (base, lh, glyphs) in &mut lines {
        // A blank line where the page leaves one.
        if let Some((pbase, plh)) = prev
            && *base - pbase > 1.8 * plh.max(*lh)
        {
            out.push('\n');
        }
        prev = Some((*base, *lh));
        glyphs.sort_by(|a, b| a.x0.total_cmp(&b.x0).then(a.order.cmp(&b.order)));
        let mut col = 0usize;
        let mut last: Option<Glyph> = None;
        for g in glyphs.iter() {
            if let Some(p) = last {
                if same_glyph(&p, g) {
                    continue;
                }
                let gap = g.x0 - (p.x0 + p.width());
                let spaced = gap > 0.25 * p.height().min(g.height());
                match layout {
                    TextLayout::Reading => {
                        if spaced {
                            out.push(' ');
                        }
                    }
                    // Within a word the characters follow one another (proportional widths would drift off the grid);
                    // after a gap the next one goes to its column, at least one space on.
                    TextLayout::Physical if spaced => {
                        let target = (((g.x0 - left) / unit).round().max(0.0) as usize).max(col + 1);
                        for _ in col..target {
                            out.push(' ');
                        }
                        col = target;
                    }
                    TextLayout::Physical => {}
                }
            } else if layout == TextLayout::Physical {
                let target = ((g.x0 - left) / unit).round().max(0.0) as usize;
                for _ in 0..target {
                    out.push(' ');
                }
                col = target;
            }
            out.push(g.ch);
            col += columns_of(g.ch);
            last = Some(*g);
        }
        out.push('\n');
    }
    out
}

/// The width of one column of the physical layout: half the most common character height of the page.
fn grid_unit(lines: &[(f32, f32, Vec<Glyph>)]) -> f32 {
    let mut heights: Vec<f32> = lines.iter().flat_map(|(_, _, g)| g.iter().map(Glyph::height)).collect();
    heights.sort_by(f32::total_cmp);
    let median = heights.get(heights.len() / 2).copied().unwrap_or(10.0);
    (median / 2.0).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{CharBox, PdfRect};

    fn put(chars: &mut Vec<CharBox>, ch: char, x: f32, top: f32, size: f32, w: f32) {
        let op = chars.len();
        chars.push(CharBox { ch, rect_pt: PdfRect { x0: x, y0: top, x1: x + w, y1: top + size }, vertical: false, stream_op_index: op });
    }

    fn row(chars: &mut Vec<CharBox>, text: &str, x: f32, top: f32, size: f32) {
        let mut x = x;
        for ch in text.chars() {
            let w = size * estimated_advance_em(ch);
            put(chars, ch, x, top, size, w);
            x += w;
        }
    }

    fn column(chars: &mut Vec<CharBox>, text: &str, x: f32, top: f32, size: f32) {
        for (i, ch) in text.chars().enumerate() {
            put(chars, ch, x, top + i as f32 * size, size, size);
        }
    }

    #[test]
    fn lines_are_read_top_to_bottom_and_left_to_right_whatever_the_drawing_order() {
        let mut chars = Vec::new();
        row(&mut chars, "second", 10.0, 34.0, 10.0);
        row(&mut chars, "first", 10.0, 20.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "first\nsecond\n");
    }

    #[test]
    fn a_gap_between_words_is_a_space_and_runs_that_touch_are_not_split() {
        let mut chars = Vec::new();
        row(&mut chars, "標準", 0.0, 0.0, 10.0);
        // A second run continues the word right where the first ended.
        row(&mut chars, "仕様書", 20.0, 0.0, 10.0);
        row(&mut chars, "第1章", 80.0, 0.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "標準仕様書 第1章\n");
    }

    #[test]
    fn the_physical_layout_keeps_table_columns_aligned() {
        let mut chars = Vec::new();
        row(&mut chars, "ID", 0.0, 0.0, 10.0);
        row(&mut chars, "名称", 50.0, 0.0, 10.0);
        row(&mut chars, "0010001", 0.0, 12.0, 10.0);
        row(&mut chars, "住民", 50.0, 12.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Physical);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        // Both cells of the second column start at the same column of the text.
        let at = |line: &str, word: &str| line[..line.find(word).unwrap()].chars().map(columns_of).sum::<usize>();
        assert_eq!(at(lines[0], "名称"), 10);
        assert_eq!(at(lines[1], "住民"), 10);
    }

    #[test]
    fn vertical_columns_are_read_right_to_left_top_to_bottom() {
        let mut chars = Vec::new();
        column(&mut chars, "デジタル庁告示", 100.0, 10.0, 10.0);
        column(&mut chars, "第二十七号", 85.0, 10.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "デジタル庁告示\n第二十七号\n");
    }

    #[test]
    fn upright_digits_in_a_column_are_read_inside_it() {
        let mut chars = Vec::new();
        column(&mut chars, "令和", 100.0, 10.0, 10.0);
        // 縦中横: "８" drawn as a horizontal glyph in the column's cell.
        put(&mut chars, '8', 102.0, 30.0, 10.0, 5.0);
        column(&mut chars, "年七月", 100.0, 40.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "令和8年七月\n");
    }

    #[test]
    fn characters_spaced_out_down_a_column_are_read_as_the_column() {
        // A signature: 内閣総理大臣　高市　早苗, a character's space between the words; beside it 総　務　省 with a
        // gap after each character (the pitch twice the size).
        let mut chars = Vec::new();
        column(&mut chars, "内閣総理大臣", 100.0, 10.0, 10.0);
        column(&mut chars, "高市", 100.0, 80.0, 10.0);
        column(&mut chars, "早苗", 100.0, 110.0, 10.0);
        for (i, ch) in "総務省".chars().enumerate() {
            put(&mut chars, ch, 120.0, 10.0 + i as f32 * 21.0, 10.0, 10.0);
        }
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "総 務 省\n内閣総理大臣 高市 早苗\n");
    }

    #[test]
    fn spaced_characters_off_the_column_are_not_a_column() {
        // Three cells one below another, three lines apart, but not on one x: lines, not a column.
        let mut chars = Vec::new();
        row(&mut chars, "甲", 0.0, 0.0, 10.0);
        row(&mut chars, "乙", 2.0, 30.0, 10.0);
        row(&mut chars, "丙", 4.0, 60.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "甲\n\n乙\n\n丙\n");
    }

    #[test]
    fn a_column_the_length_of_the_page_keeps_the_page_one_tier() {
        // Right to left: a full column; a short column at the top; a short column at the foot (a gap down the
        // middle). The short columns' stretches do not overlap each other, but both overlap the full column.
        let mut chars = Vec::new();
        column(&mut chars, "一二三四五六七八九十", 200.0, 0.0, 10.0);
        column(&mut chars, "上の列", 185.0, 0.0, 10.0);
        column(&mut chars, "下の列", 170.0, 70.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "一二三四五六七八九十\n上の列\n下の列\n");
    }

    #[test]
    fn a_band_no_column_crosses_divides_the_tiers() {
        // Two tiers of two columns each, a gutter between them: the upper tier is read first, right to left.
        let mut chars = Vec::new();
        column(&mut chars, "甲の一", 200.0, 0.0, 10.0);
        column(&mut chars, "甲の二", 185.0, 0.0, 10.0);
        column(&mut chars, "乙の一", 200.0, 80.0, 10.0);
        column(&mut chars, "乙の二", 185.0, 80.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "甲の一\n甲の二\n\n乙の一\n乙の二\n");
    }

    #[test]
    fn columns_set_low_at_the_right_and_high_at_the_left_are_read_right_to_left() {
        // A page of a notice: the signatures low at the right, then the heading 附　則 and the article high at the
        // left. Their extents do not overlap, but they share no width either: one tier.
        let mut chars = Vec::new();
        column(&mut chars, "内閣総理大臣", 300.0, 200.0, 10.0);
        column(&mut chars, "総務大臣", 285.0, 220.0, 10.0);
        put(&mut chars, '附', 250.0, 20.0, 10.0, 10.0);
        put(&mut chars, '則', 250.0, 40.0, 10.0, 10.0);
        column(&mut chars, "この告示は", 235.0, 10.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "内閣総理大臣\n総務大臣\n附 則\nこの告示は\n");
    }

    #[test]
    fn the_physical_layout_starts_each_column_as_far_down_as_it_stands() {
        // A paragraph: its first column one character down, the next at the top; then a column two characters down.
        let mut chars = Vec::new();
        column(&mut chars, "地方公共団体", 200.0, 10.0, 10.0);
        column(&mut chars, "情報システム", 185.0, 0.0, 10.0);
        column(&mut chars, "庁内連携", 170.0, 20.0, 10.0);
        let physical = page_text(&PageTextIndex { chars: chars.clone() }, TextLayout::Physical);
        assert_eq!(physical, "\u{3000}地方公共団体\n情報システム\n\u{3000}\u{3000}庁内連携\n");
        // Reading keeps the text alone.
        assert_eq!(page_text(&PageTextIndex { chars }, TextLayout::Reading), "地方公共団体\n情報システム\n庁内連携\n");
    }

    #[test]
    fn a_column_starting_with_a_sideways_letter_is_as_wide_as_its_characters() {
        // "API連携" down a column two characters below the top of the tier: A, P, I turned a quarter (one em across,
        // half an em down), then 連携. The column beside it starts at the top.
        let mut chars = Vec::new();
        column(&mut chars, "地方公共団体", 215.0, 0.0, 10.0);
        for (i, ch) in "API".chars().enumerate() {
            let op = chars.len();
            let top = 20.0 + i as f32 * 5.0;
            chars.push(CharBox { ch, rect_pt: PdfRect { x0: 200.0, y0: top, x1: 210.0, y1: top + 5.0 }, vertical: true, stream_op_index: op });
        }
        column(&mut chars, "連携", 200.0, 35.0, 10.0);
        let physical = page_text(&PageTextIndex { chars }, TextLayout::Physical);
        assert_eq!(physical, "地方公共団体\n\u{3000}\u{3000}API連携\n");
    }

    #[test]
    fn a_long_gap_in_a_column_keeps_the_column_one_line() {
        // 規定に基 … 庁内 … 連携: the characters between drawn as figures leave gaps of four and three characters.
        let mut chars = Vec::new();
        column(&mut chars, "規定に基", 200.0, 0.0, 10.0);
        column(&mut chars, "庁内", 200.0, 80.0, 10.0);
        column(&mut chars, "連携", 200.0, 130.0, 10.0);
        // The next column runs the length of the page: one tier.
        column(&mut chars, "次の列は頁の下まで続く長い列です", 185.0, 0.0, 10.0);
        let physical = page_text(&PageTextIndex { chars: chars.clone() }, TextLayout::Physical);
        assert_eq!(physical, "規定に基\u{3000}\u{3000}\u{3000}\u{3000}庁内\u{3000}\u{3000}\u{3000}連携\n次の列は頁の下まで続く長い列です\n");
        assert_eq!(page_text(&PageTextIndex { chars }, TextLayout::Reading), "規定に基 庁内 連携\n次の列は頁の下まで続く長い列です\n");
    }

    #[test]
    fn a_page_mostly_vertical_puts_its_columns_before_its_horizontal_lines() {
        let mut chars = Vec::new();
        column(&mut chars, "本文の縦書き", 100.0, 10.0, 10.0);
        row(&mut chars, "1", 50.0, 200.0, 8.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "本文の縦書き\n\n1\n");
    }

    #[test]
    fn a_character_overprinted_for_bold_is_read_once() {
        let mut chars = Vec::new();
        row(&mut chars, "告示", 0.0, 0.0, 10.0);
        row(&mut chars, "告示", 0.3, 0.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "告示\n");
    }

    #[test]
    fn a_short_stack_of_characters_is_not_taken_for_vertical_writing() {
        // Two table cells one below another are lines, not a column.
        let mut chars = Vec::new();
        row(&mut chars, "○", 0.0, 0.0, 10.0);
        row(&mut chars, "○", 0.0, 12.0, 10.0);
        let text = page_text(&PageTextIndex { chars }, TextLayout::Reading);
        assert_eq!(text, "○\n○\n");
    }

    #[test]
    fn spaces_and_an_empty_page() {
        assert_eq!(page_text(&PageTextIndex::default(), TextLayout::Reading), "");
        let mut chars = Vec::new();
        row(&mut chars, "a b", 0.0, 0.0, 10.0);
        // The drawn space is dropped; the gap it leaves makes the space.
        assert_eq!(page_text(&PageTextIndex { chars }, TextLayout::Reading), "a b\n");
    }
}
