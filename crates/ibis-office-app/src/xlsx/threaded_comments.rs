// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメントスレッド (Threaded Comments): 挿入 > コメント (⌥⌘M) opens the
//! sidebar's panel with the selected cell's thread to write, reply, resolve
//! and delete.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, SheetCommand, ThreadPost, ThreadedComment};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use super::comment_card::post_time;

pub(crate) struct ThreadedCommentPanel {
    /// The cell (column, row).
    pub(crate) cell: (u32, u32),
    pub(crate) input: Entity<InputState>,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    _subscription: Subscription,
}

/// A new GUID for a threaded comment, from clock and counter.
fn new_guid() -> String {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seed = (crate::clock::unix_millis() as u64) ^ COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    let mix = |mut x: u64| {
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^ (x >> 33)
    };
    let (a, b) = (mix(seed), mix(seed ^ 0x5555_5555_5555_5555));
    format!("{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}", a >> 32, (a >> 16) & 0xffff, a & 0xfff, 0x8000 | (b >> 48) & 0x3fff, b & 0xffff_ffff_ffff)
}

fn current_datetime() -> String {
    let date = crate::docx::comments::comment_author().date;
    let trimmed = date.trim_end_matches('Z');
    format!("{trimmed}.00")
}

impl SlideEditor {
    /// Open the threaded comment of the selected cell.
    pub(crate) fn open_threaded_comment_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let (Some((column, row)), Some((_, sheet))) = (self.current_cell(), self.current_sheet()) else { return };
        let has_thread = sheet.thread_at(row, column).is_some();
        let placeholder = if has_thread {
            crate::shell::i18n::text("_xlsx.type-a-reply-and-press-enter")
        } else {
            crate::shell::i18n::text("_shell.type-a-comment-and-press-enter")
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe_in(&input, window, |this, input, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let text = input.read(cx).value().to_string();
                this.send_threaded_comment(&text, window, cx);
            }
        });
        input.update(cx, |i, cx| i.focus(window, cx));
        let panel = ThreadedCommentPanel {
            cell: (column, row),
            input,
            ui: Default::default(),
            _subscription: subscription,
        };
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.threaded_comment = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Submit a comment (new thread if cell has none, or reply to existing).
    pub(crate) fn send_threaded_comment(&mut self, body: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = body.trim().to_string();
        if text.is_empty() {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.threaded_comment.as_ref()) else { return };
        let (column, row) = p.cell;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let slide = self.current_slide;
        let author = crate::shell::user_name::user_name().to_string();
        let date = current_datetime();
        let existing = sheet.thread_at(row, column).cloned();

        let thread = match existing {
            Some(mut t) => {
                t.posts.push(ThreadPost { author, date, text });
                t
            }
            None => ThreadedComment {
                id: new_guid(),
                row,
                column,
                posts: vec![ThreadPost { author, date, text }],
                done: false,
            },
        };

        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetThreadedComment {
            slide,
            row,
            column,
            thread: Some(thread),
        })]) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
        }
        if let Some(panel) = self.sheet_state().and_then(|s| s.panels.threaded_comment.as_ref()) {
            panel.input.update(cx, |i, cx| {
                i.set_value("", window, cx);
                i.set_placeholder(crate::shell::i18n::text("_xlsx.type-a-reply-and-press-enter"), window, cx);
            });
        }
        cx.notify();
    }

    /// Toggle resolved/reopened state of a threaded comment.
    pub(crate) fn resolve_threaded_comment(&mut self, row: u32, column: u32, done: bool, cx: &mut Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(mut thread) = sheet.thread_at(row, column).cloned() else { return };
        thread.done = done;
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetThreadedComment {
            slide,
            row,
            column,
            thread: Some(thread),
        })]) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
        }
        cx.notify();
    }

    /// Delete a cell's threaded comment.
    pub(crate) fn delete_threaded_comment(&mut self, row: u32, column: u32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetThreadedComment {
            slide,
            row,
            column,
            thread: None,
        })]) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
        }
        if self.wb().panels.threaded_comment.as_ref().is_some_and(|p| p.cell == (column, row)) {
            self.wb_mut().panels.threaded_comment = None;
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
        if let Some(p) = self.wb().panels.threaded_comment.as_ref() {
            let (c, r) = p.cell;
            if ranges.iter().any(|rng| (rng.first_row..=rng.last_row).contains(&r) && (rng.first_column..=rng.last_column).contains(&c)) {
                self.wb_mut().panels.threaded_comment = None;
            }
        }
        if let Some(p) = self.wb().panels.note.as_ref() {
            let (c, r) = p.cell;
            if ranges.iter().any(|rng| (rng.first_row..=rng.last_row).contains(&r) && (rng.first_column..=rng.last_column).contains(&c)) {
                self.wb_mut().panels.note = None;
            }
        }
        cx.notify();
    }

    /// The sidebar panel for the selected cell's threaded comment.
    pub(crate) fn threaded_comment_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.threaded_comment.as_ref()?;
        let (column, row) = p.cell;
        let input = p.input.clone();
        let (_, sheet) = self.current_sheet()?;
        let thread = sheet.thread_at(row, column).cloned();
        let theme = cx.theme().clone();

        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };

        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        body = body.child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(crate::shell::i18n::format("comment.cell", &[("cell", &ibis_doc_model::cell_name(column, row))])),
        );

        if let Some(t) = &thread {
            let done = t.done;
            let mut thread_card = v_flex()
                .gap_2()
                .p(px(8.))
                .rounded(px(6.))
                .border_1()
                .border_color(theme.border)
                .bg(if done { theme.muted } else { theme.background });

            for (i, post) in t.posts.iter().enumerate() {
                thread_card = thread_card.child(
                    v_flex()
                        .gap(px(2.))
                        .when(i > 0, |d| d.pl_3().border_t_1().border_color(theme.border).pt_1())
                        .child(
                            h_flex()
                                .justify_between()
                                .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child(post.author.clone()))
                                .child(div().text_sm().text_color(theme.muted_foreground).child(post_time(&post.date))),
                        )
                        .child(div().text_sm().child(post.text.clone())),
                );
            }

            if done {
                thread_card = thread_card.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(crate::shell::i18n::text("_shell.resolved")),
                );
            }

            let act_row = h_flex()
                .gap_1()
                .child(
                    mark(
                        "resolve",
                        crate::shell::chrome::action_button("comment-resolve")
                            .small()

                            .label(crate::shell::i18n::text(if done { "pptx.comments.reopen" } else { "_shell.resolve" }))
                            .on_click(cx.listener(move |this, _, _, cx| this.resolve_threaded_comment(row, column, !done, cx)))
                            .into_any_element(),
                    ),
                )
                .child(
                    mark(
                        "delete",
                        crate::shell::chrome::destructive_button("comment-delete",cx)
                            .small()
                            .label(crate::shell::i18n::text("_shell.delete"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.delete_threaded_comment(row, column, cx);
                                this.focus_handle.focus(window, cx);
                            }))
                            .into_any_element(),
                    ),
                );

            thread_card = thread_card.child(act_row);
            body = body.child(thread_card);
        }

        // Input field and send button
        let input_area = v_flex()
            .gap_1()
            .child(mark("input", crate::shell::chrome::text_field(&input).full_width().into_any_element()))
            .child(
                h_flex()
                    .justify_end()
                    .child(
                        mark(
                            "send",
                            crate::shell::chrome::primary_button("comment-send")
                                .small()
                                .label(crate::shell::i18n::text(if thread.is_some() { "_shell.reply" } else { "_xlsx.post" }))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let text = input.read(cx).value().to_string();
                                    this.send_threaded_comment(&text, window, cx);
                                }))
                                .into_any_element(),
                        ),
                    ),
            );

        body = body.child(input_area);

        Some(SidebarPanel {
            title: text("_shell.comment"),
            body: body.into_any_element(),
        })
    }
}
