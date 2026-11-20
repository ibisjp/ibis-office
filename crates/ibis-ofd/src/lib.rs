// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Native OFD (GB/T 33190-2016) container parser and [`ibis_pdf::scene::PageScene`] builder.

use std::collections::HashMap;
use std::sync::Arc;

use ibis_ooxml::zip::Archive;
use ibis_pdf::cos::ObjectId;
use ibis_pdf::scene::{
    CharBox, DisplayCmd, FillRule, PageInfo, PageScene, PageTextIndex, PathCmd, PdfRect,
};
use roxmltree::{Document as XmlDoc, Node};

pub mod signature;
pub use signature::{
    OfdSignatureCryptoResult, OfdSignatureVerification, SignatureOutcome, summarize_signatures,
    verify_signatures,
};

const PT_PER_MM: f32 = 72.0 / 25.4;
const MAX_ENTRY_BYTES: usize = 32 * 1024 * 1024;
const MAX_DELTA_REPEAT: usize = 10_000;

/// Errors returned when reading or interpreting an OFD package.
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
struct OfdPageRef {
    path: String,
    box_mm: [f32; 4],
}

#[derive(Clone, Debug)]
struct OfdTemplate {
    path: String,
}

#[derive(Clone, Debug)]
struct OfdMedia {
    path: String,
}

/// Parsed OFD package backed by an in-memory ZIP archive.
pub struct OfdContainer {
    archive: Archive,
    pages: Vec<OfdPageRef>,
    templates: HashMap<String, OfdTemplate>,
    medias: HashMap<String, OfdMedia>,
    signatures: Vec<OfdSignatureVerification>,
}

/// Returns `true` if `bytes` looks like an OFD ZIP package (contains `OFD.xml`).
pub fn is_ofd(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"PK\x03\x04") {
        return false;
    }
    let Ok(archive) = Archive::new(Arc::from(bytes)) else {
        return false;
    };
    archive.entries().iter().any(|e| e.name.eq_ignore_ascii_case("OFD.xml"))
}

impl OfdContainer {
    /// Parse an OFD ZIP package from raw bytes.
    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let archive = Archive::new(bytes.into()).map_err(|e| Error::new(e.to_string()))?;
        let ofd_xml = read_entry_utf8(&archive, "OFD.xml")?;
        let ofd_doc = XmlDoc::parse(&ofd_xml).map_err(|e| Error::new(format!("OFD.xml: {e}")))?;

        let doc_body = ofd_doc
            .descendants()
            .find(|n| is_tag(*n, "DocBody"))
            .ok_or_else(|| Error::new("OFD.xml has no DocBody"))?;

        let doc_root_rel = child_text(doc_body, "DocRoot")
            .ok_or_else(|| Error::new("OFD.xml has no DocRoot"))?;
        let doc_root_path = resolve_path("", doc_root_rel)?;

        let signatures_rel = child_text(doc_body, "Signatures");
        let signatures = match signatures_rel {
            Some(rel) => {
                let sig_path = resolve_path("", rel)?;
                verify_signatures(&archive, &sig_path)
            }
            None => Vec::new(),
        };

        let doc_xml = read_entry_utf8(&archive, &doc_root_path)?;
        let doc = XmlDoc::parse(&doc_xml).map_err(|e| Error::new(format!("{doc_root_path}: {e}")))?;

        let doc_dir = parent_dir(&doc_root_path);
        let common_data = doc.descendants().find(|n| is_tag(*n, "CommonData"));
        let default_box_mm = common_data
            .and_then(|cd| parse_page_area(cd))
            .unwrap_or([0.0, 0.0, 210.0, 297.0]);

        let mut medias = HashMap::new();
        let mut templates = HashMap::new();

        if let Some(cd) = common_data {
            for res_tag in ["PublicRes", "DocumentRes"] {
                for node in cd.children().filter(|n| is_tag(*n, res_tag)) {
                    if let Some(rel) = node.text().map(str::trim).filter(|s| !s.is_empty())
                        && let Ok(res_path) = resolve_path(doc_dir, rel)
                    {
                        let _ = load_resources(&archive, &res_path, &mut medias);
                    }
                }
            }
            for tmpl in cd.children().filter(|n| is_tag(*n, "TemplatePage")) {
                if let (Some(id), Some(base_loc)) = (tmpl.attribute("ID"), tmpl.attribute("BaseLoc"))
                    && let Ok(path) = resolve_path(doc_dir, base_loc)
                {
                    templates.insert(id.to_string(), OfdTemplate { path });
                }
            }
        }

        let mut pages = Vec::new();
        if let Some(pages_node) = doc.descendants().find(|n| is_tag(*n, "Pages")) {
            for page_node in pages_node.children().filter(|n| is_tag(*n, "Page")) {
                let Some(base_loc) = page_node.attribute("BaseLoc") else {
                    continue;
                };
                let path = resolve_path(doc_dir, base_loc)?;
                pages.push(OfdPageRef {
                    path,
                    box_mm: default_box_mm,
                });
            }
        }

        // Resolve per-page Area overrides lazily or upfront; upfront keeps PageInfo accurate.
        for page in &mut pages {
            if let Ok(page_xml) = read_entry_utf8(&archive, &page.path)
                && let Ok(page_doc) = XmlDoc::parse(&page_xml)
                && let Some(area) = parse_page_area(page_doc.root_element())
            {
                page.box_mm = area;
            }
        }

        Ok(Self {
            archive,
            pages,
            templates,
            medias,
            signatures,
        })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn signature_count(&self) -> usize {
        self.signatures.len()
    }

    pub fn signature_verifications(&self) -> &[OfdSignatureVerification] {
        &self.signatures
    }

    /// How many signatures the package has and what they were found to be (`None` if it has none).
    pub fn signature_summary(&self) -> Option<(usize, SignatureOutcome)> {
        summarize_signatures(&self.signatures)
    }

    /// Return [`PageInfo`] metadata for all pages (boxes converted from mm to PDF points).
    pub fn page_infos(&self) -> Vec<PageInfo> {
        self.pages
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                let rect = mm_box_to_pt_rect(p.box_mm);
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

    /// Interpret a single OFD page (including background/foreground templates) into a [`PageScene`].
    pub fn interpret_page_scene(&self, page_index: usize) -> Result<PageScene> {
        let page = self
            .pages
            .get(page_index)
            .ok_or_else(|| Error::new(format!("page index {page_index} out of range")))?;

        let page_xml = read_entry_utf8(&self.archive, &page.path)?;
        let page_doc = XmlDoc::parse(&page_xml).map_err(|e| Error::new(format!("{}: {e}", page.path)))?;
        let root = page_doc.root_element();

        let box_mm = parse_page_area(root).unwrap_or(page.box_mm);
        let rect = mm_box_to_pt_rect(box_mm);
        let info = PageInfo {
            index: page_index,
            object_id: ObjectId::new(page_index as u32 + 1, 0),
            media_box: rect,
            crop_box: rect,
            rotate: 0,
        };

        let mut bg_templates = Vec::new();
        let mut fg_templates = Vec::new();
        for tmpl_ref in root.children().filter(|n| is_tag(*n, "Template")) {
            let Some(id) = tmpl_ref.attribute("TemplateID") else {
                continue;
            };
            let z = tmpl_ref.attribute("ZOrder").unwrap_or("Background");
            if z.eq_ignore_ascii_case("Foreground") {
                fg_templates.push(id);
            } else {
                bg_templates.push(id);
            }
        }

        let mut display_list = Vec::new();
        let mut chars = Vec::new();

        for id in bg_templates {
            if let Some(tmpl) = self.templates.get(id) {
                let _ = self.render_page_xml(&tmpl.path, &mut display_list, &mut chars);
            }
        }
        self.render_content_node(root, &mut display_list, &mut chars);
        for id in fg_templates {
            if let Some(tmpl) = self.templates.get(id) {
                let _ = self.render_page_xml(&tmpl.path, &mut display_list, &mut chars);
            }
        }

        Ok(PageScene {
            info,
            display_list,
            text_index: PageTextIndex { chars },
        })
    }

    fn render_page_xml(
        &self,
        path: &str,
        display_list: &mut Vec<DisplayCmd>,
        chars: &mut Vec<CharBox>,
    ) -> Result<()> {
        let xml = read_entry_utf8(&self.archive, path)?;
        let doc = XmlDoc::parse(&xml).map_err(|e| Error::new(format!("{path}: {e}")))?;
        self.render_content_node(doc.root_element(), display_list, chars);
        Ok(())
    }

    fn render_content_node(
        &self,
        page_root: Node<'_, '_>,
        display_list: &mut Vec<DisplayCmd>,
        chars: &mut Vec<CharBox>,
    ) {
        let Some(content) = page_root.children().find(|n| is_tag(*n, "Content")) else {
            return;
        };
        for layer in content.children().filter(|n| is_tag(*n, "Layer")) {
            self.render_container(layer, display_list, chars);
        }
    }

    fn render_container(
        &self,
        container: Node<'_, '_>,
        display_list: &mut Vec<DisplayCmd>,
        chars: &mut Vec<CharBox>,
    ) {
        for child in container.children().filter(|n| n.is_element()) {
            match child.tag_name().name() {
                "PageBlock" => self.render_container(child, display_list, chars),
                "PathObject" => render_path_object(child, display_list),
                "TextObject" => render_text_object(child, display_list, chars),
                "ImageObject" => self.render_image_object(child, display_list),
                _ => {}
            }
        }
    }

    fn render_image_object(&self, node: Node<'_, '_>, display_list: &mut Vec<DisplayCmd>) {
        let Some(res_id) = node.attribute("ResourceID") else {
            return;
        };
        let Some(media) = self.medias.get(res_id) else {
            return;
        };
        let Some(boundary) = node.attribute("Boundary").and_then(parse_4_f32) else {
            return;
        };
        let Ok(raw) = read_entry_bytes(&self.archive, &media.path) else {
            return;
        };
        let Ok(img) = image::load_from_memory(&raw) else {
            return;
        };
        let rgba_img = img.to_rgba8();
        let (width, height) = rgba_img.dimensions();
        let bounds = PdfRect {
            x0: boundary[0] * PT_PER_MM,
            y0: boundary[1] * PT_PER_MM,
            x1: (boundary[0] + boundary[2]) * PT_PER_MM,
            y1: (boundary[1] + boundary[3]) * PT_PER_MM,
        };
        display_list.push(DisplayCmd::DrawImage {
            bounds,
            width,
            height,
            rgba: rgba_img.into_raw(),
        });
    }
}

fn is_tag(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

fn child_text<'a>(node: Node<'a, 'a>, tag: &str) -> Option<&'a str> {
    node.children()
        .find(|n| is_tag(*n, tag))
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn parent_dir(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((dir, _)) => dir,
        None => "",
    }
}

fn resolve_path(base_dir: &str, rel: &str) -> Result<String> {
    let trimmed = rel.trim();
    if trimmed.is_empty() {
        return Err(Error::new("empty path in OFD"));
    }
    let combined = if let Some(abs) = trimmed.strip_prefix('/') {
        abs.to_string()
    } else if base_dir.is_empty() {
        trimmed.to_string()
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
        .ok_or_else(|| Error::new(format!("missing OFD entry: {path}")))?;
    if entry.size > MAX_ENTRY_BYTES {
        return Err(Error::new(format!("OFD entry exceeds size limit: {path}")));
    }
    archive.read(entry).map_err(|e| Error::new(e.to_string()))
}

fn read_entry_utf8(archive: &Archive, path: &str) -> Result<String> {
    let bytes = read_entry_bytes(archive, path)?;
    String::from_utf8(bytes).map_err(|e| Error::new(format!("{path} is not valid UTF-8: {e}")))
}

fn load_resources(
    archive: &Archive,
    res_path: &str,
    medias: &mut HashMap<String, OfdMedia>,
) -> Result<()> {
    let xml = read_entry_utf8(archive, res_path)?;
    let doc = XmlDoc::parse(&xml).map_err(|e| Error::new(format!("{res_path}: {e}")))?;
    let root = doc.root_element();
    let base_loc = root.attribute("BaseLoc").unwrap_or("");
    let res_dir = parent_dir(res_path);
    let media_base = if base_loc.is_empty() {
        res_dir.to_string()
    } else {
        resolve_path(res_dir, base_loc)?
    };
    for mm in doc.descendants().filter(|n| is_tag(*n, "MultiMedia")) {
        let Some(id) = mm.attribute("ID") else {
            continue;
        };
        let Some(file) = child_text(mm, "MediaFile") else {
            continue;
        };
        if let Ok(path) = resolve_path(&media_base, file) {
            medias.insert(id.to_string(), OfdMedia { path });
        }
    }
    Ok(())
}

fn parse_4_f32(s: &str) -> Option<[f32; 4]> {
    let mut nums = s.split_whitespace().filter_map(|t| t.parse::<f32>().ok());
    Some([nums.next()?, nums.next()?, nums.next()?, nums.next()?])
}

fn parse_6_f32(s: &str) -> Option<[f32; 6]> {
    let mut nums = s.split_whitespace().filter_map(|t| t.parse::<f32>().ok());
    Some([
        nums.next()?,
        nums.next()?,
        nums.next()?,
        nums.next()?,
        nums.next()?,
        nums.next()?,
    ])
}

fn parse_page_area(node: Node<'_, '_>) -> Option<[f32; 4]> {
    let area = node
        .children()
        .find(|n| is_tag(*n, "PageArea") || is_tag(*n, "Area"))?;
    for tag in ["CropBox", "PhysicalBox", "ApplicationBox"] {
        if let Some(txt) = child_text(area, tag)
            && let Some(b) = parse_4_f32(txt)
        {
            return Some(b);
        }
    }
    None
}

fn mm_box_to_pt_rect(b: [f32; 4]) -> PdfRect {
    PdfRect {
        x0: b[0] * PT_PER_MM,
        y0: b[1] * PT_PER_MM,
        x1: (b[0] + b[2]) * PT_PER_MM,
        y1: (b[1] + b[3]) * PT_PER_MM,
    }
}

fn parse_color(node: Node<'_, '_>, tag: &str) -> Option<[u8; 4]> {
    let color_node = node.children().find(|n| is_tag(*n, tag))?;
    let val = color_node.attribute("Value")?;
    let parts: Vec<f32> = val
        .split_whitespace()
        .filter_map(|t| t.parse::<f32>().ok())
        .collect();
    let alpha = color_node
        .attribute("Alpha")
        .and_then(|a| a.parse::<u8>().ok())
        .unwrap_or(255);
    match parts.as_slice() {
        [r, g, b] => Some([
            r.clamp(0.0, 255.0) as u8,
            g.clamp(0.0, 255.0) as u8,
            b.clamp(0.0, 255.0) as u8,
            alpha,
        ]),
        [gray] => {
            let g = gray.clamp(0.0, 255.0) as u8;
            Some([g, g, g, alpha])
        }
        [c, m, y, k] => {
            let r = (255.0 * (1.0 - c / 255.0) * (1.0 - k / 255.0)).clamp(0.0, 255.0) as u8;
            let g = (255.0 * (1.0 - m / 255.0) * (1.0 - k / 255.0)).clamp(0.0, 255.0) as u8;
            let b = (255.0 * (1.0 - y / 255.0) * (1.0 - k / 255.0)).clamp(0.0, 255.0) as u8;
            Some([r, g, b, alpha])
        }
        _ => None,
    }
}

fn transform_pt(x: f32, y: f32, bx: f32, by: f32, ctm: Option<[f32; 6]>) -> (f32, f32) {
    let (tx, ty) = match ctm {
        Some([a, b, c, d, e, f]) => (a * x + c * y + e, b * x + d * y + f),
        None => (x, y),
    };
    ((bx + tx) * PT_PER_MM, (by + ty) * PT_PER_MM)
}

fn render_path_object(node: Node<'_, '_>, display_list: &mut Vec<DisplayCmd>) {
    let Some(boundary) = node.attribute("Boundary").and_then(parse_4_f32) else {
        return;
    };
    let Some(data) = child_text(node, "AbbreviatedData") else {
        return;
    };
    let ctm = node.attribute("CTM").and_then(parse_6_f32);
    let do_stroke = node.attribute("Stroke").map(|s| s == "true").unwrap_or(true);
    let do_fill = node.attribute("Fill").map(|s| s == "true").unwrap_or(false);
    let stroke_rgba = if do_stroke {
        Some(parse_color(node, "StrokeColor").unwrap_or([0, 0, 0, 255]))
    } else {
        None
    };
    let fill_rgba = if do_fill {
        Some(parse_color(node, "FillColor").unwrap_or([0, 0, 0, 255]))
    } else {
        None
    };
    if stroke_rgba.is_none() && fill_rgba.is_none() {
        return;
    }
    let stroke_width = node
        .attribute("LineWidth")
        .and_then(|w| w.parse::<f32>().ok())
        .unwrap_or(0.353)
        * PT_PER_MM;
    let fill_rule = match node.attribute("Rule") {
        Some("Even-Odd") => FillRule::EvenOdd,
        _ => FillRule::NonZero,
    };

    let commands = parse_abbreviated_data(data, boundary[0], boundary[1], ctm);
    if commands.is_empty() {
        return;
    }
    let bounds = mm_box_to_pt_rect(boundary);
    display_list.push(DisplayCmd::DrawPath {
        bounds,
        commands,
        fill_rule,
        fill_rgba,
        stroke_rgba,
        stroke_width,
    });
}

fn parse_abbreviated_data(
    data: &str,
    bx: f32,
    by: f32,
    ctm: Option<[f32; 6]>,
) -> Vec<PathCmd> {
    let tokens: Vec<&str> = data.split_whitespace().collect();
    let mut cmds = Vec::new();
    let mut i = 0;
    let next_f = |idx: &mut usize| -> Option<f32> {
        let v = tokens.get(*idx)?.parse::<f32>().ok()?;
        *idx += 1;
        Some(v)
    };
    while i < tokens.len() {
        let op = tokens[i];
        i += 1;
        match op {
            "S" | "M" => {
                if let (Some(x), Some(y)) = (next_f(&mut i), next_f(&mut i)) {
                    let (px, py) = transform_pt(x, y, bx, by, ctm);
                    cmds.push(PathCmd::MoveTo(px, py));
                }
            }
            "L" => {
                if let (Some(x), Some(y)) = (next_f(&mut i), next_f(&mut i)) {
                    let (px, py) = transform_pt(x, y, bx, by, ctm);
                    cmds.push(PathCmd::LineTo(px, py));
                }
            }
            "Q" => {
                if let (Some(cx), Some(cy), Some(x), Some(y)) =
                    (next_f(&mut i), next_f(&mut i), next_f(&mut i), next_f(&mut i))
                {
                    let (pcx, pcy) = transform_pt(cx, cy, bx, by, ctm);
                    let (px, py) = transform_pt(x, y, bx, by, ctm);
                    cmds.push(PathCmd::QuadTo(pcx, pcy, px, py));
                }
            }
            "B" => {
                if let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                    next_f(&mut i),
                    next_f(&mut i),
                    next_f(&mut i),
                    next_f(&mut i),
                    next_f(&mut i),
                    next_f(&mut i),
                ) {
                    let (px1, py1) = transform_pt(x1, y1, bx, by, ctm);
                    let (px2, py2) = transform_pt(x2, y2, bx, by, ctm);
                    let (px, py) = transform_pt(x, y, bx, by, ctm);
                    cmds.push(PathCmd::CubicTo(px1, py1, px2, py2, px, py));
                }
            }
            "A" => {
                // rx ry angle large sweep x y -> approximate endpoint line
                if let (Some(_rx), Some(_ry), Some(_ang), Some(_l), Some(_s), Some(x), Some(y)) = (
                    next_f(&mut i),
                    next_f(&mut i),
                    next_f(&mut i),
                    next_f(&mut i),
                    next_f(&mut i),
                    next_f(&mut i),
                    next_f(&mut i),
                ) {
                    let (px, py) = transform_pt(x, y, bx, by, ctm);
                    cmds.push(PathCmd::LineTo(px, py));
                }
            }
            "C" => cmds.push(PathCmd::Close),
            _ => {}
        }
    }
    cmds
}

fn parse_deltas(raw: Option<&str>) -> Vec<f32> {
    let Some(raw) = raw else {
        return Vec::new();
    };
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i] == "g" {
            if let (Some(cnt), Some(val)) = (
                tokens.get(i + 1).and_then(|s| s.parse::<usize>().ok()),
                tokens.get(i + 2).and_then(|s| s.parse::<f32>().ok()),
            ) {
                for _ in 0..cnt.min(MAX_DELTA_REPEAT) {
                    out.push(val);
                }
                i += 3;
                continue;
            }
        } else if let Ok(val) = tokens[i].parse::<f32>() {
            out.push(val);
        }
        i += 1;
    }
    out
}

fn render_text_object(
    node: Node<'_, '_>,
    display_list: &mut Vec<DisplayCmd>,
    chars: &mut Vec<CharBox>,
) {
    let Some(boundary) = node.attribute("Boundary").and_then(parse_4_f32) else {
        return;
    };
    let size_mm = node
        .attribute("Size")
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(3.5);
    let font_size = size_mm * PT_PER_MM;
    let ctm = node.attribute("CTM").and_then(parse_6_f32);
    let rgba = parse_color(node, "FillColor")
        .or_else(|| parse_color(node, "StrokeColor"))
        .unwrap_or([0, 0, 0, 255]);

    for tc in node.children().filter(|n| is_tag(*n, "TextCode")) {
        let Some(text) = tc.text().filter(|s| !s.is_empty()) else {
            continue;
        };
        let mut x_mm = tc.attribute("X").and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
        let mut y_mm = tc.attribute("Y").and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
        let dx_list = parse_deltas(tc.attribute("DeltaX"));
        let dy_list = parse_deltas(tc.attribute("DeltaY"));

        if dx_list.is_empty() && dy_list.is_empty() {
            let (ox, oy) = transform_pt(x_mm, y_mm, boundary[0], boundary[1], ctm);
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
        } else {
            for (idx, ch) in text.chars().enumerate() {
                let (ox, oy) = transform_pt(x_mm, y_mm, boundary[0], boundary[1], ctm);
                if !ch.is_whitespace() {
                    display_list.push(DisplayCmd::DrawGlyphRun {
                        text: ch.to_string(),
                        origin: (ox, oy),
                        font_size,
                        rgba,
                    });
                }
                chars.push(CharBox {
                    ch,
                    rect_pt: PdfRect {
                        x0: ox,
                        y0: oy - font_size,
                        x1: ox + font_size * 0.6,
                        y1: oy,
                    },
                    vertical: false,
                    stream_op_index: display_list.len(),
                });
                x_mm += dx_list.get(idx).copied().unwrap_or(size_mm * 0.55);
                y_mm += dy_list.get(idx).copied().unwrap_or(0.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_delta_repeat_syntax() {
        assert_eq!(parse_deltas(Some("g 3 2.5 1.0")), vec![2.5, 2.5, 2.5, 1.0]);
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(resolve_path("Doc_0", "../../etc/passwd").is_err());
        assert_eq!(resolve_path("Doc_0/Pages", "../Res/img.png").unwrap(), "Doc_0/Res/img.png");
    }

    #[test]
    fn parses_synthetic_ofd_package_and_renders_scene() {
        let mut w = ibis_ooxml::zip::Writer::new();
        let meta = ibis_ooxml::zip::EntryMeta::default();
        w.add(
            "OFD.xml",
            meta,
            br#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016" Version="1.1">
  <ofd:DocBody>
    <ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot>
    <ofd:Signatures>Doc_0/Signs/Signatures.xml</ofd:Signatures>
  </ofd:DocBody>
</ofd:OFD>"#,
        ).unwrap();
        w.add(
            "Doc_0/Document.xml",
            meta,
            br#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Document xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:CommonData>
    <ofd:PageArea><ofd:PhysicalBox>0 0 210 297</ofd:PhysicalBox></ofd:PageArea>
  </ofd:CommonData>
  <ofd:Pages>
    <ofd:Page ID="1" BaseLoc="Pages/Page_0/Content.xml"/>
  </ofd:Pages>
</ofd:Document>"#,
        ).unwrap();
        w.add(
            "Doc_0/Signs/Signatures.xml",
            meta,
            br#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Signatures xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Signature ID="s1" BaseLoc="Sign_0/Signature.xml"/>
</ofd:Signatures>"#,
        ).unwrap();
        let page_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:Content>
    <ofd:Layer ID="1">
      <ofd:PathObject ID="2" Boundary="10 10 50 20" Stroke="true" Fill="true">
        <ofd:FillColor Value="240 240 240"/>
        <ofd:StrokeColor Value="0 0 0"/>
        <ofd:AbbreviatedData>M 0 0 L 50 0 L 50 20 L 0 20 C</ofd:AbbreviatedData>
      </ofd:PathObject>
      <ofd:TextObject ID="3" Boundary="15 15 40 10" Font="1" Size="4">
        <ofd:FillColor Value="0 0 0"/>
        <ofd:TextCode X="0" Y="5">OFD测试</ofd:TextCode>
      </ofd:TextObject>
    </ofd:Layer>
  </ofd:Content>
</ofd:Page>"#.as_bytes();
        w.add("Doc_0/Pages/Page_0/Content.xml", meta, page_xml).unwrap();
        use base64::Engine as _;
        let digest_b64 = base64::engine::general_purpose::STANDARD.encode(signature::sm3_digest(page_xml));
        let sig_xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Signature xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:SignedInfo>
    <ofd:SignatureMethod>1.2.156.10197.1.501</ofd:SignatureMethod>
    <ofd:References CheckMethod="1.2.156.10197.1.401">
      <ofd:Reference FileRef="/Doc_0/Pages/Page_0/Content.xml">
        <ofd:CheckValue>{digest_b64}</ofd:CheckValue>
      </ofd:Reference>
    </ofd:References>
  </ofd:SignedInfo>
  <ofd:SignedValue>SignedValue.dat</ofd:SignedValue>
</ofd:Signature>"#
        );
        w.add("Doc_0/Signs/Sign_0/Signature.xml", meta, sig_xml.as_bytes()).unwrap();
        w.add("Doc_0/Signs/Sign_0/SignedValue.dat", meta, &[0u8; 64]).unwrap();
        let bytes: Arc<[u8]> = w.finish().unwrap().into();
        assert!(is_ofd(&bytes));
        let ofd = OfdContainer::from_bytes(bytes).expect("parse synthetic OFD");
        assert_eq!(ofd.page_count(), 1);
        assert_eq!(ofd.signature_count(), 1);
        assert!(ofd.signature_verifications()[0].all_references_ok());
        assert_eq!(ofd.signature_summary(), Some((1, SignatureOutcome::DigestsMatch)));
        let scene = ofd.interpret_page_scene(0).expect("render synthetic OFD page");
        assert_eq!(scene.display_list.len(), 2);
        assert_eq!(scene.text_index.plain_text(), "OFD测试");
    }
}
