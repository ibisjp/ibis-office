// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #159, stage B (xlsx): the reads and the edit tools added after stage A. Each edit makes one undo step, is read back, and is still there after saving and opening the file again.

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
    fn blank(&mut self, name: &str) -> (String, String) {
        let bytes = ibis_formats::blank_file(ibis_formats::Kind::Workbook).unwrap();
        std::fs::write(self.dir.join(name), bytes).unwrap();
        let x = self.open(name);
        let sheet = self.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"][0]["name"].as_str().unwrap().to_string();
        (x, sheet)
    }

    /// The acceptance of one edit tool (design 8): the read shows `expect` after it, one undo takes it back (and a redo gives it again), and it is still read after saving and opening the file again.
    fn check(&mut self, x: &str, tool: &str, args: Value, read: &dyn Fn(&mut T, &str) -> Value, expect: Value) {
        let before = read(self, x);
        assert_ne!(before, expect, "{tool}: the edit changes nothing");
        let r = self.call(tool, args);
        assert_eq!(r["can_undo"], true, "{tool}");
        assert_eq!(read(self, x), expect, "{tool}: read after the edit");
        assert_eq!(self.call("app_undo_document", json!({ "document_id": x }))["undone"], 1, "{tool}");
        assert_eq!(read(self, x), before, "{tool}: one undo takes it back");
        self.call("app_redo_document", json!({ "document_id": x }));
        assert_eq!(read(self, x), expect, "{tool}: redo");
        let copy = format!("copy_{tool}.xlsx");
        self.call("app_save_document", json!({ "document_id": x, "path": self.dir.join(&copy).to_string_lossy(), "overwrite": true }));
        let y = self.open(&copy);
        assert_eq!(read(self, &y), expect, "{tool}: after saving and opening again");
        self.call("app_close_document", json!({ "document_id": y }));
    }

    #[allow(dead_code)]
    fn reopen(&mut self, id: &str, name: &str) -> String {
        let copy = format!("copy_{name}");
        self.call("app_save_document", json!({ "document_id": id, "path": self.dir.join(&copy).to_string_lossy(), "overwrite": true }));
        self.open(&copy)
    }
}

#[test]
fn mcp_xlsx_reads_of_stage_b_show_what_the_edit_tools_change() {
    let mut t = T::new("xlsx_b_reads", &["xlsx-table.xlsx", "xlsx-formats.xlsx", "xlsx-print-titles.xlsx", "xlsx-sales.xlsx"]);
    // View, names, conditional formats, hidden columns and the format of a cell (colours as references).
    let f = t.open("xlsx-formats.xlsx");
    let sheet = t.call("xlsx_list_sheets", json!({ "document_id": f }))["sheets"][0]["name"].as_str().unwrap().to_string();
    let view = t.call("xlsx_get_sheet_view", json!({ "document_id": f, "sheet": sheet }));
    assert_eq!((view["freeze_panes"].as_str(), view["zoom"].as_u64(), view["gridlines"].as_bool()), (Some("B2"), Some(100), Some(true)));
    assert_eq!(t.call("xlsx_get_used_range", json!({ "document_id": f, "sheet": sheet }))["hidden_columns"], json!(["D"]));
    let names = t.call("xlsx_list_names", json!({ "document_id": f }));
    assert_eq!((names["names"][0]["name"].as_str(), names["names"][0]["scope"].as_str()), (Some("売上一覧"), Some("workbook")));
    assert!(names["names"][0]["refers_to"].as_str().unwrap().starts_with('='));
    let cf = t.call("xlsx_list_conditional_formats", json!({ "document_id": f, "sheet": sheet }));
    assert_eq!((cf["conditional_formats"][0]["ranges"][0].as_str(), cf["conditional_formats"][0]["rules"][0]["type"].as_str(), cf["conditional_formats"][0]["rules"][0]["operator"].as_str()), (Some("B2:B19"), Some("cell_value"), Some("greater_than")));
    assert_eq!(cf["conditional_formats"][0]["rules"][0]["format"]["fill"], "#FFEB9C");
    let format = t.call("xlsx_get_cells", json!({ "document_id": f, "sheet": sheet, "range": "A1", "kind": "format" }))["rows"][0][0].clone();
    assert_eq!(format["color"]["theme"], "text1");
    assert_eq!(format["borders"]["top"], Value::Null);
    assert_eq!(t.call("xlsx_list_sheets", json!({ "document_id": f }))["sheets"].as_array().unwrap().len(), 2);
    // Tables and their borders.
    let tb = t.open("xlsx-table.xlsx");
    let tables = t.call("xlsx_list_tables", json!({ "document_id": tb }));
    assert_eq!((tables["tables"][0]["name"].as_str(), tables["tables"][0]["range"].as_str(), tables["tables"][0]["style"]["name"].as_str()), (Some("Table1"), Some("A1:C6"), Some("TableStyleMedium2")));
    let sheet = tables["tables"][0]["sheet"].as_str().unwrap().to_string();
    let top = t.call("xlsx_get_cells", json!({ "document_id": tb, "sheet": sheet, "range": "A1", "kind": "format" }))["rows"][0][0]["borders"]["top"].clone();
    assert_eq!((top["style"].as_str(), top["color"].as_str()), (Some("thin"), Some("#46B1E1")));
    // Page setup and print titles.
    let p = t.open("xlsx-print-titles.xlsx");
    let sheet = t.call("xlsx_list_sheets", json!({ "document_id": p }))["sheets"][0]["name"].as_str().unwrap().to_string();
    let setup = t.call("xlsx_get_page_setup", json!({ "document_id": p, "sheet": sheet }));
    assert_eq!((setup["paper"]["name"].as_str(), setup["orientation"].as_str(), setup["scale"].as_u64()), (Some("a4"), Some("portrait"), Some(100)));
    assert!(setup["print_titles"].as_str().unwrap().ends_with("$1:$1"));
    assert_eq!(setup["header_footer"]["footer"], "&C&P / &N");
    // Charts, and the document info a workbook has.
    let s = t.open("xlsx-sales.xlsx");
    let charts = t.call("xlsx_list_charts", json!({ "document_id": s }));
    assert_eq!((charts["charts"][0]["kind"].as_str(), charts["charts"][0]["title"].as_str(), charts["charts"][0]["series"].as_array().map(Vec::len)), (Some("column"), Some("店舗別の売上"), Some(3)));
    assert_eq!(t.call("xlsx_list_charts", json!({ "document_id": s, "sheet": "商品" }))["charts"], json!([]));
    assert_eq!(t.err("xlsx_list_charts", json!({ "document_id": s, "sheet": "none" })), "no_such_sheet");
    let info = t.call("app_get_document_info", json!({ "document_id": s }));
    assert_eq!(info["calculation"]["mode"], "automatic");
    for tool in ["xlsx_list_validations", "xlsx_list_comments", "xlsx_list_hyperlinks", "xlsx_list_cell_styles"] {
        let args = if tool == "xlsx_list_validations" { json!({ "document_id": s, "sheet": "商品" }) } else { json!({ "document_id": s }) };
        t.call(tool, args);
    }
    assert_eq!(t.call("app_get_document_info", json!({ "document_id": s }))["dirty"], false);
}

fn border(t: &mut T, x: &str, sheet: &str, cell: &str, edge: &str) -> Value {
    t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": cell, "kind": "format" }))["rows"][0][0]["borders"][edge].clone()
}

fn format_of(t: &mut T, x: &str, sheet: &str, cell: &str, key: &str) -> Value {
    t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": cell, "kind": "format" }))["rows"][0][0][key].clone()
}

#[test]
fn mcp_xlsx_borders_and_format_attributes_are_read_back() {
    let mut t = T::new("xlsx_b_format", &[]);
    let (x, sheet) = t.blank("blank.xlsx");
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "B2:C3", "values": [[1, 2], [3, 4]] }));
    let s = sheet.clone();
    // Outline: the four outer edges of B2:C3 only.
    let read = move |t: &mut T, x: &str| json!([border(t, x, &s, "B2", "top"), border(t, x, &s, "B2", "left"), border(t, x, &s, "C3", "bottom"), border(t, x, &s, "C3", "right"), border(t, x, &s, "B2", "bottom")]);
    let thick = json!({ "style": "thick", "color": "#FF0000" });
    t.check(&x, "xlsx_set_borders", json!({ "document_id": x, "sheet": sheet, "range": "B2:C3", "edges": ["outline"], "style": "thick", "color": "#FF0000" }), &read, json!([thick, thick, thick, thick, null]));
    // Inside lines, and all with the default style.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| json!([border(t, x, &s, "B2", "bottom"), border(t, x, &s, "C2", "left"), border(t, x, &s, "B3", "top")]);
    let thin = json!({ "style": "thin", "color": "#000000" });
    t.check(&x, "xlsx_set_borders", json!({ "document_id": x, "sheet": sheet, "range": "B2:C3", "edges": ["inside_horizontal", "inside_vertical"] }), &read, json!([thin, thin, thin]));
    // none removes.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| border(t, x, &s, "B2", "top");
    t.check(&x, "xlsx_set_borders", json!({ "document_id": x, "sheet": sheet, "range": "B2", "edges": ["top"], "style": "none" }), &read, Value::Null);
    assert_eq!(t.err("xlsx_set_borders", json!({ "document_id": x, "sheet": sheet, "range": "B2", "style": "wavy" })), "invalid_argument");
    assert_eq!(t.err("xlsx_set_borders", json!({ "document_id": x, "sheet": sheet, "range": "B2", "edges": ["middle"] })), "invalid_argument");
    // The attributes xlsx_set_format got.
    for (key, value) in [("strike", json!(true)), ("indent", json!(2)), ("text_rotation", json!(45)), ("shrink_to_fit", json!(true)), ("locked", json!(false)), ("hidden", json!(true))] {
        let s = sheet.clone();
        let k = key.to_string();
        let read = move |t: &mut T, x: &str| format_of(t, x, &s, "B2", &k);
        t.check(&x, "xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "B2", key: value.clone() }), &read, value);
    }
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| format_of(t, x, &s, "C2", "text_rotation");
    t.check(&x, "xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "C2", "text_rotation": -90 }), &read, json!(-90));
    assert_eq!(t.err("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "B2", "indent": 99 })), "invalid_argument");
    assert_eq!(t.err("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "B2", "style": "No Such Style" })), "invalid_argument");
}

#[test]
fn mcp_xlsx_sheets_rows_columns_and_view_are_set_and_read() {
    let mut t = T::new("xlsx_b_view", &[]);
    let (x, sheet) = t.blank("blank.xlsx");
    t.call("xlsx_add_sheet", json!({ "document_id": x, "name": "Second" }));
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:A5", "values": [[1], [2], [3], [4], [5]] }));
    t.call("xlsx_set_size", json!({ "document_id": x, "sheet": sheet, "axis": "row", "at": 1, "count": 6, "size": 15 }));
    t.call("xlsx_set_size", json!({ "document_id": x, "sheet": sheet, "axis": "column", "at": "A", "count": 6, "size": 60 }));
    // Hidden sheets.
    let read = |t: &mut T, x: &str| t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"].as_array().unwrap().iter().map(|s| s["state"].clone()).collect::<Vec<_>>().into();
    t.check(&x, "xlsx_hide_sheet", json!({ "document_id": x, "sheet": "Second" }), &read, json!(["visible", "hidden"]));
    assert_eq!(t.err("xlsx_hide_sheet", json!({ "document_id": x, "sheet": sheet })), "unsupported_operation", "the last shown sheet stays");
    t.check(&x, "xlsx_show_sheet", json!({ "document_id": x, "sheet": "Second" }), &read, json!(["visible", "visible"]));
    // Rows and columns.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| t.call("xlsx_get_used_range", json!({ "document_id": x, "sheet": s }))["hidden_rows"].clone();
    t.check(&x, "xlsx_hide_rows", json!({ "document_id": x, "sheet": sheet, "at": 2, "count": 2 }), &read, json!(["2:3"]));
    assert_eq!(t.err("xlsx_hide_rows", json!({ "document_id": x, "sheet": sheet, "at": 2 })), "invalid_argument");
    t.check(&x, "xlsx_show_rows", json!({ "document_id": x, "sheet": sheet, "at": 2, "count": 2 }), &read, json!([]));
    assert_eq!(t.err("xlsx_hide_rows", json!({ "document_id": x, "sheet": sheet, "at": 2_000_000 })), "invalid_argument");
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| t.call("xlsx_get_used_range", json!({ "document_id": x, "sheet": s }))["hidden_columns"].clone();
    t.check(&x, "xlsx_hide_columns", json!({ "document_id": x, "sheet": sheet, "at": "B" }), &read, json!(["B"]));
    t.check(&x, "xlsx_show_columns", json!({ "document_id": x, "sheet": sheet, "at": 2 }), &read, json!([]));
    // Freeze panes and the view.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| t.call("xlsx_get_sheet_view", json!({ "document_id": x, "sheet": s }))["freeze_panes"].clone();
    t.check(&x, "xlsx_set_freeze_panes", json!({ "document_id": x, "sheet": sheet, "cell": "B3" }), &read, json!("B3"));
    t.check(&x, "xlsx_set_freeze_panes", json!({ "document_id": x, "sheet": sheet, "cell": null }), &read, Value::Null);
    assert_eq!(t.err("xlsx_set_freeze_panes", json!({ "document_id": x, "sheet": sheet, "cell": "B2:C3" })), "invalid_argument");
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| {
        let v = t.call("xlsx_get_sheet_view", json!({ "document_id": x, "sheet": s }));
        json!([v["gridlines"], v["headings"], v["zoom"], v["tab_color"], v["show_formulas"], v["show_zeros"]])
    };
    t.check(&x, "xlsx_set_sheet_view", json!({ "document_id": x, "sheet": sheet, "gridlines": false, "headings": false, "zoom": 150, "tab_color": "#00AA00", "show_formulas": true, "show_zeros": false }), &read, json!([false, false, 150, "#00AA00", true, false]));
    assert_eq!(t.err("xlsx_set_sheet_view", json!({ "document_id": x, "sheet": sheet, "zoom": 5 })), "invalid_argument");
    assert_eq!(t.err("xlsx_set_sheet_view", json!({ "document_id": x, "sheet": sheet })), "invalid_argument");
}

#[test]
fn mcp_xlsx_page_setup_protection_calculation_and_properties() {
    let mut t = T::new("xlsx_b_book", &[]);
    let (x, sheet) = t.blank("blank.xlsx");
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:B3", "values": [[1, 2], [3, 4], [5, 6]] }));
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| {
        let p = t.call("xlsx_get_page_setup", json!({ "document_id": x, "sheet": s }));
        json!([p["paper"]["name"], p["orientation"], p["margins"]["left"], p["scale"], p["print_area"], p["header_footer"]["footer"], p["center_horizontally"]])
    };
    t.check(&x, "xlsx_set_page_setup", json!({ "document_id": x, "sheet": sheet, "paper": "a3", "orientation": "landscape", "margins": { "left": 36 }, "scale": 80, "print_area": "A1:B3", "footer": "&C&P", "center_horizontally": true }), &read, json!(["a3", "landscape", 36.0, 80, "A1:B3", "&C&P", true]));
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| {
        let p = t.call("xlsx_get_page_setup", json!({ "document_id": x, "sheet": s }));
        json!([p["fit_to"], p["print_titles"].as_str().map(|s| s.split('!').last().unwrap_or("").to_string())])
    };
    t.check(&x, "xlsx_set_page_setup", json!({ "document_id": x, "sheet": sheet, "fit_to": { "pages_wide": 1, "pages_tall": 0 }, "print_titles": "$1:$1" }), &read, json!([{ "pages_wide": 1, "pages_tall": 0 }, "$1:$1"]));
    assert_eq!(t.err("xlsx_set_page_setup", json!({ "document_id": x, "sheet": sheet, "paper": "tabloid-ish" })), "invalid_argument");
    assert_eq!(t.err("xlsx_set_page_setup", json!({ "document_id": x, "sheet": sheet, "print_titles": "A1" })), "invalid_argument");
    assert_eq!(t.err("xlsx_set_page_setup", json!({ "document_id": x, "sheet": sheet, "scale": 1000 })), "invalid_argument");
    // Protection: no password is taken.
    let read = |t: &mut T, x: &str| {
        let l = t.call("xlsx_list_sheets", json!({ "document_id": x }));
        json!([l["sheets"][0]["protected"], l["sheets"][0]["allowed"]["sort"]])
    };
    t.check(&x, "xlsx_set_protection", json!({ "document_id": x, "sheet": sheet, "value": true, "allow": { "sort": true } }), &read, json!([true, true]));
    assert_eq!(t.err("xlsx_set_protection", json!({ "document_id": x, "sheet": sheet, "value": true, "password": "secret" })), "invalid_argument");
    let read = |t: &mut T, x: &str| t.call("xlsx_list_sheets", json!({ "document_id": x }))["workbook_protected"].clone();
    t.check(&x, "xlsx_set_protection", json!({ "document_id": x, "value": true }), &read, json!(true));
    // Calculation.
    let read = |t: &mut T, x: &str| {
        let c = t.call("app_get_document_info", json!({ "document_id": x }))["calculation"].clone();
        json!([c["mode"], c["iterative"], c["max_iterations"]])
    };
    t.check(&x, "xlsx_set_calculation", json!({ "document_id": x, "mode": "manual", "iterative": true, "max_iterations": 50 }), &read, json!(["manual", true, 50]));
    assert_eq!(t.err("xlsx_set_calculation", json!({ "document_id": x, "mode": "sometimes" })), "invalid_argument");
    // Properties.
    let read = |t: &mut T, x: &str| {
        let p = t.call("app_get_document_info", json!({ "document_id": x }))["properties"].clone();
        json!([p["title"], p["author"], p["keywords"]])
    };
    t.check(&x, "xlsx_set_document_properties", json!({ "document_id": x, "title": "Plan", "author": "Ibis", "keywords": "a, b" }), &read, json!(["Plan", "Ibis", "a, b"]));
    assert_eq!(t.err("xlsx_set_document_properties", json!({ "document_id": x })), "invalid_argument");
}

fn cells(t: &mut T, x: &str, sheet: &str, range: &str) -> Value {
    t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": range }))["rows"].clone()
}

#[test]
fn mcp_xlsx_copy_move_replace_and_sheets() {
    let mut t = T::new("xlsx_b_cells", &[]);
    let (x, sheet) = t.blank("blank.xlsx");
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:B2", "values": [["Apple pie", 2], ["apple", "=B1*10"]] }));
    t.call("xlsx_set_format", json!({ "document_id": x, "sheet": sheet, "range": "A1", "bold": true }));
    // Copy: values, the relative formula moves, the format comes along.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| cells(t, x, &s, "D1:E2");
    t.check(&x, "xlsx_copy_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:B2", "to": "D1" }), &read, json!([["Apple pie", 2.0], ["apple", 20.0]]));
    assert_eq!(t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": sheet, "range": "E2", "kind": "formula" }))["rows"], json!([["=E1*10"]]));
    assert_eq!(format_of(&mut t, &x, &sheet, "D1", "bold"), json!(true));
    // Values only: no formula, no format.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| t.call("xlsx_get_cells", json!({ "document_id": x, "sheet": s, "range": "E3", "kind": "formula" }))["rows"].clone();
    t.check(&x, "xlsx_copy_cells", json!({ "document_id": x, "sheet": sheet, "range": "B2", "to": "E3", "what": "values" }), &read, json!([[20.0]]));
    assert_eq!(t.err("xlsx_copy_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1", "to": "A1:B2" })), "invalid_argument");
    assert_eq!(t.err("xlsx_copy_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1", "to": "B1", "what": "everything" })), "invalid_argument");
    // Move.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| json!([cells(t, x, &s, "G1"), cells(t, x, &s, "A1")]);
    t.check(&x, "xlsx_move_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1", "to": "G1" }), &read, json!([[["Apple pie"]], [[null]]]));
    // Replace text: constants only, case folded.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| cells(t, x, &s, "G1:G1").as_array().unwrap().iter().chain(cells(t, x, &s, "A2:A2").as_array().unwrap().iter()).cloned().collect::<Vec<_>>().into();
    t.check(&x, "xlsx_replace_text", json!({ "document_id": x, "find": "APPLE", "replace": "Pear" }), &read, json!([["Pear pie"], ["Pear"]]));
    assert_eq!(t.err("xlsx_replace_text", json!({ "document_id": x, "find": "", "replace": "x" })), "invalid_argument");
    // Copy a sheet.
    let read = |t: &mut T, x: &str| t.call("xlsx_list_sheets", json!({ "document_id": x }))["sheets"].as_array().unwrap().iter().map(|s| s["name"].clone()).collect::<Vec<_>>().into();
    t.check(&x, "xlsx_copy_sheet", json!({ "document_id": x, "sheet": sheet, "name": "Copy" }), &read, json!([sheet.clone(), "Copy"]));
    assert_eq!(t.err("xlsx_copy_sheet", json!({ "document_id": x, "sheet": sheet, "name": "Copy" })), "invalid_argument");
    assert_eq!(cells(&mut t, &x, "Copy", "G1"), json!([["Pear pie"]]));
}

#[test]
fn mcp_xlsx_names_validation_conditional_filter_table_links_comments() {
    let mut t = T::new("xlsx_b_struct", &[]);
    let (x, sheet) = t.blank("blank.xlsx");
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:B6", "values": [["Item", "Qty"], ["a", 5], ["b", 1], ["a", 5], ["c", 9], ["b", 1]] }));
    // Names.
    let read = |t: &mut T, x: &str| t.call("xlsx_list_names", json!({ "document_id": x }))["names"].as_array().unwrap().iter().map(|n| json!([n["name"], n["refers_to"], n["scope"]])).collect::<Vec<_>>().into();
    let refers = format!("={sheet}!$B$2:$B$6");
    t.check(&x, "xlsx_add_name", json!({ "document_id": x, "name": "Quantities", "refers_to": refers }), &read, json!([["Quantities", refers, "workbook"]]));
    assert_eq!(t.err("xlsx_add_name", json!({ "document_id": x, "name": "Quantities", "refers_to": "=1" })), "invalid_argument");
    assert_eq!(t.err("xlsx_add_name", json!({ "document_id": x, "name": "A1", "refers_to": "=1" })), "invalid_argument");
    t.check(&x, "xlsx_delete_name", json!({ "document_id": x, "name": "Quantities" }), &read, json!([]));
    // Validation.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| t.call("xlsx_list_validations", json!({ "document_id": x, "sheet": s }))["validations"].clone();
    let list = |t: &mut T, x: &str| {
        let v = read(t, x);
        v.as_array().unwrap().iter().map(|r| json!([r["ranges"], r["kind"], r["formula1"], r["ignore_blank"]])).collect::<Vec<_>>().into()
    };
    t.check(&x, "xlsx_set_validation", json!({ "document_id": x, "sheet": sheet, "range": "D1:E1", "kind": "list", "items": ["x", "y"], "ignore_blank": false }), &list, json!([[["D1:E1"], "list", "\"x,y\"", false]]));
    t.check(&x, "xlsx_set_validation", json!({ "document_id": x, "sheet": sheet, "range": "B2:B6", "kind": "whole", "operator": "between", "formula1": "1", "formula2": "10", "error": { "style": "warning", "text": "1 to 10" } }), &list, json!([[["D1:E1"], "list", "\"x,y\"", false], [["B2:B6"], "whole", "1", true]]));
    assert_eq!(t.err("xlsx_set_validation", json!({ "document_id": x, "sheet": sheet, "range": "B2", "kind": "whole", "formula1": "1" })), "invalid_argument");
    assert_eq!(t.err("xlsx_set_validation", json!({ "document_id": x, "sheet": sheet, "range": "B2", "kind": "list" })), "invalid_argument");
    t.check(&x, "xlsx_set_validation", json!({ "document_id": x, "sheet": sheet, "range": "D1:E1", "kind": null }), &list, json!([[["B2:B6"], "whole", "1", true]]));
    // Conditional formats.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| {
        let v = t.call("xlsx_list_conditional_formats", json!({ "document_id": x, "sheet": s }))["conditional_formats"].clone();
        v.as_array().unwrap().iter().map(|f| json!([f["ranges"], f["rules"][0]["type"], f["rules"][0]["format"]["fill"]])).collect::<Vec<_>>().into()
    };
    t.check(&x, "xlsx_add_conditional_format", json!({ "document_id": x, "sheet": sheet, "range": "B2:B6", "rule": { "type": "cell_value", "operator": "greater_than", "formula1": "4" }, "format": { "fill": "#FFC7CE", "color": "#9C0006" } }), &read, json!([[["B2:B6"], "cell_value", "#FFC7CE"]]));
    t.check(&x, "xlsx_add_conditional_format", json!({ "document_id": x, "sheet": sheet, "range": "A2:A6", "rule": { "type": "duplicate" }, "format": { "bold": true } }), &read, json!([[["B2:B6"], "cell_value", "#FFC7CE"], [["A2:A6"], "duplicate", null]]));
    t.check(&x, "xlsx_add_conditional_format", json!({ "document_id": x, "sheet": sheet, "range": "B2:B6", "rule": { "type": "color_scale", "points": [{ "type": "min", "color": "#FFFFFF" }, { "type": "max", "color": "#63BE7B" }] } }), &read, json!([[["B2:B6"], "cell_value", "#FFC7CE"], [["A2:A6"], "duplicate", null], [["B2:B6"], "color_scale", null]]));
    assert_eq!(t.err("xlsx_add_conditional_format", json!({ "document_id": x, "sheet": sheet, "range": "B2", "rule": { "type": "duplicate" } })), "invalid_argument");
    assert_eq!(t.err("xlsx_add_conditional_format", json!({ "document_id": x, "sheet": sheet, "range": "B2", "rule": { "type": "sparkles" }, "format": {} })), "invalid_argument");
    t.check(&x, "xlsx_delete_conditional_format", json!({ "document_id": x, "sheet": sheet, "index": 1 }), &read, json!([[["B2:B6"], "cell_value", "#FFC7CE"], [["B2:B6"], "color_scale", null]]));
    // Hyperlink and comment.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| t.call("xlsx_list_hyperlinks", json!({ "document_id": x, "sheet": s }))["hyperlinks"].clone();
    t.check(&x, "xlsx_set_hyperlink", json!({ "document_id": x, "sheet": sheet, "range": "A1", "url": "https://example.com/", "tooltip": "site" }), &read, json!([{ "sheet": sheet, "range": "A1", "url": "https://example.com/", "location": null, "tooltip": "site" }]));
    t.check(&x, "xlsx_set_hyperlink", json!({ "document_id": x, "sheet": sheet, "range": "A1" }), &read, json!([]));
    assert_eq!(t.err("xlsx_set_hyperlink", json!({ "document_id": x, "sheet": sheet, "range": "A1", "url": "u", "location": "l" })), "invalid_argument");
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| t.call("xlsx_list_comments", json!({ "document_id": x, "sheet": s }))["comments"].clone();
    t.check(&x, "xlsx_add_comment", json!({ "document_id": x, "sheet": sheet, "cell": "B3", "text": "check this", "author": "Ibis" }), &read, json!([{ "sheet": sheet, "cell": "B3", "author": "Ibis", "text": "check this" }]));
    t.check(&x, "xlsx_delete_comment", json!({ "document_id": x, "sheet": sheet, "cell": "B3" }), &read, json!([]));
    assert_eq!(t.err("xlsx_delete_comment", json!({ "document_id": x, "sheet": sheet, "cell": "B3" })), "invalid_argument");
    // Auto filter and table.
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| {
        let f = t.call("xlsx_list_tables", json!({ "document_id": x, "sheet": s }))["auto_filters"].clone();
        f.as_array().unwrap().iter().map(|f| json!([f["range"], f["columns"][0]["values"]])).collect::<Vec<_>>().into()
    };
    t.check(&x, "xlsx_set_auto_filter", json!({ "document_id": x, "sheet": sheet, "range": "A1:B6", "columns": [{ "column": "A", "values": ["a"] }] }), &read, json!([["A1:B6", ["a"]]]));
    t.check(&x, "xlsx_set_auto_filter", json!({ "document_id": x, "sheet": sheet, "range": null }), &read, json!([]));
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| {
        let f = t.call("xlsx_list_tables", json!({ "document_id": x, "sheet": s }))["tables"].clone();
        f.as_array().unwrap().iter().map(|f| json!([f["name"], f["range"], f["style"]["name"], f["columns"]])).collect::<Vec<_>>().into()
    };
    t.check(&x, "xlsx_set_table", json!({ "document_id": x, "sheet": sheet, "range": "A1:B6", "name": "Orders" }), &read, json!([["Orders", "A1:B6", "TableStyleMedium2", ["Item", "Qty"]]]));
    t.check(&x, "xlsx_set_table", json!({ "document_id": x, "sheet": sheet, "name": "Orders", "style": "TableStyleLight9" }), &read, json!([["Orders", "A1:B6", "TableStyleLight9", ["Item", "Qty"]]]));
    assert_eq!(t.err("xlsx_set_table", json!({ "document_id": x, "sheet": sheet, "range": "A2:B3" })), "invalid_argument", "overlaps, and no headings");
    // The table's style is laid on its cells: the heading is white on the style's colour.
    assert_eq!(format_of(&mut t, &x, &sheet, "A1", "bold"), json!(true));
    assert_ne!(format_of(&mut t, &x, &sheet, "A1", "fill"), Value::Null);
    t.check(&x, "xlsx_set_table", json!({ "document_id": x, "sheet": sheet, "name": "Orders", "remove": true }), &read, json!([]));
}

#[test]
fn mcp_xlsx_remove_duplicates_and_subtotals() {
    let mut t = T::new("xlsx_b_rows", &[]);
    let (x, sheet) = t.blank("blank.xlsx");
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:B6", "values": [["Item", "Qty"], ["a", 5], ["b", 1], ["a", 5], ["c", 9], ["b", 1]] }));
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| cells(t, x, &s, "A1:B6");
    let r = t.call("xlsx_remove_duplicates", json!({ "document_id": x, "sheet": sheet, "range": "A1:B6", "has_header": true }));
    assert_eq!(r["removed"], 2);
    assert_eq!(read(&mut t, &x), json!([["Item", "Qty"], ["a", 5.0], ["b", 1.0], ["c", 9.0], [null, null], [null, null]]));
    t.call("app_undo_document", json!({ "document_id": x }));
    assert_eq!(read(&mut t, &x)[3], json!(["a", 5.0]));
    assert_eq!(t.err("xlsx_remove_duplicates", json!({ "document_id": x, "sheet": sheet, "range": "A1:B6", "columns": ["C"] })), "out_of_range");
    // Subtotals: a total row after each group of Item, the sort first.
    t.call("xlsx_sort_range", json!({ "document_id": x, "sheet": sheet, "range": "A1:B6", "keys": [{ "column": "A" }], "has_header": true }));
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| t.call("xlsx_get_used_range", json!({ "document_id": x, "sheet": s }))["used_range"].clone();
    t.check(&x, "xlsx_apply_subtotal", json!({ "document_id": x, "sheet": sheet, "range": "A1:B6", "group_by": "A", "function": "sum", "columns": ["B"] }), &read, json!("A1:B10"));
    assert_eq!(t.err("xlsx_apply_subtotal", json!({ "document_id": x, "sheet": sheet, "range": "A1:B6", "group_by": "A", "function": "median", "columns": ["B"] })), "invalid_argument");
}

#[test]
fn mcp_xlsx_charts_are_inserted_set_and_deleted() {
    let mut t = T::new("xlsx_b_charts", &[]);
    let (x, sheet) = t.blank("blank.xlsx");
    t.call("xlsx_set_cells", json!({ "document_id": x, "sheet": sheet, "range": "A1:C4", "values": [["Month", "Sales", "Cost"], ["Apr", 10, 4], ["May", 12, 5], ["Jun", 9, 6]] }));
    let s = sheet.clone();
    let read = move |t: &mut T, x: &str| {
        let c = t.call("xlsx_list_charts", json!({ "document_id": x, "sheet": s }))["charts"].clone();
        c.as_array().unwrap().iter().map(|c| json!([c["kind"], c["title"], c["legend"], c["frame"]["width"], c["series"].as_array().map(Vec::len)])).collect::<Vec<_>>().into()
    };
    t.check(&x, "xlsx_insert_chart", json!({ "document_id": x, "sheet": sheet, "kind": "line", "data_range": "A1:C4", "title": "Monthly", "legend": "bottom", "frame": { "width": 300 } }), &read, json!([["line", "Monthly", "bottom", 300.0, 2]]));
    let chart = t.call("xlsx_list_charts", json!({ "document_id": x }))["charts"][0].clone();
    assert_eq!(chart["series"][0]["values"], json!(format!("{sheet}!$B$2:$B$4")), "{chart}");
    let id = chart["shape"].as_u64().unwrap();
    // Change it.
    t.check(&x, "xlsx_set_chart", json!({ "document_id": x, "sheet": sheet, "chart": id, "title": "Spring", "legend": "none", "frame": { "x": 40, "y": 200 } }), &read, json!([["line", "Spring", null, 300.0, 2]]));
    assert_eq!(t.err("xlsx_set_chart", json!({ "document_id": x, "sheet": sheet, "chart": id, "kind": "pie" })), "unsupported_operation");
    assert_eq!(t.err("xlsx_set_chart", json!({ "document_id": x, "sheet": sheet, "chart": 9999, "title": "x" })), "no_such_shape");
    assert_eq!(t.err("xlsx_insert_chart", json!({ "document_id": x, "sheet": sheet, "kind": "treemap", "data_range": "A1:C4" })), "unsupported_operation");
    assert_eq!(t.err("xlsx_insert_chart", json!({ "document_id": x, "sheet": sheet, "kind": "pie", "data_range": "A1:A1" })), "invalid_argument");
    t.check(&x, "xlsx_delete_chart", json!({ "document_id": x, "sheet": sheet, "chart": id }), &read, json!([]));
}
