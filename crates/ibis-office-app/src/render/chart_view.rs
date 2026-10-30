// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing charts: bars, lines, areas, pies and doughnuts with their axes,
//! data labels and legend.
//!
//! Marks are painted on one canvas; text is laid out as elements so it uses
//! the same fonts as the rest of the slide.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{
    Axis, AxisKind, Chart, ChartDataTable, ChartEx, ChartGroup, ChartText, ChartType, DispBlanksAs, EMU_PER_POINT, EmuRect, Fill,
    FractionRect, Grouping, LabelPiece, LabelPosition, MarkerSymbol, OfPieKind, OfPieSplit, Series, Side, Stroke, TextStyle,
    format_number,
};

use crate::render::shape_view::{Resources, background, hsla, polygon};

/// A rectangle in pixels, relative to the chart frame.
#[derive(Clone, Copy, Debug)]
struct Area {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Area {
    fn right(&self) -> f32 {
        self.x + self.w
    }

    fn bottom(&self) -> f32 {
        self.y + self.h
    }

    fn of(rect: FractionRect, w: f32, h: f32) -> Self {
        let f = |v: i32| v as f32 / 100_000.0;
        Self { x: f(rect.x) * w, y: f(rect.y) * h, w: f(rect.width) * w, h: f(rect.height) * h }
    }
}

/// Something painted on the chart's canvas, in frame-relative pixels.
enum Mark {
    Fill(Vec<(f32, f32)>, Background),
    Stroke(Vec<(f32, f32)>, f32, Hsla),
    /// A dashed line: its points, width, colour, and the lengths of its dashes and gaps in turn (pixels). One path, and
    /// in a PDF one line with its dash pattern.
    Dashed(Vec<(f32, f32)>, f32, Hsla, Vec<f32>),
    /// The marks after this are cut to this rectangle (x, y, w, h), or not
    /// cut (`None`): the series are drawn inside the plot area, as Office
    /// cuts a line past the axis's maximum.
    Clip(Option<(f32, f32, f32, f32)>),
}

#[derive(Clone, Copy, PartialEq)]
enum Anchor {
    Center,
    /// Text starts at the point.
    Start,
    /// Text ends at the point.
    End,
}

struct Label {
    x: f32,
    y: f32,
    text: String,
    style: ChartText,
    anchor: Anchor,
    fill: Fill,
    border: Option<Stroke>,
}

impl Label {
    fn plain(x: f32, y: f32, text: String, style: ChartText, anchor: Anchor) -> Self {
        Self { x, y, text, style, anchor, fill: Fill::None, border: None }
    }
}

/// The soft shadow of chart style 10: the second effect style of Office's default theme (theme1.xml effectStyleLst, as in the file of GitHub #22: outerShdw blurRad 40000, dist 23000, dir 5400000, black at alpha 35000; the blur radius is taken as 2 sigma)
/// under a rectangle, as layers of one translucent fill each, the widest first (`scale`: pixels per EMU).
fn soft_shadow(marks: &mut Vec<Mark>, corners: &[(f32, f32)], scale: f32) {
    const LAYERS: usize = 6;
    let (alpha, sigma) = (0.35f32, 20_000.0 * scale);
    let (dx, dy) = (0.0, 22_860.0 * scale);
    let (x0, x1) = corners.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (y0, y1) = corners.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    // The shadow's opacity at a distance `d` outside the edge: a Gaussian edge's tail.
    let tail = |d: f32| alpha * 0.5 * (1.0 - erf_approx(d / (sigma * std::f32::consts::SQRT_2)));
    let step = 2.0 * sigma / LAYERS as f32;
    let mut covered = 0.0f32;
    for k in (0..LAYERS).rev() {
        // Layer k reaches (k + 1) steps out and lies over the wider ones.
        let want = tail((k as f32 + 0.5) * step);
        let a = ((want - covered) / (1.0 - covered)).clamp(0.0, 1.0);
        covered = want.max(covered);
        let e = (k + 1) as f32 * step;
        let rect = vec![(x0 - e + dx, y0 - e + dy), (x1 + e + dx, y0 - e + dy), (x1 + e + dx, y1 + e + dy), (x0 - e + dx, y1 + e + dy)];
        marks.push(Mark::Fill(rect, Hsla { h: 0.0, s: 0.0, l: 0.0, a }.into()));
    }
}

/// The error function (Abramowitz and Stegun 7.1.26).
fn erf_approx(x: f32) -> f32 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let y = 1.0 - (((((1.061_405_4 * t - 1.453_152_1) * t) + 1.421_413_7) * t - 0.284_496_72) * t + 0.254_829_6) * t * (-x * x).exp();
    if x >= 0.0 { y } else { -y }
}

/// The key of a line series in a legend: its line across `key_w`, whole or, of a dashed series (`a:prstDash`), in dashes.
fn key_line(stroke: Option<&ibis_doc_model::Stroke>, key_w: f32, size: f32, thickness: f32, color: Option<Hsla>) -> Vec<Div> {
    let pattern = stroke.map_or(&[][..], |s| s.dash.pattern());
    let top = size * 0.5 - thickness / 2.0;
    let bar = |left: f32, width: f32| div().absolute().left(px(left)).w(px(width)).top(px(top)).h(px(thickness)).when_some(color, |d, c| d.bg(c));
    if pattern.is_empty() {
        return vec![div().absolute().left_0().w_full().top(px(top)).h(px(thickness)).when_some(color, |d, c| d.bg(c))];
    }
    let mut out = Vec::new();
    let (mut at, mut i) = (0.0f32, 0usize);
    while at < key_w && out.len() < 64 {
        let length = (pattern[i % pattern.len()] * thickness).max(0.5);
        if i % 2 == 0 {
            out.push(bar(at, length.min(key_w - at)));
        }
        at += length;
        i += 1;
    }
    out
}

/// A line as a mark: whole, or, of a dashed stroke (`a:prstDash`), dashed as its pattern says (its lengths in line widths,
/// ECMA-376 §20.1.10.48; a line thinner than a pixel dashes as one of a pixel).
fn push_stroke(marks: &mut Vec<Mark>, points: Vec<(f32, f32)>, width: f32, stroke: &ibis_doc_model::Stroke) {
    let pattern = stroke.dash.pattern();
    if pattern.is_empty() || points.len() < 2 {
        marks.push(Mark::Stroke(points, width, hsla(stroke.color)));
        return;
    }
    let unit = width.max(1.0);
    marks.push(Mark::Dashed(points, width, hsla(stroke.color), pattern.iter().map(|l| l * unit).collect()));
}

/// The height of a row of the outer levels of a category axis of several levels: the labels' line, with the room
/// the lines that part the categories need above and below it.
fn level_row_height(axis: &Axis, px_per_point: f32) -> f32 {
    text_height(&axis.text, px_per_point) + 8.0 * px_per_point
}

/// Wraps a horizontal category label to `max_width`, breaking after hyphens or spaces first.
fn wrap_category_label(text: &str, max_width: f32, style: &ChartText, px_per_point: f32) -> String {
    if max_width <= 0.0 || text_width(text, style, px_per_point) <= max_width {
        return text.to_string();
    }
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        cur.push(ch);
        if ch == '-' || ch == ' ' {
            tokens.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    if tokens.len() <= 1 {
        return text.to_string();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for tok in tokens {
        let candidate = format!("{line}{tok}");
        if !line.is_empty() && text_width(&candidate, style, px_per_point) > max_width {
            lines.push(std::mem::take(&mut line));
            line = tok;
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines.join("\n")
}

/// Shortens a horizontal-bar category label with `"…"` when it is wider than `max_width`.
fn ellipsize_category_label(text: &str, max_width: f32, style: &ChartText, px_per_point: f32, res: &Resources) -> String {
    let size = style.size as f32 / 100.0 * px_per_point;
    let font = chart_font(style, text, res);
    if max_width <= 0.0 || res.text_width(text, &font, size) <= max_width {
        return text.to_string();
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > 1 {
        for count in (1..words.len()).rev() {
            let candidate = format!("{}…", words[..count].join(" "));
            if res.text_width(&candidate, &font, size) <= max_width {
                return candidate;
            }
        }
    }
    let chars: Vec<char> = text.chars().collect();
    for count in (1..chars.len()).rev() {
        let prefix: String = chars[..count].iter().collect();
        let candidate = format!("{}…", prefix.trim_end());
        if res.text_width(&candidate, &font, size) <= max_width {
            return candidate;
        }
    }
    "…".to_string()
}

/// Wraps `text` onto lines of at most `max_width` using measured glyph widths.
fn wrap_text_to_width(text: &str, max_width: f32, style: &ChartText, px_per_point: f32, res: &Resources) -> String {
    let size = style.size as f32 / 100.0 * px_per_point;
    let font = chart_font(style, text, res);
    if max_width <= 0.0 {
        return text.to_string();
    }
    let mut out_lines: Vec<String> = Vec::new();
    for raw_line in text.lines() {
        if res.text_width(raw_line, &font, size) <= max_width {
            out_lines.push(raw_line.to_string());
            continue;
        }
        let mut tokens: Vec<String> = Vec::new();
        let mut cur = String::new();
        for ch in raw_line.chars() {
            cur.push(ch);
            if ch == ' ' || ch == '-' {
                tokens.push(std::mem::take(&mut cur));
            }
        }
        if !cur.is_empty() {
            tokens.push(cur);
        }
        let mut line = String::new();
        for tok in tokens {
            let candidate = format!("{line}{tok}");
            if !line.is_empty() && res.text_width(candidate.trim_end(), &font, size) > max_width {
                out_lines.push(line.trim_end().to_string());
                line = tok.trim_start().to_string();
            } else {
                line = candidate;
            }
        }
        if !line.trim_end().is_empty() {
            out_lines.push(line.trim_end().to_string());
        }
    }
    if out_lines.is_empty() { text.to_string() } else { out_lines.join("\n") }
}

fn wrapped_title_text(title: &ibis_doc_model::ChartTitle, w: f32, px_per_point: f32, res: &Resources) -> String {
    let max_w = match title.at {
        Some((fx, _)) => w - fx as f32 / 100_000.0 * w - 6.0 * px_per_point,
        None => w - 2.0 * CHART_PADDING_PT * px_per_point,
    };
    wrap_text_to_width(&title.text, max_w.max(w * 0.2), &title.style, px_per_point, res)
}

fn data_table_row_height(chart: &Chart, px_per_point: f32) -> f32 {
    let cat_size = chart
        .axes
        .iter()
        .find(|a| a.kind == AxisKind::Category)
        .map_or(9.0 * px_per_point, |a| a.text.size as f32 / 100.0 * px_per_point);
    (cat_size * 1.35).max(13.0 * px_per_point)
}

/// `color` made darker (a 3-D chart's sides) by `factor` of its lightness.
fn shaded(color: Hsla, factor: f32) -> Hsla {
    Hsla { l: (color.l * factor).clamp(0.0, 1.0), ..color }
}

/// The sides of a 3-D shape: the polygon `front` drawn again `ext` further back (up and to the right on
/// the page, as `ext = (dx, dy)` with `dy` up), each edge a quad; the shape's own front is painted over
/// them afterwards. The edges that run level get the lighter shade (the top), the others the darker.
fn side_faces(marks: &mut Vec<Mark>, front: &[(f32, f32)], ext: (f32, f32), color: Hsla) {
    let n = front.len();
    for i in 0..n {
        let (p, q) = (front[i], front[(i + 1) % n]);
        let level = (q.1 - p.1).abs() <= (q.0 - p.0).abs();
        let shade = if level { 0.85 } else { 0.68 };
        let quad = vec![p, q, (q.0 + ext.0, q.1 - ext.1), (p.0 + ext.0, p.1 - ext.1)];
        marks.push(Mark::Fill(quad, shaded(color, shade).into()));
    }
}

/// A 3-D line's ribbon: each segment of `line` drawn again `ext` further back.
fn ribbon_faces(marks: &mut Vec<Mark>, line: &[(f32, f32)], ext: (f32, f32), color: Hsla) {
    for pair in line.windows(2) {
        let (p, q) = (pair[0], pair[1]);
        let quad = vec![p, q, (q.0 + ext.0, q.1 - ext.1), (p.0 + ext.0, p.1 - ext.1)];
        marks.push(Mark::Fill(quad, shaded(color, 0.72).into()));
    }
}

/// The value range an axis shows and its tick spacing.
#[derive(Clone, Copy, Debug)]
struct Scale {
    min: f64,
    max: f64,
    step: f64,
    reversed: bool,
    log_base: Option<f64>,
}

impl Scale {
    /// Position of `v` along the axis, 0 at the start and 1 at the end.
    fn at(&self, v: f64) -> f32 {
        let t = if let Some(b) = self.log_base {
            let v_safe = v.max(self.min).max(1e-12);
            let min_log = self.min.max(1e-12).log(b);
            let max_log = self.max.max(1e-12).log(b);
            let span = max_log - min_log;
            if span.abs() > 1e-12 {
                ((v_safe.log(b) - min_log) / span) as f32
            } else {
                0.0
            }
        } else {
            ((v - self.min) / (self.max - self.min)) as f32
        };
        if self.reversed { 1.0 - t } else { t }
    }

    /// The range for values from `low` to `high`, with at most `intervals`
    /// ticks' intervals when the axis chooses its own step.
    fn fit(low: f64, high: f64, axis: Option<&Axis>, intervals: f64) -> Self {
        let fixed_min = axis.and_then(|a| a.min).map(|n| n.0);
        let fixed_max = axis.and_then(|a| a.max).map(|n| n.0);
        let log_base = axis.and_then(|a| a.log_base).map(|n| n.0).filter(|b| *b > 1.0);
        if let Some(b) = log_base {
            let low_pos = if low > 0.0 { low } else { 1.0 };
            let min = fixed_min.filter(|m| *m > 0.0).unwrap_or_else(|| b.powf(low_pos.log(b).floor()));
            let max = fixed_max.filter(|m| *m > min).unwrap_or_else(|| b.powf(high.max(min * b).log(b).ceil()));
            return Self { min, max, step: b, reversed: axis.is_some_and(|a| a.reversed), log_base: Some(b) };
        }
        let raw_low = low;
        // Microsoft KB214075: when all values are positive and the span is less
        // than 1/6 of the maximum (`low > high * 5 / 6`), the automatic axis
        // minimum does not drop to zero.
        let narrow_positive = fixed_min.is_none() && raw_low > 0.0 && high > raw_low && (high - raw_low) < high / 6.0;
        let low = fixed_min.unwrap_or_else(|| if narrow_positive { raw_low } else { raw_low.min(0.0) });
        // Like Excel, leave 5% of the range free above the highest value.
        let high = fixed_max.unwrap_or(high + (high - low).abs() * 0.05).max(low);
        // Values all zero (an empty template), with nothing fixed: Excel's axis goes 0 to 1 in 5 steps (0.2).
        let all_zero = fixed_max.is_none() && high == 0.0 && low == 0.0;
        let high = if all_zero { 1.0 } else { high };
        let range = if high > low { high - low } else { 1.0 };
        let intervals = if all_zero { intervals.min(5.0) } else { intervals };
        let step = axis.and_then(|a| a.major_unit).map(|n| n.0).filter(|s| *s > 0.0).unwrap_or_else(|| nice_step(range / intervals));
        // Below zero, PowerPoint leaves a whole step under the lowest value
        // (measured against its PDF output: -35 and -50 both go to -100).
        let min = fixed_min.unwrap_or_else(|| {
            if low < 0.0 {
                ((low / step).floor() - 1.0) * step
            } else if narrow_positive {
                let floored = (low / step).floor() * step;
                if low - floored < step * 0.5 { (floored - step).max(0.0) } else { floored }
            } else {
                (low / step).floor() * step
            }
        });
        let mut max = fixed_max.unwrap_or_else(|| (high / step).ceil() * step);
        if max <= min {
            max = min + step;
        }
        Self { min, max, step, reversed: axis.is_some_and(|a| a.reversed), log_base: None }
    }

    fn ticks(&self) -> Vec<f64> {
        if let Some(b) = self.log_base {
            let mut ticks = Vec::new();
            let min_exp = (self.min.max(1e-12).log(b)).floor() as i32;
            let max_exp = (self.max.max(1e-12).log(b)).ceil() as i32;
            for exp in min_exp..=max_exp {
                let val = b.powi(exp);
                if val >= self.min - 1e-9 && val <= self.max + 1e-9 {
                    ticks.push(val);
                }
            }
            if ticks.is_empty() {
                ticks.push(self.min);
                ticks.push(self.max);
            }
            ticks
        } else {
            // Only ticks within the range: a maximum set between steps (7.7
            // million in steps of a million) has none at its end.
            let count = ((self.max - self.min) / self.step + 1e-6).floor().clamp(0.0, 50.0) as usize;
            (0..=count).map(|i| self.min + self.step * i as f64).collect()
        }
    }
}

/// How many intervals an axis `length` pixels long divides into at most
/// when it chooses its own step: PowerPoint leaves each tick label about
/// one and a half times its extent along the axis (its height on an upright axis, its
/// width on a horizontal one), and never more than ten intervals.
/// (Measured against its PDF output: slide 24's 0–90,000 in steps of
/// 10,000, slide 12's horizontal bar to 1.5, slide 19's 0–600 in 100s.)
fn tick_intervals(axis: Option<&Axis>, low: f64, high: f64, length: f32, horizontal: bool, px_per_point: f32, tight: bool, word: bool) -> f64 {
    let text = axis.map(|a| a.text.clone()).unwrap_or_default();
    let size = text.size as f32 / 100.0 * px_per_point;
    let extent = if horizontal {
        let unit = axis.and_then(|a| a.display_unit).map(|u| u.0).unwrap_or(1.0);
        let format = axis.map(|a| a.format.as_str()).unwrap_or("General");
        let widest = [low, high].iter().map(|v| number_width(&format_number(v / unit, format))).fold(0.0, f32::max);
        // The widest label and two ems of room (measured on PCHT's
        // horizontal bars: 0–6 in 2s, 0–100% in 50%s across 134 pt).
        widest * size + 2.0 * size
    } else {
        size * 1.2
    };
    if extent <= 0.0 {
        return 10.0;
    }
    // (Excel's own charts put the labels of an upright axis closer: 11 steps of -40..180 in a 119 pt plot, 9 pt text.)
    if !horizontal && !tight && !word {
        // PowerPoint's upright value axis (the PDFs of a 2-D line chart, automatic axis 0..20, 7, 10, 14 and 18 pt text, plots 10 to 254pt high): the
        // labels, a line (1.2 em) each, stacked one more than the intervals, fit the axis and half a label over; and at most ten intervals.
        // `intervals + 1` labels: `(intervals + 1) * line <= length + line / 2`; every one of the 32 axes' step is the smallest of 1, 2 and 5 times
        // a power of ten that makes at most that many intervals.
        return (((length / extent) - 0.5).floor() as f64).clamp(1.0, 10.0);
    }
    // (A Word chart keeps the earlier factor, 1.4: its automatic intervals are not measured yet.)
    let factor = if horizontal { 1.5 } else if tight { 1.0 } else { 1.4 };
    ((length / (extent * factor)).floor() as f64).clamp(2.0, 10.0)
}

/// Horizontal bars use the labels of a preliminary axis to estimate
/// their spacing. A data maximum of 7.3 is labelled 8; the raw decimal
/// would unnecessarily reduce its axis to 0, 5 and 10. Scatter axes keep
/// their existing extent estimate until their independent layout is fitted.
fn bar_tick_intervals(axis: Option<&Axis>, low: f64, high: f64, length: f32, px_per_point: f32) -> f64 {
    let labels = Scale::fit(low, high, axis, 10.0);
    tick_intervals(axis, labels.min, labels.max, length, true, px_per_point, false, false)
}

/// 1, 2 or 5 times a power of ten, at least `raw`. With a tenth of the
/// range, this is the smallest such step that divides the axis into at
/// most ten intervals, as Excel chooses it.
fn nice_step(raw: f64) -> f64 {
    if raw <= 0.0 || !raw.is_finite() {
        return 1.0;
    }
    let magnitude = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * magnitude).find(|s| *s >= raw).unwrap_or(10.0 * magnitude)
}

fn series_color(series: &Series) -> Option<Hsla> {
    match &series.fill {
        Fill::Solid(c) => Some(hsla(*c)),
        Fill::Linear { stops, .. } | Fill::TileLinear { stops, .. } => stops.first().map(|s| hsla(s.color)),
        _ => series.stroke.as_ref().map(|s| hsla(s.color)),
    }
}

fn point_fill(series: &Series, index: usize) -> &Fill {
    series.point_fills.iter().find(|(i, _)| *i == index).map(|(_, f)| f).unwrap_or(&series.fill)
}

/// A point's explicit fill wins; otherwise use the marker's own fill.
/// The series fill belongs to the line/area and can differ from its marker.
fn marker_fill<'a>(series: &'a Series, index: usize, marker: &'a ibis_doc_model::Marker) -> &'a Fill {
    series.point_fills.iter().find(|(i, _)| *i == index).map(|(_, fill)| fill).unwrap_or(&marker.fill)
}

fn point_stroke(series: &Series, index: usize) -> Option<&Stroke> {
    match series.point_strokes.iter().find(|(i, _)| *i == index) {
        Some((_, stroke)) => stroke.as_ref(),
        None => series.stroke.as_ref(),
    }
}

fn is_round(group: &ChartGroup) -> bool {
    matches!(group.kind, ChartType::Pie { .. } | ChartType::Doughnut { .. } | ChartType::OfPie { .. })
}

fn is_of_pie(group: &ChartGroup) -> bool {
    matches!(group.kind, ChartType::OfPie { .. })
}

fn is_radar(group: &ChartGroup) -> bool {
    matches!(group.kind, ChartType::Radar { .. })
}

fn is_surface(group: &ChartGroup) -> bool {
    matches!(group.kind, ChartType::Surface { .. })
}

fn surface_min_max(group: &ChartGroup) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for ser in &group.series {
        for v in ser.values.iter().flatten() {
            if v.0 < min { min = v.0; }
            if v.0 > max { max = v.0; }
        }
    }
    if min.is_infinite() || max.is_infinite() {
        (0.0, 100.0)
    } else if (max - min).abs() < 1e-6 {
        (min, min + 10.0)
    } else {
        (min, max)
    }
}

fn surface_band_color(t: f32) -> Hsla {
    let t_clamped = t.clamp(0.0, 1.0);
    if t_clamped < 0.25 {
        hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x5B9BD5)))
    } else if t_clamped < 0.50 {
        hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xED7D31)))
    } else if t_clamped < 0.75 {
        hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xA5A5A5)))
    } else {
        hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xFFC000)))
    }
}

/// The radius of a chart area's rounded corners, in EMU.
const ROUNDED_CORNER_EMU: f32 = 8.0 * 12_700.0;

/// Draw `chart` filling `frame` (frame-relative, so x and y are ignored).
/// Where an axis sits follows where it crosses the axis it crosses (`crosses`: at that one's start, at its
/// end; the default is at zero, which is at the start for the axes drawn here), turned about when that
/// axis runs the other way round (a reversed category axis puts the value axis at the right, or at the top
/// of horizontal bars; a reversed value axis puts the category axis at the top). An axis that crosses at
/// the far end (`crosses` at the maximum, with the other axis the usual way round) is drawn on the far
/// side (right, or top) whatever its `axPos` says, as Excel and PowerPoint work it out from the crossing.
/// `None` if no axis moves.
fn crossing_adjusted(chart: &Chart) -> Option<Chart> {
    let moves = |a: &Axis| -> bool {
        if a.crosses_at.is_some() || a.deleted || !matches!(a.kind, AxisKind::Value | AxisKind::Category) {
            return false;
        }
        let Some(other) = a.cross_axis_id.and_then(|id| chart.axes.iter().find(|b| b.source_id == Some(id) || b.id == id)) else { return false };
        let far = (a.crosses == Some(ibis_doc_model::AxisCrosses::Maximum)) != other.reversed;
        let declared_far = matches!(a.side, Side::Right | Side::Top);
        far != declared_far
    };
    if !chart.axes.iter().any(|a| moves(a)) {
        return None;
    }
    let mut copy = chart.clone();
    for (axis, original) in copy.axes.iter_mut().zip(&chart.axes) {
        if moves(original) {
            axis.side = match axis.side {
                Side::Left => Side::Right,
                Side::Right => Side::Left,
                Side::Top => Side::Bottom,
                Side::Bottom => Side::Top,
            };
        }
    }
    Some(copy)
}

/// The value the bars (and areas) of `group` start from: where its category axis crosses the value axis
/// (zero by default, the scale's end for `crosses` at the minimum or maximum, or `crossesAt`).
fn base_value_of(chart: &Chart, group: &ChartGroup, s: &Scale) -> f64 {
    let at = group.axes.iter().filter_map(|id| chart.axis(*id)).find(|a| a.kind == AxisKind::Category);
    let value = match at {
        Some(a) => match (a.crosses_at, a.crosses) {
            (Some(v), _) => v.0,
            (None, Some(ibis_doc_model::AxisCrosses::Maximum)) => s.max,
            (None, Some(ibis_doc_model::AxisCrosses::Minimum)) => s.min,
            _ => 0.0,
        },
        None => 0.0,
    };
    value.clamp(s.min, s.max)
}

/// Whether a number format shows a percentage with no decimal digits (`0%`, `#0%`, `#,##0%`, `0 %`): its first section has a `%` and no `.` before it. Only `0%` was checked against PowerPoint.
fn percent_format_without_decimals(format: &str) -> bool {
    let first = format.split(';').next().unwrap_or("");
    let quoted_out: String = first.split('"').step_by(2).collect();
    quoted_out.contains('%') && !quoted_out.contains('.')
}

/// The whole percentages of a pie's values (those over zero) that add up to 100: each share rounded down, then the 1% that are missing given, one each, to the shares with the largest
/// fractions (the first of equals first). PowerPoint 16.0.20527.20088's PDF of nine pies (`chart-pie-percent-rounding`, labels `0%`): 8.2 3.2 1.4 1.2 0.9 is 55 22 9 8 6 (plain rounding:
/// 21 for the second, 99 in all) and three equal values are 34 33 33; a share with a decimal format (`0.0%`) is rounded alone (33.3 33.3 33.3).
fn whole_percentages(values: &[Option<ibis_doc_model::Number>]) -> Option<Vec<Option<i64>>> {
    let positive: Vec<f64> = values.iter().map(|v| v.map(|n| n.0).filter(|v| *v > 0.0).unwrap_or(0.0)).collect();
    let total: f64 = positive.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let shares: Vec<f64> = positive.iter().map(|v| v / total * 100.0).collect();
    let mut whole: Vec<i64> = shares.iter().map(|s| (s + 1e-9).floor() as i64).collect();
    let mut missing = 100 - whole.iter().sum::<i64>();
    let mut order: Vec<usize> = (0..shares.len()).filter(|i| positive[*i] > 0.0).collect();
    order.sort_by(|a, b| {
        let (fa, fb) = (shares[*a] - whole[*a] as f64, shares[*b] - whole[*b] as f64);
        fb.partial_cmp(&fa).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(b))
    });
    for i in order {
        if missing <= 0 {
            break;
        }
        whole[i] += 1;
        missing -= 1;
    }
    Some(whole.into_iter().zip(positive).map(|(w, v)| (v > 0.0).then_some(w)).collect())
}

/// Whether a pie's data label, with no `c:separator`, stands on two lines: a category name with the percentage and nothing else (PowerPoint 16.0.20527.20088's PDFs of every
/// pair and triple of category name, value, percentage and series name, at bestFit, outEnd and ctr: all the others are joined with ", ").
fn pie_label_on_two_lines(labels: &ibis_doc_model::DataLabels) -> bool {
    labels.separator.is_none() && labels.show_category && labels.show_percent && !labels.show_value && !labels.show_series_name
}

/// The front plane and the depth offsets (right, up) of a right-angle 3-D chart's box in a presentation, fitted into the plot area `inner`
/// (the `inner` manual layout, or the automatic plot area). From PowerPoint 16.0.20527.20088's PDFs of 3-D column charts (the front plane's ticks and
/// the back wall's edges in the wall bitmap; 25 charts: rotX 0, 15, 30, rotY 0, 20, 40, depth 50, 100, 200 %, hPercent 50 and 200, 1 to 3 series, 6 and 12
/// categories, gapWidth, gapDepth, `standard` and `clustered` grouping, and the plot area's size):
/// - the box is a front plane of `n_cat` bands, and a depth `D`; the depth goes right by D x sin(rotY) and up by D x sin(rotX) (0.342, 0.639, 0 of the
///   band for rotY 20, 40, 0 and 0.259, 0.503, 0 for rotX 15, 30, 0; measured 0.342, 0.639, 0.003 and 0.261, 0.503, 0.002);
/// - `D` is a band for each series in `standard` grouping (series one behind the other; gapWidth and gapDepth change nothing); in `clustered` grouping (the
///   series side by side in a row) it is a bar's width x (1 + gapDepth) where a bar's width is the band over (series - (series - 1) x overlap + gapWidth)
///   (a band for one series with the gaps at 150; 0.714, 0.556 of a band for 2 and 3 series, 0.714 for 3 series at gapWidth 50, 0.333 at gapDepth 50; measured
///   0.707, 0.547, 0.707, 0.33); times the depth percentage;
/// - the plane's height is its width times a ratio times the height percentage; when there is none (automatic), the height percentage is 100 x the plot area's
///   height over its width. For `clustered` grouping the ratio is 1, as ECMA-376 says (`c:hPercent` is the height as a percentage of the width: 0.7096 of the width in a
///   351.9 x 249.66pt plot area, for 1, 2 and 3 series, whatever the depth; hPercent 50 gives 0.5). For `standard` it is 0.5 (0.3548 of the width; hPercent 50 and 200 give
///   0.25 and 1.0) for one series, and PowerPoint's is 7/12 from two series on (0.4139 for 2 and for 3 series, 12 categories): the 0.5 and the 7/12 are as measured,
///   and why is not understood (not the depth: one series at depth 200% is 0.3548 as at 100%, and 2 and 3 series have 2 and 3 bands of depth and the same 0.4139), so only the
///   0.5 is used;
/// - the largest box that fits goes in the plot area, centred. (PowerPoint's boxes are about 2pt smaller on each side, 1.6% of the width: the front plane's left
///   edge is 1.97pt inside the plot area in all the charts, and 0 to 2.7pt short on the right; nothing about the walls' thickness or lines explains it, so it is left out.)
fn powerpoint_3d_box(inner: Area, view: &ibis_doc_model::View3D, n_cat: usize, n_ser: usize, gap_width: u32, overlap: i32, px_per_point: f32) -> (Area, f32, f32) {
    let _ = px_per_point;
    let (avail_w, avail_h) = (inner.w.max(10.0), inner.h.max(10.0));
    let series = n_ser.max(1) as f32;
    let depth_bands = if view.series_in_depth {
        series
    } else {
        let bar = 1.0 / (series - (series - 1.0) * overlap as f32 / 100.0 + gap_width as f32 / 100.0);
        bar * (1.0 + view.gap_depth as f32 / 100.0)
    } * view.depth_percent as f32 / 100.0;
    let deep = depth_bands / n_cat.max(1) as f32;
    let kx = deep * (view.rot_y as f32).to_radians().sin().abs();
    let ky = deep * (view.rot_x as f32).to_radians().sin().abs();
    let height_percent = view.height_percent.map_or(100.0 * inner.h / inner.w, |p| p as f32);
    let ratio = if view.series_in_depth { 0.5 } else { 1.0 };
    let kh = ratio * height_percent / 100.0;
    let front_w = (avail_w / (1.0 + kx)).min(avail_h / (kh + ky)).max(10.0);
    let (dx, dy, front_h) = (kx * front_w, ky * front_w, kh * front_w);
    let left = inner.x + (inner.w - (front_w + dx)) / 2.0;
    let top = inner.y + (inner.h - (front_h + dy)) / 2.0 + dy;
    (Area { x: left, y: top, w: front_w, h: front_h }, dx, dy)
}

pub(crate) fn chart_element(chart: &Chart, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    if frame.width <= 0 || frame.height <= 0 {
        return div();
    }
    let supported = chart.axes.iter().any(|a| a.kind == AxisKind::Series).then(|| {
        let mut copy = chart.clone();
        copy.axes.retain(|a| a.kind != AxisKind::Series);
        copy
    });
    let chart = supported.as_ref().unwrap_or(chart);
    // A treemap and a sunburst have no groups or axes of their own (the
    // invisible ones they were read with are not drawn).
    let tree_only = matches!(chart.ex, Some(ChartEx::Treemap { .. } | ChartEx::Sunburst(_))).then(|| {
        let mut copy = chart.clone();
        copy.groups.clear();
        copy.axes.clear();
        copy
    });
    let chart = tree_only.as_ref().unwrap_or(chart);
    let fitted = fit_substitutes(chart, res);
    let chart = fitted.as_ref().unwrap_or(chart);
    let crossed = crossing_adjusted(chart);
    let chart = crossed.as_ref().unwrap_or(chart);
    let (w, h) = (frame.width as f32 * scale, frame.height as f32 * scale);
    let px_per_point = scale * EMU_PER_POINT as f32;
    let plot = if tree_only.is_some() {
        tree_plot(chart, w, h, px_per_point, res)
    } else {
        chart.plot_area.map(|r| Area::of(r, w, h)).unwrap_or_else(|| automatic_plot(chart, w, h, px_per_point, res))
    };
    let plot = side_axes_layout(chart, plot, w, px_per_point);
    let plot = slanted_labels_layout(chart, plot, h, px_per_point);
    let mut plot = plot;
    if chart.data_table.is_some() {
        let count = chart.groups.iter().flat_map(|g| &g.series).count().max(1);
        let row_h = data_table_row_height(chart, px_per_point);
        let max_bottom = h - CHART_PADDING_PT * px_per_point - (1 + count) as f32 * row_h;
        if plot.bottom() > max_bottom {
            plot.h = (max_bottom - plot.y).max(20.0 * px_per_point);
        }
    }

    // A 3-D chart (right-angle axes). In a worksheet (Excel's defaults) the box's front plane is the
    // plot area less the depth, which goes up and to the right. In a presentation PowerPoint fits the
    // box into the plot area (`inner` layout) as `powerpoint_3d_box` says.
    let view3d = chart.groups.iter().find(|g| !is_round(g)).and_then(|g| g.three_d);
    // The series of a 3-D chart stand one behind the other for `standard` grouping; for `clustered` (in a presentation; a sheet's are as Excel's) side by side in a row.
    let series_behind_each_other = chart.excel_layout || view3d.is_none_or(|v| v.series_in_depth);
    let mut powerpoint_front: Option<Area> = None;
    let depth: Option<(f32, f32)> = view3d.map(|v| {
        if chart.excel_layout || !v.right_angle_axes {
            let kx = ((v.rot_y as f32).to_radians().sin().abs() / 20f32.to_radians().sin()).clamp(0.3, 3.0);
            let ky = ((v.rot_x as f32).to_radians().sin().abs() / 15f32.to_radians().sin()).clamp(0.3, 3.0);
            let base = (0.055 * plot.w).min(0.098 * plot.h) * v.depth_percent as f32 / 100.0;
            (base * kx, base * ky)
        } else {
            let group = chart.groups.iter().find(|g| !is_round(g));
            let series = group.filter(|g| g.grouping == Grouping::Standard).map_or(1, |g| g.series.len().max(1));
            let (gap_width, overlap) = match group.map(|g| g.kind) {
                Some(ChartType::Bar { gap_width, overlap, .. }) => (gap_width, overlap),
                _ => (150, 0),
            };
            let (front, dx, dy) = powerpoint_3d_box(plot, &v, category_count(chart).max(1), series, gap_width, overlap, px_per_point);
            powerpoint_front = Some(front);
            (dx, dy)
        }
    });
    let _box_plot = plot;
    if let Some(front) = powerpoint_front {
        plot = front;
    } else if let Some((dx, dy)) = depth {
        plot = Area { x: plot.x, y: plot.y + dy, w: (plot.w - dx).max(10.0), h: (plot.h - dy).max(10.0) };
    }
    let mut marks = Vec::new();
    let mut labels = Vec::new();
    let mut rotated = Vec::new();
    if let Some((dx, dy)) = depth {
        // The floor, the back wall and the left wall.
        let wall = hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x868686)));
        let line_w = 0.75 * scale;
        let (l, r, t, b) = (plot.x, plot.right(), plot.y, plot.bottom());
        let floor = vec![(l, b), (r, b), (r + dx, b - dy), (l + dx, b - dy)];
        // (The floor is Excel's light grey on a sheet; in a presentation, with no fill of its own, it is clear: PowerPoint's PDF.)
        if chart.excel_layout {
            marks.push(Mark::Fill(floor.clone(), hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xF2F2F2))).into()));
        }
        // (Walls with no fill of their own are Excel's grey on a sheet; in a presentation a wall with `a:noFill` is clear and has no outline: PowerPoint's PDF of
        // a chart whose floor and walls are `a:noFill` with no line shows only the bars.)
        let wall_fill = background(&chart.plot_background).or_else(|| chart.excel_layout.then(|| hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xC0C0C0))).into()));
        let walls_shown = wall_fill.is_some();
        if let Some(bg) = wall_fill {
            // The left wall in the same ground as the back wall.
            marks.push(Mark::Fill(vec![(l, t), (l + dx, t - dy), (l + dx, b - dy), (l, b)], bg.clone()));
            marks.push(Mark::Fill(vec![(l + dx, t - dy), (r + dx, t - dy), (r + dx, b - dy), (l + dx, b - dy)], bg));
        }
        if walls_shown || chart.excel_layout {
            let mut floor_outline = floor;
            floor_outline.push((l, b));
            marks.push(Mark::Stroke(floor_outline, line_w, wall));
            marks.push(Mark::Stroke(vec![(l, b), (l, t), (l + dx, t - dy), (l + dx, b - dy)], line_w, wall));
            marks.push(Mark::Stroke(vec![(l + dx, t - dy), (r + dx, t - dy)], line_w, wall));
        }
    } else if let Some(bg) = background(&chart.plot_background) {
        marks.push(Mark::Fill(vec![(plot.x, plot.y), (plot.right(), plot.y), (plot.right(), plot.bottom()), (plot.x, plot.bottom())], bg));
    }

    let horizontal = chart.groups.iter().any(|g| matches!(g.kind, ChartType::Bar { horizontal: true, .. }));
    let categories = category_count(chart).max(1);
    let category_names: Vec<String> =
        chart.groups.iter().flat_map(|g| &g.series).map(|s| s.categories.clone()).find(|c| !c.is_empty()).unwrap_or_default();
    let category_axis = chart.axes.iter().find(|a| a.kind == AxisKind::Category && !a.deleted);
    let reversed_categories = chart.axes.iter().find(|a| a.kind == AxisKind::Category).is_some_and(|a| a.reversed);

    // A date axis places its categories by their dates, a slot of its base
    // unit each, from its minimum to its maximum.
    let time = category_axis.filter(|_| !horizontal).and_then(|a| {
        let t = a.time.as_ref()?;
        let units: Vec<Option<f64>> = t.dates.iter().map(|d| d.map(|d| time_index(d.0, t.base))).collect();
        let known = units.iter().flatten().copied();
        let lo = a.min.map(|m| time_index(m.0, t.base)).or_else(|| known.clone().reduce(f64::min))?;
        let hi = a.max.map(|m| time_index(m.0, t.base)).or_else(|| known.reduce(f64::max))?.max(lo);
        Some((t, units, lo, hi))
    });
    // Where category `i` sits along the category direction: its band's start and width.
    let band_len = match &time {
        Some((_, _, lo, hi)) => plot.w / (hi - lo + 1.0) as f32,
        None => (if horizontal { plot.h } else { plot.w }) / categories as f32,
    };
    let time_x = |unit: f64| time.as_ref().map_or(0.0, |(_, _, lo, _)| plot.x + ((unit - lo) as f32) * band_len);
    let band = |i: usize| {
        if let Some((_, units, _, _)) = &time {
            // Off the axis when the category has no date.
            let u = units.get(i).copied().flatten().unwrap_or(f64::NAN);
            return (time_x(u), band_len);
        }
        let i = if reversed_categories != horizontal { categories - 1 - i } else { i };
        (if horizontal { plot.y } else { plot.x } + band_len * i as f32, band_len)
    };

    // Value scales per value axis.
    let mut scales: Vec<(u32, Scale)> = Vec::new();
    for group in chart.groups.iter().filter(|g| !is_round(g)) {
        let axis_id = value_axis(chart, group).map(|a| a.id).unwrap_or(0);
        if scales.iter().any(|(id, _)| *id == axis_id) {
            continue;
        }
        let (low, high) = value_range(chart, axis_id);
        let intervals = if horizontal {
            bar_tick_intervals(chart.axis(axis_id), low, high, plot.w, px_per_point)
        } else {
            tick_intervals(chart.axis(axis_id), low, high, plot.h, false, px_per_point, chart.excel_layout, chart.word_layout)
        };
        let mut scale = Scale::fit(low, high, chart.axis(axis_id), intervals);
        // 100% stacked goes to 100% exactly (no room above, as PowerPoint and Excel).
        if percent_axis(chart, axis_id) && chart.axis(axis_id).is_none_or(|a| a.max.is_none()) && high <= 1.0 {
            scale.max = 1.0;
        }
        scales.push((axis_id, scale));
    }
    let scale_for = |group: &ChartGroup| {
        let id = value_axis(chart, group).map(|a| a.id).unwrap_or(0);
        scales.iter().find(|(i, _)| *i == id).map(|(_, s)| *s).unwrap_or(Scale { min: 0.0, max: 1.0, step: 0.2, reversed: false, log_base: None })
    };
    // Pixel position of a value along the value direction.
    let value_px = |s: &Scale, v: f64| {
        let t = s.at(v).clamp(-0.5, 1.5);
        if horizontal { plot.x + t * plot.w } else { plot.bottom() - t * plot.h }
    };

    // Gridlines and value axis labels, behind the marks: the minor ones
    // first, between the major ones.
    if !chart.groups.iter().any(is_radar) {
        for (id, s) in &scales {
        let Some(axis) = chart.axis(*id) else { continue };
        if let Some(grid) = &axis.minor_gridlines {
            let minor = axis.minor_unit.map_or(s.step / 5.0, |u| u.0);
            let count = ((s.max - s.min) / minor + 1e-6).floor().clamp(0.0, 500.0) as usize;
            let width = grid.width as f32 * scale;
            for i in 0..=count {
                let v = s.min + minor * i as f64;
                let on_major = ((v - s.min) / s.step - ((v - s.min) / s.step).round()).abs() < 1e-6;
                if on_major && axis.gridlines.is_some() {
                    continue;
                }
                let p = value_px(s, v);
                let line = if horizontal { vec![(p, plot.y), (p, plot.bottom())] } else { vec![(plot.x, p), (plot.right(), p)] };
                push_stroke(&mut marks, line, width, grid);
            }
        }
    }
    for (id, s) in &scales {
        let Some(axis) = chart.axis(*id) else { continue };
        for tick in s.ticks() {
            let p = value_px(s, tick);
            if let Some(grid) = &axis.gridlines {
                let width = grid.width as f32 * scale;
                if let Some((dx, dy)) = depth {
                    // On the back wall, and across the side wall (or the floor of a horizontal bar chart) to it.
                    let (back, side) = if horizontal {
                        (vec![(p + dx, plot.y - dy), (p + dx, plot.bottom() - dy)], vec![(p, plot.bottom()), (p + dx, plot.bottom() - dy)])
                    } else {
                        (vec![(plot.x + dx, p - dy), (plot.right() + dx, p - dy)], vec![(plot.x, p), (plot.x + dx, p - dy)])
                    };
                    push_stroke(&mut marks, back, width, grid);
                    push_stroke(&mut marks, side, width, grid);
                } else {
                    let line = if horizontal { vec![(p, plot.y), (p, plot.bottom())] } else { vec![(plot.x, p), (plot.right(), p)] };
                    push_stroke(&mut marks, line, width, grid);
                }
            }
            if !axis.deleted && axis.show_labels {
                let unit = axis.display_unit.map(|u| u.0).unwrap_or(1.0);
                // A 100% stacked chart's General axis shows percentages.
                let format = if axis.format == "General" && percent_axis(chart, *id) { "0%" } else { axis.format.as_str() };
                let text = format_number(tick / unit, format);
                // Tick marks reaching out of the axis push the labels away
                // by their length (measured against PowerPoint's PDF output).
                let gap = (4.0 + if axis.ticks_outside { 3.75 } else { 0.0 }) * px_per_point;
                // PowerPoint leaves out tick labels that do not fit in the frame.
                let width = number_width(&text) * axis.text.size as f32 / 100.0 * px_per_point;
                let side = axis_label_side(axis, horizontal);
                let fits = match (horizontal, side) {
                    (true, _) => true,
                    (false, Side::Right) => plot.right() + gap + width <= w,
                    (false, _) => plot.x - gap - width >= 0.0,
                };
                if !fits {
                    continue;
                }
                labels.push(match (horizontal, side) {
                    (true, Side::Top) => Label::plain(p, plot.y - gap - text_height(&axis.text, px_per_point) / 2.0, text, axis.text.clone(), Anchor::Center),
                    (true, _) => Label::plain(p, plot.bottom() + gap + text_height(&axis.text, px_per_point) / 2.0, text, axis.text.clone(), Anchor::Center),
                    (false, Side::Right) => Label::plain(plot.right() + gap, p, text, axis.text.clone(), Anchor::Start),
                    (false, _) => Label::plain(plot.x - gap, p, text, axis.text.clone(), Anchor::End),
                });
            }
        }
    }

    // Category axis gridlines, behind the marks.
    if let Some(axis) = category_axis {
        if let Some(grid) = &axis.gridlines {
            let width = grid.width as f32 * scale;
            for i in 0..categories {
                let (b_start, _) = band(i);
                let p = b_start;
                let line = if horizontal { vec![(plot.x, p), (plot.right(), p)] } else { vec![(p, plot.y), (p, plot.bottom())] };
                push_stroke(&mut marks, line, width, grid);
            }
        }
    }
    }

    // A scatter's x axis: a value scale across the plot, its gridlines and
    // labels as an upright value axis's but level.
    let mut x_scales: Vec<(u32, Scale)> = Vec::new();
    for group in chart.groups.iter().filter(|g| is_scatter(g)) {
        let id = x_axis(chart, group).map(|a| a.id).unwrap_or(u32::MAX);
        if x_scales.iter().any(|(i, _)| *i == id) {
            continue;
        }
        let xs = chart.groups.iter().filter(|g| is_scatter(g)).flat_map(|g| &g.series).flat_map(|s| (0..s.values.len()).map(move |i| x_value(s, i)));
        let (low, high) = xs.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
        let (low, high) = if low > high { (0.0, 1.0) } else { (low, high) };
        let axis = chart.axis(id);
        let intervals = tick_intervals(axis, low, high, plot.w, true, px_per_point, false, false);
        x_scales.push((id, Scale::fit(low, high, axis, intervals)));
    }
    for (id, s) in &x_scales {
        let Some(axis) = chart.axis(*id) else { continue };
        for tick in s.ticks() {
            let p = plot.x + s.at(tick) * plot.w;
            if let Some(grid) = &axis.gridlines {
                push_stroke(&mut marks, vec![(p, plot.y), (p, plot.bottom())], grid.width as f32 * scale, grid);
            }
            if !axis.deleted && axis.show_labels {
                let unit = axis.display_unit.map(|u| u.0).unwrap_or(1.0);
                let text = format_number(tick / unit, &axis.format);
                let gap = (4.0 + if axis.ticks_outside { 3.75 } else { 0.0 }) * px_per_point;
                let y = if axis.side == Side::Top { plot.y - gap - text_height(&axis.text, px_per_point) / 2.0 } else { plot.bottom() + gap + text_height(&axis.text, px_per_point) / 2.0 };
                labels.push(Label::plain(p, y, text, axis.text.clone(), Anchor::Center));
            }
        }
    }
    let x_scale_for = |group: &ChartGroup| {
        let id = x_axis(chart, group).map(|a| a.id).unwrap_or(u32::MAX);
        x_scales.iter().find(|(i, _)| *i == id).map(|(_, s)| *s).unwrap_or(Scale { min: 0.0, max: 1.0, step: 0.2, reversed: false, log_base: None })
    };

    let cut = !chart.groups.iter().any(is_round) && !chart.groups.iter().any(is_radar) && depth.is_none();
    if cut {
        marks.push(Mark::Clip(Some((plot.x, plot.y, plot.w, plot.h))));
    }
    for group in &chart.groups {
        match group.kind {
            ChartType::Radar { style } => {
                let s = scale_for(group);
                let (cx, cy) = (plot.x + plot.w / 2.0, plot.y + plot.h / 2.0);
                let pad = 24.0 * px_per_point;
                let max_r = (plot.w.min(plot.h) / 2.0 - pad).max(10.0 * px_per_point);
                let n_cats = categories.max(3);
                let angle = |i: usize| -> f32 {
                    -std::f32::consts::FRAC_PI_2 + 2.0 * std::f32::consts::PI * (i as f32) / (n_cats as f32)
                };
                let span = (s.max - s.min).max(1e-9);
                let val_to_r = |v: f64| -> f32 {
                    let ratio = ((v - s.min) / span).clamp(0.0, 1.0) as f32;
                    ratio * max_r
                };

                // Radial spokes from center to each category vertex.
                for i in 0..n_cats {
                    let a = angle(i);
                    let pt = (cx + max_r * a.cos(), cy + max_r * a.sin());
                    marks.push(Mark::Stroke(vec![(cx, cy), pt], 1.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xD0D0D0)))));
                }

                // Concentric web / polygon gridlines for ticks.
                let val_axis = value_axis(chart, group);
                for tick in s.ticks() {
                    let r = val_to_r(tick);
                    if r > 1.0 {
                        let mut ring: Vec<(f32, f32)> = (0..n_cats)
                            .map(|i| {
                                let a = angle(i);
                                (cx + r * a.cos(), cy + r * a.sin())
                            })
                            .collect();
                        if let Some(first) = ring.first().copied() {
                            ring.push(first);
                        }
                        marks.push(Mark::Stroke(ring, 1.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xD9D9D9)))));

                        if val_axis.is_none_or(|a| !a.deleted && a.show_labels) {
                            let val_axis_fmt = val_axis.map(|a| a.format.as_str()).unwrap_or("General");
                            let val_axis_text = val_axis.map(|a| a.text.clone()).unwrap_or(ChartText { size: 800, ..Default::default() });
                            let text = format_number(tick, val_axis_fmt);
                            labels.push(Label::plain(cx + 3.0 * px_per_point, cy - r, text, val_axis_text, Anchor::Start));
                        }
                    }
                }

                // Category labels around the perimeter.
                for (ci, name) in category_names.iter().enumerate().take(n_cats) {
                    let a = angle(ci);
                    let r_cat = max_r + 8.0 * px_per_point;
                    let lx = cx + r_cat * a.cos();
                    let ly = cy + r_cat * a.sin();
                    let anchor = if a.cos().abs() < 0.25 {
                        Anchor::Center
                    } else if a.cos() > 0.0 {
                        Anchor::Start
                    } else {
                        Anchor::End
                    };
                    let cat_text = category_axis.map(|ax| ax.text.clone()).unwrap_or_default();
                    labels.push(Label::plain(lx, ly, name.clone(), cat_text, anchor));
                }

                // Series polygons, fills, strokes, and markers.
                for series in &group.series {
                    let points: Vec<(f32, f32)> = (0..n_cats)
                        .map(|ci| {
                            let raw_val = series.values.get(ci).and_then(|v| *v).map(|n| n.0);
                            let val = match raw_val {
                                Some(v) => v,
                                None if chart.disp_blanks_as == Some(DispBlanksAs::Zero) => 0.0,
                                None => s.min,
                            };
                            let r = val_to_r(val);
                            let a = angle(ci);
                            (cx + r * a.cos(), cy + r * a.sin())
                        })
                        .collect();

                    if points.is_empty() {
                        continue;
                    }

                    if style == Some(ibis_doc_model::RadarStyle::Filled) {
                        let fill_c = series_color(series).unwrap_or_else(|| hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472C4))));
                        let bg: Background = background(&series.fill).unwrap_or_else(|| {
                            let mut c = fill_c;
                            c.a = 0.35;
                            c.into()
                        });
                        marks.push(Mark::Fill(points.clone(), bg));
                    }

                    let stroke_w = series.stroke.as_ref().map_or(2.0 * scale, |st| (st.width as f32 * scale).max(1.0));
                    let stroke_c = series.stroke.as_ref().map_or_else(
                        || series_color(series).unwrap_or_else(|| hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472C4)))),
                        |st| hsla(st.color),
                    );
                    let mut outline = points.clone();
                    if let Some(first) = points.first() {
                        outline.push(*first);
                    }
                    if let Some(stroke) = &series.stroke {
                        push_stroke(&mut marks, outline, stroke_w, stroke);
                    } else {
                        marks.push(Mark::Stroke(outline, stroke_w, stroke_c));
                    }

                    let has_markers = style == Some(ibis_doc_model::RadarStyle::Marker)
                        || series.marker.as_ref().is_some_and(|m| m.symbol != MarkerSymbol::None);
                    if has_markers {
                        let symbol = series.marker.as_ref().map_or(MarkerSymbol::Circle, |m| {
                            if m.symbol == MarkerSymbol::None {
                                MarkerSymbol::Circle
                            } else {
                                m.symbol
                            }
                        });
                        let marker_r = series.marker.as_ref().map_or(3.0 * px_per_point, |m| (m.size as f32 * px_per_point / 2.0).max(2.0));
                        let marker_bg: Background = series.marker.as_ref().and_then(|m| background(&m.fill)).unwrap_or_else(|| stroke_c.into());
                        for &(px, py) in &points {
                            marks.push(Mark::Fill(marker_shape(symbol, px, py, marker_r), marker_bg.clone()));
                        }
                    }

                    if series.labels.is_some() {
                        for (ci, &(px, py)) in points.iter().enumerate() {
                            if let Some(val) = series.values.get(ci).and_then(|v| *v).map(|n| n.0) {
                                let text = format_number(val, "General");
                                labels.push(Label::plain(
                                    px,
                                    py - 6.0 * px_per_point,
                                    text,
                                    ChartText { size: 800, ..Default::default() },
                                    Anchor::Center,
                                ));
                            }
                        }
                    }
                }
            }
            ChartType::Bar { gap_width, overlap, .. } => {
                let s = scale_for(group);
                let k = group.series.len().max(1) as f32;
                let stacked = group.grouping != Grouping::Standard;
                let is_3d_series_depth = depth.is_some() && group.grouping == Grouping::Standard && series_behind_each_other;
                let (overlap, gap) = (if stacked { 1.0 } else { overlap as f32 / 100.0 }, gap_width as f32 / 100.0);
                let lanes = if stacked || is_3d_series_depth { 1.0 } else { k - (k - 1.0) * overlap };
                let bar = band_len / (lanes + gap);
                let base_value = base_value_of(chart, group, &s);
                let totals = category_totals(group, categories);
                let mut stack_pos = vec![0f64; categories];
                let mut stack_neg = vec![0f64; categories];
                let mut bar_edges: Vec<Vec<Option<(f32, f32, f32)>>> = vec![vec![None; categories]; group.series.len()];
                let series_order: Vec<usize> = if is_3d_series_depth {
                    (0..group.series.len()).rev().collect()
                } else {
                    (0..group.series.len()).collect()
                };
                for si in series_order {
                    let series = &group.series[si];
                    for (ci, value) in series.values.iter().enumerate() {
                        let effective_value = match value.map(|n| n.0) {
                            Some(v) => Some(v),
                            None if chart.disp_blanks_as == Some(DispBlanksAs::Zero) => Some(0.0),
                            None => None,
                        };
                        let Some(value) = effective_value else { continue };
                        let plotted = if group.grouping == Grouping::PercentStacked && totals[ci] != 0.0 { value / totals[ci] } else { value };
                        let (start, end) = if stacked {
                            let stack = if plotted >= 0.0 { &mut stack_pos[ci] } else { &mut stack_neg[ci] };
                            let start = *stack;
                            *stack += plotted;
                            (start, *stack)
                        } else {
                            (base_value, plotted)
                        };
                        let (band_start, _) = band(ci);
                        // Horizontal bars put the first series at the bottom of
                        // their band (nearest the axis's start), as PowerPoint.
                        let order = if horizontal != reversed_categories { group.series.len() - 1 - si } else { si };
                        let lane = if stacked || is_3d_series_depth { 0.0 } else { order as f32 * (1.0 - overlap) };
                        let a = band_start + bar * gap / 2.0 + lane * bar;
                        let b = a + bar;
                        let (v0, v1) = (value_px(&s, start), value_px(&s, end));
                        if ci < categories {
                            bar_edges[si][ci] = Some((a, b, v1));
                        }
                        let corners = if horizontal {
                            vec![(v0, a), (v1, a), (v1, b), (v0, b)]
                        } else {
                            vec![(a, v0), (b, v0), (b, v1), (a, v1)]
                        };
                        // A 3-D bar stands a little back from the front plane, with sides.
                        let (shift_dx, shift_dy, ext_dx, ext_dy) = match depth {
                            Some((dx, dy)) => {
                                if is_3d_series_depth {
                                    let shift_f = (si as f32 + 0.15) / k;
                                    let ext_f = 0.55 / k;
                                    (dx * shift_f, dy * shift_f, dx * ext_f, dy * ext_f)
                                } else {
                                    (dx * 0.25, dy * 0.25, dx * 0.45, dy * 0.45)
                                }
                            }
                            None => (0.0, 0.0, 0.0, 0.0),
                        };
                        let corners: Vec<(f32, f32)> = if depth.is_some() {
                            corners.iter().map(|&(x, y)| (x + shift_dx, y - shift_dy)).collect()
                        } else {
                            corners
                        };
                        if depth.is_some() {
                            if let Some(color) = background_color(point_fill(series, ci)) {
                                side_faces(&mut marks, &corners, (ext_dx, ext_dy), color);
                            }
                        }
                        // Chart style 10 draws the bars with a soft shadow and a thin white outline (measured on Excel's PDF of GitHub #22; the other styles are not checked).
                        let styled = depth.is_none() && chart.style == Some(10);
                        if styled && background(point_fill(series, ci)).is_some() {
                            soft_shadow(&mut marks, &corners, scale);
                        }
                        if let Some(bg) = background(point_fill(series, ci)) {
                            marks.push(Mark::Fill(corners.clone(), bg));
                        }
                        if let Some(stroke) = point_stroke(series, ci) {
                            let mut outline = corners.clone();
                            outline.push(corners[0]);
                            push_stroke(&mut marks, outline, stroke.width as f32 * scale, stroke);
                        } else if styled && series.stroke.is_none() && background(point_fill(series, ci)).is_some() {
                            let mut outline = corners.clone();
                            outline.push(corners[0]);
                            marks.push(Mark::Stroke(outline, 9_525.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffffff)))));
                        }
                        let default = if stacked { LabelPosition::Center } else { LabelPosition::OutsideEnd };
                        let (mid, end_px, start_px) = ((a + b) / 2.0, v1, v0);
                        let grows = if horizontal { end_px >= start_px } else { end_px <= start_px };
                        let place = move |pos: LabelPosition, text_h: f32, text_w_hint: f32| -> (f32, f32, Anchor) {
                            let dir = if grows { 1.0 } else { -1.0 };
                            let along = |d: f32| if horizontal { (end_px + d * dir, mid) } else { (mid, end_px - d * dir) };
                            let pad = 2.0;
                            let (x, y) = match pos {
                                LabelPosition::InsideEnd => along(-(if horizontal { text_w_hint } else { text_h }) / 2.0 - pad),
                                LabelPosition::InsideBase => {
                                    let d = (if horizontal { text_w_hint } else { text_h }) / 2.0 + pad;
                                    if horizontal { (start_px + d * dir, mid) } else { (mid, start_px - d * dir) }
                                }
                                LabelPosition::Center => {
                                    if horizontal { ((start_px + end_px) / 2.0, mid) } else { (mid, (start_px + end_px) / 2.0) }
                                }
                                // Beyond the bar's bottom edge / top edge (a waterfall's decreases stand under their bars).
                                LabelPosition::Below if !horizontal => (mid, start_px.max(end_px) + text_h / 2.0 + pad),
                                LabelPosition::Above if !horizontal => (mid, start_px.min(end_px) - text_h / 2.0 - pad),
                                _ => along((if horizontal { text_w_hint } else { text_h }) / 2.0 + pad),
                            };
                            (x + shift_dx, y - shift_dy, Anchor::Center)
                        };
                        let flip = if grows { 1.0 } else { -1.0 };
                        let moves = if horizontal { (w * flip, h) } else { (w, h * flip) };
                        push_label(&mut labels, series, ci, value, totals[ci], default, (moves.0, moves.1, plot), px_per_point, res, place);
                    }
                    if let Some(tl) = &series.trendline {
                        let raw_points: Vec<(f64, f64, f32, f32)> = series.values.iter().enumerate().filter_map(|(ci, v)| {
                            let val = v.as_ref()?.0;
                            let (band_start, _) = band(ci);
                            let order = if horizontal != reversed_categories { group.series.len() - 1 - si } else { si };
                            let lane = if stacked || is_3d_series_depth { 0.0 } else { order as f32 * (1.0 - overlap) };
                            let mid = band_start + bar * gap / 2.0 + lane * bar + bar / 2.0;
                            let v_px = value_px(&s, val);
                            let (x_val, y_val) = ((ci + 1) as f64, val);
                            let (sx, sy) = if horizontal { (v_px, mid) } else { (mid, v_px) };
                            Some((x_val, y_val, sx, sy))
                        }).collect();
                        draw_trendline(&mut marks, &mut labels, tl, series, &raw_points, plot, scale);
                    }
                    if let Some(eb) = &series.error_bars {
                        let pts: Vec<(usize, f64, f32, f32)> = series.values.iter().enumerate().filter_map(|(ci, v)| {
                            let val = v.as_ref()?.0;
                            let (band_start, _) = band(ci);
                            let order = if horizontal != reversed_categories { group.series.len() - 1 - si } else { si };
                            let lane = if stacked || is_3d_series_depth { 0.0 } else { order as f32 * (1.0 - overlap) };
                            let mid = band_start + bar * gap / 2.0 + lane * bar + bar / 2.0;
                            let v_px = value_px(&s, val);
                            let (sx, sy) = if horizontal { (v_px, mid) } else { (mid, v_px) };
                            Some((ci, val, sx, sy))
                        }).collect();
                        draw_error_bars(&mut marks, eb, series, &pts, plot, scale, &s, horizontal);
                    }
                }
                if stacked
                    && depth.is_none()
                    && let Some(connector) = group.of_pie.as_ref().and_then(|op| op.connector.as_ref())
                {
                    let ser_stroke = connector
                        .clone()
                        .unwrap_or_else(|| ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x595959)), 9_525));
                    let stroke_w = ser_stroke.width as f32 * scale;
                    for si in 0..group.series.len() {
                        for ci in 0..categories.saturating_sub(1) {
                            if let (Some((a0, b0, v0_end)), Some((a1, b1, v1_end))) = (bar_edges[si][ci], bar_edges[si][ci + 1]) {
                                let (p0, p1) = if b0 <= a1 { (b0, a1) } else { (a0, b1) };
                                let seg = if horizontal {
                                    vec![(v0_end, p0), (v1_end, p1)]
                                } else {
                                    vec![(p0, v0_end), (p1, v1_end)]
                                };
                                push_stroke(&mut marks, seg, stroke_w, &ser_stroke);
                            }
                        }
                    }
                }
            }
            ChartType::Line | ChartType::Area => {
                let s = scale_for(group);
                let base = value_px(&s, 0f64.clamp(s.min, s.max));
                let base0 = base;
                let totals = category_totals(group, categories);
                let mut stack = vec![0f64; categories];
                // A 3-D chart of series side by side (not stacked) puts each series in a row of its own
                // along the depth, the first at the front, drawn from the back; the others share one row.
                let rows = if depth.is_some() && group.grouping == Grouping::Standard && series_behind_each_other { group.series.len().max(1) } else { 1 };
                let mut order: Vec<(usize, &Series)> = group.series.iter().enumerate().collect();
                if rows > 1 {
                    order.reverse();
                }
                for (si, series) in order {
                    let (shift_f, ext_f) = if rows > 1 {
                        ((si as f32 + 0.15) / rows as f32, 0.55 / rows as f32)
                    } else if group.kind == ChartType::Area {
                        (0.2, 0.5)
                    } else {
                        (0.3, 0.2)
                    };
                    // A 3-D area or line stands back from the front plane: the base moves with it.
                    let base = match (depth, horizontal) {
                        (Some((dx, _)), true) => base0 + dx * shift_f,
                        (Some((_, dy)), false) => base0 - dy * shift_f,
                        _ => base0,
                    };
                    // A stacked area is the band between the series below
                    // and this one; a missing value adds nothing.
                    if group.kind == ChartType::Area && group.grouping != Grouping::Standard {
                        let at = |ci: usize, v: f64| {
                            let (start, len) = band(ci);
                            // Clipped to the plot area, as PowerPoint draws it.
                            let (lo, hi) = (s.min.min(s.max), s.max.max(s.min));
                            let (along, p) = (start + len / 2.0, value_px(&s, v.clamp(lo, hi)));
                            let (x, y) = if horizontal { (p, along) } else { (along, p) };
                            match depth {
                                Some((dx, dy)) => (x + dx * shift_f, y - dy * shift_f),
                                None => (x, y),
                            }
                        };
                        let lower: Vec<(f32, f32)> = (0..categories).map(|ci| at(ci, stack[ci])).collect();
                        let upper: Vec<(f32, f32)> = (0..categories)
                            .map(|ci| {
                                let value = series.values.get(ci).copied().flatten().map(|n| n.0).unwrap_or(0.0);
                                let v = if group.grouping == Grouping::PercentStacked && totals[ci] != 0.0 { value / totals[ci] } else { value };
                                at(ci, stack[ci] + v)
                            })
                            .collect();
                        if let Some(bg) = background(&series.fill) {
                            let outline: Vec<(f32, f32)> = upper.iter().copied().chain(lower.iter().rev().copied()).collect();
                            if let (Some((dx, dy)), Some(color)) = (depth, background_color(&series.fill)) {
                                side_faces(&mut marks, &outline, (dx * ext_f, dy * ext_f), color);
                            }
                            marks.push(Mark::Fill(outline, bg));
                        }
                    }
                    let mut points: Vec<(usize, f32, f32, f64)> = Vec::new();
                    for (ci, value) in series.values.iter().enumerate() {
                        let effective_value = match value.map(|n| n.0) {
                            Some(v) => Some(v),
                            None if chart.disp_blanks_as == Some(DispBlanksAs::Zero) => Some(0.0),
                            None => None,
                        };
                        let Some(value) = effective_value else { continue };
                        let mut v = if group.grouping == Grouping::PercentStacked && totals[ci] != 0.0 { value / totals[ci] } else { value };
                        if group.grouping != Grouping::Standard {
                            stack[ci] += v;
                            v = stack[ci];
                        }
                        let (start, len) = band(ci);
                        let along = start + len / 2.0;
                        let p = value_px(&s, v);
                        let (x, y) = if horizontal { (p, along) } else { (along, p) };
                        let (x, y) = match depth {
                            Some((dx, dy)) => {
                                (x + dx * shift_f, y - dy * shift_f)
                            }
                            None => (x, y),
                        };
                        points.push((ci, x, y, value));
                    }
                    if points.is_empty() {
                        continue;
                    }
                    if group.kind == ChartType::Area && group.grouping == Grouping::Standard {
                        let mut outline: Vec<(f32, f32)> = points.iter().map(|p| (p.1, p.2)).collect();
                        let (first, last) = (points[0], points[points.len() - 1]);
                        if horizontal {
                            outline.push((base, last.2));
                            outline.push((base, first.2));
                        } else {
                            outline.push((last.1, base));
                            outline.push((first.1, base));
                        }
                        if let (Some((dx, dy)), Some(color)) = (depth, background_color(&series.fill)) {
                            side_faces(&mut marks, &outline, (dx * ext_f, dy * ext_f), color);
                        }
                        if let Some(bg) = background(&series.fill) {
                            marks.push(Mark::Fill(outline, bg));
                        }
                    }
                    if group.kind == ChartType::Line && !series.point_strokes.is_empty() && series.stroke.is_some() {
                        // Colours by point: each segment in the colour of the
                        // point it leads to.
                        for pair in points.windows(2) {
                            if pair[1].0 != pair[0].0 + 1 && chart.disp_blanks_as != Some(DispBlanksAs::Span) {
                                continue;
                            }
                            if let Some(stroke) = point_stroke(series, pair[1].0) {
                                if let Some((dx, dy)) = depth {
                                    ribbon_faces(&mut marks, &[(pair[0].1, pair[0].2), (pair[1].1, pair[1].2)], (dx * ext_f, dy * ext_f), hsla(stroke.color));
                                }
                                push_stroke(&mut marks, vec![(pair[0].1, pair[0].2), (pair[1].1, pair[1].2)], stroke.width as f32 * scale, stroke);
                            }
                        }
                    } else if let Some(stroke) = &series.stroke {
                        let stroke_w = stroke.width as f32 * scale;
                        let stroke_c = hsla(stroke.color);
                        let ribbon_ext = depth.map(|(dx, dy)| (dx * ext_f, dy * ext_f));
                        if chart.disp_blanks_as == Some(DispBlanksAs::Span) {
                            let line: Vec<(f32, f32)> = points.iter().map(|p| (p.1, p.2)).collect();
                            if let Some(ext) = ribbon_ext {
                                ribbon_faces(&mut marks, &line, ext, stroke_c);
                            }
                            push_stroke(&mut marks, line, stroke_w, stroke);
                        } else {
                            let mut segment: Vec<(f32, f32)> = Vec::new();
                            let mut last_ci: Option<usize> = None;
                            for &(ci, x, y, _) in &points {
                                if let Some(last) = last_ci {
                                    if ci != last + 1 {
                                        if segment.len() >= 2 {
                                            if let Some(ext) = ribbon_ext {
                                                ribbon_faces(&mut marks, &segment, ext, stroke_c);
                                            }
                                            push_stroke(&mut marks, segment.clone(), stroke_w, stroke);
                                        }
                                        segment.clear();
                                    }
                                }
                                segment.push((x, y));
                                last_ci = Some(ci);
                            }
                            if segment.len() >= 2 {
                                if let Some(ext) = ribbon_ext {
                                    ribbon_faces(&mut marks, &segment, ext, stroke_c);
                                }
                                push_stroke(&mut marks, segment, stroke_w, stroke);
                            }
                        }
                    }
                    let marker = series.marker.as_ref().filter(|m| m.symbol != MarkerSymbol::None && group.kind == ChartType::Line);
                    let marker_r = marker.map(|m| m.size as f32 * px_per_point / 2.0).unwrap_or(0.0);
                    for &(ci, x, y, value) in &points {
                        if let Some(m) = marker {
                            let fill = marker_fill(series, ci, m);
                            if let Some(color) = background_color(fill) {
                                marks.extend(marker_marks(m, si, x, y, marker_r, color, background(fill), px_per_point));
                            }
                        }
                        let place = move |pos: LabelPosition, text_h: f32, _: f32| -> (f32, f32, Anchor) {
                            let gap = marker_r + 3.0;
                            match pos {
                                LabelPosition::Above => (x, y - gap - text_h / 2.0, Anchor::Center),
                                LabelPosition::Below => (x, y + gap + text_h / 2.0, Anchor::Center),
                                LabelPosition::Left => (x - gap, y, Anchor::End),
                                LabelPosition::Center => (x, y, Anchor::Center),
                                _ => (x + gap, y, Anchor::Start),
                            }
                        };
                        push_label(&mut labels, series, ci, value, totals[ci], LabelPosition::Right, (w, h, plot), px_per_point, res, place);
                    }
                    if let Some(tl) = &series.trendline {
                        let raw_points: Vec<(f64, f64, f32, f32)> = points.iter().map(|&(ci, x, y, val)| {
                            ((ci + 1) as f64, val, x, y)
                        }).collect();
                        draw_trendline(&mut marks, &mut labels, tl, series, &raw_points, plot, scale);
                    }
                    if let Some(eb) = &series.error_bars {
                        let pts: Vec<(usize, f64, f32, f32)> = points.iter().map(|&(ci, x, y, val)| {
                            (ci, val, x, y)
                        }).collect();
                        draw_error_bars(&mut marks, eb, series, &pts, plot, scale, &s, false);
                    }
                }
            }
            ChartType::Scatter { smooth } => {
                let (sx, sy) = (x_scale_for(group), scale_for(group));
                for (si, series) in group.series.iter().enumerate() {
                    let points: Vec<(usize, f32, f32, f64)> = (0..series.values.len())
                        .filter_map(|i| {
                            let v = match series.values.get(i).copied().flatten().map(|n| n.0) {
                                Some(v) => Some(v),
                                None if chart.disp_blanks_as == Some(DispBlanksAs::Zero) => Some(0.0),
                                None => None,
                            }?;
                            // (A scatter's y is always up the plot, also beside horizontal bars.)
                            Some((i, plot.x + sx.at(x_value(series, i)) * plot.w, plot.bottom() - sy.at(v).clamp(-0.5, 1.5) * plot.h, v))
                        })
                        .collect();
                    if let Some(stroke) = &series.stroke
                        && points.len() > 1
                    {
                        let stroke_w = stroke.width as f32 * scale;
                        if chart.disp_blanks_as == Some(DispBlanksAs::Span) {
                            let line: Vec<(f32, f32)> = points.iter().map(|p| (p.1, p.2)).collect();
                            let line = if smooth { smooth_curve(&line) } else { line };
                            push_stroke(&mut marks, line, stroke_w, stroke);
                        } else {
                            let mut segment: Vec<(f32, f32)> = Vec::new();
                            let mut last_i: Option<usize> = None;
                            for &(i, x, y, _) in &points {
                                if let Some(last) = last_i {
                                    if i != last + 1 {
                                        if segment.len() >= 2 {
                                            let line = if smooth { smooth_curve(&segment) } else { segment.clone() };
                                            push_stroke(&mut marks, line, stroke_w, stroke);
                                        }
                                        segment.clear();
                                    }
                                }
                                segment.push((x, y));
                                last_i = Some(i);
                            }
                            if segment.len() >= 2 {
                                let line = if smooth { smooth_curve(&segment) } else { segment };
                                push_stroke(&mut marks, line, stroke_w, stroke);
                            }
                        }
                    }
                    let marker = series.marker.as_ref().filter(|m| m.symbol != MarkerSymbol::None);
                    let marker_r = marker.map(|m| m.size as f32 * px_per_point / 2.0).unwrap_or(0.0);
                    for &(i, x, y, value) in &points {
                        if let Some(m) = marker {
                            let fill = marker_fill(series, i, m);
                            if let Some(color) = background_color(fill) {
                                marks.extend(marker_marks(m, si, x, y, marker_r, color, background(fill), px_per_point));
                            }
                        }
                        let place = move |pos: LabelPosition, text_h: f32, _: f32| -> (f32, f32, Anchor) {
                            let gap = marker_r + 3.0;
                            match pos {
                                LabelPosition::Above => (x, y - gap - text_h / 2.0, Anchor::Center),
                                LabelPosition::Below => (x, y + gap + text_h / 2.0, Anchor::Center),
                                LabelPosition::Left => (x - gap, y, Anchor::End),
                                LabelPosition::Center => (x, y, Anchor::Center),
                                _ => (x + gap, y, Anchor::Start),
                            }
                        };
                        push_label(&mut labels, series, i, value, value, LabelPosition::Right, (w, h, plot), px_per_point, res, place);
                    }
                    if let Some(tl) = &series.trendline {
                        let raw_points: Vec<(f64, f64, f32, f32)> = points.iter().map(|&(i, x, y, val)| {
                            (x_value(series, i), val, x, y)
                        }).collect();
                        draw_trendline(&mut marks, &mut labels, tl, series, &raw_points, plot, scale);
                    }
                    if let Some(eb) = &series.error_bars {
                        let is_x = eb.direction == ibis_doc_model::ErrorBarDirection::X;
                        let pts: Vec<(usize, f64, f32, f32)> = points.iter().map(|&(i, x, y, val)| {
                            let data_v = if is_x { x_value(series, i) } else { val };
                            (i, data_v, x, y)
                        }).collect();
                        let s_used = if is_x { &sx } else { &sy };
                        draw_error_bars(&mut marks, eb, series, &pts, plot, scale, s_used, is_x);
                    }
                }
            }
            ChartType::Bubble { scale: bubble_scale } => {
                let (sx, sy) = (x_scale_for(group), scale_for(group));
                // The largest bubble is a quarter of the plot's smaller side
                // across (at 100%), the others by their area.
                let biggest = group.series.iter().flat_map(|s| s.bubble_sizes.iter().flatten()).map(|n| n.0.abs()).fold(0.0, f64::max);
                let max_r = plot.w.min(plot.h) * 0.25 / 2.0 * bubble_scale as f32 / 100.0;
                for series in &group.series {
                    for (i, v) in series.values.iter().enumerate() {
                        let Some(v) = v else { continue };
                        let size = series.bubble_sizes.get(i).copied().flatten().map_or(0.0, |n| n.0.abs());
                        if biggest <= 0.0 || size <= 0.0 {
                            continue;
                        }
                        let r = max_r * (size / biggest).sqrt() as f32;
                        let (x, y) = (plot.x + sx.at(x_value(series, i)) * plot.w, value_px(&sy, v.0));
                        let circle: Vec<(f32, f32)> = (0..48).map(|k| {
                            let a = k as f32 / 48.0 * std::f32::consts::TAU;
                            (x + r * a.cos(), y + r * a.sin())
                        }).collect();
                        if let Some(bg) = background(point_fill(series, i)) {
                            marks.push(Mark::Fill(circle.clone(), bg));
                        }
                        // A sphere: the edge shaded, and a highlight of lighter discs, each smaller than the
                        // one before and further up and to the left.
                        if series.bubble_3d
                            && let Some(color) = background_color(point_fill(series, i))
                        {
                            for k in 1..=4 {
                                let f = k as f32;
                                let (cr, cx, cy) = (r * (1.0 - 0.17 * f), x - r * 0.07 * f, y - r * 0.07 * f);
                                let disc: Vec<(f32, f32)> = (0..32).map(|j| {
                                    let a = j as f32 / 32.0 * std::f32::consts::TAU;
                                    (cx + cr * a.cos(), cy + cr * a.sin())
                                }).collect();
                                let lighter = Hsla { l: (color.l + (1.0 - color.l) * 0.17 * f).min(1.0), a: color.a * 0.75, ..color };
                                marks.push(Mark::Fill(disc, lighter.into()));
                            }
                        }
                        if let Some(stroke) = point_stroke(series, i) {
                            let mut outline = circle.clone();
                            outline.push(circle[0]);
                            push_stroke(&mut marks, outline, stroke.width as f32 * scale, stroke);
                        }
                        let place = move |_: LabelPosition, _: f32, _: f32| -> (f32, f32, Anchor) { (x, y, Anchor::Center) };
                        push_label(&mut labels, series, i, v.0, v.0, LabelPosition::Center, (w, h, plot), px_per_point, res, place);
                    }
                }
            }
            ChartType::Pie { first_angle } | ChartType::Doughnut { first_angle, .. } => {
                let ring_count = if matches!(group.kind, ChartType::Doughnut { .. }) { group.series.len() } else { 1 };
                for (ring, series) in group.series.iter().take(ring_count).enumerate() {
                    // With no separator named, PowerPoint joins a pie label's parts with ", " (its PDFs of every pair and triple of category name, value, percentage and series
                    // name, at bestFit, outEnd and ctr) except a category name with the percentage and nothing else, which stand on two lines.
                    let broken;
                    let series = if series.labels.as_ref().is_some_and(pie_label_on_two_lines) {
                        let mut copy = series.clone();
                        if let Some(l) = copy.labels.as_mut() {
                            l.separator = Some("
".to_string());
                        }
                        broken = copy;
                        &broken
                    } else {
                        series
                    };
                    let hole = match group.kind {
                        ChartType::Doughnut { hole, .. } => hole as f32 / 100.0,
                        _ => 0.0,
                    };
                    let (cx, cy) = (plot.x + plot.w / 2.0, plot.y + plot.h / 2.0);
                    let label_radius = plot.w.min(plot.h) / 2.0;
                    // Automatic outside/best-fit pie labels need room around the
                    // slices; keep their anchors while reserving a text line.
                    let reserve = if chart.plot_area.is_none() && matches!(group.kind, ChartType::Pie { .. }) {
                        series.labels.as_ref().filter(|l| l.show_value || l.show_category || l.show_series_name || l.show_percent)
                            .filter(|l| matches!(l.position, None | Some(LabelPosition::OutsideEnd | LabelPosition::BestFit)))
                            .map_or(0.0, |l| text_height(&l.style, px_per_point))
                    } else { 0.0 };
                    let radius = (label_radius - reserve).max(0.0);
                    // The automatic bottom-legend pie leaves chart padding and
                    // half the legend text size above its circle (native PDF).
                    let radius = if chart.plot_area.is_none() && matches!(group.kind, ChartType::Pie { .. } | ChartType::Doughnut { .. }) {
                        chart.legend.as_ref().filter(|l| l.layout.is_none() && l.side == Side::Bottom)
                            .map_or(radius, |l| radius.min((cy - CHART_PADDING_PT * px_per_point - l.text.size as f32 / 100.0 * px_per_point * 0.5).max(0.0)))
                    } else { radius };
                    // Each doughnut series is a concentric ring, first innermost.
                    // Share the radial space from the hole to the plot edge evenly.
                    let inner = radius * hole;
                    let (radius, inner) = if matches!(group.kind, ChartType::Doughnut { .. }) {
                        let thickness = (radius - inner) / ring_count as f32;
                        (inner + thickness * (ring + 1) as f32, inner + thickness * ring as f32)
                    } else { (radius, inner) };
                    // A 3-D pie: an ellipse (flatter the less it is tilted towards the viewer) with a side of
                    // some thickness, the whole of it centred in the plot area.
                    let tilt = group.three_d.filter(|_| matches!(group.kind, ChartType::Pie { .. }));
                    let (radius, cy, yk, thick) = match tilt {
                        Some(v) => {
                            let yk = ((v.rot_x as f32).to_radians().sin().abs() * 0.87).clamp(0.1, 1.0);
                            let tk = 0.2 * v.depth_percent as f32 / 100.0;
                            let rx = (plot.w / 2.0).min(plot.h / (2.0 * yk + tk));
                            let total = 2.0 * rx * yk + rx * tk;
                            (rx, plot.y + (plot.h - total) / 2.0 + rx * yk, yk, rx * tk)
                        }
                        None => (radius, cy, 1.0, 0.0),
                    };
                    let total: f64 = series.values.iter().flatten().map(|n| n.0.max(0.0)).sum();
                    if total <= 0.0 {
                        continue;
                    }
                    // Slices pulled out (`explosion`, a percentage of the radius) shrink the pie so that the
                    // farthest one still ends at the plot's edge (PowerPoint: a 25% explosion draws the pie
                    // at 1/1.25 of its size).
                    let explosion_of = |ci: usize| series.point_explosions.iter().find(|(i, _)| *i == ci).map_or(series.explosion, |(_, e)| *e) as f32 / 100.0;
                    let most = if matches!(group.kind, ChartType::Pie { .. }) && tilt.is_none() {
                        (0..series.values.len()).map(explosion_of).fold(0.0, f32::max)
                    } else { 0.0 };
                    let radius = radius / (1.0 + most);
                    let label_radius = label_radius / (1.0 + most);
                    let mut angle = first_angle as f32;
                    for (ci, value) in series.values.iter().enumerate() {
                        let Some(value) = value.map(|n| n.0).filter(|v| *v > 0.0) else { continue };
                        let sweep = (value / total) as f32 * 360.0;
                        let (a0, a1) = ((angle - 90.0).to_radians(), (angle + sweep - 90.0).to_radians());
                        let steps = ((sweep / 3.0).ceil() as usize).max(2);
                        // This slice's own centre, moved out along its middle direction.
                        let pull = if most > 0.0 { explosion_of(ci) * radius } else { 0.0 };
                        let (cx, cy) = (cx + pull * ((a0 + a1) / 2.0).cos(), cy + pull * ((a0 + a1) / 2.0).sin());
                        let arc = |r: f32| (0..=steps).map(move |i| {
                            let t = a0 + (a1 - a0) * i as f32 / steps as f32;
                            (cx + r * t.cos(), cy + r * yk * t.sin())
                        });
                        if tilt.is_some()
                            && let Some(color) = background_color(point_fill(series, ci))
                        {
                            // The side shows where the rim faces the viewer (the lower half).
                            let side = shaded(color, 0.7);
                            let rim: Vec<f32> = (0..=steps).map(|i| a0 + (a1 - a0) * i as f32 / steps as f32).collect();
                            for pair in rim.windows(2) {
                                if (pair[0].sin() + pair[1].sin()) / 2.0 < 0.0 {
                                    continue;
                                }
                                let at = |t: f32, dy: f32| (cx + radius * t.cos(), cy + radius * yk * t.sin() + dy);
                                marks.push(Mark::Fill(vec![at(pair[0], 0.0), at(pair[1], 0.0), at(pair[1], thick), at(pair[0], thick)], side.into()));
                            }
                        }
                        let mut outline: Vec<(f32, f32)> = arc(radius).collect();
                        if inner > 0.0 {
                            outline.extend(arc(inner).collect::<Vec<_>>().into_iter().rev());
                        } else {
                            outline.push((cx, cy));
                        }
                        if let Some(bg) = background(point_fill(series, ci)) {
                            marks.push(Mark::Fill(outline.clone(), bg));
                        }
                        if let Some(stroke) = point_stroke(series, ci) {
                            let mut closed = outline.clone();
                            closed.push(outline[0]);
                            push_stroke(&mut marks, closed, stroke.width as f32 * scale, stroke);
                        }
                        let mid = (a0 + a1) / 2.0;
                        let tilted = tilt.is_some();
                        let place = move |pos: LabelPosition, text_h: f32, text_w: f32| -> (f32, f32, Anchor) {
                            let r = match pos {
                                LabelPosition::OutsideEnd => label_radius * 1.15,
                                LabelPosition::InsideEnd => radius * 0.8,
                                LabelPosition::Center if inner == 0.0 => radius * 0.5,
                                _ if inner > 0.0 => (radius + inner) / 2.0,
                                // Best fit: inside while the label fits across the slice there, else outside
                                // the pie, close to its edge (PowerPoint puts a small slice's label out).
                                // Office does not publish how it places a best-fit label: the inner radius (0.65 of the
                                // pie's) and the 4 pt gap are our choices, compared by eye with one PowerPoint PDF only.
                                LabelPosition::BestFit if !tilted => {
                                    let inside = label_radius * 0.65;
                                    let across = mid.sin().abs() * text_w + mid.cos().abs() * text_h;
                                    if inside * (a1 - a0).abs() >= across {
                                        inside
                                    } else {
                                        label_radius + 4.0 * px_per_point + mid.cos().abs() * text_w / 2.0 + mid.sin().abs() * text_h / 2.0
                                    }
                                }
                                _ => label_radius * 0.65,
                            };
                            (cx + r * mid.cos(), cy + r * yk * mid.sin(), Anchor::Center)
                        };
                        push_label(&mut labels, series, ci, value, total, LabelPosition::BestFit, (w, h, plot), px_per_point, res, place);
                        angle += sweep;
                    }
                }
            }
            ChartType::OfPie { kind } => {
                let Some(series) = group.series.first() else { continue };
                let op = group.of_pie.as_ref();
                let total: f64 = series.values.iter().flatten().map(|n| n.0.max(0.0)).sum();
                if total <= 0.0 {
                    continue;
                }

                let valid_count = series.values.iter().filter(|v| v.is_some_and(|n| n.0 > 0.0)).count();
                let mut secondary_mask = vec![false; series.values.len()];
                let split_kind = op.and_then(|o| o.split).unwrap_or(OfPieSplit::Position);
                let split_pos = op.and_then(|o| o.split_position);

                match split_kind {
                    OfPieSplit::Position => {
                        let n_sec = split_pos.map(|n| n.0 as usize).unwrap_or(2).clamp(1, valid_count.saturating_sub(1).max(1));
                        let mut marked = 0;
                        for (ci, val) in series.values.iter().enumerate().rev() {
                            if val.is_some_and(|n| n.0 > 0.0) && marked < n_sec {
                                secondary_mask[ci] = true;
                                marked += 1;
                            }
                        }
                    }
                    OfPieSplit::Value => {
                        let threshold = split_pos.map(|n| n.0).unwrap_or(0.0);
                        for (ci, val) in series.values.iter().enumerate() {
                            if let Some(v) = val.map(|n| n.0).filter(|v| *v > 0.0) {
                                if v < threshold {
                                    secondary_mask[ci] = true;
                                }
                            }
                        }
                    }
                    OfPieSplit::Percent => {
                        let pct_thresh = split_pos.map(|n| n.0).unwrap_or(10.0);
                        for (ci, val) in series.values.iter().enumerate() {
                            if let Some(v) = val.map(|n| n.0).filter(|v| *v > 0.0) {
                                if (v / total * 100.0) < pct_thresh {
                                    secondary_mask[ci] = true;
                                }
                            }
                        }
                    }
                    OfPieSplit::Custom => {
                        if let Some(pts) = op.and_then(|o| o.custom_points.as_ref()) {
                            for (ci, _) in series.values.iter().enumerate() {
                                if pts.iter().flatten().any(|&p| p as usize == ci) {
                                    secondary_mask[ci] = true;
                                }
                            }
                        }
                    }
                }

                let sec_count = secondary_mask.iter().copied().enumerate().filter(|(i, m)| *m && series.values[*i].is_some_and(|n| n.0 > 0.0)).count();
                if sec_count == 0 && valid_count >= 2 {
                    if let Some((last_idx, _)) = series.values.iter().enumerate().rfind(|(_, v)| v.is_some_and(|n| n.0 > 0.0)) {
                        secondary_mask[last_idx] = true;
                    }
                } else if sec_count == valid_count && valid_count >= 2 {
                    if let Some((first_idx, _)) = series.values.iter().enumerate().find(|(_, v)| v.is_some_and(|n| n.0 > 0.0)) {
                        secondary_mask[first_idx] = false;
                    }
                }

                let secondary_total: f64 = series.values.iter().enumerate()
                    .filter(|(i, _)| secondary_mask[*i])
                    .filter_map(|(_, v)| v.as_ref().map(|n| n.0.max(0.0)))
                    .sum();

                let second_size_pct = op.and_then(|o| o.second_size).unwrap_or(75).clamp(30, 200) as f32;
                let size_ratio = second_size_pct / 100.0;
                let gap_pct = op.and_then(|o| o.gap_width).unwrap_or(100).clamp(20, 400) as f32;
                let gap_ratio = gap_pct / 100.0;

                let is_bar = matches!(kind, Some(OfPieKind::Bar));
                let cy = plot.y + plot.h / 2.0;

                let width_factor = if is_bar {
                    2.0 + gap_ratio * 0.7 + 0.75 * size_ratio
                } else {
                    2.0 + gap_ratio * 0.7 + 2.0 * size_ratio
                };
                let height_limit = plot.h * 0.45;
                let width_limit = (plot.w * 0.95) / width_factor;
                let r1 = height_limit.min(width_limit).max(10.0);
                let r2 = r1 * size_ratio;
                let gap = r1 * gap_ratio * 0.7;

                let bar_w = (r1 * 0.75 * size_ratio).max(12.0);
                let bar_h = 2.0 * r2;

                let total_used_w = if is_bar {
                    2.0 * r1 + gap + bar_w
                } else {
                    2.0 * r1 + gap + 2.0 * r2
                };
                let start_x = plot.x + (plot.w - total_used_w).max(0.0) / 2.0;
                let cx1 = start_x + r1;

                let (cx2, bar_x, bar_y) = if is_bar {
                    let bx = start_x + 2.0 * r1 + gap;
                    let by = cy - bar_h / 2.0;
                    (bx + bar_w / 2.0, bx, by)
                } else {
                    let c2 = start_x + 2.0 * r1 + gap + r2;
                    (c2, c2 - r2, cy - r2)
                };

                let sweep_comp = if total > 0.0 { (secondary_total / total) as f32 * 360.0 } else { 0.0 };
                let half_sweep = sweep_comp / 2.0;
                let comp_a0 = (-half_sweep).to_radians();
                let comp_a1 = half_sweep.to_radians();

                let conn_p_top = (cx1 + r1 * comp_a0.cos(), cy + r1 * comp_a0.sin());
                let conn_s_top = if is_bar { (bar_x, bar_y) } else { (cx2, cy - r2) };

                let conn_p_bot = (cx1 + r1 * comp_a1.cos(), cy + r1 * comp_a1.sin());
                let conn_s_bot = if is_bar { (bar_x, bar_y + bar_h) } else { (cx2, cy + r2) };

                // 1. Draw composite slice in primary pie
                if secondary_total > 0.0 {
                    let steps = ((sweep_comp / 3.0).ceil() as usize).max(2);
                    let arc = (0..=steps).map(|i| {
                        let t = comp_a0 + (comp_a1 - comp_a0) * i as f32 / steps as f32;
                        (cx1 + r1 * t.cos(), cy + r1 * t.sin())
                    });
                    let mut outline: Vec<(f32, f32)> = arc.collect();
                    outline.push((cx1, cy));

                    let comp_bg = Hsla { h: 0.0, s: 0.0, l: 0.75, a: 1.0 };
                    marks.push(Mark::Fill(outline.clone(), comp_bg.into()));
                    let mut closed = outline.clone();
                    closed.push(outline[0]);
                    marks.push(Mark::Stroke(closed, 1.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xFFFFFF)))));

                    let lx = cx1 + (r1 * 0.65);
                    let ly = cy;
                    let other_text = crate::shell::i18n::text("_shell.more").to_string();
                    labels.push(Label::plain(
                        lx,
                        ly,
                        format!("{other_text} ({:.1}%)", secondary_total / total * 100.0),
                        ChartText { size: 900, ..ChartText::default() },
                        Anchor::Center,
                    ));
                }

                // 2. Draw remaining primary slices
                let mut curr_angle = half_sweep;
                for (ci, value) in series.values.iter().enumerate() {
                    if secondary_mask[ci] { continue; }
                    let Some(val) = value.map(|n| n.0).filter(|v| *v > 0.0) else { continue };
                    let sweep = (val / total) as f32 * 360.0;
                    let (a0, a1) = (curr_angle.to_radians(), (curr_angle + sweep).to_radians());
                    let steps = ((sweep / 3.0).ceil() as usize).max(2);
                    let arc = (0..=steps).map(|i| {
                        let t = a0 + (a1 - a0) * i as f32 / steps as f32;
                        (cx1 + r1 * t.cos(), cy + r1 * t.sin())
                    });
                    let mut outline: Vec<(f32, f32)> = arc.collect();
                    outline.push((cx1, cy));
                    if let Some(bg) = background(point_fill(series, ci)) {
                        marks.push(Mark::Fill(outline.clone(), bg));
                    }
                    if let Some(stroke) = point_stroke(series, ci) {
                        let mut closed = outline.clone();
                        closed.push(outline[0]);
                        push_stroke(&mut marks, closed, stroke.width as f32 * scale, stroke);
                    } else {
                        let mut closed = outline.clone();
                        closed.push(outline[0]);
                        marks.push(Mark::Stroke(closed, 1.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xFFFFFF)))));
                    }
                    let mid = (a0 + a1) / 2.0;
                    let place = move |_pos: LabelPosition, _: f32, _: f32| -> (f32, f32, Anchor) {
                        (cx1 + (r1 * 0.65) * mid.cos(), cy + (r1 * 0.65) * mid.sin(), Anchor::Center)
                    };
                    push_label(&mut labels, series, ci, val, total, LabelPosition::BestFit, (w, h, plot), px_per_point, res, place);
                    curr_angle += sweep;
                }

                // 3. Draw secondary plot
                if is_bar {
                    let mut seg_y = bar_y;
                    for (ci, value) in series.values.iter().enumerate() {
                        if !secondary_mask[ci] { continue; }
                        let Some(val) = value.map(|n| n.0).filter(|v| *v > 0.0) else { continue };
                        let seg_h = if secondary_total > 0.0 { (val / secondary_total) as f32 * bar_h } else { 0.0 };
                        let rect_pts = vec![
                            (bar_x, seg_y),
                            (bar_x + bar_w, seg_y),
                            (bar_x + bar_w, seg_y + seg_h),
                            (bar_x, seg_y + seg_h),
                        ];
                        if let Some(bg) = background(point_fill(series, ci)) {
                            marks.push(Mark::Fill(rect_pts.clone(), bg));
                        }
                        if let Some(stroke) = point_stroke(series, ci) {
                            let mut closed = rect_pts.clone();
                            closed.push(rect_pts[0]);
                            push_stroke(&mut marks, closed, stroke.width as f32 * scale, stroke);
                        } else {
                            let mut closed = rect_pts.clone();
                            closed.push(rect_pts[0]);
                            marks.push(Mark::Stroke(closed, 1.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xFFFFFF)))));
                        }
                        let place = move |_pos: LabelPosition, _: f32, _: f32| -> (f32, f32, Anchor) {
                            (bar_x + bar_w / 2.0, seg_y + seg_h / 2.0, Anchor::Center)
                        };
                        push_label(&mut labels, series, ci, val, total, LabelPosition::BestFit, (w, h, plot), px_per_point, res, place);
                        seg_y += seg_h;
                    }
                } else {
                    let mut sec_angle = -90.0f32;
                    for (ci, value) in series.values.iter().enumerate() {
                        if !secondary_mask[ci] { continue; }
                        let Some(val) = value.map(|n| n.0).filter(|v| *v > 0.0) else { continue };
                        let sweep = if secondary_total > 0.0 { (val / secondary_total) as f32 * 360.0 } else { 0.0 };
                        let (a0, a1) = (sec_angle.to_radians(), (sec_angle + sweep).to_radians());
                        let steps = ((sweep / 3.0).ceil() as usize).max(2);
                        let arc = (0..=steps).map(|i| {
                            let t = a0 + (a1 - a0) * i as f32 / steps as f32;
                            (cx2 + r2 * t.cos(), cy + r2 * t.sin())
                        });
                        let mut outline: Vec<(f32, f32)> = arc.collect();
                        outline.push((cx2, cy));
                        if let Some(bg) = background(point_fill(series, ci)) {
                            marks.push(Mark::Fill(outline.clone(), bg));
                        }
                        if let Some(stroke) = point_stroke(series, ci) {
                            let mut closed = outline.clone();
                            closed.push(outline[0]);
                            push_stroke(&mut marks, closed, stroke.width as f32 * scale, stroke);
                        } else {
                            let mut closed = outline.clone();
                            closed.push(outline[0]);
                            marks.push(Mark::Stroke(closed, 1.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xFFFFFF)))));
                        }
                        let mid = (a0 + a1) / 2.0;
                        let place = move |_pos: LabelPosition, _: f32, _: f32| -> (f32, f32, Anchor) {
                            (cx2 + (r2 * 0.65) * mid.cos(), cy + (r2 * 0.65) * mid.sin(), Anchor::Center)
                        };
                        push_label(&mut labels, series, ci, val, total, LabelPosition::BestFit, (w, h, plot), px_per_point, res, place);
                        sec_angle += sweep;
                    }
                }

                // 4. Draw connector lines
                let conn_stroke_color = op.and_then(|o| o.connector.as_ref()?.as_ref()).map(|s| hsla(s.color)).unwrap_or_else(|| Hsla { h: 0.0, s: 0.0, l: 0.5, a: 1.0 });
                let conn_stroke_width = op.and_then(|o| o.connector.as_ref()?.as_ref()).map(|s| s.width as f32 * scale).unwrap_or(1.0 * px_per_point);

                marks.push(Mark::Stroke(vec![conn_p_top, conn_s_top], conn_stroke_width, conn_stroke_color));
                marks.push(Mark::Stroke(vec![conn_p_bot, conn_s_bot], conn_stroke_width, conn_stroke_color));
            }
            ChartType::Stock { up_down_bars, hi_low_lines, drop_lines } => {
                let s = scale_for(group);
                let n_ser = group.series.len();
                let (open_series, high_series, low_series, close_series) = if n_ser >= 4 {
                    (Some(&group.series[0]), Some(&group.series[1]), Some(&group.series[2]), Some(&group.series[3]))
                } else if n_ser >= 3 {
                    (None, Some(&group.series[0]), Some(&group.series[1]), Some(&group.series[2]))
                } else if n_ser == 2 {
                    (None, Some(&group.series[0]), Some(&group.series[1]), None)
                } else if n_ser == 1 {
                    (None, None, None, Some(&group.series[0]))
                } else {
                    (None, None, None, None)
                };

                for ci in 0..categories {
                    let (start, len) = band(ci);
                    let cx = start + len / 2.0;
                    let half_bar_w = (len * 0.25).clamp(2.0 * px_per_point, 24.0 * px_per_point);

                    let open_val = open_series.and_then(|ser| ser.values.get(ci).copied().flatten().map(|n| n.0));
                    let high_val = high_series.and_then(|ser| ser.values.get(ci).copied().flatten().map(|n| n.0));
                    let low_val = low_series.and_then(|ser| ser.values.get(ci).copied().flatten().map(|n| n.0));
                    let close_val = close_series.and_then(|ser| ser.values.get(ci).copied().flatten().map(|n| n.0));

                    // 1. Drop lines: vertical line from lowest value down to category axis / plot bottom
                    if drop_lines {
                        let min_v = low_val.or(close_val).or(open_val).or(high_val);
                        if let Some(v) = min_v {
                            let y_low = value_px(&s, v);
                            let y_base = if horizontal { plot.x } else { plot.bottom() };
                            let pts = if horizontal { vec![(y_low, cx), (y_base, cx)] } else { vec![(cx, y_low), (cx, y_base)] };
                            marks.push(Mark::Stroke(pts, 1.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x808080)))));
                        }
                    }

                    // 2. High-Low lines: line connecting high to low
                    if hi_low_lines {
                        if let (Some(h), Some(l)) = (high_val, low_val) {
                            let y_high = value_px(&s, h);
                            let y_low = value_px(&s, l);
                            let stroke_color = high_series.and_then(|ser| ser.stroke.as_ref()).map(|st| hsla(st.color))
                                .unwrap_or_else(|| hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x000000))));
                            let stroke_width = high_series.and_then(|ser| ser.stroke.as_ref()).map(|st| st.width as f32 * scale).unwrap_or(1.0 * px_per_point);
                            let pts = if horizontal { vec![(y_low, cx), (y_high, cx)] } else { vec![(cx, y_high), (cx, y_low)] };
                            marks.push(Mark::Stroke(pts, stroke_width, stroke_color));
                        }
                    }

                    // 3. Up-Down bars (candlesticks)
                    if up_down_bars && open_val.is_some() && close_val.is_some() {
                        let o = open_val.unwrap();
                        let c = close_val.unwrap();
                        let y_o = value_px(&s, o);
                        let y_c = value_px(&s, c);
                        let (y_top, y_bot) = if y_o < y_c { (y_o, y_c) } else { (y_c, y_o) };
                        let is_up = c >= o;
                        let (bg, border_color) = if is_up {
                            (background(&ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xFFFFFF)))),
                             hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x000000))))
                        } else {
                            (background(&ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x000000)))),
                             hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x000000))))
                        };
                        let rect_pts = if horizontal {
                            vec![(y_top, cx - half_bar_w), (y_bot, cx - half_bar_w), (y_bot, cx + half_bar_w), (y_top, cx + half_bar_w)]
                        } else {
                            vec![(cx - half_bar_w, y_top), (cx + half_bar_w, y_top), (cx + half_bar_w, y_bot), (cx - half_bar_w, y_bot)]
                        };
                        if let Some(bg) = bg {
                            marks.push(Mark::Fill(rect_pts.clone(), bg));
                        }
                        let mut closed = rect_pts.clone();
                        closed.push(rect_pts[0]);
                        marks.push(Mark::Stroke(closed, 1.0 * scale, border_color));
                    } else {
                        // Tick marks for Open and Close when not using up_down_bars (e.g. HLC or OHLC bar chart)
                        if let Some(c) = close_val {
                            let y_close = value_px(&s, c);
                            let pts = if horizontal { vec![(y_close, cx), (y_close, cx + half_bar_w)] } else { vec![(cx, y_close), (cx + half_bar_w, y_close)] };
                            marks.push(Mark::Stroke(pts, 1.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x000000)))));
                        }
                        if let Some(o) = open_val {
                            let y_open = value_px(&s, o);
                            let pts = if horizontal { vec![(y_open, cx - half_bar_w), (y_open, cx)] } else { vec![(cx - half_bar_w, y_open), (cx, y_open)] };
                            marks.push(Mark::Stroke(pts, 1.0 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x000000)))));
                        }
                    }

                    // 4. Data labels
                    for ser in &group.series {
                        if let Some(val) = ser.values.get(ci).copied().flatten().map(|n| n.0) {
                            let y_val = value_px(&s, val);
                            let place = move |_pos: LabelPosition, _: f32, _: f32| -> (f32, f32, Anchor) {
                                if horizontal {
                                    (y_val, cx, Anchor::Center)
                                } else {
                                    (cx, y_val, Anchor::Center)
                                }
                            };
                            push_label(&mut labels, ser, ci, val, val, LabelPosition::BestFit, (w, h, plot), px_per_point, res, place);
                        }
                    }
                }
            }
            ChartType::Surface { is_3d, wireframe } => {
                let num_series = group.series.len();
                let num_cats = group.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
                if num_series > 0 && num_cats > 0 {
                    let (min_val, max_val) = surface_min_max(group);
                    let mut grid: Vec<Vec<f64>> = Vec::with_capacity(num_series);
                    for ser in &group.series {
                        let mut row = Vec::with_capacity(num_cats);
                        for ci in 0..num_cats {
                            let v = ser.values.get(ci).copied().flatten().map(|n| n.0).unwrap_or(min_val);
                            row.push(v);
                        }
                        grid.push(row);
                    }

                    let norm_z = |val: f64| -> f32 {
                        if (max_val - min_val).abs() < 1e-6 {
                            0.5
                        } else {
                            ((val - min_val) / (max_val - min_val)).clamp(0.0, 1.0) as f32
                        }
                    };

                    if is_3d {
                        // 3D Isometric Projection
                        let cx = plot.x + plot.w * 0.5;
                        let cy = plot.y + plot.h * 0.54;
                        let box_w = plot.w * 0.65;
                        let box_d = plot.h * 0.32;
                        let box_h = plot.h * 0.40;

                        let project = |s_idx: usize, c_idx: usize| -> (f32, f32) {
                            let u = if num_cats > 1 { c_idx as f32 / (num_cats - 1) as f32 - 0.5 } else { 0.0 };
                            let v = if num_series > 1 { s_idx as f32 / (num_series - 1) as f32 - 0.5 } else { 0.0 };
                            let z = norm_z(grid[s_idx][c_idx]);
                            let px = cx + (u - v) * box_w * 0.707;
                            let py = cy + (u + v) * box_d * 0.5 - z * box_h;
                            (px, py)
                        };

                        if num_series >= 2 && num_cats >= 2 {
                            let mut quads: Vec<(f32, usize, usize)> = Vec::with_capacity((num_series - 1) * (num_cats - 1));
                            for s_idx in 0..(num_series - 1) {
                                for c_idx in 0..(num_cats - 1) {
                                    let depth = (s_idx as f32 / num_series as f32) + (c_idx as f32 / num_cats as f32);
                                    quads.push((depth, s_idx, c_idx));
                                }
                            }
                            quads.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

                            for (_, s_idx, c_idx) in quads {
                                let p00 = project(s_idx, c_idx);
                                let p01 = project(s_idx, c_idx + 1);
                                let p11 = project(s_idx + 1, c_idx + 1);
                                let p10 = project(s_idx + 1, c_idx);

                                let avg_z = (norm_z(grid[s_idx][c_idx])
                                    + norm_z(grid[s_idx][c_idx + 1])
                                    + norm_z(grid[s_idx + 1][c_idx + 1])
                                    + norm_z(grid[s_idx + 1][c_idx]))
                                    * 0.25;
                                let color = surface_band_color(avg_z);

                                if !wireframe {
                                    let fill_pts = vec![p00, p01, p11, p10];
                                    let rgb_val = match (avg_z * 4.0) as usize {
                                        0 => 0x5B9BD5,
                                        1 => 0xED7D31,
                                        2 => 0xA5A5A5,
                                        _ => 0xFFC000,
                                    };
                                    let fill_bg = background(&ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(rgb_val))));
                                    if let Some(bg) = fill_bg {
                                        marks.push(Mark::Fill(fill_pts, bg));
                                    }
                                    let stroke_pts = vec![p00, p01, p11, p10, p00];
                                    marks.push(Mark::Stroke(stroke_pts, 0.75 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x595959)))));
                                } else {
                                    let stroke_pts = vec![p00, p01, p11, p10, p00];
                                    marks.push(Mark::Stroke(stroke_pts, 1.0 * scale, color));
                                }
                            }
                        } else {
                            for s_idx in 0..num_series {
                                let mut pts = Vec::with_capacity(num_cats);
                                for c_idx in 0..num_cats {
                                    pts.push(project(s_idx, c_idx));
                                }
                                marks.push(Mark::Stroke(pts, 1.5 * scale, surface_band_color(s_idx as f32 / num_series.max(1) as f32)));
                            }
                        }
                    } else {
                        // 2D Contour Map
                        let pad_x = 10.0 * px_per_point;
                        let pad_y = 10.0 * px_per_point;
                        let left = plot.x + pad_x;
                        let right = plot.right() - pad_x;
                        let top = plot.y + pad_y;
                        let bottom = plot.bottom() - pad_y;

                        let project_2d = |s_idx: usize, c_idx: usize| -> (f32, f32) {
                            let px = if num_cats > 1 {
                                left + c_idx as f32 / (num_cats - 1) as f32 * (right - left)
                            } else {
                                (left + right) * 0.5
                            };
                            let py = if num_series > 1 {
                                bottom - s_idx as f32 / (num_series - 1) as f32 * (bottom - top)
                            } else {
                                (top + bottom) * 0.5
                            };
                            (px, py)
                        };

                        if num_series >= 2 && num_cats >= 2 {
                            for s_idx in 0..(num_series - 1) {
                                for c_idx in 0..(num_cats - 1) {
                                    let p00 = project_2d(s_idx, c_idx);
                                    let p01 = project_2d(s_idx, c_idx + 1);
                                    let p11 = project_2d(s_idx + 1, c_idx + 1);
                                    let p10 = project_2d(s_idx + 1, c_idx);

                                    let avg_z = (norm_z(grid[s_idx][c_idx])
                                        + norm_z(grid[s_idx][c_idx + 1])
                                        + norm_z(grid[s_idx + 1][c_idx + 1])
                                        + norm_z(grid[s_idx + 1][c_idx]))
                                        * 0.25;
                                    let color = surface_band_color(avg_z);

                                    if !wireframe {
                                        let fill_pts = vec![p00, p01, p11, p10];
                                        let rgb_val = match (avg_z * 4.0) as usize {
                                            0 => 0x5B9BD5,
                                            1 => 0xED7D31,
                                            2 => 0xA5A5A5,
                                            _ => 0xFFC000,
                                        };
                                        let fill_bg = background(&ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(rgb_val))));
                                        if let Some(bg) = fill_bg {
                                            marks.push(Mark::Fill(fill_pts, bg));
                                        }
                                        let stroke_pts = vec![p00, p01, p11, p10, p00];
                                        marks.push(Mark::Stroke(stroke_pts, 0.75 * scale, hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x8C8C8C)))));
                                    } else {
                                        let stroke_pts = vec![p00, p01, p11, p10, p00];
                                        marks.push(Mark::Stroke(stroke_pts, 1.0 * scale, color));
                                    }
                                }
                            }
                        } else {
                            for s_idx in 0..num_series {
                                let mut pts = Vec::with_capacity(num_cats);
                                for c_idx in 0..num_cats {
                                    pts.push(project_2d(s_idx, c_idx));
                                }
                                marks.push(Mark::Stroke(pts, 1.5 * scale, surface_band_color(s_idx as f32 / num_series.max(1) as f32)));
                            }
                        }
                    }
                }
            }
        }
    }

    match &chart.ex {
        Some(ChartEx::Box { boxes, fill, mean_marker, mean_line, outliers }) => {
            if let Some(group) = chart.groups.first() {
                let s = scale_for(group);
                let gap = match group.kind {
                    ChartType::Bar { gap_width, .. } => gap_width as f32 / 100.0,
                    _ => 1.0,
                };
                let half = band_len / (1.0 + gap) / 2.0;
                let color = background_color(fill).unwrap_or_else(|| hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472C4))));
                for (i, b) in boxes.iter().enumerate() {
                    let (start, len) = band(i);
                    let cx = start + len / 2.0;
                    let y = |v: f64| value_px(&s, v);
                    let (q1, q3, med) = (y(b.q1.0), y(b.q3.0), y(b.median.0));
                    // Excel draws the whiskers, the box's outline, the median and the mean in the fill's darker shade (80 %).
                    let whisker = {
                        let rgb = match fill { Fill::Solid(c) => c.rgb.0, _ => 0x156082 };
                        let dark = |v: u32| ((v as f32) * 0.8).round() as u32;
                        hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb((dark((rgb >> 16) & 0xff) << 16) | (dark((rgb >> 8) & 0xff) << 8) | dark(rgb & 0xff))))
                    };
                    let thin = 1.0 * px_per_point * 0.75;
                    marks.push(Mark::Stroke(vec![(cx, q3), (cx, y(b.whisker_high.0))], thin, whisker));
                    marks.push(Mark::Stroke(vec![(cx, q1), (cx, y(b.whisker_low.0))], thin, whisker));
                    for v in [b.whisker_high.0, b.whisker_low.0] {
                        marks.push(Mark::Stroke(vec![(cx - half * 0.5, y(v)), (cx + half * 0.5, y(v))], thin, whisker));
                    }
                    marks.push(Mark::Fill(vec![(cx - half, q3), (cx + half, q3), (cx + half, q1), (cx - half, q1)], color.into()));
                    marks.push(Mark::Stroke(vec![(cx - half, q3), (cx + half, q3), (cx + half, q1), (cx - half, q1), (cx - half, q3)], thin, whisker));
                    marks.push(Mark::Stroke(vec![(cx - half, med), (cx + half, med)], thin, whisker));
                    let dark = whisker;
                    if *mean_line {
                        let ym = y(b.mean.0);
                        marks.push(Mark::Stroke(vec![(cx - half, ym), (cx + half, ym)], thin, dark));
                    }
                    if *mean_marker {
                        let (ym, r) = (y(b.mean.0), 3.0 * px_per_point);
                        marks.push(Mark::Stroke(vec![(cx - r, ym - r), (cx + r, ym + r)], thin, dark));
                        marks.push(Mark::Stroke(vec![(cx - r, ym + r), (cx + r, ym - r)], thin, dark));
                    }
                    if *outliers {
                        for o in &b.outliers {
                            let (yo, r) = (y(o.0), 2.8 * px_per_point);
                            let ring: Vec<(f32, f32)> = (0..=16).map(|k| {
                                let a = k as f32 / 16.0 * std::f32::consts::TAU;
                                (cx + r * a.cos(), yo + r * a.sin())
                            }).collect();
                            marks.push(Mark::Stroke(ring, thin, color));
                        }
                    }
                }
            }
        }
        Some(ChartEx::Treemap { nodes, overlapping, values, labels_at_end }) => draw_treemap(nodes, (*overlapping, *values, *labels_at_end), plot, px_per_point, res, &mut marks, &mut labels),
        Some(ChartEx::Sunburst(nodes)) => draw_sunburst(nodes, plot, px_per_point, res, &mut marks, &mut labels),
        Some(ChartEx::Synthesized { links }) => {
            // A waterfall's connector lines, from one column's right edge to another's left edge.
            if let Some(group) = chart.groups.first().filter(|_| !links.is_empty()) {
                let s = scale_for(group);
                let gap = match group.kind {
                    ChartType::Bar { gap_width, .. } => gap_width as f32 / 100.0,
                    _ => 1.0,
                };
                let half = band_len / (1.0 + gap) / 2.0;
                let line = hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xD9D9D9)));
                for (from, from_level, to, to_level) in links {
                    let ((a, alen), (b, blen)) = (band(*from), band(*to));
                    let (x0, x1) = (a + alen / 2.0 + half, b + blen / 2.0 - half);
                    marks.push(Mark::Stroke(vec![(x0, value_px(&s, from_level.0)), (x1, value_px(&s, to_level.0))], 1.0 * px_per_point * 0.75, line));
                }
            }
        }
        None => {}
    }
    if cut {
        marks.push(Mark::Clip(None));
    }
    // Axis lines and category labels, above the marks.
    if !chart.groups.iter().any(is_radar) && !chart.groups.iter().any(is_of_pie) && !chart.groups.iter().any(|g| matches!(g.kind, ChartType::Surface { is_3d: true, .. })) {
        if let Some(axis) = category_axis {
            if let Some(line) = &axis.line {
                let base = chart
                    .groups
                    .iter()
                    .find(|g| !is_round(g))
                    .map(|g| {
                        let s = scale_for(g);
                        value_px(&s, base_value_of(chart, g, &s))
                    })
                    .unwrap_or(if horizontal { plot.x } else { plot.bottom() });
                let points = if horizontal { vec![(base, plot.y), (base, plot.bottom())] } else { vec![(plot.x, base), (plot.right(), base)] };
                push_stroke(&mut marks, points, line.width as f32 * scale, line);
            }
            if axis.show_labels {
                let gap = 4.0 * px_per_point;
                // Like PowerPoint, labels of a horizontal axis that do not fit
                // side by side are turned to read upwards, and then thinned out
                // if even that leaves them overlapping.
                let fixed_skip = axis.label_skip.max(1) as f32;
                let widest = category_names.iter().take(categories).map(|n| n.lines().map(|l| text_width(l, &axis.text, px_per_point)).fold(0.0, f32::max)).fold(0.0, f32::max);
                let rotation = match axis.label_rotation {
                    // Excel turns a date axis's labels upright (found on the
                    // charts of a user's private workbook).
                    // A date axis's labels are its ticks' (a major unit apart):
                    // turned 45° when a slanted line fits between them, else
                    // upright (Excel).
                    0 if !horizontal && time.is_some() => {
                        let spacing = time_labels_spacing(time.as_ref(), axis, band_len, text_height(&axis.text, px_per_point));
                        let widest = time_label_widest(time.as_ref(), axis, px_per_point);
                        let line = text_height(&axis.text, px_per_point);
                        match time.as_ref() {
                            // Automatic or overcrowded: the unit and turn chosen together.
                            Some((t, ..)) if t.major.is_none() || spacing < line * 1.5 => auto_time_step(t, band_len, widest, line).2,
                            // With one set, 45° when a slanted line fits (TikTok).
                            _ if widest <= spacing => 0,
                            // PowerPoint turns them straight up, never 45°.
                            _ if !chart.excel_layout => -5_400_000,
                            _ => crowded_rotation(&axis.text, spacing, px_per_point),
                        }
                    }
                    // Staggered labels (every other one lower) and labels with explicit rot="0" or multi-level categories do not turn.
                    0 if axis.stagger_labels || axis.fixed_label_rotation || !axis.category_levels.is_empty() => 0,
                    0 if !horizontal && widest > band_len * fixed_skip && axis.date => -5_400_000,
                    0 if !horizontal && widest > band_len * fixed_skip => crowded_rotation(&axis.text, band_len * fixed_skip, px_per_point),
                    r => r,
                };
                // Every label (a horizontal axis, or an automatic skip on one)
                // at least: `step_by` takes no 0.
                let skip = match axis.label_skip {
                    0 if axis.stagger_labels || axis.fixed_label_rotation || !axis.category_levels.is_empty() => 1,
                    0 if rotation != 0 => (text_height(&axis.text, px_per_point) / band_len).ceil().max(1.0) as usize,
                    0 if !horizontal => (widest / band_len).ceil().max(1.0) as usize,
                    n => n as usize,
                }
                .max(1);
                // A date axis labels its ticks: from its first date, every major
                // unit (or as often as the labels fit).
                let time_labels: Option<Vec<(f32, String)>> = time.as_ref().map(|(t, _, lo, hi)| {
                    let first = axis.min.map(|m| m.0).or_else(|| t.dates.iter().flatten().map(|d| d.0).reduce(f64::min)).unwrap_or(0.0);
                    // Counted in months or years, dates stand for their month's
                    // (year's) first day.
                    let first = unit_start(first, t.base);
                    let (step, unit) = match t.major {
                        Some((n, u)) => {
                            let per = match (t.base, u) {
                                (b, u) if b == u => 1.0,
                                (ibis_doc_model::TimeUnit::Days, ibis_doc_model::TimeUnit::Months) => 30.44,
                                (ibis_doc_model::TimeUnit::Days, ibis_doc_model::TimeUnit::Years) => 365.25,
                                (ibis_doc_model::TimeUnit::Months, ibis_doc_model::TimeUnit::Years) => 12.0,
                                _ => 1.0,
                            };
                            let spacing = band_len * n.0 as f32 * per;
                            let line = text_height(&axis.text, px_per_point);
                            if spacing < line * 1.5 {
                                let widest = time_label_widest(time.as_ref(), axis, px_per_point);
                                let (step, unit, _) = auto_time_step(t, band_len, widest, line);
                                (step, unit)
                            } else {
                                (n.0.max(1.0), u)
                            }
                        }
                        None => {
                            let widest = time_label_widest(time.as_ref(), axis, px_per_point);
                            let (step, unit, _) = auto_time_step(t, band_len, widest, text_height(&axis.text, px_per_point));
                            (step, unit)
                        }
                    };
                    let mut out = Vec::new();
                    let mut n = 0.0;
                    loop {
                        let date = add_time(first, step * n, unit);
                        let u = time_index(date, t.base);
                        if u > *hi + 1e-9 || n > 10_000.0 {
                            break;
                        }
                        if u >= *lo - 1e-9 {
                            out.push((time_x(u) + band_len / 2.0, ibis_doc_model::format_number(date, &axis.format)));
                        }
                        n += 1.0;
                    }
                    out
                });
                let time_skip = match &time_labels {
                    Some(list) if list.len() > 1 => {
                        let spacing = (list[1].0 - list[0].0).abs();
                        let line = text_height(&axis.text, px_per_point);
                        let needed = if rotation == 0 { widest } else { line * 1.25 };
                        (needed / spacing.max(1.0)).ceil().max(1.0) as usize
                    }
                    _ => 1,
                };
                let names: Vec<(usize, String)> = match &time_labels {
                    Some(list) => list.iter().enumerate().step_by(time_skip).map(|(i, (_, n))| (i, n.clone())).collect(),
                    None => category_names.iter().cloned().enumerate().take(categories).step_by(skip).collect(),
                };
                for (ci, name) in names.iter().map(|(i, n)| (*i, n)) {
                    let (start, len) = match &time_labels {
                        Some(list) => (list[ci].0 - band_len / 2.0, band_len),
                        None => band(ci),
                    };
                    if rotation != 0 {
                        let Some(color) = axis.text.color else { continue };
                        let (at, attach) = if horizontal {
                            ((plot.x - gap, start + len / 2.0), crate::render::rotated_text::Attach::RightMiddle)
                        } else if rotation.abs() != 5_400_000 {
                            ((start + len / 2.0, plot.bottom() + gap), crate::render::rotated_text::Attach::TopEnd)
                        } else {
                            ((start + len / 2.0, plot.bottom() + gap), crate::render::rotated_text::Attach::TopCenter)
                        };
                        rotated.push(crate::render::rotated_text::RotatedLine {
                            text: name.replace('\n', " "),
                            font: chart_font(&axis.text, name, res),
                            size: px(axis.text.size as f32 / 100.0 * px_per_point),
                            color: hsla(color),
                            at,
                            attach,
                            degrees: rotation as f32 / 60_000.0,
                            // Upright labels below the plot stay within the frame,
                            // which PowerPoint clips the chart to (the file's font
                            // may be wider than the one PowerPoint drew with).
                            max_len: (!horizontal && rotation.abs() == 5_400_000).then(|| h - (plot.bottom() + gap) - 1.0),
                        });
                        continue;
                    }
                    let label = if horizontal {
                        let room = (plot.x - gap - CHART_PADDING_PT * px_per_point).max(0.0);
                        let text = ellipsize_category_label(name, room, &axis.text, px_per_point, res);
                        Label::plain(plot.x - gap, start + len / 2.0, text, axis.text.clone(), Anchor::End)
                    } else {
                        let text = if (axis.fixed_label_rotation || !axis.category_levels.is_empty()) && !axis.stagger_labels {
                            wrap_category_label(name, len * skip as f32, &axis.text, px_per_point)
                        } else {
                            name.clone()
                        };
                        let lines = text.lines().count().max(1) as f32;
                        let half = lines * text_height(&axis.text, px_per_point) / 2.0;
                        // Labels that would leave the frame move up into it, over
                        // the plot area if need be (as PowerPoint draws them).
                        let lower = if axis.stagger_labels && widest > band_len * 1.08 && ci % 2 == 1 { text_height(&axis.text, px_per_point) * 1.2 } else { 0.0 };
                        let y = if axis.side == Side::Top {
                            (plot.y - gap - half - lower).max(half)
                        } else if chart.data_table.is_some() {
                            plot.bottom() + data_table_row_height(chart, px_per_point) / 2.0
                        } else {
                            (plot.bottom() + gap + half + lower).min(h - half)
                        };
                        Label::plain(start + len / 2.0, y, text, axis.text.clone(), Anchor::Center)
                    };
                    labels.push(label);
                }
                // The outer levels of categories of several levels (a year over its months): a row of labels for each,
                // under the labels, with the lines that part the categories running down through the rows.
                if !axis.category_levels.is_empty() && !horizontal && rotation == 0 && time_labels.is_none() && axis.side != Side::Top && categories > 0 {
                    let row = level_row_height(axis, px_per_point);
                    let inner_lines = names.iter().map(|(_, n)| wrap_category_label(n, band_len * skip as f32, &axis.text, px_per_point).lines().count().max(1)).max().unwrap_or(1) as f32;
                    let inner_row = (inner_lines - 1.0) * text_height(&axis.text, px_per_point) + row;
                    let (line_w, line_c) = axis.line.as_ref().map_or((0.75 * px_per_point, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x000000))), |l| (l.width as f32 * scale, l.color));
                    let edge = |i: usize| if i >= categories { let (a, l) = band(categories - 1); a + l } else { band(i).0 };
                    for i in 0..=categories {
                        marks.push(Mark::Stroke(vec![(edge(i), plot.bottom()), (edge(i), plot.bottom() + inner_row)], line_w, hsla(line_c)));
                    }
                    let mut top = plot.bottom() + inner_row;
                    for level in &axis.category_levels {
                        let bottom = top + row;
                        for (start, len, name) in level {
                            let (x0, x1) = (edge(*start), edge(start + len));
                            marks.push(Mark::Stroke(vec![(x0, plot.bottom()), (x0, bottom)], line_w, hsla(line_c)));
                            if start + len >= categories {
                                marks.push(Mark::Stroke(vec![(x1, plot.bottom()), (x1, bottom)], line_w, hsla(line_c)));
                            }
                            labels.push(Label::plain((x0 + x1) / 2.0, (top + row / 2.0).min(h - text_height(&axis.text, px_per_point) / 2.0), name.clone(), axis.text.clone(), Anchor::Center));
                        }
                        top = bottom;
                    }
                }
            }
        }
        for axis in chart.axes.iter().filter(|a| a.kind == AxisKind::Value && !a.deleted) {
            let Some(line) = &axis.line else { continue };
            let points = match (horizontal, axis.side) {
                (true, Side::Top) => vec![(plot.x, plot.y), (plot.right(), plot.y)],
                (true, _) => vec![(plot.x, plot.bottom()), (plot.right(), plot.bottom())],
                (false, Side::Right) => vec![(plot.right(), plot.y), (plot.right(), plot.bottom())],
                (false, Side::Top) => vec![(plot.x, plot.y), (plot.right(), plot.y)],
                (false, Side::Bottom) => vec![(plot.x, plot.bottom()), (plot.right(), plot.bottom())],
                (false, _) => vec![(plot.x, plot.y), (plot.x, plot.bottom())],
            };
            push_stroke(&mut marks, points, line.width as f32 * scale, line);
        }
    }

    // The plot area's outline (`c:plotArea/c:spPr/a:ln`), over the marks and axes.
    if let Some(border) = chart.plot_border.as_ref().filter(|_| depth.is_none() && !chart.groups.iter().any(|g| is_round(g) || is_radar(g))) {
        let outline = vec![(plot.x, plot.y), (plot.right(), plot.y), (plot.right(), plot.bottom()), (plot.x, plot.bottom()), (plot.x, plot.y)];
        push_stroke(&mut marks, outline, border.width as f32 * scale, border);
    }

    if let Some(table) = &chart.data_table {
        draw_data_table(chart, table, plot, horizontal, px_per_point, &mut marks, &mut labels);
    }

    #[cfg(all(target_os = "macos", feature = "scenarios"))]
    LAID_OUT.with(|l| {
        if let Some(list) = l.borrow_mut().as_mut() {
            list.push(laid_out(chart, (w, h), plot, &marks, &labels, &rotated, px_per_point, res));
        }
    });
    let canvas_marks = std::rc::Rc::new(marks);
    let container = div()
        .absolute()
        .left_0()
        .top_0()
        .w(px(w))
        .h(px(h))
        // PowerPoint clips a chart to its frame.
        .overflow_hidden()
        .when_some(background(&chart.background), |d, bg| d.bg(bg))
        .when_some(chart.border.as_ref(), |d, b| d.border(px((b.width as f32 * scale).max(0.5))).border_color(hsla(b.color)))
        // The chart area's rounded corners (`c:roundedCorners`).
        .when(chart.rounded_corners, |d| d.rounded(px(ROUNDED_CORNER_EMU * scale)))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    // Off the screen (or the printed page) nothing is drawn:
                    // a sheet of charts would otherwise make every chart's
                    // paths for its first frame.
                    if !bounds.intersects(&window.content_mask().bounds) {
                        return;
                    }
                    let o = bounds.origin;
                    // The marks' paths, made once (tessellating thousands of
                    // points takes long) and moved to where the chart is.
                    let paths = cached_paths(&canvas_marks, window.scale_factor());
                    let mut clip: Option<Bounds<Pixels>> = None;
                    for item in paths.iter() {
                        match item {
                            Painted::Clip(r) => {
                                clip = r.map(|(x, y, w, h)| Bounds::new(point(o.x + px(x), o.y + px(y)), size(px(w), px(h))));
                            }
                            Painted::Path(path, bg) => {
                                let mut path = path.clone();
                                for v in &mut path.vertices {
                                    v.xy_position = v.xy_position + o;
                                }
                                path.bounds.origin = path.bounds.origin + o;
                                if VECTOR_EXPORT.with(|v| v.get()) {
                                    path.outline = path.outline.map(|outline| std::sync::Arc::new(outline.translated(o)));
                                }
                                match clip {
                                    Some(bounds) => window.with_content_mask(Some(ContentMask { bounds }), |window| window.paint_path(path, *bg)),
                                    None => window.paint_path(path, *bg),
                                }
                            }
                        }
                    }
                },
            )
            .absolute()
            .size_full(),
        );
    let rotated_titles = axis_title_rotated(chart, plot, px_per_point, res);
    let container = container
        .children(labels.into_iter().chain(title_label(chart, w, h, px_per_point, res)).chain(axis_title_labels(chart, plot, w, h, px_per_point, res)).filter_map(|l| label_element(l, px_per_point, res)))
        .when(!rotated.is_empty() || !rotated_titles.is_empty(), |d| {
            let mut all_rotated = rotated;
            all_rotated.extend(rotated_titles);
            d.child(crate::render::rotated_text::rotated_lines(all_rotated))
        });
    match &chart.legend {
        Some(legend) => container.child(legend_element(chart, legend, w, h, px_per_point, res)),
        None => container,
    }
}

/// A box that paints `marks` (relative to its top left corner), as the chart's own canvas does.
fn marks_canvas(marks: Vec<Mark>) -> Div {
    div().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                if !bounds.intersects(&window.content_mask().bounds) {
                    return;
                }
                let o = bounds.origin;
                let paths = cached_paths(&marks, window.scale_factor());
                for item in paths.iter() {
                    if let Painted::Path(path, bg) = item {
                        let mut path = path.clone();
                        for v in &mut path.vertices {
                            v.xy_position = v.xy_position + o;
                        }
                        path.bounds.origin = path.bounds.origin + o;
                        if VECTOR_EXPORT.with(|v| v.get()) {
                            path.outline = path.outline.map(|outline| std::sync::Arc::new(outline.translated(o)));
                        }
                        window.paint_path(path, *bg);
                    }
                }
            },
        )
        .size_full(),
    )
}

/// What `chart_element` laid out, in pixels of the frame: for the
/// scenarios, which set `LAID_OUT` to `Some` to collect it.
#[cfg(all(target_os = "macos", feature = "scenarios"))]
#[derive(Clone, Debug, Default)]
pub(crate) struct LaidOut {
    pub(crate) size: (f32, f32),
    /// Left, top, width, height.
    pub(crate) plot: (f32, f32, f32, f32),
    /// Each text, where it is placed, and its turn (clockwise, degrees).
    pub(crate) texts: Vec<(String, (f32, f32), f32)>,
    /// Each filled area's bounds (left, top, right, bottom) and fill.
    pub(crate) fills: Vec<((f32, f32, f32, f32), Background)>,
}

#[cfg(all(target_os = "macos", feature = "scenarios"))]
thread_local! {
    pub(crate) static LAID_OUT: std::cell::RefCell<Option<Vec<LaidOut>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(all(target_os = "macos", feature = "scenarios"))]
#[allow(clippy::too_many_arguments)]
fn laid_out(chart: &Chart, size: (f32, f32), plot: Area, marks: &[Mark], labels: &[Label], rotated: &[crate::render::rotated_text::RotatedLine], px_per_point: f32, res: &Resources) -> LaidOut {
    let fills = marks
        .iter()
        .filter_map(|m| match m {
            Mark::Fill(points, bg) if !points.is_empty() => {
                let (x0, x1) = points.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
                let (y0, y1) = points.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
                Some(((x0, y0, x1, y1), *bg))
            }
            _ => None,
        })
        .collect();
    let title = title_label(chart, size.0, size.1, px_per_point, res);
    let upright = labels.iter().chain(title.as_ref()).map(|l| (l.text.clone(), (l.x, l.y), 0.0));
    let titles = axis_title_labels(chart, plot, size.0, size.1, px_per_point, res).into_iter().map(|l| (l.text, (l.x, l.y), 0.0));
    let turned_titles = axis_title_rotated(chart, plot, px_per_point, res);
    let turned = rotated.iter().chain(&turned_titles).map(|r| (r.text.clone(), r.at, r.degrees));
    LaidOut { size, plot: (plot.x, plot.y, plot.w, plot.h), texts: upright.chain(titles).chain(turned).collect(), fills }
}

/// A mark made ready to paint: its path relative to the chart, or a change
/// of the clip.
enum Painted {
    Path(Path<Pixels>, Background),
    Clip(Option<(f32, f32, f32, f32)>),
}

thread_local! {
    /// Set while exporting a vector PDF: the cached paths' outlines are then
    /// moved with them, for the PDF (moving them costs time on screen).
    pub(crate) static VECTOR_EXPORT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Painted marks of the charts drawn lately, by a hash of their marks and
    /// the screen's density: GPUI draws the whole window on every scroll.
    static PAINTED: std::cell::RefCell<Vec<(u64, usize, std::rc::Rc<Vec<Painted>>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// How many charts' painted marks to keep (a sheet full of charts).
const CACHED_CHARTS: usize = 64;

/// How many points of marks to keep painted in all (a path costs about 400 bytes a point: a chart of a million points kept
/// for each of the 64 would be 25 GB). The charts drawn last are kept, the one just made at least.
const CACHED_POINTS: usize = 400_000;

/// `marks` as paths relative to the chart, from the cache when drawn before.
fn cached_paths(marks: &[Mark], scale_factor: f32) -> std::rc::Rc<Vec<Painted>> {
    let key = marks_hash(marks, scale_factor);
    if let Some(found) = PAINTED.with(|c| c.borrow().iter().find(|(k, _, _)| *k == key).map(|(_, _, p)| p.clone())) {
        return found;
    }
    let to_px = |pts: &[(f32, f32)]| pts.iter().map(|(x, y)| point(px(*x), px(*y))).collect::<Vec<_>>();
    let painted: Vec<Painted> = marks
        .iter()
        .filter_map(|mark| match mark {
            Mark::Fill(pts, bg) => {
                let pts = to_px(pts);
                band_path(&pts).or_else(|| polygon(&pts, None, true)).map(|p| Painted::Path(p, *bg))
            }
            // At least a pixel of the screen (a thin line is a pixel wide, as
            // PowerPoint draws it).
            Mark::Stroke(pts, width, color) => polygon(&to_px(pts), Some(px(width.max(1.0 / scale_factor))), false).map(|p| Painted::Path(p, (*color).into())),
            Mark::Dashed(pts, width, color, pattern) => {
                crate::render::shape_view::dashed_line_path(&to_px(pts), px(width.max(1.0 / scale_factor)), pattern, 0.0).map(|p| Painted::Path(p, (*color).into()))
            }
            Mark::Clip(r) => Some(Painted::Clip(*r)),
        })
        .collect();
    let painted = std::rc::Rc::new(painted);
    let weight: usize = marks.iter().map(|m| match m { Mark::Fill(p, _) | Mark::Stroke(p, _, _) => p.len(), Mark::Dashed(p, _, _, _) => p.len(), Mark::Clip(_) => 1 }).sum();
    PAINTED.with(|c| {
        let mut c = c.borrow_mut();
        while !c.is_empty() && (c.len() >= CACHED_CHARTS || c.iter().map(|(_, w, _)| *w).sum::<usize>() + weight > CACHED_POINTS) {
            c.remove(0);
        }
        c.push((key, weight, painted.clone()));
    });
    painted
}

/// A band between two lines over the same places along the axis (a
/// stacked area: the upper line one way, then the lower one back), as two
/// triangles a step instead of tessellated (which takes long for thousands
/// of points); `None` for another shape, or where the lines cross.
fn band_path(points: &[Point<Pixels>]) -> Option<Path<Pixels>> {
    let n = points.len() / 2;
    if n < 2 || points.len() != 2 * n {
        return None;
    }
    let (upper, lower) = points.split_at(n);
    let below = |i: usize| lower[n - 1 - i];
    // Along x (columns) or y (bars), in order; across, one side of the other.
    let along_x = (0..n).all(|i| upper[i].x == below(i).x);
    let along_y = !along_x && (0..n).all(|i| upper[i].y == below(i).y);
    let (along, across): (fn(Point<Pixels>) -> Pixels, fn(Point<Pixels>) -> Pixels) = match (along_x, along_y) {
        (true, _) => (|p| p.x, |p| p.y),
        (_, true) => (|p| p.y, |p| p.x),
        _ => return None,
    };
    let ascending = along(upper[n - 1]) >= along(upper[0]);
    let mut side = 0.0;
    for i in 0..n {
        if i > 0 && (along(upper[i]) >= along(upper[i - 1])) != ascending && along(upper[i]) != along(upper[i - 1]) {
            return None;
        }
        let d = f32::from(across(below(i)) - across(upper[i]));
        if d != 0.0 {
            if side == 0.0 {
                side = d.signum();
            } else if d.signum() != side {
                return None;
            }
        }
    }
    let mut path = Path::new(points[0]);
    let st = (point(0., 1.), point(0., 1.), point(0., 1.));
    // Triangles with no area (a series of zeros, where the lines meet) are
    // left out, as tessellating leaves them.
    let flat = |a: Point<Pixels>, b: Point<Pixels>, c: Point<Pixels>| f32::from(b.x - a.x) * f32::from(c.y - a.y) == f32::from(c.x - a.x) * f32::from(b.y - a.y);
    for i in 0..n - 1 {
        let (a, b, c, d) = (upper[i], upper[i + 1], below(i + 1), below(i));
        if !flat(a, b, c) {
            path.push_triangle((a, b, c), st);
        }
        if !flat(a, c, d) {
            path.push_triangle((a, c, d), st);
        }
    }
    if path.vertices.is_empty() {
        return None;
    }
    // The outline, for a vector PDF.
    let mut verbs = vec![OutlineVerb::MoveTo(points[0])];
    verbs.extend(points[1..].iter().map(|p| OutlineVerb::LineTo(*p)));
    verbs.push(OutlineVerb::Close);
    path.outline = Some(std::sync::Arc::new(PathOutline { verbs, stroke: None }));
    Some(path)
}

/// A hash of `marks` (their points, widths and colours) and the density.
fn marks_hash(marks: &[Mark], scale_factor: f32) -> u64 {
    // FNV-1a over the numbers' bits: quick for the many points of a chart.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut add = |v: u32| {
        h ^= v as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    add(scale_factor.to_bits());
    let points = |pts: &[(f32, f32)], add: &mut dyn FnMut(u32)| {
        add(pts.len() as u32);
        for (x, y) in pts {
            add(x.to_bits());
            add(y.to_bits());
        }
    };
    for mark in marks {
        match mark {
            Mark::Fill(pts, bg) => {
                add(0);
                points(pts, &mut add);
                format!("{bg:?}").bytes().for_each(|b| add(b as u32));
            }
            Mark::Stroke(pts, width, color) => {
                add(1);
                points(pts, &mut add);
                add(width.to_bits());
                [color.h, color.s, color.l, color.a].iter().for_each(|v| add(v.to_bits()));
            }
            Mark::Dashed(pts, width, color, pattern) => {
                add(3);
                points(pts, &mut add);
                add(width.to_bits());
                [color.h, color.s, color.l, color.a].iter().for_each(|v| add(v.to_bits()));
                add(pattern.len() as u32);
                pattern.iter().for_each(|l| add(l.to_bits()));
            }
            Mark::Clip(r) => {
                add(2);
                if let Some((x, y, w, h)) = r {
                    [x, y, w, h].iter().for_each(|v| add(v.to_bits()));
                }
            }
        }
    }
    h
}

/// The chart with its text made smaller where a substitute font stands in
/// for a typeface that is not installed (see `Resources::substitute_scale`),
/// or `None` when no text needs it.
fn fit_substitutes(chart: &Chart, res: &Resources) -> Option<Chart> {
    // Each text is judged by the font its typical text is drawn in: the
    // category names, the series names, the title; digits for values.
    let factor = |t: &ChartText, sample: &str| res.substitute_scale_for(t.font_for(sample).as_deref(), &chart_font(t, sample, res), sample);
    // The longest of them stands for the lot (a typeface of Japanese letters
    // is judged by its widest text).
    let category = chart.groups.iter().flat_map(|g| &g.series).flat_map(|s| s.categories.iter()).max_by_key(|c| c.chars().count()).cloned().unwrap_or_else(|| "0".into());
    let series_name = chart.groups.iter().flat_map(|g| &g.series).map(|s| s.name.clone()).max_by_key(|n| n.chars().count()).unwrap_or_default();
    let axis_sample = |a: &Axis| if a.kind == AxisKind::Value { "0".to_string() } else { category.clone() };
    let title = chart.title.as_ref().map(|t| t.text.clone()).unwrap_or_default();
    let needed = chart.axes.iter().any(|a| factor(&a.text, &axis_sample(a)) < 1.0)
        || chart.legend.as_ref().is_some_and(|l| factor(&l.text, &series_name) < 1.0)
        || chart.title.as_ref().is_some_and(|t| factor(&t.style, &title) < 1.0)
        || chart.groups.iter().flat_map(|g| &g.series).flat_map(|s| &s.labels).any(|l| factor(&l.style, "0") < 1.0);
    if !needed {
        return None;
    }
    let mut chart = chart.clone();
    let fit = |t: &mut ChartText, sample: &str| t.size = (t.size as f32 * factor(t, sample)).round() as u32;
    for a in &mut chart.axes {
        let sample = axis_sample(a);
        fit(&mut a.text, &sample);
    }
    if let Some(l) = &mut chart.legend {
        fit(&mut l.text, &series_name);
    }
    if let Some(t) = &mut chart.title {
        fit(&mut t.style, &title);
    }
    for series in chart.groups.iter_mut().flat_map(|g| &mut g.series) {
        if let Some(labels) = &mut series.labels {
            fit(&mut labels.style, "0");
            labels.points.iter_mut().filter_map(|p| p.style.as_mut()).for_each(|t| fit(t, "0"));
        }
    }
    Some(chart)
}

/// A spreadsheet date (days since 1899-12-30) counted in `unit`s: the day
/// itself, or the month or year it falls in.
fn time_index(serial: f64, unit: ibis_doc_model::TimeUnit) -> f64 {
    use ibis_doc_model::TimeUnit;
    match unit {
        TimeUnit::Days => serial.floor(),
        TimeUnit::Months => {
            let (y, m, _) = civil(serial);
            (y * 12 + m - 1) as f64
        }
        TimeUnit::Years => civil(serial).0 as f64,
    }
}

/// How far apart a date axis's labels are, in pixels: its major unit in
/// base units times a base unit's width (or a line and a quarter apart when
/// automatic).
fn time_labels_spacing(time: Option<&(&ibis_doc_model::TimeScale, Vec<Option<f64>>, f64, f64)>, _axis: &Axis, band_len: f32, line: f32) -> f32 {
    let Some((t, ..)) = time else { return band_len };
    use ibis_doc_model::TimeUnit;
    match t.major {
        Some((n, unit)) => {
            let per = match (t.base, unit) {
                (b, u) if b == u => 1.0,
                (TimeUnit::Days, TimeUnit::Months) => 30.44,
                (TimeUnit::Days, TimeUnit::Years) => 365.25,
                (TimeUnit::Months, TimeUnit::Years) => 12.0,
                _ => 1.0,
            };
            band_len * (n.0 as f32 * per).max(1.0)
        }
        None => (line * 1.25 / band_len).ceil().max(1.0) * band_len,
    }
}

/// The major unit Excel picks for a date axis whose unit is automatic, and
/// how its labels turn: the first of 1, 2, 3 or 7 days, 1, 2, 3 or 6
/// months, 1, 2, 5 or 10 years (no finer than the base unit) whose labels
/// fit level, else slanted (45°), else upright; a base unit is `band_len`
/// wide, a label `widest` wide and `line` high. (Days a line apart turned
/// upright over two and a half years became months slanted in Excel: the
/// a daily chart of a user's private workbook; months upright stayed so:
/// a monthly chart of another.)
fn auto_time_step(t: &ibis_doc_model::TimeScale, band_len: f32, widest: f32, line: f32) -> (f64, ibis_doc_model::TimeUnit, i32) {
    use ibis_doc_model::TimeUnit;
    let per = |unit: TimeUnit| match (t.base, unit) {
        (b, u) if b == u => 1.0,
        (TimeUnit::Days, TimeUnit::Months) => 30.44,
        (TimeUnit::Days, TimeUnit::Years) => 365.25,
        (TimeUnit::Months, TimeUnit::Years) => 12.0,
        _ => 1.0,
    };
    let steps: &[(f64, TimeUnit)] = &[
        (1.0, TimeUnit::Days),
        (2.0, TimeUnit::Days),
        (3.0, TimeUnit::Days),
        (7.0, TimeUnit::Days),
        (14.0, TimeUnit::Days),
        (1.0, TimeUnit::Months),
        (2.0, TimeUnit::Months),
        (3.0, TimeUnit::Months),
        (4.0, TimeUnit::Months),
        (6.0, TimeUnit::Months),
        (1.0, TimeUnit::Years),
        (2.0, TimeUnit::Years),
        (5.0, TimeUnit::Years),
        (10.0, TimeUnit::Years),
        (20.0, TimeUnit::Years),
    ];
    let finer = |u: TimeUnit| matches!((t.base, u), (TimeUnit::Months, TimeUnit::Days) | (TimeUnit::Years, TimeUnit::Days | TimeUnit::Months));
    for &(n, unit) in steps.iter().filter(|(_, u)| !finer(*u)) {
        let spacing = band_len * n as f32 * per(unit);
        if widest <= spacing * 0.95 {
            return (n, unit, 0);
        }
        // Excel turns date axis labels 45° when spaced enough for slanted reading.
        if spacing >= line * 1.5 && spacing >= 24.0 {
            return (n, unit, -2_700_000);
        }
        if line * 1.25 <= spacing {
            return (n, unit, -5_400_000);
        }
    }
    (10.0, TimeUnit::Years, -2_700_000)
}

/// The widest date label of a date axis, in pixels.
fn time_scale_label_widest(t: &ibis_doc_model::TimeScale, axis: &Axis, px_per_point: f32) -> f32 {
    t.dates.iter().flatten().take(64).map(|d| text_width(&ibis_doc_model::format_number(d.0, &axis.format), &axis.text, px_per_point)).fold(0.0, f32::max)
}

/// The widest date label of a date axis, in pixels.
fn time_label_widest(time: Option<&(&ibis_doc_model::TimeScale, Vec<Option<f64>>, f64, f64)>, axis: &Axis, px_per_point: f32) -> f32 {
    let Some((t, ..)) = time else { return 0.0 };
    time_scale_label_widest(t, axis, px_per_point)
}

/// The first day of the unit the date `serial` falls in.
fn unit_start(serial: f64, unit: ibis_doc_model::TimeUnit) -> f64 {
    use ibis_doc_model::TimeUnit;
    let (y, m, _) = civil(serial);
    match unit {
        TimeUnit::Days => serial.floor(),
        TimeUnit::Months => serial_of(y, m, 1),
        TimeUnit::Years => serial_of(y, 1, 1),
    }
}

/// `count` `unit`s after the date `serial` (months and years keep the day
/// of the month, as far as the month has it).
fn add_time(serial: f64, count: f64, unit: ibis_doc_model::TimeUnit) -> f64 {
    use ibis_doc_model::TimeUnit;
    let months = match unit {
        TimeUnit::Days => return serial + count,
        TimeUnit::Months => count as i64,
        TimeUnit::Years => count as i64 * 12,
    };
    let (y, m, d) = civil(serial);
    let total = y * 12 + (m - 1) + months;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let last = days_in_month(y, m);
    serial_of(y, m, d.min(last))
}

fn days_in_month(y: i64, m: i64) -> i64 {
    serial_of(if m == 12 { y + 1 } else { y }, if m == 12 { 1 } else { m + 1 }, 1) as i64 - serial_of(y, m, 1) as i64
}

/// Year, month and day of a spreadsheet date.
fn civil(serial: f64) -> (i64, i64, i64) {
    // Days since 1970-01-01, then the civil-from-days algorithm.
    let z = serial.floor() as i64 - 25_569 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

/// The spreadsheet date of a year, month and day.
fn serial_of(y: i64, m: i64, d: i64) -> f64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468 + 25_569) as f64
}

/// Padding of the chart area, in points.
const CHART_PADDING_PT: f32 = 7.0;

/// The gap between a value axis's labels and the chart's edge, in the text's
/// size, as Excel's engine lays it out (a worksheet's and Word's charts);
/// PowerPoint's is 0.8. Chosen by the mean difference of XCHT (221 charts of
/// Excel for Mac's PDF): gap 0.2 0.0963, 0.4 0.0928, 0.5 0.0918, 0.6 0.0930,
/// against 0.1019 with 0.8. Not for Word's (DCHT got worse) nor for stock and
/// surface charts and combinations of kinds (XCHTX got worse).
const EXCEL_VALUE_LABEL_GAP: f32 = 0.5;

/// The padding of a chart read from a chartEx part: Excel keeps less of it
/// (the labels of its value axis stand 2.6 pt from the chart's edge).
fn chart_padding_pt(chart: &Chart) -> f32 {
    // (Tried 2.6 pt for chartEx charts: the XCHTE ratios got worse, 0.150 to 0.161.)
    let _ = chart;
    CHART_PADDING_PT
}

/// The plot area when a manual layout leaves too little room for the value
/// axes' labels.
///
/// PowerPoint keeps a manual plot layout only while the labels beside it
/// fit; otherwise it lays the plot out horizontally between the labels and
/// keeps the vertical position (found on the test deck: every chart whose
/// right-hand labels did not fit the space the layout left was drawn this
/// way, the others exactly as laid out).
/// The plot area PowerPoint lays out itself (measured against its PDF
/// output). With nothing around it (no legend, no axis labels) it is inset
/// about 11.5 pt from the frame on every side. With axis labels it is the
/// frame less the chart's padding, the value labels at the sides with a gap
/// of their size, half a line above for the top label, and the category
/// labels below (their length when crowded labels turn upright).
fn automatic_plot(chart: &Chart, w: f32, h: f32, px_per_point: f32, res: &Resources) -> Area {
    let labelled = chart.axes.iter().any(|a| !a.deleted && a.show_labels);
    let round = chart.groups.iter().any(is_round);
    let radar = chart.groups.iter().any(is_radar);
    let horizontal_bars = chart.groups.iter().any(|g| matches!(g.kind, ChartType::Bar { horizontal: true, .. }));
    let has_title = chart.title.as_ref().is_some_and(|t| !chart.title_overlay && t.at.is_none_or(|(_, fy)| fy < 25_000));
    let title_band = |pad: f32| {
        chart.title.as_ref().filter(|t| !chart.title_overlay && t.at.is_none_or(|(_, fy)| fy < 25_000)).map_or(0.0, |t| {
            let lines = wrapped_title_text(t, w, px_per_point, res).lines().count().max(1) as f32;
            let text_h = lines * t.style.size as f32 / 100.0 * px_per_point * 1.2;
            match t.at {
                Some((_, fy)) => fy as f32 / 100_000.0 * h + text_h,
                None => text_h + pad * 0.5,
            }
        })
    };
    let pie_outside_labels = chart.groups.iter().filter(|g| matches!(g.kind, ChartType::Pie { .. })).flat_map(|g| &g.series).any(|s| {
        s.labels.as_ref().is_some_and(|l| {
            (l.show_value || l.show_category || l.show_series_name || l.show_percent)
                && matches!(l.position, None | Some(LabelPosition::OutsideEnd | LabelPosition::BestFit))
        })
    });
    if (round || radar) && chart.legend.is_none() && !labelled && !has_title && !pie_outside_labels {
        // A pie or radar alone is inset like a plot with nothing around it.
        let inset = 11.5 * px_per_point;
        return Area { x: inset, y: inset, w: (w - 2.0 * inset).max(1.0), h: (h - 2.0 * inset).max(1.0) };
    }
    // A legend above or below, laid out automatically, takes a line of its
    // text and the padding from that side.
    let legend_band = match &chart.legend {
        None => Some((0.0, 0.0)),
        Some(l) if l.layout.is_none() && matches!(l.side, Side::Top | Side::Bottom) => {
            // A line and a half of its text, and a line's gap to the axis
            // labels (measured on PCHT: the legend's middle 13 pt and the
            // plot's bottom 43 pt above the frame's bottom, at 8 pt).
            let size = l.text.size as f32 / 100.0 * px_per_point;
            let band = size * 1.5 * legend_lines(chart, l, w, px_per_point, res) + size * 1.0;
            Some(if l.side == Side::Top { (band, 0.0) } else { (0.0, band) })
        }
        Some(l) if l.layout.is_none() => Some((0.0, 0.0)),
        // A side legend moved by hand: the plot is laid out as with one at
        // the side, kept off the legend's own place (legend_right below).
        Some(l) if l.overlay != Some(true) && matches!(l.side, Side::Left | Side::Right) => Some((0.0, 0.0)),
        // A legend above or below moved by hand, not over the plot: the plot is laid out as with none
        // (it keeps off the legend's own place only by the chart's padding), so the axis labels' room
        // is still looked at.
        Some(l) if l.overlay != Some(true) && matches!(l.side, Side::Top | Side::Bottom) => {
            // ... but not under the legend's own place: the plot ends before it (a gap of the padding).
            let gap = CHART_PADDING_PT * px_per_point;
            Some(match l.layout {
                Some(r) if l.side == Side::Top => (((r.y + r.height) as f32 / 100_000.0 * h + gap).clamp(0.0, h * 0.5), 0.0),
                Some(r) => (0.0, ((1.0 - r.y as f32 / 100_000.0) * h + gap).clamp(0.0, h * 0.5)),
                None => (0.0, 0.0),
            })
        }
        Some(_) => None,
    };
    // A legend at a side takes its width and a gap from that side.
    let (legend_left, legend_right) = match &chart.legend {
        Some(l) if l.layout.is_none() && l.overlay != Some(true) && matches!(l.side, Side::Left | Side::Right) => {
            let area = side_legend_area(chart, l, w, h, px_per_point, res);
            let taken = area.w + CHART_PADDING_PT * px_per_point;
            if l.side == Side::Left { (taken, 0.0) } else { (0.0, taken) }
        }
        // A legend moved by hand (manualLayout) beside the plot, not over it,
        // keeps the plot off its own place: the plot ends before it (Excel's
        // automatic plot does not run under it; the user's chart with a right
        // legend at x = 0.849 had its bars under the legend).
        Some(l) if l.overlay != Some(true) && matches!(l.side, Side::Left | Side::Right) && l.layout.is_some() => {
            let r = l.layout.unwrap();
            let (x, rw) = (r.x as f32 / 100_000.0, r.width as f32 / 100_000.0);
            let gap = CHART_PADDING_PT * px_per_point;
            if l.side == Side::Left {
                (((x + rw) * w + gap).clamp(0.0, w * 0.5), 0.0)
            } else {
                (0.0, ((1.0 - x) * w + gap).clamp(0.0, w * 0.5))
            }
        }
        _ => (0.0, 0.0),
    };
    // No axis labels but a legend above or below: the plot is inset like one with nothing around it and
    // stops short of the legend's room (measured against Word with DCHT-00187, 00200).
    if let Some((legend_top, legend_bottom)) = legend_band
        && !round
        && !radar
        && !horizontal_bars
        && chart.legend.is_some()
        && !labelled
    {
        let inset = 11.5 * px_per_point;
        return Area { x: inset, y: inset + legend_top, w: (w - 2.0 * inset).max(1.0), h: (h - 2.0 * inset - legend_top - legend_bottom).max(1.0) };
    }
    if let Some((legend_top, legend_bottom)) = legend_band
        && !round
        && !radar
        && !horizontal_bars
        && (chart.legend.is_none() || labelled)
    {
        if !labelled {
            // A legend above or below keeps its room (measured against Word with DCHT-00187, 00200: the
            // plot stops above the legend).
            let inset = 11.5 * px_per_point;
            return Area { x: inset, y: inset + legend_top, w: (w - 2.0 * inset).max(1.0), h: (h - 2.0 * inset - legend_top - legend_bottom).max(1.0) };
        }
        let pad = chart_padding_pt(chart) * px_per_point;
        let excel_gap = chart.excel_layout
            && !chart.groups.iter().any(|g| matches!(g.kind, ChartType::Stock { .. } | ChartType::Surface { .. }))
            // Labels on the far side (`tickLblPos high`) kept 0.8: XCHT-00184 got worse.
            && !chart.axes.iter().any(|a| a.labels_high)
            // A combination of kinds (columns and a line) kept 0.8: XCHTX got worse.
            && chart.groups.iter().all(|g| std::mem::discriminant(&g.kind) == std::mem::discriminant(&chart.groups[0].kind));
        let (mut left, right) = value_label_widths(chart, px_per_point, if excel_gap { EXCEL_VALUE_LABEL_GAP } else { 0.8 });
        // Limited font-advance probe: keep the preliminary tick set and gap.
        let axes: Vec<&Axis> = chart.axes.iter().filter(|a| a.kind == AxisKind::Value && !a.deleted).collect();
        if chart.legend.as_ref().is_some_and(|l| l.layout.is_none()
            && l.source_position == Some(ibis_doc_model::LegendPosition::TopRight)
            && l.overlay == Some(true))
            && axes.len() == 1
            && axes[0].side == Side::Left && !axes[0].labels_high && axes[0].show_labels
        {
            let axis = axes[0];
            let (low, high) = value_range(chart, axis.id);
            let unit = axis.display_unit.map(|u| u.0).unwrap_or(1.0);
            let size = axis.text.size as f32 / 100.0 * px_per_point;
            let is_percent = percent_axis(chart, axis.id);
            let format = if axis.format == "General" && is_percent { "0%" } else { axis.format.as_str() };
            let intervals = if axis.major_unit.is_some() { 10.0 } else { 5.0 };
            let mut scale = Scale::fit(low, high, Some(axis), intervals);
            if is_percent && axis.max.is_none() && high <= 1.0 {
                scale.max = 1.0;
            }
            left = scale.ticks().into_iter().map(|tick| {
                let text = format_number(tick / unit, format);
                res.text_width(&text, &chart_font(&axis.text, &text, res), size)
            }).fold(0.0, f32::max) + size * 0.8;
        }
        let value_size = chart
            .axes
            .iter()
            .filter(|a| a.kind == AxisKind::Value && !a.deleted && a.show_labels)
            .map(|a| a.text.size as f32 / 100.0 * px_per_point)
            .fold(0.0, f32::max);
        let is_column_bar = chart.groups.iter().any(|g| matches!(g.kind, ChartType::Bar { horizontal: false, .. } | ChartType::Stock { .. }));
        let is_scatter_or_bubble = chart.groups.iter().any(|g| is_scatter(g) || matches!(g.kind, ChartType::Bubble { .. }));
        let left_axis_title_pad = chart.axes.iter().find(|a| !a.deleted && a.title.is_some() && a.side == Side::Left).map_or(0.0, |a| {
            a.title.as_ref().map_or(0.0, |t| t.style.size as f32 / 100.0 * px_per_point * 1.5 + 4.0 * px_per_point)
        });
        let right_axis_title_pad = chart.axes.iter().find(|a| !a.deleted && a.title.is_some() && a.side == Side::Right).map_or(0.0, |a| {
            a.title.as_ref().map_or(0.0, |t| t.style.size as f32 / 100.0 * px_per_point * 1.5 + 4.0 * px_per_point)
        });
        let x = pad + left + legend_left + left_axis_title_pad;
        let is_line_or_area = chart.groups.iter().any(|g| matches!(g.kind, ChartType::Line | ChartType::Area));
        let right_pad = pad + if (is_column_bar || is_scatter_or_bubble || is_line_or_area) && right == 0.0 && legend_right == 0.0 { 4.0 * px_per_point } else { 0.0 };
        let plot_w = (w - x - right_pad - right - legend_right - right_axis_title_pad).max(w * 0.2);
        // A title at the top pushes the plot down by its lines.
        let title = title_band(pad);
        let top_factor = if is_column_bar { 0.85 } else { 0.35 };
        let mut top = pad + value_size * top_factor + legend_top + title;
        // The category labels of an axis at the top take their room there.
        let mut labels_on_top = 0.0f32;
        let below = legend_bottom + (if legend_bottom > 0.0 { 3.0 * px_per_point } else { 0.0 }) + match chart.axes.iter().find(|a| a.kind != AxisKind::Value && !a.deleted && a.show_labels) {
            Some(axis) => {
                let size = axis.text.size as f32 / 100.0 * px_per_point;
                let names: Vec<String> = chart.groups.iter().flat_map(|g| &g.series).map(|s| s.categories.clone()).find(|c| !c.is_empty()).unwrap_or_default();
                let count = category_count(chart).max(1);
                let widest = match &axis.time {
                    Some(t) => time_scale_label_widest(t, axis, px_per_point),
                    None => names.iter().map(|n| text_width(n, &axis.text, px_per_point)).fold(0.0, f32::max),
                };
                let upright = axis.label_rotation.abs() >= 2_700_000
                    || (axis.label_rotation == 0 && !axis.stagger_labels && !axis.fixed_label_rotation && axis.category_levels.is_empty() && widest > plot_w / count as f32);
                // A date axis's labels start below its tick marks and a gap
                // (Excel; a private workbook's chart's reach its legend
                // otherwise).
                let gap = if axis.ticks_outside { 3.75 * px_per_point } else { 0.0 };
                // Staggered labels stand in two rows when they do not fit in one.
                let wrapped_rows = if (axis.fixed_label_rotation || !axis.category_levels.is_empty()) && !axis.stagger_labels && widest > plot_w / count as f32 {
                    2.2
                } else if axis.stagger_labels && widest > plot_w / count as f32 * 1.08 {
                    2.2
                } else {
                    1.2
                };
                let rows = wrapped_rows;
                let label_depth = if upright {
                    if axis.label_rotation.abs() == 2_700_000 || (axis.time.is_some() && axis.label_rotation == 0) {
                        (widest * 0.75 + size * 0.5).max(size * rows)
                    } else {
                        widest
                    }
                } else {
                    size * rows
                };
                let band = gap + label_depth + size * 0.3 + axis.category_levels.len() as f32 * level_row_height(axis, px_per_point);
                if axis.side == Side::Top {
                    // The labels stand in the margin above the plot, which is made up for by the room
                    // left below (measured against PowerPoint with PCHT: 15 and 11 px of the 31 px band).
                    labels_on_top = band * 0.5;
                    band * 0.35
                } else {
                    band
                }
            }
            // A scatter's level value axis: a line of its labels below.
            None => chart.axes.iter().find(|a| a.kind == AxisKind::Value && !a.deleted && a.show_labels && a.side == Side::Bottom).map_or(0.0, |axis| {
                let size = axis.text.size as f32 / 100.0 * px_per_point;
                let ticks = if axis.ticks_outside { 3.75 * px_per_point } else { 0.0 };
                ticks + size * 1.7
            }),
        };
        let bottom_axis_title_pad = chart.axes.iter().find(|a| !a.deleted && a.title.is_some() && a.side == Side::Bottom).map_or(0.0, |a| {
            a.title.as_ref().map_or(0.0, |t| t.style.size as f32 / 100.0 * px_per_point * 1.5 + 4.0 * px_per_point)
        });
        let below = below + bottom_axis_title_pad;
        top += labels_on_top;
        return Area { x, y: top, w: plot_w, h: (h - top - pad - below).max(h * 0.2) };
    }
    // Horizontal bars: the category labels at the left, the value labels
    // below, the legend band below them (measured on PCHT's bar charts).
    if let Some((legend_top, legend_bottom)) = legend_band
        && horizontal_bars
        && !round
        && labelled
    {
        let pad = chart_padding_pt(chart) * px_per_point;
        let category = chart.axes.iter().find(|a| a.kind != AxisKind::Value && !a.deleted && a.show_labels);
        let names: Vec<String> = chart.groups.iter().flat_map(|g| &g.series).map(|s| s.categories.clone()).find(|c| !c.is_empty()).unwrap_or_default();
        let left = category.map_or(0.0, |axis| {
            let widest = names.iter().map(|n| text_width(n, &axis.text, px_per_point)).fold(0.0, f32::max);
            let ticks = if axis.ticks_outside { 3.75 * px_per_point } else { 0.0 };
            widest + ticks + axis.text.size as f32 / 100.0 * px_per_point * 0.5
        });
        let value = chart.axes.iter().find(|a| a.kind == AxisKind::Value && !a.deleted && a.show_labels);
        let value_band = value.map_or(0.0, |axis| {
            let size = axis.text.size as f32 / 100.0 * px_per_point;
            let ticks = if axis.ticks_outside { 3.75 * px_per_point } else { 0.0 };
            ticks + size * 1.7
        });
        // (Value labels at the top, over reversed categories, take their room there.)
        let on_top = value.is_some_and(|a| axis_label_side(a, true) == Side::Top);
        let (below, value_top) = if on_top { (0.0, value_band) } else { (value_band, 0.0) };
        let x = pad + left + legend_left;
        // A title at the top pushes the plot down by its lines.
        let title = title_band(pad);
        let top = pad + legend_top + title + value_top;
        // Half the last value label beyond the plot's right end.
        let right = pad + value.map_or(0.0, |a| a.text.size as f32 / 100.0 * px_per_point);
        let plot_w = (w - x - right - legend_right).max(w * 0.2);
        return Area { x, y: top, w: plot_w, h: (h - top - pad - below - legend_bottom).max(h * 0.2) };
    }
    if radar {
        let title = title_band(8.0 * px_per_point);
        let (lt, lb) = legend_band.unwrap_or((0.0, 0.0));
        let pad = chart_padding_pt(chart) * px_per_point;
        let left = legend_left + pad;
        let right = legend_right + pad;
        let top = title + lt + pad;
        let bottom = lb + pad;
        let plot_w = (w - left - right).max(w * 0.2);
        let plot_h = (h - top - bottom).max(h * 0.2);
        return Area { x: left, y: top, w: plot_w, h: plot_h };
    }
    // An automatic bottom legend leaves symmetric horizontal room for
    // a pie. Its centre is the chart frame's centre, not 52% across.
    let centered_round = (chart.groups.iter().all(|g| matches!(g.kind, ChartType::Pie { .. }))
        || chart.groups.iter().all(|g| matches!(g.kind, ChartType::Doughnut { .. }))
        || chart.groups.iter().all(|g| matches!(g.kind, ChartType::OfPie { .. })))
        && round && chart.legend.as_ref().is_none_or(|l| l.layout.is_none() && l.side == Side::Bottom);
    if has_title {
        let pad = CHART_PADDING_PT * px_per_point;
        let top = chart.title.as_ref().map_or(0.0, |t| {
            let lines = wrapped_title_text(t, w, px_per_point, res).lines().count().max(1) as f32;
            let line_h = t.style.size as f32 / 100.0 * px_per_point * 1.2;
            2.0 * pad + (lines + 1.0) * line_h
        });
        let bottom = legend_band.map_or(pad, |(_, lb)| if lb > 0.0 { lb + 2.5 * pad } else { pad });
        return Area {
            x: w * if centered_round { 0.08 } else { 0.1 },
            y: top,
            w: w * 0.84,
            h: (h - top - bottom).max(h * 0.2),
        };
    }
    // Keep the existing fractional inset, but reserve at least 2.7 text
    // sizes for the bottom legend. Native 8pt legends leave about 21.67pt
    // on both 140.16pt presentation and 108pt spreadsheet frames.
    let size = chart.legend.as_ref().map_or(0.0, |l| l.text.size as f32 / 100.0 * px_per_point);
    let legend_lift = if centered_round && chart.legend.is_some() {
        (size * 0.3).max(size * 1.35 - h * 0.06)
    } else { 0.0 };
    let top = h * 0.08 - legend_lift;
    Area { x: w * if centered_round { 0.08 } else { 0.1 }, y: top, w: w * 0.84, h: (h * 0.72).max(h * 0.2) }
}

/// The room the value axis labels take at the left and the right of the
/// plot: the widest label and a gap of `gap` times the label size.
fn value_label_widths(chart: &Chart, px_per_point: f32, gap: f32) -> (f32, f32) {
    let value_axes: Vec<&Axis> = chart.axes.iter().filter(|a| a.kind == AxisKind::Value && !a.deleted).collect();
    let is_left = |a: &Axis| (a.side == Side::Left && !a.labels_high) || (a.side == Side::Right && a.labels_high);
    let is_right = |a: &Axis| (a.side == Side::Right && !a.labels_high) || (a.side == Side::Left && a.labels_high);
    let widest = |a: &Axis| -> f32 {
        if !a.show_labels {
            return 0.0;
        }
        let (low, high) = value_range(chart, a.id);
        let unit = a.display_unit.map(|u| u.0).unwrap_or(1.0);
        let size = a.text.size as f32 / 100.0 * px_per_point;
        let is_percent = percent_axis(chart, a.id);
        let format = if a.format == "General" && is_percent { "0%" } else { a.format.as_str() };
        // Value axes with automatic units typically divide into about 8 intervals when a bottom legend shortens the plot, and up to 10 on taller plots.
        let intervals = if a.major_unit.is_some() {
            10.0
        } else if chart.legend.as_ref().is_some_and(|l| l.side == Side::Bottom) {
            8.0
        } else {
            10.0
        };
        let mut scale = Scale::fit(low, high, Some(a), intervals);
        if is_percent && a.max.is_none() && high <= 1.0 {
            scale.max = 1.0;
        }
        let labels = scale.ticks().into_iter().map(|t| number_width(&format_number(t / unit, format)) * size);
        labels.fold(0.0, f32::max) + a.text.size as f32 / 100.0 * px_per_point * gap
    };
    let left = value_axes.iter().filter(|a| is_left(a)).map(|a| widest(a)).fold(0.0, f32::max);
    let right = value_axes.iter().filter(|a| is_right(a)).map(|a| widest(a)).fold(0.0, f32::max);
    (left, right)
}

fn side_axes_layout(chart: &Chart, plot: Area, w: f32, px_per_point: f32) -> Area {
    let horizontal_bars = chart.groups.iter().any(|g| matches!(g.kind, ChartType::Bar { horizontal: true, .. }));
    let has_value_axis = chart.axes.iter().any(|a| a.kind == AxisKind::Value && !a.deleted);
    if horizontal_bars || chart.plot_area.is_none() || !has_value_axis {
        return plot;
    }
    // The labels, and a gap of half their size to the plot. They count as
    // fitting only with a gap of their whole size (measured against
    // PowerPoint's PDF output).
    let (left, right) = value_label_widths(chart, px_per_point, 0.5);
    let (fit_left, fit_right) = value_label_widths(chart, px_per_point, 1.0);
    let pad = CHART_PADDING_PT * px_per_point;
    if fit_left <= plot.x && fit_right <= w - plot.right() {
        return plot;
    }
    let x = (pad + left).max(plot.x);
    // When a legend stands at the right (not overlaid), do not expand the plot area's right edge
    // beyond its explicit plotArea layout, which was placed to leave room for the legend.
    let right_limit = if chart.legend.as_ref().is_some_and(|l| l.side == Side::Right && l.overlay != Some(true)) {
        plot.right()
    } else {
        let right_pad = if right == 0.0 && plot.right() > w { 0.75 * pad } else { pad };
        w - right_pad - right
    };
    let width = (right_limit - x).max(w * 0.2);
    Area { x, w: width, ..plot }
}

/// A plot area whose category labels turn is shortened from below until
/// the turned labels fit in the frame, keeping its top (measured against
/// PowerPoint's PDF output of a slide of a user's private presentation).
fn slanted_labels_layout(chart: &Chart, plot: Area, h: f32, px_per_point: f32) -> Area {
    let horizontal_bars = chart.groups.iter().any(|g| matches!(g.kind, ChartType::Bar { horizontal: true, .. }));
    let Some(axis) = chart.axes.iter().find(|a| a.kind == AxisKind::Category && !a.deleted && a.show_labels) else { return plot };
    if horizontal_bars || axis.label_rotation != 0 || axis.fixed_label_rotation || !axis.category_levels.is_empty() {
        return plot;
    }
    let names: Vec<String> = chart.groups.iter().flat_map(|g| &g.series).map(|s| s.categories.clone()).find(|c| !c.is_empty()).unwrap_or_default();
    let count = category_count(chart).max(1);
    let band = plot.w / count as f32 * axis.label_skip.max(1) as f32;
    let widest = names.iter().map(|n| text_width(n, &axis.text, px_per_point)).fold(0.0, f32::max);
    if widest <= band {
        return plot;
    }
    let line = text_height(&axis.text, px_per_point);
    // Only slanted labels: upright ones are drawn smaller to fit instead.
    if axis.date || crowded_rotation(&axis.text, band, px_per_point) != -2_700_000 {
        return plot;
    }
    let depth = (widest + line) * std::f32::consts::FRAC_1_SQRT_2;
    // PowerPoint's labels reach about a tenth further than this estimate
    // of them, and it keeps 3 pt clear below them (a private presentation's slide: the
    // zero line 9 px higher than with the estimate alone).
    let needed = 7.0 * px_per_point + depth * 1.1;
    let room = h - plot.bottom();
    if needed <= room {
        return plot;
    }
    Area { h: (plot.h - (needed - room)).max(plot.h * 0.5), ..plot }
}

/// How PowerPoint turns category labels too wide for their band `room`
/// pixels wide: 45° when a slanted line fits the band, else upright.
fn crowded_rotation(text: &ChartText, room: f32, px_per_point: f32) -> i32 {
    if text_height(text, px_per_point) * std::f32::consts::SQRT_2 <= room { -2_700_000 } else { -5_400_000 }
}

fn value_axis<'c>(chart: &'c Chart, group: &ChartGroup) -> Option<&'c Axis> {
    // A scatter's two value axes: the upright one is its y axis.
    let scatter = is_scatter(group);
    let is_val = |a: &&Axis| a.kind == AxisKind::Value && (!scatter || matches!(a.side, Side::Left | Side::Right));
    let own = group.axes.iter().filter_map(|id| chart.axis(*id)).find(is_val);
    if own.is_some_and(|a| a.deleted && a.min.is_none() && a.max.is_none())
        && let Some(primary) = chart.axes.iter().find(|a| is_val(&a) && !a.deleted)
    {
        return Some(primary);
    }
    own
}

/// A scatter's (or bubble chart's) x axis: its level value axis.
fn x_axis<'c>(chart: &'c Chart, group: &ChartGroup) -> Option<&'c Axis> {
    group.axes.iter().filter_map(|id| chart.axis(*id)).find(|a| a.kind == AxisKind::Value && matches!(a.side, Side::Top | Side::Bottom))
}

/// How many categories the chart's category axis has: the most values (or names) of a series
/// of the groups that stand on it. A scatter or bubble group in a chart with other groups stands on
/// its own axes, and its points are not categories.
fn category_count(chart: &Chart) -> usize {
    let others = chart.groups.iter().any(|g| !is_scatter(g));
    chart
        .groups
        .iter()
        .filter(|g| !others || !is_scatter(g))
        .flat_map(|g| &g.series)
        // PowerPoint gives every cell of the value range a category, even
        // when fewer category names are cached.
        .map(|s| s.values.len().max(s.categories.len()))
        .max()
        .unwrap_or(0)
}

fn is_scatter(group: &ChartGroup) -> bool {
    matches!(group.kind, ChartType::Scatter { .. } | ChartType::Bubble { .. })
}

/// A scatter series' x value at point `i` (1, 2, 3, … without numbers).
fn x_value(series: &Series, i: usize) -> f64 {
    series.x_values.get(i).copied().flatten().map_or(i as f64 + 1.0, |n| n.0)
}

/// Whether the category axis runs backwards (`maxMin`).
fn reversed_category_axis(chart: &Chart) -> bool {
    chart.axes.iter().find(|a| a.kind == AxisKind::Category).is_some_and(|a| a.reversed)
}

/// Whether value axis `id` is a 100% stacked group's.
fn percent_axis(chart: &Chart, id: u32) -> bool {
    chart.groups.iter().any(|g| g.grouping == Grouping::PercentStacked && value_axis(chart, g).map(|a| a.id).unwrap_or(0) == id)
}

/// Sum of each category's values in a group (for percent stacking and pie shares).
fn category_totals(group: &ChartGroup, categories: usize) -> Vec<f64> {
    (0..categories)
        .map(|ci| group.series.iter().filter_map(|s| s.values.get(ci).copied().flatten()).map(|n| n.0).sum())
        .collect()
}

/// The lowest and highest value drawn against axis `id`.
fn value_range(chart: &Chart, id: u32) -> (f64, f64) {
    let mut low = f64::MAX;
    let mut high = f64::MIN;
    for group in chart.groups.iter().filter(|g| value_axis(chart, g).map(|a| a.id).unwrap_or(0) == id) {
        let categories = group.series.iter().map(|s| s.values.len()).max().unwrap_or(0);
        match group.grouping {
            Grouping::PercentStacked => {
                low = low.min(0.0);
                high = high.max(1.0);
            }
            Grouping::Stacked => {
                for ci in 0..categories {
                    let values = group.series.iter().filter_map(|s| s.values.get(ci).copied().flatten()).map(|n| n.0);
                    let (pos, neg): (Vec<f64>, Vec<f64>) = values.partition(|v| *v >= 0.0);
                    high = high.max(pos.iter().sum());
                    low = low.min(neg.iter().sum());
                }
            }
            Grouping::Standard => {
                for s in &group.series {
                    let valid: Vec<f64> = s.values.iter().flatten().map(|n| n.0).collect();
                    let n = valid.len();
                    let mean = if n > 0 { valid.iter().sum::<f64>() / n as f64 } else { 0.0 };
                    let std_dev = if n > 1 {
                        (valid.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64).sqrt()
                    } else {
                        0.0
                    };
                    let std_err = if n > 0 { std_dev / (n as f64).sqrt() } else { 0.0 };
                    for (ci, v) in s.values.iter().enumerate() {
                        let Some(v) = v else { continue };
                        low = low.min(v.0);
                        high = high.max(v.0);
                        if let Some(eb) = &s.error_bars {
                            use ibis_doc_model::{ErrorBarType, ErrorBarValueType};
                            let (p, m) = match eb.val_type {
                                ErrorBarValueType::FixedValue => {
                                    let d = eb.value.map(|n| n.0).unwrap_or(1.0);
                                    (d, d)
                                }
                                ErrorBarValueType::Percentage => {
                                    let d = v.0.abs() * (eb.value.map(|n| n.0).unwrap_or(5.0) / 100.0);
                                    (d, d)
                                }
                                ErrorBarValueType::StandardDeviation => {
                                    let d = eb.value.map(|n| n.0).unwrap_or(1.0) * std_dev;
                                    (d, d)
                                }
                                ErrorBarValueType::StandardError => (std_err, std_err),
                                ErrorBarValueType::Custom => {
                                    let fb = eb.value.map(|n| n.0).unwrap_or(1.0);
                                    let p = if eb.plus.is_empty() { fb } else { eb.plus.get(ci).copied().flatten().map_or(0.0, |n| n.0) };
                                    let m = if eb.minus.is_empty() { fb } else { eb.minus.get(ci).copied().flatten().map_or(0.0, |n| n.0) };
                                    (p, m)
                                }
                            };
                            if matches!(eb.bar_type, ErrorBarType::Both | ErrorBarType::Plus) {
                                high = high.max(v.0 + p);
                            }
                            if matches!(eb.bar_type, ErrorBarType::Both | ErrorBarType::Minus) {
                                low = low.min(v.0 - m);
                            }
                        }
                    }
                }
            }
        }
    }
    if low > high { (0.0, 1.0) } else { (low, high) }
}

/// A smooth curve through `points` (Catmull-Rom, as Office's smoothed
/// lines pass through each point), as short straight pieces.
fn smooth_curve(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = Vec::with_capacity(points.len() * 12);
    for i in 0..points.len() - 1 {
        let p0 = points[i.saturating_sub(1)];
        let (p1, p2) = (points[i], points[i + 1]);
        let p3 = points[(i + 2).min(points.len() - 1)];
        for k in 0..12 {
            let t = k as f32 / 12.0;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3);
            out.push((f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1)));
        }
    }
    out.push(*points.last().unwrap());
    out
}

/// The marks of one data marker. The symbol the file names (`c:symbol`) decides: a star, an x, a plus, a
/// dash and a dot are drawn as Office does (thin strokes of the marker's colour, and two small bars); the
/// automatic one takes the series' turn in Office's order (a diamond, a square, a triangle, an x, a star, a
/// circle, a plus, a dash); the others are the filled shapes of [`marker_shape`].
fn marker_marks(m: &ibis_doc_model::Marker, series_index: usize, x: f32, y: f32, r: f32, color: Hsla, other: Option<Background>, px_per_point: f32) -> Vec<Mark> {
    use ibis_doc_model::MarkerSourceSymbol as S;
    const AUTO: [S; 8] = [S::Diamond, S::Square, S::Triangle, S::Cross, S::Star, S::Circle, S::Plus, S::Dash];
    let named = m.source.as_ref().and_then(|s| s.symbol);
    let named = match named {
        Some(S::Auto) => Some(AUTO[series_index % AUTO.len()]),
        other => other,
    };
    let line = (0.75 * px_per_point).max(1.0);
    let stroke = |pts: Vec<(f32, f32)>| Mark::Stroke(pts, line, color);
    let bar = |w: f32, h: f32| Mark::Fill(vec![(x - w, y - h), (x + w, y - h), (x + w, y + h), (x - w, y + h)], color.into());
    match named {
        Some(S::Cross) => vec![stroke(vec![(x - r, y - r), (x + r, y + r)]), stroke(vec![(x - r, y + r), (x + r, y - r)])],
        Some(S::Plus) => vec![stroke(vec![(x - r, y), (x + r, y)]), stroke(vec![(x, y - r), (x, y + r)])],
        Some(S::Star) => vec![stroke(vec![(x - r, y - r), (x + r, y + r)]), stroke(vec![(x - r, y + r), (x + r, y - r)]), stroke(vec![(x, y - r), (x, y + r)])],
        Some(S::Dash) => vec![bar(r, r * 0.25)],
        Some(S::Dot) => vec![bar(r * 0.6, r * 0.27)],
        Some(S::Diamond) => vec![Mark::Fill(marker_shape(MarkerSymbol::Diamond, x, y, r), color.into())],
        Some(S::Square) => vec![Mark::Fill(marker_shape(MarkerSymbol::Square, x, y, r), color.into())],
        Some(S::Triangle) => vec![Mark::Fill(marker_shape(MarkerSymbol::Triangle, x, y, r), color.into())],
        Some(S::Circle) => vec![Mark::Fill(marker_shape(MarkerSymbol::Circle, x, y, r), color.into())],
        _ => match other {
            Some(bg) => vec![Mark::Fill(marker_shape(m.symbol, x, y, r), bg)],
            None => Vec::new(),
        },
    }
}

fn marker_shape(symbol: MarkerSymbol, x: f32, y: f32, r: f32) -> Vec<(f32, f32)> {
    match symbol {
        MarkerSymbol::Square => vec![(x - r, y - r), (x + r, y - r), (x + r, y + r), (x - r, y + r)],
        MarkerSymbol::Diamond => vec![(x, y - r), (x + r, y), (x, y + r), (x - r, y)],
        MarkerSymbol::Triangle => vec![(x, y - r), (x + r, y + r), (x - r, y + r)],
        _ => (0..16).map(|i| {
            let t = i as f32 / 16.0 * std::f32::consts::TAU;
            (x + r * t.cos(), y + r * t.sin())
        })
        .collect(),
    }
}

/// A rough width of a number label, in ems: the typical widths of digits,
/// separators and signs. The labels' own font is often a theme font that
/// is not installed (游ゴシック), so it is not measured.
fn number_width(text: &str) -> f32 {
    text.chars()
        .map(|c| match c {
            '0'..='9' => 0.55,
            '.' | ',' => 0.3,
            '%' => 0.8,
            c if (c as u32) >= 0x2e80 => 1.0,
            _ => 0.6,
        })
        .sum()
}

/// A rough width of `text`: digits and Latin letters are about half as wide
/// as the font size, East Asian characters a full size.
fn text_width(text: &str, style: &ChartText, px_per_point: f32) -> f32 {
    let size = style.size as f32 / 100.0 * px_per_point;
    let longest = text.lines().map(|l| l.chars().map(|c| if (c as u32) < 0x2e80 { 0.6 } else { 1.0 }).sum::<f32>()).fold(0.0, f32::max);
    longest * size
}

fn text_height(style: &ChartText, px_per_point: f32) -> f32 {
    style.size as f32 / 100.0 * px_per_point * 1.2
}

/// Add the data label of point `index` of `series`, if it has one.
/// `place` maps a label position, the text's height and a width estimate to
/// the label's anchor point.
#[allow(clippy::too_many_arguments)]
fn push_label(
    labels: &mut Vec<Label>,
    series: &Series,
    index: usize,
    value: f64,
    total: f64,
    default: LabelPosition,
    // The size the label's offset is a fraction of, and the plot area.
    (move_w, move_h, plot): (f32, f32, Area),
    px_per_point: f32,
    res: &Resources,
    place: impl Fn(LabelPosition, f32, f32) -> (f32, f32, Anchor),
) {
    let Some(settings) = &series.labels else { return };
    let point = settings.points.iter().find(|p| p.index == index);
    if point.is_some_and(|p| p.deleted) {
        return;
    }
    let show_series_name = point.and_then(|p| p.show_series_name).unwrap_or(settings.show_series_name);
    let show_category = point.and_then(|p| p.show_category).unwrap_or(settings.show_category);
    let show_value = point.and_then(|p| p.show_value).unwrap_or(settings.show_value);
    let show_percent = point.and_then(|p| p.show_percent).unwrap_or(settings.show_percent);
    let piece_text = |piece: &LabelPiece| match piece {
        LabelPiece::Text(t) => t.clone(),
        // The labels' format is for the percentage when it is a percent
        // format and the label shows a percentage too: the value then keeps
        // the series' format.
        LabelPiece::Value => format_number(
            value,
            settings.format.as_deref().filter(|f| !(show_percent && f.contains('%'))).unwrap_or(&series.format),
        ),
        // A percentage takes the labels' own format when it is one (`0.0%`).
        LabelPiece::Percent => {
            let format = settings.format.as_deref().filter(|f| f.contains('%')).unwrap_or("0%");
            let share = if total != 0.0 { value / total } else { 0.0 };
            // Whole percentages are adjusted to add up to 100% (PowerPoint's PDF of five datasets; with decimals each is rounded alone).
            match whole_percentages(&series.values).filter(|_| percent_format_without_decimals(format)) {
                Some(whole) => whole.get(index).copied().flatten().map_or_else(|| format_number(share, format), |p| format_number(p as f64 / 100.0, format)),
                None => format_number(share, format),
            }
        }
        LabelPiece::Category => series.categories.get(index).cloned().unwrap_or_default(),
        LabelPiece::SeriesName => series.name.clone(),
    };
    let text = match point.and_then(|p| p.text.as_ref()) {
        Some(pieces) => pieces.iter().map(piece_text).collect::<String>(),
        None => {
            let mut parts = Vec::new();
            if show_series_name {
                parts.push(piece_text(&LabelPiece::SeriesName));
            }
            if show_category {
                parts.push(piece_text(&LabelPiece::Category));
            }
            if show_value {
                parts.push(piece_text(&LabelPiece::Value));
            }
            if show_percent {
                parts.push(piece_text(&LabelPiece::Percent));
            }
            if parts.is_empty() {
                return;
            }
            parts.join(settings.separator.as_deref().unwrap_or(", "))
        }
    };
    let style = point.and_then(|p| p.style.clone()).unwrap_or_else(|| settings.style.clone());
    let fill = point.and_then(|p| p.fill.clone()).unwrap_or_else(|| settings.fill.clone());
    let border = point.and_then(|p| p.border.clone()).unwrap_or_else(|| settings.border.clone());
    let position = point.and_then(|p| p.position).or(settings.position).unwrap_or(default);
    let lines = text.lines().count().max(1) as f32;
    let text_h = label_line(&text, &style, px_per_point, res) * lines;
    let text_w = text_width(&text, &style, px_per_point);
    let (x, y, anchor) = place(position, text_h, text_w);
    // PowerPoint keeps data labels inside the plot area: a label outside a
    // bar's end before it is moved, any other label where it stays. A label
    // centred on its point (the top of a stack) may stick out.
    let inside = |y: f32| {
        if position == LabelPosition::Center {
            return y;
        }
        y.clamp(plot.y + text_h / 2.0, (plot.bottom() - text_h / 2.0).max(plot.y + text_h / 2.0))
    };
    let (dx, dy) = point.map(|p| p.offset).unwrap_or((0, 0));
    let y = if position == LabelPosition::OutsideEnd { inside(y) } else { y };
    let (x, y) = (x + dx as f32 / 100_000.0 * move_w, y + dy as f32 / 100_000.0 * move_h);
    // A label moved by hand may leave the plot area.
    let y = if (dx, dy) == (0, 0) { inside(y) } else { y };
    labels.push(Label { x, y, text, style, anchor, fill, border });
}

/// The side on which a value axis places its tick labels (`c:tickLblPos`).
fn axis_label_side(axis: &Axis, horizontal: bool) -> Side {
    if axis.labels_low {
        if horizontal { Side::Bottom } else { Side::Left }
    } else {
        axis.side
    }
}

/// The font to draw `text` in `style` with.
fn chart_font(style: &ChartText, text: &str, res: &Resources) -> Font {
    res.font(&TextStyle { font_size: style.size, bold: style.bold, font: style.font_for(text), ..TextStyle::default() })
}

/// The height of a line of a data label: PowerPoint makes it as high as
/// the font's ascent, descent and line gap (the text centred in it, as
/// GPUI draws it).
fn label_line(text: &str, style: &ChartText, px_per_point: f32, res: &Resources) -> f32 {
    let size = style.size as f32 / 100.0 * px_per_point;
    let (a, d, gap) = res.line_metrics(&chart_font(style, text, res));
    (a + d + gap) * size
}

fn text_div(text: &str, style: &ChartText, px_per_point: f32, res: &Resources) -> Option<Div> {
    let color = style.color?;
    let font = chart_font(style, text, res);
    Some(
        div()
            .font_family(font.family)
            .font_weight(font.weight)
            .font_features(font.features)
            .text_size(px(style.size as f32 / 100.0 * px_per_point))
            .line_height(relative(1.2))
            .text_color(hsla(color))
            .whitespace_nowrap()
            .children(text.lines().map(|l| div().child(SharedString::from(l.to_string())))),
    )
}

/// A label hung on a zero-width box at its anchor point, the text overflowing
/// it to the side the anchor asks for. (Overlapping wide boxes made GPUI drop
/// the text of labels underneath.)
fn label_element(label: Label, px_per_point: f32, res: &Resources) -> Option<Div> {
    let lines = label.text.lines().count().max(1) as f32;
    let line = label_line(&label.text, &label.style, px_per_point, res);
    let height = line * lines;
    let scale = px_per_point / EMU_PER_POINT as f32;
    let bg = background(&label.fill);
    let has_box = bg.is_some() || label.border.is_some();
    let pad_y = if has_box { 1.5 * px_per_point } else { 0.0 };
    let text = text_div(&label.text, &label.style, px_per_point, res)?
        .line_height(px(line))
        .flex_shrink_0()
        .when(has_box, |d| d.px(px(3.0 * px_per_point)).py(px(pad_y)))
        .when_some(bg, |d, b| d.bg(b))
        .when_some(label.border, |d, b| d.border(px((b.width as f32 * scale).max(0.5))).border_color(hsla(b.color)));
    Some(
        div()
            .absolute()
            .left(px(label.x))
            .top(px(label.y - height / 2.0 - pad_y))
            .w(px(0.))
            .flex()
            .map(|d| match label.anchor {
                Anchor::Center => d.justify_center().text_center(),
                Anchor::Start => d.justify_start().text_left(),
                Anchor::End => d.justify_end().text_right(),
            })
            .child(text),
    )
}

/// The chart's title as a label: at its manual position (top-left corner),
/// or centred at the top.
fn title_label(chart: &Chart, w: f32, h: f32, px_per_point: f32, res: &Resources) -> Option<Label> {
    let title = chart.title.as_ref()?;
    let text = wrapped_title_text(title, w, px_per_point, res);
    let lines = text.lines().count().max(1) as f32;
    let height = label_line(&text, &title.style, px_per_point, res) * lines;
    let (x, y, anchor) = match title.at {
        // The corner is the title box's, whose text sits inside its insets
        // (3 pt at the sides, 1.5 pt above and below, as for data labels).
        Some((fx, fy)) => (fx as f32 / 100_000.0 * w + 3.0 * px_per_point, fy as f32 / 100_000.0 * h + 1.5 * px_per_point, Anchor::Start),
        None => (w / 2.0, CHART_PADDING_PT * px_per_point, Anchor::Center),
    };
    Some(Label::plain(x, y + height / 2.0, text, title.style.clone(), anchor))
}

/// The axes' titles as horizontal labels (top and bottom axes).
fn axis_title_labels(chart: &Chart, plot: Area, w: f32, h: f32, px_per_point: f32, res: &Resources) -> Vec<Label> {
    let mut labels = Vec::new();
    // (A deleted axis keeps its title, which Office draws over the plot area's edge.)
    for axis in chart.axes.iter() {
        let Some(title) = &axis.title else { continue };
        let lines = title.text.lines().count().max(1) as f32;
        let line_h = label_line(&title.text, &title.style, px_per_point, res);
        let height = line_h * lines;
        let (x, y, anchor) = match axis.side {
            // A side axis' title set horizontal (`rot="0"`) at a place of its own: its box's top-left corner, the text
            // inside the box's insets (as the chart's title).
            Side::Left | Side::Right if axis.title_rotation == Some(0) && title.at.is_some() => {
                let (fx, fy) = title.at.unwrap_or((0, 0));
                (fx as f32 / 100_000.0 * w + 3.0 * px_per_point, fy as f32 / 100_000.0 * h + 1.5 * px_per_point + height / 2.0, Anchor::Start)
            }
            Side::Bottom if axis.deleted => (plot.x + plot.w / 2.0, plot.bottom() - 2.0 * px_per_point - height / 2.0, Anchor::Center),
            Side::Top if axis.deleted => (plot.x + plot.w / 2.0, plot.y + 2.0 * px_per_point + height / 2.0, Anchor::Center),
            Side::Bottom => {
                let x = plot.x + plot.w / 2.0;
                let size = axis.text.size as f32 / 100.0 * px_per_point;
                let offset = if axis.show_labels { size * 1.5 + 4.0 * px_per_point } else { 4.0 * px_per_point };
                let y = plot.bottom() + offset + height / 2.0;
                (x, y, Anchor::Center)
            }
            Side::Top => {
                let x = plot.x + plot.w / 2.0;
                let y = (plot.y - 10.0 * px_per_point - height / 2.0).max(CHART_PADDING_PT * px_per_point);
                (x, y, Anchor::Center)
            }
            Side::Left | Side::Right => continue,
        };
        labels.push(Label::plain(x, y, title.text.clone(), title.style.clone(), anchor));
    }
    labels
}

/// The axes' titles as vertical rotated lines (left and right axes).
fn axis_title_rotated(chart: &Chart, plot: Area, px_per_point: f32, res: &Resources) -> Vec<crate::render::rotated_text::RotatedLine> {
    let mut rotated = Vec::new();
    let (left_w, right_w) = value_label_widths(chart, px_per_point, 0.5);
    for axis in chart.axes.iter() {
        let Some(title) = &axis.title else { continue };
        // (One set horizontal at a place of its own is drawn as a label: `axis_title_labels`.)
        if axis.title_rotation == Some(0) && title.at.is_some() {
            continue;
        }
        let line_h = label_line(&title.text, &title.style, px_per_point, res);
        let color = title.style.color.unwrap_or(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb::BLACK));
        let font = chart_font(&title.style, &title.text, res);
        let size = px(title.style.size as f32 / 100.0 * px_per_point);
        let at = match axis.side {
            Side::Left if axis.deleted => (plot.x + 2.0 * px_per_point + line_h / 2.0, plot.y + plot.h / 2.0),
            Side::Right if axis.deleted => (plot.right() - 2.0 * px_per_point - line_h / 2.0, plot.y + plot.h / 2.0),
            Side::Left => {
                let offset = if axis.show_labels { left_w + 4.0 * px_per_point } else { 4.0 * px_per_point };
                let x = (plot.x - offset - line_h / 2.0).max(CHART_PADDING_PT * px_per_point + line_h / 2.0);
                let y = plot.y + plot.h / 2.0;
                (x, y)
            }
            Side::Right => {
                let offset = if axis.show_labels { right_w + 4.0 * px_per_point } else { 4.0 * px_per_point };
                let x = plot.right() + offset + line_h / 2.0;
                let y = plot.y + plot.h / 2.0;
                (x, y)
            }
            Side::Bottom | Side::Top => continue,
        };
        rotated.push(crate::render::rotated_text::RotatedLine {
            text: title.text.clone(),
            font,
            size,
            color: hsla(color),
            at,
            attach: crate::render::rotated_text::Attach::Center,
            degrees: -90.0,
            max_len: Some(plot.h),
        });
    }
    rotated
}

/// The legend's entry for a series' trendline, as Office names it: the kind and the series (`線形 (S1)`).
fn trendline_legend_name(series: &Series, trendline: &ibis_doc_model::Trendline) -> String {
    use ibis_doc_model::TrendlineType as T;
    let id = match trendline.kind {
        T::Linear => "pptx.chart.legend.trendline.linear",
        T::Exponential => "pptx.chart.legend.trendline.exp",
        T::Logarithmic => "pptx.chart.legend.trendline.log",
        T::Polynomial => "pptx.chart.legend.trendline.poly",
        T::Power => "pptx.chart.legend.trendline.power",
        T::MovingAverage => "pptx.chart.legend.trendline.moving-avg",
    };
    crate::shell::i18n::format(id, &[("series", &series.name), ("period", &trendline.period)])
}

/// Whether a series has a legend entry: some data, or a range of cells
/// (empty ones) in a workbook.
fn listed(series: &Series) -> bool {
    series.values.iter().any(Option::is_some) || (series.values.len() > 1 && series.categories.len() > 1)
}

/// The names a legend lists (not counting hidden entries).
fn legend_names(chart: &Chart, legend: &ibis_doc_model::Legend) -> Vec<String> {
    if let Some(surf) = chart.groups.iter().find(|g| is_surface(g)) {
        let (min_val, max_val) = surface_min_max(surf);
        let count = 4;
        let step = (max_val - min_val) / count as f64;
        let names: Vec<String> = (0..count).map(|k| {
            let low = min_val + k as f64 * step;
            let high = min_val + (k + 1) as f64 * step;
            format!("{low:.0}-{high:.0}")
        }).collect();
        return names.into_iter().enumerate().filter(|(i, _)| !legend.hidden.contains(i)).map(|(_, n)| n).collect();
    }
    let varied_line = chart.groups.iter().find(|g| g.kind == ChartType::Line && g.series.len() == 1 && g.series[0].point_strokes.len() > 1);
    if let Some(ChartEx::Treemap { nodes, .. } | ChartEx::Sunburst(nodes)) = &chart.ex {
        return nodes.iter().map(|n| n.name.clone()).collect();
    }
    let names: Vec<String> = match chart.groups.iter().find(|g| is_round(g)).or(varied_line).and_then(|g| g.series.first()) {
        Some(series) => series.categories.clone(),
        None => {
            let mut names: Vec<String> = chart.groups.iter().flat_map(|g| g.series.iter().filter(|s| listed(s))).map(|s| s.name.clone()).collect();
            // The trendlines are listed after the series.
            names.extend(chart.groups.iter().flat_map(|g| g.series.iter().filter(|s| listed(s))).filter_map(|s| s.trendline.as_ref().map(|t| trendline_legend_name(s, t))));
            names
        }
    };
    names.into_iter().enumerate().filter(|(i, _)| !legend.hidden.contains(i)).map(|(_, n)| n).collect()
}

/// The rows an automatic legend above or below the plot wraps into (one
/// when its entries fit a row; else a grid of as many columns as fit).
/// Whether this legend wraps by the chart's padding: one of a PowerPoint chart, above or below the plot, laid out automatically.
fn power_point_legend_wraps(chart: &Chart, legend: &ibis_doc_model::Legend) -> bool {
    !chart.excel_layout && legend.layout.is_none() && matches!(legend.side, Side::Bottom | Side::Top)
}

fn legend_rows(chart: &Chart, legend: &ibis_doc_model::Legend, w: f32, px_per_point: f32, res: &Resources) -> usize {
    legend_layout(chart, legend, w, px_per_point, res).0
}

/// The height of a legend's rows in lines of one and a half of its text: a column stands wider apart (`LEGEND_ENTRY_PITCH`).
fn legend_lines(chart: &Chart, legend: &ibis_doc_model::Legend, w: f32, px_per_point: f32, res: &Resources) -> f32 {
    let (rows, column) = legend_layout(chart, legend, w, px_per_point, res);
    rows as f32 * if column { LEGEND_ENTRY_PITCH / 1.5 } else { 1.0 }
}

/// How many rows an automatic legend above or below stands in, and whether they are one column.
fn legend_layout(chart: &Chart, legend: &ibis_doc_model::Legend, w: f32, px_per_point: f32, res: &Resources) -> (usize, bool) {
    let names = legend_names(chart, legend);
    if names.len() <= 1 {
        return (1, false);
    }
    let size = legend.text.size as f32 / 100.0 * px_per_point;
    let has_line = chart.groups.iter().any(|g| g.kind == ChartType::Line);
    let excel_key = chart.excel_layout && has_line;
    let key = if excel_key { (19.4 + 2.6) * px_per_point } else { size * if has_line { 2.44 } else { 0.56 } + size * 0.37 };
    let widths: Vec<f32> = names.iter().map(|n| key + res.text_width(n, &chart_font(&legend.text, n, res), size)).collect();
    let total: f32 = widths.iter().sum::<f32>() + (names.len() - 1) as f32 * size * 0.5;
    // PowerPoint's legend above or below the plot wraps when its row would pass the chart's padding (PCHT-00204: a row of 160 pt
    // in a chart 167 pt wide stands in a column; one of 130 pt stays a row).
    if power_point_legend_wraps(chart, legend) {
        let room = w - 2.0 * CHART_PADDING_PT * px_per_point;
        if total <= room {
            return (1, false);
        }
        let widest = widths.iter().cloned().fold(0.0, f32::max).max(1.0);
        let columns = ((room + size * 0.5) / (widest + size * 0.5)).floor().max(1.0) as usize;
        return (names.len().div_ceil(columns), columns == 1);
    }
    // As the grid is made in `legend_element`: only a row well past the box wraps.
    if total <= w * 1.15 {
        return (1, false);
    }
    let widest = widths.iter().cloned().fold(0.0, f32::max).max(1.0);
    let columns = if excel_key { ((w * 0.9) / (widest + 4.9 * px_per_point)).floor().max(1.0) as usize } else { ((w * 1.0) / (widest + size * LEGEND_MIN_GAP)).floor().max(1.0) as usize };
    (names.len().div_ceil(columns), false)
}

/// The least room between two entries of a legend's grid, in ems of its text: the grid's columns are as many as fit with it
/// (the entries of a full row of equal columns touched, a name running into the next key).
const LEGEND_MIN_GAP: f32 = 0.3;

/// Line pitch of a legend's entries in a column, in ems of its text.
const LEGEND_ENTRY_PITCH: f32 = 1.85;

/// The box of a legend at a side that is laid out automatically: as wide as
/// its widest entry, its entries a line apart, centred down the chart
/// inside its padding (as Excel lays it out).
fn side_legend_area(chart: &Chart, legend: &ibis_doc_model::Legend, w: f32, h: f32, px_per_point: f32, res: &Resources) -> Area {
    let size = legend.text.size as f32 / 100.0 * px_per_point;
    let names = legend_names(chart, legend);
    let has_line = chart.groups.iter().any(|g| {
        g.kind == ChartType::Line
            || matches!(g.kind, ChartType::Radar { style: None | Some(ibis_doc_model::RadarStyle::Standard | ibis_doc_model::RadarStyle::Marker) })
    });
    let key_w = size * if has_line { 2.44 } else { 0.56 };
    let widest = names.iter().map(|n| key_w + size * 0.37 + res.text_width(n, &chart_font(&legend.text, n, res), size)).fold(0.0, f32::max);
    let lw = (widest + size * 0.5).min(w * 0.5);
    let lh = (names.len().max(1) as f32 * size * LEGEND_ENTRY_PITCH).min(h);
    let pad = CHART_PADDING_PT * px_per_point;
    let x = if legend.side == Side::Left { pad } else { w - pad - lw };
    Area { x, y: (h - lh) / 2.0, w: lw, h: lh }
}

fn legend_element(chart: &Chart, legend: &ibis_doc_model::Legend, w: f32, h: f32, px_per_point: f32, res: &Resources) -> Div {
    // Pies list their slices; other charts their series.
    let round = chart.groups.iter().find(|g| is_round(g));
    let surface = chart.groups.iter().find(|g| is_surface(g));
    // Each entry's name, colour and, for a line, its series (whose line and
    // markers the key shows).
    let entries: Vec<(String, Option<Hsla>, Option<(&Series, bool, usize)>)> = if let Some(ChartEx::Treemap { nodes, .. } | ChartEx::Sunburst(nodes)) = &chart.ex {
        nodes.iter().map(|n| (n.name.clone(), Some(hsla(n.color)), None)).collect()
    } else if let Some(surf) = surface {
        let (min_val, max_val) = surface_min_max(surf);
        let count = 4;
        let step = (max_val - min_val) / count as f64;
        (0..count)
            .map(|k| {
                let low = min_val + k as f64 * step;
                let high = min_val + (k + 1) as f64 * step;
                let name = format!("{low:.0}-{high:.0}");
                let color = surface_band_color(k as f32 / (count - 1) as f32);
                (name, Some(color), None)
            })
            .collect()
    } else {
        // A line chart of one series with colours by point lists its points.
        let varied_line = chart.groups.iter().find(|g| g.kind == ChartType::Line && g.series.len() == 1 && g.series[0].point_strokes.len() > 1);
        match round.or(varied_line).and_then(|g| g.series.first()) {
            Some(series) if round.is_none() => series
                .categories
                .iter()
                .enumerate()
                .map(|(i, name)| (name.clone(), point_stroke(series, i).map(|s| hsla(s.color)).or_else(|| series_color(series)), Some((series, false, 0))))
                .collect(),
            Some(series) => series
                .categories
                .iter()
                .enumerate()
                .map(|(i, name)| (name.clone(), background_color(point_fill(series, i)), None))
                .collect(),
            None => chart
                .groups
                .iter()
                // A series without any data has no entry (PowerPoint), unless
                // it has cells, all empty (Excel lists those).
                .flat_map(|g| {
                    let is_radar_line = matches!(g.kind, ChartType::Radar { style: None | Some(ibis_doc_model::RadarStyle::Standard | ibis_doc_model::RadarStyle::Marker) });
                    let is_radar_marker = matches!(g.kind, ChartType::Radar { style: Some(ibis_doc_model::RadarStyle::Marker) });
                    g.series.iter().filter(|s| listed(s)).map(move |s| {
                        let has_marker = s.marker.as_ref().is_some_and(|m| m.symbol != MarkerSymbol::None);
                        let line = g.kind == ChartType::Line || (matches!(g.kind, ChartType::Scatter { .. }) && (s.stroke.is_some() || has_marker)) || is_radar_line;
                        (s, line, is_radar_marker)
                    })
                })
                .enumerate()
                .map(|(index, (s, line, is_radar_marker))| {
                    let color = if line {
                        s.stroke.as_ref().map(|st| hsla(st.color)).or_else(|| series_color(s)).or_else(|| s.marker.as_ref().and_then(|m| background_color(&m.fill)))
                    } else {
                        series_color(s)
                    };
                    (s.name.clone(), color, line.then_some((s, is_radar_marker, index)))
                })
                .chain(
                    // The trendlines follow the series: a dashed key (`usize::MAX` marks it).
                    chart.groups.iter().flat_map(|g| g.series.iter().filter(|s| listed(s))).filter_map(|s| {
                        let t = s.trendline.as_ref()?;
                        let color = t.stroke.as_ref().map(|st| hsla(st.color)).or_else(|| series_color(s));
                        Some((trendline_legend_name(s, t), color, Some((s, false, usize::MAX))))
                    }),
                )
                .collect(),
        }
    };
    let area = match legend.layout {
        Some(rect) => Area::of(rect, w, h),
        None => match legend.side {
            // Under the title, when the chart has one at the top.
            Side::Top => match chart.title.as_ref().filter(|t| t.at.is_none()) {
                Some(title) => {
                    let lines = title.text.lines().count().max(1) as f32;
                    let top = CHART_PADDING_PT * px_per_point + label_line(&title.text, &title.style, px_per_point, res) * lines;
                    Area { x: 0.0, y: top, w, h: legend.text.size as f32 / 100.0 * px_per_point * 1.5 * legend_lines(chart, legend, w, px_per_point, res) + legend.text.size as f32 / 100.0 * px_per_point }
                }
                None => Area { x: 0.0, y: 0.0, w, h: h * 0.1 },
            },
            Side::Left | Side::Right => side_legend_area(chart, legend, w, h, px_per_point, res),
            // A line and a half high, the chart's padding above its bottom.
            Side::Bottom => {
                let size = legend.text.size as f32 / 100.0 * px_per_point;
                let line = size * 1.5 * legend_lines(chart, legend, w, px_per_point, res);
                // Native automatic pie/doughnut legend keys sit about 0.83pt higher
                // at 8pt text, in both presentation and spreadsheet frames.
                let lift = if chart.groups.iter().all(|g| matches!(g.kind, ChartType::Pie { .. }))
                    || chart.groups.iter().all(|g| matches!(g.kind, ChartType::Doughnut { .. }))
                    || chart.groups.iter().all(|g| matches!(g.kind, ChartType::OfPie { .. })) { size * 0.1 } else { 0.0 };
                Area { x: 0.0, y: h - CHART_PADDING_PT * px_per_point - line - lift, w, h: line }
            }
        },
    };
    let size = legend.text.size as f32 / 100.0 * px_per_point;
    let mut entries: Vec<_> = entries.into_iter().enumerate().filter(|(i, _)| !legend.hidden.contains(i)).map(|(_, e)| e).collect();
    // A legend at a side lists stacked series from the top of the stack
    // down, as they appear in the chart.
    let stacks_up = chart.groups.iter().any(|g| {
        matches!(g.grouping, Grouping::Stacked | Grouping::PercentStacked) && !matches!(g.kind, ChartType::Bar { horizontal: true, .. })
    });
    if round.is_none() && stacks_up && matches!(legend.side, Side::Left | Side::Right) {
        entries.reverse();
    }
    // Clustered horizontal bars list their series from the top bar down,
    // as they appear (the first series is the bottom one), wherever the
    // legend is (measured on the coverage suite PCHT).
    let clustered_bars = chart.groups.iter().any(|g| g.grouping == Grouping::Standard && matches!(g.kind, ChartType::Bar { horizontal: true, .. }));
    if round.is_none() && clustered_bars && !reversed_category_axis(chart) {
        entries.reverse();
    }
    let visible = entries.len().max(1) as f32;
    // Measured against PowerPoint: a bar's key is a square 0.56 of the text
    // size; when the chart has a line, every key is as wide as a line's,
    // 2.44 of the text size.
    let has_line = entries.iter().any(|e| e.2.is_some());
    // Excel's line key is a fixed 19.4 pt long whatever the text size
    // (measured on a private workbook's legend at 10 pt: 19.2 pt, and the
    // coverage suite XCHT at 8 pt: 19.5 pt), PowerPoint's 2.44 of the size.
    let excel_key = chart.excel_layout && has_line;
    let key_w = if excel_key { 19.4 * px_per_point } else { size * if has_line { 2.44 } else { 0.56 } };
    // Existing native Pie/Doughnut legend vectors put the key-to-text gap
    // at about 0.228em in automatic bottom rows (PCHT/XCHT, 8pt Calibri).
    let key_gap = if excel_key { 2.6 * px_per_point } else { size * if legend.layout.is_none() && legend.side == Side::Bottom && chart.groups.iter().all(is_round) { 0.228 } else { 0.37 } };
    // Between entries: a legend laid out automatically keeps them close,
    // about half its text size apart (PowerPoint and Excel, measured on the
    // coverage suites PCHT and XCHT); one given its own box spreads them
    // wider (1.75 sizes: decks with legends placed by hand).
    let entry_gap = size * if legend.layout.is_some() { 1.75 } else { 0.56 };
    let widest = entries.iter().map(|(name, _, _)| key_w + key_gap + text_width(name, &legend.text, px_per_point)).fold(0.0, f32::max);
    // Entries stack when the legend is at a side (unless its box is wide
    // enough for them in a row), or when a row of the widest entry does
    // not fit the box.
    // Measured in the legend's font: whether the entries fit in a row.
    let measured = |name: &str| res.text_width(name, &chart_font(&legend.text, name, res), size);
    let total: f32 = entries.iter().map(|(name, _, _)| key_w + key_gap + measured(name)).sum();
    let pp_wrap = power_point_legend_wraps(chart, legend);
    let fits_row = total.min(visible * widest) + (visible - 1.0) * entry_gap <= if pp_wrap { area.w - 2.0 * CHART_PADDING_PT * px_per_point } else { area.w };
    // A row narrows its gaps to stay within the box, down to half a size.
    let entry_gap = if visible > 1.0 { entry_gap.min((area.w - total) / (visible - 1.0)).max(size * 0.5) } else { entry_gap };
    // Two entries' widths side by side (with 5 % to spare: the substitute
    // fonts here run a little wider than Office's): a grid rather than one
    // column.
    let two_fit = 2.0 * entries.iter().map(|(name, _, _)| key_w + key_gap + measured(name)).fold(0.0, f32::max) <= area.w * 1.05;
    let column = match legend.layout {
        Some(_) if matches!(legend.side, Side::Left | Side::Right) => !two_fit || area.w < w * 0.45,
        Some(_) => !fits_row && !two_fit && area.h >= visible * size * 1.2,
        // A row past the chart's padding stands in a column when only one entry's width fits across (PCHT-00204).
        None => matches!(legend.side, Side::Left | Side::Right) || (pp_wrap && !fits_row && visible > 1.0 && 2.0 * widest + entry_gap > area.w - 2.0 * CHART_PADDING_PT * px_per_point),
    };
    // Entries in a row that do not fit it wrap into a grid of equal
    // columns, as many as fit (Excel's bottom legend of eight series).
    let measured_widest = entries.iter().map(|(name, _, _)| key_w + key_gap + measured(name)).fold(0.0, f32::max);
    // Only a row well past the box wraps: one a little over it (as the
    // substitute fonts here make it) stays a row with its gaps narrowed.
    let row_least = total + (visible - 1.0) * size * 0.5;
    let grid = !column && !fits_row && visible > 1.0 && row_least > area.w * 1.15;
    // As many columns as the widest entry allows, and at least as many as
    // the box's height needs (its rows a line and a half apart); the box's
    // width shared between them.
    // Excel's grid is as wide as its entries (4.9 pt between them) and
    // centred, in at most 90 % of the chart (a private workbook's legend of
    // 55 entries: 7 columns of 58.5 pt on a chart about 505 pt wide).
    let by_width = if excel_key { ((area.w * 0.9) / (measured_widest + 4.9 * px_per_point).max(1.0)).floor().max(1.0) } else { ((area.w * 1.0) / (measured_widest + size * LEGEND_MIN_GAP).max(1.0)).floor().max(1.0) };
    let rows_fit = (area.h / (size * 1.2)).floor().max(1.0);
    let grid_columns = by_width.max((visible / rows_fit).ceil()).min(visible);
    // (A hair under the share, so rounding does not wrap the last column.)
    let grid_width = if excel_key { measured_widest + 4.9 * px_per_point } else { area.w / grid_columns - 0.5 };
    let items = entries.into_iter().filter_map(|(name, color, line)| {
        let swatch = match line {
            // A line's key is its line, as thick as the series', with its
            // marker in the middle at the marker's size.
            // A trendline's key: its dashes.
            Some((series, _, usize::MAX)) => {
                let thickness = series.trendline.as_ref().and_then(|t| t.stroke.as_ref()).map_or(1.5, |st| st.width as f32 * px_per_point / EMU_PER_POINT as f32).max(0.5);
                let dashes = (0..3).map(|k| {
                    let step = key_w / 3.0;
                    div().absolute().left(px(step * k as f32)).w(px(step * 0.62)).top(px(size * 0.5 - thickness / 2.0)).h(px(thickness)).when_some(color, |d, c| d.bg(c))
                });
                div().relative().flex_shrink_0().w(px(key_w)).h(px(size)).children(dashes)
            }
            Some((series, is_radar_marker, index)) => {
                let thickness = series.stroke.as_ref().map_or(1.0, |st| st.width as f32 * px_per_point / EMU_PER_POINT as f32).max(0.5);
                let has_marker = series.marker.as_ref().is_some_and(|m| m.symbol != MarkerSymbol::None) || is_radar_marker;
                let marker = if has_marker {
                    let symbol = series.marker.as_ref().map_or(MarkerSymbol::Circle, |m| m.symbol);
                    let d = series.marker.as_ref().map_or(size * 0.5, |m| (m.size as f32 * px_per_point).min(size * 0.6));
                    let fill = series.marker.as_ref().and_then(|m| background_color(&m.fill)).or(color);
                    // The shapes a box cannot show (a diamond, a triangle, strokes) are painted.
                    let plain = series.marker.as_ref().is_none_or(|m| {
                        matches!(m.source.as_ref().and_then(|s| s.symbol), None | Some(ibis_doc_model::MarkerSourceSymbol::Circle | ibis_doc_model::MarkerSourceSymbol::Square))
                            && matches!(m.symbol, MarkerSymbol::Circle | MarkerSymbol::Square | MarkerSymbol::None)
                    });
                    if plain {
                        Some(
                            div()
                                .absolute()
                                .left(px(key_w / 2.0 - d / 2.0))
                                .top(px(size * 0.5 - d / 2.0))
                                .size(px(d))
                                .when(symbol == MarkerSymbol::Circle || symbol == MarkerSymbol::None, |d| d.rounded_full())
                                .when_some(fill, |d, c| d.bg(c)),
                        )
                    } else {
                        let marker = series.marker.clone().unwrap_or(ibis_doc_model::Marker { source: None, symbol: MarkerSymbol::Circle, size: 5, fill: Fill::None });
                        let marks = match fill {
                            Some(c) => marker_marks(&marker, index, d / 2.0, d / 2.0, d / 2.0, c, Some(c.into()), px_per_point),
                            None => Vec::new(),
                        };
                        Some(marks_canvas(marks).absolute().left(px(key_w / 2.0 - d / 2.0)).top(px(size * 0.5 - d / 2.0)).size(px(d)))
                    }
                } else {
                    None
                };
                div()
                    .relative()
                    .flex_shrink_0()
                    .w(px(key_w))
                    .h(px(size))
                    .children((series.stroke.is_some() || !has_marker).then(|| key_line(series.stroke.as_ref(), key_w, size, thickness, color)).into_iter().flatten())
                    .children(marker)
            }
            None => div().flex_shrink_0().w(px(key_w)).h(px(size * 0.56)).when_some(color, |d, c| d.bg(c)),
        };
        let entry = div().flex().items_center().gap(px(key_gap)).child(swatch).child(text_div(&name, &legend.text, px_per_point, res)?);
        // A grid gives every entry the widest one's width.
        let entry = if grid { entry.flex_shrink_0().min_w_0().w(px(grid_width)) } else { entry };
        // A column divides the box's height evenly between its entries.
        Some(if column { entry.h(px(match legend.layout { Some(_) => (area.h / visible).clamp(size * 0.95, size * LEGEND_ENTRY_PITCH), None => size * LEGEND_ENTRY_PITCH })) } else { entry })
    });
    // The entries form a block centred in the legend's box; in a column
    // they are left-aligned with each other.
    let block = if grid {
        // Rows of the grid, each as many entries as it has columns.
        let items: Vec<_> = items.collect();
        let per_row = grid_columns.max(1.0) as usize;
        let mut rows = Vec::new();
        let mut items = items.into_iter().peekable();
        let row_count = (visible / per_row as f32).ceil().max(1.0);
        // The rows share the box's height evenly, as a column's entries do.
        let row_h = (area.h / row_count).max(size * 1.2);
        while items.peek().is_some() {
            rows.push(div().flex().flex_row().items_center().h(px(row_h)).children(items.by_ref().take(per_row).collect::<Vec<_>>()));
        }
        div().flex().flex_col().items_start().children(rows)
    } else {
        div()
            .flex()
            .when(column, |d| d.flex_col().items_start())
            .when(!column, |d| d.gap(px(entry_gap)))
            // A row stays one row, spreading past the box evenly if it must.
            .when(!column, |d| d.flex_row().flex_shrink_0().justify_center().items_center())
            .children(items)
    };
    div()
        .absolute()
        .left(px(area.x))
        .top(px(area.y))
        .w(px(area.w))
        .h(px(area.h))
        .flex()
        .justify_center()
        .items_center()
        .child(block)
}

/// The room a treemap or sunburst has: the chart less its title and legend.
fn tree_plot(chart: &Chart, w: f32, h: f32, px_per_point: f32, res: &Resources) -> Area {
    let pad = CHART_PADDING_PT * px_per_point;
    let mut top = pad;
    let mut bottom = pad;
    if let Some(title) = chart.title.as_ref().filter(|t| t.at.is_none() && !chart.title_overlay) {
        // (Excel leaves about 5 pt more under a treemap's or sunburst's title than a plain chart does.)
        top += label_line(&title.text, &title.style, px_per_point, res) * title.text.lines().count().max(1) as f32 + pad * 0.5 + 5.0 * px_per_point;
    }
    if let Some(legend) = &chart.legend {
        let size = legend.text.size as f32 / 100.0 * px_per_point;
        // (And about 7 pt more round the legend; measured on test/editing/xlsx-chartex-treemap.pdf.)
        let band = size * 1.5 * legend_rows(chart, legend, w - 2.0 * pad, px_per_point, res) as f32 + size + 7.0 * px_per_point;
        if legend.side == Side::Bottom { bottom += band } else if legend.side == Side::Top { top += band }
    }
    Area { x: pad, y: top, w: (w - 2.0 * pad).max(1.0), h: (h - top - bottom).max(1.0) }
}

fn tree_text(color: u32, bold: bool, size_pt: f32) -> ChartText {
    ChartText { size: (size_pt * 100.0) as u32, bold, color: Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(color))), ..ChartText::default() }
}

/// The rectangles for `values` (largest first) filling (x, y, w, h), as Excel lays a treemap out: the list is cut
/// in two at its middle (the first half getting the larger share when the count is odd), the rectangle across its
/// longer side in proportion to the two halves' sums, and each half again the same way. (Read off the treemap Excel
/// drew in test/editing/xlsx-chartex-treemap.pdf: three groups of 3, 3 and 2 members; other shapes are not checked.)
fn split_tree(values: &[f64], x: f32, y: f32, w: f32, h: f32) -> Vec<(f32, f32, f32, f32)> {
    if values.len() <= 1 {
        return vec![(x, y, w.max(0.0), h.max(0.0)); values.len()];
    }
    let first = values.len().div_ceil(2);
    let (a, b) = values.split_at(first);
    let (sa, sb): (f64, f64) = (a.iter().sum(), b.iter().sum());
    let total = sa + sb;
    if total <= 0.0 {
        return vec![(x, y, 0.0, 0.0); values.len()];
    }
    let share = (sa / total) as f32;
    // (A rectangle that is about square is cut across: Excel cut a 115 x 115.3 one so.)
    let mut out = if w >= h * 0.98 {
        let wa = w * share;
        let mut left = split_tree(a, x, y, wa, h);
        left.extend(split_tree(b, x + wa, y, w - wa, h));
        left
    } else {
        let ha = h * share;
        let mut top = split_tree(a, x, y, w, ha);
        top.extend(split_tree(b, x, y + ha, w, h - ha));
        top
    };
    out.truncate(values.len());
    out
}

fn draw_treemap(nodes: &[ibis_doc_model::TreeNode], (overlapping, values, at_end): (bool, bool, bool), area: Area, px_per_point: f32, res: &Resources, marks: &mut Vec<Mark>, labels: &mut Vec<Label>) {
    #[allow(clippy::too_many_arguments)]
    fn place(nodes: &[ibis_doc_model::TreeNode], area: (f32, f32, f32, f32), opts: (bool, bool, bool), px_per_point: f32, res: &Resources, marks: &mut Vec<Mark>, labels: &mut Vec<Label>) {
        let (overlapping, values, at_end) = opts;
        let mut order: Vec<&ibis_doc_model::TreeNode> = nodes.iter().collect();
        order.sort_by(|a, b| b.value.0.partial_cmp(&a.value.0).unwrap_or(std::cmp::Ordering::Equal));
        let sizes: Vec<f64> = order.iter().map(|n| n.value.0).collect();
        let rects = split_tree(&sizes, area.0, area.1, area.2, area.3);
        let white = hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xFFFFFF)));
        let style = tree_text(0xFFFFFF, false, 9.0);
        for (node, (x, y, w, h)) in order.iter().zip(rects) {
            let color = hsla(node.color);
            marks.push(Mark::Fill(vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)], color.into()));
            marks.push(Mark::Stroke(vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)], 1.0 * px_per_point * 0.75, white));
            let line = text_height(&style, px_per_point);
            let leaf = node.children.is_empty();
            let text = if leaf && values { format!("{} {}", node.name, general_number(node.value.0)) } else { node.name.clone() };
            // A banner (a band for the group's name above its members) or, overlapping, the name on the group's corner.
            let header = if leaf || overlapping { 0.0 } else { line + 3.0 * px_per_point };
            if w > text_width(&node.name, &style, px_per_point) + 6.0 * px_per_point && h > line + 2.0 * px_per_point {
                let label = if text_width(&text, &style, px_per_point) + 6.0 * px_per_point <= w { text } else { node.name.clone() };
                // The members' names stand at the bottom when asked to (`inEnd`); the groups' always at the top.
                let ly = if leaf && at_end && overlapping { y + h - 1.5 * px_per_point - line / 2.0 } else { y + 1.5 * px_per_point + line / 2.0 };
                labels.push(Label::plain(x + 3.0 * px_per_point, ly, label, style.clone(), Anchor::Start));
            }
            if !leaf {
                let inner = if overlapping { (x, y, w, h) } else { (x + 1.0 * px_per_point, y + header, (w - 2.0 * px_per_point).max(0.0), (h - header - 1.0 * px_per_point).max(0.0)) };
                place(&node.children, inner, opts, px_per_point, res, marks, labels);
            }
        }
    }
    place(nodes, (area.x, area.y, area.w, area.h), (overlapping, values, at_end), px_per_point, res, marks, labels);
}

fn general_number(v: f64) -> String {
    format_number(v, "General")
}

fn draw_sunburst(nodes: &[ibis_doc_model::TreeNode], area: Area, px_per_point: f32, _res: &Resources, marks: &mut Vec<Mark>, labels: &mut Vec<Label>) {
    fn depth(nodes: &[ibis_doc_model::TreeNode]) -> usize {
        nodes.iter().map(|n| 1 + depth(&n.children)).max().unwrap_or(0)
    }
    let levels = depth(nodes).max(1);
    let (cx, cy) = (area.x + area.w / 2.0, area.y + area.h / 2.0);
    let radius = (area.w.min(area.h) / 2.0 - 2.0 * px_per_point).max(1.0);
    // Excel leaves a hole in the middle (about a sixth of the radius).
    let hole = radius / 6.0;
    let ring = (radius - hole) / levels as f32;
    let white = hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xFFFFFF)));
    let style = tree_text(0xFFFFFF, false, 8.0);
    #[allow(clippy::too_many_arguments)]
    fn arcs(nodes: &[ibis_doc_model::TreeNode], from: f32, span: f32, level: usize, c: (f32, f32), (ring, hole): (f32, f32), px_per_point: f32, white: Hsla, style: &ChartText, marks: &mut Vec<Mark>, labels: &mut Vec<Label>) {
        let total: f64 = nodes.iter().map(|n| n.value.0).sum();
        if total <= 0.0 {
            return;
        }
        // The largest first, clockwise from the top.
        let mut order: Vec<&ibis_doc_model::TreeNode> = nodes.iter().collect();
        order.sort_by(|a, b| b.value.0.partial_cmp(&a.value.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut start = from;
        for node in order {
            let sweep = span * (node.value.0 / total) as f32;
            let (r0, r1) = (hole + ring * level as f32, hole + ring * (level + 1) as f32);
            let steps = ((sweep / std::f32::consts::TAU) * 120.0).ceil().max(2.0) as usize;
            let at = |r: f32, k: usize| {
                let a = start + sweep * k as f32 / steps as f32;
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            };
            let mut poly: Vec<(f32, f32)> = (0..=steps).map(|k| at(r1, k)).collect();
            poly.extend((0..=steps).rev().map(|k| at(r0, k)));
            marks.push(Mark::Fill(poly.clone(), hsla(node.color).into()));
            poly.push(poly[0]);
            marks.push(Mark::Stroke(poly, 1.0 * px_per_point * 0.75, white));
            let mid = start + sweep / 2.0;
            let chord = (r0 + r1) / 2.0 * sweep;
            if chord > 2.2 * text_height(style, px_per_point) && !node.name.is_empty() {
                let r = (r0 + r1) / 2.0;
                labels.push(Label::plain(c.0 + r * mid.cos(), c.1 + r * mid.sin(), node.name.clone(), style.clone(), Anchor::Center));
            }
            arcs(&node.children, start, sweep, level + 1, c, (ring, hole), px_per_point, white, style, marks, labels);
            start += sweep;
        }
    }
    arcs(nodes, -std::f32::consts::FRAC_PI_2, std::f32::consts::TAU, 0, (cx, cy), (ring, hole), px_per_point, white, &style, marks, labels);
}

fn background_color(fill: &Fill) -> Option<Hsla> {
    match fill {
        Fill::Solid(c) => Some(hsla(*c)),
        Fill::Linear { stops, .. } | Fill::TileLinear { stops, .. } => stops.first().map(|s| hsla(s.color)),
        _ => None,
    }
}

fn draw_data_table(
    chart: &Chart,
    table: &ChartDataTable,
    plot: Area,
    horizontal: bool,
    px_per_point: f32,
    marks: &mut Vec<Mark>,
    labels: &mut Vec<Label>,
) {
    if horizontal {
        return;
    }
    let series_list: Vec<&Series> = chart.groups.iter().flat_map(|g| &g.series).collect();
    if series_list.is_empty() {
        return;
    }

    let categories = series_list
        .iter()
        .map(|s| s.values.len().max(s.categories.len()))
        .max()
        .unwrap_or(0)
        .max(1);

    let row_h = data_table_row_height(chart, px_per_point);
    let cat_top = plot.bottom();
    let table_top = cat_top + row_h;
    let table_bottom = table_top + row_h * series_list.len() as f32;

    let name_col_w = (plot.w * 0.22).clamp(40.0 * px_per_point, 120.0 * px_per_point);
    let table_left = (plot.x - name_col_w).max(2.0 * px_per_point);
    let table_right = plot.right();

    let data_w = table_right - (table_left + name_col_w);
    let col_w = data_w / categories as f32;
    let sep_x = table_left + name_col_w;

    let border_color = hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xbfbfbf)));
    let border_stroke = 1.0;

    // 1. 水平罫線 (show_horz_border)
    if table.show_horz_border {
        marks.push(Mark::Stroke(vec![(sep_x, cat_top), (table_right, cat_top)], border_stroke, border_color));
        for i in 0..series_list.len() {
            let y = table_top + i as f32 * row_h;
            marks.push(Mark::Stroke(vec![(table_left, y), (table_right, y)], border_stroke, border_color));
        }
    }

    // 2. 垂直罫線 (show_vert_border)
    if table.show_vert_border {
        marks.push(Mark::Stroke(vec![(sep_x, cat_top), (sep_x, table_bottom)], border_stroke, border_color));
        for j in 1..categories {
            let x = sep_x + j as f32 * col_w;
            marks.push(Mark::Stroke(vec![(x, cat_top), (x, table_bottom)], border_stroke, border_color));
        }
    }

    // 3. 外枠 (show_outline)
    if table.show_outline {
        marks.push(Mark::Stroke(
            vec![
                (sep_x, cat_top),
                (table_right, cat_top),
                (table_right, table_top),
                (sep_x, table_top),
                (sep_x, cat_top),
            ],
            border_stroke,
            border_color,
        ));
        marks.push(Mark::Stroke(
            vec![
                (table_left, table_top),
                (table_right, table_top),
                (table_right, table_bottom),
                (table_left, table_bottom),
                (table_left, table_top),
            ],
            border_stroke,
            border_color,
        ));
    }

    let cat_text = chart.axes.iter().find(|a| a.kind == AxisKind::Category).map(|a| &a.text);
    let text_style = ChartText {
        size: cat_text.map_or(900, |t| t.size),
        bold: false,
        color: cat_text.and_then(|t| t.color).or(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x333333)))),
        font: cat_text.and_then(|t| t.font.clone()),
        east_asian_font: cat_text.and_then(|t| t.east_asian_font.clone()),
    };

    // 4. 各行（系列）のデータ
    for (row_idx, series) in series_list.iter().enumerate() {
        let row_y = table_top + row_idx as f32 * row_h;
        let text_y = row_y + row_h * 0.5;

        // 系列名と凡例キー
        let mut name_x = table_left + 4.0 * px_per_point;
        if table.show_keys {
            let key_w = 7.0 * px_per_point;
            let key_h = 7.0 * px_per_point;
            let key_x = name_x;
            let key_y = row_y + (row_h - key_h) * 0.5;
            let fill_bg = background(&series.fill).unwrap_or_else(|| hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472c4))).into());
            marks.push(Mark::Fill(
                vec![
                    (key_x, key_y),
                    (key_x + key_w, key_y),
                    (key_x + key_w, key_y + key_h),
                    (key_x, key_y + key_h),
                ],
                fill_bg,
            ));
            name_x += key_w + 3.0 * px_per_point;
        }

        labels.push(Label::plain(
            name_x,
            text_y,
            series.name.clone(),
            text_style.clone(),
            Anchor::Start,
        ));

        // データ列
        let sep_x = table_left + name_col_w;
        for (col_idx, val) in series.values.iter().enumerate() {
            if col_idx >= categories {
                break;
            }
            let cell_center_x = sep_x + col_idx as f32 * col_w + col_w * 0.5;
            let text = val.map(|v| {
                if series.format.is_empty() {
                    v.0.to_string()
                } else {
                    format_number(v.0, &series.format)
                }
            }).unwrap_or_default();

            labels.push(Label::plain(
                cell_center_x,
                text_y,
                text,
                text_style.clone(),
                Anchor::Center,
            ));
        }
    }
}

fn dashed_line(points: &[(f32, f32)], dash_len: f32, gap_len: f32) -> Vec<Vec<(f32, f32)>> {
    let mut segments = Vec::new();
    let mut drawing = true;
    let mut remaining = dash_len;
    let mut current_segment = Vec::new();
    for window in points.windows(2) {
        let (p0, p1) = (window[0], window[1]);
        let dx = p1.0 - p0.0;
        let dy = p1.1 - p0.1;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 1e-4 {
            continue;
        }
        let (ux, uy) = (dx / dist, dy / dist);
        let mut travelled = 0.0;
        let mut cur_pt = p0;
        while travelled < dist {
            let step = (dist - travelled).min(remaining);
            let next_pt = (cur_pt.0 + ux * step, cur_pt.1 + uy * step);
            travelled += step;
            remaining -= step;
            if drawing {
                if current_segment.is_empty() {
                    current_segment.push(cur_pt);
                }
                current_segment.push(next_pt);
            }
            if remaining <= 1e-4 {
                if drawing {
                    if current_segment.len() >= 2 {
                        segments.push(std::mem::take(&mut current_segment));
                    }
                    drawing = false;
                    remaining = gap_len;
                } else {
                    drawing = true;
                    remaining = dash_len;
                }
            }
            cur_pt = next_pt;
        }
    }
    if drawing && current_segment.len() >= 2 {
        segments.push(current_segment);
    }
    segments
}

/// Draw error bars for a series if configured.
fn draw_error_bars(
    marks: &mut Vec<Mark>,
    eb: &ibis_doc_model::ErrorBars,
    _series: &ibis_doc_model::Series,
    // (point_index, data_value, screen_x, screen_y)
    points: &[(usize, f64, f32, f32)],
    plot: Area,
    scale: f32,
    scale_ref: &Scale,
    horizontal: bool,
) {
    if points.is_empty() {
        return;
    }

    use ibis_doc_model::{ErrorBarType, ErrorBarValueType};

    let valid_vals: Vec<f64> = points.iter().map(|p| p.1).collect();
    let n = valid_vals.len();
    let mean = if n > 0 { valid_vals.iter().sum::<f64>() / (n as f64) } else { 0.0 };
    let std_dev = if n > 1 {
        let variance = valid_vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / ((n - 1) as f64);
        variance.sqrt()
    } else {
        0.0
    };
    let std_err = if n > 0 { std_dev / (n as f64).sqrt() } else { 0.0 };

    let stroke_color = eb.stroke.as_ref().map(|s| hsla(s.color)).unwrap_or_else(|| {
        hsla(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x434343)))
    });
    let stroke_width = eb.stroke.as_ref().map(|s| s.width as f32 * scale).unwrap_or(1.0 * scale).max(1.0);
    let cap_w = 3.5 * scale;

    for &(ci, val, sx, sy) in points {
        let (err_plus, err_minus) = match eb.val_type {
            ErrorBarValueType::FixedValue => {
                let v = eb.value.map(|n| n.0).unwrap_or(1.0);
                (v, v)
            }
            ErrorBarValueType::Percentage => {
                let pct = eb.value.map(|n| n.0).unwrap_or(5.0);
                let v = val.abs() * (pct / 100.0);
                (v, v)
            }
            ErrorBarValueType::StandardDeviation => {
                let mult = eb.value.map(|n| n.0).unwrap_or(1.0);
                let v = mult * std_dev;
                (v, v)
            }
            ErrorBarValueType::StandardError => (std_err, std_err),
            ErrorBarValueType::Custom => {
                let fallback = eb.value.map(|n| n.0).unwrap_or(1.0);
                let p = if eb.plus.is_empty() { fallback } else { eb.plus.get(ci).copied().flatten().map_or(0.0, |n| n.0) };
                let m = if eb.minus.is_empty() { fallback } else { eb.minus.get(ci).copied().flatten().map_or(0.0, |n| n.0) };
                (p, m)
            }
        };

        if !horizontal {
            let v_plus = val + err_plus;
            let v_minus = val - err_minus;
            let py_plus = plot.bottom() - scale_ref.at(v_plus).clamp(-0.5, 1.5) * plot.h;
            let py_minus = plot.bottom() - scale_ref.at(v_minus).clamp(-0.5, 1.5) * plot.h;

            match eb.bar_type {
                ErrorBarType::Both => {
                    marks.push(Mark::Stroke(vec![(sx, py_minus), (sx, py_plus)], stroke_width, stroke_color));
                    if !eb.no_end_cap {
                        marks.push(Mark::Stroke(vec![(sx - cap_w, py_plus), (sx + cap_w, py_plus)], stroke_width, stroke_color));
                        marks.push(Mark::Stroke(vec![(sx - cap_w, py_minus), (sx + cap_w, py_minus)], stroke_width, stroke_color));
                    }
                }
                ErrorBarType::Plus => {
                    marks.push(Mark::Stroke(vec![(sx, sy), (sx, py_plus)], stroke_width, stroke_color));
                    if !eb.no_end_cap {
                        marks.push(Mark::Stroke(vec![(sx - cap_w, py_plus), (sx + cap_w, py_plus)], stroke_width, stroke_color));
                    }
                }
                ErrorBarType::Minus => {
                    marks.push(Mark::Stroke(vec![(sx, sy), (sx, py_minus)], stroke_width, stroke_color));
                    if !eb.no_end_cap {
                        marks.push(Mark::Stroke(vec![(sx - cap_w, py_minus), (sx + cap_w, py_minus)], stroke_width, stroke_color));
                    }
                }
            }
        } else {
            let v_plus = val + err_plus;
            let v_minus = val - err_minus;
            let px_plus = plot.x + scale_ref.at(v_plus).clamp(-0.5, 1.5) * plot.w;
            let px_minus = plot.x + scale_ref.at(v_minus).clamp(-0.5, 1.5) * plot.w;

            match eb.bar_type {
                ErrorBarType::Both => {
                    marks.push(Mark::Stroke(vec![(px_minus, sy), (px_plus, sy)], stroke_width, stroke_color));
                    if !eb.no_end_cap {
                        marks.push(Mark::Stroke(vec![(px_plus, sy - cap_w), (px_plus, sy + cap_w)], stroke_width, stroke_color));
                        marks.push(Mark::Stroke(vec![(px_minus, sy - cap_w), (px_minus, sy + cap_w)], stroke_width, stroke_color));
                    }
                }
                ErrorBarType::Plus => {
                    marks.push(Mark::Stroke(vec![(sx, sy), (px_plus, sy)], stroke_width, stroke_color));
                    if !eb.no_end_cap {
                        marks.push(Mark::Stroke(vec![(px_plus, sy - cap_w), (px_plus, sy + cap_w)], stroke_width, stroke_color));
                    }
                }
                ErrorBarType::Minus => {
                    marks.push(Mark::Stroke(vec![(sx, sy), (px_minus, sy)], stroke_width, stroke_color));
                    if !eb.no_end_cap {
                        marks.push(Mark::Stroke(vec![(px_minus, sy - cap_w), (px_minus, sy + cap_w)], stroke_width, stroke_color));
                    }
                }
            }
        }
    }
}

/// Draw a trendline for a series if configured.
#[allow(clippy::too_many_arguments)]
fn draw_trendline(
    marks: &mut Vec<Mark>,
    labels: &mut Vec<Label>,
    trendline: &ibis_doc_model::Trendline,
    series: &ibis_doc_model::Series,
    // (x_value, y_value, screen_x, screen_y)
    raw_points: &[(f64, f64, f32, f32)],
    plot: Area,
    scale: f32,
) {
    if raw_points.is_empty() {
        return;
    }

    use ibis_doc_model::TrendlineType;

    let valid_points: Vec<(f64, f64, f32, f32)> = raw_points
        .iter()
        .copied()
        .filter(|&(x, y, _, _)| {
            if !x.is_finite() || !y.is_finite() {
                return false;
            }
            match trendline.kind {
                TrendlineType::Logarithmic => x > 0.0,
                TrendlineType::Exponential => y > 0.0,
                TrendlineType::Power => x > 0.0 && y > 0.0,
                _ => true,
            }
        })
        .collect();

    if valid_points.len() < 2 && trendline.kind != TrendlineType::MovingAverage {
        return;
    }

    let default_color = match &series.stroke {
        Some(s) => s.color,
        None => match &series.fill {
            ibis_doc_model::Fill::Solid(c) => *c,
            _ => ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x4472c4)),
        },
    };
    let stroke_color = trendline.stroke.as_ref().map(|s| s.color).unwrap_or(default_color);
    let stroke_width = trendline.stroke.as_ref().map(|s| s.width as f32 * scale).unwrap_or(1.5 * EMU_PER_POINT as f32 * scale);

    let min_pt = valid_points.iter().min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let max_pt = valid_points.iter().max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let min_x = min_pt.map(|p| p.0).unwrap_or(0.0);
    let max_x = max_pt.map(|p| p.0).unwrap_or(1.0);
    let min_sx = min_pt.map(|p| p.2).unwrap_or(plot.x);
    let max_sx = max_pt.map(|p| p.2).unwrap_or(plot.right());
    let dx_val = max_x - min_x;
    let dx_scr = max_sx - min_sx;

    let x_to_screen = |x: f64| -> f32 {
        if dx_val.abs() < 1e-12 {
            min_sx
        } else {
            min_sx + ((x - min_x) / dx_val) as f32 * dx_scr
        }
    };

    let min_y = valid_points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_y = valid_points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let dy_val = max_y - min_y;

    let y_to_screen = |y: f64| -> f32 {
        if dy_val.abs() < 1e-12 {
            (plot.y + plot.bottom()) / 2.0
        } else {
            let mut lower = &valid_points[0];
            let mut upper = &valid_points[valid_points.len() - 1];
            for p in &valid_points {
                if p.1 <= y && (lower.1 > y || p.1 >= lower.1) {
                    lower = p;
                }
                if p.1 >= y && (upper.1 < y || p.1 <= upper.1) {
                    upper = p;
                }
            }
            if (upper.1 - lower.1).abs() > 1e-12 {
                let t = ((y - lower.1) / (upper.1 - lower.1)) as f32;
                lower.3 + t * (upper.3 - lower.3)
            } else {
                lower.3
            }
        }
    };

    let mut line_points: Vec<(f32, f32)> = Vec::new();
    let equation_text;
    let mut r_squared_val = 1.0f64;

    match trendline.kind {
        TrendlineType::Linear => {
            let n = valid_points.len() as f64;
            let sum_x: f64 = valid_points.iter().map(|p| p.0).sum();
            let sum_y: f64 = valid_points.iter().map(|p| p.1).sum();
            let mean_x = sum_x / n;
            let mean_y = sum_y / n;
            let mut num = 0.0;
            let mut den = 0.0;
            for p in &valid_points {
                num += (p.0 - mean_x) * (p.1 - mean_y);
                den += (p.0 - mean_x).powi(2);
            }
            let a = if den.abs() > 1e-12 { num / den } else { 0.0 };
            let b = mean_y - a * mean_x;

            equation_text = if b >= 0.0 {
                format!("y = {:.3}x + {:.3}", a, b)
            } else {
                format!("y = {:.3}x - {:.3}", a, b.abs())
            };

            let mut ss_tot = 0.0;
            let mut ss_res = 0.0;
            for p in &valid_points {
                let y_hat = a * p.0 + b;
                ss_tot += (p.1 - mean_y).powi(2);
                ss_res += (p.1 - y_hat).powi(2);
            }
            r_squared_val = if ss_tot > 1e-12 { (1.0 - ss_res / ss_tot).clamp(0.0, 1.0) } else { 1.0 };

            for i in 0..=30 {
                let t = i as f64 / 30.0;
                let x = min_x + t * (max_x - min_x);
                let y = a * x + b;
                line_points.push((x_to_screen(x), y_to_screen(y)));
            }
        }
        TrendlineType::Logarithmic => {
            let n = valid_points.len() as f64;
            let sum_x: f64 = valid_points.iter().map(|p| p.0.ln()).sum();
            let sum_y: f64 = valid_points.iter().map(|p| p.1).sum();
            let mean_x = sum_x / n;
            let mean_y = sum_y / n;
            let mut num = 0.0;
            let mut den = 0.0;
            for p in &valid_points {
                let lx = p.0.ln();
                num += (lx - mean_x) * (p.1 - mean_y);
                den += (lx - mean_x).powi(2);
            }
            let a = if den.abs() > 1e-12 { num / den } else { 0.0 };
            let b = mean_y - a * mean_x;

            equation_text = if b >= 0.0 {
                format!("y = {:.3}ln(x) + {:.3}", a, b)
            } else {
                format!("y = {:.3}ln(x) - {:.3}", a, b.abs())
            };

            let mut ss_tot = 0.0;
            let mut ss_res = 0.0;
            for p in &valid_points {
                let y_hat = a * p.0.ln() + b;
                ss_tot += (p.1 - mean_y).powi(2);
                ss_res += (p.1 - y_hat).powi(2);
            }
            r_squared_val = if ss_tot > 1e-12 { (1.0 - ss_res / ss_tot).clamp(0.0, 1.0) } else { 1.0 };

            for i in 0..=30 {
                let t = i as f64 / 30.0;
                let x = min_x + t * (max_x - min_x);
                let y = a * x.ln() + b;
                line_points.push((x_to_screen(x), y_to_screen(y)));
            }
        }
        TrendlineType::Exponential => {
            let n = valid_points.len() as f64;
            let sum_x: f64 = valid_points.iter().map(|p| p.0).sum();
            let sum_y: f64 = valid_points.iter().map(|p| p.1.ln()).sum();
            let mean_x = sum_x / n;
            let mean_y = sum_y / n;
            let mut num = 0.0;
            let mut den = 0.0;
            for p in &valid_points {
                let ly = p.1.ln();
                num += (p.0 - mean_x) * (ly - mean_y);
                den += (p.0 - mean_x).powi(2);
            }
            let a = if den.abs() > 1e-12 { num / den } else { 0.0 };
            let b_ln = mean_y - a * mean_x;
            let b = b_ln.exp();

            equation_text = format!("y = {:.3}e^({:.3}x)", b, a);

            let mean_orig_y = valid_points.iter().map(|p| p.1).sum::<f64>() / n;
            let mut ss_tot = 0.0;
            let mut ss_res = 0.0;
            for p in &valid_points {
                let y_hat = b * (a * p.0).exp();
                ss_tot += (p.1 - mean_orig_y).powi(2);
                ss_res += (p.1 - y_hat).powi(2);
            }
            r_squared_val = if ss_tot > 1e-12 { (1.0 - ss_res / ss_tot).clamp(0.0, 1.0) } else { 1.0 };

            for i in 0..=30 {
                let t = i as f64 / 30.0;
                let x = min_x + t * (max_x - min_x);
                let y = b * (a * x).exp();
                line_points.push((x_to_screen(x), y_to_screen(y)));
            }
        }
        TrendlineType::Power => {
            let n = valid_points.len() as f64;
            let sum_x: f64 = valid_points.iter().map(|p| p.0.ln()).sum();
            let sum_y: f64 = valid_points.iter().map(|p| p.1.ln()).sum();
            let mean_x = sum_x / n;
            let mean_y = sum_y / n;
            let mut num = 0.0;
            let mut den = 0.0;
            for p in &valid_points {
                let lx = p.0.ln();
                let ly = p.1.ln();
                num += (lx - mean_x) * (ly - mean_y);
                den += (lx - mean_x).powi(2);
            }
            let a = if den.abs() > 1e-12 { num / den } else { 0.0 };
            let b_ln = mean_y - a * mean_x;
            let b = b_ln.exp();

            equation_text = format!("y = {:.3}x^{:.3}", b, a);

            let mean_orig_y = valid_points.iter().map(|p| p.1).sum::<f64>() / n;
            let mut ss_tot = 0.0;
            let mut ss_res = 0.0;
            for p in &valid_points {
                let y_hat = b * p.0.powf(a);
                ss_tot += (p.1 - mean_orig_y).powi(2);
                ss_res += (p.1 - y_hat).powi(2);
            }
            r_squared_val = if ss_tot > 1e-12 { (1.0 - ss_res / ss_tot).clamp(0.0, 1.0) } else { 1.0 };

            for i in 0..=30 {
                let t = i as f64 / 30.0;
                let x = min_x + t * (max_x - min_x);
                let y = b * x.powf(a);
                line_points.push((x_to_screen(x), y_to_screen(y)));
            }
        }
        TrendlineType::Polynomial => {
            let s0 = valid_points.len() as f64;
            let mut s1 = 0.0;
            let mut s2 = 0.0;
            let mut s3 = 0.0;
            let mut s4 = 0.0;
            let mut t0 = 0.0;
            let mut t1 = 0.0;
            let mut t2 = 0.0;
            for p in &valid_points {
                let (x, y) = (p.0, p.1);
                let x2 = x * x;
                s1 += x;
                s2 += x2;
                s3 += x2 * x;
                s4 += x2 * x2;
                t0 += y;
                t1 += x * y;
                t2 += x2 * y;
            }
            let det = s4 * (s2 * s0 - s1 * s1) - s3 * (s3 * s0 - s1 * s2) + s2 * (s3 * s1 - s2 * s2);
            let (a, b, c) = if det.abs() > 1e-12 {
                let da = t2 * (s2 * s0 - s1 * s1) - s3 * (t1 * s0 - s1 * t0) + s2 * (t1 * s1 - s2 * t0);
                let db = s4 * (t1 * s0 - s1 * t0) - t2 * (s3 * s0 - s1 * s2) + s2 * (s3 * t0 - t1 * s2);
                let dc = s4 * (s2 * t0 - t1 * s1) - s3 * (s3 * t0 - t1 * s2) + t2 * (s3 * s1 - s2 * s2);
                (da / det, db / det, dc / det)
            } else {
                (0.0, 0.0, t0 / s0)
            };

            equation_text = format!("y = {:.3}x² {:+.3}x {:+.3}", a, b, c);

            let mean_y = t0 / s0;
            let mut ss_tot = 0.0;
            let mut ss_res = 0.0;
            for p in &valid_points {
                let y_hat = a * p.0 * p.0 + b * p.0 + c;
                ss_tot += (p.1 - mean_y).powi(2);
                ss_res += (p.1 - y_hat).powi(2);
            }
            r_squared_val = if ss_tot > 1e-12 { (1.0 - ss_res / ss_tot).clamp(0.0, 1.0) } else { 1.0 };

            for i in 0..=30 {
                let t = i as f64 / 30.0;
                let x = min_x + t * (max_x - min_x);
                let y = a * x * x + b * x + c;
                line_points.push((x_to_screen(x), y_to_screen(y)));
            }
        }
        TrendlineType::MovingAverage => {
            let period = (trendline.period as usize).clamp(2, valid_points.len());
            equation_text = crate::shell::i18n::format("_render.period-point-moving-average", &[("period", &period)]);
            if valid_points.len() >= period {
                for i in (period - 1)..valid_points.len() {
                    let avg_y: f64 = valid_points[(i + 1 - period)..=i].iter().map(|p| p.1).sum::<f64>() / period as f64;
                    let x = valid_points[i].0;
                    line_points.push((x_to_screen(x), y_to_screen(avg_y)));
                }
            }
        }
    }

    if line_points.len() >= 2 {
        let segments = dashed_line(&line_points, 6.0 * EMU_PER_POINT as f32 * scale, 4.0 * EMU_PER_POINT as f32 * scale);
        for seg in segments {
            marks.push(Mark::Stroke(seg, stroke_width, hsla(stroke_color)));
        }
    }

    let mut label_parts = Vec::new();
    if trendline.disp_eq && !equation_text.is_empty() {
        label_parts.push(equation_text);
    }
    if trendline.disp_r_sqr && trendline.kind != TrendlineType::MovingAverage {
        label_parts.push(format!("R² = {:.4}", r_squared_val));
    }

    if !label_parts.is_empty() {
        let text = label_parts.join("\n");
        let last_pt = line_points.last().copied().unwrap_or((plot.x + plot.w * 0.8, plot.y + plot.h * 0.2));
        let min_x = plot.x;
        let max_x = plot.right().max(min_x);
        let label_x = (last_pt.0 - 10.0).clamp(min_x, max_x);
        let min_y = plot.y;
        let max_y = plot.bottom().max(min_y);
        let label_y = (last_pt.1 - 15.0).clamp(min_y, max_y);
        let style = ibis_doc_model::ChartText {
            size: 900,
            bold: true,
            color: Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x333333))),
            font: None,
            east_asian_font: None,
        };
        labels.push(Label::plain(
            label_x,
            label_y,
            text,
            style,
            Anchor::Center,
        ));
    }
}

#[cfg(test)]
mod time_tests {
    use super::{add_time, civil, serial_of};

    #[test]
    fn dates_round_trip() {
        // 2016-05-20 is 42510; 2022-01-31 is 44592.
        assert_eq!(civil(42510.0), (2016, 5, 20));
        assert_eq!(serial_of(2022, 1, 31), 44592.0);
        assert_eq!(add_time(42510.0, 2.0, ibis_doc_model::TimeUnit::Months), serial_of(2016, 7, 20));
        assert_eq!(add_time(serial_of(2021, 1, 31), 1.0, ibis_doc_model::TimeUnit::Months), serial_of(2021, 2, 28));
        assert_eq!(ibis_doc_model::format_number(serial_of(2022, 1, 1), "yyyy/mm/dd"), "2022/01/01");
    }
}

#[cfg(test)]
mod scale_tests {
    use super::{Scale, bar_tick_intervals};

    #[test]
    fn horizontal_decimal_data_uses_integer_tick_labels() {
        // Office's automatically scaled bar: the values reach 7.3, but
        // the axis labels are 0, 2, 4, 6 and 8, not decimal data values.
        let intervals = bar_tick_intervals(None, 0.0, 7.3, 180.0, 1.0);
        let scale = Scale::fit(0.0, 7.3, None, intervals);
        assert_eq!(scale.ticks(), vec![0.0, 2.0, 4.0, 6.0, 8.0]);
        assert!(scale.max >= 7.3);
    }

    #[test]
    fn horizontal_small_ranges_keep_fractional_ticks() {
        let intervals = bar_tick_intervals(None, 0.0, 0.73, 240.0, 1.0);
        let scale = Scale::fit(0.0, 0.73, None, intervals);
        assert!((scale.step - 0.2).abs() < 1e-12);
        assert!((scale.max - 0.8).abs() < 1e-12);
        assert!((scale.at(0.4) - 0.5).abs() < 1e-6);
    }
}

#[cfg(test)]
mod marker_tests {
    use super::marker_fill;
    use ibis_doc_model::{Color, Fill, Marker, MarkerSymbol, Rgb, Series};

    #[test]
    fn marker_keeps_its_fill_and_point_overrides() {
        let blue = Fill::Solid(Color::opaque(Rgb(0x4472c4)));
        let red = Fill::Solid(Color::opaque(Rgb(0xff0000)));
        let green = Fill::Solid(Color::opaque(Rgb(0x00ff00)));
        let marker = Marker { source: None, symbol: MarkerSymbol::Circle, size: 5, fill: red.clone() };
        let series = Series {
            name: "S1".into(), categories: vec![], values: vec![], format: "General".into(),
            fill: blue, stroke: None, point_fills: vec![(1, green.clone()), (2, Fill::None)],
            point_strokes: vec![], explosion: 0, point_explosions: vec![], marker: Some(marker.clone()), labels: None, trendline: None, error_bars: None, x_values: vec![], bubble_sizes: vec![], bubble_3d: false,
        };
        assert_eq!(marker_fill(&series, 0, &marker), &red);
        assert_eq!(marker_fill(&series, 1, &marker), &green);
        assert_eq!(marker_fill(&series, 2, &marker), &Fill::None);
        let unfilled = Marker { fill: Fill::None, ..marker };
        assert_eq!(marker_fill(&series, 0, &unfilled), &Fill::None);
    }
}

#[cfg(test)]
mod shadow_tests {
    use super::{Mark, soft_shadow};

    /// Chart style 10: a bar's soft shadow reaches about 3 pt out, lies 1.8 pt lower than the bar, and is
    /// drawn as layers from the widest to the narrowest (GitHub #22: the chart of style 10 has a shadow).
    #[test]
    fn a_bars_shadow_is_layers_from_the_widest_down_a_little() {
        let scale = 1.0 / 12_700.0; // one pixel per point
        let bar = vec![(10.0, 20.0), (30.0, 20.0), (30.0, 60.0), (10.0, 60.0)];
        let mut marks = Vec::new();
        soft_shadow(&mut marks, &bar, scale);
        let rects: Vec<&Vec<(f32, f32)>> = marks.iter().map(|m| match m { Mark::Fill(points, _) => points, _ => panic!("a fill") }).collect();
        assert_eq!(rects.len(), 6);
        let widths: Vec<f32> = rects.iter().map(|r| r[1].0 - r[0].0).collect();
        assert!(widths.windows(2).all(|w| w[0] > w[1]), "the widest first: {widths:?}");
        // The widest reaches 2 sigma = 3.15 pt out of the bar at the sides, and is moved 1.8 pt down.
        assert!((widths[0] - (20.0 + 2.0 * 3.15)).abs() < 0.05, "{widths:?}");
        assert!((rects[0][0].1 - (20.0 - 3.15 + 1.8)).abs() < 0.05, "{:?}", rects[0]);
        assert!((rects[0][2].1 - (60.0 + 3.15 + 1.8)).abs() < 0.05, "{:?}", rects[0]);
    }
}

#[cfg(test)]
mod dash_line_tests {
    use super::{Mark, push_stroke};
    use ibis_doc_model::{Color, LineDash, Rgb, Stroke};

    const ALL: [LineDash; 10] = [
        LineDash::Dot, LineDash::Dash, LineDash::LgDash, LineDash::DashDot, LineDash::LgDashDot, LineDash::LgDashDotDot,
        LineDash::SysDash, LineDash::SysDot, LineDash::SysDashDot, LineDash::SysDashDotDot,
    ];

    /// The marks `push_stroke` makes of a line of `dash` along `points`, `width` wide.
    fn marks_of(dash: LineDash, points: Vec<(f32, f32)>, width: f32) -> Vec<Mark> {
        let mut stroke = Stroke::solid(Color::opaque(Rgb(0xff0000)), 12_700);
        stroke.dash = dash;
        let mut marks = Vec::new();
        push_stroke(&mut marks, points, width, &stroke);
        marks
    }

    /// Every line type (`a:prstDash`, ECMA-376 §20.1.10.48), at several widths, is one mark: the whole line with its
    /// pattern, the lengths of the pattern in line widths (a line thinner than a pixel dashes as one of a pixel).
    #[test]
    fn every_dash_type_is_one_dashed_line_in_its_pattern_at_every_width() {
        let line = vec![(0.0, 5.0), (400.0, 5.0)];
        for dash in ALL {
            for width in [0.25f32, 1.0, 2.0, 3.5] {
                let marks = marks_of(dash, line.clone(), width);
                match marks.as_slice() {
                    [Mark::Dashed(points, w, _, pattern)] => {
                        assert_eq!(points, &line, "{dash:?} at {width}: the line whole");
                        assert_eq!(*w, width, "{dash:?}: as wide as the line");
                        let want: Vec<f32> = dash.pattern().iter().map(|l| l * width.max(1.0)).collect();
                        assert_eq!(pattern, &want, "{dash:?} at {width}: the pattern in line widths");
                        assert_eq!(pattern.len() % 2, 0, "{dash:?}: dashes and gaps in turn");
                    }
                    other => panic!("{dash:?} at {width}: one dashed line, got {} marks", other.len()),
                }
            }
        }
    }

    /// A solid line is a line; a line of one point is left to be a (nothing) line, not dashed; a bend keeps its corner.
    #[test]
    fn a_solid_line_is_one_line_and_a_bend_keeps_its_corner() {
        let solid = Stroke::solid(Color::opaque(Rgb(0)), 3_175);
        let mut marks = Vec::new();
        push_stroke(&mut marks, vec![(0.0, 0.0), (20.0, 0.0)], 0.25, &solid);
        assert!(matches!(marks.as_slice(), [Mark::Stroke(points, w, _)] if points.len() == 2 && *w == 0.25));
        let one = marks_of(LineDash::Dash, vec![(1.0, 1.0)], 1.0);
        assert!(matches!(one.as_slice(), [Mark::Stroke(points, ..)] if points.len() == 1), "a point is not dashed");
        let bent = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 20.0)];
        let marks = marks_of(LineDash::Dash, bent.clone(), 1.0);
        assert!(matches!(marks.as_slice(), [Mark::Dashed(points, ..)] if *points == bent), "the corner stays");
    }
}

#[cfg(test)]
mod powerpoint_chart_tests {
    use super::{Area, nice_step, percent_format_without_decimals, pie_label_on_two_lines, powerpoint_3d_box, tick_intervals, whole_percentages};
    use ibis_doc_model::View3D;

    /// PowerPoint's automatic value axis (0..20, 10pt text): the most intervals that fit a plot of each height are `floor(height / (1.2 x size) - 0.5)`, at most
    /// ten, and the step is the smallest 1, 2 or 5 times a power of ten with so few intervals: in PowerPoint's PDFs the plots 20.4, 44.0, 67.6, 91.3, 122.8 and
    /// 162.1pt high have 1, 2, 4, 4, 4 and 10 intervals (steps 20, 10, 5, 5, 5 and 2), that is at most 1, 3, 5, 7, 9 and 10.
    #[test]
    fn a_value_axis_has_the_intervals_that_one_more_label_fit_in_the_height_and_half_a_label() {
        let at = |h: f32| tick_intervals(None, 0.0, 20.0, h, false, 1.0, false, false);
        assert_eq!(at(20.4), 1.0);
        assert_eq!(at(44.03), 3.0);
        assert_eq!(at(67.65), 5.0);
        assert_eq!(at(91.27), 7.0);
        assert_eq!(at(122.77), 9.0);
        assert_eq!(at(162.14), 10.0);
        assert_eq!(at(253.92), 10.0);
        // the step that follows from them (range 0..20)
        let step = |n: f64| nice_step(20.0 / n);
        assert_eq!([at(20.4), at(44.03), at(67.65), at(122.77), at(162.14)].map(step), [20.0, 10.0, 5.0, 5.0, 2.0]);
    }

    /// PowerPoint's PDF `chart-label-separator-sweep` (pie, no separator): the label of the category name and the percentage is on two lines (`Alpha` over `55%`), the other five
    /// combinations on one line with commas (`Echo, 0.9`; `8.2, 55%`; `Alpha, 8.2, 55%`; `Series, 8.2`; `Series, Alpha, 8.2`).
    #[test]
    fn a_pie_label_is_on_two_lines_only_for_the_category_and_the_percentage() {
        let labels = |cat: bool, val: bool, pct: bool, ser: bool| ibis_doc_model::DataLabels { show_category: cat, show_value: val, show_percent: pct, show_series_name: ser, ..Default::default() };
        assert!(pie_label_on_two_lines(&labels(true, false, true, false)));
        for (cat, val, pct, ser) in [(true, true, false, false), (false, true, true, false), (true, true, true, false), (false, true, false, true), (true, true, false, true)] {
            assert!(!pie_label_on_two_lines(&labels(cat, val, pct, ser)), "{cat} {val} {pct} {ser}");
        }
        let named = ibis_doc_model::DataLabels { separator: Some("; ".into()), ..labels(true, false, true, false) };
        assert!(!pie_label_on_two_lines(&named), "a separator the file names is used as it is");
    }

    /// PowerPoint's PDF `chart-pie-percent-rounding` (labels `0%`): the whole percentages add up to 100 (the largest fractions get the missing ones, the first of equals first).
    #[test]
    fn whole_percentages_add_up_to_one_hundred() {
        let n = |v: &[f64]| v.iter().map(|x| Some(ibis_doc_model::Number(*x))).collect::<Vec<_>>();
        let whole = |v: &[f64]| whole_percentages(&n(v)).unwrap().into_iter().flatten().collect::<Vec<_>>();
        assert_eq!(whole(&[8.2, 3.2, 1.4, 1.2, 0.9]), [55, 22, 9, 8, 6]);
        assert_eq!(whole(&[1.0, 1.0, 1.0]), [34, 33, 33]);
        assert_eq!(whole(&[1.0, 2.0, 3.0]), [17, 33, 50]);
        assert_eq!(whole(&[1.0, 1.0, 1.0, 1.0]), [25, 25, 25, 25]);
        assert_eq!(whole(&[1.0, 1.0, 1.0, 1.0, 1.0]), [20, 20, 20, 20, 20]);
        assert_eq!(whole(&[5.5, 4.5, 3.5, 2.5, 1.5]), [31, 26, 20, 14, 9]);
        assert!(whole_percentages(&n(&[0.0, 0.0])).is_none());
        for f in ["0%", "#0%", "#,##0%", "0 %", "0%;-0%"] {
            assert!(percent_format_without_decimals(f), "{f}");
        }
        for f in ["0.0%", "0.00%", "#,##0.0%", "General", "0"] {
            assert!(!percent_format_without_decimals(f), "{f}");
        }
    }

    /// PowerPoint's 3-D box (rotX 15, rotY 20, depth 100%, 12 categories, one series) in a 351.9 x 249.66pt plot area: the front plane is 0.3548 of its width high
    /// (0.5 x 249.66 / 351.9), the depth goes right by sin(20) and up by sin(15) of a band; an explicit height percentage is half of it times the width.
    #[test]
    fn a_3d_box_has_the_depth_of_a_band_and_the_height_of_the_plot_ratio() {
        let inner = Area { x: 97.0, y: 68.66, w: 351.9, h: 249.66 };
        let view = View3D::default();
        let (front, dx, dy) = powerpoint_3d_box(inner, &view, 12, 1, 150, 0, 1.0);
        let band = front.w / 12.0;
        assert!((dx / band - 20f32.to_radians().sin()).abs() < 1e-4, "{dx} {band}");
        assert!((dy / band - 15f32.to_radians().sin()).abs() < 1e-4, "{dy} {band}");
        assert!((front.h / front.w - 0.5 * 249.66 / 351.9).abs() < 1e-4, "{} {}", front.h, front.w);
        assert!((front.w + dx - inner.w).abs() < 1e-3, "the box fills the plot area");
        let half = View3D { height_percent: Some(50), ..view };
        let (front, ..) = powerpoint_3d_box(inner, &half, 12, 1, 150, 0, 1.0);
        assert!((front.h / front.w - 0.25).abs() < 1e-4);
        // hPercent 200 is as high as wide: it is the height that limits the box.
        let tall = View3D { height_percent: Some(200), ..view };
        let (front, _, dy) = powerpoint_3d_box(inner, &tall, 12, 1, 150, 0, 1.0);
        assert!((front.h / front.w - 1.0).abs() < 1e-4);
        assert!((front.h + dy - inner.h).abs() < 1e-3);
        // Clustered (side by side): the depth of a bar's slot, a band for one series with the gaps at 150, 0.714 of a band for 2, and the plane is as high as the plot ratio.
        let clustered = View3D { series_in_depth: false, ..view };
        let (front, dx1, _) = powerpoint_3d_box(inner, &clustered, 12, 1, 150, 0, 1.0);
        assert!((dx1 / (front.w / 12.0) - 20f32.to_radians().sin()).abs() < 1e-4);
        assert!((front.h / front.w - 249.66 / 351.9).abs() < 1e-4);
        let (front, dx2c, _) = powerpoint_3d_box(inner, &clustered, 12, 2, 150, 0, 1.0);
        assert!((dx2c / (front.w / 12.0) / 20f32.to_radians().sin() - 2.5 / 3.5).abs() < 1e-4);
        // Two series: the depth is twice as deep.
        let (front, dx2, _) = powerpoint_3d_box(inner, &view, 12, 2, 150, 0, 1.0);
        assert!((dx2 / (front.w / 12.0) - 2.0 * 20f32.to_radians().sin()).abs() < 1e-4);
    }
}
