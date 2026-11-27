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

/// Where a document is: its real path and the folder it is shown relative to.
pub struct Located {
    pub real: PathBuf,
    /// The allowed folder (or the `folder` argument) the path is under.
    pub base: PathBuf,
}

impl Located {
    /// The path from `base`, with `/` (what a result's `revision.path` says).
    pub fn relative(&self) -> String {
        self.real.strip_prefix(&self.base).map(|r| r.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")).unwrap_or_default()
    }
}

/// `path` with `.` and `..` taken out by the text alone.
fn normalized(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The real path of the part of `path` that exists, and the rest after it as it was written.
fn canonical_so_far(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut at = path.to_path_buf();
    loop {
        if let Ok(real) = at.canonicalize() {
            return rest.iter().rev().fold(real, |p, name: &std::ffi::OsString| p.join(name));
        }
        match (at.file_name().map(|n| n.to_os_string()), at.parent().map(Path::to_path_buf)) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                at = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// The path of a document a tool is asked for (design 159, 3.1): an absolute path, or one from `folder` (an allowed folder, or one under it; with none, the only allowed folder).
/// `must_exist` is false for a file that is only in a Git commit.
pub fn locate(allow: &[PathBuf], folder: Option<&str>, path: &str, extensions: &[&str], must_exist: bool) -> Result<Located, ToolError> {
    if allow.is_empty() {
        return Err(ToolError::new("not_allowed_path", "no folder is allowed: start the server with --allow-dir"));
    }
    if path.is_empty() || path.contains('\0') {
        return Err(ToolError::invalid("path is empty"));
    }
    let folder = match folder {
        None => None,
        Some(f) if f.is_empty() || f.contains('\0') => return Err(ToolError::invalid("folder is empty")),
        Some(f) => {
            let real = Path::new(f).canonicalize().map_err(|e| ToolError::new("not_allowed_path", format!("{f}: {e}")))?;
            if !allow.iter().any(|dir| real.starts_with(dir)) {
                return Err(ToolError::new("not_allowed_path", format!("{f} is not an allowed folder")));
            }
            Some(real)
        }
    };
    let given = Path::new(path);
    let joined = if given.is_absolute() {
        given.to_path_buf()
    } else {
        let base = match (&folder, allow) {
            (Some(f), _) => f.clone(),
            (None, [only]) => only.clone(),
            _ => return Err(ToolError::new("ambiguous_folder", "several folders are allowed: say which with folder, or give an absolute path")),
        };
        base.join(given)
    };
    let real = if must_exist {
        // Whether it is there is not said of a path outside the allowed folders.
        if !allow.iter().any(|dir| canonical_so_far(&normalized(&joined)).starts_with(dir)) {
            return Err(ToolError::new("not_allowed_path", format!("{path} is not under an allowed folder")));
        }
        resolve(allow, &joined.to_string_lossy(), extensions, true)?
    } else {
        let real = canonical_so_far(&normalized(&joined));
        if !allow.iter().any(|dir| real.starts_with(dir)) {
            return Err(ToolError::new("not_allowed_path", format!("{path} is not under an allowed folder")));
        }
        let extension = extension_of(&real);
        if !extensions.iter().any(|e| *e == extension) {
            return Err(ToolError::new("unsupported_format", format!("the extension .{extension} is not one of: {}", extensions.join(", "))));
        }
        real
    };
    if let Some(f) = &folder
        && !real.starts_with(f)
    {
        return Err(ToolError::new("not_allowed_path", format!("{path} is not under the folder {}", f.display())));
    }
    let base = folder.or_else(|| allow.iter().filter(|dir| real.starts_with(dir)).max_by_key(|dir| dir.components().count()).cloned()).unwrap_or_else(|| real.parent().map(Path::to_path_buf).unwrap_or_default());
    Ok(Located { real, base })
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
