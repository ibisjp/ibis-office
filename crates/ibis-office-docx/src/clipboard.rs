// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! In-process document clipboard. Package-local IDs never cross documents.
//! Whole body paragraphs are imported at a paragraph boundary; other selections
//! use the text clipboard. The OS payload carries only a token, not source IDs.
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};
use gpui_kit::*;
use ibis_doc_model::{Block, FlowCommand, FlowParagraph, FlowPos, FlowRunKind, FlowRun, Link, RunFormat, InsertFormat};
use ibis_doc_model::flow_command::{self, Container};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[derive(Clone)]
struct Fragment {
    token: String,
    text: String,
    paragraphs: Vec<FlowParagraph>,
    source: ibis_doc_model::Presentation,
}
thread_local! {
    static TEXT_CLIPBOARD: RefCell<Option<(String, String, Vec<FlowRun>)>> = const { RefCell::new(None) };
    static CLIPBOARD: RefCell<Option<Fragment>> = const { RefCell::new(None) };
}
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[ibis_office_macros::editor_ext(Clipboard)]
impl SlideEditor {
    /// Resolve text properties before crossing a document boundary, including
    /// colour, highlight and URL links. Never import character-style IDs.
    pub(crate) fn copy_formatted_text(&self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        if start.para != end.para { return false; }
        let Some(p) = self.paragraph_of(start.para) else { return false };
        let mut offset = 0;
        let mut runs = Vec::new();
        for r in &p.runs {
            let len = r.len();
            let a = start.offset.saturating_sub(offset).min(len);
            let b = end.offset.saturating_sub(offset).min(len);
            offset += len;
            if a == b { continue; }
            let FlowRunKind::Text(value) = &r.kind else { return false };
            if r.revision.is_some() || matches!(r.link, Some(Link::Anchor(_))) { return false; }
            let mut run = r.clone();
            run.kind = FlowRunKind::Text(value.chars().skip(a).take(b - a).collect());
            run.format = resolved_format(r);
            run.char_style = None;
            runs.push(run);
        }
        let token = format!("ibis-docx-text-v2:{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        TEXT_CLIPBOARD.with(|c| *c.borrow_mut() = Some((token.clone(), text.into(), runs)));
        cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(text.into(), token));
        true
    }

    pub(crate) fn paste_formatted_text(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let fragment = TEXT_CLIPBOARD.with(|c| c.borrow().as_ref().filter(|(token, text, _)|
            item.metadata() == Some(token) && item.text().as_deref() == Some(text.as_str())).cloned());
        let Some((_, _, runs)) = fragment else { return false };
        let Some((start, end)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        let track = doc.track_changes;
        // Character fragments in one paragraph can preserve source formatting
        // and be recorded as one insertion. Structural tracked paste remains
        // outside this path.
        if track && start.para != end.para { return false; }
        let who = crate::comments::comment_author();
        let mut at = if track && start != end { super::editing::after_tracked_delete(doc,start,end) } else { start };
        let mut commands = Vec::new();
        if start != end {
            if track {
                commands.push(FlowCommand::TrackDelete { from:start, to:end, author:who.author.clone(), date:who.date.clone() });
            } else if start.para == end.para && !self.resources.markup_view.showing_markup() {
                let Some(p) = self.paragraph_of(start.para) else { return false; };
                commands.extend(super::editing::visible_delete_commands(&self.resources.markup_view, p,start.offset,end.offset));
            } else {
                commands.push(FlowCommand::Delete { from: start, to: end });
            }
        }
        for r in runs {
            let len = r.len();
            let FlowRunKind::Text(text) = r.kind else { return false };
            commands.push(FlowCommand::InsertText { at, text, format: Some(InsertFormat {
                format: r.format, char_style: None,
            }) });
            let to = FlowPos::new(at.para, at.offset + len);
            commands.push(FlowCommand::SetLink { from: at, to, link: r.link });
            if track {
                let revision = ibis_doc_model::Revision { kind:ibis_doc_model::RevisionKind::Insert, author:who.author.clone(), date:who.date.clone() };
                commands.push(FlowCommand::MarkRevision { from:at,to,revision:Some(revision) });
            }
            at = to;
        }
        if self.run_flow(commands, cx) {
            self.set_caret(at, false, cx);
            self.scroll_to_caret(cx);
        }
        true
    }

    /// Paste matching destination formatting (DOCX-EDIT-009).
    /// Inherits destination font, size, and color while preserving inline emphasis (bold, italic, underline, strike, highlight).
    pub(crate) fn paste_matching_format(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let fragment = TEXT_CLIPBOARD.with(|c| c.borrow().as_ref().filter(|(token, text, _)|
            item.metadata() == Some(token) && item.text().as_deref() == Some(text.as_str())).cloned());
        let Some((_, _, runs)) = fragment else { return false };
        let Some((start, end)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        let track = doc.track_changes;
        if track && start.para != end.para { return false; }
        let who = crate::comments::comment_author();
        let mut at = if track && start != end { super::editing::after_tracked_delete(doc, start, end) } else { start };
        let mut commands = Vec::new();
        if start != end {
            if track {
                commands.push(FlowCommand::TrackDelete { from: start, to: end, author: who.author.clone(), date: who.date.clone() });
            } else if start.para == end.para && !self.resources.markup_view.showing_markup() {
                let Some(p) = self.paragraph_of(start.para) else { return false; };
                commands.extend(super::editing::visible_delete_commands(&self.resources.markup_view, p, start.offset, end.offset));
            } else {
                commands.push(FlowCommand::Delete { from: start, to: end });
            }
        }
        let (dest_direct, _) = self.current_format().unwrap_or_default();
        for r in runs {
            let len = r.len();
            let FlowRunKind::Text(text) = r.kind else { return false };
            let mut merged = dest_direct.clone();
            if r.style.bold { merged.bold = Some(true); }
            if r.style.italic { merged.italic = Some(true); }
            if r.style.underline {
                merged.underline = Some(true);
                merged.underline_style = Some(r.style.underline_style);
                merged.underline_color = Some(r.style.underline_color);
            }
            if r.style.strike { merged.strike = Some(true); }
            if r.style.double_strike { merged.double_strike = Some(true); }
            if r.style.highlight.is_some() { merged.highlight = Some(r.style.highlight); }

            commands.push(FlowCommand::InsertText {
                at,
                text,
                format: Some(InsertFormat {
                    format: merged,
                    char_style: None,
                }),
            });
            let to = FlowPos::new(at.para, at.offset + len);
            commands.push(FlowCommand::SetLink { from: at, to, link: r.link });
            if track {
                let revision = ibis_doc_model::Revision { kind: ibis_doc_model::RevisionKind::Insert, author: who.author.clone(), date: who.date.clone() };
                commands.push(FlowCommand::MarkRevision { from: at, to, revision: Some(revision) });
            }
            at = to;
        }
        if self.run_flow(commands, cx) {
            self.set_caret(at, false, cx);
            self.scroll_to_caret(cx);
        }
        true
    }

    pub(crate) fn copy_paragraphs(&self, text: &str, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        let (Some((Container::Body, a)), Some((Container::Body, b))) =
            (flow_command::locate(doc, start.para), flow_command::locate(doc, end.para)) else { return false };
        if start.offset != 0 || a > b { return false; }
        let mut paragraphs = Vec::new();
        for block in &doc.body[a..=b] {
            let Block::Paragraph(p) = block else { return false };
            if p.section.is_some() || !p.bookmarks.is_empty() || p.field_start.is_some() || p.field_end
                || p.runs.iter().any(|r| r.revision.is_some() || matches!(r.link, Some(Link::Anchor(_)))
                    || !matches!(r.kind, FlowRunKind::Text(_) | FlowRunKind::Picture { .. } | FlowRunKind::PictureControl { .. } | FlowRunKind::Shape(_) | FlowRunKind::Tab | FlowRunKind::PositionalTab { .. } | FlowRunKind::LineBreak | FlowRunKind::CheckBox { .. } | FlowRunKind::DropDownList { .. } | FlowRunKind::DatePicker { .. } | FlowRunKind::RichText { .. } | FlowRunKind::PlainText { .. } | FlowRunKind::BuildingBlockGallery { .. } | FlowRunKind::RepeatingSection { .. } | FlowRunKind::Group { .. } | FlowRunKind::FormField(_))) { return false; }
            paragraphs.push(p.clone());
        }
        if end.offset != paragraphs.last().unwrap().len() { return false; }
        let token = format!("ibis-docx-paragraphs-v1:{}", NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        CLIPBOARD.with(|c| *c.borrow_mut() = Some(Fragment {
            token: token.clone(), text: text.into(), paragraphs, source: self.presentation().clone(),
        }));
        cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(text.into(), token));
        true
    }

    pub(crate) fn paste_paragraphs(&mut self, item: &ClipboardItem, cx: &mut Context<Self>) -> bool {
        let fragment = CLIPBOARD.with(|c| c.borrow().as_ref().filter(|f|
            item.metadata() == Some(&f.token) && item.text().as_deref() == Some(f.text.as_str())).cloned());
        let Some(mut f) = fragment else { return false };
        let Some((start, end)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        if start != end || start.offset != 0 || doc.track_changes { return false; }
        let Some((Container::Body, index)) = flow_command::locate(doc, start.para) else { return false };
        let source = f.source.flow.as_ref().unwrap();
        let mut numbering = doc.styles.numbering.clone();
        let mut abstracts = BTreeMap::new();
        for (id, value) in &source.styles.numbering.abstracts {
            let new = numbering.abstracts.keys().next_back().copied().unwrap_or(0) + 1;
            abstracts.insert(*id, new);
            numbering.abstracts.insert(new, value.clone());
        }
        let mut nums = BTreeMap::new();
        for (id, value) in &source.styles.numbering.nums {
            let Some(&abstract_id) = abstracts.get(&value.abstract_id) else { return false };
            let new = numbering.nums.keys().next_back().copied().unwrap_or(0) + 1;
            nums.insert(*id, new);
            let mut value = value.clone();
            value.abstract_id = abstract_id;
            numbering.nums.insert(new, value);
        }
        let mut styles = BTreeMap::new();
        for id in source.styles.styles.keys() {
            let mut new = format!("IbisPaste_{id}");
            while doc.styles.styles.contains_key(&new) || styles.values().any(|v| v == &new) { new.push('_'); }
            styles.insert(id.clone(), new);
        }
        let mut imported_styles = (*doc.styles).clone();
        imported_styles.numbering = numbering.clone();
        let mut commands = vec![FlowCommand::SetNumbering { numbering }];
        for (id, value) in &source.styles.styles {
            let mut value = value.clone();
            value.based_on = value.based_on.as_ref().and_then(|id| styles.get(id).cloned());
            value.para.num_id = value.para.num_id.map(|id| if id == 0 { 0 } else { nums.get(&id).copied().unwrap_or(0) });
            imported_styles.styles.insert(styles[id].clone(), value.clone());
            commands.push(FlowCommand::SetStyle { id: styles[id].clone(), style: Some(value) });
        }
        if f.paragraphs.iter().any(|p| p.list.is_some_and(|list| !nums.contains_key(&list.num_id))
            || p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Picture { media, .. } | FlowRunKind::PictureControl { media, .. }
                if !f.source.media.iter().any(|m| m.id == media)))) { return false; }
        let mut next = doc.new_id();
        let mut media_map = BTreeMap::new();
        for p in &mut f.paragraphs {
            p.id = next;
            next += 1;
            p.style = p.style.as_ref().or(source.styles.default_paragraph.as_ref()).and_then(|id| styles.get(id).cloned());
            p.table_style = None;
            // Materialize inherited properties so destination defaults cannot leak.
            p.direct.align = Some(p.align);
            p.direct.left = Some(p.left); p.direct.right = Some(p.right); p.direct.first_line = Some(p.first_line);
            p.direct.before = Some(p.space_before); p.direct.after = Some(p.space_after); p.direct.line = Some(p.line);
            p.direct.num_id = Some(p.list.map_or(0, |list| nums[&list.num_id]));
            p.direct.level = p.list.map(|list| list.level);
            for r in &mut p.runs {
                r.format = resolved_format(r);
                r.char_style = r.char_style.as_ref().and_then(|id| styles.get(id).cloned());
                if let FlowRunKind::Picture { media, .. } | FlowRunKind::PictureControl { media, .. } = &mut r.kind {
                    if let Some(&id) = media_map.get(media) { *media = id; continue; }
                    let Some(m) = f.source.media.iter().find(|m| m.id == *media) else { return false };
                    let id = self.document.ids().add_media(m.format, m.data.clone());
                    let format = match m.format {
                        ibis_doc_model::MediaFormat::Png => Some(ImageFormat::Png),
                        ibis_doc_model::MediaFormat::Jpeg => Some(ImageFormat::Jpeg),
                        ibis_doc_model::MediaFormat::Gif => Some(ImageFormat::Gif),
                        ibis_doc_model::MediaFormat::Bmp => Some(ImageFormat::Bmp),
                        ibis_doc_model::MediaFormat::Tiff => Some(ImageFormat::Tiff),
                        ibis_doc_model::MediaFormat::Svg => Some(ImageFormat::Svg),
                        _ => None,
                    };
                    if let Some(format) = format { self.resources.images.insert(id, Arc::new(Image::from_bytes(format, m.data.to_vec()))); }
                    media_map.insert(*media, id);
                    *media = id;
                }
            }
        }
        for p in &mut f.paragraphs { p.resolve(&imported_styles); }
        let caret = FlowPos::new(f.paragraphs.last().unwrap().id, f.paragraphs.last().unwrap().len());
        commands.push(FlowCommand::ReplaceBlockRange { container: Container::Body, index, remove: 0,
            blocks: f.paragraphs.into_iter().map(Block::Paragraph).collect() });
        if self.run_flow(commands, cx) {
            self.set_caret(caret, false, cx);
            self.scroll_to_caret(cx);
        }
        true
    }

    /// Copy the current selection (or current paragraph when nothing is selected) to the system clipboard as Markdown.
    pub(crate) fn copy_as_markdown(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let blocks: Vec<Block> = if let Some((start, end)) = self.ordered_selection() {
            if start.para == end.para && start != end {
                if let Some(p) = self.paragraph_of(start.para) {
                    let mut sliced = p.clone();
                    let mut offset = 0usize;
                    let mut runs = Vec::new();
                    for r in &p.runs {
                        let len = r.len();
                        let a = start.offset.saturating_sub(offset).min(len);
                        let b = end.offset.saturating_sub(offset).min(len);
                        offset += len;
                        if a == b {
                            continue;
                        }
                        let mut run = r.clone();
                        if let FlowRunKind::Text(value) = &r.kind {
                            run.kind = FlowRunKind::Text(value.chars().skip(a).take(b - a).collect());
                        }
                        runs.push(run);
                    }
                    sliced.runs = runs;
                    vec![Block::Paragraph(sliced)]
                } else {
                    Vec::new()
                }
            } else if start != end {
                let mut collected = Vec::new();
                let mut in_range = false;
                for block in &doc.body {
                    let contains_start = block_contains_para(block, start.para);
                    let contains_end = block_contains_para(block, end.para);
                    if contains_start {
                        in_range = true;
                    }
                    if in_range {
                        collected.push(block.clone());
                    }
                    if contains_end {
                        break;
                    }
                }
                collected
            } else if let Some(p) = self.paragraph_of(start.para) {
                vec![Block::Paragraph(p.clone())]
            } else {
                doc.body.clone()
            }
        } else {
            doc.body.clone()
        };
        if blocks.is_empty() {
            return false;
        }
        let md = ibis_formats::markdown::export_blocks_to_markdown(&blocks, doc, &self.presentation().media);
        if md.is_empty() {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(md));
        true
    }

    /// Paste Markdown text from the system clipboard as rich formatted blocks (headings, lists, tables, inline formatting).
    pub(crate) fn paste_markdown(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some((start, end)) = self.ordered_selection()
            && self.reject_hidden_history_range(start, end, cx)
        {
            return false;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        if text.trim().is_empty() {
            return false;
        }
        let Some((start, end)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        let Some((Container::Body, index)) = flow_command::locate(doc, start.para) else {
            // Inside a table cell or header/footer: fall back to plain text insertion.
            self.type_text(&text, cx);
            return true;
        };

        let mut temp_doc = (**doc).clone();
        let mut media = self.presentation().media.clone();
        let prev_media_len = media.len();
        let Ok(mut blocks) = ibis_formats::markdown::parse_markdown_fragment_blocks(&text, &mut temp_doc, &mut media) else {
            return false;
        };
        if blocks.is_empty() {
            return false;
        }

        let mut commands = Vec::new();
        if temp_doc.styles.numbering != doc.styles.numbering {
            commands.push(FlowCommand::SetNumbering {
                numbering: temp_doc.styles.numbering.clone(),
            });
        }
        for (id, style) in &temp_doc.styles.styles {
            if doc.styles.styles.get(id) != Some(style) {
                commands.push(FlowCommand::SetStyle {
                    id: id.clone(),
                    style: Some(style.clone()),
                });
            }
        }

        // Register any newly decoded inline images in document media & resource cache.
        let mut media_remap = BTreeMap::new();
        for new_m in media.iter().skip(prev_media_len) {
            let new_id = self.document.ids().add_media(new_m.format, new_m.data.clone());
            let format = match new_m.format {
                ibis_doc_model::MediaFormat::Png => Some(ImageFormat::Png),
                ibis_doc_model::MediaFormat::Jpeg => Some(ImageFormat::Jpeg),
                ibis_doc_model::MediaFormat::Gif => Some(ImageFormat::Gif),
                ibis_doc_model::MediaFormat::Bmp => Some(ImageFormat::Bmp),
                ibis_doc_model::MediaFormat::Tiff => Some(ImageFormat::Tiff),
                ibis_doc_model::MediaFormat::Svg => Some(ImageFormat::Svg),
                _ => None,
            };
            if let Some(format) = format {
                self.resources.images.insert(new_id, Arc::new(Image::from_bytes(format, new_m.data.to_vec())));
            }
            media_remap.insert(new_m.id, new_id);
        }
        if !media_remap.is_empty() {
            for block in &mut blocks {
                remap_block_media(block, &media_remap);
            }
        }

        let caret = last_caret_in_blocks(&blocks).unwrap_or(start);
        let replace_empty_para = start == end
            && self.paragraph_of(start.para).is_some_and(|p| p.text().is_empty());
        let (insert_at, remove_count) = if replace_empty_para {
            (index, 1)
        } else {
            (index + 1, 0)
        };
        commands.push(FlowCommand::ReplaceBlockRange {
            container: Container::Body,
            index: insert_at,
            remove: remove_count,
            blocks,
        });
        if self.run_flow(commands, cx) {
            self.set_caret(caret, false, cx);
            self.scroll_to_caret(cx);
            return true;
        }
        false
    }
}

fn block_contains_para(block: &Block, target: u32) -> bool {
    match block {
        Block::Paragraph(p) => p.id == target,
        Block::Table(t) => t.rows.iter().any(|r| {
            r.cells
                .iter()
                .any(|c| c.blocks.iter().any(|b| block_contains_para(b, target)))
        }),
    }
}

fn remap_block_media(block: &mut Block, remap: &BTreeMap<ibis_doc_model::MediaId, ibis_doc_model::MediaId>) {
    match block {
        Block::Paragraph(p) => {
            for r in &mut p.runs {
                if let FlowRunKind::Picture { media, .. } | FlowRunKind::PictureControl { media, .. } = &mut r.kind
                    && let Some(&new_id) = remap.get(media)
                {
                    *media = new_id;
                }
            }
        }
        Block::Table(t) => {
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    for b in &mut cell.blocks {
                        remap_block_media(b, remap);
                    }
                }
            }
        }
    }
}

fn last_caret_in_blocks(blocks: &[Block]) -> Option<FlowPos> {
    for block in blocks.iter().rev() {
        match block {
            Block::Paragraph(p) => return Some(FlowPos::new(p.id, p.len())),
            Block::Table(t) => {
                if let Some(pos) = t
                    .rows
                    .iter()
                    .rev()
                    .flat_map(|r| r.cells.iter().rev())
                    .find_map(|c| last_caret_in_blocks(&c.blocks))
                {
                    return Some(pos);
                }
            }
        }
    }
    None
}

/// Materialize all supported run properties so destination defaults cannot leak.
pub(super) fn resolved_format(r: &FlowRun) -> RunFormat {
    let s = &r.style;
    RunFormat { font_theme: None, east_asian_font_theme: None, h_ansi_font_theme: None, cs_font_theme: None, font: s.font.clone(), h_ansi_font: s.h_ansi_font.clone(), east_asian_font: s.east_asian_font.clone(), cs_font: s.cs_font.clone(), east_asia_hint: Some(s.east_asia_hint), size: Some(s.size), cs_size: s.cs_size,
        bold: Some(s.bold), italic: Some(s.italic), underline: Some(s.underline), underline_style: Some(s.underline_style),
        underline_color: Some(s.underline_color), strike: Some(s.strike), double_strike: Some(s.double_strike), caps: Some(s.caps), small_caps: Some(s.small_caps),
        color: Some(s.color), theme_color: r.format.theme_color.clone(), highlight: Some(s.highlight), shd_pattern: None, effects: None, border: Some(s.border.clone()),
        position: Some(s.position), raise: Some(s.raise), spacing: Some(s.spacing), scale: Some(s.scale), fit_text: Some(s.fit_text), hidden: r.format.hidden, kern: Some(s.kern.unwrap_or(0)), emphasis: Some(s.emphasis), combine: s.combine.map(Some), tate_chu_yoko: Some(s.tate_chu_yoko), tate_chu_yoko_fit: Some(s.tate_chu_yoko_fit),
        outline: Some(s.outline), shadow: Some(s.shadow), emboss: Some(s.emboss), imprint: Some(s.imprint), no_proof: r.format.no_proof,
        lang: r.format.lang.clone(), lang_east_asia: r.format.lang_east_asia.clone(), lang_bidi: r.format.lang_bidi.clone(),
        ligatures: Some(s.ligatures), num_form: Some(s.num_form), num_spacing: Some(s.num_spacing), contextual_alternates: Some(s.contextual_alternates), enclosure_sizes: s.enclosure_sizes }
}
