// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! CMYK colours (ibisOffice's own; PowerPoint has none), for a file that has turned the CMYK mode on.
//!
//! The model keeps sRGB everywhere ([`crate::Rgb`]). In a file with the mode on, a colour the user chooses is first passed
//! through CMYK and back ([`CmykConverter::to_gamut`]), so that the sRGB value saved is the colour the press prints; the PDF
//! export turns the colours to CMYK again with the same converter. The file saves only the mode's flag and the profile's name.
//!
//! [`CmykConverter`] is the way between the two: [`OpenCmykConverter`] (a published model, no profile needed) is the default;
//! a converter made from an ICC profile (the app's `shell::cmyk_profile`) can take its place.

use crate::Rgb;

/// A CMYK colour, each ink in percent (0 to 100). `K` alone (`0,0,0,100`) and a rich black (`60,40,40,100`) are different colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Cmyk {
    pub c: u8,
    pub m: u8,
    pub y: u8,
    pub k: u8,
}

impl Cmyk {
    pub const fn new(c: u8, m: u8, y: u8, k: u8) -> Self {
        Self { c: clamp_percent(c), m: clamp_percent(m), y: clamp_percent(y), k: clamp_percent(k) }
    }

    /// The inks as an array `[c, m, y, k]`.
    pub const fn array(self) -> [u8; 4] {
        [self.c, self.m, self.y, self.k]
    }

    /// The inks as fractions, 0.0 to 1.0.
    pub fn fractions(self) -> [f64; 4] {
        [self.c as f64 / 100.0, self.m as f64 / 100.0, self.y as f64 / 100.0, self.k as f64 / 100.0]
    }

    /// A CMYK colour from fractions (rounded to whole percent, clamped to 0..1).
    pub fn from_fractions([c, m, y, k]: [f64; 4]) -> Self {
        let p = |v: f64| (v.clamp(0.0, 1.0) * 100.0).round() as u8;
        Self { c: p(c), m: p(m), y: p(y), k: p(k) }
    }
}

const fn clamp_percent(v: u8) -> u8 {
    if v > 100 { 100 } else { v }
}

/// The way between CMYK and sRGB. Implemented by [`OpenCmykConverter`] and by the converter of an ICC profile.
pub trait CmykConverter: Send + Sync {
    /// A name that tells the user which conversion is in use (the profile's name).
    fn name(&self) -> String;
    /// The sRGB colour `cmyk` is drawn as.
    fn to_rgb(&self, cmyk: Cmyk) -> Rgb;
    /// The sRGB colour inks given as fractions (0 to 1) are drawn as, without the rounding to whole percent of [`Cmyk`] (a picture's
    /// pixels: a gradient would show steps of a percent). The default rounds to [`Cmyk`]; a converter that can do better does.
    fn to_rgb_fractions(&self, ink: [f64; 4]) -> Rgb {
        self.to_rgb(Cmyk::from_fractions(ink))
    }
    /// A CMYK colour that is drawn as `rgb` (the nearest the press can make; a colour outside its gamut is clipped).
    fn to_cmyk(&self, rgb: Rgb) -> Cmyk;
    /// `rgb` as the press prints it: through CMYK and back (a colour outside what the inks make comes back inside). The black
    /// ink's share is the converter's own (its black generation).
    fn to_gamut(&self, rgb: Rgb) -> Rgb {
        self.to_rgb(self.to_cmyk(rgb))
    }
}

/// A published model of a coated offset press (the Neugebauer model: the colour of a patch is the mix of the 16 colours the inks
/// make at full strength, weighted by the area each takes, in linear light), with the dot gain that SWOP-like presses have.
/// It needs no profile; it is the default until the user chooses an ICC profile.
#[derive(Clone, Copy, Debug, Default)]
pub struct OpenCmykConverter;

/// Dot gain (tone value increase) at 50 %: 20 %, the figure of SWOP (ISO 12647-2's coated papers give 13 to 22 %). An area `a`
/// prints as `a + 4 * gain * a * (1 - a)`.
const DOT_GAIN_AT_HALF: f64 = 0.20;

/// The sRGB colours of the full-strength inks and their overprints on coated paper (`[c][m][y]`, 0 = off, 1 = full), the values
/// the SWOP press gives for the primaries as commonly published for sRGB.
const PRIMARIES: [[[[u8; 3]; 2]; 2]; 2] = [
    [[[255, 255, 255], [255, 242, 0]], [[236, 0, 140], [237, 28, 36]]],
    [[[0, 174, 239], [0, 166, 81]], [[46, 49, 146], [55, 52, 53]]],
];
/// Black at full strength on paper. Black ink over the other inks multiplies their light by this.
const BLACK_INK: [u8; 3] = [35, 31, 32];

fn to_linear(v: f64) -> f64 {
    let v = v / 255.0;
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn from_linear(l: f64) -> f64 {
    let l = l.clamp(0.0, 1.0);
    255.0 * if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }
}

fn gained(area: f64) -> f64 {
    (area + 4.0 * DOT_GAIN_AT_HALF * area * (1.0 - area)).clamp(0.0, 1.0)
}

/// The sRGB (0 to 255, not rounded) of inks given as fractions `[c, m, y, k]`.
fn forward(ink: [f64; 4]) -> [f64; 3] {
    let [c, m, y, k] = ink.map(gained);
    let mut out = [0.0; 3];
    for (ci, cw) in [(0, 1.0 - c), (1, c)] {
        for (mi, mw) in [(0, 1.0 - m), (1, m)] {
            for (yi, yw) in [(0, 1.0 - y), (1, y)] {
                let w = cw * mw * yw;
                for (o, p) in out.iter_mut().zip(PRIMARIES[ci][mi][yi]) {
                    *o += w * to_linear(p as f64);
                }
            }
        }
    }
    for (o, b) in out.iter_mut().zip(BLACK_INK) {
        *o = from_linear(*o * ((1.0 - k) + k * to_linear(b as f64)));
    }
    out
}

impl CmykConverter for OpenCmykConverter {
    fn name(&self) -> String {
        "Open (coated offset model)".into()
    }

    fn to_rgb(&self, cmyk: Cmyk) -> Rgb {
        self.to_rgb_fractions(cmyk.fractions())
    }

    fn to_rgb_fractions(&self, ink: [f64; 4]) -> Rgb {
        let [r, g, b] = forward(ink.map(|v| v.clamp(0.0, 1.0))).map(|v| v.round().clamp(0.0, 255.0) as u8);
        Rgb::from_rgb(r, g, b)
    }

    fn to_cmyk(&self, rgb: Rgb) -> Cmyk {
        let target = [rgb.r() as f64, rgb.g() as f64, rgb.b() as f64];
        // The black ink can take at most the darkness of the colour (its largest channel). For some shares of that, the other
        // three inks are found that give the colour (Newton's method on the model); the most black that fits as well as the
        // best does (within a unit of the 255 scale) is taken, as a press's black generation does.
        let kmax = (1.0 - target.iter().cloned().fold(0.0, f64::max) / 255.0).clamp(0.0, 1.0);
        let tries: Vec<(f64, [f64; 3], f64)> = (0..=10).map(|j| {
            let k = kmax * j as f64 / 10.0;
            let (x, err) = solve_cmy(target, k);
            (k, x, err)
        }).collect();
        let best = tries.iter().map(|t| t.2).fold(f64::MAX, f64::min);
        let (k, x, _) = tries.into_iter().rev().find(|t| t.2 <= best + 1.0).expect("a candidate");
        Cmyk::from_fractions([x[0], x[1], x[2], k])
    }
}

/// The cyan, magenta and yellow that give `target` (sRGB 0 to 255) with the black `k`, and how far (largest channel) they are off.
fn solve_cmy(target: [f64; 3], k: f64) -> ([f64; 3], f64) {
    let f = |x: [f64; 3]| forward([x[0], x[1], x[2], k]);
    // The first guess: the plain formula.
    let mut x = [0.0f64; 3];
    for i in 0..3 {
        x[i] = if k >= 1.0 { 0.0 } else { ((1.0 - target[i] / 255.0 - k) / (1.0 - k)).clamp(0.0, 1.0) };
    }
    let error = |x: [f64; 3]| {
        let now = f(x);
        (0..3).map(|i| (now[i] - target[i]).abs()).fold(0.0, f64::max)
    };
    for _ in 0..24 {
        let now = f(x);
        let err = [now[0] - target[0], now[1] - target[1], now[2] - target[2]];
        if err.iter().all(|e| e.abs() < 0.25) {
            break;
        }
        // The Jacobian by differences.
        let h = 1e-3;
        let mut jac = [[0.0f64; 3]; 3];
        for j in 0..3 {
            let mut xh = x;
            xh[j] = (xh[j] + h).min(1.0);
            let mut xl = x;
            xl[j] = (xl[j] - h).max(0.0);
            let (fh, fl) = (f(xh), f(xl));
            let d = xh[j] - xl[j];
            for i in 0..3 {
                jac[i][j] = (fh[i] - fl[i]) / d;
            }
        }
        let Some(step) = solve3(jac, err) else { break };
        let mut moved = false;
        for j in 0..3 {
            let nx = (x[j] - step[j]).clamp(0.0, 1.0);
            if (nx - x[j]).abs() > 1e-6 {
                moved = true;
            }
            x[j] = nx;
        }
        if !moved {
            break;
        }
    }
    (x, error(x))
}

/// Solves `a * x = b` (Cramer's rule; `None` when `a` is singular).
fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(a);
    if d.abs() < 1e-9 {
        return None;
    }
    let mut out = [0.0; 3];
    for (j, o) in out.iter_mut().enumerate() {
        let mut m = a;
        for i in 0..3 {
            m[i][j] = b[i];
        }
        *o = det(m) / d;
    }
    Some(out)
}

/// A file's CMYK mode: whether it is on and the profile it was made with (the name and an identifier only; the profile is
/// not embedded). The colours themselves stay sRGB: in a file with the mode on they are kept inside what the press can print
/// ([`CmykConverter::to_gamut`]), so what is seen is what prints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CmykMode {
    pub enabled: bool,
    /// The profile the file was made with (`name|identifier`; `None`: the open model).
    pub profile: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_is_paper_and_k100_is_the_black_ink() {
        let c = OpenCmykConverter;
        assert_eq!(c.to_rgb(Cmyk::new(0, 0, 0, 0)), Rgb::WHITE);
        assert_eq!(c.to_rgb(Cmyk::new(0, 0, 0, 100)), Rgb::from_rgb(35, 31, 32));
        assert_eq!(c.to_rgb(Cmyk::new(100, 0, 0, 0)), Rgb::from_rgb(0, 174, 239));
        assert_eq!(c.to_rgb(Cmyk::new(0, 100, 100, 0)), Rgb::from_rgb(237, 28, 36));
    }

    #[test]
    fn rich_black_is_darker_than_k100() {
        let c = OpenCmykConverter;
        let k = c.to_rgb(Cmyk::new(0, 0, 0, 100));
        let rich = c.to_rgb(Cmyk::new(60, 40, 40, 100));
        assert!(rich.r() < k.r() && rich.g() < k.g() && rich.b() < k.b());
    }

    #[test]
    fn k50_has_the_dot_gain_of_a_press() {
        // 50 % black ink prints about mid-dark grey (well below the 188 a gainless mix would give).
        let g = OpenCmykConverter.to_rgb(Cmyk::new(0, 0, 0, 50)).r();
        assert!((140..=170).contains(&g), "{g}");
    }

    #[test]
    fn the_way_back_finds_colours_the_press_can_make() {
        let c = OpenCmykConverter;
        for cmyk in [Cmyk::new(40, 20, 60, 10), Cmyk::new(80, 10, 30, 0), Cmyk::new(0, 90, 70, 20), Cmyk::new(10, 10, 10, 60)] {
            let rgb = c.to_rgb(cmyk);
            let back = c.to_rgb(c.to_cmyk(rgb));
            for (a, b) in [(rgb.r(), back.r()), (rgb.g(), back.g()), (rgb.b(), back.b())] {
                assert!((a as i32 - b as i32).abs() <= 4, "{cmyk:?} {rgb:?} {back:?}");
            }
        }
    }

    #[test]
    fn a_vivid_green_comes_back_inside_what_the_press_prints() {
        let c = OpenCmykConverter;
        let green = Rgb::from_rgb(0, 255, 0);
        let inside = c.to_gamut(green);
        assert_ne!(inside, green);
        // The press's green is duller (the green falls, some blue comes in), and printing it again changes nothing.
        assert!(inside.g() < 255 && inside.b() > 0, "{inside:?}");
        let again = c.to_gamut(inside);
        for (a, b) in [(inside.r(), again.r()), (inside.g(), again.g()), (inside.b(), again.b())] {
            assert!((a as i32 - b as i32).abs() <= 3, "{inside:?} {again:?}");
        }
        // Paper stays paper.
        assert_eq!(c.to_gamut(Rgb::WHITE), Rgb::WHITE);
    }
}
