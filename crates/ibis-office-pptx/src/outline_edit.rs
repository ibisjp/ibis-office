// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! アウトライン表示の編集 (表示 > アウトライン表示): the outline pane lists the slides' titles and body
//! paragraphs as lines, and a line can be typed in. Typing and deleting go to the title or the body paragraph of the
//! slide; Enter on a title makes a new slide, on a body line a new paragraph of the same level; Tab and Shift+Tab
//! (and ⌥⇧→ ⌥⇧←) demote and promote a line (a title made a body line of the slide before, a first-level body line
//! made a slide of its own); ⌥⇧↑ ⌥⇧↓ move a line (a title moves its slide). Every change is one edit that can be
//! undone.
use std::ops::Range;

use gpui_kit::*;
use ibis_doc_model::{Command, ParagraphFormat, Shape, ShapeId, Slide, SlideId, TextBody};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// What a line of the outline is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineKind {
    /// A slide's title.
    Title,
    /// Paragraph `n` of a slide's body.
    Body(usize),
}

/// A line of the outline.
#[derive(Clone, Debug)]
pub(crate) struct OutlineLine {
    pub(crate) slide: SlideId,
    pub(crate) kind: LineKind,
    pub(crate) level: u8,
    pub(crate) text: String,
    /// The shape its text is in, if the slide has one.
    pub(crate) shape: Option<ShapeId>,
}

/// The line being edited: where the caret and the other end of the selection are, in characters of the line.
#[derive(Clone, Debug, Default)]
pub struct OutlineEdit {
    pub(crate) line: usize,
    pub(crate) caret: usize,
    /// The other end of the selection: its line and its character in it (the caret's own when nothing is selected).
    pub(crate) anchor_line: usize,
    pub(crate) anchor: usize,
    /// What the input method is composing at the selection's start.
    pub(crate) marked: Option<String>,
    /// How the input method underlines each clause of `marked` (UTF-16 units of it; empty when the platform does not say).
    pub marked_clauses: Vec<ImeClause>,
    /// The last edit was this line's typing (more of it joins that edit).
    pub(crate) typing: bool,
    /// The slide whose empty body (which has no line) shows its one empty line: the caret is in it.
    pub(crate) blank: Option<SlideId>,
}

impl OutlineEdit {
    /// The selected characters of the caret's line, when the selection is in that line alone.
    pub(crate) fn range(&self) -> Range<usize> {
        if self.anchor_line == self.line { self.caret.min(self.anchor)..self.caret.max(self.anchor) } else { self.caret..self.caret }
    }

    /// The selection from its start to its end as (line, character), when it is not empty.
    pub(crate) fn span(&self) -> Option<((usize, usize), (usize, usize))> {
        let (a, b) = ((self.anchor_line, self.anchor), (self.line, self.caret));
        (a != b).then(|| if a <= b { (a, b) } else { (b, a) })
    }
}

/// How far a level is indented, in EMU (as the Tab key of the slide's text).
const LEVEL_INDENT: i64 = 457_200;

/// The shapes of a slide that hold its title and its body: by placeholder, else the top text shape and the next.
fn slide_parts(slide: &Slide) -> (Option<ShapeId>, Option<ShapeId>) {
    let (mut title, mut body) = (None, None);
    let mut texts: Vec<&Shape> = slide.shapes.iter().filter(|s| s.text_body().is_some()).collect();
    texts.sort_by_key(|s| s.frame.y);
    for shape in &texts {
        if let Some(ph) = slide.placeholders.get(&shape.id) {
            match ph.placeholder.normalized() {
                "title" => {
                    title.get_or_insert(shape.id);
                }
                "body" | "subTitle" => {
                    body.get_or_insert(shape.id);
                }
                _ => {}
            }
        }
    }
    if title.is_none() && body.is_none() && slide.placeholders.is_empty() {
        title = texts.first().map(|s| s.id);
        body = texts.get(1).map(|s| s.id);
    }
    (title, body)
}

/// A slide's outline: its title (the first paragraph of its title) and the paragraphs of its body, each with its level.
pub fn outline_of(slide: &Slide) -> (String, Vec<(u8, String)>) {
    let (title, body) = slide_parts(slide);
    let title = title.and_then(|id| text_body_of(slide, id)).and_then(|b| b.paragraphs.first().map(|p| p.text())).unwrap_or_default();
    let body = body.and_then(|id| text_body_of(slide, id)).filter(|b| !is_blank(b)).map(|b| b.paragraphs.iter().map(|p| (p.level, p.text())).collect()).unwrap_or_default();
    (title, body)
}

fn text_body_of(slide: &Slide, shape: ShapeId) -> Option<&TextBody> {
    slide.shape(shape).and_then(|s| s.text_body())
}

/// The character at which paragraph `n` of `body` starts in its text.
fn paragraph_start(body: &TextBody, n: usize) -> usize {
    body.paragraphs.iter().take(n).map(|p| p.text().chars().count() + 1).sum()
}

/// The size (hundredths of a point) the text of paragraph `n` of `body` has at level `level`: that of another paragraph at
/// that level, else its own scaled as PowerPoint's default levels are (28, 24, 20, 18 pt).
fn level_size(body: &TextBody, n: usize, level: u8) -> Option<u32> {
    const DEFAULT: [u32; 9] = [28, 24, 20, 18, 18, 18, 18, 18, 18];
    let first_size = |p: &ibis_doc_model::Paragraph| p.runs.iter().find(|r| !r.text.is_empty()).and_then(|r| r.style.font_size);
    if let Some(size) = body.paragraphs.iter().enumerate().filter(|(i, p)| *i != n && p.level == level).find_map(|(_, p)| first_size(p)) {
        return Some(size);
    }
    let p = body.paragraphs.get(n)?;
    let current = first_size(p)?;
    Some((current as f32 * DEFAULT[usize::from(level.min(8))] as f32 / DEFAULT[usize::from(p.level.min(8))] as f32).round() as u32)
}

/// The commands that make paragraph `n` of a body level `level`: its level and indent, and the size of that level.
fn level_commands(slide: SlideId, shape: ShapeId, body: &TextBody, n: usize, level: u8) -> Vec<Command> {
    let old = body.paragraphs.get(n).map_or(0, |p| p.level);
    let at = paragraph_start(body, n);
    let len = body.paragraphs.get(n).map_or(0, |p| p.text().chars().count());
    let indent = LEVEL_INDENT * (i64::from(level) - i64::from(old));
    let mut commands = vec![Command::SetParagraphFormat { slide, shape, start: at, end: at, format: ParagraphFormat { level: Some(level), indent_by: Some(indent), ..Default::default() } }];
    if let (Some(size), true) = (level_size(body, n, level), len > 0) {
        commands.push(Command::SetCharacterFormat { slide, shape, start: at, end: at + len, format: ibis_doc_model::CharacterFormat { font_size: Some(size), ..Default::default() } });
    }
    commands
}

/// A body that holds one empty paragraph and nothing else (a new slide's): no line of the outline.
fn is_blank(body: &TextBody) -> bool {
    body.paragraphs.len() <= 1 && body.paragraphs.iter().all(|p| p.text().is_empty())
}

#[ibis_office_macros::editor_ext(OutlineEditExt)]
impl SlideEditor {
    /// The lines of the outline: for each slide its title, then its body's paragraphs.
    pub(crate) fn outline_lines(&self) -> Vec<OutlineLine> {
        let mut lines = Vec::new();
        let blank_slide = self.pptx().and_then(|s| s.outline_edit.as_ref()).and_then(|e| e.blank);
        for slide in &self.presentation().slides {
            let (title, body) = slide_parts(slide);
            let title_text = title.and_then(|id| text_body_of(slide, id)).and_then(|b| b.paragraphs.first().map(|p| p.text())).unwrap_or_default();
            lines.push(OutlineLine { slide: slide.id, kind: LineKind::Title, level: 0, text: title_text, shape: title });
            if let Some((id, body)) = body.and_then(|id| text_body_of(slide, id).map(|b| (id, b))) {
                if !is_blank(body) || blank_slide == Some(slide.id) {
                    for (n, p) in body.paragraphs.iter().enumerate() {
                        lines.push(OutlineLine { slide: slide.id, kind: LineKind::Body(n), level: p.level, text: p.text(), shape: Some(id) });
                    }
                }
            }
        }
        lines
    }

    /// The line being edited in the outline view, if one is.
    pub fn outline_edit(&self) -> Option<&OutlineEdit> {
        if !self.is_outline_view() {
            return None;
        }
        self.pptx().and_then(|s| s.outline_edit.as_ref())
    }

    /// Whether a line of the outline is being edited (its keys and its text are the outline's).
    pub fn is_outline_editing(&self) -> bool {
        self.outline_edit().is_some()
    }

    pub(crate) fn end_outline_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.outline_edit = None;
        }
        cx.notify();
    }

    /// The caret put in line `line` of the outline, before character `caret`.
    pub fn begin_outline_edit(&mut self, line: usize, caret: usize, window: &mut Window, cx: &mut Context<Self>) {
        let lines = self.outline_lines();
        let Some(l) = lines.get(line) else { return };
        let caret = caret.min(l.text.chars().count());
        let slide = l.slide;
        self.end_text_edit(window, cx);
        if let Some(s) = self.pptx_mut() {
            s.outline_edit = Some(OutlineEdit { line, caret, anchor_line: line, anchor: caret, marked: None, marked_clauses: Vec::new(), typing: false, blank: None });
        }
        if slide != self.current_slide {
            self.select_slide(slide, window, cx);
        }
        // The keys and the text typed go to the editor (not the slide list).
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The selection made to reach to line `line`, before character `caret` (a drag, or a click with Shift).
    pub(crate) fn extend_outline_selection(&mut self, line: usize, caret: usize, window: &mut Window, cx: &mut Context<Self>) {
        let lines = self.outline_lines();
        let Some(l) = lines.get(line) else { return };
        let caret = caret.min(l.text.chars().count());
        let slide = l.slide;
        let moved = self.pptx_mut().and_then(|s| s.outline_edit.as_mut()).is_some_and(|e| {
            let changed = (e.line, e.caret) != (line, caret);
            e.line = line;
            e.caret = caret;
            e.marked = None;
            e.typing = false;
            changed
        });
        if moved {
            if slide != self.current_slide {
                self.current_slide = slide;
                self.select_slide(slide, window, cx);
                self.focus_handle.focus(window, cx);
            }
            cx.notify();
        }
    }

    /// Put the edit at line `line`, caret `caret` (the text just changed; the slide shown follows).
    fn outline_place(&mut self, line: usize, caret: usize, window: Option<&mut Window>, cx: &mut Context<Self>) {
        let lines = self.outline_lines();
        let line = line.min(lines.len().saturating_sub(1));
        let caret = lines.get(line).map_or(0, |l| caret.min(l.text.chars().count()));
        let slide = lines.get(line).map(|l| l.slide);
        if let Some(s) = self.pptx_mut() {
            let blank = s.outline_edit.as_ref().and_then(|e| e.blank).filter(|b| Some(*b) == slide);
            s.outline_edit = Some(OutlineEdit { line, caret, anchor_line: line, anchor: caret, marked: None, marked_clauses: Vec::new(), typing: false, blank });
        }
        if let (Some(slide), Some(window)) = (slide, window)
            && slide != self.current_slide
        {
            self.select_slide(slide, window, cx);
            self.focus_handle.focus(window, cx);
        } else if let Some(slide) = slide {
            self.current_slide = slide;
        }
        cx.notify();
    }

    /// The edit made one undo step (`Group`), or joined to the typing before it.
    fn outline_run(&mut self, commands: Vec<Command>, typing: bool, cx: &mut Context<Self>) -> bool {
        if commands.is_empty() {
            return false;
        }
        if self.is_pptx_mark_as_final() || self.is_pptx_read_only_blocked() {
            return false;
        }
        let joined = typing && self.pptx().and_then(|s| s.outline_edit.as_ref()).is_some_and(|e| e.typing);
        let result = if joined { self.document.amend_last(commands) } else { self.document.execute(commands) };
        match result {
            Ok(()) => {
                cx.notify();
                true
            }
            Err(err) => {
                log::warn!("outline edit rejected: {err}");
                false
            }
        }
    }

    /// The selected characters replaced by `text` (a newline is a space: a line is one paragraph).
    pub(crate) fn outline_type(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(edit) = self.outline_edit().cloned() else { return };
        let lines = self.outline_lines();
        // A selection over several lines: those lines of one body, the text of them replaced as one.
        if let Some(((l1, o1), (l2, o2))) = edit.span().filter(|((a, _), (b, _))| a != b) {
            let (first, last) = (&lines[l1], &lines[l2]);
            let (LineKind::Body(n1), LineKind::Body(n2)) = (first.kind, last.kind) else { return };
            if first.slide != last.slide || lines[l1..=l2].iter().any(|l| l.slide != first.slide) {
                return;
            }
            let Some(shape) = first.shape else { return };
            let Some(body) = self.presentation().slide(first.slide).and_then(|s| text_body_of(s, shape)) else { return };
            let text = text.replace(['\n', '\r'], " ");
            let at = paragraph_start(body, n1) + o1;
            let end = paragraph_start(body, n2) + o2;
            let command = Command::EditText { slide: first.slide, shape, at, remove: end - at, insert: text.clone() };
            if self.outline_run(vec![command], false, cx) {
                self.outline_place(l1, o1 + text.chars().count(), None, cx);
            }
            return;
        }
        let Some(line) = lines.get(edit.line) else { return };
        let Some(shape) = line.shape else { return };
        let Some(body) = self.presentation().slide(line.slide).and_then(|s| text_body_of(s, shape)) else { return };
        let base = match line.kind {
            LineKind::Title => 0,
            LineKind::Body(n) => paragraph_start(body, n),
        };
        let text = text.replace(['\n', '\r'], " ");
        let range = edit.range();
        let command = Command::EditText { slide: line.slide, shape, at: base + range.start, remove: range.len(), insert: text.clone() };
        if self.outline_run(vec![command], true, cx) {
            let at = range.start + text.chars().count();
            if let Some(e) = self.pptx_mut().and_then(|s| s.outline_edit.as_mut()) {
                e.caret = at;
                e.anchor = at;
                e.anchor_line = e.line;
                e.marked = None;
                e.typing = true;
            }
        }
    }

    /// Backspace (`forward`: Delete): the selection, or a character; at the start of a body line it joins the line before.
    pub(crate) fn outline_delete(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.outline_edit().cloned() else { return };
        let lines = self.outline_lines();
        let Some(line) = lines.get(edit.line).cloned() else { return };
        if edit.span().is_some() {
            return self.outline_type("", cx);
        }
        let len = line.text.chars().count();
        let Some(shape) = line.shape else { return };
        let Some(body) = self.presentation().slide(line.slide).and_then(|s| text_body_of(s, shape)).cloned() else { return };
        let base = match line.kind {
            LineKind::Title => 0,
            LineKind::Body(n) => paragraph_start(&body, n),
        };
        let mut remove = 1;
        let (at, caret_line, caret) = match (forward, line.kind) {
            (false, _) if edit.caret > 0 => {
                // One character; a kanji with its variation selectors (an IVS, #158) goes whole.
                let from = crate::render::rich_text::prev_selector_boundary(&line.text.chars().collect::<Vec<char>>(), edit.caret);
                remove = edit.caret - from;
                (base + from, edit.line, from)
            }
            (false, LineKind::Body(n)) if n > 0 => {
                // Joined to the paragraph before it.
                let before = body.paragraphs[n - 1].text().chars().count();
                (base - 1, edit.line - 1, before)
            }
            (true, _) if edit.caret < len => {
                remove = crate::render::rich_text::next_selector_boundary(&line.text.chars().collect::<Vec<char>>(), edit.caret) - edit.caret;
                (base + edit.caret, edit.line, edit.caret)
            }
            (true, LineKind::Body(n)) if n + 1 < body.paragraphs.len() => (base + len, edit.line, edit.caret),
            _ => return,
        };
        let command = Command::EditText { slide: line.slide, shape, at, remove, insert: String::new() };
        if self.outline_run(vec![command], false, cx) {
            self.outline_place(caret_line, caret, None, cx);
        }
    }

    /// Enter: at the end of a title (or inside it) a new slide, the text after the caret its title; in a body line a new
    /// paragraph of the same level.
    pub(crate) fn outline_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.outline_edit().cloned() else { return };
        let lines = self.outline_lines();
        let Some(line) = lines.get(edit.line).cloned() else { return };
        let Some(shape) = line.shape else { return };
        match line.kind {
            LineKind::Body(n) => {
                let Some(body) = self.presentation().slide(line.slide).and_then(|s| text_body_of(s, shape)) else { return };
                let at = paragraph_start(body, n) + edit.range().start;
                let remove = edit.range().len();
                let command = Command::EditText { slide: line.slide, shape, at, remove, insert: "\n".into() };
                if self.outline_run(vec![command], false, cx) {
                    self.outline_place(edit.line + 1, 0, Some(window), cx);
                }
            }
            LineKind::Title => {
                let Some(index) = self.presentation().slide_index(line.slide) else { return };
                let chars: Vec<char> = line.text.chars().collect();
                let start = edit.range().start.min(chars.len());
                let end = edit.range().end.min(chars.len());
                let tail: String = chars[end..].iter().collect();
                let mut commands = Vec::new();
                if start < chars.len() {
                    commands.push(Command::EditText { slide: line.slide, shape, at: start, remove: chars.len() - start, insert: String::new() });
                }
                let layout = self.presentation().slide(line.slide).and_then(|s| s.layout);
                let id = self.document.ids().new_slide_id();
                let slide = self.new_slide(index + 1, id, layout);
                let new_title = slide_parts(&slide).0;
                commands.push(Command::InsertSlide { index: index + 1, slide });
                if let (Some(title), false) = (new_title, tail.is_empty()) {
                    commands.push(Command::SetText { slide: id, shape: title, text: tail });
                }
                if self.outline_run(vec![Command::Group(commands)], false, cx) {
                    // The new slide's title line: after this slide's lines.
                    let lines = self.outline_lines();
                    let at = lines.iter().position(|l| l.slide == id && l.kind == LineKind::Title).unwrap_or(0);
                    self.outline_place(at, 0, Some(window), cx);
                }
            }
        }
    }

    /// Tab (`demote`) and Shift+Tab: the line one level down or up; a title made a line of the slide before; a
    /// first-level body line made a slide.
    pub(crate) fn outline_indent(&mut self, demote: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.outline_edit().cloned() else { return };
        let lines = self.outline_lines();
        let Some(line) = lines.get(edit.line).cloned() else { return };
        // Several lines selected: each body line one level down or up (the titles stay).
        if let Some(((first, _), (last, _))) = edit.span().filter(|((a, _), (b, _))| a != b) {
            let mut commands = Vec::new();
            for l in &lines[first..=last] {
                let LineKind::Body(n) = l.kind else { continue };
                let new = if demote { (l.level < 8).then(|| l.level + 1) } else { l.level.checked_sub(1) };
                if let (Some(new), Some(shape)) = (new, l.shape)
                    && let Some(body) = self.presentation().slide(l.slide).and_then(|s| text_body_of(s, shape))
                {
                    commands.extend(level_commands(l.slide, shape, body, n, new));
                }
            }
            if !commands.is_empty() {
                self.outline_run(vec![Command::Group(commands)], false, cx);
            }
            return;
        }
        match (line.kind, demote) {
            (LineKind::Title, false) => {}
            (LineKind::Title, true) => self.outline_demote_title(&line, edit.line, edit.caret, window, cx),
            (LineKind::Body(n), true) => {
                if line.level >= 8 {
                    return;
                }
                let Some(shape) = line.shape else { return };
                let Some(body) = self.presentation().slide(line.slide).and_then(|s| text_body_of(s, shape)) else { return };
                let commands = level_commands(line.slide, shape, body, n, line.level + 1);
                if self.outline_run(vec![Command::Group(commands)], false, cx) {
                    self.outline_place(edit.line, edit.caret, None, cx);
                }
            }
            (LineKind::Body(n), false) if line.level > 0 => {
                let Some(shape) = line.shape else { return };
                let Some(body) = self.presentation().slide(line.slide).and_then(|s| text_body_of(s, shape)) else { return };
                let commands = level_commands(line.slide, shape, body, n, line.level - 1);
                if self.outline_run(vec![Command::Group(commands)], false, cx) {
                    self.outline_place(edit.line, edit.caret, None, cx);
                }
            }
            (LineKind::Body(n), false) => self.outline_promote_to_slide(&line, n, edit.line, edit.caret, window, cx),
        }
    }

    /// A title made a first-level line at the end of the body of the slide before it, its own body lines a level down; its
    /// slide goes.
    fn outline_demote_title(&mut self, line: &OutlineLine, line_index: usize, caret: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.presentation().slide_index(line.slide) else { return };
        if index == 0 {
            return;
        }
        let slides = &self.presentation().slides;
        let (before, this) = (&slides[index - 1], &slides[index]);
        let Some(before_body) = slide_parts(before).1 else { return };
        let Some(old) = text_body_of(before, before_body) else { return };
        let (_, this_body) = slide_parts(this);
        // The lines that come with it: the title's text, then the body's paragraphs a level down.
        let mut added: Vec<(String, u8)> = vec![(line.text.clone(), 0)];
        if let Some(body) = this_body.and_then(|id| text_body_of(this, id)).filter(|b| !is_blank(b)) {
            added.extend(body.paragraphs.iter().map(|p| (p.text(), (p.level + 1).min(8))));
        }
        let old_for_sizes = old.clone();
        let mut body = old.clone();
        let blank = is_blank(&body);
        let first_margin = body.paragraphs.first().map_or(0, |p| p.margin_left);
        let total = body.text().chars().count();
        let start_paragraph = if blank { 0 } else { body.paragraphs.len() };
        let text: Vec<&str> = added.iter().map(|(t, _)| t.as_str()).collect();
        let joined = if blank { text.join("\n") } else { format!("\n{}", text.join("\n")) };
        body.splice(if blank { 0 } else { total }, 0, &joined);
        let sizes: Vec<Option<u32>> = added.iter().map(|(_, level)| level_size(&old_for_sizes, usize::MAX, *level)).collect();
        for (i, (_, level)) in added.iter().enumerate() {
            if let Some(p) = body.paragraphs.get_mut(start_paragraph + i) {
                p.level = *level;
                p.margin_left = first_margin + LEVEL_INDENT * i64::from(*level);
                if let Some(size) = sizes[i] {
                    for run in &mut p.runs {
                        run.style.font_size = Some(size);
                    }
                }
            }
        }
        let first_new = start_paragraph;
        let commands = vec![Command::SetTextBody { slide: before.id, shape: before_body, body }, Command::RemoveSlide { slide: line.slide }];
        let (before_id, title_id) = (before.id, line.slide);
        let _ = (title_id, line_index);
        if self.outline_run(vec![Command::Group(commands)], false, cx) {
            // (A body that is one empty line shows it: the caret is in it.)
            if let Some(e) = self.pptx_mut().and_then(|s| s.outline_edit.as_mut()) {
                e.blank = Some(before_id);
            }
            let lines = self.outline_lines();
            let at = lines.iter().position(|l| l.slide == before_id && l.kind == LineKind::Body(first_new)).unwrap_or(0);
            self.outline_place(at, caret, Some(window), cx);
        }
    }

    /// A first-level body line (paragraph `n`) made the title of a new slide after its own, the body lines after it moving
    /// to that slide's body.
    fn outline_promote_to_slide(&mut self, line: &OutlineLine, n: usize, line_index: usize, caret: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.presentation().slide_index(line.slide) else { return };
        let Some(shape) = line.shape else { return };
        let Some(old) = self.presentation().slide(line.slide).and_then(|s| text_body_of(s, shape)).cloned() else { return };
        let moved: Vec<ibis_doc_model::Paragraph> = old.paragraphs[n + 1..].to_vec();
        let layout = self.presentation().slide(line.slide).and_then(|s| s.layout);
        let id = self.document.ids().new_slide_id();
        let mut slide = self.new_slide(index + 1, id, layout);
        let (title, body) = slide_parts(&slide);
        if let Some(title) = title {
            slide.shapes.iter_mut().filter(|s| s.id == title).for_each(|s| {
                if let ibis_doc_model::ShapeKind::TextBox(b) = &mut s.kind {
                    b.splice(0, b.text().chars().count(), &line.text);
                } else if let ibis_doc_model::ShapeKind::Auto(a) = &mut s.kind
                    && let Some(b) = a.text.as_mut()
                {
                    b.splice(0, b.text().chars().count(), &line.text);
                }
            });
        }
        if let Some(body_id) = body
            && !moved.is_empty()
        {
            for s in slide.shapes.iter_mut().filter(|s| s.id == body_id) {
                let target = match &mut s.kind {
                    ibis_doc_model::ShapeKind::TextBox(b) => Some(b),
                    ibis_doc_model::ShapeKind::Auto(a) => a.text.as_mut(),
                    _ => None,
                };
                if let Some(b) = target {
                    b.paragraphs = moved.clone();
                }
            }
        }
        // The line and the ones after it out of this body (the paragraph break before them with them).
        let start = paragraph_start(&old, n);
        let total = old.text().chars().count();
        let (at, remove) = if n == 0 { (0, total) } else { (start - 1, total - (start - 1)) };
        let commands = vec![Command::EditText { slide: line.slide, shape, at, remove, insert: String::new() }, Command::InsertSlide { index: index + 1, slide }];
        let _ = line_index;
        if self.outline_run(vec![Command::Group(commands)], false, cx) {
            let lines = self.outline_lines();
            let at = lines.iter().position(|l| l.slide == id && l.kind == LineKind::Title).unwrap_or(0);
            self.outline_place(at, caret, Some(window), cx);
        }
    }

    /// ⌥⇧↑ ⌥⇧↓: the line one place up or down: a body line past its neighbour in the same body, a title with its slide.
    pub(crate) fn outline_move(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.outline_edit().cloned() else { return };
        let lines = self.outline_lines();
        let Some(line) = lines.get(edit.line).cloned() else { return };
        match line.kind {
            LineKind::Title => {
                let Some(index) = self.presentation().slide_index(line.slide) else { return };
                let to = if down { index + 1 } else { index.wrapping_sub(1) };
                if to >= self.presentation().slides.len() {
                    return;
                }
                if self.outline_run(vec![Command::MoveSlide { slide: line.slide, index: to }], false, cx) {
                    let lines = self.outline_lines();
                    let at = lines.iter().position(|l| l.slide == line.slide && l.kind == LineKind::Title).unwrap_or(0);
                    self.outline_place(at, edit.caret, Some(window), cx);
                }
            }
            LineKind::Body(n) => {
                let Some(shape) = line.shape else { return };
                let Some(old) = self.presentation().slide(line.slide).and_then(|s| text_body_of(s, shape)).cloned() else { return };
                let to = if down { n + 1 } else { n.wrapping_sub(1) };
                if to >= old.paragraphs.len() {
                    return;
                }
                let mut body = old;
                body.paragraphs.swap(n, to);
                if self.outline_run(vec![Command::SetTextBody { slide: line.slide, shape, body }], false, cx) {
                    let at = if down { edit.line + 1 } else { edit.line - 1 };
                    self.outline_place(at, edit.caret, Some(window), cx);
                }
            }
        }
    }

    /// A key of the outline line being edited. `true` if the key was the outline's.
    pub(crate) fn outline_key(&mut self, key: &str, modifiers: &Modifiers, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.outline_edit().cloned() else { return false };
        let lines = self.outline_lines();
        let Some(line) = lines.get(edit.line) else { return false };
        let command = modifiers.platform || (modifiers.control && !cfg!(target_os = "macos"));
        let len = line.text.chars().count();
        let shift = modifiers.shift;
        let set = |this: &mut Self, line: usize, caret: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>| {
            let lines = this.outline_lines();
            let line = line.min(lines.len().saturating_sub(1));
            let caret = caret.min(lines.get(line).map_or(0, |l| l.text.chars().count()));
            let (anchor_line, anchor) = if extend { (edit.anchor_line, edit.anchor) } else { (line, caret) };
            if let Some(slide) = lines.get(line).map(|l| l.slide)
                && slide != this.current_slide
            {
                this.select_slide(slide, window, cx);
                this.focus_handle.focus(window, cx);
            }
            if let Some(s) = this.pptx_mut() {
                let slide = lines.get(line).map(|l| l.slide);
                let blank = s.outline_edit.as_ref().and_then(|e| e.blank).filter(|b| Some(*b) == slide);
                s.outline_edit = Some(OutlineEdit { line, caret, anchor_line, anchor, marked: None, marked_clauses: Vec::new(), typing: false, blank });
            }
            cx.notify();
        };
        match key {
            "left" if modifiers.alt => {
                let chars: Vec<char> = line.text.chars().collect();
                set(self, edit.line, crate::shell::text_edit::word_boundary(&chars, edit.caret, false), shift, window, cx);
            }
            "right" if modifiers.alt => {
                let chars: Vec<char> = line.text.chars().collect();
                set(self, edit.line, crate::shell::text_edit::word_boundary(&chars, edit.caret, true), shift, window, cx);
            }
            "left" if command => set(self, edit.line, 0, shift, window, cx),
            "right" if command => set(self, edit.line, len, shift, window, cx),
            "left" => {
                let r = edit.range();
                if !r.is_empty() && !shift {
                    set(self, edit.line, r.start, false, window, cx);
                } else if edit.caret == 0 && edit.line > 0 {
                    let up = lines[edit.line - 1].text.chars().count();
                    set(self, edit.line - 1, up, shift, window, cx);
                } else {
                    let chars: Vec<char> = line.text.chars().collect();
                    set(self, edit.line, crate::render::rich_text::prev_selector_boundary(&chars, edit.caret), shift, window, cx);
                }
            }
            "right" => {
                let r = edit.range();
                if !r.is_empty() && !shift {
                    set(self, edit.line, r.end, false, window, cx);
                } else if edit.caret >= len && edit.line + 1 < lines.len() {
                    set(self, edit.line + 1, 0, shift, window, cx);
                } else {
                    let chars: Vec<char> = line.text.chars().collect();
                    set(self, edit.line, crate::render::rich_text::next_selector_boundary(&chars, edit.caret), shift, window, cx);
                }
            }
            "up" if modifiers.alt && shift => self.outline_move(false, window, cx),
            "down" if modifiers.alt && shift => self.outline_move(true, window, cx),
            "up" if edit.line > 0 => set(self, edit.line - 1, edit.caret, shift, window, cx),
            "down" if edit.line + 1 < lines.len() => set(self, edit.line + 1, edit.caret, shift, window, cx),
            "up" | "down" => {}
            "home" => set(self, edit.line, 0, shift, window, cx),
            "end" => set(self, edit.line, len, shift, window, cx),
            "backspace" => self.outline_delete(false, cx),
            "delete" => self.outline_delete(true, cx),
            "enter" => self.outline_enter(window, cx),
            "tab" => self.outline_indent(!shift, window, cx),
            "a" if command => {
                // Every line of the outline.
                let last = lines.len() - 1;
                let last_len = lines[last].text.chars().count();
                if let Some(s) = self.pptx_mut().and_then(|s| s.outline_edit.as_mut()) {
                    s.anchor_line = 0;
                    s.anchor = 0;
                    s.line = last;
                    s.caret = last_len;
                }
                cx.notify();
            }
            "c" | "x" if command && !modifiers.alt => {
                self.outline_copy(key == "x", cx);
            }
            "v" if command && !modifiers.alt => {
                self.outline_paste(cx);
            }
            "escape" => self.end_outline_edit(cx),
            _ => return false,
        }
        true
    }

    /// Copy or cut the selected text in the outline to the system clipboard.
    pub(crate) fn outline_copy(&mut self, cut: bool, cx: &mut Context<Self>) -> bool {
        if !self.is_outline_editing() {
            return false;
        }
        if let Some(text) = self.outline_selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            if cut {
                self.outline_delete(false, cx);
            }
        }
        true
    }

    /// Paste the system clipboard's text into the outline at the caret or selection.
    pub(crate) fn outline_paste(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.is_outline_editing() {
            return false;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else { return false };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return false;
        }
        self.outline_type(&text, cx);
        true
    }

    /// The text of the selected lines (a newline between the lines), if something is selected.
    pub(crate) fn outline_selected_text(&self) -> Option<String> {
        let edit = self.outline_edit()?;
        let ((l1, o1), (l2, o2)) = edit.span()?;
        let lines = self.outline_lines();
        let part = |l: usize, from: usize, to: usize| -> String { lines[l].text.chars().skip(from).take(to.saturating_sub(from)).collect() };
        if l1 == l2 {
            return Some(part(l1, o1, o2));
        }
        let mut out = vec![part(l1, o1, usize::MAX)];
        out.extend((l1 + 1..l2).map(|l| lines[l].text.clone()));
        out.push(part(l2, 0, o2));
        Some(out.join("\n"))
    }

    // ----- The input hooks (see `shell/input.rs`) ----------------------------

    /// The text of the line edited with what the input method composes at the caret, and where it is in that text.
    fn outline_display(&self) -> Option<(String, Range<usize>)> {
        let edit = self.outline_edit()?;
        let line = self.outline_lines().into_iter().nth(edit.line)?;
        let r = edit.range();
        let chars: Vec<char> = line.text.chars().collect();
        let mut text: String = chars[..r.start.min(chars.len())].iter().collect();
        let marked = edit.marked.clone().unwrap_or_default();
        text.push_str(&marked);
        let marked_range = r.start..r.start + marked.chars().count();
        text.extend(chars[r.end.min(chars.len())..].iter());
        Some((text, marked_range))
    }

    pub(crate) fn outline_text_for_range(&self, range: Range<usize>, adjusted: &mut Option<Range<usize>>) -> Option<String> {
        let (text, _) = self.outline_display()?;
        let offsets = utf16_offsets(&text);
        let (start, end) = (to_char(&offsets, range.start), to_char(&offsets, range.end));
        *adjusted = Some(offsets[start]..offsets[end]);
        Some(text.chars().skip(start).take(end - start).collect())
    }

    pub(crate) fn outline_selected_range(&self) -> Option<UTF16Selection> {
        let edit = self.outline_edit()?;
        let (text, marked) = self.outline_display()?;
        let offsets = utf16_offsets(&text);
        let at = |i: usize| offsets[i.min(offsets.len() - 1)];
        // While composing, the caret is at the end of what is composed.
        let r = if edit.marked.is_some() { marked.end..marked.end } else { edit.range() };
        Some(UTF16Selection { range: at(r.start)..at(r.end), reversed: edit.caret < edit.anchor })
    }

    /// The clauses of the composing text and how the input method underlines them.
    pub(crate) fn outline_set_clauses(&mut self, clauses: Vec<ImeClause>, cx: &mut Context<Self>) {
        if let Some(e) = self.pptx_mut().and_then(|s| s.outline_edit.as_mut()) {
            e.marked_clauses = if e.marked.is_some() { clauses } else { Vec::new() };
        }
        cx.notify();
    }

    pub(crate) fn outline_marked_range(&self) -> Option<Range<usize>> {
        let edit = self.outline_edit()?;
        edit.marked.as_ref()?;
        let (text, marked) = self.outline_display()?;
        let offsets = utf16_offsets(&text);
        Some(offsets[marked.start.min(offsets.len() - 1)]..offsets[marked.end.min(offsets.len() - 1)])
    }

    pub(crate) fn outline_input_replace(&mut self, range: Option<Range<usize>>, text: &str, cx: &mut Context<Self>) {
        if self.outline_edit().is_none() {
            return;
        }
        if let Some(e) = self.pptx_mut().and_then(|s| s.outline_edit.as_mut()) {
            e.marked = None;
        }
        if let (Some(range), Some((all, _))) = (range, self.outline_display()) {
            let offsets = utf16_offsets(&all);
            let (start, end) = (to_char(&offsets, range.start), to_char(&offsets, range.end));
            if let Some(e) = self.pptx_mut().and_then(|s| s.outline_edit.as_mut()) {
                e.anchor_line = e.line;
                e.anchor = start;
                e.caret = end;
            }
        }
        self.outline_type(text, cx);
    }

    pub(crate) fn outline_input_mark(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(e) = self.pptx_mut().and_then(|s| s.outline_edit.as_mut()) {
            e.marked = (!text.is_empty()).then(|| text.to_string());
            e.marked_clauses = Vec::new();
        }
        cx.notify();
    }

    pub(crate) fn outline_input_unmark(&mut self, cx: &mut Context<Self>) {
        let marked = self.pptx_mut().and_then(|s| s.outline_edit.as_mut()).and_then(|e| e.marked.take());
        if let Some(text) = marked.filter(|t| !t.is_empty()) {
            self.outline_type(&text, cx);
        }
    }
}

fn utf16_offsets(text: &str) -> Vec<usize> {
    let mut out = Vec::with_capacity(text.len() + 1);
    let mut at = 0;
    for c in text.chars() {
        out.push(at);
        at += c.len_utf16();
    }
    out.push(at);
    out
}

fn to_char(offsets: &[usize], utf16: usize) -> usize {
    offsets.iter().position(|&o| o >= utf16).unwrap_or(offsets.len().saturating_sub(1))
}
