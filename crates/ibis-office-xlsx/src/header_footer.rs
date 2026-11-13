// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Headers and footers of printed sheets: Excel's codes (`&L`, `&C`, `&R`
//! sections; `&P`, `&N`, `&D`, `&T`, `&F`, `&A` fields; `&B`, `&I`, `&U`,
//! `&E`, `&S`, `&X`, `&Y` toggles; `&"font,style"`, `&12`, `&KRRGGBB`), laid
//! out on the page as Excel for Mac prints them (measured in its PDFs of
//! xlsx-XPAGE-28 to 37: each run starts on a whole point; a left section
//! at the left margin rounded up, a centred one at the page's middle
//! rounded (a half down), a right one ending at the right margin, its start
//! rounded down; the header's baseline the header margin and the line's
//! ascent, rounded up; the footer's the footer margin and its descent above
//! the page's foot, rounded down; the ascent and descent each with a third
//! of the font's line gap, fitted to those PDFs).

use ibis_doc_model::{CellStyle, Rgb, SheetPrint, TextStyle};

use crate::render::shape_view::Resources;

/// A run of a header or footer section, as its codes format it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HfRun {
    pub text: String,
    pub(crate) font: Option<String>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// 0 none, 1 single, 2 double.
    pub underline: u8,
    pub strike: bool,
    /// Points.
    pub size: f32,
    pub color: Option<Rgb>,
    /// 1 superscript, -1 subscript.
    pub(crate) shift: i8,
}

/// What the fields of a page stand for.
pub(crate) struct Fields<'a> {
    pub(crate) page: u32,
    pub(crate) pages: u32,
    pub(crate) sheet: &'a str,
    pub(crate) file: &'a str,
    pub(crate) date: String,
    pub(crate) time: String,
}

/// The left, centre and right sections of `code`, their fields filled in.
pub(crate) fn parse(code: &str, fields: &Fields, default_font: Option<&str>, default_size: f32) -> [Vec<HfRun>; 3] {
    let mut sections: [Vec<HfRun>; 3] = Default::default();
    let base = HfRun { font: default_font.map(str::to_string), size: default_size, ..HfRun::default() };
    let mut section = 1; // Text before any section code is centred.
    let mut style = base.clone();
    let mut text = String::new();
    let flush = |sections: &mut [Vec<HfRun>; 3], section: usize, style: &HfRun, text: &mut String| {
        if !text.is_empty() {
            sections[section].push(HfRun { text: std::mem::take(text), ..style.clone() });
        }
    };
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '&' || i + 1 >= chars.len() {
            text.push(c);
            i += 1;
            continue;
        }
        let k = chars[i + 1];
        i += 2;
        match k {
            '&' => text.push('&'),
            'L' | 'C' | 'R' => {
                flush(&mut sections, section, &style, &mut text);
                section = match k {
                    'L' => 0,
                    'C' => 1,
                    _ => 2,
                };
                style = base.clone();
            }
            'P' | 'N' => {
                // `&P+1`, `&P-1`: the number moved.
                let mut n = if k == 'P' { fields.page as i64 } else { fields.pages as i64 };
                if i < chars.len() && (chars[i] == '+' || chars[i] == '-') && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
                    let sign = if chars[i] == '+' { 1 } else { -1 };
                    let mut j = i + 1;
                    let mut v = 0i64;
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        v = v * 10 + chars[j].to_digit(10).unwrap_or(0) as i64;
                        j += 1;
                    }
                    n += sign * v;
                    i = j;
                }
                text.push_str(&n.to_string());
            }
            'D' => text.push_str(&fields.date),
            'T' => text.push_str(&fields.time),
            'F' => text.push_str(fields.file),
            'A' => text.push_str(fields.sheet),
            'Z' | 'G' => {}
            'B' | 'I' | 'U' | 'E' | 'S' | 'X' | 'Y' => {
                flush(&mut sections, section, &style, &mut text);
                match k {
                    'B' => style.bold = !style.bold,
                    'I' => style.italic = !style.italic,
                    'U' => style.underline = if style.underline == 1 { 0 } else { 1 },
                    'E' => style.underline = if style.underline == 2 { 0 } else { 2 },
                    'S' => style.strike = !style.strike,
                    'X' => style.shift = if style.shift == 1 { 0 } else { 1 },
                    _ => style.shift = if style.shift == -1 { 0 } else { -1 },
                }
            }
            'K' => {
                flush(&mut sections, section, &style, &mut text);
                let hex: String = chars[i..].iter().take(6).collect();
                if hex.len() == 6 && hex.chars().all(|h| h.is_ascii_hexdigit()) {
                    style.color = u32::from_str_radix(&hex, 16).ok().map(Rgb);
                }
                i = (i + 6).min(chars.len());
            }
            '"' => {
                flush(&mut sections, section, &style, &mut text);
                let end = chars[i..].iter().position(|&x| x == '"').map_or(chars.len(), |p| i + p);
                let spec: String = chars[i..end].iter().collect();
                i = (end + 1).min(chars.len());
                let (name, kind) = spec.split_once(',').unwrap_or((spec.as_str(), ""));
                if name != "-" && !name.is_empty() {
                    style.font = Some(name.to_string());
                }
                let kind = kind.to_lowercase();
                style.bold = kind.contains("bold");
                style.italic = kind.contains("italic") || kind.contains("oblique");
            }
            d if d.is_ascii_digit() => {
                flush(&mut sections, section, &style, &mut text);
                let mut v = d.to_digit(10).unwrap_or(0);
                while i < chars.len() && chars[i].is_ascii_digit() {
                    v = v * 10 + chars[i].to_digit(10).unwrap_or(0);
                    i += 1;
                }
                style.size = v.max(1) as f32;
            }
            other => {
                // Not a code: kept as written.
                text.push('&');
                text.push(other);
            }
        }
    }
    flush(&mut sections, section, &style, &mut text);
    sections
}

/// A piece of a header or footer placed on the page (points from the
/// page's top-left corner).
#[derive(Clone, Debug)]
pub struct PageMark {
    pub x: f32,
    pub baseline: f32,
    pub run: HfRun,
    pub font: gpui_kit::Font,
}

/// A run's own size and the baseline's shift for super- and subscript.
fn run_size(run: &HfRun, scale: f32) -> (f32, f32) {
    let size = run.size * scale;
    match run.shift {
        1 => (size * 0.65, -size * 0.33),
        -1 => (size * 0.65, size * 0.14),
        _ => (size, 0.0),
    }
}

/// The marks of the header (`header`) or footer of one page.
pub(crate) fn layout(sections: &[Vec<HfRun>; 3], header: bool, print: &SheetPrint, scale: f32, res: &Resources) -> Vec<PageMark> {
    const PT: f32 = 12_700.0;
    let (paper_w, paper_h) = print.page_size();
    let (page_w, page_h) = (paper_w as f32 / PT, paper_h as f32 / PT);
    let hf = &print.header_footer;
    let k = if hf.scale_with_doc { scale } else { 1.0 };
    // Not aligned with the margins: three quarters of an inch from the edges
    // (Excel for Mac, xlsx-XPAGE-37; one sample).
    let (left_edge, right_edge) = if hf.align_with_margins { (print.margins[0] as f32 / PT, page_w - print.margins[1] as f32 / PT) } else { (54.0, page_w - 54.0) };
    let font_of = |run: &HfRun| {
        res.font(&TextStyle { font: run.font.clone(), font_size: (run.size * 100.0) as u32, bold: run.bold, italic: run.italic, ..TextStyle::default() })
    };
    // Each section's lines of runs (a line break in the text starts a new
    // line) and their widths; the lines' ascent and descent.
    let mut placed: Vec<(usize, Vec<Vec<(HfRun, gpui_kit::Font, f32, f32)>>)> = Vec::new();
    let (mut ascent, mut descent) = (0f32, 0f32);
    for (s, runs) in sections.iter().enumerate() {
        if runs.is_empty() {
            continue;
        }
        let mut lines: Vec<Vec<(HfRun, gpui_kit::Font, f32, f32)>> = vec![Vec::new()];
        for run in runs {
            let font = font_of(run);
            let (size, shift) = run_size(run, k);
            // The font's line gap, a third of it over the ascent and a third
            // under the descent (fitted to Excel's PDFs: 游ゴシック 11 pt,
            // whose gap is half an em, sits 2 pt lower than its ascent
            // alone; Times New Roman 18 pt, with almost none, does not).
            let (a, d, gap) = res.line_metrics(&font);
            ascent = ascent.max((a + gap / 3.0) * size - shift);
            descent = descent.max((d + gap / 3.0) * size + shift);
            let text = run.text.replace('\r', "");
            for (n, part) in text.split('\n').enumerate() {
                if n > 0 {
                    lines.push(Vec::new());
                }
                if part.is_empty() {
                    continue;
                }
                let part_run = HfRun { text: part.to_string(), ..run.clone() };
                let width = res.text_width(part, &font, size);
                lines.last_mut().expect("a line").push((part_run, font.clone(), width, shift));
            }
        }
        placed.push((s, lines));
    }
    let baseline = if header {
        (print.header_margin as f32 / PT + ascent - 1e-3).ceil()
    } else {
        (page_h - print.footer_margin as f32 / PT - descent + 1e-3).floor()
    };
    // The distance between the lines of a section.
    let pitch = (ascent + descent).ceil();
    let mut out = Vec::new();
    for (s, lines) in placed {
        let count = lines.len();
        for (row, items) in lines.into_iter().enumerate() {
            // A header's lines go down from its first, a footer's up from its last.
            let line_baseline = if header { baseline + row as f32 * pitch } else { baseline - (count - 1 - row) as f32 * pitch };
            // Each run starts on a whole point.
            let mut width = 0.0;
            for (i, (_, _, w, _)) in items.iter().enumerate() {
                width = if i + 1 == items.len() { width + w } else { (width + w - 1e-3).ceil() };
            }
            let mut x = match s {
                0 => (left_edge - 1e-3).ceil(),
                1 => ((page_w - width) / 2.0 - 0.5).ceil(),
                _ => (right_edge - width + 1e-3).floor(),
            };
            for (run, font, w, shift) in items {
                let mut run = run;
                run.size = run_size(&run, k).0;
                out.push(PageMark { x, baseline: line_baseline + shift, run, font });
                x = (x + w - 1e-3).ceil();
            }
        }
    }
    out
}

/// The header and footer marks of page `page` (from 1) of `pages`.
pub fn page_marks(print: &SheetPrint, default_style: &CellStyle, scale: f32, page: u32, pages: u32, sheet: &str, file: &str, res: &Resources) -> Vec<PageMark> {
    let hf = &print.header_footer;
    if hf.is_empty() {
        return Vec::new();
    }
    let number = if print.use_first_page_number { print.first_page_number.unwrap_or(1) + page - 1 } else { page };
    let total = if print.use_first_page_number { print.first_page_number.unwrap_or(1) + pages - 1 } else { pages };
    // The date and time now, in the computer's time zone.
    let now = crate::clock::local_now();
    let fields = Fields { page: number, pages: total, sheet, file, date: format!("{}/{}/{}", now.year, now.month, now.day), time: format!("{}:{:02}", now.hour, now.minute) };
    let (header, footer) = if hf.different_first && page == 1 {
        (hf.first_header.as_deref(), hf.first_footer.as_deref())
    } else if hf.different_odd_even && page % 2 == 0 {
        (hf.even_header.as_deref(), hf.even_footer.as_deref())
    } else {
        (hf.odd_header.as_deref(), hf.odd_footer.as_deref())
    };
    let size = default_style.font_size as f32 / 100.0;
    let font = default_style.font.as_deref();
    let mut out = Vec::new();
    for (code, is_header) in [(header, true), (footer, false)] {
        if let Some(code) = code {
            out.extend(layout(&parse(code, &fields, font, if size > 0.0 { size } else { 11.0 }), is_header, print, scale, res));
        }
    }
    out
}

/// The size (points, unscaled) of printed headings (`printOptions/@headings`):
/// the row numbers' column as wide as the last row's number and a margin
/// (23 pt for 2 digits of 11 pt 游ゴシック in Excel for Mac's PDF of
/// xlsx-XPAGE-27: the digits and 10.8 pt, rounded; one sample), the column letters'
/// row as tall as the sheet's default row.
pub fn heading_size(sheet: &ibis_doc_model::Sheet, default_style: &CellStyle, res: &Resources) -> (f64, f64) {
    let size = if default_style.font_size > 0 { default_style.font_size as f32 / 100.0 } else { 11.0 };
    let font = res.font(&TextStyle { font: default_style.font.clone(), font_size: (size * 100.0) as u32, ..TextStyle::default() });
    // As many digits as the last row used has.
    let last = sheet.cells.last().map_or(1, |c| c.row as usize + 1);
    let digits = last.to_string().len().max(2);
    let width = res.text_width(&"0".repeat(digits), &font, size);
    let row = if sheet.default_row > 0 { sheet.default_row as f64 / 12_700.0 } else { 18.0 };
    (((width + 10.8) as f64).round(), row)
}

/// The column letters and row numbers of printed headings: each centred in
/// its column (row), rounded down to a whole point, its baseline above the bottom of
/// its band (5 pt at 100 %: Excel for Mac, xlsx-XPAGE-27). `cols` and `rows` are (index,
/// from, to) in EMU of the slide, in the order printed; `corner` the
/// headings' top-left corner and `size` their width and height, in points.
#[allow(clippy::too_many_arguments)]
pub fn heading_marks(cols: &[(u32, i64, i64)], rows: &[(u32, i64, i64)], corner: (f32, f32), size: (f32, f32), px_per_emu: f32, default_style: &CellStyle, scale: f32, res: &Resources) -> Vec<PageMark> {
    let font_size = (if default_style.font_size > 0 { default_style.font_size as f32 / 100.0 } else { 11.0 }) * scale;
    let font = res.font(&TextStyle { font: default_style.font.clone(), font_size: default_style.font_size, ..TextStyle::default() });
    let run = |text: String| HfRun { text, font: default_style.font.clone(), size: font_size, ..HfRun::default() };
    let mut out = Vec::new();
    let mut x = corner.0 + size.0;
    for &(k, a, b) in cols {
        let w = (b - a) as f32 * px_per_emu;
        let label = ibis_doc_model::cell_name(k, 0).trim_end_matches('1').to_string();
        let tw = res.text_width(&label, &font, font_size);
        out.push(PageMark { x: (x + (w - tw) / 2.0 + 1e-3).floor(), baseline: (corner.1 + size.1 - 5.0 * scale).round(), run: run(label), font: font.clone() });
        x += w;
    }
    let mut y = corner.1 + size.1;
    for &(k, a, b) in rows {
        let h = (b - a) as f32 * px_per_emu;
        let label = (k + 1).to_string();
        let tw = res.text_width(&label, &font, font_size);
        out.push(PageMark { x: (corner.0 + (size.0 - tw) / 2.0 + 1e-3).floor(), baseline: (y + h - 5.0 * scale).round(), run: run(label), font: font.clone() });
        y += h;
    }
    out
}

/// The rows and columns of a print titles name (`Sheet1!$1:$2,Sheet1!$A:$B`,
/// either alone), each as first and last (from 0).
pub fn parse_titles(formula: &str) -> (Option<(u32, u32)>, Option<(u32, u32)>) {
    let (mut rows, mut columns) = (None, None);
    for part in formula.split(',') {
        let r = part.rsplit_once('!').map_or(part, |(_, r)| r).replace('$', "");
        let Some((a, b)) = r.trim().split_once(':') else { continue };
        if let (Ok(a), Ok(b)) = (a.parse::<u32>(), b.parse::<u32>()) {
            if a > 0 && b >= a {
                rows = Some((a - 1, b - 1));
            }
        } else if a.chars().all(|c| c.is_ascii_alphabetic()) && b.chars().all(|c| c.is_ascii_alphabetic()) {
            let col = |s: &str| ibis_calc::formula::parser::parse_cell(&format!("{}1", s.to_uppercase())).map(|c| c.column);
            if let (Some(a), Some(b)) = (col(a), col(b)) {
                columns = Some((a.min(b), a.max(b)));
            }
        }
    }
    (rows, columns)
}

/// Split a raw Excel header/footer code string into its Left, Center and Right sections.
/// Text before any section code belongs to Center.
/// Escaped ampersands `&&` are preserved.
pub(crate) fn split_sections(code: &str) -> (String, String, String) {
    let mut left = String::new();
    let mut center = String::new();
    let mut right = String::new();
    let chars: Vec<char> = code.chars().collect();
    let mut current_sec = 1; // 0: Left, 1: Center, 2: Right. Defaults to Center.
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' && i + 1 < chars.len() {
            match chars[i + 1] {
                'L' => {
                    current_sec = 0;
                    i += 2;
                    continue;
                }
                'C' => {
                    current_sec = 1;
                    i += 2;
                    continue;
                }
                'R' => {
                    current_sec = 2;
                    i += 2;
                    continue;
                }
                '&' => {
                    let target = match current_sec {
                        0 => &mut left,
                        1 => &mut center,
                        _ => &mut right,
                    };
                    target.push('&');
                    target.push('&');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        let target = match current_sec {
            0 => &mut left,
            1 => &mut center,
            _ => &mut right,
        };
        target.push(chars[i]);
        i += 1;
    }
    (left, center, right)
}

/// Combine Left, Center, and Right section strings into an Excel header/footer code string.
/// Returns None if all sections are empty.
pub(crate) fn join_sections(left: &str, center: &str, right: &str) -> Option<String> {
    let mut out = String::new();
    let left = left.trim();
    let center = center.trim();
    let right = right.trim();
    if !left.is_empty() {
        out.push_str("&L");
        out.push_str(left);
    }
    if !center.is_empty() {
        out.push_str("&C");
        out.push_str(center);
    }
    if !right.is_empty() {
        out.push_str("&R");
        out.push_str(right);
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::{Fields, join_sections, parse, parse_titles, split_sections};

    #[test]
    fn print_titles_are_read() {
        assert_eq!(parse_titles("'売上 表'!$1:$2,'売上 表'!$A:$B"), (Some((0, 1)), Some((0, 1))));
        assert_eq!(parse_titles("Sheet1!$3:$3"), (Some((2, 2)), None));
    }

    #[test]
    fn codes_are_read() {
        let f = Fields { page: 2, pages: 5, sheet: "売上", file: "a.xlsx", date: "2026/10/7".into(), time: "9:05".into() };
        let [l, c, r] = parse("&L&A&C&BPage&B &P of &N&R&\"Arial,Bold Italic\"&14&KFF0000&F &&", &f, Some("游ゴシック"), 11.0);
        assert_eq!(l.iter().map(|x| x.text.as_str()).collect::<String>(), "売上");
        assert_eq!(c.len(), 2);
        assert!(c[0].bold && c[0].text == "Page" && !c[1].bold && c[1].text == " 2 of 5");
        assert_eq!(r[0].text, "a.xlsx &");
        assert_eq!((r[0].font.as_deref(), r[0].bold, r[0].italic, r[0].size, r[0].color), (Some("Arial"), true, true, 14.0, Some(ibis_doc_model::Rgb(0xFF0000))));
        let [_, c, _] = parse("x&X2&X y&Y2&Y &P+1", &f, None, 11.0);
        assert_eq!(c.iter().map(|x| (x.text.as_str(), x.shift)).collect::<Vec<_>>(), vec![("x", 0), ("2", 1), (" y", 0), ("2", -1), (" 3", 0)]);
    }

    #[test]
    fn split_and_join_sections_work() {
        assert_eq!(split_sections(""), (String::new(), String::new(), String::new()));
        assert_eq!(join_sections("", "", ""), None);

        let code = "&L&A&C&P / &N&R&D";
        let (l, c, r) = split_sections(code);
        assert_eq!(l, "&A");
        assert_eq!(c, "&P / &N");
        assert_eq!(r, "&D");
        assert_eq!(join_sections(&l, &c, &r), Some(code.to_string()));

        let code_center = "&C&P";
        let (l, c, r) = split_sections(code_center);
        assert_eq!((l.as_str(), c.as_str(), r.as_str()), ("", "&P", ""));
        assert_eq!(join_sections(&l, &c, &r), Some("&C&P".to_string()));

        let raw_text = "中央のみのテキスト";
        let (l, c, r) = split_sections(raw_text);
        assert_eq!((l.as_str(), c.as_str(), r.as_str()), ("", "中央のみのテキスト", ""));

        let escaped = "&L左&&ロゴ&R右&&端";
        let (l, c, r) = split_sections(escaped);
        assert_eq!((l.as_str(), c.as_str(), r.as_str()), ("左&&ロゴ", "", "右&&端"));
        assert_eq!(join_sections(&l, &c, &r), Some("&L左&&ロゴ&R右&&端".to_string()));
    }
}
