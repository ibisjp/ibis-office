// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ISO/IEC 15444-1 JPEG 2000 codestream (`.j2k`/`.j2c`) and JP2 box (`.jp2`) encoder.

use crate::dwt::{Subband, forward_53_2d, forward_rct};
use crate::ebcot::{BandOrient, EncodedCodeBlock, encode_codeblock};
use crate::tier2::write_packet;
use crate::{Error, Result};

/// Pixel format for encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// 8-bit grayscale (`1` byte per pixel).
    Gray8,
    /// 8-bit RGB (`3` bytes per pixel, interleaved).
    Rgb8,
}

/// Options for JPEG 2000 encoding.
#[derive(Clone, Copy, Debug)]
pub struct EncodeOptions {
    /// Requested DWT decomposition levels (`0..=5`, clamped to image size).
    pub decomp_levels: u8,
    /// Code-block dimension exponent (`4` => `16x16`, `5` => `32x32`, `6` => `64x64`).
    pub codeblock_exp: u8,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            decomp_levels: 0,
            codeblock_exp: 6, // 64x64
        }
    }
}

/// Encode an 8-bit grayscale or RGB image into a lossless JPEG 2000 codestream (`.j2k`).
pub fn encode_j2k(
    pixels: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    options: &EncodeOptions,
) -> Result<Vec<u8>> {
    if width == 0 || height == 0 {
        return Err(Error::new("image width and height must be non-zero"));
    }
    let px_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| Error::new("image dimensions overflow"))?;
    let channels = match format {
        PixelFormat::Gray8 => 1usize,
        PixelFormat::Rgb8 => 3usize,
    };
    let expected = px_count
        .checked_mul(channels)
        .ok_or_else(|| Error::new("pixel buffer size overflow"))?;
    if pixels.len() != expected {
        return Err(Error::new(format!(
            "expected {expected} bytes for {width}x{height} {format:?}, got {}",
            pixels.len()
        )));
    }

    let decomp_levels = (width.min(height).ilog2()).min(options.decomp_levels as u32) as u8;
    let cb_exp = options.codeblock_exp.clamp(2, 6);
    let cb_size = 1usize << cb_exp;
    let guard_bits: u8 = 2;

    // 1. Level shift & optional RCT
    let comp_planes: Vec<Vec<i32>> = match format {
        PixelFormat::Gray8 => {
            vec![pixels.iter().map(|&p| p as i32 - 128).collect()]
        }
        PixelFormat::Rgb8 => {
            let mut r = Vec::with_capacity(px_count);
            let mut g = Vec::with_capacity(px_count);
            let mut b = Vec::with_capacity(px_count);
            for chunk in pixels.chunks_exact(3) {
                r.push(chunk[0]);
                g.push(chunk[1]);
                b.push(chunk[2]);
            }
            let (y, cb, cr) = forward_rct(&r, &g, &b);
            vec![y, cb, cr]
        }
    };

    // 2. Forward 5-3 DWT per component
    let comp_subbands: Vec<Vec<Subband>> = comp_planes
        .iter()
        .map(|plane| forward_53_2d(plane, width as usize, height as usize, decomp_levels))
        .collect();

    // 3. Tier-1 encode every subband's code-blocks
    // subband_encoded[comp][sb_idx] = (cb_cols, cb_rows, Vec<EncodedCodeBlock>)
    let mut subband_encoded: Vec<Vec<(usize, usize, Vec<EncodedCodeBlock>)>> =
        Vec::with_capacity(channels);
    for sbs in &comp_subbands {
        let mut sb_list = Vec::with_capacity(sbs.len());
        for sb in sbs {
            let gain: u32 = match sb.orientation {
                0 => 0,
                1 | 2 => 1,
                _ => 2,
            };
            let epsilon_b = 8 + gain;
            let mb = (guard_bits as u32) + epsilon_b - 1;
            let cb_cols = sb.width.div_ceil(cb_size);
            let cb_rows = sb.height.div_ceil(cb_size);
            let mut cbs = Vec::with_capacity(cb_cols * cb_rows);
            for cby in 0..cb_rows {
                for cbx in 0..cb_cols {
                    let x0 = cbx * cb_size;
                    let y0 = cby * cb_size;
                    let cw = (sb.width - x0).min(cb_size);
                    let ch = (sb.height - y0).min(cb_size);
                    let mut block = Vec::with_capacity(cw * ch);
                    for y in y0..(y0 + ch) {
                        let row_off = y * sb.width + x0;
                        block.extend_from_slice(&sb.data[row_off..row_off + cw]);
                    }
                    cbs.push(encode_codeblock(
                        &block,
                        cw,
                        ch,
                        BandOrient::from_u8(sb.orientation),
                        mb,
                    ));
                }
            }
            sb_list.push((cb_cols, cb_rows, cbs));
        }
        subband_encoded.push(sb_list);
    }

    // 4. Tier-2 packets in LRCP order (1 layer, 1 + decomp_levels resolutions, channels)
    let mut tile_data = Vec::new();
    let num_res = 1 + decomp_levels as usize;
    for r in 0..num_res {
        for c in 0..channels {
            let packet = if r == 0 {
                let (cols, rows, ref cbs) = subband_encoded[c][0];
                write_packet(&[(cols, rows, cbs.as_slice())])
            } else {
                let base = 1 + (r - 1) * 3;
                let (c1, r1, ref b1) = subband_encoded[c][base];
                let (c2, r2, ref b2) = subband_encoded[c][base + 1];
                let (c3, r3, ref b3) = subband_encoded[c][base + 2];
                write_packet(&[
                    (c1, r1, b1.as_slice()),
                    (c2, r2, b2.as_slice()),
                    (c3, r3, b3.as_slice()),
                ])
            };
            tile_data.extend_from_slice(&packet);
        }
    }

    // 5. Assemble J2K codestream markers
    let mut out = Vec::with_capacity(256 + tile_data.len());
    // SOC
    out.extend_from_slice(&[0xFF, 0x4F]);
    // SIZ
    write_siz(&mut out, width, height, channels as u16);
    // COD
    write_cod(
        &mut out,
        decomp_levels,
        cb_exp,
        matches!(format, PixelFormat::Rgb8),
    );
    // QCD
    write_qcd(&mut out, decomp_levels, guard_bits);
    // SOT (Lsot = 10, Isot = 0, Psot = 14 + tile_data.len(), TPsot = 0, TNsot = 1)
    // Psot includes SOT marker (2) + Lsot payload (10) + SOD marker (2) + tile_data.
    let psot = 14u32 + tile_data.len() as u32;
    out.extend_from_slice(&[0xFF, 0x90]);
    out.extend_from_slice(&10u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&psot.to_be_bytes());
    out.push(0); // TPsot
    out.push(1); // TNsot
    // SOD
    out.extend_from_slice(&[0xFF, 0x93]);
    out.extend_from_slice(&tile_data);
    // EOC
    out.extend_from_slice(&[0xFF, 0xD9]);
    Ok(out)
}

/// Encode an 8-bit grayscale or RGB image into a lossless JP2 file container (`.jp2`).
pub fn encode_jp2(
    pixels: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
    options: &EncodeOptions,
) -> Result<Vec<u8>> {
    let codestream = encode_j2k(pixels, width, height, format, options)?;
    let channels: u16 = match format {
        PixelFormat::Gray8 => 1,
        PixelFormat::Rgb8 => 3,
    };
    let enum_cs: u32 = match format {
        PixelFormat::Gray8 => 17, // greyscale
        PixelFormat::Rgb8 => 16,  // sRGB
    };

    let mut out = Vec::with_capacity(128 + codestream.len());
    // JP2 Signature box
    out.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
    ]);
    // File Type box ('ftyp')
    write_box(&mut out, *b"ftyp", b"jp2 \0\0\0\0jp2 ");

    // JP2 Header superbox ('jp2h')
    let mut ihdr = Vec::with_capacity(14);
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&channels.to_be_bytes());
    ihdr.push(7); // BPC: 8-bit unsigned (value - 1)
    ihdr.push(7); // Compression type: 7 = JPEG 2000
    ihdr.push(0); // UnkC
    ihdr.push(0); // IPR

    let mut colr = Vec::with_capacity(7);
    colr.push(1); // METH = 1 (Enumerated Colourspace)
    colr.push(0); // PREC
    colr.push(0); // APPROX
    colr.extend_from_slice(&enum_cs.to_be_bytes());

    let mut jp2h_payload = Vec::new();
    write_box(&mut jp2h_payload, *b"ihdr", &ihdr);
    write_box(&mut jp2h_payload, *b"colr", &colr);
    write_box(&mut out, *b"jp2h", &jp2h_payload);

    // Contiguous Codestream box ('jp2c')
    write_box(&mut out, *b"jp2c", &codestream);
    Ok(out)
}

fn write_box(out: &mut Vec<u8>, box_type: [u8; 4], payload: &[u8]) {
    let len = (8 + payload.len()) as u32;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&box_type);
    out.extend_from_slice(payload);
}

fn write_siz(out: &mut Vec<u8>, width: u32, height: u32, csiz: u16) {
    let lsiz = 38 + 3 * csiz;
    out.extend_from_slice(&[0xFF, 0x51]);
    out.extend_from_slice(&lsiz.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // Rsiz
    out.extend_from_slice(&width.to_be_bytes()); // Xsiz
    out.extend_from_slice(&height.to_be_bytes()); // Ysiz
    out.extend_from_slice(&0u32.to_be_bytes()); // XOsiz
    out.extend_from_slice(&0u32.to_be_bytes()); // YOsiz
    out.extend_from_slice(&width.to_be_bytes()); // XTsiz
    out.extend_from_slice(&height.to_be_bytes()); // YTsiz
    out.extend_from_slice(&0u32.to_be_bytes()); // XTOsiz
    out.extend_from_slice(&0u32.to_be_bytes()); // YTOsiz
    out.extend_from_slice(&csiz.to_be_bytes()); // Csiz
    for _ in 0..csiz {
        out.push(7); // Ssiz: 8-bit unsigned
        out.push(1); // XRsiz
        out.push(1); // YRsiz
    }
}

fn write_cod(out: &mut Vec<u8>, decomp_levels: u8, cb_exp: u8, mct: bool) {
    out.extend_from_slice(&[0xFF, 0x52]);
    out.extend_from_slice(&12u16.to_be_bytes()); // Lcod
    out.push(0); // Scod
    out.push(0); // Progression order: LRCP
    out.extend_from_slice(&1u16.to_be_bytes()); // Number of layers: 1
    out.push(u8::from(mct)); // Multiple component transform
    out.push(decomp_levels); // Decomposition levels
    out.push(cb_exp - 2); // Code-block width exponent - 2
    out.push(cb_exp - 2); // Code-block height exponent - 2
    out.push(0); // Code-block style
    out.push(1); // Transformation: 5-3 reversible
}

fn write_qcd(out: &mut Vec<u8>, decomp_levels: u8, guard_bits: u8) {
    let num_subbands = 1 + 3 * decomp_levels as usize;
    let lqcd = (3 + num_subbands) as u16;
    out.extend_from_slice(&[0xFF, 0x5C]);
    out.extend_from_slice(&lqcd.to_be_bytes());
    out.push(guard_bits << 5); // Sqcd: no quantization (0) + guard bits
    // LL subband (gain = 0 -> epsilon = 8)
    out.push(8u8 << 3);
    // Detail subbands (HL, LH: gain = 1 -> 9; HH: gain = 2 -> 10)
    for _ in 0..decomp_levels {
        out.push(9u8 << 3);
        out.push(9u8 << 3);
        out.push(10u8 << 3);
    }
}
