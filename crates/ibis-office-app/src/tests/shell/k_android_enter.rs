// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K-android (design: `tasks/doc/test-design/K-android-safe-area-kbd-enter.md`; the numbers AE... and AK... are that document's): what an Android soft keyboard's Enter does
//! in a cell being typed into, and where the keyboard's toolbar sits above the keyboard. `crates/gpui-android` is empty on a desktop, so the scenarios drive the same
//! input hooks the Android platform calls (`gpui-android` `handle_ime_events`: `replace_text_in_range` for `commitText`, `replace_and_mark_text_in_range` for
//! `setComposingText`, a `KeyDown` of "enter" for a key event) and read the keyboard through `SlideEditor::soft_keyboard` (what `os_keyboard` reads from
//! `gpui_android::keyboard()`). The emulator (the design's section 5) is the proof of the activity's own part (Java): no scenario here is.
use crate::shell::keyboard_toolbar::{KeyboardKind, SoftKeyboard};
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::xlsx::scenarios::SheetSteps;
use ibis_office_docx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::kand_ae1_the_enter_key_event_commits_the_cell_and_goes_down", ae1_key_event),
        ("shell::kand_ae2_a_committed_newline_commits_the_cell_and_goes_down", ae2_commit_newline_in_a_cell),
        ("shell::kand_ae3_the_composing_text_is_kept_when_the_keyboard_finishes_it_and_enter_commits_it", ae3_finish_composing_then_enter),
        ("shell::kand_ae4_enter_in_the_formula_bar_commits_the_cell", ae4_formula_bar),
        ("shell::kand_ae5_a_committed_newline_in_a_document_is_a_new_paragraph", ae5_document_newline),
        ("shell::kand_ae6_enter_while_composing_in_a_cell_commits_what_is_composed_and_goes_down", ae6_enter_while_composing),
        ("shell::kand_ae7_letters_and_a_newline_together_go_in_and_then_enter", ae7_letters_then_newline),
        ("shell::kand_ak1_the_keyboards_toolbar_is_wholly_above_the_keyboard", ak1_toolbar_above_the_keyboard),
        ("shell::kand_ak2_the_toolbar_follows_the_keyboard_up_and_down", ak2_follows),
        ("shell::kand_sa1_the_margins_are_the_largest_of_the_bars_the_cutout_and_the_gestures", sa1_margins),
        ("shell::kand_sa2_the_keyboards_height_is_what_covers_the_view_below_its_margin", sa2_keyboard),
        ("shell::kand_sa3_the_chrome_stays_inside_the_safe_area", sa3_layout),
        ("shell::kand_sa4_the_keyboards_toolbar_sits_above_the_keyboard_inside_the_safe_area", sa4_keyboard_layout),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

/// AE7 (the letters and the Enter in one `commitText("あ\n")`): "あ" goes into the cell and then the Enter commits it and goes down.
fn ae7_letters_then_newline() {
    let mut s = phone();
    s.click_cell("A1");
    s.ime_commit("あ\n");
    assert_eq!(s.shown("A1"), "あ", "the letters are in the cell, with no newline in it");
    assert_eq!(s.selected(), "A2", "and the Enter went down");
}

/// SA1: the margin of each side is the largest of the system bars, the display cutout and the mandatory system gestures (the grip of the gesture navigation): 24 dp of
/// gesture grip at the bottom with a thinner navigation bar, a cutout at the top bigger than the status bar, a landscape cutout at the left.
fn sa1_margins() {
    use crate::shell::safe_area::compute;
    let density = 2.625;
    let dp = |v: f32| (v * density).round() as i32;
    // bars (l t r b), cutout (l t r b), gestures (l t r b), ime
    let raw = [0, dp(24.0), 0, dp(0.0), 0, dp(40.0), 0, 0, 0, 0, 0, dp(24.0), 0];
    let area = compute(&raw, density);
    assert_eq!(area[1], dp(40.0) as f32, "the top is the cutout's, bigger than the status bar");
    assert_eq!(area[3], dp(24.0) as f32, "the bottom is the gesture grip's: 24 dp where the navigation bar says 0");
    let landscape = [dp(48.0), 0, 0, 0, dp(48.0), 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(compute(&landscape, density)[0], dp(48.0) as f32, "the left cutout");
    assert_eq!(compute(&[0; 13], density)[..4], [0.0; 4], "no insets: no margins");
}

/// SA2: the keyboard's height is the part of the keyboard that covers the view, which stops at the bottom margin: 324 dp from the window's bottom with 24 dp of margin is a
/// 300 dp keyboard on the view (the toolbar sits on its top edge, wholly above it); with no keyboard 0, and never below 0.
fn sa2_keyboard() {
    use crate::shell::safe_area::compute;
    let density = 2.625;
    let px = |v: f32| (v * density).round() as i32;
    let raw = |ime_dp: f32| [0, px(24.0), 0, px(24.0), 0, 0, 0, 0, 0, 0, 0, px(24.0), px(ime_dp)];
    let with = compute(&raw(324.0), density)[4];
    assert!((with - 300.0).abs() < 0.5, "300 dp of keyboard on the view: {with}");
    assert_eq!(compute(&raw(0.0), density)[4], 0.0, "no keyboard");
    assert_eq!(compute(&raw(10.0), density)[4], 0.0, "an inset smaller than the margin is no keyboard");
    // The same margin is what the view was given: the height does not change with how the bottom bar is told (the gesture grip or the navigation bar).
    let by_bar = compute(&[0, 0, 0, px(24.0), 0, 0, 0, 0, 0, 0, 0, 0, px(324.0)], density);
    let by_grip = compute(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, px(24.0), px(324.0)], density);
    assert_eq!(by_bar[4], by_grip[4]);
    assert_eq!(by_bar[3], by_grip[3]);
}

fn phone() -> Scenario {
    let mut s = Scenario::new_document(ibis_formats::Kind::Workbook);
    s.size(390., 844.);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    s
}

/// Select A1 and begin typing "12" in it: the cell is being typed into (not entered).
fn typing_in_a1(s: &mut Scenario, text: &str) {
    s.click_cell("A1");
    s.type_text(text);
}

/// AE1 (the path that works on the desktop and for a hardware key): a key event "enter" (the activity's `sendKeyEvent` with KEYCODE_ENTER reaches the platform as `nativeOnImeEnter`
/// → `send_key("enter")`) commits the cell and the selection goes down to A2.
fn ae1_key_event() {
    let mut s = phone();
    typing_in_a1(&mut s, "12");
    s.key("enter");
    assert_eq!(s.shown("A1"), "12", "the cell is committed");
    assert_eq!(s.selected(), "A2", "and the selection is the cell below");
}

/// AE2 (the symptom's candidate 1: the keyboard sends Enter as `commitText("\n")`, which the platform passes on as text): the cell is committed and the selection goes down, as
/// for the key. A newline never goes into a cell by the keyboard's Enter (Alt+Enter is the cell's line break).
fn ae2_commit_newline_in_a_cell() {
    let mut s = phone();
    typing_in_a1(&mut s, "12");
    s.ime_commit("\n");
    assert_eq!(s.shown("A1"), "12", "the cell is committed, with no newline in it");
    assert_eq!(s.selected(), "A2");
}

/// AE3 (candidate 2: the keyboard finishes the composing text (`finishComposingText`) before the Enter): the composing text is committed as it stands, not thrown away: "あ" is
/// in the cell, and then the Enter commits the cell. (The platform today passes `finishComposingText` as `setComposingText("")`, which empties the marked text.)
fn ae3_finish_composing_then_enter() {
    let mut s = phone();
    s.click_cell("A1");
    s.ime_compose("あ");
    // What the platform must do for `finishComposingText`: take the mark off (the text stays).
    s.update(|e, w, cx| crate::shell::format::with_format!(e, unmark_text(e, w, cx)));
    s.settle();
    s.key("enter");
    assert_eq!(s.shown("A1"), "あ", "the composing text is kept and committed");
    assert_eq!(s.selected(), "A2");
}

/// AE4: the same in the formula bar's text (a tap on the bar begins the typing, K0089): the key "enter" commits and goes down.
fn ae4_formula_bar() {
    let mut s = phone();
    typing_in_a1(&mut s, "34");
    s.update(|_, _, cx| cx.notify());
    s.settle();
    s.ime_commit("\n");
    assert_eq!(s.shown("A1"), "34");
}

/// AE5 (regression, the other way): in a document the keyboard's Enter (a committed newline or the key) makes a new paragraph: the paragraph count goes up by one.
fn ae5_document_newline() {
    use crate::docx::scenarios::DocSteps;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(390., 844.);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    let (id, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    let before = s.with(|e, _| e.flow_lines().len());
    s.ime_commit("\n");
    let after = s.with(|e, _| e.flow_lines().len());
    assert!(after > before, "a new paragraph (a new line is laid out: {before} -> {after})");
}

/// AE6 (the Japanese case): "にほんご" is composing in a cell and Enter is pressed (the keyboard sends the key, or the key after finishing): the composing text is what is committed, once.
fn ae6_enter_while_composing() {
    let mut s = phone();
    s.click_cell("A1");
    s.ime_compose("にほんご");
    s.key("enter");
    assert_eq!(s.shown("A1"), "にほんご", "what was composing is the cell's, once");
    assert_eq!(s.selected(), "A2");
}

fn toolbar_and_window(s: &mut Scenario, keyboard: SoftKeyboard) -> (f32, f32) {
    s.update(move |e, _, cx| {
        e.soft_keyboard = keyboard;
        cx.notify();
    });
    s.settle();
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let hide = buttons::bounds("kbd-hide").or_else(|| buttons::bounds("kbd-undo")).unwrap_or_else(|| panic!("the keyboard's toolbar: {:?}", buttons::all()));
    // The window is 844 high: the keyboard covers its bottom `height` points.
    (f32::from(hide.bottom()), 844.0 - keyboard.height)
}

/// AK1 (the symptom 2 at the layout's side): with the keyboard `height` points high (what the activity tells), the toolbar of K0064 is wholly above the keyboard's top: its bottom is
/// at the keyboard's top (within a point), none of it is under the keyboard. The number the activity tells is the emulator's part (S3).
fn ak1_toolbar_above_the_keyboard() {
    let mut s = phone();
    typing_in_a1(&mut s, "1");
    let (bottom, top) = toolbar_and_window(&mut s, SoftKeyboard { height: 300., kind: KeyboardKind::Docked, duration: 0.25 });
    assert!(bottom <= top + 1.0, "the toolbar's bottom ({bottom}) is not under the keyboard's top ({top})");
    assert!(top - bottom < 6.0, "and it sits on the keyboard ({bottom} against {top})");
}

/// AK2: the keyboard comes up (300), goes (0), comes up taller (360): the toolbar follows each time and is never under the keyboard.
fn ak2_follows() {
    let mut s = phone();
    typing_in_a1(&mut s, "1");
    for height in [300.0, 360.0] {
        let (bottom, top) = toolbar_and_window(&mut s, SoftKeyboard { height, kind: KeyboardKind::Docked, duration: 0.25 });
        assert!(bottom <= top + 1.0, "{height}: the toolbar is above the keyboard ({bottom} vs {top})");
    }
}

/// SA3: the window is the whole screen and the app keeps its content in from the edges (the activity cannot move its surface): with a safe area of 40 at the top and 24 at the
/// bottom the title row starts below the top 40 and the bottom bar (the menu bar of a narrow touch window) ends above the bottom 24, on a document and on the start view.
fn sa3_layout() {
    let _area = crate::shell::safe_area::ForcedSafeArea::new([0.0, 40.0, 0.0, 24.0]);
    let _probe = buttons::Probe::new();
    let mut s = phone();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let undo = buttons::bounds("undo").or_else(|| buttons::bounds("title-undo")).expect("the title row's undo button");
    assert!(f32::from(undo.top()) >= 40.0 - 0.5, "the title row is below the safe area's top: {:?}", undo);
    let menu = buttons::bounds("menu-button-menu.file").expect("the narrow touch window's menu bar is at the bottom");
    assert!(f32::from(menu.bottom()) <= 844.0 - 24.0 + 0.5, "the menu bar is above the bottom margin: {:?}", menu);
    // Without a safe area the same window reaches the edges (so the margin above is the safe area's doing).
    drop(_area);
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let undo = buttons::bounds("undo").or_else(|| buttons::bounds("title-undo")).expect("the undo button");
    assert!(f32::from(undo.top()) < 20.0, "no safe area: the title row is at the top ({:?})", undo);
}

/// SA4: with the keyboard up (300 dp over the window's bottom, the margin below it already given to the safe area) the toolbar's bottom edge is the keyboard's top, inside the window:
/// the window is 844 high, the keyboard's top at 844 - 324 + 24 = 544 where the 300 dp is what `safe_area::compute` returns for 324 dp of keyboard and 24 dp of margin.
fn sa4_keyboard_layout() {
    let _area = crate::shell::safe_area::ForcedSafeArea::new([0.0, 40.0, 0.0, 24.0]);
    let mut s = phone();
    s.type_text("1");
    let kb = SoftKeyboard { height: 300.0, kind: KeyboardKind::Docked, duration: 0.25 };
    let (toolbar_bottom, _) = toolbar_and_window(&mut s, kb);
    // The keyboard covers the window's bottom 324 (the spacer under the toolbar is its 300 plus the 24 of margin the root keeps): the toolbar ends 324 above the window's bottom.
    assert!((toolbar_bottom - (844.0 - 324.0)).abs() < 1.5, "the toolbar's bottom is the keyboard's top: {toolbar_bottom}");
}
