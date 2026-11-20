// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pictures of Word's art page borders (`w:pgBorders` with an art
//! `ST_Border`), read from ECMA-376's `OfficeOpenXML-WordprocessingMLArtBorders.zip`
//! (5th edition, Part 1), which the app ships unmodified and hands over as
//! bytes. The zip holds `<border>_<position>.png` for eight positions: the
//! four edges and the four corners.
//!
//! A border's pictures are taken out of the zip when a document first uses
//! it, and kept.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ibis_doc_model::ArtBorder;
use ibis_ooxml::zip::Archive;

/// Where a picture of an art border goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArtPart {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl ArtPart {
    /// The position's name in the zip's file names.
    fn suffix(self) -> &'static str {
        match self {
            ArtPart::Top => "top",
            ArtPart::Bottom => "bottom",
            ArtPart::Left => "left",
            ArtPart::Right => "right",
            ArtPart::TopLeft => "topLeft",
            ArtPart::TopRight => "topRight",
            ArtPart::BottomLeft => "bottomLeft",
            ArtPart::BottomRight => "bottomRight",
        }
    }
}

/// A picture of an art border, as in the zip.
#[derive(Debug)]
pub struct ArtPicture {
    /// The PNG file, unmodified.
    pub png: Arc<[u8]>,
    pub width: u32,
    pub height: u32,
    /// Every pixel that shows is a grey (red, green and blue equal).
    pub grey: bool,
    /// No pixel shows (fully transparent): the position is not drawn.
    pub blank: bool,
    /// Line art: every pixel fully opaque and at least nine in ten white
    /// (the white kinds, a 1-pixel black outline round white; zigZagStitch,
    /// people, snowflakes and the like).
    pub line_art: bool,
}

impl ArtPicture {
    /// A picture from its PNG file (one of the zip's, served on its own to
    /// the web version), if it is an 8-bit RGBA PNG.
    pub fn from_png(png: Vec<u8>) -> Option<Self> {
        let (width, height, grey, blank, line_art) = png_info(&png)?;
        Some(Self { png: png.into(), width, height, grey, blank, line_art })
    }
}

/// The file name of `art`'s picture for `part` in the zip.
pub fn file_name(art: ArtBorder, part: ArtPart) -> Option<String> {
    let stem = match art {
        // The zip's names differ from ST_Border's for these two.
        ArtBorder::Waveline => "waveLine",
        ArtBorder::PalmsColor => "palmsColo",
        // The zip has no tribal1 to tribal6. Word draws them as these six
        // (the coverage tests' references from Word, DPGB-00343 to 00354,
        // have the same pixels as DPGB-00355 to 00366 at both sizes, but
        // for the label: compared 2026-10-05).
        ArtBorder::Tribal1 => "triangle1",
        ArtBorder::Tribal2 => "triangle2",
        ArtBorder::Tribal3 => "triangleCircle1",
        ArtBorder::Tribal4 => "triangleCircle2",
        ArtBorder::Tribal5 => "shapes1",
        ArtBorder::Tribal6 => "shapes2",
        other => other.as_str(),
    };
    Some(format!("{stem}_{}.png", part.suffix()))
}

/// The art borders' pictures, from the zip.
pub struct ArtBorderPictures {
    archive: Archive,
    index: HashMap<String, usize>,
    cache: Mutex<HashMap<(ArtBorder, ArtPart), Option<Arc<ArtPicture>>>>,
}

impl ArtBorderPictures {
    /// The pictures in `zip` (the bytes of OfficeOpenXML-WordprocessingMLArtBorders.zip).
    pub fn new(zip: Arc<[u8]>) -> Result<Self, String> {
        let archive = Archive::new(zip).map_err(|e| e.message().to_string())?;
        let index = archive.entries().iter().enumerate().map(|(i, e)| (e.name.clone(), i)).collect();
        Ok(Self { archive, index, cache: Mutex::new(HashMap::new()) })
    }

    /// The picture of `art` for `part`, if the zip has a readable one.
    pub fn picture(&self, art: ArtBorder, part: ArtPart) -> Option<Arc<ArtPicture>> {
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entry((art, part))
            .or_insert_with(|| {
                let name = file_name(art, part)?;
                let entry = &self.archive.entries()[*self.index.get(&name)?];
                let png = self.archive.read(entry).ok()?;
                ArtPicture::from_png(png).map(Arc::new)
            })
            .clone()
    }
}

/// The size of an 8-bit RGBA PNG (the zip's pictures are all such), whether
/// all its showing pixels are grey, whether none shows, and whether it is
/// line art (see `ArtPicture::line_art`).
fn png_info(png: &[u8]) -> Option<(u32, u32, bool, bool, bool)> {
    let (width, height, pixels) = decode_rgba(png)?;
    let (mut grey, mut blank, mut opaque, mut white) = (true, true, true, 0usize);
    for px in pixels.chunks_exact(4) {
        if px[3] < 255 {
            opaque = false;
        }
        if px[0] >= 250 && px[1] >= 250 && px[2] >= 250 {
            white += 1;
        }
        if px[3] > 0 {
            blank = false;
            if px[0] != px[1] || px[1] != px[2] {
                grey = false;
            }
        }
    }
    let line_art = opaque && white * 10 >= pixels.len() / 4 * 9;
    Some((width, height, grey, blank, line_art))
}

/// The pixels of an 8-bit RGBA PNG (not interlaced), row by row.
fn decode_rgba(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !png.starts_with(SIGNATURE) {
        return None;
    }
    let u32_at = |at: usize| png.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let (mut at, mut header, mut data) = (SIGNATURE.len(), None, Vec::new());
    while at + 8 <= png.len() {
        let len = u32_at(at)? as usize;
        let kind = png.get(at + 4..at + 8)?;
        let body = png.get(at + 8..at + 8 + len)?;
        match kind {
            b"IHDR" => header = Some((u32_at(at + 8)?, u32_at(at + 12)?, *body.get(8)?, *body.get(9)?, *body.get(12)?)),
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let (width, height, depth, colour, interlace) = header?;
    // 8 bits, red-green-blue-alpha, not interlaced.
    if depth != 8 || colour != 6 || interlace != 0 {
        return None;
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&data).ok()?;
    let stride = width as usize * 4;
    if raw.len() < (stride + 1) * height as usize {
        return None;
    }
    let mut out = vec![0u8; stride * height as usize];
    let mut prev = vec![0u8; stride];
    for y in 0..height as usize {
        let row = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        let filter = row[0];
        let mut line = vec![0u8; stride];
        for x in 0..stride {
            let a = if x >= 4 { line[x - 4] } else { 0 };
            let b = prev[x];
            let c = if x >= 4 { prev[x - 4] } else { 0 };
            let v = row[x + 1];
            line[x] = match filter {
                0 => v,
                1 => v.wrapping_add(a),
                2 => v.wrapping_add(b),
                3 => v.wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => {
                    let p = a as i16 + b as i16 - c as i16;
                    let (pa, pb, pc) = ((p - a as i16).abs(), (p - b as i16).abs(), (p - c as i16).abs());
                    v.wrapping_add(if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c })
                }
                _ => return None,
            };
        }
        out[y * stride..(y + 1) * stride].copy_from_slice(&line);
        prev = line;
    }
    Some((width, height, out))
}

/// `png` with its grey pixels recoloured as Word colours an art border:
/// black becomes `tint`, white stays white, and the greys between go by
/// their lightness; pixels of other colours stay as they are (doubleD's
/// orange and waveline's yellow keep their colour in Word, DPGB-00164 and
/// DPGB-00374). A new PNG (uncompressed).
pub fn recolour_greys(png: &[u8], tint: (u8, u8, u8)) -> Option<Vec<u8>> {
    let (width, height, mut pixels) = decode_rgba(png)?;
    let mix = |c: u8, l: u8| (c as u32 + (255 - c as u32) * l as u32 / 255) as u8;
    for px in pixels.chunks_exact_mut(4) {
        if px[0] == px[1] && px[1] == px[2] {
            let l = px[0];
            px[0] = mix(tint.0, l);
            px[1] = mix(tint.1, l);
            px[2] = mix(tint.2, l);
        }
    }
    Some(encode_rgba(width, height, &pixels))
}

/// `png` with its dark pixels spread `radius` pixels each way (each pixel
/// takes the darkest of the square about it, channel by channel), then
/// lightened to `ink` of their darkness (1 keeps them): a thin outline made
/// wider, to stay visible made smaller, and as light as a line thinner than
/// a pixel would come out. A new PNG (uncompressed); `png` itself is not
/// changed.
///
/// The picture is first laid on white (basicWhiteDashes' edges are a black
/// outline on transparent): the result is opaque.
pub fn spread_dark(png: &[u8], radius: u32, ink: f32) -> Option<Vec<u8>> {
    spread_thin(png, radius, ink, false)
}

/// Like `spread_dark`, but with `keep_thick` the dark areas wider than the
/// spread (basicWideOutline's bar, marquee's blocks) stay as they are, and
/// only the thin lines (their 1-pixel outlines) are spread and lightened.
pub fn spread_thin(png: &[u8], radius: u32, ink: f32, keep_thick: bool) -> Option<Vec<u8>> {
    let (width, height, mut pixels) = decode_rgba(png)?;
    for px in pixels.chunks_exact_mut(4) {
        let a = px[3] as u32;
        for c in 0..3 {
            px[c] = ((px[c] as u32 * a + 255 * (255 - a)) / 255) as u8;
        }
        px[3] = 255;
    }
    let (w, h, r) = (width as usize, height as usize, radius as usize);
    // The darkest (or lightest) of the square of side 2r + 1 about each
    // pixel, channel by channel: rows, then columns.
    let filter = |src: &[u8], darkest: bool| -> Vec<u8> {
        let pick = |it: &mut dyn Iterator<Item = u8>| if darkest { it.min() } else { it.max() }.unwrap_or(255);
        let mut rows = src.to_vec();
        for y in 0..h {
            for x in 0..w {
                let (a, b) = (x.saturating_sub(r), (x + r).min(w - 1));
                for c in 0..3 {
                    rows[(y * w + x) * 4 + c] = pick(&mut (a..=b).map(|i| src[(y * w + i) * 4 + c]));
                }
            }
        }
        let mut out = rows.clone();
        for y in 0..h {
            let (a, b) = (y.saturating_sub(r), (y + r).min(h - 1));
            for x in 0..w {
                for c in 0..3 {
                    out[(y * w + x) * 4 + c] = pick(&mut (a..=b).map(|j| rows[(j * w + x) * 4 + c]));
                }
            }
        }
        out
    };
    let light = |v: u8| (255.0 - (255.0 - v as f32) * ink).round() as u8;
    let out = if keep_thick {
        // The thick areas: what survives taking away, then giving back, r
        // pixels of darkness (an opening).
        let thick = filter(&filter(&pixels, false), true);
        let mut thin = pixels.clone();
        for (t, k) in thin.chunks_exact_mut(4).zip(thick.chunks_exact(4)) {
            for c in 0..3 {
                if k[c] < 128 {
                    t[c] = 255;
                }
            }
        }
        let spread = filter(&thin, true);
        let mut out = pixels.clone();
        for ((o, s), k) in out.chunks_exact_mut(4).zip(spread.chunks_exact(4)).zip(thick.chunks_exact(4)) {
            for c in 0..3 {
                o[c] = if k[c] < 128 { o[c] } else { light(s[c]) };
            }
        }
        out
    } else {
        let mut out = filter(&pixels, true);
        for px in out.chunks_exact_mut(4) {
            for c in 0..3 {
                px[c] = light(px[c]);
            }
        }
        out
    };
    Some(encode_rgba(width, height, &out))
}

/// An 8-bit RGBA PNG of `pixels`, its data in stored (uncompressed) blocks.
fn encode_rgba(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for row in pixels.chunks_exact(stride) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    while let Some(block) = blocks.next() {
        zlib.push(if blocks.peek().is_none() { 1 } else { 0 });
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    zlib.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], body: &[u8]| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        let crc = ibis_ooxml::zip::crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    };
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(file_name(ArtBorder::Apples, ArtPart::TopLeft).as_deref(), Some("apples_topLeft.png"));
        assert_eq!(file_name(ArtBorder::Waveline, ArtPart::Left).as_deref(), Some("waveLine_left.png"));
        assert_eq!(file_name(ArtBorder::PalmsColor, ArtPart::Bottom).as_deref(), Some("palmsColo_bottom.png"));
        assert_eq!(file_name(ArtBorder::Tribal3, ArtPart::Top).as_deref(), Some("triangleCircle1_top.png"));
    }

    /// The shipped zip has every border's eight pictures, as 8-bit RGBA PNGs.
    #[test]
    fn shipped_zip_has_the_pictures() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/data/artborders/OfficeOpenXML-WordprocessingMLArtBorders.zip");
        let zip = std::fs::read(path).expect("the art borders' zip");
        let pictures = ArtBorderPictures::new(zip.into()).unwrap();
        let apples = pictures.picture(ArtBorder::Apples, ArtPart::Top).expect("apples_top.png");
        assert_eq!((apples.width, apples.height, apples.grey, apples.blank), (660, 660, false, false));
        let dots = pictures.picture(ArtBorder::BasicBlackDots, ArtPart::TopLeft).unwrap();
        assert!(dots.grey && !dots.blank);
        assert!(pictures.picture(ArtBorder::BasicWhiteSquares, ArtPart::Top).unwrap().line_art);
        assert!(!apples.line_art && !pictures.picture(ArtBorder::Fans, ArtPart::Top).unwrap().line_art);
        // birds has no left and right edges.
        assert!(pictures.picture(ArtBorder::Birds, ArtPart::Left).unwrap().blank);
        let d = pictures.picture(ArtBorder::DoubleD, ArtPart::Top).unwrap();
        let tinted = recolour_greys(&d.png, (0xc0, 0, 0)).unwrap();
        let (w, h, px) = decode_rgba(&tinted).unwrap();
        assert_eq!((w, h), (d.width, d.height));
        // No black left; red where it was.
        assert!(px.chunks_exact(4).all(|p| p[3] == 0 || !(p[0] == 0 && p[1] == 0 && p[2] == 0)));
        assert!(px.chunks_exact(4).any(|p| p[3] > 0 && p == [0xc0, 0, 0, p[3]]));
        for &art in ArtBorder::ALL {
            for part in [ArtPart::Top, ArtPart::Bottom, ArtPart::Left, ArtPart::Right, ArtPart::TopLeft, ArtPart::TopRight, ArtPart::BottomLeft, ArtPart::BottomRight] {
                assert!(pictures.picture(art, part).is_some(), "{art:?} {part:?}");
            }
        }
    }
}
