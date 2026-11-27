// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #159, stage A: the edit tools. Each makes one undo step, is read back, and is still there after saving and opening the file again (design 8).

use std::path::{Path, PathBuf};

use ibis_doc_service::{Limits, Service};
use serde_json::{Value, json};

fn sandbox(test: &str, files: &[&str]) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/doc-service-tests").join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in files {
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(f), dir.join(f)).unwrap();
    }
    // The pictures the insert tools read (PNG 120 x 60 px, JPEG 80 x 160 px).
    for f in ["picture-120x60.png", "picture-80x160.jpg"] {
        std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(f), dir.join(f)).unwrap();
    }
    dir.canonicalize().unwrap()
}

struct T {
    s: Service,
    dir: PathBuf,
}

impl T {
    fn new(test: &str, files: &[&str]) -> Self {
        let dir = sandbox(test, files);
        Self { s: Service::new(&[dir.clone()], false, Limits::default()), dir }
    }

    fn call(&mut self, tool: &str, args: Value) -> Value {
        self.s.call(tool, &args).unwrap_or_else(|e| panic!("{tool}: {e}"))
    }

    fn err(&mut self, tool: &str, args: Value) -> String {
        self.s.call(tool, &args).unwrap_err().code.to_string()
    }

    fn open(&mut self, name: &str) -> String {
        self.call("app_open_document", json!({ "path": self.dir.join(name).to_string_lossy() }))["document_id"].as_str().unwrap().to_string()
    }

    /// Save as `copy_<name>` and open that.
    fn reopen(&mut self, id: &str, name: &str) -> String {
        let copy = format!("copy_{name}");
        self.call("app_save_document", json!({ "document_id": id, "path": self.dir.join(&copy).to_string_lossy(), "overwrite": true }));
        self.open(&copy)
    }
}

#[test]
fn mcp_docx_edits_are_one_undo_step_each_and_survive_saving() {
    let mut t = T::new("docx_edits", &["autospace-letters.docx"]);
    let d = t.open("autospace-letters.docx");
    let first = t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"].as_str().unwrap().to_string();
    assert!(first.chars().count() >= 2, "{first:?}");
    // Insert.
    let r = t.call("docx_insert_text", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "text": "ABC" }));
    assert_eq!(r["dirty"], true);
    assert_eq!(t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"], format!("ABC{first}"));
    // One undo takes it back.
    assert_eq!(t.call("app_undo_document", json!({ "document_id": d }))["undone"], 1);
    assert_eq!(t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"], first);
    t.call("app_redo_document", json!({ "document_id": d }));
    // Delete what was inserted.
    t.call("docx_delete_text", json!({ "document_id": d, "range": { "from": { "paragraph": 0, "offset": 0 }, "to": { "paragraph": 0, "offset": 3 } } }));
    assert_eq!(t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"], first);
    // Replace: every place, one undo step.
    t.call("docx_insert_text", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "text": "QQ and QQ and " }));
    let r = t.call("docx_replace_text", json!({ "document_id": d, "find": "QQ", "replace": "ZZZ" }));
    assert_eq!(r["replaced"], 2);
    assert!(t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"].as_str().unwrap().starts_with("ZZZ and ZZZ and "));
    t.call("app_undo_document", json!({ "document_id": d }));
    assert!(t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"].as_str().unwrap().starts_with("QQ and QQ and "));
    t.call("app_redo_document", json!({ "document_id": d }));
    // Format a range: bold, 14.5 pt, a colour, a typeface.
    t.call("docx_set_run_format", json!({ "document_id": d, "range": { "from": { "paragraph": 0, "offset": 0 }, "to": { "paragraph": 0, "offset": 3 } }, "bold": true, "size": 14.5, "color": "#FF8000", "font": { "latin": "Arial", "east_asian": "BIZ UDゴシック" } }));
    let runs = t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["runs"].clone();
    let run = &runs[0];
    assert_eq!((run["bold"].clone(), run["size"].clone(), run["color"].clone()), (json!(true), json!(14.5), json!("#FF8000")), "{runs}");
    assert_eq!(run["font"], "Arial");
    assert_eq!(run["east_asian_font"], "BIZ UDゴシック");
    // A wrong argument is refused and changes nothing.
    assert_eq!(t.err("docx_set_run_format", json!({ "document_id": d, "range": { "from": { "paragraph": 0, "offset": 0 }, "to": { "paragraph": 0, "offset": 1 } }, "color": "red" })), "invalid_argument");
    assert_eq!(t.err("docx_set_run_format", json!({ "document_id": d, "range": { "from": { "paragraph": 0, "offset": 0 }, "to": { "paragraph": 0, "offset": 1 } }, "colour": "#000000" })), "invalid_argument");
    assert_eq!(t.err("docx_insert_text", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 99999 }, "text": "x" })), "out_of_range");
    assert_eq!(t.err("docx_insert_text", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "text": "a\nb" })), "invalid_argument");
    // A table with text in its cells: one undo step for all of it.
    let tables_before = t.call("docx_list_tables", json!({ "document_id": d }))["tables"].as_array().unwrap().len();
    t.call("docx_insert_table", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "rows": 2, "columns": 2, "cells": [["a", "b"], ["c", "d"]] }));
    assert_eq!(t.call("docx_list_tables", json!({ "document_id": d }))["tables"].as_array().unwrap().len(), tables_before + 1);
    // Saved, opened again: the edits are there.
    let again = t.reopen(&d, "autospace-letters.docx");
    let run = t.call("docx_get_paragraph", json!({ "document_id": again, "paragraph": 0 }));
    let text = t.call("docx_get_text", json!({ "document_id": again, "format": "plain" }))["text"].as_str().unwrap().to_string();
    assert!(text.contains("ZZZ and ZZZ and "), "{run}");
    assert!(text.contains("a\tb") && text.contains("c\td"), "the table's cells: {text:?}");
    let first_run = &run["runs"][0];
    assert_eq!(first_run["bold"], true);
    assert_eq!(first_run["color"], "#FF8000");
    t.call("app_undo_document", json!({ "document_id": d }));
    assert_eq!(t.call("docx_list_tables", json!({ "document_id": d }))["tables"].as_array().unwrap().len(), tables_before, "the table and its cells are one undo step");
}

#[test]
fn mcp_xlsx_formula_reads_respect_sheet_protection() {
    use ibis_doc_model::sheet::{CellRange, SheetProtection};
    use ibis_doc_model::sheet_command::{CellEdit, CellInput, SheetCommand, StyleChange};
    use ibis_doc_model::{Command, Document};
    use ibis_formats::{Kind, SaveOptions};

    let bytes = ibis_formats::blank_file(Kind::Workbook).unwrap();
    let opened = ibis_formats::open(bytes).unwrap();
    let mut document = Document::new(opened.presentation);
    let sheet_id = document.presentation().slides[0].id;
    document.execute(vec![
        Command::Sheet(SheetCommand::SetCells { sheet: sheet_id, edits: vec![
            CellEdit { row: 0, column: 0, input: CellInput::Formula("1+2".into()) },
            CellEdit { row: 0, column: 1, input: CellInput::Formula("2+3".into()) },
        ] }),
        Command::Sheet(SheetCommand::SetFormat { sheet: sheet_id, ranges: vec![
            CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 },
        ], change: StyleChange::FormulaHidden(true) }),
        Command::Sheet(SheetCommand::SetProtection { sheet: sheet_id, protection: Some(SheetProtection::default()) }),
    ]).unwrap();

    let mut t = T::new("xlsx_hidden_formulas", &[]);
    let bytes = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    std::fs::write(t.dir.join("protected.xlsx"), bytes).unwrap();
    let x = t.open("protected.xlsx");
    let sheet = t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"][0]["name"].as_str().unwrap().to_string();
    let formulas = t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:B1", "kind": "formula" }));
    assert_eq!(formulas["rows"], json!([[3.0, "=2+3"]]));
    // Nothing else of the MCP tools gives the formula: the text of the cell is what is searched and shown.
    assert_eq!(t.call("xlsx_find_text", json!({ "document_id": x, "query": "1+2" }))["matches"], json!([]));
    for kind in ["value", "text", "format"] {
        let out = t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:B1", "kind": kind })).to_string();
        assert!(!out.contains("1+2"), "{kind}: {out}");
    }
}

#[test]
fn mcp_xlsx_cell_inputs_use_shared_rules() {
    let mut t = T::new("xlsx_cell_inputs", &[]);
    let bytes = ibis_formats::blank_file(ibis_formats::Kind::Workbook).unwrap();
    std::fs::write(t.dir.join("blank.xlsx"), bytes).unwrap();
    let x = t.open("blank.xlsx");
    let sheet = t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"][0]["name"].as_str().unwrap().to_string();
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "AA1:AA5", "values": [["="], ["=  1+2  "], [true], ["1,234"], ["＝１＋２"]] }));
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "AA1:AA5" }))["rows"], json!([["="], [3.0], [true], ["1,234"], [3.0]]));
    // The limit is checked from the range, before the cells are made: a range of the whole sheet with one value is refused at once.
    assert_eq!(t.err("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "AA1:AA100001", "values": 0 })), "out_of_range");
    assert_eq!(t.err("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:XFD1048576", "values": "x" })), "out_of_range");
    assert_eq!(t.err("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:XFD1048576", "values": [[1]] })), "out_of_range");
}

#[test]
fn mcp_xlsx_edits_calculate_format_and_survive_saving() {
    let mut t = T::new("xlsx_edits", &["xlsx-sales.xlsx"]);
    let x = t.open("xlsx-sales.xlsx");
    let sheet = t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"][0]["name"].as_str().unwrap().to_string();
    // Values, a formula that is calculated, one undo step.
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "Z1:Z3", "values": [[1], [2.5], ["=Z1+Z2"]] }));
    let cells = t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Z1:Z3" }));
    assert_eq!(cells["rows"], json!([[1.0], [2.5], [3.5]]));
    let formulas = t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Z3", "kind": "formula" }));
    assert_eq!(formulas["rows"], json!([["=Z1+Z2"]]));
    // A single value fills the range; null clears.
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Y2", "values": "text" }));
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Y2" }))["rows"], json!([["text"], ["text"]]));
    assert_eq!(t.err("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Y3", "values": [["a"], ["b"]] })), "out_of_range");
    // Format.
    t.call("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "Z1:Z3", "bold": true, "size": 12, "color": "#112233", "fill": { "theme": "accent1", "brightness": 0.4 }, "number_format": "0.00", "horizontal_alignment": "center" }));
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Z1:Z2", "kind": "text" }))["rows"], json!([["1.00"], ["2.50"]]));
    assert_eq!(t.err("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "Z1", "horizontal_alignment": "middle" })), "invalid_argument");
    assert_eq!(t.err("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "Z1" })), "invalid_argument");
    // `null` takes a format back to the Normal style's: the same file's cells read as they did before the format.
    let plain = t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y2", "kind": "format" }))["rows"][0][0].clone();
    assert!(plain["bold"].is_boolean(), "{plain}");
    t.call("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "Y1", "bold": true, "size": 18, "color": "#112233", "fill": "#FFEE00" }));
    assert_ne!(t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y1", "kind": "format" }))["rows"][0][0], plain);
    t.call("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "Y1", "bold": null, "size": null, "color": null, "fill": null }));
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y1", "kind": "format" }))["rows"][0][0], plain, "null: the Normal style's");
    assert_eq!(t.err("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "Y1", "font": { "theme": "other" } })), "invalid_argument");
    // Clear.
    t.call("xlsx_clear_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Y2", "what": "all" }));
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Y2" }))["rows"], json!([[null], [null]]));
    // Sheets.
    t.call("xlsx_add_sheet", json!({ "document_id": x, "name": "Extra" }));
    t.call("xlsx_rename_sheet", json!({ "document_id": x, "sheet": "Extra", "name": "Renamed" }));
    assert_eq!(t.err("xlsx_add_sheet", json!({ "document_id": x, "name": "renamed" })), "invalid_argument");
    assert_eq!(t.err("xlsx_add_sheet", json!({ "document_id": x, "name": "a/b" })), "invalid_argument");
    let names: Vec<String> = t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"].as_array().unwrap().iter().map(|s| s["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names.last().map(String::as_str), Some("Renamed"));
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": "Renamed", "range": "A1", "values": "x" }));
    t.call("xlsx_calculate_workbook", json!({ "document_id": x }));
    // Saved and opened again.
    let again = t.reopen(&x, "xlsx-sales.xlsx");
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": again, "sheet": sheet, "range": "Z1:Z3" }))["rows"], json!([[1.0], [2.5], [3.5]]));
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": again, "sheet": "Renamed", "range": "A1" }))["rows"], json!([["x"]]));
    // Undo, one step at a time, takes it all back; the last sheet is not deleted.
    assert_eq!(t.call("app_undo_document", json!({ "document_id": x, "count": 100 }))["can_undo"], false);
    let sheets = t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"].as_array().unwrap().len();
    assert_eq!(sheets, names.len() - 1);
    for name in &names[..names.len() - 1] {
        if t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"].as_array().unwrap().len() > 1 {
            t.call("xlsx_delete_sheet", json!({ "document_id": x, "sheet": name }));
        }
    }
    let last = t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"][0]["name"].as_str().unwrap().to_string();
    assert_eq!(t.err("xlsx_delete_sheet", json!({ "document_id": x, "sheet": last })), "unsupported_operation");
}

#[test]
fn mcp_pptx_edits_are_one_undo_step_each_and_survive_saving() {
    let mut t = T::new("pptx_edits", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let slide = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }));
    let with_text = slide["shapes"].as_array().unwrap().iter().find(|s| s["text"].is_string()).expect("a shape with text").clone();
    let shape = with_text["shape_id"].as_u64().unwrap();
    // Text, one undo step.
    t.call("pptx_set_shape_text", json!({ "document_id": p, "slide": 1, "shape": shape, "text": "QQ first\nQQ second" }));
    assert_eq!(t.call("pptx_get_shape_text", json!({ "document_id": p, "slide": 1, "shape": shape }))["paragraphs"].as_array().unwrap().len(), 2);
    let r = t.call("pptx_replace_text", json!({ "document_id": p, "find": "QQ", "replace": "Z" }));
    assert_eq!(r["replaced"], 2);
    let shown = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }));
    let text = shown["shapes"].as_array().unwrap().iter().find(|s| s["shape_id"] == shape).unwrap()["text"].clone();
    assert_eq!(text, "Z first\nZ second");
    t.call("app_undo_document", json!({ "document_id": p }));
    let undone = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }));
    assert_eq!(undone["shapes"].as_array().unwrap().iter().find(|s| s["shape_id"] == shape).unwrap()["text"], "QQ first\nQQ second");
    // Format the characters of the first paragraph.
    t.call("pptx_set_run_format", json!({ "document_id": p, "slide": 1, "shape": shape, "range": { "paragraph": 0, "from": 0, "to": 2 }, "bold": true, "size": 20, "color": "#336699" }));
    assert_eq!(t.err("pptx_set_run_format", json!({ "document_id": p, "slide": 1, "shape": shape, "bold": "yes" })), "invalid_argument");
    // `null` takes a format back to what the styles give; a theme typeface is written as the theme's (with a warning); null for a typeface is the theme's.
    let run = |t: &mut T| t.call("pptx_get_shape_text", json!({ "document_id": p, "slide": 1, "shape": shape }))["paragraphs"][0]["runs"][0].clone();
    let before = run(&mut t);
    assert_eq!((before["bold"].clone(), before["size"].clone(), before["color"].clone()), (json!(true), json!(20.0), json!("#336699")));
    t.call("pptx_set_run_format", json!({ "document_id": p, "slide": 1, "shape": shape, "range": { "paragraph": 0, "from": 0, "to": 2 }, "bold": null, "size": null, "color": null }));
    let after = run(&mut t);
    assert_eq!((after["bold"].clone(), after["size"].clone(), after["color"].clone()), (Value::Null, Value::Null, Value::Null), "{after}");
    let themed = t.call("pptx_set_run_format", json!({ "document_id": p, "slide": 1, "shape": shape, "range": { "paragraph": 0, "from": 0, "to": 2 }, "font": { "theme": "major" } }));
    let warned = themed["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "theme_font_resolved");
    let named = run(&mut t)["font"].is_string();
    assert!(warned && named || t.err("pptx_set_run_format", json!({ "document_id": p, "slide": 1, "shape": shape, "font": { "theme": "major" } })) == "unsupported_operation", "a theme typeface is written as its name, or refused when the file has none");
    t.call("pptx_set_run_format", json!({ "document_id": p, "slide": 1, "shape": shape, "range": { "paragraph": 0, "from": 0, "to": 2 }, "font": null }));
    assert_eq!(run(&mut t)["font"], Value::Null, "null: the theme's typeface again");
    assert_eq!(t.err("pptx_set_run_format", json!({ "document_id": p, "slide": 1, "shape": shape, "font": { "theme": "other" } })), "invalid_argument");
    // Frame in points, notes.
    t.call("pptx_set_frame", json!({ "document_id": p, "slide": 1, "frames": [{ "shape": shape, "x": 36, "y": 72, "width": 200, "height": 50.5 }] }));
    let moved = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }));
    let frame = moved["shapes"].as_array().unwrap().iter().find(|s| s["shape_id"] == shape).unwrap().clone();
    assert_eq!((frame["x"].clone(), frame["y"].clone(), frame["width"].clone(), frame["height"].clone()), (json!(36.0), json!(72.0), json!(200.0), json!(50.5)));
    t.call("pptx_set_notes", json!({ "document_id": p, "slide": 1, "text": "memo" }));
    // Slides.
    let count = t.call("pptx_list_slides", json!({ "document_id": p }))["slides"].as_array().unwrap().len();
    let layouts = t.call("pptx_list_layouts", json!({ "document_id": p }));
    assert!(!layouts["layouts"].as_array().unwrap().is_empty());
    t.call("pptx_add_slide", json!({ "document_id": p, "index": 1 }));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"].as_array().unwrap().len(), count + 1);
    t.call("pptx_move_slide", json!({ "document_id": p, "slide": 2, "index": 0 }));
    assert_eq!(t.err("pptx_move_slide", json!({ "document_id": p, "slide": 2, "index": 999 })), "out_of_range");
    t.call("pptx_move_slide", json!({ "document_id": p, "slide": 1, "index": 1 }));
    t.call("pptx_delete_shape", json!({ "document_id": p, "slide": 1, "shapes": [shape] }));
    assert_eq!(t.err("pptx_get_shape_text", json!({ "document_id": p, "slide": 1, "shape": shape })), "no_such_shape");
    t.call("app_undo_document", json!({ "document_id": p }));
    // Saved and opened again: the new slide, the notes, the frame.
    let again = t.reopen(&p, "pptx-plan.pptx");
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": again }))["slides"].as_array().unwrap().len(), count + 1);
    assert_eq!(t.call("pptx_get_notes", json!({ "document_id": again, "slide": 1 }))["notes"], "memo");
    // The stage B reads.
    let theme = t.call("pptx_get_theme", json!({ "document_id": again }));
    assert!(theme["colors"]["accent1"].as_str().is_some_and(|c| c.starts_with('#') && c.len() == 7), "{theme}");
    assert!(t.call("pptx_list_comments", json!({ "document_id": again, "slide": 1 }))["comments"].is_array());
    assert!(t.call("pptx_get_transition", json!({ "document_id": again, "slide": 1 })).get("transition").is_some());
    assert!(t.call("pptx_list_animations", json!({ "document_id": again, "slide": 1 }))["animations"].is_array());
    let first_shape = t.call("pptx_get_slide", json!({ "document_id": again, "slide": 1 }))["shapes"][0]["shape_id"].clone();
    assert_eq!(t.err("pptx_get_table", json!({ "document_id": again, "slide": 1, "shape": first_shape })), "unsupported_operation");
    let saved = t.call("pptx_get_slide", json!({ "document_id": again, "slide": 1 }));
    let frame = saved["shapes"].as_array().unwrap().iter().find(|s| s["x"] == 36.0 && s["y"] == 72.0 && s["width"] == 200.0);
    assert!(frame.is_some(), "the moved shape is where it was put: {saved}");
    t.call("pptx_delete_slide", json!({ "document_id": p, "slide": 1 }));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"].as_array().unwrap().len(), count);
}

/// #159 stage B: in a document `null` takes a direct format away (the styles decide again) and survives saving; a theme typeface is written as the theme's name with a warning.
#[test]
fn mcp_docx_null_takes_a_format_back_and_a_theme_typeface_is_written() {
    let mut t = T::new("docx_null", &["autospace-letters.docx"]);
    let d = t.open("autospace-letters.docx");
    let range = json!({ "from": { "paragraph": 0, "offset": 0 }, "to": { "paragraph": 0, "offset": 3 } });
    let run = |t: &mut T, d: &str| t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["runs"][0].clone();
    let before = run(&mut t, &d);
    t.call("docx_set_run_format", json!({ "document_id": d, "range": range, "bold": true, "size": 31, "color": "#FF8000", "font": "Arial" }));
    let direct = run(&mut t, &d);
    assert_eq!((direct["bold"].clone(), direct["size"].clone(), direct["color"].clone()), (json!(true), json!(31.0), json!("#FF8000")));
    t.call("docx_set_run_format", json!({ "document_id": d, "range": range, "bold": null, "size": null, "color": null, "font": null }));
    let reset = run(&mut t, &d);
    for key in ["bold", "size", "color", "font"] {
        assert_eq!(reset[key], before[key], "{key} is what the styles give again: {before} {reset}");
    }
    let saved = t.reopen(&d, "autospace-letters.docx");
    assert_eq!(run(&mut t, &saved)["size"], before["size"], "and the file has no direct size");
    let themed = t.call("docx_set_run_format", json!({ "document_id": d, "range": range, "font": { "theme": "minor" } }));
    assert!(themed["warnings"].as_array().unwrap().iter().any(|w| w["code"] == "theme_font_resolved"), "{themed}");
    assert_eq!(t.err("docx_set_run_format", json!({ "document_id": d, "range": range, "font": { "theme": "x" } })), "invalid_argument");
}

/// #159 stage B, spreadsheets: rows and columns, sizes, merging, sorting, moving a sheet. Each is one undo step and is still there after saving.
#[test]
fn mcp_xlsx_stage_b_rows_columns_merge_sort_and_move_survive_saving() {
    let mut t = T::new("xlsx_stage_b", &["xlsx-sales.xlsx"]);
    let x = t.open("xlsx-sales.xlsx");
    let sheet = t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"][0]["name"].as_str().unwrap().to_string();
    let cells = |t: &mut T, id: &str, range: &str| t.call("xlsx_get_cells", json!({ "document_id": id, "sheet": sheet, "range": range }))["rows"].clone();
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Z4", "values": [["c", 3], ["a", 1], ["b", 2], ["tot", "=SUM(Z1:Z3)"]] }));
    // Sort the first three rows by the number, descending: the keys are in the range.
    t.call("xlsx_sort_range", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Z3", "keys": [{ "column": "Z", "order": "descending" }] }));
    assert_eq!(cells(&mut t, &x, "Y1:Z3"), json!([["c", 3.0], ["b", 2.0], ["a", 1.0]]));
    t.call("xlsx_sort_range", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Z3", "keys": [{ "column": 25 }] }));
    assert_eq!(cells(&mut t, &x, "Y1:Z3"), json!([["a", 1.0], ["b", 2.0], ["c", 3.0]]));
    assert_eq!(t.err("xlsx_sort_range", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Z3", "keys": [{ "column": "A" }] })), "out_of_range");
    assert_eq!(t.err("xlsx_sort_range", json!({ "document_id": x, "sheet": sheet, "range": "Y1:Z3", "keys": [{ "column": "Z", "order": "up" }] })), "invalid_argument");
    // A row inserted above row 2: what was there moves down, and the formula that sums it follows.
    t.call("xlsx_insert_rows", json!({ "document_id": x, "sheet": sheet, "at": 2, "count": 2 }));
    assert_eq!(cells(&mut t, &x, "Y1:Z6"), json!([["a", 1.0], [null, null], [null, null], ["b", 2.0], ["c", 3.0], ["tot", 6.0]]));
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Z6", "kind": "formula" }))["rows"], json!([["=SUM(Z1:Z5)"]]));
    t.call("xlsx_delete_rows", json!({ "document_id": x, "sheet": sheet, "at": 2, "count": 2 }));
    assert_eq!(cells(&mut t, &x, "Y1:Z4"), json!([["a", 1.0], ["b", 2.0], ["c", 3.0], ["tot", 6.0]]));
    // Columns by letter or by number.
    t.call("xlsx_insert_columns", json!({ "document_id": x, "sheet": sheet, "at": "Z" }));
    assert_eq!(cells(&mut t, &x, "Y1:AA1"), json!([["a", null, 1.0]]));
    t.call("xlsx_delete_columns", json!({ "document_id": x, "sheet": sheet, "at": 26 }));
    assert_eq!(cells(&mut t, &x, "Y1:Z1"), json!([["a", 1.0]]));
    assert_eq!(t.err("xlsx_insert_rows", json!({ "document_id": x, "sheet": sheet, "at": 0 })), "invalid_argument");
    assert_eq!(t.err("xlsx_insert_columns", json!({ "document_id": x, "sheet": sheet, "at": "XFE" })), "invalid_argument");
    // Sizes in points; merging and splitting.
    t.call("xlsx_set_size", json!({ "document_id": x, "sheet": sheet, "axis": "column", "at": "Y", "count": 2, "size": 90 }));
    t.call("xlsx_set_size", json!({ "document_id": x, "sheet": sheet, "axis": "row", "at": 1, "size": 30.5 }));
    assert_eq!(t.err("xlsx_set_size", json!({ "document_id": x, "sheet": sheet, "axis": "row", "at": 1, "size": 500 })), "invalid_argument");
    assert_eq!(t.err("xlsx_set_size", json!({ "document_id": x, "sheet": sheet, "axis": "side", "at": 1, "size": 5 })), "invalid_argument");
    t.call("xlsx_merge_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y10:Z10", "center": true }));
    assert_eq!(t.err("xlsx_merge_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y10" })), "invalid_argument");
    t.call("xlsx_unmerge_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y10:Z10" }));
    t.call("xlsx_merge_cells", json!({ "document_id": x, "sheet": sheet, "range": "Y12:Z12" }));
    // Moving a sheet.
    t.call("xlsx_add_sheet", json!({ "document_id": x, "name": "Last" }));
    t.call("xlsx_move_sheet", json!({ "document_id": x, "sheet": "Last", "index": 0 }));
    assert_eq!(t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"][0]["name"], "Last");
    assert_eq!(t.err("xlsx_move_sheet", json!({ "document_id": x, "sheet": "Last", "index": 99 })), "out_of_range");
    // Saved and opened again.
    let again = t.reopen(&x, "xlsx-sales.xlsx");
    assert_eq!(t.call("xlsx_list_sheets", json!({ "document_id": again }))["sheets"][0]["name"], "Last");
    assert_eq!(cells(&mut t, &again, "Y1:Z4"), json!([["a", 1.0], ["b", 2.0], ["c", 3.0], ["tot", 6.0]]));
    // One undo step each: all the way back, the sheet is as it was.
    t.call("app_undo_document", json!({ "document_id": x, "count": 100 }));
    assert_eq!(cells(&mut t, &x, "Y1:Z4"), json!([[null, null], [null, null], [null, null], [null, null]]));
}

/// #159 stage B, documents: the text of a table cell, a paragraph style, the styles the document has.
#[test]
fn mcp_docx_stage_b_table_cells_and_paragraph_styles_survive_saving() {
    let mut t = T::new("docx_stage_b", &["autospace-letters.docx"]);
    let d = t.open("autospace-letters.docx");
    let styles = t.call("docx_list_styles", json!({ "document_id": d }));
    let list = styles["styles"].as_array().unwrap();
    assert!(!list.is_empty(), "{styles}");
    let style = list.iter().find(|s| s["id"].as_str() != styles["default"].as_str()).unwrap_or(&list[0])["id"].as_str().unwrap().to_string();
    t.call("docx_set_paragraph_style", json!({ "document_id": d, "paragraphs": [0], "style": style }));
    assert_eq!(t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["style"], style.as_str());
    t.call("docx_set_paragraph_style", json!({ "document_id": d, "paragraphs": [0], "style": null }));
    assert_eq!(t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["style"], Value::Null);
    t.call("app_undo_document", json!({ "document_id": d }));
    assert_eq!(t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["style"], style.as_str());
    assert_eq!(t.err("docx_set_paragraph_style", json!({ "document_id": d, "paragraphs": [0], "style": "No Such Style" })), "invalid_argument");
    assert_eq!(t.err("docx_set_paragraph_style", json!({ "document_id": d, "paragraphs": [9999], "style": style })), "out_of_range");
    // A table, then a cell.
    t.call("docx_insert_table", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "rows": 2, "columns": 2, "cells": [["a", "b"], ["c", ""]] }));
    t.call("docx_set_table_cell", json!({ "document_id": d, "table": 0, "row": 1, "column": 1, "text": "new" }));
    t.call("docx_set_table_cell", json!({ "document_id": d, "table": 0, "row": 0, "column": 0, "text": "" }));
    assert_eq!(t.call("docx_get_table", json!({ "document_id": d, "table": 0 }))["rows"], json!([["", "b"], ["c", "new"]]));
    assert_eq!(t.err("docx_set_table_cell", json!({ "document_id": d, "table": 0, "row": 2, "column": 0, "text": "x" })), "out_of_range");
    assert_eq!(t.err("docx_set_table_cell", json!({ "document_id": d, "table": 99, "row": 0, "column": 0, "text": "x" })), "out_of_range");
    assert_eq!(t.err("docx_set_table_cell", json!({ "document_id": d, "table": 0, "row": 0, "column": 0, "text": "x\ny" })), "invalid_argument");
    let again = t.reopen(&d, "autospace-letters.docx");
    assert_eq!(t.call("docx_get_table", json!({ "document_id": again, "table": 0 }))["rows"], json!([["", "b"], ["c", "new"]]));
    assert_eq!(t.call("docx_get_paragraph", json!({ "document_id": again, "paragraph": 0 }))["style"], style.as_str());
}

/// #159 stage B, presentations: a text box added with its frame, a fill.
#[test]
fn mcp_pptx_stage_b_text_box_and_fill_survive_saving() {
    let mut t = T::new("pptx_stage_b", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let before = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"].as_array().unwrap().len();
    let r = t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "text_box", "frame": { "x": 40, "y": 60, "width": 220, "height": 40.5 }, "text": "hello box" }));
    let shape = r["shape"].as_u64().unwrap();
    let slide = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }));
    assert_eq!(slide["shapes"].as_array().unwrap().len(), before + 1);
    let made = slide["shapes"].as_array().unwrap().iter().find(|s| s["shape_id"] == shape).unwrap().clone();
    assert_eq!((made["x"].clone(), made["y"].clone(), made["width"].clone(), made["height"].clone(), made["text"].clone()), (json!(40.0), json!(60.0), json!(220.0), json!(40.5), json!("hello box")));
    let kind_before = made["kind"].clone();
    t.call("pptx_set_fill", json!({ "document_id": p, "slide": 1, "shapes": [shape], "color": "#FFCC00" }));
    let filled = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"].as_array().unwrap().iter().find(|s| s["shape_id"] == shape).unwrap().clone();
    assert_ne!(filled["kind"], kind_before, "a filled text box is a shape with a fill: {filled}");
    assert_eq!(filled["text"], "hello box");
    let themed = t.call("pptx_set_fill", json!({ "document_id": p, "slide": 1, "shapes": [shape], "color": { "theme": "accent1", "brightness": 0.4 } }));
    assert!(themed["warnings"].as_array().unwrap().is_empty(), "the theme colour is written as a reference, no warning: {themed}");
    // The fill reads back as the reference, and survives a save and a read.
    let read_fill = |t: &mut T, doc: &str| t.call("pptx_get_slide", json!({ "document_id": doc, "slide": 1 }))["shapes"].as_array().unwrap().iter().find(|s| s["shape_id"] == shape).unwrap()["fill"].clone();
    let fill = read_fill(&mut t, &p);
    assert_eq!((fill["theme"].clone(), fill["brightness"].clone()), (json!("accent1"), json!(0.4)), "{fill}");
    assert!(fill["resolved"].as_str().is_some_and(|c| c.starts_with('#')), "{fill}");
    let reopened = t.reopen(&p, "pptx-theme-fill.pptx");
    let kept = t.call("pptx_get_slide", json!({ "document_id": reopened, "slide": 1 }))["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "hello box").unwrap()["fill"].clone();
    assert_eq!(kept, fill, "the reference is kept through save and read");
    // One undo step takes it back to the earlier solid fill.
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(read_fill(&mut t, &p), json!("#FFCC00"));
    t.call("app_redo_document", json!({ "document_id": p }));
    assert_eq!(t.err("pptx_set_fill", json!({ "document_id": p, "slide": 1, "shapes": [999_999], "color": "#112233" })), "no_such_shape");
    assert_eq!(t.err("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "star", "frame": { "x": 0, "y": 0, "width": 1, "height": 1 } })), "invalid_argument");
    assert_eq!(t.err("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "text_box", "frame": { "x": 0, "y": 0, "width": 0, "height": 1 } })), "invalid_argument");
    // One undo step each.
    t.call("pptx_set_fill", json!({ "document_id": p, "slide": 1, "shapes": [shape], "color": null }));
    let again = t.reopen(&p, "pptx-plan.pptx");
    let saved = t.call("pptx_get_slide", json!({ "document_id": again, "slide": 1 }));
    let found = saved["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "hello box" && s["x"] == 40.0 && s["width"] == 220.0);
    assert!(found.is_some(), "{saved}");
    t.call("app_undo_document", json!({ "document_id": p, "count": 3 }));
    assert_eq!(t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"].as_array().unwrap().len(), before + 1);
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"].as_array().unwrap().len(), before);
}

/// #159, inserting: a paragraph before and after a body paragraph (with a style), and a picture in a paragraph. One undo step each, and saved and opened again.
#[test]
fn mcp_docx_insert_paragraph_and_picture_are_undoable_and_survive_saving() {
    let mut t = T::new("docx_insert", &["autospace-letters.docx"]);
    let d = t.open("autospace-letters.docx");
    let count = |t: &mut T, d: &str| t.call("docx_list_paragraphs", json!({ "document_id": d, "limit": 100000 }))["paragraphs"].as_array().unwrap().len();
    let before = count(&mut t, &d);
    let first = t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"].as_str().unwrap().to_string();
    let styles = t.call("docx_list_styles", json!({ "document_id": d }));
    let style = styles["styles"].as_array().unwrap().iter().find(|s| s["id"] != styles["default"]).map(|s| s["id"].as_str().unwrap().to_string()).expect("a second paragraph style");
    // After paragraph 0, with a style; before paragraph 0.
    assert_eq!(t.call("docx_insert_paragraph", json!({ "document_id": d, "paragraph": 0, "text": "after zero", "style": style }))["paragraph"], 1);
    assert_eq!(t.call("docx_insert_paragraph", json!({ "document_id": d, "paragraph": 0, "position": "before", "text": "before zero" }))["paragraph"], 0);
    assert_eq!(count(&mut t, &d), before + 2);
    let p = |t: &mut T, d: &str, n: usize| t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": n }));
    assert_eq!(p(&mut t, &d, 0)["text"], "before zero");
    assert_eq!(p(&mut t, &d, 1)["text"], first);
    assert_eq!(p(&mut t, &d, 2)["text"], "after zero");
    assert_eq!(p(&mut t, &d, 2)["style"], style.as_str());
    // An empty paragraph, and the errors.
    t.call("docx_insert_paragraph", json!({ "document_id": d, "paragraph": 2 }));
    assert_eq!(p(&mut t, &d, 3)["text"], "");
    assert_eq!(t.err("docx_insert_paragraph", json!({ "document_id": d, "paragraph": 0, "text": "a\nb" })), "invalid_argument");
    assert_eq!(t.err("docx_insert_paragraph", json!({ "document_id": d, "paragraph": 0, "style": "No Such Style" })), "invalid_argument");
    assert_eq!(t.err("docx_insert_paragraph", json!({ "document_id": d, "paragraph": 99999 })), "out_of_range");
    assert_eq!(t.err("docx_insert_paragraph", json!({ "document_id": d, "paragraph": 0, "position": "middle" })), "invalid_argument");
    // A picture: both sizes, one size (the proportion is kept), none (96 pixels per inch).
    let path = t.dir.join("picture-120x60.png").to_string_lossy().to_string();
    let jpeg = t.dir.join("picture-80x160.jpg").to_string_lossy().to_string();
    t.call("docx_insert_picture", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "path": path, "width": 100, "height": 40 }));
    t.call("docx_insert_picture", json!({ "document_id": d, "at": { "paragraph": 1, "offset": 0 }, "path": path, "width": 90 }));
    t.call("docx_insert_picture", json!({ "document_id": d, "at": { "paragraph": 2, "offset": 0 }, "path": jpeg }));
    let sizes = |t: &mut T, d: &str, n: usize| {
        let v = p(t, d, n)["pictures"].clone();
        (v[0]["width"].as_f64().unwrap(), v[0]["height"].as_f64().unwrap())
    };
    assert_eq!(sizes(&mut t, &d, 0), (100.0, 40.0));
    assert_eq!(sizes(&mut t, &d, 1), (90.0, 45.0));
    assert_eq!(sizes(&mut t, &d, 2), (60.0, 120.0));
    assert_eq!(t.err("docx_insert_picture", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "path": path, "width": -1 })), "invalid_argument");
    assert_eq!(t.err("docx_insert_picture", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "path": t.dir.join("copy_none.png").to_string_lossy() })), "file_not_found");
    assert_eq!(t.err("docx_insert_picture", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "path": t.dir.join("autospace-letters.docx").to_string_lossy() })), "unsupported_format");
    // Saved and opened again: the paragraphs and the pictures are there.
    let again = t.reopen(&d, "autospace-letters.docx");
    assert_eq!(count(&mut t, &again), before + 3);
    assert_eq!(p(&mut t, &again, 2)["text"], "after zero");
    assert_eq!(p(&mut t, &again, 2)["style"], style.as_str());
    assert_eq!(sizes(&mut t, &again, 0), (100.0, 40.0));
    assert_eq!(sizes(&mut t, &again, 1), (90.0, 45.0));
    assert_eq!(sizes(&mut t, &again, 2), (60.0, 120.0));
    // One undo step each: the three pictures, then the empty paragraph, then the two paragraphs.
    t.call("app_undo_document", json!({ "document_id": d, "count": 3 }));
    assert!(p(&mut t, &d, 0)["pictures"].as_array().unwrap().is_empty() && p(&mut t, &d, 1)["pictures"].as_array().unwrap().is_empty());
    t.call("app_undo_document", json!({ "document_id": d }));
    assert_eq!(count(&mut t, &d), before + 2);
    t.call("app_undo_document", json!({ "document_id": d }));
    assert_eq!(count(&mut t, &d), before + 1);
    t.call("app_undo_document", json!({ "document_id": d }));
    assert_eq!(count(&mut t, &d), before);
    assert_eq!(p(&mut t, &d, 0)["text"], first);
}

/// #159, inserting: a picture on a slide, with a frame, with one size and without; undo; saved and opened again.
#[test]
fn mcp_pptx_insert_picture_is_undoable_and_survives_saving() {
    let mut t = T::new("pptx_insert_picture", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let shapes = |t: &mut T, d: &str| t.call("pptx_get_slide", json!({ "document_id": d, "slide": 1 }))["shapes"].as_array().unwrap().clone();
    let before = shapes(&mut t, &p).len();
    let png = t.dir.join("picture-120x60.png").to_string_lossy().to_string();
    let jpeg = t.dir.join("picture-80x160.jpg").to_string_lossy().to_string();
    let a = t.call("pptx_insert_picture", json!({ "document_id": p, "slide": 1, "path": png, "frame": { "x": 30, "y": 40.5, "width": 200, "height": 100 } }));
    let b = t.call("pptx_insert_picture", json!({ "document_id": p, "slide": 1, "path": png, "frame": { "x": 10, "y": 20, "width": 150 } }));
    let c = t.call("pptx_insert_picture", json!({ "document_id": p, "slide": 1, "path": jpeg }));
    let (a, b, c) = (a["shape"].as_u64().unwrap(), b["shape"].as_u64().unwrap(), c["shape"].as_u64().unwrap());
    let find = |list: &[Value], id: u64| list.iter().find(|s| s["shape_id"] == id).unwrap().clone();
    let list = shapes(&mut t, &p);
    assert_eq!(list.len(), before + 3);
    let (sa, sb, sc) = (find(&list, a), find(&list, b), find(&list, c));
    assert_eq!((sa["kind"].clone(), sa["x"].clone(), sa["y"].clone(), sa["width"].clone(), sa["height"].clone()), (json!("picture"), json!(30.0), json!(40.5), json!(200.0), json!(100.0)));
    assert_eq!((sb["width"].clone(), sb["height"].clone()), (json!(150.0), json!(75.0)));
    // Without a frame: its size in pixels (60 x 120 pt), centred on the slide.
    assert_eq!((sc["width"].clone(), sc["height"].clone()), (json!(60.0), json!(120.0)));
    assert!(sc["x"].as_f64().unwrap() > 0.0 && sc["y"].as_f64().unwrap() > 0.0, "{sc}");
    assert_eq!(t.err("pptx_insert_picture", json!({ "document_id": p, "slide": 99, "path": png })), "out_of_range");
    assert_eq!(t.err("pptx_insert_picture", json!({ "document_id": p, "slide": 1, "path": png, "frame": { "width": 0 } })), "invalid_argument");
    assert_eq!(t.err("pptx_insert_picture", json!({ "document_id": p, "slide": 1, "path": t.dir.join("pptx-plan.pptx").to_string_lossy() })), "unsupported_format");
    let again = t.reopen(&p, "pptx-plan.pptx");
    let saved = shapes(&mut t, &again);
    assert_eq!(saved.iter().filter(|s| s["kind"] == "picture" && s["x"] == 30.0 && s["width"] == 200.0 && s["height"] == 100.0).count(), 1, "{saved:?}");
    assert_eq!(saved.iter().filter(|s| s["kind"] == "picture").count(), 3);
    t.call("app_undo_document", json!({ "document_id": p, "count": 3 }));
    assert_eq!(shapes(&mut t, &p).len(), before);
}

/// #159: the hyperlink theme colours (`hyperlink`, `followed_hyperlink`) are taken for a docx text colour and an xlsx cell's colour and fill; one undo takes each back; they survive saving.
#[test]
fn mcp_hyperlink_theme_colours_are_taken_undone_once_and_survive_saving() {
    let mut t = T::new("hyperlink_theme", &["autospace-letters.docx", "xlsx-sales.xlsx"]);
    // docx
    let d = t.open("autospace-letters.docx");
    let range = json!({ "from": { "paragraph": 0, "offset": 0 }, "to": { "paragraph": 0, "offset": 3 } });
    let run = |t: &mut T, d: &str| t.call("docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["runs"][0].clone();
    let before = run(&mut t, &d);
    t.call("docx_set_run_format", json!({ "document_id": d, "range": range, "color": { "theme": "hyperlink" } }));
    assert_eq!(run(&mut t, &d)["color"], "#0563C1", "{}", run(&mut t, &d));
    t.call("docx_set_run_format", json!({ "document_id": d, "range": range, "color": { "theme": "followed_hyperlink" } }));
    assert_eq!(run(&mut t, &d)["color"], "#954F72");
    let saved = t.reopen(&d, "autospace-letters.docx");
    assert_eq!(run(&mut t, &saved)["color"], "#954F72", "kept by saving");
    t.call("app_undo_document", json!({ "document_id": d }));
    assert_eq!(run(&mut t, &d)["color"], "#0563C1", "one undo takes one call back");
    t.call("app_undo_document", json!({ "document_id": d }));
    assert_eq!(run(&mut t, &d)["color"], before["color"]);
    // xlsx
    let x = t.open("xlsx-sales.xlsx");
    let sheet = t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"][0]["name"].as_str().unwrap().to_string();
    let format = |t: &mut T, x: &str| t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "Z1", "kind": "format" }))["rows"][0][0].clone();
    let plain = format(&mut t, &x);
    t.call("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "Z1", "color": { "theme": "hyperlink" }, "fill": { "theme": "followed_hyperlink", "brightness": 0.8 } }));
    let themed = format(&mut t, &x);
    assert_ne!(themed, plain, "{themed}");
    let again = t.reopen(&x, "xlsx-sales.xlsx");
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": again, "sheet": sheet, "range": "Z1", "kind": "format" }))["rows"][0][0], themed, "kept by saving");
    t.call("app_undo_document", json!({ "document_id": x }));
    assert_eq!(format(&mut t, &x), plain, "one undo takes both back");
}

#[test]
fn mcp_pptx_stage_b_line_order_and_properties_survive_saving() {
    let mut t = T::new("pptx_stage_b_line", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let shape = t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "text_box", "frame": { "x": 40, "y": 60, "width": 220, "height": 40 }, "text": "line box" }))["shape"].as_u64().unwrap();
    t.call("pptx_set_fill", json!({ "document_id": p, "slide": 1, "shapes": [shape], "color": "#FFCC00" }));
    let find = |t: &mut T, doc: &str| t.call("pptx_get_slide", json!({ "document_id": doc, "slide": 1 }))["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "line box").unwrap().clone();
    // The outline: a theme colour (kept as a reference), width, dash and an arrow.
    t.call("pptx_set_line", json!({ "document_id": p, "slide": 1, "shapes": [shape], "color": { "theme": "accent2" }, "width": 3, "dash": "dash", "arrow_tail": "triangle" }));
    let line = find(&mut t, &p)["line"].clone();
    assert_eq!((line["width"].clone(), line["dash"].clone(), line["arrow_tail"].clone(), line["color"]["theme"].clone()), (json!(3.0), json!("dash"), json!("triangle"), json!("accent2")), "{line}");
    let again = t.reopen(&p, "pptx-stage-b-line.pptx");
    assert_eq!(find(&mut t, &again)["line"], line, "the outline survives saving");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(find(&mut t, &p)["line"], json!(null));
    t.call("app_redo_document", json!({ "document_id": p }));
    t.call("pptx_set_line", json!({ "document_id": p, "slide": 1, "shapes": [shape], "color": null }));
    assert_eq!(find(&mut t, &p)["line"], json!(null));
    assert_eq!(t.err("pptx_set_line", json!({ "document_id": p, "slide": 1, "shapes": [shape], "dash": "wavy" })), "invalid_argument");
    // The order: to the back and to the front.
    let count = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"].as_array().unwrap().len();
    t.call("pptx_arrange_shapes", json!({ "document_id": p, "slide": 1, "shapes": [shape], "order": "back" }));
    assert_eq!(t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"][0]["shape_id"], shape);
    t.call("pptx_arrange_shapes", json!({ "document_id": p, "slide": 1, "shapes": [shape], "order": "front" }));
    assert_eq!(t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"][count - 1]["shape_id"], shape);
    // Properties.
    t.call("pptx_set_shape_properties", json!({ "document_id": p, "slide": 1, "shape": shape, "name": "Callout", "rotation": 30, "hidden": true, "alt_text": "a box" }));
    let s = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"].as_array().unwrap().iter().find(|s| s["shape_id"] == shape).unwrap().clone();
    assert_eq!((s["name"].clone(), s["rotation"].clone(), s["hidden"].clone()), (json!("Callout"), json!(30.0), json!(true)), "{s}");
    t.call("app_undo_document", json!({ "document_id": p }));
    let s = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"].as_array().unwrap().iter().find(|s| s["shape_id"] == shape).unwrap().clone();
    assert_eq!(s["hidden"], json!(false), "one undo step takes all the properties back");
}

#[test]
fn mcp_pptx_stage_b_paragraph_and_text_frame_survive_saving() {
    let mut t = T::new("pptx_stage_b_text", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let shape = t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "text_box", "frame": { "x": 40, "y": 60, "width": 220, "height": 80 }, "text": "one\ntwo" }))["shape"].as_u64().unwrap();
    t.call("pptx_set_paragraph_format", json!({ "document_id": p, "slide": 1, "shape": shape, "paragraphs": [1], "alignment": "center", "space_after": 6, "line_spacing": 1.5 }));
    t.call("pptx_set_text_frame", json!({ "document_id": p, "slide": 1, "shape": shape, "vertical_alignment": "middle", "margins": { "left": 10 }, "wrap": false, "autofit": "shrink" }));
    let read = |t: &mut T, doc: &str| t.call("pptx_get_shape_text", json!({ "document_id": doc, "slide": 1, "shape": shape }));
    let r = read(&mut t, &p);
    assert_eq!((r["paragraphs"][0]["alignment"].clone(), r["paragraphs"][1]["alignment"].clone()), (json!(null), json!("center")), "{r}");
    assert_eq!(r["paragraphs"][1]["space_after"], json!(6.0));
    assert_eq!((r["text_frame"]["vertical_alignment"].clone(), r["text_frame"]["margins"]["left"].clone(), r["text_frame"]["wrap"].clone(), r["text_frame"]["autofit"].clone()), (json!("middle"), json!(10.0), json!(false), json!("shrink")));
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(read(&mut t, &p)["text_frame"]["vertical_alignment"], json!("top"), "one undo step per call");
    assert_eq!(t.err("pptx_set_paragraph_format", json!({ "document_id": p, "slide": 1, "shape": shape, "alignment": "diagonal" })), "invalid_argument");
    assert_eq!(t.err("pptx_set_paragraph_format", json!({ "document_id": p, "slide": 1, "shape": shape, "paragraphs": [9], "level": 1 })), "out_of_range");
}

#[test]
fn mcp_pptx_stage_b_copy_group_and_copy_slide_survive_saving() {
    let mut t = T::new("pptx_stage_b_copy", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let make = |t: &mut T, text: &str, x: f64| t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "text_box", "frame": { "x": x, "y": 60, "width": 80, "height": 30 }, "text": text }))["shape"].as_u64().unwrap();
    let (a, b) = (make(&mut t, "copy A", 40.0), make(&mut t, "copy B", 140.0));
    let slide = |t: &mut T, doc: &str, n: usize| t.call("pptx_get_slide", json!({ "document_id": doc, "slide": n }));
    // Copy.
    let before = slide(&mut t, &p, 1)["shapes"].as_array().unwrap().len();
    let r = t.call("pptx_copy_shape", json!({ "document_id": p, "slide": 1, "shapes": [a, b] }));
    let ids = r["shapes"].as_array().unwrap().clone();
    assert_eq!(ids.len(), 2, "{r}");
    let s = slide(&mut t, &p, 1);
    assert_eq!(s["shapes"].as_array().unwrap().len(), before + 2);
    let copy = s["shapes"].as_array().unwrap().iter().find(|x| x["shape_id"] == ids[0]).unwrap().clone();
    assert_eq!((copy["text"].clone(), copy["x"].clone()), (json!("copy A"), json!(50.0)));
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(slide(&mut t, &p, 1)["shapes"].as_array().unwrap().len(), before, "one undo step");
    // Group and ungroup.
    let g = t.call("pptx_group_shapes", json!({ "document_id": p, "slide": 1, "shapes": [a, b] }))["group"].as_str().unwrap().to_string();
    let s = slide(&mut t, &p, 1);
    assert_eq!(s["groups"][0]["group"], json!(g), "{s}");
    assert_eq!(s["groups"][0]["children"].as_array().unwrap().len(), 2);
    let again = t.reopen(&p, "pptx-stage-b-group.pptx");
    assert_eq!(slide(&mut t, &again, 1)["groups"].as_array().unwrap().len(), 1, "the group survives saving");
    // The two are one group now: nothing to group (the screen's 配置 > グループ化 is off too).
    assert_eq!(t.err("pptx_group_shapes", json!({ "document_id": p, "slide": 1, "shapes": [a, b] })), "invalid_argument");
    // A shape of a group alone is not copied; the whole group is, with its own group.
    assert_eq!(t.err("pptx_copy_shape", json!({ "document_id": p, "slide": 1, "shapes": [a] })), "unsupported_operation");
    let r = t.call("pptx_copy_shape", json!({ "document_id": p, "slide": 1, "shapes": [a, b] }));
    assert_eq!(r["shapes"].as_array().unwrap().len(), 2, "{r}");
    let s = slide(&mut t, &p, 1);
    assert_eq!(s["groups"].as_array().unwrap().len(), 2, "the copy is a group of its own: {s}");
    t.call("app_undo_document", json!({ "document_id": p }));
    // A group and a shape outside it: the group goes whole into the new one (a group inside a group).
    let c = make(&mut t, "copy C", 240.0);
    t.call("pptx_group_shapes", json!({ "document_id": p, "slide": 1, "shapes": [a, c] }));
    let s = slide(&mut t, &p, 1);
    assert_eq!(s["groups"].as_array().unwrap().len(), 2, "the old group and the new one holding it: {s}");
    assert_eq!(s["groups"][0]["children"][0], json!({ "group": g }), "{s}");
    t.call("app_undo_document", json!({ "document_id": p }));
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(slide(&mut t, &p, 1)["groups"].as_array().unwrap().len(), 1, "undone to the first group (the extra shape is gone too)");
    t.call("pptx_ungroup_shapes", json!({ "document_id": p, "slide": 1, "group": g }));
    assert!(slide(&mut t, &p, 1)["groups"].as_array().unwrap().is_empty());
    // Copy a slide.
    let slides = t.call("pptx_list_slides", json!({ "document_id": p }))["slides"].as_array().unwrap().len();
    let r = t.call("pptx_copy_slide", json!({ "document_id": p, "slide": 1 }));
    assert_eq!(r["slide"], json!(2));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"].as_array().unwrap().len(), slides + 1);
    assert_eq!(slide(&mut t, &p, 2)["shapes"], slide(&mut t, &p, 1)["shapes"], "the copy has the same shapes");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"].as_array().unwrap().len(), slides);
}

#[test]
fn mcp_pptx_stage_b_slide_transition_and_document_settings_survive_saving() {
    let mut t = T::new("pptx_stage_b_slide", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    // Transition.
    t.call("pptx_set_transition", json!({ "document_id": p, "slides": [1], "effect": "fade", "duration": 1.5, "advance_after": 4 }));
    let tr = t.call("pptx_get_transition", json!({ "document_id": p, "slide": 1 }))["transition"].clone();
    assert_eq!((tr["effect"].clone(), tr["duration"].clone(), tr["advance_after"].clone()), (json!("fade"), json!(1.5), json!(4.0)), "{tr}");
    let again = t.reopen(&p, "pptx-stage-b-transition.pptx");
    assert_eq!(t.call("pptx_get_transition", json!({ "document_id": again, "slide": 1 }))["transition"]["effect"], json!("fade"));
    assert_eq!(t.err("pptx_set_transition", json!({ "document_id": p, "slides": [1], "effect": "sparkle" })), "invalid_argument");
    t.call("pptx_set_transition", json!({ "document_id": p, "slides": [1], "effect": null }));
    assert_eq!(t.call("pptx_get_transition", json!({ "document_id": p, "slide": 1 }))["transition"], json!(null));
    // Slide properties, size, header and footer, document properties.
    t.call("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "hidden": true, "name": "Opening", "background": "#102030" }));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"][0]["hidden"], json!(true));
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"][0]["hidden"], json!(false), "one undo step");
    t.call("pptx_set_slide_size", json!({ "document_id": p, "preset": "4:3" }));
    let info = t.call("app_get_document_info", json!({ "document_id": p }));
    assert_eq!((info["slide_size"]["width"].clone(), info["slide_size"]["height"].clone()), (json!(720.0), json!(540.0)), "{info}");
    assert_eq!(t.err("pptx_set_slide_size", json!({ "document_id": p, "preset": "4:3", "scale_content": true })), "unsupported_operation");
    t.call("pptx_set_header_footer", json!({ "document_id": p, "slide_number": true, "footer": true, "hide_on_title": true }));
    t.call("pptx_set_document_properties", json!({ "document_id": p, "title": "Plan", "author": "Mac" }));
    let info = t.call("app_get_document_info", json!({ "document_id": p }));
    assert_eq!((info["properties"]["title"].clone(), info["header_footer"]["slide_number"].clone(), info["header_footer"]["hide_on_title"].clone()), (json!("Plan"), json!(true), json!(true)), "{info}");
    // Animations: deleting needs one; none here, so an index is out of range and all is a no-op.
    assert_eq!(t.err("pptx_delete_animation", json!({ "document_id": p, "slide": 1, "index": [0] })), "out_of_range");
}

#[test]
fn mcp_pptx_stage_b_shapes_tables_fills_and_links_survive_saving() {
    let mut t = T::new("pptx_stage_b_table", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let slide = |t: &mut T, doc: &str| t.call("pptx_get_slide", json!({ "document_id": doc, "slide": 1 }));
    // A preset shape and a line.
    let star = t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "star5", "frame": { "x": 300, "y": 60, "width": 80, "height": 80 }, "text": "hi" }))["shape"].as_u64().unwrap();
    let line = t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "line", "frame": { "x": 300, "y": 160, "width": 80, "height": 0.5 } }))["shape"].as_u64().unwrap();
    assert_eq!(t.err("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "nonsense", "frame": { "x": 0, "y": 0, "width": 1, "height": 1 } })), "invalid_argument");
    let s = slide(&mut t, &p);
    let made = s["shapes"].as_array().unwrap().iter().find(|x| x["shape_id"] == star).unwrap().clone();
    assert_eq!(made["fill"]["theme"], json!("accent1"), "{made}");
    assert!(s["shapes"].as_array().unwrap().iter().any(|x| x["shape_id"] == line));
    // Gradient and transparency (the colour stays a reference).
    t.call("pptx_set_fill", json!({ "document_id": p, "slide": 1, "shapes": [star], "color": { "theme": "accent2" }, "transparency": 0.5 }));
    let f = slide(&mut t, &p)["shapes"].as_array().unwrap().iter().find(|x| x["shape_id"] == star).unwrap()["fill"].clone();
    assert_eq!(f["theme"], json!("accent2"), "{f}");
    t.call("pptx_set_fill", json!({ "document_id": p, "slide": 1, "shapes": [star], "gradient": { "angle": 90, "colors": ["#FF0000", { "theme": "accent1" }] } }));
    assert_eq!(t.err("pptx_set_fill", json!({ "document_id": p, "slide": 1, "shapes": [star], "picture": "x.png" })), "unsupported_operation");
    // Link.
    t.call("pptx_set_shape_properties", json!({ "document_id": p, "slide": 1, "shape": star, "hyperlink": "https://example.com/" }));
    assert_eq!(t.err("pptx_set_shape_properties", json!({ "document_id": p, "slide": 1, "shape": star, "hyperlink": 5 })), "invalid_argument");
    // A table.
    let table = t.call("pptx_insert_table", json!({ "document_id": p, "slide": 1, "rows": 2, "columns": 3, "cells": [["A", "B", "C"], ["1", "2", "3"]] }))["shape"].as_u64().unwrap();
    t.call("pptx_set_table_cell", json!({ "document_id": p, "slide": 1, "shape": table, "row": 1, "column": 2, "text": "three", "fill": "#FFEE00" }));
    let read = |t: &mut T, doc: &str| t.call("pptx_get_table", json!({ "document_id": doc, "slide": 1, "shape": table }));
    let r = read(&mut t, &p);
    assert_eq!((r["rows"][1]["cells"][2]["text"].clone(), r["rows"][1]["cells"][2]["fill"].clone(), r["rows"][0]["cells"][1]["text"].clone()), (json!("three"), json!("#FFEE00"), json!("B")), "{r}");
    assert_eq!(t.err("pptx_set_table_cell", json!({ "document_id": p, "slide": 1, "shape": table, "row": 5, "column": 0, "text": "x" })), "out_of_range");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(read(&mut t, &p)["rows"][1]["cells"][2]["text"], json!("3"), "one undo step per call");
    t.call("app_redo_document", json!({ "document_id": p }));
    let again = t.reopen(&p, "pptx-stage-b-table.pptx");
    let table_again = t.call("pptx_get_slide", json!({ "document_id": again, "slide": 1 }))["shapes"].as_array().unwrap().iter().find(|x| x["kind"] == json!("table")).map(|x| x["shape_id"].clone());
    assert!(table_again.is_some(), "the table survives saving");
}


#[test]
fn mcp_pptx_add_animation_is_one_undo_step_and_survives_saving() {
    let mut t = T::new("pptx_add_animation", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let shape = t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"][0]["shape_id"].as_u64().unwrap();
    let before = t.call("pptx_list_animations", json!({ "document_id": p, "slide": 1 }))["animations"].as_array().unwrap().len();
    let r = t.call("pptx_add_animation", json!({ "document_id": p, "slide": 1, "shape": shape, "effect": "fly", "direction": "from_left", "start": "after_previous", "delay": 0.5, "duration": 1.5 }));
    assert_eq!(r["index"], json!(before));
    let list = t.call("pptx_list_animations", json!({ "document_id": p, "slide": 1 }))["animations"].clone();
    let a = &list[before];
    assert_eq!((a["class"].clone(), a["start"].clone(), a["delay"].clone(), a["duration"].clone(), a["shape_id"].clone()), (json!("entrance"), json!("after_previous"), json!(0.5), json!(1.5), json!(shape)), "{list}");
    let again = t.reopen(&p, "pptx-add-animation.pptx");
    let saved = t.call("pptx_list_animations", json!({ "document_id": again, "slide": 1 }))["animations"].clone();
    assert_eq!(saved.as_array().unwrap().len(), before + 1, "the animation survives saving");
    assert_eq!(t.err("pptx_add_animation", json!({ "document_id": p, "slide": 1, "shape": shape, "effect": "explode" })), "invalid_argument");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_list_animations", json!({ "document_id": p, "slide": 1 }))["animations"].as_array().unwrap().len(), before, "one undo step");
    // Deleting what was added.
    t.call("app_redo_document", json!({ "document_id": p }));
    t.call("pptx_delete_animation", json!({ "document_id": p, "slide": 1, "index": [before] }));
    assert_eq!(t.call("pptx_list_animations", json!({ "document_id": p, "slide": 1 }))["animations"].as_array().unwrap().len(), before);
}

#[test]
fn mcp_pptx_add_comment_is_one_undo_step_and_survives_saving() {
    let mut t = T::new("pptx_add_comment", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let before = t.call("pptx_list_comments", json!({ "document_id": p, "slide": 1 }))["comments"].as_array().unwrap().len();
    t.call("pptx_add_comment", json!({ "document_id": p, "slide": 1, "text": "Check the numbers", "author": "Mac Tester" }));
    t.call("pptx_add_comment", json!({ "document_id": p, "slide": 1, "text": "No author given" }));
    let both = t.call("pptx_list_comments", json!({ "document_id": p, "slide": 1 }))["comments"].clone();
    assert_eq!(both[before + 1]["author"], json!("Assistant"), "the default author is Assistant, not the user's name: {both}");
    t.call("app_undo_document", json!({ "document_id": p }));
    let list = t.call("pptx_list_comments", json!({ "document_id": p, "slide": 1 }))["comments"].clone();
    assert_eq!(list.as_array().unwrap().len(), before + 1);
    let c = &list[before];
    assert_eq!((c["text"].clone(), c["author"].clone(), c["resolved"].clone()), (json!("Check the numbers"), json!("Mac Tester"), json!(false)), "{c}");
    assert!(c["created"].as_str().is_some_and(|d| d.len() >= 16), "{c}");
    let again = t.reopen(&p, "pptx-add-comment.pptx");
    let saved = t.call("pptx_list_comments", json!({ "document_id": again, "slide": 1 }))["comments"].clone();
    assert!(saved.as_array().unwrap().iter().any(|c| c["text"] == "Check the numbers" && c["author"] == "Mac Tester"), "the comment survives saving: {saved}");
    assert_eq!(t.err("pptx_add_comment", json!({ "document_id": p, "slide": 1, "text": "  " })), "invalid_argument");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_list_comments", json!({ "document_id": p, "slide": 1 }))["comments"].as_array().unwrap().len(), before, "one undo step");
}

#[test]
fn mcp_pptx_align_shapes_is_one_undo_step() {
    let mut t = T::new("pptx_align", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let make = |t: &mut T, x: f64, w: f64| t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "text_box", "frame": { "x": x, "y": 200, "width": w, "height": 30 }, "text": "a" }))["shape"].as_u64().unwrap();
    let (a, b, c) = (make(&mut t, 20.0, 50.0), make(&mut t, 120.0, 80.0), make(&mut t, 400.0, 60.0));
    let xs = |t: &mut T| -> Vec<f64> { t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"].as_array().unwrap().iter().filter(|s| [a, b, c].contains(&s["shape_id"].as_u64().unwrap())).map(|s| s["x"].as_f64().unwrap()).collect() };
    t.call("pptx_align_shapes", json!({ "document_id": p, "slide": 1, "shapes": [a, b, c], "edge": "left" }));
    assert_eq!(xs(&mut t), vec![20.0, 20.0, 20.0]);
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(xs(&mut t), vec![20.0, 120.0, 400.0], "one undo step");
    // Spread: equal gaps between 20..460 (widths 50, 80, 60 leave 250 over two gaps).
    t.call("pptx_align_shapes", json!({ "document_id": p, "slide": 1, "shapes": [a, b, c], "edge": "distribute_horizontally" }));
    assert_eq!(xs(&mut t), vec![20.0, 20.0 + 50.0 + 125.0, 400.0]);
    assert_eq!(t.err("pptx_align_shapes", json!({ "document_id": p, "slide": 1, "shapes": [a, b], "edge": "distribute_horizontally" })), "invalid_argument");
    assert_eq!(t.err("pptx_align_shapes", json!({ "document_id": p, "slide": 1, "shapes": [a], "edge": "sideways" })), "invalid_argument");
}

#[test]
fn mcp_pptx_set_theme_changes_the_colours_that_follow_it_and_survives_saving() {
    let mut t = T::new("pptx_set_theme", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let star = t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "rect", "frame": { "x": 300, "y": 60, "width": 80, "height": 80 }, "text": "t" }))["shape"].as_u64().unwrap();
    let fill = |t: &mut T, doc: &str| t.call("pptx_get_slide", json!({ "document_id": doc, "slide": 1 }))["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "t").unwrap()["fill"].clone();
    let before = fill(&mut t, &p);
    assert_eq!(before["theme"], json!("accent1"), "{before}");
    t.call("pptx_set_theme", json!({ "document_id": p, "colors": { "accent1": "#00AA00" } }));
    assert_eq!(t.call("pptx_get_theme", json!({ "document_id": p }))["colors"]["accent1"], json!("#00AA00"));
    let after = fill(&mut t, &p);
    assert_eq!((after["theme"].clone(), after["resolved"].clone()), (json!("accent1"), json!("#00AA00")), "the fill follows the theme: {after}");
    let again = t.reopen(&p, "pptx-set-theme.pptx");
    assert_eq!(t.call("pptx_get_theme", json!({ "document_id": again }))["colors"]["accent1"], json!("#00AA00"), "the theme survives saving");
    assert_eq!(fill(&mut t, &again)["resolved"], json!("#00AA00"));
    assert_eq!(t.err("pptx_set_theme", json!({ "document_id": p, "theme": "Ion" })), "file_not_found");
    assert_eq!(t.err("pptx_set_theme", json!({ "document_id": p, "colors": { "accent9": "#000000" } })), "invalid_argument");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_ne!(t.call("pptx_get_theme", json!({ "document_id": p }))["colors"]["accent1"], json!("#00AA00"), "one undo step");
    let _ = star;
}

#[test]
fn mcp_pptx_set_theme_takes_the_theme_of_another_file() {
    let mut t = T::new("pptx_set_theme_file", &["pptx-plan.pptx"]);
    // A reference file with another theme: the plan with its accent 1 changed and a typeface set.
    let r = t.open("pptx-plan.pptx");
    t.call("pptx_set_theme", json!({ "document_id": r, "colors": { "accent1": "#00AA00", "accent2": "#AA00AA" }, "fonts": { "minor": { "latin": "Georgia" } } }));
    t.call("app_save_document", json!({ "document_id": r, "path": t.dir.join("theme-ref.pptx").to_string_lossy(), "overwrite": true }));
    t.call("app_close_document", json!({ "document_id": r, "discard": true }));
    let p = t.open("pptx-plan.pptx");
    t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "rect", "frame": { "x": 300, "y": 60, "width": 80, "height": 80 }, "text": "t" }));
    let fill = |t: &mut T, doc: &str| t.call("pptx_get_slide", json!({ "document_id": doc, "slide": 1 }))["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "t").unwrap()["fill"].clone();
    assert_eq!(fill(&mut t, &p)["theme"], json!("accent1"));
    let name = t.dir.join("theme-ref.pptx").to_string_lossy().to_string();
    t.call("pptx_set_theme", json!({ "document_id": p, "theme": name }));
    let theme = t.call("pptx_get_theme", json!({ "document_id": p }));
    assert_eq!((theme["colors"]["accent1"].clone(), theme["colors"]["accent2"].clone()), (json!("#00AA00"), json!("#AA00AA")), "{theme}");
    assert_eq!(theme["fonts"]["minor"]["latin"], json!("Georgia"), "{theme}");
    let after = fill(&mut t, &p);
    assert_eq!((after["theme"].clone(), after["resolved"].clone()), (json!("accent1"), json!("#00AA00")), "the fill follows the new theme: {after}");
    let again = t.reopen(&p, "pptx-set-theme-file.pptx");
    assert_eq!(t.call("pptx_get_theme", json!({ "document_id": again }))["colors"]["accent1"], json!("#00AA00"), "the theme survives saving");
    // Colours given as well win over the file's.
    t.call("pptx_set_theme", json!({ "document_id": p, "theme": name, "colors": { "accent1": "#112233" } }));
    assert_eq!(t.call("pptx_get_theme", json!({ "document_id": p }))["colors"]["accent1"], json!("#112233"));
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_get_theme", json!({ "document_id": p }))["colors"]["accent1"], json!("#00AA00"), "one undo step");
    assert_eq!(t.err("pptx_set_theme", json!({ "document_id": p, "theme": "C:/no/such/theme.pptx" })), "file_not_found");
}

#[test]
fn mcp_pptx_insert_connector_glues_to_shapes_and_survives_saving() {
    let mut t = T::new("pptx_insert_connector", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let mk = |t: &mut T, x: u32| t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "rect", "frame": { "x": x, "y": 300, "width": 80, "height": 40 } }))["shape"].as_u64().unwrap();
    let (a, b) = (mk(&mut t, 20), mk(&mut t, 300));
    let count = |t: &mut T, doc: &str| t.call("pptx_get_slide", json!({ "document_id": doc, "slide": 1 }))["shapes"].as_array().unwrap().len();
    let before = count(&mut t, &p);
    for kind in ["straight", "elbow", "curved"] {
        let r = t.call("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "kind": kind, "arrow": "end", "start": { "shape": a, "site": 3 }, "end": { "shape": b, "site": 1 } }));
        assert!(r["shape"].as_u64().is_some(), "{r}");
    }
    t.call("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "start": { "x": 10, "y": 10 }, "end": { "x": 100, "y": 50 } }));
    assert_eq!(count(&mut t, &p), before + 4);
    let again = t.reopen(&p, "pptx-insert-connector.pptx");
    assert_eq!(count(&mut t, &again), before + 4, "the connectors survive saving");
    assert_eq!(t.err("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "start": { "shape": a, "site": 9 }, "end": { "x": 1, "y": 1 } })), "out_of_range");
    assert_eq!(t.err("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "start": { "shape": 99999, "site": 0 }, "end": { "x": 1, "y": 1 } })), "no_such_shape");
    assert_eq!(t.err("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "kind": "zigzag", "start": { "x": 1, "y": 1 }, "end": { "x": 1, "y": 1 } })), "invalid_argument");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(count(&mut t, &p), before + 3, "one undo step");
}

#[test]
fn mcp_pptx_set_slide_properties_changes_the_layout() {
    let mut t = T::new("pptx_set_layout", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let layouts = t.call("pptx_list_layouts", json!({ "document_id": p }))["layouts"].as_array().unwrap().clone();
    assert!(layouts.len() >= 2, "{layouts:?}");
    let current = t.call("pptx_list_slides", json!({ "document_id": p }))["slides"][0]["layout"].as_u64().unwrap();
    let target = if current == 1 { 2 } else { 1 };
    let target_name = layouts[target as usize - 1]["name"].as_str().unwrap().to_string();
    let extra = t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "rect", "frame": { "x": 300, "y": 60, "width": 80, "height": 80 }, "text": "keep" }))["shape"].as_u64().unwrap();
    t.call("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": target }));
    let s = t.call("pptx_list_slides", json!({ "document_id": p }));
    assert_eq!((s["slides"][0]["layout"].clone(), s["slides"][0]["layout_name"].clone()), (json!(target), json!(target_name)), "{s}");
    assert!(t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }))["shapes"].as_array().unwrap().iter().any(|x| x["shape_id"] == extra), "a shape that is not a placeholder stays");
    // By name, then saved and opened again.
    t.call("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": layouts[current as usize - 1]["name"] }));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"][0]["layout"], json!(current));
    t.call("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": target }));
    let again = t.reopen(&p, "pptx-set-layout.pptx");
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": again }))["slides"][0]["layout"], json!(target), "the layout survives saving");
    assert_eq!(t.err("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": 999 })), "out_of_range");
    assert_eq!(t.err("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": "No such layout" })), "invalid_argument");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"][0]["layout"], json!(current), "one undo step");
}

#[test]
fn mcp_pptx_set_chart_data_replaces_the_data_and_survives_saving() {
    let mut t = T::new("pptx_set_chart_data", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let chart = t.call("pptx_insert_chart", json!({ "document_id": p, "slide": 1, "kind": "column", "data": [["", "East", "West"], ["Q1", 10, 20], ["Q2", 30, 40]] }))["shape"].as_u64().unwrap();
    let series = |t: &mut T, doc: &str| t.call("pptx_list_charts", json!({ "document_id": doc, "slide": 1 }))["charts"][0]["series"].clone();
    assert_eq!(series(&mut t, &p).as_array().unwrap().len(), 2);
    t.call("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": chart, "series_in": "columns", "data": [["", "East", "West", "North"], ["Q1", 1, 2, 3], ["Q2", 4, 5, 6], ["Q3", 7, 8, 9]] }));
    let s = series(&mut t, &p);
    assert_eq!((s.as_array().unwrap().len(), s[2]["name"].clone(), s[2]["values"].clone()), (3, json!("North"), json!(3)), "{s}");
    let again = t.reopen(&p, "pptx-set-chart-data.pptx");
    let s = series(&mut t, &again);
    assert_eq!((s.as_array().unwrap().len(), s[0]["values"].clone()), (3, json!(3)), "the data survives saving: {s}");
    assert_eq!(t.err("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": chart, "data": [["a"], ["b"]] })), "invalid_argument");
    assert_eq!(t.err("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": 99999, "data": [["", "a"], ["x", 1]] })), "no_such_shape");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(series(&mut t, &p).as_array().unwrap().len(), 2, "one undo step");
}

#[test]
fn mcp_pptx_insert_chart_is_one_undo_step_and_survives_saving() {
    let mut t = T::new("pptx_insert_chart", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let before = t.call("pptx_list_charts", json!({ "document_id": p, "slide": 1 }))["charts"].as_array().unwrap().len();
    let shape = t.call("pptx_insert_chart", json!({ "document_id": p, "slide": 1, "kind": "pie", "frame": { "x": 50, "y": 60, "width": 300, "height": 200 } }))["shape"].as_u64().unwrap();
    let charts = t.call("pptx_list_charts", json!({ "document_id": p, "slide": 1 }))["charts"].clone();
    assert_eq!(charts.as_array().unwrap().len(), before + 1, "{charts}");
    assert!(charts.as_array().unwrap().iter().any(|c| c["shape_id"] == shape && c["width"] == 300.0 && !c["series"].as_array().unwrap().is_empty()), "{charts}");
    let again = t.reopen(&p, "pptx-insert-chart.pptx");
    assert_eq!(t.call("pptx_list_charts", json!({ "document_id": again, "slide": 1 }))["charts"].as_array().unwrap().len(), before + 1, "the chart survives saving");
    // With data: the first row names the series, the first column the categories; title and legend as the other tools take them.
    let own = t.call("pptx_insert_chart", json!({ "document_id": p, "slide": 1, "kind": "column", "data": [["", "2025", "2026"], ["East", 10, 14], ["West", 7, 9]], "title": "Sales", "legend": "bottom" }))["shape"].as_u64().unwrap();
    let charts = t.call("pptx_list_charts", json!({ "document_id": p, "slide": 1 }))["charts"].clone();
    let c = charts.as_array().unwrap().iter().find(|c| c["shape_id"] == own).unwrap().clone();
    assert_eq!(c["series"].as_array().unwrap().iter().map(|s| (s["name"].clone(), s["values"].clone())).collect::<Vec<_>>(), vec![(json!("2025"), json!(2)), (json!("2026"), json!(2))], "{c}");
    let again2 = t.reopen(&p, "pptx-insert-chart-data.pptx");
    let saved = t.call("pptx_list_charts", json!({ "document_id": again2, "slide": 1 }))["charts"].clone();
    assert!(saved.as_array().unwrap().iter().any(|c| c["series"][0]["name"] == "2025" && c["series"][1]["name"] == "2026"), "the data survives saving: {saved}");
    assert_eq!(t.err("pptx_insert_chart", json!({ "document_id": p, "slide": 1, "kind": "column", "data": [[1]] })), "invalid_argument");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.err("pptx_insert_chart", json!({ "document_id": p, "slide": 1, "kind": "radar_burst" })), "invalid_argument");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_list_charts", json!({ "document_id": p, "slide": 1 }))["charts"].as_array().unwrap().len(), before, "one undo step");
}

/// A text colour set as a theme colour is still a reference after saving (it is `a:schemeClr`, not `a:srgbClr`).
#[test]
fn mcp_pptx_run_colour_as_theme_survives_saving() {
    let mut t = T::new("pptx_run_theme_colour", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let shape = t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "text_box", "frame": { "x": 40, "y": 60, "width": 200, "height": 30 }, "text": "theme text" }))["shape"].as_u64().unwrap();
    for color in [json!({ "theme": "text2" }), json!({ "theme": "accent2", "brightness": 0.4 })] {
        t.call("pptx_set_run_format", json!({ "document_id": p, "slide": 1, "shape": shape, "color": color }));
        // The shape is the one that has the text (its id is another after saving).
        let run = |t: &mut T, id: &str| {
            let slide = t.call("pptx_get_slide", json!({ "document_id": id, "slide": 1 }));
            let n = slide["shapes"].as_array().unwrap().iter().find(|s| s["text"] == "theme text").unwrap()["shape_id"].as_u64().unwrap();
            t.call("pptx_get_shape_text", json!({ "document_id": id, "slide": 1, "shape": n }))["paragraphs"][0]["runs"][0]["color"].clone()
        };
        let before = run(&mut t, &p);
        assert!(before.get("theme").is_some(), "{before}");
        let again = t.reopen(&p, "pptx-run-theme.pptx");
        assert_eq!(run(&mut t, &again), before, "the reference is kept by saving");
    }
}

/// A file PowerPoint saved with groups (G2 holds the group G1 and C; G3 holds E and F; D is alone): copying whole groups, a group inside
/// a group, and a shape of a group alone (#159). PowerPoint's own duplicate of G2, D and G3 keeps G2{G1{A,B},C}, D and G3{E,F}.
#[test]
fn mcp_pptx_copy_shape_with_groups_of_a_powerpoint_file() {
    let mut t = T::new("pptx_copy_nested_groups", &["pptx-nested-groups.pptx"]);
    let p = t.open("pptx-nested-groups.pptx");
    let slide = |t: &mut T| t.call("pptx_get_slide", json!({ "document_id": p, "slide": 1 }));
    let s = slide(&mut t);
    let by_name = |s: &Value, name: &str| s["shapes"].as_array().unwrap().iter().find(|x| x["name"] == json!(name)).unwrap_or_else(|| panic!("{name} in {s}"))["shape_id"].as_u64().unwrap();
    let [a, b, c, d, e, f] = ["A", "B", "C", "D", "E", "F"].map(|n| by_name(&s, n));
    let groups = |s: &Value| s["groups"].as_array().unwrap().len();
    assert_eq!(groups(&s), 3, "G1, G2 and G3: {s}");
    // The whole outer group: it comes with the group inside it.
    let r = t.call("pptx_copy_shape", json!({ "document_id": p, "slide": 1, "shapes": [a, b, c] }));
    assert_eq!(r["shapes"].as_array().unwrap().len(), 3, "{r}");
    let s2 = slide(&mut t);
    assert_eq!(groups(&s2), 5, "G1 and G2 again: {s2}");
    t.call("app_undo_document", json!({ "document_id": p }));
    // The group inside it alone: a group of its own.
    let r = t.call("pptx_copy_shape", json!({ "document_id": p, "slide": 1, "shapes": [a, b] }));
    assert_eq!(r["shapes"].as_array().unwrap().len(), 2, "{r}");
    assert_eq!(groups(&slide(&mut t)), 4, "G1 again, not two loose shapes");
    t.call("app_undo_document", json!({ "document_id": p }));
    // A shape and a group.
    let r = t.call("pptx_copy_shape", json!({ "document_id": p, "slide": 1, "shapes": [d, e, f] }));
    assert_eq!(r["shapes"].as_array().unwrap().len(), 3, "{r}");
    assert_eq!(groups(&slide(&mut t)), 4);
    // A shape of a group alone is refused.
    assert_eq!(t.err("pptx_copy_shape", json!({ "document_id": p, "slide": 1, "shapes": [a] })), "unsupported_operation");
    assert_eq!(t.err("pptx_copy_shape", json!({ "document_id": p, "slide": 1, "shapes": [d, e] })), "unsupported_operation");
    let again = t.reopen(&p, "pptx-nested-groups-copied.pptx");
    let s3 = t.call("pptx_get_slide", json!({ "document_id": again, "slide": 1 }));
    assert_eq!(groups(&s3), 4, "the copies' groups survive saving: {s3}");
}
