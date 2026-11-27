// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The restricted EMF dialect used to place JPEG strips on a scanned page.

use ibis_pdf::{DisplayCmd, PdfRect};

use crate::scan::{POINTS_PER_UNIT, decode_jpeg};
use crate::{Document, Error, Page, PageObject, Result, field, number};

pub(crate) fn word(bytes: &[u8], offset: usize) -> Result<i32> {
    let word = bytes
        .get(offset..)
        .and_then(<[u8]>::first_chunk::<4>)
        .ok_or(Error::Invalid)?;
    Ok(i32::from_le_bytes(*word))
}

pub(crate) fn positive(bytes: &[u8], offset: usize) -> Result<usize> {
    usize::try_from(word(bytes, offset)?).map_err(|_| Error::Invalid)
}

pub(crate) fn render(
    document: &Document<impl AsRef<[u8]>>,
    object: &PageObject<'_>,
    page: &Page<'_>,
    emf: &[u8],
) -> Result<Vec<DisplayCmd>> {
    let header_size = positive(emf, 4)?;
    let header = emf.get(..header_size).ok_or(Error::Invalid)?;
    if word(header, 0)? != 1 || header.get(40..44) != Some(b" EMF") {
        return Err(Error::Invalid);
    }
    let rotation = object.rotation.rem_euclid(360);
    let (w, h) = (page.width as f32, page.height as f32);
    let (unrotated_w, unrotated_h) = if matches!(rotation, 90 | 270) {
        (h, w)
    } else {
        (w, h)
    };
    if word(header, 24)? != 0
        || word(header, 28)? != 0
        || word(header, 32)? as f32 != unrotated_w
        || word(header, 36)? as f32 != unrotated_h
    {
        return Err(Error::UnsupportedDrawing);
    }
    let (device_x, device_y) = (positive(header, 72)?, positive(header, 76)?);
    let (mm_x, mm_y) = (positive(header, 80)?, positive(header, 84)?);
    if [device_x, device_y, mm_x, mm_y].contains(&0) {
        return Err(Error::Invalid);
    }
    let fx = (100 * mm_x as u64) as f32 / device_x as f32;
    let fy = (100 * mm_y as u64) as f32 / device_y as f32;
    let mut rest = &emf[header_size..];
    let mut picture = 300u32;
    let mut selected = false;
    let mut output = Vec::new();
    let mut decoded_pixels = 16_777_216u64;
    while !rest.is_empty() {
        let length = positive(rest, 4)?;
        if length < 8 || length % 4 != 0 {
            return Err(Error::Invalid);
        }
        let (record, tail) = rest.split_at_checked(length).ok_or(Error::Invalid)?;
        rest = tail;
        match word(record, 0)? {
            14 if rest.is_empty() && !selected && !output.is_empty() => return Ok(output),
            21 if word(record, 8)? == 3 => {}
            70 => {
                let size = positive(record, 8)?;
                let data = record.get(12..12 + size).ok_or(Error::Invalid)?;
                match data.get(..4) {
                    Some(b"DWb\0") if !selected && picture < 400 => {
                        picture += 1;
                        selected = true;
                    }
                    Some(b"DWc\0") if selected => {
                        let [x, y, width, height] = [
                            word(data, 20)? as f32 * fx,
                            word(data, 24)? as f32 * fy,
                            word(data, 52)? as f32 * fx,
                            word(data, 56)? as f32 * fy,
                        ];
                        if width <= 0.0 || height <= 0.0 {
                            return Err(Error::UnsupportedDrawing);
                        }
                        let [x0, y0, x1, y1] = match rotation {
                            0 => [x, y, x + width, y + height],
                            90 => [w - y - height, x, w - y, x + width],
                            180 => [w - x - width, h - y - height, w - x, h - y],
                            270 => [y, h - x - width, y + height, h - x],
                            _ => return Err(Error::UnsupportedDrawing),
                        }
                        .map(|v| v * POINTS_PER_UNIT);
                        let bounds = PdfRect { x0, y0, x1, y1 };
                        output.push(picture_command(
                            document,
                            object,
                            picture,
                            data,
                            rotation,
                            bounds,
                            &mut decoded_pixels,
                        )?);
                        selected = false;
                    }
                    _ => return Err(Error::UnsupportedDrawing),
                }
            }
            _ => return Err(Error::UnsupportedDrawing),
        }
    }
    Err(Error::Invalid)
}

pub(crate) fn picture_command(
    document: &Document<impl AsRef<[u8]>>,
    object: &PageObject<'_>,
    picture: u32,
    data: &[u8],
    rotation: i64,
    bounds: PdfRect,
    budget: &mut u64,
) -> Result<DisplayCmd> {
    if word(data, 28)? != 0 || word(data, 32)? != 0 || (word(data, 48)? as u32 >> 16) & 255 != 0xcc
    {
        return Err(Error::UnsupportedDrawing);
    }
    let reference = field(object.attributes, picture)?.ok_or(Error::Invalid)?;
    let raw = document.entry_body(number(reference.value, 1)?)?;
    let pw = positive(raw, 8)?;
    let ph = positive(raw, 12)?;
    if positive(raw, 0)? != raw.len()
        || word(raw, 4)? != 0
        || positive(data, 36)? != pw
        || positive(data, 40)? != ph
    {
        return Err(Error::UnsupportedDrawing);
    }
    *budget = budget
        .checked_sub((pw as u64) * (ph as u64))
        .ok_or(Error::Limit)?;
    let command = decode_jpeg(raw.get(16..).ok_or(Error::Invalid)?, rotation, bounds)?;
    let DisplayCmd::DrawImage { width, height, .. } = &command else {
        unreachable!()
    };
    let actual = if matches!(rotation, 90 | 270) {
        (*height, *width)
    } else {
        (*width, *height)
    };
    if actual != (pw as u32, ph as u32) {
        return Err(Error::Invalid);
    }
    Ok(command)
}
