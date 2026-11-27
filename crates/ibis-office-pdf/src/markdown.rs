// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/272)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Markdown export for the fixed-layout viewer.

use std::fmt::Write as _;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ibis_pdf::text_layout::{TextLayout, page_text};
use ibis_pdf::{
    DisplayCmd, Error, PdfRect, Result,
    scene::{FillRule, PageScene, PathCmd},
};
use image::ImageEncoder as _;

use crate::{PdfViewerState, ViewerSource};

/// How the text of each page is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportOptions {
    /// [`TextLayout::Physical`] keeps each character at its column (the cells of a table stay aligned) in a ```` ```text ````
    /// block, the characters as the page has them; [`TextLayout::Reading`] writes Markdown lines.
    pub layout: TextLayout,
}

/// None means the input is outside the fixed-layout Markdown export formats.
pub fn export(bytes: &[u8]) -> Result<Option<String>> {
    export_with(bytes, ExportOptions::default())
}

/// [`export`] with the page text written as `options` say.
pub fn export_with(bytes: &[u8], options: ExportOptions) -> Result<Option<String>> {
    if !bytes.starts_with(b"%PDF-") && !ibis_ofd::is_ofd(bytes) && !ibis_xps::is_xps(bytes) {
        return Ok(None);
    }
    let mut markdown = String::new();
    if bytes.starts_with(b"%PDF-") {
        let mut container = ibis_pdf::PdfContainer::from_bytes(bytes.to_vec())?;
        ibis_pdf::interpret::visit_export_scenes(&mut container, |scene| {
            write_page(&scene, scene.info.index + 1, options, &mut markdown)
        })?;
        return Ok(Some(markdown));
    }
    let mut viewer = PdfViewerState::open(bytes.to_vec())?;
    if !matches!(viewer.source, ViewerSource::Ofd(_) | ViewerSource::Xps(_)) {
        return Ok(None);
    }
    for index in 0..viewer.pages.len() {
        let scene = viewer
            .page_scene(index)
            .map_err(|error| Error::new(format!("Page {}: {error}", index + 1)))?;
        write_page(&scene, index + 1, options, &mut markdown)?;
        // The exporter consumes each page once.
        viewer.scene_cache.remove(&index);
    }
    Ok(Some(markdown))
}

fn write_page(scene: &PageScene, number: usize, options: ExportOptions, markdown: &mut String) -> Result<()> {
    if number > 1 {
        markdown.push_str("\\newpage\n\n");
    }
    let _ = writeln!(markdown, "<!-- fixed-layout-page: {number} -->\n");
    let text = page_text(&scene.text_index, options.layout);
    if text.trim().is_empty() {
        let _ = writeln!(markdown, "<!-- fixed-layout-diagnostic: Page {number}: no extractable text -->\n");
    } else if options.layout == TextLayout::Physical {
        // The columns are spaces: a code block keeps them, and the characters, as they are.
        let fence = "`".repeat(longest_backtick_run(&text).max(2) + 1);
        let _ = write!(markdown, "{fence}text\n{text}{fence}\n\n");
    } else {
        // Keep the original lines visible without interpreting them as Markdown.
        for line in text.lines() {
            if line.is_empty() {
                markdown.push('\n');
                continue;
            }
            for ch in line.chars() {
                if ch.is_ascii_punctuation() {
                    markdown.push('\\');
                }
                markdown.push(ch);
            }
            markdown.push_str("  \n");
        }
        markdown.push('\n');
    }

    let (svg, omitted_images) = graphics(scene)?;
    if omitted_images > 0 {
        let _ = writeln!(
            markdown,
            "<!-- fixed-layout-diagnostic: Page {number}: {omitted_images} image(s) could not be decoded -->\n"
        );
    }
    if let Some(svg) = svg {
        let (width, height) = page_extent(scene);
        let scale = 140.0 / width.max(height);
        let _ = writeln!(
            markdown,
            "![Page {number} graphics](data:image/svg+xml;base64,{}){{width={:.2}mm height={:.2}mm}}\n",
            STANDARD.encode(svg),
            width * scale,
            height * scale,
        );
    }
    Ok(())
}

/// The longest run of backticks in `text` (a fence around it must be longer).
fn longest_backtick_run(text: &str) -> usize {
    let (mut longest, mut run) = (0, 0);
    for ch in text.chars() {
        run = if ch == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest
}

fn rect_attrs(rect: PdfRect) -> String {
    format!(
        "x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
        rect.x0,
        rect.y0,
        rect.width(),
        rect.height()
    )
}

fn paint_attrs(kind: &str, color: Option<[u8; 4]>) -> String {
    let Some([r, g, b, alpha]) = color else {
        return format!("{kind}=\"none\"");
    };
    format!(
        "{kind}=\"#{r:02x}{g:02x}{b:02x}\" {kind}-opacity=\"{}\"",
        f32::from(alpha) / 255.0,
    )
}

fn intersect(a: PdfRect, b: PdfRect) -> PdfRect {
    let x = a.x0.max(b.x0);
    let y = a.y0.max(b.y0);
    PdfRect {
        x0: x,
        y0: y,
        x1: a.x1.min(b.x1).max(x),
        y1: a.y1.min(b.y1).max(y),
    }
}

fn page_extent(scene: &PageScene) -> (f32, f32) {
    let (width, height) = (scene.info.crop_box.width(), scene.info.crop_box.height());
    if matches!(scene.info.rotate, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    }
}

/// A positioned, text-free layer of the graphics decoded by the viewer.
fn graphics(scene: &PageScene) -> Result<(Option<String>, usize)> {
    let mut svg = String::new();
    let (width, height) = page_extent(scene);
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(Error::new(format!(
            "Invalid size on page {}",
            scene.info.index + 1
        )));
    }
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{width}pt\" height=\"{height}pt\" viewBox=\"0 0 {width} {height}\">",
    );
    let mut has_graphics = false;
    let mut omitted_images = 0;
    let mut clip = None;
    let mut clips = Vec::new();
    let mut groups = 0;
    for (index, command) in scene.display_list.iter().enumerate() {
        match command {
            DisplayCmd::SetClipRect(rect) => {
                clip = *rect;
                continue;
            }
            DisplayCmd::PushClip(rect) => {
                clips.push(clip);
                clip = Some(clip.map_or(*rect, |previous| intersect(previous, *rect)));
                continue;
            }
            DisplayCmd::PopClip => {
                clip = clips.pop().flatten();
                continue;
            }
            DisplayCmd::PushTransparencyGroup { opacity } => {
                let _ = write!(svg, "<g opacity=\"{}\">", opacity.clamp(0.0, 1.0));
                groups += 1;
                continue;
            }
            DisplayCmd::PopTransparencyGroup => {
                if groups > 0 {
                    svg.push_str("</g>");
                    groups -= 1;
                }
                continue;
            }
            DisplayCmd::DrawGlyphRun { .. } | DisplayCmd::DrawStyledGlyphRun { .. } => continue,
            DisplayCmd::DrawImage { rgba, .. } if rgba.is_empty() => {
                omitted_images += 1;
                continue;
            }
            _ => {}
        }
        if let Some(rect) = clip {
            let attrs = rect_attrs(rect);
            let _ = write!(
                svg,
                "<defs><clipPath id=\"clip{index}\"><rect {attrs}/></clipPath></defs><g clip-path=\"url(#clip{index})\">"
            );
        }
        match command {
            DisplayCmd::DrawRect {
                rect,
                fill_rgba: Some(color),
            } => {
                let _ = write!(
                    svg,
                    "<rect {} {}/>",
                    rect_attrs(*rect),
                    paint_attrs("fill", Some(*color))
                );
                has_graphics = true;
            }
            DisplayCmd::DrawPath {
                commands,
                fill_rule,
                fill_rgba,
                stroke_rgba,
                stroke_width,
                ..
            } if !commands.is_empty() && (fill_rgba.is_some() || stroke_rgba.is_some()) => {
                let mut path = String::new();
                for command in commands {
                    let _ = match command {
                        PathCmd::MoveTo(x, y) => write!(path, "M{x} {y} "),
                        PathCmd::LineTo(x, y) => write!(path, "L{x} {y} "),
                        PathCmd::QuadTo(x1, y1, x, y) => write!(path, "Q{x1} {y1} {x} {y} "),
                        PathCmd::CubicTo(x1, y1, x2, y2, x, y) => {
                            write!(path, "C{x1} {y1} {x2} {y2} {x} {y} ")
                        }
                        PathCmd::Close => write!(path, "Z "),
                    };
                }
                let rule = if *fill_rule == FillRule::EvenOdd {
                    "evenodd"
                } else {
                    "nonzero"
                };
                let _ = write!(
                    svg,
                    "<path d=\"{path}\" fill-rule=\"{rule}\" {} {} stroke-width=\"{stroke_width}\"/>",
                    paint_attrs("fill", *fill_rgba),
                    paint_attrs("stroke", *stroke_rgba)
                );
                has_graphics = true;
            }
            DisplayCmd::DrawImage {
                bounds,
                width,
                height,
                rgba,
            } => {
                let len = u64::from(*width) * u64::from(*height);
                if *width == 0 || *height == 0 || len.checked_mul(4) != Some(rgba.len() as u64) {
                    return Err(Error::new(format!(
                        "Invalid image on page {}",
                        scene.info.index + 1
                    )));
                }
                let mut png = Vec::new();
                image::codecs::png::PngEncoder::new(&mut png)
                    .write_image(rgba, *width, *height, image::ExtendedColorType::Rgba8)
                    .map_err(|error| Error::new(error.to_string()))?;
                let _ = write!(
                    svg,
                    "<image {} preserveAspectRatio=\"none\" xlink:href=\"data:image/png;base64,{}\"/>",
                    rect_attrs(*bounds),
                    STANDARD.encode(png)
                );
                has_graphics = true;
            }
            _ => {}
        }
        if clip.is_some() {
            svg.push_str("</g>");
        }
    }
    for _ in 0..groups {
        svg.push_str("</g>");
    }
    svg.push_str("</svg>");
    Ok((has_graphics.then_some(svg), omitted_images))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_pdf::{CharBox, PageInfo, PageTextIndex};

    #[test]
    fn exports_text_and_preserves_vector_and_image_commands() {
        let rect = PdfRect {
            x0: 0.0,
            y0: 0.0,
            x1: 20.0,
            y1: 30.0,
        };
        let scene = PageScene {
            info: PageInfo {
                index: 0,
                object_id: ibis_pdf::ObjectId::new(1, 0),
                media_box: rect,
                crop_box: rect,
                rotate: 0,
            },
            text_index: PageTextIndex {
                chars: vec![CharBox {
                    ch: '#',
                    rect_pt: rect,
                    vertical: false,
                    stream_op_index: 0,
                }],
            },
            display_list: vec![
                DisplayCmd::PushClip(rect),
                DisplayCmd::DrawPath {
                    bounds: rect,
                    commands: vec![
                        PathCmd::MoveTo(0.0, 0.0),
                        PathCmd::LineTo(2.0, 3.0),
                        PathCmd::Close,
                    ],
                    fill_rule: FillRule::EvenOdd,
                    fill_rgba: Some([0, 16, 32, 128]),
                    stroke_rgba: None,
                    stroke_width: 0.0,
                },
                DisplayCmd::DrawImage {
                    bounds: rect,
                    width: 1,
                    height: 1,
                    rgba: vec![255, 0, 0, 255],
                },
                DisplayCmd::PopClip,
            ],
        };
        let mut markdown = String::new();
        write_page(&scene, 2, ExportOptions::default(), &mut markdown).unwrap();
        assert!(markdown.starts_with("\\newpage\n"));
        assert!(markdown.contains(r"\#"));
        let svg = markdown
            .split_once("data:image/svg+xml;base64,")
            .unwrap()
            .1
            .split_once(')')
            .unwrap()
            .0;
        let svg = String::from_utf8(STANDARD.decode(svg).unwrap()).unwrap();
        assert!(svg.contains("viewBox=\"0 0 20 30\""));
        assert!(svg.contains("<clipPath"));
        assert!(svg.contains("fill-rule=\"evenodd\""));
        let image = svg
            .split_once("data:image/png;base64,")
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0;
        let png = STANDARD.decode(image).unwrap();
        assert_eq!(
            image::load_from_memory(&png).unwrap().to_rgba8().as_raw(),
            &[255, 0, 0, 255]
        );
    }

    fn svg_of(markdown: &str) -> String {
        let b64 = markdown
            .split_once("data:image/svg+xml;base64,")
            .unwrap()
            .1
            .split_once(')')
            .unwrap()
            .0;
        String::from_utf8(STANDARD.decode(b64).unwrap()).unwrap()
    }

    fn synthetic_pdf() -> Vec<u8> {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_owned(),
            {
                let stream = "0.9 g 10 10 80 30 re f BT /F1 18 Tf 20 60 Td (PDF sample) Tj ET";
                format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len())
            },
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{body}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1
            )
            .bytes(),
        );
        out
    }

    fn synthetic_xps() -> Vec<u8> {
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
</FixedPage>"##
                .as_bytes(),
        )
        .unwrap();
        w.finish().unwrap()
    }

    fn synthetic_ofd() -> Vec<u8> {
        let mut w = ibis_ooxml::zip::Writer::new();
        let meta = ibis_ooxml::zip::EntryMeta::default();
        w.add(
            "OFD.xml",
            meta,
            br#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:OFD xmlns:ofd="http://www.ofdspec.org/2016" Version="1.1">
  <ofd:DocBody><ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot></ofd:DocBody>
</ofd:OFD>"#,
        )
        .unwrap();
        w.add(
            "Doc_0/Document.xml",
            meta,
            br#"<?xml version="1.0" encoding="UTF-8"?>
<ofd:Document xmlns:ofd="http://www.ofdspec.org/2016">
  <ofd:CommonData><ofd:PageArea><ofd:PhysicalBox>0 0 210 297</ofd:PhysicalBox></ofd:PageArea></ofd:CommonData>
  <ofd:Pages><ofd:Page ID="1" BaseLoc="Pages/Page_0/Content.xml"/></ofd:Pages>
</ofd:Document>"#,
        )
        .unwrap();
        w.add(
            "Doc_0/Pages/Page_0/Content.xml",
            meta,
            r#"<?xml version="1.0" encoding="UTF-8"?>
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
        <ofd:TextCode X="0" Y="5">OFD sample</ofd:TextCode>
      </ofd:TextObject>
    </ofd:Layer>
  </ofd:Content>
</ofd:Page>"#
                .as_bytes(),
        )
        .unwrap();
        w.finish().unwrap()
    }

    #[test]
    fn pdf_xps_and_ofd_export_text_and_a_svg_figure_per_page() {
        for (bytes, text) in [
            (synthetic_pdf(), "PDF sample"),
            (synthetic_xps(), "XPSテスト"),
            (synthetic_ofd(), "OFD sample"),
        ] {
            let markdown = export(&bytes).unwrap().expect("a fixed-layout document");
            assert!(markdown.contains(text), "{text}: {markdown}");
            let svg = svg_of(&markdown);
            assert!(svg.contains("<svg"), "{text}");
            assert!(svg.contains("<path"), "{text}: the filled shape is drawn");
        }
        assert!(export(b"not a fixed-layout document").unwrap().is_none());
    }

    #[test]
    fn exported_markdown_becomes_a_docx_that_keeps_the_figure() {
        for bytes in [synthetic_pdf(), synthetic_xps(), synthetic_ofd()] {
            let markdown = export(&bytes).unwrap().unwrap();
            let docx = ibis_formats::markdown::import_markdown_to_docx_bytes(markdown.as_bytes()).unwrap();
            let opened = ibis_formats::open(docx).unwrap();
            assert!(
                opened
                    .presentation
                    .media
                    .iter()
                    .any(|m| m.format == ibis_doc_model::MediaFormat::Svg),
                "the page figure is kept as SVG"
            );
        }
    }

    fn text_scene(chars: Vec<CharBox>) -> PageScene {
        let rect = PdfRect { x0: 0.0, y0: 0.0, x1: 200.0, y1: 100.0 };
        PageScene {
            info: PageInfo { index: 0, object_id: ibis_pdf::ObjectId::new(1, 0), media_box: rect, crop_box: rect, rotate: 0 },
            text_index: PageTextIndex { chars },
            display_list: Vec::new(),
        }
    }

    fn row(chars: &mut Vec<CharBox>, text: &str, x: f32, top: f32) {
        let mut x = x;
        for ch in text.chars() {
            let w = 10.0 * ibis_pdf::scene::estimated_advance_em(ch);
            let op = chars.len();
            chars.push(CharBox { ch, rect_pt: PdfRect { x0: x, y0: top, x1: x + w, y1: top + 10.0 }, vertical: false, stream_op_index: op });
            x += w;
        }
    }

    #[test]
    fn the_physical_layout_writes_aligned_columns_in_a_text_block() {
        let mut chars = Vec::new();
        row(&mut chars, "ID", 0.0, 0.0);
        row(&mut chars, "1.1.", 80.0, 0.0);
        row(&mut chars, "0190001", 0.0, 12.0);
        row(&mut chars, "住民`", 80.0, 12.0);
        let mut markdown = String::new();
        write_page(&text_scene(chars), 1, ExportOptions { layout: TextLayout::Physical }, &mut markdown).unwrap();
        let block = markdown.split_once("```text\n").unwrap().1.split_once("\n```\n").unwrap().0;
        let lines: Vec<&str> = block.lines().collect();
        // No escapes, and the second column starts at the same column of both lines.
        assert_eq!(lines[0].trim_start_matches("ID").trim_start().chars().next(), Some('1'));
        assert_eq!(lines[0].find("1.1."), lines[1].find("住民`"));
        assert!(!markdown.contains('\\'));
    }

    #[test]
    fn a_text_block_fence_is_longer_than_any_backtick_run_in_it() {
        let mut chars = Vec::new();
        row(&mut chars, "a```b", 0.0, 0.0);
        let mut markdown = String::new();
        write_page(&text_scene(chars), 1, ExportOptions { layout: TextLayout::Physical }, &mut markdown).unwrap();
        assert!(markdown.contains("````text\na```b\n````\n"));
    }
}
