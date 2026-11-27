// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A picture file a tool inserts (design 5, 6.2 and 6.4): read from an allowed folder, PNG or JPEG, with its size in pixels.

use ibis_doc_model::MediaFormat;

use crate::error::ToolError;
use crate::paths;
use crate::service::Service;

/// The biggest picture file a tool inserts.
const MAX_PICTURE_BYTES: u64 = 25 * 1024 * 1024;

pub(crate) struct PictureFile {
    pub format: MediaFormat,
    pub data: Vec<u8>,
    pub pixels: (u32, u32),
}

pub(crate) fn read(service: &Service, path: &str) -> Result<PictureFile, ToolError> {
    let real = paths::resolve(&service.allow_dirs, path, &["png", "jpg", "jpeg"], true)?;
    let size = std::fs::metadata(&real).map_err(|e| ToolError::new("file_not_found", e.to_string()))?.len();
    if size > MAX_PICTURE_BYTES {
        return Err(ToolError::new("file_too_large", format!("{size} bytes; a picture is {MAX_PICTURE_BYTES} bytes at most")));
    }
    let data = std::fs::read(&real).map_err(|e| ToolError::new("file_not_found", e.to_string()))?;
    let (format, pixels) = sniff(&data).ok_or_else(|| ToolError::new("unsupported_format", "the file is not a PNG or JPEG picture"))?;
    Ok(PictureFile { format, data, pixels })
}

/// The format of the bytes (by what they start with, not by the extension) and the width and height in pixels.
pub(crate) fn sniff(data: &[u8]) -> Option<(MediaFormat, (u32, u32))> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") && data.len() >= 24 && &data[12..16] == b"IHDR" {
        let w = u32::from_be_bytes(data[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(data[20..24].try_into().ok()?);
        return (w > 0 && h > 0).then_some((MediaFormat::Png, (w, h)));
    }
    if data.starts_with(&[0xFF, 0xD8]) {
        // The first start-of-frame marker holds the height and the width.
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xFF {
                return None;
            }
            let marker = data[i + 1];
            if marker == 0xFF {
                i += 1;
                continue;
            }
            if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
                i += 2;
                continue;
            }
            let length = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                return (w > 0 && h > 0).then_some((MediaFormat::Jpeg, (w, h)));
            }
            i += 2 + length;
        }
    }
    None
}

/// A size in points from the width and height asked for (either, both or neither) and the picture's pixels (96 per inch, so 0.75 point each).
pub(crate) fn size_points(width: Option<f64>, height: Option<f64>, pixels: (u32, u32)) -> (f64, f64) {
    let (pw, ph) = (pixels.0 as f64, pixels.1 as f64);
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w * ph / pw),
        (None, Some(h)) => (h * pw / ph, h),
        (None, None) => (pw * 0.75, ph * 0.75),
    }
}

/// A length argument in points: above 0 and not over 5000.
pub(crate) fn length_arg(value: Option<&serde_json::Value>, name: &str) -> Result<Option<f64>, ToolError> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|n| *n > 0.0 && *n <= 5000.0).map(Some).ok_or_else(|| ToolError::invalid(format!("{name} is points, above 0 and at most 5000"))),
    }
}
