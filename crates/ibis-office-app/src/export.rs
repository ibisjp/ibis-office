// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Exporting slides without a window: numbered PNGs or a PDF.
//!
//!   ibisOffice --export png [--out-dir <dir>] [--width 1920] [--slides 1,3-5] <file.pptx>
//!   ibisOffice --export pdf [--out-dir <dir>] [--slides …] <file.pptx>
//!   ibisOffice --export pdf-image [--out-dir <dir>] [--width 1920] [--slides …] <file.pptx>
//!
//! Files are named after the input: `<name>-01.png`, … or `<name>.pdf`, in
//! the output directory (the current directory by default).
//!
//! Slides are drawn by the same code as the editor, on a headless renderer
//! (GPUI's Metal one on macOS, the DirectX one of ibisOffice's fork of GPUI's
//! Windows platform on Windows), so an export shows exactly what the editor
//! shows. The PDF
//! is vector graphics made from what GPUI painted (text as text, in the
//! fonts drawn with: see `pdf_vector`), with worksheets on their paper, in
//! pages as Excel prints them; `pdf-image` holds one picture per slide or
//! sheet instead (text is not selectable).
//!
//! Needs the `export` feature (on by default), which enables GPUI's test
//! support for its headless renderer.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{EmuRect, Presentation, SheetPrint, Slide};

use crate::render::shape_view::{Resources, shape_element, slide_backdrop};

/// The bounding box and overflow status of text inside an element.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextBounds {
    /// Text bounds x in pixels relative to slide image top-left.
    pub text_x: i64,
    /// Text bounds y in pixels relative to slide image top-left.
    pub text_y: i64,
    /// Width of the rendered text in pixels.
    pub text_width: i64,
    /// Height of the rendered text in pixels.
    pub text_height: i64,
    /// Whether the text overflows the shape frame in any direction.
    pub overflow: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    /// Vector graphics.
    Pdf,
    /// A picture per page.
    PdfImage,
}

pub struct Options {
    pub format: Format,
    /// Directory the files are written to.
    pub out_dir: PathBuf,
    /// File name stem, usually the input's (`決算資料` for `決算資料.pptx`).
    pub name: String,
    /// Width of each slide picture in pixels.
    pub width: u32,
    /// 1-based slide numbers; empty for all.
    pub slides: Vec<usize>,
    /// Also write `<name>-NN.layout.tsv` next to each PNG: where each element
    /// is drawn, for telling what a rendering difference belongs to.
    pub layout: bool,
    /// The input's file name, for a document's `FILENAME` fields.
    pub file_name: Option<String>,
    /// 印刷オプション: 背景の色とイメージを印刷する (DOCX-OPTIONS-010).
    pub print_backgrounds: bool,
    /// 印刷オプション: 隠し文字を印刷する (DOCX-OPTIONS-010).
    pub print_hidden_text: bool,
    /// 印刷オプション: 変更履歴とコメントを印刷する (DOCX-SEC-014).
    pub print_markup: bool,
    /// 印刷の設定: the colour of the pictures (`PdfImage`).
    pub color: crate::export_print::PrintColor,
    /// 配布資料: slides to a page (`PdfImage`).
    pub handout: Option<u8>,
    /// A frame round each slide (`PdfImage`).
    pub frame: bool,
    /// 配布資料 on a landscape page, with a header, date, footer and page number.
    pub handout_landscape: bool,
    /// ノート ページ (`PdfImage`): each slide above its notes on a page.
    pub notes_pages: bool,
    /// アウトライン (`PdfImage`): the slides' titles and bodies as text.
    pub outline_pages: bool,
    /// The file's bytes, for the notes' formatting (`notes_pages`).
    pub notes_source: Option<Vec<u8>>,
    pub handout_marks: crate::export_print::HandoutMarks,
}

/// The notes of slide `index` (from 0) with their formatting (bold, size, colour, bullets), read from the
/// notes page of the file; the plain `notes` if it cannot be.
fn rich_notes(source: Option<&[u8]>, index: usize, notes: &str) -> crate::export_print::NoteText {
    use crate::export_print::{NotePara, NoteRun, NoteText};
    let plain = || NoteText::plain(notes);
    if notes.trim().is_empty() {
        return plain();
    }
    let Some(data) = source else { return plain() };
    let Ok(temp) = ibis_pptx::notes_page::build(data, index) else { return plain() };
    let Ok(opened) = ibis_pptx::open(temp) else { return plain() };
    let Some(slide) = opened.presentation.slides.first() else { return plain() };
    let normal = |t: &str| t.replace('\u{b}', "\n").trim().to_string();
    let body = slide.shapes.iter().filter_map(|s| s.text_body()).find(|b| normal(&b.text()) == notes.trim()).or_else(|| slide.shapes.iter().filter_map(|s| s.text_body()).find(|b| !b.text().trim().is_empty()));
    let Some(body) = body else { return plain() };
    NoteText {
        paragraphs: body
            .paragraphs
            .iter()
            .map(|p| NotePara {
                level: p.level,
                bullet: p.bullet.as_ref().map(|b| b.text.clone()).filter(|t| !t.is_empty()),
                runs: p
                    .runs
                    .iter()
                    .map(|r| {
                        let style = body.style.with_run(&r.style);
                        let c = style.color.0;
                        NoteRun { text: r.text.clone(), bold: style.bold, size_pt: Some(style.font_size_pt()), color: Some((((c >> 16) & 0xff) as u8, ((c >> 8) & 0xff) as u8, (c & 0xff) as u8)) }
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// Parse a slide list such as `1,3,5-8`.
pub fn parse_slides(text: &str) -> Result<Vec<usize>, String> {
    let mut out = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let bad = || format!("Cannot read the slide specification: {part}");
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (usize, usize) = (a.trim().parse().map_err(|_| bad())?, b.trim().parse().map_err(|_| bad())?);
                if a == 0 || b < a {
                    return Err(bad());
                }
                out.extend(a..=b);
            }
            None => out.push(part.parse().ok().filter(|n| *n > 0).ok_or_else(bad)?),
        }
    }
    Ok(out)
}

/// One slide drawn at a fixed size, as the export window's root view.
struct SlidePicture {
    slide: Slide,
    scale: f32,
    size: Size<Pixels>,
    /// The top-left corner of the area drawn, in pixels of the slide.
    origin: (f32, f32),
    /// Where the area goes in the window, and its size (a printed sheet's
    /// part on its paper; else the whole window).
    inset: (f32, f32),
    view: Size<Pixels>,
    resources: Rc<Resources>,
    /// A printed sheet's header and footer, in pixels of the page.
    marks: Vec<crate::xlsx::header_footer::PageMark>,
    /// Black lines on the page (x, y, width, height in pixels): printed
    /// row and column headings' frame.
    rules: Vec<(f32, f32, f32, f32)>,
    /// More parts of the slide drawn on the page (a printed sheet's title
    /// rows and columns): each its origin in the slide, its place on the
    /// page and its size, in pixels.
    extra: Vec<((f32, f32), (f32, f32), Size<Pixels>)>,
    /// Text positions collected during rendering for layout inspection.
    shape_positions: Rc<RefCell<Vec<(ibis_doc_model::ShapeId, crate::render::rich_text::TextPositions)>>>,
}

impl Render for SlidePicture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (scale, res) = (self.scale, &self.resources);
        self.shape_positions.borrow_mut().clear();
        let shape_positions = self.shape_positions.clone();
        let part = |origin: (f32, f32), inset: (f32, f32), view: Size<Pixels>| {
            crate::render::rich_text::set_at_x_offset(inset.0 - origin.0);
            div().absolute().left(px(inset.0)).top(px(inset.1)).w(view.width).h(view.height).overflow_hidden().child(
                div()
                    .absolute()
                    .left(px(-origin.0))
                    .top(px(-origin.1))
                    .size_full()
                    .child(slide_backdrop(&self.slide, scale, res))
                    .children(self.slide.shown_shapes().filter(|s| !res.print || self.slide.shape_prints.get(&s.id).copied().unwrap_or(true)).map(|s| shape_element(s, s.frame, scale, res, None))),
            )
        };
        let extra: Vec<Div> = self.extra.iter().map(|&(o, i, v)| part(o, i, v)).collect();
        crate::render::rich_text::set_at_x_offset(self.inset.0 - self.origin.0);
        let picture = div().relative().w(self.size.width).h(self.size.height).overflow_hidden().bg(rgb(0xffffff)).children(extra).child(
            div().absolute().left(px(self.inset.0)).top(px(self.inset.1)).w(self.view.width).h(self.view.height).overflow_hidden().child(
                div()
                    .absolute()
                    .left(px(-self.origin.0))
                    .top(px(-self.origin.1))
                    .size_full()
                    .child(slide_backdrop(&self.slide, scale, res))
                    .children(self.slide.shown_shapes().filter(|s| !res.print || self.slide.shape_prints.get(&s.id).copied().unwrap_or(true)).map(|s| {
                        if s.text_body().is_some_and(|b| !b.is_empty()) {
                            let pos = crate::render::rich_text::TextPositions::default();
                            shape_positions.borrow_mut().push((s.id, pos.clone()));
                            crate::render::shape_view::shape_element_with_positions(s, s.frame, scale, res, None, Some(&pos))
                        } else {
                            shape_element(s, s.frame, scale, res, None)
                        }
                    })),
            ),
        )
        .when(!self.marks.is_empty() || !self.rules.is_empty(), |d| {
            let (marks, rules) = (self.marks.clone(), self.rules.clone());
            d.child(
                canvas(|_, _, _| {}, move |bounds, _, window, cx| {
                    for &(x, y, w, h) in &rules {
                        window.paint_quad(fill(Bounds::new(point(bounds.origin.x + px(x), bounds.origin.y + px(y)), gpui_kit::size(px(w), px(h))), gpui_kit::black()));
                    }
                    paint_marks(&marks, bounds.origin, window, cx)
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
        });
        // The elements are all built: the offset is not for the other windows.
        crate::render::rich_text::set_at_x_offset(0.0);
        picture
    }
}

/// Paint a header's or footer's runs, each from its baseline.
fn paint_marks(marks: &[crate::xlsx::header_footer::PageMark], origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
    for m in marks {
        if m.run.text.is_empty() {
            continue;
        }
        let rgb_of = |c: ibis_doc_model::Rgb| gpui_kit::rgb(c.0 & 0xFF_FFFF);
        let color: Hsla = m.run.color.map_or(gpui_kit::black(), |c| rgb_of(c).into());
        let size = m.run.size;
        let run = TextRun {
            len: m.run.text.len(),
            font: m.font.clone(),
            color,
            background_color: None,
            underline: (m.run.underline == 1).then(|| UnderlineStyle { thickness: px((size / 16.0).max(0.5)), color: Some(color), wavy: false }),
            strikethrough: m.run.strike.then(|| StrikethroughStyle { thickness: px((size / 16.0).max(0.5)), color: Some(color) }),
        };
        let line = window.text_system().shape_line(SharedString::from(m.run.text.clone()), px(size), &[run], None);
        let at = point(origin.x + px(m.x), origin.y + px(m.baseline) - line.ascent);
        line.paint(at, line.ascent + line.descent, TextAlign::Left, None, window, cx).ok();
        if m.run.underline == 2 {
            // A double underline: two thin lines under the baseline.
            let t = (size / 20.0).max(0.4);
            for k in [1.0, 2.6] {
                let y = origin.y + px(m.baseline + t * k * 1.5);
                window.paint_quad(fill(Bounds::new(point(origin.x + px(m.x), y), gpui_kit::size(line.width, px(t))), color));
            }
        }
    }
}

/// How a worksheet prints: its page setup, the scale it prints at, and the
/// part of `area` (EMU of the slide) on each page, in the order printed.
///
/// As Excel lays it out: a page holds whole columns and rows as far as they
/// fit (a column or row larger than the page is cut), a manual break starts
/// a new page, and fitting to a number of pages shrinks the sheet until its
/// pages are that many (and leaves out the manual breaks).
/// A sheet's print titles (`_xlnm.Print_Titles`): the rows repeated at the
/// top of each page and the columns at the left, as spans of the slide
/// (EMU).
pub(crate) type Titles = (Option<(i64, i64)>, Option<(i64, i64)>);

pub(crate) fn print_titles(presentation: &Presentation, slide: &Slide) -> Titles {
    let Some(workbook) = presentation.workbook.as_ref() else { return (None, None) };
    let Some(index) = workbook.sheets.iter().position(|s| s.slide == Some(slide.id)) else { return (None, None) };
    let Some(name) = workbook.names.iter().find(|n| n.sheet == Some(index) && n.name.eq_ignore_ascii_case("_xlnm.Print_Titles")) else { return (None, None) };
    let Some((frame, sheet)) = slide.shapes.iter().find_map(|s| match &s.kind {
        ibis_doc_model::ShapeKind::Sheet(sheet) => Some((s.frame, sheet)),
        _ => None,
    }) else {
        return (None, None);
    };
    let (rows, columns) = crate::xlsx::header_footer::parse_titles(&name.formula);
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let at = |list: &[i64], i: u32| list[(i as usize).min(list.len() - 1)];
    (
        rows.map(|(a, b)| (frame.y + at(&ys, a), frame.y + at(&ys, b + 1))).filter(|(a, b)| b > a),
        columns.map(|(a, b)| (frame.x + at(&xs, a), frame.x + at(&xs, b + 1))).filter(|(a, b)| b > a),
    )
}

pub(crate) fn sheet_pages(slide: &Slide, area: EmuRect, titles: Titles, headings: (f64, f64)) -> Option<(SheetPrint, f64, Vec<EmuRect>)> {
    sheet_pages_with_text(slide, area, titles, headings, &[])
}

/// What Excel for Windows multiplies the scale by to print a sheet laid out for Letter on A4 paper.
const LETTER_ON_A4: f64 = 0.95;

fn sheet_pages_with_text(slide: &Slide, area: EmuRect, titles: Titles, headings: (f64, f64), text: &[EmuRect]) -> Option<(SheetPrint, f64, Vec<EmuRect>)> {
    let (frame, sheet) = slide.shapes.iter().find_map(|s| match &s.kind {
        ibis_doc_model::ShapeKind::Sheet(sheet) => Some((s.frame, sheet)),
        _ => None,
    })?;
    let print = sheet.print.clone();
    let (paper_w, paper_h) = print.page_size();
    // The pages of a sheet that names Letter on an A4 printer break where Letter's would (on A4 paper).
    let (paper_w, paper_h) = if print.letter_on_a4 {
        let (w, h) = (ibis_doc_model::EMU_PER_INCH * 17 / 2, ibis_doc_model::EMU_PER_INCH * 11);
        if print.landscape { (h, w) } else { (w, h) }
    } else {
        (paper_w, paper_h)
    };
    let [left, right, top, bottom] = print.margins;
    let least = ibis_doc_model::EMU_PER_INCH / 2;
    // A page holds the columns and a vertical line more: the last column's right border (or grid line) is a box of 7
    // printer pixels with a pixel of room beside it (0.96 pt at 600 dpi, whatever the scale). Excel for Windows' PDFs of two
    // centred sheets (008, 072) have the room that much shorter than the paper less the margins, and 008's last column
    // (23 px of its 4178 px, against 4181 px of room) is on a page of its own.
    let room = ((paper_w - left - right - 12_192).max(least) as f64, (paper_h - top - bottom).max(least) as f64);
    let xs: Vec<i64> = sheet.column_offsets().iter().map(|x| frame.x + x).collect();
    let ys: Vec<i64> = sheet.row_offsets().iter().map(|y| frame.y + y).collect();
    let at = |edges: &[i64], breaks: &[u32]| -> Vec<i64> { breaks.iter().filter_map(|b| edges.get(*b as usize)).copied().collect() };
    // Fitting to a number of pages leaves out the manual breaks along that
    // axis; along an axis of automatic pages (0) they are kept: Excel for Mac
    // prints 1 wide × automatic tall with a row break on 2 pages and with a
    // column break on 1 (XPAGE-61 to 67).
    let (column_breaks, row_breaks) = match print.fit_to {
        Some((across, down)) => (
            if across == 0 { at(&xs, &print.column_breaks) } else { Vec::new() },
            if down == 0 { at(&ys, &print.row_breaks) } else { Vec::new() },
        ),
        None => (at(&xs, &print.column_breaks), at(&ys, &print.row_breaks)),
    };
    let mut scale = match print.fit_to {
        Some((across, down)) => {
            // The sheet prints a point wider and taller than its cells: the
            // grid line before the first column and row (borders and fills
            // reach over it), as Excel fits it (a 554 pt wide print area on
            // 537.7 pt of room prints at 96 %, not 97: xlsx-XBRD BB14).
            let (w, h) = (area.width + 12_700, area.height + 12_700);
            let mut s: f64 = 1.0;
            if across > 0 {
                s = s.min(room.0 * across as f64 / w.max(1) as f64);
            }
            if down > 0 {
                s = s.min(room.1 * down as f64 / h.max(1) as f64);
            }
            // A whole percentage, as Excel prints.
            (s * 100.0).floor() / 100.0
        }
        None => print.scale as f64 / 100.0,
    }
    .max(0.1);
    loop {
        // A page that does not hold the title rows (columns) has them
        // printed over (left of) it, taking their room.
        let reserve = |t: Option<(i64, i64)>| t.map(|(a, b)| (b, b - a));
        // Printed row and column headings take their room on every page.
        let (hw, hh) = if print.headings { (headings.0 * 12_700.0, headings.1 * 12_700.0) } else { (0.0, 0.0) };
        let columns = page_spans(&xs, &column_breaks, area.x, area.x + area.width, room.0 / scale - hw, reserve(titles.1));
        let rows = page_spans(&ys, &row_breaks, area.y, area.y + area.height, room.1 / scale - hh, reserve(titles.0));
        let fits = print.fit_to.is_none_or(|(across, down)| (across == 0 || columns.len() <= across as usize) && (down == 0 || rows.len() <= down as usize));
        if fits || scale <= 0.1 {
            let page = |(x0, x1): (i64, i64), (y0, y1): (i64, i64)| EmuRect::new(x0, y0, x1 - x0, y1 - y0);
            let mut pages: Vec<_> = if print.over_then_down {
                rows.iter().flat_map(|r| columns.iter().map(|c| page(*c, *r))).collect()
            } else {
                columns.iter().flat_map(|c| rows.iter().map(|r| page(*c, *r))).collect()
            };
            // Excel keeps empty intermediate pages, but stops at the final
            // occupied page when no explicit print area was specified. A
            // formula/literal empty string is still an occupied cell; an
            // empty cell's number format alone is not. Headers, footers,
            // grid lines and conditional formatting decorate pages already
            // selected, rather than creating new pages (Windows Office probes).
            if sheet.print_area.is_none() {
                let mut occupied = sheet_print_regions(slide, frame, sheet);
                occupied.extend_from_slice(text);
                let intersects = |a: EmuRect, b: EmuRect| a.width > 0 && a.height > 0 && b.width > 0 && b.height > 0
                    && a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom();
                let has_content = |page: EmuRect| {
                    occupied.iter().any(|&r| {
                        intersects(r, page)
                            || titles.0.is_some_and(|(a, b)| page.y >= b && intersects(r, EmuRect::new(page.x, a, page.width, b - a)))
                            || titles.1.is_some_and(|(a, b)| page.x >= b && intersects(r, EmuRect::new(a, page.y, b - a, page.height)))
                            || titles.0.zip(titles.1).is_some_and(|((ya, yb), (xa, xb))| page.y >= yb && page.x >= xb && intersects(r, EmuRect::new(xa, ya, xb - xa, yb - ya)))
                    })
                };
                if let Some(last) = pages.iter().rposition(|&page| has_content(page)) {
                    pages.truncate(last + 1);
                } else {
                    pages.truncate(1);
                }
            }
            // The sheet drawn on A4 as Letter's pages (`letter_on_a4`) is drawn at 0.95 of the scale, to a whole percent
            // (Excel for Windows' PDFs of the letter-a4 probes: columns, rows and text of 100 %, 82 % and 50 % print at 95,
            // 77 and 47 % of the A4 ones; the factor is Excel's: with its option "Scale content for A4 or 8.5 x 11" paper
            // sizes" (MapPaperSize, on by default) turned off, a sheet naming Letter prints on Letter paper at the A4 sizes,
            // the pages still broken as on Letter). The pages break as at the scale set, on Letter's paper.
            let drawn = if print.letter_on_a4 { ((scale * 100.0 * LETTER_ON_A4 + 1e-6).floor() / 100.0).max(0.1) } else { scale };
            return Some((print, drawn, pages));
        }
        scale = ((scale * 100.0).round() - 1.0).max(10.0) / 100.0;
    }
}

/// Regions that establish the sheet's print extent, before conditional
/// formatting and page decorations. These are source cell contents/formats,
/// merged cells and objects marked for printing, not a pixel-ink heuristic.
fn sheet_print_regions(slide: &Slide, frame: EmuRect, sheet: &ibis_doc_model::Sheet) -> Vec<EmuRect> {
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let edge = |list: &[i64], i: u32, default: i64| {
        list.get(i as usize).copied().unwrap_or_else(|| list.last().copied().unwrap_or(0) + i.saturating_sub(list.len().saturating_sub(1) as u32) as i64 * default)
    };
    let rectangle = |range: ibis_doc_model::CellRange| {
        let x0 = edge(&xs, range.first_column, sheet.default_column);
        let x1 = edge(&xs, range.last_column.saturating_add(1), sheet.default_column);
        let y0 = edge(&ys, range.first_row, sheet.default_row);
        let y1 = edge(&ys, range.last_row.saturating_add(1), sheet.default_row);
        EmuRect::new(frame.x + x0, frame.y + y0, x1 - x0, y1 - y0)
    };
    let mut regions = Vec::new();
    for cell in sheet.cells.iter() {
        let decorated = sheet.styles.get(cell.style as usize).is_some_and(|s| {
            s.fill.is_some() || s.fill_theme.is_some() || s.gradient.is_some() || s.pattern.is_some()
                || [s.left, s.right, s.top, s.bottom, s.diagonal_up, s.diagonal_down].iter().any(Option::is_some)
        });
        if cell.formula.is_none() && cell.text.is_empty() && matches!(cell.value, ibis_doc_model::CellValue::Empty) && !decorated { continue }
        let range = sheet.merges.iter().find(|m| m.first_row <= cell.row && cell.row <= m.last_row && m.first_column <= cell.column && cell.column <= m.last_column)
            .copied().unwrap_or(ibis_doc_model::CellRange { first_row: cell.row, last_row: cell.row, first_column: cell.column, last_column: cell.column });
        regions.push(rectangle(range));
    }
    // Cell pictures are also ordinary printed shapes, but retaining their cell
    // locations handles images whose decode or network request is still pending.
    for image in &sheet.cell_images {
        regions.push(rectangle(ibis_doc_model::CellRange { first_row: image.row, last_row: image.row, first_column: image.column, last_column: image.column }));
    }
    for &(row, column) in sheet.web_images.keys() {
        if sheet.cell(row, column).is_some_and(|cell| cell.formula.is_some()) {
            regions.push(rectangle(ibis_doc_model::CellRange { first_row: row, last_row: row, first_column: column, last_column: column }));
        }
    }
    for spark in sheet.sparklines.iter().flat_map(|group| &group.sparklines) {
        regions.push(rectangle(ibis_doc_model::CellRange { first_row: spark.row, last_row: spark.row, first_column: spark.column, last_column: spark.column }));
    }
    for shape in slide.shown_shapes().filter(|s| !matches!(s.kind, ibis_doc_model::ShapeKind::Sheet(_)) && slide.shape_prints.get(&s.id).copied().unwrap_or(true)) {
        regions.push(shape.frame);
    }
    regions
}

/// Shape print text once per sheet, and extend its stored grid only as far as
/// the measured overflow needs. The painter and pagination then use the same
/// cell edges. Fonts and glyph advances supply the extent, with no fit factor.
fn prepare_sheet_print_text(presentation: &mut Presentation, resources: &Resources) -> HashMap<ibis_doc_model::SlideId, Vec<EmuRect>> {
    let default_size = presentation.slide_size;
    let mut result = HashMap::new();
    for slide in &mut presentation.slides {
        let mut regions = Vec::new();
        let mut width = slide.size.unwrap_or(default_size).width;
        for shape in &mut slide.shapes {
            let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
            let local = crate::xlsx::sheet_view::printed_text_regions(sheet, resources);
            let end = local.iter().map(EmuRect::right).max().unwrap_or(0);
            if end > shape.frame.width && sheet.print_area.is_none() {
                let sheet = Arc::make_mut(sheet);
                let mut edge = sheet.column_offsets().last().copied().unwrap_or(0);
                while edge < end && sheet.columns.len() < 16_384 && sheet.default_column > 0 {
                    sheet.columns.push(sheet.default_column);
                    edge += sheet.default_column;
                }
                shape.frame.width = end;
                width = width.max(shape.frame.right());
            }
            regions.extend(local.into_iter().map(|r| r.translated(shape.frame.x, shape.frame.y)));
        }
        if !regions.is_empty() {
            let mut size = slide.size.unwrap_or(default_size);
            size.width = width;
            slide.size = Some(size);
            result.insert(slide.id, regions);
        }
    }
    result
}

/// `start..end` cut into spans no longer than `room`, at the cell `edges`
/// (sorted) where they can be and at the manual `breaks`.
fn page_spans(edges: &[i64], breaks: &[i64], start: i64, end: i64, room: f64, titles: Option<(i64, i64)>) -> Vec<(i64, i64)> {
    let room = (room.floor() as i64).max(1);
    let mut out = Vec::new();
    let mut at = start;
    while at < end {
        // Past the titles (their end, their size), a page holds less.
        let here = match titles {
            Some((title_end, size)) if at >= title_end => (room - size).max(room / 4).max(1),
            _ => room,
        };
        let limit = (at + here).min(end);
        let next = match breaks.iter().find(|b| **b > at && **b < limit) {
            Some(b) => *b,
            None if limit == end => end,
            None => {
                // The furthest edge that fits. A column (row) wider than the page gets a page of its
                // own, cut at the page's edge: Excel does not go on with the rest of it on the next.
                let i = edges.partition_point(|e| *e <= limit);
                edges[..i].last().copied().filter(|e| *e > at).or_else(|| edges.get(i).copied().filter(|e| *e > at)).unwrap_or(limit)
            }
        };
        out.push((at, next));
        at = next;
    }
    out
}

/// The part of `slide` to export: a worksheet's print area, as Excel
/// prints it, or the whole slide.
/// The pages a worksheet prints on (as spans of the slide, EMU), for the
/// page break preview and the page layout view: the cut lines are where it
/// breaks; and the scale it prints at (cells are drawn at that scale on paper).
pub(crate) fn preview_pages(presentation: &Presentation, slide: &Slide, resources: &Resources) -> (f64, Vec<EmuRect>) {
    let mut area = export_area(presentation, slide);
    let text: Vec<_> = slide.shapes.iter().filter_map(|shape| match &shape.kind {
        ibis_doc_model::ShapeKind::Sheet(sheet) => Some((shape.frame, sheet)),
        _ => None,
    }).flat_map(|(frame, sheet)| crate::xlsx::sheet_view::printed_text_regions(sheet, resources).into_iter().map(move |r| r.translated(frame.x, frame.y))).collect();
    if slide.shapes.iter().any(|shape| matches!(&shape.kind, ibis_doc_model::ShapeKind::Sheet(sheet) if sheet.print_area.is_none())) {
        area.width = area.width.max(text.iter().map(EmuRect::right).max().unwrap_or(area.right()) - area.x);
    }
    let titles = print_titles(presentation, slide);
    let default_style = presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
    let headings = slide
        .shapes
        .iter()
        .find_map(|s| match &s.kind {
            ibis_doc_model::ShapeKind::Sheet(sheet) => Some(crate::xlsx::header_footer::heading_size(sheet, &default_style, resources)),
            _ => None,
        })
        .unwrap_or((0.0, 0.0));
    sheet_pages_with_text(slide, area, titles, headings, &text).map(|(_, scale, pages)| (scale, pages)).unwrap_or((1.0, Vec::new()))
}

/// What Excel for Windows prints a sheet's columns and rows at, against the
/// sizes it shows them at on the screen (96 dpi pixels): it lays a printed page
/// out in the printer's units: a column comes out at the Normal font's printed digit width over
/// its screen one (`print_column_factor`: 0.917 for ＭＳ Ｐゴシック 11, 1.008 for 游ゴシック 11) and a
/// row 0.907 of its height. Measured on Excel 365's PDFs of a sheet of
/// 9.44-character columns (57 pt on the screen, 52.46 pt printed) and 18 pt rows (16.32 pt);
/// `IBIS_PRINT_COLUMNS` and `IBIS_PRINT_ROWS` override them for measuring.
pub(crate) fn windows_print_geometry(presentation: &mut Presentation) {
    if presentation.workbook.is_none() || ibis_pptx::EXCEL_METRICS != ibis_pptx::ExcelMetrics::Windows {
        return;
    }
    let factor = |name: &str, default: f64| std::env::var(name).ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(default);
    let style = presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
    let font_pt = if style.font_size > 0 { style.font_size as f64 / 100.0 } else { 11.0 };
    let by_font = ibis_pptx::EXCEL_METRICS.print_column_factor(style.font.as_deref(), font_pt);
    // The row factor is the printer's standard row over the screen's, from the Normal font's text metrics; the
    // mechanism is not worked out yet (0.9067 is the one measured for ＭＳ Ｐゴシック 11, todo-adhoc A1: an exception the
    // user decided to keep, 2026-10-21). Without it the corpus' xlsx page counts fall from 117 to 77 of 172 matching
    // (the one-page documents print on two: 059, 074, 107..110, 117 ...). It was set to 1.0 on 2026-10-22 (d34fbed79) to
    // make Mac-measured scenarios pass; that is not Excel for Windows.
    let (cf, rf) = (factor("IBIS_PRINT_COLUMNS", by_font), factor("IBIS_PRINT_ROWS", 0.9067));
    if (cf - 1.0).abs() < 1e-9 && (rf - 1.0).abs() < 1e-9 {
        return;
    }
    let by = |v: i64, f: f64| (v as f64 * f).round() as i64;
    for slide in &mut presentation.slides {
        if !slide.shapes.iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Sheet(_))) {
            continue;
        }
        if let Some(size) = &mut slide.size {
            *size = ibis_doc_model::EmuSize::new(by(size.width, cf), by(size.height, rf));
        }
        for shape in &mut slide.shapes {
            let f = shape.frame;
            // A picture or a chart keeps its own size (Excel prints it at the size it is, 411 pt wide
            // in a page of 419.7 pt of columns); it is put where its cell now is. The sheet itself is
            // all its cells.
            let is_sheet = matches!(shape.kind, ibis_doc_model::ShapeKind::Sheet(_));
            shape.frame = ibis_doc_model::EmuRect::new(by(f.x, cf), by(f.y, rf), if is_sheet { by(f.width, cf) } else { f.width }, if is_sheet { by(f.height, rf) } else { f.height });
            if let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind {
                let sheet = std::sync::Arc::make_mut(sheet);
                sheet.columns.iter_mut().for_each(|c| *c = by(*c, cf));
                sheet.rows.iter_mut().for_each(|r| *r = by(*r, rf));
                sheet.default_column = by(sheet.default_column, cf);
                sheet.default_row = by(sheet.default_row, rf);
            }
        }
    }
}

pub(crate) fn export_area(presentation: &Presentation, slide: &Slide) -> EmuRect {
    let size = presentation.size_of(slide);
    let whole = ibis_doc_model::EmuRect::new(0, 0, size.width, size.height);
    let Some(sheet) = slide.shapes.iter().find_map(|s| match &s.kind {
        ibis_doc_model::ShapeKind::Sheet(sheet) => Some((s.frame, sheet)),
        _ => None,
    }) else {
        return whole;
    };
    let (frame, sheet) = sheet;
    let Some(area) = sheet.print_area else { return whole };
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let at = |list: &[i64], i: u32| list[(i as usize).min(list.len() - 1)];
    let (x0, y0) = (at(&xs, area.first_column), at(&ys, area.first_row));
    // Whole rows or columns reach only as far as the sheet is used (the
    // grid goes on past it).
    let (x1, y1) = (at(&xs, area.last_column.saturating_add(1)).min(size.width - frame.x), at(&ys, area.last_row.saturating_add(1)).min(size.height - frame.y));
    ibis_doc_model::EmuRect::new(frame.x + x0, frame.y + y0, (x1 - x0).max(1), (y1 - y0).max(1))
}

/// The sheet as it prints: a row whose height the file fixes (`customHeight`) is laid out on whole
/// points, a fractional stored height cut off (24.75 pt prints as 24 pt; 30 such rows fit the page
/// that 742.5 pt would overflow). The cut is made before the print scale applies. The model keeps
/// the stored height, so editing and saving are unchanged.
pub(crate) fn print_layout(presentation: &Presentation, slide: &mut Slide) {
    snap_custom_rows(presentation, slide);
}

/// The custom rows of a sheet slide on the grid Excel prints them on: for Excel for Windows whole 96 dpi pixels (0.75 pt, cut
/// down: 5.85 pt = 7.8 px is 7 px = 5.25 pt; 37.5 pt = 50 px stays; a height written to 4 decimals of a centimetre by another
/// format is not cut by that noise), for Excel for Mac and the web whole points. Its PDFs: 118's rows 1..32 (stored 1029 pt) are
/// 1029 pt, not the 1008 pt of whole points (9 pages, not 6); 058's 150 rows of 5.85 pt print on 1 page. The slide (and the
/// sheet's frame) is as high as the stored rows; the rows shorter, it ends sooner.
fn snap_custom_rows(presentation: &Presentation, slide: &mut Slide) {
    let mut cut: i64 = 0;
    for shape in &mut slide.shapes {
        let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
        if sheet.custom_rows.is_empty() {
            continue;
        }
        let mut shape_cut: i64 = 0;
        let sheet = std::sync::Arc::make_mut(sheet);
        let used = sheet.custom_rows.clone();
        for row in used {
            if let Some(h) = sheet.rows.get_mut(row as usize) {
                if *h == 0 {
                    continue;
                }
                let emu = ibis_doc_model::EMU_PER_POINT as f64;
                let _ = emu;
                let snapped = {
                    // Whole points (the reader has already put a Windows row on its 0.75 pt pixels).
                    let whole = ibis_doc_model::EMU_PER_POINT;
                    if *h >= whole { *h - *h % whole } else { *h }
                };
                shape_cut += *h - snapped;
                *h = snapped;
            }
        }
        shape.frame.height = (shape.frame.height - shape_cut).max(1);
        cut += shape_cut;

    }
    if cut > 0 {
        let mut size = presentation.size_of(slide);
        size.height = (size.height - cut).max(1);
        slide.size = Some(size);
    }
}

/// A sheet printed black and white (`pageSetup/@blackAndWhite`): no fills,
/// black text and black lines (Excel for Mac's PDF of XPAGE-42: the header's
/// blue fill gone, the grey borders black).
fn black_and_white(slide: &mut Slide) {
    for shape in &mut slide.shapes {
        let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
        if !sheet.print.black_and_white {
            continue;
        }
        let sheet = std::sync::Arc::make_mut(sheet);
        for style in &mut sheet.styles {
            style.fill = None;
            style.fill_theme = None;
            style.gradient = None;
            style.pattern = None;
            style.color = ibis_doc_model::Rgb::BLACK;
            style.color_theme = None;
            for border in [&mut style.left, &mut style.right, &mut style.top, &mut style.bottom, &mut style.diagonal_up, &mut style.diagonal_down].into_iter().flatten() {
                border.color = ibis_doc_model::Rgb::BLACK;
            }
        }
    }
}

/// A headless app to draw in, with the platform's text system (text laid
/// out as in the editor) and its headless renderer if it has one, and the
/// platform, to be kept as long as the app.
pub(crate) fn headless_app() -> (Rc<dyn Platform>, HeadlessAppContext) {
    crate::xlsx::sheet_view::release_layouts();
    // On Windows the headless platform has no DirectWrite (its text system
    // lays out nothing): the full one, which opens no window either.
    let platform = gpui_kit::platform::current_platform(cfg!(not(windows)));
    let cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        Arc::new(crate::shell::icons::Assets::new("")),
        headless_renderer,
    );
    (platform, cx)
}

/// The platform's renderer drawing without a window: GPUI's Metal one, or
/// on Windows the DirectX one and on Linux the wgpu one of ibisOffice's
/// forks (GPUI has none there).
pub(crate) fn headless_renderer() -> Option<Box<dyn PlatformHeadlessRenderer>> {
    #[cfg(windows)]
    return gpui_windows::DirectXHeadlessRenderer::new()
        .map_err(|e| log::error!("DirectX: {e:#}"))
        .ok()
        .map(|renderer| Box::new(renderer) as Box<dyn PlatformHeadlessRenderer>);
    #[cfg(target_os = "linux")]
    return gpui_wgpu::WgpuHeadlessRenderer::new()
        .map_err(|e| log::error!("wgpu: {e:#}"))
        .ok()
        .map(|renderer| Box::new(renderer) as Box<dyn PlatformHeadlessRenderer>);
    #[cfg(not(any(windows, target_os = "linux")))]
    gpui_kit::platform::current_headless_renderer()
}

/// Print the time since `since` (and restart it) under `IBIS_DEBUG_TIMING`.
fn lap(what: &str, since: &mut crate::clock::Instant) {
    if std::env::var_os("IBIS_DEBUG_TIMING").is_some() {
        eprintln!("export: {what} {:.0} ms", since.elapsed().as_secs_f64() * 1000.0);
    }
    *since = crate::clock::Instant::now();
}

/// Export `presentation` as `options` say. Prints each file written.
pub fn export(mut presentation: Presentation, options: &Options) -> Result<(), String> {
    // Pictures are drawn as a PDF rasterizer draws them (the coverage
    // tests' references): glyphs without macOS' font smoothing, which
    // thickens their stems (16 % more ink in PTXT; the screen keeps it, as
    // macOS and Office draw text there).
    #[cfg(target_os = "macos")]
    gpui_macos::set_font_smoothing(false);
    let mut lap_at = crate::clock::Instant::now();
    let (_platform, mut cx) = headless_app();
    lap("headless app", &mut lap_at);
    // Picture decoding runs on real threads.
    cx.allow_parking();
    // Sheets come out as Excel prints them, without grid lines.
    let resources = Rc::new(cx.update(|app| {
        let mut resources = Resources::new(&presentation, app);
        resources.print = true;
        resources.print_backgrounds = options.print_backgrounds;
        resources.print_hidden_text = options.print_hidden_text;
        resources.print_markup = options.print_markup;
        // Shapes where they are meant to be, as PDF rasterizers draw them.
        // A presentation's picture (PNG, a picture per PDF page) stays on
        // whole pixels as the screen draws it; its vector PDF is exact, its
        // coordinates PowerPoint's (2026-10-06, tasks/doc/pptx-pdf-vs-png.md:
        // exact matched PowerPoint's PDF coordinates in 32 more tests and in
        // none fewer).
        resources.exact = presentation.flow.is_some() || presentation.workbook.is_some() || options.format == Format::Pdf;
        // Printed with every picture.
        resources.preload.wait_all();
        resources
    }));
    // A flowing document becomes its pages; the art borders' pictures are
    // made for the export's pixels (the pages fill `options.width`).
    if let Some(doc) = &presentation.flow {
        let page_pt = doc.page.size.width as f32 / ibis_doc_model::EMU_PER_POINT as f32;
        if page_pt > 0.0 {
            crate::docx::art_border::set_export_px_per_pt(options.width.max(16) as f32 / page_pt);
        }
    }
    crate::docx::flow_layout::update_fields_on_open(&mut presentation, &resources);
    crate::docx::flow_layout::expand_pages(&mut presentation, &resources, options.file_name.as_deref());
    let ts = cx.update(|app| app.text_system().clone());
    crate::xlsx::pivot::fit_built_pivots(&mut presentation, &resources, &ts);
    crate::xlsx::row_fit::fit_measured_rows(&mut presentation, &resources);
    windows_print_geometry(&mut presentation);
    let sheet_text = prepare_sheet_print_text(&mut presentation, &resources);
    lap("fields, pages, pivots", &mut lap_at);
    let count = presentation.slides.len();
    // Without `--slides`, the slides PowerPoint prints: the hidden ones are left out (their numbers
    // still count). A deck of hidden slides only comes out whole.
    let numbers: Vec<usize> = if options.slides.is_empty() {
        let shown: Vec<usize> = (1..=count).filter(|&n| presentation.flow.is_some() || presentation.workbook.is_some() || !presentation.slides[n - 1].hidden).collect();
        if shown.is_empty() { (1..=count).collect() } else { shown }
    } else {
        options.slides.clone()
    };
    if let Some(n) = numbers.iter().find(|n| **n > count) {
        return Err(format!("Slide {n} does not exist ({count} slides in total)"));
    }

    // The pictures drawn through SVG (rotated, cut to shapes, recoloured),
    // rasterized ahead on several threads: making the pictures' and shapes'
    // elements makes them.
    for &number in &numbers {
        let slide = &presentation.slides[number - 1];
        let _ = slide_backdrop(slide, 1.0, &resources);
        for shape in slide.shapes.iter().filter(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_) | ibis_doc_model::ShapeKind::Auto(_))) {
            let _ = shape_element(shape, shape.frame, 1.0, &resources, None);
        }
    }
    cx.update(|app| resources.rasterize_svg_pictures(&app.svg_renderer()));

    // The renderer draws at the display's pixel density; size the window so
    // pictures come out `options.width` pixels wide.
    let density = pixel_density(&mut cx)?;
    crate::docx::art_border::set_device_scale(density);
    let width = options.width.max(16) as f32 / density;

    lap("pictures, density", &mut lap_at);
    let mut pages = Vec::new();
    let mut vector = (options.format == Format::Pdf).then(crate::pdf_vector::PdfWriter::new);
    crate::render::chart_view::VECTOR_EXPORT.with(|v| v.set(vector.is_some()));
    // The window a vector PDF's pages are drawn in, and its size.
    let mut page_window: Option<(WindowHandle<SlidePicture>, Size<Pixels>)> = None;
    // The pages the sheets printed make (`&P` and `&N` count through all of them).
    let sheet_page_counts: Vec<usize> = if vector.is_some() {
        numbers
            .iter()
            .map(|&number| {
                let mut slide = presentation.slides[number - 1].clone();
                black_and_white(&mut slide);
                print_layout(&presentation, &mut slide);
                let area = export_area(&presentation, &slide);
                let titles = print_titles(&presentation, &slide);
                let default_style = presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
                let headings = slide.shapes.iter().find_map(|s| match &s.kind {
                    ibis_doc_model::ShapeKind::Sheet(sheet) => Some(crate::xlsx::header_footer::heading_size(sheet, &default_style, &resources)),
                    _ => None,
                }).unwrap_or((0.0, 0.0));
                sheet_pages_with_text(&slide, area, titles, headings, sheet_text.get(&slide.id).map(Vec::as_slice).unwrap_or(&[])).map_or(0, |(_, _, p)| p.len())
            })
            .collect()
    } else {
        Vec::new()
    };
    let total_sheet_pages: usize = sheet_page_counts.iter().sum();
    let mut sheet_pages_before = 0usize;
    for (position, &number) in numbers.iter().enumerate() {
        let mut slide = presentation.slides[number - 1].clone();
        black_and_white(&mut slide);
                print_layout(&presentation, &mut slide);
        let area = export_area(&presentation, &slide);
        // The scale the sheet prints at, for its dashed borders.
        let titles = print_titles(&presentation, &slide);
        let default_style = presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
        let headings = slide.shapes.iter().find_map(|s| match &s.kind {
            ibis_doc_model::ShapeKind::Sheet(sheet) => Some(crate::xlsx::header_footer::heading_size(sheet, &default_style, &resources)),
            _ => None,
        }).unwrap_or((0.0, 0.0));
        resources.print_scale.set(sheet_pages_with_text(&slide, area, titles, headings, sheet_text.get(&slide.id).map(Vec::as_slice).unwrap_or(&[])).map_or(1.0, |(_, scale, _)| scale));
        {
            // Where the pages of the sheet start, in the sheet's own EMU.
            let frame = slide.shapes.iter().find_map(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Sheet(_)).then_some(s.frame));
            let breaks = match (frame, sheet_pages_with_text(&slide, area, titles, headings, sheet_text.get(&slide.id).map(Vec::as_slice).unwrap_or(&[]))) {
                (Some(frame), Some((_, _, pages))) => (pages.iter().map(|p| p.y - frame.y).collect(), pages.iter().map(|p| p.x - frame.x).collect()),
                _ => Default::default(),
            };
            *resources.print_breaks.borrow_mut() = Arc::new(breaks);
        }
        // A worksheet in a PDF comes out on its paper, a page at a time.
        if let Some(writer) = &mut vector
            && let Some((print, scale, sheet_pages)) = sheet_pages_with_text(&slide, area, titles, headings, sheet_text.get(&slide.id).map(Vec::as_slice).unwrap_or(&[]))
        {
            print_sheet(&mut cx, writer, slide, &print, scale, &sheet_pages, titles, headings, density, &resources, &default_style, options.file_name.as_deref().unwrap_or(""), (sheet_pages_before, total_sheet_pages))?;
            sheet_pages_before += sheet_page_counts.get(position).copied().unwrap_or(0);
            continue;
        }
        // The renderer's textures reach 16 384 pixels; a longer sheet comes
        // out narrower.
        let scale = (width / area.width as f32).min(16_000.0 / density / area.height as f32);
        let window_size = gpui_kit::size(px((area.width as f32 * scale).round()), px((area.height as f32 * scale).round()));
        let origin = (area.x as f32 * scale, area.y as f32 * scale);
        if let Some(writer) = &mut vector {
            // One window for the pages of a size (making one takes a
            // renderer), the page changed in it, which draws it.
            let mut media = Vec::new();
            crate::render::preload::slide_media(&slide, &mut media);
            let reused = page_window.as_ref().filter(|(_, size)| *size == window_size).map(|(window, _)| *window);
            let window = match reused {
                Some(window) => {
                    cx.update(|app| {
                        window.update(app, |picture, _, cx| {
                            (picture.slide, picture.scale, picture.origin) = (slide, scale, origin);
                            cx.notify();
                        })
                    })
                    .map_err(|e| e.to_string())?;
                    window
                }
                None => {
                    if let Some((old, _)) = page_window.take() {
                        cx.update_window(old.into(), |_, window, _| window.remove_window()).ok();
                    }
                    let resources = resources.clone();
                    let window = cx
                        .open_window(window_size, |_, app| app.new(|_| SlidePicture { slide, scale, size: window_size, origin, inset: (0.0, 0.0), view: window_size, resources, marks: Vec::new(), extra: Vec::new(), rules: Vec::new(), shape_positions: Rc::new(RefCell::new(Vec::new())) }))
                        .map_err(|e| format!("Cannot create the window for drawing: {e}"))?;
                    cx.update_window(window.into(), |_, window, _| {
                        window.record_paints();
                        // Only recorded: the PDF is made of the record.
                        window.record_paints_only(true);
                    })
                    .map_err(|e| e.to_string())?;
                    page_window = Some((window, window_size));
                    window
                }
            };
            // A page with pictures is drawn until those GPUI loads itself
            // (turned and recoloured ones) have arrived; one without is
            // drawn once.
            let rounds = if media.is_empty() && reused.is_some() { 0 } else { 3 };
            for _ in 0..rounds {
                cx.run_until_parked();
                cx.update_window(window.into(), |_, window, _| window.refresh()).map_err(|e| e.to_string())?;
            }
            cx.run_until_parked();
            let records = cx
                .update_window(window.into(), |_, window, _| {
                    let records = window.take_recorded_paints();
                    // Recording goes on for the next page.
                    window.record_paints();
                    records
                })
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            let text_system = cx.text_system().clone();
            let page_pt = (area.width as f32 / 12_700.0, area.height as f32 / 12_700.0);
            writer.add_page(records, f32::from(window_size.width), page_pt, density, &mut |id| (text_system.font_data(id), text_system.synthetic_style(id), text_system.font_variations(id)));
            continue;
        }
        let resources = resources.clone();
        let shape_positions = Rc::new(RefCell::new(Vec::new()));
        let window = cx
            .open_window(window_size, |_, app| app.new(|_| SlidePicture { slide, scale, size: window_size, origin, inset: (0.0, 0.0), view: window_size, resources, marks: Vec::new(), extra: Vec::new(), rules: Vec::new(), shape_positions: shape_positions.clone() }))
            .map_err(|e| format!("Cannot create the window for drawing: {e}"))?;
        // Pictures decode asynchronously; draw until they have all arrived.
        let mut image = None;
        for _ in 0..4 {
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, _| window.refresh()).map_err(|e| e.to_string())?;
            cx.run_until_parked();
            image = Some(cx.capture_screenshot(window.into()).map_err(|e| format!("Cannot draw slide {number}: {e}"))?);
        }
        let mut image = image.expect("at least one capture");
        crate::export_print::recolor(&mut image, options.color);
        if options.frame && options.handout.is_none() && !options.notes_pages && !options.outline_pages {
            crate::export_print::frame(&mut image);
        }
        cx.update_window(window.into(), |_, window, _| window.remove_window()).ok();
        // if image.width() > options.width {
        //     image = image::imageops::crop_imm(&image, 0, 0, options.width, image.height()).to_image();
        // }

        match options.format {
            Format::Png => {
                let path = options.out_dir.join(format!("{}-{number:02}.png", options.name));
                image.save(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                println!("{}", path.display());
                if options.layout {
                    let px_per_emu = image.width() as f32 / area.width as f32;
                    let slide = &presentation.slides[number - 1];
                    let mut measured_bounds = HashMap::new();
                    let positions_list = shape_positions.borrow();
                    for (shape_id, pos) in positions_list.iter() {
                        if let Some(shape) = slide.shapes.iter().find(|s| s.id == *shape_id) {
                            if let Some(bounds) = compute_text_bounds_from_positions(pos, shape.frame, px_per_emu, density, (0.0, 0.0), (area.x, area.y)) {
                                measured_bounds.insert(*shape_id, bounds);
                            }
                        }
                    }
                    let path = options.out_dir.join(format!("{}-{number:02}.layout.tsv", options.name));
                    std::fs::write(&path, layout_tsv(slide, px_per_emu, (area.x, area.y), Some(&measured_bounds))).map_err(|e| format!("{}: {e}", path.display()))?;
                    // The size of the area drawn, in points (a sheet's print
                    // area), for matching the picture to a printed page.
                    let path = options.out_dir.join(format!("{}-{number:02}.area.txt", options.name));
                    let points = format!("{} {}\n", area.width as f32 / 12_700.0, area.height as f32 / 12_700.0);
                    std::fs::write(&path, points).map_err(|e| format!("{}: {e}", path.display()))?;
                }
            }
            Format::PdfImage => pages.push((image, (area.width as f32 / 12_700.0, area.height as f32 / 12_700.0))),
            Format::Pdf => {}
        }
    }
    lap("all pages", &mut lap_at);
    if let Some((window, _)) = page_window {
        cx.update_window(window.into(), |_, window, _| window.remove_window()).ok();
    }
    if let Some(writer) = vector {
        crate::render::chart_view::VECTOR_EXPORT.with(|v| v.set(false));
        if writer.missing_glyphs > 0 {
            eprintln!("ibisOffice: could not get the font data, so {} characters could not be written", writer.missing_glyphs);
        }
        let path = options.out_dir.join(format!("{}.pdf", options.name));
        let (pdf, outlined) = writer.finish()?;
        lap("PDF written (finish)", &mut lap_at);
        if !outlined.is_empty() {
            eprintln!("ibisOffice: characters of fonts that cannot be subsetted were written as outlines (they cannot be selected as text): {}", outlined.join(", "));
        }
        std::fs::write(&path, pdf).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("{}", path.display());
    }
    if options.format == Format::PdfImage {
        let path = options.out_dir.join(format!("{}.pdf", options.name));
        // ノート ページ: each slide's picture above its notes.
        if options.outline_pages {
            // アウトライン: no pictures, the text of the slides.
            let lines: Vec<crate::export_print::NotePara> = numbers
                .iter()
                .flat_map(|n| {
                    use crate::export_print::{NotePara, NoteRun};
                    let (title, body) = crate::pptx::outline_edit::outline_of(&presentation.slides[n - 1]);
                    let head = NotePara { level: 0, bullet: None, runs: vec![NoteRun { text: format!("{n}  "), size_pt: Some(12.0), ..Default::default() }, NoteRun { text: title, bold: true, size_pt: Some(14.0), ..Default::default() }] };
                    std::iter::once(head).chain(body.into_iter().map(|(level, text)| NotePara { level: level.saturating_add(1), bullet: Some("•".to_string()), runs: vec![NoteRun { text, size_pt: Some(12.0), ..Default::default() }] }))
                })
                .collect();
            pages = crate::export_print::outline_pages(&crate::export_print::NoteText { paragraphs: lines }, &options.handout_marks);
        } else if options.notes_pages {
            let images: Vec<image::RgbaImage> = pages.into_iter().map(|(image, _)| image).collect();
            let notes: Vec<crate::export_print::NoteText> = numbers.iter().map(|n| rich_notes(options.notes_source.as_deref(), n - 1, &presentation.slides[n - 1].notes)).collect();
            pages = crate::export_print::notes_pages(&images, &notes, &options.handout_marks);
        } else if let Some(per_page) = options.handout {
            // 配布資料: the slides' pictures put on pages.
            let images: Vec<image::RgbaImage> = pages.into_iter().map(|(image, _)| image).collect();
            pages = crate::export_print::handouts(&images, per_page, options.frame, options.handout_landscape, &options.handout_marks);
        }
        write_pdf(&path, &pages)?;
        println!("{}", path.display());
    }
    Ok(())
}

/// Add the pages of a worksheet to `writer`: each `pages` part of the
/// sheet, at `scale`, on the paper and within the margins of `print`.
#[allow(clippy::too_many_arguments)]
fn print_sheet(
    cx: &mut HeadlessAppContext,
    writer: &mut crate::pdf_vector::PdfWriter,
    slide: Slide,
    print: &SheetPrint,
    scale: f64,
    pages: &[EmuRect],
    titles: Titles,
    headings: (f64, f64),
    density: f32,
    resources: &Rc<Resources>,
    default_style: &ibis_doc_model::CellStyle,
    file_name: &str,
    // The pages before this sheet's and all the sheets' pages (for `&P` and `&N`).
    (pages_before, pages_in_all): (usize, usize),
) -> Result<(), String> {
    // A pixel a point: the PDF is vector graphics, so the size drawn at
    // matters only for layout (and drawing larger takes longer).
    const PX_PER_EMU: f64 = 1.0 / 12_700.0;
    let (paper_w, paper_h) = print.page_size();
    let [left, right, top, bottom] = print.margins;
    let window_size = gpui_kit::size(px((paper_w as f64 * PX_PER_EMU) as f32), px((paper_h as f64 * PX_PER_EMU) as f32));
    let px_per_emu = (scale * PX_PER_EMU) as f32;
    let resources = resources.clone();
    let sheet_name = slide.name.clone().unwrap_or_default();
    // Where the sheet's charts are, to cut them out of its pages
    // (`IBIS_CHART_RECTS=<file>`: for comparing charts with Excel's PDF).
    let chart_frames: Vec<(EmuRect, [usize; 4])> = std::env::var_os("IBIS_CHART_RECTS")
        .map(|_| {
            let cells = slide.shapes.iter().find_map(|s| match &s.kind {
                ibis_doc_model::ShapeKind::Sheet(sheet) => Some((s.frame, sheet.column_offsets(), sheet.row_offsets())),
                _ => None,
            });
            // The first and last column and row (from 0) a frame covers.
            let index = |edges: &[i64], start: i64, at: i64| edges.partition_point(|e| start + *e <= at).saturating_sub(1);
            slide
                .shapes
                .iter()
                .filter(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_)))
                .map(|s| {
                    let f = s.frame;
                    let c = cells.as_ref().map_or([0; 4], |(o, xs, ys)| {
                        [index(xs, o.x, f.x), index(ys, o.y, f.y), index(xs, o.x, f.x + f.width - 1), index(ys, o.y, f.y + f.height - 1)]
                    });
                    (f, c)
                })
                .collect()
        })
        .unwrap_or_default();
    let window = cx
        .open_window(window_size, |_, app| {
            app.new(|_| SlidePicture { slide, scale: px_per_emu, size: window_size, origin: (0.0, 0.0), inset: (0.0, 0.0), view: window_size, resources, marks: Vec::new(), extra: Vec::new(), rules: Vec::new(), shape_positions: Rc::new(RefCell::new(Vec::new())) })
        })
        .map_err(|e| format!("Cannot create the window for drawing: {e}"))?;
    cx.update_window(window.into(), |_, window, _| {
                        window.record_paints();
                        // Only recorded: the PDF is made of the record.
                        window.record_paints_only(true);
                    })
                    .map_err(|e| e.to_string())?;
    let page_pt = (paper_w as f32 / 12_700.0, paper_h as f32 / 12_700.0);
    let (mut drawing, mut adding) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
    for (i, page) in pages.iter().enumerate() {
        let page_started = crate::clock::Instant::now();
        // The title rows over a page past them, the title columns left of one.
        let title_rows = titles.0.filter(|(_, end)| page.y >= *end);
        let title_columns = titles.1.filter(|(_, end)| page.x >= *end);
        let (tw, th) = (title_columns.map_or(0, |(a, b)| b - a), title_rows.map_or(0, |(a, b)| b - a));
        let view = (page.width as f64 * scale, page.height as f64 * scale);
        // (A page of one over-wide column is cut at the paper's printable edge.)
        let view = (view.0.min((paper_w - left - right) as f64), view.1.min((paper_h - top - bottom) as f64));
        let (more_columns, more_rows) = (pages.iter().any(|q| q.x >= page.x + page.width), pages.iter().any(|q| q.y >= page.y + page.height));
        let whole = ((page.width + tw) as f64 * scale, (page.height + th) as f64 * scale);
        let room = ((paper_w - left - right) as f64, (paper_h - top - bottom) as f64);
        // A sheet laid out for Letter is drawn from the left margin and in the middle of the printable height of the A4 paper:
        // the box of Letter's printable height at 0.95 (684 pt of 720 pt in Excel's PDF of the letter-a4 probes, top at 77.9 pt of
        // A4's printable 36..805.9 pt; the page's own offset within 1 pt).
        let letter_top = if print.letter_on_a4 {
            let letter_room = ibis_doc_model::EMU_PER_INCH * 11 - top - bottom;
            ((room.1 - letter_room as f64 * LETTER_ON_A4) / 2.0).max(0.0)
        } else {
            0.0
        };
        let corner_at = (
            left as f64 + if print.centered.0 { ((room.0 - whole.0) / 2.0).max(0.0) } else { 0.0 },
            top as f64 + letter_top + if print.centered.1 { ((room.1 - whole.1) / 2.0).max(0.0) } else { 0.0 },
        );
        // Printed headings: the cells move right of the row numbers and
        // under the column letters.
        let (hw, hh) = if print.headings { (headings.0 * 12_700.0 * scale, headings.1 * 12_700.0 * scale) } else { (0.0, 0.0) };
        let corner_at = (corner_at.0 + hw, corner_at.1 + hh);
        let inset = (corner_at.0 + tw as f64 * scale, corner_at.1 + th as f64 * scale);
        let p = |v: f64| (v * PX_PER_EMU) as f32;
        let mut extra = Vec::new();
        if let Some((y0, _)) = title_rows {
            extra.push(((page.x as f32 * px_per_emu, y0 as f32 * px_per_emu), (p(inset.0), p(corner_at.1)), gpui_kit::size(px(p(view.0)), px(p(th as f64 * scale)))));
        }
        if let Some((x0, _)) = title_columns {
            extra.push(((x0 as f32 * px_per_emu, page.y as f32 * px_per_emu), (p(corner_at.0), p(inset.1)), gpui_kit::size(px(p(tw as f64 * scale)), px(p(view.1)))));
        }
        if let (Some((x0, _)), Some((y0, _))) = (title_columns, title_rows) {
            extra.push(((x0 as f32 * px_per_emu, y0 as f32 * px_per_emu), (p(corner_at.0), p(corner_at.1)), gpui_kit::size(px(p(tw as f64 * scale)), px(p(th as f64 * scale)))));
        }
        cx.update(|app| {
            window.update(app, |picture, _, cx| {
                picture.extra = extra;
                picture.origin = (page.x as f32 * px_per_emu, page.y as f32 * px_per_emu);
                picture.inset = ((inset.0 * PX_PER_EMU) as f32, (inset.1 * PX_PER_EMU) as f32);
                // The next page's first row or column must not show in this one (its fill lies
                // a hundredth of a point inside the clip once the corner has been put on a whole point).
                let (cut_x, cut_y) = (if more_columns { 0.25 } else { 0.0 }, if more_rows { 0.25 } else { 0.0 });
                picture.view = gpui_kit::size(px((view.0 * PX_PER_EMU) as f32 - cut_x), px((view.1 * PX_PER_EMU) as f32 - cut_y));
                picture.marks = crate::xlsx::header_footer::page_marks(print, default_style, scale as f32, (pages_before + i) as u32 + 1, pages_in_all.max(pages_before + pages.len()) as u32, &sheet_name, file_name, &picture.resources);
                picture.rules.clear();
                if print.headings && let Some(sheet) = picture.slide.shapes.iter().find_map(|s| match &s.kind {
                    ibis_doc_model::ShapeKind::Sheet(sheet) => Some((s.frame, sheet.clone())),
                    _ => None,
                }) {
                    let (frame, sheet) = sheet;
                    // The columns and rows shown, title ones first (EMU spans of the slide).
                    let spans = |offsets: Vec<i64>, start: i64, title: Option<(i64, i64)>, from: i64, to: i64| -> Vec<(u32, i64, i64)> {
                        let mut out = Vec::new();
                        for (lo, hi) in title.into_iter().chain(std::iter::once((from, to))) {
                            for (k, w) in offsets.windows(2).enumerate() {
                                let (a, b) = (start + w[0], start + w[1]);
                                if b > lo && a < hi && b > a {
                                    out.push((k as u32, a.max(lo), b.min(hi)));
                                }
                            }
                        }
                        out
                    };
                    let cols = spans(sheet.column_offsets(), frame.x, title_columns, page.x, page.x + page.width);
                    let rows = spans(sheet.row_offsets(), frame.y, title_rows, page.y, page.y + page.height);
                    let (x0, y0) = (p(corner_at.0 - hw), p(corner_at.1 - hh));
                    let (hwp, hhp) = (p(hw), p(hh));
                    let width: f32 = cols.iter().map(|c| (c.2 - c.1) as f32 * px_per_emu).sum();
                    let height: f32 = rows.iter().map(|r| (r.2 - r.1) as f32 * px_per_emu).sum();
                    let (right, bottom) = (x0 + hwp + width, y0 + hhp + height);
                    // The frame: round it all, and between the headings and the cells.
                    picture.rules.extend([
                        (x0 - 1.0, y0 - 1.0, right - x0 + 1.0, 1.0),
                        (x0 - 1.0, y0 - 1.0, 1.0, bottom - y0 + 1.0),
                        (x0 - 1.0, bottom - 1.0, right - x0 + 1.0, 1.0),
                        (right - 1.0, y0 - 1.0, 1.0, bottom - y0 + 1.0),
                        (x0 + hwp - 1.0, y0 + hhp - 1.0, 1.0, bottom - y0 - hhp + 1.0),
                        (x0 + hwp - 1.0, y0 + hhp - 1.0, right - x0 - hwp + 1.0, 1.0),
                    ]);
                    let labels = crate::xlsx::header_footer::heading_marks(&cols, &rows, (x0, y0), (hwp, hhp), px_per_emu, default_style, scale as f32, &picture.resources);
                    picture.marks.extend(labels);
                }
                cx.notify();
            })
        })
        .map_err(|e| e.to_string())?;
        // Changing the view draws the page; the first waits for the
        // pictures GPUI loads itself.
        for _ in 0..if i == 0 { 3 } else { 0 } {
            cx.run_until_parked();
            cx.update_window(window.into(), |_, window, _| window.refresh()).map_err(|e| e.to_string())?;
            cx.run_until_parked();
        }
        let records = cx.update_window(window.into(), |_, window, _| window.take_recorded_paints()).map_err(|e| e.to_string())?.unwrap_or_default();
        // Recording goes on for the next page.
        cx.update_window(window.into(), |_, window, _| {
                        window.record_paints();
                        // Only recorded: the PDF is made of the record.
                        window.record_paints_only(true);
                    })
                    .map_err(|e| e.to_string())?;
        drawing += page_started.elapsed();
        let adding_started = crate::clock::Instant::now();
        let text_system = cx.text_system().clone();
        // Excel puts the sheet's corner on a whole point of the sheet at
        // or inside the margin rounded up to a point (29 pt of a 0.4 inch
        // margin: at 87 % 33 points of the sheet, 28.71 pt of paper; its
        // PDFs of xlsx-XALN and XBRD at 73 to 100 %); the window put it on
        // a whole pixel. Not when centred (not measured).
        let corner = |margin: i64, centred: bool, at: f64| -> f32 {
            if centred {
                return 0.0;
            }
            let want = ((margin as f64 / 12_700.0 - 1e-6).ceil() / scale + 1e-6).floor() * scale;
            (want - at.round()) as f32
        };
        let shift = (corner(left, print.centered.0, (corner_at.0 - hw) * PX_PER_EMU), corner(top, print.centered.1 || print.letter_on_a4, (corner_at.1 - hh) * PX_PER_EMU));
        if let Some(path) = std::env::var_os("IBIS_CHART_RECTS") {
            use std::io::Write as _;
            // `sheet  page  x y width height` of each chart on this page, in
            // points from the page's top left.
            let mut text = String::new();
            for (frame, cells) in &chart_frames {
                let intersects = frame.x < page.x + page.width && frame.x + frame.width > page.x && frame.y < page.y + page.height && frame.y + frame.height > page.y;
                if !intersects {
                    continue;
                }
                let to_pt = |v: i64| v as f64 * scale / 12_700.0;
                let x = inset.0 / 12_700.0 + to_pt(frame.x - page.x) + shift.0 as f64;
                let y = inset.1 / 12_700.0 + to_pt(frame.y - page.y) + shift.1 as f64;
                text.push_str(&format!("{sheet_name}\t{}\t{x:.2}\t{y:.2}\t{:.2}\t{:.2}\t{}\t{}\t{}\t{}\n", writer.page_count() + 1, to_pt(frame.width), to_pt(frame.height), cells[0], cells[1], cells[2], cells[3]));
            }
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                file.write_all(text.as_bytes()).ok();
            }
        }
        writer.add_page_shifted(records, f32::from(window_size.width), page_pt, density, shift, &mut |id| (text_system.font_data(id), text_system.synthetic_style(id), text_system.font_variations(id)));
        adding += adding_started.elapsed();
    }
    if std::env::var_os("IBIS_DEBUG_TIMING").is_some() {
        eprintln!("export: sheet {sheet_name:?} {} pages: draw {:.0} ms, add to PDF {:.0} ms", pages.len(), drawing.as_secs_f64() * 1000.0, adding.as_secs_f64() * 1000.0);
    }
    cx.update_window(window.into(), |_, window, _| window.remove_window()).ok();
    Ok(())
}

/// Compute rendered text bounds from captured TextPositions.
pub(crate) fn compute_text_bounds_from_positions(
    positions: &crate::render::rich_text::TextPositions,
    frame: EmuRect,
    px_per_emu: f32,
    density: f32,
    slide_origin_px: (f32, f32),
    origin_emu: (i64, i64),
) -> Option<TextBounds> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut has_chars = false;

    let paragraphs = positions.paragraphs.borrow();
    for para in paragraphs.iter() {
        let p = para.borrow();
        for line in &p.lines {
            let start = line.start;
            let end = line.end.min(p.chars.len());
            if start < end {
                let left = p.chars[start].x;
                let right = p.chars[end - 1].x + p.chars[end - 1].advance;
                if right > left {
                    min_x = min_x.min(left);
                    max_x = max_x.max(right);
                    min_y = min_y.min(line.top);
                    max_y = max_y.max(line.top + line.height);
                    has_chars = true;
                }
            }
        }
    }

    if !has_chars {
        return None;
    }

    let text_x = ((min_x - slide_origin_px.0) * density).round() as i64;
    let text_y = ((min_y - slide_origin_px.1) * density).round() as i64;
    let text_width = ((max_x - min_x) * density).round().max(0.0) as i64;
    let text_height = ((max_y - min_y) * density).round().max(0.0) as i64;

    let px = |v: i64| (v as f32 * px_per_emu).round() as i64;
    let frame_x = px(frame.x - origin_emu.0);
    let frame_y = px(frame.y - origin_emu.1);
    let frame_w = px(frame.width);
    let frame_h = px(frame.height);

    let eps = 1;
    let overflow = text_x < frame_x - eps
        || text_y < frame_y - eps
        || text_x + text_width > frame_x + frame_w + eps
        || text_y + text_height > frame_y + frame_h + eps;

    Some(TextBounds { text_x, text_y, text_width, text_height, overflow })
}

/// Fallback text bounds estimation from model metadata when rendering positions are unavailable.
pub(crate) fn estimate_text_bounds(
    shape: &ibis_doc_model::Shape,
    px_per_emu: f32,
    origin_emu: (i64, i64),
) -> Option<TextBounds> {
    let body = shape.text_body()?;
    if body.is_empty() {
        return None;
    }
    let text = body.text();
    if text.trim().is_empty() {
        return None;
    }

    let f = shape.frame;
    let px = |v: i64| (v as f32 * px_per_emu).round() as i64;
    let frame_x = px(f.x - origin_emu.0);
    let frame_y = px(f.y - origin_emu.1);
    let frame_w = px(f.width);
    let frame_h = px(f.height);

    let inset_l = (body.insets.left as f32 * px_per_emu).round() as i64;
    let inset_t = (body.insets.top as f32 * px_per_emu).round() as i64;
    let inset_r = (body.insets.right as f32 * px_per_emu).round() as i64;
    let _inset_b = (body.insets.bottom as f32 * px_per_emu).round() as i64;

    let avail_w = (frame_w - inset_l - inset_r).max(1) as f32;
    let px_per_point = px_per_emu * 12_700.0;
    let font_scale = body.font_scale as f32 / 100_000.0;
    let default_fs = body.style.font_size_pt();

    let mut total_h = 0.0f32;
    let mut max_w = 0.0f32;

    for (i, p) in body.paragraphs.iter().enumerate() {
        if i > 0 {
            total_h += p.space_before as f32 * px_per_point / 100.0;
        }
        let font_size_pt = p
            .runs
            .first()
            .and_then(|r| r.style.font_size)
            .or(p.end_style.font_size)
            .map_or(default_fs, |sz| sz as f32 / 100.0)
            * font_scale;
        let line_height_px = match p.line_spacing {
            Some(ibis_doc_model::LineSpacing::Percent(pct)) => font_size_pt * px_per_point * 1.2 * (pct as f32 / 100_000.0),
            Some(ibis_doc_model::LineSpacing::Points(pts)) => pts as f32 / 100.0 * px_per_point,
            None => font_size_pt * px_per_point * 1.2,
        };

        let mut para_text_w = 0.0f32;
        for run in &p.runs {
            let run_fs = run.style.font_size.map_or(default_fs, |sz| sz as f32 / 100.0) * font_scale;
            for c in run.text.chars() {
                let em = if c.is_ascii() { 0.55 } else { 1.0 };
                para_text_w += em * run_fs * px_per_point;
            }
        }

        if body.wrap && avail_w > 0.0 && para_text_w > avail_w {
            let num_lines = (para_text_w / avail_w).ceil();
            total_h += num_lines * line_height_px;
            max_w = max_w.max(avail_w);
        } else {
            total_h += line_height_px;
            max_w = max_w.max(para_text_w);
        }

        if i + 1 < body.paragraphs.len() {
            total_h += p.space_after as f32 * px_per_point / 100.0;
        }
    }

    let text_w = max_w.round() as i64;
    let text_h = total_h.round() as i64;

    let text_x = frame_x + inset_l;
    let text_y = frame_y + inset_t;

    let eps = 1;
    let overflow = text_x < frame_x - eps
        || text_y < frame_y - eps
        || text_x + text_w > frame_x + frame_w + eps
        || text_y + text_h > frame_y + frame_h + eps;

    Some(TextBounds {
        text_x,
        text_y,
        text_width: text_w,
        text_height: text_h,
        overflow,
    })
}

/// What each element of `slide` is and where it is drawn, back to front, in
/// pixels of the exported picture.
///
/// Columns: `z layer kind x y width height text_x text_y text_width text_height overflow detail`.
pub(crate) fn layout_tsv(
    slide: &Slide,
    px_per_emu: f32,
    origin: (i64, i64),
    measured_bounds: Option<&HashMap<ibis_doc_model::ShapeId, TextBounds>>,
) -> String {
    use ibis_doc_model::{Geometry, Placeholder, ShapeKind};
    let mut out = String::from("z\tlayer\tkind\tx\ty\twidth\theight\ttext_x\ttext_y\ttext_width\ttext_height\toverflow\tdetail\n");
    let shapes = slide.shown_backdrop().iter().map(|s| ("backdrop", s)).chain(slide.shown_shapes().filter(|s| slide.shape_prints.get(&s.id).copied().unwrap_or(true)).map(|s| ("slide", s)));
    for (z, (layer, shape)) in shapes.enumerate() {
        let has_text = shape.text_body().is_some_and(|b| !b.is_empty());
        let (kind, detail) = match &shape.kind {
            _ if has_text => ("text", shape.text_body().map(|b| b.text().chars().take(40).collect::<String>()).unwrap_or_default()),
            ShapeKind::TextBox(_) => ("text", String::new()),
            ShapeKind::Auto(a) => match &a.geometry {
                _ if matches!(a.fill, ibis_doc_model::Fill::Image(_) | ibis_doc_model::Fill::TileImage { .. } | ibis_doc_model::Fill::Blip(_)) => ("picture", "図形の画像塗り".into()), // i18n-ok（layout TSV の書き出し。試験・確認用のファイルに入り、画面には出ない）
                Geometry::Formula(g) => ("shape", g.preset.clone().unwrap_or_else(|| "custom".into())),
                _ => ("shape", String::new()),
            },
            ShapeKind::Picture(_) if shape.rotation.rem_euclid(21_600_000) != 0 => {
                ("picture", format!("回転 {:.0}°", shape.rotation as f32 / 60_000.0)) // i18n-ok（layout TSV の書き出し。試験・確認用のファイルに入り、画面には出ない）
            }
            ShapeKind::Picture(_) => ("picture", String::new()),
            ShapeKind::Line(_) => ("line", String::new()),
            ShapeKind::Chart(_) => ("chart", String::new()),
            ShapeKind::Table(_) => ("table", String::new()),
            ShapeKind::Sheet(_) => ("sheet", String::new()),
            ShapeKind::Page { .. } => ("page", String::new()),
            ShapeKind::Slicer(s) => ("slicer", s.name.clone()),
            ShapeKind::Control(c) => ("control", c.name.clone()),
            ShapeKind::Timeline(t) => ("timeline", t.name.clone()),
            ShapeKind::Placeholder(Placeholder::Table) => ("table", String::new()),
            ShapeKind::Placeholder(Placeholder::Chart) => ("chart-unread", String::new()),
            ShapeKind::Placeholder(Placeholder::Diagram) => ("smartart", String::new()),
            ShapeKind::Placeholder(Placeholder::Picture(format)) => ("picture-undrawable", format!("{format:?}")),
        };
        let f = shape.frame;
        let px = |v: i64| (v as f32 * px_per_emu).round() as i64;
        let frame_x = px(f.x - origin.0);
        let frame_y = px(f.y - origin.1);
        let frame_w = px(f.width);
        let frame_h = px(f.height);
        let detail = detail.replace(['\t', '\n'], " ");

        let tb = measured_bounds.and_then(|m| m.get(&shape.id).copied()).or_else(|| {
            if has_text {
                estimate_text_bounds(shape, px_per_emu, origin)
            } else {
                None
            }
        });

        let (tx, ty, tw, th, ov) = match tb {
            Some(b) => (
                format!("{}", b.text_x),
                format!("{}", b.text_y),
                format!("{}", b.text_width),
                format!("{}", b.text_height),
                if b.overflow { "true" } else { "false" },
            ),
            None => (
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "0".to_string(),
                "false",
            ),
        };

        out.push_str(&format!("{z}\t{layer}\t{kind}\t{frame_x}\t{frame_y}\t{frame_w}\t{frame_h}\t{tx}\t{ty}\t{tw}\t{th}\t{ov}\t{detail}\n"));
    }
    out
}

/// Device pixels per logical pixel of the headless renderer: a headless
/// window's scale factor, which its pictures are drawn at.
fn pixel_density(cx: &mut HeadlessAppContext) -> Result<f32, String> {
    struct Blank;
    impl Render for Blank {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full()
        }
    }
    let probe = gpui_kit::size(px(100.), px(100.));
    let window = cx.open_window(probe, |_, app| app.new(|_| Blank)).map_err(|e| e.to_string())?;
    let density = cx.update_window(window.into(), |_, window, _| window.scale_factor()).map_err(|e| e.to_string())?;
    cx.update_window(window.into(), |_, window, _| window.remove_window()).ok();
    Ok(density.max(0.25))
}

/// A PDF with one full-page picture per page, each page's size (in points)
/// given with its picture.
fn write_pdf(path: &Path, pages: &[(image::RgbaImage, (f32, f32))]) -> Result<(), String> {
    let mut pdf: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    // Objects: 1 catalog, 2 page tree, then per page: page, contents, image.
    let page_ids: Vec<usize> = (0..pages.len()).map(|i| 3 + i * 3).collect();
    let mut object = |pdf: &mut Vec<u8>, id: usize, body: &[u8]| {
        if offsets.len() < id {
            offsets.resize(id, 0);
        }
        offsets[id - 1] = pdf.len();
        pdf.extend(format!("{id} 0 obj\n").as_bytes());
        pdf.extend(body);
        pdf.extend(b"\nendobj\n");
    };
    object(&mut pdf, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: String = page_ids.iter().map(|id| format!("{id} 0 R ")).collect();
    object(&mut pdf, 2, format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()).as_bytes());
    for ((page, (pw, ph)), id) in pages.iter().zip(&page_ids) {
        let (contents, picture) = (id + 1, id + 2);
        object(
            &mut pdf,
            *id,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 {picture} 0 R >> >> /Contents {contents} 0 R >>"
            )
            .as_bytes(),
        );
        let draw = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /Im0 Do Q");
        object(&mut pdf, contents, format!("<< /Length {} >>\nstream\n{draw}\nendstream", draw.len()).as_bytes());
        let rgb: Vec<u8> = page.pixels().flat_map(|p| [p[0], p[1], p[2]]).collect();
        let data = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
        let mut body = format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
            page.width(),
            page.height(),
            data.len()
        )
        .into_bytes();
        body.extend(&data);
        body.extend(b"\nendstream");
        object(&mut pdf, picture, &body);
    }
    let xref = pdf.len();
    pdf.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for offset in &offsets {
        pdf.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes());
    std::fs::write(path, pdf).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
#[path = "tests/export_pagination.rs"]
mod pagination_tests;

#[cfg(test)]
mod tests {
    // Not `super::*`: GPUI's `test` attribute would shadow the standard one.
    use super::{page_spans, parse_slides};

    #[test]
    fn slide_lists() {
        assert_eq!(parse_slides("1,3,5-7").unwrap(), vec![1, 3, 5, 6, 7]);
        assert!(parse_slides("0").is_err());
        assert!(parse_slides("4-2").is_err());
        assert!(parse_slides("x").is_err());
    }


    /// The page break preview draws its lines at the edges of these pages: a
    /// manual row break starts a page, and the page before it ends there
    /// (GitHub #22).
    #[test]
    fn preview_pages_break_at_the_manual_breaks() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsx-sales.xlsx");
        let mut presentation = ibis_pptx::read_xlsx(&std::fs::read(path).unwrap()).unwrap();
        let slide = presentation.slides.first_mut().unwrap();
        let mut frame_y = 0;
        let mut break_y = 0;
        for shape in &mut slide.shapes {
            if let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind {
                let sheet = std::sync::Arc::make_mut(sheet);
                sheet.print.row_breaks = vec![3];
                sheet.print.fit_to = None;
                frame_y = shape.frame.y;
                break_y = frame_y + sheet.row_offsets()[3];
            }
        }
        let area = super::export_area(&presentation, &presentation.slides[0]);
        let (_, scale, pages) = super::sheet_pages(&presentation.slides[0], area, (None, None), (0.0, 0.0)).unwrap();
        assert!(scale > 0.0);
        assert!(pages.iter().any(|p| p.y == break_y), "a page starts at the break: {pages:?} / {break_y}");
        assert!(pages.iter().any(|p| p.y + p.height == break_y), "the page before it ends there: {pages:?} / {break_y}");
        assert!(pages.iter().all(|p| p.y >= frame_y));
    }
    #[test]
    fn pages_hold_whole_cells() {
        let edges = [0, 40, 80, 120, 300, 340];
        // As many whole cells as fit; a cell larger than the page gets a page of its own, cut at the page's edge (X3b: the rest is not carried on to the next page).
        assert_eq!(page_spans(&edges, &[], 0, 340, 100.0, None), vec![(0, 80), (80, 120), (120, 300), (300, 340)]);
        // A manual break starts a page.
        assert_eq!(page_spans(&edges, &[40], 0, 120, 100.0, None), vec![(0, 40), (40, 120)]);
        assert_eq!(page_spans(&edges, &[], 0, 120, 500.0, None), vec![(0, 120)]);
        // Title rows 0..20: the pages past them hold 20 less.
        let rows: Vec<i64> = (0..=20).map(|i| i * 20).collect();
        assert_eq!(page_spans(&rows, &[], 0, 400, 100.0, Some((20, 20))), vec![(0, 100), (100, 180), (180, 260), (260, 340), (340, 400)]);
    }

    #[test]
    fn layout_tsv_header_and_columns() {
        use ibis_doc_model::{EmuRect, Shape, ShapeId, Slide, SlideId};

        let mut slide = Slide::new(SlideId(1));
        // Shape 1: Line (no text)
        slide.shapes.push(Shape::new(
            ShapeId(1),
            EmuRect::new(0, 0, 127_000, 127_000),
            ibis_doc_model::ShapeKind::Line(ibis_doc_model::Stroke::solid(ibis_doc_model::Color::BLACK, 10_000)),
        ));
        // Shape 2: Text box fitting within frame
        slide.shapes.push(Shape::text_box(
            ShapeId(2),
            EmuRect::new(127_000, 127_000, 10_000_000, 5_000_000),
            "Fit",
        ));
        // Shape 3: Text box overflowing frame
        slide.shapes.push(Shape::text_box(
            ShapeId(3),
            EmuRect::new(127_000, 127_000, 50_000, 50_000),
            "This text will overflow the small frame definitely",
        ));

        let px_per_emu = 1.0 / 12_700.0; // 1 px per point
        let tsv = super::layout_tsv(&slide, px_per_emu, (0, 0), None);
        let lines: Vec<&str> = tsv.lines().collect();

        // Check header
        assert_eq!(
            lines[0],
            "z\tlayer\tkind\tx\ty\twidth\theight\ttext_x\ttext_y\ttext_width\ttext_height\toverflow\tdetail"
        );

        assert_eq!(lines.len(), 4);

        // Line shape: no text -> text bounds should be 0 0 0 0 false
        let f1: Vec<&str> = lines[1].split('\t').collect();
        assert_eq!(f1.len(), 13);
        assert_eq!(f1[2], "line");
        assert_eq!(f1[7], "0"); // text_x
        assert_eq!(f1[8], "0"); // text_y
        assert_eq!(f1[9], "0"); // text_width
        assert_eq!(f1[10], "0"); // text_height
        assert_eq!(f1[11], "false"); // overflow

        // Fitting text box -> overflow: false
        let f2: Vec<&str> = lines[2].split('\t').collect();
        assert_eq!(f2.len(), 13);
        assert_eq!(f2[2], "text");
        assert_eq!(f2[11], "false");
        let tw2: i64 = f2[9].parse().unwrap();
        let th2: i64 = f2[10].parse().unwrap();
        assert!(tw2 > 0);
        assert!(th2 > 0);

        // Overflowing text box -> overflow: true
        let f3: Vec<&str> = lines[3].split('\t').collect();
        assert_eq!(f3.len(), 13);
        assert_eq!(f3[2], "text");
        assert_eq!(f3[11], "true");
        let tw3: i64 = f3[9].parse().unwrap();
        let th3: i64 = f3[10].parse().unwrap();
        assert!(tw3 > 0);
        assert!(th3 > 0);
    }

    #[test]
    fn layout_tsv_measured_bounds_override() {
        use ibis_doc_model::{EmuRect, Shape, ShapeId, Slide, SlideId};
        use std::collections::HashMap;

        let mut slide = Slide::new(SlideId(1));
        slide.shapes.push(Shape::text_box(
            ShapeId(1),
            EmuRect::new(0, 0, 1_000_000, 1_000_000),
            "Test",
        ));

        let mut measured = HashMap::new();
        measured.insert(
            ShapeId(1),
            super::TextBounds {
                text_x: 12,
                text_y: 34,
                text_width: 56,
                text_height: 78,
                overflow: true,
            },
        );

        let tsv = super::layout_tsv(&slide, 1.0 / 12_700.0, (0, 0), Some(&measured));
        let lines: Vec<&str> = tsv.lines().collect();
        assert_eq!(lines.len(), 2);
        let f: Vec<&str> = lines[1].split('\t').collect();
        assert_eq!(f.len(), 13);
        assert_eq!(f[7], "12");
        assert_eq!(f[8], "34");
        assert_eq!(f[9], "56");
        assert_eq!(f[10], "78");
        assert_eq!(f[11], "true");
    }
}
