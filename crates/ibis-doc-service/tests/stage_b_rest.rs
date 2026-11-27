// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #159, stage B, a theme taken from another file (`pptx_set_theme`), connectors that follow the shapes they join
//! (`pptx_insert_connector`), a slide's layout changed (`pptx_set_slide_properties`), the data of a chart changed (`pptx_set_chart_data`, `docx_set_chart_data`):
//! the numbers of PowerPoint's measurements, called as the implementation (win-pptx, 2026-11-25) takes its arguments. Design: `tasks/doc/test-design/issue-159-stage-b-rest.md`; the numbers are PowerPoint's (Windows, Microsoft 365 16.0,
//! `scripts/corpus/measure_pptx_layout_theme.ps1` and `measure_pptx_connector.ps1`).
//!
//! The tests that pass run with the others; T2 and T3 are `#[ignore]`d with their reason (`-- --ignored`) until the implementation does what they ask.

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
        match self.s.call(tool, &args) {
            Ok(v) => panic!("{tool} should have been refused: {v}"),
            Err(e) => e.code.to_string(),
        }
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

    fn slide(&mut self, doc: &str, slide: usize) -> Vec<Value> {
        self.call("pptx_get_slide", json!({ "document_id": doc, "slide": slide }))["shapes"].as_array().unwrap().clone()
    }
}

fn shape_of(shapes: &[Value], id: u64) -> Value {
    shapes.iter().find(|s| s["shape_id"] == id).unwrap_or_else(|| panic!("no shape {id} in {shapes:?}")).clone()
}

fn frame_of(shape: &Value) -> [f64; 4] {
    ["x", "y", "width", "height"].map(|k| shape[k].as_f64().unwrap_or_else(|| panic!("no {k} in {shape}")))
}

fn same(got: [f64; 4], want: [f64; 4], what: &str) {
    for (g, w) in got.iter().zip(want) {
        assert!((g - w).abs() <= 0.02, "{what}: {got:?} against {want:?}");
    }
}

/// T1: `pptx_set_theme` with `theme` = the path of another pptx takes that file's colours and typefaces as PowerPoint's `Presentation.ApplyTheme` does (the file
/// `pptx-theme-ref.pptx` is `pptx-plan.pptx` with dk2 303F9F, accent1 B8392E, accent2 2E7D32, hlink 0000EE and the typefaces Georgia and Verdana). PowerPoint: the master's
/// theme colours and typefaces are the file's, the colours the file does not change stay, the layouts and the slide's shapes (ids, texts, frames) stay.
#[test]
fn mcp_pptx_set_theme_takes_the_colours_and_typefaces_of_another_file() {
    let mut t = T::new("stage_b_rest_theme", &["pptx-plan.pptx", "pptx-theme-ref.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let before = t.call("pptx_get_theme", json!({ "document_id": p }));
    let layouts = t.call("pptx_list_layouts", json!({ "document_id": p }))["layouts"].clone();
    let slide_before = t.slide(&p, 1);
    // A shape that follows the theme (the fill is accent1), as in the test of the colours.
    let shape = t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "rect", "frame": { "x": 300, "y": 60, "width": 80, "height": 80 }, "text": "t" }))["shape"].as_u64().unwrap();
    let path = t.dir.join("pptx-theme-ref.pptx").to_string_lossy().to_string();
    t.call("pptx_set_theme", json!({ "document_id": p, "theme": path }));
    let after = t.call("pptx_get_theme", json!({ "document_id": p }));
    for (name, rgb) in [("dk2", "#303F9F"), ("accent1", "#B8392E"), ("accent2", "#2E7D32"), ("hlink", "#0000EE")] {
        assert_eq!(after["colors"][name], json!(rgb), "{name}: {after}");
    }
    for name in ["dk1", "lt1", "lt2", "accent3", "accent4", "accent5", "accent6", "folHlink"] {
        assert_eq!(after["colors"][name], before["colors"][name], "{name} is the file's, which is the same as before");
    }
    assert_eq!((after["fonts"]["major"]["latin"].clone(), after["fonts"]["minor"]["latin"].clone()), (json!("Georgia"), json!("Verdana")), "{after}");
    assert_eq!(t.call("pptx_list_layouts", json!({ "document_id": p }))["layouts"], layouts, "the layouts stay");
    let slide_after = t.slide(&p, 1);
    for old in &slide_before {
        let new = shape_of(&slide_after, old["shape_id"].as_u64().unwrap());
        assert_eq!((new["text"].clone(), frame_of(&new)), (old["text"].clone(), frame_of(old)), "a shape of the slide stays as it was");
    }
    let fill = shape_of(&slide_after, shape)["fill"].clone();
    assert_eq!((fill["theme"].clone(), fill["resolved"].clone()), (json!("accent1"), json!("#B8392E")), "a theme fill follows the file's theme: {fill}");
    let again = t.reopen(&p, "pptx-set-theme-file.pptx");
    assert_eq!(t.call("pptx_get_theme", json!({ "document_id": again }))["colors"]["accent1"], json!("#B8392E"), "the theme survives saving");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_get_theme", json!({ "document_id": p }))["colors"], before["colors"], "one undo step");
}

/// T2: the ways a theme from a file is refused: a path outside the allowed folders (`not_allowed_path`), a file that is not there (`file_not_found`), not a presentation (`unsupported_format`), and (not a refusal) `theme` together with `colors`: the colours given win over the file's.
#[test]
fn mcp_pptx_set_theme_refuses_what_cannot_be_a_theme_file() {
    let mut t = T::new("stage_b_rest_theme_errors", &["pptx-plan.pptx", "pptx-theme-ref.pptx", "picture-120x60.png"]);
    let p = t.open("pptx-plan.pptx");
    // A file that is there, outside the allowed folder (a path that is not there is `file_not_found` first).
    let outside = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/doc-service-tests-outside-theme.pptx");
    std::fs::copy(t.dir.join("pptx-theme-ref.pptx"), &outside).unwrap();
    assert_eq!(t.err("pptx_set_theme", json!({ "document_id": p, "theme": outside.to_string_lossy() })), "not_allowed_path");
    let _ = std::fs::remove_file(&outside);
    assert_eq!(t.err("pptx_set_theme", json!({ "document_id": p, "theme": t.dir.join("missing.pptx").to_string_lossy() })), "file_not_found");
    assert_eq!(t.err("pptx_set_theme", json!({ "document_id": p, "theme": t.dir.join("picture-120x60.png").to_string_lossy() })), "unsupported_format");
    let ok = t.dir.join("pptx-theme-ref.pptx").to_string_lossy().to_string();
    // Nothing was changed by the refusals.
    assert_eq!(t.call("pptx_get_theme", json!({ "document_id": p }))["colors"]["accent1"], json!("#156082"));
    // The file's theme and colours together: the colours given win.
    t.call("pptx_set_theme", json!({ "document_id": p, "theme": ok, "colors": { "accent1": "#000000" } }));
    assert_eq!(t.call("pptx_get_theme", json!({ "document_id": p }))["colors"]["accent1"], json!("#000000"));
}

/// The two rectangles A (100, 100, 100 x 60) and B (400, 200, 100 x 60) of the connector tests: (document, A's id, B's id).
fn two_rectangles(t: &mut T) -> (String, u64, u64) {
    let p = t.open("pptx-plan.pptx");
    let rect = |t: &mut T, x: u32, y: u32| t.call("pptx_insert_shape", json!({ "document_id": p, "slide": 1, "kind": "rect", "frame": { "x": x, "y": y, "width": 100, "height": 60 } }))["shape"].as_u64().unwrap();
    let a = rect(t, 100, 100);
    let b = rect(t, 400, 200);
    (p, a, b)
}

/// T3: a connector joined to two shapes (`pptx_insert_connector`, whose connection sites are numbered from 0: a rectangle's 0 top, 1 left, 2 bottom, 3 right; PowerPoint's
/// `BeginConnect(A, 4)` and `EndConnect(B, 2)` are `site` 3 and 1). PowerPoint: the frame is the box between the two sites, (200, 130, 200, 100), for a straight, an elbow and a curved
/// connector alike; when B moves to (300, 300) it is (200, 130, 100, 200), and to (50, 40) it is (50, 70, 150, 60) (flipped both ways). The connector follows in the same undo step as the
/// move, and still follows after saving and opening the file again.
#[test]
fn mcp_pptx_connector_joins_two_shapes_and_follows_them() {
    let mut t = T::new("stage_b_rest_connector", &["pptx-plan.pptx"]);
    let (p, a, b) = two_rectangles(&mut t);
    let mut ids = Vec::new();
    for kind in ["straight", "elbow", "curved"] {
        let id = t.call("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "kind": kind, "start": { "shape": a, "site": 3 }, "end": { "shape": b, "site": 1 } }))["shape"].as_u64().unwrap();
        same(frame_of(&shape_of(&t.slide(&p, 1), id)), [200.0, 130.0, 200.0, 100.0], &format!("a {kind} connector A right to B left"));
        ids.push(id);
    }
    // The joined shapes move: the connectors follow, in the same undo step as the move.
    t.call("pptx_set_frame", json!({ "document_id": p, "slide": 1, "frames": [{ "shape": b, "x": 300, "y": 300, "width": 100, "height": 60 }] }));
    same(frame_of(&shape_of(&t.slide(&p, 1), ids[0])), [200.0, 130.0, 100.0, 200.0], "after B moved to (300, 300)");
    t.call("pptx_set_frame", json!({ "document_id": p, "slide": 1, "frames": [{ "shape": b, "x": 50, "y": 40, "width": 100, "height": 60 }] }));
    same(frame_of(&shape_of(&t.slide(&p, 1), ids[0])), [50.0, 70.0, 150.0, 60.0], "after B moved to (50, 40)");
    t.call("app_undo_document", json!({ "document_id": p }));
    same(frame_of(&shape_of(&t.slide(&p, 1), ids[0])), [200.0, 130.0, 100.0, 200.0], "one undo step takes the move and the follow back together");
    t.call("app_undo_document", json!({ "document_id": p }));
    same(frame_of(&shape_of(&t.slide(&p, 1), ids[0])), [200.0, 130.0, 200.0, 100.0], "the move before it too");
    // The join is in the file: saved, opened again, and the connector still follows.
    let again = t.reopen(&p, "pptx-connector.pptx");
    // The ids are others after saving: B is the shape at (400, 200), the connector the one between the sites.
    let shapes = t.slide(&again, 1);
    let at = |frame: [f64; 4]| shapes.iter().find(|s| ["x", "y", "width", "height"].iter().zip(frame).all(|(k, v)| s[*k].as_f64().is_some_and(|g| (g - v).abs() <= 0.02))).unwrap_or_else(|| panic!("nothing at {frame:?}: {shapes:?}"))["shape_id"].as_u64().unwrap();
    let (b2, c2) = (at([400.0, 200.0, 100.0, 60.0]), at([200.0, 130.0, 200.0, 100.0]));
    t.call("pptx_set_frame", json!({ "document_id": again, "slide": 1, "frames": [{ "shape": b2, "x": 300, "y": 300, "width": 100, "height": 60 }] }));
    same(frame_of(&shape_of(&t.slide(&again, 1), c2)), [200.0, 130.0, 100.0, 200.0], "it follows after being opened again");
}

/// T4: a connector with one end joined and the other at a point ({ x, y } in points): it starts at A's right site (200, 130) and ends at (450, 130), so its frame is
/// (200, 130, 250, 0); and the refusals: a shape that is not there (`no_such_shape`), a site a rectangle does not have (0 to 3 only: 4 and 9 are `out_of_range`), a kind
/// that is none (`invalid_argument`). One undo step for the one connector.
#[test]
fn mcp_pptx_connector_with_one_end_and_the_refusals() {
    let mut t = T::new("stage_b_rest_connector_errors", &["pptx-plan.pptx"]);
    let (p, a, b) = two_rectangles(&mut t);
    let id = t.call("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "start": { "shape": a, "site": 3 }, "end": { "x": 450, "y": 130 } }))["shape"].as_u64().unwrap();
    same(frame_of(&shape_of(&t.slide(&p, 1), id)), [200.0, 130.0, 250.0, 0.0], "one end on A's right site, the other at (450, 130)");
    assert_eq!(t.err("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "start": { "shape": 999_999, "site": 1 }, "end": { "shape": b, "site": 1 } })), "no_such_shape");
    for site in [4, 9] {
        assert_eq!(t.err("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "start": { "shape": a, "site": site }, "end": { "shape": b, "site": 1 } })), "out_of_range", "site {site}");
    }
    assert_eq!(t.err("pptx_insert_connector", json!({ "document_id": p, "slide": 1, "kind": "zigzag", "start": { "x": 1, "y": 1 }, "end": { "x": 5, "y": 5 } })), "invalid_argument");
    let count = t.slide(&p, 1).len();
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.slide(&p, 1).len(), count - 1);
}

/// T5: `pptx_set_slide_properties` with `layout` (a name, or its number from `pptx_list_layouts`) changes the slide's layout as PowerPoint's `Slide.CustomLayout =`
/// does (slide 1 of pptx-plan.pptx: the title "架空商店 2026年度 事業計画" and the subtitle "Fictitious Shop — FY2026 Plan", layout 1 タイトル スライド, 960 x 540):
/// every shape keeps its id and text; a title is the new layout's title and the other text the new layout's body (in order), each taking that placeholder's frame; the
/// placeholders of the new layout that nothing took are added empty (2 つのコンテンツ: one, 比較: three).
/// (layout, the title's frame, the subtitle's frame, the shapes added: their frames).
#[test]
fn mcp_pptx_set_slide_properties_changes_the_layout_as_powerpoint_does() {
    type Frame = [f64; 4];
    let cases: [(&str, Frame, Frame, Vec<Frame>); 4] = [
        ("タイトルとコンテンツ", [66.0, 28.75, 828.0, 104.38], [66.0, 143.75, 828.0, 342.63], vec![]),
        ("2 つのコンテンツ", [66.0, 28.75, 828.0, 104.38], [66.0, 143.75, 408.0, 342.63], vec![[486.0, 143.75, 408.0, 342.63]]),
        ("比較", [66.13, 28.75, 828.0, 104.38], [66.13, 132.38, 406.13, 64.87], vec![[66.13, 197.25, 406.13, 290.13], [486.0, 132.38, 408.13, 64.87], [486.0, 197.25, 408.13, 290.13]]),
        ("セクション見出し", [65.5, 134.63, 828.0, 224.63], [65.5, 361.38, 828.0, 118.13], vec![]),
    ];
    for (n, (layout, title, body, added)) in cases.into_iter().enumerate() {
        let mut t = T::new(&format!("stage_b_rest_layout_{n}"), &["pptx-plan.pptx"]);
        let p = t.open("pptx-plan.pptx");
        let before = t.slide(&p, 1);
        assert_eq!(before.len(), 2, "slide 1 of pptx-plan.pptx has a title and a subtitle");
        // The shapes are told by their texts (this program numbers them as the file does; PowerPoint's COM numbers are 4 and 5).
        let by_text = |shapes: &[Value], text: &str| shapes.iter().find(|s| s["text"] == text).unwrap_or_else(|| panic!("no shape with the text {text:?} in {shapes:?}")).clone();
        let (title_text, sub_text) = ("架空商店 2026年度 事業計画", "Fictitious Shop — FY2026 Plan");
        let (id4, id5) = (by_text(&before, title_text)["shape_id"].as_u64().unwrap(), by_text(&before, sub_text)["shape_id"].as_u64().unwrap());
        let first = frame_of(&shape_of(&before, id4));
        t.call("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": layout }));
        let after = t.slide(&p, 1);
        assert_eq!(after.len(), 2 + added.len(), "{layout}: the shapes of the slide: {after:?}");
        let (s4, s5) = (shape_of(&after, id4), shape_of(&after, id5));
        assert_eq!((s4["text"].clone(), s5["text"].clone()), (json!(title_text), json!(sub_text)), "{layout}: the texts stay with their shapes");
        same(frame_of(&s4), title, &format!("{layout}: the title's frame"));
        same(frame_of(&s5), body, &format!("{layout}: the subtitle takes the body's frame"));
        let mut new: Vec<Frame> = after.iter().filter(|s| s["shape_id"] != id4 && s["shape_id"] != id5).map(|s| {
            assert!(s["text"].is_null() || s["text"] == json!(""), "{layout}: an added placeholder is empty: {s}");
            frame_of(s)
        }).collect();
        let mut want = added.clone();
        let key = |f: &Frame| ((f[0] * 100.0) as i64, (f[1] * 100.0) as i64);
        new.sort_by_key(key);
        want.sort_by_key(key);
        for (g, w) in new.iter().zip(&want) {
            same(*g, *w, &format!("{layout}: an added placeholder's frame"));
        }
        let slides = t.call("pptx_list_slides", json!({ "document_id": p }));
        assert_eq!(slides["slides"][0]["layout_name"], json!(layout), "the slide says its layout: {slides}");
        // Saved, opened again, and the same.
        let again = t.reopen(&p, "pptx-layout.pptx");
        assert_eq!(t.slide(&again, 1).len(), 2 + added.len(), "{layout}: the layout survives saving");
        assert_eq!(t.call("pptx_list_slides", json!({ "document_id": again }))["slides"][0]["layout_name"], json!(layout));
        // One undo step.
        t.call("app_undo_document", json!({ "document_id": p }));
        let back = t.slide(&p, 1);
        assert_eq!(back.len(), 2, "{layout}: one undo step takes the layout back");
        same(frame_of(&shape_of(&back, id4)), first, &format!("{layout}: the title's first frame"));
    }
}

/// T6: the layout given by its number, the layout it already has (nothing changes), and the refusals: a name that is not a layout (`invalid_argument`), a number past the
/// layouts (`out_of_range`), a slide that is not there (`out_of_range`). One undo step takes the slide back to its first layout.
#[test]
fn mcp_pptx_set_slide_properties_layout_by_number_and_the_refusals() {
    let mut t = T::new("stage_b_rest_layout_errors", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let layouts = t.call("pptx_list_layouts", json!({ "document_id": p }))["layouts"].clone();
    let second = layouts[1]["name"].clone();
    t.call("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": 2 }));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"][0]["layout_name"], second);
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(t.call("pptx_list_slides", json!({ "document_id": p }))["slides"][0]["layout"], json!(1), "the first layout again");
    assert_eq!(t.err("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": "no such layout" })), "invalid_argument");
    assert_eq!(t.err("pptx_set_slide_properties", json!({ "document_id": p, "slide": 1, "layout": 99 })), "out_of_range");
    assert_eq!(t.err("pptx_set_slide_properties", json!({ "document_id": p, "slide": 99, "layout": 2 })), "out_of_range");
}

/// The series of a chart of a slide as `pptx_list_charts` gives them: (name, number of values).
fn pptx_series(t: &mut T, doc: &str, shape: u64) -> Vec<(String, u64)> {
    let charts = t.call("pptx_list_charts", json!({ "document_id": doc, "slide": 1 }))["charts"].clone();
    // (A saved file numbers its shapes again: the only chart of the slide is the one.)
    let all = charts.as_array().unwrap();
    let c = all.iter().find(|c| c["shape_id"] == shape).or(if all.len() == 1 { all.first() } else { None }).unwrap_or_else(|| panic!("no chart {shape}: {charts}")).clone();
    c["series"].as_array().unwrap().iter().map(|s| (s["name"].as_str().unwrap_or("").to_string(), s["values"].as_u64().unwrap())).collect()
}

/// T7: `pptx_set_chart_data` replaces the data of a chart made by `pptx_insert_chart` (the first row names the series, the first column the categories; `series_in`
/// says which way the series run). The kind and frame are kept. What can be read back is the series' names and how many values each has (`pptx_list_charts`; the numbers
/// themselves have no reading tool yet: the data survives saving is checked by those). One undo step per call; the refusals change nothing.
#[test]
fn mcp_pptx_set_chart_data_changes_the_data_and_keeps_the_rest() {
    let mut t = T::new("stage_b_rest_pptx_chart", &["pptx-plan.pptx"]);
    let p = t.open("pptx-plan.pptx");
    let shape = t.call("pptx_insert_chart", json!({ "document_id": p, "slide": 1, "kind": "column", "frame": { "x": 50, "y": 60, "width": 300, "height": 200 }, "title": "Sales", "legend": "bottom", "data": [["", "2025", "2026"], ["East", 10, 14], ["West", 7, 9]] }))["shape"].as_u64().unwrap();
    assert_eq!(pptx_series(&mut t, &p, shape), vec![("2025".to_string(), 2), ("2026".to_string(), 2)], "what pptx_insert_chart made");
    // New data: one more category and a series less.
    t.call("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": shape, "data": [["", "Total"], ["East", 31], ["West", 22], ["North", 5.5]] }));
    assert_eq!(pptx_series(&mut t, &p, shape), vec![("Total".to_string(), 3)]);
    let charts = t.call("pptx_list_charts", json!({ "document_id": p, "slide": 1 }))["charts"].clone();
    let me = charts.as_array().unwrap().iter().find(|c| c["shape_id"] == shape).unwrap().clone();
    assert_eq!((me["width"].clone(), me["height"].clone()), (json!(300.0), json!(200.0)), "the frame is kept: {me}");
    // The series in rows.
    t.call("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": shape, "series_in": "rows", "data": [["", "Q1", "Q2"], ["East", 1, 2], ["West", 3, 4]] }));
    assert_eq!(pptx_series(&mut t, &p, shape), vec![("East".to_string(), 2), ("West".to_string(), 2)], "series in rows");
    // Saved and opened again.
    let again = t.reopen(&p, "pptx-set-chart.pptx");
    // (Saving renumbers the shapes: the only chart of the slide is the one.)
    let again_shape = t.call("pptx_list_charts", json!({ "document_id": again, "slide": 1 }))["charts"][0]["shape_id"].as_u64().unwrap();
    assert_eq!(pptx_series(&mut t, &again, again_shape), vec![("East".to_string(), 2), ("West".to_string(), 2)], "the data survives saving");
    // One undo step for each change.
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(pptx_series(&mut t, &p, shape), vec![("Total".to_string(), 3)], "one undo step");
    t.call("app_undo_document", json!({ "document_id": p }));
    assert_eq!(pptx_series(&mut t, &p, shape), vec![("2025".to_string(), 2), ("2026".to_string(), 2)], "and another");
    // The refusals: not a chart, a shape that is not there, data that is not a table of at least 2 x 2, a name where a number is expected.
    let text = t.slide(&p, 1).iter().find(|s| s["kind"] != "chart" && s["text"].is_string()).unwrap()["shape_id"].as_u64().unwrap();
    assert_eq!(t.err("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": text, "data": [["", "a"], ["x", 1]] })), "unsupported_operation");
    assert_eq!(t.err("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": 999_999, "data": [["", "a"], ["x", 1]] })), "no_such_shape");
    assert_eq!(t.err("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": shape, "data": [["a"], ["b"]] })), "invalid_argument");
    assert_eq!(t.err("pptx_set_chart_data", json!({ "document_id": p, "slide": 1, "shape": shape, "data": [["", "a"], ["x", "not a number"]] })), "invalid_argument");
    assert_eq!(pptx_series(&mut t, &p, shape), vec![("2025".to_string(), 2), ("2026".to_string(), 2)], "the refusals changed nothing");
}

/// T8: the same for a chart of a document (docx), by its place (`at` = { paragraph, offset } as `docx_list_charts` gives it): `docx_set_chart_data`. What is read back is the
/// number of series and the title (`docx_list_charts`).
#[test]
fn mcp_docx_set_chart_data_changes_the_data_and_keeps_the_rest() {
    let mut t = T::new("stage_b_rest_docx_chart", &["autospace-letters.docx"]);
    let d = t.open("autospace-letters.docx");
    t.call("docx_insert_chart", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 0 }, "kind": "column", "title": "Sales", "legend": "bottom", "data": [["", "East", "West"], ["Q1", 10, 20], ["Q2", 30, 40]] }));
    let at = json!({ "paragraph": 0, "offset": 0 });
    let chart = |t: &mut T, doc: &str| t.call("docx_list_charts", json!({ "document_id": doc }))["charts"][0].clone();
    assert_eq!((chart(&mut t, &d)["series"].clone(), chart(&mut t, &d)["title"].clone()), (json!(2), json!("Sales")), "what docx_insert_chart made");
    t.call("docx_set_chart_data", json!({ "document_id": d, "at": at, "data": [["", "East"], ["Q1", 1], ["Q2", 2], ["Q3", 3]] }));
    let changed = chart(&mut t, &d);
    assert_eq!((changed["series"].clone(), changed["title"].clone(), changed["legend"].clone()), (json!(1), json!("Sales"), json!(true)), "the series are one; the title and legend are kept: {changed}");
    let again_path = t.dir.join("copy_docx-set-chart.docx");
    t.call("app_save_document", json!({ "document_id": d, "path": again_path.to_string_lossy(), "overwrite": true }));
    let again = t.open("copy_docx-set-chart.docx");
    assert_eq!(chart(&mut t, &again)["series"], json!(1), "the data survives saving");
    t.call("app_undo_document", json!({ "document_id": d }));
    assert_eq!(chart(&mut t, &d)["series"], json!(2), "one undo step");
    assert_eq!(t.err("docx_set_chart_data", json!({ "document_id": d, "at": { "paragraph": 0, "offset": 3 }, "data": [["", "a"], ["x", 1]] })), "no_such_shape", "no chart there");
    assert_eq!(t.err("docx_set_chart_data", json!({ "document_id": d, "at": at, "data": [["a"], ["b"]] })), "invalid_argument");
}
