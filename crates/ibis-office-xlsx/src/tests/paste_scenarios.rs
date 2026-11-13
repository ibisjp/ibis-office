// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pasting as Excel does where the destination is not one plain cell: an ordinary paste (Ctrl+V) of copied merges, comments and
//! validation, a destination with merged ranges (dissolved, kept or refused), a destination bigger than the copy (tiled when it is
//! a whole number of copies, else one copy), and a destination of several areas. Against Excel's own results
//! (`test/editing/xlsx-paste-scenarios-excel.tsv`: Excel 365 for Windows through COM, `scripts/corpus/paste_scenarios_truth_win.ps1`),
//! for cells copied here and for cells that only the clipboard holds. `IBIS_PASTE_SCENARIOS_OUT=<file>` writes the table (Markdown).

use super::SheetSteps;
use super::paste_special_matrix::{differences, range, setup_with, snapshot, truth_of};
use crate::clipboard::{PasteOp, PasteSpecialOptions, PasteType};
use ibis_doc_model::CellRange;
use crate::ext::*;

/// key, copied cells (`full` A1:C2, `cell` A1, `col` A1:A2), destination (areas), merged range of the destination.
const SCENARIOS: &[(&str, &str, &[(u32, u32, u32, u32)], &str, &str)] = &[
    ("basic", "full", &[(4, 4, 4, 4)], "none", "E5 (the plain case)"),
    ("m_in", "full", &[(4, 4, 4, 4)], "m_in", "E5; the destination has E5:F5 merged, inside the copy"),
    ("m_straddle", "full", &[(4, 4, 4, 4)], "m_straddle", "E5; the destination has F6:G7 merged, partly under the copy"),
    ("m_same", "full", &[(4, 4, 4, 4)], "m_same", "E5; the destination has G5:G6 merged, as the copied C1:C2 lands"),
    ("m_outside", "full", &[(4, 4, 4, 4)], "m_outside", "E5; the destination has F7:G7 merged, outside the copy"),
    ("m_all", "full", &[(4, 4, 4, 4)], "m_all", "E5; the destination has E5:G6 merged, all of the copy"),
    ("m_in_cell", "cell", &[(4, 4, 4, 6)], "m_in", "A1 copied to E5:G5; E5:F5 merged"),
    ("m_straddle_cell", "cell", &[(4, 4, 6, 4)], "m_straddle", "A1 copied to E5:E7; F6:G7 merged"),
    ("rep_rows", "full", &[(4, 4, 9, 6)], "none", "E5:G10: 3 copies down"),
    ("rep_cols", "full", &[(4, 4, 5, 9)], "none", "E5:J6: 2 copies across"),
    ("rep_both", "full", &[(4, 4, 9, 9)], "none", "E5:J10: 3 x 2 copies"),
    ("rep_odd", "full", &[(4, 4, 6, 7)], "none", "E5:H7: not a whole number of copies"),
    ("rep_odd_rows", "full", &[(4, 4, 8, 6)], "none", "E5:G9: 2.5 copies down"),
    ("rep_cell", "cell", &[(4, 4, 6, 5)], "none", "A1 to E5:F7: filled"),
    ("rep_col", "col", &[(4, 4, 9, 5)], "none", "A1:A2 to E5:F10: 3 x 2 copies"),
    ("rep_col_odd", "col", &[(4, 4, 8, 5)], "none", "A1:A2 to E5:F9: not a whole number"),
    ("multi_cells", "full", &[(4, 4, 4, 4), (8, 4, 8, 4)], "none", "E5 and E9 (single cells)"),
    ("multi_same", "full", &[(4, 4, 5, 6), (8, 4, 9, 6)], "none", "E5:G6 and E9:G10 (the copy's size)"),
    ("multi_diff", "full", &[(4, 4, 5, 6), (8, 4, 9, 7)], "none", "E5:G6 and E9:H10 (one is not the copy's size)"),
    ("multi_cell_src", "cell", &[(4, 4, 4, 4), (8, 6, 8, 6)], "none", "A1 to E5 and G9"),
    ("multi_side", "full", &[(4, 4, 5, 6), (4, 9, 5, 11)], "none", "E5:G6 and J5:L6"),
    ("multi_col", "col", &[(4, 4, 5, 4), (8, 6, 9, 6)], "none", "A1:A2 to E5:E6 and G9:G10"),
    ("multi_cell_ranges", "cell", &[(4, 4, 4, 5), (8, 4, 8, 6)], "none", "A1 to E5:F5 and E9:G9: filled"),
    ("multi_tiles", "col", &[(4, 4, 7, 4), (4, 6, 5, 6)], "none", "A1:A2 to E5:E8 (2 copies) and G5:G6"),
];
/// Where Excel does something that is not a rule worth copying (kept as the result of the measure, not matched).
const KNOWN: &[(&str, &str)] = &[];
const METHODS: &[(&str, &str)] = &[("paste", "貼り付け（Ctrl+V）"), ("special", "形式を選択して貼り付け: すべて"), ("values", "形式を選択して貼り付け: 値"), ("formats", "形式を選択して貼り付け: 書式"), ("formulas", "形式を選択して貼り付け: 数式")];

pub(super) fn scenarios() {
    let (expected, _, refused) = truth_of("test/editing/xlsx-paste-scenarios-excel.tsv");
    let mut s = super::new_sheet();
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    let mut rows = Vec::new();
    let mut failures = 0;
    for external in [false, true] {
        for &(name, source, dest, preset, what) in SCENARIOS {
            let copied = match source {
                "cell" => range(0, 0, 0, 0),
                "col" => range(0, 0, 1, 0),
                _ => range(0, 0, 1, 2),
            };
            for &(method, method_name) in METHODS {
                let key = format!("{name}|{method}");
                let Some(want) = expected.get(&key) else { continue };
                s.menu("xlsx.insert-sheet");
                setup_with(&mut s, preset);
                let (baseline, _) = snapshot(&mut s);
                s.click_cell("A1");
                s.update(move |e, _, _| e.set_selected_ranges(&[copied]));
                s.update(|e, _, cx| e.copy_cells(false, cx));
                if external {
                    s.update(|e, _, _| {
                        if let Some(state) = e.sheet_state_mut() {
                            state.clip = None;
                        }
                    });
                }
                let areas: Vec<CellRange> = dest.iter().map(|&(r0, c0, r1, c1)| range(r0, c0, r1, c1)).collect();
                s.click_cell("E5");
                let areas2 = areas.clone();
                s.update(move |e, _, _| e.set_selected_ranges(&areas2));
                let ptype = match method {
                    "values" => Some(PasteType::Values),
                    "formats" => Some(PasteType::Formats),
                    "formulas" => Some(PasteType::Formulas),
                    "special" => Some(PasteType::All),
                    _ => None,
                };
                s.update(move |e, _, cx| match ptype {
                    Some(paste_type) => e.apply_paste_special_options(PasteSpecialOptions { paste_type, operation: PasteOp::None, skip_blanks: false, transpose: false }, cx),
                    None => e.paste_cells(false, cx),
                });
                let (got, _) = snapshot(&mut s);
                let mut diffs = differences(want, &got);
                if refused.contains(&key) && got != baseline && diffs.is_empty() {
                    diffs.push("Excel refuses, the sheet changed".to_string());
                }
                s.key("cmd-z");
                let (after_undo, _) = snapshot(&mut s);
                if got != baseline && after_undo != baseline {
                    diffs.push("undo does not restore the sheet".to_string());
                }
                let known = KNOWN.iter().find(|(k, _)| *k == key).map(|(_, why)| *why);
                if let (Some(why), false) = (known, diffs.is_empty()) {
                    rows.push((format!("{} / {name} ({what}) / {method_name}", if external { "他のアプリ" } else { "内部" }), "Excel の実測（下の理由で合わせない）".to_string(), key.clone(), vec![format!("既知の差: {why}")]));
                    continue;
                }
                if !diffs.is_empty() {
                    failures += 1;
                }
                let excel = if refused.contains(&key) { "Excel が断る（シートは変わらない）" } else { "Excel の実測と同じ形" };
                rows.push((format!("{} / {name} ({what}) / {method_name}", if external { "他のアプリ" } else { "内部" }), excel.to_string(), key.clone(), diffs));
            }
        }
    }
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    if let Ok(path) = std::env::var("IBIS_PASTE_SCENARIOS_OUT") {
        let mut md = String::from("| 組み合わせ | 期待（Excel の実測） | 結果 | 合否 |\n|---|---|---|---|\n");
        for (name, excel, key, diffs) in &rows {
            if diffs.first().is_some_and(|d| d.starts_with("既知の差")) {
                md.push_str(&format!("| {name} | {excel}（`{key}`） | {} | 既知の差（直さない） |\n", diffs.join("<br>")));
            } else if diffs.is_empty() {
                md.push_str(&format!("| {name} | {excel}（`{key}`、E3:L12 の 80 セル） | 全項目一致 | 合格 |\n"));
            } else {
                md.push_str(&format!("| {name} | {excel}（`{key}`） | {} | 不合格 |\n", diffs.join("<br>")));
            }
        }
        std::fs::write(path, md).unwrap();
    }
    let summary: Vec<String> = rows.iter().filter(|(_, _, _, d)| !d.is_empty()).map(|(n, _, _, d)| format!("{n}\n    {}", d.join("\n    "))).collect();
    assert!(failures == 0, "{failures} of {} cases differ from Excel:\n{}", rows.len(), summary.join("\n"));
}
