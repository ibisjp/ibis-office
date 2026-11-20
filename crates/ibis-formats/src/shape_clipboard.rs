// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Office drawing clipboard packages (Art::GVML ClipFormat). Geometry and text
//! reuse the DrawingML reader/writer; no raster conversion is involved.
use crate::save::pptx_new::{self, Prefixes};
use crate::{Error, Kind, Opened};
use ibis_doc_model::{MediaFormat, Presentation, Shape, ShapeKind};
use ibis_ooxml::zip::{EntryMeta, Writer};
use ibis_ooxml::{Package, SaveOptions, XmlDocument};
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
fn rels(entries: &[(String, String, String, bool)]) -> String {
    let inner: String = entries
        .iter()
        .map(|(id, kind, target, external)| {
            format!(
                "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/>",
                pptx_new::escape_attr(id),
                pptx_new::escape_attr(kind),
                pptx_new::escape_attr(target),
                if *external {
                    " TargetMode=\"External\""
                } else {
                    ""
                }
            )
        })
        .collect();
    format!("<Relationships xmlns=\"{REL}\">{inner}</Relationships>")
}
/// Read an Office vector clipboard package as an ordinary one-slide model.
pub fn read_gvml(data: Vec<u8>) -> Result<Opened, Error> {
    let source = Package::open(data)?;
    let drawing = source
        .relationships("")?
        .into_iter()
        .find(|r| r.kind == format!("{R}/drawing"))
        .and_then(|r| r.target_part(""))
        .or_else(|| {
            source
                .names()
                .find(|n| n.ends_with("/drawing1.xml"))
                .map(str::to_string)
        })
        .ok_or_else(|| Error::new("Clipboard drawing part is missing"))?;
    let bytes = source.read(&drawing)?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| Error::new("Clipboard drawing is not UTF-8"))?;
    let doc = roxmltree::Document::parse(text).map_err(|e| Error::new(e.to_string()))?;
    let canvas = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "lockedCanvas")
        .ok_or_else(|| Error::new("Clipboard drawing has no locked canvas"))?;
    let children: String = canvas
        .children()
        .filter(|n| n.is_element())
        .map(|n| &text[n.range()])
        .collect();
    let extra: String = canvas
        .namespaces()
        .filter(|n| !matches!(n.name(), Some("p" | "a" | "r" | "lc") | None))
        .map(|n| {
            format!(
                " xmlns:{}=\"{}\"",
                n.name().unwrap(),
                pptx_new::escape_attr(n.uri())
            )
        })
        .collect();
    let slide = format!(
        "<p:sld xmlns:p=\"{P}\" xmlns:a=\"{A}\" xmlns:r=\"{R}\"{extra}><p:cSld><p:spTree>{children}</p:spTree></p:cSld></p:sld>"
    );
    let mut target = Package::open(crate::blank_file(Kind::Presentation)?)?;
    for name in source
        .names()
        .filter(|n| *n != "[Content_Types].xml" && *n != "_rels/.rels")
    {
        target.set_bytes(name, source.read(name)?);
        if let Some(ct) = source.content_type(name)? {
            target.set_override(name, &ct)?;
        }
    }
    let mut entries: Vec<_> = target
        .relationships("ppt/slides/slide1.xml")?
        .into_iter()
        .filter(|r| r.kind == format!("{R}/slideLayout"))
        .map(|r| {
            (
                "rIdClipboardLayout".to_string(),
                r.kind,
                r.target,
                r.external,
            )
        })
        .collect();
    for r in source.relationships(&drawing)? {
        let target_part = r.target_part(&drawing);
        if r.kind == format!("{R}/theme")
            && let Some(part) = &target_part
        {
            target.set_bytes("ppt/theme/theme1.xml", source.read(part)?);
        }
        entries.push((
            r.id,
            r.kind,
            if r.external {
                r.target
            } else {
                format!("/{}", target_part.unwrap_or_default())
            },
            r.external,
        ));
    }
    target.set_bytes("ppt/slides/slide1.xml", slide.into_bytes());
    target.set_bytes(
        "ppt/slides/_rels/slide1.xml.rels",
        rels(&entries).into_bytes(),
    );
    crate::open(target.save(SaveOptions::default())?)
}
/// Write independently editable shapes in Office's DrawingML clipboard format.
pub fn write_gvml(presentation: &Presentation, shapes: &[Shape]) -> Result<Vec<u8>, Error> {
    if shapes.is_empty() {
        return Err(Error::new("No shapes to copy"));
    }
    let root = XmlDocument::parse(
        format!("<p:sld xmlns:p=\"{P}\" xmlns:a=\"{A}\" xmlns:r=\"{R}\"/>").as_bytes(),
    )
    .map_err(|e| Error::new(e.to_string()))?;
    let prefixes = Prefixes::at(&root, root.root_element().unwrap());
    let bounds = drawing_bounds(shapes).unwrap();
    let (min_x, min_y, width, height) = (bounds.x, bounds.y, bounds.width, bounds.height);
    let mut parts = Vec::new();
    let mut entries = Vec::new();
    let mut overrides = String::new();
    let mut media_rels = std::collections::HashMap::new();
    let mut needed = std::collections::HashSet::new();
    for shape in shapes {
        match &shape.kind {
            ShapeKind::Picture(p) => {
                needed.insert(p.media);
            }
            ShapeKind::Auto(a) => {
                if let Some(id) = a.fill.media() {
                    needed.insert(id);
                }
            }
            _ => {}
        }
        if let Some(body) = shape.text_body() {
            for p in &body.paragraphs {
                if let Some(id) = p.bullet.as_ref().and_then(|b| b.picture) {
                    needed.insert(id);
                }
            }
        }
    }
    for media in presentation.media.iter().filter(|m| needed.contains(&m.id)) {
        let (ext, ct) = match media.format {
            MediaFormat::Png => ("png", "image/png"),
            MediaFormat::Jpeg => ("jpg", "image/jpeg"),
            MediaFormat::Gif => ("gif", "image/gif"),
            MediaFormat::Bmp => ("bmp", "image/bmp"),
            MediaFormat::Tiff => ("tiff", "image/tiff"),
            MediaFormat::Svg => ("svg", "image/svg+xml"),
            MediaFormat::Webp => ("webp", "image/webp"),
            MediaFormat::Emf => ("emf", "image/x-emf"),
            MediaFormat::Wmf => ("wmf", "image/x-wmf"),
            _ => ("bin", "application/octet-stream"),
        };
        let name = format!("clipboard/media/image{}.{}", media.id.0, ext);
        let id = format!("rIdMedia{}", media.id.0);
        parts.push((name.clone(), media.data.to_vec()));
        overrides.push_str(&format!(
            "<Override PartName=\"/{name}\" ContentType=\"{ct}\"/>"
        ));
        entries.push((id.clone(), format!("{R}/image"), format!("/{name}"), false));
        media_rels.insert(media.id, id);
    }
    pptx_new::set_media_rels(media_rels.clone());
    struct ResetMedia;
    impl Drop for ResetMedia {
        fn drop(&mut self) {
            pptx_new::set_media_rels(Default::default());
        }
    }
    let _reset = ResetMedia;
    let mut shapes_xml = String::new();
    for (i, shape) in shapes.iter().enumerate() {
        let mut shape = shape.clone();
        shape.frame.x -= min_x;
        shape.frame.y -= min_y;
        let rel = match &shape.kind {
            ShapeKind::Picture(p) => media_rels.get(&p.media).map(String::as_str),
            _ => None,
        };
        let xml = pptx_new::shape_xml(&prefixes, &shape, i as u32 + 1, rel)?;
        // GVML uses DrawingML's shape namespace and a text-shape wrapper.
        let xml = xml
            .replace("<p:nvPr/>", "")
            .replace("<p:", "<a:")
            .replace("</p:", "</a:")
            .replace("<a:txBody>", "<a:txSp><a:txBody>")
            .replace("</a:txBody>", "</a:txBody><a:useSpRect/></a:txSp>");
        shapes_xml.push_str(&xml);
    }
    pptx_new::set_media_rels(Default::default());
    let drawing = format!(
        "<a:graphic xmlns:a=\"{A}\" xmlns:r=\"{R}\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/lockedCanvas\"><lc:lockedCanvas xmlns:lc=\"http://schemas.openxmlformats.org/drawingml/2006/lockedCanvas\"><a:nvGrpSpPr><a:cNvPr id=\"0\" name=\"\"/><a:cNvGrpSpPr/></a:nvGrpSpPr><a:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{width}\" cy=\"{height}\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"{width}\" cy=\"{height}\"/></a:xfrm></a:grpSpPr>{shapes_xml}</lc:lockedCanvas></a:graphicData></a:graphic>"
    );
    let types = format!(
        "<Types xmlns=\"{CT}\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/clipboard/drawings/drawing1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>{overrides}</Types>"
    );
    parts.push(("[Content_Types].xml".into(), types.into_bytes()));
    parts.push((
        "_rels/.rels".into(),
        rels(&[(
            "rId1".into(),
            format!("{R}/drawing"),
            "clipboard/drawings/drawing1.xml".into(),
            false,
        )])
        .into_bytes(),
    ));
    parts.push((
        "clipboard/drawings/drawing1.xml".into(),
        drawing.into_bytes(),
    ));
    parts.push((
        "clipboard/drawings/_rels/drawing1.xml.rels".into(),
        rels(&entries).into_bytes(),
    ));
    let mut zip = Writer::new();
    for (name, data) in parts {
        zip.add(&name, EntryMeta::new(), &data)?;
    }
    Ok(zip.finish()?)
}

/// Visual bounding rectangle after each shape's rotation, including its stroke.
pub fn drawing_bounds(shapes: &[Shape]) -> Option<ibis_doc_model::EmuRect> {
    let mut bounds: Option<(f64, f64, f64, f64)> = None;
    for shape in shapes {
        let angle = (shape.rotation as f64 / 60000.).to_radians();
        let (sin, cos) = angle.sin_cos();
        let w = shape.frame.width as f64;
        let h = shape.frame.height as f64;
        let cx = shape.frame.x as f64 + w / 2.;
        let cy = shape.frame.y as f64 + h / 2.;
        let stroke = match &shape.kind {
            ShapeKind::Auto(a) => a.stroke.as_ref().map_or(0, |s| s.width),
            ShapeKind::Line(s) => s.width,
            _ => 0,
        } as f64
            / 2.;
        let hw = (w * cos.abs() + h * sin.abs()) / 2. + stroke;
        let hh = (w * sin.abs() + h * cos.abs()) / 2. + stroke;
        bounds = Some(match bounds {
            None => (cx - hw, cy - hh, cx + hw, cy + hh),
            Some((l, t, r, b)) => (
                l.min(cx - hw),
                t.min(cy - hh),
                r.max(cx + hw),
                b.max(cy + hh),
            ),
        });
    }
    bounds.map(|(l, t, r, b)| {
        let x = l.floor() as i64;
        let y = t.floor() as i64;
        ibis_doc_model::EmuRect::new(
            x,
            y,
            (r.ceil() as i64 - x).max(1),
            (b.ceil() as i64 - y).max(1),
        )
    })
}
