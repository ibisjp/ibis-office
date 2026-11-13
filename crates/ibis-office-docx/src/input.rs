// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Text input from the system (typing and the Japanese input method) into
//! a document.
//!
//! The shell receives the input (`shell/input.rs`) and hands it to the
//! document through `FormatEditor`'s input hooks (`mod.rs`), which call
//! the methods here. The text the
//! input method sees is the caret's paragraph, in UTF-16 code units. Text
//! being composed is kept in the state (`State::marked`) and drawn at the
//! caret, not put in the document, until the input method commits it.

use std::ops::Range;

use gpui_kit::*;
use ibis_doc_model::FlowPos;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// UTF-16 offset of each character position of `text`, and the end.
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

fn to_position(offsets: &[usize], utf16: usize) -> usize {
    offsets.iter().position(|&o| o >= utf16).unwrap_or(offsets.len().saturating_sub(1))
}

#[ibis_office_macros::editor_ext(Input)]
impl SlideEditor {
    /// The caret's paragraph text and its UTF-16 offsets, when editing.
    fn input_paragraph(&self) -> Option<(FlowPos, String, Vec<usize>)> {
        let (_, caret) = self.text_selection()?;
        let p = ibis_doc_model::flow_command::paragraph(self.flow()?, caret.para)?;
        let text = p.text();
        let offsets = utf16_offsets(&text);
        Some((caret, text, offsets))
    }
}

/// The input hooks of `FormatEditor` for a document (see `mod.rs`).
#[ibis_office_macros::editor_ext(Input2)]
impl SlideEditor {
    pub(crate) fn doc_text_for_range(&mut self, range: Range<usize>, adjusted: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            return self.input_text_for_range(range, adjusted);
        }
        let (_, text, offsets) = self.input_paragraph()?;
        let (a, b) = (to_position(&offsets, range.start), to_position(&offsets, range.end));
        *adjusted = Some(offsets[a]..offsets[b]);
        Some(text.chars().skip(a).take(b - a).collect())
    }

    pub(crate) fn doc_selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            return self.input_selected_range();
        }
        let (caret, _, offsets) = self.input_paragraph()?;
        let (a, b) = self.ordered_selection()?;
        let (start, end) = if a.para == b.para { (a.offset, b.offset) } else { (caret.offset, caret.offset) };
        let at = |o: usize| offsets.get(o).copied().unwrap_or(0);
        let marked = self.doc_state().and_then(|s| s.marked.as_ref()).map_or(0, |m| m.encode_utf16().count());
        Some(UTF16Selection { range: at(start) + marked..at(end) + marked, reversed: false })
    }

    pub(crate) fn doc_marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            return self.input_marked_range();
        }
        let marked = self.doc_state()?.marked.as_ref()?;
        let (caret, _, offsets) = self.input_paragraph()?;
        let start = offsets.get(caret.offset).copied()?;
        Some(start..start + marked.encode_utf16().count())
    }

    /// How the input method underlines each clause of the composing text, kept for the page to draw (`render::ime_text`). A text box
    /// being edited keeps them in its own editing state.
    pub(crate) fn doc_set_ime_clauses(&mut self, clauses: Vec<ImeClause>, _: &mut Window, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            self.input_set_clauses(clauses, cx);
            return;
        }
        if let Some(state) = self.doc_state_mut() {
            state.marked_clauses = if state.marked.is_some() { clauses } else { Vec::new() };
        }
        cx.notify();
    }

    pub(crate) fn doc_unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            self.input_unmark(cx);
            return;
        }
        if let Some(state) = self.doc_state_mut() {
            state.marked = None;
            state.marked_clauses = Vec::new();
        }
        self.sync_ime_layout(false, cx);
        cx.notify();
    }

    /// Lays the pages out with the text being composed in them (or without it once it is not), as Word pushes the text after it on.
    /// With `reveal`, the page scrolls so the end of the composing text is in view.
    pub(crate) fn sync_ime_layout(&mut self, reveal: bool, cx: &mut Context<Self>) {
        let marked = self.doc_state().and_then(|s| s.marked.clone()).filter(|m| !m.is_empty());
        let caret = self.doc_state().filter(|s| s.box_edit.is_none()).and_then(|s| s.caret);
        let want = match (&marked, caret) {
            (Some(text), Some(caret)) => Some((caret.para, caret.offset, std::sync::Arc::<str>::from(text.as_str()))),
            _ => None,
        };
        if self.resources.ime_marked != want {
            self.resources.ime_marked = want.clone();
            self.relayout_now(cx);
        }
        if reveal && let Some((para, offset, text)) = want {
            let lines = self.flow_lines();
            // The end of the composing text: the layout has it typed in.
            if let Some((page, _, y, h)) = self.caret_box(&lines, FlowPos::new(para, offset + text.chars().count())) {
                self.scroll_into_view(page, y, h, cx);
            }
        }
    }

    pub fn doc_replace_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            self.input_replace(range, text, cx);
            return;
        }
        let composing = self.doc_state_mut().and_then(|s| { s.marked_clauses = Vec::new(); s.marked.take() }).is_some();
        if composing {
            self.sync_ime_layout(false, cx);
        }
        // A range given outside composition replaces those characters of
        // the paragraph (the input method correcting what was typed).
        if let (Some(range), false, Some((caret, _, offsets))) = (range, composing, self.input_paragraph()) {
            let (a, b) = (to_position(&offsets, range.start), to_position(&offsets, range.end));
            if a != b {
                self.set_caret(FlowPos::new(caret.para, a), false, cx);
                self.set_caret(FlowPos::new(caret.para, b), true, cx);
            }
        }
        if !text.is_empty() {
            self.type_text(text, cx);
        }
        cx.notify();
    }

    pub fn doc_replace_and_mark_text_in_range(&mut self, range: Option<Range<usize>>, text: &str, _: Option<Range<usize>>, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            if let Some(range) = range {
                self.input_replace(Some(range), "", cx);
            }
            self.input_mark(text, cx);
            return;
        }
        // Composing over a selection replaces it, as in Word.
        if self.doc_state().is_some_and(|s| s.marked.is_none()) && self.ordered_selection().is_some_and(|(a, b)| a != b) {
            self.delete_char(false, cx);
        }
        if let Some(state) = self.doc_state_mut() {
            state.marked = (!text.is_empty()).then(|| text.to_string());
            state.marked_clauses = Vec::new();
        }
        // The input method places its candidates by where the text being composed is, which is known now: it was asked once, at the
        // start of the composition, when there was no text yet (a column two points long in vertical text, where the candidates came
        // over what was typed, GitHub issue #121).
        window.invalidate_character_coordinates();
        self.sync_ime_layout(true, cx);
        cx.notify();
    }

    pub(crate) fn doc_bounds_for_range(&mut self, range: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            return self.caret_bounds();
        }
        let (_, mut caret) = self.text_selection()?;
        // The text being composed is laid out in the paragraph after the caret, so a position inside it is that many characters on
        // (wrapped text: the rectangle of the line the position is on, which puts the input method's candidates under the last line
        // of it and not over the first).
        let vertical = Self::line_of(&self.flow_lines(), caret).is_some_and(|l| l.vertical);
        if let (false, Some(marked), Some(start)) = (vertical, self.doc_state().and_then(|s| s.marked.clone()), self.input_paragraph().and_then(|(c, _, offsets)| offsets.get(c.offset).copied())) {
            let mut units = 0;
            let mut chars = 0;
            for c in marked.chars() {
                if start + units >= range.start { break; }
                units += c.len_utf16();
                chars += 1;
            }
            caret = FlowPos::new(caret.para, caret.offset + chars);
        }
        let lines = self.flow_lines();
        let (page, x, y, h) = self.caret_box(&lines, caret)?;
        let (origin, k) = self.page_origin(page)?;
        let at = point(origin.x + px(x * k), origin.y + px(y * k));
        if Self::line_of(&lines, caret).is_some_and(|l| l.vertical) {
            // The column across the line, down as far as the text being
            // composed reaches (a character a font size tall, as drawn), so
            // the input method's candidates stay off it.
            let size_pt = self.current_format().map_or(h, |(_, style)| style.size as f32 / 100.0);
            let n = self.doc_state().and_then(|s| s.marked.as_ref()).map_or(0, |m| m.chars().count());
            return Some(Bounds::new(at, size(px(h * k), px((n as f32 * size_pt * k).max(2.0)))));
        }
        Some(Bounds::new(at, size(px(2.0), px(h * k))))
    }

    /// Whether the caret is in a vertical line of the page (a text box being
    /// edited has its own caret).
    pub(crate) fn doc_text_vertical(&self) -> bool {
        if self.doc_state().is_none_or(|s| s.box_edit.is_some()) {
            return false;
        }
        let Some((_, caret)) = self.text_selection() else { return false };
        Self::line_of(&self.flow_lines(), caret).is_some_and(|l| l.vertical)
    }

    pub(crate) fn doc_character_index_for_point(&mut self, point: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            return self.input_index_for_point(point);
        }
        None
    }

    pub(crate) fn doc_accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        self.doc_state().is_some_and(|s| s.box_edit.is_some() || s.caret.is_some())
    }
}
