// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use ibis_pdf::{DisplayCmd, ObjectId, PageInfo, PageScene, PdfRect};
use image::{ImageFormat, ImageReader, imageops};
use std::io::Cursor;

use crate::{Document, Error, Page, Result};

pub(crate) const POINTS_PER_UNIT: f32 = 72.0 / 2540.0;
const MAX_IMAGE_PIXELS: u64 = 16_777_216;

impl<B: AsRef<[u8]>> Document<B> {
    /// Build a fixed-layout scene for a blank page, a JPEG scan, or a page
    /// containing supported EMF scan or vector records.
    pub fn page_scene(&self, index: usize, page: &Page<'_>) -> Result<PageScene> {
        let index32 = u32::try_from(index).map_err(|_| Error::Limit)?;
        let object_number = index32.checked_add(1).ok_or(Error::Limit)?;
        let bounds = PdfRect {
            x0: 0.0,
            y0: 0.0,
            x1: page.width as f32 * POINTS_PER_UNIT,
            y1: page.height as f32 * POINTS_PER_UNIT,
        };
        let mut display_list = Vec::new();
        for (order, object) in page.objects.iter().enumerate() {
            let drawing = self
                .object_drawing(object)?
                .ok_or(Error::UnsupportedDrawing)?;
            let full_page = object.kind == 0x8010
                && object.x == 0
                && object.y == 0
                && object.size == Some((page.width, page.height));
            let render_vector = || {
                crate::vector_emf::render(
                    page,
                    object,
                    &drawing.bytes,
                    |picture, data, bounds, budget| {
                        crate::scan_emf::picture_command(
                            self, object, picture, data, 0, bounds, budget,
                        )
                    },
                )
            };
            let commands = match drawing.kind {
                5 if full_page => vec![decode_jpeg(&drawing.bytes, object.rotation, bounds)?],
                4 if full_page => match crate::scan_emf::render(self, object, page, &drawing.bytes)
                {
                    Err(Error::UnsupportedDrawing) => render_vector()?,
                    result => result?,
                },
                4 if object.kind != 0x8010 => render_vector()?,
                1 if object.kind == 0x802e => crate::wmf_text::render(object, &drawing.bytes)?,
                _ => return Err(Error::UnsupportedDrawing),
            };
            if display_list.len().saturating_add(commands.len()) > 250_000 {
                return Err(Error::Limit);
            }
            if order != 0 {
                display_list.push(DisplayCmd::SetClipRect(None));
            }
            display_list.extend(commands);
        }
        Ok(PageScene {
            info: PageInfo {
                index,
                object_id: ObjectId::new(object_number, 0),
                media_box: bounds,
                crop_box: bounds,
                rotate: 0,
            },
            display_list,
            text_index: Default::default(),
        })
    }
}

pub(crate) fn decode_jpeg(bytes: &[u8], rotation: i64, bounds: PdfRect) -> Result<DisplayCmd> {
    let reader = || ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg);
    let (width, height) = reader().into_dimensions().map_err(|_| Error::Invalid)?;
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err(Error::Limit);
    }
    let source = reader().decode().map_err(|_| Error::Invalid)?.into_rgba8();
    let image = match rotation.rem_euclid(360) {
        0 => source,
        90 => imageops::rotate90(&source),
        180 => imageops::rotate180(&source),
        270 => imageops::rotate270(&source),
        _ => return Err(Error::UnsupportedDrawing),
    };
    Ok(DisplayCmd::DrawImage {
        bounds,
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotated_scan_preserves_bounds_and_pixel_order() {
        let mut jpeg = Vec::new();
        let pixels = [255, 0, 0, 255, 0, 0, 0, 0, 255, 0, 0, 255];
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 100)
            .encode(&pixels, 2, 2, image::ExtendedColorType::Rgb8)
            .unwrap();
        let bounds = PdfRect {
            x0: 0.0,
            y0: 0.0,
            x1: 840.0,
            y1: 594.0,
        };
        let DisplayCmd::DrawImage {
            bounds: actual,
            rgba,
            ..
        } = decode_jpeg(&jpeg, 90, bounds).unwrap()
        else {
            panic!("expected image");
        };
        let source = ImageReader::with_format(Cursor::new(&jpeg), ImageFormat::Jpeg)
            .decode()
            .unwrap()
            .into_rgba8();
        assert_eq!(actual, bounds);
        assert_eq!(rgba, imageops::rotate90(&source).into_raw());
        assert_eq!(
            decode_jpeg(&jpeg, 45, bounds),
            Err(Error::UnsupportedDrawing)
        );
        assert_eq!(decode_jpeg(&jpeg[..5], 0, bounds), Err(Error::Invalid));
    }
}
