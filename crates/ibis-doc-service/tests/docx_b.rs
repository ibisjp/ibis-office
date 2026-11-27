// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #159, stage B (docx), step 1: the reading tools (page setup, comments, tracked changes, shapes, content controls) and what
//! `docx_get_paragraph`, `docx_list_styles` and `app_get_document_info` say more.

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

fn open(s: &mut Service, dir: &Path, name: &str) -> String {
    s.call("app_open_document", &json!({ "path": dir.join(name).to_string_lossy() })).unwrap()["document_id"].as_str().unwrap().to_string()
}

fn call(s: &mut Service, tool: &str, args: Value) -> Value {
    s.call(tool, &args).unwrap_or_else(|e| panic!("{tool}: {e}"))
}

/// The page setup is read per section in points; a document of a header says its text.
#[test]
fn mcp_docx_get_page_setup_reads_the_paper_margins_and_headers() {
    let dir = sandbox("docx_b_page_setup", &["docx-report.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "docx-report.docx");
    let setup = call(&mut s, "docx_get_page_setup", json!({ "document_id": d }));
    let first = &setup["sections"][0];
    let (w, h) = (first["paper"]["width"].as_f64().unwrap(), first["paper"]["height"].as_f64().unwrap());
    assert!(w > 100.0 && h > 100.0, "{setup}");
    assert_eq!(first["orientation"], if w > h { "landscape" } else { "portrait" });
    for key in ["top", "bottom", "left", "right"] {
        assert!(first["margins"][key].as_f64().unwrap() >= 0.0, "{setup}");
    }
    assert!(first["header"].is_string() && first["footer"].is_string(), "{setup}");
    assert_eq!(first["text_direction"], "horizontal");
    assert!(setup["different_odd_even"].is_boolean());
}

#[test]
fn mcp_docx_list_tools_answer_on_a_document_without_such_things() {
    let dir = sandbox("docx_b_lists", &["autospace-letters.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "autospace-letters.docx");
    assert_eq!(call(&mut s, "docx_list_comments", json!({ "document_id": d }))["total"], 0);
    let revisions = call(&mut s, "docx_list_revisions", json!({ "document_id": d }));
    assert_eq!(revisions["total"], 0);
    assert_eq!(revisions["tracking"], false);
    assert_eq!(call(&mut s, "docx_list_shapes", json!({ "document_id": d }))["shapes"], json!([]));
    assert_eq!(call(&mut s, "docx_list_content_controls", json!({ "document_id": d }))["controls"], json!([]));
}

/// The paragraph's format as it shows, its list, links and fields; a style's own format; the document's properties.
#[test]
fn mcp_docx_get_paragraph_lists_styles_and_info_say_the_format_and_properties() {
    let dir = sandbox("docx_b_format", &["docx-report.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "docx-report.docx");
    let p = call(&mut s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }));
    let format = &p["format"];
    assert!(["left", "center", "right", "justify", "distribute"].contains(&format["alignment"].as_str().unwrap()), "{p}");
    assert!(format["line_spacing"]["rule"].is_string() && format["space_after"].is_number(), "{p}");
    assert!(p["links"].is_array() && p["fields"].is_array() && p["bookmarks"].is_array() && p["comments"].is_array(), "{p}");
    let styles = call(&mut s, "docx_list_styles", json!({ "document_id": d }));
    let any_format = styles["styles"].as_array().unwrap().iter().any(|st| !st["paragraph_format"].as_object().unwrap().is_empty() || !st["run_format"].as_object().unwrap().is_empty());
    assert!(any_format, "a style says the format it sets: {styles}");
    let info = call(&mut s, "app_get_document_info", json!({ "document_id": d }));
    assert!(info["properties"].is_object() && info["tracking_changes"].is_boolean(), "{info}");
}

/// The acceptance of one edit tool (design 8): the read shows `expect` after it, one undo takes it back (and a redo gives it again), and it is still read after saving and opening the file again.
fn check(s: &mut Service, dir: &Path, d: &str, tool: &str, args: Value, read: &dyn Fn(&mut Service, &str) -> Value, expect: Value) {
    let before = read(s, d);
    assert_ne!(before, expect, "{tool}: the edit changes nothing");
    assert_eq!(call(s, tool, args)["can_undo"], true, "{tool}");
    assert_eq!(read(s, d), expect, "{tool}: read after the edit");
    assert_eq!(call(s, "app_undo_document", json!({ "document_id": d }))["undone"], 1, "{tool}");
    assert_eq!(read(s, d), before, "{tool}: one undo takes it back");
    call(s, "app_redo_document", json!({ "document_id": d }));
    assert_eq!(read(s, d), expect, "{tool}: redo");
    let copy = format!("copy_{tool}.docx");
    call(s, "app_save_document", json!({ "document_id": d, "path": dir.join(&copy).to_string_lossy(), "overwrite": true }));
    let y = open(s, dir, &copy);
    assert_eq!(read(s, &y), expect, "{tool}: after saving and opening again");
    call(s, "app_close_document", json!({ "document_id": y }));
}

#[test]
fn mcp_docx_stage_b_step_2_setters_are_read_back_undone_and_saved() {
    let dir = sandbox("docx_b_setters", &["autospace-letters.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "autospace-letters.docx");
    // Paragraph format: the alignment and the space after of paragraph 0.
    let format = |s: &mut Service, d: &str| {
        let f = call(s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["format"].clone();
        json!({ "alignment": f["alignment"], "space_after": f["space_after"], "indent_left": f["indent_left"] })
    };
    let now = format(&mut s, &d);
    let (alignment, space_after) = (if now["alignment"] == "center" { "right" } else { "center" }, now["space_after"].as_f64().unwrap() + 7.0);
    check(&mut s, &dir, &d, "docx_set_paragraph_format", json!({ "document_id": d, "paragraphs": [0], "alignment": alignment, "space_after": space_after, "indent_left": 21 }), &format, json!({ "alignment": alignment, "space_after": space_after, "indent_left": 21.0 }));
    // Page setup: landscape A3 with margins, two columns and vertical text.
    let page = |s: &mut Service, d: &str| {
        let p = call(s, "docx_get_page_setup", json!({ "document_id": d }))["sections"][0].clone();
        json!({ "orientation": p["orientation"], "margins": p["margins"], "columns": p["columns"]["count"], "text_direction": p["text_direction"], "width": p["paper"]["width"].as_f64().map(|w| w.round()) })
    };
    check(
        &mut s,
        &dir,
        &d,
        "docx_set_page_setup",
        json!({ "document_id": d, "paper": "a3", "orientation": "landscape", "margins": { "top": 36, "bottom": 40, "left": 50, "right": 60 }, "columns": { "count": 2 }, "text_direction": "vertical" }),
        &page,
        json!({ "orientation": "landscape", "margins": { "top": 36.0, "bottom": 40.0, "left": 50.0, "right": 60.0 }, "columns": 2, "text_direction": "vertical", "width": 1191.0 }),
    );
    // Properties, tracking and the page color.
    let info = |s: &mut Service, d: &str| {
        let i = call(s, "app_get_document_info", json!({ "document_id": d }));
        json!({ "title": i["properties"]["title"], "author": i["properties"]["author"], "tracking": i["tracking_changes"] })
    };
    check(&mut s, &dir, &d, "docx_set_document_properties", json!({ "document_id": d, "title": "Stage B", "author": "Tester" }), &info, json!({ "title": "Stage B", "author": "Tester", "tracking": false }));
    check(&mut s, &dir, &d, "docx_set_track_changes", json!({ "document_id": d, "value": true }), &info, json!({ "title": "Stage B", "author": "Tester", "tracking": true }));
    let color = |s: &mut Service, d: &str| call(s, "docx_get_page_setup", json!({ "document_id": d }))["page_color"].clone();
    check(&mut s, &dir, &d, "docx_set_page_color", json!({ "document_id": d, "color": "#FFF2CC" }), &color, json!("#FFF2CC"));
    // Wrong arguments are refused and change nothing.
    for (tool, args) in [
        ("docx_set_paragraph_format", json!({ "document_id": d, "paragraphs": [0] })),
        ("docx_set_paragraph_format", json!({ "document_id": d, "paragraphs": [0], "alignment": "diagonal" })),
        ("docx_set_page_setup", json!({ "document_id": d, "paper": "a9" })),
        ("docx_set_page_color", json!({ "document_id": d, "color": "red" })),
    ] {
        assert_eq!(s.call(tool, &args).unwrap_err().code.to_string(), "invalid_argument", "{tool}");
    }
    assert_eq!(s.call("docx_set_paragraph_format", &json!({ "document_id": d, "paragraphs": [100000], "alignment": "left" })).unwrap_err().code.to_string(), "out_of_range");
}

/// Step 3: a list, and the rows, columns and format of a table. Each is read back, one undo takes it back, and it is still there after saving and opening again.
#[test]
fn mcp_docx_stage_b_step_3_lists_and_tables_are_read_back_undone_and_saved() {
    let dir = sandbox("docx_b_structure", &["autospace-letters.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "autospace-letters.docx");
    // A numbered list on paragraph 0, level 1.
    let list = |s: &mut Service, d: &str| call(s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["list"].clone();
    check(&mut s, &dir, &d, "docx_set_numbering", json!({ "document_id": d, "paragraphs": [0], "kind": "number", "format": "upper_roman", "start": 4, "level": 1 }), &|s, d| json!(list(s, d)["level"]), json!(1));
    assert!(list(&mut s, &d)["num_id"].as_u64().unwrap() > 0);
    // A table of 3 rows and 3 columns after the first paragraph's start.
    call(&mut s, "docx_insert_table", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "rows": 3, "columns": 3, "cells": [["a", "b", "c"], ["d", "e", "f"], ["g", "h", "i"]] }));
    let table = |s: &mut Service, d: &str| call(s, "docx_get_table", json!({ "document_id": d, "table": 0 }));
    let shape = |s: &mut Service, d: &str| {
        let t = table(s, d);
        json!({ "rows": t["rows"].as_array().unwrap().len(), "columns": t["column_widths"].as_array().unwrap().len() })
    };
    check(&mut s, &dir, &d, "docx_insert_table_rows", json!({ "document_id": d, "table": 0, "at": 1, "count": 2 }), &shape, json!({ "rows": 5, "columns": 3 }));
    check(&mut s, &dir, &d, "docx_delete_table_rows", json!({ "document_id": d, "table": 0, "at": 0 }), &shape, json!({ "rows": 4, "columns": 3 }));
    check(&mut s, &dir, &d, "docx_insert_table_columns", json!({ "document_id": d, "table": 0, "at": 0, "position": "before" }), &shape, json!({ "rows": 4, "columns": 4 }));
    check(&mut s, &dir, &d, "docx_delete_table_columns", json!({ "document_id": d, "table": 0, "at": 0, "count": 2 }), &shape, json!({ "rows": 4, "columns": 2 }));
    // Merging two cells of the first row into one that spans two grid columns, and splitting it again.
    let cells = |s: &mut Service, d: &str| json!(table(s, d)["cells"][0].as_array().unwrap().iter().map(|c| c["span"].clone()).collect::<Vec<_>>());
    let before = cells(&mut s, &d);
    assert_eq!(before, json!([1, 1]));
    check(&mut s, &dir, &d, "docx_merge_table_cells", json!({ "document_id": d, "table": 0, "row": 0, "cell": 0 }), &cells, json!([2]));
    check(&mut s, &dir, &d, "docx_split_table_cell", json!({ "document_id": d, "table": 0, "row": 0, "cell": 0 }), &cells, json!([1, 1]));
    assert_eq!(s.call("docx_merge_table_cells", &json!({ "document_id": d, "table": 0, "row": 0, "cell": 1 })).unwrap_err().code.to_string(), "out_of_range");
    // The format: alignment and a fill.
    let format = |s: &mut Service, d: &str| {
        let t = table(s, d);
        json!({ "alignment": t["alignment"], "fill": t["cells"][0][0]["fill"] })
    };
    check(&mut s, &dir, &d, "docx_set_table_format", json!({ "document_id": d, "table": 0, "alignment": "center", "cell_fill": { "cells": [{ "row": 0, "cell": 0 }], "color": "#FFC000" } }), &format, json!({ "alignment": "center", "fill": "#FFC000" }));
    // The document has a table of its own, so deleting ours takes the count from two to one.
    let count = |s: &mut Service, d: &str| json!(call(s, "docx_list_tables", json!({ "document_id": d }))["tables"].as_array().unwrap().len());
    let own = count(&mut s, &d).as_u64().unwrap();
    check(&mut s, &dir, &d, "docx_delete_table", json!({ "document_id": d, "table": 0 }), &count, json!(own - 1));
    // Refused and unchanged.
    for (tool, args) in [
        ("docx_set_numbering", json!({ "document_id": d, "paragraphs": [0], "kind": "bullet", "format": "decimal" })),
        ("docx_set_numbering", json!({ "document_id": d, "paragraphs": [0], "kind": "diamond" })),
    ] {
        assert_eq!(s.call(tool, &args).unwrap_err().code.to_string(), "invalid_argument", "{tool}");
    }
}

/// Step 4: a page break, a page number, a field, content controls, the header and footer, comments. Each is read back, one undo takes it back, and it is still there after saving and opening again.
#[test]
fn mcp_docx_stage_b_step_4_insertions_and_comments_are_read_back_undone_and_saved() {
    let dir = sandbox("docx_b_more", &["autospace-letters.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "autospace-letters.docx");
    let at = json!({ "paragraph": 0, "offset": 0 });
    // Content controls: a checkbox and a drop-down list.
    let controls = |s: &mut Service, d: &str| json!(call(s, "docx_list_content_controls", json!({ "document_id": d }))["controls"].as_array().unwrap().iter().map(|c| c["kind"].clone()).collect::<Vec<_>>());
    check(&mut s, &dir, &d, "docx_insert_content_control", json!({ "document_id": d, "at": at, "kind": "checkbox", "checked": true }), &controls, json!(["checkbox"]));
    // A field and the page number show among the paragraph's fields.
    let fields = |s: &mut Service, d: &str| json!(call(s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["fields"].as_array().unwrap().len());
    check(&mut s, &dir, &d, "docx_insert_page_number", json!({ "document_id": d, "at": at }), &fields, json!(1));
    check(&mut s, &dir, &d, "docx_insert_field", json!({ "document_id": d, "at": at, "instruction": "SEQ Figure", "shown": "1" }), &fields, json!(2));
    // The header and the footer text.
    let header = |s: &mut Service, d: &str| call(s, "docx_get_page_setup", json!({ "document_id": d }))["sections"][0]["header"].clone();
    check(&mut s, &dir, &d, "docx_set_header", json!({ "document_id": d, "text": "Stage B header" }), &header, json!("Stage B header"));
    let footer = |s: &mut Service, d: &str| call(s, "docx_get_page_setup", json!({ "document_id": d }))["sections"][0]["footer"].clone();
    check(&mut s, &dir, &d, "docx_set_footer", json!({ "document_id": d, "text": "Stage B footer" }), &footer, json!("Stage B footer"));
    // A comment on the first characters of paragraph 1 (or 0 if the document has one paragraph).
    let comments = |s: &mut Service, d: &str| json!(call(s, "docx_list_comments", json!({ "document_id": d }))["comments"].as_array().unwrap().iter().map(|c| json!({ "text": c["text"], "author": c["author"] })).collect::<Vec<_>>());
    let range = json!({ "from": { "paragraph": 0, "offset": 1 }, "to": { "paragraph": 0, "offset": 3 } });
    check(&mut s, &dir, &d, "docx_add_comment", json!({ "document_id": d, "range": range, "text": "Check this", "author": "Tester" }), &comments, json!([{ "text": "Check this", "author": "Tester" }]));
    let comment = call(&mut s, "docx_list_comments", json!({ "document_id": d }))["comments"][0]["id"].clone();
    check(&mut s, &dir, &d, "docx_delete_comment", json!({ "document_id": d, "id": comment }), &comments, json!([]));
    // Refused.
    assert_eq!(s.call("docx_insert_content_control", &json!({ "document_id": d, "at": at, "kind": "drop_down_list" })).unwrap_err().code.to_string(), "invalid_argument");
    assert_eq!(s.call("docx_add_comment", &json!({ "document_id": d, "range": { "from": at, "to": at }, "text": "x" })).unwrap_err().code.to_string(), "invalid_argument");
}

/// Accepting and rejecting the tracked changes of a document that has an insertion and a deletion (`test/editing/docx-tracked-changes.docx`).
#[test]
fn mcp_docx_apply_revisions_accepts_and_rejects_tracked_changes() {
    let dir = sandbox("docx_b_revisions", &["docx-tracked-changes.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "docx-tracked-changes.docx");
    let state = |s: &mut Service, d: &str| {
        let r = call(s, "docx_list_revisions", json!({ "document_id": d }));
        let kinds: Vec<Value> = r["revisions"].as_array().unwrap().iter().map(|v| v["kind"].clone()).collect();
        json!({ "kinds": kinds, "text": call(s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"] })
    };
    let first = state(&mut s, &d);
    assert_eq!(first["kinds"], json!(["insert", "delete"]), "{first}");
    check(&mut s, &dir, &d, "docx_apply_revisions", json!({ "document_id": d, "accept": true }), &state, json!({ "kinds": [], "text": "Keep added end" }));
    // Rejecting, in a document opened again.
    let r = open(&mut s, &dir, "docx-tracked-changes.docx");
    call(&mut s, "docx_apply_revisions", json!({ "document_id": r, "accept": false }));
    assert_eq!(state(&mut s, &r), json!({ "kinds": [], "text": "Keep removed end" }));
}

/// While changes are tracked, the edit tools write tracked changes: they show in docx_list_revisions, are accepted or rejected, undone and saved.
#[test]
fn mcp_docx_edits_while_tracking_are_tracked_changes() {
    let dir = sandbox("docx_b_tracked", &["autospace-letters.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "autospace-letters.docx");
    let original = call(&mut s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"].as_str().unwrap().to_string();
    assert!(original.chars().count() > 6, "{original}");
    call(&mut s, "docx_set_track_changes", json!({ "document_id": d, "value": true }));
    let revisions = |s: &mut Service, d: &str| {
        let r = call(s, "docx_list_revisions", json!({ "document_id": d }));
        json!(r["revisions"].as_array().unwrap().iter().map(|v| json!({ "kind": v["kind"], "author": v["author"], "text": v["text"] })).collect::<Vec<_>>())
    };
    assert_eq!(revisions(&mut s, &d), json!([]));
    // An insertion.
    check(&mut s, &dir, &d, "docx_insert_text", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "text": "NEW", "author": "Tester" }), &revisions, json!([{ "kind": "insert", "author": "Tester", "text": "NEW" }]));
    // A deletion of the original's first characters (after the inserted ones: offset 3 to 5).
    let removed: String = original.chars().take(2).collect();
    check(
        &mut s,
        &dir,
        &d,
        "docx_delete_text",
        json!({ "document_id": d, "range": { "from": { "paragraph": 0, "offset": 3 }, "to": { "paragraph": 0, "offset": 5 } }, "author": "Tester" }),
        &revisions,
        json!([{ "kind": "insert", "author": "Tester", "text": "NEW" }, { "kind": "delete", "author": "Tester", "text": removed }]),
    );
    // Rejecting every change brings the original text back.
    call(&mut s, "docx_apply_revisions", json!({ "document_id": d, "accept": false }));
    assert_eq!(call(&mut s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"], original.as_str());
    // A format change is a tracked change as well.
    check(&mut s, &dir, &d, "docx_set_run_format", json!({ "document_id": d, "range": { "from": { "paragraph": 0, "offset": 0 }, "to": { "paragraph": 0, "offset": 2 } }, "bold": true, "author": "Tester" }), &|s, d| json!(call(s, "docx_list_revisions", json!({ "document_id": d }))["revisions"].as_array().unwrap().iter().map(|v| v["kind"].clone()).collect::<Vec<_>>()), json!(["format"]));
    // A replacement is a deletion and an insertion; accepting leaves the new text only.
    let find: String = original.chars().skip(2).take(3).collect();
    call(&mut s, "docx_replace_text", json!({ "document_id": d, "find": find, "replace": "XYZ", "author": "Tester" }));
    let kinds = |s: &mut Service, d: &str| json!(call(s, "docx_list_revisions", json!({ "document_id": d }))["revisions"].as_array().unwrap().iter().map(|v| v["kind"].clone()).collect::<Vec<_>>());
    let now = kinds(&mut s, &d);
    assert!(now.as_array().unwrap().iter().any(|k| k == "delete") && now.as_array().unwrap().iter().any(|k| k == "insert"), "{now}");
    call(&mut s, "docx_apply_revisions", json!({ "document_id": d, "accept": true }));
    assert_eq!(kinds(&mut s, &d), json!([]));
    assert!(call(&mut s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 0 }))["text"].as_str().unwrap().contains("XYZ"));
    // Without tracking nothing is tracked (the earlier behaviour).
    call(&mut s, "docx_set_track_changes", json!({ "document_id": d, "value": false }));
    call(&mut s, "docx_insert_text", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "text": "Q" }));
    assert_eq!(kinds(&mut s, &d), json!([]));
}

/// Step 5: text boxes and shapes, their properties, a picture's properties, the watermark. Each is read back, one undo takes it back, and it is still there after saving and opening again.
#[test]
fn mcp_docx_stage_b_step_5_shapes_pictures_and_watermark_are_read_back_undone_and_saved() {
    let dir = sandbox("docx_b_objects", &["autospace-letters.docx", "picture-120x60.png"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "autospace-letters.docx");
    let at = json!({ "paragraph": 0, "offset": 0 });
    let shapes = |s: &mut Service, d: &str| {
        json!(call(s, "docx_list_shapes", json!({ "document_id": d }))["shapes"].as_array().unwrap().iter().map(|v| json!({ "kind": v["kind"], "w": v["frame"]["width"].as_f64().map(|w| w.round()), "h": v["frame"]["height"].as_f64().map(|w| w.round()), "text": v["text"] })).collect::<Vec<_>>())
    };
    check(&mut s, &dir, &d, "docx_insert_text_box", json!({ "document_id": d, "at": at, "width": 120, "height": 60, "text": "Box" }), &shapes, json!([{ "kind": "shape", "w": 120.0, "h": 60.0, "text": "Box" }]));
    // A text box is a shape that holds text in the model, so it is listed as a shape. Its properties:
    let box_state = |s: &mut Service, d: &str| {
        let v = call(s, "docx_list_shapes", json!({ "document_id": d }))["shapes"][0].clone();
        json!({ "w": v["frame"]["width"].as_f64().map(|w| w.round()), "rotation": v["rotation"].as_f64().map(|r| r.round()), "text": v["text"] })
    };
    check(&mut s, &dir, &d, "docx_set_shape_properties", json!({ "document_id": d, "at": at, "width": 200, "rotation": 30, "text": "Changed" }), &box_state, json!({ "w": 200.0, "rotation": 30.0, "text": "Changed" }));
    // A shape with a fill and a line.
    let first = |s: &mut Service, d: &str| json!(call(s, "docx_list_shapes", json!({ "document_id": d }))["shapes"].as_array().unwrap().iter().map(|v| json!({ "kind": v["kind"], "fill": v["fill"], "line": v["line"]["color"] })).collect::<Vec<_>>());
    let before = first(&mut s, &d);
    call(&mut s, "docx_insert_shape", json!({ "document_id": d, "at": at, "kind": "ellipse", "width": 50, "height": 50, "fill": "#FFC000", "line": { "color": "#1F4E79", "width": 2 } }));
    let after = first(&mut s, &d);
    assert_eq!(after.as_array().unwrap().len(), before.as_array().unwrap().len() + 1, "{after}");
    assert!(after.to_string().contains("#FFC000") && after.to_string().contains("#1F4E79"), "{after}");
    // The watermark.
    let mark = |s: &mut Service, d: &str| call(s, "docx_get_page_setup", json!({ "document_id": d }))["watermark"]["text"].clone();
    check(&mut s, &dir, &d, "docx_set_watermark", json!({ "document_id": d, "text": "DRAFT" }), &mark, json!("DRAFT"));
    // A picture's size and alternative text (the picture goes in paragraph 1).
    call(&mut s, "docx_insert_picture", json!({ "document_id": d, "at": { "paragraph": 1, "offset": 0 }, "path": dir.join("picture-120x60.png").to_string_lossy(), "width": 120 }));
    let picture = |s: &mut Service, d: &str| {
        let v = call(s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 1 }))["pictures"][0].clone();
        json!({ "w": v["width"].as_f64().map(|w| w.round()), "h": v["height"].as_f64().map(|w| w.round()), "alt": v["alt_text"]["description"] })
    };
    check(&mut s, &dir, &d, "docx_set_picture_properties", json!({ "document_id": d, "at": { "paragraph": 1, "offset": 0 }, "width": 60, "alt_text": { "description": "A test picture" } }), &picture, json!({ "w": 60.0, "h": 30.0, "alt": "A test picture" }));
    // Refused.
    for (tool, args) in [
        ("docx_insert_shape", json!({ "document_id": d, "at": at, "kind": "star", "width": 10, "height": 10 })),
        ("docx_set_picture_properties", json!({ "document_id": d, "at": { "paragraph": 1, "offset": 0 } })),
    ] {
        assert_eq!(s.call(tool, &args).unwrap_err().code.to_string(), "invalid_argument", "{tool}");
    }
    assert_eq!(s.call("docx_set_shape_properties", &json!({ "document_id": d, "at": { "paragraph": 2, "offset": 0 }, "width": 10 })).unwrap_err().code.to_string(), "out_of_range");
}

/// Step 6: a chart, a table of contents, the wrap of a picture.
#[test]
fn mcp_docx_stage_b_step_6_chart_contents_and_wrap_are_read_back_undone_and_saved() {
    let dir = sandbox("docx_b_layout", &["docx-report.docx", "picture-120x60.png"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let d = open(&mut s, &dir, "docx-report.docx");
    // A chart: listed with its frame, one more after the insertion, and still there after saving.
    let charts = |s: &mut Service, d: &str| json!(call(s, "docx_list_charts", json!({ "document_id": d }))["charts"].as_array().unwrap().len());
    let before = charts(&mut s, &d).as_u64().unwrap();
    check(&mut s, &dir, &d, "docx_insert_chart", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "kind": "pie", "width": 300, "height": 200 }), &charts, json!(before + 1));
    // A chart of the numbers given, with a title: its series and title are read back, and are still there after saving.
    let sales = |s: &mut Service, d: &str| {
        let c = call(s, "docx_list_charts", json!({ "document_id": d }))["charts"].as_array().unwrap().iter().filter(|c| c["title"] == "Sales").cloned().collect::<Vec<_>>();
        json!(c.iter().map(|c| c["series"].clone()).collect::<Vec<_>>())
    };
    check(
        &mut s,
        &dir,
        &d,
        "docx_insert_chart",
        json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "kind": "column", "title": "Sales", "legend": "bottom", "data": [["", "East", "West"], ["Q1", 10, 20], ["Q2", 30, 40], ["Q3", 50, 60]] }),
        &sales,
        json!([2]),
    );
    assert_eq!(s.call("docx_insert_chart", &json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "kind": "pie", "data": [["a", "b"], ["x", "y"]] })).unwrap_err().code.to_string(), "invalid_argument");
    // The data of that chart replaced: two series become three (the chart keeps its title), one undo step, kept by saving.
    let place = {
        let list = call(&mut s, "docx_list_charts", json!({ "document_id": d }));
        let c = list["charts"].as_array().unwrap().iter().find(|c| c["title"] == "Sales").unwrap().clone();
        json!({ "paragraph": c["paragraph"], "offset": c["offset"] })
    };
    check(
        &mut s,
        &dir,
        &d,
        "docx_set_chart_data",
        json!({ "document_id": d, "at": place, "series_in": "columns", "data": [["", "East", "West", "North"], ["Q1", 1, 2, 3], ["Q2", 4, 5, 6]] }),
        &sales,
        json!([3]),
    );
    assert_eq!(s.call("docx_set_chart_data", &json!({ "document_id": d, "at": { "paragraph": 1, "offset": 0 }, "data": [["", "a"], ["x", 1]] })).unwrap_err().code.to_string(), "no_such_shape");
    // A table of contents: the headings of the report, before paragraph 0.
    let contents = |s: &mut Service, d: &str| json!(call(s, "docx_list_paragraphs", json!({ "document_id": d, "limit": 100000 }))["total"]);
    let total = contents(&mut s, &d).as_u64().unwrap();
    let r = call(&mut s, "docx_set_table_of_contents", json!({ "document_id": d, "paragraph": 0, "from": 1, "to": 3 }));
    assert_eq!(r["warnings"][0]["code"], "no_page_numbers", "{r}");
    assert!(contents(&mut s, &d).as_u64().unwrap() > total, "the entries are paragraphs now");
    call(&mut s, "app_undo_document", json!({ "document_id": d }));
    assert_eq!(contents(&mut s, &d).as_u64().unwrap(), total, "one undo takes the table of contents back");
    // A picture's wrap: inline, then square (a floating shape afterwards, at the same place).
    call(&mut s, "docx_insert_picture", json!({ "document_id": d, "at": { "paragraph": 1, "offset": 0 }, "path": dir.join("picture-120x60.png").to_string_lossy(), "width": 120 }));
    let wrap = |s: &mut Service, d: &str| {
        let shapes = call(s, "docx_list_shapes", json!({ "document_id": d }))["shapes"].clone();
        let pic = call(s, "docx_get_paragraph", json!({ "document_id": d, "paragraph": 1 }))["pictures"][0]["wrap"].clone();
        json!({ "inline": pic, "floating": shapes.as_array().unwrap().iter().filter(|v| v["paragraph"] == 1).map(|v| v["wrap"].clone()).collect::<Vec<_>>() })
    };
    check(&mut s, &dir, &d, "docx_set_picture_wrap", json!({ "document_id": d, "at": { "paragraph": 1, "offset": 0 }, "wrap": "square" }), &wrap, json!({ "inline": null, "floating": ["square"] }));
    // Refused.
    assert_eq!(s.call("docx_insert_chart", &json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "kind": "venn" })).unwrap_err().code.to_string(), "invalid_argument");
    assert_eq!(s.call("docx_set_picture_wrap", &json!({ "document_id": d, "at": { "paragraph": 0, "offset": 3 }, "wrap": "square" })).unwrap_err().code.to_string(), "out_of_range");
    assert_eq!(s.call("docx_set_table_of_contents", &json!({ "document_id": d, "paragraph": 0, "from": 3, "to": 1 })).unwrap_err().code.to_string(), "invalid_argument");
}
