// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Recently opened files, most recent first, for ファイル > 履歴.
//!
//! Kept in a text file with one path per line, under the user's application
//! data directory (`~/Library/Application Support/ibisOffice/recent-files.txt`
//! on macOS). A missing or unreadable file is an empty list; a failed write
//! is logged and otherwise ignored.

use std::path::{Path, PathBuf};

/// How many files the list keeps.
pub(crate) const MAX_RECENT: usize = 20;

/// Where the list is stored, if the user's data directory is known.
fn store_path() -> Option<PathBuf> {
    let base = crate::shell::app_data::base();
    Some(base?.join("ibisOffice").join("recent-files.txt"))
}

/// The stored list; empty if there is none or it cannot be read.
pub(crate) fn load() -> Vec<PathBuf> {
    let Some(store) = store_path() else { return Vec::new() };
    // (A list stored with the extended-length form of Windows paths is read in the plain one.)
    std::fs::read_to_string(&store).map(|text| parse(&text)).unwrap_or_default().iter().map(|p| crate::shell::plain_path::plain(p)).collect()
}

/// Put `path` first in the stored list and return the new list.
pub(crate) fn add(path: &Path) -> Vec<PathBuf> {
    let mut list = load();
    // The absolute path, so the same file opened from different working
    // directories is one entry.
    let path = crate::shell::plain_path::canonical(path).unwrap_or_else(|_| path.to_path_buf());
    push(&mut list, path);
    save(&list);
    list
}

/// Remove only the history entry; never delete the user's file.
pub(crate) fn remove(path: &Path) -> Vec<PathBuf> {
    let mut list=load();
    list.retain(|entry|entry!=path);
    save(&list);
    list
}

fn save(list: &[PathBuf]) {
    let Some(store) = store_path() else { return };
    let mut text = String::new();
    for path in list {
        text.push_str(&path.to_string_lossy());
        text.push('\n');
    }
    let result = match store.parent() {
        Some(dir) => std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&store, text)),
        None => std::fs::write(&store, text),
    };
    if let Err(err) = result {
        log::warn!("{}: {err}", store.display());
    }
}

fn parse(text: &str) -> Vec<PathBuf> {
    let mut list = Vec::new();
    for line in text.lines().rev() {
        if !line.trim().is_empty() {
            push(&mut list, PathBuf::from(line));
        }
    }
    list
}

/// Move or insert `path` at the front, keeping at most [`MAX_RECENT`] entries.
fn push(list: &mut Vec<PathBuf>, path: PathBuf) {
    // A path with a line break cannot be stored one per line.
    if path.to_string_lossy().contains(['\n', '\r']) {
        return;
    }
    list.retain(|p| *p != path);
    list.insert(0, path);
    list.truncate(MAX_RECENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_moves_to_front_without_duplicates() {
        let mut list = Vec::new();
        push(&mut list, "/a.pptx".into());
        push(&mut list, "/b.pptx".into());
        push(&mut list, "/a.pptx".into());
        assert_eq!(list, [PathBuf::from("/a.pptx"), PathBuf::from("/b.pptx")]);
    }

    #[test]
    fn push_keeps_the_most_recent() {
        let mut list = Vec::new();
        for i in 0..MAX_RECENT + 5 {
            push(&mut list, format!("/{i}.pptx").into());
        }
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0], PathBuf::from(format!("/{}.pptx", MAX_RECENT + 4)));
        assert_eq!(list[MAX_RECENT - 1], PathBuf::from("/5.pptx"));
    }

    #[test]
    fn parse_keeps_order_and_skips_blank_and_repeated_lines() {
        let list = parse("/a.pptx\n\n/b.pptx\n/a.pptx\n");
        assert_eq!(list, [PathBuf::from("/a.pptx"), PathBuf::from("/b.pptx")]);
    }
}
