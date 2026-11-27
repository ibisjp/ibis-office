// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Makes target/pptx159/check.pptx with the #159 MCP tools (connector, layout, chart data), to be opened in the real PowerPoint.
use ibis_doc_service::{Limits, Service};
use serde_json::{Value, json};

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pptx159").canonicalize().unwrap();
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    let mut call = |tool: &str, args: Value| -> Value { s.call(tool, &args).unwrap_or_else(|e| panic!("{tool}: {e}")) };
    let p = call("app_open_document", json!({ "path": dir.join("pptx-plan.pptx").to_string_lossy() }))["document_id"].as_str().unwrap().to_string();
    let mk = |c: &mut dyn FnMut(&str, Value) -> Value, x: u32| c("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "rect", "frame": { "x": x, "y": 300, "width": 80, "height": 40 }, "text": format!("S{x}") }))["shape"].as_u64().unwrap();
    let a = mk(&mut call, 20);
    let b = mk(&mut call, 300);
    for kind in ["straight", "elbow", "curved"] {
        let r = call("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "kind": kind, "arrow": "end", "start": { "shape": a, "site": 3 }, "end": { "shape": b, "site": 1 } }));
        println!("connector {kind}: {r}");
    }
    println!("shapes a={a} b={b}");
    let layouts = call("pptx_list_layouts", json!({ "document_id": p }));
    println!("layouts: {layouts}");
    let cur = call("pptx_list_slides", json!({ "document_id": p }))["slides"][0]["layout"].as_u64().unwrap();
    let target = if cur == 1 { 2 } else { 1 };
    println!("layout before {cur} -> {target}");
    call("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": target }));
    let chart = call("pptx_insert_chart", json!({ "document_id": p, "slide": 1, "kind": "column", "data": [["", "East", "West"], ["Q1", 10, 20], ["Q2", 30, 40]] }))["shape"].as_u64().unwrap();
    call("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": chart, "series_in": "columns", "data": [["", "East", "West", "North"], ["Q1", 1, 2, 3], ["Q2", 4, 5, 6], ["Q3", 7, 8, 9]] }));
    println!("chart shape {chart}: {}", call("pptx_list_charts", json!({ "document_id": p, "slide": 1 })));
    call("app_save_document", json!({ "document_id": p, "path": dir.join("check.pptx").to_string_lossy(), "overwrite": true }));
    println!("saved");
}
