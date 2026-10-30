// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Editing a document's text: the caret and the selection, moving them,
//! and turning keys, typing and toolbar buttons into flow commands
//! (`ibis_doc_model::FlowCommand`).
//!
//! What is here only decides which command to run; the commands (and
//! undo) are in the model, so a new toolbar or menu calls the same
//! methods. The caret's place on the pages comes from the layout's line
//! positions (`flow_layout::Item::Line`).

use std::sync::Arc;

use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::flow_command::{self, Container};
use ibis_doc_model::{
    Block, CaseConversion, Command, DocumentProtection, FlowAlign, FlowCommand, FlowDocument, FlowParagraph, FlowPos, FlowRun, FlowRunKind, FlowRunStyle, FormFieldData, FormFieldKind, InsertFormat, LineHeight, ParaChange, Rgb,
    RunChange, RunFormat, ThemeFontLang,
};

use crate::docx::flow_layout::{Item, pages};
use crate::shell::editor::{MAX_ZOOM, MIN_ZOOM, RulerUnit, SlideEditor, zoom_scale};

/// Key context of a document's workspace while its text is edited.
pub(crate) const TEXT_CONTEXT: &str = "DocumentText";

#[derive(Clone, Debug, PartialEq, Eq)]
enum TocType {
    Headings((u32, u32)),
    Figures(String),
    Custom(flow_command::TocInstruction),
}

actions!(
    docx,
    [
        /// Delete the selection, or the character before the caret.
        DeleteBackward,
        /// Delete the selection, or the character after the caret.
        DeleteForward,
        /// End the paragraph (Enter).
        NewParagraph,
        /// A line break in the paragraph (Shift+Enter).
        InsertLineBreak,
        InsertTab,
        /// Shift+Tab: a list item one level up.
        OutdentList,
        /// Continue on the next page (⌘Enter).
        InsertPageBreak,
        /// Continue in the next column (⌃⇧↩ / Ctrl+Shift+Enter).
        InsertColumnBreak,
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocumentStart,
        DocumentEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
        PasteKeepSource,
        PastePlain,
        PasteMatchFormat,
        ToggleBold,
        ToggleItalic,
        ToggleUnderline,
        ToggleStrike,
        ToggleCaps,
        ToggleSmallCaps,
        ToggleSuperscript,
        ToggleSubscript,
        AlignLeft,
        AlignCenter,
        AlignRight,
        AlignJustify,
        AlignDistribute,
        GrowFont,
        ShrinkFont,
        ClearFormatting,
        /// 文字の囲み線.
        ToggleCharacterBorder,
        /// 文字の網かけ.
        ToggleCharacterShading,
        /// 検索と置換 (⌘F, ⇧⌘H).
        OpenFind,
        /// 次を検索 (⌘G, F3).
        FindNext,
        /// 前を検索 (⇧⌘G, ⇧F3).
        FindPrevious,
        /// ジャンプ (⌥⌘G, F5).
        OpenGoTo,
        /// 類義語辞典 (⇧F7 / DOCX-REVIEW-019).
        OpenThesaurus,
        /// リンク (⌘K).
        OpenLink,
        /// 文字種の変換 (DOCX-JA-020).
        ConvertCaseSentence,
        ConvertCaseLower,
        ConvertCaseUpper,
        ConvertCaseTitle,
        ConvertCaseToggle,
        ConvertCaseHalfWidth,
        ConvertCaseFullWidth,
        ConvertCaseKatakana,
        ConvertCaseHiragana,
        /// 書式のコピー (⌥⌘C / Ctrl+Shift+C).
        CopyFormat,
        /// 書式の貼り付け (⌥⌘V / Ctrl+Shift+V).
        PasteFormat,
        /// 書式ブラシの解除 (Esc).
        CancelFormatBrush,
        /// 編集記号の表示 (⌘8 / Ctrl+Shift+8).
        ToggleFormattingMarks,
        /// フィールドの更新 (F9).
        UpdateFields,
        /// フィールドのリンク解除 (⇧⌘F9 / Ctrl+Shift+F9).
        UnlinkFields,
        /// フィールドコードの表示/非表示 (Shift+F9).
        ToggleFieldCodes,
        /// すべてのフィールドコードの表示/非表示 (Alt+F9).
        ToggleAllFieldCodes,
        /// フィールドのロック (Ctrl+F11 / ⌘F11).
        LockField,
        /// フィールドのロック解除 (Ctrl+Shift+F11 / ⇧⌘F11).
        UnlockField,
        /// フィールドの挿入 (Ctrl+F9 / ⌘F9).
        InsertField,
        /// フィールドの編集.
        EditField,
        /// 上書きモード切替 (Insert / Overtype mode).
        ToggleOvertype,
        /// 隠し文字 (Ctrl+Shift+H / ⌘⇧X).
        ToggleHidden,
        /// ユーザー名の変更 (Word Options > 全般: ユーザー設定 DOCX-OPTIONS-003).
        ChangeUserName,
        /// ウィンドウの分割の切り替え (⌥⌘S / Alt+Ctrl+S DOCX-VIEW-019).
        ToggleSplit,
        /// アウトライン: 見出しのレベルを上げる・下げる、上へ・下へ移動 (Word の Mac は ⌃⇧←→↑↓、Windows は Alt+Shift+←→↑↓)。
        OutlinePromote,
        OutlineDemote,
        OutlineMoveUp,
        OutlineMoveDown,
        /// Delete the word before or after the caret (⌥⌫・⌥⌦ on a Mac, Ctrl+Backspace・Ctrl+Delete elsewhere).
        DeleteWordBackward,
        DeleteWordForward,
        /// A screen up or down (Page Up・Page Down), and selecting it with Shift.
        PageUp,
        PageDown,
        SelectPageUp,
        SelectPageDown,
        /// Line spacing single, 1.5 and double (⌘1・⌘5・⌘2, Ctrl+1・5・2).
        LineSpacingSingle,
        LineSpacingOneAndHalf,
        LineSpacingDouble,
        /// The styles Heading 1 to 3 (⌘⌥1…3, Ctrl+Alt+1…3) and Normal (⌘⇧N, Ctrl+Shift+N).
        ApplyHeading1,
        ApplyHeading2,
        ApplyHeading3,
        ApplyNormalStyle,
        /// Double underline and underline words only (⌘⇧D・⌘⇧W, Ctrl+Shift+D・W), a footnote and an endnote (⌘⌥F・⌘⌥E, Alt+Ctrl+F・E),
        /// and the bullet list (⌘⇧L, Ctrl+Shift+L).
        ToggleDoubleUnderline,
        ToggleWordUnderline,
        InsertFootnote,
        InsertEndnote,
        ToggleBulletList,
    ]
);

/// The keys of a document's text (Word's, with ⌘ on a Mac).
pub(crate) fn key_bindings() -> Vec<KeyBinding> {
    let c = Some(TEXT_CONTEXT);
    let m = |k: &str| if cfg!(target_os = "macos") { format!("cmd-{k}") } else { format!("ctrl-{k}") };
    let word = |k: &str| if cfg!(target_os = "macos") { format!("alt-{k}") } else { format!("ctrl-{k}") };
    let outline = |k: &str| if cfg!(target_os = "macos") { format!("ctrl-shift-{k}") } else { format!("alt-shift-{k}") };
    vec![
        KeyBinding::new(&word("backspace"), DeleteWordBackward, c),
        KeyBinding::new(&word("delete"), DeleteWordForward, c),
        KeyBinding::new("pageup", PageUp, c),
        KeyBinding::new("pagedown", PageDown, c),
        KeyBinding::new("shift-pageup", SelectPageUp, c),
        KeyBinding::new("shift-pagedown", SelectPageDown, c),
        KeyBinding::new(&m("1"), LineSpacingSingle, c),
        KeyBinding::new(&m("5"), LineSpacingOneAndHalf, c),
        KeyBinding::new(&m("2"), LineSpacingDouble, c),
        KeyBinding::new(&(if cfg!(target_os = "macos") { "alt-cmd-1" } else { "alt-ctrl-1" }), ApplyHeading1, c),
        KeyBinding::new(&(if cfg!(target_os = "macos") { "alt-cmd-2" } else { "alt-ctrl-2" }), ApplyHeading2, c),
        KeyBinding::new(&(if cfg!(target_os = "macos") { "alt-cmd-3" } else { "alt-ctrl-3" }), ApplyHeading3, c),
        KeyBinding::new(&m("shift-n"), ApplyNormalStyle, c),
        KeyBinding::new(&m("shift-d"), ToggleDoubleUnderline, c),
        KeyBinding::new(&m("shift-w"), ToggleWordUnderline, c),
        KeyBinding::new(&(if cfg!(target_os = "macos") { "alt-cmd-f" } else { "alt-ctrl-f" }), InsertFootnote, c),
        KeyBinding::new(&(if cfg!(target_os = "macos") { "alt-cmd-e" } else { "alt-ctrl-e" }), InsertEndnote, c),
        KeyBinding::new(&m("shift-l"), ToggleBulletList, c),
        KeyBinding::new(&outline("left"), OutlinePromote, c),
        KeyBinding::new(&outline("right"), OutlineDemote, c),
        KeyBinding::new(&outline("up"), OutlineMoveUp, c),
        KeyBinding::new(&outline("down"), OutlineMoveDown, c),
        KeyBinding::new("insert", ToggleOvertype, c),
        KeyBinding::new("backspace", DeleteBackward, c),
        KeyBinding::new("shift-backspace", DeleteBackward, c),
        KeyBinding::new("delete", DeleteForward, c),
        KeyBinding::new("enter", NewParagraph, c),
        KeyBinding::new("shift-enter", InsertLineBreak, c),
        KeyBinding::new("tab", InsertTab, c),
        KeyBinding::new("shift-tab", OutdentList, c),
        KeyBinding::new(&m("enter"), InsertPageBreak, c),
        KeyBinding::new("ctrl-shift-enter", InsertColumnBreak, c),
        KeyBinding::new(&m("shift-enter"), InsertColumnBreak, c),
        KeyBinding::new("left", MoveLeft, c),
        KeyBinding::new("right", MoveRight, c),
        KeyBinding::new("up", MoveUp, c),
        KeyBinding::new("down", MoveDown, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new(&word("left"), WordLeft, c),
        KeyBinding::new(&word("right"), WordRight, c),
        KeyBinding::new(&word("shift-left"), SelectWordLeft, c),
        KeyBinding::new(&word("shift-right"), SelectWordRight, c),
        KeyBinding::new("home", LineStart, c),
        KeyBinding::new("end", LineEnd, c),
        KeyBinding::new("shift-home", SelectLineStart, c),
        KeyBinding::new("shift-end", SelectLineEnd, c),
        KeyBinding::new(&m("left"), LineStart, c),
        KeyBinding::new(&m("right"), LineEnd, c),
        KeyBinding::new(&m("shift-left"), SelectLineStart, c),
        KeyBinding::new(&m("shift-right"), SelectLineEnd, c),
        KeyBinding::new(&m("up"), DocumentStart, c),
        KeyBinding::new(&m("down"), DocumentEnd, c),
        KeyBinding::new(&m("home"), DocumentStart, c),
        KeyBinding::new(&m("end"), DocumentEnd, c),
        KeyBinding::new(&m("shift-up"), SelectDocumentStart, c),
        KeyBinding::new(&m("shift-down"), SelectDocumentEnd, c),
        KeyBinding::new(&m("a"), SelectAll, c),
        KeyBinding::new(&m("c"), Copy, c),
        KeyBinding::new(&m("x"), Cut, c),
        KeyBinding::new(&m("v"), Paste, c),
        KeyBinding::new(&m("shift-v"), PastePlain, c),
        KeyBinding::new("alt-cmd-shift-v", PasteMatchFormat, c),
        KeyBinding::new("alt-ctrl-shift-v", PasteMatchFormat, c),
        KeyBinding::new(&m("b"), ToggleBold, c),
        KeyBinding::new(&m("i"), ToggleItalic, c),
        KeyBinding::new(&m("u"), ToggleUnderline, c),
        KeyBinding::new(&m("shift-a"), ToggleCaps, c),
        KeyBinding::new(&m("shift-k"), ToggleSmallCaps, c),
        KeyBinding::new(&m("shift-="), ToggleSuperscript, c),
        KeyBinding::new(&m("="), ToggleSubscript, c),
        KeyBinding::new(&m("l"), AlignLeft, c),
        KeyBinding::new(&m("e"), AlignCenter, c),
        KeyBinding::new(&m("r"), AlignRight, c),
        KeyBinding::new(&m("j"), AlignJustify, c),
        KeyBinding::new(&m("shift-j"), AlignDistribute, c),
        KeyBinding::new("ctrl-shift-j", AlignDistribute, c),
        KeyBinding::new(&m("shift-."), GrowFont, c),
        KeyBinding::new(&m("shift-,"), ShrinkFont, c),
        KeyBinding::new(&m("]"), GrowFont, c),
        KeyBinding::new(&m("["), ShrinkFont, c),
        KeyBinding::new(&m("\\"), ClearFormatting, c),
        KeyBinding::new(&m("space"), ClearFormatting, c),
        KeyBinding::new(&m("f"), OpenFind, c),
        KeyBinding::new(&m("k"), OpenLink, c),
        KeyBinding::new(&m("shift-h"), OpenFind, c),
        KeyBinding::new(&m("g"), FindNext, c),
        KeyBinding::new(&m("shift-g"), FindPrevious, c),
        KeyBinding::new("f3", FindNext, c),
        KeyBinding::new("shift-f3", FindPrevious, c),
        KeyBinding::new("alt-cmd-g", OpenGoTo, c),
        KeyBinding::new("alt-ctrl-g", OpenGoTo, c),
        KeyBinding::new("f5", OpenGoTo, c),
        KeyBinding::new("shift-f7", OpenThesaurus, c),
        KeyBinding::new("alt-cmd-c", CopyFormat, c),
        KeyBinding::new("alt-ctrl-c", CopyFormat, c),
        KeyBinding::new("ctrl-shift-c", CopyFormat, c),
        KeyBinding::new("alt-cmd-v", PasteFormat, c),
        KeyBinding::new("alt-ctrl-v", PasteFormat, c),
        KeyBinding::new("ctrl-shift-v", PasteFormat, c),
        KeyBinding::new("escape", CancelFormatBrush, c),
        KeyBinding::new(&m("shift-8"), ToggleFormattingMarks, c),
        KeyBinding::new(&m("8"), ToggleFormattingMarks, c),
        KeyBinding::new("f9", UpdateFields, c),
        KeyBinding::new(&m("f9"), InsertField, c),
        KeyBinding::new("ctrl-f9", InsertField, c),
        KeyBinding::new(&m("shift-f9"), UnlinkFields, c),
        KeyBinding::new("ctrl-shift-f9", UnlinkFields, c),
        KeyBinding::new("shift-f9", ToggleFieldCodes, c),
        KeyBinding::new("alt-f9", ToggleAllFieldCodes, c),
        KeyBinding::new("alt-cmd-f9", ToggleAllFieldCodes, c),
        KeyBinding::new(&m("f11"), LockField, c),
        KeyBinding::new("ctrl-f11", LockField, c),
        KeyBinding::new(&m("shift-f11"), UnlockField, c),
        KeyBinding::new("ctrl-shift-f11", UnlockField, c),
        KeyBinding::new("ctrl-shift-h", ToggleHidden, c),
        KeyBinding::new(&m("shift-x"), ToggleHidden, c),
        KeyBinding::new(if cfg!(target_os = "macos") { "alt-cmd-s" } else { "alt-ctrl-s" }, ToggleSplit, None),
    ]
}

/// A line of text on a page, from the layout (points from the page's
/// top-left corner).
#[derive(Clone, Debug)]
pub(crate) struct LineGeom {
    pub(crate) page: usize,
    pub(crate) para: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) h: f32,
    pub(crate) stops: Arc<[(usize, f32)]>,
    pub(crate) vertical: bool,
}

impl LineGeom {
    /// Whether the caret at `offset` is shown on this line: a position on
    /// it, or where it ends when it is its paragraph's last line.
    fn holds(&self, offset: usize, last_line: bool) -> bool {
        let n = self.stops.len();
        self.stops.iter().enumerate().any(|(i, (o, _))| *o == offset && (i + 1 < n || last_line))
    }

    fn x_of(&self, offset: usize) -> f32 {
        if self.vertical {
            self.x
        } else {
            self.stops.iter().find(|(o, _)| *o == offset).map_or(self.x, |(_, x)| self.x + x)
        }
    }

    fn y_of(&self, offset: usize) -> f32 {
        if self.vertical {
            self.stops.iter().find(|(o, _)| *o == offset).map_or(self.y, |(_, y)| self.y + y)
        } else {
            self.y
        }
    }

    /// The position nearest `pos` (x along a horizontal line, y down a vertical line).
    fn nearest(&self, pos: f32) -> usize {
        let origin = if self.vertical { self.y } else { self.x };
        let mut best = (f32::INFINITY, 0);
        for (o, s) in self.stops.iter() {
            let d = (origin + s - pos).abs();
            if d < best.0 {
                best = (d, *o);
            }
        }
        best.1
    }
}

/// The order of the paragraphs, per list of blocks read in turn: the body
/// (with its tables' cells), the header, the footer.
pub(crate) fn order(doc: &FlowDocument) -> Vec<u32> {
    fn walk(blocks: &[Block], out: &mut Vec<u32>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.push(p.id),
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
    walk(&doc.body, &mut out);
    out
}

/// The kind of a character, for moving by words.
fn class(c: char) -> u8 {
    match c {
        c if c.is_whitespace() => 0,
        c if c.is_ascii_punctuation() || matches!(c, '、' | '。' | '，' | '．' | '「' | '」' | '（' | '）') => 1,
        '\u{3040}'..='\u{309f}' => 2,
        '\u{30a0}'..='\u{30ff}' => 3,
        c if c as u32 >= 0x3400 => 4,
        _ => 5,
    }
}

/// Model positions in a deletion remain stable for saving/undo, but are not
/// caret stops in the final view. A boundary shared by hidden text is represented
/// by the end of the hidden run, so Left and Right each cross a visible character.
fn hidden_range(p: &FlowParagraph, offset: usize) -> Option<(usize, usize)> {
    let mut at = 0;
    for r in &p.runs {
        let end = at + r.len();
        if at <= offset && offset < end {
            let is_del = !crate::docx::flow_layout::showing_markup()
                && matches!(r.revision.as_ref().map(|v| &v.kind), Some(ibis_doc_model::RevisionKind::Delete));
            let is_vanish = r.style.hidden && !crate::docx::flow_layout::showing_hidden_text();
            if is_del || is_vanish {
                return Some((at, end));
            }
        }
        at = end;
    }
    None
}

/// A position strictly inside a hidden or deleted run (not at its endpoints).
fn hidden_range_strictly_inside(p: &FlowParagraph, offset: usize) -> Option<(usize, usize)> {
    let mut at = 0;
    for r in &p.runs {
        let end = at + r.len();
        if at < offset && offset < end {
            let is_del = !crate::docx::flow_layout::showing_markup()
                && matches!(r.revision.as_ref().map(|v| &v.kind), Some(ibis_doc_model::RevisionKind::Delete));
            let is_vanish = r.style.hidden && !crate::docx::flow_layout::showing_hidden_text();
            if is_del || is_vanish {
                return Some((at, end));
            }
        }
        at = end;
    }
    None
}

/// In final view, deleted text cannot supply the caret's character format.
/// Prefer the preceding visible run, then following text, then the mark.
pub(super) fn visible_format_at(p: &FlowParagraph, offset: usize) -> (RunFormat, Option<String>, ibis_doc_model::FlowRunStyle) {
    if crate::docx::flow_layout::showing_markup() && crate::docx::flow_layout::showing_hidden_text() || offset > p.len() { return p.format_at(offset); }
    let mut at = 0;
    let mut previous: Option<&ibis_doc_model::FlowRun> = None;
    let value = |r: &ibis_doc_model::FlowRun| (r.format.clone(), r.char_style.clone(), r.style.clone());
    for run in &p.runs {
        let end = at + run.len();
        let is_del = !crate::docx::flow_layout::showing_markup() && matches!(run.revision.as_ref().map(|r| &r.kind), Some(ibis_doc_model::RevisionKind::Delete));
        let is_vanish = run.style.hidden && !crate::docx::flow_layout::showing_hidden_text();
        if run.len() > 0 && !is_del && !is_vanish {
            if offset == 0 || (offset > at && offset <= end) { return value(run); }
            if end <= offset { previous = Some(run); }
            else if offset <= at { return value(previous.unwrap_or(run)); }
        }
        at = end;
    }
    previous.map(value).unwrap_or_else(|| (p.mark_format.clone(), None, p.mark.clone()))
}

/// The insertion point after TrackDelete physically removes older insertions.
pub(super) fn after_tracked_delete(doc: &FlowDocument, from: FlowPos, to: FlowPos) -> FlowPos {
    let Some(p) = flow_command::paragraph(doc, to.para) else { return to };
    let first = if from.para == to.para { from.offset } else { 0 };
    let mut removed = 0;
    let mut offset = 0;
    for run in &p.runs {
        let end = offset + run.len();
        if matches!(run.revision.as_ref().map(|r| &r.kind), Some(ibis_doc_model::RevisionKind::Insert)) {
            removed += end.min(to.offset).saturating_sub(offset.max(first));
        }
        offset = end;
    }
    FlowPos::new(to.para, to.offset.saturating_sub(removed))
}

/// Visible pieces of a single-paragraph selection, in safe deletion order.
pub(super) fn visible_delete_commands(p: &FlowParagraph, from: usize, to: usize) -> Vec<FlowCommand> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (offset, _) in visible_chars(p).into_iter().filter(|(o, _)| from <= *o && *o < to) {
        if let Some(last) = ranges.last_mut().filter(|last| last.1 == offset) { last.1 += 1; }
        else { ranges.push((offset, offset + 1)); }
    }
    ranges.into_iter().rev().map(|(from, to)| FlowCommand::Delete { from: FlowPos::new(p.id, from), to: FlowPos::new(p.id, to) }).collect()
}

pub(super) fn visible_chars(p: &FlowParagraph) -> Vec<(usize, char)> {
    let mut ranges = Vec::new();
    if !crate::docx::flow_layout::showing_markup() {
        let mut at = 0;
        for r in &p.runs {
            let end = at + r.len();
            if matches!(r.revision.as_ref().map(|v| &v.kind), Some(ibis_doc_model::RevisionKind::Delete)) { ranges.push((at, end)); }
            at = end;
        }
    }
    let mut i = 0;
    p.text().chars().enumerate().filter(|(offset, _)| {
        while i < ranges.len() && ranges[i].1 <= *offset { i += 1; }
        i == ranges.len() || *offset < ranges[i].0
    }).collect()
}

fn selected_paragraph_text(p: &FlowParagraph, from: usize, to: usize) -> String {
    visible_chars(p).into_iter().filter(|(o, c)| from <= *o && *o < to && *c != '\u{fffc}')
        .map(|(_, c)| if matches!(c, '\u{b}' | '\u{c}' | '\u{e}') { '\n' } else { c }).collect()
}

impl SlideEditor {
    pub(crate) fn flow(&self) -> Option<&Arc<FlowDocument>> {
        self.presentation().flow.as_ref()
    }

    /// The caret and the other end of the selection.
    pub(crate) fn text_selection(&self) -> Option<(FlowPos, FlowPos)> {
        let s = self.doc_state()?;
        let caret = s.caret?;
        Some((s.anchor.unwrap_or(caret), caret))
    }

    /// The selection in document order (start, end), when there is a caret.
    pub(crate) fn ordered_selection(&self) -> Option<(FlowPos, FlowPos)> {
        let (a, b) = self.text_selection()?;
        let doc = self.flow()?;
        Some(if self.before(doc, b, a) { (b, a) } else { (a, b) })
    }

    fn before(&self, doc: &FlowDocument, a: FlowPos, b: FlowPos) -> bool {
        if a.para == b.para {
            return a.offset < b.offset;
        }
        let order = order(doc);
        let ia = order.iter().position(|&p| p == a.para);
        let ib = order.iter().position(|&p| p == b.para);
        ia < ib
    }

    pub(crate) fn has_selection(&self) -> bool {
        self.text_selection().is_some_and(|(a, b)| a != b)
    }

    /// Put the caret at `pos` (extending the selection if `extend`).
    pub(crate) fn set_caret(&mut self, pos: FlowPos, extend: bool, cx: &mut Context<Self>) {
        let pos = self.visible_position(pos);
        let active_comment_id = self.flow().and_then(|d| flow_command::comment_at_pos(d, pos)).map(|id| {
            self.flow().and_then(|d| d.comments.iter().find(|cm| cm.id == id).and_then(|cm| cm.parent)).unwrap_or(id)
        });
        let in_hf = self.flow().is_some_and(|d| {
            matches!(flow_command::locate(d, pos.para), Some((Container::Header | Container::Footer | Container::Part { .. }, _)))
        });
        let Some(state) = self.doc_state_mut() else { return };
        if extend {
            if state.anchor.is_none() {
                state.anchor = state.caret;
            }
        } else {
            state.anchor = None;
            state.extra_selections.clear();
        }
        state.caret = Some(pos);
        state.pending = None;
        state.shape_selected = false;
        state.selected_shapes.clear();
        if state.anchor == state.caret {
            state.anchor = None;
        }
        if let Some(c) = state.comments.as_mut() {
            c.active_comment_id = active_comment_id;
        }
        self.resources.hide_document_text = !self.show_document_text && in_hf;
        cx.notify();
    }

    /// Check whether a click on page `page` at `(x, y)` (points) hits a comment marker/bubble in the margin.
    pub(crate) fn comment_at_point(&self, page: usize, x: f32, y: f32) -> Option<u32> {
        let doc = self.flow()?;
        let lines = self.flow_lines();
        let page_w = doc.page.size.width as f32 / 12_700.0;
        let bubble_x = page_w + 8.0;
        if x < bubble_x - 10.0 || x > bubble_x + 35.0 {
            return None;
        }
        for c in &doc.comments {
            if c.parent.is_some() {
                continue;
            }
            let Some((from, to)) = flow_command::comment_range(doc, c.id) else { continue };
            for l in &lines {
                if l.page != page {
                    continue;
                }
                let in_range = if l.para < from.para || l.para > to.para {
                    false
                } else if from.para == to.para {
                    let min_off = l.stops.first().map_or(0, |s| s.0);
                    let max_off = l.stops.last().map_or(min_off, |s| s.0);
                    max_off >= from.offset && min_off <= to.offset
                } else if l.para == from.para {
                    l.stops.last().is_some_and(|s| s.0 >= from.offset)
                } else if l.para == to.para {
                    l.stops.first().is_some_and(|s| s.0 <= to.offset)
                } else {
                    true
                };
                if in_range && y >= l.y - 4.0 && y <= l.y + l.h + 4.0 {
                    return Some(c.id);
                }
            }
            if let Some((c_page, _cx, cy, ch)) = self.caret_box(&lines, to) {
                if c_page == page && y >= cy - 8.0 && y <= cy + ch + 8.0 {
                    return Some(c.id);
                }
            }
        }
        None
    }

    /// The lines of the laid-out pages.
    pub(crate) fn flow_lines(&self) -> Vec<LineGeom> {
        let Some(doc) = self.flow() else { return Vec::new() };
        let pagination = pages(doc, &self.resources);
        let mut out = Vec::new();
        for (page, pg) in pagination.pages.iter().enumerate() {
            for item in &pg.items {
                if let Item::Line { para, x, y, h, stops, vertical } = item {
                    out.push(LineGeom { page, para: *para, x: *x, y: *y, h: *h, stops: stops.clone(), vertical: *vertical });
                }
            }
        }
        out
    }

    /// The line the caret at `pos` is shown on.
    pub(crate) fn line_of(lines: &[LineGeom], pos: FlowPos) -> Option<&LineGeom> {
        let mine: Vec<&LineGeom> = lines.iter().filter(|l| l.para == pos.para).collect();
        let last = mine.len().saturating_sub(1);
        // Header and footer lines are on every page; the first will do.
        let per_page_last = |i: usize| i == last || mine.get(i + 1).is_some_and(|n| n.stops.first().is_some_and(|s| s.0 == 0));
        mine.iter()
            .enumerate()
            .find(|(i, l)| l.holds(pos.offset, per_page_last(*i)))
            .map(|(_, l)| *l)
            .or_else(|| mine.iter().rev().find(|l| l.stops.first().is_some_and(|s| s.0 <= pos.offset)).copied())
    }

    /// The caret's box: page, x, top, height (points).
    pub(crate) fn caret_box(&self, lines: &[LineGeom], pos: FlowPos) -> Option<(usize, f32, f32, f32)> {
        let line = Self::line_of(lines, pos)?;
        if line.vertical {
            Some((line.page, line.x, line.y_of(pos.offset), line.h))
        } else {
            Some((line.page, line.x_of(pos.offset), line.y, line.h))
        }
    }

    /// The position nearest a point of page `page` (points).
    pub(crate) fn position_at(&self, page: usize, x: f32, y: f32) -> Option<FlowPos> {
        let lines = self.flow_lines();
        let on_page: Vec<&LineGeom> = lines.iter().filter(|l| l.page == page).collect();

        // Horizontal line: if Y is within line height and X is beyond the rightmost character, return the end of line
        // unless another line in the same vertical band covers X or starts between this line and X (e.g. table cells / columns).
        let right_space_line = on_page.iter().filter(|l| {
            if l.vertical {
                false
            } else {
                let right = l.x + l.stops.last().map_or(0.0, |s| s.1);
                y >= l.y && y <= l.y + l.h && x > right && !on_page.iter().any(|o| {
                    if o.vertical || y < o.y || y > o.y + o.h {
                        false
                    } else {
                        let o_right = o.x + o.stops.last().map_or(0.0, |s| s.1);
                        (o.x <= x && x <= o_right) || (o.x > l.x && o.x <= x)
                    }
                })
            }
        }).min_by(|a, b| {
            let right_a = a.x + a.stops.last().map_or(0.0, |s| s.1);
            let right_b = b.x + b.stops.last().map_or(0.0, |s| s.1);
            (x - right_a).total_cmp(&(x - right_b))
        });

        if let Some(line) = right_space_line {
            if let Some(&(offset, _)) = line.stops.last() {
                return Some(FlowPos::new(line.para, offset));
            }
        }

        let bottom_space_line = on_page.iter().filter(|l| {
            if l.vertical {
                let bottom = l.y + l.stops.last().map_or(0.0, |s| s.1);
                x >= l.x && x <= l.x + l.h && y > bottom && !on_page.iter().any(|o| {
                    if !o.vertical || x < o.x || x > o.x + o.h {
                        false
                    } else {
                        let o_bottom = o.y + o.stops.last().map_or(0.0, |s| s.1);
                        (o.y <= y && y <= o_bottom) || (o.y > l.y && o.y <= y)
                    }
                })
            } else {
                false
            }
        }).min_by(|a, b| {
            let bottom_a = a.y + a.stops.last().map_or(0.0, |s| s.1);
            let bottom_b = b.y + b.stops.last().map_or(0.0, |s| s.1);
            (y - bottom_a).total_cmp(&(y - bottom_b))
        });

        if let Some(line) = bottom_space_line {
            if let Some(&(offset, _)) = line.stops.last() {
                return Some(FlowPos::new(line.para, offset));
            }
        }

        // A line under the point, else the nearest by height/width; among those
        // the one nearest sideways (table cells side by side, columns).
        let distance = |l: &LineGeom| {
            if l.vertical {
                let dx = if x < l.x { l.x - x } else if x > l.x + l.h { x - l.x - l.h } else { 0.0 };
                let bottom = l.y + l.stops.last().map_or(0.0, |s| s.1);
                let dy = if y < l.y { l.y - y } else if y > bottom { y - bottom } else { 0.0 };
                (dx, dy)
            } else {
                let dy = if y < l.y { l.y - y } else if y > l.y + l.h { y - l.y - l.h } else { 0.0 };
                let right = l.x + l.stops.last().map_or(0.0, |s| s.1);
                let dx = if x < l.x { l.x - x } else if x > right { x - right } else { 0.0 };
                (dy, dx)
            }
        };
        let line = on_page.iter().min_by(|a, b| {
            let (prim_a, sec_a) = distance(a);
            let (prim_b, sec_b) = distance(b);
            let da = prim_a * 4.0 + sec_a;
            let db = prim_b * 4.0 + sec_b;
            da.total_cmp(&db).then_with(|| {
                let off_a = if a.vertical {
                    (a.y_of(a.nearest(y)) - y).abs()
                } else {
                    (a.x_of(a.nearest(x)) - x).abs()
                };
                let off_b = if b.vertical {
                    (b.y_of(b.nearest(y)) - y).abs()
                } else {
                    (b.x_of(b.nearest(x)) - x).abs()
                };
                off_a.total_cmp(&off_b)
            })
        })?;
        let offset = if line.vertical { line.nearest(y) } else { line.nearest(x) };
        Some(FlowPos::new(line.para, offset))
    }

    pub(super) fn paragraph_of(&self, id: u32) -> Option<&FlowParagraph> {
        flow_command::paragraph(self.flow()?, id)
    }

    fn visible_position(&self, mut pos: FlowPos) -> FlowPos {
        if let Some(p) = self.paragraph_of(pos.para) {
            while let Some((_, end)) = hidden_range(p, pos.offset) { pos.offset = end; }
        }
        pos
    }

    fn selection_endpoint(&self, mut pos: FlowPos) -> FlowPos {
        if let Some(p) = self.paragraph_of(pos.para) {
            while let Some((_, end)) = hidden_range_strictly_inside(p, pos.offset) { pos.offset = end; }
        }
        pos
    }

    /// View changes and undo can leave either selection end inside deleted text.
    pub(super) fn normalize_visible_selection(&mut self) {
        let Some((anchor, caret)) = self.text_selection() else { return };
        if anchor == caret {
            let caret = self.visible_position(caret);
            if let Some(state) = self.doc_state_mut() {
                state.caret = Some(caret);
                state.anchor = None;
            }
        } else {
            let anchor = self.selection_endpoint(anchor);
            let caret = self.selection_endpoint(caret);
            if let Some(state) = self.doc_state_mut() {
                state.caret = Some(caret);
                state.anchor = (anchor != caret).then_some(anchor);
            }
        }
    }

    // ----- Moving ---------------------------------------------------------

    fn step(&self, pos: FlowPos, forward: bool) -> FlowPos {
        let pos = self.visible_position(pos);
        let Some(doc) = self.flow() else { return pos };
        let len = self.paragraph_of(pos.para).map_or(0, |p| p.len());
        // Marks not drawn (a comment's) are passed over with the next
        // position, as Word does.
        let mark = |o: usize| self.paragraph_of(pos.para).is_some_and(|p| p.is_mark(o));
        if forward && pos.offset < len {
            let mut o = pos.offset;
            while o < len {
                if let Some((_, end)) = self.paragraph_of(pos.para).and_then(|p| hidden_range(p, o)) { o = end; continue; }
                let m = mark(o);
                o += 1;
                if !m {
                    break;
                }
            }
            return FlowPos::new(pos.para, o);
        }
        if !forward && pos.offset > 0 {
            let mut o = pos.offset;
            let mut skipped_deletion = false;
            while o > 0 {
                o -= 1;
                if let Some((start, _)) = self.paragraph_of(pos.para).and_then(|p| hidden_range(p, o)) {
                    o = start;
                    skipped_deletion = true;
                    continue;
                }
                if !mark(o) { return FlowPos::new(pos.para, o); }
            }
            // Leading deleted text is the paragraph's visible start: move to
            // the previous paragraph instead of stopping at the same x again.
            if !skipped_deletion { return FlowPos::new(pos.para, o); }
        }
        let (container, index) = flow_command::locate(doc, pos.para).unwrap_or((Container::Body, 0));
        let list = if matches!(container, Container::Header | Container::Footer | Container::Part { .. }) {
            flow_command::blocks(doc, container).map(|b| b.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).collect()).unwrap_or_default()
        } else {
            order(doc)
        };
        let _ = index;
        let i = list.iter().position(|&p| p == pos.para).unwrap_or(0);
        if forward {
            match list.get(i + 1) {
                Some(&next) => FlowPos::new(next, 0),
                None => pos,
            }
        } else if i > 0 {
            let prev = list[i - 1];
            FlowPos::new(prev, self.paragraph_of(prev).map_or(0, |p| p.len()))
        } else {
            pos
        }
    }

    fn word_step(&self, pos: FlowPos, forward: bool) -> FlowPos {
        let Some(p) = self.paragraph_of(pos.para) else { return pos };
        let chars = visible_chars(p);
        let mut o = chars.partition_point(|(offset, _)| *offset < pos.offset);
        if forward {
            if o >= chars.len() { return self.step(pos, true); }
            let k = class(chars[o].1);
            while o < chars.len() && class(chars[o].1) == k { o += 1; }
            while o < chars.len() && class(chars[o].1) == 0 { o += 1; }
        } else {
            if o == 0 { return self.step(pos, false); }
            while o > 0 && class(chars[o - 1].1) == 0 { o -= 1; }
            if o > 0 {
                let k = class(chars[o - 1].1);
                while o > 0 && class(chars[o - 1].1) == k { o -= 1; }
            }
        }
        FlowPos::new(pos.para, chars.get(o).map_or(p.len(), |c| c.0))
    }

    /// The visible word around `pos` (a double click).
    pub(crate) fn word_at(&self, pos: FlowPos) -> (FlowPos, FlowPos) {
        let Some(p) = self.paragraph_of(pos.para) else { return (pos, pos) };
        let chars = visible_chars(p);
        if chars.is_empty() { let pos = self.visible_position(pos); return (pos, pos); }
        let at = chars.partition_point(|(offset, _)| *offset < pos.offset).min(chars.len() - 1);
        let k = class(chars[at].1);
        let (mut a, mut b) = (at, at);
        while a > 0 && class(chars[a - 1].1) == k { a -= 1; }
        while b < chars.len() && class(chars[b].1) == k { b += 1; }
        (FlowPos::new(pos.para, chars[a].0), FlowPos::new(pos.para, chars.get(b).map_or(p.len(), |c| c.0)))
    }

    fn vertical(&self, pos: FlowPos, down: bool) -> (FlowPos, f32) {
        let lines = self.flow_lines();
        let Some(current) = Self::line_of(&lines, pos) else { return (pos, 0.0) };
        let goal = self.doc_state().and_then(|s| s.goal_x).unwrap_or_else(|| {
            if current.vertical {
                current.y_of(pos.offset)
            } else {
                current.x_of(pos.offset)
            }
        });
        let index = lines.iter().position(|l| std::ptr::eq(l, current)).unwrap_or(0);
        // The next line of the body in reading order: the layout lists a
        // page's lines in order, headers and footers after the body.
        let body = order(self.flow().map(|d| d.as_ref()).unwrap_or(&FlowDocument::default()));
        let in_body = |l: &LineGeom| body.contains(&l.para);
        let same_kind = in_body(current);
        let candidates: Box<dyn Iterator<Item = &LineGeom>> = if down { Box::new(lines[index + 1..].iter()) } else { Box::new(lines[..index].iter().rev()) };
        for l in candidates {
            let diff = if current.vertical { (l.x - current.x).abs() } else { (l.y - current.y).abs() };
            if in_body(l) == same_kind && (l.page != current.page || diff > 0.5) {
                return (FlowPos::new(l.para, l.nearest(goal)), goal);
            }
        }
        (pos, goal)
    }

    fn line_edge(&self, pos: FlowPos, end: bool) -> FlowPos {
        let lines = self.flow_lines();
        let Some(line) = Self::line_of(&lines, pos) else { return pos };
        let stops = &line.stops;
        let len = self.paragraph_of(pos.para).map_or(0, |p| p.len());
        let offset = match (end, stops.last()) {
            (false, _) => stops.first().map_or(pos.offset, |s| s.0),
            // A wrapped line's end is before its last character (after it
            // is the next line's start).
            (true, Some(last)) if last.0 < len && stops.len() >= 2 => stops[stops.len() - 2].0,
            (true, Some(last)) => last.0,
            (true, None) => pos.offset,
        };
        FlowPos::new(pos.para, offset)
    }

    fn document_edge(&self, end: bool) -> Option<FlowPos> {
        let doc = self.flow()?;
        let order = order(doc);
        if end {
            let id = *order.last()?;
            Some(FlowPos::new(id, self.paragraph_of(id)?.len()))
        } else {
            Some(FlowPos::new(*order.first()?, 0))
        }
    }

    /// Move the caret (and the selection's end when `extend`).
    pub(crate) fn move_caret(&mut self, how: Move, extend: bool, cx: &mut Context<Self>) {
        let Some((anchor, caret)) = self.text_selection() else {
            if let Some(start) = self.document_edge(false) {
                self.set_caret(start, false, cx);
            }
            return;
        };
        let lines = self.flow_lines();
        let is_vert = Self::line_of(&lines, caret).is_some_and(|l| l.vertical);
        // Without Shift, a selection collapses to its side.
        if !extend && anchor != caret && matches!(how, Move::Left | Move::Right | Move::Up | Move::Down) {
            let (a, b) = self.ordered_selection().unwrap_or((caret, caret));
            let to = match (how, is_vert) {
                (Move::Left, false) | (Move::Up, true) => Some(a),
                (Move::Right, false) | (Move::Down, true) => Some(b),
                _ => None,
            };
            if let Some(to) = to {
                self.set_caret(to, false, cx);
                return;
            }
        }
        let mut goal = None;
        let to = match (how, is_vert) {
            (Move::Left, true) => {
                let (to, g) = self.vertical(caret, true);
                goal = Some(g);
                to
            }
            (Move::Right, true) => {
                let (to, g) = self.vertical(caret, false);
                goal = Some(g);
                to
            }
            (Move::Up, true) => self.step(caret, false),
            (Move::Down, true) => self.step(caret, true),
            (Move::Left, false) => self.step(caret, false),
            (Move::Right, false) => self.step(caret, true),
            (Move::WordLeft, _) => self.word_step(caret, false),
            (Move::WordRight, _) => self.word_step(caret, true),
            (Move::Up, false) | (Move::Down, false) => {
                let (to, g) = self.vertical(caret, how == Move::Down);
                goal = Some(g);
                to
            }
            (Move::Page(lines), _) => {
                // A screen on: the caret goes that many lines (the column kept), down or up.
                let mut at = caret;
                for _ in 0..lines.unsigned_abs() {
                    let (next, g) = self.vertical(at, lines > 0);
                    goal = Some(g);
                    if next == at {
                        break;
                    }
                    at = next;
                }
                at
            }
            (Move::LineStart, _) => self.line_edge(caret, false),
            (Move::LineEnd, _) => self.line_edge(caret, true),
            (Move::DocumentStart, _) => self.document_edge(false).unwrap_or(caret),
            (Move::DocumentEnd, _) => self.document_edge(true).unwrap_or(caret),
        };
        self.set_caret(to, extend, cx);
        if let Some(state) = self.doc_state_mut() {
            state.goal_x = goal;
        }
        self.scroll_to_caret(cx);
    }

    pub(crate) fn select_all(&mut self, cx: &mut Context<Self>) {
        let (Some(a), Some(b)) = (self.document_edge(false), self.document_edge(true)) else { return };
        self.set_caret(a, false, cx);
        self.set_caret(b, true, cx);
    }

    /// Scroll so the caret is in view.
    pub(crate) fn scroll_to_caret(&mut self, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let lines = self.flow_lines();
        let Some((page, _, y, h)) = self.caret_box(&lines, caret) else { return };
        self.scroll_into_view(page, y, h, cx);
    }

    // ----- Editing ----------------------------------------------------------

    pub(crate) fn run_flow(&mut self, commands: Vec<FlowCommand>, cx: &mut Context<Self>) -> bool {
        // Edits move the offsets the other ranges of a selection hold.
        if let Some(state) = self.doc_state_mut() {
            state.extra_selections.clear();
        }
        fn is_comment_command(cmd: &FlowCommand) -> bool {
            match cmd {
                FlowCommand::SetComments(_) => true,
                FlowCommand::InsertField { kind, .. } => {
                    matches!(kind, FlowRunKind::CommentStart(_) | FlowRunKind::CommentEnd(_) | FlowRunKind::CommentRef(_))
                }
                FlowCommand::ReplaceBlocks { remove: 1, blocks, .. } => {
                    blocks.iter().all(|b| matches!(b, Block::Paragraph(_)))
                }
                FlowCommand::Many(subcmds) => subcmds.iter().all(is_comment_command),
                _ => false,
            }
        }

        fn is_formatting_command(cmd: &FlowCommand) -> bool {
            match cmd {
                FlowCommand::SetRunFormat { .. }
                | FlowCommand::SetCharStyle { .. }
                | FlowCommand::SetStyle { .. }
                | FlowCommand::SetDocumentParagraphSpacing { .. }
                | FlowCommand::SetNoteStyle { .. }
                | FlowCommand::TrackFormat { .. } => true,
                FlowCommand::SetParagraphFormat { change, .. } => {
                    !matches!(change, ParaChange::AddBookmark(_) | ParaChange::RemoveBookmark(_))
                }
                FlowCommand::Many(subcmds) => subcmds.iter().any(is_formatting_command),
                _ => false,
            }
        }

        let is_admin = commands.iter().all(|c| {
            matches!(
                c,
                FlowCommand::SetMarkAsFinal(_)
                    | FlowCommand::SetReadOnlyRecommended(_)
                    | FlowCommand::SetLockTracking { .. }
                    | FlowCommand::SetViewMode(_)
                    | FlowCommand::SetInteractionMode(_)
                    | FlowCommand::SetDocumentProtection(_)
                    | FlowCommand::SetFormattingProtection(_)
            )
        });
        if !is_admin {
            if self.is_mark_as_final() {
                log::warn!("edit rejected: document is marked as final");
                return false;
            }
            if self.is_read_only_recommended() && !self.is_read_only_dismissed() {
                log::warn!("edit rejected: document is read-only recommended");
                return false;
            }
            if self.is_read_mode() || self.is_viewing_mode() {
                log::warn!("edit rejected: document is in viewing / read mode");
                return false;
            }
            if self.is_protection_read_only() {
                log::warn!("edit rejected: document is protected as read-only");
                return false;
            }
            if self.is_protection_comments() {
                let is_comment_cmd = commands.iter().all(is_comment_command);
                if !is_comment_cmd {
                    log::warn!("edit rejected: document is protected for comments only");
                    return false;
                }
            }
            if self.is_protection_forms() {
                let is_form_cmd = commands.iter().all(|c| {
                    matches!(
                        c,
                        FlowCommand::ToggleCheckBox { .. }
                            | FlowCommand::SelectDropDownItem { .. }
                            | FlowCommand::SetDatePickerDate { .. }
                            | FlowCommand::SetContentControlText { .. }
                            | FlowCommand::SetFormFieldText { .. }
                            | FlowCommand::SetFormFieldMeta { .. }
                    )
                });
                if !is_form_cmd {
                    log::warn!("edit rejected: document is protected for forms");
                    return false;
                }
            }
            if self.is_formatting_protected() && commands.iter().any(is_formatting_command) {
                log::warn!("edit rejected: document formatting is protected");
                return false;
            }
        }
        let started = crate::debug_timing::start();
        let before = self.flow().map(|d| marked(d));
        let redefine = self.auto_redefine_targets(&commands);
        let result = self.document.execute(commands.into_iter().map(Command::Flow).collect());
        crate::debug_timing::report("edit", started);
        if let Err(err) = &result {
            log::warn!("edit rejected: {err}");
        }
        if result.is_ok()
            && let Some(before) = before
        {
            self.remove_orphans(&before);
        }
        if result.is_ok() && !redefine.is_empty() {
            self.auto_redefine_styles(&redefine);
        }
        if result.is_ok() {
            self.sync_document_peers(cx);
        }
        cx.notify();
        result.is_ok()
    }

    /// スタイルの自動更新 (`w:autoRedefine`): the paragraphs whose formatting `commands` change by
    /// hand (a paragraph's formatting, or a character formatting over the paragraph's whole text).
    fn auto_redefine_targets(&self, commands: &[FlowCommand]) -> Vec<u32> {
        let Some(doc) = self.flow() else { return Vec::new() };
        if !doc.styles.styles.values().any(|s| s.auto_update) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for command in commands {
            match command {
                FlowCommand::SetRunFormat { from, to, .. } if from.para == to.para && from.offset == 0 && flow_command::paragraph(doc, from.para).is_some_and(|p| p.len() == to.offset && to.offset > 0) => out.push(from.para),
                FlowCommand::SetParagraphFormat { first, last, change } if !matches!(change, ParaChange::Style(_) | ParaChange::List(_) | ParaChange::Level(_) | ParaChange::AddBookmark(_) | ParaChange::RemoveBookmark(_) | ParaChange::SectionBreak(_)) => {
                    let order = order(doc);
                    if let (Some(a), Some(b)) = (order.iter().position(|p| p == first), order.iter().position(|p| p == last)) {
                        out.extend(order[a.min(b)..=a.max(b)].iter().copied());
                    }
                }
                _ => {}
            }
        }
        out.sort_unstable();
        out.dedup();
        out.retain(|&id| flow_command::paragraph(doc, id).and_then(|p| p.style.clone().or_else(|| doc.styles.default_paragraph.clone())).and_then(|s| doc.styles.styles.get(&s)).is_some_and(|s| s.auto_update));
        out
    }

    /// The styles of `paragraphs` take what was changed by hand (in the step of the edit).
    fn auto_redefine_styles(&mut self, paragraphs: &[u32]) {
        for &id in paragraphs {
            let Some(doc) = self.flow() else { return };
            let Some((style_id, style, paragraph)) = flow_command::style_matching(doc, id, 0) else { continue };
            let Some((container, index)) = flow_command::locate(doc, id) else { continue };
            let commands = vec![Command::Flow(FlowCommand::SetStyle { id: style_id, style: Some(style) }), Command::Flow(FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(paragraph)] })];
            if self.document.amend_last(commands).is_err() {
                return;
            }
        }
    }

    /// Notes and comments whose marks the edit removed go with them (as
    /// Word does), in the same step of undo. Only those marked before the
    /// edit: what a file has unmarked stays.
    fn remove_orphans(&mut self, before: &Marked) {
        for _ in 0..8 {
            let Some(doc) = self.flow() else { return };
            let now = marked(doc);
            let lost: Vec<u32> = before.comments.difference(&now.comments).copied().collect();
            let Some(&id) = lost.iter().find(|i| doc.comments.iter().any(|c| c.id == **i)) else { break };
            let Some(command) = flow_command::delete_comment(doc, id) else { break };
            if self.document.amend_last(vec![Command::Flow(command)]).is_err() {
                break;
            }
        }
        let Some(doc) = self.flow() else { return };
        let now = marked(doc);
        let lost: std::collections::HashSet<(bool, u32)> = before.notes.difference(&now.notes).copied().collect();
        if lost.is_empty() {
            return;
        }
        let footnotes: Vec<_> = doc.footnotes.iter().filter(|n| !lost.contains(&(false, n.id))).cloned().collect();
        let endnotes: Vec<_> = doc.endnotes.iter().filter(|n| !lost.contains(&(true, n.id))).cloned().collect();
        let _ = self.document.amend_last(vec![Command::Flow(FlowCommand::SetNotes { footnotes, endnotes })]);
    }

    /// 挿入 > 脚注 (文末脚注): a note at the caret, the caret into its text.
    pub(crate) fn insert_note(&mut self, endnote: bool, cx: &mut Context<Self>) {
        let Some((_, end)) = self.ordered_selection() else { return };
        let Some((commands, para)) = self.flow().and_then(|d| flow_command::insert_note(d, end, endnote)) else { return };
        if self.run_flow(commands, cx)
            && let Some(len) = self.flow().and_then(|d| flow_command::paragraph(d, para)).map(|p| p.len())
        {
            self.set_caret(FlowPos::new(para, len), false, cx);
            self.scroll_to_caret(cx);
        }
    }

    /// 脚注を文末脚注に変換 (or back).
    pub(crate) fn convert_notes(&mut self, to_endnotes: bool, cx: &mut Context<Self>) {
        let Some(command) = self.flow().and_then(|d| flow_command::convert_notes(d, to_endnotes)) else { return };
        // The caret leaves a note that moves.
        let in_note = self.text_selection().and_then(|(_, c)| self.flow().and_then(|d| flow_command::locate(d, c.para))).is_some_and(|l| matches!(l.0, Container::Note { .. }));
        if self.run_flow(vec![command], cx)
            && in_note
            && let Some(start) = self.document_edge(false)
        {
            self.set_caret(start, false, cx);
        }
    }

    /// A broad paragraph deletion would also remove invisible source scopes,
    /// or move them into a different paragraph without their source XML.
    /// Keep the document and redo history intact until that writeback exists.
    fn reject_hidden_history_range(&mut self, start: FlowPos, end: FlowPos, cx: &mut Context<Self>) -> bool {
        if start.para == end.para || super::flow_layout::showing_markup() { return false; }
        let Some(doc) = self.flow() else { return false; };
        if doc.track_changes { return false; }
        let (Some((container, first)), Some((other, last))) = (flow_command::locate(doc, start.para), flow_command::locate(doc, end.para)) else { return false; };
        if container != other || first > last { return false; }
        fn deleted(block: &Block) -> bool {
            match block {
                Block::Paragraph(p) => p.runs.iter().any(|r| matches!(r.revision.as_ref().map(|v| &v.kind), Some(ibis_doc_model::RevisionKind::Delete))),
                Block::Table(t) => t.rows.iter().any(|row| row.cells.iter().any(|cell| cell.blocks.iter().any(deleted))),
            }
        }
        if !flow_command::blocks(doc,container).is_some_and(|blocks| blocks[first..=last].iter().any(deleted)) { return false; }
        self.status = Some(crate::shell::i18n::text("docx.editing.replacing-deleting-across-multiple").into());
        cx.notify();
        true
    }

    /// Replace the selection (or insert at the caret) with `text`: `\n`
    /// ends a paragraph, `\t` is a tab, `\u{b}` a line break.
    /// ツール > オートコレクト: the replacements as typed on or off.
    pub(crate) fn toggle_autocorrect(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.autocorrect_off = !state.autocorrect_off;
        }
        cx.notify();
    }

    /// ツール > オートコレクトのオプション…: the panel shared with the workbook's (its settings are the same).
    pub(crate) fn open_doc_autocorrect_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let panel = crate::shell::autocorrect::AutoCorrectPanel::new(window, cx);
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.ruby = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.text_box = None;
            state.merge = None;
            state.table_size = None;
            state.comments = None;
            state.control = None;
            state.autocorrect = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The docx panels shown by the editor's own state go (a shared panel is about to be shown).
    fn close_doc_panels(&mut self) {
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.ruby = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.text_box = None;
            state.merge = None;
            state.table_size = None;
            state.comments = None;
            state.control = None;
            state.autocorrect = None;
            state.helper = Default::default();
        }
    }

    /// ツール > 自動回復の設定… (DOCX-FILE-014).
    pub(crate) fn open_doc_autorecover_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.close_doc_panels();
        self.open_autorecover_settings(window, cx);
    }

    /// ファイル > 未保存のファイルの回復… (DOCX-FILE-015).
    pub(crate) fn open_doc_recover_unsaved(&mut self, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.close_doc_panels();
        self.open_recover_unsaved(cx);
    }

    /// 自動回復用データの保存間隔（分、ECMA-376 Part 1 §17.15.1.72 w:saveFrequency、DOCX-OPTIONS-012）。
    pub(crate) fn save_frequency(&self) -> Option<u32> {
        self.flow().and_then(|f| f.save_frequency)
    }

    /// 自動回復用データの保存間隔を設定（DOCX-OPTIONS-012）。
    pub(crate) fn set_save_frequency(&mut self, minutes: Option<u32>, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetSaveFrequency(minutes)], cx);
    }

    /// この文書で自動回復が無効化されているか（DOCX-OPTIONS-012）。
    pub(crate) fn is_autorecover_disabled_for_doc(&self) -> bool {
        if let Some(f) = self.save_frequency() {
            f == 0
        } else {
            self.autorecover_off
        }
    }

    /// この文書の自動回復の有効／無効をトグル（DOCX-OPTIONS-012）。
    pub(crate) fn toggle_autorecover_disabled_for_doc(&mut self, cx: &mut Context<Self>) {
        let cur = self.is_autorecover_disabled_for_doc();
        let new_disabled = !cur;
        let new_freq = if new_disabled { Some(0) } else { None };
        self.set_save_frequency(new_freq, cx);
    }

    // --- Font embedding & compatibility options (Word Options, DOCX-OPTIONS-013) ---

    /// ファイルにフォントを埋め込む（DOCX-OPTIONS-013）。
    pub(crate) fn embed_true_type_fonts(&self) -> bool {
        self.flow().is_some_and(|f| f.embed_true_type_fonts)
    }

    pub(crate) fn is_embed_true_type_fonts(&self) -> bool {
        self.embed_true_type_fonts()
    }

    pub(crate) fn set_embed_true_type_fonts(&mut self, embed: bool, cx: &mut Context<Self>) {
        let (sys, sub) = (self.embed_system_fonts(), self.save_subset_fonts());
        self.set_font_embedding(embed, sys, sub, cx);
    }

    pub(crate) fn toggle_embed_true_type_fonts(&mut self, cx: &mut Context<Self>) {
        let cur = self.embed_true_type_fonts();
        self.set_embed_true_type_fonts(!cur, cx);
    }

    /// 標準のシステム フォントを埋め込む（DOCX-OPTIONS-013）。
    pub(crate) fn embed_system_fonts(&self) -> bool {
        self.flow().is_some_and(|f| f.embed_system_fonts)
    }

    pub(crate) fn is_embed_system_fonts(&self) -> bool {
        self.embed_system_fonts()
    }

    pub(crate) fn set_embed_system_fonts(&mut self, embed: bool, cx: &mut Context<Self>) {
        let (tt, sub) = (self.embed_true_type_fonts(), self.save_subset_fonts());
        self.set_font_embedding(tt, embed, sub, cx);
    }

    pub(crate) fn toggle_embed_system_fonts(&mut self, cx: &mut Context<Self>) {
        let cur = self.embed_system_fonts();
        self.set_embed_system_fonts(!cur, cx);
    }

    /// 文書で使用されている文字だけを埋め込む（DOCX-OPTIONS-013）。
    pub(crate) fn save_subset_fonts(&self) -> bool {
        self.flow().is_some_and(|f| f.save_subset_fonts)
    }

    pub(crate) fn is_save_subset_fonts(&self) -> bool {
        self.save_subset_fonts()
    }

    pub(crate) fn set_save_subset_fonts(&mut self, subset: bool, cx: &mut Context<Self>) {
        let (tt, sys) = (self.embed_true_type_fonts(), self.embed_system_fonts());
        self.set_font_embedding(tt, sys, subset, cx);
    }

    pub(crate) fn toggle_save_subset_fonts(&mut self, cx: &mut Context<Self>) {
        let cur = self.save_subset_fonts();
        self.set_save_subset_fonts(!cur, cx);
    }

    pub(crate) fn set_font_embedding(&mut self, embed_true_type_fonts: bool, embed_system_fonts: bool, save_subset_fonts: bool, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetFontEmbedding {
            embed_true_type_fonts,
            embed_system_fonts,
            save_subset_fonts,
        }], cx);
    }

    /// 互換性モード（DOCX-OPTIONS-013, DOCX-FILE-011）。
    pub(crate) fn compatibility_mode(&self) -> Option<u32> {
        self.flow().and_then(|f| f.compatibility_mode)
    }

    /// 文書が互換モード（Word 2003=11, 2007=12, 2010=14 など < 15）かどうか（DOCX-FILE-011）。
    pub(crate) fn is_docx_compatibility_mode(&self) -> bool {
        self.flow().is_some_and(|f| f.is_compatibility_mode())
    }

    /// 現在の互換モードの形式名（"Word 2003", "Word 2007", "Word 2010", "Word 2013-2024"）。
    pub(crate) fn docx_compatibility_mode_name(&self) -> &'static str {
        self.flow().map_or("Word 2013-2024", |f| f.compatibility_mode_version_name())
    }

    /// 互換モードの文書を現行形式（Word 2013-2024, compatibilityMode=15）に変換する（DOCX-FILE-011）。
    pub(crate) fn convert_docx_to_current_format(&mut self, cx: &mut Context<Self>) {
        let (fe, shift) = (self.use_fe_layout(), self.do_not_expand_shift_return());
        self.set_compatibility_options(Some(15), fe, shift, cx);
        let msg = crate::shell::i18n::text("docx.file.convert.done");
        if let Some(panel) = self.doc_state_mut().and_then(|s| s.compatibility_panel.as_mut()) {
            panel.message = Some(msg.to_string());
        }
        self.status = Some(msg);
        cx.notify();
    }

    pub(crate) fn set_compatibility_mode(&mut self, mode: Option<u32>, cx: &mut Context<Self>) {
        let (fe, shift) = (self.use_fe_layout(), self.do_not_expand_shift_return());
        self.set_compatibility_options(mode, fe, shift, cx);
    }

    /// 東アジアのレイアウト規則を使用する（DOCX-OPTIONS-013）。
    pub(crate) fn use_fe_layout(&self) -> bool {
        self.flow().is_some_and(|f| f.use_fe_layout)
    }

    pub(crate) fn is_use_fe_layout(&self) -> bool {
        self.use_fe_layout()
    }

    pub(crate) fn set_use_fe_layout(&mut self, use_fe: bool, cx: &mut Context<Self>) {
        let (mode, shift) = (self.compatibility_mode(), self.do_not_expand_shift_return());
        self.set_compatibility_options(mode, use_fe, shift, cx);
    }

    pub(crate) fn toggle_use_fe_layout(&mut self, cx: &mut Context<Self>) {
        let cur = self.use_fe_layout();
        self.set_use_fe_layout(!cur, cx);
    }

    /// Shift+Return で終わる行の文字間隔を広げない（DOCX-OPTIONS-013）。
    pub(crate) fn do_not_expand_shift_return(&self) -> bool {
        self.flow().is_some_and(|f| f.do_not_expand_shift_return)
    }

    pub(crate) fn is_do_not_expand_shift_return(&self) -> bool {
        self.do_not_expand_shift_return()
    }

    pub(crate) fn set_do_not_expand_shift_return(&mut self, do_not_expand: bool, cx: &mut Context<Self>) {
        let (mode, fe) = (self.compatibility_mode(), self.use_fe_layout());
        self.set_compatibility_options(mode, fe, do_not_expand, cx);
    }

    pub(crate) fn toggle_do_not_expand_shift_return(&mut self, cx: &mut Context<Self>) {
        let cur = self.do_not_expand_shift_return();
        self.set_do_not_expand_shift_return(!cur, cx);
    }

    pub(crate) fn set_compatibility_options(&mut self, compatibility_mode: Option<u32>, use_fe_layout: bool, do_not_expand_shift_return: bool, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetCompatibilityOptions {
            compatibility_mode,
            use_fe_layout,
            do_not_expand_shift_return,
        }], cx);
    }

    pub(crate) fn doc_autocorrect_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        self.doc_state()?.autocorrect.as_ref()?;
        crate::shell::autocorrect::render_panel(
            self,
            crate::shell::autocorrect::PanelAccess {
                get: |e| e.doc_state_mut().and_then(|s| s.autocorrect.as_mut()),
                apply: |e, cx| {
                    if let Some(p) = e.doc_state_mut().and_then(|s| s.autocorrect.take()) {
                        crate::shell::autocorrect::set_settings(p.working);
                    }
                    cx.notify();
                },
                close: |e| {
                    if let Some(s) = e.doc_state_mut() {
                        s.autocorrect = None;
                    }
                },
            },
            cx,
        )
    }

    pub(crate) fn type_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.shape_selected && s.box_edit.is_none())
            && self.text_box_at_caret().is_some_and(|i| i.has_text)
        {
            self.mutate_active_shape_text(
                |body, _| {
                    let at = body.text().chars().count();
                    body.splice(at, 0, text);
                },
                cx,
            );
            return;
        }
        if text == " " && self.toggle_checkbox_around_caret(cx) {
            return;
        }
        if self.has_extra_selections() && !text.contains('\n') && !text.contains('\r') && self.flow().is_some_and(|d| !d.track_changes) {
            return self.type_into_ranges(text, cx);
        }
        let Some((start, mut end)) = self.ordered_selection() else { return };
        if self.reject_hidden_history_range(start, end, cx) { return; }
        let Some(doc) = self.flow() else { return };
        if start == end && self.is_overtype() && !text.contains('\n') && !text.contains('\r') {
            if let Some(p) = flow_command::paragraph(doc, start.para) {
                let remaining = p.len().saturating_sub(start.offset);
                let overwrite_count = text.chars().count().min(remaining);
                if overwrite_count > 0 {
                    end = FlowPos::new(start.para, start.offset + overwrite_count);
                }
            }
        }
        let mut commands = Vec::new();
        let track = doc.track_changes;
        let who = crate::docx::comments::comment_author();
        // With changes tracked, the selection is marked deleted and the text
        // goes after it.
        let at = if track && start != end { after_tracked_delete(doc, start, end) } else { start };
        if start != end {
            if track {
                commands.push(FlowCommand::TrackDelete { from: start, to: end, author: who.author.clone(), date: who.date.clone() });
            } else if start.para == end.para && !super::flow_layout::showing_markup() {
                if let Some(p) = flow_command::paragraph(doc, start.para) { commands.extend(visible_delete_commands(p, start.offset, end.offset)); }
            } else {
                commands.push(FlowCommand::Delete { from: start, to: end });
            }
        }
        let original_start = start;
        let replacing = start != end;
        let start = at;
        // The formatting typed: a button pressed with no selection, else the
        // text before the caret (what the selection started with).
        let format = self.doc_state().and_then(|s| s.pending.clone()).or_else(|| {
            if !replacing && super::flow_layout::showing_markup() { return None; }
            let p = flow_command::paragraph(doc, original_start.para)?;
            let offset = if replacing { original_start.offset + 1 } else { original_start.offset };
            let (format, char_style, _) = visible_format_at(p, offset);
            Some(InsertFormat { format, char_style })
        });
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let breaks = text.matches('\n').count();
        let first_new = doc.new_id();
        let last_segment = text.rsplit('\n').next().unwrap_or("");
        let typed = last_segment.chars().count();
        // Typed over a content control's prompt, the text is the control's from its start.
        let prompt_start = (!replacing && breaks == 0 && !doc.form_design_mode).then(|| {
            let p = flow_command::paragraph(doc, start.para)?;
            let (run, at) = p.control_at(start.offset)?;
            let is_prompt = match &p.runs[run].kind {
                FlowRunKind::RichText { placeholder: true, .. }
                | FlowRunKind::PlainText { placeholder: true, .. }
                | FlowRunKind::BuildingBlockGallery { placeholder: true, .. } => true,
                FlowRunKind::RepeatingSection { items, selected, .. } => {
                    items.get(*selected).map_or(false, |s| s == ibis_doc_model::DEFAULT_REPEATING_SECTION_ITEM_PROMPT)
                }
                _ => false,
            };
            is_prompt.then_some(at)
        }).flatten();
        // 自動の箇条書き・段落番号: a space typed after `*`, `-`, `1.`… that is all the paragraph has so far makes the paragraph a list item.
        if !replacing && !track && breaks == 0 && text == " " && prompt_start.is_none() && !self.doc_state().is_some_and(|s| s.autocorrect_off) && crate::shell::autocorrect::settings().auto_lists {
            let marker = flow_command::paragraph(doc, start.para).filter(|p| p.list.is_none() && p.len() == start.offset && p.runs.iter().all(|r| matches!(r.kind, FlowRunKind::Text(_)))).map(|p| p.text());
            if let Some(kind) = marker.as_deref().and_then(crate::shell::autocorrect::auto_list_marker) {
                let bullet = kind == crate::shell::autocorrect::AutoList::Bullet;
                let numbering = &doc.styles.numbering;
                let mut commands = vec![FlowCommand::Delete { from: FlowPos::new(start.para, 0), to: start }];
                let id = match numbering.find(bullet) {
                    Some(id) => id,
                    None => {
                        let (with, id) = numbering.with_new_list(bullet);
                        commands.push(FlowCommand::SetNumbering { numbering: with });
                        id
                    }
                };
                commands.push(FlowCommand::SetParagraphFormat { first: start.para, last: start.para, change: ParaChange::List(Some(id)) });
                if self.run_flow(commands, cx) {
                    self.set_caret(FlowPos::new(start.para, 0), false, cx);
                }
                return;
            }
        }
        // オートコレクト (DOCX-OPTIONS-007): a typed character that ends one of the replaced
        // strings ((c) → ©, --> → →…) replaces it, in the same step.
        let replacement = (!replacing && !track && breaks == 0 && typed == 1 && prompt_start.is_none() && !self.doc_state().is_some_and(|s| s.autocorrect_off)).then(|| {
            let p = flow_command::paragraph(doc, start.para)?;
            let before: String = p.text().chars().take(start.offset).collect();
            crate::shell::autocorrect::settings().correct_typing(&before, text.chars().next()?)
        }).flatten();
        if let Some((length, ref to)) = replacement {
            commands.push(FlowCommand::InsertText { at: start, text: text.clone(), format: format.clone() });
            commands.push(FlowCommand::Delete { from: FlowPos::new(start.para, start.offset + 1 - length), to: FlowPos::new(start.para, start.offset + 1) });
            commands.push(FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + 1 - length), text: to.clone(), format: format.clone() });
        } else {
            commands.push(FlowCommand::InsertText { at: start, text: text.clone(), format: format.clone() });
        }
        if track && breaks == 0 && typed > 0 {
            let revision = ibis_doc_model::Revision { kind: ibis_doc_model::RevisionKind::Insert, author: who.author.clone(), date: who.date.clone() };
            commands.push(FlowCommand::MarkRevision { from: start, to: FlowPos::new(start.para, start.offset + typed), revision: Some(revision) });
        }
        if self.run_flow(commands, cx) {
            let caret = if let Some((length, ref to)) = replacement { FlowPos::new(start.para, start.offset + 1 - length + to.chars().count()) } else if breaks == 0 { FlowPos::new(start.para, prompt_start.unwrap_or(start.offset) + typed) } else { FlowPos::new(first_new + breaks as u32 - 1, typed) };
            self.set_caret(caret, false, cx);
            if breaks > 0 {
                if let Some(state) = self.doc_state_mut() {
                    state.pending = format;
                }
            }
            self.scroll_to_caret(cx);
        }
    }

    /// Text typed (or pasted as text, in a line) with a selection of several ranges: it takes the place of each range, as one step.
    /// The caret goes after the text of the first range.
    fn type_into_ranges(&mut self, text: &str, cx: &mut Context<Self>) {
        // The ranges and the caret (an empty range too: the carets left after the first typed text).
        let mut ranges = self.doc_state().map(|s| s.extra_selections.clone()).unwrap_or_default();
        if let Some(own) = self.ordered_selection() {
            ranges.push(own);
        }
        if let Some(doc) = self.flow() {
            let order = order(doc);
            let key = |p: FlowPos| (order.iter().position(|&id| id == p.para).unwrap_or(usize::MAX), p.offset);
            ranges.sort_by_key(|&(a, _)| key(a));
        }
        if ranges.iter().any(|&(a, b)| self.reject_hidden_history_range(a, b, cx)) {
            return;
        }
        let Some(doc) = self.flow() else { return };
        let pending = self.doc_state().and_then(|s| s.pending.clone());
        let typed = text.chars().count();
        let mut commands = Vec::new();
        // From the last range up, so that each leaves the offsets of those before it.
        for &(from, to) in ranges.iter().rev() {
            let format = pending.clone().or_else(|| {
                let p = flow_command::paragraph(doc, from.para)?;
                let (format, char_style, _) = visible_format_at(p, from.offset + 1);
                Some(InsertFormat { format, char_style })
            });
            if from != to {
                commands.push(FlowCommand::Delete { from, to });
            }
            commands.push(FlowCommand::InsertText { at: from, text: text.to_string(), format });
        }
        // Where the caret of each range is after its text, in the document as it is then (an edit earlier in the same paragraph moves the
        // offsets after it); None when a range ran over paragraphs.
        let mut shift: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
        let mut carets: Option<Vec<FlowPos>> = Some(Vec::new());
        for &(from, to) in &ranges {
            if from.para != to.para {
                carets = None;
                break;
            }
            let moved = shift.entry(from.para).or_insert(0);
            if let Some(list) = carets.as_mut() {
                list.push(FlowPos::new(from.para, (from.offset as i64 + *moved) as usize + typed));
            }
            *moved += typed as i64 - (to.offset as i64 - from.offset as i64);
        }
        let first = ranges[0].0;
        if self.run_flow(commands, cx) {
            self.set_caret(FlowPos::new(first.para, first.offset + typed), false, cx);
            // The other ranges stay as carets after their texts: what is typed next goes to each.
            if let Some(carets) = carets
                && let Some(state) = self.doc_state_mut()
            {
                state.extra_selections = carets.into_iter().skip(1).map(|c| (c, c)).collect();
            }
            self.scroll_to_caret(cx);
        }
    }

    /// The caret's paragraph when it is a list item and the caret is at its
    /// start with nothing selected (where Tab, Enter and Backspace act on
    /// the list, as in Word).
    fn list_item_at_start(&self) -> Option<&FlowParagraph> {
        let (start, end) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        (start == end && start.offset == 0 && p.list.is_some()).then_some(p)
    }

    /// Tab: a list item's level down at its start, else a tab.
    pub(crate) fn tab_key(&mut self, cx: &mut Context<Self>) {
        // In a table: to the next cell (a new row after the last), as Word.
        if let Some((table, row, cell)) = self.table_at_caret() {
            let next = |e: &Self| -> Option<Option<u32>> {
                let doc = e.flow()?;
                let t = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table))?;
                let (r, c) = if cell + 1 < t.rows[row].cells.len() { (row, cell + 1) } else { (row + 1, 0) };
                Some(t.rows.get(r).and_then(|x| x.cells.get(c)).and_then(|c| c.blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None })))
            };
            match next(self) {
                Some(Some(id)) => self.set_caret(FlowPos::new(id, 0), false, cx),
                Some(None) => {
                    self.edit_table(|r, _, _| ibis_doc_model::TableEdit::InsertRow { row: r, below: true }, cx);
                    if let Some(Some(id)) = next(self) {
                        self.set_caret(FlowPos::new(id, 0), false, cx);
                    }
                }
                None => {}
            }
            return;
        }
        if let Some(level) = self.list_item_at_start().and_then(|p| p.list).map(|l| l.level) {
            self.apply_para_change(ParaChange::Level((level + 1).min(8)), cx);
        } else {
            self.type_text("\t", cx);
        }
    }

    pub(crate) fn outdent_list(&mut self, cx: &mut Context<Self>) {
        // In a table: back to the previous cell (the last of the row above from the first), as Word.
        if let Some((table, row, cell)) = self.table_at_caret() {
            let previous = self.flow().and_then(|doc| {
                let t = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table))?;
                let (r, c) = if cell > 0 { (row, cell - 1) } else if row > 0 { (row - 1, t.rows[row - 1].cells.len().checked_sub(1)?) } else { return None };
                t.rows.get(r)?.cells.get(c)?.blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None })
            });
            if let Some(id) = previous {
                self.set_caret(FlowPos::new(id, 0), false, cx);
            }
            return;
        }
        let Some((start, _)) = self.ordered_selection() else { return };
        match self.paragraph_of(start.para).and_then(|p| p.list) {
            Some(l) if l.level > 0 => self.apply_para_change(ParaChange::Level(l.level - 1), cx),
            Some(_) => self.apply_para_change(ParaChange::List(None), cx),
            None => {}
        }
    }

    /// Enter: an empty list item stops being one; else a new paragraph.
    pub(crate) fn enter_key(&mut self, cx: &mut Context<Self>) {
        if self.auto_border_on_enter(cx) {
            return;
        }
        if self.list_item_at_start().is_some_and(|p| p.is_empty()) {
            self.apply_para_change(ParaChange::List(None), cx);
        } else {
            self.type_text("\n", cx);
        }
    }

    /// 自動の罫線: Enter at the end of a paragraph that is only `---`, `___`, `===`, `***`, `~~~` or `###` (three or more) takes the
    /// characters away and gives the paragraph the border below it, and a new paragraph follows (as one step). Whether it was done.
    fn auto_border_on_enter(&mut self, cx: &mut Context<Self>) -> bool {
        use crate::shell::autocorrect::AutoBorder;
        use ibis_doc_model::{BorderStyle, FlowBorder};
        let Some((start, end)) = self.ordered_selection() else { return false };
        if start != end || self.doc_state().is_some_and(|s| s.autocorrect_off) || !crate::shell::autocorrect::settings().auto_borders {
            return false;
        }
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, start.para) else { return false };
        if doc.track_changes || p.len() != start.offset || p.list.is_some() || !p.runs.iter().all(|r| matches!(r.kind, FlowRunKind::Text(_))) {
            return false;
        }
        let Some(kind) = crate::shell::autocorrect::auto_border_marker(&p.text()) else { return false };
        let Some((container, index)) = flow_command::locate(doc, start.para) else { return false };
        let (style, width) = match kind {
            AutoBorder::Thin => (BorderStyle::Single, 9525),
            AutoBorder::Thick => (BorderStyle::Single, 28575),
            AutoBorder::Double => (BorderStyle::Double, 9525),
            AutoBorder::Dotted => (BorderStyle::Dotted, 9525),
            AutoBorder::Wave => (BorderStyle::Wave, 9525),
            AutoBorder::Triple => (BorderStyle::ThinThickThinSmallGap, 9525),
        };
        let mut line = p.clone();
        line.runs.clear();
        line.direct.borders[2] = Some(Some(FlowBorder { style, width, color: ibis_doc_model::Rgb(0), space: 12_700, art: None }));
        line.resolve(&doc.styles);
        let mut next = p.clone();
        next.id = doc.new_id();
        next.runs.clear();
        next.direct.borders = Default::default();
        next.resolve(&doc.styles);
        let next_id = next.id;
        let command = FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks: vec![Block::Paragraph(line), Block::Paragraph(next)] };
        if self.run_flow(vec![command], cx) {
            self.set_caret(FlowPos::new(next_id, 0), false, cx);
            self.scroll_to_caret(cx);
        }
        true
    }

    /// Toggle checkbox at position `pos`:
    /// Returns true if a checkbox was found at `pos` and toggled.
    pub(crate) fn toggle_checkbox_at(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_offset = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if matches!(run.kind, FlowRunKind::CheckBox { .. }) {
                    target_offset = Some(offset);
                    break;
                } else if let FlowRunKind::FormField(field) = &run.kind {
                    if matches!(field.kind, FormFieldKind::CheckBox { .. }) {
                        target_offset = Some(offset);
                        break;
                    }
                }
            }
            offset += len;
        }
        if let Some(target) = target_offset {
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::ToggleCheckBox { at }], cx);
            true
        } else {
            false
        }
    }

    /// Toggle checkbox around caret (e.g. on Space key or command):
    /// If caret or selection is at a checkbox, toggle it.
    pub(crate) fn toggle_checkbox_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        // If selection is 1 char and it's a checkbox:
        if start.para == end.para && end.offset == start.offset + 1 {
            if self.toggle_checkbox_at(start, cx) {
                return true;
            }
        }
        // If caret is collapsed: check at start.offset
        if start == end {
            if self.toggle_checkbox_at(start, cx) {
                return true;
            }
        }
        false
    }

    /// Insert a new checkbox control at the current caret position.
    pub(crate) fn insert_checkbox(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let kind = FlowRunKind::CheckBox {
            checked: false,
            checked_char: '\u{2611}',
            unchecked_char: '\u{2610}',
        };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + 1), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + 2), false, cx);
    }

    /// Whether the active shape or text box (`box_edit` or `shape_selected`) has a checklist bullet on its current paragraph(s).
    pub(crate) fn shape_has_checklist(&self) -> bool {
        let Some((at, range, _)) = self.active_shape_text_target() else { return false };
        let Some(body) = self.shape_text_body_at(at) else { return false };
        let (s, e) = range.unwrap_or_else(|| {
            let c = self.doc_state().and_then(|st| st.box_edit.as_ref()).map_or(0, |b| b.edit.caret);
            (c, c)
        });
        let touched = body.paragraphs_in(s, e);
        !touched.is_empty()
            && touched.clone().all(|i| {
                body.paragraphs[i]
                    .bullet
                    .as_ref()
                    .is_some_and(|b| matches!(b.text.as_str(), "\u{2610}" | "\u{2611}"))
            })
    }

    /// Toggle checklist / checkbox on current paragraph or insert one:
    pub(crate) fn toggle_checklist(&mut self, cx: &mut Context<Self>) {
        if self.active_shape_text_target().is_some() {
            let caret_off = self.doc_state().and_then(|s| s.box_edit.as_ref()).map(|b| b.edit.caret);
            self.mutate_active_shape_text(
                move |body, range| {
                    let (s, e) = range.unwrap_or_else(|| {
                        let c = caret_off.unwrap_or(0);
                        (c, if caret_off.is_some() { c } else { body.text().chars().count() })
                    });
                    let touched = body.paragraphs_in(s, e);
                    let all_checked = !touched.is_empty()
                        && touched.clone().all(|i| {
                            body.paragraphs[i]
                                .bullet
                                .as_ref()
                                .is_some_and(|b| matches!(b.text.as_str(), "\u{2610}" | "\u{2611}"))
                        });
                    let list = if all_checked {
                        ibis_doc_model::ListKind::None
                    } else {
                        ibis_doc_model::ListKind::Symbol('\u{2610}')
                    };
                    body.format_paragraphs(
                        s,
                        e,
                        &ibis_doc_model::ParagraphFormat {
                            list: Some(list),
                            ..Default::default()
                        },
                    );
                },
                cx,
            );
            return;
        }
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(p) = flow_command::paragraph(doc, start.para) else { return };
        if let Some(first_run) = p.runs.first() && (matches!(first_run.kind, FlowRunKind::CheckBox { .. }) || matches!(&first_run.kind, FlowRunKind::FormField(f) if matches!(f.kind, FormFieldKind::CheckBox { .. }))) {
            self.toggle_checkbox_at(FlowPos::new(start.para, 0), cx);
        } else {
            self.insert_checkbox(cx);
        }
    }

    /// テキスト フォーム フィールドの新規挿入 (DOCX-FORM-016):
    pub(crate) fn insert_legacy_form_text(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let ff = FormFieldData {
            name: "Text1".into(),
            enabled: true,
            calc_on_exit: false,
            kind: FormFieldKind::TextInput {
                value: String::new(),
                default: String::new(),
                max_length: 0,
            },
        };
        let mut run = FlowRun::new(FlowRunKind::FormField(ff), FlowRunStyle::default());
        run.style.highlight = Some(ibis_doc_model::Rgb(0xd9d9d9));
        self.run_flow(vec![
            FlowCommand::InsertText { at: start, text: " ".to_string(), format: None },
            FlowCommand::InsertField { at: start, kind: run.kind },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + 5), false, cx);
    }

    /// チェック ボックス フォーム フィールドの新規挿入 (DOCX-FORM-017):
    pub(crate) fn insert_legacy_form_checkbox(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let ff = FormFieldData {
            name: "Check1".into(),
            enabled: true,
            calc_on_exit: false,
            kind: FormFieldKind::CheckBox {
                checked: false,
                default: false,
                size_auto: true,
            },
        };
        let mut run = FlowRun::new(FlowRunKind::FormField(ff), FlowRunStyle::default());
        run.style.highlight = Some(ibis_doc_model::Rgb(0xd9d9d9));
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind: run.kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + 1), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + 2), false, cx);
    }

    /// ドロップダウン フォーム フィールドの新規挿入 (DOCX-FORM-018):
    pub(crate) fn insert_legacy_form_dropdown(&mut self, items: Vec<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let items = if items.is_empty() {
            vec![
                crate::shell::i18n::text("_docx.form-choice-1").to_string(),
                crate::shell::i18n::text("_docx.form-choice-2").to_string(),
            ]
        } else {
            items
        };
        let display_len = items.first().map_or(0, |s| s.chars().count());
        let ff = FormFieldData {
            name: "DropDown1".into(),
            enabled: true,
            calc_on_exit: false,
            kind: FormFieldKind::DropDown {
                items,
                selected: 0,
                default: 0,
            },
        };
        let mut run = FlowRun::new(FlowRunKind::FormField(ff), FlowRunStyle::default());
        run.style.highlight = Some(ibis_doc_model::Rgb(0xd9d9d9));
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind: run.kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + display_len), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len + 1), false, cx);
    }

    /// テキスト フォーム フィールドの値設定 (DOCX-FORM-016):
    pub(crate) fn set_form_field_text_at(&mut self, pos: FlowPos, text: String, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_offset = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if let FlowRunKind::FormField(ff) = &run.kind {
                    if matches!(ff.kind, FormFieldKind::TextInput { .. }) {
                        target_offset = Some(offset);
                        break;
                    }
                }
            }
            offset += len;
        }
        if let Some(target) = target_offset {
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::SetFormFieldText { at, text }], cx);
            true
        } else {
            false
        }
    }

    /// ドロップダウン リストまたはコンボボックス項目の選択 (DOCX-FORM-006, DOCX-FORM-018):
    #[allow(dead_code)]
    pub(crate) fn select_dropdown_item_at(&mut self, pos: FlowPos, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_offset = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if matches!(run.kind, FlowRunKind::DropDownList { .. }) {
                    target_offset = Some(offset);
                    break;
                } else if let FlowRunKind::FormField(field) = &run.kind {
                    if matches!(field.kind, FormFieldKind::DropDown { .. }) {
                        target_offset = Some(offset);
                        break;
                    }
                }
            }
            offset += len;
        }
        if let Some(target) = target_offset {
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::SelectDropDownItem { at, index }], cx);
            true
        } else {
            false
        }
    }

    /// ドロップダウン リスト項目の順送り切り替え (DOCX-FORM-006, DOCX-FORM-018):
    pub(crate) fn cycle_dropdown_item_at(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_info = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if let FlowRunKind::DropDownList { items, selected, .. } = &run.kind {
                    if !items.is_empty() {
                        let next = (*selected + 1) % items.len();
                        target_info = Some((offset, next));
                        break;
                    }
                } else if let FlowRunKind::FormField(field) = &run.kind {
                    if let FormFieldKind::DropDown { items, selected, .. } = &field.kind {
                        if !items.is_empty() {
                            let next = (*selected + 1) % items.len();
                            target_info = Some((offset, next));
                            break;
                        }
                    }
                }
            }
            offset += len;
        }
        if let Some((target, next_idx)) = target_info {
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::SelectDropDownItem { at, index: next_idx }], cx);
            true
        } else {
            false
        }
    }

    /// カーソル位置のドロップダウン リスト項目の順送り切り替え:
    pub(crate) fn cycle_dropdown_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        self.cycle_dropdown_item_at(start, cx)
    }

    /// ドロップダウン リスト コントロールの新規挿入 (DOCX-FORM-006):
    pub(crate) fn insert_dropdown_list(&mut self, items: Vec<(String, String)>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let display_len = items.first().map_or(0, |(d, _)| d.chars().count());
        let kind = FlowRunKind::DropDownList {
            items,
            selected: 0,
            combo: false,
        };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + display_len), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len + 1), false, cx);
    }

    /// コンボ ボックス コントロールの新規挿入 (DOCX-FORM-005):
    pub(crate) fn insert_combo_box(&mut self, items: Vec<(String, String)>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let display_len = items.first().map_or(0, |(d, _)| d.chars().count());
        let kind = FlowRunKind::DropDownList {
            items,
            selected: 0,
            combo: true,
        };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + display_len), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len + 1), false, cx);
    }

    /// 注の区切り線: the line above the footnotes (`endnote` false) or the endnotes.
    pub(crate) fn set_note_separator(&mut self, endnote: bool, kind: ibis_doc_model::NoteSeparator, cx: &mut Context<Self>) {
        if self.flow().is_some_and(|d| d.note_separators[usize::from(endnote)] == kind) {
            return;
        }
        self.run_flow(vec![FlowCommand::SetNoteSeparator { endnote, kind }], cx);
    }

    /// 日付選択コントロールの日付送り (+days) (DOCX-FORM-007):
    pub(crate) fn step_date_picker_at(&mut self, pos: FlowPos, days: i64, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_info = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if let FlowRunKind::DatePicker { full_date, date_format, .. } = &run.kind {
                    target_info = Some((offset, full_date.clone(), date_format.clone()));
                    break;
                }
            }
            offset += len;
        }
        if let Some((target, full_date, date_format)) = target_info {
            let current_time = ibis_doc_model::date_field::parse_iso8601(&full_date).unwrap_or_else(crate::clock::local_now);
            let next_time = current_time.add_days(days);
            let next_iso = next_time.to_iso_date();
            let next_text = ibis_doc_model::date_field::format_picture(&next_time, &date_format);
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::SetDatePickerDate { at, full_date: next_iso, text: next_text }], cx);
            true
        } else {
            false
        }
    }

    /// カーソル位置の日付選択コントロールの日付送り:
    pub(crate) fn step_date_picker_around_caret(&mut self, days: i64, cx: &mut Context<Self>) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        self.step_date_picker_at(start, days, cx)
    }

    /// 日付選択コントロールの新規挿入 (DOCX-FORM-007):
    pub(crate) fn insert_date_picker(&mut self, date_format: Option<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let fmt = date_format.unwrap_or_else(|| "yyyy/M/d".to_string());
        let now = crate::clock::local_now();
        let full_date = now.to_iso_date();
        let text = ibis_doc_model::date_field::format_picture(&now, &fmt);
        let display_len = text.chars().count();
        let kind = FlowRunKind::DatePicker {
            full_date,
            date_format: fmt,
            text,
        };
        self.run_flow(vec![
            FlowCommand::InsertField { at: start, kind },
            FlowCommand::InsertText { at: FlowPos::new(start.para, start.offset + display_len), text: " ".to_string(), format: None },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len + 1), false, cx);
    }

    /// リッチテキスト コンテンツ コントロールの新規挿入 (DOCX-FORM-001):
    pub(crate) fn insert_rich_text_control(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let placeholder = text.is_none();
        let text = text.unwrap_or_else(|| crate::shell::i18n::text("_docx.content-control-placeholder").to_string());
        let display_len = text.chars().count();
        let kind = FlowRunKind::RichText { text, placeholder };
        // The space after it first, so that it is not typed into the control;
        // the caret stays at the control's end, where typing replaces the prompt.
        self.run_flow(vec![
            FlowCommand::InsertText { at: start, text: " ".to_string(), format: None },
            FlowCommand::InsertField { at: start, kind },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len), false, cx);
    }

    /// プレーンテキスト コンテンツ コントロールの新規挿入 (DOCX-FORM-002):
    pub(crate) fn insert_plain_text_control(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let placeholder = text.is_none();
        let text = text.unwrap_or_else(|| crate::shell::i18n::text("_docx.content-control-placeholder").to_string());
        let display_len = text.chars().count();
        let kind = FlowRunKind::PlainText { text, placeholder };
        // The space after it first, so that it is not typed into the control;
        // the caret stays at the control's end, where typing replaces the prompt.
        self.run_flow(vec![
            FlowCommand::InsertText { at: start, text: " ".to_string(), format: None },
            FlowCommand::InsertField { at: start, kind },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len), false, cx);
    }

    /// コンテンツ コントロールのテキスト更新 (DOCX-FORM-001, DOCX-FORM-002):
    pub(crate) fn set_content_control_text_at(&mut self, pos: FlowPos, text: String, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut found_target = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if matches!(run.kind, FlowRunKind::RichText { .. } | FlowRunKind::PlainText { .. } | FlowRunKind::BuildingBlockGallery { .. } | FlowRunKind::RepeatingSection { .. } | FlowRunKind::Group { .. }) {
                    found_target = Some(offset);
                    break;
                }
            }
            offset += len;
        }
        if let Some(target) = found_target {
            let at = FlowPos::new(pos.para, target);
            self.run_flow(vec![FlowCommand::SetContentControlText { at, text, placeholder: false }], cx);
            true
        } else {
            false
        }
    }

    /// 画像コンテンツ コントロールの新規挿入 (DOCX-FORM-003):
    pub(crate) fn insert_picture_control(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let data = ibis_doc_model::DEFAULT_PICTURE_CONTROL_PLACEHOLDER_PNG.to_vec();
        let media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, data.clone());
        self.resources.images.insert(media, Arc::new(Image::from_bytes(ImageFormat::Png, data)));
        let size = ibis_doc_model::EmuSize::new(
            ibis_doc_model::DEFAULT_PICTURE_CONTROL_WIDTH_EMU,
            ibis_doc_model::DEFAULT_PICTURE_CONTROL_HEIGHT_EMU,
        );
        self.run_flow(vec![
            FlowCommand::InsertText { at: start, text: " ".to_string(), format: None },
            FlowCommand::InsertPictureControl { at: start, media, size, placeholder: true },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + 1), false, cx);
    }

    /// pos が画像コンテンツ コントロールの位置かどうか
    pub(crate) fn is_picture_control_at(&self, pos: FlowPos) -> bool {
        let Some(p) = self.paragraph_of(pos.para) else { return false };
        let mut offset = 0;
        for run in &p.runs {
            if offset == pos.offset {
                return matches!(run.kind, FlowRunKind::PictureControl { .. });
            }
            offset += run.len();
        }
        false
    }

    /// キャレット位置またはその前後の画像コンテンツ コントロール
    pub(crate) fn picture_control_at_caret(&self) -> Option<(FlowPos, ibis_doc_model::MediaId, ibis_doc_model::EmuSize, bool)> {
        let (start, _) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        let mut offset = 0;
        let mut previous = None;
        for run in &p.runs {
            if let FlowRunKind::PictureControl { media, size, placeholder, .. } = &run.kind {
                let found = Some((FlowPos::new(p.id, offset), *media, *size, *placeholder));
                if offset == start.offset {
                    return found;
                }
                if offset + 1 == start.offset {
                    previous = found;
                }
            }
            offset += run.len();
        }
        previous
    }

    /// 画像コンテンツ コントロールの画像を差し替える (DOCX-FORM-003)
    pub(crate) fn replace_picture_control_at(&mut self, at: FlowPos, data: Vec<u8>, format: ibis_doc_model::MediaFormat, keep_size: bool, cx: &mut Context<Self>) -> bool {
        self.doc_replace_picture(at, data, format, keep_size, cx)
    }

    /// キャレット周辺の画像コンテンツ コントロールの画像を差し替える
    pub(crate) fn replace_picture_control_around_caret(&mut self, data: Vec<u8>, format: ibis_doc_model::MediaFormat, keep_size: bool, cx: &mut Context<Self>) -> bool {
        let Some((at, ..)) = self.picture_control_at_caret() else { return false };
        self.replace_picture_control_at(at, data, format, keep_size, cx)
    }

    /// 画像コンテンツ コントロールを初期プレースホルダーに戻す (DOCX-FORM-003)
    pub(crate) fn reset_picture_control_at(&mut self, at: FlowPos, cx: &mut Context<Self>) -> bool {
        let placeholder_bytes = ibis_doc_model::DEFAULT_PICTURE_CONTROL_PLACEHOLDER_PNG.to_vec();
        let placeholder_media = self.document.ids().add_media(ibis_doc_model::MediaFormat::Png, placeholder_bytes.clone());
        self.resources.images.insert(placeholder_media, Arc::new(Image::from_bytes(ImageFormat::Png, placeholder_bytes)));
        let size = ibis_doc_model::EmuSize::new(
            ibis_doc_model::DEFAULT_PICTURE_CONTROL_WIDTH_EMU,
            ibis_doc_model::DEFAULT_PICTURE_CONTROL_HEIGHT_EMU,
        );
        self.run_flow(vec![FlowCommand::ResetPictureControl { at, placeholder_media, size }], cx)
    }

    /// キャレット周辺の画像コンテンツ コントロールをプレースホルダーに戻す
    pub(crate) fn reset_picture_control_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((at, ..)) = self.picture_control_at_caret() else { return false };
        self.reset_picture_control_at(at, cx)
    }

    /// 画像コンテンツ コントロールの差し替えファイルを選択して実行
    pub(crate) fn doc_pick_replace_picture_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                extensions: Vec::new(),
                multiple: false,
                prompt: Some(crate::shell::i18n::text("_shell.insert")),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let format = ibis_doc_model::MediaFormat::from_path(&path.to_string_lossy());
                this.update(cx, |this, cx| {
                    this.replace_picture_control_around_caret(data, format, false, cx);
                }).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// 文書パーツギャラリー コンテンツ コントロールの新規挿入 (DOCX-FORM-008):
    pub(crate) fn insert_building_block_gallery(&mut self, gallery: Option<String>, category: Option<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let gallery = gallery.unwrap_or_else(|| ibis_doc_model::DEFAULT_BUILDING_BLOCK_GALLERY_NAME.to_string());
        let category = category.unwrap_or_else(|| ibis_doc_model::DEFAULT_BUILDING_BLOCK_GALLERY_CATEGORY.to_string());
        let text = crate::shell::i18n::text("_docx.building-block-gallery-placeholder").to_string();
        let display_len = text.chars().count();
        let kind = FlowRunKind::BuildingBlockGallery {
            gallery,
            category,
            text,
            placeholder: true,
        };
        self.run_flow(vec![
            FlowCommand::InsertText { at: start, text: " ".to_string(), format: None },
            FlowCommand::InsertField { at: start, kind },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len), false, cx);
    }

    /// キャレット位置またはその前後の文書パーツギャラリー コンテンツ コントロール
    pub(crate) fn building_block_gallery_at_caret(&self) -> Option<(FlowPos, String, String, String, bool)> {
        let (start, _) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        let mut offset = 0;
        let mut previous = None;
        for run in &p.runs {
            if let FlowRunKind::BuildingBlockGallery { gallery, category, text, placeholder } = &run.kind {
                let len = run.len();
                let found = Some((FlowPos::new(p.id, offset), gallery.clone(), category.clone(), text.clone(), *placeholder));
                if start.offset >= offset && start.offset < offset + len {
                    return found;
                }
                if start.offset == offset + len {
                    previous = found;
                }
            }
            offset += run.len();
        }
        previous
    }

    /// 文書パーツギャラリー コンテンツ コントロールの候補を次に切り替える (DOCX-FORM-008)
    pub(crate) fn cycle_building_block_gallery_at(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if let FlowRunKind::BuildingBlockGallery { gallery, category, text, placeholder } = &run.kind {
                    target = Some((offset, gallery.clone(), category.clone(), text.clone(), *placeholder));
                    break;
                }
            }
            offset += len;
        }
        let Some((target_offset, gallery, _, current_text, placeholder)) = target else { return false };
        let at = FlowPos::new(pos.para, target_offset);
        let presets = [
            format!("{gallery} 1"),
            format!("{gallery} 2"),
            format!("{gallery} 3"),
        ];
        let next_text = if placeholder {
            presets[0].clone()
        } else {
            let idx = presets.iter().position(|p| p == &current_text);
            match idx {
                Some(i) => presets[(i + 1) % presets.len()].clone(),
                None => presets[0].clone(),
            }
        };
        let status_msg = crate::shell::i18n::format("docx.form.gallery.cycled", &[("part", &next_text)]);
        if self.run_flow(vec![FlowCommand::SetContentControlText { at, text: next_text, placeholder: false }], cx) {
            self.status = Some(status_msg.into());
            cx.notify();
            true
        } else {
            false
        }
    }

    /// キャレット周辺の文書パーツギャラリー コンテンツ コントロールの候補を次に切り替える
    pub(crate) fn cycle_building_block_gallery_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((at, ..)) = self.building_block_gallery_at_caret() else { return false };
        self.cycle_building_block_gallery_at(at, cx)
    }

    /// 文書パーツギャラリー コンテンツ コントロールを初期プレースホルダーに戻す (DOCX-FORM-008)
    pub(crate) fn reset_building_block_gallery_at(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target_offset = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if matches!(run.kind, FlowRunKind::BuildingBlockGallery { .. }) {
                    target_offset = Some(offset);
                    break;
                }
            }
            offset += len;
        }
        let Some(target_offset) = target_offset else { return false };
        let at = FlowPos::new(pos.para, target_offset);
        let prompt = crate::shell::i18n::text("_docx.building-block-gallery-placeholder").to_string();
        if self.run_flow(vec![FlowCommand::SetContentControlText { at, text: prompt, placeholder: true }], cx) {
            self.status = Some(crate::shell::i18n::text("docx.form.gallery.reset-done"));
            cx.notify();
            true
        } else {
            false
        }
    }

    /// キャレット周辺の文書パーツギャラリー コンテンツ コントロールをプレースホルダーに戻す
    pub(crate) fn reset_building_block_gallery_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((at, ..)) = self.building_block_gallery_at_caret() else { return false };
        self.reset_building_block_gallery_at(at, cx)
    }

    /// 繰返しセクション コンテンツ コントロールの新規挿入 (DOCX-FORM-009):
    pub(crate) fn insert_repeating_section(&mut self, section_title: Option<String>, initial_text: Option<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let section_title = section_title.unwrap_or_else(|| ibis_doc_model::DEFAULT_REPEATING_SECTION_TITLE.to_string());
        let text = initial_text.unwrap_or_else(|| crate::shell::i18n::text("_docx.repeating-section-placeholder").to_string());
        let display_len = text.chars().count();
        let kind = FlowRunKind::RepeatingSection {
            section_title,
            items: vec![text],
            selected: 0,
            do_not_allow_insert_delete: false,
        };
        self.run_flow(vec![
            FlowCommand::InsertText { at: start, text: " ".to_string(), format: None },
            FlowCommand::InsertField { at: start, kind },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len), false, cx);
    }

    /// キャレット位置またはその前後の繰返しセクション コンテンツ コントロール
    pub(crate) fn repeating_section_at_caret(&self) -> Option<(FlowPos, String, Vec<String>, usize, bool)> {
        let (start, _) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        let mut offset = 0;
        let mut previous = None;
        for run in &p.runs {
            if let FlowRunKind::RepeatingSection { section_title, items, selected, do_not_allow_insert_delete } = &run.kind {
                let len = run.len();
                let found = Some((FlowPos::new(p.id, offset), section_title.clone(), items.clone(), *selected, *do_not_allow_insert_delete));
                if start.offset >= offset && start.offset < offset + len {
                    return found;
                }
                if start.offset == offset + len {
                    previous = found;
                }
            }
            offset += run.len();
        }
        previous
    }

    /// 繰返しセクションに新しいセクション項目を追加する (DOCX-FORM-009)
    pub(crate) fn add_repeating_section_item_at(&mut self, pos: FlowPos, text: Option<String>, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if let FlowRunKind::RepeatingSection { section_title, items, selected, .. } = &run.kind {
                    target = Some((offset, section_title.clone(), items.len(), *selected));
                    break;
                }
            }
            offset += len;
        }
        let Some((target_offset, title, len, selected)) = target else { return false };
        let at = FlowPos::new(pos.para, target_offset);
        let insert_idx = selected + 1;
        let new_text = text.unwrap_or_else(|| format!("{title} {}", len + 1));
        let status_msg = crate::shell::i18n::format("docx.form.repeating.added", &[("section", &title)]);
        if self.run_flow(vec![FlowCommand::InsertRepeatingSectionItem { at, index: insert_idx, text: new_text, restore_selected: None }], cx) {
            self.status = Some(status_msg.into());
            cx.notify();
            true
        } else {
            false
        }
    }

    /// キャレット周辺の繰返しセクションに新しいセクション項目を追加する
    pub(crate) fn add_repeating_section_item_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((at, ..)) = self.repeating_section_at_caret() else { return false };
        self.add_repeating_section_item_at(at, None, cx)
    }

    /// 繰返しセクションから現在のセクション項目を削除する (DOCX-FORM-009)
    pub(crate) fn delete_repeating_section_item_at(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if let FlowRunKind::RepeatingSection { section_title, items, selected, .. } = &run.kind {
                    if items.len() > 1 {
                        target = Some((offset, section_title.clone(), *selected));
                    }
                    break;
                }
            }
            offset += len;
        }
        let Some((target_offset, title, selected)) = target else { return false };
        let at = FlowPos::new(pos.para, target_offset);
        let status_msg = crate::shell::i18n::format("docx.form.repeating.deleted", &[("section", &title)]);
        if self.run_flow(vec![FlowCommand::DeleteRepeatingSectionItem { at, index: selected, restore_selected: None }], cx) {
            self.status = Some(status_msg.into());
            cx.notify();
            true
        } else {
            false
        }
    }

    /// キャレット周辺の繰返しセクションから現在のセクション項目を削除する
    pub(crate) fn delete_repeating_section_item_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((at, ..)) = self.repeating_section_at_caret() else { return false };
        self.delete_repeating_section_item_at(at, cx)
    }

    /// 繰返しセクションの項目を巡回して切り替える (DOCX-FORM-009)
    pub(crate) fn cycle_repeating_section_at(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if let FlowRunKind::RepeatingSection { section_title, items, selected, .. } = &run.kind {
                    if items.len() > 1 {
                        target = Some((offset, section_title.clone(), items.len(), *selected));
                    }
                    break;
                }
            }
            offset += len;
        }
        let Some((target_offset, title, len, selected)) = target else { return false };
        let next_idx = (selected + 1) % len;
        let at = FlowPos::new(pos.para, target_offset);
        let status_msg = crate::shell::i18n::format("docx.form.repeating.cycled", &[("section", &title)]);
        if self.run_flow(vec![FlowCommand::SelectRepeatingSectionItem { at, index: next_idx }], cx) {
            self.status = Some(status_msg.into());
            cx.notify();
            true
        } else {
            false
        }
    }

    /// キャレット周辺の繰返しセクションの項目を巡回して切り替える
    pub(crate) fn cycle_repeating_section_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((at, ..)) = self.repeating_section_at_caret() else { return false };
        self.cycle_repeating_section_at(at, cx)
    }

    /// グループ化コンテンツ コントロールの新規挿入 (DOCX-FORM-014):
    pub(crate) fn insert_group_control(&mut self, text: Option<String>, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let placeholder = text.is_none();
        let text = text.unwrap_or_else(|| crate::shell::i18n::text("_docx.group-placeholder").to_string());
        let display_len = text.chars().count();
        let kind = FlowRunKind::Group { text, placeholder };
        self.run_flow(vec![
            FlowCommand::InsertText { at: start, text: " ".to_string(), format: None },
            FlowCommand::InsertField { at: start, kind },
        ], cx);
        self.set_caret(FlowPos::new(start.para, start.offset + display_len), false, cx);
    }

    /// キャレット位置またはその前後のグループ化コンテンツ コントロール (DOCX-FORM-014)
    pub(crate) fn group_control_at_caret(&self) -> Option<(FlowPos, String, bool)> {
        let (start, _) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        let mut offset = 0;
        let mut previous = None;
        for run in &p.runs {
            if let FlowRunKind::Group { text, placeholder } = &run.kind {
                let len = run.len();
                let found = Some((FlowPos::new(p.id, offset), text.clone(), *placeholder));
                if start.offset >= offset && start.offset < offset + len {
                    return found;
                }
                if start.offset == offset + len {
                    previous = found;
                }
            }
            offset += run.len();
        }
        previous
    }

    /// グループ化の解除 (DOCX-FORM-014)
    pub(crate) fn ungroup_control_at(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut offset = 0;
        let mut target = None;
        for run in &p.runs {
            let len = run.len();
            if (pos.offset >= offset && pos.offset < offset + len) || (pos.offset == offset && len == 0) {
                if matches!(run.kind, FlowRunKind::Group { .. }) {
                    target = Some(offset);
                    break;
                }
            }
            offset += len;
        }
        let Some(target_offset) = target else { return false };
        let at = FlowPos::new(pos.para, target_offset);
        let status_msg = crate::shell::i18n::text("docx.form.ungrouped").to_string();
        if self.run_flow(vec![FlowCommand::UngroupControl { at, restore: None }], cx) {
            self.status = Some(status_msg.into());
            cx.notify();
            true
        } else {
            false
        }
    }

    /// キャレット周辺のグループ化を解除する (DOCX-FORM-014)
    pub(crate) fn ungroup_control_around_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((at, ..)) = self.group_control_at_caret() else { return false };
        self.ungroup_control_at(at, cx)
    }

    /// グループ化の実行 (DOCX-FORM-014):
    /// 選択範囲がある場合はそのテキストをグループ化し、選択範囲がない場合はキャレット位置のグループ化またはプレースホルダー挿入を行う
    pub(crate) fn group_control_at_selection_or_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, end)) = self.ordered_selection() else { return false };
        if start != end {
            let sel = self.selected_text();
            if !sel.is_empty() {
                let display_len = sel.chars().count();
                let status_msg = crate::shell::i18n::text("docx.form.grouped").to_string();
                let kind = FlowRunKind::Group { text: sel, placeholder: false };
                let mut commands = Vec::new();
                commands.push(FlowCommand::Delete { from: start, to: end });
                commands.push(FlowCommand::InsertText { at: start, text: " ".to_string(), format: None });
                commands.push(FlowCommand::InsertField { at: start, kind });
                if self.run_flow(commands, cx) {
                    self.set_caret(FlowPos::new(start.para, start.offset + display_len), false, cx);
                    self.status = Some(status_msg.into());
                    cx.notify();
                    return true;
                }
                return false;
            }
        }
        if let Some((at, ..)) = self.group_control_at_caret() {
            return self.ungroup_control_at(at, cx);
        }
        let status_msg = crate::shell::i18n::text("docx.form.grouped").to_string();
        self.insert_group_control(None, cx);
        self.status = Some(status_msg.into());
        cx.notify();
        true
    }

    /// コンテンツ コントロールのデザイン モードが有効かどうか。
    pub fn form_design_mode(&self) -> bool {
        self.flow().is_some_and(|d| d.form_design_mode)
    }

    /// コンテンツ コントロールのデザイン モードを切り替える（1ステップ完全 Undo/Redo）。
    pub fn toggle_form_design_mode(&mut self, cx: &mut Context<Self>) {
        let current = self.form_design_mode();
        let target = !current;
        self.run_flow(vec![ibis_doc_model::flow_command::FlowCommand::SetFormDesignMode(target)], cx);
        let msg = if target {
            crate::shell::i18n::text("docx.form.design-mode.enabled")
        } else {
            crate::shell::i18n::text("docx.form.design-mode.disabled")
        };
        self.status = Some(msg.to_string().into());
        cx.notify();
    }

    /// 箇条書き・番号付きリスト: the paragraphs become items of a list of
    /// bullets (or numbers), or stop being list items if they are already.
    pub(crate) fn toggle_list(&mut self, bullet: bool, cx: &mut Context<Self>) {
        if self.active_shape_text_target().is_some() {
            let want = if bullet { ibis_doc_model::ListKind::Bullet } else { ibis_doc_model::ListKind::Numbered };
            self.mutate_active_shape_text(
                move |body, range| {
                    let (s, e) = range.unwrap_or_else(|| (0, body.text().chars().count()));
                    let pr = body.paragraphs_in(s, e);
                    let cur_is_same = body
                        .paragraphs
                        .get(*pr.start())
                        .and_then(|p| p.bullet.as_ref())
                        .is_some_and(|b| b.auto.is_none() == bullet);
                    let next = if cur_is_same { ibis_doc_model::ListKind::None } else { want };
                    body.format_paragraphs(
                        s,
                        e,
                        &ibis_doc_model::ParagraphFormat {
                            list: Some(next),
                            ..Default::default()
                        },
                    );
                },
                cx,
            );
            return;
        }
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow().cloned() else { return };
        let numbering = &doc.styles.numbering;
        let current = self.paragraph_of(start.para).and_then(|p| p.list).and_then(|l| numbering.level(l.num_id, 0)).map(|l| l.format == "bullet");
        let keep = self.text_selection();
        let mut commands = Vec::new();
        let change = if current == Some(bullet) {
            ParaChange::List(None)
        } else {
            let id = match numbering.find(bullet) {
                Some(id) => id,
                None => {
                    let (with, id) = numbering.with_new_list(bullet);
                    commands.push(FlowCommand::SetNumbering { numbering: with });
                    id
                }
            };
            ParaChange::List(Some(id))
        };
        commands.extend(self.selected_paragraph_format_commands(change));
        self.run_flow(commands, cx);
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = (a != c).then_some(a);
            state.caret = Some(c);
        }
    }

    /// ⌥⌫・⌥⌦ (Ctrl+Backspace・Delete): the selection, else the word before or after the caret.
    pub(crate) fn delete_word(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some((anchor, caret)) = self.text_selection() else { return };
        if anchor == caret {
            let to = self.word_step(caret, forward);
            if to == caret {
                return;
            }
            if let Some(state) = self.doc_state_mut() {
                state.anchor = Some(caret);
                state.caret = Some(to);
            }
        }
        self.delete_char(forward, cx);
    }

    /// Line spacing `lines` times the single (⌘1・⌘5・⌘2): 240 is single, 360 one and a half, 480 double.
    pub(crate) fn set_line_spacing_multiple(&mut self, twips: i64, cx: &mut Context<Self>) {
        self.apply_para_change(ParaChange::Line(LineHeight::Auto(twips as _)), cx);
    }

    /// The style named `name` (`heading 1`) for the paragraphs of the selection, or the normal one (`None`).
    pub(crate) fn apply_named_style(&mut self, name: Option<&str>, cx: &mut Context<Self>) {
        let id = match name {
            Some(n) => match self.flow().and_then(|d| d.styles.paragraph_style_named(n).map(str::to_string)) {
                Some(id) => Some(id),
                None => return,
            },
            None => None,
        };
        self.apply_style(id, cx);
    }

    /// Backspace (`forward` false) or Delete.
    pub(crate) fn delete_char(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.shape_selected && s.box_edit.is_none() && s.selected_shapes.len() > 1) {
            let mut shapes: Vec<FlowPos> = self.all_selected_shapes().into_iter().map(|(_, _, at, _)| at).collect();
            if let Some(doc) = self.flow() {
                shapes.sort_by_key(|pos| {
                    let p_idx = doc.body.iter().position(|b| matches!(b, Block::Paragraph(p) if p.id == pos.para)).unwrap_or(0);
                    (p_idx, pos.offset)
                });
            }
            shapes.dedup();
            let first = shapes.first().copied();
            let commands: Vec<FlowCommand> = shapes
                .into_iter()
                .rev()
                .map(|at| FlowCommand::Delete {
                    from: at,
                    to: FlowPos::new(at.para, at.offset + 1),
                })
                .collect();
            if !commands.is_empty() && self.run_flow(commands, cx) && let Some(first) = first {
                self.set_caret(first, false, cx);
            }
            return;
        }
        // Backspace at a list item's start takes its number away first.
        if !forward && self.list_item_at_start().is_some() {
            self.apply_para_change(ParaChange::List(None), cx);
            return;
        }
        if self.has_extra_selections() {
            let ranges = self.selection_ranges();
            if ranges.iter().any(|&(a, b)| self.reject_hidden_history_range(a, b, cx)) { return; }
            let track = self.flow().is_some_and(|d| d.track_changes);
            let who = crate::docx::comments::comment_author();
            // From the last range up, so that each leaves the offsets of those before it.
            let commands: Vec<FlowCommand> = ranges.iter().rev().map(|&(from, to)| if track {
                FlowCommand::TrackDelete { from, to, author: who.author.clone(), date: who.date.clone() }
            } else {
                FlowCommand::Delete { from, to }
            }).collect();
            let first = ranges[0].0;
            if self.run_flow(commands, cx) {
                self.set_caret(first, false, cx);
            }
            return;
        }
        let Some((start, end)) = self.ordered_selection() else { return };
        if self.reject_hidden_history_range(start, end, cx) { return; }
        let track = self.flow().is_some_and(|d| d.track_changes);
        let who = crate::docx::comments::comment_author();
        if start != end {
            let commands = if track {
                vec![FlowCommand::TrackDelete { from: start, to: end, author: who.author, date: who.date }]
            } else if start.para == end.para && !crate::docx::flow_layout::showing_markup() {
                visible_delete_commands(self.paragraph_of(start.para).unwrap(), start.offset, end.offset)
            } else { vec![FlowCommand::Delete { from: start, to: end }] };
            if self.run_flow(commands, cx) {
                self.set_caret(start, false, cx);
            }
            return;
        }
        let other = self.step(start, forward);
        let (mut a, mut b) = if forward { (start, other) } else { (other, start) };
        // Delete the visible character, keeping the invisible marks/history
        // that movement passed over. Apply this to tracked deletions as well.
        if a.para == b.para && b.offset > a.offset + 1 {
            if forward { a.offset = b.offset - 1; } else { b.offset = a.offset + 1; }
        }
        if track {
            // A tracked deletion: the character is marked (in one paragraph;
            // across paragraphs the caret just moves).
            if a.para == b.para && a != b {
                let old_len = self.paragraph_of(a.para).map_or(0, |p| p.len());
                if self.run_flow(vec![FlowCommand::TrackDelete { from: a, to: b, author: who.author, date: who.date }], cx) {
                    // A tracked insertion is removed; ordinary text is marked
                    // deleted. In markup view forward Delete stays after it.
                    let len = self.paragraph_of(a.para).map_or(0, |p| p.len());
                    let caret = if forward && len == old_len { b } else { a };
                    self.set_caret(FlowPos::new(caret.para, caret.offset.min(len)), false, cx);
                }
            } else {
                self.set_caret(other, false, cx);
            }
            return;
        }
        if other == start {
            return;
        }
        let caret_after = if forward { start } else { a };
        // Joining paragraphs of different lists (a cell and the body) is
        // not done; the caret just moves.
        let doc = self.flow().cloned();
        let same_list = doc.as_ref().is_some_and(|d| flow_command::locate(d, a.para).map(|l| l.0) == flow_command::locate(d, b.para).map(|l| l.0));
        if !same_list {
            self.set_caret(other, false, cx);
            return;
        }
        if self.run_flow(vec![FlowCommand::Delete { from: a, to: b }], cx) {
            self.set_caret(caret_after, false, cx);
            self.scroll_to_caret(cx);
        }
    }

    /// The direct formatting at the selection's start (or what typing would take).
    pub(crate) fn current_format(&self) -> Option<(RunFormat, ibis_doc_model::FlowRunStyle)> {
        if let Some(fmt) = self.box_edit_current_format() {
            return Some(fmt);
        }
        let (start, end) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        if let Some(pending) = self.doc_state().and_then(|s| s.pending.clone()) {
            let doc = self.flow()?;
            let (_, base) = doc.styles.paragraph_base(p.style.as_deref(), p.table_style.as_deref());
            let style = doc.styles.run_format(&base, pending.char_style.as_deref(), &pending.format).style();
            return Some((pending.format, style));
        }
        let (format, _, style) = if start != end {
            p.format_at(start.offset + 1)
        } else {
            visible_format_at(p, start.offset)
        };
        Some((format, style))
    }

    /// Apply a character format to the selection, or to what is typed next.
    pub(crate) fn apply_run_change(&mut self, change: RunChange, cx: &mut Context<Self>) {
        if self.apply_box_edit_run_change(&change, cx) {
            return;
        }
        // A selection of several ranges: all of them, as one step.
        if self.has_extra_selections() && self.flow().is_some_and(|d| !d.track_changes && (super::flow_layout::showing_markup() || !order(d).iter().filter_map(|&id| flow_command::paragraph(d, id)).any(|p| p.runs.iter().any(|r| r.revision.is_some())))) {
            let ranges = self.selection_ranges();
            let (extras, anchor, caret) = self.doc_state().map(|s| (s.extra_selections.clone(), s.anchor, s.caret)).unwrap_or_default();
            let commands = ranges.into_iter().map(|(from, to)| FlowCommand::SetRunFormat { from, to, change: change.clone() }).collect();
            if self.run_flow(commands, cx) {
                self.restore_extra_selections(extras, anchor, caret);
            }
            return;
        }
        let Some((start, end)) = self.ordered_selection() else { return };
        if start == end {
            let Some(p) = self.paragraph_of(start.para) else { return };
            let (mut format, char_style, _) = match self.doc_state().and_then(|s| s.pending.clone()) {
                Some(f) => (f.format, f.char_style, p.mark.clone()),
                None => visible_format_at(p, start.offset),
            };
            change.apply(&mut format);
            if let Some(state) = self.doc_state_mut() {
                state.pending = Some(InsertFormat { format, char_style });
            }
            cx.notify();
            return;
        }
        let anchor = self.text_selection();
        // The shared command skips whole table blocks between the endpoints.
        // Use the DOCX replacement path for tables, tracked formatting and
        // final-text view; hidden deleted runs must retain their formatting.
        let whole_table = self.flow().is_some_and(|doc| {
            let (Some((container, first)), Some((other, last))) = (flow_command::locate(doc, start.para), flow_command::locate(doc, end.para)) else { return false };
            container == other && first <= last && flow_command::blocks(doc, container).is_some_and(|blocks| blocks[first..=last].iter().any(|b| matches!(b, Block::Table(_))))
        });
        if whole_table || self.flow().is_some_and(|doc| doc.track_changes) || !super::flow_layout::showing_markup() {
            self.apply_selected_text_format(|format| change.apply(format), cx);
            if let (Some((a, c)), Some(state)) = (anchor, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
            }
            return;
        }
        self.run_flow(vec![FlowCommand::SetRunFormat { from: start, to: end, change }], cx);
        if let (Some((a, c)), Some(state)) = (anchor, self.doc_state_mut()) {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
    }

    pub(crate) fn toggle_bold(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.bold);
        self.apply_run_change(RunChange::Bold(!on), cx);
    }

    pub(crate) fn toggle_italic(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.italic);
        self.apply_run_change(RunChange::Italic(!on), cx);
    }

    pub(crate) fn toggle_underline(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.underline);
        self.apply_run_change(RunChange::Underline(!on), cx);
    }

    /// ⌘⇧D (double) and ⌘⇧W (words only): that kind of underline, or none when the text has it already.
    pub(crate) fn toggle_underline_kind(&mut self, kind: ibis_doc_model::UnderlineStyle, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.underline && s.underline_style == kind);
        self.set_underline_style(if on { None } else { Some(kind) }, cx);
    }

    pub(crate) fn toggle_strike(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.strike);
        self.apply_run_change(RunChange::Strike(!on), cx);
    }

    pub(crate) fn toggle_double_strike(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.double_strike);
        self.apply_run_change(RunChange::DoubleStrike(!on), cx);
    }

    pub(crate) fn toggle_caps(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.caps);
        self.apply_run_change(RunChange::Caps(!on), cx);
    }

    pub(crate) fn toggle_small_caps(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.small_caps);
        self.apply_run_change(RunChange::SmallCaps(!on), cx);
    }

    /// 現在選択のフォーマットが隠し文字（w:vanish）かどうか。
    pub(crate) fn is_hidden(&self) -> bool {
        self.current_format().is_some_and(|(_, s)| s.hidden)
    }

    /// 隠し文字 (w:vanish, DOCX-OPTIONS-008) をトグル.
    pub(crate) fn toggle_hidden(&mut self, cx: &mut Context<Self>) {
        let on = self.is_hidden();
        self.apply_run_change(RunChange::Hidden(!on), cx);
    }

    /// 中抜き・輪郭 (w:outline, DOCX-FONT-017) をトグル.
    pub(crate) fn toggle_outline(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.outline);
        self.apply_run_change(RunChange::Outline(!on), cx);
    }

    /// 影付き (w:shadow, DOCX-FONT-017) をトグル.
    pub(crate) fn toggle_shadow(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.shadow);
        self.apply_run_change(RunChange::Shadow(!on), cx);
    }

    /// 浮き出し (w:emboss, DOCX-FONT-017) をトグル.
    pub(crate) fn toggle_emboss(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.emboss);
        self.apply_run_change(RunChange::Emboss(!on), cx);
    }

    /// 浮き彫り (w:imprint, DOCX-FONT-017) をトグル.
    pub(crate) fn toggle_imprint(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.imprint);
        self.apply_run_change(RunChange::Imprint(!on), cx);
    }

    /// 文字の囲み線 (w:bdr) をトグル.
    pub(crate) fn toggle_character_border(&mut self, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.border.is_some());
        let border = if on {
            None
        } else {
            Some(ibis_doc_model::FlowBorder {
                style: ibis_doc_model::BorderStyle::Single,
                width: 4 * 12_700 / 8, // 0.5 pt
                color: ibis_doc_model::Rgb(0x000000),
                space: 0,
                art: None,
            })
        };
        self.apply_run_change(RunChange::Border(border), cx);
    }

    /// 文字の網かけ (w:shd, Word 標準 15% グレー 0xD9D9D9) をトグル.
    pub(crate) fn toggle_character_shading(&mut self, cx: &mut Context<Self>) {
        let shading_color = ibis_doc_model::Rgb(0xd9d9d9);
        let on = self.current_format().is_some_and(|(_, s)| s.highlight == Some(shading_color));
        let highlight = if on { None } else { Some(shading_color) };
        self.apply_run_change(RunChange::Highlight(highlight), cx);
    }

    /// 文字種の変換 (DOCX-JA-020).
    pub(crate) fn convert_case(&mut self, conversion: CaseConversion, cx: &mut Context<Self>) {
        let (from, to) = if let Some((s, e)) = self.ordered_selection() && s != e {
            (s, e)
        } else if let Some((_, caret)) = self.text_selection() {
            let (a, b) = self.word_at(caret);
            if a != b {
                (a, b)
            } else {
                return;
            }
        } else {
            return;
        };

        let new_to = if from.para == to.para {
            let selected = self.paragraph_of(from.para).map(|p| selected_paragraph_text(p, from.offset, to.offset)).unwrap_or_default();
            let new_len = conversion.apply(&selected).chars().count();
            FlowPos::new(from.para, from.offset + new_len)
        } else {
            to
        };

        let cmd = FlowCommand::ConvertCase { from, to, conversion };
        if self.run_flow(vec![cmd], cx) {
            self.set_caret(from, false, cx);
            self.set_caret(new_to, true, cx);
            self.scroll_to_caret(cx);
        }
    }

    pub(crate) fn toggle_text_position(&mut self, position: ibis_doc_model::VerticalPosition, cx: &mut Context<Self>) {
        let on = self.current_format().is_some_and(|(_, s)| s.position == position);
        self.apply_run_change(RunChange::Position(if on { ibis_doc_model::VerticalPosition::Baseline } else { position }), cx);
    }

    /// Returns `(font_name, size_hundredths_pt)` for every visible run in the current selection (or active shape text).
    fn target_run_fonts_and_sizes(&self) -> Vec<(String, u32)> {
        if let Some((at, range, is_box_edit)) = self.active_shape_text_target() {
            let targets = if is_box_edit || range.is_some() {
                vec![at]
            } else {
                let mut t = self.selected_shape_positions();
                if !t.contains(&at) {
                    t.insert(0, at);
                }
                t
            };
            let mut out = Vec::new();
            for target_at in targets {
                let Some(p) = self.paragraph_of(target_at.para) else { continue };
                let mut offset = 0;
                for run in &p.runs {
                    if offset == target_at.offset
                        && let FlowRunKind::Shape(shape) = &run.kind
                    {
                        let body = match &shape.kind {
                            ibis_doc_model::ShapeKind::Auto(a) => a.text.as_ref(),
                            ibis_doc_model::ShapeKind::TextBox(b) => Some(b),
                            _ => None,
                        };
                        if let Some(body) = body {
                            let default_font = body.style.font.clone().unwrap_or_else(|| "游明朝".to_string()); // i18n-ok（フォント名（既定の字形。ファイルに保存される））
                            let default_size = body.style.font_size;
                            let mut shape_out = Vec::new();
                            let mut pos = 0usize;
                            for para in &body.paragraphs {
                                for r in &para.runs {
                                    let n = r.text.chars().count();
                                    let in_range = match range {
                                        Some((from, to)) => pos < to && pos + n > from && n > 0,
                                        None => n > 0,
                                    };
                                    if in_range {
                                        let f = r.style.font.clone().or_else(|| r.style.fallback_font.clone()).unwrap_or_else(|| default_font.clone());
                                        let sz = r.style.font_size.unwrap_or(default_size);
                                        shape_out.push((f, sz));
                                    }
                                    pos += n;
                                }
                                pos += 1;
                            }
                            if shape_out.is_empty() {
                                shape_out.push((default_font, default_size));
                            }
                            out.extend(shape_out);
                        }
                    }
                    offset += run.len();
                }
            }
            if !out.is_empty() {
                return out;
            }
        }
        let Some((start, end)) = self.ordered_selection() else { return Vec::new() };
        if start == end {
            return self
                .current_format()
                .map(|(_, s)| {
                    let f = s.east_asian_font.or(s.font).unwrap_or_else(|| "游明朝".to_string()); // i18n-ok（フォント名（既定の字形。ファイルに保存される））
                    vec![(f, s.size)]
                })
                .unwrap_or_default();
        }
        let Some(doc) = self.flow() else { return Vec::new() };
        let order = order(doc);
        let (Some(i), Some(j)) = (order.iter().position(|&p| p == start.para), order.iter().position(|&p| p == end.para)) else {
            return Vec::new();
        };
        let (min_i, max_i) = (i.min(j), i.max(j));
        let mut out = Vec::new();
        for &pid in &order[min_i..=max_i] {
            let Some(p) = flow_command::paragraph(doc, pid) else { continue };
            let from_off = if pid == start.para { start.offset } else { 0 };
            let to_off = if pid == end.para { end.offset } else { p.len() };
            let mut off = 0usize;
            for run in &p.runs {
                let len = run.len();
                if off < to_off && off + len > from_off && len > 0 && matches!(run.kind, FlowRunKind::Text(_)) {
                    let style = &run.style;
                    let f = style.east_asian_font.clone().or_else(|| style.font.clone()).unwrap_or_else(|| "游明朝".to_string()); // i18n-ok（フォント名（既定の字形。ファイルに保存される））
                    out.push((f, style.size));
                }
                off += len;
            }
        }
        out
    }

    /// Toolbar font selector label: empty string when selection spans multiple distinct fonts.
    pub(crate) fn selection_font_label(&self) -> String {
        let items = self.target_run_fonts_and_sizes();
        let Some((first_font, _)) = items.first() else {
            return self
                .current_format()
                .and_then(|(_, s)| s.east_asian_font.or(s.font))
                .unwrap_or_else(|| "游明朝".to_string()); // i18n-ok（フォント名（既定の字形。ファイルに保存される））
        };
        if items.iter().all(|(f, _)| f == first_font) {
            first_font.clone()
        } else {
            String::new()
        }
    }

    /// Toolbar font size display: `"{min}+"` when selection spans multiple distinct font sizes.
    pub(crate) fn selection_font_size_label(&self) -> Option<gpui_kit::SharedString> {
        let items = self.target_run_fonts_and_sizes();
        let first = items.first().map(|(_, sz)| *sz).or_else(|| self.current_format().map(|(_, s)| s.size))?;
        let min_sz = items.iter().map(|(_, sz)| *sz).min().unwrap_or(first);
        let max_sz = items.iter().map(|(_, sz)| *sz).max().unwrap_or(first);
        let pt = min_sz as f32 / 100.0;
        if min_sz != max_sz {
            Some(format!("{pt}+").into())
        } else {
            Some(format!("{pt}").into())
        }
    }

    pub(crate) fn step_text_size(&mut self, grow: bool, cx: &mut Context<Self>) {
        if self.active_shape_text_target().is_some() {
            self.mutate_active_shape_text(
                move |body, range| {
                    let default_sz = body.style.font_size;
                    if let Some((from, to)) = range {
                        body.split_runs_at(from);
                        body.split_runs_at(to);
                        let mut pos = 0usize;
                        for para in &mut body.paragraphs {
                            for run in &mut para.runs {
                                let n = run.text.chars().count();
                                if pos >= from && pos + n <= to && n > 0 {
                                    let cur = run.style.font_size.unwrap_or(default_sz);
                                    run.style.font_size = Some(ibis_doc_model::step_font_size(cur, grow));
                                }
                                pos += n;
                            }
                            pos += 1;
                        }
                    } else {
                        body.style.font_size = ibis_doc_model::step_font_size(default_sz, grow);
                        for para in &mut body.paragraphs {
                            for run in &mut para.runs {
                                let cur = run.style.font_size.unwrap_or(default_sz);
                                run.style.font_size = Some(ibis_doc_model::step_font_size(cur, grow));
                            }
                            let cur_end = para.end_style.font_size.unwrap_or(default_sz);
                            para.end_style.font_size = Some(ibis_doc_model::step_font_size(cur_end, grow));
                        }
                    }
                },
                cx,
            );
            return;
        }
        let items = self.target_run_fonts_and_sizes();
        let has_mixed_sizes = items.first().is_some_and(|(_, first_sz)| items.iter().any(|(_, sz)| *sz != *first_sz));
        if has_mixed_sizes
            && let Some((start, end)) = self.ordered_selection()
            && start != end
        {
            let anchor = self.text_selection();
            self.apply_selected_text_format(
                |format| {
                    let next = ibis_doc_model::step_font_size(format.style().size, grow);
                    RunChange::Size(next).apply(format);
                },
                cx,
            );
            if let (Some((a, c)), Some(state)) = (anchor, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
            }
            return;
        }
        let size = self.current_format().map_or(1050, |(_, s)| s.size);
        self.apply_run_change(RunChange::Size(ibis_doc_model::step_font_size(size, grow)), cx);
    }

    pub(crate) fn set_text_size(&mut self, size: u32, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::Size(size), cx);
    }

    pub(crate) fn set_complex_script_size(&mut self, size: u32, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::ComplexScriptSize(size), cx);
    }

    pub(crate) fn set_text_color(&mut self, color: Rgb, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::Color(color), cx);
    }

    pub(crate) fn set_theme_color(&mut self, color: Rgb, theme_color: ibis_doc_model::FlowThemeColor, cx: &mut Context<Self>) {
        let resolved = theme_color.resolve(&self.presentation().theme_colors).unwrap_or(color);
        self.apply_run_change(RunChange::ThemeColor { color: resolved, theme_color }, cx);
    }

    pub(crate) fn set_highlight(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::Highlight(color), cx);
    }

    pub(crate) fn set_character_spacing(&mut self, spacing: Option<ibis_doc_model::Emu>, cx: &mut Context<Self>) {
        if self.current_format().is_some_and(|(_, s)| s.spacing == spacing.unwrap_or(0)) {
            return;
        }
        self.apply_run_change(RunChange::Spacing(spacing), cx);
    }

    pub(crate) fn set_character_scale(&mut self, scale: Option<u32>, cx: &mut Context<Self>) {
        if self.current_format().is_some_and(|(_, s)| s.scale == scale.unwrap_or(100)) {
            return;
        }
        self.apply_run_change(RunChange::Scale(scale), cx);
    }

    pub(crate) fn set_character_kerning(&mut self, kern: Option<u32>, cx: &mut Context<Self>) {
        if self.current_format().is_some_and(|(f, _)| f.kern == kern) {
            return;
        }
        self.apply_run_change(RunChange::Kern(kern), cx);
    }

    /// The paragraphs the selection touches (their first and last, in one list).
    pub(super) fn selected_paragraphs(&self) -> Option<(u32, u32)> {
        let (start, end) = self.ordered_selection()?;
        let doc = self.flow()?;
        let same = flow_command::locate(doc, start.para).map(|l| l.0) == flow_command::locate(doc, end.para).map(|l| l.0);
        Some(if same { (start.para, end.para) } else { (end.para, end.para) })
    }

    pub(crate) fn paragraph_format_commands(&self, first: u32, last: u32, change: ParaChange) -> Vec<FlowCommand> {
        let mut commands = vec![FlowCommand::SetParagraphFormat { first, last, change: change.clone() }];
        // The shared range command only visits direct paragraphs. Whole selected
        // tables need their cell paragraphs included in the same undo operation.
        if !matches!(change, ParaChange::SectionBreak(_)) && let Some(doc) = self.flow() {
            if let (Some((container, a)), Some((other, b))) = (flow_command::locate(doc, first), flow_command::locate(doc, last)) {
                if container == other && a <= b && let Some(blocks) = flow_command::blocks(doc, container) {
                    for block in &blocks[a..=b] {
                        if matches!(block, Block::Table(_)) {
                            for paragraph in super::block_tree::paragraphs(std::slice::from_ref(block)) {
                                commands.push(FlowCommand::SetParagraphFormat { first: paragraph.id, last: paragraph.id, change: change.clone() });
                            }
                        }
                    }
                }
            }
        }
        commands
    }

    /// Format commands for all touched paragraphs across the selection,
    /// including selections that span multiple table cells or the whole table.
    pub(crate) fn selected_paragraph_format_commands(&self, change: ParaChange) -> Vec<FlowCommand> {
        // A selection of several ranges: the paragraphs of all of them.
        if self.has_extra_selections()
            && let Some(doc) = self.flow()
        {
            let order = order(doc);
            let mut ids: Vec<u32> = Vec::new();
            for (a, b) in self.selection_ranges() {
                let (Some(i), Some(j)) = (order.iter().position(|&p| p == a.para), order.iter().position(|&p| p == b.para)) else { continue };
                for &id in &order[i.min(j)..=i.max(j)] {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
            return ids.into_iter().map(|id| FlowCommand::SetParagraphFormat { first: id, last: id, change: change.clone() }).collect();
        }
        let Some((start, end)) = self.ordered_selection() else { return Vec::new() };
        let Some(doc) = self.flow() else { return Vec::new() };
        let same = flow_command::locate(doc, start.para).map(|l| l.0) == flow_command::locate(doc, end.para).map(|l| l.0);
        if same {
            return self.paragraph_format_commands(start.para, end.para, change);
        }
        let order = order(doc);
        let (Some(i), Some(j)) = (order.iter().position(|&p| p == start.para), order.iter().position(|&p| p == end.para)) else {
            return self.paragraph_format_commands(end.para, end.para, change);
        };
        let (min, max) = if i <= j { (i, j) } else { (j, i) };
        let mut commands = Vec::new();
        for &para_id in &order[min..=max] {
            commands.push(FlowCommand::SetParagraphFormat { first: para_id, last: para_id, change: change.clone() });
        }
        commands
    }

    pub(crate) fn apply_para_change(&mut self, change: ParaChange, cx: &mut Context<Self>) {
        if self.active_shape_text_target().is_some() {
            let mut pf = ibis_doc_model::ParagraphFormat::default();
            match &change {
                ParaChange::Align(a) => {
                    pf.align = Some(match a {
                        FlowAlign::Left => ibis_doc_model::TextAlign::Left,
                        FlowAlign::Center => ibis_doc_model::TextAlign::Center,
                        FlowAlign::Right => ibis_doc_model::TextAlign::Right,
                        FlowAlign::Justify => ibis_doc_model::TextAlign::Justify,
                        FlowAlign::Distribute => ibis_doc_model::TextAlign::Distributed,
                    });
                }
                ParaChange::Line(LineHeight::Auto(twips)) => {
                    let pct = ((*twips as f64 / 240.0) * 100_000.0).round().max(0.0) as u32;
                    pf.line_spacing = Some(Some(ibis_doc_model::LineSpacing::Percent(pct)));
                }
                ParaChange::Line(LineHeight::Exact(emu) | LineHeight::AtLeast(emu)) => {
                    let hundredths_pt = (*emu as f64 / 127.0).round().max(0.0) as u32;
                    pf.line_spacing = Some(Some(ibis_doc_model::LineSpacing::Points(hundredths_pt)));
                }
                ParaChange::SpaceBefore(emu) => {
                    let hundredths_pt = (*emu as f64 / 127.0).round().max(0.0) as u32;
                    pf.space_before = Some(hundredths_pt);
                }
                ParaChange::SpaceAfter(emu) => {
                    let hundredths_pt = (*emu as f64 / 127.0).round().max(0.0) as u32;
                    pf.space_after = Some(hundredths_pt);
                }
                ParaChange::Left(emu) => {
                    pf.margin_left = Some(*emu);
                }
                _ => return,
            }
            self.mutate_active_shape_text(
                move |body, range| {
                    if let Some((from, to)) = range {
                        body.format_paragraphs(from, to, &pf);
                    } else {
                        if let Some(a) = pf.align {
                            body.style.align = a;
                        }
                        let end = body.text().chars().count();
                        body.format_paragraphs(0, end, &pf);
                    }
                },
                cx,
            );
            return;
        }
        let keep = self.text_selection();
        let extras = self.doc_state().map(|s| (s.extra_selections.clone(), s.anchor, s.caret));
        let commands = self.selected_paragraph_format_commands(change);
        if !commands.is_empty() && self.run_flow(commands, cx) {
            // A paragraph's format moves no offsets: the ranges stay.
            if let Some((extras, anchor, caret)) = extras.filter(|(e, _, _)| !e.is_empty()) {
                self.restore_extra_selections(extras, anchor, caret);
                return;
            }
        }
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = (a != c).then_some(a);
            state.caret = Some(c);
        }
    }

    pub(crate) fn set_alignment(&mut self, align: FlowAlign, cx: &mut Context<Self>) {
        self.apply_para_change(ParaChange::Align(align), cx);
    }

    /// Line spacing as a multiple of single (1.0, 1.15, 1.5, 2.0).
    pub(crate) fn set_line_spacing(&mut self, multiple: f32, cx: &mut Context<Self>) {
        self.apply_para_change(ParaChange::Line(LineHeight::Auto((multiple * 240.0).round() as u32)), cx);
    }

    /// インデント増・減: the left indent by a step of 10.5 pt (Word's with
    /// the default font), not below 0. If current paragraph is a list item,
    /// adjust the list level (0..=8) instead, as Word does.
    pub(crate) fn step_indent(&mut self, increase: bool, cx: &mut Context<Self>) {
        if self.active_shape_text_target().is_some() {
            let delta = if increase { 420 * 635 } else { -420 * 635 };
            self.mutate_active_shape_text(
                move |body, range| {
                    let (s, e) = range.unwrap_or_else(|| (0, body.text().chars().count()));
                    body.format_paragraphs(
                        s,
                        e,
                        &ibis_doc_model::ParagraphFormat {
                            indent_by: Some(delta),
                            ..Default::default()
                        },
                    );
                },
                cx,
            );
            return;
        }
        let Some((start, _)) = self.ordered_selection() else { return };
        if let Some(list) = self.paragraph_of(start.para).and_then(|p| p.list) {
            let new_level = if increase {
                (list.level + 1).min(8)
            } else {
                list.level.saturating_sub(1)
            };
            if new_level != list.level {
                self.apply_para_change(ParaChange::Level(new_level), cx);
                return;
            } else if !increase && list.level == 0 {
                self.apply_para_change(ParaChange::List(None), cx);
                return;
            }
        }
        let left = self.paragraph_of(start.para).map_or(0, |p| p.left);
        let step = 420 * 635;
        let to = if increase { (left / step + 1) * step } else { ((left - 1).max(0) / step) * step };
        self.apply_para_change(ParaChange::Left(to.max(0)), cx);
    }

    pub(crate) fn apply_style(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.apply_para_change(ParaChange::Style(id), cx);
    }

    pub(crate) fn clear_formatting(&mut self, cx: &mut Context<Self>) {
        self.apply_run_change(RunChange::Clear, cx);
    }

    /// 句読点のぶら下げを行う（w:overflowPunct）の切り替え。
    pub(crate) fn toggle_overflow_punct(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_overflow_punct();
        self.apply_para_change(ibis_doc_model::ParaChange::OverflowPunct(!current), cx);
    }

    /// キャレット位置の段落の句読点のぶら下げ設定（既定値 true）。
    pub(crate) fn caret_overflow_punct(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.overflow_punct)
    }

    /// 日本語と英字の間隔を自動調整する（w:autoSpaceDE）の切り替え。
    pub(crate) fn toggle_auto_space_de(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_auto_space_de();
        self.apply_para_change(ibis_doc_model::ParaChange::AutoSpaceDE(!current), cx);
    }

    /// キャレット位置の段落の日本語と英字の間隔自動調整設定（既定値 true）。
    pub(crate) fn caret_auto_space_de(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.auto_space_letters)
    }

    /// 日本語と数字の間隔を自動調整する（w:autoSpaceDN）の切り替え。
    pub(crate) fn toggle_auto_space_dn(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_auto_space_dn();
        self.apply_para_change(ibis_doc_model::ParaChange::AutoSpaceDN(!current), cx);
    }

    /// キャレット位置の段落の日本語と数字の間隔自動調整設定（既定値 true）。
    pub(crate) fn caret_auto_space_dn(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.auto_space_digits)
    }

    /// 禁則処理を行う（w:kinsoku）の切り替え。
    pub(crate) fn toggle_kinsoku(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_kinsoku();
        self.apply_para_change(ibis_doc_model::ParaChange::Kinsoku(!current), cx);
    }

    /// キャレット位置の段落の禁則処理設定（既定値 true）。
    pub(crate) fn caret_kinsoku(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.kinsoku)
    }

    /// 単語の途中で改行しない（w:wordWrap）の切り替え。
    pub(crate) fn toggle_word_wrap(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_word_wrap();
        self.apply_para_change(ibis_doc_model::ParaChange::WordWrap(!current), cx);
    }

    /// キャレット位置の段落の単語途中改行禁止設定（既定値 true）。
    pub(crate) fn caret_word_wrap(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.word_wrap)
    }

    /// 右から左へ記述する（w:bidi）の切り替え。
    pub(crate) fn toggle_bidi(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_bidi();
        self.apply_para_change(ibis_doc_model::ParaChange::Bidi(!current), cx);
    }

    /// キャレット位置の段落の右から左へ記述設定（既定値 false）。
    pub(crate) fn caret_bidi(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        self.paragraph_of(start.para).map_or(false, |p| p.bidi)
    }

    /// 行グリッド線に合わせる（w:snapToGrid）の切り替え。
    pub(crate) fn toggle_snap_to_grid(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_snap_to_grid();
        self.apply_para_change(ibis_doc_model::ParaChange::SnapToGrid(!current), cx);
    }

    /// キャレット位置の段落の行グリッド吸着設定（既定値 true）。
    pub(crate) fn caret_snap_to_grid(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return true };
        self.paragraph_of(start.para).map_or(true, |p| p.snap_to_grid)
    }

    /// 単語の途中で改行しない（ハイフネーションを行わない、w:suppressAutoHyphens）の切り替え。
    pub(crate) fn toggle_suppress_auto_hyphens(&mut self, cx: &mut Context<Self>) {
        let current = self.caret_suppress_auto_hyphens();
        self.apply_para_change(ibis_doc_model::ParaChange::SuppressAutoHyphens(!current), cx);
    }

    /// キャレット位置の段落のハイフネーション除外設定（既定値 false）。
    pub(crate) fn caret_suppress_auto_hyphens(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        self.paragraph_of(start.para).map_or(false, |p| p.suppress_auto_hyphens)
    }

    /// キャレット位置の段落のドロップ キャップ設定。
    pub(crate) fn caret_drop_cap(&self) -> Option<ibis_doc_model::DropCap> {
        let (start, _) = self.ordered_selection()?;
        self.paragraph_of(start.para).and_then(|p| p.drop_cap)
    }

    /// キャレット位置の段落のドロップ キャップ配置（None / Drop / Margin）。
    pub(crate) fn caret_drop_cap_location(&self) -> ibis_doc_model::DropCapLocation {
        self.caret_drop_cap().map_or(ibis_doc_model::DropCapLocation::None, |dc| dc.location)
    }

    /// ドロップ キャップの配置（なし / ドロップ / 余白）を設定。
    pub(crate) fn set_drop_cap_location(&mut self, location: ibis_doc_model::DropCapLocation, cx: &mut Context<Self>) {
        if location == ibis_doc_model::DropCapLocation::None {
            self.apply_para_change(ibis_doc_model::ParaChange::DropCap(None), cx);
        } else {
            let mut dc = self.caret_drop_cap().unwrap_or_default();
            dc.location = location;
            self.apply_para_change(ibis_doc_model::ParaChange::DropCap(Some(dc)), cx);
        }
    }

    /// ドロップ キャップの行数を設定（2〜5行など）。
    pub(crate) fn set_drop_cap_lines(&mut self, lines: u32, cx: &mut Context<Self>) {
        let mut dc = self.caret_drop_cap().unwrap_or(ibis_doc_model::DropCap {
            location: ibis_doc_model::DropCapLocation::Drop,
            lines: 3,
            space: 0,
        });
        dc.lines = lines.clamp(1, 10);
        self.apply_para_change(ibis_doc_model::ParaChange::DropCap(Some(dc)), cx);
    }

    /// ドロップ キャップの本文からの距離を設定（EMU）。
    pub(crate) fn set_drop_cap_distance(&mut self, space: ibis_doc_model::Emu, cx: &mut Context<Self>) {
        let mut dc = self.caret_drop_cap().unwrap_or(ibis_doc_model::DropCap {
            location: ibis_doc_model::DropCapLocation::Drop,
            lines: 3,
            space: 0,
        });
        dc.space = space;
        self.apply_para_change(ibis_doc_model::ParaChange::DropCap(Some(dc)), cx);
    }

    /// キャレット位置の図形・テキスト ボックスの垂直配置（上揃え / 中央揃え / 下揃え）。
    pub(crate) fn caret_shape_anchor(&self) -> Option<ibis_doc_model::VerticalAnchor> {
        self.text_box_at_caret().map(|i| i.anchor)
    }

    /// キャレット位置（および複数選択中のすべて）の図形・テキスト ボックスの垂直配置を設定。
    pub(crate) fn set_shape_anchor(&mut self, anchor: ibis_doc_model::VerticalAnchor, cx: &mut Context<Self>) {
        let mut targets = self.selected_shape_positions();
        if targets.is_empty()
            && let Some(info) = self.text_box_at_caret()
        {
            targets.push(info.at);
        }
        let commands: Vec<ibis_doc_model::FlowCommand> = targets
            .into_iter()
            .filter_map(|at| {
                let info = self.text_box_near(at)?;
                (info.anchor != anchor).then_some(ibis_doc_model::FlowCommand::SetShapeAnchor { at: info.at, anchor })
            })
            .collect();
        if commands.is_empty() {
            return;
        }
        let keep = self.text_selection();
        let keep_multi = self.doc_state().map(|s| s.selected_shapes.clone()).unwrap_or_default();
        let was_selected = self.doc_state().is_some_and(|s| s.shape_selected);
        let box_edit_backup = self.doc_state_mut().and_then(|s| s.box_edit.take());
        if self.run_flow(commands, cx) {
            if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
                state.anchor = Some(a);
                state.caret = Some(c);
                state.shape_selected = was_selected;
                state.selected_shapes = keep_multi;
            }
            if let Some(state) = self.doc_state_mut().and_then(|s| s.text_box.as_mut()) {
                state.anchor = anchor;
                state.initial.anchor = anchor;
            }
        }
        if let Some(edit) = box_edit_backup {
            if let Some(state) = self.doc_state_mut() {
                state.box_edit = Some(edit);
            }
            self.sync_docx_edited_box_resource();
        }
        cx.notify();
    }

    /// Every range of the selection, in document order: the ranges added
    /// with ⌘ and the caret's, when it is not empty.
    pub(crate) fn selection_ranges(&self) -> Vec<(FlowPos, FlowPos)> {
        let mut ranges = self.doc_state().map(|s| s.extra_selections.clone()).unwrap_or_default();
        if let Some((a, b)) = self.ordered_selection().filter(|(a, b)| a != b) {
            ranges.push((a, b));
        }
        if let Some(doc) = self.flow() {
            let order = order(doc);
            let key = |p: FlowPos| (order.iter().position(|&id| id == p.para).unwrap_or(usize::MAX), p.offset);
            ranges.sort_by_key(|&(a, _)| key(a));
        }
        ranges
    }

    /// Whether the selection is made of several ranges.
    pub(crate) fn has_extra_selections(&self) -> bool {
        self.doc_state().is_some_and(|s| !s.extra_selections.is_empty())
    }

    /// Put the ranges `extras` back as the selection's other ranges (after a command
    /// that run_flow cleared them, one that moved no offsets).
    pub(crate) fn restore_extra_selections(&mut self, extras: Vec<(FlowPos, FlowPos)>, anchor: Option<FlowPos>, caret: Option<FlowPos>) {
        if let Some(state) = self.doc_state_mut() {
            state.extra_selections = extras;
            state.anchor = anchor;
            state.caret = caret;
        }
    }

    /// The selected text (paragraphs joined with line feeds).
    pub(crate) fn selected_text(&self) -> String {
        if self.has_extra_selections() {
            return self.selection_ranges().into_iter().map(|(a, b)| self.range_text(a, b)).collect::<Vec<_>>().join("\n");
        }
        let Some((start, end)) = self.ordered_selection() else { return String::new() };
        self.range_text(start, end)
    }

    /// The text from `start` to `end` (paragraphs joined with line feeds).
    pub(crate) fn range_text(&self, start: FlowPos, end: FlowPos) -> String {
        let Some(doc) = self.flow() else { return String::new() };
        let order = order(doc);
        let (Some(i), Some(j)) = (order.iter().position(|&p| p == start.para), order.iter().position(|&p| p == end.para)) else {
            // Header or footer: one paragraph.
            return self.paragraph_of(start.para).map(|p| selected_paragraph_text(p, start.offset, end.offset)).unwrap_or_default();
        };
        let mut out = Vec::new();
        for (k, id) in order[i..=j].iter().enumerate() {
            let Some(p) = self.paragraph_of(*id) else { continue };
            let a = if k == 0 { start.offset } else { 0 };
            let b = if i + k == j { end.offset } else { p.len() };
            out.push(selected_paragraph_text(p, a, b));
        }
        out.join("\n")
    }

    pub(crate) fn copy_text(&mut self, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s|s.shape_selected && s.box_edit.is_none()){
            let shapes:Vec<_>=self.all_selected_shapes().into_iter().filter_map(|(_,_,at,_)|self.shape_at(at).cloned()).collect();
            if self.copy_office_shapes(&shapes,cx){return;}
        }
        let text = self.selected_text();
        // Several ranges: their text, one range to a line (no formatting).
        if self.has_extra_selections() {
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            return;
        }
        if self.copy_paragraphs(&text, cx) { return; }
        if !text.is_empty() {
            if self.copy_formatted_text(&text, cx) { return; }
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub(crate) fn cut_text(&mut self, cx: &mut Context<Self>) {
        if let Some((start,end)) = self.ordered_selection() && self.reject_hidden_history_range(start,end,cx) { return; }
        if !self.has_selection() {
            return;
        }
        self.copy_text(cx);
        self.delete_char(false, cx);
    }

    pub(crate) fn default_paste_mode(&self) -> crate::shell::editor::DefaultPasteMode {
        self.default_paste_mode
    }

    pub(crate) fn set_default_paste_mode(&mut self, mode: crate::shell::editor::DefaultPasteMode, cx: &mut Context<Self>) {
        self.default_paste_mode = mode;
        cx.notify();
    }

    /// ユーザー名 (Word Options > 全般: ユーザー設定, DOCX-OPTIONS-003).
    pub(crate) fn user_name(&self) -> String {
        if let Some(ref name) = self.custom_user_name {
            if !name.trim().is_empty() {
                return name.clone();
            }
        }
        crate::shell::user_name::user_name()
    }

    /// ユーザー頭文字 (Word Options > 全般: ユーザー設定, DOCX-OPTIONS-003).
    pub(crate) fn user_initials(&self) -> String {
        if let Some(ref inits) = self.custom_user_initials {
            if !inits.trim().is_empty() {
                return inits.clone();
            }
        }
        crate::shell::user_name::user_initials()
    }

    /// ユーザー名と頭文字を設定 (DOCX-OPTIONS-003).
    pub(crate) fn set_user_info(&mut self, name: Option<String>, initials: Option<String>, cx: &mut Context<Self>) {
        self.custom_user_name = name.clone();
        self.custom_user_initials = initials.clone();
        crate::shell::user_name::set_custom_user_info(name, initials);
        cx.notify();
    }

    /// 元の書式を保持して貼り付け (Keep Source Formatting).
    pub(crate) fn paste_keep_source(&mut self, cx: &mut Context<Self>) {
        if let Some((start, end)) = self.ordered_selection() && self.reject_hidden_history_range(start, end, cx) { return; }
        let Some(item) = cx.read_from_clipboard() else { return };
        let Some(text) = item.text() else { return };
        if self.paste_paragraphs(&item, cx) { return; }
        if self.paste_formatted_text(&item, cx) { return; }
        self.type_text(&text, cx);
    }

    /// 挿入 > 表紙 (DOCX-PAGE-022): cover `design` put at the top of the document, with the first heading as the title and the user's
    /// name and today's date; one step of undo.
    pub(crate) fn insert_cover_page(&mut self, design: usize, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let who = crate::docx::comments::comment_author();
        let title = doc
            .body
            .iter()
            .find_map(|b| match b {
                Block::Paragraph(p) if flow_command::heading_level(&doc.styles, p).is_some() && !p.text().trim().is_empty() => Some(p.text()),
                _ => None,
            })
            .unwrap_or_else(|| crate::shell::i18n::text("docx.cover.title-placeholder").to_string());
        let date: String = who.date.chars().take(10).collect();
        let blocks = ibis_doc_model::cover::cover_blocks(doc, design, &title, &who.author, &date);
        if self.run_flow(vec![FlowCommand::ReplaceBlocks { container: Container::Body, index: 0, remove: 0, blocks }], cx) {
            self.status = Some(crate::shell::i18n::text("docx.cover.inserted").into());
            cx.notify();
        }
    }

    /// 校閲 > 比較: the file picked is the original; this document is the revised one, and the changes between them are
    /// marked as tracked changes in it (one step of undo).
    pub(crate) fn pick_compare_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.open")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| this.compare_with_file(&path, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// 比較の設定: `which` 0 formatting, 1 case, 2 tables, 3 white space, 4 headers, 5 footnotes, 6 comments, 7 fields.
    pub(crate) fn toggle_compare_option(&mut self, which: u8, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let o = &mut state.compare_options;
            match which {
                0 => o.format = !o.format,
                1 => o.case = !o.case,
                2 => o.tables = !o.tables,
                3 => o.white_space = !o.white_space,
                4 => o.headers = !o.headers,
                5 => o.footnotes = !o.footnotes,
                6 => o.comments = !o.comments,
                _ => o.fields = !o.fields,
            }
        }
        cx.notify();
    }

    /// The changes from the document in the file at `path` to this one, as tracked changes.
    pub(crate) fn compare_with_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let original = std::fs::read(path).ok().and_then(|data| ibis_pptx::read(&data).ok()).and_then(|p| p.flow);
        let Some(original) = original else {
            self.status = Some(crate::shell::i18n::text("docx.merge-panel.cannot-read-file").into());
            return cx.notify();
        };
        let Some(doc) = self.flow() else { return };
        let who = crate::docx::comments::comment_author();
        let opts = self.doc_state().map(|s| s.compare_options).unwrap_or_default();
        let compared = ibis_doc_model::compare::compare_documents_with(&original, doc, &who.author, &who.date, &opts);
        let remove = doc.body.len();
        let mut commands = vec![
            FlowCommand::ReplaceBlocks { container: Container::Body, index: 0, remove, blocks: compared.body },
            FlowCommand::SetTrackChanges(true),
        ];
        if opts.headers {
            commands.push(FlowCommand::ReplaceBlocks { container: Container::Header, index: 0, remove: doc.header.len(), blocks: compared.header });
            commands.push(FlowCommand::ReplaceBlocks { container: Container::Footer, index: 0, remove: doc.footer.len(), blocks: compared.footer });
        }
        if opts.comments {
            commands.push(FlowCommand::SetComments(compared.comments));
        }
        if self.run_flow(commands, cx) {
            self.status = Some(crate::shell::i18n::text("docx.editing.changes-were-marked-tracked").into());
            cx.notify();
        }
    }

    /// 校閲 > 比較 > 比較結果を新しい文書として作成… (Word's Compare into New Document)
    pub(crate) fn pick_compare_file_new_doc(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.open")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| this.compare_with_file_new_doc(&path, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// Compares this document with `path` and opens the result as a new document ("比較結果" / Compare Result).
    pub(crate) fn compare_with_file_new_doc(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            self.path = None;
        }
        self.file_name = Some(crate::shell::i18n::text("docx.compare.result-title").into());
        self.compare_with_file(path, cx);
    }

    /// 校閲 > 文書の結合 (DOCX-REVIEW-016 / docx.combine):
    /// The file picked is combined with this document; revisions from both authors are preserved.
    pub(crate) fn pick_combine_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.open")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update(cx, |this, cx| this.combine_with_file(&path, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// Combines the document in the file at `path` with this one, merging revisions and comments.
    pub(crate) fn combine_with_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        let other = std::fs::read(path).ok().and_then(|data| ibis_pptx::read(&data).ok()).and_then(|p| p.flow);
        let Some(other) = other else {
            self.status = Some(crate::shell::i18n::text("docx.merge-panel.cannot-read-file").into());
            return cx.notify();
        };
        let Some(doc) = self.flow() else { return };
        let who = crate::docx::comments::comment_author();
        let opts = self.doc_state().map(|s| s.compare_options).unwrap_or_default();
        let blocks = ibis_doc_model::compare::combine_blocks_with(&other.body, &doc.body, &who.author, &who.date, &opts);
        let remove = doc.body.len();
        let mut commands = vec![FlowCommand::ReplaceBlocks { container: Container::Body, index: 0, remove, blocks }];
        if !other.comments.is_empty() {
            let mut merged_comments = doc.comments.clone();
            let mut max_comment_id = merged_comments.iter().map(|c| c.id).max().unwrap_or(0);
            for mut comment in other.comments.clone() {
                if merged_comments.iter().any(|c| c.id == comment.id) {
                    max_comment_id += 1;
                    comment.id = max_comment_id;
                }
                merged_comments.push(comment);
            }
            commands.push(FlowCommand::SetComments(merged_comments));
        }
        if self.run_flow(commands, cx) {
            self.status = Some(crate::shell::i18n::text("docx.editing.documents-combined").into());
            cx.notify();
        }
    }

    /// A picture another app copied (the clipboard holds an image and no text) pasted at the caret.
    pub(crate) fn paste_clipboard_image(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(item) = cx.read_from_clipboard() else { return false };
        if item.text().is_some_and(|t| !t.is_empty()) {
            return false;
        }
        let Some(ClipboardEntry::Image(image)) = item.entries().iter().find(|e| matches!(e, ClipboardEntry::Image(_))) else { return false };
        let format = match image.format {
            ImageFormat::Png => ibis_doc_model::MediaFormat::Png,
            ImageFormat::Jpeg => ibis_doc_model::MediaFormat::Jpeg,
            ImageFormat::Gif => ibis_doc_model::MediaFormat::Gif,
            ImageFormat::Bmp => ibis_doc_model::MediaFormat::Bmp,
            ImageFormat::Tiff => ibis_doc_model::MediaFormat::Tiff,
            ImageFormat::Svg => ibis_doc_model::MediaFormat::Svg,
            _ => return false,
        };
        self.doc_insert_picture(image.bytes.clone(), format, cx);
        true
    }

    /// 通常の貼り付け (Word Options の貼付の既定動作に従う, DOCX-OPTIONS-006).
    pub(crate) fn paste_text(&mut self, cx: &mut Context<Self>) {
        // A selection of several ranges: the text of the clipboard in each (one line).
        if self.has_extra_selections() {
            return self.paste_plain_text(cx);
        }
        if self.paste_office_shapes(cx) { return; }
        if self.paste_clipboard_image(cx) {
            return;
        }
        match self.default_paste_mode {
            crate::shell::editor::DefaultPasteMode::KeepSourceFormatting => {
                self.paste_keep_source(cx);
            }
            crate::shell::editor::DefaultPasteMode::MergeFormatting => {
                self.paste_match_format(cx);
            }
            crate::shell::editor::DefaultPasteMode::KeepTextOnly => {
                self.paste_plain_text(cx);
            }
        }
    }

    pub(crate) fn paste_plain_text(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else { return };
        self.type_text(&text, cx);
    }

    pub(crate) fn paste_match_format(&mut self, cx: &mut Context<Self>) {
        if let Some((start, end)) = self.ordered_selection() && self.reject_hidden_history_range(start, end, cx) { return; }
        let Some(item) = cx.read_from_clipboard() else { return };
        if self.paste_matching_format(&item, cx) { return; }
        if let Some(text) = item.text() {
            self.type_text(&text, cx);
        }
    }
}

/// How the caret moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Move {
    /// A screen of lines (the number of them), up (negative) or down.
    Page(i32),
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

/// Tables: what the caret is in, and the table commands of the menus and
/// the toolbar.
impl SlideEditor {
    /// The table, row and cell the caret is in (the innermost table).
    pub(crate) fn table_at_caret(&self) -> Option<(u32, usize, usize)> {
        let (_, caret) = self.text_selection()?;
        match flow_command::locate(self.flow()?, caret.para)? {
            (Container::Cell { table, row, cell }, _) => Some((table, row, cell)),
            _ => None,
        }
    }

    /// Insert a table of `rows` × `columns` at the caret, as wide as the
    /// text area (the caret's cell, in a table).
    pub(crate) fn insert_table(&mut self, rows: usize, columns: usize, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let width = if let Some((table, row, cell)) = self.table_at_caret() {
            self.flow()
                .and_then(|d| d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table)))
                .and_then(|t| {
                    let col_idx: usize = t.rows.get(row)?.cells[..cell].iter().map(|c| c.span.max(1) as usize).sum();
                    let span = t.rows.get(row)?.cells.get(cell).map_or(1, |c| c.span.max(1) as usize);
                    let col_w: ibis_doc_model::Emu = t.columns.iter().skip(col_idx).take(span).sum();
                    let margins = t.rows.get(row)?.cells.get(cell).map_or(0, |c| c.margins[1] + c.margins[3]);
                    Some((col_w - margins).max(360_000))
                })
                .unwrap_or_else(|| doc.page.size.width - doc.page.left - doc.page.right)
        } else {
            doc.page.size.width - doc.page.left - doc.page.right
        };
        let first = doc.new_id();
        if self.run_flow(vec![FlowCommand::InsertTable { at: caret, rows, columns, width }], cx) {
            // The caret into the first cell (its paragraph is the table's
            // first id after the table's, row's and cell's).
            let para = first + 2;
            if self.flow().is_some_and(|d| flow_command::paragraph(d, para).is_some()) {
                self.set_caret(FlowPos::new(para, 0), false, cx);
            }
        }
    }

    /// A row, column or cell edit of the caret's table.
    /// The caret's row: whether it repeats as a header and whether it is
    /// kept on one page.
    pub(crate) fn caret_row_flags(&self) -> Option<(bool, bool)> {
        let (table, row, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).map(|r| (r.header, r.cant_split))
    }

    /// The caret's cell vertical alignment (anchor).
    pub(crate) fn caret_cell_anchor(&self) -> Option<ibis_doc_model::CellAnchor> {
        let (table, row, cell) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).and_then(|r| r.cells.get(cell)).map(|c| c.anchor)
    }

    /// The caret's cell text direction.
    pub(crate) fn caret_cell_direction(&self) -> Option<ibis_doc_model::CellDirection> {
        let (table, row, cell) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).and_then(|r| r.cells.get(cell)).map(|c| c.direction)
    }

    /// The caret's cell margins (top, right, bottom, left) in EMU.
    pub(crate) fn caret_cell_margins(&self) -> Option<[ibis_doc_model::Emu; 4]> {
        let (table, row, cell) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).and_then(|r| r.cells.get(cell)).map(|c| c.margins)
    }

    /// The caret's cell no-wrap setting (`w:noWrap`).
    pub(crate) fn caret_cell_no_wrap(&self) -> Option<bool> {
        let (table, row, cell) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        t.rows.get(row).and_then(|r| r.cells.get(cell)).map(|c| c.no_wrap)
    }

    /// Toggle cell text wrapping (`w:noWrap`).
    pub(crate) fn toggle_cell_wrap_text(&mut self, cx: &mut gpui::Context<Self>) {
        let current = self.caret_cell_no_wrap().unwrap_or(false);
        self.edit_table(|r, c, _| ibis_doc_model::TableEdit::NoWrap { cells: vec![(r, c)], no_wrap: !current }, cx);
    }

    /// The caret's table alignment.
    pub(crate) fn caret_table_align(&self) -> Option<ibis_doc_model::FlowAlign> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.align)
    }

    /// The caret's table indent in EMU.
    pub(crate) fn caret_table_indent(&self) -> Option<ibis_doc_model::Emu> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.indent)
    }

    /// The caret's table cell spacing in EMU (None if not in a table; Some(None) if 0/none).
    pub(crate) fn caret_table_cell_spacing(&self) -> Option<Option<ibis_doc_model::Emu>> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.cell_spacing)
    }

    /// The caret's table floating placement (`w:tblpPr`).
    pub(crate) fn caret_table_floating(&self) -> Option<Option<ibis_doc_model::TableFloat>> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.floating)
    }

    /// Set or clear table floating (`w:tblpPr`).
    pub(crate) fn set_table_floating(&mut self, floating: Option<ibis_doc_model::TableFloat>, cx: &mut Context<Self>) {
        self.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Floating(floating), cx);
    }

    /// Set table floating distance from surrounding text (`w:tblpPr` left/right/top/bottomFromText).
    pub(crate) fn set_table_float_distance(&mut self, distance: [ibis_doc_model::Emu; 4], cx: &mut Context<Self>) {
        self.edit_table(move |_, _, _| ibis_doc_model::TableEdit::FloatDistance(distance), cx);
    }

    /// Set table floating horizontal alignment (`w:tblpPr tblpXSpec`).
    pub(crate) fn set_table_float_horz_align(&mut self, align: Option<ibis_doc_model::TableAlign>, cx: &mut Context<Self>) {
        let current = self.caret_table_floating().flatten().unwrap_or_else(ibis_doc_model::TableFloat::standard);
        let mut updated = current;
        updated.x_align = align;
        self.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Floating(Some(updated)), cx);
    }

    /// The caret's table style ID (`w:tblStyle`). None if not in a table; Some(None) if no style.
    pub(crate) fn caret_docx_table_style(&self) -> Option<Option<String>> {
        let (table, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let t = d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table))?;
        Some(t.style.as_ref().map(|s| s.id.clone()))
    }

    /// Set or clear table style (`w:tblStyle`).
    pub(crate) fn set_docx_table_style(&mut self, style_id: Option<String>, cx: &mut Context<Self>) {
        self.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Style(style_id), cx);
    }

    /// Document's table styles list (ID, display name).
    pub(crate) fn document_table_styles(&self) -> Vec<(String, String)> {
        let mut list = Vec::new();
        let mut seen = std::collections::HashSet::new();
        list.push(("TableGrid".to_string(), "docx.editing.table-grid".to_string()));
        seen.insert("TableGrid".to_string());
        if let Some(d) = self.flow() {
            for (id, style) in &d.styles.styles {
                if style.kind == ibis_doc_model::StyleKind::Table && seen.insert(id.clone()) {
                    let name = if style.name.is_empty() { id.clone() } else { style.name.clone() };
                    list.push((id.clone(), name));
                }
            }
        }
        list
    }
    pub(crate) fn edit_table(&mut self, edit: impl FnOnce(usize, usize, usize) -> ibis_doc_model::TableEdit, cx: &mut Context<Self>) {
        let Some((table, row, cell)) = self.table_at_caret() else { return };
        // The cell's grid column.
        let column = self
            .flow()
            .and_then(|d| {
                d.body.iter().chain(d.header.iter()).chain(d.footer.iter()).find_map(|b| find_table(b, table)).map(|t| t.rows[row].cells[..cell].iter().map(|c| c.span.max(1) as usize).sum())
            })
            .unwrap_or(cell);
        let caret = self.text_selection().map(|(_, c)| c);
        self.run_flow(vec![FlowCommand::EditTable { table, edit: edit(row, cell, column) }], cx);
        // The caret stays if its paragraph does; else it goes to the start.
        if let Some(c) = caret
            && self.flow().is_some_and(|d| flow_command::paragraph(d, c.para).is_none())
            && let Some(start) = self.document_edge(false)
        {
            self.set_caret(start, false, cx);
        }
    }

    pub(crate) fn delete_table(&mut self, cx: &mut Context<Self>) {
        let Some((table, _, _)) = self.table_at_caret() else { return };
        self.run_flow(vec![FlowCommand::DeleteTable { table }], cx);
        if let Some(start) = self.document_edge(false) {
            self.set_caret(start, false, cx);
        }
    }

    /// 表内の行を並べ替える (Sort rows by current column).
    pub(crate) fn sort_table(&mut self, ascending: bool, has_header: bool, cx: &mut Context<Self>) {
        self.edit_table(|_row, _cell, column| ibis_doc_model::TableEdit::SortRows { column, ascending, has_header }, cx);
    }

    pub(crate) fn can_select_table(&self) -> bool {
        self.table_at_caret().is_some()
    }

    pub(crate) fn select_table_cell(&mut self, cx: &mut Context<Self>) {
        let Some((table_id, r, c)) = self.table_at_caret() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(table) = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table_id)) else { return };
        let Some(cell) = table.rows.get(r).and_then(|row| row.cells.get(c)) else { return };
        let paras = collect_block_paragraphs(&cell.blocks);
        if let Some((start, end)) = self.paragraph_range(&paras) {
            self.set_caret(start, false, cx);
            self.set_caret(end, true, cx);
        }
    }

    pub(crate) fn select_table_row(&mut self, cx: &mut Context<Self>) {
        let Some((table_id, r, _)) = self.table_at_caret() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(table) = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table_id)) else { return };
        let Some(row) = table.rows.get(r) else { return };
        let paras: Vec<u32> = row.cells.iter().flat_map(|cell| collect_block_paragraphs(&cell.blocks)).collect();
        if let Some((start, end)) = self.paragraph_range(&paras) {
            self.set_caret(start, false, cx);
            self.set_caret(end, true, cx);
        }
    }

    pub(crate) fn select_table_column(&mut self, cx: &mut Context<Self>) {
        let Some((table_id, _, c)) = self.table_at_caret() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(table) = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table_id)) else { return };
        let mut paras = Vec::new();
        for row in &table.rows {
            if let Some(cell) = row.cells.get(c) {
                paras.extend(collect_block_paragraphs(&cell.blocks));
            }
        }
        if let Some((start, end)) = self.paragraph_range(&paras) {
            self.set_caret(start, false, cx);
            self.set_caret(end, true, cx);
        }
    }

    pub(crate) fn select_table(&mut self, cx: &mut Context<Self>) {
        let Some((table_id, _, _)) = self.table_at_caret() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(table) = doc.body.iter().chain(doc.header.iter()).chain(doc.footer.iter()).find_map(|b| find_table(b, table_id)) else { return };
        let paras: Vec<u32> = table.rows.iter().flat_map(|r| r.cells.iter().flat_map(|c| collect_block_paragraphs(&c.blocks))).collect();
        if let Some((start, end)) = self.paragraph_range(&paras) {
            self.set_caret(start, false, cx);
            self.set_caret(end, true, cx);
        }
    }

    fn paragraph_range(&self, paras: &[u32]) -> Option<(FlowPos, FlowPos)> {
        let first = *paras.first()?;
        let last = *paras.last()?;
        let last_len = self.paragraph_of(last)?.len();
        Some((FlowPos::new(first, 0), FlowPos::new(last, last_len)))
    }
}

fn collect_block_paragraphs(blocks: &[Block]) -> Vec<u32> {
    fn walk(blocks: &[Block], out: &mut Vec<u32>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.push(p.id),
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
    walk(blocks, &mut out);
    out
}

/// Table `id` in or under `block`.
fn find_table(block: &Block, id: u32) -> Option<&ibis_doc_model::FlowTable> {
    let Block::Table(t) = block else { return None };
    if t.id == id {
        return Some(t);
    }
    t.rows.iter().flat_map(|r| &r.cells).flat_map(|c| &c.blocks).find_map(|b| find_table(b, id))
}

/// Parse width and height from SVG data (attributes `width`, `height`, or `viewBox`).
pub(crate) fn svg_pixels(data: &[u8]) -> Option<(u32, u32)> {
    let text = std::str::from_utf8(data).ok()?;
    let svg_start = text.find("<svg")?;
    let svg_tag = &text[svg_start..];
    let tag_end = svg_tag.find('>')?;
    let header = &svg_tag[..tag_end];

    let parse_dim = |attr_name: &str| -> Option<f64> {
        let pattern_dq = format!("{attr_name}=\"");
        let pattern_sq = format!("{attr_name}='");
        let (val_str, quote) = if let Some(pos) = header.find(&pattern_dq) {
            (&header[pos + pattern_dq.len()..], '"')
        } else if let Some(pos) = header.find(&pattern_sq) {
            (&header[pos + pattern_sq.len()..], '\'')
        } else {
            return None;
        };
        let end = val_str.find(quote)?;
        let raw = val_str[..end].trim();
        if raw.ends_with('%') {
            return None;
        }
        let num_str = raw.trim_end_matches(|c: char| c.is_alphabetic());
        let val = num_str.parse::<f64>().ok()?;
        if raw.ends_with("pt") {
            Some(val * 96.0 / 72.0)
        } else if raw.ends_with("in") {
            Some(val * 96.0)
        } else if raw.ends_with("mm") {
            Some(val * 96.0 / 25.4)
        } else if raw.ends_with("cm") {
            Some(val * 96.0 / 2.54)
        } else {
            Some(val)
        }
    };

    let w = parse_dim("width");
    let h = parse_dim("height");
    if let (Some(w), Some(h)) = (w, h) {
        if w > 0.0 && h > 0.0 {
            return Some((w.round() as u32, h.round() as u32));
        }
    }

    // Fallback to viewBox: "min-x min-y width height"
    let pattern_dq = "viewBox=\"";
    let pattern_sq = "viewBox='";
    let (val_str, quote) = if let Some(pos) = header.find(pattern_dq) {
        (&header[pos + pattern_dq.len()..], '"')
    } else if let Some(pos) = header.find(pattern_sq) {
        (&header[pos + pattern_sq.len()..], '\'')
    } else {
        return None;
    };
    let end = val_str.find(quote)?;
    let raw = val_str[..end].trim();
    let parts: Vec<f64> = raw
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    if parts.len() == 4 && parts[2] > 0.0 && parts[3] > 0.0 {
        Some((parts[2].round() as u32, parts[3].round() as u32))
    } else {
        None
    }
}

/// The pixel size of a PNG, JPEG, GIF, BMP or SVG picture, from its header.
pub(crate) fn picture_pixels(data: &[u8]) -> Option<(u32, u32)> {
    if data.starts_with(b"<svg") || data.starts_with(b"<?xml") || (data.len() > 4 && data[..1024.min(data.len())].windows(4).any(|w| w == b"<svg")) {
        if let Some((w, h)) = svg_pixels(data) {
            return Some((w, h));
        }
    }
    let be32 = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    if data.starts_with(b"\x89PNG") && data.len() >= 24 {
        return Some((be32(&data[16..20]), be32(&data[20..24])));
    }
    if data.starts_with(b"GIF8") && data.len() >= 10 {
        return Some((u16::from_le_bytes([data[6], data[7]]) as u32, u16::from_le_bytes([data[8], data[9]]) as u32));
    }
    if data.starts_with(b"BM") && data.len() >= 26 {
        let w = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
        let h = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
        return Some((w.unsigned_abs(), h.unsigned_abs()));
    }
    if data.starts_with(&[0xff, 0xd8]) {
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xff {
                return None;
            }
            let marker = data[i + 1];
            let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                return Some((u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32, u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32));
            }
            i += 2 + len;
        }
    }
    None
}

/// Pictures.
impl SlideEditor {
    /// Insert the picture in `data` at the caret: its size at 96 pixels an
    /// inch, made smaller to fit the text's width (as Word does).
    pub(crate) fn doc_insert_picture(&mut self, data: Vec<u8>, format: ibis_doc_model::MediaFormat, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let (w, h) = picture_pixels(&data).unwrap_or((400, 300));
        let (mut cx_emu, mut cy_emu) = (w as i64 * 9525, h as i64 * 9525);
        let room = doc.page.size.width - doc.page.left - doc.page.right;
        if cx_emu > room && cx_emu > 0 {
            cy_emu = cy_emu * room / cx_emu;
            cx_emu = room;
        }
        // Drawn on screen as well as saved.
        let image_format = match format {
            ibis_doc_model::MediaFormat::Png => Some(ImageFormat::Png),
            ibis_doc_model::MediaFormat::Jpeg => Some(ImageFormat::Jpeg),
            ibis_doc_model::MediaFormat::Gif => Some(ImageFormat::Gif),
            ibis_doc_model::MediaFormat::Bmp => Some(ImageFormat::Bmp),
            ibis_doc_model::MediaFormat::Tiff => Some(ImageFormat::Tiff),
            ibis_doc_model::MediaFormat::Svg => Some(ImageFormat::Svg),
            _ => None,
        };
        let media = self.document.ids().add_media(format, data.clone());
        if let Some(f) = image_format {
            self.resources.images.insert(media, Arc::new(Image::from_bytes(f, data)));
        }
        let size = ibis_doc_model::EmuSize::new(cx_emu, cy_emu);
        if self.run_flow(vec![FlowCommand::InsertPicture { at: caret, media, size }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
        }
    }

    /// 画像 > ファイルから…: pick a picture file and insert it.
    pub(crate) fn doc_pick_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_shell.insert")) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let format = ibis_doc_model::MediaFormat::from_path(&path.to_string_lossy());
                this.update(cx, |this, cx| this.doc_insert_picture(data, format, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// Replace the picture at `at` with new image `data`.
    /// If `keep_size` is true, the current size on screen is preserved;
    /// otherwise, the new image's natural dimensions (adjusted to fit page width) are used.
    pub(crate) fn doc_replace_picture(&mut self, at: FlowPos, data: Vec<u8>, format: ibis_doc_model::MediaFormat, keep_size: bool, cx: &mut Context<Self>) -> bool {
        let image_format = match format {
            ibis_doc_model::MediaFormat::Png => Some(ImageFormat::Png),
            ibis_doc_model::MediaFormat::Jpeg => Some(ImageFormat::Jpeg),
            ibis_doc_model::MediaFormat::Gif => Some(ImageFormat::Gif),
            ibis_doc_model::MediaFormat::Bmp => Some(ImageFormat::Bmp),
            ibis_doc_model::MediaFormat::Tiff => Some(ImageFormat::Tiff),
            ibis_doc_model::MediaFormat::Svg => Some(ImageFormat::Svg),
            _ => None,
        };
        let media = self.document.ids().add_media(format, data.clone());
        if let Some(f) = image_format {
            self.resources.images.insert(media, Arc::new(Image::from_bytes(f, data.clone())));
        }
        let size = if keep_size {
            None
        } else {
            let Some(doc) = self.flow() else { return false; };
            let (w, h) = picture_pixels(&data).unwrap_or((400, 300));
            let (mut cx_emu, mut cy_emu) = (w as i64 * 9525, h as i64 * 9525);
            let room = doc.page.size.width - doc.page.left - doc.page.right;
            if cx_emu > room && cx_emu > 0 {
                cy_emu = cy_emu * room / cx_emu;
                cx_emu = room;
            }
            Some(ibis_doc_model::EmuSize::new(cx_emu, cy_emu))
        };
        self.run_flow(vec![FlowCommand::ReplacePicture { at, media, size }], cx)
    }

    /// 図の箇条書き: the paragraphs of the selection become items of a new list whose bullets are the picture `data` (the file's bytes).
    pub(crate) fn apply_picture_bullet(&mut self, data: Vec<u8>, format: ibis_doc_model::MediaFormat, cx: &mut Context<Self>) {
        let Some(doc) = self.flow().cloned() else { return };
        let image_format = match format {
            ibis_doc_model::MediaFormat::Png => Some(ImageFormat::Png),
            ibis_doc_model::MediaFormat::Jpeg => Some(ImageFormat::Jpeg),
            ibis_doc_model::MediaFormat::Gif => Some(ImageFormat::Gif),
            ibis_doc_model::MediaFormat::Bmp => Some(ImageFormat::Bmp),
            ibis_doc_model::MediaFormat::Tiff => Some(ImageFormat::Tiff),
            ibis_doc_model::MediaFormat::Svg => Some(ImageFormat::Svg),
            _ => None,
        };
        if image_format.is_none() {
            return;
        }
        let media = self.document.ids().add_media(format, data.clone());
        if let Some(f) = image_format {
            self.resources.images.insert(media, Arc::new(Image::from_bytes(f, data)));
        }
        let (numbering, id) = doc.styles.numbering.with_new_picture_list(media, None);
        let keep = self.text_selection();
        let mut commands = vec![FlowCommand::SetNumbering { numbering }];
        commands.extend(self.selected_paragraph_format_commands(ParaChange::List(Some(id))));
        self.run_flow(commands, cx);
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = (a != c).then_some(a);
            state.caret = Some(c);
        }
    }

    /// 図の箇条書き…: pick a picture file for the bullets.
    pub(crate) fn doc_pick_picture_bullet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("docx.bullet.picture").to_string().into()) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let format = ibis_doc_model::MediaFormat::from_path(&path.to_string_lossy());
                this.update(cx, |this, cx| this.apply_picture_bullet(data, format, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }

    /// 画像の変更…: pick a picture file and replace the picture at the caret.
    pub(crate) fn doc_pick_replacement_picture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, _, _)) = self.picture_at_caret() else { return };
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, extensions: Vec::new(), multiple: false, prompt: Some(crate::shell::i18n::text("_docx.change-picture").to_string().into()) });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                let Ok(data) = std::fs::read(&path) else { return };
                let format = ibis_doc_model::MediaFormat::from_path(&path.to_string_lossy());
                this.update(cx, |this, cx| this.doc_replace_picture(target, data, format, true, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        let _ = (window, cx);
    }
}

/// ページ罫線のプリセット。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageBorderPreset {
    None,
    BoxThin,
    BoxMedium,
    BoxThick,
    BoxDouble,
    TopBottom,
}

/// Page setup.
impl SlideEditor {
    /// The section the caret is in (the paragraph that ends it; 0 for the
    /// last), in the body or in a section's header or footer.
    pub(crate) fn caret_section(&self) -> u32 {
        let Some((_, caret)) = self.text_selection() else { return 0 };
        let Some(doc) = self.flow() else { return 0 };
        match flow_command::locate(doc, caret.para) {
            Some((Container::Part { section, .. }, _)) => section,
            Some((Container::Header | Container::Footer, _)) | None => 0,
            Some(_) => flow_command::section_of(doc, caret.para),
        }
    }

    /// Change the caret's section's setup (one step of undo).
    pub(crate) fn set_section(&mut self, edit: impl FnOnce(&mut ibis_doc_model::SectionSetup), cx: &mut Context<Self>) {
        let section = self.caret_section();
        let Some(command) = self.flow().and_then(|d| flow_command::set_section(d, section, edit)) else { return };
        self.run_flow(vec![command], cx);
    }

    /// Set the starting page number for the caret's section (`None` to continue
    /// numbering from previous section, `Some(n)` to restart numbering at `n`).
    pub(crate) fn set_page_number_start(&mut self, start: Option<u32>, cx: &mut Context<Self>) {
        self.set_section(
            move |s| match start {
                Some(n) => {
                    s.restart_numbering = true;
                    s.page.first_page = n.max(1);
                }
                None => {
                    s.restart_numbering = false;
                    s.page.first_page = 1;
                }
            },
            cx,
        );
    }

    /// The caret's section's page setup.
    pub(crate) fn caret_page(&self) -> Option<ibis_doc_model::PageSetup> {
        let doc = self.flow()?;
        let section = self.caret_section();
        if section == 0 {
            return Some(doc.page);
        }
        ibis_doc_model::flow_command::section_setup(doc, section).map(|s| s.page)
    }

    /// The caret's section's column settings.
    pub(crate) fn caret_columns(&self) -> ibis_doc_model::PageColumns {
        self.caret_page().map(|p| p.columns).unwrap_or_default()
    }

    /// Change the page setup (the caret's section's).
    pub(crate) fn set_page(&mut self, change: impl FnOnce(&mut ibis_doc_model::PageSetup), cx: &mut Context<Self>) {
        if self.caret_section() != 0 {
            return self.set_section(|s| change(&mut s.page), cx);
        }
        let Some(doc) = self.flow() else { return };
        let mut page = doc.page;
        change(&mut page);
        if page != doc.page {
            self.run_flow(vec![FlowCommand::SetPage { page }], cx);
        }
    }

    /// 複数ページ設定の変更（通常、見開きページ、袋とじ、本（袋とじ）、1枚に2ページ）。
    /// 文書全体の設定として各セクションにも一括適用し、1ステップで Undo/Redo 可能。
    pub(crate) fn set_multiple_pages(&mut self, mp: ibis_doc_model::MultiplePages, sheets: Option<u32>, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let mut state = ibis_doc_model::flow_command::section_state(doc, &ibis_doc_model::flow_command::section_paragraphs(doc));
        let is_mirrored = mp.is_mirrored();
        state.page.multiple_pages = mp;
        state.page.book_fold_sheets = sheets;
        state.page.mirror_margins = is_mirrored;
        for (_, _, setup) in state.breaks.iter_mut() {
            if let Some(setup) = setup {
                let setup = std::sync::Arc::make_mut(setup);
                setup.page.multiple_pages = mp;
                setup.page.book_fold_sheets = sheets;
                setup.page.mirror_margins = is_mirrored;
            }
        }
        self.run_flow(vec![FlowCommand::SetSections { state: Box::new(state) }], cx);
    }

    /// 現在の複数ページ設定を取得。
    pub(crate) fn caret_multiple_pages(&self) -> ibis_doc_model::MultiplePages {
        self.caret_page().map(|p| p.multiple_pages).unwrap_or_default()
    }

    /// 現在の袋とじ印刷枚数を取得。
    pub(crate) fn caret_book_fold_sheets(&self) -> Option<u32> {
        self.caret_page().and_then(|p| p.book_fold_sheets)
    }

    /// 行グリッドの行送り（None でグリッドなし）。
    pub(crate) fn set_line_pitch(&mut self, pitch: Option<ibis_doc_model::Emu>, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.line_pitch = pitch;
                p.doc_grid_type = if pitch.is_some() {
                    if p.doc_grid_type == ibis_doc_model::DocGridType::Default {
                        ibis_doc_model::DocGridType::Lines
                    } else {
                        p.doc_grid_type
                    }
                } else {
                    ibis_doc_model::DocGridType::Default
                };
            },
            cx,
        );
    }

    /// 行数・文字数の原稿設定（w:docGrid）。
    pub(crate) fn set_doc_grid(
        &mut self,
        grid_type: ibis_doc_model::DocGridType,
        line_pitch: Option<ibis_doc_model::Emu>,
        char_space: Option<ibis_doc_model::Emu>,
        cx: &mut Context<Self>,
    ) {
        self.set_page(
            |p| {
                p.doc_grid_type = grid_type;
                p.line_pitch = line_pitch;
                p.char_space = char_space;
            },
            cx,
        );
    }

    /// 段組の段数（1..=3 または最大まで）。
    pub(crate) fn set_columns(&mut self, count: u8, cx: &mut Context<Self>) {
        let count = count.clamp(1, ibis_doc_model::PageColumns::MAX as u8);
        self.set_page(
            |p| {
                p.columns.count = count;
                p.columns.widths = None;
                if count == 1 {
                    p.columns.separator = false;
                }
                if p.columns.space == 0 {
                    p.columns.space = 720 * 635;
                }
            },
            cx,
        );
    }

    /// 段間境界線（w:sep）の切り替え。
    pub(crate) fn toggle_column_separator(&mut self, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.columns.separator = !p.columns.separator;
            },
            cx,
        );
    }

    /// 段間境界線（w:sep）の設定。
    #[allow(dead_code)]
    pub(crate) fn set_column_separator(&mut self, separator: bool, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.columns.separator = separator;
            },
            cx,
        );
    }

    /// ページ罫線のプリセット適用。
    pub(crate) fn set_page_border_preset(&mut self, preset: PageBorderPreset, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                use ibis_doc_model::{BorderStyle, FlowBorder, Rgb};
                const PT: i64 = 12_700;
                let default_space = if p.borders_from_text { PT } else { 24 * PT };
                let border_of = |width: i64, style: BorderStyle| FlowBorder {
                    width,
                    color: Rgb::BLACK,
                    style,
                    space: default_space,
                    art: None,
                };
                match preset {
                    PageBorderPreset::None => {
                        p.borders = [None; 4];
                    }
                    PageBorderPreset::BoxThin => {
                        let b = Some(border_of(PT / 2, BorderStyle::Single));
                        p.borders = [b, b, b, b];
                    }
                    PageBorderPreset::BoxMedium => {
                        let b = Some(border_of(PT, BorderStyle::Single));
                        p.borders = [b, b, b, b];
                    }
                    PageBorderPreset::BoxThick => {
                        let b = Some(border_of(2 * PT, BorderStyle::Single));
                        p.borders = [b, b, b, b];
                    }
                    PageBorderPreset::BoxDouble => {
                        let b = Some(border_of(PT * 3 / 2, BorderStyle::Double));
                        p.borders = [b, b, b, b];
                    }
                    PageBorderPreset::TopBottom => {
                        let b = Some(border_of(PT, BorderStyle::Single));
                        p.borders = [b, None, b, None];
                    }
                }
            },
            cx,
        );
    }

    /// ページ罫線の基準位置（本文から / 用紙の端から）の設定。
    pub(crate) fn set_page_border_offset_from(&mut self, from_text: bool, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                const PT: i64 = 12_700;
                p.borders_from_text = from_text;
                let space = if from_text { PT } else { 24 * PT };
                for b in p.borders.iter_mut().flatten() {
                    b.space = space;
                }
            },
            cx,
        );
    }

    /// 現在のページ罫線プリセットを取得。
    pub(crate) fn page_border_preset(&self) -> PageBorderPreset {
        let Some(p) = self.caret_page() else { return PageBorderPreset::None };
        use ibis_doc_model::BorderStyle;
        const PT: i64 = 12_700;
        let [t, r, b, l] = p.borders;
        if t.is_none() && r.is_none() && b.is_none() && l.is_none() {
            return PageBorderPreset::None;
        }
        if t == b && r.is_none() && l.is_none() && t.is_some() {
            return PageBorderPreset::TopBottom;
        }
        if t == r && r == b && b == l {
            if let Some(border) = t {
                if border.style == BorderStyle::Double {
                    return PageBorderPreset::BoxDouble;
                }
                if border.width <= PT * 3 / 4 {
                    return PageBorderPreset::BoxThin;
                }
                if border.width <= PT * 5 / 4 {
                    return PageBorderPreset::BoxMedium;
                }
                return PageBorderPreset::BoxThick;
            }
        }
        PageBorderPreset::None
    }

    /// Portrait or landscape, keeping the paper.
    pub(crate) fn set_orientation(&mut self, landscape: bool, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                let (w, h) = (p.size.width.min(p.size.height), p.size.width.max(p.size.height));
                p.size = if landscape { ibis_doc_model::EmuSize::new(h, w) } else { ibis_doc_model::EmuSize::new(w, h) };
            },
            cx,
        );
    }

    /// A paper size (twips, portrait), keeping the orientation.
    pub(crate) fn set_paper(&mut self, width: i64, height: i64, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                let landscape = p.size.width > p.size.height;
                let (w, h) = (width * 635, height * 635);
                p.size = if landscape { ibis_doc_model::EmuSize::new(h, w) } else { ibis_doc_model::EmuSize::new(w, h) };
            },
            cx,
        );
    }

    /// Margins: top, right, bottom, left (twips).
    pub(crate) fn set_margins(&mut self, [t, r, b, l]: [i64; 4], cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.top = t * 635;
                p.right = r * 635;
                p.bottom = b * 635;
                p.left = l * 635;
            },
            cx,
        );
    }

    /// Text direction: across (LrTb) or vertical (TbRl).
    pub(crate) fn set_text_direction(&mut self, vertical: bool, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.direction = if vertical { ibis_doc_model::CellDirection::TbRl } else { ibis_doc_model::CellDirection::LrTb };
            },
            cx,
        );
    }
    /// Line numbering restart mode (`None` for off).
    pub(crate) fn set_line_numbers(&mut self, restart: Option<ibis_doc_model::LineNumberRestart>, cx: &mut Context<Self>) {
        self.set_page(
            |p| {
                p.line_numbers = restart.map(|r| {
                    let mut ln = p.line_numbers.unwrap_or(ibis_doc_model::LineNumbers {
                        count_by: 1,
                        start: 1,
                        distance: 0,
                        restart: r,
                    });
                    ln.restart = r;
                    ln
                });
            },
            cx,
        );
    }

    /// ページの色 (`w:background`).
    pub(crate) fn set_page_color(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        if self.flow().is_some_and(|d| d.page_color == color) {
            return;
        }
        self.run_flow(vec![ibis_doc_model::FlowCommand::SetPageColor(color)], cx);
    }

    /// ドキュメント全体の段落の間隔のプリセット.
    pub(crate) fn document_paragraph_spacing_preset(&self) -> Option<ibis_doc_model::ParagraphSpacingPreset> {
        self.flow().and_then(|d| d.paragraph_spacing_preset)
    }

    pub(crate) fn set_document_paragraph_spacing(&mut self, preset: ibis_doc_model::ParagraphSpacingPreset, cx: &mut Context<Self>) {
        self.run_flow(vec![ibis_doc_model::FlowCommand::SetDocumentParagraphSpacing { preset: Some(preset), styles: None }], cx);
    }

    /// ドキュメント全体のスタイル セットのプリセット.
    pub(crate) fn document_style_set_preset(&self) -> Option<ibis_doc_model::StyleSetPreset> {
        self.flow().and_then(|d| d.style_set_preset)
    }

    pub(crate) fn set_document_style_set(&mut self, preset: ibis_doc_model::StyleSetPreset, cx: &mut Context<Self>) {
        self.run_flow(vec![ibis_doc_model::FlowCommand::SetDocumentStyleSet { preset: Some(preset), styles: None }], cx);
    }

    /// ドキュメントのテーマ色のプリセットID（DOCX-STYLE-013）。
    pub(crate) fn current_docx_theme_colors_preset(&self) -> Option<&'static str> {
        let colors = &self.presentation().theme_colors;
        if colors.is_empty() {
            return Some("office");
        }
        for preset in &DOCX_THEME_COLORS_PRESETS {
            let matched = preset.colors.iter().all(|(name, rgb)| {
                colors
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .is_some_and(|(_, c)| *c == *rgb)
            });
            if matched {
                return Some(preset.id);
            }
        }
        None
    }

    pub(crate) fn is_docx_theme_colors_active(&self, preset_id: &str) -> bool {
        self.current_docx_theme_colors_preset() == Some(preset_id)
    }

    /// ドキュメントのテーマ色を適用（DOCX-STYLE-013）。
    pub(crate) fn apply_docx_theme_colors(&mut self, preset_id: &str, cx: &mut Context<Self>) {
        let Some(preset) = DOCX_THEME_COLORS_PRESETS.iter().find(|p| p.id == preset_id) else { return };
        let mut colors = self.presentation().theme_colors.clone();
        for (name, rgb) in preset.colors {
            if let Some(entry) = colors.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                entry.1 = rgb;
            } else {
                colors.push((name.to_string(), rgb));
            }
        }
        let fonts = self.presentation().theme_fonts.clone();
        let _ = self.document.execute(vec![ibis_doc_model::Command::SetTheme { colors, fonts }]);
        self.sync_document_peers(cx);
        self.status = Some(crate::shell::i18n::format("docx.design.theme-colors.applied", &[("name", &crate::shell::i18n::text(preset.label_key))]).into());
        cx.notify();
    }

    /// ドキュメントの文書テーマ（色・フォント・効果の一括設定）のプリセットID（DOCX-STYLE-012）。
    pub(crate) fn current_docx_theme_preset(&self) -> Option<&'static str> {
        let color_id = self.current_docx_theme_colors_preset()?;
        let font_id = self.current_docx_theme_fonts_preset().unwrap_or("office");
        for preset in &DOCX_DOCUMENT_THEME_PRESETS {
            if preset.color_id == color_id && preset.font_id == font_id && self.is_docx_theme_effects_active(preset.effect_id) {
                return Some(preset.id);
            }
        }
        for preset in &DOCX_DOCUMENT_THEME_PRESETS {
            if preset.color_id == color_id && preset.font_id == font_id {
                return Some(preset.id);
            }
        }
        None
    }

    pub(crate) fn is_docx_theme_active(&self, theme_id: &str) -> bool {
        self.current_docx_theme_preset() == Some(theme_id)
    }

    /// ドキュメントの文書テーマ（色・フォント・効果）を一括適用（DOCX-STYLE-012、1ステップUndo/Redo）。
    pub(crate) fn apply_docx_theme(&mut self, theme_id: &str, cx: &mut Context<Self>) {
        let Some(theme) = DOCX_DOCUMENT_THEME_PRESETS.iter().find(|t| t.id == theme_id) else { return };
        let Some(color_preset) = DOCX_THEME_COLORS_PRESETS.iter().find(|p| p.id == theme.color_id) else { return };
        let Some(font_preset) = DOCX_THEME_FONTS_PRESETS.iter().find(|p| p.id == theme.font_id) else { return };
        let mut colors = self.presentation().theme_colors.clone();
        for (name, rgb) in color_preset.colors {
            if let Some(entry) = colors.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                entry.1 = rgb;
            } else {
                colors.push((name.to_string(), rgb));
            }
        }
        let fonts = ibis_doc_model::ThemeFonts {
            major: ibis_doc_model::FontSet {
                latin: Some(font_preset.major_latin.to_string()),
                east_asian: Some(font_preset.major_ea.to_string()),
                complex: None,
            },
            minor: ibis_doc_model::FontSet {
                latin: Some(font_preset.minor_latin.to_string()),
                east_asian: Some(font_preset.minor_ea.to_string()),
                complex: None,
            },
        };
        let mut cmds = vec![ibis_doc_model::Command::SetTheme { colors, fonts: Some(fonts) }];
        if let Some(effect_preset) = ibis_pptx::theme_effects::find_effect_preset(theme.effect_id) {
            cmds.push(ibis_doc_model::Command::SetThemeEffects { name: Some(effect_preset.name.to_string()) });
        }
        let _ = self.document.execute(cmds);
        self.sync_document_peers(cx);
        self.status = Some(crate::shell::i18n::format("docx.design.themes.applied", &[("name", &crate::shell::i18n::text(theme.label_key))]).into());
        cx.notify();
    }

    /// テーマの色とフォントのカスタマイズ パネルを開く（DOCX-STYLE-012, DOCX-STYLE-013）。
    pub(crate) fn open_docx_theme_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.theme_colors_ui.editing=false;
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        use gpui_kit::component::input::InputState;
        let p = self.presentation();
        let have = p.theme_colors.clone();
        let default_colors = DOCX_THEME_COLORS_PRESETS[0].colors;
        let colors = crate::pptx::theme_panel::COLORS
            .iter()
            .map(|(name, label)| {
                let rgb = have
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, c)| *c)
                    .or_else(|| default_colors.iter().find(|(n, _)| *n == *name).map(|(_, c)| *c))
                    .unwrap_or(ibis_doc_model::Rgb(0x000000));
                let input = cx.new(|cx| InputState::new(window, cx).default_value(format!("#{:06X}", rgb.0 & 0xffffff)));
                (*name, *label, input)
            })
            .collect::<Vec<_>>();
        let fonts = p.theme_fonts.clone().unwrap_or_default();
        let font = |window: &mut Window, cx: &mut Context<Self>, v: &Option<String>, fallback: &str| {
            let val = v.clone().unwrap_or_else(|| fallback.to_string());
            cx.new(|cx| InputState::new(window, cx).default_value(val))
        };
        let font_inputs = [
            font(window, cx, &fonts.major.latin, "Aptos Display"),
            font(window, cx, &fonts.major.east_asian, "Yu Gothic"),
            font(window, cx, &fonts.minor.latin, "Aptos"),
            font(window, cx, &fonts.minor.east_asian, "Yu Gothic"),
        ];
        if let Some(s) = self.doc_state_mut() {
            s.theme_panel = Some(DocxThemePanel { colors, fonts: font_inputs, error: None });
            s.compatibility_panel = None;
            s.inspect_panel = None;
            s.docx_math = None;
            s.quick_parts = None;
            s.table_formula = None;
            s.table_size = None;
            s.picture_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.style_edit = None;
            s.link = None;
            s.find = None;
            s.comments = None;
            s.list_style = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_docx_theme_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.theme_panel = None;
        }
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn apply_docx_theme_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.doc_state().and_then(|s| s.theme_panel.as_ref()) else { return };
        let parse_hex = |text: &str| -> Option<ibis_doc_model::Rgb> {
            let t = text.trim().trim_start_matches('#');
            (t.len() == 6).then(|| u32::from_str_radix(t, 16).ok()).flatten().map(ibis_doc_model::Rgb)
        };
        let mut colors = self.presentation().theme_colors.clone();
        for (name, _, input) in &panel.colors {
            let Some(rgb) = parse_hex(input.read(cx).value().as_ref()) else {
                if let Some(p) = self.doc_state_mut().and_then(|s| s.theme_panel.as_mut()) {
                    p.error = Some(crate::shell::i18n::text("pptx.theme-panel.enter-colors-rrggbb").into());
                }
                return cx.notify();
            };
            if let Some(entry) = colors.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                entry.1 = rgb;
            } else {
                colors.push((name.to_string(), rgb));
            }
        }
        let text = |i: usize| Some(panel.fonts[i].read(cx).value().trim().to_string()).filter(|t| !t.is_empty());
        let old = self.presentation().theme_fonts.clone();
        let fonts = ibis_doc_model::ThemeFonts {
            major: ibis_doc_model::FontSet {
                latin: text(0),
                east_asian: text(1),
                complex: old.as_ref().and_then(|f| f.major.complex.clone()),
            },
            minor: ibis_doc_model::FontSet {
                latin: text(2),
                east_asian: text(3),
                complex: old.as_ref().and_then(|f| f.minor.complex.clone()),
            },
        };
        let fonts = Some(fonts).filter(|f| old.is_some() || *f != ibis_doc_model::ThemeFonts::default());
        let _ = self.document.execute(vec![ibis_doc_model::Command::SetTheme { colors, fonts }]);
        self.sync_document_peers(cx);
        self.status = Some(crate::shell::i18n::text("docx.design.theme-colors.custom-applied"));
        self.close_docx_theme_panel(window, cx);
    }

    pub(crate) fn docx_theme_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        self.shared_theme_colors_panel(cx)
    }

    /// ドキュメントのテーマ フォントのプリセットID.
    pub(crate) fn current_docx_theme_fonts_preset(&self) -> Option<&'static str> {
        let fonts = self.presentation().theme_fonts.as_ref()?;
        let major = fonts.major.latin.as_deref().unwrap_or("");
        let minor = fonts.minor.latin.as_deref().unwrap_or("");
        for preset in &DOCX_THEME_FONTS_PRESETS {
            if preset.major_latin.eq_ignore_ascii_case(major) && preset.minor_latin.eq_ignore_ascii_case(minor) {
                return Some(preset.id);
            }
        }
        None
    }

    /// ドキュメントのテーマ フォントを適用.
    pub(crate) fn apply_docx_theme_fonts(&mut self, preset_id: &str, cx: &mut Context<Self>) {
        let Some(preset) = DOCX_THEME_FONTS_PRESETS.iter().find(|p| p.id == preset_id) else { return };
        let colors = self.presentation().theme_colors.clone();
        let fonts = ibis_doc_model::ThemeFonts {
            major: ibis_doc_model::FontSet {
                latin: Some(preset.major_latin.to_string()),
                east_asian: Some(preset.major_ea.to_string()),
                complex: None,
            },
            minor: ibis_doc_model::FontSet {
                latin: Some(preset.minor_latin.to_string()),
                east_asian: Some(preset.minor_ea.to_string()),
                complex: None,
            },
        };
        let _ = self.document.execute(vec![ibis_doc_model::Command::SetTheme { colors, fonts: Some(fonts) }]);
        self.sync_document_peers(cx);
        cx.notify();
    }

    /// ドキュメントのテーマ効果プリセットID (または名前).
    pub(crate) fn current_docx_theme_effects(&self) -> Option<&str> {
        self.presentation().theme_effects.as_deref()
    }

    /// テーマ効果プリセットが現在アクティブかどうか.
    pub(crate) fn is_docx_theme_effects_active(&self, preset_id: &str) -> bool {
        let current = self.presentation().theme_effects.as_deref().unwrap_or("Office");
        let Some(preset) = ibis_pptx::theme_effects::find_effect_preset(preset_id) else { return false };
        current.eq_ignore_ascii_case(preset.id) || current.eq_ignore_ascii_case(preset.name)
    }

    /// ドキュメントのテーマ効果を適用.
    pub(crate) fn apply_docx_theme_effects(&mut self, preset_id: &str, cx: &mut Context<Self>) {
        let Some(preset) = ibis_pptx::theme_effects::find_effect_preset(preset_id) else { return };
        let _ = self.document.execute(vec![ibis_doc_model::Command::SetThemeEffects { name: Some(preset.name.to_string()) }]);
        self.sync_document_peers(cx);
        self.status = Some(crate::shell::i18n::format("docx.design.effects.applied", &[("name", &crate::shell::i18n::text(preset.label_key))]).into());
        cx.notify();
    }

    /// ドキュメントの書式設定（テーマ、フォント、段落の間隔）を新しい文書の既定として保存（docx.design.set-as-default）。
    pub(crate) fn set_docx_design_as_default(&mut self, cx: &mut Context<Self>) {
        let font_id = self.current_docx_theme_fonts_preset().unwrap_or("office");
        crate::shell::settings::set("docx.default.theme_fonts", font_id);

        let ps = self.document_paragraph_spacing_preset().unwrap_or(ibis_doc_model::ParagraphSpacingPreset::Default);
        let s = match ps {
            ibis_doc_model::ParagraphSpacingPreset::Default => "default",
            ibis_doc_model::ParagraphSpacingPreset::NoSpace => "none",
            ibis_doc_model::ParagraphSpacingPreset::Compact => "compact",
            ibis_doc_model::ParagraphSpacingPreset::Tight => "tight",
            ibis_doc_model::ParagraphSpacingPreset::Open => "open",
            ibis_doc_model::ParagraphSpacingPreset::Relaxed => "relaxed",
            ibis_doc_model::ParagraphSpacingPreset::Double => "double",
        };
        crate::shell::settings::set("docx.default.paragraph_spacing", s);

        let colors = &self.presentation().theme_colors;
        if !colors.is_empty() {
            let serialized: String = colors
                .iter()
                .map(|(k, c)| format!("{}:{:06x}", k, c.0))
                .collect::<Vec<_>>()
                .join(";");
            crate::shell::settings::set("docx.default.theme_colors", &serialized);
        }

        let style_set = self.document_style_set_preset().unwrap_or(ibis_doc_model::StyleSetPreset::Default);
        crate::shell::settings::set("docx.default.style_set", style_set.as_str());

        let effects_id = self.current_docx_theme_effects().unwrap_or("Office");
        crate::shell::settings::set("docx.default.theme_effects", effects_id);

        self.status = Some(crate::shell::i18n::text("docx.design.set-as-default.done"));
        cx.notify();
    }

    /// 新規 Word 文書に既定のドキュメント書式設定（フォント、段落の間隔、テーマ色、スタイルセット、効果）を適用。
    pub(crate) fn apply_docx_default_design(&mut self, cx: &mut Context<Self>) {
        if let Some(font_id) = crate::shell::settings::get("docx.default.theme_fonts") {
            self.apply_docx_theme_fonts(&font_id, cx);
        }
        if let Some(ps_str) = crate::shell::settings::get("docx.default.paragraph_spacing") {
            let preset = match ps_str.as_str() {
                "none" => Some(ibis_doc_model::ParagraphSpacingPreset::NoSpace),
                "compact" => Some(ibis_doc_model::ParagraphSpacingPreset::Compact),
                "tight" => Some(ibis_doc_model::ParagraphSpacingPreset::Tight),
                "open" => Some(ibis_doc_model::ParagraphSpacingPreset::Open),
                "relaxed" => Some(ibis_doc_model::ParagraphSpacingPreset::Relaxed),
                "double" => Some(ibis_doc_model::ParagraphSpacingPreset::Double),
                "default" => Some(ibis_doc_model::ParagraphSpacingPreset::Default),
                _ => None,
            };
            if let Some(preset) = preset {
                self.set_document_paragraph_spacing(preset, cx);
            }
        }
        if let Some(set_str) = crate::shell::settings::get("docx.default.style_set") {
            if let Some(set) = ibis_doc_model::StyleSetPreset::from_str(&set_str) {
                self.set_document_style_set(set, cx);
            }
        }
        if let Some(effects_id) = crate::shell::settings::get("docx.default.theme_effects") {
            self.apply_docx_theme_effects(&effects_id, cx);
        }
        if let Some(colors_str) = crate::shell::settings::get("docx.default.theme_colors") {
            let mut colors = Vec::new();
            for part in colors_str.split(';').filter(|s| !s.is_empty()) {
                if let Some((name, hex)) = part.split_once(':') {
                    if let Ok(val) = u32::from_str_radix(hex, 16) {
                        colors.push((name.to_string(), ibis_doc_model::Rgb(val)));
                    }
                }
            }
            if !colors.is_empty() {
                let fonts = self.presentation().theme_fonts.clone();
                let _ = self.document.execute(vec![ibis_doc_model::Command::SetTheme { colors, fonts }]);
            }
        }
        self.document.reset_history();
        self.sync_document_peers(cx);
        cx.notify();
    }
}

pub(crate) struct DocxThemePanel {
    pub(crate) colors: Vec<(&'static str, &'static str, Entity<gpui_kit::component::input::InputState>)>,
    pub(crate) fonts: [Entity<gpui_kit::component::input::InputState>; 4],
    pub(crate) error: Option<SharedString>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DocxDocumentThemePreset {
    pub(crate) id: &'static str,
    pub(crate) action_id: &'static str,
    pub(crate) label_key: &'static str,
    pub(crate) color_id: &'static str,
    pub(crate) font_id: &'static str,
    pub(crate) effect_id: &'static str,
}

pub(crate) const DOCX_DOCUMENT_THEME_PRESETS: [DocxDocumentThemePreset; 10] = [
    DocxDocumentThemePreset { id: "office", action_id: "docx.design.themes.office", label_key: "docx.design.themes.office", color_id: "office", font_id: "office", effect_id: "office" },
    DocxDocumentThemePreset { id: "office_2013_2022", action_id: "docx.design.themes.office_2013_2022", label_key: "docx.design.themes.office_2013_2022", color_id: "office", font_id: "calibri", effect_id: "office" },
    DocxDocumentThemePreset { id: "office_2007_2010", action_id: "docx.design.themes.office_2007_2010", label_key: "docx.design.themes.office_2007_2010", color_id: "office_2007_2010", font_id: "calibri", effect_id: "office_2007_2010" },
    DocxDocumentThemePreset { id: "facet", action_id: "docx.design.themes.facet", label_key: "docx.design.themes.facet", color_id: "green", font_id: "century-gothic", effect_id: "metro" },
    DocxDocumentThemePreset { id: "gallery", action_id: "docx.design.themes.gallery", label_key: "docx.design.themes.gallery", color_id: "violet", font_id: "arial", effect_id: "glossy" },
    DocxDocumentThemePreset { id: "integral", action_id: "docx.design.themes.integral", label_key: "docx.design.themes.integral", color_id: "blue_green", font_id: "century-gothic", effect_id: "slate" },
    DocxDocumentThemePreset { id: "ion", action_id: "docx.design.themes.ion", label_key: "docx.design.themes.ion", color_id: "red_orange", font_id: "century-gothic", effect_id: "bubbles" },
    DocxDocumentThemePreset { id: "organic", action_id: "docx.design.themes.organic", label_key: "docx.design.themes.organic", color_id: "median", font_id: "times", effect_id: "median" },
    DocxDocumentThemePreset { id: "retrospect", action_id: "docx.design.themes.retrospect", label_key: "docx.design.themes.retrospect", color_id: "orange", font_id: "calibri", effect_id: "aspect" },
    DocxDocumentThemePreset { id: "slice", action_id: "docx.design.themes.slice", label_key: "docx.design.themes.slice", color_id: "blue", font_id: "arial", effect_id: "apex" },
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct DocxThemeColorsPreset {
    pub(crate) id: &'static str,
    pub(crate) action_id: &'static str,
    pub(crate) label_key: &'static str,
    pub(crate) colors: [(&'static str, ibis_doc_model::Rgb); 12],
}

pub(crate) const DOCX_THEME_COLORS_PRESETS: [DocxThemeColorsPreset; 12] = [
    DocxThemeColorsPreset {
        id: "office",
        action_id: "docx.design.theme-colors.office",
        label_key: "docx.design.theme-colors.office",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x44546a)), ("lt2", ibis_doc_model::Rgb(0xe7e6e6)),
            ("accent1", ibis_doc_model::Rgb(0x4472c4)), ("accent2", ibis_doc_model::Rgb(0xed7d31)),
            ("accent3", ibis_doc_model::Rgb(0xa5a5a5)), ("accent4", ibis_doc_model::Rgb(0xffc000)),
            ("accent5", ibis_doc_model::Rgb(0x5b9bd5)), ("accent6", ibis_doc_model::Rgb(0x70ad47)),
            ("hlink", ibis_doc_model::Rgb(0x0563c1)), ("folHlink", ibis_doc_model::Rgb(0x954f72)),
        ],
    },
    DocxThemeColorsPreset {
        id: "office_2007_2010",
        action_id: "docx.design.theme-colors.office_2007_2010",
        label_key: "docx.design.theme-colors.office_2007_2010",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x1f497d)), ("lt2", ibis_doc_model::Rgb(0xeeece1)),
            ("accent1", ibis_doc_model::Rgb(0x4f81bd)), ("accent2", ibis_doc_model::Rgb(0xc0504d)),
            ("accent3", ibis_doc_model::Rgb(0x9bbb59)), ("accent4", ibis_doc_model::Rgb(0x8064a2)),
            ("accent5", ibis_doc_model::Rgb(0x4bacc6)), ("accent6", ibis_doc_model::Rgb(0xf79646)),
            ("hlink", ibis_doc_model::Rgb(0x0000ff)), ("folHlink", ibis_doc_model::Rgb(0x800080)),
        ],
    },
    DocxThemeColorsPreset {
        id: "grayscale",
        action_id: "docx.design.theme-colors.grayscale",
        label_key: "docx.design.theme-colors.grayscale",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x333333)), ("lt2", ibis_doc_model::Rgb(0xe6e6e6)),
            ("accent1", ibis_doc_model::Rgb(0x595959)), ("accent2", ibis_doc_model::Rgb(0x7f7f7f)),
            ("accent3", ibis_doc_model::Rgb(0x999999)), ("accent4", ibis_doc_model::Rgb(0xb2b2b2)),
            ("accent5", ibis_doc_model::Rgb(0xcccccc)), ("accent6", ibis_doc_model::Rgb(0xd9d9d9)),
            ("hlink", ibis_doc_model::Rgb(0x404040)), ("folHlink", ibis_doc_model::Rgb(0x737373)),
        ],
    },
    DocxThemeColorsPreset {
        id: "blue_warm",
        action_id: "docx.design.theme-colors.blue_warm",
        label_key: "docx.design.theme-colors.blue_warm",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x232d4b)), ("lt2", ibis_doc_model::Rgb(0xe8edf5)),
            ("accent1", ibis_doc_model::Rgb(0x4a66ac)), ("accent2", ibis_doc_model::Rgb(0x629dd1)),
            ("accent3", ibis_doc_model::Rgb(0x297fd5)), ("accent4", ibis_doc_model::Rgb(0x7f8fa9)),
            ("accent5", ibis_doc_model::Rgb(0x5a6e85)), ("accent6", ibis_doc_model::Rgb(0x8898ab)),
            ("hlink", ibis_doc_model::Rgb(0x2b579a)), ("folHlink", ibis_doc_model::Rgb(0x604878)),
        ],
    },
    DocxThemeColorsPreset {
        id: "blue",
        action_id: "docx.design.theme-colors.blue",
        label_key: "docx.design.theme-colors.blue",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x263859)), ("lt2", ibis_doc_model::Rgb(0xe8eef5)),
            ("accent1", ibis_doc_model::Rgb(0x386fa4)), ("accent2", ibis_doc_model::Rgb(0x59a5d8)),
            ("accent3", ibis_doc_model::Rgb(0x84d2f6)), ("accent4", ibis_doc_model::Rgb(0x91e5f6)),
            ("accent5", ibis_doc_model::Rgb(0x133c55)), ("accent6", ibis_doc_model::Rgb(0x588b8b)),
            ("hlink", ibis_doc_model::Rgb(0x1a759f)), ("folHlink", ibis_doc_model::Rgb(0x184e77)),
        ],
    },
    DocxThemeColorsPreset {
        id: "blue_green",
        action_id: "docx.design.theme-colors.blue_green",
        label_key: "docx.design.theme-colors.blue_green",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x1e3f47)), ("lt2", ibis_doc_model::Rgb(0xe6f2f5)),
            ("accent1", ibis_doc_model::Rgb(0x3494ba)), ("accent2", ibis_doc_model::Rgb(0x58b6c0)),
            ("accent3", ibis_doc_model::Rgb(0x75bda7)), ("accent4", ibis_doc_model::Rgb(0x7a8c8e)),
            ("accent5", ibis_doc_model::Rgb(0x84acb6)), ("accent6", ibis_doc_model::Rgb(0x2683c6)),
            ("hlink", ibis_doc_model::Rgb(0x1f7a8c)), ("folHlink", ibis_doc_model::Rgb(0x4b6584)),
        ],
    },
    DocxThemeColorsPreset {
        id: "green",
        action_id: "docx.design.theme-colors.green",
        label_key: "docx.design.theme-colors.green",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x29421e)), ("lt2", ibis_doc_model::Rgb(0xebf2e6)),
            ("accent1", ibis_doc_model::Rgb(0x549e39)), ("accent2", ibis_doc_model::Rgb(0x8ab833)),
            ("accent3", ibis_doc_model::Rgb(0xc0cf3a)), ("accent4", ibis_doc_model::Rgb(0x029676)),
            ("accent5", ibis_doc_model::Rgb(0x4ab5c4)), ("accent6", ibis_doc_model::Rgb(0x0989b1)),
            ("hlink", ibis_doc_model::Rgb(0x3b7a26)), ("folHlink", ibis_doc_model::Rgb(0x5a734c)),
        ],
    },
    DocxThemeColorsPreset {
        id: "orange",
        action_id: "docx.design.theme-colors.orange",
        label_key: "docx.design.theme-colors.orange",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x52311c)), ("lt2", ibis_doc_model::Rgb(0xf7eee8)),
            ("accent1", ibis_doc_model::Rgb(0xe48312)), ("accent2", ibis_doc_model::Rgb(0xbd582c)),
            ("accent3", ibis_doc_model::Rgb(0x865640)), ("accent4", ibis_doc_model::Rgb(0x9b8357)),
            ("accent5", ibis_doc_model::Rgb(0xc2bc80)), ("accent6", ibis_doc_model::Rgb(0x94a088)),
            ("hlink", ibis_doc_model::Rgb(0xc65911)), ("folHlink", ibis_doc_model::Rgb(0x833c0c)),
        ],
    },
    DocxThemeColorsPreset {
        id: "red_orange",
        action_id: "docx.design.theme-colors.red_orange",
        label_key: "docx.design.theme-colors.red_orange",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x4f211c)), ("lt2", ibis_doc_model::Rgb(0xf5e8e6)),
            ("accent1", ibis_doc_model::Rgb(0xd34817)), ("accent2", ibis_doc_model::Rgb(0x9b2d1f)),
            ("accent3", ibis_doc_model::Rgb(0xa28e6a)), ("accent4", ibis_doc_model::Rgb(0x956251)),
            ("accent5", ibis_doc_model::Rgb(0x918485)), ("accent6", ibis_doc_model::Rgb(0x855d5d)),
            ("hlink", ibis_doc_model::Rgb(0xb83810)), ("folHlink", ibis_doc_model::Rgb(0x7a2518)),
        ],
    },
    DocxThemeColorsPreset {
        id: "violet",
        action_id: "docx.design.theme-colors.violet",
        label_key: "docx.design.theme-colors.violet",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x3b2847)), ("lt2", ibis_doc_model::Rgb(0xf0eaf5)),
            ("accent1", ibis_doc_model::Rgb(0x8064a2)), ("accent2", ibis_doc_model::Rgb(0xa088b7)),
            ("accent3", ibis_doc_model::Rgb(0xb2a1c7)), ("accent4", ibis_doc_model::Rgb(0x5c4776)),
            ("accent5", ibis_doc_model::Rgb(0x9c85c0)), ("accent6", ibis_doc_model::Rgb(0x705697)),
            ("hlink", ibis_doc_model::Rgb(0x6b4c9a)), ("folHlink", ibis_doc_model::Rgb(0x4a356b)),
        ],
    },
    DocxThemeColorsPreset {
        id: "aspect",
        action_id: "docx.design.theme-colors.aspect",
        label_key: "docx.design.theme-colors.aspect",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x242852)), ("lt2", ibis_doc_model::Rgb(0xe3e3e3)),
            ("accent1", ibis_doc_model::Rgb(0xf0a22e)), ("accent2", ibis_doc_model::Rgb(0xa56110)),
            ("accent3", ibis_doc_model::Rgb(0x90a437)), ("accent4", ibis_doc_model::Rgb(0x43896a)),
            ("accent5", ibis_doc_model::Rgb(0x427891)), ("accent6", ibis_doc_model::Rgb(0x4e4273)),
            ("hlink", ibis_doc_model::Rgb(0xd86a14)), ("folHlink", ibis_doc_model::Rgb(0x745a45)),
        ],
    },
    DocxThemeColorsPreset {
        id: "median",
        action_id: "docx.design.theme-colors.median",
        label_key: "docx.design.theme-colors.median",
        colors: [
            ("dk1", ibis_doc_model::Rgb(0x000000)), ("lt1", ibis_doc_model::Rgb(0xffffff)),
            ("dk2", ibis_doc_model::Rgb(0x444444)), ("lt2", ibis_doc_model::Rgb(0xefefef)),
            ("accent1", ibis_doc_model::Rgb(0x775f55)), ("accent2", ibis_doc_model::Rgb(0x8c786e)),
            ("accent3", ibis_doc_model::Rgb(0xa19187)), ("accent4", ibis_doc_model::Rgb(0xb6aaa1)),
            ("accent5", ibis_doc_model::Rgb(0xcbc3bb)), ("accent6", ibis_doc_model::Rgb(0xe0ddd5)),
            ("hlink", ibis_doc_model::Rgb(0x93381e)), ("folHlink", ibis_doc_model::Rgb(0x5f2e23)),
        ],
    },
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct DocxStyleSetPreset {
    pub(crate) preset: ibis_doc_model::StyleSetPreset,
    pub(crate) action_id: &'static str,
    pub(crate) label_key: &'static str,
}

pub(crate) const DOCX_STYLE_SET_PRESETS: [DocxStyleSetPreset; 8] = [
    DocxStyleSetPreset { preset: ibis_doc_model::StyleSetPreset::Default, action_id: "docx.design.style-set.default", label_key: "docx.design.style-set.default" },
    DocxStyleSetPreset { preset: ibis_doc_model::StyleSetPreset::Casual, action_id: "docx.design.style-set.casual", label_key: "docx.design.style-set.casual" },
    DocxStyleSetPreset { preset: ibis_doc_model::StyleSetPreset::Centered, action_id: "docx.design.style-set.centered", label_key: "docx.design.style-set.centered" },
    DocxStyleSetPreset { preset: ibis_doc_model::StyleSetPreset::Formal, action_id: "docx.design.style-set.formal", label_key: "docx.design.style-set.formal" },
    DocxStyleSetPreset { preset: ibis_doc_model::StyleSetPreset::Elegant, action_id: "docx.design.style-set.elegant", label_key: "docx.design.style-set.elegant" },
    DocxStyleSetPreset { preset: ibis_doc_model::StyleSetPreset::Modern, action_id: "docx.design.style-set.modern", label_key: "docx.design.style-set.modern" },
    DocxStyleSetPreset { preset: ibis_doc_model::StyleSetPreset::Simple, action_id: "docx.design.style-set.simple", label_key: "docx.design.style-set.simple" },
    DocxStyleSetPreset { preset: ibis_doc_model::StyleSetPreset::Minimalist, action_id: "docx.design.style-set.minimalist", label_key: "docx.design.style-set.minimalist" },
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct DocxThemeFontsPreset {
    pub(crate) id: &'static str,
    pub(crate) action_id: &'static str,
    pub(crate) label_key: &'static str,
    pub(crate) major_latin: &'static str,
    pub(crate) major_ea: &'static str,
    pub(crate) minor_latin: &'static str,
    pub(crate) minor_ea: &'static str,
}

pub(crate) const DOCX_THEME_FONTS_PRESETS: [DocxThemeFontsPreset; 6] = [
    DocxThemeFontsPreset {
        id: "office",
        action_id: "docx.design.theme-fonts.office",
        label_key: "docx.design.theme-fonts.office",
        major_latin: "Aptos Display",
        major_ea: "Yu Gothic",
        minor_latin: "Aptos",
        minor_ea: "Yu Gothic",
    },
    DocxThemeFontsPreset {
        id: "calibri",
        action_id: "docx.design.theme-fonts.calibri",
        label_key: "docx.design.theme-fonts.calibri",
        major_latin: "Calibri Light",
        major_ea: "Yu Gothic Light",
        minor_latin: "Calibri",
        minor_ea: "Yu Gothic",
    },
    DocxThemeFontsPreset {
        id: "arial",
        action_id: "docx.design.theme-fonts.arial",
        label_key: "docx.design.theme-fonts.arial",
        major_latin: "Arial",
        major_ea: "MS Gothic",
        minor_latin: "Arial",
        minor_ea: "MS PGothic",
    },
    DocxThemeFontsPreset {
        id: "times",
        action_id: "docx.design.theme-fonts.times",
        label_key: "docx.design.theme-fonts.times",
        major_latin: "Times New Roman",
        major_ea: "MS Mincho",
        minor_latin: "Times New Roman",
        minor_ea: "MS PMincho",
    },
    DocxThemeFontsPreset {
        id: "century-gothic",
        action_id: "docx.design.theme-fonts.century-gothic",
        label_key: "docx.design.theme-fonts.century-gothic",
        major_latin: "Century Gothic",
        major_ea: "Yu Gothic",
        minor_latin: "Century Gothic",
        minor_ea: "Yu Gothic",
    },
    DocxThemeFontsPreset {
        id: "meiryo",
        action_id: "docx.design.theme-fonts.meiryo",
        label_key: "docx.design.theme-fonts.meiryo",
        major_latin: "Meiryo UI",
        major_ea: "Meiryo UI",
        minor_latin: "Meiryo",
        minor_ea: "Meiryo",
    },
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct DocxThemeEffectPreset {
    pub(crate) id: &'static str,
    pub(crate) action_id: &'static str,
    pub(crate) label_key: &'static str,
}

pub(crate) const DOCX_THEME_EFFECTS_PRESETS: [DocxThemeEffectPreset; 10] = [
    DocxThemeEffectPreset { id: "office", action_id: "docx.design.effects.office", label_key: "docx.design.effects.office" },
    DocxThemeEffectPreset { id: "office_2007_2010", action_id: "docx.design.effects.office_2007_2010", label_key: "docx.design.effects.office_2007_2010" },
    DocxThemeEffectPreset { id: "metro", action_id: "docx.design.effects.metro", label_key: "docx.design.effects.metro" },
    DocxThemeEffectPreset { id: "aspect", action_id: "docx.design.effects.aspect", label_key: "docx.design.effects.aspect" },
    DocxThemeEffectPreset { id: "apex", action_id: "docx.design.effects.apex", label_key: "docx.design.effects.apex" },
    DocxThemeEffectPreset { id: "slate", action_id: "docx.design.effects.slate", label_key: "docx.design.effects.slate" },
    DocxThemeEffectPreset { id: "median", action_id: "docx.design.effects.median", label_key: "docx.design.effects.median" },
    DocxThemeEffectPreset { id: "glossy", action_id: "docx.design.effects.glossy", label_key: "docx.design.effects.glossy" },
    DocxThemeEffectPreset { id: "bubbles", action_id: "docx.design.effects.bubbles", label_key: "docx.design.effects.bubbles" },
    DocxThemeEffectPreset { id: "shadow", action_id: "docx.design.effects.shadow", label_key: "docx.design.effects.shadow" },
];

/// Section breaks.
impl SlideEditor {
    /// 挿入 > セクション区切り: the paragraph is split at the caret, and a
    /// section ends with its first half.
    pub(crate) fn can_insert_section_break(&self) -> bool {
        let Some((start,end))=self.ordered_selection() else{return false};
        let Some(doc)=self.flow() else{return false};
        [start,end].into_iter().all(|pos|flow_command::locate(doc,pos.para).is_some_and(|(container,_)|container==Container::Body))
    }
    pub(crate) fn insert_section_break(&mut self, kind: ibis_doc_model::SectionBreak, cx: &mut Context<Self>) {
        if !self.can_insert_section_break() {return;}
        let Some((start, end)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let next = doc.new_id();
        let mut commands = Vec::new();
        if start != end {
            commands.push(FlowCommand::Delete { from: start, to: end });
        }
        commands.push(FlowCommand::InsertText { at: start, text: "\n".into(), format: None });
        commands.push(FlowCommand::SetParagraphFormat { first: start.para, last: start.para, change: ParaChange::SectionBreak(kind) });
        if self.run_flow(commands, cx) {
            self.set_caret(FlowPos::new(next, 0), false, cx);
            self.scroll_to_caret(cx);
        }
    }
}

/// Headers, footers and page numbers.
impl SlideEditor {
    /// 挿入 > ヘッダーとフッター: the caret into the header (made if the
    /// document has none).
    /// The caret into the header or footer of `kind` the caret's section
    /// shows (its own or an earlier section's), made when there is none.
    pub(crate) fn edit_part(&mut self, kind: ibis_doc_model::PartKind, cx: &mut Context<Self>) {
        use ibis_doc_model::PartKind;
        let section = self.caret_section();
        if section == 0 && matches!(kind, PartKind::Header | PartKind::Footer) && self.flow().is_some_and(|d| !d.linked[kind.index()] || flow_command::sections(d).len() == 1) {
            return self.edit_header(kind == PartKind::Footer, cx);
        }
        let Some(doc) = self.flow().cloned() else { return };
        let order = flow_command::sections(&doc);
        let at = order.iter().position(|s| *s == section).unwrap_or(order.len() - 1);
        let owner = order[..=at].iter().rev().copied().find(|s| flow_command::section_setup(&doc, *s).is_some_and(|x| x.parts[kind.index()].as_ref().is_some_and(|b| !b.is_empty())));
        let owner = match owner {
            Some(o) => o,
            None => {
                let Some(command) = flow_command::link_part(&doc, section, kind, true) else { return };
                if !self.run_flow(vec![command], cx) {
                    return;
                }
                section
            }
        };
        let Some(doc) = self.flow() else { return };
        let container = match (owner, kind) {
            (0, PartKind::Header) => Container::Header,
            (0, PartKind::Footer) => Container::Footer,
            (s, k) => Container::Part { section: s, kind: k },
        };
        if let Some(Block::Paragraph(p)) = flow_command::blocks(doc, container).and_then(|b| b.first()) {
            let pos = FlowPos::new(p.id, p.len());
            self.set_caret(pos, false, cx);
            self.scroll_to_caret(cx);
        }
    }

    /// 前と同じ: the caret's section's header or footer of `kind` made the
    /// previous section's again, or its own (a copy).
    pub(crate) fn toggle_linked(&mut self, kind: ibis_doc_model::PartKind, cx: &mut Context<Self>) {
        let section = self.caret_section();
        let Some(doc) = self.flow() else { return };
        let own = flow_command::section_setup(doc, section).is_some_and(|s| s.parts[kind.index()].is_some());
        let Some(command) = flow_command::link_part(doc, section, kind, !own) else { return };
        // The caret leaves a header that goes.
        let caret_in = self.text_selection().and_then(|(_, c)| flow_command::locate(doc, c.para)).map(|l| l.0);
        if self.run_flow(vec![command], cx)
            && caret_in.is_some_and(|c| matches!(c, Container::Part { .. } | Container::Header | Container::Footer))
            && self.flow().is_some_and(|d| self.text_selection().is_some_and(|(_, c)| flow_command::locate(d, c.para).is_none()))
            && let Some(start) = self.document_edge(false)
        {
            self.set_caret(start, false, cx);
        }
    }

    /// Whether the caret's section's header or footer of `kind` is the
    /// previous section's (前と同じ), and whether there is a previous one.
    pub(crate) fn part_linked(&self, kind: ibis_doc_model::PartKind) -> (bool, bool) {
        let section = self.caret_section();
        let Some(doc) = self.flow() else { return (false, false) };
        let first = flow_command::sections(doc).first() == Some(&section);
        (flow_command::section_setup(doc, section).is_some_and(|s| s.parts[kind.index()].is_none()), !first)
    }

    /// 奇数／偶数ページ別指定.
    pub(crate) fn toggle_even_and_odd(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let command = flow_command::set_even_and_odd(doc, !doc.even_and_odd);
        self.run_flow(vec![command], cx);
    }

    /// Adding a header (footer): its style (ヘッダー, フッター) first when the
    /// document lacks it (DOCX-HEADER-014).
    pub(crate) fn add_header_commands(&self, footer: bool) -> Vec<FlowCommand> {
        let mut commands = Vec::new();
        if let Some((id, style)) = self.flow().and_then(|d| flow_command::missing_header_footer_style(&d.styles, footer)) {
            commands.push(FlowCommand::SetStyle { id, style: Some(style) });
        }
        commands.push(FlowCommand::AddHeader { footer });
        commands
    }

    pub(crate) fn edit_header(&mut self, footer: bool, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let list = if footer { &doc.footer } else { &doc.header };
        if list.is_empty() {
            let id = doc.new_id();
            if self.run_flow(self.add_header_commands(footer), cx) {
                self.set_caret(FlowPos::new(id, 0), false, cx);
            }
            return;
        }
        if let Some(Block::Paragraph(p)) = list.first() {
            let pos = FlowPos::new(p.id, p.len());
            self.set_caret(pos, false, cx);
        }
    }

    /// 挿入 > ページ番号: insert page number at specified position.
    pub(crate) fn insert_page_number_at(&mut self, pos: PageNumberPosition, cx: &mut Context<Self>) {
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(doc) = self.flow().cloned() else { return };
        if pos == PageNumberPosition::Current {
            if self.run_flow(vec![FlowCommand::InsertPageNumber { at: caret }], cx) {
                self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            }
            return;
        }
        let in_header = matches!(flow_command::locate(&doc, caret.para), Some((Container::Header | Container::Footer | Container::Part { .. }, _)));
        if in_header {
            if self.run_flow(vec![FlowCommand::InsertPageNumber { at: caret }], cx) {
                self.set_caret(FlowPos::new(caret.para, caret.offset + 1), false, cx);
            }
            return;
        }
        let (footer, align) = match pos {
            PageNumberPosition::HeaderLeft => (false, FlowAlign::Left),
            PageNumberPosition::HeaderCenter => (false, FlowAlign::Center),
            PageNumberPosition::HeaderRight => (false, FlowAlign::Right),
            PageNumberPosition::FooterLeft => (true, FlowAlign::Left),
            PageNumberPosition::FooterCenter => (true, FlowAlign::Center),
            PageNumberPosition::FooterRight => (true, FlowAlign::Right),
            PageNumberPosition::Current => unreachable!(),
        };
        let mut commands = Vec::new();
        let target_list = if footer { &doc.footer } else { &doc.header };
        let para = match target_list.first() {
            Some(Block::Paragraph(p)) => {
                commands.push(FlowCommand::InsertPageNumber { at: FlowPos::new(p.id, p.len()) });
                p.id
            }
            Some(_) => return,
            None => {
                let id = doc.new_id();
                commands.extend(self.add_header_commands(footer));
                commands.push(FlowCommand::InsertPageNumber { at: FlowPos::new(id, 0) });
                id
            }
        };
        commands.push(FlowCommand::SetParagraphFormat { first: para, last: para, change: ParaChange::Align(align) });
        self.run_flow(commands, cx);
    }

    /// 挿入 > ページ番号: at the caret in a header or footer; else at the
    /// bottom of the page, centred (a footer made if there is none).
    pub(crate) fn insert_page_number(&mut self, cx: &mut Context<Self>) {
        self.insert_page_number_at(PageNumberPosition::FooterCenter, cx);
    }
}

/// The position where a page number is placed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageNumberPosition {
    Current,
    HeaderLeft,
    HeaderCenter,
    HeaderRight,
    FooterLeft,
    FooterCenter,
    FooterRight,
}

/// The notes and comments marked in the body (a comment: its start, end
/// and mark all there).
pub(crate) struct Marked {
    notes: std::collections::HashSet<(bool, u32)>,
    comments: std::collections::HashSet<u32>,
}

fn marked(doc: &FlowDocument) -> Marked {
    let mut out = Marked { notes: Default::default(), comments: Default::default() };
    if doc.footnotes.is_empty() && doc.endnotes.is_empty() && doc.comments.is_empty() {
        return out;
    }
    fn walk(blocks: &[Block], notes: &mut std::collections::HashSet<(bool, u32)>, marks: &mut std::collections::HashMap<u32, [bool; 3]>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        match r.kind {
                            ibis_doc_model::FlowRunKind::NoteRef { endnote, id } => {
                                notes.insert((endnote, id));
                            }
                            ibis_doc_model::FlowRunKind::CommentStart(i) => marks.entry(i).or_default()[0] = true,
                            ibis_doc_model::FlowRunKind::CommentEnd(i) => marks.entry(i).or_default()[1] = true,
                            ibis_doc_model::FlowRunKind::CommentRef(i) => marks.entry(i).or_default()[2] = true,
                            _ => {}
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, notes, marks)),
            }
        }
    }
    let mut marks = std::collections::HashMap::new();
    walk(&doc.body, &mut out.notes, &mut marks);
    out.comments = marks.into_iter().filter(|(_, m)| *m == [true; 3]).map(|(i, _)| i).collect();
    out
}

/// Tables of contents, links to bookmarks, and the outline.
impl SlideEditor {
    /// 見出しに番号を付ける: the heading styles numbered 1 / 1.1 / 1.1.1 (one
    /// step of undo).
    pub(crate) fn number_headings(&mut self, cx: &mut Context<Self>) {
        let Some(commands) = self.flow().and_then(|d| flow_command::heading_numbering(d)) else { return };
        self.run_flow(commands, cx);
    }

    /// フィールドの更新: the captions' numbers and the cross-references
    /// given their values now (one step of undo, or added to the last).
    /// If text is selected, only fields in the selection are updated.
    pub(crate) fn update_fields(&mut self, amend: bool, cx: &mut Context<Self>) {
        let range = self.ordered_selection().filter(|(a, b)| a != b);
        if range.is_none() {
            let caret_para = self.text_selection().map(|(_, c)| c.para);
            if caret_para.and_then(|p| self.toc_start_kind(Some(p))).is_some() {
                self.update_toc(None, cx);
                return;
            }
            // The index is made again with the fields.
            if self.flow().is_some_and(|d| super::index_fields::index_field(d).is_some()) {
                self.update_index(cx);
            }
        }
        // The table of authorities is made again with the fields.
        if range.is_none() && self.flow().is_some_and(|d| super::toa_fields::toa_field(d).is_some()) {
            self.update_toa(cx);
        }
        // The bibliography is made again with the fields.
        if range.is_none() && self.flow().is_some_and(|d| super::bibliography::bibliography_field(d).is_some()) {
            self.update_bibliography(cx);
        }
        let now = crate::clock::local_now();
        let include = self.include_text_resolver();
        let Some(mut commands) = self.flow().map(|d| flow_command::updated_fields_in_range_with(d, range, Some(&now), Some(&include))) else { return };
        commands.extend(self.linked_picture_commands(range));
        if commands.is_empty() {
            return;
        }
        if amend {
            let _ = self.document.amend_last(commands.into_iter().map(Command::Flow).collect());
            cx.notify();
        } else {
            self.run_flow(commands, cx);
        }
    }

    /// 文書内のすべてのフィールド（本文・ヘッダー・フッター・目次を含む）を更新する。
    pub(crate) fn update_all_document_fields(&mut self, cx: &mut Context<Self>) {
        if self.toc_start_kind(None).is_some() {
            self.update_toc(None, cx);
        }
        if self.flow().is_some_and(|d| super::index_fields::index_field(d).is_some()) {
            self.update_index(cx);
        }
        let now = crate::clock::local_now();
        let include = self.include_text_resolver();
        let Some(mut commands) = self.flow().map(|d| flow_command::updated_fields_in_range_with(d, None, Some(&now), Some(&include))) else { return };
        commands.extend(self.linked_picture_commands(None));
        if !commands.is_empty() {
            self.run_flow(commands, cx);
        }
    }

    /// The commands that read again the pictures that are the results of `INCLUDEPICTURE "file"` fields (all of them, or those in
    /// `range`) from their files (next to the document when the path is relative), the new pictures kept the size of the old. A file
    /// that cannot be read, or that is the picture as it is, gives none.
    fn linked_picture_commands(&mut self, range: Option<(FlowPos, FlowPos)>) -> Vec<FlowCommand> {
        #[cfg(not(target_family = "wasm"))]
        let base = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf);
        #[cfg(target_family = "wasm")]
        let base: Option<std::path::PathBuf> = None;
        let Some(doc) = self.flow().cloned() else { return Vec::new() };
        let order: Vec<u32> = doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).collect();
        let index = |id: u32| order.iter().position(|o| *o == id).unwrap_or(0);
        let mut found: Vec<(FlowPos, String, ibis_doc_model::MediaId)> = Vec::new();
        for block in &doc.body {
            let Block::Paragraph(p) = block else { continue };
            let mut offset = 0;
            for run in &p.runs {
                if let FlowRunKind::Picture { media, look, .. } = &run.kind
                    && let Some(file) = &look.linked_file
                {
                    let pos = FlowPos::new(p.id, offset);
                    let inside = range.is_none_or(|(a, b)| {
                        let at = (index(pos.para), pos.offset);
                        (index(a.para), a.offset) <= at && at < (index(b.para), b.offset)
                    });
                    if inside {
                        found.push((pos, file.clone(), *media));
                    }
                }
                offset += run.len();
            }
        }
        let mut commands = Vec::new();
        for (at, file, old_media) in found {
            let path = std::path::Path::new(&file);
            let full = if path.is_absolute() { path.to_path_buf() } else if let Some(b) = &base { b.join(path) } else { continue };
            let Ok(data) = std::fs::read(&full) else { continue };
            if self.presentation().media(old_media).is_some_and(|m| *m.data == *data) {
                continue;
            }
            let format = ibis_doc_model::MediaFormat::from_path(&full.to_string_lossy());
            let image_format = match format {
                ibis_doc_model::MediaFormat::Png => Some(ImageFormat::Png),
                ibis_doc_model::MediaFormat::Jpeg => Some(ImageFormat::Jpeg),
                ibis_doc_model::MediaFormat::Gif => Some(ImageFormat::Gif),
                ibis_doc_model::MediaFormat::Bmp => Some(ImageFormat::Bmp),
                ibis_doc_model::MediaFormat::Tiff => Some(ImageFormat::Tiff),
                ibis_doc_model::MediaFormat::Svg => Some(ImageFormat::Svg),
                _ => None,
            };
            let media = self.document.ids().add_media(format, data.clone());
            if let Some(f) = image_format {
                self.resources.images.insert(media, Arc::new(Image::from_bytes(f, data)));
            }
            commands.push(FlowCommand::ReplacePicture { at, media, size: None });
        }
        commands
    }

    /// The text an `INCLUDETEXT "file" [bookmark]` field's instruction names, read from the file (next to the document when the
    /// path is relative): the text of a .docx (its paragraphs, or those of the bookmark, one after another) or of a text file.
    /// `None` when the file cannot be read (the result stays as it was).
    fn include_text_resolver(&self) -> impl Fn(&str) -> Option<String> + use<> {
        #[cfg(not(target_family = "wasm"))]
        let base = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf);
        #[cfg(target_family = "wasm")]
        let base: Option<std::path::PathBuf> = None;
        move |instruction: &str| {
            let words = super::index_fields::words(instruction);
            let named = words.get(1).filter(|w| !w.starts_with('\\') || w.len() > 2)?;
            // A Windows path as Word writes it in a field (`C:\\dir\\file.docx`).
            let path = named.replace("\\\\", "/");
            let path = std::path::Path::new(&path);
            let full = if path.is_absolute() { path.to_path_buf() } else { base.as_ref()?.join(path) };
            let data = std::fs::read(&full).ok()?;
            let is_docx = full.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("docx"));
            if !is_docx {
                let text = ibis_pptx::csv::decode(&data).unwrap_or_else(|_| String::from_utf8_lossy(&data).into_owned());
                return Some(text.lines().map(str::trim_end).collect::<Vec<_>>().join(" ").trim().to_string());
            }
            let presentation = ibis_pptx::read(&data).ok()?;
            let doc = presentation.flow.as_ref()?;
            let bookmark = words.get(2).filter(|w| !w.starts_with('\\'));
            let paragraphs: Vec<&ibis_doc_model::FlowParagraph> = doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect();
            let range = match bookmark {
                Some(name) => {
                    let start = paragraphs.iter().position(|p| p.bookmarks.iter().any(|b| b == name))?;
                    let end = paragraphs[start..].iter().position(|p| p.bookmark_ends.iter().any(|b| b == name)).map_or(start, |o| start + o);
                    start..=end
                }
                None => 0..=paragraphs.len().saturating_sub(1),
            };
            let text: Vec<String> = paragraphs.get(range)?.iter().map(|p| p.text().replace('\u{fffc}', "")).filter(|t| !t.is_empty()).collect();
            Some(text.join(" "))
        }
    }

    /// 印刷前にフィールドを更新するかどうか (Word Options > 表示 > 印刷オプション: <w:updateFields/>).
    pub(crate) fn is_update_fields_before_printing(&self) -> bool {
        self.flow().map_or(false, |d| d.update_fields)
    }

    /// 印刷前のフィールド更新設定を変更する（1ステップの Undo/Redo）。
    pub(crate) fn set_update_fields_before_printing(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.is_update_fields_before_printing() == on {
            return;
        }
        self.run_flow(vec![FlowCommand::SetUpdateFields(on)], cx);
    }

    /// 印刷前のフィールド更新設定を切り替える。
    pub(crate) fn toggle_update_fields_before_printing(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_update_fields_before_printing();
        self.set_update_fields_before_printing(next, cx);
    }

    /// 印刷前にリンクされたデータを更新するかどうか (Word Options > 表示 > 印刷オプション).
    pub(crate) fn is_update_links_before_printing(&self) -> bool {
        self.update_links_before_printing
    }

    /// 印刷前のリンクデータ更新設定を変更する。
    #[allow(dead_code)]
    pub(crate) fn set_update_links_before_printing(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.update_links_before_printing != on {
            self.update_links_before_printing = on;
            cx.notify();
        }
    }

    /// 印刷前のリンクデータ更新設定を切り替える。
    pub(crate) fn toggle_update_links_before_printing(&mut self, cx: &mut Context<Self>) {
        self.update_links_before_printing = !self.update_links_before_printing;
        cx.notify();
    }

    /// 表示 > ダークモード（ページ）: the page shown dark (the screen only; printing and exporting are as always).
    pub(crate) fn is_dark_page(&self) -> bool {
        self.resources.dark_page
    }

    pub(crate) fn toggle_dark_page(&mut self, cx: &mut Context<Self>) {
        self.resources.dark_page = !self.resources.dark_page;
        crate::docx::flow_layout::clear_layout_caches();
        cx.notify();
    }

    /// 背景の色とイメージを印刷するかどうか (Word Options > 表示 > 印刷オプション DOCX-OPTIONS-010).
    pub(crate) fn is_print_backgrounds(&self) -> bool {
        self.print_backgrounds
    }

    /// 背景の色とイメージの印刷設定を変更する。
    pub(crate) fn set_print_backgrounds(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.print_backgrounds != on {
            self.print_backgrounds = on;
            self.resources.print_backgrounds = on;
            crate::docx::flow_layout::clear_layout_caches();
            cx.notify();
        }
    }

    /// 背景の色とイメージの印刷設定を切り替える。
    pub(crate) fn toggle_print_backgrounds(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_print_backgrounds();
        self.set_print_backgrounds(next, cx);
    }

    /// 隠し文字を印刷するかどうか (Word Options > 表示 > 印刷オプション DOCX-OPTIONS-010).
    pub(crate) fn is_print_hidden_text(&self) -> bool {
        self.print_hidden_text
    }

    /// 隠し文字の印刷設定を変更する。
    pub(crate) fn set_print_hidden_text(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.print_hidden_text != on {
            self.print_hidden_text = on;
            self.resources.print_hidden_text = on;
            crate::docx::flow_layout::clear_layout_caches();
            cx.notify();
        }
    }

    /// 隠し文字の印刷設定を切り替える。
    pub(crate) fn toggle_print_hidden_text(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_print_hidden_text();
        self.set_print_hidden_text(next, cx);
    }

    /// 変更履歴とコメントを印刷するかどうか (Word Print > 設定 > 変更履歴の印刷 DOCX-SEC-014).
    pub(crate) fn is_print_markup(&self) -> bool {
        self.print_markup
    }

    /// 変更履歴とコメントの印刷設定を変更する。
    pub(crate) fn set_print_markup(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.print_markup != on {
            self.print_markup = on;
            self.resources.print_markup = on;
            crate::docx::flow_layout::clear_layout_caches();
            cx.notify();
        }
    }

    /// 変更履歴とコメントの印刷設定を切り替える。
    pub(crate) fn toggle_print_markup(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_print_markup();
        self.set_print_markup(next, cx);
    }

    /// 現在キャレットがヘッダーまたはフッター内にあるかどうか。
    pub(crate) fn is_header_footer_editing(&self) -> bool {
        let Some((_, caret)) = self.text_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        matches!(
            ibis_doc_model::flow_command::locate(doc, caret.para),
            Some((ibis_doc_model::flow_command::Container::Header | ibis_doc_model::flow_command::Container::Footer | ibis_doc_model::flow_command::Container::Part { .. }, _))
        )
    }

    /// ヘッダー/フッター編集時に文書内のテキストを表示するかどうか (Word Header & Footer Tools > オプション > 文書内のテキストを表示).
    pub(crate) fn is_show_document_text(&self) -> bool {
        self.show_document_text
    }

    /// ヘッダー/フッター編集時の文書内テキスト表示設定を変更する。
    pub(crate) fn set_show_document_text(&mut self, on: bool, cx: &mut Context<Self>) {
        if self.show_document_text != on {
            self.show_document_text = on;
            self.resources.hide_document_text = !on && self.is_header_footer_editing();
            crate::docx::flow_layout::clear_layout_caches();
            cx.notify();
        }
    }

    /// ヘッダー/フッター編集時の文書内テキスト表示設定を切り替える。
    pub(crate) fn toggle_show_document_text(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_show_document_text();
        self.set_show_document_text(next, cx);
    }

    /// 印刷またはPDF書き出し前の更新準備処理。
    /// 「印刷前にフィールドを更新する」が有効な場合、文書内のフィールドと目次を更新する。
    /// 「印刷前にリンクされたデータを更新する」が有効な場合、リンクされたデータを更新する。
    pub(crate) fn prepare_print_or_export(&mut self, cx: &mut Context<Self>) {
        if self.flow().is_none() {
            return;
        }
        if self.is_update_fields_before_printing() {
            self.update_all_document_fields(cx);
        }
        if self.is_update_links_before_printing() {
            // リンクデータ更新の必要処理
        }
    }

    /// フィールドのリンク解除 (Ctrl+Shift+F9 / ⇧⌘F9): 選択範囲内（またはキャレット位置、選択がなければ文書全体）の
    /// フィールドを静的なプレーンテキストに変換（1ステップの Undo/Redo）。
    pub(crate) fn unlink_fields(&mut self, cx: &mut Context<Self>) {
        let now = crate::clock::local_now();
        let range = self.ordered_selection();
        let Some(commands) = self.flow().map(|d| flow_command::unlink_fields_at(d, range, Some(&now))) else { return };
        if commands.is_empty() {
            return;
        }
        self.run_flow(commands, cx);
    }

    /// フィールドコードの表示/非表示 (Shift+F9 / ⇧F9): 選択範囲内（またはキャレット位置）の
    /// フィールドのコード表示/結果表示をトグル（1ステップの Undo/Redo）。
    pub(crate) fn toggle_field_codes(&mut self, cx: &mut Context<Self>) {
        let range = self.ordered_selection();
        let Some(commands) = self.flow().map(|d| flow_command::toggle_field_codes_at(d, range)) else { return };
        if commands.is_empty() {
            return;
        }
        self.run_flow(commands, cx);
    }

    /// すべてのフィールドコードの表示/非表示 (Alt+F9 / ⌥F9): 文書全体のすべての
    /// フィールドのコード表示/結果表示をトグル（1ステップの Undo/Redo）。
    pub(crate) fn toggle_all_field_codes(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let cmd = flow_command::toggle_all_field_codes(doc);
        self.run_flow(vec![cmd], cx);
    }

    /// フィールドのロック / ロック解除 (Ctrl+F11 / ⌘F11 / Ctrl+Shift+F11 / ⇧⌘F11):
    /// 選択範囲内（またはキャレット位置、選択がなければ文書全体）のフィールドをロック/解除（1ステップの Undo/Redo）。
    pub(crate) fn lock_fields(&mut self, locked: bool, cx: &mut Context<Self>) {
        let range = self.ordered_selection();
        let Some(commands) = self.flow().map(|d| flow_command::lock_fields_at(d, range, locked)) else { return };
        if commands.is_empty() {
            return;
        }
        self.run_flow(commands, cx);
    }

    /// フィールドの挿入 (Ctrl+F9 / ⌘F9):
    /// 選択範囲がある場合は選択テキストをコードとするフィールドに即座に変換し、
    /// 選択がない場合は空のフィールドブラケット `{ }` を挿入してコード編集状態にする。
    pub(crate) fn insert_field(&mut self, cx: &mut Context<Self>) {
        let sel_text = self.selected_text();
        let range = self.ordered_selection();
        let Some(doc) = self.flow() else { return };
        if let Some((cmds, new_pos)) = flow_command::insert_field_at_range(doc, range, &sel_text, true) {
            if self.run_flow(cmds, cx) {
                self.set_caret(new_pos, false, cx);
            }
        }
    }

    /// フィールドの編集: キャレット位置のフィールドのコードを編集する。
    #[allow(dead_code)]
    pub(crate) fn edit_field_with_code(&mut self, new_code: &str, cx: &mut Context<Self>) {
        let Some((caret, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        if let Some(cmds) = flow_command::edit_field_at(doc, caret, new_code) {
            self.run_flow(cmds, cx);
        }
    }

    /// 表の計算式フィールド（= Formula）を挿入する。
    pub(crate) fn insert_table_formula(&mut self, formula: &str, format: Option<&str>, cx: &mut Context<Self>) {
        let mut instr = formula.trim().to_string();
        if !instr.starts_with('=') {
            instr = format!("= {instr}");
        }
        if let Some(fmt) = format {
            let fmt = fmt.trim();
            if !fmt.is_empty() {
                instr = format!("{instr} \\# \"{fmt}\"");
            }
        }
        let range = self.ordered_selection();
        let Some(doc) = self.flow() else { return };
        if let Some((cmds, new_pos)) = flow_command::insert_field_at_range(doc, range, &instr, false) {
            if self.run_flow(cmds, cx) {
                self.set_caret(new_pos, false, cx);
            }
        }
    }

    /// 現在の表セル位置から Word 標準のデフォルト計算式（=SUM(ABOVE) または =SUM(LEFT) 等）を提案する。
    pub(crate) fn suggest_table_formula(&self) -> String {
        let Some((_, row, col)) = self.table_at_caret() else {
            return "=SUM(ABOVE)".to_string();
        };
        let Some(table) = self.caret_table() else {
            return "=SUM(ABOVE)".to_string();
        };
        let resolver = ibis_doc_model::FlowTableCellResolver { table, row, col };
        use ibis_doc_model::TableCellResolver;
        if !resolver.resolve_direction("ABOVE").is_empty() {
            "=SUM(ABOVE)".to_string()
        } else if !resolver.resolve_direction("LEFT").is_empty() {
            "=SUM(LEFT)".to_string()
        } else {
            "=SUM()".to_string()
        }
    }

    /// 図表番号の詳細挿入: ラベル・説明文・書式・ラベル除外オプションを指定して挿入
    pub(crate) fn insert_caption_detailed(
        &mut self,
        label: &str,
        text: &str,
        format: ibis_doc_model::CaptionNumberFormat,
        exclude_label: bool,
        chapter: Option<(u32, &str)>,
        cx: &mut Context<Self>,
    ) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some((commands, _)) = self.flow().and_then(|d| flow_command::insert_caption_in_chapter(d, start.para, label, text, format, exclude_label, chapter)) else { return };
        let new_id = self.flow().map(|d| d.new_id());
        if self.run_flow(commands, cx) {
            self.update_fields(true, cx);
            if let Some(id) = new_id
                && let Some(len) = self.flow().and_then(|d| flow_command::paragraph(d, id)).map(|p| p.len())
            {
                self.set_caret(FlowPos::new(id, len), false, cx);
            }
        }
    }

    /// 図表番号の挿入: a caption (`label` and its number) after the caret's
    /// paragraph, the caret at its end; the numbers after it go up.
    pub(crate) fn insert_caption(&mut self, label: &str, cx: &mut Context<Self>) {
        self.insert_caption_detailed(label, "", ibis_doc_model::CaptionNumberFormat::Arabic, false, None, cx);
    }

    /// 相互参照: a reference to caption paragraph `caption` at the caret.
    pub(crate) fn insert_cross_reference(&mut self, caption: u32, cx: &mut Context<Self>) {
        let Some((start, end)) = self.ordered_selection() else { return };
        if start != end {
            return;
        }
        let Some(commands) = self.flow().and_then(|d| flow_command::cross_reference(d, start, caption)) else { return };
        if self.run_flow(commands, cx) {
            self.set_caret(FlowPos::new(start.para, start.offset + 1), false, cx);
        }
    }

    /// 日付と時刻: the time now in `picture` (a field's `\@`) at the caret
    /// (over the selection): a `DATE` field (`TIME` when the picture has no
    /// date) when `update` (自動的に更新する), else its text.
    pub(crate) fn insert_date(&mut self, picture: &str, update: bool, cx: &mut Context<Self>) {
        let now = crate::clock::local_now();
        let text = ibis_doc_model::date_field::format_picture(&now, picture);
        if !update {
            self.type_text(&text, cx);
            return;
        }
        let Some((start, end)) = self.ordered_selection() else { return };
        if start != end {
            self.type_text("", cx);
        }
        let Some((at, _)) = self.ordered_selection() else { return };
        let kind = if picture.contains(['y', 'M', 'd', 'g', 'e']) || picture.contains("aaa") { "DATE" } else { "TIME" };
        let field = ibis_doc_model::FlowRunKind::Field { instruction: format!("{kind} \\@ \"{picture}\""), shown: text };
        if self.run_flow(vec![FlowCommand::InsertField { at, kind: field }], cx) {
            self.set_caret(FlowPos::new(at.para, at.offset + 1), false, cx);
        }
    }

    /// 変更履歴の記録: on or off.
    pub(crate) fn toggle_track_changes(&mut self, cx: &mut Context<Self>) {
        if self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("_docx.track-changes-is-locked-so-it-can-t-be-turned-of").into());
            cx.notify();
            return;
        }
        let on = self.flow().is_some_and(|d| d.track_changes);
        self.run_flow(vec![FlowCommand::SetTrackChanges(!on)], cx);
    }

    /// 変更履歴の記録がロックされているか。
    pub(crate) fn is_track_changes_locked(&self) -> bool {
        self.flow().is_some_and(|d| d.lock_tracking)
    }

    /// 変更履歴の記録のロック: on or off.
    pub(crate) fn toggle_lock_tracking(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let next_locked = !doc.lock_tracking;
        self.run_flow(
            vec![FlowCommand::SetLockTracking {
                locked: next_locked,
                track_changes: if next_locked { true } else { doc.track_changes },
            }],
            cx,
        );
    }

    // The shared revision operation walks its body. Apply it to each DOCX
    // story, then map replacements back to their real containers. Keep the
    // common model's body-only traversal unchanged for numbering/captions.
    fn story_revision_commands_filter(
        doc: &FlowDocument,
        range: Option<(FlowPos, FlowPos)>,
        accept: bool,
        filter: Option<&std::collections::HashSet<String>>,
    ) -> Option<Vec<FlowCommand>> {
        fn contains(blocks: &[Block], id: u32) -> bool {
            blocks.iter().any(|b| match b {
                Block::Paragraph(p) => p.id == id,
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).any(|c| contains(&c.blocks, id)),
            })
        }
        let mut stories: Vec<&[Block]> = vec![&doc.body];
        for block in &doc.body {
            if let Block::Paragraph(p) = block && let Some(section) = &p.section {
                stories.extend(section.parts.iter().flatten().map(|b| b.as_slice()));
            }
        }
        stories.extend([doc.header.as_slice(), doc.footer.as_slice(), doc.header_first.as_slice(), doc.footer_first.as_slice(), doc.header_even.as_slice(), doc.footer_even.as_slice()]);
        stories.extend(doc.footnotes.iter().chain(&doc.endnotes).map(|n| n.blocks.as_slice()));
        let mut view = doc.clone();
        let mut seen = std::collections::HashSet::new();
        let mut commands = Vec::new();
        let mut matched = range.is_none();
        for story in stories {
            if range.is_some_and(|(a, b)| !contains(story, a.para) || !contains(story, b.para)) { continue; }
            matched = true;
            view.body = story.to_vec();
            for command in flow_command::resolve_revisions_filter(&view, range, accept, filter) {
                let FlowCommand::ReplaceBlocks { remove, blocks, .. } = command else { return None };
                let Some(Block::Paragraph(p)) = blocks.first() else { return None };
                let Some((container, index)) = flow_command::locate(doc, p.id) else { return None };
                if seen.insert(p.id) { commands.push(FlowCommand::ReplaceBlocks { container, index, remove, blocks }); }
            }
        }
        matched.then_some(commands)
    }

    fn story_revision_commands(doc: &FlowDocument, range: Option<(FlowPos, FlowPos)>, accept: bool) -> Option<Vec<FlowCommand>> {
        Self::story_revision_commands_filter(doc, range, accept, None)
    }

    /// 承諾・却下: the tracked changes in the selection (or at the caret), or
    /// all of them.
    pub(crate) fn resolve_revisions(&mut self, all: bool, accept: bool, cx: &mut Context<Self>) {
        if self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("_docx.cannot-accept-or-reject-changes-because-track-ch").into());
            cx.notify();
            return;
        }
        let range = if all { None } else {
            let Some(selection) = self.ordered_selection() else { return };
            Some(selection)
        };
        let Some(doc) = self.flow() else { return };
        let Some(commands) = Self::story_revision_commands(doc, range, accept) else {
            self.status = Some(crate::shell::i18n::text("docx.editing.select-changes-accept-reject").into());
            cx.notify();
            return;
        };
        if commands.is_empty() {
            return;
        }
        let caret = self.text_selection().map(|(_, c)| c);
        if self.run_flow(commands, cx)
            && let Some(c) = caret
        {
            let len = self.paragraph_of(c.para).map_or(0, |p| p.len());
            self.set_caret(FlowPos::new(c.para, c.offset.min(len)), false, cx);
        }
    }

    /// 承諾・却下: 表示されたすべての変更履歴（校閲者フィルター適用）(DOCX-REVIEW-009)
    pub(crate) fn resolve_revisions_shown(&mut self, accept: bool, cx: &mut Context<Self>) {
        if self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("_docx.cannot-accept-or-reject-changes-because-track-ch").into());
            cx.notify();
            return;
        }
        let Some(doc) = self.flow() else { return };
        let filter = self.reviewer_filter.as_ref();
        let Some(commands) = Self::story_revision_commands_filter(doc, None, accept, filter) else {
            return;
        };
        if commands.is_empty() {
            return;
        }
        let caret = self.text_selection().map(|(_, c)| c);
        if self.run_flow(commands, cx)
            && let Some(c) = caret
        {
            let len = self.paragraph_of(c.para).map_or(0, |p| p.len());
            self.set_caret(FlowPos::new(c.para, c.offset.min(len)), false, cx);
        }
    }

    /// すべての変更を承諾・却下し、変更記録を停止 (DOCX-REVIEW-009)
    pub(crate) fn resolve_revisions_and_stop_tracking(&mut self, accept: bool, cx: &mut Context<Self>) {
        if self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("_docx.cannot-accept-or-reject-changes-because-track-ch").into());
            cx.notify();
            return;
        }
        let Some(doc) = self.flow() else { return };
        let mut commands = Self::story_revision_commands(doc, None, accept).unwrap_or_default();
        if doc.track_changes {
            commands.push(FlowCommand::SetTrackChanges(false));
        }
        if commands.is_empty() {
            return;
        }
        let caret = self.text_selection().map(|(_, c)| c);
        if self.run_flow(commands, cx)
            && let Some(c) = caret
        {
            let len = self.paragraph_of(c.para).map_or(0, |p| p.len());
            self.set_caret(FlowPos::new(c.para, c.offset.min(len)), false, cx);
        }
    }

    /// Whether the caret's paragraph is in a numbered (not bulleted) list.
    pub(crate) fn caret_list_numbered(&self) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow() else { return false };
        flow_command::paragraph(doc, start.para).and_then(|p| p.list).and_then(|l| doc.styles.numbering.level(l.num_id, l.level)).is_some_and(|l| l.format != "bullet")
    }

    /// 番号の振り直し: the caret's list numbered from its paragraph on as
    /// `how` (one step of undo).
    pub(crate) fn renumber_list(&mut self, how: flow_command::Renumber, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(commands) = self.flow().and_then(|d| flow_command::renumber(d, start.para, how)) else { return };
        let keep = self.text_selection();
        self.run_flow(commands, cx);
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
    }

    /// The table of contents' first paragraph and its kind (headings, figures, or custom styles).
    fn toc_start_kind(&self, at_para: Option<u32>) -> Option<(u32, TocType)> {
        let doc = self.flow()?;
        let parse_kind = |instr: &str| -> TocType {
            if let Some(label) = flow_command::toc_caption_label(instr) {
                TocType::Figures(label)
            } else {
                let instruction = flow_command::parse_toc_instruction(instr);
                if !instruction.custom_styles.is_empty() || instruction.tc_entries.is_some() {
                    TocType::Custom(instruction)
                } else {
                    TocType::Headings(instruction.heading_levels.unwrap_or((1, 3)))
                }
            }
        };

        if let Some(target) = at_para {
            let mut current_toc = None;
            for b in &doc.body {
                if let Block::Paragraph(p) = b {
                    if let Some(instr) = p.field_start.as_ref().filter(|i| i.trim_start().starts_with("TOC")) {
                        current_toc = Some((p.id, parse_kind(instr)));
                    }
                    if p.id == target && current_toc.is_some() {
                        return current_toc;
                    }
                    if p.field_end {
                        current_toc = None;
                    }
                }
            }
            return None;
        }

        doc.body.iter().find_map(|b| match b {
            Block::Paragraph(p) => {
                let instr = p.field_start.as_ref().filter(|i| i.trim_start().starts_with("TOC"))?;
                Some((p.id, parse_kind(instr)))
            }
            _ => None,
        })
    }

    /// The table of contents' first paragraph and its levels.
    #[allow(dead_code)]
    fn toc_start(&self) -> Option<(u32, (u32, u32))> {
        self.toc_start_kind(None).and_then(|(id, k)| match k {
            TocType::Headings(lvls) => Some((id, lvls)),
            TocType::Custom(instr) => Some((id, instr.heading_levels.unwrap_or((1, 3)))),
            TocType::Figures(_) => None,
        })
    }

    /// The commands putting a table of contents with `instruction` before `before`
    /// (or in place of the one starting there), with today's pages.
    fn toc_command(&self, before: u32, replace: bool, instruction: &flow_command::TocInstruction) -> Option<FlowCommand> {
        let doc = self.flow()?;
        let width = doc.page.size.width - doc.page.left - doc.page.right;
        let lines = self.flow_lines();
        let first = doc.page.first_page;
        let page_of = |id: u32| lines.iter().find(|l| l.para == id).map(|l| first + l.page as u32);
        let (entries, bookmarks) = flow_command::toc_entries_with_instruction(doc, instruction, &page_of, doc.new_id(), width);
        Some(FlowCommand::SetToc { before, replace, entries, bookmarks })
    }

    /// Put a table of contents with `instruction` before `before` (or in place of
    /// the one starting there) as one step, defining the styles it uses
    /// (`toc 1` …) the document lacks first. Whether it did.
    fn run_toc(&mut self, before: u32, replace: bool, instruction: &flow_command::TocInstruction, cx: &mut Context<Self>) -> bool {
        let missing = self.flow().map(|d| flow_command::missing_toc_styles_for_levels(&d.styles, instruction.all_levels())).unwrap_or_default();
        let styled = !missing.is_empty();
        if styled && !self.run_flow(missing.into_iter().map(|(id, s)| FlowCommand::SetStyle { id, style: Some(s) }).collect(), cx) {
            return false;
        }
        let Some(command) = self.toc_command(before, replace, instruction) else { return styled };
        if styled { self.document.amend_last(vec![Command::Flow(command)]).is_ok() } else { self.run_flow(vec![command], cx) }
    }

    /// 挿入 > 目次: a table of contents of headings 1 to 3 before the
    /// caret's paragraph, its page numbers those after it is in.
    pub(crate) fn insert_toc(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if self.toc_start_kind(None).is_some() {
            return self.update_toc(None, cx);
        }
        let instr = flow_command::TocInstruction::default();
        if self.run_toc(start.para, false, &instr, cx) {
            // The pages moved with the table of contents: numbered again, in
            // the same step of undo.
            self.relayout_if_changed();
            if let Some((first, kind)) = self.toc_start_kind(None) {
                let latest_instr = match kind {
                    TocType::Custom(i) => i,
                    TocType::Headings(lvls) => flow_command::TocInstruction { heading_levels: Some(lvls), ..Default::default() },
                    _ => instr,
                };
                if let Some(command) = self.toc_command(first, true, &latest_instr) {
                    let _ = self.document.amend_last(vec![Command::Flow(command)]);
                }
            }
            cx.notify();
        }
    }

    /// 挿入 > 手動作成の目次: placeholder entries (levels 1 to 3) before the caret's
    /// paragraph, to type over; the `toc N` styles it uses are defined first, in the same step.
    pub(crate) fn insert_manual_toc(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        if flow_command::locate(doc, start.para).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        let missing = flow_command::missing_toc_styles_for_levels(&doc.styles, [1, 2, 3]);
        let styled = !missing.is_empty();
        if styled && !self.run_flow(missing.into_iter().map(|(id, s)| FlowCommand::SetStyle { id, style: Some(s) }).collect(), cx) {
            return;
        }
        let Some(doc) = self.flow() else { return };
        let width = doc.page.size.width - doc.page.left - doc.page.right;
        let entries = flow_command::manual_toc_entries(doc, doc.new_id(), width);
        let command = FlowCommand::SetToc { before: start.para, replace: false, entries, bookmarks: Vec::new() };
        let ok = if styled { self.document.amend_last(vec![Command::Flow(command)]).is_ok() } else { self.run_flow(vec![command], cx) };
        if ok {
            self.relayout_if_changed();
            cx.notify();
        }
    }

    /// The page (its number) each paragraph starts on, for an index.
    fn index_page_of(&self) -> impl Fn(u32) -> Option<u32> + use<> {
        let pages: std::collections::HashMap<u32, u32> = match self.flow() {
            Some(doc) => {
                let laid = super::flow_layout::pages(doc, &self.resources);
                let mut map = std::collections::HashMap::new();
                for pg in &laid.pages {
                    for item in &pg.items {
                        if let super::flow_layout::Item::Line { para, .. } = item {
                            map.entry(*para).or_insert(pg.number);
                        }
                    }
                }
                map
            }
            None => Default::default(),
        };
        move |id| pages.get(&id).copied()
    }

    /// 挿入 > 索引: an index (INDEX) of the entries the XE fields mark, before the caret's paragraph; if the document has one, it is made again.
    pub(crate) fn insert_index(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if super::index_fields::index_field(self.flow().map(|d| &**d).unwrap_or(&FlowDocument::default())).is_some() {
            return self.update_index(cx);
        }
        self.run_index(start.para, false, "INDEX \\h \"A\" \\z \"1041\"", cx);
    }

    /// 索引の更新: the index made again from the XE fields now.
    pub(crate) fn update_index(&mut self, cx: &mut Context<Self>) {
        let Some((first, instruction)) = self.flow().and_then(|d| super::index_fields::index_field(d)) else { return };
        self.run_index(first, true, &instruction, cx);
    }

    fn run_index(&mut self, before: u32, replace: bool, instruction: &str, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let page_of = self.index_page_of();
        let commands = super::index_fields::index_commands(doc, before, replace, instruction, &page_of);
        if self.run_flow(commands, cx) {
            // The pages moved with the index: numbered again, in the same step of undo.
            self.relayout_if_changed();
            if let Some((first, instruction)) = self.flow().and_then(|d| super::index_fields::index_field(d)) {
                let page_of = self.index_page_of();
                if let Some(doc) = self.flow() {
                    let again: Vec<Command> = super::index_fields::index_commands(doc, first, true, &instruction, &page_of).into_iter().filter(|c| matches!(c, FlowCommand::SetToc { .. })).map(Command::Flow).collect();
                    let _ = self.document.amend_last(again);
                }
            }
            cx.notify();
        }
    }

    /// 挿入 > 引用文献目録: a table of authorities (TOA) of the citations the TA fields mark, before the caret's paragraph; if the document
    /// has one, it is made again.
    pub(crate) fn insert_toa(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if super::toa_fields::toa_field(self.flow().map(|d| &**d).unwrap_or(&FlowDocument::default())).is_some() {
            return self.update_toa(cx);
        }
        self.run_toa(start.para, false, "TOA \\h \\c \"0\" \\p", cx);
    }

    /// 引用文献目録の更新: the table made again from the TA fields now.
    pub(crate) fn update_toa(&mut self, cx: &mut Context<Self>) {
        let Some((first, instruction)) = self.flow().and_then(|d| super::toa_fields::toa_field(d)) else { return };
        self.run_toa(first, true, &instruction, cx);
    }

    fn run_toa(&mut self, before: u32, replace: bool, instruction: &str, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let page_of = self.index_page_of();
        let commands = super::toa_fields::toa_commands(doc, before, replace, instruction, &page_of);
        if self.run_flow(commands, cx) {
            // The pages moved with the table: numbered again, in the same step of undo.
            self.relayout_if_changed();
            if let Some((first, instruction)) = self.flow().and_then(|d| super::toa_fields::toa_field(d)) {
                let page_of = self.index_page_of();
                if let Some(doc) = self.flow() {
                    let again: Vec<Command> = super::toa_fields::toa_commands(doc, first, true, &instruction, &page_of).into_iter().filter(|c| matches!(c, FlowCommand::SetToc { .. })).map(Command::Flow).collect();
                    let _ = self.document.amend_last(again);
                }
            }
            cx.notify();
        }
    }

    /// 引用文献の登録: a TA field after the selected text (the long citation is that text), in `category` (1 to 6).
    pub(crate) fn mark_citation(&mut self, category: u32, cx: &mut Context<Self>) {
        let Some((start, end)) = self.ordered_selection() else { return };
        if start == end || start.para != end.para {
            self.status = Some(crate::shell::i18n::text("docx.editing.select-text-mark-citation").into());
            return cx.notify();
        }
        let text = self.selected_text().replace('"', "\\\"");
        let kind = FlowRunKind::Field { instruction: format!("TA \\l \"{text}\" \\s \"{text}\" \\c {category} "), shown: String::new() };
        if self.run_flow(vec![FlowCommand::InsertField { at: end, kind }], cx) {
            self.set_caret(FlowPos::new(end.para, end.offset + 1), false, cx);
        }
    }

    /// 引用文献の挿入 (DOCX-REF-014 / docx.reference.insert-citation):
    /// Inserts a `{ CITATION <tag> \l 1041 }` field with shown `(<tag>)` at the caret or replaces selected text.
    pub(crate) fn insert_citation(&mut self, tag: &str, cx: &mut Context<Self>) {
        let tag = tag.trim();
        if tag.is_empty() { return; }
        let Some((start, end)) = self.ordered_selection() else { return };
        let mut commands = Vec::new();
        if start != end {
            commands.push(FlowCommand::Delete { from: start, to: end });
        }
        let kind = FlowRunKind::Field {
            instruction: format!("CITATION {tag} \\l 1041 "),
            shown: format!("({tag})"),
        };
        commands.push(FlowCommand::InsertField { at: start, kind });
        if self.run_flow(commands, cx) {
            self.set_caret(FlowPos::new(start.para, start.offset + 1), false, cx);
            let msg = crate::shell::i18n::format("docx.reference.citation.inserted", &[("tag", &tag)]);
            self.status = Some(msg.into());
            cx.notify();
        }
    }

    /// 引用文献の挿入（選択テキストまたは既定タグ）
    pub(crate) fn insert_citation_at_selection(&mut self, cx: &mut Context<Self>) {
        let tag = if let Some((start, end)) = self.ordered_selection() && start != end && start.para == end.para {
            self.selected_text()
        } else {
            let count = self.flow().map(|d| super::bibliography::collect_citations(d).len()).unwrap_or(0);
            let default_prefix = crate::shell::i18n::text("docx.reference.citation.default-tag");
            format!("{default_prefix}{}", count + 1)
        };
        self.insert_citation(&tag, cx);
    }

    /// 挿入 > 文献目録 (DOCX-REF-014 / docx.reference.bibliography):
    /// Inserts a bibliography field before the caret's paragraph (or updates existing).
    pub(crate) fn insert_bibliography(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if super::bibliography::bibliography_field(self.flow().map(|d| &**d).unwrap_or(&FlowDocument::default())).is_some() {
            return self.update_bibliography(cx);
        }
        self.run_bibliography(start.para, false, "BIBLIOGRAPHY \\l 1041", cx);
    }

    /// 文献目録の更新: updates existing bibliography field from current citations.
    pub(crate) fn update_bibliography(&mut self, cx: &mut Context<Self>) {
        let Some((first, instruction)) = self.flow().and_then(|d| super::bibliography::bibliography_field(d)) else { return };
        self.run_bibliography(first, true, &instruction, cx);
    }

    fn run_bibliography(&mut self, before: u32, replace: bool, instruction: &str, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let count = super::bibliography::collect_citations(doc).len();
        let commands = super::bibliography::bibliography_commands(doc, before, replace, instruction);
        if self.run_flow(commands, cx) {
            let key = if replace { "docx.reference.bibliography.updated" } else { "docx.reference.bibliography.inserted" };
            let count_str = count.to_string();
            let msg = crate::shell::i18n::format(key, &[("count", &count_str)]);
            self.status = Some(msg.into());
            cx.notify();
        }
    }

    /// 索引登録: an XE field after the selected text, with that text as the entry (the selection in one paragraph).
    pub(crate) fn mark_index_entry(&mut self, cx: &mut Context<Self>) {
        let Some((start, end)) = self.ordered_selection() else { return };
        if start == end || start.para != end.para {
            self.status = Some(crate::shell::i18n::text("docx.editing.select-text-mark-index-entry").into());
            return cx.notify();
        }
        let text = self.selected_text().replace('"', "\\\"");
        let kind = FlowRunKind::Field { instruction: format!("XE \"{text}\" "), shown: String::new() };
        if self.run_flow(vec![FlowCommand::InsertField { at: end, kind }], cx) {
            self.set_caret(FlowPos::new(end.para, end.offset + 1), false, cx);
        }
    }

    /// 目次項目の登録: a TC field (`TC "text" \l level`) after the selected text, which `TOC \f` collects (the selection in one
    /// paragraph).
    pub(crate) fn mark_toc_entry(&mut self, level: u32, cx: &mut Context<Self>) {
        let Some((start, end)) = self.ordered_selection() else { return };
        if start == end || start.para != end.para {
            self.status = Some(crate::shell::i18n::text("docx.editing.select-text-mark-toc-entry").into());
            return cx.notify();
        }
        let text = self.selected_text().replace('"', "\\\"");
        let kind = FlowRunKind::Field { instruction: format!("TC \"{text}\" \\l {level} "), shown: String::new() };
        if self.run_flow(vec![FlowCommand::InsertField { at: end, kind }], cx) {
            self.set_caret(FlowPos::new(end.para, end.offset + 1), false, cx);
        }
    }

    /// 挿入 > 目次項目から作る目次: `TOC \f \l "1-9"` before the caret's paragraph (the one there is updated).
    pub(crate) fn insert_toc_from_entries(&mut self, cx: &mut Context<Self>) {
        let instruction = flow_command::TocInstruction { heading_levels: None, use_outline_levels: false, tc_entries: Some(None), ..Default::default() };
        self.insert_toc_with_instruction(instruction, cx);
    }

    /// 任意スタイル指定（\t スイッチなど）の目次を挿入する。
    pub(crate) fn insert_toc_with_instruction(&mut self, instruction: flow_command::TocInstruction, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if self.run_toc(start.para, false, &instruction, cx) {
            self.relayout_if_changed();
            if let Some((first, _)) = self.toc_start_kind(None) {
                if let Some(command) = self.toc_command(first, true, &instruction) {
                    let _ = self.document.amend_last(vec![Command::Flow(command)]);
                }
            }
            cx.notify();
        }
    }

    /// 挿入 > 図表目次: a table of figures for `label` before the caret's paragraph.
    pub(crate) fn insert_table_of_figures(&mut self, label: &str, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        if self.flow().and_then(|d| flow_command::locate(d, start.para)).map(|l| l.0) != Some(Container::Body) {
            return;
        }
        if let Some(first) = self.run_table_of_figures(start.para, false, label, cx) {
            self.relayout_if_changed();
            if let Some(command) = self.table_of_figures_command(first, true, label) {
                let _ = self.document.amend_last(vec![Command::Flow(command)]);
            }
            cx.notify();
        }
    }

    fn table_of_figures_command(&self, before: u32, replace: bool, label: &str) -> Option<FlowCommand> {
        let doc = self.flow()?;
        let width = doc.page.size.width - doc.page.left - doc.page.right;
        let lines = self.flow_lines();
        let first = doc.page.first_page;
        let page_of = |id: u32| lines.iter().find(|l| l.para == id).map(|l| first + l.page as u32);
        let (entries, bookmarks) = flow_command::table_of_figures_entries(doc, label, &page_of, doc.new_id(), width);
        Some(FlowCommand::SetToc { before, replace, entries, bookmarks })
    }

    fn run_table_of_figures(&mut self, before: u32, replace: bool, label: &str, cx: &mut Context<Self>) -> Option<u32> {
        let missing = self.flow().and_then(|d| flow_command::missing_table_of_figures_style(&d.styles));
        let styled = missing.is_some();
        if let Some((id, s)) = missing {
            if !self.run_flow(vec![FlowCommand::SetStyle { id, style: Some(s) }], cx) {
                return None;
            }
        }
        let Some(command) = self.table_of_figures_command(before, replace, label) else {
            return if styled { Some(before) } else { None };
        };
        let first_id = match &command {
            FlowCommand::SetToc { entries, .. } => entries.first().map(|e| e.id),
            _ => None,
        };
        let ok = if styled {
            self.document.amend_last(vec![Command::Flow(command)]).is_ok()
        } else {
            self.run_flow(vec![command], cx)
        };
        if ok { first_id } else { None }
    }

    /// 目次の更新: the entries made again from the headings or figures or custom styles (with `levels`,
    /// or the table's own).
    pub(crate) fn update_toc(&mut self, levels: Option<(u32, u32)>, cx: &mut Context<Self>) {
        let caret_para = self.text_selection().map(|(_, c)| c.para);
        let Some((first, kind)) = caret_para.and_then(|p| self.toc_start_kind(Some(p))).or_else(|| self.toc_start_kind(None)) else { return };
        let caret = self.text_selection();
        match kind {
            TocType::Figures(label) => {
                if let Some(new_first) = self.run_table_of_figures(first, true, &label, cx) {
                    self.relayout_if_changed();
                    if let Some(command) = self.table_of_figures_command(new_first, true, &label) {
                        let _ = self.document.amend_last(vec![Command::Flow(command)]);
                    }
                }
            }
            TocType::Headings(own) => {
                let instr = flow_command::TocInstruction {
                    heading_levels: Some(levels.unwrap_or(own)),
                    ..Default::default()
                };
                if self.run_toc(first, true, &instr, cx) {
                    self.relayout_if_changed();
                    if let Some((first, _)) = self.toc_start_kind(None) {
                        if let Some(command) = self.toc_command(first, true, &instr) {
                            let _ = self.document.amend_last(vec![Command::Flow(command)]);
                        }
                    }
                }
            }
            TocType::Custom(mut instr) => {
                if let Some(lvls) = levels {
                    instr.heading_levels = Some(lvls);
                }
                if self.run_toc(first, true, &instr, cx) {
                    self.relayout_if_changed();
                    if let Some((first, _)) = self.toc_start_kind(None) {
                        if let Some(command) = self.toc_command(first, true, &instr) {
                            let _ = self.document.amend_last(vec![Command::Flow(command)]);
                        }
                    }
                }
            }
        }
        // The caret stays where it was if its paragraph does.
        if let Some((_, c)) = caret
            && self.flow().is_some_and(|d| flow_command::paragraph(d, c.para).is_none())
            && let Some(start) = self.document_edge(false)
        {
            self.set_caret(start, false, cx);
        }
        cx.notify();
    }

    /// 目次をすべて更新する (Update entire table):
    /// 見出しや図表の最新構成から目次全体を再構築する（update_toc と同等）。
    #[allow(dead_code)]
    pub(crate) fn update_toc_entire_table(&mut self, levels: Option<(u32, u32)>, cx: &mut Context<Self>) {
        self.update_toc(levels, cx);
    }

    /// ページ番号だけを更新する (Update page numbers only):
    /// 目次の各エントリーのテキストや構成は変更せず、各項目のページ番号 (PageRef / PAGEREF) のみ
    /// 最新のレイアウトに基づいて更新する（1ステップの Undo/Redo）。
    pub(crate) fn update_toc_page_numbers_only(&mut self, cx: &mut Context<Self>) -> bool {
        let caret_para = self.text_selection().map(|(_, c)| c.para);
        let Some((first, _kind)) = caret_para.and_then(|p| self.toc_start_kind(Some(p))).or_else(|| self.toc_start_kind(None)) else {
            return false;
        };
        let Some(doc) = self.flow() else { return false };
        let Some((container, start_idx)) = flow_command::locate(doc, first) else { return false };
        if container != Container::Body {
            return false;
        }

        // 目次の範囲 (start_idx から field_end まで)
        let mut end_idx = start_idx;
        for i in start_idx..doc.body.len() {
            if let Block::Paragraph(p) = &doc.body[i] {
                end_idx = i;
                if p.field_end {
                    break;
                }
            } else {
                break;
            }
        }

        let lines = self.flow_lines();
        let first_page = doc.page.first_page;
        let page_of = |id: u32| lines.iter().find(|l| l.para == id).map(|l| first_page + l.page as u32);

        let mut updated_blocks = Vec::new();
        let mut any_changed = false;

        for i in start_idx..=end_idx {
            let Block::Paragraph(p) = &doc.body[i] else {
                updated_blocks.push(doc.body[i].clone());
                continue;
            };
            let mut p_updated = p.clone();
            let mut changed = false;
            for run in &mut p_updated.runs {
                match &mut run.kind {
                    FlowRunKind::PageRef { bookmark, shown } => {
                        if let Some(target_id) = find_bookmark(doc, bookmark) {
                            if let Some(pg) = page_of(target_id) {
                                let new_shown = pg.to_string();
                                if *shown != new_shown {
                                    *shown = new_shown;
                                    changed = true;
                                }
                            }
                        }
                    }
                    FlowRunKind::Field { instruction, shown } => {
                        let words = flow_command::field_words(instruction);
                        if words.first().is_some_and(|w| w.eq_ignore_ascii_case("PAGEREF")) {
                            if let Some(bookmark) = words.get(1) {
                                if let Some(target_id) = find_bookmark(doc, bookmark) {
                                    if let Some(pg) = page_of(target_id) {
                                        let new_shown = pg.to_string();
                                        if *shown != new_shown {
                                            *shown = new_shown;
                                            changed = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            if changed {
                any_changed = true;
            }
            updated_blocks.push(Block::Paragraph(p_updated));
        }

        if any_changed {
            let count = end_idx - start_idx + 1;
            let command = FlowCommand::ReplaceBlocks {
                container: Container::Body,
                index: start_idx,
                remove: count,
                blocks: updated_blocks,
            };
            let ok = self.run_flow(vec![command], cx);
            if ok {
                self.relayout_if_changed();
                cx.notify();
            }
            ok
        } else {
            false
        }
    }

    /// Follow the link at `pos` to its bookmark (the caret to the start of
    /// its paragraph, scrolled into view): ⌘-click. Whether it went.
    /// Whether ⌘-clicking at `pos` follows a link to a bookmark.
    pub(crate) fn link_at_position(&self, pos: FlowPos) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut at = 0;
        for run in &p.runs {
            let len = run.len();
            if pos.offset >= at && pos.offset < at + len.max(1) {
                return match &run.link {Some(ibis_doc_model::Link::Url(url))=>!url.is_empty(),Some(ibis_doc_model::Link::Anchor(name))=>find_bookmark(doc,name).is_some(),None=>false};
            }
            at += len;
        }
        false
    }

    pub(crate) fn follow_link(&mut self, pos: FlowPos, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let Some(p) = flow_command::paragraph(doc, pos.para) else { return false };
        let mut at = 0;
        let mut link = None;
        for run in &p.runs {
            let len = run.len();
            if pos.offset >= at && pos.offset < at + len.max(1) {
                link = run.link.clone();
                break;
            }
            at += len;
        }
        if let Some(ibis_doc_model::Link::Url(url))=&link {crate::shell::links::open_external_url(url,cx);return true;}
        let Some(ibis_doc_model::Link::Anchor(name)) = link else { return false };
        let Some(target) = find_bookmark(doc, &name) else { return false };
        self.jump_to(target, cx);
        true
    }

    /// The caret to the start of paragraph `id`, scrolled to the top.
    pub(crate) fn jump_to(&mut self, id: u32, cx: &mut Context<Self>) {
        self.set_caret(FlowPos::new(id, 0), false, cx);
        self.scroll_to_caret(cx);
    }

    /// The headings (level, text, paragraph) for the outline.
    pub(crate) fn outline(&self) -> Vec<(u32, String, u32)> {
        let Some(doc) = self.flow() else { return Vec::new() };
        doc.body
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph(p) => flow_command::heading_level(&doc.styles, p).map(|l| (l, p.text().chars().filter(|c| !matches!(c, '\t' | '\u{b}' | '\u{c}' | '\u{e}' | '\u{fffc}')).collect::<String>(), p.id)),
                _ => None,
            })
            .filter(|(_, t, _)| !t.trim().is_empty())
            .collect()
    }

    /// 編集記号の表示／非表示をトグル切替する（¶ボタン）。
    pub(crate) fn toggle_formatting_marks(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_open = !self.formatting_marks_open;
        let show_hidden = self.formatting_marks_options.is_hidden_text_visible(self.formatting_marks_open);
        super::flow_layout::set_show_hidden_text(show_hidden);
        self.relayout_now(cx);
        cx.notify();
    }

    /// 垂直ルーラーの表示／非表示をトグル切替する (DOCX-VIEW-012).
    pub(crate) fn toggle_vertical_ruler(&mut self, cx: &mut Context<Self>) {
        self.vertical_ruler_open = !self.vertical_ruler_open;
        cx.notify();
    }

    /// ルーラーの測定単位を取得する (DOCX-VIEW-013).
    pub(crate) fn ruler_unit(&self) -> RulerUnit {
        self.ruler_unit
    }

    /// ルーラーの測定単位を設定する (DOCX-VIEW-013).
    pub(crate) fn set_ruler_unit(&mut self, unit: RulerUnit, cx: &mut Context<Self>) {
        self.ruler_unit = unit;
        cx.notify();
    }

    /// ルーラーの測定単位を順送りで切り替える (DOCX-VIEW-013).
    pub(crate) fn cycle_ruler_unit(&mut self, cx: &mut Context<Self>) {
        self.ruler_unit = self.ruler_unit.next();
        cx.notify();
    }

    /// ページ間の余白の表示／非表示を取得する (DOCX-VIEW-017).
    pub(crate) fn show_white_space(&self) -> bool {
        self.show_white_space
    }

    /// ページ間の余白の表示／非表示をトグル切替する (DOCX-VIEW-017).
    pub(crate) fn toggle_white_space(&mut self, cx: &mut Context<Self>) {
        self.show_white_space = !self.show_white_space;
        cx.notify();
    }

    /// ページ間の余白の表示／非表示を設定する (DOCX-VIEW-017).
    pub(crate) fn set_show_white_space(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_white_space = show;
        cx.notify();
    }

    /// 現在の表示設定に基づくページ間隔 (pixels) を取得する (DOCX-VIEW-017, DOCX-VIEW-002, DOCX-VIEW-003).
    pub(crate) fn page_gap(&self) -> f32 {
        if self.is_web_layout_mode() || self.is_draft_mode() {
            super::doc_ui::COLLAPSED_PAGE_GAP
        } else if self.show_white_space {
            super::doc_ui::PAGE_GAP
        } else {
            super::doc_ui::COLLAPSED_PAGE_GAP
        }
    }

    /// 1ページ表示のズーム倍率を計算する (DOCX-VIEW-009).
    /// 文書のページ全体（幅と高さ両方）がワークスペース領域内に収まる倍率。
    pub(crate) fn calc_zoom_one_page(&self) -> Option<f32> {
        let pages = self.page_sizes();
        if pages.is_empty() {
            return None;
        }
        let current = self.visible_page().min(pages.len().saturating_sub(1));
        let page_size = pages.get(current).copied().unwrap_or(pages[0]);
        let scale_100 = zoom_scale(1.0);
        let px_at_100_w = page_size.width as f32 * scale_100;
        let px_at_100_h = page_size.height as f32 * scale_100;
        if px_at_100_w <= 0.0 || px_at_100_h <= 0.0 {
            return None;
        }
        let (view_w, view_h) = self.workspace_bounds.map_or((1000.0, 700.0), |ws| (f32::from(ws.size.width), f32::from(ws.size.height)));
        let gap = self.page_gap();
        let avail_w = (view_w - 2.0 * super::doc_ui::PAGE_GAP).max(40.0);
        let avail_h = (view_h - 2.0 * gap).max(40.0);
        let zoom_w = avail_w / px_at_100_w;
        let zoom_h = avail_h / px_at_100_h;
        let zoom = zoom_w.min(zoom_h).clamp(MIN_ZOOM, MAX_ZOOM);
        Some(((zoom * 100.0).round() / 100.0).clamp(MIN_ZOOM, MAX_ZOOM))
    }

    /// 複数ページ（2ページ横並び・縦並び）表示のズーム倍率を計算する (DOCX-VIEW-009).
    /// 複数ページがワークスペース領域内に収まる倍率。
    pub(crate) fn calc_zoom_multiple_pages(&self) -> Option<f32> {
        let pages = self.page_sizes();
        if pages.is_empty() {
            return None;
        }
        let current = self.visible_page().min(pages.len().saturating_sub(1));
        let page_size = pages.get(current).copied().unwrap_or(pages[0]);
        let scale_100 = zoom_scale(1.0);
        let px_at_100_w = page_size.width as f32 * scale_100;
        let px_at_100_h = page_size.height as f32 * scale_100;
        if px_at_100_w <= 0.0 || px_at_100_h <= 0.0 {
            return None;
        }
        let (view_w, view_h) = self.workspace_bounds.map_or((1000.0, 700.0), |ws| (f32::from(ws.size.width), f32::from(ws.size.height)));
        let gap = self.page_gap();
        let avail_w = (view_w - 2.0 * super::doc_ui::PAGE_GAP).max(40.0);
        let avail_h = (view_h - gap).max(40.0);
        let zoom_w = avail_w / px_at_100_w;
        let zoom_h = avail_h / (2.0 * px_at_100_h);
        let one_page_zoom = self.calc_zoom_one_page().unwrap_or(1.0);
        let zoom = zoom_w.min(zoom_h).min(one_page_zoom * 0.7).clamp(MIN_ZOOM, MAX_ZOOM);
        Some(((zoom * 100.0).round() / 100.0).clamp(MIN_ZOOM, MAX_ZOOM))
    }

    /// 現在の表示倍率が「1ページ」表示に該当するか判定する (DOCX-VIEW-009).
    pub(crate) fn is_zoom_one_page(&self) -> bool {
        match (self.zoom, self.calc_zoom_one_page()) {
            (Some(z), Some(target)) => (z - target).abs() < 0.015,
            _ => false,
        }
    }

    /// 現在の表示倍率が「複数ページ」表示に該当するか判定する (DOCX-VIEW-009).
    pub(crate) fn is_zoom_multiple_pages(&self) -> bool {
        match (self.zoom, self.calc_zoom_multiple_pages()) {
            (Some(z), Some(target)) => (z - target).abs() < 0.015,
            _ => false,
        }
    }

    /// 現在の表示倍率が「ページ幅」表示に該当するか判定する (DOCX-VIEW-009).
    pub(crate) fn is_zoom_page_width(&self) -> bool {
        self.zoom.is_none()
    }

    /// 現在の表示倍率が「100%」表示に該当するか判定する (DOCX-VIEW-009).
    pub(crate) fn is_zoom_100(&self) -> bool {
        self.zoom.is_some_and(|z| (z - 1.0).abs() < 0.01)
    }

    /// 1ページ全体が収まるようにズーム倍率を設定する (DOCX-VIEW-009).
    pub(crate) fn zoom_one_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.calc_zoom_one_page() {
            let current = self.visible_page();
            self.set_zoom(Some(target), window, cx);
            self.scroll_to_page(current, cx);
        }
    }

    /// 複数ページが収まるようにズーム倍率を設定する (DOCX-VIEW-009).
    pub(crate) fn zoom_multiple_pages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.calc_zoom_multiple_pages() {
            let current = self.visible_page();
            self.set_zoom(Some(target), window, cx);
            self.scroll_to_page(current, cx);
        }
    }

    /// ページ幅に合わせてズーム倍率を設定する (DOCX-VIEW-009).
    pub(crate) fn zoom_page_width(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(None, window, cx);
    }

    /// 100% ズーム倍率を設定する (DOCX-VIEW-009).
    pub(crate) fn zoom_100_percent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(Some(1.0), window, cx);
    }

    /// スペース記号の表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_spaces(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_spaces = !self.formatting_marks_options.show_spaces;
        cx.notify();
    }

    /// タブ記号の表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_tabs(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_tabs = !self.formatting_marks_options.show_tabs;
        cx.notify();
    }

    /// 段落記号の表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_paragraph_marks(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_paragraph_marks = !self.formatting_marks_options.show_paragraph_marks;
        cx.notify();
    }

    /// 改行記号の表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_line_breaks(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_line_breaks = !self.formatting_marks_options.show_line_breaks;
        cx.notify();
    }

    /// 隠し文字の常時表示をトグル切替 (DOCX-OPTIONS-008).
    pub(crate) fn toggle_show_hidden_text(&mut self, cx: &mut Context<Self>) {
        self.formatting_marks_options.show_hidden_text = !self.formatting_marks_options.show_hidden_text;
        let show_hidden = self.formatting_marks_options.is_hidden_text_visible(self.formatting_marks_open);
        super::flow_layout::set_show_hidden_text(show_hidden);
        self.relayout_now(cx);
        cx.notify();
    }

    /// 文書が最終版としてマークされているか（Word の「最終版にする」）。
    pub(crate) fn is_mark_as_final(&self) -> bool {
        self.flow().is_some_and(|d| d.mark_as_final)
    }

    /// 読み取り専用を推奨する設定になっているか（Word の「読み取り専用を推奨する」）。
    pub(crate) fn is_read_only_recommended(&self) -> bool {
        self.flow().is_some_and(|d| d.read_only_recommended)
    }

    /// 読み取り専用推奨の情報バーで「編集を有効にする」を押したか。
    pub(crate) fn is_read_only_dismissed(&self) -> bool {
        self.doc_state().is_some_and(|s| s.read_only_dismissed)
    }

    /// 読み取り専用推奨の情報バーを閉じて編集を有効にする。
    pub(crate) fn dismiss_read_only(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.read_only_dismissed = true;
            cx.notify();
        }
    }

    /// 最終版としてマークを設定・解除する。
    pub(crate) fn set_mark_as_final(&mut self, mark: bool, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetMarkAsFinal(mark)], cx);
    }

    /// 最終版としてマークをトグル切替する。
    pub(crate) fn toggle_mark_as_final(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_mark_as_final();
        self.set_mark_as_final(next, cx);
    }

    /// 最終版マークを解除して編集を可能にする（Word の「それでも編集する」）。
    pub(crate) fn edit_anyway(&mut self, cx: &mut Context<Self>) {
        self.set_mark_as_final(false, cx);
    }

    /// 読み取り専用を推奨を設定・解除する。
    pub(crate) fn set_read_only_recommended(&mut self, ro: bool, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetReadOnlyRecommended(ro)], cx);
    }

    /// 読み取り専用を推奨をトグル切替する。
    pub(crate) fn toggle_read_only_recommended(&mut self, cx: &mut Context<Self>) {
        let next = !self.is_read_only_recommended();
        self.set_read_only_recommended(next, cx);
    }

    /// 文書の編集制限（保護モード）を取得。
    pub(crate) fn document_protection(&self) -> DocumentProtection {
        self.flow().map_or(DocumentProtection::None, |d| d.protection)
    }

    /// 文書が読み取り専用で保護されているか。
    pub(crate) fn is_protection_read_only(&self) -> bool {
        self.document_protection() == DocumentProtection::ReadOnly
    }

    /// 文書がコメントのみ許可で保護されているか。
    pub(crate) fn is_protection_comments(&self) -> bool {
        self.document_protection() == DocumentProtection::Comments
    }

    /// 文書がフォーム入力のみ許可で保護されているか。
    pub(crate) fn is_protection_forms(&self) -> bool {
        self.document_protection() == DocumentProtection::Forms
    }

    /// 文書の書式変更が制限されているか。
    pub(crate) fn is_formatting_protected(&self) -> bool {
        self.flow().is_some_and(|d| d.formatting_protection)
    }

    /// 書式変更の制限を設定する。
    pub(crate) fn set_formatting_protection(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetFormattingProtection(enabled)], cx);
    }

    /// 編集制限（保護モード）を設定する。
    pub(crate) fn set_document_protection(&mut self, protection: DocumentProtection, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetDocumentProtection(protection)], cx);
    }

    /// 編集制限を解除する。
    pub(crate) fn stop_protection(&mut self, cx: &mut Context<Self>) {
        let mut commands = Vec::new();
        if self.document_protection() != DocumentProtection::None {
            commands.push(FlowCommand::SetDocumentProtection(DocumentProtection::None));
        }
        if self.is_formatting_protected() {
            commands.push(FlowCommand::SetFormattingProtection(false));
        }
        if !commands.is_empty() {
            self.run_flow(commands, cx);
        }
    }

    /// 文書のテーマフォント言語・既定言語設定（ECMA-376 Part 1 §17.15.1.88 `w:themeFontLang`）を取得。
    pub(crate) fn theme_font_lang(&self) -> Option<ThemeFontLang> {
        self.flow().and_then(|d| d.theme_font_lang.clone())
    }

    /// 文書のテーマフォント言語・既定言語設定を設定する。
    pub(crate) fn set_theme_font_lang(&mut self, lang: Option<ThemeFontLang>, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetThemeFontLang(lang)], cx);
    }

    /// 現在の選択位置・キャレット位置の書式言語を文書の既定言語（themeFontLang）として設定する（Word の「既定に設定…」）。
    pub(crate) fn set_current_as_default_language(&mut self, cx: &mut Context<Self>) {
        let current_format = self.current_format().map(|(f, _)| f);
        let mut tfl = self.theme_font_lang().unwrap_or(ThemeFontLang {
            val: Some("en-US".into()),
            east_asia: Some("ja-JP".into()),
            bidi: Some("ar-SA".into()),
        });
        if let Some(f) = current_format {
            if let Some(l) = f.lang {
                tfl.val = Some(l);
            }
            if let Some(ea) = f.lang_east_asia {
                tfl.east_asia = Some(ea);
            }
            if let Some(b) = f.lang_bidi {
                tfl.bidi = Some(b);
            }
        }
        self.set_theme_font_lang(Some(tfl), cx);
    }

    /// Word 標準の情報バー（編集制限 / 最終版 / 読み取り専用の推奨 / 閲覧モード）。
    pub(crate) fn render_docx_infobar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.is_protection_read_only() {
            Some(Self::docx_infobar_banner(
                "docx-infobar-protected-readonly",
                crate::shell::i18n::text("docx.infobar.protect.readonly.title"),
                "docx.infobar.protect.readonly.message",
                "infobar-btn-stop-protection",
                "docx.infobar.protect.stop",
                |this, _, cx| this.stop_protection(cx),
                cx,
            ))
        } else if self.is_protection_comments() {
            Some(Self::docx_infobar_banner(
                "docx-infobar-protected-comments",
                crate::shell::i18n::text("docx.infobar.protect.comments.title"),
                "docx.infobar.protect.comments.message",
                "infobar-btn-stop-protection",
                "docx.infobar.protect.stop",
                |this, _, cx| this.stop_protection(cx),
                cx,
            ))
        } else if self.is_protection_forms() {
            Some(Self::docx_infobar_banner(
                "docx-infobar-protected-forms",
                crate::shell::i18n::text("docx.infobar.protect.forms.title"),
                "docx.infobar.protect.forms.message",
                "infobar-btn-stop-protection",
                "docx.infobar.protect.stop",
                |this, _, cx| this.stop_protection(cx),
                cx,
            ))
        } else if self.is_formatting_protected() {
            Some(Self::docx_infobar_banner(
                "docx-infobar-protected-formatting",
                crate::shell::i18n::text("docx.infobar.protect.formatting.title"),
                "docx.infobar.protect.formatting.message",
                "infobar-btn-stop-protection",
                "docx.infobar.protect.stop",
                |this, _, cx| this.stop_protection(cx),
                cx,
            ))
        } else if self.is_mark_as_final() {
            Some(Self::docx_infobar_banner(
                "docx-infobar-final",
                crate::shell::i18n::text("docx.infobar.final.title"),
                "docx.infobar.final.message",
                "infobar-btn-edit-anyway",
                "docx.infobar.final.edit-anyway",
                |this, _, cx| this.edit_anyway(cx),
                cx,
            ))
        } else if self.is_read_only_recommended() && !self.is_read_only_dismissed() {
            Some(Self::docx_infobar_banner(
                "docx-infobar-readonly",
                crate::shell::i18n::text("docx.infobar.readonly.title"),
                "docx.infobar.readonly.message",
                "infobar-btn-enable-editing",
                "docx.infobar.readonly.enable-editing",
                |this, _, cx| this.dismiss_read_only(cx),
                cx,
            ))
        } else if self.is_read_mode() {
            Some(Self::docx_infobar_banner(
                "docx-infobar-readmode",
                crate::shell::i18n::text("docx.infobar.readmode.title"),
                "docx.infobar.readmode.message",
                "infobar-btn-exit-readmode",
                "docx.infobar.readmode.exit",
                |this, window, cx| this.set_read_mode(false, window, cx),
                cx,
            ))
        } else if self.is_viewing_mode() {
            Some(Self::docx_infobar_banner(
                "docx-infobar-viewing",
                crate::shell::i18n::text("docx.infobar.viewing.title"),
                "docx.infobar.viewing.message",
                "infobar-btn-switch-to-editing",
                "docx.infobar.viewing.edit",
                |this, window, cx| this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Editing, window, cx),
                cx,
            ))
        } else if self.is_docx_compatibility_mode() {
            let mode_name = self.docx_compatibility_mode_name();
            let title_text: SharedString = format!("{} ({})", crate::shell::i18n::text("docx.infobar.compat.title"), mode_name).into();
            Some(Self::docx_infobar_banner(
                "docx-infobar-compat",
                title_text,
                "docx.infobar.compat.message",
                "infobar-btn-convert-compat",
                "docx.infobar.compat.convert",
                |this, _, cx| this.convert_docx_to_current_format(cx),
                cx,
            ))
        } else {
            None
        }
    }

    #[inline(never)]
    fn docx_infobar_banner(
        bar_id: &'static str,
        title: SharedString,
        msg_key: &'static str,
        btn_id: &'static str,
        btn_key: &'static str,
        on_click: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(bar_id)
            .w_full()
            .bg(rgb(0xfff4ce))
            .border_b_1()
            .border_color(rgb(0xedebe9))
            .px(px(16.))
            .py(px(6.))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(div().text_size(px(12.)).font_weight(FontWeight::BOLD).text_color(rgb(0x323130)).child(title))
                    .child(div().text_size(px(12.)).text_color(rgb(0x605e5c)).child(crate::shell::i18n::text(msg_key))),
            )
            .child(
                div()
                    .id(btn_id)
                    .px(px(12.))
                    .py(px(4.))
                    .rounded_sm()
                    .border_1()
                    .border_color(rgb(0x8a8886))
                    .bg(rgb(0xffffff))
                    .text_size(px(12.))
                    .text_color(rgb(0x323130))
                    .cursor_pointer()
                    .hover(|h| h.bg(rgb(0xf3f2f1)))
                    .child(crate::shell::i18n::text(btn_key))
                    .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx))),
            )
            .into_any_element()
    }

    /// Escape キー押下時の処理 (Immersive Reader 解除 > Focus Mode 解除 > Read Mode / Viewing Mode 解除 > 書式のコピー/貼り付け解除)。
    pub(crate) fn handle_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc_state().is_some_and(|s| s.shape_drag.is_some() || s.draw_box.is_some() || s.dragging) {
            if let Some(state) = self.doc_state_mut() {
                state.shape_drag = None;
                state.draw_box = None;
                state.dragging = false;
            }
            cx.notify();
            return;
        }
        if self.doc_state().is_some_and(|s| s.box_edit.is_some()) {
            self.commit_box_edit(window, cx);
            return;
        }
        if self.doc_state().is_some_and(|s| s.shape_selected) {
            if let Some(state) = self.doc_state_mut() {
                state.shape_selected = false;
                state.selected_shapes.clear();
            }
            cx.notify();
            return;
        }
        if self.is_immersive_reader_enabled() {
            self.close_immersive_reader(window, cx);
            return;
        }
        if self.is_focus_mode() {
            self.set_focus_mode(false, window, cx);
            return;
        }
        if self.is_read_mode() || self.is_viewing_mode() {
            self.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Editing, window, cx);
            return;
        }
        self.cancel_doc_format_brush(cx);
    }

    /// 変更履歴の表示モードを取得。
    pub(crate) fn markup_display_mode(&self) -> crate::docx::flow_layout::MarkupDisplayMode {
        crate::docx::flow_layout::markup_display_mode()
    }

    /// 変更履歴の表示モードを設定（シンプルな変更履歴・すべての変更履歴・変更履歴なし・元の文書）。
    pub(crate) fn set_markup_display_mode(&mut self, mode: crate::docx::flow_layout::MarkupDisplayMode, cx: &mut Context<Self>) {
        crate::docx::flow_layout::set_markup_display_mode(mode);
        self.relayout_now(cx);
        cx.notify();
    }

    /// 上書き入力モードかどうか (Word's Overtype mode).
    pub(crate) fn is_overtype(&self) -> bool {
        self.doc_state().is_some_and(|s| s.overtype)
    }

    /// 上書き入力モードをトグル切替。
    pub(crate) fn toggle_overtype(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.overtype = !state.overtype;
        }
        cx.notify();
    }

    /// 上書き入力モードを設定。
    #[allow(dead_code)]
    pub(crate) fn set_overtype(&mut self, overtype: bool, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.overtype = overtype;
        }
        cx.notify();
    }

    /// 文書内の変更履歴およびコメントに記録されているすべての校閲者（作成者 author）一覧を収集（DOCX-REVIEW-005）。
    pub(crate) fn revision_authors(&self) -> Vec<String> {
        let Some(doc) = self.flow() else { return Vec::new() };
        let mut authors = std::collections::BTreeSet::new();

        fn collect_from_blocks(blocks: &[ibis_doc_model::Block], authors: &mut std::collections::BTreeSet<String>) {
            for b in blocks {
                match b {
                    ibis_doc_model::Block::Paragraph(p) => {
                        for r in &p.runs {
                            if let Some(rev) = &r.revision {
                                if !rev.author.is_empty() {
                                    authors.insert(rev.author.clone());
                                }
                            }
                        }
                    }
                    ibis_doc_model::Block::Table(t) => {
                        for row in &t.rows {
                            for cell in &row.cells {
                                collect_from_blocks(&cell.blocks, authors);
                            }
                        }
                    }
                }
            }
        }

        collect_from_blocks(&doc.body, &mut authors);
        collect_from_blocks(&doc.header, &mut authors);
        collect_from_blocks(&doc.footer, &mut authors);
        collect_from_blocks(&doc.header_first, &mut authors);
        collect_from_blocks(&doc.footer_first, &mut authors);
        collect_from_blocks(&doc.header_even, &mut authors);
        collect_from_blocks(&doc.footer_even, &mut authors);
        for note in doc.footnotes.iter().chain(doc.endnotes.iter()) {
            collect_from_blocks(&note.blocks, &mut authors);
        }
        for comment in &doc.comments {
            if !comment.author.is_empty() {
                authors.insert(comment.author.clone());
            }
        }
        authors.into_iter().collect()
    }

    /// すべての校閲者が表示対象か（DOCX-REVIEW-005）。
    pub(crate) fn is_all_reviewers(&self) -> bool {
        self.reviewer_filter.is_none()
    }

    /// 特定の校閲者が表示対象か（DOCX-REVIEW-005）。
    pub(crate) fn is_reviewer_shown(&self, author: &str) -> bool {
        self.reviewer_filter.as_ref().map_or(true, |set| set.contains(author))
    }

    /// 「すべての校閲者」の表示・非表示を一括設定（DOCX-REVIEW-005）。
    pub(crate) fn set_all_reviewers(&mut self, on: bool, cx: &mut Context<Self>) {
        if on {
            self.reviewer_filter = None;
        } else {
            self.reviewer_filter = Some(std::collections::HashSet::new());
        }
        crate::docx::flow_layout::set_shown_reviewers(self.reviewer_filter.clone());
        cx.notify();
    }

    /// 特定の校閲者の表示・非表示をトグル切替（DOCX-REVIEW-005）。
    pub(crate) fn toggle_reviewer(&mut self, author: &str, cx: &mut Context<Self>) {
        let all_authors = self.revision_authors();
        let mut set = match &self.reviewer_filter {
            Some(s) => s.clone(),
            None => all_authors.iter().cloned().collect(),
        };
        if set.contains(author) {
            set.remove(author);
        } else {
            set.insert(author.to_string());
        }
        if set.len() == all_authors.len() {
            self.reviewer_filter = None;
        } else {
            self.reviewer_filter = Some(set);
        }
        crate::docx::flow_layout::set_shown_reviewers(self.reviewer_filter.clone());
        cx.notify();
    }

    // =========================================================================
    // マスター文書とサブ文書 (DOCX-TOC-016)
    // =========================================================================

    /// 文書内のすべてのサブ文書段落 `(para_id, SubdocumentInfo)` を返す (DOCX-TOC-016).
    pub(crate) fn subdocuments(&self) -> Vec<(u32, ibis_doc_model::SubdocumentInfo)> {
        let Some(doc) = self.flow() else { return Vec::new() };
        let mut out = Vec::new();
        for b in &doc.body {
            if let ibis_doc_model::Block::Paragraph(p) = b
                && let Some(ref sub) = p.subdoc
            {
                out.push((p.id, sub.clone()));
            }
        }
        out
    }

    /// キャレット位置の段落のサブ文書情報を返す (DOCX-TOC-016).
    pub(crate) fn subdocument_at_caret(&self) -> Option<(u32, ibis_doc_model::SubdocumentInfo)> {
        let (start, _) = self.ordered_selection()?;
        let p = self.paragraph_of(start.para)?;
        p.subdoc.clone().map(|s| (p.id, s))
    }

    /// キャレット位置の段落にサブ文書への参照（`w:subDoc`）を設定・挿入（1ステップ Undo/Redo 対応、DOCX-TOC-016）.
    pub(crate) fn insert_subdocument(&mut self, target: &str, cx: &mut Context<Self>) -> bool {
        let target = target.trim();
        if target.is_empty() {
            return false;
        }
        let Some((start, _)) = self.ordered_selection() else { return false };
        let Some(p) = self.paragraph_of(start.para) else { return false };
        let existing_text = p.text();
        let title = if existing_text.trim().is_empty() {
            target.to_string()
        } else {
            existing_text.trim().to_string()
        };
        let subdoc = ibis_doc_model::SubdocumentInfo {
            target: target.to_string(),
            title,
            expanded: true,
            locked: false,
            content_paragraphs: Vec::new(),
        };
        let ok = self.run_flow(
            vec![ibis_doc_model::FlowCommand::SetParagraphSubdocument {
                para: start.para,
                subdoc: Some(subdoc),
            }],
            cx,
        );
        cx.notify();
        ok
    }

    /// 見出しセクション（現在の見出し段落と配下の本文段落）をサブ文書に変換（1ステップ Undo/Redo 対応、DOCX-TOC-016）.
    pub(crate) fn create_subdocument_from_heading(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((start, _)) = self.ordered_selection() else { return false };
        let Some(doc) = self.flow().cloned() else { return false };
        let Some(idx) = doc.body.iter().position(|b| matches!(b, ibis_doc_model::Block::Paragraph(p) if p.id == start.para)) else {
            return false;
        };
        let ibis_doc_model::Block::Paragraph(head_p) = &doc.body[idx] else { return false };
        let head_id = head_p.id;
        let head_lvl = ibis_doc_model::flow_command::heading_level(&doc.styles, head_p).unwrap_or(1);
        let head_title = head_p.text().trim().to_string();
        let slug: String = head_title
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .take(24)
            .collect();
        let target = if slug.is_empty() {
            format!("subdoc_{head_id}.docx")
        } else {
            format!("{slug}.docx")
        };

        let mut content_paragraphs = Vec::new();
        let mut last_body_para_id = None;
        let mut last_body_para_len = 0;
        for b in &doc.body[idx + 1..] {
            match b {
                ibis_doc_model::Block::Paragraph(p) => {
                    if let Some(lvl) = ibis_doc_model::flow_command::heading_level(&doc.styles, p)
                        && lvl <= head_lvl
                    {
                        break;
                    }
                    if p.subdoc.is_some() {
                        break;
                    }
                    content_paragraphs.push(p.text());
                    last_body_para_id = Some(p.id);
                    last_body_para_len = p.len();
                }
                ibis_doc_model::Block::Table(_) => break,
            }
        }

        let mut cmds = Vec::new();
        if let Some(last_id) = last_body_para_id {
            cmds.push(ibis_doc_model::FlowCommand::Delete {
                from: ibis_doc_model::FlowPos::new(head_id, head_p.len()),
                to: ibis_doc_model::FlowPos::new(last_id, last_body_para_len),
            });
        }
        let subdoc = ibis_doc_model::SubdocumentInfo {
            target,
            title: if head_title.is_empty() { format!("Subdocument {head_id}") } else { head_title },
            expanded: false,
            locked: false,
            content_paragraphs,
        };
        cmds.push(ibis_doc_model::FlowCommand::SetParagraphSubdocument {
            para: head_id,
            subdoc: Some(subdoc),
        });
        let ok = self.run_flow(cmds, cx);
        if ok {
            self.set_caret(ibis_doc_model::FlowPos::new(head_id, 0), false, cx);
        }
        cx.notify();
        ok
    }

    /// サブ文書のリンクを解除してマスター文書に統合・展開（1ステップ Undo/Redo 対応、DOCX-TOC-016）.
    pub(crate) fn unlink_subdocument_at_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((para_id, subdoc)) = self.subdocument_at_caret() else { return false };
        let Some(p) = self.paragraph_of(para_id) else { return false };
        let end_offset = p.len();
        let mut cmds = vec![ibis_doc_model::FlowCommand::SetParagraphSubdocument {
            para: para_id,
            subdoc: None,
        }];
        if !subdoc.content_paragraphs.is_empty() {
            let joined = format!("\n{}", subdoc.content_paragraphs.join("\n"));
            cmds.push(ibis_doc_model::FlowCommand::InsertText {
                at: ibis_doc_model::FlowPos::new(para_id, end_offset),
                text: joined,
                format: None,
            });
            cmds.push(ibis_doc_model::FlowCommand::SetParagraphFormat {
                first: para_id,
                last: para_id,
                change: ibis_doc_model::ParaChange::Style(p.style.clone()),
            });
        }
        let ok = self.run_flow(cmds, cx);
        cx.notify();
        ok
    }

    /// 文書内のすべてのサブ文書の展開 / 折りたたみを一括トグル (DOCX-TOC-016).
    pub(crate) fn toggle_expand_subdocuments(&mut self, cx: &mut Context<Self>) -> bool {
        let subs = self.subdocuments();
        if subs.is_empty() {
            return false;
        }
        let target_expanded = !subs.iter().all(|(_, s)| s.expanded);
        let cmds: Vec<ibis_doc_model::FlowCommand> = subs
            .into_iter()
            .map(|(para, mut s)| {
                s.expanded = target_expanded;
                ibis_doc_model::FlowCommand::SetParagraphSubdocument {
                    para,
                    subdoc: Some(s),
                }
            })
            .collect();
        let ok = self.run_flow(cmds, cx);
        cx.notify();
        ok
    }

    /// キャレット位置のサブ文書のロック（読み取り専用）をトグル (DOCX-TOC-016).
    pub(crate) fn toggle_subdocument_lock_at_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((para, mut sub)) = self.subdocument_at_caret() else { return false };
        sub.locked = !sub.locked;
        let ok = self.run_flow(
            vec![ibis_doc_model::FlowCommand::SetParagraphSubdocument {
                para,
                subdoc: Some(sub),
            }],
            cx,
        );
        cx.notify();
        ok
    }

    /// 指定した段落 `para` のサブ文書のロック状態を設定 (DOCX-TOC-016).
    pub(crate) fn set_subdocument_locked(&mut self, para: u32, locked: bool, cx: &mut Context<Self>) -> bool {
        let Some(p) = self.paragraph_of(para) else { return false };
        let Some(mut sub) = p.subdoc.clone() else { return false };
        sub.locked = locked;
        let ok = self.run_flow(
            vec![ibis_doc_model::FlowCommand::SetParagraphSubdocument {
                para,
                subdoc: Some(sub),
            }],
            cx,
        );
        cx.notify();
        ok
    }

    /// 隣接する2つのサブ文書を1つに結合（1ステップ Undo/Redo 対応、DOCX-TOC-016）.
    pub(crate) fn merge_adjacent_subdocuments(&mut self, cx: &mut Context<Self>) -> bool {
        let subs = self.subdocuments();
        if subs.len() < 2 {
            return false;
        }
        let caret_para = self.ordered_selection().map(|(s, _)| s.para);
        let pos = subs
            .iter()
            .position(|(id, _)| Some(*id) == caret_para)
            .unwrap_or(0);
        let idx = if pos + 1 < subs.len() { pos } else { subs.len() - 2 };
        let (first_id, mut first_sub) = subs[idx].clone();
        let (second_id, second_sub) = subs[idx + 1].clone();
        let Some(first_p) = self.paragraph_of(first_id) else { return false };
        let Some(second_p) = self.paragraph_of(second_id) else { return false };

        if !second_sub.title.is_empty() {
            first_sub.content_paragraphs.push(second_sub.title.clone());
        }
        first_sub
            .content_paragraphs
            .extend(second_sub.content_paragraphs.iter().cloned());
        let mut cmds = Vec::new();
        if second_sub.locked {
            let mut unlocked = second_sub.clone();
            unlocked.locked = false;
            cmds.push(ibis_doc_model::FlowCommand::SetParagraphSubdocument {
                para: second_id,
                subdoc: Some(unlocked),
            });
        }
        cmds.push(ibis_doc_model::FlowCommand::SetParagraphSubdocument {
            para: second_id,
            subdoc: None,
        });
        cmds.push(ibis_doc_model::FlowCommand::Delete {
            from: ibis_doc_model::FlowPos::new(first_id, first_p.len()),
            to: ibis_doc_model::FlowPos::new(second_id, second_p.len()),
        });
        cmds.push(ibis_doc_model::FlowCommand::SetParagraphSubdocument {
            para: first_id,
            subdoc: Some(first_sub),
        });
        let ok = self.run_flow(cmds, cx);
        cx.notify();
        ok
    }

    /// キャレット位置のサブ文書を2つのサブ文書に分割（1ステップ Undo/Redo 対応、DOCX-TOC-016）.
    pub(crate) fn split_subdocument_at_caret(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((para_id, mut first_sub)) = self.subdocument_at_caret() else { return false };
        let Some(doc) = self.flow() else { return false };
        let Some(p) = self.paragraph_of(para_id) else { return false };
        let new_para_id = doc.new_id();
        let p_len = p.len();
        let p_style = p.style.clone();

        let second_content = if first_sub.content_paragraphs.len() >= 2 {
            let mid = first_sub.content_paragraphs.len() / 2;
            first_sub.content_paragraphs.split_off(mid)
        } else {
            Vec::new()
        };
        let stem = first_sub.target.trim_end_matches(".docx");
        let second_target = format!("{stem}_part2.docx");
        let second_title = format!("{} (2)", first_sub.title);
        let second_sub = ibis_doc_model::SubdocumentInfo {
            target: second_target,
            title: second_title.clone(),
            expanded: first_sub.expanded,
            locked: false,
            content_paragraphs: second_content,
        };

        let was_locked = first_sub.locked;
        let mut cmds = Vec::new();
        if was_locked {
            let mut unlocked = first_sub.clone();
            unlocked.locked = false;
            cmds.push(ibis_doc_model::FlowCommand::SetParagraphSubdocument {
                para: para_id,
                subdoc: Some(unlocked),
            });
        }
        cmds.push(ibis_doc_model::FlowCommand::InsertText {
            at: ibis_doc_model::FlowPos::new(para_id, p_len),
            text: format!("\n{second_title}"),
            format: None,
        });
        if let Some(style) = p_style {
            cmds.push(ibis_doc_model::FlowCommand::SetParagraphFormat {
                first: new_para_id,
                last: new_para_id,
                change: ibis_doc_model::ParaChange::Style(Some(style)),
            });
        }
        first_sub.locked = was_locked;
        cmds.push(ibis_doc_model::FlowCommand::SetParagraphSubdocument {
            para: para_id,
            subdoc: Some(first_sub),
        });
        cmds.push(ibis_doc_model::FlowCommand::SetParagraphSubdocument {
            para: new_para_id,
            subdoc: Some(second_sub),
        });
        let ok = self.run_flow(cmds, cx);
        cx.notify();
        ok
    }

    /// マスター文書（サブ文書管理）サイドバーパネルを開く (DOCX-TOC-016).
    pub(crate) fn open_master_doc_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        use gpui_kit::component::input::InputState;
        let default_target = self
            .subdocument_at_caret()
            .map(|(_, s)| s.target)
            .unwrap_or_else(|| "chapter1.docx".to_string());
        let target_input = cx.new(|cx| InputState::new(window, cx).default_value(default_target));
        if let Some(s) = self.doc_state_mut() {
            s.master_doc_panel = Some(MasterDocPanelState { target_input });
            s.theme_panel = None;
            s.compatibility_panel = None;
            s.inspect_panel = None;
            s.docx_math = None;
            s.quick_parts = None;
            s.table_formula = None;
            s.table_size = None;
            s.picture_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.style_edit = None;
            s.link = None;
            s.find = None;
            s.comments = None;
            s.list_style = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// マスター文書サイドバーパネルを描画 (DOCX-TOC-016).
    pub(crate) fn master_doc_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        use gpui_kit::component::{Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex};
        let panel = self.doc_state()?.master_doc_panel.as_ref()?;
        let target_input = panel.target_input.clone();
        let subs = self.subdocuments();
        let caret_sub = self.subdocument_at_caret();
        let t = crate::shell::i18n::text;

        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(div().text_sm().child(t("docx.master-doc.target-label")))
            .child(crate::shell::chrome::text_field(&target_input).full_width())
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(
                        crate::shell::chrome::primary_button("master-doc-insert")
                            .small()
                            .label(t("docx.master-doc.insert"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let path = target_input.read(cx).value().trim().to_string();
                                this.insert_subdocument(&path, cx);
                            })),
                    )
                    .child(
                        crate::shell::chrome::action_button("master-doc-create-heading")
                            .small()
                            .label(t("docx.master-doc.create-from-heading"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.create_subdocument_from_heading(cx);
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(
                        crate::shell::chrome::action_button("master-doc-toggle-expand")
                            .small()
                            .label(t("docx.master-doc.toggle-expand"))
                            .disabled(subs.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_expand_subdocuments(cx);
                            })),
                    )
                    .child(
                        crate::shell::chrome::action_button("master-doc-lock")
                            .small()
                            .label(t("docx.master-doc.lock"))
                            .disabled(caret_sub.is_none())
                            .selected(caret_sub.as_ref().is_some_and(|(_, s)| s.locked))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_subdocument_lock_at_caret(cx);
                            })),
                    )
                    .child(
                        crate::shell::chrome::destructive_button("master-doc-unlink",cx)
                            .small()
                            .label(t("docx.master-doc.unlink"))
                            .disabled(caret_sub.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.unlink_subdocument_at_caret(cx);
                            })),
                    )
                    .child(
                        crate::shell::chrome::action_button("master-doc-merge")
                            .small()
                            .label(t("docx.master-doc.merge"))
                            .disabled(subs.len() < 2)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.merge_adjacent_subdocuments(cx);
                            })),
                    )
                    .child(
                        crate::shell::chrome::action_button("master-doc-split")
                            .small()
                            .label(t("docx.master-doc.split"))
                            .disabled(caret_sub.is_none())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.split_subdocument_at_caret(cx);
                            })),
                    ),
            );

        if subs.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(t("docx.master-doc.empty")));
        } else {
            for (idx, (para_id, sub)) in subs.into_iter().enumerate() {
                let locked = sub.locked;
                let expanded = sub.expanded;
                body = body.child(
                    h_flex()
                        .id(("subdoc-row", idx))
                        .gap_1()
                        .items_center()
                        .p_1()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(0xe0e0e0))
                        .child(
                            div()
                                .id(("subdoc-title", idx))
                                .flex_1()
                                .text_sm()
                                .cursor_pointer()
                                .child(format!(
                                    "{} ({}){}{}",
                                    sub.title,
                                    sub.target,
                                    if expanded { String::new() } else { format!(" {}", t("docx.master-doc.collapsed-tag")) },
                                    if locked { " 🔒" } else { "" }
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.jump_to(para_id, cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::action_button(("subdoc-lock-btn", idx))
                                .small()
                                .label(if locked { "🔒" } else { "🔓" })
                                .selected(locked)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_subdocument_locked(para_id, !locked, cx);
                                })),
                        ),
                );
            }
        }

        Some(crate::shell::chrome::SidebarPanel {
            title: t("docx.master-doc.title"),
            body: body.into_any_element(),
        })
    }
}

pub(crate) struct MasterDocPanelState {
    pub(crate) target_input: Entity<gpui_kit::component::input::InputState>,
}


/// The paragraph bookmark `name` starts in.
fn find_bookmark(doc: &FlowDocument, name: &str) -> Option<u32> {
    fn walk(blocks: &[Block], name: &str) -> Option<u32> {
        blocks.iter().find_map(|b| match b {
            Block::Paragraph(p) => p.bookmarks.iter().any(|b| b == name).then_some(p.id),
            Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).find_map(|c| walk(&c.blocks, name)),
        })
    }
    walk(&doc.body, name)
}
