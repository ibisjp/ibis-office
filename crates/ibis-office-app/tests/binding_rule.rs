// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A panel that edits several objects at once builds what its fields show with `Shown::of` (`shell/chrome/shown.rs`): the common
//! value, or an empty field when the objects differ, and Apply writes only the fields that hold a value
//! (tasks/doc/design/ui-model-binding.md, "混在値"). This test keeps that from being undone by a later edit. It reads the sources
//! with a plain search, as `window_state_rule.rs` does:
//!
//! 1. A source that uses `Shown::of` (a converted panel) is listed in `crates/ibis-office-pptx/src/tests/binding_mixed.rs` (the property test
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
//! 6. (`no_new_raw_field_reading_in_panels`) A panel does not make a field with `InputState::new`, turn a field into a number with
//!    a bare `parse`, or round a converted number with `.round()` by itself: it uses `ShownNumber` (the rule "rounding is for display
//!    only"). The panels that already did so when the test was introduced are in `RAW_FIELD_ALLOWED` with the number of
//!    occurrences and the reason. A new occurrence fails the test; a panel that moved to `ShownNumber` has to lower its numbers in
//!    the list (the list is updated only downwards, so the test also fails when a count is lower than the list says).
//!
//! Rule: doc/rule/implimentationRule.md, 「複数の対象を編集するパネル」.

use std::path::{Path, PathBuf};

/// Sources that may use `InputState::new` in a converted panel, with the reason (a field that shows no attribute of the objects).
const INPUT_STATE_ALLOWED: &[(&str, &str)] = &[
    ("shell/frame_panel.rs", "the panel's fields are made from the text of its own `values` array (the text of a `Shown` through `Shown::one`); the scale fields show no attribute"),
    ("shell/insets_panel.rs", "the text zoom field (an empty field means \"keep\", no attribute of the objects)"),
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

/// The shell's sources (crates/ibis-office-shell/src): this crate's `src/shell/` before the shell was a crate of its own
/// (tasks/doc/crate-split-plan.md, 12).
fn shell_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ibis-office-shell/src")
}

/// The presentation screen's sources (crates/ibis-office-pptx/src): this crate's `src/pptx/` before the format was a crate of its own
/// (tasks/doc/crate-split-plan.md, 14).
fn pptx_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ibis-office-pptx/src")
}

/// The document screen's sources (crates/ibis-office-docx/src): this crate's `src/docx/` before it was a crate of its own
/// (tasks/doc/crate-split-plan.md, 15).
fn docx_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ibis-office-docx/src")
}

/// The workbook screen's sources (crates/ibis-office-xlsx/src): this crate's `src/xlsx/` before it was a crate of its own
/// (tasks/doc/crate-split-plan.md, 16).
fn xlsx_src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ibis-office-xlsx/src")
}

/// `file`'s path as the tables name it: from `src/`, and a crate cut out of it as `shell/<path>`, `docx/<path>`, `xlsx/<path>`
/// (their tests as `tests/shell/<file>`, `tests/docx/<file>`, `tests/xlsx/<file>`).
fn relative(src: &Path, file: &Path) -> String {
    let slash = |p: &Path| p.to_string_lossy().replace('\\', "/");
    match file.strip_prefix(src) {
        Ok(rel) => slash(rel),
        Err(_) => {
            // The shell's (`shell/<path>`), the document screen's (`docx/<path>`), the presentation screen's (`pptx/<path>`) and the workbook screen's (`xlsx/<path>`).
            let (name, root) = if file.starts_with(docx_src()) {
                ("docx", docx_src())
            } else if file.starts_with(pptx_src()) {
                ("pptx", pptx_src())
            } else if file.starts_with(xlsx_src()) {
                ("xlsx", xlsx_src())
            } else {
                ("shell", shell_src())
            };
            let rel = slash(file.strip_prefix(root).expect("a source of this crate or of a crate cut out of it"));
            match rel.strip_prefix("tests/") {
                Some(test) => format!("tests/{name}/{test}"),
                None => format!("{name}/{rel}"),
            }
        }
    }
}

#[test]
fn panels_with_mixed_values_are_covered_and_use_the_shared_controls() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    sources(&shell_src(), &mut files);
    sources(&docx_src(), &mut files);
    sources(&pptx_src(), &mut files);
    sources(&xlsx_src(), &mut files);
    let property_test = std::fs::read_to_string(pptx_src().join("tests/binding_mixed.rs")).expect("the property test");
    let mut converted = Vec::new();
    let mut failures = Vec::new();
    for file in files {
        let rel = relative(&src, &file);
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
            failures.push(format!("{rel}: uses `Shown::of` but is not a case of `crates/ibis-office-pptx/src/tests/binding_mixed.rs` (name: \"{stem}\")"));
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

/// (source under `src/`, `InputState::new` calls, bare number `parse` calls, `.round()` of a unit conversion, reason). Counts are of the
/// production code (before `#[cfg(test)]`) of a source that makes a field with `InputState`; comment lines are not counted. The
/// list may only shrink: lower a number (or remove the line) when a panel moves to `ShownNumber`; never raise one.
const RAW_FIELD_ALLOWED: &[(&str, usize, usize, usize, &str)] = &[
    ("docx/color_panel.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/control_panel.rs", 5, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/editing.rs", 3, 2, 3, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/find.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/fit_text_panel.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/font_panel.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/goto.rs", 1, 9, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/help_panels.rs", 27, 9, 7, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/kinsoku_panel.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/link_panel.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/list_style_panel.rs", 6, 3, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/merge_panel.rs", 13, 3, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/merge_panel/workflow/review.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/paragraph_panel.rs", 1, 4, 4, "3-A, not moved: writes only the fields whose text differs from the one it opened with (`initial`), so an unedited Apply keeps the document; the unit switch (characters, cm, pt) and the line rule need a design of their own for ShownNumber"),
    ("docx/picture_panel.rs", 5, 4, 1, "3-A, not moved: the size fields write only when their text differs from `initial` (an Apply without an edit returns before writing); the crop fields have a unit switch (cm, %, px)"),
    ("docx/print_panel.rs", 2, 5, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/quick_parts.rs", 6, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/repair.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/ruby_panel.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/source_manager.rs", 15, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/style_new.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/style_panel.rs", 1, 2, 1, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/tab_panel.rs", 1, 4, 1, "3-A, not moved: one field is reused to add or move any tab stop of a list (the stop is replaced by the typed position); it needs a redesign of the list editor, not a ShownNumber on a field"),
    ("docx/table_formula_panel.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/table_size_panel.rs", 1, 1, 1, "3-A, not moved: writes only the fields whose text differs from `initial`, so an unedited Apply keeps the table"),
    ("docx/template.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/text_box_effects.rs", 1, 1, 8, "3-A, not moved: every field is made from the f64 `to_string()` of the model value, which keeps all its digits, so an unedited Apply does not change it"),
    ("docx/text_box_panel.rs", 1, 3, 2, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("docx/thesaurus.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/alt_text_panel.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/audio_record.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/chart_panel.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/crop_panel.rs", 1, 3, 0, "3-A, not moved: the fields have a unit switch (cm, %, px) and show the first picture only (mixed values: ui-model-binding.md 2.6)"),
    ("shell/find.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/frame_panel.rs", 0, 1, 0, "the size and position panel: the length and angle fields are ShownNumber (2026-11-07); what is left is the scale (%) fields, a ratio of the size the panel was opened with, not a model value"),
    ("shell/glow_panel.rs", 0, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/gradient_panel.rs", 3, 6, 1, "3-A, not moved: the fields show the angle with 8 digits and the stop position with 3 (`{:.8}`, `{:.3}`), which holds every model value (60 000ths, 1 000ths) exactly, so an unedited Apply does not change it; moving it to ShownNumber would change the shown text (trailing zeros), so it moves when the panel is redesigned"),
    ("shell/header_footer.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/insets_panel.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/layout_panel.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/links.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/master_view.rs", 4, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/notes_marks_panel.rs", 3, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/paragraph_panel.rs", 1, 3, 0, "3-A, not moved: takes the first paragraph only (mixed values: ui-model-binding.md 2.6), and its fields show `{:.2}` (`27.00`), which ShownNumber would show as `27`; moves with the mixed-value work"),
    ("pptx/print_panel.rs", 1, 2, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/properties_panel.rs", 10, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/section_panel.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/selection_pane.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/show_panel.rs", 2, 2, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/sidebar_settings.rs", 1, 1, 1, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/size_panel.rs", 3, 1, 0, "the slide size panel: the width and height are ShownNumber (2026-11-07); what is left is the fields with a placeholder (the width and the height are given their text from the ShownNumber) and the first slide number (a whole number, no unit)"),
    ("shell/theme_panel.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/thesaurus_panel.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/threed_and_nav.rs", 1, 1, 3, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("pptx/timing_panel.rs", 2, 1, 0, "3-A, not moved: the fields show seconds with 3 digits (`{:.3}`), which holds every millisecond value exactly, so an unedited Apply does not change it"),
    ("shell/autocorrect.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/autorecover.rs", 2, 2, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/color_editor.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/color_picker_dialog.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/editor.rs", 5, 3, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/ink_panel.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/rename_file.rs", 1, 0, 0, "a text field (the file name of the rename dialog), not a number"),
    ("shell/math_editor.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/password_window.rs", 8, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("shell/replace_fonts.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/advanced_filter.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/alt_text_panel.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/analysis_panel.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/backup_save_panel.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/cell_styles.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/chart_edit.rs", 2, 10, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/chart_series.rs", 1, 6, 2, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/chart_template.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/conditional_edit.rs", 5, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/consolidate.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/control_insert.rs", 1, 5, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/custom_format.rs", 0, 5, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/custom_views.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/data_table.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/dedupe_edit.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/filter.rs", 3, 2, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/find.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/forecast_sheet.rs", 1, 2, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/formula_audit.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/frame_panel.rs", 1, 2, 1, "the size and position panel of a sheet object: the length and angle fields are ShownNumber (2026-11-07); what is left is the outline width and the text size (pt) fields, which write only when the text differs from the one shown (`attrs_initial`)"),
    ("xlsx/function_dialog.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/goal_seek.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/header_footer_edit.rs", 4, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/import_text.rs", 1, 2, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/links.rs", 3, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/move_chart.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/names_panel.rs", 3, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/paste_names_dialog.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/phonetic.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/pivot.rs", 5, 2, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/pivot_calc_field.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/pivot_calc_item.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/pivot_fields.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/pivot_group.rs", 1, 5, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/pivot_options.rs", 6, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/properties_panel.rs", 10, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/protection.rs", 3, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/query_editor.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/repair.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/row_col_size.rs", 0, 0, 2, "the row height and column width panel: the field is ShownNumber (2026-11-07); what is left is `set_selected_row_height` / `set_selected_column_width`, which take a number from a menu or toolbar, not from a field"),
    ("xlsx/scenario_manager.rs", 3, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/selection_pane.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/series.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/sheet_ui.rs", 3, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/slicer_settings.rs", 1, 2, 2, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/sort_edit.rs", 4, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/sparkline_dialog.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/subtotal_dialog.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/table_design.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/table_edit.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/text_columns.rs", 1, 1, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/theme_panel.rs", 2, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/thesaurus_panel.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/validation.rs", 1, 3, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
    ("xlsx/xml_maps.rs", 1, 0, 0, "legacy at the introduction of the lint (2026-11-06): a text/name/formula field, or a number read with a plain parse; moves to ShownNumber when the panel is next touched"),
];

const NUMBER_TYPES: &[&str] = &["f32", "f64", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize"];
const UNIT_KEYS: &[&str] = &["12700", "12_700", "360000", "360_000", "914400", "914_400", "60000", "60_000", "635", "EMU", "emu", "per_pt", "per_unit"];

/// (`InputState::new` calls, bare number parses, unit-conversion rounds) of the production code of `text`.
fn raw_field_counts(text: &str) -> (usize, usize, usize) {
    let production = text.split("#[cfg(test)]").next().unwrap_or(text);
    let inputs = production.matches("InputState::new(").count();
    if !production.contains("InputState") {
        return (inputs, 0, 0);
    }
    let (mut parses, mut rounds) = (0, 0);
    for line in production.lines() {
        if line.trim_start().starts_with("//") { continue; }
        if line.contains(".parse()") || NUMBER_TYPES.iter().any(|ty| line.contains(&format!(".parse::<{ty}>()"))) {
            parses += 1;
        }
        if line.contains(".round()") && UNIT_KEYS.iter().any(|key| line.contains(key)) {
            rounds += 1;
        }
    }
    (inputs, parses, rounds)
}

#[test]
fn no_new_raw_field_reading_in_panels() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    sources(&src, &mut files);
    sources(&shell_src(), &mut files);
    sources(&docx_src(), &mut files);
    sources(&pptx_src(), &mut files);
    sources(&xlsx_src(), &mut files);
    let mut failures = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for file in files {
        let rel = relative(&src, &file);
        if rel.starts_with("shell/chrome/") || rel == "shell/chrome.rs" || rel.starts_with("tests/") { continue; }
        let counts = raw_field_counts(&std::fs::read_to_string(&file).expect("a source"));
        let allowed = RAW_FIELD_ALLOWED.iter().find(|row| row.0 == rel);
        if let Some(row) = allowed { seen.insert(row.0); }
        let (ai, ap, ar) = allowed.map_or((0, 0, 0), |row| (row.1, row.2, row.3));
        if counts == (ai, ap, ar) { continue; }
        if counts.0 > ai || counts.1 > ap || counts.2 > ar {
            failures.push(format!("{rel}: (InputState::new, parse, round) = {counts:?}, allowed {:?}: use `ShownNumber::input` / `ShownNumber::resolve` (a field made with InputState, read with a bare parse, or a converting round must not be added)", (ai, ap, ar)));
        } else {
            failures.push(format!("{rel}: (InputState::new, parse, round) = {counts:?} is below the allowed {:?}: lower the numbers in RAW_FIELD_ALLOWED (the list only shrinks)", (ai, ap, ar)));
        }
    }
    for row in RAW_FIELD_ALLOWED {
        if !seen.contains(row.0) {
            failures.push(format!("{}: listed in RAW_FIELD_ALLOWED but the source has no such file (remove the line)", row.0));
        }
    }
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

#[test]
fn the_raw_field_search_finds_them() {
    let sample = "let a = InputState::new(w, cx);\nlet n: f32 = text.parse::<f32>().unwrap();\nlet e = (v * 12700.0).round() as i64;\n// let x = s.parse().unwrap();\n#[cfg(test)]\nlet b = InputState::new(w, cx);";
    assert_eq!(raw_field_counts(sample), (1, 1, 1));
    assert_eq!(raw_field_counts("let n = s.parse::<f32>();"), (0, 0, 0), "a source without InputState reads no field");
}
