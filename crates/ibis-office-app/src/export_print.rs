// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 印刷の設定 (PPTX-RECORD-016〜018): the colour of the printed slides
//! (カラー・グレースケール・白黒), a frame round each slide (スライドに枠を
//! 付けて印刷), and handouts (配布資料: slides on a portrait page, 1, 2, 3, 4,
//! 6 or 9 to a page), made of the slides' pictures.

use image::{Rgba, RgbaImage};

pub(crate) use crate::export_text::TextLayer;
use crate::export_text::{Align, Canvas, NoteFlow, Picture, draw_text, draw_text_on_baseline, multiply};

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

/// The page of handouts, notes and the outline when the file does not say (its `notesSz` is missing): 7.5 x 10 inches, in points.
pub const HANDOUT_PAGE: (f32, f32) = (540.0, 720.0);

/// The page's size in points: the file's notes page size (`notesSz`).
fn page_of(marks: &HandoutMarks) -> (f32, f32) {
    marks.layout.as_ref().map_or(HANDOUT_PAGE, |l| (l.page.0 as f32 / 12700.0, l.page.1 as f32 / 12700.0))
}

/// The height of the box a slide goes in at a place of the 1, 2, 3 and 6 to a page handouts, as a fraction of the page (1 to a page: 576 pt of 720,
/// 2: 262.8, 3 and 6: 158.4; Microsoft 365's PDFs of 130 slide sizes, 16:9, 4:3, 16:10, A4 and portrait, #225); the 4 and 9 to a page and anything
/// else go in the 4:3 box of the place's width.
fn place_box_height(per_page: usize) -> Option<f32> {
    match per_page {
        1 => Some(0.8),
        2 => Some(0.365),
        3 | 6 => Some(0.22),
        _ => None,
    }
}

/// Whether a place takes the snap of [`snap_scale`] (1, 2 and 3 to a page: the rule held for 130 sizes; 6 to a page: a 4:3 slide on A3 is 360 x 270 pt, half its
/// size, while the same slide on the other papers is not near 50 %, `2026-11-23-225-paper-sizes-result.md` 1・2 節; not measured for the others).
fn snaps(per_page: usize) -> bool {
    matches!(per_page, 1..=3 | 6)
}

/// PowerPoint's places for 1, 2, 3 and 6 slides to a landscape handout page, as [`powerpoint_places`] gives them for a portrait one: the box's width as a fraction of
/// the page, the centres of the columns (from the left) and of the rows (from the bottom). The box is the 4:3 one of that width, except 1 to a page (0.8 of the
/// page's height). Measured on Microsoft 365's PDFs on A4 of 16:9 and 4:3 decks (`tasks/doc/requests/2026-11-23-225-paper-sizes-result.md` 3 節: 2 across in a row, 3 across,
/// 6 as 3 by 2, 1 in the middle; every place within 0.25 unit of these). 4 and 9 are measured too (below). The others (8, 16: PowerPoint has no such layouts) are shared out evenly.
fn landscape_places(per_page: usize) -> Option<(f32, &'static [f32], &'static [f32])> {
    const COLUMN_THREE: &[f32] = &[0.2, 0.5, 0.8];
    match per_page {
        1 => Some((0.86, &[0.5], &[0.5])),
        2 => Some((0.41, &[0.275, 0.725], &[0.5])),
        3 => Some((0.26, COLUMN_THREE, &[0.7175])),
        6 => Some((0.26, COLUMN_THREE, &[0.7175, 0.2825])),
        // 4 and 9 to a landscape page: the places of the portrait page (2 by 2, 3 by 3), measured on A4 (`2026-11-25-225-more-papers-result.md`: every place within 0.3 unit).
        4 => Some((0.41, &[0.275, 0.725], &[0.7175, 0.2825])),
        9 => Some((0.26, COLUMN_THREE, &[0.79, 0.5, 0.21])),
        _ => None,
    }
}

/// The height of the box a slide goes in at a place of a landscape handout page, as a fraction of the page's height, where it is not the 4:3 box of the place's
/// width (1 to a page 0.8, as portrait; 4 to a page 0.365, as 2 to a portrait page; 9 to a page 0.22, as 3 and 6 to a portrait page; measured on A4 with 16:9 and 4:3 decks).
fn landscape_box_height(per_page: usize) -> Option<f32> {
    match per_page {
        1 => Some(0.8),
        4 => Some(0.365),
        9 => Some(0.22),
        _ => None,
    }
}

/// PowerPoint's scale of a slide that fits its box at `fit` (the box's width or height over the slide's): a slide near its own size or half of
/// it is drawn at exactly that. With Z the percentage `fit` rounds to, the target T is 100 (looked at first) or 50, and Z from 0.94 T to 1.20 T
/// makes the scale T per cent. 0.94 and 1.20 are what Microsoft 365's PDFs show (the window's edges found for 16:9 widths 200 to 1000 and for
/// 4:3 and portrait slides, and 40 random sizes of 1 to 3 to a page predicted; the reason for the values is not known): an exception the project
/// allows (docs/rule/implimentationRule.md).
fn snap_scale(fit: f32) -> f32 {
    let z = (fit * 100.0).round();
    for t in [100.0_f32, 50.0] {
        if z >= 0.94 * t && z <= 1.20 * t {
            return t / 100.0;
        }
    }
    fit
}

/// PowerPoint's own places for 1, 2, 3, 4, 6 and 9 slides to a portrait handout page (they are not in the file; the handout
/// master holds only the header, date, footer and page number), as fractions of the page: the slide's width, the centres of its
/// columns (from the left) and of its rows (from the bottom: the first row has the largest). A slide is as large as fits a 4:3 box of that width, in the middle of its place. Measured on
/// PowerPoint's PDFs (Microsoft 365 16.0.20527) of a 16:9 deck, 6 layouts, 54 slides: every place within 0.1 pt of
/// `0.07·W` (margin), `0.41·W`, `0.26·W`, centres `0.275·W`, `0.725·W`, `0.2/0.5/0.8·W`, `0.79/0.5/0.21·H`, `0.7175/0.2825·H`.
fn powerpoint_places(per_page: usize) -> Option<(f32, &'static [f32], &'static [f32])> {
    const COLUMN_ONE: &[f32] = &[0.5];
    const COLUMN_TWO: &[f32] = &[0.275, 0.725];
    const COLUMN_THREE: &[f32] = &[0.2, 0.5, 0.8];
    const ROWS_TWO: &[f32] = &[0.7175, 0.2825];
    const ROWS_THREE: &[f32] = &[0.79, 0.5, 0.21];
    match per_page {
        1 => Some((0.86, COLUMN_ONE, &[0.5])),
        2 => Some((0.86, COLUMN_ONE, ROWS_TWO)),
        3 => Some((0.41, &[0.275], ROWS_THREE)),
        4 => Some((0.41, COLUMN_TWO, ROWS_TWO)),
        6 => Some((0.41, COLUMN_TWO, ROWS_THREE)),
        9 => Some((0.26, COLUMN_THREE, ROWS_THREE)),
        _ => None,
    }
}

/// The size of the number under a slide of a handout, in points; and how far under the frame's bottom its baseline is (13.0 pt, a constant of PowerPoint's output).
const SLIDE_NUMBER_PT: f32 = 9.96;
const SLIDE_NUMBER_BASELINE_PT: f32 = 13.0;

/// The pitch of the memo lines beside a slide of a 3 to a page handout, in points (PowerPoint: 17.76 on a 540 pt page, 21 lines).
const MEMO_PITCH_PT: f32 = 17.76;

/// What a handout carries besides the slides (ヘッダーとフッター > ノートと配布資料).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HandoutMarks {
    pub header: Option<String>,
    pub date: Option<String>,
    pub footer: Option<String>,
    /// The number of the page, bottom right.
    pub number: bool,
    /// Where they go and how big: the file's handout master (the corners, 10 pt, if `None`).
    pub layout: Option<ibis_formats::handout_master::HandoutLayout>,
    /// Where a notes page puts the slide's picture and the notes' text: the file's notes master (the usual places if `None`).
    pub notes: Option<ibis_formats::handout_master::NotesFrames>,
    /// The handout's slides go down a column before the next (PowerPoint's 縦, `HandoutOrder` vertical); across a row first if not.
    pub vertical_order: bool,
    /// The number each slide of the handout carries under it, in the order of the slides on it (PowerPoint's own output, which no setting turns
    /// off): the slide's number (the first slide's number counted in, a hidden slide's number skipped). Empty: none.
    pub slide_numbers: Vec<usize>,
    /// The size of the file's slides in points (their proportions alone when `None`).
    pub slide_size_pt: Option<(f32, f32)>,
    /// The paper the page is the size of (`--print-paper` with the page fitted to it, #225): PowerPoint puts a handout's frames where its places on the
    /// page say, with the page as large as the paper (a portrait page 540 x 720 pt grown to A4 is stretched across and down by different factors, so the
    /// frames' places are fractions of the paper, not of a 540 x 720 pt page scaled by one factor). The page is the paper's size, turned on its side with `landscape`.
    pub on_paper: Option<Paper>,
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
    /// The typeface of the bullet (its advance puts the text after it); `None`: the text's own.
    pub bullet_font: Option<String>,
    pub runs: Vec<NoteRun>,
    /// Where the paragraph sits, when it is not the notes' indent by level (the outline page's titles and bodies).
    pub layout: Option<ParaLayout>,
}

/// A paragraph's own margins and spacing, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParaLayout {
    /// The left margin of the text from the box's left edge.
    pub left_pt: f32,
    /// How far the bullet (or number) hangs left of the text.
    pub hang_pt: f32,
    /// The space above the paragraph.
    pub before_pt: f32,
    /// The pitch of its lines.
    pub pitch_pt: f32,
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
        NoteText { paragraphs: text.split('\n').map(|line| NotePara { level: 0, bullet: None, bullet_font: None, runs: vec![NoteRun { text: line.to_string(), ..Default::default() }], layout: None }).collect() }
    }
}

const MARGIN: f32 = 36.0;
/// Pixels per point of a handout page's picture.
pub(crate) const PX_PER_PT: f32 = 2.0;
/// The width of the frame PowerPoint draws ("Frame slides"): 0.96 pt (1.28 of the units of its print PDF, which are 0.75 pt), measured on PrintOut PDFs of slides, notes pages and handouts with and without the option (`tasks/doc/requests/2026-11-18-225-frame.md`).
pub(crate) const FRAME_PT: f32 = 0.96;

/// A paper the pages are printed on (`--print-paper`, #225): its short and long sides in points. Microsoft Print to PDF (PowerPoint's printer) gives a page whole pixels of
/// its 600 dpi (0.12 pt): A4 4961 x 7016 pixels, 595.32 x 841.92 pt (`tasks/doc/design/225-print-paper.md`); the other papers are rounded to whole pixels the same way
/// (only A4 was measured).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paper {
    pub short: f32,
    pub long: f32,
}

/// A printed page on a paper: the page's size, and how the slide's page (the picture of a slide, a notes page or a handout page) is put on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaperFit {
    /// The paper's size in points, in the page's orientation.
    pub page_pt: (f32, f32),
    /// Points of the paper to a point of the page drawn.
    pub scale: f32,
    /// The paper's point of the drawn page's top left corner.
    pub shift: (f32, f32),
}

impl Paper {
    /// The size from millimetres, in whole pixels of the printer's 600 dpi.
    fn from_mm(a: f32, b: f32) -> Self {
        let pt = |mm: f32| (mm / 25.4 * 600.0).round() * 0.12;
        let (a, b) = (pt(a), pt(b));
        Paper { short: a.min(b), long: a.max(b) }
    }

    /// `a3`, `a4`, `a5`, `b4`, `b5` (JIS), `letter`, `legal`, `tabloid` or `<width>x<height>mm`.
    pub fn parse(text: &str) -> Result<Paper, String> {
        let name = text.trim().to_ascii_lowercase();
        let paper = match name.as_str() {
            "a3" => Self::from_mm(297.0, 420.0),
            "a4" => Self::from_mm(210.0, 297.0),
            "a5" => Self::from_mm(148.0, 210.0),
            "b4" => Self::from_mm(257.0, 364.0),
            "b5" => Self::from_mm(182.0, 257.0),
            "letter" => Paper { short: 612.0, long: 792.0 },
            "legal" => Paper { short: 612.0, long: 1008.0 },
            "tabloid" => Paper { short: 792.0, long: 1224.0 },
            _ => {
                let custom = name.strip_suffix("mm").and_then(|n| n.split_once('x')).and_then(|(w, h)| Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?)));
                match custom {
                    Some((w, h)) if w >= 10.0 && h >= 10.0 && w <= 5000.0 && h <= 5000.0 => Self::from_mm(w, h),
                    _ => return Err(format!("--print-paper takes a3, a4, a5, b4, b5, letter, legal, tabloid or <width>x<height>mm: {text}")),
                }
            }
        };
        Ok(paper)
    }

    /// Put a page of `page` points on this paper, as PowerPoint prints (`tasks/doc/requests/2026-11-23-225-patterns-7plus-result.md` 3 節): the paper is landscape when
    /// the page is as wide as it is high (a notes page and a portrait handout stay portrait), the page is scaled to the smaller of the two ratios when it fits the paper
    /// (not at all when it does not: 100 %), and put in the middle.
    pub fn fit(&self, page: (f32, f32), fit: bool) -> PaperFit {
        let (w, h) = if page.0 >= page.1 { (self.long, self.short) } else { (self.short, self.long) };
        let scale = if fit && page.0 > 0.0 && page.1 > 0.0 { (w / page.0).min(h / page.1) } else { 1.0 };
        PaperFit { page_pt: (w, h), scale, shift: ((w - page.0 * scale) / 2.0, (h - page.1 * scale) / 2.0) }
    }
}


/// What a colour is for: PowerPoint prints the grey and the black-and-white pages by the kind of thing, not by one rule for every colour (Windows
/// PowerPoint's `PrintOut` PDFs, `tasks/doc/requests/2026-11-16-225-print-color-result.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintKind {
    /// The colour of an area (a shape's fill, a gradient's stops, a shadow).
    Fill,
    /// The colour of a line (a shape's outline, a border, a rule).
    Line,
    /// The colour of text.
    Text,
    /// A picture's pixels.
    Picture,
}

impl PrintColor {
    /// The colour `rgb` of a `kind` of thing prints as. In grey: a fill and a picture in the Rec.709 luma of the colour
    /// (0.2126 R + 0.7152 G + 0.0722 B: red 54, 00B050 132, 0070C0 94, yellow 237, greys as they are), lines black, text black (white text stays white).
    /// In black and white: fills white (a gradient too), lines and text black, pictures grey as in grey (no dither). In colour: as it is.
    pub fn map_for(self, kind: PaintKind, rgb: (u8, u8, u8)) -> (u8, u8, u8) {
        let luma = || {
            let v = ibis_doc_model::Rgb::from_rgb(rgb.0, rgb.1, rgb.2).luma();
            (v, v, v)
        };
        match (self, kind) {
            (PrintColor::Color, _) => rgb,
            (_, PaintKind::Picture) | (PrintColor::Gray, PaintKind::Fill) => luma(),
            (PrintColor::BlackWhite, PaintKind::Fill) => (255, 255, 255),
            (PrintColor::Gray, PaintKind::Text) if rgb == (255, 255, 255) => rgb,
            (_, PaintKind::Line | PaintKind::Text) => (0, 0, 0),
        }
    }

    /// The colour of a picture's pixel (see `map_for`): what the picture route (`recolor`), which cannot tell a fill from a line, applies.
    pub fn map(self, rgb: (u8, u8, u8)) -> (u8, u8, u8) {
        self.map_for(PaintKind::Picture, rgb)
    }
}

/// `image` in the colour `mode`.
pub fn recolor(image: &mut RgbaImage, mode: PrintColor) {
    if mode == PrintColor::Color {
        return;
    }
    for pixel in image.pixels_mut() {
        let (r, g, b) = mode.map((pixel[0], pixel[1], pixel[2]));
        *pixel = Rgba([r, g, b, pixel[3]]);
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
    let sizes: Vec<(f32, f32)> = slides.iter().map(|s| (s.width() as f32, s.height() as f32)).collect();
    handouts_on(&sizes, per_page, framed, landscape, marks, layer, &mut |size| ImageCanvas::new(size, slides)).into_iter().map(|(canvas, size)| (canvas.page, size)).collect()
}

/// The layout of [`handouts`] over a canvas: `sizes` are the slides' sizes (any unit: their proportions are used), `blank` makes a blank
/// page of the size in points it is given.
pub(crate) fn handouts_on<C: Canvas>(sizes: &[(f32, f32)], per_page: u8, framed: bool, landscape: bool, marks: &HandoutMarks, layer: &mut dyn TextLayer, blank: &mut dyn FnMut((f32, f32)) -> C) -> Vec<(C, (f32, f32))> {
    let (columns, rows) = handout_grid(per_page, landscape);
    let per_page = columns * rows;
    let (page_w, page_h) = match marks.on_paper {
        Some(paper) if landscape => (paper.long, paper.short),
        Some(paper) => (paper.short, paper.long),
        None if landscape => (page_of(marks).1, page_of(marks).0),
        None => page_of(marks),
    };
    // PowerPoint's own places (portrait 1 to 9, landscape 1, 2, 3 and 6); anything else (on its side, 4, 8, 9 or 16) is shared out evenly.
    let places = if landscape { landscape_places(per_page) } else { powerpoint_places(per_page) };
    let cell_w = (page_w - 2.0 * MARGIN) / columns as f32;
    let cell_h = (page_h - 2.0 * MARGIN) / rows as f32;
    // Room between slides.
    let gap = if per_page > 1 { 12.0 } else { 0.0 };
    (0..sizes.len())
        .step_by(per_page.max(1))
        .enumerate()
        .map(|(page_number, first)| {
            let mut page = blank((page_w, page_h));
            for (i, index) in (first..sizes.len().min(first + per_page)).enumerate() {
                let slide = sizes[index];
                let (column, row) = if marks.vertical_order { (i / rows, i % rows) } else { (i % columns, i / columns) };
                let (w, h, x, y) = if let Some((width, xs, ys)) = places {
                    // As large as fits the place's box, in the middle of its place.
                    let box_w = width * page_w;
                    let box_h = if landscape { landscape_box_height(per_page) } else { place_box_height(per_page) }.map_or(box_w * 0.75, |f| f * page_h);
                    // The slide's size in points (the file's), for the snap below; the proportions of `sizes` if it is not known.
                    let pt = marks.slide_size_pt.filter(|_| sizes.iter().all(|s| *s == sizes[0])).unwrap_or(slide);
                    let fit = (box_w / pt.0.max(1.0)).min(box_h / pt.1.max(1.0));
                    let scale = if snaps(per_page) { snap_scale(fit) } else { fit };
                    let (w, h) = (((pt.0 * scale * PX_PER_PT).round() as u32).max(1), ((pt.1 * scale * PX_PER_PT).round() as u32).max(1));
                    let (cx, cy) = (xs[column] * page_w * PX_PER_PT, (1.0 - ys[row]) * page_h * PX_PER_PT);
                    (w, h, cx - w as f32 / 2.0, cy - h as f32 / 2.0)
                } else {
                    let (avail_w, avail_h) = (cell_w - gap, cell_h - gap);
                    let scale = (avail_w / slide.0.max(1.0) * PX_PER_PT).min(avail_h / slide.1.max(1.0) * PX_PER_PT);
                    let (w, h) = (((slide.0 * scale).round() as u32).max(1), ((slide.1 * scale).round() as u32).max(1));
                    // Centred in its cell.
                    (w, h, (MARGIN + column as f32 * cell_w) * PX_PER_PT + (cell_w * PX_PER_PT - w as f32) / 2.0, (MARGIN + row as f32 * cell_h) * PX_PER_PT + (cell_h * PX_PER_PT - h as f32) / 2.0)
                };
                page.put_slide(index, (x.round(), y.round()), (w, h), framed);
                if let Some(number) = marks.slide_numbers.get(index) {
                    // The slide's number under its frame, its left at the frame's: 9.96 pt, in 0.149 grey, its baseline 13.0 pt under the frame's
                    // bottom (Microsoft 365's PDFs, 1, 2 and 3 to a page of a 16:9 deck: 229.43 to 216.34, 386.03 to 372.98, 489.63 to 476.62).
                    let size = SLIDE_NUMBER_PT * PX_PER_PT;
                    let baseline = y.round() + h as f32 + SLIDE_NUMBER_BASELINE_PT * PX_PER_PT;
                    draw_text_on_baseline(layer, &mut page, PX_PER_PT, &number.to_string(), x.round(), baseline, size, Some((38, 38, 38)));
                }
                if per_page == 3 && places.is_some() && !landscape {
                    // Lines for notes to the right of each slide, as many as its height takes, from just under its top.
                    let pitch = MEMO_PITCH_PT * page_w / HANDOUT_PAGE.0 * PX_PER_PT;
                    let left = x.round() + w as f32 + 0.04 * page_w * PX_PER_PT;
                    let right = (1.0 - 0.07) * page_w * PX_PER_PT;
                    let count = (h as f32 / pitch + 0.01).floor() as usize;
                    for line in 1..=count {
                        page.put_rule((left, y.round() + line as f32 * pitch), right - left, 0.96 * PX_PER_PT);
                    }
                }
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
    let sizes: Vec<(f32, f32)> = slides.iter().map(|s| (s.width() as f32, s.height() as f32)).collect();
    notes_pages_on(&sizes, notes, false, marks, layer, &mut |size| ImageCanvas::new(size, slides)).into_iter().map(|(canvas, size)| (canvas.page, size)).collect()
}

/// The layout of [`notes_pages`] over a canvas (see [`handouts_on`]).
pub(crate) fn notes_pages_on<C: Canvas>(sizes: &[(f32, f32)], notes: &[NoteText], framed: bool, marks: &HandoutMarks, layer: &mut dyn TextLayer, blank: &mut dyn FnMut((f32, f32)) -> C) -> Vec<(C, (f32, f32))> {
    let (page_w, page_h) = page_of(marks);
    // The slide image and the notes' body as the file's notes master puts them (EMU of the notes page, to points of this one).
    let (kx, ky) = marks.notes.as_ref().map_or((1.0 / 12700.0, 1.0 / 12700.0), |n| (page_w / n.page.0.max(1) as f32, page_h / n.page.1.max(1) as f32));
    let frame_pt = |f: (i64, i64, i64, i64)| (f.0 as f32 * kx, f.1 as f32 * ky, f.2 as f32 * kx, f.3 as f32 * ky);
    let body = marks.notes.as_ref().map_or((685800, 4400550, 5486400, 3600450), |n| n.body);
    let (body_x, body_y, body_w, body_h) = frame_pt(body);
    // The text sits inside its box by the text box's insets (0.1 in, 0.05 in).
    let (inset_x, inset_y) = (7.2, 3.6);
    let text_w = body_w - 2.0 * inset_x;
    let mut pages = Vec::new();
    for (i, slide) in sizes.iter().enumerate() {
        let mut page = blank((page_w, page_h));
        // The notes master's image box (a file without one: 5486400 EMU wide with the slide's proportions).
        let image = marks.notes.as_ref().map_or((685800, 1143000, 5486400, (5486400.0 * slide.1 / slide.0.max(1.0)).round() as i64), |n| n.image);
        let (image_x, image_y, image_w, image_h) = frame_pt(image);
        // As large as fits the image's box, in the middle of it.
        let scale = (image_w / slide.0.max(1.0)).min(image_h / slide.1.max(1.0)) * PX_PER_PT;
        let (w, h) = (((slide.0 * scale).round() as u32).max(1), ((slide.1 * scale).round() as u32).max(1));
        let (x, y) = (image_x * PX_PER_PT + (image_w * PX_PER_PT - w as f32) / 2.0, image_y * PX_PER_PT + (image_h * PX_PER_PT - h as f32) / 2.0);
        page.put_slide(i, (x.round(), y.round()), (w, h), true);
        if framed {
            // "Frame slides" on a notes page frames the page: in PowerPoint's A4 print the frame is the notes page (540 x 720 pt) at the scale 1.1024 that fits it to the paper's width
            // (595.2 x 793.68 pt, 24.12 pt from the top and the bottom), a line of 0.96 pt on the paper.
            page.put_frame((0.0, 0.0, page_w, page_h), FRAME_PT);
        }
        let mut flow = start_notes(notes.get(i), text_w * PX_PER_PT);
        draw_notes(layer, &mut flow, &mut page, ((body_x + inset_x) * PX_PER_PT, (body_y + inset_y) * PX_PER_PT, text_w * PX_PER_PT, (body_h - 2.0 * inset_y) * PX_PER_PT));
        pages.push(page);
        // What did not fit: pages of text, from the top of the slide image.
        while notes_left(&flow) {
            let mut more = blank((page_w, page_h));
            draw_notes(layer, &mut flow, &mut more, ((body_x + inset_x) * PX_PER_PT, image_y * PX_PER_PT, text_w * PX_PER_PT, (page_h - 56.0 - image_y) * PX_PER_PT));
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
    outline_pages_on(text, marks, layer, &mut |size| ImageCanvas::new(size, &[])).into_iter().map(|(canvas, size)| (canvas.page, size)).collect()
}

/// The layout of [`outline_pages`] over a canvas (see [`handouts_on`]).
pub(crate) fn outline_pages_on<C: Canvas>(text: &NoteText, marks: &HandoutMarks, layer: &mut dyn TextLayer, blank: &mut dyn FnMut((f32, f32)) -> C) -> Vec<(C, (f32, f32))> {
    let (page_w, page_h) = page_of(marks);
    // The text's box as PowerPoint's outline page has it (PDF of a 540 x 720 pt page: x 60 to 480, y from 60 down to 652.8, as fractions of the page).
    let (side, top, bottom) = (page_w * 60.0 / 540.0, page_h * 60.0 / 720.0, page_h * 67.2 / 720.0);
    let mut flow = start_notes(Some(text), (page_w - 2.0 * side) * PX_PER_PT);
    let mut pages = Vec::new();
    loop {
        let mut page = blank((page_w, page_h));
        draw_notes(layer, &mut flow, &mut page, (side * PX_PER_PT, top * PX_PER_PT, (page_w - 2.0 * side) * PX_PER_PT, (page_h - bottom - top) * PX_PER_PT));
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

/// A handout, notes or outline page as pixels (the picture PDF).
pub(crate) struct ImageCanvas<'a> {
    pub(crate) page: RgbaImage,
    slides: &'a [RgbaImage],
}

impl<'a> ImageCanvas<'a> {
    fn new((page_w, page_h): (f32, f32), slides: &'a [RgbaImage]) -> Self {
        ImageCanvas { page: RgbaImage::from_pixel((page_w * PX_PER_PT) as u32, (page_h * PX_PER_PT) as u32, Rgba([255, 255, 255, 255])), slides }
    }
}

impl Canvas for ImageCanvas<'_> {
    fn width(&self) -> f32 {
        self.page.width() as f32
    }

    fn put_slide(&mut self, index: usize, at: (f32, f32), (w, h): (u32, u32), framed: bool) {
        let mut picture = image::imageops::resize(&self.slides[index], w, h, image::imageops::FilterType::Triangle);
        if framed {
            frame(&mut picture);
        }
        image::imageops::overlay(&mut self.page, &picture, at.0.round() as i64, at.1.round() as i64);
    }

    fn put_text(&mut self, picture: &Picture, rows: (u32, u32), at: (i64, i64)) {
        if let Picture::Image(image) = picture {
            multiply(&mut self.page, image, rows, at);
        }
    }

    fn put_frame(&mut self, rect: (f32, f32, f32, f32), line: f32) {
        let (x, y, w, h) = (rect.0 * PX_PER_PT, rect.1 * PX_PER_PT, rect.2 * PX_PER_PT, rect.3 * PX_PER_PT);
        let t = (line * PX_PER_PT).round().max(1.0) as i64;
        let (pw, ph) = (self.page.width() as i64, self.page.height() as i64);
        for k in 0..t {
            for i in (x.round() as i64)..((x + w).round() as i64) {
                for j in [y.round() as i64 + k, (y + h).round() as i64 - 1 - k] {
                    if (0..pw).contains(&i) && (0..ph).contains(&j) {
                        self.page.put_pixel(i as u32, j as u32, Rgba([0, 0, 0, 255]));
                    }
                }
            }
            for j in (y.round() as i64)..((y + h).round() as i64) {
                for i in [x.round() as i64 + k, (x + w).round() as i64 - 1 - k] {
                    if (0..pw).contains(&i) && (0..ph).contains(&j) {
                        self.page.put_pixel(i as u32, j as u32, Rgba([0, 0, 0, 255]));
                    }
                }
            }
        }
    }

    fn put_rule(&mut self, at: (f32, f32), len: f32, thickness: f32) {
        let (x0, x1) = (at.0.round().max(0.0) as u32, ((at.0 + len).round().max(0.0) as u32).min(self.page.width()));
        let top = (at.1 - thickness / 2.0).round();
        for row in 0..(thickness.round().max(1.0) as i64) {
            let y = top as i64 + row;
            if y >= 0 && (y as u32) < self.page.height() {
                for x in x0..x1 {
                    self.page.put_pixel(x, y as u32, Rgba([0, 0, 0, 255]));
                }
            }
        }
    }
}

/// What a slide's window painted, to be put on a handout or notes page as it is (the vector PDF): its records, its width in logical
/// pixels, the device pixels to a logical one, and its size in points.
pub(crate) struct SlideRecords {
    pub(crate) records: Vec<gpui_kit::PaintRecord>,
    pub(crate) width: f32,
    pub(crate) density: f32,
    pub(crate) size_pt: (f32, f32),
    /// The frames of the slide's plain filled shapes without a line (`pdf_vector::Layer::hairline`).
    pub(crate) hairline: Vec<(f32, f32, f32, f32)>,
    /// The frames of its charts (`pdf_vector::Layer::charts`).
    pub(crate) charts: Vec<(f32, f32, f32, f32)>,
}

/// A handout, notes or outline page as the records of its parts (the vector PDF): the slides' own records and the text's, each put
/// where the layout says and cut to its place, so everything on the page stays vector and the text stays text.
pub(crate) struct VectorCanvas<'a> {
    width: f32,
    slides: &'a [SlideRecords],
    pub(crate) layers: Vec<crate::pdf_vector::Layer>,
}

impl<'a> VectorCanvas<'a> {
    pub(crate) fn new((page_w, _): (f32, f32), slides: &'a [SlideRecords]) -> Self {
        VectorCanvas { width: page_w * PX_PER_PT, slides, layers: Vec::new() }
    }
}

impl Canvas for VectorCanvas<'_> {
    fn width(&self) -> f32 {
        self.width
    }

    fn put_slide(&mut self, index: usize, at: (f32, f32), (w, h): (u32, u32), framed: bool) {
        let Some(slide) = self.slides.get(index) else { return };
        let (x, y, w, h) = (at.0 / PX_PER_PT, at.1 / PX_PER_PT, w as f32 / PX_PER_PT, h as f32 / PX_PER_PT);
        self.layers.push(crate::pdf_vector::Layer { records: slide.records.clone(), scale_factor: slide.density, k: w / slide.width.max(1.0), origin: (x, y), clip: Some((x, y, w, h)), hairline: slide.hairline.clone(), charts: slide.charts.clone(), fixed_lines: false });
        if framed {
            // PowerPoint's frame: 0.96 pt, its outer edge at the slide's (the stroke's middle half of it in).
            let line = FRAME_PT;
            let bounds = gpui_kit::Bounds::new(gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y)), gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
            let edge = gpui_kit::Edges::all(gpui_kit::px(line));
            let record = gpui_kit::PaintRecord::Quad {
                bounds,
                clip: bounds,
                background: gpui_kit::Background::from(gpui_kit::Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.0 }),
                border_color: gpui_kit::Hsla { h: 0.0, s: 0.0, l: 0.0, a: 1.0 },
                border_widths: edge,
                corner_radii: gpui_kit::Corners::default(),
                border_style: gpui_kit::BorderStyle::Solid,
            };
            self.layers.push(crate::pdf_vector::Layer { records: vec![record], scale_factor: 1.0, k: 1.0, origin: (0.0, 0.0), clip: None, hairline: Vec::new(), charts: Vec::new(), fixed_lines: true });
        }
    }

    fn put_text(&mut self, picture: &Picture, rows: (u32, u32), at: (i64, i64)) {
        let Picture::Painted { records, width, density, .. } = picture else { return };
        let k = density / PX_PER_PT;
        let (x, y) = (at.0 as f32 / PX_PER_PT, at.1 as f32 / PX_PER_PT);
        // The lines in the rows only (a long text is cut into pages: each page takes what is in its rows).
        let (from, to) = (rows.0 as f32 - 2.0, rows.1 as f32 + 2.0);
        let inside = |top: gpui_kit::Pixels| {
            let y = f32::from(top) * density;
            y >= from && y <= to
        };
        let records = records
            .iter()
            .filter(|r| match r {
                gpui_kit::PaintRecord::Glyph { origin, .. } => inside(origin.y),
                gpui_kit::PaintRecord::Line { bounds, .. } => inside(bounds.top()),
                _ => true,
            })
            .cloned()
            .collect();
        let clip = (x, y, *width as f32 / PX_PER_PT, (rows.1 - rows.0) as f32 / PX_PER_PT);
        self.layers.push(crate::pdf_vector::Layer { records, scale_factor: *density, k, origin: (x, y - rows.0 as f32 / PX_PER_PT), clip: Some(clip), hairline: Vec::new(), charts: Vec::new(), fixed_lines: false });
    }

    fn put_frame(&mut self, rect: (f32, f32, f32, f32), line: f32) {
        let bounds = gpui_kit::Bounds::new(gpui_kit::point(gpui_kit::px(rect.0), gpui_kit::px(rect.1)), gpui_kit::size(gpui_kit::px(rect.2), gpui_kit::px(rect.3)));
        let record = gpui_kit::PaintRecord::Quad {
            bounds,
            clip: bounds,
            background: gpui_kit::Background::from(gpui_kit::Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.0 }),
            border_color: gpui_kit::Hsla { h: 0.0, s: 0.0, l: 0.0, a: 1.0 },
            border_widths: gpui_kit::Edges::all(gpui_kit::px(line)),
            corner_radii: gpui_kit::Corners::default(),
            border_style: gpui_kit::BorderStyle::Solid,
        };
        self.layers.push(crate::pdf_vector::Layer { records: vec![record], scale_factor: 1.0, k: 1.0, origin: (0.0, 0.0), clip: None, hairline: Vec::new(), charts: Vec::new(), fixed_lines: true });
    }

    fn put_rule(&mut self, at: (f32, f32), len: f32, thickness: f32) {
        let (x, y, w, h) = (at.0 / PX_PER_PT, (at.1 - thickness / 2.0) / PX_PER_PT, len / PX_PER_PT, thickness / PX_PER_PT);
        let bounds = gpui_kit::Bounds::new(gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y)), gpui_kit::size(gpui_kit::px(w), gpui_kit::px(h)));
        let record = gpui_kit::PaintRecord::Quad {
            bounds,
            clip: bounds,
            background: gpui_kit::Background::from(gpui_kit::Hsla { h: 0.0, s: 0.0, l: 0.0, a: 1.0 }),
            border_color: gpui_kit::Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.0 },
            border_widths: gpui_kit::Edges::default(),
            corner_radii: gpui_kit::Corners::default(),
            border_style: gpui_kit::BorderStyle::Solid,
        };
        self.layers.push(crate::pdf_vector::Layer { records: vec![record], scale_factor: 1.0, k: 1.0, origin: (0.0, 0.0), clip: None, hairline: Vec::new(), charts: Vec::new(), fixed_lines: false });
    }
}

type NotesFlow = Option<NoteFlow>;

/// The notes' text laid out for boxes `width` pixels wide.
fn start_notes(text: Option<&NoteText>, width: f32) -> NotesFlow {
    text.and_then(|t| NoteFlow::new(t, width.round() as u32))
}

fn draw_notes(layer: &mut dyn TextLayer, flow: &mut NotesFlow, page: &mut dyn Canvas, rect: (f32, f32, f32, f32)) {
    if let Some(flow) = flow {
        flow.draw_next(layer, page, rect);
    }
}

fn notes_left(flow: &NotesFlow) -> bool {
    flow.as_ref().is_some_and(|f| f.has_more())
}

/// The header, date, footer and page number of a handout page: where the
/// handout master puts them, or in the margins' corners.
fn draw_marks(layer: &mut dyn TextLayer, page: &mut dyn Canvas, marks: &HandoutMarks, number: usize, (page_w, page_h): (f32, f32)) {
    use ibis_formats::handout_master::Mark;
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

#[cfg(any(all(test, windows), all(any(target_os = "macos", target_os = "linux", windows), feature = "scenarios")))]
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
        // Rec.709 (PowerPoint's: red 54).
        assert_eq!(gray.get_pixel(0, 0), &Rgba([54, 54, 54, 255]));
        // A picture is as in grey in black and white too (no threshold, no dither).
        let mut bw = solid(2, 2, [255, 0, 0]);
        recolor(&mut bw, PrintColor::BlackWhite);
        assert_eq!(bw.get_pixel(0, 0), &Rgba([54, 54, 54, 255]));
        let mut same = solid(2, 2, [10, 20, 30]);
        recolor(&mut same, PrintColor::Color);
        assert_eq!(same.get_pixel(0, 0), &Rgba([10, 20, 30, 255]));
    }

    /// PowerPoint's colours of the kinds of thing (Windows PowerPoint's PDFs of `PrintOut`, #225): in grey the fills in Rec.709 (red 54, 00B050 132, 0070C0 94,
    /// yellow 237), lines and text black, white text white; in black and white fills white, lines and text black.
    #[test]
    fn colours_by_kind() {
        use PaintKind::*;
        let colors = [(255, 0, 0), (0, 0xB0, 0x50), (0, 0x70, 0xC0), (255, 255, 0), (0xD9, 0xD9, 0xD9), (0x59, 0x59, 0x59)];
        let grey: Vec<u8> = colors.iter().map(|&c| PrintColor::Gray.map_for(Fill, c).0).collect();
        assert_eq!(grey, [54, 132, 94, 237, 217, 89]);
        for c in colors {
            assert_eq!(PrintColor::Gray.map_for(Line, c), (0, 0, 0));
            assert_eq!(PrintColor::Gray.map_for(Text, c), (0, 0, 0));
            assert_eq!(PrintColor::BlackWhite.map_for(Fill, c), (255, 255, 255));
            assert_eq!(PrintColor::BlackWhite.map_for(Line, c), (0, 0, 0));
            assert_eq!(PrintColor::BlackWhite.map_for(Text, c), (0, 0, 0));
            assert_eq!(PrintColor::Color.map_for(Fill, c), c);
        }
        assert_eq!(PrintColor::Gray.map_for(Text, (255, 255, 255)), (255, 255, 255), "white text stays white in grey");
        assert_eq!(PrintColor::BlackWhite.map_for(Text, (255, 255, 255)), (0, 0, 0), "and is black in black and white");
    }

    struct NoText;
    impl TextLayer for NoText {
        fn typeset(&mut self, _: &ibis_doc_model::TextBody, _: u32, _: u32) -> Option<(Picture, Vec<(f32, f32)>)> {
            None
        }
    }

    /// The places PowerPoint gives 16:9 slides on its 540 x 720 pt page (Microsoft 365's PDFs: 3 to a page at (37.8, 89.1 to 631.1)
    /// 221.4 x 124.5, 4 to a page at x 37.8 and 280.8, 9 to a page 140.4 wide) and the 7 memo lines beside a slide of a 3 to a page.
    #[test]
    fn handout_places_are_powerpoints() {
        let slides: Vec<RgbaImage> = (0..9).map(|_| solid(160, 90, [200, 0, 0])).collect();
        let red_box = |page: &RgbaImage, from: (u32, u32), to: (u32, u32)| {
            let mut found: Option<(u32, u32, u32, u32)> = None;
            for y in from.1..to.1 {
                for x in from.0..to.0 {
                    let p = page.get_pixel(x, y);
                    if p[0] == 200 && p[1] == 0 {
                        found = Some(found.map_or((x, y, x, y), |f| (f.0.min(x), f.1.min(y), f.2.max(x), f.3.max(y))));
                    }
                }
            }
            found.expect("a slide")
        };
        let marks = HandoutMarks::default();
        let near = |got: u32, pt: f32| (got as f32 / PX_PER_PT - pt).abs() <= 0.5;
        let three = handouts(&slides, 3, false, false, &marks, &mut NoText);
        let page = &three[0].0;
        assert_eq!(three[0].1, (540.0, 720.0));
        // The top slide: from x 37.8 pt, 221.4 pt wide, 631.1 pt from the bottom (88.9 pt down), 124.5 pt high.
        let (x0, y0, x1, y1) = red_box(page, (0, 0), (page.width() / 2, 440));
        assert!(near(x0, 37.8) && near(x1 + 1, 259.2) && near(y0, 88.9) && near(y1 + 1, 213.4), "{:?}", (x0, y0, x1, y1));
        // Seven lines of 0.96 pt beside it, from 21.6 pt right of the slide to 37.8 pt from the page's edge.
        let column = (370.0 * PX_PER_PT) as u32;
        let rows = (0..440).filter(|&y| page.get_pixel(column, y)[0] == 0).count() as f32 / PX_PER_PT;
        assert!((rows - 7.0 * 0.96).abs() < 1.5, "{rows}");
        // 4 to a page: the second column starts at 280.8 pt; 9 to a page: three columns of 140.4 pt.
        let four = handouts(&slides, 4, false, false, &marks, &mut NoText);
        let (x0, ..) = red_box(&four[0].0, (page.width() / 2, 0), (page.width(), 440));
        assert!(near(x0, 280.8), "{x0}");
        let nine = handouts(&slides, 9, false, false, &marks, &mut NoText);
        let (x0, _, x1, _) = red_box(&nine[0].0, (0, 0), (nine[0].0.width() * 7 / 20, 300));
        assert!(near(x0, 37.8) && near(x1 + 1, 178.2), "{:?}", (x0, x1));
    }

    /// A 4:3 slide's places (Microsoft 365's PDFs of a 4:3 deck, #225): 1 to a page 464.5 x 348.4, 4 to a page 221.4 x 166.0, 9 to a page
    /// 140.4 x 105.2. (2, 3 and 6 to a page are 360 x 270 and 211.2 x 158.4 there: the rule is not known yet.)
    #[test]
    fn handout_places_of_a_four_to_three_slide() {
        let slides: Vec<RgbaImage> = (0..9).map(|_| solid(160, 120, [200, 0, 0])).collect();
        let marks = HandoutMarks::default();
        let near = |got: f32, pt: f32| (got / PX_PER_PT - pt).abs() <= 0.5;
        // (slides to a page, x, width, height, top of the first row from the page's top)
        for (n, x, w, h, top) in [(1u8, 37.8, 464.5, 348.4, 185.7), (4, 37.8, 221.4, 166.0, 120.1), (9, 37.8, 140.4, 105.2, 98.6)] {
            let page = &handouts(&slides, n, false, false, &marks, &mut NoText)[0].0;
            // The first slide: the red inside the box it is expected in (nothing else is that close to it).
            let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
            for (px, py, q) in page.enumerate_pixels() {
                if q[0] == 200 && q[1] == 0 && (px as f32 / PX_PER_PT) < x + w + 1.0 && (py as f32 / PX_PER_PT) < top + h + 1.0 {
                    (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px), y1.max(py));
                }
            }
            let got = (x0 as f32, y0 as f32, x1 as f32 + 1.0 - x0 as f32, y1 as f32 + 1.0 - y0 as f32);
            assert!(near(got.0, x) && near(got.1, top) && near(got.2, w) && near(got.3, h), "{n} to a page: {got:?}");
        }
    }

    /// The 1, 2 and 3 to a page frames of slides of different sizes (Microsoft 365's PDFs, #225): a scale near 100% or 50% snaps to it (a 960 x 540
    /// slide is 480 x 270 at 1 and 2 to a page, a 720 x 405 one 464.4 x 261.2), and a 4:3 slide is as tall as the 2, 3 and 6 to a page boxes let it.
    #[test]
    fn handout_frames_snap_to_a_whole_or_half_scale() {
        let near = |got: f32, pt: f32| (got / PX_PER_PT - pt).abs() <= 0.5;
        // (slide size in pt, slides to a page, the first slide's width and height in pt)
        let cases = [
            ((720.0, 405.0), 1u8, 464.4, 261.2),
            ((960.0, 540.0), 1, 480.0, 270.0),
            ((960.0, 540.0), 2, 480.0, 270.0),
            ((720.0, 405.0), 2, 464.4, 261.2),
            ((720.0, 540.0), 1, 464.4, 348.3),
            ((720.0, 540.0), 2, 360.0, 270.0),
            ((720.0, 540.0), 3, 211.2, 158.4),
            ((720.0, 540.0), 6, 211.2, 158.4),
            ((720.0, 405.0), 3, 221.4, 124.5),
            ((1200.0, 675.0), 1, 464.4, 261.2),
            ((400.0, 225.0), 1, 400.0, 225.0),
        ];
        for ((sw, sh), n, w, h) in cases {
            let slides: Vec<RgbaImage> = (0..2).map(|_| solid((sw / 4.0) as u32, (sh / 4.0) as u32, [200, 0, 0])).collect();
            let marks = HandoutMarks { slide_size_pt: Some((sw, sh)), ..Default::default() };
            let page = &handouts(&slides, n, false, false, &marks, &mut NoText)[0].0;
            let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
            for (px, py, q) in page.enumerate_pixels() {
                if q[0] == 200 && q[1] == 0 && px < (page.width() as f32 * if n >= 3 { 0.5 } else { 1.0 }) as u32 && (n == 1 || (py as f32) < page.height() as f32 / if n == 2 { 2.0 } else { 3.0 }) {
                    (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px), y1.max(py));
                }
            }
            let got = ((x1 + 1 - x0) as f32, (y1 + 1 - y0) as f32);
            assert!(near(got.0, w) && near(got.1, h), "{sw}x{sh} to {n} a page: {got:?}");
        }
    }

    /// #225: a handout on A4 (Microsoft 365's PDFs, `2026-11-23-225-paper-sizes-result.md`; the numbers are its units, 0.75 pt, times 0.75): the frames are put by the places'
    /// fractions of the paper, in landscape too (2 across, 3 across, 6 as 3 by 2, 1 in the middle), and a 4:3 slide that is near half its size snaps to half (6 to a page on A3).
    #[test]
    fn handouts_on_a_paper_are_put_by_fractions_of_the_paper() {
        let near = |got: f32, u: f32| (got / PX_PER_PT - u * 0.75).abs() <= 0.2 * 0.75 + 0.5 / PX_PER_PT;
        // (paper, landscape, slide size in pt, slides to a page, first frame (x, y, w, h) in units, last frame of the page's first)
        let cases = [
            ("a4", false, (960.0, 540.0), 6u8, (55.52, 144.16, 325.44, 183.04)),
            ("a4", false, (720.0, 540.0), 6, (55.52, 113.60, 325.44, 244.0)),
            ("a4", true, (960.0, 540.0), 2, (78.56, 267.52, 460.32, 258.88)),
            ("a4", true, (720.0, 540.0), 2, (68.64, 216.80, 480.0, 360.0)),
            ("a4", true, (960.0, 540.0), 3, (78.56, 142.24, 291.84, 164.0)),
            ("a4", true, (720.0, 540.0), 6, (78.56, 114.88, 291.84, 218.72)),
            ("a4", true, (960.0, 540.0), 1, (78.56, 125.28, 965.44, 543.04)),
            ("a4", true, (720.0, 540.0), 1, (137.76, 79.36, 846.88, 635.2)),
            ("a4", true, (960.0, 540.0), 4, (78.56, 94.88, 460.32, 258.72)),
            ("a4", true, (720.0, 540.0), 4, (115.52, 79.36, 386.24, 289.76)),
            ("a4", true, (960.0, 540.0), 9, (78.56, 84.64, 291.84, 164.0)),
            ("a4", true, (720.0, 540.0), 9, (108.00, 79.36, 232.80, 174.72)),
            ("a4", false, (720.0, 540.0), 4, (55.52, 195.20, 325.44, 244.0)),
            ("a4", false, (960.0, 540.0), 9, (55.52, 177.60, 206.24, 116.0)),
            ("a3", false, (720.0, 540.0), 6, (68.64, 153.28, 480.0, 360.0)),
            ("b4", false, (720.0, 540.0), 6, (68.00, 139.52, 398.24, 298.56)),
            ("letter", false, (720.0, 540.0), 6, (69.60, 105.60, 309.60, 232.16)),
        ];
        for (name, landscape, (sw, sh), n, (x, y, w, h)) in cases {
            let paper = Paper::parse(name).unwrap();
            let slides: Vec<RgbaImage> = (0..2).map(|_| solid((sw / 4.0) as u32, (sh / 4.0) as u32, [200, 0, 0])).collect();
            let marks = HandoutMarks { slide_size_pt: Some((sw, sh)), on_paper: Some(paper), ..Default::default() };
            let page = &handouts(&slides, n, false, landscape, &marks, &mut NoText)[0];
            let (page_w, page_h) = if landscape { (paper.long, paper.short) } else { (paper.short, paper.long) };
            assert_eq!(page.1, (page_w, page_h), "{name} {n}");
            // The first slide's red: the box it is in (the second slide is far from the first's corner, or on another row or column).
            let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
            let (reach_x, reach_y) = (((x + w) * 0.75 + 1.0) * PX_PER_PT, ((y + h) * 0.75 + 1.0) * PX_PER_PT);
            for (px, py, q) in page.0.enumerate_pixels() {
                if q[0] == 200 && q[1] == 0 && (px as f32) < reach_x && (py as f32) < reach_y {
                    (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px), y1.max(py));
                }
            }
            let got = (x0 as f32, y0 as f32, (x1 + 1 - x0) as f32, (y1 + 1 - y0) as f32);
            assert!(near(got.0, x) && near(got.1, y) && near(got.2, w) && near(got.3, h), "{name} landscape {landscape} {sw}x{sh} {n} to a page: {got:?}");
        }
    }

    /// 縦の順 (PowerPoint's HandoutOrder vertical): the second slide of 4 to a page is under the first, not beside it.
    #[test]
    fn vertical_order_goes_down_the_column() {
        let slides = vec![solid(160, 90, [200, 0, 0]), solid(160, 90, [0, 200, 0]), solid(160, 90, [0, 0, 200]), solid(160, 90, [200, 200, 0])];
        let green_at = |vertical: bool| {
            let marks = HandoutMarks { vertical_order: vertical, ..Default::default() };
            let page = &handouts(&slides, 4, false, false, &marks, &mut NoText)[0].0;
            let (mut x, mut y) = (0, 0);
            for (px, py, p) in page.enumerate_pixels() {
                if p[0] == 0 && p[1] == 200 && p[2] == 0 {
                    (x, y) = (px, py);
                }
            }
            (x < page.width() / 2, y < page.height() / 2)
        };
        assert_eq!(green_at(false), (false, true), "across: top right");
        assert_eq!(green_at(true), (true, false), "down: bottom left");
    }
}
