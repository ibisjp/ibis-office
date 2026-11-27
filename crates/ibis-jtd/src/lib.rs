// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Conservative, read-only JTD text import. No OpenJTD code is used.
//!
//! Format observations: OpenJTD RFC 0003 and 0009 at cfb44c9ac53126e00808c49709fb4dade8a4daae,
//! https://github.com/OpenJTD/rjtd/tree/cfb44c9ac53126e00808c49709fb4dade8a4daae/openjtd-spec/rfc
//! Table candidates also use local RFC 0009 observations at 28927abb6b103dc70453f6443a7da961acfccb92.
//! Rule data follows the independent observations documented in docs/jtd-format-notes.md.
//! These are incomplete research notes, not a complete specification.
use ibis_formats::{Error, Kind};
mod decode;
mod font;
mod compressed;
mod embedded;
mod frame;
mod thumbnail;
pub use embedded::{EmbeddedMetafile, EmbeddedMetafileInspection, inspect_embedded_metafiles};
pub use frame::{FrameCandidate, FrameLink, inspect_frame_links};
pub use thumbnail::{ThumbnailCandidate, decode_thumbnail_bmp, inspect_thumbnail};
pub use decode::{DecodedText, Diagnostic, Piece, Record, RecordKind, StyleSpan};
pub use font::FontRecord;
use ibis_doc_model::{
    Block, BorderStyle, CellDirection, FlowBorder, FlowCell, FlowRow, FlowRowLine, FlowRun,
    FlowRunKind, FlowRunStyle, FlowTable, Presentation, Rgb, RowHeight, RubyAlign,
};

const MAX_FILE: usize = 64 * 1024 * 1024;
const MAX_STREAM: usize = 32 * 1024 * 1024;
const NO_VISIBLE_CONTENT: &str = "表示できる一太郎本文を読み取れませんでした。画像・図形・埋め込みは現在未対応です";

/// A text preview, or unplaced image candidates when the decoded body has no visible content.
pub enum Preview {
    Document(Presentation),
    UnplacedImages(EmbeddedMetafileInspection),
}

/// Preserve ordinary import errors; only the specifically recognized empty-body
/// case can fall back to an explicitly unplaced image inspection.
pub fn read_preview(data: &[u8]) -> Result<Preview, Error> {
    match read(data) {
        Ok(document) => {
            // A truly empty text stream is accepted by the regular reader.
            // Only choose the gallery if it also contains known image candidates.
            let empty = document.flow.as_ref().is_some_and(|flow| flow.body.iter().all(|block| {
                matches!(block, Block::Paragraph(paragraph) if paragraph.runs.is_empty())
            }));
            if empty {
                if let Ok(candidates) = inspect_embedded_metafiles(data) {
                    if !candidates.metafiles.is_empty() { return Ok(Preview::UnplacedImages(candidates)); }
                }
            }
            Ok(Preview::Document(document))
        }
        Err(error) if error.to_string() == NO_VISIBLE_CONTENT => {
            let candidates = inspect_embedded_metafiles(data)?;
            if candidates.metafiles.is_empty() { Err(error) }
            else { Ok(Preview::UnplacedImages(candidates)) }
        }
        Err(error) => Err(error),
    }
}

/// Supported Ichitaro document or template extension (case-insensitive).
pub fn is_extension(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("jtd") || extension.eq_ignore_ascii_case("jtt") || extension.eq_ignore_ascii_case("jttc")
}

/// Never label a DOCX conversion as a native Ichitaro format, even if this
/// importer does not currently offer that compressed variant in the picker.
pub fn is_native_extension(extension: &str) -> bool {
    is_extension(extension) || extension.eq_ignore_ascii_case("jtdc")
}

fn import_run(kind: FlowRunKind, style: FlowRunStyle) -> FlowRun {
    // Drawn style alone is transient: resolving paragraph styles or saving
    // DOCX uses direct RunFormat. Preserve every attribute decoded here.
    let format = ibis_doc_model::RunFormat {
        font: style.font.clone(),
        east_asian_font: style.east_asian_font.clone(),
        size: Some(style.size),
        bold: Some(style.bold),
        italic: Some(style.italic),
        underline: Some(style.underline),
        underline_style: Some(style.underline_style),
        color: Some(style.color),
        highlight: Some(style.highlight),
        spacing: Some(style.spacing),
        scale: Some(style.scale),
        ..Default::default()
    };
    FlowRun {
        format,
        ..FlowRun::new(kind, style)
    }
}

/// Identify a supported JTD by its container and named document stream.
pub fn is_jtd(data: &[u8]) -> bool {
    checked_stream(data).is_ok_and(|s| s.starts_with(b"SsmgV.01"))
        || (data.len() <= MAX_FILE && ibis_ooxml::cfb::stream_path(data, "JSCompDocument").is_ok_and(|s| compressed::is_wrapper(&s)))
}

fn source_document(data: &[u8]) -> Result<std::borrow::Cow<'_, [u8]>, Error> {
    if data.len() > MAX_FILE { return Err(Error::new("一太郎ファイルが大きすぎます")); }
    let entries = ibis_ooxml::cfb::directory(data).map_err(|_| Error::new("対応する一太郎コンテナではありません"))?;
    if entries.iter().any(|entry| !entry.is_storage && entry.path == "DocumentText") {
        // A malformed or over-limit root stream must not silently fall back
        // to a second, unrelated compressed document in the same container.
        checked_stream(data)?;
        return Ok(std::borrow::Cow::Borrowed(data));
    }
    let stream = ibis_ooxml::cfb::stream_path(data, "JSCompDocument")
        .map_err(|_| Error::new("一太郎本文がないか、暗号化・未対応の形式です"))?;
    if stream.len() > MAX_STREAM { return Err(Error::new("圧縮一太郎文書が大きすぎます")); }
    let inner = compressed::decompress(&stream, MAX_FILE)?;
    checked_stream(&inner)?;
    Ok(std::borrow::Cow::Owned(inner))
}

/// Decode the font table without assuming that inline character selectors
/// use the same index namespace.
pub fn inspect_fonts(data: &[u8]) -> Result<Vec<FontRecord>, Error> {
    let data = source_document(data)?;
    checked_stream(&data)?;
    font_table(&data)
}

fn font_table(data: &[u8]) -> Result<Vec<FontRecord>, Error> {
    let bytes = ibis_ooxml::cfb::stream_path(data, "Font")
        .map_err(|_| Error::new("JTDのフォント表がありません"))?;
    font::parse(&bytes)
}

fn default_font<'a>(styles: &[u8], fonts: &'a [FontRecord]) -> Option<&'a FontRecord> {
    let records = view_records(styles)?;
    let mut settings = records.iter().filter(|(tag, _)| *tag == 0x1006);
    let record = settings.next()?.1;
    if settings.next().is_some() || record.first()? & 6 != 6 {
        return None;
    }
    let at = 1 + 4 * usize::from(record[0] & 1 != 0);
    let a = u16::from_be_bytes(record.get(at..at + 2)?.try_into().ok()?);
    let b = u16::from_be_bytes(record.get(at + 2..at + 4)?.try_into().ok()?);
    let primary = fonts.iter().find(|font| font.index == a)?;
    let secondary = fonts.iter().find(|font| font.index == b)?;
    // Their separate roles are not established. Apply a shared face only
    // when both explicitly stored references resolve to the same typography.
    (primary.face == secondary.face
        && primary.weight == secondary.weight
        && primary.italic == secondary.italic
        && primary.style == secondary.style
        && primary.charset == secondary.charset)
        .then_some(primary)
}

fn checked_stream(data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.len() > MAX_FILE {
        return Err(Error::new("JTDファイルが大きすぎます"));
    }
    if data.len() < 512
        || !ibis_ooxml::cfb::is_cfb(data)
        || !matches!(u16::from_le_bytes([data[30], data[31]]), 9 | 12)
        || u16::from_le_bytes([data[32], data[33]]) != 6
    {
        return Err(Error::new("対応するJTDコンテナではありません"));
    }
    let stream = ibis_ooxml::cfb::stream_path(data, "DocumentText")
        .map_err(|_| Error::new("JTD本文がないか、暗号化・未対応の形式です"))?;
    if stream.len() > MAX_STREAM {
        return Err(Error::new("JTD本文が大きすぎます"));
    }
    Ok(stream)
}

/// Known content that the preview cannot currently import. Counts report
/// detected container entries, not all possible unsupported JTD constructs.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OmittedContent {
    pub additional_sheets: usize,
    pub embedded_objects: usize,
}

pub fn omitted_content(data: &[u8]) -> Result<OmittedContent, Error> {
    let data = source_document(data)?;
    let entries = ibis_ooxml::cfb::directory(&data).map_err(|e| Error::new(e.to_string()))?;
    let mut result = OmittedContent::default();
    for entry in entries {
        let path = entry.path.as_str();
        if !entry.is_storage && path.starts_with("ObjectSheets/DocSheet/") && path.ends_with("/DocumentText") {
            let sheet = &path["ObjectSheets/DocSheet/".len()..path.len() - "/DocumentText".len()];
            if sheet.starts_with("DOCS_") && !sheet.contains('/') { result.additional_sheets += 1; }
        }
        if entry.is_storage {
            let name = path.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
            if name.starts_with("embedding") || name.starts_with("oleitem") || name == "figuredata" {
                result.embedded_objects += 1;
            }
        }
    }
    Ok(result)
}

/// Import only confirmed text into the existing document model. This is lossy
/// and must never be used as the source of a JTD save.
pub fn read(data: &[u8]) -> Result<Presentation, Error> {
    let data = source_document(data)?;
    let data = data.as_ref();
    let stream = checked_stream(data)?;
    let decoded = decode::decode(&stream)?;
    let mut presentation = ibis_formats::read(&ibis_formats::blank_file(Kind::Document)?)?;
    let flow = std::sync::Arc::make_mut(
        presentation
            .flow
            .as_mut()
            .ok_or_else(|| Error::new("文書モデルを作成できません"))?,
    );
    // Missing view-style fields inherit Ichitaro's defaults, not the
    // unrelated margins/grid of the blank Word template.
    flow.page.top = 30 * 36_000;
    flow.page.bottom = 30 * 36_000;
    flow.page.left = 30 * 36_000;
    flow.page.right = 30 * 36_000;
    let mut page_lines = 40;
    // Page geometry must be installed before table widths are derived.
    if let Ok(styles) = ibis_ooxml::cfb::stream_path(data, "DocumentViewStyles") {
        apply_view_page(&styles, &mut flow.page);
        if observed_vertical(&styles) {
            flow.page.direction = CellDirection::TbRl;
        }
        page_lines = view_style_lines(&styles).unwrap_or(40);
    }
    let cross_axis = if flow.page.direction.vertical() {
        flow.page.size.width - flow.page.left - flow.page.right
    } else {
        flow.page.size.height - flow.page.top - flow.page.bottom
    };
    flow.page.line_pitch = Some(cross_axis / i64::from(page_lines));
    flow.page.doc_grid_type = ibis_doc_model::DocGridType::Lines;
    let typography = ibis_ooxml::cfb::stream_path(data, "DocumentViewStyles").unwrap_or_default();
    let mut defaults = view_typography(&typography, &mut flow.page);
    if let Ok(fonts) = font_table(data) {
        if let Some(font) = default_font(&typography, &fonts) {
            defaults.font = Some(font.face.clone());
            defaults.east_asian_font = Some(font.face.clone());
            defaults.bold = font.weight >= 700;
            defaults.italic = font.italic;
        }
    }
    {
        let styles = std::sync::Arc::make_mut(&mut flow.styles);
        styles.defaults_run.size = Some(defaults.size);
        if defaults.font.is_some() {
            styles.defaults_run.font = defaults.font.clone();
            styles.defaults_run.east_asian_font = defaults.east_asian_font.clone();
            styles.defaults_run.bold = Some(defaults.bold);
            styles.defaults_run.italic = Some(defaults.italic);
        }
    }
    let segments = table_candidates(extract_decoded(&decoded, &defaults, flow.page.line_pitch)?);
    let mut template = match flow.body.first() {
        Some(Block::Paragraph(p)) => p.clone(),
        _ => return Err(Error::new("文書の段落を作成できません")),
    };
    template.mark = defaults.clone();
    flow.body.clear();
    let mut paragraph = template.clone();
    paragraph.runs.clear();
    // Retain the blank source's paragraph anchor on the first real body
    // paragraph (or the required trailing paragraph after tables).
    let mut original_paragraph_available = true;
    let mut next_id = 2u32;
    let mut last_table_key: Option<u16> = None;
    for segment in segments {
        match segment {
            Segment::ParagraphFormat(format) => {
                template.direct.overlay(&format);
                let direct = template.direct.clone();
                direct.apply_to(&mut template);
                if paragraph.runs.is_empty() {
                    paragraph.direct.overlay(&format);
                    let direct = paragraph.direct.clone();
                    direct.apply_to(&mut paragraph);
                }
            }
            Segment::Table {
                cells,
                total,
                strips,
                rules,
                row_alignment,
            } => {
                if !paragraph.runs.is_empty() {
                    paragraph.id = if std::mem::replace(&mut original_paragraph_available, false) {
                        1
                    } else {
                        next_id
                    };
                    next_id += 1;
                    flow.body.push(Block::Paragraph(paragraph));
                    paragraph = template.clone();
                    paragraph.runs.clear();
                }
                let table_key = if strips { Some(total) } else { None };
                let total = total as i64;
                let width = text_axis_width(&flow.page);
                let (boundaries, cells) = table_grid(cells, rules.as_ref(), total as u16);
                let line = FlowBorder {
                    width: 7_620,
                    color: Rgb::BLACK,
                    style: BorderStyle::Single,
                    space: 0,
                    art: None,
                };
                let borders = [Some(None); 6];
                let mut table = FlowTable {
                    id: next_id,
                    borders,
                    columns: boundaries
                        .windows(2)
                        .map(|p| i64::from(p[1]) * width / total - i64::from(p[0]) * width / total)
                        .collect(),
                    fit_to_page: true,
                    ..FlowTable::default()
                };
                next_id += 1;
                let mut row = FlowRow {
                    id: next_id,
                    ..FlowRow::default()
                };
                if let Some(rules) = &rules {
                    if let Some(pitch) = flow.page.line_pitch {
                        row.height = RowHeight::Exact(match template.line {
                            ibis_doc_model::LineHeight::Exact(feed) => feed,
                            _ => pitch,
                        });
                    }
                    row.lines = rule_row_lines(rules, total as u16, line, &cells, &boundaries);
                    // Line segments are relative to one display row and
                    // cannot yet be split across pages without clipping.
                    row.cant_split = !row.lines.is_empty();
                }
                next_id += 1;
                for (left, right, start, stop, contents) in cells {
                    let mut cell_para = template.clone();
                    cell_para.id = next_id;
                    next_id += 1;
                    cell_para.runs.clear();
                    // A ruled display row starts a new source paragraph. Its
                    // alignment must not inherit a preceding paragraph's
                    // right/centre alignment. Preserve a value explicitly
                    // present in this row's own header.
                    if flow.page.direction.vertical() && rules.is_some() {
                        let align = row_alignment.unwrap_or_default();
                        cell_para.align = align;
                        cell_para.direct.align = Some(align);
                    }
                    // A JTD ruled display row is one source line. On a vertical
                    // page its fixed row height becomes the column width. If
                    // the paragraph keeps Word's automatic line height, the
                    // font's ascent can put the glyph across the row border.
                    // Match the paragraph's baseline to the source row, but
                    // preserve an explicitly decoded line spacing.
                    if flow.page.direction.vertical()
                        && rules.is_some()
                        && !matches!(cell_para.line, ibis_doc_model::LineHeight::Exact(_))
                    {
                        if let RowHeight::Exact(pitch) = row.height {
                            if pitch > 0 {
                                let spacing = ibis_doc_model::LineHeight::Exact(pitch);
                                cell_para.line = spacing;
                                cell_para.direct.line = Some(spacing);
                            }
                        }
                    }
                    for item in contents {
                        match item {
                            item @ (Segment::Text(_) | Segment::StyledText(..)) => {
                                let (text, style) = item.into_text(&defaults);
                                let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                                let mut pending = String::new();
                                for ch in normalized.chars() {
                                    if matches!(ch, '\n' | '\t') {
                                        if !pending.is_empty() {
                                            cell_para.runs.push(import_run(
                                                FlowRunKind::Text(std::mem::take(&mut pending)),
                                                style.clone(),
                                            ));
                                        }
                                        let kind = if ch == '\n' {
                                            FlowRunKind::LineBreak
                                        } else {
                                            FlowRunKind::Tab
                                        };
                                        cell_para.runs.push(import_run(kind, style.clone()));
                                    } else {
                                        pending.push(ch);
                                    }
                                }
                                if !pending.is_empty() {
                                    cell_para.runs.push(import_run(
                                        FlowRunKind::Text(pending),
                                        style.clone(),
                                    ));
                                }
                            }
                            Segment::Ruby {
                                base,
                                reading,
                                style,
                                ruby_size,
                            } => cell_para.runs.push(import_run(
                                FlowRunKind::Ruby {
                                    base,
                                    text: reading,
                                    size: ruby_size,
                                    raise: style.size * 9 / 10,
                                    align: RubyAlign::Center,
                                },
                                *style,
                            )),
                            Segment::PageBreak => cell_para
                                .runs
                                .push(import_run(FlowRunKind::PageBreak, defaults.clone())),
                            _ => {}
                        }
                    }
                    let own_borders = rules.as_ref().map_or([None; 4], |rules| {
                        rule_cell_borders(rules, left, right, line)
                    });
                    row.cells.push(FlowCell {
                        id: next_id,
                        span: (stop - start) as u32,
                        margins: [
                            0,
                            (i64::from(boundaries[stop]) - i64::from(right)).max(0) * width / total,
                            0,
                            (i64::from(left) - i64::from(boundaries[start])).max(0) * width / total,
                        ],
                        own_borders,
                        blocks: vec![Block::Paragraph(cell_para)],
                        ..FlowCell::default()
                    });
                    next_id += 1;
                }
                table.rows.push(row);
                table.resolve_borders();
                let mut combined = false;
                if paragraph.runs.is_empty() {
                    if let Some(Block::Table(previous)) = flow.body.last_mut() {
                        if previous.columns == table.columns && previous.borders == table.borders {
                            previous.rows.extend(table.rows.clone());
                            combined = true;
                        } else if table_key.is_some()
                            && table_key == last_table_key
                            && previous.borders == table.borders
                        {
                            combined = merge_table_grids(previous, &table);
                        }
                    }
                }
                if !combined {
                    flow.body.push(Block::Table(table));
                }
                last_table_key = table_key;
            }
            Segment::RowStart(_)
            | Segment::RowAlignment(_)
            | Segment::RowRules(_)
            | Segment::CellStart(_, _)
            | Segment::RowEnd => {
                last_table_key = None;
            }
            item @ (Segment::Text(_) | Segment::StyledText(..)) => {
                let (text, style) = item.into_text(&defaults);
                last_table_key = None;
                let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
                let mut lines = normalized.split('\n').peekable();
                while let Some(line) = lines.next() {
                    for (index, part) in line.split('\t').enumerate() {
                        if index > 0 {
                            paragraph
                                .runs
                                .push(import_run(FlowRunKind::Tab, style.clone()));
                        }
                        if !part.is_empty() {
                            paragraph.runs.push(import_run(
                                FlowRunKind::Text(part.to_string()),
                                style.clone(),
                            ));
                        }
                    }
                    if lines.peek().is_some() {
                        paragraph.id =
                            if std::mem::replace(&mut original_paragraph_available, false) {
                                1
                            } else {
                                next_id
                            };
                        next_id = next_id
                            .checked_add(1)
                            .ok_or_else(|| Error::new("段落が多すぎます"))?;
                        flow.body.push(Block::Paragraph(paragraph));
                        paragraph = template.clone();
                        paragraph.runs.clear();
                    }
                }
            }
            Segment::PageBreak => {
                last_table_key = None;
                paragraph
                    .runs
                    .push(import_run(FlowRunKind::PageBreak, defaults.clone()));
            }
            Segment::Ruby {
                base,
                reading,
                style,
                ruby_size,
            } => {
                last_table_key = None;
                // Typography is a Word default approximation; JTD's ruby
                // size and positioning have not yet been decoded.
                paragraph.runs.push(import_run(
                    FlowRunKind::Ruby {
                        base,
                        text: reading,
                        size: ruby_size,
                        raise: style.size * 9 / 10,
                        align: RubyAlign::Center,
                    },
                    *style,
                ));
            }
        }
    }
    paragraph.id = if std::mem::replace(&mut original_paragraph_available, false) {
        1
    } else {
        next_id
    };
    flow.body.push(Block::Paragraph(paragraph));
    flow.id_floor = next_id;
    Ok(presentation)
}

/// The table grid is laid out along the text axis before the page is turned.
fn text_axis_width(page: &ibis_doc_model::PageSetup) -> i64 {
    if page.direction.vertical() {
        (page.size.height - page.top - page.bottom).max(1)
    } else {
        (page.size.width - page.left - page.right).max(1)
    }
}

fn view_records(data: &[u8]) -> Option<Vec<(u16, &[u8])>> {
    if data.get(..6)? != [0, 1, 0, 2, 0x10, 0] {
        return None;
    }
    let length = u32::from_be_bytes(data.get(6..10)?.try_into().ok()?) as usize;
    let end = 10usize.checked_add(length)?;
    let mut at = 10;
    let mut records = Vec::new();
    while at + 4 <= end {
        let tag = u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?);
        let size = u16::from_be_bytes(data.get(at + 2..at + 4)?.try_into().ok()?) as usize;
        at += 4;
        let stop = at.checked_add(size).filter(|n| *n <= end)?;
        records.push((tag, data.get(at..stop)?));
        at = stop;
    }
    (at == end).then_some(records)
}

fn view_typography(data: &[u8], page: &mut ibis_doc_model::PageSetup) -> FlowRunStyle {
    let mut result = FlowRunStyle::default();
    let mut columns = Some(80);
    let records = view_records(data);
    for (tag, value) in records.unwrap_or_default() {
        match tag {
            0x1006 if value.first().is_some_and(|mask| mask & 1 != 0) => {
                if let Some(bytes) = value.get(1..5) {
                    let size = u32::from_be_bytes(bytes.try_into().unwrap());
                    if (35..=35_278).contains(&size) {
                        result.size = (u64::from(size) * 7200 / 2540) as u32;
                    }
                }
            }
            0x100b
                if value
                    .first()
                    .is_some_and(|mask| mask & 0x80 != 0 && mask & !0xd7 == 0) =>
            {
                let mask = value[0];
                let at = 1
                    + usize::from(mask & 1 != 0)
                    + 2 * usize::from(mask & 2 != 0)
                    + [2, 4, 6]
                        .iter()
                        .filter(|bit| mask & (1 << **bit) != 0)
                        .count();
                if let Some(bytes) = value.get(at..at + 2) {
                    columns = Some(u16::from_be_bytes([bytes[0], bytes[1]]));
                }
            }
            _ => {}
        }
    }
    if let Some(columns) = columns.filter(|n| (2..=2000).contains(n)) {
        let pitch = text_axis_width(page) * 2 / i64::from(columns);
        page.char_space = Some(pitch - i64::from(result.size) * 127);
        page.doc_grid_type = ibis_doc_model::DocGridType::LinesAndChars;
    }
    result
}

/// Apply the documented paper and margin fields only when their enclosing
/// records and preceding mask fields can be read without guessing.
fn apply_view_page(data: &[u8], page: &mut ibis_doc_model::PageSetup) {
    const EMU_PER_HUNDREDTH_MM: i64 = 360;
    if data.get(..6) != Some(&[0, 1, 0, 2, 0x10, 0]) {
        return;
    }
    let Some(length_bytes) = data.get(6..10) else {
        return;
    };
    let length = u32::from_be_bytes(length_bytes.try_into().unwrap()) as usize;
    let Some(end) = 10usize.checked_add(length).filter(|end| *end <= data.len()) else {
        return;
    };
    let mut candidate = page.clone();
    let mut cursor = 10;
    while cursor + 4 <= end {
        let tag = u16::from_be_bytes([data[cursor], data[cursor + 1]]);
        let len = u16::from_be_bytes([data[cursor + 2], data[cursor + 3]]) as usize;
        cursor += 4;
        let Some(record_end) = cursor
            .checked_add(len)
            .filter(|end_record| *end_record <= end)
        else {
            return;
        };
        let record = &data[cursor..record_end];
        if tag == 0x1001 {
            if let Some(&mask) = record.first().filter(|&&mask| mask & 1 == 0) {
                let mut at = 1;
                for bit in [1, 2] {
                    if mask & (1 << bit) != 0 {
                        let Some(raw) = record.get(at..at + 4) else {
                            return;
                        };
                        let value = u32::from_be_bytes(raw.try_into().unwrap());
                        if !(1_000..=200_000).contains(&value) {
                            return;
                        }
                        if bit == 1 {
                            candidate.size.width = i64::from(value) * EMU_PER_HUNDREDTH_MM;
                        } else {
                            candidate.size.height = i64::from(value) * EMU_PER_HUNDREDTH_MM;
                        }
                        at += 4;
                    }
                }
            }
        } else if tag == 0x1002 {
            let Some(&first) = record.first().filter(|&&mask| mask & !0x90 == 0) else {
                cursor = record_end;
                continue;
            };
            let mut at = 1 + 2 * ((first & 0x10 != 0) as usize + (first & 0x80 != 0) as usize);
            let Some(&mask) = record.get(at).filter(|&&mask| mask & !0xf9 == 0) else {
                cursor = record_end;
                continue;
            };
            at += 1;
            if mask & 1 != 0 {
                if record.get(at).is_none() {
                    return;
                }
                at += 1;
            }
            for bit in [3, 4, 5, 6, 7] {
                if mask & (1 << bit) == 0 {
                    continue;
                }
                let Some(raw) = record.get(at..at + 2) else {
                    return;
                };
                let value = i64::from(u16::from_be_bytes([raw[0], raw[1]])) * EMU_PER_HUNDREDTH_MM;
                match bit {
                    3 => candidate.top = value,
                    4 => candidate.bottom = value,
                    5 => candidate.left = value,
                    6 => candidate.right = value,
                    _ => {}
                }
                at += 2;
            }
        }
        cursor = record_end;
    }
    if cursor != end
        || candidate.size.width <= candidate.left + candidate.right + 360_000
        || candidate.size.height <= candidate.top + candidate.bottom + 360_000
    {
        return;
    }
    *page = candidate;
}

/// Read the explicitly stored lines-per-page field, without assuming a
/// default for unsupported record variants.
fn view_style_lines(data: &[u8]) -> Option<u16> {
    if data.get(..6)? != [0, 1, 0, 2, 0x10, 0] {
        return None;
    }
    let length = u32::from_be_bytes(data.get(6..10)?.try_into().ok()?) as usize;
    let end = 10usize.checked_add(length)?;
    if end > data.len() {
        return None;
    }
    let mut at = 10;
    let mut found = None;
    while at + 4 <= end {
        let tag = u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?);
        let len = u16::from_be_bytes(data.get(at + 2..at + 4)?.try_into().ok()?) as usize;
        at += 4;
        let stop = at.checked_add(len)?;
        if stop > end {
            return None;
        }
        if tag == 0x100d {
            let record = data.get(at..stop)?;
            let mask = *record.first()?;
            if mask & 0x02 == 0 || mask & !0x0b != 0 || found.is_some() {
                return None;
            }
            let offset = 1 + usize::from(mask & 1 != 0);
            let bytes = record.get(offset..offset + 2)?;
            // Observed 0x08 follows the line count as a four-byte field;
            // its meaning is still unknown, but it does not shift the count.
            // Do not accept a truncated or otherwise unrecognized record.
            if mask & 0x08 != 0 && record.len() != offset + 2 + 4 {
                return None;
            }
            let lines = u16::from_be_bytes([bytes[0], bytes[1]]);
            if !(2..=500).contains(&lines) {
                return None;
            }
            found = Some(lines);
        }
        at = stop;
    }
    (at == end).then_some(found).flatten()
}

/// Align two adjacent observed display-line grids using spans. This retains
/// every original cell boundary; it does not infer logical JTD merges.
fn merge_table_grids(previous: &mut FlowTable, next: &FlowTable) -> bool {
    fn edges(columns: &[i64]) -> Option<Vec<i64>> {
        let mut out = vec![0i64];
        for &width in columns {
            out.push(out.last()?.checked_add(width)?);
        }
        Some(out)
    }
    let Some(a) = edges(&previous.columns) else {
        return false;
    };
    let Some(b) = edges(&next.columns) else {
        return false;
    };
    if a.last() != b.last() {
        return false;
    }
    let mut grid = a.clone();
    grid.extend_from_slice(&b);
    grid.sort_unstable();
    grid.dedup();
    if grid.len() > 1025 {
        return false;
    }
    fn remap(rows: &mut [FlowRow], old: &[i64], grid: &[i64]) -> bool {
        for row in rows {
            let mut at = 0usize;
            for cell in &mut row.cells {
                let end = at + cell.span.max(1) as usize;
                if end >= old.len() {
                    return false;
                }
                let (Ok(start), Ok(stop)) =
                    (grid.binary_search(&old[at]), grid.binary_search(&old[end]))
                else {
                    return false;
                };
                cell.span = (stop - start) as u32;
                at = end;
            }
            if at != old.len() - 1 {
                return false;
            }
        }
        true
    }
    let mut merged = previous.clone();
    let mut incoming = next.rows.clone();
    if !remap(&mut merged.rows, &a, &grid) || !remap(&mut incoming, &b, &grid) {
        return false;
    }
    merged.columns = grid.windows(2).map(|pair| pair[1] - pair[0]).collect();
    merged.rows.extend(incoming);
    merged.resolve_borders();
    *previous = merged;
    true
}

/// Export the currently recognized model as DOCX. Unsupported JTD features
/// cannot be represented and the resulting file is not a lossless conversion.
pub fn export_docx(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    let blank = ibis_formats::open(ibis_formats::blank_file(Kind::Document)?)?;
    let mut exported = presentation.clone();
    if let Some(flow) = exported.flow.as_mut() {
        let flow = std::sync::Arc::make_mut(flow);
        if let (Some(space), Some(size)) = (flow.page.char_space, flow.styles.defaults_run.size) {
            // Word stores the default size in half-points. Preserve the
            // source's full-width grid pitch after that unavoidable rounding.
            let rounded = (size + 25) / 50 * 50;
            flow.page.char_space = Some(space + (i64::from(size) - i64::from(rounded)) * 127);
        }
    }
    blank.source.save(&exported, ibis_formats::SaveOptions::default())
}

/// Export the currently recognized model as standalone UTF-8 HTML.
/// Unsupported JTD features are omitted, as with [`export_docx`].
pub fn export_html(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    ibis_formats::docx_convert::export_docx_to_html(presentation)
}

/// Prefer the documented direction flag; the initial record remains an
/// imperfect compatibility heuristic for variants that cannot be decoded.
fn observed_vertical(data: &[u8]) -> bool {
    view_style_direction(data).unwrap_or(false)
}

/// Decode only the documented subset of the 1002 field masks. An unknown
/// preceding field makes the direction ambiguous instead of guessing its size.
fn view_style_direction(data: &[u8]) -> Option<bool> {
    if data.get(..6)? != [0, 1, 0, 2, 0x10, 0] {
        return None;
    }
    let length = u32::from_be_bytes(data.get(6..10)?.try_into().ok()?) as usize;
    let end = 10usize.checked_add(length)?;
    if end > data.len() {
        return None;
    }
    let mut cursor = 10;
    while cursor + 4 <= end {
        let tag = u16::from_be_bytes(data.get(cursor..cursor + 2)?.try_into().ok()?);
        let len = u16::from_be_bytes(data.get(cursor + 2..cursor + 4)?.try_into().ok()?) as usize;
        cursor += 4;
        let record_end = cursor.checked_add(len)?;
        if record_end > end {
            return None;
        }
        if tag == 0x1002 {
            let record = data.get(cursor..record_end)?;
            let mut at = 0usize;
            let m1 = *record.get(at)?;
            at += 1;
            if m1 & !0x90 != 0 {
                return None;
            }
            at += 2 * ((m1 & 0x10 != 0) as usize + (m1 & 0x80 != 0) as usize);
            let m2 = *record.get(at)?;
            at += 1;
            if m2 & !0xf9 != 0 {
                return None;
            }
            at += (m2 & 1 != 0) as usize
                + 2 * [3, 4, 5, 6, 7]
                    .iter()
                    .filter(|&&bit| m2 & (1 << bit) != 0)
                    .count();
            let m3 = *record.get(at)?;
            at += 1;
            if m3 & !0x51 != 0 {
                return None;
            }
            if m3 & 1 != 0 {
                // Six paired/local documents carry a signed 32-bit field
                // before the direction byte. Its meaning remains unknown.
                let value = i32::from_be_bytes(record.get(at..at + 4)?.try_into().ok()?);
                if !matches!(value, -3 | -1 | 0) {
                    return None;
                }
                at += 4;
            }
            let vertical = if m3 & 0x10 != 0 {
                let value = *record.get(at)?;
                at += 1;
                match value {
                    0 => false,
                    1 => true,
                    _ => return None,
                }
            } else {
                false
            };
            if m3 & 0x40 != 0 && record.get(at..at.checked_add(20)?).is_none() {
                return None;
            }
            return Some(vertical);
        }
        cursor = record_end;
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Segment {
    Text(String),
    StyledText(String, Box<FlowRunStyle>),
    ParagraphFormat(ibis_doc_model::ParaFormat),
    Ruby {
        base: String,
        reading: String,
        style: Box<FlowRunStyle>,
        ruby_size: u32,
    },
    PageBreak,
    RowStart(u16),
    RowAlignment(ibis_doc_model::FlowAlign),
    RowRules(RuleLayout),
    CellStart(u16, u16),
    RowEnd,
    Table {
        cells: Vec<(u16, u16, Vec<Segment>)>,
        total: u16,
        strips: bool,
        rules: Option<RuleLayout>,
        row_alignment: Option<ibis_doc_model::FlowAlign>,
    },
}

impl Segment {
    fn into_text(self, defaults: &FlowRunStyle) -> (String, FlowRunStyle) {
        match self {
            Self::Text(text) => (text, defaults.clone()),
            Self::StyledText(text, style) => (text, *style),
            _ => unreachable!("text segment"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuleLayout {
    points: Vec<RulePoint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RulePoint {
    position: u16,
    flags: u16,
    partial: u16,
    link: u16,
    types: [u16; 4],
    linked_types: [u16; 4],
}

type SourceCell = (u16, u16, Vec<Segment>);
type GridCell = (u16, u16, usize, usize, Vec<Segment>);

/// Adapt source spans to a rectangular Word grid without deleting narrow
/// spans or changing their text. Empty gap cells are representation padding,
/// not inferred source cells. Original declarations remain in inspect().
fn table_grid(
    cells: Vec<SourceCell>,
    rules: Option<&RuleLayout>,
    width: u16,
) -> (Vec<u16>, Vec<GridCell>) {
    let centers: Vec<u16> = rules
        .map(|r| r.points.iter().map(|p| p.position).collect())
        .unwrap_or_default();
    let mut grid = vec![0, width];
    let mut mapped = Vec::new();
    for (left, right, contents) in cells {
        let a = centers
            .iter()
            .copied()
            .filter(|c| *c <= left && left - c <= 2)
            .max()
            .unwrap_or(left);
        let b = centers
            .iter()
            .copied()
            .filter(|c| *c >= right && c - right <= 2)
            .min()
            .unwrap_or(right);
        grid.extend([a, b]);
        mapped.push((left, right, a, b, contents));
    }
    for i in 1..mapped.len() {
        if mapped[i - 1].3 > mapped[i].2 {
            mapped[i - 1].3 = mapped[i - 1].1;
            mapped[i].2 = mapped[i].0;
        }
    }
    grid.extend(mapped.iter().flat_map(|(_, _, a, b, _)| [*a, *b]));
    grid.extend(centers.into_iter().filter(|c| *c <= width));
    grid.sort_unstable();
    grid.dedup();
    let mut result = Vec::new();
    let mut cursor = 0;
    for (left, right, a, b, contents) in mapped {
        let start = grid.binary_search(&a).unwrap();
        let stop = grid.binary_search(&b).unwrap();
        if start > cursor {
            result.push((grid[cursor], grid[start], cursor, start, Vec::new()));
        }
        result.push((left, right, start, stop, contents));
        cursor = stop;
    }
    if cursor < grid.len() - 1 {
        result.push((grid[cursor], width, cursor, grid.len() - 1, Vec::new()));
    }
    (grid, result)
}

/// Parse the documented 008f form only when all lengths and the final width
/// agree. Unknown variants are left to the existing conservative preview.
fn parse_rules(payload: &[u16]) -> Option<RuleLayout> {
    if payload.len() < 7 || payload[0] == 0 || payload[1] != 0 || !payload.ends_with(&[0xffff, 0]) {
        return None;
    }
    let width = payload[0] as u32;
    let mut position = u32::from(payload[2]).checked_add(1)?;
    let values = &payload[3..payload.len() - 2];
    if values.is_empty() || values.len() % 4 != 0 && values.len() % 4 != 2 {
        return None;
    }
    let mut points = Vec::new();
    let mut i = 0;
    while i < values.len() {
        if position > width || points.len() >= 64 {
            return None;
        }
        let (flags, partial, link, advance, step) = if values.len() - i == 2 {
            (values[i], 0, 0, u32::from(values[i + 1]) + 1, 2)
        } else {
            (
                values[i],
                values[i + 1],
                values[i + 2],
                u32::from(values[i + 1]) + u32::from(values[i + 3]) + 2,
                4,
            )
        };
        points.push(RulePoint {
            position: position.try_into().ok()?,
            flags,
            partial,
            link,
            types: [1; 4],
            linked_types: [1; 4],
        });
        position = position.checked_add(advance)?;
        i += step;
    }
    (position == width).then_some(RuleLayout { points })
}

/// Map only whole-height and whole-cell lines. Mid-line, partial and
/// half-height rules need a different representation and are not invented.
fn rule_cell_borders(
    layout: &RuleLayout,
    left: u16,
    right: u16,
    line: FlowBorder,
) -> [Option<Option<FlowBorder>>; 4] {
    let nearest = |edge: u16| {
        layout
            .points
            .iter()
            .enumerate()
            .filter(|(_, p)| p.position.abs_diff(edge) <= 2)
            .min_by_key(|(_, p)| p.position.abs_diff(edge))
    };
    let l = nearest(left);
    let r = nearest(right);
    let full = |point: &RulePoint| point.flags & 0x30 != 0 && point.flags & 0x03 == 0x03;
    let bottom = match (l, r) {
        (Some((li, _)), Some((ri, _))) if ri > li => {
            let kind = layout.points[li].linked_types[3];
            layout.points[li..ri]
                .iter()
                .all(|point| {
                    point.partial == 0 && point.link & 0x08 != 0 && point.linked_types[3] == kind
                })
                .then(|| typed_border(line, kind))
        }
        _ => None,
    };
    [
        Some(None),
        Some(r.and_then(|(_, p)| {
            (full(p) && p.types[0] == p.types[1]).then(|| typed_border(line, p.types[0]))
        })),
        Some(bottom),
        Some(l.and_then(|(_, p)| {
            (full(p) && p.types[0] == p.types[1]).then(|| typed_border(line, p.types[0]))
        })),
    ]
}

fn rule_types(document: &DecodedText, position: usize) -> [u16; 4] {
    [1, 2, 3, 8].map(|id| {
        document
            .style(position)
            .and_then(|properties| properties.get(&id))
            .filter(|value| value.len() == 2)
            .map(|v| u16::from_be_bytes([v[0], v[1]]))
            .filter(|value| (1..=16).contains(value))
            .unwrap_or(1)
    })
}

fn typed_border(mut line: FlowBorder, kind: u16) -> FlowBorder {
    // Names are decoded; widths beyond the default are preview approximations.
    line.style = match kind {
        4..=6 | 9 => BorderStyle::Dashed,
        7 => BorderStyle::Double,
        8 => BorderStyle::Dotted,
        10 | 11 => BorderStyle::DotDash,
        12 | 13 => BorderStyle::Wave,
        14 => BorderStyle::DoubleWave,
        _ => BorderStyle::Single,
    };
    line.width *= match kind {
        2 | 5 => 2,
        3 | 6 => 3,
        _ => 1,
    };
    if kind == 16 {
        line.width = (line.width / 3).max(1);
    }
    line
}

/// Keep source-supported line fragments that cannot be represented as a
/// DOCX cell edge. Coordinates are relative to the entire display row.
fn rule_extra_lines(layout: &RuleLayout, width: u16, line: FlowBorder) -> Vec<FlowRowLine> {
    if width == 0 {
        return Vec::new();
    }
    let scale = |position: u32| -> u16 {
        ((position.min(u32::from(width)) * 10_000) / u32::from(width)) as u16
    };
    let mut result = Vec::new();
    for (index, point) in layout.points.iter().enumerate() {
        let x = scale(u32::from(point.position));
        if point.flags & 0x30 != 0 {
            let vertical = match point.flags & 3 {
                1 => Some((0, 5_000)),
                2 => Some((5_000, 10_000)),
                3 if point.types[0] != point.types[1] => Some((0, 5_000)),
                _ => None,
            };
            if let Some((y1, y2)) = vertical {
                result.push(FlowRowLine {
                    x1: x,
                    y1,
                    x2: x,
                    y2,
                    border: typed_border(line, point.types[if y1 == 0 { 0 } else { 1 }]),
                });
                if point.flags & 3 == 3 {
                    result.push(FlowRowLine {
                        x1: x,
                        y1: 5_000,
                        x2: x,
                        y2: 10_000,
                        border: typed_border(line, point.types[1]),
                    });
                }
            }
        }
        let attached_rule = point.link & 0x30 != 0 && point.link & 3 != 0;
        if let Some(next) = layout.points.get(index + 1) {
            let end = scale(u32::from(next.position));
            if !attached_rule && point.link & 0x04 != 0 && end > x {
                result.push(FlowRowLine {
                    x1: x,
                    y1: 5_000,
                    x2: end,
                    y2: 5_000,
                    border: typed_border(line, point.linked_types[2]),
                });
            }
            // Whole bottom edges are already represented on cells. When a
            // partial span prevents that mapping, keep the linked segment.
            if !attached_rule && point.partial != 0 && point.link & 0x08 != 0 && end > x {
                result.push(FlowRowLine {
                    x1: x,
                    y1: 10_000,
                    x2: end,
                    y2: 10_000,
                    border: typed_border(line, point.linked_types[3]),
                });
            }
        }
        if point.partial != 0 {
            let end = scale(u32::from(point.position) + u32::from(point.partial) + 1);
            if end > x {
                if point.flags & 0x04 != 0 && (attached_rule || point.link & 0x04 == 0) {
                    result.push(FlowRowLine {
                        x1: x,
                        y1: 5_000,
                        x2: end,
                        y2: 5_000,
                        border: typed_border(line, point.types[2]),
                    });
                }
                if point.flags & 0x08 != 0 && (attached_rule || point.link & 0x08 == 0) {
                    result.push(FlowRowLine {
                        x1: x,
                        y1: 10_000,
                        x2: end,
                        y2: 10_000,
                        border: typed_border(line, point.types[3]),
                    });
                }
            }
        }
        // `b` can carry a vertical-rule class at the end of an item's own
        // line, independently of the next rule item.
        if point.link & 0x30 != 0 {
            let attached = scale(u32::from(point.position) + u32::from(point.partial) + 1);
            for (bit, y1, y2, kind) in [
                (1, 0, 5_000, point.linked_types[0]),
                (2, 5_000, 10_000, point.linked_types[1]),
            ] {
                if point.link & bit != 0 {
                    result.push(FlowRowLine {
                        x1: attached,
                        y1,
                        x2: attached,
                        y2,
                        border: typed_border(line, kind),
                    });
                }
            }
        }
    }
    result
}

/// Every source rule must be represented either by a cell edge or a row
/// segment. A full rule can sit inside a declared cell, and a cell's bottom
/// can consist of multiple line types; neither is a single cell border.
fn rule_row_lines(
    layout: &RuleLayout,
    width: u16,
    line: FlowBorder,
    cells: &[GridCell],
    grid: &[u16],
) -> Vec<FlowRowLine> {
    let mut lines = rule_extra_lines(layout, width, line);
    if width == 0 {
        return lines;
    }
    let scale = |p: u16| (u32::from(p) * 10_000 / u32::from(width)) as u16;
    let edges: Vec<_> = cells
        .iter()
        .map(|(left, right, start, stop, _)| {
            (
                grid[*start],
                grid[*stop],
                rule_cell_borders(layout, *left, *right, line),
            )
        })
        .collect();
    for (i, point) in layout.points.iter().enumerate() {
        let x = scale(point.position);
        if point.flags & 0x30 != 0 && point.flags & 3 == 3 && point.types[0] == point.types[1] {
            let border = typed_border(line, point.types[0]);
            let covered = edges.iter().any(|(left, right, borders)| {
                *left == point.position && borders[3] == Some(Some(border))
                    || *right == point.position && borders[1] == Some(Some(border))
            });
            if !covered {
                lines.push(FlowRowLine {
                    x1: x,
                    y1: 0,
                    x2: x,
                    y2: 10_000,
                    border,
                });
            }
        }
        let attached = point.link & 0x30 != 0 && point.link & 3 != 0;
        if point.partial == 0 && point.link & 8 != 0 && !attached {
            if let Some(next) = layout.points.get(i + 1) {
                let border = typed_border(line, point.linked_types[3]);
                let covered = edges.iter().any(|(left, right, borders)| {
                    *left <= point.position
                        && *right >= next.position
                        && borders[2] == Some(Some(border))
                });
                if !covered {
                    lines.push(FlowRowLine {
                        x1: x,
                        y1: 10_000,
                        x2: scale(next.position),
                        y2: 10_000,
                        border,
                    });
                }
            }
        }
    }
    lines
}

/// Decode the logical store before interpreting any text or record.
#[cfg(test)]
fn extract(bytes: &[u8]) -> Result<Vec<Segment>, Error> {
    extract_decoded(&decode::decode(bytes)?, &FlowRunStyle::default(), None)
}

/// Inspect original logical units and record ranges without applying table
/// inference. Unknown properties and undecoded style tails remain available.
pub fn inspect(data: &[u8]) -> Result<DecodedText, Error> {
    let data = source_document(data)?;
    decode::decode(&checked_stream(&data)?)
}

fn run_style(document: &DecodedText, position: usize, defaults: &FlowRunStyle) -> FlowRunStyle {
    let mut result = defaults.clone();
    let Some(properties) = document.style(position) else {
        return result;
    };
    let value = |id| {
        properties
            .get(&id)
            .and_then(|bytes| match bytes.as_slice() {
                [a, b] => Some(u16::from_be_bytes([*a, *b]) as u32),
                [a, b, c, d] => Some(u32::from_be_bytes([*a, *b, *c, *d])),
                _ => None,
            })
    };
    let flags = value(20).unwrap_or(0);
    if flags & 0x8000_0000 == 0 {
        return result;
    }
    if let Some(size) = value(2).filter(|size| (35..=35_278).contains(size)) {
        result.size = (u64::from(size) * 7200 / 2540) as u32;
    }
    result.bold = matches!((flags >> 26) & 3, 1 | 2);
    result.italic = matches!((flags >> 28) & 3, 1 | 2);
    result.underline = flags & 0x10 != 0 && value(13).is_some_and(|n| matches!(n, 1 | 2));
    if value(13) == Some(2) {
        result.underline_style = ibis_doc_model::UnderlineStyle::Thick;
    }
    if let Some(color) = value(15).filter(|v| *v != u32::MAX) {
        result.color = Rgb::from_rgb(color as u8, (color >> 8) as u8, (color >> 16) as u8);
    }
    if flags & 8 != 0 {
        result.color = Rgb::WHITE;
        result.highlight = Some(Rgb::BLACK);
    }
    result
}

fn append_text(
    document: &DecodedText,
    range: std::ops::Range<usize>,
    defaults: &FlowRunStyle,
    out: &mut Vec<Segment>,
) -> Result<(), Error> {
    if document.units[range.clone()]
        .iter()
        .any(|n| *n < 0x20 && !matches!(n, 9 | 10 | 13))
    {
        return Ok(());
    }
    let mut at = range.start;
    while at < range.end {
        let style = run_style(document, at, defaults);
        let index = document.styles.partition_point(|span| span.units.end <= at);
        let mut end = document.styles.get(index).map_or(range.end, |span| {
            if span.units.contains(&at) {
                span.units.end.min(range.end)
            } else {
                span.units.start.min(range.end)
            }
        });
        // A style boundary cannot divide a UTF-16 surrogate pair.
        if end < range.end
            && (0xd800..=0xdbff).contains(&document.units[end - 1])
            && (0xdc00..=0xdfff).contains(&document.units[end])
        {
            end += 1;
        }
        let text = document.text(at..end)?;
        if !text.is_empty() {
            if style == FlowRunStyle::default() {
                out.push(Segment::Text(text));
            } else {
                out.push(Segment::StyledText(text, Box::new(style)));
            }
        }
        at = end;
    }
    Ok(())
}

fn extract_decoded(
    document: &DecodedText,
    defaults: &FlowRunStyle,
    normal_pitch: Option<i64>,
) -> Result<Vec<Segment>, Error> {
    use decode::RecordKind;
    let mut out = Vec::new();
    let mut records = document.records.iter().peekable();
    while let Some(record) = records.next() {
        match &record.kind {
            RecordKind::Text => append_text(document, record.units.clone(), defaults, &mut out)?,
            RecordKind::Inline {
                selector: [0, 1 | 3],
                text,
            } => {
                if let Some(next) = records.peek() {
                    if let RecordKind::Inline {
                        selector: [1, 0x82],
                        text: reading,
                    } = &next.kind
                    {
                        let base_style = run_style(document, text.start, defaults);
                        let reading_style = run_style(document, reading.start, defaults);
                        let valid = text
                            .clone()
                            .chain(reading.clone())
                            .all(|i| document.units[i] >= 0x20);
                        if valid {
                            let explicit_reading_size =
                                document.style(reading.start).is_some_and(|p| {
                                    p.get(&2)
                                        .is_some_and(|v| v.len() == 2 && v.as_slice() != [0, 0])
                                        && p.get(&20)
                                            .is_some_and(|v| v.len() == 4 && v[0] & 0x80 != 0)
                                });
                            let ruby_size = if explicit_reading_size {
                                reading_style.size
                            } else {
                                base_style.size / 2
                            };
                            out.push(Segment::Ruby {
                                base: document.text(text.clone())?,
                                reading: document.text(reading.clone())?,
                                style: Box::new(base_style),
                                ruby_size,
                            });
                        } else {
                            append_text(document, text.clone(), defaults, &mut out)?;
                        }
                        records.next();
                        continue;
                    }
                }
                append_text(document, text.clone(), defaults, &mut out)?;
            }
            RecordKind::Record {
                class: 0x10,
                payload,
            } => {
                // Some old documents have empty ordinary headers. Keep their
                // source ranges; only complete TLV lists supply layout fields.
                if let Ok(items) = document.tlv(payload.clone()) {
                    let mut format = ibis_doc_model::ParaFormat::default();
                    for (tag, range) in &items {
                        let values = &document.units[range.clone()];
                        match (*tag, values) {
                            (0x24, [value]) => {
                                format.align = match value {
                                    0 => Some(ibis_doc_model::FlowAlign::Left),
                                    1 => Some(ibis_doc_model::FlowAlign::Center),
                                    2 => Some(ibis_doc_model::FlowAlign::Right),
                                    _ => None,
                                }
                            }
                            (0x26, [unit, left, right, first, _]) => match unit {
                                0 => {
                                    format.left_chars = Some(i32::from(*left) * 50);
                                    format.right_chars = Some(i32::from(*right) * 50);
                                    format.first_line_chars =
                                        Some((i32::from(*first) - i32::from(*left)) * 50);
                                }
                                1 => {
                                    format.left = Some(i64::from(*left) * 360);
                                    format.right = Some(i64::from(*right) * 360);
                                    format.first_line =
                                        Some((i64::from(*first) - i64::from(*left)) * 360);
                                }
                                _ => {}
                            },
                            (0x20, [kind, value, _, _]) => {
                                let pitch = normal_pitch.unwrap_or(i64::from(defaults.size) * 127);
                                let feed = match kind {
                                    1 => Some(0),
                                    2 => Some(pitch / 2),
                                    3 => Some(pitch / 3),
                                    4 => Some(pitch / 4),
                                    5 => Some(pitch * 2 / 3),
                                    6 => Some(pitch * 3 / 4),
                                    8 => Some(i64::from(*value) * 360),
                                    9 => Some(pitch * i64::from(*value) / 1000),
                                    _ => None,
                                };
                                format.line = feed.map(ibis_doc_model::LineHeight::Exact);
                                if format.line.is_some() {
                                    format.snap = Some(false);
                                }
                            }
                            _ => {}
                        }
                    }
                    let row_alignment = format.align;
                    if format != ibis_doc_model::ParaFormat::default() {
                        out.push(Segment::ParagraphFormat(format));
                    }
                    for (tag, range) in items {
                        if tag == 0x8f {
                            let values = &document.units[range.clone()];
                            if let Some(width) = values.first().filter(|n| **n > 0) {
                                out.push(Segment::RowStart(*width));
                                if let Some(align) = row_alignment {
                                    out.push(Segment::RowAlignment(align));
                                }
                                // The TLV terminator belongs to the list, not
                                // the rule value. Legacy rule helper expects it.
                                let mut rules = values.to_vec();
                                rules.extend([0xffff, 0]);
                                if let Some(mut layout) = parse_rules(&rules) {
                                    let mut at = range.start + 3;
                                    for point in &mut layout.points {
                                        point.types = rule_types(document, at);
                                        if at + 3 < range.end {
                                            point.linked_types = rule_types(document, at + 2);
                                        }
                                        at += if range.end - at == 2 { 2 } else { 4 };
                                    }
                                    out.push(Segment::RowRules(layout));
                                }
                            }
                        }
                    }
                }
            }
            RecordKind::Record {
                class: 0x30,
                payload,
            } => {
                let values = &document.units[payload.clone()];
                // Flags describe continuation/closure, not whether a source
                // span exists. Retain all declared coordinates. The exact
                // flags remain available in the inspected source record.
                if let [0, left, right, _, _] = values {
                    out.push(Segment::CellStart(*left, *right));
                }
            }
            RecordKind::Control(14) => out.push(Segment::RowEnd),
            RecordKind::Control(12) => out.push(Segment::PageBreak),
            RecordKind::Control(9 | 10 | 13) => out.push(Segment::Text(
                char::from_u32(u32::from(document.units[record.units.start]))
                    .unwrap()
                    .to_string(),
            )),
            _ => {}
        }
    }
    if !document.units.is_empty()
        && !out.iter().any(|segment| match segment {
            Segment::Text(text) | Segment::StyledText(text, _) => !text.trim().is_empty(),
            Segment::Ruby { base, .. } => !base.trim().is_empty(),
            Segment::RowRules(_) => true,
            _ => false,
        })
    {
        return Err(Error::new(NO_VISIBLE_CONTENT));
    }
    Ok(out)
}

/// Reconstruct only complete, ordered cell runs. Borders and physical widths
/// are preview approximations; source stroke styles and units remain unknown.
fn table_candidates(segments: Vec<Segment>) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut iter = segments.into_iter().peekable();
    while let Some(segment) = iter.next() {
        let Segment::RowStart(total) = segment else {
            out.push(segment);
            continue;
        };
        let mut original = vec![Segment::RowStart(total)];
        while let Some(next) = iter.peek() {
            if matches!(next, Segment::RowStart(_)) {
                break;
            }
            let end = matches!(next, Segment::RowEnd);
            original.push(iter.next().unwrap());
            if end {
                break;
            }
        }
        let mut cells: Vec<(u16, u16, Vec<Segment>)> = Vec::new();
        let mut rules = None;
        let mut row_alignment = None;
        let mut valid = matches!(original.last(), Some(Segment::RowEnd));
        for part in &original[1..] {
            match part {
                Segment::CellStart(a, b) => {
                    if a > b
                        || *b > total
                        || cells.last().is_some_and(|c| c.1 > *a)
                        || cells.len() >= 64
                    {
                        valid = false;
                    }
                    cells.push((*a, *b, Vec::new()));
                }
                Segment::RowRules(value) if cells.is_empty() && rules.is_none() => {
                    rules = Some(value.clone());
                }
                Segment::RowAlignment(value) if cells.is_empty() && row_alignment.is_none() => {
                    row_alignment = Some(*value);
                }
                Segment::RowEnd => {}
                Segment::Text(_)
                | Segment::StyledText(..)
                | Segment::Ruby { .. }
                | Segment::PageBreak => {
                    if let Some(cell) = cells.last_mut() {
                        cell.2.push(part.clone());
                    } else {
                        valid = false;
                    }
                }
                _ => valid = false,
            }
        }
        let visible = cells.iter().any(|c| {
            c.2.iter().any(|s| {
                matches!(s, Segment::Text(t) | Segment::StyledText(t, _) if !t.trim().is_empty())
                    || matches!(s, Segment::Ruby { .. })
            })
        });
        // Zero-width empty declarations are cursor/blank-line markers. They
        // do not define a zero-width Word cell, but the display row remains.
        cells.retain(|(left, right, content)| {
            if left != right { return true; }
            if content.iter().any(|s| !matches!(s, Segment::Text(t) | Segment::StyledText(t, _) if t.trim().is_empty())) {
                valid = false;
            }
            false
        });
        if valid && rules.is_some() {
            if cells.is_empty() {
                cells.push((0, total, Vec::new()));
            }
            out.push(Segment::Table {
                cells,
                total,
                strips: true,
                rules,
                row_alignment,
            });
            continue;
        }
        if valid && cells.len() >= 2 && visible {
            out.push(Segment::Table {
                cells,
                total,
                strips: false,
                rules,
                row_alignment,
            });
        } else {
            out.extend(original);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_and_mixed_rules_are_drawn_once_even_inside_a_cell() {
        let point = |position, link, bottom| RulePoint {
            position,
            flags: 0x13,
            partial: 0,
            link,
            types: [1; 4],
            linked_types: [1, 1, 1, bottom],
        };
        let mut layout = RuleLayout {
            points: vec![point(10, 8, 1), point(30, 8, 2), point(50, 0, 1)],
        };
        let line = FlowBorder {
            width: 7620,
            color: Rgb::BLACK,
            style: BorderStyle::Single,
            space: 0,
            art: None,
        };
        let cells = vec![(12, 48, 0, 2, Vec::new())];
        let grid = [10, 30, 50];
        let lines = rule_row_lines(&layout, 100, line, &cells, &grid);
        assert_eq!(lines.len(), 3);
        assert!(
            lines
                .iter()
                .any(|l| (l.x1, l.y1, l.x2, l.y2) == (3000, 0, 3000, 10000))
        );
        let bottoms: Vec<_> = lines
            .iter()
            .filter(|l| l.y1 == 10000 && l.y2 == 10000)
            .collect();
        assert_eq!(bottoms.len(), 2);
        assert_ne!(bottoms[0].border.width, bottoms[1].border.width);
        layout.points[1].linked_types[3] = 1;
        let lines = rule_row_lines(&layout, 100, line, &cells, &grid);
        assert_eq!(
            lines.len(),
            1,
            "the uniform bottom is already a cell border"
        );
    }

    fn font_styles(primary: u16, secondary: u16) -> Vec<u8> {
        let mut value = vec![7];
        value.extend(393u32.to_be_bytes());
        value.extend(primary.to_be_bytes());
        value.extend(secondary.to_be_bytes());
        let mut record = vec![0x10, 6];
        record.extend((value.len() as u16).to_be_bytes());
        record.extend(value);
        let mut styles = vec![0, 1, 0, 2, 0x10, 0];
        styles.extend((record.len() as u32).to_be_bytes());
        styles.extend(record);
        styles
    }

    #[test]
    fn root_text_and_omitted_sheets_and_objects_are_identified_separately() {
        use ibis_ooxml::cfb::Item;
        let data = ibis_ooxml::cfb::write(&[
            Item::storage("ObjectSheets"),
            Item::storage("ObjectSheets/DocSheet"),
            Item::storage("ObjectSheets/DocSheet/DOCS_0001"),
            Item::stream("ObjectSheets/DocSheet/DOCS_0001/DocumentText", decode::fixture(&[0x4e59])),
            Item::stream("DocumentText", decode::fixture(&[0x7532])),
            Item::storage("Embedding 1"),
        ]);
        let omission = omitted_content(&data).unwrap();
        assert_eq!(omission, OmittedContent { additional_sheets: 1, embedded_objects: 1 });
        let decoded = inspect(&data).unwrap();
        assert!(decoded.units.contains(&0x7532));
        assert!(!decoded.units.contains(&0x4e59));
    }

    #[test]
    fn compressed_template_imports_text_and_preserves_omission_diagnostics() {
        use ibis_ooxml::cfb::Item;
        let inner = ibis_ooxml::cfb::write(&[
            Item::stream("DocumentText", decode::fixture(&[0x7532, 0x4e59])),
            Item::storage("Embedding 1"),
        ]);
        let wrapper = compressed::fixture(&inner);
        let outer = ibis_ooxml::cfb::write(&[Item::stream("JSCompDocument", wrapper)]);
        assert!(is_extension("JTTC"));
        assert!(is_jtd(&outer));
        assert_eq!(inspect(&outer).unwrap().units, vec![0x7532, 0x4e59]);
        assert_eq!(omitted_content(&outer).unwrap().embedded_objects, 1);
        let doc = read(&outer).unwrap();
        let docx = export_docx(&doc).unwrap();
        let reopened = ibis_formats::read(&docx).unwrap();
        assert!(reopened.flow.unwrap().body.iter().any(|block| matches!(block, Block::Paragraph(p) if p.text().contains("甲乙"))));
    }

    #[test]
    fn image_only_document_reports_that_embedded_content_is_unsupported() {
        use ibis_ooxml::cfb::Item;
        let inner = ibis_ooxml::cfb::write(&[
            Item::stream("DocumentText", decode::fixture(&[10, 10])),
            Item::storage("Embedding 1"),
        ]);
        let outer = ibis_ooxml::cfb::write(&[Item::stream("JSCompDocument", compressed::fixture(&inner))]);
        let error = match read(&outer) { Ok(_) => panic!("must not present a blank preview as a successful import"), Err(error) => error };
        assert!(error.to_string().contains("埋め込み"));
    }

    #[test]
    fn oversized_root_text_does_not_fall_back_to_compressed_content() {
        use ibis_ooxml::cfb::Item;
        let inner = ibis_ooxml::cfb::write(&[Item::stream("DocumentText", decode::fixture(&[0x7532]))]);
        let data = ibis_ooxml::cfb::write(&[
            Item::stream("DocumentText", vec![0; MAX_STREAM + 1]),
            Item::stream("JSCompDocument", compressed::fixture(&inner)),
        ]);
        assert!(source_document(&data).is_err());
    }

    #[test]
    fn explicitly_agreeing_font_references_survive_word_export() {
        let data = ibis_ooxml::cfb::write(&[
            ibis_ooxml::cfb::Item::stream("DocumentText", decode::fixture(&[0x7532])),
            ibis_ooxml::cfb::Item::stream(
                "Font",
                font::fixture(&[
                    (0, "ＭＳ ゴシック", 400, false),
                    (9, "ＭＳ 明朝", 400, false),
                ]),
            ),
            ibis_ooxml::cfb::Item::stream("DocumentViewStyles", font_styles(9, 9)),
        ]);
        let model = read(&data).unwrap();
        let saved = ibis_formats::read(&export_docx(&model).unwrap()).unwrap();
        for doc in [&model, &saved] {
            let flow = doc.flow.as_ref().unwrap();
            assert_eq!(
                flow.styles.defaults_run.east_asian_font.as_deref(),
                Some("ＭＳ 明朝")
            );
            let Block::Paragraph(p) = &flow.body[0] else {
                panic!("paragraph")
            };
            assert_eq!(
                p.runs[0].style.east_asian_font.as_deref(),
                Some("ＭＳ 明朝")
            );
        }
        let fonts = inspect_fonts(&data).unwrap();
        assert!(default_font(&font_styles(0, 9), &fonts).is_none());
        assert!(default_font(&font_styles(9, 0xfffe), &fonts).is_none());
    }

    #[test]
    fn word_size_rounding_does_not_change_character_grid_pitch_or_accumulate() {
        let mut model = read(&fixture("甲")).unwrap();
        let flow = std::sync::Arc::make_mut(model.flow.as_mut().unwrap());
        std::sync::Arc::make_mut(&mut flow.styles).defaults_run.size = Some(1114);
        flow.page.char_space = Some(910);
        let expected = 1114 * 127 + 910;
        let first = ibis_formats::read(&export_docx(&model).unwrap()).unwrap();
        let second = ibis_formats::read(&export_docx(&first).unwrap()).unwrap();
        for doc in [&first, &second] {
            let flow = doc.flow.as_ref().unwrap();
            let pitch = i64::from(flow.styles.defaults_run.size.unwrap()) * 127
                + flow.page.char_space.unwrap();
            assert!(
                (pitch - expected).abs() <= 4,
                "full-width pitch changed: {pitch}"
            );
        }
        assert_eq!(model.flow.as_ref().unwrap().page.char_space, Some(910));
    }

    #[test]
    fn body_ruby_keeps_its_base_attributes_and_colour() {
        let units = [
            0x1c, 1, 7, 0, 0, 3, 0x1d, 0x6f22, 0x1e, 5, 0, 1, 0x1f, 0x1c, 1, 7, 0, 1, 0x82, 0x1d,
            0x304b, 0x3093, 0x1e, 5, 0, 1, 0x1f,
        ];
        let mut styles = vec![0];
        styles.extend(7u32.to_be_bytes());
        styles.extend([0xfe, 20, 4, 0x84, 0, 0, 0, 15, 4, 0, 0, 0, 0xff, 0xff, 0]);
        styles.push(0);
        styles.extend(((units.len() - 8) as u32).to_be_bytes());
        let data = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream(
            "DocumentText",
            decode::fixture_with_styles(&units, &styles),
        )]);
        let model = read(&data).unwrap();
        let saved = ibis_formats::read(&export_docx(&model).unwrap()).unwrap();
        for doc in [&model, &saved] {
            let Block::Paragraph(p) = &doc.flow.as_ref().unwrap().body[0] else {
                panic!("paragraph")
            };
            assert!(
                matches!(&p.runs[0].kind, FlowRunKind::Ruby { base, text, .. } if base == "漢" && text == "かん")
            );
            assert!(p.runs[0].style.bold);
            assert_eq!(p.runs[0].style.color, Rgb::from_rgb(255, 0, 0));
        }
    }

    #[test]
    fn observed_direction_field_with_signed_prefix_is_decoded_without_guessing_from_paper() {
        for signed in [-3i32, -1, 0] {
            let mut value = vec![0, 0, 0x51];
            value.extend(signed.to_be_bytes());
            value.push(1);
            for _ in 0..10 {
                value.extend(700u16.to_be_bytes());
            }
            let mut records = vec![0x10, 2];
            records.extend((value.len() as u16).to_be_bytes());
            records.extend(value);
            let mut styles = vec![0, 1, 0, 2, 0x10, 0];
            styles.extend((records.len() as u32).to_be_bytes());
            styles.extend(records);
            assert_eq!(view_style_direction(&styles), Some(true));
        }
    }

    #[test]
    fn paragraph_alignment_hanging_indent_and_fixed_feed_survive_docx() {
        let payload = [
            0, 0x24, 1, 1, 0x26, 5, 0, 4, 0, 2, 0, 0x20, 4, 8, 500, 0, 0, 0xffff, 0,
        ];
        let length = (payload.len() + 7) as u16;
        let mut units = vec![0x1c, 0x10, length];
        units.extend(payload);
        units.extend([length, 0, 0x10, 0x1f, 0x7532]);
        let data = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream(
            "DocumentText",
            decode::fixture(&units),
        )]);
        let model = read(&data).unwrap();
        let saved = ibis_formats::read(&export_docx(&model).unwrap()).unwrap();
        for doc in [&model, &saved] {
            let Block::Paragraph(p) = &doc.flow.as_ref().unwrap().body[0] else {
                panic!("paragraph")
            };
            assert_eq!(p.align, ibis_doc_model::FlowAlign::Center);
            assert_eq!(p.left_chars, Some(200));
            assert_eq!(p.first_line_chars, Some(-100));
            assert!(
                matches!(p.line, ibis_doc_model::LineHeight::Exact(feed) if (feed - 180_000).abs() < 635)
            );
        }
    }

    #[test]
    fn blank_document_and_zero_width_blank_ruled_line_are_kept() {
        let empty = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream(
            "DocumentText",
            decode::fixture(&[]),
        )]);
        let blank = read(&empty).unwrap();
        assert_eq!(blank.flow.as_ref().unwrap().page.top, 30 * 36_000);
        assert_eq!(
            blank.flow.as_ref().unwrap().page.doc_grid_type,
            ibis_doc_model::DocGridType::LinesAndChars
        );
        let layout = parse_rules(&[100, 0, 0, 8, 0, 0, 97, 0xffff, 0]).unwrap();
        let rows = table_candidates(vec![
            Segment::RowStart(100),
            Segment::RowRules(layout),
            Segment::CellStart(0, 0),
            Segment::RowEnd,
        ]);
        assert!(
            matches!(&rows[..], [Segment::Table { cells, .. }] if cells.len() == 1 && cells[0].0 == 0 && cells[0].1 == 100)
        );
    }

    #[test]
    fn tlv_rule_item_after_another_item_is_not_lost() {
        let values = [100, 0, 3, 0x23, 0, 0, 90, 0x23, 3];
        let mut payload = vec![0, 0x2a, 2, 0x1234, 0, 0x8f, values.len() as u16];
        payload.extend(values);
        payload.extend([0xffff, 0]);
        let length = (payload.len() + 7) as u16;
        let mut units = vec![0x1c, 0x10, length];
        units.extend(payload);
        units.extend([length, 0, 0x10, 0x1f, 0x7532]);
        let segments = extract(&decode::fixture(&units)).unwrap();
        assert!(segments.iter().any(|s| matches!(s, Segment::RowStart(100))));
        assert!(segments.iter().any(|s| matches!(s, Segment::RowRules(_))));
    }

    #[test]
    fn character_attributes_require_the_enable_flag() {
        use std::{collections::BTreeMap, sync::Arc};
        let mut document = decode::decode(&decode::fixture(&[0x7532])).unwrap();
        let mut properties = BTreeMap::from([
            (2, vec![1, 0xa7]),
            (13, vec![0, 2]),
            (15, vec![0, 0x33, 0x22, 0x11]),
            (20, vec![0x94, 0, 0, 0x10]),
        ]);
        document.styles = vec![StyleSpan {
            units: 0..1,
            properties: Arc::new(properties.clone()),
        }];
        let style = run_style(&document, 0, &FlowRunStyle::default());
        assert_eq!(style.size, 1199);
        assert!(style.bold && style.italic && style.underline);
        assert_eq!(style.underline_style, ibis_doc_model::UnderlineStyle::Thick);
        assert_eq!(style.color, Rgb::from_rgb(0x11, 0x22, 0x33));
        let mut model = read(&fixture("甲")).unwrap();
        let flow = std::sync::Arc::make_mut(model.flow.as_mut().unwrap());
        let Block::Paragraph(p) = &mut flow.body[0] else {
            panic!("paragraph")
        };
        p.runs[0] = import_run(FlowRunKind::Text("甲".into()), style.clone());
        let reopened = ibis_formats::read(&export_docx(&model).unwrap()).unwrap();
        let Block::Paragraph(p) = &reopened.flow.as_ref().unwrap().body[0] else {
            panic!("paragraph")
        };
        assert!(p.runs[0].style.bold && p.runs[0].style.italic && p.runs[0].style.underline);
        assert_eq!(p.runs[0].style.color, style.color);
        assert_eq!(p.runs[0].style.size, 1200);

        properties.insert(20, vec![0x3c, 0, 0, 0]);
        document.styles[0].properties = Arc::new(properties);
        assert_eq!(
            run_style(&document, 0, &FlowRunStyle::default()),
            FlowRunStyle::default()
        );
    }

    #[test]
    fn document_default_size_is_saved_in_styles_not_just_individual_runs() {
        let mut model = read(&fixture("甲")).unwrap();
        let flow = std::sync::Arc::make_mut(model.flow.as_mut().unwrap());
        std::sync::Arc::make_mut(&mut flow.styles).defaults_run.size = Some(1400);
        let reopened = ibis_formats::read(&export_docx(&model).unwrap()).unwrap();
        assert_eq!(reopened.flow.unwrap().styles.defaults_run.size, Some(1400));
    }

    #[test]
    fn numeric_inline_fields_are_preserved_but_not_serialized_as_xml_controls() {
        let bytes = decode::fixture(&[0x1c, 1, 7, 0, 0, 1, 0x1d, 5, 0x1e, 5, 0, 1, 0x1f, 0x7532]);
        let data = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream("DocumentText", bytes)]);
        let decoded = inspect(&data).unwrap();
        assert_eq!(decoded.units[7], 5);
        assert_eq!(decoded.diagnostics.len(), 1);
        let reopened = ibis_formats::read(&export_docx(&read(&data).unwrap()).unwrap()).unwrap();
        assert!(matches!(&reopened.flow.unwrap().body[0], Block::Paragraph(p) if p.text() == "甲"));
    }

    #[test]
    fn attached_rule_is_at_its_partial_endpoint_not_at_the_next_item() {
        let layout = parse_rules(&[100, 0, 0, 0x14, 1, 0x16, 96, 0xffff, 0]).unwrap();
        let line = FlowBorder {
            width: 6350,
            color: Rgb::BLACK,
            style: BorderStyle::Single,
            space: 0,
            art: None,
        };
        let lines = rule_extra_lines(&layout, 100, line);
        assert!(
            lines
                .iter()
                .any(|l| (l.x1, l.y1, l.x2, l.y2) == (100, 5000, 300, 5000))
        );
        assert!(
            lines
                .iter()
                .any(|l| (l.x1, l.y1, l.x2, l.y2) == (300, 5000, 300, 10000))
        );
    }

    #[test]
    fn rule_header_maps_only_complete_cell_edges_and_survives_docx() {
        let mut units: Vec<u16> = vec![
            0x1c, 0x10, 27, 0, 0x8f, 15, 100, 0, 3, 0x2b, 0, 8, 42, 0x23, 0, 0, 42, 0x23, 0, 0, 6,
            0xffff, 0, 27, 0, 0x10, 0x1f,
        ];
        units.extend([0x1c, 0x30, 12, 0, 0, 2, 0xff, 0, 12, 0, 0x30, 0x1f]);
        units.extend([0x1c, 0x30, 12, 0, 6, 46, 0xff, 0, 12, 0, 0x30, 0x1f]);
        units.extend("甲".encode_utf16());
        units.extend([0x1c, 0x30, 12, 0, 50, 90, 0xff, 0, 12, 0, 0x30, 0x1f]);
        units.extend("乙".encode_utf16());
        units.extend([0x1c, 0x30, 12, 0, 94, 100, 0xff, 0, 12, 0, 0x30, 0x1f]);
        units.extend([0x0e, 0, 0, 0, 0, 0, 0, 0]);
        let mut stream = b"SsmgV.01".to_vec();
        for unit in units {
            stream.extend_from_slice(&unit.to_be_bytes());
        }
        let units: Vec<u16> = stream[8..]
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        let input = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream(
            "DocumentText",
            decode::fixture(&units),
        )]);
        let imported = read(&input).unwrap();
        let saved = ibis_formats::read(&export_docx(&imported).unwrap()).unwrap();
        for doc in [&imported, &saved] {
            let table = doc
                .flow
                .as_ref()
                .unwrap()
                .body
                .iter()
                .find_map(|b| match b {
                    Block::Table(t) => Some(t),
                    _ => None,
                })
                .unwrap();
            assert_eq!(table.rows[0].cells.len(), 4);
            let a = &table.rows[0].cells[1];
            let b = &table.rows[0].cells[2];
            assert!(a.borders[3].is_some() && a.borders[1].is_some());
            assert!(a.borders[2].is_some());
            assert!(b.borders[2].is_none());
            assert!(b.borders[1].is_some());
        }
    }

    #[test]
    fn malformed_rule_records_are_not_promoted() {
        let good = [
            100, 0, 3, 0x2b, 0, 8, 42, 0x23, 0, 0, 42, 0x23, 0, 0, 6, 0xffff, 0,
        ];
        let layout = parse_rules(&good).unwrap();
        assert_eq!(
            layout.points.iter().map(|p| p.position).collect::<Vec<_>>(),
            [4, 48, 92]
        );
        let short_last = [21, 0, 0, 0x13, 0, 0, 8, 0x13, 9, 0xffff, 0];
        assert_eq!(
            parse_rules(&short_last)
                .unwrap()
                .points
                .iter()
                .map(|p| p.position)
                .collect::<Vec<_>>(),
            [1, 11]
        );
        let mut bad = good;
        bad[0] = 99;
        assert!(parse_rules(&bad).is_none());
        bad = good;
        bad[15] = 0;
        assert!(parse_rules(&bad).is_none());
        assert!(parse_rules(&good[..10]).is_none());
    }

    #[test]
    fn empty_rule_line_keeps_its_interior_and_bottom_edge() {
        let layout = RuleLayout {
            points: vec![
                RulePoint {
                    position: 4,
                    flags: 0x2b,
                    partial: 0,
                    link: 8,
                    types: [1; 4],
                    linked_types: [1; 4],
                },
                RulePoint {
                    position: 96,
                    flags: 0x23,
                    partial: 0,
                    link: 0,
                    types: [1; 4],
                    linked_types: [1; 4],
                },
            ],
        };
        let segments = table_candidates(vec![
            Segment::RowStart(100),
            Segment::RowRules(layout.clone()),
            Segment::CellStart(0, 2),
            Segment::CellStart(98, 100),
            Segment::RowEnd,
        ]);
        let [
            Segment::Table {
                cells,
                rules: Some(parsed),
                ..
            },
        ] = segments.as_slice()
        else {
            panic!("expected empty ruled table")
        };
        assert_eq!(cells.len(), 2);
        assert_eq!((cells[0].0, cells[0].1), (0, 2));
        let (grid, mapped) = table_grid(cells.clone(), Some(parsed), 100);
        assert_eq!(grid, [0, 4, 96, 100]);
        assert_eq!((mapped[1].0, mapped[1].1), (4, 96));
        let line = FlowBorder {
            width: 6350,
            color: Rgb::BLACK,
            style: BorderStyle::Single,
            space: 0,
            art: None,
        };
        assert!(
            rule_cell_borders(parsed, mapped[1].0, mapped[1].1, line)[2]
                .unwrap()
                .is_some()
        );
        let mut partial = layout;
        partial.points[0].partial = 1;
        assert!(
            rule_cell_borders(&partial, 6, 94, line)[2]
                .unwrap()
                .is_none()
        );
        partial.points[0].flags = 0x21;
        partial.points[0].link = 0x24;
        let borders = rule_cell_borders(&partial, 6, 94, line);
        assert!(
            borders[3].unwrap().is_none(),
            "half-height is not a full cell edge"
        );
        assert!(
            borders[2].unwrap().is_none(),
            "a middle line is not a bottom edge"
        );
    }

    #[test]
    fn half_and_middle_rules_become_bounded_row_lines() {
        let layout = RuleLayout {
            points: vec![
                RulePoint {
                    position: 4,
                    flags: 0x16,
                    partial: 0,
                    link: 0x14,
                    types: [1; 4],
                    linked_types: [1; 4],
                },
                RulePoint {
                    position: 96,
                    flags: 0x11,
                    partial: 0,
                    link: 0,
                    types: [1; 4],
                    linked_types: [1; 4],
                },
            ],
        };
        let line = FlowBorder {
            width: 6350,
            color: Rgb::BLACK,
            style: BorderStyle::Single,
            space: 0,
            art: None,
        };
        let lines = rule_extra_lines(&layout, 100, line);
        let coordinates: Vec<_> = lines
            .iter()
            .map(|line| (line.x1, line.y1, line.x2, line.y2))
            .collect();
        assert_eq!(
            coordinates,
            [
                (400, 5000, 400, 10000),
                (400, 5000, 9600, 5000),
                (9600, 0, 9600, 5000)
            ]
        );
        assert_eq!(lines[1].points(200.0, 40.0), ((8.0, 20.0), (192.0, 20.0)));
        assert_eq!(lines[1].points(100.0, 20.0), ((4.0, 10.0), (96.0, 10.0)));
        assert!(rule_extra_lines(&layout, 0, line).is_empty());
    }
    fn fixture(text: &str) -> Vec<u8> {
        let mut s = b"SsmgV.01".to_vec();
        for n in [0x001c_u16, 0x0010, 7, 7, 0, 0x0010, 0x001f]
            .into_iter()
            .chain(text.encode_utf16())
            .chain([0u16])
        {
            s.extend_from_slice(&n.to_be_bytes());
        }
        let units: Vec<u16> = s[8..]
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream(
            "DocumentText",
            decode::fixture(&units),
        )])
    }
    #[test]
    fn reads_text_and_is_recognized_before_xls() {
        let bytes = fixture("日本語\nsecond");
        assert!(is_jtd(&bytes));
        let result = read(&bytes).unwrap();
        let flow = result.flow.unwrap();
        assert_eq!(flow.body.len(), 2);
        assert!(matches!(&flow.body[0], Block::Paragraph(p) if p.text() == "日本語"));
        assert!(matches!(&flow.body[1], Block::Paragraph(p) if p.text() == "second"));
    }
    #[test]
    fn exports_recognized_text_as_utf8_html() {
        let model = read(&fixture("日本語 <>&\nsecond")).unwrap();
        let html = String::from_utf8(export_html(&model).unwrap()).unwrap();
        assert!(html.starts_with("<!DOCTYPE html>\n"));
        assert!(html.contains("<meta charset=\"UTF-8\">"));
        assert!(html.contains("<p style=\"text-align:justify\">日本語 &lt;&gt;&amp;</p>"));
        assert!(html.contains("<p style=\"text-align:justify\">second</p>"));
        let mut without_flow = model;
        without_flow.flow = None;
        assert!(export_html(&without_flow).is_err());
    }

    #[test]
    fn html_preserves_combined_table_spans() {
        use ibis_doc_model::VerticalMerge::{Continue, None, Start};
        let cell = |span, merge| FlowCell { span, merge, ..FlowCell::default() };
        let row = |cells| FlowRow { cells, ..FlowRow::default() };
        let table = FlowTable {
            columns: vec![50, 50, 50, 50],
            rows: vec![
                row(vec![cell(2, Start), cell(1, None), cell(1, None)]),
                row(vec![cell(2, Continue), cell(1, None), cell(1, None)]),
                row(vec![cell(2, Continue), cell(1, None), cell(1, None)]),
                row(vec![cell(2, None), cell(1, Start), cell(1, None)]),
                // The merge starts at grid column 2, but its cell index changes.
                row(vec![cell(0, None), cell(1, None), cell(1, Continue), cell(1, None)]),
                row(vec![cell(2, None), cell(1, None), cell(1, None)]),
                row(vec![cell(2, None), cell(1, Continue), cell(1, None)]),
            ],
            ..FlowTable::default()
        };
        let mut model = read(&fixture("table")).unwrap();
        std::sync::Arc::make_mut(model.flow.as_mut().unwrap()).body = vec![Block::Table(table)];
        let html = String::from_utf8(export_html(&model).unwrap()).unwrap();
        assert!(html.contains("<tr><td colspan=\"2\" rowspan=\"3\"></td><td></td><td></td></tr>"));
        assert_eq!(html.matches("<tr><td></td><td></td></tr>").count(), 2);
        assert!(html.contains("<tr><td colspan=\"2\"></td><td rowspan=\"2\"></td><td></td></tr>"));
        assert_eq!(html.matches("rowspan=").count(), 2, "a merge stops at the first unmerged row");
        assert!(!html.contains("colspan=\"0\""));
    }

    #[test]
    fn html_escapes_ruby_and_keeps_its_style_and_link() {
        let mut model = read(&fixture("ruby")).unwrap();
        let flow = std::sync::Arc::make_mut(model.flow.as_mut().unwrap());
        let Block::Paragraph(paragraph) = &mut flow.body[0] else { panic!("paragraph") };
        let run = &mut paragraph.runs[0];
        run.kind = FlowRunKind::Ruby {
            base: "<&".into(), text: ">&".into(), size: 500, raise: 1000,
            align: RubyAlign::Center,
        };
        run.style.bold = true;
        run.link = Some(ibis_doc_model::Link::Url("https://example.com/?a=1&b=2".into()));
        let html = String::from_utf8(export_html(&model).unwrap()).unwrap();
        assert!(html.contains("<a href=\"https://example.com/?a=1&amp;b=2\"><strong><ruby>&lt;&amp;<rt>&gt;&amp;</rt></ruby></strong></a>"));
    }

    #[test]
    fn exports_recognized_text_and_page_direction_as_docx() {
        let mut presentation = read(&fixture("日本語\nsecond")).unwrap();
        std::sync::Arc::make_mut(presentation.flow.as_mut().unwrap())
            .page
            .direction = CellDirection::TbRl;
        let data = export_docx(&presentation).unwrap();
        let reopened = ibis_formats::read(&data).unwrap();
        let flow = reopened.flow.unwrap();
        assert!(flow.page.direction.vertical());
        assert!(matches!(&flow.body[0], Block::Paragraph(p) if p.text() == "日本語"));
        assert!(matches!(&flow.body[1], Block::Paragraph(p) if p.text() == "second"));
    }

    #[test]
    fn observed_table_cells_export_with_approximate_grid() {
        let mut units: Vec<u16> = vec![
            0x1c, 0x10, 27, 0, 0x8f, 15, 100, 0, 3, 0x23, 0, 0, 48, 0x23, 0, 0, 48, 0x23, 0, 0, 2,
            0xffff, 0, 27, 0, 0x10, 0x1f,
        ];
        units.extend([0x1c, 0x30, 12, 0, 0, 48, 0xff, 0, 12, 0, 0x30, 0x1f]);
        units.extend("甲".encode_utf16());
        units.extend([0x1c, 0x30, 12, 0, 52, 100, 0xff, 0, 12, 0, 0x30, 0x1f]);
        units.extend("乙".encode_utf16());
        units.extend([0x0e, 0, 0, 0, 0, 0, 0, 0]);
        let mut stream = b"SsmgV.01".to_vec();
        for u in units {
            stream.extend_from_slice(&u.to_be_bytes());
        }
        let units: Vec<u16> = stream[8..]
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        let input = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream(
            "DocumentText",
            decode::fixture(&units),
        )]);
        let doc = read(&input).unwrap();
        let flow = doc.flow.as_ref().unwrap();
        let table = flow
            .body
            .iter()
            .find_map(|b| match b {
                Block::Table(t) => Some(t),
                _ => None,
            })
            .expect("table");
        assert_eq!(table.rows[0].cells.len(), 3);
        assert!(table.borders.iter().all(|b| *b == Some(None)));
        let reopened = ibis_formats::read(&export_docx(&doc).unwrap()).unwrap();
        let saved = reopened.flow.unwrap();
        let table = saved
            .body
            .iter()
            .find_map(|b| match b {
                Block::Table(t) => Some(t),
                _ => None,
            })
            .expect("saved table");
        assert_eq!(table.rows[0].cells.len(), 3);
        assert!(
            matches!(&table.rows[0].cells[0].blocks[0], Block::Paragraph(p) if p.text() == "甲")
        );
        assert!(
            matches!(&table.rows[0].cells[2].blocks[0], Block::Paragraph(p) if p.text() == "乙")
        );
        assert!(table.borders.iter().all(|b| *b == Some(None)));
    }

    #[test]
    fn vertical_ruled_row_uses_its_pitch_for_cell_baselines_and_word_export() {
        let mut units: Vec<u16> = vec![
            0x1c, 0x10, 27, 0, 0x8f, 15, 100, 0, 3, 0x23, 0, 0, 45, 0x23, 0, 0, 45, 0x23, 0, 0, 0,
            0xffff, 0, 27, 0, 0x10, 0x1f,
        ];
        units.extend([0x1c, 0x30, 12, 0, 0, 48, 0xff, 0, 12, 0, 0x30, 0x1f]);
        units.extend("甲乙".encode_utf16());
        units.extend([0x1c, 0x30, 12, 0, 52, 100, 0xff, 0, 12, 0, 0x30, 0x1f]);
        units.extend("丙".encode_utf16());
        units.push(0x0e);
        let mut view = vec![0, 1, 0, 2, 0x10, 0];
        let record = [0x10, 2, 0, 4, 0, 0, 0x10, 1];
        view.extend((record.len() as u32).to_be_bytes());
        view.extend(record);
        let input = ibis_ooxml::cfb::write(&[
            ibis_ooxml::cfb::Item::stream("DocumentText", decode::fixture(&units)),
            ibis_ooxml::cfb::Item::stream("DocumentViewStyles", view.clone()),
        ]);
        let doc = read(&input).unwrap();
        let flow = doc.flow.as_ref().unwrap();
        assert_eq!(flow.page.direction, CellDirection::TbRl);
        let pitch = flow.page.line_pitch.unwrap();
        let table = flow.body.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).unwrap();
        assert_eq!(table.rows[0].height, RowHeight::Exact(pitch));
        for cell in &table.rows[0].cells {
            let Block::Paragraph(p) = &cell.blocks[0] else { panic!("cell paragraph") };
            assert_eq!(p.line, ibis_doc_model::LineHeight::Exact(pitch));
            assert_eq!(p.direct.line, Some(ibis_doc_model::LineHeight::Exact(pitch)));
        }
        let reopened = ibis_formats::read(&export_docx(&doc).unwrap()).unwrap();
        let table = reopened.flow.as_ref().unwrap().body.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).unwrap();
        for cell in &table.rows[0].cells {
            let Block::Paragraph(p) = &cell.blocks[0] else { panic!("saved cell paragraph") };
            assert!(matches!(p.line, ibis_doc_model::LineHeight::Exact(value) if (value - pitch).abs() < 635));
        }

        // An explicit source line feed still controls both row and paragraph.
        let payload = [0, 0x20, 4, 8, 500, 0, 0, 0xffff, 0];
        let length = (payload.len() + 7) as u16;
        let mut explicit = vec![0x1c, 0x10, length];
        explicit.extend(payload);
        explicit.extend([length, 0, 0x10, 0x1f]);
        explicit.extend(&units);
        let input = ibis_ooxml::cfb::write(&[
            ibis_ooxml::cfb::Item::stream("DocumentText", decode::fixture(&explicit)),
            ibis_ooxml::cfb::Item::stream("DocumentViewStyles", view),
        ]);
        let explicit_doc = read(&input).unwrap();
        let table = explicit_doc.flow.as_ref().unwrap().body.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).unwrap();
        assert_eq!(table.rows[0].height, RowHeight::Exact(180_000));
        let Block::Paragraph(p) = &table.rows[0].cells[0].blocks[0] else { panic!("explicit cell paragraph") };
        assert_eq!(p.line, ibis_doc_model::LineHeight::Exact(180_000));
    }

    #[test]
    fn vertical_ruled_rows_do_not_inherit_a_previous_paragraph_alignment() {
        use ibis_doc_model::FlowAlign;

        // An ordinary right-aligned paragraph is followed by ruled rows.
        // The first has no alignment of its own; the others explicitly set it.
        fn header(items: &[u16]) -> Vec<u16> {
            let length = (items.len() + 10) as u16;
            let mut out = vec![0x1c, 0x10, length, 0];
            out.extend_from_slice(items);
            out.extend([0xffff, 0, length, 0, 0x10, 0x1f]);
            out
        }
        fn row(alignment: Option<u16>) -> Vec<u16> {
            let mut items = Vec::new();
            if let Some(value) = alignment { items.extend([0x24, 1, value]); }
            items.extend([0x8f, 15, 100, 0, 3, 0x23, 0, 0, 45, 0x23, 0, 0, 45, 0x23, 0, 0, 0]);
            let mut out = header(&items);
            out.extend([0x1c, 0x30, 12, 0, 0, 48, 0xff, 0, 12, 0, 0x30, 0x1f]);
            out.extend("甲".encode_utf16());
            out.extend([0x1c, 0x30, 12, 0, 52, 100, 0xff, 0, 12, 0, 0x30, 0x1f]);
            out.extend("乙".encode_utf16());
            out.push(0x0e);
            out
        }
        let mut units = header(&[0x24, 1, 2]);
        units.extend("見出し\n".encode_utf16());
        units.extend(row(None));
        units.extend(row(Some(1)));
        units.extend(row(Some(2)));
        let mut view = vec![0, 1, 0, 2, 0x10, 0];
        let record = [0x10, 2, 0, 4, 0, 0, 0x10, 1];
        view.extend((record.len() as u32).to_be_bytes());
        view.extend(record);
        let input = ibis_ooxml::cfb::write(&[
            ibis_ooxml::cfb::Item::stream("DocumentText", decode::fixture(&units)),
            ibis_ooxml::cfb::Item::stream("DocumentViewStyles", view),
        ]);
        let model = read(&input).unwrap();
        let reopened = ibis_formats::read(&export_docx(&model).unwrap()).unwrap();
        for doc in [&model, &reopened] {
            let flow = doc.flow.as_ref().unwrap();
            assert!(matches!(&flow.body[0], Block::Paragraph(p) if p.align == FlowAlign::Right));
            let rows: Vec<_> = flow.body.iter().filter_map(|b| if let Block::Table(t) = b { Some(t.rows.iter()) } else { None }).flatten().collect();
            assert_eq!(rows.len(), 3);
            for (row, expected) in rows.iter().zip([FlowAlign::Left, FlowAlign::Center, FlowAlign::Right]) {
                let Block::Paragraph(p) = &row.cells[0].blocks[0] else { panic!("cell paragraph") };
                assert_eq!(p.align, expected);
            }
        }
        // The correction is specific to a vertical page. Horizontal ruled
        // rows keep their existing inherited paragraph alignment.
        let horizontal = ibis_ooxml::cfb::write(&[
            ibis_ooxml::cfb::Item::stream("DocumentText", decode::fixture(&units)),
        ]);
        let horizontal = read(&horizontal).unwrap();
        let table = horizontal.flow.as_ref().unwrap().body.iter().find_map(|b| {
            if let Block::Table(t) = b { Some(t) } else { None }
        }).unwrap();
        let Block::Paragraph(first) = &table.rows[0].cells[0].blocks[0] else { panic!("cell paragraph") };
        assert_eq!(first.align, FlowAlign::Right);
    }

    #[test]
    fn incomplete_or_overlapping_rows_remain_text() {
        let original = vec![
            Segment::RowStart(100),
            Segment::CellStart(0, 70),
            Segment::Text("a".into()),
            Segment::CellStart(60, 100),
            Segment::Text("b".into()),
            Segment::RowEnd,
        ];
        assert_eq!(table_candidates(original.clone()), original);
        assert_eq!(
            table_candidates(vec![
                Segment::RowStart(100),
                Segment::CellStart(0, 100),
                Segment::Text("a".into())
            ])
            .iter()
            .filter(|s| matches!(s, Segment::Table { .. }))
            .count(),
            0
        );
    }

    #[test]
    fn empty_edge_declarations_are_preserved() {
        let row = vec![
            Segment::RowStart(100),
            Segment::CellStart(0, 2),
            Segment::CellStart(6, 48),
            Segment::Text("left".into()),
            Segment::CellStart(52, 90),
            Segment::Text("right".into()),
            Segment::CellStart(94, 100),
            Segment::RowEnd,
        ];
        let result = table_candidates(row.clone());
        assert!(
            matches!(&result[..], [Segment::Table { cells, total: 100, strips: false, .. }] if cells.len() == 4)
        );
        let mut with_edge_text = row;
        with_edge_text.insert(2, Segment::Text("edge".into()));
        let result = table_candidates(with_edge_text);
        assert!(
            matches!(&result[..], [Segment::Table { cells, strips: false, .. }] if cells.len() == 4)
        );
    }

    #[test]
    fn narrow_inner_declarations_are_preserved() {
        let row = vec![
            Segment::RowStart(100),
            Segment::CellStart(0, 2),
            Segment::CellStart(6, 30),
            Segment::Text("A".into()),
            Segment::CellStart(34, 36),
            Segment::CellStart(40, 42),
            Segment::Text("※".into()),
            Segment::CellStart(46, 90),
            Segment::Text("B".into()),
            Segment::CellStart(94, 100),
            Segment::RowEnd,
        ];
        let result = table_candidates(row);
        assert!(
            matches!(&result[..], [Segment::Table { cells, strips: false, .. }]
            if cells.len() == 6 && cells[3].0 == 40 && matches!(&cells[3].2[..], [Segment::Text(t)] if t == "※"))
        );
    }

    #[test]
    fn adjacent_grids_preserve_cell_boundaries_as_spans() {
        fn table(columns: &[i64]) -> FlowTable {
            FlowTable {
                columns: columns.to_vec(),
                rows: vec![FlowRow {
                    cells: columns
                        .iter()
                        .map(|_| FlowCell {
                            span: 1,
                            ..FlowCell::default()
                        })
                        .collect(),
                    ..FlowRow::default()
                }],
                ..FlowTable::default()
            }
        }
        let mut first = table(&[50, 50]);
        assert!(merge_table_grids(&mut first, &table(&[25, 25, 50])));
        assert_eq!(first.columns, [25, 25, 50]);
        assert_eq!(
            first.rows[0]
                .cells
                .iter()
                .map(|c| c.span)
                .collect::<Vec<_>>(),
            [2, 1]
        );
        assert_eq!(
            first.rows[1]
                .cells
                .iter()
                .map(|c| c.span)
                .collect::<Vec<_>>(),
            [1, 1, 1]
        );
        let before = first.clone();
        assert!(!merge_table_grids(&mut first, &table(&[40, 40])));
        assert_eq!(first, before);
    }

    #[test]
    fn view_direction_needs_valid_evidence() {
        let mut record = vec![0, 1, 0, 2, 0x10, 0, 0, 0, 0, 6, 0x10, 1, 0, 2, 0, 0];
        assert!(!observed_vertical(&record));
        record[11] = 2;
        assert!(!observed_vertical(&record));
        record[11] = 1;
        record[13] = 3;
        assert!(!observed_vertical(&record));
        assert!(!observed_vertical(&record[..12]));
    }

    #[test]
    fn direction_flag_overrides_the_first_record_heuristic() {
        fn styles(vertical: bool) -> Vec<u8> {
            let mut records = vec![0x10, 0x01, 0, 2, 0, 0];
            let value = if vertical {
                vec![0, 0, 0x10, 1]
            } else {
                vec![0, 0, 0]
            };
            records.extend([0x10, 0x02, 0, value.len() as u8]);
            records.extend(value);
            let mut out = vec![0, 1, 0, 2, 0x10, 0];
            out.extend_from_slice(&(records.len() as u32).to_be_bytes());
            out.extend(records);
            out
        }
        assert_eq!(view_style_direction(&styles(false)), Some(false));
        assert!(!observed_vertical(&styles(false)));
        assert_eq!(view_style_direction(&styles(true)), Some(true));
        assert!(observed_vertical(&styles(true)));
        let mut broken = styles(true);
        broken.pop();
        assert_eq!(view_style_direction(&broken), None);
        let mut missing_fields = styles(false);
        *missing_fields.last_mut().unwrap() = 0x40;
        assert_eq!(view_style_direction(&missing_fields), None);
    }

    #[test]
    fn page_geometry_is_bounded_and_uses_the_vertical_text_axis() {
        let mut paper = vec![0x06];
        paper.extend_from_slice(&14_800u32.to_be_bytes());
        paper.extend_from_slice(&21_000u32.to_be_bytes());
        let margins = [0, 0x78, 0x06, 0x40, 0x06, 0x40, 0x07, 0xd0, 0x07, 0xd0, 0];
        let mut records = Vec::new();
        for (tag, payload) in [
            (0x1001u16, paper.as_slice()),
            (0x1002u16, margins.as_slice()),
            (0x100du16, [0x02, 0, 26].as_slice()),
        ] {
            records.extend_from_slice(&tag.to_be_bytes());
            records.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            records.extend_from_slice(payload);
        }
        let mut styles = vec![0, 1, 0, 2, 0x10, 0];
        styles.extend_from_slice(&(records.len() as u32).to_be_bytes());
        styles.extend(records);
        assert_eq!(view_style_lines(&styles), Some(26));
        let mut page = ibis_doc_model::PageSetup::default();
        apply_view_page(&styles, &mut page);
        assert_eq!(
            (page.size.width, page.size.height),
            (14_800 * 360, 21_000 * 360)
        );
        assert_eq!(
            (page.top, page.bottom, page.left, page.right),
            (1600 * 360, 1600 * 360, 2000 * 360, 2000 * 360)
        );
        assert_eq!(text_axis_width(&page), (14_800 - 4000) * 360);
        page.direction = CellDirection::TbRl;
        assert_eq!(text_axis_width(&page), (21_000 - 3200) * 360);
        let original = page.clone();
        styles.pop();
        apply_view_page(&styles, &mut page);
        assert_eq!(
            page, original,
            "a truncated group cannot partially change the page"
        );
        assert_eq!(view_style_lines(&styles), None);
    }

    #[test]
    fn page_line_count_requires_complete_known_fields() {
        fn styles(payload: &[u8]) -> Vec<u8> {
            let mut records = vec![0x10, 0x0d, 0, payload.len() as u8];
            records.extend_from_slice(payload);
            let mut result = vec![0, 1, 0, 2, 0x10, 0];
            result.extend_from_slice(&(records.len() as u32).to_be_bytes());
            result.extend(records);
            result
        }
        assert_eq!(view_style_lines(&styles(&[0x02, 0, 26])), Some(26));
        assert_eq!(view_style_lines(&styles(&[0x03, 7, 0, 45])), Some(45));
        assert_eq!(view_style_lines(&styles(&[0x0a, 0, 25, 0, 0, 0, 2])), Some(25));
        assert_eq!(view_style_lines(&styles(&[0x0b, 7, 0, 25, 0, 0, 0, 2])), Some(25));
        assert_eq!(view_style_lines(&styles(&[0x0a, 0, 25, 0, 0, 0])), None);
        assert_eq!(view_style_lines(&styles(&[0x0a, 0, 25, 0, 0, 0, 2, 0])), None);
        assert_eq!(view_style_lines(&styles(&[0x02, 0, 0])), None);
        assert_eq!(view_style_lines(&styles(&[0x06, 0, 26])), None);
        assert_eq!(view_style_lines(&styles(&[0x03, 7, 0])), None);
    }

    #[test]
    fn observed_page_break_is_preserved_in_docx() {
        let mut s = b"SsmgV.01".to_vec();
        for unit in [
            0x1c_u16, 0x10, 7, 7, 0, 0x10, 0x1f, 0x7532, 0x000c, 0x1c, 0x10, 7, 7, 0, 0x10, 0x1f,
            0x4e59,
        ] {
            s.extend_from_slice(&unit.to_be_bytes());
        }
        let units: Vec<u16> = s[8..]
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        let data = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream(
            "DocumentText",
            decode::fixture(&units),
        )]);
        let model = read(&data).unwrap();
        assert!(
            matches!(&model.flow.as_ref().unwrap().body[0], Block::Paragraph(p) if p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::PageBreak)))
        );
        let reopened = ibis_formats::read(&export_docx(&model).unwrap()).unwrap();
        assert!(
            matches!(&reopened.flow.unwrap().body[0], Block::Paragraph(p) if p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::PageBreak)))
        );
    }

    #[test]
    fn observed_ruby_pair_is_kept_in_word_model_docx_and_html() {
        let mut s = b"SsmgV.01".to_vec();
        for unit in [
            0x1c_u16, 1, 7, 0, 0, 3, 0x1d, 0x6f22, 0x1e, 5, 0, 1, 0x1f, 0x1c, 1, 7, 0, 1, 0x82,
            0x1d, 0x304b, 0x3093, 0x1e, 5, 0, 1, 0x1f,
        ] {
            s.extend_from_slice(&unit.to_be_bytes());
        }
        let units: Vec<u16> = s[8..]
            .chunks_exact(2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
            .collect();
        let data = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream(
            "DocumentText",
            decode::fixture(&units),
        )]);
        let model = read(&data).unwrap();
        let flow = model.flow.as_ref().unwrap();
        assert!(
            matches!(&flow.body[0], Block::Paragraph(p) if matches!(&p.runs[0].kind, FlowRunKind::Ruby {base, text, ..} if base == "漢" && text == "かん"))
        );
        let html = String::from_utf8(export_html(&model).unwrap()).unwrap();
        assert!(html.contains("<ruby>漢<rt>かん</rt></ruby>"));
        let docx = export_docx(&model).unwrap();
        let reopened = ibis_formats::read(&docx).unwrap();
        assert!(
            matches!(&reopened.flow.unwrap().body[0], Block::Paragraph(p) if p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Ruby {base, text, ..} if base == "漢" && text == "かん")))
        );
    }

    #[test]
    fn inline_footer_resumes_plain_text_without_a_new_record() {
        let data = decode::fixture(&[
            0x1c, 1, 7, 0, 0, 3, 0x1d, 0x7532, 0x1e, 5, 0, 1, 0x1f, 0x4e59, 0x4e19,
        ]);
        let plain: String = extract(&data)
            .unwrap()
            .into_iter()
            .filter_map(|s| match s {
                Segment::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(plain, "甲乙丙");
        assert_eq!(
            extract(&decode::fixture(&"文章".encode_utf16().collect::<Vec<_>>())).unwrap(),
            vec![Segment::Text("文章".into())]
        );
        let broken = decode::fixture(&[0x1c, 1, 7, 0, 0, 3, 0x1d, 0x7532, 0x1e]);
        assert!(extract(&broken).is_err());
    }

    #[test]
    #[ignore = "requires a locally authorized JTD sample"]
    fn local_sample_can_be_read() {
        let path = std::env::var_os("IBIS_JTD_SAMPLE").expect("IBIS_JTD_SAMPLE");
        let bytes = std::fs::read(path).unwrap();
        if std::env::var_os("IBIS_JTD_TABLE_STATS").is_some() {
            let mut stats = std::collections::BTreeMap::new();
            for segment in table_candidates(extract(&checked_stream(&bytes).unwrap()).unwrap()) {
                if let Segment::Table { cells, .. } = segment {
                    for (left, right, content) in cells {
                        let visible = content.iter().any(|s| match s {
                            Segment::Text(t) | Segment::StyledText(t, _) => !t.trim().is_empty(),
                            Segment::Ruby { .. } => true,
                            _ => false,
                        });
                        *stats
                            .entry((right - left, visible, content.is_empty()))
                            .or_insert(0usize) += 1;
                    }
                }
            }
            eprintln!("cell width, visible: {stats:?}");
        }
        let result = read(&bytes).unwrap();
        let word = export_docx(&result).unwrap();
        if let Some(path) = std::env::var_os("IBIS_JTD_DOCX_OUT") {
            std::fs::write(path, &word).unwrap();
        }
        let reopened = ibis_formats::read(&word).unwrap();
        let flow = result.flow.unwrap();
        let saved = reopened.flow.unwrap();
        assert_eq!(flow.page.direction, saved.page.direction);
        if let Ok(expected) = std::env::var("IBIS_JTD_PAGE_MM") {
            let values: Vec<i64> = expected
                .split(',')
                .map(|s| s.parse::<i64>().unwrap() * 36_000)
                .collect();
            assert_eq!(values.len(), 6);
            for page in [&flow.page, &saved.page] {
                for (actual, expected) in [
                    page.size.width,
                    page.size.height,
                    page.top,
                    page.bottom,
                    page.left,
                    page.right,
                ]
                .into_iter()
                .zip(&values)
                {
                    assert!(
                        (actual - expected).abs() <= 635,
                        "page value differs by more than one twip"
                    );
                }
            }
            if let Some(table) = flow.body.iter().find_map(|block| match block {
                Block::Table(t) => Some(t),
                _ => None,
            }) {
                let width: i64 = table.columns.iter().sum();
                assert!(
                    (width - text_axis_width(&flow.page)).abs() < 5_000,
                    "table width must follow the text axis"
                );
            }
        }
        if let Ok(expected) = std::env::var("IBIS_JTD_PAGE_LINES") {
            let count: i64 = expected.parse().unwrap();
            let cross_axis = if flow.page.direction.vertical() {
                flow.page.size.width - flow.page.left - flow.page.right
            } else {
                flow.page.size.height - flow.page.top - flow.page.bottom
            };
            let pitch = cross_axis / count;
            assert_eq!(flow.page.line_pitch, Some(pitch));
            assert!(
                saved
                    .page
                    .line_pitch
                    .is_some_and(|saved_pitch| (saved_pitch - pitch).abs() <= 635)
            );
            for row in flow
                .body
                .iter()
                .filter_map(|block| match block {
                    Block::Table(t) => Some(t.rows.as_slice()),
                    _ => None,
                })
                .flatten()
                .filter(|row| !row.lines.is_empty())
            {
                assert_eq!(row.height, RowHeight::Exact(pitch));
            }
            let original_tables = flow.body.iter().filter_map(|block| match block {
                Block::Table(t) => Some(t),
                _ => None,
            });
            let saved_tables = saved.body.iter().filter_map(|block| match block {
                Block::Table(t) => Some(t),
                _ => None,
            });
            for (original, saved_table) in original_tables.zip(saved_tables) {
                assert_eq!(original.rows.len(), saved_table.rows.len());
                for (before, after) in original.rows.iter().zip(&saved_table.rows) {
                    if !before.lines.is_empty() {
                        assert!(
                            matches!(after.height, RowHeight::Exact(h) if (h - pitch).abs() <= 635)
                        );
                    }
                }
            }
        }
        fn visit(
            blocks: &[Block],
            texts: &mut Vec<String>,
            rubies: &mut usize,
            tables: &mut usize,
        ) {
            for block in blocks {
                match block {
                    Block::Paragraph(p) => {
                        let mut plain = String::new();
                        for run in &p.runs {
                            match &run.kind {
                                FlowRunKind::Text(t) => plain.push_str(t),
                                FlowRunKind::Ruby { base, .. } => plain.push_str(base),
                                FlowRunKind::Tab => plain.push('\t'),
                                FlowRunKind::LineBreak => plain.push('\n'),
                                _ => {}
                            }
                        }
                        texts.push(plain);
                        *rubies += p
                            .runs
                            .iter()
                            .filter(|run| matches!(run.kind, FlowRunKind::Ruby { .. }))
                            .count();
                    }
                    Block::Table(t) => {
                        *tables += 1;
                        for row in &t.rows {
                            for cell in &row.cells {
                                visit(&cell.blocks, texts, rubies, tables);
                            }
                        }
                    }
                }
            }
        }
        let (mut texts, mut rubies, mut tables) = (Vec::new(), 0, 0);
        let (mut saved_texts, mut saved_rubies, mut saved_tables) = (Vec::new(), 0, 0);
        visit(&flow.body, &mut texts, &mut rubies, &mut tables);
        visit(
            &saved.body,
            &mut saved_texts,
            &mut saved_rubies,
            &mut saved_tables,
        );
        assert_eq!(rubies, saved_rubies);
        assert_eq!(tables, saved_tables);
        if let Ok(minimum) = std::env::var("IBIS_JTD_MIN_RUBY") {
            assert!(rubies >= minimum.parse().unwrap());
        }
        if let Ok(minimum) = std::env::var("IBIS_JTD_MIN_TABLES") {
            assert!(tables >= minimum.parse().unwrap(), "tables: {tables}");
        }
        if let Ok(minimum) = std::env::var("IBIS_JTD_MIN_ROW_LINES") {
            let count: usize = flow
                .body
                .iter()
                .filter_map(|b| match b {
                    Block::Table(t) => Some(t.rows.iter().map(|r| r.lines.len()).sum::<usize>()),
                    _ => None,
                })
                .sum();
            assert!(count >= minimum.parse().unwrap(), "row segments: {count}");
            assert!(
                saved
                    .body
                    .iter()
                    .filter_map(|b| match b {
                        Block::Table(t) =>
                            Some(t.rows.iter().map(|r| r.lines.len()).sum::<usize>()),
                        _ => None,
                    })
                    .all(|count| count == 0),
                "DOCX must not imply these display-only lines survived"
            );
            eprintln!("row segments: {count}");
        }
        let text = texts.join("\n");
        assert!(!text.trim().is_empty());
        assert_eq!(text, saved_texts.join("\n"));
        // Grouping cells must not silently drop or duplicate imported text.
        let source_text: String = extract(&checked_stream(&bytes).unwrap())
            .unwrap()
            .into_iter()
            .filter_map(|s| match s {
                Segment::Text(t) | Segment::StyledText(t, _) => Some(t),
                Segment::Ruby { base, .. } => Some(base),
                _ => None,
            })
            .collect();
        assert_eq!(
            source_text
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
            text.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        );
        if let Ok(expected) = std::env::var("IBIS_JTD_EXPECT") {
            assert!(text.contains(&expected));
        }
        if let Ok(direction) = std::env::var("IBIS_JTD_DIRECTION") {
            assert_eq!(flow.page.direction.vertical(), direction == "vertical");
        }
    }
    #[test]
    fn rejects_invalid_and_unsupported() {
        assert!(read(&[]).is_err());
        let b = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream(
            "DocumentText",
            b"SsmgV.01".to_vec(),
        )]);
        assert!(read(&b).is_err());
        let mut b = fixture("test");
        b[30] = 255;
        assert!(read(&b).is_err());
    }
}
