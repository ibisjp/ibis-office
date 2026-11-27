// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Exporting slides without a window: numbered PNGs or a PDF.
//!
//!   ibisOffice --export png [--out-dir <dir>] [--width 1920] [--range 1,3-5] <file.pptx>
//!   ibisOffice --export pdf [--out-dir <dir>] [--range …] <file.pptx>
//!   ibisOffice --export pdf-image [--out-dir <dir>] [--width 1920] [--range …] <file.pptx>
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
    /// The paper the pages are printed on (`--print-paper`); `None`: the slide's own size (the notes page's, the handout's).
    pub paper: Option<crate::export_print::Paper>,
    /// With a paper: scale the page to it (`--no-print-fit-paper` puts it at 100 %).
    pub fit_paper: bool,
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
    let Ok(temp) = ibis_formats::notes_page::build(data, index) else { return plain() };
    let Ok(opened) = ibis_formats::open(temp) else { return plain() };
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
                bullet_font: None,
                runs: p
                    .runs
                    .iter()
                    .map(|r| {
                        let style = body.style.with_run(&r.style);
                        let c = style.color.0;
                        NoteRun { text: r.text.clone(), bold: style.bold, size_pt: Some(style.font_size_pt()), color: Some((((c >> 16) & 0xff) as u8, ((c >> 8) & 0xff) as u8, (c & 0xff) as u8)) }
                    })
                    .collect(),
                layout: None,
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
        // (The cells of tables with a link are recorded afresh for this page: `record_table_cells`, set for a PDF.)
        let _ = crate::render::shape_view::take_recorded_table_cells();
        let _ = crate::render::shape_view::take_recorded_text_boxes();
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

#[cfg(any(test, all(target_os = "macos", feature = "scenarios")))]
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
    // (A sheet laid out as Excel for Windows prints it has its columns on whole printer pixels, and the box is 7 of them: its pages take
    // columns up to the width less 7 px, 076's 24 columns of 4008 px on 4015.7 px of room in one page, 132's of 4722 px on 4724.4 px
    // not, and all 145 of the fit probes of `fit_columns`.)
    let whole = *sheet.printed && whole_pixel_columns(slide);
    let room_w = if whole {
        // Excel 365 on a 600 dpi printer (many-pattern probes, 2026-11-14: paper A4, Letter, B5, A5, A3, Legal, portrait and landscape, left margins
        // 0 to 180 pt in fractions of a pixel, scales 10 to 200 %, headers, borders, grid lines, 7 Normal fonts: all 218 probes agree with this
        // rule; the 7 does not change with any of them; why 7 is not known: a measured value the user allowed as an exception, 2026-11-14,
        // docs/rule/implimentationRule.md, tasks/doc/requests/2026-11-14-many-patterns-5-result.md): the page is the paper in whole pixels (rounded), its content runs from the left margin (whole
        // pixels, down, at least 8 px) to the right margin's edge (whole pixels, down), and the columns' printed pixels plus 7 fit in it.
        let px = PRINTER_PIXEL;
        let page = (paper_w as f64 / px).round();
        let from = ((left as f64 / px + 1e-6).floor()).max(8.0);
        let to = (page - right as f64 / px + 1e-6).floor().min(page);
        ((to - from - 7.0) * px).max(least as f64)
    } else {
        (paper_w - left - right - 12_192).max(least) as f64
    };
    let room = (room_w, (paper_h - top - bottom).max(least) as f64);
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
    // A sheet laid out as Excel for Windows prints it has its columns on whole pixels of the printer (`printed_column`), and fitting it to
    // one page wide takes the percentage at which they, each rounded at that scale, still fit (`fit_columns`).
    let exact_across = *sheet.printed && whole_pixel_columns(slide) && matches!(print.fit_to, Some((1, _)));
    let mut scale = match print.fit_to {
        Some((across, down)) => {
            // The sheet prints a point wider and taller than its cells: the
            // grid line before the first column and row (borders and fills
            // reach over it), as Excel fits it (a 554 pt wide print area on
            // 537.7 pt of room prints at 96 %, not 97: xlsx-XBRD BB14).
            let (w, h) = (area.width + 12_700, area.height + 12_700);
            let mut s: f64 = 1.0;
            if exact_across {
                s = s.min(fit_columns(&xs, area, room.0));
            } else if across > 0 {
                s = s.min(room.0 * across as f64 / w.max(1) as f64);
            }
            if down > 0 {
                s = s.min(room.1 * down as f64 / h.max(1) as f64);
            }
            // A whole percentage, as Excel prints.
            (s * 100.0 + 1e-6).floor() / 100.0
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
        let fits = print.fit_to.is_none_or(|(across, down)| (across == 0 || exact_across || columns.len() <= across as usize) && (down == 0 || rows.len() <= down as usize));
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

/// The largest percentage (to 100) at which the columns of `area`, each of its printer pixels times the percentage rounded to
/// whole pixels (half up, as `zoomed_row`), fit `room` (EMU). Excel 365's PDFs of 145 sheets fit to one page wide (`make_fit.py`:
/// 9 to 28 columns of 10 characters and a last one of 1 to 29, A4, 0.7 inch margins): the percentage is the largest at which the
/// rounded columns and the 7 pixels of `room` less the page's width (`room` in `sheet_pages_with_text`) fit, at all 145 (the percent
/// from 300 pt rows' PDF heights); a fit of the unrounded width plus a point (the rule before) is 1 % low at 17 of them.
fn fit_columns(xs: &[i64], area: EmuRect, room: f64) -> f64 {
    let (left, right) = (area.x, area.x + area.width);
    let mut widths: Vec<i64> = xs.windows(2).filter_map(|w| { let (a, b) = (w[0].max(left), w[1].min(right)); (b > a).then_some(b - a) }).collect();
    let covered: i64 = widths.iter().sum();
    if right - left > covered {
        widths.push(right - left - covered);
    }
    let limit = (room / PRINTER_PIXEL).floor();
    for percent in (10..=100).rev() {
        let total: f64 = widths.iter().map(|w| (*w as f64 / PRINTER_PIXEL * percent as f64 / 100.0 + 0.5).floor()).sum();
        if total <= limit {
            return percent as f64 / 100.0;
        }
    }
    0.1
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
#[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
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
    if presentation.workbook.is_none() || ibis_formats::EXCEL_METRICS != ibis_formats::ExcelMetrics::Windows {
        return;
    }
    let factor = |name: &str, default: f64| std::env::var(name).ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(default);
    let style = presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
    let font_pt = if style.font_size > 0 { style.font_size as f64 / 100.0 } else { 11.0 };
    let by_font = ibis_formats::EXCEL_METRICS.print_column_factor(style.font.as_deref(), font_pt);
    // The row factor is the printer's standard row over the screen's, from the Normal font's text metrics; the
    // mechanism is not worked out yet (0.9067 is the one measured for ＭＳ Ｐゴシック 11, todo-adhoc A1: an exception the
    // user decided to keep, 2026-10-21). Without it the corpus' xlsx page counts fall from 117 to 77 of 172 matching
    // (the one-page documents print on two: 059, 074, 107..110, 117 ...). It was set to 1.0 on 2026-10-22 (d34fbed79) to
    // make Mac-measured scenarios pass; that is not Excel for Windows.
    let (cf, rf) = (factor("IBIS_PRINT_COLUMNS", by_font), factor("IBIS_PRINT_ROWS", 0.9067));
    // Excel prints a font at a whole number of the printer's pixels (1/600 inch): 10.5 pt at 88 px, 10.56 pt (its PDFs' `Tf`), and its
    // lines break by those widths (GitHub issue #68: 46 characters of 10.5 pt fit 487.8 pt less the 2.52 pt padding, not those of 10.56).
    let printer_size = |hundredths: u32| (hundredths as f64 / 100.0 * 600.0 / 72.0).round() as u32 * 12;
    let by = |v: i64, f: f64| (v as f64 * f).round() as i64;
    for slide in &mut presentation.slides {
        if !slide.shapes.iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Sheet(_))) {
            continue;
        }
        // The rows print at whole pixels of the printer (`printed_row`); what is below a row moves with it (`RowMap`).
        let map = RowMap::of(slide, |h| printed_row(h, rf), rf);
        let my = |y: i64| map.as_ref().map_or(by(y, rf), |m| m.y(y));
        // The columns print at whole pixels of the printer too (`printed_column`); what is beside a column moves with it.
        let whole = whole_pixel_columns(slide);
        let columns = RowMap::of_columns(slide, |w| if whole { printed_column(w, cf) } else { by(w, cf) }, cf);
        let mx = |x: i64| columns.as_ref().map_or(by(x, cf), |m| m.y(x));
        if let Some(size) = &mut slide.size {
            *size = ibis_doc_model::EmuSize::new(mx(size.width), my(size.height));
        }
        // What is drawn over a chart (its user shapes, `cdr:relSizeAnchor`) stays where it is on the chart, which keeps its size and
        // is put where its cell now is: it moves with the chart instead of its position being scaled on its own (a source line
        // under the plot would else land on the axis labels).
        let charts: Vec<(ibis_doc_model::EmuRect, (i64, i64))> = slide.shapes.iter().filter(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_))).map(|s| (s.frame, (mx(s.frame.x) - s.frame.x, my(s.frame.y) - s.frame.y))).collect();
        for shape in &mut slide.shapes {
            let f = shape.frame;
            if !matches!(shape.kind, ibis_doc_model::ShapeKind::Sheet(_) | ibis_doc_model::ShapeKind::Chart(_)) {
                let inside = |c: &ibis_doc_model::EmuRect| f.x >= c.x && f.y >= c.y && f.right() <= c.right() && f.bottom() <= c.bottom();
                if let Some((_, (dx, dy))) = charts.iter().find(|(c, _)| inside(c)) {
                    shape.frame = ibis_doc_model::EmuRect::new(f.x + dx, f.y + dy, f.width, f.height);
                    continue;
                }
            }
            // A picture or a chart keeps its own size (Excel prints it at the size it is, 411 pt wide
            // in a page of 419.7 pt of columns); it is put where its cell now is. The sheet itself is
            // all its cells.
            // One the file anchors by two cells (`twoCellAnchor`, whatever its `editAs`) is printed from the printed cell of its
            // top-left to that of its bottom-right (Excel 365's PDFs of a box of 6 rows of 15 pt: 82.8 pt tall where the rows
            // print 13.8 pt, and 289.44 pt wide where its 6 columns print 48.24 pt; a `oneCellAnchor` or `absoluteAnchor` one
            // keeps its 288 x 90 pt; pictures the same, #189). A chart keeps its size (not measured).
            let is_sheet = matches!(shape.kind, ibis_doc_model::ShapeKind::Sheet(_));
            let by_cells = is_sheet || (slide.shape_two_cell.contains(&shape.id) && !matches!(shape.kind, ibis_doc_model::ShapeKind::Chart(_)));
            shape.frame = ibis_doc_model::EmuRect::new(mx(f.x), my(f.y), if by_cells { mx(f.right()) - mx(f.x) } else { f.width }, if by_cells { my(f.bottom()) - my(f.y) } else { f.height });
            if let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind {
                let sheet = std::sync::Arc::make_mut(sheet);
                sheet.columns.iter_mut().for_each(|c| *c = if whole { printed_column(*c, cf) } else { by(*c, cf) });
                // The text of a printed column breaks in a wider column than the one drawn (`printed_text_column_pt`).
                let extra: Vec<i64> = sheet.columns.iter().enumerate().map(|(i, &c)| {
                    let text = sheet.column_chars.get(&(i as u32)).filter(|_| c > 0).and_then(|&w| ibis_formats::EXCEL_METRICS.printed_text_column_pt(style.font.as_deref(), font_pt, w));
                    text.map_or(0, |pt| (pt * ibis_doc_model::EMU_PER_POINT as f64).round() as i64 - c)
                }).collect();
                *sheet.printed_text_extra = extra;
                *sheet.printed = true;
                for s in &mut sheet.styles {
                    s.font_size = printer_size(s.font_size);
                }
                for cell in sheet.cells.iter_mut() {
                    if let Some(runs) = &mut cell.runs {
                        for run in std::sync::Arc::make_mut(runs) {
                            run.size = run.size.map(printer_size);
                        }
                    }
                }
                sheet.rows.iter_mut().for_each(|r| *r = printed_row(*r, rf));
                sheet.default_column = if whole { printed_column(sheet.default_column, cf) } else { by(sheet.default_column, cf) };
                sheet.default_row = printed_row(sheet.default_row, rf);
            }
        }
    }
}

/// The EMU of a pixel of the printer Excel for Windows lays a printed page out on (600 dpi).
const PRINTER_PIXEL: f64 = ibis_doc_model::EMU_PER_INCH as f64 / 600.0;

/// A row's printed height at 100 %: its screen height times the row factor, in whole pixels of the printer (600 dpi),
/// `floor(x + 1/8)` of those pixels. The rule measured on Excel 365's PDFs of 667 rows (7 Normal fonts, at 100 %, 94 %,
/// 75 % and 63 %; `tasks/doc/data/xls-goal-printed-custom-row-rounding.csv`): a row whose pixels end in .870 is cut
/// down, .875 and .88 go up. Why the threshold is 1/8 is not known (no text metric or unit of Excel's found that gives
/// it); keeping it waits on the user's decision (AGENTS.md: no ad hoc factors).
fn printed_row(height: i64, factor: f64) -> i64 {
    if height <= 0 {
        return height;
    }
    ((height as f64 * factor / PRINTER_PIXEL + 0.125).floor() * PRINTER_PIXEL).round() as i64
}

/// Whether the slide's sheet has its columns on whole printer pixels (`printed_column`, `zoomed_row`): one that prints at a set scale or is
/// fitted to one page wide (rows free). Those fitted to two or more pages wide or to a number of pages tall are not worked out (Excel 365's
/// PDFs of 45 sheets fitted 2 pages wide: 15 of them 1 to 3 % under the largest percent at which the whole columns fit the pages; the
/// rows' rounding at a fit to pages tall is not known), so they stay as they were laid out before.
fn whole_pixel_columns(slide: &Slide) -> bool {
    !slide.shapes.iter().any(|s| matches!(&s.kind, ibis_doc_model::ShapeKind::Sheet(sheet) if matches!(sheet.print.fit_to, Some((across, down)) if across >= 2 || down > 0)))
}

/// A column's printed width at 100 %: its screen pixels times the printed digit over the screen one (`print_column_factor`),
/// rounded to whole pixels of the printer (600 dpi). Excel 365's PDFs: a column of 2.625 characters (21 screen pixels, digit
/// 8 px on the screen and 46 on the printer) is 121 px = 21 x 46/8 = 120.75, one of 8.43 characters (67 px) 385, one of 13
/// (104 px) 598 (`tasks/doc/data/xls-goal-printed-column-width.csv`: 40 widths of 4 Normal fonts, all of them).
fn printed_column(width: i64, factor: f64) -> i64 {
    if width <= 0 {
        return width;
    }
    let screen_pixel = ibis_doc_model::EMU_PER_INCH as f64 / 96.0;
    ((width as f64 / screen_pixel * factor * (600.0 / 96.0)).round() * PRINTER_PIXEL).round() as i64
}

/// A printed row on a page at `scale` (not 100 %): Excel scales the row's whole printer pixels (`printed_row`) and rounds
/// the product to whole pixels, half up (ＭＳ Ｐゴシック 11 at 75 %: a row of 34 pixels prints 25.5 → 26, one of 56 prints 42;
/// the same 667 rows as `printed_row`). The row is kept in the sheet's EMU, so that it comes to those pixels at the scale.
fn zoomed_row(height: i64, scale: f64) -> i64 {
    if height <= 0 || scale <= 0.0 {
        return height;
    }
    let pixels = (height as f64 / PRINTER_PIXEL).round();
    ((pixels * scale + 0.5).floor() * PRINTER_PIXEL / scale).round() as i64
}

/// Where a height down a sheet's slide comes to when the sheet's rows change their heights: a point in a row keeps its
/// share of the row (a picture anchored in row 20 stays in row 20 when the rows above it print shorter), a point past the
/// rows moves with the default row, one above the sheet by `factor`.
struct RowMap {
    from: i64,
    to: i64,
    old: Vec<i64>,
    new: Vec<i64>,
    default: (i64, i64),
    factor: f64,
}

impl RowMap {
    /// The map of the slide's sheet, its rows' heights changed by `height`; the sheet's top moves by `factor`.
    fn of(slide: &Slide, height: impl Fn(i64) -> i64, factor: f64) -> Option<RowMap> {
        Self::along(slide, false, height, factor)
    }

    /// The same along the columns: a point in a column keeps its share of it, and the sheet's left moves by `factor`.
    fn of_columns(slide: &Slide, width: impl Fn(i64) -> i64, factor: f64) -> Option<RowMap> {
        Self::along(slide, true, width, factor)
    }

    fn along(slide: &Slide, columns: bool, height: impl Fn(i64) -> i64, factor: f64) -> Option<RowMap> {
        let (frame, sheet) = slide.shapes.iter().find_map(|s| match &s.kind {
            ibis_doc_model::ShapeKind::Sheet(sheet) => Some((s.frame, sheet)),
            _ => None,
        })?;
        let (list, origin, default) = if columns { (&sheet.columns, frame.x, sheet.default_column) } else { (&sheet.rows, frame.y, sheet.default_row) };
        let new_rows: Vec<i64> = list.iter().map(|&h| height(h)).collect();
        Some(RowMap {
            from: origin,
            to: (origin as f64 * factor).round() as i64,
            old: tops_of(list),
            new: tops_of(&new_rows),
            default: (default, height(default)),
            factor,
        })
    }

    fn y(&self, y: i64) -> i64 {
        let r = y - self.from;
        if r <= 0 {
            return self.to + (r as f64 * self.factor).round() as i64;
        }
        let share = |d: i64, old: i64, new: i64| if old > 0 { (d as f64 * new as f64 / old as f64).round() as i64 } else { 0 };
        let (old_end, new_end) = (*self.old.last().unwrap_or(&0), *self.new.last().unwrap_or(&0));
        if r >= old_end {
            return self.to + new_end + share(r - old_end, self.default.0, self.default.1);
        }
        let i = self.old.partition_point(|&t| t <= r).saturating_sub(1);
        self.to + self.new[i] + share(r - self.old[i], self.old[i + 1] - self.old[i], self.new[i + 1] - self.new[i])
    }
}

/// The rows of a sheet laid out as Excel for Windows prints it (`windows_print_geometry`) on a page at `scale`: each comes
/// to whole printer pixels at the scale (`zoomed_row`); what is anchored below a row moves with it.
fn zoom_printed_rows(slide: &mut Slide, scale: f64) {
    let whole = whole_pixel_columns(slide);
    if (scale - 1.0).abs() < 1e-9 {
        return;
    }
    let Some(map) = RowMap::of(slide, |h| zoomed_row(h, scale), 1.0) else { return };
    let columns = whole.then(|| RowMap::of_columns(slide, |w| zoomed_row(w, scale), 1.0)).flatten();
    let mx = |x: i64| columns.as_ref().map_or(x, |m| m.y(x));
    if let Some(size) = &mut slide.size {
        size.height = map.y(size.height);
        size.width = mx(size.width);
    }
    for shape in &mut slide.shapes {
        let f = shape.frame;
        let is_sheet = matches!(shape.kind, ibis_doc_model::ShapeKind::Sheet(_));
        shape.frame = ibis_doc_model::EmuRect::new(mx(f.x), map.y(f.y), if is_sheet { mx(f.right()) - mx(f.x) } else { f.width }, if is_sheet { map.y(f.bottom()) - map.y(f.y) } else { f.height });
        if let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind {
            let sheet = std::sync::Arc::make_mut(sheet);
            sheet.rows.iter_mut().for_each(|r| *r = zoomed_row(*r, scale));
            sheet.default_row = zoomed_row(sheet.default_row, scale);
            if whole {
                sheet.columns.iter_mut().for_each(|c| *c = zoomed_row(*c, scale));
                sheet.default_column = zoomed_row(sheet.default_column, scale);
            }
        }
    }
}

/// `zoom_printed_rows` at the scale the slide's sheet prints at, when the sheet is laid out as Excel for Windows prints it.
fn zoom_printed_rows_for(presentation: &Presentation, slide: &mut Slide, resources: &Resources, text: &[EmuRect]) {
    let Some(sheet) = slide.shapes.iter().find_map(|s| match &s.kind {
        ibis_doc_model::ShapeKind::Sheet(sheet) if *sheet.printed => Some(sheet.clone()),
        _ => None,
    }) else {
        return;
    };
    let default_style = presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
    let headings = crate::xlsx::header_footer::heading_size(&sheet, &default_style, resources);
    let (area, titles) = (export_area(presentation, slide), print_titles(presentation, slide));
    let Some((_, scale, _)) = sheet_pages_with_text(slide, area, titles, headings, text) else { return };
    let (fit, whole) = (sheet.print.fit_to, whole_pixel_columns(slide));
    zoom_printed_rows(slide, scale);
    if !whole {
        return;
    }
    // The columns are now the ones that come to whole pixels at this scale: a sheet fitted to one page wide prints at it as it stands
    // (not fitted again to its zoomed widths, which would come to another percent), its manual column breaks left out as fitting does.
    if let Some((across, _)) = fit {
        for shape in &mut slide.shapes {
            if let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind {
                let sheet = std::sync::Arc::make_mut(sheet);
                sheet.print.fit_to = None;
                if across > 0 {
                    sheet.print.column_breaks.clear();
                }
                sheet.print.scale = (scale * 100.0).round() as u32;
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
/// The top of each row (and the bottom of the last) of `rows` heights, from the sheet's top.
fn tops_of(rows: &[i64]) -> Vec<i64> {
    let mut at = 0;
    let mut tops = Vec::with_capacity(rows.len() + 1);
    for h in rows {
        tops.push(at);
        at += *h;
    }
    tops.push(at);
    tops
}

fn snap_custom_rows(presentation: &Presentation, slide: &mut Slide) {
    let mut cut: i64 = 0;
    // The sheet's rows' tops before and after the cut, to move what is anchored to them (shapes, pictures) with the rows: a shape
    // on row 20 stays on row 20 when the rows above it are cut (xlsx-shape-line: 12 pt rows print at 10.8 pt in Excel, were cut to
    // whole points here, and the shapes stayed where the uncut rows were: lower and lower down the page).
    let mut moved: Option<(i64, Vec<i64>, Vec<i64>)> = None;
    let slide_height = presentation.size_of(slide).height;
    for shape in &mut slide.shapes {
        let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
        // A sheet laid out as Excel for Windows prints it (`windows_print_geometry`) keeps its rows: they are on the 0.75 pt pixels from
        // the reader and printed at their screen pixels times the row factor, not cut to whole points after it (Excel 365's PDFs of
        // custom rows of 5.25 to 30.75 pt under ＭＳ Ｐゴシック, 游ゴシック and Calibri 11: a 24.75 pt row under ＭＳ Ｐゴシック 11 prints 187
        // pixels of 600 dpi, 33 × 102 / 18; cut to whole points it would be 22 pt, 183).
        if sheet.custom_rows.is_empty() || *sheet.printed {
            continue;
        }
        let mut shape_cut: i64 = 0;
        let sheet = std::sync::Arc::make_mut(sheet);
        let used = sheet.custom_rows.clone();
        let before = tops_of(&sheet.rows);
        // Rows below the slide (a row of only a height, past the cells) are not printed: what is cut off them is not cut off the
        // slide's height or the sheet's frame (a print area of 20 rows lost the 4 rows' worth of the 80 below it in the page).
        let printed_height = (slide_height - shape.frame.y).min(shape.frame.height);
        let mut starts = Vec::with_capacity(sheet.rows.len());
        let mut at: i64 = 0;
        for h in &sheet.rows {
            starts.push(at);
            at += *h;
        }
        for row in used {
            let beyond = starts.get(row as usize).is_some_and(|start| *start >= printed_height);
            if let Some(h) = sheet.rows.get_mut(row as usize) {
                if *h == 0 {
                    continue;
                }
                if beyond {
                    let whole = ibis_doc_model::EMU_PER_POINT;
                    if *h >= whole {
                        *h -= *h % whole;
                    }
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
        moved = Some((shape.frame.y, before, tops_of(&sheet.rows)));
    }
    if let Some((sheet_y, before, after)) = moved.filter(|_| cut > 0) {
        // A shape's top and bottom edge each go to the same place on the same row (a picture or a chart keeps its size).
        let to_row = |y: i64| -> i64 {
            let offset = y - sheet_y;
            if offset < 0 {
                return y;
            }
            let row = before.partition_point(|top| *top <= offset).saturating_sub(1);
            let (Some(old_top), Some(new_top)) = (before.get(row), after.get(row)) else { return y };
            let room = after.get(row + 1).map_or(i64::MAX, |next| next - new_top);
            sheet_y + new_top + (offset - old_top).min(room)
        };
        for shape in &mut slide.shapes {
            if matches!(shape.kind, ibis_doc_model::ShapeKind::Sheet(_)) {
                continue;
            }
            let (top, bottom) = (shape.frame.y, shape.frame.bottom());
            let new_top = to_row(top);
            shape.frame.y = new_top;
            if !matches!(shape.kind, ibis_doc_model::ShapeKind::Picture(_) | ibis_doc_model::ShapeKind::Chart(_)) {
                shape.frame.height = (to_row(bottom) - new_top).max(1);
            }
        }
    }
    if cut > 0 {
        let mut size = presentation.size_of(slide);
        size.height = (size.height - cut).max(1);
        slide.size = Some(size);
    }
}

/// The frames of a slide's charts (see `hairline_frames`): in the pure black and white print a chart's fills stay grey.
fn chart_frames(slide: &Slide, scale: f32, origin: (f32, f32)) -> Vec<(f32, f32, f32, f32)> {
    slide
        .shapes
        .iter()
        .filter(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_)))
        .map(|s| (s.frame.x as f32 * scale - origin.0, s.frame.y as f32 * scale - origin.1, s.frame.width as f32 * scale, s.frame.height as f32 * scale))
        .collect()
}

/// What a chart's series look like in the pure black and white print (Windows PowerPoint's `PrintOut` PDFs, `tasks/doc/requests/2026-11-20-225-chart-print-result.md`):
/// - a series that states no format at all (no `c:spPr`) is black, white, then the dots of the paper's grid by its number (a pie's slices by theirs), with the thin black edge below;
///   a line's automatic line is black, its markers as the others;
/// - a filled series (bar, area, pie, doughnut) whose fill is stated but has no outline element gets a thin black edge: 0.8 of the units of PowerPoint's print PDF
///   (0.6 pt: 5 pixels of the printer's 600 dpi, the width any 0.6 pt line is drawn at there), and none when its outline is stated, `a:noFill` included.
///
/// - a marker has the thin black edge.
///
/// The grey print does neither (its automatic series are the grey of their colours). The colours of the fills and lines are the grey of the print
/// (`pdf_vector`: `PaintKind`).
/// The automatic fill of the `n`th series (or slice), counting from 0, in the pure black and white print: PowerPoint's patterns repeat with period 19 (series 20 is black
/// again): 1 black, 2 white, 3 to 19 the tiles of `pdf_vector::DOT_TILES` (the cells are a grid of the slide).
pub(crate) fn print_automatic_fill(n: usize) -> ibis_doc_model::Fill {
    use ibis_doc_model::{Color, Fill};
    match n % 19 {
        0 => Fill::Solid(Color::BLACK),
        1 => Fill::Solid(Color::WHITE),
        p => Fill::Pattern { preset: format!("{}{}", crate::render::shape_view::PRINT_DOTS_PRESET_PREFIX, p + 1), foreground: Color::BLACK, background: Color::WHITE },
    }
}

pub(crate) fn print_chart_series(slide: &mut Slide, color: crate::export_print::PrintColor) {
    use ibis_doc_model::{ChartType, Color, ShapeKind, Stroke};
    if color != crate::export_print::PrintColor::BlackWhite {
        return;
    }
    // 0.8 units of 0.75 pt, in EMU.
    const EDGE_EMU: i64 = 7_620;
    let automatic = print_automatic_fill;
    for shape in &mut slide.shapes {
        let ShapeKind::Chart(chart) = &mut shape.kind else { continue };
        let mut number = 0;
        for group in &mut chart.groups {
            let filled = matches!(group.kind, ChartType::Bar { .. } | ChartType::Area | ChartType::Pie { .. } | ChartType::Doughnut { .. });
            let sliced = matches!(group.kind, ChartType::Pie { .. } | ChartType::Doughnut { .. });
            for series in &mut group.series {
                let n = number;
                number += 1;
                if filled {
                    if !series.has_format {
                        series.fill = automatic(n);
                        if sliced {
                            for i in 0..series.values.len() {
                                let fill = automatic(i);
                                match series.point_fills.iter_mut().find(|(idx, _)| *idx == i) {
                                    Some(slot) => slot.1 = fill,
                                    None => series.point_fills.push((i, fill)),
                                }
                            }
                        }
                    }
                    if !series.has_outline {
                        series.stroke = Some(Stroke::solid(Color::BLACK, EDGE_EMU));
                    }
                } else {
                    if !series.has_format {
                        if let Some(line) = &mut series.stroke {
                            line.color = Color::BLACK;
                        }
                        if let Some(marker) = &mut series.marker {
                            marker.fill = automatic(n);
                        }
                    }
                    // A marker has the thin black edge too.
                    if let Some(marker) = &mut series.marker {
                        marker.edge = Some(Stroke::solid(Color::BLACK, EDGE_EMU));
                    }
                }
            }
        }
    }
}

/// The frames (x, y, width, height in the window's logical pixels, `scale` of them to an EMU and `origin` the window's top left in the page's EMU) of the shapes of
/// a slide whose fill is a plain colour and which have no line you can see: in PowerPoint's pure black and white print each of them gets a thin black edge
/// (`pdf_vector::HAIRLINE_PT`). A gradient, a picture, a pattern, a table's cell and a chart do not (Windows PowerPoint's `PrintOut` PDFs,
/// `tasks/doc/requests/2026-11-20-225-bw-hairline-result.md`); a turned or flipped shape's frame is not its painted box, so it is left out.
fn hairline_frames(slide: &Slide, scale: f32, origin: (f32, f32)) -> Vec<(f32, f32, f32, f32)> {
    use ibis_doc_model::{Fill, ShapeKind};
    slide
        .shapes
        .iter()
        .filter(|s| s.rotation == 0 && !s.flip_h && !s.flip_v)
        .filter(|s| match &s.kind {
            ShapeKind::Auto(a) => matches!(a.fill, Fill::Solid(c) if c.alpha == 255) && a.stroke.as_ref().is_none_or(|l| l.width <= 0 || l.color.alpha == 0),
            _ => false,
        })
        .map(|s| (s.frame.x as f32 * scale - origin.0, s.frame.y as f32 * scale - origin.1, s.frame.width as f32 * scale, s.frame.height as f32 * scale))
        .collect()
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

/// Text laid out and drawn as a slide's text is: a text box of a slide in a headless window of its own, for the text on the printed
/// pages that are pictures (the notes, the outline, a handout's header and footer), so it is the same on every platform.
pub(crate) struct SlideTextLayer<'a> {
    pub(crate) cx: &'a mut HeadlessAppContext,
    pub(crate) resources: Rc<Resources>,
    /// Device pixels per logical pixel (`pixel_density`).
    pub(crate) density: f32,
    /// The text comes as what the window painted, for the vector PDF, and not as pixels.
    pub(crate) vector: bool,
    /// The baseline of the first line of the text laid out last (picture pixels).
    pub(crate) last_baseline: Option<f32>,
}

impl crate::export_text::TextLayer for SlideTextLayer<'_> {
    fn first_baseline(&self) -> Option<f32> {
        self.last_baseline
    }

    fn typeset(&mut self, body: &ibis_doc_model::TextBody, width: u32, height: u32) -> Option<(crate::export_text::Picture, Vec<(f32, f32)>)> {
        let density = self.density;
        // A pixel of the picture is `1 / per_emu` EMU; a logical pixel of the window `density` of them.
        let per_emu = crate::export_print::PX_PER_PT / 12_700.0;
        let frame = EmuRect::new(0, 0, (width as f32 / per_emu).round() as i64, (height as f32 / per_emu).round() as i64);
        let mut slide = Slide::new(ibis_doc_model::SlideId(0));
        slide.shapes.push(ibis_doc_model::Shape::new(ibis_doc_model::ShapeId(1), frame, ibis_doc_model::ShapeKind::TextBox(body.clone())));
        let window_size = gpui_kit::size(px(width as f32 / density), px(height as f32 / density));
        let scale = per_emu / density;
        let positions = Rc::new(RefCell::new(Vec::new()));
        let resources = self.resources.clone();
        let shape_positions = positions.clone();
        let window = self
            .cx
            .open_window(window_size, |_, app| app.new(|_| SlidePicture { slide, scale, size: window_size, origin: (0.0, 0.0), inset: (0.0, 0.0), view: window_size, resources, marks: Vec::new(), extra: Vec::new(), rules: Vec::new(), shape_positions }))
            .ok()?;
        let picture = if self.vector {
            // Only recorded: the PDF is made of the record.
            self.cx
                .update_window(window.into(), |_, window, _| {
                    window.record_paints();
                    window.record_paints_only(true);
                })
                .ok()?;
            self.cx.run_until_parked();
            self.cx.update_window(window.into(), |_, window, _| window.refresh()).ok()?;
            self.cx.run_until_parked();
            let records = self.cx.update_window(window.into(), |_, window, _| window.take_recorded_paints()).ok().flatten();
            self.cx.update_window(window.into(), |_, window, _| window.remove_window()).ok();
            // The window's white page is what the text is on: the page has its own.
            let records = records?.into_iter().filter(|r| !matches!(r, gpui_kit::PaintRecord::Quad { background, border_widths, .. } if background.as_solid().is_some_and(|c| c.a > 0.0 && c.l > 0.99) && f32::from(border_widths.top) <= 0.0)).collect();
            crate::export_text::Picture::Painted { records, width, height, density }
        } else {
            self.cx.run_until_parked();
            self.cx.update_window(window.into(), |_, window, _| window.refresh()).ok()?;
            self.cx.run_until_parked();
            let image = self.cx.capture_screenshot(window.into()).ok();
            self.cx.update_window(window.into(), |_, window, _| window.remove_window()).ok();
            crate::export_text::Picture::Image(image?)
        };
        self.last_baseline = positions.borrow().iter().find_map(|(_, text)| text.paragraphs.borrow().first().and_then(|p| p.borrow().lines.first().map(|l| l.baseline * density)));
        let lines = positions
            .borrow()
            .iter()
            .flat_map(|(_, text)| text.paragraphs.borrow().iter().flat_map(|p| p.borrow().lines.iter().map(|l| (l.top * density, (l.top + l.height) * density)).collect::<Vec<_>>()).collect::<Vec<_>>())
            .collect();
        Some((picture, lines))
    }
}

#[cfg(target_family = "wasm")]
thread_local! {
    /// The browser's text system (the fonts it has loaded), kept by `run` for the PDF made in the page (the browser has no platform to
    /// ask: `current_platform` is for the desktop).
    static WEB_TEXT_SYSTEM: RefCell<Option<Arc<dyn PlatformTextSystem>>> = const { RefCell::new(None) };
}

/// Keep the browser's text system for `headless_app`.
#[cfg(target_family = "wasm")]
pub(crate) fn set_web_text_system(text_system: Arc<dyn PlatformTextSystem>) {
    WEB_TEXT_SYSTEM.with(|t| *t.borrow_mut() = Some(text_system));
}

/// A headless app to draw in, with the platform's text system (text laid
/// out as in the editor) and its headless renderer if it has one, and the
/// platform, to be kept as long as the app.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn headless_app() -> (Rc<dyn Platform>, HeadlessAppContext) {
    crate::register_hooks();
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

/// The headless app of the browser version: the editor's own text system (its fonts), no renderer (a PDF is made of what is
/// painted, not of pictures).
#[cfg(target_family = "wasm")]
pub(crate) fn headless_app() -> Result<HeadlessAppContext, String> {
    crate::xlsx::sheet_view::release_layouts();
    let text_system = WEB_TEXT_SYSTEM.with(|t| t.borrow().clone()).ok_or("the editor has not started")?;
    Ok(HeadlessAppContext::with_platform(text_system, Arc::new(crate::shell::icons::Assets::new("")), || None))
}

/// The platform's renderer drawing without a window (the shell's, which its scenarios and the clipboard's pictures use too).
#[cfg(not(target_family = "wasm"))]
pub(crate) use crate::shell::headless::headless_renderer;

/// Print the time since `since` (and restart it) under `IBIS_DEBUG_TIMING`.
fn lap(what: &str, since: &mut crate::clock::Instant) {
    if std::env::var_os("IBIS_DEBUG_TIMING").is_some() {
        eprintln!("export: {what} {:.0} ms", since.elapsed().as_secs_f64() * 1000.0);
    }
    *since = crate::clock::Instant::now();
}

/// `--export row-heights <file> [out.tsv]`: the height of each row of each sheet, in points, as the app lays it out when it opens the
/// file (after `fit_measured_rows`, before printing's own scaling: what Excel's `Rows(i).Height` is compared with). Tab-separated,
/// one line per row: sheet number (from 1), sheet name, row number (from 1), height (0 for a hidden row), `custom` or `auto`
/// (`customHeight`), then a last line of `kind` `default` (the row number after the last one the sheet keeps, the height of every row from it on: an empty row beyond the sheet's own). Written to `out` or, without it, to the standard output. No window is opened.
#[cfg(not(target_family = "wasm"))]
pub fn row_heights(mut presentation: Presentation, out: Option<&Path>) -> Result<(), String> {
    let (_platform, mut cx) = headless_app();
    cx.allow_parking();
    let resources = Rc::new(cx.update(|app| {
        let resources = Resources::new(&presentation, app);
        resources.preload.wait_all();
        resources
    }));
    crate::xlsx::row_fit::fit_measured_rows(&mut presentation, &resources);
    let mut tsv = String::from("sheet\tname\trow\theight_pt\tkind\n");
    let mut number = 0;
    for slide in &presentation.slides {
        for shape in &slide.shapes {
            let ibis_doc_model::ShapeKind::Sheet(sheet) = &shape.kind else { continue };
            number += 1;
            let name = presentation
                .workbook
                .as_ref()
                .and_then(|w| w.sheets.iter().find(|s| s.slide == Some(slide.id)))
                .map(|s| s.name.clone())
                .unwrap_or_default();
            let name_cell = name.replace(['\t', '\n'], " ");
            // The rows after the last one the sheet keeps are all this high (`kind` `default`, from its row number on).
            let default_line = format!("{number}\t{name_cell}\t{}\t{}\tdefault\n", sheet.rows.len() + 1, ibis_calc::number_to_xml(sheet.default_row as f64 / ibis_doc_model::EMU_PER_POINT as f64));
            for (row, &height) in sheet.rows.iter().enumerate() {
                let kind = if sheet.custom_rows.contains(&(row as u32)) { "custom" } else { "auto" };
                tsv.push_str(&format!("{number}\t{name_cell}\t{}\t{}\t{kind}\n", row + 1, ibis_calc::number_to_xml(height as f64 / ibis_doc_model::EMU_PER_POINT as f64)));
            }
            tsv.push_str(&default_line);
        }
    }
    match out {
        Some(path) => std::fs::write(path, tsv).map_err(|e| format!("{}: {e}", path.display())),
        None => {
            print!("{tsv}");
            Ok(())
        }
    }
}

/// Export `presentation` as `options` say. Prints each file written.
pub fn export(presentation: Presentation, options: &Options) -> Result<(), String> {
    let made = render(presentation, options)?;
    if let Some(writer) = made.writer {
        let path = options.out_dir.join(format!("{}.pdf", options.name));
        let finished = finish_pdf(writer)?;
        for warning in &finished.warnings {
            eprintln!("ibisOffice: {warning}");
        }
        std::fs::write(&path, finished.bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("{}", path.display());
    }
    Ok(())
}

/// What `render` made of a vector PDF.
pub(crate) struct Rendered {
    /// The vector PDF's pages, not yet written (`finish_pdf`); the browser version first gives fonts to the glyphs the browser drew.
    pub writer: Option<crate::pdf_vector::PdfWriter>,
}

/// A written vector PDF.
pub(crate) struct Finished {
    pub bytes: Vec<u8>,
    pub warnings: Vec<String>,
    /// Characters left out of the PDF for want of their font's data.
    pub missing_glyphs: usize,
}

/// The links of a page of a flowing document as the PDF holds them (the page number a place is on counts from 1, as the pages are numbered).
fn document_page_links(links: &[crate::docx::flow_layout::PageLinkRect]) -> Vec<crate::pdf_vector::PageLink> {
    use crate::docx::flow_layout::PageLinkTarget;
    links
        .iter()
        .map(|l| crate::pdf_vector::PageLink {
            rect: (l.x, l.y, l.w, l.h),
            to: match &l.target {
                PageLinkTarget::Url(u) => crate::pdf_vector::LinkTo::Url(u.clone()),
                PageLinkTarget::Place { page, x, y } => crate::pdf_vector::LinkTo::Page { number: page + 1, x: *x, y: *y },
            },
        })
        .collect()
}

/// Where a link of a slide goes, in the PDF: an address, or a slide (the top left of its page). PowerPoint's PDF has no link for the next, previous, first and last slide and
/// for ending the show (`None`: no annotation), as for a slide that is not there.
fn slide_link_to(presentation: &Presentation, link: &ibis_doc_model::SlideLink) -> Option<crate::pdf_vector::LinkTo> {
    use crate::pdf_vector::LinkTo;
    use ibis_doc_model::SlideLink;
    Some(match link {
        SlideLink::Url(u) if !u.is_empty() => LinkTo::Url(u.clone()),
        SlideLink::Slide(id) => LinkTo::Page { number: presentation.slide_index(*id)? + 1, x: 0.0, y: 0.0 },
        _ => return None,
    })
}

/// The links of the text of the slide at `index` (from 0): each line of a run that is a link, over the characters of the line as they were painted (`positions`, the shapes' text
/// and `cells`, the text of the cells of the tables, in window pixels; `k` points to a pixel).
fn text_run_links(
    presentation: &Presentation,
    index: usize,
    positions: &[(ibis_doc_model::ShapeId, crate::render::rich_text::TextPositions)],
    cells: &[(ibis_doc_model::ShapeId, HashMap<(usize, usize), crate::render::rich_text::TextPositions>)],
    k: f32,
) -> Vec<crate::pdf_vector::PageLink> {
    let Some(slide) = presentation.slides.get(index) else { return Vec::new() };
    let mut out = Vec::new();
    for (id, pos) in positions {
        if let Some(body) = slide.shape(*id).and_then(|s| s.text_body()) {
            body_links(presentation, body, pos, k, false, &mut out);
        }
    }
    for (id, recorded) in cells {
        let Some(ibis_doc_model::ShapeKind::Table(table)) = slide.shape(*id).map(|s| &s.kind) else { continue };
        for ((row, column), pos) in recorded {
            if let Some(cell) = table.rows.get(*row).and_then(|r| r.cells.get(*column)) {
                body_links(presentation, &cell.text, pos, k, false, &mut out);
            }
        }
    }
    out
}

/// The links of the runs of `body` (painted at `pos`) into `out`.
///
/// `in_document`: the text is a text box of a document's page: Word's PDF has the link over the line's box (its height, spaces in it) from 3 pixels (2.25 pt) before the first linked
/// character to 3 pixels after the last, as it has for the lines of the page; PowerPoint's is over the glyphs' ink.
fn body_links(presentation: &Presentation, body: &ibis_doc_model::TextBody, pos: &crate::render::rich_text::TextPositions, k: f32, in_document: bool, out: &mut Vec<crate::pdf_vector::PageLink>) {
    if !body.paragraphs.iter().any(|p| p.runs.iter().any(|r| r.style.link.is_some())) {
        return;
    }
    for (paragraph, slot) in body.paragraphs.iter().zip(pos.paragraphs.borrow().iter()) {
        let q = slot.borrow();
        let mut offset = 0;
        for run in &paragraph.runs {
            let len = run.text.chars().count();
            if let Some(to) = run.style.link.as_ref().and_then(|l| slide_link_to(presentation, l)) {
                // The characters of the run, line by line.
                let mut by_line: Vec<(usize, [f32; 4])> = Vec::new();
                // (A shape's text turned: the glyphs' ink boxes of a line are joined in the text's own coordinates and that box is turned, as PowerPoint's PDF has it (Windows PowerPoint at
                // 15, 45 and 90 degrees within 0.15 pt: the box of the line's glyphs, not the line's height), so each line has one box.)
                let mut unturned: Vec<(usize, [f32; 4])> = Vec::new();
                for c in offset..(offset + len).min(q.chars.len()) {
                    let (ch, Some(line)) = (q.chars[c], q.lines.get(q.chars[c].line)) else { continue };
                    // PowerPoint's PDF has the link over the boxes of the glyphs' ink (a space has none), not the line's height.
                    if in_document || q.turn.is_some() && ch.ink.is_none() {
                        // A document's text box: the cell and the line's height (Word).
                        let corners = q.corners(ch.x.min(ch.x + ch.advance), line.top, ch.advance.abs(), line.height);
                        let (x0, x1) = corners.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
                        let (y0, y1) = corners.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
                        match by_line.iter_mut().find(|(l, _)| *l == ch.line) {
                            Some((_, r)) => *r = [r[0].min(x0), r[1].min(y0), r[2].max(x1), r[3].max(y1)],
                            None => by_line.push((ch.line, [x0, y0, x1, y1])),
                        }
                        continue;
                    }
                    let Some((left, right, top, bottom)) = ch.ink else { continue };
                    let rect = [ch.x + left, line.baseline + top, ch.x + right, line.baseline + bottom];
                    match unturned.iter_mut().find(|(l, _)| *l == ch.line) {
                        Some((_, r)) => *r = [r[0].min(rect[0]), r[1].min(rect[1]), r[2].max(rect[2]), r[3].max(rect[3])],
                        None => unturned.push((ch.line, rect)),
                    }
                }
                for (line, r) in unturned {
                    let corners = q.corners(r[0], r[1], r[2] - r[0], r[3] - r[1]);
                    let (x0, x1) = corners.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
                    let (y0, y1) = corners.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
                    by_line.push((line, [x0, y0, x1, y1]));
                }
                for (_, mut r) in by_line {
                    if in_document && q.turn.is_none() {
                        let reach = 2.25 / k;
                        (r[0], r[2]) = (r[0] - reach, r[2] + reach);
                    }
                    out.push(crate::pdf_vector::PageLink { rect: (r[0] * k, r[1] * k, (r[2] - r[0]) * k, (r[3] - r[1]) * k), to: to.clone() });
                }
            }
            offset += len;
        }
    }
}

/// The links of the slide at `index` (from 0): its shapes that are links (`shape_links`), over the shape's frame, to an address or to a slide (the next, previous, first and last
/// too). `area` is the part of the slide the page shows.
fn slide_page_links(presentation: &Presentation, index: usize, area: EmuRect) -> Vec<crate::pdf_vector::PageLink> {
    let Some(slide) = presentation.slides.get(index) else { return Vec::new() };
    let mut out = Vec::new();
    // (A shape in a group is in the slide's list with its frame in the slide's; a hidden one has no link on the page.)
    for shape in slide.shown_shapes() {
        let Some(to) = slide.shape_links.get(&shape.id).and_then(|l| slide_link_to(presentation, l)) else { continue };
        // The box that holds the shape: its outline, turned, and its shadow (`Shape::link_box`).
        let [l, t, r, bottom] = shape.link_box(shape.frame);
        out.push(crate::pdf_vector::PageLink { rect: ((l as i64 - area.x) as f32 / 12_700.0, (t as i64 - area.y) as f32 / 12_700.0, (r - l) as f32 / 12_700.0, (bottom - t) as f32 / 12_700.0), to });
    }
    // A group's link is one annotation over the union of its shapes' boxes (each turned with the group, with its line), not over the turned group
    // (PowerPoint's PDF of a group turned 20 degrees: 309.6 x 261.9 pt, the union of the children's, and not 355.4 x 259.8, the group's).
    fn leaves(group: &ibis_doc_model::ShapeGroup, out: &mut Vec<ibis_doc_model::ShapeId>) {
        for member in &group.children {
            match member {
                ibis_doc_model::GroupMember::Shape(id) => out.push(*id),
                ibis_doc_model::GroupMember::Group(inner) => leaves(inner, out),
            }
        }
    }
    fn each_group<'a>(groups: &'a [ibis_doc_model::ShapeGroup], out: &mut Vec<&'a ibis_doc_model::ShapeGroup>) {
        for g in groups {
            out.push(g);
            for member in &g.children {
                if let ibis_doc_model::GroupMember::Group(inner) = member {
                    each_group(std::slice::from_ref(&**inner), out);
                }
            }
        }
    }
    let mut all = Vec::new();
    each_group(&slide.groups, &mut all);
    for group in all {
        let Some(to) = slide.group_links.get(&group.native_id).and_then(|l| slide_link_to(presentation, l)) else { continue };
        let mut ids = Vec::new();
        leaves(group, &mut ids);
        let boxes: Vec<[f64; 4]> = ids.iter().filter_map(|id| slide.shape(*id)).filter(|s| !slide.hidden_shapes.contains(&s.id)).map(|s| s.link_box(s.frame)).collect();
        let Some(first) = boxes.first().copied() else { continue };
        let [l, t, r, bottom] = boxes.iter().fold(first, |a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
        out.push(crate::pdf_vector::PageLink { rect: ((l as i64 - area.x) as f32 / 12_700.0, (t as i64 - area.y) as f32 / 12_700.0, (r - l) as f32 / 12_700.0, (bottom - t) as f32 / 12_700.0), to });
    }
    out
}

/// Write the pages of `writer`.
pub(crate) fn finish_pdf(writer: crate::pdf_vector::PdfWriter) -> Result<Finished, String> {
    let mut lap_at = crate::clock::Instant::now();
    let mut warnings = Vec::new();
    let (bytes, outlined, missing_glyphs) = writer.finish()?;
    lap("PDF written (finish)", &mut lap_at);
    if missing_glyphs > 0 {
        warnings.push(format!("could not get the font data, so {missing_glyphs} characters could not be written"));
    }
    if !outlined.is_empty() {
        warnings.push(format!("characters of fonts that cannot be subsetted were written as outlines (they cannot be selected as text): {}", outlined.join(", ")));
    }
    Ok(Finished { bytes, warnings, missing_glyphs })
}

/// アウトライン: the slides' titles (with their numbers) and body paragraphs of the slides `numbers`, as PowerPoint's outline page sets them
/// (Windows PowerPoint 16, PDFs of the outline over 18 files of three slide sizes and 24/32/44 pt titles with 18/28 pt bodies, and 174 pairs of
/// the master's margin and indent): every run 9.96 pt whatever the slide's sizes, the titles bold with their number hanging at 4.51 pt of the
/// box, titles at 30.26 pt, lines 18.12 pt apart, 6.6 pt above a title and 1.68 pt above a title's first body line. A body paragraph with the
/// master's left margin M and indent I has its bullet at R((M + I) / 3) from the title's x and its text at the larger of R(M / 3) and the end of
/// the bullet, R a rounding to 0.12 pt: the outline is the slide at a third (within 0.03 pt of the 174 pairs). The text's place after the bullet is
/// the text layer's own (a hanging indent starts the text where the bullet ends if it is past the margin).
fn outline_text(presentation: &Presentation, numbers: &[usize]) -> crate::export_print::NoteText {
    use crate::export_print::{NotePara, NoteRun, ParaLayout};
    const SIZE_PT: f32 = 9.96;
    const TITLE_AT_PT: f32 = 30.264;
    const NUMBER_AT_PT: f32 = 4.512;
    const PITCH_PT: f32 = 18.12;
    // A margin of the slide as the outline has it: a third, rounded to 0.12 pt (half up).
    let outline = |emu: i64| ((emu as f32 / 12_700.0 / 3.0 / 0.12 + 0.5).floor()) * 0.12;
    let mut paragraphs = Vec::new();
    for n in numbers {
        let slide = &presentation.slides[n - 1];
        let (title, body) = crate::pptx::outline_edit::outline_of(slide);
        let margins = crate::pptx::outline_edit::body_margins_of(slide);
        let run = |text: String, bold: bool| NoteRun { text, bold, size_pt: Some(SIZE_PT), ..Default::default() };
        paragraphs.push(NotePara {
            level: 0,
            bullet: Some(n.to_string()),
            bullet_font: None,
            runs: vec![run(title, true)],
            layout: Some(ParaLayout { left_pt: TITLE_AT_PT, hang_pt: TITLE_AT_PT - NUMBER_AT_PT, before_pt: if paragraphs.is_empty() { 0.0 } else { 6.6 }, pitch_pt: PITCH_PT }),
        });
        for (i, (_, text)) in body.into_iter().enumerate() {
            let (margin, indent) = margins.get(i).copied().unwrap_or((0, 0));
            let (text_at, bullet_at) = (outline(margin), outline(margin + indent));
            paragraphs.push(NotePara {
                level: 0,
                bullet: Some("•".to_string()),
                // PowerPoint sets the outline's bullets in Arial (0.35 em wide: 3.486 pt at 9.96 pt), whatever the slide's font is.
                bullet_font: Some("Arial".to_string()),
                runs: vec![run(text, false)],
                layout: Some(ParaLayout { left_pt: TITLE_AT_PT + text_at, hang_pt: text_at - bullet_at, before_pt: if i == 0 { 1.68 } else { 0.0 }, pitch_pt: PITCH_PT }),
            });
        }
    }
    crate::export_print::NoteText { paragraphs }
}

/// The handout marks with the number each shown slide carries under it: the slide's own number (`first_slide_number` counted in).
fn marks_with_numbers(options: &Options, presentation: &Presentation, numbers: &[usize]) -> crate::export_print::HandoutMarks {
    let mut marks = options.handout_marks.clone();
    marks.slide_numbers = slide_numbers(presentation.first_slide_number, numbers);
    marks.slide_size_pt = Some((presentation.slide_size.width as f32 / 12_700.0, presentation.slide_size.height as f32 / 12_700.0));
    marks.on_paper = handout_paper(options);
    marks
}

/// The paper a handout's page is laid out on (#225): the page is the paper's size and its frames are put by fractions of it, which is not the page of the file
/// scaled to the paper by one factor (see `HandoutMarks::on_paper`). `None`: no paper, a page not fitted to it, or notes pages and the outline (they are scaled by one factor).
fn handout_paper(options: &Options) -> Option<crate::export_print::Paper> {
    options.paper.filter(|_| options.fit_paper && options.handout.is_some() && !options.notes_pages && !options.outline_pages)
}

/// The numbers under the slides `numbers` (their places in the file, from 1) when the first slide's number is `first`: a hidden slide
/// left out leaves its number out (Microsoft 365: 1, 3, 4, 5 with the second hidden; 5, 6, 7, 8 from 5).
fn slide_numbers(first: usize, numbers: &[usize]) -> Vec<usize> {
    numbers.iter().map(|n| n + first.max(1) - 1).collect()
}

/// The notes of the slides `numbers`, as the pages carry them.
fn notes_texts(presentation: &Presentation, numbers: &[usize], options: &Options) -> Vec<crate::export_print::NoteText> {
    numbers.iter().map(|n| rich_notes(options.notes_source.as_deref(), n - 1, &presentation.slides[n - 1].notes)).collect()
}

/// The pages as `options` say: PNG pictures and the picture PDF are written to `options.out_dir` (not in the browser); the vector
/// PDF is returned, for the caller to write (the file, or the browser's download or the host's storage).
pub(crate) fn render(mut presentation: Presentation, options: &Options) -> Result<Rendered, String> {
    let mut pdf_writer = None;
    // Pictures are drawn as a PDF rasterizer draws them (the coverage
    // tests' references): glyphs without macOS' font smoothing, which
    // thickens their stems (16 % more ink in PTXT; the screen keeps it, as
    // macOS and Office draw text there).
    #[cfg(target_os = "macos")]
    gpui_macos::set_font_smoothing(false);
    let mut lap_at = crate::clock::Instant::now();
    #[cfg(not(target_family = "wasm"))]
    let (_platform, mut cx) = headless_app();
    #[cfg(target_family = "wasm")]
    let mut cx = headless_app()?;
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
    // Without `--range`, the slides PowerPoint prints: the hidden ones are left out (their numbers
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
    crate::render::shape_view::record_table_cells(options.format == Format::Pdf);
    // A flowing document's links per page (they are found from its layout, not from the slides the pages are painted as).
    // Where the text of each shape of the page last drawn is (the PDF's links of the runs that are links).
    let link_positions: Rc<RefCell<Vec<(ibis_doc_model::ShapeId, crate::render::rich_text::TextPositions)>>> = Rc::new(RefCell::new(Vec::new()));
    let document_links = (options.format == Format::Pdf).then(|| presentation.flow.as_ref().map(|f| crate::docx::flow_layout::page_links(f, &resources))).flatten();
    let mut vector = (options.format == Format::Pdf).then(|| {
        let mut writer = crate::pdf_vector::PdfWriter::new(ts.clone());
        writer.set_print_color(options.color);
        writer
    });
    // 配布資料, ノート ページ and アウトライン of a presentation in the vector PDF: the slides' records are kept to be put on those pages.
    let pages_asked = vector.is_some() && (options.handout.is_some() || options.notes_pages || options.outline_pages) && presentation.workbook.is_none() && presentation.flow.is_none();
    let mut slide_records: Vec<crate::export_print::SlideRecords> = Vec::new();
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
        print_chart_series(&mut slide, options.color);
                print_layout(&presentation, &mut slide);
                let id = slide.id;
                zoom_printed_rows_for(&presentation, &mut slide, &resources, sheet_text.get(&id).map(Vec::as_slice).unwrap_or(&[]));
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
        // The outline has no pictures of the slides.
        if pages_asked && options.outline_pages {
            continue;
        }
        let mut slide = presentation.slides[number - 1].clone();
        black_and_white(&mut slide);
        print_chart_series(&mut slide, options.color);
                print_layout(&presentation, &mut slide);
        let id = slide.id;
        zoom_printed_rows_for(&presentation, &mut slide, &resources, sheet_text.get(&id).map(Vec::as_slice).unwrap_or(&[]));
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
        let hairline = if options.color == crate::export_print::PrintColor::BlackWhite { hairline_frames(&slide, scale, origin) } else { Vec::new() };
        let charts = chart_frames(&slide, scale, origin);
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
                        .open_window(window_size, |_, app| app.new(|_| SlidePicture { slide, scale, size: window_size, origin, inset: (0.0, 0.0), view: window_size, resources, marks: Vec::new(), extra: Vec::new(), rules: Vec::new(), shape_positions: link_positions.clone() }))
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
            let page_pt = (area.width as f32 / 12_700.0, area.height as f32 / 12_700.0);
            let mut records = records;
            if options.frame && !pages_asked {
                // "Frame slides": a line of 0.96 pt round the slide (its outer edge at the slide's), as PowerPoint's PDF (`export_print::FRAME_PT`).
                // (On a paper the page is scaled, and the line is not: `PdfWriter::fit_to_paper`.)
                let on_paper = options.paper.map_or(1.0, |p| p.fit(page_pt, options.fit_paper).scale).max(1e-3);
                let line = crate::export_print::FRAME_PT * f32::from(window_size.width) / page_pt.0.max(1.0) / on_paper;
                let bounds = gpui_kit::Bounds::new(gpui_kit::point(gpui_kit::px(0.0), gpui_kit::px(0.0)), gpui_kit::size(gpui_kit::px(f32::from(window_size.width)), gpui_kit::px(f32::from(window_size.height))));
                records.push(gpui_kit::PaintRecord::Quad {
                    bounds,
                    clip: bounds,
                    background: gpui_kit::Background::from(gpui_kit::Hsla { h: 0.0, s: 0.0, l: 0.0, a: 0.0 }),
                    border_color: gpui_kit::Hsla { h: 0.0, s: 0.0, l: 0.0, a: 1.0 },
                    border_widths: gpui_kit::Edges::all(gpui_kit::px(line)),
                    corner_radii: gpui_kit::Corners::default(),
                    border_style: gpui_kit::BorderStyle::Solid,
                });
            }
            if pages_asked {
                slide_records.push(crate::export_print::SlideRecords { records, width: f32::from(window_size.width), density, size_pt: page_pt, hairline, charts });
            } else {
                writer.add_page(records, f32::from(window_size.width), page_pt, density);
                if !hairline.is_empty() || !charts.is_empty() {
                    writer.hairline_last_page(hairline, charts);
                }
                // The page's hyperlinks (the address and the places the file's links go to), as Word and PowerPoint write them in a PDF (#227).
                let links = match &document_links {
                    Some(pages) => {
                        // The lines' links from the layout, and the links of the text of the page's text boxes as they were painted (`record_text_box`).
                        let mut links = pages.get(number - 1).map(|l| document_page_links(l)).unwrap_or_default();
                        let k = page_pt.0 / f32::from(window_size.width).max(1.0);
                        for (body, pos) in crate::render::shape_view::take_recorded_text_boxes() {
                            body_links(&presentation, &body, &pos, k, true, &mut links);
                        }
                        links
                    }
                    None => {
                        let mut links = slide_page_links(&presentation, number - 1, area);
                        links.extend(text_run_links(&presentation, number - 1, &link_positions.borrow(), &crate::render::shape_view::take_recorded_table_cells(), page_pt.0 / f32::from(window_size.width).max(1.0)));
                        links
                    }
                };
                writer.link_last_page(number, links);
            }
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
    if let Some(mut writer) = vector {
        crate::render::chart_view::VECTOR_EXPORT.with(|v| v.set(false));
        if pages_asked {
            // The pages are laid out as the picture PDF's are, and made of the slides' records and the text's: the text stays text.
            let mut layer = SlideTextLayer { cx: &mut cx, resources: resources.clone(), density, vector: true, last_baseline: None };
            let marks = &marks_with_numbers(options, &presentation, &numbers);
            let sizes: Vec<(f32, f32)> = slide_records.iter().map(|s| s.size_pt).collect();
            let canvas = |size| crate::export_print::VectorCanvas::new(size, &slide_records);
            let made = if options.outline_pages {
                crate::export_print::outline_pages_on(&outline_text(&presentation, &numbers), marks, &mut layer, &mut |size| canvas(size))
            } else if options.notes_pages {
                crate::export_print::notes_pages_on(&sizes, &notes_texts(&presentation, &numbers, options), options.frame, marks, &mut layer, &mut |size| canvas(size))
            } else {
                crate::export_print::handouts_on(&sizes, options.handout.unwrap_or(1), options.frame, options.handout_landscape, marks, &mut layer, &mut |size| canvas(size))
            };
            for (page, size) in made {
                writer.add_page_layers(size, page.layers);
            }
        }
        if let Some(paper) = options.paper.filter(|_| !(pages_asked && handout_paper(options).is_some())) {
            writer.fit_to_paper(paper, options.fit_paper);
        }
        pdf_writer = Some(writer);
    }
    if options.format == Format::PdfImage {
        let path = options.out_dir.join(format!("{}.pdf", options.name));
        // ノート ページ: each slide's picture above its notes.
        if options.outline_pages {
            // アウトライン: no pictures, the text of the slides.
            let text = outline_text(&presentation, &numbers);
            pages = crate::export_print::outline_pages(&text, &options.handout_marks, &mut SlideTextLayer { cx: &mut cx, resources: resources.clone(), density, vector: false, last_baseline: None });
        } else if options.notes_pages {
            let images: Vec<image::RgbaImage> = pages.into_iter().map(|(image, _)| image).collect();
            let notes = notes_texts(&presentation, &numbers, options);
            pages = crate::export_print::notes_pages(&images, &notes, &options.handout_marks, &mut SlideTextLayer { cx: &mut cx, resources: resources.clone(), density, vector: false, last_baseline: None });
        } else if let Some(per_page) = options.handout {
            // 配布資料: the slides' pictures put on pages.
            let images: Vec<image::RgbaImage> = pages.into_iter().map(|(image, _)| image).collect();
            pages = crate::export_print::handouts(&images, per_page, options.frame, options.handout_landscape, &marks_with_numbers(options, &presentation, &numbers), &mut SlideTextLayer { cx: &mut cx, resources: resources.clone(), density, vector: false, last_baseline: None });
        }
        write_pdf(&path, &pages, options.paper.filter(|_| handout_paper(options).is_none()).map(|p| (p, options.fit_paper)))?;
        println!("{}", path.display());
    }
    Ok(Rendered { writer: pdf_writer })
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
        // Excel for Mac puts the sheet's corner on a whole point of the sheet at
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
        // Excel for Windows lays the page out on its printer's pixels (600 dpi) and puts the sheet's corner (the top left of
        // the first cell's fill; its borders are centred on the cell edges) 4 of them inside the margin, at every scale; a
        // centred sheet is centred in the room as if 16 pixels taller (wider) than its cells, then put those 4 pixels in.
        // Measured on Excel 365's PDFs of probes with margins of 0.25 to 1.2 inch at 25 % to 100 %, centred or not: the
        // first fill at the margin + 0.48 pt for all, the centred ones within 0.07 pt (`tasks/doc/xls-goal-20261112.md` 7.).
        // Why 4 and 16 is not known: they are measured values the user allowed as an exception (2026-11-14,
        // docs/rule/implimentationRule.md "測った係数を使ってよい例外"); replace them when the mechanism is found.
        let windows_corner = |margin: i64, centred: bool, room: f64, whole: f64, at: f64| -> f32 {
            let pixel = PRINTER_PIXEL / 12_700.0;
            let mut want = (margin as f64 / PRINTER_PIXEL).round() + 4.0;
            if centred {
                want += ((room - whole) / PRINTER_PIXEL - 16.0).max(0.0) / 2.0;
            }
            // The window drew the corner on its own pixels (`density` a point), which the fills and lines are snapped to.
            let drawn = (at * density as f64).round() / density as f64;
            (want * pixel - drawn) as f32
        };
        let shift = if ibis_formats::EXCEL_METRICS == ibis_formats::ExcelMetrics::Windows && !print.letter_on_a4 {
            (
                windows_corner(left, print.centered.0, room.0, whole.0, (corner_at.0 - hw) * PX_PER_EMU),
                windows_corner(top, print.centered.1, room.1, whole.1, (corner_at.1 - hh) * PX_PER_EMU),
            )
        } else {
            (corner(left, print.centered.0, (corner_at.0 - hw) * PX_PER_EMU), corner(top, print.centered.1 || print.letter_on_a4, (corner_at.1 - hh) * PX_PER_EMU))
        };
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
        writer.add_page_shifted(records, f32::from(window_size.width), page_pt, density, shift);
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
pub(crate) fn pixel_density(cx: &mut HeadlessAppContext) -> Result<f32, String> {
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
fn write_pdf(path: &Path, pages: &[(image::RgbaImage, (f32, f32))], paper: Option<(crate::export_print::Paper, bool)>) -> Result<(), String> {
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
        // On a paper the page's picture is scaled and put in the middle (`Paper::fit`).
        let (pw, ph, left, up, paper_w, paper_h) = match paper {
            Some((paper, fit)) => {
                let f = paper.fit((*pw, *ph), fit);
                (pw * f.scale, ph * f.scale, f.shift.0, f.page_pt.1 - f.shift.1 - ph * f.scale, f.page_pt.0, f.page_pt.1)
            }
            None => (*pw, *ph, 0.0, 0.0, *pw, *ph),
        };
        object(
            &mut pdf,
            *id,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {paper_w:.2} {paper_h:.2}] /Resources << /XObject << /Im0 {picture} 0 R >> >> /Contents {contents} 0 R >>"
            )
            .as_bytes(),
        );
        let draw = format!("q {pw:.2} 0 0 {ph:.2} {left:.2} {up:.2} cm /Im0 Do Q");
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
    use super::{page_spans, parse_slides, slide_numbers};

    /// GitHub #189: Excel 365 prints a shape anchored by two cells (`twoCellAnchor`) from its top-left cell to its bottom-right one
    /// (its PDF of a box of 6 rows of 15 pt: 82.8 pt tall where the rows print 13.8 pt), one anchored by one cell or absolutely at
    /// its own size.
    #[test]
    fn a_two_cell_shape_is_printed_over_its_printed_cells() {
        if ibis_formats::EXCEL_METRICS != ibis_formats::ExcelMetrics::Windows {
            return;
        }
        use ibis_doc_model::{EmuRect, Shape, ShapeId, ShapeKind};
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsx-sales.xlsx");
        let mut presentation = ibis_formats::read_xlsx(&std::fs::read(path).unwrap()).unwrap();
        let screen_rows = |p: &ibis_doc_model::Presentation| p.slides[0].shapes.iter().find_map(|s| match &s.kind {
            ShapeKind::Sheet(sheet) => Some((s.frame.y, sheet.row_offsets())),
            _ => None,
        }).unwrap();
        let (top, offsets) = screen_rows(&presentation);
        let frame = EmuRect::new(0, top + offsets[2], 1_000_000, offsets[8] - offsets[2]);
        for (id, two_cell) in [(9001, true), (9002, false)] {
            let slide = &mut presentation.slides[0];
            slide.shapes.push(Shape::new(ShapeId(id), frame, ShapeKind::Line(ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0)), 12_700))));
            if two_cell {
                slide.shape_two_cell.insert(ShapeId(id));
            }
        }
        super::windows_print_geometry(&mut presentation);
        let (new_top, new_offsets) = screen_rows(&presentation);
        let find = |id| presentation.slides[0].shapes.iter().find(|s| s.id == ShapeId(id)).unwrap().frame;
        let printed = new_offsets[8] - new_offsets[2];
        assert!(printed < frame.height, "the rows print shorter than on the screen");
        assert_eq!(find(9001).height, printed, "a two-cell shape covers its printed rows");
        assert_eq!(find(9002).height, frame.height, "a one-cell shape keeps its height");
        assert_eq!(find(9001).y, new_top + new_offsets[2]);
    }

    #[test]
    fn numbers_under_the_handout_slides() {
        assert_eq!(slide_numbers(1, &[1, 3, 4, 5]), [1, 3, 4, 5]);
        assert_eq!(slide_numbers(5, &[1, 2, 3, 4]), [5, 6, 7, 8]);
    }

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
        let mut presentation = ibis_formats::read_xlsx(&std::fs::read(path).unwrap()).unwrap();
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
    /// GitHub #104: without a print area the page range of the page break preview is what the sheet uses, and it
    /// widens, as Excel's does, when a value is entered past it (a dynamic array's spill likewise).
    #[test]
    fn page_range_grows_with_entries_past_the_used_cells() {
        use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, SheetCommand};
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsx-sales.xlsx");
        let presentation = ibis_formats::read_xlsx(&std::fs::read(path).unwrap()).unwrap();
        let slide_id = presentation.slides[0].id;
        let mut document = Document::new(presentation);
        let reach = |document: &Document| {
            let slide = &document.presentation().slides[0];
            let area = super::export_area(document.presentation(), slide);
            let (_, _, pages) = super::sheet_pages(slide, area, (None, None), (0.0, 0.0)).unwrap();
            (pages.iter().map(|p| p.x + p.width).max().unwrap(), pages.iter().map(|p| p.y + p.height).max().unwrap(), pages.len())
        };
        let (width, height, count) = reach(&document);
        let put = |document: &mut Document, row: u32, column: u32, input: CellInput| {
            document.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: slide_id, edits: vec![CellEdit { row, column, input }] })]).unwrap();
        };
        // Far below and to the right of the used cells.
        put(&mut document, 400, 60, CellInput::Value(CellValue::Number(1.0)));
        let slide = &document.presentation().slides[0];
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(slide).unwrap();
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let (frame_x, frame_y) = slide.shapes.iter().find(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Sheet(_))).map(|s| (s.frame.x, s.frame.y)).unwrap();
        let (width2, height2, count2) = reach(&document);
        assert!(width2 >= frame_x + xs[61] && height2 >= frame_y + ys[401], "the pages reach the new cell: {width2}x{height2} vs {width}x{height}");
        assert!(count2 > count, "more pages: {count2} vs {count}");
        // A spill is part of the used range too.
        put(&mut document, 500, 100, CellInput::Formula("SEQUENCE(1000,80)".into()));
        let (width3, height3, _) = reach(&document);
        assert!(width3 > width2 && height3 > height2, "the spill widens it: {width3}x{height3} vs {width2}x{height2}");
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
