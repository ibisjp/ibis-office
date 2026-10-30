// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 検索と置換 (R1, R2): finding text in the document and replacing it, and
//! the sidebar panel that does it (編集 > 検索と置換…, ⇧⌘H).
//!
//! A match is within one paragraph (across its runs), or ends with the
//! paragraph's mark (`^p`). Replacing keeps the formatting of the match's
//! first character, as Word does.
//!
//! Word's special characters in what is searched for and what replaces it:
//! `^p` the paragraph mark (in a search, only at its end), `^t` a tab,
//! `^l` a line break, `^m` a page break, `^n` a column break,
//! `^s` a non-breaking space, `^-` an optional hyphen, `^~` a non-breaking hyphen,
//! `^+` an em dash, `^=` an en dash, `^^` a caret.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::flow_command::{self, ParaChange, RunChange};
use ibis_doc_model::{Block, FlowAlign, FlowCommand, FlowDocument, FlowPos, InsertFormat, Rgb};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// 検索の方向 (Word: 検索オプション > 検索の方向 / Search Direction: All, Down, Up).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SearchDirection {
    #[default]
    All,
    Down,
    Up,
}

/// 検索対象の範囲 (Word: 検索オプション > 検索する場所 / Search in).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum SearchScope {
    #[default]
    All,
    Main,
    HeaderFooter,
    Notes,
    Comments,
    Selection,
}

/// 書式・スタイルによる検索条件 (Word: 検索オプション > 書式 / Find Format).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FindFormatting {
    pub(crate) bold: Option<bool>,
    pub(crate) italic: Option<bool>,
    pub(crate) underline: Option<bool>,
    pub(crate) strike: Option<bool>,
    pub(crate) color: Option<Rgb>,
    pub(crate) highlight: Option<Option<Rgb>>,
    pub(crate) align: Option<FlowAlign>,
    pub(crate) style: Option<String>,
}

impl FindFormatting {
    pub(crate) fn is_empty(&self) -> bool {
        self.bold.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.strike.is_none()
            && self.color.is_none()
            && self.highlight.is_none()
            && self.align.is_none()
            && self.style.is_none()
    }

    pub(crate) fn has_character_formatting(&self) -> bool {
        self.bold.is_some()
            || self.italic.is_some()
            || self.underline.is_some()
            || self.strike.is_some()
            || self.color.is_some()
            || self.highlight.is_some()
    }

    pub(crate) fn matches_para(&self, doc: &FlowDocument, p: &ibis_doc_model::FlowParagraph) -> bool {
        if let Some(al) = self.align {
            if p.align != al {
                return false;
            }
        }
        if let Some(ref target) = self.style {
            let p_style_id = p.style.as_deref().unwrap_or("Normal");
            let target_id = resolve_style_id(doc, target);
            let mut matched = p_style_id.eq_ignore_ascii_case(&target_id);
            if !matched {
                if let Some(s) = doc.styles.styles.get(p_style_id) {
                    matched = s.name.eq_ignore_ascii_case(target);
                }
            }
            if !matched {
                return false;
            }
        }
        true
    }

    pub(crate) fn matches_run_style(&self, style: &ibis_doc_model::FlowRunStyle) -> bool {
        if let Some(b) = self.bold {
            if style.bold != b {
                return false;
            }
        }
        if let Some(it) = self.italic {
            if style.italic != it {
                return false;
            }
        }
        if let Some(u) = self.underline {
            if style.underline != u {
                return false;
            }
        }
        if let Some(st) = self.strike {
            if style.strike != st {
                return false;
            }
        }
        if let Some(c) = self.color {
            if style.color != c {
                return false;
            }
        }
        if let Some(hl) = self.highlight {
            if style.highlight != hl {
                return false;
            }
        }
        true
    }

    pub(crate) fn description(&self) -> String {
        let mut parts = Vec::new();
        if let Some(b) = self.bold {
            parts.push(if b { crate::shell::i18n::text("_shell.bold") } else { crate::shell::i18n::text("_docx.not-bold") }.to_string());
        }
        if let Some(it) = self.italic {
            parts.push(if it { crate::shell::i18n::text("docx.find.format.italic") } else { crate::shell::i18n::text("_docx.not-italic") }.to_string());
        }
        if let Some(u) = self.underline {
            parts.push(if u { crate::shell::i18n::text("docx.find.format.underline") } else { crate::shell::i18n::text("_docx.not-underline") }.to_string());
        }
        if let Some(st) = self.strike {
            parts.push(if st { crate::shell::i18n::text("docx.find.format.strike") } else { crate::shell::i18n::text("_docx.not-strikethrough") }.to_string());
        }
        if let Some(c) = self.color {
            parts.push(format!("#{:06x}", c.0));
        }
        if let Some(ref st) = self.style {
            parts.push(crate::shell::i18n::format("_docx.style-st", &[("st", &st)]));
        }
        parts.join(", ")
    }
}

/// 書式・スタイルによる置換条件 (Word: 置換オプション > 書式 / Replace Format).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReplaceFormatting {
    pub(crate) bold: Option<bool>,
    pub(crate) italic: Option<bool>,
    pub(crate) underline: Option<bool>,
    pub(crate) strike: Option<bool>,
    pub(crate) color: Option<Rgb>,
    pub(crate) highlight: Option<Option<Rgb>>,
    pub(crate) align: Option<FlowAlign>,
    pub(crate) style: Option<String>,
}

impl ReplaceFormatting {
    pub(crate) fn is_empty(&self) -> bool {
        self.bold.is_none()
            && self.italic.is_none()
            && self.underline.is_none()
            && self.strike.is_none()
            && self.color.is_none()
            && self.highlight.is_none()
            && self.align.is_none()
            && self.style.is_none()
    }

    pub(crate) fn description(&self) -> String {
        let mut parts = Vec::new();
        if let Some(b) = self.bold {
            parts.push(if b { crate::shell::i18n::text("_shell.bold") } else { crate::shell::i18n::text("_docx.remove-bold") }.to_string());
        }
        if let Some(it) = self.italic {
            parts.push(if it { crate::shell::i18n::text("docx.find.format.italic") } else { crate::shell::i18n::text("_docx.remove-italic") }.to_string());
        }
        if let Some(u) = self.underline {
            parts.push(if u { crate::shell::i18n::text("docx.find.format.underline") } else { crate::shell::i18n::text("_docx.remove-underline") }.to_string());
        }
        if let Some(st) = self.strike {
            parts.push(if st { crate::shell::i18n::text("docx.find.format.strike") } else { crate::shell::i18n::text("_docx.remove-strikethrough") }.to_string());
        }
        if let Some(c) = self.color {
            parts.push(format!("#{:06x}", c.0));
        }
        if let Some(ref st) = self.style {
            parts.push(crate::shell::i18n::format("_docx.style-st", &[("st", &st)]));
        }
        parts.join(", ")
    }
}

pub(crate) fn resolve_style_id(doc: &FlowDocument, name_or_id: &str) -> String {
    if doc.styles.styles.contains_key(name_or_id) {
        name_or_id.to_string()
    } else if let Some(id) = doc.styles.paragraph_style_named(name_or_id) {
        id.to_string()
    } else {
        name_or_id.to_string()
    }
}

/// The panel's fields and what the last search found.
pub(crate) struct FindState {
    pub(crate) query: Entity<InputState>,
    pub(crate) replacement: Entity<InputState>,
    pub(crate) matches: Vec<(FlowPos, FlowPos)>,
    /// The match shown (selected).
    pub(crate) current: Option<usize>,
    pub(crate) case_sensitive: bool,
    pub(crate) whole_words: bool,
    pub(crate) wildcards: bool,
    pub(crate) direction: SearchDirection,
    pub(crate) scope: SearchScope,
    pub(crate) selection_range: Option<(FlowPos, FlowPos)>,
    pub(crate) last_query: String,
    pub(crate) find_format: FindFormatting,
    pub(crate) replace_format: ReplaceFormatting,
    pub(crate) format_section_open: bool,
    pub(crate) replace_mode: bool,
    _subscription: Subscription,
}

/// The paragraphs searched, in order: the body (with its tables), the
/// section-owned/default/first/even headers and footers, notes, and comments.
pub(super) fn paragraphs(doc: &FlowDocument, scope: SearchScope) -> Vec<(u32, Vec<(usize, char)>)> {
    fn walk(blocks: &[Block], out: &mut Vec<(u32, Vec<(usize, char)>)>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.push((p.id, super::editing::visible_chars(p))),
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    match scope {
        SearchScope::All | SearchScope::Selection => {
            walk(&doc.body, &mut out);
            for block in &doc.body {
                if let Block::Paragraph(p) = block && let Some(section) = &p.section {
                    for blocks in section.parts.iter().flatten() { walk(blocks, &mut out); }
                }
            }
            for blocks in [&doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even] {
                walk(blocks, &mut out);
            }
            for notes in [&doc.footnotes, &doc.endnotes] {
                for note in notes { walk(&note.blocks, &mut out); }
            }
            for comment in &doc.comments {
                walk(&comment.blocks, &mut out);
            }
        }
        SearchScope::Main => {
            walk(&doc.body, &mut out);
        }
        SearchScope::HeaderFooter => {
            for block in &doc.body {
                if let Block::Paragraph(p) = block && let Some(section) = &p.section {
                    for blocks in section.parts.iter().flatten() { walk(blocks, &mut out); }
                }
            }
            for blocks in [&doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even] {
                walk(blocks, &mut out);
            }
        }
        SearchScope::Notes => {
            for notes in [&doc.footnotes, &doc.endnotes] {
                for note in notes { walk(&note.blocks, &mut out); }
            }
        }
        SearchScope::Comments => {
            for comment in &doc.comments {
                walk(&comment.blocks, &mut out);
            }
        }
    }
    // An inherited part is searched once, rather than once per displayed page.
    let mut seen = std::collections::HashSet::new();
    out.retain(|(id, _)| seen.insert(*id));
    out
}

/// Compares two positions in document order based on paragraph list order.
pub(crate) fn pos_cmp(paras_order: &[u32], a: FlowPos, b: FlowPos) -> std::cmp::Ordering {
    if a.para == b.para {
        a.offset.cmp(&b.offset)
    } else {
        let ia = paras_order.iter().position(|&id| id == a.para).unwrap_or(usize::MAX);
        let ib = paras_order.iter().position(|&id| id == b.para).unwrap_or(usize::MAX);
        ia.cmp(&ib)
    }
}

/// Every match of `query` (ignoring case), in order.
pub(crate) fn find_all(doc: &FlowDocument, query: &str) -> Vec<(FlowPos, FlowPos)> {
    find_all_with_options(doc, query, false, false, false, SearchScope::All, None, None)
}

/// `text` with Word's special characters made the characters they stand
/// for (`^p` paragraph mark, `^t` tab, `^l` line break, `^m` page break,
/// `^n` column break, `^s` non-breaking space, `^-` optional hyphen,
/// `^~` non-breaking hyphen, `^+` em dash, `^=` en dash, `^^` caret).
pub(crate) fn special_characters(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '^' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('p' | 'P') => out.push('\n'),
            Some('t' | 'T') => out.push('\t'),
            Some('l' | 'L') => out.push('\u{b}'),
            Some('m' | 'M') => out.push('\u{c}'),
            Some('n' | 'N') => out.push('\u{e}'),
            Some('s' | 'S') => out.push('\u{a0}'),
            Some('-') => out.push('\u{ad}'),
            Some('~') => out.push('\u{2011}'),
            Some('+') => out.push('\u{2014}'),
            Some('=') => out.push('\u{2013}'),
            Some('^') => out.push('^'),
            Some(other) => {
                out.push('^');
                out.push(other);
            }
            None => out.push('^'),
        }
    }
    out
}

/// The paragraph after paragraph `id` in the same list of blocks (the
/// body, a cell, a header, a note), if the next block is one.
fn next_in_list(doc: &FlowDocument, id: u32) -> Option<u32> {
    fn walk(blocks: &[Block], id: u32) -> Option<Option<u32>> {
        for (i, b) in blocks.iter().enumerate() {
            match b {
                Block::Paragraph(p) if p.id == id => return Some(match blocks.get(i + 1) {
                    Some(Block::Paragraph(next)) => Some(next.id),
                    _ => None,
                }),
                Block::Table(t) => {
                    for cell in t.rows.iter().flat_map(|r| &r.cells) {
                        if let Some(found) = walk(&cell.blocks, id) {
                            return Some(found);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    let mut lists: Vec<&[Block]> = vec![&doc.body, &doc.header, &doc.footer, &doc.header_first, &doc.footer_first, &doc.header_even, &doc.footer_even];
    for block in &doc.body {
        if let Block::Paragraph(p) = block && let Some(section) = &p.section {
            lists.extend(section.parts.iter().flatten().map(Vec::as_slice));
        }
    }
    for notes in [&doc.footnotes, &doc.endnotes] {
        lists.extend(notes.iter().map(|n| n.blocks.as_slice()));
    }
    for comment in &doc.comments {
        lists.push(comment.blocks.as_slice());
    }
    lists.into_iter().find_map(|l| walk(l, id)).flatten()
}

pub(crate) fn find_all_with_options(
    doc: &FlowDocument,
    query: &str,
    case_sensitive: bool,
    whole_words: bool,
    wildcards: bool,
    scope: SearchScope,
    selection_range: Option<(FlowPos, FlowPos)>,
    find_format: Option<&FindFormatting>,
) -> Vec<(FlowPos, FlowPos)> {
    let empty_fmt = FindFormatting::default();
    let fmt = find_format.unwrap_or(&empty_fmt);
    if query.is_empty() && fmt.is_empty() {
        return Vec::new();
    }
    if scope == SearchScope::Selection && selection_range.is_none() {
        return Vec::new();
    }
    let paras = paragraphs(doc, scope);
    let mut out = Vec::new();

    if query.is_empty() {
        // 書式のみの検索: 書式条件に一致する連続した範囲を収集
        for (id, text) in paras {
            let Some(p) = flow_command::paragraph(doc, id) else { continue };
            if !fmt.matches_para(doc, p) { continue; }
            if fmt.has_character_formatting() {
                let mut start: Option<usize> = None;
                let mut prev_offset = 0;
                for &(offset, _) in &text {
                    let (_, _, style) = super::editing::visible_format_at(p, offset + 1);
                    if fmt.matches_run_style(&style) {
                        if start.is_none() {
                            start = Some(offset);
                        }
                        prev_offset = offset;
                    } else if let Some(s) = start {
                        out.push((FlowPos::new(id, s), FlowPos::new(id, prev_offset + 1)));
                        start = None;
                    }
                }
                if let Some(s) = start {
                    out.push((FlowPos::new(id, s), FlowPos::new(id, prev_offset + 1)));
                }
            } else if p.len() > 0 {
                out.push((FlowPos::new(id, 0), FlowPos::new(id, p.len())));
            }
        }
    } else if wildcards {
        let Ok(pattern) = super::wildcard::parse_wildcard(query) else {
            return Vec::new();
        };
        for (id, text) in paras {
            let text_chars: Vec<char> = text.iter().map(|(_, c)| *c).collect();
            let matches = super::wildcard::find_all_matches(&text_chars, &pattern, case_sensitive);
            for m in matches {
                if m.start >= text.len() && m.end >= text.len() {
                    continue;
                }
                let from = text.get(m.start).map_or(0, |x| x.0);
                let to = if m.end < text.len() {
                    text[m.end].0
                } else {
                    text.last().map_or(0, |x| x.0 + 1)
                };
                out.push((FlowPos::new(id, from), FlowPos::new(id, to)));
            }
        }
    } else {
        let query = special_characters(query);
        // `^p` ends the match at the paragraph's end (and selects its mark).
        let (query, paragraph_mark) = match query.strip_suffix('\n') {
            Some(rest) => (rest.to_string(), true),
            None => (query, false),
        };
        if !query.contains('\n') && (!query.is_empty() || paragraph_mark) {
            let needle: Vec<char> = if case_sensitive { query.chars().collect() } else { query.chars().flat_map(char::to_lowercase).collect() };
            for (id, text) in paras {
                if paragraph_mark {
                    // The text just before the mark, then the mark (to the start of
                    // the next paragraph, which the paragraph joins if replaced).
                    let Some(next) = next_in_list(doc, id) else { continue };
                    let len = flow_command::paragraph(doc, id).map_or(0, |p| p.len());
                    let lower: Vec<(usize, char)> = if case_sensitive { text.clone() } else { text.iter().flat_map(|&(o, c)| c.to_lowercase().map(move |c| (o, c))).collect() };
                    if lower.len() >= needle.len() && lower[lower.len() - needle.len()..].iter().map(|(_, c)| *c).eq(needle.iter().copied()) {
                        let start = lower.len() - needle.len();
                        let begins_character = start == 0 || lower[start - 1].0 != lower.get(start).map_or(usize::MAX, |x| x.0);
                        if begins_character {
                            let from = lower.get(start).map_or(len, |x| x.0);
                            out.push((FlowPos::new(id, from), FlowPos::new(next, 0)));
                        }
                    }
                    continue;
                }
                // Lowercase both sides fully and carry each scalar's model position.
                // Expansions such as İ -> i + combining dot still select one source
                // character; replacement must never select part of that expansion.
                let lower: Vec<(usize, char)> = if case_sensitive { text.clone() } else { text.iter().flat_map(|&(offset, c)| c.to_lowercase().map(move |c| (offset, c))).collect() };
                let mut i = 0;
                while i + needle.len() <= lower.len() {
                    let end = i + needle.len();
                    let begins_character = i == 0 || lower[i - 1].0 != lower[i].0;
                    let ends_character = end == lower.len() || lower[end - 1].0 != lower[end].0;
                    // Token boundaries: ASCII/Unicode punctuation, brackets, quotes, control, and spaces separate words.
                    // Alphanumeric characters, underscores, and natural script letters remain word characters.
                    let word_separator = |c: char| {
                        c.is_whitespace()
                            || (c.is_ascii_punctuation() && c != '_')
                            || c.is_control()
                            || matches!(c,
                                '\u{2000}'..='\u{206f}' // General Punctuation (quotes, dashes, ellipses, etc.)
                                | '\u{3000}'..='\u{303f}' // CJK Symbols and Punctuation (「」『』【】、。・など)
                                | '\u{ff01}'..='\u{ff0f}' // Fullwidth symbols ！＂＃＄％＆＇（）＊＋，－．／
                                | '\u{ff1a}'..='\u{ff20}' // Fullwidth symbols ：；＜＝＞？＠
                                | '\u{ff3b}'..='\u{ff40}' // Fullwidth symbols ［＼］＾＿｀
                                | '\u{ff5b}'..='\u{ff65}' // Fullwidth symbols ｛｜｝～｟｠｡｢｣､･
                            )
                    };
                    let word_char = |c: char| !word_separator(c);
                    let complete_word = !whole_words || {
                        let first = text.partition_point(|(o, _)| *o < lower[i].0);
                        let last = text.partition_point(|(o, _)| *o <= lower[end - 1].0);
                        first.checked_sub(1).and_then(|j| text.get(j)).is_none_or(|(_, c)| !word_char(*c)) &&
                            text.get(last).is_none_or(|(_, c)| !word_char(*c))
                    };
                    if begins_character && ends_character && complete_word && lower[i..end].iter().map(|(_, c)| *c).eq(needle.iter().copied()) {
                        out.push((FlowPos::new(id, lower[i].0), FlowPos::new(id, lower[end - 1].0 + 1)));
                        i = end;
                    } else {
                        i += 1;
                    }
                }
            }
        }
    }

    // 文字列検索 + 書式条件が指定されている場合、書式条件に合致するもののみに絞り込む
    if !query.is_empty() && !fmt.is_empty() {
        out.retain(|&(from, to)| {
            let Some(p) = flow_command::paragraph(doc, from.para) else { return false };
            if !fmt.matches_para(doc, p) { return false; }
            if fmt.has_character_formatting() {
                for off in from.offset..to.offset {
                    let (_, _, style) = super::editing::visible_format_at(p, off + 1);
                    if !fmt.matches_run_style(&style) {
                        return false;
                    }
                }
            }
            true
        });
    }

    if scope == SearchScope::Selection {
        if let Some((a, b)) = selection_range {
            let paras_order: Vec<u32> = paragraphs(doc, SearchScope::All).into_iter().map(|(id, _)| id).collect();
            let (sel_start, sel_end) = if pos_cmp(&paras_order, a, b) == std::cmp::Ordering::Greater { (b, a) } else { (a, b) };
            out.retain(|&(from, to)| {
                pos_cmp(&paras_order, from, sel_start) != std::cmp::Ordering::Less
                    && pos_cmp(&paras_order, to, sel_end) != std::cmp::Ordering::Greater
            });
        } else {
            out.clear();
        }
    }

    out
}

/// ワイルドカード置換において、該当マッチに対する置換文字列（`\1`, `\2`, `^&` 等）を展開する。
pub(crate) fn expand_replacement_for_match(
    doc: &FlowDocument,
    a: FlowPos,
    b: FlowPos,
    query: &str,
    replacement_template: &str,
    case_sensitive: bool,
) -> String {
    let Some(p) = flow_command::paragraph(doc, a.para) else {
        return special_characters(replacement_template);
    };
    let chars_with_pos = super::editing::visible_chars(p);
    let text_chars: Vec<char> = chars_with_pos.iter().map(|(_, c)| *c).collect();
    let Ok(pattern) = super::wildcard::parse_wildcard(query) else {
        return special_characters(replacement_template);
    };
    let matches = super::wildcard::find_all_matches(&text_chars, &pattern, case_sensitive);
    for m in matches {
        let m_from = chars_with_pos.get(m.start).map_or(0, |x| x.0);
        let m_to = if m.end < chars_with_pos.len() {
            chars_with_pos[m.end].0
        } else {
            chars_with_pos.last().map_or(0, |x| x.0 + 1)
        };
        if m_from == a.offset && m_to == b.offset {
            let specials = special_characters(replacement_template);
            return super::wildcard::expand_wildcard_replacement(&specials, &text_chars, &m.captures);
        }
    }
    special_characters(replacement_template)
}

/// マッチ範囲の一致テキストを取得する。
fn matched_text(doc: &FlowDocument, a: FlowPos, b: FlowPos) -> String {
    if a.para == b.para {
        if let Some(p) = flow_command::paragraph(doc, a.para) {
            let chars = super::editing::visible_chars(p);
            return chars
                .iter()
                .filter(|(off, _)| *off >= a.offset && *off < b.offset)
                .map(|(_, c)| *c)
                .collect();
        }
    } else if let Some(p) = flow_command::paragraph(doc, a.para) {
        let chars = super::editing::visible_chars(p);
        let mut out: String = chars.into_iter().filter(|(off, _)| *off >= a.offset).map(|(_, c)| c).collect();
        out.push('\n');
        return out;
    }
    String::new()
}

impl SlideEditor {
    /// Search for `query`: the matches, the first one after the caret shown.
    pub(crate) fn find_text(&mut self, query: &str, cx: &mut Context<Self>) -> usize {
        let Some(doc) = self.flow().cloned() else { return 0 };
        let (case_sensitive, whole_words, wildcards, direction, scope, mut selection_range, find_format) = self
            .doc_state()
            .and_then(|s| s.find.as_ref())
            .map(|s| (s.case_sensitive, s.whole_words, s.wildcards, s.direction, s.scope, s.selection_range, s.find_format.clone()))
            .unwrap_or((false, false, false, SearchDirection::All, SearchScope::All, None, FindFormatting::default()));
        if scope == SearchScope::Selection && selection_range.is_none() {
            if let Some(sel) = self.ordered_selection().filter(|(a, b)| a != b) {
                selection_range = Some(sel);
                if let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) {
                    state.selection_range = Some(sel);
                }
            }
        }
        let matches = find_all_with_options(&doc, query, case_sensitive, whole_words, wildcards, scope, selection_range, Some(&find_format));
        if doc.track_changes && matches.iter().any(|(a, b)| a.para != b.para) {
            self.status = Some(crate::shell::i18n::text("docx.find.replacing-across-multiple").into());
            cx.notify();
            return 0;
        }
        let count = matches.len();
        // Typing on in the field keeps the search where the last match began
        // (not after it, which would skip a match with every letter).
        let shown = self.doc_state().and_then(|s| s.find.as_ref()).and_then(|f| f.current.and_then(|i| f.matches.get(i).copied()));
        if let Some((a, b)) = shown
            && (self.text_selection() == Some((a, b)) || self.text_selection() == Some((b, a)))
        {
            self.set_caret(a, false, cx);
        }
        if let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) {
            state.matches = matches;
            state.current = None;
            state.last_query = query.to_string();
        }
        if count > 0 {
            let forward = direction != SearchDirection::Up;
            self.find_next(forward, cx);
        }
        count
    }

    pub(crate) fn find_direction(&self) -> SearchDirection {
        self.doc_state().and_then(|s| s.find.as_ref()).map_or(SearchDirection::All, |f| f.direction)
    }

    pub(crate) fn find_scope(&self) -> SearchScope {
        self.doc_state().and_then(|s| s.find.as_ref()).map_or(SearchScope::All, |f| f.scope)
    }

    pub(crate) fn set_find_direction(&mut self, direction: SearchDirection, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.direction = direction;
        cx.notify();
    }

    pub(crate) fn set_find_scope(&mut self, scope: SearchScope, cx: &mut Context<Self>) {
        let selection_range = if scope == SearchScope::Selection {
            self.ordered_selection().filter(|(a, b)| a != b)
        } else {
            None
        };
        if let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) {
            state.scope = scope;
            state.selection_range = selection_range;
        }
        let query = self.doc_state().and_then(|s| s.find.as_ref()).map(|f| f.query.read(cx).value().to_string()).unwrap_or_default();
        self.find_text(&query, cx);
        cx.notify();
    }

    pub(crate) fn set_find_case_sensitive(&mut self, checked: bool, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.case_sensitive = checked;
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    pub(crate) fn set_find_whole_words(&mut self, checked: bool, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.whole_words = checked;
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    pub(crate) fn set_find_wildcards(&mut self, checked: bool, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.wildcards = checked;
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    pub(crate) fn find_next_in_direction(&mut self, cx: &mut Context<Self>) {
        let forward = self.find_direction() != SearchDirection::Up;
        self.find_next(forward, cx);
    }

    /// Show the next (or previous) match, going round the document or honoring search direction.
    pub(crate) fn find_next(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.find.as_ref()) else { return };
        let n = state.matches.len();
        if n == 0 {
            return;
        }
        let is_current_match = match state.current {
            Some(curr) => state.matches.get(curr).is_some_and(|&(a, b)| {
                self.text_selection() == Some((a, b)) || self.text_selection() == Some((b, a))
            }),
            None => false,
        };
        let current_index = if is_current_match { state.current } else { None };
        let i = match current_index {
            Some(i) if forward => (i + 1) % n,
            Some(i) => (i + n - 1) % n,
            None => {
                let caret = self.doc_state().and_then(|s| s.caret);
                if let Some(caret) = caret && let Some(doc) = self.flow() {
                    let paras_order: Vec<u32> = paragraphs(doc, SearchScope::All).into_iter().map(|(id, _)| id).collect();
                    if forward {
                        state
                            .matches
                            .iter()
                            .position(|&(from, _)| pos_cmp(&paras_order, from, caret) != std::cmp::Ordering::Less)
                            .unwrap_or(0)
                    } else {
                        state
                            .matches
                            .iter()
                            .rposition(|&(_, to)| pos_cmp(&paras_order, to, caret) != std::cmp::Ordering::Greater)
                            .unwrap_or(n - 1)
                    }
                } else if forward {
                    0
                } else {
                    n - 1
                }
            }
        };
        let (a, b) = state.matches[i];
        if let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) {
            state.current = Some(i);
        }
        self.set_caret(a, false, cx);
        self.set_caret(b, true, cx);
        self.scroll_to_caret(cx);
    }

    /// The commands replacing the match `a`..`b` with `text`, in its first
    /// character's formatting (or applying `replace_format` if specified).
    pub(super) fn replacement_commands(
        doc: &FlowDocument,
        a: FlowPos,
        b: FlowPos,
        text: &str,
        replace_format: Option<&ReplaceFormatting>,
    ) -> (Vec<FlowCommand>, FlowPos) {
        let paragraph = flow_command::paragraph(doc, a.para);
        let mut format = paragraph.map(|p| {
            let (format, char_style, _) = super::editing::visible_format_at(p, a.offset + 1);
            InsertFormat { format, char_style }
        });
        if let Some(fmt) = replace_format {
            if let Some(ref mut f) = format {
                if let Some(bold) = fmt.bold { f.format.bold = Some(bold); }
                if let Some(italic) = fmt.italic { f.format.italic = Some(italic); }
                if let Some(underline) = fmt.underline { f.format.underline = Some(underline); }
                if let Some(strike) = fmt.strike { f.format.strike = Some(strike); }
                if let Some(color) = fmt.color { f.format.color = Some(color); }
                if let Some(highlight) = fmt.highlight { f.format.highlight = Some(highlight); }
            }
        }

        let mut commands = Vec::new();
        let mut insert = a;

        let has_fmt = replace_format.is_some_and(|f| !f.is_empty());

        if text.is_empty() && has_fmt {
            // 書式のみ置換 (テキスト削除は行わず、指定書式を既存の一致範囲に適用)
            let fmt = replace_format.unwrap();
            if a.para == b.para && a.offset < b.offset {
                if let Some(bold) = fmt.bold {
                    commands.push(FlowCommand::SetRunFormat { from: a, to: b, change: RunChange::Bold(bold) });
                }
                if let Some(italic) = fmt.italic {
                    commands.push(FlowCommand::SetRunFormat { from: a, to: b, change: RunChange::Italic(italic) });
                }
                if let Some(underline) = fmt.underline {
                    commands.push(FlowCommand::SetRunFormat { from: a, to: b, change: RunChange::Underline(underline) });
                }
                if let Some(strike) = fmt.strike {
                    commands.push(FlowCommand::SetRunFormat { from: a, to: b, change: RunChange::Strike(strike) });
                }
                if let Some(color) = fmt.color {
                    commands.push(FlowCommand::SetRunFormat { from: a, to: b, change: RunChange::Color(color) });
                }
                if let Some(highlight) = fmt.highlight {
                    commands.push(FlowCommand::SetRunFormat { from: a, to: b, change: RunChange::Highlight(highlight) });
                }
            }
            if let Some(align) = fmt.align {
                commands.push(FlowCommand::SetParagraphFormat { first: a.para, last: a.para, change: ParaChange::Align(align) });
            }
            if let Some(ref style) = fmt.style {
                let resolved = resolve_style_id(doc, style);
                commands.push(FlowCommand::SetParagraphFormat { first: a.para, last: a.para, change: ParaChange::Style(Some(resolved)) });
            }
            return (commands, a);
        }

        if a.para != b.para {
            // Through the paragraph's mark (`^p`): the paragraphs joined.
            commands.push(FlowCommand::Delete { from: a, to: b });
            if !text.is_empty() { commands.push(FlowCommand::InsertText { at: insert, text: text.to_string(), format }); }
        } else if doc.track_changes {
            insert = super::editing::after_tracked_delete(doc, a, b);
            let who = super::comments::comment_author();
            commands.push(FlowCommand::TrackDelete { from: a, to: b, author: who.author.clone(), date: who.date.clone() });
            if !text.is_empty() {
                commands.push(FlowCommand::InsertText { at: insert, text: text.to_string(), format });
                commands.push(FlowCommand::MarkRevision { from: insert, to: FlowPos::new(insert.para, insert.offset + text.chars().count()),
                    revision: Some(ibis_doc_model::Revision { kind: ibis_doc_model::RevisionKind::Insert, author: who.author, date: who.date }) });
            }
        } else {
            if super::flow_layout::showing_markup() {
                commands.push(FlowCommand::Delete { from: a, to: b });
            } else if let Some(p) = paragraph {
                commands.extend(super::editing::visible_delete_commands(p, a.offset, b.offset));
            }
            if !text.is_empty() { commands.push(FlowCommand::InsertText { at: insert, text: text.to_string(), format }); }
        }

        // テキスト置換後の追加書式適用
        if let Some(fmt) = replace_format {
            let count = text.chars().count();
            if count > 0 {
                let end = FlowPos::new(insert.para, insert.offset + count);
                if let Some(bold) = fmt.bold {
                    commands.push(FlowCommand::SetRunFormat { from: insert, to: end, change: RunChange::Bold(bold) });
                }
                if let Some(italic) = fmt.italic {
                    commands.push(FlowCommand::SetRunFormat { from: insert, to: end, change: RunChange::Italic(italic) });
                }
                if let Some(underline) = fmt.underline {
                    commands.push(FlowCommand::SetRunFormat { from: insert, to: end, change: RunChange::Underline(underline) });
                }
                if let Some(strike) = fmt.strike {
                    commands.push(FlowCommand::SetRunFormat { from: insert, to: end, change: RunChange::Strike(strike) });
                }
                if let Some(color) = fmt.color {
                    commands.push(FlowCommand::SetRunFormat { from: insert, to: end, change: RunChange::Color(color) });
                }
                if let Some(highlight) = fmt.highlight {
                    commands.push(FlowCommand::SetRunFormat { from: insert, to: end, change: RunChange::Highlight(highlight) });
                }
            }
            if let Some(align) = fmt.align {
                commands.push(FlowCommand::SetParagraphFormat { first: insert.para, last: insert.para, change: ParaChange::Align(align) });
            }
            if let Some(ref style) = fmt.style {
                let resolved = resolve_style_id(doc, style);
                commands.push(FlowCommand::SetParagraphFormat { first: insert.para, last: insert.para, change: ParaChange::Style(Some(resolved)) });
            }
        }

        (commands, insert)
    }

    /// Replace the match shown with `text`, then show the next.
    pub(crate) fn replace_current(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state().and_then(|s| s.find.as_ref()) else { return };
        let Some((a, b)) = state.current.and_then(|i| state.matches.get(i).copied()) else { return };
        let query = {
            let input_val = state.query.read(cx).value();
            if input_val.is_empty() {
                state.last_query.clone()
            } else {
                input_val.to_string()
            }
        };
        let wildcards = state.wildcards;
        let case_sensitive = state.case_sensitive;
        let whole_words = state.whole_words;
        let scope = state.scope;
        let selection_range = state.selection_range;
        let find_format = state.find_format.clone();
        let replace_format = state.replace_format.clone();
        let Some(doc) = self.flow().cloned() else { return };
        if !find_all_with_options(&doc, &query, case_sensitive, whole_words, wildcards, scope, selection_range, Some(&find_format)).contains(&(a, b)) {
            self.find_text(&query, cx);
            self.status = Some(crate::shell::i18n::text("docx.find.search-results-have-changed-check").into());
            cx.notify();
            return;
        }
        let effective_text = if wildcards {
            expand_replacement_for_match(&doc, a, b, &query, text, case_sensitive)
        } else {
            let raw = special_characters(text);
            if raw.contains("^&") {
                let match_str = matched_text(&doc, a, b);
                raw.replace("^&", &match_str)
            } else {
                raw
            }
        };
        if doc.track_changes && (effective_text.contains(['\n', '\r']) || a.para != b.para) {
            self.status = Some(crate::shell::i18n::text("docx.find.replacing-across-multiple").into());
            cx.notify();
            return;
        }
        let (commands, at) = Self::replacement_commands(&doc, a, b, &effective_text, Some(&replace_format));
        let match_char_count = if a.para == b.para { b.offset.saturating_sub(a.offset) } else { 0 };
        let rep_char_count = if effective_text.is_empty() && !replace_format.is_empty() {
            match_char_count
        } else {
            effective_text.chars().count()
        };
        if self.run_flow(commands, cx) {
            if scope == SearchScope::Selection {
                if let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) {
                    if let Some((_, ref mut sel_end)) = state.selection_range {
                        if sel_end.para == at.para && rep_char_count != match_char_count {
                            sel_end.offset = (sel_end.offset as isize + rep_char_count as isize - match_char_count as isize).max(0) as usize;
                        }
                    }
                }
            }
            self.set_caret(FlowPos::new(at.para, at.offset + rep_char_count), false, cx);
            self.find_text(&query, cx);
        }
    }

    /// Replace every match of `query` with `text`, as one step of undo; the
    /// number replaced.
    pub(crate) fn replace_all(&mut self, query: &str, text: &str, cx: &mut Context<Self>) -> usize {
        let Some(doc) = self.flow().cloned() else { return 0 };
        let (wildcards, case_sensitive, whole_words, scope, selection_range, find_format, replace_format) = self
            .doc_state()
            .and_then(|s| s.find.as_ref())
            .map(|s| (s.wildcards, s.case_sensitive, s.whole_words, s.scope, s.selection_range, s.find_format.clone(), s.replace_format.clone()))
            .unwrap_or((false, false, false, SearchScope::All, None, FindFormatting::default(), ReplaceFormatting::default()));
        let matches = find_all_with_options(&doc, query, case_sensitive, whole_words, wildcards, scope, selection_range, Some(&find_format));
        if matches.is_empty() { return 0; }

        let mut replacements: Vec<(FlowPos, FlowPos, String)> = Vec::with_capacity(matches.len());
        for &(a, b) in &matches {
            let rep = if wildcards {
                expand_replacement_for_match(&doc, a, b, query, text, case_sensitive)
            } else {
                let raw = special_characters(text);
                if raw.contains("^&") {
                    let match_str = matched_text(&doc, a, b);
                    raw.replace("^&", &match_str)
                } else {
                    raw
                }
            };
            if doc.track_changes && rep.contains(['\n', '\r']) {
                self.status = Some(crate::shell::i18n::text("docx.find.replacing-across-multiple").into());
                cx.notify();
                return 0;
            }
            replacements.push((a, b, rep));
        }

        // From the last, so the earlier positions stay where they are.
        let mut commands = Vec::new();
        for (a, b, rep) in replacements.into_iter().rev() {
            commands.extend(Self::replacement_commands(&doc, a, b, &rep, Some(&replace_format)).0);
        }
        let count = matches.len();
        if count > 0 && !self.run_flow(commands, cx) { return 0; }
        if let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) {
            state.matches.clear();
            state.current = None;
        }
        count
    }

    pub(crate) fn find_format(&self) -> Option<&FindFormatting> {
        self.doc_state().and_then(|s| s.find.as_ref()).map(|f| &f.find_format)
    }

    #[allow(dead_code)]
    pub(crate) fn replace_format(&self) -> Option<&ReplaceFormatting> {
        self.doc_state().and_then(|s| s.find.as_ref()).map(|f| &f.replace_format)
    }

    pub(crate) fn set_find_format(&mut self, fmt: FindFormatting, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.find_format = fmt;
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    pub(crate) fn set_replace_format(&mut self, fmt: ReplaceFormatting, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.replace_format = fmt;
        cx.notify();
    }

    pub(crate) fn clear_find_format(&mut self, cx: &mut Context<Self>) {
        self.set_find_format(FindFormatting::default(), cx);
    }

    pub(crate) fn clear_replace_format(&mut self, cx: &mut Context<Self>) {
        self.set_replace_format(ReplaceFormatting::default(), cx);
    }

    pub(crate) fn toggle_find_format_bold(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.find_format.bold = match state.find_format.bold {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        };
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    pub(crate) fn toggle_find_format_italic(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.find_format.italic = match state.find_format.italic {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        };
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    pub(crate) fn toggle_find_format_underline(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.find_format.underline = match state.find_format.underline {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        };
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    pub(crate) fn toggle_find_format_strike(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.find_format.strike = match state.find_format.strike {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        };
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn set_find_format_style(&mut self, style: Option<String>, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.find_format.style = style;
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn set_find_format_color(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.find_format.color = color;
        let query = state.query.read(cx).value().to_string();
        self.find_text(&query, cx);
        cx.notify();
    }

    pub(crate) fn toggle_replace_format_bold(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.replace_format.bold = match state.replace_format.bold {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        };
        cx.notify();
    }

    pub(crate) fn toggle_replace_format_italic(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.replace_format.italic = match state.replace_format.italic {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        };
        cx.notify();
    }

    pub(crate) fn toggle_replace_format_underline(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.replace_format.underline = match state.replace_format.underline {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        };
        cx.notify();
    }

    pub(crate) fn toggle_replace_format_strike(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.replace_format.strike = match state.replace_format.strike {
            None => Some(true),
            Some(true) => Some(false),
            Some(false) => None,
        };
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn set_replace_format_style(&mut self, style: Option<String>, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.replace_format.style = style;
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn set_replace_format_color(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.replace_format.color = color;
        cx.notify();
    }

    pub(crate) fn toggle_find_format_section(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) else { return };
        state.format_section_open = !state.format_section_open;
        cx.notify();
    }

    /// 編集 > 検索と置換…: the panel in the sidebar, its field focused.
    pub(crate) fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if self.doc_state().is_some_and(|s| s.find.is_none()) {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_shell.find-what")));
            let replacement = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_shell.replace-with")));
            let subscription = cx.subscribe_in(&query, window, |this, input, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    let q = input.read(cx).value().to_string();
                    this.find_text(&q, cx);
                }
                InputEvent::PressEnter { .. } => {
                    let forward = this.doc_state().and_then(|s| s.find.as_ref()).map_or(true, |f| f.direction != SearchDirection::Up);
                    this.find_next(forward, cx);
                }
                _ => {}
            });
            let initial_selection = self.ordered_selection().filter(|(a, b)| a != b);
            if let Some(state) = self.doc_state_mut() {
                state.link = None;
                state.find = Some(FindState {
                    query,
                    replacement,
                    matches: Vec::new(),
                    current: None,
                    case_sensitive: false,
                    whole_words: false,
                    wildcards: false,
                    direction: SearchDirection::All,
                    scope: SearchScope::All,
                    selection_range: initial_selection,
                    last_query: String::new(),
                    find_format: FindFormatting::default(),
                    replace_format: ReplaceFormatting::default(),
                    format_section_open: false,
                    replace_mode: false,
                    _subscription: subscription,
                });
            }
        } else if let Some(sel) = self.ordered_selection().filter(|(a, b)| a != b) {
            if let Some(state) = self.doc_state_mut().and_then(|s| s.find.as_mut()) {
                state.selection_range = Some(sel);
            }
        }
        if let Some(state) = self.doc_state_mut() {
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.comments = None;
            state.helper.go_to = None;
        }
        self.sidebar_open = true;
        if let Some(query) = self.doc_state().and_then(|s| s.find.as_ref()).map(|f| f.query.clone()) {
            query.update(cx, |q, cx| q.focus(window, cx));
        }
        cx.notify();
    }

    /// The panel's body.
    pub(crate) fn find_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.doc_state()?.find.as_ref()?;
        self.render_find_panel(cx)
    }

    #[inline(never)]
    fn render_find_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.find.as_ref()?;
        let (query, replacement) = (state.query.clone(), state.replacement.clone());
        let case_sensitive = state.case_sensitive;
        let whole_words = state.whole_words;
        let wildcards = state.wildcards;
        let direction = state.direction;
        let scope = state.scope;
        let find_fmt = state.find_format.clone();
        let rep_fmt = state.replace_format.clone();
        let fmt_open = state.format_section_open;
        let replace_mode = state.replace_mode;
        let has_selection = state.selection_range.is_some_and(|(a, b)| a != b);
        let status: SharedString = match (state.matches.len(), state.current) {
            (0, _) if query.read(cx).value().is_empty() && find_fmt.is_empty() => "".into(),
            (0, _) if scope == SearchScope::Selection && !has_selection => crate::shell::i18n::text("docx.find.no-selection"),
            (0, _) => crate::shell::i18n::text("docx.find.not-found"),
            (n, Some(i)) => crate::shell::i18n::format("docx.find.i-n", &[("i", &(i + 1)), ("n", &n)]).into(),
            (n, None) => crate::shell::i18n::format("docx.find.n-results", &[("n", &n)]).into(),
        };
        let next_forward = direction != SearchDirection::Up;
        let dir_button = |dir:SearchDirection,label_key:&'static str| {
            crate::shell::chrome::segment_control::SegmentItem::label(SharedString::from(format!("find-dir-{dir:?}")),crate::shell::i18n::text(label_key),direction==dir)
                .on_click(cx.listener(move |e,_,_,cx|e.set_find_direction(dir,cx)))
        };
        let scope_button = |scope_value:SearchScope,label_key:&'static str| {
            crate::shell::chrome::radio_button(SharedString::from(format!("find-scope-{scope_value:?}")),scope==scope_value,false,crate::shell::i18n::text(label_key),cx.listener(move |e,_,_,cx|e.set_find_scope(scope_value,cx)))
        };
        let mut query_section = v_flex().gap_1().child(crate::shell::chrome::text_field(&query).full_width());
        if !find_fmt.is_empty() {
            let desc: SharedString = find_fmt.description().into();
            query_section = query_section.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .text_sm()
                    .child(
                        div()
                            .text_color(rgb(0x5f6368))
                            .child(crate::shell::i18n::format("{label}: {desc}", &[
                                ("label", &crate::shell::i18n::text("docx.find.format.find-active")),
                                ("desc", &desc),
                            ])),
                    )
                    .child(
                        crate::shell::chrome::destructive_button("clear-find-format",cx)
                            .small()
                            .label(crate::shell::i18n::text("docx.find.format.clear"))
                            .on_click(cx.listener(|this, _, _, cx| this.clear_find_format(cx))),
                    ),
            );
        }

        let mut rep_section = v_flex().gap_1().child(crate::shell::chrome::text_field(&replacement).full_width());
        if !rep_fmt.is_empty() {
            let desc: SharedString = rep_fmt.description().into();
            rep_section = rep_section.child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .text_sm()
                    .child(
                        div()
                            .text_color(rgb(0x5f6368))
                            .child(crate::shell::i18n::format("{label}: {desc}", &[
                                ("label", &crate::shell::i18n::text("docx.find.format.replace-active")),
                                ("desc", &desc),
                            ])),
                    )
                    .child(
                        crate::shell::chrome::destructive_button("clear-replace-format",cx)
                            .small()
                            .label(crate::shell::i18n::text("docx.find.format.clear"))
                            .on_click(cx.listener(|this, _, _, cx| this.clear_replace_format(cx))),
                    ),
            );
        }

        let common_options=v_flex().gap_1()
            .child(crate::shell::chrome::checkbox_button("find-case-sensitive",crate::shell::i18n::text("_shell.match-case"),case_sensitive,false,cx.listener(move |e,_,_,cx|e.set_find_case_sensitive(!case_sensitive,cx))))
            .child(crate::shell::chrome::checkbox_button("find-whole-words",crate::shell::i18n::text("docx.find.find-whole-words-only"),whole_words,false,cx.listener(move |e,_,_,cx|e.set_find_whole_words(!whole_words,cx))))
            .child(crate::shell::chrome::checkbox_button("find-wildcards",crate::shell::i18n::text("docx.find.use-wildcards"),wildcards,false,cx.listener(move |e,_,_,cx|e.set_find_wildcards(!wildcards,cx))));
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                .child(crate::shell::chrome::radio_button("find-mode-search", !replace_mode, false, crate::shell::i18n::text("_shell.find"), cx.listener(|this, _, _, cx| {
                    if let Some(f) = this.doc_state_mut().and_then(|s| s.find.as_mut()) { f.replace_mode = false; } cx.notify();
                })))
                .child(crate::shell::chrome::radio_button("find-mode-replace", replace_mode, false, crate::shell::i18n::text("_shell.replace"), cx.listener(|this, _, _, cx| {
                    if let Some(f) = this.doc_state_mut().and_then(|s| s.find.as_mut()) { f.replace_mode = true; } cx.notify();
                }))))
            .child(query_section)
            .when(replace_mode, |d| d.child(rep_section)).child(common_options)
            .child(h_flex().gap_1().flex_wrap()
                .child(crate::shell::chrome::action_button("find-previous").small().label(crate::shell::i18n::text("_shell.previous")).on_click(cx.listener(|this, _, _, cx| this.find_next(false, cx))))
                .child(crate::shell::chrome::action_button("find-next").label(crate::shell::i18n::text("_shell.next")).tooltip(crate::shell::i18n::text(if replace_mode{"docx.find.replace-next"}else{"_shell.next"})).on_click(cx.listener(move |e,_,_,cx|{
                    if replace_mode {let value=e.doc_state().and_then(|s|s.find.as_ref()).map(|f|f.replacement.read(cx).value().to_string()).unwrap_or_default();e.replace_current(&value,cx);}
                    e.find_next(next_forward,cx);
                })))
                .when(replace_mode,|d|d.child(crate::shell::chrome::action_button("replace-all").label(crate::shell::i18n::text("docx.find.dir.all")).on_click(cx.listener(|e,_,_,cx|{let Some(f)=e.doc_state().and_then(|s|s.find.as_ref())else{return;};let(q,t)=(f.query.read(cx).value().to_string(),f.replacement.read(cx).value().to_string());e.replace_all(&q,&t,cx);}))))
            )
            .child(div().text_sm().child(status))
            ;

        if fmt_open {
            let fmt_btn = |id: &'static str, label_key: &'static str, state_opt: Option<bool>, on_click: Box<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>) + Send + Sync>| {
                let label_text = match state_opt {
                    Some(true) => crate::shell::i18n::text(label_key),
                    Some(false) => SharedString::from(crate::shell::i18n::format("_docx.label-clear", &[("label", &crate::shell::i18n::text(label_key).to_string())])),
                    None => crate::shell::i18n::text(label_key),
                };
                let btn = crate::shell::chrome::action_button(id).small().label(label_text).on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)));
                if state_opt == Some(true) {
                    btn.primary()
                } else {
                    btn.ghost()
                }
            };

            let find_fmt_controls = v_flex()
                .gap_1()
                .child(div().text_sm().child(crate::shell::i18n::text("docx.find.format.find-label")))
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(fmt_btn("find-fmt-bold", "docx.find.format.bold", find_fmt.bold, Box::new(|this, cx| this.toggle_find_format_bold(cx))))
                        .child(fmt_btn("find-fmt-italic", "docx.find.format.italic", find_fmt.italic, Box::new(|this, cx| this.toggle_find_format_italic(cx))))
                        .child(fmt_btn("find-fmt-underline", "docx.find.format.underline", find_fmt.underline, Box::new(|this, cx| this.toggle_find_format_underline(cx))))
                        .child(fmt_btn("find-fmt-strike", "docx.find.format.strike", find_fmt.strike, Box::new(|this, cx| this.toggle_find_format_strike(cx))))
                        .child(
                            crate::shell::chrome::destructive_button("find-fmt-clear",cx)
                                .small()
                                .label(crate::shell::i18n::text("docx.find.format.clear"))
                                .on_click(cx.listener(|this, _, _, cx| this.clear_find_format(cx))),
                        ),
                );

            let rep_fmt_controls = v_flex()
                .gap_1()
                .child(div().text_sm().child(crate::shell::i18n::text("docx.find.format.replace-label")))
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(fmt_btn("rep-fmt-bold", "docx.find.format.bold", rep_fmt.bold, Box::new(|this, cx| this.toggle_replace_format_bold(cx))))
                        .child(fmt_btn("rep-fmt-italic", "docx.find.format.italic", rep_fmt.italic, Box::new(|this, cx| this.toggle_replace_format_italic(cx))))
                        .child(fmt_btn("rep-fmt-underline", "docx.find.format.underline", rep_fmt.underline, Box::new(|this, cx| this.toggle_replace_format_underline(cx))))
                        .child(fmt_btn("rep-fmt-strike", "docx.find.format.strike", rep_fmt.strike, Box::new(|this, cx| this.toggle_replace_format_strike(cx))))
                        .child(
                            crate::shell::chrome::destructive_button("rep-fmt-clear",cx)
                                .small()
                                .label(crate::shell::i18n::text("docx.find.format.clear"))
                                .on_click(cx.listener(|this, _, _, cx| this.clear_replace_format(cx))),
                        ),
                );

            body = body
                .child(find_fmt_controls)
                .when(replace_mode, |d| d.child(rep_fmt_controls));
        }

        body = body
            .child(
                crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                    .label(crate::shell::i18n::text("docx.find.direction"))
                    .child(dir_button(SearchDirection::All, "docx.find.dir.all"))
                    .child(dir_button(SearchDirection::Down, "docx.find.dir.down"))
                    .child(dir_button(SearchDirection::Up, "docx.find.dir.up")),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(crate::shell::i18n::text("docx.find.scope")))
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .child(scope_button(SearchScope::All, "docx.find.scope.all"))
                            .child(scope_button(SearchScope::Main, "docx.find.scope.main"))
                            .child(scope_button(SearchScope::HeaderFooter, "docx.find.scope.headers-footers"))
                            .child(scope_button(SearchScope::Notes, "docx.find.scope.notes"))
                            .child(scope_button(SearchScope::Comments, "docx.find.scope.comments"))
                            .child(scope_button(SearchScope::Selection, "docx.find.scope.selection")),
                    ),
            )
;
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.find-and-replace"), body: body.into_any_element() })
    }
}
