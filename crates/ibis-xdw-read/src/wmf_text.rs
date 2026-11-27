// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! WMF text overlays stored as DocuWorks page forms.

use ibis_pdf::DisplayCmd;

use crate::scan::POINTS_PER_UNIT;
use crate::vector_emf::ansi;
use crate::{Error, PageObject, Result};

fn short(bytes: &[u8], offset: usize) -> Result<i16> {
    let value = bytes
        .get(offset..)
        .and_then(<[u8]>::first_chunk::<2>)
        .ok_or(Error::Invalid)?;
    Ok(i16::from_le_bytes(*value))
}

struct Font {
    height: f32,
    family: String,
    charset: u8,
    weight: u16,
    italic: bool,
}

pub(crate) fn render(object: &PageObject<'_>, data: &[u8]) -> Result<Vec<DisplayCmd>> {
    let (width, height) = object.size.ok_or(Error::UnsupportedDrawing)?;
    if object.rotation.rem_euclid(360) != 0 {
        return Err(Error::UnsupportedDrawing);
    }
    if !matches!(short(data, 0)?, 1 | 2) || short(data, 2)? != 9 {
        return Err(Error::UnsupportedDrawing);
    }
    let mut rest = data.get(18..).ok_or(Error::Invalid)?;
    let mut scale = None;
    let mut origin = (0.0, 0.0);
    let mut fonts = Vec::new();
    let mut font = None;
    let mut color = [0, 0, 0, 255];
    let mut transparent = false;
    let mut output = Vec::new();
    while !rest.is_empty() {
        let length = usize::try_from(crate::scan_emf::word(rest, 0)?)
            .map_err(|_| Error::Invalid)?
            .checked_mul(2)
            .ok_or(Error::Limit)?;
        if length < 6 {
            return Err(Error::Invalid);
        }
        let (record, next) = rest.split_at_checked(length).ok_or(Error::Invalid)?;
        rest = next;
        let body = &record[6..];
        match short(record, 4)? as u16 {
            0 if rest.is_empty() => return Ok(output),
            0x020b => origin = (f32::from(short(body, 2)?), f32::from(short(body, 0)?)),
            0x020c => {
                let (x, y) = (short(body, 2)?, short(body, 0)?);
                if x <= 0 || y <= 0 {
                    return Err(Error::UnsupportedDrawing);
                }
                scale = Some((
                    width as f32 * POINTS_PER_UNIT / f32::from(x),
                    height as f32 * POINTS_PER_UNIT / f32::from(y),
                ));
            }
            0x02fb => {
                if short(body, 4)? != 0 || short(body, 6)? != 0 {
                    return Err(Error::UnsupportedDrawing);
                }
                let names = body.get(18..).ok_or(Error::Invalid)?;
                let len = names.iter().position(|b| *b == 0).ok_or(Error::Invalid)?;
                let charset = body[13];
                let family = ansi(&names[..len], charset)?
                    .into_iter()
                    .map(|(ch, _)| ch)
                    .collect();
                fonts.push(Font {
                    height: f32::from(short(body, 0)?.unsigned_abs()),
                    family,
                    charset,
                    weight: short(body, 8)?.clamp(100, 900) as u16,
                    italic: body[10] != 0,
                });
                if fonts.len() > 4096 {
                    return Err(Error::Limit);
                }
            }
            0x012d => {
                font = Some(usize::try_from(short(body, 0)?).map_err(|_| Error::Invalid)?);
            }
            0x0201 => {
                let _ = crate::scan_emf::word(body, 0)?;
            }
            0x0209 => {
                let [r, g, b, _] = crate::scan_emf::word(body, 0)?.to_le_bytes();
                color = [r, g, b, 255];
            }
            0x0102 => transparent = short(body, 0)? == 1,
            0x0a32 if transparent && short(body, 6)? == 0 => {
                let (sx, sy) = scale.ok_or(Error::UnsupportedDrawing)?;
                let font = fonts
                    .get(font.ok_or(Error::UnsupportedDrawing)?)
                    .ok_or(Error::Invalid)?;
                let count = usize::from(short(body, 4)? as u16);
                if output.len().saturating_add(count) > 250_000 {
                    return Err(Error::Limit);
                }
                let encoded = body.get(8..8 + count).ok_or(Error::Invalid)?;
                let spacing = body
                    .get(8 + count.next_multiple_of(2)..)
                    .ok_or(Error::Invalid)?;
                if spacing.len() != count * 2 {
                    return Err(Error::UnsupportedDrawing);
                }
                let mut x = (object.x as f32) * POINTS_PER_UNIT
                    + (f32::from(short(body, 2)?) - origin.0) * sx;
                let y = (object.y as f32) * POINTS_PER_UNIT
                    + (f32::from(short(body, 0)?) - origin.1 + font.height * 0.88) * sy;
                let mut offset = 0;
                for (ch, units) in ansi(encoded, font.charset)? {
                    let mut advance = 0.0;
                    for _ in 0..units {
                        advance += f32::from(short(spacing, offset)?) * sx;
                        offset += 2;
                    }
                    if !ch.is_whitespace() {
                        output.push(DisplayCmd::DrawStyledGlyphRun {
                            text: ch.to_string(),
                            origin: (x, y),
                            font_size: font.height * sy,
                            family: font.family.clone(),
                            weight: font.weight,
                            italic: font.italic,
                            angle: 0.0,
                            horizontal_scale: sx / sy,
                            max_width: (advance > 0.0).then_some(advance),
                            rgba: color,
                        });
                    }
                    x += advance;
                }
            }
            _ => return Err(Error::UnsupportedDrawing),
        }
    }
    Err(Error::Invalid)
}
