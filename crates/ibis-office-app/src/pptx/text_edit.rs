// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Editing a shape's text in place, through the shell's text input
//! (`shell/input.rs`: typing and the Japanese input method reach
//! `FormatEditor`'s input hooks) and the keys. The caret and the selection
//! are positions in the shape's text (characters of `TextBody::text`,
//! paragraphs joined by `\n`); every change is a command, so formatting a
//! selection keeps the edit going. Text being composed is kept here and
//! shown in the shape, not put in the document, until it is committed.

use std::ops::Range;

use gpui_kit::*;
use ibis_doc_model::{Command, ShapeId, ShapeKind, TextBody};

use crate::shell::editor::SlideEditor;

/// The text edit in progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextEditing {
    pub(crate) shape: ShapeId,
    /// Where the selection started, and the caret (its moving end).
    pub(crate) anchor: usize,
    pub(crate) caret: usize,
    /// Text the input method is composing, shown at the caret.
    pub(crate) marked: Option<String>,
    /// The shape was made by this edit: typing joins its undo step, and it
    /// goes if left empty (as PowerPoint drops an empty new text box).
    pub(crate) is_new: bool,
    /// The last change was typing, so more typing joins its undo step.
    pub(crate) typing: bool,
    /// The cell (row, column) of a table whose text is edited, instead of
    /// the shape's own text.
    pub(crate) cell: Option<(usize, usize)>,
    /// The other corner of the cells selected with it (Shift+click), for
    /// セルの結合.
    pub(crate) cell_anchor: Option<(usize, usize)>,
    /// The edit gave the shape an empty text body (its undo length right
    /// after). Left with no input at all, the body is taken back, so a mere
    /// click does not change the shape (and its undo history).
    pub(crate) added_body: Option<usize>,
}

impl TextEditing {
    pub(crate) fn range(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
}

/// The UTF-16 offset of each character position of `text`, and the end.
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

static CARET_BLINK_VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
static CARET_BLINK_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CARET_BLINK_SHAPE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) fn caret_blink_visible() -> bool {
    CARET_BLINK_VISIBLE.load(std::sync::atomic::Ordering::Relaxed)
}

impl SlideEditor {
    pub(crate) fn restart_caret_blink(&self, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering::Relaxed;
        CARET_BLINK_VISIBLE.store(true, Relaxed);
        let shape = self.text_editing().map_or(0, |e| e.shape.0);
        CARET_BLINK_SHAPE.store(shape, Relaxed);
        let generation = CARET_BLINK_GEN.fetch_add(1, Relaxed) + 1;
        if shape == 0 {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(530)).await;
                if CARET_BLINK_GEN.load(Relaxed) != generation {
                    break;
                }
                let ok = this
                    .update(cx, |this, cx| {
                        if this.text_editing().is_none() || CARET_BLINK_GEN.load(Relaxed) != generation {
                            CARET_BLINK_VISIBLE.store(true, Relaxed);
                            return false;
                        }
                        let v = !CARET_BLINK_VISIBLE.load(Relaxed);
                        CARET_BLINK_VISIBLE.store(v, Relaxed);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !ok {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn ensure_caret_blink(&self, cx: &mut Context<Self>) {
        use std::sync::atomic::Ordering::Relaxed;
        if let Some(edit) = self.text_editing() {
            if CARET_BLINK_SHAPE.load(Relaxed) != edit.shape.0 {
                self.restart_caret_blink(cx);
            }
        } else if CARET_BLINK_SHAPE.load(Relaxed) != 0 {
            CARET_BLINK_SHAPE.store(0, Relaxed);
            CARET_BLINK_VISIBLE.store(true, Relaxed);
            CARET_BLINK_GEN.fetch_add(1, Relaxed);
        }
    }
    pub(crate) fn text_editing(&self) -> Option<&TextEditing> {
        self.pptx()
            .and_then(|s| s.edit.as_ref())
            .or_else(|| self.editing.as_ref().map(|e| &e.edit))
            .or_else(|| self.doc_state().and_then(|s| s.box_edit.as_ref()).map(|b| &b.edit))
    }

    pub(crate) fn text_editing_mut(&mut self) -> Option<&mut TextEditing> {
        if self.pptx().is_some() {
            return self.pptx_mut().and_then(|s| s.edit.as_mut());
        }
        if self.editing.is_some() {
            return self.editing.as_mut().map(|e| &mut e.edit);
        }
        self.doc_state_mut().and_then(|s| s.box_edit.as_mut()).map(|b| &mut b.edit)
    }

    /// The text body being edited: the shape's, or its table cell's.
    pub(crate) fn edited_body(&self) -> Option<&TextBody> {
        if let Some(box_edit) = self.doc_state().and_then(|s| s.box_edit.as_ref()) {
            return self.shape_text_body_at(box_edit.at);
        }
        let edit = self.text_editing()?;
        let shape = self.slide().shape(edit.shape)?;
        match (edit.cell, &shape.kind) {
            (Some((r, c)), ShapeKind::Table(table)) => table.rows.get(r)?.cells.get(c).map(|cell| &cell.text),
            (Some(_), _) => None,
            (None, _) => shape.text_body(),
        }
    }

    /// The text being edited.
    pub(crate) fn edited_text(&self) -> Option<String> {
        self.edited_body().map(TextBody::text)
    }

    /// `commands` on the edited shape's text as commands on the edited
    /// cell's when a table cell is edited (the others as they are).
    pub(crate) fn in_edited_cell(&self, commands: Vec<Command>) -> Vec<Command> {
        let Some(edit) = self.text_editing() else { return commands };
        let Some((row, column)) = edit.cell else { return commands };
        commands
            .into_iter()
            .map(|c| match &c {
                Command::EditText { shape, .. } | Command::SetCharacterFormat { shape, .. } | Command::SetParagraphFormat { shape, .. } | Command::SetTextBody { shape, .. }
                    if *shape == edit.shape =>
                {
                    Command::InCell { row, column, command: Box::new(c) }
                }
                _ => c,
            })
            .collect()
    }

    /// Tab (`forward`) or Shift+Tab in a table: edit the next (previous)
    /// cell that is not covered by another, with its text selected, as
    /// PowerPoint does; nothing past the table's ends.
    pub(crate) fn next_table_cell(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((shape, Some((row, column)))) = self.text_editing().map(|e| (e.shape, e.cell)) else { return };
        let Some(ShapeKind::Table(table)) = self.slide().shape(shape).map(|s| &s.kind) else { return };
        let cells: Vec<(usize, usize)> =
            table.rows.iter().enumerate().flat_map(|(r, row)| row.cells.iter().enumerate().filter(|(_, c)| !c.merged).map(move |(c, _)| (r, c))).collect();
        let Some(at) = cells.iter().position(|&c| c == (row, column)) else { return };
        let next = if forward { cells.get(at + 1) } else { at.checked_sub(1).and_then(|i| cells.get(i)) };
        let Some(&next) = next else {
            // Tab in the last cell: a row more, and the caret in its first cell (as PowerPoint).
            if forward && !self.slide().shape(shape).is_some_and(|s| matches!(&s.kind, ShapeKind::Table(t) if t.has_merges())) {
                let rows = match self.slide().shape(shape).map(|s| &s.kind) {
                    Some(ShapeKind::Table(t)) => t.rows.len(),
                    _ => return,
                };
                self.table_insert_row(true, window, cx);
                if self.slide().shape(shape).is_some_and(|s| matches!(&s.kind, ShapeKind::Table(t) if t.rows.len() == rows + 1)) {
                    self.begin_table_cell_edit(shape, (rows, 0), window, cx);
                    if let Some(e) = self.text_editing_mut() {
                        e.anchor = 0;
                    }
                }
            }
            return;
        };
        self.begin_table_cell_edit(shape, next, window, cx);
        if let Some(e) = self.text_editing_mut() {
            e.anchor = 0;
        }
        cx.notify();
    }

    /// Start editing the text of cell `cell` of table `shape`, with the
    /// caret at its end.
    pub(crate) fn begin_table_cell_edit(&mut self, shape: ShapeId, cell: (usize, usize), window: &mut Window, cx: &mut Context<Self>) {
        if self.text_editing().is_some_and(|e| e.shape == shape && e.cell == Some(cell)) {
            return;
        }
        self.end_text_edit(window, cx);
        let len = match self.slide().shape(shape).map(|s| &s.kind) {
            Some(ShapeKind::Table(t)) => t.rows.get(cell.0).and_then(|r| r.cells.get(cell.1)).map(|c| c.text.text().chars().count()),
            _ => None,
        };
        let Some(len) = len else { return };
        if let Some(state) = self.pptx_mut() {
            state.edit = Some(TextEditing { shape, anchor: len, caret: len, marked: None, is_new: false, typing: false, cell: Some(cell), cell_anchor: None, added_body: None });
        }
        self.set_selection(vec![shape]);
        self.focus_handle.focus(window, cx);
        self.restart_caret_blink(cx);
        cx.notify();
    }

    /// Start editing the text of `shape` with the caret at its end (or
    /// selecting all of it).
    pub(crate) fn begin_text_edit(&mut self, shape: ShapeId, is_new: bool, select_all: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.text_editing().is_some_and(|e| e.shape == shape) {
            return;
        }
        self.end_text_edit(window, cx);
        // (The slide's text takes the keys and the typing from the outline's line.)
        if let Some(s) = self.pptx_mut() {
            s.outline_edit = None;
        }
        let slide = self.current_slide;
        let mut added_body = None;
        // A shape with no text yet gets a body to type into.
        if let Some(s) = self.slide().shape(shape)
            && s.text_body().is_none()
            && let ShapeKind::Auto(auto) = &s.kind
        {
            let mut auto = auto.clone();
            auto.text = Some(crate::pptx::editing::centred_body());
            let kind = Box::new(ShapeKind::Auto(auto));
            if let Err(err) = self.document.execute(vec![Command::SetKind { slide, shape, kind }]) {
                log::warn!("edit rejected: {err}");
                return;
            }
            added_body = Some(self.document.undo_len());
        }
        let Some(len) = self.slide().shape(shape).and_then(|s| s.text_body()).map(|b| b.text().chars().count()) else { return };
        let anchor = if select_all { 0 } else { len };
        if let Some(state) = self.pptx_mut() {
            state.edit = Some(TextEditing { shape, anchor, caret: len, marked: None, is_new, typing: false, cell: None, cell_anchor: None, added_body });
        }
        self.set_selection(vec![shape]);
        self.focus_handle.focus(window, cx);
        self.restart_caret_blink(cx);
        cx.notify();
    }

    /// Take back the empty text body that a click gave a shape, when nothing
    /// was typed, and leave the text edit. A change of the shape other than
    /// its text (a fill, say) then sees the shape as it was, which the saved
    /// file can hold. Returns whether it did.
    pub(crate) fn drop_untouched_body(&mut self) -> bool {
        let untouched = self.text_editing().is_some_and(|e| {
            e.cell.is_none() && e.marked.is_none() && e.added_body.is_some_and(|n| n == self.document.undo_len())
                && self.slide().shape(e.shape).and_then(|s| s.text_body()).is_none_or(|b| b.text().trim().is_empty())
        });
        if !untouched || self.doc_state().is_some_and(|s| s.box_edit.is_some()) || self.editing.is_some() {
            return false;
        }
        if let Some(s) = self.pptx_mut() {
            s.pending_click = None;
            s.goal_x = None;
            s.edit = None;
        }
        self.document.retract_last();
        self.sync_selection();
        true
    }

    /// Finish editing (a new text box left empty goes).
    pub(crate) fn end_text_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Composed text not committed yet is put in first.
        if self.text_editing().is_some_and(|e| e.marked.is_some()) {
            self.commit_marked(cx);
        }
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            self.commit_box_edit(window, cx);
            return;
        }
        if self.editing.is_some() {
            self.commit_edit(window, cx);
            return;
        }
        let Some(edit) = self.pptx_mut().and_then(|s| {
            s.pending_click = None;
            s.goal_x = None;
            s.edit.take()
        }) else {
            return;
        };
        let empty = edit.cell.is_none() && self.slide().shape(edit.shape).and_then(|s| s.text_body()).is_none_or(|b| b.text().trim().is_empty());
        if edit.is_new && empty {
            self.document.retract_last();
            self.set_selection(Vec::new());
        } else if empty && edit.added_body.is_some_and(|n| n == self.document.undo_len()) {
            // Nothing was typed: the body the edit added goes (no redo step).
            self.document.retract_last();
        }
        self.sync_selection();
        cx.notify();
    }

    /// Apply a change of the edited text as a command; typing joins the
    /// last step while it goes on (and a new box's typing its insertion).
    fn edit_text(&mut self, command: Command, typing: bool, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.text_editing().cloned() else { return false };
        let join = edit.is_new || (typing && edit.typing);
        let command = self.in_edited_cell(vec![command]);
        let result = if join { self.document.amend_last(command) } else { self.document.execute(command) };
        if let Err(err) = result {
            log::warn!("edit rejected: {err}");
            return false;
        }
        if let Some(e) = self.text_editing_mut() {
            e.typing = typing;
            if !join {
                e.is_new = false;
            }
        }
        if self.is_workbook() {
            self.sync_workbook_peers(cx);
        }
        self.restart_caret_blink(cx);
        cx.notify();
        true
    }

    fn edit_text_splice(&mut self, start: usize, remove: usize, insert: &str, typing: bool, cx: &mut Context<Self>) -> bool {
        if let Some((at, edit)) = self.doc_state().and_then(|s| s.box_edit.as_ref()).map(|b| (b.at, b.edit.clone())) {
            let join = edit.is_new || (typing && edit.typing);
            let cmd = ibis_doc_model::FlowCommand::SpliceShapeText { at, start, remove, insert: insert.to_string() };
            let keep = self.text_selection();
            let done = if join {
                let res = self.document.amend_last(vec![Command::Flow(cmd)]);
                if res.is_ok() {
                    self.sync_document_peers(cx);
                }
                res.is_ok()
            } else {
                self.run_flow(vec![cmd], cx)
            };
            if done {
                if let (Some((a, c)), Some(st)) = (keep, self.doc_state_mut()) {
                    st.anchor = Some(a);
                    st.caret = Some(c);
                }
                if let Some(b) = self.doc_state_mut().and_then(|s| s.box_edit.as_mut()) {
                    b.edit.typing = typing;
                    b.edit.is_new = false;
                }
                self.sync_docx_edited_box_resource();
                cx.notify();
            }
            return done;
        }
        let Some(edit) = self.text_editing().cloned() else { return false };
        let command = Command::EditText { slide: self.current_slide, shape: edit.shape, at: start, remove, insert: insert.to_string() };
        self.edit_text(command, typing, cx)
    }

    /// Put `text` in place of the selection (or at the caret).
    pub(crate) fn shape_type_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(edit) = self.text_editing().cloned() else { return };
        let range = edit.range();
        if self.edit_text_splice(range.start, range.len(), text, true, cx) {
            let at = range.start + text.chars().count();
            if let Some(e) = self.text_editing_mut() {
                e.anchor = at;
                e.caret = at;
            }
            if self.pages_are_slides() {
                // A web address or a mail address finished by a space or Enter becomes a link (as PowerPoint's autoformat).
                if matches!(text, " " | "\n" | "\u{3000}") {
                    // 自動修正: a typo or a mark typed out (`teh`, `(c)`) is put right first (as PowerPoint's AutoCorrect).
                    let end = self.autocorrect_before(range.start, cx);
                    self.autolink_before(end, cx);
                }
                // "- " or "1. " at the start of a paragraph: a bulleted or numbered list (as PowerPoint's autoformat).
                if text == " " {
                    self.autolist_after_space(range.start, cx);
                }
            }
        }
    }

    /// A space typed at `space_at`, after a bullet mark (`-`, `*`, `•`) or a number (`1.`, `2)`) that is all
    /// of the paragraph before it: the mark taken away and the paragraph made a bulleted or numbered list.
    fn autolist_after_space(&mut self, space_at: usize, cx: &mut Context<Self>) {
        let Some(edit) = self.text_editing().cloned() else { return };
        let Some(chars) = self.edited_text().map(|t| t.chars().collect::<Vec<char>>()) else { return };
        let space_at = space_at.min(chars.len());
        let start = chars[..space_at].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
        let marker: String = chars[start..space_at].iter().collect();
        let numbered = !marker.is_empty() && marker.len() <= 4 && marker.chars().last().is_some_and(|c| c == '.' || c == ')') && marker[..marker.len() - 1].chars().all(|c| c.is_ascii_digit()) && marker.len() > 1;
        let kind = match marker.as_str() {
            "-" | "*" | "•" | "・" | "–" => ibis_doc_model::ListKind::Bullet, // i18n-ok（入力の記号）
            _ if numbered => ibis_doc_model::ListKind::Numbered,
            _ => return,
        };
        // Not a paragraph that is that kind of list already.
        if self.selected_list() == Some(kind) {
            return;
        }
        let slide = self.current_slide;
        let remove = Command::EditText { slide, shape: edit.shape, at: start, remove: marker.chars().count() + 1, insert: String::new() };
        if !self.edit_text(remove, false, cx) {
            return;
        }
        if let Some(e) = self.text_editing_mut() {
            e.anchor = start;
            e.caret = start;
        }
        let command = Command::SetParagraphFormat { slide, shape: edit.shape, start, end: start, format: ibis_doc_model::ParagraphFormat { list: Some(kind), ..Default::default() } };
        let commands = self.in_edited_cell(vec![command]);
        self.run(commands, cx);
    }

    /// 自動修正: the word just before `end` (the typed space or Enter's place) replaced by what the table of
    /// corrections has for it. The place the typed space is at after that.
    fn autocorrect_before(&mut self, end: usize, cx: &mut Context<Self>) -> usize {
        let Some(edit) = self.text_editing().cloned() else { return end };
        let Some(chars) = self.edited_text().map(|t| t.chars().collect::<Vec<char>>()) else { return end };
        let end = end.min(chars.len());
        let start = chars[..end].iter().rposition(|c| c.is_whitespace()).map_or(0, |i| i + 1);
        let word: String = chars[start..end].iter().collect();
        // Not what follows a character of a word (`(c)` in the middle of one is not the mark).
        let Some(correct) = autocorrection(&word) else { return end };
        let slide = self.current_slide;
        let command = Command::EditText { slide, shape: edit.shape, at: start, remove: word.chars().count(), insert: correct.clone() };
        if !self.edit_text(command, false, cx) {
            return end;
        }
        let space_at = start + correct.chars().count();
        // The caret after the typed space that follows.
        if let Some(e) = self.text_editing_mut() {
            e.anchor = space_at + 1;
            e.caret = space_at + 1;
        }
        space_at
    }

    /// The word just before `end` (the typed space or Enter's place) made a link if it is a web or mail address.
    fn autolink_before(&mut self, end: usize, cx: &mut Context<Self>) {
        let Some(edit) = self.text_editing().cloned() else { return };
        let Some(chars) = self.edited_text().map(|t| t.chars().collect::<Vec<char>>()) else { return };
        let end = end.min(chars.len());
        let start = chars[..end].iter().rposition(|c| c.is_whitespace()).map_or(0, |i| i + 1);
        let mut word: String = chars[start..end].iter().collect();
        // Marks that end a sentence are not the address's.
        let trimmed = word.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', '）', '。', '、']).len();
        let cut = word.chars().count() - word[..trimmed].chars().count();
        word.truncate(trimmed);
        let end = end - cut;
        let url = autolink_target(&word);
        let Some(url) = url else { return };
        let slide = self.current_slide;
        let command = Command::SetCharacterFormat { slide, shape: edit.shape, start, end, format: ibis_doc_model::CharacterFormat { link: Some(Some(ibis_doc_model::SlideLink::Url(url))), color:Some(ibis_doc_model::Rgb(0x0563c1)),underline:Some(true), ..Default::default() } };
        let commands = self.in_edited_cell(vec![command]);
        self.run(commands, cx);
    }

    /// Backspace (`forward`: Delete): the selection, or the character before
    /// (after) the caret.
    pub(crate) fn delete_text(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.text_editing().cloned() else { return };
        let len = self.edited_text().map_or(0, |t| t.chars().count());
        let mut range = edit.range();
        if range.is_empty() {
            range = if forward { range.start..(range.start + 1).min(len) } else { range.start.saturating_sub(1)..range.start };
        }
        if range.is_empty() {
            return;
        }
        if self.edit_text_splice(range.start, range.len(), "", false, cx) {
            if let Some(e) = self.text_editing_mut() {
                e.anchor = range.start;
                e.caret = range.start;
            }
        }
    }

    /// ⌘C ⌘X: the selected text on the system's clipboard (`cut`: and taken away). `false` when no text is edited.
    pub(crate) fn copy_edited_text(&mut self, cut: bool, cx: &mut Context<Self>) -> bool {
        if self.is_outline_editing() {
            return self.outline_copy(cut, cx);
        }
        let Some(edit) = self.text_editing().cloned() else { return false };
        let range = edit.range();
        if range.is_empty() {
            return true;
        }
        let Some(text) = self.edited_text().map(|t| t.chars().skip(range.start).take(range.len()).collect::<String>()) else { return true };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        if cut {
            self.delete_text(false, cx);
        }
        true
    }

    /// ⌘V in a text: the clipboard's text put in (paragraphs as they are), in place of the selection. `false` if none.
    pub(crate) fn paste_edited_text(&mut self, cx: &mut Context<Self>) -> bool {
        if self.is_outline_editing() {
            return self.outline_paste(cx);
        }
        if self.text_editing().is_none() {
            return false;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else { return false };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.is_empty() {
            return false;
        }
        self.shape_type_text(&text, cx);
        true
    }

    /// Picture files dropped on the slide (from the Finder) at `position` (the window's): each put there, the
    /// middle of the first at the point (a picture that took the place of an empty placeholder stays in it).
    pub(crate) fn drop_picture_files(&mut self, paths: &[std::path::PathBuf], position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.slide_layout() else { return };
        let at = layout.to_emu(position);
        self.end_text_edit(window, cx);
        let mut inserted = Vec::new();
        for path in paths {
            let ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg"));
            let Ok(data) = std::fs::read(path).map_err(|e| log::warn!("{}: {e}", path.display())) else { continue };
            if !ok {
                continue;
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if let Some(id) = self.insert_picture(&name, data, cx) {
                inserted.push(id);
            }
        }
        // The first picture's middle at the drop point (the others stay where they were put).
        if let Some(&first) = inserted.first()
            && !self.slide().placeholders.contains_key(&first)
            && let Some(frame) = self.slide().shape(first).map(|s| s.frame)
        {
            let size = self.presentation().size_of(self.slide());
            let x = (at.x - frame.width / 2).clamp(0, (size.width - frame.width).max(0));
            let y = (at.y - frame.height / 2).clamp(0, (size.height - frame.height).max(0));
            if (x, y) != (frame.x, frame.y) {
                let slide = self.current_slide;
                self.run(vec![Command::SetFrame { slide, shape: first, frame: ibis_doc_model::EmuRect::new(x, y, frame.width, frame.height) }], cx);
            }
        }
        if !inserted.is_empty() {
            self.set_selection(inserted);
        }
    }

    /// What the system's clipboard holds, as a number (to tell that something else was copied since).
    fn system_clipboard_stamp(cx: &Context<Self>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let Some(item) = cx.read_from_clipboard() {
            for entry in item.entries() {
                match entry {
                    ClipboardEntry::String(s) => s.text().hash(&mut hasher),
                    ClipboardEntry::Image(i) => i.bytes.hash(&mut hasher),
                    ClipboardEntry::ExternalPaths(p) => p.paths().hash(&mut hasher),
                }
            }
        }
        crate::native_shape_clipboard::read().hash(&mut hasher);
        hasher.finish()
    }

    /// ⌘C on shapes: what other applications can take of them goes on the system's clipboard: a
    /// picture's image (one picture selected), else the text of the shapes (paragraphs; shapes
    /// apart by a blank line). The shapes themselves are kept here (for pasting here).
    pub(crate) fn write_shapes_to_system_clipboard(&mut self, cx: &mut Context<Self>) {
        let shapes: Vec<ibis_doc_model::Shape> = self.selected_shapes().into_iter().cloned().collect();
        if self.copy_office_shapes(&shapes,cx){return;}
        let picture = match shapes.as_slice() {
            [one] => match &one.kind {
                ibis_doc_model::ShapeKind::Picture(p) => self.presentation().media(p.media).and_then(|m| {
                    let format = match m.format {
                        ibis_doc_model::MediaFormat::Png => ImageFormat::Png,
                        ibis_doc_model::MediaFormat::Jpeg => ImageFormat::Jpeg,
                        ibis_doc_model::MediaFormat::Gif => ImageFormat::Gif,
                        ibis_doc_model::MediaFormat::Bmp => ImageFormat::Bmp,
                        ibis_doc_model::MediaFormat::Svg => ImageFormat::Svg,
                        _ => return None,
                    };
                    Some(Image::from_bytes(format, m.data.to_vec()))
                }),
                _ => None,
            },
            _ => None,
        };
        if let Some(image) = picture {
            return cx.write_to_clipboard(ClipboardItem::new_image(&image));
        }
        let texts: Vec<String> = shapes.iter().filter_map(|s| s.text_body().map(|b| b.text())).filter(|t| !t.is_empty()).collect();
        if !texts.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(texts.join("\n\n")));
        }
    }

    /// The shapes copied: what the system's clipboard holds now is noted, so a later copy in another application is told.
    pub(crate) fn stamp_shape_clipboard(&mut self, cx: &mut Context<Self>) {
        let stamp = Self::system_clipboard_stamp(cx);
        if let Some(s) = self.pptx_mut() {
            s.clipboard_stamp = Some(stamp);
        }
    }

    /// ⌘V on the slide: what was copied last, the shapes (copied here) or what another application put on the clipboard since.
    pub(crate) fn paste_from_clipboards(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let stamp = self.pptx().and_then(|s| s.clipboard_stamp);
        let external_is_newer = stamp != Some(Self::system_clipboard_stamp(cx));
        if !external_is_newer && self.paste_shapes(cx) {
            return true;
        }
        self.paste_office_shapes(cx) || self.paste_text_as_text_box(window, cx) || self.paste_shapes(cx)
    }

    /// ⌘V with no text edited and no shapes copied: the clipboard's text in a new text box in the middle of the slide.
    pub(crate) fn paste_text_as_text_box(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        // A picture (a screenshot, a copied image) or picture files (copied in the Finder) come first.
        if let Some(item) = cx.read_from_clipboard() {
            let mut inserted = false;
            for entry in item.entries() {
                match entry {
                    ClipboardEntry::Image(image) => {
                        let extension = match image.format {
                            ImageFormat::Png => "png",
                            ImageFormat::Jpeg => "jpg",
                            ImageFormat::Gif => "gif",
                            ImageFormat::Svg => "svg",
                            ImageFormat::Bmp => "bmp",
                            _ => continue,
                        };
                        inserted |= self.insert_picture(&format!("clipboard.{extension}"), image.bytes.clone(), cx).is_some();
                    }
                    ClipboardEntry::ExternalPaths(paths) => {
                        for path in paths.paths() {
                            let ok = path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg"));
                            if let (true, Ok(data)) = (ok, std::fs::read(path)) {
                                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                inserted |= self.insert_picture(&name, data, cx).is_some();
                            }
                        }
                    }
                    ClipboardEntry::String(_) => {}
                }
            }
            if inserted {
                return true;
            }
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else { return false };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if text.trim().is_empty() {
            return false;
        }
        // Cells copied from a worksheet (rows of text with tabs between the cells): a table.
        let lines: Vec<&str> = text.trim_end_matches('\n').split('\n').collect();
        let grid: Vec<Vec<String>> = lines.iter().map(|l| l.split('\t').map(|c| c.trim().to_string()).collect()).collect();
        let columns = grid.first().map_or(0, Vec::len);
        if columns >= 2 && grid.len() >= 1 && grid.iter().all(|r| r.len() == columns) && columns <= 30 && grid.len() <= 100 {
            self.insert_slide_table_with(grid.len(), columns, Some(&grid), window, cx);
            return true;
        }
        let size = self.presentation().size_of(self.slide());
        let frame = self.presentation().default_text_box_frame(ibis_doc_model::EmuPoint::new(size.width / 4, size.height / 3));
        let id = self.document.ids().new_shape_id();
        self.insert_shape(ibis_doc_model::Shape::text_box(id, frame, &text), cx);
        self.set_selection(vec![id]);
        let _ = window;
        true
    }

    /// ⌥← ⌥→ (and with Shift: the selection): the caret to the start of the previous word or the end
    /// of the next one.
    pub(crate) fn word_target(&self, forward: bool) -> Option<usize> {
        let edit = self.text_editing()?;
        let chars: Vec<char> = self.edited_text()?.chars().collect();
        Some(word_boundary(&chars, edit.caret.min(chars.len()), forward))
    }

    /// ⌥⌫ ⌥⌦ (⌘⌫: to the start of the line): the selection, else the text to the word's (line's) edge, taken away.
    pub(crate) fn delete_to(&mut self, forward: bool, line: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.text_editing().cloned() else { return };
        if !edit.range().is_empty() {
            return self.delete_text(forward, cx);
        }
        let Some(chars) = self.edited_text().map(|t| t.chars().collect::<Vec<char>>()) else { return };
        let caret = edit.caret.min(chars.len());
        let to = if line {
            if forward { chars[caret..].iter().position(|&c| c == '\n').map_or(chars.len(), |i| caret + i) } else { chars[..caret].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1) }
        } else {
            word_boundary(&chars, caret, forward)
        };
        if to == caret {
            return self.delete_text(forward, cx);
        }
        let (start, end) = (caret.min(to), caret.max(to));
        if self.edit_text_splice(start, end - start, "", false, cx) && let Some(e) = self.text_editing_mut() {
            e.anchor = start;
            e.caret = start;
        }
    }

    /// Shift+Enter: a line break (a new line of the same paragraph).
    pub(crate) fn insert_line_break(&mut self, cx: &mut Context<Self>) {
        if self.is_document() {
            self.shape_type_text("\n", cx);
            return;
        }
        let Some(edit) = self.text_editing().cloned() else { return };
        let slide = self.current_slide;
        let Some(mut body) = self.edited_body().cloned() else { return };
        let range = edit.range();
        body.splice(range.start, range.len(), "\n");
        // The paragraph the break started.
        let mut at = 0;
        for i in 0..body.paragraphs.len() {
            let len = body.paragraphs[i].text().chars().count();
            if at == range.start + 1 && i > 0 {
                body.paragraphs[i].line_break = true;
                break;
            }
            at += len + 1;
        }
        body.unify_lines();
        let command = Command::SetTextBody { slide, shape: edit.shape, body };
        if self.edit_text(command, false, cx) {
            if let Some(e) = self.text_editing_mut() {
                e.anchor = range.start + 1;
                e.caret = range.start + 1;
            }
        }
    }

    /// Move the caret to `to`; with `extend`, the selection's end.
    pub(crate) fn shape_move_caret(&mut self, to: usize, extend: bool, cx: &mut Context<Self>) {
        let len = self.edited_text().map_or(0, |t| t.chars().count());
        if let Some(e) = self.text_editing_mut() {
            e.caret = to.min(len);
            if !extend {
                e.anchor = e.caret;
            }
            e.typing = false;
        }
        self.restart_caret_blink(cx);
        cx.notify();
    }

    /// Where the caret goes for a key: left, right, a line up or down (the
    /// paragraph before or after, at the same place), the paragraph's start
    /// or end, the text's start or end.
    pub(crate) fn caret_target(&self, key: &str, whole: bool, extend: bool) -> Option<usize> {
        let edit = self.text_editing()?;
        let text = self.edited_text()?;
        let chars: Vec<char> = text.chars().collect();
        let caret = edit.caret.min(chars.len());
        let start_of_line = chars[..caret].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
        let end_of_line = chars[caret..].iter().position(|&c| c == '\n').map_or(chars.len(), |i| caret + i);
        let column = caret - start_of_line;
        // By the lines as painted, when they are known.
        if !whole
            && matches!(key, "up" | "down" | "home" | "end")
            && let Some(to) = self.caret_marks().and_then(|m| m.line_target(caret, key, self.goal_x(caret)))
        {
            return Some(to);
        }
        Some(match (key, whole) {
            ("up", true) | ("home", true) => 0,
            ("down", true) | ("end", true) => chars.len(),
            // Without Shift, a selection collapses to its end first.
            ("left", _) => {
                let r = edit.range();
                if !r.is_empty() && !extend { r.start } else { caret.saturating_sub(1) }
            }
            ("right", _) => {
                let r = edit.range();
                if !r.is_empty() && !extend { r.end } else { (caret + 1).min(chars.len()) }
            }
            ("home", false) => start_of_line,
            ("end", false) => end_of_line,
            ("up", false) => {
                if start_of_line == 0 {
                    0
                } else {
                    let prev_start = chars[..start_of_line - 1].iter().rposition(|&c| c == '\n').map_or(0, |i| i + 1);
                    (prev_start + column).min(start_of_line - 1)
                }
            }
            ("down", false) => {
                if end_of_line >= chars.len() {
                    chars.len()
                } else {
                    let next_start = end_of_line + 1;
                    let next_end = chars[next_start..].iter().position(|&c| c == '\n').map_or(chars.len(), |i| next_start + i);
                    (next_start + column).min(next_end)
                }
            }
            _ => return None,
        })
    }

    /// The edited shape's text with the composing text shown at the caret,
    /// for drawing; `None` when nothing is being composed.
    pub(crate) fn composing_body(&self, shape: ShapeId) -> Option<TextBody> {
        let edit = self.text_editing().filter(|e| e.shape == shape)?;
        let marked = edit.marked.as_ref()?;
        let mut body = self.edited_body()?.clone();
        let range = edit.range();
        body.splice(range.start, range.len(), marked);
        // Underlined, as input methods show what is being composed.
        let format = ibis_doc_model::CharacterFormat { underline: Some(true), ..Default::default() };
        body.format_runs(range.start, range.start + marked.chars().count(), &format);
        Some(body)
    }

    pub(crate) fn commit_marked(&mut self, cx: &mut Context<Self>) {
        let Some(marked) = self.text_editing_mut().and_then(|e| e.marked.take()) else { return };
        if !marked.is_empty() {
            self.shape_type_text(&marked, cx);
        }
        if self.is_document() {
            self.sync_docx_edited_box_resource();
        }
    }

    // ----- The input hooks (see `shell/input.rs`) ----------------------------

    pub(crate) fn input_text_for_range(&self, range: Range<usize>, adjusted: &mut Option<Range<usize>>) -> Option<String> {
        let text = self.edited_text()?;
        let offsets = utf16_offsets(&text);
        let (start, end) = (to_char(&offsets, range.start), to_char(&offsets, range.end));
        *adjusted = Some(offsets[start]..offsets[end]);
        Some(text.chars().skip(start).take(end - start).collect())
    }

    pub(crate) fn input_selected_range(&self) -> Option<UTF16Selection> {
        let edit = self.text_editing()?;
        let text = self.edited_text()?;
        let offsets = utf16_offsets(&text);
        let r = edit.range();
        let at = |i: usize| offsets[i.min(offsets.len() - 1)];
        Some(UTF16Selection { range: at(r.start)..at(r.end), reversed: edit.caret < edit.anchor })
    }

    /// The UTF-16 offset in the edited text of the character at `p`.
    pub(crate) fn input_index_for_point(&self, p: Point<Pixels>) -> Option<usize> {
        let i = self.text_index_at(p)?;
        let offsets = utf16_offsets(&self.edited_text()?);
        offsets.get(i).copied()
    }

    pub(crate) fn input_marked_range(&self) -> Option<Range<usize>> {
        let edit = self.text_editing()?;
        let marked = edit.marked.as_ref()?;
        let text = self.edited_text()?;
        let offsets = utf16_offsets(&text);
        let start = offsets[edit.range().start.min(offsets.len() - 1)];
        Some(start..start + marked.encode_utf16().count())
    }

    pub(crate) fn input_replace(&mut self, range: Option<Range<usize>>, text: &str, cx: &mut Context<Self>) {
        if self.text_editing().is_none() {
            return;
        }
        if let Some(e) = self.text_editing_mut() {
            e.marked = None;
        }
        if let (Some(range), Some(all)) = (range, self.edited_text()) {
            let offsets = utf16_offsets(&all);
            let (start, end) = (to_char(&offsets, range.start), to_char(&offsets, range.end));
            if let Some(e) = self.text_editing_mut() {
                e.anchor = start;
                e.caret = end;
            }
        }
        self.shape_type_text(text, cx);
        if self.is_document() {
            self.sync_docx_edited_box_resource();
        }
    }

    pub(crate) fn input_mark(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(e) = self.text_editing_mut() {
            e.marked = (!text.is_empty()).then(|| text.to_string());
        }
        if self.is_document() {
            self.sync_docx_edited_box_resource();
        }
        cx.notify();
    }

    pub(crate) fn input_unmark(&mut self, cx: &mut Context<Self>) {
        self.commit_marked(cx);
    }
}

/// Where the caret and the selected text are drawn, relative to the slide
/// (pixels at `scale`), as rectangles.
pub(crate) struct CaretGeometry {
    pub(crate) caret: Bounds<Pixels>,
    pub(crate) selection: Vec<Bounds<Pixels>>,
}

impl SlideEditor {
    /// The caret and selection, laid out as the text is drawn: each
    /// paragraph a line (long lines are not wrapped here yet), placed by
    /// the body's insets, vertical anchor and each paragraph's alignment.
    /// (Until the drawn text's positions are recorded by the renderer.)
    pub(crate) fn caret_geometry(&self, scale: f32) -> Option<CaretGeometry> {
        use ibis_doc_model::{EMU_PER_POINT, LineSpacing, TextAlign, VerticalAnchor};
        let edit = self.text_editing()?;
        let shape = self.slide().shape(edit.shape)?;
        let body = shape.text_body()?;
        let px_per_point = scale * EMU_PER_POINT as f32;
        let font_scale = body.font_scale as f32 / 100_000.0;
        // Each paragraph: (its text, its line height and space above, in px).
        let mut lines = Vec::new();
        let mut total = 0.0;
        for (i, p) in body.paragraphs.iter().enumerate() {
            let style = body.style.with_run(p.runs.first().map(|r| &r.style).unwrap_or(&p.end_style));
            let size = ibis_doc_model::autofit_font_size_pt(style.font_size_pt(), body.font_scale) * px_per_point;
            let height = match p.line_spacing {
                Some(LineSpacing::Percent(v)) => size * 1.2 * v as f32 / 100_000.0,
                Some(LineSpacing::Points(v)) => v as f32 / 100.0 * px_per_point,
                None => size * 1.2,
            };
            let above = if i == 0 { 0.0 } else { p.space_before as f32 / 100.0 * px_per_point };
            let font = crate::render::shape_view::body_font(&style, &self.resources);
            lines.push((p.text(), height, above, style, size, font, p.align.unwrap_or(body.style.align), p.margin_left, p.indent));
            total += height + above;
        }
        let f = shape.frame;
        let (left, top) = (f.x as f32 * scale, f.y as f32 * scale);
        let inner_w = (f.width - body.insets.left - body.insets.right) as f32 * scale;
        let inner_h = (f.height - body.insets.top - body.insets.bottom) as f32 * scale;
        let x0 = left + body.insets.left as f32 * scale;
        let mut y = top + body.insets.top as f32 * scale
            + match body.anchor {
                VerticalAnchor::Top => 0.0,
                VerticalAnchor::Middle => ((inner_h - total) / 2.0).max(0.0),
                VerticalAnchor::Bottom => (inner_h - total).max(0.0),
            };
        let range = edit.range();
        let mut at = 0usize;
        let mut caret = None;
        let mut selection = Vec::new();
        for (text, height, above, _style, font_size, font, align, margin, indent) in &lines {
            y += above;
            let chars: Vec<char> = text.chars().collect();
            let width = |n: usize| self.resources.text_width(&chars[..n.min(chars.len())].iter().collect::<String>(), font, *font_size);
            let full = width(chars.len());
            let start_x = x0 + (*margin + *indent).max(0) as f32 * scale;
            let room = inner_w - (*margin + *indent).max(0) as f32 * scale;
            let offset = match align {
                TextAlign::Left | TextAlign::Justify | TextAlign::Distributed => 0.0,
                TextAlign::Center => ((room - full) / 2.0).max(0.0),
                TextAlign::Right => (room - full).max(0.0),
            };
            let x_of = |n: usize| start_x + offset + width(n);
            let end = at + chars.len();
            if edit.caret >= at && edit.caret <= end && caret.is_none() {
                caret = Some(Bounds::new(point(px(x_of(edit.caret - at)), px(y)), size(px(1.5), px(*height))));
            }
            let (a, b) = (range.start.max(at), range.end.min(end));
            if a < b || (range.start <= at && range.end > end) {
                let (xa, xb) = (x_of(a.saturating_sub(at)), if range.end > end { x_of(chars.len()) + font_size * 0.3 } else { x_of(b - at) });
                selection.push(Bounds::new(point(px(xa), px(y)), size(px((xb - xa).max(1.0)), px(*height))));
            }
            at = end + 1;
            y += height;
        }
        let caret = caret.unwrap_or_else(|| Bounds::new(point(px(x0), px(y)), size(px(1.5), px(12.0))));
        Some(CaretGeometry { caret, selection })
    }

    /// The words of the edited text the spell checker does not know, as ranges of characters (for the red wavy line
    /// under them while the text is edited). The checker is asked again only when the text has changed.
    pub(crate) fn misspelled_ranges(&self) -> Vec<Range<usize>> {
        if !crate::spelling::available() || !crate::shell::autocorrect::settings().spell_as_you_type || self.text_editing().is_some_and(|e| e.marked.is_some()) {
            return Vec::new();
        }
        let (Some(text), Some(state)) = (self.edited_text(), self.pptx()) else { return Vec::new() };
        let mut cache = state.spell_cache.borrow_mut();
        if let Some((cached, found)) = cache.as_ref().filter(|(t, _)| *t == text) {
            let _ = cached;
            return found.clone();
        }
        let found: Vec<Range<usize>> = crate::spelling::check_by_settings(&text, None).into_iter().map(|m| m.start..m.end).collect();
        *cache = Some((text, found.clone()));
        found
    }

    /// What the caret and the selection marks are, for painting them
    /// where the edited text was painted.
    pub(crate) fn caret_marks(&self) -> Option<CaretMarks> {
        let edit = self.text_editing()?;
        let shape = edit.shape;
        let body = self.composing_body(shape).or_else(|| self.edited_body().cloned())?;
        let lengths = body.paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.chars().count()).sum()).collect();
        let range = edit.range();
        let (caret, selection) = match &edit.marked {
            Some(m) => (Some(range.start + m.chars().count()), 0..0),
            None if range.is_empty() => (Some(edit.caret), 0..0),
            None => (None, range),
        };
        let positions = self
            .pptx()
            .map(|s| s.positions.clone())
            .or_else(|| self.editing.as_ref().map(|e| e.positions.clone()))
            .or_else(|| self.doc_state().and_then(|s| s.box_edit.as_ref()).map(|b| b.positions.clone()))?;
        Some(CaretMarks { positions, lengths, caret, selection })
    }

    /// The character (in the edited text) at `p` (window pixels), from
    /// where the text was last painted.
    pub(crate) fn text_index_at(&self, p: Point<Pixels>) -> Option<usize> {
        let marks = self.caret_marks()?;
        let list = marks.painted()?;
        // In the text's own coordinates (it may be turned).
        let (x, y) = list.first()?.text_point(f32::from(p.x), f32::from(p.y));
        // The paragraph at y, or the nearest one.
        let index = list
            .iter()
            .position(|q| y < f32::from(q.bounds.origin.y + q.bounds.size.height))
            .unwrap_or(list.len().saturating_sub(1));
        let local = list.get(index)?.index_at(x, y);
        Some(marks.lengths[..index].iter().map(|n| n + 1).sum::<usize>() + local)
    }

    /// The word (or, with `paragraph`, the paragraph) around character
    /// `i` of the edited text, as a range of characters.
    pub(crate) fn text_unit_around(&self, i: usize, paragraph: bool) -> (usize, usize) {
        let chars: Vec<char> = self.edited_text().unwrap_or_default().chars().collect();
        let i = i.min(chars.len());
        if paragraph {
            let a = chars[..i].iter().rposition(|&c| c == '\n').map_or(0, |k| k + 1);
            let b = chars[i..].iter().position(|&c| c == '\n').map_or(chars.len(), |k| i + k);
            return (a, b);
        }
        // Letters and digits, hiragana, katakana and kanji each make words.
        let class = |c: char| match c {
            '\n' => 0,
            c if c.is_whitespace() => 1,
            '\u{3040}'..='\u{309F}' => 3,
            '\u{30A0}'..='\u{30FF}' | '\u{FF66}'..='\u{FF9F}' => 4,
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' | '\u{F900}'..='\u{FAFF}' | '々' => 5,
            c if c.is_alphanumeric() || c == '_' => 2,
            _ => 6,
        };
        // The character at i, or the one before it at the end of a word.
        let k = if i < chars.len() && class(chars[i]) != 0 { i } else if i > 0 { i - 1 } else { return (i, i) };
        let c = class(chars[k]);
        if c == 0 {
            return (i, i);
        }
        let a = chars[..k].iter().rposition(|&x| class(x) != c).map_or(0, |p| p + 1);
        let mut b = chars[k..].iter().position(|&x| class(x) != c).map_or(chars.len(), |p| k + p);
        // A word takes the spaces after it, as in PowerPoint.
        if c != 1 {
            while b < chars.len() && class(chars[b]) == 1 {
                b += 1;
            }
        }
        (a, b)
    }

    /// The x (the text's own coordinates) ↑↓ keep to from `caret`, when
    /// they were the keys that put it there.
    fn goal_x(&self, caret: usize) -> Option<f32> {
        let (at, x) = self
            .pptx()
            .and_then(|s| s.goal_x)
            .or_else(|| self.editing.as_ref().and_then(|e| e.goal_x))
            .or_else(|| self.doc_state().and_then(|s| s.box_edit.as_ref()).and_then(|b| b.goal_x))?;
        (at == caret).then_some(x)
    }

    /// Move the caret a line up or down (`key`), keeping to the x it had
    /// when the first of these keys was pressed, as PowerPoint does.
    pub(crate) fn move_caret_vertically(&mut self, key: &str, extend: bool, cx: &mut Context<Self>) {
        let Some(caret) = self.text_editing().map(|e| e.caret) else { return };
        let goal = self.goal_x(caret).or_else(|| self.caret_marks().and_then(|m| m.rects(true)).and_then(|(c, _)| c).map(|b| f32::from(b.origin.x)));
        let Some(to) = self.caret_target(key, false, extend) else { return };
        self.shape_move_caret(to, extend, cx);
        if let Some(goal) = goal {
            if let Some(state) = self.pptx_mut() {
                state.goal_x = Some((to, goal));
            } else if let Some(edit) = self.editing.as_mut() {
                edit.goal_x = Some((to, goal));
            } else if let Some(box_edit) = self.doc_state_mut().and_then(|s| s.box_edit.as_mut()) {
                box_edit.goal_x = Some((to, goal));
            }
        }
    }

    /// Put the caret where the click that started the edit was, now that
    /// the text has been painted.
    pub(crate) fn place_caret_at_pending_click(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.pptx_mut().and_then(|s| s.pending_click.take()) else { return };
        if let Some(i) = self.text_index_at(p) {
            self.shape_move_caret(i, false, cx);
        }
    }

    /// The caret in the window (for the input method's candidate window).
    pub(crate) fn caret_bounds(&self) -> Option<Bounds<Pixels>> {
        if let Some((Some(caret), _)) = self.caret_marks().and_then(|m| m.polygons(true)) {
            // Around the (perhaps turned) caret.
            let xs = caret.map(|p| p.0);
            let ys = caret.map(|p| p.1);
            let (l, r) = (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max));
            let (t, b) = (ys.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MIN, f32::max));
            return Some(Bounds::new(point(px(l), px(t)), size(px(r - l), px(b - t))));
        }
        let layout = self.slide_layout()?;
        let g = self.caret_geometry(layout.scale)?;
        Some(Bounds::new(layout.origin + g.caret.origin, g.caret.size))
    }
}

/// The caret and the selection of the edited text, painted where its
/// characters were painted (`TextPositions`, written as the text is drawn).
#[derive(Clone)]
pub(crate) struct CaretMarks {
    positions: crate::render::rich_text::TextPositions,
    /// The characters of each paragraph drawn.
    lengths: Vec<usize>,
    /// The caret shown (none while text is selected).
    caret: Option<usize>,
    selection: Range<usize>,
}

impl CaretMarks {
    /// These marks with characters `range` as the selection and no caret (for the rectangles of a word).
    pub(crate) fn with_selection(&self, range: Range<usize>) -> CaretMarks {
        CaretMarks { positions: self.positions.clone(), lengths: self.lengths.clone(), caret: None, selection: range }
    }

    /// Each paragraph's positions, when the text drawn is this text.
    fn painted(&self) -> Option<Vec<crate::render::rich_text::ParagraphPositions>> {
        let list = self.positions.paragraphs.borrow();
        if list.len() != self.lengths.len() {
            return None;
        }
        let out: Vec<_> = list.iter().map(|p| p.borrow().clone()).collect();
        out.iter().zip(&self.lengths).all(|(p, n)| p.chars.len() == *n && !p.lines.is_empty()).then_some(out)
    }

    /// Where a key moves the caret from `caret` by the painted lines: up
    /// or down a line (to the same x), or to its line's start or end.
    fn line_target(&self, caret: usize, key: &str, goal: Option<f32>) -> Option<usize> {
        let list = self.painted()?;
        let starts: Vec<usize> = self.lengths.iter().scan(0, |at, n| { let s = *at; *at += n + 1; Some(s) }).collect();
        let total = self.lengths.iter().sum::<usize>() + self.lengths.len().saturating_sub(1);
        let n = starts.iter().rposition(|&s| s <= caret)?;
        let p = &list[n];
        let local = (caret - starts[n]).min(p.chars.len());
        let (x, line) = p.caret(local);
        let x = goal.unwrap_or(x);
        let l = *p.lines.get(line)?;
        let at = |n: usize, line: &crate::render::rich_text::LinePosition| starts[n] + list[n].index_at(x, line.top + line.height / 2.0);
        Some(match key {
            "home" => starts[n] + l.start,
            "end" => {
                let wrapped = l.end < p.chars.len();
                let space = l.end > l.start && wrapped;
                starts[n] + if space { l.end - 1 } else { l.end }
            }
            "up" if line > 0 => at(n, &p.lines[line - 1]),
            "up" if n > 0 => at(n - 1, list[n - 1].lines.last()?),
            "up" => 0,
            "down" if line + 1 < p.lines.len() => at(n, &p.lines[line + 1]),
            "down" if n + 1 < list.len() => at(n + 1, list[n + 1].lines.first()?),
            "down" => total,
            _ => return None,
        })
    }

    /// `rects` in the window: each a four-cornered polygon (turned with the
    /// text when it is).
    pub(crate) fn polygons(&self, caret: bool) -> Option<(Option<[(f32, f32); 4]>, Vec<[(f32, f32); 4]>)> {
        let (c, selection) = self.rects(caret)?;
        let list = self.painted()?;
        let p = list.first()?;
        let quad = |b: Bounds<Pixels>| p.corners(f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height));
        Some((c.map(quad), selection.into_iter().map(quad).collect()))
    }

    /// The caret (with `caret` even when text is selected: at the moving
    /// end) and the selected text's rectangles, in the text's own
    /// coordinates (the window's unless the text is turned).
    pub(crate) fn rects(&self, caret: bool) -> Option<(Option<Bounds<Pixels>>, Vec<Bounds<Pixels>>)> {
        let list = self.painted()?;
        let locate = |i: usize| {
            let mut at = 0;
            for (n, len) in self.lengths.iter().enumerate() {
                if i <= at + len {
                    return (n, i - at);
                }
                at += len + 1;
            }
            let last = self.lengths.len().saturating_sub(1);
            (last, self.lengths.get(last).copied().unwrap_or(0))
        };
        let caret_at = |i: usize| {
            let (n, local) = locate(i);
            let p = &list[n];
            let (x, line) = p.caret(local);
            let l = p.lines.get(line).or(p.lines.last())?;
            Some(Bounds::new(point(px(x), px(l.top)), size(px(1.5), px(l.height))))
        };
        let shown = self.caret.or(if caret && !self.selection.is_empty() { Some(self.selection.end) } else { None });
        let caret = shown.and_then(caret_at);
        let mut selection = Vec::new();
        if !self.selection.is_empty() {
            let (first, a) = locate(self.selection.start);
            let (last, b) = locate(self.selection.end);
            for (n, p) in list.iter().enumerate().take(last + 1).skip(first) {
                let from = if n == first { a } else { 0 };
                let to = if n == last { b } else { p.chars.len() };
                // The paragraph's end is selected too.
                let past_end = n < last;
                for line in &p.lines {
                    let (s, e) = (from.max(line.start), to.min(line.end));
                    let at_end = past_end && line.end == p.chars.len();
                    if s >= e && !(at_end && from <= line.end) {
                        continue;
                    }
                    let end_of = |i: usize| p.chars.get(i).map_or(line.x, |c| c.x + c.advance);
                    let (xa, mut xb) = if s < e {
                        (p.chars[s].x, end_of(e - 1))
                    } else {
                        let x = if line.end > line.start { end_of(line.end - 1) } else { line.x };
                        (x, x)
                    };
                    if at_end {
                        xb += line.height * 0.25;
                    }
                    selection.push(Bounds::new(point(px(xa), px(line.top)), size(px((xb - xa).max(1.0)), px(line.height))));
                }
            }
        }
        Some((caret, selection))
    }
}

/// What kind of character `c` is for moving by words: spaces, letters and digits, hiragana,
/// katakana, ideographs, and the marks between (each run of one kind is a word).
fn word_class(c: char) -> u8 {
    match c {
        c if c.is_whitespace() => 0,
        '\u{3041}'..='\u{309f}' => 2,
        '\u{30a0}'..='\u{30ff}' | '\u{ff66}'..='\u{ff9f}' => 3,
        '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' | '\u{3005}' => 4,
        c if c.is_alphanumeric() || c == '_' => 1,
        _ => 5,
    }
}

/// Where the caret goes from `from` by a word: to the end of the next one (`forward`), or the start of
/// the previous one, over the spaces before it.
pub(crate) fn word_boundary(chars: &[char], from: usize, forward: bool) -> usize {
    let n = chars.len();
    let mut i = from.min(n);
    if forward {
        while i < n && word_class(chars[i]) == 0 {
            i += 1;
        }
        if i < n {
            let class = word_class(chars[i]);
            while i < n && word_class(chars[i]) == class {
                i += 1;
            }
        }
    } else {
        while i > 0 && word_class(chars[i - 1]) == 0 {
            i -= 1;
        }
        if i > 0 {
            let class = word_class(chars[i - 1]);
            while i > 0 && word_class(chars[i - 1]) == class {
                i -= 1;
            }
        }
    }
    i
}

#[cfg(test)]
mod word_tests {
    use super::word_boundary;

    #[test]
    fn caret_moves_by_words() {
        let t: Vec<char> = "Hello, world  です。カタカナ漢字".chars().collect();
        assert_eq!(t.len(), 23);
        // Forward: to the end of the next word, over the spaces.
        let forward: Vec<usize> = [0, 5, 6, 12, 16, 17, 21].iter().map(|&i| word_boundary(&t, i, true)).collect();
        assert_eq!(forward, [5, 6, 12, 16, 17, 21, 23]);
        // Back: to the start of the previous one.
        let back: Vec<usize> = [14, 23, 21, 7, 0].iter().map(|&i| word_boundary(&t, i, false)).collect();
        assert_eq!(back, [7, 21, 17, 5, 0]);
    }
}

/// What the typed `word` is corrected to (自動修正): by the AutoCorrect all the formats share (`shell::autocorrect`: its
/// list, the case of the word kept, two initial capitals…). `None` for a word it leaves as it is.
fn autocorrection(word: &str) -> Option<String> {
    let settings = crate::shell::autocorrect::settings();
    if !settings.as_you_type {
        return None;
    }
    let corrected = settings.correct_word(word, false);
    (corrected != word).then_some(corrected)
}

#[cfg(test)]
mod autocorrect_tests {
    use super::autocorrection;

    #[test]
    fn the_table_corrects_what_it_knows() {
        assert_eq!(autocorrection("teh").as_deref(), Some("the"));
        assert_eq!(autocorrection("Teh").as_deref(), Some("The"));
        assert_eq!(autocorrection("(c)").as_deref(), Some("©"));
        assert_eq!(autocorrection("i").as_deref(), Some("I"));
        assert_eq!(autocorrection("I"), None);
        assert_eq!(autocorrection("the"), None);
        assert_eq!(autocorrection("日本語"), None);
    }
}

/// The address `word` is, if it is a web address (`http://`, `https://`, `www.`) or a mail address.
fn autolink_target(word: &str) -> Option<String> {
    let lower = word.to_ascii_lowercase();
    let rest = |prefix: &str| lower.strip_prefix(prefix).is_some_and(|r| r.len() >= 3 && r.contains('.') && r.is_ascii());
    if rest("http://") || rest("https://") {
        return Some(word.to_string());
    }
    if lower.starts_with("www.") && lower.len() > 6 && lower.is_ascii() && lower[4..].contains('.') {
        return Some(format!("http://{word}"));
    }
    if let Some((local, domain)) = word.split_once('@')
        && !local.is_empty()
        && local.chars().all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c))
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && domain.chars().all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
    {
        return Some(format!("mailto:{word}"));
    }
    None
}

#[cfg(test)]
mod autolink_tests {
    use super::autolink_target;

    #[test]
    fn web_and_mail_addresses_are_found() {
        assert_eq!(autolink_target("https://example.com/a").as_deref(), Some("https://example.com/a"));
        assert_eq!(autolink_target("www.example.jp").as_deref(), Some("http://www.example.jp"));
        assert_eq!(autolink_target("taro@example.co.jp").as_deref(), Some("mailto:taro@example.co.jp"));
        for not in ["example", "http://", "www.", "a@b", "@example.com", "100.5", "日本語.jp"] {
            assert_eq!(autolink_target(not), None, "{not}");
        }
    }
}
