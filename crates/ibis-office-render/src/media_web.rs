// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The web build's player of 動画・音声: the bytes of the clip are a Blob the browser's `HTMLMediaElement` (an `<audio>` or a `<video>` that is
//! not put in the page) plays, as the system's player does on the desktop (`media_player`). The sound is the element's own; a video's frames are
//! taken from it by drawing it on an offscreen canvas and reading the pixels back, and GPUI draws them in the video's place on the slide, so the
//! video lies under the slide's other shapes, its corners, outline and shadow, as on the desktop.

use std::sync::Arc;

use gpui_kit::RenderImage;
use wasm_bindgen::{JsCast, JsValue};

/// A video frame, as GPUI draws it.
pub type Frame = Arc<RenderImage>;

/// Frames wider than this are drawn smaller (reading back a 4K frame thirty times a second is not worth it).
const MAX_FRAME_WIDTH: u32 = 1280;

/// The element for `bytes`: `{ el, url }`, or `null` when the browser cannot play `mime`.
const OPEN: &str = "
const probe = document.createElement(video ? 'video' : 'audio');
// Chrome does not list QuickTime but plays the H.264 movies in it: they are offered as MP4 (the bytes are the same container family).
if (!probe.canPlayType(mime) && mime === 'video/quicktime') mime = 'video/mp4';
if (!probe.canPlayType(mime)) return null;
const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
probe.src = url;
probe.preload = 'auto';
probe.playsInline = true;
probe.loop = false;
probe.__t = -1;
// What the browser says when it cannot play (the element's error, a refused play()), and when the length is known: read by `Player`.
probe.addEventListener('error', () => {
  const e = probe.error;
  probe.__err = e ? ('media error ' + e.code + (e.message ? ': ' + e.message : '')) : 'media error';
});
probe.addEventListener('loadedmetadata', () => { probe.__meta = true; });
probe.load();
return { el: probe, url };
";

/// The newest frame of `el` as `{ data, width, height }` (RGBA), or `null` when it has none that was not taken yet.
const GRAB: &str = "
if (el.readyState < 2 || !el.videoWidth) return null;
if (el.currentTime === el.__t) return null;
el.__t = el.currentTime;
const k = Math.min(1, maxw / el.videoWidth);
const w = Math.max(1, Math.round(el.videoWidth * k)), h = Math.max(1, Math.round(el.videoHeight * k));
if (!el.__c || el.__c.width !== w || el.__c.height !== h) {
  el.__c = new OffscreenCanvas(w, h);
  el.__g = el.__c.getContext('2d', { willReadFrequently: true });
}
el.__g.drawImage(el, 0, 0, w, h);
return { data: el.__g.getImageData(0, 0, w, h).data, width: w, height: h };
";

/// Plays `el`; a refusal is kept in `__err` for `Player::take_error` (the status bar says it).
const PLAY: &str = "const p = el.play(); if (p && p.catch) p.catch((e) => { el.__err = 'play: ' + (e && e.name ? e.name : 'refused') + (e && e.message ? ' - ' + e.message : ''); });";

/// Lets the element go: stopped, its Blob freed.
const CLOSE: &str = "el.pause(); el.removeAttribute('src'); el.load(); URL.revokeObjectURL(url);";

/// The width over the height of the video in `bytes`, from its metadata (`null` when the browser cannot read one in time).
const ASPECT: &str = "return (async () => {
  const v = document.createElement('video');
  if (!v.canPlayType(mime) && mime === 'video/quicktime') mime = 'video/mp4';
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  try {
    v.preload = 'metadata';
    v.src = url;
    await new Promise((resolve, reject) => { v.onloadedmetadata = resolve; v.onerror = reject; setTimeout(reject, 8000); });
    return v.videoWidth > 0 && v.videoHeight > 0 ? v.videoWidth / v.videoHeight : null;
  } catch (e) {
    return null;
  } finally {
    v.removeAttribute('src');
    v.load();
    URL.revokeObjectURL(url);
  }
})();";

thread_local! {
    static ASPECT_FN: js_sys::Function = js_sys::Function::new_with_args("bytes,mime", ASPECT);
    static OPEN_FN: js_sys::Function = js_sys::Function::new_with_args("bytes,mime,video", OPEN);
    static GRAB_FN: js_sys::Function = js_sys::Function::new_with_args("el,maxw", GRAB);
    static PLAY_FN: js_sys::Function = js_sys::Function::new_with_args("el", PLAY);
    static CLOSE_FN: js_sys::Function = js_sys::Function::new_with_args("el,url", CLOSE);
}

fn get(target: &JsValue, name: &str) -> JsValue {
    js_sys::Reflect::get(target, &JsValue::from_str(name)).unwrap_or(JsValue::UNDEFINED)
}

fn set(target: &JsValue, name: &str, value: JsValue) {
    let _ = js_sys::Reflect::set(target, &JsValue::from_str(name), &value);
}

fn ms(seconds: f64) -> u32 {
    if seconds.is_finite() && seconds > 0.0 { (seconds * 1000.0) as u32 } else { 0 }
}

/// Whether the browser can play content type `mime`.
pub fn can_play(mime: &str) -> bool {
    let probe = js_sys::Function::new_with_args(
        "mime",
        "const v = document.createElement('video'); return !!v.canPlayType(mime) || (mime === 'video/quicktime' && !!v.canPlayType('video/mp4'));",
    );
    probe.call1(&JsValue::NULL, &JsValue::from_str(mime)).ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// The width over the height of the video in `data`, as `media_player::video_aspect` gives it on the desktop; read from the metadata
/// the browser loads, so it takes a moment. `None` when it cannot be read.
pub async fn video_aspect_of(data: &[u8], mime: &str) -> Option<f64> {
    let bytes = js_sys::Uint8Array::from(data);
    let args = js_sys::Array::of2(&bytes, &JsValue::from_str(mime));
    let promise = ASPECT_FN.with(|f| f.apply(&JsValue::NULL, &args)).ok()?.dyn_into::<js_sys::Promise>().ok()?;
    wasm_bindgen_futures::JsFuture::from(promise).await.ok()?.as_f64().filter(|a| *a > 0.0)
}

/// A clip being played.
pub struct Player {
    el: JsValue,
    url: String,
    video: bool,
}

impl Player {
    /// The clip in `data` (of content type `mime`), ready to play (`video`: its frames are taken too); `None` when the browser cannot play it.
    pub fn open_bytes(data: &[u8], mime: &str, video: bool) -> Option<Player> {
        let bytes = js_sys::Uint8Array::from(data);
        let args = js_sys::Array::of3(&bytes, &JsValue::from_str(mime), &JsValue::from_bool(video));
        let made = OPEN_FN.with(|f| f.apply(&JsValue::NULL, &args)).ok()?;
        if made.is_null() || made.is_undefined() {
            return None;
        }
        Some(Player { el: get(&made, "el"), url: get(&made, "url").as_string()?, video })
    }

    pub fn play(&self) {
        let _ = PLAY_FN.with(|f| f.call1(&JsValue::NULL, &self.el));
    }

    pub fn pause(&self) {
        if let Ok(f) = get(&self.el, "pause").dyn_into::<js_sys::Function>() {
            let _ = f.call0(&self.el);
        }
    }

    /// Paused and back at the start.
    pub fn rewind(&self) {
        self.pause();
        self.seek_to_ms(0);
    }

    pub fn seek_to_ms(&self, ms: u32) {
        set(&self.el, "currentTime", JsValue::from_f64(f64::from(ms) / 1000.0));
    }

    pub fn current_time_ms(&self) -> u32 {
        ms(get(&self.el, "currentTime").as_f64().unwrap_or(0.0))
    }

    /// Whole length in milliseconds (0 until the browser knows it).
    pub fn duration_ms(&self) -> u32 {
        ms(get(&self.el, "duration").as_f64().unwrap_or(0.0))
    }

    /// What the browser said when it could not play (once): the element's error or a refused `play()`.
    pub fn take_error(&self) -> Option<String> {
        let error = get(&self.el, "__err").as_string()?;
        set(&self.el, "__err", JsValue::UNDEFINED);
        Some(error)
    }

    /// Whether the length became known since the last call (once): the control bar shows it then.
    pub fn take_metadata_ready(&self) -> bool {
        let ready = get(&self.el, "__meta").as_bool().unwrap_or(false);
        if ready {
            set(&self.el, "__meta", JsValue::FALSE);
        }
        ready
    }

    pub fn is_playing(&self) -> bool {
        !get(&self.el, "paused").as_bool().unwrap_or(true) && !self.finished()
    }

    pub fn set_volume(&self, volume: f32) {
        set(&self.el, "volume", JsValue::from_f64(f64::from(volume.clamp(0.0, 1.0))));
    }

    /// Whether it played to its end.
    pub fn finished(&self) -> bool {
        get(&self.el, "ended").as_bool().unwrap_or(false)
    }

    /// Whether it played to its end, taking trim_end_ms into account.
    pub fn finished_with_trim(&self, trim_end_ms: u32) -> bool {
        (trim_end_ms > 0 && self.current_time_ms() >= trim_end_ms) || self.finished()
    }

    /// The newest frame, if there is one that was not taken yet.
    pub fn frame(&self) -> Option<Frame> {
        if !self.video {
            return None;
        }
        let args = js_sys::Array::of2(&self.el, &JsValue::from_f64(f64::from(MAX_FRAME_WIDTH)));
        let grabbed = GRAB_FN.with(|f| f.apply(&JsValue::NULL, &args)).ok()?;
        if grabbed.is_null() || grabbed.is_undefined() {
            return None;
        }
        let (width, height) = (get(&grabbed, "width").as_f64()? as u32, get(&grabbed, "height").as_f64()? as u32);
        let data = js_sys::Uint8ClampedArray::new(&get(&grabbed, "data")).to_vec();
        // RGBA as the canvas gave it; the picture painter takes blue first (see `browser_raster::make_slot`); a frame has no alpha.
        let mut bytes = Vec::with_capacity(data.len());
        for p in data.chunks_exact(4) {
            bytes.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
        let buffer = image::RgbaImage::from_raw(width, height, bytes)?;
        Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = CLOSE_FN.with(|f| f.call2(&JsValue::NULL, &self.el, &JsValue::from_str(&self.url)));
    }
}
