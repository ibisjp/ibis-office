// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Rotated and flipped pictures.
//!
//! GPUI draws pictures upright only. A picture that is rotated or flipped
//! is wrapped in an SVG document that embeds it with the rotation, flips and
//! crop applied; GPUI rasterizes that SVG once (with resvg) and draws the
//! result like any picture. The SVG covers the rotated frame's bounding box,
//! so the corners outside the picture stay transparent.

use gpui_kit::SMOOTH_SVG_SCALE_FACTOR;
use ibis_doc_model::{Crop, EmuRect};

/// The encoding of a picture that can be embedded in SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encoding {
    Png,
    Jpeg,
    Gif,
    Webp,
    Svg,
}

impl Encoding {
    fn mime(self) -> &'static str {
        match self {
            Encoding::Png => "image/png",
            Encoding::Jpeg => "image/jpeg",
            Encoding::Gif => "image/gif",
            Encoding::Webp => "image/webp",
            Encoding::Svg => "image/svg+xml",
        }
    }
}

/// Largest rasterized size, in pixels; bigger pictures are drawn smaller.
const MAX_PIXELS: f64 = 16_000_000.0;
/// Width the frame gets when the picture's own size is unknown.
const FALLBACK_WIDTH: f64 = 2048.0;

/// The size an SVG says it is, for `pixels` rasterized: GPUI rasterizes an
/// SVG picture at twice its size (`SvgRenderer::render_single_frame`, by
/// `SMOOTH_SVG_SCALE_FACTOR`), so it says half. Else a picture takes four
/// times its pixels (seconds for a large photo).
fn stated(pixels: f64) -> f64 {
    ((pixels / SMOOTH_SVG_SCALE_FACTOR as f64) * 1000.0).round().max(500.0) / 1000.0
}

/// A rotated picture ready to draw: the SVG and where it goes on the slide.
pub(crate) struct RotatedPicture {
    pub(crate) svg: Vec<u8>,
    /// The rotated frame's bounding box, in EMU.
    pub(crate) bounds: EmuRect,
}

/// Wrap a picture drawn in `frame`, rotated clockwise by `rotation`
/// (60 000ths of a degree) about the frame's centre, flipped as given,
/// cut to `subpaths` when given, and given the colour effects of `look`.
pub(crate) fn rotated_picture(
    data: &[u8],
    encoding: Encoding,
    frame: EmuRect,
    rotation: i32,
    flip_h: bool,
    flip_v: bool,
    crop: Crop,
    subpaths: Option<&[Vec<(f64, f64)>]>,
    look: &Look,
) -> RotatedPicture {
    let degrees = rotation as f64 / 60_000.0;
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (fw_emu, fh_emu) = (frame.width.max(1) as f64, frame.height.max(1) as f64);
    let bw_emu = fw_emu * cos.abs() + fh_emu * sin.abs();
    let bh_emu = fw_emu * sin.abs() + fh_emu * cos.abs();
    let (cx_emu, cy_emu) = (frame.x as f64 + fw_emu / 2.0, frame.y as f64 + fh_emu / 2.0);
    let bounds = EmuRect::new(
        (cx_emu - bw_emu / 2.0).round() as i64,
        (cy_emu - bh_emu / 2.0).round() as i64,
        bw_emu.round() as i64,
        bh_emu.round() as i64,
    );

    // Rasterize at the picture's own resolution across the frame.
    let frac = |v: i32| v as f64 / 100_000.0;
    let visible_w = (1.0 - frac(crop.left) - frac(crop.right)).max(0.01);
    let visible_h = (1.0 - frac(crop.top) - frac(crop.bottom)).max(0.01);
    let frame_px = pixel_size(data, encoding).map(|(w, _)| w as f64 * visible_w).unwrap_or(FALLBACK_WIDTH).max(16.0);
    let mut k = frame_px / fw_emu; // pixels per EMU
    let pixels = bw_emu * k * bh_emu * k;
    if pixels > MAX_PIXELS {
        k *= (MAX_PIXELS / pixels).sqrt();
    }
    let (fw, fh, bw, bh) = (fw_emu * k, fh_emu * k, bw_emu * k, bh_emu * k);

    // The whole picture, placed so the cropped part fills the frame.
    // (The picture's own area: the frame less the fill rectangle's insets, which may be negative.)
    let [fl, ft, fr, fb] = look.fill_rect.map_or([0.0; 4], |r| r.map(|v| v as f64 / 100_000.0));
    let (rx, ry, rw, rh) = (fw * fl, fh * ft, fw * (1.0 - fl - fr), fh * (1.0 - ft - fb));
    let (ix, iy) = (rx - frac(crop.left) / visible_w * rw, ry - frac(crop.top) / visible_h * rh);
    let (iw, ih) = (rw / visible_w, rh / visible_h);
    let (sx, sy) = (if flip_h { -1.0 } else { 1.0 }, if flip_v { -1.0 } else { 1.0 });
    let clip_shape = match subpaths {
        Some(subpaths) => {
            let mut d = String::new();
            for points in subpaths {
                for (i, (x, y)) in points.iter().enumerate() {
                    d.push_str(&format!("{}{:.2} {:.2} ", if i == 0 { 'M' } else { 'L' }, x * k, y * k));
                }
                d.push_str("Z ");
            }
            format!(r#"<path d="{d}"/>"#)
        }
        None => format!(r#"<rect x="0" y="0" width="{fw:.3}" height="{fh:.3}"/>"#),
    };
    let primitives = filter_primitives(look);
    let filter = if primitives.is_empty() { String::new() } else { format!(r#"<filter id="fx" color-interpolation-filters="sRGB" x="0" y="0" width="1" height="1">{primitives}</filter>"#) };
    let filter_attr = if filter.is_empty() { "" } else { r#" filter="url(#fx)""# };
    // Flip, then rotate, about the frame's centre (OOXML's order).
    let turn = format!("translate({cx:.3} {cy:.3}) rotate({deg}) scale({sx} {sy}) translate({ncx:.3} {ncy:.3})", cx = fw / 2.0, cy = fh / 2.0, deg = degrees, sx = sx, sy = sy, ncx = -fw / 2.0, ncy = -fh / 2.0);
    // The picture turns with the outline, or stays upright under the turned outline (`rotWithShape` 0: the outline carries the turn).
    let (clip_shape, open, close) = if look.upright {
        let turned = clip_shape.replacen("<path ", &format!("<path transform=\"{turn}\" "), 1).replacen("<rect ", &format!("<rect transform=\"{turn}\" "), 1);
        (turned, String::new(), "")
    } else {
        (clip_shape, format!(r#"<g transform="{turn}">"#), "</g>")
    };
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" "#,
            r#"width="{w}" height="{h}" viewBox="{vx:.3} {vy:.3} {bw:.3} {bh:.3}">"#,
            r#"<defs><clipPath id="frame">{clip_shape}</clipPath>{filter}</defs>"#,
            r#"{open}"#,
            r#"<g clip-path="url(#frame)"><image{filter_attr} x="{ix:.3}" y="{iy:.3}" width="{iw:.3}" height="{ih:.3}" preserveAspectRatio="none" xlink:href="data:{mime};base64,"#,
        ),
        w = stated(bw.round().max(1.0)),
        h = stated(bh.round().max(1.0)),
        vx = fw / 2.0 - bw / 2.0,
        vy = fh / 2.0 - bh / 2.0,
        bw = bw,
        bh = bh,
        clip_shape = clip_shape,
        filter = filter,
        filter_attr = filter_attr,
        open = open,
        ix = ix,
        iy = iy,
        iw = iw,
        ih = ih,
        mime = encoding.mime(),
    );
    let mut out = svg.into_bytes();
    base64(data, &mut out);
    out.extend_from_slice(br#""/></g>"#);
    out.extend_from_slice(close.as_bytes());
    out.extend_from_slice(b"</svg>");
    RotatedPicture { svg: out, bounds }
}

/// Wrap a picture that fills a shape `width` × `height` EMU, cut to the
/// shape's outline: `subpaths` are the outline's closed parts, as points
/// relative to the frame in EMU. The SVG covers the frame.
pub(crate) fn clipped_picture(data: &[u8], encoding: Encoding, width: i64, height: i64, subpaths: &[Vec<(f64, f64)>]) -> Vec<u8> {
    let (fw_emu, fh_emu) = (width.max(1) as f64, height.max(1) as f64);
    let frame_px = pixel_size(data, encoding).map(|(w, _)| w as f64).unwrap_or(FALLBACK_WIDTH).max(16.0);
    let mut k = frame_px / fw_emu;
    let pixels = fw_emu * k * fh_emu * k;
    if pixels > MAX_PIXELS {
        k *= (MAX_PIXELS / pixels).sqrt();
    }
    let (fw, fh) = (fw_emu * k, fh_emu * k);
    let mut d = String::new();
    for points in subpaths {
        for (i, (x, y)) in points.iter().enumerate() {
            d.push_str(&format!("{}{:.2} {:.2} ", if i == 0 { 'M' } else { 'L' }, x * k, y * k));
        }
        d.push_str("Z ");
    }
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" "#,
            r#"width="{w}" height="{h}" viewBox="0 0 {fw:.3} {fh:.3}">"#,
            r#"<defs><clipPath id="shape"><path d="{d}"/></clipPath></defs>"#,
            r#"<image clip-path="url(#shape)" x="0" y="0" width="{fw:.3}" height="{fh:.3}" preserveAspectRatio="none" xlink:href="data:{mime};base64,"#,
        ),
        w = stated(fw.round().max(1.0)),
        h = stated(fh.round().max(1.0)),
        fw = fw,
        fh = fh,
        d = d,
        mime = encoding.mime(),
    );
    let mut out = svg.into_bytes();
    base64(data, &mut out);
    out.extend_from_slice(br#""/></svg>"#);
    out
}

/// Wrap a picture recoloured as a duotone: its luminance maps from `dark`
/// (black) to `light` (white). The SVG has the picture's own size (at most
/// `MAX_PIXELS`).
pub(crate) fn duotone_picture(data: &[u8], encoding: Encoding, dark: (u8, u8, u8), light: (u8, u8, u8)) -> Vec<u8> {
    let (w, h) = own_size(data, encoding);
    let table = |a: u8, b: u8| format!("{:.4} {:.4}", a as f64 / 255.0, b as f64 / 255.0);
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{sw}" height="{sh}" viewBox="0 0 {w} {h}">"#,
            r#"<defs><filter id="duo" color-interpolation-filters="sRGB">"#,
            r#"<feColorMatrix type="matrix" values="0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0 0 0 1 0"/>"#,
            r#"<feComponentTransfer><feFuncR type="table" tableValues="{r}"/><feFuncG type="table" tableValues="{g}"/><feFuncB type="table" tableValues="{b}"/></feComponentTransfer>"#,
            r#"</filter></defs><image filter="url(#duo)" x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="none" xlink:href="data:{mime};base64,"#,
        ),
        w = w,
        h = h,
        sw = stated(w),
        sh = stated(h),
        r = table(dark.0, light.0),
        g = table(dark.1, light.1),
        b = table(dark.2, light.2),
        mime = encoding.mime(),
    );
    let mut out = svg.into_bytes();
    base64(data, &mut out);
    out.extend_from_slice(br#""/></svg>"#);
    out
}

/// Wrap a PNG picture of an art page border (`w` × `h` pixels) to be
/// rasterized at `raster_w` × `raster_h` pixels (the zip's strips are 20 000
/// pixels long, more than a texture takes). With `tint`, it is recoloured as
/// Word colours its grey art borders: black becomes `tint`, white stays
/// white, and the greys between go by their lightness (the coverage tests'
/// references from Word: birds' grey #cccccc comes out #f2cccc in C00000).
pub(crate) fn art_border_picture(png: &[u8], w: u32, h: u32, raster_w: f64, raster_h: f64, tint: Option<(u8, u8, u8)>) -> Vec<u8> {
    let colour = tint.map_or(String::new(), |(r, g, b)| {
        let table = |c: u8| format!("{:.4} 1", c as f64 / 255.0);
        format!(
            concat!(
                r#"<feColorMatrix type="matrix" values="0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0 0 0 1 0"/>"#,
                r#"<feComponentTransfer><feFuncR type="table" tableValues="{r}"/><feFuncG type="table" tableValues="{g}"/><feFuncB type="table" tableValues="{b}"/></feComponentTransfer>"#,
            ),
            r = table(r),
            g = table(g),
            b = table(b),
        )
    });
    let filtered = !colour.is_empty();
    let filter = if filtered { format!(r#"<defs><filter id="tint" color-interpolation-filters="sRGB">{colour}</filter></defs>"#) } else { String::new() };
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{sw}" height="{sh}" viewBox="0 0 {w} {h}">"#,
            "{filter}",
            r#"<image{use_filter} x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="none" xlink:href="data:image/png;base64,"#,
        ),
        sw = stated(raster_w),
        sh = stated(raster_h),
        w = w,
        h = h,
        filter = filter,
        use_filter = if filtered { r#" filter="url(#tint)""# } else { "" },
    );
    let mut out = svg.into_bytes();
    base64(png, &mut out);
    out.extend_from_slice(br#""/></svg>"#);
    out
}

/// Wrap a picture with the colour `clear` made transparent: a pixel's
/// opacity falls with how near its colour is to `clear` (fully clear at
/// it, opaque from three levels off). The SVG has the picture's own size (at
/// most `MAX_PIXELS`).
pub(crate) fn clear_colour_picture(data: &[u8], encoding: Encoding, clear: (u8, u8, u8)) -> Vec<u8> {
    let (w, h) = own_size(data, encoding);
    // The distance, summed over the channels, taken towards the side each
    // channel can move from the clear colour (exact for white, black and
    // other colours whose channels are all 0 or 255).
    let k = 85.0;
    let term = |c: u8| if c >= 128 { (-k, k * c as f64 / 255.0) } else { (k, -k * c as f64 / 255.0) };
    let (r, g, b) = (term(clear.0), term(clear.1), term(clear.2));
    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{sw}" height="{sh}" viewBox="0 0 {w} {h}">"#,
            r#"<defs><filter id="clear" color-interpolation-filters="sRGB">"#,
            r#"<feColorMatrix in="SourceGraphic" type="matrix" result="mask" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 {rk} {gk} {bk} 0 {c}"/>"#,
            r#"<feComposite in="SourceGraphic" in2="mask" operator="in"/>"#,
            r#"</filter></defs><image filter="url(#clear)" x="0" y="0" width="{w}" height="{h}" preserveAspectRatio="none" xlink:href="data:{mime};base64,"#,
        ),
        w = w,
        h = h,
        sw = stated(w),
        sh = stated(h),
        rk = r.0,
        gk = g.0,
        bk = b.0,
        c = r.1 + g.1 + b.1,
        mime = encoding.mime(),
    );
    let mut out = svg.into_bytes();
    base64(data, &mut out);
    out.extend_from_slice(br#""/></svg>"#);
    out
}

fn base64(data: &[u8], out: &mut Vec<u8>) {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63]);
        out.push(TABLE[(n >> 12) as usize & 63]);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] } else { b'=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] } else { b'=' });
    }
}

/// Width and height in pixels, from the file's header.
/// The size an SVG of the picture's own size gets: the picture's, down to
/// `MAX_PIXELS` (a photo of 57 million pixels takes seconds to rasterize).
fn own_size(data: &[u8], encoding: Encoding) -> (f64, f64) {
    let (w, h) = pixel_size(data, encoding).map(|(w, h)| (w as f64, h as f64)).unwrap_or((FALLBACK_WIDTH, FALLBACK_WIDTH));
    let k = (MAX_PIXELS / (w * h).max(1.0)).sqrt().min(1.0);
    if k < 1.0 { ((w * k).round().max(1.0), (h * k).round().max(1.0)) } else { (w, h) }
}

fn pixel_size(data: &[u8], encoding: Encoding) -> Option<(u32, u32)> {
    let be16 = |at: usize| Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?) as u32);
    let be32 = |at: usize| Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?));
    let le16 = |at: usize| Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?) as u32);
    match encoding {
        Encoding::Png => Some((be32(16)?, be32(20)?)),
        Encoding::Gif => Some((le16(6)?, le16(8)?)),
        Encoding::Jpeg => {
            // The frame header (SOFn) holds the size; skip other segments.
            let mut at = 2;
            while at + 9 < data.len() {
                if data[at] != 0xFF {
                    return None;
                }
                let marker = data[at + 1];
                let length = be16(at + 2)? as usize;
                if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                    return Some((be16(at + 7)?, be16(at + 5)?));
                }
                at += 2 + length;
            }
            None
        }
        // A vector picture has no pixel size; the fallback width applies.
        Encoding::Svg => None,
        Encoding::Webp => {
            let le24 = |at: usize| Some(le16(at)? | (*data.get(at + 2)? as u32) << 16);
            match data.get(12..16)? {
                b"VP8X" => Some((le24(24)? + 1, le24(27)? + 1)),
                b"VP8 " => Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
                b"VP8L" => {
                    let bits = u32::from_le_bytes(data.get(21..25)?.try_into().ok()?);
                    Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
                }
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_pads() {
        let mut out = Vec::new();
        base64(b"Man", &mut out);
        base64(b"Ma", &mut out);
        base64(b"M", &mut out);
        assert_eq!(out, b"TWFuTWE=TQ==");
    }

    #[test]
    fn quarter_turn_swaps_the_bounds() {
        let png = [0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0, 200, 0, 0, 0, 100];
        let p = rotated_picture(&png, Encoding::Png, EmuRect::new(0, 0, 2000, 1000), 5_400_000, false, false, Crop::default(), None, &Look::default());
        assert_eq!(p.bounds, EmuRect::new(500, -500, 1000, 2000));
        let svg = String::from_utf8(p.svg).unwrap();
        // Rasterized at the picture's width across the frame: 200 px for
        // 2000 EMU (stated half: GPUI rasterizes an SVG at twice its size).
        assert!(svg.contains(r#"width="50" height="100""#), "{svg}");
        assert!(svg.contains("rotate(90)"));
    }

    #[test]
    fn jpeg_size_from_frame_header() {
        // SOI, an APP0 segment of length 4, then SOF0 with 16 × 32.
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 17, 8, 0, 32, 0, 16, 3];
        assert_eq!(pixel_size(&jpeg, Encoding::Jpeg), Some((16, 32)));
    }
}

/// What a styled picture shows beyond its pixels: [`styled_picture`]'s look.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Look {
    /// A colour made transparent (`a:clrChange` to alpha 0).
    pub clear: Option<(u8, u8, u8)>,
    /// Brightness and contrast (`a:lum`), 100 000ths.
    pub luminance: Option<(i32, i32)>,
    /// Dark and light colours (`a:duotone`).
    pub duotone: Option<((u8, u8, u8), (u8, u8, u8))>,
    pub effects: ibis_doc_model::BlipEffects,
    pub soft_edge: Option<i64>,
    pub artistic_effect: Option<ibis_doc_model::ArtisticEffect>,
    /// The area the picture covers, as insets from the frame in 100 000ths (`a:stretch/a:fillRect`); a turned picture's.
    pub fill_rect: Option<[i32; 4]>,
    /// The picture stays upright while the outline that cuts it turns (`rotWithShape` 0).
    pub upright: bool,
}

fn rgb_text(c: ibis_doc_model::Rgb) -> String {
    format!("rgb({},{},{})", c.r(), c.g(), c.b())
}

/// The filter primitives of a [`Look`], applied one after the other: a colour changed into another, grey,
/// black and white, brightness and contrast (measured against PowerPoint: the brightness is added to
/// each channel and the contrast turns each channel about the middle by 1 / (1 - c) above zero and
/// 1 + c below), the two colours of a duotone, one colour for all, and the opacity.
fn filter_primitives(look: &Look) -> String {
    const GRAY: &str = r#"<feColorMatrix type="matrix" values="0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0.2126 0.7152 0.0722 0 0 0 0 0 1 0"/>"#;
    let linear = |slope: f64, intercept: f64| format!(r#"<feComponentTransfer><feFuncR type="linear" slope="{slope:.5}" intercept="{intercept:.5}"/><feFuncG type="linear" slope="{slope:.5}" intercept="{intercept:.5}"/><feFuncB type="linear" slope="{slope:.5}" intercept="{intercept:.5}"/></feComponentTransfer>"#);
    let mut f = String::new();
    if let Some(clear) = look.clear {
        let k = 85.0;
        let term = |c: u8| if c >= 128 { (-k, k * c as f64 / 255.0) } else { (k, -k * c as f64 / 255.0) };
        let (r, g, b) = (term(clear.0), term(clear.1), term(clear.2));
        f.push_str(&format!(
            concat!(
                r#"<feColorMatrix in="SourceGraphic" type="matrix" result="clear_mask" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 {rk} {gk} {bk} 0 {c}"/>"#,
                r#"<feComposite in="SourceGraphic" in2="clear_mask" operator="in"/>"#,
            ),
            rk = r.0,
            gk = g.0,
            bk = b.0,
            c = r.1 + g.1 + b.1,
        ));
    }
    if let Some((from, to)) = look.effects.change {
        // The mask: a pixel's opacity falls with how near its colour is to `from`, summed over the channels
        // (a table per channel gives each one's distance, up to four levels).
        let table = |t: u8| (0..256).map(|i| format!("{:.3}", ((i as f64 - t as f64).abs() / 4.0).min(1.0))).collect::<Vec<_>>().join(" ");
        f.push_str(&format!(
            concat!(
                r#"<feComponentTransfer in="SourceGraphic" result="d"><feFuncR type="table" tableValues="{r}"/><feFuncG type="table" tableValues="{g}"/><feFuncB type="table" tableValues="{b}"/></feComponentTransfer>"#,
                r#"<feColorMatrix in="d" type="matrix" result="m" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 1 1 0 0"/>"#,
                r#"<feComposite in="SourceGraphic" in2="m" operator="in" result="keep"/>"#,
                r#"<feFlood flood-color="{to}" result="fl"/><feComposite in="fl" in2="m" operator="out" result="swap"/>"#,
                r#"<feComposite in="swap" in2="SourceAlpha" operator="in" result="swap2"/>"#,
                r#"<feMerge><feMergeNode in="keep"/><feMergeNode in="swap2"/></feMerge>"#,
            ),
            r = table(from.r()),
            g = table(from.g()),
            b = table(from.b()),
            to = rgb_text(to),
        ));
    }
    if look.effects.grayscale {
        f.push_str(GRAY);
    }
    if let Some(t) = look.effects.bilevel {
        let t = t as f64 / 100_000.0;
        f.push_str(GRAY);
        f.push_str(&linear(1000.0, -1000.0 * t));
    }
    if let Some((bright, contrast)) = look.luminance {
        let (b, c) = (bright as f64 / 100_000.0, contrast as f64 / 100_000.0);
        let k = if c >= 0.0 { 1.0 / (1.0 - c).max(0.001) } else { 1.0 + c };
        f.push_str(&linear(k, 0.5 - 0.5 * k + b));
    }
    if let Some((dark, light)) = look.duotone {
        let table = |a: u8, b: u8| format!("{:.4} {:.4}", a as f64 / 255.0, b as f64 / 255.0);
        f.push_str(GRAY);
        f.push_str(&format!(r#"<feComponentTransfer><feFuncR type="table" tableValues="{}"/><feFuncG type="table" tableValues="{}"/><feFuncB type="table" tableValues="{}"/></feComponentTransfer>"#, table(dark.0, light.0), table(dark.1, light.1), table(dark.2, light.2)));
    }
    if let Some(c) = look.effects.replace {
        f.push_str(&format!(r#"<feFlood flood-color="{}" result="flat"/><feComposite in="flat" in2="SourceAlpha" operator="in"/>"#, rgb_text(c)));
    }
    if let Some(sat) = look.effects.saturation {
        f.push_str(&format!(r#"<feColorMatrix type="saturate" values="{:.4}"/>"#, sat as f64 / 100_000.0));
    }
    if let Some(a) = look.effects.opacity {
        f.push_str(&format!(r#"<feComponentTransfer><feFuncA type="linear" slope="{:.4}"/></feComponentTransfer>"#, a as f64 / 100_000.0));
    }
    if let Some(edge) = look.soft_edge {
        let pt = edge as f64 / 12_700.0;
        f.push_str(&format!(r#"<feGaussianBlur stdDeviation="{:.2}"/>"#, pt));
    }
    if let Some(art) = look.artistic_effect {
        use ibis_doc_model::ArtisticEffect;
        match art {
            ArtisticEffect::Blur { radius } => {
                let r = (radius as f64).clamp(1.0, 50.0);
                f.push_str(&format!(r#"<feGaussianBlur stdDeviation="{:.1}"/>"#, r));
            }
            ArtisticEffect::FilmGrain { .. } => {
                f.push_str(r#"<feTurbulence type="fractalNoise" baseFrequency="0.8" numOctaves="3" result="noise"/><feComposite in="SourceGraphic" in2="noise" operator="arithmetic" k1="0" k2="1" k3="0.25" k4="0"/>"#);
            }
            ArtisticEffect::Glass { .. } => {
                f.push_str(r#"<feTurbulence type="turbulence" baseFrequency="0.05" numOctaves="2" result="turb"/><feDisplacementMap in2="turb" in="SourceGraphic" scale="15" xChannelSelector="R" yChannelSelector="G"/>"#);
            }
            ArtisticEffect::GlowEdges { .. } => {
                f.push_str(r#"<feConvolveMatrix order="3" kernelMatrix="-1 -1 -1 -1 8 -1 -1 -1 -1"/>"#);
            }
            ArtisticEffect::PencilGrayscale { .. } => {
                f.push_str(GRAY);
                f.push_str(r#"<feConvolveMatrix order="3" kernelMatrix="0 -1 0 -1 5 -1 0 -1 0"/>"#);
            }
            ArtisticEffect::Cutout { .. } => {
                f.push_str(r#"<feComponentTransfer><feFuncR type="discrete" tableValues="0 0.33 0.66 1"/><feFuncG type="discrete" tableValues="0 0.33 0.66 1"/><feFuncB type="discrete" tableValues="0 0.33 0.66 1"/></feComponentTransfer>"#);
            }
            ArtisticEffect::ChalkSketch { .. } | ArtisticEffect::Photocopy { .. } | ArtisticEffect::CrisscrossEtching { .. } => {
                f.push_str(GRAY);
                f.push_str(r#"<feComponentTransfer><feFuncR type="linear" slope="3" intercept="-1"/><feFuncG type="linear" slope="3" intercept="-1"/><feFuncB type="linear" slope="3" intercept="-1"/></feComponentTransfer>"#);
            }
            ArtisticEffect::Cement { .. } | ArtisticEffect::Texturizer { .. } | ArtisticEffect::MosaicBubbles { .. } | ArtisticEffect::PlasticWrap { .. } => {
                f.push_str(r#"<feTurbulence type="fractalNoise" baseFrequency="0.04" numOctaves="3" result="noise"/><feDisplacementMap in="SourceGraphic" in2="noise" scale="8" xChannelSelector="R" yChannelSelector="G"/>"#);
            }
            ArtisticEffect::GlowDiffused { .. } => {
                f.push_str(r#"<feGaussianBlur stdDeviation="3" result="blur"/><feMerge><feMergeNode in="blur"/><feMergeNode in="SourceGraphic"/></feMerge>"#);
            }
            ArtisticEffect::LineDrawing { .. } | ArtisticEffect::PencilSketch { .. } => {
                f.push_str(GRAY);
                f.push_str(r#"<feConvolveMatrix order="3" kernelMatrix="-1 -1 -1 -1 9 -1 -1 -1 -1"/>"#);
            }
            ArtisticEffect::Marker { .. } | ArtisticEffect::PaintBrush { .. } | ArtisticEffect::PaintStrokes { .. } | ArtisticEffect::PastelsSmooth { .. } | ArtisticEffect::WatercolorSponge { .. } | ArtisticEffect::LightScreen { .. } => {
                f.push_str(r#"<feComponentTransfer><feFuncR type="discrete" tableValues="0 0.25 0.5 0.75 1"/><feFuncG type="discrete" tableValues="0 0.25 0.5 0.75 1"/><feFuncB type="discrete" tableValues="0 0.25 0.5 0.75 1"/></feComponentTransfer>"#);
            }
        }
    }
    f
}

/// Where a tiled picture's tiles go: the tile's size and the place of one tile (EMU, from the frame's
/// corner), and whether alternate tiles are mirrored across and down.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TileLayout {
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    pub flip_x: bool,
    pub flip_y: bool,
}

/// Wrap a picture that fills a frame `width` × `height` EMU: cropped, put in `fill_rect` (insets from the
/// frame, 100 000ths) or tiled as `tile` says, cut to `subpaths` (the outline's closed parts, frame-relative
/// EMU) when given, and given the colour effects of `look`. The SVG covers the frame.
pub(crate) fn styled_picture(data: &[u8], encoding: Encoding, width: i64, height: i64, subpaths: Option<&[Vec<(f64, f64)>]>, crop: Crop, fill_rect: Option<[i32; 4]>, tile: Option<TileLayout>, look: &Look) -> Vec<u8> {
    let (fw_emu, fh_emu) = (width.max(1) as f64, height.max(1) as f64);
    // A tiled picture repeats across the frame, so each tile keeps the picture's own pixels; at least two
    // pixels a point (the export resolution) so upscaling does not sample adjacent atlas tiles at the edges.
    let natural_px = pixel_size(data, encoding).map(|(w, _)| match tile {
        Some(t) if t.width > 0.0 => w as f64 * (fw_emu / t.width).max(1.0),
        _ => w as f64,
    });
    let frame_px = natural_px.unwrap_or(FALLBACK_WIDTH).max(fw_emu / 6_350.0).max(16.0);
    let mut k = frame_px / fw_emu;
    let pixels = fw_emu * k * fh_emu * k;
    if pixels > MAX_PIXELS {
        k *= (MAX_PIXELS / pixels).sqrt();
    }
    let (fw, fh) = (fw_emu * k, fh_emu * k);
    let frac = |v: i32| v as f64 / 100_000.0;
    // The area the picture covers: the frame, or the fill rectangle in it.
    let [rl, rt, rr, rb] = fill_rect.unwrap_or([0; 4]);
    let (ax, ay, aw, ah) = (frac(rl) * fw, frac(rt) * fh, (1.0 - frac(rl) - frac(rr)) * fw, (1.0 - frac(rt) - frac(rb)) * fh);
    let visible_w = (1.0 - frac(crop.left) - frac(crop.right)).max(0.01);
    let visible_h = (1.0 - frac(crop.top) - frac(crop.bottom)).max(0.01);
    let (ix, iy, iw, ih) = (ax - frac(crop.left) / visible_w * aw, ay - frac(crop.top) / visible_h * ah, aw / visible_w, ah / visible_h);
    let mut clip = String::new();
    if let Some(subpaths) = subpaths {
        let mut d = String::new();
        for points in subpaths {
            for (i, (x, y)) in points.iter().enumerate() {
                d.push_str(&format!("{}{:.2} {:.2} ", if i == 0 { 'M' } else { 'L' }, x * k, y * k));
            }
            d.push_str("Z ");
        }
        clip = format!(r#"<clipPath id="shape"><path d="{d}"/></clipPath>"#);
    }
    let primitives = filter_primitives(look);
    let filter = if primitives.is_empty() { String::new() } else { format!(r#"<filter id="fx" color-interpolation-filters="sRGB" x="0" y="0" width="1" height="1">{primitives}</filter>"#) };
    let filter_attr = if filter.is_empty() { "" } else { r#" filter="url(#fx)""# };
    let mut encoded = Vec::new();
    base64(data, &mut encoded);
    let encoded = String::from_utf8(encoded).unwrap_or_default();
    let href = format!("data:{};base64,{encoded}", encoding.mime());
    let (pattern, body) = match tile {
        Some(t) => {
            let (tw, th) = (t.width * k, t.height * k);
            let (pw, ph) = (if t.flip_x { 2.0 * tw } else { tw }, if t.flip_y { 2.0 * th } else { th });
            let image = |transform: String| format!(r#"<image{filter_attr}{transform} x="0" y="0" width="{tw:.3}" height="{th:.3}" preserveAspectRatio="none" xlink:href="{href}"/>"#);
            let mut cells = image(String::new());
            if t.flip_x {
                cells += &image(format!(r#" transform="translate({:.3} 0) scale(-1 1)""#, 2.0 * tw));
            }
            if t.flip_y {
                cells += &image(format!(r#" transform="translate(0 {:.3}) scale(1 -1)""#, 2.0 * th));
            }
            if t.flip_x && t.flip_y {
                cells += &image(format!(r#" transform="translate({:.3} {:.3}) scale(-1 -1)""#, 2.0 * tw, 2.0 * th));
            }
            (
                format!(r#"<pattern id="tile" patternUnits="userSpaceOnUse" x="{:.3}" y="{:.3}" width="{pw:.3}" height="{ph:.3}">{cells}</pattern>"#, t.x * k, t.y * k),
                format!(r#"<rect x="0" y="0" width="{fw:.3}" height="{fh:.3}" fill="url(#tile)"/>"#),
            )
        }
        None => (
            format!(r#"<clipPath id="area"><rect x="{ax:.3}" y="{ay:.3}" width="{aw:.3}" height="{ah:.3}"/></clipPath>"#),
            format!(r#"<g clip-path="url(#area)"><image{filter_attr} x="{ix:.3}" y="{iy:.3}" width="{iw:.3}" height="{ih:.3}" preserveAspectRatio="none" xlink:href="{href}"/></g>"#),
        ),
    };
    format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{sw}" height="{sh}" viewBox="0 0 {fw:.3} {fh:.3}">"#,
            r#"<defs>{clip}{filter}{pattern}</defs><g{clip_attr}>{body}</g></svg>"#,
        ),
        sw = stated(fw.round().max(1.0)),
        sh = stated(fh.round().max(1.0)),
        fw = fw,
        fh = fh,
        clip = clip,
        filter = filter,
        pattern = pattern,
        clip_attr = if clip.is_empty() { "" } else { r#" clip-path="url(#shape)""# },
        body = body,
    )
    .into_bytes()
}
