// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #225: `app_export_document` takes `paper` (the name `--print-paper` takes: a3 a4 a5 b4 b5 letter legal tabloid, or `<width>x<height>mm`) and
//! `fit_paper` (false = `--no-print-fit-paper`) and passes them to the program. Design: `tasks/doc/test-design/issue-225-paper.md` (R-9). The program is a stand-in that
//! writes the arguments it was given into the PDF it makes (a shell script, or a batch file on Windows), so what is tested is what the tool asks the program.
//!
//! Each test is `#[ignore]`d until the arguments exist; the implementer takes the `#[ignore]` off.

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

/// A stand-in for `ibisOffice --export pdf --out-dir <dir> <file>`: it writes `export.pdf` (the input is `export.<extension>`) holding its own arguments.
fn stand_in(dir: &Path) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let program = dir.join("fake-ibisoffice.sh");
        std::fs::write(&program, "#!/bin/sh\nout=$4\nprintf '%s\\n' \"$@\" > \"$out/export.pdf\"\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        program
    }
    #[cfg(not(unix))]
    {
        let program = dir.join("fake-ibisoffice.bat");
        std::fs::write(&program, "@echo off\r\nset out=%~4\r\necho %* > \"%out%\\export.pdf\"\r\n").unwrap();
        program
    }
}

fn exported(s: &mut Service, dir: &Path, id: &str, extra: Value) -> Result<String, String> {
    let mut args = json!({ "document_id": id, "format": "pdf", "path": dir.join("out.pdf").to_string_lossy(), "overwrite": true });
    for (k, v) in extra.as_object().unwrap() {
        args[k] = v.clone();
    }
    s.call("app_export_document", &args).map_err(|e| e.code.to_string())?;
    Ok(String::from_utf8_lossy(&std::fs::read(dir.join("out.pdf")).unwrap()).into_owned())
}

/// R-9: the paper goes to the program as `--print-paper <name>`, `fit_paper: false` as `--no-print-fit-paper`; with neither, the arguments are the old ones (no
/// `--print-paper`: the pages are the slides' own size). The names are the CLI's, in either case.
#[test]
fn mcp_export_passes_the_paper_and_the_fit_to_the_program() {
    let dir = sandbox("export_paper", &["pptx-plan.pptx", "autospace-letters.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    s.set_exporter(Some(stand_in(&dir)));
    let p = s.call("app_open_document", &json!({ "path": dir.join("pptx-plan.pptx").to_string_lossy() })).unwrap()["document_id"].as_str().unwrap().to_string();
    let plain = exported(&mut s, &dir, &p, json!({})).unwrap();
    assert!(!plain.contains("--print-paper") && !plain.contains("--no-print-fit-paper"), "no paper given: {plain}");
    for name in ["a3", "a4", "a5", "b4", "b5", "letter", "legal", "tabloid", "210x297mm"] {
        let got = exported(&mut s, &dir, &p, json!({ "paper": name })).unwrap();
        assert!(got.contains("--print-paper") && got.contains(name) && !got.contains("--no-print-fit-paper"), "{name}: {got}");
    }
    let unfit = exported(&mut s, &dir, &p, json!({ "paper": "letter", "fit_paper": false })).unwrap();
    assert!(unfit.contains("--print-paper") && unfit.contains("letter") && unfit.contains("--no-print-fit-paper"), "{unfit}");
    let fit = exported(&mut s, &dir, &p, json!({ "paper": "letter", "fit_paper": true })).unwrap();
    assert!(!fit.contains("--no-print-fit-paper"), "fit_paper true is the default: {fit}");
}

/// R-9: the refusals, none of which runs the program: a name that is not a paper, `fit_paper` without `paper`, a paper for pictures (a PNG has no paper), a paper for a
/// document that is not a presentation (the page of a document or a sheet is its own page setup's).
#[test]
fn mcp_export_refuses_a_paper_that_cannot_be_used() {
    let dir = sandbox("export_paper_errors", &["pptx-plan.pptx", "autospace-letters.docx"]);
    let mut s = Service::new(&[dir.clone()], false, Limits::default());
    s.set_exporter(Some(stand_in(&dir)));
    let p = s.call("app_open_document", &json!({ "path": dir.join("pptx-plan.pptx").to_string_lossy() })).unwrap()["document_id"].as_str().unwrap().to_string();
    let d = s.call("app_open_document", &json!({ "path": dir.join("autospace-letters.docx").to_string_lossy() })).unwrap()["document_id"].as_str().unwrap().to_string();
    for bad in ["a9", "", "A4; rm", "0x0mm", "9999x9999mm"] {
        assert_eq!(exported(&mut s, &dir, &p, json!({ "paper": bad })).unwrap_err(), "invalid_argument", "paper {bad:?}");
    }
    assert_eq!(exported(&mut s, &dir, &p, json!({ "fit_paper": false })).unwrap_err(), "invalid_argument", "fit_paper without paper");
    let png = s.call("app_export_document", &json!({ "document_id": p, "format": "png", "path": dir.join("o.png").to_string_lossy(), "overwrite": true, "paper": "a4" })).unwrap_err().code;
    assert_eq!(png, "invalid_argument", "a PNG has no paper");
    assert_eq!(exported(&mut s, &dir, &d, json!({ "paper": "a4" })).unwrap_err(), "unsupported_operation", "a document's page is its own page setup's");
}
