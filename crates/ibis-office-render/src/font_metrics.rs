// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! How high the Office apps make a line of text and where they put its
//! baseline, shared by the formats.
//!
//! The Office apps take a font's height from its Windows ascent and descent
//! (`OS/2` `usWinAscent` / `usWinDescent`), or from its typographic ones
//! when the font asks for those (`fsSelection` bit 7, `USE_TYPO_METRICS`:
//! Aptos). macOS's text system reports `hhea`'s, which differ for some of
//! the fonts Office uses (Yu Gothic: 0.88 / 0.22 against 0.985 / 0.302;
//! Consolas, Candara, Corbel, Constantia, Century Gothic). Measured against
//! the PDFs PowerPoint, Excel and Word make (the text's coordinates in them).
//!
//! - [`font_file_metrics`] reads the ascent and descent from a font file;
//!   `Resources::line_metrics` uses it for the fonts drawn.
//! - [`office_metrics`] has the values of the typefaces Office uses, for
//!   when the typeface is not installed and another font stands in for it.
//! - [`powerpoint_below_baseline`] is where PowerPoint puts the baseline in
//!   a line (`rich_text` lays out the lines with it).
//!
//! For Word (docx): a line is the font's height, `(ascent + descent) ×
//! size`, and the baseline its ascent below the line's top (commit cf33cf3
//! of the docx branch). `Resources::line_metrics(font)` gives the ascent and
//! descent of the font drawn in ems, from its file (as Word takes them), and
//! [`office_metrics`] (the typeface's name) those of a typeface that is not
//! installed; together they replace `docx::flow_layout::word_metrics`.

/// The height of a single-spaced line in PowerPoint, in multiples of the
/// font's size, whatever the font (measured: Calibri, Arial, Times New
/// Roman, Cambria, Century, Aptos, 游ゴシック, 游明朝, メイリオ, ＭＳ ゴシック at
/// 8 to 54 pt all advance 1.2 × size a line).
pub const POWERPOINT_SINGLE: f32 = 1.2;

/// The ascent and descent (ems) of the font in `data` (the `index`th of a
/// collection): its Windows ones, or its typographic ones when it asks for
/// them to be used. `None` when the file cannot be read.
pub(crate) fn font_file_metrics(data: &[u8], index: u32) -> Option<(f32, f32)> {
    let u16_at = |at: usize| -> Option<u16> { Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?])) };
    let u32_at = |at: usize| -> Option<u32> { Some(u32::from_be_bytes([*data.get(at)?, *data.get(at + 1)?, *data.get(at + 2)?, *data.get(at + 3)?])) };
    let offset = if data.starts_with(b"ttcf") {
        let count = u32_at(8)?;
        if index >= count {
            return None;
        }
        u32_at(12 + 4 * index as usize)? as usize
    } else {
        0
    };
    let tables = u16_at(offset + 4)? as usize;
    let find = |tag: &[u8; 4]| -> Option<usize> {
        (0..tables).find_map(|i| {
            let record = offset + 12 + 16 * i;
            (data.get(record..record + 4)? == tag).then(|| u32_at(record + 8).map(|o| o as usize)).flatten()
        })
    };
    let head = find(b"head")?;
    let em = f32::from(u16_at(head + 18)?);
    if em <= 0.0 {
        return None;
    }
    let os2 = find(b"OS/2")?;
    let selection = u16_at(os2 + 62)?;
    let (ascent, descent) = if selection & 0x80 != 0 {
        let ascent = u16_at(os2 + 68)? as i16;
        let descent = u16_at(os2 + 70)? as i16;
        (f32::from(ascent), -f32::from(descent))
    } else {
        (f32::from(u16_at(os2 + 74)?), f32::from(u16_at(os2 + 76)?))
    };
    (ascent > 0.0 && descent >= 0.0).then(|| (ascent / em, descent / em))
}

/// What Microsoft Excel's row height follows in a font's file (`xlsx::row_fit`): `OS/2`'s `usWinAscent` and `usWinDescent`, `hhea`'s
/// ascender, descender and lineGap, `head`'s unitsPerEm, and the rows of the `VDMX` group Windows' GDI reads its ascent and descent
/// from, if the font has one for the DEFAULT charset at 1:1. `None` when the file cannot be read.
pub struct WinRowMetrics {
    pub win_ascent: f64,
    pub win_descent: f64,
    pub hhea_ascent: f64,
    pub hhea_descent: f64,
    pub hhea_line_gap: f64,
    pub units_per_em: f64,
    /// `OS/2`'s ySubscriptYSize and ySuperscriptYSize (font units): the size Windows gives a subscript and a superscript, in proportion to the em.
    pub y_subscript_size: f64,
    pub y_superscript_size: f64,
    /// `OS/2`'s ySubscriptYOffset and ySuperscriptYOffset (font units, both positive): how far Windows moves a subscript down and a superscript up.
    pub y_subscript_offset: f64,
    pub y_superscript_offset: f64,
    /// `OS/2`'s ulCodePageRange1 (0 when the table is version 0): bit 17 is code page 932, Japanese.
    pub code_page_range_1: u32,
    /// The VDMX group of the DEFAULT charset for square pixels: (ppem, yMax, yMin) of each row it has.
    pub vdmx: Option<Vec<(u16, i16, i16)>>,
}

impl WinRowMetrics {
    /// A Japanese code page typeface whose `OS/2` has no script offsets Word can use: ySuperscriptYOffset 0 or the ascent itself (ＭＳ, HG: the ascent; BIZ UD, 游ゴシック, UD デジタル教科書体: 0),
    /// against the real ones of メイリオ (0.45), 游明朝 (0.35) and Noto JP. Word gives such a typeface a script of 0.6 of the body that rises by the body less its size
    /// (Windows Word's XPS, 294 typefaces at 20 and 24 pt: 47 of 47 typefaces, none outside it; `tasks/doc/requests/2026-11-16-192-rise-result.md`).
    pub fn japanese_without_script_offsets(&self) -> bool {
        self.code_page_range_1 & (1 << 17) != 0 && (self.y_superscript_offset == 0.0 || self.y_superscript_offset == self.win_ascent)
    }

    /// GDI's `tmAscent` and `tmDescent` in pixels of a device where the em is `ppem` pixels (`CreateFont` with `lfHeight = -ppem`, DEFAULT charset): the `VDMX` group's `yMax` and
    /// `-yMin` where the font has one with a row for `ppem` and `ppem` is under 255 (up to 254 the glyphs are hinted and the group's rows are the hinted values; from 255 GDI scales the
    /// outline), else `usWinAscent` and `usWinDescent` scaled to the pixel and rounded. Windows GDI at 600 dpi, 82 typefaces, 145 faces, ppem 20 to 620
    /// (`tasks/doc/requests/2026-11-18-192-gdi-metrics-*.tsv`): the same in 137 faces at every ppem, the other 8 (variable fonts, Noto) within a pixel at a few. A `VDMX` group of
    /// another charset (Tahoma's is ANSI) is not read by GDI for the DEFAULT charset, and not here (`vdmx` is the DEFAULT charset's).
    pub fn gdi_ascent_descent(&self, ppem: u32) -> (i32, i32) {
        if ppem < 255 {
            if let Some(&(_, y_max, y_min)) = self.vdmx.as_ref().and_then(|rows| rows.iter().find(|r| u32::from(r.0) == ppem)) {
                return (i32::from(y_max), -i32::from(y_min));
            }
        }
        let scale = |units: f64| (units * f64::from(ppem) / self.units_per_em + 0.5).floor() as i32;
        (scale(self.win_ascent), scale(self.win_descent))
    }

    /// GDI's `tmExternalLeading` in pixels at `ppem`: `external_leading`'s formula scaled to the pixel and rounded (the same in all 145 faces at every ppem where the file is the one GDI used).
    pub fn gdi_external_leading(&self, ppem: u32) -> i32 {
        let units = (self.hhea_line_gap + self.hhea_ascent - self.hhea_descent - self.win_ascent - self.win_descent).max(0.0);
        (units * f64::from(ppem) / self.units_per_em + 0.5).floor() as i32
    }

    /// GDI's external leading (`TEXTMETRIC.tmExternalLeading`) in ems: `hhea` ascender − descender + lineGap − `usWinAscent` − `usWinDescent`, never negative. The one formula
    /// Excel's turned text and furigana (`xlsx::sheet_view`) and Word's lines of Latin typefaces (`office_external_leading`) use.
    pub fn external_leading(&self) -> f32 {
        ((self.hhea_line_gap + self.hhea_ascent - self.hhea_descent - self.win_ascent - self.win_descent).max(0.0) / self.units_per_em) as f32
    }
}

impl WinRowMetrics {
    /// GDI's `tmAscent` at `ppem` pixels: `usWinAscent` scaled to the em and rounded to a pixel (游ゴシック 74 at 75 px, 87 at 88 px, 115 at 117 px).
    pub fn gdi_ascent_px(&self, ppem: f64) -> f64 {
        (self.win_ascent * ppem / self.units_per_em).round()
    }

    /// GDI's `tmExternalLeading` at `ppem` pixels: the font's external leading (`external_leading`, in ems) scaled and rounded to a pixel (Arial 2, 3 and 4 px at
    /// 75, 88 and 117 px; Times New Roman 3, 4, 5; 游ゴシック 24, 28, 37).
    pub fn gdi_external_leading_px(&self, ppem: f64) -> f64 {
        (f64::from(self.external_leading()) * ppem).round()
    }

    /// What a turned line's box is thicker than the line by, per side, beyond the fixed part (GitHub issue #189, Excel's EMF and PDF of 99 typeface and size
    /// pairs: the box is `tmHeight + 2 S` thick, and `S` is 0.5 pt plus this): half of the external leading and half of how far the ascent passes the em, in
    /// pixels at `ppem` (rms 0.51 px over the 99 pairs, the measurement's own quantum; the fixed 0.5 pt is not here: it is a measured constant of
    /// `docs/rule/implimentationRule.md`, "測った係数を使ってよい例外", placed in `xlsx::sheet_view`).
    pub fn rotated_box_margin_px(&self, ppem: f64) -> f64 {
        rotated_box_margin_px_em(f64::from(self.external_leading()), self.win_ascent / self.units_per_em, ppem)
    }
}

/// `WinRowMetrics::rotated_box_margin_px` from the two values in ems (`external_leading()` and `usWinAscent / unitsPerEm`), as a font's cached metrics hold them:
/// `½ × (tmExternalLeading + max(0, tmAscent − ppem))`, each of the two GDI values rounded to a pixel at `ppem`.
pub fn rotated_box_margin_px_em(external_leading_em: f64, win_ascent_em: f64, ppem: f64) -> f64 {
    let (leading, ascent) = ((external_leading_em * ppem).round(), (win_ascent_em * ppem).round());
    0.5 * (leading + (ascent - ppem).max(0.0))
}

pub(crate) fn font_file_win_row_metrics(data: &[u8], index: u32) -> Option<WinRowMetrics> {
    let u16_at = |at: usize| -> Option<u16> { Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?])) };
    let i16_at = |at: usize| -> Option<i16> { u16_at(at).map(|v| v as i16) };
    let u32_at = |at: usize| -> Option<u32> { Some(u32::from_be_bytes([*data.get(at)?, *data.get(at + 1)?, *data.get(at + 2)?, *data.get(at + 3)?])) };
    let offset = if data.starts_with(b"ttcf") {
        if index >= u32_at(8)? {
            return None;
        }
        u32_at(12 + 4 * index as usize)? as usize
    } else {
        0
    };
    let tables = u16_at(offset + 4)? as usize;
    let find = |tag: &[u8; 4]| -> Option<usize> {
        (0..tables).find_map(|i| {
            let record = offset + 12 + 16 * i;
            (data.get(record..record + 4)? == tag).then(|| u32_at(record + 8).map(|o| o as usize)).flatten()
        })
    };
    let units_per_em = f64::from(u16_at(find(b"head")? + 18)?);
    let os2 = find(b"OS/2")?;
    let (win_ascent, win_descent) = (f64::from(u16_at(os2 + 74)?), f64::from(u16_at(os2 + 76)?));
    let (y_subscript_size, y_superscript_size) = (f64::from(i16_at(os2 + 12)?), f64::from(i16_at(os2 + 20)?));
    let (y_subscript_offset, y_superscript_offset) = (f64::from(i16_at(os2 + 16)?), f64::from(i16_at(os2 + 24)?));
    let os2_version = u16_at(os2)?;
    let code_page_range_1 = if os2_version >= 1 { u32_at(os2 + 78).unwrap_or(0) } else { 0 };
    let hhea = find(b"hhea")?;
    let (hhea_ascent, hhea_descent, hhea_line_gap) = (f64::from(i16_at(hhea + 4)?), f64::from(i16_at(hhea + 6)?), f64::from(i16_at(hhea + 8)?));
    let vdmx = find(b"VDMX").and_then(|v| {
        // The ratio records: bCharSet, xRatio, yStartRatio, yEndRatio; a group per record, its offset from the table's start.
        let ratios = u16_at(v + 4)? as usize;
        let at = (0..ratios).find(|&i| {
            let r = v + 6 + 4 * i;
            let byte = |k: usize| data.get(r + k).copied().unwrap_or(0);
            let (set, x, y0, y1) = (byte(0), byte(1), byte(2), byte(3));
            // Square pixels (aspect 1:1) are in the record's range of y/x (x = 0: any).
            set == 1 && (x == 0 || (y0 <= x && x <= y1.max(y0)))
        })?;
        let group = v + usize::from(u16_at(v + 6 + 4 * ratios + 2 * at)?);
        let rows = u16_at(group)? as usize;
        (0..rows).map(|i| Some((u16_at(group + 4 + 6 * i)?, i16_at(group + 6 + 6 * i)?, i16_at(group + 8 + 6 * i)?))).collect::<Option<Vec<_>>>()
    });
    (units_per_em > 0.0).then_some(WinRowMetrics { win_ascent, win_descent, hhea_ascent, hhea_descent, hhea_line_gap, units_per_em, y_subscript_size, y_superscript_size, y_subscript_offset, y_superscript_offset, code_page_range_1, vdmx })
}

/// The Windows metrics of the typefaces Office ships, for a PC that has not the typefaces: the table `data/office-font-metrics.bin` (format in `data/office-font-metrics.md`,
/// written by `scripts/gen-office-font-metrics.py` from the typefaces' own files on a Windows PC), read in place.
static OFFICE_FONT_METRICS: &[u8] = include_bytes!("../data/office-font-metrics.bin");

/// How many ppem rows (8 to 254) a face of the table has for its VDMX group.
const OFFICE_ROWS: usize = 247;
const OFFICE_RECORD: usize = 36;

/// `name`'s face of the table of the typefaces Office ships (`bold`, `italic`), as the metrics of a file. `None` for a typeface the table does not have, or a table of another version.
pub fn office_font_face(name: &str, bold: bool, italic: bool) -> Option<WinRowMetrics> {
    office_font_face_in(OFFICE_FONT_METRICS, name, bold, italic)
}

fn office_font_face_in(data: &[u8], name: &str, bold: bool, italic: bool) -> Option<WinRowMetrics> {
    let u16_at = |at: usize| data.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let i16_at = |at: usize| u16_at(at).map(|v| v as i16);
    let u32_at = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if data.get(..4)? != b"IBFM" || u32_at(4)? != 1 {
        return None;
    }
    let (count, index, strings, rows) = (u32_at(8)? as usize, u32_at(12)? as usize, u32_at(16)? as usize, u32_at(24)? as usize);
    let key = name.trim().to_lowercase();
    let flags = u8::from(bold) | (u8::from(italic) << 1);
    // The records are sorted by name, then by the flags: a binary search.
    let (mut lo, mut hi) = (0usize, count);
    while lo < hi {
        let mid = (lo + hi) / 2;
        let at = index + mid * OFFICE_RECORD;
        let (name_at, name_len) = (u32_at(at)? as usize, u16_at(at + 4)? as usize);
        let record_name = data.get(strings + name_at..strings + name_at + name_len)?;
        let record_flags = *data.get(at + 6)? & 3;
        match (record_name.cmp(key.as_bytes()), record_flags.cmp(&flags)) {
            (std::cmp::Ordering::Equal, std::cmp::Ordering::Equal) => {
                let row_at = u32_at(at + 32)?;
                let vdmx = (row_at != u32::MAX).then(|| {
                    let first = rows + row_at as usize * OFFICE_ROWS * 4;
                    (0..OFFICE_ROWS).filter_map(|i| {
                        let (y_max, y_min) = (i16_at(first + 4 * i)?, i16_at(first + 4 * i + 2)?);
                        ((y_max, y_min) != (0, 0)).then_some((8 + i as u16, y_max, y_min))
                    }).collect::<Vec<_>>()
                });
                return Some(WinRowMetrics {
                    win_ascent: f64::from(u16_at(at + 10)?),
                    win_descent: f64::from(u16_at(at + 12)?),
                    hhea_ascent: f64::from(i16_at(at + 14)?),
                    hhea_descent: f64::from(i16_at(at + 16)?),
                    hhea_line_gap: f64::from(i16_at(at + 18)?),
                    units_per_em: f64::from(u16_at(at + 8)?),
                    y_subscript_size: f64::from(i16_at(at + 20)?),
                    y_subscript_offset: f64::from(i16_at(at + 22)?),
                    y_superscript_size: f64::from(i16_at(at + 24)?),
                    y_superscript_offset: f64::from(i16_at(at + 26)?),
                    code_page_range_1: u32_at(at + 28)?,
                    vdmx,
                });
            }
            (std::cmp::Ordering::Less, _) | (std::cmp::Ordering::Equal, std::cmp::Ordering::Less) => lo = mid + 1,
            _ => hi = mid,
        }
    }
    None
}

/// Whether the font in `data` (the `index`th of a collection) says it supports the Japanese code page (932, Shift_JIS):
/// bit 17 of `OS/2` `ulCodePageRange1` (a table of version 1 or later). Word makes the lines of such typefaces 1.3 times
/// their Windows ascent plus descent (docx `flow_layout::lead_em`).
pub(crate) fn font_file_has_japanese(data: &[u8], index: u32) -> bool {
    let u16_at = |at: usize| -> Option<u16> { Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?])) };
    let u32_at = |at: usize| -> Option<u32> { Some(u32::from_be_bytes([*data.get(at)?, *data.get(at + 1)?, *data.get(at + 2)?, *data.get(at + 3)?])) };
    let read = || -> Option<bool> {
        let offset = if data.starts_with(b"ttcf") {
            if index >= u32_at(8)? {
                return None;
            }
            u32_at(12 + 4 * index as usize)? as usize
        } else {
            0
        };
        let tables = u16_at(offset + 4)? as usize;
        let os2 = (0..tables).find_map(|i| {
            let record = offset + 12 + 16 * i;
            (data.get(record..record + 4)? == b"OS/2").then(|| u32_at(record + 8).map(|o| o as usize)).flatten()
        })?;
        if u16_at(os2)? < 1 {
            return None;
        }
        Some(u32_at(os2 + 78)? & (1 << 17) != 0)
    };
    read().unwrap_or(false)
}

/// What Word draws a run's underline and strike from in a font's file, in ems (GitHub issue #192, Word's XPS of ＭＳ 明朝, メイリオ, 游明朝, Arial and 250 more).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecorationMetrics {
    /// `post` underlinePosition (negative below the baseline) and underlineThickness.
    pub underline_position: f32,
    pub underline_thickness: f32,
    /// `OS/2` yStrikeoutPosition (the top of the stroke above the baseline) and yStrikeoutSize.
    pub strike_position: f32,
    pub strike_size: f32,
    /// `OS/2` usWinDescent.
    pub win_descent: f32,
    /// The font says it supports an East Asian code page (`OS/2` ulCodePageRange1 bits 17 to 20: 932, 936, 949, 950).
    pub east_asian: bool,
}

/// The values of [`DecorationMetrics`] in the font in `data` (the `index`th of a collection); `None` when the file cannot be read.
pub(crate) fn font_file_decoration_metrics(data: &[u8], index: u32) -> Option<DecorationMetrics> {
    let u16_at = |at: usize| -> Option<u16> { Some(u16::from_be_bytes([*data.get(at)?, *data.get(at + 1)?])) };
    let i16_at = |at: usize| -> Option<i16> { u16_at(at).map(|v| v as i16) };
    let u32_at = |at: usize| -> Option<u32> { Some(u32::from_be_bytes([*data.get(at)?, *data.get(at + 1)?, *data.get(at + 2)?, *data.get(at + 3)?])) };
    let offset = if data.starts_with(b"ttcf") {
        if index >= u32_at(8)? {
            return None;
        }
        u32_at(12 + 4 * index as usize)? as usize
    } else {
        0
    };
    let tables = u16_at(offset + 4)? as usize;
    let find = |tag: &[u8; 4]| -> Option<usize> {
        (0..tables).find_map(|i| {
            let record = offset + 12 + 16 * i;
            (data.get(record..record + 4)? == tag).then(|| u32_at(record + 8).map(|o| o as usize)).flatten()
        })
    };
    let em = f32::from(u16_at(find(b"head")? + 18)?);
    if em <= 0.0 {
        return None;
    }
    let post = find(b"post")?;
    let os2 = find(b"OS/2")?;
    let east_asian = u16_at(os2)? >= 1 && u32_at(os2 + 78).is_some_and(|bits| bits & (0xF << 17) != 0);
    Some(DecorationMetrics {
        underline_position: f32::from(i16_at(post + 8)?) / em,
        underline_thickness: f32::from(i16_at(post + 10)?) / em,
        strike_position: f32::from(i16_at(os2 + 28)?) / em,
        strike_size: f32::from(i16_at(os2 + 26)?) / em,
        win_descent: f32::from(u16_at(os2 + 76)?) / em,
        east_asian,
    })
}

/// The values of [`DecorationMetrics`] in a font file's own units (before dividing by `units_per_em`): `post` underlinePosition and underlineThickness, `OS/2`
/// yStrikeoutPosition, yStrikeoutSize, usWinDescent.
struct FileDecoration {
    units_per_em: f32,
    underline_position: i16,
    underline_thickness: i16,
    strike_position: i16,
    strike_size: i16,
    win_descent: u16,
}

impl FileDecoration {
    fn metrics(&self) -> DecorationMetrics {
        let em = self.units_per_em;
        DecorationMetrics {
            underline_position: f32::from(self.underline_position) / em,
            underline_thickness: f32::from(self.underline_thickness) / em,
            strike_position: f32::from(self.strike_position) / em,
            strike_size: f32::from(self.strike_size) / em,
            win_descent: f32::from(self.win_descent) / em,
            east_asian: true,
        }
    }
}

// The values below were read from the files in `C:\Windows\Fonts` of Windows (head unitsPerEm, post, OS/2; GitHub issue #192, `tasks/doc/requests/2026-11-14-192-word-result.md`),
// not measured. Every one of the files has ulCodePageRange1 bit 17 set (an East Asian code page), so each is an East Asian typeface.
/// msmincho.ttc, ＭＳ 明朝 and ＭＳ Ｐ明朝 (no bold file: Word's bold is a synthetic one, drawn with the regular's table).
const MS_MINCHO: FileDecoration = FileDecoration { units_per_em: 256.0, underline_position: -24, underline_thickness: 12, strike_position: 66, strike_size: 13, win_descent: 36 };
/// msgothic.ttc, ＭＳ ゴシック, ＭＳ Ｐゴシック and MS UI Gothic (no bold file, as above).
const MS_GOTHIC: FileDecoration = FileDecoration { units_per_em: 256.0, underline_position: -17, underline_thickness: 19, strike_position: 66, strike_size: 13, win_descent: 36 };
/// meiryo.ttc, メイリオ (Meiryo Regular).
const MEIRYO: FileDecoration = FileDecoration { units_per_em: 2048.0, underline_position: -205, underline_thickness: 102, strike_position: 760, strike_size: 108, win_descent: 901 };
/// meiryob.ttc, メイリオ 太字 (Meiryo Bold).
const MEIRYO_BOLD: FileDecoration = FileDecoration { units_per_em: 2048.0, underline_position: -205, underline_thickness: 102, strike_position: 722, strike_size: 184, win_descent: 901 };
/// meiryo.ttc, Meiryo UI Regular.
const MEIRYO_UI: FileDecoration = FileDecoration { units_per_em: 2048.0, underline_position: -205, underline_thickness: 102, strike_position: 760, strike_size: 108, win_descent: 430 };
/// meiryob.ttc, Meiryo UI Bold.
const MEIRYO_UI_BOLD: FileDecoration = FileDecoration { units_per_em: 2048.0, underline_position: -205, underline_thickness: 102, strike_position: 722, strike_size: 184, win_descent: 430 };
/// yumin.ttf, 游明朝 (Yu Mincho Regular).
const YU_MINCHO: FileDecoration = FileDecoration { units_per_em: 2048.0, underline_position: -205, underline_thickness: 104, strike_position: 858, strike_size: 94, win_descent: 598 };
/// yumindb.ttf, 游明朝 Demibold. Whether Word uses it for bold is not confirmed (the bold of 游明朝 was not measured).
const YU_MINCHO_DEMIBOLD: FileDecoration = FileDecoration { units_per_em: 2048.0, underline_position: -205, underline_thickness: 104, strike_position: 858, strike_size: 94, win_descent: 631 };
/// YuGothR.ttc, 游ゴシック (Yu Gothic Regular).
const YU_GOTHIC: FileDecoration = FileDecoration { units_per_em: 2048.0, underline_position: -205, underline_thickness: 104, strike_position: 858, strike_size: 94, win_descent: 619 };
/// YuGothB.ttc, 游ゴシック Bold (Yu Gothic Bold).
const YU_GOTHIC_BOLD: FileDecoration = FileDecoration { units_per_em: 2048.0, underline_position: -205, underline_thickness: 104, strike_position: 858, strike_size: 94, win_descent: 612 };

/// [`DecorationMetrics`] of the typefaces Office ships that a PC may lack (a stand-in's file would give the stand-in's), from the values of their files in Windows (the constants
/// above, in the files' units divided by unitsPerEm). `None` for the others.
pub fn office_decoration_metrics(name: &str, bold: bool) -> Option<DecorationMetrics> {
    let n = name.trim();
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| n.starts_with(p));
    let f = if starts(&["MS UI Gothic", "MS Gothic", "MS PGothic"]) || (n.starts_with("ＭＳ") && (n.contains("ゴシック"))) { // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        &MS_GOTHIC
    } else if starts(&["MS Mincho", "MS PMincho"]) || (n.starts_with("ＭＳ") && n.contains("明朝")) { // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        &MS_MINCHO
    } else if starts(&["Meiryo UI"]) {
        if bold { &MEIRYO_UI_BOLD } else { &MEIRYO_UI }
    } else if matches!(n, "メイリオ" | "Meiryo") { // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        if bold { &MEIRYO_BOLD } else { &MEIRYO }
    } else if starts(&["游明朝", "Yu Mincho", "YuMincho"]) { // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        if bold { &YU_MINCHO_DEMIBOLD } else { &YU_MINCHO }
    } else if matches!(n, "游ゴシック" | "Yu Gothic" | "YuGothic") { // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        if bold { &YU_GOTHIC_BOLD } else { &YU_GOTHIC }
    } else {
        return None;
    };
    Some(f.metrics())
}

/// The ascent and descent (ems) Office gives the typeface `name` (as a
/// document names it), from the fonts it ships (`OS/2`); `None` for other
/// typefaces.
///
/// ＭＳ ゴシック・ＭＳ 明朝 (and their P and UI forms): PowerPoint takes the
/// fonts' Windows values, 0.8594 / 0.1406 (measured on its PDF: ＭＳ ゴシック
/// single-spaced at 8 to 54 pt puts the baseline 1.2 × 0.8594 × size below
/// the line's top, rounded; at 40 pt 41 pt, where 0.87 would give 42). Word
/// puts it at 0.87 of a 1 em line (the docx owner's measurement on Word's
/// PDF), so the difference is the app's: docx keeps its own value for
/// these typefaces.
pub fn office_metrics(name: &str) -> Option<(f32, f32)> {
    let n = name.trim();
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| n.starts_with(p));
    Some(match n {
        _ if starts(&["ＭＳ", "MS Gothic", "MS PGothic", "MS UI Gothic", "MS Mincho", "MS PMincho"]) => (0.8594, 0.1406), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ if starts(&["游ゴシック", "Yu Gothic UI"]) && n.contains("UI") => (1.0791, 0.251), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ if starts(&["游ゴシック", "Yu Gothic", "YuGothic"]) => (0.9849, 0.3022), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ if starts(&["游明朝", "Yu Mincho", "YuMincho"]) => (0.9951, 0.292), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        // BIZ UDゴシック・BIZ UDPゴシック・BIZ UD明朝・BIZ UDP明朝 (Windows 11's files, 2048 units to the em: usWinAscent 1802, usWinDescent 246; Word's XPS, GitHub issue #179).
        _ if starts(&["BIZ UD"]) => (0.8799, 0.1201),
        "メイリオ" | "Meiryo" => (1.0601, 0.4399), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ if starts(&["Meiryo UI"]) => (1.0601, 0.21),
        _ if starts(&["Calibri", "Carlito", "Candara", "Corbel", "Constantia"]) => (0.9521, 0.2686),
        "Cambria" => (0.9502, 0.2222),
        "Arial" => (0.9053, 0.2119),
        "Arial Unicode MS" => (1.0688, 0.2368),
        "Times New Roman" => (0.8911, 0.2163),
        "Century" | "Century Schoolbook" => (0.9858, 0.2158),
        "Century Gothic" => (0.9712, 0.2202),
        "Consolas" => (0.9199, 0.251),
        "Verdana" => (1.0054, 0.21),
        _ if starts(&["Aptos"]) => (0.939, 0.2817),
        _ if starts(&["Hiragino Sans", "Hiragino Kaku Gothic", "ヒラギノ角ゴ"]) => (1.0, 0.234), // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ => return None,
    })
}

/// GDI's external leading (`TEXTMETRIC.tmExternalLeading`) of a typeface in ems, where it has any: `max(0, hhea.lineGap - ((usWinAscent + usWinDescent) - (hhea.ascender - hhea.descender)))`
/// of its file. Word's line is the Windows ascent and descent plus it: 2355/2048 em for Arial (Windows Word's XPS of `blank-paras-after-wrapsquare.docx`: 12.07 pt for 10.5 pt),
/// 2355/2048 for Times New Roman (Word's 13.8 pt for 12 pt), the Windows ascent and descent alone for Calibri, Verdana, Tahoma, Cambria, Courier New and Georgia (their leading is 0:
/// 1.2207, 1.2153, 1.2070, 1.1724, 1.1328, 1.1362 em). The files' values (Office's own copies of the typefaces, Windows' equal; units per em 2048): (usWinAscent, usWinDescent,
/// hhea.ascender, hhea.descender, hhea.lineGap) of Arial 1854, 434, 1854, -434, 67; Times New Roman 1825, 443, 1825, -443, 87; Century 2019, 442, 2019, -443, 0; Century Gothic 1989,
/// 451, 2060, -451, 0. `None` for a typeface not listed (its leading is not known, not 0). A typeface installed here is not read: a stand-in under its name would give its own.
pub fn office_external_leading(name: &str) -> Option<f32> {
    let n = name.trim();
    let (win_a, win_d, hhea_a, hhea_d, gap) = match n {
        "Arial" => (1854.0, 434.0, 1854.0, -434.0, 67.0),
        "Times New Roman" => (1825.0, 443.0, 1825.0, -443.0, 87.0),
        "Century" | "Century Schoolbook" => (2019.0, 442.0, 2019.0, -443.0, 0.0),
        "Century Gothic" => (1989.0, 451.0, 2060.0, -451.0, 0.0),
        "Calibri" | "Verdana" | "Tahoma" | "Cambria" | "Courier New" | "Georgia" => return Some(0.0),
        _ => return None,
    };
    Some(WinRowMetrics { win_ascent: win_a, win_descent: win_d, hhea_ascent: hhea_a, hhea_descent: hhea_d, hhea_line_gap: gap, units_per_em: 2048.0, y_subscript_size: 0.0, y_superscript_size: 0.0, y_subscript_offset: 0.0, y_superscript_offset: 0.0, code_page_range_1: 0, vdmx: None }.external_leading())
}

/// The `OS/2` `sTypoAscender` (in ems) of the typefaces Office ships that Word's `rightVertical` ruby (the ruby's characters stacked at the base's right) places its box by:
/// ＭＳ 明朝 / ＭＳ ゴシック 220/256 (the same as their `usWinAscent`), BIZ UD and 游明朝 / 游ゴシック 1802/2048, メイリオ 1798/2048 (Windows 11's files, read by the Windows team,
/// `tasks/doc/requests/2026-11-14-many-patterns-3-result.md`). `None` for a typeface this table does not have (the caller takes its Windows ascent).
pub fn office_typo_ascender(name: &str) -> Option<f32> {
    let n = name.trim();
    let starts = |prefixes: &[&str]| prefixes.iter().any(|p| n.starts_with(p));
    Some(match n {
        _ if starts(&["ＭＳ", "MS Gothic", "MS PGothic", "MS UI Gothic", "MS Mincho", "MS PMincho"]) => 220.0 / 256.0, // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ if starts(&["游ゴシック", "Yu Gothic", "YuGothic", "游明朝", "Yu Mincho", "YuMincho", "BIZ UD"]) => 1802.0 / 2048.0, // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        "メイリオ" | "Meiryo" => 1798.0 / 2048.0, // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
        _ => return None,
    })
}

/// The characters the font files of the typefaces Office uses have (their cmap, Windows 11's files, tasks/doc/m85-font-coverage.tsv), by block: `font`,
/// `block` (first-last: the characters the row says something about) and the ranges the font has in it. For the typefaces a PC may lack (a stand-in draws
/// them there, whose cmap is not theirs). The data file `office_glyph_ranges.tsv` is made by `scripts/gen-word-fallback-typefaces.py`.
const OFFICE_GLYPH_RANGES_TSV: &str = include_str!("office_glyph_ranges.tsv");

/// The advance of ＭＳ 明朝 and ＭＳ ゴシック for each character (their font files' hmtx, the two agree): ranges of the characters of 0.5 em and of 1 em. Made by
/// `scripts/gen-ms-fixed-advance.py` (GitHub issue #167).
const MS_FIXED_ADVANCE_TSV: &str = include_str!("ms_fixed_advance.tsv");

/// The advance (in em, 0.5 or 1.0) that ＭＳ 明朝 and ＭＳ ゴシック give `c` in their font files, which Word sets it in whatever the OS draws it with (Word's PDF of
/// `ms-fixed-width.docx`: Greek, Cyrillic and Ǎ 1 em, the dashes, •, arrows, ∅ and € 0.5 em, to the character); `None` for a character the fonts lack.
pub fn ms_fixed_advance_em(c: char) -> Option<f32> {
    use std::sync::OnceLock;
    type Table = Vec<(u32, u32, f32)>;
    static TABLE: OnceLock<Table> = OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t: Table = Vec::new();
        for line in MS_FIXED_ADVANCE_TSV.lines().filter(|l| !l.starts_with('#')) {
            let Some((kind, ranges)) = line.split_once('\t') else { continue };
            let em = if kind == "half" { 0.5 } else { 1.0 };
            for r in ranges.split(',').filter(|r| !r.is_empty()) {
                let (a, b) = r.split_once('-').unwrap_or((r, r));
                if let (Ok(a), Ok(b)) = (u32::from_str_radix(a, 16), u32::from_str_radix(b, 16)) {
                    t.push((a, b, em));
                }
            }
        }
        t.sort_unstable_by_key(|r| r.0);
        t
    });
    let u = c as u32;
    let i = table.partition_point(|r| r.0 <= u);
    i.checked_sub(1).map(|i| table[i]).filter(|r| u <= r.1).map(|r| r.2)
}

/// The advance of ＭＳ Ｐ明朝 and ＭＳ Ｐゴシック for each character (their font files' hmtx, BMP, units per em 256): sections `@font <PMincho|PGothic> <upem>`, then the runs of code
/// points with the same advance, `first[-last][<TAB>advance]` (no advance is the em). Read from the files of Windows 11 with `struct` (Windows Word's XPS of 240 strings agree with it within 0.21 unit, no kern
/// or GPOS pair adjustment applies: `tasks/doc/requests/2026-11-16-54-acc-result.md`, #54). Data, not edited by hand.
const MS_P_ADVANCE_TSV: &str = include_str!("ms_p_advance.tsv");

/// The advance (in em) that ＭＳ Ｐ明朝 (`gothic` false) or ＭＳ Ｐゴシック gives `c` in its font file, for the PC that lacks the typeface; `None` for a character it lacks.
pub fn ms_p_advance_em(gothic: bool, c: char) -> Option<f32> {
    use std::sync::OnceLock;
    type Table = Vec<(u32, u32, u16)>;
    static TABLES: OnceLock<[(Table, f32); 2]> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        let mut out: [(Table, f32); 2] = [(Vec::new(), 256.0), (Vec::new(), 256.0)];
        let mut at = None;
        for line in MS_P_ADVANCE_TSV.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let mut f = line.split('\t');
            let head = f.next().unwrap_or("");
            if head == "@font" {
                let name = f.next().unwrap_or("");
                at = Some(usize::from(name == "PGothic"));
                if let (Some(i), Some(u)) = (at, f.next().and_then(|u| u.parse::<f32>().ok())) {
                    out[i].1 = u;
                }
                continue;
            }
            // A line with no advance is the em (the `@font` line's units per em), as most characters have.
            let Some(i) = at else { continue };
            let adv = match f.next() {
                None => out[i].1 as u16,
                Some(a) => match a.parse::<u16>() {
                    Ok(a) => a,
                    Err(_) => continue,
                },
            };
            let (a, b) = head.split_once('-').unwrap_or((head, head));
            if let (Ok(a), Ok(b)) = (u32::from_str_radix(a, 16), u32::from_str_radix(b, 16)) {
                out[i].0.push((a, b, adv));
            }
        }
        for t in &mut out {
            t.0.sort_unstable_by_key(|r| r.0);
        }
        out
    });
    let (table, upem) = &tables[usize::from(gothic)];
    let u = c as u32;
    let i = table.partition_point(|r| r.0 <= u);
    i.checked_sub(1).map(|i| table[i]).filter(|r| u <= r.1).map(|r| f32::from(r.2) / upem)
}

/// The advance of Meiryo and Meiryo UI (and their bold faces) for each character (their font files' hmtx, BMP, units per em 2048): the same shape as `MS_P_ADVANCE_TSV`
/// (sections `@font <Meiryo|MeiryoUI|MeiryoBold|MeiryoUIBold> <upem>`). Read from the files of Windows 11 with `struct`; Windows Word's XPS of 40 strings in each of the two
/// faces agree with it (the quarter em between Japanese and Latin is added to it; `w:kern` 2 adds the Latin kern pairs only, and `characterSpacingControl` changes no
/// advance): `tasks/doc/requests/2026-11-16-54-corpus-result.md`, #54. Data, not edited by hand.
const MEIRYO_ADVANCE_TSV: &str = include_str!("meiryo_advance.tsv");

/// The advance of Century (`CENTURY.TTF`, the one face: bold and italic are synthesised and keep its advance) for each character, the font file's hmtx (BMP, units per em 2048): the
/// same shape as `MS_P_ADVANCE_TSV` (one section, `@font Century <upem>`). Read from the file of Windows 11 with `struct`. Windows Word's PDF of the 95 ASCII characters at 10, 10.5, 11
/// and 12 pt has every line within 0.1% of the sum of hmtx x size / 2048 ("TOE" at 10 pt 21.70 pt); its `kern` table does not act in Word's PDF; `w:hint="eastAsia"` keeps
/// the ASCII letters and digits in Century: `tasks/doc/requests/2026-11-18-century-advance-result.md`, #54. Data, not edited by hand.
const CENTURY_ADVANCE_TSV: &str = include_str!("century_advance.tsv");

/// The advance (in em) that Century gives `c` in its font file, for the PC that lacks the typeface; `None` for a character it lacks.
pub fn century_advance_em(c: char) -> Option<f32> {
    use std::sync::OnceLock;
    static TABLE: OnceLock<(Vec<(u32, u32, u16)>, f32)> = OnceLock::new();
    let (table, upem) = TABLE.get_or_init(|| {
        let (mut out, mut upem) = (Vec::new(), 2048.0f32);
        for line in CENTURY_ADVANCE_TSV.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let mut f = line.split('\t');
            let head = f.next().unwrap_or("");
            if head == "@font" {
                f.next();
                if let Some(u) = f.next().and_then(|u| u.parse::<f32>().ok()) {
                    upem = u;
                }
                continue;
            }
            let adv = match f.next() {
                None => upem as u16,
                Some(a) => match a.parse::<u16>() {
                    Ok(a) => a,
                    Err(_) => continue,
                },
            };
            let (a, b) = head.split_once('-').unwrap_or((head, head));
            if let (Ok(a), Ok(b)) = (u32::from_str_radix(a, 16), u32::from_str_radix(b, 16)) {
                out.push((a, b, adv));
            }
        }
        out.sort_unstable_by_key(|r| r.0);
        (out, upem)
    });
    let u = c as u32;
    let i = table.partition_point(|r| r.0 <= u);
    i.checked_sub(1).map(|i| table[i]).filter(|r| u <= r.1).map(|r| f32::from(r.2) / upem)
}

/// Which of the four Meiryo faces `name` is (`ui`: Meiryo UI), with the bold one when `bold`; `None` when it is neither Meiryo nor Meiryo UI.
pub fn meiryo_face(name: &str) -> Option<bool> {
    match name.trim() {
        "メイリオ" | "Meiryo" => Some(false), // i18n-ok（フォント名の判定。訳さない）
        "Meiryo UI" | "メイリオ UI" => Some(true), // i18n-ok（フォント名の判定。訳さない）
        _ => None,
    }
}

/// The advance (in em) that Meiryo (`ui` false) or Meiryo UI gives `c` in its font file (the bold face when `bold`), for the PC that lacks the typeface; `None` for a character it lacks.
pub fn meiryo_advance_em(ui: bool, bold: bool, c: char) -> Option<f32> {
    use std::sync::OnceLock;
    type Table = Vec<(u32, u32, u16)>;
    static TABLES: OnceLock<[(Table, f32); 4]> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        let mut out: [(Table, f32); 4] = [(Vec::new(), 2048.0), (Vec::new(), 2048.0), (Vec::new(), 2048.0), (Vec::new(), 2048.0)];
        let mut at = None;
        for line in MEIRYO_ADVANCE_TSV.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let mut f = line.split('\t');
            let head = f.next().unwrap_or("");
            if head == "@font" {
                at = match f.next().unwrap_or("") {
                    "Meiryo" => Some(0),
                    "MeiryoUI" => Some(1),
                    "MeiryoBold" => Some(2),
                    "MeiryoUIBold" => Some(3),
                    _ => None,
                };
                if let (Some(i), Some(u)) = (at, f.next().and_then(|u| u.parse::<f32>().ok())) {
                    out[i].1 = u;
                }
                continue;
            }
            let Some(i) = at else { continue };
            let adv = match f.next() {
                None => out[i].1 as u16,
                Some(a) => match a.parse::<u16>() {
                    Ok(a) => a,
                    Err(_) => continue,
                },
            };
            let (a, b) = head.split_once('-').unwrap_or((head, head));
            if let (Ok(a), Ok(b)) = (u32::from_str_radix(a, 16), u32::from_str_radix(b, 16)) {
                out[i].0.push((a, b, adv));
            }
        }
        for t in &mut out {
            t.0.sort_unstable_by_key(|r| r.0);
        }
        out
    });
    let (table, upem) = &tables[usize::from(ui) + 2 * usize::from(bold)];
    let u = c as u32;
    let i = table.partition_point(|r| r.0 <= u);
    i.checked_sub(1).map(|i| table[i]).filter(|r| u <= r.1).map(|r| f32::from(r.2) / upem)
}

/// The font file's name for an Office typeface's name (the Japanese and the English one of ＭＳ 明朝, 游ゴシック and the others the tables have).
fn office_font_file_name(name: &str) -> &str {
    match name.trim() {
        "ＭＳ 明朝" | "MS Mincho" => "MS Mincho",
        "ＭＳ ゴシック" | "MS Gothic" => "MS Gothic",
        "游明朝" | "Yu Mincho" => "Yu Mincho",
        "游ゴシック" | "Yu Gothic" => "Yu Gothic",
        other => other,
    }
}

/// Whether the Office typeface `name` has a glyph for `c`, as its font file's table here says; `None` when the table has nothing for the typeface or for the
/// character's block.
pub fn office_has_glyph(name: &str, c: char) -> Option<bool> {
    let font = office_font_file_name(name);
    let u = c as u32;
    for line in OFFICE_GLYPH_RANGES_TSV.lines().filter(|l| !l.starts_with('#')) {
        let mut f = line.split('\t');
        let (Some(row_font), Some(block), ranges) = (f.next(), f.next(), f.next().unwrap_or("")) else { continue };
        if !row_font.eq_ignore_ascii_case(font) {
            continue;
        }
        let (first, last) = block.split_once('-')?;
        if !(u32::from_str_radix(first, 16).ok()?..=u32::from_str_radix(last, 16).ok()?).contains(&u) {
            continue;
        }
        return Some(ranges.split(',').filter(|r| !r.is_empty()).any(|r| {
            let (a, b) = r.split_once('-').unwrap_or((r, r));
            matches!((u32::from_str_radix(a, 16), u32::from_str_radix(b, 16)), (Ok(a), Ok(b)) if (a..=b).contains(&u))
        }));
    }
    None
}

/// How far below its baseline PowerPoint ends a line `height` high whose
/// single spacing would be `single` high (1.2 × its largest size), for a
/// font whose ascent and descent divide single spacing so that `natural`
/// of it is below the baseline (`single × descent / (ascent + descent)`).
/// The baseline is this far above the line's bottom.
///
/// A single-spaced line is divided as the font's ascent and descent
/// divide its height. A line with other spacing (a multiple or an exact
/// height) has a quarter of its height below the baseline, and more by as
/// much as the font's own part below the baseline exceeds a quarter of
/// single spacing (メイリオ); a line smaller than single spacing keeps no more
/// below the baseline than single spacing does. (Measured on PowerPoint's
/// PDF: Calibri, 游ゴシック, メイリオ, ＭＳ ゴシック, Arial and Aptos at 60 to 250 %
/// and exactly 6 to 50 pt, 20 pt text.)
pub(crate) fn powerpoint_below_baseline(height: f32, single: f32, natural: f32) -> f32 {
    if (height - single).abs() < single * 1e-4 {
        return natural;
    }
    let quarter = 0.25 * height + (natural - 0.25 * single).max(0.0);
    if height < single { quarter.min(natural) } else { quarter }
}

/// Where PowerPoint puts the baseline in a line `height` high, from its
/// top, for a font with ascent `ascent` and descent `descent` (any unit)
/// whose single spacing is `single` high (see
/// [`powerpoint_below_baseline`]).
pub fn powerpoint_baseline(height: f32, single: f32, ascent: f32, descent: f32) -> f32 {
    height - powerpoint_below_baseline(height, single, natural_below(single, ascent, descent))
}

/// How much of single spacing `single` is below the baseline for a font
/// with ascent `ascent` and descent `descent`.
pub(crate) fn natural_below(single: f32, ascent: f32, descent: f32) -> f32 {
    if ascent + descent > 0.0 { single * descent / (ascent + descent) } else { single * 0.2 }
}

/// The advance widths of U+3040..=U+30FF (Hiragana and Katakana) in `Meiryo UI`
/// (`meiryo.ttc` font index 2, `unitsPerEm = 2048`, `hmtx` table), in units of
/// `1/256 em` (`advanceWidth / 8`, exact in `meiryo.ttc`).
///
/// While `Meiryo` (`meiryo.ttc` index 0) and `Hiragino Sans` have full-width or
/// near-full-width (`palt` ~0.91 em) kana, `Meiryo UI` has condensed proportional
/// kana (`hmtx` averaging ~0.785 em) while keeping full-width (`1.0 em`) kanji.
pub(crate) const MEIRYO_UI_KANA_WIDTHS: [u16; 192] = [
    256, 163, 209, 169, 209, 121, 164, 161, 201, 169, 209, 209, 209, 193, 205, 129,
    161, 209, 209, 177, 197, 177, 197, 185, 185, 217, 217, 217, 217, 191, 204, 201,
    201, 195, 201, 161, 201, 209, 201, 209, 161, 161, 219, 201, 217, 227, 209, 219,
    219, 219, 209, 213, 213, 209, 209, 209, 209, 209, 209, 219, 219, 219, 193, 227,
    209, 209, 195, 177, 217, 169, 219, 153, 193, 177, 161, 195, 227, 195, 177, 217,
    219, 219, 193, 209, 196, 172, 173, 256, 256, 256, 256, 128, 128, 188, 188, 256,
    256, 150, 193, 145, 185, 150, 190, 153, 193, 169, 196, 183, 205, 209, 213, 177,
    197, 201, 209, 177, 197, 209, 213, 209, 213, 196, 209, 209, 213, 193, 205, 177,
    185, 201, 201, 148, 188, 205, 196, 209, 181, 181, 205, 191, 185, 209, 169, 207,
    217, 217, 167, 181, 189, 173, 193, 193, 209, 209, 209, 209, 209, 209, 193, 153,
    209, 177, 193, 169, 209, 169, 209, 129, 161, 177, 150, 209, 193, 185, 149, 193,
    209, 209, 181, 191, 201, 148, 169, 193, 226, 247, 181, 128, 209, 191, 191, 256,
];

/// The ink of the glyphs of U+3040..=U+30FF in `Meiryo UI` (`meiryo.ttc` font index 2, `unitsPerEm = 2048`): the left and right edge of
/// each glyph's outline (`glyf`'s `xMin` and `xMax`) from the left edge of its cell, in units of `1/2048` em; `(0, 0)` where the
/// typeface has no glyph. Where the cell is narrowed to `MEIRYO_UI_KANA_WIDTHS`, a stand-in's glyph (cut for the full-width cell) is
/// drawn as wide as this ink and centred on it, so that it does not run into its neighbours as it would at the width it was cut for.
pub(crate) const MEIRYO_UI_KANA_INK: [(u16, u16); 192] = [
    (0, 0), (41, 1243), (56, 1610), (104, 1298), (142, 1617), (43, 906), (80, 1219), (50, 1229),
    (62, 1546), (84, 1299), (72, 1612), (54, 1619), (54, 1726), (104, 1461), (104, 1750), (102, 937),
    (102, 1278), (139, 1611), (139, 1859), (98, 1348), (98, 1729), (98, 1327), (98, 1670), (136, 1405),
    (136, 1405), (66, 1705), (66, 1789), (55, 1682), (55, 1869), (56, 1443), (56, 1739), (73, 1545),
    (73, 1659), (110, 1428), (110, 1659), (59, 1193), (73, 1468), (73, 1729), (101, 1498), (101, 1530),
    (80, 1185), (80, 1369), (75, 1675), (130, 1502), (64, 1705), (27, 1812), (87, 1585), (138, 1709),
    (138, 1889), (138, 1837), (58, 1655), (58, 1719), (58, 1722), (25, 1633), (25, 1719), (25, 1687),
    (35, 1657), (35, 1657), (35, 1657), (138, 1689), (138, 1789), (138, 1796), (146, 1433), (91, 1779),
    (84, 1632), (68, 1610), (74, 1446), (108, 1345), (45, 1661), (116, 1305), (110, 1687), (106, 1173),
    (88, 1449), (80, 1317), (120, 1186), (66, 1447), (37, 1752), (63, 1450), (32, 1330), (27, 1652),
    (97, 1646), (52, 1696), (47, 1455), (81, 1611), (80, 1632), (54, 1400), (117, 1359), (0, 0),
    (0, 0), (234, 741), (228, 672), (234, 741), (228, 672), (289, 1190), (289, 1411), (248, 1835),
    (614, 1440), (45, 1132), (122, 1509), (61, 1047), (71, 1372), (113, 1086), (144, 1383), (67, 1157),
    (54, 1490), (62, 1287), (52, 1506), (72, 1326), (72, 1599), (66, 1605), (66, 1623), (60, 1319),
    (60, 1535), (19, 1585), (19, 1653), (68, 1257), (68, 1654), (57, 1613), (57, 1855), (117, 1618),
    (117, 1618), (17, 1527), (17, 1724), (54, 1594), (54, 1663), (115, 1456), (115, 1583), (53, 1348),
    (53, 1526), (74, 1545), (74, 1657), (10, 1120), (99, 1416), (99, 1544), (48, 1520), (48, 1676),
    (379, 1424), (379, 1424), (74, 1570), (63, 1465), (88, 1427), (5, 1647), (85, 1255), (15, 1638),
    (15, 1722), (15, 1706), (157, 1276), (157, 1378), (157, 1443), (92, 1293), (92, 1576), (92, 1629),
    (10, 1660), (10, 1660), (10, 1660), (20, 1651), (20, 1651), (20, 1651), (93, 1501), (52, 1194),
    (54, 1607), (85, 1303), (70, 1474), (43, 1276), (78, 1617), (57, 1295), (37, 1637), (61, 906),
    (116, 1129), (145, 1309), (134, 1071), (32, 1642), (199, 1485), (159, 1321), (110, 1081), (171, 1407),
    (68, 1608), (57, 1613), (194, 1311), (119, 1476), (144, 1583), (37, 1055), (24, 1312), (171, 1606),
    (68, 1843), (57, 1984), (194, 1568), (328, 696), (168, 1504), (289, 1188), (289, 1327), (451, 1610),
];

/// The ink of `c`'s glyph in `Meiryo UI` as the left and right edge from the left of its cell, in ems (`MEIRYO_UI_KANA_INK`).
pub(crate) fn meiryo_ui_kana_ink(c: char) -> Option<(f32, f32)> {
    let u = c as usize;
    if !(0x3040..=0x30ff).contains(&u) {
        return None;
    }
    let (left, right) = MEIRYO_UI_KANA_INK[u - 0x3040];
    (right > left).then(|| (f32::from(left) / 2048.0, f32::from(right) / 2048.0))
}

/// Fallback kana advance widths (U+3040..=U+30FF, in `1/256 em`) for an
/// uninstalled typeface whose kana metrics differ from the substitute font's.
pub(crate) fn fallback_kana_widths(name: &str) -> Option<&'static [u16; 192]> {
    if name.trim().starts_with("Meiryo UI") {
        Some(&MEIRYO_UI_KANA_WIDTHS)
    } else {
        None
    }
}

/// Whether `name` is an uninstalled proportional Japanese typeface (`ＭＳ Ｐ`, `MS P`, `HGP`, `Meiryo UI`).
pub(crate) fn is_proportional_japanese_typeface(name: &str) -> bool {
    let n = name.trim();
    n.starts_with("ＭＳ Ｐ") || n.starts_with("MS P") || n.starts_with("HGP") || n.starts_with("Meiryo UI") // i18n-ok（フォント名の判定）
}

/// Fallback advance width (in `1/256 em`, from `msgothic.ttc` `MS PGothic` / `meiryo.ttc` `Meiryo UI` `hmtx`)
/// for a full-width alphanumeric or sign (`U+FF01..=U+FF5E`, `U+30FB`) in a proportional Japanese typeface
/// when substituted by `Hiragino Sans` (`pwid` in `Hiragino Sans` narrows `U+FF10..=U+FF3A` to ~0.71–0.85 em
/// instead of `MS PGothic` / `Meiryo UI`'s proportional widths, where full-width digits and Latin letters
/// match their half-width counterparts).
pub(crate) fn fallback_fullwidth_alnum_width(c: char) -> Option<u16> {
    const DIGITS: [u16; 10] = [128, 128, 128, 128, 128, 128, 128, 128, 128, 128];
    const UPPER: [u16; 26] = [
        140, 141, 145, 145, 133, 124, 147, 145, 65, 120, 139, 124, 167,
        145, 153, 138, 153, 142, 138, 135, 145, 140, 181, 138, 138, 133,
    ];
    const LOWER: [u16; 26] = [
        128, 134, 126, 134, 130, 81, 126, 131, 60, 60, 124, 60, 194,
        131, 134, 134, 134, 90, 120, 87, 131, 120, 170, 120, 120, 118,
    ];
    match c as u32 {
        0x3000 | 0x25b6 | 0x25b7 | 0x25c0 | 0x25c1 => Some(256),
        0xff08 | 0xff09 | 0xff1a | 0xff1b | 0x30fb => Some(76),
        0xff10..=0xff19 => Some(DIGITS[(c as usize) - 0xff10]),
        0xff21..=0xff3a => Some(UPPER[(c as usize) - 0xff21]),
        0xff41..=0xff5a => Some(LOWER[(c as usize) - 0xff41]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::WinRowMetrics;

    /// GitHub issue #189: the typeface-dependent part of the turned line's box margin, against Excel's measured S minus its fixed 0.5 pt (4.17 px at 600 dpi).
    #[test]
    fn rotated_box_margin_follows_external_leading_and_ascent_excess() {
        let font = |win_ascent: f64, win_descent: f64, hhea_ascent: f64, hhea_descent: f64, gap: f64, upm: f64| WinRowMetrics {
            // `hhea`'s descender is negative, as in the file.
            win_ascent, win_descent, hhea_ascent, hhea_descent: -hhea_descent, hhea_line_gap: gap, units_per_em: upm,
            y_subscript_size: 0.0, y_superscript_size: 0.0, y_subscript_offset: 0.0, y_superscript_offset: 0.0, code_page_range_1: 0, vdmx: None,
        };
        // (typeface, Excel's measured S at 9, 10.5 and 14 pt: 75, 88 and 117 px)
        let cases = [
            ("游ゴシック", font(2017.0, 619.0, 1802.0, 455.0, 1024.0, 2048.0), [15.3, 17.3, 20.9]),
            ("Arial", font(1854.0, 434.0, 1854.0, 434.0, 67.0, 2048.0), [5.08, 5.37, 5.95]),
            ("Times New Roman", font(1825.0, 443.0, 1825.0, 443.0, 87.0, 2048.0), [5.7, 6.2, 6.2]),
            ("Meiryo UI", font(2171.0, 430.0, 2171.0, 430.0, 0.0, 2048.0), [6.2, 6.7, 7.7]),
            ("ＭＳ 明朝", font(220.0, 36.0, 220.0, 36.0, 0.0, 256.0), [4.7, 5.2, 4.7]),
        ];
        for (name, metrics, measured) in cases {
            for (ppem, measured) in [75.0, 88.0, 117.0].into_iter().zip(measured) {
                let s = 4.17 + metrics.rotated_box_margin_px(ppem);
                assert!((s - measured).abs() < 2.0,  "{name} at {ppem} px: {s} against {measured}");
            }
        }
    }

    #[test]
    fn century_advances_are_the_font_file() {
        // Word's PDF, 10 pt: TOE 21.70 pt (hmtx 4438 units), SFR 20.14 pt (4135 units).
        let toe: f32 = "TOE".chars().map(|c| century_advance_em(c).unwrap() * 2048.0).sum();
        assert_eq!(toe, 4438.0);
        let sfr: f32 = "SFR".chars().map(|c| century_advance_em(c).unwrap() * 2048.0).sum();
        assert_eq!(sfr, 4135.0);
        assert_eq!(century_advance_em(' ').map(|e| e * 2048.0), Some(569.0));
        assert_eq!(century_advance_em('あ'), None);
    }

    #[test]
    fn ms_p_advances_are_the_font_files() {
        // Windows Word's XPS of ＭＳ Ｐ明朝 (089's line `□有り（` + 27 full-width spaces + `）`): 256, 256, 164, 128, 27 x 170, 128 of 256 = 5522 units.
        let units: f32 = ['□', '有', 'り', '（'].iter().map(|c| ms_p_advance_em(false, *c).unwrap() * 256.0).sum::<f32>() + 27.0 * ms_p_advance_em(false, '\u{3000}').unwrap() * 256.0 + ms_p_advance_em(false, '）').unwrap() * 256.0;
        assert!((units - 5522.0).abs() < 0.01, "{units}");
        assert_eq!(ms_p_advance_em(false, ' ').map(|e| e * 256.0), Some(78.0));
        assert_eq!(ms_p_advance_em(false, 'り').map(|e| e * 256.0), Some(164.0));
    }

    use super::*;

    /// 20 pt text: (line height, font's part below the baseline) → the
    /// baseline from the line's top PowerPoint drew (rounded to points).
    #[test]
    fn baselines_as_powerpoint_draws_them() {
        let single = 24.0;
        let natural = |a: f32, d: f32| single * d / (a + d);
        let calibri = natural(0.9521, 0.2686);
        let meiryo = natural(1.0601, 0.4399);
        let ms_gothic = natural(0.8594, 0.1406);
        let base = |h: f32, n: f32| (h - powerpoint_below_baseline(h, single, n)).round();
        // Single spacing: the font's division.
        assert_eq!(base(24.0, calibri), 19.0);
        assert_eq!(base(24.0, meiryo), 17.0);
        assert_eq!(base(24.0, ms_gothic), 21.0);
        // Multiples.
        assert_eq!(base(36.0, calibri), 27.0);
        assert_eq!(base(60.0, calibri), 45.0);
        assert_eq!(base(60.0, meiryo), 44.0);
        assert_eq!(base(25.2, ms_gothic), 19.0);
        assert_eq!(base(22.8, ms_gothic), 19.0);
        assert_eq!(base(16.8, calibri), 13.0);
        assert_eq!(base(22.8, meiryo), 16.0);
        // Exact heights.
        assert_eq!(base(10.0, calibri), 8.0);
        assert_eq!(base(40.0, calibri), 30.0);
        assert_eq!(base(20.0, meiryo), 14.0);
    }

    #[test]
    fn reads_windows_metrics_from_a_font_file() {
        let data = include_bytes!("../fonts/carlito/Carlito-Regular.ttf");
        let (a, d) = font_file_metrics(data, 0).unwrap();
        assert!((a - 0.9521).abs() < 1e-3 && (d - 0.2686).abs() < 1e-3, "{a} {d}");
    }

    #[test]
    fn japanese_without_script_offsets_is_cp932_with_offset_zero_or_ascent() {
        let m = |cp: u32, offset: f64| WinRowMetrics { win_ascent: 220.0, win_descent: 36.0, hhea_ascent: 220.0, hhea_descent: -36.0, hhea_line_gap: 0.0, units_per_em: 256.0, y_subscript_size: 128.0, y_superscript_size: 128.0, y_subscript_offset: 92.0, y_superscript_offset: offset, code_page_range_1: cp, vdmx: None };
        assert!(m(1 << 17, 220.0).japanese_without_script_offsets()); // ＭＳ 明朝: the ascent
        assert!(m(1 << 17, 0.0).japanese_without_script_offsets()); // BIZ UD, 游ゴシック: 0
        assert!(!m(1 << 17, 116.0).japanese_without_script_offsets()); // メイリオ, 游明朝: a real offset
        assert!(!m(1, 220.0).japanese_without_script_offsets()); // not code page 932
    }

    /// The table of the typefaces Office ships (`data/office-font-metrics.bin`): Segoe UI's face has the metrics and the VDMX rows GDI's ascent and descent at 600 dpi come from
    /// (`tasks/doc/requests/2026-11-18-192-gdi-metrics-latin.tsv`: ppem 100 is 107 and 26, ppem 257 is 277 and 65), a regular and a bold face are two, a typeface not in it is none.
    #[test]
    fn the_table_of_office_typefaces_gives_gdi_ascents() {
        let segoe = office_font_face("Segoe UI", false, false).expect("Segoe UI");
        assert_eq!((segoe.units_per_em, segoe.win_ascent, segoe.win_descent), (2048.0, 2210.0, 514.0));
        assert_eq!(segoe.gdi_ascent_descent(100), (107, 26));
        assert_eq!(segoe.gdi_ascent_descent(257), (277, 65));
        assert!(office_font_face("segoe ui", true, false).is_some() && office_font_face("Segoe UI", true, false).unwrap().win_ascent > 0.0);
        let ms = office_font_face("ＭＳ 明朝", false, false).expect("ＭＳ 明朝");
        assert_eq!((ms.units_per_em, ms.win_ascent, ms.win_descent), (256.0, 220.0, 36.0));
        assert!(ms.japanese_without_script_offsets());
        assert!(office_font_face("No Such Typeface", false, false).is_none());
        assert!(office_font_face_in(b"IBFM\x02\0\0\0", "Segoe UI", false, false).is_none(), "another version of the table is not read");
    }

    #[test]
    fn win_row_metrics_read_os2_hhea_and_vdmx() {
        // Carlito has Calibri's metrics (usWinAscent 1950, usWinDescent 550 of 2048, hhea 1950, -550, 0) and no VDMX table.
        let data = include_bytes!("../fonts/carlito/Carlito-Regular.ttf");
        let m = font_file_win_row_metrics(data, 0).unwrap();
        assert_eq!((m.win_ascent, m.win_descent, m.units_per_em), (1950.0, 550.0, 2048.0));
        assert_eq!((m.hhea_ascent, m.hhea_descent, m.hhea_line_gap), (1950.0, -550.0, 0.0));
        assert!(m.vdmx.is_none());
        // A made-up font of four tables: head, OS/2, hhea and a VDMX with one ratio record (charset given, 1:1) and a group of
        // two rows (ppem 11: yMax 9, yMin -3; ppem 12: yMax 10, yMin -3).
        let tables: [(&[u8; 4], Vec<u8>); 4] = [
            (b"head", { let mut t = vec![0u8; 54]; t[18..20].copy_from_slice(&1000u16.to_be_bytes()); t }),
            (b"OS/2", { let mut t = vec![0u8; 78]; t[74..76].copy_from_slice(&900u16.to_be_bytes()); t[76..78].copy_from_slice(&200u16.to_be_bytes()); t }),
            (b"hhea", { let mut t = vec![0u8; 36]; t[4..6].copy_from_slice(&800i16.to_be_bytes()); t[6..8].copy_from_slice(&(-150i16).to_be_bytes()); t[8..10].copy_from_slice(&90i16.to_be_bytes()); t }),
            (b"VDMX", {
                // version, numRecs, numRatios, one ratio (set, x 1, y 1..1), its group's offset (12), then the group.
                let mut t = vec![0u8, 1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 12];
                t.extend_from_slice(&[0, 2, 11, 12]);
                for (ppem, hi, lo) in [(11u16, 9i16, -3i16), (12, 10, -3)] {
                    t.extend_from_slice(&ppem.to_be_bytes());
                    t.extend_from_slice(&hi.to_be_bytes());
                    t.extend_from_slice(&lo.to_be_bytes());
                }
                t
            }),
        ];
        let build = |charset: u8| {
            let mut out = vec![0u8; 12 + 16 * 4];
            out[4..6].copy_from_slice(&4u16.to_be_bytes());
            for (i, (tag, body)) in tables.iter().enumerate() {
                let at = out.len();
                let mut body = body.clone();
                if *tag == b"VDMX" {
                    body[6] = charset;
                }
                out.extend_from_slice(&body);
                let rec = 12 + 16 * i;
                out[rec..rec + 4].copy_from_slice(*tag);
                out[rec + 8..rec + 12].copy_from_slice(&(at as u32).to_be_bytes());
            }
            out
        };
        let m = font_file_win_row_metrics(&build(1), 0).unwrap();
        assert_eq!((m.win_ascent, m.win_descent, m.units_per_em), (900.0, 200.0, 1000.0));
        assert_eq!((m.hhea_ascent, m.hhea_descent, m.hhea_line_gap), (800.0, -150.0, 90.0));
        assert_eq!(m.vdmx, Some(vec![(11, 9, -3), (12, 10, -3)]));
        assert!(font_file_win_row_metrics(&build(0), 0).unwrap().vdmx.is_none(), "a VDMX group of another charset does not count");
    }

    /// GDI's `tmAscent`/`tmDescent` at 600 dpi (#192): the VDMX row below ppem 255 (Segoe UI at ppem 100: 107 and 26 where 2210 and 514 of 2048 scale to 108 and 25), the scaled and
    /// rounded Windows ascent and descent without a row and from ppem 255 (Segoe UI at 257: 277 and 65, the table's), the external leading rounded at the pixel.
    #[test]
    fn gdi_ascent_and_descent_follow_the_vdmx_rows_under_ppem_255() {
        let mut m = WinRowMetrics { win_ascent: 2210.0, win_descent: 514.0, hhea_ascent: 2210.0, hhea_descent: -514.0, hhea_line_gap: 67.0, units_per_em: 2048.0, y_subscript_size: 0.0, y_superscript_size: 0.0, y_subscript_offset: 0.0, y_superscript_offset: 0.0, code_page_range_1: 0, vdmx: Some(vec![(100, 107, -26), (254, 274, -64), (255, 273, -65)]) };
        assert_eq!(m.gdi_ascent_descent(100), (107, 26));
        assert_eq!(m.gdi_ascent_descent(254), (274, 64));
        assert_eq!(m.gdi_ascent_descent(255), (275, 64), "from 255 the outline is scaled, the VDMX row is not read");
        assert_eq!(m.gdi_ascent_descent(257), (277, 65));
        assert_eq!(m.gdi_ascent_descent(101), (109, 25), "no row for the ppem: scaled");
        m.vdmx = None;
        assert_eq!(m.gdi_ascent_descent(100), (108, 25));
        assert_eq!((m.gdi_external_leading(100), m.gdi_external_leading(600)), (3, 20));
    }
}

#[cfg(test)]
mod glyph_range_tests {
    use super::office_has_glyph;

    #[test]
    fn the_font_files_tables() {
        // Courier New has ■ (25A0) and not ① (2460); Segoe UI Symbol has ☆ and not Ⅰ; Cambria Math has ① – ⑳; ＭＳ 明朝 is MS Mincho's.
        assert_eq!(office_has_glyph("Courier New", '■'), Some(true));
        assert_eq!(office_has_glyph("Courier New", '①'), Some(false));
        assert_eq!(office_has_glyph("Segoe UI Symbol", '☆'), Some(true));
        assert_eq!(office_has_glyph("Segoe UI Symbol", 'Ⅰ'), Some(false));
        assert_eq!(office_has_glyph("Cambria Math", '①'), Some(true));
        assert_eq!(office_has_glyph("ＭＳ 明朝", '※'), office_has_glyph("MS Mincho", '※'));
        // Not in a block of the tables, or a typeface without one: unknown.
        assert_eq!(office_has_glyph("Courier New", 'é'), None);
        assert_eq!(office_has_glyph("No Such Typeface", '■'), None);
    }
}
