// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The comments' one panel (shell/comments.rs) on the three formats. Every move is what the user does: the menu,
//! typing, and a click on the part where it was drawn (`CommentPanelState::ui` gives where each was drawn).

use super::{Scenario, button_probe as buttons};
use crate::shell::comments::CommentFormat;
use crate::shell::dialog::InteractionLayer;
use crate::xlsx::scenarios::SheetSteps;
use crate::xlsx::threaded_comments::XlsxComments;
use crate::docx::comments::DocxComments;
use crate::docx::scenarios::DocSteps;
use crate::pptx::comments::PptxComments;
use ibis_doc_model::FlowPos;
use crate::docx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::comments_common_workbook", workbook),
        ("shell::comments_common_document", document),
        ("shell::comments_common_presentation", presentation),
        ("shell::comments_polish_scroll_reset", scroll_reset),
        ("shell::comments_polish_hover_mark", hover_mark),
        ("shell::comments_round2_popup", round2_popup),
        ("shell::comments_round2_panel", round2_panel),
    ]
}

/// The window position of the middle of the panel's part `name`, as it was drawn.
fn center<F: CommentFormat>(s: &mut Scenario, name: &str) -> (f32, f32) {
    s.settle();
    let b = s.with(|e, _| F::state(e).and_then(|st| st.ui.borrow().get(name).copied()));
    let b = b.unwrap_or_else(|| panic!("the part {name} was not drawn"));
    (f32::from(b.center().x), f32::from(b.center().y))
}

/// A item of a post's ⋯ menu, as the user does it: click the ⋯, click the item (by its place in the menu).
fn pick<F: CommentFormat>(s: &mut Scenario, more: &str, item: usize) {
    let mut n = more.trim_start_matches("more-").split('-').map(|v| v.parse::<usize>().unwrap());
    let (thread, post) = (n.next().unwrap(), n.next().unwrap());
    let find = |item: usize| buttons::all().into_iter().find(|id| id.contains("searchable-item-comment-actions") && id.ends_with(&format!(", {item})")));
    let mut row = None;
    // (A menu that an earlier pick left half-open takes a second click.)
    for _ in 0..3 {
        buttons::clear();
        s.settle();
        buttons::click(s, &format!("NamedInteger(\"comment-more\", {})", thread * 1000 + post));
        s.settle();
        row = find(item);
        if row.is_some() {
            break;
        }
    }
    s.screenshot("comments-menu");
    let row = row.unwrap_or_else(|| panic!("the menu of {more} has no item {item}: {:?}", buttons::all()));
    buttons::click(s, &row);
    s.settle();
}

fn press<F: CommentFormat>(s: &mut Scenario, name: &str) {
    let at = center::<F>(s, name);
    s.click(at);
    s.settle();
}

/// The delete question's buttons live in the same map, but the panel's state may be gone from the view: read it from
/// the pending delete.
fn press_question(s: &mut Scenario, name: &str) {
    s.settle();
    let b = s.with(|e, _| e.comment_delete.as_ref().and_then(|p| p.ui.borrow().get(name).copied()));
    let b = b.unwrap_or_else(|| panic!("the question's {name} was not drawn"));
    s.click((f32::from(b.center().x), f32::from(b.center().y)));
    s.settle();
}

fn threads(s: &mut Scenario) -> Vec<(String, usize, bool)> {
    s.with(|e, _| XlsxComments::threads(e).into_iter().map(|t| (t.anchor.unwrap_or_default(), t.posts.len(), t.resolved)).collect())
}

fn thread_at(s: &mut Scenario, cell: (u32, u32)) -> Option<ibis_doc_model::ThreadedComment> {
    s.with(|e, _| e.current_sheet().unwrap().1.thread_at(cell.1, cell.0).cloned())
}

/// The workbook: the list of all the sheet's threads by cell, a new comment, an answer, an edit, resolve and reopen,
/// a delete after the question (no, then yes), Undo, and a click on a card going to its cell.
fn workbook() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-threads.xlsx");
    s.size(1400., 800.);
    s.click_cell("C3");
    s.menu("insert.comment");
    s.settle();
    assert!(s.with(|e, _| e.sidebar_open && XlsxComments::state(e).is_some()), "the panel is open");
    let before = threads(&mut s);
    assert_eq!(before.len(), 2, "the sheet's two threads are listed: {before:?}");
    assert_eq!(before[0].0, "B2");
    assert_eq!(before[1].0, "B4");
    assert!(before[1].2, "B4 is resolved");

    // A new comment on C3 (no thread there): type, click the post button.
    s.type_text("New comment on C3");
    s.screenshot("comments-xlsx-typing-hint");
    press::<XlsxComments>(&mut s, "send-new");
    let t = thread_at(&mut s, (2, 2)).expect("the thread on C3");
    assert_eq!(t.posts[0].text, "New comment on C3");
    let list = threads(&mut s);
    assert_eq!(list.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(), ["B2", "C3", "B4"], "listed by cell");
    s.screenshot("comments-xlsx-list");

    // An answer: C3 is selected and has the thread now, so its card has the field; type, post.
    s.type_text("An answer");
    press::<XlsxComments>(&mut s, "send-reply-1");
    assert_eq!(thread_at(&mut s, (2, 2)).unwrap().posts.len(), 2);

    // Edit the answer (the user's own): its text comes into the field.
    pick::<XlsxComments>(&mut s, "more-1-1", 0);
    assert_eq!(s.with(|e, cx| XlsxComments::state(e).unwrap().input.read(cx).value().to_string()), "An answer");
    s.key("cmd-a");
    s.type_text("An edited answer");
    s.screenshot("comments-xlsx-editing");
    press::<XlsxComments>(&mut s, "send-edit-1001");
    assert_eq!(thread_at(&mut s, (2, 2)).unwrap().posts[1].text, "An edited answer");

    // Resolve and reopen.
    pick::<XlsxComments>(&mut s, "more-1-0", 1);
    assert!(thread_at(&mut s, (2, 2)).unwrap().done);
    pick::<XlsxComments>(&mut s, "more-1-0", 1);
    assert!(!thread_at(&mut s, (2, 2)).unwrap().done);

    // Delete asks first: キャンセル keeps it, 削除 removes it, Undo brings it back.
    pick::<XlsxComments>(&mut s, "more-1-0", 2);
    assert_eq!(s.with(|e, _| e.interaction_layer()), InteractionLayer::CommentDelete);
    assert!(thread_at(&mut s, (2, 2)).is_some(), "not yet");
    s.screenshot("comments-xlsx-delete-question");
    press_question(&mut s, "delete-cancel");
    assert_eq!(s.with(|e, _| e.interaction_layer()), InteractionLayer::Document);
    assert!(thread_at(&mut s, (2, 2)).is_some(), "kept");
    pick::<XlsxComments>(&mut s, "more-1-0", 2);
    press_question(&mut s, "delete-ok");
    assert!(thread_at(&mut s, (2, 2)).is_none(), "deleted");
    s.update(|e, w, cx| e.undo(&crate::Undo, w, cx));
    assert!(thread_at(&mut s, (2, 2)).is_some(), "Undo brings it back");

    // A click on the card of B4 goes to B4.
    press::<XlsxComments>(&mut s, "card-2");
    assert_eq!(s.with(|e, _| e.current_cell()), Some((1, 3)), "B4 is selected");
}

fn texts(s: &mut Scenario) -> Vec<String> {
    s.with(|e, _| DocxComments::threads(e).into_iter().map(|t| t.posts.iter().map(|p| p.text.clone()).collect::<Vec<_>>().join("|")).collect())
}

/// The document: a comment on the selection by the panel's field, a second one above it (listed by its place in
/// the text), a click on a card selecting its range, an answer, an edit, resolve, and the delete question.
fn document() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1400., 800.);
    let (p1, _) = s.paragraph(0);
    let (p2, p2_text) = s.paragraph(1);
    // No selection: the common message, no panel.
    s.click_text(FlowPos::new(p1, 0));
    s.menu("insert.comment");
    s.settle();
    assert!(s.with(|e, _| e.panel_message.is_some() && DocxComments::state(e).is_none()), "no selection: the message");
    s.update(|e, _, cx| e.dismiss_panel_message(cx));

    // A comment on a selection of the second paragraph.
    s.update(|e, _, cx| {
        e.set_caret(FlowPos::new(p2, 0), false, cx);
        e.set_caret(FlowPos::new(p2, p2_text.len().min(10)), true, cx);
    });
    s.menu("insert.comment");
    s.type_text("Comment on the second paragraph");
    press::<DocxComments>(&mut s, "send-new");
    assert_eq!(s.document().comments.len(), 1);
    // And one on the first paragraph: listed first (its place is above).
    s.update(|e, _, cx| {
        e.set_caret(FlowPos::new(p1, 0), false, cx);
        e.set_caret(FlowPos::new(p1, 5), true, cx);
    });
    s.menu("insert.comment");
    s.type_text("Comment on the first paragraph");
    press::<DocxComments>(&mut s, "send-new");
    assert_eq!(texts(&mut s), ["Comment on the first paragraph", "Comment on the second paragraph"], "listed by their places in the text");
    s.screenshot("comments-docx-list");

    // A click on the second card selects the second paragraph's range.
    press::<DocxComments>(&mut s, "card-1");
    let sel = s.selection().expect("the range is selected");
    assert_eq!(sel.0.para, p2, "the click went to the comment's place");

    // An answer to the first, then an edit of the answer.
    press::<DocxComments>(&mut s, "reply-0");
    s.type_text("An answer");
    press::<DocxComments>(&mut s, "send-reply-0");
    assert_eq!(texts(&mut s)[0], "Comment on the first paragraph|An answer");
    pick::<DocxComments>(&mut s, "more-0-1", 0);
    s.key("cmd-a");
    s.type_text("An edited answer");
    press::<DocxComments>(&mut s, "send-edit-1");
    assert_eq!(texts(&mut s)[0], "Comment on the first paragraph|An edited answer");
    assert_eq!(s.document().comments.len(), 3);

    // Resolve (the thread with its answer), reopen.
    pick::<DocxComments>(&mut s, "more-0-0", 1);
    assert_eq!(s.document().comments.iter().filter(|c| c.done).count(), 2, "the comment and its answer are resolved");
    assert!(s.with(|e, _| DocxComments::threads(e)[0].resolved));
    pick::<DocxComments>(&mut s, "more-0-0", 1);
    assert!(!s.with(|e, _| DocxComments::threads(e)[0].resolved));

    // Delete asks first.
    pick::<DocxComments>(&mut s, "more-0-0", 2);
    assert_eq!(s.with(|e, _| e.interaction_layer()), InteractionLayer::CommentDelete);
    s.screenshot("comments-docx-delete-question");
    press_question(&mut s, "delete-cancel");
    assert_eq!(s.document().comments.len(), 3, "kept");
    pick::<DocxComments>(&mut s, "more-0-0", 2);
    press_question(&mut s, "delete-ok");
    assert_eq!(s.document().comments.len(), 1, "the thread and its answer went");
    s.update(|e, w, cx| e.undo(&crate::Undo, w, cx));
    assert_eq!(s.document().comments.len(), 3, "Undo brings them back");
}

/// The presentation: a comment on the selected shape, one on the slide, an answer, an edit, resolve, the delete
/// question, and a click on a card selecting its shape.
fn presentation() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1400., 800.);
    s.page(1);
    s.select_shape(0);
    let shape = s.with(|e, _| e.selected_shapes().first().map(|sh| sh.id)).expect("a shape is selected");
    s.menu("insert.comment");
    s.type_text("Comment on the shape");
    press::<PptxComments>(&mut s, "send-new");
    assert_eq!(s.with(|e, _| e.slide().comments.len()), 1);
    assert_eq!(s.with(|e, _| e.slide().comments[0].shape), Some(shape));
    s.screenshot("comments-pptx-list");

    // An answer and its edit.
    press::<PptxComments>(&mut s, "reply-0");
    s.type_text("An answer");
    press::<PptxComments>(&mut s, "send-reply-0");
    assert_eq!(s.with(|e, _| e.slide().comments[0].replies.len()), 1);
    pick::<PptxComments>(&mut s, "more-0-1", 0);
    s.key("cmd-a");
    s.type_text("An edited answer");
    press::<PptxComments>(&mut s, "send-edit-1");
    assert_eq!(s.with(|e, _| e.slide().comments[0].replies[0].text.clone()), "An edited answer");

    // Resolve, reopen.
    pick::<PptxComments>(&mut s, "more-0-0", 1);
    assert!(s.with(|e, _| e.slide().comments[0].resolved));
    pick::<PptxComments>(&mut s, "more-0-0", 1);
    assert!(!s.with(|e, _| e.slide().comments[0].resolved));

    // A click on the card selects the commented shape.
    s.update(|e, _, cx| {
        e.shape_edit.selection.clear();
        cx.notify();
    });
    press::<PptxComments>(&mut s, "card-0");
    assert_eq!(s.with(|e, _| e.selected_shapes().first().map(|sh| sh.id)), Some(shape));

    // Delete asks first.
    pick::<PptxComments>(&mut s, "more-0-0", 2);
    assert_eq!(s.with(|e, _| e.interaction_layer()), InteractionLayer::CommentDelete);
    s.screenshot("comments-pptx-delete-question");
    press_question(&mut s, "delete-cancel");
    assert_eq!(s.with(|e, _| e.slide().comments.len()), 1, "kept");
    pick::<PptxComments>(&mut s, "more-0-0", 2);
    press_question(&mut s, "delete-ok");
    assert!(s.with(|e, _| e.slide().comments.is_empty()), "deleted");
    s.update(|e, w, cx| e.undo(&crate::Undo, w, cx));
    assert_eq!(s.with(|e, _| e.slide().comments.len()), 1, "Undo brings it back");
}

/// (c) The panel shown, a new cell chosen in the body: the panel's scroll goes back to the top, where the field for a new
/// comment is.
fn scroll_reset() {
    let mut s = Scenario::open("test/editing/xlsx-threads.xlsx");
    s.size(1400., 500.);
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let commands = (4..20u32)
            .map(|row| {
                let post = ibis_doc_model::ThreadPost { author: "Tester".into(), date: "2026-11-01T09:00:00.00".into(), text: format!("Thread on row {row}") };
                let thread = ibis_doc_model::ThreadedComment { id: format!("{{00000000-0000-4000-8000-{row:012}}}"), row, column: 3, posts: vec![post], done: false };
                ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetThreadedComment { slide, row, column: 3, thread: Some(thread) })
            })
            .collect();
        e.document.execute(commands).unwrap();
    });
    s.click_cell("A1");
    s.menu("insert.comment");
    s.settle();
    // The list is longer than the pane: scroll to its end.
    s.update(|e, _, cx| {
        let max = e.sidebar_scroll.max_offset().y;
        e.sidebar_scroll.set_offset(gpui_kit::point(gpui_kit::px(0.), -max));
        cx.notify();
    });
    s.settle();
    assert!(s.with(|e, _| f32::from(e.sidebar_scroll.offset().y)) < -50., "the list is scrolled down");
    s.screenshot("comments-xlsx-scrolled");
    s.click_cell("F30");
    s.settle();
    assert_eq!(s.with(|e, _| f32::from(e.sidebar_scroll.offset().y)), 0., "a new cell: the panel is at the top");
    s.screenshot("comments-xlsx-scroll-reset");
}

/// (d) The pointer on the comment mark (the square that holds the corner triangle) shows the card; the rest of the cell
/// does not. (e) A long thread's card scrolls, with its scrollbar.
fn hover_mark() {
    let mut s = Scenario::open("test/editing/xlsx-threads.xlsx");
    s.size(1400., 800.);
    // B2's top right corner: the bottom of row 1's right edge of column B.
    let tr = s.with(|e, _| e.cell_corner(1, 0)).unwrap();
    let (x, y) = (f32::from(tr.x), f32::from(tr.y));
    s.hover((x - 30., y + 12.));
    assert_eq!(s.with(|e, _| e.wb().comment_hover), None, "inside the cell, off the mark");
    s.hover((x - 3., y + 3.));
    assert_eq!(s.with(|e, _| e.wb().comment_hover), Some((1, 1)), "on the mark");
    s.screenshot("comments-xlsx-hover-card");
    s.hover((x - 3., y + 30.));
    assert_eq!(s.with(|e, _| e.wb().comment_hover), None, "below the mark");
    s.hover((x - 30., y + 3.));
    assert_eq!(s.with(|e, _| e.wb().comment_hover), None, "left of the mark");

    // A long thread: the card is not taller than its limit, and scrolls.
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let posts = (0..14).map(|i| ibis_doc_model::ThreadPost { author: "Tester".into(), date: "2026-11-01T09:00:00.00".into(), text: format!("Post number {i} of a long thread") }).collect();
        let thread = ibis_doc_model::ThreadedComment { id: "{00000000-0000-4000-8000-000000000001}".into(), row: 1, column: 1, posts, done: false };
        e.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetThreadedComment { slide, row: 1, column: 1, thread: Some(thread) })]).unwrap();
    });
    s.hover((x - 3., y + 3.));
    s.settle();
    assert_eq!(s.with(|e, _| e.wb().comment_hover), Some((1, 1)));
    assert!(s.with(|e, _| f32::from(e.wb().comment_scroll.max_offset().y)) > 50., "the card scrolls");
    s.screenshot("comments-xlsx-hover-card-long");
}

// ----- Round 2 (2026-11-03): the popup's hover, the pinned field, the card to the top, own and others' posts ---------

/// A thread on column D (`3`) of `row`: one post by someone else, one by the user.
fn thread_on(slide: ibis_doc_model::SlideId, row: u32) -> ibis_doc_model::Command {
    let other = ibis_doc_model::ThreadPost { author: "Tester".into(), date: "2026-11-01T09:00:00.00".into(), text: format!("Thread on row {row}") };
    let own = ibis_doc_model::ThreadPost { author: crate::shell::user_name::user_name(), date: "2026-11-01T09:30:00.00".into(), text: "My answer".into() };
    let thread = ibis_doc_model::ThreadedComment { id: format!("{{00000000-0000-4000-8000-{row:012}}}"), row, column: 3, posts: vec![other, own], done: false };
    ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetThreadedComment { slide, row, column: 3, thread: Some(thread) })
}

fn shown(s: &mut Scenario) -> Option<(u32, u32)> {
    s.with(|e, _| e.wb().comment_hover)
}

fn popup_bounds(s: &mut Scenario) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    s.with(|e, _| e.wb().comment_popup.bounds.get()).expect("the popup was drawn")
}

/// (2) The popup of a comment: kept 0.5 s after a pointer that was on the mark for 1 s or more leaves it; not kept after a
/// shorter time; kept while the pointer is on it; its left edge is the right edge of the mark's square; as tall as the
/// window holds; the wheel on it scrolls it.
fn round2_popup() {
    let mut s = Scenario::open("test/editing/xlsx-threads.xlsx");
    s.size(1400., 800.);
    // B2's top right corner (the right edge of the square that holds the mark).
    let tr = s.with(|e, _| e.cell_top_right(1, 1)).unwrap();
    let (x, y) = (f32::from(tr.x), f32::from(tr.y));
    let (on_mark, off_mark) = ((x - 3., y + 3.), (x - 40., y + 14.));

    // A short stay on the mark: the popup goes as the pointer leaves.
    s.hover(on_mark);
    assert_eq!(shown(&mut s), Some((1, 1)));
    s.wait(300);
    s.hover(off_mark);
    assert_eq!(shown(&mut s), None, "after less than a second: gone at once");

    // A stay of a second: it stays for half a second more.
    s.hover(on_mark);
    s.wait(1100);
    s.hover(off_mark);
    assert_eq!(shown(&mut s), Some((1, 1)), "just left");
    s.wait(400);
    assert_eq!(shown(&mut s), Some((1, 1)), "0.4 s after leaving");
    s.wait(200);
    assert_eq!(shown(&mut s), None, "0.6 s after leaving");

    // Back on the mark within the grace time: it stays and is not hidden afterwards.
    s.hover(on_mark);
    s.wait(1100);
    s.hover(off_mark);
    s.wait(300);
    s.hover(on_mark);
    s.wait(1000);
    assert_eq!(shown(&mut s), Some((1, 1)), "the pointer came back to the mark");

    // No gap: the popup's left edge is the right edge of the square on the mark, its top the cell's top.
    let card = popup_bounds(&mut s);
    // (The bounds are those inside the card's 1 px border.)
    assert!((f32::from(card.left()) - 1. - x).abs() < 0.5, "left {} vs the mark's right edge {x}", f32::from(card.left()));
    assert!((f32::from(card.top()) - 1. - y).abs() < 0.5, "top {} vs the cell's top {y}: {card:?}", f32::from(card.top()));
    // The pointer crossing from the mark's last pixel to the popup's first one: never off both.
    s.hover((x - 0.5, y + 3.));
    s.hover((x + 0.5, y + 3.));
    s.wait(700);
    assert_eq!(shown(&mut s), Some((1, 1)), "on the popup, long after");
    s.screenshot("comments-round2-popup-on-card");
    // On the popup: it stays as long as the pointer is on it, then goes after the grace time.
    s.wait(3000);
    assert_eq!(shown(&mut s), Some((1, 1)), "still on the popup");
    s.hover((x + 100., y + 400.));
    s.wait(300);
    assert_eq!(shown(&mut s), Some((1, 1)), "just left the popup");
    s.wait(300);
    assert_eq!(shown(&mut s), None, "0.6 s after leaving the popup");

    // A long thread: the popup is as tall as the window holds (8 px of margin at both ends), at the margin, and scrolls.
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let posts = (0..40).map(|i| ibis_doc_model::ThreadPost { author: if i % 3 == 2 { crate::shell::user_name::user_name() } else { "Tester".into() }, date: "2026-11-01T09:00:00.00".into(), text: format!("Post number {i} of a long thread") }).collect();
        let thread = ibis_doc_model::ThreadedComment { id: "{00000000-0000-4000-8000-000000000001}".into(), row: 1, column: 1, posts, done: false };
        e.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetThreadedComment { slide, row: 1, column: 1, thread: Some(thread) })]).unwrap();
    });
    s.hover(on_mark);
    s.wait(100);
    let card = popup_bounds(&mut s);
    let height = f32::from(card.size.height);
    assert!((height + 2. - (800. - 16.)).abs() < 1., "as tall as the window allows: {height}");
    assert!((f32::from(card.top()) - 9.).abs() < 1. && (f32::from(card.bottom()) - 791.).abs() < 1., "inside the window: {card:?}");
    s.hover((x + 40., y + 200.));
    let before = s.with(|e, _| f32::from(e.wb().comment_scroll.offset().y));
    s.wheel((x + 40., y + 200.), -300.);
    let after = s.with(|e, _| f32::from(e.wb().comment_scroll.offset().y));
    assert!(after < before - 100., "the wheel on the popup scrolls it: {before} -> {after}");
    assert_eq!(shown(&mut s), Some((1, 1)));
    s.screenshot("comments-round2-popup-long");
}

/// (1) The field for an answer is pinned under the list when its place at the end of the thread is out of sight with the list
/// at the top. (3) A cell with a thread chosen in the body brings its card to the top of the panel's view. (5) The user's own
/// posts sit in; the others' do not.
fn round2_panel() {
    let mut s = Scenario::open("test/editing/xlsx-threads.xlsx");
    s.size(1400., 520.);
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let commands = (4..24u32).map(|r| thread_on(slide, r)).collect();
        e.document.execute(commands).unwrap();
    });
    s.click_cell("A1");
    s.menu("insert.comment");
    s.settle();
    let pinned = |s: &mut Scenario| s.with(|e, _| XlsxComments::state(e).map(|st| st.pinned_reply));
    let part = |s: &mut Scenario, name: &str| s.with(|e, _| XlsxComments::state(e).and_then(|st| st.ui.borrow().get(name).copied()));
    // (5) Own and others: the one's left edge is the indent in from the other's.
    let (other, own) = (part(&mut s, "post-2-0").expect("drawn"), part(&mut s, "post-2-1").expect("drawn"));
    assert!((f32::from(own.left() - other.left()) - f32::from(crate::shell::comments::OWN_INDENT)).abs() < 0.5, "own {:?} other {:?}", own, other);

    // A thread late in the list (row 20 of 4..24, the 19th of the 22 threads): the field for an answer would be far below the pane.
    s.click_cell("D21");
    s.settle();
    s.settle();
    assert_eq!(pinned(&mut s), Some(true), "its field is out of sight at the top of the list: pinned");
    let field = part(&mut s, "reply-pinned").expect("the pinned field was drawn");
    let panel_bottom = 520.;
    assert!(f32::from(field.bottom()) <= panel_bottom && f32::from(field.top()) > 100., "the pinned field is in the window: {field:?}");
    assert!(part(&mut s, "reply-field").is_none() || pinned(&mut s) == Some(true));
    // (3) Its card is at the top of the view (as far as the end of the list lets it be).
    let (card, view) = (part(&mut s, "card-18").expect("drawn"), s.with(|e, _| e.sidebar_scroll.bounds()));
    let gap = f32::from(card.top() - view.top());
    assert!((0. ..=crate::shell::chrome::FORM_PADDING + 1.).contains(&gap), "the card's top is at the view's top: {gap}");
    s.screenshot("comments-round2-pinned");

    // A thread at the top of the list: its field is in sight, inline.
    s.click_cell("B2");
    s.settle();
    s.settle();
    assert_eq!(pinned(&mut s), Some(false), "in sight: inline");
    assert!(part(&mut s, "reply-field").is_some());
    let (card, view) = (part(&mut s, "card-0").expect("drawn"), s.with(|e, _| e.sidebar_scroll.bounds()));
    assert!(f32::from(card.top() - view.top()) <= crate::shell::chrome::FORM_PADDING + 1., "the first card is at the top: card {card:?} view {view:?} offset {:?} max {:?}", s.with(|e, _| e.sidebar_scroll.offset()), s.with(|e, _| e.sidebar_scroll.max_offset()));
    s.screenshot("comments-round2-inline");

    // A thread in the middle: its card goes to the top of the view, after a scroll (not by the user's click in the panel).
    s.click_cell("D13");
    s.settle();
    s.settle();
    let (card, view) = (part(&mut s, "card-10").expect("drawn"), s.with(|e, _| e.sidebar_scroll.bounds()));
    let gap = f32::from(card.top() - view.top());
    assert!((gap - crate::shell::chrome::FORM_PADDING).abs() < 1., "the middle thread's card is at the top of the view: {gap}");
    s.screenshot("comments-round2-to-top");
}
