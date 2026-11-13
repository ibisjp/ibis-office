// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Paths as people write them. On Windows `std::fs::canonicalize` returns the extended-length
//! form (`\\?\C:\Users\...`), which shows as it is on the start screen and which Windows
//! refuses once a path with `/` is added to it.

use std::path::{Path, PathBuf};

/// `path` without the extended-length prefix (`\\?\C:\...` becomes `C:\...`, `\\?\UNC\server\share`
/// becomes `\\server\share`); any other path as it is.
pub fn plain(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        // Only a drive path (`C:\...`) has a plain form of its own.
        Some(rest) if rest.len() >= 2 && rest.as_bytes()[0].is_ascii_alphabetic() && rest.as_bytes()[1] == b':' => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// `std::fs::canonicalize`, in the plain form.
pub fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(|p| plain(&p))
}

#[cfg(test)]
mod tests {
    use super::plain;
    use std::path::Path;

    #[test]
    fn the_extended_length_prefix_is_taken_off() {
        assert_eq!(plain(Path::new(r"\\?\C:\Users\a\b.docx")), Path::new(r"C:\Users\a\b.docx"));
        assert_eq!(plain(Path::new(r"\\?\UNC\server\share\b.docx")), Path::new(r"\\server\share\b.docx"));
        // Not a drive path: kept.
        assert_eq!(plain(Path::new(r"\\?\Volume{1}\b.docx")), Path::new(r"\\?\Volume{1}\b.docx"));
        assert_eq!(plain(Path::new("/home/a/b.docx")), Path::new("/home/a/b.docx"));
        assert_eq!(plain(Path::new(r"C:\Users\a")), Path::new(r"C:\Users\a"));
    }
}
