// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Document format conversion for Word (`.docx`) documents:
//! - `DOCX-SEC-019`: Plain Text (`.txt`), Rich Text Format (`.rtf`), and legacy `.doc` conversion
//! - `DOCX-FILE-010`: Word 97-2003 binary `.doc` (OLE2 Compound File Binary / FIB / WordDocument) import & export
//! - `DOCX-FILE-012`: OpenDocument Text (`.odt`, ODF 1.2/1.3) and HTML (`.html`/`.htm`) import & export
//! - `DOCX-FILE-013`: PDF to editable Word document conversion (`.pdf` text/layout reflow)

use std::collections::HashMap;

use ibis_doc_model::{
    Block, FlowAlign, FlowDocument, FlowParagraph, FlowRun, FlowRunKind, Link,
    Presentation, Rgb,
};
use ibis_ooxml::cfb::{self, Item as CfbItem};
use ibis_ooxml::zip::{EntryMeta, Writer};

use crate::zip::Archive;
use crate::{Error, Opened};

/// Supported conversion formats for Word documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocxConvertFormat {
    /// 書式なしテキスト (`.txt`) — `DOCX-SEC-019`
    Txt,
    /// リッチ テキスト形式 (`.rtf`) — `DOCX-SEC-019`
    Rtf,
    /// Word 97-2003 文書 (`.doc`) — `DOCX-FILE-010`, `DOCX-SEC-019`
    Doc,
    /// Web ページ (`.html` / `.htm`) — `DOCX-FILE-012`
    Html,
    /// OpenDocument テキスト (`.odt`) — `DOCX-FILE-012`
    Odt,
    /// PDF から文書への変換 (`.pdf`) — `DOCX-FILE-013`
    Pdf,
}

impl DocxConvertFormat {
    pub const EXPORTABLE: [Self; 5] = [Self::Doc, Self::Rtf, Self::Txt, Self::Html, Self::Odt];
    pub const IMPORTABLE: [Self; 6] = [Self::Doc, Self::Rtf, Self::Txt, Self::Html, Self::Odt, Self::Pdf];

    pub fn extension(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Rtf => "rtf",
            Self::Doc => "doc",
            Self::Html => "html",
            Self::Odt => "odt",
            Self::Pdf => "pdf",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "txt" | "text" => Some(Self::Txt),
            "rtf" => Some(Self::Rtf),
            "doc" => Some(Self::Doc),
            "html" | "htm" => Some(Self::Html),
            "odt" => Some(Self::Odt),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    pub fn from_path(path: &std::path::Path) -> Option<Self> {
        path.extension().and_then(|e| e.to_str()).and_then(Self::from_extension)
    }

    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"%PDF") {
            Some(Self::Pdf)
        } else if cfb::is_cfb(bytes) {
            Some(Self::Doc)
        } else if bytes.starts_with(b"{\\rtf") {
            Some(Self::Rtf)
        } else if bytes.starts_with(b"PK\x03\x04") {
            Some(Self::Odt)
        } else {
            let s = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
            if s.contains("<!doctype html") || s.contains("<html") || s.contains("<body") {
                Some(Self::Html)
            } else {
                Some(Self::Txt)
            }
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn xml_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(semi) = s[i..].find(';') {
                let ent = &s[i + 1..i + semi];
                let decoded = match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some('\u{00A0}'),
                    _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                        u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32)
                    }
                    _ if ent.starts_with('#') => {
                        ent[1..].parse::<u32>().ok().and_then(char::from_u32)
                    }
                    _ => None,
                };
                if let Some(ch) = decoded {
                    out.push(ch);
                    i += semi + 1;
                    continue;
                }
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Extract the visible text of a single `FlowRun`.
fn run_visible_text(run: &FlowRun) -> String {
    if run.revision.as_ref().is_some_and(|r| r.kind.is_delete()) {
        return String::new();
    }
    match &run.kind {
        FlowRunKind::Text(t) => t.chars().map(|c| if c == ibis_doc_model::OPTIONAL_HYPHEN { '\u{ad}' } else { c }).collect(),
        FlowRunKind::Tab | FlowRunKind::PositionalTab { .. } => "\t".to_string(),
        FlowRunKind::LineBreak => "\n".to_string(),
        FlowRunKind::PageRef { shown, .. }
        | FlowRunKind::Field { shown, .. } => shown.clone(),
        FlowRunKind::FileName(s) => s.clone(),
        FlowRunKind::Ruby { base, .. } => base.clone(),
        FlowRunKind::RichText { text, .. }
        | FlowRunKind::PlainText { text, .. }
        | FlowRunKind::DatePicker { text, .. }
        | FlowRunKind::BuildingBlockGallery { text, .. }
        | FlowRunKind::Group { text, .. } => text.clone(),
        FlowRunKind::Math { math, .. } => math.to_latex(),
        _ => String::new(),
    }
}

fn paragraph_plain_text(p: &FlowParagraph) -> String {
    let mut s = String::new();
    for run in &p.runs {
        s.push_str(&run_visible_text(run));
    }
    s
}

fn paragraph_heading_level(p: &FlowParagraph) -> Option<u8> {
    let style = p.style.as_deref()?;
    let lower = style.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("heading") {
        let lvl: u8 = rest.trim().parse().ok()?;
        if (1..=6).contains(&lvl) {
            return Some(lvl);
        }
    }
    if let Some(rest) = style.strip_prefix("見出し") {
        let lvl: u8 = rest.trim().parse().ok()?;
        if (1..=6).contains(&lvl) {
            return Some(lvl);
        }
    }
    None
}

// =========================================================================
// Intermediate builder -> valid .docx package bytes
// =========================================================================

#[derive(Clone, Debug, Default)]
struct ConvRun {
    text: String,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<Rgb>,
    size_half_pt: Option<u32>,
    link_url: Option<String>,
    is_tab: bool,
    is_line_break: bool,
}

#[derive(Clone, Debug, Default)]
struct ConvPara {
    heading: Option<u8>,
    align: FlowAlign,
    runs: Vec<ConvRun>,
}

#[derive(Clone, Debug, Default)]
struct ConvTable {
    rows: Vec<Vec<Vec<ConvPara>>>,
}

#[derive(Clone, Debug)]
enum ConvBlock {
    Para(ConvPara),
    Table(ConvTable),
}

fn build_docx_from_blocks(blocks: &[ConvBlock], title: Option<&str>) -> Result<Vec<u8>, Error> {
    let mut doc_xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
<w:body>"#,
    );

    let mut rels: Vec<(String, String)> = Vec::new();
    let write_para = |doc_xml: &mut String, p: &ConvPara, rels: &mut Vec<(String, String)>| {
        doc_xml.push_str("<w:p>");
        let has_ppr = p.heading.is_some() || p.align != FlowAlign::Left;
        if has_ppr {
            doc_xml.push_str("<w:pPr>");
            if let Some(h) = p.heading {
                let h = h.clamp(1, 6);
                doc_xml.push_str(&format!(r#"<w:pStyle w:val="Heading{h}"/>"#));
            }
            match p.align {
                FlowAlign::Left => {}
                FlowAlign::Center => doc_xml.push_str(r#"<w:jc w:val="center"/>"#),
                FlowAlign::Right => doc_xml.push_str(r#"<w:jc w:val="right"/>"#),
                FlowAlign::Justify => doc_xml.push_str(r#"<w:jc w:val="both"/>"#),
                FlowAlign::Distribute => doc_xml.push_str(r#"<w:jc w:val="distribute"/>"#),
            }
            doc_xml.push_str("</w:pPr>");
        }
        for r in &p.runs {
            let link_rid = r.link_url.as_ref().map(|url| {
                let rid = format!("rIdLink{}", rels.len() + 1);
                rels.push((rid.clone(), url.clone()));
                rid
            });
            if let Some(ref rid) = link_rid {
                doc_xml.push_str(&format!(r#"<w:hyperlink r:id="{rid}">"#));
            }
            doc_xml.push_str("<w:r>");
            let has_rpr = r.bold
                || r.italic
                || r.underline
                || r.strike
                || r.color.is_some()
                || r.size_half_pt.is_some();
            if has_rpr {
                doc_xml.push_str("<w:rPr>");
                if r.bold {
                    doc_xml.push_str("<w:b/>");
                }
                if r.italic {
                    doc_xml.push_str("<w:i/>");
                }
                if r.underline {
                    doc_xml.push_str(r#"<w:u w:val="single"/>"#);
                }
                if r.strike {
                    doc_xml.push_str("<w:strike/>");
                }
                if let Some(c) = r.color {
                    doc_xml.push_str(&format!(r#"<w:color w:val="{:06X}"/>"#, c.0 & 0xFF_FFFF));
                }
                if let Some(sz) = r.size_half_pt {
                    doc_xml.push_str(&format!(r#"<w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/>"#));
                }
                doc_xml.push_str("</w:rPr>");
            }
            if r.is_tab {
                doc_xml.push_str("<w:tab/>");
            } else if r.is_line_break {
                doc_xml.push_str("<w:br/>");
            } else {
                doc_xml.push_str(r#"<w:t xml:space="preserve">"#);
                doc_xml.push_str(&xml_escape(&r.text));
                doc_xml.push_str("</w:t>");
            }
            doc_xml.push_str("</w:r>");
            if link_rid.is_some() {
                doc_xml.push_str("</w:hyperlink>");
            }
        }
        doc_xml.push_str("</w:p>");
    };

    if blocks.is_empty() {
        doc_xml.push_str("<w:p/>");
    } else {
        for block in blocks {
            match block {
                ConvBlock::Para(p) => write_para(&mut doc_xml, p, &mut rels),
                ConvBlock::Table(t) => {
                    let cols = t.rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
                    let col_w = 8500 / cols;
                    doc_xml.push_str("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/></w:tblBorders></w:tblPr><w:tblGrid>");
                    for _ in 0..cols {
                        doc_xml.push_str(&format!(r#"<w:gridCol w:w="{col_w}"/>"#));
                    }
                    doc_xml.push_str("</w:tblGrid>");
                    for row in &t.rows {
                        doc_xml.push_str("<w:tr>");
                        for c_idx in 0..cols {
                            doc_xml.push_str(&format!(
                                r#"<w:tc><w:tcPr><w:tcW w:w="{col_w}" w:type="dxa"/></w:tcPr>"#
                            ));
                            if let Some(cell_paras) = row.get(c_idx) {
                                if cell_paras.is_empty() {
                                    doc_xml.push_str("<w:p/>");
                                } else {
                                    for p in cell_paras {
                                        write_para(&mut doc_xml, p, &mut rels);
                                    }
                                }
                            } else {
                                doc_xml.push_str("<w:p/>");
                            }
                            doc_xml.push_str("</w:tc>");
                        }
                        doc_xml.push_str("</w:tr>");
                    }
                    doc_xml.push_str("</w:tbl>");
                }
            }
        }
    }

    doc_xml.push_str("</w:body></w:document>");

    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
</Types>"#;

    let pkg_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
</Relationships>"#;

    let mut doc_rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rIdStyles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    );
    for (rid, url) in &rels {
        doc_rels.push_str(&format!(
            r#"<Relationship Id="{rid}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="{}" TargetMode="External"/>"#,
            xml_escape(url)
        ));
    }
    doc_rels.push_str("</Relationships>");

    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults>
<w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:eastAsia="Yu Gothic" w:hAnsi="Calibri"/><w:sz w:val="21"/></w:rPr></w:rPrDefault>
</w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:pPr><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:b/><w:sz w:val="24"/></w:rPr></w:style>
</w:styles>"#;

    let title_str = xml_escape(title.unwrap_or(""));
    let core_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:title>{title_str}</dc:title>
</cp:coreProperties>"#
    );

    let mut writer = Writer::new();
    for (name, content) in [
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", pkg_rels.as_bytes()),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes()),
        ("word/document.xml", doc_xml.as_bytes()),
        ("word/styles.xml", styles_xml.as_bytes()),
        ("docProps/core.xml", core_xml.as_bytes()),
    ] {
        writer
            .add(name, EntryMeta::new(), content)
            .map_err(|e| Error::new(e.to_string()))?;
    }
    writer.finish().map_err(|e| Error::new(e.to_string()))
}

// =========================================================================
// 1. Plain Text (.txt) — DOCX-SEC-019
// =========================================================================

/// Export a `FlowDocument` as UTF-8 plain text (`.txt`).
pub fn export_txt(doc: &FlowDocument) -> String {
    let mut lines = Vec::new();
    for block in &doc.body {
        match block {
            Block::Paragraph(p) => {
                lines.push(paragraph_plain_text(p));
            }
            Block::Table(t) => {
                for row in &t.rows {
                    let cells: Vec<String> = row
                        .cells
                        .iter()
                        .map(|c| {
                            c.blocks
                                .iter()
                                .filter_map(|b| match b {
                                    Block::Paragraph(p) => Some(paragraph_plain_text(p)),
                                    _ => None,
                                })
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .collect();
                    lines.push(cells.join("\t"));
                }
            }
        }
    }
    lines.join("\n")
}

/// Export a `Presentation` (Word `.docx`) as UTF-8 plain text bytes.
pub fn export_docx_to_txt(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    let doc = presentation
        .flow
        .as_ref()
        .ok_or_else(|| Error::new("Word 文書データがありません"))?;
    Ok(export_txt(doc).into_bytes())
}

/// Decode text bytes (UTF-8, UTF-16LE/BE with BOM, or Shift_JIS fallback).
fn decode_text_bytes(data: &[u8]) -> String {
    if data.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = data[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if data.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = data[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data);
    if let Ok(s) = std::str::from_utf8(data) {
        return s.to_string();
    }
    let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(data);
    cow.into_owned()
}

/// Convert plain text (`.txt`) bytes into `.docx` package bytes.
pub fn import_txt_to_docx_bytes(data: &[u8]) -> Result<Vec<u8>, Error> {
    let text = decode_text_bytes(data);
    let mut blocks = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        let mut runs = Vec::new();
        let parts: Vec<&str> = line.split('\t').collect();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                runs.push(ConvRun {
                    is_tab: true,
                    ..Default::default()
                });
            }
            if !part.is_empty() {
                runs.push(ConvRun {
                    text: (*part).to_string(),
                    ..Default::default()
                });
            }
        }
        blocks.push(ConvBlock::Para(ConvPara {
            runs,
            ..Default::default()
        }));
    }
    build_docx_from_blocks(&blocks, None)
}

/// Open plain text (`.txt`) bytes as an editable Word document (`Opened`).
pub fn import_txt(data: &[u8]) -> Result<Opened, Error> {
    let docx_bytes = import_txt_to_docx_bytes(data)?;
    crate::open(docx_bytes)
}

// =========================================================================
// 2. Rich Text Format (.rtf) — DOCX-SEC-019
// =========================================================================

fn rtf_escape_str(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\line "),
            '\t' => out.push_str("\\tab "),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => {
                let mut buf = [0u16; 2];
                for &u in c.encode_utf16(&mut buf).iter() {
                    let signed = u as i16;
                    out.push_str(&format!("\\u{signed}?"));
                }
            }
        }
    }
    out
}

/// Export a `FlowDocument` as Rich Text Format (`.rtf`).
pub fn export_rtf(doc: &FlowDocument) -> String {
    let mut out = String::from(
        "{\\rtf1\\ansi\\ansicpg65001\\uc1\\deff0{\\fonttbl{\\f0 Calibri;}{\\f1 Yu Gothic;}}\n",
    );

    let write_para_runs = |out: &mut String, p: &FlowParagraph| {
        if let Some(lvl) = paragraph_heading_level(p) {
            out.push_str(&format!("\\s{lvl}\\outlinelevel{} ", lvl.saturating_sub(1)));
        }
        match p.align {
            FlowAlign::Left => out.push_str("\\ql "),
            FlowAlign::Center => out.push_str("\\qc "),
            FlowAlign::Right => out.push_str("\\qr "),
            FlowAlign::Justify | FlowAlign::Distribute => out.push_str("\\qj "),
        }
        for run in &p.runs {
            let txt = run_visible_text(run);
            if txt.is_empty() {
                continue;
            }
            out.push('{');
            if run.style.bold {
                out.push_str("\\b ");
            }
            if run.style.italic {
                out.push_str("\\i ");
            }
            if run.style.underline {
                out.push_str("\\ul ");
            }
            if run.style.strike || run.style.double_strike {
                out.push_str("\\strike ");
            }
            let half_pt = (run.style.size / 50).max(2);
            out.push_str(&format!("\\fs{half_pt} "));
            out.push_str(&rtf_escape_str(&txt));
            out.push('}');
        }
    };

    for block in &doc.body {
        match block {
            Block::Paragraph(p) => {
                out.push_str("\\pard");
                write_para_runs(&mut out, p);
                out.push_str("\\par\n");
            }
            Block::Table(t) => {
                for row in &t.rows {
                    out.push_str("\\trowd\\trgaph108");
                    for (idx, _) in row.cells.iter().enumerate() {
                        let right = (idx + 1) * 2880;
                        out.push_str(&format!("\\cellx{right}"));
                    }
                    out.push('\n');
                    for cell in &row.cells {
                        out.push_str("\\pard\\intbl ");
                        for (bi, b) in cell.blocks.iter().enumerate() {
                            if let Block::Paragraph(p) = b {
                                if bi > 0 {
                                    out.push_str("\\line ");
                                }
                                write_para_runs(&mut out, p);
                            }
                        }
                        out.push_str("\\cell\n");
                    }
                    out.push_str("\\row\n");
                }
            }
        }
    }

    out.push('}');
    out
}

/// Export a `Presentation` (Word `.docx`) as RTF bytes.
pub fn export_docx_to_rtf(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    let doc = presentation
        .flow
        .as_ref()
        .ok_or_else(|| Error::new("Word 文書データがありません"))?;
    Ok(export_rtf(doc).into_bytes())
}

#[derive(Clone, Debug)]
struct RtfState {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    size_half_pt: Option<u32>,
    align: FlowAlign,
    heading: Option<u8>,
    in_table: bool,
    skip_group: bool,
}

impl Default for RtfState {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            size_half_pt: None,
            align: FlowAlign::Left,
            heading: None,
            in_table: false,
            skip_group: false,
        }
    }
}

/// Convert RTF (`.rtf`) bytes into `.docx` package bytes.
pub fn import_rtf_to_docx_bytes(data: &[u8]) -> Result<Vec<u8>, Error> {
    let s = String::from_utf8_lossy(data);
    let bytes = s.as_bytes();
    let mut i = 0;

    let mut stack: Vec<RtfState> = Vec::new();
    let mut state = RtfState::default();

    let mut blocks: Vec<ConvBlock> = Vec::new();
    let mut cur_para = ConvPara::default();
    let mut cur_text = String::new();
    let mut cur_row: Vec<Vec<ConvPara>> = Vec::new();
    let mut cur_cell: Vec<ConvPara> = Vec::new();
    let mut table_rows: Vec<Vec<Vec<ConvPara>>> = Vec::new();
    let mut pending_surrogate: Option<u16> = None;

    let flush_run = |cur_text: &mut String, cur_para: &mut ConvPara, state: &RtfState| {
        if !cur_text.is_empty() {
            cur_para.runs.push(ConvRun {
                text: std::mem::take(cur_text),
                bold: state.bold,
                italic: state.italic,
                underline: state.underline,
                strike: state.strike,
                size_half_pt: state.size_half_pt,
                ..Default::default()
            });
        }
    };

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                flush_run(&mut cur_text, &mut cur_para, &state);
                stack.push(state.clone());
                i += 1;
                if bytes.get(i..i + 2) == Some(b"\\*") {
                    state.skip_group = true;
                    i += 2;
                }
            }
            b'}' => {
                flush_run(&mut cur_text, &mut cur_para, &state);
                if let Some(prev) = stack.pop() {
                    state = prev;
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                match bytes[i] {
                    b'\\' | b'{' | b'}' => {
                        if !state.skip_group {
                            cur_text.push(bytes[i] as char);
                        }
                        i += 1;
                    }
                    b'\'' => {
                        i += 1;
                        if i + 2 <= bytes.len() {
                            if let Ok(b) = u8::from_str_radix(&s[i..i + 2], 16) {
                                if !state.skip_group {
                                    cur_text.push(b as char);
                                }
                            }
                            i += 2;
                        }
                    }
                    b'\n' | b'\r' => {
                        i += 1;
                    }
                    _ if bytes[i].is_ascii_alphabetic() => {
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                            i += 1;
                        }
                        let word = &s[start..i];
                        let mut neg = false;
                        if i < bytes.len() && bytes[i] == b'-' {
                            neg = true;
                            i += 1;
                        }
                        let num_start = i;
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                        let param: Option<i32> = if i > num_start {
                            s[num_start..i].parse::<i32>().ok().map(|v| if neg { -v } else { v })
                        } else {
                            None
                        };
                        if i < bytes.len() && bytes[i] == b' ' {
                            i += 1;
                        }

                        if matches!(
                            word,
                            "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "header" | "footer"
                        ) {
                            state.skip_group = true;
                            continue;
                        }
                        if state.skip_group {
                            continue;
                        }

                        match word {
                            "u" => {
                                if let Some(n) = param {
                                    let u = (n as i16) as u16;
                                    // Skip replacement char (usually '?')
                                    if i < bytes.len() && bytes[i] == b'?' {
                                        i += 1;
                                    }
                                    if (0xD800..=0xDBFF).contains(&u) {
                                        pending_surrogate = Some(u);
                                    } else if (0xDC00..=0xDFFF).contains(&u) {
                                        if let Some(hi) = pending_surrogate.take() {
                                            if let Some(Ok(ch)) = char::decode_utf16([hi, u]).next() {
                                                cur_text.push(ch);
                                            }
                                        }
                                    } else if let Some(ch) = char::from_u32(u as u32) {
                                        cur_text.push(ch);
                                    }
                                }
                            }
                            "b" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                state.bold = param.unwrap_or(1) != 0;
                            }
                            "i" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                state.italic = param.unwrap_or(1) != 0;
                            }
                            "ul" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                state.underline = param.unwrap_or(1) != 0;
                            }
                            "ulnone" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                state.underline = false;
                            }
                            "strike" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                state.strike = param.unwrap_or(1) != 0;
                            }
                            "fs" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                state.size_half_pt = param.filter(|&v| v > 0).map(|v| v as u32);
                            }
                            "pard" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                state.align = FlowAlign::Left;
                                state.heading = None;
                                state.in_table = false;
                            }
                            "ql" => {
                                state.align = FlowAlign::Left;
                                cur_para.align = FlowAlign::Left;
                            }
                            "qc" => {
                                state.align = FlowAlign::Center;
                                cur_para.align = FlowAlign::Center;
                            }
                            "qr" => {
                                state.align = FlowAlign::Right;
                                cur_para.align = FlowAlign::Right;
                            }
                            "qj" => {
                                state.align = FlowAlign::Justify;
                                cur_para.align = FlowAlign::Justify;
                            }
                            "outlinelevel" => {
                                if let Some(lvl) = param {
                                    if (0..=5).contains(&lvl) {
                                        let h = (lvl + 1) as u8;
                                        state.heading = Some(h);
                                        cur_para.heading = Some(h);
                                    }
                                }
                            }
                            "intbl" | "trowd" => {
                                state.in_table = true;
                            }
                            "tab" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                cur_para.runs.push(ConvRun {
                                    is_tab: true,
                                    ..Default::default()
                                });
                            }
                            "line" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                cur_para.runs.push(ConvRun {
                                    is_line_break: true,
                                    ..Default::default()
                                });
                            }
                            "par" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                cur_para.align = state.align;
                                if cur_para.heading.is_none() {
                                    cur_para.heading = state.heading;
                                }
                                if !table_rows.is_empty() && !state.in_table {
                                    blocks.push(ConvBlock::Table(ConvTable {
                                        rows: std::mem::take(&mut table_rows),
                                    }));
                                }
                                if state.in_table {
                                    cur_cell.push(std::mem::take(&mut cur_para));
                                } else {
                                    blocks.push(ConvBlock::Para(std::mem::take(&mut cur_para)));
                                }
                            }
                            "cell" => {
                                flush_run(&mut cur_text, &mut cur_para, &state);
                                cur_para.align = state.align;
                                cur_cell.push(std::mem::take(&mut cur_para));
                                cur_row.push(std::mem::take(&mut cur_cell));
                            }
                            "row" => {
                                if !cur_cell.is_empty() || !cur_para.runs.is_empty() || !cur_text.is_empty() {
                                    flush_run(&mut cur_text, &mut cur_para, &state);
                                    cur_cell.push(std::mem::take(&mut cur_para));
                                    cur_row.push(std::mem::take(&mut cur_cell));
                                }
                                if !cur_row.is_empty() {
                                    table_rows.push(std::mem::take(&mut cur_row));
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            b'\r' | b'\n' => {
                i += 1;
            }
            _ => {
                let ch = s[i..].chars().next().unwrap();
                if !state.skip_group {
                    cur_text.push(ch);
                }
                i += ch.len_utf8();
            }
        }
    }

    flush_run(&mut cur_text, &mut cur_para, &state);
    if !table_rows.is_empty() {
        blocks.push(ConvBlock::Table(ConvTable {
            rows: std::mem::take(&mut table_rows),
        }));
    }
    if !cur_para.runs.is_empty() {
        cur_para.align = state.align;
        if cur_para.heading.is_none() {
            cur_para.heading = state.heading;
        }
        blocks.push(ConvBlock::Para(cur_para));
    }

    build_docx_from_blocks(&blocks, None)
}

/// Open RTF (`.rtf`) bytes as an editable Word document (`Opened`).
pub fn import_rtf(data: &[u8]) -> Result<Opened, Error> {
    let docx_bytes = import_rtf_to_docx_bytes(data)?;
    crate::open(docx_bytes)
}

// =========================================================================
// 3. Legacy Word 97-2003 (.doc) — DOCX-FILE-010, DOCX-SEC-019
// =========================================================================

fn doc_flow_to_conv_blocks(doc: &FlowDocument) -> Vec<ConvBlock> {
    let map_para = |p: &FlowParagraph| -> ConvPara {
        let mut runs = Vec::new();
        for r in &p.runs {
            if r.revision.as_ref().is_some_and(|rev| rev.kind.is_delete()) {
                continue;
            }
            match &r.kind {
                FlowRunKind::Tab | FlowRunKind::PositionalTab { .. } => runs.push(ConvRun {
                    is_tab: true,
                    ..Default::default()
                }),
                FlowRunKind::LineBreak => runs.push(ConvRun {
                    is_line_break: true,
                    ..Default::default()
                }),
                _ => {
                    let txt = run_visible_text(r);
                    if !txt.is_empty() {
                        runs.push(ConvRun {
                            text: txt,
                            bold: r.style.bold,
                            italic: r.style.italic,
                            underline: r.style.underline,
                            strike: r.style.strike || r.style.double_strike,
                            color: (r.style.color != Rgb::BLACK).then_some(r.style.color),
                            size_half_pt: Some((r.style.size / 50).max(2)),
                            link_url: match &r.link {
                                Some(Link::Url(u)) => Some(u.clone()),
                                _ => None,
                            },
                            ..Default::default()
                        });
                    }
                }
            }
        }
        ConvPara {
            heading: paragraph_heading_level(p),
            align: p.align,
            runs,
        }
    };

    let mut blocks = Vec::new();
    for block in &doc.body {
        match block {
            Block::Paragraph(p) => blocks.push(ConvBlock::Para(map_para(p))),
            Block::Table(t) => {
                let mut rows = Vec::new();
                for row in &t.rows {
                    let mut cells = Vec::new();
                    for cell in &row.cells {
                        let paras: Vec<ConvPara> = cell
                            .blocks
                            .iter()
                            .filter_map(|b| match b {
                                Block::Paragraph(p) => Some(map_para(p)),
                                _ => None,
                            })
                            .collect();
                        cells.push(paras);
                    }
                    rows.push(cells);
                }
                blocks.push(ConvBlock::Table(ConvTable { rows }));
            }
        }
    }
    blocks
}

/// Export a `FlowDocument` as a Word 97-2003 binary `.doc` file (MS-CFB OLE2 container
/// with standard FIB + UTF-16LE `WordDocument` stream, `1Table` stream, and rich `IbisDocRtf` stream).
pub fn export_doc(doc: &FlowDocument) -> Result<Vec<u8>, Error> {
    // Build the MS-DOC WordDocument stream:
    // FIB header (1024 bytes, fcMin = 0x0400) followed by UTF-16LE character stream.
    let mut text_stream_chars: Vec<u16> = Vec::new();
    for block in &doc.body {
        match block {
            Block::Paragraph(p) => {
                let txt = paragraph_plain_text(p);
                text_stream_chars.extend(txt.encode_utf16());
                text_stream_chars.push(0x000D); // CR paragraph mark in MS-DOC
            }
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        for (bi, b) in cell.blocks.iter().enumerate() {
                            if let Block::Paragraph(p) = b {
                                if bi > 0 {
                                    text_stream_chars.push(0x000B);
                                }
                                text_stream_chars.extend(paragraph_plain_text(p).encode_utf16());
                            }
                        }
                        text_stream_chars.push(0x0007); // MS-DOC cell mark
                    }
                    text_stream_chars.push(0x0007); // MS-DOC row mark
                }
            }
        }
    }

    let fc_min: u32 = 0x0400;
    let ccp_text: u32 = text_stream_chars.len() as u32;
    let fc_mac: u32 = fc_min + ccp_text * 2;
    let mut word_doc = vec![0u8; fc_min as usize];
    // wIdent = 0xA5EC (MS-DOC FibBase magic)
    word_doc[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
    // nFib = 0x00C1 (Word 97-2003)
    word_doc[2..4].copy_from_slice(&0x00C1u16.to_le_bytes());
    // lid = 0x0411 (ja-JP)
    word_doc[6..8].copy_from_slice(&0x0411u16.to_le_bytes());
    // flags at offset 0x0A: fExtChar (bit 12 = 0x1000, UTF-16 text indicator for our reader) + fWhichTblStm (bit 9 = 0x0200, 1Table)
    word_doc[10..12].copy_from_slice(&0x1200u16.to_le_bytes());
    // nFibBack = 0x00BF
    word_doc[12..14].copy_from_slice(&0x00BFu16.to_le_bytes());
    // fcMin (offset 0x18 = 24) and fcMac (offset 0x1C = 28)
    word_doc[24..28].copy_from_slice(&fc_min.to_le_bytes());
    word_doc[28..32].copy_from_slice(&fc_mac.to_le_bytes());
    // ccpText (offset 0x4C = 76)
    word_doc[76..80].copy_from_slice(&ccp_text.to_le_bytes());

    for u in &text_stream_chars {
        word_doc.extend_from_slice(&u.to_le_bytes());
    }

    let rtf_bytes = export_rtf(doc).into_bytes();
    let table_stream = vec![0u8; 64];

    let items = vec![
        CfbItem::stream("WordDocument", word_doc),
        CfbItem::stream("1Table", table_stream),
        CfbItem::stream("IbisDocRtf", rtf_bytes),
    ];
    Ok(cfb::write(&items))
}

/// Export a `Presentation` (Word `.docx`) as Word 97-2003 `.doc` bytes.
pub fn export_docx_to_doc(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    let doc = presentation
        .flow
        .as_ref()
        .ok_or_else(|| Error::new("Word 文書データがありません"))?;
    export_doc(doc)
}

/// Whether `data` is an old Word document (`.doc`): a compound file with a `WordDocument` stream (or the RTF stream this
/// program writes into the `.doc` it saves).
pub fn is_doc(data: &[u8]) -> bool {
    cfb::is_cfb(data) && (cfb::stream(data, "WordDocument").is_ok() || cfb::stream(data, "IbisDocRtf").is_ok())
}

/// Convert legacy `.doc` bytes into `.docx` package bytes.
pub fn import_doc_to_docx_bytes(data: &[u8]) -> Result<Vec<u8>, Error> {
    // 1. If it's an OLE2 Compound File Binary:
    if cfb::is_cfb(data) {
        // Check if rich RTF stream is stored inside the compound file:
        if let Ok(rtf_bytes) = cfb::stream(data, "IbisDocRtf") {
            if !rtf_bytes.is_empty() {
                return import_rtf_to_docx_bytes(&rtf_bytes);
            }
        }
        // A Word 97-2003 document with its formatting ([MS-DOC]); else its text alone, as below.
        if let Some(docx) = crate::msdoc::to_docx(data) {
            return Ok(docx);
        }
        // Otherwise parse MS-DOC `WordDocument` FIB + text stream:
        if let Ok(wd) = cfb::stream(data, "WordDocument") {
            if wd.len() >= 32 {
                let w_ident = u16::from_le_bytes([wd[0], wd[1]]);
                let flags = u16::from_le_bytes([wd[10], wd[11]]);
                let fc_min = u32::from_le_bytes([wd[24], wd[25], wd[26], wd[27]]) as usize;
                let fc_mac = u32::from_le_bytes([wd[28], wd[29], wd[30], wd[31]]) as usize;
                let ccp_text = if wd.len() >= 80 {
                    u32::from_le_bytes([wd[76], wd[77], wd[78], wd[79]]) as usize
                } else {
                    0
                };
                // A Word file keeps its text in the pieces of the piece table (the CLX of the table stream), in any order and
                // part 8-bit, part UTF-16: read it from there when the file has one.
                if w_ident == 0xA5EC {
                    if let Some(raw_text) = doc_text_from_pieces(data, &wd) {
                        return doc_raw_text_to_docx_bytes(&raw_text);
                    }
                }
                if w_ident == 0xA5EC && fc_min < wd.len() {
                    let end = fc_mac.min(wd.len()).max(fc_min);
                    let slice = &wd[fc_min..end];
                    let is_utf16 = (flags & 0x1000) != 0
                        || (ccp_text > 0 && slice.len() >= ccp_text * 2);
                    let raw_text = if is_utf16 {
                        let units: Vec<u16> = slice
                            .chunks_exact(2)
                            .take(if ccp_text > 0 { ccp_text } else { slice.len() / 2 })
                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                            .collect();
                        String::from_utf16_lossy(&units)
                    } else {
                        let take_len = if ccp_text > 0 { ccp_text.min(slice.len()) } else { slice.len() };
                        let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(&slice[..take_len]);
                        cow.into_owned()
                    };
                    return doc_raw_text_to_docx_bytes(&raw_text);
                }
            }
        }
    }

    // 2. If `.doc` is actually an RTF file with `.doc` extension:
    let trimmed = data.iter().copied().skip_while(|b| b.is_ascii_whitespace()).take(5).collect::<Vec<u8>>();
    if trimmed.starts_with(b"{\\rtf") {
        return import_rtf_to_docx_bytes(data);
    }

    // 3. If `.doc` is HTML with `.doc` extension:
    let head_str = String::from_utf8_lossy(&data[..data.len().min(256)]).to_ascii_lowercase();
    if head_str.contains("<html") || head_str.contains("<!doctype html") {
        return import_html_to_docx_bytes(data);
    }

    // 4. Fallback plain text / raw text recovery:
    import_txt_to_docx_bytes(data)
}

/// The main text of a Word 97-2003 file, from the piece table ([MS-DOC] 2.4.1 and 2.9.177): the FIB gives where the CLX is
/// in the table stream (`fcClx` and `lcbClx`, the 34th pair of `FibRgFcLcb97`, at 0x1A2) and which table stream (`0Table`
/// or `1Table`, the bit 0x0200 of the flags) holds it. Each piece is a run of characters at a file position: 8-bit Windows-1252
/// when its `fc` has the bit 30 set (then the position is `fc / 2`), UTF-16 otherwise. Only the main document's
/// `ccpText` characters are taken (footnotes, headers and the like follow it). `None` when the file has no such table.
fn doc_text_from_pieces(data: &[u8], wd: &[u8]) -> Option<String> {
    let u16_at = |b: &[u8], o: usize| b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]));
    let u32_at = |b: &[u8], o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]));
    let flags = u16_at(wd, 10)?;
    if flags & 0x0100 != 0 {
        return None; // encrypted
    }
    let fc_clx = u32_at(wd, 0x1A2)? as usize;
    let lcb_clx = u32_at(wd, 0x1A6)? as usize;
    if lcb_clx == 0 {
        return None;
    }
    let table = cfb::stream(data, if flags & 0x0200 != 0 { "1Table" } else { "0Table" }).ok()?;
    let clx = table.get(fc_clx..fc_clx.checked_add(lcb_clx)?)?;
    // Prc entries (0x01, a 16-bit size and the data) come first, then the Pcdt (0x02, a 32-bit size, the PlcPcd).
    let mut at = 0;
    while *clx.get(at)? == 0x01 {
        at += 3 + u16_at(clx, at + 1)? as usize;
    }
    if *clx.get(at)? != 0x02 {
        return None;
    }
    let plc_len = u32_at(clx, at + 1)? as usize;
    let plc = clx.get(at + 5..at + 5 + plc_len)?;
    // n + 1 character positions (4 bytes) and n piece descriptors (8 bytes).
    let n = plc_len.checked_sub(4)? / 12;
    let ccp_text = u32_at(wd, 76)? as usize;
    let mut text: Vec<u16> = Vec::new();
    for i in 0..n {
        let cp_start = u32_at(plc, i * 4)? as usize;
        let cp_end = (u32_at(plc, (i + 1) * 4)? as usize).min(ccp_text);
        if cp_end <= cp_start {
            continue;
        }
        let count = cp_end - cp_start;
        let fc = u32_at(plc, (n + 1) * 4 + i * 8 + 2)?;
        if fc & 0x4000_0000 != 0 {
            let start = ((fc & 0x3FFF_FFFF) / 2) as usize;
            let bytes = wd.get(start..start + count)?;
            let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
            text.extend(decoded.encode_utf16());
        } else {
            let start = fc as usize;
            let bytes = wd.get(start..start + count * 2)?;
            text.extend(bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])));
        }
    }
    (!text.is_empty()).then(|| strip_doc_fields(&String::from_utf16_lossy(&text)))
}

/// The text of a Word file without its field codes: a field is 0x13, its instruction, 0x14, its result and 0x15 (the
/// instruction is dropped, the result kept; a field with no result goes whole). Fields may nest.
fn strip_doc_fields(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // For each open field, whether its instruction is still being read.
    let mut open: Vec<bool> = Vec::new();
    for c in text.chars() {
        match c {
            '\x13' => open.push(true),
            '\x14' => {
                if let Some(last) = open.last_mut() {
                    *last = false;
                }
            }
            '\x15' => {
                open.pop();
            }
            _ if open.iter().any(|&in_instruction| in_instruction) => {}
            _ => out.push(c),
        }
    }
    out
}

fn doc_raw_text_to_docx_bytes(raw_text: &str) -> Result<Vec<u8>, Error> {
    let mut blocks = Vec::new();
    let mut table_rows: Vec<Vec<Vec<ConvPara>>> = Vec::new();
    let mut cur_row: Vec<Vec<ConvPara>> = Vec::new();

    let plain_para = |text: String| ConvPara { runs: vec![ConvRun { text, ..Default::default() }], ..Default::default() };
    for seg in raw_text.split('\r') {
        if seg.contains('\x07') {
            // Cells end with a cell mark (0x07), and a row with a mark of its own right after the last cell's: an empty piece
            // after a cell is the end of the row. Text after the last mark is an ordinary paragraph.
            let mut pieces: Vec<&str> = seg.split('\x07').collect();
            let trailing = pieces.pop().unwrap_or("");
            let mut after_cell = false;
            for piece in pieces {
                let clean: String = piece.chars().filter(|&c| c == '\t' || c == '\n' || !c.is_control()).collect();
                if clean.is_empty() && after_cell {
                    if !cur_row.is_empty() {
                        table_rows.push(std::mem::take(&mut cur_row));
                    }
                    after_cell = false;
                } else {
                    cur_row.push(vec![plain_para(clean)]);
                    after_cell = true;
                }
            }
            if !cur_row.is_empty() {
                table_rows.push(std::mem::take(&mut cur_row));
            }
            let clean: String = trailing.chars().filter(|&c| c == '\t' || !c.is_control()).collect();
            if !clean.is_empty() {
                if !table_rows.is_empty() {
                    blocks.push(ConvBlock::Table(ConvTable { rows: std::mem::take(&mut table_rows) }));
                }
                blocks.push(ConvBlock::Para(plain_para(clean)));
            }
            continue;
        }
        if !table_rows.is_empty() {
            blocks.push(ConvBlock::Table(ConvTable {
                rows: std::mem::take(&mut table_rows),
            }));
        }
        let clean: String = seg
            .chars()
            .filter(|&c| c == '\t' || !c.is_control())
            .collect();
        if !clean.is_empty() || blocks.is_empty() {
            blocks.push(ConvBlock::Para(ConvPara {
                runs: if clean.is_empty() {
                    Vec::new()
                } else {
                    vec![ConvRun {
                        text: clean,
                        ..Default::default()
                    }]
                },
                ..Default::default()
            }));
        }
    }
    if !table_rows.is_empty() {
        blocks.push(ConvBlock::Table(ConvTable { rows: table_rows }));
    }
    build_docx_from_blocks(&blocks, None)
}

/// Open legacy `.doc` bytes as an editable Word document (`Opened`).
pub fn import_doc(data: &[u8]) -> Result<Opened, Error> {
    let docx_bytes = import_doc_to_docx_bytes(data)?;
    crate::open(docx_bytes)
}

// =========================================================================
// 4. HTML (.html / .htm) — DOCX-FILE-012
// =========================================================================

/// Export a `FlowDocument` as a standalone HTML5 document (`.html`).
pub fn export_html(doc: &FlowDocument) -> String {
    let title = doc
        .properties
        .title
        .as_deref()
        .filter(|t| !t.is_empty())
        .unwrap_or("Document");
    let mut out = format!(
        "<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n<meta charset=\"UTF-8\">\n<title>{}</title>\n<style>body{{font-family:'Yu Gothic',Calibri,sans-serif;line-height:1.5;margin:2em;}}table{{border-collapse:collapse;width:100%;margin:1em 0;}}td,th{{border:1px solid #888;padding:6px 8px;}}</style>\n</head>\n<body>\n",
        xml_escape(title)
    );

    let render_para = |out: &mut String, p: &FlowParagraph| {
        let tag = match paragraph_heading_level(p) {
            Some(1) => "h1",
            Some(2) => "h2",
            Some(3) => "h3",
            Some(4) => "h4",
            Some(5) => "h5",
            Some(6) => "h6",
            _ => "p",
        };
        let align_attr = match p.align {
            FlowAlign::Left => "",
            FlowAlign::Center => " style=\"text-align:center\"",
            FlowAlign::Right => " style=\"text-align:right\"",
            FlowAlign::Justify | FlowAlign::Distribute => " style=\"text-align:justify\"",
        };
        out.push_str(&format!("<{tag}{align_attr}>"));
        for run in &p.runs {
            if run.revision.as_ref().is_some_and(|r| r.kind.is_delete()) {
                continue;
            }
            match &run.kind {
                FlowRunKind::LineBreak => {
                    out.push_str("<br>");
                    continue;
                }
                FlowRunKind::Tab | FlowRunKind::PositionalTab { .. } => {
                    out.push_str("&#9;");
                    continue;
                }
                _ => {}
            }
            let txt = run_visible_text(run);
            if txt.is_empty() {
                continue;
            }
            let mut chunk = xml_escape(&txt);
            if run.style.bold {
                chunk = format!("<strong>{chunk}</strong>");
            }
            if run.style.italic {
                chunk = format!("<em>{chunk}</em>");
            }
            if run.style.underline {
                chunk = format!("<u>{chunk}</u>");
            }
            if run.style.strike || run.style.double_strike {
                chunk = format!("<s>{chunk}</s>");
            }
            if run.style.color != Rgb::BLACK {
                chunk = format!(
                    "<span style=\"color:#{:06X}\">{chunk}</span>",
                    run.style.color.0 & 0xFF_FFFF
                );
            }
            if let Some(Link::Url(url)) = &run.link {
                chunk = format!("<a href=\"{}\">{chunk}</a>", xml_escape(url));
            }
            out.push_str(&chunk);
        }
        out.push_str(&format!("</{tag}>\n"));
    };

    for block in &doc.body {
        match block {
            Block::Paragraph(p) => render_para(&mut out, p),
            Block::Table(t) => {
                out.push_str("<table>\n");
                for row in &t.rows {
                    out.push_str("<tr>");
                    let cell_tag = if row.header { "th" } else { "td" };
                    for cell in &row.cells {
                        out.push_str(&format!("<{cell_tag}>"));
                        for b in &cell.blocks {
                            if let Block::Paragraph(p) = b {
                                render_para(&mut out, p);
                            }
                        }
                        out.push_str(&format!("</{cell_tag}>"));
                    }
                    out.push_str("</tr>\n");
                }
                out.push_str("</table>\n");
            }
        }
    }

    out.push_str("</body>\n</html>\n");
    out
}

/// Export a `Presentation` (Word `.docx`) as HTML bytes.
pub fn export_docx_to_html(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    let doc = presentation
        .flow
        .as_ref()
        .ok_or_else(|| Error::new("Word 文書データがありません"))?;
    Ok(export_html(doc).into_bytes())
}

fn extract_html_attr(tag_inner: &str, attr_name: &str) -> Option<String> {
    let lower = tag_inner.to_ascii_lowercase();
    let pat = format!("{attr_name}=");
    let pos = lower.find(&pat)?;
    let rest = &tag_inner[pos + pat.len()..].trim_start();
    if let Some(stripped) = rest.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(xml_unescape(&stripped[..end]))
    } else if let Some(stripped) = rest.strip_prefix('\'') {
        let end = stripped.find('\'')?;
        Some(xml_unescape(&stripped[..end]))
    } else {
        let end = rest.find(|c: char| c.is_ascii_whitespace() || c == '>').unwrap_or(rest.len());
        Some(xml_unescape(&rest[..end]))
    }
}

/// Convert HTML (`.html` / `.htm`) bytes into `.docx` package bytes.
pub fn import_html_to_docx_bytes(data: &[u8]) -> Result<Vec<u8>, Error> {
    let html = decode_text_bytes(data);
    let mut blocks: Vec<ConvBlock> = Vec::new();
    let mut cur_para = ConvPara::default();
    let mut in_para = false;
    let mut in_table = false;
    let mut in_cell = false;
    let mut table_rows: Vec<Vec<Vec<ConvPara>>> = Vec::new();
    let mut cur_row: Vec<Vec<ConvPara>> = Vec::new();
    let mut cur_cell: Vec<ConvPara> = Vec::new();
    let mut skip_depth: usize = 0;
    let mut doc_title: Option<String> = None;
    let mut in_title = false;
    let mut title_buf = String::new();

    let mut bold = false;
    let mut italic = false;
    let mut underline = false;
    let mut strike = false;
    let mut color_stack: Vec<Option<Rgb>> = Vec::new();
    let mut link_stack: Vec<Option<String>> = Vec::new();

    let finish_para = |cur_para: &mut ConvPara,
                       in_para: &mut bool,
                       in_cell: bool,
                       cur_cell: &mut Vec<ConvPara>,
                       blocks: &mut Vec<ConvBlock>| {
        if *in_para || !cur_para.runs.is_empty() {
            let p = std::mem::take(cur_para);
            if in_cell {
                cur_cell.push(p);
            } else {
                blocks.push(ConvBlock::Para(p));
            }
            *in_para = false;
        }
    };

    let mut pos = 0;
    while pos < html.len() {
        if let Some(lt_rel) = html[pos..].find('<') {
            let text_chunk = &html[pos..pos + lt_rel];
            if in_title {
                title_buf.push_str(&xml_unescape(text_chunk));
            } else if skip_depth == 0 {
                let decoded = xml_unescape(text_chunk);
                let normalized = decoded.replace(['\r', '\n'], " ");
                if !normalized.trim().is_empty() || (in_para && !normalized.is_empty()) {
                    cur_para.runs.push(ConvRun {
                        text: normalized,
                        bold,
                        italic,
                        underline,
                        strike,
                        color: color_stack.last().copied().flatten(),
                        link_url: link_stack.last().cloned().flatten(),
                        ..Default::default()
                    });
                }
            }
            pos += lt_rel;
            let Some(gt_rel) = html[pos..].find('>') else {
                break;
            };
            let raw_tag = &html[pos + 1..pos + gt_rel].trim();
            pos += gt_rel + 1;

            if raw_tag.starts_with('!') || raw_tag.starts_with('?') {
                continue;
            }
            let is_closing = raw_tag.starts_with('/');
            let tag_body = if is_closing { &raw_tag[1..].trim() } else { raw_tag };
            let tag_name = tag_body
                .split(|c: char| c.is_ascii_whitespace() || c == '/')
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();

            if matches!(tag_name.as_str(), "head" | "style" | "script") {
                if is_closing {
                    skip_depth = skip_depth.saturating_sub(1);
                } else if !tag_body.ends_with('/') {
                    skip_depth += 1;
                }
                continue;
            }
            if tag_name == "title" {
                if is_closing {
                    in_title = false;
                    let t = title_buf.trim().to_string();
                    if !t.is_empty() {
                        doc_title = Some(t);
                    }
                } else {
                    in_title = true;
                    title_buf.clear();
                }
                continue;
            }
            if skip_depth > 0 {
                continue;
            }

            match tag_name.as_str() {
                "p" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    if is_closing {
                        finish_para(&mut cur_para, &mut in_para, in_cell, &mut cur_cell, &mut blocks);
                    } else {
                        finish_para(&mut cur_para, &mut in_para, in_cell, &mut cur_cell, &mut blocks);
                        in_para = true;
                        if let Some(lvl_ch) = tag_name.strip_prefix('h') {
                            if let Ok(lvl) = lvl_ch.parse::<u8>() {
                                cur_para.heading = Some(lvl);
                            }
                        }
                        if let Some(style) = extract_html_attr(tag_body, "style") {
                            let sl = style.to_ascii_lowercase();
                            if sl.contains("text-align:center") || sl.contains("text-align: center") {
                                cur_para.align = FlowAlign::Center;
                            } else if sl.contains("text-align:right") || sl.contains("text-align: right") {
                                cur_para.align = FlowAlign::Right;
                            } else if sl.contains("text-align:justify") || sl.contains("text-align: justify") {
                                cur_para.align = FlowAlign::Justify;
                            }
                        }
                    }
                }
                "br" => {
                    cur_para.runs.push(ConvRun {
                        is_line_break: true,
                        ..Default::default()
                    });
                }
                "b" | "strong" => bold = !is_closing,
                "i" | "em" => italic = !is_closing,
                "u" => underline = !is_closing,
                "s" | "strike" | "del" => strike = !is_closing,
                "span" | "font" => {
                    if is_closing {
                        color_stack.pop();
                    } else {
                        let mut found_color = None;
                        if let Some(style) = extract_html_attr(tag_body, "style") {
                            if let Some(c_idx) = style.to_ascii_lowercase().find("color:") {
                                let after = style[c_idx + 6..].trim_start();
                                if let Some(hex) = after.strip_prefix('#') {
                                    if hex.len() >= 6 {
                                        if let Some(Ok(rgb)) = hex.get(..6).map(|h| u32::from_str_radix(h, 16)) {
                                            found_color = Some(Rgb(rgb));
                                        }
                                    }
                                }
                            }
                        }
                        color_stack.push(found_color);
                    }
                }
                "a" => {
                    if is_closing {
                        link_stack.pop();
                    } else {
                        link_stack.push(extract_html_attr(tag_body, "href"));
                    }
                }
                "table" => {
                    if is_closing {
                        finish_para(&mut cur_para, &mut in_para, in_cell, &mut cur_cell, &mut blocks);
                        if !cur_row.is_empty() {
                            table_rows.push(std::mem::take(&mut cur_row));
                        }
                        if !table_rows.is_empty() {
                            blocks.push(ConvBlock::Table(ConvTable {
                                rows: std::mem::take(&mut table_rows),
                            }));
                        }
                        in_table = false;
                        in_cell = false;
                    } else {
                        finish_para(&mut cur_para, &mut in_para, in_cell, &mut cur_cell, &mut blocks);
                        in_table = true;
                    }
                }
                "tr" => {
                    if is_closing && in_table {
                        if !cur_cell.is_empty() || !cur_para.runs.is_empty() {
                            finish_para(&mut cur_para, &mut in_para, true, &mut cur_cell, &mut blocks);
                            cur_row.push(std::mem::take(&mut cur_cell));
                        }
                        if !cur_row.is_empty() {
                            table_rows.push(std::mem::take(&mut cur_row));
                        }
                    }
                }
                "td" | "th" => {
                    if is_closing {
                        finish_para(&mut cur_para, &mut in_para, true, &mut cur_cell, &mut blocks);
                        if cur_cell.is_empty() {
                            cur_cell.push(ConvPara::default());
                        }
                        cur_row.push(std::mem::take(&mut cur_cell));
                        in_cell = false;
                    } else {
                        in_cell = true;
                    }
                }
                _ => {}
            }
        } else {
            let text_chunk = &html[pos..];
            if skip_depth == 0 {
                let decoded = xml_unescape(text_chunk);
                if !decoded.trim().is_empty() {
                    cur_para.runs.push(ConvRun {
                        text: decoded.trim().to_string(),
                        bold,
                        italic,
                        underline,
                        strike,
                        color: color_stack.last().copied().flatten(),
                        link_url: link_stack.last().cloned().flatten(),
                        ..Default::default()
                    });
                }
            }
            break;
        }
    }

    finish_para(&mut cur_para, &mut in_para, in_cell, &mut cur_cell, &mut blocks);
    build_docx_from_blocks(&blocks, doc_title.as_deref())
}

/// Open HTML (`.html` / `.htm`) bytes as an editable Word document (`Opened`).
pub fn import_html(data: &[u8]) -> Result<Opened, Error> {
    let docx_bytes = import_html_to_docx_bytes(data)?;
    crate::open(docx_bytes)
}

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
        auto_styles.push_str(&format!(
            r#"<style:style style:name="{p_style_name}" style:family="paragraph"><style:paragraph-properties fo:text-align="{fo_align}"/></style:style>"#
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

#[derive(Clone, Debug, Default)]
struct OdtStyleDef {
    align: Option<FlowAlign>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    color: Option<Rgb>,
    size_half_pt: Option<u32>,
}

/// Convert OpenDocument Text (`.odt`) bytes into `.docx` package bytes.
pub fn import_odt_to_docx_bytes(data: &[u8]) -> Result<Vec<u8>, Error> {
    let archive = Archive::new(data).map_err(|e| Error::new(format!("ODT アーカイブを開けません: {e}")))?;
    let content_bytes = archive
        .read("content.xml")
        .map_err(|e| Error::new(format!("ODT content.xml を読めません: {e}")))?
        .ok_or_else(|| Error::new("ODT に content.xml がありません"))?;
    let content_str = std::str::from_utf8(&content_bytes)
        .map_err(|e| Error::new(format!("ODT content.xml UTF-8 エラー: {e}")))?;
    let xml = roxmltree::Document::parse(content_str)
        .map_err(|e| Error::new(format!("ODT content.xml 解析エラー: {e}")))?;

    let mut styles: HashMap<String, OdtStyleDef> = HashMap::new();
    for node in xml.descendants().filter(|n| n.has_tag_name("style")) {
        let Some(name) = node.attribute(("urn:oasis:names:tc:opendocument:xmlns:style:1.0", "name")).or_else(|| node.attribute("name")) else {
            continue;
        };
        let mut def = OdtStyleDef::default();
        for child in node.children().filter(|c| c.is_element()) {
            match child.tag_name().name() {
                "paragraph-properties" => {
                    for attr in child.attributes() {
                        if attr.name().ends_with("text-align") {
                            def.align = Some(match attr.value() {
                                "center" => FlowAlign::Center,
                                "end" | "right" => FlowAlign::Right,
                                "justify" => FlowAlign::Justify,
                                _ => FlowAlign::Left,
                            });
                        }
                    }
                }
                "text-properties" => {
                    for attr in child.attributes() {
                        let aname = attr.name();
                        let aval = attr.value();
                        if aname.ends_with("font-weight") && aval == "bold" {
                            def.bold = true;
                        } else if aname.ends_with("font-style") && aval == "italic" {
                            def.italic = true;
                        } else if aname.ends_with("text-underline-style") && aval != "none" {
                            def.underline = true;
                        } else if aname.ends_with("text-line-through-style") && aval != "none" {
                            def.strike = true;
                        } else if aname.ends_with("color") && aval.starts_with('#') && aval.len() == 7 {
                            if let Some(Ok(rgb)) = aval.get(1..).map(|h| u32::from_str_radix(h, 16)) {
                                def.color = Some(Rgb(rgb));
                            }
                        } else if aname == "font-size" {
                            if let Some(pt_str) = aval.strip_suffix("pt") {
                                if let Ok(pt) = pt_str.parse::<f32>() {
                                    def.size_half_pt = Some((pt * 2.0).round() as u32);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        styles.insert(name.to_string(), def);
    }

    fn collect_odt_runs(
        node: roxmltree::Node<'_, '_>,
        styles: &HashMap<String, OdtStyleDef>,
        inherited: &OdtStyleDef,
        link_url: Option<&str>,
        runs: &mut Vec<ConvRun>,
    ) {
        for child in node.children() {
            if child.is_text() {
                if let Some(t) = child.text() {
                    if !t.is_empty() {
                        runs.push(ConvRun {
                            text: t.to_string(),
                            bold: inherited.bold,
                            italic: inherited.italic,
                            underline: inherited.underline,
                            strike: inherited.strike,
                            color: inherited.color,
                            size_half_pt: inherited.size_half_pt,
                            link_url: link_url.map(str::to_string),
                            ..Default::default()
                        });
                    }
                }
            } else if child.is_element() {
                match child.tag_name().name() {
                    "span" => {
                        let mut cur = inherited.clone();
                        if let Some(sname) = child
                            .attribute(("urn:oasis:names:tc:opendocument:xmlns:text:1.0", "style-name"))
                            .or_else(|| child.attribute("style-name"))
                        {
                            if let Some(sdef) = styles.get(sname) {
                                cur.bold |= sdef.bold;
                                cur.italic |= sdef.italic;
                                cur.underline |= sdef.underline;
                                cur.strike |= sdef.strike;
                                if sdef.color.is_some() {
                                    cur.color = sdef.color;
                                }
                                if sdef.size_half_pt.is_some() {
                                    cur.size_half_pt = sdef.size_half_pt;
                                }
                            }
                        }
                        collect_odt_runs(child, styles, &cur, link_url, runs);
                    }
                    "a" => {
                        let href = child
                            .attribute(("http://www.w3.org/1999/xlink", "href"))
                            .or_else(|| child.attribute("href"));
                        collect_odt_runs(child, styles, inherited, href.or(link_url), runs);
                    }
                    "tab" => runs.push(ConvRun {
                        is_tab: true,
                        ..Default::default()
                    }),
                    "line-break" => runs.push(ConvRun {
                        is_line_break: true,
                        ..Default::default()
                    }),
                    "s" => {
                        let count = child
                            .attribute(("urn:oasis:names:tc:opendocument:xmlns:text:1.0", "c"))
                            .or_else(|| child.attribute("c"))
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(1);
                        runs.push(ConvRun {
                            text: " ".repeat(count),
                            bold: inherited.bold,
                            italic: inherited.italic,
                            underline: inherited.underline,
                            strike: inherited.strike,
                            color: inherited.color,
                            size_half_pt: inherited.size_half_pt,
                            link_url: link_url.map(str::to_string),
                            ..Default::default()
                        });
                    }
                    _ => collect_odt_runs(child, styles, inherited, link_url, runs),
                }
            }
        }
    }

    let parse_odt_para = |p_node: roxmltree::Node<'_, '_>| -> ConvPara {
        let mut p = ConvPara::default();
        if p_node.tag_name().name() == "h" {
            let lvl = p_node
                .attribute(("urn:oasis:names:tc:opendocument:xmlns:text:1.0", "outline-level"))
                .or_else(|| p_node.attribute("outline-level"))
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(1)
                .clamp(1, 6);
            p.heading = Some(lvl);
        }
        let mut base_style = OdtStyleDef::default();
        if let Some(sname) = p_node
            .attribute(("urn:oasis:names:tc:opendocument:xmlns:text:1.0", "style-name"))
            .or_else(|| p_node.attribute("style-name"))
        {
            if let Some(sdef) = styles.get(sname) {
                if let Some(al) = sdef.align {
                    p.align = al;
                }
                base_style = sdef.clone();
            }
        }
        collect_odt_runs(p_node, &styles, &base_style, None, &mut p.runs);
        p
    };

    let mut blocks = Vec::new();
    if let Some(office_text) = xml.descendants().find(|n| n.tag_name().name() == "text" && n.parent().is_some_and(|p| p.tag_name().name() == "body")) {
        for child in office_text.children().filter(|c| c.is_element()) {
            match child.tag_name().name() {
                "p" | "h" => blocks.push(ConvBlock::Para(parse_odt_para(child))),
                "table" => {
                    let mut rows = Vec::new();
                    for r_node in child.children().filter(|c| c.tag_name().name() == "table-row") {
                        let mut cells = Vec::new();
                        for c_node in r_node.children().filter(|c| c.tag_name().name() == "table-cell") {
                            let paras: Vec<ConvPara> = c_node
                                .children()
                                .filter(|c| matches!(c.tag_name().name(), "p" | "h"))
                                .map(parse_odt_para)
                                .collect();
                            cells.push(paras);
                        }
                        if !cells.is_empty() {
                            rows.push(cells);
                        }
                    }
                    blocks.push(ConvBlock::Table(ConvTable { rows }));
                }
                _ => {}
            }
        }
    }

    let mut doc_title = None;
    if let Ok(Some(meta_bytes)) = archive.read("meta.xml") {
        if let Ok(meta_str) = std::str::from_utf8(&meta_bytes) {
            if let Ok(meta_xml) = roxmltree::Document::parse(meta_str) {
                if let Some(t_node) = meta_xml.descendants().find(|n| n.tag_name().name() == "title") {
                    if let Some(t) = t_node.text() {
                        if !t.is_empty() {
                            doc_title = Some(t.to_string());
                        }
                    }
                }
            }
        }
    }

    build_docx_from_blocks(&blocks, doc_title.as_deref())
}

/// Whether `data` is an OpenDocument text document or template (a zip whose `mimetype` says so).
pub fn is_odt(data: &[u8]) -> bool {
    if !data.starts_with(b"PK") {
        return false;
    }
    let Ok(archive) = crate::zip::Archive::new(data) else { return false };
    archive.read("mimetype").ok().flatten().is_some_and(|m| m.starts_with(b"application/vnd.oasis.opendocument.text"))
}

/// Open OpenDocument Text (`.odt`) bytes as an editable Word document (`Opened`).
pub fn import_odt(data: &[u8]) -> Result<Opened, Error> {
    let docx_bytes = import_odt_to_docx_bytes(data)?;
    crate::open(docx_bytes)
}

// =========================================================================
// 6. PDF Reflow (.pdf -> .docx) — DOCX-FILE-013
// =========================================================================

fn parse_pdf_literal_string(bytes: &[u8], start: usize) -> (String, usize) {
    let mut i = start;
    let mut depth = 1usize;
    let mut raw = Vec::new();
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'(' => {
                depth += 1;
                raw.push(b'(');
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth > 0 {
                    raw.push(b')');
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                match bytes[i] {
                    b'n' => {
                        raw.push(b'\n');
                        i += 1;
                    }
                    b'r' => {
                        raw.push(b'\r');
                        i += 1;
                    }
                    b't' => {
                        raw.push(b'\t');
                        i += 1;
                    }
                    b'(' | b')' | b'\\' => {
                        raw.push(bytes[i]);
                        i += 1;
                    }
                    b'\r' | b'\n' => {
                        i += 1;
                    }
                    c if (b'0'..=b'7').contains(&c) => {
                        let mut oct = (c - b'0') as u8;
                        i += 1;
                        for _ in 0..2 {
                            if i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                                oct = oct.wrapping_mul(8).wrapping_add(bytes[i] - b'0');
                                i += 1;
                            } else {
                                break;
                            }
                        }
                        raw.push(oct);
                    }
                    other => {
                        raw.push(other);
                        i += 1;
                    }
                }
            }
            b => {
                raw.push(b);
                i += 1;
            }
        }
    }
    let decoded = if raw.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(&raw).into_owned()
    };
    (decoded, i)
}

fn parse_pdf_hex_string(bytes: &[u8], start: usize, cmap: &HashMap<u16, String>) -> (String, usize) {
    let mut i = start;
    let mut hex_digits = Vec::new();
    while i < bytes.len() && bytes[i] != b'>' {
        let b = bytes[i];
        if b.is_ascii_hexdigit() {
            hex_digits.push(b);
        }
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'>' {
        i += 1;
    }
    if hex_digits.len() % 2 == 1 {
        hex_digits.push(b'0');
    }
    let raw: Vec<u8> = hex_digits
        .chunks_exact(2)
        .filter_map(|c| {
            let s = std::str::from_utf8(c).ok()?;
            u8::from_str_radix(s, 16).ok()
        })
        .collect();

    if raw.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return (String::from_utf16_lossy(&units), i);
    }
    if !cmap.is_empty() && raw.len() % 2 == 0 {
        let mut mapped = String::new();
        let mut all_found = true;
        for chunk in raw.chunks_exact(2) {
            let cid = u16::from_be_bytes([chunk[0], chunk[1]]);
            if let Some(u) = cmap.get(&cid) {
                mapped.push_str(u);
            } else {
                all_found = false;
                break;
            }
        }
        if all_found && !mapped.is_empty() {
            return (mapped, i);
        }
    }
    (String::from_utf8_lossy(&raw).into_owned(), i)
}

fn parse_tounicode_cmap(stream_str: &str, cmap: &mut HashMap<u16, String>) {
    // Parse `beginbfchar ... endbfchar`
    let mut pos = 0;
    while let Some(b_idx) = stream_str[pos..].find("beginbfchar") {
        let start = pos + b_idx + "beginbfchar".len();
        let end = stream_str[start..]
            .find("endbfchar")
            .map(|e| start + e)
            .unwrap_or(stream_str.len());
        let section = &stream_str[start..end];
        let mut tokens = Vec::new();
        let mut p = 0;
        let sb = section.as_bytes();
        while p < sb.len() {
            if sb[p] == b'<' {
                if let Some(gt) = section[p + 1..].find('>') {
                    tokens.push(&section[p + 1..p + 1 + gt]);
                    p += gt + 2;
                    continue;
                }
            }
            p += 1;
        }
        for pair in tokens.chunks_exact(2) {
            if let Ok(cid) = u16::from_str_radix(pair[0].trim(), 16) {
                let dst = pair[1].trim();
                if dst.len() % 4 == 0 {
                    let units: Vec<u16> = (0..dst.len() / 4)
                        .filter_map(|k| u16::from_str_radix(&dst[k * 4..k * 4 + 4], 16).ok())
                        .collect();
                    cmap.insert(cid, String::from_utf16_lossy(&units));
                }
            }
        }
        pos = end;
    }
}

/// Convert PDF (`.pdf`) bytes into editable `.docx` package bytes by extracting
/// and reflowing PDF text streams (including FlateDecode compressed streams and ToUnicode CMaps).
pub fn import_pdf_to_docx_bytes(data: &[u8]) -> Result<Vec<u8>, Error> {
    if !data.starts_with(b"%PDF") {
        return Err(Error::new("有効な PDF ファイルではありません"));
    }

    // Extract all stream buffers (decompressing FlateDecode where possible).
    let mut decoded_streams: Vec<Vec<u8>> = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let Some(s_rel) = data[pos..].windows(6).position(|w| w == b"stream") else {
            break;
        };
        let header_slice = &data[pos..pos + s_rel];
        let mut stream_start = pos + s_rel + 6;
        if data.get(stream_start) == Some(&b'\r') {
            stream_start += 1;
        }
        if data.get(stream_start) == Some(&b'\n') {
            stream_start += 1;
        }
        let Some(e_rel) = data[stream_start..].windows(9).position(|w| w == b"endstream") else {
            break;
        };
        let mut stream_end = stream_start + e_rel;
        while stream_end > stream_start && matches!(data[stream_end - 1], b'\r' | b'\n') {
            stream_end -= 1;
        }
        let raw_stream = &data[stream_start..stream_end];
        let header_str = String::from_utf8_lossy(header_slice);
        if header_str.contains("/FlateDecode") {
            if let Ok(inflated) = miniz_oxide::inflate::decompress_to_vec_zlib(raw_stream) {
                decoded_streams.push(inflated);
            } else if let Ok(inflated) = miniz_oxide::inflate::decompress_to_vec(raw_stream) {
                decoded_streams.push(inflated);
            } else {
                decoded_streams.push(raw_stream.to_vec());
            }
        } else {
            decoded_streams.push(raw_stream.to_vec());
        }
        pos = stream_start + e_rel + 9;
    }

    if decoded_streams.is_empty() {
        decoded_streams.push(data.to_vec());
    }

    // First pass: collect ToUnicode CMap mappings if any.
    let mut cmap: HashMap<u16, String> = HashMap::new();
    for st in &decoded_streams {
        let s = String::from_utf8_lossy(st);
        if s.contains("beginbfchar") {
            parse_tounicode_cmap(&s, &mut cmap);
        }
    }

    // Second pass: parse BT ... ET text blocks across content streams.
    let mut blocks: Vec<ConvBlock> = Vec::new();
    for st in &decoded_streams {
        let s = String::from_utf8_lossy(st);
        if s.contains("beginbfchar") || !s.contains("BT") {
            continue;
        }
        let bytes = st.as_slice();
        let mut i = 0;
        let mut cur_line = String::new();
        let mut cur_font_size: f32 = 11.0;
        let mut line_font_size: Option<f32> = None;
        let mut last_num: Option<f32> = None;

        let flush_line = |cur_line: &mut String,
                          line_font_size: &mut Option<f32>,
                          cur_font_size: f32,
                          blocks: &mut Vec<ConvBlock>| {
            let trimmed = cur_line.trim();
            if !trimmed.is_empty() {
                let font_size = line_font_size.unwrap_or(cur_font_size);
                let heading = if font_size >= 18.0 {
                    Some(1)
                } else if font_size >= 14.0 {
                    Some(2)
                } else {
                    None
                };
                blocks.push(ConvBlock::Para(ConvPara {
                    heading,
                    runs: vec![ConvRun {
                        text: trimmed.to_string(),
                        bold: heading.is_some(),
                        size_half_pt: Some((font_size * 2.0).round() as u32),
                        ..Default::default()
                    }],
                    ..Default::default()
                }));
            }
            cur_line.clear();
            *line_font_size = None;
        };

        while i < bytes.len() {
            match bytes[i] {
                b'(' => {
                    let (txt, next_i) = parse_pdf_literal_string(bytes, i + 1);
                    for (li, part) in txt.split('\n').enumerate() {
                        if li > 0 {
                            flush_line(&mut cur_line, &mut line_font_size, cur_font_size, &mut blocks);
                        }
                        if !part.is_empty() && line_font_size.is_none() {
                            line_font_size = Some(cur_font_size);
                        }
                        cur_line.push_str(part);
                    }
                    i = next_i;
                }
                b'<' if bytes.get(i + 1) != Some(&b'<') => {
                    let (txt, next_i) = parse_pdf_hex_string(bytes, i + 1, &cmap);
                    if !txt.is_empty() && line_font_size.is_none() {
                        line_font_size = Some(cur_font_size);
                    }
                    cur_line.push_str(&txt);
                    i = next_i;
                }
                b'T' => {
                    let next = bytes.get(i + 1).copied();
                    match next {
                        Some(b'd') | Some(b'D') | Some(b'*') | Some(b'm') => {
                            flush_line(&mut cur_line, &mut line_font_size, cur_font_size, &mut blocks);
                            i += 2;
                        }
                        Some(b'f') => {
                            if let Some(sz) = last_num.take() {
                                if sz > 0.0 {
                                    cur_font_size = sz;
                                }
                            }
                            i += 2;
                        }
                        _ => i += 1,
                    }
                }
                b'E' if bytes.get(i + 1) == Some(&b'T') => {
                    flush_line(&mut cur_line, &mut line_font_size, cur_font_size, &mut blocks);
                    i += 2;
                }
                c if c.is_ascii_digit() || c == b'-' || c == b'.' => {
                    let start = i;
                    i += 1;
                    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                        i += 1;
                    }
                    if let Ok(num_str) = std::str::from_utf8(&bytes[start..i]) {
                        if let Ok(v) = num_str.parse::<f32>() {
                            last_num = Some(v);
                        }
                    }
                }
                _ => i += 1,
            }
        }
        flush_line(&mut cur_line, &mut line_font_size, cur_font_size, &mut blocks);
    }

    build_docx_from_blocks(&blocks, None)
}

/// Open PDF (`.pdf`) bytes as an editable Word document (`Opened`).
pub fn import_pdf(data: &[u8]) -> Result<Opened, Error> {
    let docx_bytes = import_pdf_to_docx_bytes(data)?;
    crate::open(docx_bytes)
}

// =========================================================================
// Unified Dispatch
// =========================================================================

/// Export a `Presentation` (Word `.docx`) to the given target format.
pub fn export_docx_format(
    presentation: &Presentation,
    format: DocxConvertFormat,
) -> Result<Vec<u8>, Error> {
    match format {
        DocxConvertFormat::Txt => export_docx_to_txt(presentation),
        DocxConvertFormat::Rtf => export_docx_to_rtf(presentation),
        DocxConvertFormat::Doc => export_docx_to_doc(presentation),
        DocxConvertFormat::Html => export_docx_to_html(presentation),
        DocxConvertFormat::Odt => export_docx_to_odt(presentation),
        DocxConvertFormat::Pdf => Err(Error::new("PDF の書き出しは export_pdf を使用してください")),
    }
}

/// Import bytes of the given `DocxConvertFormat` into an editable Word document (`Opened`).
pub fn import_docx_format(data: &[u8], format: DocxConvertFormat) -> Result<Opened, Error> {
    match format {
        DocxConvertFormat::Txt => import_txt(data),
        DocxConvertFormat::Rtf => import_rtf(data),
        DocxConvertFormat::Doc => import_doc(data),
        DocxConvertFormat::Html => import_html(data),
        DocxConvertFormat::Odt => import_odt(data),
        DocxConvertFormat::Pdf => import_pdf(data),
    }
}
