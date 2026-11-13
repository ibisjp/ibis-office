// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Cells as an HTML table, the way Excel puts them on the clipboard (`public.html` on the Mac, "HTML Format" on Windows,
//! `text/html` elsewhere): `<td>` with `style` (colours, font, borders, `mso-number-format`), `colspan`/`rowspan`, and Excel's
//! attributes `x:num` (the number), `x:fmla` (the formula), `x:str`, `x:bool`, `x:err`.
//!
//! [`write`] makes the table of copied cells; [`read`] reads Excel's (and any other program's) table: the `<style>` block's
//! rules for `td` and its classes, then the cell's own `style`, then `align`.

use ibis_calc::input::{Typed, parse_imported};
use ibis_doc_model::{Border, BorderLine, CellAlign, CellInput, CellRange, CellStyle, CellUnderline, CellValue, CellVerticalAlign, Rgb};

use super::clipboard::Clip;

/// One cell read from a table.
#[derive(Clone, Debug)]
pub(crate) struct HtmlCell {
    pub(crate) row: u32,
    pub(crate) column: u32,
    pub(crate) rows: u32,
    pub(crate) columns: u32,
    pub(crate) input: CellInput,
    /// The text the table shows (the cell's display).
    pub(crate) shown: String,
    pub(crate) style: CellStyle,
}

// ---------------------------------------------------------------- writing

fn escape(text: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\'' if attribute => out.push_str("&#39;"),
            '\r' => {}
            '\n' if !attribute => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out
}

fn hex(rgb: Rgb) -> String {
    format!("#{:06X}", rgb.0 & 0xFF_FFFF)
}

/// `mso-number-format` writes each punctuation mark of the code after a backslash.
fn number_format_css(code: &str) -> String {
    if code == "General" {
        return "General".to_string();
    }
    let mut out = String::from("\"");
    for c in code.chars() {
        if c.is_ascii_punctuation() && c != '-' && c != '_' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn border_css(side: &str, border: &Option<Border>) -> Option<String> {
    let b = border.as_ref()?;
    let (width, kind) = match b.line {
        BorderLine::Hair => (".5pt", "solid"),
        BorderLine::Thin => (".5pt", "solid"),
        BorderLine::Medium => ("1.5pt", "solid"),
        BorderLine::Thick => ("2.0pt", "solid"),
        BorderLine::Double => ("2.25pt", "double"),
        BorderLine::Dotted => (".5pt", "dotted"),
        BorderLine::Dashed | BorderLine::DashDot | BorderLine::DashDotDot | BorderLine::SlantDashDot => (".5pt", "dashed"),
        BorderLine::MediumDashed | BorderLine::MediumDashDot | BorderLine::MediumDashDotDot => ("1.5pt", "dashed"),
    };
    Some(format!("border-{side}:{width} {kind} {}", hex(b.color)))
}

/// The declarations of a cell's `style`.
fn style_css(style: &CellStyle) -> String {
    let mut d: Vec<String> = Vec::new();
    if let Some(font) = &style.font {
        d.push(format!("font-family:\"{}\"", font.replace('"', "")));
    }
    d.push(format!("font-size:{}pt", style.font_size as f64 / 100.0));
    d.push(format!("font-weight:{}", if style.bold { 700 } else { 400 }));
    d.push(format!("font-style:{}", if style.italic { "italic" } else { "normal" }));
    let mut decoration = Vec::new();
    if style.underline.enabled() {
        decoration.push("underline");
    }
    if style.strike {
        decoration.push("line-through");
    }
    if !decoration.is_empty() {
        d.push(format!("text-decoration:{}", decoration.join(" ")));
        match style.underline {
            CellUnderline::Double => d.push("text-underline-style:double".into()),
            CellUnderline::SingleAccounting => d.push("text-underline-style:single-accounting".into()),
            CellUnderline::DoubleAccounting => d.push("text-underline-style:double-accounting".into()),
            _ => {}
        }
    } else {
        d.push("text-decoration:none".into());
    }
    d.push(format!("color:{}", hex(style.color)));
    if let Some(fill) = style.fill {
        d.push(format!("background:{}", hex(fill)));
        d.push("mso-pattern:black none".into());
    }
    for (side, border) in [("top", &style.top), ("right", &style.right), ("bottom", &style.bottom), ("left", &style.left)] {
        if let Some(css) = border_css(side, border) {
            d.push(css);
        }
    }
    match style.align {
        CellAlign::General => {}
        CellAlign::Left => d.push("text-align:left".into()),
        CellAlign::Center => d.push("text-align:center".into()),
        CellAlign::Right => d.push("text-align:right".into()),
        CellAlign::Fill => d.push("text-align:fill".into()),
        CellAlign::Justify => d.push("text-align:justify".into()),
        CellAlign::CenterAcross => d.push("text-align:center-across".into()),
        CellAlign::Distributed => d.push("text-align:distributed".into()),
    }
    d.push(
        match style.vertical {
            CellVerticalAlign::Top => "vertical-align:top",
            CellVerticalAlign::Center => "vertical-align:middle",
            CellVerticalAlign::Bottom => "vertical-align:bottom",
            CellVerticalAlign::Justify => "vertical-align:justify",
            CellVerticalAlign::Distributed => "vertical-align:distributed",
        }
        .into(),
    );
    d.push(format!("white-space:{}", if style.wrap { "normal" } else { "nowrap" }));
    if style.indent > 0 {
        d.push(format!("mso-char-indent-count:{}", style.indent));
    }
    if style.rotation != 0 {
        d.push(format!("mso-rotate:{}", style.rotation));
    }
    d.push(format!("mso-number-format:{}", number_format_css(&style.number_format)));
    d.join(";")
}

/// The attribute that tells a cell's kind to Excel, and its formula.
fn kind_attributes(cell: &ibis_doc_model::SheetCell) -> String {
    let mut out = String::new();
    match &cell.value {
        CellValue::Number(n) => out.push_str(&format!(" x:num=\"{n}\"")),
        CellValue::Bool(b) => out.push_str(&format!(" x:bool=\"{}\"", if *b { "TRUE" } else { "FALSE" })),
        CellValue::Error(e) => out.push_str(&format!(" x:err=\"{}\"", e.as_str())),
        CellValue::Text(t) => {
            // Text that would read back as another kind of value says it is text.
            if cell.formula.is_none() && !matches!(parse_imported(t, false), Typed::Value(CellValue::Text(_), _)) {
                out.push_str(&format!(" x:str=\"'{}\"", escape(t, true)));
            } else {
                out.push_str(" x:str");
            }
        }
        CellValue::Empty => {}
    }
    if let Some(f) = &cell.formula {
        out.push_str(&format!(" x:fmla=\"={}\"", escape(f, true)));
    }
    out
}

/// The HTML table of copied cells (`merges` are ranges of the clip's own rows and columns).
pub(crate) fn write(clip: &Clip, merges: &[CellRange]) -> String {
    let rows = clip.range.last_row - clip.range.first_row + 1;
    let columns = clip.range.last_column - clip.range.first_column + 1;
    let mut out = String::new();
    out.push_str(
        "<html xmlns:o=\"urn:schemas-microsoft-com:office:office\"\nxmlns:x=\"urn:schemas-microsoft-com:office:excel\"\nxmlns=\"http://www.w3.org/TR/REC-html40\">\n\n<head>\n<meta http-equiv=Content-Type content=\"text/html; charset=utf-8\">\n<meta name=ProgId content=Excel.Sheet>\n<meta name=Generator content=\"ibisOffice\">\n</head>\n\n<body>\n",
    );
    out.push_str("<table border=0 cellpadding=0 cellspacing=0 style='border-collapse:collapse'>\n<!--StartFragment-->\n");
    for c in 0..columns {
        match clip.col_widths.iter().find(|(i, _)| *i == c) {
            Some((_, w)) => {
                let px = (*w as f64 / 9525.0).round() as u32;
                out.push_str(&format!(" <col width={px} style='width:{}pt'>\n", px as f64 * 0.75));
            }
            None => out.push_str(" <col>\n"),
        }
    }
    let merge_at = |r: u32, c: u32| merges.iter().find(|m| m.first_row == r && m.first_column == c);
    let covered = |r: u32, c: u32| merges.iter().any(|m| (m.first_row..=m.last_row).contains(&r) && (m.first_column..=m.last_column).contains(&c) && !(m.first_row == r && m.first_column == c));
    for r in 0..rows {
        out.push_str(" <tr>\n");
        for c in 0..columns {
            if covered(r, c) {
                continue;
            }
            let mut span = String::new();
            if let Some(m) = merge_at(r, c) {
                if m.last_column > m.first_column {
                    span.push_str(&format!(" colspan={}", m.last_column - m.first_column + 1));
                }
                if m.last_row > m.first_row {
                    span.push_str(&format!(" rowspan={}", m.last_row - m.first_row + 1));
                }
            }
            match clip.cell_at(r, c) {
                Some(cell) => {
                    out.push_str(&format!("  <td{span}{} style='{}'>{}</td>\n", kind_attributes(&cell.cell), style_css(&cell.style), escape(&cell.cell.text, false)));
                }
                None => out.push_str(&format!("  <td{span}></td>\n")),
            }
        }
        out.push_str(" </tr>\n");
    }
    out.push_str("<!--EndFragment-->\n</table>\n</body>\n\n</html>\n");
    out
}

// ---------------------------------------------------------------- reading

/// One tag or one piece of text of the markup.
enum Token {
    Open(String, Vec<(String, String)>),
    Close(String),
    Text(String),
    /// The contents of a `<style>` element.
    Style(String),
}

fn decode(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let decoded = rest.find(';').filter(|end| *end <= 10).and_then(|end| {
            let name = &rest[1..end];
            let c = match name {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => ' ',
                _ => {
                    let n = name.strip_prefix('#')?;
                    let code = match n.strip_prefix(['x', 'X']) {
                        Some(h) => u32::from_str_radix(h, 16).ok()?,
                        None => n.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
            };
            Some((c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn tokenize(html: &str) -> Vec<Token> {
    let bytes = html.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut text_start = 0;
    let flush = |tokens: &mut Vec<Token>, from: usize, to: usize| {
        if to > from {
            tokens.push(Token::Text(html[from..to].to_string()));
        }
    };
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &html[i..];
        if rest.starts_with("<!--") {
            flush(&mut tokens, text_start, i);
            i = rest.find("-->").map_or(bytes.len(), |e| i + e + 3);
            text_start = i;
            continue;
        }
        // A tag begins with a letter, `/`, or `!`; any other `<` is text.
        let Some(next) = rest.as_bytes().get(1).copied() else { break };
        if !(next.is_ascii_alphabetic() || next == b'/' || next == b'!' || next == b'?') {
            i += 1;
            continue;
        }
        flush(&mut tokens, text_start, i);
        // The tag ends at the first `>` outside quotes.
        let mut j = i + 1;
        let mut quote: Option<u8> = None;
        while j < bytes.len() {
            match (quote, bytes[j]) {
                (None, b'>') => break,
                (None, q @ (b'"' | b'\'')) => quote = Some(q),
                (Some(q), c) if c == q => quote = None,
                _ => {}
            }
            j += 1;
        }
        let inner = &html[i + 1..j.min(bytes.len())];
        i = (j + 1).min(bytes.len());
        text_start = i;
        if inner.starts_with('!') || inner.starts_with('?') {
            continue;
        }
        if let Some(name) = inner.strip_prefix('/') {
            tokens.push(Token::Close(name.trim().to_ascii_lowercase()));
            continue;
        }
        let inner = inner.trim_end_matches('/');
        let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
        let name = inner[..name_end].to_ascii_lowercase();
        let attrs = parse_attributes(&inner[name_end..]);
        if name == "style" || name == "script" {
            let close = format!("</{name}");
            let lower = html[i..].to_ascii_lowercase();
            let end = lower.find(&close).map_or(bytes.len(), |e| i + e);
            if name == "style" {
                tokens.push(Token::Style(html[i..end].to_string()));
            }
            i = html[end..].find('>').map_or(bytes.len(), |e| end + e + 1);
            text_start = i;
            continue;
        }
        tokens.push(Token::Open(name, attrs));
    }
    flush(&mut tokens, text_start, bytes.len());
    tokens
}

fn parse_attributes(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == '/') {
            i += 1;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() && chars[i] != '=' {
            i += 1;
        }
        if i == start {
            break;
        }
        let name: String = chars[start..i].iter().collect::<String>().to_ascii_lowercase();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < chars.len() && chars[i] == '=' {
            i += 1;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                let q = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != q {
                    value.push(chars[i]);
                    i += 1;
                }
                i += 1;
            } else {
                while i < chars.len() && !chars[i].is_whitespace() {
                    value.push(chars[i]);
                    i += 1;
                }
            }
        }
        out.push((name, decode(&value)));
    }
    out
}

type Declarations = Vec<(String, String)>;

fn declarations(text: &str) -> Declarations {
    let mut out = Vec::new();
    let mut part = String::new();
    let mut quote: Option<char> = None;
    let push = |part: &mut String, out: &mut Declarations| {
        if let Some((k, v)) = part.split_once(':') {
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
            if !k.is_empty() {
                out.push((k, v));
            }
        }
        part.clear();
    };
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            part.push(c);
            escaped = false;
            continue;
        }
        match (quote, c) {
            (_, '\\') => {
                part.push(c);
                escaped = true;
            }
            (None, '"' | '\'') => {
                quote = Some(c);
                part.push(c);
            }
            (Some(q), c) if c == q => {
                quote = None;
                part.push(c);
            }
            (None, ';') => push(&mut part, &mut out),
            _ => part.push(c),
        }
    }
    push(&mut part, &mut out);
    out
}

/// The rules of a `<style>` block: (selector, declarations).
fn rules(css: &str, into: &mut Vec<(String, Declarations)>) {
    let mut text = css.replace("<!--", " ").replace("-->", " ");
    while let Some(start) = text.find("/*") {
        let end = text[start..].find("*/").map_or(text.len(), |e| start + e + 2);
        text.replace_range(start..end, " ");
    }
    for block in text.split('}') {
        let Some((selectors, body)) = block.split_once('{') else { continue };
        if selectors.trim_start().starts_with('@') {
            continue;
        }
        let d = declarations(body);
        for selector in selectors.split(',') {
            let s = selector.trim().to_ascii_lowercase();
            if !s.is_empty() {
                into.push((s, d.clone()));
            }
        }
    }
}

fn color(value: &str) -> Option<Rgb> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(h) = v.strip_prefix('#') {
        return match h.len() {
            3 => {
                let n = u32::from_str_radix(h, 16).ok()?;
                let (r, g, b) = ((n >> 8) & 15, (n >> 4) & 15, n & 15);
                Some(Rgb::from_rgb((r * 17) as u8, (g * 17) as u8, (b * 17) as u8))
            }
            6 => Some(Rgb(u32::from_str_radix(h, 16).ok()?)),
            _ => None,
        };
    }
    if let Some(args) = v.strip_prefix("rgb(").and_then(|a| a.strip_suffix(')')) {
        let n: Vec<u8> = args.split(',').filter_map(|p| p.trim().parse::<f64>().ok()).map(|x| x.clamp(0.0, 255.0) as u8).collect();
        return (n.len() == 3).then(|| Rgb::from_rgb(n[0], n[1], n[2]));
    }
    Some(match v.as_str() {
        "black" | "windowtext" => Rgb::BLACK,
        "white" | "window" => Rgb::WHITE,
        "red" => Rgb(0xFF0000),
        "green" => Rgb(0x008000),
        "blue" => Rgb(0x0000FF),
        "yellow" => Rgb(0xFFFF00),
        "gray" | "grey" => Rgb(0x808080),
        "silver" => Rgb(0xC0C0C0),
        "maroon" => Rgb(0x800000),
        "navy" => Rgb(0x000080),
        "purple" => Rgb(0x800080),
        "teal" => Rgb(0x008080),
        "olive" => Rgb(0x808000),
        "lime" => Rgb(0x00FF00),
        "aqua" | "cyan" => Rgb(0x00FFFF),
        "fuchsia" | "magenta" => Rgb(0xFF00FF),
        "orange" => Rgb(0xFFA500),
        _ => return None,
    })
}

/// A length in points (`12.0pt`, `16px`, `.5pt`).
fn points(value: &str) -> Option<f64> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(n) = v.strip_suffix("pt") {
        return n.trim().parse().ok();
    }
    if let Some(n) = v.strip_suffix("px") {
        return n.trim().parse::<f64>().ok().map(|p| p * 0.75);
    }
    v.parse().ok()
}

fn border(value: &str) -> Option<Border> {
    let mut width = 0.5;
    let mut kind = "solid";
    let mut rgb = Rgb::BLACK;
    let mut any = false;
    for word in value.split_whitespace() {
        let w = word.to_ascii_lowercase();
        if w == "none" || w == "hidden" {
            return None;
        }
        if let Some(p) = points(&w).filter(|_| w.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.')) {
            width = p;
            any = true;
        } else if matches!(w.as_str(), "solid" | "double" | "dotted" | "dashed" | "hairline" | "groove" | "ridge" | "inset" | "outset") {
            kind = match w.as_str() {
                "double" => "double",
                "dotted" => "dotted",
                "dashed" => "dashed",
                _ => "solid",
            };
            any = true;
        } else if let Some(c) = color(&w) {
            rgb = c;
        }
    }
    if !any {
        return None;
    }
    let line = match kind {
        "double" => BorderLine::Double,
        "dotted" => BorderLine::Dotted,
        "dashed" if width >= 1.0 => BorderLine::MediumDashed,
        "dashed" => BorderLine::Dashed,
        _ if width >= 1.75 => BorderLine::Thick,
        _ if width >= 1.0 => BorderLine::Medium,
        _ => BorderLine::Thin,
    };
    Some(Border { line, color: rgb })
}

fn unescape_css(value: &str) -> String {
    let v = value.trim();
    let v = v.strip_prefix('"').and_then(|s| s.strip_suffix('"')).or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\''))).unwrap_or(v);
    let mut out = String::new();
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn number_format(value: &str) -> String {
    let code = unescape_css(value);
    match code.as_str() {
        "" | "General" => "General".into(),
        "Percent" => "0%".into(),
        "Fixed" => "0.00".into(),
        "Standard" => "#,##0.00".into(),
        "Short Date" => "yyyy/m/d".into(),
        "Scientific" => "0.00E+00".into(),
        _ => code,
    }
}

/// Apply one declaration to a cell's format.
fn apply(style: &mut CellStyle, key: &str, value: &str) {
    let v = value.trim();
    let lower = v.to_ascii_lowercase();
    match key {
        "color" => {
            if let Some(c) = color(v) {
                style.color = c;
                style.color_theme = None;
            }
        }
        "background" | "background-color" => {
            style.fill = color(v);
            style.fill_theme = None;
        }
        "font-weight" => style.bold = lower == "bold" || lower == "bolder" || lower.parse::<u32>().is_ok_and(|w| w >= 600),
        "font-style" => style.italic = lower == "italic" || lower == "oblique",
        "font-size" => {
            if let Some(p) = points(v) {
                style.font_size = (p * 100.0).round().clamp(100.0, 40900.0) as u32;
            }
        }
        "font-family" => {
            let first = v.split(',').next().unwrap_or("").trim().trim_matches(['"', '\'']).trim();
            if !first.is_empty() {
                style.font = Some(first.to_string());
            }
        }
        "text-decoration" => {
            style.underline = if lower.contains("underline") { CellUnderline::Single } else { CellUnderline::None };
            style.strike = lower.contains("line-through");
        }
        "text-underline-style" => {
            if style.underline.enabled() {
                style.underline = match lower.as_str() {
                    "double" => CellUnderline::Double,
                    "single-accounting" => CellUnderline::SingleAccounting,
                    "double-accounting" => CellUnderline::DoubleAccounting,
                    _ => CellUnderline::Single,
                };
            }
        }
        "text-align" => {
            style.align = match lower.as_str() {
                "left" => CellAlign::Left,
                "center" => CellAlign::Center,
                "right" => CellAlign::Right,
                "fill" => CellAlign::Fill,
                "justify" => CellAlign::Justify,
                "center-across" => CellAlign::CenterAcross,
                "distributed" => CellAlign::Distributed,
                _ => CellAlign::General,
            }
        }
        "vertical-align" => {
            let vertical = match lower.as_str() {
                "top" => CellVerticalAlign::Top,
                "middle" | "center" => CellVerticalAlign::Center,
                "justify" => CellVerticalAlign::Justify,
                "distributed" => CellVerticalAlign::Distributed,
                _ => CellVerticalAlign::Bottom,
            };
            style.vertical = vertical;
            style.vertical_set = vertical != CellVerticalAlign::Bottom;
        }
        "white-space" => style.wrap = lower == "normal" || lower == "pre-wrap",
        "mso-char-indent-count" => style.indent = v.parse().unwrap_or(0),
        "mso-rotate" => style.rotation = v.parse::<f64>().map_or(0, |r| r.rem_euclid(181.0) as u8),
        "mso-number-format" => style.number_format = number_format(v),
        "border" => {
            let b = border(v);
            (style.top, style.right, style.bottom, style.left) = (b, b, b, b);
        }
        "border-top" => style.top = border(v),
        "border-right" => style.right = border(v),
        "border-bottom" => style.bottom = border(v),
        "border-left" => style.left = border(v),
        _ => {}
    }
}

fn selector_matches(selector: &str, tag: &str, classes: &[String]) -> bool {
    let (element, class) = match selector.split_once('.') {
        Some((e, c)) => (e, Some(c)),
        None => (selector, None),
    };
    let element_ok = element.is_empty() || element == "*" || element == tag;
    let class_ok = class.is_none_or(|c| classes.iter().any(|k| k.eq_ignore_ascii_case(c)));
    element_ok && class_ok && !(element.is_empty() && class.is_none())
}

struct Building {
    row: u32,
    column: u32,
    rows: u32,
    columns: u32,
    attrs: Vec<(String, String)>,
    tag: String,
    text: String,
}

fn finish(b: Building, sheet_rules: &[(String, Declarations)], date1904: bool) -> HtmlCell {
    let attr = |name: &str| b.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    let classes: Vec<String> = attr("class").map(|c| c.split_whitespace().map(str::to_string).collect()).unwrap_or_default();
    let mut style = CellStyle::default();
    for (selector, decls) in sheet_rules {
        if selector_matches(selector, &b.tag, &classes) {
            for (k, v) in decls {
                apply(&mut style, k, v);
            }
        }
    }
    let inline = attr("style").map(declarations).unwrap_or_default();
    for (k, v) in &inline {
        apply(&mut style, k, v);
    }
    let shown: String = b.text.split_whitespace().collect::<Vec<_>>().join(" ");
    // The text keeps its line breaks (`<br>` became `\n`), but not the markup's own.
    let shown = if b.text.contains('\n') {
        b.text.split('\n').map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n").trim_matches('\n').to_string()
    } else {
        shown
    };
    let has = |name: &str| attr(name).is_some();
    let number = |value: &str| value.trim().parse::<f64>().ok().filter(|n| n.is_finite());
    let value = if has("x:num") {
        attr("x:num").and_then(number).or_else(|| number(&shown.replace(',', ""))).map(CellValue::Number)
    } else if has("x:bool") {
        Some(CellValue::Bool(!attr("x:bool").unwrap_or("TRUE").eq_ignore_ascii_case("FALSE") && !shown.eq_ignore_ascii_case("FALSE")))
    } else if let Some(e) = attr("x:err").filter(|e| !e.is_empty()) {
        ibis_calc::value::CellError::parse(e).map(CellValue::Error)
    } else if has("x:str") {
        let text = match attr("x:str") {
            Some(t) if !t.is_empty() => t.strip_prefix('\'').unwrap_or(t).to_string(),
            _ => shown.clone(),
        };
        Some(CellValue::Text(text))
    } else {
        None
    };
    let mut input = match (attr("x:fmla").filter(|f| f.len() > 1), value) {
        (Some(f), _) => CellInput::Formula(f.strip_prefix('=').unwrap_or(f).to_string()),
        (None, Some(v)) => CellInput::Value(v),
        (None, None) if shown.is_empty() => CellInput::Clear,
        (None, None) => match parse_imported(&shown, date1904) {
            Typed::Formula(f) => CellInput::Formula(f),
            Typed::Value(v, format) => {
                if style.number_format == "General"
                    && let Some(code) = format
                {
                    style.number_format = code.to_string();
                }
                CellInput::Value(v)
            }
        },
    };
    if let CellInput::Value(CellValue::Text(t)) = &input
        && t.is_empty()
    {
        input = CellInput::Clear;
    }
    // `align` says what the cell shows; a value's own side is not a choice made in the cell.
    if let Some(align) = attr("align")
        && !inline.iter().any(|(k, _)| k == "text-align")
        && style.align == CellAlign::General
    {
        let natural = match &input {
            CellInput::Value(CellValue::Number(_)) => "right",
            CellInput::Value(CellValue::Bool(_) | CellValue::Error(_)) => "center",
            CellInput::Formula(_) => "",
            _ => "left",
        };
        let align = align.to_ascii_lowercase();
        if align != natural && !natural.is_empty() {
            style.align = match align.as_str() {
                "left" => CellAlign::Left,
                "center" => CellAlign::Center,
                "right" => CellAlign::Right,
                _ => CellAlign::General,
            };
        }
    }
    HtmlCell { row: b.row, column: b.column, rows: b.rows, columns: b.columns, input, shown, style }
}

/// The first table of `html`, as cells; `None` when there is no table.
pub(crate) fn read(html: &str, date1904: bool) -> Option<Vec<HtmlCell>> {
    let tokens = tokenize(html);
    let mut sheet_rules: Vec<(String, Declarations)> = Vec::new();
    for t in &tokens {
        if let Token::Style(css) = t {
            rules(css, &mut sheet_rules);
        }
    }
    let mut cells = Vec::new();
    let mut depth = 0u32;
    let mut seen_table = false;
    let mut done = false;
    let mut row: i64 = -1;
    let mut column = 0u32;
    // Cells that rowspan/colspan cover: (row, column).
    let mut taken: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut current: Option<Building> = None;
    let mut hidden = 0u32;
    for token in tokens {
        if done {
            break;
        }
        match token {
            Token::Open(name, attrs) => match name.as_str() {
                "table" => {
                    depth += 1;
                    seen_table = true;
                }
                "tr" if depth == 1 => {
                    if let Some(b) = current.take() {
                        cells.push(finish(b, &sheet_rules, date1904));
                    }
                    row += 1;
                    column = 0;
                }
                "td" | "th" if depth == 1 => {
                    if let Some(b) = current.take() {
                        cells.push(finish(b, &sheet_rules, date1904));
                    }
                    if row < 0 {
                        row = 0;
                    }
                    while taken.contains(&(row as u32, column)) {
                        column += 1;
                    }
                    let span = |n: &str| attrs.iter().find(|(k, _)| k == n).and_then(|(_, v)| v.trim().parse::<u32>().ok()).filter(|s| (1..=1000).contains(s)).unwrap_or(1);
                    let (rows, columns) = (span("rowspan"), span("colspan"));
                    for r in 0..rows {
                        for c in 0..columns {
                            taken.insert((row as u32 + r, column + c));
                        }
                    }
                    current = Some(Building { row: row as u32, column, rows, columns, attrs, tag: name, text: String::new() });
                    column += columns;
                }
                "br" => {
                    if let Some(b) = current.as_mut()
                        && hidden == 0
                    {
                        b.text.push('\n');
                    }
                }
                // Ruby text is not displayed (Excel hides it with `display:none`).
                "rt" if current.is_some() => hidden += 1,
                _ => {}
            },
            Token::Close(name) => match name.as_str() {
                "td" | "th" if depth == 1 => {
                    if let Some(b) = current.take() {
                        cells.push(finish(b, &sheet_rules, date1904));
                    }
                }
                "rt" => hidden = hidden.saturating_sub(1),
                "table" => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        if let Some(b) = current.take() {
                            cells.push(finish(b, &sheet_rules, date1904));
                        }
                        done = true;
                    }
                }
                _ => {}
            },
            Token::Text(text) => {
                if let Some(b) = current.as_mut()
                    && hidden == 0
                {
                    // The markup's own line breaks and runs of spaces are one space; only `<br>` breaks a line.
                    let text = decode(&text);
                    let mut last_space = b.text.ends_with(' ') || b.text.ends_with('\n');
                    for c in text.chars() {
                        if c.is_whitespace() {
                            if !last_space {
                                b.text.push(' ');
                            }
                            last_space = true;
                        } else {
                            b.text.push(c);
                            last_space = false;
                        }
                    }
                }
            }
            Token::Style(_) => {}
        }
    }
    if let Some(b) = current.take() {
        cells.push(finish(b, &sheet_rules, date1904));
    }
    (seen_table && !cells.is_empty()).then_some(cells)
}

// ------------------------------------------------- the clipboard formats

// The CF_HTML frame around the HTML (put on and taken off) is the shell's clipboard's: `native_shape_clipboard::{cf_html, html_of_bytes}`.

/// A clip made of a table's cells, for pasting them as copied cells: their formulas keep their references as the table says them
/// (the cell they came from is not known, so `origin`, where the paste starts, stands for it), and the merged ranges come back
/// as ranges of the clip's own rows and columns.
pub(crate) fn clip_of(cells: Vec<HtmlCell>, slide: ibis_doc_model::SlideId, text: &str, origin: (u32, u32)) -> (Clip, Vec<CellRange>) {
    let height = cells.iter().map(|c| c.row + c.rows).max().unwrap_or(1);
    let width = cells.iter().map(|c| c.column + c.columns).max().unwrap_or(1);
    let mut merges = Vec::new();
    let mut out = Vec::new();
    for c in cells {
        if c.rows > 1 || c.columns > 1 {
            merges.push(CellRange { first_row: c.row, first_column: c.column, last_row: c.row + c.rows - 1, last_column: c.column + c.columns - 1 });
        }
        let (value, formula) = match &c.input {
            CellInput::Value(v) => (v.clone(), None),
            CellInput::Formula(f) => (CellValue::Empty, Some(f.clone())),
            _ => (CellValue::Empty, None),
        };
        out.push(super::clipboard::ClipCell {
            row: c.row,
            column: c.column,
            cell: ibis_doc_model::SheetCell {
                row: c.row,
                column: c.column,
                kind: ibis_doc_model::CellKind::of(&value),
                value,
                text: c.shown.clone(),
                formula,
                style: 0,
                phonetic: None,
                runs: None,
            },
            style: c.style,
            orig_row: origin.0 + c.row,
            orig_column: origin.1 + c.column,
        });
    }
    let clip = Clip {
        slide,
        range: CellRange { first_row: 0, first_column: 0, last_row: height - 1, last_column: width - 1 },
        cells: out,
        cut: false,
        text: text.to_string(),
        validations: Vec::new(),
        cell_images: Vec::new(),
        col_widths: Vec::new(),
        comments: Vec::new(),
        merges: merges.clone(),
        source: None,
        foreign: false,
        origin: None,
        cut_elsewhere: false,
        spill_ghosts: Vec::new(),
    };
    (clip, merges)
}

/// Whether the table shows the text the clipboard holds beside it (the same non-empty fields in the same order): a table left
/// on the clipboard by an earlier copy, beside text that something else put there, is not what was copied.
pub(crate) fn matches_text(cells: &[HtmlCell], text: &str) -> bool {
    let fields = |s: &str| -> Vec<String> { s.split(['\t', '\n', '\r']).map(|f| f.trim().trim_matches('"').trim().to_string()).filter(|f| !f.is_empty()).collect() };
    let mut ordered: Vec<&HtmlCell> = cells.iter().collect();
    ordered.sort_by_key(|c| (c.row, c.column));
    let shown: Vec<String> = ordered.iter().flat_map(|c| fields(&c.shown)).collect();
    shown == fields(text)
}
