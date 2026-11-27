// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Waiting for ibisOffice to quit, and ending it if it will not. Done with
//! the system's own tools (`ps`/`kill`, `tasklist`/`taskkill`), so that
//! nothing but std is needed.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Whether process `pid` is running (a zombie, waiting to be reaped, is not).
pub fn is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let out = Command::new("ps").args(["-o", "stat=", "-p", &pid.to_string()]).stderr(Stdio::null()).output();
        match out {
            Ok(out) => {
                let stat = String::from_utf8_lossy(&out.stdout);
                let stat = stat.trim();
                !stat.is_empty() && !stat.starts_with('Z')
            }
            Err(_) => false,
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        let out = Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"]).creation_flags(0x0800_0000).stderr(Stdio::null()).output();
        match out {
            Ok(out) => String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\"")),
            Err(_) => false,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// Ask process `pid` to end (`force`: make it).
pub fn terminate(pid: u32, force: bool) {
    #[cfg(unix)]
    {
        let signal = if force { "-KILL" } else { "-TERM" };
        let _ = Command::new("kill").args([signal, &pid.to_string()]).stderr(Stdio::null()).status();
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        let mut command = Command::new("taskkill");
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        command.args(["/PID", &pid.to_string()]);
        if force {
            command.arg("/F");
        }
        let _ = command.stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    #[cfg(not(any(unix, windows)))]
    let _ = (pid, force);
}

/// What happened to the process waited for.
#[derive(Debug, PartialEq, Eq)]
pub enum Exit {
    /// It had quit (or was not there).
    Quit,
    /// It did not quit in time and was ended.
    Ended,
    /// It is still running.
    Running,
}

/// Wait up to `timeout` for `pid` to quit; then end it (asking, then forcing).
pub fn wait_for_exit(pid: u32, timeout: Duration) -> Exit {
    let wait = |limit: Duration| {
        let start = Instant::now();
        while is_alive(pid) {
            if start.elapsed() >= limit {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        true
    };
    if wait(timeout) {
        return Exit::Quit;
    }
    terminate(pid, false);
    if wait(Duration::from_secs(5)) {
        return Exit::Ended;
    }
    terminate(pid, true);
    if wait(Duration::from_secs(5)) { Exit::Ended } else { Exit::Running }
}

/// Whether a process runs the program `exe` (the full path; macOS: the
/// executable inside the .app).
#[cfg(target_os = "macos")]
pub fn is_running(exe: &std::path::Path) -> bool {
    // pgrep -f matches the whole command line with a regular expression: the
    // path, then the end of the line or the first argument.
    let mut pattern = String::from("^");
    for c in exe.to_string_lossy().chars() {
        if "\\.^$*+?()[]{}|".contains(c) {
            pattern.push('\\');
        }
        pattern.push(c);
    }
    pattern.push_str("( |$)");
    Command::new("pgrep").args(["-f", &pattern]).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}
