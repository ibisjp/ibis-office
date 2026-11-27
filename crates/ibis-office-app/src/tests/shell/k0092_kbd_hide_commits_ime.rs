// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The ⌄ of the keyboard's toolbar (K0064, K0091) with text being composed (an input method's marked text, underlined): the composing text is put in the
//! text first, as the OS's unmarkText does when a keyboard goes, and the keyboard closes. Before, the composition was left on the page (drawn, not in the
//! text) and kept by the keyboard, which typed it again when it came back (the report: 「あいうえお」 composing, ⌄, a tap on the text: the committed
//! text and the underlined composition both there, the caret gone).
//! The numbers H1... are this file's: H1 a document (the text goes in once, no composition, the caret after it, the keyboard closed), H2 a tap on the text
//! afterwards (nothing is doubled, no composition comes back), H3 a cell being edited (the composition is over, the cell still edited), H4 nothing
//! composing (the ⌄ is as K0091 has it).
use crate::docx::ext::*;
use crate::docx::scenarios::DocSteps;
use crate::pptx::ext::*;
use crate::xlsx::ext::*;
use crate::xlsx::scenarios::SheetSteps;
use crate::shell::keyboard_toolbar::{KeyboardKind, SoftKeyboard, Target};
use crate::shell::scenario::{Scenario, button_probe as buttons};
use ibis_doc_model::{Block, FlowPos};
use ibis_formats::Kind;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::kbd_hide_commits_ime_h1_the_button_commits_the_composition_in_a_document", h1_document),
        ("shell::kbd_hide_commits_ime_h2_a_tap_afterwards_doubles_nothing", h2_tap_afterwards),
        ("shell::kbd_hide_commits_ime_h3_a_cell_being_edited_keeps_its_edit_and_loses_the_composition", h3_cell),
        ("shell::kbd_hide_commits_ime_h4_nothing_composing_the_button_is_as_it_was", h4_nothing_composing),
        ("shell::kbd_hide_commits_ime_h5_a_text_box_being_edited_keeps_its_edit_and_loses_the_composition", h5_text_box),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

const PHONE_KEYBOARD: SoftKeyboard = SoftKeyboard { height: 300., kind: KeyboardKind::Docked, duration: 0.25 };
const TEXT: &str = "あいうえお";

fn screen(s: &mut Scenario) {
    s.size(390., 700.);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        e.soft_keyboard = PHONE_KEYBOARD;
        cx.notify();
    });
    s.settle();
}

/// A new document made as the user makes one (a blank one loaded directly has no paragraph), the caret in its first paragraph.
fn new_document() -> Scenario {
    let mut s = Scenario::start_view();
    s.menu("file.new.document");
    s.key("cmd-8");
    screen(&mut s);
    let (id, _) = s.paragraph(0);
    s.click_text(FlowPos::new(id, 0));
    s
}

fn paragraph_texts(s: &mut Scenario) -> Vec<String> {
    s.with(|e, _| e.flow().expect("a document").body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect())
}

fn composing(s: &mut Scenario) -> bool {
    s.update(|e, window, cx| e.composing_for_scenario(window, cx))
}

fn press_hide(s: &mut Scenario) {
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::click(s, "kbd-hide");
    s.settle();
}

/// H1: 「あいうえお」 composing (underlined), the ⌄: the text is in the paragraph once, nothing is composing, the caret is after it, the keyboard is closed.
fn h1_document() {
    let mut s = new_document();
    s.ime_compose(TEXT);
    assert!(composing(&mut s), "setup: the text is being composed");
    assert!(!paragraph_texts(&mut s).iter().any(|t| t.contains(TEXT)), "setup: the composition is not in the text yet");
    press_hide(&mut s);
    let texts = paragraph_texts(&mut s);
    assert_eq!(texts.iter().map(|t| t.matches(TEXT).count()).sum::<usize>(), 1, "the composition is in the text once: {texts:?}");
    assert!(!composing(&mut s), "and nothing is composing");
    let caret = s.with(|e, _| e.text_selection().map(|(_, c)| c)).expect("the caret is kept");
    let (id, _) = s.paragraph(0);
    assert_eq!(caret, FlowPos::new(id, TEXT.chars().count()), "the caret is after the text");
    assert!(!s.with(|e, _| e.soft_keyboard.visible()), "the keyboard is closed");
}

/// H2: after that a tap on the text (the keyboard comes back) changes nothing: still once, nothing composing; typing goes on after it.
fn h2_tap_afterwards() {
    let mut s = new_document();
    s.ime_compose(TEXT);
    press_hide(&mut s);
    let (id, _) = s.paragraph(0);
    let (page, x, y, h) = s.update(|e, _, cx| {
        let lines = e.flow_lines();
        let (page, x, y, h) = e.caret_box(&lines, FlowPos::new(id, 2)).expect("laid out");
        e.scroll_into_view(page, y, h, cx);
        (page, x, y, h)
    });
    let (origin, k) = s.with(|e, _| e.page_origin(page)).expect("the page is placed");
    let at = gpui_kit::point(gpui_kit::px(f32::from(origin.x) + x * k + 0.5), gpui_kit::px(f32::from(origin.y) + (y + h / 2.0) * k));
    let t = |phase| gpui_kit::PlatformInput::Touch(gpui_kit::TouchEvent { id: gpui_kit::TouchId(1), phase, position: at, predicted_position: None, force: None });
    s.dispatch(vec![t(gpui_kit::TouchPhase::Started)]);
    s.advance_ms(60);
    s.dispatch(vec![t(gpui_kit::TouchPhase::Ended)]);
    s.settle();
    let texts = paragraph_texts(&mut s);
    assert_eq!(texts.iter().map(|t| t.matches(TEXT).count()).sum::<usize>(), 1, "still once after the tap: {texts:?}");
    assert!(!composing(&mut s), "nothing composing after the tap");
    // Typing goes on at the tap (here after the second character): one more character, the five still there once.
    s.ime("", "か");
    let texts = paragraph_texts(&mut s);
    assert_eq!(texts, vec!["あいかうえお".to_string()], "one more character goes in where the tap was: {texts:?}");
}

/// H3: in a cell being typed into, the composition is put in the cell's edit and is over after the ⌄; the cell is still being edited (K0064: nothing is
/// entered; K0091: the edit stays).
fn h3_cell() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    screen(&mut s);
    s.click_cell("K20");
    s.ime_compose("\u{3042}\u{3044}\u{3046}");
    assert!(composing(&mut s), "setup: composing in the cell");
    press_hide(&mut s);
    assert!(s.with(|e, _| e.cell_edit().is_some()), "the cell is still being edited");
    assert!(!composing(&mut s), "nothing is composing");
    assert_eq!(s.with(|e, _| e.keyboard_target()), Target::SheetValue, "the edit is the cell's");
}

/// H5: in a text box being edited, the same.
fn h5_text_box() {
    let mut s = Scenario::new_document(Kind::Presentation);
    screen(&mut s);
    s.update(|e, window, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(914_400, 914_400, 3_657_600, 914_400);
        e.execute(vec![ibis_doc_model::Command::InsertShape { slide: e.current_slide, shapes: vec![(0, ibis_doc_model::Shape::text_box(id, frame, "box"))] }], cx);
        e.begin_text_edit(id, false, false, window, cx);
    });
    s.settle();
    s.ime_compose("にほん");
    assert!(composing(&mut s), "setup: composing in the text box");
    press_hide(&mut s);
    assert!(!composing(&mut s), "the composition is over");
    assert!(s.with(|e, _| e.text_editing().is_some()), "the text box is still being edited");
}

/// H4: with nothing composing the ⌄ is what K0091 made it: the keyboard closes, the text and the caret stay.
fn h4_nothing_composing() {
    let mut s = new_document();
    s.ime("", "か");
    let before = paragraph_texts(&mut s);
    press_hide(&mut s);
    assert_eq!(paragraph_texts(&mut s), before, "the text is as it was");
    assert!(!s.with(|e, _| e.soft_keyboard.visible()), "the keyboard is closed");
}
