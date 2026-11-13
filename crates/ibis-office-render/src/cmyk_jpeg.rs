// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A CMYK or YCCK JPEG (four components) as an sRGB picture.
//!
//! The four planes are read as they are (`zune-jpeg`, the decoder the `image` crate uses, asked for the picture's own colour
//! space), made into inks, and the CMYK converter in use turns the inks into sRGB: the ICC profile the user chose, or the open
//! model of `ibis_doc_model::OpenCmykConverter`. Photoshop and most other programs write such a JPEG with an Adobe `APP14`
//! marker and the inks inverted (255 is no ink): transform 0 is CMYK, transform 2 is YCCK (the first three planes are YCbCr
//! of the inverted inks). A picture with no Adobe marker has the inks as they are.
//!
//! The converter is sampled on a 17-point grid of the four inks once, and each pixel is the multilinear mix of the 16 nearest
//! grid points (the way printers' tables are read): the profile's conversion is slow, and a photograph has millions of pixels.

use ibis_doc_model::CmykConverter;
use image::{Rgba, RgbaImage};
use zune_core::{colorspace::ColorSpace, options::DecoderOptions};
use zune_jpeg::JpegDecoder;

/// Points of the grid on each ink.
const GRID: usize = 17;

/// The Adobe `APP14` marker's colour transform (0 CMYK, 1 YCbCr, 2 YCCK), if the picture has the marker.
fn adobe_transform(data: &[u8]) -> Option<u8> {
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        match marker {
            0xff => i += 1,
            0xd8 | 0x01 | 0xd0..=0xd7 => i += 2,
            0xda | 0xd9 => return None,
            _ => {
                let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                if marker == 0xee {
                    let payload = data.get(i + 4..i + 2 + len)?;
                    if payload.len() >= 12 && &payload[..5] == b"Adobe" {
                        return Some(payload[11]);
                    }
                }
                i += 2 + len;
            }
        }
    }
    None
}

/// The converter sampled on the grid: sRGB (0 to 255) at each point `[c][m][y][k]`.
fn sample(converter: &dyn CmykConverter) -> Vec<[f32; 3]> {
    let mut grid = Vec::with_capacity(GRID * GRID * GRID * GRID);
    let at = |i: usize| i as f64 / (GRID - 1) as f64;
    for c in 0..GRID {
        for m in 0..GRID {
            for y in 0..GRID {
                for k in 0..GRID {
                    let rgb = converter.to_rgb_fractions([at(c), at(m), at(y), at(k)]);
                    grid.push([rgb.r() as f32, rgb.g() as f32, rgb.b() as f32]);
                }
            }
        }
    }
    grid
}

/// The colour of inks `ink` (0 to 255 each) from the sampled grid.
fn lookup(grid: &[[f32; 3]], ink: [u8; 4]) -> [u8; 3] {
    let mut index = [0usize; 4];
    let mut frac = [0f32; 4];
    for d in 0..4 {
        let p = ink[d] as f32 / 255.0 * (GRID - 1) as f32;
        let i = (p.floor() as usize).min(GRID - 2);
        index[d] = i;
        frac[d] = p - i as f32;
    }
    let mut out = [0f32; 3];
    for corner in 0..16usize {
        let mut weight = 1f32;
        let mut flat = 0usize;
        for d in 0..4 {
            let bit = (corner >> d) & 1;
            weight *= if bit == 1 { frac[d] } else { 1.0 - frac[d] };
            flat = flat * GRID + index[d] + bit;
        }
        if weight > 0.0 {
            for (o, v) in out.iter_mut().zip(grid[flat]) {
                *o += weight * v;
            }
        }
    }
    out.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

/// The inks of one pixel from its four stored samples: `stored` as the file has them, whether the file's inks are inverted and
/// whether the first three are YCbCr (YCCK).
fn inks(stored: [u8; 4], inverted: bool, ycck: bool) -> [u8; 4] {
    let [a, b, c, k] = stored;
    let [x, y, z] = if ycck {
        // JFIF's YCbCr to RGB. What a decoder gives as CMYK is 255 minus this (libjpeg), and an Adobe picture's inks are 255 minus
        // that again: the ink is the converted value itself.
        let (yy, cb, cr) = (a as f32, b as f32 - 128.0, c as f32 - 128.0);
        let r = (yy + 1.402 * cr).round().clamp(0.0, 255.0) as u8;
        let g = (yy - 0.344_136 * cb - 0.714_136 * cr).round().clamp(0.0, 255.0) as u8;
        let bl = (yy + 1.772 * cb).round().clamp(0.0, 255.0) as u8;
        [r, g, bl]
    } else {
        [a, b, c]
    };
    if inverted && !ycck {
        [255 - x, 255 - y, 255 - z, 255 - k]
    } else if inverted {
        [x, y, z, 255 - k]
    } else {
        [x, y, z, k]
    }
}

/// The picture as sRGB through `converter`; `None` when it is not a four-component JPEG or cannot be decoded.
pub fn decode(data: &[u8], converter: &dyn CmykConverter) -> Option<RgbaImage> {
    let mut decoder = JpegDecoder::new_with_options(data, DecoderOptions::default());
    decoder.decode_headers().ok()?;
    let space = decoder.get_input_colorspace()?;
    if !matches!(space, ColorSpace::CMYK | ColorSpace::YCCK) {
        return None;
    }
    let (width, height) = {
        let info = decoder.info()?;
        (info.width as u32, info.height as u32)
    };
    // The picture's own colour space as the output: the four planes as they are (upsampled), no conversion.
    decoder.set_options(DecoderOptions::default().jpeg_set_out_colorspace(space));
    let planes = decoder.decode().ok()?;
    if planes.len() < width as usize * height as usize * 4 {
        return None;
    }
    let inverted = adobe_transform(data).is_some();
    let ycck = space == ColorSpace::YCCK;
    let grid = sample(converter);
    let mut out = RgbaImage::new(width, height);
    for (px, stored) in out.pixels_mut().zip(planes.chunks_exact(4)) {
        let [r, g, b] = lookup(&grid, inks([stored[0], stored[1], stored[2], stored[3]], inverted, ycck));
        *px = Rgba([r, g, b, 255]);
    }
    Some(out)
}

#[cfg(test)]
#[path = "tests/cmyk_jpeg.rs"]
mod tests;
