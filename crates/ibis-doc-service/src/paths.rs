// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The paths a tool may touch (design 5): under an allowed folder only, by the real path (links followed, `..` gone), with an extension of the
//! documents (or of what an export writes). Nothing is allowed by default.

use std::path::{Path, PathBuf};

use crate::error::ToolError;

/// The extensions of the documents that are opened and saved.
pub const DOCUMENT_EXTENSIONS: &[&str] = &["docx", "docm", "dotx", "xlsx", "xlsm", "pptx", "pptm"];

fn extension_of(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase()
}

/// `path` as a real path inside one of `allow` (`must_exist`: the file is there; else its folder is, and the file name is added to that folder's real path).
pub fn resolve(allow: &[PathBuf], path: &str, extensions: &[&str], must_exist: bool) -> Result<PathBuf, ToolError> {
    if allow.is_empty() {
        return Err(ToolError::new("not_allowed_path", "no folder is allowed: start the server with --allow-dir"));
    }
    if path.is_empty() || path.contains('\0') {
        return Err(ToolError::invalid("path is empty"));
    }
    let given = Path::new(path);
    let real = if must_exist || given.exists() {
        given.canonicalize().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { ToolError::new("file_not_found", path.to_string()) } else { ToolError::new("not_allowed_path", format!("{path}: {e}")) })?
    } else {
        let name = given.file_name().ok_or_else(|| ToolError::invalid("path has no file name"))?;
        let parent = given.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let parent = parent.canonicalize().map_err(|_| ToolError::new("not_allowed_path", format!("the folder of {path} does not exist")))?;
        parent.join(name)
    };
    if !allow.iter().any(|dir| real.starts_with(dir)) {
        return Err(ToolError::new("not_allowed_path", format!("{path} is not under an allowed folder")));
    }
    let extension = extension_of(&real);
    if !extensions.iter().any(|e| *e == extension) {
        return Err(ToolError::new("unsupported_format", format!("the extension .{extension} is not one of: {}", extensions.join(", "))));
    }
    Ok(real)
}

/// A path as a client is shown it. On Windows `canonicalize` gives the extended-length form (`\\?\C:\dir\file.docx`, a network share `\\?\UNC\host\share`),
/// which a client cannot use as it is: without that prefix (`C:\dir\file.docx`, `\\host\share`). What is not in that form is as it is.
pub fn shown(path: &Path) -> String {
    strip_extended_prefix(&path.to_string_lossy())
}

fn strip_extended_prefix(text: &str) -> String {
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        text.to_string()
    }
}

/// The folders the server was given, as real paths.
pub fn canonical_dirs(dirs: &[PathBuf]) -> Vec<PathBuf> {
    dirs.iter().filter_map(|d| d.canonicalize().ok()).collect()
}

/// The sum of the uncompressed sizes the zip's central directory says (what opening the file would expand to), or `None` for what is not a zip.
pub fn expanded_size(data: &[u8]) -> Option<u64> {
    let u16_at = |at: usize| data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as u64);
    let search_from = data.len().checked_sub(22)?;
    let end = (search_from.saturating_sub(65535)..=search_from).rev().find(|&i| data.get(i..i + 4) == Some(&[0x50, 0x4b, 0x05, 0x06][..]))?;
    let count = u16_at(end + 10)?;
    let mut at = u32_at(end + 16)? as usize;
    let mut total = 0u64;
    for _ in 0..count {
        if data.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02][..]) {
            return None;
        }
        total = total.saturating_add(u32_at(at + 24)?);
        at += 46 + u16_at(at + 28)? + u16_at(at + 30)? + u16_at(at + 32)?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::strip_extended_prefix as strip;

    #[test]
    fn the_extended_length_prefix_of_windows_is_not_shown() {
        assert_eq!(strip(r"\\?\C:\Users\me\a.docx"), r"C:\Users\me\a.docx");
        assert_eq!(strip(r"\\?\UNC\host\share\a.docx"), r"\\host\share\a.docx");
        assert_eq!(strip(r"C:\Users\me\a.docx"), r"C:\Users\me\a.docx");
        assert_eq!(strip("/Users/me/a.docx"), "/Users/me/a.docx");
    }
}
