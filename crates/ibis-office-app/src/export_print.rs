// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 印刷の設定 (PPTX-RECORD-016〜018): the colour of the printed slides
//! (カラー・グレースケール・白黒), a frame round each slide (スライドに枠を
//! 付けて印刷), and handouts (配布資料: slides on a portrait page, 1, 2, 3, 4,
//! 6 or 9 to a page), made of the slides' pictures.

use image::{Rgba, RgbaImage};

pub(crate) use crate::export_text::TextLayer;
use crate::export_text::{Align, NoteFlow, draw_text};

/// How the slides are printed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrintColor {
    #[default]
    Color,
    /// Shades of grey.
    Gray,
    /// Black and white only.
    BlackWhite,
}

/// The handout page, in points (A4, portrait), and its margin.
pub const HANDOUT_PAGE: (f32, f32) = (595.0, 842.0);

/// What a handout carries besides the slides (ヘッダーとフッター > ノートと配布資料).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HandoutMarks {
    pub header: Option<String>,
    pub date: Option<String>,
    pub footer: Option<String>,
    /// The number of the page, bottom right.
    pub number: bool,
    /// Where they go and how big: the file's handout master (the corners, 10 pt, if `None`).
    pub layout: Option<ibis_pptx::handout_master::HandoutLayout>,
}
/// The notes of a slide as a printed page carries them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteText {
    pub paragraphs: Vec<NotePara>,
}

/// A paragraph of notes: its level (an indent), its bullet and its runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NotePara {
    pub level: u8,
    pub bullet: Option<String>,
    pub runs: Vec<NoteRun>,
}

/// A run of notes: bold, its size in points (12 if `None`) and colour.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteRun {
    pub text: String,
    pub bold: bool,
    pub size_pt: Option<f32>,
    pub color: Option<(u8, u8, u8)>,
}

impl NoteText {
    /// Plain notes: a paragraph for each line.
    pub fn plain(text: &str) -> Self {
        NoteText { paragraphs: text.split('\n').map(|line| NotePara { level: 0, bullet: None, runs: vec![NoteRun { text: line.to_string(), ..Default::default() }] }).collect() }
    }
}

const MARGIN: f32 = 36.0;
/// Pixels per point of a handout page's picture.
pub(crate) const PX_PER_PT: f32 = 2.0;

/// `image` in the colour `mode`.
pub fn recolor(image: &mut RgbaImage, mode: PrintColor) {
    if mode == PrintColor::Color {
        return;
    }
    for pixel in image.pixels_mut() {
        // Rec. 601 luma, the weights a grey print is made with.
        let luma = (0.299 * pixel[0] as f32 + 0.587 * pixel[1] as f32 + 0.114 * pixel[2] as f32).round() as u8;
        let value = if mode == PrintColor::BlackWhite { if luma >= 128 { 255 } else { 0 } } else { luma };
        *pixel = Rgba([value, value, value, pixel[3]]);
    }
}

/// A one-pixel black line round `image`.
pub fn frame(image: &mut RgbaImage) {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    for x in 0..w {
        image.put_pixel(x, 0, Rgba([0, 0, 0, 255]));
        image.put_pixel(x, h - 1, Rgba([0, 0, 0, 255]));
    }
    for y in 0..h {
        image.put_pixel(0, y, Rgba([0, 0, 0, 255]));
        image.put_pixel(w - 1, y, Rgba([0, 0, 0, 255]));
    }
}

/// The columns and rows of a handout page with `per_page` slides (turned on its side: a row for a column).
pub fn handout_grid(per_page: u8, landscape: bool) -> (usize, usize) {
    let (columns, rows) = match per_page {
        1 => (1, 1),
        2 => (1, 2),
        3 => (1, 3),
        4 => (2, 2),
        6 => (2, 3),
        8 => (2, 4),
        16 => (4, 4),
        _ => (3, 3),
    };
    if landscape { (rows, columns) } else { (columns, rows) }
}

/// The slides as handout pages: `per_page` to a page, in rows (a column for
/// 2 and 3), each as large as its place takes at its own proportions, with a
/// frame if `framed`. Each page with its size in points.
pub fn handouts(slides: &[RgbaImage], per_page: u8, framed: bool, landscape: bool, marks: &HandoutMarks, layer: &mut dyn TextLayer) -> Vec<(RgbaImage, (f32, f32))> {
    let (columns, rows) = handout_grid(per_page, landscape);
    let per_page = columns * rows;
    let (page_w, page_h) = if landscape { (HANDOUT_PAGE.1, HANDOUT_PAGE.0) } else { HANDOUT_PAGE };
    let cell_w = (page_w - 2.0 * MARGIN) / columns as f32;
    let cell_h = (page_h - 2.0 * MARGIN) / rows as f32;
    // Room between slides.
    let gap = if per_page > 1 { 12.0 } else { 0.0 };
    slides
        .chunks(per_page)
        .enumerate()
        .map(|(page_number, group)| {
            let mut page = RgbaImage::from_pixel((page_w * PX_PER_PT) as u32, (page_h * PX_PER_PT) as u32, Rgba([255, 255, 255, 255]));
            for (i, slide) in group.iter().enumerate() {
                let (column, row) = (i % columns, i / columns);
                let (avail_w, avail_h) = (cell_w - gap, cell_h - gap);
                let scale = (avail_w / slide.width().max(1) as f32 * PX_PER_PT).min(avail_h / slide.height().max(1) as f32 * PX_PER_PT);
                let (w, h) = (((slide.width() as f32 * scale).round() as u32).max(1), ((slide.height() as f32 * scale).round() as u32).max(1));
                let mut picture = image::imageops::resize(slide, w, h, image::imageops::FilterType::Triangle);
                if framed {
                    frame(&mut picture);
                }
                // Centred in its cell.
                let x = (MARGIN + column as f32 * cell_w) * PX_PER_PT + (cell_w * PX_PER_PT - w as f32) / 2.0;
                let y = (MARGIN + row as f32 * cell_h) * PX_PER_PT + (cell_h * PX_PER_PT - h as f32) / 2.0;
                image::imageops::overlay(&mut page, &picture, x.round() as i64, y.round() as i64);
            }
            draw_marks(layer, &mut page, marks, page_number + 1, (page_w, page_h));
            (page, (page_w, page_h))
        })
        .collect()
}

/// ノート ページ: each slide's picture on top of an A4 portrait page (with a thin
/// frame, as PowerPoint draws it), its notes below, and the header, date,
/// footer and page number the notes carry. Notes that do not fit go on further pages
/// (the text only, from the top). Each page with its size in points.
pub fn notes_pages(slides: &[RgbaImage], notes: &[NoteText], marks: &HandoutMarks, layer: &mut dyn TextLayer) -> Vec<(RgbaImage, (f32, f32))> {
    let (page_w, page_h) = HANDOUT_PAGE;
    // The slide as wide as the page's text allows, from the top; the notes under it.
    let side = 72.0;
    let top = 60.0;
    let blank = || RgbaImage::from_pixel((page_w * PX_PER_PT) as u32, (page_h * PX_PER_PT) as u32, Rgba([255, 255, 255, 255]));
    let mut pages = Vec::new();
    for (i, slide) in slides.iter().enumerate() {
        let mut page = blank();
        let avail_w = page_w - 2.0 * side;
        let scale = avail_w / slide.width().max(1) as f32 * PX_PER_PT;
        let (w, h) = (((slide.width() as f32 * scale).round() as u32).max(1), ((slide.height() as f32 * scale).round() as u32).max(1));
        let mut picture = image::imageops::resize(slide, w, h, image::imageops::FilterType::Triangle);
        frame(&mut picture);
        image::imageops::overlay(&mut page, &picture, (side * PX_PER_PT) as i64, (top * PX_PER_PT) as i64);
        let text_top = top * PX_PER_PT + h as f32 + 28.0 * PX_PER_PT;
        let mut flow = start_notes(notes.get(i), avail_w * PX_PER_PT);
        draw_notes(layer, &mut flow, &mut page, (side * PX_PER_PT, text_top, avail_w * PX_PER_PT, (page_h - 56.0) * PX_PER_PT - text_top));
        pages.push(page);
        // What did not fit: pages of text, from the top.
        while notes_left(&flow) {
            let mut more = blank();
            draw_notes(layer, &mut flow, &mut more, (side * PX_PER_PT, top * PX_PER_PT, avail_w * PX_PER_PT, (page_h - 56.0 - top) * PX_PER_PT));
            pages.push(more);
        }
    }
    pages
        .into_iter()
        .enumerate()
        .map(|(n, mut page)| {
            draw_marks(layer, &mut page, marks, n + 1, (page_w, page_h));
            (page, (page_w, page_h))
        })
        .collect()
}

/// アウトライン: the slides' titles (with their numbers) and body paragraphs as text, the levels indented, over pages of an A4
/// portrait sheet; with the header, date, footer and page number the notes carry. Each page with its size in points.
pub fn outline_pages(text: &NoteText, marks: &HandoutMarks, layer: &mut dyn TextLayer) -> Vec<(RgbaImage, (f32, f32))> {
    let (page_w, page_h) = HANDOUT_PAGE;
    let (side, top) = (72.0, 60.0);
    let blank = || RgbaImage::from_pixel((page_w * PX_PER_PT) as u32, (page_h * PX_PER_PT) as u32, Rgba([255, 255, 255, 255]));
    let mut flow = start_notes(Some(text), (page_w - 2.0 * side) * PX_PER_PT);
    let mut pages = Vec::new();
    loop {
        let mut page = blank();
        draw_notes(layer, &mut flow, &mut page, (side * PX_PER_PT, top * PX_PER_PT, (page_w - 2.0 * side) * PX_PER_PT, (page_h - 56.0 - top) * PX_PER_PT));
        pages.push(page);
        if !notes_left(&flow) {
            break;
        }
    }
    pages
        .into_iter()
        .enumerate()
        .map(|(n, mut page)| {
            draw_marks(layer, &mut page, marks, n + 1, (page_w, page_h));
            (page, (page_w, page_h))
        })
        .collect()
}

type NotesFlow = Option<NoteFlow>;

/// The notes' text laid out for boxes `width` pixels wide.
fn start_notes(text: Option<&NoteText>, width: f32) -> NotesFlow {
    text.and_then(|t| NoteFlow::new(t, width.round() as u32))
}

fn draw_notes(layer: &mut dyn TextLayer, flow: &mut NotesFlow, page: &mut RgbaImage, rect: (f32, f32, f32, f32)) {
    if let Some(flow) = flow {
        flow.draw_next(layer, page, rect);
    }
}

fn notes_left(flow: &NotesFlow) -> bool {
    flow.as_ref().is_some_and(|f| f.has_more())
}

/// The header, date, footer and page number of a handout page: where the
/// handout master puts them, or in the margins' corners.
fn draw_marks(layer: &mut dyn TextLayer, page: &mut RgbaImage, marks: &HandoutMarks, number: usize, (page_w, page_h): (f32, f32)) {
    use ibis_pptx::handout_master::Mark;
    if let Some(layout) = &marks.layout {
        // The master's boxes, scaled from the notes page to this one; the text inside as its box says.
        let (kx, ky) = (page_w * PX_PER_PT / layout.page.0.max(1) as f32, page_h * PX_PER_PT / layout.page.1.max(1) as f32);
        let inset_x = 7.2 * PX_PER_PT;
        let inset_y = 3.6 * PX_PER_PT;
        let number_text = number.to_string();
        for place in &layout.places {
            let text = match place.mark {
                Mark::Header => marks.header.as_deref(),
                Mark::Date => marks.date.as_deref(),
                Mark::Footer => marks.footer.as_deref(),
                Mark::Number => marks.number.then_some(number_text.as_str()),
            };
            let Some(text) = text else { continue };
            let (x, y, w, h) = (place.frame.0 as f32 * kx, place.frame.1 as f32 * ky, place.frame.2 as f32 * kx, place.frame.3 as f32 * ky);
            let size = place.size_pt * PX_PER_PT;
            let (align, at) = match place.align.as_str() {
                "r" => (Align::Right, x + w - inset_x),
                "ctr" => (Align::Center, x + w / 2.0),
                _ => (Align::Left, x + inset_x),
            };
            let top = if place.bottom { y + h - inset_y - size * 1.2 } else { y + inset_y };
            draw_text(layer, page, PX_PER_PT, text, at, top, size, align);
        }
        return;
    }
    let size = 10.0 * PX_PER_PT;
    let (left, right) = (MARGIN * PX_PER_PT, (page_w - MARGIN) * PX_PER_PT);
    let (top, bottom) = (14.0 * PX_PER_PT, (page_h - 14.0 - 12.0) * PX_PER_PT);
    if let Some(text) = &marks.header {
        draw_text(layer, page, PX_PER_PT, text, left, top, size, Align::Left);
    }
    if let Some(text) = &marks.date {
        draw_text(layer, page, PX_PER_PT, text, right, top, size, Align::Right);
    }
    if let Some(text) = &marks.footer {
        draw_text(layer, page, PX_PER_PT, text, left, bottom, size, Align::Left);
    }
    if marks.number {
        draw_text(layer, page, PX_PER_PT, &number.to_string(), right, bottom, size, Align::Right);
    }
}

#[cfg(any(all(test, windows), all(any(target_os = "macos", target_os = "linux"), feature = "scenarios")))]
#[path = "tests/pptx/export_print_tests.rs"]
pub(crate) mod export_print_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba([rgb[0], rgb[1], rgb[2], 255]))
    }

    #[test]
    fn colours() {
        let mut gray = solid(2, 2, [255, 0, 0]);
        recolor(&mut gray, PrintColor::Gray);
        assert_eq!(gray.get_pixel(0, 0), &Rgba([76, 76, 76, 255]));
        let mut bw = solid(2, 2, [255, 0, 0]);
        recolor(&mut bw, PrintColor::BlackWhite);
        assert_eq!(bw.get_pixel(0, 0), &Rgba([0, 0, 0, 255]));
        let mut light = solid(2, 2, [250, 250, 200]);
        recolor(&mut light, PrintColor::BlackWhite);
        assert_eq!(light.get_pixel(1, 1), &Rgba([255, 255, 255, 255]));
        let mut same = solid(2, 2, [10, 20, 30]);
        recolor(&mut same, PrintColor::Color);
        assert_eq!(same.get_pixel(0, 0), &Rgba([10, 20, 30, 255]));
    }
}
