// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #159: the catalog, opening and reading the three kinds, saving, and what is not allowed (design 8).

use std::path::{Path, PathBuf};

use ibis_doc_service::{Access, Limits, Service, catalog};
use serde_json::{Value, json};

fn editing(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)
}

/// A folder of its own under `target/` with the sample files copied in.
fn sandbox(test: &str, files: &[&str]) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/doc-service-tests").join(test);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in files {
        std::fs::copy(editing(f), dir.join(f)).unwrap();
    }
    dir.canonicalize().unwrap()
}

fn service(dir: &Path) -> Service {
    Service::new(&[dir.to_path_buf()], false, Limits::default())
}

fn open(service: &mut Service, dir: &Path, name: &str) -> String {
    let r = service.call("app_open_document", &json!({ "path": dir.join(name).to_string_lossy() })).unwrap();
    r["document_id"].as_str().unwrap().to_string()
}

#[test]
fn mcp_catalog_names_follow_the_naming_rule() {
    // commandNamingRule A: {screen}_{verb}_{object}, snake_case, 64 characters or fewer; the verbs are the rule's.
    let verbs = ["get", "list", "set", "insert", "add", "delete", "clear", "move", "copy", "apply", "show", "hide", "open", "save", "export", "close", "find", "replace", "undo", "redo", "calculate", "merge", "unmerge", "split", "sort", "rename", "remove", "arrange", "align", "group", "ungroup"];
    // Beyond the rule's verbs: find, replace, undo, redo, calculate, merge, unmerge, sort, rename and remove are named so by Office itself (commandNamingRule 3: an operation outside the table keeps
    // the name Office gives it).
    let mut seen = std::collections::HashSet::new();
    for tool in catalog() {
        assert!(seen.insert(tool.name), "{} twice", tool.name);
        assert!(tool.name.len() <= 64 && tool.name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'), "{}", tool.name);
        let parts: Vec<&str> = tool.name.split('_').collect();
        assert!(["app", "docx", "xlsx", "pptx"].contains(&parts[0]) && parts.len() >= 3 && verbs.contains(&parts[1]), "{} is not {{screen}}_{{verb}}_{{object}}", tool.name);
        let schema = (tool.schema)();
        assert_eq!(schema["type"], "object", "{}", tool.name);
        assert!(!tool.description.is_empty());
    }
}

#[test]
fn mcp_reads_never_change_a_document() {
    let dir = sandbox("reads", &["autospace-letters.docx", "xlsx-sales.xlsx", "pptx-plan.pptx"]);
    let mut s = service(&dir);
    for name in ["autospace-letters.docx", "xlsx-sales.xlsx", "pptx-plan.pptx"] {
        let id = open(&mut s, &dir, name);
        let tools: Vec<_> = catalog().iter().filter(|t| t.access == Access::Read && t.name.split('_').next() == Some(&name[name.len() - 4..]) ).collect();
        assert!(!tools.is_empty(), "{name}");
        let info = s.call("app_get_document_info", &json!({ "document_id": id })).unwrap();
        assert_eq!(info["dirty"], false);
    }
}

#[test]
fn mcp_docx_xlsx_pptx_are_read() {
    let dir = sandbox("read_kinds", &["autospace-letters.docx", "xlsx-sales.xlsx", "pptx-plan.pptx"]);
    let mut s = service(&dir);
    let d = open(&mut s, &dir, "autospace-letters.docx");
    let text = s.call("docx_get_text", &json!({ "document_id": d, "format": "plain" })).unwrap();
    assert!(!text["text"].as_str().unwrap().is_empty());
    let list = s.call("docx_list_paragraphs", &json!({ "document_id": d, "limit": 2 })).unwrap();
    assert_eq!(list["paragraphs"].as_array().unwrap().len(), 2.min(list["total"].as_u64().unwrap() as usize));
    let first = s.call("docx_get_paragraph", &json!({ "document_id": d, "paragraph": 0 })).unwrap();
    assert_eq!(first["paragraph"], 0);
    assert_eq!(s.call("docx_get_paragraph", &json!({ "document_id": d, "paragraph": 100000 })).unwrap_err().code, "out_of_range");
    // The wrong kind of tool.
    assert_eq!(s.call("xlsx_list_sheets", &json!({ "document_id": d })).unwrap_err().code, "unsupported_operation");

    let x = open(&mut s, &dir, "xlsx-sales.xlsx");
    let sheets = s.call("xlsx_list_sheets", &json!({ "document_id": x })).unwrap();
    let name = sheets["sheets"][0]["name"].as_str().unwrap().to_string();
    let used = sheets["sheets"][0]["used_range"].as_str().unwrap().to_string();
    let cells = s.call("xlsx_get_cells", &json!({ "document_id": x, "sheet": name, "range": used, "kind": "text" })).unwrap();
    assert!(cells["rows"].as_array().unwrap().iter().flat_map(|r| r.as_array().unwrap()).any(|c| c.as_str().is_some_and(|t| !t.is_empty())));
    assert_eq!(s.call("xlsx_get_cells", &json!({ "document_id": x, "sheet": "no such sheet", "range": "A1" })).unwrap_err().code, "no_such_sheet");
    assert_eq!(s.call("xlsx_get_cells", &json!({ "document_id": x, "sheet": name, "range": "A0" })).unwrap_err().code, "invalid_argument");

    let p = open(&mut s, &dir, "pptx-plan.pptx");
    let slides = s.call("pptx_list_slides", &json!({ "document_id": p })).unwrap();
    assert!(slides["slides"].as_array().unwrap().len() > 1);
    let slide = s.call("pptx_get_slide", &json!({ "document_id": p, "slide": 1 })).unwrap();
    assert!(slide["shapes"].as_array().is_some());
    assert_eq!(s.call("pptx_get_slide", &json!({ "document_id": p, "slide": 0 })).unwrap_err().code, "out_of_range");
}

#[test]
fn mcp_saving_keeps_the_file_unless_overwrite_is_asked() {
    let dir = sandbox("saving", &["autospace-letters.docx"]);
    let mut s = service(&dir);
    let id = open(&mut s, &dir, "autospace-letters.docx");
    let original = std::fs::read(dir.join("autospace-letters.docx")).unwrap();
    // A new name is the default: saved, and the file read back is the original byte for byte (nothing was changed).
    let target = dir.join("copy.docx");
    let r = s.call("app_save_document", &json!({ "document_id": id, "path": target.to_string_lossy() })).unwrap();
    assert_eq!(r["saved"], true);
    assert_eq!(std::fs::read(&target).unwrap(), original, "an unchanged document is saved as it was");
    // Over an existing file: refused without overwrite, allowed with it.
    let e = s.call("app_save_document", &json!({ "document_id": id, "path": target.to_string_lossy() })).unwrap_err();
    assert_eq!(e.code, "not_allowed_path");
    assert_eq!(s.call("app_save_document", &json!({ "document_id": id, "path": target.to_string_lossy(), "overwrite": true })).unwrap()["saved"], true);
    // The file changed on disk after it was opened: not replaced unless forced.
    let source = dir.join("autospace-letters.docx");
    std::fs::write(&source, b"changed by another program").unwrap();
    let e = s.call("app_save_document", &json!({ "document_id": id, "path": source.to_string_lossy(), "overwrite": true })).unwrap_err();
    assert_eq!(e.code, "file_changed_on_disk");
    assert_eq!(std::fs::read(&source).unwrap(), b"changed by another program");
    assert_eq!(s.call("app_save_document", &json!({ "document_id": id, "path": source.to_string_lossy(), "overwrite": "force" })).unwrap()["saved"], true);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    // No temporary file is left.
    assert!(std::fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains("ibis-saving")));
}

#[test]
fn mcp_paths_are_allowed_by_folder_extension_and_size() {
    let dir = sandbox("paths", &["autospace-letters.docx"]);
    let outside = sandbox("paths_outside", &["xlsx-sales.xlsx"]);
    // Nothing allowed by default.
    let mut none = Service::new(&[], false, Limits::default());
    assert_eq!(none.call("app_open_document", &json!({ "path": dir.join("autospace-letters.docx").to_string_lossy() })).unwrap_err().code, "not_allowed_path");
    let mut s = service(&dir);
    // Another folder, and `..` out of the allowed one, and a link out of it.
    assert_eq!(s.call("app_open_document", &json!({ "path": outside.join("xlsx-sales.xlsx").to_string_lossy() })).unwrap_err().code, "not_allowed_path");
    let escaping = dir.join("..").join("paths_outside").join("xlsx-sales.xlsx");
    assert_eq!(s.call("app_open_document", &json!({ "path": escaping.to_string_lossy() })).unwrap_err().code, "not_allowed_path");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outside.join("xlsx-sales.xlsx"), dir.join("link.xlsx")).unwrap();
        assert_eq!(s.call("app_open_document", &json!({ "path": dir.join("link.xlsx").to_string_lossy() })).unwrap_err().code, "not_allowed_path");
    }
    // Not a document extension; missing.
    std::fs::write(dir.join("a.txt"), "x").unwrap();
    assert_eq!(s.call("app_open_document", &json!({ "path": dir.join("a.txt").to_string_lossy() })).unwrap_err().code, "unsupported_format");
    assert_eq!(s.call("app_open_document", &json!({ "path": dir.join("none.docx").to_string_lossy() })).unwrap_err().code, "file_not_found");
    // A file larger than the limit.
    let mut small = Service::new(&[dir.clone()], false, Limits { max_file_size: 1000, ..Limits::default() });
    assert_eq!(small.call("app_open_document", &json!({ "path": dir.join("autospace-letters.docx").to_string_lossy() })).unwrap_err().code, "file_too_large");
    // A documents limit.
    let mut one = Service::new(&[dir.clone()], false, Limits { max_documents: 1, ..Limits::default() });
    open(&mut one, &dir, "autospace-letters.docx");
    assert_eq!(one.call("app_open_document", &json!({ "path": dir.join("autospace-letters.docx").to_string_lossy() })).unwrap_err().code, "out_of_range");
}

#[test]
fn mcp_read_only_has_no_tool_that_changes_anything() {
    let dir = sandbox("read_only", &["autospace-letters.docx"]);
    let mut s = Service::new(&[dir.clone()], true, Limits::default());
    assert!(s.tools().iter().all(|t| t.access == Access::Read));
    let id = open(&mut s, &dir, "autospace-letters.docx");
    assert_eq!(s.call("app_undo_document", &json!({ "document_id": id })).unwrap_err().code, "read_only");
    assert_eq!(s.call("app_save_document", &json!({ "document_id": id, "path": dir.join("x.docx").to_string_lossy() })).unwrap_err().code, "read_only");
    // Close and the unknown handle.
    assert_eq!(s.call("app_close_document", &json!({ "document_id": id })).unwrap()["closed"], true);
    assert_eq!(s.call("app_get_document_info", &json!({ "document_id": id })).unwrap_err().code, "no_such_document");
    let _: Value = s.call("app_get_version", &json!({})).unwrap();
}

#[cfg(unix)]
#[test]
fn mcp_export_runs_the_program_and_puts_the_files_where_asked() {
    use std::os::unix::fs::PermissionsExt;
    let dir = sandbox("export", &["autospace-letters.docx"]);
    // A stand-in for `ibisOffice --export`: it writes `<name>.pdf`, or `<name>-01.png` and `-02.png`, into --out-dir, as the real one does.
    let program = dir.join("fake-ibisoffice.sh");
    std::fs::write(
        &program,
        "#!/bin/sh\nformat=$2; out=$4; shift 4\nfor last; do :; done\nname=$(basename \"$last\"); name=${name%.*}\nif [ \"$format\" = pdf ]; then printf PDF > \"$out/$name.pdf\"; else printf P1 > \"$out/$name-01.png\"; printf P2 > \"$out/$name-02.png\"; fi\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut s = service(&dir);
    let id = open(&mut s, &dir, "autospace-letters.docx");
    // No program, no export.
    assert_eq!(s.call("app_export_document", &json!({ "document_id": id, "format": "pdf", "path": dir.join("a.pdf").to_string_lossy() })).unwrap_err().code, "unsupported_operation");
    s.set_exporter(Some(program));
    let r = s.call("app_export_document", &json!({ "document_id": id, "format": "pdf", "path": dir.join("a.pdf").to_string_lossy() })).unwrap();
    assert_eq!(r["exported"], true);
    assert_eq!(std::fs::read(dir.join("a.pdf")).unwrap(), b"PDF");
    // A file that is there is not replaced without overwrite.
    assert_eq!(s.call("app_export_document", &json!({ "document_id": id, "format": "pdf", "path": dir.join("a.pdf").to_string_lossy() })).unwrap_err().code, "not_allowed_path");
    s.call("app_export_document", &json!({ "document_id": id, "format": "pdf", "path": dir.join("a.pdf").to_string_lossy(), "overwrite": true })).unwrap();
    // Pictures: one a page.
    let r = s.call("app_export_document", &json!({ "document_id": id, "format": "png", "path": dir.join("page.png").to_string_lossy(), "range": "1-2", "width": 800 })).unwrap();
    assert_eq!(r["files"].as_array().unwrap().len(), 2);
    assert_eq!(std::fs::read(dir.join("page-02.png")).unwrap(), b"P2");
    // Wrong arguments; a place outside the allowed folders; a wrong extension.
    assert_eq!(s.call("app_export_document", &json!({ "document_id": id, "format": "pdf", "path": dir.join("b.pdf").to_string_lossy(), "range": "1;rm" })).unwrap_err().code, "invalid_argument");
    // The paper (#225): a name or <width>x<height>mm, nothing else is passed to the program.
    for paper in ["a4;rm", "--x", "", "x210mm", "210x297"] {
        assert_eq!(s.call("app_export_document", &json!({ "document_id": id, "format": "pdf", "path": dir.join("b.pdf").to_string_lossy(), "paper": paper })).unwrap_err().code, "invalid_argument", "{paper}");
    }
    // A paper that can be used is for a presentation only (a document's page is its own page setup's); the presentation case is tests/export_paper.rs.
    for paper in ["A4", "letter", "210x297mm"] {
        assert_eq!(s.call("app_export_document", &json!({ "document_id": id, "format": "pdf", "path": dir.join("a.pdf").to_string_lossy(), "overwrite": true, "paper": paper, "fit_paper": false })).unwrap_err().code, "unsupported_operation", "{paper}");
    }
    assert_eq!(s.call("app_export_document", &json!({ "document_id": id, "format": "pdf", "path": "/tmp/x.pdf" })).unwrap_err().code, "not_allowed_path");
    assert_eq!(s.call("app_export_document", &json!({ "document_id": id, "format": "pdf", "path": dir.join("b.png").to_string_lossy() })).unwrap_err().code, "unsupported_format");
    // No work folder is left.
    assert!(std::fs::read_dir(std::env::temp_dir()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(&format!("ibisoffice-export-{}", &id[1..]))));
}
