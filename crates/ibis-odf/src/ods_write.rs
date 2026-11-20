// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A workbook written as an OpenDocument spreadsheet (`.ods`), from the
//! ODF 1.2 specification: the sheets, cells (values, formulas with the value
//! last calculated, hyperlinks), merged cells, column widths and row heights,
//! the cell formats and the number formats Excel's codes name.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ibis_doc_model::{BorderLine, CellAlign, CellKind, CellUnderline, CellValue, CellVerticalAlign, Presentation, Sheet, SheetState, sheet_calc::sheet_of_slide};

mod chart;
mod features;
mod shapes;

use ibis_formats::Error;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn cm(emu: i64) -> String {
    format!("{:.4}cm", emu as f64 / 360_000.0)
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

// ------------------------------------------------------------- formulas

fn sheet_part(name: &str) -> String {
    let plain = name.chars().all(|c| c.is_alphanumeric() || c == '_');
    if plain { format!("${name}") } else { format!("$'{}'", name.replace('\'', "''")) }
}

/// An Excel cell reference (`$A$1`) at `chars[i..]`: its length and ODF text.
fn cell_at(chars: &[char], i: usize) -> Option<usize> {
    let mut j = i;
    if chars.get(j) == Some(&'$') {
        j += 1;
    }
    let letters = j;
    while chars.get(j).is_some_and(|c| c.is_ascii_uppercase()) {
        j += 1;
    }
    if j == letters || j - letters > 3 {
        return None;
    }
    if chars.get(j) == Some(&'$') {
        j += 1;
    }
    let digits = j;
    while chars.get(j).is_some_and(|c| c.is_ascii_digit()) {
        j += 1;
    }
    (j > digits).then_some(j - i)
}

/// An Excel formula (without `=`) as an OpenFormula one (`of:=…`).
pub(crate) fn formula_to_odf(formula: &str) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::from("of:=");
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev_word = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_' || chars[i - 1] == '.');
        if c == '"' {
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
            continue;
        }
        if c == ',' {
            out.push(';');
            i += 1;
            continue;
        }
        if prev_word {
            out.push(c);
            i += 1;
            continue;
        }
        // An optional sheet name and `!`.
        let mut start = i;
        let mut sheet: Option<String> = None;
        if c == '\'' {
            let mut j = i + 1;
            let mut name = String::new();
            while j < chars.len() {
                if chars[j] == '\'' {
                    if chars.get(j + 1) == Some(&'\'') {
                        name.push('\'');
                        j += 2;
                        continue;
                    }
                    break;
                }
                name.push(chars[j]);
                j += 1;
            }
            if chars.get(j + 1) == Some(&'!') {
                sheet = Some(name);
                start = j + 2;
            }
        } else if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '.') {
                j += 1;
            }
            if chars.get(j) == Some(&'!') {
                sheet = Some(chars[i..j].iter().collect());
                start = j + 1;
            }
        }
        if let Some(len) = cell_at(&chars, start) {
            let end = start + len;
            let mut tail = end;
            let mut second = None;
            if chars.get(end) == Some(&':')
                && let Some(l2) = cell_at(&chars, end + 1)
            {
                second = Some(chars[end + 1..end + 1 + l2].iter().collect::<String>());
                tail = end + 1 + l2;
            }
            // A function name (`LOG10(`) or a longer word is not a reference.
            let next = chars.get(tail);
            if next.is_none_or(|n| !(n.is_alphanumeric() || *n == '_' || *n == '(' || *n == '.')) {
                let first: String = chars[start..end].iter().collect();
                out.push('[');
                match &sheet {
                    Some(s) => out.push_str(&format!("{}.", sheet_part(s))),
                    None => out.push('.'),
                }
                out.push_str(&first);
                if let Some(s2) = second {
                    out.push_str(&format!(":.{s2}"));
                }
                out.push(']');
                i = tail;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

// ---------------------------------------------------------- number formats

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Number,
    Percent,
    Date,
    Time,
}

/// The data style (as XML with `NAME` to fill) for an Excel format code, and
/// what kind of value it shows.
fn data_style(code: &str) -> Option<(String, Kind)> {
    let first = code.split(';').next().unwrap_or("");
    if first.is_empty() || first.eq_ignore_ascii_case("general") || first == "@" {
        return None;
    }
    let chars: Vec<char> = first.chars().collect();
    // Tokens: (kind of token, text).
    let mut parts = String::new();
    let (mut has_date, mut has_time, mut has_number, mut percent) = (false, false, false, false);
    let mut i = 0;
    let (mut int_digits, mut decimals, mut grouping) = (0usize, 0usize, false);
    let mut seen_point = false;
    let mut last_hour = false;
    let mut pending: Vec<usize> = Vec::new(); // positions of `m` in `parts` to decide
    let _ = &mut pending;
    let text = |s: &str| format!("<number:text>{}</number:text>", esc(s));
    let mut number_at: Option<usize> = None;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let mut j = i + 1;
                let mut s = String::new();
                while j < chars.len() && chars[j] != '"' {
                    s.push(chars[j]);
                    j += 1;
                }
                parts.push_str(&text(&s));
                i = j + 1;
            }
            '\\' => {
                if let Some(n) = chars.get(i + 1) {
                    parts.push_str(&text(&n.to_string()));
                }
                i += 2;
            }
            '[' => {
                let mut j = i + 1;
                let mut s = String::new();
                while j < chars.len() && chars[j] != ']' {
                    s.push(chars[j]);
                    j += 1;
                }
                if let Some(sym) = s.strip_prefix('$') {
                    let symbol = sym.split('-').next().unwrap_or("");
                    parts.push_str(&text(symbol));
                } else if s.starts_with('h') || s.starts_with('m') || s.starts_with('s') {
                    has_time = true;
                }
                i = j + 1;
            }
            '0' | '#' | '?' => {
                has_number = true;
                if number_at.is_none() {
                    number_at = Some(parts.len());
                    parts.push_str("\u{1}");
                }
                if seen_point {
                    decimals += 1;
                } else if c == '0' {
                    int_digits += 1;
                }
                i += 1;
            }
            ',' if has_number && chars.get(i + 1).is_some_and(|n| matches!(n, '#' | '0' | '?')) => {
                grouping = true;
                i += 1;
            }
            ',' if has_number => {
                i += 1;
            }
            '.' if has_number || chars.get(i + 1).is_some_and(|n| matches!(n, '0' | '#' | '?')) => {
                seen_point = true;
                if number_at.is_none() {
                    number_at = Some(parts.len());
                    parts.push_str("\u{1}");
                    has_number = true;
                }
                i += 1;
            }
            '%' => {
                percent = true;
                i += 1;
            }
            'E' | 'e' if chars.get(i + 1).is_some_and(|n| *n == '+' || *n == '-') => return None,
            'y' | 'Y' => {
                let n = chars[i..].iter().take_while(|x| x.eq_ignore_ascii_case(&'y')).count();
                parts.push_str(&format!("<number:year{}/>", if n >= 3 { " number:style=\"long\"" } else { "" }));
                has_date = true;
                i += n;
            }
            'd' | 'D' => {
                let n = chars[i..].iter().take_while(|x| x.eq_ignore_ascii_case(&'d')).count();
                parts.push_str(&match n {
                    1 => "<number:day/>".into(),
                    2 => "<number:day number:style=\"long\"/>".to_string(),
                    3 => "<number:day-of-week/>".into(),
                    _ => "<number:day-of-week number:style=\"long\"/>".into(),
                });
                has_date = true;
                i += n;
            }
            'm' | 'M' => {
                let n = chars[i..].iter().take_while(|x| x.eq_ignore_ascii_case(&'m')).count();
                // Minutes after an hour, or before seconds.
                let after = chars[i + n..].iter().find(|x| x.is_ascii_alphabetic()).copied();
                let minutes = last_hour || after.is_some_and(|x| x.eq_ignore_ascii_case(&'s'));
                if minutes {
                    parts.push_str(&format!("<number:minutes{}/>", if n >= 2 { " number:style=\"long\"" } else { "" }));
                    has_time = true;
                } else {
                    parts.push_str(&match n {
                        1 => "<number:month/>".into(),
                        2 => "<number:month number:style=\"long\"/>".to_string(),
                        3 => "<number:month number:textual=\"true\"/>".into(),
                        _ => "<number:month number:textual=\"true\" number:style=\"long\"/>".into(),
                    });
                    has_date = true;
                }
                last_hour = false;
                i += n;
            }
            'h' | 'H' => {
                let n = chars[i..].iter().take_while(|x| x.eq_ignore_ascii_case(&'h')).count();
                parts.push_str(&format!("<number:hours{}/>", if n >= 2 { " number:style=\"long\"" } else { "" }));
                has_time = true;
                last_hour = true;
                i += n;
            }
            's' | 'S' => {
                let n = chars[i..].iter().take_while(|x| x.eq_ignore_ascii_case(&'s')).count();
                parts.push_str(&format!("<number:seconds{}/>", if n >= 2 { " number:style=\"long\"" } else { "" }));
                has_time = true;
                i += n;
            }
            'A' | 'a' if first[first.char_indices().nth(i).map_or(0, |x| x.0)..].to_ascii_uppercase().starts_with("AM/PM") => {
                parts.push_str("<number:am-pm/>");
                i += 5;
            }
            'G' | 'g' if first[first.char_indices().nth(i).map_or(0, |x| x.0)..].to_ascii_lowercase().starts_with("general") => {
                number_at = Some(parts.len());
                parts.push_str("\u{1}");
                has_number = true;
                i += 7;
            }
            '_' | '*' => i += 2,
            _ => {
                parts.push_str(&text(&c.to_string()));
                i += 1;
            }
        }
    }
    let number = format!(
        "<number:number number:decimal-places=\"{decimals}\" number:min-integer-digits=\"{}\"{}/>",
        int_digits.max(if decimals > 0 { 0 } else { 1 }).max(usize::from(int_digits > 0)),
        if grouping { " number:grouping=\"true\"" } else { "" }
    );
    let body = parts.replace('\u{1}', &number);
    let (tag, kind) = if has_date {
        ("date-style", Kind::Date)
    } else if has_time {
        ("time-style", Kind::Time)
    } else if percent {
        ("percentage-style", Kind::Percent)
    } else if has_number {
        ("number-style", Kind::Number)
    } else {
        return None;
    };
    let body = if kind == Kind::Percent { format!("{body}<number:text>%</number:text>") } else { body };
    let body = if kind == Kind::Time && first.contains("[h") { body.replacen("<number:hours", "<number:hours", 1) } else { body };
    Some((format!("<number:{tag} style:name=\"NAME\">{body}</number:{tag}>"), kind))
}

// ------------------------------------------------------------------ write

fn border_text(b: &Option<ibis_doc_model::Border>) -> String {
    match b {
        None => "none".into(),
        Some(b) => {
            let (width, style) = match b.line {
                BorderLine::Hair => ("0.2pt", "solid"),
                BorderLine::Thin => ("0.75pt", "solid"),
                BorderLine::Medium | BorderLine::MediumDashed | BorderLine::MediumDashDot | BorderLine::MediumDashDotDot => ("1.5pt", "solid"),
                BorderLine::Thick => ("2.5pt", "solid"),
                BorderLine::Double => ("2.25pt", "double"),
                BorderLine::Dotted => ("0.75pt", "dotted"),
                BorderLine::Dashed | BorderLine::DashDot | BorderLine::DashDotDot | BorderLine::SlantDashDot => ("0.75pt", "dashed"),
            };
            format!("{width} {style} #{:06X}", b.color.0)
        }
    }
}

fn text_xml(text: &str, link: Option<&str>) -> String {
    let mut body = String::new();
    let mut spaces = 0;
    let flush = |body: &mut String, spaces: &mut usize| {
        if *spaces > 0 {
            body.push_str(&format!("<text:s text:c=\"{}\"/>", spaces));
            *spaces = 0;
        }
    };
    let mut first_char = true;
    for c in text.chars() {
        match c {
            ' ' if first_char || spaces > 0 => spaces += 1,
            ' ' => body.push(' '),
            '\t' => {
                flush(&mut body, &mut spaces);
                body.push_str("<text:tab/>");
            }
            '\n' => {
                flush(&mut body, &mut spaces);
                body.push_str("<text:line-break/>");
            }
            '\r' => {}
            c => {
                // A run of spaces: the first stays a space, the rest are counted.
                if spaces > 0 {
                    body.push(' ');
                    spaces -= 1;
                    flush(&mut body, &mut spaces);
                }
                body.push_str(&esc(&c.to_string()));
            }
        }
        first_char = false;
    }
    if spaces > 0 {
        body.push(' ');
        spaces -= 1;
        flush(&mut body, &mut spaces);
    }
    match link {
        Some(l) => format!("<text:p><text:a xlink:href=\"{}\">{body}</text:a></text:p>", esc(l)),
        None => format!("<text:p>{body}</text:p>"),
    }
}

/// The extent of `sizes` (a size for each of the first indexes, `default` after) up to `at`: the index there and the offset in it.
fn locate(sizes: &[i64], default: i64, at: i64, limit: u32) -> (u32, i64) {
    let (mut index, mut start) = (0u32, 0i64);
    loop {
        let size = sizes.get(index as usize).copied().unwrap_or(default);
        if at < start + size {
            return (index, (at - start).max(0));
        }
        start += size;
        index += 1;
        if index >= limit {
            return (limit - 1, 0);
        }
    }
}

/// The pictures on a sheet's slide as frames anchored in the cell they start in: by (row, column), the XML of each.
/// The images are collected into `files` (path in the package, media type, bytes).
fn pictures_of(presentation: &Presentation, refs: ChartRefs, slide: &ibis_doc_model::Slide, sheet: &Sheet, name: &str, files: &mut Vec<(String, String, std::sync::Arc<[u8]>)>, objects: &mut Vec<(String, String)>, shape_styles: &mut String) -> BTreeMap<(u32, u32), Vec<String>> {
    use ibis_doc_model::{MediaFormat, ShapeKind};
    let mut out: BTreeMap<(u32, u32), Vec<String>> = BTreeMap::new();
    for (n, shape) in slide.shapes.iter().enumerate() {
        if let ShapeKind::Chart(c) = &shape.kind {
            let series_refs = refs(shape.id, c);
            let Some((xml, linked)) = chart::content(c, shape.frame.width, shape.frame.height, series_refs.as_deref()) else { continue };
            let notify = if linked.is_empty() { String::new() } else { format!(" draw:notify-on-update-of-ranges=\"{}\"", linked.join(" ")) };
            let dir = format!("Object {}", objects.len() + 1);
            objects.push((dir.clone(), xml));
            let f = shape.frame;
            let (c0, x0) = locate(&sheet.columns, sheet.default_column, f.x, 16384);
            let (r0, y0) = locate(&sheet.rows, sheet.default_row, f.y, 1_048_576);
            let (c1, x1) = locate(&sheet.columns, sheet.default_column, f.x + f.width, 16384);
            let (r1, y1) = locate(&sheet.rows, sheet.default_row, f.y + f.height, 1_048_576);
            out.entry((r0, c0)).or_default().push(format!(
                "<draw:frame draw:z-index=\"{n}\" draw:name=\"Chart {}\" draw:style-name=\"gr1\" svg:width=\"{}\" svg:height=\"{}\" svg:x=\"{}\" svg:y=\"{}\" table:end-cell-address=\"{}.{}{}\" table:end-x=\"{}\" table:end-y=\"{}\"><draw:object{notify} xlink:href=\"./{dir}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/></draw:frame>",
                objects.len(),
                cm(f.width),
                cm(f.height),
                cm(x0),
                cm(y0),
                sheet_part(name),
                column_letters(c1),
                r1 + 1,
                cm(x1),
                cm(y1)
            ));
            continue;
        }
        if matches!(shape.kind, ShapeKind::Auto(_) | ShapeKind::TextBox(_) | ShapeKind::Line(_)) {
            let (c0, x0) = locate(&sheet.columns, sheet.default_column, shape.frame.x, 16384);
            let (r0, y0) = locate(&sheet.rows, sheet.default_row, shape.frame.y, 1_048_576);
            if let Some(xml) = shapes::shape_xml(shape, n, x0, y0, shape_styles) {
                out.entry((r0, c0)).or_default().push(xml);
            }
            continue;
        }
        let ShapeKind::Picture(picture) = &shape.kind else { continue };
        let Some(media) = presentation.media.iter().find(|m| m.id == picture.media) else { continue };
        let (ext, mime) = match media.format {
            MediaFormat::Png => ("png", "image/png"),
            MediaFormat::Jpeg => ("jpg", "image/jpeg"),
            MediaFormat::Gif => ("gif", "image/gif"),
            MediaFormat::Bmp => ("bmp", "image/bmp"),
            MediaFormat::Tiff => ("tif", "image/tiff"),
            MediaFormat::Webp => ("webp", "image/webp"),
            MediaFormat::Svg => ("svg", "image/svg+xml"),
            MediaFormat::Emf => ("emf", "image/x-emf"),
            MediaFormat::Wmf => ("wmf", "image/x-wmf"),
            _ => continue,
        };
        let path = match files.iter().find(|(_, _, d)| std::sync::Arc::ptr_eq(d, &media.data) || **d == *media.data) {
            Some((p, ..)) => p.clone(),
            None => {
                let p = format!("Pictures/image{}.{ext}", files.len() + 1);
                files.push((p.clone(), mime.to_string(), media.data.clone()));
                p
            }
        };
        let f = shape.frame;
        let (c0, x0) = locate(&sheet.columns, sheet.default_column, f.x, 16384);
        let (r0, y0) = locate(&sheet.rows, sheet.default_row, f.y, 1_048_576);
        let (c1, x1) = locate(&sheet.columns, sheet.default_column, f.x + f.width, 16384);
        let (r1, y1) = locate(&sheet.rows, sheet.default_row, f.y + f.height, 1_048_576);
        let end = format!("{}.{}{}", sheet_part(name), column_letters(c1), r1 + 1);
        let title = if picture.name.is_empty() { format!("Picture {}", n + 1) } else { picture.name.clone() };
        let descr = if picture.alt_text.description.is_empty() { String::new() } else { format!("<svg:desc>{}</svg:desc>", esc(&picture.alt_text.description)) };
        out.entry((r0, c0)).or_default().push(format!(
            "<draw:frame draw:z-index=\"{n}\" draw:name=\"{}\" draw:style-name=\"gr1\" svg:width=\"{}\" svg:height=\"{}\" svg:x=\"{}\" svg:y=\"{}\" table:end-cell-address=\"{end}\" table:end-x=\"{}\" table:end-y=\"{}\"><draw:image xlink:href=\"{path}\" xlink:type=\"simple\" xlink:show=\"embed\" xlink:actuate=\"onLoad\"/>{descr}</draw:frame>",
            esc(&title),
            cm(f.width),
            cm(f.height),
            cm(x0),
            cm(y0),
            cm(x1),
            cm(y1)
        ));
    }
    out
}

/// Where the series of a sheet's chart (shape, chart as read) take their data from: references into the workbook.
pub(crate) type ChartRefs<'a> = &'a dyn Fn(ibis_doc_model::ShapeId, &ibis_doc_model::Chart) -> Option<Vec<ibis_formats::SeriesRefs>>;

/// The workbook of `presentation` as an `.ods`.
pub fn write(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    write_with(presentation, &|_, _| None)
}

/// The workbook of `presentation` as an `.ods`, its charts' series reading the cells `refs` names (as they do in the
/// `.xlsx` they came from) beside the data they hold themselves.
pub(crate) fn write_with(presentation: &Presentation, refs: ChartRefs) -> Result<Vec<u8>, Error> {
    let workbook = presentation.workbook.as_ref().ok_or_else(|| Error::new("ブックではありません"))?;
    let mut sheets: Vec<(String, bool, std::sync::Arc<Sheet>)> = Vec::new();
    let mut sheet_slides: Vec<Option<ibis_doc_model::SlideId>> = Vec::new();
    for ws in &workbook.sheets {
        let sheet = match ws.slide {
            Some(id) => presentation.slides.iter().find(|s| s.id == id).and_then(sheet_of_slide).cloned(),
            None => ws.hidden.clone(),
        };
        if let Some(sheet) = sheet {
            sheets.push((ws.name.clone(), ws.state != SheetState::Visible, sheet));
            sheet_slides.push(ws.slide);
        }
    }
    if sheets.is_empty() {
        return Err(Error::new("シートがありません"));
    }
    let mut data_styles: Vec<String> = Vec::new();
    let mut data_names: HashMap<String, (String, Kind)> = HashMap::new();
    let mut auto = String::new();
    let mut body = String::new();
    let mut column_styles: HashMap<i64, String> = HashMap::new();
    let mut row_styles: HashMap<i64, String> = HashMap::new();
    // The pictures' files (`Pictures/imageN.ext`, once for each medium).
    let mut named_styles = String::new();
    let mut all_validations = String::new();
    let mut next_style = 0u32;
    let mut chart_objects: Vec<(String, String)> = Vec::new();
    let mut picture_files: Vec<(String, String, std::sync::Arc<[u8]>)> = Vec::new();
    for (si, (name, hidden, sheet)) in sheets.iter().enumerate() {
        let mut frames = sheet_slides[si].and_then(|id| presentation.slides.iter().find(|s| s.id == id)).map(|slide| pictures_of(presentation, refs, slide, sheet, name, &mut picture_files, &mut chart_objects, &mut auto)).unwrap_or_default();
        for c in &sheet.comments {
            let lines: String = c.text.split('\n').map(|l| format!("<text:p>{}</text:p>", esc(l))).collect();
            frames.entry((c.row, c.column)).or_default().insert(0, format!("<office:annotation><dc:creator>{}</dc:creator>{lines}</office:annotation>", esc(&c.author)));
        }
        let mut validation_cells: HashMap<(u32, u32), String> = HashMap::new();
        let validation_xml = features::validations(sheet, name, &format!("val{si}_"), &mut validation_cells);
        let mut frame_rows: BTreeSet<u32> = frames.keys().map(|(r, _)| *r).collect();
        frame_rows.extend(validation_cells.keys().map(|(r, _)| *r));
        all_validations.push_str(&validation_xml);
        let mut cell_style_names: HashMap<u32, String> = HashMap::new();
        // Styles used.
        let used: BTreeSet<u32> = sheet.cells.iter().map(|c| c.style).collect();
        for &u in &used {
            let Some(st) = sheet.styles.get(u as usize) else { continue };
            let mut data_ref = String::new();
            if let Some(entry) = data_names.get(&st.number_format) {
                data_ref = format!(" style:data-style-name=\"{}\"", entry.0);
            } else if let Some((xml, kind)) = data_style(&st.number_format) {
                let n = format!("N{}", data_styles.len() + 1);
                data_styles.push(xml.replace("NAME", &n));
                data_ref = format!(" style:data-style-name=\"{n}\"");
                data_names.insert(st.number_format.clone(), (n, kind));
            }
            let sname = format!("ce{si}_{u}");
            let mut text = String::new();
            if st.bold {
                text.push_str(" fo:font-weight=\"bold\"");
            }
            if st.italic {
                text.push_str(" fo:font-style=\"italic\"");
            }
            match st.underline {
                CellUnderline::None => {}
                CellUnderline::Double | CellUnderline::DoubleAccounting => text.push_str(" style:text-underline-style=\"solid\" style:text-underline-type=\"double\""),
                _ => text.push_str(" style:text-underline-style=\"solid\" style:text-underline-type=\"single\""),
            }
            if st.strike {
                text.push_str(" style:text-line-through-style=\"solid\"");
            }
            if st.color.0 != 0 {
                text.push_str(&format!(" fo:color=\"#{:06X}\"", st.color.0));
            }
            text.push_str(&format!(" fo:font-size=\"{}pt\"", st.font_size as f64 / 100.0));
            if let Some(f) = &st.font {
                text.push_str(&format!(" style:font-name=\"{0}\" fo:font-family=\"{0}\"", esc(f)));
            }
            let mut cell = String::new();
            if let Some(f) = st.fill {
                cell.push_str(&format!(" fo:background-color=\"#{:06X}\"", f.0));
            }
            for (n, b) in [("left", &st.left), ("right", &st.right), ("top", &st.top), ("bottom", &st.bottom)] {
                if b.is_some() {
                    cell.push_str(&format!(" fo:border-{n}=\"{}\"", border_text(b)));
                }
            }
            if st.vertical_set || st.vertical != CellVerticalAlign::Bottom {
                cell.push_str(match st.vertical {
                    CellVerticalAlign::Top => " style:vertical-align=\"top\"",
                    CellVerticalAlign::Center => " style:vertical-align=\"middle\"",
                    _ => " style:vertical-align=\"bottom\"",
                });
            }
            if st.wrap {
                cell.push_str(" fo:wrap-option=\"wrap\"");
            }
            if st.shrink {
                cell.push_str(" style:shrink-to-fit=\"true\"");
            }
            if st.rotation != 0 && st.rotation != 255 {
                let angle = if st.rotation <= 90 { st.rotation as i32 } else { 360 - (st.rotation as i32 - 90) };
                cell.push_str(&format!(" style:rotation-angle=\"{angle}\""));
            }
            let mut para = String::new();
            match st.align {
                CellAlign::Left => para.push_str(" fo:text-align=\"start\""),
                CellAlign::Center | CellAlign::CenterAcross => para.push_str(" fo:text-align=\"center\""),
                CellAlign::Right => para.push_str(" fo:text-align=\"end\""),
                CellAlign::Justify | CellAlign::Distributed => para.push_str(" fo:text-align=\"justify\""),
                _ => {}
            }
            if st.indent > 0 {
                para.push_str(&format!(" fo:margin-left=\"{}pt\"", st.indent * 9));
            }
            auto.push_str(&format!("<style:style style:name=\"{sname}\" style:family=\"table-cell\" style:parent-style-name=\"Default\"{data_ref}>"));
            auto.push_str(&format!("<style:table-cell-properties{cell}/>"));
            if !para.is_empty() {
                auto.push_str(&format!("<style:paragraph-properties{para}/>"));
            }
            auto.push_str(&format!("<style:text-properties{text}/></style:style>"));
            cell_style_names.insert(u, sname);
        }
        // Columns and rows.
        let mut table = format!("<table:table table:name=\"{}\"{}>", esc(name), if *hidden { " table:display=\"false\"" } else { "" });
        let ncols = sheet.columns.len().clamp(1, 16384);
        let mut c = 0;
        while c < ncols {
            let w = sheet.columns.get(c).copied().unwrap_or(sheet.default_column);
            let mut e = c + 1;
            while e < ncols && sheet.columns.get(e).copied().unwrap_or(sheet.default_column) == w {
                e += 1;
            }
            let hide = w == 0;
            let real = if hide { sheet.hidden_columns.get(&(c as u32)).copied().unwrap_or(sheet.default_column) } else { w };
            let key = real * 2 + i64::from(hide);
            let len = column_styles.len();
            let style = column_styles.entry(key).or_insert_with(|| {
                let n = format!("co{}", len + 1);
                auto.push_str(&format!("<style:style style:name=\"{n}\" style:family=\"table-column\"><style:table-column-properties style:column-width=\"{}\"/></style:style>", cm(real)));
                n
            });
            table.push_str(&format!("<table:table-column table:style-name=\"{style}\" table:number-columns-repeated=\"{}\"{}/>", e - c, if hide { " table:visibility=\"collapse\"" } else { "" }));
            c = e;
        }
        // Cells by row.
        let mut by_row: BTreeMap<u32, Vec<&ibis_doc_model::SheetCell>> = BTreeMap::new();
        for cell in &sheet.cells {
            by_row.entry(cell.row).or_default().push(cell);
        }
        let mut covered: HashMap<(u32, u32), ()> = HashMap::new();
        let mut spans: HashMap<(u32, u32), (u32, u32)> = HashMap::new();
        for m in &sheet.merges {
            spans.insert((m.first_row, m.first_column), (m.last_row - m.first_row + 1, m.last_column - m.first_column + 1));
            for r in m.first_row..=m.last_row {
                for col in m.first_column..=m.last_column {
                    if (r, col) != (m.first_row, m.first_column) {
                        covered.insert((r, col), ());
                    }
                }
            }
        }
        let mut links: HashMap<(u32, u32), String> = HashMap::new();
        for h in &sheet.hyperlinks {
            let target = match (&h.target, &h.location) {
                (Some(t), _) => Some(t.clone()),
                (None, Some(l)) => Some(format!("#{}", l.replace('!', "."))),
                _ => None,
            };
            if let Some(t) = target {
                links.entry((h.range.first_row, h.range.first_column)).or_insert(t);
            }
        }
        let last_row = by_row.keys().next_back().copied().into_iter().chain(sheet.merges.iter().map(|m| m.last_row)).chain(frame_rows.iter().next_back().copied()).chain(sheet.rows.iter().rposition(|r| *r != sheet.default_row).map(|r| r as u32)).max();
        let by_row: BTreeMap<u32, Vec<&ibis_doc_model::SheetCell>> = by_row;
        if let Some(last_row) = last_row {
            let mut row = 0u32;
            while row <= last_row {
                let height = sheet.rows.get(row as usize).copied().unwrap_or(sheet.default_row);
                let hide = height == 0;
                let cells = by_row.get(&row).cloned().unwrap_or_default();
                let merge_cols: Vec<u32> = covered.keys().filter(|(r, _)| *r == row).map(|(_, c)| *c).chain(spans.keys().filter(|(r, _)| *r == row).map(|(_, c)| *c)).collect();
                if cells.is_empty() && merge_cols.is_empty() && !frame_rows.contains(&row) {
                    // A run of rows with nothing in them and the same height.
                    let mut e = row + 1;
                    while e <= last_row && !by_row.contains_key(&e) && !covered.keys().any(|(r, _)| *r == e) && !spans.keys().any(|(r, _)| *r == e) && !frame_rows.contains(&e) && sheet.rows.get(e as usize).copied().unwrap_or(sheet.default_row) == height {
                        e += 1;
                    }
                    let real = if hide { sheet.hidden_rows.get(&row).copied().unwrap_or(sheet.default_row) } else { height };
                    let key = real;
                    let len = row_styles.len();
                    let style = row_styles.entry(key).or_insert_with(|| {
                        let n = format!("ro{}", len + 1);
                        auto.push_str(&format!("<style:style style:name=\"{n}\" style:family=\"table-row\"><style:table-row-properties style:row-height=\"{}\" style:use-optimal-row-height=\"false\"/></style:style>", cm(real)));
                        n
                    });
                    table.push_str(&format!("<table:table-row table:style-name=\"{style}\" table:number-rows-repeated=\"{}\"{}><table:table-cell/></table:table-row>", e - row, if hide { " table:visibility=\"collapse\"" } else { "" }));
                    row = e;
                    continue;
                }
                let real = if hide { sheet.hidden_rows.get(&row).copied().unwrap_or(sheet.default_row) } else { height };
                let len = row_styles.len();
                let style = row_styles.entry(real).or_insert_with(|| {
                    let n = format!("ro{}", len + 1);
                    auto.push_str(&format!("<style:style style:name=\"{n}\" style:family=\"table-row\"><style:table-row-properties style:row-height=\"{}\" style:use-optimal-row-height=\"false\"/></style:style>", cm(real)));
                    n
                });
                table.push_str(&format!("<table:table-row table:style-name=\"{style}\"{}>", if hide { " table:visibility=\"collapse\"" } else { "" }));
                let mut columns: BTreeSet<u32> = cells.iter().map(|c| c.column).collect();
                columns.extend(merge_cols);
                columns.extend(frames.keys().filter(|(r, _)| *r == row).map(|(_, c)| *c));
                columns.extend(validation_cells.keys().filter(|(r, _)| *r == row).map(|(_, c)| *c));
                let by_col: HashMap<u32, &&ibis_doc_model::SheetCell> = cells.iter().map(|c| (c.column, c)).collect();
                let mut next = 0u32;
                for col in columns {
                    if col > next {
                        table.push_str(&format!("<table:table-cell table:number-columns-repeated=\"{}\"/>", col - next));
                    }
                    next = col + 1;
                    if covered.contains_key(&(row, col)) {
                        table.push_str("<table:covered-table-cell/>");
                        continue;
                    }
                    let span = spans.get(&(row, col)).map(|(r, c)| format!(" table:number-rows-spanned=\"{r}\" table:number-columns-spanned=\"{c}\"")).unwrap_or_default();
                    let shapes = frames.get(&(row, col)).map(|f| f.concat()).unwrap_or_default();
                    let span = format!("{span}{}", validation_cells.get(&(row, col)).map(|v| format!(" table:content-validation-name=\"{v}\"")).unwrap_or_default());
                    let Some(cell) = by_col.get(&col) else {
                        if shapes.is_empty() {
                            table.push_str(&format!("<table:table-cell{span}/>"));
                        } else {
                            table.push_str(&format!("<table:table-cell{span}>{shapes}</table:table-cell>"));
                        }
                        continue;
                    };
                    let st = sheet.styles.get(cell.style as usize);
                    let kind = st.and_then(|s| data_names.get(&s.number_format)).map(|d| d.1);
                    let style = cell_style_names.get(&cell.style).map(|n| format!(" table:style-name=\"{n}\"")).unwrap_or_default();
                    let formula = cell.formula.as_deref().map(|f| format!(" table:formula=\"{}\"", esc(&formula_to_odf(f)))).unwrap_or_default();
                    let link = links.get(&(row, col)).map(String::as_str);
                    let (value_attrs, shown) = match (&cell.value, cell.kind) {
                        (CellValue::Number(n), _) => {
                            let n = *n;
                            match kind {
                                Some(Kind::Date) => {
                                    let serial = n + if workbook.date1904 { 1462.0 } else { 0.0 };
                                    let day = serial.floor();
                                    let (y, m, d) = ibis_calc::eval::convert::civil_from_days(day as i64 - 25569);
                                    let secs = ((serial - day) * 86400.0).round() as u32;
                                    let date = format!("{y:04}-{m:02}-{d:02}");
                                    let v = if secs > 0 { format!("{date}T{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60) } else { date };
                                    (format!(" office:value-type=\"date\" office:date-value=\"{v}\""), cell.text.clone())
                                }
                                Some(Kind::Time) => {
                                    let secs = (n * 86400.0).round() as i64;
                                    (format!(" office:value-type=\"time\" office:time-value=\"PT{:02}H{:02}M{:02}S\"", secs / 3600, secs / 60 % 60, secs % 60), cell.text.clone())
                                }
                                Some(Kind::Percent) => (format!(" office:value-type=\"percentage\" office:value=\"{}\"", ibis_calc::number_to_xml(n)), cell.text.clone()),
                                _ => (format!(" office:value-type=\"float\" office:value=\"{}\"", ibis_calc::number_to_xml(n)), cell.text.clone()),
                            }
                        }
                        (CellValue::Bool(b), _) => (format!(" office:value-type=\"boolean\" office:boolean-value=\"{b}\""), if *b { "TRUE".into() } else { "FALSE".into() }),
                        (CellValue::Text(t), _) => (" office:value-type=\"string\"".to_string(), t.clone()),
                        (CellValue::Error(_), _) => (" office:value-type=\"string\"".to_string(), cell.text.clone()),
                        (CellValue::Empty, CellKind::Empty) => (String::new(), String::new()),
                        (CellValue::Empty, _) => (String::new(), String::new()),
                    };
                    if shown.is_empty() && value_attrs.is_empty() && formula.is_empty() && shapes.is_empty() {
                        table.push_str(&format!("<table:table-cell{style}{span}/>"));
                    } else if shown.is_empty() && value_attrs.is_empty() && formula.is_empty() {
                        table.push_str(&format!("<table:table-cell{style}{span}>{shapes}</table:table-cell>"));
                    } else {
                        table.push_str(&format!("<table:table-cell{style}{span}{formula}{value_attrs}>{shapes}{}</table:table-cell>", text_xml(&shown, link)));
                    }
                }
                table.push_str("</table:table-row>");
                row += 1;
            }
        } else {
            table.push_str("<table:table-row><table:table-cell/></table:table-row>");
        }
        table.push_str(&features::conditional(sheet, name, &mut next_style, &mut named_styles));
        table.push_str("</table:table>");
        body.push_str(&table);
    }
    // Named ranges.
    let mut names = String::new();
    for n in &workbook.names {
        if n.hidden || n.sheet.is_some() {
            continue;
        }
        // A range of one sheet: `Sheet1!$A$1:$B$2`.
        let f = n.formula.trim();
        let Some((sheet, range)) = f.rsplit_once('!') else { continue };
        let sheet = sheet.trim_matches('\'').replace("''", "'");
        let range = range.replace('$', "");
        let cells: Vec<&str> = range.split(':').collect();
        if cells.is_empty() || cells.len() > 2 || cells.iter().any(|c| cell_at(&c.chars().collect::<Vec<_>>(), 0) != Some(c.len())) {
            continue;
        }
        let address = if cells.len() == 2 { format!("{0}.{1}:{0}.{2}", sheet_part(&sheet), cells[0], cells[1]) } else { format!("{0}.{1}", sheet_part(&sheet), cells[0]) };
        names.push_str(&format!("<table:named-range table:name=\"{}\" table:base-cell-address=\"{}.{}\" table:cell-range-address=\"{}\"/>", esc(&n.name), sheet_part(&sheet), cells[0], address));
    }
    if !names.is_empty() {
        body.push_str(&format!("<table:named-expressions>{names}</table:named-expressions>"));
    }
    let ns = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:number=\"urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:calcext=\"urn:org:documentfoundation:names:experimental:calc:xmlns:calcext:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:of=\"urn:oasis:names:tc:opendocument:xmlns:of:1.2\"";
    let validations = if all_validations.is_empty() { String::new() } else { format!("<table:content-validations>{all_validations}</table:content-validations>") };
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {ns} office:version=\"1.2\"><office:automatic-styles>{}{auto}<style:style style:name=\"gr1\" style:family=\"graphic\"><style:graphic-properties draw:stroke=\"none\" draw:fill=\"none\"/></style:style></office:automatic-styles><office:body><office:spreadsheet>{validations}{body}</office:spreadsheet></office:body></office:document-content>",
        data_styles.concat()
    );
    let styles = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-styles {ns} office:version=\"1.2\"><office:styles><style:default-style style:family=\"table-cell\"><style:text-properties style:font-name=\"Arial\" fo:font-size=\"10pt\"/></style:default-style><style:style style:name=\"Default\" style:family=\"table-cell\"/><style:style style:name=\"standard\" style:family=\"graphic\"/>{named_styles}</office:styles></office:document-styles>");
    let manifest = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<manifest:manifest xmlns:manifest=\"urn:oasis:names:tc:opendocument:xmlns:manifest:1.0\" manifest:version=\"1.2\"><manifest:file-entry manifest:full-path=\"/\" manifest:version=\"1.2\" manifest:media-type=\"application/vnd.oasis.opendocument.spreadsheet\"/><manifest:file-entry manifest:full-path=\"content.xml\" manifest:media-type=\"text/xml\"/><manifest:file-entry manifest:full-path=\"styles.xml\" manifest:media-type=\"text/xml\"/>{PICTURES}</manifest:manifest>";
    let mut zip = ibis_ooxml::zip::Writer::new();
    let meta = ibis_ooxml::zip::EntryMeta::default();
    let mut put = |name: &str, data: &str| zip.add(name, meta, data.as_bytes()).map_err(|e| Error::new(format!("{e}")));
    // The `mimetype` first and not compressed is what the specification asks; the writer stores small entries as they are.
    put("mimetype", "application/vnd.oasis.opendocument.spreadsheet")?;
    let object_entries: String = chart_objects.iter().map(|(dir, _)| format!("<manifest:file-entry manifest:full-path=\"{dir}/\" manifest:media-type=\"application/vnd.oasis.opendocument.chart\"/><manifest:file-entry manifest:full-path=\"{dir}/content.xml\" manifest:media-type=\"text/xml\"/>")).collect();
    let entries: String = object_entries + &picture_files.iter().map(|(path, mime, _)| format!("<manifest:file-entry manifest:full-path=\"{path}\" manifest:media-type=\"{mime}\"/>")).collect::<String>();
    put("META-INF/manifest.xml", &manifest.replace("{PICTURES}", &entries))?;
    put("content.xml", &content)?;
    put("styles.xml", &styles)?;
    for (dir, xml) in &chart_objects {
        put(&format!("{dir}/content.xml"), xml)?;
    }
    for (path, _, data) in &picture_files {
        zip.add(path, meta, data).map_err(|e| Error::new(format!("{e}")))?;
    }
    zip.finish().map_err(|e| Error::new(format!("{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formulas_become_open_formula() {
        assert_eq!(formula_to_odf("SUM(A1:A3,2)"), "of:=SUM([.A1:.A3];2)");
        assert_eq!(formula_to_odf("B2*Sheet2!C3+1"), "of:=[.B2]*[$Sheet2.C3]+1");
        assert_eq!(formula_to_odf("IF(A1>0,\"a,b\",\"c\")"), "of:=IF([.A1]>0;\"a,b\";\"c\")");
        assert_eq!(formula_to_odf("SUM('My Sheet'!A1:B2)"), "of:=SUM([$'My Sheet'.A1:.B2])");
        assert_eq!(formula_to_odf("LOG10($A$1)"), "of:=LOG10([.$A$1])");
        assert_eq!(column_letters(27), "AB");
    }

    #[test]
    fn number_formats_become_data_styles() {
        let (xml, kind) = data_style("#,##0.00").unwrap();
        assert!(kind == Kind::Number && xml.contains("decimal-places=\"2\"") && xml.contains("grouping=\"true\""), "{xml}");
        assert!(data_style("0.0%").unwrap().1 == Kind::Percent);
        let (xml, kind) = data_style("yyyy/mm/dd").unwrap();
        assert!(kind == Kind::Date && xml.contains("<number:year number:style=\"long\"/>") && xml.contains("<number:month number:style=\"long\"/>"), "{xml}");
        let (xml, kind) = data_style("h:mm:ss").unwrap();
        assert!(kind == Kind::Time && xml.contains("<number:minutes number:style=\"long\"/>"), "{xml}");
        assert!(data_style("General").is_none());
    }
}
