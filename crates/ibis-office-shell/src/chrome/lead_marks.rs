// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Small previews at the left of a menu line's words (2026-11-09): the colour chips of a theme's colours and the three
//! samples of a theme's effects. A menu line says what it is with a picture ([`MenuItem::lead`]) and the one part
//! here draws it, so the document, spreadsheet and presentation screens' Colors and Effects menus look the same.
//!
//! Colours: the six accent colours (accent1 to accent6). They are what charts, shapes and tables take their colours
//! from, and they are what tells the built-in colour sets apart; the dark and light text and background colours are
//! white, black or a grey in most sets, so chips of them would look alike.
//!
//! Effects: a theme's effect scheme is three styles, subtle, moderate and intense (`effectStyleLst`), made of an outer or
//! inner shadow, a bevel, a glow, a reflection or a soft edge. The line shows one small square for each, with the
//! style's effect on it, so a flat scheme (Metro: no effects) and a glossy or glowing one are told apart without applying.

use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::Rgb;

use crate::palette::{doc, shade};

/// One preview of a menu line.
#[derive(Clone, Debug, PartialEq)]
pub enum LeadMark {
    /// A rounded square of a colour.
    Chip(Rgb),
    /// A character drawn in a typeface that has its glyph (a bullet symbol: `SYMBOL_FACE`), so that the system does not hand it to the colour emoji typeface
    /// (a menu line's text takes the variation selector U+FE0E for nothing: the heavy check mark ✔ was a purple emoji on Windows, GitHub issue #142).
    Glyph(char),
    /// A square of the neutral blue with an effect style on it.
    Effect(EffectSample),
}

/// The typeface of a `LeadMark::Glyph`: Windows' symbol typeface, as Word draws its bullet symbols; where it is not installed the system's own plain
/// typeface stands in (macOS, Linux, the Web).
pub(crate) const SYMBOL_FACE: &str = "Segoe UI Symbol";

/// What an effect style does to a shape, as much as a 14 px square can show.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EffectSample {
    /// An outer shadow: its blur and distance in points, and its opacity (0 to 1).
    pub(crate) outer_shadow: Option<(f32, f32, f32)>,
    /// An inner shadow: the same.
    pub inner_shadow: Option<(f32, f32, f32)>,
    /// A bevel (`sp3d` with `bevelT`).
    pub bevel: bool,
    /// A glow: its radius in points.
    pub glow: Option<f32>,
    /// A reflection under the shape.
    pub reflection: bool,
    /// A soft edge: its radius in points.
    pub soft_edge: Option<f32>,
}

/// A chip's side and the gap between marks.
const CHIP: f32 = 12.0;
const GAP: f32 = 3.0;
/// The room an effect square takes: its side, and what its shadow, glow and reflection need around it.
const EFFECT_CELL: f32 = 18.0;
const EFFECT_SQUARE: f32 = 12.0;
/// The colour an effect is shown on: the first accent colour of Office's own theme, so the shadow and the glow read against it.
const EFFECT_BASE: u32 = 0x4472c4;

impl EffectSample {
    /// The sample of one `a:effectStyle` of a theme's effect scheme (`ibis_pptx::theme_effects`).
    pub(crate) fn from_style_xml(xml: &str) -> Self {
        fn attr(xml: &str, tag: &str, name: &str) -> Option<f32> {
            let start = xml.find(&format!("<a:{tag}"))?;
            let rest = &xml[start..xml[start..].find('>').map_or(xml.len(), |e| start + e)];
            let at = rest.find(&format!("{name}=\""))? + name.len() + 2;
            rest[at..].split('"').next()?.parse().ok()
        }
        fn shadow(xml: &str, tag: &str) -> Option<(f32, f32, f32)> {
            let start = xml.find(&format!("<a:{tag}"))?;
            let end = xml[start..].find(&format!("</a:{tag}>")).map_or(xml.len(), |e| start + e);
            let part = &xml[start..end];
            let alpha = part.find("<a:alpha val=\"").and_then(|a| part[a + 14..].split('"').next()?.parse::<f32>().ok()).unwrap_or(100_000.0) / 100_000.0;
            // EMU to points (12700 EMU a point).
            Some((attr(part, tag, "blurRad")? / 12_700.0, attr(part, tag, "dist").unwrap_or(0.0) / 12_700.0, alpha))
        }
        EffectSample {
            outer_shadow: shadow(xml, "outerShdw"),
            inner_shadow: shadow(xml, "innerShdw"),
            bevel: xml.contains("<a:bevelT"),
            glow: attr(xml, "glow", "rad").map(|r| r / 12_700.0),
            reflection: xml.contains("<a:reflection"),
            soft_edge: attr(xml, "softEdge", "rad").map(|r| r / 12_700.0),
        }
    }
}

/// The six accent colours of a theme's colour set (`(name, colour)` as the presets list them).
pub fn accent_chips(colors: &[(&str, Rgb)]) -> Vec<LeadMark> {
    (1..=6).filter_map(|n| colors.iter().find(|(name, _)| name.eq_ignore_ascii_case(&format!("accent{n}"))).map(|(_, rgb)| LeadMark::Chip(*rgb))).collect()
}

/// The three samples (subtle, moderate, intense) of the effect scheme `id` of `ibis_pptx::theme_effects::THEME_EFFECTS`.
pub fn effect_samples(id: &str) -> Vec<LeadMark> {
    let Some(preset) = ibis_pptx::theme_effects::THEME_EFFECTS.iter().find(|p| p.id == id) else { return Vec::new() };
    [preset.subtle_effect, preset.moderate_effect, preset.intense_effect].into_iter().map(|xml| LeadMark::Effect(EffectSample::from_style_xml(xml))).collect()
}

/// The mark of a bullet symbol: the character in the symbol typeface.
pub fn bullet_glyph(symbol: &str) -> Vec<LeadMark> {
    symbol.chars().next().map(|c| vec![LeadMark::Glyph(c)]).unwrap_or_default()
}

/// How wide the marks are, for the width of a menu.
pub(crate) fn width(marks: &[LeadMark]) -> f32 {
    marks.iter().map(|m| match m {
        LeadMark::Chip(_) | LeadMark::Glyph(_) => CHIP,
        LeadMark::Effect(_) => EFFECT_CELL,
    }).sum::<f32>() + GAP * marks.len().saturating_sub(1) as f32
}

/// The marks in a row.
pub(crate) fn element(marks: &[LeadMark]) -> AnyElement {
    h_flex().flex_none().items_center().gap(px(GAP)).children(marks.iter().map(|m| match m {
        LeadMark::Chip(rgb) => div().w(px(CHIP)).h(px(CHIP)).flex_none().rounded(px(3.)).bg(doc(rgb.0)).border_1().border_color(shade(0.0, 0.2)).into_any_element(),
        LeadMark::Glyph(c) => div().w(px(CHIP + 2.0)).h(px(CHIP + 2.0)).flex_none().flex().items_center().justify_center().text_size(px(CHIP)).font_family(SYMBOL_FACE).child(c.to_string()).into_any_element(),
        LeadMark::Effect(sample) => effect_square(sample),
    })).into_any_element()
}

fn effect_square(sample: &EffectSample) -> AnyElement {
    let base: gpui::Hsla = doc(EFFECT_BASE).into();
    let mut shadows = Vec::new();
    if let Some((blur, dist, alpha)) = sample.outer_shadow {
        shadows.push(gpui::BoxShadow { color: gpui::hsla(0., 0., 0., alpha.min(1.0)), offset: point(px(0.), px(dist.min(3.0))), blur_radius: px(blur.clamp(1.0, 6.0)), spread_radius: px(0.), inset: false });
    }
    if let Some(glow) = sample.glow {
        shadows.push(gpui::BoxShadow { color: { let mut c = base; c.a = 0.55; c }, offset: point(px(0.), px(0.)), blur_radius: px(glow.clamp(2.0, 6.0)), spread_radius: px(0.), inset: false });
    }
    if let Some((blur, dist, alpha)) = sample.inner_shadow {
        // An inner shadow: dark along the upper edge, inside the square.
        shadows.push(gpui::BoxShadow { color: gpui::hsla(0., 0., 0., (alpha * 1.4).min(0.8)), offset: point(px(0.), px(dist.clamp(1.0, 3.0))), blur_radius: px(blur.clamp(1.0, 4.0)), spread_radius: px(0.), inset: true });
    }
    let mut square = div().relative().w(px(EFFECT_SQUARE)).h(px(EFFECT_SQUARE)).flex_none().rounded(px(2.)).bg(base).when(!shadows.is_empty(), |d| d.shadow(shadows.clone()));
    if let Some(edge) = sample.soft_edge {
        // A soft edge fades the border into the background: a white frame over the edge, thicker for a bigger radius.
        square = if edge > 3.0 { square.border_2() } else { square.border_1() }.border_color(gpui::hsla(0., 0., 1., 0.55));
    }
    if sample.bevel {
        // A bevel: light on the upper left edges and dark on the lower right ones.
        square = square.border_1().border_color(gpui::hsla(0., 0., 1., 0.0)).child(
            div().absolute().inset_0().rounded(px(2.)).border_t_2().border_l_2().border_color(gpui::hsla(0., 0., 1., 0.55)).child(
                div().absolute().inset_0().rounded(px(2.)).border_b_2().border_r_2().border_color(gpui::hsla(0., 0., 0., 0.35)),
            ),
        );
    }
    // The reflection is a faint copy under the square, so a cell is taller than the square when there is one.
    let reflection = sample.reflection.then(|| div().w(px(EFFECT_SQUARE)).h(px(4.)).mt(px(1.)).rounded_b(px(2.)).bg({ let mut c = base; c.a = 0.28; c }));
    div().w(px(EFFECT_CELL)).h(px(EFFECT_CELL)).flex_none().flex().flex_col().items_center().justify_center().child(square).children(reflection).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    // `use gpui_kit::*` in the parent brings gpui's own `test` attribute into scope through `use super::*`; the std one is named in full.
    #[core::prelude::v1::test]
    fn the_samples_read_the_effect_scheme() {
        let office = effect_samples("office");
        assert_eq!(office.len(), 3);
        let LeadMark::Effect(intense) = &office[2] else { panic!() };
        assert!(intense.bevel && intense.outer_shadow.is_some());
        let metro = effect_samples("metro");
        let LeadMark::Effect(flat) = &metro[0] else { panic!() };
        assert_eq!(*flat, EffectSample::default());
        let median = effect_samples("median");
        let LeadMark::Effect(soft) = &median[0] else { panic!() };
        assert!(soft.soft_edge.is_some());
    }

    #[core::prelude::v1::test]
    fn a_bullet_symbol_is_a_glyph_mark() {
        assert_eq!(bullet_glyph("✔"), vec![LeadMark::Glyph('✔')]);
        assert!(bullet_glyph("").is_empty());
        assert_eq!(width(&bullet_glyph("★")), CHIP);
    }

    #[core::prelude::v1::test]
    fn six_accent_chips() {
        let colors = [("dk1", Rgb(0)), ("accent1", Rgb(1)), ("accent2", Rgb(2)), ("accent3", Rgb(3)), ("accent4", Rgb(4)), ("accent5", Rgb(5)), ("accent6", Rgb(6))];
        assert_eq!(accent_chips(&colors).len(), 6);
    }
}
