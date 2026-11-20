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
}

struct BridgeState {
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
                    name: "nativeOnImeBackspace".into(),
                    sig: "()V".into(),
                    fn_ptr: native_on_ime_backspace as *mut std::ffi::c_void,
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

extern "system" fn native_on_ime_enter(_env: JNIEnv<'_>, _class: JClass<'_>) {
    if let Some(state) = BRIDGE.get() {
        state.ime_queue.lock().push_back(ImeBridgeEvent::Enter);
        state.waker.wake();
    }
}
