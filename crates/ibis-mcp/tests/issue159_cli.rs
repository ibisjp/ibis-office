// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #159 (versioned reads), the CLI side (design 3.5): `--call <tool> --args <JSON>` with `--folder`, `--revision`, `--commit`.
//! Test design: `tasks/doc/test-design/159-versioned-read.md`. `#[ignore]` until implemented.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn sandbox(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/mcp-tests").join(format!("i159-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/autospace-letters.docx"), dir.join("a.docx")).unwrap();
    // A repository of its own (the folder is inside the work tree of this repository, whose history lies outside the allowed folder and is not read).
    assert!(Command::new("git").arg("-C").arg(&dir).args(["init", "-q"]).status().unwrap().success());
    dir.canonicalize().unwrap()
}

fn run(args: &[&str]) -> (i32, Value, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ibisOffice-mcp")).args(args).output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    (out.status.code().unwrap(), serde_json::from_str(text.trim()).unwrap_or(Value::Null), String::from_utf8_lossy(&out.stderr).to_string())
}

#[test]
fn i159_cli_a_call_prints_the_result_as_json_with_the_revision() {
    let dir = sandbox("a");
    let d = dir.to_string_lossy().to_string();
    let (code, json, err) = run(&["--allow-dir", &d, "--call", "app_open_document", "--args", r#"{"path":"a.docx"}"#]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(json["revision"]["kind"], "working_tree");
    assert_eq!(json["revision"]["path"], "a.docx");
}

#[test]
fn i159_cli_b_the_flags_take_the_same_names_as_the_tool_arguments() {
    let dir = sandbox("b");
    let d = dir.to_string_lossy().to_string();
    let (code, json, err) = run(&["--allow-dir", &d, "--allow-dir", &d, "--folder", &d, "--revision", "working-tree", "--call", "app_open_document", "--args", r#"{"path":"a.docx"}"#]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(json["revision"]["kind"], "working_tree");
}

#[test]
fn i159_cli_c_a_refused_revision_exits_non_zero_with_the_error_code_on_standard_error() {
    let dir = sandbox("c");
    let d = dir.to_string_lossy().to_string();
    for (extra, code) in [(vec!["--revision", "git", "--commit", "HEAD"], "invalid_argument"), (vec!["--revision", "git", "--commit", "0123456789abcdef0123456789abcdef01234567"], "revision_not_found"), (vec!["--revision", "nope"], "invalid_argument")] {
        let mut args = vec!["--allow-dir", d.as_str(), "--call", "app_open_document", "--args", r#"{"path":"a.docx"}"#];
        args.extend(extra);
        let (status, json, err) = run(&args);
        assert_ne!(status, 0);
        assert!(json.is_null(), "no result on standard output");
        assert!(err.contains(code), "{err}");
    }
}

#[test]
fn i159_cli_d_a_path_outside_the_allowed_folder_is_refused() {
    let dir = sandbox("d");
    let d = dir.to_string_lossy().to_string();
    let (status, _, err) = run(&["--allow-dir", &d, "--call", "app_open_document", "--args", r#"{"path":"../x.docx"}"#]);
    assert_ne!(status, 0);
    assert!(err.contains("not_allowed_path"), "{err}");
}

#[test]
fn i159_cli_e_help_lists_the_new_flags() {
    let out = Command::new(env!("CARGO_BIN_EXE_ibisOffice-mcp")).arg("--help").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    for flag in ["--folder", "--revision", "--commit", "--expected-revision", "--call", "--args"] {
        assert!(text.contains(flag), "{flag}");
    }
}
