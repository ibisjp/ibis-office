// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! How shapes look: colours, fills, outlines and geometry.

use crate::geometry::Emu;
use crate::model::MediaId;
use crate::shape_geometry::FormulaGeometry;

/// A 24-bit sRGB colour, `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u32);

impl Rgb {
    pub const BLACK: Self = Self(0x000000);
    pub const WHITE: Self = Self(0xffffff);

    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self(((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }

    pub const fn r(self) -> u8 {
        (self.0 >> 16) as u8
    }

    pub const fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }

    pub const fn b(self) -> u8 {
        self.0 as u8
    }

    /// Excel's tint: towards white (positive) or black (negative) in HLS lightness.
    /// `tint_millionths`: -1_000_000..=1_000_000.
    pub fn tinted(self, tint_millionths: i32) -> Self {
        if tint_millionths == 0 {
            return self;
        }
        let tint = tint_millionths as f64 / 1_000_000.0;
        let (r, g, b) = (self.r() as f64 / 255.0, self.g() as f64 / 255.0, self.b() as f64 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let l = (max + min) / 2.0;
        let (h, s) = if max == min {
            (0.0, 0.0)
        } else {
            let d = max - min;
            let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
            let h = if max == r {
                (g - b) / d + if g < b { 6.0 } else { 0.0 }
            } else if max == g {
                (b - r) / d + 2.0
            } else {
                (r - g) / d + 4.0
            } / 6.0;
            (h, s)
        };
        let l = if tint < 0.0 { l * (1.0 + tint) } else { l * (1.0 - tint) + tint };
        let l = l.clamp(0.0, 1.0);
        let (r, g, b) = if s == 0.0 {
            (l, l, l)
        } else {
            let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
            let p = 2.0 * l - q;
            let channel = |t: f64| {
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
            (channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0))
        };
        let byte = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u32;
        Self(byte(r) << 16 | byte(g) << 8 | byte(b))
    }
}

/// A reflection under a shape (`a:reflection`), as the file has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reflection {
    /// Blur radius, EMU (`blurRad`).
    pub blur: i64,
    /// Opacity at the start and end of the fade, 1/1000 % (`stA`, `endA`).
    pub start_alpha: i32,
    pub end_alpha: i32,
    /// Where the fade starts and ends, 1/1000 % of the shape (`stPos`, `endPos`).
    pub start_pos: i32,
    pub end_pos: i32,
    /// Distance from the shape, EMU (`dist`), and its direction, 1/60000 ° (`dir`).
    pub distance: i64,
    pub direction: i32,
    /// Direction of the fade, 1/60000 ° (`fadeDir`).
    pub fade_direction: i32,
    /// Scale, 1/1000 % (`sx`, `sy`; `sy` −100 000 mirrors it), and skew,
    /// 1/60000 ° (`kx`, `ky`).
    pub sx: i32,
    pub sy: i32,
    pub kx: i32,
    pub ky: i32,
    /// Alignment (`algn`).
    pub align: TileAlign,
    /// Whether it turns with the shape (`rotWithShape`).
    pub rot_with_shape: bool,
}

impl Reflection {
    /// PowerPoint's standard reflection preset with distance and end position.
    pub fn standard(distance: i64, end_pos: i32) -> Self {
        Self {
            blur: 6_350,
            start_alpha: 52_000,
            end_alpha: 300,
            start_pos: 0,
            end_pos,
            distance,
            direction: 5_400_000,
            fade_direction: 5_400_000,
            sx: 100_000,
            sy: -100_000,
            kx: 0,
            ky: 0,
            align: TileAlign::BottomLeft,
            rot_with_shape: false,
        }
    }
}

/// A drop shadow behind a shape (`a:outerShdw`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shadow {
    pub color: Color,
    /// Blur radius, in EMU.
    pub blur: i64,
    /// Offset from the shape, in EMU, towards `direction`.
    pub distance: i64,
    /// In 60 000ths of a degree, clockwise from the right.
    pub direction: i32,
    /// Horizontal scaling factor in 100 000ths (default 100 000 = 100%).
    pub sx: i32,
    /// Vertical scaling factor in 100 000ths (default 100 000 = 100%).
    pub sy: i32,
    /// Horizontal skew angle in 60 000ths of a degree (default 0).
    pub kx: i32,
    /// Vertical skew angle in 60 000ths of a degree (default 0).
    pub ky: i32,
    /// Shadow alignment (`ST_RectAlignment`).
    pub align: TileAlign,
    /// Whether the shadow rotates with the shape (`rotWithShape`).
    pub rot_with_shape: bool,
}

impl Default for Shadow {
    fn default() -> Self {
        Self::simple(Color::BLACK, 0, 0, 0)
    }
}

impl Shadow {
    pub fn simple(color: Color, blur: i64, distance: i64, direction: i32) -> Self {
        Self {
            color,
            blur,
            distance,
            direction,
            sx: 100_000,
            sy: 100_000,
            kx: 0,
            ky: 0,
            align: TileAlign::Bottom,
            rot_with_shape: true,
        }
    }

    pub fn has_transform(&self) -> bool {
        self.sx != 100_000 || self.sy != 100_000 || self.kx != 0 || self.ky != 0
    }
}

/// A colour with opacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    pub rgb: Rgb,
    /// 0 is transparent, 255 opaque.
    pub alpha: u8,
}

impl Color {
    pub const BLACK: Self = Self::opaque(Rgb::BLACK);
    pub const WHITE: Self = Self::opaque(Rgb::WHITE);

    pub const fn opaque(rgb: Rgb) -> Self {
        Self { rgb, alpha: 255 }
    }
}

/// One colour of a gradient, at `position` along it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GradientStop {
    /// 0 to 100 000, as OOXML's `pos` (thousandths of a percent).
    pub position: u32,
    pub color: Color,
}

/// How the inside of a shape or a slide background is painted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Fill {
    #[default]
    None,
    Solid(Color),
    /// A linear gradient. `angle` is in 60 000ths of a degree, clockwise from
    /// left-to-right, as OOXML's `a:lin ang`. `scaled` indicates whether the
    /// angle scales with the shape's aspect ratio (`a:lin scaled="1"`).
    Linear {
        angle: i32,
        scaled: bool,
        stops: Vec<GradientStop>,
    },
    /// A linear gradient tiled across the shape (`a:gradFill` with `a:tileRect`).
    TileLinear {
        angle: i32,
        scaled: bool,
        stops: Vec<GradientStop>,
        tile: [i32; 4],
        flip: TileFlip,
    },
    /// A picture stretched over the area.
    Image(MediaId),
    /// A picture tiled over the area (`a:blipFill` with `a:tile`).
    TileImage {
        media: MediaId,
        tx: i64,
        ty: i64,
        sx: i32,
        sy: i32,
        flip: TileFlip,
        align: TileAlign,
    },
    /// A picture with the adjustments `a:blipFill` can carry: a range of the picture (`a:srcRect`),
    /// the place it fills (`a:fillRect`), colour effects, or tiling. A plain stretched or tiled
    /// picture is [`Fill::Image`] or [`Fill::TileImage`].
    Blip(Box<BlipFill>),
    /// A two-colour pattern (`a:pattFill`): `preset` is its name
    /// (`ST_PresetPatternVal`, such as `"pct5"`), drawn as an 8 × 8 tile of
    /// 1 pt cells (see [`pattern_tile`]) anchored at the slide's corner.
    Pattern { preset: String, foreground: Color, background: Color },
    /// A path gradient (`a:gradFill` with `a:path`): the first stop at
    /// `focus` (`a:fillToRect`: its left, top, right and bottom insets from
    /// the shape's box, 0 to 100 000), the last at the edge of `path`.
    Path { path: GradientPath, focus: [i32; 4], stops: Vec<GradientStop> },
}

impl Fill {
    /// The picture this fill shows, if it is a picture fill.
    pub fn media(&self) -> Option<MediaId> {
        match self {
            Fill::Image(media) | Fill::TileImage { media, .. } => Some(*media),
            Fill::Blip(blip) => Some(blip.media),
            _ => None,
        }
    }
}

/// A picture fill with its adjustments (`a:blipFill`): see [`Fill::Blip`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlipFill {
    pub media: MediaId,
    /// The part of the picture shown (`a:srcRect`).
    pub crop: crate::Crop,
    /// The area the picture covers, as insets from the frame in 100 000ths (`a:stretch/a:fillRect`; negative runs outside).
    pub fill_rect: Option<[i32; 4]>,
    /// Tiling (`a:tile`) instead of stretching.
    pub tile: Option<crate::PictureTile>,
    /// Dark and light colours of a duotone (`a:duotone`).
    pub duotone: Option<(crate::Rgb, crate::Rgb)>,
    /// Brightness and contrast (`a:lum`), 100 000ths.
    pub luminance: Option<(i32, i32)>,
    /// Grey, black and white, one colour, opacity (`a:blip`'s other children).
    pub effects: crate::BlipEffects,
    /// Whether the picture turns with the shape (`a:blipFill rotWithShape`; `false`: it stays upright and the turned outline cuts it).
    pub rot_with_shape: bool,
}

impl BlipFill {
    /// A fill that shows the picture as is.
    pub fn new(media: MediaId) -> Self {
        Self { media, crop: crate::Crop::default(), fill_rect: None, tile: None, duotone: None, luminance: None, effects: crate::BlipEffects::default(), rot_with_shape: true }
    }

    /// Whether it is just the picture stretched or tiled (so a [`Fill::Image`] or [`Fill::TileImage`] says it all).
    pub fn is_plain(&self) -> bool {
        self.rot_with_shape && self.crop == crate::Crop::default() && self.fill_rect.is_none() && self.duotone.is_none() && self.luminance.is_none() && self.effects.is_none()
    }
}

/// How a tiled picture flips on alternating tiles (`ST_TileFlipMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TileFlip {
    #[default]
    None,
    X,
    Y,
    Xy,
}

impl TileFlip {
    pub fn parse(name: &str) -> Self {
        match name {
            "x" => Self::X,
            "y" => Self::Y,
            "xy" => Self::Xy,
            _ => Self::None,
        }
    }
}

/// How a tiled picture aligns to its frame (`ST_RectAlignment`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TileAlign {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl TileAlign {
    pub fn parse(name: &str) -> Self {
        match name {
            "tl" => Self::TopLeft,
            "t" => Self::Top,
            "tr" => Self::TopRight,
            "l" => Self::Left,
            "ctr" => Self::Center,
            "r" => Self::Right,
            "bl" => Self::BottomLeft,
            "b" => Self::Bottom,
            "br" => Self::BottomRight,
            _ => Self::TopLeft,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::TopLeft => "tl",
            Self::Top => "t",
            Self::TopRight => "tr",
            Self::Left => "l",
            Self::Center => "ctr",
            Self::Right => "r",
            Self::BottomLeft => "bl",
            Self::Bottom => "b",
            Self::BottomRight => "br",
        }
    }
}

/// The outline a path gradient spreads to (`ST_PathShadeType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GradientPath {
    Circle,
    Rect,
    Shape,
}

impl GradientPath {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "circle" => Self::Circle,
            "rect" => Self::Rect,
            "shape" => Self::Shape,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::Rect => "rect",
            Self::Shape => "shape",
        }
    }
}

/// The 8 × 8 tile of pattern `preset`: a row per byte, top to bottom, the
/// high bit the leftmost cell; a set bit is the foreground colour. `None`
/// for an unknown name.
///
/// The tiles were read off the patterns PowerPoint draws (its PDF of the
/// coverage suite PFIL, 1 pt per cell), one cell at a time.
pub fn pattern_tile(preset: &str) -> Option<[u8; 8]> {
    let rows: [u8; 8] = match preset {
        "pct5" => [0x80, 0, 0, 0, 0x08, 0, 0, 0],
        "pct10" => [0x80, 0, 0x08, 0, 0x80, 0, 0x08, 0],
        "pct20" => [0x88, 0, 0x22, 0, 0x88, 0, 0x22, 0],
        "pct25" => [0x88, 0x22, 0x88, 0x22, 0x88, 0x22, 0x88, 0x22],
        "pct30" => [0xaa, 0x44, 0xaa, 0x11, 0xaa, 0x44, 0xaa, 0x11],
        "pct40" => [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x15],
        "pct50" => [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
        "pct60" => [0xee, 0x55, 0xbb, 0x55, 0xee, 0x55, 0xbb, 0x55],
        "pct70" => [0x77, 0xdd, 0x77, 0xdd, 0x77, 0xdd, 0x77, 0xdd],
        "pct75" => [0x77, 0xff, 0xdd, 0xff, 0x77, 0xff, 0xdd, 0xff],
        "pct80" => [0xf7, 0xff, 0x7f, 0xff, 0xf7, 0xff, 0x7f, 0xff],
        "pct90" => [0x7f, 0xff, 0xff, 0xff, 0xf7, 0xff, 0xff, 0xff],
        "horz" => [0xff, 0, 0, 0, 0, 0, 0, 0],
        "vert" => [0x80; 8],
        "ltHorz" => [0xff, 0, 0, 0, 0xff, 0, 0, 0],
        "ltVert" => [0x88; 8],
        "dkHorz" => [0xff, 0xff, 0, 0, 0xff, 0xff, 0, 0],
        "dkVert" => [0xcc; 8],
        "narHorz" => [0xff, 0, 0xff, 0, 0xff, 0, 0xff, 0],
        "narVert" => [0xaa; 8],
        "dashHorz" => [0xf0, 0, 0, 0, 0x0f, 0, 0, 0],
        "dashVert" => [0x80, 0x80, 0x80, 0x80, 0x08, 0x08, 0x08, 0x08],
        "cross" | "lgGrid" => [0xff, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80],
        "smGrid" => [0xff, 0x88, 0x88, 0x88, 0xff, 0x88, 0x88, 0x88],
        "dotGrid" => [0xaa, 0, 0x80, 0, 0x80, 0, 0x80, 0],
        "dnDiag" => [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01],
        "upDiag" => [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80],
        "ltDnDiag" => [0x88, 0x44, 0x22, 0x11, 0x88, 0x44, 0x22, 0x11],
        "ltUpDiag" => [0x11, 0x22, 0x44, 0x88, 0x11, 0x22, 0x44, 0x88],
        "dkDnDiag" => [0xcc, 0x66, 0x33, 0x99, 0xcc, 0x66, 0x33, 0x99],
        "dkUpDiag" => [0x33, 0x66, 0xcc, 0x99, 0x33, 0x66, 0xcc, 0x99],
        "wdDnDiag" => [0xc1, 0xe0, 0x70, 0x38, 0x1c, 0x0e, 0x07, 0x83],
        "wdUpDiag" => [0x83, 0x07, 0x0e, 0x1c, 0x38, 0x70, 0xe0, 0xc1],
        "dashDnDiag" => [0, 0, 0x88, 0x44, 0x22, 0x11, 0, 0],
        "dashUpDiag" => [0, 0, 0x11, 0x22, 0x44, 0x88, 0, 0],
        "diagCross" => [0x81, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x81],
        "smCheck" => [0x99, 0x66, 0x66, 0x99, 0x99, 0x66, 0x66, 0x99],
        "lgCheck" => [0xf0, 0xf0, 0xf0, 0xf0, 0x0f, 0x0f, 0x0f, 0x0f],
        "smConfetti" => [0x80, 0x08, 0x40, 0x02, 0x10, 0x01, 0x20, 0x04],
        "lgConfetti" => [0xb1, 0x30, 0x03, 0x1b, 0xd8, 0xc0, 0x0c, 0x8d],
        "horzBrick" => [0xff, 0x80, 0x80, 0x80, 0xff, 0x08, 0x08, 0x08],
        "diagBrick" => [0x01, 0x02, 0x04, 0x08, 0x18, 0x24, 0x42, 0x81],
        "solidDmnd" => [0x10, 0x38, 0x7c, 0xfe, 0x7c, 0x38, 0x10, 0],
        "openDmnd" => [0x82, 0x44, 0x28, 0x10, 0x28, 0x44, 0x82, 0x01],
        "dotDmnd" => [0x80, 0, 0x22, 0, 0x08, 0, 0x22, 0],
        "plaid" => [0xaa, 0x55, 0xaa, 0x55, 0xf0, 0xf0, 0xf0, 0xf0],
        "sphere" => [0x77, 0x89, 0x8f, 0x8f, 0x77, 0x98, 0xf8, 0xf8],
        "weave" => [0x88, 0x54, 0x22, 0x45, 0x88, 0x14, 0x22, 0x51],
        "divot" => [0, 0x10, 0x08, 0x10, 0, 0x80, 0x01, 0x80],
        "shingle" => [0x01, 0x82, 0x44, 0x38, 0x06, 0x01, 0x00, 0x00],
        "wave" => [0, 0x18, 0xa4, 0x03, 0, 0x18, 0xa4, 0x03],
        "trellis" => [0xff, 0x66, 0xff, 0x99, 0xff, 0x66, 0xff, 0x99],
        "zigZag" => [0x81, 0x42, 0x24, 0x18, 0x81, 0x42, 0x24, 0x18],
        _ => return None,
    };
    Some(rows)
}

/// The 8 × 8 tile of a spreadsheet cell's pattern (`x:patternFill/@patternType`, such as `darkVertical`), as Excel prints
/// it: a row per byte, top to bottom, the high bit the leftmost cell; a set bit is the foreground colour. Excel's tiles are
/// not PowerPoint's (`darkGray` is not 75 % there, `lightTrellis` is not `diagCross`).
///
/// Read off the patterns Excel draws (its PDF of the coverage suite XFIL, 1 pt per cell, the tile anchored at the paper's
/// corner), one cell at a time. `None` for a name that is not one of the 17 patterns.
pub fn spreadsheet_pattern_tile(pattern_type: &str) -> Option<[u8; 8]> {
    Some(match pattern_type {
        "gray0625" => [0x80, 0, 0x08, 0, 0x80, 0, 0x08, 0],
        "gray125" => [0x88, 0, 0x22, 0, 0x88, 0, 0x22, 0],
        "lightGray" => [0x88, 0x22, 0x88, 0x22, 0x88, 0x22, 0x88, 0x22],
        "mediumGray" => [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
        "darkGray" => [0xee, 0xbb, 0xee, 0xbb, 0xee, 0xbb, 0xee, 0xbb],
        "lightHorizontal" => [0xff, 0, 0, 0, 0xff, 0, 0, 0],
        "darkHorizontal" => [0xff, 0xff, 0, 0, 0xff, 0xff, 0, 0],
        "lightVertical" => [0x88; 8],
        "darkVertical" => [0xcc; 8],
        "lightDown" => [0x88, 0x44, 0x22, 0x11, 0x88, 0x44, 0x22, 0x11],
        "darkDown" => [0xcc, 0x66, 0x33, 0x99, 0xcc, 0x66, 0x33, 0x99],
        "lightUp" => [0x11, 0x22, 0x44, 0x88, 0x11, 0x22, 0x44, 0x88],
        "darkUp" => [0x33, 0x66, 0xcc, 0x99, 0x33, 0x66, 0xcc, 0x99],
        "lightGrid" => [0xff, 0x88, 0x88, 0x88, 0xff, 0x88, 0x88, 0x88],
        "darkGrid" => [0x99, 0x66, 0x66, 0x99, 0x99, 0x66, 0x66, 0x99],
        "lightTrellis" => [0x88, 0x55, 0x22, 0x55, 0x88, 0x55, 0x22, 0x55],
        "darkTrellis" => [0xff, 0x66, 0xff, 0x99, 0xff, 0x66, 0xff, 0x99],
        _ => return None,
    })
}

/// The spreadsheet pattern (`x:patternFill/@patternType`) a cell's pattern preset stands for (the reader maps the 17 of a
/// workbook to presets, see `spreadsheet_pattern_tile`).
pub fn spreadsheet_pattern_of_preset(preset: &str) -> Option<&'static str> {
    Some(match preset {
        "pct5" => "gray0625",
        "pct10" => "gray125",
        "pct25" => "lightGray",
        "pct50" => "mediumGray",
        "pct75" => "darkGray",
        "ltHorz" => "lightHorizontal",
        "dkHorz" => "darkHorizontal",
        "ltVert" => "lightVertical",
        "dkVert" => "darkVertical",
        "ltDnDiag" => "lightDown",
        "dkDnDiag" => "darkDown",
        "ltUpDiag" => "lightUp",
        "dkUpDiag" => "darkUp",
        "smGrid" => "lightGrid",
        "cross" => "darkGrid",
        "diagCross" => "lightTrellis",
        "trellis" => "darkTrellis",
        _ => return None,
    })
}

impl Fill {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }
}

/// Decoration at the end of a line (`a:headEnd` / `a:tailEnd`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineEnd {
    #[default]
    None,
    /// A filled triangle.
    Triangle(EndSize),
    /// A filled arrowhead with a notch in its back.
    Stealth(EndSize),
    /// An open arrowhead, two strokes.
    Arrow(EndSize),
    Diamond(EndSize),
    Oval(EndSize),
}

/// The width and length of a line end, each 0 (small), 1 (medium) or 2
/// (large).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EndSize {
    pub width: u8,
    pub length: u8,
}

impl LineEnd {
    /// The end of `a:headEnd` / `a:tailEnd` attributes.
    pub fn parse(kind: Option<&str>, width: Option<&str>, length: Option<&str>) -> Self {
        let size = |v: Option<&str>| match v {
            Some("sm") => 0,
            Some("lg") => 2,
            _ => 1,
        };
        let s = EndSize { width: size(width), length: size(length) };
        match kind {
            Some("triangle") => Self::Triangle(s),
            Some("stealth") => Self::Stealth(s),
            Some("arrow") => Self::Arrow(s),
            Some("diamond") => Self::Diamond(s),
            Some("oval") => Self::Oval(s),
            _ => Self::None,
        }
    }
}

/// A line's dash pattern (`a:prstDash`, `ST_PresetLineDashVal`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineDash {
    #[default]
    Solid,
    Dot,
    Dash,
    LgDash,
    DashDot,
    LgDashDot,
    LgDashDotDot,
    SysDash,
    SysDot,
    SysDashDot,
    SysDashDotDot,
}

impl LineDash {
    pub const ALL: [LineDash; 11] = [
        LineDash::Solid,
        LineDash::Dot,
        LineDash::Dash,
        LineDash::LgDash,
        LineDash::DashDot,
        LineDash::LgDashDot,
        LineDash::LgDashDotDot,
        LineDash::SysDash,
        LineDash::SysDot,
        LineDash::SysDashDot,
        LineDash::SysDashDotDot,
    ];

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.name() == name)
    }

    /// The name as written in `a:prstDash/@val`.
    pub fn name(self) -> &'static str {
        match self {
            LineDash::Solid => "solid",
            LineDash::Dot => "dot",
            LineDash::Dash => "dash",
            LineDash::LgDash => "lgDash",
            LineDash::DashDot => "dashDot",
            LineDash::LgDashDot => "lgDashDot",
            LineDash::LgDashDotDot => "lgDashDotDot",
            LineDash::SysDash => "sysDash",
            LineDash::SysDot => "sysDot",
            LineDash::SysDashDot => "sysDashDot",
            LineDash::SysDashDotDot => "sysDashDotDot",
        }
    }

    /// Dashes and gaps in turn, in line widths (ECMA-376 §20.1.10.48).
    /// Empty for a solid line.
    pub fn pattern(self) -> &'static [f32] {
        match self {
            LineDash::Solid => &[],
            LineDash::Dot => &[1.0, 3.0],
            LineDash::Dash => &[4.0, 3.0],
            LineDash::LgDash => &[8.0, 3.0],
            LineDash::DashDot => &[4.0, 3.0, 1.0, 3.0],
            LineDash::LgDashDot => &[8.0, 3.0, 1.0, 3.0],
            LineDash::LgDashDotDot => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
            LineDash::SysDash => &[3.0, 1.0],
            LineDash::SysDot => &[1.0, 1.0],
            LineDash::SysDashDot => &[3.0, 1.0, 1.0, 1.0],
            LineDash::SysDashDotDot => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        }
    }
}

/// How the ends of a line and of its dashes are drawn (`a:ln/@cap`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineCap {
    /// Cut off at the end (`flat`).
    #[default]
    Flat,
    /// Half a line width more, rounded (`rnd`).
    Round,
    /// Half a line width more, square (`sq`).
    Square,
}

/// How lines join at vertices (`a:round`, `a:bevel`, `a:miter`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineJoin {
    /// Extended sharp corner (`miter`).
    #[default]
    Miter,
    /// Rounded corner (`round`).
    Round,
    /// Flat beveled corner (`bevel`).
    Bevel,
}

/// A shape outline or a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stroke {
    pub color: Color,
    pub width: Emu,
    /// Not solid (`dash` is not [`LineDash::Solid`]).
    pub dashed: bool,
    /// Decoration at the start of the line.
    pub head: LineEnd,
    /// Decoration at the end of the line.
    pub tail: LineEnd,
    pub dash: LineDash,
    pub cap: LineCap,
    pub join: LineJoin,
    /// One line or several side by side (`a:ln/@cmpd`).
    pub compound: LineCompound,
    /// Where the stroke is aligned relative to its path (`a:ln/@algn`).
    pub alignment: LineAlignment,
    /// An optional fill for the stroke (e.g. gradient stroke `a:gradFill`).
    pub fill: Option<Fill>,
}

/// Where a stroke is aligned relative to its path (`ST_PenAlignment`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineAlignment {
    /// Centred on the path (`ctr`).
    #[default]
    Center,
    /// Drawn on the inside of the path (`in`).
    Inset,
}

impl LineAlignment {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "ctr" => Some(Self::Center),
            "in" => Some(Self::Inset),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Center => "ctr",
            Self::Inset => "in",
        }
    }
}

/// A line drawn as several thinner ones side by side (`ST_CompoundLine`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCompound {
    #[default]
    Single,
    Double,
    ThickThin,
    ThinThick,
    Triple,
}

impl LineCompound {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "sng" => Self::Single,
            "dbl" => Self::Double,
            "thickThin" => Self::ThickThin,
            "thinThick" => Self::ThinThick,
            "tri" => Self::Triple,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "sng",
            Self::Double => "dbl",
            Self::ThickThin => "thickThin",
            Self::ThinThick => "thinThick",
            Self::Triple => "tri",
        }
    }

    /// The lines across the width, from the left of the line's direction
    /// to its right, as fractions of the width (start, end); measured on
    /// PowerPoint's PDFs (the coverage suite PLIN): double is thirds,
    /// thick-thin 3 : 1 : 1, triple 1 : 1 : 2 : 1 : 1 roughly.
    pub fn bands(self) -> &'static [(f32, f32)] {
        match self {
            Self::Single => &[(0.0, 1.0)],
            Self::Double => &[(0.0, 1.0 / 3.0), (2.0 / 3.0, 1.0)],
            Self::ThickThin => &[(0.0, 0.6), (0.8, 1.0)],
            Self::ThinThick => &[(0.0, 0.2), (0.4, 1.0)],
            Self::Triple => &[(0.0, 1.0 / 6.0), (2.0 / 6.0, 4.0 / 6.0), (5.0 / 6.0, 1.0)],
        }
    }
}

impl Stroke {
    /// PowerPoint's default outline: 0.75 pt.
    pub const DEFAULT_WIDTH: Emu = 9_525;

    pub fn solid(color: Color, width: Emu) -> Self {
        Self { color, width, dashed: false, head: LineEnd::None, tail: LineEnd::None, dash: LineDash::Solid, cap: LineCap::Flat, join: LineJoin::Miter, compound: LineCompound::Single, alignment: LineAlignment::Center, fill: None }
    }

    /// This stroke with dash pattern `dash` (and `dashed` to match).
    pub fn with_dash(self, dash: LineDash) -> Self {
        Self { dashed: dash != LineDash::Solid, dash, ..self }
    }

    /// This stroke with an explicit fill (e.g. gradient fill).
    pub fn with_fill(self, fill: Fill) -> Self {
        Self { fill: Some(fill), ..self }
    }
}

/// The outline of a shape.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Geometry {
    #[default]
    Rect,
    /// A rectangle with rounded corners. `radius` is the corner radius as a
    /// fraction of the shorter side, in 100 000ths (OOXML's `adj`, default 16 667).
    RoundRect { radius: u32 },
    Ellipse,
    /// Any other preset, or a custom geometry: guide formulas and paths.
    Formula(FormulaGeometry),
}

impl Geometry {
    /// Where text goes in a shape `w` × `h`: left, top, right and bottom
    /// (the presets' `a:rect`).
    pub fn text_rect(&self, w: f64, h: f64) -> (f64, f64, f64, f64) {
        match self {
            Geometry::Rect => (0.0, 0.0, w, h),
            Geometry::RoundRect { radius } => {
                // roundRect: il = ss × adj × (1 − cos 45°).
                let il = w.min(h) * *radius as f64 / 100_000.0 * 0.29289;
                (il, il, w - il, h - il)
            }
            Geometry::Ellipse => {
                let (dx, dy) = (w / 2.0 * (1.0 - std::f64::consts::FRAC_1_SQRT_2), h / 2.0 * (1.0 - std::f64::consts::FRAC_1_SQRT_2));
                (dx, dy, w - dx, h - dy)
            }
            Geometry::Formula(f) => f.text_rect(w, h),
        }
    }

    /// The preset geometry name (e.g. "rect", "roundRect", "ellipse", "heart"),
    /// or None if this is a custom geometry without a preset.
    pub fn preset_name(&self) -> Option<&str> {
        match self {
            Geometry::Rect => Some("rect"),
            Geometry::RoundRect { .. } => Some("roundRect"),
            Geometry::Ellipse => Some("ellipse"),
            Geometry::Formula(f) => f.preset.as_deref(),
        }
    }
}

/// Presets for 3D bevels (OOXML's `ST_BevelPresetType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BevelPreset {
    #[default]
    Circle,
    RelaxedInset,
    Slope,
    Cross,
    Angle,
    SoftRound,
    Convex,
    CoolSlant,
    Divot,
    Riblet,
    HardEdge,
    ArtDeco,
}

impl BevelPreset {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "circle" => Some(Self::Circle),
            "relaxedInset" | "relaxedIns" => Some(Self::RelaxedInset),
            "slope" => Some(Self::Slope),
            "cross" => Some(Self::Cross),
            "angle" => Some(Self::Angle),
            "softRound" => Some(Self::SoftRound),
            "convex" => Some(Self::Convex),
            "coolSlant" => Some(Self::CoolSlant),
            "divot" => Some(Self::Divot),
            "riblet" => Some(Self::Riblet),
            "hardEdge" => Some(Self::HardEdge),
            "artDeco" => Some(Self::ArtDeco),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Circle => "circle",
            Self::RelaxedInset => "relaxedInset",
            Self::Slope => "slope",
            Self::Cross => "cross",
            Self::Angle => "angle",
            Self::SoftRound => "softRound",
            Self::Convex => "convex",
            Self::CoolSlant => "coolSlant",
            Self::Divot => "divot",
            Self::Riblet => "riblet",
            Self::HardEdge => "hardEdge",
            Self::ArtDeco => "artDeco",
        }
    }
}

/// A 3D bevel effect on a shape (`a:sp3d/a:bevelT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Bevel {
    pub preset: BevelPreset,
    /// Width in EMU.
    pub width: i64,
    /// Height in EMU.
    pub height: i64,
}

impl Bevel {
    /// PowerPoint's default bevel size: 6 pt (76 200 EMU).
    pub const DEFAULT_SIZE: i64 = 76_200;

    pub fn new(preset: BevelPreset, width: i64, height: i64) -> Self {
        Self { preset, width, height }
    }

    pub fn standard(preset: BevelPreset) -> Self {
        Self { preset, width: Self::DEFAULT_SIZE, height: Self::DEFAULT_SIZE }
    }
}

