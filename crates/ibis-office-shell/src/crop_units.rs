// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The units of the numeric crop of a picture (トリミング) and the arithmetic between them, shared by the screens that
//! crop with numbers (the presentation and workbook screens' crop panel).
//!
//! A crop side is a fraction of the SOURCE picture (the file's `a:srcRect`, in 1/1000 %). In a unit other than a percent the
//! amount is that fraction of the source picture's natural size: its pixels, 1 px = 1/96 in (CSS pixel; 9525 EMU, the size a
//! picture without a resolution of its own has in Office), 1 pt = 1/72 in, 1 in = 25.4 mm.

/// The unit a crop side is typed and shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CropUnit {
    #[default]
    Percent,
    Millimeters,
    Inches,
    Points,
    Pixels,
}

impl CropUnit {
    pub const ALL: [CropUnit; 5] = [Self::Percent, Self::Millimeters, Self::Inches, Self::Points, Self::Pixels];

    /// The mark after a value (the same in every language: a symbol of the unit).
    pub fn abbreviation(self) -> &'static str {
        match self {
            Self::Percent => "%",
            Self::Millimeters => "mm",
            Self::Inches => "inch",
            Self::Points => "pt",
            Self::Pixels => "px",
        }
    }

    /// Pixels (96 per inch) in one of this unit; `None` for a percent (a fraction, not a length).
    fn pixels_per_unit(self) -> Option<f64> {
        match self {
            Self::Percent => None,
            Self::Millimeters => Some(96. / 25.4),
            Self::Inches => Some(96.),
            Self::Points => Some(96. / 72.),
            Self::Pixels => Some(1.),
        }
    }

    /// The step of the ▲▼ of a field in this unit.
    pub fn step(self) -> f64 {
        match self {
            Self::Inches => 0.1,
            _ => 1.,
        }
    }

    fn decimals(self) -> usize {
        match self {
            Self::Percent | Self::Inches => 3,
            Self::Millimeters | Self::Points => 2,
            Self::Pixels => 1,
        }
    }

    /// The fraction of a side of `source_px` pixels that `amount` of this unit is.
    pub fn fraction(self, amount: f64, source_px: f64) -> f64 {
        match self.pixels_per_unit() {
            None => amount / 100.,
            Some(px) => amount * px / source_px.max(1.),
        }
    }

    /// The amount of this unit that `fraction` of a side of `source_px` pixels is.
    pub fn amount(self, fraction: f64, source_px: f64) -> f64 {
        match self.pixels_per_unit() {
            None => fraction * 100.,
            Some(px) => fraction * source_px.max(1.) / px,
        }
    }

    /// An amount as typed in a field: its decimals, without trailing zeros.
    pub fn format(self, amount: f64) -> String {
        let text = format!("{:.*}", self.decimals(), amount);
        let text = if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text };
        if text == "-0" { "0".to_string() } else { text }
    }
}

/// The size left of a source picture of `source` (width, height in pixels) after cropping the fractions
/// `[left, top, right, bottom]`, in `unit` (a percent: of the source's size).
pub fn cropped_size(unit: CropUnit, source: (f64, f64), fractions: [f64; 4]) -> (f64, f64) {
    let (w, h) = (1. - fractions[0] - fractions[2], 1. - fractions[1] - fractions[3]);
    (unit.amount(w, source.0), unit.amount(h, source.1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn units_convert_through_the_source_pixels() {
        // A 960 x 480 px picture (10 in x 5 in at 96 dpi): 1 inch is 10 % of its width, and 96 px, 72 pt, 25.4 mm.
        let width = 960.;
        for (unit, amount) in [(CropUnit::Inches, 1.), (CropUnit::Pixels, 96.), (CropUnit::Points, 72.), (CropUnit::Millimeters, 25.4), (CropUnit::Percent, 10.)] {
            let fraction = unit.fraction(amount, width);
            assert!((fraction - 0.1).abs() < 1e-12, "{unit:?}: {fraction}");
            assert!((unit.amount(fraction, width) - amount).abs() < 1e-9);
        }
    }

    #[core::prelude::v1::test]
    fn the_size_left_is_the_source_less_the_sides() {
        let (w, h) = cropped_size(CropUnit::Pixels, (800., 600.), [0.1, 0.25, 0.15, 0.]);
        assert!((w - 600.).abs() < 1e-9 && (h - 450.).abs() < 1e-9, "{w} x {h}");
        let (w, h) = cropped_size(CropUnit::Percent, (800., 600.), [0.1, 0.25, 0.15, 0.]);
        assert!((w - 75.).abs() < 1e-9 && (h - 75.).abs() < 1e-9);
    }

    #[core::prelude::v1::test]
    fn amounts_are_written_without_trailing_zeros() {
        assert_eq!(CropUnit::Percent.format(10.), "10");
        assert_eq!(CropUnit::Millimeters.format(2.5), "2.5");
        assert_eq!(CropUnit::Inches.format(0.125), "0.125");
        assert_eq!(CropUnit::Pixels.format(-0.04), "0");
    }
}
