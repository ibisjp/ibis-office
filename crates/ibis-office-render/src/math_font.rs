// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What a formula's typesetting takes from the maths font: the constants of its OpenType MATH table (ECMA-376 leaves the
//! numbers to the font; the OpenType specification says what each one means), and the shapes of its glyphs (their ink,
//! their advance, the italic correction, the variants a sign grows through).
//!
//! The font is the one that draws the formula: Cambria Math when it is there (the test fonts of `IBIS_EXTRA_FONT_DIRS`, or
//! Windows), else STIX Two Math, which macOS has. A font that cannot be read gives the numbers of STIX Two Math (below).

use std::sync::{Mutex, OnceLock};

use ttf_parser::{Face, GlyphId};

/// The font's constants, in em (of its size), and the face they were read from.
pub(crate) struct MathMetrics {
    face: Option<Face<'static>>,
    pub script: f32,
    pub axis: f32,
    pub rule: f32,
    pub(crate) num_up: f32,
    pub(crate) den_down: f32,
    pub(crate) frac_gap: f32,
    pub(crate) sup_up: f32,
    pub(crate) sup_up_cramped: f32,
    pub(crate) sub_down: f32,
    pub(crate) sub_top_max: f32,
    pub(crate) sup_bottom_min: f32,
    pub(crate) sup_drop: f32,
    pub(crate) sub_drop: f32,
    pub(crate) sub_sup_gap: f32,
    pub(crate) script_space: f32,
    pub(crate) rad_gap: f32,
    pub(crate) rad_rule: f32,
    pub(crate) upper_rise: f32,
    pub(crate) lower_drop: f32,
    pub(crate) upper_gap: f32,
    pub(crate) lower_gap: f32,
    pub leading: f32,
    pub ascent: f32,
    pub descent: f32,
    pub(crate) display_operator_min: f32,
    pub(crate) accent_base_height: f32,
    pub(crate) delimited_min: f32,
}

/// The numbers of STIX Two Math (OFL-1.1, https://github.com/stipub/stixfonts): its OpenType MATH table, in em. For when no
/// maths font can be read.
const DEFAULT: MathMetrics = MathMetrics {
    face: None,
    script: 0.70,
    axis: 0.258,
    rule: 0.068,
    num_up: 0.64,
    den_down: 0.64,
    frac_gap: 0.15,
    sup_up: 0.36,
    sup_up_cramped: 0.252,
    sub_down: 0.21,
    sub_top_max: 0.368,
    sup_bottom_min: 0.12,
    sup_drop: 0.23,
    sub_drop: 0.16,
    sub_sup_gap: 0.15,
    script_space: 0.04,
    rad_gap: 0.085,
    rad_rule: 0.068,
    upper_rise: 0.30,
    lower_drop: 0.67,
    upper_gap: 0.135,
    lower_gap: 0.135,
    leading: 0.15,
    ascent: 0.762,
    descent: 0.238,
    display_operator_min: 1.8,
    accent_base_height: 0.48,
    delimited_min: 1.325,
};

/// How a large operator (∑, ∫) is drawn: the font's own glyph scaled by `scale` of the size, its ink (em of the glyph, at its
/// size) reaching `below` and `above` the baseline and `right` of its origin; `centre` is where the ink of the variant the font
/// grows the sign to is centred, above the baseline (em of the size).
#[derive(Clone, Copy, Debug)]
pub(crate) struct BigOperator {
    pub scale: f32,
    pub below: f32,
    pub above: f32,
    pub right: f32,
    pub centre: f32,
}

/// Font files offered by the test fonts (`IBIS_EXTRA_FONT_DIRS`), before the metrics are first asked for.
static OFFERED: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());
static METRICS: OnceLock<MathMetrics> = OnceLock::new();

/// A font file given to the text system: kept when it is a maths font, to take the numbers from (ahead of the system's).
#[cfg(not(target_family = "wasm"))]
pub(crate) fn offer_font(data: &[u8]) {
    if METRICS.get().is_none() && has_math_table(data) {
        OFFERED.lock().unwrap_or_else(|e| e.into_inner()).push(data.to_vec());
    }
}

#[cfg(not(target_family = "wasm"))]
fn has_math_table(data: &[u8]) -> bool {
    (0..ttf_parser::fonts_in_collection(data).unwrap_or(1)).any(|i| Face::parse(data, i).is_ok_and(|f| f.tables().math.is_some()))
}

/// The maths font's numbers (read once).
pub fn metrics() -> &'static MathMetrics {
    METRICS.get_or_init(|| {
        #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
        let mut candidates: Vec<Vec<u8>> = std::mem::take(&mut *OFFERED.lock().unwrap_or_else(|e| e.into_inner()));
        #[cfg(target_os = "macos")]
        candidates.extend(std::fs::read("/System/Library/Fonts/Supplemental/STIXTwoMath.otf").ok());
        #[cfg(windows)]
        candidates.extend(std::fs::read("C:\\Windows\\Fonts\\cambria.ttc").ok());
        // Linux: the maths fonts its distributions package (fonts-stix,
        // fonts-dejavu-extra, fonts-lmodern), where they put them.
        #[cfg(target_os = "linux")]
        for path in [
            "/usr/share/fonts/opentype/stix/STIXTwoMath-Regular.otf",
            "/usr/share/fonts/truetype/dejavu/DejaVuMathTeXGyre.ttf",
            "/usr/share/texmf/fonts/opentype/public/lm-math/latinmodern-math.otf",
        ] {
            candidates.extend(std::fs::read(path).ok());
        }
        for data in candidates {
            let data: &'static [u8] = Box::leak(data.into_boxed_slice());
            for index in 0..ttf_parser::fonts_in_collection(data).unwrap_or(1) {
                if let Ok(face) = Face::parse(data, index)
                    && let Some(read) = MathMetrics::read(face)
                {
                    return read;
                }
            }
        }
        DEFAULT
    })
}

impl MathMetrics {
    fn read(face: Face<'static>) -> Option<Self> {
        let math = face.tables().math?;
        let c = math.constants?;
        let upm = f32::from(face.units_per_em());
        let em = |v: i16| f32::from(v) / upm;
        Some(Self {
            script: f32::from(c.script_percent_scale_down()) / 100.0,
            axis: em(c.axis_height().value),
            rule: em(c.fraction_rule_thickness().value),
            num_up: em(c.fraction_numerator_display_style_shift_up().value),
            den_down: em(c.fraction_denominator_display_style_shift_down().value),
            frac_gap: em(c.fraction_num_display_style_gap_min().value),
            sup_up: em(c.superscript_shift_up().value),
            sup_up_cramped: em(c.superscript_shift_up_cramped().value),
            sub_down: em(c.subscript_shift_down().value),
            sub_top_max: em(c.subscript_top_max().value),
            sup_bottom_min: em(c.superscript_bottom_min().value),
            sup_drop: em(c.superscript_baseline_drop_max().value),
            sub_drop: em(c.subscript_baseline_drop_min().value),
            sub_sup_gap: em(c.sub_superscript_gap_min().value),
            script_space: em(c.space_after_script().value),
            rad_gap: em(c.radical_vertical_gap().value),
            rad_rule: em(c.radical_rule_thickness().value),
            upper_rise: em(c.upper_limit_baseline_rise_min().value),
            lower_drop: em(c.lower_limit_baseline_drop_min().value),
            upper_gap: em(c.upper_limit_gap_min().value),
            lower_gap: em(c.lower_limit_gap_min().value),
            leading: em(c.math_leading().value),
            ascent: f32::from(face.ascender()) / upm,
            descent: -f32::from(face.descender()) / upm,
            display_operator_min: f32::from(c.display_operator_min_height()) / upm,
            accent_base_height: em(c.accent_base_height().value),
            delimited_min: f32::from(c.delimited_sub_formula_min_height()) / upm,
            face: Some(face),
        })
    }

    fn glyph(&self, c: char) -> Option<(&Face<'static>, GlyphId)> {
        let face = self.face.as_ref()?;
        Some((face, face.glyph_index(c)?))
    }

    /// How far the ink of `c` reaches below and above the baseline (em), when the font has the glyph.
    pub fn ink(&self, c: char) -> Option<(f32, f32)> {
        let (face, g) = self.glyph(c)?;
        let r = face.glyph_bounding_box(g)?;
        let upm = f32::from(face.units_per_em());
        Some((-f32::from(r.y_min) / upm, f32::from(r.y_max) / upm))
    }

    /// How far the ink of `c` reaches left and right of its origin (em), when the font has the glyph.
    pub(crate) fn ink_x(&self, c: char) -> Option<(f32, f32)> {
        let (face, g) = self.glyph(c)?;
        let r = face.glyph_bounding_box(g)?;
        let upm = f32::from(face.units_per_em());
        Some((f32::from(r.x_min) / upm, f32::from(r.x_max) / upm))
    }

    /// The advance of `c` (em), when the font has the glyph.
    pub fn advance(&self, c: char) -> Option<f32> {
        let (face, g) = self.glyph(c)?;
        Some(f32::from(face.glyph_hor_advance(g)?) / f32::from(face.units_per_em()))
    }

    /// How far the slant of `c` leans over what follows it (em; the MATH table's italic correction).
    pub(crate) fn italic_correction(&self, c: char) -> f32 {
        let Some((face, g)) = self.glyph(c) else { return 0.0 };
        let upm = f32::from(face.units_per_em());
        face.tables()
            .math
            .and_then(|m| m.glyph_info)
            .and_then(|i| i.italic_corrections)
            .and_then(|v| v.get(g))
            .map_or(0.0, |v| f32::from(v.value) / upm)
    }

    /// By how much the glyph of the bracket `c` is scaled to be the font's variant that is the first at least `height` (em of
    /// the size) high: the variant's ink height over the glyph's.
    pub(crate) fn delimiter_scale(&self, c: char, height: f32) -> Option<f32> {
        let (face, g) = self.glyph(c)?;
        let upm = f32::from(face.units_per_em());
        let base = face.glyph_bounding_box(g)?;
        let base_h = f32::from(base.y_max - base.y_min) / upm;
        let construction = face.tables().math?.variants?.vertical_constructions.get(g)?;
        let variant = construction.variants.into_iter().find(|v| f32::from(v.advance_measurement) >= height * upm).or_else(|| construction.variants.into_iter().last())?;
        let r = face.glyph_bounding_box(variant.variant_glyph)?;
        Some(f32::from(r.y_max - r.y_min) / upm / base_h)
    }

    /// How the large operator `c` is drawn in display style: the font's glyph scaled to the variant the font grows it to
    /// (the first one at least 1.5 times `display_operator_min` high), by the mean of the scales that give the variant's height and its
    /// width (the variant is narrower than the scaled glyph).
    pub(crate) fn big_operator(&self, c: char) -> Option<BigOperator> {
        let (face, g) = self.glyph(c)?;
        let upm = f32::from(face.units_per_em());
        let base = face.glyph_bounding_box(g)?;
        let (base_w, base_h) = (f32::from(base.x_max - base.x_min) / upm, f32::from(base.y_max - base.y_min) / upm);
        let construction = face.tables().math?.variants?.vertical_constructions.get(g)?;
        // PowerPoint takes the variant at least 1.5 times the font's display-operator height (measured on its formulas).
        let min = self.display_operator_min * upm * 1.5;
        let variant = construction.variants.into_iter().find(|v| f32::from(v.advance_measurement) >= min).or_else(|| construction.variants.into_iter().last())?;
        let r = face.glyph_bounding_box(variant.variant_glyph)?;
        let (w, h) = (f32::from(r.x_max - r.x_min) / upm, f32::from(r.y_max - r.y_min) / upm);
        let scale = (h / base_h + w / base_w) / 2.0;
        Some(BigOperator {
            scale,
            below: -f32::from(base.y_min) / upm,
            above: f32::from(base.y_max) / upm,
            right: f32::from(base.x_max) / upm,
            centre: f32::from(r.y_min + r.y_max) / 2.0 / upm,
        })
    }
}

