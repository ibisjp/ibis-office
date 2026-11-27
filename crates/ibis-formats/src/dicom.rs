// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust non-diagnostic DICOM (`.dcm`) in-memory anonymizer and Office
//! asset extractor for ibisOffice.
//!
//! Non-SaMD boundary:
//! - Intended strictly for academic presentations, case reports, and research
//!   documentation. Never provides primary diagnostic viewing or clinical
//!   decision support.
//! - Strips patient-identifying DICOM tags in memory before generating an
//!   anonymized `.dcm` copy, metadata summary, or uncompressed PNG preview.

use std::fmt::Write as _;
use std::io::Cursor;

use base64::Engine as _;
use dicom_object::from_reader;
pub use dicom_object::Tag;

use crate::Error;

/// Default patient-identifying DICOM tags stripped during in-memory anonymization.
pub const DEFAULT_PII_TAGS: &[(Tag, &str)] = &[
    (Tag(0x0010, 0x0010), "PatientName"),
    (Tag(0x0010, 0x0020), "PatientID"),
    (Tag(0x0010, 0x0030), "PatientBirthDate"),
    (Tag(0x0010, 0x0040), "PatientSex"),
    (Tag(0x0010, 0x1000), "OtherPatientIDs"),
    (Tag(0x0010, 0x1001), "OtherPatientNames"),
    (Tag(0x0010, 0x1040), "PatientAddress"),
    (Tag(0x0010, 0x2154), "PatientTelephoneNumbers"),
    (Tag(0x0008, 0x0050), "AccessionNumber"),
    (Tag(0x0008, 0x0080), "InstitutionName"),
    (Tag(0x0008, 0x0081), "InstitutionAddress"),
    (Tag(0x0008, 0x0090), "ReferringPhysicianName"),
    (Tag(0x0008, 0x1048), "PhysiciansOfRecord"),
    (Tag(0x0008, 0x1050), "PerformingPhysicianName"),
    (Tag(0x0008, 0x1070), "OperatorsName"),
];

/// Rectangular region of interest (in pixels) to black out in-memory before PNG embedding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedactionRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Configurable anonymization profile (can be populated from declarative JSON).
#[derive(Debug, Clone)]
pub struct DicomAnonymizationProfile {
    /// Additional `(group, element)` tags to strip alongside [`DEFAULT_PII_TAGS`].
    pub extra_strip_tags: Vec<Tag>,
    /// Strip study/series/acquisition/content dates (`(0008,0020..0023)`).
    pub strip_dates: bool,
    /// Optional pixel-space rectangles to mask out burned-in annotations.
    pub roi_masks: Vec<RedactionRect>,
}

impl Default for DicomAnonymizationProfile {
    fn default() -> Self {
        Self {
            extra_strip_tags: Vec::new(),
            strip_dates: true,
            roi_masks: Vec::new(),
        }
    }
}

impl DicomAnonymizationProfile {
    /// Parse a declarative JSON anonymization profile:
    /// `{"strip_dates": true, "extra_strip_tags": ["0008,1040"], "roi_masks": [{"x":0,"y":0,"width":100,"height":20}]}`.
    pub fn from_json_str(json: &str) -> Result<Self, Error> {
        let v: serde_json::Value = serde_json::from_str(json)
            .map_err(|e| Error::new(format!("DICOM 匿名化プロファイル JSON の解析に失敗しました: {e}")))?;
        let mut profile = Self::default();
        if let Some(b) = v.get("strip_dates").and_then(serde_json::Value::as_bool) {
            profile.strip_dates = b;
        }
        if let Some(tags) = v.get("extra_strip_tags").and_then(serde_json::Value::as_array) {
            for item in tags {
                if let Some(s) = item.as_str()
                    && let Some(tag) = parse_hex_tag(s)
                {
                    profile.extra_strip_tags.push(tag);
                }
            }
        }
        if let Some(masks) = v.get("roi_masks").and_then(serde_json::Value::as_array) {
            for m in masks {
                let x = m.get("x").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
                let y = m.get("y").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
                let width = m.get("width").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
                let height = m.get("height").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
                if width > 0 && height > 0 {
                    profile.roi_masks.push(RedactionRect { x, y, width, height });
                }
            }
        }
        Ok(profile)
    }
}

fn parse_hex_tag(s: &str) -> Option<Tag> {
    let cleaned = s.trim().trim_start_matches('(').trim_end_matches(')');
    if let Some((g, e)) = cleaned.split_once(',') {
        let group = u16::from_str_radix(g.trim().trim_start_matches("0x"), 16).ok()?;
        let elem = u16::from_str_radix(e.trim().trim_start_matches("0x"), 16).ok()?;
        return Some(Tag(group, elem));
    }
    // Eight hex digits in one run (`00081040`).
    if cleaned.len() == 8 && cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        let group = u16::from_str_radix(&cleaned[..4], 16).ok()?;
        let elem = u16::from_str_radix(&cleaned[4..], 16).ok()?;
        return Some(Tag(group, elem));
    }
    None
}

/// The tags written in `text` (`0008,1040`, `(0008,1040)` or `00081040`, separated by spaces, semicolons or new lines): the tags
/// read, and the pieces that are not a tag (the screen tells the user about those and strips only the ones read).
pub fn parse_tag_list(text: &str) -> (Vec<Tag>, Vec<String>) {
    let mut tags = Vec::new();
    let mut unreadable = Vec::new();
    for piece in text.split(|c: char| c.is_whitespace() || c == ';') {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        match parse_hex_tag(piece) {
            Some(tag) if !tags.contains(&tag) => tags.push(tag),
            Some(_) => {}
            None => unreadable.push(piece.to_string()),
        }
    }
    (tags, unreadable)
}

/// The width and height in the header of the PNG in `png`.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    Some((u32::from_be_bytes(png[16..20].try_into().ok()?), u32::from_be_bytes(png[20..24].try_into().ok()?)))
}

/// Extracted non-diagnostic DICOM asset ready for Office document embedding.
#[derive(Debug, Clone)]
pub struct DicomNonDiagnosticAsset {
    /// Re-serialized DICOM Part 10 bytes with PII tags removed.
    pub anonymized_dcm: Vec<u8>,
    /// Names of tags that were present and stripped.
    pub stripped_tags: Vec<String>,
    /// Non-identifying imaging parameters `(label, value)`.
    pub metadata_rows: Vec<(String, String)>,
    /// Uncompressed 8-bit/16-bit grayscale or 8-bit RGB PNG preview, if decodable.
    pub png_bytes: Option<Vec<u8>>,
    /// Non-fatal warnings (e.g. compressed transfer syntax fallback notice).
    pub warnings: Vec<String>,
    /// `BurnedInAnnotation (0028,0301)` says `YES`: patient information may be written into the picture's own pixels, which
    /// only a blacked-out rectangle removes (the screen asks the user to draw them).
    pub burned_in: bool,
}

/// Returns `true` if `data` has a DICOM Part 10 preamble (`"DICM"` at byte 128..132).
pub fn is_dicom(data: &[u8]) -> bool {
    data.len() >= 132 && &data[128..132] == b"DICM"
}

/// Anonymize DICOM Part 10 bytes in memory and extract non-diagnostic metadata
/// and an uncompressed PNG preview when supported.
pub fn anonymize_and_extract(
    data: &[u8],
    profile: &DicomAnonymizationProfile,
) -> Result<DicomNonDiagnosticAsset, Error> {
    if !is_dicom(data) {
        return Err(Error::new("DICOM Part 10 ヘッダ (DICM) が見つかりません"));
    }
    let mut obj = from_reader(Cursor::new(data))
        .map_err(|e| Error::new(format!("DICOM の読み込みに失敗しました: {e}")))?;

    let mut stripped_tags = Vec::new();
    for &(tag, name) in DEFAULT_PII_TAGS {
        if obj.remove_element(tag) {
            stripped_tags.push(name.to_string());
        }
    }
    if profile.strip_dates {
        for &(tag, name) in &[
            (Tag(0x0008, 0x0020), "StudyDate"),
            (Tag(0x0008, 0x0021), "SeriesDate"),
            (Tag(0x0008, 0x0022), "AcquisitionDate"),
            (Tag(0x0008, 0x0023), "ContentDate"),
        ] {
            if obj.remove_element(tag) {
                stripped_tags.push(name.to_string());
            }
        }
    }
    for &tag in &profile.extra_strip_tags {
        if obj.remove_element(tag) {
            stripped_tags.push(format!("{tag}"));
        }
    }

    let ts_uid = obj.meta().transfer_syntax().trim().to_string();
    let mut metadata_rows = Vec::new();
    for &(tag, label) in &[
        (Tag(0x0008, 0x0060), "modality"),
        (Tag(0x0008, 0x1030), "study-description"),
        (Tag(0x0008, 0x103E), "series-description"),
        (Tag(0x0018, 0x0050), "slice-thickness"),
        (Tag(0x0018, 0x0060), "kvp"),
        (Tag(0x0028, 0x0004), "photometric"),
        (Tag(0x0028, 0x0010), "rows"),
        (Tag(0x0028, 0x0011), "columns"),
        (Tag(0x0028, 0x0100), "bits-allocated"),
    ] {
        if let Ok(elem) = obj.element(tag)
            && let Ok(val) = elem.to_str()
        {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                metadata_rows.push((crate::words::text(&format!("medical.dicom.tag.{label}")), trimmed.to_string()));
            }
        }
    }
    metadata_rows.push(("TransferSyntaxUID".to_string(), ts_uid.clone()));

    let mut warnings = Vec::new();
    let mut burned_in = false;
    if let Ok(elem) = obj.element(Tag(0x0028, 0x0301))
        && let Ok(val) = elem.to_str()
        && val.trim().eq_ignore_ascii_case("YES")
    {
        burned_in = true;
        warnings.push(crate::words::text("medical.dicom.warning.burned-in"));
    }
    let png_bytes = extract_uncompressed_png(&obj, &ts_uid, &profile.roi_masks, &mut warnings);

    let mut anonymized_dcm = Vec::new();
    obj.write_all(&mut anonymized_dcm)
        .map_err(|e| Error::new(format!("匿名化 DICOM の書き出しに失敗しました: {e}")))?;

    Ok(DicomNonDiagnosticAsset {
        anonymized_dcm,
        stripped_tags,
        metadata_rows,
        png_bytes,
        warnings,
        burned_in,
    })
}

/// Convert `.dcm` bytes into a non-diagnostic, anonymized Markdown document
/// suitable for opening in ibisOffice as a Word/DOCX case-report draft.
pub fn input_to_markdown(data: &[u8]) -> Result<Option<String>, Error> {
    if !is_dicom(data) {
        return Ok(None);
    }
    let asset = anonymize_and_extract(data, &DicomAnonymizationProfile::default())?;
    Ok(Some(format_asset_markdown(&asset)))
}

/// Format an extracted [`DicomNonDiagnosticAsset`] into Markdown blocks.
pub fn format_asset_markdown(asset: &DicomNonDiagnosticAsset) -> String {
    let mut md = String::new();
    let _ = writeln!(md, "---");
    let _ = writeln!(md, "title: \"{}\"", crate::words::text("medical.dicom.title"));
    let _ = writeln!(md, "---\n");
    let _ = writeln!(md, "# {}\n", crate::words::text("medical.dicom.heading"));
    let _ = writeln!(md, "> {}\n", crate::words::text("medical.dicom.notice"));

    if !asset.stripped_tags.is_empty() {
        let _ = writeln!(
            md,
            "- {}",
            crate::words::format("medical.dicom.stripped-tags", &[("count", &asset.stripped_tags.len()), ("tags", &asset.stripped_tags.join("`, `"))])
        );
    } else {
        let _ = writeln!(md, "- {}", crate::words::text("medical.dicom.no-stripped-tags"));
    }
    for w in &asset.warnings {
        let _ = writeln!(md, "- {}", crate::words::format("medical.dicom.note", &[("text", w)]));
    }
    md.push('\n');

    if let Some(png) = &asset.png_bytes {
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        let _ = writeln!(md, "## {}\n", crate::words::text("medical.dicom.preview-heading"));
        let _ = writeln!(md, "![DICOM Anonymized Preview](data:image/png;base64,{b64})\n");
    }

    let _ = writeln!(md, "## {}\n", crate::words::text("medical.dicom.params-heading"));
    let _ = writeln!(md, "{}", crate::words::text("medical.dicom.params-head"));
    let _ = writeln!(md, "| --- | --- |");
    for (k, v) in &asset.metadata_rows {
        let _ = writeln!(
            md,
            "| {} | {} |",
            k.replace('|', "\\|"),
            v.replace('|', "\\|")
        );
    }
    md
}

fn is_uncompressed_le_ts(ts_uid: &str) -> bool {
    matches!(
        ts_uid,
        "1.2.840.10008.1.2" | "1.2.840.10008.1.2.1" | "1.2.840.10008.1.2.1.99"
    )
}

fn is_jpeg2000_ts(ts_uid: &str) -> bool {
    matches!(
        ts_uid,
        "1.2.840.10008.1.2.4.90"
            | "1.2.840.10008.1.2.4.91"
            | "1.2.840.10008.1.2.4.92"
            | "1.2.840.10008.1.2.4.93"
    )
}

fn is_jpegls_ts(ts_uid: &str) -> bool {
    matches!(ts_uid, "1.2.840.10008.1.2.4.80" | "1.2.840.10008.1.2.4.81")
}

fn is_jxl_ts(ts_uid: &str) -> bool {
    matches!(
        ts_uid,
        "1.2.840.10008.1.2.4.110" | "1.2.840.10008.1.2.4.111" | "1.2.840.10008.1.2.4.112"
    )
}

fn extract_uncompressed_png(
    obj: &dicom_object::DefaultDicomObject,
    ts_uid: &str,
    roi_masks: &[RedactionRect],
    warnings: &mut Vec<String>,
) -> Option<Vec<u8>> {
    if is_jpeg2000_ts(ts_uid) {
        return extract_jpeg2000_png(obj, roi_masks, warnings);
    }
    if is_jpegls_ts(ts_uid) {
        return extract_jpegls_png(obj, roi_masks, warnings);
    }
    if is_jxl_ts(ts_uid) {
        return extract_jxl_png(obj, roi_masks, warnings);
    }
    if !is_uncompressed_le_ts(ts_uid) {
        warnings.push(crate::words::format("medical.dicom.warning.compressed", &[("ts_uid", &ts_uid)]));
        return None;
    }

    let rows = obj.element(Tag(0x0028, 0x0010)).ok()?.to_int::<u32>().ok()?;
    let cols = obj.element(Tag(0x0028, 0x0011)).ok()?.to_int::<u32>().ok()?;
    let bits = obj
        .element(Tag(0x0028, 0x0100))
        .ok()
        .and_then(|e| e.to_int::<u32>().ok())
        .unwrap_or(8);
    let samples = obj
        .element(Tag(0x0028, 0x0002))
        .ok()
        .and_then(|e| e.to_int::<u32>().ok())
        .unwrap_or(1);
    let photometric = obj
        .element(Tag(0x0028, 0x0004))
        .ok()
        .and_then(|e| e.to_str().ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "MONOCHROME2".to_string());

    if rows == 0 || cols == 0 || rows > 8192 || cols > 8192 {
        warnings.push(crate::words::text("medical.dicom.warning.size"));
        return None;
    }

    let pixel_elem = obj.element(Tag(0x7FE0, 0x0010)).ok()?;
    let raw = pixel_elem.to_bytes().ok()?;
    let px_count = (rows as usize).checked_mul(cols as usize)?;

    let rgba: Vec<u8> = if samples == 3 && bits == 8 {
        let needed = px_count.checked_mul(3)?;
        if raw.len() < needed {
            return None;
        }
        let mut out = Vec::with_capacity(px_count * 4);
        for chunk in raw[..needed].chunks_exact(3) {
            out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
        }
        out
    } else if samples == 1 && bits == 8 {
        if raw.len() < px_count {
            return None;
        }
        let invert = photometric == "MONOCHROME1";
        let mut out = Vec::with_capacity(px_count * 4);
        for &v in &raw[..px_count] {
            let g = if invert { 255 - v } else { v };
            out.extend_from_slice(&[g, g, g, 255]);
        }
        out
    } else if samples == 1 && bits == 16 {
        let needed = px_count.checked_mul(2)?;
        if raw.len() < needed {
            return None;
        }
        let mut min_v = u16::MAX;
        let mut max_v = 0u16;
        let vals: Vec<u16> = raw[..needed]
            .chunks_exact(2)
            .map(|c| {
                let v = u16::from_le_bytes([c[0], c[1]]);
                min_v = min_v.min(v);
                max_v = max_v.max(v);
                v
            })
            .collect();
        let span = (max_v.saturating_sub(min_v)).max(1) as f32;
        let invert = photometric == "MONOCHROME1";
        let mut out = Vec::with_capacity(px_count * 4);
        for v in vals {
            let norm = (((v - min_v) as f32 / span) * 255.0).round().clamp(0.0, 255.0) as u8;
            let g = if invert { 255 - norm } else { norm };
            out.extend_from_slice(&[g, g, g, 255]);
        }
        out
    } else {
        warnings.push(crate::words::format("medical.dicom.warning.pixel-format", &[("samples", &samples), ("bits", &bits)]));
        return None;
    };

    let mut rgba = rgba;
    apply_roi_masks(&mut rgba, cols, rows, roi_masks);
    Some(encode_rgba_png(cols, rows, &rgba))
}

fn apply_roi_masks(rgba: &mut [u8], width: u32, height: u32, masks: &[RedactionRect]) {
    for mask in masks {
        let x0 = mask.x.min(width);
        let y0 = mask.y.min(height);
        let x1 = mask.x.saturating_add(mask.width).min(width);
        let y1 = mask.y.saturating_add(mask.height).min(height);
        for y in y0..y1 {
            for x in x0..x1 {
                let off = ((y as usize * width as usize) + x as usize) * 4;
                if off + 4 <= rgba.len() {
                    rgba[off..off + 4].copy_from_slice(&[0, 0, 0, 255]);
                }
            }
        }
    }
}

fn extract_jpeg2000_png(
    obj: &dicom_object::DefaultDicomObject,
    roi_masks: &[RedactionRect],
    warnings: &mut Vec<String>,
) -> Option<Vec<u8>> {
    let Some(j2k_bytes) = read_encapsulated_or_raw_pixel_bytes(obj) else {
        warnings.push(crate::words::text("medical.dicom.warning.j2k-unreadable"));
        return None;
    };

    let decoded = match j2k_rs::decode(&j2k_bytes) {
        Ok(d) => d,
        Err(e) => {
            warnings.push(crate::words::format("medical.dicom.warning.j2k-decode", &[("error", &e)]));
            return None;
        }
    };
    let px_count = (decoded.width as usize).checked_mul(decoded.height as usize)?;
    let rgba = match decoded.channels {
        1 => {
            let mut out = Vec::with_capacity(px_count * 4);
            for &g in decoded.data.iter().take(px_count) {
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        3 => {
            let mut out = Vec::with_capacity(px_count * 4);
            for chunk in decoded.data.chunks_exact(3).take(px_count) {
                out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            out
        }
        4 => decoded.data,
        ch => {
            warnings.push(crate::words::format("medical.dicom.warning.j2k-channels", &[("count", &ch)]));
            return None;
        }
    };
    let mut rgba = rgba;
    apply_roi_masks(&mut rgba, decoded.width, decoded.height, roi_masks);
    Some(encode_rgba_png(decoded.width, decoded.height, &rgba))
}

fn read_encapsulated_or_raw_pixel_bytes(obj: &dicom_object::DefaultDicomObject) -> Option<Vec<u8>> {
    let pixel_elem = obj.element(Tag(0x7FE0, 0x0010)).ok()?;
    if let Some(frags) = pixel_elem.fragments() {
        let mut combined = Vec::new();
        for frag in frags {
            combined.extend_from_slice(frag);
        }
        Some(combined)
    } else if let Ok(raw) = pixel_elem.to_bytes() {
        Some(raw.into_owned())
    } else {
        None
    }
}

fn extract_jpegls_png(
    obj: &dicom_object::DefaultDicomObject,
    roi_masks: &[RedactionRect],
    warnings: &mut Vec<String>,
) -> Option<Vec<u8>> {
    let Some(jls_bytes) = read_encapsulated_or_raw_pixel_bytes(obj) else {
        warnings.push(crate::words::text("medical.dicom.warning.jls-unreadable"));
        return None;
    };
    let decoded = match jpegls::decode_image(&jls_bytes) {
        Ok(d) => d,
        Err(e) => {
            warnings.push(crate::words::format("medical.dicom.warning.jls-decode", &[("error", &e)]));
            return None;
        }
    };
    let px_count = (decoded.width as usize).checked_mul(decoded.height as usize)?;
    let shift = decoded.bits_per_sample.saturating_sub(8);
    let to_u8 = |s: u16| -> u8 { (s >> shift).min(255) as u8 };
    let rgba = match decoded.components {
        1 => {
            let mut out = Vec::with_capacity(px_count * 4);
            for &s in decoded.samples.iter().take(px_count) {
                let g = to_u8(s);
                out.extend_from_slice(&[g, g, g, 255]);
            }
            out
        }
        3 if decoded.samples.len() >= px_count * 3 => {
            let mut out = Vec::with_capacity(px_count * 4);
            let r = &decoded.samples[..px_count];
            let g = &decoded.samples[px_count..px_count * 2];
            let b = &decoded.samples[px_count * 2..px_count * 3];
            for i in 0..px_count {
                out.extend_from_slice(&[to_u8(r[i]), to_u8(g[i]), to_u8(b[i]), 255]);
            }
            out
        }
        ch => {
            warnings.push(crate::words::format("medical.dicom.warning.jls-channels", &[("count", &ch)]));
            return None;
        }
    };
    let mut rgba = rgba;
    apply_roi_masks(&mut rgba, decoded.width, decoded.height, roi_masks);
    let mut rgba = rgba;
    apply_roi_masks(&mut rgba, decoded.width, decoded.height, roi_masks);
    Some(encode_rgba_png(decoded.width, decoded.height, &rgba))
}

fn extract_jxl_png(
    obj: &dicom_object::DefaultDicomObject,
    roi_masks: &[RedactionRect],
    warnings: &mut Vec<String>,
) -> Option<Vec<u8>> {
    let Some(jxl_bytes) = read_encapsulated_or_raw_pixel_bytes(obj) else {
        warnings.push(crate::words::text("medical.dicom.warning.jxl-unreadable"));
        return None;
    };
    let image = match jxl_oxide::JxlImage::builder().read(Cursor::new(&jxl_bytes)) {
        Ok(img) => img,
        Err(e) => {
            warnings.push(crate::words::format("medical.dicom.warning.jxl-header", &[("error", &e)]));
            return None;
        }
    };
    let render = match image.render_frame(0) {
        Ok(r) => r,
        Err(e) => {
            warnings.push(crate::words::format("medical.dicom.warning.jxl-frame", &[("error", &e)]));
            return None;
        }
    };
    let fb = render.image_all_channels();
    let w = fb.width() as u32;
    let h = fb.height() as u32;
    let ch = fb.channels();
    let buf = fb.buf();
    let px_count = (w as usize).checked_mul(h as usize)?;
    let to_u8 = |v: f32| -> u8 { (v.clamp(0.0, 1.0) * 255.0).round() as u8 };

    let mut rgba = Vec::with_capacity(px_count * 4);
    match ch {
        1 => {
            for &v in buf.iter().take(px_count) {
                let g = to_u8(v);
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        3 => {
            for px in buf.chunks_exact(3).take(px_count) {
                rgba.extend_from_slice(&[to_u8(px[0]), to_u8(px[1]), to_u8(px[2]), 255]);
            }
        }
        4 => {
            for px in buf.chunks_exact(4).take(px_count) {
                rgba.extend_from_slice(&[to_u8(px[0]), to_u8(px[1]), to_u8(px[2]), to_u8(px[3])]);
            }
        }
        other => {
            warnings.push(crate::words::format("medical.dicom.warning.jxl-channels", &[("count", &other)]));
            return None;
        }
    }
    apply_roi_masks(&mut rgba, w, h, roi_masks);
    Some(encode_rgba_png(w, h, &rgba))
}

/// Anonymize a DICOM file in memory and embed its PNG preview directly onto a
/// PowerPoint slide (`slide_idx`) as a [`ShapeKind::Picture`] shape, returning
/// the extracted [`DicomNonDiagnosticAsset`].
pub fn embed_dicom_into_pptx_slide(
    presentation: &mut ibis_doc_model::Presentation,
    slide_idx: usize,
    dcm_bytes: &[u8],
    profile: &DicomAnonymizationProfile,
) -> Result<DicomNonDiagnosticAsset, Error> {
    use ibis_doc_model::{EMU_PER_POINT, EmuRect, MediaFormat, Picture, Shape, ShapeKind};

    let asset = anonymize_and_extract(dcm_bytes, profile)?;
    let Some(png) = asset.png_bytes.clone() else {
        return Err(Error::new(
            "この DICOM ファイルからはプレビュー PNG を生成できないため、スライドへ埋め込めませんでした",
        ));
    };
    if slide_idx >= presentation.slides.len() {
        return Err(Error::new(format!(
            "スライド番号 {slide_idx} が範囲外です（総スライド数: {}）",
            presentation.slides.len()
        )));
    }
    let media_id = presentation.add_media(MediaFormat::Png, png);
    let shape_id = presentation.new_shape_id();
    let mut pic = Picture::new(media_id);
    pic.name = "DICOM Anonymized Asset".to_string();
    pic.alt_text.description =
        "Non-diagnostic anonymized DICOM asset (PII tags stripped in memory)".to_string();

    let frame = EmuRect::new(
        72 * EMU_PER_POINT,
        72 * EMU_PER_POINT,
        360 * EMU_PER_POINT,
        360 * EMU_PER_POINT,
    );
    presentation.slides[slide_idx]
        .shapes
        .push(Shape::new(shape_id, frame, ShapeKind::Picture(pic)));
    Ok(asset)
}

/// Anonymize a DICOM file in memory and append its non-diagnostic Markdown
/// blocks (disclaimer, anonymized PNG preview, imaging parameter table) into an
/// existing Word (`FlowDocument`) presentation.
pub fn embed_dicom_into_docx_flow(
    presentation: &mut ibis_doc_model::Presentation,
    dcm_bytes: &[u8],
    profile: &DicomAnonymizationProfile,
) -> Result<DicomNonDiagnosticAsset, Error> {
    let asset = anonymize_and_extract(dcm_bytes, profile)?;
    let md = format_asset_markdown(&asset);
    let parsed = crate::markdown::open_markdown(md.into_bytes())?;
    let Some(new_flow) = parsed.presentation.flow else {
        return Err(Error::new("DICOM ブロックの DOCX 変換に失敗しました"));
    };
    if let Some(existing_flow) = presentation.flow.as_mut() {
        std::sync::Arc::make_mut(existing_flow)
            .body
            .extend(new_flow.body.iter().cloned());
    } else {
        presentation.flow = Some(new_flow);
    }
    Ok(asset)
}

/// Minimal pure-Rust RGBA PNG encoder using `miniz_oxide` (no extra crate deps).
fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw_scanlines = Vec::with_capacity((width as usize * 4 + 1) * height as usize);
    let stride = width as usize * 4;
    for y in 0..height as usize {
        raw_scanlines.push(0); // filter type 0 (None)
        raw_scanlines.extend_from_slice(&rgba[y * stride..(y + 1) * stride]);
    }
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&raw_scanlines, 6);

    let mut png = Vec::with_capacity(64 + compressed.len());
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&width.to_be_bytes());
    ihdr[4..8].copy_from_slice(&height.to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 6; // color type RGBA
    write_png_chunk(&mut png, b"IHDR", &ihdr);
    write_png_chunk(&mut png, b"IDAT", &compressed);
    write_png_chunk(&mut png, b"IEND", &[]);
    png
}

fn write_png_chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(tag);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use dicom_core::{PrimitiveValue, VR};
    use dicom_object::StandardDataDictionary;
    use dicom_object::mem::{InMemDicomObject, InMemElement};
    use dicom_object::meta::FileMetaTableBuilder;

    fn build_synthetic_dicom(ts_uid: &str, with_pixels: bool) -> Vec<u8> {
        let mut obj = InMemDicomObject::new_empty_with_dict(StandardDataDictionary);
        // Patient-identifying tags (should be stripped)
        obj.put(InMemElement::new(
            Tag(0x0010, 0x0010),
            VR::PN,
            "YAMADA^TARO",
        ));
        obj.put(InMemElement::new(
            Tag(0x0010, 0x0020),
            VR::LO,
            "PAT-999999",
        ));
        obj.put(InMemElement::new(
            Tag(0x0008, 0x0080),
            VR::LO,
            "SYNTHETIC TEST HOSPITAL",
        ));
        // Non-identifying imaging parameters (should be kept)
        obj.put(InMemElement::new(Tag(0x0008, 0x0060), VR::CS, "CT"));
        obj.put(InMemElement::new(Tag(0x0018, 0x0060), VR::DS, "120"));
        obj.put(InMemElement::new(
            Tag(0x0028, 0x0002),
            VR::US,
            PrimitiveValue::from(1u16),
        ));
        obj.put(InMemElement::new(
            Tag(0x0028, 0x0004),
            VR::CS,
            "MONOCHROME2",
        ));
        obj.put(InMemElement::new(
            Tag(0x0028, 0x0010),
            VR::US,
            PrimitiveValue::from(4u16),
        ));
        obj.put(InMemElement::new(
            Tag(0x0028, 0x0011),
            VR::US,
            PrimitiveValue::from(4u16),
        ));
        obj.put(InMemElement::new(
            Tag(0x0028, 0x0100),
            VR::US,
            PrimitiveValue::from(8u16),
        ));
        if with_pixels {
            let pixels: Vec<u8> = (0u8..16).map(|i| i * 16).collect();
            obj.put(InMemElement::new(
                Tag(0x7FE0, 0x0010),
                VR::OB,
                PrimitiveValue::U8(pixels.into()),
            ));
        }

        let file_obj = obj
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax(ts_uid)
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.2")
                    .media_storage_sop_instance_uid("1.2.3.4.5.6.7.8.9"),
            )
            .expect("with_meta");
        let mut out = Vec::new();
        file_obj.write_all(&mut out).expect("write_all");
        out
    }

    #[test]
    fn anonymizes_pii_and_extracts_png_and_markdown() {
        let dcm = build_synthetic_dicom("1.2.840.10008.1.2.1", true);
        assert!(is_dicom(&dcm));
        assert!(!is_dicom(b"NOT A DICOM"));

        let asset = anonymize_and_extract(&dcm, &DicomAnonymizationProfile::default())
            .expect("anonymize_and_extract");
        assert!(asset.stripped_tags.contains(&"PatientName".to_string()));
        assert!(asset.stripped_tags.contains(&"PatientID".to_string()));
        assert!(asset.stripped_tags.contains(&"InstitutionName".to_string()));

        // Re-read anonymized_dcm and verify PII is completely absent.
        let reloaded = from_reader(Cursor::new(&asset.anonymized_dcm)).expect("reload");
        assert!(reloaded.element(Tag(0x0010, 0x0010)).is_err());
        assert!(reloaded.element(Tag(0x0010, 0x0020)).is_err());
        assert!(reloaded.element(Tag(0x0008, 0x0080)).is_err());
        assert_eq!(
            reloaded
                .element(Tag(0x0008, 0x0060))
                .unwrap()
                .to_str()
                .unwrap()
                .trim(),
            "CT"
        );

        let png = asset.png_bytes.expect("should decode uncompressed 8-bit PNG");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

        let md = input_to_markdown(&dcm).unwrap().unwrap();
        assert!(!md.contains("YAMADA"));
        assert!(!md.contains("PAT-999999"));
        assert!(!md.contains("SYNTHETIC TEST HOSPITAL"));
        assert!(md.contains("data:image/png;base64,"));
        assert!(md.contains("CT"));

        let opened = crate::markdown::open_markdown(md.into_bytes()).expect("open_markdown");
        assert!(opened.presentation.flow.is_some());
    }

    #[test]
    fn decodes_jpeg2000_dicom_in_pure_rust_and_falls_back_for_unsupported_ts() {
        // 1. JPEG 2000 Lossless (1.2.840.10008.1.2.4.90) encoded with j2k_rs
        let pixels: Vec<u8> = (0u8..16).map(|i| i * 16).collect();
        let j2k = j2k_rs::encode_j2k(
            &pixels,
            4,
            4,
            j2k_rs::PixelFormat::Gray8,
            &j2k_rs::EncodeOptions::default(),
        )
        .expect("encode_j2k");

        let mut obj = InMemDicomObject::new_empty_with_dict(StandardDataDictionary);
        obj.put(InMemElement::new(Tag(0x0010, 0x0010), VR::PN, "J2K^PATIENT"));
        obj.put(InMemElement::new(Tag(0x0008, 0x0060), VR::CS, "CT"));
        obj.put(InMemElement::new(Tag(0x0028, 0x0002), VR::US, PrimitiveValue::from(1u16)));
        obj.put(InMemElement::new(Tag(0x0028, 0x0010), VR::US, PrimitiveValue::from(4u16)));
        obj.put(InMemElement::new(Tag(0x0028, 0x0011), VR::US, PrimitiveValue::from(4u16)));
        obj.put(InMemElement::new(Tag(0x0028, 0x0100), VR::US, PrimitiveValue::from(8u16)));
        obj.put(InMemElement::new(
            Tag(0x7FE0, 0x0010),
            VR::OB,
            dicom_core::value::PixelFragmentSequence::new(vec![], vec![j2k]),
        ));
        let file_obj = obj
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax("1.2.840.10008.1.2.4.90")
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.2")
                    .media_storage_sop_instance_uid("1.2.3.4.5.6.7.8.90"),
            )
            .unwrap();
        let mut j2k_dcm = Vec::new();
        file_obj.write_all(&mut j2k_dcm).unwrap();

        let asset = anonymize_and_extract(&j2k_dcm, &DicomAnonymizationProfile::default())
            .expect("should decode JPEG 2000 DICOM");
        assert!(asset.stripped_tags.contains(&"PatientName".to_string()));
        let png = asset.png_bytes.expect("should produce PNG from JPEG 2000 DICOM");
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));

        // 2. JPEG-LS Lossless (1.2.840.10008.1.2.4.80) encoded with pure_jpegls
        let jls_samples: Vec<u16> = (0u16..16).map(|i| i * 16).collect();
        let mut jls = Vec::new();
        jpegls::encode_image(&jls_samples, 4, 4, 1, &jpegls::EncodeOptions::default(), &mut jls)
            .expect("encode jpegls");
        let mut obj_jls = InMemDicomObject::new_empty_with_dict(StandardDataDictionary);
        obj_jls.put(InMemElement::new(Tag(0x0010, 0x0010), VR::PN, "JLS^PATIENT"));
        obj_jls.put(InMemElement::new(Tag(0x0008, 0x0060), VR::CS, "CT"));
        obj_jls.put(InMemElement::new(
            Tag(0x7FE0, 0x0010),
            VR::OB,
            dicom_core::value::PixelFragmentSequence::new(Vec::<u32>::new(), vec![jls]),
        ));
        let mut jls_dcm = Vec::new();
        obj_jls
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax("1.2.840.10008.1.2.4.80")
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.2")
                    .media_storage_sop_instance_uid("1.2.3.4.5.6.7.8.80"),
            )
            .unwrap()
            .write_all(&mut jls_dcm)
            .unwrap();
        let asset_jls = anonymize_and_extract(&jls_dcm, &DicomAnonymizationProfile::default())
            .expect("should decode JPEG-LS DICOM");
        assert!(asset_jls.stripped_tags.contains(&"PatientName".to_string()));
        assert!(asset_jls.png_bytes.expect("JPEG-LS PNG").starts_with(b"\x89PNG\r\n\x1a\n"));

        // 3. JPEG-XL Lossless (1.2.840.10008.1.2.4.110) decoded with jxl-oxide
        let minimal_jxl: Vec<u8> = vec![
            0xff, 0x0a, 0x30, 0x54, 0x10, 0x09, 0x08, 0x06, 0x01, 0x00, 0x78, 0x00,
            0x4b, 0x38, 0x41, 0x3c, 0xb6, 0x3a, 0x51, 0xfe, 0x00, 0x47, 0x1e, 0xa0,
            0x85, 0xb8, 0x27, 0x1a, 0x48, 0x45, 0x84, 0x1b, 0x71, 0x4f, 0xa8, 0x3e,
            0x8e, 0x30, 0x03, 0x92, 0x84, 0x01,
        ];
        let mut obj_jxl = InMemDicomObject::new_empty_with_dict(StandardDataDictionary);
        obj_jxl.put(InMemElement::new(Tag(0x0010, 0x0010), VR::PN, "JXL^PATIENT"));
        obj_jxl.put(InMemElement::new(Tag(0x0008, 0x0060), VR::CS, "MR"));
        obj_jxl.put(InMemElement::new(
            Tag(0x7FE0, 0x0010),
            VR::OB,
            dicom_core::value::PixelFragmentSequence::new(Vec::<u32>::new(), vec![minimal_jxl]),
        ));
        let mut jxl_dcm = Vec::new();
        obj_jxl
            .with_meta(
                FileMetaTableBuilder::new()
                    .transfer_syntax("1.2.840.10008.1.2.4.110")
                    .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.4")
                    .media_storage_sop_instance_uid("1.2.3.4.5.6.7.8.110"),
            )
            .unwrap()
            .write_all(&mut jxl_dcm)
            .unwrap();
        let asset_jxl = anonymize_and_extract(&jxl_dcm, &DicomAnonymizationProfile::default())
            .expect("should decode JPEG-XL DICOM");
        assert!(asset_jxl.stripped_tags.contains(&"PatientName".to_string()));
        assert!(asset_jxl.png_bytes.expect("JPEG-XL PNG").starts_with(b"\x89PNG\r\n\x1a\n"));

        // 4. Unsupported compressed syntax (MPEG2 1.2.840.10008.1.2.4.100)
        let dcm = build_synthetic_dicom("1.2.840.10008.1.2.4.100", false);
        let asset = anonymize_and_extract(&dcm, &DicomAnonymizationProfile::default())
            .expect("should succeed with metadata fallback");
        assert!(asset.png_bytes.is_none());
        assert!(!asset.warnings.is_empty());
        assert!(asset.stripped_tags.contains(&"PatientName".to_string()));
    }

    #[test]
    fn json_profile_roi_mask_and_direct_pptx_docx_embedding() {
        let profile = DicomAnonymizationProfile::from_json_str(
            r#"{"strip_dates": true, "extra_strip_tags": ["0008,1030"], "roi_masks": [{"x": 0, "y": 0, "width": 2, "height": 2}]}"#,
        )
        .expect("from_json_str");
        assert!(profile.extra_strip_tags.contains(&Tag(0x0008, 0x1030)));
        assert_eq!(profile.roi_masks.len(), 1);

        let dcm = build_synthetic_dicom("1.2.840.10008.1.2.1", true);

        // Embed directly into a blank PPTX slide
        let blank_pptx = crate::blank::blank_file(crate::Kind::Presentation).unwrap();
        let mut pptx = crate::save::open(blank_pptx).unwrap();
        let before_shapes = pptx.presentation.slides[0].shapes.len();
        embed_dicom_into_pptx_slide(&mut pptx.presentation, 0, &dcm, &profile)
            .expect("embed into pptx");
        assert_eq!(pptx.presentation.slides[0].shapes.len(), before_shapes + 1);
        assert!(matches!(
            pptx.presentation.slides[0].shapes.last().unwrap().kind,
            ibis_doc_model::ShapeKind::Picture(_)
        ));

        // Embed directly into a blank DOCX flow
        let blank_docx = crate::blank::blank_file(crate::Kind::Document).unwrap();
        let mut docx = crate::save::open(blank_docx).unwrap();
        embed_dicom_into_docx_flow(&mut docx.presentation, &dcm, &profile)
            .expect("embed into docx");
        assert!(docx.presentation.flow.as_ref().unwrap().body.len() > 2);
    }
}
