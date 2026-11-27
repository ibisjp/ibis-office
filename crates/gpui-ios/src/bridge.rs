// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/247)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use futures::channel::oneshot;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::ffi::{CStr, CString, c_char, c_void};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, OnceLock};

unsafe extern "C" {
    fn ibis_ios_wake_main();
    fn ibis_ios_show_keyboard();
    fn ibis_ios_hide_keyboard();
    fn ibis_ios_open_url(url: *const c_char);
    fn ibis_ios_get_clipboard_text() -> *mut c_char;
    fn ibis_ios_free_c_string(ptr: *mut c_char);
    fn ibis_ios_set_clipboard_text(text: *const c_char);
    fn ibis_ios_pick_file_for_open(request_id: i64);
    fn ibis_ios_pick_path_for_save(request_id: i64, suggested_name: *const c_char);
    fn ibis_ios_sync_saved_file(path: *const c_char) -> bool;
}

#[derive(Debug)]
pub enum ImeBridgeEvent {
    Commit(String),
    Composing(String),
    Backspace,
    Enter,
}

/// The kind of the soft keyboard on the screen (K0064): the OS tells it with its frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardKind {
    /// At the bottom of the screen, across its width, with no key of its own to hide it (an iPhone's).
    Docked,
    /// The same on an iPad: its right-bottom key hides it.
    DockedWithHideKey,
    /// A small keyboard that floats (an iPad's), or one split in two: narrower than the screen or not at its bottom.
    Floating,
    /// A hardware keyboard is connected: only the small bar of suggestions (an iPad's shortcut bar) or nothing is shown.
    Hardware,
}

/// The soft keyboard now: how much of the bottom of the view it covers (points), its kind, and how long it takes to move (seconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardInfo {
    pub height: f32,
    pub kind: KeyboardKind,
    pub duration: f32,
}

impl KeyboardInfo {
    pub const HIDDEN: KeyboardInfo = KeyboardInfo { height: 0.0, kind: KeyboardKind::Docked, duration: 0.0 };
}

struct BridgeState {
    keyboard: Mutex<KeyboardInfo>,
    next_request_id: AtomicI64,
    open_waiters: Mutex<HashMap<i64, oneshot::Sender<anyhow::Result<Option<Vec<PathBuf>>>>>>,
    save_waiters: Mutex<HashMap<i64, oneshot::Sender<anyhow::Result<Option<PathBuf>>>>>,
    open_paths: Mutex<VecDeque<String>>,
    ime_queue: Mutex<VecDeque<ImeBridgeEvent>>,
}

static BRIDGE: OnceLock<Arc<BridgeState>> = OnceLock::new();

fn state() -> &'static Arc<BridgeState> {
    BRIDGE.get_or_init(|| {
        Arc::new(BridgeState {
            keyboard: Mutex::new(KeyboardInfo::HIDDEN),
            next_request_id: AtomicI64::new(1),
            open_waiters: Mutex::new(HashMap::new()),
            save_waiters: Mutex::new(HashMap::new()),
            open_paths: Mutex::new(VecDeque::new()),
            ime_queue: Mutex::new(VecDeque::new()),
        })
    })
}

/// The soft keyboard as the OS last told (`UIKeyboardWillChangeFrameNotification`, `apps/ios/main.m`).
pub fn keyboard() -> KeyboardInfo {
    *state().keyboard.lock()
}

#[unsafe(no_mangle)]
pub extern "C" fn ibis_ios_keyboard_changed(height: f32, kind: i32, duration: f32) {
    let kind = match kind {
        1 => KeyboardKind::Floating,
        2 => KeyboardKind::Hardware,
        3 => KeyboardKind::DockedWithHideKey,
        _ => KeyboardKind::Docked,
    };
    *state().keyboard.lock() = KeyboardInfo { height: height.max(0.0), kind, duration: duration.max(0.0) };
    wake_main();
}

pub fn wake_main() {
    unsafe { ibis_ios_wake_main() };
}

pub fn show_keyboard() {
    unsafe { ibis_ios_show_keyboard() };
}

pub fn hide_keyboard() {
    unsafe { ibis_ios_hide_keyboard() };
}

pub fn open_url(url: &str) {
    if let Ok(c) = CString::new(url) {
        unsafe { ibis_ios_open_url(c.as_ptr()) };
    }
}

pub fn get_clipboard_text() -> Option<String> {
    let ptr = unsafe { ibis_ios_get_clipboard_text() };
    if ptr.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned();
    unsafe { ibis_ios_free_c_string(ptr) };
    Some(text)
}

pub fn set_clipboard_text(text: &str) {
    if let Ok(c) = CString::new(text) {
        unsafe { ibis_ios_set_clipboard_text(c.as_ptr()) };
    }
}

pub fn pick_file_for_open() -> oneshot::Receiver<anyhow::Result<Option<Vec<PathBuf>>>> {
    let (tx, rx) = oneshot::channel();
    let req_id = state().next_request_id.fetch_add(1, Ordering::Relaxed);
    state().open_waiters.lock().insert(req_id, tx);
    unsafe { ibis_ios_pick_file_for_open(req_id) };
    rx
}

pub fn pick_path_for_save(
    suggested_name: Option<&str>,
) -> oneshot::Receiver<anyhow::Result<Option<PathBuf>>> {
    let (tx, rx) = oneshot::channel();
    let req_id = state().next_request_id.fetch_add(1, Ordering::Relaxed);
    state().save_waiters.lock().insert(req_id, tx);
    let name = suggested_name.unwrap_or("document.docx");
    if let Ok(c) = CString::new(name) {
        unsafe { ibis_ios_pick_path_for_save(req_id, c.as_ptr()) };
    }
    rx
}

pub fn sync_saved_file(path: &std::path::Path) -> bool {
    let Some(path_str) = path.to_str() else {
        return false;
    };
    let Ok(c) = CString::new(path_str) else {
        return false;
    };
    unsafe { ibis_ios_sync_saved_file(c.as_ptr()) }
}

pub fn drain_ime_events() -> Vec<ImeBridgeEvent> {
    state().ime_queue.lock().drain(..).collect()
}

pub fn drain_open_paths() -> Vec<String> {
    state().open_paths.lock().drain(..).collect()
}

#[unsafe(no_mangle)]
pub extern "C" fn ibis_ios_on_file_picked(request_id: i64, path: *const c_char) {
    let picked = if path.is_null() {
        None
    } else {
        let s = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
        Some(vec![PathBuf::from(s)])
    };
    if let Some(tx) = state().open_waiters.lock().remove(&request_id) {
        let _ = tx.send(Ok(picked));
    }
    wake_main();
}

#[unsafe(no_mangle)]
pub extern "C" fn ibis_ios_on_save_path_picked(request_id: i64, path: *const c_char) {
    let picked = if path.is_null() {
        None
    } else {
        let s = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
        Some(PathBuf::from(s))
    };
    if let Some(tx) = state().save_waiters.lock().remove(&request_id) {
        let _ = tx.send(Ok(picked));
    }
    wake_main();
}

#[unsafe(no_mangle)]
pub extern "C" fn ibis_ios_on_open_path(path: *const c_char) {
    if path.is_null() {
        return;
    }
    let s = unsafe { CStr::from_ptr(path) }.to_string_lossy().into_owned();
    state().open_paths.lock().push_back(format!("file://{s}"));
    wake_main();
}

#[unsafe(no_mangle)]
pub extern "C" fn ibis_ios_ime_commit(text: *const c_char) {
    let s = if text.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
    };
    state().ime_queue.lock().push_back(ImeBridgeEvent::Commit(s));
    wake_main();
}

#[unsafe(no_mangle)]
pub extern "C" fn ibis_ios_ime_composing(text: *const c_char) {
    let s = if text.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
    };
    state()
        .ime_queue
        .lock()
        .push_back(ImeBridgeEvent::Composing(s));
    wake_main();
}

#[unsafe(no_mangle)]
pub extern "C" fn ibis_ios_ime_backspace() {
    state().ime_queue.lock().push_back(ImeBridgeEvent::Backspace);
    wake_main();
}

#[unsafe(no_mangle)]
pub extern "C" fn ibis_ios_ime_enter() {
    state().ime_queue.lock().push_back(ImeBridgeEvent::Enter);
    wake_main();
}

pub fn make_non_null_view(ptr: *mut c_void) -> Option<std::ptr::NonNull<c_void>> {
    std::ptr::NonNull::new(ptr)
}
