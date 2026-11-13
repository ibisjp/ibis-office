// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Replacing an installed folder with a new one, and putting it back, for
//! macOS (the `.app`) and Windows (the folder holding `ibisOffice.exe` and
//! what goes with it). Done here in Rust, not by a script, so that a change
//! of the steps is a new ibisOfficeUpdater (the one in the new version is
//! the one that runs). The order is what keeps the
//! installed copy whole at every moment:
//!
//! 1. `install`: copy the new folder next to the old one (`<old>.update-new`),
//!    move the old one aside (`<old>.update-backup`), move the copy into
//!    place. Anything failing puts the old one back.
//! 2. `rollback`: the old one back in place (the new one did not start).
//! 3. `cleanup`: the backup removed (the new one started).
//!
//! Windows does not swap the folder (see `install_files`): the user may keep their own files in it.

use std::path::{Path, PathBuf};

fn beside(old: &Path, suffix: &str) -> PathBuf {
    let mut name = old.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    old.with_file_name(name)
}

/// The old copy, moved aside by `install`.
pub fn backup_of(old: &Path) -> PathBuf {
    beside(old, ".update-backup")
}

fn staged_of(old: &Path) -> PathBuf {
    beside(old, ".update-new")
}

/// Copy a folder. macOS: `ditto`, which keeps everything a signed .app has (symbolic links, extended attributes).
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/bin/ditto").arg(from).arg(to).output().map_err(|e| format!("ditto: {e}"))?;
        if out.status.success() { Ok(()) } else { Err(format!("ditto {}: {}", out.status, String::from_utf8_lossy(&out.stderr).trim())) }
    }
    #[cfg(not(target_os = "macos"))]
    copy_dir(from, to).map_err(|e| e.to_string())
}

#[allow(dead_code)] // macOS copies with ditto
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&src, &dst)?;
        } else if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(std::fs::read_link(&src)?, &dst)?;
            #[cfg(not(unix))]
            {
                std::fs::copy(&src, &dst)?;
            }
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Put `new` in place of `old`. `check` looks at the copy (made next to `old`) before anything is replaced. On an error `old` is as it was.
pub fn install(old: &Path, new: &Path, check: &dyn Fn(&Path) -> Result<(), String>) -> Result<(), String> {
    let (staged, backup) = (staged_of(old), backup_of(old));
    let _ = std::fs::remove_dir_all(&staged);
    let _ = std::fs::remove_dir_all(&backup);
    if let Err(err) = copy_tree(new, &staged) {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(format!("copying {} to {}: {err}", new.display(), staged.display()));
    }
    if let Err(err) = check(&staged) {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(format!("the copy {} is not good: {err}", staged.display()));
    }
    if let Err(err) = std::fs::rename(old, &backup) {
        let _ = std::fs::remove_dir_all(&staged);
        return Err(format!("moving {} aside: {err}", old.display()));
    }
    if let Err(err) = std::fs::rename(&staged, old) {
        let back = std::fs::rename(&backup, old);
        let _ = std::fs::remove_dir_all(&staged);
        return Err(format!("moving the new copy into place: {err}{}", back.err().map(|e| format!(" (and putting the old one back: {e})")).unwrap_or_default()));
    }
    Ok(())
}

/// The old copy back in place of the new one.
pub fn rollback(old: &Path) -> Result<(), String> {
    let backup = backup_of(old);
    if !backup.is_dir() {
        return Err(format!("{} is not there", backup.display()));
    }
    std::fs::remove_dir_all(old).map_err(|e| format!("removing the new copy {}: {e}", old.display()))?;
    std::fs::rename(&backup, old).map_err(|e| format!("moving {} back: {e}", backup.display()))
}

/// The backup removed.
pub fn cleanup(old: &Path) -> Result<(), String> {
    let backup = backup_of(old);
    if backup.exists() {
        std::fs::remove_dir_all(&backup).map_err(|e| format!("removing {}: {e}", backup.display()))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Windows: the install folder is the user's too (they may keep files next to ibisOffice.exe), so the folder is not swapped: the
// files the new version has are replaced one by one, and only the files of the list of what ibisOffice ever distributed are removed.
// ---------------------------------------------------------------------------------------------------------------------------------

/// What every version of ibisOffice's Windows zip has put in the install folder (`distributed-files.txt`, embedded).
pub const DISTRIBUTED: &str = include_str!("../distributed-files.txt");

/// The relative paths of a list (`distributed-files.txt`): one per line, `#` comments and blank lines skipped, `/` between folders.
pub fn parse_list(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(|l| l.replace('\\', "/")).collect()
}

/// The key a relative path is compared by (Windows file names do not differ by case).
fn key_of(rel: &str) -> String {
    rel.replace('\\', "/").to_lowercase()
}

/// A relative path that stays inside the install folder.
fn is_safe(rel: &str) -> bool {
    !rel.is_empty() && !rel.starts_with('/') && !rel.contains(':') && !rel.split('/').any(|p| p == ".." || p == ".")
}

/// Every file under `root` (relative paths with `/`); folders are not listed.
pub fn files_under(root: &Path) -> std::io::Result<Vec<String>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                walk(root, &path, out)?;
            } else {
                let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

/// The files of `installed` that are not in the list `distributed`: the user's own (the app tells how many before an update).
pub fn user_files(installed: &Path, distributed: &[String]) -> Vec<String> {
    let known: std::collections::HashSet<String> = distributed.iter().map(|d| key_of(d)).collect();
    files_under(installed).unwrap_or_default().into_iter().filter(|f| !known.contains(&key_of(f))).collect()
}

fn remove_file_any(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(_) if path.exists() => {
            // A read-only file cannot be deleted on Windows until it is made writable.
            if let Ok(meta) = std::fs::metadata(path) {
                let mut perm = meta.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perm.set_readonly(false);
                let _ = std::fs::set_permissions(path, perm);
            }
            std::fs::remove_file(path)
        }
        other => other,
    }
}

fn journal_of(backup: &Path) -> PathBuf {
    backup.join(".update-journal")
}

/// Put the files of `new` in `old` one by one, the old version of each moved to `<old>.update-backup` first, and remove the files of
/// `distributed` that `new` lacks (also moved there). Files of `old` that are not in `new` or in the list are not touched. On an error
/// everything is put back, and `old` is as it was.
pub fn install_files(old: &Path, new: &Path, distributed: &[String]) -> Result<(), String> {
    let backup = backup_of(old);
    if backup.exists() {
        // Ours, from an update that did not finish.
        remove_tree(&backup).map_err(|e| format!("removing the earlier backup {}: {e}", backup.display()))?;
    }
    let new_files = files_under(new).map_err(|e| format!("listing {}: {e}", new.display()))?;
    if new_files.is_empty() {
        return Err(format!("{} has no files", new.display()));
    }
    if let Some(bad) = new_files.iter().find(|f| !is_safe(f)) {
        return Err(format!("the new version has a file outside its folder: {bad}"));
    }
    let present: std::collections::HashSet<String> = new_files.iter().map(|f| key_of(f)).collect();
    let removals: Vec<String> = distributed.iter().filter(|d| is_safe(d) && !present.contains(&key_of(d)) && old.join(d).is_file()).cloned().collect();
    std::fs::create_dir_all(&backup).map_err(|e| format!("making {}: {e}", backup.display()))?;
    let mut done = Journal::default();
    let result = (|| -> Result<(), String> {
        for rel in &new_files {
            let (target, aside) = (old.join(rel), backup.join(rel));
            if target.is_file() {
                move_aside(&target, &aside, &mut done, rel)?;
            }
            if let Some(dir) = target.parent() {
                make_dirs(dir, old, &mut done)?;
            }
            // Noted before the copy: a copy that stops half way leaves a file that has to go.
            done.placed.push(rel.clone());
            std::fs::copy(new.join(rel), &target).map_err(|e| format!("copying {rel}: {e}"))?;
        }
        for rel in &removals {
            move_aside(&old.join(rel), &backup.join(rel), &mut done, rel)?;
        }
        Ok(())
    })();
    let _ = std::fs::write(journal_of(&backup), done.created_dirs.join("\n"));
    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            let back = undo(old, &backup, &done.placed, &done.created_dirs);
            let _ = remove_tree(&backup);
            Err(format!("{err}{}", back.err().map(|e| format!(" (and putting the old files back: {e})")).unwrap_or_default()))
        }
    }
}

#[derive(Default)]
struct Journal {
    /// Files of the new version that were put in place.
    placed: Vec<String>,
    /// Relative paths of the folders made for them.
    created_dirs: Vec<String>,
}

fn move_aside(from: &Path, to: &Path, _done: &mut Journal, rel: &str) -> Result<(), String> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("making {}: {e}", dir.display()))?;
    }
    std::fs::rename(from, to).map_err(|e| format!("moving {rel} aside: {e}"))
}

/// Make `dir` and the folders above it that are missing (below `root`), noting each one made.
fn make_dirs(dir: &Path, root: &Path, done: &mut Journal) -> Result<(), String> {
    if dir.is_dir() || dir == root {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        make_dirs(parent, root, done)?;
    }
    std::fs::create_dir(dir).map_err(|e| format!("making {}: {e}", dir.display()))?;
    done.created_dirs.push(dir.strip_prefix(root).unwrap_or(dir).to_string_lossy().replace('\\', "/"));
    Ok(())
}

/// Put things back: the new files taken out of `old` (`placed`), everything in `backup` moved back, the folders made removed.
fn undo(old: &Path, backup: &Path, placed: &[String], created_dirs: &[String]) -> Result<(), String> {
    let mut problems = Vec::new();
    for rel in placed {
        let target = old.join(rel);
        if target.is_file() && let Err(e) = remove_file_any(&target) {
            problems.push(format!("removing {rel}: {e}"));
        }
    }
    for rel in files_under(backup).unwrap_or_default() {
        if rel == ".update-journal" {
            continue;
        }
        let target = old.join(&rel);
        if let Some(dir) = target.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::rename(backup.join(&rel), &target) {
            problems.push(format!("moving {rel} back: {e}"));
        }
    }
    for rel in created_dirs.iter().rev() {
        let _ = std::fs::remove_dir(old.join(rel));
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("; ")) }
}

/// The old files back in place of the new ones (the new version did not start).
pub fn rollback_files(old: &Path, new: &Path) -> Result<(), String> {
    let backup = backup_of(old);
    if !backup.is_dir() {
        return Err(format!("{} is not there", backup.display()));
    }
    let placed = files_under(new).map_err(|e| format!("listing {}: {e}", new.display()))?;
    let created: Vec<String> = std::fs::read_to_string(journal_of(&backup)).unwrap_or_default().lines().map(str::to_string).filter(|l| !l.is_empty()).collect();
    undo(old, &backup, &placed, &created)?;
    remove_tree(&backup).map_err(|e| format!("removing {}: {e}", backup.display()))
}

/// The backup removed: its files one by one (a read-only one too), then its folders. It holds only what the update moved aside.
pub fn cleanup_files(old: &Path) -> Result<(), String> {
    let backup = backup_of(old);
    if backup.exists() {
        remove_tree(&backup).map_err(|e| format!("removing {}: {e}", backup.display()))?;
    }
    Ok(())
}

/// Remove a folder of ours file by file, making read-only files writable first.
fn remove_tree(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            remove_tree(&path)?;
        } else {
            remove_file_any(&path)?;
        }
    }
    std::fs::remove_dir(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ibis-updater-swap-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("old/sub")).unwrap();
        std::fs::create_dir_all(dir.join("new/sub")).unwrap();
        std::fs::write(dir.join("old/app.exe"), "old").unwrap();
        std::fs::write(dir.join("old/sub/data"), "old data").unwrap();
        std::fs::write(dir.join("new/app.exe"), "new").unwrap();
        std::fs::write(dir.join("new/sub/data"), "new data").unwrap();
        dir
    }

    #[test]
    fn install_then_cleanup() {
        let dir = tree("ok");
        install(&dir.join("old"), &dir.join("new"), &|_| Ok(())).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("old/app.exe")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(dir.join("old/sub/data")).unwrap(), "new data");
        assert_eq!(std::fs::read_to_string(backup_of(&dir.join("old")).join("app.exe")).unwrap(), "old");
        cleanup(&dir.join("old")).unwrap();
        assert!(!backup_of(&dir.join("old")).exists());
        assert!(dir.join("new").is_dir());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_copy_that_fails_the_check_leaves_the_old_one() {
        let dir = tree("check");
        assert!(install(&dir.join("old"), &dir.join("new"), &|_| Err("bad".into())).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("old/app.exe")).unwrap(), "old");
        assert!(!staged_of(&dir.join("old")).exists() && !backup_of(&dir.join("old")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rollback_puts_the_old_one_back() {
        let dir = tree("rollback");
        install(&dir.join("old"), &dir.join("new"), &|_| Ok(())).unwrap();
        rollback(&dir.join("old")).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("old/app.exe")).unwrap(), "old");
        assert!(!backup_of(&dir.join("old")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_new_folder_leaves_the_old_one() {
        let dir = tree("missing");
        assert!(install(&dir.join("old"), &dir.join("nothing"), &|_| Ok(())).is_err());
        assert_eq!(std::fs::read_to_string(dir.join("old/app.exe")).unwrap(), "old");
        assert!(!backup_of(&dir.join("old")).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- Windows: the files are replaced one by one and the user's files stay ----

    fn list() -> Vec<String> {
        parse_list("# comment\nibisOffice.exe\nibisOfficeUpdater.exe\nResources/Licenses/license.md\nResources/old/gone.txt\n")
    }

    fn put(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    /// An install folder of an old version with the user's files in it (also in a folder, one read-only), and a new version that has a
    /// folder of its own (`bin`) and lacks `Resources/old/gone.txt`.
    fn install_tree(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ibis-updater-files-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let old = dir.join("ibisOffice");
        put(&old.join("ibisOffice.exe"), "old exe");
        put(&old.join("ibisOfficeUpdater.exe"), "old updater");
        put(&old.join("Resources/Licenses/license.md"), "old licence");
        put(&old.join("Resources/old/gone.txt"), "old gone");
        put(&old.join("report.docx"), "my docx");
        put(&old.join("data.xlsx"), "my xlsx");
        put(&old.join("slides/deck.pptx"), "my pptx");
        put(&old.join("slides/deep/notes.txt"), "my notes");
        put(&old.join("Resources/Licenses/mine.txt"), "my file among the licences");
        let readonly = old.join("readonly.docx");
        put(&readonly, "read only");
        let mut perm = std::fs::metadata(&readonly).unwrap().permissions();
        perm.set_readonly(true);
        std::fs::set_permissions(&readonly, perm).unwrap();
        let new = dir.join("unpacked/ibisOffice");
        put(&new.join("ibisOffice.exe"), "new exe");
        put(&new.join("ibisOfficeUpdater.exe"), "new updater");
        put(&new.join("Resources/Licenses/license.md"), "new licence");
        put(&new.join("bin/helper.dll"), "new dll");
        dir
    }

    fn user_files_stay(old: &Path) {
        assert_eq!(read(&old.join("report.docx")), "my docx");
        assert_eq!(read(&old.join("data.xlsx")), "my xlsx");
        assert_eq!(read(&old.join("slides/deck.pptx")), "my pptx");
        assert_eq!(read(&old.join("slides/deep/notes.txt")), "my notes");
        assert_eq!(read(&old.join("Resources/Licenses/mine.txt")), "my file among the licences");
        assert_eq!(read(&old.join("readonly.docx")), "read only");
        assert!(std::fs::metadata(old.join("readonly.docx")).unwrap().permissions().readonly(), "a read-only file stays read-only");
    }

    fn finish(dir: &Path) {
        // The read-only file has to be writable for the folder to go.
        let readonly = dir.join("ibisOffice/readonly.docx");
        if let Ok(meta) = std::fs::metadata(&readonly) {
            let mut perm = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = std::fs::set_permissions(&readonly, perm);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn files_install_keeps_the_users_files_and_removes_what_the_new_version_dropped() {
        let dir = install_tree("keep");
        let (old, new) = (dir.join("ibisOffice"), dir.join("unpacked/ibisOffice"));
        install_files(&old, &new, &list()).unwrap();
        assert_eq!(read(&old.join("ibisOffice.exe")), "new exe");
        assert_eq!(read(&old.join("ibisOfficeUpdater.exe")), "new updater", "the updater is beside the exe");
        assert_eq!(read(&old.join("Resources/Licenses/license.md")), "new licence");
        assert_eq!(read(&old.join("bin/helper.dll")), "new dll", "a folder only the new version has is made");
        assert!(!old.join("Resources/old/gone.txt").exists(), "the old version's file the new one lacks is removed");
        user_files_stay(&old);
        // The old files are aside until the cleanup.
        assert_eq!(read(&backup_of(&old).join("ibisOffice.exe")), "old exe");
        cleanup_files(&old).unwrap();
        assert!(!backup_of(&old).exists());
        user_files_stay(&old);
        assert!(new.join("ibisOffice.exe").exists(), "the unpacked folder is not touched here");
        finish(&dir);
    }

    #[test]
    fn files_rollback_puts_everything_back_and_the_users_files_stay() {
        let dir = install_tree("rollback");
        let (old, new) = (dir.join("ibisOffice"), dir.join("unpacked/ibisOffice"));
        install_files(&old, &new, &list()).unwrap();
        rollback_files(&old, &new).unwrap();
        assert_eq!(read(&old.join("ibisOffice.exe")), "old exe");
        assert_eq!(read(&old.join("ibisOfficeUpdater.exe")), "old updater");
        assert_eq!(read(&old.join("Resources/old/gone.txt")), "old gone");
        assert!(!old.join("bin").exists(), "the folder the new version made is gone again");
        assert!(!backup_of(&old).exists());
        user_files_stay(&old);
        finish(&dir);
    }

    #[test]
    fn files_a_failure_half_way_puts_everything_back() {
        let dir = install_tree("fail");
        let (old, new) = (dir.join("ibisOffice"), dir.join("unpacked/ibisOffice"));
        // The new version's `bin\helper.dll` cannot go in place: a folder of that name is in the way.
        std::fs::create_dir_all(old.join("bin/helper.dll")).unwrap();
        put(&old.join("bin/helper.dll/inside.txt"), "the user's");
        assert!(install_files(&old, &new, &list()).is_err());
        assert_eq!(read(&old.join("ibisOffice.exe")), "old exe");
        assert_eq!(read(&old.join("ibisOfficeUpdater.exe")), "old updater");
        assert_eq!(read(&old.join("Resources/Licenses/license.md")), "old licence");
        assert_eq!(read(&old.join("Resources/old/gone.txt")), "old gone");
        assert_eq!(read(&old.join("bin/helper.dll/inside.txt")), "the user's");
        assert!(!backup_of(&old).exists());
        user_files_stay(&old);
        finish(&dir);
    }

    #[test]
    fn files_nothing_outside_the_list_is_removed_even_when_the_new_version_lacks_it() {
        let dir = install_tree("list");
        let (old, new) = (dir.join("ibisOffice"), dir.join("unpacked/ibisOffice"));
        // A list without `Resources/old/gone.txt`: that file is the user's now.
        install_files(&old, &new, &parse_list("ibisOffice.exe\nibisOfficeUpdater.exe\nResources/Licenses/license.md")).unwrap();
        assert_eq!(read(&old.join("Resources/old/gone.txt")), "old gone");
        // The names are compared without regard to case; a path that leaves the folder is never removed.
        assert!(user_files(&old, &list()).contains(&"report.docx".to_string()));
        assert!(!user_files(&old, &list()).iter().any(|f| f.eq_ignore_ascii_case("ibisoffice.exe")));
        assert!(!is_safe("../x") && !is_safe("a/../b") && !is_safe("/x") && !is_safe("c:/x") && is_safe("Resources/a.txt"));
        finish(&dir);
    }

    #[test]
    fn the_embedded_list_names_what_the_zips_have_had() {
        let list = parse_list(DISTRIBUTED);
        for file in ["ibisOffice.exe", "ibisOfficeUpdater.exe", "Resources/artborders/ECMA-NOTICE.txt", "Resources/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip", "Resources/Licenses/Carlito-OFL.txt", "Resources/Licenses/license.md"] {
            assert!(list.iter().any(|l| l == file), "{file} is in distributed-files.txt");
        }
        assert!(list.iter().all(|l| is_safe(l)));
    }
}
