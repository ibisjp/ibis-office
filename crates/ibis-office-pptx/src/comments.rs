// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメント: the presentation's side of the comments' panel (shell/comments.rs, the one panel of the three
//! formats): the slide's comments (the modern comments with their answers) and their operations as one step to
//! undo (`Command::SetComments`), where a comment is put on the slide or the selected shape (挿入 > コメント,
//! ⌥⌘M); and their marks on the slide (the editor only, not the slide show, printing or exports).

use gpui_kit::*;
use ibis_doc_model::{Author, Command, CommentReply, EmuPoint, ShapeId, SlideComment};

use crate::shell::chrome::SidebarPanel;
use crate::shell::comments::{self, CommentFormat, CommentPanelState, CommentPost, CommentThread};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The panel's state: the comments' common one, with the shape (none: the slide) as the place of a new comment.
pub(crate) type CommentsPanel = CommentPanelState<Option<ShapeId>>;

/// The presentation's comments for `shell::comments`.
pub struct PptxComments;

/// Who writes and now, as the panel puts them on a comment (the time as PowerPoint writes it, without a zone letter).
fn author_and_time() -> (Author, String) {
    let who = comments::author_and_date();
    (Author { initials: crate::shell::user_name::initials(&who.author), name: who.author }, who.date.trim_end_matches('Z').to_string())
}

/// Format an ISO 8601 date string as the comments' panel shows a time (`comments::comment_date_label`).
pub(crate) fn format_comment_date(raw: &str) -> String {
    comments::comment_date_label(raw)
}

/// A comment's thread id: its own, else its place in the slide's list.
fn thread_key(index: usize, c: &SlideComment) -> String {
    if c.id.is_empty() { format!("#{index}") } else { c.id.clone() }
}

impl PptxComments {
    fn index_of(editor: &SlideEditor, key: &str) -> Option<usize> {
        editor.slide().comments.iter().enumerate().position(|(i, c)| thread_key(i, c) == key)
    }
}

/// Where comment `c`'s mark is on `slide`: its place, else the top right
/// corner of its shape, else the slide's top left.
pub(crate) fn mark_place(slide: &ibis_doc_model::Slide, c: &SlideComment) -> EmuPoint {
    c.position.or_else(|| c.shape.and_then(|id| slide.shape(id)).map(|s| EmuPoint::new(s.frame.right(), s.frame.y))).unwrap_or(EmuPoint::new(0, 0))
}

impl CommentFormat for PptxComments {
    type Anchor = Option<ShapeId>;

    fn state(editor: &SlideEditor) -> Option<&CommentsPanel> { editor.pptx()?.comments.as_ref() }
    fn state_mut(editor: &mut SlideEditor) -> Option<&mut CommentsPanel> { editor.pptx_mut()?.comments.as_mut() }
    fn install(editor: &mut SlideEditor, state: CommentsPanel) {
        if let Some(s) = editor.pptx_mut() { s.comments = Some(state); }
    }
    fn close_others(editor: &mut SlideEditor) {
        editor.shape_edit.gradient_panel = None; editor.shape_edit.shadow_panel = None; editor.shape_edit.glow_panel = None; editor.shape_edit.paragraph_panel = None; editor.shape_edit.size_panel = None; editor.shape_edit.insets_panel = None;
        editor.shape_edit.find = None; editor.shape_edit.crop_panel = None; editor.shape_edit.frame_panel = None; editor.shape_edit.header_footer = None;
        if let Some(s) = editor.pptx_mut() {
            s.timing_panel = None; s.section_panel = None;          editor.shape_edit.chart_panel = None; 
        }
    }
    fn close(editor: &mut SlideEditor) {
        if let Some(s) = editor.pptx_mut() { s.comments = None; }
    }

    fn threads(editor: &SlideEditor) -> Vec<CommentThread> {
        editor
            .slide()
            .comments
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let root = CommentPost { id: "0".into(), author: c.author.name.clone(), date: c.created.clone(), text: c.text.clone(), editable: comments::is_own(&c.author.name) };
                let replies = c.replies.iter().enumerate().map(|(j, r)| CommentPost { id: (j + 1).to_string(), author: r.author.name.clone(), date: r.created.clone(), text: r.text.clone(), editable: comments::is_own(&r.author.name) });
                CommentThread { id: thread_key(i, c), anchor: None, posts: std::iter::once(root).chain(replies).collect(), resolved: c.resolved }
            })
            .collect()
    }

    fn active_thread(editor: &SlideEditor) -> Option<String> {
        if let Some(key) = Self::state(editor).and_then(|s| s.active.clone()) {
            return Some(key);
        }
        let shape = editor.selected_shapes().first().map(|s| s.id)?;
        editor.slide().comments.iter().enumerate().find(|(_, c)| c.shape == Some(shape)).map(|(i, c)| thread_key(i, c))
    }

    fn new_prompt(_editor: &SlideEditor, anchor: Option<&Option<ShapeId>>) -> SharedString {
        crate::shell::i18n::text(if matches!(anchor, Some(Some(_))) { "shell.comments.new-shape" } else { "shell.comments.new-slide" })
    }

    fn idle_hint() -> Option<&'static str> { Some("shell.comments.hint-insert") }

    fn add(editor: &mut SlideEditor, shape: &Option<ShapeId>, text: &str, cx: &mut Context<SlideEditor>) -> bool {
        let (author, created) = author_and_time();
        // On a shape its mark follows the shape; else the slide's top left.
        let position = shape.is_none().then(|| EmuPoint::new(0, 0));
        let comment = SlideComment { id: comments::new_guid(), author, created, text: text.to_string(), position, shape: *shape, resolved: false, replies: Vec::new() };
        editor.change_comments(|c| c.push(comment), cx)
    }

    fn reply(editor: &mut SlideEditor, thread: &str, text: &str, cx: &mut Context<SlideEditor>) -> bool {
        let Some(index) = Self::index_of(editor, thread) else { return false };
        let (author, created) = author_and_time();
        let reply = CommentReply { id: comments::new_guid(), author, created, text: text.to_string() };
        editor.change_comments(|c| { if let Some(c) = c.get_mut(index) { c.replies.push(reply) } }, cx)
    }

    fn edit(editor: &mut SlideEditor, thread: &str, post: &str, text: &str, cx: &mut Context<SlideEditor>) -> bool {
        let (Some(index), Ok(n)) = (Self::index_of(editor, thread), post.parse::<usize>()) else { return false };
        editor.change_comments(
            |c| {
                if let Some(c) = c.get_mut(index) {
                    match n.checked_sub(1) {
                        None => c.text = text.to_string(),
                        Some(j) => { if let Some(r) = c.replies.get_mut(j) { r.text = text.to_string() } }
                    }
                }
            },
            cx,
        )
    }

    fn set_resolved(editor: &mut SlideEditor, thread: &str, resolved: bool, cx: &mut Context<SlideEditor>) {
        let Some(index) = Self::index_of(editor, thread) else { return };
        editor.change_comments(|c| { if let Some(c) = c.get_mut(index) { c.resolved = resolved } }, cx);
    }

    fn delete(editor: &mut SlideEditor, thread: &str, cx: &mut Context<SlideEditor>) {
        let Some(index) = Self::index_of(editor, thread) else { return };
        editor.change_comments(|c| { if index < c.len() { c.remove(index); } }, cx);
    }

    fn goto(editor: &mut SlideEditor, thread: &str, cx: &mut Context<SlideEditor>) {
        let Some(shape) = Self::index_of(editor, thread).and_then(|i| editor.slide().comments[i].shape) else { return };
        if editor.pptx().is_some() {
            editor.shape_edit.selection = vec![shape];
        }
        cx.notify();
    }
}

#[ibis_office_macros::editor_ext(Comments)]
impl SlideEditor {
    /// 挿入 > コメント: the panel, its field for a comment on the selected shape (or the slide).
    pub fn open_slide_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let shape = self.selected_shapes().first().map(|s| s.id);
        comments::open::<PptxComments>(self, Some(shape), window, cx);
    }

    /// コメントを表示 off: the panel closed.
    pub(crate) fn close_slide_comments(&mut self, cx: &mut Context<Self>) {
        PptxComments::close(self);
        cx.notify();
    }

    /// The slide's comments changed by `change`, as one step to undo.
    fn change_comments(&mut self, change: impl FnOnce(&mut Vec<SlideComment>), cx: &mut Context<Self>) -> bool {
        let slide = self.current_slide;
        let mut comments = self.slide().comments.clone();
        change(&mut comments);
        if comments == self.slide().comments {
            return false;
        }
        self.run(vec![Command::SetComments { slide, comments }], cx);
        true
    }

    /// The field's text written as the comment or answer it is for.
    pub(crate) fn send_slide_comment(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        comments::send::<PptxComments>(self, text, window, cx);
    }

    /// 返信: the field for an answer to comment `index`.
    pub(crate) fn reply_to_slide_comment(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.slide().comments.get(index).map(|c| thread_key(index, c)) else { return };
        comments::begin_reply::<PptxComments>(self, &key, window, cx);
    }

    /// 解決 (or 再開).
    pub(crate) fn resolve_slide_comment(&mut self, index: usize, done: bool, cx: &mut Context<Self>) {
        self.change_comments(|c| { if let Some(c) = c.get_mut(index) { c.resolved = done } }, cx);
    }

    /// 削除: the comment and its answers.
    pub(crate) fn delete_slide_comment(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change_comments(|c| { if index < c.len() { c.remove(index); } }, cx);
    }

    /// Context menu entries for slide comment `index` (PPTX-COLLAB-001..004).
    pub(crate) fn comment_context_menu_entries(&self, index: usize) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(comment) = self.slide().comments.get(index) else { return Vec::new() };
        let done = comment.resolved;
        vec![
            MenuItem::tr("_shell.reply")
                .icon("reply")
                .on(move |e, w, cx| e.reply_to_slide_comment(index, w, cx))
                .into(),
            MenuItem::tr(if done { "pptx.comments.reopen" } else { "_shell.resolve" })
                .icon(if done { "undo" } else { "check_circle" })
                .on(move |e, _, cx| e.resolve_slide_comment(index, !done, cx))
                .into(),
            MenuItem::tr("_shell.delete")
                .icon("delete")
                .on(move |e, _, cx| e.delete_slide_comment(index, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("insert.comment")
                .icon("add_comment")
                .shortcut("⌥⌘M")
                .on(|e, w, cx| e.open_slide_comments(w, cx))
                .into(),
        ]
    }

    /// Open context menu for slide comment `index` at pointer `position`.
    pub(crate) fn open_comment_context_menu(&mut self, index: usize, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = self.comment_context_menu_entries(index);
        if !entries.is_empty() {
            self.open_popup_entries(entries, position, window, cx);
        }
    }

    pub fn slide_comments_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        comments::panel::<PptxComments>(self, cx)
    }
}
