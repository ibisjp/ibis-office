// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメント: the one component of the comments' panel for the documents, the workbooks and the
//! presentations (doc/rule/implimentationRule.md; tasks/doc/reports/2026-11-02-comments-common-component.md).
//!
//! A format (docx `comments.xml`, pptx modern comments, xlsx threaded comments) implements
//! [`CommentFormat`]: it lists its threads as plain data ([`CommentThread`]) and does the
//! operations (add, reply, edit, resolve, delete, go to the place in the body) on its own model.
//! This part builds the panel (the field, the cards, the buttons), the question before a delete
//! and the state of the field; no format draws a comment card of its own.

use std::rc::Rc;

use gpui::{ElementId, FontWeight, MouseButton};
use crate::chrome::{Dropdown, DropdownItem, Handler};
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::chrome::{ActionButtonKind, ActionDefinition, SidebarPanel, action_bar};
use crate::editor::SlideEditor;
use crate::i18n::{format as i18n_format, text};
use crate::palette::rgb;

/// One post of a thread: the first is the comment, the others its answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentPost {
    /// The post's own id (for `edit`); the format knows what it names.
    pub id: String,
    pub author: String,
    /// As the file writes it (`2026-10-07T09:30:00.00`, `2026-10-17T01:45:00Z`); shown by [`comment_date_label`].
    pub date: String,
    pub text: String,
    /// Written by the user of this app (may be edited).
    pub editable: bool,
}

/// A comment with its answers, as the panel shows it, whatever the format keeps it as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentThread {
    pub id: String,
    /// What it is on (a cell name, a shape), if the panel says it.
    pub anchor: Option<String>,
    pub posts: Vec<CommentPost>,
    pub resolved: bool,
}

/// What the panel's field writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommentTarget {
    /// A new comment on the state's anchor.
    New,
    /// An answer to a thread.
    Reply(String),
    /// A new text for a post of a thread.
    Edit { thread: String, post: String },
}

/// The panel's field and what it is for; one per open panel, kept in the format's state.
pub struct CommentPanelState<A> {
    pub input: Entity<TextareaState>,
    pub target: Option<CommentTarget>,
    /// Where a new comment would go (a range, a shape, a cell), if there is a place.
    pub anchor: Option<A>,
    /// The thread the user chose in the panel.
    pub active: Option<String>,
    pub ui: CommentBounds,
    /// The place of a new comment the panel was last drawn for: a new place puts the panel's scroll back to the top, where the
    /// field for a new comment is.
    pub(crate) seen_anchor: Option<A>,
    /// The thread that was the active one when the panel was last drawn: another one (a cell, a place in the body was
    /// chosen) brings its card to the top of the panel's view (`pending_scroll`).
    pub(crate) seen_active: Option<String>,
    /// The index of the card to put at the top of the view once it has been drawn (its place is known only then).
    pub pending_scroll: Rc<std::cell::Cell<Option<usize>>>,
    /// Whether the field for an answer is drawn in the pinned part under the scrolling list (its place at the end of the
    /// thread would be out of sight even with the list at the top).
    pub pinned_reply: bool,
    /// The height of the field for an answer when it was last drawn (inline or pinned).
    pub(crate) field_height: f32,
    _subscription: Subscription,
}

/// Where the panel's parts were drawn, by name (`card-0`, `reply-0`, `done-0`, `delete-0`, `edit-0-0`, `send`, `cancel`,
/// `delete-ok`, `delete-cancel`): for the scenario tests to click what the user clicks.
pub(crate) type CommentBounds = Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>;

/// A delete waiting for the answer of the question (the editor shows it as a message box).
pub struct PendingDelete {
    pub ui: CommentBounds,
    pub confirm: Rc<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)>,
}

/// What a format gives the comments' panel. Everything takes the editor: the format's state and
/// model are reached through it.
pub trait CommentFormat: 'static {
    /// Where a new comment goes: a text range, a shape, a cell.
    type Anchor: Clone + PartialEq + 'static;

    fn state(editor: &SlideEditor) -> Option<&CommentPanelState<Self::Anchor>>;
    fn state_mut(editor: &mut SlideEditor) -> Option<&mut CommentPanelState<Self::Anchor>>;
    /// Keep the state as the format's open panel (and close the panels it replaces).
    fn install(editor: &mut SlideEditor, state: CommentPanelState<Self::Anchor>);
    /// Close the panels that cannot show with this one.
    fn close_others(editor: &mut SlideEditor);
    /// The panel closed.
    fn close(editor: &mut SlideEditor);

    /// The threads in the order to list them.
    fn threads(editor: &SlideEditor) -> Vec<CommentThread>;
    /// The thread to mark: the user's choice, else the one at the caret, cell or selection.
    fn active_thread(editor: &SlideEditor) -> Option<String>;
    /// The words above the field for a new comment on `anchor`.
    fn new_prompt(editor: &SlideEditor, anchor: Option<&Self::Anchor>) -> SharedString;
    /// The words in the place of the field while it is not for anything (None: nothing).
    fn idle_hint() -> Option<&'static str> { None }
    /// What the field is for when the panel opens at `anchor`.
    fn open_target(_editor: &SlideEditor, anchor: Option<&Self::Anchor>) -> Option<CommentTarget> {
        anchor.map(|_| CommentTarget::New)
    }
    /// What the field is for after a post or a cancel.
    fn rest_target(_editor: &SlideEditor) -> Option<CommentTarget> { None }
    /// What the field is for while the state says nothing (a format whose field follows the selection, as the
    /// cell of a workbook does).
    fn live_target(_editor: &SlideEditor) -> Option<CommentTarget> { None }
    /// Where a new comment goes now, when that follows the selection (it wins over the state's anchor).
    fn anchor_now(_editor: &SlideEditor) -> Option<Self::Anchor> { None }

    fn add(editor: &mut SlideEditor, anchor: &Self::Anchor, text: &str, cx: &mut Context<SlideEditor>) -> bool;
    fn reply(editor: &mut SlideEditor, thread: &str, text: &str, cx: &mut Context<SlideEditor>) -> bool;
    fn edit(editor: &mut SlideEditor, thread: &str, post: &str, text: &str, cx: &mut Context<SlideEditor>) -> bool;
    fn set_resolved(editor: &mut SlideEditor, thread: &str, resolved: bool, cx: &mut Context<SlideEditor>);
    fn delete(editor: &mut SlideEditor, thread: &str, cx: &mut Context<SlideEditor>);
    /// Go to the thread's place in the body (select the range, the shape, the cell).
    fn goto(editor: &mut SlideEditor, thread: &str, cx: &mut Context<SlideEditor>);
}

// ----- Words and times -----------------------------------------------------------------------

/// `2026-10-17T01:45:00Z` / `2026-10-07T09:30:00.00` as `2026/10/17 01:45` (ja) or `10/17/2026 01:45`
/// (en); a date alone without the time; anything else as it is.
pub fn comment_date_label(raw: &str) -> String {
    let raw = raw.trim().trim_end_matches('Z');
    let (date, time) = raw.split_once('T').unwrap_or((raw, ""));
    let d: Vec<&str> = date.split('-').collect();
    if d.len() != 3 || d.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return raw.to_string();
    }
    let (y, m, day) = (d[0], d[1], d[2]);
    let day_text = match crate::i18n::language() {
        crate::i18n::Language::English => format!("{m}/{day}/{y}"),
        crate::i18n::Language::Japanese => format!("{y}/{m}/{day}"),
    };
    let mut hm = time.split(':');
    match (hm.next().filter(|h| !h.is_empty()), hm.next()) {
        (Some(h), Some(min)) => format!("{day_text} {h:0>2}:{}", &min[..min.len().min(2)]),
        _ => day_text,
    }
}

/// A new GUID (`{xxxxxxxx-xxxx-4xxx-xxxx-xxxxxxxxxxxx}`) for a comment or an answer, from the clock and a count.
pub fn new_guid() -> String {
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

/// Who writes and when, as every format's new post takes them: the user's name and initials, and the time in UTC (to the
/// minute, `2026-10-01T09:00:00Z`).
pub fn author_and_date() -> ibis_doc_model::flow_command::CommentAuthor {
    comment_author()
}

/// Whether a post by `author` may be edited here: the user's own.
pub fn is_own(author: &str) -> bool {
    author == crate::user_name::user_name()
}

// ----- The panel's life ----------------------------------------------------------------------

impl<A> CommentPanelState<A> {
    fn make<F: CommentFormat<Anchor = A>>(window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 8).placeholder(text("shell.comments.placeholder")));
        let subscription = cx.subscribe_in(&input, window, |this, input, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { secondary, .. } = event
                && *secondary
            {
                let body = input.read(cx).value().to_string();
                send::<F>(this, &body, window, cx);
            }
        });
        Self { input, target: None, anchor: None, active: None, ui: Default::default(), seen_anchor: None, seen_active: None, pending_scroll: Default::default(), pinned_reply: false, field_height: 0.0, _subscription: subscription }
    }
}

/// What the field is for: the state's, else the format's live one.
pub(crate) fn target_of<F: CommentFormat>(editor: &SlideEditor) -> Option<CommentTarget> {
    F::state(editor)?.target.clone().or_else(|| F::live_target(editor))
}

/// Where a new comment goes: the format's live place, else the state's.
pub(crate) fn anchor_of<F: CommentFormat>(editor: &SlideEditor) -> Option<F::Anchor> {
    F::anchor_now(editor).or_else(|| F::state(editor).and_then(|s| s.anchor.clone()))
}

/// Open the panel (挿入 > コメント): its field for a new comment on `anchor` (or what the format makes of it).
pub fn open<F: CommentFormat>(editor: &mut SlideEditor, anchor: Option<F::Anchor>, window: &mut Window, cx: &mut Context<SlideEditor>) {
    // The others first: closing them may clear the format's panels with them.
    let had = F::state(editor).is_some();
    F::close_others(editor);
    if !had || F::state(editor).is_none() {
        let state = CommentPanelState::<F::Anchor>::make::<F>(window, cx);
        F::install(editor, state);
    }
    let target = F::open_target(editor, anchor.as_ref());
    if let Some(state) = F::state_mut(editor) {
        state.anchor = anchor;
        state.target = target.clone();
        state.input.update(cx, |i, cx| i.set_value("", window, cx));
    }
    if target_of::<F>(editor).is_some()
        && let Some(input) = F::state(editor).map(|s| s.input.clone())
    {
        input.update(cx, |i, cx| i.focus(window, cx));
    }
    editor.sidebar_open = true;
    cx.notify();
}

/// The field's text written as what it is for (a new comment, an answer, a new text).
pub fn send<F: CommentFormat>(editor: &mut SlideEditor, body: &str, window: &mut Window, cx: &mut Context<SlideEditor>) {
    let body = body.trim().to_string();
    let (target, anchor) = (target_of::<F>(editor), anchor_of::<F>(editor));
    let Some(target) = target else { return };
    if body.is_empty() {
        return;
    }
    let done = match target {
        CommentTarget::New => anchor.is_some_and(|a| F::add(editor, &a, &body, cx)),
        CommentTarget::Reply(thread) => F::reply(editor, &thread, &body, cx),
        CommentTarget::Edit { thread, post } => F::edit(editor, &thread, &post, &body, cx),
    };
    if done {
        let rest = F::rest_target(editor);
        if let Some(state) = F::state_mut(editor) {
            state.target = rest;
            state.input.update(cx, |i, cx| i.set_value("", window, cx));
        }
    }
    cx.notify();
}

/// The field for an answer to `thread`.
pub fn begin_reply<F: CommentFormat>(editor: &mut SlideEditor, thread: &str, window: &mut Window, cx: &mut Context<SlideEditor>) {
    begin::<F>(editor, CommentTarget::Reply(thread.to_string()), "", window, cx);
}

/// The field for a new text of the post `post` of `thread`, with its text in it.
pub(crate) fn begin_edit<F: CommentFormat>(editor: &mut SlideEditor, thread: &str, post: &str, window: &mut Window, cx: &mut Context<SlideEditor>) {
    let current = F::threads(editor).into_iter().find(|t| t.id == thread).and_then(|t| t.posts.into_iter().find(|p| p.id == post)).map(|p| p.text).unwrap_or_default();
    begin::<F>(editor, CommentTarget::Edit { thread: thread.to_string(), post: post.to_string() }, &current, window, cx);
}

fn begin<F: CommentFormat>(editor: &mut SlideEditor, target: CommentTarget, initial: &str, window: &mut Window, cx: &mut Context<SlideEditor>) {
    if F::state(editor).is_none() {
        F::close_others(editor);
        let state = CommentPanelState::<F::Anchor>::make::<F>(window, cx);
        F::install(editor, state);
    }
    if let Some(state) = F::state_mut(editor) {
        state.target = Some(target);
        let input = state.input.clone();
        input.update(cx, |i, cx| {
            i.set_value(initial, window, cx);
            i.focus(window, cx);
        });
    }
    editor.sidebar_open = true;
    cx.notify();
}

/// The field put away (キャンセル).
pub(crate) fn cancel<F: CommentFormat>(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) {
    let rest = F::rest_target(editor);
    if let Some(state) = F::state_mut(editor) {
        state.target = rest;
        state.input.update(cx, |i, cx| i.set_value("", window, cx));
    }
    cx.notify();
}

/// Choose a thread in the panel and go to its place in the body.
pub fn activate<F: CommentFormat>(editor: &mut SlideEditor, thread: &str, cx: &mut Context<SlideEditor>) {
    if let Some(state) = F::state_mut(editor) {
        state.active = Some(thread.to_string());
        // The user chose it here: it is where they are looking, no scrolling to it.
        state.seen_active = Some(thread.to_string());
    }
    F::goto(editor, thread, cx);
    cx.notify();
}

/// Ask before deleting `thread` (the message box of `render_delete_confirm`).
pub fn ask_delete<F: CommentFormat>(editor: &mut SlideEditor, thread: &str, cx: &mut Context<SlideEditor>) {
    let thread = thread.to_string();
    let ui = F::state(editor).map(|s| s.ui.clone()).unwrap_or_default();
    editor.comment_delete = Some(PendingDelete { ui, confirm: Rc::new(move |editor, cx| F::delete(editor, &thread, cx)) });
    cx.notify();
}

impl SlideEditor {
    /// The question before a comment is deleted (modal; Escape or キャンセル: no).
    pub(crate) fn render_delete_confirm(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = self.comment_delete.as_ref().map(|p| p.ui.clone()).unwrap_or_default();
        let (ui_no, ui_yes) = (ui.clone(), ui);
        let no = ActionDefinition::new("comment-delete-cancel", text("shell.comments.cancel"), ActionButtonKind::Normal)
            .on_bounds(move |b, _, _| { ui_no.borrow_mut().insert("delete-cancel".into(), b); })
            .on_click(cx.listener(|this, _, _, cx| {
            this.comment_delete = None;
            cx.notify();
        }));
        let yes = ActionDefinition::new("comment-delete-ok", text("shell.comments.delete"), ActionButtonKind::Destructive)
            .on_bounds(move |b, _, _| { ui_yes.borrow_mut().insert("delete-ok".into(), b); })
            .on_click(cx.listener(|this, _, _, cx| {
            if let Some(pending) = this.comment_delete.take() {
                (pending.confirm)(this, cx);
            }
            cx.notify();
        }));
        crate::message_box::message_box("comment-delete-dialog", text("shell.comments.delete-title"), text("shell.comments.delete-message"), action_bar().action(yes).action(no), &self.msgbox, false, cx)
    }
}

// ----- The panel -----------------------------------------------------------------------------

const MUTED: u32 = 0x5f6368;
/// A thread's light blue (the panel's card, the hover popup's outside); `palette::rgb` gives its dark counterpart.
pub const THREAD_BG: u32 = 0xe3f2fd;
const CARD_BG_ACTIVE: u32 = 0xc9e4f8;
const CARD_BG_RESOLVED: u32 = 0xf1f3f4;
const CARD_BG_RESOLVED_ACTIVE: u32 = 0xe8eaed;
const CARD_ACCENT: u32 = 0x4a9fd8;
/// The rounded rectangle of one post (and of the field): white with this edge.
pub const POST_BORDER: u32 = 0xdadce0;

/// `child` with its drawn bounds kept under `name`.
fn mark(ui: &CommentBounds, name: String, child: AnyElement) -> AnyElement {
    let ui = ui.clone();
    div()
        .relative()
        .child(child)
        .child(canvas(|_, _, _| {}, move |bounds, _, _, _| { ui.borrow_mut().insert(name.clone(), bounds); }).absolute().top_0().left_0().size_full())
        .into_any_element()
}

/// How far the user's own posts (and the fields they write in) sit in from the left, as one's own message in a chat does;
/// the others' start at the edge. The first post of a thread and a reply are not told apart.
/// The space between the cards of the list (`gap_2`).
const CARD_GAP: f32 = 8.;

pub const OWN_INDENT: Pixels = px(10.);

/// The rounded white rectangle one post (its author, time, ⋯ and text) or the field sits in.
fn post_box() -> Div {
    v_flex().w_full().gap_1().p(px(8.)).rounded_lg().bg(rgb(0xffffff)).border_1().border_color(rgb(POST_BORDER))
}

/// The comments' panel of format `F`: the field, the threads as cards, with their buttons.
pub fn panel<F: CommentFormat>(editor: &mut SlideEditor, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    let (input, ui) = { let st = F::state(editor)?; (st.input.clone(), st.ui.clone()) };
    let (target, anchor) = (target_of::<F>(editor), anchor_of::<F>(editor));
    // A new place (another cell was chosen in the body): the field for a new comment is at the top, so show the top.
    if let Some(a) = anchor.clone()
        && let Some(state) = F::state_mut(editor)
        && state.seen_anchor.as_ref() != Some(&a)
    {
        state.seen_anchor = Some(a);
        editor.sidebar_scroll.set_offset(Point::default());
    }
    let threads = F::threads(editor);
    let active = F::active_thread(editor);
    let room = editor.sidebar_layout.get().room;
    let (pending, pinned_reply) = {
        let reply = match &target {
            Some(CommentTarget::Reply(t)) => threads.iter().position(|x| x.id == *t),
            _ => None,
        };
        let state = F::state_mut(editor)?;
        // Another thread became the active one from outside the panel: its card goes to the top of the view.
        if state.seen_active != active {
            state.seen_active = active.clone();
            if let Some(i) = active.as_ref().and_then(|a| threads.iter().position(|t| t.id == *a)) {
                state.pending_scroll.set(Some(i));
            }
        }
        // The field for an answer is pinned when its place at the end of the thread would be out of sight with the list at
        // the top (`reply_cut_off`). Drawn once either way, so it does not flap: the end is measured as if the field were in it.
        let bounds = ui.borrow();
        let field = bounds.get("reply-field").or_else(|| bounds.get("reply-pinned")).map(|b| f32::from(b.size.height));
        if let Some(h) = field {
            state.field_height = h;
        }
        state.pinned_reply = match reply {
            None => false,
            Some(i) => reply_cut_off(&bounds, i, state.pinned_reply, state.field_height, room).unwrap_or(state.pinned_reply),
        };
        (state.pending_scroll.clone(), state.pinned_reply.then_some(reply).flatten())
    };
    let new_prompt = F::new_prompt(editor, anchor.as_ref());
    let owner = cx.weak_entity();
    let mut body = v_flex().gap_2().w_full();

    // The field for a new comment, above the list.
    if matches!(target, Some(CommentTarget::New)) {
        body = body.child(field_box::<F>("new", Some(new_prompt), &input, &ui, &target, cx));
    } else if target.is_none()
        && let Some(hint) = F::idle_hint()
    {
        body = body.child(div().text_sm().text_color(rgb(MUTED)).child(text(hint)));
    }

    if threads.is_empty() && !matches!(target, Some(CommentTarget::New)) {
        body = body.child(div().text_sm().text_color(rgb(MUTED)).child(text("shell.comments.empty")));
    }

    for (index, thread) in threads.iter().enumerate() {
        let id = thread.id.clone();
        let (is_active, done) = (active.as_deref() == Some(id.as_str()), thread.resolved);
        let mut card = v_flex()
            .id(ElementId::NamedInteger("comment-card".into(), index as u64))
            .gap_1()
            .p(px(6.))
            .rounded(px(10.))
            .cursor_pointer()
            .bg(rgb(match (is_active, done) {
                (true, true) => CARD_BG_RESOLVED_ACTIVE,
                (true, false) => CARD_BG_ACTIVE,
                (false, true) => CARD_BG_RESOLVED,
                (false, false) => THREAD_BG,
            }))
            .border(px(if is_active { 1.5 } else { 1.0 }))
            .border_color(if is_active { rgb(CARD_ACCENT) } else { gpui::transparent_black().into() })
            .hover(|s| if is_active { s } else { s.border_color(rgb(CARD_ACCENT)) })
            .on_mouse_down(MouseButton::Left, {
                let id = id.clone();
                cx.listener(move |this, _, _, cx| activate::<F>(this, &id, cx))
            });
        if let Some(anchor) = &thread.anchor {
            card = card.child(div().px(px(2.)).text_xs().text_color(rgb(MUTED)).child(anchor.clone()));
        }
        for (j, post) in thread.posts.iter().enumerate() {
            let editing = matches!(&target, Some(CommentTarget::Edit { thread: t, post: p }) if *t == thread.id && *p == post.id);
            if editing {
                card = card.child(div().w_full().pl(OWN_INDENT).child(field_box::<F>(("edit", index * 1000 + j), Some(text("shell.comments.edit-prompt")), &input, &ui, &target, cx)));
                continue;
            }
            // The name and the time may wrap; the ⋯ keeps its place at the right.
            let mut who = h_flex().flex_1().min_w_0().flex_wrap().gap_x_2().items_center().child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(post.author.clone()));
            let date = comment_date_label(&post.date);
            if !date.is_empty() {
                who = who.child(div().text_sm().text_color(rgb(MUTED)).child(date));
            }
            let mut head = h_flex().w_full().gap_1().items_start().child(who);
            if let Some(menu) = post_menu::<F>(index, j, &thread.id, post, j == 0, done, &ui, &owner) {
                head = head.child(div().flex_none().child(menu));
            }
            let post_el = post_box()
                .child(head)
                .when(j == 0 && done, |d| d.child(div().text_xs().text_color(rgb(MUTED)).child(text("shell.comments.resolved"))))
                .child(div().text_sm().child(post.text.clone()));
            card = card.child(div().w_full().when(is_own(&post.author), |d| d.pl(OWN_INDENT)).child(mark(&ui, format!("post-{index}-{j}"), post_el.into_any_element())));
        }
        let reply_here = matches!(&target, Some(CommentTarget::Reply(t)) if *t == thread.id);
        if reply_here && pinned_reply == Some(index) {
            // In the pinned part, under the list.
        } else if reply_here {
            card = card.child(div().w_full().pl(OWN_INDENT).child(mark(&ui, "reply-field".into(), field_box::<F>(("reply", index), None, &input, &ui, &target, cx))));
        } else {
            // Not the one the field is on: the place to click for an answer (it looks as the field does when it is empty).
            let id_reply = id.clone();
            card = card.child(div().w_full().pl(OWN_INDENT).child(mark(
                &ui,
                format!("reply-{index}"),
                post_box()
                    .id(ElementId::NamedInteger("comment-reply".into(), index as u64))
                    .py(px(5.))
                    .cursor_text()
                    .text_sm()
                    .text_color(rgb(MUTED))
                    .child(text("shell.comments.reply-prompt"))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| begin_reply::<F>(this, &id_reply, window, cx)))
                    .into_any_element(),
            )));
        }
        body = body.child(mark(&ui, format!("card-{index}"), card.into_any_element()));
    }
    let scroll = editor.sidebar_scroll.clone();
    let (ui_scroll, owner_scroll) = (ui.clone(), owner.clone());
    // After the cards are drawn: the card to bring to the top of the view, now that its place is known.
    let scroller = canvas(|_, _, _| {}, move |_, _, _, cx| {
        let Some(index) = pending.get() else { return };
        let bounds = ui_scroll.borrow();
        let (Some(card), view) = (bounds.get(&format!("card-{index}")), scroll.bounds()) else { return };
        pending.set(None);
        if view.size.height <= px(0.) || scroll.max_offset().y <= px(0.) {
            return;
        }
        // Its top where the first card's is with the list at the top: the panel's padding under the view's top.
        let shift = card.top() - (view.top() + px(crate::chrome::FORM_PADDING));
        let offset = scroll.offset();
        let y = (offset.y - shift).clamp(-scroll.max_offset().y, px(0.));
        if y != offset.y {
            scroll.set_offset(point(offset.x, y));
            let owner = owner_scroll.clone();
            cx.defer(move |cx| { owner.update(cx, |_, cx| cx.notify()).ok(); });
        }
    })
    .absolute()
    .size_0();
    let list = v_flex().w_full().child(mark(&ui, "body".into(), body.into_any_element())).child(scroller);
    let mut panel = SidebarPanel { title: text("shell.comments.title"), body: list.into_any_element() };
    if let Some(index) = pinned_reply {
        let field = div().w_full().pl(OWN_INDENT).child(field_box::<F>(("reply", index), None, &input, &ui, &target, cx));
        panel = panel.pinned(mark(&ui, "reply-pinned".into(), field.into_any_element()));
    }
    Some(panel)
}

/// Whether the field for an answer to thread `index` would end below the room the pane has even with the list at the top:
/// where its card ends (measured with the field in it: if the field is pinned the card does not have it, so its height and
/// the gap are added), under the panel's padding on both sides. None while the places are not known.
fn reply_cut_off(bounds: &std::collections::BTreeMap<String, Bounds<Pixels>>, index: usize, pinned: bool, field_height: f32, room: f32) -> Option<bool> {
    let (card, body) = (bounds.get(&format!("card-{index}"))?, bounds.get("body")?);
    if room <= 0. {
        return None;
    }
    let padding = crate::chrome::FORM_PADDING;
    let missing = if pinned { field_height + CARD_GAP } else { 0. };
    let end = f32::from(card.bottom() - body.top()) + missing;
    Some(padding + end + padding > room)
}

/// The ⋯ of a post: a menu of 編集 (the user's own posts), 解決/再開 and 削除 (the thread's; on its first post). None when
/// there is nothing in it.
#[allow(clippy::too_many_arguments)]
fn post_menu<F: CommentFormat>(
    index: usize,
    j: usize,
    thread: &str,
    post: &CommentPost,
    first: bool,
    done: bool,
    ui: &CommentBounds,
    owner: &WeakEntity<SlideEditor>,
) -> Option<AnyElement> {
    let mut items: Vec<DropdownItem> = Vec::new();
    if post.editable {
        let (t, p) = (thread.to_string(), post.id.clone());
        let handler: Handler = Rc::new(move |this, window, cx| begin_edit::<F>(this, &t, &p, window, cx));
        items.push(DropdownItem::new(text("shell.comments.edit")).icon("edit").on_select(handler));
    }
    if first {
        let t = thread.to_string();
        let handler: Handler = Rc::new(move |this, _, cx| {
            F::set_resolved(this, &t, !done, cx);
            cx.notify();
        });
        items.push(DropdownItem::new(text(if done { "shell.comments.reopen" } else { "shell.comments.resolve" })).icon(if done { "undo" } else { "check" }).on_select(handler));
        let t = thread.to_string();
        let handler: Handler = Rc::new(move |this, _, cx| ask_delete::<F>(this, &t, cx));
        items.push(DropdownItem::new(text("shell.comments.delete")).icon("delete").on_select(handler));
    }
    if items.is_empty() {
        return None;
    }
    let button = crate::chrome::action_button(ElementId::NamedInteger("comment-more".into(), (index * 1000 + j) as u64))
        .ghost()
        .xsmall()
        .w(px(24.))
        .h(px(24.))
        .px_0()
        .icon(crate::icons::Material("more_horiz"))
        .tooltip(text("shell.row.actions"));
    let menu = Dropdown::with_button("comment-actions", button, items.into_iter().map(|i| i.menu().into()).collect(), false, index * 1000 + j, owner)
        .checkmarks(false)
        .searchable(false);
    Some(div().on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(mark(ui, format!("more-{index}-{j}"), menu.into_any_element())).into_any_element())
}

/// The hint under the field: shown while the field has text and the keyboard focus.
#[derive(IntoElement)]
struct SaveHint {
    input: Entity<TextareaState>,
}

impl RenderOnce for SaveHint {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.input.read(cx);
        let shown = !state.value().is_empty() && state.focus_handle(cx).contains_focused(window, cx);
        let mac = cfg!(target_os = "macos");
        h_flex()
            .gap_0p5()
            .items_center()
            .text_xs()
            .text_color(rgb(MUTED))
            .when(shown, |d| {
                d.when(mac, |d| d.child(gpui_kit::component::Icon::new(crate::icons::Material("keyboard_command_key")).size(px(12.))))
                    .child(text(if mac { "shell.comments.save-hint-mac" } else { "shell.comments.save-hint-other" }))
            })
    }
}

/// The field (the one text area of the state) in a rounded white rectangle: an optional heading (with the ✕ that puts
/// an edit away), the common text field (the ✕ inside clears it), the hint under it and the post button.
fn field_box<F: CommentFormat>(
    id: impl Into<FieldId>,
    heading: Option<SharedString>,
    input: &Entity<TextareaState>,
    ui: &CommentBounds,
    target: &Option<CommentTarget>,
    cx: &mut Context<SlideEditor>,
) -> AnyElement {
    let id: FieldId = id.into();
    let editing = matches!(target, Some(CommentTarget::Edit { .. }));
    let tip = match (cfg!(target_os = "macos"), editing) {
        (true, false) => "shell.comments.post-tip-mac",
        (false, false) => "shell.comments.post-tip-other",
        (true, true) => "shell.comments.save-tip-mac",
        (false, true) => "shell.comments.save-tip-other",
    };
    let send_input = input.clone();
    let send = crate::chrome::primary_button(ElementId::Name(format!("comment-send-{}", id.0).into()))
        .small()
        .icon(crate::icons::Material("send"))
        .tooltip(text(tip))
        .on_click(cx.listener(move |this, _, window, cx| {
            let body = send_input.read(cx).value().to_string();
            send::<F>(this, &body, window, cx);
        }));
    let cancel_id = ElementId::Name(format!("comment-cancel-{}", id.0).into());
    // The ✕ at the rectangle's corner only ends an edit of a posted comment (the field's own ✕ only clears the text).
    let head = (heading.is_some() || editing).then(|| {
        h_flex()
            .justify_between()
            .items_center()
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(heading.unwrap_or_default()))
            .when(editing, |d| {
                d.child(mark(ui, format!("cancel-{}", id.0),
                    div()
                        .id(cancel_id)
                        .px(px(4.))
                        .py(px(2.))
                        .cursor_pointer()
                        .text_sm()
                        .text_color(rgb(0x888888))
                        .hover(|h| h.text_color(rgb(0x000000)))
                        .child(gpui_kit::component::Icon::new(crate::icons::Material("close")).size(px(14.)))
                        .on_click(cx.listener(|this, _, window, cx| cancel::<F>(this, window, cx)))
                        .into_any_element(),
                ))
            })
    });
    post_box()
        .gap_2()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(head)
        .child(crate::chrome::text_area_field(input))
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(SaveHint { input: input.clone() })
                .child(mark(ui, format!("send-{}", id.0), send.into_any_element())),
        )
        .into_any_element()
}

/// The suffix of a field's element ids, from its place in the list.
pub(crate) struct FieldId(String);
impl From<&str> for FieldId {
    fn from(s: &str) -> Self { Self(s.to_string()) }
}
impl From<(&str, usize)> for FieldId {
    fn from((s, n): (&str, usize)) -> Self { Self(format!("{s}-{n}")) }
}

/// The title of the comment of `cell`/`n` for a format that names it: see `CommentFormat::new_prompt`.
pub fn cell_prompt(cell: &str) -> SharedString {
    i18n_format("shell.comments.new-cell", &[("cell", &cell)]).into()
}

/// Who writes (the user's name, shell::user_name) and now, in UTC.
pub fn comment_author() -> ibis_doc_model::flow_command::CommentAuthor {
    // The same name as the other formats' comments (custom override,
    // else the OS's full name, else the account name).
    let author = crate::user_name::user_name();
    let initials = crate::user_name::user_initials();
    ibis_doc_model::flow_command::CommentAuthor { author, initials, date: now_utc() }
}

/// The time now as `w:date` writes it (`2026-10-01T09:00:00Z`).
pub(crate) fn now_utc() -> String {
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
