// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/247)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use crate::dispatcher::AndroidDispatcher;
use crate::display::{AndroidDisplay, AndroidDisplayState};
use crate::jni_bridge::{self, ImeBridgeEvent};
use crate::window::{AndroidWindow, AndroidWindowInner};
use android_activity::input::{InputEvent, KeyAction, KeyEvent, Keycode, MotionAction};
use android_activity::{AndroidApp, InputStatus, MainEvent, PollEvent};
use anyhow::Result;
use futures::channel::oneshot;
use gpui::{
    Action, AnyWindowHandle, AppLifecyclePhase, BackgroundExecutor, ClipboardItem, CursorStyle,
    DummyKeyboardMapper, ForegroundExecutor, KeyDownEvent, KeyUpEvent, Keymap, Keystroke, Menu,
    MenuItem, Modifiers, PathPromptOptions, PinchEvent, Pixels, Platform, PlatformDisplay, PlatformInput,
    PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Point,
    Task, ThermalState, TouchEvent, TouchId, TouchPhase, WindowAppearance,
    WindowParams, WindowVisibility, point, px,
};
use gpui_wgpu::{CosmicTextSystem, GpuContext};
use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const TAP_SLOP_PX: f32 = 8.0;

/// What the platform keeps of the touches, to know a tap (for the soft keyboard).
#[derive(Debug, Clone, Copy, Default)]
struct TapState {
    /// Fingers on the screen now.
    active: u32,
    start: (f32, f32),
    /// The only finger has not moved out of the slop since it went down.
    candidate: bool,
}

#[derive(Debug, Clone, Copy)]
enum TouchState {
    Idle,
    Single {
        pointer_id: i32,
        last: (f32, f32),
    },
    Pinch {
        p1_id: i32,
        p1: (f32, f32),
        p2_id: i32,
        p2: (f32, f32),
        last_dist: f32,
    },
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn midpoint(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5)
}

struct AndroidKeyboardLayout;

impl PlatformKeyboardLayout for AndroidKeyboardLayout {
    fn id(&self) -> &str {
        "us"
    }

    fn name(&self) -> &str {
        "US"
    }
}

#[derive(Default)]
struct PlatformCallbacks {
    quit: Option<Box<dyn FnMut() -> bool>>,
    reopen: Option<Box<dyn FnMut()>>,
    app_lifecycle: Option<Box<dyn FnMut(AppLifecyclePhase)>>,
    memory_warning: Option<Box<dyn FnMut()>>,
    open_urls: Option<Box<dyn FnMut(Vec<String>)>>,
}

pub struct AndroidPlatform {
    app: AndroidApp,
    dispatcher: Arc<AndroidDispatcher>,
    text_system: Arc<CosmicTextSystem>,
    gpu_context: GpuContext,
    display_state: Arc<Mutex<AndroidDisplayState>>,
    window: RefCell<Option<Rc<AndroidWindowInner>>>,
    callbacks: RefCell<PlatformCallbacks>,
    quit_requested: Cell<bool>,
    touch_state: Cell<TouchState>,
    tap: Cell<TapState>,
    /// A tap ended: whether the soft keyboard shows is decided on the next turn.
    keyboard_check: Cell<bool>,
    /// The soft keyboard the window was last laid out with (K0064).
    keyboard_seen: Cell<jni_bridge::KeyboardInfo>,
    safe_area_seen: Cell<[f32; 4]>,
}

impl AndroidPlatform {
    pub fn new(app: AndroidApp) -> Self {
        jni_bridge::init(&app);
        let waker = app.create_waker();
        let dispatcher = Arc::new(AndroidDispatcher::new(waker));
        let text_system = Arc::new(CosmicTextSystem::new("sans-serif"));
        let density = app.config().density().unwrap_or(320) as f32;
        let scale_factor = (density / 160.0).max(1.0);
        let display_state = Arc::new(Mutex::new(AndroidDisplayState {
            physical_width: 1080,
            physical_height: 1920,
            scale_factor,
        }));

        Self {
            app,
            dispatcher,
            text_system,
            gpu_context: Rc::new(RefCell::new(None)),
            display_state,
            window: RefCell::new(None),
            callbacks: RefCell::new(PlatformCallbacks::default()),
            quit_requested: Cell::new(false),
            touch_state: Cell::new(TouchState::Idle),
            tap: Cell::new(TapState::default()),
            keyboard_check: Cell::new(false),
            keyboard_seen: Cell::new(jni_bridge::KeyboardInfo::HIDDEN),
            safe_area_seen: Cell::new([0.0; 4]),
        }
    }

    fn current_scale_factor(&self) -> f32 {
        let density = self.app.config().density().unwrap_or(320) as f32;
        (density / 160.0).max(1.0)
    }

    fn update_display_from_window(&self) {
        if let Some(win) = self.app.native_window() {
            let w = (win.width().max(1)) as u32;
            let h = (win.height().max(1)) as u32;
            let scale = self.current_scale_factor();
            if let Some(window) = self.window.borrow().as_ref() {
                window.notify_resize(w, h, scale);
            } else {
                let mut state = self.display_state.lock();
                state.physical_width = w;
                state.physical_height = h;
                state.scale_factor = scale;
            }
        }
    }

    /// The soft keyboard moved: the window is laid out again (the shell reads `gpui_android::keyboard()` as it lays out; K0064).
    fn handle_keyboard(&self) {
        let now = jni_bridge::keyboard();
        let area = jni_bridge::safe_area();
        if now == self.keyboard_seen.get() && area == self.safe_area_seen.get() {
            return;
        }
        self.keyboard_seen.set(now);
        self.safe_area_seen.set(area);
        self.update_display_from_window();
    }

    /// One touch of the screen for gpui's touch recognizer (K0065).
    fn touch(window: &AndroidWindowInner, id: i32, phase: TouchPhase, at: (f32, f32)) {
        window.dispatch_input(PlatformInput::Touch(TouchEvent {
            id: TouchId(id as u64),
            phase,
            position: point(px(at.0), px(at.1)),
            predicted_position: None,
            force: None,
        }));
    }

    fn send_key(&self, window: &AndroidWindowInner, key: &str) {
        if let Ok(keystroke) = Keystroke::parse(key) {
            window.dispatch_input(PlatformInput::KeyDown(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }));
            window.dispatch_input(PlatformInput::KeyUp(KeyUpEvent { keystroke }));
            window.request_frame();
        }
    }

    fn handle_ime_events(&self) {
        let events = jni_bridge::drain_ime_events();
        if events.is_empty() {
            return;
        }
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        for event in events {
            match event {
                ImeBridgeEvent::Commit(text) => {
                    // The keyboard's Enter may come as text: "\n", or the letters and then "\n" ("あ\n"). The letters are committed and the Enter is the key
                    // (a cell's Enter commits it and goes down; a document's is a new paragraph), whichever way the keyboard sent it.
                    let letters = text.trim_end_matches(['\n', '\r']);
                    let enter = letters.len() < text.len() && !letters.contains(['\n', '\r']);
                    let commit = if enter { letters } else { text.as_str() };
                    if !commit.is_empty() {
                        window.with_input_handler(|handler| {
                            handler.replace_text_in_range(None, commit);
                        });
                    }
                    if enter {
                        log::info!("ibis-android: ime enter via commitText({:?})", text);
                        self.send_key(&window, "enter");
                    }
                    window.request_frame();
                }
                ImeBridgeEvent::FinishComposing => {
                    // (The marked text stays and is no longer marked, as the OS's unmarkText does; before, it was emptied as if the keyboard had composed "".)
                    window.with_input_handler(|handler| {
                        handler.unmark_text();
                    });
                    window.request_frame();
                }
                ImeBridgeEvent::Composing(text) => {
                    window.with_input_handler(|handler| {
                        handler.replace_and_mark_text_in_range(None, &text, None);
                    });
                    window.request_frame();
                }
                ImeBridgeEvent::Backspace => self.send_key(&window, "backspace"),
                ImeBridgeEvent::Enter => {
                    log::info!("ibis-android: ime enter via the Enter key event");
                    self.send_key(&window, "enter")
                }
            }
        }
    }

    fn keycode_to_gpui(code: Keycode, shift: bool) -> Option<(&'static str, Option<&'static str>)> {
        Some(match code {
            Keycode::A => ("a", Some(if shift { "A" } else { "a" })),
            Keycode::B => ("b", Some(if shift { "B" } else { "b" })),
            Keycode::C => ("c", Some(if shift { "C" } else { "c" })),
            Keycode::D => ("d", Some(if shift { "D" } else { "d" })),
            Keycode::E => ("e", Some(if shift { "E" } else { "e" })),
            Keycode::F => ("f", Some(if shift { "F" } else { "f" })),
            Keycode::G => ("g", Some(if shift { "G" } else { "g" })),
            Keycode::H => ("h", Some(if shift { "H" } else { "h" })),
            Keycode::I => ("i", Some(if shift { "I" } else { "i" })),
            Keycode::J => ("j", Some(if shift { "J" } else { "j" })),
            Keycode::K => ("k", Some(if shift { "K" } else { "k" })),
            Keycode::L => ("l", Some(if shift { "L" } else { "l" })),
            Keycode::M => ("m", Some(if shift { "M" } else { "m" })),
            Keycode::N => ("n", Some(if shift { "N" } else { "n" })),
            Keycode::O => ("o", Some(if shift { "O" } else { "o" })),
            Keycode::P => ("p", Some(if shift { "P" } else { "p" })),
            Keycode::Q => ("q", Some(if shift { "Q" } else { "q" })),
            Keycode::R => ("r", Some(if shift { "R" } else { "r" })),
            Keycode::S => ("s", Some(if shift { "S" } else { "s" })),
            Keycode::T => ("t", Some(if shift { "T" } else { "t" })),
            Keycode::U => ("u", Some(if shift { "U" } else { "u" })),
            Keycode::V => ("v", Some(if shift { "V" } else { "v" })),
            Keycode::W => ("w", Some(if shift { "W" } else { "w" })),
            Keycode::X => ("x", Some(if shift { "X" } else { "x" })),
            Keycode::Y => ("y", Some(if shift { "Y" } else { "y" })),
            Keycode::Z => ("z", Some(if shift { "Z" } else { "z" })),
            Keycode::Keycode0 => ("0", Some(if shift { ")" } else { "0" })),
            Keycode::Keycode1 => ("1", Some(if shift { "!" } else { "1" })),
            Keycode::Keycode2 => ("2", Some(if shift { "@" } else { "2" })),
            Keycode::Keycode3 => ("3", Some(if shift { "#" } else { "3" })),
            Keycode::Keycode4 => ("4", Some(if shift { "$" } else { "4" })),
            Keycode::Keycode5 => ("5", Some(if shift { "%" } else { "5" })),
            Keycode::Keycode6 => ("6", Some(if shift { "^" } else { "6" })),
            Keycode::Keycode7 => ("7", Some(if shift { "&" } else { "7" })),
            Keycode::Keycode8 => ("8", Some(if shift { "*" } else { "8" })),
            Keycode::Keycode9 => ("9", Some(if shift { "(" } else { "9" })),
            Keycode::Space => ("space", Some(" ")),
            Keycode::Enter | Keycode::NumpadEnter => ("enter", None),
            Keycode::Del => ("backspace", None),
            Keycode::ForwardDel => ("delete", None),
            Keycode::Tab => ("tab", None),
            Keycode::Escape => ("escape", None),
            Keycode::DpadUp => ("up", None),
            Keycode::DpadDown => ("down", None),
            Keycode::DpadLeft => ("left", None),
            Keycode::DpadRight => ("right", None),
            Keycode::MoveHome => ("home", None),
            Keycode::MoveEnd => ("end", None),
            Keycode::PageUp => ("pageup", None),
            Keycode::PageDown => ("pagedown", None),
            Keycode::Comma => (",", Some(if shift { "<" } else { "," })),
            Keycode::Period => (".", Some(if shift { ">" } else { "." })),
            Keycode::Minus => ("-", Some(if shift { "_" } else { "-" })),
            Keycode::Equals => ("=", Some(if shift { "+" } else { "=" })),
            Keycode::Slash => ("/", Some(if shift { "?" } else { "/" })),
            Keycode::Semicolon => (";", Some(if shift { ":" } else { ";" })),
            Keycode::Apostrophe => ("'", Some(if shift { "\"" } else { "'" })),
            Keycode::LeftBracket => ("[", Some(if shift { "{" } else { "[" })),
            Keycode::RightBracket => ("]", Some(if shift { "}" } else { "]" })),
            Keycode::Backslash => ("\\", Some(if shift { "|" } else { "\\" })),
            Keycode::Grave => ("`", Some(if shift { "~" } else { "`" })),
            _ => return None,
        })
    }

    fn handle_hardware_key(&self, window: &AndroidWindowInner, key_event: &KeyEvent) -> InputStatus {
        let code = key_event.key_code();
        if key_event.action() == KeyAction::Down && code == Keycode::Back {
            jni_bridge::hide_keyboard();
            return InputStatus::Handled;
        }
        let meta = key_event.meta_state();
        let modifiers = Modifiers {
            control: meta.ctrl_on(),
            alt: meta.alt_on(),
            shift: meta.shift_on(),
            platform: false,
            function: false,
        };
        window.modifiers.set(modifiers);
        let Some((key, key_char)) = Self::keycode_to_gpui(code, modifiers.shift) else {
            return InputStatus::Unhandled;
        };
        let key_char = if modifiers.control || modifiers.alt {
            None
        } else {
            key_char.map(str::to_string)
        };
        let keystroke = Keystroke {
            modifiers,
            key: key.to_string(),
            key_char,
        };
        match key_event.action() {
            KeyAction::Down => {
                window.dispatch_input(PlatformInput::KeyDown(KeyDownEvent {
                    keystroke,
                    is_held: key_event.repeat_count() > 0,
                    prefer_character_input: false,
                }));
                window.request_frame();
                InputStatus::Handled
            }
            KeyAction::Up => {
                window.dispatch_input(PlatformInput::KeyUp(KeyUpEvent { keystroke }));
                window.request_frame();
                InputStatus::Handled
            }
            _ => InputStatus::Unhandled,
        }
    }

    fn handle_open_paths(&self) {
        let paths = jni_bridge::drain_open_paths();
        if paths.is_empty() {
            return;
        }
        let mut cb = self.callbacks.borrow_mut().open_urls.take();
        if let Some(ref mut callback) = cb {
            callback(paths);
        }
        let mut slot = self.callbacks.borrow_mut();
        if slot.open_urls.is_none() {
            slot.open_urls = cb;
        }
    }

    fn handle_input_events(&self) {
        let Ok(mut iter) = self.app.input_events_iter() else {
            return;
        };
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        let scale = self.current_scale_factor();

        while iter.next(|event| match event {
            InputEvent::KeyEvent(key_event) => self.handle_hardware_key(&window, key_event),
            InputEvent::MotionEvent(motion) => {
                // Every finger goes to gpui's touch recognizer as a touch of its own (K0065): one finger taps, drags and scrolls, two fingers scroll
                // together, as on every platform. The platform keeps the pinch (the two fingers' distance) and whether the touch was a tap, for the
                // soft keyboard.
                let action_idx = motion.pointer_index();
                let active_ptr = motion.pointer_at_index(action_idx);
                let ptr_id = active_ptr.pointer_id();
                let pos = (active_ptr.x() / scale, active_ptr.y() / scale);

                match motion.action() {
                    MotionAction::Down => {
                        self.tap.set(TapState { active: 1, start: pos, candidate: true });
                        self.touch_state.set(TouchState::Single { pointer_id: ptr_id, last: pos });
                        Self::touch(&window, ptr_id, TouchPhase::Started, pos);
                        InputStatus::Handled
                    }
                    MotionAction::PointerDown => {
                        let mut tap = self.tap.get();
                        tap.active += 1;
                        tap.candidate = false;
                        self.tap.set(tap);
                        Self::touch(&window, ptr_id, TouchPhase::Started, pos);
                        if let TouchState::Single { pointer_id: p1_id, last: p1 } = self.touch_state.get()
                            && ptr_id != p1_id
                        {
                            let dist = distance(p1, pos).max(1.0);
                            let mid = midpoint(p1, pos);
                            self.touch_state.set(TouchState::Pinch { p1_id, p1, p2_id: ptr_id, p2: pos, last_dist: dist });
                            window.dispatch_input(PlatformInput::Pinch(PinchEvent {
                                position: point(px(mid.0), px(mid.1)),
                                delta: 0.0,
                                modifiers: Modifiers::default(),
                                phase: TouchPhase::Started,
                            }));
                        }
                        window.request_frame();
                        InputStatus::Handled
                    }
                    MotionAction::Move => {
                        let mut tap = self.tap.get();
                        let mut state = self.touch_state.get();
                        for p in motion.pointers() {
                            let id = p.pointer_id();
                            let pt = (p.x() / scale, p.y() / scale);
                            Self::touch(&window, id, TouchPhase::Moved, pt);
                            match &mut state {
                                TouchState::Single { pointer_id, last } if *pointer_id == id => {
                                    *last = pt;
                                    if tap.candidate && distance(tap.start, pt) >= TAP_SLOP_PX {
                                        tap.candidate = false;
                                    }
                                }
                                TouchState::Pinch { p1_id, p1, p2_id, p2, .. } => {
                                    if id == *p1_id {
                                        *p1 = pt;
                                    } else if id == *p2_id {
                                        *p2 = pt;
                                    }
                                }
                                _ => {}
                            }
                        }
                        self.tap.set(tap);
                        if let TouchState::Pinch { p1, p2, last_dist, p1_id, p2_id } = state {
                            let new_dist = distance(p1, p2).max(1.0);
                            let delta = (new_dist / last_dist.max(1.0)) - 1.0;
                            let mid = midpoint(p1, p2);
                            state = TouchState::Pinch { p1_id, p1, p2_id, p2, last_dist: new_dist };
                            window.dispatch_input(PlatformInput::Pinch(PinchEvent {
                                position: point(px(mid.0), px(mid.1)),
                                delta,
                                modifiers: Modifiers::default(),
                                phase: TouchPhase::Moved,
                            }));
                        }
                        self.touch_state.set(state);
                        window.request_frame();
                        InputStatus::Handled
                    }
                    MotionAction::PointerUp => {
                        Self::touch(&window, ptr_id, TouchPhase::Ended, pos);
                        let mut tap = self.tap.get();
                        tap.active = tap.active.saturating_sub(1);
                        tap.candidate = false;
                        self.tap.set(tap);
                        if let TouchState::Pinch { p1, p2, .. } = self.touch_state.get() {
                            let mid = midpoint(p1, p2);
                            window.dispatch_input(PlatformInput::Pinch(PinchEvent {
                                position: point(px(mid.0), px(mid.1)),
                                delta: 0.0,
                                modifiers: Modifiers::default(),
                                phase: TouchPhase::Ended,
                            }));
                            self.touch_state.set(TouchState::Idle);
                        }
                        window.request_frame();
                        InputStatus::Handled
                    }
                    MotionAction::Up | MotionAction::Cancel => {
                        let cancelled = motion.action() == MotionAction::Cancel;
                        if cancelled {
                            for p in motion.pointers() {
                                Self::touch(&window, p.pointer_id(), TouchPhase::Cancelled, (p.x() / scale, p.y() / scale));
                            }
                        } else {
                            Self::touch(&window, ptr_id, TouchPhase::Ended, pos);
                        }
                        let tap = self.tap.get();
                        if !cancelled && tap.candidate {
                            // A tap: whether the soft keyboard shows is decided on the next turn (the page has handled the tap by then).
                            self.keyboard_check.set(true);
                        }
                        self.tap.set(TapState::default());
                        if let TouchState::Pinch { p1, p2, .. } = self.touch_state.get() {
                            let mid = midpoint(p1, p2);
                            window.dispatch_input(PlatformInput::Pinch(PinchEvent {
                                position: point(px(mid.0), px(mid.1)),
                                delta: 0.0,
                                modifiers: Modifiers::default(),
                                phase: TouchPhase::Ended,
                            }));
                        }
                        self.touch_state.set(TouchState::Idle);
                        window.request_frame();
                        InputStatus::Handled
                    }
                    _ => InputStatus::Unhandled,
                }
            }
            _ => InputStatus::Unhandled,
        }) {}
    }
}

impl Platform for AndroidPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        BackgroundExecutor::new(Arc::clone(&self.dispatcher) as Arc<dyn gpui::PlatformDispatcher>)
    }

    fn foreground_executor(&self) -> ForegroundExecutor {
        ForegroundExecutor::new(Arc::clone(&self.dispatcher) as Arc<dyn gpui::PlatformDispatcher>)
    }

    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        Arc::clone(&self.text_system) as Arc<dyn PlatformTextSystem>
    }

    fn run(&self, on_finish_launching: Box<dyn 'static + FnOnce()>) {
        let mut launch_cb = Some(on_finish_launching);

        while !self.quit_requested.get() {
            let needs_immediate = self.dispatcher.has_pending_main_tasks()
                || self
                    .window
                    .borrow()
                    .as_ref()
                    .is_some_and(|w| *w.frame_requested.lock());

            let timeout = if needs_immediate {
                Some(Duration::ZERO)
            } else {
                Some(Duration::from_millis(50))
            };

            let mut should_break = false;
            self.app.poll_events(timeout, |event| match event {
                PollEvent::Main(main_event) => match main_event {
                    MainEvent::InitWindow { .. } => {
                        self.update_display_from_window();
                        if let Some(cb) = launch_cb.take() {
                            cb();
                        } else if let Some(win) = self.app.native_window() {
                            if let Some(window) = self.window.borrow().as_ref() {
                                window.set_native_window(Some(win));
                            }
                        }
                    }
                    MainEvent::WindowResized { .. } | MainEvent::ContentRectChanged { .. } => {
                        self.update_display_from_window();
                    }
                    MainEvent::TerminateWindow { .. } => {
                        if let Some(window) = self.window.borrow().as_ref() {
                            window.set_native_window(None);
                        }
                    }
                    MainEvent::GainedFocus => {
                        if let Some(window) = self.window.borrow().as_ref() {
                            window.is_active.set(true);
                            window.visibility.set(WindowVisibility::Visible);
                            if let Some(cb) =
                                window.callbacks.borrow_mut().active_status_change.as_mut()
                            {
                                cb(true);
                            }
                            window.request_frame();
                        }
                        if let Some(cb) = self.callbacks.borrow_mut().app_lifecycle.as_mut() {
                            cb(AppLifecyclePhase::Active);
                        }
                    }
                    MainEvent::LostFocus => {
                        if let Some(window) = self.window.borrow().as_ref() {
                            window.is_active.set(false);
                            if let Some(cb) =
                                window.callbacks.borrow_mut().active_status_change.as_mut()
                            {
                                cb(false);
                            }
                        }
                        if let Some(cb) = self.callbacks.borrow_mut().app_lifecycle.as_mut() {
                            cb(AppLifecyclePhase::Inactive);
                        }
                    }
                    MainEvent::Pause => {
                        if let Some(cb) = self.callbacks.borrow_mut().app_lifecycle.as_mut() {
                            cb(AppLifecyclePhase::Background);
                        }
                    }
                    MainEvent::Resume { .. } => {
                        if let Some(cb) = self.callbacks.borrow_mut().app_lifecycle.as_mut() {
                            cb(AppLifecyclePhase::Foreground);
                        }
                    }
                    MainEvent::LowMemory => {
                        if let Some(cb) = self.callbacks.borrow_mut().memory_warning.as_mut() {
                            cb();
                        }
                    }
                    MainEvent::Destroy => {
                        should_break = true;
                    }
                    _ => {}
                },
                PollEvent::Wake => {}
                _ => {}
            });

            if should_break {
                break;
            }

            self.handle_input_events();
            if self.keyboard_check.take()
                && let Some(window) = self.window.borrow().clone()
            {
                let accepts_text = window.with_input_handler(|h| h.query_accepts_text_input()).unwrap_or(false);
                if accepts_text {
                    jni_bridge::show_keyboard();
                } else {
                    jni_bridge::hide_keyboard();
                }
            }
            self.handle_keyboard();
            self.handle_ime_events();
            self.handle_open_paths();
            self.dispatcher.drain_main_tasks();

            if let Some(window) = self.window.borrow().clone() {
                window.dispatch_frame();
            }
        }
    }

    fn quit(&self) {
        self.quit_requested.set(true);
    }

    fn restart(&self, _binary_path: Option<PathBuf>, _arguments: Vec<OsString>) {}

    fn activate(&self, _ignoring_other_apps: bool) {}

    fn hide(&self) {}

    fn hide_other_apps(&self) {}

    fn unhide_other_apps(&self) {}

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        vec![Rc::new(AndroidDisplay::new(Arc::clone(&self.display_state)))]
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(Rc::new(AndroidDisplay::new(Arc::clone(&self.display_state))))
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        self.window.borrow().as_ref().map(|w| w.handle)
    }

    fn open_window(
        &self,
        handle: AnyWindowHandle,
        _options: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        let native_window = self.app.native_window();
        let inner = AndroidWindowInner::new(
            handle,
            Arc::clone(&self.display_state),
            native_window,
            Rc::clone(&self.gpu_context),
            self.app.create_waker(),
        )?;
        *self.window.borrow_mut() = Some(Rc::clone(&inner));
        Ok(Box::new(AndroidWindow { inner }))
    }

    fn window_appearance(&self) -> WindowAppearance {
        WindowAppearance::Light
    }

    fn open_url(&self, url: &str) {
        jni_bridge::open_url(url);
    }

    fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {
        self.callbacks.borrow_mut().open_urls = Some(callback);
    }

    fn register_url_scheme(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn prompt_for_paths(
        &self,
        _options: PathPromptOptions,
    ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> {
        jni_bridge::pick_file_for_open()
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        jni_bridge::pick_path_for_save(suggested_name)
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        false
    }

    fn reveal_path(&self, _path: &Path) {}

    fn open_with_system(&self, _path: &Path) {}

    fn on_quit(&self, callback: Box<dyn FnMut() -> bool>) {
        self.callbacks.borrow_mut().quit = Some(callback);
    }

    fn on_reopen(&self, callback: Box<dyn FnMut()>) {
        self.callbacks.borrow_mut().reopen = Some(callback);
    }

    fn on_system_sleep(&self, _callback: Box<dyn FnMut()>) {}

    fn on_system_wake(&self, _callback: Box<dyn FnMut()>) {}

    fn on_app_lifecycle(&self, callback: Box<dyn FnMut(AppLifecyclePhase)>) {
        self.callbacks.borrow_mut().app_lifecycle = Some(callback);
    }

    fn on_memory_warning(&self, callback: Box<dyn FnMut()>) {
        self.callbacks.borrow_mut().memory_warning = Some(callback);
    }

    fn set_menus(&self, _menus: Vec<Menu>, _keymap: &Keymap) {}

    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {}

    fn on_app_menu_action(&self, _callback: Box<dyn FnMut(&dyn Action)>) {}

    fn on_will_open_app_menu(&self, _callback: Box<dyn FnMut()>) {}

    fn on_validate_app_menu_command(&self, _callback: Box<dyn FnMut(&dyn Action) -> bool>) {}

    fn thermal_state(&self) -> ThermalState {
        ThermalState::Nominal
    }

    fn on_thermal_state_change(&self, _callback: Box<dyn FnMut()>) {}

    fn prevent_idle_sleep(&self, _reason: &str) -> Task<Result<gpui::ActivityGuard>> {
        Task::ready(Ok(gpui::ActivityGuard::noop()))
    }

    fn app_path(&self) -> Result<PathBuf> {
        Ok(PathBuf::from("/"))
    }

    fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> {
        anyhow::bail!("auxiliary executables are not supported on Android")
    }

    fn set_cursor_style(&self, _style: CursorStyle) {}

    fn hide_cursor_until_mouse_moves(&self) {}

    fn is_cursor_visible(&self) -> bool {
        false
    }

    fn should_auto_hide_scrollbars(&self) -> bool {
        true
    }

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        jni_bridge::get_clipboard_text().map(ClipboardItem::new_string)
    }

    fn write_to_clipboard(&self, item: ClipboardItem) {
        if let Some(text) = item.text() {
            jni_bridge::set_clipboard_text(&text);
        }
    }

    fn write_credentials(&self, _url: &str, _username: &str, _password: &[u8]) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn read_credentials(&self, _url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> {
        Task::ready(Ok(None))
    }

    fn delete_credentials(&self, _url: &str) -> Task<Result<()>> {
        Task::ready(Ok(()))
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(AndroidKeyboardLayout)
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(DummyKeyboardMapper)
    }

    fn on_keyboard_layout_change(&self, _callback: Box<dyn FnMut()>) {}
}
