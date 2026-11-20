// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// The web build's browser-side helpers of ibis-office-render (media_web.rs: the player of video and audio; browser_raster.rs: text the browser
// draws). They are functions of this file, imported by wasm-bindgen, not source text built into a function at run time (`new Function` needs
// `unsafe-eval`, which the Content-Security-Policy of a host page does not give).
export const openFn = (bytes, mime, video) => {
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
};

export const grabFn = (el, maxw) => {
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
};

export const playFn = (el) => {
  const p = el.play(); if (p && p.catch) p.catch((e) => { el.__err = 'play: ' + (e && e.name ? e.name : 'refused') + (e && e.message ? ' - ' + e.message : ''); });
};

export const closeFn = (el, url) => {
  el.pause(); el.removeAttribute('src'); el.load(); URL.revokeObjectURL(url);
};

export const aspectFn = (bytes, mime) => {
  return (async () => {
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
  })();
};

export const canPlayFn = (mime) => {
  const v = document.createElement('video'); return !!v.canPlayType(mime) || (mime === 'video/quicktime' && !!v.canPlayType('video/mp4'));
};

export const drawFn = (font, texts, xs, baseline, grow, pitch, cols, rows) => {
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
};

export const verticalFn = async (font, ch, size, color, sideways, scale) => {
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
  const asc = g.measureText('\u3042').emHeightAscent;
  return { data, px, asc: asc > 0 ? asc : 0.88 * size };
};

