// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pivot table's +/− buttons (展開・折りたたみ): the rows and columns of a folded item, its cells against what Excel
//! 16.0 (build 20527) showed for the same states (`test/editing/pivot-expand-excel.txt`, measured through COM on the
//! files in `test/editing/pivot-expand-*.xlsx`), a click on the button, Undo, the save and the field commands.

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::ext::*;

const EXCEL: &str = "test/editing/pivot-expand-excel.txt";

/// Excel's cells of one state: the section named `label` of the measurement, as rows of (text, indent).
fn excel(label: &str) -> Vec<Vec<(String, u32)>> {
    let text = std::fs::read_to_string(crate::shell::scenario::repo_root().join(EXCEL)).expect("Excel's measurement");
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("### ") else { continue };
        let mut parts = rest.split(" | ");
        if parts.next() != Some(label) {
            continue;
        }
        let rows: usize = parts.nth(1).and_then(|r| r.strip_prefix("rows ")).and_then(|r| r.split(' ').next()).and_then(|r| r.parse().ok()).expect("the row count");
        return lines
            .take(rows)
            .map(|row| {
                row.split('\t')
                    .map(|cell| match cell.strip_prefix('~').and_then(|c| c.split_once(':')) {
                        Some((n, t)) => (t.to_string(), n.parse().unwrap()),
                        None => (cell.to_string(), 0),
                    })
                    .collect()
            })
            .collect();
    }
    panic!("no state {label:?} in the measurement");
}

fn parse(reference: &str) -> (u32, u32) {
    let split = reference.find(|c: char| c.is_ascii_digit()).unwrap();
    (ibis_calc::formula::parser::column_index(&reference[..split]).unwrap(), reference[split..].parse::<u32>().unwrap() - 1)
}

fn pivot(s: &mut Scenario) -> ibis_doc_model::PivotTable {
    s.with(|e, _| e.current_sheet().and_then(|(_, sh)| sh.pivots.first().cloned())).expect("the pivot table")
}

/// The pivot's cells against Excel's state `label`: every cell's text, and a row label's indent.
fn same_as_excel(s: &mut Scenario, label: &str) {
    let want = excel(label);
    let loc = pivot(s).location;
    let (rows, columns) = (loc.last_row - loc.first_row + 1, loc.last_column - loc.first_column + 1);
    assert_eq!((rows as usize, columns as usize), (want.len(), want[0].len()), "{label}: the table's size");
    let got: Vec<Vec<(String, u32)>> = s.with(|e, _| {
        let (_, sheet) = e.current_sheet().unwrap();
        (loc.first_row..=loc.last_row)
            .map(|r| {
                (loc.first_column..=loc.last_column)
                    .map(|c| sheet.cell(r, c).map_or((String::new(), 0), |cell| (cell.text.clone(), sheet.styles[cell.style as usize].indent)))
                    .collect()
            })
            .collect()
    });
    for (r, (g, w)) in got.iter().zip(&want).enumerate() {
        let texts = |row: &[(String, u32)]| row.iter().map(|c| c.0.clone()).collect::<Vec<_>>();
        assert_eq!(texts(g), texts(w), "{label}: row {r} of the table");
        // A label's indent (compact form) is Excel's.
        if !w[0].0.is_empty() && w[0].1 > 0 {
            assert_eq!(g[0].1, w[0].1, "{label}: indent of row {r}");
        }
    }
}

/// The button of the label in `cell` as (shown as folded, centre in the window).
fn button(s: &mut Scenario, cell: &str) -> Option<(bool, (f32, f32))> {
    let (column, row) = parse(cell);
    s.with(move |e, _| {
        let (_, sheet) = e.current_sheet().unwrap();
        let g = e.sheet_state().unwrap().header_geometry.borrow().clone();
        sheet.pivots.iter().flat_map(|p| p.drill_buttons().into_iter().map(move |l| (p, l))).find(|(_, l)| l.row == row && l.column == column).and_then(|(p, l)| {
            let (x, y, side) = crate::pivot_drill::drill_button_rect(&g, &l)?;
            let folded = p.fields[l.field].items.get(l.item as usize).is_some_and(|i| i.collapsed);
            Some((folded, (x + side / 2.0, y + side / 2.0)))
        })
    })
}

/// A real click on the +/− button beside the label in `cell`.
fn press(s: &mut Scenario, cell: &str) {
    let (_, at) = button(s, cell).unwrap_or_else(|| panic!("no +/- button at {cell}"));
    s.click(at);
    s.settle();
}

fn open(file: &str) -> Scenario {
    let mut s = Scenario::open(&format!("test/editing/pivot-expand-{file}.xlsx"));
    s.size(1400., 800.);
    s.settle();
    s
}

fn refresh(s: &mut Scenario) {
    s.update(|e, _, cx| e.refresh_pivots(true, cx));
    s.settle();
}

fn folded(s: &mut Scenario, field: &str, item: &str) -> bool {
    let p = pivot(s);
    let f = p.fields.iter().find(|f| f.name == field).unwrap();
    f.items.iter().any(|i| i.collapsed && i.x.and_then(|x| f.shared.get(x as usize)).is_some_and(|v| *v == ibis_doc_model::CellValue::Text(item.to_string())))
}

/// Rows: the buttons beside 2025年 / 2026年 (not beside the regions), the file's folded state, the values built again
/// as Excel's, a click folding and unfolding, Undo, the field commands, and the state kept by a save.
pub(super) fn rows_click_undo_save() {
    let mut s = open("rows-2026");
    same_as_excel(&mut s, "S1 collapse 2026");
    // The file's own cells are what Excel wrote; built again they are the same.
    refresh(&mut s);
    same_as_excel(&mut s, "S1 collapse 2026");
    // A+ beside 2026年 (folded), a − beside 2025年 (open), none beside the regions or 総計.
    let (r2025, r2026) = ("A5", "A8");
    assert_eq!(button(&mut s, r2025).map(|b| b.0), Some(false), "2025年 is open (−)"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    assert_eq!(button(&mut s, r2026).map(|b| b.0), Some(true), "2026年 is folded (+)"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    assert!(button(&mut s, "A6").is_none() && button(&mut s, "A7").is_none() && button(&mut s, "A9").is_none(), "the innermost field has no button");
    s.screenshot("pivot-expand-rows-folded-2026");

    press(&mut s, r2026);
    same_as_excel(&mut s, "S1 compact base");
    assert!(!folded(&mut s, "年", "2026年")); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    s.screenshot("pivot-expand-rows-all-open");
    // Fold 2026年 again, then 2025年: each is Excel's state.
    press(&mut s, "A8");
    same_as_excel(&mut s, "S1 collapse 2026");
    press(&mut s, "A5");
    same_as_excel(&mut s, "S1 collapse both");
    s.screenshot("pivot-expand-rows-all-folded");
    // One step each to undo.
    s.key("cmd-z");
    s.settle();
    same_as_excel(&mut s, "S1 collapse 2026");
    s.key("cmd-z");
    s.settle();
    same_as_excel(&mut s, "S1 compact base");
    s.key("cmd-shift-z");
    s.settle();
    same_as_excel(&mut s, "S1 collapse 2026");

    // フィールドの折りたたみ / 展開 from the selected label.
    s.click_cell("A5");
    s.update(|e, _, cx| e.set_pivot_selected_field_collapsed(true, cx));
    s.settle();
    same_as_excel(&mut s, "S1 field ShowDetail=false");
    s.update(|e, _, cx| e.set_pivot_selected_field_collapsed(false, cx));
    s.settle();
    same_as_excel(&mut s, "S1 field ShowDetail=true");

    // The state is kept by the save: 2026年 folded, 2025年 open.
    press(&mut s, "A8");
    assert!(folded(&mut s, "年", "2026年") && !folded(&mut s, "年", "2025年")); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    s.save_as("pivot-expand-rows.xlsx");
    let reopened = s.reopen();
    let p = reopened.slides.iter().find_map(|sl| sl.shapes.iter().find_map(|sh| if let ibis_doc_model::ShapeKind::Sheet(sh) = &sh.kind { sh.pivots.first().cloned() } else { None })).expect("the pivot in the saved file");
    let year = p.fields.iter().find(|f| f.name == "年").unwrap(); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    let state: Vec<bool> = year.items.iter().filter(|i| i.kind.is_none()).map(|i| i.collapsed).collect();
    assert_eq!(state, vec![false, true], "sd of 2025年 and 2026年 in the saved file"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    s.reload_saved();
    same_as_excel(&mut s, "S1 collapse 2026");
    assert_eq!(button(&mut s, "A8").map(|b| b.0), Some(true));

    // Hidden, the buttons are gone (showDrill); the folded rows stay folded.
    s.click_cell("A5");
    s.update(|e, _, cx| e.toggle_pivot_drill_buttons(cx));
    s.settle();
    assert!(button(&mut s, "A8").is_none(), "no buttons when ShowDrillIndicators is off");
    same_as_excel(&mut s, "S1 collapse 2026");
}

/// The folded state belongs to the item's name: it stays when a field is added, moved or taken out, and a new item is open.
pub(super) fn state_kept_when_fields_change() {
    let mut s = open("rows-2026");
    refresh(&mut s);
    let place = |s: &mut Scenario, name: &str, at: usize| {
        let name = name.to_string();
        s.update(move |e, _, cx| {
            let slide = e.current_slide;
            e.change_pivot((slide, 0), move |p| {
                let f = p.fields.iter().position(|f| f.name == name).unwrap();
                p.place(f, Some(ibis_doc_model::PivotAxis::Row), at);
            }, cx)
        });
        s.settle();
    };
    place(&mut s, "担当", 2); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    same_as_excel(&mut s, "S5 after adding 担当 inner"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    place(&mut s, "年", 1); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    same_as_excel(&mut s, "S5 after moving 年 to pos 2 (地域 is now outer)"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    place(&mut s, "年", 0); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    same_as_excel(&mut s, "S5 after moving 年 back to pos 1"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    // Taken out of the table and put back: still folded.
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.change_pivot((slide, 0), |p| {
            let f = p.fields.iter().position(|f| f.name == "年").unwrap(); // i18n-ok（Excel の表示・試験文書の項目名との照合）
            p.place(f, None, 0);
        }, cx)
    });
    s.settle();
    place(&mut s, "年", 0); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    same_as_excel(&mut s, "S5 after remove and re-add 年"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    assert!(folded(&mut s, "年", "2026年")); // i18n-ok（Excel の表示・試験文書の項目名との照合）
}

/// Columns: the buttons over the column labels, folding a column group, both together with the rows.
pub(super) fn columns_and_both_axes() {
    let mut s = open("columns");
    same_as_excel(&mut s, "S4 collapse col item A");
    refresh(&mut s);
    same_as_excel(&mut s, "S4 collapse col item A");
    // The column labels are in row 4: 商品 over the first column of its group.
    let (a, b) = ("B4", "C4");
    assert_eq!(button(&mut s, a).map(|b| b.0), Some(true), "A is folded");
    assert_eq!(button(&mut s, b).map(|b| b.0), Some(false), "B is open");
    s.screenshot("pivot-expand-columns-folded-a");
    press(&mut s, a);
    same_as_excel(&mut s, "S4 cols 商品>チャネル base"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    // Fold A then B: Excel's.
    press(&mut s, "B4");
    same_as_excel(&mut s, "S4 collapse col item A");
    let b = s.with(|e, _| {
        let (_, sheet) = e.current_sheet().unwrap();
        sheet.pivots[0].drill_buttons().into_iter().filter(|l| l.on_columns).map(|l| (l.row, l.column)).collect::<Vec<_>>()
    });
    assert_eq!(b.len(), 2, "one button per column group: {b:?}");
    let second = ibis_doc_model::cell_name(b[1].1, b[1].0);
    press(&mut s, &second);
    same_as_excel(&mut s, "S4 collapse col A and B");
    s.screenshot("pivot-expand-columns-all-folded");
    s.key("cmd-z");
    s.settle();
    same_as_excel(&mut s, "S4 collapse col item A");

}

/// Rows and columns folded together. (One window per process: a scenario opens one book.)
pub(super) fn rows_and_columns_together() {
    let mut s = open("rows-and-columns");
    same_as_excel(&mut s, "S4b rows 2026 collapsed + col B collapsed");
    refresh(&mut s);
    same_as_excel(&mut s, "S4b rows 2026 collapsed + col B collapsed");
    s.screenshot("pivot-expand-rows-and-columns");
}

/// Three levels: a folded middle item, folding the outer one, and the middle one still folded after it is opened again.
pub(super) fn three_levels() {
    let mut s = open("three-levels");
    same_as_excel(&mut s, "S3 2025 expanded again (middle state kept?)");
    refresh(&mut s);
    same_as_excel(&mut s, "S3 2025 expanded again (middle state kept?)");
    assert!(folded(&mut s, "地域", "関東"), "the middle item is folded"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    press(&mut s, "A5");
    same_as_excel(&mut s, "S3 + 2025 collapsed");
    press(&mut s, "A5");
    same_as_excel(&mut s, "S3 2025 expanded again (middle state kept?)");
    assert!(folded(&mut s, "地域", "関東"), "still folded after the outer item opens again"); // i18n-ok（Excel の表示・試験文書の項目名との照合）
    s.screenshot("pivot-expand-three-levels");
}

/// The layouts: 表形式, 小計を下, 小計なし, アウトライン (3 levels), two values.
pub(super) fn layouts_and_values() {
    for (file, state) in [
        ("tabular", "S2 layout tabular collapse 2026 subtotal top"),
        ("compact-bottom", "S2 compact subtotal bottom collapsed 2026"),
        ("tabular-no-subtotal", "S2 tabular, 年 subtotal OFF, 2026 collapsed"), // i18n-ok（Excel の表示・試験文書の項目名との照合）
        ("outline-three", "S9 outline + 2025 collapsed"),
        ("two-values", "S6 2026 collapsed + col A collapsed (two data fields)"),
        ("field-folded", "S1 field ShowDetail=false"),
    ] {
        let mut s = open(file);
        same_as_excel(&mut s, state);
        refresh(&mut s);
        same_as_excel(&mut s, state);
        s.screenshot(&format!("pivot-expand-{file}"));
    }
}

/// The real workbook (`test/private`, not published): every outer label of the pivot sheet has a button, a click folds and
/// unfolds it, Undo restores it.
pub(super) fn private_book() {
    // The file and the pivot sheet's name come from the environment (`IBIS_PRIVATE_XLSX`, `IBIS_PRIVATE_XLSX_SHEET`): the real names are not published.
    let (Some(path), Some(sheet_name)) = (std::env::var("IBIS_PRIVATE_XLSX").ok().filter(|p| std::path::Path::new(p).exists()), std::env::var("IBIS_PRIVATE_XLSX_SHEET").ok()) else {
        return;
    };
    let mut s = Scenario::open(&path);
    s.size(1600., 900.);
    s.settle();
    let sheet = s.with(|e, _| e.presentation().slides.iter().position(|sl| sl.name.as_deref() == Some(sheet_name.as_str())));
    let sheet = sheet.expect("the pivot sheet");
    s.update(|e, window, cx| {
        let id = e.presentation().slides[sheet].id;
        e.select_slide(id, window, cx);
    });
    s.settle();
    let buttons = s.with(|e, _| e.current_sheet().unwrap().1.pivots[0].drill_buttons().into_iter().filter(|l| !l.on_columns).collect::<Vec<_>>());
    assert!(!buttons.is_empty(), "the outer row labels have +/- buttons");
    // Built again from the source, the table has the rows Excel saved (the row labels; the values of a date group field's "<" item are not compared).
    let cells = |s: &mut Scenario| -> Vec<(u32, u32, String)> {
        s.with(|e, _| {
            let (_, sh) = e.current_sheet().unwrap();
            let l = sh.pivots[0].location;
            sh.cells.iter().filter(|c| (l.first_row..=l.last_row).contains(&c.row) && (l.first_column..=l.last_column).contains(&c.column)).map(|c| (c.row, c.column, c.text.clone())).collect()
        })
    };
    let cached: std::collections::BTreeMap<(u32, u32), String> = cells(&mut s).into_iter().map(|(r, c, t)| ((r, c), t)).collect();
    refresh(&mut s);
    let rebuilt: std::collections::BTreeMap<(u32, u32), String> = cells(&mut s).into_iter().map(|(r, c, t)| ((r, c), t)).collect();
    let keys: std::collections::BTreeSet<_> = cached.keys().chain(rebuilt.keys()).copied().collect();
    let differing: Vec<_> = keys.iter().filter(|k| k.1 == 0).filter_map(|k| {
        let (a, b) = (cached.get(k).cloned().unwrap_or_default(), rebuilt.get(k).cloned().unwrap_or_default());
        (a != b).then(|| (*k, a, b))
    }).collect();
    assert!(differing.is_empty(), "the table built again is Excel's: {} cells differ, first {:?}", differing.len(), differing.iter().take(40).collect::<Vec<_>>());
    let rows_before = pivot(&mut s).location.last_row;
    s.screenshot("private-pivot-before");
    let first = buttons[0];
    let name = ibis_doc_model::cell_name(first.column, first.row);
    let was_folded = pivot(&mut s).fields[first.field].items[first.item as usize].collapsed;
    s.update(|e, _, cx| e.toggle_pivot_item((e.current_slide, 0), first.field, first.item, cx));
    s.settle();
    let rows_after = pivot(&mut s).location.last_row;
    s.screenshot("private-pivot-after-toggle");
    assert_ne!(rows_before, rows_after, "{name}: folding or unfolding changes the rows");
    assert_eq!(rows_after > rows_before, was_folded, "a folded item adds rows when it opens ({name}: {rows_before} -> {rows_after}, folded {was_folded})");
    s.key("cmd-z");
    s.settle();
    assert_eq!(pivot(&mut s).location.last_row, rows_before, "Undo");
}
