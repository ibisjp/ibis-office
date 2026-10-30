// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Excel (`.xlsx`) worksheets: cells, their formats, column widths and row
//! heights, read into a [`Sheet`].
//!
//! Values are shown as Excel last calculated them (the cached `c/v`), with
//! the cell's number format applied. Drawings (charts, pictures, shapes)
//! are read by the presentation reader, which shares DrawingML with slides.

use std::collections::{HashMap, HashSet};

use ibis_doc_model::{
    Border, BorderLine, CellUnderline, CellAlign, CellError, CellGradient, CellKind, CellRange, CellValue, CellStyle, CellVerticalAlign, EMU_PER_INCH, EMU_PER_POINT, Emu, GradientPath, Rgb,
    Sheet, SheetCell, SheetPrint, VerticalPosition,
};
use roxmltree::Node;

use crate::theme::Theme;
use crate::xml::{attr, attr_bool, attr_i64, child, children, elements};

/// Excel's legacy colour palette (`indexed` colours 0–63).
const PALETTE: [u32; 64] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x000000, 0xFFFFFF, 0xFF0000,
    0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000, 0x008000, 0x000080, 0x808000, 0x800080, 0x008080,
    0xC0C0C0, 0x808080, 0x9999FF, 0x993366, 0xFFFFCC, 0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080,
    0xFF00FF, 0xFFFF00, 0x00FFFF, 0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99,
    0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600, 0x666699,
    0x969696, 0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399, 0x333333,
];

/// Theme colour names in the order of SpreadsheetML's `theme` index.
const THEME_ORDER: [&str; 12] =
    ["lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"];

/// Built-in number formats (the Japanese locale's).
pub(crate) fn builtin_format(id: i64) -> Option<&'static str> {
    u32::try_from(id).ok().and_then(ibis_calc::numfmt::builtin)
}

/// The built-in format id of a format code, if it is one.
pub(crate) fn builtin_format_id(code: &str) -> Option<u32> {
    (0..=81).find(|&id| builtin_format(id) == Some(code)).map(|id| id as u32)
}

/// Resolves SpreadsheetML colours (`rgb`, `theme` with `tint`, `indexed`).
pub(crate) struct SheetColors<'t> {
    theme: &'t Theme,
    /// A custom palette (`styles.xml` `colors/indexedColors`).
    palette: Vec<Rgb>,
}

impl<'t> SheetColors<'t> {
    pub(crate) fn new(theme: &'t Theme, styles: Option<Node>) -> Self {
        let palette = styles
            .and_then(|s| child(s, "colors"))
            .and_then(|c| child(c, "indexedColors"))
            .map(|list| children(list, "rgbColor").filter_map(|c| attr(c, "rgb").and_then(argb)).collect())
            .unwrap_or_default();
        Self { theme, palette }
    }

    /// The font a scheme font (`<scheme val="minor|major"/>`) is shown in:
    /// Excel in Japanese shows the theme's Japanese font for the whole
    /// text (Calibri with the scheme comes out in 游ゴシック, as the PDFs of
    /// Excel for Mac show).
    pub(crate) fn scheme_font(&self, scheme: &str) -> Option<&str> {
        match scheme {
            "minor" => self.theme.japanese_font(false),
            "major" => self.theme.japanese_font(true),
            _ => None,
        }
    }

    /// The theme colour a `color` / `fgColor` element names (`theme` and `tint`).
    pub(crate) fn theme_color(&self, node: Node) -> Option<ibis_doc_model::ThemeColor> {
        let index = attr_i64(node, "theme")?;
        if !(0..THEME_ORDER.len() as i64).contains(&index) || attr(node, "rgb").is_some() {
            return None;
        }
        let tint = attr(node, "tint").and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0);
        Some(ibis_doc_model::ThemeColor { index: index as u8, tint: (tint * 1_000_000.0).round() as i32 })
    }

    /// The colour of a `color` / `fgColor` element; `None` for automatic.
    pub(crate) fn color(&self, node: Node) -> Option<Rgb> {
        let base = if let Some(rgb) = attr(node, "rgb") {
            argb(rgb)?
        } else if let Some(index) = attr_i64(node, "theme") {
            let name = THEME_ORDER.get(index.max(0) as usize)?;
            self.theme.color(name)?
        } else if let Some(index) = attr_i64(node, "indexed") {
            let index = index.max(0) as usize;
            self.palette.get(index).copied().or_else(|| match index {
                64 => Some(Rgb::BLACK),
                65 => Some(Rgb::WHITE),
                _ => PALETTE.get(index).map(|v| Rgb(*v)),
            })?
        } else {
            return None;
        };
        let tint = attr(node, "tint").and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0);
        Some(if tint == 0.0 { base } else { apply_tint(base, tint) })
    }
}

fn argb(value: &str) -> Option<Rgb> {
    let hex = if value.len() == 8 { &value[2..] } else { value };
    u32::from_str_radix(hex, 16).ok().map(Rgb)
}

/// Excel's tint: towards white (positive) or black (negative) in HLS
/// lightness.
pub(crate) fn apply_tint(rgb: Rgb, tint: f64) -> Rgb {
    let (h, l, s) = to_hls(rgb);
    let l = if tint < 0.0 { l * (1.0 + tint) } else { l * (1.0 - tint) + tint };
    from_hls(h, l.clamp(0.0, 1.0), s)
}

fn to_hls(rgb: Rgb) -> (f64, f64, f64) {
    let (r, g, b) = (rgb.r() as f64 / 255.0, rgb.g() as f64 / 255.0, rgb.b() as f64 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, l, 0.0);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;
    (h, l, s)
}

fn from_hls(h: f64, l: f64, s: f64) -> Rgb {
    let channel = |t: f64, p: f64, q: f64| {
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
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        (channel(h + 1.0 / 3.0, p, q), channel(h, p, q), channel(h - 1.0 / 3.0, p, q))
    };
    let byte = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u32;
    Rgb(byte(r) << 16 | byte(g) << 8 | byte(b))
}

/// A cell format (`cellXfs/xf`) resolved to a style and a number format.
#[derive(Clone, Debug, Default)]
pub(crate) struct CellFormat {
    pub(crate) style: CellStyle,
    pub(crate) number_format: String,
}

/// The workbook's cell formats.
pub(crate) struct Styles {
    pub(crate) formats: Vec<CellFormat>,
    /// The workbook counts dates from 1904 (set by the workbook's reader).
    pub(crate) date1904: bool,
    /// The default font: its typeface and size (hundredths of a point).
    pub(crate) default_font: (Option<String>, u32),
    /// The conditional formats' formats (`dxfs`).
    pub(crate) dxfs: Vec<ibis_doc_model::CfStyle>,
    /// The errors a cell's rich value (`c/@vm`) holds, by value metadata
    /// index: a file stores #CALC! and #SPILL! as #VALUE! with the true kind
    /// in its rich data (set by the workbook's reader).
    pub(crate) rich_errors: HashMap<u32, CellError>,
    /// A cell with a picture in it (`vm` of the cell → the index of its file among `richValueRel.xml`'s).
    pub(crate) rich_images: HashMap<u32, u32>,
    /// User-defined custom cell styles.
    pub(crate) custom_cell_styles: Vec<ibis_doc_model::CustomCellStyle>,
}

fn parse_gradient_fill(g: Node, colors: &SheetColors) -> Option<CellGradient> {
    let stops: Vec<(u16, Rgb)> = children(g, "stop")
        .filter_map(|s| {
            let pos = (attr(s, "position")?.parse::<f64>().ok()?.clamp(0.0, 1.0) * 1000.0).round() as u16;
            let col = child(s, "color").and_then(|c| colors.color(c))?;
            Some((pos, col))
        })
        .collect();
    if stops.len() < 2 {
        return None;
    }
    if attr(g, "type") == Some("path") {
        let parse_f = |name: &str| {
            (attr(g, name).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0).clamp(0.0, 1.0) * 100_000.0).round() as u32
        };
        let l = parse_f("left");
        let t = parse_f("top");
        let r = 100_000 - parse_f("right");
        let b = 100_000 - parse_f("bottom");
        let focus = [l, t, r, b];
        Some(CellGradient::Path { path: GradientPath::Rect, focus, stops })
    } else {
        let degree = attr(g, "degree").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0).rem_euclid(360.0) as u16;
        Some(CellGradient::Linear { degree, stops })
    }
}

fn map_pattern(pattern_type: &str) -> Option<&'static str> {
    match pattern_type {
        "gray0625" => Some("pct5"),
        "gray125" => Some("pct10"),
        "lightGray" => Some("pct25"),
        "mediumGray" => Some("pct50"),
        "darkGray" => Some("pct75"),
        "lightHorizontal" => Some("ltHorz"),
        "darkHorizontal" => Some("dkHorz"),
        "lightVertical" => Some("ltVert"),
        "darkVertical" => Some("dkVert"),
        "lightDown" => Some("ltDnDiag"),
        "darkDown" => Some("dkDnDiag"),
        "lightUp" => Some("ltUpDiag"),
        "darkUp" => Some("dkUpDiag"),
        "lightGrid" => Some("smGrid"),
        "darkGrid" => Some("cross"),
        "lightTrellis" => Some("diagCross"),
        "darkTrellis" => Some("trellis"),
        _ => None,
    }
}

fn parse_border_edge(b: Node, name: &str, colors: &SheetColors) -> Option<Border> {
    let e = child(b, name)?;
    let name = attr(e, "style")?;
    if name == "none" {
        return None;
    }
    let line = BorderLine::from_ooxml(name).unwrap_or(BorderLine::Thin);
    Some(Border {
        line,
        color: child(e, "color").and_then(|c| colors.color(c)).unwrap_or(Rgb::BLACK),
    })
}

fn parse_dxf(d: Node, colors: &SheetColors) -> ibis_doc_model::CfStyle {
    let font = child(d, "font");
    let flag = |name: &str| font.and_then(|f| child(f, name)).map(|c| attr_bool(c, "val").unwrap_or(true));
    let fill_node = child(d, "fill");
    let pattern_fill = fill_node.and_then(|f| child(f, "patternFill"));
    let gradient_fill = fill_node.and_then(|f| child(f, "gradientFill"));

    let fill = pattern_fill
        .as_ref()
        .and_then(|p| {
            match attr(*p, "patternType") {
                Some("none") => None,
                // A dxf's solid fill is its bgColor; with no bgColor Excel
                // fills nothing, whatever the fgColor (the coverage tests'
                // references from Excel, XCF-00177 to 00190). Excel writes
                // solid as fgColor indexed="64", the system foreground,
                // with the colour in bgColor: reading fgColor made such
                // rows black.
                Some("solid") => child(*p, "bgColor").and_then(|c| colors.color(c)),
                _ => child(*p, "bgColor").or_else(|| child(*p, "fgColor")).and_then(|c| colors.color(c)),
            }
        })
        .or_else(|| {
            gradient_fill.as_ref().and_then(|g| {
                children(*g, "stop").next().and_then(|s| child(s, "color")).and_then(|c| colors.color(c))
            })
        });

    let pattern = pattern_fill.as_ref().and_then(|p| {
        let ptype = attr(*p, "patternType")?;
        let preset = map_pattern(ptype)?;
        let fg = child(*p, "fgColor").and_then(|c| colors.color(c)).unwrap_or(Rgb::BLACK);
        let bg = child(*p, "bgColor").and_then(|c| colors.color(c)).unwrap_or(Rgb::WHITE);
        Some((preset.to_string(), fg, bg))
    });

    let gradient = gradient_fill.as_ref().and_then(|g| parse_gradient_fill(*g, colors));

    let underline = font.and_then(|f| child(f, "u")).map(|u| {
        CellUnderline::from_ooxml(attr(u, "val").unwrap_or("single"))
    });

    let vert_align = font.and_then(|f| child(f, "vertAlign")).and_then(|v| attr(v, "val")).map(|val| {
        match val {
            "superscript" => VerticalPosition::Superscript,
            "subscript" => VerticalPosition::Subscript,
            _ => VerticalPosition::Baseline,
        }
    });

    let border = child(d, "border");
    let side = |name: &str| border.and_then(|b| parse_border_edge(b, name, colors));
    let number_format = child(d, "numFmt").and_then(|n| attr(n, "formatCode")).map(str::to_string);
    ibis_doc_model::CfStyle {
        color: font.and_then(|f| child(f, "color")).and_then(|c| colors.color(c)),
        bold: flag("b"),
        italic: flag("i"),
        underline,
        strike: flag("strike"),
        fill,
        gradient,
        pattern,
        vert_align,
        borders: [side("left"), side("right"), side("top"), side("bottom")],
        number_format,
    }
}

pub(crate) fn read_styles(root: Option<Node>, colors: &SheetColors) -> Styles {
    let Some(root) = root else {
        return Styles { formats: vec![CellFormat { number_format: "General".into(), ..Default::default() }], date1904: false, default_font: (None, 1100), dxfs: Vec::new(), rich_errors: HashMap::new(), rich_images: HashMap::new(), custom_cell_styles: Vec::new() };
    };
    let number_formats: HashMap<i64, String> = child(root, "numFmts")
        .into_iter()
        .flat_map(|n| children(n, "numFmt"))
        .filter_map(|n| Some((attr_i64(n, "numFmtId")?, attr(n, "formatCode")?.to_string())))
        .collect();

    struct FontProps {
        name: Option<String>,
        size: u32,
        bold: bool,
        italic: bool,
        underline: CellUnderline,
        strike: bool,
        color: Rgb,
        color_theme: Option<ibis_doc_model::ThemeColor>,
        vert_align: VerticalPosition,
    }
    let flag = |n: Node, name: &str| child(n, name).is_some_and(|c| attr_bool(c, "val").unwrap_or(true));
    let fonts: Vec<FontProps> = child(root, "fonts")
        .into_iter()
        .flat_map(|n| children(n, "font"))
        .map(|f| FontProps {
            name: child(f, "scheme")
                .and_then(|n| attr(n, "val"))
                .and_then(|scheme| colors.scheme_font(scheme))
                .map(str::to_string)
                .or_else(|| child(f, "name").and_then(|n| attr(n, "val")).map(str::to_string)),
            size: child(f, "sz").and_then(|n| attr(n, "val")).and_then(|v| v.parse::<f64>().ok()).map_or(1100, |v| (v * 100.0).round() as u32),
            bold: flag(f, "b"),
            italic: flag(f, "i"),
            underline: child(f, "u").map_or(CellUnderline::None, |u| CellUnderline::from_ooxml(attr(u, "val").unwrap_or("single"))),
            strike: flag(f, "strike"),
            color: child(f, "color").and_then(|c| colors.color(c)).unwrap_or(Rgb::BLACK),
            color_theme: child(f, "color").and_then(|c| colors.theme_color(c)),
            vert_align: match child(f, "vertAlign").and_then(|v| attr(v, "val")) {
                Some("superscript") => VerticalPosition::Superscript,
                Some("subscript") => VerticalPosition::Subscript,
                _ => VerticalPosition::Baseline,
            },
        })
        .collect();
    let patterns: Vec<Option<(String, Rgb, Rgb)>> = child(root, "fills")
        .into_iter()
        .flat_map(|n| children(n, "fill"))
        .map(|f| {
            let pattern = child(f, "patternFill")?;
            let ptype = attr(pattern, "patternType")?;
            if ptype == "darkDown"
                && child(pattern, "fgColor").and_then(|c| attr(c, "auto")).is_some()
                && child(pattern, "bgColor").and_then(|c| attr(c, "indexed")).map_or(false, |idx| idx == "65")
            {
                return None;
            }
            let preset = map_pattern(ptype)?;
            let fg = child(pattern, "fgColor").and_then(|c| colors.color(c)).unwrap_or(Rgb::BLACK);
            let bg = child(pattern, "bgColor").and_then(|c| colors.color(c)).unwrap_or(Rgb::WHITE);
            Some((preset.to_string(), fg, bg))
        })
        .collect();
    let fills: Vec<Option<Rgb>> = child(root, "fills")
        .into_iter()
        .flat_map(|n| children(n, "fill"))
        .map(|f| {
            if let Some(pattern) = child(f, "patternFill") {
                match attr(pattern, "patternType") {
                    None | Some("none") => None,
                    Some("solid") => child(pattern, "fgColor").and_then(|c| colors.color(c)),
                    Some("darkDown")
                        if child(pattern, "fgColor").and_then(|c| attr(c, "auto")).is_some()
                            && child(pattern, "bgColor").and_then(|c| attr(c, "indexed")).map_or(false, |idx| idx == "65") =>
                    {
                        Some(Rgb::BLACK)
                    }
                    Some(_) => child(pattern, "bgColor").and_then(|c| colors.color(c)).or_else(|| child(pattern, "fgColor").and_then(|c| colors.color(c))),
                }
            } else if let Some(g) = child(f, "gradientFill") {
                children(g, "stop").next().and_then(|s| child(s, "color")).and_then(|c| colors.color(c))
            } else {
                None
            }
        })
        .collect();
    // The theme colour of each solid fill, by fill index.
    let fill_themes: Vec<Option<ibis_doc_model::ThemeColor>> = child(root, "fills")
        .into_iter()
        .flat_map(|n| children(n, "fill"))
        .map(|f| {
            let pattern = child(f, "patternFill")?;
            (attr(pattern, "patternType") == Some("solid")).then(|| child(pattern, "fgColor").and_then(|c| colors.theme_color(c))).flatten()
        })
        .collect();
    // Gradient fills (linear and path), by fill index.
    let gradients: Vec<Option<CellGradient>> = child(root, "fills")
        .into_iter()
        .flat_map(|n| children(n, "fill"))
        .map(|f| child(f, "gradientFill").and_then(|g| parse_gradient_fill(g, colors)))
        .collect();
    let edge = |b: Node, name: &str| parse_border_edge(b, name, colors);
    struct Edges {
        left: Option<Border>,
        right: Option<Border>,
        top: Option<Border>,
        bottom: Option<Border>,
        up: Option<Border>,
        down: Option<Border>,
    }
    let borders: Vec<Edges> = child(root, "borders")
        .into_iter()
        .flat_map(|n| children(n, "border"))
        .map(|b| {
            let diagonal = edge(b, "diagonal");
            // `start` and `end` (the newer names, left-to-right) stand for
            // `left` and `right` when those are empty, as Excel prints them.
            Edges {
                left: edge(b, "left").or_else(|| edge(b, "start")),
                right: edge(b, "right").or_else(|| edge(b, "end")),
                top: edge(b, "top"),
                bottom: edge(b, "bottom"),
                up: diagonal.filter(|_| attr_bool(b, "diagonalUp").unwrap_or(false)),
                down: diagonal.filter(|_| attr_bool(b, "diagonalDown").unwrap_or(false)),
            }
        })
        .collect();

    let parse_xf = |xf: Node| -> (CellStyle, bool, bool, bool, bool, bool, bool) {
        let index = |name: &str| attr_i64(xf, name).unwrap_or(0).max(0) as usize;
        let mut style = CellStyle::default();
        if let Some(font) = fonts.get(index("fontId")) {
            style.font = font.name.clone();
            style.font_size = font.size;
            style.bold = font.bold;
            style.italic = font.italic;
            style.underline = font.underline;
            style.strike = font.strike;
            style.color = font.color;
            style.color_theme = font.color_theme;
            style.vert_align = font.vert_align;
        }
        style.fill = fills.get(index("fillId")).copied().flatten();
        style.fill_theme = fill_themes.get(index("fillId")).copied().flatten();
        style.gradient = gradients.get(index("fillId")).cloned().flatten();
        style.pattern = patterns.get(index("fillId")).cloned().flatten();
        if let Some(e) = borders.get(index("borderId")) {
            (style.left, style.right, style.top, style.bottom) = (e.left, e.right, e.top, e.bottom);
            (style.diagonal_up, style.diagonal_down) = (e.up, e.down);
        }
        if let Some(a) = child(xf, "alignment") {
            style.align = match attr(a, "horizontal") {
                Some("left") => CellAlign::Left,
                Some("center") => CellAlign::Center,
                Some("right") => CellAlign::Right,
                Some("centerContinuous") => CellAlign::CenterAcross,
                Some("distributed") => CellAlign::Distributed,
                Some("fill") => CellAlign::Fill,
                Some("justify") => CellAlign::Justify,
                _ => CellAlign::General,
            };
            style.vertical_set = attr(a, "vertical").is_some();
            style.vertical = match attr(a, "vertical") {
                Some("top") => CellVerticalAlign::Top,
                Some("center") => CellVerticalAlign::Center,
                Some("justify") => CellVerticalAlign::Justify,
                Some("distributed") => CellVerticalAlign::Distributed,
                _ => CellVerticalAlign::Bottom,
            };
            style.wrap = attr_bool(a, "wrapText").unwrap_or(false);
            style.indent = attr_i64(a, "indent").unwrap_or(0).max(0) as u32;
            style.shrink = attr_bool(a, "shrinkToFit").unwrap_or(false);
            style.rotation = attr_i64(a, "textRotation").unwrap_or(0).clamp(0, 255) as u8;
        }
        if let Some(p) = child(xf, "protection") {
            style.unlocked = attr_bool(p, "locked") == Some(false);
            style.formula_hidden = attr_bool(p, "hidden").unwrap_or(false);
        }
        let id = attr_i64(xf, "numFmtId").unwrap_or(0);
        let number_format = number_formats.get(&id).cloned().or_else(|| builtin_format(id).map(str::to_string)).unwrap_or_else(|| "General".into());
        style.number_format = number_format;

        let has_flag = |name: &str, has_content: bool| {
            match attr_bool(xf, name) {
                Some(v) => v,
                None => has_content,
            }
        };
        let apply_num = has_flag("applyNumberFormat", id > 0);
        let apply_font = has_flag("applyFont", attr_i64(xf, "fontId").unwrap_or(0) > 0);
        let apply_fill = has_flag("applyFill", attr_i64(xf, "fillId").unwrap_or(0) > 0);
        let apply_border = has_flag("applyBorder", attr_i64(xf, "borderId").unwrap_or(0) > 0);
        let apply_align = has_flag("applyAlignment", child(xf, "alignment").is_some());
        let apply_protect = has_flag("applyProtection", child(xf, "protection").is_some());

        (style, apply_num, apply_align, apply_font, apply_border, apply_fill, apply_protect)
    };

    let formats = child(root, "cellXfs")
        .into_iter()
        .flat_map(|n| children(n, "xf"))
        .enumerate()
        .map(|(xf_index, xf)| {
            let (mut style, _, _, _, _, _, _) = parse_xf(xf);
            let number_format = style.number_format.clone();
            style.xf = Some(xf_index as u32);
            CellFormat { style, number_format }
        })
        .collect::<Vec<_>>();
    let default_font = fonts.first().map_or((None, 1100), |f| (f.name.clone(), f.size));
    let formats = if formats.is_empty() { vec![CellFormat { number_format: "General".into(), ..Default::default() }] } else { formats };

    let style_xfs: Vec<_> = child(root, "cellStyleXfs")
        .into_iter()
        .flat_map(|n| children(n, "xf"))
        .map(parse_xf)
        .collect();

    let custom_cell_styles: Vec<ibis_doc_model::CustomCellStyle> = child(root, "cellStyles")
        .into_iter()
        .flat_map(|n| children(n, "cellStyle"))
        .filter_map(|cs| {
            let name = attr(cs, "name")?;
            let xf_id = attr_i64(cs, "xfId")? as usize;
            let builtin_id = attr_i64(cs, "builtinId");
            if builtin_id.is_some() || name.eq_ignore_ascii_case("Normal") || name == "標準" {
                return None;
            }
            let &(ref format, num, align, font, border, fill, protect) = style_xfs.get(xf_id)?;
            Some(ibis_doc_model::CustomCellStyle {
                name: name.to_string(),
                format: format.clone(),
                include_number_format: num,
                include_alignment: align,
                include_font: font,
                include_border: border,
                include_fill: fill,
                include_protection: protect,
            })
        })
        .collect();
    let dxfs = child(root, "dxfs")
        .into_iter()
        .flat_map(|d| children(d, "dxf"))
        .map(|d| parse_dxf(d, colors))
        .collect();
    Styles { formats, date1904: false, default_font, dxfs, rich_errors: HashMap::new(), rich_images: HashMap::new(), custom_cell_styles }
}

fn read_data_consolidate(root: Node) -> Option<ibis_doc_model::DataConsolidate> {
    let dc = child(root, "dataConsolidate")?;
    let function = attr(dc, "function").and_then(ibis_doc_model::ConsolidateFunction::from_ooxml).unwrap_or_default();
    let left_labels = attr_bool(dc, "leftLabels").unwrap_or(false);
    let top_labels = attr_bool(dc, "topLabels").unwrap_or(false);
    let link = attr_bool(dc, "link").unwrap_or(false);
    let data_refs = child(dc, "dataRefs")
        .into_iter()
        .flat_map(|refs| children(refs, "dataRef"))
        .filter_map(|r| attr(r, "ref").map(str::to_string))
        .collect();
    Some(ibis_doc_model::DataConsolidate {
        function,
        left_labels,
        top_labels,
        link,
        data_refs,
    })
}

fn read_scenarios(root: Node) -> Option<ibis_doc_model::SheetScenarios> {
    let sc_node = child(root, "scenarios")?;
    let current = attr_i64(sc_node, "current").map(|v| v.max(0) as u32);
    let show = attr_i64(sc_node, "show").map(|v| v.max(0) as u32);
    let sqref = attr(sc_node, "sqref").map(str::to_string);
    let mut scenarios = Vec::new();
    for s in children(sc_node, "scenario") {
        let name = attr(s, "name").unwrap_or_default().to_string();
        let comment = attr(s, "comment").map(str::to_string);
        let user = attr(s, "user").map(str::to_string);
        let locked = attr_bool(s, "locked").unwrap_or(false);
        let hidden = attr_bool(s, "hidden").unwrap_or(false);
        let mut input_cells = Vec::new();
        for ic in children(s, "inputCells") {
            let r = attr(ic, "r").unwrap_or_default().to_string();
            let val = attr(ic, "val").unwrap_or_default().to_string();
            let num_fmt_id = attr_i64(ic, "numFmtId").map(|v| v.max(0) as u32);
            let deleted = attr_bool(ic, "deleted").unwrap_or(false);
            input_cells.push(ibis_doc_model::ScenarioInputCell {
                r,
                val,
                num_fmt_id,
                deleted,
            });
        }
        scenarios.push(ibis_doc_model::SheetScenario {
            name,
            comment,
            user,
            locked,
            hidden,
            input_cells,
        });
    }
    Some(ibis_doc_model::SheetScenarios {
        current,
        show,
        sqref,
        scenarios,
    })
}

pub(crate) fn read_custom_workbook_views(root: Node) -> Vec<ibis_doc_model::CustomWorkbookView> {
    child(root, "customWorkbookViews")
        .into_iter()
        .flat_map(|w| children(w, "customWorkbookView"))
        .map(|v| {
            let name = attr(v, "name").unwrap_or_default().to_string();
            let guid = attr(v, "guid").unwrap_or_default().to_string();
            let include_print_settings = attr_bool(v, "includePrintSettings").unwrap_or(true);
            let include_hidden = attr_bool(v, "includeHiddenRowCol").unwrap_or(true);
            let active_sheet_index = attr_i64(v, "activeSheetId").map_or(0, |id| (id.max(1) - 1) as usize);
            ibis_doc_model::CustomWorkbookView {
                name,
                guid,
                include_print_settings,
                include_hidden,
                active_sheet_index,
            }
        })
        .collect()
}

pub(crate) fn read_custom_sheet_views(root: Node) -> Vec<ibis_doc_model::CustomSheetView> {
    child(root, "customSheetViews")
        .into_iter()
        .flat_map(|w| children(w, "customSheetView"))
        .map(|v| {
            let guid = attr(v, "guid").unwrap_or_default().to_string();
            let name = attr(v, "name").unwrap_or_default().to_string();
            let zoom = attr_i64(v, "scale").map(|z| z.max(10) as u16);
            let grid_lines = attr_bool(v, "showGridLines").unwrap_or(true);
            let headings = attr_bool(v, "showRowCol").unwrap_or(true);
            let show_ruler = attr_bool(v, "showRuler").unwrap_or(true);
            let page_break_preview = attr(v, "view") == Some("pageBreakPreview");
            let pane_elem = child(v, "pane");
            let pane_state = pane_elem.and_then(|p| attr(p, "state")).unwrap_or("split");
            let (frozen, split) = match pane_elem {
                Some(p) if matches!(pane_state, "frozen" | "frozenSplit") => {
                    let parse_s = |name: &str| attr(p, name).and_then(|val| val.parse::<f64>().ok()).unwrap_or(0.0).max(0.0) as u32;
                    (Some((parse_s("xSplit"), parse_s("ySplit"))), None)
                }
                Some(p) => {
                    let top_left = attr(p, "topLeftCell").and_then(cell_ref);
                    let (col, row) = match top_left {
                        Some((c, r)) => (c, r),
                        None => {
                            let parse_s = |name: &str| attr(p, name).and_then(|val| val.parse::<f64>().ok()).unwrap_or(0.0).max(0.0) as u32;
                            (parse_s("xSplit"), parse_s("ySplit"))
                        }
                    };
                    (None, (col > 0 || row > 0).then_some((col, row)))
                }
                None => (None, None),
            };
            let selection = child(v, "selection");
            let active_cell = selection.and_then(|s| attr(s, "activeCell")).and_then(cell_ref);
            let selection_range = selection.and_then(|s| attr(s, "sqref")).and_then(parse_range);
            let print = if child(v, "pageSetUp").is_some() || child(v, "pageSetup").is_some() || child(v, "pageMargins").is_some() || child(v, "headerFooter").is_some() || child(v, "printOptions").is_some() {
                Some(read_print(v, attr_bool(v, "showGridLines").unwrap_or(true)))
            } else {
                None
            };
            let auto_filter = child(v, "autoFilter").and_then(|f| {
                let range = attr(f, "ref").and_then(parse_range)?;
                Some(ibis_doc_model::AutoFilter { range, columns: Vec::new() })
            });
            let show_formulas = attr_bool(v, "showFormulas").unwrap_or(false);
            let show_zeros = attr_bool(v, "showZeros").unwrap_or(true);
            ibis_doc_model::CustomSheetView {
                guid,
                name,
                zoom,
                grid_lines,
                headings,
                show_ruler,
                page_break_preview,
                frozen,
                split,
                active_cell,
                selection_range,
                print,
                hidden_rows: None,
                hidden_columns: None,
                auto_filter: auto_filter.map(Some),
                filtered_rows: None,
                show_formulas,
                show_zeros,
            }
        })
        .collect()
}

/// A sheet's filter (`autoFilter`): its range and the columns' filters
/// (values and dates, the average, comparisons, top 10, date periods,
/// colours; a colour from the styles' `dxfs`).
fn read_auto_filter(root: Node, dxfs: &[ibis_doc_model::CfStyle]) -> Option<ibis_doc_model::AutoFilter> {
    let f = child(root, "autoFilter")?;
    let range = attr(f, "ref").and_then(parse_range)?;
    let columns = children(f, "filterColumn")
        .filter_map(|fc| Some((range.first_column + attr_i64(fc, "colId")?.max(0) as u32, column_filter(fc, dxfs)?)))
        .collect();
    Some(ibis_doc_model::AutoFilter { range, columns })
}

/// One `filterColumn`'s filter, if it is one the model has.
fn column_filter(fc: Node, dxfs: &[ibis_doc_model::CfStyle]) -> Option<ibis_doc_model::ColumnFilter> {
    use ibis_doc_model::{ColumnFilter, DateGroup};
    let on = |n: Node, name: &str, default: bool| attr(n, name).map_or(default, |v| matches!(v, "1" | "true"));
    if let Some(v) = child(fc, "filters") {
        let mut values: Vec<String> = children(v, "filter").filter_map(|x| attr(x, "val").map(str::to_string)).collect();
        if on(v, "blank", false) {
            values.push(String::new());
        }
        let number = |n: Node, name: &str| attr(n, name).and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let groups: Vec<DateGroup> = children(v, "dateGroupItem")
            .map(|g| DateGroup {
                grouping: attr(g, "dateTimeGrouping").unwrap_or("year").to_string(),
                year: number(g, "year"),
                month: number(g, "month"),
                day: number(g, "day"),
                hour: number(g, "hour"),
                minute: number(g, "minute"),
                second: number(g, "second"),
            })
            .collect();
        return Some(if groups.is_empty() { ColumnFilter::Values(values) } else { ColumnFilter::Dates { values, groups } });
    }
    if let Some(d) = child(fc, "dynamicFilter") {
        return Some(match attr(d, "type")? {
            "aboveAverage" => ColumnFilter::AboveAverage(true),
            "belowAverage" => ColumnFilter::AboveAverage(false),
            "null" => return None,
            kind => ColumnFilter::Period(kind.to_string()),
        });
    }
    if let Some(t) = child(fc, "top10") {
        return Some(ColumnFilter::Top { top: on(t, "top", true), percent: on(t, "percent", false), value: attr(t, "val").unwrap_or("10").to_string() });
    }
    if let Some(c) = child(fc, "colorFilter") {
        let cell = on(c, "cellColor", true);
        let style = attr_i64(c, "dxfId").and_then(|i| dxfs.get(i.max(0) as usize));
        let color = style.and_then(|s| if cell { s.fill.or(s.pattern.as_ref().map(|p| p.1)) } else { s.color });
        return Some(ColumnFilter::Color { cell, color });
    }
    if let Some(i) = child(fc, "iconFilter") {
        let icon_set = attr(i, "iconSet").unwrap_or("3TrafficLights1").to_string();
        let icon_id = attr_i64(i, "iconId").unwrap_or(0).max(0) as u32;
        return Some(ColumnFilter::Icon { icon_set, icon_id });
    }
    let list = child(fc, "customFilters")?;
    let conditions: Vec<(String, String)> = children(list, "customFilter").map(|c| (attr(c, "operator").unwrap_or("equal").to_string(), attr(c, "val").unwrap_or("").to_string())).collect();
    (!conditions.is_empty()).then(|| ColumnFilter::Custom { and: on(list, "and", false), conditions })
}

/// A sheet's data validation rules (`dataValidations`).
fn read_validations(root: Node) -> Vec<ibis_doc_model::Validation> {
    child(root, "dataValidations")
        .into_iter()
        .flat_map(|d| children(d, "dataValidation"))
        .map(|v| {
            let formula = |name: &str| child(v, name).and_then(|f| f.text()).map(ibis_calc::formula::from_file);
            let text = |name: &str| attr(v, name).unwrap_or("").to_string();
            ibis_doc_model::Validation {
                ranges: attr(v, "sqref").unwrap_or("").split_whitespace().filter_map(parse_range).collect(),
                kind: attr(v, "type").unwrap_or("none").to_string(),
                operator: attr(v, "operator").unwrap_or("between").to_string(),
                formula1: formula("formula1"),
                formula2: formula("formula2"),
                allow_blank: attr_bool(v, "allowBlank").unwrap_or(false),
                dropdown: !attr_bool(v, "showDropDown").unwrap_or(false),
                prompt: attr_bool(v, "showInputMessage").unwrap_or(false).then(|| (text("promptTitle"), text("prompt"))).filter(|(t, p)| !t.is_empty() || !p.is_empty()),
                error: attr_bool(v, "showErrorMessage").unwrap_or(false).then(|| (attr(v, "errorStyle").unwrap_or("stop").to_string(), text("errorTitle"), text("error"))),
            }
        })
        .collect()
}

/// The 2010 conditional formats (`x14:cfRule`) that no rule of 2007 names (by `x14:id`): icon sets and data bars
/// only 2010 has (3Stars, 3Triangles, 5Boxes), as well as rules referencing defined names or other sheets which
/// Excel writes exclusively into `<x14:conditionalFormattings>`.
fn read_conditional_x14_only(root: Node, colors: &SheetColors) -> Vec<ibis_doc_model::ConditionalFormat> {
    use ibis_doc_model::{CfKind, CfRule, CfValue, ConditionalFormat, DataBarAxisPosition, DataBarDirection};
    let named: std::collections::HashSet<String> = root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "id" && n.parent().is_some_and(|p| p.tag_name().name() == "ext"))
        .filter_map(|n| n.text().map(|t| t.trim().to_string()))
        .collect();
    let cfvo = |n: Node| CfValue {
        kind: attr(n, "type").unwrap_or("num").to_string(),
        value: attr(n, "val")
            .map(str::to_string)
            .or_else(|| child(n, "f").and_then(|f| f.text()).map(ibis_calc::formula::from_file)),
        gte: attr_bool(n, "gte").unwrap_or(true),
    };
    let mut out = Vec::new();
    for group in root.descendants().filter(|n| n.is_element() && n.tag_name().name() == "conditionalFormatting" && n.attribute("sqref").is_none()) {
        let ranges: Vec<CellRange> = child(group, "sqref").and_then(|s| s.text()).unwrap_or("").split_whitespace().filter_map(parse_range).collect();
        let mut rules = Vec::new();
        for r in children(group, "cfRule") {
            if r.attribute("id").is_some_and(|id| named.contains(id.trim())) {
                continue;
            }
            let formulas: Vec<String> = children(r, "f")
                .chain(children(r, "formula"))
                .filter_map(|f| f.text())
                .map(ibis_calc::formula::from_file)
                .collect();
            let kind = match attr(r, "type").unwrap_or("") {
                "cellIs" => CfKind::CellIs {
                    operator: attr(r, "operator").unwrap_or("equal").to_string(),
                    formulas,
                },
                "timePeriod" => {
                    let period = attr(r, "timePeriod").unwrap_or("today").to_string();
                    let formula = formulas.into_iter().next().unwrap_or_default();
                    CfKind::TimePeriod { period, formula }
                }
                "expression"
                | "containsText"
                | "notContainsText"
                | "beginsWith"
                | "endsWith"
                | "containsBlanks"
                | "notContainsBlanks"
                | "containsErrors"
                | "notContainsErrors" => match formulas.into_iter().next() {
                    Some(f) => CfKind::Expression(f),
                    None => continue,
                },
                "top10" => CfKind::Top10 {
                    rank: attr_i64(r, "rank").unwrap_or(10).max(0) as u32,
                    percent: attr_bool(r, "percent").unwrap_or(false),
                    bottom: attr_bool(r, "bottom").unwrap_or(false),
                },
                "aboveAverage" => CfKind::AboveAverage {
                    above: attr_bool(r, "aboveAverage").unwrap_or(true),
                    equal: attr_bool(r, "equalAverage").unwrap_or(false),
                    std_dev: attr_i64(r, "stdDev").unwrap_or(0).max(0) as u32,
                },
                "uniqueValues" => CfKind::Unique,
                "duplicateValues" => CfKind::Duplicate,
                "colorScale" => {
                    let Some(cs) = child(r, "colorScale") else { continue };
                    let points: Vec<CfValue> = children(cs, "cfvo").map(cfvo).collect();
                    let cols: Vec<Rgb> = children(cs, "color").map(|c| colors.color(c).unwrap_or(Rgb::BLACK)).collect();
                    CfKind::ColorScale(points.into_iter().zip(cols).collect())
                }
                "dataBar" => {
                    let Some(db) = child(r, "dataBar") else { continue };
                    let points: Vec<CfValue> = children(db, "cfvo").map(cfvo).collect();
                    let color = child(db, "fillColor")
                        .or_else(|| child(db, "color"))
                        .and_then(|c| colors.color(c))
                        .unwrap_or(Rgb(0x638EC6));
                    let min_length = attr_i64(db, "minLength").map(|v| v.max(0) as u32);
                    let max_length = attr_i64(db, "maxLength").map(|v| v.max(0) as u32);
                    let gradient = attr_bool(db, "gradient").unwrap_or_else(|| attr(db, "gradient") != Some("0"));
                    let negative_color = child(db, "negativeFillColor")
                        .or_else(|| child(db, "negativeBarColor"))
                        .and_then(|c| colors.color(c));
                    let axis_color = child(db, "axisColor").and_then(|c| colors.color(c));
                    let axis_position = Some(match attr(db, "axisPosition") {
                        Some("midpoint") => DataBarAxisPosition::Midpoint,
                        Some("none") => DataBarAxisPosition::None,
                        _ => DataBarAxisPosition::Automatic,
                    });
                    let border_color = attr_bool(db, "border")
                        .unwrap_or(false)
                        .then(|| child(db, "borderColor").and_then(|c| colors.color(c)).unwrap_or(color));
                    let negative_border_color = child(db, "negativeBorderColor").and_then(|c| colors.color(c));
                    let direction = match attr(db, "direction") {
                        Some("leftToRight") => DataBarDirection::LeftToRight,
                        Some("rightToLeft") => DataBarDirection::RightToLeft,
                        _ => DataBarDirection::Context,
                    };
                    let at = |i: usize| {
                        points.get(i).cloned().unwrap_or(CfValue {
                            kind: if i == 0 { "min".into() } else { "max".into() },
                            value: None,
                            gte: true,
                        })
                    };
                    CfKind::DataBar {
                        min: at(0),
                        max: at(1),
                        color,
                        gradient,
                        negative_color,
                        axis_color,
                        axis_position,
                        min_length,
                        max_length,
                        border_color,
                        negative_border_color,
                        direction,
                    }
                }
                "iconSet" => {
                    let Some(is) = child(r, "iconSet") else { continue };
                    let icons: Vec<Option<ibis_doc_model::CfIcon>> = is
                        .descendants()
                        .filter(|n| n.tag_name().name() == "cfIcon")
                        .map(|ci| {
                            let s = attr(ci, "iconSet").unwrap_or("NoIcon").to_string();
                            let id = attr_i64(ci, "iconId").unwrap_or(-1) as i32;
                            Some(if id < 0 || s == "NoIcon" {
                                ibis_doc_model::CfIcon::no_icon()
                            } else {
                                ibis_doc_model::CfIcon::new(s, id)
                            })
                        })
                        .collect();
                    CfKind::IconSet {
                        set: attr(is, "iconSet").unwrap_or("3TrafficLights1").to_string(),
                        values: children(is, "cfvo").map(cfvo).collect(),
                        reverse: attr_bool(is, "reverse").unwrap_or(false),
                        show_value: attr_bool(is, "showValue").unwrap_or(true),
                        custom: attr_bool(is, "custom").unwrap_or(false),
                        icons,
                    }
                }
                _ => continue,
            };
            let style = child(r, "dxf").map(|d| parse_dxf(d, colors));
            rules.push(CfRule {
                kind,
                priority: attr_i64(r, "priority").unwrap_or(1000).max(0) as u32,
                stop: attr_bool(r, "stopIfTrue").unwrap_or(false),
                style,
            });
        }
        if !rules.is_empty() && !ranges.is_empty() {
            out.push(ConditionalFormat { source_index: None, ranges, rules });
        }
    }
    out
}

/// A sheet's conditional formats (`conditionalFormatting`), with their
/// rules' formats from `dxfs`.
fn read_conditional(root: Node, styles: &Styles, colors: &SheetColors) -> Vec<ibis_doc_model::ConditionalFormat> {
    use ibis_doc_model::{CfKind, CfRule, CfValue, ConditionalFormat, DataBarAxisPosition, DataBarDirection};
    let cfvo = |n: Node| CfValue { kind: attr(n, "type").unwrap_or("num").to_string(), value: attr(n, "val").map(str::to_string).or_else(|| child(n, "f").and_then(|f| f.text()).map(ibis_calc::formula::from_file)), gte: attr_bool(n, "gte").unwrap_or(true) };
    // The Excel 2010 rules (`x14:cfRule` in the sheet's `extLst`), by their ids: a rule of 2007 names its
    // 2010 half by `x14:id` in its own `extLst`, and that half holds the data bar's gradient, border, direction and
    // axis, and the icon sets of 2010.
    let x14_rules: std::collections::HashMap<String, Node> = root
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "cfRule" && n.attribute("id").is_some() && n.parent().is_some_and(|p| p.tag_name().name() == "conditionalFormatting"))
        .filter_map(|n| Some((n.attribute("id")?.to_string(), n)))
        .collect();
    let x14_of = |r: Node| -> Option<Node> { r.descendants().find(|n| n.is_element() && n.tag_name().name() == "id" && n.parent().is_some_and(|p| p.tag_name().name() == "ext")).and_then(|n| n.text()).and_then(|id| x14_rules.get(id.trim()).copied()) };
    children(root, "conditionalFormatting")
        .enumerate().map(|(source_index, cf)| {
            let ranges = attr(cf, "sqref").unwrap_or("").split_whitespace().filter_map(parse_range).collect();
            let rules = children(cf, "cfRule")
                .map(|r| {
                    let mut formulas: Vec<String> = children(r, "formula")
                        .chain(children(r, "f"))
                        .filter_map(|f| f.text())
                        .map(ibis_calc::formula::from_file)
                        .collect();
                    if formulas.is_empty()
                        && let Some(x14) = x14_of(r)
                    {
                        formulas = children(x14, "f")
                            .chain(children(x14, "formula"))
                            .filter_map(|f| f.text())
                            .map(ibis_calc::formula::from_file)
                            .collect();
                    }
                    let kind = match attr(r, "type").unwrap_or("") {
                        "cellIs" => CfKind::CellIs { operator: attr(r, "operator").unwrap_or("equal").to_string(), formulas },
                        "timePeriod" => {
                            let period = attr(r, "timePeriod").unwrap_or("today").to_string();
                            let formula = formulas.into_iter().next().unwrap_or_default();
                            CfKind::TimePeriod { period, formula }
                        }
                        "expression" | "containsText" | "notContainsText" | "beginsWith" | "endsWith" | "containsBlanks" | "notContainsBlanks" | "containsErrors" | "notContainsErrors" => {
                            match formulas.into_iter().next() {
                                Some(f) => CfKind::Expression(f),
                                None => CfKind::Other,
                            }
                        }
                        "top10" => CfKind::Top10 {
                            rank: attr_i64(r, "rank").unwrap_or(10).max(0) as u32,
                            percent: attr_bool(r, "percent").unwrap_or(false),
                            bottom: attr_bool(r, "bottom").unwrap_or(false),
                        },
                        "aboveAverage" => CfKind::AboveAverage {
                            above: attr_bool(r, "aboveAverage").unwrap_or(true),
                            equal: attr_bool(r, "equalAverage").unwrap_or(false),
                            std_dev: attr_i64(r, "stdDev").unwrap_or(0).max(0) as u32,
                        },
                        "uniqueValues" => CfKind::Unique,
                        "duplicateValues" => CfKind::Duplicate,
                        "colorScale" => match child(r, "colorScale") {
                            Some(cs) => {
                                let points: Vec<CfValue> = children(cs, "cfvo").map(cfvo).collect();
                                let cols: Vec<Rgb> = children(cs, "color").map(|c| colors.color(c)).map(|c| c.unwrap_or(Rgb::BLACK)).collect();
                                CfKind::ColorScale(points.into_iter().zip(cols).collect())
                            }
                            None => CfKind::Other,
                        },
                        "dataBar" => match child(r, "dataBar") {
                            Some(db) => {
                                let points: Vec<CfValue> = children(db, "cfvo").map(cfvo).collect();
                                let color = child(db, "color").and_then(|c| colors.color(c)).unwrap_or(Rgb(0x638EC6));
                                let x14_db = x14_of(r).and_then(|x| child(x, "dataBar")).or_else(|| r.descendants().find(|n| n.tag_name().name() == "dataBar" && n != &db));
                                // The 2010 half's lengths (0 to 100 by default) over the 2007 half's (10 to 90).
                                let min_length = x14_db.and_then(|x| attr_i64(x, "minLength")).or_else(|| attr_i64(db, "minLength")).map(|v| v.max(0) as u32);
                                let max_length = x14_db.and_then(|x| attr_i64(x, "maxLength")).or_else(|| attr_i64(db, "maxLength")).map(|v| v.max(0) as u32);

                                let gradient = if let Some(x14) = x14_db {
                                    attr_bool(x14, "gradient").unwrap_or_else(|| attr(x14, "gradient") != Some("0"))
                                } else {
                                    !r.descendants().any(|n| n.tag_name().name() == "dataBar" && attr(n, "gradient") == Some("0"))
                                };
                                let negative_color = x14_db.and_then(|x14| child(x14, "negativeFillColor").or_else(|| child(x14, "negativeBarColor"))).and_then(|c| colors.color(c))
                                    .or_else(|| child(db, "negativeFillColor").or_else(|| child(db, "negativeBarColor")).and_then(|c| colors.color(c)));
                                let axis_color = x14_db.and_then(|x14| child(x14, "axisColor")).and_then(|c| colors.color(c))
                                    .or_else(|| child(db, "axisColor").and_then(|c| colors.color(c)));
                                // A rule of 2007 alone has no axis and no negative bars; with its 2010 half the axis is automatic
                                // unless named.
                                let axis_position = Some(match x14_db.map(|x14| attr(x14, "axisPosition")).unwrap_or(attr(db, "axisPosition")) {
                                    Some("midpoint") => DataBarAxisPosition::Midpoint,
                                    Some("none") => DataBarAxisPosition::None,
                                    Some(_) => DataBarAxisPosition::Automatic,
                                    None if x14_db.is_some() => DataBarAxisPosition::Automatic,
                                    None => DataBarAxisPosition::None,
                                });
                                let border_color = x14_db.filter(|x| attr_bool(*x, "border").unwrap_or(false)).map(|x| child(x, "borderColor").and_then(|c| colors.color(c)).unwrap_or(color));
                                let negative_border_color = x14_db.and_then(|x| child(x, "negativeBorderColor")).and_then(|c| colors.color(c));
                                let direction = match x14_db.and_then(|x| attr(x, "direction")) {
                                    Some("leftToRight") => DataBarDirection::LeftToRight,
                                    Some("rightToLeft") => DataBarDirection::RightToLeft,
                                    _ => DataBarDirection::Context,
                                };

                                let at = |i: usize| points.get(i).cloned().unwrap_or(CfValue { kind: if i == 0 { "min".into() } else { "max".into() }, value: None, gte: true });
                                CfKind::DataBar {
                                    min: at(0),
                                    max: at(1),
                                    color,
                                    gradient,
                                    negative_color,
                                    axis_color,
                                    axis_position,
                                    min_length,
                                    max_length,
                                    border_color,
                                    negative_border_color,
                                    direction,
                                }
                            }
                            None => CfKind::Other,
                        },
                        "iconSet" => match child(r, "iconSet") {
                            Some(is) => {
                                let x14_is = x14_of(r).and_then(|x| child(x, "iconSet")).or_else(|| r.descendants().find(|n| n.tag_name().name() == "iconSet" && n != &is));
                                let set = x14_is.and_then(|x14| attr(x14, "iconSet")).or_else(|| attr(is, "iconSet")).unwrap_or("3TrafficLights1").to_string();
                                let reverse = x14_is.and_then(|x14| attr_bool(x14, "reverse")).or_else(|| attr_bool(is, "reverse")).unwrap_or(false);
                                let show_value = x14_is.and_then(|x14| attr_bool(x14, "showValue")).or_else(|| attr_bool(is, "showValue")).unwrap_or(true);
                                let (custom, icons) = if let Some(x14) = x14_is {
                                    let custom = attr_bool(x14, "custom").unwrap_or(false) || attr(x14, "custom") == Some("1");
                                    let list: Vec<Option<ibis_doc_model::CfIcon>> = x14.descendants()
                                        .filter(|n| n.tag_name().name() == "cfIcon")
                                        .map(|ci| {
                                            let s = attr(ci, "iconSet").unwrap_or("NoIcon").to_string();
                                            let id = attr_i64(ci, "iconId").unwrap_or(-1) as i32;
                                            if id < 0 || s == "NoIcon" {
                                                Some(ibis_doc_model::CfIcon::no_icon())
                                            } else {
                                                Some(ibis_doc_model::CfIcon::new(s, id))
                                            }
                                        })
                                        .collect();
                                    (custom, list)
                                } else {
                                    (false, Vec::new())
                                };
                                CfKind::IconSet {
                                    set,
                                    values: children(is, "cfvo").map(cfvo).collect(),
                                    reverse,
                                    show_value,
                                    custom,
                                    icons,
                                }
                            }
                            None => CfKind::Other,
                        },
                        _ => CfKind::Other,
                    };
                    let style = attr_i64(r, "dxfId")
                        .and_then(|i| styles.dxfs.get(i.max(0) as usize))
                        .cloned()
                        .or_else(|| {
                            child(r, "dxf")
                                .or_else(|| x14_of(r).and_then(|x| child(x, "dxf")))
                                .map(|d| parse_dxf(d, colors))
                        });
                    CfRule { kind, priority: attr_i64(r, "priority").unwrap_or(0).max(0) as u32, stop: attr_bool(r, "stopIfTrue").unwrap_or(false), style }
                })
                .collect();
            ConditionalFormat { source_index: Some(source_index), ranges, rules }
        })
        .collect()
}

/// Shared strings (`sharedStrings.xml`), rich text flattened.
pub(crate) fn read_shared_strings(root: Option<Node>) -> Vec<String> {
    root.into_iter().flat_map(|r| children(r, "si")).map(inline_text).collect()
}

/// The readings (`rPh`) of the shared strings, by index: the text of each
/// `rPh`, joined (`None` for a string without one).
pub(crate) fn read_shared_phonetics(root: Option<Node>) -> Vec<Option<std::sync::Arc<str>>> {
    root.into_iter()
        .flat_map(|r| children(r, "si"))
        .map(|si| {
            let text: String = children(si, "rPh").filter_map(|p| child(p, "t")).map(|t| t.text().unwrap_or("")).collect();
            (!text.is_empty()).then(|| text.into())
        })
        .collect()
}

/// What the shared strings carry besides their text, by index: readings
/// and the parts in their own fonts.
pub(crate) struct SharedExtras {
    pub(crate) phonetics: Vec<Option<std::sync::Arc<str>>>,
    pub(crate) runs: Vec<Option<std::sync::Arc<Vec<ibis_doc_model::CellRun>>>>,
}

pub(crate) fn read_shared_extras(root: Option<Node>, colors: &SheetColors) -> SharedExtras {
    SharedExtras {
        phonetics: read_shared_phonetics(root),
        runs: root.into_iter().flat_map(|r| children(r, "si")).map(|si| read_runs(si, colors)).collect(),
    }
}

/// The runs (`r`) of an `si` or `is` element with their fonts, if it has any.
pub(crate) fn read_runs(node: Node, colors: &SheetColors) -> Option<std::sync::Arc<Vec<ibis_doc_model::CellRun>>> {
    let runs: Vec<ibis_doc_model::CellRun> = children(node, "r")
        .map(|r| {
            let text = decode_xstring(&child(r, "t").and_then(|t| t.text()).unwrap_or(""));
            let mut run = ibis_doc_model::CellRun { text, ..Default::default() };
            if let Some(p) = child(r, "rPr") {
                let flag = |name: &str| Some(child(p, name).is_some_and(|c| attr_bool(c, "val").unwrap_or(true)));
                run.bold = flag("b");
                run.italic = flag("i");
                run.strike = flag("strike");
                run.underline = Some(child(p, "u").map_or(CellUnderline::None, |u| CellUnderline::from_ooxml(attr(u, "val").unwrap_or("single"))));
                run.font = child(p, "rFont").and_then(|n| attr(n, "val")).map(str::to_string);
                run.size = child(p, "sz").and_then(|n| attr(n, "val")).and_then(|v| v.parse::<f64>().ok()).map(|v| (v * 100.0).round() as u32);
                run.color = child(p, "color").and_then(|c| colors.color(c));
            }
            run
        })
        .collect();
    (!runs.is_empty()).then(|| std::sync::Arc::new(runs))
}

/// The text of an `si` or `is` element: its `t`, or its runs' `t`s.
/// Phonetic guides (`rPh`) are left out.
fn inline_text(node: Node) -> String {
    let mut out = String::new();
    for n in elements(node) {
        match n.tag_name().name() {
            "t" => out.push_str(n.text().unwrap_or("")),
            "r" => {
                if let Some(t) = child(n, "t") {
                    out.push_str(t.text().unwrap_or(""));
                }
            }
            _ => {}
        }
    }
    decode_xstring(&out)
}

/// Text as SpreadsheetML escapes it (`ST_Xstring`): `_xHHHH_` is the
/// character U+HHHH (control characters, which XML cannot hold), and
/// `_x005F_` an underscore that would otherwise start one.
pub(crate) fn decode_xstring(s: &str) -> String {
    if !s.contains("_x") {
        return s.to_string();
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if b[i] == b'_' && i + 7 <= s.len() && b[i + 1] == b'x' && b[i + 6] == b'_' && s[i + 2..i + 6].bytes().all(|c| c.is_ascii_hexdigit()) {
            if let Some(ch) = u32::from_str_radix(&s[i + 2..i + 6], 16).ok().and_then(char::from_u32) {
                out.push(ch);
                i += 7;
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Text encoded as SpreadsheetML escapes it (`ST_Xstring`):
/// control characters invalid in XML 1.0 are encoded as `_xHHHH_`.
pub(crate) fn encode_xstring(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let code = ch as u32;
        if (code <= 0x1f && ch != '\t' && ch != '\r' && ch != '\n') || code == 0x7f {
            out.push_str(&format!("_x{:04x}_", code));
        } else if ch == '_'
            && i + 6 < chars.len()
            && chars[i + 1] == 'x'
            && chars[i + 6] == '_'
            && chars[i + 2..i + 6].iter().all(|c| c.is_ascii_hexdigit())
        {
            out.push_str("_x005F_");
        } else {
            out.push(ch);
        }
        i += 1;
    }
    out
}

/// Repeat the fill character (`*x`) to fill the column width.
fn expand_fill(base: String, offset: usize, fill_char: char, col_width: f64) -> String {
    let count = if fill_char == ' ' {
        let total_w = (col_width * 2.0).round() as usize;
        let total_w = total_w.saturating_sub(2);
        let mut used_w: usize = base.chars().map(|c| match c {
            ' ' | '.' | ',' | '(' => 1,
            '+' | '%' => 3,
            _ => 2,
        }).sum();
        if base.contains('-') && base.contains('/') {
            used_w = used_w.saturating_sub(1);
        }
        total_w.saturating_sub(used_w)
    } else {
        let n = base.len();
        let total = (col_width * 6.0 - 5.0).max(0.0) as usize;
        total.saturating_sub(6 * n) / 5
    };
    if count == 0 {
        return base;
    }
    let mut out = String::with_capacity(base.len() + count * fill_char.len_utf8());
    let offset = offset.min(base.len());
    out.push_str(&base[..offset]);
    for _ in 0..count {
        out.push(fill_char);
    }
    out.push_str(&base[offset..]);
    out
}

/// A cell's value from its type (`c/@t`) and `v`, and the value as displayed.
fn cell_value(kind: Option<&str>, raw: &str, c: Node, strings: &[String], number_format: &str, date1904: bool, col_width: f64) -> (CellValue, String) {
    let value = match kind {
        Some("s") => CellValue::Text(raw.trim().parse::<usize>().ok().and_then(|i| strings.get(i)).cloned().unwrap_or_default()),
        Some("str") => {
            let text = decode_xstring(raw);
            if let Some(e) = CellError::parse(text.trim()) {
                CellValue::Error(e)
            } else {
                CellValue::Text(text)
            }
        },
        Some("inlineStr") => CellValue::Text(child(c, "is").map(inline_text).unwrap_or_default()),
        Some("b") => CellValue::Bool(raw.trim() == "1" || raw.trim().eq_ignore_ascii_case("true")),
        Some("e") => match CellError::parse(raw.trim()) {
            Some(e) => CellValue::Error(e),
            None => CellValue::Text(raw.to_string()),
        },
        _ if raw.is_empty() => CellValue::Empty,
        _ => match raw.trim().parse::<f64>() {
            Ok(v) => CellValue::Number(v),
            Err(_) => CellValue::Text(raw.to_string()),
        },
    };
    let text = display_with_width(&value, number_format, date1904, col_width);
    (value, text)
}

/// A value as a cell of the number format shows it (`#` for a value the
/// format cannot show, which Excel repeats across the cell).
#[allow(dead_code)]
pub(crate) fn display(value: &CellValue, number_format: &str, date1904: bool) -> String {
    display_with_width(value, number_format, date1904, 8.43)
}

/// A value as a cell of the number format shows it, expanding `*x` according
/// to the column width.
pub(crate) fn display_with_width(value: &CellValue, number_format: &str, date1904: bool, col_width: f64) -> String {
    let f = ibis_calc::numfmt::format(value, number_format, date1904);
    if f.overflow {
        "########".into()
    } else if let Some((offset, fill_char)) = f.fill {
        expand_fill(f.text, offset, fill_char, col_width)
    } else {
        f.text
    }
}

/// A cell reference ("B12") as 0-based column and row.
/// エラー チェックで無視するエラー (`ignoredErrors/ignoredError`).
fn read_ignored_errors(root: Node) -> Vec<ibis_doc_model::error_check::IgnoredError> {
    child(root, "ignoredErrors")
        .into_iter()
        .flat_map(|list| children(list, "ignoredError"))
        .map(|e| ibis_doc_model::error_check::IgnoredError {
            ranges: attr(e, "sqref").map(|s| s.split_whitespace().filter_map(parse_range).collect()).unwrap_or_default(),
            attributes: e.attributes().filter(|a| a.name() != "sqref" && matches!(a.value(), "1" | "true")).map(|a| a.name().to_string()).collect(),
        })
        .collect()
}

pub(crate) fn cell_ref(reference: &str) -> Option<(u32, u32)> {
    let split = reference.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = reference.split_at(split);
    let mut column = 0u32;
    for c in letters.chars() {
        if !c.is_ascii_uppercase() {
            return None;
        }
        column = column * 26 + (c as u32 - 'A' as u32 + 1);
    }
    let row: u32 = digits.parse().ok()?;
    Some((column.checked_sub(1)?, row.checked_sub(1)?))
}

/// The width of a column `chars` characters wide (the stored width, which
/// counts the cell's padding) for a maximum digit width of `digit` points,
/// by ECMA-376 Part 1 §18.3.1.13 in points: Excel for Mac saves 13.75 as
/// 13.6640625 and prints both 82 pt wide (xlsx-XALN), 1.25 as 1.1640625
/// (7 pt), 8.75 as 8.6640625 (52 pt).
pub(crate) fn column_pixels(chars: f64, digit: f64) -> f64 {
    column_pixels_in(EXCEL_METRICS, chars, digit)
}

/// The same in `metrics`' pixels: the whole pixels are of `digit` ÷ the pixel's
/// points, the result in points.
pub(crate) fn column_pixels_in(metrics: ExcelMetrics, chars: f64, digit: f64) -> f64 {
    let pixel = metrics.pixel_pt();
    let d = digit / pixel;
    let px = ((256.0 * chars + (128.0 / d).floor()) / 256.0) * d;
    // ECMA-376's formula truncates to the pixel (as Excel does); a width
    // written from points then reads back as the same pixels.
    px.floor() * pixel
}

/// The width of a column with none of its own, in points: `defaultColWidth`
/// characters, or `baseColWidth` (8 if not given) characters and Excel's
/// padding.
pub(crate) fn default_column_width(format: Option<Node>, digit: f64) -> f64 {
    format
        .and_then(|f| attr(f, "defaultColWidth"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|w| column_pixels(w, digit))
        .unwrap_or_else(|| {
            let base = format.and_then(|f| attr(f, "baseColWidth")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(8.0);
            let pixel = EXCEL_METRICS.pixel_pt();
            // The width of a column without `defaultColWidth` is the base width's pixels and 5 more for the cell's padding and
            // grid line, rounded up to a multiple of 8 pixels (ECMA-376 18.3.1.81 on `baseColWidth`): 8 digits of 8 px and 5 make
            // 69 px, which Excel for Windows prints as 72 (a column of 72 px: 49.7 pt at 100 % where 69 px gave 47.6 pt); 8 digits
            // of 7 px and 5 make 61, shown as 64.
            let pixels = base * (digit / pixel) + 5.0;
            if EXCEL_METRICS == ExcelMetrics::Windows { (pixels / 8.0).ceil() * 8.0 * pixel } else { pixels.trunc() * pixel }
        })
}

pub(crate) fn unqualified_attr<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<&'a str> {
    // roxmltree's attribute(name) also matches namespaced attributes by local
    // name. Metadata fields must be unqualified, regardless of source order.
    node.attributes().find(|a| a.namespace().is_none() && a.name() == name).map(|a| a.value())
}

/// Parse a metadata index after XML Schema whitespace normalization. Internal
/// spaces and non-XML whitespace remain invalid; range checks follow the parse.
pub(crate) fn metadata_index(value: &str) -> Option<u32> {
    value.trim_matches([' ', '\t', '\r', '\n']).parse().ok()
}

/// The rich-value errors by value metadata index (`c/@vm`, from 1): the
/// `valueMetadata` record of the XLRICHVALUE type → its future block's
/// `xlrd:rvb/@i` → the rich value (`rdrichvalue.xml`) of an `_error`
/// structure, whose `errorType` is ERROR.TYPE less one (2 #VALUE!, 8 #SPILL!,
/// 13 #CALC!; Excel's own files). Anything else is left out.
pub(crate) fn rich_value_errors(metadata: Node, values: Node, structures: Node) -> HashMap<u32, CellError> {
    const RICH: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata";
    let mut out = HashMap::new();
    let types: Vec<_> = child(metadata, "metadataTypes").into_iter().flat_map(|t| children(t, "metadataType")).map(|n| attr(n, "name")).collect();
    let Some(kind) = types.iter().position(|n| *n == Some("XLRICHVALUE")).map(|i| i + 1) else { return out };
    let Some(future) = children(metadata, "futureMetadata").find(|n| attr(*n, "name") == Some("XLRICHVALUE")) else { return out };
    let blocks: Vec<Option<usize>> = children(future, "bk")
        .map(|bk| bk.descendants().find(|n| n.tag_name().name() == "rvb" && (n.tag_name().namespace() == Some(RICH) || n.tag_name().namespace().is_none())).and_then(|n| attr(n, "i")).and_then(|i| i.parse().ok()))
        .collect();
    let structures: Vec<(Option<&str>, Vec<&str>)> = elements(structures)
        .filter(|n| n.tag_name().name() == "s")
        .map(|s| (attr(s, "t"), elements(s).filter(|k| k.tag_name().name() == "k").filter_map(|k| attr(k, "n")).collect()))
        .collect();
    let rich: Vec<Option<u32>> = elements(values)
        .filter(|n| n.tag_name().name() == "rv")
        .map(|rv| {
            let (t, keys) = structures.get(attr(rv, "s")?.parse::<usize>().ok()?)?;
            if *t != Some("_error") { return None; }
            let at = keys.iter().position(|k| *k == "errorType")?;
            elements(rv).filter(|n| n.tag_name().name() == "v").nth(at)?.text()?.trim().parse().ok()
        })
        .collect();
    let Some(cells) = child(metadata, "valueMetadata") else { return out };
    for (index, bk) in children(cells, "bk").enumerate() {
        let Some(rc) = children(bk, "rc").find(|rc| attr(*rc, "t").and_then(|t| t.parse::<usize>().ok()) == Some(kind)) else { continue };
        let Some(rv) = attr(rc, "v").and_then(|v| v.parse::<usize>().ok()).and_then(|v| blocks.get(v).copied().flatten()) else { continue };
        let Some(error_type) = rich.get(rv).copied().flatten() else { continue };
        let error = match error_type {
            0 => CellError::Null, 1 => CellError::Div0, 2 => CellError::Value, 3 => CellError::Ref,
            4 => CellError::Name, 5 => CellError::Num, 6 => CellError::NA, 7 => CellError::GettingData,
            8 => CellError::Spill, 9 => CellError::Connect, 10 => CellError::Blocked, 11 => CellError::Unknown,
            12 => CellError::Field, 13 => CellError::Calc,
            _ => continue,
        };
        out.insert(index as u32 + 1, error);
    }
    out
}

/// The pictures in cells (セルに配置): the same chain as [`rich_value_errors`], to a
/// `_localImage` structure whose `_rvRel:LocalImageIdentifier` is the index of
/// the picture's relationship among `richValueRel.xml`'s. By the cell's `vm`.
pub(crate) fn rich_value_images(metadata: Node, values: Node, structures: Node) -> HashMap<u32, u32> {
    const RICH: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata";
    let mut out = HashMap::new();
    let types: Vec<_> = child(metadata, "metadataTypes").into_iter().flat_map(|t| children(t, "metadataType")).map(|n| attr(n, "name")).collect();
    let Some(kind) = types.iter().position(|n| *n == Some("XLRICHVALUE")).map(|i| i + 1) else { return out };
    let Some(future) = children(metadata, "futureMetadata").find(|n| attr(*n, "name") == Some("XLRICHVALUE")) else { return out };
    let blocks: Vec<Option<usize>> = children(future, "bk")
        .map(|bk| bk.descendants().find(|n| n.tag_name().name() == "rvb" && (n.tag_name().namespace() == Some(RICH) || n.tag_name().namespace().is_none())).and_then(|n| attr(n, "i")).and_then(|i| i.parse().ok()))
        .collect();
    let structures: Vec<(Option<&str>, Vec<&str>)> = elements(structures)
        .filter(|n| n.tag_name().name() == "s")
        .map(|s| (attr(s, "t"), elements(s).filter(|k| k.tag_name().name() == "k").filter_map(|k| attr(k, "n")).collect()))
        .collect();
    let images: Vec<Option<u32>> = elements(values)
        .filter(|n| n.tag_name().name() == "rv")
        .map(|rv| {
            let (t, keys) = structures.get(attr(rv, "s")?.parse::<usize>().ok()?)?;
            if *t != Some("_localImage") { return None; }
            let at = keys.iter().position(|k| *k == "_rvRel:LocalImageIdentifier")?;
            elements(rv).filter(|n| n.tag_name().name() == "v").nth(at)?.text()?.trim().parse().ok()
        })
        .collect();
    let Some(cells) = child(metadata, "valueMetadata") else { return out };
    for (index, bk) in children(cells, "bk").enumerate() {
        let Some(rc) = children(bk, "rc").find(|rc| attr(*rc, "t").and_then(|t| t.parse::<usize>().ok()) == Some(kind)) else { continue };
        let Some(rv) = attr(rc, "v").and_then(|v| v.parse::<usize>().ok()).and_then(|v| blocks.get(v).copied().flatten()) else { continue };
        if let Some(rel) = images.get(rv).copied().flatten() {
            out.insert(index as u32 + 1, rel);
        }
    }
    out
}

/// Qualified (type, future block) records that explicitly declare a dynamic
/// array. Unrecognized metadata remains opaque.
pub(crate) fn dynamic_metadata_records(root: Node) -> HashSet<(u32, u32)> {
    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    const DYNAMIC: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray";
    const URI: &str = "{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}";
    fn standard<'a, 'i>(node: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
        elements(node).filter(move |n| n.has_tag_name((MAIN, name)))
    }
    fn one<'a, 'i>(node: Node<'a, 'i>, name: &'static str) -> Option<Node<'a, 'i>> {
        let mut nodes = standard(node, name);
        let first = nodes.next()?;
        nodes.next().is_none().then_some(first)
    }
    let mut indices = HashSet::new();
    if !root.has_tag_name((MAIN, "metadata")) { return indices; }
    let (Some(types), Some(_cells)) = (one(root, "metadataTypes"), one(root, "cellMetadata")) else { return indices };
    let types: Vec<_> = standard(types, "metadataType").map(|n| unqualified_attr(n, "name")).collect();
    if types.iter().any(|n| n.is_none()) || types.iter().filter(|n| **n == Some("XLDAPR")).count() != 1 { return indices; }
    let kind = types.iter().position(|n| *n == Some("XLDAPR")).unwrap() + 1;
    let mut futures = standard(root, "futureMetadata").filter(|n| unqualified_attr(*n, "name") == Some("XLDAPR"));
    let Some(future) = futures.next() else { return indices };
    if futures.next().is_some() { return indices; }
    let flags: Vec<bool> = standard(future, "bk").map(|bk| {
        let Some(list) = one(bk, "extLst") else { return false };
        let props: Vec<_> = standard(list, "ext").filter(|ext| unqualified_attr(*ext, "uri") == Some(URI))
            .flat_map(elements).filter(|n| n.has_tag_name((DYNAMIC, "dynamicArrayProperties"))).collect();
        props.len() == 1 && unqualified_attr(props[0], "fDynamic").is_some_and(|value| matches!(value.trim_matches([' ', '\t', '\r', '\n']), "1" | "true"))
    }).collect();
    let Ok(kind) = u32::try_from(kind) else { return indices };
    for (v, dynamic) in flags.into_iter().enumerate() {
        if dynamic { if let Ok(v) = u32::try_from(v) { indices.insert((kind, v)); } }
    }
    indices
}

/// Cell metadata is dynamic only if one of its records references an
/// explicitly dynamic XLDAPR future block.
pub(crate) fn dynamic_metadata_indices(root: Node) -> HashSet<u32> {
    const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    let records = dynamic_metadata_records(root);
    let mut indices = HashSet::new();
    if records.is_empty() { return indices; }
    let Some(cells) = elements(root).find(|n| n.has_tag_name((MAIN, "cellMetadata"))) else { return indices };
    for (index, bk) in elements(cells).filter(|n| n.has_tag_name((MAIN, "bk"))).enumerate() {
        if elements(bk).filter(|n| n.has_tag_name((MAIN, "rc"))).any(|rc| {
            unqualified_attr(rc, "t").and_then(metadata_index).zip(unqualified_attr(rc, "v").and_then(metadata_index))
                .is_some_and(|record| records.contains(&record))
        }) { if let Ok(index) = u32::try_from(index + 1) { indices.insert(index); } }
    }
    indices
}

/// The default row height for a sheet in points.
/// If `sheetFormatPr/@customHeight` is true, the declared `defaultRowHeight`
/// is used (defaulting to 15.0 pt if omitted).
/// If `customHeight` is false/omitted, Excel calculates the row height from the default
/// font. In Excel for Mac (with Calibri 11pt), the default row height is 18.0 pt.
///
/// With ＭＳ Ｐゴシック and the other MS fonts as the default font, Excel keeps
/// the declared `defaultRowHeight` (13.5 pt: the rows of the chart sheets of
/// a user's private workbook are 13.5 pt in Excel's PDF, not 18).
pub(crate) fn default_row_height(format: Option<Node>, default_font: (&Option<String>, u32)) -> f64 {
    default_row_height_in(EXCEL_METRICS, format, default_font)
}

/// Which Excel the sizes of rows and columns follow. The two lay the same file
/// out a little differently: Excel for Windows (desktop) counts a column in
/// whole 96 dpi pixels (the digit 7 px, 0.75 pt a pixel) and keep a declared
/// `defaultRowHeight`; Excel for Mac counts a pixel as a point (the digit 6 pt)
/// and fits the rows without a height to the font (18 pt for 11 pt).
/// One switch for all of them: the screen and the print use the same one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExcelMetrics {
    Windows,
    Mac,
}

/// The sizes the reader lays sheets out by (the user's decision on which Excel
/// to follow: Excel for Windows (desktop), decided by the user 2026-10-20).
pub const EXCEL_METRICS: ExcelMetrics = ExcelMetrics::Windows;

/// The Normal font a workbook gets without one named (Office's default).
const DEFAULT_FONT: &str = "Calibri";

/// The space between a cell's edge and its text, in points (each side).
const CELL_TEXT_PADDING_PT: f64 = 2.0;

impl ExcelMetrics {
    /// A pixel of a column's width, in points.
    pub fn pixel_pt(self) -> f64 {
        match self {
            ExcelMetrics::Windows => 0.75,
            ExcelMetrics::Mac => 1.0,
        }
    }

    /// What Excel for Windows prints a column at, against its width on the screen: the Normal font's
    /// widest digit is measured in whole pixels on each device (`font_digit`): 96 dpi on the screen, the
    /// printer's 600 dpi in print (1/600 inch = 0.12 pt), so a column of the same characters is
    /// `printed_digit * 0.12 pt` against `screen_digit * 0.75 pt` wide. Excel 365's PDFs: a column stored 10
    /// characters wide is 46 * 10 units for MS PGothic 11 (60 pt on the screen: 0.92), 51 for
    /// Yu Gothic 11 (1.02), 47 for Calibri 11 (52.5 pt: 1.07), 40 for Courier New 8, ...
    pub fn print_column_factor(self, font: Option<&str>, font_pt: f64) -> f64 {
        match self {
            ExcelMetrics::Windows => {
                let printed = crate::font_digit::max_digit_px(font.unwrap_or(DEFAULT_FONT), font_pt, 600.0).map_or_else(|| (font_pt * 600.0 / 72.0 * 0.5).round().max(1.0), f64::from);
                printed * 0.12 / self.digit_pt(font, font_pt)
            }
            ExcelMetrics::Mac => 1.0,
        }
    }

    /// The maximum digit width of the default font (`font_pt` points), in points.
    pub fn digit_pt(self, font: Option<&str>, font_pt: f64) -> f64 {
        match self {
            // The widest digit of the Normal font in whole pixels at 96 dpi, as GDI measures it (Excel does the
            // same): 8 px for MS PGothic or Yu Gothic 11 pt, 7 for Calibri 11 pt, so a column stored 9.4414
            // characters wide is 76 px = 57 pt (Excel 365's `Columns.Width`). A font this machine does not have,
            // or no GDI (macOS, Linux): half an em.
            ExcelMetrics::Windows => {
                let measured = crate::font_digit::max_digit_px(font.unwrap_or(DEFAULT_FONT), font_pt, 96.0);
                measured.map_or_else(|| (font_pt * 96.0 / 72.0 * 0.5).round(), f64::from).max(1.0) * 0.75
            }
            // About 0.55 em, as Excel for Mac measures it (6 pt for 11 pt).
            ExcelMetrics::Mac => (font_pt * 0.55).round().max(1.0),
        }
    }
}

pub(crate) fn default_row_height_in(metrics: ExcelMetrics, format: Option<Node>, default_font: (&Option<String>, u32)) -> f64 {
    let (font_name, default_font_size) = default_font;
    // The declared `defaultRowHeight` is what Excel for Windows lays the rows
    // without a height out by; Excel for Mac keeps it only for the MS fonts and
    // works the height out from the font otherwise.
    let ms_font = font_name.as_deref().is_some_and(|n| n.starts_with("ＭＳ") || n.starts_with("MS "));
    let declared = format.and_then(|f| attr(f, "defaultRowHeight")).and_then(|v| v.parse::<f64>().ok());
    let keeps_declared = metrics == ExcelMetrics::Windows || ms_font;
    let custom_height = format.and_then(|f| attr_bool(f, "customHeight")).unwrap_or(false) || (keeps_declared && declared.is_some());
    if custom_height {
        declared.unwrap_or(15.0)
    } else {
        let font_pt = (default_font_size as f64 / 100.0).max(1.0);
        (font_pt * (18.0 / 11.0)).round().max(18.0)
    }
}

/// Single-line row height in points for a given font size, calibrated against Excel for Mac (Calibri).
fn font_row_height(font_pt: f64) -> f64 {
    if font_pt <= 11.0 {
        18.0
    } else if (font_pt - 18.0).abs() < 1.0 {
        24.0
    } else if (font_pt - 24.0).abs() < 1.0 {
        31.0
    } else if (font_pt - 36.0).abs() < 1.0 {
        47.0
    } else {
        (18.0 + (font_pt - 11.0) * (29.0 / 25.0)).round().max(18.0)
    }
}

/// A line of text of `font` at `font_pt` in points, as Excel for Mac fits a
/// row to it. Measured on Excel for Mac's PDF (macOS 26.5.1) of
/// a user's private workbook (its cover sheet), 2026-10-11:
/// - Meiryo UI: 16 pt a line at 10 pt (rows 8 to 13: text pitch 16.0 with
///   15 pt stored); at 14 pt only inferred (row 1: the text sits as if the row
///   were about 19.5 pt, the stored height) and at 9 pt not measured. The
///   formula 7 + 0.9 × size is a line through those two points, not a rule.
/// - ＭＳ Ｐゴシック (the workbook's default font): 13.5 pt at 11 pt (rows with
///   no text: pitch 13.5 = the stored default row height); other sizes are
///   this ratio, 1.2273, unmeasured.
/// - Others: `font_row_height` (Calibri, measured earlier on the coverage suites).
fn line_height(font: Option<&str>, font_pt: f64) -> f64 {
    match font {
        Some(f) if f.contains("Meiryo") => 7.0 + font_pt * 0.9,
        Some(f) if f.starts_with("ＭＳ") || f.starts_with("MS ") => font_pt * 1.2273,
        _ => font_row_height(font_pt),
    }
}

/// Excel's most a row can be high (409.5 pt): a cell of many lines stops there, as Excel's automatic height does.
pub(crate) const MAX_ROW_HEIGHT_PT: f64 = 409.5;

/// Calculate the needed row height in points for a cell with text, column width, font size and wrap setting.
pub(crate) fn cell_needed_height(text: &str, col_chars: f64, font: Option<&str>, font_pt: f64, wrap: bool, bold: bool) -> f64 {
    cell_needed_height_uncapped(text, col_chars, font, font_pt, wrap, bold).min(MAX_ROW_HEIGHT_PT)
}

fn cell_needed_height_uncapped(text: &str, col_chars: f64, font: Option<&str>, font_pt: f64, wrap: bool, bold: bool) -> f64 {
    let line = line_height(font, font_pt);
    if text.is_empty() {
        return line;
    }
    let mut total_lines = 0;
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            total_lines += 1;
            continue;
        }
        if !wrap {
            total_lines += 1;
        } else {
            // Halfwidth characters count as 1 unit, fullwidth (CJK etc.) as 2 units.
            let mut width_units = 0;
            for c in paragraph.chars() {
                if c.is_ascii() {
                    width_units += 1;
                } else {
                    width_units += 2;
                }
            }
            // A half-width letter is half the font's size wide, a column
            // `col_chars` digits (6 pt) wide: 10 pt Meiryo UI holds 1.2 times
            // the units of the digit count.
            let measured = font.is_some_and(|f| f.contains("Meiryo") || f.starts_with("ＭＳ") || f.starts_with("MS "));
            let lines = if measured {
                // Wrapped by the widths of its characters, as Excel breaks
                // it: a CJK character an em, a digit about 0.58 em, other
                // ASCII 0.28 to 0.64 em (bold 6 % wider), inside the column
                // less 6 pt (its padding). The header "第28期見込(2026)"
                // (Meiryo UI 10 pt bold, 15 characters wide) is 2 lines in
                // Excel for Mac's PDF of one of the user's private
                // workbooks (a summary sheet, stored 28.5 pt, printed 29 pt).
                let inner = (col_chars * 6.0 - 6.0).max(font_pt);
                let em = font_pt * if bold { 1.06 } else { 1.0 };
                let width_of = |c: char| -> f64 {
                    em * if !c.is_ascii() {
                        1.0
                    } else if c.is_ascii_digit() {
                        0.58
                    } else if c == ' ' {
                        0.28
                    } else if c.is_ascii_uppercase() {
                        0.64
                    } else if c.is_ascii_lowercase() {
                        0.55
                    } else {
                        0.35
                    }
                };
                let mut lines = 1usize;
                let mut used = 0.0;
                for c in paragraph.chars() {
                    let w = width_of(c);
                    if used + w > inner && used > 0.0 {
                        lines += 1;
                        used = 0.0;
                    }
                    used += w;
                }
                lines
            } else {
                (width_units as f64 / col_chars.max(1.0)).ceil() as usize
            };
            total_lines += lines.max(1);
        }
    }
    if wrap {
        let pitch = if font.is_some_and(|f| f.contains("Meiryo") || f.starts_with("ＭＳ") || f.starts_with("MS ")) {
            line
        } else if font_pt > 11.0 {
            (19.0 * font_pt / 11.0).round()
        } else {
            19.0
        };
        (total_lines.max(1) as f64 * pitch).max(line)
    } else if total_lines > 1 {
        let pitch = if font.is_some_and(|f| f.contains("Meiryo") || f.starts_with("ＭＳ") || f.starts_with("MS ")) {
            line
        } else if font_pt > 11.0 {
            (19.0 * font_pt / 11.0).round()
        } else {
            19.0
        };
        total_lines as f64 * pitch
    } else {
        line
    }
}

/// Read one worksheet. `digit` is the default font's maximum digit width,
/// which column widths are measured in. Widths come out in points, as Excel
/// for Mac lays them out (a "pixel" there is a point).
pub(crate) fn read_sheet(root: Node, styles: &Styles, strings: &[String], extras: &SharedExtras, digit: f64, colors: &SheetColors, dynamic_metadata: &HashSet<u32>) -> Sheet {
    let format = child(root, "sheetFormatPr");
    let default_row_pt = default_row_height(format, (&styles.default_font.0, styles.default_font.1));
    // The line heights below are Excel for Mac's (18 pt for an 11 pt font).
    // A file that declares its own default row height lays its text rows out
    // by that height: the lines are scaled by the declared one over the Mac one.
    let row_scale = if EXCEL_METRICS == ExcelMetrics::Windows && format.and_then(|f| attr(f, "defaultRowHeight")).is_some() {
        let mac = ((styles.default_font.1 as f64 / 100.0).max(1.0) * (18.0 / 11.0)).round().max(18.0);
        default_row_pt / mac
    } else {
        1.0
    };
    let scaled = |font: Option<&str>, height: f64| if font.is_some_and(|f| f.contains("Meiryo") || f.starts_with("ＭＳ") || f.starts_with("MS ")) { height } else { height * row_scale };
    let declared_default = EXCEL_METRICS == ExcelMetrics::Windows && format.and_then(|f| attr(f, "defaultRowHeight")).is_some();
    let default_width = default_column_width(format, digit);
    let px = |v: f64| (v * EMU_PER_POINT as f64).round() as Emu;
    let pt = |v: f64| (v * EMU_PER_POINT as f64).round() as Emu;

    let default_col_chars = format.and_then(|f| attr(f, "defaultColWidth")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(8.43);
    // Column widths as given, by index.
    let mut widths: HashMap<u32, Emu> = HashMap::new();
    let mut col_chars: HashMap<u32, f64> = HashMap::new();
    let mut hidden_columns = std::collections::BTreeMap::new();
    let mut spills = std::collections::BTreeMap::new();
    let mut data_tables: Vec<ibis_doc_model::DataTable> = Vec::new();
    let mut outline_rows = std::collections::BTreeMap::new();
    let mut outline_columns = std::collections::BTreeMap::new();
    let mut hidden_rows = std::collections::BTreeMap::new();
    let mut custom_rows = std::collections::BTreeSet::new();
    let mut custom_columns = std::collections::BTreeSet::new();
    let mut max_column = 0u32;
    let mut column_fonts: Vec<(u32, u32, Option<String>, u32)> = Vec::new();
    let mut row_fonts: std::collections::BTreeMap<u32, (Option<String>, u32)> = std::collections::BTreeMap::new();
    // The styles (xf) of the cells written in each row, each once (see `Sheet::written_fonts`).
    let mut written_xfs: std::collections::BTreeMap<u32, Vec<usize>> = std::collections::BTreeMap::new();
    for col in child(root, "cols").into_iter().flat_map(|c| children(c, "col")) {
        let (Some(min), Some(max)) = (attr_i64(col, "min"), attr_i64(col, "max")) else { continue };
        let hidden = attr_bool(col, "hidden").unwrap_or(false);
        let char_w = attr(col, "width").and_then(|v| v.parse::<f64>().ok()).unwrap_or(default_col_chars);
        let width = column_pixels(char_w, digit);
        let level = attr_i64(col, "outlineLevel").unwrap_or(0).clamp(0, 7) as u8;
        let col_format = attr_i64(col, "style").filter(|&s| s > 0).and_then(|s| styles.formats.get(s as usize));
        column_fonts.push((min.max(1) as u32 - 1, max.clamp(1, 16_384) as u32 - 1, col_format.and_then(|f| f.style.font.clone()), col_format.map_or(0, |f| f.style.font_size)));
        for c in (min.max(1) as u32 - 1)..(max.clamp(1, 16_384) as u32) {
            if level > 0 {
                outline_columns.insert(c, level);
            }
            col_chars.insert(c, char_w);
            // A width the user fixed (`customWidth`): the column does not widen by itself when a number is entered.
            if attr_bool(col, "customWidth").unwrap_or(false) {
                custom_columns.insert(c);
            }
            widths.insert(c, if hidden { 0 } else { px(width) });
            if hidden {
                hidden_columns.insert(c, px(width));
            }
        }
    }

    let mut cells = Vec::new();
    let mut cell_images: Vec<ibis_doc_model::CellImage> = Vec::new();
    let mut heights: HashMap<u32, Emu> = HashMap::new();
    // Rows Excel measures by their fonts (no `customHeight`, no wrapping text): see `Sheet::measured_rows`.
    let mut measured: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut max_row = 0u32;
    // The rows that have a cell element (a row of only a height is not part of what Excel prints).
    let mut rows_with_cells: HashSet<u32> = HashSet::new();
    let mut style_index: HashMap<usize, u32> = HashMap::new();
    let mut sheet_styles: Vec<CellStyle> = Vec::new();
    let mut row_styles: Vec<(u32, u32)> = Vec::new();
    // Shared formulas by index: the formula and the cell it is written for.
    let mut shared: HashMap<String, (String, u32, u32)> = HashMap::new();
    let (mut rows_seen, mut rows_reported) = (0u32, 0usize);
    // The cells of merged ranges: Excel does not fit a row to the text of a merged cell.
    let merged_cells: Vec<CellRange> = child(root, "mergeCells")
        .into_iter()
        .flat_map(|m| children(m, "mergeCell"))
        .filter_map(|m| {
            let (a, b) = attr(m, "ref")?.split_once(':')?;
            let ((c0, r0), (c1, r1)) = (cell_ref(a)?, cell_ref(b)?);
            Some(CellRange { first_row: r0.min(r1), first_column: c0.min(c1), last_row: r0.max(r1), last_column: c0.max(c1) })
        })
        .collect();
    for row in child(root, "sheetData").into_iter().flat_map(|d| children(d, "row")) {
        // The progress bar of the loading box.
        rows_seen += 1;
        if rows_seen % 2048 == 0 {
            let end = row.range().end;
            crate::progress::rows_done((end - rows_reported) as u64);
            rows_reported = end;
        }
        let Some(r) = attr_i64(row, "r").map(|v| (v.max(1) - 1) as u32) else { continue };
        if let Some(level) = attr_i64(row, "outlineLevel").filter(|&l| l > 0) {
            outline_rows.insert(r, level.min(7) as u8);
        }
        let hidden = attr_bool(row, "hidden").unwrap_or(false);
        let custom_height = attr_bool(row, "customHeight").unwrap_or(false);
        let declared_ht = attr(row, "ht").and_then(|v| v.parse::<f64>().ok());
        // Excel for Windows fits a row that has no height to what is in it (the
        // default height only for a row with nothing in it).
        let fits_content = EXCEL_METRICS == ExcelMetrics::Windows && declared_default;
        let mut auto_ht = if fits_content { 0.0 } else { default_row_pt };
        let win_step = |h: f64| if fits_content { (h / 0.75 - 1e-6).ceil() * 0.75 } else { h };
        if attr_bool(row, "customFormat").unwrap_or(false) {
            let format = attr_i64(row, "s").filter(|&s| s > 0).and_then(|s| styles.formats.get(s as usize));
            row_fonts.insert(r, (format.and_then(|f| f.style.font.clone()), format.map_or(0, |f| f.style.font_size)));
        }
        let row_xf = attr_i64(row, "s")
            .filter(|&s| s > 0 && attr_bool(row, "customFormat").unwrap_or(true))
            .map(|s| s as usize);
        if let Some(xf) = row_xf
            && let Some(format) = styles.formats.get(xf)
        {
            let style = &format.style;
            let visible = style.fill.is_some()
                || style.gradient.is_some()
                || style.pattern.is_some()
                || style.left.is_some()
                || style.right.is_some()
                || style.top.is_some()
                || style.bottom.is_some()
                || style.diagonal_up.is_some()
                || style.diagonal_down.is_some();
            if visible {
                let index = *style_index.entry(xf).or_insert_with(|| {
                    sheet_styles.push(style.clone());
                    (sheet_styles.len() - 1) as u32
                });
                max_row = max_row.max(r + 1);
                row_styles.push((r, index));
            }
        }
        let mut wraps_text = false;
        let mut any_wrap = false;
        // A cell without a reference follows the one before it.
        let mut next_column = 0u32;
        for c in children(row, "c") {
            let Some((column, row_index)) = (match attr(c, "r") {
                Some(reference) => cell_ref(reference),
                None => Some((next_column, r)),
            }) else {
                continue;
            };
            next_column = column + 1;
            rows_with_cells.insert(row_index);
            let xf = attr_i64(c, "s").map(|s| s.max(0) as usize).or(row_xf).unwrap_or(0);
            let format = styles.formats.get(xf).or_else(|| styles.formats.first());
            let raw = child(c, "v").and_then(|v| v.text()).unwrap_or("");
            let col_width = col_chars.get(&column).copied().unwrap_or(default_col_chars);
            let (mut value, mut text) = cell_value(attr(c, "t"), raw, c, strings, format.map_or("General", |f| f.number_format.as_str()), styles.date1904, col_width);
            // An error whose true kind is in the rich data (#CALC!, #SPILL!).
            if matches!(value, CellValue::Error(_))
                && let Some(&error) = unqualified_attr(c, "vm").and_then(metadata_index).and_then(|i| styles.rich_errors.get(&i))
            {
                value = CellValue::Error(error);
                text = display_with_width(&value, format.map_or("General", |f| f.number_format.as_str()), styles.date1904, col_width);
            }
            let in_merge = merged_cells.iter().any(|m| m.first_row <= row_index && row_index <= m.last_row && m.first_column <= column && column <= m.last_column);
            // A row with a wrapping cell (a merged one too) is left at the height the file has: this is a limit of ibisOffice, not a
            // rule of Microsoft Excel, which fits such a row to the lines of the text (a merged cell is not fitted); ibisOffice does not
            // measure the lines of wrapped text with the formula of `ibis-office-app`'s `xlsx::row_fit` yet.
            any_wrap |= format.is_some_and(|f| f.style.wrap);
            if !in_merge {
                let list = written_xfs.entry(row_index).or_default();
                if !list.contains(&xf) {
                    list.push(xf);
                }
            }
            if !custom_height && !hidden && in_merge && !text.is_empty() {
                // A merged cell does not fit its row to its text (it does not wrap into more rows), but a larger
                // font still makes the row as tall as one line of it (Excel's PDFs of xlsx-XCEL).
                let font_pt = format.map_or(styles.default_font.1 as f64 / 100.0, |f| f.style.font_size as f64 / 100.0).max(1.0);
                let font_name = format.and_then(|f| f.style.font.as_deref()).or(styles.default_font.0.as_deref());
                auto_ht = auto_ht.max(win_step(scaled(font_name, line_height(font_name, font_pt))));
            }
            if !custom_height && !hidden && !in_merge {
                let font_pt = format.map_or(styles.default_font.1 as f64 / 100.0, |f| f.style.font_size as f64 / 100.0).max(1.0);
                let wrap = format.map_or(false, |f| f.style.wrap);
                any_wrap |= wrap;
                // A cell with nothing in it does not make its row taller.
                if !text.is_empty() {
                    let font_name = format.and_then(|f| f.style.font.as_deref()).or(styles.default_font.0.as_deref());
                    // Only rows of fonts whose Mac metrics differ from the ones
                    // the file was saved with are fitted again (see below).
                    wraps_text |= wrap && font_name.is_some_and(|f| f.contains("Meiryo") || f.starts_with("ＭＳ") || f.starts_with("MS "));
                    // Excel for Windows leaves the declared default height to a row of
                    // one line of the default font's size (it fits a row again only
                    // when it is edited).
                    auto_ht = auto_ht.max(win_step(scaled(font_name, cell_needed_height(&text, col_width, font_name, font_pt, wrap, format.is_some_and(|f| f.style.bold)))));
                }
            }
            // A picture in the cell (セルに配置): drawn over it, the cell's own `#VALUE!` not shown.
            if let Some(&index) = unqualified_attr(c, "vm").and_then(metadata_index).and_then(|i| styles.rich_images.get(&i)) {
                cell_images.push(ibis_doc_model::CellImage { row: row_index, column, index });
                text = String::new();
            }
            let kind = CellKind::of(&value);
            // A dynamic array formula (`cm` with an array formula): the area its result fills.
            if unqualified_attr(c, "cm").and_then(metadata_index).is_some_and(|index| dynamic_metadata.contains(&index))
                && c.has_tag_name(("http://schemas.openxmlformats.org/spreadsheetml/2006/main", "c"))
                && let Some(f) = elements(c).find(|n| n.has_tag_name(("http://schemas.openxmlformats.org/spreadsheetml/2006/main", "f")))
                && f.has_tag_name(("http://schemas.openxmlformats.org/spreadsheetml/2006/main", "f"))
                && unqualified_attr(f, "t") == Some("array")
                && let Some(range) = unqualified_attr(f, "ref").and_then(parse_range)
            {
                spills.insert((row_index, column), range);
            }
            if value == CellValue::Error(CellError::Spill) {
                spills.entry((row_index, column)).or_insert(CellRange {
                    first_row: row_index,
                    first_column: column,
                    last_row: row_index,
                    last_column: column,
                });
            }
            // A data table (`f t="dataTable"`): its results are values.
            if let Some(f) = elements(c).find(|n| n.has_tag_name(("http://schemas.openxmlformats.org/spreadsheetml/2006/main", "f")) && unqualified_attr(*n, "t") == Some("dataTable"))
                && let Some(range) = unqualified_attr(f, "ref").and_then(parse_range)
            {
                let input = |name: &str| unqualified_attr(f, name).and_then(cell_ref).map(|(c, r)| (r, c));
                let flag = |name: &str| unqualified_attr(f, name).is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
                let (row_input, column_input) = if flag("dt2D") { (input("r1"), input("r2")) } else if flag("dtr") { (input("r1"), None) } else { (None, input("r1")) };
                data_tables.push(ibis_doc_model::DataTable { range, row_input, column_input });
            }
            let formula = elements(c).find(|n| n.has_tag_name(("http://schemas.openxmlformats.org/spreadsheetml/2006/main", "f"))).and_then(|f| {
                let own = f.text().map(ibis_calc::formula::from_file).filter(|t| !t.is_empty());
                if unqualified_attr(f, "t") != Some("shared") {
                    return own;
                }
                let si = unqualified_attr(f, "si")?.to_string();
                match own {
                    Some(text) => {
                        shared.insert(si, (text.clone(), row_index, column));
                        Some(text)
                    }
                    None => {
                        let (text, r0, c0) = shared.get(&si)?;
                        Some(shift_references(text, row_index as i64 - *r0 as i64, column as i64 - *c0 as i64))
                    }
                }
            });
            let style = format.map(|f| f.style.clone()).unwrap_or_default();
            let visible = formula.is_some()
                // Typed empty text and values hidden by number formats still
                // establish Excel's print extent (Windows Office controls:
                // A1 plus a typed value at C70 prints all six grid pages).
                // This tests actual cells, not the worksheet's dimension hint.
                || !value.is_empty()
                || !text.is_empty()
                || style.fill.is_some()
                || style.gradient.is_some()
                || style.pattern.is_some()
                || style.left.is_some()
                || style.right.is_some()
                || style.top.is_some()
                || style.bottom.is_some()
                || style.diagonal_up.is_some()
                || style.diagonal_down.is_some()
                // The empty cells a text is centred across (the page reaches them).
                || style.align == ibis_doc_model::CellAlign::CenterAcross;
            // A cell with only a format that does not show (a number
            // format, a font) is kept, for what is typed into it, but does
            // not make the sheet any bigger.
            // Typed empty text is distinct from a genuinely empty cell both
            // for calculation (e.g. TOCOL ignore=1) and for the print extent.
            if !visible && value.is_empty() && (xf == 0 || styles.formats.get(xf).is_none()) {
                continue;
            }
            let index = *style_index.entry(xf).or_insert_with(|| {
                sheet_styles.push(style);
                (sheet_styles.len() - 1) as u32
            });
            if visible {
                max_row = max_row.max(row_index + 1);
                max_column = max_column.max(column + 1);
            }
            let shared = if attr(c, "t") == Some("s") { raw.trim().parse::<usize>().ok() } else { None };
            let phonetic = shared.and_then(|i| extras.phonetics.get(i)).cloned().flatten();
            let runs = match attr(c, "t") {
                Some("s") => shared.and_then(|i| extras.runs.get(i)).cloned().flatten(),
                Some("inlineStr") => child(c, "is").and_then(|n| read_runs(n, colors)),
                _ => None,
            };
            cells.push(SheetCell { runs, phonetic, row: row_index, column, value, text, kind, formula, style: index });
        }
        if hidden {
            heights.insert(r, 0);
            let ht = declared_ht.unwrap_or(default_row_pt);
            hidden_rows.insert(r, pt(ht));
        } else if custom_height {
            // A custom row is a whole number of device pixels in Excel for Windows (0.75 pt, cut down: 5.85 pt = 7.8 px is 7 px = 5.25 pt;
            // 37.5 pt = 50 px stays; a height written to 4 decimals of a centimetre by another format is not cut by that noise): its PDFs show
            // 118's rows 1..32 (stored 1029 pt) at 1029 pt, not the 1008 pt of whole points, and 058's 150 rows of 5.85 pt on 1 page.
            // Elsewhere OOXML ht is a height in points, fractions included, and printing cuts it to whole points (`export::print_layout`).
            // Saving writes only the rows the person changed, as they set them (40 pt is written 40 pt; read again it is 39.75 pt).
            let ht = declared_ht.filter(|h| h.is_finite() && *h >= 0.0).map(|h| if EXCEL_METRICS == ExcelMetrics::Windows { (h / 0.75 + 0.01).floor() * 0.75 } else { h }).unwrap_or(default_row_pt);
            heights.insert(r, pt(ht));
            custom_rows.insert(r);
        } else {
            // Excel for Mac fits again a row holding wrapping text of Meiryo UI
            // or ＭＳ ゴシック when it opens the file (one private workbook's
            // cover sheet: rows stored 15 pt come out 16 pt, 28.5 pt come out
            // 30 pt); the others keep the stored height: that workbook's other
            // sheets (stored 15 pt, printed 15.0 pt), and another private
            // workbook's 游ゴシック 12 pt wrapped rows, whose row boundaries in
            // the PDF are exactly the stored 126, 168, 147 and 84 pt (our
            // estimate made the 84 pt row 104).
            // Excel for Windows keeps a stored height (the refitting below is what Excel
            // for Mac does on opening, to fonts whose metrics differ from its own).
            if !any_wrap {
                measured.insert(r);
            }
            let keeps_stored = (!wraps_text || EXCEL_METRICS == ExcelMetrics::Windows) && declared_ht.is_some();
            heights.insert(r, pt(if keeps_stored { declared_ht.unwrap_or(auto_ht) } else if auto_ht > 0.0 { auto_ht } else { default_row_pt }));
        }
    }

    let merges: Vec<CellRange> = child(root, "mergeCells")
        .into_iter()
        .flat_map(|m| children(m, "mergeCell"))
        .filter_map(|m| {
            let reference = attr(m, "ref")?;
            let (a, b) = reference.split_once(':')?;
            let ((c0, r0), (c1, r1)) = (cell_ref(a)?, cell_ref(b)?);
            Some(CellRange { first_row: r0.min(r1), first_column: c0.min(c1), last_row: r0.max(r1), last_column: c0.max(c1) })
        })
        .collect();
    for m in &merges {
        max_row = max_row.max(m.last_row + 1);
        max_column = max_column.max(m.last_column + 1);
    }
    if !row_styles.is_empty() && max_column > 0 {
        let existing: HashSet<(u32, u32)> = cells.iter().map(|c| (c.row, c.column)).collect();
        for (r, index) in row_styles {
            for col in 0..max_column {
                if !existing.contains(&(r, col)) {
                    cells.push(SheetCell {
                        runs: None,
                        phonetic: None,
                        row: r,
                        column: col,
                        value: CellValue::Empty,
                        text: String::new(),
                        kind: CellKind::Empty,
                        formula: None,
                        style: index,
                    });
                }
            }
        }
        cells.sort_by_key(|c| (c.row, c.column));
    }

    let view = child(root, "sheetViews").and_then(|v| child(v, "sheetView"));
    let grid_lines = view.and_then(|v| attr_bool(v, "showGridLines")).unwrap_or(true);
    let headings = view.and_then(|v| attr_bool(v, "showRowColHeaders")).unwrap_or(true);
    let show_ruler = view.and_then(|v| attr_bool(v, "showRuler")).unwrap_or(true);
    let page_break_preview = view.and_then(|v| attr(v, "view")) == Some("pageBreakPreview");
    let page_layout = view.and_then(|v| attr(v, "view")) == Some("pageLayout");
    let show_formulas = view.and_then(|v| attr_bool(v, "showFormulas")).unwrap_or(false);
    let show_zeros = view.and_then(|v| attr_bool(v, "showZeros")).unwrap_or(true);
    // Page break preview and page layout view have a zoom of their own.
    let zoom_attr = if page_break_preview {
        "zoomScaleSheetLayoutView"
    } else if page_layout {
        "zoomScalePageLayoutView"
    } else {
        "zoomScale"
    };
    let zoom = view
        .and_then(|v| attr(v, zoom_attr).or_else(|| attr(v, "zoomScale")))
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|z| *z > 0 && *z != 100);
    let pane_elem = view.and_then(|v| child(v, "pane"));
    let pane_state = pane_elem.and_then(|p| attr(p, "state")).unwrap_or("split");
    let (frozen, split) = match pane_elem {
        Some(p) if matches!(pane_state, "frozen" | "frozenSplit") => {
            let parse_s = |name: &str| attr(p, name).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0).max(0.0) as u32;
            (Some((parse_s("xSplit"), parse_s("ySplit"))), None)
        }
        Some(p) => {
            let top_left = attr(p, "topLeftCell").and_then(cell_ref);
            let (col, row) = match top_left {
                Some((c, r)) => (c, r),
                None => {
                    let parse_s = |name: &str| attr(p, name).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0).max(0.0) as u32;
                    (parse_s("xSplit"), parse_s("ySplit"))
                }
            };
            (None, (col > 0 || row > 0).then_some((col, row)))
        }
        None => (None, None),
    };
    let print = read_print(root, grid_lines);
    // Text that runs on past its cell over the empty cells beside it is printed
    // as far as it reaches on the page, so the sheet's extent includes those
    // columns without adding a new page across for overflowing text alone.
    {
        let occupied: HashSet<(u32, u32)> = cells.iter().filter(|c| !c.text.is_empty() || c.formula.is_some()).map(|c| (c.row, c.column)).collect();
        let col_emu = |c: u32| widths.get(&c).copied().unwrap_or(px(default_width));
        let width_of = |c: u32| col_emu(c) as f64 / EMU_PER_POINT as f64;
        let (paper_w, _) = print.page_size();
        let [left, right, _, _] = print.margins;
        let page_room_emu = ((paper_w - left - right).max(EMU_PER_INCH / 2) as f64 / (print.scale.max(10) as f64 / 100.0)).floor() as i64;
        let used_emu: i64 = (0..max_column).map(&col_emu).sum();
        let page_limit_emu = ((used_emu.max(1) + page_room_emu - 1) / page_room_emu) * page_room_emu;
        for cell in cells.iter().filter(|c| c.kind == CellKind::Text && !c.text.is_empty()) {
            let Some(style) = sheet_styles.get(cell.style as usize) else { continue };
            if style.wrap || style.rotation != 0 || style.shrink || !matches!(style.align, CellAlign::General | CellAlign::Left) || cell.text.contains('\n') {
                continue;
            }
            if merges.iter().any(|m| m.first_row <= cell.row && cell.row <= m.last_row && m.first_column <= cell.column && cell.column <= m.last_column) {
                continue;
            }
            let em = style.font_size as f64 / 100.0;
            let font = style.font.as_deref().or(styles.default_font.0.as_deref()).unwrap_or(DEFAULT_FONT);
            // The text's width in its font (GDI); where that cannot be measured, half an em a Latin character and a whole
            // one a wide one. Plus the cell's padding on both sides.
            let text_width = crate::font_digit::text_width_pt(font, em, style.bold, &cell.text)
                .unwrap_or_else(|| cell.text.chars().map(|c| if c.is_ascii() { 0.5 * em } else { em }).sum::<f64>());
            let need = text_width + 2.0 * CELL_TEXT_PADDING_PT;
            let mut reach = cell.column;
            let mut room = width_of(cell.column);
            let mut end_emu: i64 = (0..=cell.column).map(&col_emu).sum();
            while room < need && reach < 16_383 && !occupied.contains(&(cell.row, reach + 1)) && end_emu + col_emu(reach + 1) <= page_limit_emu {
                reach += 1;
                room += width_of(reach);
                end_emu += col_emu(reach);
            }
            if room >= need || !occupied.contains(&(cell.row, reach + 1)) {
                max_column = max_column.max(reach + 1);
            }
        }
    }
    let columns = (0..max_column).map(|c| widths.get(&c).copied().unwrap_or(px(default_width))).collect();
    // A row of its own height counts even with nothing shown in it (a picture in a cell leaves its cell's text empty), but
    // only when the row has a cell: a `<row ht=.. customHeight/>` with no cell is not printed (Excel's PDF of a sheet whose
    // last rows, 647 and 648, are only a height has no page for them).
    let mut printed_rows = max_row;
    if let Some(last) = heights.iter().filter(|(r, h)| **h != pt(default_row_pt) && rows_with_cells.contains(r)).map(|(r, _)| *r).max() {
        max_row = max_row.max(last + 1);
        printed_rows = max_row;
    }
    // The rows of a height alone stay in the model (the person's edit of an empty row is kept), but print no page.
    if let Some(last) = heights.iter().filter(|(_, h)| **h != pt(default_row_pt)).map(|(r, _)| *r).max() {
        max_row = max_row.max(last + 1);
    }
    let rows = (0..max_row).map(|r| heights.get(&r).copied().unwrap_or(pt(default_row_pt))).collect();
    // The rows with no `row` element are measured too.
    let measured_rows: Vec<u32> = (0..max_row).filter(|r| measured.contains(r) || !heights.contains_key(r)).collect();
    cells.sort_by_key(|c| (c.row, c.column));
    let show_phonetic = view.and_then(|v| attr_bool(v, "showPhonetic")).unwrap_or(false);
    let phonetic_pr = child(root, "phoneticPr").map(|p| {
        let font_id = attr(p, "fontId").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let phonetic_type = attr(p, "type").and_then(ibis_doc_model::PhoneticType::from_ooxml).unwrap_or_default();
        let alignment = attr(p, "alignment").and_then(ibis_doc_model::PhoneticAlignment::from_ooxml).unwrap_or_default();
        ibis_doc_model::PhoneticProperties { font_id, phonetic_type, alignment }
    });
    let mut sheet = Sheet {
        columns,
        rows,
        cells: cells.into(),
        styles: sheet_styles,
        merges,
        grid_lines,
        headings,
        print_area: None,
        tab_color: None,
        frozen,
        split,
        protection: read_protection(root),
        protected_ranges: read_protected_ranges(root),
        page_break_preview,
        page_layout,
        show_ruler,
        zoom,
        print,
        default_row: pt(default_row_pt),
        default_column: px(default_width),
        measured_rows: ibis_doc_model::Unsaved(measured_rows),
        column_fonts: ibis_doc_model::Unsaved(column_fonts),
        row_fonts: ibis_doc_model::Unsaved(row_fonts),
        written_fonts: ibis_doc_model::Unsaved(
            written_xfs
                .into_iter()
                .map(|(r, xfs)| {
                    let fonts: Vec<(Option<String>, u32)> = xfs
                        .into_iter()
                        .map(|xf| {
                            let format = styles.formats.get(xf).or_else(|| styles.formats.first());
                            (format.and_then(|f| f.style.font.clone()), format.map_or(0, |f| f.style.font_size))
                        })
                        .collect();
                    (r, fonts)
                })
                .collect(),
        ),
        fitted_rows: Default::default(),
        printed_rows: ibis_doc_model::Unsaved(printed_rows),
        hidden_rows,
        custom_rows,
        custom_columns,
        hidden_columns,
        spills,
        conditional: read_conditional(root, styles, colors),
        conditional_x14: read_conditional_x14_only(root, colors),
        validations: read_validations(root),
        hyperlinks: Vec::new(),
        comments: Vec::new(),
        outline_rows,
        outline_columns,
        auto_filter: read_auto_filter(root, &styles.dxfs),
        filtered_rows: Default::default(),
        tables: Vec::new(),
        sparklines: read_sparklines(root, colors),
        threads: Vec::new(),
        pivots: Vec::new(),
        data_tables,
        data_consolidate: read_data_consolidate(root),
        scenarios: read_scenarios(root),
        custom_views: read_custom_sheet_views(root),
        show_phonetic,
        phonetic_pr,
        ignored_errors: read_ignored_errors(root),
        cell_images,
        web_images: Default::default(),
        show_formulas,
        show_zeros,
        hide_ink: false,
        book_context: Default::default(),
    };
    if let Some(f) = &sheet.auto_filter {
        sheet.filtered_rows = ibis_doc_model::sheet_command::filtered_rows(&sheet, f, styles.date1904).into_iter().filter(|&row| sheet.rows.get(row as usize) == Some(&0)).collect();
    }
    sheet
}

/// The sheet's sparkline groups (`extLst/ext/x14:sparklineGroups`), with
/// the defaults of MS-XLSX 2.6.89 for what a group leaves out.
fn read_sparklines(root: Node, colors: &SheetColors) -> Vec<ibis_doc_model::SparklineGroup> {
    use ibis_doc_model::{Sparkline, SparklineAxis, SparklineEmpty, SparklineGroup, SparklineKind};
    let mut out = Vec::new();
    let groups = child(root, "extLst").into_iter().flat_map(|e| children(e, "ext")).filter_map(|e| child(e, "sparklineGroups"));
    for groups in groups {
        for g in children(groups, "sparklineGroup") {
            let flag = |name: &str| matches!(attr(g, name), Some("1" | "true"));
            let axis = |kind: &str, value: &str| match attr(g, kind) {
                Some("group") => SparklineAxis::Group,
                Some("custom") => SparklineAxis::Custom(attr(g, value).unwrap_or("0").to_string()),
                _ => SparklineAxis::Individual,
            };
            let sparklines = child(g, "sparklines")
                .into_iter()
                .flat_map(|s| children(s, "sparkline"))
                .filter_map(|s| {
                    let f = child(s, "f")?.text()?.trim();
                    let at = child(s, "sqref")?.text()?.trim();
                    let (column, row) = cell_ref(at)?;
                    let (sheet, area) = match f.rsplit_once('!') {
                        Some((sh, a)) => (Some(sh.trim_matches('\'').replace("''", "'")), a),
                        None => (None, f),
                    };
                    let data = parse_range(&area.replace('$', ""))?;
                    Some(Sparkline { row, column, sheet, data })
                })
                .collect();
            out.push(SparklineGroup {
                kind: match attr(g, "type") {
                    Some("column") => SparklineKind::Column,
                    Some("stacked") => SparklineKind::WinLoss,
                    _ => SparklineKind::Line,
                },
                series: child(g, "colorSeries").and_then(|c| colors.color(c)),
                negative: child(g, "colorNegative").and_then(|c| colors.color(c)),
                axis: child(g, "colorAxis").and_then(|c| colors.color(c)),
                markers_color: child(g, "colorMarkers").and_then(|c| colors.color(c)),
                first_color: child(g, "colorFirst").and_then(|c| colors.color(c)),
                last_color: child(g, "colorLast").and_then(|c| colors.color(c)),
                high_color: child(g, "colorHigh").and_then(|c| colors.color(c)),
                low_color: child(g, "colorLow").and_then(|c| colors.color(c)),
                line_weight: attr(g, "lineWeight").and_then(|v| v.parse::<f64>().ok()).map_or(75, |w| (w * 100.0).round().max(0.0) as u32),
                markers: flag("markers"),
                high: flag("high"),
                low: flag("low"),
                first: flag("first"),
                last: flag("last"),
                negative_points: flag("negative"),
                display_x_axis: flag("displayXAxis"),
                empty: match attr(g, "displayEmptyCellsAs") {
                    Some("zero") => SparklineEmpty::Zero,
                    Some("span") => SparklineEmpty::Span,
                    _ => SparklineEmpty::Gap,
                },
                min_axis: axis("minAxisType", "manualMin"),
                max_axis: axis("maxAxisType", "manualMax"),
                right_to_left: flag("rightToLeft"),
                display_hidden: flag("displayHidden"),
                date_axis: flag("dateAxis"),
                date_range: child(g, "f").and_then(|f| f.text()).map(|t| t.trim().to_string()),
                sparklines,
            });
        }
    }
    out
}

/// Apply table styles (tableBorderDxfId, headerRowBorderDxfId, dataDxfId, etc.) from `table*.xml` parts to the sheet.
pub(crate) fn apply_tables(sheet: &mut Sheet, table_xmls: &[&str], dxfs: &[ibis_doc_model::CfStyle]) {
    for xml in table_xmls {
        let Ok(doc) = roxmltree::Document::parse(xml) else { continue };
        let root = doc.root_element();
        let Some(range) = attr(root, "ref").and_then(parse_range) else { continue };

        let header_row_count = attr(root, "headerRowCount")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or_else(|| if attr(root, "headerRowShown") == Some("0") { 0 } else { 1 });
        let totals_row_count = attr(root, "totalsRowCount")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or_else(|| if attr(root, "totalsRowShown") == Some("1") { 1 } else { 0 });

        // Its name and columns, for structured references (`Table1[Column]`).
        sheet.tables.push(ibis_doc_model::SheetTable {
            name: attr(root, "displayName").or_else(|| attr(root, "name")).unwrap_or_default().to_string(),
            range,
            header_rows: header_row_count,
            totals_rows: totals_row_count,
            columns: child(root, "tableColumns")
                .into_iter()
                .flat_map(|tc| children(tc, "tableColumn"))
                .map(|col| attr(col, "name").unwrap_or_default().to_string())
                .collect(),
            totals: {
                let totals: Vec<ibis_doc_model::TableTotal> = child(root, "tableColumns")
                    .into_iter()
                    .flat_map(|tc| children(tc, "tableColumn"))
                    .map(|col| ibis_doc_model::TableTotal { function: attr(col, "totalsRowFunction").filter(|f| *f != "none").map(str::to_string), label: attr(col, "totalsRowLabel").map(str::to_string) })
                    .collect();
                if totals.iter().all(|t| *t == Default::default()) { Vec::new() } else { totals }
            },
            filter: child(root, "autoFilter").map(|f| {
                let r = attr(f, "ref").and_then(parse_range).unwrap_or(CellRange { last_row: range.last_row.saturating_sub(totals_row_count), ..range });
                ibis_doc_model::AutoFilter {
                    range: r,
                    columns: children(f, "filterColumn").filter_map(|fc| Some((r.first_column + attr_i64(fc, "colId")?.max(0) as u32, column_filter(fc, dxfs)?))).collect(),
                }
            }),
            style: child(root, "tableStyleInfo").map(|i| {
                let flag = |n: &str| matches!(attr(i, n), Some("1" | "true"));
                ibis_doc_model::TableStyleInfo {
                    name: attr(i, "name").unwrap_or_default().to_string(),
                    first_column: flag("showFirstColumn"),
                    last_column: flag("showLastColumn"),
                    row_stripes: flag("showRowStripes"),
                    column_stripes: flag("showColumnStripes"),
                }
            }),
        });

        let table_border = attr_i64(root, "tableBorderDxfId").and_then(|v| dxfs.get(v as usize));
        let header_dxf = attr_i64(root, "headerRowDxfId").and_then(|v| dxfs.get(v as usize));
        let header_border = attr_i64(root, "headerRowBorderDxfId").and_then(|v| dxfs.get(v as usize));
        let totals_dxf = attr_i64(root, "totalsRowDxfId").and_then(|v| dxfs.get(v as usize));
        let totals_border = attr_i64(root, "totalsRowBorderDxfId").and_then(|v| dxfs.get(v as usize));

        let columns: Vec<(Option<&ibis_doc_model::CfStyle>, Option<&ibis_doc_model::CfStyle>, Option<&ibis_doc_model::CfStyle>)> = child(root, "tableColumns")
            .into_iter()
            .flat_map(|tc| children(tc, "tableColumn"))
            .map(|col| {
                let data = attr_i64(col, "dataDxfId").and_then(|v| dxfs.get(v as usize));
                let header = attr_i64(col, "headerRowDxfId").and_then(|v| dxfs.get(v as usize));
                let totals = attr_i64(col, "totalsRowDxfId").and_then(|v| dxfs.get(v as usize));
                (data, header, totals)
            })
            .collect();

        let mut cell_indices: HashMap<(u32, u32), usize> = HashMap::new();
        for (i, cell) in sheet.cells.iter().enumerate() {
            cell_indices.insert((cell.row, cell.column), i);
        }

        let mut style_cache: HashMap<CellStyle, u32> = HashMap::new();
        for (i, s) in sheet.styles.iter().enumerate() {
            style_cache.entry(s.clone()).or_insert(i as u32);
        }

        let mut added_cells = Vec::new();

        for r in range.first_row..=range.last_row {
            let is_header = r < range.first_row + header_row_count;
            let is_totals = r + totals_row_count > range.last_row;
            for c in range.first_column..=range.last_column {
                let col_idx = (c - range.first_column) as usize;
                let col_dxfs = columns.get(col_idx).copied();

                let (fill, cell_color, cell_bold, mut left, mut right, mut top, mut bottom) = if is_header {
                    let h_dxf = col_dxfs.and_then(|c| c.1).or(header_dxf);
                    let h_bdr = header_border;
                    (
                        h_dxf.and_then(|d| d.fill),
                        h_dxf.and_then(|d| d.color),
                        h_dxf.and_then(|d| d.bold),
                        h_dxf.and_then(|d| d.borders[0]),
                        h_dxf.and_then(|d| d.borders[1]),
                        h_dxf.and_then(|d| d.borders[2]),
                        h_bdr.and_then(|d| d.borders[3]).or_else(|| h_dxf.and_then(|d| d.borders[3])),
                    )
                } else if is_totals {
                    let t_dxf = col_dxfs.and_then(|c| c.2).or(totals_dxf);
                    let t_bdr = totals_border;
                    (
                        t_dxf.and_then(|d| d.fill),
                        t_dxf.and_then(|d| d.color),
                        t_dxf.and_then(|d| d.bold),
                        t_dxf.and_then(|d| d.borders[0]),
                        t_dxf.and_then(|d| d.borders[1]),
                        t_bdr.and_then(|d| d.borders[2]).or_else(|| t_dxf.and_then(|d| d.borders[2])),
                        t_dxf.and_then(|d| d.borders[3]),
                    )
                } else {
                    let d_dxf = col_dxfs.and_then(|c| c.0);
                    (
                        d_dxf.and_then(|d| d.fill),
                        d_dxf.and_then(|d| d.color),
                        d_dxf.and_then(|d| d.bold),
                        d_dxf.and_then(|d| d.borders[0]),
                        d_dxf.and_then(|d| d.borders[1]),
                        d_dxf.and_then(|d| d.borders[2]),
                        d_dxf.and_then(|d| d.borders[3]),
                    )
                };

                if let Some(tb) = table_border {
                    if r == range.first_row && top.is_none() {
                        top = tb.borders[2];
                    }
                    if r == range.last_row && bottom.is_none() {
                        bottom = tb.borders[3];
                    }
                    if c == range.first_column && left.is_none() {
                        left = tb.borders[0];
                    }
                    if c == range.last_column && right.is_none() {
                        right = tb.borders[1];
                    }
                }

                if let Some(&cell_idx) = cell_indices.get(&(r, c)) {
                    let orig_style = &sheet.styles[sheet.cells[cell_idx].style as usize];
                    let mut s = orig_style.clone();
                    let mut modified = false;

                    if s.fill.is_none() && let Some(f) = fill {
                        s.fill = Some(f);
                        modified = true;
                    }
                    if is_header && let Some(true) = cell_bold {
                        s.bold = true;
                        modified = true;
                    }
                    if s.color == Rgb::BLACK && let Some(color) = cell_color {
                        s.color = color;
                        modified = true;
                    }
                    if s.top.is_none() && let Some(b) = top {
                        s.top = Some(b);
                        modified = true;
                    }
                    if s.bottom.is_none() && let Some(b) = bottom {
                        s.bottom = Some(b);
                        modified = true;
                    }
                    if s.left.is_none() && let Some(b) = left {
                        s.left = Some(b);
                        modified = true;
                    }
                    if s.right.is_none() && let Some(b) = right {
                        s.right = Some(b);
                        modified = true;
                    }

                    if modified {
                        let new_style_idx = *style_cache.entry(s.clone()).or_insert_with(|| {
                            sheet.styles.push(s);
                            (sheet.styles.len() - 1) as u32
                        });
                        sheet.cells[cell_idx].style = new_style_idx;
                    }
                } else if fill.is_some() || top.is_some() || bottom.is_some() || left.is_some() || right.is_some() {
                    let s = CellStyle {
                        fill,
                        top,
                        bottom,
                        left,
                        right,
                        bold: is_header && cell_bold.unwrap_or(false),
                        color: cell_color.unwrap_or(Rgb::BLACK),
                        ..CellStyle::default()
                    };
                    let style_idx = *style_cache.entry(s.clone()).or_insert_with(|| {
                        sheet.styles.push(s);
                        (sheet.styles.len() - 1) as u32
                    });
                    added_cells.push(SheetCell { runs: None, phonetic: None,
                        row: r,
                        column: c,
                        value: CellValue::Empty,
                        text: String::new(),
                        kind: CellKind::Empty,
                        formula: None,
                        style: style_idx,
                    });
                }
            }
        }

        if !added_cells.is_empty() {
            sheet.cells.extend(added_cells);
            sheet.cells.sort_by_key(|c| (c.row, c.column));
        }
    }
}

/// Read cell comments from a worksheet's comments part (`xl/comments{N}.xml`).
pub(crate) fn read_comments(root: Node) -> Vec<ibis_doc_model::SheetComment> {
    let authors: Vec<String> = child(root, "authors")
        .into_iter()
        .flat_map(|a| children(a, "author"))
        .map(|n| n.text().unwrap_or("").to_string())
        .collect();

    let mut comments = Vec::new();
    if let Some(list) = child(root, "commentList") {
        for comment in children(list, "comment") {
            let Some(r) = attr(comment, "ref") else { continue };
            let Some((col, row)) = cell_ref(r) else { continue };
            let author_id = attr_i64(comment, "authorId").unwrap_or(0).max(0) as usize;
            let author = authors.get(author_id).cloned().unwrap_or_default();
            let text = child(comment, "text").map(inline_text).unwrap_or_default();
            comments.push(ibis_doc_model::SheetComment {
                row,
                column: col,
                author,
                text,
            });
        }
    }
    comments
}

/// The display names of the book's people (`personList`), by id.
pub(crate) fn read_persons(root: Node) -> HashMap<String, String> {
    children(root, "person").filter_map(|p| Some((attr(p, "id")?.to_string(), attr(p, "displayName").unwrap_or("").to_string()))).collect()
}

/// A sheet's threaded comments (`ThreadedComments`): each first post with
/// its replies (`parentId`), in the order written.
pub(crate) fn read_threads(root: Node, persons: &HashMap<String, String>) -> Vec<ibis_doc_model::ThreadedComment> {
    use ibis_doc_model::{ThreadPost, ThreadedComment};
    let mut threads: Vec<(String, ThreadedComment)> = Vec::new();
    for c in children(root, "threadedComment") {
        let post = ThreadPost {
            author: attr(c, "personId").and_then(|id| persons.get(id)).cloned().unwrap_or_default(),
            date: attr(c, "dT").unwrap_or("").to_string(),
            text: child(c, "text").and_then(|t| t.text()).unwrap_or("").to_string(),
        };
        match attr(c, "parentId") {
            Some(parent) => {
                if let Some((_, t)) = threads.iter_mut().find(|(id, _)| id == parent) {
                    t.posts.push(post);
                }
            }
            None => {
                let Some((column, row)) = attr(c, "ref").and_then(cell_ref) else { continue };
                let done = matches!(attr(c, "done"), Some("1" | "true"));
                let id = attr(c, "id").unwrap_or("").to_string();
                threads.push((id.clone(), ThreadedComment { id, row, column, posts: vec![post], done }));
            }
        }
    }
    threads.into_iter().map(|(_, t)| t).collect()
}

/// A range such as `'Sheet 1'!$A$1:$BG$384` (the first of a list); whole
/// columns (`$A:$CG`) or rows (`$1:$40`) reach to [`OPEN_END`].
pub(crate) fn parse_range(text: &str) -> Option<CellRange> {
    let first = text.split(',').next()?;
    let range = first.rsplit_once('!').map_or(first, |(_, r)| r).replace('$', "");
    let (a, b) = range.split_once(':').unwrap_or((&range, &range));
    // A corner: a cell, a column (letters) or a row (digits).
    let corner = |r: &str, end: bool| -> Option<(u32, u32)> {
        if let Some(cell) = cell_ref(r) {
            return Some(cell);
        }
        let open = if end { OPEN_END } else { 0 };
        if r.chars().all(|c| c.is_ascii_digit()) {
            return Some((open, r.parse::<u32>().ok()?.checked_sub(1)?));
        }
        let (column, _) = cell_ref(&format!("{r}1"))?;
        Some((column, open))
    };
    let ((c0, r0), (c1, r1)) = (corner(a, false)?, corner(b, true)?);
    Some(CellRange { first_row: r0.min(r1), first_column: c0.min(c1), last_row: r0.max(r1), last_column: c0.max(c1) })
}

/// The far end of a range of whole rows or columns.
pub(crate) const OPEN_END: u32 = u32::MAX - 1;

/// Grow `sheet` to at least `rows` rows and `columns` columns, the new ones
/// at the sheet's default sizes (for drawings anchored past the last cell).
pub(crate) fn extend_sheet(sheet: &mut Sheet, root: Node, columns: u32, rows: u32, digit: f64, default_font: (&Option<String>, u32)) {
    let format = child(root, "sheetFormatPr");
    let default_row = default_row_height(format, default_font);
    let default_width = default_column_width(format, digit);
    let widths: HashMap<u32, Emu> = child(root, "cols")
        .into_iter()
        .flat_map(|c| children(c, "col"))
        .filter_map(|col| {
            let (min, max) = (attr_i64(col, "min")?, attr_i64(col, "max")?);
            let hidden = attr_bool(col, "hidden").unwrap_or(false);
            let width = attr(col, "width").and_then(|v| v.parse::<f64>().ok()).map(|w| column_pixels(w, digit)).unwrap_or(default_width);
            let w = if hidden { 0 } else { (width * EMU_PER_POINT as f64).round() as Emu };
            Some(((min.max(1) as u32 - 1)..(max.clamp(1, 16_384) as u32)).map(move |c| (c, w)))
        })
        .flatten()
        .collect();
    while (sheet.columns.len() as u32) < columns {
        let c = sheet.columns.len() as u32;
        sheet.columns.push(widths.get(&c).copied().unwrap_or((default_width * EMU_PER_POINT as f64).round() as Emu));
    }
    while (sheet.rows.len() as u32) < rows {
        sheet.rows.push((default_row * EMU_PER_POINT as f64).round() as Emu);
    }
}

/// Move the relative cell references in `formula` by `rows` and `columns`
/// (a shared formula copied from its first cell). `$` fixes a part; text in
/// quotes is left alone.
pub(crate) fn shift_references(formula: &str, rows: i64, columns: i64) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::with_capacity(formula.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' || c == '\'' {
            // Strings and quoted sheet names.
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                i += 1;
                if chars[i - 1] == c {
                    break;
                }
            }
            continue;
        }
        let starts_word = i == 0 || !(chars[i - 1].is_alphanumeric() || matches!(chars[i - 1], '_' | '.'));
        if starts_word && let Some((end, text)) = reference_at(&chars, i, rows, columns) {
            out.push_str(&text);
            i = end;
            continue;
        }
        // Skip the rest of a name (a function or a defined name).
        if c.is_alphanumeric() || c == '_' {
            while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '.')) {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// A cell reference (`A1`, `$B$2`) starting at `start`, moved: its end and
/// its new text.
fn reference_at(chars: &[char], start: usize, rows: i64, columns: i64) -> Option<(usize, String)> {
    let mut i = start;
    let column_fixed = chars.get(i) == Some(&'$');
    if column_fixed {
        i += 1;
    }
    let letters_start = i;
    while i < chars.len() && chars[i].is_ascii_uppercase() && i - letters_start < 3 {
        i += 1;
    }
    let letters: String = chars[letters_start..i].iter().collect();
    if letters.is_empty() {
        return None;
    }
    let row_fixed = chars.get(i) == Some(&'$');
    if row_fixed {
        i += 1;
    }
    let digits_start = i;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return None;
    }
    // Not part of a longer name or a function call.
    if chars.get(i).is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '(' | '.')) {
        return None;
    }
    let digits: String = chars[digits_start..i].iter().collect();
    let (column, row) = cell_ref(&format!("{letters}{digits}"))?;
    let column = if column_fixed { column as i64 } else { column as i64 + columns };
    let row = if row_fixed { row as i64 } else { row as i64 + rows };
    if column < 0 || row < 0 {
        return Some((i, "#REF!".into()));
    }
    let name = ibis_doc_model::cell_name(column as u32, row as u32);
    let split = name.find(|c: char| c.is_ascii_digit()).unwrap_or(name.len());
    let (l, d) = name.split_at(split);
    Some((i, format!("{}{l}{}{d}", if column_fixed { "$" } else { "" }, if row_fixed { "$" } else { "" })))
}

/// How a worksheet is printed. Without a `pageSetup`, it goes on A4 (the
/// Japanese locale's paper); with one that names no paper, on the region's paper in Excel for
/// Windows (A4 in Japan), on Letter (the format's default) elsewhere.
fn read_print(root: Node, show_grid_lines: bool) -> SheetPrint {
    let mut print = SheetPrint::default();
    let number = |n: Node, name: &str| attr(n, name).and_then(|v| v.parse::<f64>().ok());
    let inches = |v: f64| (v * EMU_PER_INCH as f64).round() as Emu;
    if let Some(setup) = child(root, "pageSetup").or_else(|| child(root, "pageSetUp")) {
        // The paper a `pageSetup` names is Letter (1) when it names none (ECMA-376). Excel for Windows prints a sheet laid out
        // for Letter on the A4 paper of a printer set for A4 (the region's default paper, `locale_paper`: 15 of the corpus'
        // documents, every one of them with an A4 PDF; and Excel's PDFs of `paperSize` 1 and of none are the same),
        // (its text, rows and columns come out 5 points of the scale smaller there: pagesetup-probe, not worked out yet).
        // Paper sizes this table does not know print on the machine's
        // default paper, and on A4 in the Mac's.
        let named = attr_i64(setup, "paperSize");
        let region = if EXCEL_METRICS == ExcelMetrics::Windows { crate::locale_paper::default_paper_size() } else { 1 };
        let letter_on_a4 = EXCEL_METRICS == ExcelMetrics::Windows && region == 9 && named.unwrap_or(1) == 1;
        let missing = if letter_on_a4 { 9 } else { region };
        let known = |n: i64| -> Option<(Emu, Emu)> {
            Some(match n {
                1 | 2 => (inches(8.5), inches(11.0)),
                5 => (inches(8.5), inches(14.0)),
                8 => (297 * 36_000, 420 * 36_000),
                11 => (148 * 36_000, 210 * 36_000),
                12 => (257 * 36_000, 364 * 36_000),
                13 => (182 * 36_000, 257 * 36_000),
                43 => SheetPrint::HAGAKI,
                44 => SheetPrint::OFUKU_HAGAKI,
                3 => (inches(11.0), inches(17.0)),
                6 => (inches(5.5), inches(8.5)),
                7 => (inches(7.25), inches(10.5)),
                14 => (inches(8.5), inches(13.0)),
                9 | 10 => SheetPrint::A4,
                _ => return None,
            })
        };
        // A number the table does not know (257, a custom size) prints on the machine's default paper in Excel for
        // Windows (A4 in the Japanese PC's PDF of 151; Excel for the web's was Letter), on A4 in the Mac's.
        print.paper = known(if letter_on_a4 { 9 } else { named.unwrap_or(missing) }).unwrap_or_else(|| if EXCEL_METRICS == ExcelMetrics::Windows { known(missing).unwrap_or((inches(8.5), inches(11.0))) } else { SheetPrint::A4 });
        print.landscape = attr(setup, "orientation") == Some("landscape");
        print.scale = attr_i64(setup, "scale").filter(|s| (10..=400).contains(s)).unwrap_or(100) as u32;
        print.over_then_down = attr(setup, "pageOrder") == Some("overThenDown");
        print.black_and_white = attr_bool(setup, "blackAndWhite").unwrap_or(false);
        print.draft = attr_bool(setup, "draft").unwrap_or(false);
        print.cell_errors = attr(setup, "errors").map(str::to_string);
        print.first_page_number = attr_i64(setup, "firstPageNumber").map(|n| n.max(0) as u32);
        print.use_first_page_number = attr_bool(setup, "useFirstPageNumber").unwrap_or(false);
        let fit = child(root, "sheetPr").and_then(|p| child(p, "pageSetUpPr")).and_then(|p| attr_bool(p, "fitToPage")).unwrap_or(false);
        if fit {
            let pages = |name: &str| attr_i64(setup, name).unwrap_or(1).max(0) as u32;
            print.fit_to = Some((pages("fitToWidth"), pages("fitToHeight")));
        } else {
            // A sheet that names Letter on an A4 printer is laid out in pages of Letter's size and drawn smaller on the A4 paper
            // (the page's `letter_on_a4`: see `print_sheet`); not when it is fitted to pages (Excel's PDFs: paper1fit prints
            // as paper9 does).
            print.letter_on_a4 = letter_on_a4;
        }
    }
    if let Some(m) = child(root, "pageMargins") {
        for (i, name) in ["left", "right", "top", "bottom"].into_iter().enumerate() {
            if let Some(v) = number(m, name) {
                print.margins[i] = inches(v.max(0.0));
            }
        }
        if let Some(v) = number(m, "header") {
            print.header_margin = inches(v.max(0.0));
        }
        if let Some(v) = number(m, "footer") {
            print.footer_margin = inches(v.max(0.0));
        }
    }
    if let Some(o) = child(root, "printOptions") {
        print.centered = (attr_bool(o, "horizontalCentered").unwrap_or(false), attr_bool(o, "verticalCentered").unwrap_or(false));
        print.grid_lines = attr_bool(o, "gridLines").unwrap_or_else(|| attr_bool(o, "gridLinesSet") == Some(false) && show_grid_lines);
        print.headings = attr_bool(o, "headings").unwrap_or(false);
    }
    if let Some(hf) = child(root, "headerFooter") {
        print.header_footer.different_first = attr_bool(hf, "differentFirst").unwrap_or(false);
        print.header_footer.different_odd_even = attr_bool(hf, "differentOddEven").unwrap_or(false);
        print.header_footer.scale_with_doc = attr_bool(hf, "scaleWithDoc").unwrap_or(true);
        print.header_footer.align_with_margins = attr_bool(hf, "alignWithMargins").unwrap_or(true);
        let text_child = |name: &str| child(hf, name).and_then(|n| n.text()).map(decode_xstring);
        print.header_footer.odd_header = text_child("oddHeader");
        print.header_footer.odd_footer = text_child("oddFooter");
        print.header_footer.even_header = text_child("evenHeader");
        print.header_footer.even_footer = text_child("evenFooter");
        print.header_footer.first_header = text_child("firstHeader");
        print.header_footer.first_footer = text_child("firstFooter");
    }
    // A break's `id` is the row (column) it follows, counted from 1: the
    // 0-based index of the one it comes before.
    let breaks = |name: &str| -> Vec<u32> {
        let mut out: Vec<u32> = child(root, name).into_iter().flat_map(|b| children(b, "brk")).filter_map(|b| attr_i64(b, "id")).filter(|id| *id > 0).map(|id| id as u32).collect();
        out.sort_unstable();
        out.dedup();
        out
    };
    print.row_breaks = breaks("rowBreaks");
    print.column_breaks = breaks("colBreaks");
    print
}

/// What a chunk of a text file's rows (CSV, TSV) made into cells.
struct CsvChunk {
    cells: Vec<SheetCell>,
    /// The number formats the cells use, in order of first use (a cell's `style` is an index into it until the chunks are put together); `None` is General.
    codes: Vec<Option<&'static str>>,
    /// The heights of the chunk's rows, in EMU.
    heights: Vec<Emu>,
    /// The sheet's size by the cells that show something, as `read_sheet` counts it.
    max_row: u32,
    max_column: u32,
}

/// A number as the General format shows it, for the numbers whose text is just the shortest digits that give the number back (not rounded to the width, not in exponent form): `None` for the others.
fn general_number_text(n: f64) -> Option<String> {
    let a = n.abs();
    if n == 0.0 {
        return Some("0".into());
    }
    if !(1e-3..1e9).contains(&a) {
        return None;
    }
    let text = n.to_string();
    (text.len() <= 10).then_some(text)
}

/// The cells of a text file's `records` (the first row at `first_row`), made as `read_sheet` makes them from the workbook `csv::workbook_from` writes: the values typed in as Excel reads them, the number format they give, rows as high as their text needs.
fn csv_chunk(first_row: usize, records: &[Vec<String>], base: &CellStyle, default_font: &Option<String>, default_row_pt: f64, col_chars: f64) -> CsvChunk {
    let pt = |v: f64| (v * EMU_PER_POINT as f64).round() as Emu;
    let mut chunk = CsvChunk { cells: Vec::new(), codes: Vec::new(), heights: Vec::with_capacity(records.len()), max_row: 0, max_column: 0 };
    let font_pt = (base.font_size as f64 / 100.0).max(1.0);
    let font = base.font.as_deref().or(default_font.as_deref());
    // The height of a line of text (the same for every format: the font is the default one).
    let line_height = cell_needed_height("x", col_chars, font, font_pt, false, base.bold);
    for (offset, record) in records.iter().enumerate() {
        let row = (first_row + offset) as u32;
        // As `read_sheet` fits a row for Excel for Windows: to what is in it, in steps of 0.75 pt.
        let fits = EXCEL_METRICS == ExcelMetrics::Windows;
        let mut row_height = if fits { 0.0 } else { default_row_pt };
        for (column, field) in record.iter().enumerate() {
            if field.is_empty() {
                continue;
            }
            let column = column as u32;
            let (value, formula, code) = match ibis_calc::input::parse_input(field, false) {
                ibis_calc::input::Typed::Formula(f) => (CellValue::Empty, Some(ibis_calc::formula::from_file(&ibis_calc::formula::to_file(&f))).filter(|t| !t.is_empty()), None),
                ibis_calc::input::Typed::Value(v, code) => {
                    // As the workbook file holds it, and as reading that gives it back.
                    let v = match v {
                        // A short number typed as it is (not worked out, as a percentage or a date is) comes back as it was written.
                        CellValue::Number(n) if code.is_none() && field.len() <= 15 => CellValue::Number(n),
                        CellValue::Number(n) => {
                            let written = ibis_calc::eval::number_to_text(n);
                            written.trim().parse::<f64>().map(CellValue::Number).unwrap_or(CellValue::Text(written))
                        }
                        CellValue::Text(t) if t.contains('\r') => CellValue::Text(t.replace("\r\n", "\n").replace('\r', "\n")),
                        other => other,
                    };
                    (v, None, code)
                }
            };
            let text = match (&value, code) {
                (CellValue::Number(n), None) => general_number_text(*n).unwrap_or_else(|| display_with_width(&value, "General", false, col_chars)),
                _ => display_with_width(&value, code.unwrap_or("General"), false, col_chars),
            };
            if !text.is_empty() {
                let needed = if text.contains('\n') { cell_needed_height(&text, col_chars, font, font_pt, false, base.bold) } else { line_height };
                row_height = row_height.max(if fits { (needed / 0.75 - 1e-6).ceil() * 0.75 } else { needed });
            }
            let visible = formula.is_some() || !text.is_empty();
            if !visible && value.is_empty() {
                continue;
            }
            if visible {
                chunk.max_row = chunk.max_row.max(row + 1);
                chunk.max_column = chunk.max_column.max(column + 1);
            }
            let style = match chunk.codes.iter().position(|c| *c == code) {
                Some(i) => i,
                None => {
                    chunk.codes.push(code);
                    chunk.codes.len() - 1
                }
            };
            let kind = CellKind::of(&value);
            chunk.cells.push(SheetCell { runs: None, phonetic: None, row, column, value, text, kind, formula, style: style as u32 });
        }
        chunk.heights.push(pt(if row_height > 0.0 { row_height } else { default_row_pt }));
    }
    chunk
}

/// Fills `sheet`, the empty sheet of the workbook `csv::workbook_from("")` writes, with the cells of a text file's `records`: what `read_sheet` makes of the workbook that function writes from the same records, without writing and reading its XML (most of the time of opening a large file).
pub(crate) fn fill_csv_sheet(sheet: &mut Sheet, root: Node, styles: &Styles, digit: f64, records: &[Vec<String>]) {
    const CHUNK_ROWS: usize = 10_000;
    let format = child(root, "sheetFormatPr");
    let default_row_pt = default_row_height(format, (&styles.default_font.0, styles.default_font.1));
    let default_width = default_column_width(format, digit);
    let col_chars = format.and_then(|f| attr(f, "defaultColWidth")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(8.43);
    let pt = |v: f64| (v * EMU_PER_POINT as f64).round() as Emu;
    let mut base = styles.formats.first().map(|f| f.style.clone()).unwrap_or_default();
    // A text file's cells sit at the top of their rows (issue #17).
    base.vertical = ibis_doc_model::CellVerticalAlign::Top;
    base.vertical_set = true;
    let default_font = styles.default_font.0.clone();

    let items: Vec<(usize, &[Vec<String>])> = records.chunks(CHUNK_ROWS).enumerate().map(|(i, c)| (i * CHUNK_ROWS, c)).collect();
    let done = std::sync::atomic::AtomicUsize::new(0);
    let chunks = crate::reader::par_map(&items, |(first, chunk)| {
        let made = csv_chunk(*first, chunk, &base, &default_font, default_row_pt, col_chars);
        // The progress bar: most of the time of a large file is here.
        let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        crate::progress::fraction(0.15 + 0.8 * n as f64 / items.len() as f64);
        made
    });

    // Put together: styles by first use (as `read_sheet` makes them), rows, columns.
    let mut sheet_styles: Vec<CellStyle> = Vec::new();
    let mut style_of: Vec<(Option<&'static str>, u32)> = Vec::new();
    let mut cells: Vec<SheetCell> = Vec::with_capacity(chunks.iter().map(|c| c.cells.len()).sum());
    let mut heights: Vec<Emu> = Vec::with_capacity(records.len());
    let (mut max_row, mut max_column) = (0u32, 0u32);
    for chunk in chunks {
        let map: Vec<u32> = chunk
            .codes
            .iter()
            .map(|code| match style_of.iter().find(|(c, _)| c == code) {
                Some((_, i)) => *i,
                None => {
                    let mut style = base.clone();
                    if let Some(code) = code {
                        style.number_format = (*code).to_string();
                        style.xf = None;
                    }
                    sheet_styles.push(style);
                    let i = (sheet_styles.len() - 1) as u32;
                    style_of.push((*code, i));
                    i
                }
            })
            .collect();
        cells.extend(chunk.cells.into_iter().map(|mut c| {
            c.style = map[c.style as usize];
            c
        }));
        heights.extend(chunk.heights);
        max_row = max_row.max(chunk.max_row);
        max_column = max_column.max(chunk.max_column);
    }
    sheet.cells = cells.into();
    sheet.styles = sheet_styles;
    sheet.columns = (0..max_column).map(|_| pt(default_width)).collect();
    sheet.rows = (0..max_row).map(|r| heights.get(r as usize).copied().unwrap_or(pt(default_row_pt))).collect();
}

/// Builds the cells and row heights for a slice of CSV records starting at `first_row`,
/// adding new number formats to `sheet_styles` as needed.
pub fn process_csv_chunk(
    first_row: usize,
    records: &[Vec<String>],
    base_style: &CellStyle,
    sheet_styles: &mut Vec<CellStyle>,
) -> (Vec<SheetCell>, Vec<Emu>) {
    const CHUNK_ROWS: usize = 10_000;
    let default_row_pt = 13.5;
    let col_chars = 8.43;
    let default_font = None;

    let mut style_of: Vec<(Option<&'static str>, u32)> = Vec::new();
    for (idx, s) in sheet_styles.iter().enumerate() {
        if s.number_format.is_empty() || s.number_format == "General" {
            style_of.push((None, idx as u32));
        }
    }

    let items: Vec<(usize, &[Vec<String>])> = records.chunks(CHUNK_ROWS).enumerate().map(|(i, c)| (first_row + i * CHUNK_ROWS, c)).collect();
    let chunks = crate::reader::par_map(&items, |(first, chunk)| {
        csv_chunk(*first, chunk, base_style, &default_font, default_row_pt, col_chars)
    });

    let mut cells = Vec::with_capacity(chunks.iter().map(|c| c.cells.len()).sum());
    let mut heights = Vec::with_capacity(records.len());
    for chunk in chunks {
        let map: Vec<u32> = chunk
            .codes
            .iter()
            .map(|code| match style_of.iter().find(|(c, _)| c == code) {
                Some((_, i)) => *i,
                None => {
                    let mut style = base_style.clone();
                    if let Some(code) = code {
                        style.number_format = (*code).to_string();
                        style.xf = None;
                    }
                    sheet_styles.push(style);
                    let i = (sheet_styles.len() - 1) as u32;
                    style_of.push((*code, i));
                    i
                }
            })
            .collect();
        cells.extend(chunk.cells.into_iter().map(|mut c| {
            c.style = map[c.style as usize];
            c
        }));
        heights.extend(chunk.heights);
    }
    (cells, heights)
}

#[cfg(test)]
mod csv_fast_path_tests {
    use super::*;

    /// The quick text of a number is what the General format gives it.
    #[test]
    fn the_quick_text_of_a_number_is_the_general_formats() {
        let mut state = 0x2545F4914F6CDD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut checked = 0;
        for i in 0..400_000u64 {
            let r = next();
            // Integers, decimals of a few digits, long decimals, magnitudes from 1e-5 to 1e12, both signs.
            let n = match i % 6 {
                0 => (r % 2_000_000_000) as f64 - 1_000_000_000.0,
                1 => (r % 100_000) as f64 / 100.0,
                2 => (r % 10_000_000) as f64 / 1000.0 * if r & 1 == 0 { 1.0 } else { -1.0 },
                3 => (r % 1_000_000_007) as f64 / 7.0,
                4 => (r as f64 / u64::MAX as f64) * 10f64.powi((r % 18) as i32 - 5),
                _ => f64::from_bits(r) % 1e12,
            };
            if !n.is_finite() {
                continue;
            }
            if let Some(quick) = general_number_text(n) {
                assert_eq!(quick, display_with_width(&CellValue::Number(n), "General", false, 8.43), "for {n:?}");
                checked += 1;
            }
        }
        for n in [0.0, -0.0, 1.0, -1.0, 0.001, 0.0009999, 999_999_999.0, 1e9, 0.1 + 0.2, 1234567.891, 12345678.91, 123456789.1] {
            if let Some(quick) = general_number_text(n) {
                assert_eq!(quick, display_with_width(&CellValue::Number(n), "General", false, 8.43), "for {n:?}");
            }
        }
        assert!(checked > 100_000, "the quick text is used for the numbers that matter: {checked}");
    }
}

#[cfg(test)]
mod tests {

    /// A conditional format's solid fill is its bgColor: Excel writes
    /// fgColor indexed="64" (the system foreground, black) beside it, and
    /// fills nothing for a solid fill with an fgColor only (XCF-00177 to 00190).
    #[test]
    fn dxf_solid_fill_is_its_bgcolor() {
        let xml = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dxfs count="3">
            <dxf><fill><patternFill patternType="solid"><fgColor indexed="64"/><bgColor rgb="FFFFF9E6"/></patternFill></fill></dxf>
            <dxf><fill><patternFill patternType="solid"><fgColor rgb="FF00FF00"/></patternFill></fill></dxf>
            <dxf><fill><patternFill><bgColor rgb="FFC6EFCE"/></patternFill></fill></dxf>
        </dxfs></styleSheet>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let theme = crate::theme::Theme::default();
        let colors = SheetColors::new(&theme, Some(doc.root_element()));
        let styles = read_styles(Some(doc.root_element()), &colors);
        let fills: Vec<_> = styles.dxfs.iter().map(|d| d.fill).collect();
        assert_eq!(fills, vec![Some(Rgb(0xFFF9E6)), None, Some(Rgb(0xC6EFCE))]);
    }

    #[test]
    fn print_areas() {
        let r = parse_range("'Sheet 1'!$A$1:$BG$384").unwrap();
        assert_eq!((r.first_column, r.first_row, r.last_column, r.last_row), (0, 0, 58, 383));
        let r = parse_range("見積書!$A:$CG").unwrap();
        assert_eq!((r.first_column, r.first_row, r.last_column, r.last_row), (0, 0, 84, OPEN_END));
    }

    #[test]
    fn print_setup() {
        let xml = r#"<worksheet><sheetPr><pageSetUpPr fitToPage="1"/></sheetPr><printOptions horizontalCentered="1"/><pageMargins left="0.25" right="0.25" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="9" orientation="landscape" fitToHeight="0"/><rowBreaks count="1"><brk id="40" max="16383" man="1"/></rowBreaks></worksheet>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let print = read_print(doc.root_element(), true);
        assert_eq!(print.page_size(), (297 * 36_000, 210 * 36_000));
        assert_eq!(print.fit_to, Some((1, 0)));
        assert_eq!(print.margins[0], EMU_PER_INCH / 4);
        assert_eq!(print.centered, (true, false));
        assert_eq!(print.row_breaks, vec![40]);
        let doc = roxmltree::Document::parse("<worksheet/>").unwrap();
        assert_eq!(read_print(doc.root_element(), true), SheetPrint::default());
    }

    #[test]
    fn extended_print_setup() {
        let xml = r#"<worksheet>
            <printOptions gridLines="1" headings="1" horizontalCentered="1" verticalCentered="1"/>
            <pageMargins left="0.4" right="0.4" top="0.5" bottom="0.5" header="0.2" footer="0.2"/>
            <pageSetup paperSize="43" scale="75" pageOrder="overThenDown" blackAndWhite="1" draft="1" errors="dash" firstPageNumber="5" useFirstPageNumber="1"/>
            <headerFooter differentFirst="1" scaleWithDoc="0" alignWithMargins="0">
                <oddHeader>&amp;CPage &amp;P</oddHeader>
                <oddFooter>&amp;RFooter</oddFooter>
                <firstHeader>&amp;CFirst</firstHeader>
            </headerFooter>
        </worksheet>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let print = read_print(doc.root_element(), true);
        assert_eq!(print.paper, SheetPrint::HAGAKI);
        assert_eq!(print.scale, 75);
        assert!(print.over_then_down);
        assert!(print.black_and_white);
        assert!(print.draft);
        assert_eq!(print.cell_errors.as_deref(), Some("dash"));
        assert_eq!(print.first_page_number, Some(5));
        assert!(print.use_first_page_number);
        assert!(print.grid_lines);
        assert!(print.headings);
        assert_eq!(print.centered, (true, true));
        assert_eq!(print.header_margin, (0.2 * EMU_PER_INCH as f64).round() as Emu);
        assert_eq!(print.footer_margin, (0.2 * EMU_PER_INCH as f64).round() as Emu);
        assert!(print.header_footer.different_first);
        assert!(!print.header_footer.scale_with_doc);
        assert!(!print.header_footer.align_with_margins);
        assert_eq!(print.header_footer.odd_header.as_deref(), Some("&CPage &P"));
        assert_eq!(print.header_footer.odd_footer.as_deref(), Some("&RFooter"));
        assert_eq!(print.header_footer.first_header.as_deref(), Some("&CFirst"));

        // Roundtrip with write_print
        let mut xml_doc = ibis_ooxml::xml::Document::parse(xml.as_bytes()).unwrap();
        crate::save::xlsx::sizes::write_print(&mut xml_doc, &print).unwrap();
        let bytes = xml_doc.to_bytes();
        let serialized = std::str::from_utf8(&bytes).unwrap();
        let doc2 = roxmltree::Document::parse(serialized).unwrap();
        let print2 = read_print(doc2.root_element(), true);
        assert_eq!(print, print2);
    }

    #[test]
    fn shared_formulas_move() {
        assert_eq!(shift_references("SUM(B2:B10)+$C$1*D$3", 2, 1), "SUM(C4:C12)+$C$1*E$3");
        assert_eq!(shift_references("'Sheet 1'!A1&\"A1\"", 1, 0), "'Sheet 1'!A2&\"A1\"");
        assert_eq!(shift_references("LOG10(A1)", 1, 0), "LOG10(A2)");
    }

    use super::*;

    #[test]
    fn cell_references() {
        assert_eq!(cell_ref("A1"), Some((0, 0)));
        assert_eq!(cell_ref("CH366"), Some((85, 365)));
        assert_eq!(cell_ref("1A"), None);
    }

    #[test]
    fn column_width_formula() {
        // ECMA-376 Part 1 §18.3.1.13 with a 6 pt digit: the widths Excel
        // for Mac saves (13.6640625) and prints (82 pt).
        assert_eq!(column_pixels_in(ExcelMetrics::Mac, 13.75, 6.0), 82.0);
        assert_eq!(column_pixels_in(ExcelMetrics::Mac, 13.6640625, 6.0), 82.0);
        assert_eq!(column_pixels_in(ExcelMetrics::Mac, 8.75, 6.0), 52.0);
        assert_eq!(column_pixels_in(ExcelMetrics::Mac, 8.6640625, 6.0), 52.0);
        assert_eq!(column_pixels_in(ExcelMetrics::Mac, 1.25, 6.0), 7.0);
        assert_eq!(column_pixels_in(ExcelMetrics::Mac, 1.1640625, 6.0), 7.0);
        assert_eq!(column_pixels_in(ExcelMetrics::Mac, 12.0, 6.0), 72.0);
        assert_eq!(column_pixels_in(ExcelMetrics::Mac, 3.0, 6.0), 18.0);
    }

    #[test]
    fn column_width_in_the_webs_pixels() {
        // Excel 365 on Windows: the digit is measured by GDI (`font_digit`): 8 px (6 pt) for 11 pt MS PGothic, so a
        // column stored 9.4414 characters wide is 76 px = 57 pt (read from Excel's Columns(i).Width), 7 px for
        // Calibri 11 pt (a column shown 8.43 characters wide, stored 9.140625, is 64 px = 48 pt). Fonts this machine does not have are skipped.
        let ms: Option<&str> = Some("ＭＳ Ｐゴシック");
        if crate::font_digit::max_digit_px(ms.unwrap(), 11.0, 96.0).is_some() {
            let digit = ExcelMetrics::Windows.digit_pt(ms, 11.0);
            assert_eq!(digit, 6.0);
            assert_eq!(column_pixels_in(ExcelMetrics::Windows, 9.44140625, digit), 57.0);
            assert_eq!(column_pixels_in(ExcelMetrics::Windows, 12.0, digit), 72.0);
            // Printed 46 units of 1/600 inch a digit (5.52 pt) against 6 pt shown (Excel 365: 52.46 pt for the 57 pt column).
            let f = ExcelMetrics::Windows.print_column_factor(ms, 11.0);
            assert!((f - 0.92).abs() < 1e-3 && (57.0 * f - 52.44).abs() < 0.1, "{f}");
        }
        if crate::font_digit::max_digit_px("Calibri", 11.0, 96.0).is_some() {
            assert_eq!(ExcelMetrics::Windows.digit_pt(Some("Calibri"), 11.0), 5.25);
            assert_eq!(column_pixels_in(ExcelMetrics::Windows, 9.140625, 5.25), 48.0);
        }
        assert_eq!(ExcelMetrics::Mac.digit_pt(ms, 11.0), 6.0);
    }

    #[test]
    fn default_row_height_follows_the_metrics() {
        let doc = roxmltree::Document::parse(r#"<sheetFormatPr xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" defaultRowHeight="12.75"/>"#).unwrap();
        let format = Some(doc.root_element());
        let arial = Some("Arial".to_string());
        let ms = Some("ＭＳ Ｐゴシック".to_string());
        // The web keeps the declared height; Mac fits an Arial sheet to the font
        // (18 pt) and keeps the declared one only for the MS fonts.
        assert_eq!(default_row_height_in(ExcelMetrics::Windows, format, (&arial, 1000)), 12.75);
        assert_eq!(default_row_height_in(ExcelMetrics::Mac, format, (&arial, 1000)), 18.0);
        assert_eq!(default_row_height_in(ExcelMetrics::Mac, format, (&ms, 1100)), 12.75);
        assert_eq!(default_row_height_in(ExcelMetrics::Windows, format, (&ms, 1100)), 12.75);
    }

    #[test]
    fn tint_lightens_and_darkens() {
        assert_eq!(apply_tint(Rgb(0x000000), 0.5), Rgb(0x808080));
        assert_eq!(apply_tint(Rgb(0xFFFFFF), -0.5), Rgb(0x808080));
    }

    #[test]
    fn table_styles_and_borders_applied() {
        let mut sheet = Sheet {
            cells: Vec::from([
                SheetCell { runs: None, phonetic: None, row: 0, column: 0, value: CellValue::Text("Name".into()), text: "Name".into(), kind: CellKind::Text, formula: None, style: 0 },
                SheetCell { runs: None, phonetic: None, row: 1, column: 0, value: CellValue::Text("Item".into()), text: "Item".into(), kind: CellKind::Text, formula: None, style: 0 },
            ]).into(),
            styles: vec![CellStyle::default()],
            ..Default::default()
        };
        let table_xml = r#"<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" ref="A1:A2" tableBorderDxfId="0" headerRowBorderDxfId="1">
            <tableColumns count="1">
                <tableColumn id="1" name="Name" dataDxfId="2"/>
            </tableColumns>
        </table>"#;
        let thin_border = Border { line: BorderLine::Thin, color: Rgb::BLACK };
        let dxfs = vec![
            ibis_doc_model::CfStyle { borders: [Some(thin_border), Some(thin_border), Some(thin_border), Some(thin_border)], ..Default::default() },
            ibis_doc_model::CfStyle { borders: [None, None, None, Some(thin_border)], ..Default::default() },
            ibis_doc_model::CfStyle { borders: [Some(thin_border), Some(thin_border), None, None], ..Default::default() },
        ];
        apply_tables(&mut sheet, &[table_xml], &dxfs);
        let header_st = &sheet.styles[sheet.cells[0].style as usize];
        assert_eq!(header_st.top, Some(thin_border));
        assert_eq!(header_st.bottom, Some(thin_border));
        assert_eq!(header_st.left, Some(thin_border));
        assert_eq!(header_st.right, Some(thin_border));
        let data_st = &sheet.styles[sheet.cells[1].style as usize];
        assert_eq!(data_st.bottom, Some(thin_border));
        assert_eq!(data_st.left, Some(thin_border));
        assert_eq!(data_st.right, Some(thin_border));
    }

    #[test]
    fn parses_rich_value_errors() {
        let metadata = r#"<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
            xmlns:xlrd="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata">
            <metadataTypes count="1">
                <metadataType name="XLRICHVALUE"/>
            </metadataTypes>
            <futureMetadata name="XLRICHVALUE" count="2">
                <bk><extLst><ext><xlrd:rvb i="0"/></ext></extLst></bk>
                <bk><extLst><ext><xlrd:rvb i="1"/></ext></extLst></bk>
            </futureMetadata>
            <valueMetadata count="2">
                <bk><rc t="1" v="0"/></bk>
                <bk><rc t="1" v="1"/></bk>
            </valueMetadata>
        </metadata>"#;
        let structures = r#"<rvStructures xmlns="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata" count="1">
            <s t="_error">
                <k n="errorType" t="i"/>
            </s>
        </rvStructures>"#;
        let values = r#"<rvData xmlns="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata" count="2">
            <rv s="0"><v>8</v></rv>
            <rv s="0"><v>13</v></rv>
        </rvData>"#;

        let meta_doc = roxmltree::Document::parse(metadata).unwrap();
        let struct_doc = roxmltree::Document::parse(structures).unwrap();
        let val_doc = roxmltree::Document::parse(values).unwrap();
        let errors = rich_value_errors(meta_doc.root_element(), val_doc.root_element(), struct_doc.root_element());
        assert_eq!(errors.get(&1), Some(&CellError::Spill));
        assert_eq!(errors.get(&2), Some(&CellError::Calc));
    }

    #[test]
    fn expands_fill_format_with_column_width() {
        // Space fill
        let val = CellValue::Number(1234.0);
        let res = display_with_width(&val, r#""$"* #,##0"#, false, 60.0);
        // Base is "$ 1,234". Used width: '$'(2) + ' '(1) + '1'(2) + ','(1) + '2'(2) + '3'(2) + '4'(2) = 12.
        // Total width: 60*2 - 2 = 118. Fill count = 118 - 12 = 106.
        assert_eq!(res.len(), 7 + 106);
        assert!(res.starts_with("$ "));
        assert!(res.ends_with("1,234"));

        // Character fill (hyphen)
        let val = CellValue::Number(123.0);
        let res = display_with_width(&val, r#"*-123"#, false, 60.0);
        // Base is "123" (len 3). Total = 60*6 - 5 = 355. saturating_sub(18) = 337. count = 337 / 5 = 67.
        assert_eq!(res.chars().filter(|&c| c == '-').count(), 67);
    }

    #[test]
    fn calculates_auto_row_height() {
        // Font size auto row height
        assert_eq!(font_row_height(11.0), 18.0);
        assert_eq!(font_row_height(18.0), 24.0);
        assert_eq!(font_row_height(24.0), 31.0);
        assert_eq!(font_row_height(36.0), 47.0);

        // Wrapped lines auto row height (Calibri 11pt, col width 10)
        assert_eq!(cell_needed_height("単一行", 10.0, None, 11.0, true, false), 19.0);
        assert_eq!(cell_needed_height("二行に折り返す長い文章テキスト", 10.0, None, 11.0, true, false), 57.0);
        assert_eq!(cell_needed_height("三行に折り返される非常に長い文章テキストサンプル", 10.0, None, 11.0, true, false), 95.0);
        assert_eq!(cell_needed_height("改行1行目\n改行2行目\n改行3行目", 10.0, None, 11.0, true, false), 57.0);
        assert_eq!(cell_needed_height("自動拡張される3行の折り返しテキスト", 10.0, None, 11.0, true, false), 76.0);
        // Many lines stop at Excel's most (409.5 pt).
        assert_eq!(cell_needed_height(&"行\n".repeat(100), 10.0, None, 11.0, true, false), 409.5);
    }

    #[test]
    fn parses_x14_conditional_formatting_with_defined_name() {
        let mut zip = ibis_ooxml::zip::Writer::new();
        let meta = ibis_ooxml::zip::EntryMeta::default();

        zip.add("[Content_Types].xml", meta.clone(), br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#).unwrap();

        zip.add("_rels/.rels", meta.clone(), br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#).unwrap();

        zip.add("xl/_rels/workbook.xml.rels", meta.clone(), br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#).unwrap();

        zip.add("xl/workbook.xml", meta.clone(), br#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="Sheet1" sheetId="1" r:id="rId1"/>
  </sheets>
  <definedNames>
    <definedName name="_NAME_">Sheet1!$B$2</definedName>
  </definedNames>
</workbook>"#).unwrap();

        zip.add("xl/worksheets/sheet1.xml", meta, br#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
           xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"
           xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main">
  <sheetData>
    <row r="2">
      <c r="B2" t="inlineStr"><is><t>Apple</t></is></c>
    </row>
    <row r="3">
      <c r="A3" t="inlineStr"><is><t>A</t></is></c>
      <c r="B3"><v>1</v></c>
    </row>
    <row r="4">
      <c r="A4" t="inlineStr"><is><t>B</t></is></c>
      <c r="B4"><v>2</v></c>
    </row>
    <row r="5">
      <c r="A5" t="inlineStr"><is><t>A</t></is></c>
      <c r="B5"><v>3</v></c>
    </row>
  </sheetData>
  <extLst>
    <ext uri="{78C0D931-6437-407d-A8EE-F0AA7539E650}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main">
      <x14:conditionalFormattings>
        <x14:conditionalFormatting xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main">
          <x14:cfRule type="expression" priority="1" id="{E5B9A136-1234-4567-89AB-CDEF01234567}">
            <xm:f>LEFT(_NAME_,1)&lt;&gt;$A3</xm:f>
            <x14:dxf>
              <font><b val="1"/><color rgb="FF9C0006"/></font>
              <fill><patternFill patternType="solid"><bgColor rgb="FFFFC7CE"/></patternFill></fill>
            </x14:dxf>
          </x14:cfRule>
          <xm:sqref>A3:B5</xm:sqref>
        </x14:conditionalFormatting>
      </x14:conditionalFormattings>
    </ext>
  </extLst>
</worksheet>"#).unwrap();

        let bytes = zip.finish().unwrap();
        let presentation = crate::read(&bytes).unwrap();
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&presentation.slides[0]).unwrap();
        assert_eq!(sheet.conditional_x14.len(), 1);
        let cf = &sheet.conditional_x14[0];
        assert_eq!(cf.ranges, vec![CellRange { first_row: 2, first_column: 0, last_row: 4, last_column: 1 }]);
        assert_eq!(cf.rules.len(), 1);
        assert_eq!(cf.rules[0].kind, ibis_doc_model::CfKind::Expression("LEFT(_NAME_,1)<>$A3".into()));
        let st = cf.rules[0].style.as_ref().unwrap();
        assert_eq!(st.bold, Some(true));
        assert_eq!(st.color, Some(Rgb(0x9C0006)));
        assert_eq!(st.fill, Some(Rgb(0xFFC7CE)));

        assert_eq!(sheet.book_context.sheet_name, "Sheet1");
        assert_eq!(sheet.book_context.sheet_index, 0);
        assert_eq!(sheet.book_context.defined_names.len(), 1);
        assert_eq!(sheet.book_context.defined_names[0].name, "_NAME_");
        assert_eq!(sheet.book_context.defined_names[0].formula, "Sheet1!$B$2");
    }
}



/// シートの保護 (`sheetProtection`), if the sheet is protected.
pub(crate) fn read_protection(root: Node) -> Option<ibis_doc_model::SheetProtection> {
    let p = child(root, "sheetProtection")?;
    if !attr_bool(p, "sheet").unwrap_or(false) {
        return None;
    }
    // Each attribute says the operation is *not* allowed when true (the
    // selections: not allowed when true too), with Excel's defaults.
    let not = |name: &str, default: bool| !attr_bool(p, name).unwrap_or(default);
    let allowed = ibis_doc_model::ProtectionAllowed {
        select_locked: not("selectLockedCells", false),
        select_unlocked: not("selectUnlockedCells", false),
        format_cells: not("formatCells", true),
        format_columns: not("formatColumns", true),
        format_rows: not("formatRows", true),
        insert_columns: not("insertColumns", true),
        insert_rows: not("insertRows", true),
        insert_hyperlinks: not("insertHyperlinks", true),
        delete_columns: not("deleteColumns", true),
        delete_rows: not("deleteRows", true),
        sort: not("sort", true),
        auto_filter: not("autoFilter", true),
        pivot_tables: not("pivotTables", true),
        objects: not("objects", false),
        scenarios: not("scenarios", false),
    };
    let hash = match (attr(p, "algorithmName"), attr(p, "hashValue"), attr(p, "saltValue")) {
        (Some(algorithm), Some(hash), Some(salt)) => Some(ibis_doc_model::PasswordHash {
            algorithm: algorithm.to_string(),
            hash: hash.to_string(),
            salt: salt.to_string(),
            spin_count: attr_i64(p, "spinCount").unwrap_or(0).max(0) as u32,
        }),
        _ => None,
    };
    Some(ibis_doc_model::SheetProtection { hash, legacy: attr(p, "password").map(str::to_string), allowed })
}

/// 編集可能な範囲 (`protectedRanges`), if any are set.
pub(crate) fn read_protected_ranges(root: Node) -> Vec<ibis_doc_model::ProtectedRange> {
    let Some(prs) = child(root, "protectedRanges") else { return Vec::new() };
    children(prs, "protectedRange").filter_map(|p| {
        let name = attr(p, "name").unwrap_or("").to_string();
        let ranges: Vec<ibis_doc_model::CellRange> = attr(p, "sqref").unwrap_or("").split_whitespace().filter_map(parse_range).collect();
        if ranges.is_empty() && name.is_empty() {
            return None;
        }
        let hash = match (attr(p, "algorithmName"), attr(p, "hashValue"), attr(p, "saltValue")) {
            (Some(algorithm), Some(hash), Some(salt)) => Some(ibis_doc_model::PasswordHash {
                algorithm: algorithm.to_string(),
                hash: hash.to_string(),
                salt: salt.to_string(),
                spin_count: attr_i64(p, "spinCount").unwrap_or(0).max(0) as u32,
            }),
            _ => None,
        };
        let legacy = attr(p, "password").map(str::to_string);
        Some(ibis_doc_model::ProtectedRange { name, ranges, hash, legacy })
    }).collect()
}

/// ブックの保護 (`workbookProtection`), if the workbook is protected.
pub(crate) fn read_workbook_protection(root: Node) -> Option<ibis_doc_model::WorkbookProtection> {
    let p = child(root, "workbookProtection")?;
    let lock_structure = attr_bool(p, "lockStructure").unwrap_or(false);
    let lock_windows = attr_bool(p, "lockWindows").unwrap_or(false);
    let lock_revision = attr_bool(p, "lockRevision").unwrap_or(false);

    let algorithm = attr(p, "workbookAlgorithmName").or_else(|| attr(p, "algorithmName"));
    let hash = attr(p, "workbookHashValue").or_else(|| attr(p, "hashValue"));
    let salt = attr(p, "workbookSaltValue").or_else(|| attr(p, "saltValue"));
    let spin_count = attr_i64(p, "workbookSpinCount")
        .or_else(|| attr_i64(p, "spinCount"))
        .unwrap_or(0)
        .max(0) as u32;

    let hash_obj = match (algorithm, hash, salt) {
        (Some(algo), Some(h), Some(s)) => Some(ibis_doc_model::PasswordHash {
            algorithm: algo.to_string(),
            hash: h.to_string(),
            salt: s.to_string(),
            spin_count,
        }),
        _ => None,
    };
    let legacy = attr(p, "workbookPassword").or_else(|| attr(p, "password")).map(str::to_string);
    Some(ibis_doc_model::WorkbookProtection {
        hash: hash_obj,
        legacy,
        lock_structure,
        lock_windows,
        lock_revision,
    })
}

/// ウィンドウの非表示 (`bookViews/workbookView/@visibility="hidden"`).
pub(crate) fn read_workbook_window_hidden(root: Node) -> bool {
    child(root, "bookViews")
        .into_iter()
        .flat_map(|bv| children(bv, "workbookView"))
        .any(|v| attr(v, "visibility") == Some("hidden"))
}

