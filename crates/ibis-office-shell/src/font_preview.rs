// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Cache and rendering of font family previews for the font selection dropdown.
//!
//! * Renders the font's name in its own typeface offscreen.
//! * On macOS, briefly loads the font via CoreText/CoreGraphics, renders it to a bitmap,
//!   converts it to a BMP image, and immediately drops the OS font reference (`CTFont`).
//!   This ensures that even with 1000 fonts on the system, OS font handles and memory are
//!   not kept open.
//! * The rendered BMP image is cached in main memory (`FontPreviewCache`) as an `Arc<gpui::Image>`.
//! * When scrolling or returning to previously viewed fonts, the cached BMP is reused directly
//!   without querying the OS.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui_kit::Image;
#[cfg(target_os = "macos")]
use gpui_kit::ImageFormat;

#[derive(Clone)]
pub(crate) struct FontPreview {
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
}

struct FontPreviewCache {
    entries: HashMap<String, FontPreview>,
}

static CACHE: OnceLock<Mutex<FontPreviewCache>> = OnceLock::new();

fn cache() -> &'static Mutex<FontPreviewCache> {
    CACHE.get_or_init(|| Mutex::new(FontPreviewCache { entries: HashMap::new() }))
}

/// Retrieves the cached preview image for `font_name` if already rendered.
pub(crate) fn get_preview(font_name: &str) -> Option<FontPreview> {
    cache().lock().ok()?.entries.get(font_name).cloned()
}

/// Retrieves or synchronously renders and caches the preview image for `font_name`.
/// OS font references are immediately dropped upon rendering.
pub(crate) fn get_or_render_preview(font_name: &str, display_name: &str) -> Option<FontPreview> {
    // The glyphs are baked in the chrome's text colour of the appearance now (dark text on light, light text on dark), so the
    // cache is per appearance: a bitmap drawn for the light chrome is unreadable on the dark one (#114).
    let text = text_color();
    let cache_key = format!("{font_name}\t{display_name}\t{:02x}{:02x}{:02x}", text[0], text[1], text[2]);
    if let Some(existing) = get_preview(&cache_key) {
        return Some(existing);
    }

    #[cfg(target_os = "macos")]
    {
        if let Some((bmp_bytes, width, height)) = render_preview_bmp_macos(font_name, display_name, text) {
            let preview = FontPreview {
                image: Arc::new(Image::from_bytes(ImageFormat::Bmp, bmp_bytes)),
                width,
                height,
            };
            if let Ok(mut c) = cache().lock() {
                c.entries.insert(cache_key, preview.clone());
            }
            return Some(preview);
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = (font_name, display_name, text);
    }

    None
}

/// The colour of the chrome's text now (`TEXT` of the chrome, turned for the dark appearance), as `[r, g, b]`.
fn text_color() -> [u8; 3] {
    let c = crate::palette::rgb(crate::chrome::TEXT);
    [(c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8]
}

#[cfg(target_os = "macos")]
fn render_preview_bmp_macos(font_name: &str, display_name: &str, text: [u8; 3]) -> Option<(Vec<u8>, f32, f32)> {
    use core_foundation::attributed_string::CFMutableAttributedString;
    use core_foundation::base::{CFRange, TCFType};
    use core_foundation::string::CFString;
    use core_graphics::color_space::CGColorSpace;
    use core_graphics::context::CGContext;
    use core_text::line::CTLine;
    use core_text::string_attributes::kCTFontAttributeName;

    const SCALE: f64 = 2.0;
    const LOGICAL_HEIGHT: f32 = 18.0;

    // 1. Temporarily load the OS font handle at 2x Retina resolution.
    let ct_font = core_text::font::new_from_name(font_name, 13.5 * SCALE).ok()?;

    // 2. Prepare attributed string with the font attribute.
    let mut string = CFMutableAttributedString::new();
    let cf = CFString::new(display_name);
    string.replace_str(&cf, CFRange::init(0, 0));
    let length = string.char_len();
    if length == 0 {
        return None;
    }
    string.set_attribute(CFRange::init(0, length), unsafe { kCTFontAttributeName }, &ct_font);

    // 3. Create line and measure typographical bounds.
    let line = CTLine::new_with_attributed_string(string.as_concrete_TypeRef());
    let bounds = line.get_typographic_bounds();
    if bounds.width <= 0.0 {
        return None;
    }
    let height = (LOGICAL_HEIGHT as f64 * SCALE).round() as usize;
    let width = (bounds.width.ceil() as usize + (4.0 * SCALE) as usize).clamp(4, 440);
    let logical_width = width as f32 / SCALE as f32;

    // 4. Render into CoreGraphics bitmap context (transparent background).
    let mut buffer = vec![0u8; width * height * 4];
    {
        let space = CGColorSpace::create_device_rgb();
        // kCGImageAlphaPremultipliedLast = 1
        let context = CGContext::create_bitmap_context(
            Some(buffer.as_mut_ptr() as *mut std::ffi::c_void),
            width,
            height,
            8,
            width * 4,
            &space,
            1,
        );
        let text_h = bounds.ascent + bounds.descent;
        let baseline_y = ((height as f64 - text_h) * 0.5 + bounds.descent).max(bounds.descent);
        context.set_text_position(2.0 * SCALE, baseline_y);
        line.draw(&context);
    }

    // 5. Explicitly drop all CoreText / OS font handles immediately so OS font references are cut.
    drop(line);
    drop(string);
    drop(ct_font);

    // 6. Convert CoreText glyph coverage into straight-alpha UI text color (`text`)
    // with transparent background (alpha = 0).
    for chunk in buffer.chunks_exact_mut(4) {
        let a = chunk[3];
        if a == 0 {
            chunk[0] = text[0];
            chunk[1] = text[1];
            chunk[2] = text[2];
            chunk[3] = 0;
        } else if chunk[0] == 0 && chunk[1] == 0 && chunk[2] == 0 {
            chunk[0] = text[0];
            chunk[1] = text[1];
            chunk[2] = text[2];
            chunk[3] = a;
        } else {
            let a_u16 = a as u16;
            chunk[0] = ((chunk[0] as u16 * 255 + a_u16 / 2) / a_u16).min(255) as u8;
            chunk[1] = ((chunk[1] as u16 * 255 + a_u16 / 2) / a_u16).min(255) as u8;
            chunk[2] = ((chunk[2] as u16 * 255 + a_u16 / 2) / a_u16).min(255) as u8;
            chunk[3] = a;
        }
    }

    // 7. Encode buffer into 32-bit BMP format with alpha channel (BITMAPV4HEADER, top-down BGRA).
    Some((rgba_to_bmp(width, height, &buffer), logical_width, LOGICAL_HEIGHT))
}

/// Encodes an RGBA byte slice (top-down row order) into a 32-bit BMP with an explicit alpha channel
/// (`BITMAPV4HEADER` with `BI_BITFIELDS` masks so `image`'s BMP decoder preserves transparency).
#[cfg(any(target_os = "macos", test))]
fn rgba_to_bmp(width: usize, height: usize, rgba: &[u8]) -> Vec<u8> {
    const FILE_HEADER_SIZE: usize = 14;
    const DIB_HEADER_SIZE: usize = 108; // BITMAPV4HEADER
    const PIXEL_OFFSET: usize = FILE_HEADER_SIZE + DIB_HEADER_SIZE;

    let row_size = width * 4;
    let image_size = row_size * height;
    let file_size = PIXEL_OFFSET + image_size;
    let mut bmp = Vec::with_capacity(file_size);

    // BMP File Header (14 bytes)
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes()); // Reserved
    bmp.extend_from_slice(&(PIXEL_OFFSET as u32).to_le_bytes()); // Offset to pixel data

    // DIB Header (BITMAPV4HEADER, 108 bytes)
    bmp.extend_from_slice(&(DIB_HEADER_SIZE as u32).to_le_bytes()); // bV4Size
    bmp.extend_from_slice(&(width as i32).to_le_bytes()); // bV4Width
    bmp.extend_from_slice(&(-(height as i32)).to_le_bytes()); // bV4Height (negative = top-down)
    bmp.extend_from_slice(&1u16.to_le_bytes()); // bV4Planes
    bmp.extend_from_slice(&32u16.to_le_bytes()); // bV4BitCount (32bpp)
    bmp.extend_from_slice(&3u32.to_le_bytes()); // bV4V4Compression (BI_BITFIELDS = 3)
    bmp.extend_from_slice(&(image_size as u32).to_le_bytes()); // bV4SizeImage
    bmp.extend_from_slice(&2835i32.to_le_bytes()); // bV4XPelsPerMeter (~72 DPI)
    bmp.extend_from_slice(&2835i32.to_le_bytes()); // bV4YPelsPerMeter (~72 DPI)
    bmp.extend_from_slice(&0u32.to_le_bytes()); // bV4ClrUsed
    bmp.extend_from_slice(&0u32.to_le_bytes()); // bV4ClrImportant
    bmp.extend_from_slice(&0x00FF_0000u32.to_le_bytes()); // bV4RedMask
    bmp.extend_from_slice(&0x0000_FF00u32.to_le_bytes()); // bV4GreenMask
    bmp.extend_from_slice(&0x0000_00FFu32.to_le_bytes()); // bV4BlueMask
    bmp.extend_from_slice(&0xFF00_0000u32.to_le_bytes()); // bV4AlphaMask
    bmp.extend_from_slice(&0x7352_4742u32.to_le_bytes()); // bV4CSType ('sRGB')
    bmp.extend_from_slice(&[0u8; 36]); // bV4Endpoints
    bmp.extend_from_slice(&0u32.to_le_bytes()); // bV4GammaRed
    bmp.extend_from_slice(&0u32.to_le_bytes()); // bV4GammaGreen
    bmp.extend_from_slice(&0u32.to_le_bytes()); // bV4GammaBlue

    // Pixel data: RGBA -> BGRA
    for chunk in rgba.chunks_exact(4) {
        let r = chunk[0];
        let g = chunk[1];
        let b = chunk[2];
        let a = chunk[3];
        bmp.push(b);
        bmp.push(g);
        bmp.push(r);
        bmp.push(a);
    }

    bmp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_to_bmp_preserves_alpha_and_top_down_order() {
        // 2x1 image: pixel 0 is transparent background, pixel 1 is dark text (#1f1f1f, opaque).
        let rgba = vec![
            0x1f, 0x1f, 0x1f, 0,   // transparent pixel
            0x1f, 0x1f, 0x1f, 255, // opaque dark text pixel
        ];
        let bmp = rgba_to_bmp(2, 1, &rgba);
        assert_eq!(&bmp[0..2], b"BM");
        assert_eq!(bmp.len(), 122 + 8);
        assert_eq!(&bmp[122..126], &[0x1f, 0x1f, 0x1f, 0]);
        assert_eq!(&bmp[126..130], &[0x1f, 0x1f, 0x1f, 255]);

        let decoded = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp)
            .expect("BMP should decode via image crate")
            .into_rgba8();
        assert_eq!(decoded.dimensions(), (2, 1));
        assert_eq!(decoded.get_pixel(0, 0).0, [0x1f, 0x1f, 0x1f, 0], "background pixel must stay transparent (alpha=0)");
        assert_eq!(decoded.get_pixel(1, 0).0, [0x1f, 0x1f, 0x1f, 255], "foreground pixel must be #1f1f1f opaque");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_font_preview_has_transparent_background_and_visible_glyphs() {
        let (bmp, w, h) = render_preview_bmp_macos("Helvetica", "Helvetica", [0x1f, 0x1f, 0x1f]).expect("renders Helvetica");
        assert!(w > 10.0 && h == 18.0);
        let decoded = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp)
            .expect("decodes rendered font BMP")
            .into_rgba8();
        let total_pixels = (decoded.width() * decoded.height()) as usize;
        let transparent_pixels = decoded.pixels().filter(|p| p.0[3] == 0).count();
        let glyph_pixels = decoded.pixels().filter(|p| p.0[3] > 0 && p.0[0] == 0x1f).count();
        assert!(transparent_pixels > total_pixels / 2, "majority of preview bitmap must be transparent background, got {transparent_pixels}/{total_pixels}");
        assert!(glyph_pixels > 20, "preview bitmap must contain visible dark glyph pixels, got {glyph_pixels}");
    }
}
