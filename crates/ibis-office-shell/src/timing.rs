// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `--timing`: stages of starting, in milliseconds since the process
//! started.

use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

static START: OnceLock<Option<SystemTime>> = OnceLock::new();

/// Start timing if `on`, from when the system says the process began
/// (before the executable was loaded), or failing that from now.
pub fn enable(on: bool) {
    START.get_or_init(|| on.then(|| process_start().unwrap_or_else(crate::clock::system_now)));
}

static QUIT_AFTER_DOCUMENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FILE_READ_START: OnceLock<SystemTime> = OnceLock::new();

/// Mark the start of reading the file.
pub(crate) fn mark_read_start() {
    FILE_READ_START.get_or_init(crate::clock::system_now);
    mark("read-start");
}

/// Quit once the document opened at launch has been drawn twice.
pub fn quit_after_document(on: bool) {
    QUIT_AFTER_DOCUMENT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The file is loaded into `window`: mark it and the next two frames,
/// the document drawn, and quit then if asked to.
pub(crate) fn document_loaded(window: &mut gpui_kit::Window) {
    mark("loaded");
    if let (Some(Some(_)), Some(read_start)) = (START.get(), FILE_READ_START.get()) {
        let ms = crate::clock::system_now().duration_since(*read_start).unwrap_or(Duration::ZERO).as_secs_f64() * 1000.0;
        eprintln!("ibisOffice timing: read-start to loaded: {ms:.1} ms ({:.3} s)", ms / 1000.0);
    }
    window.on_next_frame(|window, _| {
        mark("document-frame");
        if let (Some(Some(_)), Some(read_start)) = (START.get(), FILE_READ_START.get()) {
            let ms = crate::clock::system_now().duration_since(*read_start).unwrap_or(Duration::ZERO).as_secs_f64() * 1000.0;
            eprintln!("ibisOffice timing: read-start to document-frame (render complete): {ms:.1} ms ({:.3} s)", ms / 1000.0);
        }
        window.on_next_frame(|_, cx| {
            mark("document-second-frame");
            if QUIT_AFTER_DOCUMENT.swap(false, std::sync::atomic::Ordering::Relaxed) {
                cx.quit();
            }
        });
        window.refresh();
    });
    window.refresh();
}

pub fn mark(stage: &str) {
    if let Some(Some(start)) = START.get() {
        let ms = crate::clock::system_now().duration_since(*start).unwrap_or(Duration::ZERO).as_secs_f64() * 1000.0;
        eprintln!("ibisOffice timing: {stage} {ms:.1}");
    }
}

/// When this process was started, from `sysctl kern.proc.pid` (macOS).
#[cfg(target_os = "macos")]
fn process_start() -> Option<SystemTime> {
    // struct kinfo_proc begins with kp_proc, whose first member, the
    // union p_un, holds __p_starttime (a timeval) at offset 0.
    const CTL_KERN: i32 = 1;
    const KERN_PROC: i32 = 14;
    const KERN_PROC_PID: i32 = 1;
    const KINFO_PROC_SIZE: usize = 648;
    const STARTTIME_OFFSET: usize = 0;
    unsafe extern "C" {
        fn sysctl(name: *const i32, namelen: u32, oldp: *mut u8, oldlenp: *mut usize, newp: *const u8, newlen: usize) -> i32;
    }
    let mib = [CTL_KERN, KERN_PROC, KERN_PROC_PID, std::process::id() as i32];
    let mut buf = [0u8; KINFO_PROC_SIZE];
    let mut len = buf.len();
    // SAFETY: the buffer and its length are valid for the call; the
    // kernel writes at most `len` bytes.
    let ok = unsafe { sysctl(mib.as_ptr(), mib.len() as u32, buf.as_mut_ptr(), &mut len, std::ptr::null(), 0) } == 0;
    if !ok || len < STARTTIME_OFFSET + 16 {
        return None;
    }
    let secs = i64::from_ne_bytes(buf[STARTTIME_OFFSET..STARTTIME_OFFSET + 8].try_into().ok()?);
    let usecs = i32::from_ne_bytes(buf[STARTTIME_OFFSET + 8..STARTTIME_OFFSET + 12].try_into().ok()?);
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(secs.max(0) as u64) + Duration::from_micros(usecs.max(0) as u64);
    // Sanity: within the last minute.
    crate::clock::system_now().duration_since(start).ok().filter(|d| *d < Duration::from_secs(60)).map(|_| start)
}

#[cfg(not(target_os = "macos"))]
fn process_start() -> Option<SystemTime> {
    None
}
