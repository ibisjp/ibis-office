// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメント (X5): the document's side of the comments' panel (shell/comments.rs, the one panel of the three
//! formats): the threads of `comments.xml` in the order of their places in the text, and the operations on the
//! flow document (`flow_command::{add,reply,edit,resolve,delete}_comment`). A comment is put on the selection
//! (挿入 > コメント, ⌥⌘M).

use gpui_kit::*;
use ibis_doc_model::flow_command;
use ibis_doc_model::{Block, FlowDocument, FlowPos};

use crate::shell::chrome::SidebarPanel;
use crate::shell::comments::{self, CommentFormat, CommentPanelState, CommentPost, CommentThread};
use crate::shell::editor::SlideEditor;

// Who writes and when is shared with the other formats' comments (shell/comments.rs).
pub use crate::shell::comments::comment_author;
use crate::ext::*;

/// The panel's state: the comments' common one, with the selected range as the place of a new comment.
pub(crate) type CommentState = CommentPanelState<(FlowPos, FlowPos)>;

/// The document's comments for `shell::comments`.
pub struct DocxComments;

/// A comment's text.
fn comment_text(blocks: &[Block]) -> String {
    blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect::<Vec<_>>().join("\n")
}
/// Calculate the render position `(page, y)` of the anchor of comment `comment_id`.
pub(crate) fn comment_anchor_position(editor: &SlideEditor, doc: &FlowDocument, comment_id: u32) -> (usize, f32) {
    if let Some((from, _)) = flow_command::comment_range(doc, comment_id) {
        let lines = editor.flow_lines();
        for l in &lines {
            if l.para == from.para {
                let min_off = l.stops.first().map_or(0, |s| s.0);
                let max_off = l.stops.last().map_or(min_off, |s| s.0);
                if from.offset >= min_off && from.offset <= max_off {
                    return (l.page, l.y);
                }
            }
        }
        for l in &lines {
            if l.para == from.para {
                return (l.page, l.y);
            }
        }
    }
    (usize::MAX, f32::MAX)
}


impl DocxComments {
    /// The thread named `id` (a comment's number) as its comment's number.
    fn number(id: &str) -> Option<u32> { id.parse().ok() }

    /// The thread that the comment at the caret belongs to.
    fn at_caret(editor: &SlideEditor) -> Option<String> {
        let caret = editor.doc_state()?.caret?;
        let doc = editor.flow()?;
        let id = flow_command::comment_at_pos(doc, caret)?;
        Some(doc.comments.iter().find(|c| c.id == id).and_then(|c| c.parent).unwrap_or(id).to_string())
    }
}

impl CommentFormat for DocxComments {
    type Anchor = (FlowPos, FlowPos);

    fn state(editor: &SlideEditor) -> Option<&CommentState> { editor.doc_state()?.comments.as_ref() }
    fn state_mut(editor: &mut SlideEditor) -> Option<&mut CommentState> { editor.doc_state_mut()?.comments.as_mut() }
    fn install(editor: &mut SlideEditor, state: CommentState) {
        if let Some(s) = editor.doc_state_mut() { s.comments = Some(state); }
    }
    fn close_others(editor: &mut SlideEditor) {
        if let Some(state) = editor.doc_state_mut() {
            state.tab_edit = None;
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
        }
    }
    fn close(editor: &mut SlideEditor) {
        if let Some(s) = editor.doc_state_mut() { s.comments = None; }
    }

    fn threads(editor: &SlideEditor) -> Vec<CommentThread> {
        let Some(doc) = editor.flow() else { return Vec::new() };
        let mut parents: Vec<_> = doc.comments.iter().filter(|c| c.parent.is_none()).collect();
        parents.sort_by(|a, b| {
            let pos_a = comment_anchor_position(editor, doc, a.id);
            let pos_b = comment_anchor_position(editor, doc, b.id);
            pos_a.0.cmp(&pos_b.0).then_with(|| pos_a.1.total_cmp(&pos_b.1)).then_with(|| a.id.cmp(&b.id))
        });
        let post = |c: &ibis_doc_model::flow::Comment| CommentPost {
            id: c.id.to_string(),
            author: c.author.clone(),
            date: c.date.clone(),
            text: comment_text(&c.blocks),
            editable: comments::is_own(&c.author),
        };
        parents
            .into_iter()
            .map(|c| CommentThread {
                id: c.id.to_string(),
                anchor: None,
                posts: std::iter::once(post(c)).chain(doc.comments.iter().filter(|r| r.parent == Some(c.id)).map(post)).collect(),
                resolved: c.done,
            })
            .collect()
    }

    fn active_thread(editor: &SlideEditor) -> Option<String> {
        Self::state(editor)?.active.clone().or_else(|| Self::at_caret(editor))
    }

    fn new_prompt(_editor: &SlideEditor, _anchor: Option<&(FlowPos, FlowPos)>) -> SharedString {
        crate::shell::i18n::text("shell.comments.new-selection")
    }

    fn idle_hint() -> Option<&'static str> { Some("shell.comments.hint-select-text") }

    fn add(editor: &mut SlideEditor, &(from, to): &(FlowPos, FlowPos), text: &str, cx: &mut Context<SlideEditor>) -> bool {
        let who = comment_author();
        let Some(commands) = editor.flow().and_then(|doc| flow_command::add_comment(doc, from, to, &who, text)) else { return false };
        let keep = editor.text_selection();
        let ran = editor.run_flow(commands, cx);
        if ran
            && let Some(state) = editor.doc_state_mut()
            && let Some((_, c)) = keep
        {
            // The marks moved the selection's end along.
            state.anchor = None;
            state.caret = Some(c);
        }
        ran
    }

    fn reply(editor: &mut SlideEditor, thread: &str, text: &str, cx: &mut Context<SlideEditor>) -> bool {
        let who = comment_author();
        let Some(id) = Self::number(thread) else { return false };
        let Some(commands) = editor.flow().and_then(|doc| flow_command::reply_comment(doc, id, &who, text)) else { return false };
        editor.run_flow(commands, cx)
    }

    fn edit(editor: &mut SlideEditor, _thread: &str, post: &str, text: &str, cx: &mut Context<SlideEditor>) -> bool {
        let Some(id) = Self::number(post) else { return false };
        let Some(command) = editor.flow().and_then(|doc| flow_command::edit_comment(doc, id, text)) else { return false };
        editor.run_flow(vec![command], cx)
    }

    fn set_resolved(editor: &mut SlideEditor, thread: &str, resolved: bool, cx: &mut Context<SlideEditor>) {
        let Some(id) = Self::number(thread) else { return };
        let Some(command) = editor.flow().map(|doc| flow_command::resolve_comment(doc, id, resolved)) else { return };
        editor.run_flow(vec![command], cx);
    }

    fn delete(editor: &mut SlideEditor, thread: &str, cx: &mut Context<SlideEditor>) {
        let Some(id) = Self::number(thread) else { return };
        let Some(command) = editor.flow().and_then(|d| flow_command::delete_comment(d, id)) else { return };
        editor.run_flow(vec![command], cx);
    }

    fn goto(editor: &mut SlideEditor, thread: &str, cx: &mut Context<SlideEditor>) {
        let Some(id) = Self::number(thread) else { return };
        if let Some((from, to)) = editor.flow().and_then(|doc| flow_command::comment_range(doc, id)) {
            editor.jump_to_range(from, to, cx);
        }
    }
}

#[ibis_office_macros::editor_ext(Comments)]
impl SlideEditor {
    /// 挿入 > コメント: the panel, its field for a comment on the selection.
    pub fn open_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let anchor = self.ordered_selection().filter(|(a, b)| a != b);
        comments::open::<DocxComments>(self, anchor, window, cx);
        let active = DocxComments::at_caret(self);
        if let (Some(active), Some(state)) = (active, DocxComments::state_mut(self)) {
            state.active = Some(active);
        }
    }

    /// Jump to comment `id`: select its range, scroll to it, and mark it active in the panel.
    pub(crate) fn focus_comment(&mut self, id: u32, cx: &mut Context<Self>) {
        comments::activate::<DocxComments>(self, &id.to_string(), cx);
    }

    /// The field's text written as the comment or answer it is for.
    pub fn send_comment(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        comments::send::<DocxComments>(self, text, window, cx);
    }

    /// 返信: the field for an answer to comment `id`.
    pub fn reply_to(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        comments::begin_reply::<DocxComments>(self, &id.to_string(), window, cx);
    }

    /// 返信欄や新規コメント欄を閉じる（キャンセル）。
    pub(crate) fn cancel_comment_input(&mut self, cx: &mut Context<Self>) {
        let rest = DocxComments::rest_target(self);
        if let Some(state) = DocxComments::state_mut(self) {
            state.target = rest;
        }
        cx.notify();
    }

    /// 解決 (or 再開): the comment and its answers resolved or opened again.
    pub(crate) fn resolve(&mut self, id: u32, done: bool, cx: &mut Context<Self>) {
        DocxComments::set_resolved(self, &id.to_string(), done, cx);
    }

    /// 削除の確認: the question before the comment goes (the common message box).
    pub(crate) fn confirm_delete_comment(&mut self, id: u32, cx: &mut Context<Self>) {
        comments::ask_delete::<DocxComments>(self, &id.to_string(), cx);
    }

    /// 削除: the comment, its answers and their marks.
    pub(crate) fn delete_comment(&mut self, id: u32, cx: &mut Context<Self>) {
        DocxComments::delete(self, &id.to_string(), cx);
    }

    pub fn comments_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        comments::panel::<DocxComments>(self, cx)
    }
}
