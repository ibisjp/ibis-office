// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 色の動作（`p:animClr`、色を値に持つ `p:set`）の評価（ECMA-376 Part 1 19.5.3 `p:animClr`）。
//!
//! `p:animClr` は、`from`（なければ図形の今の色）から `to` まで、`clrSpc`（`rgb` か `hsl`）の空間で一様に動かす。`by` は、
//! 今の色に足す量（`p:hsl` は色相が 1/60000 度、彩度と輝度が 1/100000。`p:rgb` は各チャネルが 1/100000）。
//! 色相の動く向きは `dir`（`cw` で増える向き、`ccw` で減る向き）。

use crate::drawing::Rgb;

/// A colour an animation names: a theme colour (`a:schemeClr`) or a plain one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnimColor {
    Scheme(String),
    Rgb(Rgb),
}

/// What a colour behaviour changes (`attrName`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSlot {
    /// `fillcolor`, `fillColor`.
    Fill,
    /// `stroke.color`.
    Line,
    /// `style.color`.
    Text,
}

/// A colour behaviour at a moment: `q` (0 to 1) of the way from `from` to `to` (or by `by`).
#[derive(Clone, Debug, PartialEq)]
pub struct ColorChange {
    pub slot: ColorSlot,
    pub from: Option<AnimColor>,
    pub to: Option<AnimColor>,
    /// `p:by` in hue (degrees), saturation and lightness (0 to 1 each).
    pub by_hsl: Option<(f64, f64, f64)>,
    /// `p:by` in red, green and blue (0 to 1 each).
    pub by_rgb: Option<(f64, f64, f64)>,
    /// `clrSpc="hsl"`.
    pub hsl: bool,
    /// `dir="ccw"`.
    pub ccw: bool,
    /// The colours a `p:anim` of colours (`valueType="clr"`, `p:tavLst`) goes through, each at its time (0 to 1).
    pub steps: Vec<(f64, AnimColor)>,
    /// The steps are taken at once (`calcmode="discrete"`), not run into one another.
    pub discrete: bool,
    pub q: f64,
}

/// A colour behaviour as written (before its time).
#[derive(Clone, Debug, PartialEq)]
pub struct ColorSpec {
    pub change: ColorChange,
    /// A `p:set`: the colour is `to` from the start (no moving).
    pub is_set: bool,
}

fn slot_of(attr: &str) -> Option<ColorSlot> {
    match attr {
        "fillcolor" | "fillColor" => Some(ColorSlot::Fill),
        "stroke.color" => Some(ColorSlot::Line),
        "style.color" => Some(ColorSlot::Text),
        _ => None,
    }
}

/// The value of attribute `key` in the first start tag `tag` of `s`.
fn attr_of<'a>(s: &'a str, tag: &str, key: &str) -> Option<&'a str> {
    let at = s.find(&format!("<{tag}"))?;
    let rest = &s[at..];
    let end = rest.find('>')?;
    let head = &rest[..end];
    let k = head.find(&format!(" {key}=\""))? + key.len() + 3;
    let len = head[k..].find('"')?;
    Some(&head[k..k + len])
}

/// The part of `raw` inside `<p:tag>` … `</p:tag>`.
fn section<'a>(raw: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<p:{tag}>");
    let at = raw.find(&open)? + open.len();
    let end = raw[at..].find(&format!("</p:{tag}>"))?;
    Some(&raw[at..at + end])
}

fn color_in(s: &str) -> Option<AnimColor> {
    if let Some(v) = attr_of(s, "a:schemeClr", "val") {
        return Some(AnimColor::Scheme(v.to_string()));
    }
    if let Some(v) = attr_of(s, "a:srgbClr", "val") {
        return u32::from_str_radix(v, 16).ok().map(|n| AnimColor::Rgb(Rgb(n)));
    }
    if s.contains("<p:rgb") {
        let c = |k: &str| attr_of(s, "p:rgb", k)?.parse::<f64>().ok().map(|v| (v / 100_000.0 * 255.0).round().clamp(0.0, 255.0) as u8);
        return Some(AnimColor::Rgb(Rgb::from_rgb(c("r")?, c("g")?, c("b")?)));
    }
    None
}

/// The colour behaviour an opaque `p:animClr` or a `p:set` of a colour (`<p:to><p:clrVal>`) says.
pub fn color_spec(raw: &str) -> Option<ColorSpec> {
    let is_set = raw.starts_with("<p:set");
    let is_anim = raw.starts_with("<p:anim ");
    if !is_set && !is_anim && !raw.starts_with("<p:animClr") {
        return None;
    }
    let attr = section(raw, "attrName")?;
    let slot = slot_of(attr.trim())?;
    if is_anim {
        let list = section(raw, "tavLst")?;
        let mut steps = Vec::new();
        let mut rest = list;
        while let Some(at) = rest.find("<p:tav ") {
            let tav = &rest[at..];
            let end = tav.find("</p:tav>").unwrap_or(tav.len());
            let tm = attr_of(&tav[..end], "p:tav", "tm")?.parse::<f64>().ok()? / 100_000.0;
            steps.push((tm, color_in(&tav[..end])?));
            rest = &tav[end..];
        }
        if steps.is_empty() {
            return None;
        }
        let discrete = attr_of(raw, "p:anim", "calcmode") == Some("discrete");
        return Some(ColorSpec { change: ColorChange { slot, from: None, to: None, by_hsl: None, by_rgb: None, hsl: false, ccw: false, steps, discrete, q: 0.0 }, is_set: false });
    }
    let (from, to) = (section(raw, "from").and_then(color_in), section(raw, "to").and_then(color_in));
    let by = section(raw, "by");
    let by_hsl = by.filter(|b| b.contains("<p:hsl")).and_then(|b| {
        let n = |k: &str| attr_of(b, "p:hsl", k)?.parse::<f64>().ok();
        Some((n("h")? / 60_000.0, n("s")? / 100_000.0, n("l")? / 100_000.0))
    });
    let by_rgb = by.filter(|b| b.contains("<p:rgb")).and_then(|b| {
        let n = |k: &str| attr_of(b, "p:rgb", k)?.parse::<f64>().ok().map(|v| v / 100_000.0);
        Some((n("r")?, n("g")?, n("b")?))
    });
    if to.is_none() && by_hsl.is_none() && by_rgb.is_none() {
        return None;
    }
    let hsl = attr_of(raw, "p:animClr", "clrSpc") == Some("hsl");
    let ccw = attr_of(raw, "p:animClr", "dir") == Some("ccw");
    Some(ColorSpec { change: ColorChange { slot, from, to, by_hsl, by_rgb, hsl, ccw, steps: Vec::new(), discrete: false, q: 0.0 }, is_set })
}

/// Theme colour `name` of an animation (`bg1`, `tx2`, `accent3`…) as the theme scheme names it (`lt1`, `dk2`, `accent3`…).
pub fn scheme_slot(name: &str) -> &str {
    match name {
        "bg1" => "lt1",
        "tx1" => "dk1",
        "bg2" => "lt2",
        "tx2" => "dk2",
        other => other,
    }
}

fn to_hsl(c: Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (f64::from(c.r()) / 255.0, f64::from(c.g()) / 255.0, f64::from(c.b()) / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r { ((g - b) / d).rem_euclid(6.0) } else if max == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
    (h * 60.0, s, l)
}

fn from_hsl(h: f64, s: f64, l: f64) -> Rgb {
    let (s, l) = (s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let q = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb::from_rgb(q(r), q(g), q(b))
}

impl ColorChange {
    /// The colour the shape has with this change at `q` of its time, where its own colour is `base`; `theme` gives a theme
    /// colour (by the scheme's name, `lt1`, `accent2`…).
    pub fn resolve(&self, base: Rgb, theme: &dyn Fn(&str) -> Option<Rgb>) -> Rgb {
        let rgb_of = |c: &AnimColor| match c {
            AnimColor::Rgb(c) => Some(*c),
            AnimColor::Scheme(n) => theme(scheme_slot(n)),
        };
        let q = self.q.clamp(0.0, 1.0);
        // The colour of text does not run: PowerPoint draws the text in its own colour for the whole time of the behaviour and in the new
        // one from its end (measured, フォントの色: black to 1.9 s of 2 s, then the accent colour; the fill and the outline do run).
        let q = if self.slot == ColorSlot::Text && self.steps.is_empty() && q < 1.0 { 0.0 } else { q };
        if !self.steps.is_empty() {
            // The colour of the last step whose time has come; between two steps, run from the one to the next (not `discrete`).
            let at = self.steps.iter().rposition(|(t, _)| *t <= q).unwrap_or(0);
            let here = rgb_of(&self.steps[at].1).unwrap_or(base);
            return match self.steps.get(at + 1) {
                Some((t1, next)) if !self.discrete => {
                    let (t0, end) = (self.steps[at].0, rgb_of(next).unwrap_or(here));
                    let k = if *t1 > t0 { (q - t0) / (t1 - t0) } else { 1.0 };
                    ColorChange { steps: Vec::new(), from: Some(AnimColor::Rgb(here)), to: Some(AnimColor::Rgb(end)), q: k, ..self.clone() }.resolve(base, theme)
                }
                _ => here,
            };
        }
        let start = self.from.as_ref().and_then(rgb_of).unwrap_or(base);
        let lerp = |a: f64, b: f64| a + (b - a) * q;
        let (h0, s0, l0) = to_hsl(start);
        if let Some((dh, ds, dl)) = self.by_hsl {
            // The hue ends `dh` away but goes the way `dir` says (measured on PowerPoint: 補色 2, `h="-7200000"` and `dir="cw"`, takes the hue
            // 240 degrees round through violet and orange to the green 120 degrees below the start, not the short way back).
            let travel = if self.ccw { -((-dh).rem_euclid(360.0)) } else { dh.rem_euclid(360.0) };
            return from_hsl(h0 + travel * q, s0 + ds * q, l0 + dl * q);
        }
        if let Some((dr, dg, db)) = self.by_rgb {
            let ch = |v: u8, d: f64| (f64::from(v) + d * 255.0 * q).round().clamp(0.0, 255.0) as u8;
            return Rgb::from_rgb(ch(start.r(), dr), ch(start.g(), dg), ch(start.b(), db));
        }
        let Some(end) = self.to.as_ref().and_then(rgb_of) else { return start };
        if self.hsl {
            let (h1, s1, l1) = to_hsl(end);
            // The hue goes the way `dir` says (clockwise: it grows).
            let mut dh = h1 - h0;
            if self.ccw {
                if dh > 0.0 {
                    dh -= 360.0;
                }
            } else if dh < 0.0 {
                dh += 360.0;
            }
            return from_hsl(h0 + dh * q, lerp(s0, s1), lerp(l0, l1));
        }
        let ch = |a: u8, b: u8| lerp(f64::from(a), f64::from(b)).round().clamp(0.0, 255.0) as u8;
        Rgb::from_rgb(ch(start.r(), end.r()), ch(start.g(), end.g()), ch(start.b(), end.b()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = r#"<p:animClr clrSpc="rgb" dir="cw"><p:cBhvr><p:cTn id="" dur="2000" fill="hold"/><p:attrNameLst><p:attrName>fillcolor</p:attrName></p:attrNameLst></p:cBhvr><p:to><a:schemeClr val="accent2"/></p:to></p:animClr>"#;

    #[test]
    fn a_colour_goes_from_its_own_to_the_named_one_in_rgb() {
        let spec = color_spec(RAW).unwrap();
        assert_eq!(spec.change.slot, ColorSlot::Fill);
        let theme = |n: &str| (n == "accent2").then_some(Rgb::from_rgb(200, 100, 0));
        let at = |q: f64| ColorChange { q, ..spec.change.clone() }.resolve(Rgb::from_rgb(0, 100, 200), &theme);
        assert_eq!(at(0.0), Rgb::from_rgb(0, 100, 200));
        assert_eq!(at(0.5), Rgb::from_rgb(100, 100, 100));
        assert_eq!(at(1.0), Rgb::from_rgb(200, 100, 0));
    }

    #[test]
    fn by_hsl_adds_to_hue_saturation_and_lightness() {
        let raw = r#"<p:animClr clrSpc="hsl" dir="cw"><p:cBhvr><p:attrNameLst><p:attrName>fillcolor</p:attrName></p:attrNameLst></p:cBhvr><p:by><p:hsl h="7200000" s="0" l="0"/></p:by></p:animClr>"#;
        let spec = color_spec(raw).unwrap();
        assert_eq!(spec.change.by_hsl, Some((120.0, 0.0, 0.0)));
        // Red turned 120 degrees is green.
        let red = Rgb::from_rgb(255, 0, 0);
        assert_eq!(ColorChange { q: 1.0, ..spec.change.clone() }.resolve(red, &|_| None), Rgb::from_rgb(0, 255, 0));
        // By -120 degrees going clockwise: red turns the long way (240 degrees) to the blue 120 below it, through green at half way, to the blue at the end.
        let back = ColorChange { by_hsl: Some((-120.0, 0.0, 0.0)), q: 0.5, ..spec.change };
        assert_eq!(back.resolve(red, &|_| None), Rgb::from_rgb(0, 255, 0));
        assert_eq!(ColorChange { q: 1.0, ..back }.resolve(red, &|_| None), Rgb::from_rgb(0, 0, 255));
    }

    #[test]
    fn a_set_of_a_colour_is_read() {
        let raw = r#"<p:set><p:cBhvr><p:cTn id="" dur="500" fill="hold"/><p:attrNameLst><p:attrName>style.color</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:clrVal><a:schemeClr val="bg1"/></p:clrVal></p:to></p:set>"#;
        let spec = color_spec(raw).unwrap();
        assert!(spec.is_set);
        assert_eq!((spec.change.slot, spec.change.to), (ColorSlot::Text, Some(AnimColor::Scheme("bg1".into()))));
        assert_eq!(scheme_slot("bg1"), "lt1");
    }

    #[test]
    fn colours_that_step_are_read_and_resolved() {
        let raw = r#"<p:anim calcmode="discrete" valueType="clr"><p:cBhvr><p:attrNameLst><p:attrName>fillcolor</p:attrName></p:attrNameLst></p:cBhvr><p:tavLst><p:tav tm="0"><p:val><p:clrVal><a:schemeClr val="accent2"/></p:clrVal></p:val></p:tav><p:tav tm="50000"><p:val><p:clrVal><a:schemeClr val="hlink"/></p:clrVal></p:val></p:tav></p:tavLst></p:anim>"#;
        let spec = color_spec(raw).unwrap();
        assert_eq!(spec.change.steps.len(), 2);
        let theme = |n: &str| match n {
            "accent2" => Some(Rgb::from_rgb(1, 2, 3)),
            "hlink" => Some(Rgb::from_rgb(9, 8, 7)),
            _ => None,
        };
        let at = |q: f64| ColorChange { q, ..spec.change.clone() }.resolve(Rgb::BLACK, &theme);
        assert_eq!((at(0.2), at(0.5), at(1.0)), (Rgb::from_rgb(1, 2, 3), Rgb::from_rgb(9, 8, 7), Rgb::from_rgb(9, 8, 7)));
    }

    #[test]
    fn the_colour_of_text_changes_at_the_end_not_along_the_way() {
        let raw = RAW.replace("fillcolor", "style.color");
        let spec = color_spec(&raw).unwrap();
        assert_eq!(spec.change.slot, ColorSlot::Text);
        let theme = |n: &str| (n == "accent2").then_some(Rgb::from_rgb(200, 100, 0));
        let base = Rgb::from_rgb(0, 0, 0);
        let at = |q: f64| ColorChange { q, ..spec.change.clone() }.resolve(base, &theme);
        assert_eq!((at(0.0), at(0.5), at(0.99)), (base, base, base));
        assert_eq!(at(1.0), Rgb::from_rgb(200, 100, 0));
    }

    #[test]
    fn hsl_round_trips() {
        for c in [Rgb::from_rgb(1, 80, 197), Rgb::from_rgb(237, 125, 49), Rgb::from_rgb(10, 10, 10)] {
            let (h, s, l) = to_hsl(c);
            assert_eq!(from_hsl(h, s, l), c);
        }
    }
}
