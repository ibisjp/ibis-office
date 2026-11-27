// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Themes: the colour scheme, the colour map and theme fonts.

use std::collections::HashMap;

use ibis_doc_model::{Color, Rgb};
use roxmltree::Node;

use crate::xml::{attr, child, children, elements};

const DRAWING_NS:&str="http://schemas.openxmlformats.org/drawingml/2006/main";
fn colour_attribute<'a>(node:Node<'a,'_>,name:&str)->Option<&'a str> {
    node.attributes().find(|a|a.namespace().is_none() && a.name()==name).map(|a|a.value())
}

/// Theme colours by name (`dk1`, `accent1`, …) and theme fonts.
#[derive(Clone, Debug, Default)]
pub(crate) struct Theme {
    colors: HashMap<String, Rgb>,
    major_latin: Option<String>,
    major_east_asian: Option<String>,
    minor_latin: Option<String>,
    minor_east_asian: Option<String>,
    /// The fonts for Japanese text (`<a:font script="Jpan">`), which Word
    /// uses when the East Asian font is left empty.
    major_japanese: Option<String>,
    minor_japanese: Option<String>,
    major_complex: Option<String>,
    minor_complex: Option<String>,
    pub(crate) fmt_scheme_name: Option<String>,
}

impl Theme {
    /// The Japanese font of the theme's major (headings) or minor (body)
    /// fonts, if it names one (`<a:font script="Jpan">`, else `ea`).
    pub(crate) fn japanese_font(&self, major: bool) -> Option<&str> {
        let (jpan, ea) = if major { (&self.major_japanese, &self.major_east_asian) } else { (&self.minor_japanese, &self.minor_east_asian) };
        jpan.as_deref().filter(|f| !f.is_empty()).or(ea.as_deref().filter(|f| !f.is_empty()))
    }

    /// The theme's typefaces as Word's theme font references (`majorHAnsi`, `minorEastAsia`, ...) resolve them.
    pub(crate) fn font_names(&self) -> ibis_doc_model::ThemeFonts {
        let set = |major: bool| ibis_doc_model::FontSet {
            latin: self.typeface(if major { "+mj-lt" } else { "+mn-lt" }),
            east_asian: self.east_asian_typeface(major),
            complex: self.complex_typeface(major),
        };
        ibis_doc_model::ThemeFonts { major: set(true), minor: set(false) }
    }

    /// The colour scheme, by name.
    pub(crate) fn colors(&self) -> Vec<(String, Rgb)> {
        let mut out: Vec<(String, Rgb)> = self.colors.iter().map(|(k, v)| (k.clone(), *v)).collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// A theme of these colours only (no fonts).
    pub(crate) fn from_colors(colors: &[(String, Rgb)]) -> Self {
        Self { colors: colors.iter().cloned().collect(), ..Self::default() }
    }

    /// A theme of these colours and fonts.
    pub(crate) fn from_colors_and_fonts(colors: &[(String, Rgb)], fonts: Option<&ibis_doc_model::ThemeFonts>) -> Self {
        let mut t = Self::from_colors(colors);
        if let Some(f) = fonts {
            t.major_latin = f.major.latin.clone();
            t.major_east_asian = f.major.east_asian.clone();
            t.major_complex = f.major.complex.clone();
            t.minor_latin = f.minor.latin.clone();
            t.minor_east_asian = f.minor.east_asian.clone();
            t.minor_complex = f.minor.complex.clone();
        }
        t
    }

    pub(crate) fn read(theme: Node) -> Self {
        let mut out = Self::default();
        let Some(elements) = child(theme, "themeElements") else { return out };
        out.apply_elements(elements);
        out
    }

    /// A copy of this theme with the schemes of an `a:themeOverride` part applied.
    pub(crate) fn with_override(&self, override_root: Node) -> Self {
        let mut out = self.clone();
        out.apply_elements(override_root);
        out
    }

    fn apply_elements(&mut self, elements: Node) {
        if let Some(scheme) = child(elements, "clrScheme") {
            for entry in scheme.children().filter(Node::is_element) {
                // Theme colours are srgbClr or sysClr, without modifiers.
                let Some(value) = entry.children().find(Node::is_element) else { continue };
                let hex = if value.has_tag_name("sysClr") { attr(value, "lastClr") } else { attr(value, "val") };
                if let Some(rgb) = hex.and_then(parse_hex) {
                    self.colors.insert(entry.tag_name().name().to_string(), rgb);
                }
            }
        }
        if let Some(fonts) = child(elements, "fontScheme") {
            let typeface = |group: &str, script: &str| {
                child(fonts, group)
                    .and_then(|g| child(g, script))
                    .and_then(|f| attr(f, "typeface"))
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
            };
            self.major_latin = typeface("majorFont", "latin");
            self.major_east_asian = typeface("majorFont", "ea");
            self.minor_latin = typeface("minorFont", "latin");
            self.minor_east_asian = typeface("minorFont", "ea");
            let japanese = |group: &str| {
                child(fonts, group)
                    .and_then(|g| children(g, "font").find(|f| attr(*f, "script") == Some("Jpan")))
                    .and_then(|f| attr(f, "typeface"))
                    .filter(|t| !t.is_empty())
                    .map(str::to_string)
            };
            self.major_japanese = japanese("majorFont");
            self.minor_japanese = japanese("minorFont");
            self.major_complex = typeface("majorFont", "cs");
            self.minor_complex = typeface("minorFont", "cs");
        }
        if let Some(fmt) = child(elements, "fmtScheme") {
            self.fmt_scheme_name = attr(fmt, "name").map(str::to_string);
        }
    }

    /// The theme's fonts, for the model.
    pub(crate) fn fonts(&self) -> ibis_doc_model::ThemeFonts {
        let set = |latin: &Option<String>, east_asian: &Option<String>, japanese: &Option<String>, complex: &Option<String>| ibis_doc_model::FontSet {
            latin: latin.clone(),
            east_asian: east_asian.clone().or_else(|| japanese.clone()),
            complex: complex.clone(),
        };
        ibis_doc_model::ThemeFonts {
            major: set(&self.major_latin, &self.major_east_asian, &self.major_japanese, &self.major_complex),
            minor: set(&self.minor_latin, &self.minor_east_asian, &self.minor_japanese, &self.minor_complex),
        }
    }

    /// A scheme colour by its own name (`dk1`, `accent1`, …).
    pub(crate) fn color(&self, name: &str) -> Option<Rgb> {
        self.colors.get(name).copied()
    }

    /// Resolve a typeface that may name a theme font (`+mj-lt`, `+mn-ea`, …).
    pub(crate) fn typeface(&self, name: &str) -> Option<String> {
        let resolved = match name {
            "+mj-lt" => self.major_latin.clone(),
            "+mj-ea" => self.major_east_asian.clone().or_else(|| self.major_latin.clone()),
            "+mj-cs" => self.major_complex.clone().or_else(|| self.major_latin.clone()),
            "+mn-lt" => self.minor_latin.clone(),
            "+mn-ea" => self.minor_east_asian.clone().or_else(|| self.minor_latin.clone()),
            "+mn-cs" => self.minor_complex.clone().or_else(|| self.minor_latin.clone()),
            "" => None,
            other => Some(other.to_string()),
        };
        resolved.filter(|t| !t.is_empty())
    }

    /// For the theme's East Asian font `name` (`+mn-ea`, `+mj-ea`) left
    /// empty by the theme: its Japanese font, if it names one.
    pub(crate) fn empty_east_asian_japanese(&self, name: &str) -> Option<String> {
        let (ea, jpan) = match name {
            "+mn-ea" => (&self.minor_east_asian, &self.minor_japanese),
            "+mj-ea" => (&self.major_east_asian, &self.major_japanese),
            _ => return None,
        };
        ea.as_deref().filter(|f| !f.is_empty()).is_none().then(|| jpan.clone()).flatten()
    }

    /// A Word document's theme East Asian font (`+mj-ea`, `+mn-ea`): the
    /// `ea` font, else the Japanese script's font, else the Latin one (a
    /// document written in Japanese, as the reader assumes).
    pub(crate) fn east_asian_typeface(&self, major: bool) -> Option<String> {
        let (ea, jpan) = if major { (&self.major_east_asian, &self.major_japanese) } else { (&self.minor_east_asian, &self.minor_japanese) };
        ea.clone().or_else(|| jpan.clone()).or_else(|| self.typeface(if major { "+mj-lt" } else { "+mn-lt" }))
    }

    /// A Word document's theme Complex Script font (`+mj-cs`, `+mn-cs`): the
    /// `cs` font, else the Latin one.
    pub(crate) fn complex_typeface(&self, major: bool) -> Option<String> {
        let cs = if major { &self.major_complex } else { &self.minor_complex };
        cs.clone().or_else(|| self.typeface(if major { "+mj-cs" } else { "+mn-cs" }))
    }
}

/// The colours of Office's default theme ("Office", 2013 and later).
fn office_default_colour(name: &str) -> Option<Rgb> {
    Some(Rgb(match name {
        "dk1" => 0x000000,
        "lt1" => 0xFFFFFF,
        "dk2" => 0x44546A,
        "lt2" => 0xE7E6E6,
        "accent1" => 0x4472C4,
        "accent2" => 0xED7D31,
        "accent3" => 0xA5A5A5,
        "accent4" => 0xFFC000,
        "accent5" => 0x5B9BD5,
        "accent6" => 0x70AD47,
        "hlink" => 0x0563C1,
        "folHlink" => 0x954F72,
        _ => return None,
    }))
}

/// Resolves colour elements for one slide: its theme plus the colour map that
/// turns `tx1`, `bg1`, … into theme colour names.
#[derive(Clone, Debug)]
pub(crate) struct Colors<'t> {
    theme: &'t Theme,
    map: HashMap<String, String>,
    /// Keep the theme reference of a `a:schemeClr` on the colour it gives (presentations: the colour follows the theme).
    refs: bool,
}

impl<'t> Colors<'t> {
    /// The colour map of a master (`p:clrMap`), optionally overridden by a
    /// layout's or slide's `p:clrMapOvr`.
    pub(crate) fn new(theme: &'t Theme, master_map: Option<Node>, overrides: &[Option<Node>]) -> Self {
        let mut map: HashMap<String, String> = [
            ("bg1", "lt1"),
            ("tx1", "dk1"),
            ("bg2", "lt2"),
            ("tx2", "dk2"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let mut apply = |node: Node| {
            for a in node.attributes() {
                map.insert(a.name().to_string(), a.value().to_string());
            }
        };
        if let Some(node) = master_map {
            apply(node);
        }
        for over in overrides.iter().flatten() {
            if let Some(node) = child(*over, "overrideClrMapping") {
                apply(node);
            }
        }
        Self { theme, map, refs: false }
    }

    /// These colours keep the theme reference of every `a:schemeClr` they resolve (see [`ibis_doc_model::ThemeRef`]).
    pub(crate) fn keeping_theme_refs(mut self) -> Self {
        self.refs = true;
        self
    }

    pub(crate) fn theme(&self) -> &Theme {
        self.theme
    }

    pub(crate) fn with_theme<'u>(&self, theme: &'u Theme) -> Colors<'u> {
        Colors { theme, map: self.map.clone(), refs: self.refs }
    }

    /// A theme colour by name (`accent1`, `tx1`, …), through the colour map.
    pub(crate) fn named(&self, name: &str) -> Color {
        Color::opaque(self.scheme(name).unwrap_or(Rgb::BLACK))
    }

    /// A theme colour by name as a colour that keeps its reference (when references are kept): what a hyperlink's colour is.
    pub(crate) fn named_ref(&self, name: &str) -> Option<Color> {
        let rgb = self.scheme(name)?;
        let mut color = Color::opaque(rgb);
        if self.refs {
            let slot = ibis_doc_model::ThemeSlot::from_name(name)?;
            let mapped = self.map.get(name).map(String::as_str).unwrap_or(name);
            let via = ibis_doc_model::ThemeSlot::from_name(mapped)?;
            color.theme = ibis_doc_model::ThemeRef::new(slot, via, &[]).map(|t| t.resolved_to(rgb, 255));
        }
        Some(color)
    }

    pub(crate) fn scheme(&self, name: &str) -> Option<Rgb> {
        let mapped = self.map.get(name).map(String::as_str).unwrap_or(name);
        // A file with no theme part (or a theme without colours) is drawn with Office's own default theme, as Office does: a chart's
        // automatic series colours are accent 1, 2, ... of it, not black.
        self.theme.colors.get(mapped).copied().or_else(|| self.theme.colors.is_empty().then(|| office_default_colour(mapped)).flatten())
    }

    /// The colour given by the first colour element among `parent`'s children.
    /// `placeholder` is the colour `phClr` stands for, from a style reference.
    pub(crate) fn in_parent(&self, parent: Node, placeholder: Option<Color>) -> Option<Color> {
        parent.children().filter(Node::is_element).find_map(|n| self.resolve(n, placeholder))
    }

    /// Resolve one colour element (`a:srgbClr`, `a:schemeClr`, …) with its
    /// modifiers. `None` if `node` is not a colour element.
    pub(crate) fn resolve(&self, node: Node, placeholder: Option<Color>) -> Option<Color> {
        if node.tag_name().namespace()!=Some(DRAWING_NS) {return None;}
        let base = match node.tag_name().name() {
            "srgbClr" => Color::opaque(colour_attribute(node, "val").and_then(parse_hex)?),
            "sysClr" => Color::opaque(colour_attribute(node, "lastClr").and_then(parse_hex).unwrap_or(Rgb::BLACK)),
            "schemeClr" => match colour_attribute(node, "val")? {
                "phClr" => placeholder.unwrap_or(Color::BLACK),
                name => Color::opaque(self.scheme(name).unwrap_or(Rgb::BLACK)),
            },
            "prstClr" => Color::opaque(preset_color(colour_attribute(node, "val")?)),
            "scrgbClr" => {
                let channel = |name| {
                    let linear = colour_attribute(node, name).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) / 100_000.0;
                    (linear_to_srgb(linear) * 255.0).round() as u8
                };
                Color::opaque(Rgb::from_rgb(channel("r"), channel("g"), channel("b")))
            }
            "hslClr" => {
                let get = |name| colour_attribute(node, name).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
                let (r, g, b) = hsl_to_rgb(get("hue") / 21_600_000.0, get("sat") / 100_000.0, get("lum") / 100_000.0);
                Color::opaque(Rgb::from_rgb(r, g, b))
            }
            _ => return None,
        };
        let mut color = apply_modifiers(base, node);
        if self.refs && node.tag_name().name() == "schemeClr" {
            color.theme = self.theme_ref(node).map(|t| t.resolved_to(color.rgb, color.alpha));
        }
        Some(color)
    }

    /// The reference a `a:schemeClr` makes (its name as written, as the colour map turns it, and its transforms). `None` for
    /// `phClr`, a name that is no slot, or transforms the reference cannot hold (they stay in the file's element).
    fn theme_ref(&self, node: Node) -> Option<ibis_doc_model::ThemeRef> {
        let name = ibis_doc_model::ThemeSlot::from_name(colour_attribute(node, "val")?)?;
        let mapped = self.map.get(name.name()).map(String::as_str).unwrap_or(name.name());
        let via = ibis_doc_model::ThemeSlot::from_name(mapped)?;
        let mut mods = Vec::new();
        for m in elements(node) {
            if m.tag_name().namespace() != Some(DRAWING_NS) {
                return None;
            }
            let val = match colour_attribute(m, "val") {
                Some(v) => v.parse::<i32>().ok()?,
                None => 0,
            };
            mods.push(ibis_doc_model::ThemeMod::new(m.tag_name().name(), val)?);
        }
        // A colour the theme does not name (a file without a theme part) is drawn with Office's defaults; it still follows the slot.
        ibis_doc_model::ThemeRef::new(name, via, &mods)
    }
}

fn parse_hex(value: &str) -> Option<Rgb> {
    (value.len() == 6).then(|| u32::from_str_radix(value, 16).ok()).flatten().map(Rgb)
}

/// The colour of a preset colour name (`a:prstClr`, ST_PresetColorVal of ECMA-376 Part 1: the 140 names of
/// the CSS colour keywords, `dk`/`lt`/`med` for dark/light/medium).
fn preset_color(name: &str) -> Rgb {
    // The long names (`darkBlue`, `lightBlue`, `mediumBlue`) are the short ones' (`dkBlue`, `ltBlue`, `medBlue`).
    let short;
    let name = match (name.strip_prefix("dark"), name.strip_prefix("light"), name.strip_prefix("medium")) {
        (Some(rest), _, _) => {
            short = format!("dk{rest}");
            short.as_str()
        }
        (_, Some(rest), _) => {
            short = format!("lt{rest}");
            short.as_str()
        }
        (_, _, Some(rest)) => {
            short = format!("med{rest}");
            short.as_str()
        }
        _ => name,
    };
    Rgb(match name {
        "aliceBlue" => 0xf0f8ff,
        "antiqueWhite" => 0xfaebd7,
        "aqua" => 0x00ffff,
        "aquamarine" => 0x7fffd4,
        "azure" => 0xf0ffff,
        "beige" => 0xf5f5dc,
        "bisque" => 0xffe4c4,
        "black" => 0x000000,
        "blanchedAlmond" => 0xffebcd,
        "blue" => 0x0000ff,
        "blueViolet" => 0x8a2be2,
        "brown" => 0xa52a2a,
        "burlyWood" => 0xdeb887,
        "cadetBlue" => 0x5f9ea0,
        "chartreuse" => 0x7fff00,
        "chocolate" => 0xd2691e,
        "coral" => 0xff7f50,
        "cornflowerBlue" => 0x6495ed,
        "cornsilk" => 0xfff8dc,
        "crimson" => 0xdc143c,
        "cyan" => 0x00ffff,
        "dkBlue" => 0x00008b,
        "dkCyan" => 0x008b8b,
        "dkGoldenrod" => 0xb8860b,
        "dkGray" => 0xa9a9a9,
        "dkGreen" => 0x006400,
        "dkGrey" => 0xa9a9a9,
        "dkKhaki" => 0xbdb76b,
        "dkMagenta" => 0x8b008b,
        "dkOliveGreen" => 0x556b2f,
        "dkOrange" => 0xff8c00,
        "dkOrchid" => 0x9932cc,
        "dkRed" => 0x8b0000,
        "dkSalmon" => 0xe9967a,
        "dkSeaGreen" => 0x8fbc8f,
        "dkSlateBlue" => 0x483d8b,
        "dkSlateGray" => 0x2f4f4f,
        "dkSlateGrey" => 0x2f4f4f,
        "dkTurquoise" => 0x00ced1,
        "dkViolet" => 0x9400d3,
        "deepPink" => 0xff1493,
        "deepSkyBlue" => 0x00bfff,
        "dimGray" => 0x696969,
        "dimGrey" => 0x696969,
        "dodgerBlue" => 0x1e90ff,
        "firebrick" => 0xb22222,
        "floralWhite" => 0xfffaf0,
        "forestGreen" => 0x228b22,
        "fuchsia" => 0xff00ff,
        "gainsboro" => 0xdcdcdc,
        "ghostWhite" => 0xf8f8ff,
        "gold" => 0xffd700,
        "goldenrod" => 0xdaa520,
        "gray" => 0x808080,
        "grey" => 0x808080,
        "green" => 0x008000,
        "greenYellow" => 0xadff2f,
        "honeydew" => 0xf0fff0,
        "hotPink" => 0xff69b4,
        "indianRed" => 0xcd5c5c,
        "indigo" => 0x4b0082,
        "ivory" => 0xfffff0,
        "khaki" => 0xf0e68c,
        "lavender" => 0xe6e6fa,
        "lavenderBlush" => 0xfff0f5,
        "lawnGreen" => 0x7cfc00,
        "lemonChiffon" => 0xfffacd,
        "ltBlue" => 0xadd8e6,
        "ltCoral" => 0xf08080,
        "ltCyan" => 0xe0ffff,
        "ltGoldenrodYellow" => 0xfafad2,
        "ltGray" => 0xd3d3d3,
        "ltGreen" => 0x90ee90,
        "ltGrey" => 0xd3d3d3,
        "ltPink" => 0xffb6c1,
        "ltSalmon" => 0xffa07a,
        "ltSeaGreen" => 0x20b2aa,
        "ltSkyBlue" => 0x87cefa,
        "ltSlateGray" => 0x778899,
        "ltSlateGrey" => 0x778899,
        "ltSteelBlue" => 0xb0c4de,
        "ltYellow" => 0xffffe0,
        "lime" => 0x00ff00,
        "limeGreen" => 0x32cd32,
        "linen" => 0xfaf0e6,
        "magenta" => 0xff00ff,
        "maroon" => 0x800000,
        "medAquamarine" => 0x66cdaa,
        "medBlue" => 0x0000cd,
        "medOrchid" => 0xba55d3,
        "medPurple" => 0x9370db,
        "medSeaGreen" => 0x3cb371,
        "medSlateBlue" => 0x7b68ee,
        "medSpringGreen" => 0x00fa9a,
        "medTurquoise" => 0x48d1cc,
        "medVioletRed" => 0xc71585,
        "midnightBlue" => 0x191970,
        "mintCream" => 0xf5fffa,
        "mistyRose" => 0xffe4e1,
        "moccasin" => 0xffe4b5,
        "navajoWhite" => 0xffdead,
        "navy" => 0x000080,
        "oldLace" => 0xfdf5e6,
        "olive" => 0x808000,
        "oliveDrab" => 0x6b8e23,
        "orange" => 0xffa500,
        "orangeRed" => 0xff4500,
        "orchid" => 0xda70d6,
        "paleGoldenrod" => 0xeee8aa,
        "paleGreen" => 0x98fb98,
        "paleTurquoise" => 0xafeeee,
        "paleVioletRed" => 0xdb7093,
        "papayaWhip" => 0xffefd5,
        "peachPuff" => 0xffdab9,
        "peru" => 0xcd853f,
        "pink" => 0xffc0cb,
        "plum" => 0xdda0dd,
        "powderBlue" => 0xb0e0e6,
        "purple" => 0x800080,
        "red" => 0xff0000,
        "rosyBrown" => 0xbc8f8f,
        "royalBlue" => 0x4169e1,
        "saddleBrown" => 0x8b4513,
        "salmon" => 0xfa8072,
        "sandyBrown" => 0xf4a460,
        "seaGreen" => 0x2e8b57,
        "seaShell" => 0xfff5ee,
        "sienna" => 0xa0522d,
        "silver" => 0xc0c0c0,
        "skyBlue" => 0x87ceeb,
        "slateBlue" => 0x6a5acd,
        "slateGray" => 0x708090,
        "slateGrey" => 0x708090,
        "snow" => 0xfffafa,
        "springGreen" => 0x00ff7f,
        "steelBlue" => 0x4682b4,
        "tan" => 0xd2b48c,
        "teal" => 0x008080,
        "thistle" => 0xd8bfd8,
        "tomato" => 0xff6347,
        "turquoise" => 0x40e0d0,
        "violet" => 0xee82ee,
        "wheat" => 0xf5deb3,
        "white" => 0xffffff,
        "whiteSmoke" => 0xf5f5f5,
        "yellow" => 0xffff00,
        "yellowGreen" => 0x9acd32,
        _ => 0x000000,
    })
}

/// Apply DrawingML colour transforms in document order as PowerPoint does.
fn apply_modifiers(color: Color, node: Node) -> Color {
    let mods: Vec<(&str, f32)> = elements(node)
        .filter(|m| m.tag_name().namespace() == Some(DRAWING_NS))
        .map(|m| (m.tag_name().name(), colour_attribute(m, "val").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0)))
        .collect();
    ibis_doc_model::apply_color_modifiers(color, &mods)
}

use ibis_doc_model::theme_color::{hsl_to_rgb_f32, linear_to_srgb};

/// `h`, `s` and `l` in 0..=1.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let (r, g, b) = hsl_to_rgb_f32(h, s, l);
    let to8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    (to8(r), to8(g), to8(b))
}

/// Line widths of the theme's `a:lnStyleLst`, for `p:style/a:lnRef idx`.
pub(crate) fn line_style<'a, 'i>(theme: Node<'a, 'i>, idx: usize) -> Option<Node<'a, 'i>> {
    let list = child(child(child(theme, "themeElements")?, "fmtScheme")?, "lnStyleLst")?;
    children(list, "ln").nth(idx.checked_sub(1)?)
}

/// The fill style `idx` refers to: 1–999 index `a:fillStyleLst`, 1001 and up
/// `a:bgFillStyleLst`.
pub(crate) fn fill_style<'a, 'i>(theme: Node<'a, 'i>, idx: usize) -> Option<Node<'a, 'i>> {
    let scheme = child(child(theme, "themeElements")?, "fmtScheme")?;
    let (list, index) = if idx > 1000 { ("bgFillStyleLst", idx - 1001) } else { ("fillStyleLst", idx.checked_sub(1)?) };
    elements(child(scheme, list)?).nth(index)
}

/// The background fill style `p:bgRef idx` refers to (ECMA-376 20.1.4.2.1):
/// 1–999 index `a:fillStyleLst`, 1001 and up `a:bgFillStyleLst`, the same
/// numbering as `fill_style`.
pub(crate) fn bg_fill_style<'a, 'i>(theme: Node<'a, 'i>, idx: usize) -> Option<Node<'a, 'i>> {
    fill_style(theme, idx)
}

/// The effect style `idx` refers to: 1-based index in `a:effectStyleLst`.
pub(crate) fn effect_style<'a, 'i>(theme: Node<'a, 'i>, idx: usize) -> Option<Node<'a, 'i>> {
    let scheme = child(child(theme, "themeElements")?, "fmtScheme")?;
    let list = child(scheme, "effectStyleLst")?;
    children(list, "effectStyle").nth(idx.checked_sub(1)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_without_a_theme_uses_the_office_default_colours() {
        let theme = Theme::default();
        let colors = Colors::new(&theme, None, &[]);
        assert_eq!(colors.named("accent1").rgb, Rgb(0x4472C4));
        assert_eq!(colors.named("tx2").rgb, Rgb(0x44546A));
    }

    fn color_of(xml: &str) -> Color {
        // Colour fixtures use the same DrawingML URI as a real Office part.
        let xml=format!("<root xmlns=\"{DRAWING_NS}\">{xml}</root>");
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let theme = Theme::default();
        let colors = Colors::new(&theme, None, &[]);
        colors.resolve(doc.root_element().first_element_child().unwrap(), None).unwrap()
    }

    #[test]
    fn lum_mod_and_off_lighten() {
        // 0x4472C4 at lumMod 60% / lumOff 40% is PowerPoint's 0x8FAADC.
        let c = color_of(r#"<srgbClr val="4472C4"><lumMod val="60000"/><lumOff val="40000"/></srgbClr>"#);
        let (r, g, b) = (c.rgb.r() as i32, c.rgb.g() as i32, c.rgb.b() as i32);
        assert!((r - 0x8f).abs() <= 2 && (g - 0xaa).abs() <= 2 && (b - 0xdc).abs() <= 2, "{:06x}", c.rgb.0);
    }

    #[test]
    fn alpha_modifier() {
        let c = color_of(r#"<srgbClr val="FF0000"><alpha val="50000"/></srgbClr>"#);
        assert_eq!(c.rgb, Rgb(0xff0000));
        assert_eq!(c.alpha, 128);
    }

    #[test]
    fn alpha_offset_is_ordered_clamped_and_namespace_scoped() {
        for (transforms,expected) in [
            (r#"<alpha val="50000"/><alphaOff val="10000"/>"#,154),
            (r#"<alpha val="50000"/><alphaMod val="50000"/><alphaOff val="10000"/>"#,90),
            (r#"<alpha val="50000"/><alphaOff val="10000"/><alphaMod val="50000"/>"#,77),
            (r#"<alpha val="50000"/><alphaOff val="-100000"/>"#,0),
            (r#"<alpha val="50000"/><alphaOff val="100000"/>"#,255),
            (r#"<alpha val="50000"/><x:alphaOff xmlns:x="urn:ibis:test:alpha" val="100000"/>"#,128),
            (r#"<alpha val="50000"/><alphaOff xmlns:x="urn:ibis:test:alpha" x:val="100000" val="0"/>"#,128),
        ] {
            let color=color_of(&format!("<srgbClr val=\"112233\">{transforms}</srgbClr>"));
            assert_eq!(color.rgb,Rgb(0x112233));assert_eq!(color.alpha,expected,"{transforms}");
        }
    }

    #[test]
    fn scheme_colors_follow_color_map() {
        let doc = roxmltree::Document::parse(
            r#"<theme xmlns="http://schemas.openxmlformats.org/drawingml/2006/main"><themeElements><clrScheme><dk1><sysClr lastClr="000000"/></dk1><lt1><srgbClr val="FFFFFF"/></lt1></clrScheme></themeElements></theme>"#,
        )
        .unwrap();
        let theme = Theme::read(doc.root_element());
        let map = roxmltree::Document::parse(r#"<clrMap bg1="dk1" tx1="lt1"/>"#).unwrap();
        let colors = Colors::new(&theme, Some(map.root_element()), &[]);
        let tx1 = roxmltree::Document::parse(r#"<schemeClr xmlns="http://schemas.openxmlformats.org/drawingml/2006/main" val="tx1"/>"#).unwrap();
        assert_eq!(colors.resolve(tx1.root_element(), None).unwrap().rgb, Rgb(0xffffff));
    }

    /// The preset colour names (`a:prstClr`), short and long forms, as the specification names them.
    #[test]
    fn preset_colors_have_their_values() {
        assert_eq!(preset_color("moccasin"), Rgb(0xffe4b5));
        assert_eq!(preset_color("tomato"), Rgb(0xff6347));
        assert_eq!(preset_color("dkRed"), Rgb(0x8b0000));
        assert_eq!(preset_color("darkRed"), Rgb(0x8b0000));
        assert_eq!(preset_color("lightGoldenrodYellow"), Rgb(0xfafad2));
        assert_eq!(preset_color("medVioletRed"), Rgb(0xc71585));
        assert_eq!(preset_color("mediumVioletRed"), Rgb(0xc71585));
        assert_eq!(preset_color("slateGrey"), Rgb(0x708090));
        assert_eq!(preset_color("green"), Rgb(0x008000));
        assert_eq!(preset_color("lime"), Rgb(0x00ff00));
    }
}
