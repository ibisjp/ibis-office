// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/247)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use crate::bridge;
use crate::display::{IosDisplay, IosDisplayState};
use gpui::{
    AnyWindowHandle, Bounds, Capslock, DevicePixels, DispatchEventResult, GpuSpecs, Modifiers,
    Pixels, PlatformAtlas, PlatformDisplay, PlatformInput, PlatformInputHandler, PlatformWindow,
    Point, PromptButton, PromptLevel, RequestFrameOptions, Scene, Size, WindowAppearance,
    WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowVisibility, px,
};
use gpui_wgpu::{GpuContext, WgpuRenderer, WgpuSurfaceConfig};
use parking_lot::Mutex;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, RawDisplayHandle,
    RawWindowHandle, UiKitDisplayHandle, UiKitWindowHandle, WindowHandle,
};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Debug)]
struct IosSurfaceHandle {
    ui_view: NonNull<c_void>,
}

unsafe impl Send for IosSurfaceHandle {}
unsafe impl Sync for IosSurfaceHandle {}

impl HasWindowHandle for IosSurfaceHandle {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = RawWindowHandle::UiKit(UiKitWindowHandle::new(self.ui_view));
        Ok(unsafe { WindowHandle::borrow_raw(handle) })
    }
}

impl HasDisplayHandle for IosSurfaceHandle {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let handle = RawDisplayHandle::UiKit(UiKitDisplayHandle::new());
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

pub struct IosWindowInner {
    pub handle: AnyWindowHandle,
    pub display_state: Arc<Mutex<IosDisplayState>>,
    pub ui_view: NonNull<c_void>,
    pub renderer: RefCell<WgpuRenderer>,
    pub atlas: Arc<dyn PlatformAtlas>,
    pub callbacks: RefCell<WindowCallbacks>,
    pub input_handler: RefCell<Option<PlatformInputHandler>>,
    pub mouse_position: Cell<Point<Pixels>>,
    pub modifiers: Cell<Modifiers>,
    pub is_active: Cell<bool>,
    pub visibility: Cell<WindowVisibility>,
    pub frame_requested: Arc<Mutex<bool>>,
}

impl IosWindowInner {
    pub fn new(
        handle: AnyWindowHandle,
        display_state: Arc<Mutex<IosDisplayState>>,
        ui_view: NonNull<c_void>,
        gpu_context: GpuContext,
    ) -> anyhow::Result<Rc<Self>> {
        let state = display_state.lock();
        let scale = state.scale_factor.max(1.0);
        let config = WgpuSurfaceConfig {
            size: Size {
                width: DevicePixels((state.logical_width * scale).round().max(1.0) as i32),
                height: DevicePixels((state.logical_height * scale).round().max(1.0) as i32),
            },
            transparent: false,
            preferred_present_mode: None,
        };
        drop(state);

        let surface_handle = IosSurfaceHandle { ui_view };
        if gpu_context.borrow().is_none() {
            let instance = gpui_wgpu::wgpu::Instance::new(gpui_wgpu::wgpu::InstanceDescriptor {
                backends: gpui_wgpu::wgpu::Backends::METAL,
                flags: gpui_wgpu::wgpu::InstanceFlags::default(),
                backend_options: gpui_wgpu::wgpu::BackendOptions::default(),
                memory_budget_thresholds: gpui_wgpu::wgpu::MemoryBudgetThresholds::default(),
                display: Some(Box::new(surface_handle.clone())),
            });
            let window_handle = surface_handle
                .window_handle()
                .map_err(|e| anyhow::anyhow!("Failed to get window handle: {e}"))?;
            let surface = unsafe {
                instance.create_surface_unsafe(gpui_wgpu::wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: None,
                    raw_window_handle: window_handle.as_raw(),
                })?
            };
            let ctx = gpui_wgpu::WgpuContext::new(instance, &surface, None)?;
            *gpu_context.borrow_mut() = Some(ctx);
        }
        let renderer = WgpuRenderer::new(gpu_context, &surface_handle, config, None)?;
        let atlas = Arc::clone(renderer.sprite_atlas()) as Arc<dyn PlatformAtlas>;

        Ok(Rc::new(Self {
            handle,
            display_state,
            ui_view,
            renderer: RefCell::new(renderer),
            atlas,
            callbacks: RefCell::new(WindowCallbacks::default()),
            input_handler: RefCell::new(None),
            mouse_position: Cell::new(Point::default()),
            modifiers: Cell::new(Modifiers::default()),
            is_active: Cell::new(true),
            visibility: Cell::new(WindowVisibility::Visible),
            frame_requested: Arc::new(Mutex::new(true)),
        }))
    }

    pub fn notify_resize(&self, logical_width: f32, logical_height: f32, scale_factor: f32) {
        let scale = scale_factor.max(1.0);
        {
            let mut state = self.display_state.lock();
            state.logical_width = logical_width.max(1.0);
            state.logical_height = logical_height.max(1.0);
            state.scale_factor = scale;
        }
        self.renderer.borrow_mut().update_drawable_size(Size {
            width: DevicePixels((logical_width * scale).round().max(1.0) as i32),
            height: DevicePixels((logical_height * scale).round().max(1.0) as i32),
        });
        let logical = Size {
            width: px(logical_width.max(1.0)),
            height: px(logical_height.max(1.0)),
        };
        if let Some(cb) = self.callbacks.borrow_mut().resize.as_mut() {
            cb(logical, scale);
        }
        self.request_frame();
    }

    pub fn request_frame(&self) {
        *self.frame_requested.lock() = true;
        bridge::wake_main();
    }

    pub fn dispatch_frame(&self) {
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

    pub fn with_input_handler<R>(&self, f: impl FnOnce(&mut PlatformInputHandler) -> R) -> Option<R> {
        let mut handler = self.input_handler.borrow_mut().take()?;
        let result = f(&mut handler);
        let mut slot = self.input_handler.borrow_mut();
        if slot.is_none() {
            *slot = Some(handler);
        }
        Some(result)
    }
}

pub struct IosWindow {
    pub inner: Rc<IosWindowInner>,
}

impl HasWindowHandle for IosWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let handle = RawWindowHandle::UiKit(UiKitWindowHandle::new(self.inner.ui_view));
        Ok(unsafe { WindowHandle::borrow_raw(handle) })
    }
}

impl HasDisplayHandle for IosWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let handle = RawDisplayHandle::UiKit(UiKitDisplayHandle::new());
        Ok(unsafe { DisplayHandle::borrow_raw(handle) })
    }
}

impl PlatformWindow for IosWindow {
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
        Size {
            width: px(state.logical_width.max(1.0)),
            height: px(state.logical_height.max(1.0)),
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
        Some(Rc::new(IosDisplay::new(Arc::clone(
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
        Some(Rc::new(move || {
            *requested.lock() = true;
            bridge::wake_main();
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
        if !self.inner.renderer.borrow_mut().draw(scene) {
            log::warn!("WgpuRenderer::draw returned false on iOS");
        }
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        Arc::clone(&self.inner.atlas)
    }

    fn is_subpixel_rendering_supported(&self) -> bool {
        false
    }

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        None
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {}
}
