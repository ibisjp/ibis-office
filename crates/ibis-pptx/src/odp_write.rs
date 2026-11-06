// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Writing a presentation as an OpenDocument presentation (`.odp`, ODF 1.3), from the specification: the slides with their
//! shapes (text boxes, preset and rounded shapes, lines, pictures, tables), text with its formatting, lists, fills (solid,
//! gradient, picture), lines (width, colour, dashes, arrows), shadows, backgrounds, the layouts as master pages, notes, hidden
//! slides and transitions. Charts, animations, comments and effects (glow, reflection, soft edges) are not written.

use std::collections::HashMap;
use std::fmt::Write as _;

use ibis_doc_model::text::{AutoNumber, Caps, LineSpacing, SlideLink, Strike, TextAlign, TextBody, TextField, Underline, VerticalAnchor};
use ibis_doc_model::{Animation, AnimationClass, AnimationLook, AnimationRepeat, AnimationSide, AnimationStart, Fill, Geometry, LineDash, LineEnd, Presentation, Shape, ShapeKind, Slide, Stroke};

use crate::Error;
use crate::odp_text::esc;

const MIMETYPE: &str = "application/vnd.oasis.opendocument.presentation";

fn cm(v: i64) -> String {
    let s = format!("{:.3}", v as f64 / 360_000.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{}cm", if s.is_empty() || s == "-" { "0" } else { s })
}

fn pt(hundredths: i64) -> String {
    let s = format!("{:.2}", hundredths as f64 / 100.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}pt")
}

fn hex(rgb: ibis_doc_model::Rgb) -> String {
    format!("#{:06x}", rgb.0 & 0xFF_FFFF)
}

/// A style bucket: the automatic styles of `content.xml` or of `styles.xml`.
#[derive(Default)]
struct Bucket {
    xml: String,
    names: HashMap<String, String>,
}

struct W<'p> {
    p: &'p Presentation,
    buckets: [Bucket; 2],
    current: usize,
    counter: usize,
    /// Entries of `office:styles` (gradients, dashes, markers, fill images).
    named: String,
    named_names: HashMap<String, String>,
    fonts: std::collections::BTreeSet<String>,
    pictures: Vec<(String, Vec<u8>)>,
    picture_names: HashMap<u64, String>,
}

impl<'p> W<'p> {
    fn new(p: &'p Presentation) -> Self {
        W { p, buckets: [Bucket::default(), Bucket::default()], current: 0, counter: 0, named: String::new(), named_names: HashMap::new(), fonts: Default::default(), pictures: Vec::new(), picture_names: HashMap::new() }
    }

    /// The automatic style of a family with these properties (made once).
    fn style(&mut self, prefix: &str, family: &str, parent: Option<&str>, body: &str) -> String {
        let key = format!("{family}|{}|{body}", parent.unwrap_or(""));
        if let Some(n) = self.buckets[self.current].names.get(&key) {
            return n.clone();
        }
        self.counter += 1;
        let name = format!("{prefix}{}", self.counter);
        let parent = parent.map(|p| format!(r#" style:parent-style-name="{p}""#)).unwrap_or_default();
        let _ = write!(self.buckets[self.current].xml, r#"<style:style style:name="{name}" style:family="{family}"{parent}>{body}</style:style>"#);
        self.buckets[self.current].names.insert(key, name.clone());
        name
    }

    /// An entry of `office:styles` made once; `xml` has `{name}` where its name goes.
    fn named(&mut self, kind: &str, key: &str, xml: &str) -> String {
        let full = format!("{kind}|{key}");
        if let Some(n) = self.named_names.get(&full) {
            return n.clone();
        }
        self.counter += 1;
        let name = format!("{kind}{}", self.counter);
        self.named += &xml.replace("{name}", &name);
        self.named_names.insert(full, name.clone());
        name
    }

    fn picture(&mut self, media: ibis_doc_model::MediaId) -> Option<String> {
        if let Some(n) = self.picture_names.get(&media.0) {
            return Some(n.clone());
        }
        let m = self.p.media.iter().find(|m| m.id == media)?;
        let ext = match m.format {
            ibis_doc_model::MediaFormat::Png => "png",
            ibis_doc_model::MediaFormat::Jpeg => "jpg",
            ibis_doc_model::MediaFormat::Gif => "gif",
            ibis_doc_model::MediaFormat::Bmp => "bmp",
            ibis_doc_model::MediaFormat::Tiff => "tif",
            ibis_doc_model::MediaFormat::Svg => "svg",
            ibis_doc_model::MediaFormat::Emf => "emf",
            ibis_doc_model::MediaFormat::Wmf => "wmf",
            _ => return None,
        };
        let name = format!("Pictures/image{}.{ext}", self.pictures.len() + 1);
        self.pictures.push((name.clone(), m.data.to_vec()));
        self.picture_names.insert(media.0, name.clone());
        Some(name)
    }

    fn font(&mut self, name: &str) -> String {
        self.fonts.insert(name.to_string());
        esc(name)
    }
}

fn dash_def(dash: LineDash) -> Option<(&'static str, &'static str)> {
    // The pattern as ODF writes it: dots, their length and the distance, in the line's widths.
    Some(match dash {
        LineDash::Solid => return None,
        LineDash::Dash => ("Dash", r#"draw:dots1="1" draw:dots1-length="400%" draw:distance="300%""#),
        LineDash::LgDash => ("Long Dash", r#"draw:dots1="1" draw:dots1-length="800%" draw:distance="300%""#),
        LineDash::SysDash => ("Sys Dash", r#"draw:dots1="1" draw:dots1-length="300%" draw:distance="100%""#),
        LineDash::Dot | LineDash::SysDot => ("Dot", r#"draw:dots1="1" draw:dots1-length="100%" draw:distance="100%""#),
        LineDash::DashDot | LineDash::SysDashDot => ("Dash Dot", r#"draw:dots1="1" draw:dots1-length="400%" draw:dots2="1" draw:dots2-length="100%" draw:distance="300%""#),
        LineDash::LgDashDot => ("Long Dash Dot", r#"draw:dots1="1" draw:dots1-length="800%" draw:dots2="1" draw:dots2-length="100%" draw:distance="300%""#),
        LineDash::LgDashDotDot | LineDash::SysDashDotDot => ("Dash Dot Dot", r#"draw:dots1="1" draw:dots1-length="400%" draw:dots2="2" draw:dots2-length="100%" draw:distance="300%""#),
    })
}

fn marker_def(end: LineEnd) -> Option<(&'static str, &'static str)> {
    Some(match end {
        LineEnd::None => return None,
        LineEnd::Triangle(_) | LineEnd::Stealth(_) | LineEnd::Arrow(_) => ("Arrow", r#"svg:viewBox="0 0 20 30" svg:d="m10 0-10 30h20z""#),
        LineEnd::Diamond(_) => ("Square", r#"svg:viewBox="0 0 1131 1131" svg:d="m0 564 564 564 563-564-563-564z""#),
        LineEnd::Oval(_) => ("Circle", r#"svg:viewBox="0 0 1131 1131" svg:d="m462 1118-102-29-102-51-93-72-72-93-51-102-29-102-13-105 13-102 29-106 51-102 72-89 93-72 102-50 102-34 106-9 101 9 106 34 98 50 93 72 72 89 51 102 29 106 13 102-13 105-29 102-51 102-72 93-93 72-98 51-106 29-101 13z""#),
    })
}

fn alpha_pct(alpha: u8) -> String {
    format!("{}%", (f64::from(alpha) / 255.0 * 100.0).round())
}

/// The graphic properties of a fill (and the named objects it needs).
fn fill_props(w: &mut W, fill: &Fill, rotation_deg: f64) -> String {
    match fill {
        Fill::None => r#"draw:fill="none""#.to_string(),
        Fill::Solid(c) => {
            let mut s = format!(r##"draw:fill="solid" draw:fill-color="{}""##, hex(c.rgb));
            if c.alpha < 255 {
                let _ = write!(s, r#" draw:opacity="{}""#, alpha_pct(c.alpha));
            }
            s
        }
        Fill::Linear { angle, stops, .. } | Fill::TileLinear { angle, stops, .. } if stops.len() >= 2 => {
            let first = stops.first().expect("two stops");
            let last = stops.last().expect("two stops");
            // The angle is on the page in ODF (counterclockwise); in DrawingML it turns with the shape (clockwise).
            let odf = (90.0 - f64::from(*angle) / 60_000.0 - rotation_deg).rem_euclid(360.0);
            let mut xml = format!(
                r##"<draw:gradient draw:name="{{name}}" draw:style="linear" draw:start-color="{}" draw:end-color="{}" draw:start-intensity="100%" draw:end-intensity="100%" draw:angle="{}deg" draw:border="0%">"##,
                hex(first.color.rgb),
                hex(last.color.rgb),
                odf.round()
            );
            if stops.len() > 2 {
                for s in stops {
                    let _ = write!(xml, r##"<loext:gradient-stop svg:offset="{}" svg:stop-color="{}"/>"##, f64::from(s.position) / 100_000.0, hex(s.color.rgb));
                }
            }
            xml += "</draw:gradient>";
            let key = format!("{odf}|{stops:?}");
            let name = w.named("Gradient", &key, &xml);
            format!(r#"draw:fill="gradient" draw:fill-gradient-name="{name}""#)
        }
        Fill::Linear { stops, .. } | Fill::TileLinear { stops, .. } => match stops.first() {
            Some(s) => format!(r##"draw:fill="solid" draw:fill-color="{}""##, hex(s.color.rgb)),
            None => r#"draw:fill="none""#.to_string(),
        },
        Fill::Image(_) | Fill::TileImage { .. } | Fill::Blip(_) => match fill.media().and_then(|media| w.picture(media)) {
            Some(href) => {
                let xml = format!(r#"<draw:fill-image draw:name="{{name}}" xlink:href="{href}" xlink:type="simple" xlink:show="embed" xlink:actuate="onLoad"/>"#);
                let name = w.named("Bitmap", &href, &xml);
                let style = if matches!(fill, Fill::TileImage { .. }) { "repeat" } else { "stretch" };
                format!(r#"draw:fill="bitmap" draw:fill-image-name="{name}" style:repeat="{style}" draw:fill-image-rendering-style="{style}""#)
            }
            None => r#"draw:fill="none""#.to_string(),
        },
        _ => r#"draw:fill="none""#.to_string(),
    }
}

fn stroke_props(w: &mut W, stroke: &Option<Stroke>) -> String {
    let Some(s) = stroke else { return r#"draw:stroke="none""#.to_string() };
    let mut out = String::new();
    let dash = if s.dashed && s.dash == LineDash::Solid { LineDash::Dash } else { s.dash };
    match dash_def(dash) {
        Some((name, attrs)) => {
            let xml = format!(r#"<draw:stroke-dash draw:name="{{name}}" draw:style="rect" {attrs}/>"#);
            let n = w.named("Dash", name, &xml);
            let _ = write!(out, r#"draw:stroke="dash" draw:stroke-dash="{n}""#);
        }
        None => out += r#"draw:stroke="solid""#,
    }
    let _ = write!(out, r##" svg:stroke-width="{}" svg:stroke-color="{}""##, cm(s.width), hex(s.color.rgb));
    if s.color.alpha < 255 {
        let _ = write!(out, r#" svg:stroke-opacity="{}""#, alpha_pct(s.color.alpha));
    }
    match s.cap {
        ibis_doc_model::LineCap::Round => out += r#" svg:stroke-linecap="round""#,
        ibis_doc_model::LineCap::Square => out += r#" svg:stroke-linecap="square""#,
        ibis_doc_model::LineCap::Flat => {}
    }
    match s.join {
        ibis_doc_model::LineJoin::Round => out += r#" draw:stroke-linejoin="round""#,
        ibis_doc_model::LineJoin::Bevel => out += r#" draw:stroke-linejoin="bevel""#,
        ibis_doc_model::LineJoin::Miter => out += r#" draw:stroke-linejoin="miter""#,
    }
    for (end, kind) in [(s.head, "start"), (s.tail, "end")] {
        if let Some((name, attrs)) = marker_def(end) {
            let xml = format!(r#"<draw:marker draw:name="{{name}}" {attrs}/>"#);
            let n = w.named("Marker", name, &xml);
            let _ = write!(out, r#" draw:marker-{kind}="{n}" draw:marker-{kind}-width="{}""#, cm((s.width * 3).max(180_000)));
        }
    }
    out
}

fn shadow_props(shadow: &Option<ibis_doc_model::Shadow>) -> String {
    let Some(s) = shadow else { return String::new() };
    let angle = f64::from(s.direction) / 60_000.0;
    let (dx, dy) = ((s.distance as f64 * angle.to_radians().cos()).round() as i64, (s.distance as f64 * angle.to_radians().sin()).round() as i64);
    let mut out = format!(r##" draw:shadow="visible" draw:shadow-offset-x="{}" draw:shadow-offset-y="{}" draw:shadow-color="{}""##, cm(dx), cm(dy), hex(s.color.rgb));
    if s.color.alpha < 255 {
        let _ = write!(out, r#" draw:shadow-opacity="{}""#, alpha_pct(s.color.alpha));
    }
    out
}

fn text_area_props(body: Option<&TextBody>) -> String {
    let Some(b) = body else { return String::new() };
    let mut s = format!(
        r#" fo:padding-left="{}" fo:padding-top="{}" fo:padding-right="{}" fo:padding-bottom="{}" draw:textarea-vertical-align="{}" fo:wrap-option="{}""#,
        cm(b.insets.left),
        cm(b.insets.top),
        cm(b.insets.right),
        cm(b.insets.bottom),
        match b.anchor {
            VerticalAnchor::Top => "top",
            VerticalAnchor::Middle => "middle",
            VerticalAnchor::Bottom => "bottom",
        },
        if b.wrap { "wrap" } else { "no-wrap" }
    );
    match b.autofit {
        ibis_doc_model::text::TextAutofit::Shrink => s += r#" style:shrink-to-fit="true" draw:auto-grow-height="false""#,
        ibis_doc_model::text::TextAutofit::ResizeShape => s += r#" draw:auto-grow-height="true""#,
        ibis_doc_model::text::TextAutofit::None => s += r#" draw:auto-grow-height="false""#,
    }
    if b.vertical {
        s += r#" style:writing-mode="tb-rl""#;
    }
    s
}

fn transform(shape: &Shape) -> String {
    let f = shape.frame;
    if shape.rotation == 0 {
        return String::new();
    }
    // OpenDocument turns the object about its own corner and then moves it; the shape here turns about its centre.
    let angle = -(f64::from(shape.rotation) / 60_000.0).to_radians();
    let (cos, sin) = (angle.cos(), angle.sin());
    let (hw, hh) = (f.width as f64 / 2.0, f.height as f64 / 2.0);
    let (cx, cy) = (f.x as f64 + hw, f.y as f64 + hh);
    let tx = cx - (hw * cos + hh * sin);
    let ty = cy - (-hw * sin + hh * cos);
    format!(r#" draw:transform="rotate ({angle:.6}) translate ({} {})""#, cm(tx.round() as i64), cm(ty.round() as i64))
}

fn frame_attrs(shape: &Shape) -> String {
    let f = shape.frame;
    if shape.rotation == 0 {
        format!(r#"svg:width="{}" svg:height="{}" svg:x="{}" svg:y="{}""#, cm(f.width), cm(f.height), cm(f.x), cm(f.y))
    } else {
        format!(r#"svg:width="{}" svg:height="{}"{}"#, cm(f.width), cm(f.height), transform(shape))
    }
}

/// The shape's outline as an enhanced path in 1/100 mm (360 EMU): `M`, `L`, `C`, `Z` and `N`.
fn outline_path(geometry: &Geometry, width: i64, height: i64) -> Option<(String, i64, i64)> {
    let (w, h) = ((width / 360).max(1), (height / 360).max(1));
    let (wf, hf) = (w as f64, h as f64);
    let n = |v: f64| v.round() as i64;
    let mut d = String::new();
    match geometry {
        Geometry::Rect => {
            let _ = write!(d, "M 0 0 L {w} 0 {w} {h} 0 {h} Z N");
        }
        Geometry::Ellipse => {
            let k = 0.552_284_75;
            let (cx, cy, rx, ry) = (wf / 2.0, hf / 2.0, wf / 2.0, hf / 2.0);
            let _ = write!(
                d,
                "M {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} Z N",
                n(cx + rx), n(cy),
                n(cx + rx), n(cy + ry * k), n(cx + rx * k), n(cy + ry), n(cx), n(cy + ry),
                n(cx - rx * k), n(cy + ry), n(cx - rx), n(cy + ry * k), n(cx - rx), n(cy),
                n(cx - rx), n(cy - ry * k), n(cx - rx * k), n(cy - ry), n(cx), n(cy - ry),
                n(cx + rx * k), n(cy - ry), n(cx + rx), n(cy - ry * k), n(cx + rx), n(cy)
            );
        }
        Geometry::RoundRect { radius } => {
            let r = (wf.min(hf) * f64::from(*radius) / 100_000.0).min(wf.min(hf) / 2.0);
            let k = r * 0.552_284_75;
            let _ = write!(
                d,
                "M {} 0 L {} 0 C {} 0 {w} {} {w} {} L {w} {} C {w} {} {} {h} {} {h} L {} {h} C {} {h} 0 {} 0 {} L 0 {} C 0 {} {} 0 {} 0 Z N",
                n(r), n(wf - r), n(wf - r + k), n(r - k), n(r), n(hf - r), n(hf - r + k), n(wf - r + k), n(wf - r), n(r), n(r - k), n(hf - r + k), n(hf - r), n(r), n(r - k), n(r), n(r)
            );
            // (The corners: quarter curves; the numbers above are in the order of the path.)
        }
        Geometry::Formula(f) => {
            let outlines = f.outline(width as f64, height as f64);
            for outline in &outlines {
                for sub in &outline.subpaths {
                    let mut points = sub.points.iter();
                    let Some(first) = points.next() else { continue };
                    let _ = write!(d, "M {} {}", n(first.0 / 360.0), n(first.1 / 360.0));
                    for p in points {
                        let _ = write!(d, " L {} {}", n(p.0 / 360.0), n(p.1 / 360.0));
                    }
                    d += if sub.closed { " Z N " } else { " N " };
                }
            }
            if d.is_empty() {
                return None;
            }
        }
    }
    Some((d.trim().to_string(), w, h))
}

fn run_props(w: &mut W, style: &ibis_doc_model::text::RunStyle, base: &ibis_doc_model::text::TextStyle) -> String {
    let mut s = String::new();
    let size = style.font_size.unwrap_or(base.font_size);
    let _ = write!(s, r#" fo:font-size="{0}" style:font-size-asian="{0}" style:font-size-complex="{0}""#, pt(i64::from(size)));
    if style.bold.unwrap_or(base.bold) {
        s += r#" fo:font-weight="bold" style:font-weight-asian="bold" style:font-weight-complex="bold""#;
    }
    if style.italic.unwrap_or(base.italic) {
        s += r#" fo:font-style="italic" style:font-style-asian="italic" style:font-style-complex="italic""#;
    }
    let color = style.color.unwrap_or(base.color);
    let _ = write!(s, r#" fo:color="{}""#, hex(color));
    if let Some(font) = style.font.as_ref().or(base.font.as_ref()) {
        let f = w.font(font);
        let _ = write!(s, r#" style:font-name="{f}" style:font-name-asian="{f}" style:font-name-complex="{f}""#);
    }
    if style.underline {
        let (ustyle, extra) = match style.underline_kind {
            Underline::Double => ("solid", r#" style:text-underline-type="double""#),
            Underline::Heavy => ("solid", r#" style:text-underline-width="bold""#),
            Underline::Dotted => ("dotted", ""),
            Underline::DottedHeavy => ("dotted", r#" style:text-underline-width="bold""#),
            Underline::Dash => ("dash", ""),
            Underline::DashHeavy => ("dash", r#" style:text-underline-width="bold""#),
            _ => ("solid", ""),
        };
        let _ = write!(s, r#" style:text-underline-style="{ustyle}"{extra} style:text-underline-color="font-color""#);
    }
    match style.strike {
        Strike::Single => s += r#" style:text-line-through-style="solid""#,
        Strike::Double => s += r#" style:text-line-through-style="solid" style:text-line-through-type="double""#,
        Strike::None => {}
    }
    match style.caps {
        Caps::All => s += r#" fo:text-transform="uppercase""#,
        Caps::Small => s += r#" fo:font-variant="small-caps""#,
        Caps::None => {}
    }
    if style.baseline != 0 {
        let _ = write!(s, r#" style:text-position="{}% 58%""#, f64::from(style.baseline) / 1000.0);
    }
    if style.spacing != 0 {
        let _ = write!(s, r#" fo:letter-spacing="{}""#, pt(i64::from(style.spacing)));
    }
    if let Some(h) = style.highlight {
        let _ = write!(s, r#" fo:background-color="{}""#, hex(h));
    }
    if let Some(lang) = &style.lang {
        if let Some((l, c)) = lang.split_once('-') {
            let _ = write!(s, r#" fo:language="{l}" fo:country="{c}""#);
        } else {
            let _ = write!(s, r#" fo:language="{lang}""#);
        }
    }
    format!("<style:text-properties{s}/>")
}

fn list_style(w: &mut W, bullet: &ibis_doc_model::text::Bullet, margin: i64, indent: i64) -> String {
    // The bullet sits `-indent` before the text of a hanging paragraph.
    let label = (-indent).max(0);
    let before = if indent < 0 { (margin - label).max(0) } else { margin + indent };
    let props = format!(r#"<style:list-level-properties text:space-before="{}" text:min-label-width="{}"/>"#, cm(before), cm(label));
    let color = bullet.color.map(|c| format!(r#" fo:color="{}""#, hex(c))).unwrap_or_default();
    let size = format!(r#" fo:font-size="{}%""#, (f64::from(bullet.size) / 1000.0).round().clamp(25.0, 400.0));
    let body = match &bullet.auto {
        Some(AutoNumber { scheme, start_at }) => {
            let (format, rest) = if let Some(r) = scheme.strip_prefix("arabic") {
                ("1", r)
            } else if let Some(r) = scheme.strip_prefix("alphaLc") {
                ("a", r)
            } else if let Some(r) = scheme.strip_prefix("alphaUc") {
                ("A", r)
            } else if let Some(r) = scheme.strip_prefix("romanLc") {
                ("i", r)
            } else if let Some(r) = scheme.strip_prefix("romanUc") {
                ("I", r)
            } else {
                ("1", "Period")
            };
            let (prefix, suffix) = match rest {
                "ParenBoth" => ("(", ")"),
                "ParenR" => ("", ")"),
                "Plain" => ("", ""),
                _ => ("", "."),
            };
            let start = if *start_at > 1 { format!(r#" text:start-value="{start_at}""#) } else { String::new() };
            format!(r#"<text:list-level-style-number text:level="1" style:num-format="{format}" style:num-prefix="{prefix}" style:num-suffix="{suffix}"{start}>{props}<style:text-properties{color}{size}/></text:list-level-style-number>"#)
        }
        None => {
            let ch: String = bullet.text.chars().next().map(String::from).unwrap_or_else(|| "•".to_string());
            format!(r#"<text:list-level-style-bullet text:level="1" text:bullet-char="{}">{props}<style:text-properties{color}{size}/></text:list-level-style-bullet>"#, esc(&ch))
        }
    };
    let key = body.clone();
    let xml = format!(r#"<text:list-style style:name="{{name}}">{body}</text:list-style>"#);
    // A list style is an automatic style: written to the bucket in use.
    if let Some(n) = w.buckets[w.current].names.get(&key) {
        return n.clone();
    }
    w.counter += 1;
    let name = format!("L{}", w.counter);
    w.buckets[w.current].xml += &xml.replace("{name}", &name);
    w.buckets[w.current].names.insert(key, name.clone());
    name
}

fn paragraphs_xml(w: &mut W, body: &TextBody, out: &mut String) {
    let base = &body.style;
    let mut open_list: Option<String> = None;
    for para in &body.paragraphs {
        let bullet = para.bullet.as_ref();
        let list = bullet.map(|b| list_style(w, b, para.margin_left, para.indent));
        if open_list != list {
            if open_list.is_some() {
                *out += "</text:list>";
            }
            if let Some(l) = &list {
                let _ = write!(out, r#"<text:list text:style-name="{l}">"#);
            }
            open_list = list.clone();
        }
        let align = match para.align.unwrap_or(base.align) {
            TextAlign::Left => "start",
            TextAlign::Center => "center",
            TextAlign::Right => "end",
            TextAlign::Justify | TextAlign::Distributed => "justify",
        };
        let mut p = format!(r#"<style:paragraph-properties fo:text-align="{align}""#);
        if list.is_none() {
            let _ = write!(p, r#" fo:margin-left="{}" fo:text-indent="{}""#, cm(para.margin_left), cm(para.indent));
        } else {
            p += r#" fo:margin-left="0cm" fo:text-indent="0cm""#;
        }
        let _ = write!(p, r#" fo:margin-top="{}" fo:margin-bottom="{}""#, pt(i64::from(para.space_before)), pt(i64::from(para.space_after)));
        match para.line_spacing {
            Some(LineSpacing::Percent(v)) => {
                let _ = write!(p, r#" fo:line-height="{}%""#, (f64::from(v) / 1000.0).round());
            }
            Some(LineSpacing::Points(v)) => {
                let _ = write!(p, r#" fo:line-height="{}""#, pt(i64::from(v)));
            }
            None => {}
        }
        p += "/>";
        let end_props = run_props(w, &para.end_style, base);
        let pstyle = w.style("P", "paragraph", None, &format!("{p}{end_props}"));
        let mut text = String::new();
        for run in &para.runs {
            let props = run_props(w, &run.style, base);
            let tstyle = w.style("T", "text", None, &props);
            let inner = match &run.style.field {
                Some(TextField::SlideNumber) => "<text:page-number>‹#›</text:page-number>".to_string(),
                Some(TextField::DateTime { .. }) => format!("<text:date>{}</text:date>", esc(&run.text)),
                None => text_runs(&run.text),
            };
            // A link is inside its span (LibreOffice takes a link's look from the span around it).
            match &run.style.link {
                Some(SlideLink::Url(url)) => {
                    let _ = write!(text, r#"<text:span text:style-name="{tstyle}"><text:a xlink:type="simple" xlink:href="{}">{inner}</text:a></text:span>"#, esc(url));
                }
                _ => {
                    let _ = write!(text, r#"<text:span text:style-name="{tstyle}">{inner}</text:span>"#);
                }
            }
        }
        if list.is_some() {
            let _ = write!(out, r#"<text:list-item><text:p text:style-name="{pstyle}">{text}</text:p></text:list-item>"#);
        } else {
            let _ = write!(out, r#"<text:p text:style-name="{pstyle}">{text}</text:p>"#);
        }
    }
    if open_list.is_some() {
        *out += "</text:list>";
    }
}

/// Text with ODF's markup for tabs, runs of blanks and line breaks.
fn text_runs(text: &str) -> String {
    let mut out = String::new();
    let mut blanks = 0;
    let flush = |out: &mut String, blanks: &mut usize| {
        if *blanks > 0 {
            if *blanks == 1 {
                out.push(' ');
            } else {
                let _ = write!(out, r#"<text:s text:c="{}"/>"#, blanks);
            }
            *blanks = 0;
        }
    };
    for (i, c) in text.chars().enumerate() {
        match c {
            ' ' => {
                // A single space inside the text stays one; the first of a run after another character too.
                blanks += 1;
                if i == 0 && blanks == 1 {
                    // (A leading blank must be a `text:s`.)
                    out += r#"<text:s/>"#;
                    blanks = 0;
                }
            }
            '\t' => {
                flush(&mut out, &mut blanks);
                out += "<text:tab/>";
            }
            '\n' | '\u{b}' => {
                flush(&mut out, &mut blanks);
                out += "<text:line-break/>";
            }
            '&' | '<' | '>' | '"' => {
                flush(&mut out, &mut blanks);
                out += &esc(&c.to_string());
            }
            c => {
                flush(&mut out, &mut blanks);
                out.push(c);
            }
        }
    }
    flush(&mut out, &mut blanks);
    out
}

fn name_attr(names: &HashMap<ibis_doc_model::ShapeId, String>, shape: &Shape) -> String {
    let id = format!(r#" draw:id="id{}""#, shape.id.0);
    id + &names.get(&shape.id).map(|n| format!(r#" draw:name="{}""#, esc(n))).unwrap_or_default()
}

fn shape_xml(w: &mut W, names: &HashMap<ibis_doc_model::ShapeId, String>, shape: &Shape, out: &mut String) {
    let name = name_attr(names, shape);
    let rotation_deg = f64::from(shape.rotation) / 60_000.0;
    match &shape.kind {
        ShapeKind::TextBox(body) => {
            let gp = format!(r#"draw:stroke="none" draw:fill="none"{}"#, text_area_props(Some(body)));
            let style = w.style("gr", "graphic", Some("standard"), &format!("<style:graphic-properties {gp}/>"));
            let mut inner = String::new();
            paragraphs_xml(w, body, &mut inner);
            let _ = write!(out, r#"<draw:frame{name} draw:style-name="{style}" {}><draw:text-box>{inner}</draw:text-box></draw:frame>"#, frame_attrs(shape));
        }
        ShapeKind::Auto(a) => {
            let mut gp = format!("{} {}{}", fill_props(w, &a.fill, rotation_deg), stroke_props(w, &a.stroke), shadow_props(&a.shadow));
            gp += &text_area_props(a.text.as_ref());
            let style = w.style("gr", "graphic", Some("standard"), &format!("<style:graphic-properties {gp}/>"));
            let (kind, modifiers) = match &a.geometry {
                Geometry::Rect => ("ooxml-rect".to_string(), String::new()),
                Geometry::RoundRect { radius } => ("ooxml-roundRect".to_string(), format!(r#" draw:modifiers="{radius}""#)),
                Geometry::Ellipse => ("ooxml-ellipse".to_string(), String::new()),
                Geometry::Formula(f) => match &f.preset {
                    Some(p) => {
                        let values: Vec<String> = f.adjust.iter().filter_map(|g| g.formula.strip_prefix("val ").map(|v| v.trim().to_string())).collect();
                        (format!("ooxml-{p}"), if values.is_empty() { String::new() } else { format!(r#" draw:modifiers="{}""#, values.join(" ")) })
                    }
                    None => ("ooxml-non-primitive".to_string(), String::new()),
                },
            };
            let mut inner = String::new();
            if let Some(b) = &a.text {
                paragraphs_xml(w, b, &mut inner);
            }
            let (mh, mv) = (shape.flip_h, shape.flip_v);
            // The outline as a path, for the programs that do not know the preset by its name.
            let (path, vw, vh) = outline_path(&a.geometry, shape.frame.width, shape.frame.height).unwrap_or_else(|| ("M 0 0 L 1 0 1 1 0 1 Z N".to_string(), 1, 1));
            let _ = write!(
                out,
                r#"<draw:custom-shape{name} draw:style-name="{style}" {}>{inner}<draw:enhanced-geometry svg:viewBox="0 0 {vw} {vh}" draw:mirror-horizontal="{mh}" draw:mirror-vertical="{mv}" draw:type="{kind}"{modifiers} draw:enhanced-path="{path}"/></draw:custom-shape>"#,
                frame_attrs(shape)
            );
        }
        ShapeKind::Line(stroke) => {
            let f = shape.frame;
            let (x1, x2) = if shape.flip_h { (f.x + f.width, f.x) } else { (f.x, f.x + f.width) };
            let (y1, y2) = if shape.flip_v { (f.y + f.height, f.y) } else { (f.y, f.y + f.height) };
            let gp = stroke_props(w, &Some(stroke.clone()));
            let style = w.style("gr", "graphic", Some("standard"), &format!("<style:graphic-properties {gp}/>"));
            let _ = write!(out, r#"<draw:line{name} draw:style-name="{style}" svg:x1="{}" svg:y1="{}" svg:x2="{}" svg:y2="{}"/>"#, cm(x1), cm(y1), cm(x2), cm(y2));
        }
        ShapeKind::Picture(pic) => {
            let Some(href) = w.picture(pic.media) else { return };
            let mut gp = format!("{} {}{}", r#"draw:fill="none""#, stroke_props(w, &pic.stroke), shadow_props(&pic.shadow));
            // The crop in ODF is in lengths of the picture as shown (fractions of the frame, here, of the cropped frame).
            let c = pic.crop;
            if c.left != 0 || c.top != 0 || c.right != 0 || c.bottom != 0 {
                let f = shape.frame;
                let (visible_w, visible_h) = (1.0 - (f64::from(c.left) + f64::from(c.right)) / 100_000.0, 1.0 - (f64::from(c.top) + f64::from(c.bottom)) / 100_000.0);
                let (full_w, full_h) = (f.width as f64 / visible_w.max(0.01), f.height as f64 / visible_h.max(0.01));
                let len = |frac: i32, full: f64| cm((f64::from(frac) / 100_000.0 * full).round() as i64);
                let _ = write!(gp, r#" fo:clip="rect({}, {}, {}, {})""#, len(c.top, full_h), len(c.right, full_w), len(c.bottom, full_h), len(c.left, full_w));
            }
            let style = w.style("gr", "graphic", Some("standard"), &format!("<style:graphic-properties {gp}/>"));
            let alt = if pic.alt_text.description.is_empty() { String::new() } else { format!("<svg:desc>{}</svg:desc>", esc(&pic.alt_text.description)) };
            let _ = write!(out, r#"<draw:frame{name} draw:style-name="{style}" {}><draw:image xlink:href="{href}" xlink:type="simple" xlink:show="embed" xlink:actuate="onLoad"/>{alt}</draw:frame>"#, frame_attrs(shape));
        }
        ShapeKind::Table(table) => {
            let mut cols = String::new();
            for width in &table.columns {
                let s = w.style("co", "table-column", None, &format!(r#"<style:table-column-properties style:column-width="{}"/>"#, cm(*width)));
                let _ = write!(cols, r#"<table:table-column table:style-name="{s}"/>"#);
            }
            let mut rows = String::new();
            for row in &table.rows {
                let rs = w.style("ro", "table-row", None, &format!(r#"<style:table-row-properties style:row-height="{}"/>"#, cm(row.height)));
                let mut cells = String::new();
                for cell in &row.cells {
                    let bg = match &cell.fill {
                        Fill::Solid(c) => format!(r#" fo:background-color="{}""#, hex(c.rgb)),
                        _ => String::new(),
                    };
                    let props = format!(
                        r#"<style:table-cell-properties fo:border-left="{}" fo:border-right="{}" fo:border-top="{}" fo:border-bottom="{}"{bg} fo:padding-left="{}" fo:padding-top="{}" fo:padding-right="{}" fo:padding-bottom="{}" style:vertical-align="{}"/>"#,
                        border(&cell.borders.left),
                        border(&cell.borders.right),
                        border(&cell.borders.top),
                        border(&cell.borders.bottom),
                        cm(cell.text.insets.left),
                        cm(cell.text.insets.top),
                        cm(cell.text.insets.right),
                        cm(cell.text.insets.bottom),
                        match cell.text.anchor {
                            VerticalAnchor::Top => "top",
                            VerticalAnchor::Middle => "middle",
                            VerticalAnchor::Bottom => "bottom",
                        }
                    );
                    let cs = w.style("ce", "table-cell", None, &props);
                    let tag = if cell.merged { "table:covered-table-cell" } else { "table:table-cell" };
                    let span = if cell.column_span > 1 || cell.row_span > 1 && !cell.merged { format!(r#" table:number-columns-spanned="{}" table:number-rows-spanned="{}""#, cell.column_span.max(1), cell.row_span.max(1)) } else { String::new() };
                    let mut inner = String::new();
                    paragraphs_xml(w, &cell.text, &mut inner);
                    let _ = write!(cells, r#"<{tag} table:style-name="{cs}"{span}>{inner}</{tag}>"#);
                }
                let _ = write!(rows, r#"<table:table-row table:style-name="{rs}">{cells}</table:table-row>"#);
            }
            let style = w.style("gr", "graphic", Some("standard"), r#"<style:graphic-properties draw:fill="none" draw:stroke="none"/>"#);
            let _ = write!(out, r#"<draw:frame{name} draw:style-name="{style}" {}><table:table>{cols}{rows}</table:table></draw:frame>"#, frame_attrs(shape));
        }
        _ => {}
    }
}

fn border(b: &Option<Stroke>) -> String {
    match b {
        Some(s) => format!("{} solid {}", pt((s.width as f64 / 127.0).round() as i64), hex(s.color.rgb)),
        None => "none".to_string(),
    }
}

fn background_style(w: &mut W, fill: &Fill) -> Option<String> {
    if matches!(fill, Fill::None) {
        return None;
    }
    let gp = fill_props(w, fill, 0.0);
    Some(w.style("dp", "drawing-page", None, &format!("<style:drawing-page-properties {gp} presentation:background-visible=\"true\" presentation:background-objects-visible=\"true\"/>")))
}

fn transition_props(slide: &Slide) -> String {
    let Some(t) = &slide.transition else { return String::new() };
    let mut s = String::new();
    let (kind, sub): (&str, &str) = match (t.effect.as_str(), t.direction.as_deref().unwrap_or("")) {
        ("fade", _) => ("fade", "crossfade"),
        ("wipe", "l") => ("barWipe", "leftToRight"),
        ("wipe", "r") => ("barWipe", "leftToRight"),
        ("wipe", "u" | "d") => ("barWipe", "topToBottom"),
        ("push", "l") => ("pushWipe", "fromRight"),
        ("push", "u") => ("pushWipe", "fromBottom"),
        ("push", "d") => ("pushWipe", "fromTop"),
        ("push", _) => ("pushWipe", "fromLeft"),
        ("cover", "l") => ("slideWipe", "fromRight"),
        ("cover", "u") => ("slideWipe", "fromBottom"),
        ("cover", "d") => ("slideWipe", "fromTop"),
        ("cover", _) => ("slideWipe", "fromLeft"),
        ("split", d) => ("splitWipe", if d.contains("vert") { "verticalCenter" } else { "horizontalCenter" }),
        ("randomBar", d) => ("randomBarWipe", if d.contains("vert") { "vertical" } else { "horizontal" }),
        ("checker", _) => ("checkerBoardWipe", "across"),
        ("blinds", d) => ("blindsWipe", if d.contains("vert") { "vertical" } else { "horizontal" }),
        ("dissolve", _) => ("dissolve", ""),
        ("zoom" | "circle" | "diamond" | "plus" | "wedge" | "wheel", _) => ("irisWipe", "rectangle"),
        _ => ("", ""),
    };
    if !kind.is_empty() {
        let _ = write!(s, r#" smil:type="{kind}""#);
        if !sub.is_empty() {
            let _ = write!(s, r#" smil:subtype="{sub}""#);
        }
        if matches!(t.effect.as_str(), "wipe") && t.direction.as_deref() == Some("r") {
            s += r#" smil:direction="reverse""#;
        }
    }
    if let Some(after) = t.after {
        let secs = after / 1000;
        let _ = write!(s, r#" presentation:transition-type="automatic" presentation:duration="PT{:02}H{:02}M{:02}S""#, secs / 3600, secs / 60 % 60, secs % 60);
    }
    s
}

/// The ODF name of a preset (by DrawingML's id) and class.
fn preset_name(id: u32, class: AnimationClass) -> Option<&'static str> {
    let base = match class {
        AnimationClass::Emphasis => match id {
            8 => "spin",
            6 => "grow-and-shrink",
            7 => "change-fill-color",
            3 => "change-font-color",
            9 => "transparency",
            26 => "pulse",
            32 => "teeter",
            _ => return None,
        },
        _ => match id {
            1 => "appear",
            2 => "fly-in",
            3 => "venetian-blinds",
            4 => "box",
            5 => "checkerboard",
            6 => "circle",
            7 => "crawl-in",
            8 => "diamond",
            9 => "dissolve-in",
            10 => "fade-in",
            11 => "flash-once",
            12 => "peek-in",
            13 => "plus",
            14 => "random-bars",
            15 => "spiral-in",
            16 => "split",
            17 => "stretch",
            18 => "strips",
            19 => "swivel",
            20 => "wedge",
            21 => "wheel",
            22 => "wipe",
            23 => "zoom-in",
            _ => return None,
        },
    };
    Some(base)
}

fn seconds(ms: u32) -> String {
    let s = format!("{:.3}", f64::from(ms) / 1000.0);
    format!("{}s", s.trim_end_matches('0').trim_end_matches('.'))
}

/// One effect as an `anim:par` with its behaviors.
fn effect_xml(a: &Animation) -> String {
    let target = format!("id{}", a.shape.0);
    let exit = a.class == AnimationClass::Exit;
    let (class_name, class_prefix) = match a.class {
        AnimationClass::Entrance => ("entrance", "ooo-entrance-"),
        AnimationClass::Exit => ("exit", "ooo-exit-"),
        AnimationClass::Emphasis => ("emphasis", "ooo-emphasis-"),
        AnimationClass::Path => ("motion-path", ""),
        // Not written to ODF (the playing of a clip is not an effect there).
        AnimationClass::Media => ("custom", ""),
    };
    let node_type = match a.start {
        AnimationStart::OnClick => "on-click",
        AnimationStart::WithPrevious => "with-previous",
        AnimationStart::AfterPrevious => "after-previous",
    };
    let mut attrs = format!(r#" smil:begin="{}" smil:fill="hold" presentation:node-type="{node_type}" presentation:preset-class="{class_name}""#, seconds(a.delay));
    if a.class != AnimationClass::Path {
        let name = preset_name(a.preset.0, a.class).map(str::to_string).or_else(|| {
            Some(match a.look {
                AnimationLook::Appear => "appear".to_string(),
                AnimationLook::Fade => "fade-in".to_string(),
                AnimationLook::Fly(_) => "fly-in".to_string(),
                AnimationLook::Spin(_) => "spin".to_string(),
                AnimationLook::Scale(..) => "grow-and-shrink".to_string(),
                _ => return None,
            })
        });
        if let Some(n) = name {
            // (An exit is the entrance played backwards: its name ends "out".)
            let n = if exit { n.replace("-in", "-out").replace("appear", "disappear") } else { n };
            let _ = write!(attrs, r#" presentation:preset-id="{class_prefix}{n}" presentation:preset-sub-type="{}""#, a.preset.1);
        }
    }
    if a.accel > 0 {
        let _ = write!(attrs, r#" smil:accelerate="{}""#, f64::from(a.accel) / 100_000.0);
    }
    if a.decel > 0 {
        let _ = write!(attrs, r#" smil:decelerate="{}""#, f64::from(a.decel) / 100_000.0);
    }
    if a.auto_reverse {
        attrs += r#" smil:autoReverse="true""#;
    }
    match a.repeat_count {
        AnimationRepeat::Count(n) if n > 1 => {
            let _ = write!(attrs, r#" smil:repeatCount="{n}""#);
        }
        AnimationRepeat::UntilNextClick | AnimationRepeat::UntilEndOfSlide => attrs += r#" smil:repeatCount="indefinite""#,
        _ => {}
    }
    let dur = seconds(a.duration.max(1));
    let set = |to: &str, begin: &str| format!(r#"<anim:set smil:begin="{begin}" smil:dur="0.001s" smil:fill="hold" smil:targetElement="{target}" smil:attributeName="visibility" smil:to="{to}"/>"#);
    let mut inner = String::new();
    let mode = if exit { "out" } else { "in" };
    match (a.class, a.look) {
        (AnimationClass::Emphasis, AnimationLook::Spin(angle)) => {
            let _ = write!(inner, r#"<anim:animateTransform smil:dur="{dur}" smil:fill="hold" smil:targetElement="{target}" smil:attributeName="rotate" svg:type="rotate" smil:by="{}"/>"#, f64::from(angle) / 60_000.0);
        }
        (AnimationClass::Emphasis, AnimationLook::Scale(w, h)) => {
            for (name, factor) in [("width", w), ("height", h)] {
                let _ = write!(inner, r#"<anim:animate smil:dur="{dur}" smil:fill="hold" smil:targetElement="{target}" smil:attributeName="{name}" smil:keyTimes="0;1" smil:values="{name};{name}*{}"/>"#, f64::from(factor) / 100_000.0);
            }
        }
        (AnimationClass::Path, look) => {
            let (dx, dy) = match look {
                AnimationLook::Motion(x, y) => (f64::from(x) / 100_000.0, f64::from(y) / 100_000.0),
                _ => (0.0, 0.0),
            };
            let mut path = String::from("M 0 0");
            for (x, y) in &a.path {
                let _ = write!(path, " L {} {}", f64::from(*x) / 100_000.0, f64::from(*y) / 100_000.0);
            }
            if a.path.is_empty() {
                let _ = write!(path, " L {dx} {dy}");
            }
            let _ = write!(inner, r#"<anim:animateMotion smil:dur="{dur}" smil:fill="hold" smil:targetElement="{target}" svg:path="{path}"/>"#);
        }
        (_, AnimationLook::Fly(side)) => {
            let (x, y) = match side {
                AnimationSide::Left => ("0-width/2", "y"),
                AnimationSide::Right => ("1+width/2", "y"),
                AnimationSide::Top => ("x", "0-height/2"),
                AnimationSide::Bottom => ("x", "1+height/2"),
            };
            if !exit {
                inner += &set("visible", "0s");
            }
            for (name, from, home) in [("x", x, "x"), ("y", y, "y")] {
                let values = if exit { format!("{home};{from}") } else { format!("{from};{home}") };
                let _ = write!(inner, r#"<anim:animate smil:dur="{dur}" smil:fill="hold" smil:targetElement="{target}" smil:attributeName="{name}" smil:keyTimes="0;1" smil:values="{values}"/>"#);
            }
            if exit {
                inner += &set("hidden", &dur);
            }
        }
        (_, AnimationLook::Fade | AnimationLook::Other) => {
            if !exit {
                inner += &set("visible", "0s");
            }
            let _ = write!(inner, r#"<anim:transitionFilter smil:dur="{dur}" smil:targetElement="{target}" smil:type="fade" smil:mode="{mode}"/>"#);
            if exit {
                inner += &set("hidden", &dur);
            }
        }
        (_, _) => {
            inner += &set(if exit { "hidden" } else { "visible" }, "0s");
        }
    }
    format!("<anim:par{attrs}>{inner}</anim:par>")
}

/// The slide's animations as the timing tree of ODF: the main sequence of click groups.
fn animations_xml(slide: &Slide) -> String {
    if slide.animations.iter().all(|a| a.is_media()) {
        return String::new();
    }
    // Click groups, each of steps (what starts together), each of effects.
    let mut groups: Vec<Vec<Vec<&Animation>>> = Vec::new();
    for a in slide.animations.iter().filter(|a| !a.is_media()) {
        match a.start {
            AnimationStart::OnClick => groups.push(vec![vec![a]]),
            AnimationStart::WithPrevious => match groups.last_mut().and_then(|g| g.last_mut()) {
                Some(step) => step.push(a),
                None => groups.push(vec![vec![a]]),
            },
            AnimationStart::AfterPrevious => match groups.last_mut() {
                Some(g) => g.push(vec![a]),
                None => groups.push(vec![vec![a]]),
            },
        }
    }
    let mut out = String::from(r#"<anim:par presentation:node-type="timing-root"><anim:seq presentation:node-type="main-sequence" smil:dur="indefinite" smil:restart="never">"#);
    for (gi, group) in groups.iter().enumerate() {
        let click = group.first().and_then(|s| s.first()).is_some_and(|a| a.start == AnimationStart::OnClick);
        let _ = write!(out, r#"<anim:par smil:begin="{}">"#, if click || gi > 0 { "next" } else { "0s" });
        let mut elapsed = 0u32;
        for step in group {
            let _ = write!(out, r#"<anim:par smil:begin="{}">"#, seconds(elapsed));
            for a in step {
                out += &effect_xml(a);
            }
            out += "</anim:par>";
            elapsed += step.iter().map(|a| a.delay + a.duration).max().unwrap_or(0);
        }
        out += "</anim:par>";
    }
    out + "</anim:seq></anim:par>"
}

/// The `.odp` of the presentation.
pub fn write(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    let mut w = W::new(presentation);
    let size = presentation.slide_size;
    // The master pages: the layouts' backdrops and backgrounds (the first one is the default).
    let mut masters = String::new();
    let mut master_of_layout: HashMap<u64, String> = HashMap::new();
    w.current = 1;
    for layout in &presentation.layouts {
        let name = format!("Layout{}", layout.id.0);
        let bg = background_style(&mut w, &layout.background);
        let mut shapes = String::new();
        let no_names = HashMap::new();
        for shape in &layout.backdrop {
            shape_xml(&mut w, &no_names, shape, &mut shapes);
        }
        let bg_attr = bg.map(|b| format!(r#" draw:style-name="{b}""#)).unwrap_or_default();
        let _ = write!(masters, r#"<style:master-page style:name="{}" style:display-name="{}" style:page-layout-name="PM1"{bg_attr}>{shapes}</style:master-page>"#, esc(&name), esc(&layout.name));
        master_of_layout.insert(layout.id.0, name);
    }
    if masters.is_empty() {
        masters = r#"<style:master-page style:name="Default" style:page-layout-name="PM1"/>"#.to_string();
    }
    w.current = 0;
    let default_master = presentation.layouts.first().map(|l| format!("Layout{}", l.id.0)).unwrap_or_else(|| "Default".to_string());
    let mut pages = String::new();
    for (i, slide) in presentation.slides.iter().enumerate() {
        let master = slide.layout.and_then(|l| master_of_layout.get(&l.0).cloned()).unwrap_or_else(|| default_master.clone());
        let mut dp = String::new();
        if let Some(Fill::Solid(_) | Fill::Linear { .. } | Fill::Image(_) | Fill::TileImage { .. } | Fill::Blip(_) | Fill::TileLinear { .. }) = Some(&slide.background) {
            let gp = fill_props(&mut w, &slide.background, 0.0);
            let _ = write!(dp, "{gp}");
        }
        let trans = transition_props(slide);
        let mut dp_attrs = dp;
        if slide.hide_backdrop {
            dp_attrs += r#" presentation:background-objects-visible="false""#;
        }
        dp_attrs += &trans;
        let style = w.style("dp", "drawing-page", None, &format!("<style:drawing-page-properties {dp_attrs}/>"));
        let mut shapes = String::new();
        for shape in &slide.shapes {
            if slide.hidden_shapes.contains(&shape.id) {
                continue;
            }
            shape_xml(&mut w, &slide.shape_names, shape, &mut shapes);
        }
        let name = slide.name.clone().unwrap_or_else(|| format!("page{}", i + 1));
        let visibility = if slide.hidden { r#" presentation:visibility="hidden""# } else { "" };
        let anims = animations_xml(slide);
        let mut notes = String::new();
        if !slide.notes.trim().is_empty() {
            let mut paras = String::new();
            for line in slide.notes.lines() {
                let _ = write!(paras, "<text:p>{}</text:p>", text_runs(line));
            }
            let _ = write!(
                notes,
                r#"<presentation:notes><draw:frame presentation:class="notes" svg:width="{}" svg:height="{}" svg:x="{}" svg:y="{}"><draw:text-box>{paras}</draw:text-box></draw:frame></presentation:notes>"#,
                cm(5_486_400),
                cm(3_600_450),
                cm(685_800),
                cm(4_400_550)
            );
        }
        let _ = write!(pages, r#"<draw:page draw:name="{}" draw:style-name="{style}" draw:master-page-name="{}"{visibility}>{shapes}{anims}{notes}</draw:page>"#, esc(&name), esc(&master));
    }
    let ns = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" xmlns:smil="urn:oasis:names:tc:opendocument:xmlns:smil-compatible:1.0" xmlns:anim="urn:oasis:names:tc:opendocument:xmlns:animation:1.0" xmlns:loext="urn:org:documentfoundation:names:experimental:office:xmlns:loext:1.0""#;
    let fonts: String = w.fonts.iter().map(|f| format!(r#"<style:font-face style:name="{0}" svg:font-family="'{0}'"/>"#, esc(f))).collect();
    let font_decls = format!("<office:font-face-decls>{fonts}</office:font-face-decls>");
    let content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content {ns} office:version="1.3">{font_decls}<office:automatic-styles>{}</office:automatic-styles><office:body><office:presentation>{pages}</office:presentation></office:body></office:document-content>"#,
        w.buckets[0].xml
    );
    let styles = format!(
        r##"<?xml version="1.0" encoding="UTF-8"?><office:document-styles {ns} office:version="1.3">{font_decls}<office:styles><style:default-style style:family="graphic"><style:graphic-properties draw:stroke="none" draw:fill="none"/><style:paragraph-properties style:writing-mode="lr-tb"/></style:default-style><style:style style:name="standard" style:family="graphic"><style:graphic-properties draw:stroke="solid" svg:stroke-width="0.053cm" svg:stroke-color="#3465a4" draw:fill="solid" draw:fill-color="#729fcf"/></style:style>{}</office:styles><office:automatic-styles><style:page-layout style:name="PM1"><style:page-layout-properties fo:margin-top="0cm" fo:margin-bottom="0cm" fo:margin-left="0cm" fo:margin-right="0cm" fo:page-width="{}" fo:page-height="{}" style:print-orientation="landscape"/></style:page-layout>{}</office:automatic-styles><office:master-styles>{masters}</office:master-styles></office:document-styles>"##,
        w.named,
        cm(size.width),
        cm(size.height),
        w.buckets[1].xml
    );
    let mut manifest = format!(r#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3"><manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="{MIMETYPE}"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>"#);
    for (name, _) in &w.pictures {
        let mime = match name.rsplit('.').next().unwrap_or("") {
            "png" => "image/png",
            "jpg" => "image/jpeg",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "tif" => "image/tiff",
            "svg" => "image/svg+xml",
            "emf" => "image/x-emf",
            "wmf" => "image/x-wmf",
            _ => "application/octet-stream",
        };
        let _ = write!(manifest, r#"<manifest:file-entry manifest:full-path="{name}" manifest:media-type="{mime}"/>"#);
    }
    manifest += "</manifest:manifest>";
    let meta = format!(r#"<?xml version="1.0" encoding="UTF-8"?><office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" office:version="1.3"><office:meta><meta:generator>ibisOffice</meta:generator></office:meta></office:document-meta>"#);
    let mut zip = ibis_ooxml::zip::Writer::new();
    let entry = ibis_ooxml::zip::EntryMeta::default();
    let mut put = |name: &str, data: &[u8]| zip.add(name, entry.clone(), data).map_err(|e| Error::new(format!("{e}")));
    // The `mimetype` first and not compressed, as the specification asks.
    put("mimetype", MIMETYPE.as_bytes())?;
    put("META-INF/manifest.xml", manifest.as_bytes())?;
    put("content.xml", content.as_bytes())?;
    put("styles.xml", styles.as_bytes())?;
    put("meta.xml", meta.as_bytes())?;
    for (name, data) in &w.pictures {
        put(name, data)?;
    }
    zip.finish().map_err(|e| Error::new(format!("{e}")))
}
