// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A paragraph whose runs differ in size, laid out and wrapped as one text.
//!
//! GPUI's styled text has one font size, so a paragraph that mixes sizes
//! (a large number with a small unit, a superscript note mark) is laid out
//! here: it is broken into lines across its runs, each line is as high as
//! its largest run, and the runs of a line sit on a shared baseline or line
//! up as `a:pPr/@fontAlgn` asks.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::*;
use ibis_doc_model::{FontAlign, TextAlign};

use crate::render::line_break::{Kinsoku, LatinBreaks, may_break};
use crate::render::math_view::{self, MathBox};
use crate::render::font_metrics::{POWERPOINT_SINGLE, natural_below, powerpoint_below_baseline};

/// The size of a pixel of the device Office draws a PDF on, in points: 600 dpi. Its emboldened glyphs (Excel's PDF: `0.12 Tc` on a
/// bold of a typeface without one; PowerPoint's: a bold run 18.12 pt a character) are a pixel wider.
const OFFICE_PDF_DEVICE_PIXEL_PT: f32 = 72.0 / 600.0;

/// A run of the paragraph, ready to draw.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    /// 縦中横 of vertical writing: `None` sets the digits as PowerPoint does (two by two); `Some` as the run says (Word's).
    pub(crate) tate: Option<ibis_doc_model::TateChuYoko>,
    /// The run's 文字の拡大縮小 (`w:w`) as a factor: what a 縦中横 run is squeezed by (and all of it; 行の幅に合わせる adds none).
    pub(crate) tate_scale: f32,
    pub(crate) text: String,
    pub(crate) font: Font,
    /// The size drawn, in pixels (smaller than the run's size for a
    /// superscript or subscript).
    pub(crate) size: f32,
    pub(crate) color: Hsla,
    pub(crate) underline: bool,
    /// How the underline is drawn (`a:rPr/@u`).
    pub(crate) underline_kind: ibis_doc_model::Underline,
    /// How far the run is raised above the baseline, in pixels (negative
    /// lowers it).
    pub(crate) raise: f32,
    /// Added after each character, in pixels.
    pub(crate) spacing: f32,
    /// Added after each character of a bold the typeface does not have (drawn by thickening the regular glyphs), in
    /// pixels: Office's PDF device is 600 dpi and its emboldening widens a glyph by a pixel of it (0.12 pt). Set by
    /// `lay_out` from the typeface the text is shaped with.
    pub(crate) synthetic_bold: std::cell::Cell<f32>,
    /// Background colour behind the characters (a highlighter).
    pub(crate) highlight: Option<Hsla>,
    /// Outline around the characters: colour and width in pixels.
    pub(crate) outline: Option<(Hsla, f32)>,
    /// Shadow behind the characters: colour, offset (dx, dy) and blur radius in pixels.
    pub(crate) shadow: Option<(Hsla, f32, f32, f32)>,
    /// Inner shadow inside the characters (`a:innerShdw`): colour, offset (dx, dy) and blur radius in pixels.
    pub(crate) inner_shadow: Option<(Hsla, f32, f32, f32)>,
    /// A glow round the characters: colour, radius in pixels and the characters' font size in points (the glow's size depends on it,
    /// `text_glow_scale`).
    pub(crate) glow: Option<(Hsla, f32, f32)>,
    /// The characters painted with a gradient or a pattern (`a:gradFill`,
    /// `a:pattFill`) instead of `color`, across the run's stretch of a line.
    pub(crate) fill: Option<ibis_doc_model::Fill>,
    /// The font's ascent and descent at `size`, in pixels.
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    /// A line through the characters (`a:rPr/@strike`).
    pub(crate) strike: ibis_doc_model::Strike,
    /// The run's size the strike is placed for, in pixels (the same for a
    /// small capital as for the capitals around it).
    pub(crate) strike_size: f32,
    /// A formula drawn typeset in place of `text` (which stays its one-line
    /// form, so that positions in it still count).
    pub(crate) math: Option<ibis_doc_model::math::Math>,
    /// Run border box (`w:bdr` inside a Word text box): `(color, width_px, space_px)`.
    pub(crate) border: Option<(Hsla, f32, f32)>,
    /// Fallback kana advance widths (U+3040..=U+30FF, in `1/256 em`) when
    /// substituting an uninstalled typeface such as `Meiryo UI`.
    pub(crate) kana_widths: Option<&'static [u16; 192]>,
    /// The advance widths of an uninstalled typeface (`UD デジタル 教科書体 NK`) whose characters are not as wide as the substitute font's
    /// (`fallback_widths`).
    pub(crate) advance_table: Option<&'static crate::render::fallback_widths::AdvanceTable>,
    /// Whether the piece substitutes an uninstalled proportional Japanese typeface
    /// (`ＭＳ Ｐ`, `HGP`, `Meiryo UI`), so full-width alphanumerics use proportional advances.
    pub(crate) proportional_alnum: bool,
    /// The run's language is Japanese, Chinese or Korean (`a:rPr/@lang`): its Latin commas and full stops hang with `hangingPunct`.
    pub(crate) east_asian_lang: bool,
}

impl Piece {
    /// The space added after each character, in pixels: the run's character spacing and the widening of an emboldened
    /// glyph.
    pub(crate) fn spacing_px(&self) -> f32 {
        self.spacing + self.synthetic_bold.get()
    }
}

/// How much the radius of a text's glow (`a:glow/@rad`) is scaled by in Windows PowerPoint, for characters `size_pt` points
/// high: the glow is the shape's (grown by half, blurred by a sixth) of a radius `rad * s`, and `s` depends on the font size
/// alone (not on the radius, the typeface or bold). Fitted `s = 0.0527 * size_pt^(2/3)` to 176 measurements of Windows
/// PowerPoint 16.0.20527 (8 to 130 pt, radii 2.5 to 50 pt, five typefaces, within 5 %; median per size rms 1.8 %):
/// `tasks/doc/glow-measurements-20261106.md`. Below 8 pt and above 130 pt it is an extrapolation. The user allowed this
/// fitted formula for the glow of text (2026-11-06), an exception to the rule against ad hoc factors.
pub(crate) fn text_glow_scale(size_pt: f32) -> f32 {
    0.0527 * size_pt.max(0.0).powf(2.0 / 3.0)
}

/// How much smaller PowerPoint draws a small capital than a capital.
pub(crate) const SMALL_CAPS_SCALE: f32 = 0.8;

/// `text` drawn with capitals `caps`: the pieces, each with the fraction of
/// the run's size it is drawn at. Each character becomes one (a letter whose
/// capital is several characters, like ß, is kept), so that the characters
/// keep their positions in the text.
pub(crate) fn capitals(text: &str, caps: ibis_doc_model::Caps) -> Vec<(String, f32)> {
    let upper = |c: char| {
        let mut u = c.to_uppercase();
        match (u.next(), u.next()) {
            (Some(one), None) => one,
            _ => c,
        }
    };
    match caps {
        ibis_doc_model::Caps::None => vec![(text.to_string(), 1.0)],
        ibis_doc_model::Caps::All => vec![(text.chars().map(upper).collect(), 1.0)],
        ibis_doc_model::Caps::Small => {
            let mut out: Vec<(String, f32)> = Vec::new();
            for c in text.chars() {
                let small = c.is_lowercase() && upper(c) != c;
                let k = if small { SMALL_CAPS_SCALE } else { 1.0 };
                match out.last_mut() {
                    Some((t, last)) if *last == k => t.push(upper(c)),
                    _ => out.push((upper(c).to_string(), k)),
                }
            }
            if out.is_empty() {
                out.push((String::new(), 1.0));
            }
            out
        }
    }
}

/// The lines through a piece's characters (`a:rPr/@strike`), as rectangles
/// (x, y, width, height) for a stretch from the baseline's start (`x`, `y`),
/// `advance` wide.
fn strike_rects(piece: &Piece, (x, y): (f32, f32), advance: f32) -> Vec<(f32, f32, f32, f32)> {
    let size = piece.strike_size;
    let thickness = (size * STRIKE_THICKNESS).max(0.5);
    let middle = y - size * STRIKE_HEIGHT;
    match piece.strike {
        ibis_doc_model::Strike::None => Vec::new(),
        ibis_doc_model::Strike::Single => vec![(x, middle - thickness / 2.0, advance, thickness)],
        ibis_doc_model::Strike::Double => {
            let gap = size * DOUBLE_STRIKE_GAP / 2.0;
            vec![(x, middle - gap - thickness / 2.0, advance, thickness), (x, middle + gap - thickness / 2.0, advance, thickness)]
        }
    }
}

/// Where PowerPoint puts a strike's middle above the baseline, and how thick
/// it draws it, in ems; and how far apart the middles of a double strike's
/// lines are (measured in PowerPoint's PDF: 20 pt Calibri and Yu Gothic, a
/// 1 pt line 4.35 pt above the baseline; a double strike's 2 pt apart).
const STRIKE_HEIGHT: f32 = 0.2175;
const STRIKE_THICKNESS: f32 = 0.05;
const DOUBLE_STRIKE_GAP: f32 = 0.1;

/// An underline's shape below the baseline, as PowerPoint for Mac draws it
/// (measured in its PDF at 14, 20, 28 and 32 pt, Calibri and 游ゴシック):
/// its lines are `t` thick, Calibri's underline thickness (0.0654 em) down
/// to whole points; a single line's top is the font's underline position
/// rounded to whole points below the baseline (`p`: Calibri's 0.1133 em,
/// 游ゴシック's 0.1001), a heavy one's, a double one's band's and a wave's `t`
/// higher; dashes are measured in `t` from where the underline starts.
/// Lengths here are in pixels.
enum UnderlineShape {
    /// Lines (top, height) across, dashed as `dashes` (on, off, …; none:
    /// solid).
    Lines { lines: Vec<(f32, f32)>, dashes: &'static [f32] },
    /// Zigzag bands: each band's top, how far it goes down and back (the
    /// zigzag's depth) and its vertical thickness; it peaks every `2t`.
    Wave { tops: Vec<f32>, depth: f32, thickness: f32 },
}

/// A run's underline thickness in pixels for a run `size` pixels high at
/// `px_per_point` (0: not rounded to points).
fn underline_thickness(size: f32, px_per_point: f32) -> f32 {
    const THICKNESS: f32 = 0.0654;
    if px_per_point > 0.0 {
        (size / px_per_point * THICKNESS).floor().max(1.0) * px_per_point
    } else {
        (size * THICKNESS).max(0.5)
    }
}

/// The underline position below the baseline, in ems: Calibri's, and
/// 游ゴシック's for East Asian text.
const UNDERLINE_POSITION: f32 = 0.1133;
const EAST_ASIAN_UNDERLINE_POSITION: f32 = 0.1001;

/// An underline's top below the baseline (`p` of `underline_shape`), in
/// pixels.
fn underline_position(piece: &Piece, px_per_point: f32) -> f32 {
    let em = if piece.text.chars().any(is_cjk) { EAST_ASIAN_UNDERLINE_POSITION } else { UNDERLINE_POSITION };
    let p = piece.strike_size * em;
    if px_per_point > 0.0 { (p / px_per_point).round() * px_per_point } else { p }
}

fn underline_shape(kind: ibis_doc_model::Underline, t: f32, p: f32, point: f32) -> UnderlineShape {
    use ibis_doc_model::Underline as U;
    const SOLID: &[f32] = &[];
    const DOTTED: &[f32] = &[1.0, 1.0];
    const DASH: &[f32] = &[3.0, 2.0];
    const LONG: &[f32] = &[7.0, 4.0];
    const DOT_DASH: &[f32] = &[3.0, 2.0, 1.0, 2.0];
    const DOT_DOT_DASH: &[f32] = &[3.0, 2.0, 1.0, 2.0, 1.0, 2.0];
    let thin = |dashes| UnderlineShape::Lines { lines: vec![(p, t)], dashes };
    let heavy = |dashes| UnderlineShape::Lines { lines: vec![(p - t, t)], dashes };
    match kind {
        U::Single | U::Words => thin(SOLID),
        U::Dotted => thin(DOTTED),
        U::Dash => thin(DASH),
        U::DashLong => thin(LONG),
        U::DotDash => thin(DOT_DASH),
        U::DotDotDash => thin(DOT_DOT_DASH),
        // The heavy dashed lines are as thick as the thin ones (measured).
        U::DottedHeavy => heavy(DOTTED),
        U::DashHeavy => heavy(DASH),
        U::DashLongHeavy => heavy(LONG),
        U::DotDashHeavy => heavy(DOT_DASH),
        U::DotDotDashHeavy => heavy(DOT_DOT_DASH),
        U::Heavy => UnderlineShape::Lines { lines: vec![(p - t, t + point.max(0.0))], dashes: SOLID },
        U::Double => {
            // Two lines at the top and the bottom of a band `2t` high.
            let line = (t / 2.0).max(point.min(t));
            UnderlineShape::Lines { lines: vec![(p - t, line), (p + t - line, line)], dashes: SOLID }
        }
        U::Wavy => UnderlineShape::Wave { tops: vec![p - t], depth: t, thickness: t },
        U::WavyHeavy => UnderlineShape::Wave { tops: vec![p - t], depth: t, thickness: 2.0 * t },
        U::WavyDouble => UnderlineShape::Wave { tops: vec![p - 2.0 * t, p], depth: t, thickness: t },
    }
}

/// Paint a stretch's underline and strike: from the baseline's start
/// (`x`, `y`), `advance` wide, the underline's dashes and waves running on
/// from `origin`; turned by `turn`.
fn paint_decorations(piece: &Piece, (x, y): (f32, f32), advance: f32, origin: f32, px_per_point: f32, turn: Option<Turn>, window: &mut Window) {
    let rect = |window: &mut Window, (rx, ry, rw, rh): (f32, f32, f32, f32)| match turn {
        Some(turn) => paint_turned_rect(window, (rx, ry, rw, rh), piece.color, turn),
        None => window.paint_quad(fill(Bounds::new(point(px(rx), px(ry)), size(px(rw), px(rh))), piece.color)),
    };
    if piece.underline && advance > 0.0 {
        let t = underline_thickness(piece.strike_size, px_per_point);
        let unit = t;
        let end = x + advance;
        match underline_shape(piece.underline_kind, t, underline_position(piece, px_per_point), px_per_point) {
            UnderlineShape::Lines { lines, dashes } => {
                for (top, height) in lines {
                    let top = y + top;
                    if dashes.is_empty() {
                        rect(window, (x, top, advance, height));
                        continue;
                    }
                    let period: f32 = dashes.iter().sum::<f32>() * unit;
                    let mut at = origin + ((x - origin) / period).floor() * period;
                    while at < end {
                        let mut d = at;
                        for (k, len) in dashes.iter().enumerate() {
                            let next = d + len * unit;
                            if k % 2 == 0 {
                                let (a, b) = (d.max(x), next.min(end));
                                if b > a {
                                    rect(window, (a, top, b - a, height));
                                }
                            }
                            d = next;
                        }
                        at += period;
                    }
                }
            }
            UnderlineShape::Wave { tops, depth, thickness } => {
                // A zigzag band: its top edge at a peak every `2t` from
                // where the underline starts, `depth` lower between, its
                // bottom edge `thickness` below its top edge.
                let half = unit;
                for top in tops {
                    let top = y + top;
                    let at = |a: f32| {
                        let phase = ((a - origin) / half).rem_euclid(2.0);
                        top + depth * (1.0 - (phase - 1.0).abs())
                    };
                    let mut xs = vec![x];
                    let mut k = ((x - origin) / half).floor() + 1.0;
                    while origin + k * half < end {
                        xs.push(origin + k * half);
                        k += 1.0;
                    }
                    xs.push(end);
                    let upper: Vec<(f32, f32)> = xs.iter().map(|&a| (a, at(a))).collect();
                    let mut corners: Vec<(f32, f32)> = upper.clone();
                    corners.extend(upper.iter().rev().map(|&(a, b)| (a, b + thickness)));
                    let corners: Vec<Point<Pixels>> = corners
                        .into_iter()
                        .map(|(a, b)| {
                            let (a, b) = turn.map_or((a, b), |t| t.apply(a, b));
                            point(px(a), px(b))
                        })
                        .collect();
                    if let Some(path) = crate::render::shape_view::polygon(&corners, None, true) {
                        window.paint_path(path, piece.color);
                    }
                }
            }
        }
    }
    for r in strike_rects(piece, (x, y), advance) {
        rect(window, r);
    }
}

/// The height of a line.
#[derive(Clone, Copy, Debug)]
pub(crate) enum LineRule {
    /// A multiple of the line's largest size.
    Multiple(f32),
    /// Exactly this many pixels.
    Fixed(f32),
}

#[derive(Clone, Debug)]
pub(crate) struct RichParagraph {
    pub(crate) pieces: Vec<Piece>,
    pub(crate) align: TextAlign,
    pub(crate) font_align: FontAlign,
    pub(crate) line: LineRule,
    /// The pitch of the line grid the paragraph is set on, in pixels (`Paragraph::grid_pitch`).
    pub(crate) grid_pitch: Option<f32>,
    pub(crate) wrap: bool,
    /// Where the first line starts, from the element's left edge (negative
    /// for a hanging indent); the other lines start at the edge.
    pub(crate) first_line: f32,
    /// Left margin of the paragraph (`a:pPr/@marL`), in pixels.
    pub(crate) margin_left: f32,
    /// Tab stops (`a:pPr/a:tabLst`), in pixels from the text body's left inset.
    pub(crate) tabs: Vec<f32>,
    /// Default tab stop interval (`a:pPr/@defTabSz`), in pixels (0: unused).
    pub(crate) default_tab: f32,
    /// `a:pPr/@hangingPunct`: 、。，．,. may hang past the end of a line.
    pub(crate) hanging_punct: bool,
    /// `a:pPr/@latinLnBrk`: a line may break inside a Latin word.
    pub(crate) latin_line_break: bool,
    /// The width the lines are broken to, in pixels, when it is known
    /// exactly (else the element's width, which the layout rounds to
    /// pixels, so that the lines would break differently at each zoom).
    pub(crate) room: Option<f32>,
    /// Pixels per point: the baselines are put on whole points from the
    /// top of the text, as PowerPoint puts them (0: not rounded).
    pub(crate) px_per_point: f32,
    /// The text the paragraph is in, which places it and which its
    /// baselines are rounded from the top of (`None`: its own top).
    pub(crate) text_top: Option<TextTop>,
    /// The bullet, when the text is turned (`TextBlock::turn`).
    pub(crate) bullet: Option<BulletMark>,
    /// Where the paragraph's characters and lines are painted, written at
    /// each paint for a caret, a selection or a click (`None`: not kept).
    pub(crate) positions: Option<Rc<RefCell<ParagraphPositions>>>,
    /// Whether this is the last paragraph of its text: its last line ends
    /// at its font's descent below the baseline instead of at the line's
    /// bottom (PowerPoint's text is as high as that, for a text centred or
    /// at the bottom of its box).
    pub(crate) last: bool,
    /// Word's lines (`TextBody::word_lines`): a justified paragraph's last line is not spread.
    pub(crate) word_lines: bool,
    /// East Asian vertical writing (`eaVert`): the text is turned a quarter
    /// turn, its East Asian characters drawn upright.
    pub(crate) upright: bool,
    /// Lines from left to right in vertical writing (`mongolianVert`):
    /// placed mirrored across the text's area.
    pub(crate) mirror: bool,
    /// Where the paragraph's text is meant to start (window pixels, unrounded
    /// by layout): corrects for GPUI layout snapping to whole device pixels.
    pub(crate) at_x: Option<f32>,
}

thread_local! {
    /// Where the window's origin is from the slide's, in pixels: a picture of a page (a sheet's print, with its margin and the part of the
    /// sheet it shows) draws the slide offset in the window, so `at_x` (from the slide's edge) is moved by as much.
    static AT_X_OFFSET: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

pub(crate) fn set_at_x_offset(offset: f32) {
    AT_X_OFFSET.with(|o| o.set(offset));
}

pub(crate) fn at_x_offset() -> f32 {
    AT_X_OFFSET.with(|o| o.get())
}

/// The paragraphs of one text, placed together: the layout rounds each
/// paragraph's height to pixels, which adds up over the paragraphs (and
/// moves a text centred or at the bottom of its box), so the paragraphs
/// are painted from their unrounded heights instead.
#[derive(Clone, Debug, Default)]
pub(crate) struct TextBlock {
    /// Each paragraph's height with its spaces above and below (pixels,
    /// unrounded), recorded as it is prepainted.
    pub(crate) heights: Rc<RefCell<Vec<f32>>>,
    /// Where the text's first line starts (window pixels), set after the
    /// paragraphs are prepainted (`TextBlock::place`).
    pub(crate) top: Rc<std::cell::Cell<f32>>,
    /// Where the next paragraph starts below `top`, pixels (as they are
    /// painted in order).
    pub(crate) next: Rc<std::cell::Cell<f32>>,
    /// Text in columns (`numCol`): the height of a column, the distance from one column's left edge to the next's, and
    /// the number of columns (pixels). The paragraphs go down a column to its end and on from the top of the next.
    pub(crate) flow: Rc<std::cell::Cell<Option<(f32, f32, usize)>>>,
    /// The column the paragraph painted last is in.
    pub(crate) column: Rc<std::cell::Cell<usize>>,
    /// How the text is turned (window pixels), set with `top`.
    pub(crate) turn: Rc<std::cell::Cell<Option<Turn>>>,
    /// The area the text is placed in (top and height, window pixels), set
    /// with `top`: lines laid out from left to right (`mongolianVert`) are
    /// mirrored in it.
    pub(crate) area: Rc<std::cell::Cell<(f32, f32)>>,
    /// Text bent to a preset warp (`a:prstTxWarp`): the glyphs and lines collected as the paragraphs paint,
    /// painted bent when the last is done (`paint_warp`). `None`: the text is painted as it is.
    pub(crate) warp: Rc<RefCell<Option<Warp>>>,
}

/// What a text bent to a warp collects while it is painted, in window pixels.
#[derive(Debug)]
pub(crate) struct Warp {
    pub(crate) geometry: ibis_doc_model::FormulaGeometry,
    /// Each glyph's contours with the colour of its text.
    pub(crate) glyphs: Vec<(Vec<Vec<(f32, f32)>>, Hsla, usize)>,
    /// Each line's left and right ends, top and bottom, and its baseline.
    pub(crate) lines: Vec<(f32, f32, f32, f32, f32)>,
}

impl Warp {
    pub(crate) fn new(geometry: ibis_doc_model::FormulaGeometry) -> Self {
        Self { geometry, glyphs: Vec::new(), lines: Vec::new() }
    }
}

/// A point `u` (0 to 1) of the way along the polyline `curve`, by its length.
fn along(curve: &[(f32, f32)], lengths: &[f32], u: f32) -> (f32, f32) {
    let total = *lengths.last().unwrap_or(&0.0);
    if curve.is_empty() {
        return (0.0, 0.0);
    }
    if total <= 0.0 {
        return curve[0];
    }
    let target = u.clamp(0.0, 1.0) * total;
    let k = lengths.partition_point(|&l| l < target).clamp(1, curve.len() - 1);
    let (a, b) = (curve[k - 1], curve[k]);
    let span = lengths[k] - lengths[k - 1];
    let f = if span > 0.0 { (target - lengths[k - 1]) / span } else { 0.0 };
    (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)
}

/// The cumulative lengths of the polyline `curve`.
fn cumulative(curve: &[(f32, f32)]) -> Vec<f32> {
    let mut out = Vec::with_capacity(curve.len());
    let mut total = 0.0;
    for (i, p) in curve.iter().enumerate() {
        if i > 0 {
            total += (p.0 - curve[i - 1].0).hypot(p.1 - curve[i - 1].1);
        }
        out.push(total);
    }
    out
}

/// A text along one guide curve (an arch): its baseline runs along the curve, scaled so that the text
/// is as long as the curve, each glyph standing at right angles to it.
fn paint_warp_on_path(window: &mut Window, warp: &Warp, (ax, ay, _aw, _ah): (f32, f32, f32, f32), curve: &[(f32, f32)], on_curve: &dyn Fn(usize) -> bool) {
    let lengths = cumulative(curve);
    let total = *lengths.last().unwrap_or(&0.0);
    let (x0, x1) = warp.glyphs.iter().filter(|g| on_curve(g.2)).flat_map(|(g, _, _)| g.iter().flatten()).fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    if total <= 1.0 || x1 <= x0 {
        return;
    }
    // The first line's baseline is the curve.
    let baseline = warp.lines.iter().enumerate().find(|(i, _)| on_curve(*i)).map_or(0.0, |(_, l)| l.4);
    let k = 1.0f32;
    let map = |(x, y): (f32, f32)| -> (f32, f32) {
        // The text keeps its size, its middle at the curve's middle.
        let s = (total / 2.0 + (x - (x0 + x1) / 2.0) * k).clamp(0.0, total);
        let at = along(curve, &lengths, s / total);
        let before = along(curve, &lengths, ((s - 0.5) / total).max(0.0));
        let after = along(curve, &lengths, ((s + 0.5) / total).min(1.0));
        let (tx, ty) = (after.0 - before.0, after.1 - before.1);
        let norm = tx.hypot(ty).max(1e-6);
        // Up from the baseline is at right angles to the way along the curve, to the left of it.
        let (ux, uy) = (ty / norm, -tx / norm);
        let up = (baseline - y) * k;
        (at.0 + ux * up, at.1 + uy * up)
    };
    // The bent text is as wide as the area (or as high), kept in its proportions: against the top for
    // a curve in the upper half, the bottom for one in the lower half (measured against PowerPoint).
    let mut shaped: Vec<(Vec<(f32, f32)>, usize)> = Vec::new();
    let mut colours: Vec<Hsla> = Vec::new();
    for (polygons, color, _) in warp.glyphs.iter().filter(|g| on_curve(g.2)) {
        let slot = match colours.iter().position(|c| c == color) {
            Some(i) => i,
            None => {
                colours.push(*color);
                colours.len() - 1
            }
        };
        for polygon in polygons {
            let mut points: Vec<(f32, f32)> = Vec::with_capacity(polygon.len() * 2);
            for (i, &a) in polygon.iter().enumerate() {
                let b = polygon[(i + 1) % polygon.len()];
                let n = ((b.0 - a.0).hypot(b.1 - a.1) / 2.0).ceil().max(1.0) as usize;
                for j in 0..n {
                    let f = j as f32 / n as f32;
                    points.push(map((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)));
                }
            }
            shaped.push((points, slot));
        }
    }
    // (Where the curve is, in the area: the text is not moved to fit.)
    let (fit, ox, oy) = (1.0f32, ax, ay);
    let mut grouped: Vec<(Hsla, Vec<(Vec<Point<Pixels>>, bool)>)> = colours.into_iter().map(|c| (c, Vec::new())).collect();
    for (points, slot) in shaped {
        grouped[slot].1.push((points.into_iter().map(|(x, y)| point(px(ox + x * fit), px(oy + y * fit))).collect(), true));
    }
    for (color, subpaths) in grouped {
        if let Some(path) = crate::render::shape_view::compound_path(&subpaths, None) {
            window.paint_path(path, color);
        }
    }
}

/// Paint the glyphs collected in `warp`, bent: the box the text fills (the lines' union) is stretched over
/// the region between the warp's two guide curves, `area` (left, top, width, height in window pixels)
/// being the box the guides are drawn in. Nothing is painted if the warp has no two guide curves.
pub(crate) fn paint_warp(window: &mut Window, warp: &Warp, (ax, ay, aw, ah): (f32, f32, f32, f32)) {
    if warp.glyphs.is_empty() || warp.lines.is_empty() {
        return;
    }
    let outlines = warp.geometry.outline(aw as f64, ah as f64);
    let curves: Vec<Vec<(f32, f32)>> = outlines.iter().filter_map(|o| o.subpaths.first()).map(|s| s.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect()).collect();
    if curves.len() == 1 && curves[0].len() >= 2 {
        paint_warp_on_path(window, warp, (ax, ay, aw, ah), &curves[0], &|_| true);
        return;
    }
    // A button: an arch, a straight line and an arch, a line of text on each in turn.
    if curves.len() == 3 && curves.iter().all(|c| c.len() >= 2) {
        for (n, curve) in curves.iter().enumerate() {
            paint_warp_on_path(window, warp, (ax, ay, aw, ah), curve, &|line| line.min(2) == n);
        }
        return;
    }
    if curves.len() < 2 || curves.len() % 2 != 0 || curves.iter().any(|c| c.len() < 2) {
        return;
    }
    let lengths: Vec<Vec<f32>> = curves.iter().map(|c| cumulative(c)).collect();
    // The curves' own box is stretched to the area (the waves' swing is not left empty above and below).
    let (cx0, cx1) = curves.iter().flatten().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.0), hi.max(p.0)));
    let (cy0, cy1) = curves.iter().flatten().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
    let _ = (cx0, cx1, cy0, cy1, aw, ah);
    let (fx, fy) = (1.0f32, 1.0f32);
    let (cx0, cy0) = (0.0f32, 0.0f32);
    // Each pair of curves takes the lines in turn (the first pair the first line, the second pair the
    // second line, ...; the last pair the rest).
    let pairs = curves.len() / 2;
    let line_count = warp.lines.len();
    let pair_of = |line: usize| -> usize { if pairs == 1 { 0 } else { line.min(pairs - 1) } };
    let _ = line_count;
    // By colour: one path each.
    let mut colours: Vec<(Hsla, Vec<(Vec<Point<Pixels>>, bool)>)> = Vec::new();
    for pair in 0..pairs {
        let members: Vec<&(Vec<Vec<(f32, f32)>>, Hsla, usize)> = warp.glyphs.iter().filter(|g| pair_of(g.2) == pair).collect();
        // What is stretched over the region is the ink (the glyphs' own box), not the lines' boxes.
        let (_, _, y0, y1) = members.iter().flat_map(|(g, _, _)| g.iter().flatten()).fold((f32::MAX, f32::MIN, f32::MAX, f32::MIN), |(a, b, c, d), p| (a.min(p.0), b.max(p.0), c.min(p.1), d.max(p.1)));
        // (Across, all the lines share the whole text's width.)
        let (x0, x1, _, _) = warp.glyphs.iter().flat_map(|(g, _, _)| g.iter().flatten()).fold((f32::MAX, f32::MIN, f32::MAX, f32::MIN), |(a, b, c, d), p| (a.min(p.0), b.max(p.0), c.min(p.1), d.max(p.1)));
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let (bw, bh) = (x1 - x0, y1 - y0);
        let (top, bottom, lt, lb) = (&curves[pair * 2], &curves[pair * 2 + 1], &lengths[pair * 2], &lengths[pair * 2 + 1]);
        let map = |(x, y): (f32, f32)| -> Point<Pixels> {
            let (u, v) = ((x - x0) / bw, (y - y0) / bh);
            let (ta, tb) = (along(top, lt, u), along(bottom, lb, u));
            let (mx, my) = (ta.0 + (tb.0 - ta.0) * v, ta.1 + (tb.1 - ta.1) * v);
            point(px(ax + (mx - cx0) * fx), px(ay + (my - cy0) * fy))
        };
        for (polygons, color, _) in members {
            let slot = match colours.iter().position(|(c, _)| c == color) {
                Some(i) => i,
                None => {
                    colours.push((*color, Vec::new()));
                    colours.len() - 1
                }
            };
            for polygon in polygons {
                // Long edges cut short, for the bend.
                let mut points: Vec<Point<Pixels>> = Vec::with_capacity(polygon.len() * 2);
                for (i, &a) in polygon.iter().enumerate() {
                    let b = polygon[(i + 1) % polygon.len()];
                    let n = ((b.0 - a.0).hypot(b.1 - a.1) / 2.0).ceil().max(1.0) as usize;
                    for k in 0..n {
                        let f = k as f32 / n as f32;
                        points.push(map((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f)));
                    }
                }
                colours[slot].1.push((points, true));
            }
        }
    }
    for (color, subpaths) in colours {
        if let Some(path) = crate::render::shape_view::compound_path(&subpaths, None) {
            window.paint_path(path, color);
        }
    }
}

impl TextBlock {
    /// Put the text in the area from `top` down `height` (window pixels),
    /// `anchor` of the way down the room left (0 top, 0.5 middle, 1 bottom).
    pub(crate) fn place(&self, top: f32, height: f32, anchor: f32) {
        let text: f32 = self.heights.borrow().iter().sum();
        // (Text in columns starts at the top: its height is not one column's.)
        self.top.set(if self.flow.get().is_some() { top } else { top + (height - text) * anchor });
        self.column.set(0);
        self.area.set((top, height));
        self.next.set(0.0);
    }

    /// Turn the text as `turn` says, its area's middle at (`cx`, `cy`).
    pub(crate) fn turn(&self, cx: f32, cy: f32, turn: Option<TextTurn>) {
        let turn = turn
            .map(|t| {
                let mut trn = Turn::about(cx, cy, t.text).then(Turn::about(cx + t.frame.0, cy + t.frame.1, t.shape));
                trn.shape_turned = t.shape_turned;
                trn
            })
            .filter(Turn::turns);
        self.turn.set(turn);
    }
}

/// A turn of painted text, in window pixels: a rotation about some point
/// (`p` → `R·p + t`, clockwise as the screen's y runs down). Text is never
/// mirrored: PowerPoint turns a flipped shape's text instead.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Turn {
    pub(crate) cos: f32,
    pub(crate) sin: f32,
    pub(crate) tx: f32,
    pub(crate) ty: f32,
    pub(crate) shape_turned: bool,
}

impl Turn {
    /// `degrees` clockwise about (`cx`, `cy`).
    pub(crate) fn about(cx: f32, cy: f32, degrees: f32) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self { cos, sin, tx: cx - cos * cx + sin * cy, ty: cy - sin * cx - cos * cy, shape_turned: false }
    }

    /// This turn, then `next`.
    pub(crate) fn then(self, next: Turn) -> Self {
        let (tx, ty) = next.apply(self.tx, self.ty);
        Self {
            cos: next.cos * self.cos - next.sin * self.sin,
            sin: next.sin * self.cos + next.cos * self.sin,
            tx,
            ty,
            shape_turned: self.shape_turned || next.shape_turned,
        }
    }

    /// Where the turn takes (`x`, `y`).
    pub(crate) fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.cos * x - self.sin * y + self.tx, self.sin * x + self.cos * y + self.ty)
    }

    /// The point the turn takes to (`x`, `y`).
    pub(crate) fn invert(&self, x: f32, y: f32) -> (f32, f32) {
        let (dx, dy) = (x - self.tx, y - self.ty);
        (self.cos * dx + self.sin * dy, -self.sin * dx + self.cos * dy)
    }

    pub(crate) fn radians(&self) -> f32 {
        self.sin.atan2(self.cos)
    }

    /// Whether it moves anything.
    pub(crate) fn turns(&self) -> bool {
        self.sin.abs() > 1e-6 || self.cos < 0.0 || self.tx.abs() > 1e-3 || self.ty.abs() > 1e-3
    }


}

/// How a text is turned with its shape (`TextBlock::place`): `text`
/// degrees about the middle of its area, then `shape` degrees about the
/// shape's middle, `frame` pixels from the area's middle (before turning).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextTurn {
    pub(crate) text: f32,
    pub(crate) shape: f32,
    pub(crate) frame: (f32, f32),
    pub(crate) shape_turned: bool,
}

/// A paragraph's bullet, painted with turned text (unturned text has its
/// bullet as an element of its own): its text and font, `x` from the
/// paragraph's left edge, and `drop` below the first line's baseline.
#[derive(Clone, Debug)]
pub(crate) struct BulletMark {
    pub(crate) text: String,
    pub(crate) font: Font,
    pub(crate) size: f32,
    pub(crate) color: Hsla,
    pub(crate) x: f32,
    pub(crate) drop: f32,
}

/// A paragraph's place in its text (`RichParagraph::text_top`).
#[derive(Clone, Debug)]
pub(crate) struct TextTop {
    pub(crate) block: TextBlock,
    pub(crate) index: usize,
    /// The space above and below this paragraph, pixels.
    pub(crate) before: f32,
    pub(crate) after: f32,
}

/// Where a painted paragraph's characters and lines are, in window
/// pixels, for drawing a caret or a selection and for finding the
/// character at a click; the same for every format.
///
/// Characters are counted as the document model counts them
/// (`TextBody::splice`): the Unicode scalar values of the paragraph's text,
/// its runs' texts joined in order. An empty paragraph has one line and no
/// characters.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // For the formats' text editing, which is moving to it.
pub(crate) struct ParagraphPositions {
    /// The paragraph's element.
    pub(crate) bounds: Bounds<Pixels>,
    /// Each character's left edge and advance, and its line.
    pub(crate) chars: Vec<CharPosition>,
    pub(crate) lines: Vec<LinePosition>,
    /// How the text is turned. `bounds`, `chars` and `lines` are in the
    /// text's own coordinates: the window's before this turn (the same
    /// when it is `None`). Map them with `to_window` to draw (a turned
    /// caret or selection is a turned rectangle), and a click with
    /// `text_point` before `index_at`.
    pub(crate) turn: Option<Turn>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CharPosition {
    pub(crate) x: f32,
    pub(crate) advance: f32,
    pub(crate) line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LinePosition {
    /// The characters of the line (`start..end`; the line's trailing spaces
    /// are in it).
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// Where the line starts (after alignment and indent) and its top,
    /// baseline and height.
    pub(crate) x: f32,
    pub(crate) top: f32,
    pub(crate) baseline: f32,
    pub(crate) height: f32,
    /// The column the line is in (text in columns, `numCol`; 0 otherwise) and its left and right edges.
    pub(crate) column: usize,
    pub(crate) left: f32,
    pub(crate) right: f32,
}

#[allow(dead_code)] // For the formats' text editing, which is moving to it.
impl ParagraphPositions {
    /// (`x`, `y`) of the text's coordinates in the window.
    pub(crate) fn to_window(&self, x: f32, y: f32) -> (f32, f32) {
        self.turn.map_or((x, y), |t| t.apply(x, y))
    }

    /// A point of the window (`x`, `y`) in the text's coordinates.
    pub(crate) fn text_point(&self, x: f32, y: f32) -> (f32, f32) {
        self.turn.map_or((x, y), |t| t.invert(x, y))
    }

    /// The four corners (window pixels, clockwise from the top left before
    /// the turn) of a rectangle of the text's coordinates.
    pub(crate) fn corners(&self, x: f32, y: f32, w: f32, h: f32) -> [(f32, f32); 4] {
        [self.to_window(x, y), self.to_window(x + w, y), self.to_window(x + w, y + h), self.to_window(x, y + h)]
    }

    /// Where a caret before character `index` goes (`index` = the number
    /// of characters: after the last): its x and line.
    pub(crate) fn caret(&self, index: usize) -> (f32, usize) {
        if let Some(c) = self.chars.get(index) {
            // At a line's start, the caret is on that line.
            return (c.x, c.line);
        }
        match self.chars.last() {
            Some(c) => (c.x + c.advance, c.line),
            None => (self.lines.first().map_or(f32::from(self.bounds.origin.x), |l| l.x), 0),
        }
    }

    /// The character a click at (`x`, `y`) puts the caret before: on the
    /// line at `y` (the nearest one), before the character whose middle is
    /// right of `x` (after the line's last character when none is; a
    /// wrapped line's end is before its first character on the next line,
    /// so it is the end of its text without its trailing space).
    pub(crate) fn index_at(&self, x: f32, y: f32) -> usize {
        let column = self.column_at(x);
        let Some(line) = self
            .lines
            .iter()
            .filter(|l| l.column == column)
            .find(|l| y < l.top + l.height)
            .or(self.lines.iter().rfind(|l| l.column == column))
        else {
            return 0;
        };
        self.index_in_line(x, line)
    }

    /// The column of this paragraph's lines at `x`: the one whose edges `x` is between, else the nearest.
    pub(crate) fn column_at(&self, x: f32) -> usize {
        let distance = |l: &LinePosition| if x < l.left { l.left - x } else { (x - l.right).max(0.0) };
        self.lines
            .iter()
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
            .map_or(0, |l| l.column)
    }

    /// The character before which a click at `x` on `line` puts the caret (see `index_at`).
    pub(crate) fn index_in_line(&self, x: f32, line: &LinePosition) -> usize {
        let wrapped = line.end < self.chars.len();
        for i in line.start..line.end {
            let c = self.chars[i];
            if x < c.x + c.advance / 2.0 {
                return i;
            }
        }
        if wrapped && line.end > line.start { line.end - 1 } else { line.end }
    }
}

/// Where the paragraphs of a text body are painted
/// (`shape_view::text_body_element`): one `ParagraphPositions` a paragraph,
/// in order, each written as its paragraph is painted.
#[derive(Clone, Debug, Default)]
pub(crate) struct TextPositions {
    pub(crate) paragraphs: Rc<RefCell<Vec<Rc<RefCell<ParagraphPositions>>>>>,
}

impl TextPositions {
    /// The slot of each of `count` paragraphs (made or dropped to match).
    pub(crate) fn slots(&self, count: usize) -> Vec<Rc<RefCell<ParagraphPositions>>> {
        let mut list = self.paragraphs.borrow_mut();
        list.resize_with(count, Default::default);
        list.clone()
    }
}

/// One character of the paragraph.
#[derive(Clone, Copy, Debug)]
struct Char {
    piece: usize,
    /// Byte range in the piece's text.
    start: usize,
    end: usize,
    c: char,
    /// Unspaced advance from the shaper.
    raw: f32,
    advance: f32,
    /// A typeset formula's ascent and descent, in place of the piece's.
    lift: Option<(f32, f32)>,
    /// In East Asian vertical writing, an ASCII digit that stands upright
    /// in a cell of its own (`Tcy`): the first of a pair, the second of a
    /// pair (which takes no room), or a digit alone.
    tcy: Tcy,
    /// The piece's run is in an East Asian language (`Piece::east_asian_lang`).
    east_asian: bool,
}

/// How an ASCII digit of East Asian vertical writing is set (PowerPoint
/// sets the digits of a run two by two side by side upright, each pair in
/// one cell of the column, a last digit alone upright in a cell).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Tcy {
    #[default]
    No,
    First,
    Second,
    Alone,
    /// A character of a run set horizontal in vertical as a whole (Word's 縦中横): the first takes the cell of one em, the others none.
    Whole,
}

#[derive(Clone, Debug)]
struct Line {
    /// Range in the paragraph's characters.
    start: usize,
    end: usize,
    /// Without trailing spaces.
    width: f32,
    height: f32,
    /// Of the baseline, from the top of the line.
    baseline: f32,
    /// How far below the baseline the line's font reaches in single
    /// spacing (where the text ends after its last line).
    natural: f32,
}

#[derive(Default)]
struct Layout {
    width: Option<f32>,
    chars: Vec<Char>,
    lines: Vec<Line>,
    /// The typeset formula of each piece that has one.
    maths: Vec<Option<MathBox>>,
}

pub(crate) fn rich_paragraph(paragraph: RichParagraph) -> RichParagraphElement {
    RichParagraphElement { paragraph: Rc::new(paragraph), layout: Default::default() }
}

pub(crate) struct RichParagraphElement {
    paragraph: Rc<RichParagraph>,
    layout: Rc<RefCell<Layout>>,
}

impl IntoElement for RichParagraphElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for RichParagraphElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        _cx: &mut App,
    ) -> (LayoutId, ()) {
        let paragraph = self.paragraph.clone();
        let layout = self.layout.clone();
        let id = window.request_measured_layout(Style::default(), move |known, available, window, _| {
            let width = if paragraph.wrap {
                known.width.or(match available.width {
                    AvailableSpace::Definite(w) => Some(w),
                    _ => None,
                })
            } else {
                None
            };
            let mut layout = layout.borrow_mut();
            *layout = lay_out(&paragraph, width.map(f32::from), window);
            let widest = layout.lines.iter().map(|l| l.width).fold(0.0, f32::max);
            let height = text_height(&paragraph, &layout.lines);
            size(known.width.unwrap_or(px(widest.ceil())), px(height))
        });
        (id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) {
        let paragraph = &self.paragraph;
        let box_width = f32::from(bounds.size.width);
        let stale = paragraph.wrap && self.layout.borrow().width.is_none_or(|w| (w - box_width).abs() > 0.5);
        if stale {
            *self.layout.borrow_mut() = lay_out(paragraph, paragraph.wrap.then_some(box_width), window);
        }
        if let Some(t) = &paragraph.text_top {
            let mut heights = t.block.heights.borrow_mut();
            if heights.len() <= t.index {
                heights.resize(t.index + 1, 0.0);
            }
            heights[t.index] = t.before + text_height(paragraph, &self.layout.borrow().lines) + t.after;
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let paragraph = &self.paragraph;
        let box_width = f32::from(bounds.size.width);
        let stale = {
            let layout = self.layout.borrow();
            paragraph.wrap && layout.width.is_none_or(|w| (w - box_width).abs() > 0.5)
        };
        if stale {
            *self.layout.borrow_mut() = lay_out(paragraph, paragraph.wrap.then_some(box_width), window);
        }
        let layout = self.layout.borrow();
        let origin = f32::from(bounds.origin.y);
        // Baselines are rounded from the top of the text, the paragraph
        // starting where the ones above end (not where the layout, rounded
        // to pixels, puts it).
        // Text in columns goes down a column line by line: a line that does not fit under the ones above starts the next column
        // (a paragraph can be split between columns).
        let flow = paragraph.text_top.as_ref().and_then(|t| t.block.flow.get().map(|f| (t, f)));
        let (text_top, mut top) = match &paragraph.text_top {
            Some(t) => (t.block.top.get(), t.block.next.get() + t.before),
            None => (origin, 0.0),
        };
        let paragraph_top = top;
        let mut consumed = 0.0;
        if let Some(t) = &paragraph.text_top {
            t.block.next.set(top + text_height(paragraph, &layout.lines) + t.after);
        }
        let turn = paragraph.text_top.as_ref().and_then(|t| t.block.turn.get());
        let mut positions = paragraph.positions.as_ref().map(|_| ParagraphPositions { bounds, turn, ..Default::default() });
        let snap_device = |v: f32, sf: f32| {
            let d = v * sf;
            (if (d - d.trunc()).abs() == 0.5 { d.trunc() } else { d.round() }) / sf
        };
        let x_correction = match (paragraph.at_x, paragraph.align) {
            // Where the text is meant to start, not where the box was snapped to (the box sits on its parent's snapped edge plus its
            // padding, which can round the other way than the exact position does: half a pixel off).
            (Some(at_x), TextAlign::Left) if (at_x - f32::from(bounds.origin.x)).abs() <= 1.5 => at_x - f32::from(bounds.origin.x),
            _ => 0.0,
        };
        let line_count = layout.lines.len();
        for (n, line) in layout.lines.iter().enumerate() {
            let mut column_shift = 0.0;
            if let Some((t, (column_height, pitch, count))) = flow {
                if top > 0.5 && top + line.height > column_height + 0.5 && t.block.column.get() + 1 < count {
                    t.block.column.set(t.block.column.get() + 1);
                    consumed += top;
                    top = 0.0;
                }
                column_shift = t.block.column.get() as f32 * pitch;
            }
            let start = if n == 0 { paragraph.first_line } else { 0.0 };
            let room = if paragraph.room == Some(0.0) { 0.0 } else { box_width } - start;
            // 両端揃え (but the last line) and 均等割り付け: the room left
            // shared out between the characters that show (a line of one is
            // centred).
            let shown_chars = layout.chars[line.start..line.end].iter().rposition(|c| !c.c.is_whitespace()).map_or(0, |k| k + 1);
            let shown_slice = &layout.chars[line.start..line.start + shown_chars];
            let shown_width: f32 = shown_slice.iter().map(|c| c.advance).sum();
            // A comma or full stop that hangs past the edge (the line broke because it did not fit) is not spread over the room: the characters before it
            // are, and it follows the last of them (PowerPoint's justified line of 032 p10 ends past the edge with its comma).
            let hang = shown_slice.last().filter(|c| paragraph.hanging_punct && hangs(c.c) && shown_width > room + 0.01 && shown_chars > 2).map_or(0.0, |c| c.advance);
            let spread_chars = shown_chars - usize::from(hang > 0.0);
            let spread_width = shown_width - hang;
            let spread = match paragraph.align {
                TextAlign::Distributed => true,
                TextAlign::Justify => n + 1 < line_count || (!paragraph.word_lines && !paragraph.last && paragraph.text_top.as_ref().is_some_and(|t| t.after == 0.0)),
                _ => false,
            } && room > spread_width
                && paragraph.room != Some(0.0);
            let word_spaces = if spread && paragraph.align == TextAlign::Justify && !shown_slice.iter().any(|c| is_cjk(c.c)) {
                shown_slice.iter().filter(|c| c.c == ' ').count()
            } else {
                0
            };
            let space_extra = if word_spaces > 0 { (room - spread_width) / word_spaces as f32 } else { 0.0 };
            let extra = if spread && word_spaces == 0 && spread_chars > 1 { (room - spread_width) / (spread_chars - 1) as f32 } else { 0.0 };
            let char_extra = |k: usize| -> f32 {
                if !spread || k >= line.start + spread_chars {
                    0.0
                } else if word_spaces > 0 {
                    if layout.chars[k].c == ' ' { space_extra } else { 0.0 }
                } else if k + 1 < line.start + spread_chars {
                    extra
                } else {
                    0.0
                }
            };
            let mut x = f32::from(bounds.origin.x)
                + column_shift
                + x_correction
                + start
                + match paragraph.align {
                    TextAlign::Left | TextAlign::Justify => 0.0,
                    TextAlign::Distributed if shown_chars == 1 && spread => (room - shown_width) / 2.0,
                    TextAlign::Distributed => 0.0,
                    TextAlign::Center => (room - line.width) / 2.0,
                    TextAlign::Right => room - line.width,
                };
            // PowerPoint on a screen puts a line's baseline on a whole point from the top of the text; its PDF does not (Windows PowerPoint's PDF of 009:
            // two lines of 14 pt are 16.80 pt apart, 1.2 x 14, on the 0.12 pt grid of the 600 dpi page; whole points would make 17).
            let baseline = if window.is_recording_paints_only() { top + line.baseline } else { snap(top + line.baseline, paragraph.px_per_point) };
            let mut baseline = text_top + baseline;
            let mut line_top = text_top + top;
            if paragraph.mirror && let Some(t) = &paragraph.text_top {
                // Lines from left to right: each line where it would be
                // mirrored in the text's area (its characters are not).
                let (area_top, area_height) = t.block.area.get();
                let mirrored = area_top + area_height - (line_top - area_top) - line.height;
                baseline += mirrored - line_top;
                line_top = mirrored;
            }
            if let Some(p) = &mut positions {
                let n = p.lines.len();
                let (left, column) = (f32::from(bounds.origin.x) + column_shift, flow.map_or(0, |(t, _)| t.block.column.get()));
                p.lines.push(LinePosition { start: line.start, end: line.end, x, top: line_top, baseline, height: line.height, column, left, right: left + box_width });
                let mut at = x;
                for (k, c) in layout.chars[line.start..line.end].iter().enumerate() {
                    let adv = c.advance + char_extra(line.start + k);
                    p.chars.push(CharPosition { x: at, advance: adv, line: n });
                    at += adv;
                }
            }
            let reference = reference(paragraph, &layout.chars[line.start..line.end]);
            // Every line shares the shape transform: a line-specific pivot would
            // compress the spacing as the shape rotates.
            let line_turn = turn;
            // Text bent to a warp: the line's box is kept, the glyphs are collected as they come.
            let warp = paragraph.text_top.as_ref().map(|t| t.block.warp.clone()).filter(|w| w.borrow().is_some());
            if let Some(w) = &warp && let Some(w) = w.borrow_mut().as_mut() {
                w.lines.push((x, x + line.width, line_top, line_top + line.height, baseline));
            }
            if let (0, Some(b)) = (n, &paragraph.bullet) {
                match turn {
                    Some(turn) => paint_turned_bullet(b, f32::from(bounds.origin.x) + b.x, baseline + b.drop, turn, window),
                    None => paint_bullet(b, f32::from(bounds.origin.x) + column_shift + b.x, baseline + b.drop, window),
                }
            }
            // Where text painted with a gradient or a pattern spans along
            // the line: each run of stretches with the same fill (pieces of
            // one run), from its first character to its last, and the line's
            // ascent and descent.
            let mut fill_boxes: Vec<(usize, usize, f32, f32)> = Vec::new();
            {
                let (mut k, mut at) = (line.start, x);
                while k < line.end {
                    let fill = &paragraph.pieces[layout.chars[k].piece].fill;
                    let start = (k, at);
                    while k < line.end && paragraph.pieces[layout.chars[k].piece].fill == *fill {
                        at += layout.chars[k].advance + char_extra(k);
                        k += 1;
                    }
                    if fill.is_some() {
                        fill_boxes.push((start.0, k, start.1, at));
                    }
                }
            }
            let fill_box = |i: usize| {
                fill_boxes.iter().find(|b| b.0 <= i && i < b.1).map(|b| (b.2, baseline - reference.0, (b.3 - b.2).max(1.0), (reference.0 + reference.1).max(1.0)))
            };
            // Where the underline being drawn started on this line (its
            // dashes and waves run on from there across the pieces).
            let ul_state: std::cell::Cell<Option<(f32, f32, ibis_doc_model::Underline, f32)>> = std::cell::Cell::new(None);
            let ul_origin = |piece: &Piece, x: f32| -> f32 {
                let key = (piece.underline_kind, piece.strike_size);
                match ul_state.get() {
                    Some((origin, end, kind, size)) if piece.underline && (kind, size) == key && (end - x).abs() < 0.5 => {
                        origin
                    }
                    _ => x,
                }
            };
            // Each stretch of the line that belongs to one piece.
            let mut i = line.start;
            while i < line.end {
                let piece_index = layout.chars[i].piece;
                let mut j = i;
                while j < line.end && layout.chars[j].piece == piece_index {
                    j += 1;
                }
                let piece = &paragraph.pieces[piece_index];
                let text = &piece.text[layout.chars[i].start..layout.chars[j - 1].end];
                if let Some(boxed) = &layout.maths[piece_index] {
                    // A formula, from its first character's place.
                    if layout.chars[i].start == 0 {
                        let y = baseline - piece.raise;
                        math_view::paint(boxed, point(px(x), px(y)), piece.color, &piece.font, window, cx);
                    }
                    x += (i..j).map(|k| layout.chars[k].advance + char_extra(k)).sum::<f32>();
                    i = j;
                    continue;
                }
                let run = TextRun {
                    len: text.len(),
                    font: piece.font.clone(),
                    color: piece.color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(text.to_string().into(), px(piece.size), &[run], None);
                let y = baseline - piece.raise + align_offset(paragraph.font_align, piece, reference);
                let turn = line_turn;
                let advance: f32 = (i..j).map(|k| layout.chars[k].advance + char_extra(k)).sum();
                // A strike stops at the line's last character that shows
                // (not through the spaces a wrapped line ends with; full-width
                // ideographic spaces and trailing underlined spaces at the end
                // of a paragraph are drawn).
                let shown = layout.chars[line.start..line.end]
                    .iter()
                    .rposition(|c| !c.c.is_whitespace() || c.c == '\u{3000}' || (n + 1 == line_count && paragraph.pieces[c.piece].underline))
                    .map_or(line.start, |k| line.start + k + 1);
                let end = j.min(shown).max(i);
                // (No room after the last character that shows.)
                let struck: f32 = (i..end).map(|k| layout.chars[k].advance + if k + 1 < shown { char_extra(k) } else { 0.0 }).sum();
                let origin = ul_origin(piece, x);
                ul_state.set(piece.underline.then_some((origin, x + advance, piece.underline_kind, piece.strike_size)));
                if let Some(w) = &warp {
                    let mut taken: Vec<(Vec<Vec<(f32, f32)>>, Hsla)> = Vec::new();
                    let mut readable = true;
                    'glyphs: for run in &shaped.runs {
                        let Some((data, index)) = window.text_system().font_data(run.font_id) else { readable = false; break };
                        for glyph in &run.glyphs {
                            let before = text[..glyph.index.min(text.len())].chars().count() as f32;
                            let (gx, gy) = (x + f32::from(glyph.position.x) + piece.spacing_px() * before, y + f32::from(glyph.position.y));
                            let Some(polygons) = crate::render::glyph_outline::glyph_polygons(&data, index, glyph.id.0, piece.size) else { readable = false; break 'glyphs };
                            let polygons = slanted_if_synthetic(polygons, window.text_system().synthetic_style(run.font_id).1);
                            taken.push((polygons.into_iter().map(|p| p.into_iter().map(|(a, b)| (gx + a, gy + b)).collect()).collect(), piece.color));
                        }
                    }
                    if readable && let Some(w) = w.borrow_mut().as_mut() {
                        let line_index = w.lines.len().saturating_sub(1);
                        w.glyphs.extend(taken.into_iter().map(|(g, c)| (g, c, line_index)));
                        x += advance;
                        i = j;
                        continue;
                    }
                }
                if let (Some(fill), Some(area)) = (&piece.fill, fill_box(i))
                    && paint_filled(&shaped, text, piece, fill, (x, y), area, turn, paragraph.px_per_point, window)
                {
                    paint_decorations(piece, (x, baseline - piece.raise), struck, origin, paragraph.px_per_point, turn, window);
                    x += advance;
                    i = j;
                    continue;
                }
                if let Some(turn) = turn {
                    paint_turned(&shaped, text, &layout.chars[i..j], piece, (x, y), advance, line.height, turn, paragraph.upright, window);
                    paint_decorations(piece, (x, baseline - piece.raise), struck, origin, paragraph.px_per_point, Some(turn), window);
                    x += advance;
                    i = j;
                    continue;
                }
                if let Some(color) = piece.highlight {
                    // Across the stretch, from the font's ascent to its descent.
                    let area = Bounds::new(point(px(x), px(y - piece.ascent)), size(px(advance), px(piece.ascent + piece.descent)));
                    window.paint_quad(fill(area, color));
                }
                if let Some((color, bw, space)) = piece.border {
                    let bx = x - space;
                    let by = y - piece.ascent - space;
                    let bw_total = advance + 2.0 * space;
                    let bh_total = piece.ascent + piece.descent + 2.0 * space;
                    window.paint_quad(fill(Bounds::new(point(px(bx), px(by)), size(px(bw_total), px(bw))), color));
                    window.paint_quad(fill(Bounds::new(point(px(bx), px(by + bh_total - bw)), size(px(bw_total), px(bw))), color));
                    window.paint_quad(fill(Bounds::new(point(px(bx), px(by)), size(px(bw), px(bh_total))), color));
                    window.paint_quad(fill(Bounds::new(point(px(bx + bw_total - bw), px(by)), size(px(bw), px(bh_total))), color));
                }
                if let Some((color, radius, size_pt)) = piece.glow {
                    // The glow: PowerPoint's glow of text is the glyphs' outline (and the outline's stroke) grown by half the radius and
                    // blurred by a Gaussian of a sixth of it, then given the glow's alpha, as it is for a shape
                    // (`shape_view::glow_spread_blur`; tasks/doc/glow-measurements-20261030.md). That is one blurred picture of the
                    // piece (`blur_shadow`), not letters stacked in rings, which stepped. The glow surrounds the filled AND outlined
                    // glyph: DrawingML's centred outline adds half its width. A radius is not clamped by the font size.
                    let (spread, sigma) = crate::render::shape_view::glow_spread_blur(radius * text_glow_scale(size_pt));
                    let grow = spread + piece.outline.map_or(0., |(_, width)| width * 0.5);
                    let mut outlines: Vec<crate::render::blur_shadow::Polygons> = Vec::new();
                    let mut readable = true;
                    'glow_glyphs: for run in &shaped.runs {
                        let Some((data, index)) = window.text_system().font_data(run.font_id) else { readable = false; break };
                        for glyph in &run.glyphs {
                            let before = text[..glyph.index.min(text.len())].chars().count() as f32;
                            let (gx, gy) = (x + f32::from(glyph.position.x) + (piece.spacing_px() + extra) * before, y + f32::from(glyph.position.y));
                            let Some(polygons) = crate::render::glyph_outline::glyph_polygons(&data, index, glyph.id.0, piece.size) else { readable = false; break 'glow_glyphs };
                            let polygons = slanted_if_synthetic(polygons, window.text_system().synthetic_style(run.font_id).1);
                            let polygons: crate::render::blur_shadow::Polygons = polygons.into_iter().map(|p| p.into_iter().map(|(a, b)| (gx + a, gy + b)).collect()).collect();
                            if grow > 0.0 {
                                for contour in &polygons {
                                    outlines.extend(crate::render::blur_shadow::stroke_quads(contour, true, 2.0 * grow));
                                }
                            }
                            outlines.push(polygons);
                        }
                    }
                    if readable && !outlines.is_empty() {
                        crate::render::blur_shadow::paint(window, &outlines, sigma, color);
                    }
                    // (A font whose outlines cannot be read draws no glow: the web build reads them too, from the font bytes of
                    // gpui-pre-wgpu's `font_data`, with skrifa.)
                }
                if let Some((color, dx, dy, blur)) = piece.shadow {
                    // The shadow's letters sit where the text's do: after the character spacing and
                    // the extra space a distributed line adds. PowerPoint blurs the glyph mask with a
                    // Gaussian of deviation `TEXT_SHADOW_SIGMA_PER_BLUR * blurRad` and paints it at the
                    // shadow's alpha inside the letters. The blur is the glyph drawn at offsets that follow that
                    // Gaussian (rings of equal probability), each at the alpha whose product over all of them
                    // leaves the shadow's alpha where every copy overlaps.
                    let offsets = text_shadow_offsets(blur);
                    let alpha = 1.0 - (1.0 - color.a).powf(1.0 / offsets.len() as f32);
                    let tint = Hsla { a: alpha, ..color };
                    for (ox, oy) in offsets {
                        let at = point(px(x + dx + ox), px(y + dy + oy));
                        for run in &shaped.runs {
                            for glyph in &run.glyphs {
                                let before = text[..glyph.index.min(text.len())].chars().count() as f32;
                                let _ = window.paint_glyph(at + glyph.position + point(px((piece.spacing_px() + extra) * before), px(0.0)), run.font_id, glyph.id, px(piece.size), tint);
                            }
                        }
                    }
                }
                if let Some((color, width)) = piece.outline {
                    // The outline: the text painted around itself in the outline's
                    // colour, out to half the line's width, and then the text over
                    // it, so the fill stays (as `paint_turned` does).
                    let r = width / 2.0;
                    // Rings out to the line's half width, so the outline is solid.
                    let rings = (r / 0.75).ceil().max(1.0) as usize;
                    let spots = (1..=rings).flat_map(|ring| {
                        let radius = r * ring as f32 / rings as f32;
                        (0..12).map(move |k| (k as f32 / 12.0 * std::f32::consts::TAU).sin_cos()).map(move |(s, c)| (c * radius, s * radius))
                    });
                    for (ox, oy) in spots {
                        let at = point(px(x + ox), px(y + oy));
                        for run in &shaped.runs {
                            for glyph in &run.glyphs {
                                let before = text[..glyph.index.min(text.len())].chars().count();
                                let dx: f32 = (i..(i + before).min(j)).map(|k| layout.chars[k].advance + char_extra(k) - layout.chars[k].raw).sum();
                                let _ = window.paint_glyph(at + glyph.position + point(px(dx), px(0.0)), run.font_id, glyph.id, px(piece.size), color);
                            }
                        }
                    }
                }
                if piece.spacing_px() == 0.0 && extra == 0.0 && space_extra == 0.0 && piece.kana_widths.is_none() && piece.advance_table.is_none() && !piece.proportional_alnum && !text.contains('\t') {
                    let height = shaped.ascent + shaped.descent;
                    let _ = shaped.paint(point(px(x), px(y) - shaped.ascent), height, gpui_kit::TextAlign::Left, None, window, cx);
                } else {
                    paint_spaced(&shaped, text, &layout.chars[i..j], i, &char_extra, piece, point(px(x), px(y)), window);
                }
                if let Some(inner_sh) = piece.inner_shadow {
                    paint_inner_shadow(&shaped, text, &layout.chars[i..j], piece, extra, (x, y), inner_sh, window);
                }
                paint_decorations(piece, (x, baseline - piece.raise), struck, origin, paragraph.px_per_point, None, window);
                x += advance;
                i = j;
            }
            top += line.height;
        }
        if let (Some(t), true) = (&paragraph.text_top, consumed > 0.0) {
            t.block.next.set(paragraph_top + text_height(paragraph, &layout.lines) + t.after - consumed);
        }
        if let (Some(slot), Some(p)) = (&paragraph.positions, positions) {
            *slot.borrow_mut() = p;
        }
    }
}

/// Paint glyph `glyph` of `font_id` with its origin at (`x`, `y`) of the
/// text's coordinates, turned by `turn`: moved to where the turn takes its
/// origin and turned about it there (so its unturned box is where it ends
/// up, which is what clipping looks at).
fn paint_turned_glyph(window: &mut Window, (x, y): (f32, f32), font_id: FontId, glyph: GlyphId, size: f32, color: Hsla, turn: Turn) {
    let (mut tx, mut ty) = turn.apply(x, y);
    let is_net_rotated = (turn.cos - 1.0).abs() > 1e-3 || turn.sin.abs() > 1e-3;
    let is_orthogonal = turn.sin.abs() < 1e-3 || turn.cos.abs() < 1e-3;
    if is_net_rotated && is_orthogonal {
        let sf = window.scale_factor();
        tx = (tx * sf).round() / sf;
        ty = (ty * sf).round() / sf;
    }
    paint_glyph_at(window, (tx, ty), turn.radians(), font_id, glyph, size, color);
}

/// Where a glyph stands upright in a column of vertical writing is painted: the middle of its cell, a full width along the
/// line, is at (`cx`, `gy` - 0.38 em) of a line laid out across (`size` pixels), the cell turned back a quarter turn about its
/// middle (which stays where it is). The glyph's origin is `across` pixels from the cell's middle, to the right as it
/// stands (a full width character: half a width to the left). Returns the glyph's origin and the angle it is painted at.
fn upright_glyph((cx, gy): (f32, f32), size: f32, across: f32, turn: Turn) -> ((f32, f32), f32) {
    // The em square: from 0.88 em above the baseline to 0.12 em below, so its middle is 0.38 em above the baseline; the origin
    // is that much below the middle once the glyph stands.
    let cy = gy - 0.38 * size;
    let (ox, oy) = (cx + 0.38 * size, cy - across);
    (turn.apply(ox, oy), turn.radians() - std::f32::consts::FRAC_PI_2)
}

/// Paint `glyph` with its origin at (`tx`, `ty`) (window pixels), turned
/// `angle` radians clockwise about its origin.
fn paint_glyph_at(window: &mut Window, at: (f32, f32), angle: f32, font_id: FontId, glyph: GlyphId, size: f32, color: Hsla) {
    paint_glyph_scaled(window, at, angle, 1.0, font_id, glyph, size, color);
}

/// `paint_glyph_at` with the glyph squeezed to `xscale` of its width (about its origin).
#[allow(clippy::too_many_arguments)]
fn paint_glyph_scaled(window: &mut Window, (tx, ty): (f32, f32), angle: f32, xscale: f32, font_id: FontId, glyph: GlyphId, size: f32, color: Hsla) {
    let scale = window.scale_factor();
    let pivot = point(ScaledPixels(tx * scale), ScaledPixels(ty * scale));
    // The renderer drops a glyph whose unturned box (from its origin, right and up by about a size) is
    // outside the window, whatever the turn does to it: a line of sideways text at the slide's edge lost
    // its letters one by one. So the glyph is drawn at an anchor inside the window and the turn carries
    // it from there to where it belongs.
    let viewport = window.viewport_size();
    let (vw, vh) = (f32::from(viewport.width), f32::from(viewport.height));
    let anchor = if tx >= 0.0 && tx <= vw - size && ty >= size && ty <= vh {
        (tx, ty)
    } else {
        (tx.clamp(0.0, (vw - size).max(0.0)), ty.clamp(size.min(vh), vh))
    };
    let from = point(ScaledPixels(anchor.0 * scale), ScaledPixels(anchor.1 * scale));
    let transformation = TransformationMatrix::unit()
        .translate(pivot)
        .rotate(Radians(angle))
        .scale(gpui_kit::size(xscale, 1.0))
        .translate(point(ScaledPixels(-from.x.0), ScaledPixels(-from.y.0)));
    let _ = window.paint_glyph_transformed(point(px(anchor.0), px(anchor.1)), font_id, glyph, px(size), color, transformation);
}

/// A rectangle of the text's coordinates painted turned.
fn paint_turned_rect(window: &mut Window, (x, y, w, h): (f32, f32, f32, f32), color: Hsla, turn: Turn) {
    let corners: Vec<Point<Pixels>> = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
        .into_iter()
        .map(|(a, b)| {
            let (a, b) = turn.apply(a, b);
            point(px(a), px(b))
        })
        .collect();
    if let Some(path) = crate::render::shape_view::polygon(&corners, None, true) {
        window.paint_path(path, color);
    }
}

/// A stretch of a turned line: `shaped` (of `text`, in `piece`) from its
/// baseline's start (`x`, `y`), `advance` wide, with its highlight and
/// outline (`paint_decorations` draws its underline and strike).
fn paint_turned(shaped: &ShapedLine, text: &str, chars: &[Char], piece: &Piece, (x, y): (f32, f32), advance: f32, line_height: f32, turn: Turn, upright: bool, window: &mut Window) {
    if let Some(color) = piece.highlight {
        paint_turned_rect(window, (x, y - piece.ascent, advance, piece.ascent + piece.descent), color, turn);
    }
    let glyph_at = |index: usize, position: Point<Pixels>| {
        let before = text[..index.min(text.len())].chars().count() as f32;
        (x + f32::from(position.x) + piece.spacing_px() * before, y + f32::from(position.y))
    };
    if let Some((color, width)) = piece.outline {
        let r = width / 2.0;
        let rings = (r / 0.75).ceil().max(1.0) as usize;
        for ring in 1..=rings {
            let radius = r * ring as f32 / rings as f32;
            for k in 0..12 {
                let (s, c) = (k as f32 / 12.0 * std::f32::consts::TAU).sin_cos();
                for run in &shaped.runs {
                    for glyph in &run.glyphs {
                        let (gx, gy) = glyph_at(glyph.index, glyph.position);
                        paint_turned_glyph(window, (gx + c * radius, gy + s * radius), run.font_id, glyph.id, piece.size, color, turn);
                    }
                }
            }
        }
    }
    // What the digits set upright in cells (`Tcy`) of the stretch take or give along the line, before each character.
    let mut shift = vec![0.0f32; chars.len() + 1];
    for (k, c) in chars.iter().enumerate() {
        shift[k + 1] = shift[k] + if c.tcy == Tcy::No { 0.0 } else { c.advance - c.raw };
    }
    for run in &shaped.runs {
        for glyph in &run.glyphs {
            let at_char = text[..glyph.index.min(text.len())].chars().count();
            let (mut gx, gy) = glyph_at(glyph.index, glyph.position);
            gx += shift.get(at_char).copied().unwrap_or(0.0);
            let this = text[glyph.index.min(text.len())..].chars().next();
            let tcy = chars.get(at_char).map_or(Tcy::No, |c| c.tcy);
            if glyph.is_emoji {
                // Emoji are drawn unturned, where they belong.
                let (tx, ty) = turn.apply(gx, gy);
                let _ = window.paint_emoji(point(px(tx), px(ty)), run.font_id, glyph.id, px(piece.size));
            } else if upright && tcy == Tcy::Whole {
                // The run as one cell: its glyphs side by side across the line, centred, squeezed when 行の幅に合わせる is on and it is
                // wider than the line.
                let first = chars.iter().position(|c| c.tcy == Tcy::Whole).unwrap_or(0);
                let total: f32 = chars.iter().filter(|c| c.tcy == Tcy::Whole).map(|c| c.raw).sum();
                let before: f32 = chars[first..at_char].iter().map(|c| c.raw).sum();
                let _ = line_height;
                let squeeze = if matches!(piece.tate, Some(ibis_doc_model::TateChuYoko::Fit | ibis_doc_model::TateChuYoko::Grow)) && total > 0.0 { piece.tate_scale } else { 1.0 };
                let cell_start = x + chars[..first].iter().map(|c| c.raw).sum::<f32>() + shift[first];
                let (at, angle) = upright_glyph((cell_start + piece.size / 2.0, gy), piece.size, (-total / 2.0 + before) * squeeze, turn);
                paint_glyph_scaled(window, at, angle, squeeze, run.font_id, glyph.id, piece.size, piece.color);
            } else if upright && tcy != Tcy::No {
                // A digit upright in a cell of one em along the line: the digits of a pair side by side, centred across the cell.
                let (cell, pair, offset) = match tcy {
                    Tcy::Second => (gx - chars[at_char - 1].advance, chars[at_char - 1].raw + chars[at_char].raw, chars[at_char - 1].raw),
                    Tcy::First => (gx, chars[at_char].raw + chars[at_char + 1].raw, 0.0),
                    _ => (gx, chars[at_char].raw, 0.0),
                };
                let (at, angle) = upright_glyph((cell + piece.size / 2.0, gy), piece.size, -pair / 2.0 + offset, turn);
                paint_glyph_at(window, at, angle, run.font_id, glyph.id, piece.size, piece.color);
            } else if upright && this.is_some_and(is_cjk) {
                let c = this.unwrap_or(' ');
                // The font's vertical form of a mark (、。「」ー…): drawn in the same place as the upright character.
                let id = if crate::render::vertical_text::vertical_glyph(c) {
                    crate::render::vertical_text::vertical_glyph_id(window.text_system(), run.font_id, c).unwrap_or(glyph.id)
                } else {
                    glyph.id
                };
                let (at, angle) = upright_glyph((gx + piece.size / 2.0, gy), piece.size, -piece.size / 2.0, turn);
                paint_glyph_at(window, at, angle, run.font_id, id, piece.size, piece.color);
            } else {
                paint_turned_glyph(window, (gx, gy), run.font_id, glyph.id, piece.size, piece.color, turn);
            }
        }
    }

}

/// A glyph's polygons (from its baseline, y down) slanted as the text system draws an italic asked of a font without one
/// (x moves right by a fifth of the height above the baseline).
fn slanted_if_synthetic(polygons: Vec<Vec<(f32, f32)>>, italic: bool) -> Vec<Vec<(f32, f32)>> {
    if !italic {
        return polygons;
    }
    polygons.into_iter().map(|p| p.into_iter().map(|(a, b)| (a - 0.2 * b, b)).collect()).collect()
}

/// A stretch of text painted with a gradient or a pattern (`fill`): its
/// glyphs as polygons, the fill laid over `area` (x, y, width, height: the
/// run's extent along the line), turned by `turn`. False (nothing painted)
/// when a glyph's outline cannot be read: the text is then drawn in its
/// colour.
#[allow(clippy::too_many_arguments)]
fn paint_filled(
    shaped: &ShapedLine,
    text: &str,
    piece: &Piece,
    fill: &ibis_doc_model::Fill,
    (x, y): (f32, f32),
    (bx, by, bw, bh): (f32, f32, f32, f32),
    turn: Option<Turn>,
    px_per_point: f32,
    window: &mut Window,
) -> bool {
    // The glyphs' contours, in the area's own coordinates.
    let mut contours: Vec<Vec<(f32, f32)>> = Vec::new();
    for run in &shaped.runs {
        let Some((data, index)) = window.text_system().font_data(run.font_id) else { return false };
        for glyph in &run.glyphs {
            let before = text[..glyph.index.min(text.len())].chars().count() as f32;
            let (gx, gy) = (x + f32::from(glyph.position.x) + piece.spacing_px() * before, y + f32::from(glyph.position.y));
            let Some(polygons) = crate::render::glyph_outline::glyph_polygons(&data, index, glyph.id.0, piece.size) else { return false };
            let polygons = slanted_if_synthetic(polygons, window.text_system().synthetic_style(run.font_id).1);
            contours.extend(polygons.into_iter().map(|p| p.into_iter().map(|(px_, py)| (gx + px_ - bx, gy + py - by)).collect()));
        }
    }
    let to_window = move |pts: &[(f32, f32)]| -> Vec<Point<Pixels>> {
        pts.iter()
            .map(|&(a, b)| {
                let (a, b) = (a + bx, b + by);
                let (a, b) = turn.map_or((a, b), |t| t.apply(a, b));
                point(px(a), px(b))
            })
            .collect()
    };
    match fill {
        ibis_doc_model::Fill::Pattern { background, .. } => {
            let windowed: Vec<Vec<Point<Pixels>>> = contours.iter().map(|c| to_window(c)).collect();
            let closed: Vec<(Vec<Point<Pixels>>, bool)> = windowed.iter().map(|c| (c.clone(), true)).collect();
            if let Some(path) = crate::render::shape_view::compound_path(&closed, None) {
                window.paint_path(path, crate::render::shape_view::hsla(*background));
            }
            // The pattern's cells from the area's corner, 1 pt each.
            let scale = px_per_point.max(0.01) / ibis_doc_model::EMU_PER_POINT as f32;
            let (ox, oy) = turn.map_or((bx, by), |t| t.apply(bx, by));
            crate::render::shape_view::paint_pattern(window, fill, &windowed, point(px(ox), px(oy)), scale);
        }
        _ => crate::render::gradient::paint_gradient(window, fill, &contours, (bw, bh), 1.0, &to_window),
    }
    true
}

/// A bullet (a number) with its baseline's start at (`x`, `y`).
fn paint_bullet(b: &BulletMark, x: f32, y: f32, window: &mut Window) {
    let run = TextRun { len: b.text.len(), font: b.font.clone(), color: b.color, background_color: None, underline: None, strikethrough: None };
    let shaped = window.text_system().shape_line(b.text.clone().into(), px(b.size), &[run], None);
    for run in &shaped.runs {
        for glyph in &run.glyphs {
            let _ = window.paint_glyph(point(px(x + f32::from(glyph.position.x)), px(y + f32::from(glyph.position.y))), run.font_id, glyph.id, px(b.size), b.color);
        }
    }
}

/// A turned text's bullet, its baseline's start at (`x`, `y`).
fn paint_turned_bullet(b: &BulletMark, x: f32, y: f32, turn: Turn, window: &mut Window) {
    let run = TextRun { len: b.text.len(), font: b.font.clone(), color: b.color, background_color: None, underline: None, strikethrough: None };
    let shaped = window.text_system().shape_line(b.text.clone().into(), px(b.size), &[run], None);
    for run in &shaped.runs {
        for glyph in &run.glyphs {
            paint_turned_glyph(window, (x + f32::from(glyph.position.x), y + f32::from(glyph.position.y)), run.font_id, glyph.id, b.size, b.color, turn);
        }
    }
}

/// Paint an inner shadow (`a:innerShdw`) inside the glyphs of `shaped`: pixels inside the glyph
/// whose shifted neighbours `(px - dx, py - dy)` within the blur radius lie outside the glyph
/// are tinted with `sh_color`.
fn paint_inner_shadow(
    shaped: &ShapedLine,
    text: &str,
    chars: &[Char],
    piece: &Piece,
    extra: f32,
    (x, y): (f32, f32),
    (sh_color, dx, dy, blur): (Hsla, f32, f32, f32),
    window: &mut Window,
) {
    let r1 = (blur * 0.25).max(0.5);
    let r2 = (blur * 0.5).max(1.0);
    let mut offsets = [(0.0f32, 0.0f32); 13];
    for k in 0..6 {
        let (s, c) = (k as f32 / 6.0 * std::f32::consts::TAU).sin_cos();
        offsets[1 + k] = (c * r1, s * r1);
        offsets[7 + k] = (c * r2, s * r2);
    }
    let spaced = !(piece.spacing_px() == 0.0 && extra == 0.0 && piece.kana_widths.is_none() && piece.advance_table.is_none() && !piece.proportional_alnum && !text.contains('\t'));
    for run in &shaped.runs {
        let Some((data, index)) = window.text_system().font_data(run.font_id) else { continue };
        let synth_italic = window.text_system().synthetic_style(run.font_id).1;
        for glyph in &run.glyphs {
            if glyph.is_emoji {
                continue;
            }
            let Some(polygons) = crate::render::glyph_outline::glyph_polygons(&data, index, glyph.id.0, piece.size) else { continue };
            let polygons = slanted_if_synthetic(polygons, synth_italic);
            if polygons.is_empty() {
                continue;
            }
            let shift_x = if spaced {
                let before = text[..glyph.index.min(text.len())].chars().count();
                chars[..before.min(chars.len())].iter().map(|c| c.advance + extra - c.raw).sum::<f32>()
            } else {
                0.0
            };
            let gx = x + f32::from(glyph.position.x) + shift_x;
            let gy = y + f32::from(glyph.position.y);
            let mut min_x = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for poly in &polygons {
                for &(px_, py_) in poly {
                    min_x = min_x.min(gx + px_);
                    max_x = max_x.max(gx + px_);
                    min_y = min_y.min(gy + py_);
                    max_y = max_y.max(gy + py_);
                }
            }
            if min_x >= max_x || min_y >= max_y {
                continue;
            }
            let x0 = min_x.floor() as i32;
            let x1 = max_x.ceil() as i32;
            let y0 = min_y.floor() as i32;
            let y1 = max_y.ceil() as i32;
            let gw = (x1 - x0).clamp(1, 512) as usize;
            let gh = (y1 - y0).clamp(1, 512) as usize;
            let mut mask = vec![false; gw * gh];
            let mut crossings: Vec<f32> = Vec::new();
            for row in 0..gh {
                let py_f = y0 as f32 + row as f32 + 0.5 - gy;
                crossings.clear();
                for poly in &polygons {
                    let n = poly.len();
                    if n < 3 {
                        continue;
                    }
                    for k in 0..n {
                        let (ax, ay) = poly[k];
                        let (bx, by) = poly[(k + 1) % n];
                        if (ay <= py_f && by > py_f) || (by <= py_f && ay > py_f) {
                            let t = (py_f - ay) / (by - ay);
                            crossings.push(gx + ax + t * (bx - ax));
                        }
                    }
                }
                crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                for pair in crossings.chunks_exact(2) {
                    let c_start = ((pair[0] - x0 as f32 - 0.5).ceil() as i32).max(0) as usize;
                    let c_end = ((pair[1] - x0 as f32 - 0.5).floor() as i32 + 1).clamp(0, gw as i32) as usize;
                    for col in c_start..c_end.min(gw) {
                        mask[row * gw + col] = true;
                    }
                }
            }
            let is_inside = |wx: f32, wy: f32| -> bool {
                let col = (wx - x0 as f32).floor() as i32;
                let row = (wy - y0 as f32).floor() as i32;
                if col < 0 || row < 0 || col >= gw as i32 || row >= gh as i32 {
                    false
                } else {
                    mask[row as usize * gw + col as usize]
                }
            };
            for row in 0..gh {
                let wy = y0 as f32 + row as f32 + 0.5;
                for col in 0..gw {
                    if !mask[row * gw + col] {
                        continue;
                    }
                    let wx = x0 as f32 + col as f32 + 0.5;
                    let mut outside_count = 0usize;
                    for &(ox, oy) in &offsets {
                        if !is_inside(wx - dx + ox, wy - dy + oy) {
                            outside_count += 1;
                        }
                    }
                    if outside_count > 0 {
                        let frac = outside_count as f32 / offsets.len() as f32;
                        let tint = Hsla { a: sh_color.a * frac, ..sh_color };
                        let area = Bounds::new(point(px(x0 as f32 + col as f32), px(y0 as f32 + row as f32)), size(px(1.0), px(1.0)));
                        window.paint_quad(fill(area, tint));
                    }
                }
            }
        }
    }
}

/// Paint `shaped` (the line of `text`) glyph by glyph from `baseline`,
/// placing each character at its laid-out advance (`c.advance + extra`).
fn paint_spaced(shaped: &ShapedLine, text: &str, chars: &[Char], start_index: usize, char_extra: &dyn Fn(usize) -> f32, piece: &Piece, baseline: Point<Pixels>, window: &mut Window) {
    for run in &shaped.runs {
        for glyph in &run.glyphs {
            let before = text[..glyph.index.min(text.len())].chars().count();
            let dx: f32 = chars[..before.min(chars.len())].iter().enumerate().map(|(k, c)| c.advance + char_extra(start_index + k) - c.raw).sum();
            let at = point(baseline.x + glyph.position.x + px(dx), baseline.y + glyph.position.y);
            let _ = if glyph.is_emoji {
                window.paint_emoji(at, run.font_id, glyph.id, px(piece.size))
            } else {
                window.paint_glyph(at, run.font_id, glyph.id, px(piece.size), piece.color)
            };
        }
    }
}

/// The ascent and descent of the largest piece among `chars`, which the
/// others line up with.
fn reference(paragraph: &RichParagraph, chars: &[Char]) -> (f32, f32) {
    chars
        .iter()
        .map(|c| c.lift.unwrap_or((paragraph.pieces[c.piece].ascent, paragraph.pieces[c.piece].descent)))
        .fold((0.0, 0.0), |m, v| if v.0 + v.1 > m.0 + m.1 { v } else { m })
}

/// How far below the shared baseline `piece` goes to line up with the
/// largest piece of its line.
fn align_offset(align: FontAlign, piece: &Piece, reference: (f32, f32)) -> f32 {
    match align {
        FontAlign::Top => piece.ascent - reference.0,
        FontAlign::Center => ((piece.ascent - piece.descent) - (reference.0 - reference.1)) / 2.0,
        FontAlign::Bottom => reference.1 - piece.descent,
        // Auto provisionally uses offset 0 (the previous baseline placement);
        // Office's automatic alignment semantics are not yet verified.
        FontAlign::Auto | FontAlign::Baseline => 0.0,
    }
}

fn lay_out(paragraph: &RichParagraph, width: Option<f32>, window: &mut Window) -> Layout {
    let mut chars = Vec::new();
    let mut maths: Vec<Option<MathBox>> = vec![None; paragraph.pieces.len()];
    for (index, piece) in paragraph.pieces.iter().enumerate() {
        if piece.text.is_empty() {
            continue;
        }
        if let Some(math) = &piece.math {
            // The formula's width is shared out between its characters.
            let boxed = math_view::typeset(math, piece.size, &mut math_view::window_measure(piece.font.clone(), window));
            let count = piece.text.chars().count().max(1) as f32;
            for (start, c) in piece.text.char_indices() {
                chars.push(Char { piece: index, start, end: start + c.len_utf8(), c, raw: boxed.width / count, advance: boxed.width / count, lift: Some((boxed.ascent, boxed.descent)), tcy: Tcy::No, east_asian: piece.east_asian_lang });
            }
            maths[index] = Some(boxed);
            continue;
        }
        let run = TextRun {
            len: piece.text.len(),
            font: piece.font.clone(),
            color: piece.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(piece.text.clone().into(), px(piece.size), &[run], None);
        let emboldened = shaped.runs.first().is_some_and(|r| window.text_system().synthetic_style(r.font_id).0);
        piece.synthetic_bold.set(if emboldened { OFFICE_PDF_DEVICE_PIXEL_PT * paragraph.px_per_point } else { 0.0 });
        let first_of_piece = chars.len();
        let bounds: Vec<(usize, char)> = piece.text.char_indices().collect();
        for (k, &(start, c)) in bounds.iter().enumerate() {
            let end = bounds.get(k + 1).map_or(piece.text.len(), |b| b.0);
            // For the first character (`k == 0`), the piece starts at 0.0 (`shaped.x_for_index(0)`
            // returns `glyph[0].position.x`, which is negative when OpenType `palt` shifts an
            // opening bracket left by 0.5 em; using it as `x0` would inflate `x1 - x0` and make
            // the piece's character advances sum to more than `shaped.width`).
            let x0 = if k == 0 { 0.0 } else { f32::from(shaped.x_for_index(start)) };
            let x1 = if end == piece.text.len() { f32::from(shaped.width) } else { f32::from(shaped.x_for_index(end)) };
            let raw = (x1 - x0).max(0.0);
            let next_c = bounds.get(k + 1).map(|b| b.1);
            let adv = if let Some(w) = piece.kana_widths
                && (0x3041..=0x30fa).contains(&(c as usize))
                && !next_c.is_some_and(|n| "（「『【〔〈《［｛〘〖".contains(n)) // i18n-ok（禁則の文字の並び）
            {
                (w[c as usize - 0x3040] as f32 / 256.0) * piece.size + piece.spacing_px()
            } else if let Some(em) = piece.advance_table.and_then(|t| t.advance(c)) {
                em * piece.size + piece.spacing_px()
            } else if piece.proportional_alnum
                && let Some(w) = crate::render::font_metrics::fallback_fullwidth_alnum_width(c)
            {
                (w as f32 / 256.0) * piece.size + piece.spacing_px()
            } else if c == ' ' && piece.spacing_px() == 0.0 && piece.text.chars().all(|ch| ch == ' ') {
                // Office (DirectWrite/GDI) snaps space advances to 96 DPI half-pixels:
                // (Points from pixels by the paragraph's own scale: below one pixel a point the space came out too narrow.)
                let size_pt = piece.size / paragraph.px_per_point.max(1e-3);
                if size_pt > 0.0 {
                    let em_adv = raw / piece.size;
                    let px96 = (size_pt * (96.0 / 72.0)) * em_adv;
                    let snapped_px96 = (px96 * 2.0).round() / 2.0;
                    let snapped_pt = snapped_px96 * (72.0 / 96.0);
                    snapped_pt * paragraph.px_per_point
                } else {
                    raw
                }
            } else {
                raw + piece.spacing_px()
            };
            chars.push(Char { piece: index, start, end, c, raw, advance: adv, lift: None, tcy: Tcy::No, east_asian: piece.east_asian_lang });
        }
        if paragraph.upright {
            match piece.tate {
                None => set_tcy(&mut chars[first_of_piece..], piece.size),
                Some(ibis_doc_model::TateChuYoko::Off) => {}
                Some(_) => {
                    // The cell of one em is shared out between the characters by their widths, so that the caret goes through it
                    // one character at a time.
                    let total: f32 = chars[first_of_piece..].iter().map(|c| c.raw).sum();
                    let n = chars[first_of_piece..].len().max(1) as f32;
                    for c in chars[first_of_piece..].iter_mut() {
                        c.tcy = Tcy::Whole;
                        c.advance = if total > 0.0 { c.raw * piece.size / total } else { piece.size / n };
                    }
                }
            }
        }
    }
    // Text that fills its box exactly (full-width characters, a box sized
    // to them) stays on one line despite rounding: a fiftieth of a point
    // over the exact width, or half a pixel over the rounded one.
    let (room, slack) = match (width, paragraph.room) {
        (Some(_), Some(room)) => (Some(room), 0.02 * paragraph.px_per_point.max(0.05)),
        _ => (width, 0.5),
    };
    let rules = Breaks { hang: paragraph.hanging_punct, latin: LatinBreaks { anywhere: paragraph.latin_line_break, ..LatinBreaks::default() } };
    let breaks = break_lines(&mut chars, room, paragraph.first_line, paragraph.margin_left, &paragraph.tabs, paragraph.default_tab, slack, rules);
    let lines = breaks
        .into_iter()
        .map(|(start, end)| {
            let visible = chars[start..end].iter().rposition(|c| !c.c.is_whitespace()).map_or(start, |p| start + p + 1);
            let width = chars[start..visible].iter().map(|c| c.advance).sum();
            // An empty paragraph's line is its (empty) first piece's.
            let (a, d, largest) = match paragraph.pieces.first() {
                Some(p) if start == end => (p.ascent, p.descent, p.size),
                _ => {
                    let (a, d) = reference(paragraph, &chars[start..end]);
                    (a, d, chars[start..end].iter().map(|c| paragraph.pieces[c.piece].size).fold(0.0, f32::max))
                }
            };
            let single = POWERPOINT_SINGLE * largest;
            let height = match paragraph.line {
                LineRule::Multiple(m) => m * largest,
                LineRule::Fixed(h) => h,
            };
            let natural = natural_below(single, a, d);
            let baseline = height - powerpoint_below_baseline(height, single, natural);
            // Text horizontal in vertical that is not squeezed to the line (行の幅に合わせる off) makes its line as wide as it is.
            let grow = chars[start..end]
                .iter()
                .filter(|c| c.tcy == Tcy::Whole && paragraph.pieces[c.piece].tate == Some(ibis_doc_model::TateChuYoko::Grow))
                .map(|c| whole_width(&chars, c.piece) * paragraph.pieces[c.piece].tate_scale)
                .fold(0.0f32, f32::max);
            // (The line grows on both sides of the text, which stays in the middle of it.)
            // On a line grid the line takes whole pitches (Windows Word 16.0.20527, 縦中横 of 1 to 8 digits of MS Gothic 10.5, 12 and 20 pt in a
            // vertical text box on a 20 pt grid: the column was 20 pt for a width up to 20, 40 up to 40, 60 up to 60: the next pitch up).
            let grow = match paragraph.grid_pitch.filter(|p| *p > 0.0) {
                Some(pitch) if grow > 0.0 => (grow / pitch - 1e-3).ceil() * pitch,
                _ => grow,
            };
            let extra = (grow - height).max(0.0);
            let height = height + extra;
            let baseline = baseline + extra / 2.0;
            // A formula taller than the text makes its line taller.
            let (math_ascent, math_descent) = chars[start..end].iter().filter_map(|c| c.lift).fold((0.0f32, 0.0f32), |m, v| (m.0.max(v.0), m.1.max(v.1)));
            // A line of nothing but a formula is as high as the formula (PowerPoint's own); with other text the baseline is
            // the text's when that is lower.
            let only_math = chars[start..end].iter().all(|c| c.lift.is_some());
            let (height, baseline) = if math_ascent + math_descent > 0.0 {
                let base = if only_math { math_ascent } else { baseline.max(math_ascent) };
                (height.max(base + math_descent), base)
            } else {
                (height, baseline)
            };
            Line { start, end, width, height, baseline, natural }
        })
        .collect();
    Layout { width, chars, lines, maths }
}

/// Sets the ASCII digits among `chars` (one piece's) as PowerPoint sets them in East Asian vertical writing: each run of digits
/// is cut two by two, a pair of digits stands upright side by side and a last digit stands upright alone, each in a cell
/// of one em (`size`) along the column (found in PowerPoint: "26年" has 26 side by side, "2026年" has 20 and 26 one under the
/// other, "026" has 02 and 6, "5年" has the 5 upright; nothing in the file says so, the text is plain).
fn whole_width(chars: &[Char], piece: usize) -> f32 {
    chars.iter().filter(|c| c.piece == piece && c.tcy == Tcy::Whole).map(|c| c.raw).sum()
}

fn set_tcy(chars: &mut [Char], size: f32) {
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].c.is_ascii_digit() {
            i += 1;
            continue;
        }
        let run = chars[i..].iter().take_while(|c| c.c.is_ascii_digit()).count();
        let mut k = 0;
        while k < run {
            if k + 1 < run {
                // (The cell is shared out by the digits' widths, for the caret.)
                let (a, b) = (chars[i + k].raw, chars[i + k + 1].raw);
                let first = if a + b > 0.0 { size * a / (a + b) } else { size / 2.0 };
                chars[i + k].tcy = Tcy::First;
                chars[i + k].advance = first;
                chars[i + k + 1].tcy = Tcy::Second;
                chars[i + k + 1].advance = size - first;
                k += 2;
            } else {
                chars[i + k].tcy = Tcy::Alone;
                chars[i + k].advance = size;
                k += 1;
            }
        }
        i += run;
    }
}

/// The height of the paragraph's `lines`: the last paragraph of a text
/// ends at its last line's font's descent below the baseline.
fn text_height(paragraph: &RichParagraph, lines: &[Line]) -> f32 {
    let mut height: f32 = lines.iter().map(|l| l.height).sum();
    if let (true, Some(last)) = (paragraph.last, lines.last()) {
        height += last.baseline + last.natural - last.height;
    }
    height
}

/// `y` (pixels) on the nearest whole point, `px_per_point` pixels a point
/// (as it is when that is 0).
fn snap(y: f32, px_per_point: f32) -> f32 {
    if px_per_point > 0.0 { (y / px_per_point).round() * px_per_point } else { y }
}

/// Next tab stop position (in pixels from the text body's left inset) strictly after `x`.
fn next_tab_stop(x: f32, tabs: &[f32], default_tab: f32) -> Option<f32> {
    if let Some(&stop) = tabs.iter().find(|&&pos| pos > x + 1e-3) {
        return Some(stop);
    }
    if default_tab > 0.0 {
        let k = ((x + 1e-3) / default_tab).floor() + 1.0;
        return Some((k * default_tab).max(x));
    }
    None
}

/// Ranges of `chars` that make up each line within `width` (and `slack`
/// over it), the first line starting `first_line` in.
fn break_lines(
    chars: &mut [Char],
    width: Option<f32>,
    first_line: f32,
    margin_left: f32,
    tabs: &[f32],
    default_tab: f32,
    slack: f32,
    rules: Breaks,
) -> Vec<(usize, usize)> {
    if chars.is_empty() {
        return vec![(0, 0)];
    }
    if width.is_none() {
        let mut used = 0.0;
        for c in chars.iter_mut() {
            if c.c == '\t' {
                let abs_x = margin_left + first_line + used;
                if let Some(stop) = next_tab_stop(abs_x, tabs, default_tab) {
                    c.advance = (stop - abs_x).max(0.0);
                }
            }
            used += c.advance;
        }
        return vec![(0, chars.len())];
    }
    let Some(width) = width else { return vec![(0, chars.len())] };
    let mut lines = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    // The last place a line may end before the current character.
    let mut last_break = None;
    let mut i = 0;
    while i < chars.len() {
        if chars[i].c == '\t' {
            let line_offset = if lines.is_empty() { first_line } else { 0.0 };
            let abs_x = margin_left + line_offset + used;
            if let Some(stop) = next_tab_stop(abs_x, tabs, default_tab) {
                chars[i].advance = (stop - abs_x).max(0.0);
            }
        }
        let c = chars[i];
        if i > start && may_break_between(chars, i, rules.latin) {
            last_break = Some(i);
        }
        // Spaces, and with `hangingPunct` the commas and full stops, hang past the edge: one of them, after characters that fit (PowerPoint's
        // PDFs of `kinsoku-*-hang1-probe.pptx`: a second 。 of 。。 takes the character before them to the next line, as without `hangingPunct`).
        let room = if lines.is_empty() { width - first_line } else { width };
        // Only where the line may end after it (or after the spaces that follow it): the full stop inside "figshare.com" does not hang, the word
        // breaks before it (PowerPoint's PDF of corpus 152); "words, and" hangs its comma (`kinsoku-*-hang1-probe.pptx`).
        let ends_here = chars.get(i + 1).is_none_or(|n| n.c.is_whitespace() || may_break_between(chars, i + 1, rules.latin));
        // The Latin comma and full stop hang only in a run of an East Asian language (PowerPoint's PDFs: "words," of `kinsoku-*-hang1-probe.pptx`,
        // lang ja-JP, hangs; "range," of corpus 139, English, goes down with its word).
        let hang = rules.hang && hangs(c.c) && (c.east_asian || !matches!(c.c, ',' | '.')) && ends_here && used <= room + slack;
        if !c.c.is_whitespace() && !hang && used + c.advance > room + slack && i > start {
            let end = last_break.filter(|&b| b > start).unwrap_or(i);
            lines.push((start, end));
            start = end;
            used = 0.0;
            for k in start..i {
                if chars[k].c == '\t' {
                    let abs_x = margin_left + used;
                    if let Some(stop) = next_tab_stop(abs_x, tabs, default_tab) {
                        chars[k].advance = (stop - abs_x).max(0.0);
                    }
                }
                used += chars[k].advance;
            }
            last_break = None;
            continue;
        }
        used += c.advance;
        i += 1;
    }
    lines.push((start, chars.len()));
    lines
}

/// Whether a line may break between `chars[at - 1]` and `chars[at]` in PowerPoint's text: the rule of `may_break`, and a run of
/// ideographic spaces is not parted from the character after it when a line may not start with that character ("(　　　)": the
/// brackets and the spaces go to the next line together, and a line does not start with the spaces and a lone ")": PowerPoint's PDF of
/// corpus pptx-C011-b1).
fn may_break_between(chars: &[Char], at: usize, latin: LatinBreaks) -> bool {
    if !may_break(chars[at - 1].c, chars[at].c, Some(Kinsoku::POWERPOINT), latin) {
        return false;
    }
    if chars[at].c == '\u{3000}'
        && let Some(next) = chars[at..].iter().map(|c| c.c).find(|&c| c != '\u{3000}')
    {
        return !Kinsoku::POWERPOINT.no_start.contains(next);
    }
    true
}

/// How a paragraph's lines break: whether punctuation hangs (`hangingPunct`) and where Latin text breaks (`latinLnBrk`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Breaks {
    pub(crate) hang: bool,
    pub(crate) latin: LatinBreaks,
}

impl Default for Breaks {
    fn default() -> Self {
        Breaks { hang: true, latin: LatinBreaks::default() }
    }
}

/// Punctuation that may hang past the end of a line (`hangingPunct`): the Japanese and the Latin commas and full stops (PowerPoint's PDFs of
/// `kinsoku-*-hang1-probe.pptx`; not the closing brackets, nor ｡ ､).
fn hangs(c: char) -> bool {
    matches!(c, '、' | '。' | '，' | '．' | ',' | '.')
}

pub(crate) fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x2460..=0x24ff | 0x25a0..=0x25ff | 0x2e80..=0x9fff | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xffef | 0x20000..=0x3ffff)
}

/// Whether `c` goes with the character before it into one glyph and takes no room of its own: a variation selector
/// (U+FE00..FE0F, and the ideographic ones U+E0100..E01EF that make an IVS with a kanji), the zero width joiner, an emoji
/// modifier (skin tone), a tag character (U+E0020..E007F) or the combining enclosing keycap.
pub(crate) fn is_cluster_extender(c: char) -> bool {
    matches!(c as u32, 0xFE00..=0xFE0F | 0xE0100..=0xE01EF | 0x200D | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F | 0x20E3)
        // The combining marks (Unicode's Mn blocks for the Latin and Japanese text: accents, the voiced and semi-voiced marks of kana, the ideographic tone
        // marks, symbol marks, half marks): part of the character before them, with no advance of their own (E5).
        || matches!(c as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0x302A..=0x302F | 0x3099..=0x309A | 0xFE20..=0xFE2F)
}

/// Whether `c` (after `prev`) is a part of the character cluster `prev` is in: an extender, or the character a joiner joins.
pub(crate) fn joins_previous(prev: Option<char>, c: char) -> bool {
    is_cluster_extender(c) || prev == Some('\u{200D}')
}

/// The start and the end (exclusive) of the cluster that holds `chars[at]`.
pub(crate) fn cluster_bounds(chars: &[char], at: usize) -> (usize, usize) {
    let mut start = at;
    while start > 0 && joins_previous(Some(chars[start - 1]), chars[start]) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < chars.len() && joins_previous(Some(chars[end - 1]), chars[end]) {
        end += 1;
    }
    (start, end)
}

/// The end (byte offset, exclusive) of the cluster that starts at byte `start` of `text`.
pub(crate) fn cluster_end(text: &str, start: usize) -> usize {
    let mut prev: Option<char> = None;
    for (i, c) in text[start..].char_indices() {
        if i > 0 && !joins_previous(prev, c) {
            return start + i;
        }
        prev = Some(c);
    }
    text.len()
}

/// Whether the cluster is an emoji: it has the emoji presentation selector (U+FE0F) or a joiner, or a character of the
/// pictograph blocks (U+1F300..1FAFF), or one Unicode draws as an emoji by default outside them.
pub(crate) fn is_emoji_cluster(cluster: &[char]) -> bool {
    cluster.iter().any(|&c| matches!(c as u32, 0xFE0F | 0x200D | 0x1F300..=0x1FAFF) || emoji_presentation(c))
}

/// The characters below U+1F300 whose `Emoji_Presentation` is `Yes` (Unicode's emoji-data.txt): drawn as an emoji without
/// the selector (✨ ⌛ ⚽ ⭐ ❌ …), so the emoji typeface draws them as it draws the pictographs.
fn emoji_presentation(c: char) -> bool {
    matches!(c as u32,
        0x231A..=0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3 | 0x25FD..=0x25FE | 0x2614..=0x2615 | 0x2648..=0x2653 | 0x267F
        | 0x2693 | 0x26A1 | 0x26AA..=0x26AB | 0x26BD..=0x26BE | 0x26C4..=0x26C5 | 0x26CE | 0x26D4 | 0x26EA | 0x26F2..=0x26F3
        | 0x26F5 | 0x26FA | 0x26FD | 0x2705 | 0x270A..=0x270B | 0x2728 | 0x274C | 0x274E | 0x2753..=0x2755 | 0x2757
        | 0x2795..=0x2797 | 0x27B0 | 0x27BF | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55 | 0x1F004 | 0x1F0CF | 0x1F18E
        | 0x1F191..=0x1F19A | 0x1F1E6..=0x1F1FF | 0x1F201 | 0x1F21A | 0x1F22F | 0x1F232..=0x1F236 | 0x1F238..=0x1F23A
        | 0x1F250..=0x1F251)
}

/// The colour emoji typeface of the OS: what Word draws emoji with (Segoe UI Emoji on Windows; Apple Color Emoji on a Mac).
pub(crate) fn emoji_family() -> &'static str {
    if cfg!(target_os = "macos") {
        "Apple Color Emoji"
    } else if cfg!(target_os = "windows") {
        "Segoe UI Emoji"
    } else {
        "Noto Color Emoji"
    }
}

/// `font` in the colour emoji typeface.
pub(crate) fn emoji_font(font: &Font) -> Font {
    let mut f = font.clone();
    f.family = emoji_family().into();
    f
}

/// The deviation of PowerPoint's text shadow blur per unit of `blurRad`. Measured on the shadow bitmaps of PowerPoint
/// 16.0.20527.20088's PDFs (`a:rPr/a:effectLst/a:outerShdw`, black 43%, 54pt text): PowerPoint draws the shadow at a resolution
/// (200 dpi for blurRad 1.5pt and 3pt, 100 dpi for 6pt, 50 dpi for 12pt, 28.7 dpi for 24pt) that keeps the blur radius at 8.3 to
/// 9.5 bitmap pixels, and the blur is about 2.2 to 2.5 pixels' deviation (0.264 of the radius in every case). Fitted
/// (the deviation of an extra Gaussian that turns the 1.5pt shadow's mask into the 6pt and 24pt masks, pixel footprint included;
/// and the edge of the 3pt mask, and of the 12pt mask from the 3pt mask): 0.79pt for 3pt, 3.2pt for 12pt, 1.58pt for 6pt,
/// 6.45pt for 24pt, that is 0.263, 0.266, 0.263 and 0.269 of blurRad.
const TEXT_SHADOW_SIGMA_PER_BLUR: f32 = 0.265;

/// The offsets (pixels) at which a text shadow of blur radius `blur` is drawn: one at (0, 0) when the blur is under a
/// pixel, else 3 rings of 8 points whose radii cut the Gaussian of deviation `TEXT_SHADOW_SIGMA_PER_BLUR * blur` into 3 parts of equal
/// probability (the radius of the middle of each part: `sigma * sqrt(-2 ln(1 - p))` at p = 1/6, 1/2, 5/6), the rings turned
/// by a third of the angle between points so that the points do not line up, and the radii scaled so that the offsets' variance is sigma^2 in each direction.
fn text_shadow_offsets(blur: f32) -> Vec<(f32, f32)> {
    if blur < 1.0 {
        return vec![(0.0, 0.0)];
    }
    let sigma = blur * TEXT_SHADOW_SIGMA_PER_BLUR;
    let mut offsets = Vec::with_capacity(24);
    let ps = [1.0f32 / 6.0, 0.5, 5.0 / 6.0];
    // The rings' mean square radius is made 2 * sigma^2 (the variance of the 2-D Gaussian), as the middles of the parts alone fall short of it.
    let mean_square = ps.iter().map(|p| -2.0 * (1.0 - p).ln()).sum::<f32>() / ps.len() as f32;
    let fit = (2.0 / mean_square).sqrt();
    for (ring, p) in ps.into_iter().enumerate() {
        let radius = sigma * fit * (-2.0 * (1.0 - p).ln()).sqrt();
        for k in 0..8 {
            let angle = (k as f32 + ring as f32 / 3.0) / 8.0 * std::f32::consts::TAU;
            let (s, c) = angle.sin_cos();
            offsets.push((c * radius, s * radius));
        }
    }
    offsets
}

#[cfg(test)]
mod tests {
    use super::{Breaks, Char, CharPosition, LinePosition, ParagraphPositions, break_lines, text_shadow_offsets, TEXT_SHADOW_SIGMA_PER_BLUR};

    /// The shadow's blur is a Gaussian of deviation 0.265 blurRad (PowerPoint's PDFs of blurRad 3pt, 6pt, 12pt and 24pt): the offsets' variance in each direction, whatever the radius.
    #[test]
    fn text_shadow_offsets_have_the_deviation_of_the_blur_fraction() {
        for blur in [4.0f32, 8.3, 33.0] {
            let offsets = text_shadow_offsets(blur);
            let n = offsets.len() as f32;
            let var_x = offsets.iter().map(|o| o.0 * o.0).sum::<f32>() / n;
            let var_y = offsets.iter().map(|o| o.1 * o.1).sum::<f32>() / n;
            let sigma = blur * TEXT_SHADOW_SIGMA_PER_BLUR;
            assert!((var_x / (sigma * sigma) - 1.0).abs() < 0.08 && (var_y / (sigma * sigma) - 1.0).abs() < 0.08, "{blur}: {var_x} {var_y} against {}", sigma * sigma);
        }
        assert_eq!(text_shadow_offsets(0.5), vec![(0.0, 0.0)]);
    }

    /// "ab c" wrapped after "ab " (10 px a character), then "de".
    fn positions() -> ParagraphPositions {
        let chars = [(0.0, 0), (10.0, 0), (20.0, 0), (0.0, 1), (10.0, 1)]
            .map(|(x, line)| CharPosition { x, advance: 10.0, line })
            .to_vec();
        let lines = vec![
            LinePosition { start: 0, end: 3, x: 0.0, top: 0.0, baseline: 8.0, height: 12.0, column: 0, left: 0.0, right: 100.0 },
            LinePosition { start: 3, end: 5, x: 0.0, top: 12.0, baseline: 20.0, height: 12.0, column: 0, left: 0.0, right: 100.0 },
        ];
        ParagraphPositions { chars, lines, ..Default::default() }
    }

    #[test]
    fn turns_compose_and_invert() {
        use super::Turn;
        let close = |a: (f32, f32), b: (f32, f32)| assert!((a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3, "{a:?} {b:?}");
        // A quarter turn clockwise about (10, 10): right of it goes below.
        let q = Turn::about(10.0, 10.0, 90.0);
        close(q.apply(20.0, 10.0), (10.0, 20.0));
        close(q.invert(10.0, 20.0), (20.0, 10.0));
        // Half a turn about one point, then back about another: a move.
        let t = Turn::about(0.0, 0.0, 180.0).then(Turn::about(5.0, 0.0, 180.0));
        close(t.apply(1.0, 1.0), (11.0, 1.0));
        assert!(t.turns() && t.radians().abs() < 1e-3);
        assert!(!Turn::about(3.0, 4.0, 0.0).turns());
    }

    #[test]
    fn caret_positions() {
        let p = positions();
        assert_eq!(p.caret(0), (0.0, 0));
        assert_eq!(p.caret(3), (0.0, 1));
        assert_eq!(p.caret(5), (20.0, 1));
    }

    #[test]
    fn click_positions() {
        let p = positions();
        assert_eq!(p.index_at(4.0, 5.0), 0);
        assert_eq!(p.index_at(6.0, 5.0), 1);
        // Past a wrapped line's end: before its trailing space.
        assert_eq!(p.index_at(100.0, 5.0), 2);
        assert_eq!(p.index_at(100.0, 15.0), 5);
        assert_eq!(p.index_at(12.0, 100.0), 4);
    }

    fn chars(text: &str, advance: f32) -> Vec<Char> {
        text.char_indices().map(|(i, c)| Char { piece: 0, start: i, end: i + c.len_utf8(), c, raw: advance, advance, lift: None, tcy: super::Tcy::No, east_asian: true }).collect()
    }

    /// PowerPoint's vertical text sets the digits two by two upright (found in PowerPoint: 26年 has 26 side by side, 2026年 has 20 and
    /// 26 one under the other, 026 has 02 and 6, 5年 has the 5 upright); a fullwidth digit and other characters are not set so.
    #[test]
    fn digits_of_vertical_text_are_set_two_by_two() {
        use super::{Tcy, set_tcy};
        let tcy = |text: &str| {
            let mut c = chars(text, 1.0);
            set_tcy(&mut c, 2.0);
            c.iter().map(|c| (c.tcy, c.advance)).collect::<Vec<_>>()
        };
        assert_eq!(tcy("26年"), vec![(Tcy::First, 1.0), (Tcy::Second, 1.0), (Tcy::No, 1.0)]);
        assert_eq!(tcy("2026年").iter().map(|t| t.0).collect::<Vec<_>>(), vec![Tcy::First, Tcy::Second, Tcy::First, Tcy::Second, Tcy::No]);
        assert_eq!(tcy("026").iter().map(|t| t.0).collect::<Vec<_>>(), vec![Tcy::First, Tcy::Second, Tcy::Alone]);
        assert_eq!(tcy("5年").iter().map(|t| t.0).collect::<Vec<_>>(), vec![Tcy::Alone, Tcy::No]);
        assert_eq!(tcy("A1B２").iter().map(|t| t.0).collect::<Vec<_>>(), vec![Tcy::No, Tcy::Alone, Tcy::No, Tcy::No]);
    }

    #[test]
    fn breaks_at_spaces() {
        let mut c = chars("ab cd ef", 1.0);
        // The space after "cd" hangs past the edge.
        assert_eq!(break_lines(&mut c, Some(5.0), 0.0, 0.0, &[], 0.0, 0.5, Breaks::default()), vec![(0, 6), (6, 8)]);
    }

    #[test]
    fn breaks_between_japanese_characters_but_not_before_punctuation() {
        let mut c = chars("あいうーえお", 1.0);
        // "うー" stays together: the first line breaks before "う".
        assert_eq!(break_lines(&mut c, Some(3.0), 0.0, 0.0, &[], 0.0, 0.5, Breaks::default()), vec![(0, 2), (2, 5), (5, 6)]);
    }

    #[test]
    fn a_hanging_indent_gives_the_first_line_more_room() {
        let mut c = chars("abcdef", 1.0);
        assert_eq!(break_lines(&mut c, Some(4.0), -2.0, 0.0, &[], 0.0, 0.5, Breaks::default()), vec![(0, 6)]);
        assert_eq!(break_lines(&mut c, Some(4.0), 1.0, 0.0, &[], 0.0, 0.5, Breaks::default()), vec![(0, 3), (3, 6)]);
    }

    #[test]
    fn a_comma_hangs_past_the_end_of_a_line() {
        let mut c = chars("あいう、え", 1.0);
        assert_eq!(break_lines(&mut c, Some(3.0), 0.0, 0.0, &[], 0.0, 0.5, Breaks::default()), vec![(0, 4), (4, 5)]);
    }

    /// #66: a character Unicode draws as an emoji by default is an emoji without the selector; one drawn as text by
    /// default is not, unless the selector asks for the emoji.
    #[test]
    fn default_emoji_need_no_selector() {
        for c in ['✨', '⌛', '⚽', '⭐', '❌', '🀄', '🎈'] {
            assert!(super::is_emoji_cluster(&[c]), "{c} is an emoji");
        }
        for c in ['♠', '✂', '☎', 'A', '字'] {
            assert!(!super::is_emoji_cluster(&[c]), "{c} is text");
        }
        assert!(super::is_emoji_cluster(&['♠', '\u{FE0F}']));
    }

    #[test]
    fn a_full_stop_inside_a_word_does_not_hang() {
        // "figshare.com" in a box of 8: the stop does not hang, the word breaks before it (PowerPoint's PDF of corpus 152).
        let mut c = chars("figshare.com", 1.0);
        assert_eq!(break_lines(&mut c, Some(8.0), 0.0, 0.0, &[], 0.0, 0.5, Breaks::default()), vec![(0, 8), (8, 12)]);
        // Before a space it does: the line breaks after the space.
        let mut c = chars("abcdefgh, ij", 1.0);
        assert_eq!(break_lines(&mut c, Some(8.0), 0.0, 0.0, &[], 0.0, 0.5, Breaks::default()), vec![(0, 10), (10, 12)]);
    }

    #[test]
    fn a_long_word_breaks_where_it_must() {
        let mut c = chars("abcdef", 1.0);
        assert_eq!(break_lines(&mut c, Some(4.0), 0.0, 0.0, &[], 0.0, 0.5, Breaks::default()), vec![(0, 4), (4, 6)]);
    }

    fn lines(text: &str, room: f32) -> Vec<(usize, usize)> {
        let mut c = chars(text, 1.0);
        break_lines(&mut c, Some(room), 0.0, 0.0, &[], 0.0, 0.5, Breaks::default())
    }

    /// Issue 68, P1: `℃ ° ′ ″` and the half-width `｡ ｣ ､ ･ ﾞ ｯ ｰ` do not start a line in PowerPoint; `― 〜 ‐` do.
    #[test]
    fn powerpoint_line_start_characters() {
        assert_eq!(lines("あいうえ℃お", 4.0), vec![(0, 3), (3, 6)]);
        assert_eq!(lines("あいうえｯお", 4.0), vec![(0, 3), (3, 6)]);
        assert_eq!(lines("あいう―え", 3.0), vec![(0, 3), (3, 5)]);
        assert_eq!(lines("あいう〜え", 3.0), vec![(0, 3), (3, 5)]);
    }

    /// Issue 68, P2: `＄ ￥ $` do not end a line.
    #[test]
    fn powerpoint_line_end_characters() {
        assert_eq!(lines("あいう＄えお", 4.0), vec![(0, 3), (3, 6)]);
        assert_eq!(lines("あいう$えお", 4.0), vec![(0, 3), (3, 6)]);
    }

    /// Issue 68, P3 and P4: the ASCII comma and full stop hang as the Japanese ones do; only one mark hangs, a second takes the
    /// character before the first down with it.
    #[test]
    fn powerpoint_hanging_marks() {
        assert_eq!(lines("あいう,え", 3.0), vec![(0, 4), (4, 5)]);
        assert_eq!(lines("あいう.え", 3.0), vec![(0, 4), (4, 5)]);
        assert_eq!(lines("あいうえ。。お", 4.0), vec![(0, 3), (3, 7)]);
    }

    /// Issue 68, P5, P6, P7 and P8: no break after `/` (`latinLnBrk="0"`), none inside `――`, one after an em dash, none after a no-break space.
    #[test]
    fn powerpoint_breaks_inside_latin_text_and_pairs() {
        assert_eq!(lines("あa/b/c/dいう", 4.0)[0], (0, 1));
        assert_eq!(lines("あいう――え", 4.0)[0], (0, 3));
        assert_eq!(lines("abc—def", 5.0)[0], (0, 4));
        assert_eq!(lines("あいう10\u{a0}kgえ", 6.0)[0], (0, 3));
        assert_eq!(lines("あいう10\u{202f}kgえ", 6.0)[0], (0, 3));
    }
}
