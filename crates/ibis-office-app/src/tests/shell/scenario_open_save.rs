// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Open and save without an edit, by the window's own path (review 0.8.5, 「開いた直後にフィールドの更新などでモデルを書き換える」).
//! `crates/ibis-pptx/tests/roundtrip_corpus.rs` opens the bytes and saves the model as it was read. The window does more as it
//! opens a file (`SlideEditor::load`: fields, tables of contents, indexes, pages, pivots, row heights, dates), so a file nobody
//! edited could be saved as another file, or not be saved. Here every public test file is opened as the window opens it, saved
//! as 保存 saves it, and the saved file is compared with the original part by part. The result goes to
//! `target/roundtrip/app-result.tsv`.
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::open_and_save_without_an_edit_keeps_every_part", open_and_save_without_an_edit_keeps_every_part)]
}

/// (file name, why it differs and where it is tracked): defects not mended yet.
const KNOWN: &[(&str, &str)] = &[];

fn files() -> Vec<std::path::PathBuf> {
    fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["docx", "xlsx", "pptx"].contains(&e.to_ascii_lowercase().as_str())) {
                out.push(path);
            }
        }
    }
    let root = crate::shell::scenario::repo_root().join("test");
    let mut found = Vec::new();
    collect(&root.join("editing"), &mut found);
    collect(&root.join("benchmark"), &mut found);
    for dir in ["docx", "xlsx", "pptx"] {
        collect(&root.join("corpus-report").join(dir), &mut found);
    }
    if let Ok(dirs) = std::env::var("IBIS_ROUNDTRIP_DIRS") {
        for dir in dirs.split(':').filter(|d| !d.is_empty()) {
            collect(std::path::Path::new(dir), &mut found);
        }
    }
    found.sort();
    found.dedup();
    if let Some(only) = std::env::var("IBIS_ROUNDTRIP_ONLY").ok().filter(|o| !o.is_empty()) {
        found.retain(|p| p.to_string_lossy().contains(&only));
    }
    found
}

fn open_and_save_without_an_edit_keeps_every_part() {
    let files = files();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let out = crate::shell::scenario::repo_root().join("target/roundtrip");
    let _ = std::fs::create_dir_all(&out);
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut checked = 0;
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let Ok(original) = std::fs::read(path) else { continue };
        let copy = out.join(format!("app-{name}"));
        let _ = std::fs::copy(path, &copy);
        // As `open_read` hands the file over to the window.
        let Ok(loaded) = crate::read_file(&copy) else { continue };
        checked += 1;
        let shown: gpui_kit::SharedString = name.clone().into();
        s.update(|e, window, cx| {
            e.load(loaded.presentation, shown, window, cx);
            e.set_loaded_source(loaded.source);
            e.set_file_path(Some(copy.clone()));
        });
        s.settle();
        // Under another name: a file with a password to modify refuses to be written over.
        let target = out.join(format!("app-saved-{name}"));
        let saved = s.update(|e, window, _| e.save_to(target.clone(), window));
        let outcome = match saved {
            Err(error) => Some(format!("save: {error}")),
            Ok(()) => {
                let after = std::fs::read(&target).unwrap_or_default();
                match ibis_pptx::ibis_ooxml::canon::diff_packages(&original, &after, &[]) {
                    Ok(report) if report.is_same() => None,
                    Ok(report) => {
                        let names: Vec<String> = report.differences.iter().take(5).map(|(n, _)| n.clone()).collect();
                        Some(format!("{} part(s) differ: {}", report.differences.len(), names.join(" ")))
                    }
                    Err(error) => Some(format!("parts: {error:?}")),
                }
            }
        };
        let _ = std::fs::remove_file(&copy);
        let _ = std::fs::remove_file(&target);
        if let Some(detail) = outcome {
            failures.push((name, detail));
        }
    }
    let table: String = failures.iter().map(|(n, d)| format!("{n}\tfail\t{d}\n")).collect();
    let _ = std::fs::write(out.join("app-result.tsv"), format!("file\tstatus\tdetail\n{table}"));
    eprintln!("open and save: {checked} files checked, {} failed (target/roundtrip/app-result.tsv)", failures.len());
    assert!(checked >= 20 || std::env::var("IBIS_ROUNDTRIP_ONLY").is_ok(), "the public test files were found: {checked}");
    let new: Vec<String> = failures.iter().filter(|(n, _)| !KNOWN.iter().any(|(k, _)| k == n)).map(|(n, d)| format!("{n}: {d}")).collect();
    assert!(new.is_empty(), "files saved as other files without an edit:\n{}", new.join("\n"));
}
