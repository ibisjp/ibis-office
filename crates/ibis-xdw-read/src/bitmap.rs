// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Device-independent bitmaps carried by EMF drawing records.

use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, RgbaImage, codecs::bmp::BmpDecoder};

use crate::scan_emf::{positive, word};
use crate::{Error, Result};

pub(crate) fn stretch_dib(record: &[u8]) -> Result<RgbaImage> {
    if word(record, 64)? != 0 || (word(record, 68)? as u32 >> 16) & 255 != 0xcc {
        return Err(Error::UnsupportedDrawing);
    }
    let image = dib(record, 48, 16_777_216)?;
    let (width, height) = image.dimensions();
    if word(record, 32)? != 0
        || word(record, 36)? != 0
        || positive(record, 40)? != width as usize
        || positive(record, 44)? != height as usize
    {
        return Err(Error::UnsupportedDrawing);
    }
    Ok(image)
}

pub(crate) fn dib(record: &[u8], at: usize, max_pixels: u64) -> Result<RgbaImage> {
    let subfield = |offset: usize, len: usize| {
        let start = positive(record, offset)?;
        let length = positive(record, len)?;
        record
            .get(start..start.checked_add(length).ok_or(Error::Invalid)?)
            .ok_or(Error::Invalid)
    };
    let info = subfield(at, at + 4)?;
    let bits = subfield(at + 8, at + 12)?;
    if word(info, 16)? == 2 && word(info, 14)? & 0xffff == 4 {
        return rle4(info, bits, max_pixels);
    }
    let mut dib = Vec::with_capacity(info.len() + bits.len());
    dib.extend_from_slice(info);
    dib.extend_from_slice(bits);
    let decoder =
        BmpDecoder::new_without_file_header(Cursor::new(dib)).map_err(|_| Error::Invalid)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > max_pixels {
        return Err(Error::Limit);
    }
    Ok(DynamicImage::from_decoder(decoder)
        .map_err(|_| Error::Invalid)?
        .into_rgba8())
}

fn rle4(info: &[u8], mut data: &[u8], budget: u64) -> Result<RgbaImage> {
    if word(info, 0)? != 40 {
        return Err(Error::UnsupportedDrawing);
    }
    let width = positive(info, 4)?;
    let height = positive(info, 8)?;
    let count = width.checked_mul(height).ok_or(Error::Limit)?;
    if count == 0 || count as u64 > budget {
        return Err(Error::Limit);
    }
    let entries = match positive(info, 32)? {
        0 => 16,
        n @ 1..=16 => n,
        _ => return Err(Error::Invalid),
    };
    let palette = info.get(40..40 + entries * 4).ok_or(Error::Invalid)?;
    let mut image = vec![0u8; count];
    let (mut x, mut y) = (0usize, 0usize);
    loop {
        let n = usize::from(crate::byte(&mut data)?);
        let value = crate::byte(&mut data)?;
        let (length, literal) = match (n, value) {
            (0, 0) => {
                x = 0;
                y += 1;
                continue;
            }
            (0, 1) => break,
            (0, 2) => {
                x += usize::from(crate::byte(&mut data)?);
                y += usize::from(crate::byte(&mut data)?);
                if x > width || y >= height {
                    return Err(Error::Invalid);
                }
                continue;
            }
            (0, n) => {
                let length = usize::from(n);
                let bytes = length.div_ceil(2).next_multiple_of(2);
                let (values, tail) = data.split_at_checked(bytes).ok_or(Error::Invalid)?;
                data = tail;
                (length, Some(values))
            }
            (n, _) => (n, None),
        };
        // Some writers encode the final padding nibble of an odd-width row.
        if y >= height || x > width || x + length > width.next_multiple_of(2) {
            return Err(Error::Invalid);
        }
        let row = (height - 1 - y) * width;
        for i in 0..length.min(width - x) {
            let pair = literal.map_or(value, |bytes| bytes[i / 2]);
            image[row + x + i] = if i % 2 == 0 { pair >> 4 } else { pair & 15 };
        }
        x += length;
    }
    let mut rgba = Vec::with_capacity(count * 4);
    for index in image {
        let at = usize::from(index) * 4;
        let bgra = palette.get(at..at + 4).ok_or(Error::Invalid)?;
        rgba.extend_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
    }
    RgbaImage::from_raw(width as u32, height as u32, rgba).ok_or(Error::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_odd_width_rle4_padding() {
        let mut info = [0u8; 48];
        for (offset, value) in [(0, 40i32), (4, 3), (8, 1), (32, 2)] {
            info[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        info[46] = 255;
        let image = rle4(&info, &[4, 0x10, 0, 0, 0, 1], 3).unwrap();
        assert_eq!(
            image.into_raw(),
            [255, 0, 0, 255, 0, 0, 0, 255, 255, 0, 0, 255]
        );
        assert_eq!(rle4(&info, &[5, 0x10, 0, 1], 3), Err(Error::Invalid));
    }

    #[test]
    fn decodes_bottom_up_dib_rows_from_record_offsets() {
        let mut record = vec![0; 128];
        for (offset, value) in [
            (40, 1i32),
            (44, 2),
            (48, 80),
            (52, 40),
            (56, 120),
            (60, 8),
            (68, 0x00cc_0020),
            (80, 40),
            (84, 1),
            (88, 2),
            (92, 0x0018_0001),
        ] {
            record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        record[120..].copy_from_slice(&[255, 0, 0, 0, 0, 0, 255, 0]);
        let image = stretch_dib(&record).unwrap();
        assert_eq!(image.dimensions(), (1, 2));
        assert_eq!(image.into_raw(), [255, 0, 0, 255, 0, 0, 255, 255]);
        assert_eq!(stretch_dib(&record[..127]), Err(Error::Invalid));
    }
}
