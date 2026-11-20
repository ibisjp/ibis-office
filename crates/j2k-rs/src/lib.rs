// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust JPEG 2000 (ISO/IEC 15444-1) encoder and decoder for codestreams
//! (`.j2k`/`.j2c`) and JP2 containers (`.jp2`).

pub mod decode;
pub mod dwt;
pub mod ebcot;
pub mod encode;
pub mod mq;
pub mod tier2;

pub use decode::{ColorSpace, DecodedImage, decode};
pub use encode::{EncodeOptions, PixelFormat, encode_j2k, encode_jp2};

/// Error returned by JPEG 2000 encoding or decoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_grayscale_j2k_losslessly_across_dimensions() {
        for (w, h) in [(1u32, 1u32), (8, 8), (17, 13), (32, 32), (71, 65)] {
            let pixels: Vec<u8> = (0..(w * h))
                .map(|i| (((i * 37) ^ (i >> 3)) & 0xFF) as u8)
                .collect();
            let j2k = encode_j2k(&pixels, w, h, PixelFormat::Gray8, &EncodeOptions::default())
                .expect("encode_j2k gray");
            assert!(j2k.starts_with(&[0xFF, 0x4F, 0xFF, 0x51]));
            assert!(j2k.ends_with(&[0xFF, 0xD9]));

            let decoded = decode(&j2k).expect("decode_j2k gray");
            assert_eq!(decoded.width, w);
            assert_eq!(decoded.height, h);
            assert_eq!(decoded.channels, 1);
            assert_eq!(decoded.data, pixels, "mismatch at {w}x{h}");
        }
    }

    #[test]
    fn roundtrips_rgb_j2k_and_jp2_losslessly() {
        let (w, h) = (29u32, 23u32);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                rgb.push(((x * 9 + y * 3) & 0xFF) as u8);
                rgb.push(((y * 11) & 0xFF) as u8);
                rgb.push((((x ^ y) * 17) & 0xFF) as u8);
            }
        }

        let j2k = encode_j2k(&rgb, w, h, PixelFormat::Rgb8, &EncodeOptions::default())
            .expect("encode_j2k rgb");
        let dec_j2k = decode(&j2k).expect("decode_j2k rgb");
        assert_eq!(dec_j2k.width, w);
        assert_eq!(dec_j2k.height, h);
        assert_eq!(dec_j2k.channels, 3);
        assert_eq!(dec_j2k.data, rgb);

        let jp2 = encode_jp2(&rgb, w, h, PixelFormat::Rgb8, &EncodeOptions::default())
            .expect("encode_jp2 rgb");
        let dec_jp2 = decode(&jp2).expect("decode_jp2 rgb");
        assert_eq!(dec_jp2.width, w);
        assert_eq!(dec_jp2.height, h);
        assert_eq!(dec_jp2.data, rgb);
    }
}
