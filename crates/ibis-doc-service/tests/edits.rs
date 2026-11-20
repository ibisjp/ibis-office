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
