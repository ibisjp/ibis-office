// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OpenDocument Text (`.odt`, ODF 1.2/1.3) import and export (moved from ibis-formats' `docx_convert`).

use ibis_doc_model::{FlowAlign, FlowDocument, Presentation};
use ibis_formats::docx_convert::{doc_flow_to_conv_blocks, xml_escape, ConvBlock, ConvPara};
use ibis_formats::{Error, Opened};
use ibis_ooxml::zip::{EntryMeta, Writer};
use ibis_formats::zip::Archive;

// =========================================================================
// 5. OpenDocument Text (.odt) — DOCX-FILE-012
// =========================================================================

/// Export a `FlowDocument` as an OASIS OpenDocument Text (`.odt`) ZIP package.
pub fn export_odt(doc: &FlowDocument) -> Result<Vec<u8>, Error> {
    let blocks = doc_flow_to_conv_blocks(doc);
    let mut auto_styles = String::new();
    let mut body_xml = String::new();
    let mut p_style_counter = 0usize;
    let mut t_style_counter = 0usize;

    let mut render_conv_para = |body_xml: &mut String, p: &ConvPara| {
        p_style_counter += 1;
        let p_style_name = format!("P{p_style_counter}");
        let fo_align = match p.align {
            FlowAlign::Left => "start",
            FlowAlign::Center => "center",
            FlowAlign::Right => "end",
            FlowAlign::Justify | FlowAlign::Distribute => "justify",
        };
        // A heading's automatic style has the default heading style `Heading N` as its parent (as LibreOffice writes it), so that a reader
        // knows the paragraph is a heading of that level.
        let parent = p.heading.map(|h| format!(r#" style:parent-style-name="Heading_20_{h}""#)).unwrap_or_default();
        auto_styles.push_str(&format!(
            r#"<style:style style:name="{p_style_name}" style:family="paragraph"{parent}><style:paragraph-properties fo:text-align="{fo_align}"/></style:style>"#
        ));

        if let Some(h) = p.heading {
            body_xml.push_str(&format!(
                r#"<text:h text:style-name="{p_style_name}" text:outline-level="{h}">"#
            ));
        } else {
            body_xml.push_str(&format!(r#"<text:p text:style-name="{p_style_name}">"#));
        }

        for r in &p.runs {
            if r.is_tab {
                body_xml.push_str("<text:tab/>");
                continue;
            }
            if r.is_line_break {
                body_xml.push_str("<text:line-break/>");
                continue;
            }
            if r.text.is_empty() {
                continue;
            }
            t_style_counter += 1;
            let t_style_name = format!("T{t_style_counter}");
            let mut props = String::new();
            if r.bold {
                props.push_str(r#" fo:font-weight="bold" style:font-weight-asian="bold""#);
            }
            if r.italic {
                props.push_str(r#" fo:font-style="italic" style:font-style-asian="italic""#);
            }
            if r.underline {
                props.push_str(r#" style:text-underline-style="solid""#);
            }
            if r.strike {
                props.push_str(r#" style:text-line-through-style="solid""#);
            }
            if let Some(c) = r.color {
                props.push_str(&format!(r##" fo:color="#{:06X}""##, c.0 & 0xFF_FFFF));
            }
            if let Some(sz) = r.size_half_pt {
                let pt = sz as f32 / 2.0;
                props.push_str(&format!(
                    r#" fo:font-size="{pt}pt" style:font-size-asian="{pt}pt""#
                ));
            }
            auto_styles.push_str(&format!(
                r#"<style:style style:name="{t_style_name}" style:family="text"><style:text-properties{props}/></style:style>"#
            ));

            if let Some(url) = &r.link_url {
                body_xml.push_str(&format!(
                    r#"<text:a xlink:type="simple" xlink:href="{}"><text:span text:style-name="{t_style_name}">{}</text:span></text:a>"#,
                    xml_escape(url),
                    xml_escape(&r.text)
                ));
            } else {
                body_xml.push_str(&format!(
                    r#"<text:span text:style-name="{t_style_name}">{}</text:span>"#,
                    xml_escape(&r.text)
                ));
            }
        }

        if p.heading.is_some() {
            body_xml.push_str("</text:h>");
        } else {
            body_xml.push_str("</text:p>");
        }
    };

    for block in &blocks {
        match block {
            ConvBlock::Para(p) => render_conv_para(&mut body_xml, p),
            ConvBlock::Table(t) => {
                body_xml.push_str("<table:table table:name=\"Table1\">");
                let cols = t.rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
                body_xml.push_str(&format!(
                    r#"<table:table-column table:number-columns-repeated="{cols}"/>"#
                ));
                for row in &t.rows {
                    body_xml.push_str("<table:table-row>");
                    for cell in row {
                        body_xml.push_str("<table:table-cell office:value-type=\"string\">");
                        for p in cell {
                            render_conv_para(&mut body_xml, p);
                        }
                        body_xml.push_str("</table:table-cell>");
                    }
                    body_xml.push_str("</table:table-row>");
                }
                body_xml.push_str("</table:table>");
            }
        }
    }

    let content_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.2">
<office:automatic-styles>{auto_styles}</office:automatic-styles>
<office:body><office:text>{body_xml}</office:text></office:body>
</office:document-content>"#
    );

    let manifest_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.2">
<manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>
<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
<manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
<manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#;

    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" office:version="1.2">
<office:styles/>
</office:document-styles>"#;

    let title_esc = xml_escape(doc.properties.title.as_deref().unwrap_or(""));
    let meta_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-meta xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" office:version="1.2">
<office:meta><dc:title>{title_esc}</dc:title></office:meta>
</office:document-meta>"#
    );

    let mut writer = Writer::new();
    for (name, content) in [
        ("mimetype", b"application/vnd.oasis.opendocument.text".as_slice()),
        ("META-INF/manifest.xml", manifest_xml.as_bytes()),
        ("content.xml", content_xml.as_bytes()),
        ("styles.xml", styles_xml.as_bytes()),
        ("meta.xml", meta_xml.as_bytes()),
    ] {
        writer
            .add(name, EntryMeta::new(), content)
            .map_err(|e| Error::new(e.to_string()))?;
    }
    writer.finish().map_err(|e| Error::new(e.to_string()))
}

/// Export a `Presentation` (Word `.docx`) as OpenDocument Text (`.odt`) bytes.
pub fn export_docx_to_odt(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    let doc = presentation
        .flow
        .as_ref()
        .ok_or_else(|| Error::new("Word 文書データがありません"))?;
    export_odt(doc)
}

/// Convert OpenDocument Text (`.odt`) bytes into `.docx` package bytes.
pub fn import_odt_to_docx_bytes(data: &[u8]) -> Result<Vec<u8>, Error> {
    crate::odt_import::import(data)
}

/// Whether `data` is an OpenDocument text document or template (a zip whose `mimetype` says so).
pub fn is_odt(data: &[u8]) -> bool {
    if !data.starts_with(b"PK") {
        return false;
    }
    let Ok(archive) = Archive::new(data) else { return false };
    archive.read("mimetype").ok().flatten().is_some_and(|m| m.starts_with(b"application/vnd.oasis.opendocument.text"))
}

/// Open OpenDocument Text (`.odt`) bytes as an editable Word document (`Opened`).
pub fn import_odt(data: &[u8]) -> Result<Opened, Error> {
    let docx_bytes = import_odt_to_docx_bytes(data)?;
    ibis_formats::open(docx_bytes)
}

