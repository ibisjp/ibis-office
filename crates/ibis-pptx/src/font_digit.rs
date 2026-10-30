// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The widest digit of a font, in pixels at a device resolution.
//!
//! Excel for Windows counts a column's width in characters of its Normal font's widest digit
//! (`MDW`, ECMA-376 Part 1 18.3.1.13) and measures that digit through GDI (`GetCharWidth32` of
//! `0`..`9`) on the device it lays out for: whole pixels at 96 dpi on the screen, at the printer's
//! resolution (600 dpi for "Microsoft Print to PDF") when it prints. The font is made `round(pt × dpi /
//! 72)` pixels high, as `MulDiv(pt, dpi, 72)` does, and GDI's hinting decides the width: Calibri 11 pt
//! is 7 px on the screen though 0.5068 em × 15 px is 7.6. So it is asked of GDI, not worked out.
//!
//! Where GDI is not there (macOS, Linux), `None`: the caller falls back on half an em, a digit's width in the
//! fonts Office's defaults come in. A font this machine does not have is measured as Excel does, in whatever GDI's
//! font mapper gives for it.

use std::cell::RefCell;
use std::collections::HashMap;

/// The widest of the digits `0`..`9` of `font` at `pt` points, in pixels of a `dpi` device; `None`
/// when it cannot be measured (no GDI).
pub fn max_digit_px(font: &str, pt: f64, dpi: f64) -> Option<u32> {
    thread_local! {
        static CACHE: RefCell<HashMap<(String, u64, u64), Option<u32>>> = RefCell::new(HashMap::new());
    }
    let key = (font.to_string(), pt.to_bits(), dpi.to_bits());
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).copied()) {
        return hit;
    }
    let measured = measure(font, pt, dpi);
    CACHE.with(|c| c.borrow_mut().insert(key, measured));
    measured
}

#[cfg(windows)]
fn measure(font: &str, pt: f64, dpi: f64) -> Option<u32> {
    use std::ffi::c_void;
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateFontW(height: i32, width: i32, escapement: i32, orientation: i32, weight: i32, italic: u32, underline: u32, strike_out: u32, charset: u32, out_precision: u32, clip_precision: u32, quality: u32, pitch_and_family: u32, face: *const u16) -> *mut c_void;
        fn CreateCompatibleDC(dc: *mut c_void) -> *mut c_void;
        fn SelectObject(dc: *mut c_void, object: *mut c_void) -> *mut c_void;
        fn GetCharWidth32W(dc: *mut c_void, first: u32, last: u32, widths: *mut i32) -> i32;
        fn DeleteObject(object: *mut c_void) -> i32;
        fn DeleteDC(dc: *mut c_void) -> i32;
    }
    if font.is_empty() || pt <= 0.0 || dpi <= 0.0 {
        return None;
    }
    let name: Vec<u16> = font.encode_utf16().chain(std::iter::once(0)).collect();
    if name.len() > 32 {
        return None;
    }
    let height = -((pt * dpi / 72.0).round() as i32);
    // SAFETY: plain GDI calls on objects made and freed here.
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let object = CreateFontW(height, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, name.as_ptr());
        let mut result = None;
        if !object.is_null() {
            let old = SelectObject(dc, object);
            // (A font this machine does not have is the one GDI's font mapper gives instead, as it is for Excel:
            // the name of a generic Mincho or Gothic, a font the file's author had.)
            let mut widths = [0i32; 10];
            if GetCharWidth32W(dc, 0x30, 0x39, widths.as_mut_ptr()) != 0 {
                result = widths.iter().copied().max().filter(|w| *w > 0).map(|w| w as u32);
            }
            SelectObject(dc, old);
            DeleteObject(object);
        }
        DeleteDC(dc);
        result
    }
}

/// The width of `text` in `font` at `pt` points, in points, as GDI sets it at the printer's resolution; `None`
/// when it cannot be measured. (`bold` is the font's weight 700.)
pub fn text_width_pt(font: &str, pt: f64, bold: bool, text: &str) -> Option<f64> {
    measure_text(font, pt, bold, text)
}

#[cfg(windows)]
fn measure_text(font: &str, pt: f64, bold: bool, text: &str) -> Option<f64> {
    use std::ffi::c_void;
    #[repr(C)]
    struct Size {
        cx: i32,
        cy: i32,
    }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateFontW(height: i32, width: i32, escapement: i32, orientation: i32, weight: i32, italic: u32, underline: u32, strike_out: u32, charset: u32, out_precision: u32, clip_precision: u32, quality: u32, pitch_and_family: u32, face: *const u16) -> *mut c_void;
        fn CreateCompatibleDC(dc: *mut c_void) -> *mut c_void;
        fn SelectObject(dc: *mut c_void, object: *mut c_void) -> *mut c_void;
        fn GetTextExtentPoint32W(dc: *mut c_void, text: *const u16, count: i32, size: *mut Size) -> i32;
        fn DeleteObject(object: *mut c_void) -> i32;
        fn DeleteDC(dc: *mut c_void) -> i32;
    }
    const DPI: f64 = 600.0;
    let name: Vec<u16> = font.encode_utf16().chain(std::iter::once(0)).collect();
    if font.is_empty() || pt <= 0.0 || name.len() > 32 {
        return None;
    }
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return Some(0.0);
    }
    let height = -((pt * DPI / 72.0).round() as i32);
    // SAFETY: plain GDI calls on objects made and freed here.
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let object = CreateFontW(height, 0, 0, 0, if bold { 700 } else { 400 }, 0, 0, 0, 1, 0, 0, 0, 0, name.as_ptr());
        let mut result = None;
        if !object.is_null() {
            let old = SelectObject(dc, object);
            let mut size = Size { cx: 0, cy: 0 };
            if GetTextExtentPoint32W(dc, wide.as_ptr(), wide.len() as i32, &mut size) != 0 {
                result = Some(size.cx as f64 * 72.0 / DPI);
            }
            SelectObject(dc, old);
            DeleteObject(object);
        }
        DeleteDC(dc);
        result
    }
}

/// A character's advance in `font` at `pt` points, in points, as Word lays text out: whole pixels of the
/// printer's resolution (600 dpi), the advance GDI gives a font made `round(pt × 600 / 72)` pixels high
/// (`GetCharWidth32`: hinted, not the outline's). Measured on Word's PDFs: a letter's advance is a whole number
/// of 600 dpi pixels (the mean distance from one, over 286 letters of a Times New Roman text, 0.04 pixel against
/// 0.25 for a resolution it is not). `None` where GDI is not there, or for a character it cannot measure.
pub fn char_advance_pt(font: &str, pt: f64, bold: bool, italic: bool, c: char) -> Option<f64> {
    thread_local! {
        static CACHE: RefCell<HashMap<(String, u64, bool, bool, char), Option<f64>>> = RefCell::new(HashMap::new());
    }
    let key = (font.to_string(), pt.to_bits(), bold, italic, c);
    if let Some(hit) = CACHE.with(|cache| cache.borrow().get(&key).copied()) {
        return hit;
    }
    // A measure that failed is asked once more and not remembered (GDI can fail for a moment); only an answer is kept.
    let measured = measure_char(font, pt, bold, italic, c).or_else(|| measure_char(font, pt, bold, italic, c));
    if measured.is_some() {
        CACHE.with(|cache| cache.borrow_mut().insert(key, measured));
    }
    measured
}

#[cfg(windows)]
fn measure_char(font: &str, pt: f64, bold: bool, italic: bool, c: char) -> Option<f64> {
    use std::ffi::c_void;
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateFontW(height: i32, width: i32, escapement: i32, orientation: i32, weight: i32, italic: u32, underline: u32, strike_out: u32, charset: u32, out_precision: u32, clip_precision: u32, quality: u32, pitch_and_family: u32, face: *const u16) -> *mut c_void;
        fn CreateCompatibleDC(dc: *mut c_void) -> *mut c_void;
        fn SelectObject(dc: *mut c_void, object: *mut c_void) -> *mut c_void;
        fn GetCharWidth32W(dc: *mut c_void, first: u32, last: u32, widths: *mut i32) -> i32;
        fn DeleteObject(object: *mut c_void) -> i32;
        fn DeleteDC(dc: *mut c_void) -> i32;
    }
    const DPI: f64 = 600.0;
    let name: Vec<u16> = font.encode_utf16().chain(std::iter::once(0)).collect();
    if font.is_empty() || pt <= 0.0 || name.len() > 32 || (c as u32) > 0xFFFF {
        return None;
    }
    let height = -((pt * DPI / 72.0).round() as i32);
    // SAFETY: plain GDI calls on objects made and freed here.
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let object = CreateFontW(height, 0, 0, 0, if bold { 700 } else { 400 }, italic as u32, 0, 0, 1, 0, 0, 0, 0, name.as_ptr());
        let mut result = None;
        if !object.is_null() {
            let old = SelectObject(dc, object);
            let mut width = 0i32;
            if GetCharWidth32W(dc, c as u32, c as u32, &mut width) != 0 {
                result = Some(width as f64 * 72.0 / DPI);
            }
            SelectObject(dc, old);
            DeleteObject(object);
        }
        DeleteDC(dc);
        result
    }
}

#[cfg(not(windows))]
fn measure_char(_font: &str, _pt: f64, _bold: bool, _italic: bool, _c: char) -> Option<f64> {
    None
}

#[cfg(not(windows))]
fn measure_text(_font: &str, _pt: f64, _bold: bool, _text: &str) -> Option<f64> {
    None
}

#[cfg(not(windows))]
fn measure(font: &str, pt: f64, dpi: f64) -> Option<u32> {
    if font.is_empty() || pt <= 0.0 || dpi <= 0.0 || font.encode_utf16().count() > 31 {
        return None;
    }
    let ppem = (pt * dpi / 72.0).round();
    if ppem <= 0.0 {
        return None;
    }
    // Digit advances (advanceWidth / unitsPerEm in hmtx) of the Windows/Office fonts,
    // with hdmx screen-pixel flooring for Calibri and Meiryo where GDI hints digits down at 96 dpi.
    let n = font.trim();
    let lower = n.to_ascii_lowercase();
    let (adv_em, hdmx_floor) = if n.starts_with("ＭＳ") || lower.starts_with("ms ") || n.starts_with("游明朝") || lower.starts_with("yu mincho") || lower.starts_with("yumincho") || lower == "times new roman" || lower == "times" { // i18n-ok（フォント名の照合）
        (128.0 / 256.0, false)
    } else if n.starts_with("游ゴシック") || lower.starts_with("yu gothic") || lower.starts_with("yugothic") { // i18n-ok（フォント名の照合）
        (1132.0 / 2048.0, false)
    } else if n.starts_with("メイリオ") || lower.starts_with("meiryo") { // i18n-ok（フォント名の照合）
        (1282.0 / 2048.0, true)
    } else if lower.starts_with("calibri") || lower.starts_with("carlito") {
        (1038.0 / 2048.0, true)
    } else if lower == "arial" || lower == "helvetica" || lower == "liberation sans" {
        (1139.0 / 2048.0, false)
    } else if lower.starts_with("courier") || lower.starts_with("lucida console") {
        (1229.0 / 2048.0, false)
    } else if lower == "tahoma" {
        (1118.0 / 2048.0, false)
    } else if lower == "verdana" {
        (1303.0 / 2048.0, false)
    } else if lower.starts_with("noto sans") {
        (0.555, false)
    } else {
        (0.5, false)
    };
    let raw = ppem * adv_em;
    let px = if hdmx_floor && dpi <= 120.0 { raw.floor() } else { raw.round() };
    Some((px as u32).max(1))
}

#[cfg(all(test, windows))]
mod tests {
    use super::{char_advance_pt, max_digit_px};

    /// Whole pixels of 600 dpi, for the faces, sizes, weights and slants Word's documents come in.
    #[test]
    fn char_advances_are_whole_600_dpi_pixels() {
        let mut measured = 0;
        for font in ["Times New Roman", "Century", "Arial", "Calibri", "Courier New", "Verdana", "ＭＳ 明朝", "ＭＳ ゴシック"] {
            for pt in [8.0, 9.0, 10.0, 10.5, 10.56, 11.0, 12.0, 14.0, 28.0] {
                for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                    for c in "AWaiMmw .,:;0123456789()-/".chars() {
                        let Some(w) = char_advance_pt(font, pt, bold, italic, c) else { continue };
                        let px = w * 600.0 / 72.0;
                        assert!((px - px.round()).abs() < 1e-6, "{font} {pt} bold {bold} italic {italic} {c:?}: {px}");
                        assert!(px >= 0.0);
                        measured += 1;
                    }
                }
            }
        }
        assert!(measured > 3000, "{measured}");
    }

    /// A bigger size is not narrower for the same letter (the font is made `round(pt × 600 / 72)` pixels high).
    #[test]
    fn char_advance_grows_with_the_size() {
        for font in ["Times New Roman", "Arial", "Century"] {
            for c in "AWaim0 ".chars() {
                let widths: Vec<f64> = [8.0, 10.0, 12.0, 16.0, 24.0].iter().filter_map(|pt| char_advance_pt(font, *pt, false, false, c)).collect();
                assert!(widths.windows(2).all(|w| w[0] <= w[1]), "{font} {c:?}: {widths:?}");
            }
        }
    }

    /// The fixed-width faces set a half-width letter half an em (to the pixel), as ＭＳ 明朝 does on Word's page.
    #[test]
    fn fixed_width_faces_are_half_an_em() {
        for font in ["ＭＳ 明朝", "ＭＳ ゴシック"] {
            let Some(a) = char_advance_pt(font, 10.5, false, false, 'a') else { continue };
            let Some(w) = char_advance_pt(font, 10.5, false, false, 'W') else { continue };
            assert_eq!(a, w, "{font}: one width for all letters");
            let em_px = 10.5f64 * 600.0 / 72.0;
            assert!((a * 600.0 / 72.0 - em_px / 2.0).abs() <= 8.0, "{font}: {a} pt against half of {em_px} px");
        }
    }

    /// A monospaced face (Courier New) has one width for every letter, 0.6 em to the pixel.
    #[test]
    fn monospaced_faces_have_one_width() {
        let Some(a) = char_advance_pt("Courier New", 10.0, false, false, 'a') else { return };
        for c in "AWi 0.-".chars() {
            assert_eq!(char_advance_pt("Courier New", 10.0, false, false, c), Some(a), "{c:?}");
        }
        assert!((a * 600.0 / 72.0 - 10.0 * 600.0 / 72.0 * 0.6).abs() <= 1.0, "{a}");
    }

    /// What it does not measure: no name, a name GDI cannot take (longer than 31), a character outside the BMP, a size that is not
    /// positive. A name that is no face on this machine is the font mapper's, as it is for Word.
    #[test]
    fn char_advance_falls_back_where_it_cannot_measure() {
        assert_eq!(char_advance_pt("", 10.5, false, false, 'a'), None);
        assert_eq!(char_advance_pt(&"x".repeat(40), 10.5, false, false, 'a'), None);
        assert_eq!(char_advance_pt("Times New Roman", 10.5, false, false, '\u{1F600}'), None);
        assert_eq!(char_advance_pt("Times New Roman", 0.0, false, false, 'a'), None);
        assert_eq!(char_advance_pt("Times New Roman", -3.0, false, false, 'a'), None);
        let unknown = char_advance_pt("No Such Face Anywhere", 10.5, false, false, 'a');
        assert!(unknown.is_some_and(|w| w > 0.0), "{unknown:?}");
    }

    /// The same question twice (the cache) is the same answer.
    #[test]
    fn char_advance_is_cached_and_stable() {
        let a = char_advance_pt("Arial", 11.0, false, false, 'g');
        assert!(a.is_some());
        assert_eq!(a, char_advance_pt("Arial", 11.0, false, false, 'g'));
    }

    /// Excel 365's `Columns(1).Width` for a column stored 10 characters wide (80 px = 60 pt, or 70 px =
    /// 52.5 pt) and its PDF's column widths (the printer's digit × 10 in 1/600 inch), for eleven fonts.
    #[test]
    fn the_digits_of_the_fonts_excel_was_measured_with() {
        let cases = [
            ("ＭＳ Ｐゴシック", 11.0, 8, 46),
            ("ＭＳ ゴシック", 11.0, 8, 46),
            ("ＭＳ 明朝", 11.0, 8, 46),
            ("游ゴシック", 11.0, 8, 51),
            ("Meiryo UI", 9.0, 7, 47),
            ("Calibri", 11.0, 7, 47),
            ("Arial", 10.0, 7, 46),
            ("Courier New", 8.0, 7, 40),
            ("Tahoma", 10.0, 7, 45),
            ("Times New Roman", 12.0, 8, 50),
            ("Verdana", 9.0, 8, 48),
        ];
        for (font, pt, screen, printer) in cases {
            let Some(s) = max_digit_px(font, pt, 96.0) else { continue };
            assert_eq!((s, max_digit_px(font, pt, 600.0)), (screen, Some(printer)), "{font} {pt} pt");
        }
    }
}

#[cfg(all(test, not(windows)))]
mod tests_without_gdi {
    use super::char_advance_pt;

    /// Where GDI is not there (macOS, Linux) nothing is measured: the callers keep the text system's advance.
    #[test]
    fn char_advance_is_none_without_gdi() {
        for font in ["Times New Roman", "Century", "Arial"] {
            assert_eq!(char_advance_pt(font, 10.5, false, false, 'a'), None);
            assert_eq!(char_advance_pt(font, 10.5, true, true, 'W'), None);
        }
    }
}
