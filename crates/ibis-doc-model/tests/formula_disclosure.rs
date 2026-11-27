// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A hidden formula (数式を表示しない on a protected sheet) must not leave the sheet through any path (#275).
//!
//! The one place that decides is `Sheet::formula_is_hidden`; paths that hand a formula out take it from `Sheet::visible_formula`,
//! `Sheet::shown_formula`, `Sheet::copy_of` or `BookView::visible_formula`. This test is the alarm for a new path:
//! 1. a file of the xlsx screen crate or the MCP tools that reads `.formula` and is not on the list below fails: add it only after
//!    reading how it uses the formula (editing, shifting, saving and calculating need the raw formula; showing it, copying it out or
//!    searching it must go through the functions above);
//! 2. no line builds the text `"={formula}"` from a cell's own `.formula` outside the model.

use ibis_doc_model::CellValue;
use ibis_doc_model::sheet::{CellKind, CellStyle, Sheet, SheetCell, SheetProtection};
use std::path::{Path, PathBuf};

/// Files known to read `.formula` for something other than showing it: editing, shifting, filling, saving, calculating, finding the
/// cells that have one, or a name's or rule's formula (which belongs to the workbook, not to a cell). Paths from `crates/`.
const REVIEWED: &[&str] = &[
    "ibis-doc-service/src/tools_xlsx.rs",
    // Names' formulas (the workbook's, not a cell's).
    "ibis-doc-service/src/tools_xlsx_read.rs",
    // Copying goes through `Sheet::copy_of`; moving keeps the formula in the cell; replacing text skips cells that have a formula.
    "ibis-doc-service/src/tools_xlsx_struct.rs",
    "ibis-office-xlsx/src/autosum.rs",
    "ibis-office-xlsx/src/cell_edit.rs",
    "ibis-office-xlsx/src/chart_insert.rs",
    "ibis-office-xlsx/src/checkbox.rs",
    "ibis-office-xlsx/src/clipboard_html.rs",
    "ibis-office-xlsx/src/clipboard_native.rs",
    "ibis-office-xlsx/src/clipboard_xmlss.rs",
    "ibis-office-xlsx/src/clipboard.rs",
    "ibis-office-xlsx/src/compatibility_panel.rs",
    "ibis-office-xlsx/src/conditional_edit.rs",
    "ibis-office-xlsx/src/conditional.rs",
    "ibis-office-xlsx/src/context_menu.rs",
    "ibis-office-xlsx/src/dialog_hooks.rs",
    "ibis-office-xlsx/src/error_check.rs",
    "ibis-office-xlsx/src/fill.rs",
    "ibis-office-xlsx/src/filter.rs",
    "ibis-office-xlsx/src/find.rs",
    "ibis-office-xlsx/src/flash_fill.rs",
    "ibis-office-xlsx/src/formula_audit.rs",
    "ibis-office-xlsx/src/formula_layout.rs",
    "ibis-office-xlsx/src/goal_seek.rs",
    "ibis-office-xlsx/src/goto_special.rs",
    "ibis-office-xlsx/src/import_text.rs",
    "ibis-office-xlsx/src/links.rs",
    "ibis-office-xlsx/src/machine_readability.rs",
    "ibis-office-xlsx/src/menu_items.rs",
    "ibis-office-xlsx/src/names_list.rs",
    "ibis-office-xlsx/src/names_panel.rs",
    "ibis-office-xlsx/src/paste_names_dialog.rs",
    "ibis-office-xlsx/src/paste_names.rs",
    "ibis-office-xlsx/src/pivot_calc_field.rs",
    "ibis-office-xlsx/src/pivot_calc_item.rs",
    "ibis-office-xlsx/src/pivot.rs",
    "ibis-office-xlsx/src/print.rs",
    "ibis-office-xlsx/src/range_field.rs",
    "ibis-office-xlsx/src/series.rs",
    "ibis-office-xlsx/src/sheet_transfer.rs",
    "ibis-office-xlsx/src/sheet_ui.rs",
    "ibis-office-xlsx/src/sheet_view.rs",
    "ibis-office-xlsx/src/solver_panel.rs",
    "ibis-office-xlsx/src/spelling_panel.rs",
    "ibis-office-xlsx/src/table_design.rs",
    "ibis-office-xlsx/src/table_edit.rs",
    "ibis-office-xlsx/src/text_columns.rs",
    "ibis-office-xlsx/src/toolbar.rs",
    "ibis-office-xlsx/src/trace.rs",
    "ibis-office-xlsx/src/validation.rs",
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") && path.file_name().is_some_and(|n| n != "tests.rs") {
            out.push(path);
        }
    }
}

/// Whether the text has `.formula` as a field name (not `.formula_hidden`, `.formula_text`...).
fn reads_formula_field(code: &str) -> bool {
    code.match_indices(".formula").any(|(at, m)| !code[at + m.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_'))
}

#[test]
fn a_path_that_reads_cell_formulas_is_reviewed() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for dir in ["ibis-office-xlsx/src", "ibis-doc-service/src"] {
        rust_files(&crates.join(dir), &mut files);
    }
    let mut unreviewed = Vec::new();
    let mut leaks = Vec::new();
    for file in files {
        let name = file.strip_prefix(crates).unwrap().to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(&file).unwrap();
        // Test modules inside a source file come last; what follows `#[cfg(test)]` is not a path.
        let code = text.split("#[cfg(test)]").next().unwrap();
        if reads_formula_field(code) && !REVIEWED.contains(&name.as_str()) {
            unreviewed.push(name.clone());
        }
        for (n, line) in code.lines().enumerate() {
            let reads_cell_formula = line.contains(".formula.") || line.contains("&c.formula") || line.contains("&cell.formula");
            if line.contains("format!(\"={") && reads_cell_formula {
                leaks.push(format!("{name}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(unreviewed.is_empty(), "these files read `.formula` and are not reviewed for hidden formulas (see the header): {unreviewed:?}");
    assert!(leaks.is_empty(), "a hidden formula would leave here; use Sheet::shown_formula / visible_formula: {leaks:#?}");
}

#[test]
fn a_hidden_formula_goes_nowhere_a_person_reads() {
    let mut sheet = Sheet::default();
    sheet.styles.push(CellStyle::default());
    sheet.styles.push(CellStyle { formula_hidden: true, ..Default::default() });
    let cell = |row: u32, style: u32| SheetCell {
        runs: None,
        phonetic: None,
        row,
        column: 0,
        value: CellValue::Number(3.0),
        text: "3".into(),
        kind: CellKind::Number,
        formula: Some("SECRET(1)".into()),
        style,
    };
    // Style 0 shows its formula; the style that was pushed hides it. (Style 0 is not hidden: `formula_hidden` is false by default;
    // the pushed style is the last one.)
    let hiding = (sheet.styles.len() - 1) as u32;
    sheet.put_cell(0, 0, Some(cell(0, 0)));
    sheet.put_cell(1, 0, Some(cell(1, hiding)));
    // Without protection the format alone hides nothing.
    assert!(sheet.shown_formula(sheet.cell(1, 0).unwrap()).is_some());
    sheet.protection = Some(SheetProtection::default());
    for (row, hidden) in [(0, false), (1, true)] {
        let c = sheet.cell(row, 0).unwrap();
        assert_eq!(sheet.formula_is_hidden(c), hidden);
        assert_eq!(sheet.visible_formula(c).is_none(), hidden);
        assert_eq!(sheet.shown_formula(c).is_none(), hidden);
        assert_eq!(sheet.copy_of(c).formula.is_none(), hidden, "a copy carries the value only");
        assert_eq!(sheet.copy_of(c).value, CellValue::Number(3.0));
    }
}
