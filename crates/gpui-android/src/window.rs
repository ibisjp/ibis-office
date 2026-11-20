// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/247)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// Changed for GitHub issue #256 (masanork, commit eb3f1a78 of github.com/masanork/ibis-office): the sprite atlas and the renderer are kept when the native window comes back from the file picker; only the surface is replaced.
use crate::display::{AndroidDisplay, AndroidDisplayState};
use android_activity::AndroidAppWaker;
use gpui::{
    AnyWindowHandle, Bounds, Capslock, DevicePixels, DispatchEventResult, GpuSpecs, Modifiers,
    Pixels, PlatformAtlas, PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow,
    Point, PromptButton, PromptLevel, RequestFrameOptions, Scene, Size, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowVisibility, px,
};
use gpui_wgpu::{GpuContext, WgpuRenderer, WgpuSurfaceConfig};
use ndk::native_window::NativeWindow;
use parking_lot::Mutex;
use raw_window_handle::{
    AndroidDisplayHandle, DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle,
    RawDisplayHandle, WindowHandle,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct AndroidSurfaceHandle {
    window: NativeWindow,
}

unsafe impl Send for AndroidSurfaceHandle {}
unsafe impl Sync for AndroidSurfaceHandle {}

impl HasWindowHandle for AndroidSurfaceHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.window.window_handle()
    }
}

impl HasDisplayHandle for AndroidSurfaceHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let handle = RawDisplayHandle::Android(AndroidDisplayHandle::new());
        Ok(unsafe { DisplayHandle::borrow_raw(handle) })
    }
}

#[derive(Default)]
pub struct WindowCallbacks {
    pub request_frame: Option<Box<dyn FnMut(RequestFrameOptions)>>,
    pub input: Option<Box<dyn FnMut(PlatformInput) -> DispatchEventResult>>,
    pub active_status_change: Option<Box<dyn FnMut(bool)>>,
    pub visibility_change: Option<Box<dyn FnMut(WindowVisibility)>>,
    pub hover_status_change: Option<Box<dyn FnMut(bool)>>,
    pub resize: Option<Box<dyn FnMut(Size<Pixels>, f32)>>,
    pub moved: Option<Box<dyn FnMut()>>,
    pub should_close: Option<Box<dyn FnMut() -> bool>>,
    pub hit_test_window_control: Option<Box<dyn FnMut() -> Option<WindowControlArea>>>,
    pub close: Option<Box<dyn FnOnce()>>,
    pub appearance_changed: Option<Box<dyn FnMut()>>,
}

pub struct AndroidWindowInner {
    pub handle: AnyWindowHandle,
    pub display_state: Arc<Mutex<AndroidDisplayState>>,
    pub gpu_context: GpuContext,
    pub renderer: RefCell<Option<WgpuRenderer>>,
    // Drop the renderer before releasing the handle it uses.
    native_window: RefCell<Option<NativeWindow>>,
    surface_active: Cell<bool>,
    pub atlas: RefCell<Option<Arc<dyn PlatformAtlas>>>,
    pub callbacks: RefCell<WindowCallbacks>,
    pub input_handler: RefCell<Option<PlatformInputHandler>>,
    pub mouse_position: Cell<Point<Pixels>>,
    pub modifiers: Cell<Modifiers>,
    pub is_active: Cell<bool>,
    pub visibility: Cell<WindowVisibility>,
    pub frame_requested: Arc<Mutex<bool>>,
    pub waker: AndroidAppWaker,
}

impl AndroidWindowInner {
    pub fn new(
        handle: AnyWindowHandle,
        display_state: Arc<Mutex<AndroidDisplayState>>,
        native_window: Option<NativeWindow>,
        gpu_context: GpuContext,
        waker: AndroidAppWaker,
    ) -> anyhow::Result<Rc<Self>> {
        let renderer = match &native_window {
            Some(win) => Some(Self::create_renderer(&gpu_context, win, &display_state)?),
            None => None,
        };
        let atlas = renderer
            .as_ref()
            .map(|r| Arc::clone(r.sprite_atlas()) as Arc<dyn PlatformAtlas>);

        Ok(Rc::new(Self {
            handle,
            display_state,
            surface_active: Cell::new(native_window.is_some()),
            native_window: RefCell::new(native_window),
            gpu_context,
            renderer: RefCell::new(renderer),
            atlas: RefCell::new(atlas),
            callbacks: RefCell::new(WindowCallbacks::default()),
            input_handler: RefCell::new(None),
            mouse_position: Cell::new(Point::default()),
            modifiers: Cell::new(Modifiers::default()),
            is_active: Cell::new(true),
            visibility: Cell::new(WindowVisibility::Visible),
            frame_requested: Arc::new(Mutex::new(true)),
            waker,
        }))
    }

    fn surface_config(display_state: &Mutex<AndroidDisplayState>) -> WgpuSurfaceConfig {
        let state = display_state.lock();
        WgpuSurfaceConfig {
            size: Size {
                width: DevicePixels(state.physical_width.max(1) as i32),
                height: DevicePixels(state.physical_height.max(1) as i32),
            },
            transparent: false,
            preferred_present_mode: None,
        }
    }

    fn create_renderer(
        gpu_context: &GpuContext,
        native_window: &NativeWindow,
        display_state: &Mutex<AndroidDisplayState>,
    ) -> anyhow::Result<WgpuRenderer> {
        let surface_handle = AndroidSurfaceHandle {
            window: native_window.clone(),
        };
        let config = Self::surface_config(display_state);
        WgpuRenderer::new(Rc::clone(gpu_context), &surface_handle, config, None)
    }

    pub fn set_native_window(&self, native_window: Option<NativeWindow>) {
        let mut renderer = self.renderer.borrow_mut();
        self.surface_active.set(false);
        if let Some(renderer) = renderer.as_mut() {
            renderer.unconfigure_surface();
        }
        let Some(window) = native_window else {
            return;
        };

        let result = if let Some(renderer) = renderer.as_mut() {
            let config = Self::surface_config(&self.display_state);
            let handle = AndroidSurfaceHandle {
                window: window.clone(),
            };
            let context = self.gpu_context.borrow();
            let instance = &context
                .as_ref()
                .expect("renderer requires a GPU context")
                .instance;
            renderer.replace_surface(&handle, config, instance)
        } else {
            Self::create_renderer(&self.gpu_context, &window, &self.display_state).map(|created| {
                *self.atlas.borrow_mut() =
                    Some(Arc::clone(created.sprite_atlas()) as Arc<dyn PlatformAtlas>);
                *renderer = Some(created);
            })
        };

        match result {
            Ok(()) => {
                *self.native_window.borrow_mut() = Some(window);
                self.surface_active.set(true);
                self.request_frame();
            }
            Err(err) => log::error!("Failed to attach Android renderer to native window: {err:#}"),
        }
    }

    pub fn notify_resize(&self, width: u32, height: u32, scale_factor: f32) {
        {
            let mut state = self.display_state.lock();
            state.physical_width = width.max(1);
            state.physical_height = height.max(1);
            state.scale_factor = scale_factor.max(1.0);
        }
        if self.surface_active.get()
            && let Some(renderer) = self.renderer.borrow_mut().as_mut()
        {
            renderer.update_drawable_size(Size {
                width: DevicePixels(width.max(1) as i32),
                height: DevicePixels(height.max(1) as i32),
            });
        }
        let logical = Size {
            width: px(width.max(1) as f32 / scale_factor.max(1.0)),
            height: px(height.max(1) as f32 / scale_factor.max(1.0)),
        };
        if let Some(cb) = self.callbacks.borrow_mut().resize.as_mut() {
            cb(logical, scale_factor.max(1.0));
        }
        self.request_frame();
    }

    pub fn request_frame(&self) {
        *self.frame_requested.lock() = true;
        self.waker.wake();
    }

    pub fn dispatch_frame(&self) {
        if !self.surface_active.get() {
            return;
        }
        let should_draw = {
            let mut flag = self.frame_requested.lock();
            let was = *flag;
            *flag = false;
            was
        };
        if !should_draw {
            return;
        }
        let mut cb = self.callbacks.borrow_mut().request_frame.take();
        if let Some(ref mut callback) = cb {
            callback(RequestFrameOptions {
                require_presentation: false,
                force_render: false,
            });
        }
        let mut slot = self.callbacks.borrow_mut();
        if slot.request_frame.is_none() {
            slot.request_frame = cb;
        }
    }

    pub fn dispatch_input(&self, input: PlatformInput) -> DispatchEventResult {
        if let PlatformInput::MouseMove(ref ev) = input {
            self.mouse_position.set(ev.position);
        } else if let PlatformInput::MouseDown(ref ev) = input {
            self.mouse_position.set(ev.position);
        } else if let PlatformInput::MouseUp(ref ev) = input {
            self.mouse_position.set(ev.position);
        }
        let mut cb = self.callbacks.borrow_mut().input.take();
        let result = if let Some(ref mut callback) = cb {
            callback(input)
        } else {
            DispatchEventResult::default()
        };
        let mut slot = self.callbacks.borrow_mut();
        if slot.input.is_none() {
            slot.input = cb;
        }
        result
    }

    pub fn with_input_handler<R>(
        &self,
        f: impl FnOnce(&mut PlatformInputHandler) -> R,
    ) -> Option<R> {
        let mut handler = self.input_handler.borrow_mut().take()?;
        let result = f(&mut handler);
        let mut slot = self.input_handler.borrow_mut();
        if slot.is_none() {
            *slot = Some(handler);
        }
        Some(result)
    }
}

pub struct AndroidWindow {
    pub inner: Rc<AndroidWindowInner>,
}

impl HasWindowHandle for AndroidWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

impl HasDisplayHandle for AndroidWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let handle = RawDisplayHandle::Android(AndroidDisplayHandle::new());
        Ok(unsafe { DisplayHandle::borrow_raw(handle) })
    }
}

impl PlatformWindow for AndroidWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(Point::default(), self.content_size())
    }

    fn is_maximized(&self) -> bool {
        true
    }

    fn window_bounds(&self) -> WindowBounds {
        WindowBounds::Fullscreen(self.bounds())
    }

    fn content_size(&self) -> Size<Pixels> {
        let state = self.inner.display_state.lock();
        let scale = state.scale_factor.max(1.0);
        Size {
            width: px(state.physical_width as f32 / scale),
            height: px(state.physical_height as f32 / scale),
        }
    }

    fn resize(&mut self, _size: Size<Pixels>) {}

    fn scale_factor(&self) -> f32 {
        self.inner.display_state.lock().scale_factor.max(1.0)
    }

    fn appearance(&self) -> WindowAppearance {
        WindowAppearance::Light
    }

    fn display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        Some(Rc::new(AndroidDisplay::new(Arc::clone(
            &self.inner.display_state,
        ))))
    }

    fn mouse_position(&self) -> Point<Pixels> {
        self.inner.mouse_position.get()
    }

    fn modifiers(&self) -> Modifiers {
        self.inner.modifiers.get()
    }

    fn capslock(&self) -> Capslock {
        Capslock::default()
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        *self.inner.input_handler.borrow_mut() = Some(input_handler);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.inner.input_handler.borrow_mut().take()
    }

    fn prompt(
        &self,
        _level: PromptLevel,
        _msg: &str,
        _detail: Option<&str>,
        _answers: &[PromptButton],
    ) -> Option<futures::channel::oneshot::Receiver<usize>> {
        None
    }

    fn activate(&self) {
        self.inner.is_active.set(true);
    }

    fn is_active(&self) -> bool {
        self.inner.is_active.get()
    }

    fn visibility(&self) -> WindowVisibility {
        self.inner.visibility.get()
    }

    fn is_hovered(&self) -> bool {
        false
    }

    fn background_appearance(&self) -> WindowBackgroundAppearance {
        WindowBackgroundAppearance::Opaque
    }

    fn set_title(&mut self, _title: &str) {}

    fn set_background_appearance(&self, _background_appearance: WindowBackgroundAppearance) {}

    fn minimize(&self) {}

    fn zoom(&self) {}

    fn toggle_fullscreen(&self) {}

    fn is_fullscreen(&self) -> bool {
        true
    }

    fn frame_waker(&self) -> Option<Rc<dyn Fn()>> {
        let requested = Arc::clone(&self.inner.frame_requested);
        let waker = self.inner.waker.clone();
        Some(Rc::new(move || {
            *requested.lock() = true;
            waker.wake();
        }))
    }

    fn schedule_frame(&self) {
        self.inner.request_frame();
    }

    fn on_request_frame(&self, callback: Box<dyn FnMut(RequestFrameOptions)>) {
        self.inner.callbacks.borrow_mut().request_frame = Some(callback);
    }

    fn on_input(&self, callback: Box<dyn FnMut(PlatformInput) -> DispatchEventResult>) {
        self.inner.callbacks.borrow_mut().input = Some(callback);
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.inner.callbacks.borrow_mut().active_status_change = Some(callback);
    }

    fn on_visibility_change(&self, callback: Box<dyn FnMut(WindowVisibility)>) {
        self.inner.callbacks.borrow_mut().visibility_change = Some(callback);
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.inner.callbacks.borrow_mut().hover_status_change = Some(callback);
    }

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        self.inner.callbacks.borrow_mut().resize = Some(callback);
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        self.inner.callbacks.borrow_mut().moved = Some(callback);
    }

    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        self.inner.callbacks.borrow_mut().should_close = Some(callback);
    }

    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        self.inner.callbacks.borrow_mut().hit_test_window_control = Some(callback);
    }

    fn on_close(&self, callback: Box<dyn FnOnce()>) {
        self.inner.callbacks.borrow_mut().close = Some(callback);
    }

    fn on_appearance_changed(&self, callback: Box<dyn FnMut()>) {
        self.inner.callbacks.borrow_mut().appearance_changed = Some(callback);
    }

    fn draw(&self, scene: &Scene) {
        if self.inner.surface_active.get()
            && let Some(renderer) = self.inner.renderer.borrow_mut().as_mut()
        {
            if !renderer.draw(scene) {
                log::warn!("WgpuRenderer::draw returned false on Android");
            }
        }
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        if let Some(renderer) = self.inner.renderer.borrow().as_ref() {
            return Arc::clone(renderer.sprite_atlas()) as Arc<dyn PlatformAtlas>;
        }
        self.inner
            .atlas
            .borrow()
            .as_ref()
            .expect("sprite_atlas requested before Android surface initialized")
            .clone()
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        false
    }

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        None
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {}
}
