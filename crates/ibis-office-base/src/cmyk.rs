// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! CMYK colours in the editor (ibisOffice's own; PowerPoint has none): the converter in use, and "what the press prints" for a
//! colour (`ibis_doc_model::CmykConverter::to_gamut`). In a file with the CMYK mode on, what the colour picker and the palettes
//! show, and what they save, is the colour passed through CMYK and back, so that what is seen is what prints. The black ink's
//! share is the converter's own (its black generation); the user does not set it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use ibis_doc_model::{Cmyk, CmykConverter, OpenCmykConverter, Rgb};

static CONVERTER: RwLock<Option<Arc<dyn CmykConverter>>> = RwLock::new(None);
/// Converted colours (sRGB to what prints, sRGB to CMYK), kept until the converter changes: a colour wheel asks for hundreds.
static GAMUT_CACHE: Mutex<Option<HashMap<u32, u32>>> = Mutex::new(None);
static CMYK_CACHE: Mutex<Option<HashMap<u32, Cmyk>>> = Mutex::new(None);
/// Whether the screen shows colours as the press prints them now (the open file has the CMYK mode on); set at each draw.
static DISPLAY_GAMUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The conversion in use: the open model until the user chooses an ICC profile (`shell::cmyk_profile`).
pub fn converter() -> Arc<dyn CmykConverter> {
    CONVERTER.read().ok().and_then(|c| c.clone()).unwrap_or_else(|| Arc::new(OpenCmykConverter))
}

/// Puts `converter` in use.
pub fn set_converter(converter: Arc<dyn CmykConverter>) {
    if let Ok(mut c) = CONVERTER.write() {
        *c = Some(converter);
    }
    *GAMUT_CACHE.lock().unwrap() = None;
    *CMYK_CACHE.lock().unwrap() = None;
}

/// `rgb` as the press prints it (through CMYK and back, with the converter in use).
pub fn gamut(rgb: Rgb) -> Rgb {
    let mut cache = GAMUT_CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some(v) = map.get(&rgb.0) {
        return Rgb(*v);
    }
    drop(cache);
    let out = converter().to_gamut(rgb);
    GAMUT_CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(rgb.0, out.0);
    out
}

/// The CMYK values `rgb` is printed with (the PDF export).
pub fn cmyk_of(rgb: Rgb) -> Cmyk {
    if let Some(c) = CMYK_CACHE.lock().unwrap().get_or_insert_with(HashMap::new).get(&rgb.0) {
        return *c;
    }
    let out = converter().to_cmyk(rgb);
    CMYK_CACHE.lock().unwrap().get_or_insert_with(HashMap::new).insert(rgb.0, out);
    out
}

/// Whether `rgb` is outside what the press prints (it comes back from CMYK visibly different).
pub fn out_of_gamut(rgb: Rgb) -> bool {
    let g = gamut(rgb);
    let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
    d(rgb.r(), g.r()).max(d(rgb.g(), g.g())).max(d(rgb.b(), g.b())) > 12
}

/// The screen shows the colours as printed from now on (or as they are).
pub fn set_display_gamut(on: bool) {
    DISPLAY_GAMUT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the screen shows colours as printed (the open file has the CMYK mode on).
pub fn display_gamut_on() -> bool {
    DISPLAY_GAMUT.load(std::sync::atomic::Ordering::Relaxed)
}

/// What to draw for `rgb`: the colour as printed in a file with the CMYK mode on, else the colour itself.
pub fn shown(rgb: Rgb) -> Rgb {
    if DISPLAY_GAMUT.load(std::sync::atomic::Ordering::Relaxed) { gamut(rgb) } else { rgb }
}
