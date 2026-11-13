// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 形式を選択して貼り付け: every combination of the dialog's paste type (10) x operation (5) x skip blanks x transpose, for
//! cells copied here and for cells that only the clipboard holds, against what Excel did (`test/editing/xlsx-paste-special-excel.tsv`,
//! measured in Excel 365 for Windows through COM `Range.PasteSpecial`, see `tasks/doc/xlsx-paste-special-matrix.md`).
//!
//! The source A1:C2 holds a number with bold, fill, font colour, number format, borders, a comment and a validation (A1), a formula
//! (B1) with its own fill, colour and format, a text merged down (C1:C2), a blank formatted cell (A2) and a number (B2); it has its own
//! column widths. The destination E5:G7 holds numbers, a text, a formula, a fill, left borders, a comment (F7) and its own widths.
//! `IBIS_PASTE_MATRIX_OUT=<file>` writes the table of results (Markdown).

use super::SheetSteps;
use crate::shell::scenario::{Scenario, repo_root};
use crate::clipboard::{PasteOp, PasteSpecialOptions, PasteType};
use ibis_doc_model::sheet_command::{Axis, Edge, StyleChange};
use ibis_doc_model::{Border, BorderLine, CellEdit, CellInput, CellRange, CellValue, Command, Rgb, SheetCommand, SheetComment, Validation, sheet_calc::sheet_of_slide};
use std::collections::BTreeMap;
use crate::ext::*;

pub(super) fn range(r0: u32, c0: u32, r1: u32, c1: u32) -> CellRange {
    CellRange { first_row: r0, first_column: c0, last_row: r1, last_column: c1 }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Snap {
    pub(super) formula: Option<String>,
    pub(super) value: String,
    pub(super) bold: bool,
    pub(super) fill: Option<String>,
    pub(super) color: String,
    pub(super) number_format: String,
    pub(super) borders: [bool; 4],
    pub(super) comment: Option<String>,
    pub(super) validation: bool,
    pub(super) merge: Option<String>,
}

fn hex(c: Rgb) -> String {
    format!("{:06X}", c.0)
}

pub(super) fn setup(s: &mut Scenario) {
    setup_with(s, "none");
}

/// The sheet of the matrix, with a merged range of the destination (`preset`: `m_in` E5:F5, `m_straddle` F6:G7, `m_same` G5:G6,
/// `m_outside` F7:G7, `m_all` E5:G6).
pub(super) fn setup_with(s: &mut Scenario, preset: &str) {
    setup_deco(s, preset, "none");
}

/// `setup_with` and a decoration of the destination, put on before the merged range is made (as in the Excel measure): `dec`
/// thin borders on every edge of E5:G7 and comments on E5, E6 and G6; `refs` formulas that point at the copied cells (J9, K3, L11).
pub(super) fn setup_deco(s: &mut Scenario, preset: &str, deco: &str) {
    let preset = preset.to_string();
    let deco = deco.to_string();
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let num = |row: u32, column: u32, n: f64| CellEdit { row, column, input: CellInput::Value(CellValue::Number(n)) };
        let mut edits = vec![
            num(0, 0, 10.0),
            CellEdit { row: 0, column: 1, input: CellInput::Formula("A1*2".into()) },
            CellEdit { row: 0, column: 2, input: CellInput::Value(CellValue::Text("abc".into())) },
            num(1, 1, 4.0),
        ];
        edits.push(num(4, 4, 100.0));
        edits.push(num(4, 5, 200.0));
        edits.push(num(4, 6, 300.0));
        edits.push(CellEdit { row: 5, column: 4, input: CellInput::Value(CellValue::Text("txt".into())) });
        edits.push(CellEdit { row: 5, column: 5, input: CellInput::Formula("E5+1".into()) });
        edits.push(num(5, 6, 50.0));
        edits.push(num(6, 4, 7.0));
        edits.push(num(6, 5, 8.0));
        edits.push(num(6, 6, 9.0));
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
        let fmt = |r: CellRange, change: StyleChange| Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![r], change });
        let line = Some(Border { line: BorderLine::Thin, color: Rgb(0) });
        let a1 = range(0, 0, 0, 0);
        commands.push(fmt(a1, StyleChange::Bold(true)));
        commands.push(fmt(a1, StyleChange::Fill(Some(Rgb(0xFF0000)))));
        commands.push(fmt(a1, StyleChange::Color(Rgb(0x0000FF))));
        commands.push(fmt(a1, StyleChange::NumberFormat("0.00".into())));
        commands.push(fmt(a1, StyleChange::Border(Edge::Top, line)));
        commands.push(fmt(a1, StyleChange::Border(Edge::Left, line)));
        let b1 = range(0, 1, 0, 1);
        commands.push(fmt(b1, StyleChange::Fill(Some(Rgb(0xFFFF00)))));
        commands.push(fmt(b1, StyleChange::Color(Rgb(0x008000))));
        commands.push(fmt(b1, StyleChange::NumberFormat("#,##0".into())));
        commands.push(fmt(range(0, 2, 0, 2), StyleChange::Bold(true)));
        commands.push(fmt(range(1, 0, 1, 0), StyleChange::Fill(Some(Rgb(0x99CCFF)))));
        commands.push(fmt(range(1, 0, 1, 0), StyleChange::Border(Edge::Bottom, line)));
        commands.push(fmt(range(1, 1, 1, 1), StyleChange::NumberFormat("0.0".into())));
        commands.push(Command::Sheet(SheetCommand::Merge { slide, range: range(0, 2, 1, 2), center: false }));
        let dest = range(4, 4, 6, 6);
        commands.push(fmt(dest, StyleChange::Fill(Some(Rgb(0xD9D9D9)))));
        commands.push(fmt(dest, StyleChange::Border(Edge::Left, line)));
        commands.push(Command::Sheet(SheetCommand::SetComment { slide, row: 0, column: 0, comment: Some(SheetComment { row: 0, column: 0, author: "src".into(), text: "src note".into(), ..Default::default() }) }));
        commands.push(Command::Sheet(SheetCommand::SetComment { slide, row: 6, column: 5, comment: Some(SheetComment { row: 6, column: 5, author: "dst".into(), text: "dst note".into(), ..Default::default() }) }));
        let rule = Validation { ranges: vec![a1], kind: "whole".into(), operator: "between".into(), formula1: Some("1".into()), formula2: Some("100".into()), allow_blank: true, ..Default::default() };
        commands.push(Command::Sheet(SheetCommand::SetValidation { slide, ranges: vec![a1], rule: Some(rule) }));
        let merged = match preset.as_str() {
            "m_in" => Some(range(4, 4, 4, 5)),
            "m_straddle" => Some(range(5, 5, 6, 6)),
            "m_same" => Some(range(4, 6, 5, 6)),
            "m_outside" => Some(range(6, 5, 6, 6)),
            "m_all" => Some(range(4, 4, 5, 6)),
            _ => None,
        };
        if deco == "dec" {
            for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
                commands.push(fmt(dest, StyleChange::Border(edge, line)));
            }
            for (row, column, text) in [(4u32, 4u32, "c1"), (5, 4, "c2"), (5, 6, "c3")] {
                commands.push(Command::Sheet(SheetCommand::SetComment { slide, row, column, comment: Some(SheetComment { row, column, author: String::new(), text: text.into(), ..Default::default() }) }));
            }
        }
        if deco == "refs" {
            let formula = |row: u32, column: u32, f: &str| CellEdit { row, column, input: CellInput::Formula(f.into()) };
            commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits: vec![formula(8, 9, "A1+B2"), formula(2, 10, "C1"), formula(10, 11, "SUM(A1:C2)")] }));
        }
        if let Some(range) = merged {
            commands.push(Command::Sheet(SheetCommand::Merge { slide, range, center: false }));
        }
        // The widths first: selecting a column that holds a merged range takes the whole range in.
        for (column, chars) in [(0u32, 20.0f32), (1, 12.0), (2, 15.0), (3, 8.0), (4, 8.0), (5, 9.0), (6, 10.0), (7, 8.0), (8, 8.0), (9, 8.0), (10, 8.0), (11, 8.0)] {
            e.select_lines(Axis::Columns, column, column, cx);
            e.set_selected_column_width(chars, cx);
        }
        e.document.execute(commands).unwrap();
    });
}

/// Borders as Excel reports them: an edge shows when the cell or its neighbour draws it.
pub(super) fn snapshot(s: &mut Scenario) -> (BTreeMap<String, Snap>, Vec<i64>) {
    snapshot_in(s, 2..=11, 4..=11, &[4, 5, 6])
}

/// `snapshot` over the cells A1:L12 and the widths of the columns A..L.
pub(super) fn snapshot_all(s: &mut Scenario) -> (BTreeMap<String, Snap>, Vec<i64>) {
    snapshot_in(s, 0..=11, 0..=11, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11])
}

fn snapshot_in(s: &mut Scenario, rows: std::ops::RangeInclusive<u32>, columns: std::ops::RangeInclusive<u32>, widths: &[usize]) -> (BTreeMap<String, Snap>, Vec<i64>) {
    s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        let style = |r: u32, c: u32| sheet.cell(r, c).map(|x| &sheet.styles[x.style as usize]);
        let mut out = BTreeMap::new();
        for r in rows.clone() {
            for c in columns.clone() {
                let cell = sheet.cell(r, c);
                // Excel reports the font and fill of a merged range's first cell for every cell it covers.
                let (fr, fc) = sheet.merges.iter().find(|m| (m.first_row..=m.last_row).contains(&r) && (m.first_column..=m.last_column).contains(&c)).map_or((r, c), |m| (m.first_row, m.first_column));
                let st = style(fr, fc);
                let own_style = style(r, c);
                let own = |f: fn(&ibis_doc_model::CellStyle) -> bool, rr: u32, cc: u32| style(rr, cc).is_some_and(f);
                let borders = [
                    own(|s| s.left.is_some(), r, c) || (c > 0 && own(|s| s.right.is_some(), r, c - 1)),
                    own(|s| s.top.is_some(), r, c) || (r > 0 && own(|s| s.bottom.is_some(), r - 1, c)),
                    own(|s| s.bottom.is_some(), r, c) || own(|s| s.top.is_some(), r + 1, c),
                    own(|s| s.right.is_some(), r, c) || own(|s| s.left.is_some(), r, c + 1),
                ];
                let value = match cell.map(|x| &x.value) {
                    None | Some(CellValue::Empty) => "-".to_string(),
                    Some(CellValue::Number(n)) => format!("n:{n}"),
                    Some(CellValue::Text(t)) => format!("s:{t}"),
                    Some(CellValue::Error(e)) => format!("e:{e:?}"),
                    Some(other) => format!("{other:?}"),
                };
                let merge = sheet.merges.iter().find(|m| (m.first_row..=m.last_row).contains(&r) && (m.first_column..=m.last_column).contains(&c)).map(|m| format!("{}:{}", ibis_doc_model::cell_name(m.first_column, m.first_row), ibis_doc_model::cell_name(m.last_column, m.last_row)));
                let validation = sheet.validations.iter().any(|v| v.kind == "whole" && v.ranges.iter().any(|vr| (vr.first_row..=vr.last_row).contains(&r) && (vr.first_column..=vr.last_column).contains(&c)));
                out.insert(
                    ibis_doc_model::cell_name(c, r),
                    Snap {
                        formula: cell.and_then(|x| x.formula.clone()).map(|f| norm_formula(&f)),
                        value,
                        bold: st.is_some_and(|s| s.bold),
                        fill: st.and_then(|s| s.fill).map(hex),
                        color: st.map_or("000000".into(), |s| hex(s.color)),
                        // (Not the range's: Excel reports a covered cell's own number format.)
                        number_format: own_style.map_or("General".into(), |s| s.number_format.clone()),
                        borders,
                        comment: sheet.comment_at(r, c).map(|x| x.text.clone()),
                        validation,
                        merge,
                    },
                );
            }
        }
        (out, widths.iter().map(|&c| sheet.columns.get(c).copied().unwrap_or(sheet.default_column) as i64).collect())
    })
}

fn norm_formula(f: &str) -> String {
    format!("={}", f.trim_start_matches('=').replace(' ', "").to_uppercase())
}

/// The truth file: key -> (cells by address, widths E F G in Excel characters).
pub(super) fn truth_of(file: &str) -> (BTreeMap<String, BTreeMap<String, Snap>>, BTreeMap<String, Vec<i64>>, std::collections::BTreeSet<String>) {
    let text = std::fs::read_to_string(repo_root().join(file)).unwrap();
    let (mut cells, mut widths): (BTreeMap<String, BTreeMap<String, Snap>>, BTreeMap<String, Vec<i64>>) = Default::default();
    let mut refused = std::collections::BTreeSet::new();
    for line in text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        if f[1] == "E" {
            if f[2] == "1" {
                refused.insert(f[0].to_string());
            }
            continue;
        }
        if f[1] == "S" {
            // The lines of this key list only the cells that are not blank and unformatted: the region's other cells are blank.
            let at = |name: &str| {
                let letters: String = name.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
                (name[letters.len()..].parse::<u32>().unwrap() - 1, letters.bytes().fold(0u32, |n, b| n * 26 + (b - b'A' + 1) as u32) - 1)
            };
            let ((r0, c0), (r1, c1)) = (at(f[2]), at(f[3]));
            let blank = Snap { formula: None, value: "-".into(), bold: false, fill: None, color: "000000".into(), number_format: "General".into(), borders: [false; 4], comment: None, validation: false, merge: None };
            let entry = cells.entry(f[0].into()).or_default();
            for r in r0..=r1 {
                for c in c0..=c1 {
                    entry.insert(ibis_doc_model::cell_name(c, r), blank.clone());
                }
            }
            continue;
        }
        if f[1] == "W" {
            widths.insert(f[0].into(), f[2..].iter().map(|x| x.parse().unwrap()).collect());
            continue;
        }
        let b: Vec<bool> = f[8].chars().map(|c| c == '1').collect();
        let opt = |x: &str| (x != "-").then(|| x.to_string());
        cells.entry(f[0].into()).or_default().insert(
            f[1].into(),
            Snap {
                formula: opt(f[2]).filter(|x| x.starts_with('=')).map(|x| norm_formula(&x)),
                value: f[3].to_string(),
                bold: f[4] == "1",
                fill: opt(f[5]),
                color: f[6].into(),
                number_format: f[7].into(),
                borders: [b[0], b[1], b[2], b[3]],
                comment: opt(f[9]),
                validation: f[10] == "1",
                merge: opt(f[11]),
            },
        );
    }
    (cells, widths, refused)
}

fn truth() -> (BTreeMap<String, BTreeMap<String, Snap>>, BTreeMap<String, Vec<i64>>) {
    let (cells, widths, _) = truth_of("test/editing/xlsx-paste-special-excel.tsv");
    (cells, widths)
}

fn describe(x: &Snap) -> String {
    format!(
        "{}{} b{} fill={} fc={} nf={} bd={} cm={} dv={} mg={}",
        x.formula.clone().unwrap_or_default(),
        x.value,
        x.bold as u8,
        x.fill.clone().unwrap_or("-".into()),
        x.color,
        x.number_format,
        x.borders.iter().map(|&b| if b { '1' } else { '0' }).collect::<String>(),
        x.comment.clone().unwrap_or("-".into()),
        x.validation as u8,
        x.merge.clone().unwrap_or("-".into())
    )
}

fn same_value(a: &str, b: &str) -> bool {
    match (a.strip_prefix("n:"), b.strip_prefix("n:")) {
        (Some(x), Some(y)) => matches!((x.parse::<f64>(), y.parse::<f64>()), (Ok(x), Ok(y)) if (x - y).abs() <= 1e-9 * x.abs().max(1.0)),
        _ => a == b,
    }
}

pub(super) fn differences(expected: &BTreeMap<String, Snap>, got: &BTreeMap<String, Snap>) -> Vec<String> {
    let mut out = Vec::new();
    for (addr, e) in expected {
        let g = &got[addr];
        let mut parts = Vec::new();
        if e.formula != g.formula {
            parts.push(format!("formula {:?} -> {:?}", e.formula, g.formula));
        }
        if !same_value(&e.value, &g.value) {
            parts.push(format!("value {} -> {}", e.value, g.value));
        }
        if e.bold != g.bold {
            parts.push(format!("bold {} -> {}", e.bold, g.bold));
        }
        if e.fill != g.fill {
            parts.push(format!("fill {:?} -> {:?}", e.fill, g.fill));
        }
        if e.color != g.color {
            parts.push(format!("color {} -> {}", e.color, g.color));
        }
        if e.number_format != g.number_format {
            parts.push(format!("numfmt {} -> {}", e.number_format, g.number_format));
        }
        if e.borders != g.borders {
            parts.push(format!("borders(L,T,B,R) {:?} -> {:?}", e.borders, g.borders));
        }
        if e.comment != g.comment {
            parts.push(format!("comment {:?} -> {:?}", e.comment, g.comment));
        }
        if e.validation != g.validation {
            parts.push(format!("validation {} -> {}", e.validation, g.validation));
        }
        if e.merge != g.merge {
            parts.push(format!("merge {:?} -> {:?}", e.merge, g.merge));
        }
        if !parts.is_empty() {
            out.push(format!("{addr}: {}", parts.join("; ")));
        }
    }
    out
}

const TYPES: [(&str, PasteType, &str); 10] = [
    ("all", PasteType::All, "すべて"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("formulas", PasteType::Formulas, "数式"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("values", PasteType::Values, "値"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("formats", PasteType::Formats, "書式"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("comments", PasteType::Comments, "コメント"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("validation", PasteType::Validation, "入力規則"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("noborders", PasteType::AllExceptBorders, "罫線なし"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("widths", PasteType::ColumnWidths, "列幅"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("formulas_nf", PasteType::FormulasAndNumberFormats, "数式と数値の書式"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("values_nf", PasteType::ValuesAndNumberFormats, "値と数値の書式"), // i18n-ok (Japanese text under test or a name table that matches labels)
];
const OPS: [(&str, PasteOp, &str); 5] = [
    ("none", PasteOp::None, "演算なし"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("add", PasteOp::Add, "加算"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("sub", PasteOp::Subtract, "減算"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("mul", PasteOp::Multiply, "乗算"), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("div", PasteOp::Divide, "除算"), // i18n-ok (Japanese text under test or a name table that matches labels)
];

pub(super) fn matrix() {
    let (expected, expected_widths) = truth();
    let mut s = super::new_sheet();
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    setup(&mut s);
    let (baseline, base_widths) = snapshot(&mut s);
    let (src_widths, dst_widths) = {
        let w = s.with(|e, _| {
            let sheet = sheet_of_slide(e.slide()).unwrap();
            [0usize, 1, 2, 4, 5, 6].map(|c| sheet.columns[c] as i64)
        });
        ([("20", w[0]), ("12", w[1]), ("15", w[2])], [("8", w[3]), ("9", w[4]), ("10", w[5])])
    };
    // The widths of cells that went through a file (another program's copy) are rounded by the file's unit.
    let label_of = |w: i64| src_widths.iter().chain(dst_widths.iter()).find(|(_, v)| (*v - w).abs() * 100 <= *v * 2).map_or(format!("?{w}"), |(l, _)| l.to_string());
    assert_eq!(base_widths, dst_widths.iter().map(|x| x.1).collect::<Vec<_>>());
    let mut rows = Vec::new();
    let mut failures = 0;
    for external in [false, true] {
        for (tkey, ptype, tname) in TYPES {
            for (okey, op, oname) in OPS {
                for skip in [false, true] {
                    for transpose in [false, true] {
                        let key = format!("{tkey}|{okey}|{}|{}", skip as u8, transpose as u8);
                        let name = format!("{} / {tname} / {oname}{}{}", if external { "他のアプリ" } else { "内部" }, if skip { " / 空白を無視" } else { "" }, if transpose { " / 行列を入れ替え" } else { "" }); // i18n-ok (Japanese text under test or a name table that matches labels)
                        s.click_cell("A1");
                        s.update(|e, _, _| e.set_selected_ranges(&[range(0, 0, 1, 2)]));
                        s.update(|e, _, cx| e.copy_cells(false, cx));
                        if external {
                            s.update(|e, _, _| {
                                if let Some(state) = e.sheet_state_mut() {
                                    state.clip = None;
                                }
                            });
                        }
                        s.click_cell("E5");
                        s.update(move |e, _, cx| e.apply_paste_special_options(PasteSpecialOptions { paste_type: ptype, operation: op, skip_blanks: skip, transpose }, cx));
                        let (got, widths) = snapshot(&mut s);
                        let mut diffs = differences(&expected[&key], &got);
                        let want_w: Vec<String> = expected_widths[&key].iter().map(|w| w.to_string()).collect();
                        let got_w: Vec<String> = widths.iter().map(|&w| label_of(w)).collect();
                        if want_w != got_w {
                            diffs.push(format!("column widths E,F,G {:?} -> {:?}", want_w, got_w));
                        }
                        s.key("cmd-z");
                        let (after_undo, w2) = snapshot(&mut s);
                        if after_undo != baseline || w2 != base_widths {
                            diffs.push("undo does not restore the sheet".to_string());
                        }
                        if !diffs.is_empty() {
                            failures += 1;
                        }
                        rows.push((name, key.clone(), diffs));
                    }
                }
            }
        }
    }
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    if let Ok(path) = std::env::var("IBIS_PASTE_MATRIX_OUT") {
        let mut md = String::from("| 組み合わせ | 期待（Excel の実測との差） | 結果 | 合否 |\n|---|---|---|---|\n"); // i18n-ok (Japanese text under test or a name table that matches labels)
        for (name, key, diffs) in &rows {
            if diffs.is_empty() {
                md.push_str(&format!("| {name} | Excel の実測 `{key}`（16 セル・列幅）| 全項目一致 | 合格 |\n")); // i18n-ok (Japanese text under test or a name table that matches labels)
            } else {
                md.push_str(&format!("| {name} | Excel の実測 `{key}`（期待 -> ibisOffice で違う所） | {} | 不合格 |\n", diffs.join("<br>"))); // i18n-ok (Japanese text under test or a name table that matches labels)
            }
        }
        std::fs::write(path, md).unwrap();
    }
    let summary: Vec<String> = rows.iter().filter(|(_, _, d)| !d.is_empty()).map(|(n, _, d)| format!("{n}\n    {}", d.join("\n    "))).collect();
    assert!(failures == 0, "{failures} of {} combinations differ from Excel:\n{}", rows.len(), summary.join("\n"));
    let _ = describe;
}
