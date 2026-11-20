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
    MenuItem, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PathPromptOptions, PinchEvent, Pixels, Platform, PlatformDisplay, PlatformInput,
    PlatformKeyboardLayout, PlatformKeyboardMapper, PlatformTextSystem, PlatformWindow, Point,
    ScrollDelta, ScrollWheelEvent, Task, ThermalState, TouchPhase, WindowAppearance,
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
use std::time::{Duration, Instant};

const TAP_SLOP_PX: f32 = 8.0;
const DOUBLE_TAP_MAX_DIST_PX: f32 = 24.0;
const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(400);

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

#[derive(Debug, Clone, Copy)]
enum TouchState {
    Idle,
    Single {
        start: (f32, f32),
        last: (f32, f32),
        panning: bool,
    },
}

pub struct IosPlatform {
    ui_view: NonNull<c_void>,
    dispatcher: Arc<IosDispatcher>,
    text_system: Arc<CosmicTextSystem>,
    gpu_context: GpuContext,
    display_state: Arc<Mutex<IosDisplayState>>,
    window: RefCell<Option<Rc<IosWindowInner>>>,
    callbacks: RefCell<PlatformCallbacks>,
    touch_state: Cell<TouchState>,
    last_tap: Cell<Option<(Instant, (f32, f32), usize)>>,
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
            touch_state: Cell::new(TouchState::Idle),
            last_tap: Cell::new(None),
        }
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

    pub fn touch_began(&self, x: f32, y: f32) {
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        let pos = (x, y);
        self.touch_state.set(TouchState::Single {
            start: pos,
            last: pos,
            panning: false,
        });
        window.dispatch_input(PlatformInput::MouseMove(MouseMoveEvent {
            position: point(px(x), px(y)),
            pressed_button: None,
            modifiers: Modifiers::default(),
        }));
        window.request_frame();
    }

    pub fn touch_moved(&self, x: f32, y: f32) {
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        let pos = (x, y);
        if let TouchState::Single {
            start,
            last,
            mut panning,
        } = self.touch_state.get()
        {
            if !panning && distance(start, pos) >= TAP_SLOP_PX {
                panning = true;
                window.dispatch_input(PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position: point(px(x), px(y)),
                    delta: ScrollDelta::Pixels(point(px(0.0), px(0.0))),
                    modifiers: Modifiers::default(),
                    touch_phase: TouchPhase::Started,
                }));
            }
            if panning {
                let dx = pos.0 - last.0;
                let dy = pos.1 - last.1;
                window.dispatch_input(PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position: point(px(x), px(y)),
                    delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
                    modifiers: Modifiers::default(),
                    touch_phase: TouchPhase::Moved,
                }));
                window.request_frame();
            }
            self.touch_state.set(TouchState::Single {
                start,
                last: pos,
                panning,
            });
        }
    }

    pub fn touch_ended(&self, x: f32, y: f32, cancelled: bool) {
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        let pos = (x, y);
        match self.touch_state.get() {
            TouchState::Single { panning: true, .. } => {
                window.dispatch_input(PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position: point(px(x), px(y)),
                    delta: ScrollDelta::Pixels(point(px(0.0), px(0.0))),
                    modifiers: Modifiers::default(),
                    touch_phase: if cancelled {
                        TouchPhase::Cancelled
                    } else {
                        TouchPhase::Ended
                    },
                }));
            }
            TouchState::Single { panning: false, .. } if !cancelled => {
                let now = Instant::now();
                let click_count = match self.last_tap.get() {
                    Some((prev_t, prev_pos, prev_count))
                        if now.duration_since(prev_t) <= DOUBLE_TAP_WINDOW
                            && distance(prev_pos, pos) <= DOUBLE_TAP_MAX_DIST_PX =>
                    {
                        (prev_count % 3) + 1
                    }
                    _ => 1,
                };
                self.last_tap.set(Some((now, pos, click_count)));
                let gpui_pos: Point<Pixels> = point(px(x), px(y));
                window.dispatch_input(PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: gpui_pos,
                    modifiers: Modifiers::default(),
                    click_count,
                    first_mouse: false,
                }));
                window.dispatch_input(PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: gpui_pos,
                    modifiers: Modifiers::default(),
                    click_count,
                }));
                let accepts_text = window
                    .with_input_handler(|h| h.query_accepts_text_input())
                    .unwrap_or(false);
                if accepts_text {
                    bridge::show_keyboard();
                } else {
                    bridge::hide_keyboard();
                }
            }
            _ => {}
        }
        self.touch_state.set(TouchState::Idle);
        window.request_frame();
    }

    pub fn pinch(&self, x: f32, y: f32, delta: f32, phase: u8) {
        let Some(window) = self.window.borrow().clone() else {
            return;
        };
        let touch_phase = match phase {
            0 => {
                self.touch_state.set(TouchState::Idle);
                TouchPhase::Started
            }
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

    pub fn tick(&self) {
        self.handle_ime_events();
        self.handle_open_paths();
        self.dispatcher.drain_main_tasks();
        if let Some(window) = self.window.borrow().clone() {
            window.dispatch_frame();
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
