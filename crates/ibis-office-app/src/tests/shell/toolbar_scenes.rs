// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #241: the table of the toolbar's own orders (`shell/toolbar_defaults.rs`) is what the formats make, in every scene of every format. Each
//! scene is reached in the scenario: by what a document has (a table, a picture) and by putting an object in (a table, a picture, a clip,
//! a line) and selecting it. With `IBIS_DUMP_TOOLBAR_DEFAULTS=1` the scenario writes the table (`target/toolbar-defaults.rs`) instead of
//! checking it.
use crate::docx::ext::*;
use crate::pptx::ext::*;
use crate::shell::chrome::{MenuEntry, ToolItem};
use crate::shell::scenario::Scenario;
use crate::shell::toolbar_layout as layout;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::toolbar_defaults_match_the_table", defaults_match_the_table)]
}

/// The first position of a paragraph in the first table's first cell, and of a picture, of the open document (docx-report.docx has both).
fn docx_table_caret(s: &mut Scenario) {
    let pos = s.with(|e, _| {
        let doc = e.flow().unwrap();
        for b in &doc.body {
            if let ibis_doc_model::Block::Table(t) = b {
                for b2 in &t.rows[0].cells[0].blocks {
                    if let ibis_doc_model::Block::Paragraph(p) = b2 {
                        return ibis_doc_model::FlowPos::new(p.id, 0);
                    }
                }
            }
        }
        panic!("a table in the document");
    });
    s.update(|e, _, cx| e.set_caret(pos, false, cx));
}

fn docx_picture_caret(s: &mut Scenario) {
    let pos = s.with(|e, _| {
        let doc = e.flow().unwrap();
        for b in &doc.body {
            if let ibis_doc_model::Block::Paragraph(p) = b {
                let mut off = 0;
                for r in &p.runs {
                    if matches!(r.kind, ibis_doc_model::FlowRunKind::Picture { .. }) {
                        return ibis_doc_model::FlowPos::new(p.id, off);
                    }
                    off += r.len();
                }
            }
        }
        panic!("a picture in the document");
    });
    s.update(|e, _, cx| e.set_caret(pos, false, cx));
}

fn nothing(_: &mut Scenario) {}

fn slide_shape(s: &mut Scenario) {
    s.select_shape(0);
}

fn slide_table(s: &mut Scenario) {
    s.update(|e, w, cx| e.insert_slide_table(2, 2, w, cx));
}

/// Select the shape that was put in (a click on its middle, as a person selects it).
fn select_new(s: &mut Scenario, id: Option<ibis_doc_model::ShapeId>) {
    let id = id.expect("the object was put in");
    s.settle();
    let at = s.with(|e, _| e.slide().shapes.iter().position(|shape| shape.id == id)).expect("the new shape is on the slide");
    s.select_shape(at);
}

fn slide_picture(s: &mut Scenario) {
    let id = s.update(|e, _, cx| ["star", "favorite", "home", "check", "add", "image", "search"].into_iter().find_map(|name| e.insert_icon(name, cx)));
    select_new(s, id);
}

fn slide_clip(s: &mut Scenario) {
    let path = crate::shell::scenario::repo_root().join("test/editing/clip-test.mp4");
    let data = std::fs::read(&path).expect("the clip");
    let id = s.update(|e, _, cx| e.insert_clip(&path, data, false, cx));
    select_new(s, id);
}

fn slide_line(s: &mut Scenario) {
    let first = match s.toolbar_part("insert-line") {
        Some(ToolItem::Button(b)) => b.menu.unwrap_or_default().into_iter().find_map(|m| if let MenuEntry::Item(i) = m { Some(i.id) } else { None }).expect("a line to draw"),
        _ => panic!("the line button"),
    };
    s.toolbar_menu("insert-line", first);
    let (x, y) = s.shape_position(0);
    s.drag((x - 60., y - 30.), (x + 60., y + 30.));
}

/// (the format, the file, the scene it must be in, what puts the document in it)
const STATES: &[(&str, &str, &str, fn(&mut Scenario))] = &[
    ("docx", "test/editing/docx-report.docx", "none", nothing),
    ("docx", "test/editing/docx-report.docx", "table", docx_table_caret),
    ("docx", "test/editing/docx-report.docx", "image", docx_picture_caret),
    ("xlsx", "test/editing/xlsx-sales.xlsx", "cell", nothing),
    ("pptx", "test/editing/88-review-order.pptx", "none", nothing),
    ("pptx", "test/editing/88-review-order.pptx", "shape", slide_shape),
    ("pptx", "test/editing/88-review-order.pptx", "table", slide_table),
    ("pptx", "test/editing/88-review-order.pptx", "image", slide_picture),
    ("pptx", "test/editing/88-review-order.pptx", "video", slide_clip),
    ("pptx", "test/editing/88-review-order.pptx", "line", slide_line),
];

fn defaults_match_the_table() {
    let dump = std::env::var("IBIS_DUMP_TOOLBAR_DEFAULTS").is_ok();
    let mut scenes: Vec<(String, String, Vec<String>)> = Vec::new();
    let mut buttons: Vec<(String, String, String)> = Vec::new();
    for (ext, path, expected, setup) in STATES {
        let mut s = Scenario::open(path);
        s.size(1200., 800.);
        s.settle();
        setup(&mut s);
        s.settle();
        let (scene, items) = s.update(|e, _, _| (crate::shell::format::with_format!(e, toolbar_scene(e)), e.toolbar_items_default()));
        assert_eq!(scene, *expected, "{path}: the document is in the scene {expected}");
        scenes.push((ext.to_string(), scene.to_string(), layout::default_keys(&items)));
        for item in &items {
            let (id, icon, tip) = match item {
                ToolItem::Button(b) => (b.id.to_string(), b.icon.unwrap_or("").to_string(), if let Some(t) = &b.text { t.to_string() } else { b.tooltip.to_string() }),
                ToolItem::Select { id, tooltip, .. } => (id.to_string(), "arrow_drop_down".to_string(), tooltip.to_string()),
                ToolItem::FontSize { .. } => ("font-size".to_string(), "format_size".to_string(), "toolbar.customize.font-size".to_string()),
                ToolItem::Separator => continue,
            };
            // The table keeps the id of the words, not the words (they are in the language the dump ran in).
            let tip = if crate::shell::i18n::lookup_words(&tip).is_some() || tip.is_empty() { tip } else { crate::shell::i18n::key_of_japanese(&tip).unwrap_or_else(|| id.clone()) };
            if !buttons.iter().any(|(i, _, _)| *i == id) {
                buttons.push((id, icon, tip));
            }
        }
    }
    if dump {
        let q = |s: &str| format!("{s:?}");
        let mut out = String::from("pub(crate) const SCENES: &[(&str, &str, &[&str])] = &[\n");
        for (e, sc, keys) in &scenes {
            out += &format!("    ({}, {}, &[{}]),\n", q(e), q(sc), keys.iter().map(|k| q(k)).collect::<Vec<_>>().join(", "));
        }
        out += "];\n/// Every button of those orders: its id, its icon, and its tooltip as the format wrote it (a key of the words, or the words).\npub(crate) const BUTTONS: &[(&str, &str, &str)] = &[\n";
        for (i, icon, tip) in &buttons {
            out += &format!("    ({}, {}, {}),\n", q(i), q(icon), q(tip));
        }
        out += "];\n";
        std::fs::write(crate::shell::scenario::repo_root().join("target/toolbar-defaults.rs"), out).expect("write the table");
        return;
    }
    for (ext, scene, keys) in &scenes {
        assert_eq!(crate::shell::toolbar_defaults::keys(ext, scene).as_ref(), Some(keys), "the table's order of {ext}/{scene} is what the format makes (run with IBIS_DUMP_TOOLBAR_DEFAULTS=1 and copy target/toolbar-defaults.rs)");
    }
    for (id, icon, tip) in &buttons {
        assert_eq!(crate::shell::toolbar_defaults::button(id), Some((icon.as_str(), tip.as_str())), "the table's button {id}");
    }
}
