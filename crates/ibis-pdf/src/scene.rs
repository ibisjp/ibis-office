// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Viewer/editor intermediate representation (Layer 2 bridge) and page tree resolution.

use crate::container::PdfContainer;
use crate::cos::{CosDict, CosObj, ObjectId};
use crate::{Error, Result};

/// Axis-aligned rectangle in PDF user-space points (1/72 inch).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl PdfRect {
    pub fn width(self) -> f32 {
        (self.x1 - self.x0).abs()
    }

    pub fn height(self) -> f32 {
        (self.y1 - self.y0).abs()
    }
}

/// Resolved metadata for a single page in the `/Pages` tree.
#[derive(Clone, Debug, PartialEq)]
pub struct PageInfo {
    /// 0-based page index.
    pub index: usize,
    pub object_id: ObjectId,
    pub media_box: PdfRect,
    pub crop_box: PdfRect,
    pub rotate: u16,
}

/// Path segment in PDF user/device space points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathCmd {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

/// Drawing commands emitted by a page interpreter for GPUI rendering.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayCmd {
    /// Replace the current device-space rectangular clip. None clears it.
    SetClipRect(Option<PdfRect>),
    PushClip(PdfRect),
    PopClip,
    PushTransparencyGroup {
        opacity: f32,
    },
    PopTransparencyGroup,
    DrawRect {
        rect: PdfRect,
        fill_rgba: Option<[u8; 4]>,
    },
    DrawPath {
        bounds: PdfRect,
        commands: Vec<PathCmd>,
        fill_rule: FillRule,
        fill_rgba: Option<[u8; 4]>,
        stroke_rgba: Option<[u8; 4]>,
        stroke_width: f32,
    },
    DrawGlyphRun {
        text: String,
        origin: (f32, f32),
        font_size: f32,
        rgba: [u8; 4],
    },
    DrawStyledGlyphRun {
        text: String,
        origin: (f32, f32),
        font_size: f32,
        family: String,
        weight: u16,
        italic: bool,
        angle: f32,
        horizontal_scale: f32,
        max_width: Option<f32>,
        rgba: [u8; 4],
    },
    DrawImage {
        bounds: PdfRect,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}

/// Character bounding box for text selection, search, and highlight annotation creation.
#[derive(Clone, Debug, PartialEq)]
pub struct CharBox {
    pub ch: char,
    pub rect_pt: PdfRect,
    pub vertical: bool,
    pub stream_op_index: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageTextIndex {
    pub chars: Vec<CharBox>,
}

/// The width of `ch` in ems where the page does not say (a glyph run of an XPS or an OFD, drawn by whatever font the viewer has): a full-width character one em, any other 0.55. It
/// gives each character of a run a box of its own, so that a drag picks the characters it passes.
pub fn estimated_advance_em(ch: char) -> f32 {
    let u = ch as u32;
    if (0x1100..=0x115F).contains(&u) || (0x2E80..=0xA4CF).contains(&u) || (0xAC00..=0xD7A3).contains(&u) || (0xF900..=0xFAFF).contains(&u) || (0xFE30..=0xFE6F).contains(&u) || (0xFF00..=0xFF60).contains(&u) || (0xFFE0..=0xFFE6).contains(&u) || (0x20000..=0x3FFFD).contains(&u) {
        1.0
    } else {
        0.55
    }
}

impl PageTextIndex {
    pub fn plain_text(&self) -> String {
        self.chars.iter().map(|c| c.ch).collect()
    }

    /// Whether the characters at `a` and `b` stand on one line (their baselines are within half a character height).
    pub fn same_line(&self, a: usize, b: usize) -> bool {
        let (p, q) = (&self.chars[a], &self.chars[b]);
        (p.rect_pt.y1 - q.rect_pt.y1).abs() <= 0.5 * p.rect_pt.height().max(q.rect_pt.height())
    }

    /// The boxes of the characters get the width they take (up to the next character of the line); the interpreter knows only where a
    /// glyph starts. A character that ends a line keeps the width it was given.
    /// (ibisOffice's change to the viewer scene of the fork: selecting text needs the real extent of a character.)
    pub fn fit_widths(&mut self) {
        for i in 0..self.chars.len().saturating_sub(1) {
            if !self.same_line(i, i + 1) {
                continue;
            }
            let (here, next) = (self.chars[i].rect_pt, self.chars[i + 1].rect_pt);
            // Within the same run of text the next character is to the right and not a whole column away.
            if next.x0 > here.x0 && next.x0 - here.x0 <= 3.0 * here.height().max(1.0) {
                self.chars[i].rect_pt.x1 = next.x0;
            }
        }
    }

    /// The character under the point (in page points), else the one nearest to it: dragging outside the text still picks.
    pub fn hit(&self, x: f32, y: f32) -> Option<usize> {
        let mut best: Option<(f32, usize)> = None;
        for (i, c) in self.chars.iter().enumerate() {
            let r = c.rect_pt;
            let (x0, x1, y0, y1) = (r.x0.min(r.x1), r.x0.max(r.x1), r.y0.min(r.y1), r.y0.max(r.y1));
            let dx = if x < x0 { x0 - x } else if x > x1 { x - x1 } else { 0.0 };
            let dy = if y < y0 { y0 - y } else if y > y1 { y - y1 } else { 0.0 };
            // A line is nearer than a character of another line: the vertical distance counts more.
            let d = dy * 4.0 + dx;
            if d == 0.0 {
                return Some(i);
            }
            if best.is_none_or(|(b, _)| d < b) {
                best = Some((d, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// The text of the characters `a..=b` (in the order of the page's text), a line break where the next character is on another line.
    pub fn text_between(&self, a: usize, b: usize) -> String {
        if self.chars.is_empty() {
            return String::new();
        }
        let (a, b) = (a.min(b), a.max(b).min(self.chars.len() - 1));
        let mut out = String::new();
        for i in a..=b {
            if i > a && !self.same_line(i - 1, i) {
                out.push('\n');
            }
            out.push(self.chars[i].ch);
        }
        out
    }

    /// Where `query` stands in the text: character ranges `[start, end)`, not across a line, not overlapping.
    pub fn find(&self, query: &str, match_case: bool) -> Vec<(usize, usize)> {
        let needle: Vec<char> = query.chars().collect();
        let n = self.chars.len();
        let mut out = Vec::new();
        if needle.is_empty() || needle.len() > n {
            return out;
        }
        let same = |a: char, b: char| if match_case { a == b } else { a.to_lowercase().eq(b.to_lowercase()) };
        let mut at = 0;
        while at + needle.len() <= n {
            let end = at + needle.len();
            let ok = (0..needle.len()).all(|k| same(self.chars[at + k].ch, needle[k])) && (at + 1..end).all(|i| self.same_line(i - 1, i));
            if ok {
                out.push((at, end));
                at = end;
            } else {
                at += 1;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, x0: f32, baseline: f32) -> Vec<CharBox> {
        text.chars().enumerate().map(|(i, ch)| CharBox { ch, rect_pt: PdfRect { x0: x0 + i as f32 * 6.0, y0: baseline - 10.0, x1: x0 + i as f32 * 6.0 + 6.0, y1: baseline }, vertical: false, stream_op_index: i }).collect()
    }

    fn page() -> PageTextIndex {
        let mut chars = line("Hello world", 10.0, 20.0);
        chars.extend(line("second line", 10.0, 40.0));
        PageTextIndex { chars }
    }

    #[test]
    fn a_point_picks_the_character_under_it_or_the_nearest() {
        let p = page();
        assert_eq!(p.hit(11.0, 15.0), Some(0));
        assert_eq!(p.hit(10.0 + 6.0 * 4.0 + 1.0, 15.0), Some(4));
        // Right of a line: its last character; below the last line: the nearest of it.
        assert_eq!(p.hit(500.0, 15.0), Some(10));
        assert_eq!(p.hit(11.0, 500.0), Some(11));
        // Between the lines the nearer line wins.
        assert_eq!(p.hit(11.0, 23.0), Some(0));
        assert_eq!(p.hit(11.0, 27.0), Some(11));
        assert_eq!(PageTextIndex::default().hit(0.0, 0.0), None);
    }

    #[test]
    fn the_text_between_two_characters_breaks_where_the_line_does() {
        let p = page();
        assert_eq!(p.text_between(6, 10), "world");
        assert_eq!(p.text_between(6, 14), "world\nseco");
        assert_eq!(p.text_between(14, 6), "world\nseco");
        assert_eq!(p.text_between(0, 999), "Hello world\nsecond line");
        assert_eq!(PageTextIndex::default().text_between(0, 3), "");
    }

    #[test]
    fn a_search_finds_the_text_inside_a_line_with_or_without_case() {
        let p = page();
        assert_eq!(p.find("world", true), vec![(6, 11)]);
        assert_eq!(p.find("WORLD", true), vec![]);
        assert_eq!(p.find("WORLD", false), vec![(6, 11)]);
        assert_eq!(p.find("o", false), vec![(4, 5), (7, 8), (14, 15)]);
        // Not across the end of a line.
        assert_eq!(p.find("dsec", false), vec![]);
        assert_eq!(p.find("", false), vec![]);
    }

    #[test]
    fn the_boxes_take_the_width_up_to_the_next_character() {
        let mut p = PageTextIndex { chars: line("ab", 0.0, 10.0) };
        // The interpreter gives each a width of 0.6 of the size; the next character is at 6.
        p.chars[0].rect_pt.x1 = 3.0;
        p.fit_widths();
        assert_eq!(p.chars[0].rect_pt.x1, 6.0);
    }
}


/// Combined rendering + text-index output for one PDF page.
#[derive(Clone, Debug, PartialEq)]
pub struct PageScene {
    pub info: PageInfo,
    pub display_list: Vec<DisplayCmd>,
    pub text_index: PageTextIndex,
}

/// Collect all pages in order from `/Root -> /Pages`, inheriting `MediaBox`/`CropBox`/`Rotate`.
pub fn collect_pages(container: &mut PdfContainer) -> Result<Vec<PageInfo>> {
    let (_, catalog) = container.catalog()?;
    let pages_id = catalog
        .get_ref(b"Pages")
        .ok_or_else(|| Error::new("catalog missing /Pages root"))?;
    let mut out = Vec::new();
    collect_page_node(container, pages_id, None, None, 0, &mut out)?;
    Ok(out)
}

fn collect_page_node(
    container: &mut PdfContainer,
    node_id: ObjectId,
    inherited_media: Option<PdfRect>,
    inherited_crop: Option<PdfRect>,
    inherited_rotate: u16,
    out: &mut Vec<PageInfo>,
) -> Result<()> {
    let node = container.resolve(node_id)?;
    let Some(dict) = node.as_dict().cloned() else {
        return Ok(());
    };

    let media = parse_rect(&dict, b"MediaBox").or(inherited_media);
    let crop = parse_rect(&dict, b"CropBox").or(inherited_crop);
    let rotate = dict
        .get_int(b"Rotate")
        .map(|r| (r.rem_euclid(360)) as u16)
        .unwrap_or(inherited_rotate);

    match dict.get_name(b"Type") {
        Some(b"Pages") => {
            if let Some(kids) = dict.get_array(b"Kids").map(|k| k.to_vec()) {
                for kid in kids {
                    if let Some(kid_id) = kid.as_ref() {
                        collect_page_node(container, kid_id, media, crop, rotate, out)?;
                    }
                }
            }
        }
        Some(b"Page") => {
            let media_box = media.unwrap_or(PdfRect {
                x0: 0.0,
                y0: 0.0,
                x1: 595.0,
                y1: 842.0,
            });
            let crop_box = crop.unwrap_or(media_box);
            let index = out.len();
            out.push(PageInfo {
                index,
                object_id: node_id,
                media_box,
                crop_box,
                rotate,
            });
        }
        _ => {}
    }
    Ok(())
}

fn parse_rect(dict: &CosDict, key: &[u8]) -> Option<PdfRect> {
    let arr = dict.get_array(key)?;
    if arr.len() != 4 {
        return None;
    }
    let nums: Option<Vec<f64>> = arr.iter().map(CosObj::as_f64).collect();
    let nums = nums?;
    Some(PdfRect {
        x0: nums[0] as f32,
        y0: nums[1] as f32,
        x1: nums[2] as f32,
        y1: nums[3] as f32,
    })
}
