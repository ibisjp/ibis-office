// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A file as it was in a Git commit (design 159, 3.1). The `git` program of the computer is started with its arguments as a list (no shell), without the
//! caller's `GIT_*` variables, and never reaches the network. The commit is a hexadecimal SHA only (a branch or tag name is not a version that stays what it was),
//! and the repository must be inside an allowed folder (its history is not read through a folder that is only part of it).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::ToolError;
use crate::paths;

/// A file read from a commit.
pub(crate) struct GitFile {
    /// The full SHA of the commit.
    pub commit: String,
    pub data: Vec<u8>,
}

/// The longest a `git` program may take.
const LIMIT: Duration = Duration::from_secs(60);

struct Run {
    code: i32,
    stdout: Vec<u8>,
    stderr: String,
}

fn git(root: &Path, args: &[&str]) -> Result<Run, ToolError> {
    let mut command = Command::new("git");
    command.arg("--no-optional-locks").arg("-C").arg(paths::shown(root)).args(args);
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().to_ascii_uppercase().starts_with("GIT_") {
            command.env_remove(&name);
        }
    }
    command.env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_TERMINAL_PROMPT", "0").env("GIT_NO_LAZY_FETCH", "1");
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ToolError::new("git_not_available", "the git program was not found on this computer: install Git (https://git-scm.com/downloads) and put it on the PATH to read a version from a commit")
        } else {
            ToolError::new("internal", format!("git: {e}"))
        }
    })?;
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(out) = out.as_mut() {
            let _ = out.read_to_end(&mut bytes);
        }
        bytes
    });
    let error_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(err) = err.as_mut() {
            let _ = err.read_to_end(&mut bytes);
        }
        bytes
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ToolError::new("internal", "git took more than 60 seconds"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(ToolError::new("internal", format!("git: {e}"))),
        }
    };
    let stdout = reader.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&error_reader.join().unwrap_or_default()).trim().to_string();
    Ok(Run { code: status.code().unwrap_or(-1), stdout, stderr })
}

/// The root of the repository `file` is in: the nearest folder above it with a `.git`.
fn repository_of(file: &Path) -> Option<PathBuf> {
    file.parent()?.ancestors().find(|dir| dir.join(".git").exists()).map(Path::to_path_buf)
}

/// What `commit` (4 to 64 hexadecimal digits, not a name) was for the file at `file` (a real path under an allowed folder), at most `max_size` bytes.
pub(crate) fn read_commit(allow: &[PathBuf], file: &Path, commit: &str, max_size: u64) -> Result<GitFile, ToolError> {
    if !(4..=64).contains(&commit.len()) || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ToolError::invalid("commit is a SHA of 4 to 64 hexadecimal digits (a branch name, a tag or HEAD~1 is not accepted: it does not stay one version)"));
    }
    let commit = commit.to_ascii_lowercase();
    let root = repository_of(file).ok_or_else(|| ToolError::new("unsupported_revision", format!("{} is not in a Git repository", file.display())))?;
    if !allow.iter().any(|dir| root.starts_with(dir)) {
        return Err(ToolError::new("not_allowed_path", "the Git repository of this file is not inside an allowed folder: its history is not read"));
    }
    let relative = file
        .strip_prefix(&root)
        .ok()
        .and_then(|r| r.components().map(|c| c.as_os_str().to_str()).collect::<Option<Vec<_>>>())
        .map(|parts| parts.join("/"))
        .ok_or_else(|| ToolError::invalid("the path is not in the repository"))?;
    let refused = |run: &Run, fallback: ToolError| {
        let text = run.stderr.to_ascii_lowercase();
        if text.contains("dubious ownership") || text.contains("not a git repository") {
            ToolError::new("unsupported_revision", format!("git cannot use this repository: {}", run.stderr))
        } else if text.contains("ambiguous") {
            ToolError::invalid(format!("the short SHA {commit} matches more than one object: give more digits"))
        } else {
            fallback
        }
    };
    // The commit, in full.
    let revision = format!("{commit}^{{commit}}");
    let run = git(&root, &["rev-parse", "--verify", "--end-of-options", &revision])?;
    let full = String::from_utf8_lossy(&run.stdout).trim().to_ascii_lowercase();
    if run.code != 0 || !full.starts_with(&commit) || full.len() < 40 {
        return Err(refused(&run, ToolError::new("revision_not_found", format!("no commit {commit} in the repository"))));
    }
    // The file in it: a plain file only.
    let run = git(&root, &["ls-tree", "-z", &full, "--", &relative])?;
    if run.code != 0 {
        return Err(refused(&run, ToolError::new("internal", format!("git ls-tree: {}", run.stderr))));
    }
    let listing = String::from_utf8_lossy(&run.stdout).into_owned();
    let entry = listing.split('\0').filter_map(|line| line.split_once('\t')).find(|(_, name)| *name == relative).map(|(info, _)| info.to_string());
    let Some(entry) = entry else {
        return Err(ToolError::new("file_not_found", format!("{relative} is not in the commit {full}")));
    };
    let mut fields = entry.split(' ');
    let (mode, kind, object) = (fields.next().unwrap_or(""), fields.next().unwrap_or(""), fields.next().unwrap_or(""));
    if mode == "120000" || kind == "commit" || kind != "blob" || object.len() < 40 || !object.bytes().all(|b| b.is_ascii_hexdigit()) {
        let what = if mode == "120000" { "a symbolic link" } else if kind == "commit" { "a submodule" } else { "not a file" };
        return Err(ToolError::new("unsupported_revision", format!("{relative} in the commit {full} is {what}")));
    }
    let run = git(&root, &["cat-file", "-s", object])?;
    let size: u64 = String::from_utf8_lossy(&run.stdout).trim().parse().map_err(|_| ToolError::new("internal", format!("git cat-file -s: {}", run.stderr)))?;
    if size > max_size {
        return Err(ToolError::new("file_too_large", format!("{size} bytes in the commit; the limit is {max_size} (--max-file-size)")));
    }
    let run = git(&root, &["cat-file", "blob", object])?;
    if run.code != 0 {
        return Err(ToolError::new("internal", format!("git cat-file: {}", run.stderr)));
    }
    if run.stdout.starts_with(b"version https://git-lfs.github.com/spec/") {
        return Err(ToolError::new("unsupported_revision", format!("{relative} in the commit {full} is a Git LFS pointer, not the file")));
    }
    Ok(GitFile { commit: full, data: run.stdout })
}
