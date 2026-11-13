// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The direction of the dependencies between the ibisOffice crates (tasks/doc/crate-split-plan.md, 5.1): each crate may depend only
//! on the ones below it. Edit `ALLOWED` when a crate is added or a stage of the split moves one; a dependency that is not listed
//! (above all, a lower crate depending on a higher one, or a format crate on another) fails here.
//!
//!   ibis-office-app  ->  (docx, xlsx and pptx are the crates ibis-office-docx, ibis-office-xlsx and ibis-office-pptx)  ->  ibis-office-shell  ->  ibis-office-render  ->  ibis-office-base
//!
//! The crates of the document model and the file formats (`ibis-doc-model`, `ibis-pptx`, `ibis-calc`, ...) are below all of them
//! and are not checked here.
//!
//! Until the formats are crates of their own, the same direction holds between the directories of `ibis-office-app/src`
//! (stage 2 of the plan): a format names no other format. (The shell, a crate since stage 4, cannot name the formats at all.)
//! `the_layers_inside_the_app_name_only_the_ones_below` reads the sources for paths that break it. A format reaches the shell
//! through its `FormatEditor` (the hooks of `shell/format.rs`), not the other way round.

use std::path::Path;

/// (crate, the ibis-office-* crates it may depend on). The updater and the AppImage runtime are programs of their own.
const ALLOWED: &[(&str, &[&str])] = &[
    ("ibis-office-base", &[]),
    ("ibis-office-render", &["ibis-office-base"]),
    ("ibis-office-shell", &["ibis-office-base", "ibis-office-render", "ibis-office-updater"]),
    // A format's crate: the shell and the crates below it, and the attribute macro; never another format (5.1).
    ("ibis-office-docx", &["ibis-office-base", "ibis-office-render", "ibis-office-shell", "ibis-office-macros"]),
    ("ibis-office-pptx", &["ibis-office-base", "ibis-office-render", "ibis-office-shell", "ibis-office-macros"]),
    // The workbook screen (XLSX), cut out in stage 5 (tasks/doc/crate-split-plan.md, 16): a format on the shell, naming no other format.
    ("ibis-office-xlsx", &["ibis-office-base", "ibis-office-render", "ibis-office-shell", "ibis-office-macros"]),
    ("ibis-office-app", &["ibis-office-base", "ibis-office-render", "ibis-office-shell", "ibis-office-docx", "ibis-office-pptx", "ibis-office-xlsx", "ibis-office-updater", "ibis-office-macros"]),
    // The attribute macro `#[editor_ext]`: built for the compiler, it depends on no crate of ibisOffice.
    ("ibis-office-macros", &[]),
    ("ibis-office-updater", &[]),
];

/// The names of the `ibis-office-*` crates a manifest lists as dependencies (any section whose name has "dependencies" in it).
fn office_dependencies(manifest: &str) -> Vec<String> {
    let mut in_dependencies = false;
    let mut found = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_dependencies = line.contains("dependencies");
            continue;
        }
        if !in_dependencies || line.starts_with('#') {
            continue;
        }
        if let Some((name, _)) = line.split_once('=') {
            let name = name.trim().split('.').next().unwrap_or("").trim();
            if name.starts_with("ibis-office-") && !found.contains(&name.to_string()) {
                found.push(name.to_string());
            }
        }
    }
    found
}

#[test]
fn crates_depend_only_downwards() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut problems = Vec::new();
    let mut seen = Vec::new();
    for entry in std::fs::read_dir(crates).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(manifest) = std::fs::read_to_string(entry.path().join("Cargo.toml")) else { continue };
        if !name.starts_with("ibis-office-") {
            continue;
        }
        seen.push(name.clone());
        let Some((_, allowed)) = ALLOWED.iter().find(|(c, _)| *c == name) else {
            problems.push(format!("{name}: not in ALLOWED (tests/crate_layering.rs): say what it may depend on"));
            continue;
        };
        for dependency in office_dependencies(&manifest) {
            if dependency != name && !allowed.contains(&dependency.as_str()) {
                problems.push(format!("{name} depends on {dependency}, which the layering does not allow (tasks/doc/crate-split-plan.md, 5.1)"));
            }
        }
    }
    for (name, _) in ALLOWED {
        if !seen.iter().any(|s| s == name) {
            problems.push(format!("{name} is in ALLOWED but there is no such crate"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_reading_of_a_manifest_finds_the_dependencies() {
    let manifest = "[package]\nname = \"x\"\n[dependencies]\nibis-office-base.workspace = true\n# ibis-office-app = 1\n[target.'cfg(windows)'.dependencies]\nibis-office-render = { path = \"..\" }\n[features]\nibis-office-app = []\n";
    assert_eq!(office_dependencies(manifest), vec!["ibis-office-base", "ibis-office-render"]);
}

/// (directory of `ibis-office-app/src`, the directories its code must not name). The files at the top of `src` (the exports, the
/// registration of the formats) are the app itself and may name all of them; `src/tests` is test code.
const LAYERS: &[(&str, &[&str])] = &[];

/// Whether `line` names the module `module` of this crate: `crate::module` or `super::module` not followed by more of a name
/// (`crate::pptx_theme` is another module). Comments are not code.
fn names_module(line: &str, module: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") {
        return false;
    }
    ["crate::", "super::"].iter().any(|root| {
        let path = format!("{root}{module}");
        code.match_indices(&path).any(|(at, _)| !code[at + path.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_'))
    })
}

/// The `.rs` files under `dir`, in the order of their paths.
fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_layers_inside_the_app_name_only_the_ones_below() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut problems = Vec::new();
    for (layer, forbidden) in LAYERS {
        let mut files = Vec::new();
        rust_files(&src.join(layer), &mut files);
        assert!(!files.is_empty(), "src/{layer} has no source files: update LAYERS (tests/crate_layering.rs)");
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for (number, line) in text.lines().enumerate() {
                for module in *forbidden {
                    if names_module(line, module) {
                        let shown = file.strip_prefix(&src).unwrap_or(&file).display();
                        problems.push(format!("{shown}:{}: {layer} names {module} (tasks/doc/crate-split-plan.md, 5.1): {}", number + 1, line.trim()));
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_reading_of_a_line_finds_the_named_module() {
    assert!(names_module("    let x = crate::docx::flow_layout::pages(p);", "docx"));
    assert!(names_module("use super::super::xlsx::State;", "xlsx"));
    assert!(names_module("use crate::pptx;", "pptx"));
    assert!(!names_module("use crate::pptx_theme::Theme;", "pptx"));
    assert!(!names_module("    // see crate::docx::flow_layout", "docx"));
    assert!(!names_module("let kind = ibis_pptx::preset_geometry(name);", "pptx"));
}
