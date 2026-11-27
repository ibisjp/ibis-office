// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Theme colour references (`a:schemeClr` with its transforms, ECMA-376 20.1.2.3) kept on a colour, and the
//! one function that works a colour out of a reference and the theme's colours.

use crate::drawing::{Color, Rgb};

/// The theme colour slots a `a:schemeClr` can name: the twelve of the colour scheme, and the four names the master's
/// `clrMap` turns into them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeSlot {
    Dk1, Lt1, Dk2, Lt2, Accent1, Accent2, Accent3, Accent4, Accent5, Accent6, Hlink, FolHlink, Tx1, Bg1, Tx2, Bg2,
}

impl ThemeSlot {
    const ALL: [(ThemeSlot, &'static str); 16] = [
        (ThemeSlot::Dk1, "dk1"), (ThemeSlot::Lt1, "lt1"), (ThemeSlot::Dk2, "dk2"), (ThemeSlot::Lt2, "lt2"),
        (ThemeSlot::Accent1, "accent1"), (ThemeSlot::Accent2, "accent2"), (ThemeSlot::Accent3, "accent3"),
        (ThemeSlot::Accent4, "accent4"), (ThemeSlot::Accent5, "accent5"), (ThemeSlot::Accent6, "accent6"),
        (ThemeSlot::Hlink, "hlink"), (ThemeSlot::FolHlink, "folHlink"),
        (ThemeSlot::Tx1, "tx1"), (ThemeSlot::Bg1, "bg1"), (ThemeSlot::Tx2, "tx2"), (ThemeSlot::Bg2, "bg2"),
    ];

    /// The name as written in the file (`val` of `a:schemeClr`).
    pub fn name(self) -> &'static str {
        Self::ALL.iter().find(|(s, _)| *s == self).map(|(_, n)| *n).unwrap_or("dk1")
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().find(|(_, n)| *n == name).map(|(s, _)| *s)
    }

    /// The slot in the colour scheme (`dk1` … `folHlink`), with `tx1`, `bg1`, `tx2` and `bg2` under the standard colour map.
    pub fn standard(self) -> Self {
        match self {
            Self::Tx1 => Self::Dk1,
            Self::Bg1 => Self::Lt1,
            Self::Tx2 => Self::Dk2,
            Self::Bg2 => Self::Lt2,
            other => other,
        }
    }
}

/// The transforms a colour element can carry (`a:lumMod` …), by their element names.
const MOD_NAMES: [&str; 28] = [
    "lum", "lumMod", "lumOff", "sat", "satMod", "satOff", "hue", "hueMod", "hueOff", "comp", "red", "redMod", "redOff", "green",
    "greenMod", "greenOff", "blue", "blueMod", "blueOff", "gamma", "invGamma", "tint", "shade", "alpha", "alphaOff", "alphaMod",
    "gray", "inv",
];

/// How many transforms a reference keeps (a colour with more is held as its colour only).
pub const MAX_MODS: usize = 4;

/// One transform: the index of its element name in [`MOD_NAMES`], and its `val` (0 when it has none).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThemeMod {
    kind: u8,
    pub val: i32,
}

impl ThemeMod {
    pub fn new(name: &str, val: i32) -> Option<Self> {
        MOD_NAMES.iter().position(|n| *n == name).map(|i| Self { kind: i as u8, val })
    }

    pub fn name(&self) -> &'static str {
        MOD_NAMES[self.kind as usize]
    }

    /// Whether the element writes a `val` (`a:comp`, `a:gray`, `a:inv`, … have none).
    pub fn has_val(&self) -> bool {
        !matches!(self.name(), "comp" | "gamma" | "invGamma" | "gray" | "inv")
    }
}

/// A colour that refers to the theme: the slot, as the file names it (`name`) and as the colour map turns it (`via`), and the
/// transforms in the order they apply. `resolved_rgb` and `resolved_alpha` are what the reference worked out to when it was
/// last resolved: a colour whose own `rgb` or `alpha` has since changed no longer follows the reference (see [`Color::theme_ref`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThemeRef {
    pub name: ThemeSlot,
    pub via: ThemeSlot,
    mods: [Option<ThemeMod>; MAX_MODS],
    pub resolved_rgb: Rgb,
    pub resolved_alpha: u8,
}

impl ThemeRef {
    /// A reference to `name` (mapped to `via` of the colour scheme) with these transforms. `None` when there are too many.
    pub fn new(name: ThemeSlot, via: ThemeSlot, mods: &[ThemeMod]) -> Option<Self> {
        if mods.len() > MAX_MODS {
            return None;
        }
        let mut slots = [None; MAX_MODS];
        for (slot, m) in slots.iter_mut().zip(mods) {
            *slot = Some(*m);
        }
        Some(Self { name, via: via.standard(), mods: slots, resolved_rgb: Rgb::BLACK, resolved_alpha: 255 })
    }

    pub fn mods(&self) -> impl Iterator<Item = ThemeMod> + '_ {
        self.mods.iter().flatten().copied()
    }

    /// The reference as Office's `TintAndShade` ("brightness", -1.0 to 1.0): positive lightens (`lumMod` = 1 - b, `lumOff` = b),
    /// negative darkens (`lumMod` = 1 + b).
    pub fn with_brightness(name: ThemeSlot, brightness: f32) -> Option<Self> {
        let b = brightness.clamp(-1.0, 1.0);
        let scale = |v: f32| (v * 100_000.0).round() as i32;
        let mods: Vec<ThemeMod> = if b > 0.0 {
            vec![ThemeMod::new("lumMod", scale(1.0 - b))?, ThemeMod::new("lumOff", scale(b))?]
        } else if b < 0.0 {
            vec![ThemeMod::new("lumMod", scale(1.0 + b))?]
        } else {
            Vec::new()
        };
        Self::new(name, name, &mods)
    }

    /// The reference's brightness when its transforms are only `lumMod` / `lumOff` of the `TintAndShade` form; 0 for none.
    pub fn brightness(&self) -> Option<f32> {
        // The transparency (`alpha` transforms) is not part of the brightness.
        let mods: Vec<ThemeMod> = self.mods().filter(|m| !matches!(m.name(), "alpha" | "alphaMod" | "alphaOff")).collect();
        match mods.as_slice() {
            [] => Some(0.0),
            [m] if m.name() == "lumMod" => Some(m.val as f32 / 100_000.0 - 1.0),
            [m, o] if m.name() == "lumMod" && o.name() == "lumOff" => Some(o.val as f32 / 100_000.0),
            _ => None,
        }
    }

    /// The colour this reference stands for under `colors` (the theme's, by scheme name). `None` when the theme names none of
    /// the slot.
    pub fn resolve(&self, colors: &[(String, Rgb)]) -> Option<Color> {
        let base = colors.iter().find(|(n, _)| n == self.via.name()).map(|(_, c)| *c)?;
        let pairs: Vec<(&str, f32)> = self.mods().map(|m| (m.name(), m.val as f32)).collect();
        let mut color = apply_color_modifiers(Color::opaque(base), &pairs);
        color.theme = Some(Self { resolved_rgb: color.rgb, resolved_alpha: color.alpha, ..*self });
        Some(color)
    }

    /// This reference with the colour it resolved to recorded.
    pub fn resolved_to(mut self, rgb: Rgb, alpha: u8) -> Self {
        self.resolved_rgb = rgb;
        self.resolved_alpha = alpha;
        self
    }
}

impl Color {
    /// The theme reference this colour still follows: `None` when it has none, or when its `rgb` or `alpha` has been set to
    /// something else since (a colour chosen directly drops the reference).
    pub fn theme_ref(&self) -> Option<&ThemeRef> {
        self.theme.as_ref().filter(|t| t.resolved_rgb == self.rgb && t.resolved_alpha == self.alpha)
    }

    /// This colour worked out again under the theme `colors`, when it follows a reference.
    pub fn retheme(&mut self, colors: &[(String, Rgb)]) {
        if let Some(t) = self.theme_ref().copied()
            && let Some(c) = t.resolve(colors)
        {
            *self = c;
        }
    }

    /// A colour that follows `theme`, worked out under `colors` (`fallback` when the theme has no such slot).
    pub fn themed(theme: ThemeRef, colors: &[(String, Rgb)], fallback: Rgb) -> Self {
        theme.resolve(colors).unwrap_or_else(|| {
            let mut c = Color::opaque(fallback);
            c.theme = Some(theme.resolved_to(fallback, 255));
            c
        })
    }
}

/// Apply the transforms `mods` (element name and `val`) to `color`, in order (ECMA-376 20.1.2.3).
pub fn apply_color_modifiers(mut color: Color, mods: &[(&str, f32)]) -> Color {
    for &(name, raw_val) in mods {
        let val = raw_val / 100_000.0;
        let (r, g, b) = (color.rgb.r() as f32 / 255.0, color.rgb.g() as f32 / 255.0, color.rgb.b() as f32 / 255.0);
        let (r, g, b) = match name {
            "lum" => {
                let (h, s, _) = rgb_to_hsl(r, g, b);
                hsl_to_rgb_f32(h, s, val.clamp(0.0, 1.0))
            }
            "lumMod" => {
                let (h, s, mut l) = rgb_to_hsl(r, g, b);
                l *= val;
                hsl_to_rgb_f32(h, s, l.clamp(0.0, 1.0))
            }
            "lumOff" => {
                let (h, s, mut l) = rgb_to_hsl(r, g, b);
                l += val;
                hsl_to_rgb_f32(h, s, l.clamp(0.0, 1.0))
            }
            "sat" => {
                let (h, _, l) = rgb_to_hsl(r, g, b);
                hsl_to_rgb_f32(h, val.clamp(0.0, 1.0), l)
            }
            "satMod" => {
                let (h, mut s, l) = rgb_to_hsl(r, g, b);
                s *= val;
                // Do not clamp `s` to 1.0 before converting back to RGB: when a prior `tint`
                // raised `l` toward 1.0, `s > 1.0` scales the remaining chroma `2 * s * (1 - l)`
                // before `hsl_to_rgb_f32` clamps each RGB channel to 0.0..=1.0 (pptx-C020-b3).
                hsl_to_rgb_f32(h, s.max(0.0), l)
            }
            "satOff" => {
                let (h, mut s, l) = rgb_to_hsl(r, g, b);
                s += val;
                hsl_to_rgb_f32(h, s.clamp(0.0, 1.0), l)
            }
            "hue" => {
                let (_, s, l) = rgb_to_hsl(r, g, b);
                let h = (raw_val / 21_600_000.0).rem_euclid(1.0);
                hsl_to_rgb_f32(h, s, l)
            }
            "hueMod" => {
                let (mut h, s, l) = rgb_to_hsl(r, g, b);
                h = (h * val).rem_euclid(1.0);
                hsl_to_rgb_f32(h, s, l)
            }
            "hueOff" => {
                let (mut h, s, l) = rgb_to_hsl(r, g, b);
                h = (h + raw_val / 21_600_000.0).rem_euclid(1.0);
                hsl_to_rgb_f32(h, s, l)
            }
            "comp" => {
                let (h, s, l) = rgb_to_hsl(r, g, b);
                let h = (h + 0.5).rem_euclid(1.0);
                hsl_to_rgb_f32(h, s, l)
            }
            "red" => (val.clamp(0.0, 1.0), g, b),
            "redMod" => ((r * val).clamp(0.0, 1.0), g, b),
            "redOff" => ((r + val).clamp(0.0, 1.0), g, b),
            "green" => (r, val.clamp(0.0, 1.0), b),
            "greenMod" => (r, (g * val).clamp(0.0, 1.0), b),
            "greenOff" => (r, (g + val).clamp(0.0, 1.0), b),
            "blue" => (r, g, val.clamp(0.0, 1.0)),
            "blueMod" => (r, g, (b * val).clamp(0.0, 1.0)),
            "blueOff" => (r, g, (b + val).clamp(0.0, 1.0)),
            "gamma" => (linear_to_srgb(r), linear_to_srgb(g), linear_to_srgb(b)),
            "invGamma" => (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b)),
            // Tint mixes towards white, shade towards black (in linear light).
            "tint" => map_linear(r, g, b, |c| 1.0 - (1.0 - c) * val),
            "shade" => map_linear(r, g, b, |c| c * val),
            "alpha" => {
                color.alpha = (val.clamp(0.0, 1.0) * 255.0).round() as u8;
                (r, g, b)
            }
            "alphaOff" => {
                color.alpha = (color.alpha as f32 + val * 255.0).clamp(0.0, 255.0).round() as u8;
                (r, g, b)
            }
            "alphaMod" => {
                color.alpha = (color.alpha as f32 * val).clamp(0.0, 255.0).round() as u8;
                (r, g, b)
            }
            "gray" => {
                let y = 0.299 * r + 0.587 * g + 0.114 * b;
                (y, y, y)
            }
            "inv" => map_linear(r, g, b, |c| 1.0 - c),
            _ => (r, g, b),
        };
        let to8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        color.rgb = Rgb::from_rgb(to8(r), to8(g), to8(b));
    }
    color
}

fn map_linear(r: f32, g: f32, b: f32, f: impl Fn(f32) -> f32) -> (f32, f32, f32) {
    let m = |c: f32| linear_to_srgb(f(srgb_to_linear(c)));
    (m(r), m(g), m(b))
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

/// `h`, `s` and `l` in 0..=1. Returns `(r, g, b)` in 0.0..=1.0.
pub fn hsl_to_rgb_f32(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s <= 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
}

// ---- Working the references out again after the theme changed -------------------------------------------------------

use crate::drawing::{Fill, GradientStop, Stroke};
use crate::model::{Shape, ShapeKind};
use crate::text::{RunStyle, TextBody, TextStyle};

fn retheme_stops(stops: &mut [GradientStop], colors: &[(String, Rgb)]) {
    for s in stops {
        s.color.retheme(colors);
    }
}

fn retheme_fill(fill: &mut Fill, colors: &[(String, Rgb)]) {
    match fill {
        Fill::Solid(c) => c.retheme(colors),
        Fill::Linear { stops, .. } | Fill::TileLinear { stops, .. } | Fill::Path { stops, .. } => retheme_stops(stops, colors),
        Fill::Pattern { foreground, background, .. } => {
            foreground.retheme(colors);
            background.retheme(colors);
        }
        _ => {}
    }
}

fn retheme_stroke(stroke: &mut Option<Stroke>, colors: &[(String, Rgb)]) {
    if let Some(s) = stroke {
        s.color.retheme(colors);
    }
}

/// The text colour of a style (or run) that follows a theme reference, worked out again.
fn retheme_text_colour(rgb: &mut Rgb, theme: &mut Option<ThemeRef>, colors: &[(String, Rgb)]) {
    let Some(t) = theme.filter(|t| t.resolved_rgb == *rgb) else { return };
    if let Some(c) = t.resolve(colors) {
        *rgb = c.rgb;
        *theme = c.theme;
    }
}

fn retheme_style(style: &mut TextStyle, colors: &[(String, Rgb)]) {
    retheme_text_colour(&mut style.color, &mut style.color_theme, colors);
}

fn retheme_run(run: &mut RunStyle, colors: &[(String, Rgb)]) {
    if let Some(mut rgb) = run.color {
        retheme_text_colour(&mut rgb, &mut run.color_theme, colors);
        run.color = Some(rgb);
    }
    if let Some((c, _)) = &mut run.outline {
        c.retheme(colors);
    }
    if let Some(f) = &mut run.fill {
        retheme_fill(f, colors);
    }
    if let Some(s) = &mut run.shadow {
        s.color.retheme(colors);
    }
    if let Some((c, _)) = &mut run.glow {
        c.retheme(colors);
    }
}

fn retheme_body(body: &mut TextBody, colors: &[(String, Rgb)]) {
    retheme_style(&mut body.style, colors);
    for p in &mut body.paragraphs {
        if let Some(b) = &mut p.bullet
            && let Some(mut rgb) = b.color
        {
            retheme_text_colour(&mut rgb, &mut b.color_theme, colors);
            b.color = Some(rgb);
        }
        for r in &mut p.runs {
            retheme_run(&mut r.style, colors);
        }
        retheme_run(&mut p.end_style, colors);
    }
}

fn retheme_chart_text(text: &mut crate::chart::ChartText, colors: &[(String, Rgb)]) {
    if let Some(c) = &mut text.color {
        c.retheme(colors);
    }
}

/// A chart's colours that follow a theme reference (series, points, markers, labels, axes, gridlines, title, legend, areas).
fn retheme_chart(chart: &mut crate::chart::Chart, colors: &[(String, Rgb)]) {
    fn labels(l: &mut crate::chart::DataLabels, colors: &[(String, Rgb)]) {
        retheme_chart_text(&mut l.style, colors);
        retheme_fill(&mut l.fill, colors);
        retheme_stroke(&mut l.border, colors);
        for p in &mut l.points {
            if let Some(s) = &mut p.style {
                retheme_chart_text(s, colors);
            }
            if let Some(f) = &mut p.fill {
                retheme_fill(f, colors);
            }
            if let Some(b) = &mut p.border {
                retheme_stroke(b, colors);
            }
        }
    }
    for group in &mut chart.groups {
        for s in &mut group.series {
            retheme_fill(&mut s.fill, colors);
            retheme_stroke(&mut s.stroke, colors);
            for (_, f) in &mut s.point_fills {
                retheme_fill(f, colors);
            }
            for (_, st) in &mut s.point_strokes {
                retheme_stroke(st, colors);
            }
            if let Some(sh) = &mut s.inner_shadow {
                sh.color.retheme(colors);
            }
            if let Some(m) = &mut s.marker {
                retheme_fill(&mut m.fill, colors);
                retheme_stroke(&mut m.edge, colors);
            }
            if let Some(l) = &mut s.labels {
                labels(l, colors);
            }
            if let Some(t) = &mut s.trendline {
                retheme_stroke(&mut t.stroke, colors);
            }
            if let Some(e) = &mut s.error_bars {
                retheme_stroke(&mut e.stroke, colors);
            }
        }
    }
    for axis in &mut chart.axes {
        retheme_stroke(&mut axis.line, colors);
        retheme_stroke(&mut axis.gridlines, colors);
        retheme_stroke(&mut axis.minor_gridlines, colors);
        retheme_chart_text(&mut axis.text, colors);
        if let Some(t) = &mut axis.title {
            retheme_chart_text(&mut t.style, colors);
        }
    }
    if let Some(t) = &mut chart.title {
        retheme_chart_text(&mut t.style, colors);
    }
    if let Some(l) = &mut chart.legend {
        retheme_chart_text(&mut l.text, colors);
    }
    retheme_fill(&mut chart.background, colors);
    retheme_stroke(&mut chart.border, colors);
    retheme_fill(&mut chart.plot_background, colors);
    retheme_stroke(&mut chart.plot_border, colors);
}

fn retheme_shape(shape: &mut Shape, colors: &[(String, Rgb)]) {
    match &mut shape.kind {
        ShapeKind::TextBox(body) => retheme_body(body, colors),
        ShapeKind::Auto(a) => {
            retheme_fill(&mut a.fill, colors);
            retheme_stroke(&mut a.stroke, colors);
            if let Some(t) = &mut a.text {
                retheme_body(t, colors);
            }
            if let Some(s) = &mut a.shadow {
                s.color.retheme(colors);
            }
            if let Some(s) = &mut a.inner_shadow {
                s.color.retheme(colors);
            }
            if let Some((c, _)) = &mut a.glow {
                c.retheme(colors);
            }
        }
        ShapeKind::Line(stroke) => stroke.color.retheme(colors),
        ShapeKind::Chart(chart) => retheme_chart(chart, colors),
        ShapeKind::Table(table) => {
            for row in &mut table.rows {
                for cell in &mut row.cells {
                    retheme_body(&mut cell.text, colors);
                    retheme_fill(&mut cell.fill, colors);
                    for b in [&mut cell.borders.left, &mut cell.borders.right, &mut cell.borders.top, &mut cell.borders.bottom, &mut cell.borders.tl_to_br, &mut cell.borders.bl_to_tr] {
                        retheme_stroke(b, colors);
                    }
                }
            }
        }
        _ => {}
    }
}

impl crate::model::ObjectDefaults {
    /// The colours of 既定の図形・線・テキスト ボックス that follow a theme reference, worked out again under `colors`.
    pub fn retheme(&mut self, colors: &[(String, Rgb)]) {
        for d in [&mut self.shape, &mut self.text_box].into_iter().flatten() {
            retheme_fill(&mut d.fill, colors);
            retheme_stroke(&mut d.stroke, colors);
            retheme_style(&mut d.text, colors);
            if let Some(s) = &mut d.shadow {
                s.color.retheme(colors);
            }
            if let Some((c, _)) = &mut d.glow {
                c.retheme(colors);
            }
        }
        retheme_stroke(&mut self.line, colors);
    }
}

impl crate::model::Presentation {
    /// Work every colour that follows a theme reference out again under the presentation's theme colours (after it is read, and
    /// when the theme is replaced). A colour without a reference, or one set to another colour since, is left as it is.
    pub fn resolve_theme_colors(&mut self) {
        if self.theme_colors.is_empty() {
            return;
        }
        let colors = std::mem::take(&mut self.theme_colors);
        for slide in &mut self.slides {
            retheme_fill(&mut slide.background, &colors);
            for s in slide.shapes.iter_mut().chain(slide.backdrop.iter_mut()) {
                retheme_shape(s, &colors);
            }
        }
        self.object_defaults.retheme(&colors);
        for layout in &mut self.layouts {
            retheme_fill(&mut layout.background, &colors);
            for s in &mut layout.backdrop {
                retheme_shape(s, &colors);
            }
        }
        self.theme_colors = colors;
    }
}
