// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OpenDocument text (`.odt`, ODF 1.2/1.3) read as the `.docx` made from it: the page (size, margins, header and footer), the
//! styles (the document's own named and automatic styles, with their parents), paragraphs, character formatting, tab stops, lists,
//! tables, pictures. Written from the OpenDocument specification (OASIS ODF 1.3, parts 1 and 3), not from any program's code.

use std::collections::HashMap;

use ibis_formats::zip::Archive;
use ibis_formats::Error;
use ibis_ooxml::zip::{EntryMeta, Writer};
use roxmltree::{Document, Node};

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The value of the attribute whose local name is `name` (the ODF names are told apart by their element, not by the prefix).
fn attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

fn is(n: Node, name: &str) -> bool {
    n.is_element() && n.tag_name().name() == name
}

/// A length (`2.5cm`, `12pt`, `0.5in`, `10mm`, `16px`, `1pc`) in twips (1/20 pt).
fn twips(s: &str) -> Option<i64> {
    twips_f(s).map(|t| t.round() as i64)
}

/// `twips` without the rounding.
fn twips_f(s: &str) -> Option<f64> {
    let s = s.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))?;
    let (n, unit) = s.split_at(split);
    let n: f64 = n.parse().ok()?;
    let per = match unit {
        "in" => 1440.0,
        "cm" => 1440.0 / 2.54,
        "mm" => 1440.0 / 25.4,
        "pt" => 20.0,
        "pc" => 240.0,
        "px" => 15.0,
        _ => return None,
    };
    Some(n * per)
}

fn emu(s: &str) -> Option<i64> {
    twips(s).map(|t| t * 635)
}

type Props = HashMap<String, String>;

fn props_of(style: Node, child: &str) -> Props {
    let mut m = Props::new();
    for c in style.children().filter(|c| is(*c, child)) {
        for a in c.attributes() {
            m.insert(a.name().to_string(), a.value().to_string());
        }
    }
    m
}

#[derive(Clone, Debug, Default)]
struct Tab {
    position: i64,
    kind: String,
    leader: Option<char>,
}

#[derive(Clone, Debug, Default)]
struct StyleDef {
    family: String,
    name: String,
    display: String,
    parent: Option<String>,
    para: Props,
    text: Props,
    table: Props,
    cell: Props,
    column: Props,
    /// `style:ruby-properties` of a ruby style.
    ruby: Props,
    row: Props,
    graphic: Props,
    tabs: Vec<Tab>,
    /// An empty `<style:tab-stops/>`: the style drops the tab stops it would inherit.
    tabs_cleared: bool,
    list_style: Option<String>,
    master_page: Option<String>,
    /// `style:section-properties` of a section style, and its `style:columns` as `w:cols`.
    section: Props,
    section_cols: Option<String>,
    /// An automatic style (`office:automatic-styles`): Word reads it as direct formatting of the paragraph, not as a style.
    automatic: bool,
}

fn read_tabs(style: Node) -> Vec<Tab> {
    let mut tabs = Vec::new();
    for pp in style.children().filter(|c| is(*c, "paragraph-properties")) {
        for ts in pp.children().filter(|c| is(*c, "tab-stops")) {
            for t in ts.children().filter(|c| is(*c, "tab-stop")) {
                let Some(position) = attr(t, "position").and_then(twips) else { continue };
                let leader = attr(t, "leader-text").and_then(|s| s.chars().next()).or_else(|| match attr(t, "leader-style") {
                    Some("none") | None => None,
                    Some("dotted") => Some('.'),
                    Some("solid") => Some('_'),
                    Some(_) => Some('-'),
                });
                tabs.push(Tab { position, kind: attr(t, "type").unwrap_or("left").to_string(), leader });
            }
        }
    }
    tabs
}

fn read_style(n: Node) -> Option<StyleDef> {
    let name = attr(n, "name")?.to_string();
    Some(StyleDef {
        family: attr(n, "family").unwrap_or("paragraph").to_string(),
        display: attr(n, "display-name").unwrap_or(&name).to_string(),
        name,
        parent: attr(n, "parent-style-name").map(str::to_string),
        para: props_of(n, "paragraph-properties"),
        text: props_of(n, "text-properties"),
        table: props_of(n, "table-properties"),
        cell: props_of(n, "table-cell-properties"),
        column: props_of(n, "table-column-properties"),
        ruby: props_of(n, "ruby-properties"),
        row: props_of(n, "table-row-properties"),
        graphic: props_of(n, "graphic-properties"),
        tabs: read_tabs(n),
        tabs_cleared: n.descendants().any(|c| is(c, "tab-stops") && !c.children().any(|t| is(t, "tab-stop"))),
        list_style: attr(n, "list-style-name").map(str::to_string),
        master_page: attr(n, "master-page-name").map(str::to_string),
        section: props_of(n, "section-properties"),
        section_cols: n.children().find(|c| is(*c, "section-properties")).and_then(|sp| sp.children().find(|c| is(*c, "columns"))).and_then(|c| {
            let count = attr(c, "column-count").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
            (count > 1).then(|| {
                let sep = if c.children().any(|x| is(x, "column-sep")) { r#" w:sep="1""# } else { "" };
                format!(r#"<w:cols w:num="{count}" w:space="{}"{sep}/>"#, attr(c, "column-gap").and_then(twips).unwrap_or(720))
            })
        }),
        automatic: n.parent().is_some_and(|p| is(p, "automatic-styles")),
    })
}

#[derive(Default)]
struct Styles {
    all: Vec<StyleDef>,
    index: HashMap<(String, String), usize>,
    default_para: Option<StyleDef>,
    /// `style:use-optimal-column-width` of the default style of the `table-column` family.
    default_optimal_column: Option<bool>,
    fonts: HashMap<String, String>,
}

impl Styles {
    fn get(&self, family: &str, name: &str) -> Option<&StyleDef> {
        self.index.get(&(family.to_string(), name.to_string())).map(|i| &self.all[*i])
    }
    /// The tab stops a style inherits from its parents (the nearest parent that has any), as `clear` tabs.
    fn inherited_clears(&self, family: &str, parent: Option<&str>) -> Vec<Tab> {
        let mut cur = parent.and_then(|p| self.get(family, p));
        for _ in 0..16 {
            let Some(c) = cur else { return Vec::new() };
            if !c.tabs.is_empty() {
                return c.tabs.iter().map(|t| Tab { kind: "clear".into(), leader: None, ..t.clone() }).collect();
            }
            if c.tabs_cleared {
                return Vec::new();
            }
            cur = c.parent.as_deref().and_then(|p| self.get(family, p));
        }
        Vec::new()
    }
    fn id(&self, family: &str, name: &str) -> Option<String> {
        self.index.get(&(family.to_string(), name.to_string())).map(|i| self.style_id(*i))
    }
    /// Whether `s` or one of its parents is a heading style (`Heading N`).
    fn is_heading(&self, s: &StyleDef) -> bool {
        let is_heading_name = |n: &str| n.replace("_20_", " ").strip_prefix("Heading ").is_some_and(|n| n.trim().parse::<u32>().is_ok());
        let mut cur = Some(s);
        let mut guard = 0;
        while let Some(c) = cur {
            if is_heading_name(&c.name) || c.parent.as_deref().is_some_and(is_heading_name) {
                return true;
            }
            guard += 1;
            if guard > 16 {
                break;
            }
            cur = c.parent.as_deref().and_then(|p| self.get(&c.family, p));
        }
        false
    }
    /// The list style a paragraph style gets from itself or its parents (`style:list-style-name`).
    fn chain_list_style<'s>(&'s self, s: &'s StyleDef) -> Option<&'s str> {
        let mut cur = Some(s);
        let mut guard = 0;
        while let Some(c) = cur {
            if let Some(l) = &c.list_style {
                return Some(l);
            }
            guard += 1;
            if guard > 16 {
                break;
            }
            cur = c.parent.as_deref().and_then(|p| self.get(&c.family, p));
        }
        None
    }
    /// Whether `s` or one of its parents sets the paragraph property `key`.
    fn chain_has_para(&self, s: &StyleDef, key: &str) -> bool {
        let mut cur = Some(s);
        let mut guard = 0;
        while let Some(c) = cur {
            if c.para.contains_key(key) {
                return true;
            }
            guard += 1;
            if guard > 16 {
                break;
            }
            cur = c.parent.as_deref().and_then(|p| self.get(&c.family, p));
        }
        false
    }
    /// The id of style `i`: the built-in `HeadingN` for the style named `Heading N` (ODF writes the space as `_20_`), else `S<i>`.
    fn style_id(&self, i: usize) -> String {
        let s = &self.all[i];
        if s.family == "paragraph" {
            let name = s.name.replace("_20_", " ");
            if let Some(n) = name.strip_prefix("Heading ").and_then(|n| n.trim().parse::<u32>().ok()).filter(|n| (1..=6).contains(n)) {
                return format!("Heading{n}");
            }
        }
        format!("S{i}")
    }
    fn add(&mut self, s: StyleDef) {
        self.index.insert((s.family.clone(), s.name.clone()), self.all.len());
        self.all.push(s);
    }
    /// The graphic properties of a graphic style with those of its parents under them.
    fn graphic(&self, name: &str) -> Props {
        let mut chain = Vec::new();
        let mut cur = self.get("graphic", name);
        while let Some(c) = cur {
            chain.push(c);
            if chain.len() > 16 {
                break;
            }
            cur = c.parent.as_deref().and_then(|p| self.get("graphic", p));
        }
        let mut out = Props::new();
        for c in chain.iter().rev() {
            for (k, v) in &c.graphic {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }
    fn font(&self, name: &str) -> String {
        self.fonts.get(name).cloned().unwrap_or_else(|| name.to_string())
    }
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('\'').trim_matches('"').to_string()
}

fn load_styles(docs: &[&Document]) -> Styles {
    let mut st = Styles::default();
    for doc in docs {
        for decl in doc.descendants().filter(|n| is(*n, "font-face")) {
            if let Some(name) = attr(decl, "name") {
                let family = attr(decl, "font-family").map(unquote).unwrap_or_else(|| name.to_string());
                st.fonts.insert(name.to_string(), family);
            }
        }
    }
    for doc in docs {
        for n in doc.descendants() {
            if is(n, "default-style") && attr(n, "family") == Some("paragraph") && st.default_para.is_none() {
                st.default_para = Some(StyleDef {
                    family: "paragraph".into(),
                    para: props_of(n, "paragraph-properties"),
                    text: props_of(n, "text-properties"),
                    tabs: read_tabs(n),
                    ..Default::default()
                });
            }
            if is(n, "default-style") && attr(n, "family") == Some("table-column") && st.default_optimal_column.is_none() {
                st.default_optimal_column = props_of(n, "table-column-properties").get("use-optimal-column-width").map(|v| v == "true");
            }
            if is(n, "style") && n.parent().is_some_and(|p| is(p, "styles") || is(p, "automatic-styles")) {
                if let Some(s) = read_style(n) {
                    st.add(s);
                }
            }
        }
    }
    st
}

/// A border (`0.5pt solid #000000`) as the attributes of a `w:top`-like element.
fn border_attrs(v: &str) -> Option<String> {
    let v = v.trim();
    if v.is_empty() || v == "none" || v == "hidden" {
        return None;
    }
    let mut width = 0.75_f64;
    let mut style = "single";
    let mut color = "auto".to_string();
    for part in v.split_whitespace() {
        if let Some(t) = twips(part) {
            width = t as f64 / 20.0;
        } else if part.starts_with('#') && part.len() == 7 {
            color = part[1..].to_uppercase();
        } else {
            style = match part {
                "double" => "double",
                "dotted" => "dotted",
                "dashed" => "dashed",
                "groove" | "ridge" | "inset" | "outset" => "single",
                _ => "single",
            };
        }
    }
    let sz = if style == "double" {
        // `fo:border` of a double line is the whole width (two lines and the gap, Word writes 3 x the line). Word reads it as the line
        // width `w:sz` of one of its double-line presets (2, 4, 6, 12, 18, 24 = 1/4, 1/2, 3/4, 1 1/2, 2 1/4, 3 pt): measured with -SaveAs todocx
        // on 50 whole widths (0.75 to 20 pt, three equal lines): 0.75-1.15pt gives 2, 1.2-1.92 gives 4, 1.93-3.4 gives 6, 3.45-5.5 gives 12,
        // 5.75-7.5 gives 18, 8 and more gives 24, that is the nearest preset to width/3 (the switch points lie within 0.2/8 pt of the middles).
        let line = width * 8.0 / 3.0;
        [2_i64, 4, 6, 12, 18, 24].windows(2).find(|w| line <= (w[0] + w[1]) as f64 / 2.0).map_or(24, |w| w[0])
    } else {
        ((width * 8.0).round() as i64).clamp(2, 96)
    };
    Some(format!(r#"w:val="{style}" w:sz="{sz}" w:space="0" w:color="{color}""#))
}

/// The four borders of `props` (`border`, then `border-top` and so on) as `w:top`/`w:left`/`w:bottom`/`w:right` elements in `tag`.
fn borders(props: &Props, tag: &str) -> String {
    let all = props.get("border");
    let mut out = String::new();
    for (side, w) in [("top", "top"), ("left", "left"), ("bottom", "bottom"), ("right", "right")] {
        let v = props.get(&format!("border-{side}")).or(all);
        if let Some(mut a) = v.and_then(|v| border_attrs(v)) {
            // A paragraph border's distance from the text is the padding of that side (`w:space` in points, at most 31: Word reads
            // `fo:padding` 0.4305in as 31 and writes it back so; measured with -SaveAs todocx on a border of width 0).
            if tag == "pBdr" {
                let pad = props.get(&format!("padding-{side}")).or_else(|| props.get("padding")).and_then(|v| twips_f(v)).unwrap_or(0.0);
                let space = (pad / 20.0).round().clamp(0.0, 31.0) as i64;
                a = a.replace(r#"w:space="0""#, &format!(r#"w:space="{space}""#));
            }
            out.push_str(&format!("<w:{w} {a}/>"));
        }
    }
    if out.is_empty() { String::new() } else { format!("<w:{tag}>{out}</w:{tag}>") }
}

fn shade(props: &Props) -> String {
    match props.get("background-color").map(String::as_str) {
        Some(c) if c.starts_with('#') && c.len() == 7 => format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{}"/>"#, c[1..].to_uppercase()),
        _ => String::new(),
    }
}

/// The run properties of `text-properties`, in the order of the schema.
fn rpr(text: &Props, styles: &Styles) -> String {
    let mut out = String::new();
    let font = |key: &str| text.get(key).map(|n| styles.font(n));
    let ascii = font("font-name").or_else(|| text.get("font-family").map(|s| unquote(s)));
    let east = font("font-name-asian").or_else(|| text.get("font-family-asian").map(|s| unquote(s)));
    let cs = font("font-name-complex").or_else(|| text.get("font-family-complex").map(|s| unquote(s)));
    if ascii.is_some() || east.is_some() || cs.is_some() {
        let mut f = String::from("<w:rFonts");
        if let Some(a) = &ascii {
            f.push_str(&format!(r#" w:ascii="{0}" w:hAnsi="{0}""#, esc(a)));
        }
        if let Some(e) = &east {
            f.push_str(&format!(r#" w:eastAsia="{}""#, esc(e)));
        }
        if let Some(c) = &cs {
            f.push_str(&format!(r#" w:cs="{}""#, esc(c)));
        }
        f.push_str("/>");
        out.push_str(&f);
    }
    match text.get("font-weight").map(String::as_str) {
        Some("bold") => out.push_str("<w:b/><w:bCs/>"),
        Some("normal") => out.push_str(r#"<w:b w:val="0"/><w:bCs w:val="0"/>"#),
        _ => {}
    }
    match text.get("font-style").map(String::as_str) {
        Some("italic") | Some("oblique") => out.push_str("<w:i/><w:iCs/>"),
        Some("normal") => out.push_str(r#"<w:i w:val="0"/><w:iCs w:val="0"/>"#),
        _ => {}
    }
    match text.get("text-transform").map(String::as_str) {
        Some("uppercase") => out.push_str("<w:caps/>"),
        _ => {}
    }
    if text.get("font-variant").map(String::as_str) == Some("small-caps") {
        out.push_str("<w:smallCaps/>");
    }
    match text.get("text-line-through-style").map(String::as_str) {
        Some("none") => out.push_str(r#"<w:strike w:val="0"/>"#),
        Some(_) => out.push_str(if text.get("text-line-through-type").map(String::as_str) == Some("double") { "<w:dstrike/>" } else { "<w:strike/>" }),
        None => {}
    }
    if text.get("display").map(String::as_str) == Some("none") {
        out.push_str("<w:vanish/>");
    }
    match text.get("color").map(String::as_str) {
        Some(c) if c.starts_with('#') && c.len() == 7 => out.push_str(&format!(r#"<w:color w:val="{}"/>"#, c[1..].to_uppercase())),
        _ => {}
    }
    // `fo:letter-spacing="normal"` is `w:spacing w:val="0"`: Word writes it on the runs (and paragraph marks) that say it, over a style that spaces.
    if let Some(sp) = text.get("letter-spacing").and_then(|s| if s.trim() == "normal" { Some(0) } else { twips(s) }) {
        out.push_str(&format!(r#"<w:spacing w:val="{sp}"/>"#));
    }
    if let Some(p) = text.get("text-scale").and_then(|s| s.trim_end_matches('%').parse::<i64>().ok()) {
        out.push_str(&format!(r#"<w:w w:val="{p}"/>"#));
    }
    // `style:letter-kerning` is `w:kern`: Word writes 3 for true and 0 for false (measured with todocx on the default style set to true,
    // to false and without the attribute: true and absent both give `w:kern 3` in the docDefaults, false gives none; a run or a style that says
    // false under a default of true gets `w:kern 0`).
    match text.get("letter-kerning").map(String::as_str) {
        Some("true") => out.push_str(r#"<w:kern w:val="3"/>"#),
        Some("false") => out.push_str(r#"<w:kern w:val="0"/>"#),
        _ => {}
    }
    if let Some(sz) = text.get("font-size").and_then(|s| s.strip_suffix("pt")).and_then(|s| s.parse::<f64>().ok()) {
        let half = (sz * 2.0).round() as i64;
        out.push_str(&format!(r#"<w:sz w:val="{half}"/>"#));
    }
    if let Some(sz) = text.get("font-size-complex").and_then(|s| s.strip_suffix("pt")).and_then(|s| s.parse::<f64>().ok()) {
        out.push_str(&format!(r#"<w:szCs w:val="{}"/>"#, (sz * 2.0).round() as i64));
    } else if let Some(sz) = text.get("font-size").and_then(|s| s.strip_suffix("pt")).and_then(|s| s.parse::<f64>().ok()) {
        out.push_str(&format!(r#"<w:szCs w:val="{}"/>"#, (sz * 2.0).round() as i64));
    }
    match text.get("text-underline-style").map(String::as_str) {
        Some("none") => out.push_str(r#"<w:u w:val="none"/>"#),
        Some(s) => {
            let val = match (s, text.get("text-underline-type").map(String::as_str)) {
                (_, Some("double")) => "double",
                ("dotted", _) => "dotted",
                ("dash", _) | ("long-dash", _) => "dash",
                ("wave", _) => "wave",
                _ => "single",
            };
            out.push_str(&format!(r#"<w:u w:val="{val}"/>"#));
        }
        None => {}
    }
    if let Some(bg) = text.get("background-color") {
        if bg.starts_with('#') && bg.len() == 7 {
            out.push_str(&format!(r#"<w:shd w:val="clear" w:color="auto" w:fill="{}"/>"#, bg[1..].to_uppercase()));
        }
    }
    if let Some(pos) = text.get("text-position") {
        let first = pos.split_whitespace().next().unwrap_or("");
        if first.starts_with("super") || first.starts_with('+') && first != "+0%" {
            out.push_str(r#"<w:vertAlign w:val="superscript"/>"#);
        } else if first.starts_with("sub") || first.starts_with('-') {
            out.push_str(r#"<w:vertAlign w:val="subscript"/>"#);
        }
    }
    out.push_str(&combine_xml(text));
    out
}

/// `style:text-combine="lines"` (two lines in one) is `w:eastAsianLayout w:combine` with the bracket pair named by
/// `style:text-combine-start-char` (measured with -SaveAs todocx: "(" ")" is `w:combineBrackets="round"`; the .docx has it in the run's
/// own properties, which is where the renderer reads it).
fn combine_xml(text: &Props) -> String {
    if text.get("text-combine").map(String::as_str) != Some("lines") {
        return String::new();
    }
    let brackets = match text.get("text-combine-start-char").map(String::as_str) {
        Some("(") => r#" w:combineBrackets="round""#,
        Some("[") => r#" w:combineBrackets="square""#,
        Some("<") => r#" w:combineBrackets="angle""#,
        Some("{") => r#" w:combineBrackets="curly""#,
        _ => "",
    };
    format!(r#"<w:eastAsianLayout w:id="1" w:combine="1"{brackets}/>"#)
}

/// The paragraph properties of `paragraph-properties` (and the tab stops), in the order of the schema. `lists` leaves out the
/// indents (the list level's own apply).
fn ppr(para: &Props, tabs: &[Tab], lists: bool) -> String {
    let mut out = String::new();
    if para.get("keep-with-next").map(String::as_str) == Some("always") {
        out.push_str("<w:keepNext/>");
    }
    if para.get("keep-together").map(String::as_str) == Some("always") {
        out.push_str("<w:keepLines/>");
    }
    if para.get("break-before").map(String::as_str) == Some("page") {
        out.push_str("<w:pageBreakBefore/>");
    }
    if let Some(w) = para.get("widows").and_then(|s| s.parse::<i64>().ok()) {
        out.push_str(if w > 0 { "<w:widowControl/>" } else { r#"<w:widowControl w:val="0"/>"# });
    }
    out.push_str(&borders(para, "pBdr"));
    out.push_str(&shade(para));
    if !tabs.is_empty() {
        out.push_str("<w:tabs>");
        for t in tabs {
            let val = match t.kind.as_str() {
                "center" => "center",
                "right" => "right",
                "char" => "decimal",
                "clear" => "clear",
                _ => "left",
            };
            let leader = match t.leader {
                Some('.') => r#" w:leader="dot""#,
                Some('_') => r#" w:leader="underscore""#,
                Some('-') => r#" w:leader="hyphen""#,
                _ => "",
            };
            out.push_str(&format!(r#"<w:tab w:val="{val}"{leader} w:pos="{}"/>"#, t.position));
        }
        out.push_str("</w:tabs>");
    }
    // Measured with Word (-SaveAs todocx on variants of one odt, 102): `style:line-break="normal"` is `w:wordWrap w:val="0"` (strict writes
    // nothing: the kinsoku rules stay on), `style:punctuation-wrap="simple"` is `w:overflowPunct w:val="0"` (hanging writes nothing),
    // `style:text-autospace="none"` is `w:autoSpaceDE w:val="0"` (ideograph-alpha writes nothing), and `w:autoSpaceDN` is 0 whatever the
    // ODF says (Word's pPrDefault of an imported ODF has `autoSpaceDN w:val="0"`; ODF has no property for it). Corpus: documents 0.840 with DN
    // following DE, 0.846 with DN 0 (049, 130, 043 and others up; 128 down: its pages are 0.70 against Word's PDF for Word's own docx too).
    let flag = |on: bool| if on { "" } else { r#" w:val="0""# };
    if let Some(v) = para.get("line-break") {
        out.push_str(&format!("<w:wordWrap{}/>", flag(v == "strict")));
    }
    if let Some(v) = para.get("punctuation-wrap") {
        out.push_str(&format!("<w:overflowPunct{}/>", flag(v == "hanging")));
    }
    if let Some(v) = para.get("text-autospace") {
        out.push_str(&format!(r#"<w:autoSpaceDE{}/><w:autoSpaceDN w:val="0"/>"#, flag(v == "ideograph-alpha")));
    }
    match para.get("snap-to-layout-grid").map(String::as_str) {
        Some("false") => out.push_str(r#"<w:snapToGrid w:val="0"/>"#),
        // Word writes an explicit `true` as `w:snapToGrid` too (todocx of 034: a paragraph of the Header style, whose style has it off,
        // with `style:snap-to-layout-grid="true"` is `<w:snapToGrid/>`), which turns the style's off back on.
        Some("true") => out.push_str("<w:snapToGrid/>"),
        _ => {}
    }
    let mut sp = String::new();
    if let Some(v) = para.get("margin-top").and_then(|s| twips(s)) {
        sp.push_str(&format!(r#" w:before="{}""#, v.max(0)));
    }
    if let Some(v) = para.get("margin-bottom").and_then(|s| twips(s)) {
        sp.push_str(&format!(r#" w:after="{}""#, v.max(0)));
    }
    if let Some(lh) = para.get("line-height") {
        if let Some(p) = lh.strip_suffix('%').and_then(|s| s.parse::<f64>().ok()) {
            sp.push_str(&format!(r#" w:line="{}" w:lineRule="auto""#, (p * 2.4).round() as i64));
        } else if let Some(t) = twips(lh) {
            if t > 0 {
                sp.push_str(&format!(r#" w:line="{t}" w:lineRule="exact""#));
            }
        }
    } else if let Some(t) = para.get("line-height-at-least").and_then(|s| twips(s)) {
        sp.push_str(&format!(r#" w:line="{t}" w:lineRule="atLeast""#));
    }
    if !sp.is_empty() {
        out.push_str(&format!("<w:spacing{sp}/>"));
    }
    // ODF 1.3 20.346 `style:contextual-spacing`: no space between paragraphs of the same style (`w:contextualSpacing`, which Word writes it
    // from and reads it as).
    if let Some(v) = para.get("contextual-spacing") {
        out.push_str(&format!("<w:contextualSpacing{}/>", if v == "true" { "" } else { r#" w:val="0""# }));
    }
    if !lists {
        let mut ind = String::new();
        if let Some(v) = para.get("margin-left").and_then(|s| twips(s)) {
            ind.push_str(&format!(r#" w:left="{v}""#));
        }
        if let Some(v) = para.get("margin-right").and_then(|s| twips(s)) {
            ind.push_str(&format!(r#" w:right="{v}""#));
        }
        if let Some(v) = para.get("text-indent").and_then(|s| twips(s)) {
            if v >= 0 {
                ind.push_str(&format!(r#" w:firstLine="{v}""#));
            } else {
                ind.push_str(&format!(r#" w:hanging="{}""#, -v));
            }
        }
        if !ind.is_empty() {
            out.push_str(&format!("<w:ind{ind}/>"));
        }
    }
    match para.get("text-align").map(String::as_str) {
        Some("center") => out.push_str(r#"<w:jc w:val="center"/>"#),
        Some("end") | Some("right") => out.push_str(r#"<w:jc w:val="right"/>"#),
        Some("justify") => out.push_str(r#"<w:jc w:val="both"/>"#),
        Some("start") | Some("left") => out.push_str(r#"<w:jc w:val="left"/>"#),
        _ => {}
    }
    out
}

// ---------------------------------------------------------------------------------------------------------------------------------
// lists

struct ListLevel {
    number: bool,
    format: String,
    prefix: String,
    suffix: String,
    start: i64,
    display_levels: usize,
    bullet: String,
    bullet_font: Option<String>,
    left: i64,
    hanging: i64,
    followed_by: String,
    tab_pos: Option<i64>,
    /// `fo:text-align` of the list level's properties: where the label sits in the space it has (`w:lvlJc`).
    align: &'static str,
}

fn list_levels(style: Node, styles: &Styles) -> Vec<ListLevel> {
    let mut levels: Vec<(usize, ListLevel)> = Vec::new();
    for l in style.children().filter(|c| is(*c, "list-level-style-number") || is(*c, "list-level-style-bullet") || is(*c, "list-level-style-image")) {
        let Some(level) = attr(l, "level").and_then(|v| v.parse::<usize>().ok()) else { continue };
        let number = is(l, "list-level-style-number");
        let props = l.children().find(|c| is(*c, "list-level-properties"));
        let (mut left, mut hanging) = (0, 0);
        let mut followed_by = "listtab".to_string();
        let mut tab_pos = None;
        if let Some(p) = props {
            if let Some(la) = p.children().find(|c| is(*c, "list-level-label-alignment")) {
                left = attr(la, "margin-left").and_then(twips).unwrap_or(0);
                hanging = -attr(la, "text-indent").and_then(twips).unwrap_or(0);
                followed_by = attr(la, "label-followed-by").unwrap_or("listtab").to_string();
                tab_pos = attr(la, "list-tab-stop-position").and_then(twips);
            } else {
                // The old way: space before the label and the label's width.
                let before = attr(p, "space-before").and_then(twips).unwrap_or(0);
                let width = attr(p, "min-label-width").and_then(twips).unwrap_or(0);
                left = before + width;
                hanging = width;
            }
        }
        let align = match props.and_then(|p| attr(p, "text-align")) {
            Some("end") | Some("right") => "right",
            Some("center") => "center",
            _ => "left",
        };
        let text = l.children().find(|c| is(*c, "text-properties")).map(|_| props_of(l, "text-properties"));
        levels.push((
            level,
            ListLevel {
                number,
                format: attr(l, "num-format").unwrap_or("1").to_string(),
                prefix: attr(l, "num-prefix").unwrap_or("").to_string(),
                suffix: attr(l, "num-suffix").unwrap_or("").to_string(),
                start: attr(l, "start-value").and_then(|v| v.parse().ok()).unwrap_or(1),
                display_levels: attr(l, "display-levels").and_then(|v| v.parse().ok()).unwrap_or(1),
                bullet: attr(l, "bullet-char").unwrap_or("\u{2022}").to_string(),
                bullet_font: text.as_ref().and_then(|t| t.get("font-name").map(|n| styles.font(n))),
                left,
                hanging,
                followed_by,
                tab_pos,
                align,
            },
        ));
    }
    levels.sort_by_key(|(l, _)| *l);
    levels.into_iter().map(|(_, l)| l).collect()
}

fn num_fmt(format: &str) -> &'static str {
    match format {
        "1" => "decimal",
        "a" => "lowerLetter",
        "A" => "upperLetter",
        "i" => "lowerRoman",
        "I" => "upperRoman",
        "" => "none",
        f if f.contains('\u{4e00}') => "japaneseCounting",
        f if f.contains('\u{3042}') => "aiueo",
        f if f.contains('\u{30a2}') => "aiueo",
        f if f.contains('\u{ff11}') => "decimalFullWidth",
        _ => "decimal",
    }
}

fn abstract_num(id: usize, levels: &[ListLevel]) -> String {
    let mut s = format!(r#"<w:abstractNum w:abstractNumId="{id}"><w:multiLevelType w:val="multilevel"/>"#);
    for (i, l) in levels.iter().enumerate().take(9) {
        let text = if l.number {
            let mut t = esc(&l.prefix);
            let from = (i + 1).saturating_sub(l.display_levels.max(1));
            for k in from..=i {
                if k > from {
                    t.push('.');
                }
                t.push_str(&format!("%{}", k + 1));
            }
            t.push_str(&esc(&l.suffix));
            t
        } else {
            esc(&l.bullet)
        };
        let fmt = if l.number { num_fmt(&l.format) } else { "bullet" };
        let suff = match l.followed_by.as_str() {
            "space" => "space",
            "nothing" => "nothing",
            _ => "tab",
        };
        s.push_str(&format!(r#"<w:lvl w:ilvl="{i}"><w:start w:val="{}"/><w:numFmt w:val="{fmt}"/><w:suff w:val="{suff}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="{}"/><w:pPr>"#, l.start, l.align));
        if let (Some(tp), "listtab") = (l.tab_pos, l.followed_by.as_str()) {
            s.push_str(&format!(r#"<w:tabs><w:tab w:val="num" w:pos="{tp}"/></w:tabs>"#));
        }
        s.push_str(&format!(r#"<w:ind w:left="{}" w:hanging="{}"/></w:pPr>"#, l.left, l.hanging.max(0)));
        if !l.number {
            if let Some(f) = &l.bullet_font {
                s.push_str(&format!(r#"<w:rPr><w:rFonts w:ascii="{0}" w:hAnsi="{0}" w:hint="default"/></w:rPr>"#, esc(f)));
            }
        }
        s.push_str("</w:lvl>");
    }
    s.push_str("</w:abstractNum>");
    s
}

// ---------------------------------------------------------------------------------------------------------------------------------
// the body

struct Image {
    name: String,
    bytes: Vec<u8>,
}

struct Ctx<'a> {
    styles: &'a Styles,
    archive: &'a Archive<'a>,
    list_styles: HashMap<String, usize>,
    list_nodes: HashMap<String, Node<'a, 'a>>,
    abstract_nums: Vec<String>,
    nums: Vec<(usize, bool)>,
    last_num: HashMap<String, usize>,
    links: Vec<(String, String)>,
    images: Vec<Image>,
    drawing_id: u32,
    bookmark_id: u32,
    /// The bodies of the footnotes (`w:footnote` paragraphs), in the order their references appear (id = index + 1).
    footnotes: Vec<String>,
    /// The same for the endnotes (`w:endnote` paragraphs; Word saves an endnote of an .odt like a footnote: a `w:endnoteRef` first run and a final empty paragraph, measured with todocx).
    endnotes: Vec<String>,
    /// Depth of table cells being read (a page break before a paragraph in a cell is not kept).
    in_cell: usize,
    /// A page break before the next paragraph (a table whose style breaks the page before it).
    pending_break: bool,
}

impl<'a> Ctx<'a> {
    fn num_for(&mut self, style: &str, restart: bool) -> Option<usize> {
        if !restart {
            if let Some(n) = self.last_num.get(style) {
                return Some(*n);
            }
        }
        let abs = if let Some(a) = self.list_styles.get(style) {
            *a
        } else {
            let node = *self.list_nodes.get(style)?;
            let levels = list_levels(node, self.styles);
            if levels.is_empty() {
                return None;
            }
            let id = self.abstract_nums.len();
            self.abstract_nums.push(abstract_num(id, &levels));
            self.list_styles.insert(style.to_string(), id);
            id
        };
        self.nums.push((abs, restart));
        let n = self.nums.len();
        self.last_num.insert(style.to_string(), n);
        Some(n)
    }
}

fn spaces_text(count: usize) -> String {
    " ".repeat(count)
}

struct RunBuilder {
    xml: String,
    /// Whether the text so far ends with a space (or is empty): the next white space is dropped.
    at_space: bool,
    /// Direct run properties of every run (the text properties of the paragraph's automatic style).
    base_rpr: String,
    /// Name of the paragraph style (family `paragraph`), for the sizes and the language a ruby takes from it.
    para_style: Option<String>,
}

/// The text property `key` of the style `name` of `family`, or of the nearest parent that has it; else of the default paragraph style.
fn text_prop(styles: &Styles, family: &str, name: Option<&str>, key: &str) -> Option<String> {
    let mut cur = name.and_then(|n| styles.get(family, n));
    let mut guard = 0;
    while let Some(s) = cur {
        if let Some(v) = s.text.get(key) {
            return Some(v.clone());
        }
        guard += 1;
        if guard > 16 {
            break;
        }
        cur = s.parent.as_deref().and_then(|p| styles.get(family, p));
    }
    None
}

/// `text_prop` of a size in half points (`fo:font-size="6pt"` is 12).
fn half_points(v: Option<String>) -> Option<u32> {
    let pt = v?.strip_suffix("pt")?.parse::<f64>().ok()?;
    Some((pt * 2.0).round() as u32)
}

fn run(rb: &mut RunBuilder, style_id: Option<&str>, body: &str) {
    rb.xml.push_str("<w:r>");
    if style_id.is_some() || !rb.base_rpr.is_empty() {
        let r = style_id.map(|s| format!(r#"<w:rStyle w:val="{s}"/>"#)).unwrap_or_default();
        rb.xml.push_str(&format!("<w:rPr>{r}{}</w:rPr>", rb.base_rpr));
    }
    rb.xml.push_str(body);
    rb.xml.push_str("</w:r>");
}

fn text_run(rb: &mut RunBuilder, style_id: Option<&str>, text: &str) {
    if text.is_empty() {
        return;
    }
    run(rb, style_id, &format!(r#"<w:t xml:space="preserve">{}</w:t>"#, esc(text)));
}

fn inline_content<'a>(node: Node<'a, 'a>, ctx: &mut Ctx<'a>, rb: &mut RunBuilder, style_id: Option<&str>) {
    for child in node.children() {
        if child.is_text() {
            let raw = child.text().unwrap_or("");
            let mut out = String::new();
            let mut prev_space = rb.at_space;
            for ch in raw.chars() {
                if matches!(ch, ' ' | '\t' | '\n' | '\r') {
                    if !prev_space {
                        out.push(' ');
                        prev_space = true;
                    }
                } else {
                    out.push(ch);
                    prev_space = false;
                }
            }
            rb.at_space = prev_space;
            text_run(rb, style_id, &out);
            continue;
        }
        if !child.is_element() {
            continue;
        }
        match child.tag_name().name() {
            "span" => {
                let sid = attr(child, "style-name").and_then(|n| ctx.styles.id("text", n));
                let combine = attr(child, "style-name").and_then(|n| ctx.styles.get("text", n)).map(|s| combine_xml(&s.text)).unwrap_or_default();
                let keep = rb.base_rpr.len();
                rb.base_rpr.push_str(&combine);
                inline_content(child, ctx, rb, sid.as_deref().or(style_id));
                rb.base_rpr.truncate(keep);
            }
            "s" => {
                let c = attr(child, "c").and_then(|v| v.parse().ok()).unwrap_or(1);
                text_run(rb, style_id, &spaces_text(c));
                rb.at_space = true;
            }
            "tab" => {
                run(rb, style_id, "<w:tab/>");
                rb.at_space = true;
            }
            "line-break" => {
                run(rb, style_id, "<w:br/>");
                rb.at_space = true;
            }
            "a" => {
                let href = attr(child, "href").unwrap_or("");
                let sid = attr(child, "style-name").and_then(|n| ctx.styles.id("text", n));
                if href.starts_with('#') {
                    rb.xml.push_str(&format!(r#"<w:hyperlink w:anchor="{}">"#, esc(&href[1..])));
                } else {
                    let rid = format!("rIdL{}", ctx.links.len() + 1);
                    ctx.links.push((rid.clone(), href.to_string()));
                    rb.xml.push_str(&format!(r#"<w:hyperlink r:id="{rid}">"#));
                }
                inline_content(child, ctx, rb, sid.as_deref().or(style_id));
                rb.xml.push_str("</w:hyperlink>");
            }
            "bookmark-start" | "bookmark" => {
                if let Some(name) = attr(child, "name") {
                    ctx.bookmark_id += 1;
                    rb.xml.push_str(&format!(r#"<w:bookmarkStart w:id="{}" w:name="{}"/>"#, ctx.bookmark_id, esc(name)));
                    if child.tag_name().name() == "bookmark" {
                        rb.xml.push_str(&format!(r#"<w:bookmarkEnd w:id="{}"/>"#, ctx.bookmark_id));
                    }
                }
            }
            "bookmark-end" => {}
            "note" => {
                let endnote = attr(child, "note-class") == Some("endnote");
                // ODF 1.3 6.3.2: `text:note-body` holds the footnote's paragraphs. Word writes the reference mark (`w:footnoteRef`)
                // as the first run of the footnote and the reference as a superscript run in the text.
                if let Some(body_node) = child.children().find(|c| is(*c, "note-body")) {
                    let mut body = String::new();
                    blocks(body_node, ctx, None, &mut body);
                    if !body.starts_with("<w:p>") {
                        body.insert_str(0, "<w:p/>");
                    }
                    let mark = if endnote { r#"<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:endnoteRef/></w:r>"# } else { r#"<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteRef/></w:r>"# };
                    if let Some(i) = body.find("</w:pPr>").filter(|i| body.find("</w:p>").is_none_or(|e| *i < e)) {
                        body.insert_str(i + "</w:pPr>".len(), mark);
                    } else if body.starts_with("<w:p>") {
                        body.insert_str("<w:p>".len(), mark);
                    }
                    // Word's reader of an .odt ends every footnote with one more, empty paragraph (measured: all 25 footnotes of a saved
                    // .odt are `w:footnote` with a final empty `w:p`; dropping them shortens the layout by a page).
                    body.push_str("<w:p/>");
                    let (list, tag) = if endnote { (&mut ctx.endnotes, "endnoteReference") } else { (&mut ctx.footnotes, "footnoteReference") };
                    list.push(body);
                    let id = list.len();
                    rb.xml.push_str(&format!(r#"<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:{tag} w:id="{id}"/></w:r>"#));
                }
            }
            "soft-page-break" | "annotation" | "annotation-end" | "tracked-changes" | "change-start" | "change-end" | "change" => {}
            "page-number" => field(rb, style_id, "PAGE", child),
            "page-count" => field(rb, style_id, "NUMPAGES", child),
            "frame" | "connector" | "line" | "custom-shape" | "g" | "rect" | "ellipse" => drawing(child, ctx, rb),
            "ruby" => {
                // `text:ruby` is `w:ruby` (the ruby text in `w:rt`, in its own character style; the size, the raise and the base size are
                // taken from the runs as for a .docx without `w:hps`). `style:ruby-align` is `w:rubyAlign` (ODF 1.3 §20.348 and
                // ECMA-376 §17.3.3.26 name the same five values).
                let align = attr(child, "style-name").and_then(|n| ctx.styles.get("ruby", n)).and_then(|s| s.ruby.get("ruby-align")).map(|a| match a.as_str() {
                    "left" => "left",
                    "right" => "right",
                    "distribute-letter" => "distributeLetter",
                    "distribute-space" => "distributeSpace",
                    _ => "center",
                });
                let mut base = RunBuilder { xml: String::new(), at_space: rb.at_space, base_rpr: rb.base_rpr.clone(), para_style: rb.para_style.clone() };
                for part in child.children().filter(|c| is(*c, "ruby-base")) {
                    inline_content(part, ctx, &mut base, style_id);
                }
                let mut text = RunBuilder { xml: String::new(), at_space: true, base_rpr: rb.base_rpr.clone(), para_style: rb.para_style.clone() };
                let mut rt_style: Option<&str> = None;
                for part in child.children().filter(|c| is(*c, "ruby-text")) {
                    let sid = attr(part, "style-name").and_then(|n| ctx.styles.id("text", n));
                    rt_style = attr(part, "style-name");
                    inline_content(part, ctx, &mut text, sid.as_deref().or(style_id));
                }
                if text.xml.is_empty() {
                    rb.xml.push_str(&base.xml);
                } else {
                    // `w:rubyPr` has five required children (CT_RubyPr). Word writes them as measured by todocx on 034 (five rubies, base
                    // 12 pt / ruby 6 pt and base 9 pt / ruby 4.5 pt): `hps` is the size of the ruby text in half points (the ruby-text style's
                    // font size), `hpsBaseText` the size of the base (the span in the base, else the paragraph style, else the default style),
                    // `hpsRaise` is the base size less 2 (22 for 24, 16 for 18; this is also the raise the docx reader takes without it), and
                    // `lid` the Asian language of the base (the default style has ja / JP: `ja-JP`).
                    let base_span = child.children().find(|c| is(*c, "ruby-base")).and_then(|b| b.children().find(|c| is(*c, "span"))).and_then(|sp| attr(sp, "style-name"));
                    let styles = &*ctx.styles;
                    let default = |key: &str| styles.default_para.as_ref().and_then(|d| d.text.get(key).cloned());
                    let size = |first: Option<&str>, family: &str, key: &str| {
                        first
                            .and_then(|n| half_points(text_prop(styles, "text", Some(n), key).or_else(|| text_prop(styles, family, Some(n), key))))
                            .or_else(|| half_points(text_prop(styles, "paragraph", rb.para_style.as_deref(), key)))
                            .or_else(|| half_points(default(key)))
                    };
                    let base_half = size(base_span, "text", "font-size-asian").or_else(|| size(base_span, "text", "font-size")).unwrap_or(20);
                    let rt_half = size(rt_style, "text", "font-size-asian").or_else(|| size(rt_style, "text", "font-size")).unwrap_or(base_half / 2);
                    let lang = |first: Option<&str>, key: &str| {
                        first
                            .and_then(|n| text_prop(styles, "text", Some(n), key))
                            .or_else(|| text_prop(styles, "paragraph", rb.para_style.as_deref(), key))
                            .or_else(|| default(key))
                    };
                    let lid = match (lang(base_span, "language-asian"), lang(base_span, "country-asian")) {
                        (Some(l), Some(c)) if l != "none" => format!("{l}-{c}"),
                        (Some(l), None) if l != "none" => l,
                        _ => "ja-JP".to_string(),
                    };
                    let align = align.unwrap_or("center");
                    let pr = format!(
                        r#"<w:rubyPr><w:rubyAlign w:val="{align}"/><w:hps w:val="{rt_half}"/><w:hpsRaise w:val="{}"/><w:hpsBaseText w:val="{base_half}"/><w:lid w:val="{lid}"/></w:rubyPr>"#,
                        base_half.saturating_sub(2)
                    );
                    rb.xml.push_str(&format!("<w:r><w:ruby>{pr}<w:rt>{}</w:rt><w:rubyBase>{}</w:rubyBase></w:ruby></w:r>", text.xml, base.xml));
                }
                rb.at_space = base.at_space;
            }
            _ => inline_content(child, ctx, rb, style_id),
        }
    }
}

fn field(rb: &mut RunBuilder, style_id: Option<&str>, instr: &str, node: Node) {
    let shown: String = node.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect();
    let shown = if shown.trim().is_empty() { "1".to_string() } else { shown };
    run(rb, style_id, r#"<w:fldChar w:fldCharType="begin"/>"#);
    run(rb, style_id, &format!(r#"<w:instrText xml:space="preserve"> {instr} </w:instrText>"#));
    run(rb, style_id, r#"<w:fldChar w:fldCharType="separate"/>"#);
    text_run(rb, style_id, &shown);
    run(rb, style_id, r#"<w:fldChar w:fldCharType="end"/>"#);
    rb.at_space = false;
}

// ---------------------------------------------------------------------------------------------------------------------------------
// drawing objects: frames (pictures, text boxes), connectors and lines, anchored to a paragraph, the page or a character

fn percent_alpha(v: Option<&String>) -> String {
    match v.and_then(|s| s.trim_end_matches('%').parse::<f64>().ok()) {
        Some(p) if p < 100.0 => format!(r#"<a:alpha val="{}"/>"#, (p * 1000.0).round() as i64),
        _ => String::new(),
    }
}

fn fill_xml(g: &Props) -> String {
    match g.get("fill").map(String::as_str) {
        Some("solid") => {
            let c = g.get("fill-color").map(|c| c.trim_start_matches('#').to_uppercase()).unwrap_or_else(|| "729FCF".into());
            format!(r#"<a:solidFill><a:srgbClr val="{c}">{}</a:srgbClr></a:solidFill>"#, percent_alpha(g.get("opacity")))
        }
        _ => "<a:noFill/>".to_string(),
    }
}

/// The outline of a text box from the graphic style's `fo:border` ("0.04in double #000000"). Word reads that border as the shape's outline
/// (a Word-written ODF with `fo:border="0.03999in double #000000"` is `a:ln w="36566" cmpd="dbl"` in the docx Word saves from it; measured
/// by saving the ODT as docx); `none` or a border of width 0 is no outline.
fn border_line_xml(g: &Props) -> Option<String> {
    let border = g.get("border")?;
    let mut parts = border.split_whitespace();
    let (w, style, color) = (parts.next()?, parts.next()?, parts.next()?);
    let w = emu(w)?;
    if w <= 0 || style == "none" || style == "hidden" {
        return None;
    }
    let cmpd = if style == "double" { r#" cmpd="dbl""# } else { "" };
    let dash = match style {
        "dashed" => r#"<a:prstDash val="dash"/>"#,
        "dotted" => r#"<a:prstDash val="sysDot"/>"#,
        _ => r#"<a:prstDash val="solid"/>"#,
    };
    Some(format!(r#"<a:ln w="{w}"{cmpd}><a:solidFill><a:srgbClr val="{}"/></a:solidFill>{dash}</a:ln>"#, color.trim_start_matches('#').to_uppercase()))
}

fn line_xml(g: &Props) -> String {
    match g.get("stroke").map(String::as_str) {
        Some("none") | None => "<a:ln><a:noFill/></a:ln>".to_string(),
        Some(kind) => {
            let w = g.get("stroke-width").and_then(|v| emu(v)).unwrap_or(9525).max(1);
            let c = g.get("stroke-color").map(|c| c.trim_start_matches('#').to_uppercase()).unwrap_or_else(|| "000000".into());
            let dash = if kind == "dash" { r#"<a:prstDash val="dash"/>"# } else { "" };
            let cap = match g.get("stroke-linecap").map(String::as_str) {
                Some("round") => r#" cap="rnd""#,
                Some("square") => r#" cap="sq""#,
                _ => "",
            };
            format!(r#"<a:ln w="{w}"{cap}><a:solidFill><a:srgbClr val="{c}">{}</a:srgbClr></a:solidFill>{dash}</a:ln>"#, percent_alpha(g.get("stroke-opacity")))
        }
    }
}

/// `wp:anchor` (or `wp:inline`) holding `graphic`, placed as the graphic style `g` and the anchor type say.
#[allow(clippy::too_many_arguments)]
fn anchored(ctx: &mut Ctx, node: Node, g: &Props, x: i64, y: i64, cx: i64, cy: i64, graphic: &str, name: &str) -> String {
    ctx.drawing_id += 1;
    let id = ctx.drawing_id;
    let name = esc(name);
    let anchor_type = attr(node, "anchor-type").unwrap_or("paragraph");
    if anchor_type == "as-char" {
        return format!(r#"<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{id}" name="{name}"/><wp:cNvGraphicFramePr/>{graphic}</wp:inline></w:drawing>"#);
    }
    let rel_h = match g.get("horizontal-rel").map(String::as_str).unwrap_or(if anchor_type == "page" { "page" } else { "paragraph" }) {
        "page" | "page-start-margin" | "page-end-margin" => "page",
        "page-content" => "margin",
        "char" => "character",
        _ => "column",
    };
    let rel_v = match g.get("vertical-rel").map(String::as_str).unwrap_or(if anchor_type == "page" { "page" } else { "paragraph" }) {
        "page" | "page-start-margin" | "page-end-margin" => "page",
        "page-content" => "margin",
        "line" | "char" | "text" | "baseline" => "line",
        _ => "paragraph",
    };
    let pos_h = match g.get("horizontal-pos").map(String::as_str) {
        Some("center") => "<wp:align>center</wp:align>".to_string(),
        Some("right") | Some("outside") => "<wp:align>right</wp:align>".to_string(),
        Some("left") | Some("inside") => "<wp:align>left</wp:align>".to_string(),
        _ => format!("<wp:posOffset>{x}</wp:posOffset>"),
    };
    let pos_v = match g.get("vertical-pos").map(String::as_str) {
        Some("middle") => "<wp:align>center</wp:align>".to_string(),
        Some("bottom") => "<wp:align>bottom</wp:align>".to_string(),
        Some("top") => "<wp:align>top</wp:align>".to_string(),
        _ => format!("<wp:posOffset>{y}</wp:posOffset>"),
    };
    let square = matches!(g.get("wrap").map(String::as_str), Some("parallel") | Some("dynamic"));
    // `style:wrap="none"` (no text beside the frame) is `wrapTopAndBottom` in the docx Word saves from the ODT (measured).
    let wrap = if square {
        r#"<wp:wrapSquare wrapText="bothSides"/>"#
    } else if g.get("wrap").map(String::as_str) == Some("none") {
        "<wp:wrapTopAndBottom/>"
    } else {
        "<wp:wrapNone/>"
    };
    let behind = g.get("run-through").map(String::as_str) == Some("background") && !square;
    let z = attr(node, "z-index").and_then(|v| v.parse::<i64>().ok()).unwrap_or(id as i64).rem_euclid(4_000_000_000);
    format!(
        r#"<w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="{z}" behindDoc="{}" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="{rel_h}">{pos_h}</wp:positionH><wp:positionV relativeFrom="{rel_v}">{pos_v}</wp:positionV><wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>{wrap}<wp:docPr id="{id}" name="{name}"/><wp:cNvGraphicFramePr/>{graphic}</wp:anchor></w:drawing>"#,
        if behind { 1 } else { 0 }
    )
}

/// The position and the `a:xfrm` attribute of the turn of an object of `cx` x `cy` whose `draw:transform` says where it is (Word writes
/// `translate(-w/2 -h/2) rotate(a) translate(cx cy)` for a turned shape and takes `svg:x`/`svg:y` off).
fn transformed(node: Node, x: i64, y: i64, cx: i64, cy: i64) -> (i64, i64, String) {
    let Some(t) = attr(node, "transform") else { return (x, y, String::new()) };
    let (mx, my, angle) = crate::odp_shape::placed_box(t, cx as f64, cy as f64, emu);
    let rot = crate::odp_shape::ooxml_rotation(angle);
    (((mx - cx as f64 / 2.0).round()) as i64, ((my - cy as f64 / 2.0).round()) as i64, if rot != 0 { format!(r#" rot="{rot}""#) } else { String::new() })
}

fn drawing<'a>(node: Node<'a, 'a>, ctx: &mut Ctx<'a>, rb: &mut RunBuilder) {
    let g = attr(node, "style-name").map(|n| ctx.styles.graphic(n)).unwrap_or_default();
    let name = attr(node, "name").unwrap_or("Shape").to_string();
    let xml = match node.tag_name().name() {
        "frame" => {
            let (Some(cx), Some(cy)) = (attr(node, "width").and_then(emu), attr(node, "height").and_then(emu)) else { return };
            let x = attr(node, "x").and_then(emu).unwrap_or(0);
            let y = attr(node, "y").and_then(emu).unwrap_or(0);
            let (x, y, rot) = transformed(node, x, y, cx, cy);
            if let Some(image) = node.children().find(|c| is(*c, "image")) {
                let Some(href) = attr(image, "href") else { return };
                let Ok(Some(bytes)) = ctx.archive.read(href) else { return };
                let ext = href.rsplit('.').next().unwrap_or("png").to_lowercase();
                let file = format!("image{}.{}", ctx.images.len() + 1, ext);
                ctx.images.push(Image { name: file, bytes });
                let rid = format!("rIdI{}", ctx.images.len());
                let id = ctx.drawing_id + 1;
                // `fo:clip="rect(top, right, bottom, left)"` is the crop, in lengths of the picture at its own size (its pixels at its
                // density): Word writes it as `a:srcRect` in 1/100000 of that size (t 10654 for 1.14778 in of a 2370 px picture of 220 dpi).
                let src_rect = {
                    let inner = g.get("clip").and_then(|c| c.trim().strip_prefix("rect(")?.strip_suffix(')'));
                    let sides: Vec<i64> = inner.map(|i| i.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).filter_map(emu).collect()).unwrap_or_default();
                    let size = ctx.images.last().and_then(|i| crate::odp_shape::picture_size(&i.bytes));
                    match (size, sides.len()) {
                        (Some((pw, ph)), 4) => {
                            let frac = |v: i64, total: f64| ((v as f64 / (total * 12_700.0)) * 100_000.0).round().clamp(0.0, 99_000.0) as i64;
                            let (t, r, b, l) = (frac(sides[0], ph), frac(sides[1], pw), frac(sides[2], ph), frac(sides[3], pw));
                            [("l", l), ("t", t), ("r", r), ("b", b)].iter().filter(|(_, v)| *v != 0).fold(String::new(), |mut a, (k, v)| {
                                a.push_str(&format!(r#" {k}="{v}""#));
                                a
                            })
                        }
                        _ => String::new(),
                    }
                };
                let src_rect = if src_rect.is_empty() { String::new() } else { format!("<a:srcRect{src_rect}/>") };
                let graphic = format!(
                    r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="{0}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/>{src_rect}<a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>"#,
                    esc(&name)
                );
                anchored(ctx, node, &g, x, y, cx, cy, &graphic.replacen("<pic:spPr><a:xfrm>", &format!("<pic:spPr><a:xfrm{rot}>"), 1), &name)
            } else if let Some(tb) = node.children().find(|c| is(*c, "text-box")) {
                let mut body = String::new();
                blocks(tb, ctx, None, &mut body);
                if body.is_empty() {
                    body.push_str("<w:p/>");
                }
                let pad = |side: &str| g.get(&format!("padding-{side}")).or(g.get("padding")).and_then(|v| emu(v)).unwrap_or(0);
                let (fill, line) = (fill_xml(&g), border_line_xml(&g).unwrap_or_else(|| line_xml(&g)));
                let anchor = match g.get("textarea-vertical-align").map(String::as_str) {
                    Some("middle") => "ctr",
                    Some("bottom") => "b",
                    _ => "t",
                };
                let graphic = format!(
                    r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{fill}{line}</wps:spPr><wps:txbx><w:txbxContent>{body}</w:txbxContent></wps:txbx><wps:bodyPr lIns="{}" tIns="{}" rIns="{}" bIns="{}" anchor="{anchor}"/></wps:wsp></a:graphicData></a:graphic>"#,
                    pad("left"),
                    pad("top"),
                    pad("right"),
                    pad("bottom")
                );
                anchored(ctx, node, &g, x, y, cx, cy, &graphic.replacen("<wps:spPr><a:xfrm>", &format!("<wps:spPr><a:xfrm{rot}>"), 1), &name)
            } else {
                return;
            }
        }
        "custom-shape" => {
            let (Some(cx), Some(cy)) = (attr(node, "width").and_then(emu), attr(node, "height").and_then(emu)) else { return };
            let x = attr(node, "x").and_then(emu).unwrap_or(0);
            let y = attr(node, "y").and_then(emu).unwrap_or(0);
            let (x, y, rot) = transformed(node, x, y, cx, cy);
            let eg = node.children().find(|c| is(*c, "enhanced-geometry"));
            // The geometry as the equations and the path of the file give it for the shape's size (in 1/100 mm).
            let geometry = eg.and_then(|eg| crate::enhanced_geometry::evaluate_in(eg, (cx as f64 / 360.0, cy as f64 / 360.0), true));
            let (geom_xml, text_rect) = match &geometry {
                Some(geo) => {
                    let rect = geo.text_area.map(|(l, t, r, b)| {
                        let f = |v: f64, e: i64| (v * e as f64).round() as i64;
                        format!(r#"<a:rect l="{}" t="{}" r="{}" b="{}"/>"#, f(l, cx), f(t, cy), f(r, cx), f(b, cy))
                    });
                    (format!(r#"<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/>{}<a:pathLst>{}</a:pathLst></a:custGeom>"#, rect.as_deref().unwrap_or(r#"<a:rect l="0" t="0" r="r" b="b"/>"#), geo.paths), rect)
                }
                None => (r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#.to_string(), None),
            };
            let _ = text_rect;
            let has_text = node.children().any(|c| is(c, "p") || is(c, "h") || is(c, "list"));
            let mut body = String::new();
            if has_text {
                blocks(node, ctx, None, &mut body);
            }
            let pad = |side: &str| g.get(&format!("padding-{side}")).or(g.get("padding")).and_then(|v| emu(v)).unwrap_or(0);
            let (fill, line) = (fill_xml(&g), line_xml(&g));
            let anchor = match g.get("textarea-vertical-align").map(String::as_str) {
                Some("middle") => "ctr",
                Some("bottom") => "b",
                _ => "t",
            };
            let txbx = if body.is_empty() { String::new() } else { format!("<wps:txbx><w:txbxContent>{body}</w:txbxContent></wps:txbx>") };
            let graphic = format!(
                r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>{geom_xml}{fill}{line}</wps:spPr>{txbx}<wps:bodyPr lIns="{}" tIns="{}" rIns="{}" bIns="{}" anchor="{anchor}"/></wps:wsp></a:graphicData></a:graphic>"#,
                pad("left"),
                pad("top"),
                pad("right"),
                pad("bottom")
            );
            anchored(ctx, node, &g, x, y, cx, cy, &graphic.replacen("<wps:spPr><a:xfrm>", &format!("<wps:spPr><a:xfrm{rot}>"), 1), &name)
        }
        "connector" | "line" => {
            let get = |k: &str| attr(node, k).and_then(emu);
            let (Some(x1), Some(y1), Some(x2), Some(y2)) = (get("x1"), get("y1"), get("x2"), get("y2")) else { return };
            let (x, y) = (x1.min(x2), y1.min(y2));
            let (cx, cy) = ((x2 - x1).abs(), (y2 - y1).abs());
            let flip = format!("{}{}", if x2 < x1 { r#" flipH="1""# } else { "" }, if y2 < y1 { r#" flipV="1""# } else { "" });
            let line = line_xml(&g);
            let graphic = format!(
                r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvCnPr/><wps:spPr><a:xfrm{flip}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="line"><a:avLst/></a:prstGeom>{line}</wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic>"#
            );
            anchored(ctx, node, &g, x, y, cx, cy, &graphic, &name)
        }
        "g" => {
            for child in node.children().filter(|c| c.is_element()) {
                drawing(child, ctx, rb);
            }
            return;
        }
        _ => return,
    };
    run(rb, None, &xml);
    rb.at_space = false;
}

fn paragraph<'a>(node: Node<'a, 'a>, ctx: &mut Ctx<'a>, list: Option<(usize, usize)>, out: &mut String) {
    let style = attr(node, "style-name").and_then(|n| ctx.styles.get("paragraph", n));
    let heading = node.tag_name().name() == "h";
    let level = attr(node, "outline-level").and_then(|v| v.parse::<i64>().ok()).unwrap_or(1).clamp(1, 9);
    // A heading whose style is not one of the file's heading styles (an automatic style of its own, or none) is the built-in heading of
    // its level, its own paragraph properties set on the paragraph.
    // A heading in a style of the file that is not a heading style (Word writes `text:h` in its Normal style, or an automatic style
    // of it, for a paragraph that only has an outline level) is formatted by that style alone; one without a style is the built-in heading.
    // A style whose parent chain reaches `Heading N` (ODF's default heading styles, whose names the spec's office applications use for
    // outline levels; the parent may be named without being defined in the file) is a heading style: the built-in heading again.
    let builtin = heading && style.is_none_or(|s| ctx.styles.is_heading(s));
    let mut head = String::new();
    let mut after_num = String::new();
    if builtin {
        head.push_str(&format!(r#"<w:pStyle w:val="Heading{}"/>"#, level.min(6)));
        if let Some(s) = style {
            after_num.push_str(&ppr(&s.para, &s.tabs, list.is_some()));
        }
    } else if let Some(s) = style {
        head.push_str(&format!(r#"<w:pStyle w:val="{}"/>"#, ctx.styles.id("paragraph", &s.name).unwrap_or_default()));
    }
    // An automatic style is direct formatting of the paragraph on its named parent style: the paragraphs of an ODF written by Word each
    // have an automatic style of their own, but Word reads them as one style (so contextual spacing leaves no space between them: 016's
    // title and date lines), and writes the paragraph properties and the text properties straight into the paragraph and its runs.
    let list_ind: Option<(i64, i64)> = if list.is_none() {
        style.and_then(|s| {
            let ls = ctx.styles.chain_list_style(s)?.to_string();
            if ctx.styles.chain_has_para(s, "margin-left") || ctx.styles.chain_has_para(s, "text-indent") {
                return None;
            }
            let node = ctx.list_nodes.get(&ls).copied()?;
            list_levels(node, ctx.styles).first().map(|l| (l.left, l.hanging.max(0)))
        })
    } else {
        None
    };
    let mut base_rpr = String::new();
    let mut auto_direct = String::new();
    let mut column_break = false;
    if !builtin {
        if let Some(s) = style.filter(|s| s.automatic) {
            let mut chain = Vec::new();
            let mut cur = Some(s);
            while let Some(c) = cur.filter(|c| c.automatic && chain.len() < 16) {
                chain.push(c);
                cur = c.parent.as_deref().and_then(|p| ctx.styles.get(&c.family, p));
            }
            let mut para = Props::new();
            let mut text = Props::new();
            let mut tabs: Vec<Tab> = Vec::new();
            let mut effective: Vec<Tab> = cur.map(|r| r.tabs.clone()).unwrap_or_default();
            let mut clears = ctx.styles.inherited_clears("paragraph", cur.and_then(|r| r.parent.as_deref()));
            if effective.is_empty() && cur.is_some_and(|r| !r.tabs_cleared) {
                effective = clears.iter().map(|t| Tab { kind: "left".into(), ..t.clone() }).collect();
            }
            clears.clear();
            for c in chain.iter().rev() {
                para.extend(c.para.iter().map(|(k, v)| (k.clone(), v.clone())));
                text.extend(c.text.iter().map(|(k, v)| (k.clone(), v.clone())));
                if !c.tabs.is_empty() {
                    tabs = c.tabs.clone();
                    effective = c.tabs.clone();
                } else if c.tabs_cleared {
                    // Word writes `<w:tab w:val="clear">` for every tab stop it drops (034: 34 paragraphs, measured with -SaveAs todocx).
                    tabs = effective.iter().map(|t| Tab { kind: "clear".into(), leader: None, ..t.clone() }).collect();
                    effective.clear();
                }
            }
            head.clear();
            if let Some(root) = cur {
                head.push_str(&format!(r#"<w:pStyle w:val="{}"/>"#, ctx.styles.id("paragraph", &root.name).unwrap_or_default()));
            }
            if let Some((l, h)) = list_ind {
                para.insert("margin-left".into(), format!("{}pt", l as f64 / 20.0));
                para.insert("text-indent".into(), format!("{}pt", -(h as f64) / 20.0));
            }
            // Not in a list, so the list style gives no indent: the paragraph's own indent is written out.
            column_break = para.get("break-before").map(String::as_str) == Some("column");
            auto_direct = ppr(&para, &tabs, false);
            base_rpr = rpr(&text, ctx.styles);
        }
    }
    let mut direct = String::new();
    if std::mem::take(&mut ctx.pending_break) {
        direct.push_str("<w:pageBreakBefore/>");
    }
    if let Some((ilvl, num)) = list {
        direct.push_str(&format!(r#"<w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num}"/></w:numPr>"#));
    }
    // A paragraph outside any `text:list` whose style has a list style is not numbered: Word reads it with `w:numId 0` (measured with
    // -SaveAs todocx: Word writes numId 0 there) and takes the indent of the list style's first level when the style chain sets no
    // `fo:margin-left` itself (846 / 420 twips for a level of margin-left 0.5875in and text-indent -0.2916in).
    if list.is_none() && style.is_some_and(|s| ctx.styles.chain_list_style(s).is_some()) {
        direct.push_str(r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="0"/></w:numPr>"#);
    }
    if auto_direct.is_empty() {
        if let Some((l, h)) = list_ind {
            direct.push_str(&format!(r#"<w:ind w:left="{l}" w:hanging="{h}"/>"#));
        }
    }
    direct.push_str(&after_num);
    direct.push_str(&auto_direct);
    // The paragraph whose style names a master page starts a section (the section break is its page break): Word's .docx of an .odt
    // has no `w:pageBreakBefore` on it (measured with -SaveAs todocx on a document with 4 master pages and 6 `fo:break-before="page"`:
    // the 4 paragraphs that name a master page have no `w:pageBreakBefore`, the 2 that only break have it; ours had 6 and 2 pages more).
    if ctx.in_cell > 0 || master_of(node, ctx.styles).is_some() {
        // Word reads `fo:break-before="page"` of a paragraph in a table cell as no page break (measured with -SaveAs todocx: the .docx
        // has no `w:pageBreakBefore` in the cells whose ODF paragraph styles break before).
        direct = direct.replace("<w:pageBreakBefore/>", "");
        if style.is_some_and(|s| !s.automatic && s.para.get("break-before").map(String::as_str) == Some("page")) {
            direct.push_str(r#"<w:pageBreakBefore w:val="0"/>"#);
        }
    }
    if heading {
        direct.push_str(&format!(r#"<w:outlineLvl w:val="{}"/>"#, level - 1));
    }
    // The paragraph mark has the text properties too (an empty paragraph is as high as its mark's font).
    let mark = if base_rpr.is_empty() { String::new() } else { format!("<w:rPr>{base_rpr}</w:rPr>") };
    let ppr_all = format!("{head}{direct}{mark}");
    let mut rb = RunBuilder { xml: String::new(), at_space: true, base_rpr, para_style: style.map(|s| s.name.clone()) };
    // `fo:break-before="column"` is a column break as the first run of the paragraph (measured with todocx on 083: `<w:br w:type="column"/>`;
    // the section has one column, so Word breaks the page there). Table cells are left as they were (not measured).
    // A paragraph that names a master page starts a section and has none (025: Word's docx has no column break there).
    if column_break && ctx.in_cell == 0 && master_of(node, ctx.styles).is_none() {
        run(&mut rb, None, r#"<w:br w:type="column"/>"#);
    }
    inline_content(node, ctx, &mut rb, None);
    out.push_str("<w:p>");
    if !ppr_all.is_empty() {
        out.push_str(&format!("<w:pPr>{ppr_all}</w:pPr>"));
    }
    out.push_str(&rb.xml);
    out.push_str("</w:p>");
}

fn blocks<'a>(parent: Node<'a, 'a>, ctx: &mut Ctx<'a>, list: Option<(usize, usize)>, out: &mut String) {
    for child in parent.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "p" | "h" => paragraph(child, ctx, list, out),
            "list" => list_block(child, ctx, list, out),
            "table" => table(child, ctx, out),
            "section" | "index-body" | "table-of-content" | "alphabetical-index" | "illustration-index" | "table-index" | "user-index" | "bibliography" | "index-title" => blocks(child, ctx, list, out),
            _ => {}
        }
    }
}

fn list_block<'a>(node: Node<'a, 'a>, ctx: &mut Ctx<'a>, outer: Option<(usize, usize)>, out: &mut String) {
    let (depth, num) = match outer {
        Some((d, n)) => (d + 1, Some(n)),
        None => {
            let style = attr(node, "style-name").unwrap_or("");
            let cont = attr(node, "continue-numbering") == Some("true") || attr(node, "continue-list").is_some();
            (0, ctx.num_for(style, !cont))
        }
    };
    let Some(num) = num else {
        for item in node.children().filter(|c| is(*c, "list-item") || is(*c, "list-header")) {
            blocks(item, ctx, None, out);
        }
        return;
    };
    for item in node.children().filter(|c| is(*c, "list-item") || is(*c, "list-header")) {
        let header = is(item, "list-header");
        blocks(item, ctx, if header { None } else { Some((depth, num)) }, out);
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// tables

fn table<'a>(node: Node<'a, 'a>, ctx: &mut Ctx<'a>, out: &mut String) {
    let tstyle = attr(node, "style-name").and_then(|n| ctx.styles.get("table", n));
    if tstyle.is_some_and(|t| t.table.get("break-before").map(String::as_str) == Some("page")) {
        ctx.pending_break = true;
    }
    // Columns.
    let mut cols: Vec<i64> = Vec::new();
    // Whether each column has `style:use-optimal-column-width` (its own style, else the default style of the family, else false).
    let mut optimal: Vec<bool> = Vec::new();
    fn collect_cols<'a>(n: Node<'a, 'a>, ctx: &Ctx<'a>, cols: &mut Vec<i64>, optimal: &mut Vec<bool>) {
        for c in n.children().filter(|c| c.is_element()) {
            match c.tag_name().name() {
                "table-column" => {
                    let repeat = attr(c, "number-columns-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(1024);
                    let w = attr(c, "style-name").and_then(|s| ctx.styles.get("table-column", s)).and_then(|s| s.column.get("column-width")).and_then(|w| twips(w)).unwrap_or(2000);
                    let opt = attr(c, "style-name")
                        .and_then(|s| ctx.styles.get("table-column", s))
                        .and_then(|s| s.column.get("use-optimal-column-width"))
                        .map(|v| v == "true")
                        .or(ctx.styles.default_optimal_column)
                        .unwrap_or(false);
                    for _ in 0..repeat {
                        cols.push(w);
                        optimal.push(opt);
                    }
                }
                "table-columns" | "table-header-columns" => collect_cols(c, ctx, cols, optimal),
                _ => {}
            }
        }
    }
    collect_cols(node, ctx, &mut cols, &mut optimal);
    if cols.is_empty() {
        cols.push(2000);
        optimal.push(false);
    }
    let total: i64 = cols.iter().sum();
    out.push_str("<w:tbl><w:tblPr>");
    let width = tstyle.and_then(|t| t.table.get("width")).and_then(|w| twips(w)).unwrap_or(total);
    out.push_str(&format!(r#"<w:tblW w:w="{width}" w:type="dxa"/>"#));
    let ind = tstyle.and_then(|t| t.table.get("margin-left")).and_then(|w| twips(w)).unwrap_or(0);
    match tstyle.and_then(|t| t.table.get("align")).map(String::as_str) {
        Some("center") => out.push_str(r#"<w:jc w:val="center"/>"#),
        Some("right") => out.push_str(r#"<w:jc w:val="right"/>"#),
        _ => {
            if ind != 0 {
                out.push_str(&format!(r#"<w:tblInd w:w="{ind}" w:type="dxa"/>"#));
            }
        }
    }
    // Word reads a table as autofit when every column is `use-optimal-column-width` and as fixed when any column is not (measured with
    // todocx: all `true` -> autofit; one `false` or unset (no default style) among them -> fixed; the default style of the
    // table-column family that Word writes into styles.xml is `true`, so its tables without the attribute are autofit).
    if !optimal.iter().all(|o| *o) {
        out.push_str(r#"<w:tblLayout w:type="fixed"/>"#);
    }
    out.push_str(r#"<w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="0" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid>"#);
    for w in &cols {
        out.push_str(&format!(r#"<w:gridCol w:w="{w}"/>"#));
    }
    out.push_str("</w:tblGrid>");
    // vertical spans still running: per column (rows left, width in columns).
    let mut vspan: Vec<(usize, usize, String)> = vec![(0, 1, String::new()); cols.len() + 1];
    fn rows<'a>(n: Node<'a, 'a>, ctx: &mut Ctx<'a>, cols: &[i64], vspan: &mut Vec<(usize, usize, String)>, header: bool, out: &mut String) {
        for r in n.children().filter(|c| c.is_element()) {
            match r.tag_name().name() {
                "table-header-rows" | "table-rows" | "table-row-group" => rows(r, ctx, cols, vspan, header || r.tag_name().name() == "table-header-rows", out),
                "table-row" => {
                    let repeat = attr(r, "number-rows-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(256);
                    for _ in 0..repeat {
                        row(r, ctx, cols, vspan, header, out);
                    }
                }
                _ => {}
            }
        }
    }
    fn row<'a>(r: Node<'a, 'a>, ctx: &mut Ctx<'a>, cols: &[i64], vspan: &mut Vec<(usize, usize, String)>, header: bool, out: &mut String) {
        out.push_str("<w:tr><w:trPr>");
        if let Some(s) = attr(r, "style-name").and_then(|s| ctx.styles.get("table-row", s)) {
            if let Some(h) = s.row.get("row-height").and_then(|v| twips(v)) {
                out.push_str(&format!(r#"<w:trHeight w:val="{h}" w:hRule="exact"/>"#));
            } else if let Some(h) = s.row.get("min-row-height").and_then(|v| twips(v)).filter(|h| *h > 0) {
                out.push_str(&format!(r#"<w:trHeight w:val="{h}" w:hRule="atLeast"/>"#));
            }
            if s.row.get("keep-together").map(String::as_str) == Some("always") {
                out.push_str("<w:cantSplit/>");
            }
        }
        if header {
            out.push_str("<w:tblHeader/>");
        }
        out.push_str("</w:trPr>");
        let mut col = 0usize;
        let mut skip = 0usize;
        let mut cells = r.children().filter(|c| is(*c, "table-cell") || is(*c, "covered-table-cell")).collect::<Vec<_>>().into_iter();
        while col < cols.len() {
            // A cell below a vertical span: continue it.
            if vspan[col].0 > 0 && skip == 0 {
                let (left, w, props) = vspan[col].clone();
                let width: i64 = cols[col..(col + w).min(cols.len())].iter().sum();
                // Word writes the borders, margins and vertical alignment of the span's first cell into each cell below it as well
                // (the covered cells of the .odt carry no style; measured by saving an .odt as .docx in Word: every continuation cell
                // has the first cell's `w:tcBorders`, `w:tcMar` and `w:vAlign`, and the table's outer border shows along the span).
                out.push_str(&format!(r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{}<w:vMerge/>{props}</w:tcPr><w:p/></w:tc>"#, if w > 1 { format!(r#"<w:gridSpan w:val="{w}"/>"# ) } else { String::new() }));
                for k in col..(col + w).min(cols.len()) {
                    vspan[k].0 = left - 1;
                    vspan[k].2 = props.clone();
                }
                // The covered cells of this span in the row are in the file; take them.
                for _ in 0..w {
                    cells.next();
                }
                col += w;
                continue;
            }
            let Some(cell) = cells.next() else { break };
            if is(cell, "covered-table-cell") {
                // The columns of a horizontal span are already counted with its cell; a covered cell outside one is a column.
                if skip > 0 {
                    skip -= 1;
                } else {
                    col += 1;
                }
                continue;
            }
            let cs = attr(cell, "number-columns-spanned").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1);
            let rs = attr(cell, "number-rows-spanned").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1);
            let repeat = attr(cell, "number-columns-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).clamp(1, 1024);
            for _ in 0..repeat {
                if col >= cols.len() {
                    break;
                }
                let span = cs.min(cols.len() - col);
                let width: i64 = cols[col..col + span].iter().sum();
                out.push_str(&format!(r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>"#));
                if span > 1 {
                    out.push_str(&format!(r#"<w:gridSpan w:val="{span}"/>"#));
                }
                if rs > 1 {
                    out.push_str(r#"<w:vMerge w:val="restart"/>"#);
                }
                let mut vprops = String::new();
                if let Some(s) = attr(cell, "style-name").and_then(|s| ctx.styles.get("table-cell", s)) {
                    let b = borders(&s.cell, "tcBorders");
                    out.push_str(&b);
                    vprops.push_str(&b);
                    out.push_str(&shade(&s.cell));
                    // `style:writing-mode="tb-rl"` of a cell is `w:textDirection tbRlV` (measured: Word saving an .odt as .docx).
                    if s.cell.get("writing-mode").map(String::as_str) == Some("tb-rl") {
                        out.push_str(r#"<w:textDirection w:val="tbRlV"/>"#);
                        vprops.push_str(r#"<w:textDirection w:val="tbRlV"/>"#);
                    }
                    match s.cell.get("vertical-align").map(String::as_str) {
                        Some("middle") => out.push_str(r#"<w:vAlign w:val="center"/>"#),
                        Some("bottom") => out.push_str(r#"<w:vAlign w:val="bottom"/>"#),
                        _ => {}
                    }
                    let pad = |side: &str| s.cell.get(&format!("padding-{side}")).or(s.cell.get("padding")).and_then(|v| twips(v));
                    let mut mar = String::new();
                    for (side, tag) in [("top", "top"), ("left", "left"), ("bottom", "bottom"), ("right", "right")] {
                        if let Some(v) = pad(side) {
                            mar.push_str(&format!(r#"<w:{tag} w:w="{v}" w:type="dxa"/>"#));
                        }
                    }
                    if !mar.is_empty() {
                        out.push_str(&format!("<w:tcMar>{mar}</w:tcMar>"));
                        vprops.push_str(&format!("<w:tcMar>{mar}</w:tcMar>"));
                    }
                    match s.cell.get("vertical-align").map(String::as_str) {
                        Some("middle") => vprops.push_str(r#"<w:vAlign w:val="center"/>"#),
                        Some("bottom") => vprops.push_str(r#"<w:vAlign w:val="bottom"/>"#),
                        _ => {}
                    }
                }
                if rs > 1 {
                    for k in col..col + span {
                        vspan[k] = (rs - 1, span, vprops.clone());
                    }
                }
                out.push_str("</w:tcPr>");
                let mut body = String::new();
                ctx.in_cell += 1;
                blocks(cell, ctx, None, &mut body);
                ctx.in_cell -= 1;
                if body.is_empty() || !body.ends_with("</w:p>") {
                    body.push_str("<w:p/>");
                }
                out.push_str(&body);
                out.push_str("</w:tc>");
                col += span;
            }
            skip = cs - 1;
        }
        out.push_str("</w:tr>");
    }
    rows(node, ctx, &cols, &mut vspan, false, out);
    out.push_str("</w:tbl>");
    // Two tables one after the other would be one table in the .docx: a paragraph keeps them apart.
    if node.next_siblings().skip(1).find(|n| n.is_element()).is_some_and(|n| is(n, "table")) {
        out.push_str("<w:p/>");
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------

struct Page {
    width: i64,
    height: i64,
    landscape: bool,
    top: i64,
    bottom: i64,
    left: i64,
    right: i64,
    header_distance: i64,
    footer_distance: i64,
    grid: Option<(&'static str, i64, i64)>,
    /// `w:cols` of the page's columns (`style:columns`, ODF 1.3 20.xxx): the number of columns, the gap, and the separator line.
    cols: Option<String>,
}

/// The page of a master page (the first one when `master_name` is none or unknown), and the master page's header and footer.
fn read_page<'a>(styles_doc: Option<&'a Document<'a>>, master_name: Option<&str>) -> (Page, Option<Node<'a, 'a>>, Option<Node<'a, 'a>>) {
    let mut page = Page { width: 11906, height: 16838, landscape: false, top: 1440, bottom: 1440, left: 1440, right: 1440, header_distance: 720, footer_distance: 720, grid: None, cols: None };
    let (mut header_node, mut footer_node): (Option<Node>, Option<Node>) = (None, None);
    if let Some(sd) = styles_doc {
        let master = sd
            .descendants()
            .filter(|n| is(*n, "master-page"))
            .find(|n| master_name.is_none_or(|m| attr(*n, "name") == Some(m)))
            .or_else(|| sd.descendants().find(|n| is(*n, "master-page")));
        if let Some(m) = master {
            header_node = m.children().find(|c| is(*c, "header"));
            footer_node = m.children().find(|c| is(*c, "footer"));
            let layout = attr(m, "page-layout-name").and_then(|ln| sd.descendants().find(|n| is(*n, "page-layout") && attr(*n, "name") == Some(ln)));
            if let Some(l) = layout {
                let p = props_of(l, "page-layout-properties");
                let tw = |k: &str| p.get(k).and_then(|v| twips(v));
                page.width = tw("page-width").unwrap_or(page.width);
                page.height = tw("page-height").unwrap_or(page.height);
                page.landscape = p.get("print-orientation").map(String::as_str) == Some("landscape");
                // A negative `fo:margin-top` / `fo:margin-bottom` (Word writes a docx's negative, "exact" margin so) is not a valid
                // length for a margin: Word reads it as its default margin, 3.5 cm (1985 twips) at the top and 3.0 cm (1701) at the
                // bottom, and the header and the footer at 720 (measured: margin-top -1.5, -0.709, -0.4, -0.2 and -0.05 in all give
                // `w:pgMar w:top="1985" w:header="720"`; margin-bottom -1.5, -0.709 and -0.1 in give `w:bottom="1701"`; 0 gives 0).
                let (mut top, mut bottom) = (tw("margin-top").unwrap_or(0), tw("margin-bottom").unwrap_or(0));
                let (negative_top, negative_bottom) = (top < 0, bottom < 0);
                if negative_top {
                    top = 1985;
                }
                if negative_bottom {
                    bottom = 1701;
                }
                page.left = tw("margin-left").unwrap_or(page.left);
                page.right = tw("margin-right").unwrap_or(page.right);
                page.top = top;
                page.bottom = bottom;
                page.header_distance = if negative_top { 720 } else { top };
                page.footer_distance = if negative_bottom { 720 } else { bottom };
                if let Some(c) = l.children().find(|c| is(*c, "page-layout-properties")).and_then(|pp| pp.children().find(|c| is(*c, "columns"))) {
                    let count = attr(c, "column-count").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1);
                    if count > 1 {
                        let sep = if c.children().any(|x| is(x, "column-sep")) { r#" w:sep="1""# } else { "" };
                        let listed: Vec<Node> = c.children().filter(|x| is(*x, "column")).collect();
                        let body_width = (page.width - page.left - page.right).max(1);
                        if listed.len() == count && count > 0 {
                            // Columns of their own widths: the relative width (`n*`) includes the column's padding.
                            let rel = |x: &Node| attr(*x, "rel-width").and_then(|v| v.trim_end_matches('*').parse::<f64>().ok()).unwrap_or(1.0);
                            let total: f64 = listed.iter().map(rel).sum::<f64>().max(1.0);
                            let pad = |x: &Node, k: &str| attr(*x, k).and_then(twips).unwrap_or(0);
                            let mut cols = String::new();
                            for (i, x) in listed.iter().enumerate() {
                                let share = (body_width as f64 * rel(x) / total).round() as i64;
                                let width = (share - pad(x, "start-indent") - pad(x, "end-indent")).max(1);
                                let space = pad(x, "end-indent") + listed.get(i + 1).map_or(0, |n| pad(n, "start-indent"));
                                cols.push_str(&format!(r#"<w:col w:w="{width}" w:space="{space}"/>"#));
                            }
                            page.cols = Some(format!(r#"<w:cols w:num="{count}"{sep} w:equalWidth="0">{cols}</w:cols>"#));
                        } else {
                            let gap = attr(c, "column-gap").and_then(twips).unwrap_or(720);
                            page.cols = Some(format!(r#"<w:cols w:num="{count}" w:space="{gap}"{sep}/>"#));
                        }
                    }
                }
                let hf = |tag: &str| l.children().find(|c| is(*c, tag)).map(|h| props_of(h, "header-footer-properties"));
                // A page layout without `style:header-style` (or `style:footer-style`) has Word's default header (footer) distance, 720
                // twips, and the margin stays the body's own (measured with -SaveAs todocx: a layout with margin-top 0.7875in and neither
                // style gives `w:top="1134" w:header="720"`; one with margin-top 0.5in, header-style min-height 0.0909in gives
                // `w:top="851" w:header="720"`).
                if hf("header-style").is_none() {
                    page.header_distance = 720;
                }
                if hf("footer-style").is_none() {
                    page.footer_distance = 720;
                }
                // Word writes the page's top and bottom margins as the distance of the header and of the footer from the page edge
                // (`fo:margin-top`, `fo:margin-bottom`), and the rest of them as the height of the header and of the footer
                // (`fo:min-height`, plus the gap `fo:margin-bottom` of the header and `fo:margin-top` of the footer), whether or not the
                // master page has a header or a footer: Word reads them back as the margins (its PDF of the file shows it).
                if let Some(h) = hf("header-style") {
                    let min = h.get("min-height").and_then(|v| twips(v)).unwrap_or(0);
                    let gap = h.get("margin-bottom").and_then(|v| twips(v)).unwrap_or(0);
                    page.top = top + min + gap;
                }
                if let Some(f) = hf("footer-style") {
                    let min = f.get("min-height").and_then(|v| twips(v)).unwrap_or(0);
                    let gap = f.get("margin-top").and_then(|v| twips(v)).unwrap_or(0);
                    page.bottom = bottom + min + gap;
                }
                // The layout grid: lines (`line`) or lines and characters (`both`), the lines' pitch the page's height inside the page
                // layout's own margins (`fo:margin-top`, `fo:margin-bottom`; the header and the footer are inside them, ODF 1.3 20.219
                // `style:layout-grid-lines`: the number of lines per page) over the number of lines. Word reads the file so: the pitch of a
                // docx whose body is 38.77 lines of 18 pt is stored as 38 lines and comes back as 756.9 pt / 38 = 19.9 pt.
                if let Some(lines) = p.get("layout-grid-lines").and_then(|v| v.parse::<i64>().ok()).filter(|n| *n > 0) {
                    let kind = match p.get("layout-grid-mode").map(String::as_str) {
                        Some("line") => Some("lines"),
                        Some("both") => Some("linesAndChars"),
                        _ => None,
                    };
                    if let Some(kind) = kind {
                        let height = page.height - top - bottom;
                        // `style:layout-grid-base-width` (`both` only) is the docx's `w:docGrid/@w:charSpace`, the character pitch Word adds to
                        // every character. Word converts between the two with the length in EMU: base width (in) x 914400, truncated
                        // toward zero. Measured by saving 28 odt files with base widths -0.005..0.1 in as .docx in Word and reading
                        // `charSpace` back (0.0002 in -> 182 (182.88), 0.0044 -> 4023 (4023.36), 0.01 -> 9144, 0.1 -> 91440, -0.001 -> -914;
                        // the same in two documents with different default font sizes). It agrees with what Word wrote for 30 docx
                        // files: charSpace 4096 -> 0.0044 in, 4290 -> 0.0046, 6651 -> 0.0072, -4292 -> -0.0046 (cs / 914400 truncated to
                        // 4 decimals).
                        let char_space = if kind == "linesAndChars" {
                            p.get("layout-grid-base-width").and_then(|v| twips_f(v)).map(|t| (t * 635.0 + 1e-6).trunc() as i64).unwrap_or(0)
                        } else {
                            0
                        };
                        page.grid = Some((kind, height / lines, char_space));
                    }
                }
            }
        }
    }
    (page, header_node, footer_node)
}

/// The master page that the style of a paragraph (or the style of a table) names, directly or through its parents.
fn master_of<'a>(node: Node<'a, 'a>, styles: &Styles) -> Option<String> {
    let (family, node) = match node.tag_name().name() {
        "p" | "h" => ("paragraph", node),
        "table" => ("table", node),
        "list" => ("paragraph", node.descendants().find(|n| is(*n, "p") || is(*n, "h"))?),
        _ => return None,
    };
    let mut cur = attr(node, "style-name").and_then(|n| styles.get(family, n));
    let mut guard = 0;
    while let Some(s) = cur {
        if let Some(m) = &s.master_page {
            return Some(m.clone());
        }
        guard += 1;
        if guard > 16 {
            break;
        }
        cur = s.parent.as_deref().and_then(|p| styles.get(family, p));
    }
    None
}

/// The sections of the body: each starts at a block whose style names a master page (the first one at the first block).
struct Sections {
    /// Per section: the master page, where it begins in the body's XML, where its last paragraph begins, and the section's own
    /// properties when it comes from a `text:section` (or follows one).
    list: Vec<(Option<String>, usize, Option<usize>, Option<SecProps>)>,
    /// The next block starts a section after a `text:section`.
    pending: bool,
}

/// What a `text:section` changes in the page of the section (ODF 1.3 20.xxx `style:section-properties`): Word writes a section of its
/// own, continuing on the same page, with the page margins plus the section's margins (measured: page left 340 + section
/// `fo:margin-left` 0.118in = 170 twips -> `w:pgMar w:left="510"`; page right 397 + (-0.0041in = -6) -> 391) and the section's columns.
#[derive(Clone, Default)]
struct SecProps {
    left: i64,
    right: i64,
    cols: Option<String>,
    continuous: bool,
}

fn body_blocks<'a>(parent: Node<'a, 'a>, ctx: &mut Ctx<'a>, out: &mut String, st: &mut Sections) {
    for child in parent.children().filter(|c| c.is_element()) {
        let name = child.tag_name().name();
        if name == "section" {
            let sec = attr(child, "style-name").and_then(|n| ctx.styles.get("section", n));
            let props = sec.and_then(|s| {
                let tw = |k: &str| s.section.get(k).and_then(|v| twips(v)).unwrap_or(0);
                let p = SecProps { left: tw("margin-left"), right: tw("margin-right"), cols: s.section_cols.clone(), continuous: !st.list.is_empty() };
                (p.left != 0 || p.right != 0 || p.cols.is_some()).then_some(p)
            });
            if let Some(p) = props {
                let master = st.list.last().and_then(|e| e.0.clone());
                match st.list.last_mut() {
                    Some(last) if last.1 == out.len() => *last = (master, out.len(), None, Some(p)),
                    _ => st.list.push((master, out.len(), None, Some(p))),
                }
                st.pending = false;
                body_blocks(child, ctx, out, st);
                st.pending = true;
            } else {
                body_blocks(child, ctx, out, st);
            }
            continue;
        }
        if matches!(name, "index-body" | "table-of-content" | "alphabetical-index" | "illustration-index" | "table-index" | "user-index" | "bibliography" | "index-title") {
            body_blocks(child, ctx, out, st);
            continue;
        }
        if !matches!(name, "p" | "h" | "list" | "table") {
            continue;
        }
        let m = master_of(child, ctx.styles);
        if st.list.is_empty() {
            st.list.push((m, out.len(), None, None));
        } else if m.is_some() {
            match st.list.last_mut() {
                Some(last) if last.1 == out.len() && last.3.is_some() => last.0 = m,
                _ => st.list.push((m, out.len(), None, None)),
            }
        } else if st.pending {
            let master = st.list.last().and_then(|e| e.0.clone());
            st.list.push((master, out.len(), None, Some(SecProps { continuous: true, ..Default::default() })));
        }
        st.pending = false;
        let start = out.len();
        match name {
            "p" | "h" => {
                paragraph(child, ctx, None, out);
                if let Some(last) = st.list.last_mut() {
                    last.2 = Some(start);
                }
            }
            "list" => {
                list_block(child, ctx, None, out);
                if let Some(last) = st.list.last_mut() {
                    last.2 = None;
                }
            }
            _ => {
                table(child, ctx, out);
                if let Some(last) = st.list.last_mut() {
                    last.2 = None;
                }
            }
        }
    }
}

/// The text of the document as `.docx` package bytes.
pub fn import(data: &[u8]) -> Result<Vec<u8>, Error> {
    let archive = Archive::new(data).map_err(|e| Error::new(format!("ODT アーカイブを開けません: {e}")))?; // i18n-ok（読めないファイルの理由）
    let read = |name: &str| -> Result<Option<String>, Error> {
        match archive.read(name)? {
            Some(b) => Ok(Some(String::from_utf8(b).map_err(|e| Error::new(format!("ODT {name} UTF-8 エラー: {e}")))?)), // i18n-ok
            None => Ok(None),
        }
    };
    let content_text = read("content.xml")?.ok_or_else(|| Error::new("ODT に content.xml がありません"))?; // i18n-ok
    let styles_text = read("styles.xml")?;
    let meta_text = read("meta.xml")?;
    let content = Document::parse(&content_text).map_err(|e| Error::new(format!("ODT content.xml 解析エラー: {e}")))?; // i18n-ok
    let styles_doc = match &styles_text {
        Some(t) => Some(Document::parse(t).map_err(|e| Error::new(format!("ODT styles.xml 解析エラー: {e}")))?), // i18n-ok
        None => None,
    };
    let mut docs: Vec<&Document> = Vec::new();
    if let Some(s) = &styles_doc {
        docs.push(s);
    }
    docs.push(&content);
    let styles = load_styles(&docs);

    let mut ctx = Ctx {
        styles: &styles,
        archive: &archive,
        list_styles: HashMap::new(),
        list_nodes: HashMap::new(),
        abstract_nums: Vec::new(),
        nums: Vec::new(),
        last_num: HashMap::new(),
        links: Vec::new(),
        images: Vec::new(),
        drawing_id: 0,
        bookmark_id: 0,
        footnotes: Vec::new(),
        endnotes: Vec::new(),
        in_cell: 0,
        pending_break: false,
    };
    for doc in &docs {
        for n in doc.descendants().filter(|n| is(*n, "list-style")) {
            if let Some(name) = attr(n, "name") {
                ctx.list_nodes.insert(name.to_string(), n);
            }
        }
    }

    // The sections and their pages: one page layout, header and footer per master page in use.
    let body = content.descendants().find(|n| is(*n, "text") && n.parent().is_some_and(|p| is(p, "body")));
    let mut body_xml = String::new();
    let mut sections = Sections { list: Vec::new(), pending: false };
    if let Some(b) = body {
        body_blocks(b, &mut ctx, &mut body_xml, &mut sections);
    }
    if sections.list.is_empty() {
        sections.list.push((None, 0, None, None));
    }
    if body_xml.is_empty() {
        body_xml.push_str("<w:p/>");
    }
    struct PageInfo {
        master: Option<String>,
        page: Page,
        header: Option<String>,
        footer: Option<String>,
        /// `style:header-first` / `style:footer-first` (the first page of the section): `w:titlePg` with `w:type="first"` parts.
        header_first: Option<String>,
        footer_first: Option<String>,
    }
    let mut infos: Vec<PageInfo> = Vec::new();
    let mut info_of: Vec<usize> = Vec::new();
    for (m, _, _, _) in &sections.list {
        let idx = match infos.iter().position(|i| i.master == *m) {
            Some(i) => i,
            None => {
                let (page, hn, fnode) = read_page(styles_doc.as_ref(), m.as_deref());
                let mut header = None;
                let mut footer = None;
                if let Some(h) = hn {
                    let mut x = String::new();
                    blocks(h, &mut ctx, None, &mut x);
                    if x.is_empty() {
                        x.push_str("<w:p/>");
                    }
                    header = Some(x);
                }
                if let Some(f) = fnode {
                    let mut x = String::new();
                    blocks(f, &mut ctx, None, &mut x);
                    if x.is_empty() {
                        x.push_str("<w:p/>");
                    }
                    footer = Some(x);
                }
                // ODF 1.3 `style:header-first`/`style:footer-first`: Word's .docx of such a master page has the section's
                // `w:titlePg` and `w:headerReference w:type="first"` (measured with -SaveAs todocx: 3 master pages with a
                // `style:header` and a `style:header-first` each give 3 sections with titlePg and 2 header references).
                let master = styles_doc.as_ref().and_then(|sd| sd.descendants().find(|n| is(*n, "master-page") && m.as_deref().is_none_or(|m| attr(*n, "name") == Some(m))).or_else(|| sd.descendants().find(|n| is(*n, "master-page"))));
                let mut first = |tag: &str| {
                    let node = master?.children().find(|c| is(*c, tag))?;
                    let mut x = String::new();
                    blocks(node, &mut ctx, None, &mut x);
                    if x.is_empty() {
                        x.push_str("<w:p/>");
                    }
                    Some(x)
                };
                let header_first = first("header-first");
                let footer_first = first("footer-first");
                infos.push(PageInfo { master: m.clone(), page, header, footer, header_first, footer_first });
                infos.len() - 1
            }
        };
        info_of.push(idx);
    }
    let sect_of = |k: usize| -> String {
        let info = &infos[info_of[k]];
        let page = &info.page;
        let extra = sections.list[k].3.clone().unwrap_or_default();
        let mut sect = String::from("<w:sectPr>");
        if info.header.is_some() {
            sect.push_str(&format!(r#"<w:headerReference w:type="default" r:id="rIdHdr{}"/>"#, info_of[k] + 1));
        }
        if info.footer.is_some() {
            sect.push_str(&format!(r#"<w:footerReference w:type="default" r:id="rIdFtr{}"/>"#, info_of[k] + 1));
        }
        if info.header_first.is_some() {
            sect.push_str(&format!(r#"<w:headerReference w:type="first" r:id="rIdHdrF{}"/>"#, info_of[k] + 1));
        }
        if info.footer_first.is_some() {
            sect.push_str(&format!(r#"<w:footerReference w:type="first" r:id="rIdFtrF{}"/>"#, info_of[k] + 1));
        }
        if extra.continuous {
            sect.push_str(r#"<w:type w:val="continuous"/>"#);
        }
        sect.push_str(&format!(
            r#"<w:pgSz w:w="{}" w:h="{}"{}/><w:pgMar w:top="{}" w:right="{}" w:bottom="{}" w:left="{}" w:header="{}" w:footer="{}" w:gutter="0"/>{}{}{}</w:sectPr>"#,
            page.width,
            page.height,
            if page.landscape { r#" w:orient="landscape""# } else { "" },
            page.top,
            page.right + extra.right,
            page.bottom,
            page.left + extra.left,
            page.header_distance,
            page.footer_distance,
            extra.cols.clone().or_else(|| page.cols.clone()).unwrap_or_default(),
            if info.header_first.is_some() || info.footer_first.is_some() { "<w:titlePg/>" } else { "" },
            page.grid
                .map(|(kind, pitch, cs)| {
                    let cs = if cs != 0 { format!(r#" w:charSpace="{cs}""#) } else { String::new() };
                    format!(r#"<w:docGrid w:type="{kind}" w:linePitch="{pitch}"{cs}/>"#)
                })
                .unwrap_or_default()
        ));
        sect
    };
    // A section ends with a section break in its last paragraph's properties (a table or a list at the end gets a paragraph of its own).
    for k in (0..sections.list.len().saturating_sub(1)).rev() {
        let sect = sect_of(k);
        match sections.list[k].2 {
            Some(start) => {
                if body_xml[start..].starts_with("<w:p><w:pPr>") {
                    let close = start + body_xml[start..].find("</w:pPr>").unwrap_or(0);
                    body_xml.insert_str(close, &sect);
                } else {
                    body_xml.insert_str(start + "<w:p>".len(), &format!("<w:pPr>{sect}</w:pPr>"));
                }
            }
            None => {
                let at = sections.list[k + 1].1;
                body_xml.insert_str(at, &format!("<w:p><w:pPr>{sect}</w:pPr></w:p>"));
            }
        }
    }
    let sect = sect_of(sections.list.len() - 1);

    let ns = format!(r#"xmlns:w="{W_NS}" xmlns:r="{R_NS}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#);
    let document_xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {ns}><w:body>{body_xml}{sect}</w:body></w:document>"#);

    // Styles.
    let mut styles_xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="{W_NS}"><w:docDefaults>"#);
    let default_rpr = styles
        .default_para
        .as_ref()
        .map(|d| {
            let mut text = d.text.clone();
            text.entry("letter-kerning".to_string()).or_insert_with(|| "true".to_string());
            rpr(&text, &styles)
        })
        .unwrap_or_default();
    let default_ppr = styles.default_para.as_ref().map(|d| ppr(&d.para, &d.tabs, false)).unwrap_or_default();
    styles_xml.push_str(&format!("<w:rPrDefault><w:rPr>{default_rpr}</w:rPr></w:rPrDefault><w:pPrDefault><w:pPr>{default_ppr}</w:pPr></w:pPrDefault></w:docDefaults>"));
    styles_xml.push_str(r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#);
    for (level, size) in [(1, 32), (2, 28), (3, 24), (4, 22), (5, 21), (6, 21)] {
        // The file's own style of the level takes the id.
        if (0..styles.all.len()).any(|i| styles.style_id(i) == format!("Heading{level}")) {
            continue;
        }
        styles_xml.push_str(&format!(r#"<w:style w:type="paragraph" w:styleId="Heading{level}"><w:name w:val="heading {level}"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="{}"/></w:pPr><w:rPr><w:b/><w:sz w:val="{size}"/></w:rPr></w:style>"#, level - 1));
    }
    for (i, s) in styles.all.iter().enumerate() {
        let (kind, based) = match s.family.as_str() {
            "paragraph" => ("paragraph", true),
            "text" => ("character", true),
            _ => continue,
        };
        let parent = s.parent.as_deref().and_then(|p| styles.id(&s.family, p));
        styles_xml.push_str(&format!(r#"<w:style w:type="{kind}" w:styleId="{}"><w:name w:val="{}"/>"#, styles.style_id(i), esc(&format!("{} ({})", s.display, i))));
        match parent {
            Some(p) => styles_xml.push_str(&format!(r#"<w:basedOn w:val="{p}"/>"#)),
            None if based && kind == "paragraph" => styles_xml.push_str(r#"<w:basedOn w:val="Normal"/>"#),
            None => {}
        }
        if kind == "paragraph" {
            let mut p = if s.tabs.is_empty() && s.tabs_cleared {
                ppr(&s.para, &styles.inherited_clears(&s.family, s.parent.as_deref()), s.list_style.is_some())
            } else {
                ppr(&s.para, &s.tabs, s.list_style.is_some())
            };
            if let Some(ls) = &s.list_style {
                if let Some(num) = ctx.num_for(ls, false) {
                    p = format!(r#"<w:numPr><w:ilvl w:val="0"/><w:numId w:val="{num}"/></w:numPr>{p}"#);
                }
            }
            if !p.is_empty() {
                styles_xml.push_str(&format!("<w:pPr>{p}</w:pPr>"));
            }
        }
        let r = rpr(&s.text, &styles);
        if !r.is_empty() {
            styles_xml.push_str(&format!("<w:rPr>{r}</w:rPr>"));
        }
        styles_xml.push_str("</w:style>");
    }
    styles_xml.push_str("</w:styles>");

    // Numbering.
    let mut numbering_xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="{W_NS}">"#);
    for a in &ctx.abstract_nums {
        numbering_xml.push_str(a);
    }
    for (i, (abs, restart)) in ctx.nums.iter().enumerate() {
        numbering_xml.push_str(&format!(r#"<w:num w:numId="{}"><w:abstractNumId w:val="{abs}"/>"#, i + 1));
        if *restart {
            for l in 0..9 {
                numbering_xml.push_str(&format!(r#"<w:lvlOverride w:ilvl="{l}"><w:startOverride w:val="1"/></w:lvlOverride>"#));
            }
        }
        numbering_xml.push_str("</w:num>");
    }
    numbering_xml.push_str("</w:numbering>");

    let part = |tag: &str, inner: &str| format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{tag} {ns}>{inner}</w:{tag}>"#);

    let mut ct = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="jpg" ContentType="image/jpeg"/><Default Extension="jpeg" ContentType="image/jpeg"/><Default Extension="gif" ContentType="image/gif"/><Default Extension="bmp" ContentType="image/bmp"/><Default Extension="emf" ContentType="image/x-emf"/><Default Extension="wmf" ContentType="image/x-wmf"/><Default Extension="svg" ContentType="image/svg+xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>"#,
    );
    if !ctx.footnotes.is_empty() {
        ct.push_str(r#"<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>"#);
    }
    if !ctx.endnotes.is_empty() {
        ct.push_str(r#"<Override PartName="/word/endnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"/>"#);
    }
    for (i, info) in infos.iter().enumerate() {
        if info.header.is_some() {
            ct.push_str(&format!(r#"<Override PartName="/word/header{0}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>"#, i + 1));
        }
        if info.footer.is_some() {
            ct.push_str(&format!(r#"<Override PartName="/word/footer{0}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>"#, i + 1));
        }
        if info.header_first.is_some() {
            ct.push_str(&format!(r#"<Override PartName="/word/headerF{0}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>"#, i + 1));
        }
        if info.footer_first.is_some() {
            ct.push_str(&format!(r#"<Override PartName="/word/footerF{0}.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"/>"#, i + 1));
        }
    }
    ct.push_str("</Types>");
    let pkg_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#;
    let mut doc_rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rIdNum" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/><Relationship Id="rIdSet" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>"#,
    );
    for (i, info) in infos.iter().enumerate() {
        if info.header.is_some() {
            doc_rels.push_str(&format!(r#"<Relationship Id="rIdHdr{0}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header{0}.xml"/>"#, i + 1));
        }
        if info.footer.is_some() {
            doc_rels.push_str(&format!(r#"<Relationship Id="rIdFtr{0}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footer{0}.xml"/>"#, i + 1));
        }
        if info.header_first.is_some() {
            doc_rels.push_str(&format!(r#"<Relationship Id="rIdHdrF{0}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="headerF{0}.xml"/>"#, i + 1));
        }
        if info.footer_first.is_some() {
            doc_rels.push_str(&format!(r#"<Relationship Id="rIdFtrF{0}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="footerF{0}.xml"/>"#, i + 1));
        }
    }
    if !ctx.footnotes.is_empty() {
        doc_rels.push_str(r#"<Relationship Id="rIdFtn" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>"#);
    }
    if !ctx.endnotes.is_empty() {
        doc_rels.push_str(r#"<Relationship Id="rIdEdn" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/endnotes" Target="endnotes.xml"/>"#);
    }
    for (rid, url) in &ctx.links {
        doc_rels.push_str(&format!(r#"<Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{}" TargetMode="External"/>"#, esc(url)));
    }
    for (i, img) in ctx.images.iter().enumerate() {
        doc_rels.push_str(&format!(r#"<Relationship Id="rIdI{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/{}"/>"#, i + 1, img.name));
    }
    doc_rels.push_str("</Relationships>");

    let title = meta_text
        .as_deref()
        .and_then(|t| Document::parse(t).ok().and_then(|d| d.descendants().find(|n| is(*n, "title")).and_then(|n| n.text().map(str::to_string))))
        .unwrap_or_default();
    let core_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title></cp:coreProperties>"#,
        esc(&title)
    );

    let mut w = Writer::new();
    let mut add = |name: &str, bytes: &[u8]| w.add(name, EntryMeta::new(), bytes).map_err(|e| Error::new(e.to_string()));
    add("[Content_Types].xml", ct.as_bytes())?;
    add("_rels/.rels", pkg_rels.as_bytes())?;
    add("word/_rels/document.xml.rels", doc_rels.as_bytes())?;
    add("word/document.xml", document_xml.as_bytes())?;
    add("word/styles.xml", styles_xml.as_bytes())?;
    add("word/numbering.xml", numbering_xml.as_bytes())?;
    // Word 2013 and later (compatibility mode 15): a table's indent is its edge, as Word reads the file.
    let settings_xml = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="{W_NS}"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);
    add("word/settings.xml", settings_xml.as_bytes())?;
    let part_rels = {
        let mut r = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
        for (rid, url) in &ctx.links {
            r.push_str(&format!(r#"<Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{}" TargetMode="External"/>"#, esc(url)));
        }
        for (i, img) in ctx.images.iter().enumerate() {
            r.push_str(&format!(r#"<Relationship Id="rIdI{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/{}"/>"#, i + 1, img.name));
        }
        r.push_str("</Relationships>");
        r
    };
    for (i, info) in infos.iter().enumerate() {
        if let Some(h) = &info.header {
            add(&format!("word/header{}.xml", i + 1), part("hdr", h).as_bytes())?;
            add(&format!("word/_rels/header{}.xml.rels", i + 1), part_rels.as_bytes())?;
        }
        if let Some(f) = &info.footer {
            add(&format!("word/footer{}.xml", i + 1), part("ftr", f).as_bytes())?;
            add(&format!("word/_rels/footer{}.xml.rels", i + 1), part_rels.as_bytes())?;
        }
        if let Some(h) = &info.header_first {
            add(&format!("word/headerF{}.xml", i + 1), part("hdr", h).as_bytes())?;
            add(&format!("word/_rels/headerF{}.xml.rels", i + 1), part_rels.as_bytes())?;
        }
        if let Some(f) = &info.footer_first {
            add(&format!("word/footerF{}.xml", i + 1), part("ftr", f).as_bytes())?;
            add(&format!("word/_rels/footerF{}.xml.rels", i + 1), part_rels.as_bytes())?;
        }
    }
    if !ctx.footnotes.is_empty() {
        let mut inner = String::from(r#"<w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>"#);
        for (i, f) in ctx.footnotes.iter().enumerate() {
            inner.push_str(&format!(r#"<w:footnote w:id="{}">{f}</w:footnote>"#, i + 1));
        }
        add("word/footnotes.xml", part("footnotes", &inner).as_bytes())?;
        add("word/_rels/footnotes.xml.rels", part_rels.as_bytes())?;
    }
    if !ctx.endnotes.is_empty() {
        let mut inner = String::from(r#"<w:endnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:endnote><w:endnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:endnote>"#);
        for (i, f) in ctx.endnotes.iter().enumerate() {
            inner.push_str(&format!(r#"<w:endnote w:id="{}">{f}</w:endnote>"#, i + 1));
        }
        add("word/endnotes.xml", part("endnotes", &inner).as_bytes())?;
        add("word/_rels/endnotes.xml.rels", part_rels.as_bytes())?;
    }
    for img in &ctx.images {
        add(&format!("word/media/{}", img.name), &img.bytes)?;
    }
    add("docProps/core.xml", core_xml.as_bytes())?;
    w.finish().map_err(|e| Error::new(e.to_string()))
}
