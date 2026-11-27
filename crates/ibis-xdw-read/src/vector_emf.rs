// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Page EMF records used by DocuWorks for monochrome and colour forms.

use std::collections::HashMap;
use std::sync::Arc;

use ibis_pdf::scene::{FillRule, PathCmd};
use ibis_pdf::{DisplayCmd, PdfRect};

use crate::scan::POINTS_PER_UNIT;
use crate::scan_emf::{positive, word};
use crate::{Error, Page, PageObject, Result};

#[derive(Clone)]
struct Font {
    family: String,
    height: f32,
    weight: u16,
    italic: bool,
    angle: f32,
    vertical: bool,
    charset: u8,
}

enum Object {
    Brush(Option<[u8; 4]>),
    Pattern(Arc<image::RgbaImage>),
    Font(Font),
    Pen([u8; 4], f32),
}

#[derive(Clone)]
struct State {
    unit: (f32, f32),
    placement: (f32, f32),
    multiply: bool,
    text_transparent: bool,
    current: (f32, f32),
    building_path: bool,
    window: (i32, i32),
    viewport: (i32, i32),
    brush: Option<[u8; 4]>,
    pattern: Option<Arc<image::RgbaImage>>,
    brush_origin: (f32, f32),
    font: Option<Font>,
    pen: ([u8; 4], f32),
    path_points: usize,
    text_color: [u8; 4],
    align: i32,
    clip: Option<PdfRect>,
    complex_clip: Option<Vec<PathCmd>>,
    page_bounds: PdfRect,
    path: Vec<PathCmd>,
    fill_rule: FillRule,
}

fn color(value: i32) -> [u8; 4] {
    let [r, g, b, _] = value.to_le_bytes();
    [r, g, b, 255]
}

fn utf16(bytes: &[u8]) -> impl Iterator<Item = u16> {
    bytes
        .chunks_exact(2)
        .map(|p| u16::from_le_bytes([p[0], p[1]]))
}

pub(crate) fn ansi(bytes: &[u8], charset: u8) -> Result<Vec<(char, usize)>> {
    let encoding = match charset {
        0 | 1 => encoding_rs::WINDOWS_1252,
        128 => encoding_rs::SHIFT_JIS,
        _ => return Err(Error::UnsupportedDrawing),
    };
    let mut input = bytes;
    let mut output = Vec::new();
    while let Some(&lead) = input.first() {
        let length = if charset == 128 && matches!(lead, 0x81..=0x9f | 0xe0..=0xfc) {
            2
        } else {
            1
        };
        let (encoded, rest) = input.split_at_checked(length).ok_or(Error::Invalid)?;
        let decoded = encoding
            .decode_without_bom_handling_and_without_replacement(encoded)
            .ok_or(Error::Invalid)?;
        let mut chars = decoded.chars();
        let ch = chars.next().ok_or(Error::Invalid)?;
        if chars.next().is_some() {
            return Err(Error::UnsupportedDrawing);
        }
        output.push((ch, length));
        input = rest;
    }
    Ok(output)
}

impl State {
    fn point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.window.0 as f32 + self.viewport.0 as f32) * self.unit.0 + self.placement.0,
            (y - self.window.1 as f32 + self.viewport.1 as f32) * self.unit.1 + self.placement.1,
        )
    }

    fn rect(&self, record: &[u8], at: usize) -> Result<PdfRect> {
        let (x0, y0) = self.point(word(record, at)? as f32, word(record, at + 4)? as f32);
        let (x1, y1) = self.point(word(record, at + 8)? as f32, word(record, at + 12)? as f32);
        Ok(PdfRect { x0, y0, x1, y1 })
    }

    fn fill_color(&self) -> Option<[u8; 4]> {
        self.brush
            .map(|[r, g, b, _]| [r, g, b, if self.multiply { 96 } else { 255 }])
    }

    fn shape(&self, rect: PdfRect, ellipse: bool) -> DisplayCmd {
        let PdfRect { x0, y0, x1, y1 } = rect;
        let commands = if ellipse {
            let (x, y) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let (dx, dy) = ((x1 - x0) * 0.27614237, (y1 - y0) * 0.27614237);
            vec![
                PathCmd::MoveTo(x1, y),
                PathCmd::CubicTo(x1, y + dy, x + dx, y1, x, y1),
                PathCmd::CubicTo(x - dx, y1, x0, y + dy, x0, y),
                PathCmd::CubicTo(x0, y - dy, x - dx, y0, x, y0),
                PathCmd::CubicTo(x + dx, y0, x1, y - dy, x1, y),
                PathCmd::Close,
            ]
        } else {
            vec![
                PathCmd::MoveTo(x0, y0),
                PathCmd::LineTo(x1, y0),
                PathCmd::LineTo(x1, y1),
                PathCmd::LineTo(x0, y1),
                PathCmd::Close,
            ]
        };
        DisplayCmd::DrawPath {
            bounds: rect,
            commands,
            fill_rule: self.fill_rule,
            fill_rgba: self.fill_color(),
            stroke_rgba: Some(self.pen.0).filter(|c| c[3] != 0),
            stroke_width: self.pen.1,
        }
    }

    fn comment(&mut self, comment: &[u8], output: &mut Vec<DisplayCmd>) -> Result<()> {
        match comment {
            b"DW01" | [0 | 1, 0, 0, 0] => {}
            b"DW02" => self.path.clear(),
            b"DW03" => {
                let rectangle = rectangular_path_bounds(&self.path);
                let bounds = rectangle
                    .or_else(|| path_bounds(&self.path))
                    .ok_or(Error::Invalid)?;
                let path = std::mem::take(&mut self.path);
                self.complex_clip = rectangle.is_none().then_some(path);
                self.clip = Some(intersect(self.page_bounds, bounds));
                output.push(DisplayCmd::SetClipRect(self.clip));
            }
            b"DW04" | b"DW05" => {
                let fill = comment == b"DW04";
                if let Some(bounds) = path_bounds(&self.path) {
                    output.push(DisplayCmd::DrawPath {
                        bounds,
                        commands: std::mem::take(&mut self.path),
                        fill_rule: self.fill_rule,
                        fill_rgba: if fill { self.fill_color() } else { None },
                        stroke_rgba: (!fill).then_some(self.pen.0),
                        stroke_width: self.pen.1,
                    });
                }
            }
            _ if comment.starts_with(b"DW06") => {
                self.complex_clip = None;
                self.clip = Some(intersect(self.page_bounds, self.rect(comment, 4)?));
                output.push(DisplayCmd::SetClipRect(self.clip));
            }
            [b'D', b'W', op, encoding @ (0x20 | 0x40 | 0x80), ..]
                if op & !0x13 == 0 || matches!(op, 0x20 | 0x40 | 0x50) =>
            {
                let count = positive(comment, 4)?;
                self.path_points = self.path_points.saturating_add(count);
                if count == 0
                    || self.path.len().saturating_add(count) > 65_536
                    || self.path_points > 2_000_000
                {
                    return Err(Error::Limit);
                }
                let mut data = comment.get(8..).ok_or(Error::Invalid)?;
                let mut point = (0i32, 0i32);
                let mut coordinates = Vec::with_capacity(count);
                for index in 0..count {
                    if index == 0 || *encoding == 0x20 {
                        let (pair, rest) = data.split_at_checked(4).ok_or(Error::Invalid)?;
                        data = rest;
                        point = (
                            i32::from(i16::from_le_bytes([pair[0], pair[1]])),
                            i32::from(i16::from_le_bytes([pair[2], pair[3]])),
                        );
                    } else {
                        let (dx, dy) = if *encoding == 0x40 {
                            (
                                i32::from(crate::byte(&mut data)? as i8),
                                i32::from(crate::byte(&mut data)? as i8),
                            )
                        } else {
                            let packed = crate::byte(&mut data)?;
                            (
                                nibble(packed >> 4, &mut data)?,
                                nibble(packed & 15, &mut data)?,
                            )
                        };
                        point.0 += dx;
                        point.1 += dy;
                    }
                    coordinates.push(self.point(point.0 as f32, point.1 as f32));
                }
                let mut points = coordinates.into_iter();
                let standalone = matches!(op, 0x20 | 0x40 | 0x50);
                let mut commands = Vec::with_capacity(count + 1);
                if standalone || op & 1 != 0 {
                    let (x, y) = points.next().ok_or(Error::Invalid)?;
                    commands.push(PathCmd::MoveTo(x, y));
                } else if self.path.is_empty() {
                    return Err(Error::Invalid);
                }
                while let Some((x, y)) = points.next() {
                    if op & 0x10 != 0 {
                        let (x2, y2) = points.next().ok_or(Error::Invalid)?;
                        let (end_x, end_y) = points.next().ok_or(Error::Invalid)?;
                        commands.push(PathCmd::CubicTo(x, y, x2, y2, end_x, end_y));
                    } else {
                        commands.push(PathCmd::LineTo(x, y));
                    }
                }
                if *op == 0x20 || op & 2 != 0 {
                    commands.push(PathCmd::Close);
                }
                if standalone {
                    let fill = *op == 0x20;
                    output.push(DisplayCmd::DrawPath {
                        bounds: path_bounds(&commands).ok_or(Error::Invalid)?,
                        commands,
                        fill_rule: self.fill_rule,
                        fill_rgba: if fill { self.fill_color() } else { None },
                        stroke_rgba: (!fill && self.pen.0[3] != 0).then_some(self.pen.0),
                        stroke_width: self.pen.1,
                    });
                } else {
                    self.path.extend(commands);
                }
            }
            _ => return Err(Error::UnsupportedDrawing),
        }
        Ok(())
    }

    fn text(&self, record: &[u8], unicode: bool, output: &mut Vec<DisplayCmd>) -> Result<()> {
        if !self.text_transparent {
            return Err(Error::UnsupportedDrawing);
        }
        let font = self.font.as_ref().ok_or(Error::UnsupportedDrawing)?;
        let count = positive(record, 44)?;
        if count.saturating_add(output.len()) > 250_000 {
            return Err(Error::Limit);
        }
        let start = positive(record, 48)?;
        let options = word(record, 52)?;
        if options & !6 != 0 || self.align & !30 != 0 {
            return Err(Error::UnsupportedDrawing);
        }
        let chars = record
            .get(start..start + if unicode { 2 * count } else { count })
            .ok_or(Error::Invalid)?;
        let dx = positive(record, 72)?;
        if dx == 0 && self.align & 6 != 0 {
            return Err(Error::UnsupportedDrawing);
        }
        let advance = if dx == 0 {
            None
        } else {
            Some(record.get(dx..dx + 4 * count).ok_or(Error::Invalid)?)
        };
        let total = advance.map_or(0.0, |data| {
            data.chunks_exact(4)
                .map(|d| i32::from_le_bytes(d.try_into().unwrap()) as f32)
                .sum()
        });
        let mut distance = -match self.align & 6 {
            2 => total,
            6 => total / 2.0,
            _ => 0.0,
        };
        let baseline = match self.align & 24 {
            24 => 0.0,
            8 => -0.12 * font.height,
            _ => 0.88 * font.height,
        };
        let (sin, cos) = font.angle.sin_cos();
        let x = word(record, 36)? as f32 - baseline * sin;
        let y = word(record, 40)? as f32 + baseline * cos;
        if options & 2 != 0 {
            return Err(Error::UnsupportedDrawing);
        }
        if options & 4 != 0 {
            let clip = intersect(
                self.clip.unwrap_or(self.page_bounds),
                self.rect(record, 56)?,
            );
            output.push(DisplayCmd::SetClipRect(Some(clip)));
        }
        let glyph = |text, distance, max_width, upright| {
            let inset = if upright {
                (font.height * 0.5, -font.height * 0.38)
            } else {
                (0.0, 0.0)
            };
            let (dx, dy) = (distance + inset.0, inset.1);
            let (mut ox, mut oy) = self.point(x + dx * cos - dy * sin, y + dx * sin + dy * cos);
            let size = font.height * self.unit.1;
            if upright {
                ox -= size / 2.0;
                oy += size * 0.38;
            }
            DisplayCmd::DrawStyledGlyphRun {
                text,
                origin: (ox, oy),
                font_size: size,
                family: font.family.clone(),
                weight: font.weight,
                italic: font.italic,
                angle: if upright {
                    font.angle - std::f32::consts::FRAC_PI_2
                } else {
                    font.angle
                },
                horizontal_scale: 1.0,
                max_width,
                rgba: self.text_color,
            }
        };
        let decoded = if unicode {
            char::decode_utf16(utf16(chars))
                .map(|ch| {
                    let ch = ch.map_err(|_| Error::Invalid)?;
                    Ok((ch, ch.len_utf16()))
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            ansi(chars, font.charset)?
        };
        if let Some(advance) = advance {
            let mut index = 0;
            for (ch, units) in decoded {
                let mut spacing = 0.0;
                for _ in 0..units {
                    spacing += word(advance, index)? as f32;
                    index += 4;
                }
                if !ch.is_whitespace() {
                    let width = spacing * self.unit.0;
                    let upright = font.vertical
                        && matches!(ch, '\u{3041}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}');
                    output.push(glyph(
                        ch.to_string(),
                        distance,
                        (width > 0.0).then_some(width),
                        upright,
                    ));
                }
                distance += spacing;
            }
        } else {
            let text: String = decoded.into_iter().map(|(ch, _)| ch).collect();
            if font.vertical {
                return Err(Error::UnsupportedDrawing);
            }
            output.push(glyph(text, distance, None, false));
        }
        if options & 4 != 0 {
            output.push(DisplayCmd::SetClipRect(self.clip));
        }
        Ok(())
    }
}

fn rectangular_path_bounds(path: &[PathCmd]) -> Option<PdfRect> {
    use PathCmd::{Close, LineTo, MoveTo};
    let points = match path {
        [
            MoveTo(x, y),
            LineTo(x1, y1),
            LineTo(x2, y2),
            LineTo(x3, y3),
            Close,
        ] => [(*x, *y), (*x1, *y1), (*x2, *y2), (*x3, *y3)],
        [
            MoveTo(x, y),
            LineTo(x1, y1),
            LineTo(x2, y2),
            LineTo(x3, y3),
            LineTo(x4, y4),
            Close,
        ] if (x, y) == (x4, y4) => [(*x, *y), (*x1, *y1), (*x2, *y2), (*x3, *y3)],
        _ => return None,
    };
    let bounds = path_bounds(path)?;
    let corners = [
        (bounds.x0, bounds.y0),
        (bounds.x1, bounds.y0),
        (bounds.x1, bounds.y1),
        (bounds.x0, bounds.y1),
    ];
    if bounds.x0 >= bounds.x1
        || bounds.y0 >= bounds.y1
        || !corners.iter().all(|corner| points.contains(corner))
        || !points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .all(|(a, b)| a.0 == b.0 || a.1 == b.1)
    {
        return None;
    }
    Some(bounds)
}

fn nibble(value: u8, data: &mut &[u8]) -> Result<i32> {
    match value {
        8 => Ok(i32::from(crate::byte(data)? as i8)),
        9..=15 => Ok(i32::from(value) - 16),
        _ => Ok(i32::from(value)),
    }
}

fn path_bounds(path: &[PathCmd]) -> Option<PdfRect> {
    let mut bounds: Option<PdfRect> = None;
    let mut include = |x: f32, y: f32| match &mut bounds {
        Some(b) => {
            b.x0 = b.x0.min(x);
            b.x1 = b.x1.max(x);
            b.y0 = b.y0.min(y);
            b.y1 = b.y1.max(y);
        }
        None => {
            bounds = Some(PdfRect {
                x0: x,
                x1: x,
                y0: y,
                y1: y,
            })
        }
    };
    for command in path {
        match *command {
            PathCmd::MoveTo(x, y) | PathCmd::LineTo(x, y) => include(x, y),
            PathCmd::CubicTo(x, y, x2, y2, end_x, end_y) => {
                include(x, y);
                include(x2, y2);
                include(end_x, end_y);
            }
            _ => {}
        }
    }
    bounds
}

fn intersect(a: PdfRect, b: PdfRect) -> PdfRect {
    let x0 = a.x0.max(b.x0);
    let y0 = a.y0.max(b.y0);
    PdfRect {
        x0,
        y0,
        x1: a.x1.min(b.x1).max(x0),
        y1: a.y1.min(b.y1).max(y0),
    }
}

pub(crate) fn render(
    page: &Page<'_>,
    object: &PageObject<'_>,
    emf: &[u8],
    mut picture_command: impl FnMut(u32, &[u8], PdfRect, &mut u64) -> Result<DisplayCmd>,
) -> Result<Vec<DisplayCmd>> {
    let size = positive(emf, 4)?;
    let header = emf.get(..size).ok_or(Error::Invalid)?;
    if word(header, 0)? != 1 || header.get(40..44) != Some(b" EMF") {
        return Err(Error::Invalid);
    }
    let page_content = object.kind == 0x8010;
    let rotation = object.rotation.rem_euclid(360);
    if !matches!(rotation, 0 | 90 | 180 | 270) || (!page_content && rotation != 0) {
        return Err(Error::UnsupportedDrawing);
    }
    let (width, height) = if matches!(rotation, 90 | 270) {
        (page.height, page.width)
    } else {
        (page.width, page.height)
    };
    if page_content
        && (word(header, 24)? != 0
            || word(header, 28)? != 0
            || positive(header, 32)? != width as usize
            || positive(header, 36)? != height as usize)
    {
        return Err(Error::UnsupportedDrawing);
    }
    let dx = positive(header, 72)?;
    let dy = positive(header, 76)?;
    let mx = positive(header, 80)?;
    let my = positive(header, 84)?;
    if [dx, dy, mx, my].contains(&0) {
        return Err(Error::Invalid);
    }
    let mut state = State {
        unit: (
            100.0 * mx as f32 / dx as f32 * POINTS_PER_UNIT,
            100.0 * my as f32 / dy as f32 * POINTS_PER_UNIT,
        ),
        placement: (
            object.x as f32 * POINTS_PER_UNIT,
            object.y as f32 * POINTS_PER_UNIT,
        ),
        multiply: false,
        text_transparent: true,
        current: (0.0, 0.0),
        building_path: false,
        path: Vec::new(),
        path_points: 0,
        pen: (color(0), POINTS_PER_UNIT),
        fill_rule: FillRule::EvenOdd,
        window: (0, 0),
        viewport: (0, 0),
        brush: Some(color(0x00ff_ffff)),
        pattern: None,
        brush_origin: (0.0, 0.0),
        font: None,
        text_color: color(0),
        align: 0,
        clip: None,
        complex_clip: None,
        page_bounds: PdfRect {
            x0: 0.0,
            y0: 0.0,
            x1: width as f32 * POINTS_PER_UNIT,
            y1: height as f32 * POINTS_PER_UNIT,
        },
    };
    let mut objects = HashMap::new();
    let mut output = Vec::new();
    let mut rest = &emf[size..];
    let mut has_window = page_content;
    let mut saved_states = Vec::new();
    let mut picture = 300u32;
    let mut picture_selected = false;
    let mut raster_budget = 16_777_216u64;
    while !rest.is_empty() {
        let size = positive(rest, 4)?;
        if size < 8 || size % 4 != 0 {
            return Err(Error::Invalid);
        }
        let (record, tail) = rest.split_at_checked(size).ok_or(Error::Invalid)?;
        rest = tail;
        let before = output.len();
        match word(record, 0)? {
            9 if !page_content && !has_window && output.is_empty() => {
                let (width, height) = object.size.ok_or(Error::UnsupportedDrawing)?;
                let (wx, wy) = (positive(record, 8)?, positive(record, 12)?);
                if wx == 0 || wy == 0 {
                    return Err(Error::Invalid);
                }
                state.unit = (
                    width as f32 * POINTS_PER_UNIT / wx as f32,
                    height as f32 * POINTS_PER_UNIT / wy as f32,
                );
                has_window = true;
            }
            10 => state.window = (word(record, 8)?, word(record, 12)?),
            12 => state.viewport = (word(record, 8)?, word(record, 12)?),
            // Brush origin is irrelevant to solid brushes. This subset uses
            // transparent backgrounds and no pen drawing.
            58 => {}
            13 => {
                state.brush_origin = (
                    word(record, 8)? as f32 * state.unit.0,
                    word(record, 12)? as f32 * state.unit.1,
                )
            }
            27 => {
                state.current = (word(record, 8)? as f32, word(record, 12)? as f32);
                if state.building_path {
                    let (x, y) = state.point(state.current.0, state.current.1);
                    state.path.push(PathCmd::MoveTo(x, y));
                }
            }
            33 => {
                if saved_states.len() >= 32 || state.building_path || !state.path.is_empty() {
                    return Err(Error::UnsupportedDrawing);
                }
                saved_states.push(state.clone());
            }
            34 if word(record, 8)? == -1 && !state.building_path && state.path.is_empty() => {
                let points = state.path_points;
                state = saved_states.pop().ok_or(Error::Invalid)?;
                state.path_points = points;
                output.push(DisplayCmd::SetClipRect(state.clip));
            }
            54 if has_window => {
                let (end_x, end_y) = (word(record, 8)? as f32, word(record, 12)? as f32);
                let (x0, y0) = state.point(state.current.0, state.current.1);
                let (x1, y1) = state.point(end_x, end_y);
                if state.building_path {
                    if state.path.len() >= 65_536 {
                        return Err(Error::Limit);
                    }
                    if state.path.is_empty() {
                        state.path.push(PathCmd::MoveTo(x0, y0));
                    }
                    state.path.push(PathCmd::LineTo(x1, y1));
                } else {
                    output.push(DisplayCmd::DrawPath {
                        bounds: PdfRect {
                            x0: x0.min(x1),
                            y0: y0.min(y1),
                            x1: x0.max(x1),
                            y1: y0.max(y1),
                        },
                        commands: vec![PathCmd::MoveTo(x0, y0), PathCmd::LineTo(x1, y1)],
                        fill_rule: state.fill_rule,
                        fill_rgba: None,
                        stroke_rgba: (state.pen.0[3] != 0).then_some(state.pen.0),
                        stroke_width: state.pen.1,
                    });
                }
                state.current = (end_x, end_y);
            }
            59 if !state.building_path => {
                state.path.clear();
                state.building_path = true;
            }
            60 if state.building_path => state.building_path = false,
            61 if state.building_path && !state.path.is_empty() => state.path.push(PathCmd::Close),
            kind @ (62 | 63 | 64) if !state.building_path && has_window => {
                let bounds = path_bounds(&state.path).ok_or(Error::Invalid)?;
                output.push(DisplayCmd::DrawPath {
                    bounds,
                    commands: std::mem::take(&mut state.path),
                    fill_rule: state.fill_rule,
                    fill_rgba: (kind != 64).then(|| state.fill_color()).flatten(),
                    stroke_rgba: (kind != 62 && state.pen.0[3] != 0).then_some(state.pen.0),
                    stroke_width: state.pen.1,
                });
            }
            75 if word(record, 8)? == 0 && word(record, 12)? == 5 => {
                state.clip = None;
                state.complex_clip = None;
                output.push(DisplayCmd::SetClipRect(None));
            }
            18 => {
                state.text_transparent = match word(record, 8)? {
                    1 => true,
                    2 => false,
                    _ => return Err(Error::UnsupportedDrawing),
                }
            }
            21 if word(record, 8)? == 3 => {}
            20 => {
                state.multiply = match word(record, 8)? {
                    9 => true,
                    13 => false,
                    _ => return Err(Error::UnsupportedDrawing),
                }
            }
            30 if has_window => {
                state.clip = Some(intersect(
                    state.clip.unwrap_or(state.page_bounds),
                    state.rect(record, 8)?,
                ));
                output.push(DisplayCmd::SetClipRect(state.clip));
            }
            kind @ (42 | 43) if has_window => {
                output.push(state.shape(state.rect(record, 8)?, kind == 42));
            }
            4 if has_window => {
                let count = positive(record, 24)?;
                if !(2..=65_536).contains(&count) {
                    return Err(Error::Limit);
                }
                state.path_points = state.path_points.saturating_add(count);
                if state.path_points > 2_000_000 {
                    return Err(Error::Limit);
                }
                let mut commands = Vec::with_capacity(count);
                for i in 0..count {
                    let offset = 28 + i * 8;
                    let (x, y) = state.point(
                        word(record, offset)? as f32,
                        word(record, offset + 4)? as f32,
                    );
                    commands.push(if i == 0 {
                        PathCmd::MoveTo(x, y)
                    } else {
                        PathCmd::LineTo(x, y)
                    });
                }
                output.push(DisplayCmd::DrawPath {
                    bounds: path_bounds(&commands).ok_or(Error::Invalid)?,
                    commands,
                    fill_rule: state.fill_rule,
                    fill_rgba: None,
                    stroke_rgba: Some(state.pen.0).filter(|c| c[3] != 0),
                    stroke_width: state.pen.1,
                });
            }
            19 => {
                state.fill_rule = match word(record, 8)? {
                    1 => FillRule::EvenOdd,
                    2 => FillRule::NonZero,
                    _ => return Err(Error::UnsupportedDrawing),
                }
            }
            14 if rest.is_empty() && has_window && !picture_selected => {
                if rotation != 0 {
                    rotate_commands(
                        &mut output,
                        rotation,
                        page.width as f32 * POINTS_PER_UNIT,
                        page.height as f32 * POINTS_PER_UNIT,
                    )?;
                }
                return Ok(output);
            }
            22 => state.align = word(record, 8)?,
            24 => state.text_color = color(word(record, 8)?),
            // Opaque text is rejected by the text interpreter.
            25 => {
                let _ = word(record, 8)?;
            }
            37 => match word(record, 8)? as u32 {
                0x8000_0000 => {
                    state.brush = Some(color(0x00ff_ffff));
                    state.pattern = None;
                }
                0x8000_0004 => {
                    state.brush = Some(color(0));
                    state.pattern = None;
                }
                0x8000_0005 => {
                    state.brush = None;
                    state.pattern = None;
                }
                0x8000_0008 => state.pen = ([0; 4], 0.0),
                0x8000_0007 => state.pen = (color(0), state.unit.0),
                0x8000_000d => state.font = None,
                handle => match objects.get(&handle) {
                    Some(Object::Brush(brush)) => {
                        state.brush = *brush;
                        state.pattern = None;
                    }
                    Some(Object::Pattern(tile)) => {
                        state.brush = Some(color(0));
                        state.pattern = Some(tile.clone());
                    }
                    Some(Object::Font(font)) => state.font = Some(font.clone()),
                    Some(Object::Pen(color, width)) => state.pen = (*color, *width),
                    None => return Err(Error::UnsupportedDrawing),
                },
            },
            38 if word(record, 12)? == 0 => {
                let width = positive(record, 16)?.max(1) as f32 * state.unit.0;
                objects.insert(
                    word(record, 8)? as u32,
                    Object::Pen(color(word(record, 24)?), width),
                );
            }
            39 => {
                let brush = match word(record, 12)? {
                    0 => Some(color(word(record, 16)?)),
                    1 => None,
                    _ => return Err(Error::UnsupportedDrawing),
                };
                objects.insert(word(record, 8)? as u32, Object::Brush(brush));
            }
            40 => {
                objects.remove(&(word(record, 8)? as u32));
            }
            70 => {
                let size = positive(record, 8)?;
                let comment = record.get(12..12 + size).ok_or(Error::Invalid)?;
                match comment.get(..4) {
                    Some(b"DWb\0") if picture < 400 && !picture_selected => {
                        picture += 1;
                        picture_selected = true;
                    }
                    Some(b"DWc\0") if has_window && picture_selected => {
                        let x = word(comment, 20)? as f32;
                        let y = word(comment, 24)? as f32;
                        let (x0, y0) = state.point(x, y);
                        let (x1, y1) = state
                            .point(x + word(comment, 52)? as f32, y + word(comment, 56)? as f32);
                        if !(x1 > x0 && y1 > y0) {
                            return Err(Error::UnsupportedDrawing);
                        }
                        output.push(picture_command(
                            picture,
                            comment,
                            PdfRect { x0, y0, x1, y1 },
                            &mut raster_budget,
                        )?);
                        picture_selected = false;
                    }
                    _ => state.comment(comment, &mut output)?,
                }
            }
            // DSTCOPY leaves the destination untouched.
            76 if (word(record, 40)? as u32 >> 16) & 255 == 0xaa => {}
            81 if has_window => {
                let image = crate::bitmap::stretch_dib(record)?;
                raster_budget = raster_budget
                    .checked_sub(u64::from(image.width()) * u64::from(image.height()))
                    .ok_or(Error::Limit)?;
                let x = word(record, 24)? as f32;
                let y = word(record, 28)? as f32;
                let (x0, y0) = state.point(x, y);
                let (x1, y1) =
                    state.point(x + word(record, 72)? as f32, y + word(record, 76)? as f32);
                if !(x1 > x0 && y1 > y0) {
                    return Err(Error::UnsupportedDrawing);
                }
                output.push(DisplayCmd::DrawImage {
                    bounds: PdfRect { x0, y0, x1, y1 },
                    width: image.width(),
                    height: image.height(),
                    rgba: image.into_raw(),
                });
            }
            76 if word(record, 40)? == 0x00f0_0021
                && word(record, 88)? == 0
                && word(record, 96)? == 0 =>
            {
                let x = word(record, 24)? as f32;
                let y = word(record, 28)? as f32;
                let (x0, y0) = state.point(x, y);
                let (x1, y1) =
                    state.point(x + word(record, 32)? as f32, y + word(record, 36)? as f32);
                // Preserve coverage for printer rules thinner than a screen
                // pixel. GPUI rasterizes filled paths with antialiasing.
                output.push(DisplayCmd::DrawPath {
                    bounds: PdfRect {
                        x0: x0.min(x1),
                        y0: y0.min(y1),
                        x1: x0.max(x1),
                        y1: y0.max(y1),
                    },
                    commands: vec![
                        PathCmd::MoveTo(x0, y0),
                        PathCmd::LineTo(x1, y0),
                        PathCmd::LineTo(x1, y1),
                        PathCmd::LineTo(x0, y1),
                        PathCmd::Close,
                    ],
                    fill_rule: FillRule::NonZero,
                    fill_rgba: state.brush,
                    stroke_rgba: None,
                    stroke_width: 0.0,
                });
            }
            82 if word(record, 24)? == 0 => {
                let family = String::from_utf16(
                    &utf16(record.get(40..104).ok_or(Error::Invalid)?)
                        .take_while(|unit| *unit != 0)
                        .collect::<Vec<_>>(),
                )
                .map_err(|_| Error::Invalid)?;
                let vertical = family.starts_with('@');
                let font = Font {
                    family: family.trim_start_matches('@').to_owned(),
                    vertical,
                    height: word(record, 12)?.unsigned_abs() as f32,
                    weight: word(record, 28)?.clamp(100, 900) as u16,
                    italic: record[32] != 0,
                    charset: record[35],
                    angle: -(word(record, 20)? as f32 / 10.0).to_radians(),
                };
                objects.insert(word(record, 8)? as u32, Object::Font(font));
            }
            94 if word(record, 12)? == 0 => {
                let tile = crate::bitmap::dib(record, 16, 4096)?;
                if tile.width() == 0 || tile.height() == 0 {
                    return Err(Error::Invalid);
                }
                objects.insert(word(record, 8)? as u32, Object::Pattern(Arc::new(tile)));
            }
            95 if word(record, 16)? == 0
                && word(record, 24)? == 0
                && word(record, 28)? & !0x0001_3300 == 0
                && word(record, 36)? == 0
                && word(record, 48)? == 0 =>
            {
                let width = positive(record, 32)?.max(1) as f32 * state.unit.0;
                objects.insert(
                    word(record, 8)? as u32,
                    Object::Pen(color(word(record, 40)?), width),
                );
            }
            kind @ (83 | 84) if has_window => state.text(record, kind == 84, &mut output)?,
            _ => return Err(Error::UnsupportedDrawing),
        }
        if output.len() > before && (state.pattern.is_some() || state.complex_clip.is_some()) {
            let bounds = state.clip.unwrap_or(state.page_bounds);
            let added = output.split_off(before);
            for command in added {
                let command = if let Some(tile) = &state.pattern
                    && matches!(
                        command,
                        DisplayCmd::DrawPath {
                            fill_rgba: Some(_),
                            ..
                        }
                    ) {
                    crate::path_clip::tile_fill(
                        command,
                        tile,
                        state.brush_origin,
                        state.unit,
                        bounds,
                        &mut raster_budget,
                    )?
                } else {
                    Some(command)
                };
                if let Some(command) = command {
                    if let Some(clip) = &state.complex_clip {
                        if let Some(command) =
                            crate::path_clip::apply(command, clip, bounds, &mut raster_budget)?
                        {
                            output.push(command);
                        }
                    } else {
                        output.push(command);
                    }
                }
            }
        }
        if output.len() > 250_000 || objects.len() > 65_536 {
            return Err(Error::Limit);
        }
    }
    Err(Error::Invalid)
}

fn rotate_commands(
    commands: &mut [DisplayCmd],
    rotation: i64,
    width: f32,
    height: f32,
) -> Result<()> {
    let point = |(x, y): (f32, f32)| match rotation {
        90 => (width - y, x),
        180 => (width - x, height - y),
        270 => (y, height - x),
        _ => (x, y),
    };
    let rect = |r: &mut PdfRect| {
        let a = point((r.x0, r.y0));
        let b = point((r.x1, r.y1));
        *r = PdfRect {
            x0: a.0.min(b.0),
            y0: a.1.min(b.1),
            x1: a.0.max(b.0),
            y1: a.1.max(b.1),
        };
    };
    for command in commands {
        match command {
            DisplayCmd::SetClipRect(clip) => {
                if let Some(bounds) = clip {
                    rect(bounds);
                }
            }
            DisplayCmd::DrawPath {
                bounds, commands, ..
            } => {
                rect(bounds);
                for command in commands {
                    *command = match *command {
                        PathCmd::MoveTo(x, y) => {
                            let (x, y) = point((x, y));
                            PathCmd::MoveTo(x, y)
                        }
                        PathCmd::LineTo(x, y) => {
                            let (x, y) = point((x, y));
                            PathCmd::LineTo(x, y)
                        }
                        PathCmd::QuadTo(a, b, x, y) => {
                            let (a, b) = point((a, b));
                            let (x, y) = point((x, y));
                            PathCmd::QuadTo(a, b, x, y)
                        }
                        PathCmd::CubicTo(a, b, c, d, x, y) => {
                            let (a, b) = point((a, b));
                            let (c, d) = point((c, d));
                            let (x, y) = point((x, y));
                            PathCmd::CubicTo(a, b, c, d, x, y)
                        }
                        PathCmd::Close => PathCmd::Close,
                    };
                }
            }
            DisplayCmd::DrawStyledGlyphRun { origin, angle, .. } => {
                *origin = point(*origin);
                *angle += (rotation as f32).to_radians();
            }
            DisplayCmd::DrawImage {
                bounds,
                width,
                height,
                rgba,
            } => {
                rect(bounds);
                let image = image::RgbaImage::from_raw(*width, *height, std::mem::take(rgba))
                    .ok_or(Error::Invalid)?;
                let image = match rotation {
                    90 => image::imageops::rotate90(&image),
                    180 => image::imageops::rotate180(&image),
                    270 => image::imageops::rotate270(&image),
                    _ => image,
                };
                (*width, *height) = image.dimensions();
                *rgba = image.into_raw();
            }
            _ => return Err(Error::UnsupportedDrawing),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_image_content_with_its_page_bounds() {
        let mut commands = [DisplayCmd::DrawImage {
            bounds: PdfRect {
                x0: 1.0,
                y0: 2.0,
                x1: 3.0,
                y1: 3.0,
            },
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
        }];
        rotate_commands(&mut commands, 270, 5.0, 8.0).unwrap();
        let DisplayCmd::DrawImage {
            bounds,
            width,
            height,
            rgba,
        } = &commands[0]
        else {
            panic!("expected rotated image");
        };
        assert_eq!((*width, *height), (1, 2));
        assert_eq!(
            (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
            (2.0, 5.0, 3.0, 7.0)
        );
        assert_eq!(rgba, &[0, 0, 255, 255, 255, 0, 0, 255]);
    }

    fn put(bytes: &mut [u8], at: usize, value: i32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn record(kind: i32, payload: &[u8]) -> Vec<u8> {
        let size = (payload.len() + 8).next_multiple_of(4);
        let mut result = vec![0; size];
        put(&mut result, 0, kind);
        put(&mut result, 4, size as i32);
        result[8..8 + payload.len()].copy_from_slice(payload);
        result
    }

    fn words(values: &[i32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    fn comment(data: &[u8]) -> Vec<u8> {
        record(70, &[&(data.len() as i32).to_le_bytes()[..], data].concat())
    }

    fn fixture() -> Vec<u8> {
        let mut header = record(1, &[0; 80]);
        put(&mut header, 32, 21000);
        put(&mut header, 36, 29700);
        header[40..44].copy_from_slice(b" EMF");
        for (offset, value) in [(72, 2100), (76, 2970), (80, 210), (84, 297)] {
            put(&mut header, offset, value);
        }
        let mut font = record(82, &[0; 96]);
        put(&mut font, 8, 2);
        put(&mut font, 12, -20);
        put(&mut font, 28, 700);
        let mut blt = record(76, &[0; 92]);
        for (offset, value) in [(24, 12), (28, 13), (32, 30), (36, 2), (40, 0x00f0_0021)] {
            put(&mut blt, offset, value);
        }
        let mut text = record(84, &[0; 80]);
        for (offset, value) in [
            (36, 22),
            (40, 30),
            (44, 2),
            (48, 76),
            (72, 80),
            (80, 12),
            (84, 16),
        ] {
            put(&mut text, offset, value);
        }
        text[76..78].copy_from_slice(&0x6a4bu16.to_le_bytes());
        text[78..80].copy_from_slice(&0x6881u16.to_le_bytes());
        let points = [10i16, 10, 20, 10, 22, 20, 28, 15];
        let points: Vec<_> = points
            .iter()
            .flat_map(|point| point.to_le_bytes())
            .collect();
        [
            header,
            record(39, &words(&[1, 0, 0x0000_00ff, 0])),
            record(37, &1i32.to_le_bytes()),
            blt,
            font,
            record(37, &2i32.to_le_bytes()),
            record(22, &24i32.to_le_bytes()),
            comment(&[&b"DW06"[..], &words(&[10, 10, 90, 90])].concat()),
            text,
            comment(b"DW02"),
            comment(&[&b"DW\x13\x20"[..], &4i32.to_le_bytes(), &points].concat()),
            comment(b"DW04"),
            record(14, &[0; 12]),
        ]
        .concat()
    }

    fn draw(emf: &[u8]) -> Result<Vec<DisplayCmd>> {
        let object = PageObject {
            kind: 0x8010,
            x: 0,
            y: 0,
            rotation: 0,
            size: Some((21000, 29700)),
            drawing: None,
            attributes: &[],
        };
        let page = Page {
            width: 21000,
            height: 29700,
            objects: vec![object],
        };
        render(&page, &page.objects[0], emf, |_, _, _, _| {
            Err(Error::UnsupportedDrawing)
        })
    }

    #[test]
    fn draws_patcopy_positioned_unicode_and_cubic_path() {
        let output = draw(&fixture()).unwrap();
        let [
            DisplayCmd::DrawPath {
                commands: rectangle,
                fill_rgba: Some([255, 0, 0, 255]),
                ..
            },
            DisplayCmd::SetClipRect(Some(_)),
            DisplayCmd::DrawStyledGlyphRun {
                text: first,
                origin: a,
                weight: 700,
                ..
            },
            DisplayCmd::DrawStyledGlyphRun {
                text: second,
                origin: b,
                ..
            },
            DisplayCmd::DrawPath { commands, .. },
        ] = output.as_slice()
        else {
            panic!("unexpected display list: {output:?}");
        };
        assert_eq!(rectangle.len(), 5);
        assert_eq!((first.as_str(), second.as_str()), ("橋", "梁"));
        assert!((b.0 - a.0 - 120.0 * POINTS_PER_UNIT).abs() < 0.001);
        assert_eq!(a.1, b.1);
        assert!(matches!(
            commands.as_slice(),
            [PathCmd::MoveTo(..), PathCmd::CubicTo(..), PathCmd::Close]
        ));
    }

    #[test]
    fn accepts_only_axis_aligned_rectangular_path_clips() {
        let mut path = vec![
            PathCmd::MoveTo(0.0, 0.0),
            PathCmd::LineTo(4.0, 0.0),
            PathCmd::LineTo(4.0, 2.0),
            PathCmd::LineTo(0.0, 2.0),
            PathCmd::Close,
        ];
        assert_eq!(
            rectangular_path_bounds(&path),
            Some(PdfRect {
                x0: 0.0,
                y0: 0.0,
                x1: 4.0,
                y1: 2.0
            })
        );
        path[2] = PathCmd::LineTo(3.0, 2.0);
        assert_eq!(rectangular_path_bounds(&path), None);
        let mut escape = &[0xfb][..];
        assert_eq!(nibble(8, &mut escape), Ok(-5));
        assert_eq!(nibble(15, &mut escape), Ok(-1));
        assert_eq!(nibble(8, &mut escape), Err(Error::Invalid));
    }

    #[test]
    fn rejects_unknown_records_and_truncated_drawings() {
        let mut emf = fixture();
        let eof = emf.len() - 20;
        put(&mut emf, eof, 999);
        assert_eq!(draw(&emf), Err(Error::UnsupportedDrawing));
        assert_eq!(draw(&emf[..emf.len() - 1]), Err(Error::Invalid));
    }
}
