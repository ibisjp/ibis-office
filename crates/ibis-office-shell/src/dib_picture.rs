// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Windows bitmaps (BMP files and the DIBs the clipboard holds: Print Screen, Alt+Print Screen) as RGBA pixels.
//!
//! A screenshot reaches the clipboard as `CF_DIB` only (no PNG). Its layout varies: BITMAPINFOHEADER or the V4 / V5
//! headers, 24 or 32 bits, `BI_RGB` or `BI_BITFIELDS` (masks after a 40-byte header, or inside a V4 / V5 header),
//! bottom-up or top-down (negative height), a 32-bit alpha that is all zero. The pixel offset is computed here from
//! the header, not taken from the file header the clipboard layer put in front of it.

/// RGBA pixels of a BMP (with its 14-byte file header), `(width, height, rgba)`. `None` for what is not decoded
/// (RLE, JPEG or PNG inside a BMP, a size that does not fit the bytes).
pub(crate) fn bmp_to_rgba(bmp: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if !bmp.starts_with(b"BM") {
        return None;
    }
    dib_to_rgba(bmp.get(14..)?)
}

/// RGBA pixels of a DIB (the bytes after the file header: header, masks, palette, pixels).
pub(crate) fn dib_to_rgba(dib: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let u16_at = |i: usize| Some(u16::from_le_bytes(dib.get(i..i + 2)?.try_into().ok()?) as u32);
    let u32_at = |i: usize| Some(u32::from_le_bytes(dib.get(i..i + 4)?.try_into().ok()?));
    let header = u32_at(0)? as usize;
    // BITMAPCOREHEADER (12): 16-bit sizes, 3-byte palette entries, no compression.
    let core = header == 12;
    if !core && header < 40 {
        return None;
    }
    let (width, signed_height, bits, compression, colors_used) = if core {
        (u16_at(4)? as i64, u16_at(6)? as i32, u16_at(10)?, 0, 0)
    } else {
        (u32_at(4)? as i32 as i64, u32_at(8)? as i32, u16_at(14)?, u32_at(16)?, u32_at(32)?)
    };
    if width <= 0 || signed_height == 0 {
        return None;
    }
    let top_down = signed_height < 0;
    let (width, height) = (width as usize, signed_height.unsigned_abs() as usize);
    if width > 1 << 16 || height > 1 << 16 {
        return None;
    }
    // BI_RGB 0, BI_BITFIELDS 3, BI_ALPHABITFIELDS 6. (RLE 1 / 2, JPEG 4, PNG 5 are not drawn here.)
    if !matches!(compression, 0 | 3 | 6) || !matches!(bits, 1 | 4 | 8 | 16 | 24 | 32) {
        return None;
    }
    if compression != 0 && !matches!(bits, 16 | 32) {
        return None;
    }
    // Masks: inside a V4 / V5 header (offset 40), else three (or four for BI_ALPHABITFIELDS) after a 40-byte header.
    let mut masks = [0u32; 4];
    let mut after_header = header;
    if compression != 0 {
        if header >= 52 {
            // BITMAPV2INFOHEADER (52) has three masks, V3 and later four.
            for (k, mask) in masks.iter_mut().enumerate().take(if header >= 56 { 4 } else { 3 }) {
                *mask = u32_at(40 + 4 * k)?;
            }
        } else {
            let count = if compression == 6 { 4 } else { 3 };
            for (k, mask) in masks.iter_mut().enumerate().take(count) {
                *mask = u32_at(40 + 4 * k)?;
            }
            after_header = 40 + 4 * count;
        }
    }
    if compression == 0 && bits == 16 {
        masks = [0x7c00, 0x03e0, 0x001f, 0];
    }
    if compression == 0 && bits == 32 {
        masks = [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0];
    }
    // Palette (up to 8 bits): biClrUsed entries, else 2^bits.
    let palette_len = if bits <= 8 {
        (if colors_used == 0 { 1usize << bits } else { colors_used as usize }).min(1 << bits)
    } else {
        0
    };
    let entry = if core { 3 } else { 4 };
    let palette_at = after_header;
    let pixels_at = palette_at + palette_len * entry;
    let stride = (width * bits as usize).div_ceil(32) * 4;
    let rows = dib.get(pixels_at..pixels_at.checked_add(stride.checked_mul(height)?)?)?;
    let palette: Vec<[u8; 3]> = (0..palette_len)
        .map(|k| {
            let at = palette_at + k * entry;
            [dib[at + 2], dib[at + 1], dib[at]]
        })
        .collect();
    let channel = |value: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 255;
        }
        let shift = mask.trailing_zeros();
        let max = mask >> shift;
        (((value & mask) >> shift) as u64 * 255 / max as u64) as u8
    };
    let mut rgba = vec![0u8; width * height * 4];
    for y in 0..height {
        let src = if top_down { y } else { height - 1 - y };
        let row = &rows[src * stride..(src + 1) * stride];
        for x in 0..width {
            let out = (y * width + x) * 4;
            let px: [u8; 4] = match bits {
                1 | 4 | 8 => {
                    let per = 8 / bits as usize;
                    let byte = row[x / per];
                    let shift = 8 - bits as usize * (x % per + 1);
                    let index = ((byte >> shift) as usize) & ((1 << bits) - 1);
                    let [r, g, b] = palette.get(index).copied().unwrap_or([0, 0, 0]);
                    [r, g, b, 255]
                }
                16 => {
                    let v = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]) as u32;
                    [channel(v, masks[0]), channel(v, masks[1]), channel(v, masks[2]), if masks[3] == 0 { 255 } else { channel(v, masks[3]) }]
                }
                24 => [row[x * 3 + 2], row[x * 3 + 1], row[x * 3], 255],
                _ => {
                    let v = u32::from_le_bytes([row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]]);
                    [channel(v, masks[0]), channel(v, masks[1]), channel(v, masks[2]), if masks[3] == 0 { 255 } else { channel(v, masks[3]) }]
                }
            };
            rgba[out..out + 4].copy_from_slice(&px);
        }
    }
    // A screenshot's 32-bit pixels carry no alpha (the fourth byte is 0 for every pixel): it is opaque, not clear.
    if matches!(bits, 16 | 32) && masks[3] != 0 && rgba.chunks_exact(4).all(|p| p[3] == 0) {
        rgba.chunks_exact_mut(4).for_each(|p| p[3] = 255);
    }
    Some((width as u32, height as u32, rgba))
}

/// Pure-Rust RFC 2083 PNG encoder.
pub fn rgba_to_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xffff_ffffu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], data: &[u8]) {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(crc(&body).to_be_bytes());
    }

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.push(8); // bit depth = 8
    ihdr.push(6); // color type = 6 (RGBA)
    ihdr.push(0); // compression = deflate
    ihdr.push(0); // filter = standard
    ihdr.push(0); // interlace = none
    chunk(&mut out, b"IHDR", &ihdr);

    let row_len = (w * 4) as usize;
    let mut raw = Vec::with_capacity((h as usize) * (row_len + 1));
    for y in 0..h as usize {
        raw.push(0); // filter: None
        let start = y * row_len;
        if start + row_len <= rgba.len() {
            raw.extend_from_slice(&rgba[start..start + row_len]);
        } else {
            raw.resize(raw.len() + row_len, 0);
        }
    }

    // zlib stream with non-compressed deflate blocks
    let mut z = vec![0x78, 0x01];
    for (i, block) in raw.chunks(65_535).enumerate() {
        let last = (i + 1) * 65_535 >= raw.len();
        z.push(last as u8);
        z.extend((block.len() as u16).to_le_bytes());
        z.extend((!(block.len() as u16)).to_le_bytes());
        z.extend_from_slice(block);
    }
    // Adler-32
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &raw {
        a = (a + x as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend(((b << 16) | a).to_be_bytes());

    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DIB: `header` as BITMAPINFOHEADER (40) or V5 (124), `masks` after the header (only when `header == 40` and
    /// compression 3), rows bottom-up unless `height` is negative.
    fn dib(header: u32, bits: u16, compression: u32, width: i32, height: i32, masks: &[u32], palette: &[[u8; 4]], rows: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(header.to_le_bytes());
        out.extend(width.to_le_bytes());
        out.extend(height.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(bits.to_le_bytes());
        out.extend(compression.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend([0u8; 8]);
        out.extend((palette.len() as u32).to_le_bytes()); // biClrUsed
        out.extend(0u32.to_le_bytes());
        if header >= 108 {
            for k in 0..4 {
                out.extend(masks.get(k).copied().unwrap_or(0).to_le_bytes());
            }
            out.extend(b"BGRs");
            out.resize(header as usize, 0);
        } else if compression == 3 {
            for m in masks {
                out.extend(m.to_le_bytes());
            }
        }
        for p in palette {
            out.extend(p);
        }
        let stride = (width as usize * bits as usize).div_ceil(32) * 4;
        for row in rows {
            let mut r = row.clone();
            r.resize(stride, 0);
            out.extend(r);
        }
        out
    }
    fn with_file_header(dib: &[u8]) -> Vec<u8> {
        let mut bmp = b"BM".to_vec();
        bmp.extend((14 + dib.len() as u32).to_le_bytes());
        bmp.extend([0u8; 4]);
        bmp.extend(0u32.to_le_bytes()); // an offset nobody should trust
        bmp.extend(dib);
        bmp
    }
    // Two rows, two columns: red green / blue white (top row first).
    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const WHITE: [u8; 4] = [255; 4];
    fn expected() -> Vec<u8> {
        [RED, GREEN, BLUE, WHITE].concat()
    }
    fn bgr(p: [u8; 4]) -> [u8; 3] {
        [p[2], p[1], p[0]]
    }
    fn bgrx(p: [u8; 4], x: u8) -> [u8; 4] {
        [p[2], p[1], p[0], x]
    }

    #[test]
    fn rgb24_bottom_up_with_row_padding() {
        let rows = vec![[bgr(BLUE), bgr(WHITE)].concat(), [bgr(RED), bgr(GREEN)].concat()];
        let d = dib(40, 24, 0, 2, 2, &[], &[], &rows);
        assert_eq!(dib_to_rgba(&d), Some((2, 2, expected())));
        assert_eq!(bmp_to_rgba(&with_file_header(&d)), Some((2, 2, expected())));
    }

    #[test]
    fn rgb24_top_down_negative_height() {
        let rows = vec![[bgr(RED), bgr(GREEN)].concat(), [bgr(BLUE), bgr(WHITE)].concat()];
        let d = dib(40, 24, 0, 2, -2, &[], &[], &rows);
        assert_eq!(dib_to_rgba(&d), Some((2, 2, expected())));
    }

    #[test]
    fn rgb32_alpha_byte_zero_is_opaque() {
        let rows = vec![[bgrx(BLUE, 0), bgrx(WHITE, 0)].concat(), [bgrx(RED, 0), bgrx(GREEN, 0)].concat()];
        let d = dib(40, 32, 0, 2, 2, &[], &[], &rows);
        assert_eq!(dib_to_rgba(&d), Some((2, 2, expected())));
    }

    #[test]
    fn bitfields32_masks_after_a_40_byte_header() {
        let masks = [0x00ff_0000, 0x0000_ff00, 0x0000_00ff];
        let rows = vec![[bgrx(BLUE, 0), bgrx(WHITE, 0)].concat(), [bgrx(RED, 0), bgrx(GREEN, 0)].concat()];
        let d = dib(40, 32, 3, 2, 2, &masks, &[], &rows);
        assert_eq!(dib_to_rgba(&d), Some((2, 2, expected())));
        // The file header's offset (0 here, and wrong in a clipboard layer that miscounts masks) is not used.
        assert_eq!(bmp_to_rgba(&with_file_header(&d)), Some((2, 2, expected())));
    }

    #[test]
    fn bitfields32_v5_header_holds_its_masks_and_zero_alpha_is_opaque() {
        let masks = [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000];
        let rows = vec![[bgrx(RED, 0), bgrx(GREEN, 0)].concat(), [bgrx(BLUE, 0), bgrx(WHITE, 0)].concat()];
        let d = dib(124, 32, 3, 2, -2, &masks, &[], &rows);
        assert_eq!(dib_to_rgba(&d), Some((2, 2, expected())));
    }

    #[test]
    fn v5_alpha_is_kept_when_it_is_there() {
        let masks = [0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000];
        let rows = vec![[bgrx(RED, 128), bgrx(GREEN, 255)].concat()];
        let d = dib(124, 32, 3, 2, -1, &masks, &[], &rows);
        let (_, _, rgba) = dib_to_rgba(&d).unwrap();
        assert_eq!([rgba[3], rgba[7]], [128, 255]);
    }

    #[test]
    fn bitfields16_565() {
        let masks = [0xf800, 0x07e0, 0x001f];
        let px = |r: u16, g: u16, b: u16| ((r << 11) | (g << 5) | b).to_le_bytes().to_vec();
        let rows = vec![[px(31, 0, 0), px(0, 63, 0)].concat()];
        let d = dib(40, 16, 3, 2, -1, &masks, &[], &rows);
        let (_, _, rgba) = dib_to_rgba(&d).unwrap();
        assert_eq!(rgba, [RED, GREEN].concat());
    }

    #[test]
    fn palette_8_and_4_and_1_bit() {
        let palette = [[0, 0, 255, 0], [0, 255, 0, 0], [255, 0, 0, 0], [255, 255, 255, 0]]; // BGRx: red green blue white
        let d8 = dib(40, 8, 0, 2, -2, &[], &palette, &[vec![0, 1], vec![2, 3]]);
        assert_eq!(dib_to_rgba(&d8), Some((2, 2, expected())));
        let d4 = dib(40, 4, 0, 2, -2, &[], &palette, &[vec![0x01], vec![0x23]]);
        assert_eq!(dib_to_rgba(&d4), Some((2, 2, expected())));
        let d1 = dib(40, 1, 0, 2, -1, &[], &palette[..2], &[vec![0b0100_0000]]);
        assert_eq!(dib_to_rgba(&d1).unwrap().2, [RED, GREEN].concat());
    }

    #[test]
    fn what_does_not_fit_is_refused() {
        let rows = vec![bgr(RED).to_vec()];
        let d = dib(40, 24, 0, 2, 2, &[], &[], &rows); // pixels for one row, header says two
        assert_eq!(dib_to_rgba(&d), None);
        assert_eq!(dib_to_rgba(&[0; 10]), None);
        let mut rle = dib(40, 8, 1, 2, 2, &[], &[], &[]);
        rle.resize(100, 0);
        assert_eq!(dib_to_rgba(&rle), None);
    }
}
