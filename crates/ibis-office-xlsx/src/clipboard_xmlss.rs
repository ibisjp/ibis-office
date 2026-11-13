// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Copied cells as Excel's "XML Spreadsheet" (SpreadsheetML 2003, `urn:schemas-microsoft-com:office:spreadsheet`), the
//! clipboard format Excel for Windows pastes with the cells' formulas and formats: it does not paste an embedded worksheet
//! put on the clipboard as memory (it wants a storage there), and the HTML table loses the formulas
//! (tasks/doc/excel-native-clipboard-investigation.md, 2026-11-08). The formulas are written in R1C1 (`ss:Formula`), as Excel
//! writes them.

use ibis_calc::number_to_xml;
use ibis_doc_model::{Border, BorderLine, CellAlign, CellRange, CellStyle, CellUnderline, CellValue, CellVerticalAlign, Rgb};

use super::clipboard::Clip;

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#10;"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// A column's letters (`AB`) as its index (0-based).
fn column_index(letters: &str) -> Option<u32> {
    let mut n = 0u32;
    for b in letters.bytes() {
        n = n.checked_mul(26)?.checked_add(u32::from(b - b'A') + 1)?;
    }
    n.checked_sub(1)
}

/// A formula of A1 references (`A2*2`, `SUM($B$1:B3)`, `Sheet2!C4`) in R1C1, for the cell at `row`, `column` (0-based):
/// a relative row or column as its distance (`R[-1]`, `C[2]`), an absolute one as its number (`R1`, `C3`), the same row or
/// column as nothing (`RC[-1]`). Text in quotes and the names of functions are left as they are.
pub(crate) fn a1_to_r1c1(formula: &str, row: u32, column: u32) -> String {
    let chars: Vec<char> = formula.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // Text and quoted sheet names are copied whole.
        if c == '"' || c == '\'' {
            let q = c;
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == q {
                    if chars.get(i + 1) == Some(&q) {
                        out.push(q);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        let word_before = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_' || chars[i - 1] == '.');
        if (c == '$' || c.is_ascii_uppercase()) && !word_before {
            // [$]LETTERS[$]DIGITS, not followed by a letter, a digit, '(' or '!'.
            let mut j = i;
            let col_abs = chars.get(j) == Some(&'$');
            if col_abs {
                j += 1;
            }
            let ls = j;
            while j < chars.len() && chars[j].is_ascii_uppercase() && j - ls < 3 {
                j += 1;
            }
            let letters: String = chars[ls..j].iter().collect();
            let row_abs = chars.get(j) == Some(&'$');
            let mut k = if row_abs { j + 1 } else { j };
            let ds = k;
            while k < chars.len() && chars[k].is_ascii_digit() {
                k += 1;
            }
            let digits: String = chars[ds..k].iter().collect();
            let after_ok = chars.get(k).is_none_or(|n| !(n.is_alphanumeric() || *n == '_' || *n == '(' || *n == '!' || *n == '.'));
            if !letters.is_empty() && !digits.is_empty() && after_ok
                && let (Some(col), Ok(r)) = (column_index(&letters), digits.parse::<u32>())
                && r >= 1
            {
                let r = r - 1;
                let rpart = if row_abs { format!("R{}", r + 1) } else if r == row { "R".to_string() } else { format!("R[{}]", r as i64 - row as i64) };
                let cpart = if col_abs { format!("C{}", col + 1) } else if col == column { "C".to_string() } else { format!("C[{}]", col as i64 - column as i64) };
                out.push_str(&rpart);
                out.push_str(&cpart);
                i = k;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn color(rgb: Rgb) -> String {
    format!("#{:06X}", rgb.0 & 0xFF_FFFF)
}

/// A style's SpreadsheetML (`ss:Style` children).
fn style_xml(s: &CellStyle) -> String {
    let mut x = String::new();
    let h = match s.align {
        CellAlign::Left => Some("Left"),
        CellAlign::Center => Some("Center"),
        CellAlign::Right => Some("Right"),
        CellAlign::Fill => Some("Fill"),
        CellAlign::Justify => Some("Justify"),
        CellAlign::CenterAcross => Some("CenterAcrossSelection"),
        CellAlign::Distributed => Some("Distributed"),
        CellAlign::General => None,
    };
    let v = match s.vertical {
        CellVerticalAlign::Top => "Top",
        CellVerticalAlign::Center => "Center",
        CellVerticalAlign::Bottom => "Bottom",
        CellVerticalAlign::Justify => "Justify",
        CellVerticalAlign::Distributed => "Distributed",
    };
    x += &format!(
        "<Alignment{} ss:Vertical=\"{v}\"{}{}{}{}/>",
        h.map(|h| format!(" ss:Horizontal=\"{h}\"")).unwrap_or_default(),
        if s.wrap { " ss:WrapText=\"1\"" } else { "" },
        if s.indent > 0 { format!(" ss:Indent=\"{}\"", s.indent) } else { String::new() },
        if s.shrink { " ss:ShrinkToFit=\"1\"" } else { "" },
        match s.rotation {
            0 => String::new(),
            255 => " ss:VerticalText=\"1\"".to_string(),
            r if r <= 90 => format!(" ss:Rotate=\"{r}\""),
            r => format!(" ss:Rotate=\"{}\"", 90 - i32::from(r.min(180))),
        }
    );
    let side = |name: &str, b: &Option<Border>| -> String {
        let Some(b) = b else { return String::new() };
        let (line, weight) = match b.line {
            BorderLine::Hair => ("Continuous", 0),
            BorderLine::Thin => ("Continuous", 1),
            BorderLine::Medium => ("Continuous", 2),
            BorderLine::Thick => ("Continuous", 3),
            BorderLine::Double => ("Double", 3),
            BorderLine::Dotted => ("Dot", 1),
            BorderLine::Dashed => ("Dash", 1),
            BorderLine::DashDot => ("DashDot", 1),
            BorderLine::DashDotDot => ("DashDotDot", 1),
            BorderLine::MediumDashed => ("Dash", 2),
            BorderLine::MediumDashDot => ("DashDot", 2),
            BorderLine::MediumDashDotDot => ("DashDotDot", 2),
            BorderLine::SlantDashDot => ("SlantDashDot", 2),
        };
        format!("<Border ss:Position=\"{name}\" ss:LineStyle=\"{line}\" ss:Weight=\"{weight}\" ss:Color=\"{}\"/>", color(b.color))
    };
    x += &format!("<Borders>{}{}{}{}</Borders>", side("Bottom", &s.bottom), side("Left", &s.left), side("Right", &s.right), side("Top", &s.top));
    let underline = match s.underline {
        CellUnderline::None => "",
        CellUnderline::Single => " ss:Underline=\"Single\"",
        CellUnderline::Double => " ss:Underline=\"Double\"",
        CellUnderline::SingleAccounting => " ss:Underline=\"SingleAccounting\"",
        CellUnderline::DoubleAccounting => " ss:Underline=\"DoubleAccounting\"",
    };
    x += &format!(
        "<Font{} ss:Size=\"{}\" ss:Color=\"{}\"{}{}{}{underline}/>",
        s.font.as_ref().map(|f| format!(" ss:FontName=\"{}\"", esc(f))).unwrap_or_default(),
        number_to_xml(f64::from(s.font_size) / 100.0),
        color(s.color),
        if s.bold { " ss:Bold=\"1\"" } else { "" },
        if s.italic { " ss:Italic=\"1\"" } else { "" },
        if s.strike { " ss:StrikeThrough=\"1\"" } else { "" },
    );
    x += &match s.fill {
        Some(fill) => format!("<Interior ss:Color=\"{}\" ss:Pattern=\"Solid\"/>", color(fill)),
        None => "<Interior/>".to_string(),
    };
    x += &if s.number_format.is_empty() || s.number_format == "General" { "<NumberFormat/>".to_string() } else { format!("<NumberFormat ss:Format=\"{}\"/>", esc(&s.number_format)) };
    x
}

/// The cells of `clip` (with its merged ranges, in the clip's own rows and columns) as an XML Spreadsheet; `None` with no cells.
pub(crate) fn write(clip: &Clip, merges: &[CellRange]) -> Option<String> {
    if clip.cells.is_empty() {
        return None;
    }
    let mut styles: Vec<String> = Vec::new();
    let mut cells: Vec<(u32, u32, usize, &super::clipboard::ClipCell)> = Vec::new();
    // A merged range is its first cell (`ss:MergeAcross`, `ss:MergeDown`); the cells it covers are not written.
    let covered = |r: u32, c: u32| merges.iter().any(|m| (m.first_row..=m.last_row).contains(&r) && (m.first_column..=m.last_column).contains(&c) && !(m.first_row == r && m.first_column == c));
    for c in clip.cells.iter().filter(|c| !covered(c.row, c.column)) {
        let xml = style_xml(&c.style);
        let id = styles.iter().position(|s| *s == xml).unwrap_or_else(|| {
            styles.push(xml);
            styles.len() - 1
        });
        cells.push((c.row, c.column, id, c));
    }
    cells.sort_by_key(|&(r, c, ..)| (r, c));
    let rows = cells.iter().map(|c| c.0).max().unwrap_or(0) + 1;
    let columns = cells.iter().map(|c| c.1).max().unwrap_or(0) + 1;
    let mut table = format!("<Table ss:ExpandedColumnCount=\"{columns}\" ss:ExpandedRowCount=\"{rows}\" x:FullColumns=\"1\" x:FullRows=\"1\">");
    // The columns' widths, in points (EMU / 12700).
    let mut widths: Vec<(u32, i64)> = clip.col_widths.iter().map(|&(c, w)| (c, w)).collect();
    widths.sort_by_key(|w| w.0);
    let mut next = 0;
    for (c, w) in widths {
        let index = if c == next { String::new() } else { format!(" ss:Index=\"{}\"", c + 1) };
        table += &format!("<Column{index} ss:Width=\"{}\"/>", number_to_xml((w as f64 / 12_700.0 * 100.0).round() / 100.0));
        next = c + 1;
    }
    let mut row = u32::MAX;
    let mut next_column = 0;
    for (r, c, id, cell) in &cells {
        if *r != row {
            if row != u32::MAX {
                table += "</Row>";
            }
            let index = if row == u32::MAX && *r == 0 || row != u32::MAX && *r == row + 1 { String::new() } else { format!(" ss:Index=\"{}\"", r + 1) };
            table += &format!("<Row{index}>");
            row = *r;
            next_column = 0;
        }
        let index = if *c == next_column { String::new() } else { format!(" ss:Index=\"{}\"", c + 1) };
        next_column = c + 1;
        let merge = merges.iter().find(|m| m.first_row == *r && m.first_column == *c).map(|m| {
            let mut a = String::new();
            if m.last_column > m.first_column {
                a += &format!(" ss:MergeAcross=\"{}\"", m.last_column - m.first_column);
            }
            if m.last_row > m.first_row {
                a += &format!(" ss:MergeDown=\"{}\"", m.last_row - m.first_row);
            }
            a
        });
        if let Some(m) = merges.iter().find(|m| m.first_row == *r && m.first_column == *c) {
            next_column = m.last_column + 1;
        }
        let formula = cell.cell.formula.as_ref().map(|f| format!(" ss:Formula=\"={}\"", esc(&a1_to_r1c1(f, cell.orig_row, cell.orig_column)))).unwrap_or_default();
        let data = match &cell.cell.value {
            CellValue::Number(n) => format!("<Data ss:Type=\"Number\">{}</Data>", number_to_xml(*n)),
            CellValue::Text(t) => format!("<Data ss:Type=\"String\">{}</Data>", esc(t)),
            CellValue::Bool(b) => format!("<Data ss:Type=\"Boolean\">{}</Data>", u8::from(*b)),
            CellValue::Error(e) => format!("<Data ss:Type=\"Error\">{}</Data>", esc(e.as_str())),
            CellValue::Empty => String::new(),
        };
        table += &format!("<Cell{index} ss:StyleID=\"s{id}\"{}{formula}>{data}</Cell>", merge.unwrap_or_default());
    }
    if row != u32::MAX {
        table += "</Row>";
    }
    table += "</Table>";
    let styles_xml: String = styles.iter().enumerate().map(|(i, s)| format!("<Style ss:ID=\"s{i}\">{s}</Style>")).collect();
    Some(format!(
        "<?xml version=\"1.0\"?>\r\n<?mso-application progid=\"Excel.Sheet\"?>\r\n<Workbook xmlns=\"urn:schemas-microsoft-com:office:spreadsheet\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\" xmlns:ss=\"urn:schemas-microsoft-com:office:spreadsheet\" xmlns:html=\"http://www.w3.org/TR/REC-html40\"><Styles>{styles_xml}</Styles><Worksheet ss:Name=\"Sheet1\">{table}</Worksheet></Workbook>"
    ))
}

#[cfg(test)]
mod tests {
    use super::a1_to_r1c1;
    use crate::clipboard::{Clip, ClipCell};
    use ibis_doc_model::{CellKind, CellRange, CellStyle, CellValue, Rgb, SheetCell, SlideId};

    fn cell(row: u32, column: u32, value: CellValue, formula: Option<&str>, style: CellStyle) -> ClipCell {
        let text = match &value {
            CellValue::Number(n) => n.to_string(),
            CellValue::Text(t) => t.to_string(),
            _ => String::new(),
        };
        ClipCell {
            row,
            column,
            cell: SheetCell { row, column, kind: CellKind::of(&value), value, text, formula: formula.map(str::to_string), style: 0, phonetic: None, runs: None },
            style,
            orig_row: row,
            orig_column: column,
        }
    }

    /// Two rows of cells copied from A1: a bold cell with a fill and a format, text to escape, formulas of relative and absolute
    /// references, a merged pair. `IBIS_XMLSS_OUT` names a file to write it to (to put it on the clipboard and paste it into
    /// Excel: `scripts/corpus/office_truth_win.ps1 -SaveAs paste`).
    #[test]
    fn copied_cells_as_an_xml_spreadsheet() {
        let bold = CellStyle { bold: true, fill: Some(Rgb(0x336699)), number_format: "0.00".into(), ..CellStyle::default() };
        let cells = vec![
            cell(0, 0, CellValue::Number(5.0), None, bold),
            cell(0, 1, CellValue::Text("a&b<c>".into()), None, CellStyle::default()),
            cell(0, 2, CellValue::Number(10.0), Some("A1*2"), CellStyle::default()),
            cell(1, 0, CellValue::Text("merged".into()), None, CellStyle::default()),
            cell(1, 1, CellValue::Empty, None, CellStyle::default()),
            cell(1, 2, CellValue::Number(5.0), Some("SUM($A$1:B1)"), CellStyle::default()),
        ];
        let clip = Clip {
            slide: SlideId(1),
            range: CellRange { first_row: 0, first_column: 0, last_row: 1, last_column: 2 },
            cells,
            cut: false,
            text: String::new(),
            validations: Vec::new(),
            cell_images: Vec::new(),
            col_widths: vec![(0, 12_700 * 80)],
            comments: Vec::new(),
            merges: Vec::new(),
            source: None,
            foreign: false,
            origin: None,
            cut_elsewhere: false,
            spill_ghosts: Vec::new(),
        };
        let merges = [CellRange { first_row: 1, first_column: 0, last_row: 1, last_column: 1 }];
        let xml = super::write(&clip, &merges).unwrap();
        if let Ok(out) = std::env::var("IBIS_XMLSS_OUT") {
            std::fs::write(out, &xml).unwrap();
        }
        assert!(xml.contains(" ss:Bold=\"1\"") && xml.contains("<Interior ss:Color=\"#336699\" ss:Pattern=\"Solid\"/>"), "{xml}");
        assert!(xml.contains("<NumberFormat ss:Format=\"0.00\"/>"), "{xml}");
        assert!(xml.contains("<Data ss:Type=\"String\">a&amp;b&lt;c&gt;</Data>"), "{xml}");
        assert!(xml.contains("ss:Formula=\"=RC[-2]*2\""), "{xml}");
        assert!(xml.contains("ss:Formula=\"=SUM(R1C1:R[-1]C[-1])\""), "{xml}");
        assert!(xml.contains("<Column ss:Width=\"80\"/>"), "{xml}");
        // The merged pair is its first cell; the cell it covers is not written, and C2 follows it without an index.
        assert!(xml.contains("ss:MergeAcross=\"1\"><Data ss:Type=\"String\">merged</Data></Cell><Cell ss:StyleID"), "{xml}");
    }

    #[test]
    fn a1_references_become_r1c1() {
        // B2 (row 1, column 1): A2 is the column before; $B$1 row 1 column 2; text and functions stay.
        assert_eq!(a1_to_r1c1("A2*2", 1, 1), "RC[-1]*2");
        assert_eq!(a1_to_r1c1("SUM($B$1:B3)+LOG10(4)", 1, 1), "SUM(R1C2:R[1]C)+LOG10(4)");
        assert_eq!(a1_to_r1c1("\"A1\"&'My Sheet'!C4", 1, 1), "\"A1\"&'My Sheet'!R[2]C[1]");
    }
}
