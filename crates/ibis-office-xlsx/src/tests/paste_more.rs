// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the other paste tests leave out, against Excel's own results (`test/editing/xlsx-paste-more-excel.tsv`: Excel 365 for Windows
//! through COM, `scripts/corpus/paste_more_truth_win.ps1`; the key of each case says what was done): the merge command (the comments
//! of covered cells and the borders inside the range are dropped), a merge made by a paste, cut and paste (the source is emptied,
//! formulas that point at it follow), an operation onto a merged destination, a transposed paste with every kind of destination, and the
//! column widths of a destination bigger than the copy. Copies are made here and again from cells that only the clipboard holds.
//! `IBIS_PASTE_MORE_OUT=<file>` writes the table (Markdown); `IBIS_PASTE_MORE_FILTER=<text>` runs only the keys that contain it.

use super::SheetSteps;
use super::paste_special_matrix::{differences, range, setup_deco, snapshot_all, truth_of};
use crate::clipboard::{PasteOp, PasteSpecialOptions, PasteType};
use ibis_doc_model::CellRange;
use crate::ext::*;

fn cell_of(name: &str) -> (u32, u32) {
    let letters: String = name.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let row: u32 = name[letters.len()..].parse().unwrap();
    let column = letters.bytes().fold(0u32, |n, b| n * 26 + (b.to_ascii_uppercase() - b'A' + 1) as u32);
    (row - 1, column - 1)
}

fn area_of(text: &str) -> CellRange {
    let (a, b) = text.split_once(':').unwrap_or((text, text));
    let ((r0, c0), (r1, c1)) = (cell_of(a), cell_of(b));
    range(r0, c0, r1, c1)
}

fn source_of(name: &str) -> CellRange {
    match name {
        "cell" => range(0, 0, 0, 0),
        "col" => range(0, 0, 1, 0),
        "merge" => range(0, 2, 1, 2),
        "block" => range(4, 4, 5, 6),
        "row" => range(0, 0, 0, 2),
        _ => range(0, 0, 1, 2),
    }
}

fn type_of(method: &str) -> PasteType {
    match method {
        "formulas" => PasteType::Formulas,
        "values" => PasteType::Values,
        "formats" => PasteType::Formats,
        "comments" => PasteType::Comments,
        "validation" => PasteType::Validation,
        "noborders" => PasteType::AllExceptBorders,
        "widths" => PasteType::ColumnWidths,
        "formulas_nf" => PasteType::FormulasAndNumberFormats,
        "values_nf" => PasteType::ValuesAndNumberFormats,
        _ => PasteType::All,
    }
}

fn op_of(op: &str) -> PasteOp {
    match op {
        "add" => PasteOp::Add,
        "sub" => PasteOp::Subtract,
        "mul" => PasteOp::Multiply,
        "div" => PasteOp::Divide,
        _ => PasteOp::None,
    }
}

/// The title of a group of keys, for the table.
fn group_of(key: &str) -> &'static str {
    let f: Vec<&str> = key.split('|').collect();
    match (f[0], f[5], f[7], f[6], f[4]) {
        ("merge", ..) => "結合コマンド", // i18n-ok (Japanese text under test or a name table that matches labels)
        ("cut", _, "1", ..) => "切り取り（行列を入れ替え）", // i18n-ok (Japanese text under test or a name table that matches labels)
        ("cut", ..) => "切り取り", // i18n-ok (Japanese text under test or a name table that matches labels)
        (_, "widths", "1", ..) => "列幅の繰り返し", // i18n-ok (Japanese text under test or a name table that matches labels)
        (_, _, "1", ..) => "行列を入れ替え", // i18n-ok (Japanese text under test or a name table that matches labels)
        (_, _, _, op, _) if op != "none" => "結合のある貼り先への演算", // i18n-ok (Japanese text under test or a name table that matches labels)
        (_, _, _, _, "dec") => "結合を作る貼り付け", // i18n-ok (Japanese text under test or a name table that matches labels)
        _ => "列幅の繰り返し", // i18n-ok (Japanese text under test or a name table that matches labels)
    }
}

fn widths_of(emu: &[i64]) -> Vec<i64> {
    emu.iter().map(|&w| (w as f64 / ibis_doc_model::EMU_PER_POINT as f64 / 6.4).round() as i64).collect()
}

pub(super) fn more() {
    let (expected, expected_widths, refused) = truth_of("test/editing/xlsx-paste-more-excel.tsv");
    let filter = std::env::var("IBIS_PASTE_MORE_FILTER").unwrap_or_default();
    // IBIS_PASTE_MORE_PART=<i>/<n>: run the i-th of n slices of the keys (a run of all of them in one process takes too much memory).
    let part: Option<(usize, usize)> = std::env::var("IBIS_PASTE_MORE_PART").ok().and_then(|p| p.split_once('/').map(|(i, n)| (i.parse().unwrap(), n.parse().unwrap())));
    let mut slot = Some(super::new_sheet());
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    let mut rows: Vec<(String, String, Vec<String>, &'static str)> = Vec::new();
    let mut failures = 0;
    for external in [false, true] {
        for (index, (key, want)) in expected.iter().enumerate() {
            if !key.contains(&filter) || part.is_some_and(|(i, n)| index % n != i) {
                continue;
            }
            let f: Vec<&str> = key.split('|').collect();
            let (kind, src, dst, preset, deco, method, op, transpose) = (f[0], f[1], f[2], f[3], f[4], f[5], f[6], f[7] == "1");
            if external && kind != "copy" {
                continue;
            }
            // Excel's own result of a transposed paste into several areas that are a column's height (a one-column copy: A1:A2): it
            // puts the first cell in every cell of the areas; nothing explains it, so it is not copied.
            let known = src == "col" && dst.contains('+') && transpose && method != "widths";
            // A fresh workbook every so often: each case adds a sheet, and a thousand of them in one workbook take gigabytes.
            if rows.len() % 40 == 39 {
                drop(slot.take());
                slot = Some(super::new_sheet());
            }
            let s = slot.as_mut().unwrap();
            s.menu("xlsx.insert-sheet");
            setup_deco(s, preset, deco);
            let (baseline, base_widths) = snapshot_all(s);
            let areas: Vec<CellRange> = dst.split('+').map(area_of).collect();
            s.click_cell("A1");
            if kind == "merge" {
                let first = areas[0];
                s.update(move |e, _, _| e.set_selected_ranges(&[first]));
                if method == "across" {
                    s.update(|e, _, cx| e.merge_across(cx));
                } else {
                    s.update(|e, _, cx| e.merge_cells(false, cx));
                }
            } else {
                let copied = source_of(src);
                s.update(move |e, _, _| e.set_selected_ranges(&[copied]));
                let cut = kind == "cut";
                s.update(move |e, _, cx| e.copy_cells(cut, cx));
                if external {
                    s.update(|e, _, _| {
                        if let Some(state) = e.sheet_state_mut() {
                            state.clip = None;
                        }
                    });
                }
                s.click_cell("E5");
                let areas2 = areas.clone();
                s.update(move |e, _, _| e.set_selected_ranges(&areas2));
                let options = PasteSpecialOptions { paste_type: type_of(method), operation: op_of(op), skip_blanks: false, transpose };
                let plain = method == "paste";
                s.update(move |e, _, cx| {
                    if plain {
                        e.paste_cells(false, cx)
                    } else {
                        e.apply_paste_special_options(options, cx)
                    }
                });
            }
            let (got, widths) = snapshot_all(s);
            let mut diffs = differences(want, &got);
            let (want_w, got_w) = (&expected_widths[key], widths_of(&widths));
            // Widths that went through a file (another program's copy) are rounded by the file's unit.
            let width_diff: Vec<String> = (0..want_w.len()).filter(|&i| (got_w[i] - want_w[i]).abs() as f64 > if external { 0.02 * want_w[i] as f64 + 0.5 } else { 0.0 }).map(|i| format!("column {} width {} -> {}", (b'A' + i as u8) as char, want_w[i], got_w[i])).collect();
            diffs.extend(width_diff);
            if refused.contains(key) && got != baseline && diffs.is_empty() {
                diffs.push("Excel refuses, the sheet changed".to_string());
            }
            s.key("cmd-z");
            let (after_undo, w2) = snapshot_all(s);
            if (got != baseline || widths != base_widths) && (after_undo != baseline || w2 != base_widths) {
                diffs.push("undo does not restore the sheet".to_string());
            }
            if known && !diffs.is_empty() {
                rows.push((format!("{} / {key}", if external { "他のアプリ" } else { "内部" }), "Excel の実測（理由が分からないので合わせない）".to_string(), vec![format!("既知の差: {}", diffs.join("; "))], group_of(key))); // i18n-ok (Japanese text under test or a name table that matches labels)
                continue;
            }
            if !diffs.is_empty() {
                failures += 1;
            }
            rows.push((format!("{} / {key}", if external { "他のアプリ" } else { "内部" }), if refused.contains(key) { "Excel が断る（シートは変わらない）" } else { "Excel の実測と同じ形" }.to_string(), diffs, group_of(key))); // i18n-ok (Japanese text under test or a name table that matches labels)
        }
    }
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    if let Ok(path) = std::env::var("IBIS_PASTE_MORE_OUT") {
        let mut md = String::from("| 組み合わせ | 期待（Excel の実測） | 結果 | 合否 |\n|---|---|---|---|\n"); // i18n-ok (Japanese text under test or a name table that matches labels)
        for (name, excel, diffs, _) in &rows {
            if diffs.first().is_some_and(|d| d.starts_with("既知の差")) { // i18n-ok (Japanese text under test or a name table that matches labels)
                md.push_str(&format!("| {name} | {excel} | {} | 既知の差（直さない） |\n", diffs.join("<br>"))); // i18n-ok (Japanese text under test or a name table that matches labels)
            } else if diffs.is_empty() {
                md.push_str(&format!("| {name} | {excel} | 全項目一致 | 合格 |\n")); // i18n-ok (Japanese text under test or a name table that matches labels)
            } else {
                md.push_str(&format!("| {name} | {excel} | {} | 不合格 |\n", diffs.join("<br>"))); // i18n-ok (Japanese text under test or a name table that matches labels)
            }
        }
        std::fs::write(path, md).unwrap();
    }
    let summary: Vec<String> = rows.iter().filter(|(_, _, d, _)| !d.is_empty() && !d.first().is_some_and(|x| x.starts_with("既知の差"))).map(|(n, _, d, _)| format!("{n}\n    {}", d.join("\n    "))).collect(); // i18n-ok (Japanese text under test or a name table that matches labels)
    assert!(failures == 0, "{failures} of {} cases differ from Excel:\n{}", rows.len(), summary.join("\n"));
}
