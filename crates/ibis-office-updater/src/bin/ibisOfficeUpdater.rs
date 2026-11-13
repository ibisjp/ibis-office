// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibisOfficeUpdater: started by ibisOffice (which then quits) after it has
//! downloaded, unpacked and checked a newer version.
//!
//! ```text
//! ibisOfficeUpdater --pid <ibisOffice's pid> --old <installed> --new <unpacked>
//!                   --version <new version> --log <updater.log> [--workdir <folder to remove>]
//! ```
//!
//! `installed` and `unpacked` are the `.app` folders on macOS and the folders
//! holding `ibisOffice.exe` on Windows. The steps, each logged as it starts
//! and as it succeeds or fails:
//!
//! 1. `wait-exit`: wait for ibisOffice to quit (30 s; then end it).
//! 2. `install`: replace the installed copy (`swap`. macOS: the new one copied
//!    next to it and checked, the old one moved aside, the copy moved into
//!    place. Windows: the files of the new version replace the old ones one by
//!    one, and the user's own files in the folder stay). The old files are kept aside.
//! 3. `launch` and `check-running`: start the new ibisOffice; it must still
//!    be running after a few seconds.
//! 4. `cleanup`: remove the old copy and the unpacked folder.
//!
//! If a step fails the old copy is put back (`rollback`) and started again.
//! The exit code is 0 only for a finished update.

use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", windows))]
use std::process::{Command, Stdio};
use std::time::Duration;

use ibis_office_updater::log::Log;
use ibis_office_updater::process::{self, Exit};

struct Args {
    pid: u32,
    old: PathBuf,
    new: PathBuf,
    version: String,
    log: PathBuf,
    workdir: Option<PathBuf>,
    wait: Duration,
}

fn parse() -> Result<Args, String> {
    let mut map = std::collections::HashMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(key) = args.next() {
        let value = args.next().ok_or(format!("{key} needs a value"))?;
        map.insert(key, value);
    }
    let get = |key: &str| map.get(key).cloned().ok_or(format!("{key} is missing"));
    Ok(Args {
        pid: get("--pid")?.parse().map_err(|_| "--pid is not a number".to_string())?,
        old: PathBuf::from(get("--old")?),
        new: PathBuf::from(get("--new")?),
        version: get("--version")?,
        log: PathBuf::from(get("--log")?),
        workdir: map.get("--workdir").map(PathBuf::from),
        wait: Duration::from_secs(map.get("--wait-secs").and_then(|s| s.parse().ok()).unwrap_or(30)),
    })
}

fn main() {
    let args = match parse() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("ibisOfficeUpdater: {err}");
            std::process::exit(2);
        }
    };
    let log = Log::new(&args.log, &args.version);
    std::process::exit(run(&args, &log));
}

fn run(args: &Args, log: &Log) -> i32 {
    log.start("update", &format!("ibisOfficeUpdater {}, {} -> {}", env!("CARGO_PKG_VERSION"), args.new.display(), args.old.display()));

    log.start("wait-exit", &format!("pid {}", args.pid));
    match process::wait_for_exit(args.pid, args.wait) {
        Exit::Quit => log.ok("wait-exit", "ibisOffice quit"),
        Exit::Ended => log.ok("wait-exit", "ibisOffice did not quit in time and was ended"),
        Exit::Running => {
            log.fail("wait-exit", "ibisOffice is still running; nothing was changed");
            return 1;
        }
    }

    log.start("install", "replacing the installed copy");
    if let Err(err) = platform::install(&args.old, &args.new) {
        log.fail("install", &err);
        relaunch_old(args, log);
        return 1;
    }
    log.ok("install", "the new copy is in place; the old one is kept aside");

    log.start("launch", &args.old.display().to_string());
    match platform::launch(&args.old) {
        Ok(mut started) => {
            log.ok("launch", "started");
            log.start("check-running", "ibisOffice must keep running for a few seconds");
            if let Err(err) = platform::check_running(&args.old, &mut started) {
                log.fail("check-running", &err);
                rollback_and_relaunch(args, log);
                return 1;
            }
            log.ok("check-running", "running");
        }
        Err(err) => {
            log.fail("launch", &err);
            rollback_and_relaunch(args, log);
            return 1;
        }
    }

    log.start("cleanup", "removing the old copy and the unpacked files");
    let mut failed = false;
    if let Err(err) = platform::cleanup(&args.old, &args.new) {
        log.fail("cleanup", &err);
        failed = true;
    }
    if let Some(dir) = &args.workdir
        && let Err(err) = remove_dir(dir)
    {
        log.fail("cleanup", &format!("{}: {err}", dir.display()));
        failed = true;
    }
    if !failed {
        log.ok("cleanup", "done");
    }
    log.ok("update", &format!("updated to {}", args.version));
    0
}

/// Remove a folder; ibisOfficeUpdater itself may be running from inside it (Windows cannot delete a running program: the app removes what is left when it next starts).
fn remove_dir(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(err) if dir.exists() => {
            #[cfg(windows)]
            {
                // This program is in that folder: a program running cannot be deleted, so a command removes the folder a few seconds after this one has ended.
                use std::os::windows::process::CommandExt as _;
                let line = format!("ping -n 4 127.0.0.1 >nul & rmdir /S /Q \"{}\"", dir.display());
                if Command::new("cmd").args(["/C", &line]).creation_flags(0x0800_0008).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().is_ok() {
                    return Ok(());
                }
            }
            Err(err)
        }
        _ => Ok(()),
    }
}

fn rollback_and_relaunch(args: &Args, log: &Log) {
    log.start("rollback", "putting the old copy back");
    match platform::rollback(&args.old, &args.new) {
        Ok(()) => log.ok("rollback", "the old copy is back"),
        Err(err) => log.fail("rollback", &err),
    }
    relaunch_old(args, log);
}

fn relaunch_old(args: &Args, log: &Log) {
    log.start("relaunch-old", &args.old.display().to_string());
    match platform::launch(&args.old) {
        Ok(_) => log.ok("relaunch-old", "started"),
        Err(err) => log.fail("relaunch-old", &err),
    }
}

/// Run a command; its output is the error text when it fails (macOS: `open`, `codesign`).
#[cfg(target_os = "macos")]
fn run_command(command: &mut Command) -> Result<(), String> {
    let out = command.stdin(Stdio::null()).output().map_err(|e| format!("{:?}: {e}", command.get_program()))?;
    if out.status.success() {
        return Ok(());
    }
    let text = format!("{} {}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    Err(format!("{:?} {}: {}", command.get_program(), out.status, text.trim()))
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use ibis_office_updater::swap;

    pub struct Started;

    /// The copy must be a whole, validly signed app before it replaces anything.
    fn check(app: &Path) -> Result<(), String> {
        run_command(Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(app))
    }

    pub fn install(old: &Path, new: &Path) -> Result<(), String> {
        swap::install(old, new, &check)
    }

    pub fn rollback(old: &Path, _: &Path) -> Result<(), String> {
        swap::rollback(old)
    }

    pub fn cleanup(old: &Path, _: &Path) -> Result<(), String> {
        swap::cleanup(old)
    }

    pub fn launch(app: &Path) -> Result<Started, String> {
        run_command(Command::new("/usr/bin/open").arg(app))?;
        Ok(Started)
    }

    /// Seen running within 20 seconds, and still running 3 seconds after.
    pub fn check_running(app: &Path, _: &mut Started) -> Result<(), String> {
        let exe = app.join("Contents/MacOS/ibisOffice");
        let start = std::time::Instant::now();
        while !process::is_running(&exe) {
            if start.elapsed() > Duration::from_secs(20) {
                return Err("ibisOffice did not start within 20 seconds".into());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        std::thread::sleep(Duration::from_secs(3));
        if process::is_running(&exe) { Ok(()) } else { Err("ibisOffice quit within 3 seconds of starting".into()) }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use ibis_office_updater::swap;
    use std::os::windows::process::CommandExt as _;

    pub type Started = std::process::Child;

    pub fn install(old: &Path, new: &Path) -> Result<(), String> {
        swap::install_files(old, new, &swap::parse_list(swap::DISTRIBUTED))
    }

    pub fn rollback(old: &Path, new: &Path) -> Result<(), String> {
        swap::rollback_files(old, new)
    }

    pub fn cleanup(old: &Path, _: &Path) -> Result<(), String> {
        swap::cleanup_files(old)
    }

    pub fn launch(dir: &Path) -> Result<Started, String> {
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        let exe = dir.join("ibisOffice.exe");
        Command::new(&exe).current_dir(dir).creation_flags(DETACHED_PROCESS).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("{}: {e}", exe.display()))
    }

    /// Still running 3 seconds after it was started.
    pub fn check_running(_: &Path, child: &mut Started) -> Result<(), String> {
        std::thread::sleep(Duration::from_secs(3));
        match child.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(format!("ibisOffice quit within 3 seconds of starting ({status})")),
            Err(err) => Err(format!("cannot tell whether ibisOffice runs: {err}")),
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use super::*;

    pub struct Started;

    fn unsupported<T>() -> Result<T, String> {
        Err("ibisOfficeUpdater runs on macOS and Windows only".into())
    }

    pub fn install(_: &Path, _: &Path) -> Result<(), String> {
        unsupported()
    }

    pub fn rollback(_: &Path, _: &Path) -> Result<(), String> {
        unsupported()
    }

    pub fn cleanup(_: &Path, _: &Path) -> Result<(), String> {
        unsupported()
    }

    pub fn launch(_: &Path) -> Result<Started, String> {
        unsupported()
    }

    pub fn check_running(_: &Path, _: &mut Started) -> Result<(), String> {
        unsupported()
    }
}
