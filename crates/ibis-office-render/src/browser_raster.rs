// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The web build's text the browser draws (Japanese: the bundled fonts have none) as pixels: the browser's Canvas draws it into memory,
//! and the pixels are read back (the coverage of the text), for the effects that need the letters' shape (the glow of text, `blur_shadow`).

use wasm_bindgen::{JsCast, JsValue};

/// The Canvas 2D drawing of `texts` (each at its x, on one baseline) in CSS font `font`, grown by a round stroke of `grow` on every side
/// when `grow` is more than 0, on a canvas of `cols` x `rows` cells of `pitch` logical pixels. Coordinates are logical pixels from the canvas's
/// top-left; the baseline is `baseline` down. The coverage (alpha, 0 to 1) of each cell.
const DRAW: &str = "
const c = new OffscreenCanvas(cols, rows);
const g = c.getContext('2d', { willReadFrequently: true });
g.scale(1 / pitch, 1 / pitch);
g.font = font;
g.textBaseline = 'alphabetic';
g.fillStyle = '#000';
g.strokeStyle = '#000';
g.lineJoin = 'round';
g.lineCap = 'round';
g.lineWidth = 2 * grow;
for (let i = 0; i < texts.length; i++) {
  if (grow > 0) g.strokeText(texts[i], xs[i], baseline);
  g.fillText(texts[i], xs[i], baseline);
}
const d = g.getImageData(0, 0, cols, rows).data;
const a = new Float32Array(cols * rows);
for (let i = 0; i < a.length; i++) a[i] = d[i * 4 + 3] / 255;
return a;
";

thread_local! {
    static DRAW_FN: js_sys::Function = js_sys::Function::new_with_args("font,texts,xs,baseline,grow,pitch,cols,rows", DRAW);
}

/// The CSS `font` shorthand for `font` at `size` logical pixels: the typefaces the document names and the browser's stand-ins (`Font::fallbacks`, the
/// empty name that separates them left out), then `sans-serif`.
pub fn css_font(font: &gpui_kit::Font, size: f32) -> String {
    let families: Vec<String> = font.fallbacks.iter().flat_map(|f| f.fallback_list().iter()).filter(|n| !n.is_empty()).map(|n| format!("\"{}\"", n.replace('"', ""))).collect();
    format!(
        "{} {} {}px {}",
        if font.style == gpui_kit::FontStyle::Italic { "italic" } else { "normal" },
        font.weight.0,
        size,
        families.iter().map(String::as_str).chain(["sans-serif"]).collect::<Vec<_>>().join(", ")
    )
}

/// See `DRAW`. `None` when the browser cannot draw it.
pub(crate) fn text_coverage(font: &str, clusters: &[(String, f32)], baseline: f32, grow: f32, pitch: f32, cols: usize, rows: usize) -> Option<Vec<f32>> {
    let texts = js_sys::Array::new();
    let xs = js_sys::Array::new();
    for (text, x) in clusters {
        texts.push(&JsValue::from_str(text));
        xs.push(&JsValue::from_f64(f64::from(*x)));
    }
    let args = js_sys::Array::new();
    for value in [JsValue::from_str(font), texts.into(), xs.into(), JsValue::from_f64(f64::from(baseline)), JsValue::from_f64(f64::from(grow)), JsValue::from_f64(f64::from(pitch)), JsValue::from_f64(cols as f64), JsValue::from_f64(rows as f64)] {
        args.push(&value);
    }
    let result = DRAW_FN.with(|f| f.apply(&JsValue::NULL, &args)).ok()?;
    let array = result.dyn_into::<js_sys::Float32Array>().ok()?;
    Some(array.to_vec())
}

// ---- Vertical text: the browser's own vertical glyphs ----
//
// The fonts the browser draws with are not in the app, so their `vert`/`vrt2` glyphs cannot be asked for, and Canvas 2D has no vertical writing.
// The browser's layout does: an SVG `foreignObject` with `writing-mode: vertical-rl` is shaped with the font's vertical glyphs (、。「」ー…) or turns the
// character (UAX #50, `text-orientation`), and drawing that SVG on a canvas (a data URL: the canvas stays readable) gives its pixels. The image loads
// asynchronously: a glyph not ready yet is drawn the old way, and `pending` asks the page for another frame.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use gpui_kit::RenderImage;

const VERTICAL: &str = "
const S = size, W = 2 * size;
const esc = ch.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='${W}' height='${W}'><foreignObject width='${W}' height='${W}'><div xmlns='http://www.w3.org/1999/xhtml' style='position:absolute;left:${S / 2}px;top:${S / 2}px;width:${S}px;height:${S}px;font:${font};line-height:${S}px;writing-mode:vertical-rl;text-orientation:${sideways ? 'sideways' : 'mixed'};color:${color};white-space:pre'>${esc}</div></foreignObject></svg>`;
const img = new Image();
img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
await new Promise((resolve, reject) => { img.onload = resolve; img.onerror = reject; });
const px = Math.max(1, Math.ceil(W * scale));
const c = new OffscreenCanvas(px, px);
const g = c.getContext('2d', { willReadFrequently: true });
g.drawImage(img, 0, 0, px, px);
const data = g.getImageData(0, 0, px, px).data;
g.font = font;
const asc = g.measureText('\\u3042').emHeightAscent;
return { data, px, asc: asc > 0 ? asc : 0.88 * size };
";

enum Slot {
    Loading,
    Ready(Arc<RenderImage>, f32),
    Failed,
}

thread_local! {
    static VERTICAL_FN: js_sys::Function = js_sys::Function::new_with_args("font,ch,size,color,sideways,scale", &format!("return (async () => {{{VERTICAL}}})();"));
    static SLOTS: RefCell<HashMap<u64, Slot>> = RefCell::new(HashMap::new());
    static LOADING: Cell<usize> = const { Cell::new(0) };
}

/// Whether a glyph is still being made (draw again soon).
pub fn pending() -> bool {
    LOADING.with(|n| n.get() > 0)
}

/// The picture of `ch` as the browser sets it in vertical text (`sideways`: turned, else its vertical glyph or upright) in CSS font `font`
/// (a `font` shorthand with `size`px) and `color` (a CSS colour): a square of 2 x `size` logical pixels whose middle square is the character's cell,
/// and how far the cell's top is above the horizontal baseline (the em box's ascent). `None` while it is made.
pub fn vertical_glyph(font: &str, ch: &str, size: f32, color: &str, sideways: bool, scale: f32) -> Option<(Arc<RenderImage>, f32)> {
    let mut hasher = DefaultHasher::new();
    (font, ch, size.to_bits(), color, sideways, scale.to_bits()).hash(&mut hasher);
    let key = hasher.finish();
    let found = SLOTS.with(|slots| match slots.borrow().get(&key) {
        Some(Slot::Ready(image, ascent)) => Some(Some((image.clone(), *ascent))),
        Some(Slot::Loading | Slot::Failed) => Some(None),
        None => None,
    });
    if let Some(found) = found {
        return found;
    }
    SLOTS.with(|slots| slots.borrow_mut().insert(key, Slot::Loading));
    LOADING.with(|n| n.set(n.get() + 1));
    let args = js_sys::Array::new();
    for value in [JsValue::from_str(font), JsValue::from_str(ch), JsValue::from_f64(f64::from(size)), JsValue::from_str(color), JsValue::from_bool(sideways), JsValue::from_f64(f64::from(scale))] {
        args.push(&value);
    }
    let promise = VERTICAL_FN.with(|f| f.apply(&JsValue::NULL, &args)).ok().and_then(|v| v.dyn_into::<js_sys::Promise>().ok());
    wasm_bindgen_futures::spawn_local(async move {
        let slot = match promise {
            Some(promise) => match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(result) => make_slot(&result).unwrap_or(Slot::Failed),
                Err(_) => Slot::Failed,
            },
            None => Slot::Failed,
        };
        SLOTS.with(|slots| slots.borrow_mut().insert(key, slot));
        LOADING.with(|n| n.set(n.get().saturating_sub(1)));
    });
    None
}

fn make_slot(result: &JsValue) -> Option<Slot> {
    let field = |name: &str| js_sys::Reflect::get(result, &JsValue::from_str(name)).ok();
    let px = field("px")?.as_f64()? as u32;
    let ascent = field("asc")?.as_f64()? as f32;
    let data = js_sys::Uint8ClampedArray::new(&field("data")?).to_vec();
    // The colour as it came, not premultiplied, in the order the picture painter takes (see `blur_shadow::paint`).
    let mut bytes = Vec::with_capacity(data.len());
    for p in data.chunks_exact(4) {
        bytes.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
    }
    let buffer = image::RgbaImage::from_raw(px, px, bytes)?;
    Some(Slot::Ready(Arc::new(RenderImage::new([image::Frame::new(buffer)])), ascent))
}
