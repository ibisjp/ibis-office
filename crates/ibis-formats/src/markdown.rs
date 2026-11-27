// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Rich Markdown (`.md`) native reader, writer, and `.docx` converter for `ibisOffice`.
//!
//! Supports:
//! - YAML Frontmatter (`--- ... ---`): document properties (`title`, `subtitle`, `author`, `keywords`, `description`, `company`),
//!   page setup (`size`/`paper`, `orientation`, `margins`, `columns`, `column_separator`, `direction`),
//!   typography (`font`, `east_asian_font`, `font_size`, `style_set`, `paragraph_spacing`),
//!   header & footer (`header`, `footer` with `{page}` and `{filename}` fields),
//!   `watermark`, `page_color`, `line_numbers`, and `track_changes`.
//! - Block elements:
//!   - ATX headings (`#` .. `######`) with optional attribute blocks `{#anchor align=center}`
//!   - Paragraphs with optional `{align=left|center|right|justify|distribute}`
//!   - Blockquotes (`> ...`) and GitHub Alerts (`> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`)
//!   - Bullet lists (`- `, `* `, `+ `) and ordered lists (`1. `) with nested levels (`0..8`)
//!   - Task lists (`- [ ] `, `- [x] `) mapped to `FlowRunKind::CheckBox`
//!   - Fenced code blocks (` ```lang ... ``` `) with monospace styling, shading, and border
//!   - Display math (`$$ ... $$` and ` ```math ` / ` ```latex `) via `ibis_doc_model::latex::parse_latex`
//!   - Native Office charts (` ```chart `) with `type`, `title`, `width`, `height`, and CSV or YAML series data
//!   - GFM Pipe tables (`| ... |`) with column alignment (`:---`, `:---:`, `---:`), header shading, and inline formatting
//!   - Page breaks (`\newpage`, `::: pagebreak`, `<!-- pagebreak -->`), column breaks (`\columnbreak`, `::: columnbreak`),
//!     horizontal rules (`---`, `***`), Table of Contents (`[[toc]]`, `::: toc`), and Footnote definitions (`[^id]: ...`)
//! - Inline elements:
//!   - `**bold**`, `*italic*`, `***bold italic***`, `~~strikethrough~~`, `++underline++`, `==highlight==`, `^superscript^`, `~subscript~`, `` `code` ``
//!   - Inline math `$...$`
//!   - Links `[text](url)` and `[text](#bookmark)`
//!   - Footnote references `[^id]`
//!   - Japanese Ruby `{漢字|かんじ}` and `｜漢字《かんじ》`
//!   - Tate-chu-yoko `[25]{.tcy}` and Emphasis marks `《《強調》》` / `[強調]{.em}`
//!   - CriticMarkup Tracked Changes & Comments: `{++insert++}`, `{--delete--}`, `{~~old~>new~~}`, `{>>author: comment<<}`
//!   - Inline Data URI images `![alt](data:image/png;base64,...){width=... height=...}`

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use ibis_doc_model::latex::parse_latex;
use ibis_doc_model::{
    Block, BorderStyle, CellDirection, Chart, ChartGroup, ChartText, ChartTitle, ChartType,
    Comment, DocProperties, EmphasisMark, Emu, EmuRect, EmuSize, Fill, FlowAlign, FlowBorder,
    FlowCell, FlowDocument, FlowParagraph, FlowPos, FlowRow, FlowRun, FlowRunKind, FlowRunStyle,
    FlowStyle, FlowStyles, FlowTable, Grouping, LineHeight, LineNumberRestart, LineNumbers, Link,
    ListRef, Media, MediaFormat, MediaId, Note, Number, PageSetup, ParaFormat, ParagraphSpacingPreset,
    PictureLook, Presentation, Revision, RevisionKind, Rgb, RowHeight, RubyAlign, RunFormat,
    Series, ShapeId, ShapeKind, StyleKind, StyleSetPreset, TableLook, TableStyleRef,
    VerticalMerge, VerticalPosition, Watermark,
};

use crate::save::pptx_new_chart::NewChartKind;
use crate::{Error, Opened, SaveOptions};

const EMU_PER_PT: i64 = 12_700;
const EMU_PER_INCH: i64 = 914_400;
const EMU_PER_MM: f64 = 36_000.0;
const DEFAULT_CHART_SIZE: EmuSize = EmuSize::new(5_486_400, 3_200_400);

/// Returns `true` when `data` looks like a UTF-8 Markdown document rather than a binary archive or plain CSV.
pub fn is_markdown(data: &[u8]) -> bool {
    if data.starts_with(b"PK\x03\x04") || data.starts_with(b"\xD0\xCF\x11\xE0") || data.starts_with(b"%PDF-") {
        return false;
    }
    if data.contains(&0) {
        return false;
    }
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return false;
    }

    // Check YAML frontmatter at top of file.
    if let Some(after_open) = trimmed.strip_prefix("---\n").or_else(|| trimmed.strip_prefix("---\r\n")) {
        if let Some((fm, _rest)) = split_frontmatter_body(after_open) {
            if fm.lines().any(|l| {
                let l = l.trim();
                !l.is_empty()
                    && !l.starts_with('#')
                    && l.split_once(':').is_some_and(|(k, _)| {
                        !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                    })
            }) {
                return true;
            }
        }
    }

    // Text of a table (a CSV or a TSV: the same number of commas, or of tabs, in each of its lines) is no Markdown for the markers a cell may start with: a bullet (`- note`, `+81-90-...`, `* x`), a
    // quotation or a number and a point. Only what a table does not have (headings, fences, pipes, emphasis, links) says Markdown then.
    let tabular = looks_tabular(trimmed);
    for line in trimmed.lines().take(200) {
        let l = line.trim_start();
        let weak = l.starts_with("- ") || l.starts_with("* ") || l.starts_with("+ ") || l.starts_with("> ") || l == ">" || is_ordered_list_line(l);
        if tabular && weak {
            continue;
        }
        if let Some(rest) = l.strip_prefix('#') {
            let hashes = 1 + rest.chars().take_while(|&c| c == '#').count();
            if hashes <= 6 && rest[hashes - 1..].starts_with(' ') {
                return true;
            }
        }
        if l.starts_with("```")
            || l.starts_with("> ")
            || l == ">"
            || l.starts_with("- [ ] ")
            || l.starts_with("- [x] ")
            || l.starts_with("- [X] ")
            || l.starts_with("- ")
            || l.starts_with("* ")
            || l.starts_with("+ ")
            || l.starts_with(":::")
            || l.starts_with("[^")
            || l == "\\newpage"
            || l == "\\columnbreak"
            || l == "[[toc]]"
            || l.starts_with("$$")
            || is_horizontal_rule(l)
            || is_ordered_list_line(l)
        {
            return true;
        }
        if l.starts_with('|') && l.ends_with('|') && l.len() >= 3 {
            return true;
        }
        if l.contains("**")
            || l.contains("](")
            || l.contains("~~")
            || l.contains("==")
            || l.contains("{++")
            || l.contains("{--")
            || l.contains("{~~")
            || l.contains("{>>")
            || l.contains("[^")
            || has_inline_code_pair(l)
        {
            return true;
        }
    }

    false
}

/// Whether the first lines have the same number (one or more) of commas, or of tabs: the lines of a CSV or a TSV.
fn looks_tabular(text: &str) -> bool {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).take(20).collect();
    if lines.len() < 2 {
        return false;
    }
    [',', '\t'].iter().any(|d| {
        let n = lines[0].matches(*d).count();
        n >= 1 && lines.iter().all(|l| l.matches(*d).count() == n)
    })
}

fn is_ordered_list_line(l: &str) -> bool {
    let digits = l.bytes().take_while(|b| b.is_ascii_digit()).count();
    (1..=4).contains(&digits) && l.as_bytes().get(digits) == Some(&b'.') && l.as_bytes().get(digits + 1) == Some(&b' ')
}

fn has_inline_code_pair(l: &str) -> bool {
    if let Some(first) = l.find('`') {
        let rest = &l[first + 1..];
        rest.contains('`')
    } else {
        false
    }
}

fn split_frontmatter_body(after_open: &str) -> Option<(&str, &str)> {
    let mut offset = 0;
    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" || trimmed == "..." {
            let fm = &after_open[..offset];
            let rest = &after_open[offset + line.len()..];
            return Some((fm, rest));
        }
        offset += line.len();
    }
    None
}

/// Read a Markdown document into a [`Presentation`] whose `flow` is the rich [`FlowDocument`].
pub fn read_markdown(data: &[u8]) -> Result<Presentation, Error> {
    Ok(open_markdown(data.to_vec())?.presentation)
}

/// Open a Markdown document with a `.docx`-backed [`crate::Source`] so it can be edited and saved
/// either back to `.md` or directly to `.docx` (preserving native charts, math, tables, footnotes, comments, and page setup).
pub fn open_markdown(data: Vec<u8>) -> Result<Opened, Error> {
    open_markdown_with_base_dir(data, None)
}

/// Open a Markdown document resolving relative image paths against `base_dir` (when provided).
pub fn open_markdown_with_base_dir(data: Vec<u8>, base_dir: Option<&std::path::Path>) -> Result<Opened, Error> {
    let text = std::str::from_utf8(&data).map_err(|_| Error::new("Markdown ファイルが UTF-8 ではありません"))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let mut opened = crate::save::open(crate::blank::blank_file(crate::Kind::Document)?)?;
    let baseline_pres = opened.presentation.clone();
    let mut doc = (**opened
        .presentation
        .flow
        .as_ref()
        .ok_or_else(|| Error::new("白紙文書を開けません"))?)
    .clone();

    let mut media_list: Vec<Media> = opened.presentation.media.clone();
    parse_markdown_into(
        text,
        &mut doc,
        &mut media_list,
        Some(&opened.source),
        Some(&baseline_pres),
        base_dir,
    )?;

    let page_size = doc.page.size;
    let document = Arc::new(doc);
    opened.presentation.media = media_list;
    opened.presentation.slide_size = page_size;
    if let Some(slide) = opened.presentation.slides.first_mut() {
        if let Some(shape) = slide.shapes.first_mut() {
            shape.frame = EmuRect::new(0, 0, page_size.width, page_size.height);
            shape.kind = ShapeKind::Page {
                document: document.clone(),
                index: 0,
            };
        }
    }
    opened.presentation.flow = Some(document);
    Ok(opened)
}

/// Parse a Markdown text fragment into [`Block`]s using `doc`'s styles/numbering (e.g. for clipboard paste).
pub fn parse_markdown_fragment_blocks(
    text: &str,
    doc: &mut FlowDocument,
    media: &mut Vec<Media>,
) -> Result<Vec<Block>, Error> {
    let mut ctx = ParseContext {
        next_id: doc.new_id().max(10),
        next_shape_id: doc.new_shape_id().max(10),
        next_media_id: media.iter().map(|m| m.id.0).max().unwrap_or(0) + 1,
        next_comment_id: doc.comments.iter().map(|c| c.id).max().unwrap_or(0) + 1,
        styles: (*doc.styles).clone(),
        bullet_num_id: None,
        ordered_num_id: None,
        footnote_ids: HashMap::new(),
        footnote_order: Vec::new(),
        comments: Vec::new(),
        media,
        source: None,
        baseline_pres: None,
        base_dir: None,
    };
    let mut footnote_defs: HashMap<String, String> = HashMap::new();
    let mut blocks = parse_blocks(text, &mut ctx, &mut footnote_defs)?;
    doc.styles = Arc::new(ctx.styles);
    resolve_all_blocks(&mut blocks, &doc.styles);
    Ok(blocks)
}

/// Export a slice of [`Block`]s (such as a selection) to Markdown without YAML frontmatter.
pub fn export_blocks_to_markdown(
    blocks: &[Block],
    doc: &FlowDocument,
    media: &[Media],
) -> String {
    let mut out = String::new();
    let comments_by_id: HashMap<u32, &Comment> = doc.comments.iter().map(|c| (c.id, c)).collect();
    let mut prev_was_list = false;
    for block in blocks {
        let is_list = matches!(block, Block::Paragraph(p) if p.list.is_some() || p.runs.first().is_some_and(|r| matches!(r.kind, FlowRunKind::CheckBox { .. })));
        if prev_was_list && !is_list && !out.ends_with("\n\n") {
            out.push('\n');
        }
        serialize_block(block, doc, media, &comments_by_id, &mut out);
        prev_was_list = is_list;
    }
    out.trim_end().to_string()
}

/// Convert Markdown bytes directly into a `.docx` file's bytes.
pub fn import_markdown_to_docx_bytes(data: &[u8]) -> Result<Vec<u8>, Error> {
    let opened = open_markdown(data.to_vec())?;
    opened
        .source
        .save(&opened.presentation, SaveOptions::default())
}

/// Export a `.docx` file's bytes to deterministic Rich Markdown.
pub fn export_docx_to_markdown(docx_bytes: &[u8]) -> Result<String, Error> {
    let pres = crate::read_docx(docx_bytes)?;
    export_presentation_to_markdown(&pres)
}

/// Export a [`Presentation`] (with `flow`) to deterministic Rich Markdown.
pub fn export_presentation_to_markdown(presentation: &Presentation) -> Result<String, Error> {
    let flow = presentation
        .flow
        .as_ref()
        .ok_or_else(|| Error::new("ワード文書ではありません"))?;
    Ok(export_markdown(flow, &presentation.media))
}

// ============================================================================
// Parser implementation
// ============================================================================

struct ParseContext<'a> {
    next_id: u32,
    next_shape_id: u64,
    next_media_id: u64,
    next_comment_id: u32,
    styles: FlowStyles,
    bullet_num_id: Option<u32>,
    ordered_num_id: Option<u32>,
    footnote_ids: HashMap<String, u32>,
    footnote_order: Vec<(String, u32)>,
    comments: Vec<Comment>,
    media: &'a mut Vec<Media>,
    source: Option<&'a crate::Source>,
    baseline_pres: Option<&'a Presentation>,
    base_dir: Option<&'a std::path::Path>,
}

impl<'a> ParseContext<'a> {
    fn fresh_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn fresh_shape_id(&mut self) -> ShapeId {
        let id = self.next_shape_id;
        self.next_shape_id += 1;
        ShapeId(id)
    }

    fn fresh_media_id(&mut self) -> MediaId {
        let id = self.next_media_id;
        self.next_media_id += 1;
        MediaId(id)
    }

    fn fresh_comment_id(&mut self) -> u32 {
        let id = self.next_comment_id;
        self.next_comment_id += 1;
        id
    }

    fn ensure_heading_style(&mut self, level: u8) -> String {
        let level = level.clamp(1, 6);
        let id = format!("Heading{level}");
        let name = format!("heading {level}");
        if let Some(existing) = self.styles.paragraph_style_named(&name) {
            return existing.to_string();
        }
        if self.styles.styles.contains_key(&id) {
            return id;
        }
        let size_pt = match level {
            1 => 20.0,
            2 => 16.0,
            3 => 13.5,
            4 => 12.0,
            5 => 11.0,
            _ => 10.5,
        };
        let before_pt = match level {
            1 => 14.0,
            2 => 12.0,
            3 => 10.0,
            _ => 6.0,
        };
        let after_pt = match level {
            1 | 2 => 6.0,
            _ => 4.0,
        };
        let size_hp = (size_pt * 100.0) as u32;
        let style = FlowStyle {
            name,
            kind: StyleKind::Paragraph,
            based_on: self.styles.default_paragraph.clone(),
            next: self.styles.default_paragraph.clone(),
            quick: true,
            ui_priority: Some(9),
            auto_update: false,
            conditional: Vec::new(),
            num_id: None,
            para: ParaFormat {
                before: Some((before_pt * EMU_PER_PT as f64) as Emu),
                after: Some((after_pt * EMU_PER_PT as f64) as Emu),
                keep_next: Some(true),
                keep_lines: Some(true),
                ..ParaFormat::default()
            },
            run: RunFormat {
                bold: Some(true),
                size: Some(size_hp),
                cs_size: Some(size_hp),
                ..RunFormat::default()
            },
        };
        self.styles.styles.insert(id.clone(), style);
        id
    }

    fn ensure_bullet_list(&mut self) -> u32 {
        if let Some(id) = self.bullet_num_id {
            return id;
        }
        let (numbering, num_id) = self.styles.numbering.with_new_list(true);
        self.styles.numbering = numbering;
        self.bullet_num_id = Some(num_id);
        num_id
    }

    fn ensure_ordered_list(&mut self) -> u32 {
        if let Some(id) = self.ordered_num_id {
            return id;
        }
        let (numbering, num_id) = self.styles.numbering.with_new_list(false);
        self.styles.numbering = numbering;
        self.ordered_num_id = Some(num_id);
        num_id
    }

    fn footnote_id_for(&mut self, label: &str) -> u32 {
        if let Some(&id) = self.footnote_ids.get(label) {
            return id;
        }
        let id = self.footnote_ids.len() as u32 + 1;
        self.footnote_ids.insert(label.to_string(), id);
        self.footnote_order.push((label.to_string(), id));
        id
    }
}

fn parse_markdown_into(
    text: &str,
    doc: &mut FlowDocument,
    media: &mut Vec<Media>,
    source: Option<&crate::Source>,
    baseline_pres: Option<&Presentation>,
    base_dir: Option<&std::path::Path>,
) -> Result<(), Error> {
    let mut body_text = text;
    let mut frontmatter_text = None;

    let trimmed_start = text.trim_start_matches(['\n', '\r']);
    if let Some(after_open) = trimmed_start
        .strip_prefix("---\n")
        .or_else(|| trimmed_start.strip_prefix("---\r\n"))
    {
        if let Some((fm, rest)) = split_frontmatter_body(after_open) {
            frontmatter_text = Some(fm);
            body_text = rest;
        }
    }

    let mut ctx = ParseContext {
        next_id: doc.new_id().max(10),
        next_shape_id: doc.new_shape_id().max(10),
        next_media_id: media.iter().map(|m| m.id.0).max().unwrap_or(0) + 1,
        next_comment_id: 0,
        styles: (*doc.styles).clone(),
        bullet_num_id: None,
        ordered_num_id: None,
        footnote_ids: HashMap::new(),
        footnote_order: Vec::new(),
        comments: Vec::new(),
        media,
        source,
        baseline_pres,
        base_dir,
    };

    if let Some(fm) = frontmatter_text {
        apply_frontmatter(fm, doc, &mut ctx);
    }

    let mut footnote_defs: HashMap<String, String> = HashMap::new();
    let blocks = parse_blocks(body_text, &mut ctx, &mut footnote_defs)?;

    let first_existing_para_id = doc.body.iter().find_map(|b| match b {
        Block::Paragraph(p) => Some(p.id),
        _ => None,
    });

    doc.styles = Arc::new(ctx.styles.clone());
    doc.body = if blocks.is_empty() {
        let mut p = FlowParagraph {
            id: ctx.fresh_id(),
            style: doc.styles.default_paragraph.clone(),
            ..FlowParagraph::default()
        };
        p.resolve(&doc.styles);
        vec![Block::Paragraph(p)]
    } else {
        blocks
    };
    if let Some(existing_id) = first_existing_para_id {
        if let Some(first_para) = doc.body.iter_mut().find_map(|b| match b {
            Block::Paragraph(p) => Some(p),
            _ => None,
        }) {
            first_para.id = existing_id;
        } else {
            let mut p = FlowParagraph {
                id: existing_id,
                style: doc.styles.default_paragraph.clone(),
                ..FlowParagraph::default()
            };
            p.resolve(&doc.styles);
            doc.body.push(Block::Paragraph(p));
        }
    }

    // Build footnotes in order of reference or definition.
    for (label, _def_text) in &footnote_defs {
        let _ = ctx.footnote_id_for(label);
    }
    let order = ctx.footnote_order.clone();
    let mut notes = Vec::new();
    for (label, note_id) in order {
        let content = footnote_defs
            .get(&label)
            .map(String::as_str)
            .unwrap_or(&label);
        let pid = ctx.fresh_id();
        let text_style = ctx
            .styles
            .paragraph_style_named("footnote text")
            .map(str::to_string)
            .or_else(|| ctx.styles.default_paragraph.clone());
        let mark_style = ctx
            .styles
            .styles
            .iter()
            .find(|(_, s)| s.kind == StyleKind::Character && s.name.eq_ignore_ascii_case("footnote reference"))
            .map(|(id, _)| id.clone());
        let superscript = RunFormat {
            position: Some(VerticalPosition::Superscript),
            ..RunFormat::default()
        };
        let mark_format = if mark_style.is_some() {
            RunFormat::default()
        } else {
            superscript
        };
        let mut p = FlowParagraph {
            id: pid,
            style: text_style,
            ..FlowParagraph::default()
        };
        p.runs.push(FlowRun {
            kind: FlowRunKind::NoteMark,
            style: FlowRunStyle::default(),
            format: mark_format,
            char_style: mark_style,
            link: None,
            revision: None,
            show_field_code: false,
            field_locked: false,
            control: None,
        });
        p.runs.push(FlowRun::new(
            FlowRunKind::Text(" ".into()),
            FlowRunStyle::default(),
        ));
        let inline_runs = parse_inlines(content, &mut ctx, &RunFormat::default());
        p.runs.extend(inline_runs);
        p.resolve(&doc.styles);
        p.normalize();
        notes.push(Note {
            id: note_id,
            blocks: vec![Block::Paragraph(p)],
            ..Default::default()
        });
    }
    doc.footnotes = notes;

    // Ensure watermark has a header part so saving to .docx succeeds.
    if doc.watermark.is_some() && doc.linked[0] {
        doc.linked[0] = false;
        if doc.header.is_empty() {
            let mut hp = FlowParagraph {
                id: ctx.fresh_id(),
                style: doc
                    .styles
                    .paragraph_style_named("header")
                    .map(str::to_string)
                    .or_else(|| doc.styles.default_paragraph.clone()),
                ..FlowParagraph::default()
            };
            hp.resolve(&doc.styles);
            doc.header.push(Block::Paragraph(hp));
        }
    }

    doc.comments = ctx.comments;

    // Resolve all paragraphs against final styles.
    doc.styles = Arc::new(ctx.styles);
    resolve_all_blocks(&mut doc.body, &doc.styles);
    resolve_all_blocks(&mut doc.header, &doc.styles);
    resolve_all_blocks(&mut doc.footer, &doc.styles);

    // If a `[[toc]]` placeholder was inserted, populate its entries from the document headings.
    populate_toc_blocks(doc);

    Ok(())
}

fn resolve_all_blocks(blocks: &mut [Block], styles: &FlowStyles) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                p.resolve(styles);
                p.normalize();
            }
            Block::Table(t) => {
                t.apply_style(styles);
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        resolve_all_blocks(&mut cell.blocks, styles);
                    }
                }
            }
        }
    }
}

fn populate_toc_blocks(doc: &mut FlowDocument) {
    // Collect headings in body.
    let mut headings: Vec<(u8, String, String)> = Vec::new();
    let mut bm_counter = 1usize;
    for block in &mut doc.body {
        if let Block::Paragraph(p) = block {
            let level = p
                .style
                .as_deref()
                .and_then(|s| s.strip_prefix("Heading").or_else(|| s.strip_prefix("heading ")))
                .and_then(|n| n.trim().parse::<u8>().ok());
            if let Some(lvl) = level.filter(|&l| (1..=3).contains(&l)) {
                let text = p.text().trim().to_string();
                if !text.is_empty() {
                    let bm = if let Some(existing) = p.bookmarks.first() {
                        existing.clone()
                    } else {
                        let name = format!("_TocMd{bm_counter}");
                        bm_counter += 1;
                        p.bookmarks.push(name.clone());
                        name
                    };
                    headings.push((lvl, text, bm));
                }
            }
        }
    }
    if headings.is_empty() {
        return;
    }

    let mut idx = 0;
    while idx < doc.body.len() {
        let is_toc = matches!(
            &doc.body[idx],
            Block::Paragraph(p) if p.field_start.as_deref().is_some_and(|f| f.trim_start().starts_with("TOC")) && p.runs.is_empty()
        );
        if is_toc {
            let mut entries = Vec::new();
            let mut next_id = doc.new_id();
            for (i, (lvl, title, bm)) in headings.iter().enumerate() {
                let mut p = FlowParagraph {
                    id: next_id,
                    style: doc.styles.default_paragraph.clone(),
                    direct: ParaFormat {
                        left: Some(((lvl.saturating_sub(1) as i64) * 12) * EMU_PER_PT),
                        ..ParaFormat::default()
                    },
                    field_start: (i == 0).then(|| "TOC \\o \"1-3\" \\h \\z \\u".to_string()),
                    field_end: i + 1 == headings.len(),
                    ..FlowParagraph::default()
                };
                next_id += 1;
                let mut run = FlowRun::new(FlowRunKind::Text(title.clone()), FlowRunStyle::default());
                run.link = Some(Link::Anchor(bm.clone()));
                p.runs.push(run);
                p.resolve(&doc.styles);
                entries.push(Block::Paragraph(p));
            }
            let count = entries.len();
            doc.body.splice(idx..idx + 1, entries);
            idx += count;
        } else {
            idx += 1;
        }
    }
}

// ============================================================================
// Frontmatter parsing
// ============================================================================

fn apply_frontmatter(fm: &str, doc: &mut FlowDocument, ctx: &mut ParseContext<'_>) {
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    let mut current_section: Option<String> = None;

    for raw_line in fm.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw_line.chars().take_while(|&c| c == ' ' || c == '\t').count();
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_ascii_lowercase().replace('-', "_");
            let val = unquote_yaml(v.trim());
            if indent == 0 {
                if val.is_empty() {
                    current_section = Some(key);
                } else {
                    current_section = None;
                    if let Some(inner) = val.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                        for part in inner.split(',') {
                            if let Some((ik, iv)) = part.split_once(':') {
                                map.insert(
                                    format!("{key}.{}", ik.trim().to_ascii_lowercase().replace('-', "_")),
                                    unquote_yaml(iv.trim()),
                                );
                            }
                        }
                    }
                    map.insert(key, val);
                }
            } else if let Some(sec) = &current_section {
                map.insert(format!("{sec}.{key}"), val);
            }
        }
    }

    // Document properties
    if let Some(v) = map.get("title") {
        doc.properties.title = Some(v.clone());
    }
    if let Some(v) = map.get("subtitle").or_else(|| map.get("subject")) {
        doc.properties.subject = Some(v.clone());
    }
    if let Some(v) = map.get("author").or_else(|| map.get("creator")) {
        doc.properties.creator = Some(v.clone());
    }
    if let Some(v) = map.get("keywords").or_else(|| map.get("tags")) {
        let cleaned = v.trim_start_matches('[').trim_end_matches(']');
        let joined = cleaned
            .split(',')
            .map(|s| unquote_yaml(s.trim()))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        doc.properties.keywords = (!joined.is_empty()).then_some(joined);
    }
    if let Some(v) = map.get("description").or_else(|| map.get("summary")) {
        doc.properties.description = Some(v.clone());
    }
    if let Some(v) = map.get("category") {
        doc.properties.category = Some(v.clone());
    }
    if let Some(v) = map.get("company") {
        doc.properties.company = Some(v.clone());
    }

    // Page size & orientation
    if let Some(size_name) = map
        .get("size")
        .or_else(|| map.get("paper"))
        .or_else(|| map.get("page.size"))
        .or_else(|| map.get("page.paper"))
    {
        if let Some((w, h)) = parse_paper_size(size_name) {
            doc.page.size = EmuSize::new(w, h);
        }
    }
    if let Some(orient) = map
        .get("orientation")
        .or_else(|| map.get("page.orientation"))
    {
        let (w, h) = (
            doc.page.size.width.min(doc.page.size.height),
            doc.page.size.width.max(doc.page.size.height),
        );
        match orient.to_ascii_lowercase().as_str() {
            "landscape" | "横" => doc.page.size = EmuSize::new(h, w),
            "portrait" | "縦" => doc.page.size = EmuSize::new(w, h),
            _ => {}
        }
    }

    // Margins
    if let Some(margin_preset) = map.get("margins").or_else(|| map.get("page.margins")) {
        match margin_preset.to_ascii_lowercase().as_str() {
            "narrow" | "狭い" => {
                let m = (12.7 * EMU_PER_MM).round() as Emu;
                doc.page.top = m;
                doc.page.bottom = m;
                doc.page.left = m;
                doc.page.right = m;
            }
            "wide" | "広い" => {
                doc.page.top = (25.4 * EMU_PER_MM).round() as Emu;
                doc.page.bottom = (25.4 * EMU_PER_MM).round() as Emu;
                doc.page.left = (50.8 * EMU_PER_MM).round() as Emu;
                doc.page.right = (50.8 * EMU_PER_MM).round() as Emu;
            }
            "normal" | "標準" => {
                doc.page.top = (35.0 * EMU_PER_MM).round() as Emu;
                doc.page.bottom = (30.0 * EMU_PER_MM).round() as Emu;
                doc.page.left = (30.0 * EMU_PER_MM).round() as Emu;
                doc.page.right = (30.0 * EMU_PER_MM).round() as Emu;
            }
            other => {
                if let Some(emu) = parse_length_emu(other) {
                    doc.page.top = emu;
                    doc.page.bottom = emu;
                    doc.page.left = emu;
                    doc.page.right = emu;
                }
            }
        }
    }
    for (keys, field) in [
        (["margins.top", "page.margins.top", "margin_top"], &mut doc.page.top),
        (["margins.bottom", "page.margins.bottom", "margin_bottom"], &mut doc.page.bottom),
        (["margins.left", "page.margins.left", "margin_left"], &mut doc.page.left),
        (["margins.right", "page.margins.right", "margin_right"], &mut doc.page.right),
    ] {
        for k in keys {
            if let Some(v) = map.get(k).and_then(|s| parse_length_emu(s)) {
                *field = v;
            }
        }
    }

    // Columns
    if let Some(cols) = map
        .get("columns")
        .or_else(|| map.get("page.columns"))
        .and_then(|v| v.parse::<u32>().ok())
    {
        doc.page.columns.count = cols.clamp(1, 12) as u8;
    }
    if let Some(sep) = map
        .get("column_separator")
        .or_else(|| map.get("page.column_separator"))
    {
        doc.page.columns.separator = parse_bool(sep);
    }

    // Text direction (horizontal / vertical)
    if let Some(dir) = map
        .get("direction")
        .or_else(|| map.get("text_direction"))
        .or_else(|| map.get("page.direction"))
    {
        match dir.to_ascii_lowercase().as_str() {
            "vertical" | "tbrl" | "縦書き" => doc.page.direction = CellDirection::TbRl,
            "horizontal" | "lrtb" | "横書き" => doc.page.direction = CellDirection::LrTb,
            _ => {}
        }
    }

    // Line numbers
    if let Some(ln) = map.get("line_numbers").or_else(|| map.get("page.line_numbers")) {
        if parse_bool(ln) || ln.eq_ignore_ascii_case("continuous") || ln.eq_ignore_ascii_case("page") {
            let restart = if ln.eq_ignore_ascii_case("continuous") {
                LineNumberRestart::Continuous
            } else {
                LineNumberRestart::NewPage
            };
            doc.page.line_numbers = Some(LineNumbers {
                start: 1,
                count_by: 1,
                distance: 0,
                restart,
            });
        }
    }

    // Typography & Presets
    if let Some(font) = map.get("font").or_else(|| map.get("font_family")) {
        ctx.styles.defaults_run.font = Some(font.clone());
        ctx.styles.defaults_run.h_ansi_font = Some(font.clone());
        ctx.styles.defaults_run.east_asian_font = Some(font.clone());
        if let Some(normal_id) = ctx.styles.default_paragraph.clone() {
            if let Some(st) = ctx.styles.styles.get_mut(&normal_id) {
                st.run.font = Some(font.clone());
                st.run.h_ansi_font = Some(font.clone());
                st.run.east_asian_font = Some(font.clone());
            }
        }
    }
    if let Some(ea_font) = map.get("east_asian_font") {
        ctx.styles.defaults_run.east_asian_font = Some(ea_font.clone());
        if let Some(normal_id) = ctx.styles.default_paragraph.clone() {
            if let Some(st) = ctx.styles.styles.get_mut(&normal_id) {
                st.run.east_asian_font = Some(ea_font.clone());
            }
        }
    }
    if let Some(fs) = map.get("font_size") {
        let pt = fs.trim_end_matches("pt").trim().parse::<f64>().ok();
        if let Some(pt) = pt.filter(|&p| p >= 4.0 && p <= 144.0) {
            let hp = (pt * 100.0).round() as u32;
            ctx.styles.defaults_run.size = Some(hp);
            ctx.styles.defaults_run.cs_size = Some(hp);
            if let Some(normal_id) = ctx.styles.default_paragraph.clone() {
                if let Some(st) = ctx.styles.styles.get_mut(&normal_id) {
                    st.run.size = Some(hp);
                    st.run.cs_size = Some(hp);
                }
            }
        }
    }
    if let Some(preset_name) = map.get("style_set") {
        let preset = match preset_name.to_ascii_lowercase().replace('-', "_").as_str() {
            "default" | "既定" => Some(StyleSetPreset::Default),
            "casual" | "カジュアル" => Some(StyleSetPreset::Casual),
            "centered" | "中央揃え" => Some(StyleSetPreset::Centered),
            "formal" | "フォーマル" => Some(StyleSetPreset::Formal),
            "elegant" | "エレガント" => Some(StyleSetPreset::Elegant),
            "modern" | "モダン" => Some(StyleSetPreset::Modern),
            "simple" | "シンプル" => Some(StyleSetPreset::Simple),
            "minimalist" | "ミニマリスト" => Some(StyleSetPreset::Minimalist),
            _ => None,
        };
        if let Some(p) = preset {
            p.apply(&mut ctx.styles);
            doc.style_set_preset = Some(p);
        }
    }
    if let Some(spacing_name) = map.get("paragraph_spacing") {
        let preset = match spacing_name.to_ascii_lowercase().replace('-', "_").as_str() {
            "default" | "既定" => Some(ParagraphSpacingPreset::Default),
            "no_space" | "no_paragraph_space" | "none" | "段落間にスペースを入れない" => {
                Some(ParagraphSpacingPreset::NoSpace)
            }
            "compact" | "コンパクト" => Some(ParagraphSpacingPreset::Compact),
            "tight" | "狭い" => Some(ParagraphSpacingPreset::Tight),
            "open" | "開放的" => Some(ParagraphSpacingPreset::Open),
            "relaxed" | "やや広い" => Some(ParagraphSpacingPreset::Relaxed),
            "double" | "2倍" | "広い" => Some(ParagraphSpacingPreset::Double),
            _ => None,
        };
        if let Some(p) = preset {
            let (before, after, line) = p.spacing();
            if let Some(normal_id) = ctx.styles.default_paragraph.clone() {
                if let Some(st) = ctx.styles.styles.get_mut(&normal_id) {
                    st.para.before = Some(before);
                    st.para.after = Some(after);
                    st.para.line = Some(line);
                }
            }
            doc.paragraph_spacing_preset = Some(p);
        }
    }

    // Header & Footer
    if let Some(h) = map.get("header") {
        if !h.is_empty() {
            let p = build_header_footer_paragraph(h, false, ctx);
            doc.header = vec![Block::Paragraph(p)];
            doc.linked[0] = false;
        }
    }
    if let Some(f) = map.get("footer") {
        if !f.is_empty() {
            let p = build_header_footer_paragraph(f, true, ctx);
            doc.footer = vec![Block::Paragraph(p)];
            doc.linked[1] = false;
        }
    }

    // Watermark
    if let Some(wm) = map.get("watermark") {
        if !wm.is_empty() {
            doc.watermark = Some(Watermark {
                text: wm.clone(),
                font: ctx
                    .styles
                    .defaults_run
                    .east_asian_font
                    .clone()
                    .unwrap_or_else(|| "游明朝".to_string()),
                color: Rgb(0xC0C0C0),
                semitransparent: true,
                rotation: -45,
                size: None,
            });
        }
    }

    // Page color
    if let Some(pc) = map.get("page_color").and_then(|s| parse_hex_rgb(s)) {
        doc.page_color = Some(pc);
    }

    // Track changes
    if let Some(tc) = map.get("track_changes") {
        doc.track_changes = parse_bool(tc);
    }
}

fn build_header_footer_paragraph(
    template: &str,
    footer: bool,
    ctx: &mut ParseContext<'_>,
) -> FlowParagraph {
    let style_name = if footer { "footer" } else { "header" };
    let style = ctx
        .styles
        .paragraph_style_named(style_name)
        .map(str::to_string)
        .or_else(|| ctx.styles.default_paragraph.clone());
    let mut p = FlowParagraph {
        id: ctx.fresh_id(),
        style,
        direct: ParaFormat {
            align: if footer && template.trim() == "{page}" {
                Some(FlowAlign::Center)
            } else {
                None
            },
            ..ParaFormat::default()
        },
        ..FlowParagraph::default()
    };

    let mut rest = template;
    while !rest.is_empty() {
        let next_page = rest.find("{page}");
        let next_file = rest.find("{filename}");
        match (next_page, next_file) {
            (Some(pi), Some(fi)) if fi < pi => {
                if fi > 0 {
                    p.runs.extend(parse_inlines(&rest[..fi], ctx, &RunFormat::default()));
                }
                p.runs.push(FlowRun::new(
                    FlowRunKind::FileName("document.md".into()),
                    FlowRunStyle::default(),
                ));
                rest = &rest[fi + "{filename}".len()..];
            }
            (Some(pi), _) => {
                if pi > 0 {
                    p.runs.extend(parse_inlines(&rest[..pi], ctx, &RunFormat::default()));
                }
                p.runs.push(FlowRun::new(FlowRunKind::PageNumber, FlowRunStyle::default()));
                rest = &rest[pi + "{page}".len()..];
            }
            (None, Some(fi)) => {
                if fi > 0 {
                    p.runs.extend(parse_inlines(&rest[..fi], ctx, &RunFormat::default()));
                }
                p.runs.push(FlowRun::new(
                    FlowRunKind::FileName("document.md".into()),
                    FlowRunStyle::default(),
                ));
                rest = &rest[fi + "{filename}".len()..];
            }
            (None, None) => {
                p.runs.extend(parse_inlines(rest, ctx, &RunFormat::default()));
                break;
            }
        }
    }
    p
}

fn unquote_yaml(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"') && s.len() >= 2)
        || (s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2)
    {
        s[1..s.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        s.to_string()
    }
}

fn parse_bool(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "true" | "yes" | "on" | "1"
    )
}

fn parse_paper_size(name: &str) -> Option<(Emu, Emu)> {
    let mm = |w: f64, h: f64| ((w * EMU_PER_MM).round() as Emu, (h * EMU_PER_MM).round() as Emu);
    match name.trim().to_ascii_lowercase().as_str() {
        "a3" => Some(mm(297.0, 420.0)),
        "a4" => Some((11_906 * 635, 16_838 * 635)),
        "a5" => Some(mm(148.0, 210.0)),
        "b4" => Some(mm(257.0, 364.0)),
        "b5" => Some(mm(182.0, 257.0)),
        "letter" => Some((8 * EMU_PER_INCH + EMU_PER_INCH / 2, 11 * EMU_PER_INCH)),
        "legal" => Some((8 * EMU_PER_INCH + EMU_PER_INCH / 2, 14 * EMU_PER_INCH)),
        _ => None,
    }
}

fn parse_length_emu(s: &str) -> Option<Emu> {
    let s = s.trim();
    if let Some(v) = s.strip_suffix("mm") {
        v.trim()
            .parse::<f64>()
            .ok()
            .map(|n| (n * EMU_PER_MM).round() as Emu)
    } else if let Some(v) = s.strip_suffix("cm") {
        v.trim()
            .parse::<f64>()
            .ok()
            .map(|n| (n * 10.0 * EMU_PER_MM).round() as Emu)
    } else if let Some(v) = s.strip_suffix("in") {
        v.trim()
            .parse::<f64>()
            .ok()
            .map(|n| (n * EMU_PER_INCH as f64).round() as Emu)
    } else if let Some(v) = s.strip_suffix("pt") {
        v.trim()
            .parse::<f64>()
            .ok()
            .map(|n| (n * EMU_PER_PT as f64).round() as Emu)
    } else if let Some(v) = s.strip_suffix("px") {
        v.trim()
            .parse::<f64>()
            .ok()
            .map(|n| (n * 9_525.0).round() as Emu)
    } else {
        s.parse::<f64>()
            .ok()
            .map(|n| (n * EMU_PER_MM).round() as Emu)
    }
}

fn parse_hex_rgb(s: &str) -> Option<Rgb> {
    let hex = s.trim().strip_prefix('#').unwrap_or(s.trim());
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        u32::from_str_radix(hex, 16).ok().map(Rgb)
    } else {
        None
    }
}

// ============================================================================
// Block parsing
// ============================================================================

fn parse_blocks(
    body_text: &str,
    ctx: &mut ParseContext<'_>,
    footnote_defs: &mut HashMap<String, String>,
) -> Result<Vec<Block>, Error> {
    let lines: Vec<&str> = body_text.lines().collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let raw = lines[i];
        let trimmed = raw.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Footnote definition: `[^id]: text...`
        if let Some(after_bracket) = trimmed.strip_prefix("[^") {
            if let Some((label, rest)) = after_bracket.split_once("]:") {
                if !label.is_empty() && !label.contains(' ') {
                    let mut def = rest.trim().to_string();
                    i += 1;
                    while i < lines.len() {
                        let next_raw = lines[i];
                        if next_raw.starts_with("  ") || next_raw.starts_with('\t') {
                            if !def.is_empty() {
                                def.push(' ');
                            }
                            def.push_str(next_raw.trim());
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    footnote_defs.insert(label.to_string(), def);
                    let _ = ctx.footnote_id_for(label);
                    continue;
                }
            }
        }

        if trimmed.ends_with(" -->")
            && (trimmed.starts_with("<!-- fixed-layout-page: ")
                || trimmed.starts_with("<!-- fixed-layout-diagnostic: "))
        {
            i += 1;
            continue;
        }

        // Page break & Column break & TOC directives
        if matches!(
            trimmed,
            "\\newpage" | "\\pagebreak" | "::: pagebreak" | "<!-- pagebreak -->"
        ) {
            let mut p = FlowParagraph {
                id: ctx.fresh_id(),
                style: ctx.styles.default_paragraph.clone(),
                runs: vec![FlowRun::new(FlowRunKind::PageBreak, FlowRunStyle::default())],
                ..FlowParagraph::default()
            };
            p.resolve(&ctx.styles);
            blocks.push(Block::Paragraph(p));
            i += 1;
            continue;
        }
        if matches!(
            trimmed,
            "\\columnbreak" | "::: columnbreak" | "<!-- columnbreak -->"
        ) {
            let mut p = FlowParagraph {
                id: ctx.fresh_id(),
                style: ctx.styles.default_paragraph.clone(),
                runs: vec![FlowRun::new(
                    FlowRunKind::ColumnBreak,
                    FlowRunStyle::default(),
                )],
                ..FlowParagraph::default()
            };
            p.resolve(&ctx.styles);
            blocks.push(Block::Paragraph(p));
            i += 1;
            continue;
        }
        if matches!(trimmed, "[[toc]]" | "[TOC]" | "::: toc") {
            let p = FlowParagraph {
                id: ctx.fresh_id(),
                style: ctx.styles.default_paragraph.clone(),
                field_start: Some("TOC \\o \"1-3\" \\h \\z \\u".to_string()),
                field_end: true,
                ..FlowParagraph::default()
            };
            blocks.push(Block::Paragraph(p));
            i += 1;
            continue;
        }

        // Horizontal rule: `---`, `***`, `___`
        if is_horizontal_rule(trimmed) {
            let border = FlowBorder {
                width: 635 * 6,
                color: Rgb(0xBFBFBF),
                style: BorderStyle::Single,
                space: 12_700,
                art: None,
            };
            let mut p = FlowParagraph {
                id: ctx.fresh_id(),
                style: ctx.styles.default_paragraph.clone(),
                direct: ParaFormat {
                    borders: [None, None, Some(Some(border)), None],
                    after: Some(6 * EMU_PER_PT),
                    ..ParaFormat::default()
                },
                ..FlowParagraph::default()
            };
            p.resolve(&ctx.styles);
            blocks.push(Block::Paragraph(p));
            i += 1;
            continue;
        }

        // Fenced code / math / chart block: ```lang ... ```
        if let Some(fence_info) = trimmed.strip_prefix("```").or_else(|| trimmed.strip_prefix("~~~")) {
            let fence_marker = if trimmed.starts_with("```") { "```" } else { "~~~" };
            let lang = fence_info.trim().to_ascii_lowercase();
            i += 1;
            let mut content_lines = Vec::new();
            while i < lines.len() {
                if lines[i].trim().starts_with(fence_marker) {
                    i += 1;
                    break;
                }
                content_lines.push(lines[i]);
                i += 1;
            }
            let content = content_lines.join("\n");
            match lang.split_whitespace().next().unwrap_or("") {
                "math" | "latex" | "tex" => {
                    blocks.push(build_display_math_block(&content, ctx));
                }
                "chart" => {
                    blocks.push(build_chart_block(&content, ctx)?);
                }
                "mermaid" => {
                    blocks.push(build_mermaid_block(&content, ctx)?);
                }
                "plantuml" | "puml" => {
                    let fence_lang = lang.split_whitespace().next().unwrap_or("plantuml");
                    blocks.push(build_plantuml_block(fence_lang, &content, ctx));
                }
                _ => {
                    blocks.push(build_code_block(&content, ctx));
                }
            }
            continue;
        }

        // Display math block: $$ ... $$
        if trimmed.starts_with("$$") {
            if trimmed.len() > 4 && trimmed.ends_with("$$") {
                let inner = trimmed[2..trimmed.len() - 2].trim();
                blocks.push(build_display_math_block(inner, ctx));
                i += 1;
                continue;
            }
            let mut math_lines = Vec::new();
            let first_after = trimmed.strip_prefix("$$").unwrap_or("").trim();
            if !first_after.is_empty() {
                math_lines.push(first_after.to_string());
            }
            i += 1;
            while i < lines.len() {
                let l = lines[i].trim();
                if let Some(before_close) = l.strip_suffix("$$") {
                    if !before_close.trim().is_empty() {
                        math_lines.push(before_close.trim().to_string());
                    }
                    i += 1;
                    break;
                }
                math_lines.push(lines[i].to_string());
                i += 1;
            }
            blocks.push(build_display_math_block(&math_lines.join("\n"), ctx));
            continue;
        }

        // ATX Heading: `#` .. `######`
        if let Some((level, heading_text)) = parse_atx_heading(trimmed) {
            let (clean_text, attrs) = strip_trailing_block_attrs(heading_text);
            let style_id = ctx.ensure_heading_style(level);
            let mut p = FlowParagraph {
                id: ctx.fresh_id(),
                style: Some(style_id),
                direct: ParaFormat {
                    align: attrs.align,
                    ..ParaFormat::default()
                },
                bookmarks: attrs.id.into_iter().collect(),
                runs: parse_inlines(clean_text, ctx, &RunFormat::default()),
                ..FlowParagraph::default()
            };
            p.resolve(&ctx.styles);
            p.normalize();
            blocks.push(Block::Paragraph(p));
            i += 1;
            continue;
        }

        // Pipe table: row followed by separator row `| --- | --- |`
        if is_table_row(trimmed) && i + 1 < lines.len() && is_table_separator(lines[i + 1].trim()) {
            let header_cells = split_table_row(trimmed);
            let alignments = parse_table_alignments(lines[i + 1].trim());
            i += 2;
            let mut body_rows = Vec::new();
            while i < lines.len() && is_table_row(lines[i].trim()) {
                body_rows.push(split_table_row(lines[i].trim()));
                i += 1;
            }
            blocks.push(build_table_block(header_cells, alignments, body_rows, ctx));
            continue;
        }

        // Blockquote or GitHub Alert: `> ...`
        if trimmed.starts_with('>') {
            let mut quote_lines = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let l = lines[i].trim_start();
                let stripped = l.strip_prefix("> ").or_else(|| l.strip_prefix('>')).unwrap_or("");
                quote_lines.push(stripped);
                i += 1;
            }
            blocks.extend(build_blockquote_blocks(&quote_lines, ctx));
            continue;
        }

        // Bullet / Ordered / Task List item
        if let Some(item) = parse_list_marker(raw) {
            let num_id = if item.ordered {
                ctx.ensure_ordered_list()
            } else {
                ctx.ensure_bullet_list()
            };
            let (clean_text, attrs) = strip_trailing_block_attrs(item.text);
            let mut runs = Vec::new();
            if let Some(checked) = item.task_checked {
                runs.push(FlowRun::new(
                    FlowRunKind::CheckBox {
                        checked,
                        checked_char: '☑',
                        unchecked_char: '☐',
                    },
                    FlowRunStyle::default(),
                ));
                runs.push(FlowRun::new(
                    FlowRunKind::Text(" ".into()),
                    FlowRunStyle::default(),
                ));
            }
            runs.extend(parse_inlines(clean_text, ctx, &RunFormat::default()));
            let mut p = FlowParagraph {
                id: ctx.fresh_id(),
                style: ctx.styles.default_paragraph.clone(),
                direct: ParaFormat {
                    align: attrs.align,
                    num_id: if item.task_checked.is_some() { None } else { Some(num_id) },
                    level: if item.task_checked.is_some() { None } else { Some(item.level as u32) },
                    ..ParaFormat::default()
                },
                bookmarks: attrs.id.into_iter().collect(),
                list: if item.task_checked.is_some() {
                    None
                } else {
                    Some(ListRef {
                        num_id,
                        level: item.level as u32,
                    })
                },
                runs,
                ..FlowParagraph::default()
            };
            if item.task_checked.is_some() && item.level > 0 {
                p.direct.left = Some((item.level as i64) * 18 * EMU_PER_PT);
            }
            p.resolve(&ctx.styles);
            p.normalize();
            blocks.push(Block::Paragraph(p));
            i += 1;
            continue;
        }

        // Normal paragraph (accumulate consecutive non-blank, non-special lines)
        let mut para_lines: Vec<&str> = Vec::new();
        while i < lines.len() {
            let l_raw = lines[i];
            let l_trim = l_raw.trim();
            if l_trim.is_empty()
                || l_trim.starts_with("```")
                || l_trim.starts_with("~~~")
                || l_trim.starts_with("$$")
                || l_trim.starts_with('>')
                || l_trim.starts_with(":::")
                || l_trim == "\\newpage"
                || l_trim == "\\pagebreak"
                || l_trim == "\\columnbreak"
                || l_trim == "[[toc]]"
                || l_trim == "[TOC]"
                || is_horizontal_rule(l_trim)
                || parse_atx_heading(l_trim).is_some()
                || parse_list_marker(l_raw).is_some()
                || (is_table_row(l_trim)
                    && i + 1 < lines.len()
                    && is_table_separator(lines[i + 1].trim()))
                || (l_trim.starts_with("[^") && l_trim.contains("]:"))
            {
                break;
            }
            para_lines.push(l_raw);
            i += 1;
        }
        // A line that starts with a marker no branch above handles (e.g. an unknown `::: name` line) made no progress here and the loop never ended;
        // it is a paragraph line.
        if para_lines.is_empty() && i < lines.len() {
            para_lines.push(lines[i]);
            i += 1;
        }

        if !para_lines.is_empty() {
            let mut runs = Vec::new();
            let mut para_attrs = BlockAttrs::default();
            for (li, pline) in para_lines.iter().enumerate() {
                let hard_break = pline.ends_with("  ") || pline.ends_with('\\');
                let mut content = pline.trim_end();
                if content.ends_with('\\') {
                    content = &content[..content.len() - 1];
                }
                let content = content.trim();
                let content = if li + 1 == para_lines.len() {
                    let (c, a) = strip_trailing_block_attrs(content);
                    para_attrs = a;
                    c
                } else {
                    content
                };
                if li > 0 && !runs.is_empty() {
                    let prev_hard = para_lines[li - 1].ends_with("  ")
                        || para_lines[li - 1].trim_end().ends_with('\\');
                    if prev_hard {
                        runs.push(FlowRun::new(
                            FlowRunKind::LineBreak,
                            FlowRunStyle::default(),
                        ));
                    } else {
                        runs.push(FlowRun::new(
                            FlowRunKind::Text(" ".into()),
                            FlowRunStyle::default(),
                        ));
                    }
                }
                let _ = hard_break;
                runs.extend(parse_inlines(content, ctx, &RunFormat::default()));
            }
            let mut p = FlowParagraph {
                id: ctx.fresh_id(),
                style: ctx.styles.default_paragraph.clone(),
                direct: ParaFormat {
                    align: para_attrs.align,
                    ..ParaFormat::default()
                },
                bookmarks: para_attrs.id.into_iter().collect(),
                runs,
                ..FlowParagraph::default()
            };
            p.resolve(&ctx.styles);
            p.normalize();
            blocks.push(Block::Paragraph(p));
        }
    }

    Ok(blocks)
}

#[derive(Default)]
struct BlockAttrs {
    id: Option<String>,
    align: Option<FlowAlign>,
}

fn strip_trailing_block_attrs(text: &str) -> (&str, BlockAttrs) {
    let trimmed = text.trim_end();
    if let Some(open) = trimmed.rfind('{') {
        if trimmed.ends_with('}') && open > 0 {
            // Do not mistake CriticMarkup `{++...++}` or ruby `{漢字|かな}` for block attributes.
            let inner = &trimmed[open + 1..trimmed.len() - 1];
            if !inner.contains('|')
                && !inner.starts_with('+')
                && !inner.starts_with('-')
                && !inner.starts_with('~')
                && !inner.starts_with('>')
                && !inner.starts_with('=')
            {
                let mut attrs = BlockAttrs::default();
                let mut recognized = false;
                for token in inner.split_whitespace() {
                    if let Some(id) = token.strip_prefix('#') {
                        if !id.is_empty() {
                            attrs.id = Some(id.to_string());
                            recognized = true;
                        }
                    } else if let Some(al) = token.strip_prefix("align=").or_else(|| token.strip_prefix("text-align=")) {
                        attrs.align = match al.trim_matches('"').to_ascii_lowercase().as_str() {
                            "center" => Some(FlowAlign::Center),
                            "right" => Some(FlowAlign::Right),
                            "justify" | "both" => Some(FlowAlign::Justify),
                            "distribute" => Some(FlowAlign::Distribute),
                            "left" => Some(FlowAlign::Left),
                            _ => None,
                        };
                        if attrs.align.is_some() {
                            recognized = true;
                        }
                    } else if matches!(token, ".center" | ".right" | ".justify" | ".distribute") {
                        attrs.align = match token {
                            ".center" => Some(FlowAlign::Center),
                            ".right" => Some(FlowAlign::Right),
                            ".justify" => Some(FlowAlign::Justify),
                            ".distribute" => Some(FlowAlign::Distribute),
                            _ => None,
                        };
                        recognized = true;
                    }
                }
                if recognized {
                    return (trimmed[..open].trim_end(), attrs);
                }
            }
        }
    }
    (trimmed, BlockAttrs::default())
}

fn is_horizontal_rule(s: &str) -> bool {
    if s.len() < 3 {
        return false;
    }
    let chars: Vec<char> = s.chars().filter(|&c| c != ' ').collect();
    if chars.len() < 3 {
        return false;
    }
    let first = chars[0];
    matches!(first, '-' | '*' | '_') && chars.iter().all(|&c| c == first)
}

fn parse_atx_heading(s: &str) -> Option<(u8, &str)> {
    let hashes = s.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) {
        let rest = &s[hashes..];
        if rest.is_empty() {
            return Some((hashes as u8, ""));
        }
        if let Some(stripped) = rest.strip_prefix(' ') {
            let trimmed = stripped.trim().trim_end_matches('#').trim();
            return Some((hashes as u8, trimmed));
        }
    }
    None
}

struct ListMarker<'a> {
    ordered: bool,
    level: u8,
    task_checked: Option<bool>,
    text: &'a str,
}

fn parse_list_marker(raw_line: &str) -> Option<ListMarker<'_>> {
    let mut indent_spaces = 0usize;
    let mut byte_idx = 0usize;
    for ch in raw_line.chars() {
        match ch {
            ' ' => {
                indent_spaces += 1;
                byte_idx += 1;
            }
            '\t' => {
                indent_spaces += 2;
                byte_idx += 1;
            }
            _ => break,
        }
    }
    let level = ((indent_spaces / 2) as u8).min(8);
    let rest = &raw_line[byte_idx..];

    // Unordered: `- `, `* `, `+ `
    if let Some(after_bullet) = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("+ "))
    {
        if let Some(task_rest) = after_bullet.strip_prefix("[ ] ") {
            return Some(ListMarker {
                ordered: false,
                level,
                task_checked: Some(false),
                text: task_rest,
            });
        }
        if let Some(task_rest) = after_bullet
            .strip_prefix("[x] ")
            .or_else(|| after_bullet.strip_prefix("[X] "))
        {
            return Some(ListMarker {
                ordered: false,
                level,
                task_checked: Some(true),
                text: task_rest,
            });
        }
        return Some(ListMarker {
            ordered: false,
            level,
            task_checked: None,
            text: after_bullet,
        });
    }

    // Ordered: `1. `, `2. `, etc.
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        let after_digits = &rest[digits..];
        if let Some(after_dot) = after_digits
            .strip_prefix(". ")
            .or_else(|| after_digits.strip_prefix(") "))
        {
            return Some(ListMarker {
                ordered: true,
                level,
                task_checked: None,
                text: after_dot,
            });
        }
    }

    None
}

fn build_code_block(code: &str, ctx: &mut ParseContext<'_>) -> Block {
    let border = FlowBorder {
        width: 635 * 4,
        color: Rgb(0xD9D9D9),
        style: BorderStyle::Single,
        space: 4 * EMU_PER_PT,
        art: None,
    };
    let mono_format = RunFormat {
        font: Some("Consolas".to_string()),
        h_ansi_font: Some("Consolas".to_string()),
        east_asian_font: Some("ＭＳ ゴシック".to_string()),
        size: Some(950),
        cs_size: Some(950),
        no_proof: Some(true),
        ..RunFormat::default()
    };
    let mut runs = Vec::new();
    for (idx, line) in code.split('\n').enumerate() {
        if idx > 0 {
            runs.push(FlowRun {
                kind: FlowRunKind::LineBreak,
                style: FlowRunStyle::default(),
                format: mono_format.clone(),
                char_style: None,
                link: None,
                revision: None,
                show_field_code: false,
                field_locked: false,
                control: None,
            });
        }
        let line_clean = line.strip_suffix('\r').unwrap_or(line);
        if !line_clean.is_empty() {
            runs.push(FlowRun {
                kind: FlowRunKind::Text(line_clean.to_string()),
                style: FlowRunStyle::default(),
                format: mono_format.clone(),
                char_style: None,
                link: None,
                revision: None,
                show_field_code: false,
                field_locked: false,
                control: None,
            });
        }
    }
    let mut p = FlowParagraph {
        id: ctx.fresh_id(),
        style: ctx.styles.default_paragraph.clone(),
        direct: ParaFormat {
            shading: Some(Some(Rgb(0xF6F8FA))),
            borders: [
                Some(Some(border)),
                Some(Some(border)),
                Some(Some(border)),
                Some(Some(border)),
            ],
            left: Some(8 * EMU_PER_PT),
            right: Some(8 * EMU_PER_PT),
            before: Some(6 * EMU_PER_PT),
            after: Some(6 * EMU_PER_PT),
            line: Some(LineHeight::Auto(276)),
            ..ParaFormat::default()
        },
        runs,
        ..FlowParagraph::default()
    };
    p.resolve(&ctx.styles);
    Block::Paragraph(p)
}

fn build_display_math_block(latex: &str, ctx: &mut ParseContext<'_>) -> Block {
    let math = parse_latex(latex.trim());
    let mut p = FlowParagraph {
        id: ctx.fresh_id(),
        style: ctx.styles.default_paragraph.clone(),
        direct: ParaFormat {
            align: Some(FlowAlign::Center),
            before: Some(6 * EMU_PER_PT),
            after: Some(6 * EMU_PER_PT),
            ..ParaFormat::default()
        },
        runs: vec![FlowRun::new(
            FlowRunKind::Math {
                math,
                display: true,
            },
            FlowRunStyle::default(),
        )],
        ..FlowParagraph::default()
    };
    p.resolve(&ctx.styles);
    Block::Paragraph(p)
}

fn build_blockquote_blocks(lines: &[&str], ctx: &mut ParseContext<'_>) -> Vec<Block> {
    let first = lines.first().map(|s| s.trim()).unwrap_or("");
    let alert = match first {
        "[!NOTE]" => Some(("NOTE", Rgb(0x0969DA), Rgb(0xEDF5FF))),
        "[!TIP]" => Some(("TIP", Rgb(0x1A7F37), Rgb(0xEEFAF2))),
        "[!IMPORTANT]" => Some(("IMPORTANT", Rgb(0x8250DF), Rgb(0xF6F0FF))),
        "[!WARNING]" => Some(("WARNING", Rgb(0x9A6700), Rgb(0xFFF8E6))),
        "[!CAUTION]" => Some(("CAUTION", Rgb(0xCF222E), Rgb(0xFFECEB))),
        _ => None,
    };

    let (accent_color, bg_color, content_lines) = match alert {
        Some((_, accent, bg)) => (accent, Some(bg), &lines[1..]),
        None => (Rgb(0xD0D7DE), None, lines),
    };

    let left_border = FlowBorder {
        width: 635 * 18,
        color: accent_color,
        style: BorderStyle::Single,
        space: 8 * EMU_PER_PT,
        art: None,
    };
    let para_direct = ParaFormat {
        left: Some(14 * EMU_PER_PT),
        before: Some(4 * EMU_PER_PT),
        after: Some(4 * EMU_PER_PT),
        shading: bg_color.map(Some),
        borders: [None, None, None, Some(Some(left_border))],
        ..ParaFormat::default()
    };

    let mut out = Vec::new();
    if let Some((label, accent, _)) = alert {
        let mut runs = vec![FlowRun {
            kind: FlowRunKind::Text(label.to_string()),
            style: FlowRunStyle::default(),
            format: RunFormat {
                bold: Some(true),
                color: Some(accent),
                ..RunFormat::default()
            },
            char_style: None,
            link: None,
            revision: None,
            show_field_code: false,
            field_locked: false,
            control: None,
        }];
        let joined = content_lines
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !joined.is_empty() {
            runs.push(FlowRun::new(
                FlowRunKind::LineBreak,
                FlowRunStyle::default(),
            ));
            runs.extend(parse_inlines(&joined, ctx, &RunFormat::default()));
        }
        let mut p = FlowParagraph {
            id: ctx.fresh_id(),
            style: ctx.styles.default_paragraph.clone(),
            direct: para_direct,
            runs,
            ..FlowParagraph::default()
        };
        p.resolve(&ctx.styles);
        p.normalize();
        out.push(Block::Paragraph(p));
        return out;
    }

    let joined = content_lines
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut p = FlowParagraph {
        id: ctx.fresh_id(),
        style: ctx.styles.default_paragraph.clone(),
        direct: para_direct,
        runs: parse_inlines(&joined, ctx, &RunFormat::default()),
        ..FlowParagraph::default()
    };
    p.resolve(&ctx.styles);
    p.normalize();
    out.push(Block::Paragraph(p));
    out
}

// ============================================================================
// Pipe Table parsing
// ============================================================================

fn is_table_row(s: &str) -> bool {
    let s = s.trim();
    s.starts_with('|') && s.ends_with('|') && s.len() >= 3
}

fn is_table_separator(s: &str) -> bool {
    if !is_table_row(s) {
        return false;
    }
    let cells = split_table_row(s);
    !cells.is_empty()
        && cells.iter().all(|c| {
            let t = c.trim().trim_start_matches(':').trim_end_matches(':');
            t.len() >= 3 && t.chars().all(|ch| ch == '-')
        })
}

fn split_table_row(s: &str) -> Vec<String> {
    let inner = s
        .trim()
        .strip_prefix('|')
        .unwrap_or(s)
        .strip_suffix('|')
        .unwrap_or(s);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'|') {
            cur.push('|');
            chars.next();
        } else if ch == '|' {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

fn parse_table_alignments(sep: &str) -> Vec<Option<FlowAlign>> {
    split_table_row(sep)
        .into_iter()
        .map(|c| {
            let t = c.trim();
            match (t.starts_with(':'), t.ends_with(':')) {
                (true, true) => Some(FlowAlign::Center),
                (false, true) => Some(FlowAlign::Right),
                (true, false) => Some(FlowAlign::Left),
                (false, false) => None,
            }
        })
        .collect()
}

fn build_table_block(
    header_cells: Vec<String>,
    alignments: Vec<Option<FlowAlign>>,
    body_rows: Vec<Vec<String>>,
    ctx: &mut ParseContext<'_>,
) -> Block {
    let col_count = header_cells
        .len()
        .max(alignments.len())
        .max(body_rows.iter().map(Vec::len).max().unwrap_or(0))
        .max(1);

    let total_width: Emu = 6 * EMU_PER_INCH;
    let col_w = (total_width / col_count as Emu) / 635 * 635;
    let columns = vec![col_w; col_count];

    let line = FlowBorder {
        width: 12_700 / 2,
        color: Rgb(0xBFBFBF),
        style: BorderStyle::Single,
        space: 0,
        art: None,
    };

    let build_row = |cells_text: Vec<String>, is_header: bool, ctx: &mut ParseContext<'_>| -> FlowRow {
        let mut cells = Vec::with_capacity(col_count);
        for c_idx in 0..col_count {
            let raw_cell = cells_text.get(c_idx).map(String::as_str).unwrap_or("");
            let align = alignments.get(c_idx).copied().flatten();
            let base_fmt = if is_header {
                RunFormat {
                    bold: Some(true),
                    ..RunFormat::default()
                }
            } else {
                RunFormat::default()
            };
            let parts: Vec<&str> = raw_cell.split("<br>").collect();
            let mut runs = Vec::new();
            for (pi, part) in parts.iter().enumerate() {
                if pi > 0 {
                    runs.push(FlowRun::new(
                        FlowRunKind::LineBreak,
                        FlowRunStyle::default(),
                    ));
                }
                runs.extend(parse_inlines(part.trim(), ctx, &base_fmt));
            }
            let mut p = FlowParagraph {
                id: ctx.fresh_id(),
                style: ctx.styles.default_paragraph.clone(),
                table_style: Some("TableGrid".to_string()),
                direct: ParaFormat {
                    align,
                    ..ParaFormat::default()
                },
                runs,
                ..FlowParagraph::default()
            };
            p.resolve(&ctx.styles);
            p.normalize();
            cells.push(FlowCell {
                id: ctx.fresh_id(),
                blocks: vec![Block::Paragraph(p)],
                span: 1,
                merge: VerticalMerge::None,
                shading: is_header.then_some(Rgb(0xF2F4F8)),
                borders: [Some(line), Some(line), Some(line), Some(line)],
                own_borders: [None; 4],
                diagonals: [None; 2],
                anchor: ibis_doc_model::CellAnchor::Top,
                direction: ibis_doc_model::CellDirection::LrTb,
                margins: [0, 108 * 635, 0, 108 * 635],
                no_wrap: false,
                ..FlowCell::default()
            });
        }
        FlowRow {
            id: ctx.fresh_id(),
            height: RowHeight::Auto,
            header: is_header,
            cant_split: false,
            cells,
            ..FlowRow::default()
        }
    };

    let mut rows = Vec::with_capacity(1 + body_rows.len());
    rows.push(build_row(header_cells, true, ctx));
    for r in body_rows {
        rows.push(build_row(r, false, ctx));
    }

    let mut table = FlowTable {
        id: ctx.fresh_id(),
        columns,
        rows,
        indent: 0,
        align: FlowAlign::Left,
        cell_spacing: None,
        borders: [Some(Some(line)); 6],
        style: Some(TableStyleRef {
            id: "TableGrid".to_string(),
            look: TableLook {
                first_row: true,
                last_row: false,
                first_column: false,
                last_column: false,
                no_hband: false,
                no_vband: true,
            },
            row_band: 1,
            col_band: 1,
        }),
        autofit: None,
        floating: None,
        ..FlowTable::default()
    };
    table.apply_style(&ctx.styles);
    Block::Table(table)
}

// ============================================================================
// Native Office Chart block (` ```chart `)
// ============================================================================

fn build_chart_block(content: &str, ctx: &mut ParseContext<'_>) -> Result<Block, Error> {
    let mut kind = NewChartKind::ClusteredColumn;
    let mut title: Option<String> = None;
    let mut size = DEFAULT_CHART_SIZE;
    let mut categories: Vec<String> = Vec::new();
    let mut series_list: Vec<(String, Vec<Option<Number>>)> = Vec::new();
    let mut csv_lines: Vec<&str> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            // YAML series item: `- name: 2025, values: [10, 20, 30]` or `- 2025: [10, 20, 30]`
            if let Some((sname, svals)) = parse_yaml_series_item(rest) {
                series_list.push((sname, svals));
                continue;
            }
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let val = v.trim();
            match key.as_str() {
                "type" | "kind" | "chart" => {
                    kind = parse_chart_kind(val);
                    continue;
                }
                "title" => {
                    let t = unquote_yaml(val);
                    if !t.is_empty() {
                        title = Some(t);
                    }
                    continue;
                }
                "width" => {
                    if let Some(w) = parse_length_emu(val) {
                        size.width = w.max(EMU_PER_INCH);
                    }
                    continue;
                }
                "height" => {
                    if let Some(h) = parse_length_emu(val) {
                        size.height = h.max(EMU_PER_INCH);
                    }
                    continue;
                }
                "categories" | "labels" | "x" => {
                    categories = parse_yaml_string_list(val);
                    continue;
                }
                "series" | "data" => {
                    continue;
                }
                _ => {}
            }
        }
        if trimmed.contains(',') {
            csv_lines.push(trimmed);
        }
    }

    // If CSV lines were provided instead of YAML `series:`, parse them.
    if series_list.is_empty() && !csv_lines.is_empty() {
        let rows: Vec<Vec<String>> = csv_lines
            .iter()
            .map(|l| l.split(',').map(|c| unquote_yaml(c.trim())).collect())
            .collect();
        if rows.len() >= 2 && rows[0].len() >= 2 {
            let header = &rows[0];
            categories = rows[1..]
                .iter()
                .map(|r| r.first().cloned().unwrap_or_default())
                .collect();
            for col in 1..header.len() {
                let name = header[col].clone();
                let values = rows[1..]
                    .iter()
                    .map(|r| {
                        r.get(col)
                            .and_then(|s| s.trim().parse::<f64>().ok())
                            .map(Number)
                    })
                    .collect();
                series_list.push((name, values));
            }
        }
    }

    build_chart_block_from_spec(kind, title, size, categories, series_list, content, ctx)
}

fn build_mermaid_block(content: &str, ctx: &mut ParseContext<'_>) -> Result<Block, Error> {
    match crate::mermaid::render_mermaid(content) {
        Some(crate::mermaid::MermaidOutput::Chart(spec)) => {
            if ctx.source.is_some() && ctx.baseline_pres.is_some() {
                build_chart_block_from_spec(
                    spec.kind,
                    spec.title,
                    DEFAULT_CHART_SIZE,
                    spec.categories,
                    spec.series,
                    content,
                    ctx,
                )
            } else if let Some((bytes, width_px, height_px)) =
                crate::mermaid::render_chart_spec_svg(&spec)
            {
                Ok(build_mermaid_svg_paragraph(bytes, width_px, height_px, content, ctx))
            } else {
                Ok(build_code_block(content, ctx))
            }
        }
        Some(crate::mermaid::MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) => Ok(build_mermaid_svg_paragraph(
            bytes,
            width_px,
            height_px,
            content,
            ctx,
        )),
        None => Ok(build_code_block(content, ctx)),
    }
}

/// A `plantuml` / `puml` fence: drawn offline as SVG (the Mermaid renderers are shared), its source kept in the picture's
/// description and its fence word in the title (`__plantuml__:<word>`), so a save writes the same fence back. A source with
/// a macro, a timing diagram it cannot draw or an unknown diagram type stays a code block.
fn build_plantuml_block(fence_lang: &str, content: &str, ctx: &mut ParseContext<'_>) -> Block {
    let rendered = crate::mermaid::render_plantuml(content);
    let has_blocking = rendered.unsupported.iter().any(|u| matches!(u.kind, "macro" | "timing" | "diagram-type"));
    if !has_blocking
        && let Some(crate::mermaid::MermaidOutput::Svg { bytes, width_px, height_px }) = rendered.output
    {
        let marker = format!("__plantuml__:{fence_lang}");
        return build_diagram_svg_paragraph(bytes, width_px, height_px, &marker, content, ctx);
    }
    build_code_block(content, ctx)
}

fn build_mermaid_svg_paragraph(
    bytes: Vec<u8>,
    width_px: u32,
    height_px: u32,
    content: &str,
    ctx: &mut ParseContext<'_>,
) -> Block {
    build_diagram_svg_paragraph(bytes, width_px, height_px, "__mermaid__", content, ctx)
}

fn build_diagram_svg_paragraph(
    bytes: Vec<u8>,
    width_px: u32,
    height_px: u32,
    marker_title: &str,
    content: &str,
    ctx: &mut ParseContext<'_>,
) -> Block {
    const EMU_PER_PX: i64 = 9_525;
    let raw_w = (width_px.max(1) as i64) * EMU_PER_PX;
    let raw_h = (height_px.max(1) as i64) * EMU_PER_PX;
    let max_w = 6 * EMU_PER_INCH;
    let (width, height) = if raw_w > max_w {
        let scaled_h = (raw_h * max_w / raw_w).max(EMU_PER_PT * 10);
        (max_w, scaled_h)
    } else {
        (raw_w, raw_h)
    };
    let media_id = ctx.fresh_media_id();
    ctx.media.push(Media {
        id: media_id,
        format: MediaFormat::Svg,
        data: bytes.into(),
    });
    let mut look = PictureLook::default();
    look.alt_text.title = marker_title.to_string();
    look.alt_text.description = content.to_string();
    let mut p = FlowParagraph {
        id: ctx.fresh_id(),
        style: ctx.styles.default_paragraph.clone(),
        direct: ParaFormat {
            align: Some(FlowAlign::Center),
            before: Some(6 * EMU_PER_PT),
            after: Some(6 * EMU_PER_PT),
            ..ParaFormat::default()
        },
        runs: vec![FlowRun::new(
            FlowRunKind::Picture {
                media: media_id,
                size: EmuSize::new(width, height),
                look,
            },
            FlowRunStyle::default(),
        )],
        ..FlowParagraph::default()
    };
    p.resolve(&ctx.styles);
    Block::Paragraph(p)
}

fn build_chart_block_from_spec(
    kind: NewChartKind,
    title: Option<String>,
    size: EmuSize,
    categories: Vec<String>,
    series_list: Vec<(String, Vec<Option<Number>>)>,
    raw_fallback: &str,
    ctx: &mut ParseContext<'_>,
) -> Result<Block, Error> {
    let shape_id = ctx.fresh_shape_id();
    let (Some(source), Some(baseline_pres)) = (ctx.source, ctx.baseline_pres) else {
        // Without a DOCX source (e.g. clipboard fragment paste), render as a fenced code fallback.
        return Ok(build_code_block(raw_fallback, ctx));
    };
    let base_para_id = baseline_pres
        .flow
        .as_ref()
        .and_then(|f| {
            f.body.iter().find_map(|b| match b {
                Block::Paragraph(p) => Some(p.id),
                _ => None,
            })
        })
        .unwrap_or(1);

    let mut shape = source.new_doc_chart(
        baseline_pres,
        FlowPos::new(base_para_id, 0),
        shape_id,
        size,
        kind,
    )?;

    if let ShapeKind::Chart(chart_box) = &mut shape.kind {
        let mut chart = (**chart_box).clone();
        let has_custom_data = !series_list.is_empty();
        if has_custom_data {
            let temp_series: Vec<Series> = series_list
                .iter()
                .map(|(name, values)| Series {
                    name: name.clone(),
                    categories: categories.clone(),
                    values: values.clone(),
                    x_values: Vec::new(),
                    bubble_sizes: Vec::new(),
                    format: "General".to_string(),
                    fill: Fill::None,
                    stroke: None,
                    marker: None,
                    explosion: 0,
                    inner_shadow: None,
                    point_fills: Vec::new(),
                    point_strokes: Vec::new(),
                    point_explosions: Vec::new(),
                    labels: None,
                    trendline: None,
                    error_bars: None,
                    bubble_3d: false,
                    has_format: false,
                    has_outline: false,
                })
                .collect();
            let temp_chart = Chart {
                groups: vec![ChartGroup {
                    kind: ChartType::Bar {
                        horizontal: false,
                        gap_width: 150,
                        overlap: 0,
                    },
                    of_pie: None,
                    grouping: Grouping::Standard,
                    series: temp_series,
                    axes: Vec::new(),
                    three_d: None,
                }],
                ..Chart::empty()
            };
            let chart_data = crate::save::pptx_chart_data::workbook_from_chart(&temp_chart)?;
            let wb_pres = crate::read(&chart_data.workbook)?;
            chart = crate::save::pptx_chart_data::chart_from_data(
                &chart,
                &wb_pres,
                &chart_data.series,
            );
            if let Some(t) = &title {
                chart.title = Some(ChartTitle {
                    text: t.clone(),
                    style: ChartText::default(),
                    at: None,
                });
            }
            source.set_chart_data(shape_id, chart.clone(), chart_data);
        } else if let Some(t) = &title {
            chart.title = Some(ChartTitle {
                text: t.clone(),
                style: ChartText::default(),
                at: None,
            });
        }
        *chart_box = Box::new(chart);
    }

    let mut p = FlowParagraph {
        id: ctx.fresh_id(),
        style: ctx.styles.default_paragraph.clone(),
        direct: ParaFormat {
            align: Some(FlowAlign::Center),
            before: Some(6 * EMU_PER_PT),
            after: Some(6 * EMU_PER_PT),
            ..ParaFormat::default()
        },
        runs: vec![FlowRun::new(
            FlowRunKind::Shape(Box::new(shape)),
            FlowRunStyle::default(),
        )],
        ..FlowParagraph::default()
    };
    p.resolve(&ctx.styles);
    Ok(Block::Paragraph(p))
}

fn parse_chart_kind(s: &str) -> NewChartKind {
    match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "column" | "clustered-column" | "縦棒" => NewChartKind::ClusteredColumn,
        "stacked-column" | "積み上げ縦棒" => NewChartKind::StackedColumn,
        "percent-stacked-column" | "100%-stacked-column" => NewChartKind::PercentStackedColumn,
        "bar" | "clustered-bar" | "横棒" => NewChartKind::ClusteredBar,
        "stacked-bar" | "積み上げ横棒" => NewChartKind::StackedBar,
        "percent-stacked-bar" | "100%-stacked-bar" => NewChartKind::PercentStackedBar,
        "line" | "折れ線" => NewChartKind::Line,
        "stacked-line" => NewChartKind::StackedLine,
        "area" | "面" => NewChartKind::Area,
        "stacked-area" => NewChartKind::StackedArea,
        "pie" | "円" => NewChartKind::Pie,
        "doughnut" | "donut" | "ドーナツ" => NewChartKind::Doughnut,
        "radar" | "レーダー" => NewChartKind::Radar,
        "scatter" | "散布図" => NewChartKind::Scatter,
        _ => NewChartKind::ClusteredColumn,
    }
}

fn parse_yaml_string_list(s: &str) -> Vec<String> {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|item| unquote_yaml(item.trim()))
        .filter(|item| !item.is_empty())
        .collect()
}

fn parse_yaml_number_list(s: &str) -> Vec<Option<Number>> {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|item| item.trim().parse::<f64>().ok().map(Number))
        .collect()
}

fn parse_yaml_series_item(s: &str) -> Option<(String, Vec<Option<Number>>)> {
    let s = s.trim().trim_start_matches('{').trim_end_matches('}');
    if let Some(vals_idx) = s.find("values:") {
        let before = s[..vals_idx].trim().trim_end_matches(',');
        let name = before
            .strip_prefix("name:")
            .map(|n| unquote_yaml(n.trim()))
            .unwrap_or_else(|| unquote_yaml(before));
        let vals_str = s[vals_idx + "values:".len()..].trim();
        return Some((name, parse_yaml_number_list(vals_str)));
    }
    if let Some((k, v)) = s.split_once(':') {
        let name = unquote_yaml(k.trim());
        let vals = parse_yaml_number_list(v.trim());
        if !vals.is_empty() {
            return Some((name, vals));
        }
    }
    None
}

// ============================================================================
// Inline parsing
// ============================================================================

fn parse_inlines(text: &str, ctx: &mut ParseContext<'_>, base_fmt: &RunFormat) -> Vec<FlowRun> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    let bytes = text.as_bytes();

    let flush_plain = |plain: &mut String, out: &mut Vec<FlowRun>, fmt: &RunFormat| {
        if !plain.is_empty() {
            out.push(FlowRun {
                kind: FlowRunKind::Text(std::mem::take(plain)),
                style: FlowRunStyle::default(),
                format: fmt.clone(),
                char_style: None,
                link: None,
                revision: None,
                show_field_code: false,
                field_locked: false,
                control: None,
            });
        }
    };

    while i < text.len() {
        let rest = &text[i..];

        // Escape sequence `\*`, `\[`, etc.
        if bytes[i] == b'\\' && i + 1 < text.len() {
            let next_ch = text[i + 1..].chars().next().unwrap();
            if next_ch.is_ascii_punctuation() {
                plain.push(next_ch);
                i += 1 + next_ch.len_utf8();
                continue;
            }
        }

        // CriticMarkup Insertion: `{++text++}`
        if let Some(after) = rest.strip_prefix("{++") {
            if let Some(end) = after.find("++}") {
                flush_plain(&mut plain, &mut out, base_fmt);
                let inner = &after[..end];
                let (author, body) = split_critic_author(inner);
                let mut runs = parse_inlines(body, ctx, base_fmt);
                for r in &mut runs {
                    r.revision = Some(Revision {
                        kind: RevisionKind::Insert,
                        author: author.clone(),
                        date: "2026-01-01T00:00:00Z".to_string(),
                    });
                }
                out.extend(runs);
                i += 3 + end + 3;
                continue;
            }
        }

        // CriticMarkup Deletion: `{--text--}`
        if let Some(after) = rest.strip_prefix("{--") {
            if let Some(end) = after.find("--}") {
                flush_plain(&mut plain, &mut out, base_fmt);
                let inner = &after[..end];
                let (author, body) = split_critic_author(inner);
                let mut runs = parse_inlines(body, ctx, base_fmt);
                for r in &mut runs {
                    r.revision = Some(Revision {
                        kind: RevisionKind::Delete,
                        author: author.clone(),
                        date: "2026-01-01T00:00:00Z".to_string(),
                    });
                }
                out.extend(runs);
                i += 3 + end + 3;
                continue;
            }
        }

        // CriticMarkup Substitution: `{~~old~>new~~}`
        if let Some(after) = rest.strip_prefix("{~~") {
            if let Some(end) = after.find("~~}") {
                let inner = &after[..end];
                if let Some((old_part, new_part)) = inner.split_once("~>") {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let mut del_runs = parse_inlines(old_part, ctx, base_fmt);
                    for r in &mut del_runs {
                        r.revision = Some(Revision {
                            kind: RevisionKind::Delete,
                            author: "Author".to_string(),
                            date: "2026-01-01T00:00:00Z".to_string(),
                        });
                    }
                    let mut ins_runs = parse_inlines(new_part, ctx, base_fmt);
                    for r in &mut ins_runs {
                        r.revision = Some(Revision {
                            kind: RevisionKind::Insert,
                            author: "Author".to_string(),
                            date: "2026-01-01T00:00:00Z".to_string(),
                        });
                    }
                    out.extend(del_runs);
                    out.extend(ins_runs);
                    i += 3 + end + 3;
                    continue;
                }
            }
        }

        // CriticMarkup Comment: `{>>author: comment<<}`
        if let Some(after) = rest.strip_prefix("{>>") {
            if let Some(end) = after.find("<<}") {
                flush_plain(&mut plain, &mut out, base_fmt);
                let inner = after[..end].trim();
                let (author, comment_text) = split_critic_author(inner);
                let cid = ctx.fresh_comment_id();
                let mut cp = FlowParagraph {
                    id: ctx.fresh_id(),
                    style: ctx
                        .styles
                        .paragraph_style_named("annotation text")
                        .map(str::to_string)
                        .or_else(|| ctx.styles.default_paragraph.clone()),
                    runs: vec![FlowRun::new(
                        FlowRunKind::Text(comment_text.to_string()),
                        FlowRunStyle::default(),
                    )],
                    ..FlowParagraph::default()
                };
                cp.resolve(&ctx.styles);
                let initials: String = author.chars().take(2).collect();
                ctx.comments.push(Comment {
                    id: cid,
                    author,
                    initials,
                    date: "2026-01-01T00:00:00Z".to_string(),
                    blocks: vec![Block::Paragraph(cp)],
                    parent: None,
                    done: false,
                    para_id: Some(format!(
                        "{:08X}",
                        0x1C00_0000u32.wrapping_add(cid.wrapping_mul(0x0001_0101)) & 0x7FFF_FFFF
                    )),
                });
                // Wrap the preceding run in CommentStart..CommentEnd if available.
                let start_run = FlowRun {
                    kind: FlowRunKind::CommentStart(cid),
                    style: FlowRunStyle::default(),
                    format: RunFormat::default(),
                    char_style: None,
                    link: None,
                    revision: None,
                    show_field_code: false,
                    field_locked: false,
                    control: None,
                };
                let end_run = FlowRun {
                    kind: FlowRunKind::CommentEnd(cid),
                    style: FlowRunStyle::default(),
                    format: RunFormat::default(),
                    char_style: None,
                    link: None,
                    revision: None,
                    show_field_code: false,
                    field_locked: false,
                    control: None,
                };
                let ref_run = FlowRun::new(FlowRunKind::CommentRef(cid), FlowRunStyle::default());
                if let Some(last) = out.pop() {
                    out.push(start_run);
                    out.push(last);
                    out.push(end_run);
                    out.push(ref_run);
                } else {
                    out.push(start_run);
                    out.push(end_run);
                    out.push(ref_run);
                }
                i += 3 + end + 3;
                continue;
            }
        }

        // Japanese Ruby: `{漢字|かんじ}`
        if let Some(after) = rest.strip_prefix('{') {
            if let Some(close) = after.find('}') {
                let inner = &after[..close];
                if !inner.contains('\n')
                    && let Some((base, ruby)) = inner.split_once('|')
                    && !base.is_empty()
                    && !ruby.is_empty()
                {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    out.push(FlowRun {
                        kind: FlowRunKind::Ruby {
                            base: base.to_string(),
                            text: ruby.to_string(),
                            align: RubyAlign::DistributeSpace,
                            size: 1000,
                            raise: 0,
                        },
                        style: FlowRunStyle::default(),
                        format: base_fmt.clone(),
                        char_style: None,
                        link: None,
                        revision: None,
                        show_field_code: false,
                        field_locked: false,
                        control: None,
                    });
                    i += 1 + close + 1;
                    continue;
                }
            }
        }

        // Narou / Aozora Ruby: `｜漢字《かんじ》`
        if let Some(after) = rest.strip_prefix('｜') {
            if let Some(open_rt) = after.find('《') {
                if !after[open_rt + '《'.len_utf8()..].starts_with('《') {
                    if let Some(close_rt) = after[open_rt + '《'.len_utf8()..].find('》') {
                        let base = &after[..open_rt];
                        let ruby = &after[open_rt + '《'.len_utf8()..open_rt + '《'.len_utf8() + close_rt];
                        if !base.is_empty() && !ruby.is_empty() {
                            flush_plain(&mut plain, &mut out, base_fmt);
                            out.push(FlowRun {
                                kind: FlowRunKind::Ruby {
                                    base: base.to_string(),
                                    text: ruby.to_string(),
                                    align: RubyAlign::DistributeSpace,
                                    size: 1000,
                                    raise: 0,
                                },
                                style: FlowRunStyle::default(),
                                format: base_fmt.clone(),
                                char_style: None,
                                link: None,
                                revision: None,
                                show_field_code: false,
                                field_locked: false,
                                control: None,
                            });
                            i += '｜'.len_utf8() + open_rt + '《'.len_utf8() + close_rt + '》'.len_utf8();
                            continue;
                        }
                    }
                }
            }
        }

        // Emphasis dots: `《《強調》》`
        if let Some(after) = rest.strip_prefix("《《") {
            if let Some(end) = after.find("》》") {
                let inner = &after[..end];
                if !inner.is_empty() {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let mut fmt = base_fmt.clone();
                    fmt.emphasis = Some(EmphasisMark::Dot);
                    out.extend(parse_inlines(inner, ctx, &fmt));
                    i += "《《".len() + end + "》》".len();
                    continue;
                }
            }
        }

        // Inline image: `![alt](data:image/...;base64,...){width=... height=...}`
        if let Some(after_bang) = rest.strip_prefix("![") {
            if let Some(alt_end) = after_bang.find("](") {
                let alt = &after_bang[..alt_end];
                let after_open_paren = &after_bang[alt_end + 2..];
                if let Some(url_end) = after_open_paren.find(')') {
                    let raw_target = after_open_paren[..url_end].trim();
                    let url = strip_markdown_link_title(raw_target);
                    let mut total_consumed = 2 + alt_end + 2 + url_end + 1;
                    let mut explicit_w: Option<Emu> = None;
                    let mut explicit_h: Option<Emu> = None;
                    let after_paren = &after_open_paren[url_end + 1..];
                    if let Some(attr_open) = after_paren.strip_prefix('{') {
                        if let Some(attr_close) = attr_open.find('}') {
                            let attr_str = &attr_open[..attr_close];
                            for part in attr_str.split_whitespace() {
                                if let Some(w) = part.strip_prefix("width=").and_then(parse_length_emu) {
                                    explicit_w = Some(w.max(EMU_PER_PT * 10));
                                } else if let Some(h) = part.strip_prefix("height=").and_then(parse_length_emu) {
                                    explicit_h = Some(h.max(EMU_PER_PT * 10));
                                }
                            }
                            total_consumed += 1 + attr_close + 1;
                        }
                    }
                    if let Some((format, data)) = decode_data_uri_image(url).or_else(|| load_local_image(url, ctx.base_dir)) {
                        let img_size = resolve_image_size(&data, format, explicit_w, explicit_h);
                        flush_plain(&mut plain, &mut out, base_fmt);
                        let media_id = ctx.fresh_media_id();
                        ctx.media.push(Media {
                            id: media_id,
                            format,
                            data: data.into(),
                        });
                        let mut look = PictureLook::default();
                        if !alt.is_empty() {
                            look.alt_text.description = alt.to_string();
                        }
                        out.push(FlowRun {
                            kind: FlowRunKind::Picture {
                                media: media_id,
                                size: img_size,
                                look,
                            },
                            style: FlowRunStyle::default(),
                            format: base_fmt.clone(),
                            char_style: None,
                            link: None,
                            revision: None,
                            show_field_code: false,
                            field_locked: false,
                            control: None,
                        });
                        i += total_consumed;
                        continue;
                    }
                }
            }
        }

        // Footnote reference: `[^id]`
        if let Some(after) = rest.strip_prefix("[^") {
            if let Some(close) = after.find(']') {
                let label = &after[..close];
                if !label.is_empty() && !label.contains(' ') && !after[close + 1..].starts_with(':') {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let id = ctx.footnote_id_for(label);
                    let mut fmt = base_fmt.clone();
                    fmt.position = Some(VerticalPosition::Superscript);
                    out.push(FlowRun {
                        kind: FlowRunKind::NoteRef {
                            endnote: false,
                            id,
                        },
                        style: FlowRunStyle::default(),
                        format: fmt,
                        char_style: None,
                        link: None,
                        revision: None,
                        show_field_code: false,
                        field_locked: false,
                        control: None,
                    });
                    i += 2 + close + 1;
                    continue;
                }
            }
        }

        // Span attributes or Link: `[text]{.tcy}` / `[text]{.em}` / `[text](url)`
        if let Some(after_open) = rest.strip_prefix('[') {
            if let Some(close_bracket) = find_matching_bracket(after_open) {
                let inner_text = &after_open[..close_bracket];
                let after_bracket = &after_open[close_bracket + 1..];

                // `[text]{.tcy}` or `[text]{.em}`
                if let Some(attr_after) = after_bracket.strip_prefix('{') {
                    if let Some(attr_close) = attr_after.find('}') {
                        let attr_body = attr_after[..attr_close].trim();
                        if matches!(attr_body, ".tcy" | "tcy") {
                            flush_plain(&mut plain, &mut out, base_fmt);
                            let mut fmt = base_fmt.clone();
                            fmt.tate_chu_yoko = Some(true);
                            out.extend(parse_inlines(inner_text, ctx, &fmt));
                            i += 1 + close_bracket + 1 + 1 + attr_close + 1;
                            continue;
                        } else if matches!(attr_body, ".em" | ".emphasis" | "em") {
                            flush_plain(&mut plain, &mut out, base_fmt);
                            let mut fmt = base_fmt.clone();
                            fmt.emphasis = Some(EmphasisMark::Dot);
                            out.extend(parse_inlines(inner_text, ctx, &fmt));
                            i += 1 + close_bracket + 1 + 1 + attr_close + 1;
                            continue;
                        }
                    }
                }

                // `[text](url)`
                if let Some(paren_after) = after_bracket.strip_prefix('(') {
                    if let Some(close_paren) = paren_after.find(')') {
                        let target = paren_after[..close_paren].trim();
                        flush_plain(&mut plain, &mut out, base_fmt);
                        let link = if let Some(anchor) = target.strip_prefix('#') {
                            Link::Anchor(anchor.to_string())
                        } else {
                            Link::Url(target.to_string())
                        };
                        let mut link_fmt = base_fmt.clone();
                        if link_fmt.color.is_none() {
                            link_fmt.color = Some(Rgb(0x0563C1));
                        }
                        if link_fmt.underline.is_none() {
                            link_fmt.underline = Some(true);
                        }
                        let mut runs = parse_inlines(inner_text, ctx, &link_fmt);
                        for r in &mut runs {
                            r.link = Some(link.clone());
                        }
                        out.extend(runs);
                        i += 1 + close_bracket + 1 + 1 + close_paren + 1;
                        continue;
                    }
                }
            }
        }

        // Inline code: `` `code` ``
        if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                if end > 0 {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let mut fmt = base_fmt.clone();
                    fmt.font = Some("Consolas".to_string());
                    fmt.h_ansi_font = Some("Consolas".to_string());
                    fmt.east_asian_font = Some("ＭＳ ゴシック".to_string());
                    fmt.highlight = Some(Some(Rgb(0xF2F4F8)));
                    out.push(FlowRun {
                        kind: FlowRunKind::Text(after[..end].to_string()),
                        style: FlowRunStyle::default(),
                        format: fmt,
                        char_style: None,
                        link: None,
                        revision: None,
                        show_field_code: false,
                        field_locked: false,
                        control: None,
                    });
                    i += 1 + end + 1;
                    continue;
                }
            }
        }

        // Inline math: `$math$`
        if let Some(after) = rest.strip_prefix('$') {
            if !after.starts_with('$') {
                if let Some(end) = after.find('$') {
                    let latex = &after[..end];
                    if !latex.is_empty() && !latex.starts_with(' ') && !latex.ends_with(' ') {
                        flush_plain(&mut plain, &mut out, base_fmt);
                        let math = parse_latex(latex);
                        out.push(FlowRun {
                            kind: FlowRunKind::Math {
                                math,
                                display: false,
                            },
                            style: FlowRunStyle::default(),
                            format: base_fmt.clone(),
                            char_style: None,
                            link: None,
                            revision: None,
                            show_field_code: false,
                            field_locked: false,
                            control: None,
                        });
                        i += 1 + end + 1;
                        continue;
                    }
                }
            }
        }

        // Bold + Italic: `***text***`
        if let Some(after) = rest.strip_prefix("***") {
            if let Some(end) = after.find("***") {
                if end > 0 {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let mut fmt = base_fmt.clone();
                    fmt.bold = Some(true);
                    fmt.italic = Some(true);
                    out.extend(parse_inlines(&after[..end], ctx, &fmt));
                    i += 3 + end + 3;
                    continue;
                }
            }
        }

        // Bold: `**text**`
        if let Some(after) = rest.strip_prefix("**") {
            if let Some(end) = after.find("**") {
                if end > 0 {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let mut fmt = base_fmt.clone();
                    fmt.bold = Some(true);
                    out.extend(parse_inlines(&after[..end], ctx, &fmt));
                    i += 2 + end + 2;
                    continue;
                }
            }
        }

        // Strikethrough: `~~text~~`
        if let Some(after) = rest.strip_prefix("~~") {
            if let Some(end) = after.find("~~") {
                if end > 0 {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let mut fmt = base_fmt.clone();
                    fmt.strike = Some(true);
                    out.extend(parse_inlines(&after[..end], ctx, &fmt));
                    i += 2 + end + 2;
                    continue;
                }
            }
        }

        // Underline: `++text++`
        if let Some(after) = rest.strip_prefix("++") {
            if let Some(end) = after.find("++") {
                if end > 0 {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let mut fmt = base_fmt.clone();
                    fmt.underline = Some(true);
                    out.extend(parse_inlines(&after[..end], ctx, &fmt));
                    i += 2 + end + 2;
                    continue;
                }
            }
        }

        // Highlight: `==text==`
        if let Some(after) = rest.strip_prefix("==") {
            if let Some(end) = after.find("==") {
                if end > 0 {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let mut fmt = base_fmt.clone();
                    fmt.highlight = Some(Some(Rgb(0xFFFF00)));
                    out.extend(parse_inlines(&after[..end], ctx, &fmt));
                    i += 2 + end + 2;
                    continue;
                }
            }
        }

        // Italic: `*text*`
        if let Some(after) = rest.strip_prefix('*') {
            if !after.starts_with('*') && !after.starts_with(' ') {
                if let Some(end) = after.find('*') {
                    if end > 0 {
                        flush_plain(&mut plain, &mut out, base_fmt);
                        let mut fmt = base_fmt.clone();
                        fmt.italic = Some(true);
                        out.extend(parse_inlines(&after[..end], ctx, &fmt));
                        i += 1 + end + 1;
                        continue;
                    }
                }
            }
        }

        // Superscript: `^text^`
        if let Some(after) = rest.strip_prefix('^') {
            if let Some(end) = after.find('^') {
                if end > 0 && !after[..end].contains(' ') {
                    flush_plain(&mut plain, &mut out, base_fmt);
                    let mut fmt = base_fmt.clone();
                    fmt.position = Some(VerticalPosition::Superscript);
                    out.extend(parse_inlines(&after[..end], ctx, &fmt));
                    i += 1 + end + 1;
                    continue;
                }
            }
        }

        // Subscript: `~text~`
        if let Some(after) = rest.strip_prefix('~') {
            if !after.starts_with('~') {
                if let Some(end) = after.find('~') {
                    if end > 0 && !after[..end].contains(' ') {
                        flush_plain(&mut plain, &mut out, base_fmt);
                        let mut fmt = base_fmt.clone();
                        fmt.position = Some(VerticalPosition::Subscript);
                        out.extend(parse_inlines(&after[..end], ctx, &fmt));
                        i += 1 + end + 1;
                        continue;
                    }
                }
            }
        }

        let ch = rest.chars().next().unwrap();
        plain.push(ch);
        i += ch.len_utf8();
    }

    flush_plain(&mut plain, &mut out, base_fmt);
    out
}

fn find_matching_bracket(s: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (idx, ch) in s.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_critic_author(s: &str) -> (String, &str) {
    if let Some((author, rest)) = s.split_once(':') {
        let a = author.trim();
        if !a.is_empty() && a.len() <= 32 && !a.contains('\n') {
            return (a.to_string(), rest.trim());
        }
    }
    ("Author".to_string(), s)
}

fn strip_markdown_link_title(target: &str) -> &str {
    let trimmed = target.trim();
    let trimmed = trimmed.strip_prefix('<').and_then(|s| s.strip_suffix('>')).unwrap_or(trimmed);
    if let Some((path, _)) = trimmed.split_once(" \"") {
        path.trim()
    } else if let Some((path, _)) = trimmed.split_once(" '") {
        path.trim()
    } else {
        trimmed
    }
}

fn decode_data_uri_image(uri: &str) -> Option<(MediaFormat, Vec<u8>)> {
    let after_data = uri.strip_prefix("data:image/")?;
    let (mime_sub, b64) = after_data.split_once(";base64,")?;
    let format = match mime_sub.to_ascii_lowercase().as_str() {
        "png" => MediaFormat::Png,
        "jpeg" | "jpg" => MediaFormat::Jpeg,
        "gif" => MediaFormat::Gif,
        "bmp" => MediaFormat::Bmp,
        "svg+xml" | "svg" => MediaFormat::Svg,
        "x-emf" | "emf" => MediaFormat::Emf,
        "x-wmf" | "wmf" => MediaFormat::Wmf,
        _ => return None,
    };
    let bytes = decode_base64(b64.trim())?;
    Some((format, bytes))
}

/// Load a relative local image path against `base_dir` (non-WASM only, confined within `base_dir`).
#[cfg(not(target_family = "wasm"))]
fn load_local_image(url: &str, base_dir: Option<&std::path::Path>) -> Option<(MediaFormat, Vec<u8>)> {
    let base_dir = base_dir?;
    let clean = url.split(['?', '#']).next().unwrap_or(url).trim();
    if clean.is_empty()
        || clean.starts_with("http://")
        || clean.starts_with("https://")
        || clean.starts_with("data:")
        || clean.starts_with("file:")
    {
        return None;
    }
    let rel = std::path::Path::new(clean);
    if rel.is_absolute() {
        return None;
    }
    let candidate = base_dir.join(rel);
    let canon_base = std::fs::canonicalize(base_dir).ok()?;
    let canon_file = std::fs::canonicalize(&candidate).ok()?;
    if !canon_file.starts_with(&canon_base) {
        return None;
    }
    let bytes = std::fs::read(&canon_file).ok()?;
    let format = detect_image_format(&bytes, &canon_file)?;
    Some((format, bytes))
}

#[cfg(target_family = "wasm")]
fn load_local_image(_url: &str, _base_dir: Option<&std::path::Path>) -> Option<(MediaFormat, Vec<u8>)> {
    None
}

fn detect_image_format(bytes: &[u8], path: &std::path::Path) -> Option<MediaFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(MediaFormat::Png);
    }
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        return Some(MediaFormat::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(MediaFormat::Gif);
    }
    if bytes.starts_with(b"BM") {
        return Some(MediaFormat::Bmp);
    }
    let ext = path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase());
    match ext.as_deref() {
        Some("png") => Some(MediaFormat::Png),
        Some("jpg" | "jpeg") => Some(MediaFormat::Jpeg),
        Some("gif") => Some(MediaFormat::Gif),
        Some("bmp") => Some(MediaFormat::Bmp),
        Some("svg") => Some(MediaFormat::Svg),
        Some("emf") => Some(MediaFormat::Emf),
        Some("wmf") => Some(MediaFormat::Wmf),
        _ => None,
    }
}

fn infer_image_dimensions_px(data: &[u8], format: MediaFormat) -> Option<(u32, u32)> {
    match format {
        MediaFormat::Png if data.len() >= 24 && data.starts_with(b"\x89PNG\r\n\x1a\n") => {
            let w = u32::from_be_bytes(data[16..20].try_into().ok()?);
            let h = u32::from_be_bytes(data[20..24].try_into().ok()?);
            (w > 0 && h > 0).then_some((w, h))
        }
        MediaFormat::Gif if data.len() >= 10 && (data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) => {
            let w = u16::from_le_bytes(data[6..8].try_into().ok()?) as u32;
            let h = u16::from_le_bytes(data[8..10].try_into().ok()?) as u32;
            (w > 0 && h > 0).then_some((w, h))
        }
        MediaFormat::Bmp if data.len() >= 26 && data.starts_with(b"BM") => {
            let w = i32::from_le_bytes(data[18..22].try_into().ok()?).unsigned_abs();
            let h = i32::from_le_bytes(data[22..26].try_into().ok()?).unsigned_abs();
            (w > 0 && h > 0).then_some((w, h))
        }
        _ => None,
    }
}

fn resolve_image_size(data: &[u8], format: MediaFormat, explicit_w: Option<Emu>, explicit_h: Option<Emu>) -> EmuSize {
    match (explicit_w, explicit_h) {
        (Some(w), Some(h)) => EmuSize::new(w, h),
        (Some(w), None) => {
            let h = infer_image_dimensions_px(data, format)
                .map(|(pw, ph)| ((w as f64) * (ph as f64) / (pw as f64)).round() as Emu)
                .unwrap_or(w * 2 / 3);
            EmuSize::new(w, h.max(EMU_PER_PT * 10))
        }
        (None, Some(h)) => {
            let w = infer_image_dimensions_px(data, format)
                .map(|(pw, ph)| ((h as f64) * (pw as f64) / (ph as f64)).round() as Emu)
                .unwrap_or(h * 3 / 2);
            EmuSize::new(w.max(EMU_PER_PT * 10), h)
        }
        (None, None) => {
            if let Some((pw, ph)) = infer_image_dimensions_px(data, format) {
                // Assume 96 DPI (9525 EMU per px), capped to 5 inches wide.
                let max_w = 5 * EMU_PER_INCH;
                let mut w = pw as Emu * 9_525;
                let mut h = ph as Emu * 9_525;
                if w > max_w {
                    h = ((h as f64) * (max_w as f64) / (w as f64)).round() as Emu;
                    w = max_w;
                }
                EmuSize::new(w.max(EMU_PER_PT * 10), h.max(EMU_PER_PT * 10))
            } else {
                EmuSize::new(3 * EMU_PER_INCH, 2 * EMU_PER_INCH)
            }
        }
    }
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    let val = |b: u8| -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    let mut out = Vec::with_capacity(cleaned.len() * 3 / 4);
    let mut chunks = cleaned.chunks(4);
    for chunk in &mut chunks {
        match chunk.len() {
            4 => {
                let (a, b, c, d) = (val(chunk[0])?, val(chunk[1])?, val(chunk[2])?, val(chunk[3])?);
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
                out.push((c << 6) | d);
            }
            3 => {
                let (a, b, c) = (val(chunk[0])?, val(chunk[1])?, val(chunk[2])?);
                out.push((a << 2) | (b >> 4));
                out.push((b << 4) | (c >> 2));
            }
            2 => {
                let (a, b) = (val(chunk[0])?, val(chunk[1])?);
                out.push((a << 2) | (b >> 4));
            }
            _ => return None,
        }
    }
    Some(out)
}

fn encode_base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

// ============================================================================
// Serializer (FlowDocument -> Deterministic Rich Markdown)
// ============================================================================

/// Export a [`FlowDocument`] and its media to deterministic Rich Markdown.
pub fn export_markdown(doc: &FlowDocument, media: &[Media]) -> String {
    let mut out = String::new();

    // 1. YAML Frontmatter (written when metadata, non-default page setup, headers/footers, or watermark exist)
    let fm = serialize_frontmatter(doc);
    if !fm.is_empty() {
        out.push_str("---\n");
        out.push_str(&fm);
        out.push_str("---\n\n");
    }

    let comments_by_id: HashMap<u32, &Comment> = doc.comments.iter().map(|c| (c.id, c)).collect();

    // 2. Body blocks
    let mut prev_was_list = false;
    let mut i = 0;
    while i < doc.body.len() {
        let block = &doc.body[i];

        // Collapse TOC field blocks back to `[[toc]]`
        if let Block::Paragraph(p) = block {
            if p.field_start
                .as_deref()
                .is_some_and(|f| f.trim_start().starts_with("TOC"))
            {
                if prev_was_list {
                    out.push('\n');
                    prev_was_list = false;
                }
                out.push_str("[[toc]]\n\n");
                while i < doc.body.len() {
                    let is_end = matches!(&doc.body[i], Block::Paragraph(ep) if ep.field_end);
                    i += 1;
                    if is_end {
                        break;
                    }
                }
                continue;
            }
        }

        let is_list = matches!(block, Block::Paragraph(p) if p.list.is_some() || p.runs.first().is_some_and(|r| matches!(r.kind, FlowRunKind::CheckBox { .. })));
        if prev_was_list && !is_list && !out.ends_with("\n\n") {
            out.push('\n');
        }

        serialize_block(block, doc, media, &comments_by_id, &mut out);
        prev_was_list = is_list;
        i += 1;
    }

    // 3. Footnotes
    if !doc.footnotes.is_empty() {
        if !out.ends_with("\n\n") && !out.is_empty() {
            out.push('\n');
        }
        for note in &doc.footnotes {
            let mut note_md = String::new();
            for b in &note.blocks {
                if let Block::Paragraph(p) = b {
                    let runs: Vec<FlowRun> = p
                        .runs
                        .iter()
                        .skip_while(|r| {
                            matches!(r.kind, FlowRunKind::NoteMark)
                                || matches!(&r.kind, FlowRunKind::Text(t) if t == " ")
                        })
                        .cloned()
                        .collect();
                    note_md.push_str(&serialize_runs(&runs, media, &comments_by_id));
                }
            }
            out.push_str(&format!("[^{}]: {}\n", note.id, note_md.trim()));
        }
    }

    let trimmed = out.trim_end();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

fn serialize_frontmatter(doc: &FlowDocument) -> String {
    let mut lines = Vec::new();
    let props: &DocProperties = &doc.properties;
    if let Some(title) = props.title.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("title: {}", quote_yaml_if_needed(title)));
    }
    if let Some(subject) = props.subject.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("subtitle: {}", quote_yaml_if_needed(subject)));
    }
    if let Some(creator) = props.creator.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("author: {}", quote_yaml_if_needed(creator)));
    }
    if let Some(keywords) = props.keywords.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("keywords: {}", quote_yaml_if_needed(keywords)));
    }
    if let Some(description) = props.description.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!(
            "description: {}",
            quote_yaml_if_needed(description)
        ));
    }
    if let Some(company) = props.company.as_deref().filter(|s| !s.is_empty()) {
        lines.push(format!("company: {}", quote_yaml_if_needed(company)));
    }

    let default_page = PageSetup::default();
    let is_landscape = doc.page.size.width > doc.page.size.height;
    let (short_side, long_side) = (
        doc.page.size.width.min(doc.page.size.height),
        doc.page.size.width.max(doc.page.size.height),
    );
    if (short_side, long_side) != (default_page.size.width, default_page.size.height) {
        if let Some(name) = paper_size_name(short_side, long_side) {
            lines.push(format!("size: {name}"));
        }
    }
    if is_landscape {
        lines.push("orientation: landscape".to_string());
    }
    if doc.page.columns.count > 1 {
        lines.push(format!("columns: {}", doc.page.columns.count));
        if doc.page.columns.separator {
            lines.push("column_separator: true".to_string());
        }
    }
    if doc.page.direction == CellDirection::TbRl {
        lines.push("direction: vertical".to_string());
    }
    if let Some(ln) = &doc.page.line_numbers {
        let mode = if ln.restart == LineNumberRestart::Continuous {
            "continuous"
        } else {
            "true"
        };
        lines.push(format!("line_numbers: {mode}"));
    }
    if let Some(preset) = doc.style_set_preset {
        let name = match preset {
            StyleSetPreset::Default => "default",
            StyleSetPreset::Casual => "casual",
            StyleSetPreset::Centered => "centered",
            StyleSetPreset::Formal => "formal",
            StyleSetPreset::Elegant => "elegant",
            StyleSetPreset::Modern => "modern",
            StyleSetPreset::Simple => "simple",
            StyleSetPreset::Minimalist => "minimalist",
        };
        lines.push(format!("style_set: {name}"));
    }
    if let Some(preset) = doc.paragraph_spacing_preset {
        let name = match preset {
            ParagraphSpacingPreset::Default => "default",
            ParagraphSpacingPreset::NoSpace => "no_space",
            ParagraphSpacingPreset::Compact => "compact",
            ParagraphSpacingPreset::Tight => "tight",
            ParagraphSpacingPreset::Open => "open",
            ParagraphSpacingPreset::Relaxed => "relaxed",
            ParagraphSpacingPreset::Double => "double",
        };
        lines.push(format!("paragraph_spacing: {name}"));
    }
    if !doc.linked[0] && !doc.header.is_empty() {
        let h_text = serialize_header_footer(&doc.header);
        if !h_text.is_empty() {
            lines.push(format!("header: {}", quote_yaml_if_needed(&h_text)));
        }
    }
    if !doc.linked[1] && !doc.footer.is_empty() {
        let f_text = serialize_header_footer(&doc.footer);
        if !f_text.is_empty() {
            lines.push(format!("footer: {}", quote_yaml_if_needed(&f_text)));
        }
    }
    if let Some(wm) = &doc.watermark {
        lines.push(format!("watermark: {}", quote_yaml_if_needed(&wm.text)));
    }
    if let Some(pc) = doc.page_color {
        lines.push(format!("page_color: \"#{:06X}\"", pc.0));
    }
    if doc.track_changes {
        lines.push("track_changes: true".to_string());
    }

    if lines.is_empty() {
        String::new()
    } else {
        let mut s = lines.join("\n");
        s.push('\n');
        s
    }
}

fn serialize_header_footer(blocks: &[Block]) -> String {
    let mut s = String::new();
    for b in blocks {
        if let Block::Paragraph(p) = b {
            for r in &p.runs {
                match &r.kind {
                    FlowRunKind::Text(t) => s.push_str(t),
                    FlowRunKind::PageNumber => s.push_str("{page}"),
                    FlowRunKind::FileName(_) => s.push_str("{filename}"),
                    _ => {}
                }
            }
        }
    }
    s
}

fn quote_yaml_if_needed(s: &str) -> String {
    if s.contains(':')
        || s.contains('#')
        || s.contains('{')
        || s.contains('}')
        || s.contains('[')
        || s.contains(']')
        || s.starts_with(' ')
        || s.ends_with(' ')
    {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

fn paper_size_name(short: Emu, long: Emu) -> Option<&'static str> {
    let near = |a: Emu, b: Emu| (a - b).abs() < 50_000;
    let mm = |v: f64| (v * EMU_PER_MM).round() as Emu;
    if near(short, mm(297.0)) && near(long, mm(420.0)) {
        Some("A3")
    } else if near(short, 11_906 * 635) && near(long, 16_838 * 635) {
        Some("A4")
    } else if near(short, mm(148.0)) && near(long, mm(210.0)) {
        Some("A5")
    } else if near(short, mm(182.0)) && near(long, mm(257.0)) {
        Some("B5")
    } else if near(short, 8 * EMU_PER_INCH + EMU_PER_INCH / 2) && near(long, 11 * EMU_PER_INCH) {
        Some("Letter")
    } else if near(short, 8 * EMU_PER_INCH + EMU_PER_INCH / 2) && near(long, 14 * EMU_PER_INCH) {
        Some("Legal")
    } else {
        None
    }
}

fn serialize_block(
    block: &Block,
    doc: &FlowDocument,
    media: &[Media],
    comments: &HashMap<u32, &Comment>,
    out: &mut String,
) {
    match block {
        Block::Paragraph(p) => serialize_paragraph(p, doc, media, comments, out),
        Block::Table(t) => serialize_table(t, media, comments, out),
    }
}

fn serialize_paragraph(
    p: &FlowParagraph,
    doc: &FlowDocument,
    media: &[Media],
    comments: &HashMap<u32, &Comment>,
    out: &mut String,
) {
    // 1. Standalone PageBreak / ColumnBreak
    if p.runs.len() == 1 {
        match &p.runs[0].kind {
            FlowRunKind::PageBreak => {
                out.push_str("\\newpage\n\n");
                return;
            }
            FlowRunKind::ColumnBreak => {
                out.push_str("\\columnbreak\n\n");
                return;
            }
            FlowRunKind::Math { math, display: true } => {
                out.push_str("$$\n");
                out.push_str(&math.to_latex());
                out.push_str("\n$$\n\n");
                return;
            }
            FlowRunKind::Shape(shape) => {
                if let ShapeKind::Chart(chart) = &shape.kind {
                    serialize_chart(chart, out);
                    return;
                }
            }
            FlowRunKind::Picture { look, .. } if look.alt_text.title == "__mermaid__" => {
                out.push_str("```mermaid\n");
                out.push_str(&look.alt_text.description);
                if !look.alt_text.description.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
                return;
            }
            FlowRunKind::Picture { look, .. }
                if look.alt_text.title == "__plantuml__" || look.alt_text.title.starts_with("__plantuml__:") =>
            {
                let fence = look
                    .alt_text
                    .title
                    .strip_prefix("__plantuml__:")
                    .filter(|lang| !lang.is_empty())
                    .unwrap_or("plantuml");
                out.push_str("```");
                out.push_str(fence);
                out.push('\n');
                out.push_str(&look.alt_text.description);
                if !look.alt_text.description.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
                return;
            }
            _ => {}
        }
    }

    // 2. Horizontal rule (empty paragraph with bottom border only)
    if p.runs.is_empty()
        && p.direct.borders[2].is_some_and(|b| b.is_some())
        && p.direct.borders[0].is_none()
        && p.direct.borders[3].is_none()
    {
        out.push_str("---\n\n");
        return;
    }

    // 3. Fenced code block (shaded + 4 borders + Consolas font)
    if p.direct.shading.is_some_and(|s| s.is_some())
        && p.direct.borders[0].is_some_and(|b| b.is_some())
        && p.direct.borders[2].is_some_and(|b| b.is_some())
    {
        out.push_str("```\n");
        for r in &p.runs {
            match &r.kind {
                FlowRunKind::Text(t) => out.push_str(t),
                FlowRunKind::LineBreak => out.push('\n'),
                FlowRunKind::Tab => out.push('\t'),
                _ => {}
            }
        }
        out.push_str("\n```\n\n");
        return;
    }

    // 4. Blockquote / Alert (left border only)
    if p.direct.borders[3].is_some_and(|b| b.is_some()) && p.direct.borders[0].is_none() {
        if let Some(first_run) = p.runs.first()
            && let FlowRunKind::Text(label) = &first_run.kind
            && matches!(
                label.as_str(),
                "NOTE" | "TIP" | "IMPORTANT" | "WARNING" | "CAUTION"
            )
            && first_run.format.bold == Some(true)
        {
            out.push_str(&format!("> [!{label}]\n"));
            let rest_runs: Vec<FlowRun> = p
                .runs
                .iter()
                .skip(1)
                .skip_while(|r| matches!(r.kind, FlowRunKind::LineBreak))
                .cloned()
                .collect();
            let body = serialize_runs(&rest_runs, media, comments);
            for line in body.lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
            return;
        }
        let body = serialize_runs(&p.runs, media, comments);
        for line in body.lines() {
            out.push_str("> ");
            out.push_str(line);
            out.push('\n');
        }
        out.push('\n');
        return;
    }

    let attr_suffix = format_block_attrs(p);

    // 5. Heading
    let heading_level = p
        .style
        .as_deref()
        .and_then(|s| {
            s.strip_prefix("Heading")
                .or_else(|| s.strip_prefix("heading "))
                .and_then(|n| n.trim().parse::<u8>().ok())
                .or_else(|| {
                    doc.styles
                        .styles
                        .get(s)
                        .and_then(|st| st.name.strip_prefix("heading "))
                        .and_then(|n| n.trim().parse::<u8>().ok())
                })
        });

    if let Some(lvl) = heading_level.filter(|&l| (1..=6).contains(&l)) {
        let hashes = "#".repeat(lvl as usize);
        let text = serialize_runs(&p.runs, media, comments);
        out.push_str(&format!("{hashes} {text}{attr_suffix}\n\n"));
        return;
    }

    // 6. Task list item (`FlowRunKind::CheckBox` at start of paragraph)
    if let Some(FlowRun {
        kind: FlowRunKind::CheckBox { checked, .. },
        ..
    }) = p.runs.first()
    {
        let rest_runs: Vec<FlowRun> = p.runs[1..].to_vec();
        let text = serialize_runs(&rest_runs, media, comments);
        let text = text.strip_prefix(' ').unwrap_or(&text);
        let mark = if *checked { "[x]" } else { "[ ]" };
        out.push_str(&format!("- {mark} {text}{attr_suffix}\n"));
        return;
    }

    // 7. Bullet or Ordered list item
    if let Some(ListRef { num_id, level }) = p.list {
        let indent = "  ".repeat(level as usize);
        let is_ordered = doc
            .styles
            .numbering
            .level(num_id, level)
            .is_some_and(|lvl| lvl.format != "bullet");
        let marker = if is_ordered { "1." } else { "-" };
        let text = serialize_runs(&p.runs, media, comments);
        out.push_str(&format!("{indent}{marker} {text}{attr_suffix}\n"));
        return;
    }

    // 8. Normal paragraph
    let text = serialize_runs(&p.runs, media, comments);
    if !text.is_empty() || !attr_suffix.is_empty() {
        out.push_str(&format!("{text}{attr_suffix}\n\n"));
    }
}

fn format_block_attrs(p: &FlowParagraph) -> String {
    let mut parts = Vec::new();
    if let Some(bm) = p.bookmarks.first().filter(|b| !b.starts_with("_Toc")) {
        parts.push(format!("#{bm}"));
    }
    if let Some(align) = p.direct.align {
        match align {
            FlowAlign::Center => parts.push("align=center".to_string()),
            FlowAlign::Right => parts.push("align=right".to_string()),
            FlowAlign::Justify => parts.push("align=justify".to_string()),
            FlowAlign::Distribute => parts.push("align=distribute".to_string()),
            FlowAlign::Left => {}
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" {{{}}}", parts.join(" "))
    }
}

fn serialize_table(
    t: &FlowTable,
    media: &[Media],
    comments: &HashMap<u32, &Comment>,
    out: &mut String,
) {
    if t.rows.is_empty() {
        return;
    }
    let col_count = t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    if col_count == 0 {
        return;
    }

    let format_row = |row: &FlowRow| -> String {
        let mut cells = Vec::with_capacity(col_count);
        for c_idx in 0..col_count {
            if let Some(cell) = row.cells.get(c_idx) {
                let mut cell_parts = Vec::new();
                for b in &cell.blocks {
                    if let Block::Paragraph(p) = b {
                        cell_parts.push(
                            serialize_runs(&p.runs, media, comments)
                                .replace('\n', "<br>")
                                .replace('|', "\\|"),
                        );
                    }
                }
                cells.push(cell_parts.join("<br>"));
            } else {
                cells.push(String::new());
            }
        }
        format!("| {} |\n", cells.join(" | "))
    };

    out.push_str(&format_row(&t.rows[0]));

    // Alignment separator row
    let mut sep_cells = Vec::with_capacity(col_count);
    for c_idx in 0..col_count {
        let align = t
            .rows
            .first()
            .and_then(|r| r.cells.get(c_idx))
            .and_then(|c| c.blocks.first())
            .and_then(|b| match b {
                Block::Paragraph(p) => p.direct.align,
                _ => None,
            });
        let marker = match align {
            Some(FlowAlign::Center) => ":---:",
            Some(FlowAlign::Right) => "---:",
            Some(FlowAlign::Left) => ":---",
            _ => "---",
        };
        sep_cells.push(marker);
    }
    out.push_str(&format!("| {} |\n", sep_cells.join(" | ")));

    for row in &t.rows[1..] {
        out.push_str(&format_row(row));
    }
    out.push('\n');
}

fn serialize_chart(chart: &Chart, out: &mut String) {
    out.push_str("```chart\n");
    let kind_name = match chart.groups.first().map(|g| (&g.kind, g.grouping)) {
        Some((ChartType::Bar { horizontal: false, .. }, Grouping::Standard)) => "column",
        Some((ChartType::Bar { horizontal: false, .. }, Grouping::Stacked)) => "stacked-column",
        Some((ChartType::Bar { horizontal: false, .. }, Grouping::PercentStacked)) => {
            "percent-stacked-column"
        }
        Some((ChartType::Bar { horizontal: true, .. }, Grouping::Standard)) => "bar",
        Some((ChartType::Bar { horizontal: true, .. }, Grouping::Stacked)) => "stacked-bar",
        Some((ChartType::Bar { horizontal: true, .. }, Grouping::PercentStacked)) => {
            "percent-stacked-bar"
        }
        Some((ChartType::Line, _)) => "line",
        Some((ChartType::Area, Grouping::Stacked | Grouping::PercentStacked)) => "stacked-area",
        Some((ChartType::Area, Grouping::Standard)) => "area",
        Some((ChartType::Pie { .. }, _)) => "pie",
        Some((ChartType::Doughnut { .. }, _)) => "doughnut",
        Some((ChartType::Radar { .. }, _)) => "radar",
        Some((ChartType::Scatter { .. }, _)) => "scatter",
        _ => "column",
    };
    out.push_str(&format!("type: {kind_name}\n"));
    if let Some(title) = &chart.title
        && !title.text.is_empty()
    {
        out.push_str(&format!("title: {}\n", quote_yaml_if_needed(&title.text)));
    }
    let all_series: Vec<&Series> = chart.groups.iter().flat_map(|g| &g.series).collect();
    if let Some(first) = all_series.first()
        && !first.categories.is_empty()
    {
        let cats: Vec<String> = first
            .categories
            .iter()
            .map(|c| quote_yaml_if_needed(c))
            .collect();
        out.push_str(&format!("categories: [{}]\n", cats.join(", ")));
    }
    if !all_series.is_empty() {
        out.push_str("series:\n");
        for s in all_series {
            let vals: Vec<String> = s
                .values
                .iter()
                .map(|v| match v {
                    Some(Number(n)) => {
                        if (*n - n.round()).abs() < 1e-9 {
                            format!("{}", *n as i64)
                        } else {
                            format!("{n}")
                        }
                    }
                    None => "0".to_string(),
                })
                .collect();
            out.push_str(&format!(
                "  - name: {}, values: [{}]\n",
                quote_yaml_if_needed(&s.name),
                vals.join(", ")
            ));
        }
    }
    out.push_str("```\n\n");
}

fn serialize_runs(
    runs: &[FlowRun],
    media: &[Media],
    comments: &HashMap<u32, &Comment>,
) -> String {
    let mut out = String::new();
    for run in runs {
        let mut piece = match &run.kind {
            FlowRunKind::Text(t) => format_text_run(t, &run.format, run.link.is_some()),
            FlowRunKind::LineBreak => "  \n".to_string(),
            FlowRunKind::Tab => "\t".to_string(),
            FlowRunKind::PageBreak => "\\newpage".to_string(),
            FlowRunKind::ColumnBreak => "\\columnbreak".to_string(),
            FlowRunKind::PageNumber => "{page}".to_string(),
            FlowRunKind::FileName(_) => "{filename}".to_string(),
            FlowRunKind::Math { math, display } => {
                let l = math.to_latex();
                if *display {
                    format!("$${l}$$")
                } else {
                    format!("${l}$")
                }
            }
            FlowRunKind::Ruby { base, text, .. } => format!("{{{base}|{text}}}"),
            FlowRunKind::NoteRef { id, .. } => format!("[^{id}]"),
            FlowRunKind::CommentRef(cid) => {
                if let Some(c) = comments.get(cid) {
                    let body: String = c
                        .blocks
                        .iter()
                        .filter_map(|b| match b {
                            Block::Paragraph(p) => Some(p.text()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    if c.author.is_empty() || c.author == "Author" {
                        format!("{{>>{body}<<}}")
                    } else {
                        format!("{{>>{}: {body}<<}}", c.author)
                    }
                } else {
                    String::new()
                }
            }
            FlowRunKind::Picture {
                media: mid,
                size,
                look,
            } => {
                if let Some(m) = media.iter().find(|m| m.id == *mid) {
                    let mime = match m.format {
                        MediaFormat::Png => "png",
                        MediaFormat::Jpeg => "jpeg",
                        MediaFormat::Gif => "gif",
                        MediaFormat::Bmp => "bmp",
                        MediaFormat::Svg => "svg+xml",
                        MediaFormat::Emf => "x-emf",
                        MediaFormat::Wmf => "x-wmf",
                        _ => "png",
                    };
                    let b64 = encode_base64(&m.data);
                    let alt = if !look.alt_text.description.is_empty() {
                        look.alt_text.description.as_str()
                    } else {
                        look.alt_text.title.as_str()
                    };
                    let w_mm = (size.width as f64 / EMU_PER_MM).round() as i64;
                    let h_mm = (size.height as f64 / EMU_PER_MM).round() as i64;
                    format!("![{alt}](data:image/{mime};base64,{b64}){{width={w_mm}mm height={h_mm}mm}}")
                } else {
                    String::new()
                }
            }
            _ => String::new(),
        };

        if let Some(link) = &run.link
            && !piece.is_empty()
        {
            let target = match link {
                Link::Url(u) => u.clone(),
                Link::Anchor(b) => format!("#{b}"),
            };
            piece = format!("[{piece}]({target})");
        }

        if let Some(rev) = &run.revision
            && !piece.is_empty()
        {
            match rev.kind {
                RevisionKind::Insert | RevisionKind::MoveTo => {
                    piece = format!("{{++{piece}++}}");
                }
                RevisionKind::Delete | RevisionKind::MoveFrom => {
                    piece = format!("{{--{piece}--}}");
                }
                _ => {}
            }
        }

        out.push_str(&piece);
    }
    out
}

fn format_text_run(text: &str, fmt: &RunFormat, is_link: bool) -> String {
    if text.is_empty() {
        return String::new();
    }
    // Inline code detection
    if fmt.font.as_deref() == Some("Consolas") && fmt.highlight.is_some_and(|h| h.is_some()) {
        return format!("`{text}`");
    }

    let mut s = text.to_string();
    if fmt.tate_chu_yoko == Some(true) {
        s = format!("[{s}]{{.tcy}}");
    }
    if fmt.emphasis.is_some() {
        s = format!("《《{s}》》");
    }
    match fmt.position {
        Some(VerticalPosition::Superscript) => s = format!("^{s}^"),
        Some(VerticalPosition::Subscript) => s = format!("~{s}~"),
        _ => {}
    }
    if fmt.highlight.is_some_and(|h| h.is_some()) {
        s = format!("=={s}==");
    }
    if fmt.underline == Some(true) && !is_link {
        s = format!("++{s}++");
    }
    if fmt.strike == Some(true) || fmt.double_strike == Some(true) {
        s = format!("~~{s}~~");
    }
    match (fmt.bold == Some(true), fmt.italic == Some(true)) {
        (true, true) => s = format!("***{s}***"),
        (true, false) => s = format!("**{s}**"),
        (false, true) => s = format!("*{s}*"),
        (false, false) => {}
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_markdown_round_trip_preserves_metafile_bytes() {
        for (format, mime, original) in [
            (MediaFormat::Emf, "x-emf", include_bytes!("../../ibis-emf/tests/fixtures/shapes.emf").as_slice()),
            (MediaFormat::Wmf, "x-wmf", include_bytes!("../../ibis-emf/tests/fixtures/shapes.wmf").as_slice()),
        ] {
            let snippet = format!(
                "![drawing](data:image/{mime};base64,{}){{width=40mm height=30mm}}\n",
                encode_base64(original)
            );
            let package = import_markdown_to_docx_bytes(snippet.as_bytes()).unwrap();
            let opened = crate::open(package).unwrap();
            let markdown = export_presentation_to_markdown(&opened.presentation).unwrap();
            assert!(markdown.contains(&format!("data:image/{mime};base64,")));
            let rebuilt = crate::open(import_markdown_to_docx_bytes(markdown.as_bytes()).unwrap()).unwrap();
            assert!(rebuilt.presentation.media.iter().any(|media| {
                media.format == format && media.data.as_ref() == original
            }));
        }
    }

    #[test]
    fn fixed_layout_markdown_comments_do_not_become_document_text() {
        let markdown = b"<!-- fixed-layout-page: 1 -->\n\nShared document\n\n<!-- fixed-layout-diagnostic: Page 1: no extractable text -->\n";
        let opened = crate::open(import_markdown_to_docx_bytes(markdown).unwrap()).unwrap();
        let flow = opened.presentation.flow.unwrap();
        let text = ibis_doc_model::flow_command::all_paragraphs(&flow).iter().map(|p| p.text()).collect::<Vec<_>>();
        assert_eq!(text, ["Shared document"]);
    }

    #[test]
    fn detects_markdown_vs_csv() {
        assert!(is_markdown(b"# Heading 1\n\nSome paragraph."));
        assert!(is_markdown(b"---\ntitle: Test\n---\nHello"));
        assert!(is_markdown(b"| A | B |\n| --- | --- |\n| 1 | 2 |\n"));
        assert!(!is_markdown(b"name,age,city\nAlice,30,Tokyo\nBob,25,Osaka\n"));
        assert!(!is_markdown(b"PK\x03\x04\x14\x00"));
    }

    #[test]
    fn rich_markdown_roundtrips_and_saves_to_valid_docx() {
        let md = r#"---
title: "2026年度 技術戦略レポート"
subtitle: "Cloudflare Artifacts × ibisOffice"
author: "masanork"
keywords: "markdown, ooxml, gitops"
size: A4
columns: 2
column_separator: true
style_set: modern
paragraph_spacing: relaxed
header: "機密文書 - {filename}"
footer: "{page}"
watermark: "社外秘"
track_changes: true
---

# 第1章 はじめに {#intro align=center}

本書は **太字**、*斜体*、***太字斜体***、~~取消線~~、++下線++、==蛍光ペン==、上付き x^2^、下付き H~2~O、および `inline_code()` を備えたリッチ Markdown 文書です[^1]。
また日本語組版として {漢字|かんじ} や ｜青空文庫《あおぞらぶんこ》、傍点《《強調表現》》、縦中横 [26]{.tcy}、リンク [sorane](https://docs.sorane.dev) にも対応しています。

変更履歴として {++追加されたテキスト++} と {--削除されたテキスト--}、およびレビューコメント対象語句{>>masanork: ここを要確認<<}を含みます。

> [!IMPORTANT]
> Git リポジトリ上の Markdown を直接 ibisOffice のページレイアウトで編集・保存できます。

- 箇条書きアイテム 1
  - ネストされたアイテム 1.1
- [ ] 未完了タスク
- [x] 完了済みタスク

1. 番号付きリスト 1
2. 番号付きリスト 2

$$
\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

```rust
fn main() {
    println!("Hello, ibisOffice Markdown!");
}
```

| 項目 | 2025年度 | 2026年度 |
| :--- | :---: | ---: |
| 売上高 | 120 | 180 |
| 営業利益 | 30 | 55 |

```chart
type: column
title: 年度別業績推移
categories: [Q1, Q2, Q3, Q4]
series:
  - name: 2025年度, values: [25, 30, 28, 37]
  - name: 2026年度, values: [40, 45, 42, 53]
```

\newpage

## 第2章 まとめ

インライン数式 $E = mc^2$ のテストです。

[^1]: 脚注の本文です。**書式**も使えます。
"#;

        // 1. Open as rich Markdown
        let opened = open_markdown(md.as_bytes().to_vec()).expect("open_markdown succeeds");
        let flow = opened.presentation.flow.as_ref().expect("flow document present");

        assert_eq!(flow.properties.title.as_deref(), Some("2026年度 技術戦略レポート"));
        assert_eq!(flow.properties.creator.as_deref(), Some("masanork"));
        assert_eq!(flow.page.columns.count, 2);
        assert!(flow.page.columns.separator);
        assert_eq!(flow.watermark.as_ref().map(|w| w.text.as_str()), Some("社外秘"));
        assert!(flow.track_changes);
        assert_eq!(flow.footnotes.len(), 1);
        assert_eq!(flow.comments.len(), 1);

        // 2. Save to .docx and re-read via read_docx to verify 100% OOXML compatibility!
        let docx_bytes = opened
            .source
            .save(&opened.presentation, SaveOptions::default())
            .expect("saving Markdown-opened document to .docx succeeds");
        let reread = crate::read_docx(&docx_bytes).expect("re-reading saved .docx succeeds");
        let reread_flow = reread.flow.as_ref().expect("reread flow");

        assert_eq!(reread_flow.properties.title.as_deref(), Some("2026年度 技術戦略レポート"));
        assert_eq!(reread_flow.page.columns.count, 2);
        assert_eq!(reread_flow.footnotes.len(), 1);
        assert_eq!(reread_flow.comments.len(), 1);
        assert_eq!(
            reread_flow.watermark.as_ref().map(|w| w.text.as_str()),
            Some("社外秘")
        );

        // Verify chart survived .docx save & reread with custom title and 2 series!
        let mut found_chart = None;
        for b in &reread_flow.body {
            if let Block::Paragraph(p) = b {
                for r in &p.runs {
                    if let FlowRunKind::Shape(s) = &r.kind
                        && let ShapeKind::Chart(c) = &s.kind
                    {
                        found_chart = Some((**c).clone());
                    }
                }
            }
        }
        let chart = found_chart.expect("chart saved and reread from .docx");
        assert_eq!(chart.title.as_ref().map(|t| t.text.as_str()), Some("年度別業績推移"));
        let series: Vec<&Series> = chart.groups.iter().flat_map(|g| &g.series).collect();
        assert_eq!(series.len(), 2);
        assert_eq!(series[0].name, "2025年度");
        assert_eq!(series[1].name, "2026年度");
        assert_eq!(series[0].categories, vec!["Q1", "Q2", "Q3", "Q4"]);

        // 3. Export back to Markdown and verify key constructs roundtrip
        let exported_md = export_markdown(flow, &opened.presentation.media);
        assert!(exported_md.contains("title: 2026年度 技術戦略レポート"));
        assert!(exported_md.contains("columns: 2"));
        assert!(exported_md.contains("# 第1章 はじめに {#intro align=center}"));
        assert!(exported_md.contains("{漢字|かんじ}"));
        assert!(exported_md.contains("《《強調表現》》"));
        assert!(exported_md.contains("[26]{.tcy}"));
        assert!(exported_md.contains("{++追加されたテキスト++}"));
        assert!(exported_md.contains("{--削除されたテキスト--}"));
        assert!(exported_md.contains("{>>masanork: ここを要確認<<}"));
        assert!(exported_md.contains("> [!IMPORTANT]"));
        assert!(exported_md.contains("- [ ] 未完了タスク"));
        assert!(exported_md.contains("- [x] 完了済みタスク"));
        assert!(exported_md.contains("```chart"));
        assert!(exported_md.contains("[^1]: 脚注の本文です。**書式**も使えます。"));
    }

    #[test]
    fn detects_markdown_edge_cases() {
        assert!(is_markdown(b"1. First item\n2. Second item\n"));
        assert!(is_markdown(b"Hello\n\n---\n\nWorld\n"));
        assert!(is_markdown(b"Use `cargo test` to verify.\n"));
        assert!(is_markdown(b"This is ~~deprecated~~ and ==important==.\n"));
        assert!(!is_markdown(b""));
        assert!(!is_markdown(b"   \n\t\n"));
        assert!(!is_markdown(b"col1,col2,col3\n1,2,3\n4,5,6\n"));
        assert!(!is_markdown(b"plain text without any markdown markers\njust two lines\n"));
        assert!(!is_markdown(b"hello\x00world"));
    }

    #[test]
    fn loads_local_relative_images_and_rejects_traversal() {
        let dir = std::env::temp_dir().join(format!("ibis_md_img_test_{}", std::process::id()));
        let img_dir = dir.join("images");
        std::fs::create_dir_all(&img_dir).unwrap();

        // Minimal PNG header + IHDR (400x200) so detect_image_format & infer_image_dimensions_px succeed.
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&400u32.to_be_bytes());
        png.extend_from_slice(&200u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        png.extend_from_slice(&[0; 4]);
        let img_path = img_dir.join("chart.png");
        std::fs::write(&img_path, &png).unwrap();

        let outside = dir.parent().unwrap().join("ibis_md_outside_test.png");
        std::fs::write(&outside, &png).unwrap();

        let md = format!(
            "# Image Test\n\n![sample](images/chart.png)\n\n![escape](../ibis_md_outside_test.png)\n\n![abs]({})\n",
            outside.display()
        );
        let opened = open_markdown_with_base_dir(md.into_bytes(), Some(&dir)).expect("open_markdown_with_base_dir");
        let flow = opened.presentation.flow.as_ref().unwrap();

        let mut pictures = Vec::new();
        for b in &flow.body {
            if let Block::Paragraph(p) = b {
                for r in &p.runs {
                    if let FlowRunKind::Picture { media, size, look } = &r.kind {
                        pictures.push((*media, *size, look.alt_text.description.clone()));
                    }
                }
            }
        }
        assert_eq!(pictures.len(), 1, "only relative image inside base_dir should be loaded");
        assert_eq!(pictures[0].2, "sample");
        // 400x200 => 2:1 aspect ratio at 96 DPI (9525 EMU/px)
        assert_eq!(pictures[0].1.width, 400 * 9_525);
        assert_eq!(pictures[0].1.height, 200 * 9_525);

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn fragment_blocks_roundtrip_for_clipboard() {
        let opened = crate::save::open(crate::blank::blank_file(crate::Kind::Document).unwrap()).unwrap();
        let mut doc = (**opened.presentation.flow.as_ref().unwrap()).clone();
        let mut media = opened.presentation.media.clone();

        let fragment = "## Subheading\n\nText with **bold**, ~~strike~~, and `code`.\n\n- item A\n- item B\n\n| K | V |\n| :--- | ---: |\n| x | 42 |\n\n```mermaid\npie\n  \"A\" : 70\n  \"B\" : 30\n```\n";
        let blocks = parse_markdown_fragment_blocks(fragment, &mut doc, &mut media).expect("parse fragment");
        assert!(blocks.len() >= 5);
        assert!(media.iter().any(|m| m.format == MediaFormat::Svg), "pie fragment should fall back to SVG");

        let exported = export_blocks_to_markdown(&blocks, &doc, &media);
        assert!(exported.contains("## Subheading"));
        assert!(exported.contains("**bold**"));
        assert!(exported.contains("~~strike~~"));
        assert!(exported.contains("`code`"));
        assert!(exported.contains("- item A"));
        assert!(exported.contains("| x | 42 |"));
        assert!(exported.contains("```mermaid\npie"));
    }

    #[test]
    fn parses_and_exports_alerts_nested_lists_and_tables() {
        let md = "\
> [!NOTE]
> Note body

> [!WARNING]
> Warning body

- level 0
  - level 1
    - level 2

| Left | Center | Right |
| :--- | :---: | ---: |
| a | b | c |
";
        let pres = read_markdown(md.as_bytes()).expect("read_markdown");
        let flow = pres.flow.as_ref().unwrap();
        let out = export_markdown(flow, &pres.media);
        assert!(out.contains("> [!NOTE]"));
        assert!(out.contains("> [!WARNING]"));
        assert!(out.contains("- level 0"));
        assert!(out.contains("  - level 1"));
        assert!(out.contains("    - level 2"));
        assert!(out.contains("| **Left** | **Center** | **Right** |"));
        assert!(out.contains("| :--- | :---: | ---: |"));
        assert!(out.contains("| a | b | c |"));
    }

    #[test]
    fn mermaid_diagrams_roundtrip_svg_and_native_charts() {
        let md = "\
# Mermaid

```mermaid
sequenceDiagram
  User->>API: Request
  API-->>User: Response
```

```mermaid
flowchart LR
  A[Start] --> B{Check}
  B -->|OK| C((Done))
```

```mermaid
pie showData
  title Share
  \"Chrome\" : 65
  \"Other\" : 35
```
";
        let opened = open_markdown(md.as_bytes().to_vec()).expect("open_markdown");
        let pres = &opened.presentation;
        let flow = pres.flow.as_ref().unwrap();

        let svg_count = pres
            .media
            .iter()
            .filter(|m| m.format == MediaFormat::Svg)
            .count();
        assert_eq!(svg_count, 2, "sequenceDiagram and flowchart should produce 2 SVG media items");

        let has_native_chart = flow.body.iter().any(|b| {
            matches!(
                b,
                Block::Paragraph(p)
                    if p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Shape(s) if matches!(s.kind, ShapeKind::Chart(_))))
            )
        });
        assert!(has_native_chart, "pie diagram should produce a native Office chart");

        // Verify .docx save + reread works cleanly
        let saved = opened.source.save(pres, Default::default()).expect("save docx");
        let reread = crate::read(&saved).expect("reread docx");
        assert!(reread.media.iter().any(|m| m.format == MediaFormat::Svg));

        // Verify Markdown export preserves mermaid source for SVG diagrams and chart YAML for native charts
        let exported = export_markdown(flow, &pres.media);
        assert!(exported.contains("```mermaid\nsequenceDiagram"));
        assert!(exported.contains("```mermaid\nflowchart LR"));
        assert!(exported.contains("```chart\ntype: pie"));
    }

    #[test]
    fn plantuml_fences_are_drawn_as_svg_and_written_back() {
        let md = "# PlantUML\n\n```plantuml\n@startuml\nAlice -> Bob: Hello\nBob --> Alice: Hi\n@enduml\n```\n\n```puml\n@startmindmap\n* Root\n** Child\n@endmindmap\n```\n";
        let opened = open_markdown(md.as_bytes().to_vec()).expect("open_markdown");
        let pres = &opened.presentation;
        let flow = pres.flow.as_ref().unwrap();
        let svg_count = pres.media.iter().filter(|m| m.format == MediaFormat::Svg).count();
        assert_eq!(svg_count, 2, "both PlantUML blocks should be drawn as SVG");
        let exported = export_markdown(flow, &pres.media);
        assert!(exported.contains("```plantuml\n@startuml\nAlice -> Bob: Hello"), "{exported}");
        assert!(exported.contains("```puml\n@startmindmap"), "{exported}");
    }
}
