// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメントスレッド (Threaded Comments): 挿入 > コメント (⌥⌘M) opens the sidebar's comments panel
//! (shell/comments.rs, the one panel of the three formats): all the threads of the sheet by cell, the
//! selected cell's marked, and the field for a new comment (the cell has none) or an answer.
//! This part is the workbook's side of it: the model's operations and the order of the list.

use ibis_doc_model::{Command, SheetCommand, ThreadPost, ThreadedComment};
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::comments::{self, CommentFormat, CommentPanelState, CommentPost, CommentTarget, CommentThread};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The panel's state: the comments' common one, with a cell (column, row) as the place of a new comment.
pub(crate) type ThreadedCommentPanel = CommentPanelState<(u32, u32)>;

/// The workbook's comments for `shell::comments`.
pub struct XlsxComments;

/// A thread's id as the panel names it: its GUID, else its cell.
fn thread_key(t: &ThreadedComment) -> String {
    if t.id.is_empty() { format!("cell:{},{}", t.column, t.row) } else { t.id.clone() }
}

impl XlsxComments {
    fn find(editor: &SlideEditor, key: &str) -> Option<ThreadedComment> {
        editor.current_sheet()?.1.threads.iter().find(|t| thread_key(t) == key).cloned()
    }

    fn set(editor: &mut SlideEditor, t: &ThreadedComment, thread: Option<ThreadedComment>) -> bool {
        let slide = editor.current_slide;
        match editor.document.execute(vec![Command::Sheet(SheetCommand::SetThreadedComment { slide, row: t.row, column: t.column, thread })]) {
            Ok(_) => true,
            Err(err) => {
                editor.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
                false
            }
        }
    }
}

impl CommentFormat for XlsxComments {
    type Anchor = (u32, u32);

    fn state(editor: &SlideEditor) -> Option<&ThreadedCommentPanel> { editor.sheet_state()?.panels.threaded_comment.as_ref() }
    fn state_mut(editor: &mut SlideEditor) -> Option<&mut ThreadedCommentPanel> { editor.sheet_state_mut()?.panels.threaded_comment.as_mut() }
    fn install(editor: &mut SlideEditor, state: ThreadedCommentPanel) { editor.wb_mut().panels.threaded_comment = Some(state); }
    fn close_others(editor: &mut SlideEditor) {
        let s = editor.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
    }
    fn close(editor: &mut SlideEditor) { editor.wb_mut().panels.threaded_comment = None; }

    fn threads(editor: &SlideEditor) -> Vec<CommentThread> {
        let Some((_, sheet)) = editor.current_sheet() else { return Vec::new() };
        let mut all: Vec<&ThreadedComment> = sheet.threads.iter().collect();
        all.sort_by_key(|t| (t.row, t.column));
        all.into_iter()
            .map(|t| CommentThread {
                id: thread_key(t),
                anchor: Some(ibis_doc_model::cell_name(t.column, t.row)),
                posts: t
                    .posts
                    .iter()
                    .enumerate()
                    .map(|(i, p)| CommentPost { id: i.to_string(), author: p.author.clone(), date: p.date.clone(), text: p.text.clone(), editable: comments::is_own(&p.author) })
                    .collect(),
                resolved: t.done,
            })
            .collect()
    }

    fn active_thread(editor: &SlideEditor) -> Option<String> {
        let (column, row) = editor.current_cell()?;
        editor.current_sheet()?.1.thread_at(row, column).map(thread_key)
    }

    fn new_prompt(_editor: &SlideEditor, anchor: Option<&(u32, u32)>) -> SharedString {
        comments::cell_prompt(&anchor.map(|&(c, r)| ibis_doc_model::cell_name(c, r)).unwrap_or_default())
    }

    /// The field follows the selected cell (`live_target`): it is never fixed when the panel opens.
    fn open_target(_editor: &SlideEditor, _anchor: Option<&(u32, u32)>) -> Option<CommentTarget> { None }

    fn live_target(editor: &SlideEditor) -> Option<CommentTarget> {
        let (column, row) = editor.current_cell()?;
        match editor.current_sheet()?.1.thread_at(row, column) {
            Some(t) => Some(CommentTarget::Reply(thread_key(t))),
            None => Some(CommentTarget::New),
        }
    }

    fn anchor_now(editor: &SlideEditor) -> Option<(u32, u32)> { editor.current_cell() }

    fn add(editor: &mut SlideEditor, &(column, row): &(u32, u32), text: &str, _cx: &mut Context<SlideEditor>) -> bool {
        let who = comments::author_and_date();
        let date = format!("{}.00", who.date.trim_end_matches('Z'));
        let post = ThreadPost { author: who.author, date, text: text.to_string() };
        let thread = ThreadedComment { id: comments::new_guid(), row, column, posts: vec![post], done: false };
        Self::set(editor, &thread.clone(), Some(thread))
    }

    fn reply(editor: &mut SlideEditor, key: &str, text: &str, _cx: &mut Context<SlideEditor>) -> bool {
        let Some(mut t) = Self::find(editor, key) else { return false };
        let who = comments::author_and_date();
        t.posts.push(ThreadPost { author: who.author, date: format!("{}.00", who.date.trim_end_matches('Z')), text: text.to_string() });
        Self::set(editor, &t.clone(), Some(t))
    }

    fn edit(editor: &mut SlideEditor, key: &str, post: &str, text: &str, _cx: &mut Context<SlideEditor>) -> bool {
        let Some(mut t) = Self::find(editor, key) else { return false };
        let Some(p) = post.parse::<usize>().ok().and_then(|i| t.posts.get_mut(i)) else { return false };
        p.text = text.to_string();
        Self::set(editor, &t.clone(), Some(t))
    }

    fn set_resolved(editor: &mut SlideEditor, key: &str, resolved: bool, _cx: &mut Context<SlideEditor>) {
        if let Some(mut t) = Self::find(editor, key) {
            t.done = resolved;
            Self::set(editor, &t.clone(), Some(t));
        }
    }

    fn delete(editor: &mut SlideEditor, key: &str, _cx: &mut Context<SlideEditor>) {
        if let Some(t) = Self::find(editor, key) {
            Self::set(editor, &t, None);
        }
    }

    fn goto(editor: &mut SlideEditor, key: &str, _cx: &mut Context<SlideEditor>) {
        if let Some(t) = Self::find(editor, key) {
            editor.set_cell(t.column, t.row);
            editor.scroll_range_into_view(true);
        }
    }
}

#[ibis_office_macros::editor_ext(ThreadedComments)]
impl SlideEditor {
    /// Open the comments' panel on the selected cell (the cell's thread, or the field for a new one).
    pub fn open_threaded_comment_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(cell) = self.current_cell() else { return };
        comments::open::<XlsxComments>(self, Some(cell), window, cx);
    }

    /// Submit a comment (a new thread if the selected cell has none, else an answer).
    pub fn send_threaded_comment(&mut self, body: &str, window: &mut Window, cx: &mut Context<Self>) {
        comments::send::<XlsxComments>(self, body, window, cx);
    }

    /// Toggle resolved/reopened state of the threaded comment of a cell.
    pub fn resolve_threaded_comment(&mut self, row: u32, column: u32, done: bool, cx: &mut Context<Self>) {
        let Some(key) = self.current_sheet().and_then(|(_, s)| s.thread_at(row, column).map(thread_key)) else { return };
        XlsxComments::set_resolved(self, &key, done, cx);
        cx.notify();
    }

    /// Delete a cell's threaded comment (the menu's コメントの削除: the panel closes with it).
    pub(crate) fn delete_threaded_comment(&mut self, row: u32, column: u32, cx: &mut Context<Self>) {
        if let Some(key) = self.current_sheet().and_then(|(_, s)| s.thread_at(row, column).map(thread_key)) {
            XlsxComments::delete(self, &key, cx);
        }
        if XlsxComments::state(self).is_some_and(|p| p.anchor == Some((column, row))) {
            XlsxComments::close(self);
        }
        cx.notify();
    }

    /// Delete threaded comments and notes in selected ranges.
    pub(crate) fn delete_threaded_comments_and_notes(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let mut commands: Vec<Command> = Vec::new();
        for c in &sheet.comments {
            if ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)) {
                commands.push(Command::Sheet(SheetCommand::SetComment { slide, row: c.row, column: c.column, comment: None }));
            }
        }
        for t in &sheet.threads {
            if ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&t.row) && (r.first_column..=r.last_column).contains(&t.column)) {
                commands.push(Command::Sheet(SheetCommand::SetThreadedComment { slide, row: t.row, column: t.column, thread: None }));
            }
        }
        if commands.is_empty() {
            return;
        }
        if let Err(err) = self.document.execute(commands) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
        }
        if let Some((c, r)) = self.wb().panels.threaded_comment.as_ref().and_then(|p| p.anchor) {
            if ranges.iter().any(|rng| (rng.first_row..=rng.last_row).contains(&r) && (rng.first_column..=rng.last_column).contains(&c)) {
                self.wb_mut().panels.threaded_comment = None;
            }
        }
        cx.notify();
    }

    /// The sidebar's comments panel of the sheet.
    pub fn threaded_comment_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        comments::panel::<XlsxComments>(self, cx)
    }
}
