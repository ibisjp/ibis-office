// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Native XPS / OpenXPS (ECMA-388) container parser and [`ibis_pdf::scene::PageScene`] builder.

use std::sync::Arc;

use ibis_ooxml::zip::Archive;
use ibis_pdf::cos::ObjectId;
use ibis_pdf::scene::{
    CharBox, DisplayCmd, FillRule, PageInfo, PageScene, PageTextIndex, PathCmd, PdfRect,
};
use roxmltree::{Document as XmlDoc, Node};

const PT_PER_XPS_PX: f32 = 72.0 / 96.0;
const MAX_ENTRY_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug)]
struct XpsPageRef {
    path: String,
    width_px: f32,
    height_px: f32,
}

/// Parsed XPS / OpenXPS package backed by an in-memory ZIP archive.
pub struct XpsContainer {
    archive: Archive,
    pages: Vec<XpsPageRef>,
    signature_count: usize,
}

/// Returns `true` if `bytes` looks like an XPS/OpenXPS OPC ZIP package.
pub fn is_xps(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"PK\x03\x04") {
        return false;
    }
    let Ok(archive) = Archive::new(Arc::from(bytes)) else {
        return false;
    };
    archive.entries().iter().any(|e| {
        e.name.eq_ignore_ascii_case("FixedDocSeq.fdseq")
            || e.name.eq_ignore_ascii_case("FixedDocumentSequence.fdseq")
            || e.name.to_ascii_lowercase().ends_with(".fdseq")
    })
}

impl XpsContainer {
    /// Parse an XPS / OpenXPS ZIP package from raw bytes.
    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let archive = Archive::new(bytes.into()).map_err(|e| Error::new(e.to_string()))?;
        let fdseq_path = find_fdseq_path(&archive)?;
        let fdseq_xml = read_entry_utf8(&archive, &fdseq_path)?;
        let fdseq_doc =
            XmlDoc::parse(&fdseq_xml).map_err(|e| Error::new(format!("{fdseq_path}: {e}")))?;
        let fdseq_dir = parent_dir(&fdseq_path);

        let mut pages = Vec::new();
        for doc_ref in fdseq_doc
            .descendants()
            .filter(|n| is_tag(*n, "DocumentReference"))
        {
            let Some(src) = doc_ref.attribute("Source") else {
                continue;
            };
            let fdoc_path = resolve_path(fdseq_dir, src)?;
            let fdoc_xml = read_entry_utf8(&archive, &fdoc_path)?;
            let fdoc_doc =
                XmlDoc::parse(&fdoc_xml).map_err(|e| Error::new(format!("{fdoc_path}: {e}")))?;
            let fdoc_dir = parent_dir(&fdoc_path);

            for page_content in fdoc_doc
                .descendants()
                .filter(|n| is_tag(*n, "PageContent"))
            {
                let Some(page_src) = page_content.attribute("Source") else {
                    continue;
                };
                let page_path = resolve_path(fdoc_dir, page_src)?;
                let mut width_px = page_content
                    .attribute("Width")
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(793.7);
                let mut height_px = page_content
                    .attribute("Height")
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(1122.5);

                if let Ok(page_xml) = read_entry_utf8(&archive, &page_path)
                    && let Ok(page_doc) = XmlDoc::parse(&page_xml)
                {
                    let root = page_doc.root_element();
                    if let Some(w) = root.attribute("Width").and_then(|s| s.parse::<f32>().ok()) {
                        width_px = w;
                    }
                    if let Some(h) = root.attribute("Height").and_then(|s| s.parse::<f32>().ok()) {
                        height_px = h;
                    }
                }

                pages.push(XpsPageRef {
                    path: page_path,
                    width_px,
                    height_px,
                });
            }
        }

        let signature_count = archive
            .entries()
            .iter()
            .filter(|e| e.name.to_ascii_lowercase().ends_with(".psdsxs"))
            .count();

        Ok(Self {
            archive,
            pages,
            signature_count,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn signature_count(&self) -> usize {
        self.signature_count
    }

    /// Return [`PageInfo`] metadata for all pages (converted from 96 DPI px to 72 pt/in).
    pub fn page_infos(&self) -> Vec<PageInfo> {
        self.pages
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                let rect = PdfRect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: p.width_px * PT_PER_XPS_PX,
                    y1: p.height_px * PT_PER_XPS_PX,
                };
                PageInfo {
                    index: idx,
                    object_id: ObjectId::new(idx as u32 + 1, 0),
                    media_box: rect,
                    crop_box: rect,
                    rotate: 0,
                }
            })
            .collect()
    }

    /// Interpret a single `.fpage` into a [`PageScene`].
    pub fn interpret_page_scene(&self, page_index: usize) -> Result<PageScene> {
        let page = self
            .pages
            .get(page_index)
            .ok_or_else(|| Error::new(format!("page index {page_index} out of range")))?;

        let page_xml = read_entry_utf8(&self.archive, &page.path)?;
        let doc = XmlDoc::parse(&page_xml).map_err(|e| Error::new(format!("{}: {e}", page.path)))?;
        let root = doc.root_element();

        let width_px = root
            .attribute("Width")
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(page.width_px);
        let height_px = root
            .attribute("Height")
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(page.height_px);
        let rect = PdfRect {
            x0: 0.0,
            y0: 0.0,
            x1: width_px * PT_PER_XPS_PX,
            y1: height_px * PT_PER_XPS_PX,
        };
        let info = PageInfo {
            index: page_index,
            object_id: ObjectId::new(page_index as u32 + 1, 0),
            media_box: rect,
            crop_box: rect,
            rotate: 0,
        };

        let mut display_list = Vec::new();
        let mut chars = Vec::new();
        let identity = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        self.render_children(root, identity, &mut display_list, &mut chars);

        Ok(PageScene {
            info,
            display_list,
            text_index: PageTextIndex { chars },
        })
    }

    fn render_children(
        &self,
        parent: Node<'_, '_>,
        ctm: [f32; 6],
        display_list: &mut Vec<DisplayCmd>,
        chars: &mut Vec<CharBox>,
    ) {
        for child in parent.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "Canvas" => {
                    let local = parse_transform(child.attribute("RenderTransform")).unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                    let next = mul_affine(ctm, local);
                    self.render_children(child, next, display_list, chars);
                }
                "Path" => self.render_path(child, ctm, display_list),
                "Glyphs" => render_glyphs(child, ctm, display_list, chars),
                _ => {}
            }
        }
    }

    fn render_path(
        &self,
        node: Node<'_, '_>,
        parent_ctm: [f32; 6],
        display_list: &mut Vec<DisplayCmd>,
    ) {
        let local = parse_transform(node.attribute("RenderTransform"))
            .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let ctm = mul_affine(parent_ctm, local);

        let data_str = node.attribute("Data").or_else(|| {
            node.children()
                .find(|n| is_tag(*n, "Path.Data"))
                .and_then(|pd| pd.children().find(|n| is_tag(*n, "PathGeometry")))
                .and_then(|pg| pg.attribute("Figures"))
        });

        let fill_brush = node
            .children()
            .find(|n| is_tag(*n, "Path.Fill"))
            .and_then(|pf| pf.children().find(|n| n.is_element()));

        if let Some(brush) = fill_brush
            && brush.tag_name().name() == "ImageBrush"
            && let Some(img_src) = brush.attribute("ImageSource")
        {
            if let Some(bounds) = data_str
                .and_then(|d| path_bounds_pt(d, ctm))
                .or_else(|| brush.attribute("Viewport").and_then(|v| viewport_bounds_pt(v, ctm)))
                && let Ok(path) = resolve_path("", img_src)
                && let Ok(raw) = read_entry_bytes(&self.archive, &path)
                && let Ok(img) = image::load_from_memory(&raw)
            {
                let rgba_img = img.to_rgba8();
                let (width, height) = rgba_img.dimensions();
                display_list.push(DisplayCmd::DrawImage {
                    bounds,
                    width,
                    height,
                    rgba: rgba_img.into_raw(),
                });
            }
            return;
        }

        let Some(data) = data_str else {
            return;
        };
        let (fill_rule, commands) = parse_xps_path_data(data, ctm);
        if commands.is_empty() {
            return;
        }
        let fill_rgba = node
            .attribute("Fill")
            .and_then(parse_xps_color)
            .or_else(|| {
                fill_brush
                    .filter(|b| b.tag_name().name() == "SolidColorBrush")
                    .and_then(|b| b.attribute("Color"))
                    .and_then(parse_xps_color)
            });
        let stroke_rgba = node.attribute("Stroke").and_then(parse_xps_color);
        if fill_rgba.is_none() && stroke_rgba.is_none() {
            return;
        }
        let stroke_width = node
            .attribute("StrokeThickness")
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(1.0)
            * PT_PER_XPS_PX;

        let bounds = commands_bounds(&commands);
        display_list.push(DisplayCmd::DrawPath {
            bounds,
            commands,
            fill_rule,
            fill_rgba,
            stroke_rgba,
            stroke_width,
        });
    }
}

fn find_fdseq_path(archive: &Archive) -> Result<String> {
    if let Some(e) = archive.entries().iter().find(|e| {
        e.name.eq_ignore_ascii_case("FixedDocSeq.fdseq")
            || e.name.eq_ignore_ascii_case("FixedDocumentSequence.fdseq")
            || e.name.to_ascii_lowercase().ends_with(".fdseq")
    }) {
        return Ok(e.name.clone());
    }
    if let Ok(rels_xml) = read_entry_utf8(archive, "_rels/.rels")
        && let Ok(doc) = XmlDoc::parse(&rels_xml)
    {
        for rel in doc.descendants().filter(|n| is_tag(*n, "Relationship")) {
            if let (Some(ty), Some(target)) = (rel.attribute("Type"), rel.attribute("Target"))
                && ty.to_ascii_lowercase().contains("fixedrepresentation")
            {
                return resolve_path("", target);
            }
        }
    }
    Err(Error::new("XPS FixedDocSeq.fdseq not found"))
}

fn is_tag(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

fn parent_dir(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((dir, _)) => dir,
        None => "",
    }
}

fn resolve_path(base_dir: &str, rel: &str) -> Result<String> {
    let trimmed = rel.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return Err(Error::new("empty path in XPS"));
    }
    let combined = if let Some(abs) = trimmed.strip_prefix('/') {
        abs.to_string()
    } else if base_dir.is_empty() {
        trimmed
    } else {
        format!("{base_dir}/{trimmed}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(Error::new(format!("invalid path traversal: {rel}")));
                }
            }
            s => parts.push(s),
        }
    }
    Ok(parts.join("/"))
}

fn read_entry_bytes(archive: &Archive, path: &str) -> Result<Vec<u8>> {
    let entry = archive
        .entries()
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(path))
        .ok_or_else(|| Error::new(format!("missing XPS entry: {path}")))?;
    if entry.size > MAX_ENTRY_BYTES {
        return Err(Error::new(format!("XPS entry exceeds size limit: {path}")));
    }
    archive.read(entry).map_err(|e| Error::new(e.to_string()))
}

fn read_entry_utf8(archive: &Archive, path: &str) -> Result<String> {
    let bytes = read_entry_bytes(archive, path)?;
    String::from_utf8(bytes).map_err(|e| Error::new(format!("{path} is not valid UTF-8: {e}")))
}

fn parse_transform(raw: Option<&str>) -> Option<[f32; 6]> {
    let nums: Vec<f32> = raw?
        .split([',', ' ', '\t', '\r', '\n'])
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f32>().ok())
        .collect();
    if nums.len() == 6 {
        Some([nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]])
    } else {
        None
    }
}

fn mul_affine(p: [f32; 6], c: [f32; 6]) -> [f32; 6] {
    [
        c[0] * p[0] + c[1] * p[2],
        c[0] * p[1] + c[1] * p[3],
        c[2] * p[0] + c[3] * p[2],
        c[2] * p[1] + c[3] * p[3],
        c[4] * p[0] + c[5] * p[2] + p[4],
        c[4] * p[1] + c[5] * p[3] + p[5],
    ]
}

fn apply_pt(x: f32, y: f32, m: [f32; 6]) -> (f32, f32) {
    let tx = m[0] * x + m[2] * y + m[4];
    let ty = m[1] * x + m[3] * y + m[5];
    (tx * PT_PER_XPS_PX, ty * PT_PER_XPS_PX)
}

fn parse_xps_color(raw: &str) -> Option<[u8; 4]> {
    let s = raw.trim();
    if let Some(hex) = s.strip_prefix('#') {
        return match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some([r, g, b, 255])
            }
            8 => {
                let a = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let r = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let g = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let b = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some([r, g, b, a])
            }
            _ => None,
        };
    }
    if let Some(sc) = s.strip_prefix("sc#") {
        let vals: Vec<f32> = sc
            .split([',', ' '])
            .filter(|t| !t.is_empty())
            .filter_map(|t| t.parse::<f32>().ok())
            .collect();
        return match vals.as_slice() {
            [r, g, b] => Some([
                (r.clamp(0.0, 1.0) * 255.0) as u8,
                (g.clamp(0.0, 1.0) * 255.0) as u8,
                (b.clamp(0.0, 1.0) * 255.0) as u8,
                255,
            ]),
            [a, r, g, b] => Some([
                (r.clamp(0.0, 1.0) * 255.0) as u8,
                (g.clamp(0.0, 1.0) * 255.0) as u8,
                (b.clamp(0.0, 1.0) * 255.0) as u8,
                (a.clamp(0.0, 1.0) * 255.0) as u8,
            ]),
            _ => None,
        };
    }
    None
}

fn tokenize_path(data: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    for ch in data.chars() {
        if ch.is_ascii_alphabetic() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            tokens.push(ch.to_string());
        } else if ch == ',' || ch.is_whitespace() {
            if !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn parse_xps_path_data(data: &str, ctm: [f32; 6]) -> (FillRule, Vec<PathCmd>) {
    let tokens = tokenize_path(data);
    let mut cmds = Vec::new();
    let mut fill_rule = FillRule::EvenOdd;
    let mut i = 0;
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut sx = 0.0f32;
    let mut sy = 0.0f32;
    let mut last_op = "";

    let next_f = |idx: &mut usize| -> Option<f32> {
        let v = tokens.get(*idx)?.parse::<f32>().ok()?;
        *idx += 1;
        Some(v)
    };

    while i < tokens.len() {
        let tok = tokens[i].as_str();
        let op = if tok.len() == 1 && tok.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
            i += 1;
            if tok == "F" {
                if let Some(rule) = tokens.get(i).map(String::as_str) {
                    fill_rule = if rule == "1" {
                        FillRule::NonZero
                    } else {
                        FillRule::EvenOdd
                    };
                    i += 1;
                }
                continue;
            }
            last_op = match tok {
                "M" => "L",
                "m" => "l",
                other => other,
            };
            tok
        } else {
            last_op
        };

        match op {
            "M" | "m" => {
                let Some((x, y)) = next_f(&mut i).zip(next_f(&mut i)) else {
                    break;
                };
                if op == "m" {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                sx = cx;
                sy = cy;
                let (px, py) = apply_pt(cx, cy, ctm);
                cmds.push(PathCmd::MoveTo(px, py));
            }
            "L" | "l" => {
                let Some((x, y)) = next_f(&mut i).zip(next_f(&mut i)) else {
                    break;
                };
                if op == "l" {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                let (px, py) = apply_pt(cx, cy, ctm);
                cmds.push(PathCmd::LineTo(px, py));
            }
            "H" | "h" => {
                let Some(x) = next_f(&mut i) else { break };
                cx = if op == "h" { cx + x } else { x };
                let (px, py) = apply_pt(cx, cy, ctm);
                cmds.push(PathCmd::LineTo(px, py));
            }
            "V" | "v" => {
                let Some(y) = next_f(&mut i) else { break };
                cy = if op == "v" { cy + y } else { y };
                let (px, py) = apply_pt(cx, cy, ctm);
                cmds.push(PathCmd::LineTo(px, py));
            }
            "C" | "c" => {
                let Some((x1, y1, x2, y2, x, y)) = (|| {
                    Some((
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                    ))
                })() else {
                    break;
                };
                let (ax1, ay1, ax2, ay2, ax, ay) = if op == "c" {
                    (cx + x1, cy + y1, cx + x2, cy + y2, cx + x, cy + y)
                } else {
                    (x1, y1, x2, y2, x, y)
                };
                cx = ax;
                cy = ay;
                let (px1, py1) = apply_pt(ax1, ay1, ctm);
                let (px2, py2) = apply_pt(ax2, ay2, ctm);
                let (px, py) = apply_pt(ax, ay, ctm);
                cmds.push(PathCmd::CubicTo(px1, py1, px2, py2, px, py));
            }
            "Q" | "q" => {
                let Some((qx, qy, x, y)) = (|| {
                    Some((
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                    ))
                })() else {
                    break;
                };
                let (aqx, aqy, ax, ay) = if op == "q" {
                    (cx + qx, cy + qy, cx + x, cy + y)
                } else {
                    (qx, qy, x, y)
                };
                cx = ax;
                cy = ay;
                let (pqx, pqy) = apply_pt(aqx, aqy, ctm);
                let (px, py) = apply_pt(ax, ay, ctm);
                cmds.push(PathCmd::QuadTo(pqx, pqy, px, py));
            }
            "A" | "a" => {
                let Some((_rx, _ry, _rot, _large, _sweep, x, y)) = (|| {
                    Some((
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                        next_f(&mut i)?,
                    ))
                })() else {
                    break;
                };
                if op == "a" {
                    cx += x;
                    cy += y;
                } else {
                    cx = x;
                    cy = y;
                }
                let (px, py) = apply_pt(cx, cy, ctm);
                cmds.push(PathCmd::LineTo(px, py));
            }
            "Z" | "z" => {
                cx = sx;
                cy = sy;
                cmds.push(PathCmd::Close);
            }
            _ => break,
        }
    }
    (fill_rule, cmds)
}

fn commands_bounds(cmds: &[PathCmd]) -> PdfRect {
    let mut x0 = f32::INFINITY;
    let mut y0 = f32::INFINITY;
    let mut x1 = f32::NEG_INFINITY;
    let mut y1 = f32::NEG_INFINITY;
    let mut push = |x: f32, y: f32| {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    };
    for &cmd in cmds {
        match cmd {
            PathCmd::MoveTo(x, y) | PathCmd::LineTo(x, y) => push(x, y),
            PathCmd::QuadTo(cx, cy, x, y) => {
                push(cx, cy);
                push(x, y);
            }
            PathCmd::CubicTo(cx1, cy1, cx2, cy2, x, y) => {
                push(cx1, cy1);
                push(cx2, cy2);
                push(x, y);
            }
            PathCmd::Close => {}
        }
    }
    if x0.is_finite() {
        PdfRect { x0, y0, x1, y1 }
    } else {
        PdfRect {
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        }
    }
}

fn path_bounds_pt(data: &str, ctm: [f32; 6]) -> Option<PdfRect> {
    let (_, cmds) = parse_xps_path_data(data, ctm);
    if cmds.is_empty() {
        return None;
    }
    let b = commands_bounds(&cmds);
    (b.width() > 0.0 && b.height() > 0.0).then_some(b)
}

fn viewport_bounds_pt(vp: &str, ctm: [f32; 6]) -> Option<PdfRect> {
    let nums: Vec<f32> = vp
        .split([',', ' '])
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f32>().ok())
        .collect();
    if nums.len() != 4 {
        return None;
    }
    let (x0, y0) = apply_pt(nums[0], nums[1], ctm);
    let (x1, y1) = apply_pt(nums[0] + nums[2], nums[1] + nums[3], ctm);
    Some(PdfRect { x0, y0, x1, y1 })
}

fn render_glyphs(
    node: Node<'_, '_>,
    parent_ctm: [f32; 6],
    display_list: &mut Vec<DisplayCmd>,
    chars: &mut Vec<CharBox>,
) {
    let Some(text) = node.attribute("UnicodeString").filter(|s| !s.is_empty()) else {
        return;
    };
    let local = parse_transform(node.attribute("RenderTransform"))
        .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let ctm = mul_affine(parent_ctm, local);
    let ox_px = node
        .attribute("OriginX")
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.0);
    let oy_px = node
        .attribute("OriginY")
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.0);
    let em_px = node
        .attribute("FontRenderingEmSize")
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(12.0);
    let font_size = em_px * PT_PER_XPS_PX;
    let rgba = node
        .attribute("Fill")
        .and_then(parse_xps_color)
        .unwrap_or([0, 0, 0, 255]);
    let (ox, oy) = apply_pt(ox_px, oy_px, ctm);

    display_list.push(DisplayCmd::DrawGlyphRun {
        text: text.to_string(),
        origin: (ox, oy),
        font_size,
        rgba,
    });

    // Each character has its own box along the run (a run is one drawing command, so its characters' places are estimated: `estimated_advance_em`).
    let mut x = ox;
    for ch in text.chars() {
        let advance = font_size * ibis_pdf::scene::estimated_advance_em(ch);
        chars.push(CharBox {
            ch,
            rect_pt: PdfRect {
                x0: x,
                y0: oy - font_size,
                x1: x + advance,
                y1: oy,
            },
            vertical: false,
            stream_op_index: display_list.len(),
        });
        x += advance;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synthetic_xps_package_and_renders_scene() {
        let mut w = ibis_ooxml::zip::Writer::new();
        let meta = ibis_ooxml::zip::EntryMeta::default();
        w.add(
            "FixedDocSeq.fdseq",
            meta,
            br#"<?xml version="1.0" encoding="utf-8"?>
<FixedDocumentSequence xmlns="http://schemas.microsoft.com/xps/2005/06">
  <DocumentReference Source="Documents/1/FixedDoc.fdoc"/>
</FixedDocumentSequence>"#,
        )
        .unwrap();
        w.add(
            "Documents/1/FixedDoc.fdoc",
            meta,
            br#"<?xml version="1.0" encoding="utf-8"?>
<FixedDocument xmlns="http://schemas.microsoft.com/xps/2005/06">
  <PageContent Source="Pages/1.fpage"/>
</FixedDocument>"#,
        )
        .unwrap();
        w.add(
            "Documents/1/Pages/1.fpage",
            meta,
            r##"<?xml version="1.0" encoding="utf-8"?>
<FixedPage Width="793.7" Height="1122.5" xmlns="http://schemas.microsoft.com/xps/2005/06" xml:lang="ja-JP">
  <Path Data="M 40,40 L 200,40 L 200,100 L 40,100 Z" Fill="#FFEFEFEF" Stroke="#FF000000" StrokeThickness="1"/>
  <Glyphs OriginX="50" OriginY="80" FontRenderingEmSize="16" Fill="#FF112233" UnicodeString="XPSテスト"/>
</FixedPage>"##.as_bytes(),
        )
        .unwrap();
        let bytes: Arc<[u8]> = w.finish().unwrap().into();
        assert!(is_xps(&bytes));
        let xps = XpsContainer::from_bytes(bytes).expect("parse synthetic XPS");
        assert_eq!(xps.page_count(), 1);
        let scene = xps.interpret_page_scene(0).expect("render synthetic XPS page");
        assert_eq!(scene.display_list.len(), 2);
        assert_eq!(scene.text_index.plain_text(), "XPSテスト");
        // Each character of a run has a box of its own, going right along the run.
        let xs: Vec<f32> = scene.text_index.chars.iter().map(|c| c.rect_pt.x0).collect();
        assert!(xs.windows(2).all(|w| w[1] > w[0]), "{xs:?}");
    }
}
