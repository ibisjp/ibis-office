// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OpenDocument spreadsheets (`.ods`, ISO/IEC 26300): read as the `.xlsx`
//! made from them, as an old `.xls` is. Written from the OpenDocument
//! specification (version 1.2), not from any program's code.
//!
//! What is read: the sheets (names, hidden), cells (numbers, text, logical
//! values, dates and times, percentages, currencies; formulas with the
//! value last calculated), merged cells, column widths and row heights
//! (hidden columns and rows), the cell styles (font, size, bold, italic,
//! underline, strike, colour, fill, borders, alignment, wrap, rotation, the
//! number formats), hyperlinks in cells and named ranges.

use std::collections::{BTreeMap, HashMap};

mod chart;
mod features;
mod shapes;

use roxmltree::{Document, Node};

use crate::Error;

const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
const TABLE: &str = "urn:oasis:names:tc:opendocument:xmlns:table:1.0";
const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
const NUMBER: &str = "urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0";
const DRAW: &str = "urn:oasis:names:tc:opendocument:xmlns:drawing:1.0";
const SVG: &str = "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const XLINK: &str = "http://www.w3.org/1999/xlink";
const CALCEXT: &str = "urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0";
const MIMETYPE: &str = "application/vnd.oasis.opendocument.spreadsheet";

fn bad(message: &str) -> Error {
    Error::new(format!("OpenDocument スプレッドシート：{message}"))
}

/// Whether `data` is an OpenDocument spreadsheet (a zip whose `mimetype` says so).
pub fn is_ods(data: &[u8]) -> bool {
    if !data.starts_with(b"PK") {
        return false;
    }
    let Ok(archive) = crate::zip::Archive::new(data) else { return false };
    archive.read("mimetype").ok().flatten().is_some_and(|m| m.starts_with(MIMETYPE.as_bytes()) || m.starts_with(b"application/vnd.oasis.opendocument.spreadsheet-template"))
}

fn attr<'a>(n: Node<'a, '_>, ns: &str, local: &str) -> Option<&'a str> {
    n.attribute((ns, local))
}

fn children<'a, 'i>(n: Node<'a, 'i>, ns: &'static str, local: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == local && c.tag_name().namespace() == Some(ns))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

// ------------------------------------------------------------------ lengths

/// A length (`2.5cm`, `0.9in`, `12pt`, `10mm`, `20px`) in points.
fn points(text: &str) -> Option<f64> {
    let t = text.trim();
    let split = t.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(t.len());
    let (number, unit) = t.split_at(split);
    let v: f64 = number.trim().parse().ok()?;
    Some(match unit {
        "cm" => v / 2.54 * 72.0,
        "mm" => v / 25.4 * 72.0,
        "in" => v * 72.0,
        "pt" => v,
        "pc" => v * 12.0,
        "px" => v * 0.75,
        _ => return None,
    })
}

/// A length in EMU (914 400 to the inch).
fn emu(text: &str) -> Option<i64> {
    points(text).map(|p| (p * 12700.0).round() as i64)
}

/// A cell address such as `.E24` or `Sheet1.$E$24` as (column, row), both from 0.
fn cell_address(text: &str) -> Option<(u32, u32)> {
    let (_, cell) = split_reference(text);
    let cell = cell.replace('$', "");
    let letters: String = cell.chars().take_while(char::is_ascii_alphabetic).collect();
    let row: u32 = cell[letters.len()..].parse().ok()?;
    let col = letters.to_ascii_uppercase().bytes().fold(0u32, |a, b| a * 26 + (b - b'A') as u32 + 1);
    (col >= 1 && row >= 1).then(|| (col - 1, row - 1))
}

/// A picture on a sheet (`draw:frame` with a `draw:image`), anchored in a cell.
struct Picture {
    row: u32,
    col: u32,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    /// The cell the lower right corner is in, and the offsets in it.
    end: Option<(u32, u32, i64, i64)>,
    href: String,
    name: String,
    descr: String,
    /// An embedded chart (its object's folder in `href`) rather than a picture.
    chart: bool,
}

/// The pictures of a cell (frames in it, each with its picture).
fn cell_pictures(cell: Node, row: u32, col: u32) -> Vec<Picture> {
    let mut out = Vec::new();
    for frame in children(cell, DRAW, "frame") {
        // A frame with an embedded object: a chart (its folder), whose picture is only a stand-in.
        let object = children(frame, DRAW, "object").next();
        if children(frame, DRAW, "object-ole").next().is_some() {
            continue;
        }
        let image = if object.is_some() { object } else { children(frame, DRAW, "image").next() };
        let Some(image) = image else { continue };
        let Some(href) = attr(image, XLINK, "href").filter(|h| !h.is_empty() && !h.starts_with('#')) else { continue };
        let length = |name: &str| attr(frame, SVG, name).and_then(emu).unwrap_or(0);
        let end = attr(frame, TABLE, "end-cell-address").and_then(cell_address).map(|(c, r)| (c, r, attr(frame, TABLE, "end-x").and_then(emu).unwrap_or(0), attr(frame, TABLE, "end-y").and_then(emu).unwrap_or(0)));
        let text_of = |n: &'static str| children(frame, SVG, n).next().map(|d| d.text().unwrap_or("").to_string()).unwrap_or_default();
        out.push(Picture { row, col, x: length("x"), y: length("y"), width: length("width"), height: length("height"), end, href: href.trim_start_matches("./").to_string(), name: attr(frame, DRAW, "name").unwrap_or("").to_string(), descr: text_of("desc"), chart: object.is_some() });
    }
    out
}

// ------------------------------------------------------------------- styles

/// What a cell style says (a property left `None` is not said).
#[derive(Clone, Debug, Default)]
struct Format {
    bold: Option<bool>,
    italic: Option<bool>,
    underline: Option<bool>,
    double_underline: bool,
    strike: Option<bool>,
    color: Option<String>,
    size: Option<f64>,
    font: Option<String>,
    fill: Option<String>,
    /// left, right, top, bottom: (style, width in points, colour)
    borders: [Option<(String, f64, String)>; 4],
    halign: Option<String>,
    valign: Option<String>,
    wrap: Option<bool>,
    rotation: Option<i32>,
    indent: Option<f64>,
    shrink: Option<bool>,
    data_style: Option<String>,
    parent: Option<String>,
}

fn color(text: &str) -> Option<String> {
    let t = text.trim();
    if t == "transparent" {
        return None;
    }
    let hex = t.strip_prefix('#')?;
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| hex.to_ascii_uppercase())
}

fn border(text: &str) -> Option<(String, f64, String)> {
    let t = text.trim();
    if t == "none" || t.is_empty() {
        return Some((String::new(), 0.0, String::new()));
    }
    let mut width = 0.75;
    let (mut style, mut rgb) = (String::from("solid"), String::from("000000"));
    for part in t.split_whitespace() {
        if let Some(w) = points(part) {
            width = w;
        } else if let Some(c) = color(part) {
            rgb = c;
        } else {
            style = part.to_string();
        }
    }
    Some((style, width, rgb))
}

fn read_format(style: Node) -> Format {
    let mut f = Format { parent: attr(style, STYLE, "parent-style-name").map(str::to_string), data_style: attr(style, STYLE, "data-style-name").map(str::to_string), ..Format::default() };
    for p in children(style, STYLE, "text-properties") {
        if let Some(v) = attr(p, FO, "font-weight") {
            f.bold = Some(v == "bold" || v.parse::<u32>().is_ok_and(|n| n >= 600));
        }
        if let Some(v) = attr(p, FO, "font-style") {
            f.italic = Some(v == "italic" || v == "oblique");
        }
        if let Some(v) = attr(p, STYLE, "text-underline-style") {
            f.underline = Some(v != "none");
            f.double_underline = attr(p, STYLE, "text-underline-type") == Some("double");
        }
        if let Some(v) = attr(p, STYLE, "text-line-through-style") {
            f.strike = Some(v != "none");
        }
        if let Some(c) = attr(p, FO, "color").and_then(color) {
            f.color = Some(c);
        }
        if let Some(s) = attr(p, FO, "font-size").and_then(points) {
            f.size = Some(s);
        }
        if let Some(n) = attr(p, STYLE, "font-name").or_else(|| attr(p, FO, "font-family")) {
            f.font = Some(n.trim_matches('\'').to_string());
        }
    }
    for p in children(style, STYLE, "table-cell-properties") {
        if let Some(v) = attr(p, FO, "background-color") {
            f.fill = color(v);
        }
        if let Some(b) = attr(p, FO, "border") {
            let b = border(b);
            for slot in f.borders.iter_mut() {
                *slot = b.clone();
            }
        }
        for (i, name) in ["border-left", "border-right", "border-top", "border-bottom"].iter().enumerate() {
            if let Some(b) = attr(p, FO, name) {
                f.borders[i] = border(b);
            }
        }
        if let Some(v) = attr(p, STYLE, "vertical-align") {
            f.valign = Some(v.to_string());
        }
        if let Some(v) = attr(p, FO, "wrap-option") {
            f.wrap = Some(v == "wrap");
        }
        if let Some(v) = attr(p, STYLE, "rotation-angle").and_then(|a| a.parse::<i32>().ok()) {
            f.rotation = Some(v);
        }
        if let Some(v) = attr(p, STYLE, "shrink-to-fit") {
            f.shrink = Some(v == "true");
        }
    }
    for p in children(style, STYLE, "paragraph-properties") {
        if let Some(v) = attr(p, FO, "text-align") {
            f.halign = Some(v.to_string());
        }
        if let Some(v) = attr(p, FO, "margin-left").and_then(points) {
            f.indent = Some(v);
        }
    }
    f
}

/// `base` with what `over` says on top.
fn merged(base: &Format, over: &Format) -> Format {
    let mut out = base.clone();
    macro_rules! take {
        ($($field:ident),*) => { $(if over.$field.is_some() { out.$field = over.$field.clone(); })* };
    }
    take!(bold, italic, underline, strike, color, size, font, fill, halign, valign, wrap, rotation, indent, shrink, data_style);
    if over.underline.is_some() {
        out.double_underline = over.double_underline;
    }
    for i in 0..4 {
        if over.borders[i].is_some() {
            out.borders[i] = over.borders[i].clone();
        }
    }
    out
}

// ------------------------------------------------------------ number styles

fn quoted(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    if text == "%" {
        return "%".into();
    }
    if text.chars().all(|c| c == ' ' || c == '-' || c == '/' || c == ':' || c == '(' || c == ')' || c == ',' || c == '.') {
        return text.replace(' ', "\\ ").replace(',', "\\,").replace('.', "\\.");
    }
    format!("\"{}\"", text.replace('"', "\"\""))
}

fn node_text(n: Node) -> String {
    n.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect()
}

/// An Excel format code made of the `number:*-style` element's parts.
fn number_code(style: Node, all: &HashMap<String, String>) -> Option<String> {
    let kind = style.tag_name().name();
    let mut code = String::new();
    let mut seen_minutes_context = false;
    for part in style.children().filter(|c| c.is_element()) {
        let name = part.tag_name().name();
        let style_attr = |n: &str| attr(part, NUMBER, n);
        match name {
            "number" => {
                let decimals = style_attr("decimal-places").and_then(|d| d.parse::<usize>().ok()).unwrap_or(0);
                let min_int = style_attr("min-integer-digits").and_then(|d| d.parse::<usize>().ok()).unwrap_or(1);
                let grouping = style_attr("grouping") == Some("true");
                let int = if grouping { "#,##".to_string() + &"0".repeat(min_int.max(1)) } else { "0".repeat(min_int.max(1)) };
                code.push_str(&int);
                if decimals > 0 {
                    code.push('.');
                    code.push_str(&"0".repeat(decimals));
                }
                // The display factor 1000 or more drops thousands: a comma.
                if style_attr("display-factor").is_some_and(|f| f == "1000") {
                    code.push(',');
                }
            }
            "scientific-number" => {
                let decimals = style_attr("decimal-places").and_then(|d| d.parse::<usize>().ok()).unwrap_or(2);
                code.push('0');
                if decimals > 0 {
                    code.push('.');
                    code.push_str(&"0".repeat(decimals));
                }
                code.push_str("E+00");
            }
            "fraction" => {
                let digits = style_attr("max-denominator-value").map_or(1, |d| d.len().max(1));
                code.push_str(&format!("# {}/{}", "?".repeat(digits), "?".repeat(digits)));
            }
            "text" => code.push_str(&quoted(&node_text(part))),
            "text-content" => code.push('@'),
            "currency-symbol" => {
                let symbol = node_text(part);
                code.push_str(&format!("[${symbol}]"));
            }
            "day" => code.push_str(if style_attr("style") == Some("long") { "dd" } else { "d" }),
            "day-of-week" => code.push_str(if style_attr("style") == Some("long") { "dddd" } else { "ddd" }),
            "month" => {
                let textual = style_attr("textual") == Some("true");
                let long = style_attr("style") == Some("long");
                code.push_str(match (textual, long) {
                    (true, true) => "mmmm",
                    (true, false) => "mmm",
                    (false, true) => "mm",
                    _ => "m",
                });
            }
            "year" => code.push_str(if style_attr("style") == Some("long") { "yyyy" } else { "yy" }),
            "era" => code.push_str(if style_attr("style") == Some("long") { "ggg" } else { "g" }),
            "hours" => {
                seen_minutes_context = true;
                code.push_str(if style_attr("style") == Some("long") { "hh" } else { "h" });
            }
            "minutes" => {
                let _ = seen_minutes_context;
                code.push_str(if style_attr("style") == Some("long") { "mm" } else { "m" });
            }
            "seconds" => {
                code.push_str(if style_attr("style") == Some("long") { "ss" } else { "s" });
                if let Some(d) = style_attr("decimal-places").and_then(|d| d.parse::<usize>().ok()).filter(|d| *d > 0) {
                    code.push('.');
                    code.push_str(&"0".repeat(d));
                }
            }
            "am-pm" => code.push_str("AM/PM"),
            _ => {}
        }
    }
    if kind == "percentage-style" && !code.contains('%') {
        code.push('%');
    }
    // A time past 24 hours (`number:truncate-on-overflow="false"`): hours in brackets.
    if kind == "time-style" && attr(style, NUMBER, "truncate-on-overflow") == Some("false") {
        code = code.replacen("hh", "[hh]", 1).replacen('h', "[h]", 1).replace("[[hh]]", "[hh]").replace("[[h]]", "[h]");
    }
    if code.is_empty() {
        return None;
    }
    // The negative section: `style:map` on value()<0 names another style.
    for map in children(style, STYLE, "map") {
        let cond = attr(map, STYLE, "condition").unwrap_or("");
        if cond.replace(' ', "") == "value()<0"
            && let Some(other) = attr(map, STYLE, "apply-style-name").and_then(|n| all.get(n))
        {
            code = format!("{code};{other}");
        }
    }
    Some(code)
}

// ----------------------------------------------------------------- formulas

/// An Excel sheet reference's name part: quoted when it needs it.
fn sheet_ref(name: &str) -> String {
    let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if plain { name.to_string() } else { format!("'{}'", name.replace('\'', "''")) }
}

/// One side of a range in OpenFormula (`$Sheet1.A1`, `.A1`, `'My Sheet'.$B$2`):
/// its sheet (if given) and cell.
fn split_reference(part: &str) -> (Option<String>, String) {
    let p = part.trim();
    let (sheet_text, cell) = if let Some(rest) = p.strip_prefix('$').filter(|r| r.starts_with('\'')).or(Some(p).filter(|r| r.starts_with('\''))) {
        // A quoted sheet name, up to the closing quote (a doubled quote inside).
        let bytes: Vec<char> = rest.chars().collect();
        let mut i = 1;
        let mut name = String::new();
        while i < bytes.len() {
            if bytes[i] == '\'' {
                if bytes.get(i + 1) == Some(&'\'') {
                    name.push('\'');
                    i += 2;
                    continue;
                }
                break;
            }
            name.push(bytes[i]);
            i += 1;
        }
        let after: String = bytes.get(i + 1..).map(|r| r.iter().collect()).unwrap_or_default();
        (Some(name), after.trim_start_matches('.').to_string())
    } else if let Some(dot) = p.rfind('.') {
        let sheet = p[..dot].trim_start_matches('$');
        (if sheet.is_empty() { None } else { Some(sheet.to_string()) }, p[dot + 1..].to_string())
    } else {
        (None, p.to_string())
    };
    (sheet_text, cell)
}

/// A formula of the document (`of:=SUM([.A1:.A3];2)`) as an Excel formula without the `=`.
fn formula_to_excel(text: &str) -> String {
    let t = text.trim();
    let t = t.strip_prefix("of:").or_else(|| t.strip_prefix("oooc:")).or_else(|| t.strip_prefix("msoxl:")).unwrap_or(t);
    let t = t.strip_prefix('=').unwrap_or(t);
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::with_capacity(t.len());
    let mut i = 0;
    let mut array_depth = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                // A string: copied, a doubled quote inside stays.
                out.push('"');
                i += 1;
                while i < chars.len() {
                    out.push(chars[i]);
                    if chars[i] == '"' {
                        if chars.get(i + 1) == Some(&'"') {
                            out.push('"');
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1;
            }
            '[' => {
                // A reference: up to the closing bracket outside quotes.
                let mut j = i + 1;
                let mut quote = false;
                while j < chars.len() && (quote || chars[j] != ']') {
                    if chars[j] == '\'' {
                        quote = !quote;
                    }
                    j += 1;
                }
                let inner: String = chars[i + 1..j.min(chars.len())].iter().collect();
                let (a, b) = match inner.split_once(':') {
                    Some((a, b)) if !inner.starts_with('\'') || inner.matches('\'').count() % 2 == 0 => (a.to_string(), Some(b.to_string())),
                    _ => (inner.clone(), None),
                };
                let (sheet_a, cell_a) = split_reference(&a);
                let prefix = sheet_a.as_deref().map(|s| format!("{}!", sheet_ref(s))).unwrap_or_default();
                out.push_str(&prefix);
                out.push_str(&cell_a);
                if let Some(b) = b {
                    let (_, cell_b) = split_reference(&b);
                    out.push(':');
                    out.push_str(&cell_b);
                }
                i = j + 1;
            }
            '{' => {
                array_depth += 1;
                out.push('{');
                i += 1;
            }
            '}' => {
                array_depth -= 1;
                out.push('}');
                i += 1;
            }
            ';' => {
                out.push(if array_depth > 0 { ',' } else { ',' });
                i += 1;
            }
            '|' if array_depth > 0 => {
                out.push(';');
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    // The prefixes of functions Excel also has under its own names.
    out.replace("COM.MICROSOFT.", "").replace("ORG.OPENOFFICE.", "")
}

// ------------------------------------------------------------------- values

/// An ISO date or date-time (`2024-01-31`, `2024-01-31T12:30:00`) as a serial number.
fn date_serial(text: &str) -> Option<f64> {
    let (date, time) = text.split_once('T').map_or((text, None), |(d, t)| (d, Some(t)));
    let mut it = date.split('-');
    let (y, m, d): (i64, i64, i64) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?);
    let days = ibis_calc::eval::convert::days_from_civil(y, m, d) + 25569;
    let fraction = match time {
        Some(t) => {
            let mut p = t.trim_end_matches('Z').split(':');
            let (h, mi): (f64, f64) = (p.next()?.parse().ok()?, p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0));
            let s: f64 = p.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
            (h * 3600.0 + mi * 60.0 + s) / 86400.0
        }
        None => 0.0,
    };
    Some(days as f64 + fraction)
}

/// A duration (`PT12H30M00S`) as a fraction of a day (more than 1 past a day).
fn time_serial(text: &str) -> Option<f64> {
    let t = text.strip_prefix('P')?;
    let (day_part, time_part) = t.split_once('T').unwrap_or((t, ""));
    let mut seconds = 0.0;
    let mut number = String::new();
    for c in day_part.chars() {
        if c == 'D' {
            seconds += number.parse::<f64>().ok()? * 86400.0;
            number.clear();
        } else if c.is_ascii_digit() || c == '.' {
            number.push(c);
        }
    }
    number.clear();
    for c in time_part.chars() {
        match c {
            'H' => {
                seconds += number.parse::<f64>().ok()? * 3600.0;
                number.clear();
            }
            'M' => {
                seconds += number.parse::<f64>().ok()? * 60.0;
                number.clear();
            }
            'S' => {
                seconds += number.parse::<f64>().ok()?;
                number.clear();
            }
            c if c.is_ascii_digit() || c == '.' => number.push(c),
            _ => {}
        }
    }
    Some(seconds / 86400.0)
}

/// The text of a cell (`text:p`s with their spaces, tabs and line breaks) and a link in it.
fn cell_text(cell: Node) -> (String, Option<String>) {
    fn collect(n: Node, out: &mut String, link: &mut Option<String>) {
        for c in n.children() {
            if c.is_text() {
                out.push_str(c.text().unwrap_or(""));
            } else if c.is_element() {
                match (c.tag_name().namespace(), c.tag_name().name()) {
                    (Some(TEXT), "s") => out.push_str(&" ".repeat(attr(c, TEXT, "c").and_then(|v| v.parse().ok()).unwrap_or(1))),
                    (Some(TEXT), "tab") => out.push('\t'),
                    (Some(TEXT), "line-break") => out.push('\n'),
                    (Some(TEXT), "a") => {
                        if link.is_none() {
                            *link = attr(c, XLINK, "href").map(str::to_string);
                        }
                        collect(c, out, link);
                    }
                    _ => collect(c, out, link),
                }
            }
        }
    }
    let mut out = String::new();
    let mut link = None;
    let mut first = true;
    for p in children(cell, TEXT, "p") {
        if !first {
            out.push('\n');
        }
        first = false;
        collect(p, &mut out, &mut link);
    }
    (out, link)
}

// ---------------------------------------------------------------- the book

#[derive(Default)]
struct Styles {
    fonts: Vec<String>,
    fills: Vec<String>,
    borders: Vec<String>,
    formats: Vec<String>,
    xfs: Vec<String>,
    /// The formats of conditional formats (`dxfs`).
    dxfs: Vec<String>,
    index: HashMap<String, usize>,
}

impl Styles {
    fn intern(list: &mut Vec<String>, xml: String) -> usize {
        match list.iter().position(|x| *x == xml) {
            Some(i) => i,
            None => {
                list.push(xml);
                list.len() - 1
            }
        }
    }

    /// The index of a conditional format's format (`dxf`).
    fn dxf(&mut self, xml: &str) -> usize {
        Self::intern(&mut self.dxfs, xml.to_string())
    }

    /// The index of the cell format (`cellXfs`) for `f` and the number format code.
    fn xf(&mut self, f: &Format, code: Option<&str>, defaults: &(String, f64)) -> usize {
        let font = {
            let mut x = String::from("<font>");
            if f.bold == Some(true) {
                x.push_str("<b/>");
            }
            if f.italic == Some(true) {
                x.push_str("<i/>");
            }
            if f.strike == Some(true) {
                x.push_str("<strike/>");
            }
            if f.underline == Some(true) {
                x.push_str(if f.double_underline { "<u val=\"double\"/>" } else { "<u/>" });
            }
            x.push_str(&format!("<sz val=\"{}\"/>", ibis_calc::number_to_xml(f.size.unwrap_or(defaults.1))));
            if let Some(c) = &f.color {
                x.push_str(&format!("<color rgb=\"FF{c}\"/>"));
            }
            x.push_str(&format!("<name val=\"{}\"/><family val=\"2\"/></font>", esc(f.font.as_deref().unwrap_or(&defaults.0))));
            x
        };
        let font_id = Self::intern(&mut self.fonts, font);
        let fill_id = match &f.fill {
            Some(c) => Self::intern(&mut self.fills, format!("<fill><patternFill patternType=\"solid\"><fgColor rgb=\"FF{c}\"/><bgColor indexed=\"64\"/></patternFill></fill>")) + 2,
            None => 0,
        };
        let side = |name: &str, b: &Option<(String, f64, String)>| match b {
            Some((style, width, rgb)) if !style.is_empty() => {
                let kind = match (style.as_str(), *width) {
                    ("double", _) => "double",
                    ("dotted", _) => "dotted",
                    ("dashed", _) => "dashed",
                    (_, w) if w >= 2.5 => "thick",
                    (_, w) if w >= 1.2 => "medium",
                    (_, w) if w < 0.3 => "hair",
                    _ => "thin",
                };
                format!("<{name} style=\"{kind}\"><color rgb=\"FF{rgb}\"/></{name}>")
            }
            _ => format!("<{name}/>"),
        };
        let border_xml = format!("<border>{}{}{}{}<diagonal/></border>", side("left", &f.borders[0]), side("right", &f.borders[1]), side("top", &f.borders[2]), side("bottom", &f.borders[3]));
        let border_id = if border_xml == "<border><left/><right/><top/><bottom/><diagonal/></border>" { 0 } else { Self::intern(&mut self.borders, border_xml) + 1 };
        let num_id = match code {
            None => 0,
            Some(c) => Self::intern(&mut self.formats, c.to_string()) + 164,
        };
        let horizontal = match f.halign.as_deref() {
            Some("center") => Some("center"),
            Some("end") | Some("right") => Some("right"),
            Some("start") | Some("left") => Some("left"),
            Some("justify") => Some("justify"),
            _ => None,
        };
        let vertical = match f.valign.as_deref() {
            Some("top") => Some("top"),
            Some("middle") => Some("center"),
            Some("bottom") => Some("bottom"),
            _ => None,
        };
        let mut alignment = String::new();
        if let Some(h) = horizontal {
            alignment.push_str(&format!(" horizontal=\"{h}\""));
        }
        if let Some(v) = vertical {
            alignment.push_str(&format!(" vertical=\"{v}\""));
        }
        if f.wrap == Some(true) {
            alignment.push_str(" wrapText=\"1\"");
        }
        if f.shrink == Some(true) {
            alignment.push_str(" shrinkToFit=\"1\"");
        }
        if let Some(r) = f.rotation.filter(|r| *r != 0) {
            let r = r.rem_euclid(360);
            alignment.push_str(&format!(" textRotation=\"{}\"", if r <= 90 { r } else if r >= 270 { 90 + (360 - r) } else { 0 }));
        }
        if let Some(indent) = f.indent.filter(|i| *i > 0.0) {
            alignment.push_str(&format!(" indent=\"{}\"", (indent / 5.0).round().max(1.0) as u32));
        }
        let xf = format!(
            "<xf numFmtId=\"{num_id}\" fontId=\"{font_id}\" fillId=\"{fill_id}\" borderId=\"{border_id}\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"{}>{}</xf>",
            if alignment.is_empty() { "" } else { " applyAlignment=\"1\"" },
            if alignment.is_empty() { String::new() } else { format!("<alignment{alignment}/>") }
        );
        let key = xf.clone();
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        self.xfs.push(xf);
        self.index.insert(key, self.xfs.len() - 1);
        self.xfs.len() - 1
    }

    fn xml(&self, defaults: &(String, f64)) -> String {
        let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<styleSheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">");
        if !self.formats.is_empty() {
            s.push_str(&format!("<numFmts count=\"{}\">", self.formats.len()));
            for (i, c) in self.formats.iter().enumerate() {
                s.push_str(&format!("<numFmt numFmtId=\"{}\" formatCode=\"{}\"/>", 164 + i, esc(c)));
            }
            s.push_str("</numFmts>");
        }
        // The first font is the default's (what a cell with no style of its own has).
        s.push_str(&format!("<fonts count=\"{}\">", self.fonts.len()));
        for f in &self.fonts {
            s.push_str(f);
        }
        s.push_str("</fonts>");
        let _ = defaults;
        s.push_str(&format!("<fills count=\"{}\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>", self.fills.len() + 2));
        for f in &self.fills {
            s.push_str(f);
        }
        s.push_str("</fills>");
        s.push_str(&format!("<borders count=\"{}\"><border><left/><right/><top/><bottom/><diagonal/></border>", self.borders.len() + 1));
        for b in &self.borders {
            s.push_str(b);
        }
        s.push_str("</borders><cellStyleXfs count=\"1\"><xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellStyleXfs>");
        s.push_str(&format!("<cellXfs count=\"{}\">", self.xfs.len()));
        for x in &self.xfs {
            s.push_str(x);
        }
        s.push_str("</cellXfs><cellStyles count=\"1\"><cellStyle name=\"Normal\" xfId=\"0\" builtinId=\"0\"/></cellStyles>");
        if !self.dxfs.is_empty() {
            s.push_str(&format!("<dxfs count=\"{}\">{}</dxfs>", self.dxfs.len(), self.dxfs.concat()));
        }
        s.push_str("</styleSheet>");
        s
    }
}

fn column_letters(mut n: u32) -> String {
    let mut out = String::new();
    loop {
        out.insert(0, (b'A' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    out
}

/// A spreadsheet document as the bytes of an `.xlsx` made from it.
pub fn to_xlsx(data: &[u8]) -> Result<Vec<u8>, Error> {
    let archive = crate::zip::Archive::new(data)?;
    let content = archive.read("content.xml")?.ok_or_else(|| bad("content.xml がありません"))?;
    let content = String::from_utf8(content).map_err(|_| bad("content.xml が UTF-8 ではありません"))?;
    let doc = Document::parse_with_options(&content, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }).map_err(|e| bad(&format!("content.xml を読めません：{e}")))?;
    let styles_doc_text = archive.read("styles.xml")?.and_then(|b| String::from_utf8(b).ok());
    let styles_doc = styles_doc_text.as_deref().and_then(|t| Document::parse_with_options(t, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }).ok());
    let root = doc.root_element();
    let body = children(root, OFFICE, "body").next().ok_or_else(|| bad("本文がありません"))?;
    let spreadsheet = children(body, OFFICE, "spreadsheet").next().ok_or_else(|| bad("スプレッドシートではありません"))?;

    // Styles: named ones (styles.xml and the document's), the data styles, the defaults.
    let mut cell_styles: HashMap<String, Format> = HashMap::new();
    let mut column_widths: HashMap<String, f64> = HashMap::new();
    let mut row_heights: HashMap<String, f64> = HashMap::new();
    let mut data_nodes: Vec<Node> = Vec::new();
    // The graphic, paragraph and text styles (for the shapes).
    let mut drawing_styles: shapes::Styles = HashMap::new();
    let mut default_format = Format::default();
    let mut sources: Vec<Node> = Vec::new();
    if let Some(sd) = &styles_doc {
        sources.push(sd.root_element());
    }
    sources.push(root);
    for source in sources {
        for holder_name in ["styles", "automatic-styles"] {
            for holder in children(source, OFFICE, holder_name) {
                for st in holder.children().filter(|c| c.is_element()) {
                    let name = attr(st, STYLE, "name");
                    match st.tag_name().name() {
                        "style" => {
                            let Some(name) = name else { continue };
                            match attr(st, STYLE, "family") {
                                Some("table-cell") => {
                                    if let Some(display) = attr(st, STYLE, "display-name") {
                                        cell_styles.entry(display.to_string()).or_insert_with(|| read_format(st));
                                    }
                                    cell_styles.insert(name.to_string(), read_format(st));
                                }
                                Some("graphic" | "paragraph" | "text") => {
                                    let mut p = HashMap::new();
                                    for props in st.children().filter(|c| c.is_element()) {
                                        for a in props.attributes() {
                                            p.insert(a.name().to_string(), a.value().to_string());
                                        }
                                    }
                                    drawing_styles.insert(name.to_string(), (attr(st, STYLE, "parent-style-name").map(str::to_string), p));
                                }
                                Some("table-column") => {
                                    if let Some(w) = children(st, STYLE, "table-column-properties").next().and_then(|p| attr(p, STYLE, "column-width")).and_then(points) {
                                        column_widths.insert(name.to_string(), w);
                                    }
                                }
                                Some("table-row") => {
                                    if let Some(h) = children(st, STYLE, "table-row-properties").next().and_then(|p| attr(p, STYLE, "row-height")).and_then(points) {
                                        row_heights.insert(name.to_string(), h);
                                    }
                                }
                                _ => {}
                            }
                        }
                        "default-style" if attr(st, STYLE, "family") == Some("table-cell") => default_format = read_format(st),
                        "number-style" | "percentage-style" | "currency-style" | "date-style" | "time-style" | "text-style" | "boolean-style" if st.tag_name().namespace() == Some(NUMBER) => data_nodes.push(st),
                        _ => {}
                    }
                }
            }
        }
    }
    // Number format codes by style name (a mapped style's code needed first: two passes).
    let mut codes: HashMap<String, String> = HashMap::new();
    for _ in 0..2 {
        for n in &data_nodes {
            let Some(name) = attr(*n, STYLE, "name") else { continue };
            if let Some(code) = number_code(*n, &codes) {
                codes.insert(name.to_string(), code);
            } else if n.tag_name().name() == "text-style" {
                codes.insert(name.to_string(), "@".into());
            }
        }
    }
    let default_font = (default_format.font.clone().unwrap_or_else(|| "Arial".into()), default_format.size.unwrap_or(10.0));
    let resolve = |name: &str| -> Format {
        let mut chain = Vec::new();
        let mut at = Some(name.to_string());
        while let Some(n) = at {
            if chain.len() > 16 {
                break;
            }
            let f = cell_styles.get(&n).cloned().unwrap_or_default();
            at = f.parent.clone();
            chain.push(f);
        }
        let mut out = default_format.clone();
        for f in chain.iter().rev() {
            out = merged(&out, f);
        }
        out
    };

    let mut styles = Styles::default();
    // Style 0: the default (`Normal`).
    styles.xf(&default_format, None, &default_font);
    let mut sheet_xml: Vec<(String, String)> = Vec::new(); // (name, xml)
    let mut sheet_rels: Vec<String> = Vec::new();
    let mut hidden_sheets: Vec<bool> = Vec::new();
    let mut names_xml = String::new();
    let mut budget_cells = 8_000_000usize;
    // The data validations: the definitions by name.
    let mut validation_defs: HashMap<String, Node> = HashMap::new();
    for defs in children(spreadsheet, TABLE, "content-validations") {
        for d in children(defs, TABLE, "content-validation") {
            if let Some(n) = attr(d, TABLE, "name") {
                validation_defs.insert(n.to_string(), d);
            }
        }
    }
    let mut media_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut media_by_href: HashMap<String, String> = HashMap::new();
    let mut drawings: Vec<(usize, String, String)> = Vec::new();
    // The notes' parts by sheet: (sheet number, comments, legacy drawing).
    let mut note_parts: Vec<(usize, String, String)> = Vec::new();
    // The charts' parts, and the sheets they read.
    let mut chart_parts: Vec<(String, String)> = Vec::new();
    let chart_tables: Vec<(String, Node)> = children(spreadsheet, TABLE, "table").map(|t| (attr(t, TABLE, "name").unwrap_or("Sheet").to_string(), t)).collect();
    for table in children(spreadsheet, TABLE, "table") {
        let name = attr(table, TABLE, "name").unwrap_or("Sheet").to_string();
        hidden_sheets.push(attr(table, TABLE, "display") == Some("false"));
        // Columns.
        let mut cols = String::new();
        let mut col = 0u32;
        let mut col_default_style: Vec<(u32, u32, String)> = Vec::new();
        for group in table.children().filter(|c| c.is_element()) {
            let in_group = group.tag_name().name() == "table-column-group" || group.tag_name().name() == "table-header-columns" || group.tag_name().name() == "table-columns";
            let columns: Vec<Node> = if in_group { children(group, TABLE, "table-column").collect() } else if group.tag_name().name() == "table-column" { vec![group] } else { Vec::new() };
            for c in columns {
                let repeat = attr(c, TABLE, "number-columns-repeated").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1).min(16384);
                let width = attr(c, TABLE, "style-name").and_then(|s| column_widths.get(s)).copied();
                let hidden = matches!(attr(c, TABLE, "visibility"), Some("collapse") | Some("filter"));
                if let Some(s) = attr(c, TABLE, "default-cell-style-name") {
                    col_default_style.push((col, col + repeat - 1, s.to_string()));
                }
                if width.is_some() || hidden {
                    // The width in characters of the digit width the reader uses.
                    let digit = crate::xlsx::EXCEL_METRICS.digit_pt(Some(default_font.0.as_str()), default_font.1);
                    let chars = width.map_or(48.0, |w| w) / digit;
                    cols.push_str(&format!("<col min=\"{}\" max=\"{}\" width=\"{}\" customWidth=\"1\"{}/>", col + 1, col + repeat, ibis_calc::number_to_xml((chars * 256.0).ceil() / 256.0), if hidden { " hidden=\"1\"" } else { "" }));
                }
                col += repeat;
            }
        }
        let mut rows_xml = String::new();
        let mut merges: Vec<String> = Vec::new();
        let mut links: Vec<(String, String)> = Vec::new();
        let mut pictures: Vec<Picture> = Vec::new();
        let mut drawn: Vec<shapes::Shape> = Vec::new();
        // The notes: (row, column, author, text).
        let mut notes: Vec<(u32, u32, String, String)> = Vec::new();
        let mut validation_cells: std::collections::BTreeMap<String, std::collections::BTreeSet<(u32, u32)>> = std::collections::BTreeMap::new();
        let mut row = 0u32;
        let mut max_row = 0u32;
        let mut max_col = 0u32;
        let mut row_nodes: Vec<Node> = Vec::new();
        for group in table.children().filter(|c| c.is_element()) {
            match group.tag_name().name() {
                "table-row" => row_nodes.push(group),
                "table-row-group" | "table-header-rows" | "table-rows" => row_nodes.extend(children(group, TABLE, "table-row")),
                _ => {}
            }
        }
        // Rows covered by a merge from above (their cells are covered ones, skipped).
        for r in row_nodes {
            let row_repeat = attr(r, TABLE, "number-rows-repeated").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
            let height = attr(r, TABLE, "style-name").and_then(|s| row_heights.get(s)).copied();
            let row_hidden = matches!(attr(r, TABLE, "visibility"), Some("collapse") | Some("filter"));
            // The cells of the row: (column, xml) once, for each repeat of the row.
            let mut cells: Vec<(u32, String)> = Vec::new();
            let mut c = 0u32;
            for cell in r.children().filter(|n| n.is_element()) {
                let covered = cell.tag_name().name() == "covered-table-cell";
                if !covered && cell.tag_name().name() != "table-cell" {
                    continue;
                }
                let repeat = attr(cell, TABLE, "number-columns-repeated").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
                if covered {
                    c += repeat;
                    continue;
                }
                let value_type = attr(cell, OFFICE, "value-type").or_else(|| attr(cell, CALCEXT, "value-type"));
                let formula = attr(cell, TABLE, "formula");
                let style_name = attr(cell, TABLE, "style-name").map(str::to_string).or_else(|| col_default_style.iter().find(|(a, b, _)| *a <= c && c <= *b).map(|x| x.2.clone()));
                if let Some(v) = attr(cell, TABLE, "content-validation-name") {
                    let set = validation_cells.entry(v.to_string()).or_default();
                    for rr in 0..row_repeat.min(2048) {
                        for cc in 0..repeat.min(1024) {
                            set.insert((row + rr, c + cc));
                        }
                    }
                }
                if let Some(note) = children(cell, OFFICE, "annotation").next() {
                    let author = children(note, DC, "creator").next().map(|a| a.text().unwrap_or("").to_string()).unwrap_or_default();
                    let text = children(note, TEXT, "p").map(|p| p.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect::<String>()).collect::<Vec<_>>().join("\n");
                    notes.push((row, c, author, text));
                }
                let found_shapes = shapes::cell_shapes(cell, row, c, &drawing_styles);
                let has_shapes = !found_shapes.is_empty();
                drawn.extend(found_shapes);
                let frames = cell_pictures(cell, row, c);
                let has_frames = !frames.is_empty();
                pictures.extend(frames);
                let has_content = value_type.is_some() || formula.is_some() || has_frames || has_shapes || children(cell, OFFICE, "annotation").next().is_some();
                // A run of empty unstyled cells, or a very long run of empty styled ones, is skipped.
                if !has_content && (style_name.is_none() || repeat > 64) {
                    c += repeat;
                    continue;
                }
                let format = style_name.as_deref().map(&resolve).unwrap_or_else(|| default_format.clone());
                let mut code = format.data_style.as_deref().and_then(|d| codes.get(d)).cloned();
                let (text, link) = cell_text(cell);
                // The value as the cell element for the sheet (`t`, `v`).
                let number = |name: &str| attr(cell, OFFICE, name).and_then(|v| v.parse::<f64>().ok());
                let (t, v): (Option<&str>, Option<String>) = match value_type {
                    Some("float") | Some("percentage") | Some("currency") => (None, number("value").map(ibis_calc::number_to_xml)),
                    Some("date") => {
                        if code.is_none() {
                            code = Some("yyyy/m/d".into());
                        }
                        (None, attr(cell, OFFICE, "date-value").and_then(date_serial).map(ibis_calc::number_to_xml))
                    }
                    Some("time") => {
                        if code.is_none() {
                            code = Some("h:mm:ss".into());
                        }
                        (None, attr(cell, OFFICE, "time-value").and_then(time_serial).map(ibis_calc::number_to_xml))
                    }
                    Some("boolean") => (Some("b"), Some(if attr(cell, OFFICE, "boolean-value") == Some("true") { "1".into() } else { "0".into() })),
                    Some("string") => (Some("str"), Some(attr(cell, OFFICE, "string-value").map_or(text.clone(), str::to_string))),
                    Some("error") => (Some("e"), Some(text.clone())),
                    _ => (None, None),
                };
                if value_type == Some("percentage") && code.is_none() {
                    code = Some("0%".into());
                }
                let xf = styles.xf(&format, code.as_deref(), &default_font);
                let span_cols = attr(cell, TABLE, "number-columns-spanned").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
                let span_rows = attr(cell, TABLE, "number-rows-spanned").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
                for k in 0..repeat.min(if has_content { 1024 } else { 64 }) {
                    let col_index = c + k;
                    let reference = format!("{}{}", column_letters(col_index), row + 1);
                    let mut x = format!("<c r=\"{reference}\" s=\"{xf}\"");
                    match (formula, &v, t) {
                        (Some(f), v, t) => {
                            let kind = match t {
                                Some("str") => " t=\"str\"",
                                Some("b") => " t=\"b\"",
                                Some("e") => " t=\"e\"",
                                _ => "",
                            };
                            x.push_str(kind);
                            x.push_str(&format!("><f>{}</f>", esc(&formula_to_excel(f))));
                            if let Some(v) = v {
                                x.push_str(&format!("<v>{}</v>", esc(v)));
                            }
                            x.push_str("</c>");
                        }
                        (None, Some(v), Some("str")) => x.push_str(&format!(" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>", esc(v))),
                        (None, Some(v), Some(kind)) => x.push_str(&format!(" t=\"{kind}\"><v>{}</v></c>", esc(v))),
                        (None, Some(v), None) => x.push_str(&format!("><v>{v}</v></c>")),
                        (None, None, _) => x.push_str("/>"),
                    }
                    cells.push((col_index, x));
                    if k == 0 && (span_cols > 1 || span_rows > 1) {
                        merges.push(format!("{reference}:{}{}", column_letters(col_index + span_cols - 1), row + span_rows));
                    }
                    if k == 0
                        && let Some(link) = &link
                    {
                        links.push((reference.clone(), link.clone()));
                    }
                }
                max_col = max_col.max(c + repeat.min(1024));
                c += repeat;
            }
            let has_cells = !cells.is_empty();
            let styled = height.is_some() || row_hidden;
            if has_cells || styled {
                for k in 0..row_repeat.min(if has_cells { 2048 } else { 64 }) {
                    let index = row + k;
                    // A row of other rows' repeats has its cells moved down.
                    let mut x = format!("<row r=\"{}\"", index + 1);
                    if let Some(h) = height {
                        x.push_str(&format!(" ht=\"{}\" customHeight=\"1\"", ibis_calc::number_to_xml(h)));
                    }
                    if row_hidden {
                        x.push_str(" hidden=\"1\"");
                    }
                    x.push('>');
                    for (col_index, cell_xml) in &cells {
                        x.push_str(&cell_xml.replacen(&format!("r=\"{}{}\"", column_letters(*col_index), row + 1), &format!("r=\"{}{}\"", column_letters(*col_index), index + 1), 1));
                        if budget_cells == 0 {
                            return Err(bad("セルが多すぎます"));
                        }
                        budget_cells -= 1;
                    }
                    x.push_str("</row>");
                    rows_xml.push_str(&x);
                    max_row = max_row.max(index + 1);
                }
            }
            row += row_repeat;
        }
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">");
        xml.push_str(&format!("<dimension ref=\"A1:{}{}\"/>", column_letters(max_col.max(1) - 1), max_row.max(1)));
        xml.push_str("<sheetViews><sheetView workbookViewId=\"0\"/></sheetViews><sheetFormatPr defaultRowHeight=\"15\" ibisEmptyRowsAsSaved=\"1\"/>");
        if !cols.is_empty() {
            xml.push_str(&format!("<cols>{cols}</cols>"));
        }
        xml.push_str(&format!("<sheetData>{rows_xml}</sheetData>"));
        if !merges.is_empty() {
            xml.push_str(&format!("<mergeCells count=\"{}\">", merges.len()));
            for m in &merges {
                xml.push_str(&format!("<mergeCell ref=\"{m}\"/>"));
            }
            xml.push_str("</mergeCells>");
        }
        xml.push_str(&features::conditional_xml(table, &|n| cell_styles.get(n).cloned(), &mut styles));
        xml.push_str(&features::validations_xml(&validation_defs, &validation_cells));
        let mut rels = String::new();
        if !links.is_empty() {
            xml.push_str("<hyperlinks>");
            for (i, (reference, target)) in links.iter().enumerate() {
                if let Some(location) = target.strip_prefix('#') {
                    xml.push_str(&format!("<hyperlink ref=\"{reference}\" location=\"{}\"/>", esc(&location.replace('.', "!"))));
                } else {
                    xml.push_str(&format!("<hyperlink ref=\"{reference}\" r:id=\"rId{}\"/>", i + 1));
                    rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"{}\" TargetMode=\"External\"/>", i + 1, esc(target)));
                }
            }
            xml.push_str("</hyperlinks>");
        }
        xml.push_str("<pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/>");
        // The pictures: a drawing part with the anchors, and the images (read from the package) related from it.
        let mut anchors = String::new();
        let mut drawing_rels = String::new();
        for (n, pic) in pictures.iter().enumerate() {
            let rid = format!("rId{}", n + 1);
            let from = format!("<xdr:from><xdr:col>{}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:from>", pic.col, pic.x.max(0), pic.row, pic.y.max(0));
            let name = if pic.name.is_empty() { format!("Picture {}", n + 1) } else { pic.name.clone() };
            let body = if pic.chart {
                // A chart: its part, read from the object's folder.
                let Some(content) = archive.read(&format!("{}/content.xml", pic.href)).ok().flatten().and_then(|b| String::from_utf8(b).ok()) else { continue };
                let Some(chart) = chart::read_chart(&content, &chart_tables) else { continue };
                let xml = crate::xls_chart::chart_xml(&chart, &chart::OdsCells { tables: chart_tables.clone() }, &|n| if crate::english_names() { format!("Series{n}") } else { format!("系列{n}") }); // i18n-ok（文書に保存する名前）
                chart_parts.push((format!("xl/charts/chart{}.xml", chart_parts.len() + 1), xml));
                drawing_rels.push_str(&format!("<Relationship Id=\"{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/chart{}.xml\"/>", chart_parts.len()));
                format!("<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{}\" name=\"{}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" r:id=\"{rid}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/>", n + 2, esc(&name), pic.width, pic.height)
            } else {
                let Ok(Some(bytes)) = archive.read(&pic.href) else { continue };
                let ext = pic.href.rsplit('.').next().unwrap_or("png").to_ascii_lowercase();
                let file = match media_by_href.get(&pic.href) {
                    Some(f) => f.clone(),
                    None => {
                        let f = format!("image{}.{}", media_files.len() + 1, ext);
                        media_files.push((format!("xl/media/{f}"), bytes));
                        media_by_href.insert(pic.href.clone(), f.clone());
                        f
                    }
                };
                drawing_rels.push_str(&format!("<Relationship Id=\"{rid}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/{file}\"/>"));
                format!("<xdr:pic><xdr:nvPicPr><xdr:cNvPr id=\"{}\" name=\"{}\" descr=\"{}\"/><xdr:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></xdr:cNvPicPr></xdr:nvPicPr><xdr:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/>", n + 2, esc(&name), esc(&pic.descr), pic.width, pic.height)
            };
            match pic.end {
                Some((c, r, dx, dy)) => anchors.push_str(&format!("<xdr:twoCellAnchor editAs=\"oneCell\">{from}<xdr:to><xdr:col>{c}</xdr:col><xdr:colOff>{}</xdr:colOff><xdr:row>{r}</xdr:row><xdr:rowOff>{}</xdr:rowOff></xdr:to>{body}</xdr:twoCellAnchor>", dx.max(0), dy.max(0))),
                None => anchors.push_str(&format!("<xdr:oneCellAnchor>{from}<xdr:ext cx=\"{}\" cy=\"{}\"/>{body}</xdr:oneCellAnchor>", pic.width, pic.height)),
            }
        }
        for (k, shape) in drawn.iter().enumerate() {
            anchors.push_str(&shapes::anchor_xml(shape, 1000 + k));
        }
        let mut rels = rels;
        if !anchors.is_empty() {
            xml.push_str("<drawing r:id=\"rIdDrawing\"/>");
            rels.push_str(&format!("<Relationship Id=\"rIdDrawing\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing\" Target=\"../drawings/drawing{}.xml\"/>", sheet_xml.len() + 1));
            drawings.push((sheet_xml.len() + 1, format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">{anchors}</xdr:wsDr>"), drawing_rels));
        }
        if !notes.is_empty() {
            let n = sheet_xml.len() + 1;
            let mut authors: Vec<&str> = Vec::new();
            let mut list = String::new();
            let mut shapes = String::new();
            for (k, (r, c, author, text)) in notes.iter().enumerate() {
                let at = authors.iter().position(|a| *a == author).unwrap_or_else(|| {
                    authors.push(author);
                    authors.len() - 1
                });
                list.push_str(&format!("<comment ref=\"{}{}\" authorId=\"{at}\"><text><r><t xml:space=\"preserve\">{}</t></r></text></comment>", column_letters(*c), r + 1, esc(text)));
                shapes.push_str(&format!("<v:shape id=\"_x0000_s{}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:80pt;margin-top:2pt;width:108pt;height:60pt;z-index:{};visibility:hidden\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\"><v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"></div></v:textbox><x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{}, 15, {}, 2, {}, 15, {}, 16</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{r}</x:Row><x:Column>{c}</x:Column></x:ClientData></v:shape>", 1024 * n + k + 1, k + 1, c + 1, r.saturating_sub(1), c + 3, r + 3));
            }
            let authors: String = authors.iter().map(|a| format!("<author>{}</author>", esc(a))).collect();
            let comments = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><authors>{authors}</authors><commentList>{list}</commentList></comments>");
            let vml = format!("<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\"><o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"{n}\"/></o:shapelayout><v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>{shapes}</xml>");
            xml.push_str("<legacyDrawing r:id=\"rIdVml\"/>");
            rels.push_str(&format!("<Relationship Id=\"rIdComments\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments\" Target=\"../comments{n}.xml\"/><Relationship Id=\"rIdVml\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing\" Target=\"../drawings/vmlDrawing{n}.vml\"/>"));
            note_parts.push((n, comments, vml));
        }
        xml.push_str("</worksheet>");
        sheet_xml.push((name, xml));
        sheet_rels.push(rels);
    }
    if sheet_xml.is_empty() {
        return Err(bad("シートがありません"));
    }
    // Named ranges (`table:named-range`).
    for expressions in children(spreadsheet, TABLE, "named-expressions") {
        for n in children(expressions, TABLE, "named-range") {
            let (Some(name), Some(range)) = (attr(n, TABLE, "name"), attr(n, TABLE, "cell-range-address")) else { continue };
            let reference = range.split(':').map(|p| {
                let (sheet, cell) = split_reference(p);
                (sheet, cell)
            }).collect::<Vec<_>>();
            let text = match reference.as_slice() {
                [(Some(s), a), (_, b)] => format!("{}!{a}:{b}", sheet_ref(s)),
                [(Some(s), a)] => format!("{}!{a}", sheet_ref(s)),
                _ => continue,
            };
            names_xml.push_str(&format!("<definedName name=\"{}\">{}</definedName>", esc(name), esc(&text)));
        }
    }

    // The package.
    let mut zip = ibis_ooxml::zip::Writer::new();
    let meta = ibis_ooxml::zip::EntryMeta::new();
    let put = |zip: &mut ibis_ooxml::zip::Writer, name: &str, data: &str| zip.add(name, meta, data.as_bytes()).map_err(|e| bad(&format!("{e}")));
    let mut types = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"vml\" ContentType=\"application/vnd.openxmlformats-officedocument.vmlDrawing\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Default Extension=\"gif\" ContentType=\"image/gif\"/><Default Extension=\"bmp\" ContentType=\"image/bmp\"/><Default Extension=\"tif\" ContentType=\"image/tiff\"/><Default Extension=\"tiff\" ContentType=\"image/tiff\"/><Default Extension=\"svg\" ContentType=\"image/svg+xml\"/><Default Extension=\"emf\" ContentType=\"image/x-emf\"/><Default Extension=\"wmf\" ContentType=\"image/x-wmf\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml\"/>");
    let mut workbook_rels = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
    let mut sheets = String::new();
    for (i, (name, _)) in sheet_xml.iter().enumerate() {
        types.push_str(&format!("<Override PartName=\"/xl/worksheets/sheet{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>", i + 1));
        workbook_rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{}.xml\"/>", i + 1, i + 1));
        sheets.push_str(&format!("<sheet name=\"{}\" sheetId=\"{}\"{} r:id=\"rId{}\"/>", esc(name), i + 1, if hidden_sheets[i] { " state=\"hidden\"" } else { "" }, i + 1));
    }
    workbook_rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>", sheet_xml.len() + 1));
    for (n, ..) in &drawings {
        types.push_str(&format!("<Override PartName=\"/xl/drawings/drawing{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>"));
    }
    for (name, _) in &chart_parts {
        types.push_str(&format!("<Override PartName=\"/{name}\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>"));
    }
    for (n, ..) in &note_parts {
        types.push_str(&format!("<Override PartName=\"/xl/comments{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml\"/>"));
    }
    types.push_str("</Types>");
    put(&mut zip, "[Content_Types].xml", &types)?;
    put(&mut zip, "_rels/.rels", "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>")?;
    let defined = if names_xml.is_empty() { String::new() } else { format!("<definedNames>{names_xml}</definedNames>") };
    put(&mut zip, "xl/workbook.xml", &format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><bookViews><workbookView/></bookViews><sheets>{sheets}</sheets>{defined}<calcPr calcId=\"191029\"/></workbook>"))?;
    put(&mut zip, "xl/_rels/workbook.xml.rels", &workbook_rels)?;
    put(&mut zip, "xl/styles.xml", &styles.xml(&default_font))?;
    for (i, (_, xml)) in sheet_xml.iter().enumerate() {
        put(&mut zip, &format!("xl/worksheets/sheet{}.xml", i + 1), xml)?;
        if !sheet_rels[i].is_empty() {
            put(&mut zip, &format!("xl/worksheets/_rels/sheet{}.xml.rels", i + 1), &format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{}</Relationships>", sheet_rels[i]))?;
        }
    }
    for (n, xml, rels) in &drawings {
        put(&mut zip, &format!("xl/drawings/drawing{n}.xml"), xml)?;
        put(&mut zip, &format!("xl/drawings/_rels/drawing{n}.xml.rels"), &format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>"))?;
    }
    for (name, xml) in &chart_parts {
        put(&mut zip, name, xml)?;
    }
    for (n, comments, vml) in &note_parts {
        put(&mut zip, &format!("xl/comments{n}.xml"), comments)?;
        put(&mut zip, &format!("xl/drawings/vmlDrawing{n}.vml"), vml)?;
    }
    for (name, bytes) in &media_files {
        zip.add(name, meta, bytes).map_err(|e| bad(&format!("{e}")))?;
    }
    let _ = BTreeMap::<u8, u8>::new();
    zip.finish().map_err(|e| bad(&format!("{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_are_made_excels() {
        assert_eq!(formula_to_excel("of:=SUM([.A1:.A3];2)"), "SUM(A1:A3,2)");
        assert_eq!(formula_to_excel("of:=[.B2]*[$Sheet2.C3]+1"), "B2*Sheet2!C3+1");
        assert_eq!(formula_to_excel("of:=IF([.A1]>0;\"a;b\";\"c\")"), "IF(A1>0,\"a;b\",\"c\")");
        assert_eq!(formula_to_excel("of:=SUM(['My Sheet'.A1:.B2])"), "SUM('My Sheet'!A1:B2)");
        assert_eq!(formula_to_excel("of:=SUM({1;2|3;4})"), "SUM({1,2;3,4})");
        assert_eq!(formula_to_excel("of:=COM.MICROSOFT.CONCAT([.A1];[.B1])"), "CONCAT(A1,B1)");
    }

    #[test]
    fn lengths_dates_and_times() {
        assert!((points("2.54cm").unwrap() - 72.0).abs() < 1e-9);
        assert!((points("1in").unwrap() - 72.0).abs() < 1e-9);
        assert_eq!(date_serial("2024-01-31"), Some(45322.0));
        assert!((date_serial("2024-01-31T12:00:00").unwrap() - 45322.5).abs() < 1e-9);
        assert!((time_serial("PT12H30M00S").unwrap() - 0.520_833_333_333_333_3).abs() < 1e-12);
        assert_eq!(column_letters(0), "A");
        assert_eq!(column_letters(27), "AB");
    }

    #[test]
    fn a_small_document_becomes_a_workbook() {
        let content = r##"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:number="urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0">
<office:automatic-styles>
<number:percentage-style style:name="N11"><number:number number:decimal-places="1" number:min-integer-digits="1"/><number:text>%</number:text></number:percentage-style>
<style:style style:name="ce1" style:family="table-cell"><style:text-properties fo:font-weight="bold" fo:color="#ff0000"/></style:style>
<style:style style:name="ce2" style:family="table-cell" style:data-style-name="N11"/>
<style:style style:name="co1" style:family="table-column"><style:table-column-properties style:column-width="2.54cm"/></style:style>
</office:automatic-styles>
<office:body><office:spreadsheet>
<table:table table:name="Data"><table:table-column table:style-name="co1"/><table:table-column/>
<table:table-row><table:table-cell office:value-type="string" table:style-name="ce1"><text:p>Total</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="12.5"/></table:table-row>
<table:table-row><table:table-cell table:formula="of:=[.B1]*2" office:value-type="float" office:value="25"/><table:table-cell office:value-type="percentage" office:value="0.125" table:style-name="ce2"/></table:table-row>
</table:table></office:spreadsheet></office:body></office:document-content>"##;
        let mut zip = ibis_ooxml::zip::Writer::new();
        let meta = ibis_ooxml::zip::EntryMeta::new();
        zip.add("mimetype", meta, MIMETYPE.as_bytes()).unwrap();
        zip.add("content.xml", meta, content.as_bytes()).unwrap();
        let ods = zip.finish().unwrap();
        assert!(is_ods(&ods));
        let xlsx = to_xlsx(&ods).unwrap();
        let p = crate::read(&xlsx).unwrap();
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&p.slides[0]).unwrap();
        let value = |r: u32, c: u32| sheet.cell(r, c).map(|x| x.value.clone());
        assert_eq!(value(0, 0), Some(ibis_doc_model::CellValue::Text("Total".into())));
        assert_eq!(value(0, 1), Some(ibis_doc_model::CellValue::Number(12.5)));
        assert_eq!(sheet.cell(1, 0).and_then(|c| c.formula.clone()).as_deref(), Some("B1*2"));
        assert_eq!(value(1, 0), Some(ibis_doc_model::CellValue::Number(25.0)));
        let style = |r: u32, c: u32| sheet.styles[sheet.cell(r, c).unwrap().style as usize].clone();
        assert!(style(0, 0).bold);
        assert_eq!(style(0, 0).color, ibis_doc_model::Rgb(0xFF0000));
        assert_eq!(style(1, 1).number_format, "0.0%");
        assert_eq!(sheet.cell(1, 1).unwrap().text, "12.5%");
    }

    /// With `ODS_OUT` set, the workbooks made from the test files are written there (to be checked with the validator).
    #[test]
    fn the_workbooks_made_from_documents_are_written_for_checking() {
        let Ok(dir) = std::env::var("ODS_OUT") else { return };
        for name in ["ods_features_lo.ods", "lo_pictures.ods", "ods_charts_lo.ods", "ods_shapes_lo.ods"] {
            let data = std::fs::read(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap();
            std::fs::write(std::path::Path::new(&dir).join(format!("{name}.xlsx")), to_xlsx(&data).unwrap()).unwrap();
        }
    }
}
