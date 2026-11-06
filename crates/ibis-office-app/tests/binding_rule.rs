// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A panel that edits several objects at once builds what its fields show with `Shown::of` (`shell/chrome/shown.rs`): the common
//! value, or an empty field when the objects differ, and Apply writes only the fields that hold a value
//! (tasks/doc/design/ui-model-binding.md, "混在値"). This test keeps that from being undone by a later edit. It reads the sources
//! with a plain search, as `window_state_rule.rs` does:
//!
//! 1. A source that uses `Shown::of` (a converted panel) is listed in `src/tests/pptx/binding_mixed.rs` (the property test
//!    "two shapes that differ, one field typed, the other attributes keep each shape's own value" runs for every listed panel).
//! 2. A converted panel does not read its values from the first object (`targets[0]`, `.first()` of the targets or shapes): that is
//!    what made Apply set every object to the first one's values.
//! 3. A converted panel makes its fields only through `ShownNumber::input` / `ShownText::input` (no `InputState::new` for a field
//!    that shows an attribute), and does not turn a field back into a number with a bare `parse` of `.value()`.
//! 4. A converted panel that draws a slider, a segment control, a dropdown, a checkbox or a radio group for such an attribute
//!    passes the `Shown` to it (`.shown(&shown)`, `radio_button_shown`), so the empty state is the shared control's.
//! 5. No source outside `shell/chrome/` calls `.mixed(` or `.indeterminate(` (the controls' private modes: `mixed(true)` with a
//!    value, or a value that contradicts "mixed", cannot be built from one `Shown`). They are private in the controls, so the
//!    compiler refuses too; this keeps a later edit from making them public again.
//!
//! Rule: doc/rule/implimentationRule.md, 「複数の対象を編集するパネル」.

use std::path::{Path, PathBuf};

/// Sources that may use `InputState::new` in a converted panel, with the reason (a field that shows no attribute of the objects).
const INPUT_STATE_ALLOWED: &[(&str, &str)] = &[
    ("pptx/frame_panel.rs", "the panel's fields are made from the text of its own `values` array (the text of a `Shown` through `Shown::one`); the scale fields show no attribute"),
    ("pptx/insets_panel.rs", "the text zoom field (an empty field means \"keep\", no attribute of the objects)"),
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("a directory") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn panels_with_mixed_values_are_covered_and_use_the_shared_controls() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    let property_test = std::fs::read_to_string(src.join("tests/pptx/binding_mixed.rs")).expect("the property test");
    let mut converted = Vec::new();
    let mut failures = Vec::new();
    for file in files {
        let rel = file.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        if rel.starts_with("shell/chrome/") || rel == "shell/chrome.rs" { continue; }
        let text = std::fs::read_to_string(&file).expect("a source");
        let production = text.split("#[cfg(test)]").next().unwrap_or(&text);
        // 5.
        for banned in [".mixed(", ".indeterminate("] {
            if production.contains(banned) {
                failures.push(format!("{rel}: calls `{banned}` directly; give the control the `Shown` (`.shown(&shown)`)"));
            }
        }
        if !production.contains("Shown::of(") { continue; }
        converted.push(rel.clone());
        let stem = rel.rsplit('/').next().unwrap().trim_end_matches(".rs").trim_end_matches("_panel");
        // 1.
        if !property_test.contains(&format!("name: \"{stem}\"")) {
            failures.push(format!("{rel}: uses `Shown::of` but is not a case of `src/tests/pptx/binding_mixed.rs` (name: \"{stem}\")"));
        }
        // 2.
        for banned in ["targets[0]", "targets.first()", "shapes.first()", "shapes[0]"] {
            if production.contains(banned) {
                failures.push(format!("{rel}: reads a value from the first object (`{banned}`); use `Shown::of` over all the objects"));
            }
        }
        // 3.
        if production.contains("InputState::new(") && !INPUT_STATE_ALLOWED.iter().any(|(f, _)| *f == rel) {
            failures.push(format!("{rel}: makes a field with `InputState::new`; use `ShownNumber::input` / `ShownText::input`"));
        }
        if production.contains(".value().trim().parse") || production.contains(".value().parse") {
            failures.push(format!("{rel}: turns a field into a number with a bare parse; use `ShownNumber::resolve` (`Typed`)"));
        }
        // 4.
        for (control, attribute) in [("chrome::slider(", ".shown("), ("SegmentControl::new(", ".shown("), ("Dropdown::", ".shown("), ("Checkbox::new(", ".shown("), ("radio_button(", "radio_button_shown(")] {
            if production.contains(control) && !production.contains(attribute) {
                failures.push(format!("{rel}: draws `{control}` but never passes the `Shown` (`{attribute}`)"));
            }
        }
    }
    assert!(!converted.is_empty(), "no panel uses Shown::of: the search is stale");
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}
