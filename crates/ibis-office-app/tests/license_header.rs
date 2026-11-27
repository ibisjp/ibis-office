// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Every Rust source of the crates starts with the license lines
//! (AGPL-3.0-only, the copyright and the additional terms under section
//! 7(b), which publish/NOTICE states), so that the terms travel with the
//! source. A new file without them fails here.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() != "target" {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The copyright line: `// Copyright (c) ibis inc.`, or, for a file contributed
/// under the ibisOffice CLA, `// Copyright (c) ibis inc. and <name> (contributed
/// under the ibisOffice CLA, <URL of the GitHub issue or comment>)`. The name
/// stays; the URL must be in this repository's issues.
fn is_copyright_line(line: &str) -> bool {
    if line == "// Copyright (c) ibis inc." {
        return true;
    }
    let Some(rest) = line.strip_prefix("// Copyright (c) ibis inc. and ") else { return false };
    let Some(rest) = rest.strip_suffix(')') else { return false };
    let Some((name, url)) = rest.split_once(" (contributed under the ibisOffice CLA, ") else { return false };
    let name_ok = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_');
    let tail = url.strip_prefix("https://github.com/ibisjp/ibis-office/");
    name_ok && tail.is_some_and(|t| !t.is_empty() && !t.contains(' ') && !t.contains(')'))
}

#[test]
fn every_source_starts_with_the_license_lines() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    assert!(files.len() > 100, "the crates' sources were found: {}", files.len());
    let missing: Vec<_> = files
        .iter()
        .filter(|f| {
            let text = std::fs::read_to_string(f).unwrap_or_default();
            let mut lines = text.lines();
            !(lines.next() == Some("// SPDX-License-Identifier: AGPL-3.0-only")
                && lines.next().is_some_and(is_copyright_line)
                && lines.next() == Some("// Additional terms under AGPL-3.0 section 7(b): see NOTICE."))
        })
        .collect();
    assert!(missing.is_empty(), "without the license lines: {missing:?}");
}

#[test]
fn the_notice_asks_derivatives_for_powered_by_ibisoffice() {
    let notice = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../publish/NOTICE")).expect("publish/NOTICE");
    assert!(notice.contains("Powered by ibisOffice"));
    assert!(notice.contains("https://github.com/ibisjp/ibis-office"));
    assert!(notice.contains("7(b)"));
}
