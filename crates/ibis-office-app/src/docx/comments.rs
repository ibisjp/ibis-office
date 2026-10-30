// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメント (X5): the sidebar panel listing the comments, where a comment
//! is put on the selection (挿入 > コメント, ⌥⌘M), answered, resolved and
//! removed.

use gpui::{ElementId, FontWeight, MouseButton};
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::flow_command::{self, CommentAuthor};
use ibis_doc_model::{Block, FlowDocument, FlowPos};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// What the panel's field writes: a new comment on a range, or an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentTarget {
    New { from: FlowPos, to: FlowPos },
    Reply(u32),
}

/// The panel's field and what it is for.
pub(crate) struct CommentState {
    pub(crate) input: Entity<TextareaState>,
    pub(crate) target: Option<CommentTarget>,
    pub(crate) active_comment_id: Option<u32>,
    _subscription: Subscription,
}

/// Who writes (the user's name, shell::user_name) and now, in UTC.
pub(crate) fn comment_author() -> CommentAuthor {
    // The same name as the other formats' comments (custom override,
    // else the OS's full name, else the account name).
    let author = crate::shell::user_name::user_name();
    let initials = crate::shell::user_name::user_initials();
    CommentAuthor { author, initials, date: now_utc() }
}

/// The time now as `w:date` writes it (`2026-10-01T09:00:00Z`).
fn now_utc() -> String {
    // To the minute, as Word writes `w:date` (`…:00Z`): the keystrokes of one typing are in one revision, not a
    // new one each time the second changes.
    let secs = (crate::clock::unix_millis() / 60_000 * 60) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// A comment's text.
fn comment_text(blocks: &[Block]) -> String {
    blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect::<Vec<_>>().join("\n")
}
/// Format UTC ISO date string ("2026-10-17T01:45:00Z") according to locale ("yyyy/MM/dd HH:mm" or "MM/dd/yyyy HH:mm").
fn format_comment_date(iso_date: &str) -> String {
    if iso_date.len() >= 16 {
        let y = &iso_date[0..4];
        let m = &iso_date[5..7];
        let d = &iso_date[8..10];
        let time = &iso_date[11..16]; // HH:mm
        if crate::shell::i18n::language() == crate::shell::i18n::Language::Japanese {
            format!("{y}/{m}/{d} {time}")
        } else {
            format!("{m}/{d}/{y} {time}")
        }
    } else {
        iso_date.to_string()
    }
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

impl SlideEditor {
    /// 挿入 > コメント: the panel, its field for a comment on the selection.
    pub(crate) fn open_comments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let target = self.ordered_selection().filter(|(a, b)| a != b).map(|(from, to)| CommentTarget::New { from, to });
        if self.doc_state().is_some_and(|s| s.comments.is_none()) {
            let input = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(1, 8)
                    .placeholder(crate::shell::i18n::text("_shell.type-a-comment-and-press-enter"))
            });
            let subscription = cx.subscribe_in(&input, window, |this, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, .. } = event {
                    if *secondary {
                        let text = input.read(cx).value().to_string();
                        this.send_comment(&text, window, cx);
                    }
                }
            });
            if let Some(state) = self.doc_state_mut() {
                state.comments = Some(CommentState { input, target: None, active_comment_id: None, _subscription: subscription });
            }
        }
        let active_comment_id = self.doc_state().and_then(|s| s.caret).and_then(|c| {
            let doc = self.flow()?;
            let id = flow_command::comment_at_pos(doc, c)?;
            Some(doc.comments.iter().find(|cm| cm.id == id).and_then(|cm| cm.parent).unwrap_or(id))
        });
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            if let Some(c) = state.comments.as_mut() {
                c.target = target;
                if active_comment_id.is_some() {
                    c.active_comment_id = active_comment_id;
                }
            }
        }
        if target.is_some()
            && let Some(input) = self.doc_state().and_then(|s| s.comments.as_ref()).map(|c| c.input.clone())
        {
            input.update(cx, |i, cx| i.focus(window, cx));
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Jump to comment `id`: select its range, scroll to it, and mark it active in the panel.
    pub(crate) fn focus_comment(&mut self, id: u32, cx: &mut Context<Self>) {
        if let Some(c) = self.doc_state_mut().and_then(|s| s.comments.as_mut()) {
            c.active_comment_id = Some(id);
        }
        if let Some(doc) = self.flow() {
            if let Some((from, to)) = flow_command::comment_range(doc, id) {
                self.jump_to_range(from, to, cx);
            }
        }
        cx.notify();
    }

    /// The field's text written as the comment or answer it is for.
    pub(crate) fn send_comment(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.trim();
        let Some(target) = self.doc_state().and_then(|s| s.comments.as_ref()).and_then(|c| c.target) else { return };
        if text.is_empty() {
            return;
        }
        let who = comment_author();
        let Some(doc) = self.flow() else { return };
        let commands = match target {
            CommentTarget::New { from, to } => flow_command::add_comment(doc, from, to, &who, text),
            CommentTarget::Reply(id) => flow_command::reply_comment(doc, id, &who, text),
        };
        let Some(commands) = commands else { return };
        let keep = self.text_selection();
        if self.run_flow(commands, cx)
            && let Some(state) = self.doc_state_mut()
        {
            if let Some(c) = state.comments.as_mut() {
                c.target = None;
            }
            // The marks moved the selection's end along.
            if let Some((_, c)) = keep {
                state.anchor = None;
                state.caret = Some(c);
            }
        }
        if let Some(input) = self.doc_state().and_then(|s| s.comments.as_ref()).map(|c| c.input.clone()) {
            input.update(cx, |i, cx| i.set_value("", window, cx));
        }
        cx.notify();
    }

    /// 返信: the field for an answer to comment `id`.
    pub(crate) fn reply_to(&mut self, id: u32, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.doc_state_mut().and_then(|s| s.comments.as_mut()) {
            c.target = Some(CommentTarget::Reply(id));
        }
        if let Some(input) = self.doc_state().and_then(|s| s.comments.as_ref()).map(|c| c.input.clone()) {
            input.update(cx, |i, cx| {
                i.set_value("", window, cx);
                i.focus(window, cx);
            });
        }
        cx.notify();
    }

    /// 返信欄や新規コメント欄を閉じる（キャンセル）。
    pub(crate) fn cancel_comment_input(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = self.doc_state_mut().and_then(|s| s.comments.as_mut()) {
            c.target = None;
        }
        cx.notify();
    }

    /// 解決 (or 再開): the comment and its answers resolved or opened again.
    pub(crate) fn resolve(&mut self, id: u32, done: bool, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let command = flow_command::resolve_comment(doc, id, done);
        self.run_flow(vec![command], cx);
    }

    /// 削除確認ダイアログを開く。
    pub(crate) fn confirm_delete_comment(&mut self, id: u32, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.comment_delete_confirm = Some(id);
        }
        cx.notify();
    }

    /// 削除: the comment, its answers and their marks.
    pub(crate) fn delete_comment(&mut self, id: u32, cx: &mut Context<Self>) {
        let Some(command) = self.flow().and_then(|d| flow_command::delete_comment(d, id)) else { return };
        self.run_flow(vec![command], cx);
    }

    pub(crate) fn comments_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.comments.as_ref()?;
        let (input, target) = (state.input.clone(), state.target);
        let doc = self.flow()?.clone();
        let active_comment_id = state.active_comment_id.or_else(|| {
            let caret = self.doc_state().and_then(|s| s.caret)?;
            let id = flow_command::comment_at_pos(&doc, caret)?;
            Some(doc.comments.iter().find(|c| c.id == id).and_then(|c| c.parent).unwrap_or(id))
        });
        let mut body = v_flex().gap_2().px(px(14.)).py(px(8.)).p_0();

        let post_tooltip = if cfg!(target_os = "macos") {
            format!("{} (⌘Enter)", crate::shell::i18n::text("docx.comments.post"))
        } else {
            format!("{} (Ctrl+Enter)", crate::shell::i18n::text("docx.comments.post"))
        };

        // 新規スレッドの開始時：パネル上部に TextArea と送信ボタンを表示
        if let Some(CommentTarget::New { .. }) = target {
            let header = h_flex()
                .justify_between()
                .items_center()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("docx.comments.comment-selection")))
                .child(
                    div()
                        .id("docx-comment-cancel-new")
                        .px(px(4.))
                        .py(px(2.))
                        .cursor_pointer()
                        .text_sm()
                        .text_color(rgb(0x888888))
                        .hover(|h| h.text_color(rgb(0x000000)))
                        .child("✕")
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_comment_input(cx))),
                );
            let input_send = input.clone();
            let input_box = v_flex()
                .gap_2()
                .p(px(8.))
                .rounded_md()
                .bg(rgb(0xf8f9fa))
                .border_1()
                .border_color(rgb(0xe0e0e0))
                .child(header)
                .child(Textarea::new(&input))
                .child(
                    h_flex()
                        .justify_end()
                        .child(
                            crate::shell::chrome::primary_button("comment-send")
                                .small()
                                .icon(crate::shell::icons::Material("send"))
                                .tooltip(post_tooltip.clone())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let text = input_send.read(cx).value().to_string();
                                    this.send_comment(&text, window, cx);
                                })),
                        ),
                );
            body = body.child(input_box);
        } else if target.is_none() {
            body = body.child(div().text_sm().text_color(rgb(0x888888)).child(crate::shell::i18n::text("docx.comments.select-text-then-insert-comment")));
        }

        // コメント一覧をアンカーの Y 昇順にソート
        let mut parent_comments: Vec<_> = doc.comments.iter().filter(|c| c.parent.is_none()).collect();
        parent_comments.sort_by(|a, b| {
            let pos_a = comment_anchor_position(self, &doc, a.id);
            let pos_b = comment_anchor_position(self, &doc, b.id);
            pos_a.0.cmp(&pos_b.0).then_with(|| pos_a.1.total_cmp(&pos_b.1)).then_with(|| a.id.cmp(&b.id))
        });

        for c in parent_comments {
            let id = c.id;
            let done = c.done;
            let is_active = active_comment_id == Some(id);
            let is_replying_to_this = matches!(target, Some(CommentTarget::Reply(reply_id)) if reply_id == id);

            let resolved_tag = if done { format!(" [{}]", crate::shell::i18n::text("_shell.resolved")) } else { "".into() };
            let date_str = format_comment_date(&c.date);

            let mut card = v_flex()
                .id(ElementId::NamedInteger("comment-card".into(), id as u64))
                .gap_1()
                .p(px(8.))
                .rounded(px(8.))
                .cursor_pointer()
                .bg(if is_active {
                    if done { rgb(0xe8eaed) } else { rgb(0xfeefc3) }
                } else if done {
                    rgb(0xf1f3f4)
                } else {
                    rgb(0xfef7e0)
                })
                .border(px(if is_active { 1.5 } else { 1.0 }))
                .border_color(if is_active {
                    rgb(0xf9ab00)
                } else {
                    gpui::transparent_black().into()
                })
                .hover(|s| {
                    if !is_active {
                        s.bg(if done { rgb(0xe8eaed) } else { rgb(0xfdefba) }).border_color(rgb(0xf9ab00))
                    } else {
                        s
                    }
                })
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                    this.focus_comment(id, cx);
                }))
                .child(div().text_sm().text_color(rgb(0x5f6368)).child(format!("{}　{}{}", c.author, date_str, resolved_tag))) // i18n-ok（文書の中の名前・全角の区切り・記号）
                .child(div().text_sm().child(comment_text(&c.blocks)));
            if is_active {
                card = card.border_l(px(3.5));
            }

            // 2つ目以降のコメント（返信）：＜氏名＞＜日時＞をグレーで描画して、次の行にコメント本文
            for r in doc.comments.iter().filter(|r| r.parent == Some(id)) {
                let r_date = format_comment_date(&r.date);
                let r_header = div().text_sm().text_color(rgb(0x5f6368)).child(format!("{}　{}", r.author, r_date)); // i18n-ok（文書の中の名前・全角の区切り・記号）
                let r_body = div().text_sm().child(comment_text(&r.blocks));
                card = card.child(v_flex().pl(px(8.)).pt(px(2.)).gap_0p5().child(r_header).child(r_body));
            }

            // 青字のアクションボタン（返信・解決・削除）
            let reply_btn = div()
                .id(ElementId::NamedInteger("comment-reply".into(), id as u64))
                .px(px(4.))
                .py(px(2.))
                .rounded_sm()
                .cursor_pointer()
                .text_sm()
                .text_color(rgb(0x1a73e8))
                .hover(|h| h.bg(rgb(0xe8f0fe)).underline())
                .child(crate::shell::i18n::text("_shell.reply"))
                .on_click(cx.listener(move |this, _, window, cx| this.reply_to(id, window, cx)));

            let done_btn = div()
                .id(ElementId::NamedInteger("comment-done".into(), id as u64))
                .px(px(4.))
                .py(px(2.))
                .rounded_sm()
                .cursor_pointer()
                .text_sm()
                .text_color(rgb(0x1a73e8))
                .hover(|h| h.bg(rgb(0xe8f0fe)).underline())
                .child(crate::shell::i18n::text(if done { "pptx.comments.reopen" } else { "_shell.resolve" }))
                .on_click(cx.listener(move |this, _, _, cx| this.resolve(id, !done, cx)));

            let delete_btn = crate::shell::chrome::destructive_button(("comment-delete", id),cx)
                .small().label(crate::shell::i18n::text("_shell.delete"))
                .on_click(cx.listener(move |this, _, _, cx| this.confirm_delete_comment(id, cx)));
            card = card.child(
                h_flex()
                    .gap_2()
                    .pt(px(2.))
                    .child(reply_btn)
                    .child(done_btn)
                    .child(delete_btn),
            );

            // 返信ボタン押下時：吹き出しカードの中の返信ボタンの下に TextArea を配置
            if is_replying_to_this {
                let reply_header = h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(rgb(0x444444)).child(crate::shell::i18n::text("_shell.reply")))
                    .child(
                        div()
                            .id("docx-comment-cancel-reply")
                            .px(px(4.))
                            .py(px(2.))
                            .cursor_pointer()
                            .text_sm()
                            .text_color(rgb(0x888888))
                            .hover(|h| h.text_color(rgb(0x000000)))
                            .child("✕")
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_comment_input(cx))),
                    );
                let input_reply_send = input.clone();
                let reply_box = v_flex()
                    .gap_1()
                    .mt(px(4.))
                    .p(px(6.))
                    .rounded_md()
                    .bg(rgb(0xffffff))
                    .border_1()
                    .border_color(rgb(0xe0e0e0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(reply_header)
                    .child(Textarea::new(&input))
                    .child(
                        h_flex()
                            .justify_end()
                            .child(
                                crate::shell::chrome::primary_button(("comment-reply-send", id))
                                    .small()
                                    .icon(crate::shell::icons::Material("send"))
                                    .tooltip(post_tooltip.clone())
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let text = input_reply_send.read(cx).value().to_string();
                                        this.send_comment(&text, window, cx);
                                    })),
                            ),
                    );
                card = card.child(reply_box);
            }

            body = body.child(card);
        }
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.comment"), body: body.into_any_element() })
    }
}
