// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/247)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use android_activity::{AndroidApp, AndroidAppWaker};
use futures::channel::oneshot;
use jni::JNIEnv;
use jni::JavaVM;
use jni::NativeMethod;
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
use jni::sys::jlong;
use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, OnceLock};

#[derive(Debug)]
pub enum ImeBridgeEvent {
    Commit(String),
    Composing(String),
    Backspace,
    Enter,
    /// The keyboard finished the composing text (`finishComposingText`): it stays, as the text, and stops being composed (not emptied).
    FinishComposing,
}

/// The soft keyboard now (K0064): how much of the window's bottom it covers (logical pixels, dp), and how long it takes to move (seconds).
/// On Android it is always across the window's bottom (`Docked`); a floating one is not told (it covers no edge, so it is shown no toolbar).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardKind {
    Docked,
    DockedWithHideKey,
    Floating,
    Hardware,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardInfo {
    pub height: f32,
    pub kind: KeyboardKind,
    pub duration: f32,
}

impl KeyboardInfo {
    pub const HIDDEN: KeyboardInfo = KeyboardInfo { height: 0.0, kind: KeyboardKind::Docked, duration: 0.0 };
}

/// The soft keyboard as the activity last told (`WindowInsets`' ime in `IbisActivity.java`).
/// The window's safe area (left, top, right, bottom, dp): how far the app keeps its content in from each edge of the window (the system bars, the display cutout and the
/// gesture grip: `shell::safe_area::compute`). The window's surface is the whole screen (the activity draws to it directly, so a margin on its view does nothing),
/// and the app lays itself out inside this.
static SAFE_AREA_DP: Mutex<[f32; 4]> = Mutex::new([0.0; 4]);

pub fn safe_area() -> [f32; 4] {
    *SAFE_AREA_DP.lock()
}

pub fn keyboard() -> KeyboardInfo {
    BRIDGE.get().map_or(KeyboardInfo::HIDDEN, |state| *state.keyboard.lock())
}

struct BridgeState {
    keyboard: Mutex<KeyboardInfo>,
    vm: JavaVM,
    activity: GlobalRef,
    waker: AndroidAppWaker,
    next_request_id: AtomicI64,
    open_waiters: Mutex<HashMap<i64, oneshot::Sender<anyhow::Result<Option<Vec<PathBuf>>>>>>,
    save_waiters: Mutex<HashMap<i64, oneshot::Sender<anyhow::Result<Option<PathBuf>>>>>,
    open_paths: Mutex<VecDeque<String>>,
    ime_queue: Mutex<VecDeque<ImeBridgeEvent>>,
}

static BRIDGE: OnceLock<Arc<BridgeState>> = OnceLock::new();

pub fn init(app: &AndroidApp) {
    if BRIDGE.get().is_some() {
        return;
    }
    let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr().cast()) }
        .expect("Failed to wrap Android JavaVM");
    let activity_ref = {
        let mut env = vm.attach_current_thread().expect("attach_current_thread");
        let obj = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
        if let Ok(class) = env.get_object_class(&obj) {
            let methods = [
                NativeMethod {
                    name: "nativeOnFilePicked".into(),
                    sig: "(JLjava/lang/String;)V".into(),
                    fn_ptr: native_on_file_picked as *mut std::ffi::c_void,
                },
                NativeMethod {
                    name: "nativeOnSavePathPicked".into(),
                    sig: "(JLjava/lang/String;)V".into(),
                    fn_ptr: native_on_save_path_picked as *mut std::ffi::c_void,
                },
                NativeMethod {
                    name: "nativeOnOpenPath".into(),
                    sig: "(Ljava/lang/String;)V".into(),
                    fn_ptr: native_on_open_path as *mut std::ffi::c_void,
                },
                NativeMethod {
                    name: "nativeOnImeCommit".into(),
                    sig: "(Ljava/lang/String;)V".into(),
                    fn_ptr: native_on_ime_commit as *mut std::ffi::c_void,
                },
                NativeMethod {
                    name: "nativeOnImeComposing".into(),
                    sig: "(Ljava/lang/String;)V".into(),
                    fn_ptr: native_on_ime_composing as *mut std::ffi::c_void,
                },
                NativeMethod {
                    name: "nativeOnImeFinishComposing".into(),
                    sig: "()V".into(),
                    fn_ptr: native_on_ime_finish_composing as *mut std::ffi::c_void,
                },
                NativeMethod {
                    name: "nativeSafeArea".into(),
                    sig: "([IF)[F".into(),
                    fn_ptr: native_safe_area as *mut std::ffi::c_void,
                },
                NativeMethod {
                    name: "nativeOnImeBackspace".into(),
                    sig: "()V".into(),
                    fn_ptr: native_on_ime_backspace as *mut std::ffi::c_void,
                },
                NativeMethod {
                    name: "nativeOnKeyboardChanged".into(),
                    sig: "(FIF)V".into(),
                    fn_ptr: native_on_keyboard_changed as *mut std::ffi::c_void,
                },
                NativeMethod {
                    name: "nativeOnImeEnter".into(),
                    sig: "()V".into(),
                    fn_ptr: native_on_ime_enter as *mut std::ffi::c_void,
                },
            ];
            if let Err(err) = env.register_native_methods(&class, &methods) {
                log::error!("Failed to register native JNI methods: {err}");
            }
        }
        let global = env.new_global_ref(&obj).expect("new_global_ref");
        std::mem::forget(obj);
        global
    };
    let state = Arc::new(BridgeState {
        keyboard: Mutex::new(KeyboardInfo::HIDDEN),
        vm,
        activity: activity_ref,
        waker: app.create_waker(),
        next_request_id: AtomicI64::new(1),
        open_waiters: Mutex::new(HashMap::new()),
        save_waiters: Mutex::new(HashMap::new()),
        open_paths: Mutex::new(VecDeque::new()),
        ime_queue: Mutex::new(VecDeque::new()),
    });
    let _ = BRIDGE.set(state);
}

fn with_env<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> jni::errors::Result<R>,
{
    let state = BRIDGE.get()?;
    let mut env = state.vm.attach_current_thread().ok()?;
    match f(&mut env, state.activity.as_obj()) {
        Ok(val) => Some(val),
        Err(err) => {
            log::warn!("JNI call failed: {err}");
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            None
        }
    }
}

pub fn show_keyboard() {
    with_env(|env, activity| {
        env.call_method(activity, "showKeyboard", "()V", &[])?;
        Ok(())
    });
}

pub fn hide_keyboard() {
    with_env(|env, activity| {
        env.call_method(activity, "hideKeyboard", "()V", &[])?;
        Ok(())
    });
}

pub fn open_url(url: &str) {
    with_env(|env, activity| {
        let jurl = env.new_string(url)?;
        env.call_method(
            activity,
            "openUrl",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&jurl)],
        )?;
        Ok(())
    });
}

pub fn get_clipboard_text() -> Option<String> {
    with_env(|env, activity| {
        let res = env
            .call_method(activity, "getClipboardText", "()Ljava/lang/String;", &[])?
            .l()?;
        if res.is_null() {
            return Ok(None);
        }
        let jstr = JString::from(res);
        let text: String = env.get_string(&jstr)?.into();
        Ok(Some(text))
    })
    .flatten()
}

pub fn set_clipboard_text(text: &str) {
    with_env(|env, activity| {
        let jtext = env.new_string(text)?;
        env.call_method(
            activity,
            "setClipboardText",
            "(Ljava/lang/String;)V",
            &[JValue::Object(&jtext)],
        )?;
        Ok(())
    });
}

pub fn pick_file_for_open() -> oneshot::Receiver<anyhow::Result<Option<Vec<PathBuf>>>> {
    let (tx, rx) = oneshot::channel();
    let Some(state) = BRIDGE.get() else {
        let _ = tx.send(Ok(None));
        return rx;
    };
    let req_id = state.next_request_id.fetch_add(1, Ordering::Relaxed);
    state.open_waiters.lock().insert(req_id, tx);
    with_env(|env, activity| {
        env.call_method(activity, "pickFileForOpen", "(J)V", &[JValue::Long(req_id)])?;
        Ok(())
    });
    rx
}

pub fn pick_path_for_save(
    suggested_name: Option<&str>,
) -> oneshot::Receiver<anyhow::Result<Option<PathBuf>>> {
    let (tx, rx) = oneshot::channel();
    let Some(state) = BRIDGE.get() else {
        let _ = tx.send(Ok(None));
        return rx;
    };
    let req_id = state.next_request_id.fetch_add(1, Ordering::Relaxed);
    state.save_waiters.lock().insert(req_id, tx);
    let name = suggested_name.unwrap_or("document.docx");
    with_env(|env, activity| {
        let jname = env.new_string(name)?;
        env.call_method(
            activity,
            "pickPathForSave",
            "(JLjava/lang/String;)V",
            &[JValue::Long(req_id), JValue::Object(&jname)],
        )?;
        Ok(())
    });
    rx
}

pub fn sync_saved_file(path: &std::path::Path) -> bool {
    let Some(path_str) = path.to_str() else {
        return false;
    };
    with_env(|env, activity| {
        let jpath = env.new_string(path_str)?;
        let res = env
            .call_method(
                activity,
                "syncSavedFile",
                "(Ljava/lang/String;)Z",
                &[JValue::Object(&jpath)],
            )?
            .z()?;
        Ok(res)
    })
    .unwrap_or(false)
}

pub fn drain_ime_events() -> Vec<ImeBridgeEvent> {
    let Some(state) = BRIDGE.get() else {
        return Vec::new();
    };
    let mut queue = state.ime_queue.lock();
    queue.drain(..).collect()
}

pub fn drain_open_paths() -> Vec<String> {
    let Some(state) = BRIDGE.get() else {
        return Vec::new();
    };
    let mut queue = state.open_paths.lock();
    queue.drain(..).collect()
}

fn jstring_to_option(env: &mut JNIEnv<'_>, s: JString<'_>) -> Option<String> {
    if s.is_null() {
        return None;
    }
    env.get_string(&s).ok().map(Into::into)
}

extern "system" fn native_on_file_picked(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    request_id: jlong,
    path: JString<'_>,
) {
    let picked = jstring_to_option(&mut env, path).map(|p| vec![PathBuf::from(p)]);
    if let Some(state) = BRIDGE.get() {
        if let Some(tx) = state.open_waiters.lock().remove(&request_id) {
            let _ = tx.send(Ok(picked));
        }
        state.waker.wake();
    }
}

extern "system" fn native_on_save_path_picked(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    request_id: jlong,
    path: JString<'_>,
) {
    let picked = jstring_to_option(&mut env, path).map(PathBuf::from);
    if let Some(state) = BRIDGE.get() {
        if let Some(tx) = state.save_waiters.lock().remove(&request_id) {
            let _ = tx.send(Ok(picked));
        }
        state.waker.wake();
    }
}

extern "system" fn native_on_open_path(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    path: JString<'_>,
) {
    if let Some(p) = jstring_to_option(&mut env, path) {
        if let Some(state) = BRIDGE.get() {
            state.open_paths.lock().push_back(format!("file://{p}"));
            state.waker.wake();
        }
    }
}

extern "system" fn native_on_ime_commit(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    text: JString<'_>,
) {
    let text = jstring_to_option(&mut env, text).unwrap_or_default();
    if let Some(state) = BRIDGE.get() {
        state.ime_queue.lock().push_back(ImeBridgeEvent::Commit(text));
        state.waker.wake();
    }
}

extern "system" fn native_on_ime_composing(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    text: JString<'_>,
) {
    let text = jstring_to_option(&mut env, text).unwrap_or_default();
    if let Some(state) = BRIDGE.get() {
        state
            .ime_queue
            .lock()
            .push_back(ImeBridgeEvent::Composing(text));
        state.waker.wake();
    }
}

extern "system" fn native_on_ime_backspace(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(state) = BRIDGE.get() {
        state.ime_queue.lock().push_back(ImeBridgeEvent::Backspace);
        state.waker.wake();
    }
}

extern "system" fn native_on_ime_finish_composing(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(state) = BRIDGE.get() {
        state.ime_queue.lock().push_back(ImeBridgeEvent::FinishComposing);
        state.waker.wake();
    }
}

/// The app's way to work out the safe area (`ibis_office_shell::safe_area::compute`): the raw insets (13 pixels) and the density in, the margins (left, top, right, bottom, pixels)
/// and the keyboard's height (dp) out. Set once at the start (`set_safe_area_fn`).
pub type SafeAreaFn = fn(&[i32; 13], f32) -> [f32; 5];
static SAFE_AREA: OnceLock<SafeAreaFn> = OnceLock::new();

pub fn set_safe_area_fn(f: SafeAreaFn) {
    let _ = SAFE_AREA.set(f);
}

extern "system" fn native_safe_area<'a>(mut env: JNIEnv<'a>, _class: JClass<'a>, raw: jni::objects::JIntArray<'a>, density: f32) -> jni::sys::jfloatArray {
    let mut buffer = [0i32; 13];
    if !matches!(env.get_array_length(&raw), Ok(13)) || env.get_int_array_region(&raw, 0, &mut buffer).is_err() {
        return std::ptr::null_mut();
    }
    let Some(compute) = SAFE_AREA.get() else { return std::ptr::null_mut() };
    let out = compute(&buffer, density);
    let density = if density > 0.0 { density } else { 1.0 };
    *SAFE_AREA_DP.lock() = [out[0] / density, out[1] / density, out[2] / density, out[3] / density];
    if let Some(state) = BRIDGE.get() {
        state.waker.wake();
    }
    match env.new_float_array(5) {
        Ok(array) => {
            if env.set_float_array_region(&array, 0, &out).is_err() {
                return std::ptr::null_mut();
            }
            array.into_raw()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

extern "system" fn native_on_ime_enter(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(state) = BRIDGE.get() {
        state.ime_queue.lock().push_back(ImeBridgeEvent::Enter);
        state.waker.wake();
    }
}

extern "system" fn native_on_keyboard_changed(_env: JNIEnv<'_>, _class: JClass<'_>, height_dp: f32, kind: i32, duration: f32) {
    if let Some(state) = BRIDGE.get() {
        let kind = match kind {
            1 => KeyboardKind::Floating,
            2 => KeyboardKind::Hardware,
            3 => KeyboardKind::DockedWithHideKey,
            _ => KeyboardKind::Docked,
        };
        *state.keyboard.lock() = KeyboardInfo { height: height_dp.max(0.0), kind, duration: duration.max(0.0) };
        state.waker.wake();
    }
}
