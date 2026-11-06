// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The round trip of a whole corpus (review 0.8.5, 検証の仕組み): every `.docx`, `.xlsx` and `.pptx` of the public test files is
//! opened, saved without any edit, and opened again. What must hold, for the three formats alike:
//!   1. the save works;
//!   2. no part of the file is lost, added or changed (`ibis_ooxml::canon::diff_packages`: a file nobody edited is saved as it was);
//!   3. the file read again gives the model it gave the first time.
//!
//! It runs on `test/editing`, `test/benchmark` and `test/corpus-report` by default. `IBIS_ROUNDTRIP_DIRS` (directories separated by
//! `:`) adds others, searched in their subdirectories too: the corpus of a release check (`test/private`, `test/github-issue`).
//! Every file is checked even when one fails (a panic of the reader is one failure of that file), and the result goes to
//! `target/roundtrip/result.tsv` (file, format, step, status, detail). The test fails when a file fails that is not in `KNOWN`
//! (a defect not mended yet, with its reason): a defect is added there only with the reason, and removed when it is mended.
//! `every_known_failure_still_fails` keeps the list honest.
//!
//!   cargo test --release -p ibis-pptx --test roundtrip_corpus -- --nocapture

use std::path::{Path, PathBuf};

use ibis_pptx::SaveOptions;

/// (file name, step, why it fails and where it is tracked)
const KNOWN: &[(&str, &str, &str)] = &[];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test")
}

fn collect(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if recursive {
                collect(&path, true, out);
            }
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["docx", "xlsx", "pptx"].contains(&e.to_ascii_lowercase().as_str())) {
            out.push(path);
        }
    }
}

fn files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in ["editing", "benchmark"] {
        collect(&root().join(dir), false, &mut found);
    }
    for dir in ["docx", "xlsx", "pptx"] {
        collect(&root().join("corpus-report").join(dir), false, &mut found);
    }
    if let Ok(dirs) = std::env::var("IBIS_ROUNDTRIP_DIRS") {
        for dir in dirs.split(':').filter(|d| !d.is_empty()) {
            collect(Path::new(dir), true, &mut found);
        }
    }
    found.sort();
    found.dedup();
    found
}

/// What went wrong with one file: the step and the detail; `None`: it round-trips (or cannot be opened at all: a protected file).
fn check(data: &[u8]) -> Option<(&'static str, String)> {
    let Ok(opened) = ibis_pptx::open(data.to_vec()) else { return None };
    let saved = match opened.source.save(&opened.presentation, SaveOptions::default()) {
        Ok(saved) => saved,
        Err(e) => return Some(("save", format!("{e:?}"))),
    };
    match ibis_ooxml::canon::diff_packages(data, &saved, &[]) {
        Ok(report) if report.is_same() => {}
        Ok(report) => {
            let names: Vec<String> = report.differences.iter().take(5).map(|(n, _)| n.clone()).collect();
            return Some(("parts", format!("{} part(s) differ: {}", report.differences.len(), names.join(" "))));
        }
        Err(e) => return Some(("parts", format!("{e:?}"))),
    }
    let again = match ibis_pptx::open(saved) {
        Ok(again) => again,
        Err(e) => return Some(("reopen", format!("{e:?}"))),
    };
    if again.presentation != opened.presentation {
        return Some(("model", "the file read again gives another model".to_string()));
    }
    None
}

/// The failures of every file: (file name, format, step, detail).
fn run() -> (usize, Vec<(String, String, &'static str, String)>) {
    let files = files();
    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let format = path.extension().unwrap().to_string_lossy().to_ascii_lowercase();
        let Ok(data) = std::fs::read(path) else { continue };
        let outcome = std::panic::catch_unwind(|| check(&data)).unwrap_or_else(|_| Some(("panic", "the reader or the writer panicked".to_string())));
        if let Some((step, detail)) = outcome {
            failures.push((name, format, step, detail));
        }
    }
    let table: String = failures.iter().map(|(n, f, s, d)| format!("{n}\t{f}\t{s}\tfail\t{d}\n")).collect();
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/roundtrip");
    let _ = std::fs::create_dir_all(&out);
    let _ = std::fs::write(out.join("result.tsv"), format!("file\tformat\tstep\tstatus\tdetail\n{table}"));
    eprintln!("round trip: {} files checked, {} failed (target/roundtrip/result.tsv)", files.len(), failures.len());
    (files.len(), failures)
}

#[test]
fn every_file_round_trips_without_an_edit() {
    let (checked, failures) = run();
    assert!(checked >= 20, "the public test files were found: {checked}");
    let new: Vec<String> = failures
        .iter()
        .filter(|(n, _, s, _)| !KNOWN.iter().any(|(kn, ks, _)| kn == n && ks == s))
        .map(|(n, f, s, d)| format!("{n} ({f}): {s}: {d}"))
        .collect();
    assert!(new.is_empty(), "files that do not round-trip:\n{}", new.join("\n"));
}

#[test]
fn every_known_failure_still_fails() {
    let (_, failures) = run();
    let mended: Vec<&str> = KNOWN.iter().filter(|(n, s, _)| !failures.iter().any(|(fn_, _, fs, _)| fn_ == n && fs == s)).map(|(n, _, _)| *n).collect();
    assert!(mended.is_empty(), "mended: take these out of KNOWN: {mended:?}");
}

/// The comparison is not blind: a part changed by one byte is reported.
#[test]
fn a_changed_part_is_seen() {
    let data = std::fs::read(root().join("editing/docx-report.docx")).unwrap();
    let mut package = ibis_ooxml::Package::open(data.clone()).unwrap();
    let mut xml = package.read("word/document.xml").unwrap().to_vec();
    let at = xml.iter().position(|b| *b == b'>').unwrap();
    xml.insert(at + 1, b'x');
    package.set_bytes("word/document.xml", xml);
    let changed = package.save(ibis_ooxml::SaveOptions::default()).unwrap();
    assert!(!ibis_ooxml::canon::diff_packages(&data, &changed, &[]).unwrap().is_same());
}
