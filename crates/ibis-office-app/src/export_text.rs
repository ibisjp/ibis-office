// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Text drawn on a printed page's picture (the notes, the outline, and the header, date, footer and number of a handout).
//!
//! It is laid out and drawn by the text layout the slides' own text is (`TextLayer`, made over the headless window the export
//! draws in), so it is the same on every platform and in the same fonts, and not by a platform's own text API.

use ibis_doc_model::{Bullet, Paragraph, Rgb, Run, RunStyle, TextAlign, TextBody, TextStyle};
use image::{Rgba, RgbaImage};

use crate::export_print::NoteText;

/// What lays text out and draws it: the slides' own text drawing, in a box.
pub(crate) trait TextLayer {
    /// `body` laid out in a box `width` pixels wide (a page picture's pixels) and at most `height` tall: the picture of the box (the
    /// text on white) and the top and bottom of each of its lines in that picture's pixels.
    fn typeset(&mut self, body: &TextBody, width: u32, height: u32) -> Option<(RgbaImage, Vec<(f32, f32)>)>;
}

/// Where a line sits against the `x` given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Center,
    Right,
}

/// The tallest picture of text a layer is asked for (the renderer's textures reach 16 384 pixels).
const MAX_HEIGHT: u32 = 15_000;
/// The most paragraphs laid out as one picture (an outline of many slides comes in pieces).
const CHUNK: usize = 60;
/// A level's indent and a bullet's hang, in points.
const LEVEL_PT: i64 = 18;

/// `text` as paragraphs of the slides' text model: a level's indent, the bullet hanging, the runs' own size, weight and colour.
pub(crate) fn note_paragraphs(text: &NoteText) -> Vec<Paragraph> {
    const BASE_PT: f32 = 12.0;
    text.paragraphs
        .iter()
        .map(|p| {
            let size = |pt: Option<f32>| ((pt.unwrap_or(BASE_PT) * 100.0).round() as u32).clamp(TextStyle::MIN_FONT_SIZE, TextStyle::MAX_FONT_SIZE);
            let runs = p
                .runs
                .iter()
                .map(|r| Run {
                    text: r.text.clone(),
                    style: RunStyle { font_size: Some(size(r.size_pt)), bold: r.bold.then_some(true), color: r.color.map(|(r, g, b)| Rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32)), ..RunStyle::default() },
                })
                .collect();
            let hang = LEVEL_PT * 12_700;
            let level = p.level as i64 * hang;
            let bullet = p.bullet.as_ref().map(|b| Bullet { text: b.clone(), picture: None, size: 100_000, color: None, auto: None, symbol: None });
            Paragraph {
                runs,
                margin_left: if bullet.is_some() { level + hang } else { level },
                indent: if bullet.is_some() { -hang } else { 0 },
                bullet,
                end_style: RunStyle { font_size: Some(size(p.runs.first().and_then(|r| r.size_pt))), ..RunStyle::default() },
                ..Paragraph::default()
            }
        })
        .collect()
}

/// `paragraphs` as a text in a box (no insets, wrapped, from the top).
fn body_of(paragraphs: Vec<Paragraph>, align: TextAlign) -> TextBody {
    let style = TextStyle { font_size: 1200, align, ..TextStyle::default() };
    TextBody { paragraphs, style, insets: ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 }, ..TextBody::default() }
}

/// `layer`'s picture put on `page` with its rows `from..to` at (`x`, `y`): the text is black or coloured on white, so each pixel
/// multiplies what is on the page (white leaves it, text darkens it).
fn multiply(page: &mut RgbaImage, layer: &RgbaImage, (from, to): (u32, u32), (x, y): (i64, i64)) {
    for row in from..to.min(layer.height()) {
        for column in 0..layer.width() {
            let (px, py) = (x + column as i64, y + (row - from) as i64);
            if px < 0 || py < 0 || px >= page.width() as i64 || py >= page.height() as i64 {
                continue;
            }
            let over = layer.get_pixel(column, row).0;
            if over[..3] == [255, 255, 255] {
                continue;
            }
            let under = page.get_pixel(px as u32, py as u32).0;
            let mix = |u: u8, o: u8| ((u as u32 * o as u32 + 127) / 255) as u8;
            page.put_pixel(px as u32, py as u32, Rgba([mix(under[0], over[0]), mix(under[1], over[1]), mix(under[2], over[2]), 255]));
        }
    }
}

/// `text`, `size` pixels high (`px_per_pt` to a point), in black on `page`: its left (or right, or middle) at `x`, the top of its
/// line at `top`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_text(layer: &mut dyn TextLayer, page: &mut RgbaImage, px_per_pt: f32, text: &str, x: f32, top: f32, size: f32, align: Align) {
    if text.is_empty() {
        return;
    }
    let width = page.width() as f32;
    let (left, box_width, text_align) = match align {
        Align::Left => (x, width - x, TextAlign::Left),
        Align::Right => (0.0, x, TextAlign::Right),
        Align::Center => {
            let half = x.min(width - x);
            (x - half, 2.0 * half, TextAlign::Center)
        }
    };
    if box_width < 8.0 {
        return;
    }
    let pt = size / px_per_pt;
    let run = Run { text: text.to_string(), style: RunStyle { font_size: Some((pt * 100.0).round() as u32), ..RunStyle::default() } };
    let body = body_of(vec![Paragraph { runs: vec![run], ..Paragraph::default() }], text_align);
    let Some((picture, _)) = layer.typeset(&body, box_width.round() as u32, (size * 2.0).ceil() as u32) else { return };
    multiply(page, &picture, (0, picture.height()), (left.round() as i64, top.round() as i64));
}

/// A height (pixels) the `paragraphs` surely fit in at `width`: every character as wide as its size, a line and a half to a line.
fn room_for(paragraphs: &[Paragraph], width: u32) -> u32 {
    let px_per_pt = crate::export_print::PX_PER_PT;
    let total: f32 = paragraphs
        .iter()
        .map(|p| {
            let size = p.runs.iter().filter_map(|r| r.style.font_size).max().or(p.end_style.font_size).unwrap_or(1200) as f32 / 100.0 * px_per_pt;
            let chars: usize = p.runs.iter().map(|r| r.text.chars().count()).sum::<usize>() + 4;
            let lines = (chars as f32 * size / width.max(1) as f32).ceil() + 1.0;
            lines * size * 1.5
        })
        .sum();
    (total.ceil() as u32 + 16).min(MAX_HEIGHT)
}

/// The notes of one slide (or the outline) laid out as one text, and put into boxes (pages) as many lines as each takes.
pub(crate) struct NoteFlow {
    /// The paragraphs not laid out yet, in pieces.
    pieces: std::collections::VecDeque<Vec<Paragraph>>,
    width: u32,
    /// The piece being drawn: its picture and its lines, and the line to go on from.
    current: Option<(RgbaImage, Vec<(f32, f32)>)>,
    at: usize,
}

impl NoteFlow {
    /// `None` for notes with no text. The boxes are `width` pixels wide.
    pub(crate) fn new(text: &NoteText, width: u32) -> Option<Self> {
        if text.paragraphs.iter().all(|p| p.runs.iter().all(|r| r.text.trim().is_empty())) {
            return None;
        }
        let paragraphs = note_paragraphs(text);
        let pieces = paragraphs.chunks(CHUNK).map(<[Paragraph]>::to_vec).collect();
        Some(Self { pieces, width, current: None, at: 0 })
    }

    /// Whether there is text not drawn yet.
    pub(crate) fn has_more(&self) -> bool {
        self.current.is_some() || !self.pieces.is_empty()
    }

    /// The text from where the last box ended, in the box `(x, y, width, height)` of `page`, as many whole lines as fit (the next
    /// box takes the rest).
    pub(crate) fn draw_next(&mut self, layer: &mut dyn TextLayer, page: &mut RgbaImage, (x, y, _, height): (f32, f32, f32, f32)) {
        while self.current.is_none() {
            let Some(paragraphs) = self.pieces.pop_front() else { return };
            let height = room_for(&paragraphs, self.width);
            self.current = layer.typeset(&body_of(paragraphs, TextAlign::Left), self.width, height).filter(|(_, lines)| !lines.is_empty());
            self.at = 0;
        }
        let Some((picture, lines)) = &self.current else { return };
        let start = lines[self.at].0;
        // Always at least a line: a box too small for one takes it, so the text ends.
        let take = lines[self.at..].iter().take_while(|l| l.1 - start <= height).count().max(1);
        let end = lines[self.at + take - 1].1;
        multiply(page, picture, (start.floor().max(0.0) as u32, end.ceil() as u32), (x.round() as i64, y.round() as i64));
        self.at += take;
        if self.at >= lines.len() {
            self.current = None;
        }
    }
}
