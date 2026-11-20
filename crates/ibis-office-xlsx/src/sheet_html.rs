// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A sheet as a standalone HTML page (File > Download > HTML): the values as they show, the cells' formats, merged cells and
//! column widths, in a `<table>`. Hidden rows and columns, formulas (the value shown stands in), charts and pictures are left out.
//! Unlike the clipboard's table (`clipboard_html`) it carries none of Excel's own (`mso-*`, `x:`) attributes.

use ibis_doc_model::{Presentation, Sheet, sheet_calc::sheet_of_slide};

/// A sheet with more cells in its used range than this is not turned into HTML (the page would be too big to open).
pub const MAX_CELLS: u64 = 1_000_000;

/// Why a sheet was not written.
#[derive(Debug, PartialEq, Eq)]
pub enum SheetHtmlError {
    NoSheet,
    TooLarge { cells: u64 },
}

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

fn is_hidden_column(sheet: &Sheet, column: u32) -> bool {
    sheet.columns.get(column as usize).is_some_and(|w| *w == 0)
}

fn is_hidden_row(sheet: &Sheet, row: u32) -> bool {
    sheet.rows.get(row as usize).is_some_and(|h| *h == 0)
}

/// The sheet at slide index `slide` as a whole HTML page (UTF-8).
pub fn sheet_to_html(presentation: &Presentation, slide: usize) -> Result<String, SheetHtmlError> {
    let page = presentation.slides.get(slide).ok_or(SheetHtmlError::NoSheet)?;
    let sheet = sheet_of_slide(page).ok_or(SheetHtmlError::NoSheet)?;
    let title = page.name.clone().unwrap_or_default();
    let (rows, columns) = sheet.cells.iter().filter(|c| !c.text.is_empty() || c.style != 0).fold((0u32, 0u32), |(r, c), x| (r.max(x.row + 1), c.max(x.column + 1)));
    let cells = rows as u64 * columns as u64;
    if cells > MAX_CELLS {
        return Err(SheetHtmlError::TooLarge { cells });
    }
    let merge_at = |r: u32, c: u32| sheet.merges.iter().find(|m| m.first_row == r && m.first_column == c);
    let covered = |r: u32, c: u32| {
        sheet.merges.iter().any(|m| (m.first_row..=m.last_row).contains(&r) && (m.first_column..=m.last_column).contains(&c) && !(m.first_row == r && m.first_column == c))
    };
    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<meta name=\"generator\" content=\"ibisOffice\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape(&title, false)));
    out.push_str("<style>body{margin:8px}.scroll{overflow-x:auto}table{border-collapse:collapse}td{padding:0 2px;overflow:hidden}</style>\n</head>\n<body>\n<div class=\"scroll\">\n<table>\n");
    for c in 0..columns {
        if is_hidden_column(sheet, c) {
            continue;
        }
        match sheet.columns.get(c as usize).copied().filter(|w| *w > 0) {
            Some(w) => out.push_str(&format!("<col style=\"width:{}px\">\n", (w as f64 / 9525.0).round() as u32)),
            None => out.push_str("<col>\n"),
        }
    }
    for r in 0..rows {
        if is_hidden_row(sheet, r) {
            continue;
        }
        out.push_str("<tr>\n");
        for c in 0..columns {
            if covered(r, c) || is_hidden_column(sheet, c) {
                continue;
            }
            let mut span = String::new();
            if let Some(m) = merge_at(r, c) {
                if m.last_column > m.first_column {
                    span.push_str(&format!(" colspan=\"{}\"", m.last_column - m.first_column + 1));
                }
                if m.last_row > m.first_row {
                    span.push_str(&format!(" rowspan=\"{}\"", m.last_row - m.first_row + 1));
                }
            }
            match sheet.cell(r, c) {
                Some(cell) => {
                    let style = sheet.styles.get(cell.style as usize).map(crate::clipboard_html::style_css).unwrap_or_default();
                    let style: Vec<&str> = style.split(';').filter(|d| !d.is_empty() && !d.starts_with("mso-") && !d.contains("text-underline-style")).collect();
                    out.push_str(&format!("<td{span} style=\"{}\">{}</td>\n", escape(&style.join(";"), true), escape(&cell.text, false)));
                }
                None => out.push_str(&format!("<td{span}></td>\n")),
            }
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n</div>\n</body>\n</html>\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(csv: &str) -> Presentation {
        let bytes = ibis_formats::csv::workbook_from(csv, "Data").expect("a workbook");
        ibis_formats::open(bytes).expect("opens").presentation
    }

    #[test]
    fn the_shown_values_are_a_table_with_the_text_escaped() {
        let html = sheet_to_html(&book("品名,数量\n\"a<b & \"\"c\"\"\",1234.5\n"), 0).unwrap();
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<meta charset=\"utf-8\">"));
        assert!(html.contains("品名"));
        assert!(html.contains("a&lt;b &amp; \"c\""), "{html}");
        assert!(html.contains("1,234.5") || html.contains("1234.5"), "{html}");
        assert_eq!(html.matches("<tr>").count(), 2);
        assert!(!html.contains("mso-") && !html.contains("x:num"), "none of Excel's own attributes");
    }

    #[test]
    fn a_sheet_with_nothing_is_an_empty_table_and_a_missing_sheet_is_an_error() {
        let html = sheet_to_html(&book(""), 0).unwrap();
        assert!(!html.contains("<tr>"));
        assert_eq!(sheet_to_html(&book(""), 5), Err(SheetHtmlError::NoSheet));
    }
}
