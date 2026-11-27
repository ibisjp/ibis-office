// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `app_*` tools: documents of any kind (design 6.1).

use std::io::Write;

use sha2::{Digest, Sha256};

use ibis_doc_model::Document;
use ibis_formats::Kind;
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, boolean, document_id, object, string};
use crate::error::ToolError;
use std::path::PathBuf;

use crate::paths::{self, DOCUMENT_EXTENSIONS};
use crate::service::{Args, Fingerprint, Open, RevisionInfo, Service, hex};

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "app_get_version", description: "The version of ibisOffice this server runs.", access: Access::Read, returns_content: false, schema: &|| object(&[], &[]), run: get_version },
        Tool { name: "app_list_documents", description: "The documents that are open.", access: Access::Read, returns_content: false, schema: &|| object(&[], &[]), run: list_documents },
        Tool {
            name: "app_open_document",
            description: "Open a docx, xlsx or pptx file (under an allowed folder) and get its document_id. Nothing is run and nothing outside the file is fetched.",
            access: Access::Read,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        ("path", string("The file's path: absolute, or from `folder` (with only one folder allowed, from that). Not needed with revision.kind open_document.")),
                        ("folder", string("An allowed folder the path is from (needed for a relative path when several folders are allowed).")),
                        (
                            "revision",
                            json!({
                                "type": "object",
                                "description": "Which version to read (default: the file as it is on disk now). The result's `revision` says which version was read.",
                                "properties": {
                                    "kind": { "enum": ["working_tree", "git", "open_document"], "description": "working_tree: the file on disk now. git: the file in a commit (read only: it cannot be saved over its own path). open_document: a copy of a document this server has open, as it is now with its unsaved changes." },
                                    "commit": string("With git: the commit's SHA, 4 to 64 hexadecimal digits (not a branch or tag name). The result gives the full SHA."),
                                    "document_id": string("With open_document: the document to copy."),
                                },
                                "required": ["kind"],
                                "additionalProperties": false,
                            }),
                        ),
                        ("password", string("The password of an encrypted file.")),
                    ],
                    &[],
                )
            },
            run: open_document,
        },
        Tool { name: "app_get_document_info", description: "The kind, size, state (unsaved changes, signature, macros) and undo state of an open document.", access: Access::Read, returns_content: false, schema: &|| object(&[document_id()], &["document_id"]), run: get_document_info },
        Tool {
            name: "app_save_document",
            description: "Save an open document to `path`. A new name is the default; writing over an existing file needs overwrite: true (\"force\" also when the file changed since it was opened).",
            access: Access::File,
            returns_content: false,
            schema: &|| {
                object(
                    &[document_id(), ("path", string("Where to save.")), ("overwrite", json!({ "description": "true to replace an existing file; \"force\" to replace even one changed on disk since it was opened.", "enum": [true, false, "force"] }))],
                    &["document_id", "path"],
                )
            },
            run: save_document,
        },
        Tool {
            name: "app_close_document",
            description: "Close an open document. A document with unsaved changes is closed only with discard: true.",
            access: Access::Read,
            returns_content: false,
            schema: &|| object(&[document_id(), ("discard", boolean("Drop the unsaved changes."))], &["document_id"]),
            run: close_document,
        },
        Tool {
            name: "app_export_document",
            description: "Write an open document (as it is now, saved or not) as a PDF, or as PNG pictures (one a page: path name-01.png, name-02.png, ...). Needs the ibisOffice program next to this server.",
            access: Access::File,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("format", json!({ "enum": ["pdf", "png"] })),
                        ("path", string("Where to write (.pdf, or .png whose name gets -01, -02, ...).")),
                        ("range", string("The pages or slides, from 1: 1,3-5.")),
                        ("width", crate::catalog::integer("PNG width in pixels (16 to 8192).")),
                        ("overwrite", boolean("Replace files that are there.")),
                        ("paper", string("The paper the pages are printed on, as `--print-paper`: a3, a4, a5, b4, b5, letter, legal, tabloid or <width>x<height>mm (a presentation's pages and handouts are put on it as PowerPoint prints). Default: the pages' own size.")),
                        ("fit_paper", boolean("With paper: scale the pages to fit it (default true); false puts them at 100 %.")),
                    ],
                    &["document_id", "format", "path"],
                )
            },
            run: export_document,
        },
        Tool { name: "app_undo_document", description: "Undo the last edits (one tool call is one step).", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), ("count", crate::catalog::integer("How many steps (default 1)."))], &["document_id"]), run: undo_document },
        Tool { name: "app_redo_document", description: "Redo what was undone.", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), ("count", crate::catalog::integer("How many steps (default 1)."))], &["document_id"]), run: redo_document },
    ]
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Document => "docx",
        Kind::Workbook => "xlsx",
        Kind::Presentation => "pptx",
        Kind::Pdf => "pdf",
    }
}

fn get_version(service: &mut Service, _: &Value) -> Result<Value, ToolError> {
    Ok(json!({ "name": "ibisOffice", "version": service.version }))
}

fn describe(id: &str, open: &Open) -> Value {
    let presentation = open.document.presentation();
    let mut info = json!({
        "document_id": id,
        "kind": kind_name(open.kind),
        "path": paths::shown(&open.path),
        "dirty": open.document.is_dirty(),
        "signed": open.signed,
        "has_macros": open.has_macros,
        "can_undo": open.document.can_undo(),
        "can_redo": open.document.can_redo(),
        "revision": open.revision_json(),
    });
    match open.kind {
        Kind::Document => {
            if let Some(flow) = presentation.flow.as_ref() {
                info["paragraphs"] = json!(flow.body.iter().filter(|b| matches!(b, ibis_doc_model::flow::Block::Paragraph(_))).count());
                info["tables"] = json!(flow.body.iter().filter(|b| matches!(b, ibis_doc_model::flow::Block::Table(_))).count());
                info["properties"] = crate::tools_xlsx_read::properties_json(&flow.properties);
                info["tracking_changes"] = json!(flow.track_changes);
            }
        }
        Kind::Workbook => {
            info["sheets"] = json!(presentation.slides.len());
            if let Some(w) = presentation.workbook.as_ref() {
                info["calculation"] = json!({ "mode": match w.calc_mode { ibis_doc_model::sheet::CalcMode::Manual => "manual", ibis_doc_model::sheet::CalcMode::AutoNoTable => "automatic_except_tables", _ => "automatic" }, "iterative": w.iterate, "max_iterations": w.max_iterations, "max_change": w.max_change.0 });
                info["properties"] = crate::tools_xlsx_read::properties_json(&w.properties);
            }
        }
        _ => {
            info["slides"] = json!(presentation.slides.len());
            info["slide_size"] = json!({ "width": presentation.slide_size.width as f64 / 12700.0, "height": presentation.slide_size.height as f64 / 12700.0 });
            info["properties"] = crate::tools_xlsx_read::properties_json(&presentation.properties);
            let m = &presentation.header_footer;
            info["header_footer"] = json!({ "date": m.date, "slide_number": m.number, "footer": m.footer, "hide_on_title": !m.on_title_slides });
        }
    }
    info
}

fn list_documents(service: &mut Service, _: &Value) -> Result<Value, ToolError> {
    let documents: Vec<Value> = service.documents.iter().map(|(id, open)| describe(id, open)).collect();
    Ok(json!({ "documents": documents }))
}

/// Which version a call asks for.
enum Version {
    WorkingTree,
    Git(String),
    OpenDocument(String),
}

fn parse_version(args: &Value) -> Result<Version, ToolError> {
    let revision = match args.get("revision") {
        None | Some(Value::Null) => return Ok(Version::WorkingTree),
        Some(r) if r.is_object() => r,
        Some(_) => return Err(ToolError::invalid("revision is an object: { \"kind\": \"working_tree\" | \"git\" | \"open_document\", ... }")),
    };
    if let Some(map) = revision.as_object()
        && let Some(key) = map.keys().find(|k| !["kind", "commit", "document_id"].contains(&k.as_str()))
    {
        return Err(ToolError::invalid(format!("revision.{key} is not an argument")));
    }
    match revision.str_arg("kind").map_err(|_| ToolError::invalid("revision.kind is working_tree, git or open_document"))? {
        "working_tree" => Ok(Version::WorkingTree),
        "git" => Ok(Version::Git(revision.opt_str("commit")?.ok_or_else(|| ToolError::invalid("revision.commit is required with git"))?.to_string())),
        "open_document" => Ok(Version::OpenDocument(revision.opt_str("document_id")?.ok_or_else(|| ToolError::invalid("revision.document_id is required with open_document"))?.to_string())),
        other => Err(ToolError::invalid(format!("revision.kind {other:?} is not working_tree, git or open_document"))),
    }
}

fn open_document(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let version = parse_version(args)?;
    if let Version::OpenDocument(from) = &version {
        // A copy of a document this server has open, as it is now (its unsaved changes too).
        if service.documents.len() >= service.limits.max_documents {
            return Err(ToolError::new("out_of_range", format!("{} documents are open already; close one first", service.documents.len())));
        }
        let Some(source) = service.documents.get(from) else {
            return Err(ToolError::new("unsupported_revision", format!("{from} is not a document this server has open (a document open in the ibisOffice window is another program's: it cannot be read from here yet)")));
        };
        let mut document = Document::new(source.document.presentation().clone());
        document.set_file(Some(paths::shown(&source.path)));
        if source.document.is_dirty() {
            document.mark_dirty();
        }
        let open = Open {
            kind: source.kind,
            document,
            source: source.source.clone(),
            path: source.path.clone(),
            fingerprint: source.fingerprint.clone(),
            signed: source.signed,
            has_macros: source.has_macros,
            revision: RevisionInfo { kind: "open_document", commit: source.revision.commit.clone(), path: source.revision.path.clone() },
            states: vec![source.state().to_string()],
            redo_states: Vec::new(),
            pinned: source.pinned,
        };
        let id = service.new_document_id();
        let info = describe(&id, &open);
        service.documents.insert(id, open);
        return Ok(info);
    }
    let git = matches!(version, Version::Git(_));
    let located = paths::locate(&service.allow_dirs, args.opt_str("folder")?, args.str_arg("path")?, DOCUMENT_EXTENSIONS, !git)?;
    let path = located.real.clone();
    if service.documents.len() >= service.limits.max_documents {
        return Err(ToolError::new("out_of_range", format!("{} documents are open already; close one first", service.documents.len())));
    }
    let (data, commit) = if let Version::Git(commit) = &version {
        let file = crate::git_read::read_commit(&service.allow_dirs, &path, commit, service.limits.max_file_size)?;
        (file.data, Some(file.commit))
    } else {
        let size = std::fs::metadata(&path).map_err(|e| ToolError::new("file_not_found", e.to_string()))?.len();
        if size > service.limits.max_file_size {
            return Err(ToolError::new("file_too_large", format!("{size} bytes; the limit is {} (--max-file-size)", service.limits.max_file_size)));
        }
        (std::fs::read(&path).map_err(|e| ToolError::new("file_not_found", e.to_string()))?, None)
    };
    let revision = RevisionInfo { kind: if git { "git" } else { "working_tree" }, commit, path: Some(located.relative()) };
    open_data(service, data, path, revision, git)
}

/// The bytes of a file read as the version `revision` says, opened as a new document.
fn open_data(service: &mut Service, data: Vec<u8>, path: PathBuf, revision: RevisionInfo, pinned: bool) -> Result<Value, ToolError> {
    if let Some(expanded) = paths::expanded_size(&data)
        && expanded > (data.len() as u64).saturating_mul(service.limits.max_expansion).max(1 << 20)
    {
        return Err(ToolError::new("file_too_large", format!("the file would expand to {expanded} bytes (more than {} times its size)", service.limits.max_expansion)));
    }
    let fingerprint = Fingerprint::of_file(&path, &data);
    let has_macros = Kind::has_macros(path.extension().and_then(|e| e.to_str()).unwrap_or(""));
    let opened = ibis_odf::open(data).map_err(|e| {
        let message = e.to_string();
        if message.contains("パスワード") || message.contains("暗号") || message.to_ascii_lowercase().contains("password") {
            ToolError::new("needs_password", "the file is encrypted")
        } else {
            ToolError::new("unsupported_format", message)
        }
    })?;
    let kind = Kind::of(&opened.presentation);
    if kind == Kind::Pdf {
        return Err(ToolError::new("unsupported_format", "a PDF is not an editable document"));
    }
    let signed = opened.source.has_signature();
    let mut document = Document::new(opened.presentation);
    document.set_file(Some(paths::shown(&path)));
    let id = service.new_document_id();
    let states = vec![Open::first_state(&fingerprint)];
    let open = Open { kind, document, source: opened.source, path, fingerprint, signed, has_macros, revision, states, redo_states: Vec::new(), pinned };
    let info = describe(&id, &open);
    service.documents.insert(id, open);
    Ok(info)
}

fn get_document_info(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    Ok(describe(id, open))
}

fn close_document(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let discard = args.opt_bool("discard")?.unwrap_or(false);
    let open = service.open_document(id)?;
    if open.document.is_dirty() && !discard {
        return Err(ToolError::new("discard_required", "the document has unsaved changes: save it, or close it with discard: true"));
    }
    service.documents.remove(id);
    Ok(json!({ "closed": true }))
}

fn steps(args: &Value) -> Result<usize, ToolError> {
    Ok(args.opt_usize("count")?.unwrap_or(1).clamp(1, 1000))
}

fn undo_document(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let count = steps(args)?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let done = (0..count).take_while(|_| open.document.undo()).count();
    Ok(json!({ "undone": done, "dirty": open.document.is_dirty(), "can_undo": open.document.can_undo(), "can_redo": open.document.can_redo() }))
}

fn redo_document(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let count = steps(args)?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let done = (0..count).take_while(|_| open.document.redo()).count();
    Ok(json!({ "redone": done, "dirty": open.document.is_dirty(), "can_undo": open.document.can_undo(), "can_redo": open.document.can_redo() }))
}

fn save_document(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let overwrite = match args.get("overwrite") {
        None | Some(Value::Null) | Some(Value::Bool(false)) => "no",
        Some(Value::Bool(true)) => "yes",
        Some(Value::String(s)) if s == "force" => "force",
        Some(_) => return Err(ToolError::invalid("overwrite is true, false or \"force\"")),
    };
    let target = paths::resolve(&service.allow_dirs, args.str_arg("path")?, DOCUMENT_EXTENSIONS, false)?;
    let open = service.open_document(id)?;
    let same_file = target == open.path;
    if same_file && open.pinned {
        return Err(ToolError::new("read_only", "this is a version read from a Git commit: it is not written over its own path (save it under another name)"));
    }
    if target.exists() {
        // The file on disk must be the one the caller read (design 159, 3.3); `overwrite: "force"` does not skip it.
        if let Some(expected) = args.opt_str("expected_revision")? {
            let on_disk = std::fs::read(&target).map(|d| hex(&Sha256::digest(&d))).unwrap_or_default();
            if !on_disk.eq_ignore_ascii_case(expected) {
                return Err(ToolError::new("revision_mismatch", format!("the file on disk has file_sha256 {on_disk} but expected_revision is {expected}: read it again")));
            }
        }
        if overwrite == "no" {
            return Err(ToolError::new("not_allowed_path", format!("{} exists: pass overwrite: true to replace it", target.display())));
        }
        if same_file && overwrite != "force" && !open.fingerprint.still(&target) {
            return Err(ToolError::new("file_changed_on_disk", "the file changed on disk after it was opened: pass overwrite: \"force\" to replace it anyway"));
        }
    }
    let bytes = open.source.save(open.document.presentation(), ibis_formats::SaveOptions::default()).map_err(|e| ToolError::new("internal", e.to_string()))?;
    write_replacing(&target, &bytes)?;
    open.document.mark_clean();
    if same_file {
        open.fingerprint = Fingerprint::of_file(&target, &bytes);
    }
    let mut warnings = Vec::new();
    if open.signed {
        warnings.push(json!({ "code": "signatures_removed", "message": "the file was digitally signed; a saved edit removes the signature" }));
    }
    Ok(json!({ "saved": true, "path": paths::shown(&target), "bytes": bytes.len(), "warnings": warnings }))
}

/// The file written whole next to `target` and put in its place by a rename: a failure leaves the old file as it was.
fn write_replacing(target: &std::path::Path, bytes: &[u8]) -> Result<(), ToolError> {
    let name = target.file_name().and_then(|n| n.to_str()).unwrap_or("document");
    let temp = target.with_file_name(format!(".{name}.ibis-saving"));
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, target)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        ToolError::new("internal", format!("{}: {e}", target.display()))
    })
}

/// A paper name `--print-paper` takes (`<width>x<height>mm` is 10 to 5000 mm each, as the program takes it): letters, digits, `x` and `.` only, so nothing else is passed on.
fn valid_paper(text: &str) -> bool {
    matches!(text, "a3" | "a4" | "a5" | "b4" | "b5" | "letter" | "legal" | "tabloid")
        || text.strip_suffix("mm").and_then(|n| n.split_once('x')).is_some_and(|(w, h)| [w, h].iter().all(|v| !v.is_empty() && v.len() <= 7 && v.bytes().all(|b| b.is_ascii_digit() || b == b'.') && v.parse::<f32>().is_ok_and(|n| (10.0..=5000.0).contains(&n))))
}

fn export_document(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use std::process::{Command, Stdio};

    let id = args.str_arg("document_id")?;
    let format = args.str_arg("format")?;
    if !matches!(format, "pdf" | "png") {
        return Err(ToolError::invalid("format is pdf or png"));
    }
    let target = paths::resolve(&service.allow_dirs, args.str_arg("path")?, &[format], false)?;
    let overwrite = args.opt_bool("overwrite")?.unwrap_or(false);
    let range = args.opt_str("range")?.map(str::to_string);
    if range.as_deref().is_some_and(|r| r.is_empty() || !r.bytes().all(|b| b.is_ascii_digit() || b == b',' || b == b'-')) {
        return Err(ToolError::invalid("range is pages from 1, like 1,3-5"));
    }
    let width = args.opt_usize("width")?;
    if width.is_some_and(|w| !(16..=8192).contains(&w)) {
        return Err(ToolError::invalid("width is 16 to 8192 pixels"));
    }
    let paper = args.opt_str("paper")?.map(|p| p.trim().to_ascii_lowercase());
    if paper.as_deref().is_some_and(|p| !valid_paper(p)) {
        return Err(ToolError::invalid("paper is a3, a4, a5, b4, b5, letter, legal, tabloid or <width>x<height>mm"));
    }
    let fit_paper_given = args.opt_bool("fit_paper")?;
    let fit_paper = fit_paper_given.unwrap_or(true);
    if paper.is_some() && format == "png" {
        return Err(ToolError::invalid("paper is for pdf: a picture has no paper"));
    }
    if fit_paper_given.is_some() && paper.is_none() {
        return Err(ToolError::invalid("fit_paper needs paper"));
    }
    let exporter = service.exporter.clone().ok_or_else(|| ToolError::new("unsupported_operation", "this server has no ibisOffice program to export with"))?;
    let stem = target.file_stem().and_then(|s| s.to_str()).unwrap_or("document").to_string();
    let open = service.open_document(id)?;
    let bytes = open.source.save(open.document.presentation(), ibis_formats::SaveOptions::default()).map_err(|e| ToolError::new("internal", e.to_string()))?;
    let extension = open.path.extension().and_then(|e| e.to_str()).unwrap_or("docx").to_string();
    if paper.is_some() && !matches!(extension.as_str(), "pptx" | "ppt" | "odp") {
        return Err(ToolError::new("unsupported_operation", "paper is for a presentation: the page of a document or a sheet is its own page setup's"));
    }
    // The document as it is now is written to a folder of its own, drawn there, and what is drawn is moved to where it was asked.
    let work = std::env::temp_dir().join(format!("ibisoffice-export-{}", id.trim_start_matches('d')));
    let _ = std::fs::remove_dir_all(&work);
    let (input_dir, out_dir) = (work.join("in"), work.join("out"));
    let make = || -> std::io::Result<()> {
        std::fs::create_dir_all(&input_dir)?;
        std::fs::create_dir_all(&out_dir)
    };
    make().map_err(|e| ToolError::new("internal", e.to_string()))?;
    let result = (|| -> Result<Vec<PathBuf>, ToolError> {
        let input = input_dir.join(format!("export.{extension}"));
        std::fs::write(&input, &bytes).map_err(|e| ToolError::new("internal", e.to_string()))?;
        let mut command = Command::new(&exporter);
        command.arg("--export").arg(format).arg("--out-dir").arg(&out_dir);
        if let Some(range) = &range {
            command.arg("--range").arg(range);
        }
        if let Some(paper) = &paper {
            command.arg("--print-paper").arg(paper);
            if !fit_paper {
                command.arg("--no-print-fit-paper");
            }
        }
        if let (Some(width), "png") = (width, format) {
            command.arg("--width").arg(width.to_string());
        }
        let mut child = command.arg(&input).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| ToolError::new("internal", format!("{}: {e}", exporter.display())))?;
        // Not more than 5 minutes.
        let started = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() > std::time::Duration::from_secs(300) => {
                    let _ = child.kill();
                    return Err(ToolError::new("internal", "the export took more than 5 minutes"));
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                Err(e) => return Err(ToolError::new("internal", e.to_string())),
            }
        };
        if !status.success() {
            let mut message = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut stderr, &mut message);
            }
            return Err(ToolError::new("internal", format!("the export failed: {}", message.trim())));
        }
        let mut made: Vec<PathBuf> = std::fs::read_dir(&out_dir).map_err(|e| ToolError::new("internal", e.to_string()))?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().and_then(|e| e.to_str()) == Some(format)).collect();
        made.sort();
        if made.is_empty() {
            return Err(ToolError::new("internal", "the export wrote nothing"));
        }
        // Where each goes: a PDF is the path; a picture is the path's name with -01, -02 (the number the export gave it).
        let dir = target.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let places: Vec<PathBuf> = made
            .iter()
            .map(|file| {
                if format == "pdf" {
                    target.clone()
                } else {
                    let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    let number = name.rsplit('-').next().unwrap_or("01");
                    dir.join(format!("{stem}-{number}.png"))
                }
            })
            .collect();
        if !overwrite && let Some(there) = places.iter().find(|p| p.exists()) {
            return Err(ToolError::new("not_allowed_path", format!("{} exists: pass overwrite: true to replace it", there.display())));
        }
        for (file, place) in made.iter().zip(&places) {
            let data = std::fs::read(file).map_err(|e| ToolError::new("internal", e.to_string()))?;
            write_replacing(place, &data)?;
        }
        Ok(places)
    })();
    let _ = std::fs::remove_dir_all(&work);
    let places = result?;
    Ok(json!({ "exported": true, "files": places.iter().map(|p| paths::shown(p)).collect::<Vec<_>>() }))
}
