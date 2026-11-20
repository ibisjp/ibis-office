// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust JPEG 2000 decoder wrapper over [`hayro_jpeg2000`].

use hayro_jpeg2000::{ColorSpace as HayroColorSpace, DecodeSettings, Image};

use crate::{Error, Result};

/// Decoded color space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSpace {
    Gray,
    Rgb,
    Cmyk,
    Other(u8),
}

/// Decoded 8-bit image buffer.
#[derive(Clone, Debug)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub color_space: ColorSpace,
    pub has_alpha: bool,
    pub data: Vec<u8>,
}

/// Decode a JPEG 2000 codestream (`.j2k`/`.j2c`) or container (`.jp2`) into 8-bit interleaved pixels.
pub fn decode(bytes: &[u8]) -> Result<DecodedImage> {
    let image = Image::new(bytes, &DecodeSettings::default())
        .map_err(|e| Error::new(format!("JPEG 2000 header decode failed: {e:?}")))?;
    let width = image.width();
    let height = image.height();
    let has_alpha = image.has_alpha();
    let color_space = match image.color_space() {
        HayroColorSpace::Gray => ColorSpace::Gray,
        HayroColorSpace::RGB => ColorSpace::Rgb,
        HayroColorSpace::CMYK => ColorSpace::Cmyk,
        HayroColorSpace::Unknown { num_channels } => ColorSpace::Other(*num_channels),
        _ => ColorSpace::Other(image.color_space().num_channels()),
    };
    let channels = image.color_space().num_channels() + u8::from(has_alpha);
    let data = image
        .decode()
        .map_err(|e| Error::new(format!("JPEG 2000 decode failed: {e:?}")))?;

    Ok(DecodedImage {
        width,
        height,
        channels,
        color_space,
        has_alpha,
        data,
    })
}
