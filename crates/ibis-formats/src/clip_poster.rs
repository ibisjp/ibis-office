// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The picture a new video or audio clip shows on its slide (the poster
//! frame, the speaker icon): drawn here as a PNG, since a clip's own frames
//! are not read. A dark frame with a play triangle for a video, a speaker
//! for an audio clip.

use miniz_oxide::deflate::compress_to_vec_zlib;

/// The PNG of the poster of a video of `aspect` (width / height), or of an
/// audio clip's icon, and its size in pixels.
pub fn clip_poster(audio: bool, aspect: f64) -> (Vec<u8>, u32, u32) {
    let (w, h) = if audio { (96, 96) } else {
        let aspect = if aspect.is_finite() && aspect > 0.2 && aspect < 5.0 { aspect } else { 16.0 / 9.0 };
        (480, ((480.0 / aspect).round() as u32).max(2))
    };
    let mut pixels = vec![[0x26u8, 0x26, 0x26, 0xff]; (w * h) as usize];
    let mut set = |x: i64, y: i64, c: [u8; 4]| {
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            pixels[(y as u32 * w + x as u32) as usize] = c;
        }
    };
    let white = [0xff, 0xff, 0xff, 0xff];
    if audio {
        // A speaker: a box, a cone, and two arcs of waves.
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let (fx, fy) = (x as f64 - 48.0, y as f64 - 48.0);
                let in_box = (-30.0..-14.0).contains(&fx) && fy.abs() < 9.0;
                let cone = (-14.0..4.0).contains(&fx) && fy.abs() < 9.0 + (fx + 14.0) * 1.2;
                let r = (fx * fx + fy * fy).sqrt();
                let wave = fx > 6.0 && ((r > 16.0 && r < 20.0) || (r > 27.0 && r < 31.0)) && fy.abs() < fx.abs() * 1.1;
                if in_box || cone || wave {
                    set(x, y, white);
                }
            }
        }
    } else {
        // A play triangle in a circle, in the middle.
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        let radius = (h as f64 * 0.22).max(12.0);
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let (fx, fy) = (x as f64 - cx, y as f64 - cy);
                let r = (fx * fx + fy * fy).sqrt();
                if (radius - 3.0..=radius).contains(&r) {
                    set(x, y, white);
                }
                // The triangle: pointing right, a little right of the middle.
                let tx = fx + radius * 0.3;
                if tx > -radius * 0.5 && tx < radius * 0.5 && fy.abs() < (radius * 0.5 - tx) * 0.9 + 0.0 {
                    set(x, y, white);
                }
            }
        }
    }
    (png(w, h, &pixels), w, h)
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xedb8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend(data);
    out.extend(&body);
    out.extend(crc32(&body).to_be_bytes());
}

fn png(w: u32, h: u32, pixels: &[[u8; 4]]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((w as usize * 4 + 1) * h as usize);
    for row in pixels.chunks(w as usize) {
        raw.push(0);
        for p in row {
            raw.extend(p);
        }
    }
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut header = Vec::new();
    header.extend(w.to_be_bytes());
    header.extend(h.to_be_bytes());
    header.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// A PNG representation of a captured screenshot or window snapshot.
pub fn screenshot_snapshot(w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let mut pixels = vec![[0xf4u8, 0xf6, 0xf8, 0xff]; (w * h) as usize];
    // Draw a subtle window titlebar (e.g. height 28px) and border
    let bar_h = 28.min(h);
    for y in 0..bar_h {
        for x in 0..w {
            pixels[(y * w + x) as usize] = [0xdd, 0xe1, 0xe7, 0xff];
        }
    }
    // 3 small window buttons (close, minimize, maximize)
    for (btn_x, color) in [(14, [0xed, 0x6a, 0x5e, 0xff]), (26, [0xf4, 0xbf, 0x4f, 0xff]), (38, [0x61, 0xc5, 0x54, 0xff])] {
        for dy in -3..=3 {
            for dx in -3..=3 {
                if dx * dx + dy * dy <= 9 {
                    let py = 14 + dy;
                    let px = btn_x + dx;
                    if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                        pixels[(py as u32 * w + px as u32) as usize] = color;
                    }
                }
            }
        }
    }
    // Window border
    for x in 0..w {
        pixels[x as usize] = [0xc8, 0xcd, 0xd4, 0xff];
        pixels[((h - 1) * w + x) as usize] = [0xc8, 0xcd, 0xd4, 0xff];
    }
    for y in 0..h {
        pixels[(y * w) as usize] = [0xc8, 0xcd, 0xd4, 0xff];
        pixels[(y * w + w - 1) as usize] = [0xc8, 0xcd, 0xd4, 0xff];
    }
    (png(w, h, &pixels), w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posters_are_valid_pngs_of_the_aspect() {
        let (data, w, h) = clip_poster(false, 4.0 / 3.0);
        assert_eq!((w, h), (480, 360));
        assert_eq!(&data[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        let (_, w, h) = clip_poster(true, 1.0);
        assert_eq!((w, h), (96, 96));
        // crc32 of "IEND" is the PNG's known value.
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
    }
}
