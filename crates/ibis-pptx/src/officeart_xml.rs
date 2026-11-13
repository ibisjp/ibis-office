// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An OfficeArt shape's look ([MS-ODRAW]: the shape type and the properties of its `OfficeArtFOPT`) as DrawingML:
//! its geometry, fill, outline and shadow. Shared by the old presentation (`ppt_write`) and the old workbook
//! (`xls_drawing`), which give the colors and pictures of their files through [`ArtContext`].

use std::fmt::Write as _;

use crate::ppt::Props;

/// What the writing needs from the file the shape is in.
pub(crate) trait ArtContext {
    /// The properties a shape has when it does not set them (the file's default `OfficeArtFOPT`).
    fn defaults(&self) -> &Props;
    /// The sRGB of an `OfficeArtCOLORREF` (MS-ODRAW 2.2.2) in this file: its scheme or palette index, or its RGB.
    fn color(&self, colorref: u32) -> u32;
    /// The relationship id of the picture `index` (1-based in the file's BLIP store), when it is written.
    fn picture(&mut self, index: usize) -> Option<String>;
    /// The sRGB of the page's background when it is one flat color (a shape of fill type 9 is filled with it); None when the
    /// background is a gradient or a picture, which a shape of that fill shows through to (nothing drawn).
    fn flat_background(&self) -> Option<u32> {
        Some(0xFFFFFF)
    }
}

/// The geometry of a shape of type `spt` (`a:prstGeom`, or `a:custGeom` for a freeform).
pub(crate) fn geometry_xml(spt: u16, p: &Props) -> String {
    let prst = preset(spt).unwrap_or("rect");
    let mut av = String::new();
    if spt == 2 {
        if let Some(adj) = p.int(0x147) {
            // The corner radius: the file's 21600ths of the shorter side, as the preset's 100000ths.
            let _ = write!(av, r#"<a:gd name="adj" fmla="val {}"/>"#, (i64::from(adj) * 100_000 * 2 / 3 / 21_600 * 3 / 2).clamp(0, 50_000) * 2 / 3 * 3 / 2);
        }
    }
    if matches!(spt, 61..=63) && (p.int(0x147).is_some() || p.int(0x148).is_some()) {
        // The callout's tip: adjust1 and adjust2 are its x and y in 21600ths of the shape's width and height (MS-ODRAW 2.2.2 msosptWedgeRectCallout,
        // defaults 1350 and 25650), and the preset's `adj1` and `adj2` are its offsets from the centre in 100000ths of them. PowerPoint's own pptx
        // of 058.ppt (14th slide: 23341 and 12034 are 58061 and 5715) agrees.
        let to_adj = |v: i32| (i64::from(v) - 10_800) * 100_000 / 21_600;
        let _ = write!(av, r#"<a:gd name="adj1" fmla="val {}"/><a:gd name="adj2" fmla="val {}"/>"#, to_adj(p.int(0x147).unwrap_or(1350)), to_adj(p.int(0x148).unwrap_or(25650)));
    }
    custom_geometry(p).unwrap_or_else(|| format!(r#"<a:prstGeom prst="{prst}"><a:avLst>{av}</a:avLst></a:prstGeom>"#))
}

/// The preset geometry of a shape type (`MSOSPT`).
pub(crate) fn preset(spt: u16) -> Option<&'static str> {
    Some(match spt {
        1 | 202 => "rect",
        2 => "roundRect",
        3 => "ellipse",
        4 => "diamond",
        5 => "triangle",
        6 => "rtTriangle",
        7 => "parallelogram",
        8 => "trapezoid",
        9 => "hexagon",
        10 => "octagon",
        11 => "plus",
        12 => "star5",
        13 | 14 => "rightArrow",
        15 => "homePlate",
        16 => "cube",
        17 => "wedgeRoundRectCallout",
        18 => "star16",
        19 => "arc",
        20 => "line",
        21 => "plaque",
        22 => "can",
        23 => "donut",
        32 => "straightConnector1",
        33 => "bentConnector2",
        34 => "bentConnector3",
        35 => "bentConnector4",
        36 => "bentConnector5",
        37 => "curvedConnector2",
        38 => "curvedConnector3",
        39 => "curvedConnector4",
        40 => "curvedConnector5",
        41 => "callout1",
        42 => "callout2",
        43 => "callout3",
        44 => "accentCallout1",
        45 => "accentCallout2",
        46 => "accentCallout3",
        47 => "borderCallout1",
        48 => "borderCallout2",
        49 => "borderCallout3",
        50 => "accentBorderCallout1",
        51 => "accentBorderCallout2",
        52 => "accentBorderCallout3",
        53 => "ribbon2",
        54 => "ribbon",
        55 => "chevron",
        56 => "pentagon",
        57 => "noSmoking",
        58 => "star8",
        59 => "star16",
        60 => "star32",
        61 => "wedgeRectCallout",
        62 => "wedgeRoundRectCallout",
        63 => "wedgeEllipseCallout",
        64 => "wave",
        65 => "foldedCorner",
        66 => "leftArrow",
        67 => "downArrow",
        68 => "upArrow",
        69 => "leftRightArrow",
        70 => "upDownArrow",
        71 => "irregularSeal1",
        72 => "irregularSeal2",
        73 => "lightningBolt",
        74 => "heart",
        75 => "rect",
        76 => "quadArrow",
        77 => "leftArrowCallout",
        78 => "rightArrowCallout",
        79 => "upArrowCallout",
        80 => "downArrowCallout",
        81 => "leftRightArrowCallout",
        82 => "upDownArrowCallout",
        83 => "quadArrowCallout",
        84 => "bevel",
        85 => "leftBracket",
        86 => "rightBracket",
        87 => "leftBrace",
        88 => "rightBrace",
        89 => "leftUpArrow",
        90 => "bentUpArrow",
        91 => "bentArrow",
        92 => "star24",
        93 => "stripedRightArrow",
        94 => "notchedRightArrow",
        95 => "blockArc",
        96 => "smileyFace",
        97 => "verticalScroll",
        98 => "horizontalScroll",
        99 | 100 => "circularArrow",
        101 => "uturnArrow",
        102 => "curvedRightArrow",
        103 => "curvedLeftArrow",
        104 => "curvedUpArrow",
        105 => "curvedDownArrow",
        106 => "cloudCallout",
        107 => "ellipseRibbon2",
        108 => "ellipseRibbon",
        109 => "flowChartProcess",
        110 => "flowChartDecision",
        111 => "flowChartInputOutput",
        112 => "flowChartPredefinedProcess",
        113 => "flowChartInternalStorage",
        114 => "flowChartDocument",
        115 => "flowChartMultidocument",
        116 => "flowChartTerminator",
        117 => "flowChartPreparation",
        118 => "flowChartManualInput",
        119 => "flowChartManualOperation",
        120 => "flowChartConnector",
        121 => "flowChartPunchedCard",
        122 => "flowChartPunchedTape",
        123 => "flowChartSummingJunction",
        124 => "flowChartOr",
        125 => "flowChartCollate",
        126 => "flowChartSort",
        127 => "flowChartExtract",
        128 => "flowChartMerge",
        129 => "flowChartOfflineStorage",
        130 => "flowChartOnlineStorage",
        131 => "flowChartMagneticTape",
        132 => "flowChartMagneticDisk",
        133 => "flowChartMagneticDrum",
        134 => "flowChartDisplay",
        135 => "flowChartDelay",
        // MS-ODRAW 2.4.24 MSOSPT, after the WordArt kinds (136 to 175, no preset of their own).
        176 => "flowChartAlternateProcess",
        177 => "flowChartOffpageConnector",
        182 => "leftRightUpArrow",
        183 => "sun",
        184 => "moon",
        185 => "bracketPair",
        186 => "bracePair",
        187 => "star4",
        188 => "doubleWave",
        189 => "actionButtonBlank",
        190 => "actionButtonHome",
        191 => "actionButtonHelp",
        192 => "actionButtonInformation",
        193 => "actionButtonForwardNext",
        194 => "actionButtonBackPrevious",
        195 => "actionButtonEnd",
        196 => "actionButtonBeginning",
        197 => "actionButtonReturn",
        198 => "actionButtonDocument",
        199 => "actionButtonSound",
        200 => "actionButtonMovie",
        _ => return None,
    })
}

/// An `IMsoArray` of the property `id` as its elements (each `cb` bytes), or `None`.
fn ms_array(props: &Props, id: u16) -> Option<(usize, Vec<&[u8]>)> {
    let d = props.complex(id)?;
    let n = u16::from_le_bytes([*d.first()?, *d.get(1)?]) as usize;
    let cb = u16::from_le_bytes([*d.get(4)?, *d.get(5)?]);
    let size = if cb == 0xFFF0 { 4 } else { cb as usize };
    if size == 0 {
        return None;
    }
    let items: Vec<&[u8]> = (0..n).filter_map(|i| d.get(6 + i * size..6 + (i + 1) * size)).collect();
    Some((size, items))
}

/// A freeform's geometry (`a:custGeom`) from the vertices and segments of its properties (MS-ODRAW 2.3.6).
pub(crate) fn custom_geometry(p: &Props) -> Option<String> {
    let (size, items) = ms_array(p, 0x145)?;
    let points: Vec<(i64, i64)> = items
        .iter()
        .map(|b| {
            if size >= 8 {
                let v = |at: usize| i64::from(i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]));
                (v(0), v(4))
            } else {
                (i64::from(i16::from_le_bytes([b[0], b[1]])), i64::from(i16::from_le_bytes([b[2], b[3]])))
            }
        })
        .map(|(x, y)| (if (0x8000_0000u32 as i64..=0x8000_007Fu32 as i64).contains(&(x & 0xFFFF_FFFF)) { 0 } else { x }, if (0x8000_0000u32 as i64..=0x8000_007Fu32 as i64).contains(&(y & 0xFFFF_FFFF)) { 0 } else { y }))
        .collect();
    if points.is_empty() {
        return None;
    }
    // The space of the points: the geometry rectangle, else 21600 square as the file's default.
    let mut left = p.int(0x140).map_or(0, i64::from);
    let mut top = p.int(0x141).map_or(0, i64::from);
    let mut right = p.int(0x142).map_or(21600, i64::from);
    let mut bottom = p.int(0x143).map_or(21600, i64::from);
    if right <= left || bottom <= top {
        // (A geometry rectangle that is empty: the vertices' own.)
        left = points.iter().map(|q| q.0).min().unwrap_or(0);
        right = points.iter().map(|q| q.0).max().unwrap_or(1);
        top = points.iter().map(|q| q.1).min().unwrap_or(0);
        bottom = points.iter().map(|q| q.1).max().unwrap_or(1);
    }
    let (w, h) = ((right - left).max(1), (bottom - top).max(1));
    let pt = |(x, y): (i64, i64)| format!(r#"<a:pt x="{}" y="{}"/>"#, x - left, y - top);
    let mut path = String::new();
    let mut at = 0usize;
    let take = |at: &mut usize, n: usize| -> Vec<(i64, i64)> {
        let out = points.get(*at..(*at + n).min(points.len())).unwrap_or(&[]).to_vec();
        *at += n;
        out
    };
    match ms_array(p, 0x146) {
        Some((_, segments)) if !segments.is_empty() => {
            for seg in segments {
                let v = u16::from_le_bytes([seg[0], *seg.get(1).unwrap_or(&0)]);
                let (kind, count) = (v >> 13, (v & 0x1FFF) as usize);
                match kind {
                    0 => {
                        for q in take(&mut at, count) {
                            let _ = write!(path, "<a:lnTo>{}</a:lnTo>", pt(q));
                        }
                    }
                    1 => {
                        for _ in 0..count {
                            let c = take(&mut at, 3);
                            if c.len() == 3 {
                                let _ = write!(path, "<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(c[0]), pt(c[1]), pt(c[2]));
                            }
                        }
                    }
                    2 => {
                        if let Some(q) = take(&mut at, 1).first() {
                            let _ = write!(path, "<a:moveTo>{}</a:moveTo>", pt(*q));
                        }
                    }
                    3 => path += "<a:close/>",
                    5 | 6 => {
                        // An escape: its points are skipped (the arcs and ellipses are not drawn).
                        at += (v & 0xFF) as usize;
                    }
                    _ => {}
                }
            }
        }
        _ => {
            let shape_path = p.get(0x144).unwrap_or(1);
            let all = take(&mut at, points.len());
            if let Some(first) = all.first() {
                let _ = write!(path, "<a:moveTo>{}</a:moveTo>", pt(*first));
                if shape_path == 2 || shape_path == 3 {
                    for c in all[1..].chunks_exact(3) {
                        let _ = write!(path, "<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(c[0]), pt(c[1]), pt(c[2]));
                    }
                } else {
                    for q in &all[1..] {
                        let _ = write!(path, "<a:lnTo>{}</a:lnTo>", pt(*q));
                    }
                }
                if shape_path == 1 || shape_path == 3 {
                    path += "<a:close/>";
                }
            }
        }
    }
    Some(format!(r#"<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/><a:pathLst><a:path w="{w}" h="{h}">{path}</a:path></a:pathLst></a:custGeom>"#))
}

/// The fill of a shape as `spPr` content.
pub(crate) fn fill_xml(ctx: &mut dyn ArtContext, p: &Props, has_text_or_placeholder: bool) -> String {
    let d = ctx.defaults().clone();
    let d = &d;
    let get = |id: u16| p.get(id).or_else(|| d.get(id));
    let filled = p.flag(0x1BF, 4).or_else(|| d.flag(0x1BF, 4)).unwrap_or(!has_text_or_placeholder);
    if !filled {
        return "<a:noFill/>".into();
    }
    let fill_type = get(0x180).unwrap_or(0);
    let alpha = get(0x182).map(|v| (f64::from(v) / 65536.0 * 100_000.0).round() as u32).filter(|a| *a < 100_000);
    let alpha_xml = alpha.map(|a| format!(r#"<a:alpha val="{a}"/>"#)).unwrap_or_default();
    match fill_type {
        // A picture fill.
        2 | 3 => {
            if let Some(index) = p.get(0x186).map(|v| v as usize).filter(|i| *i > 0) {
                if let Some(rid) = ctx.picture(index) {
                    return format!(r#"<a:blipFill rotWithShape="1"><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"#);
                }
            }
            "<a:noFill/>".into()
        }
        // A gradient.
        4..=8 => {
            let first = ctx.color(get(0x181).unwrap_or(0xFFFFFF));
            let second = ctx.color(get(0x183).unwrap_or(0xFFFFFF));
            let angle = get(0x18B).map_or(0.0, |v| f64::from(v as i32) / 65536.0);
            // The file's angle is of the gradient on the page: with the shape turned it is the preset's angle (clockwise from the
            // right) taken from a right angle, less the shape's turn.
            let turn = p.get(0x04).map_or(0.0, |v| f64::from(v as i32) / 65536.0);
            let ooxml = ((90.0 - angle - turn).rem_euclid(360.0) * 60000.0).round() as i64;
            // fillFocus ([MS-ODRAWXML]/[MS-ODRAW] 2.3.7.12, percent): the position of the last colour in the shade. 100 (and 0) is the plain
            // two-colour ramp; between them the ramp reaches the last colour at that position and returns to the first colour at the end
            // (025's cyan-white-cyan bands, focus 50).
            let focus = get(0x18C).map_or(100, |v| v as i32);
            // fillShadeColors ([MS-ODRAW] 2.3.7.15): the stops themselves, each a colour (4 bytes) and a position (16.16 fixed).
            if let Some((8, stops)) = ms_array(p, 0x197).filter(|(_, s)| s.len() >= 2) {
                let mut list: Vec<(u32, u32)> = stops
                    .iter()
                    .map(|e| {
                        let c = ctx.color(u32::from_le_bytes([e[0], e[1], e[2], e[3]]));
                        let pos = (f64::from(u32::from_le_bytes([e[4], e[5], e[6], e[7]])) / 65536.0 * 100_000.0).round().clamp(0.0, 100_000.0) as u32;
                        (pos, c)
                    })
                    .collect();
                // The array may stop short of the end: the ramp then runs on to the shape's second colour (092's white-to-black arrow, whose
                // last stop is at 43%).
                if list.last().is_some_and(|(pos, _)| *pos < 100_000) {
                    list.push((100_000, second));
                }
                let gs: String = list
                    .iter()
                    .map(|(pos, c)| format!(r#"<a:gs pos="{pos}"><a:srgbClr val="{c:06X}">{alpha_xml}</a:srgbClr></a:gs>"#))
                    .collect();
                return format!(r#"<a:gradFill rotWithShape="1"><a:gsLst>{gs}</a:gsLst><a:lin ang="{ooxml}" scaled="0"/></a:gradFill>"#);
            }
            let mid = if focus > 0 && focus < 100 {
                format!(r#"<a:gs pos="{}"><a:srgbClr val="{second:06X}">{alpha_xml}</a:srgbClr></a:gs>"#, focus * 1000)
            } else {
                String::new()
            };
            let last = if mid.is_empty() { second } else { first };
            format!(
                r#"<a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:srgbClr val="{first:06X}">{alpha_xml}</a:srgbClr></a:gs>{mid}<a:gs pos="100000"><a:srgbClr val="{last:06X}">{alpha_xml}</a:srgbClr></a:gs></a:gsLst><a:lin ang="{ooxml}" scaled="0"/></a:gradFill>"#
            )
        }
        // msofillBackground ([MS-ODRAW] fillType 9): the shape is filled with the slide's background (054's label boxes: white over a white
        // slide; 148's master title over a gradient: the gradient, i.e. nothing drawn).
        9 => match ctx.flat_background() {
            Some(c) => format!(r#"<a:solidFill><a:srgbClr val="{c:06X}">{alpha_xml}</a:srgbClr></a:solidFill>"#),
            None => "<a:noFill/>".into(),
        },
        _ => {
            let c = ctx.color(get(0x181).unwrap_or(0xFFFFFF));
            format!(r#"<a:solidFill><a:srgbClr val="{c:06X}">{alpha_xml}</a:srgbClr></a:solidFill>"#)
        }
    }
}

/// The shadow of a shape (`a:effectLst`), when it has one (MS-ODRAW 2.3.13).
pub(crate) fn shadow_xml(ctx: &dyn ArtContext, p: &Props) -> String {
    let d = ctx.defaults();
    let get = |id: u16| p.get(id).or_else(|| d.get(id));
    if !p.flag(0x23F, 1).or_else(|| d.flag(0x23F, 1)).unwrap_or(false) {
        return String::new();
    }
    let (dx, dy) = (get(0x205).map_or(25400.0, |v| f64::from(v as i32)), get(0x206).map_or(25400.0, |v| f64::from(v as i32)));
    let dist = (dx * dx + dy * dy).sqrt().round() as i64;
    let dir = (dy.atan2(dx).to_degrees().rem_euclid(360.0) * 60000.0).round() as i64;
    let color = ctx.color(get(0x201).unwrap_or(0x808080));
    let alpha = get(0x204).map_or(100_000, |v| (f64::from(v) / 65536.0 * 100_000.0).round() as i64).clamp(0, 100_000);
    format!(r#"<a:effectLst><a:outerShdw blurRad="0" dist="{dist}" dir="{dir}" algn="tl" rotWithShape="0"><a:srgbClr val="{color:06X}"><a:alpha val="{alpha}"/></a:srgbClr></a:outerShdw></a:effectLst>"#)
}

/// The outline of a shape.
pub(crate) fn line_xml(ctx: &dyn ArtContext, p: &Props, default_on: bool) -> String {
    let d = ctx.defaults();
    let get = |id: u16| p.get(id).or_else(|| d.get(id));
    let on = p.flag(0x1FF, 3).or_else(|| d.flag(0x1FF, 3)).unwrap_or(default_on);
    if !on {
        return "<a:ln><a:noFill/></a:ln>".into();
    }
    let width = get(0x1CB).map_or(9525, |v| i64::from(v as i32));
    let color = ctx.color(get(0x1C0).unwrap_or(0));
    let dash = match get(0x1CE).unwrap_or(0) {
        1 => "sysDash",
        2 => "sysDot",
        3 => "dashDot",
        4 => "lgDashDotDot",
        5 => "dash",
        6 => "lgDash",
        7 => "lgDashDot",
        8 => "lgDashDotDot",
        _ => "solid",
    };
    let arrow = |id: u16, tag: &str| -> String {
        let kind = match get(id).unwrap_or(0) {
            1 => "triangle",
            2 => "stealth",
            3 => "diamond",
            4 => "oval",
            5 => "arrow",
            _ => return String::new(),
        };
        format!(r#"<a:{tag} type="{kind}"/>"#)
    };
    format!(
        r#"<a:ln w="{width}"><a:solidFill><a:srgbClr val="{color:06X}"/></a:solidFill><a:prstDash val="{dash}"/>{}{}</a:ln>"#,
        arrow(0x1D0, "headEnd"),
        arrow(0x1D1, "tailEnd")
    )
}


#[cfg(test)]
mod tests {
    #[test]
    fn every_shape_type_names_a_preset_the_program_draws() {
        // Corpus 002 (.xls): its bracket pairs (185) were rectangles.
        assert_eq!(super::preset(185), Some("bracketPair"));
        for spt in 0..=202u16 {
            if let Some(name) = super::preset(spt) {
                assert!(crate::geometry::preset(name).is_some(), "{spt}: {name}");
            }
        }
    }
}
