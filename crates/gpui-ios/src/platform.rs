// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/247)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use crate::bridge::{self, ImeBridgeEvent};
use crate::dispatcher::IosDispatcher;
use crate::display::{IosDisplay, IosDisplayState};
use crate::window::{IosWindow, IosWindowInner};
use anyhow::Result;
use futures::channel::oneshot;
use gpui::{
    Action, AnyWindowHandle, AppLifecyclePhase, BackgroundExecutor, ClipboardItem, CursorStyle,
    DummyKeyboardMapper, ForegroundExecutor, KeyDownEvent, KeyUpEvent, Keymap, Keystroke, Menu,
    MenuItem, Modifiers, 
    PathPromptOptions, PinchEvent, Pixels, Platform, PlatformDisplay, PlatformInput,
    PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Point,
    Task, ThermalState, TouchEvent, TouchId, TouchPhase, WindowAppearance,
    WindowParams, point, px,
};
use gpui_wgpu::{CosmicTextSystem, GpuContext};
use parking_lot::Mutex;
use std::cell::{Cell, RefCell};
use std::ffi::{OsString, c_void};
use std::path::{Path, PathBuf};
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;

const TAP_SLOP_PX: f32 = 8.0;
/// How many ticks after a tap the soft keyboard's show or hide may wait for the page's input handler to say it takes text.
const KEYBOARD_CHECK_TICKS: u8 = 3;

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

struct IosKeyboardLayout;

impl PlatformKeyboardLayout for IosKeyboardLayout {
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

/// What the platform keeps of the touches, to know a tap (for the soft keyboard).
#[derive(Debug, Clone, Copy, Default)]
struct TapState {
    /// Fingers on the screen now.
    active: u32,
    start: (f32, f32),
    /// The only finger has not moved out of the slop since it went down.
    candidate: bool,
}

pub struct IosPlatform {
    ui_view: NonNull<c_void>,
    dispatcher: Arc<IosDispatcher>,
    text_system: Arc<CosmicTextSystem>,
    gpu_context: GpuContext,
    display_state: Arc<Mutex<IosDisplayState>>,
    window: RefCell<Option<Rc<IosWindowInner>>>,
    callbacks: RefCell<PlatformCallbacks>,
    tap: Cell<TapState>,
    /// A tap ended: whether the soft keyboard shows is decided on the next tick (the page has handled the tap by then).
    /// Ticks left to decide it in (0: nothing to decide): the page's input handler is registered when the frame after the tap is painted, so the
    /// answer is read after the frame, and asked again a few times before the keyboard is hidden.
    keyboard_check: Cell<u8>,
    /// The soft keyboard the window was last laid out with (K0064): the window is laid out again when it moves.
    keyboard_seen: Cell<bridge::KeyboardInfo>,
}

impl IosPlatform {
    pub fn new(
        ui_view: NonNull<c_void>,
        logical_width: f32,
        logical_height: f32,
        scale_factor: f32,
    ) -> Self {
        let dispatcher = Arc::new(IosDispatcher::new());
        let text_system = Arc::new(CosmicTextSystem::new("sans-serif"));
        let display_state = Arc::new(Mutex::new(IosDisplayState {
            logical_width: logical_width.max(1.0),
            logical_height: logical_height.max(1.0),
            scale_factor: scale_factor.max(1.0),
        }));

        Self {
            ui_view,
            dispatcher,
            text_system,
            gpu_context: Rc::new(RefCell::new(None)),
            display_state,
            window: RefCell::new(None),
            callbacks: RefCell::new(PlatformCallbacks::default()),
            tap: Cell::new(TapState::default()),
            keyboard_check: Cell::new(0),
            keyboard_seen: Cell::new(bridge::KeyboardInfo::HIDDEN),
        }
    }

    /// A font file added by its path, mapped and not read into memory (K0066: Apple Color Emoji is 139 MB).
    pub fn add_font_file(&self, path: &Path) -> Result<()> {
        self.text_system.add_font_file(path)
    }

    pub fn resize(&self, logical_width: f32, logical_height: f32, scale_factor: f32) {
        if let Some(window) = self.window.borrow().as_ref() {
            window.notify_resize(logical_width, logical_height, scale_factor);
        } else {
            let mut state = self.display_state.lock();
            state.logical_width = logical_width.max(1.0);
            state.logical_height = logical_height.max(1.0);
            state.scale_factor = scale_factor.max(1.0);
        }
    }

    /// One touch of the screen (K0065): `id` tells the fingers apart, `phase` is 0 began, 1 moved, 2 ended, 3 cancelled. gpui's touch recognizer (taps,
    /// pans, two fingers scrolling, their coasting) takes them as it does on the web; the platform only decides, for the soft keyboard, whether the
    /// touch was a tap.
    pub fn touch(&self, id: u64, phase: u8, x: f32, y: f32) {
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        let touch_phase = match phase {
            0 => TouchPhase::Started,
            1 => TouchPhase::Moved,
            2 => TouchPhase::Ended,
            _ => TouchPhase::Cancelled,
        };
        let mut tap = self.tap.get();
        match touch_phase {
            TouchPhase::Started => {
                // A second finger is never a tap.
                tap.candidate = tap.active == 0;
                if tap.active == 0 {
                    tap.start = (x, y);
                }
                tap.active += 1;
            }
            TouchPhase::Moved => {
                if tap.candidate && distance(tap.start, (x, y)) >= TAP_SLOP_PX {
                    tap.candidate = false;
                }
            }
            _ => {
                tap.active = tap.active.saturating_sub(1);
                if tap.active == 0 && touch_phase == TouchPhase::Ended && tap.candidate {
                    self.keyboard_check.set(KEYBOARD_CHECK_TICKS);
                }
                tap.candidate = false;
            }
        }
        self.tap.set(tap);
        window.dispatch_input(PlatformInput::Touch(TouchEvent {
            id: TouchId(id),
            phase: touch_phase,
            position: point(px(x), px(y)),
            predicted_position: None,
            force: None,
        }));
        window.request_frame();
    }

    pub fn pinch(&self, x: f32, y: f32, delta: f32, phase: u8) {
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        let touch_phase = match phase {
            0 => TouchPhase::Started,
            1 => TouchPhase::Moved,
            2 => TouchPhase::Ended,
            _ => TouchPhase::Cancelled,
        };
        window.dispatch_input(PlatformInput::Pinch(PinchEvent {
            position: point(px(x), px(y)),
            delta,
            modifiers: Modifiers::default(),
            phase: touch_phase,
        }));
        window.request_frame();
    }

    pub fn lifecycle(&self, phase: AppLifecyclePhase) {
        if let Some(cb) = self.callbacks.borrow_mut().app_lifecycle.as_mut() {
            cb(phase);
        }
    }

    pub fn memory_warning(&self) {
        if let Some(cb) = self.callbacks.borrow_mut().memory_warning.as_mut() {
            cb();
        }
    }

    fn send_key(&self, window: &IosWindowInner, key: &str) {
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
        let events = bridge::drain_ime_events();
        if events.is_empty() {
            return;
        }
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        for event in events {
            match event {
                ImeBridgeEvent::Commit(text) => {
                    window.with_input_handler(|handler| {
                        handler.replace_text_in_range(None, &text);
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
                ImeBridgeEvent::Enter => self.send_key(&window, "enter"),
            }
        }
    }

    fn handle_open_paths(&self) {
        let paths = bridge::drain_open_paths();
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

    /// The soft keyboard moved: the window is laid out again (the shell reads `gpui_ios::keyboard()` as it lays out; K0064). The view
    /// itself does not change its size: the keyboard covers its bottom and the shell leaves that part to it.
    fn handle_keyboard(&self) {
        let now = bridge::keyboard();
        if now == self.keyboard_seen.get() {
            return;
        }
        self.keyboard_seen.set(now);
        if let Some(window) = self.window.borrow().clone() {
            let (w, h, scale) = {
                let state = self.display_state.lock();
                (state.logical_width, state.logical_height, state.scale_factor)
            };
            window.notify_resize(w, h, scale);
        }
    }

    pub fn tick(&self) {
        self.handle_keyboard();
        self.handle_ime_events();
        self.handle_open_paths();
        self.dispatcher.drain_main_tasks();
        if let Some(window) = self.window.borrow().clone() {
            window.dispatch_frame();
        }
        // After the frame: the tap has been handled and painted, so the page's input handler is the one of what was tapped.
        let tries = self.keyboard_check.get();
        if tries > 0
            && let Some(window) = self.window.borrow().clone()
        {
            let asked = window.with_input_handler(|h| h.query_accepts_text_input());
            let accepts_text = asked.unwrap_or(false);
            if accepts_text {
                self.keyboard_check.set(0);
                bridge::show_keyboard();
            } else if tries <= 1 {
                self.keyboard_check.set(0);
                bridge::hide_keyboard();
            } else {
                // Not yet (no handler, or the one of the frame before the tap): ask again after the next frame.
                self.keyboard_check.set(tries - 1);
                window.request_frame();
            }
        }
    }
}

impl Platform for IosPlatform {
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
        on_finish_launching();
    }

    fn quit(&self) {}

    fn restart(&self, _binary_path: Option<PathBuf>, _arguments: Vec<OsString>) {}

    fn activate(&self, _ignoring_other_apps: bool) {}

    fn hide(&self) {}

    fn hide_other_apps(&self) {}

    fn unhide_other_apps(&self) {}

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        vec![Rc::new(IosDisplay::new(Arc::clone(&self.display_state)))]
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(Rc::new(IosDisplay::new(Arc::clone(&self.display_state))))
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        self.window.borrow().as_ref().map(|w| w.handle)
    }

    fn open_window(
        &self,
        handle: AnyWindowHandle,
        _options: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        let inner = IosWindowInner::new(
            handle,
            Arc::clone(&self.display_state),
            self.ui_view,
            Rc::clone(&self.gpu_context),
        )?;
        *self.window.borrow_mut() = Some(Rc::clone(&inner));
        Ok(Box::new(IosWindow { inner }))
    }

    fn window_appearance(&self) -> WindowAppearance {
        WindowAppearance::Light
    }

    fn open_url(&self, url: &str) {
        bridge::open_url(url);
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
        bridge::pick_file_for_open()
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        suggested_name: Option<&str>,
    ) -> oneshot::Receiver<Result<Option<PathBuf>>> {
        bridge::pick_path_for_save(suggested_name)
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
        anyhow::bail!("auxiliary executables are not supported on iOS")
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
        bridge::get_clipboard_text().map(ClipboardItem::new_string)
    }

    fn write_to_clipboard(&self, item: ClipboardItem) {
        if let Some(text) = item.text() {
            bridge::set_clipboard_text(&text);
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
        Box::new(IosKeyboardLayout)
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(DummyKeyboardMapper)
    }

    fn on_keyboard_layout_change(&self, _callback: Box<dyn FnMut()>) {}
}
