// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #159 (versioned reads): design `tasks/doc/design/159-versioned-read.md`, test design `tasks/doc/test-design/159-versioned-read.md`.
//! Every test here is `#[ignore]` until the feature exists; the implementer removes the attribute and makes it pass.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use ibis_doc_service::{Access, Limits, Service, ToolError, catalog};
use serde_json::{Value, json};

/// Tests that start `git` or change the environment take this lock (the environment is process-wide).
static GIT: Mutex<()> = Mutex::new(());

fn editing(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)
}

/// A folder of its own under `target/` with the sample files copied in.
fn sandbox(test: &str, files: &[&str]) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/doc-service-tests").join(format!("i159-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in files {
        std::fs::copy(editing(f), dir.join(f)).unwrap();
    }
    dir.canonicalize().unwrap()
}

fn service(dirs: &[&Path]) -> Service {
    let dirs: Vec<PathBuf> = dirs.iter().map(|d| d.to_path_buf()).collect();
    Service::new(&dirs, false, Limits::default())
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("git must be installed to run these tests");
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// A repository in `dir` with two commits of `name`: the first is the sample, the second has "SECOND-" put in front of the first paragraph.
/// Returns (first SHA, second SHA).
fn two_commits(dir: &Path, name: &str) -> (String, String) {
    git(dir, &["init", "-q"]);
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", "first"]);
    let first = git(dir, &["rev-parse", "HEAD"]);
    let mut s = service(&[dir]);
    let id = open_path(&mut s, &dir.join(name))["document_id"].as_str().unwrap().to_string();
    s.call("docx_insert_text", &json!({ "document_id": id, "at": { "paragraph": 0, "offset": 0 }, "text": "SECOND-" })).unwrap();
    s.call("app_save_document", &json!({ "document_id": id, "path": dir.join(name).to_string_lossy(), "overwrite": true })).unwrap();
    git(dir, &["commit", "-q", "-am", "second"]);
    (first, git(dir, &["rev-parse", "HEAD"]))
}

fn open_path(s: &mut Service, path: &Path) -> Value {
    s.call("app_open_document", &json!({ "path": path.to_string_lossy() })).unwrap()
}

fn open_rev(s: &mut Service, args: Value) -> Result<Value, ToolError> {
    s.call("app_open_document", &args)
}

fn first_paragraph(s: &mut Service, id: &str) -> String {
    s.call("docx_get_paragraph", &json!({ "document_id": id, "paragraph": 0 })).unwrap()["text"].as_str().unwrap().to_string()
}

fn sha256_hex(path: &Path) -> String {
    // The platform's own tool, so the check does not depend on the code under test.
    if cfg!(windows) {
        let out = Command::new("certutil").arg("-hashfile").arg(path).arg("SHA256").output().unwrap();
        String::from_utf8_lossy(&out.stdout).lines().nth(1).unwrap().replace(' ', "").to_lowercase()
    } else {
        let out = Command::new("shasum").args(["-a", "256"]).arg(path).output().unwrap();
        String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap().to_string()
    }
}

fn insert(s: &mut Service, id: &str, text: &str) -> Value {
    s.call("docx_insert_text", &json!({ "document_id": id, "at": { "paragraph": 0, "offset": 0 }, "text": text })).unwrap()
}

const DOC: &str = "autospace-letters.docx";

// ---- T1 relative paths --------------------------------------------------------------------------------------------------------------------------

#[test]
fn i159_t1a_a_relative_path_opens_from_the_folder() {
    let dir = sandbox("t1a", &[DOC]);
    let mut s = service(&[&dir]);
    let r = open_rev(&mut s, json!({ "folder": dir.to_string_lossy(), "path": DOC })).unwrap();
    assert!(r["document_id"].is_string());
    assert_eq!(r["revision"]["path"], DOC);
}

#[test]
fn i159_t1b_a_relative_path_cannot_leave_the_folder() {
    let dir = sandbox("t1b", &[DOC]);
    let inner = dir.join("inner");
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::copy(editing(DOC), inner.join(DOC)).unwrap();
    let outside = sandbox("t1b-outside", &[DOC]);
    let mut s = service(&[&inner]);
    let e = open_rev(&mut s, json!({ "folder": inner.to_string_lossy(), "path": format!("../{DOC}") })).unwrap_err();
    assert_eq!(e.code, "not_allowed_path");
    let e = open_rev(&mut s, json!({ "folder": inner.to_string_lossy(), "path": outside.join(DOC).to_string_lossy() })).unwrap_err();
    assert_eq!(e.code, "not_allowed_path");
    // a folder that is not an allowed folder is refused too
    let e = open_rev(&mut s, json!({ "folder": dir.to_string_lossy(), "path": DOC })).unwrap_err();
    assert_eq!(e.code, "not_allowed_path");
}

#[test]
fn i159_t1c_two_allowed_folders_without_folder_is_ambiguous_and_one_folder_may_omit_it() {
    let a = sandbox("t1c-a", &[DOC]);
    let b = sandbox("t1c-b", &[DOC]);
    let mut two = service(&[&a, &b]);
    assert_eq!(open_rev(&mut two, json!({ "path": DOC })).unwrap_err().code, "ambiguous_folder");
    assert!(open_rev(&mut two, json!({ "folder": b.to_string_lossy(), "path": DOC })).is_ok());
    let mut one = service(&[&a]);
    assert!(open_rev(&mut one, json!({ "path": DOC })).is_ok());
}

#[test]
fn i159_t1d_an_absolute_path_still_works_as_before() {
    let dir = sandbox("t1d", &[DOC]);
    let mut s = service(&[&dir]);
    assert!(s.call("app_open_document", &json!({ "path": dir.join(DOC).to_string_lossy() })).is_ok());
}

// ---- T2 working tree ----------------------------------------------------------------------------------------------------------------------------

#[test]
fn i159_t2a_the_response_names_the_working_tree_and_the_real_file_hash() {
    let dir = sandbox("t2a", &[DOC]);
    let mut s = service(&[&dir]);
    let r = open_path(&mut s, &dir.join(DOC));
    assert_eq!(r["revision"]["kind"], "working_tree");
    assert_eq!(r["revision"]["file_sha256"], sha256_hex(&dir.join(DOC)));
    assert_eq!(r["revision"]["dirty"], false);
    let state = r["revision"]["state"].as_str().unwrap();
    assert!(r["revision"]["file_sha256"].as_str().unwrap().starts_with(state));
    // the same revision appears in app_get_document_info
    let info = s.call("app_get_document_info", &json!({ "document_id": r["document_id"] })).unwrap();
    assert_eq!(info["revision"]["file_sha256"], r["revision"]["file_sha256"]);
}

#[test]
fn i159_t2b_a_file_changed_after_opening_is_not_read_and_the_revision_says_which_was_read() {
    let dir = sandbox("t2b", &[DOC]);
    let mut s = service(&[&dir]);
    let hash = sha256_hex(&dir.join(DOC));
    let r = open_path(&mut s, &dir.join(DOC));
    let id = r["document_id"].as_str().unwrap().to_string();
    let before = first_paragraph(&mut s, &id);
    std::fs::write(dir.join(DOC), b"not a docx any more").unwrap();
    let again = s.call("docx_get_paragraph", &json!({ "document_id": id, "paragraph": 0 })).unwrap();
    assert_eq!(again["text"], before);
    assert_eq!(again["document_id"], id);
    assert_eq!(again["state"], r["revision"]["state"]);
    assert_eq!(r["revision"]["file_sha256"], hash);
}

#[test]
fn i159_t2c_every_read_tool_result_carries_document_id_and_state() {
    let dir = sandbox("t2c", &[DOC, "xlsx-sales.xlsx", "pptx-plan.pptx"]);
    let mut s = service(&[&dir]);
    let d = open_path(&mut s, &dir.join(DOC))["document_id"].as_str().unwrap().to_string();
    let x = open_path(&mut s, &dir.join("xlsx-sales.xlsx"))["document_id"].as_str().unwrap().to_string();
    let p = open_path(&mut s, &dir.join("pptx-plan.pptx"))["document_id"].as_str().unwrap().to_string();
    let sheet = s.call("xlsx_list_sheets", &json!({ "document_id": x })).unwrap()["sheets"][0]["name"].as_str().unwrap().to_string();
    for (id, tool, args) in [
        (&d, "docx_get_paragraph", json!({ "paragraph": 0 })),
        (&x, "xlsx_get_cells", json!({ "sheet": sheet, "range": "A1:B2" })),
        (&p, "pptx_get_slide", json!({ "slide": 1 })),
    ] {
        let mut a = args;
        a["document_id"] = json!(id);
        let r = s.call(tool, &a).unwrap();
        assert!(r["state"].is_string(), "{tool}: {r}");
        assert_eq!(&r["document_id"], id, "{tool}");
    }
}

// ---- T3 git -------------------------------------------------------------------------------------------------------------------------------------

#[test]
fn i159_t3a_an_old_commit_is_read_and_the_full_sha_is_returned() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("t3a", &[DOC]);
    let (first, second) = two_commits(&dir, DOC);
    let mut s = service(&[&dir]);
    let old = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": first } })).unwrap();
    let new = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": second } })).unwrap();
    let (old_id, new_id) = (old["document_id"].as_str().unwrap().to_string(), new["document_id"].as_str().unwrap().to_string());
    assert_eq!(old["revision"]["commit"], first);
    assert_eq!(old["revision"]["kind"], "git");
    assert!(first_paragraph(&mut s, &new_id).starts_with("SECOND-"));
    assert!(!first_paragraph(&mut s, &old_id).starts_with("SECOND-"));
    // the hash is of the committed bytes, not of the working tree
    assert_ne!(old["revision"]["file_sha256"], new["revision"]["file_sha256"]);
    assert_eq!(new["revision"]["file_sha256"], sha256_hex(&dir.join(DOC)));
}

#[test]
fn i159_t3b_a_short_sha_is_accepted_when_unique_and_answered_in_full() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("t3b", &[DOC]);
    let (first, _) = two_commits(&dir, DOC);
    let mut s = service(&[&dir]);
    let r = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": &first[..10] } })).unwrap();
    assert_eq!(r["revision"]["commit"], first);
    // 3 characters is below the 4-character minimum of the design
    let e = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": &first[..3] } })).unwrap_err();
    assert_eq!(e.code, "invalid_argument");
}

#[test]
fn i159_t3c_names_and_option_like_values_are_refused() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("t3c", &[DOC]);
    two_commits(&dir, DOC);
    git(&dir, &["branch", "feature"]);
    git(&dir, &["tag", "v1"]);
    let mut s = service(&[&dir]);
    let long = "f".repeat(65);
    for bad in ["HEAD", "HEAD~1", "main", "master", "feature", "v1", "--help", "-x", "abc;rm", "a b", "", long.as_str()] {
        let e = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": bad } })).unwrap_err();
        assert_eq!(e.code, "invalid_argument", "commit {bad:?}");
    }
}

#[test]
fn i159_t3d_a_missing_commit_is_revision_not_found_and_a_missing_file_is_refused() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("t3d", &[DOC]);
    two_commits(&dir, DOC);
    let mut s = service(&[&dir]);
    let e = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": "0123456789abcdef0123456789abcdef01234567" } })).unwrap_err();
    assert_eq!(e.code, "revision_not_found");
    let first = git(&dir, &["rev-list", "--max-parents=0", "HEAD"]);
    let e = open_rev(&mut s, json!({ "path": "missing.docx", "revision": { "kind": "git", "commit": first } })).unwrap_err();
    assert!(e.code == "file_not_found" || e.code == "revision_not_found", "{}", e.code);
}

#[test]
fn i159_t3e_a_repository_outside_the_allowed_folder_is_not_read() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    // The repository root is the parent of the allowed folder: its history must not be read through the allowed child.
    let root = sandbox("t3e", &[]);
    let child = root.join("sub");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::copy(editing(DOC), child.join(DOC)).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "x"]);
    let sha = git(&root, &["rev-parse", "HEAD"]);
    let mut s = service(&[&child]);
    let e = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": sha } })).unwrap_err();
    assert!(e.code == "not_allowed_path" || e.code == "unsupported_revision", "{}", e.code);
}

#[test]
fn i159_t3f_a_git_revision_cannot_overwrite_its_path_but_can_be_saved_under_another_name() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("t3f", &[DOC]);
    let (first, _) = two_commits(&dir, DOC);
    let mut s = service(&[&dir]);
    let id = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": first } })).unwrap()["document_id"].as_str().unwrap().to_string();
    let before = std::fs::read(dir.join(DOC)).unwrap();
    for overwrite in [json!(true), json!("force")] {
        let e = s.call("app_save_document", &json!({ "document_id": id, "path": dir.join(DOC).to_string_lossy(), "overwrite": overwrite })).unwrap_err();
        assert_eq!(e.code, "read_only");
    }
    assert_eq!(std::fs::read(dir.join(DOC)).unwrap(), before);
    let other = dir.join("copy.docx");
    assert_eq!(s.call("app_save_document", &json!({ "document_id": id, "path": other.to_string_lossy() })).unwrap()["saved"], true);
    assert!(other.exists());
}

#[test]
fn i159_t3g_without_a_git_command_the_error_is_git_not_available() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("t3g", &[DOC]);
    let (first, _) = two_commits(&dir, DOC);
    let mut s = service(&[&dir]);
    let saved = std::env::var_os("PATH");
    let empty = dir.join("empty-path");
    std::fs::create_dir_all(&empty).unwrap();
    // SAFETY: the GIT lock serialises every test that starts git or reads the environment for it.
    unsafe { std::env::set_var("PATH", &empty) };
    let r = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": first } }));
    if let Some(p) = saved {
        unsafe { std::env::set_var("PATH", p) };
    }
    assert_eq!(r.unwrap_err().code, "git_not_available");
}

#[test]
fn i159_t3h_git_is_started_without_a_shell_and_ignores_the_callers_git_dir() {
    // A file name made of shell metacharacters is one argument; GIT_DIR from the caller's environment must not redirect the read.
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("t3h", &[]);
    let name = "a & b ; c.docx";
    std::fs::copy(editing(DOC), dir.join(name)).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", name]);
    git(&dir, &["commit", "-q", "-m", "x"]);
    let sha = git(&dir, &["rev-parse", "HEAD"]);
    let decoy = sandbox("t3h-decoy", &[]);
    git(&decoy, &["init", "-q"]);
    // SAFETY: serialised by the GIT lock.
    unsafe { std::env::set_var("GIT_DIR", decoy.join(".git")) };
    let mut s = service(&[&dir]);
    let r = open_rev(&mut s, json!({ "path": name, "revision": { "kind": "git", "commit": sha } }));
    unsafe { std::env::remove_var("GIT_DIR") };
    assert_eq!(r.unwrap()["revision"]["commit"], sha);
}

// ---- T4 state -----------------------------------------------------------------------------------------------------------------------------------

#[test]
fn i159_t4_state_changes_with_edits_returns_on_undo_and_is_the_same_for_the_same_edits() {
    let dir = sandbox("t4", &[DOC]);
    let mut s = service(&[&dir]);
    let a = open_path(&mut s, &dir.join(DOC));
    let b = open_path(&mut s, &dir.join(DOC));
    let (ia, ib) = (a["document_id"].as_str().unwrap().to_string(), b["document_id"].as_str().unwrap().to_string());
    let s0 = a["revision"]["state"].clone();
    assert_eq!(s0, b["revision"]["state"], "two opens of the same file start from the same state");
    let s1 = insert(&mut s, &ia, "XY")["state"].clone();
    assert_ne!(s1, s0);
    assert_eq!(insert(&mut s, &ib, "XY")["state"], s1, "same edit sequence, same state");
    assert_ne!(insert(&mut s, &ib, "Z")["state"], s1, "a different edit, a different state");
    let u = s.call("app_undo_document", &json!({ "document_id": ia })).unwrap();
    assert_eq!(u["state"], s0, "undo returns to the opening state");
    assert_eq!(s.call("app_get_document_info", &json!({ "document_id": ia })).unwrap()["revision"]["dirty"], false);
    assert_eq!(s.call("app_get_document_info", &json!({ "document_id": ib })).unwrap()["revision"]["dirty"], true);
}

// ---- T5 expected_revision -----------------------------------------------------------------------------------------------------------------------

#[test]
fn i159_t5a_a_matching_expected_revision_applies_and_returns_the_new_state() {
    let dir = sandbox("t5a", &[DOC]);
    let mut s = service(&[&dir]);
    let r = open_path(&mut s, &dir.join(DOC));
    let id = r["document_id"].as_str().unwrap().to_string();
    let out = s.call("docx_insert_text", &json!({ "document_id": id, "expected_revision": r["revision"]["state"], "at": { "paragraph": 0, "offset": 0 }, "text": "OK-" })).unwrap();
    assert!(first_paragraph(&mut s, &id).starts_with("OK-"));
    assert_ne!(out["state"], r["revision"]["state"]);
}

#[test]
fn i159_t5b_a_stale_expected_revision_changes_nothing() {
    let dir = sandbox("t5b", &[DOC]);
    let mut s = service(&[&dir]);
    let r = open_path(&mut s, &dir.join(DOC));
    let id = r["document_id"].as_str().unwrap().to_string();
    let stale = r["revision"]["state"].clone();
    let current = insert(&mut s, &id, "FIRST-")["state"].clone();
    let text = first_paragraph(&mut s, &id);
    let e = s.call("docx_insert_text", &json!({ "document_id": id, "expected_revision": stale, "at": { "paragraph": 0, "offset": 0 }, "text": "LATE-" })).unwrap_err();
    assert_eq!(e.code, "revision_mismatch");
    assert_eq!(first_paragraph(&mut s, &id), text, "the document is unchanged");
    // the error says both states; the refused call left no undo step
    assert!(e.message.contains(current.as_str().unwrap()) && e.message.contains(stale.as_str().unwrap()));
    s.call("app_undo_document", &json!({ "document_id": id })).unwrap();
    assert!(!first_paragraph(&mut s, &id).starts_with("FIRST-"));
}

#[test]
fn i159_t5c_omitting_expected_revision_behaves_as_before() {
    let dir = sandbox("t5c", &[DOC]);
    let mut s = service(&[&dir]);
    let id = open_path(&mut s, &dir.join(DOC))["document_id"].as_str().unwrap().to_string();
    insert(&mut s, &id, "A");
    insert(&mut s, &id, "B"); // the second edit follows a changed state; with no expected_revision it must still apply
    assert!(first_paragraph(&mut s, &id).starts_with("BA"));
}

#[test]
fn i159_t5d_every_edit_tool_in_the_catalog_has_expected_revision_in_its_schema() {
    // The catalog is the single place (design 7.7).
    for tool in catalog() {
        if tool.access == Access::Read || matches!(tool.name, "app_open_document" | "app_close_document" | "app_export_document") {
            continue;
        }
        let schema = (tool.schema)();
        assert!(schema["properties"].get("expected_revision").is_some(), "{} has no expected_revision", tool.name);
    }
}

#[test]
fn i159_t5e_save_checks_the_file_hash_and_the_older_check_stays() {
    let dir = sandbox("t5e", &[DOC]);
    let mut s = service(&[&dir]);
    let r = open_path(&mut s, &dir.join(DOC));
    let id = r["document_id"].as_str().unwrap().to_string();
    let path = dir.join(DOC);
    insert(&mut s, &id, "E-");
    // the hash of the file that was read: it matches while the file is unchanged
    let ok = s.call("app_save_document", &json!({ "document_id": id, "path": path.to_string_lossy(), "overwrite": true, "expected_revision": r["revision"]["file_sha256"] })).unwrap();
    assert_eq!(ok["saved"], true);
    // a wrong hash: nothing is written
    let bytes = std::fs::read(&path).unwrap();
    insert(&mut s, &id, "F-");
    let e = s.call("app_save_document", &json!({ "document_id": id, "path": path.to_string_lossy(), "overwrite": true, "expected_revision": "0".repeat(64) })).unwrap_err();
    assert_eq!(e.code, "revision_mismatch");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    // force does not skip expected_revision (design 3.3)
    let e = s.call("app_save_document", &json!({ "document_id": id, "path": path.to_string_lossy(), "overwrite": "force", "expected_revision": "0".repeat(64) })).unwrap_err();
    assert_eq!(e.code, "revision_mismatch");
    // without expected_revision the older check against the opening time still works
    std::fs::write(&path, b"changed by another program").unwrap();
    let e = s.call("app_save_document", &json!({ "document_id": id, "path": path.to_string_lossy(), "overwrite": true })).unwrap_err();
    assert_eq!(e.code, "file_changed_on_disk");
}

#[test]
fn i159_t5f_one_call_with_several_changes_is_checked_once_and_is_one_undo_step() {
    let dir = sandbox("t5f", &[DOC]);
    let mut s = service(&[&dir]);
    let r = open_path(&mut s, &dir.join(DOC));
    let id = r["document_id"].as_str().unwrap().to_string();
    let before = first_paragraph(&mut s, &id);
    let letter = before.chars().next().unwrap().to_string();
    let out = s.call("docx_replace_text", &json!({ "document_id": id, "expected_revision": r["revision"]["state"], "find": letter, "replace": "Q", "all": true })).unwrap();
    assert!(out["state"].is_string());
    s.call("app_undo_document", &json!({ "document_id": id })).unwrap();
    assert_eq!(first_paragraph(&mut s, &id), before);
}

// ---- T6 open_document ---------------------------------------------------------------------------------------------------------------------------

#[test]
fn i159_t6a_open_document_revision_copies_the_edited_unsaved_state_into_a_new_document() {
    let dir = sandbox("t6a", &[DOC]);
    let mut s = service(&[&dir]);
    let id = open_path(&mut s, &dir.join(DOC))["document_id"].as_str().unwrap().to_string();
    let edited = insert(&mut s, &id, "UNSAVED-");
    let copy = open_rev(&mut s, json!({ "revision": { "kind": "open_document", "document_id": id } })).unwrap();
    let cid = copy["document_id"].as_str().unwrap().to_string();
    assert_ne!(cid, id);
    assert_eq!(copy["revision"]["kind"], "open_document");
    assert_eq!(copy["revision"]["state"], edited["state"]);
    assert!(first_paragraph(&mut s, &cid).starts_with("UNSAVED-"));
    // the copy is independent
    insert(&mut s, &cid, "ONLY-COPY-");
    assert!(!first_paragraph(&mut s, &id).starts_with("ONLY-COPY-"));
}

#[test]
fn i159_t6b_a_document_that_this_server_does_not_have_is_refused() {
    let dir = sandbox("t6b", &[DOC]);
    let mut s = service(&[&dir]);
    let e = open_rev(&mut s, json!({ "revision": { "kind": "open_document", "document_id": "d-no-such" } })).unwrap_err();
    assert!(e.code == "no_such_document" || e.code == "unsupported_revision", "{}", e.code);
}

// ---- T7 catalog / schema ------------------------------------------------------------------------------------------------------------------------

#[test]
fn i159_t7_the_open_tool_schema_lists_folder_and_revision_with_their_kinds() {
    let open = catalog().iter().find(|t| t.name == "app_open_document").unwrap();
    let schema = (open.schema)();
    let props = &schema["properties"];
    for key in ["folder", "path", "revision"] {
        assert!(props.get(key).is_some(), "{key}");
    }
    let text = props["revision"].to_string();
    for kind in ["working_tree", "git", "open_document", "commit"] {
        assert!(text.contains(kind), "{kind} in {text}");
    }
}

// ---- W1-W3 Windows (test design 3) ---------------------------------------------------------------------------------------------------------------

#[cfg(windows)]
#[test]
fn i159_w1_the_same_folder_in_other_spellings_reads_the_same_commit() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("w1", &[DOC]);
    let (first, _) = two_commits(&dir, DOC);
    let plain = dir.to_string_lossy().trim_start_matches(r"\\?\").to_string(); // C:\...\i159-w1
    let slash = |text: &str| text.replace('\\', "/");
    let spellings = [plain.clone(), slash(&plain), plain.to_lowercase(), slash(&plain.to_lowercase()), dir.to_string_lossy().to_string()];
    let mut hashes = Vec::new();
    for spelling in &spellings {
        let mut s = service(&[&dir]);
        // a folder and a relative path, and the absolute path of the same file
        let a = open_rev(&mut s, json!({ "folder": spelling, "path": DOC, "revision": { "kind": "git", "commit": first } })).unwrap_or_else(|e| panic!("{spelling}: {e}"));
        let b = open_rev(&mut s, json!({ "path": format!("{spelling}/{DOC}"), "revision": { "kind": "git", "commit": first } })).unwrap_or_else(|e| panic!("{spelling}: {e}"));
        assert_eq!(a["revision"]["path"], DOC);
        assert_eq!(a["revision"]["file_sha256"], b["revision"]["file_sha256"]);
        hashes.push(a["revision"]["file_sha256"].clone());
    }
    assert!(hashes.windows(2).all(|w| w[0] == w[1]), "{hashes:?}");
}

#[cfg(windows)]
#[test]
fn i159_w2_the_missing_git_message_says_to_install_git() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("w2", &[DOC]);
    let (first, _) = two_commits(&dir, DOC);
    let mut s = service(&[&dir]);
    let saved = std::env::var_os("PATH");
    let empty = dir.join("empty-path");
    std::fs::create_dir_all(&empty).unwrap();
    // SAFETY: serialised by the GIT lock.
    unsafe { std::env::set_var("PATH", &empty) };
    let r = open_rev(&mut s, json!({ "path": DOC, "revision": { "kind": "git", "commit": first } }));
    if let Some(p) = saved {
        unsafe { std::env::set_var("PATH", p) };
    }
    let e = r.unwrap_err();
    assert_eq!(e.code, "git_not_available");
    assert!(e.message.contains("install Git") && e.message.contains("PATH"), "{}", e.message);
}

#[cfg(windows)]
#[test]
fn i159_w3_japanese_and_spaces_in_the_folder_and_file_names_are_read() {
    let _g = GIT.lock().unwrap_or_else(|e| e.into_inner());
    let dir = sandbox("w3", &[]).join("ibis 試験");
    std::fs::create_dir_all(&dir).unwrap();
    let name = "報告 q3.docx";
    std::fs::copy(editing(DOC), dir.join(name)).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["add", name]);
    git(&dir, &["commit", "-q", "-m", "x"]);
    let sha = git(&dir, &["rev-parse", "HEAD"]);
    let mut s = service(&[&dir]);
    let r = open_rev(&mut s, json!({ "path": name, "revision": { "kind": "git", "commit": sha } })).unwrap();
    assert_eq!(r["revision"]["commit"], sha);
    assert_eq!(r["revision"]["path"], name);
    assert!(!first_paragraph(&mut s, r["document_id"].as_str().unwrap()).is_empty());
}
