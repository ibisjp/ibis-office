// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K-kbd-close (iOS and Android; design: `tasks/doc/test-design/K0091.md`; the numbers KC1... are that document's): the ⌄ of the keyboard's toolbar
//! (K0064) closes the soft keyboard and it stays closed until the user taps a place to type in. What the platform asks after a tap is
//! `SlideEditor::accepts_text_now` (the same test `input_region` registers the text input by: the iOS platform's `tick` shows the keyboard when it is
//! true and hides it when not): it must be false after the ⌄ while the caret, the selection, the cell being typed into and the text box being edited
//! stay. The platforms' own tick and the real keyboard are for the simulators (the design's section 5): no scenario here is proof of them.
use crate::docx::scenarios::DocSteps;
use ibis_office_docx::ext::*;
use crate::shell::keyboard_toolbar::{KeyboardKind, SoftKeyboard, Target};
use crate::shell::scenario::{Scenario, button_probe as buttons};
use gpui_kit::*;
use ibis_formats::Kind;
use ibis_office_xlsx::ext::*;

const PHONE_KEYBOARD: SoftKeyboard = SoftKeyboard { height: 300., kind: KeyboardKind::Docked, duration: 0.25 };

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::kkbd_close_kc8_without_the_toolbars_button_the_text_input_is_taken_as_it_was", kc8_unchanged),
        ("shell::kkbd_close_kc1_the_button_closes_the_keyboard_in_a_document_and_keeps_the_caret", kc1_document),
        ("shell::kkbd_close_kc2_the_button_closes_the_keyboard_in_a_cell_and_keeps_the_edit", kc2_cell),
        ("shell::kkbd_close_kc3_the_button_closes_the_keyboard_in_a_text_box_and_keeps_the_edit", kc3_text_box),
        ("shell::kkbd_close_kc4_the_button_takes_the_focus_off_a_text_field", kc4_text_field),
        ("shell::kkbd_close_kc5_a_tap_on_the_text_opens_it_again", kc5_tap_text_again),
        ("shell::kkbd_close_kc6_other_toolbar_buttons_and_other_taps_do_not_open_it", kc6_other_taps),
        ("shell::kkbd_close_kc7_it_stays_closed_over_time_and_over_frames", kc7_stays_closed),
        ("shell::kkbd_close_kc9_a_new_file_starts_with_the_keyboard_allowed", kc9_state_is_the_windows_own),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}


fn pt(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

fn tap(s: &mut Scenario, at: Point<Pixels>) {
    let t = |phase| PlatformInput::Touch(TouchEvent { id: TouchId(1), phase, position: at, predicted_position: None, force: None });
    s.dispatch(vec![t(TouchPhase::Started)]);
    s.advance_ms(60);
    s.dispatch(vec![t(TouchPhase::Ended)]);
    s.settle();
}

/// A phone's window with the keyboard up and a finger on it.
fn screen(s: &mut Scenario) {
    s.size(390., 700.);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        e.soft_keyboard = PHONE_KEYBOARD;
        cx.notify();
    });
    s.settle();
}

fn document() -> Scenario {
    let mut s = Scenario::open("test/editing/docx-report.docx"); // (a blank document loaded directly has no paragraph)
    screen(&mut s);
    let (id, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    s.type_text("abc");
    s
}

fn workbook() -> Scenario {
    let mut s = Scenario::new_document(Kind::Workbook);
    screen(&mut s);
    s.type_text("12");
    s
}

fn text_box() -> Scenario {
    let mut s = Scenario::new_document(Kind::Presentation);
    screen(&mut s);
    s.update(|e, window, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(914_400, 914_400, 3_657_600, 914_400);
        e.execute(vec![ibis_doc_model::Command::InsertShape { slide: e.current_slide, shapes: vec![(0, ibis_doc_model::Shape::text_box(id, frame, "box"))] }], cx);
        e.begin_text_edit(id, false, false, window, cx);
    });
    s.settle();
    s
}

fn text_screen_pos(s: &mut Scenario, pos: ibis_doc_model::FlowPos) -> (f32, f32) {
    let (page, x, y, h) = s.update(|e, _, cx| {
        let lines = e.flow_lines();
        let (page, x, y, h) = e.caret_box(&lines, pos).expect("the position is laid out");
        e.scroll_into_view(page, y, h, cx);
        (page, x, y, h)
    });
    let (origin, k) = s.with(|e, _| e.page_origin(page)).expect("the page is placed");
    (f32::from(origin.x) + x * k + 0.5, f32::from(origin.y) + (y + h / 2.0) * k)
}

fn accepts(s: &mut Scenario) -> bool {
    s.update(|e, window, cx| e.accepts_text_now(window, cx))
}

fn press_hide(s: &mut Scenario) {
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::click(s, "kbd-hide");
    s.settle();
}

/// KC1 (the symptom): in a document with the caret, the ⌄ closes the keyboard (the toolbar goes) and the platform's check after the tap is told
/// "no text input" (it does not show the keyboard again); the caret is where it was and typing is still the document's once the keyboard is brought back.
fn kc1_document() {
    let mut s = document();
    assert!(accepts(&mut s), "setup: the document takes text");
    let target = s.with(|e, _| e.keyboard_target());
    press_hide(&mut s);
    assert!(!s.with(|e, _| e.soft_keyboard.visible()), "the keyboard is hidden");
    assert!(!accepts(&mut s), "and the platform's check is told no: it does not show the keyboard again");
    assert_eq!(s.with(|e, _| e.keyboard_target()), target, "the caret is kept: the document is still what is being typed into");
}

/// KC2: in a workbook with a cell being typed into ("12" not entered), the ⌄ closes the keyboard, the check is told no, and nothing is committed (the
/// cell is still being edited, K0064).
fn kc2_cell() {
    let mut s = workbook();
    assert_eq!(s.with(|e, _| e.keyboard_target()), Target::SheetValue, "setup: a cell being typed into");
    assert!(accepts(&mut s), "setup");
    press_hide(&mut s);
    assert!(!s.with(|e, _| e.soft_keyboard.visible()));
    assert!(!accepts(&mut s), "the check is told no");
    assert_eq!(s.with(|e, _| e.keyboard_target()), Target::SheetValue, "the cell is still being edited, nothing committed");
}

/// KC3: in a presentation with a text box being edited, the same.
fn kc3_text_box() {
    let mut s = text_box();
    assert!(s.with(|e, _| e.text_editing().is_some()), "setup: a text box is being edited");
    assert!(accepts(&mut s), "setup");
    press_hide(&mut s);
    assert!(!s.with(|e, _| e.soft_keyboard.visible()));
    assert!(!accepts(&mut s), "the check is told no");
    assert!(s.with(|e, _| e.text_editing().is_some()), "the text box is still being edited");
}

/// KC4: a text field (the find pane's field: a text field of the pane, not the page): the ⌄ takes the focus off it (the field is not focused, so the
/// platform's check has nothing that takes text), and the keyboard closes.
fn kc4_text_field() {
    let _probe = buttons::Probe::new(); // (input_scope works only while a probe is on)
    let mut s = Scenario::open("test/editing/docx-report.docx");
    screen(&mut s);
    buttons::input_scope(true);
    s.update(|e, window, cx| e.open_find(window, cx));
    s.settle();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let focused = |s: &mut Scenario| -> bool {
        s.update(|_, window, cx| buttons::inputs(cx).first().map(|i| i.read(cx).focus_handle(cx).is_focused(window)).unwrap_or(false))
    };
    assert!(focused(&mut s), "setup: the field has the focus");
    buttons::click(&mut s, "kbd-hide");
    s.settle();
    assert!(!s.with(|e, _| e.soft_keyboard.visible()), "the keyboard is hidden");
    assert!(!focused(&mut s), "the field has lost the focus");
    buttons::input_scope(false);
}

/// KC5 (the requirement's second half): after the ⌄, a tap on the text (the document's body, a cell, the text box, the text field) is what opens the
/// keyboard again: the check is told yes.
fn kc5_tap_text_again() {
    let mut s = document();
    press_hide(&mut s);
    assert!(!accepts(&mut s));
    let (id, _) = s.paragraph(0);
    let at = text_screen_pos(&mut s, ibis_doc_model::FlowPos::new(id, 1));
    tap(&mut s, pt(at.0, at.1));
    assert!(accepts(&mut s), "document: a tap on the text asks for the keyboard");
    drop(s); // two Scenarios cannot be alive together
    let mut s = workbook();
    press_hide(&mut s);
    assert!(!accepts(&mut s));
    let c = s.with(|e, _| e.cell_center(2, 2)).expect("a cell is on screen");
    tap(&mut s, c);
    s.type_text("5");
    assert!(accepts(&mut s), "workbook: a tap on a cell and typing asks for it");
    drop(s);
    let mut s = text_box();
    press_hide(&mut s);
    assert!(!accepts(&mut s));
    let at = s.shape_position(0);
    tap(&mut s, pt(at.0, at.1));
    assert!(accepts(&mut s), "text box: a tap on it asks for the keyboard");
}

/// KC6: with the keyboard closed, a tap on the status bar, on the title row's blank part, on a toolbar's button that is not the ⌄ (while it is shown),
/// and on the empty space around the page does not open it (only a place to type in does).
fn kc6_other_taps() {
    let mut s = document();
    press_hide(&mut s);
    for at in [pt(195., 8.), pt(195., 690.), pt(2., 300.)] {
        tap(&mut s, at);
        assert!(!accepts(&mut s), "a tap at {at:?} did not ask for the keyboard");
    }
    // While the keyboard is up, another button of the toolbar leaves it up (the ⌄ is the one that closes it).
    drop(s);
    let mut s = document();
    {
        let _probe = buttons::Probe::new();
        s.update(|_, _, cx| cx.notify());
        s.settle();
        buttons::click(&mut s, "kbd-bold");
    }
    assert!(s.with(|e, _| e.soft_keyboard.visible()) && accepts(&mut s), "bold leaves the keyboard up and the text input taken");
}

/// KC7: the keyboard stays closed: 3 s later, over many frames, and while the window is laid out again (a resize), the check is still told no. (The iOS
/// platform asks again for 3 ticks after the ⌄ — `ios-keyboard-diag` — and must hear the same.)
fn kc7_stays_closed() {
    let mut s = document();
    press_hide(&mut s);
    for _ in 0..200 {
        s.advance_ms(16);
        assert!(!accepts(&mut s), "still told no");
    }
    s.size(391., 700.);
    s.settle();
    assert!(!accepts(&mut s), "and after a resize");
}

/// KC8 (unchanged; passes now): where there is no button to press the check is as it was: a document with a caret takes text, with or without a soft
/// keyboard, on a computer.
fn kc8_unchanged() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(900., 700.);
    s.settle();
    let (id, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    assert!(accepts(&mut s), "a computer's document with a caret takes text");
    s.update(|e, _, cx| {
        e.soft_keyboard = SoftKeyboard::HIDDEN;
        cx.notify();
    });
    s.settle();
    assert!(accepts(&mut s), "with no keyboard it takes text as before");
}

/// KC9: the closed state is the window's own and is not carried to another file: after the ⌄ the file is closed (back to the start view) and another one
/// opened in the same window: it takes text and shows the keyboard on its first tap on text.
fn kc9_state_is_the_windows_own() {
    let mut s = document();
    press_hide(&mut s);
    s.update(|e, window, cx| e.back_to_start_on(true, window, cx));
    s.settle();
    // (The document has typed text: it asks first; the answer 保存しない goes to the start view.)
    s.update(|e, window, cx| e.confirm_discard_and_close(window, cx));
    s.settle();
    assert!(!s.with(|e, _| e.has_document), "setup: back at the start view");
    s.update(|e, window, cx| e.new_file(Kind::Document, window, cx));
    s.settle();
    s.key("cmd-8");
    screen(&mut s);
    let (id, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    let info = s.with(|e, _| (e.keyboard_closed, e.has_document, e.doc_state().map(|d| d.caret.is_some())));
    assert!(accepts(&mut s), "a new file takes text: the old file's ⌄ is forgotten ({info:?})");
}
