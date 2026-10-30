// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OS clipboard access for Office DrawingML, alongside GPUI's image/text formats.
#[cfg(target_os = "macos")]
mod platform {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe fn string(value: &str) -> *mut Object {
        unsafe {
            let s: *mut Object = msg_send![class!(NSString), alloc];
            let s: *mut Object =
                msg_send![s,initWithBytes:value.as_ptr() length:value.len() encoding:4usize];
            msg_send![s, autorelease]
        }
    }
    pub fn read() -> Option<Vec<u8>> {
        unsafe {
            let board: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
            let data: *mut Object =
                msg_send![board,dataForType:string("com.microsoft.Art--GVML-ClipFormat")];
            if data.is_null() {
                return None;
            }
            let len: usize = msg_send![data, length];
            let bytes: *const u8 = msg_send![data, bytes];
            if bytes.is_null() || len == 0 {
                return None;
            }
            Some(std::slice::from_raw_parts(bytes, len).to_vec())
        }
    }
    #[cfg(feature = "scenarios")]
    pub fn has_data(name: &str) -> bool {
        unsafe {
            let board: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
            let data: *mut Object = msg_send![board, dataForType:string(name)];
            !data.is_null()
        }
    }
    /// Add the native representation without removing the image/text fallbacks.
    pub fn write(bytes: &[u8], text: Option<&str>) -> bool {
        unsafe {
            let board: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
            let put = |name: &str, bytes: &[u8]| -> bool {
                let kind = string(name);
                let types: *mut Object = msg_send![class!(NSArray),arrayWithObject:kind];
                let _: isize = msg_send![board,addTypes:types owner:std::ptr::null_mut::<Object>()];
                let data: *mut Object =
                    msg_send![class!(NSData),dataWithBytes:bytes.as_ptr() length:bytes.len()];
                msg_send![board,setData:data forType:kind]
            };
            let native = put("com.microsoft.Art--GVML-ClipFormat", bytes);
            native && text.is_none_or(|text| put("public.utf8-plain-text", text.as_bytes()))
        }
    }
}
#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClipboardFormatW(name: *const u16) -> u32;
        fn OpenClipboard(owner: *mut c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut c_void;
        fn SetClipboardData(format: u32, handle: *mut c_void) -> *mut c_void;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, size: usize) -> *mut c_void;
        fn GlobalLock(handle: *mut c_void) -> *mut c_void;
        fn GlobalUnlock(handle: *mut c_void) -> i32;
        fn GlobalSize(handle: *mut c_void) -> usize;
        fn GlobalFree(handle: *mut c_void) -> *mut c_void;
    }
    struct Close;
    impl Drop for Close {
        fn drop(&mut self) {
            unsafe {
                CloseClipboard();
            }
        }
    }
    unsafe fn format() -> u32 {
        let name: Vec<u16> = "Art::GVML ClipFormat\0".encode_utf16().collect();
        unsafe { RegisterClipboardFormatW(name.as_ptr()) }
    }
    pub fn read() -> Option<Vec<u8>> {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }
            let _close = Close;
            let handle = GetClipboardData(format());
            if handle.is_null() {
                return None;
            }
            let len = GlobalSize(handle);
            let data = GlobalLock(handle);
            if data.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len).to_vec();
            GlobalUnlock(handle);
            Some(bytes)
        }
    }
    pub fn write(bytes: &[u8], text: Option<&str>) -> bool {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return false;
            }
            let _close = Close;
            let put = |format, bytes: &[u8]| -> bool {
                let handle = GlobalAlloc(2, bytes.len());
                if handle.is_null() {
                    return false;
                }
                let ptr = GlobalLock(handle);
                if ptr.is_null() {
                    GlobalFree(handle);
                    return false;
                }
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len());
                GlobalUnlock(handle);
                if SetClipboardData(format, handle).is_null() {
                    GlobalFree(handle);
                    false
                } else {
                    true
                }
            };
            let native = put(format(), bytes);
            native
                && text.is_none_or(|text| {
                    let utf16: Vec<u16> = text.encode_utf16().chain([0]).collect();
                    put(
                        13,
                        std::slice::from_raw_parts(utf16.as_ptr().cast::<u8>(), utf16.len() * 2),
                    )
                })
        }
    }
}
#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    pub fn read() -> Option<Vec<u8>> {
        None
    }
    pub fn write(_: &[u8], _: Option<&str>) -> bool {
        false
    }
}
#[cfg(feature = "scenarios")]
thread_local! {static SIMULATED:std::cell::RefCell<Option<Option<Vec<u8>>>>=const{std::cell::RefCell::new(None)};}
#[cfg(feature = "scenarios")]
pub(crate) fn simulate(bytes: Option<Vec<u8>>) {
    SIMULATED.with(|s| *s.borrow_mut() = Some(bytes));
}
pub(crate) fn read() -> Option<Vec<u8>> {
    #[cfg(feature = "scenarios")]
    if let Some(value) = SIMULATED.with(|s| s.borrow().clone()) {
        return value;
    }
    platform::read()
}
pub(crate) fn write(bytes: &[u8], text: Option<&str>) -> bool {
    #[cfg(feature = "scenarios")]
    if SIMULATED.with(|s| s.borrow().is_some()) {
        simulate(Some(bytes.to_vec()));
        return true;
    }
    platform::write(bytes, text)
}

use crate::SlideEditor;
use gpui_kit::*;
use ibis_doc_model::{
    Command, Fill, FlowCommand, FlowPos, FlowRunKind, MediaId, ShapeKind, TextBody,
};
impl SlideEditor {
    pub(crate) fn paste_office_shapes(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(bytes) = read() else {
            return false;
        };
        let paste_stamp=crate::shell::paste_placement::stamp(&bytes);
        let Ok(opened) = ibis_pptx::shape_clipboard::read_gvml(bytes) else {
            return false;
        };
        let foreign = opened.presentation;
        let Some(slide) = foreign.slides.first() else {
            return false;
        };
        if slide.shapes.is_empty() {
            return false;
        }
        let mut shapes = slide.shapes.clone();
        let mut map = std::collections::HashMap::new();
        for media in &foreign.media {
            let id = self
                .document
                .ids()
                .add_media(media.format, media.data.clone());
            self.resources.add_picture(id, media.format, &media.data);
            map.insert(media.id, id);
        }
        let remap = |id: &mut MediaId| {
            if let Some(new) = map.get(id) {
                *id = *new;
            }
        };
        let fill = |fill: &mut Fill| match fill {
            Fill::Image(id) | Fill::TileImage { media: id, .. } => remap(id),
            Fill::Blip(b) => remap(&mut b.media),
            _ => {}
        };
        let text = |body: &mut TextBody| {
            for p in &mut body.paragraphs {
                if let Some(id) = p.bullet.as_mut().and_then(|b| b.picture.as_mut()) {
                    remap(id);
                }
            }
        };
        let mut flow_shape_id = self.flow().map(|doc| doc.new_shape_id());
        for shape in &mut shapes {
            shape.id = if let Some(id) = flow_shape_id.as_mut() {
                let value = *id;
                *id += 1;
                ibis_doc_model::ShapeId(value)
            } else {
                self.document.ids().new_shape_id()
            };
            match &mut shape.kind {
                ShapeKind::Picture(p) => remap(&mut p.media),
                ShapeKind::Auto(a) => {
                    fill(&mut a.fill);
                    if let Some(body) = &mut a.text {
                        text(body);
                    }
                }
                ShapeKind::TextBox(body) => text(body),
                ShapeKind::Table(t) => {
                    for cell in t.rows.iter_mut().flat_map(|r| &mut r.cells) {
                        fill(&mut cell.fill);
                        text(&mut cell.text);
                    }
                }
                _ => {}
            }
        }
        if self.flow().is_some() {
            let Some((_, caret)) = self.text_selection() else {
                return false;
            };
            let multiple = shapes.len() > 1;
            let left = shapes.iter().map(|s| s.frame.x).min().unwrap_or(0);
            let top = shapes.iter().map(|s| s.frame.y).min().unwrap_or(0);
            let commands = shapes
                .into_iter()
                .enumerate()
                .map(|(i, mut shape)| {
                    shape.floating = multiple.then(|| ibis_doc_model::FloatingAnchor {
                        offset_x: shape.frame.x - left,
                        offset_y: shape.frame.y - top,
                        relative_h: "column".into(),
                        relative_v: "paragraph".into(),
                        wrap: "none".into(),
                        allow_overlap: true,
                        relative_height: i as u32,
                        ..Default::default()
                    });
                    FlowCommand::InsertField {
                        at: FlowPos::new(caret.para, caret.offset + i),
                        kind: FlowRunKind::Shape(Box::new(shape)),
                    }
                })
                .collect();
            return self.run_flow(commands, cx);
        }
        if self.is_workbook(){self.position_sheet_paste(&mut shapes,paste_stamp);}
        let start = self.slide().shapes.len();
        let owner = self.current_slide;
        let ids = shapes.iter().map(|s| s.id).collect();
        let commands = shapes
            .into_iter()
            .enumerate()
            .map(|(i, shape)| Command::InsertShape {
                slide: owner,
                index: start + i,
                shape,
            })
            .collect();
        self.run(commands, cx);
        self.set_selection(ids);
        cx.notify();
        true
    }
}

#[cfg(headless_export)]
pub(crate) fn image_fallback(
    presentation: &ibis_doc_model::Presentation,
    shapes: &[ibis_doc_model::Shape],
) -> Option<Image> {
    use crate::render::shape_view::{Resources, shape_element};
    use std::{rc::Rc, sync::Arc};
    struct Drawing {
        shapes: Vec<ibis_doc_model::Shape>,
        resources: Rc<Resources>,
        scale: f32,
    }
    impl Render for Drawing {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().relative().size_full().bg(white()).children(
                self.shapes
                    .iter()
                    .map(|s| shape_element(s, s.frame, self.scale, &self.resources, None)),
            )
        }
    }
    let mut shapes = shapes.to_vec();
    let bounds = ibis_pptx::shape_clipboard::drawing_bounds(&shapes)?;
    let (x, y, w, h) = (bounds.x, bounds.y, bounds.width, bounds.height);
    for shape in &mut shapes {
        shape.frame.x -= x;
        shape.frame.y -= y;
    }
    // 96 dpi in the model, 2x for an image fallback on high-density displays.
    let scale = (2.0f32 / 9525.).min(8192. / w.max(h) as f32);
    let platform = gpui_kit::platform::current_platform(false);
    let mut app = HeadlessAppContext::with_platform(
        platform.text_system(),
        Arc::new(crate::shell::icons::Assets::new("")),
        crate::export::headless_renderer,
    );
    app.allow_parking();
    app.update(crate::init);
    let resources = Rc::new(app.update(|cx| Resources::new(presentation, cx)));
    resources.preload.wait_all();
    let window = app
        .open_window(
            size(px(w as f32 * scale), px(h as f32 * scale)),
            move |_, cx| {
                cx.new(|_| Drawing {
                    shapes,
                    resources,
                    scale,
                })
            },
        )
        .ok()?;
    for _ in 0..3 {
        app.run_until_parked();
        app.update_window(window.into(), |_, window, _| window.refresh())
            .ok()?;
    }
    let rgba = app.capture_screenshot(window.into()).ok()?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut bytes, image::ImageFormat::Png).ok()?;
    Some(Image::from_bytes(ImageFormat::Png, bytes.into_inner()))
}
impl SlideEditor {
    pub(crate) fn copy_office_shapes(
        &mut self,
        shapes: &[ibis_doc_model::Shape],
        cx: &mut Context<Self>,
    ) -> bool {
        if shapes.is_empty() {
            return false;
        }
        let Ok(native) = ibis_pptx::shape_clipboard::write_gvml(self.presentation(), shapes) else {
            return false;
        };
        let text = shapes
            .iter()
            .filter_map(|s| s.text_body())
            .map(|b| b.text())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut entries = Vec::new();
        #[cfg(headless_export)]
        if let Some(image) = image_fallback(self.presentation(), shapes) {
            entries.push(ClipboardEntry::Image(image));
        }
        if !text.is_empty() {
            entries.extend(
                ClipboardItem::new_string(text.clone())
                    .entries()
                    .iter()
                    .cloned(),
            );
        }
        // GPUI's native macOS multi-entry writer keeps only strings. Write
        // the image first (DIB as well on Windows), then append text/native
        // formats without clearing that image representation.
        let simulated = {
            #[cfg(feature = "scenarios")]
            {
                SIMULATED.with(|s| s.borrow().is_some())
            }
            #[cfg(not(feature = "scenarios"))]
            {
                false
            }
        };
        if !simulated
            && cfg!(any(target_os = "macos", windows))
            && let Some(ClipboardEntry::Image(image)) = entries
                .iter()
                .find(|e| matches!(e, ClipboardEntry::Image(_)))
        {
            cx.write_to_clipboard(ClipboardItem::new_image(image));
        } else {
            cx.write_to_clipboard(ClipboardItem { entries });
        }
        let _ = write(&native, (!text.is_empty()).then_some(text.as_str()));
        true
    }
}

// Opt-in OS integration check; the caller saves/restores the user's pasteboard.
#[cfg(all(target_os = "macos", feature = "scenarios"))]
pub(crate) fn verify_system_formats(image: &Image, native: &[u8]) {
    let system = gpui_kit::platform::current_platform(false);
    system.write_to_clipboard(ClipboardItem::new_image(image));
    assert!(platform::write(native, Some("Office vector text")));
    for kind in [
        "public.png",
        "public.utf8-plain-text",
        "com.microsoft.Art--GVML-ClipFormat",
    ] {
        assert!(
            platform::has_data(kind),
            "missing pasteboard format: {kind}"
        );
    }
    assert_eq!(platform::read().as_deref(), Some(native));
}
