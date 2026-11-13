// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibisOffice's own text features PowerPoint does not have: text wrap round pictures and videos (GitHub issue #132)
//! and linked text boxes (#131). They are saved in an extension PowerPoint ignores (`ibis-pptx` `pptx_ibis`).
use crate::{Emu, ShapeId};

/// How text wraps round a picture or a video (図の形式 > テキストの回り込み).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ObjectWrapKind {
    /// The text runs under (behind) it, as in PowerPoint.
    #[default]
    None,
    /// 四角形: the lines go round the rectangle (the rotated picture's bounding box), on its widest side.
    Square,
    /// 上下: no text beside it, the lines pass above and below.
    TopBottom,
}

/// A picture's or video's text wrap: how, and the space kept round it (each side).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ObjectWrap {
    pub kind: ObjectWrapKind,
    /// The space kept on each side: left, top, right, bottom (PowerPoint's 文字列の折り返し distances).
    pub margins: [Emu; 4],
}

impl ObjectWrap {
    /// The default space round the object (0.1 inch, as Word's wrap distance).
    pub const DEFAULT_MARGIN: Emu = 91_440;
    /// No wrap, with the default space on every side (what a picture has before a wrap is chosen).
    pub const fn none() -> Self {
        Self { kind: ObjectWrapKind::None, margins: [Self::DEFAULT_MARGIN; 4] }
    }
    pub fn is_none(&self) -> bool {
        self.kind == ObjectWrapKind::None
    }
}

/// A chain of text boxes (テキスト ボックスの連結): the text flows through `boxes` in order. Each box's own text holds the part
/// that is in it (a plain paragraph list, so that PowerPoint shows all of it). `joined[i]` says that box `i`'s first paragraph
/// is the rest of box `i - 1`'s last (the text was cut in the middle of a paragraph); `joined[0]` is false. `padding[i]` says
/// that no text reached box `i` (its one empty paragraph is only there because a text box has one).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextChain {
    pub boxes: Vec<ShapeId>,
    pub joined: Vec<bool>,
    pub padding: Vec<bool>,
}

impl TextChain {
    pub fn position(&self, shape: ShapeId) -> Option<usize> {
        self.boxes.iter().position(|&b| b == shape)
    }
}

/// A text box's place in a chain, as the file has it: the chain's number on the slide, its order in the chain, whether its first
/// paragraph goes on from the box before and whether no text reached it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainLink {
    pub chain: u32,
    pub order: u32,
    pub joined: bool,
    pub padding: bool,
}

/// What one shape's ibisOffice extension holds (`pptx_ibis`): the slide's maps, shape by shape, for the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IbisShapeExt {
    pub wrap: ObjectWrap,
    /// The text box's text ignores wrap (回り込みを無視する).
    pub ignore_wrap: bool,
    /// Its place in a chain.
    pub chain: Option<ChainLink>,
}

impl IbisShapeExt {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

impl crate::Slide {
    /// The shape's ibisOffice extension, if it has anything in it.
    pub fn ibis_ext(&self, shape: ShapeId) -> Option<IbisShapeExt> {
        let chain = self.text_chains.iter().enumerate().find_map(|(n, c)| c.position(shape).map(|i| ChainLink { chain: n as u32, order: i as u32, joined: c.joined.get(i).copied().unwrap_or(false), padding: c.padding.get(i).copied().unwrap_or(false) }));
        let ext = IbisShapeExt { wrap: self.ibis_wrap.get(&shape).copied().unwrap_or_default(), ignore_wrap: self.ibis_wrap_ignore.contains(&shape), chain };
        (!ext.is_empty()).then_some(ext)
    }

    /// The slide's maps made from the shapes' extensions as a file holds them.
    pub fn apply_ibis_exts(&mut self, exts: &[(ShapeId, IbisShapeExt)]) {
        self.ibis_wrap.clear();
        self.ibis_wrap_ignore.clear();
        self.text_chains.clear();
        let mut chains: std::collections::BTreeMap<u32, Vec<(u32, ShapeId, bool, bool)>> = Default::default();
        for &(shape, ext) in exts {
            if !ext.wrap.is_none() {
                self.ibis_wrap.insert(shape, ext.wrap);
            }
            if ext.ignore_wrap {
                self.ibis_wrap_ignore.insert(shape);
            }
            if let Some(link) = ext.chain {
                chains.entry(link.chain).or_default().push((link.order, shape, link.joined, link.padding));
            }
        }
        for (_, mut members) in chains {
            members.sort_by_key(|m| m.0);
            if members.len() >= 2 {
                self.text_chains.push(TextChain { boxes: members.iter().map(|m| m.1).collect(), joined: members.iter().map(|m| m.2).collect(), padding: members.iter().map(|m| m.3).collect() });
            }
        }
    }

    /// The chain a text box is in.
    pub fn chain_of(&self, shape: ShapeId) -> Option<&TextChain> {
        self.text_chains.iter().find(|c| c.position(shape).is_some())
    }

    /// The shapes whose text wraps round others: the rectangles (EMU, a rotated shape's bounding box, grown by the margin) of
    /// the pictures and videos with a wrap, in the slide's order, each with how it wraps.
    pub fn wrap_obstacles(&self) -> Vec<(ObjectWrapKind, crate::EmuRect)> {
        self.shapes
            .iter()
            .filter(|s| matches!(s.kind, crate::ShapeKind::Picture(_)) && !self.hidden_shapes.contains(&s.id))
            .filter_map(|s| {
                let w = self.ibis_wrap.get(&s.id).filter(|w| !w.is_none())?;
                let r = rotated_bounds(s.frame, s.rotation);
                let [l, t, rt, b] = w.margins;
                Some((w.kind, crate::EmuRect { x: r.x - l, y: r.y - t, width: r.width + l + rt, height: r.height + t + b }))
            })
            .collect()
    }
}

/// The bounding rectangle of `frame` turned `rotation` (60 000ths of a degree, clockwise) about its centre.
pub fn rotated_bounds(frame: crate::EmuRect, rotation: i32) -> crate::EmuRect {
    let turn = (rotation as f64 / 60_000.0).rem_euclid(360.0).to_radians();
    let (sin, cos) = (turn.sin().abs(), turn.cos().abs());
    let (w, h) = (frame.width as f64, frame.height as f64);
    let (bw, bh) = (w * cos + h * sin, w * sin + h * cos);
    let (cx, cy) = (frame.x as f64 + w / 2.0, frame.y as f64 + h / 2.0);
    crate::EmuRect { x: (cx - bw / 2.0).round() as Emu, y: (cy - bh / 2.0).round() as Emu, width: bw.round() as Emu, height: bh.round() as Emu }
}

/// What one paragraph is when set in a text box (`flow_text` asks for it): the space above and below it and its lines
/// (each: its first and last character in the paragraph, and its height), all in one unit (pixels).
#[derive(Clone, Debug, PartialEq)]
pub struct ParagraphMeasure {
    pub before: f32,
    pub after: f32,
    pub lines: Vec<(usize, usize, f32)>,
}

/// The text of a chain divided among its boxes.
#[derive(Clone, Debug, PartialEq)]
pub struct Flowed {
    /// The paragraphs each box holds (a box no text reached holds one empty paragraph, `padding`).
    pub portions: Vec<Vec<crate::text::Paragraph>>,
    pub joined: Vec<bool>,
    pub padding: Vec<bool>,
}

/// The paragraph that goes on in the next box after a line break in the middle of a paragraph: its format, but no bullet, no
/// space above and no first-line indent (they belong to where the paragraph began).
fn continuation(mut tail: crate::text::Paragraph) -> crate::text::Paragraph {
    tail.bullet = None;
    tail.space_before = 0;
    tail.indent = 0;
    tail
}

/// Divides `paragraphs` among boxes `capacities` high (the room for text, in the unit `measure` answers in): each box takes whole
/// lines, a paragraph may be cut between two of its lines, the rest going on at the top of the next box. `measure(paragraph,
/// box)` is the paragraph's lines in the box's width. The last box takes all that is left, past its end. A box never takes less
/// than one line (a box too low for one still has the line it starts with; else the text could never move on).
pub fn flow_text(paragraphs: Vec<crate::text::Paragraph>, capacities: &[f32], measure: &mut dyn FnMut(&crate::text::Paragraph, usize) -> ParagraphMeasure) -> Flowed {
    let mut queue: std::collections::VecDeque<crate::text::Paragraph> = paragraphs.into();
    let count = capacities.len();
    let mut out = Flowed { portions: Vec::new(), joined: Vec::new(), padding: Vec::new() };
    let mut continues = false;
    for (b, &capacity) in capacities.iter().enumerate() {
        let mut portion: Vec<crate::text::Paragraph> = Vec::new();
        let mut used = 0.0f32;
        let mut next_continues = false;
        while let Some(p) = queue.pop_front() {
            if b + 1 == count {
                portion.push(p);
                continue;
            }
            let m = measure(&p, b);
            let before = if portion.is_empty() { 0.0 } else { m.before };
            let room = capacity - used - before;
            let mut fit = 0;
            let mut height = 0.0f32;
            for line in &m.lines {
                if height + line.2 <= room + 0.5 {
                    height += line.2;
                    fit += 1;
                } else {
                    break;
                }
            }
            if fit == 0 && portion.is_empty() {
                fit = 1.min(m.lines.len());
                height = m.lines.first().map_or(0.0, |l| l.2);
            }
            if fit == m.lines.len() {
                used += before + height + m.after;
                portion.push(p);
            } else if fit == 0 {
                queue.push_front(p);
                break;
            } else {
                let mut head = p;
                let tail = head.split_off(m.lines[fit].0);
                portion.push(head);
                queue.push_front(continuation(tail));
                next_continues = true;
                break;
            }
        }
        let padding = portion.is_empty();
        if padding {
            portion.push(crate::text::Paragraph::default());
        }
        out.portions.push(portion);
        out.joined.push(continues);
        out.padding.push(padding);
        continues = next_continues;
    }
    out
}

/// The whole text of a chain, from what its boxes hold (`TextChain::joined`, `padding`): a paragraph cut between two boxes is
/// one again. `bodies` is each box's paragraphs.
pub fn collect_flow(chain: &TextChain, bodies: &[Vec<crate::text::Paragraph>]) -> Vec<crate::text::Paragraph> {
    let mut out: Vec<crate::text::Paragraph> = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        if chain.padding.get(i).copied().unwrap_or(false) {
            continue;
        }
        for (k, p) in body.iter().enumerate() {
            match out.last_mut() {
                Some(last) if k == 0 && chain.joined.get(i).copied().unwrap_or(false) => last.runs.extend(p.runs.iter().cloned()),
                _ => out.push(p.clone()),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{Paragraph, Run, RunStyle};

    fn paragraph(text: &str) -> Paragraph {
        Paragraph { runs: vec![Run { text: text.to_string(), style: RunStyle::default() }], ..Paragraph::default() }
    }

    /// Ten characters to a line, a line 1 high, 1 above a paragraph and none below.
    fn measure(p: &Paragraph, _: usize) -> ParagraphMeasure {
        let n = p.text().chars().count();
        let mut lines = Vec::new();
        let mut at = 0;
        while at < n || lines.is_empty() {
            let end = (at + 10).min(n);
            lines.push((at, end, 1.0));
            at = end;
            if end == n {
                break;
            }
        }
        ParagraphMeasure { before: 1.0, after: 0.0, lines }
    }

    fn text_of(portion: &[Paragraph]) -> Vec<String> {
        portion.iter().map(|p| p.text()).collect()
    }

    #[test]
    fn text_flows_from_box_to_box_by_whole_lines_cutting_a_paragraph() {
        let paragraphs = vec![paragraph("aaaaaaaaaabbbbbbbbbbcccccccccc"), paragraph("dddd")];
        let flowed = flow_text(paragraphs.clone(), &[2.0, 2.0, 5.0], &mut measure);
        assert_eq!(text_of(&flowed.portions[0]), vec!["aaaaaaaaaabbbbbbbbbb"]);
        // (The second box has room for the rest of the paragraph and no more: 1 line, then 1 above and 1 for the next.)
        assert_eq!(text_of(&flowed.portions[1]), vec!["cccccccccc"]);
        assert_eq!(text_of(&flowed.portions[2]), vec!["dddd"]);
        assert_eq!(flowed.joined, vec![false, true, false]);
        assert_eq!(flowed.padding, vec![false, false, false]);
        // A box no text reaches is padding.
        let more = flow_text(paragraphs.clone(), &[2.0, 2.0, 5.0, 5.0], &mut measure);
        assert_eq!(more.padding, vec![false, false, false, true]);
        assert_eq!(more.portions[3].len(), 1);
        assert!(more.portions[3][0].text().is_empty());
        // What the boxes hold is the text again.
        let chain = TextChain { boxes: vec![ShapeId(1), ShapeId(2), ShapeId(3), ShapeId(4)], joined: more.joined.clone(), padding: more.padding.clone() };
        let again = collect_flow(&chain, &more.portions);
        assert_eq!(text_of(&again), text_of(&paragraphs));
    }

    #[test]
    fn a_paragraph_that_does_not_fit_goes_whole_to_the_next_box_and_the_last_box_takes_the_rest() {
        let paragraphs = vec![paragraph("aaaa"), paragraph("bbbbbbbbbbbbbbbbbbbb"), paragraph("cc")];
        // Box 1 has room for the first paragraph only (1 + 1 high of 3: the second wants 1 + 2 more).
        let flowed = flow_text(paragraphs.clone(), &[3.0, 100.0], &mut measure);
        assert_eq!(text_of(&flowed.portions[0]), vec!["aaaa", "bbbbbbbbbb"]);
        assert_eq!(text_of(&flowed.portions[1]), vec!["bbbbbbbbbb", "cc"]);
        assert_eq!(flowed.joined, vec![false, true]);
        // A box too low for one line still takes its first line, and the text moves on.
        let flowed = flow_text(paragraphs, &[0.1, 100.0], &mut measure);
        assert_eq!(text_of(&flowed.portions[0]), vec!["aaaa"]);
        assert_eq!(text_of(&flowed.portions[1]), vec!["bbbbbbbbbbbbbbbbbbbb", "cc"]);
    }

    #[test]
    fn a_continuation_has_no_bullet_no_space_before_and_no_indent() {
        let mut first = paragraph(&"x".repeat(25));
        first.bullet = Some(crate::text::Bullet { text: "•".into(), picture: None, size: 100_000, color: None, auto: None, symbol: None });
        first.space_before = 600;
        first.indent = -300;
        let flowed = flow_text(vec![first.clone()], &[2.0, 10.0], &mut measure);
        assert_eq!(flowed.portions[0][0].bullet, first.bullet);
        let rest = &flowed.portions[1][0];
        assert!(rest.bullet.is_none() && rest.space_before == 0 && rest.indent == 0);
        assert_eq!(rest.text().len(), 5);
    }
}
