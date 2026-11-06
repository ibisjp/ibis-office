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
    fn data_of(kind: &str) -> Option<Vec<u8>> {
        unsafe {
            let board: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
            let data: *mut Object = msg_send![board, dataForType: string(kind)];
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
    /// A vector picture on the pasteboard: SVG (`com.microsoft.image-svg-xml`, which Office for Mac puts there for a copied
    /// drawing; `public.svg-image`, which other apps use), else a PDF (`com.adobe.pdf`: Office for Mac puts one for a drawing
    /// and a picture of cells, and takes one as a vector picture).
    pub fn read_vector_picture() -> Option<(&'static str, Vec<u8>)> {
        data_of("com.microsoft.image-svg-xml")
            .or_else(|| data_of("public.svg-image"))
            .map(|bytes| ("svg", bytes))
            .or_else(|| data_of("com.adobe.pdf").map(|bytes| ("pdf", bytes)))
    }
    /// Add the SVG (and the PDF, when there is one) of a picture to the pasteboard without removing what it holds (the PNG fallback).
    pub fn write_vector(svg: &[u8], pdf: Option<&[u8]>, _emf: Option<&[u8]>) -> bool {
        unsafe {
            let board: *mut Object = msg_send![class!(NSPasteboard), generalPasteboard];
            let put = |name: &str, bytes: &[u8]| -> bool {
                let kind = string(name);
                let types: *mut Object = msg_send![class!(NSArray), arrayWithObject: kind];
                let _: isize = msg_send![board, addTypes: types owner: std::ptr::null_mut::<Object>()];
                let data: *mut Object = msg_send![class!(NSData), dataWithBytes: bytes.as_ptr() length: bytes.len()];
                msg_send![board, setData: data forType: kind]
            };
            let svg_ok = put("public.svg-image", svg) & put("com.microsoft.image-svg-xml", svg);
            pdf.is_none_or(|pdf| put("com.adobe.pdf", pdf)) && svg_ok
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
    /// The bytes of one type of the pasteboard.
    #[cfg(feature = "scenarios")]
    pub fn data(name: &str) -> Option<Vec<u8>> {
        data_of(name)
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
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn GetEnhMetaFileBits(emf: *mut c_void, size: u32, buffer: *mut u8) -> u32;
        fn SetEnhMetaFileBits(size: u32, buffer: *const u8) -> *mut c_void;
        fn DeleteEnhMetaFile(emf: *mut c_void) -> i32;
    }
    /// The clipboard format of an Enhanced Metafile (`CF_ENHMETAFILE`).
    const CF_ENHMETAFILE: u32 = 14;
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
    /// A vector picture on the clipboard: SVG (the registered format `image/svg+xml`, as GPUI names it), else an
    /// Enhanced Metafile (`CF_ENHMETAFILE`, which Office, Illustrator and Inkscape put there for a drawing).
    pub fn read_vector_picture() -> Option<(&'static str, Vec<u8>)> {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return None;
            }
            let _close = Close;
            let name: Vec<u16> = "image/svg+xml\0".encode_utf16().collect();
            let svg = GetClipboardData(RegisterClipboardFormatW(name.as_ptr()));
            if !svg.is_null() {
                let len = GlobalSize(svg);
                let data = GlobalLock(svg);
                if !data.is_null() {
                    let mut bytes = std::slice::from_raw_parts(data.cast::<u8>(), len).to_vec();
                    GlobalUnlock(svg);
                    // A global block is rounded up; the text ends before the padding.
                    while bytes.last() == Some(&0) {
                        bytes.pop();
                    }
                    if !bytes.is_empty() {
                        return Some(("svg", bytes));
                    }
                }
            }
            let emf = GetClipboardData(CF_ENHMETAFILE);
            if emf.is_null() {
                return None;
            }
            let len = GetEnhMetaFileBits(emf, 0, std::ptr::null_mut());
            if len == 0 {
                return None;
            }
            let mut bytes = vec![0u8; len as usize];
            if GetEnhMetaFileBits(emf, len, bytes.as_mut_ptr()) != len {
                return None;
            }
            Some(("emf", bytes))
        }
    }
    /// Add an SVG next to the picture already on the clipboard.
    pub fn write_vector(svg: &[u8], _pdf: Option<&[u8]>, emf: Option<&[u8]>) -> bool {
        unsafe {
            if OpenClipboard(std::ptr::null_mut()) == 0 {
                return false;
            }
            let _close = Close;
            let name: Vec<u16> = "image/svg+xml\0".encode_utf16().collect();
            let format = RegisterClipboardFormatW(name.as_ptr());
            let handle = GlobalAlloc(2, svg.len());
            if handle.is_null() {
                return false;
            }
            let ptr = GlobalLock(handle);
            if ptr.is_null() {
                GlobalFree(handle);
                return false;
            }
            std::ptr::copy_nonoverlapping(svg.as_ptr(), ptr.cast::<u8>(), svg.len());
            GlobalUnlock(handle);
            if SetClipboardData(format, handle).is_null() {
                GlobalFree(handle);
                return false;
            }
            // The picture as an Enhanced Metafile: Office for Windows pastes it as a vector picture (the clipboard owns the
            // metafile once it is set).
            if let Some(emf) = emf {
                let metafile = SetEnhMetaFileBits(emf.len() as u32, emf.as_ptr());
                if !metafile.is_null() && SetClipboardData(CF_ENHMETAFILE, metafile).is_null() {
                    DeleteEnhMetaFile(metafile);
                }
            }
            true
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
    pub fn read_vector_picture() -> Option<(&'static str, Vec<u8>)> {
        None
    }
    pub fn write_vector(_: &[u8], _: Option<&[u8]>, _: Option<&[u8]>) -> bool {
        false
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

/// What a scenario's copy as a picture put on its fake clipboard: the SVG and the PDF beside the picture.
#[cfg(feature = "scenarios")]
thread_local! {static WRITTEN_VECTOR:std::cell::RefCell<Option<(String,Option<Vec<u8>>)>>=const{std::cell::RefCell::new(None)};}
/// The vector picture a scenario's fake clipboard holds for a paste (the formats a real clipboard names `svg` or `pdf`).
#[cfg(feature = "scenarios")]
thread_local! {static SIMULATED_VECTOR:std::cell::RefCell<Option<(&'static str,Vec<u8>)>>=const{std::cell::RefCell::new(None)};}
#[cfg(feature = "scenarios")]
pub(crate) fn simulate_vector(picture: Option<(&'static str, Vec<u8>)>) {
    SIMULATED_VECTOR.with(|s| *s.borrow_mut() = picture);
}
#[cfg(feature = "scenarios")]
pub(crate) fn written_vector() -> Option<(String, Option<Vec<u8>>)> {
    WRITTEN_VECTOR.with(|w| w.borrow().clone())
}

/// The SVG of a picture put on the clipboard beside its PNG (a copy as a picture: vector where the receiver reads it), and,
/// where the receiver takes one, its PDF (Office for Mac pastes the PDF of the pasteboard as a vector picture).
pub(crate) fn write_svg(svg: &str) -> bool {
    // The PDF only goes where it is taken (macOS).
    #[cfg(all(headless_export, target_os = "macos"))]
    let pdf = crate::svg_pdf::svg_to_pdf(svg);
    #[cfg(not(all(headless_export, target_os = "macos")))]
    let pdf: Option<Vec<u8>> = None;
    // The metafile only goes where it is taken (Windows).
    #[cfg(all(headless_export, windows))]
    let emf = crate::svg_emf::svg_to_emf(svg);
    #[cfg(not(all(headless_export, windows)))]
    let emf: Option<Vec<u8>> = None;
    #[cfg(feature = "scenarios")]
    if SIMULATED.with(|s| s.borrow().is_some()) {
        WRITTEN_VECTOR.with(|w| *w.borrow_mut() = Some((svg.to_string(), pdf)));
        return true;
    }
    platform::write_vector(svg.as_bytes(), pdf.as_deref(), emf.as_deref())
}

/// What the clipboard holds as a picture: the file name (with the extension of its real format) and the bytes.
/// A drawing (SVG, and on Windows an Enhanced Metafile) comes before the bitmap beside it; a bitmap is named by what
/// its bytes are, not by the format the clipboard said (a TIFF from the Mac pasteboard is not a PNG).
pub(crate) fn clipboard_picture(item: Option<&ClipboardItem>) -> Option<(String, Vec<u8>)> {
    // Cells copied from a worksheet come with a drawing of them as well as their text: the text is what is pasted.
    let has_text = item.is_some_and(|i| i.text().is_some_and(|t| !t.is_empty()));
    #[cfg(feature = "scenarios")]
    let native = if has_text { None } else if let Some(picture) = SIMULATED_VECTOR.with(|s| s.borrow().clone()) { Some(picture) } else if SIMULATED.with(|s| s.borrow().is_some()) { None } else { platform::read_vector_picture() };
    #[cfg(not(feature = "scenarios"))]
    let native = if has_text { None } else { platform::read_vector_picture() };
    if let Some((ext, bytes)) = native
        && let Some((ext, bytes)) = sniff_picture(&bytes, ext)
    {
        return Some((format!("clipboard.{ext}"), bytes));
    }
    item?.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) => {
            let hint = match image.format {
                ImageFormat::Jpeg => "jpg",
                ImageFormat::Gif => "gif",
                ImageFormat::Svg => "svg",
                ImageFormat::Bmp => "bmp",
                ImageFormat::Tiff => "tiff",
                ImageFormat::Webp => "webp",
                _ => "png",
            };
            sniff_picture(&image.bytes, hint).map(|(ext, bytes)| (format!("clipboard.{ext}"), bytes))
        }
        _ => None,
    })
}

/// The extension and bytes of a picture, by what the bytes are. A TIFF is turned into a PNG (documents hold no TIFF
/// that is drawn); a metafile is kept only if it draws something. `hint` is what the source called it, for bytes that
/// show nothing (an SVG without its `<svg` in the first KiB).
pub(crate) fn sniff_picture(bytes: &[u8], hint: &'static str) -> Option<(&'static str, Vec<u8>)> {
    let ext = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "jpg"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "gif"
    } else if bytes.starts_with(b"BM") {
        "bmp"
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        "tiff"
    } else if bytes.len() >= 44 && bytes[..4] == [1, 0, 0, 0] && bytes[40..44] == *b" EMF" {
        "emf"
    } else if bytes.starts_with(&[0xd7, 0xcd, 0xc6, 0x9a]) || bytes.starts_with(&[1, 0, 9, 0]) || bytes.starts_with(&[2, 0, 9, 0]) {
        "wmf"
    } else if bytes.starts_with(b"%PDF") {
        "pdf"
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes[8..12] == *b"WEBP" {
        "webp"
    } else {
        hint
    };
    match ext {
        "tiff" => tiff_as_png(bytes).map(|png| ("png", png)),
        // A screenshot is a DIB, which documents and the renderers read differently (bit fields, alpha, top-down):
        // it is kept as a PNG of its pixels. A bitmap that is not decoded here stays as it is.
        "bmp" => Some(bmp_as_png(bytes).map_or(("bmp", bytes.to_vec()), |png| ("png", png))),
        // A PDF (what Office for Mac puts on the pasteboard for a drawing) is read into the SVG of its first page; one that shows
        // nothing here is left for the bitmap beside it.
        #[cfg(headless_export)]
        "pdf" => crate::pdf_picture::pdf_to_svg(bytes).map(|svg| ("svg", svg.into_bytes())),
        "emf" | "wmf" => {
            let format = if ext == "emf" { ibis_doc_model::MediaFormat::Emf } else { ibis_doc_model::MediaFormat::Wmf };
            crate::pptx::editing::validated_picture_size(format, bytes).map(|_| (ext, bytes.to_vec()))
        }
        _ => Some((ext, bytes.to_vec())),
    }
}

/// The PNG of a BMP's pixels (see `dib_picture`).
#[cfg(headless_export)]
pub(crate) fn bmp_as_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let (width, height, rgba) = crate::dib_picture::bmp_to_rgba(bytes)?;
    let pixels = image::RgbaImage::from_raw(width, height, rgba)?;
    let mut png = std::io::Cursor::new(Vec::new());
    pixels.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

#[cfg(not(headless_export))]
pub(crate) fn bmp_as_png(_: &[u8]) -> Option<Vec<u8>> {
    None
}

#[cfg(headless_export)]
fn tiff_as_png(bytes: &[u8]) -> Option<Vec<u8>> {
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Tiff).ok()?;
    let mut png = std::io::Cursor::new(Vec::new());
    decoded.write_to(&mut png, image::ImageFormat::Png).ok()?;
    Some(png.into_inner())
}

#[cfg(not(headless_export))]
fn tiff_as_png(_: &[u8]) -> Option<Vec<u8>> {
    None
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
        let Ok(opened) = ibis_pptx::shape_clipboard::read_gvml(bytes.clone()) else {
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
        // 縦中横 stays in the document it was copied in; anywhere else the text has none.
        let side = TATE_SIDE.with(|t| t.borrow().clone()).filter(|(b, _)| self.flow().is_some() && *b == bytes);
        for (i, shape) in shapes.iter_mut().enumerate() {
            let from = side.as_ref().and_then(|(_, s)| s.get(i));
            match &mut shape.kind {
                ShapeKind::Auto(a) => {
                    if let Some(body) = &mut a.text {
                        carry_tate_chu_yoko(body, from.and_then(|f| f.text_body()));
                    }
                }
                ShapeKind::TextBox(body) => carry_tate_chu_yoko(body, from.and_then(|f| f.text_body())),
                _ => {}
            }
        }
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

/// The PNG of an SVG drawn at `scale` pixels per SVG unit by GPUI's SVG renderer (the drawing of the screen), on a transparent
/// background (what the SVG does not paint stays clear, as in Excel's copy as picture): the bitmap that goes on the clipboard
/// beside the SVG of a copy as a picture.
#[cfg(headless_export)]
pub(crate) fn svg_as_png(svg: &str, width: f32, height: f32, scale: f32) -> Option<Image> {
    let scale = scale.min(8192. / width.max(height).max(1.));
    let renderer = SvgRenderer::new(std::sync::Arc::new(()));
    let parsed = renderer.parse_svg(svg.as_bytes()).ok()?;
    let (w, h) = ((width * scale).round().max(1.) as i32, (height * scale).round().max(1.) as i32);
    let raster = renderer.render_parsed(&parsed, SvgSize::ExactSize(gpui_kit::size(DevicePixels(w), DevicePixels(h)))).ok()?;
    let mut pixels = raster.as_bytes(0)?.to_vec();
    // BGRA with premultiplied alpha to straight RGBA.
    for p in pixels.chunks_exact_mut(4) {
        let a = u32::from(p[3]);
        let un = |v: u8| if a == 0 { 0 } else { ((u32::from(v) * 255 + a / 2) / a).min(255) as u8 };
        let (b, g, r) = (un(p[0]), un(p[1]), un(p[2]));
        p[0] = r;
        p[1] = g;
        p[2] = b;
    }
    let size = raster.size(0);
    let rgba = image::RgbaImage::from_raw(size.width.0 as u32, size.height.0 as u32, pixels)?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut bytes, image::ImageFormat::Png).ok()?;
    Some(Image::from_bytes(ImageFormat::Png, bytes.into_inner()))
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
thread_local! {
    /// The shapes of the last copy in a document, with the native bytes that went to the clipboard: 縦中横 (Word's, `w:eastAsianLayout`)
    /// is not in those bytes, so a paste into a document that finds the same bytes takes it back from here. Pasted anywhere else
    /// (a presentation, a workbook, another program) the text has none.
    static TATE_SIDE: std::cell::RefCell<Option<(Vec<u8>, Vec<ibis_doc_model::Shape>)>> = const { std::cell::RefCell::new(None) };
}

/// Gives the runs of `body` the 縦中横 the runs of `from` have (same paragraphs, same texts), or takes it away (`None`).
fn carry_tate_chu_yoko(body: &mut TextBody, from: Option<&TextBody>) {
    for (i, p) in body.paragraphs.iter_mut().enumerate() {
        let source = from.and_then(|b| b.paragraphs.get(i));
        for (k, r) in p.runs.iter_mut().enumerate() {
            r.style.tate_chu_yoko = source.and_then(|s| s.runs.get(k)).filter(|s| s.text == r.text).and_then(|s| s.style.tate_chu_yoko);
        }
    }
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
        if self.flow().is_some() {
            TATE_SIDE.with(|t| *t.borrow_mut() = Some((native.clone(), shapes.to_vec())));
        }
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
