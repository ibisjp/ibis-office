// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K-keyboard-scroll-into-view (iOS and Android; design: `tasks/doc/test-design/K-keyboard-scroll-into-view.md`; the numbers KS1... are that document's): the soft
//! keyboard takes the window's bottom (K0064: the body is shortened by its height and the keyboard's toolbar), and what is being typed into (the caret, the cell, the
//! text of a text box, the end of the composing text, the clause being chosen) must stay in the part of the window that is left, scrolled sideways too.
//! The scenarios put the keyboard up the way the platform does (`SlideEditor::soft_keyboard` set, as `keyboard_toolbar::os_keyboard` reads it, then the window drawn) and
//! read where things are drawn with the format-neutral input hooks (`bounds_for_range`, the IME candidate window's way), so the same helpers serve the three screens.
//! The platforms' own keyboard frames (iOS `keyboardWillShow`) are for the simulators (the design's section 5): no scenario here is proof of them.
use crate::docx::scenarios::DocSteps;
use crate::shell::keyboard_toolbar::{KeyboardKind, SoftKeyboard};
use crate::shell::scenario::Scenario;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use ibis_office_docx::ext::*;
use ibis_office_xlsx::ext::*;
use ibis_office_xlsx::sheet_ui::SheetUi;

const PHONE_KEYBOARD: SoftKeyboard = SoftKeyboard { height: 300., kind: KeyboardKind::Docked, duration: 0.25 };
/// The room the design asks for between what must be seen and the edge of the part of the window that is left (points).
const MARGIN: f32 = 12.0;
/// The most the design lets that room be (the scroll is the least it takes plus the margin, not a page): the existing document scroll's 48 pt line plus a little.
const MOST_ROOM: f32 = 64.0;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::kks_ks1_docx_a_caret_under_the_keyboard_is_scrolled_into_view_by_the_least", ks1_docx_hidden_caret),
        ("shell::kks_ks2_docx_a_caret_already_in_view_does_not_move_the_page", ks2_docx_visible_caret),
        ("shell::kks_ks3_docx_the_scroll_is_vertical_only_when_the_caret_is_in_view_sideways", ks3_docx_no_sideways_move),
        ("shell::kks_ks4_docx_a_caret_in_a_text_box_is_scrolled_into_view", ks4_docx_text_box),
        ("shell::kks_ks5_docx_the_end_of_the_composing_text_stays_in_view_both_ways", ks5_docx_marked_end),
        ("shell::kks_ks6_docx_the_clause_being_chosen_is_in_view_both_ways", ks6_docx_thick_clause),
        ("shell::kks_ks7_xlsx_a_cell_under_the_keyboard_is_scrolled_into_view_by_the_least", ks7_xlsx_cell_down),
        ("shell::kks_ks8_xlsx_a_cell_half_out_at_the_right_is_scrolled_into_view_sideways", ks8_xlsx_cell_right),
        ("shell::kks_ks9_xlsx_the_caret_in_a_cell_being_typed_into_stays_in_view_sideways", ks9_xlsx_cell_edit),
        ("shell::kks_ks10_xlsx_a_text_box_on_the_sheet_is_scrolled_into_view", ks10_xlsx_text_box),
        ("shell::kks_ks11_pptx_a_text_box_in_a_zoomed_slide_is_scrolled_into_view", ks11_pptx_zoomed),
        ("shell::kks_ks12_pptx_a_slide_that_fits_keeps_its_text_in_view", ks12_pptx_fit),
        ("shell::kks_ks13_closing_the_keyboard_does_not_move_the_page_back", ks13_closing_keeps_the_scroll),
        ("shell::kks_ks14_a_floating_or_hardware_keyboard_moves_nothing", ks14_no_room_taken),
        ("shell::kks_ks15_the_keyboard_growing_scrolls_again_only_when_the_caret_is_hidden", ks15_growing),
        ("shell::kks_ks16_a_wide_window_with_a_docked_keyboard_does_the_same", ks16_ipad),
        ("shell::kks_ks17_typing_in_the_formula_bar_does_not_scroll_the_body_by_itself", ks17_formula_bar_does_not_scroll),
        ("shell::kks_ks18_typing_in_the_formula_bar_shows_the_current_cell_once", ks18_formula_bar_shows_the_cell_once),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

// ----- helpers -----

fn touch_window(path: &str, w: f32, h: f32) -> Scenario {
    let mut s = Scenario::open(path);
    s.size(w, h);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    s
}

fn keyboard(s: &mut Scenario, k: SoftKeyboard) {
    s.update(move |e, _, cx| {
        e.soft_keyboard = k;
        cx.notify();
    });
    // Two frames: the one that takes the height off the body, the one the caret is looked for in.
    s.settle();
    s.update(|_, _, cx| cx.notify());
    s.settle();
}

/// The part of the window the body has now (points, the window's coordinates).
fn view(s: &mut Scenario) -> Bounds<Pixels> {
    s.with(|e, _| e.workspace_bounds).expect("the body is laid out")
}

/// Where the characters `range` (UTF-16, in the text being typed into) are drawn, by the input hook the IME's candidate window uses.
fn range_rect(s: &mut Scenario, range: std::ops::Range<usize>) -> Bounds<Pixels> {
    s.update(move |e, w, cx| crate::shell::format::with_format!(e, bounds_for_range(e, range, Default::default(), w, cx))).expect("the characters are on screen")
}

/// The caret: the box of the character before it (or after it at the start), its right edge being the caret's x for the purposes here.
fn caret_rect(s: &mut Scenario) -> Bounds<Pixels> {
    let sel = s.update(|e, w, cx| crate::shell::format::with_format!(e, selected_text_range(e, false, w, cx))).expect("a caret");
    let at = sel.range.end;
    if at > 0 { range_rect(s, at - 1..at) } else { range_rect(s, 0..1) }
}

/// Where the caret is drawn (the box's left and top, points): the same before and after when the page did not move.
fn caret_at(s: &mut Scenario) -> (f32, f32) {
    let c = caret_rect(s);
    (f32::from(c.left()), f32::from(c.top()))
}

fn marked(s: &mut Scenario) -> std::ops::Range<usize> {
    s.update(|e, w, cx| crate::shell::format::with_format!(e, marked_text_range(e, w, cx))).expect("text is being composed")
}

/// `inner` is in `view` with at least `margin` points to every edge (the horizontal ones too when `sideways`).
fn assert_in(inner: Bounds<Pixels>, view: Bounds<Pixels>, margin: f32, sideways: bool, what: &str) {
    let (t, b) = (f32::from(inner.top()), f32::from(inner.bottom()));
    let (vt, vb) = (f32::from(view.top()), f32::from(view.bottom()));
    assert!(t >= vt + margin - 0.5 && b <= vb - margin + 0.5, "{what}: vertically in the view ({t}..{b} in {vt}..{vb}, margin {margin})");
    if sideways {
        let (l, r) = (f32::from(inner.left()), f32::from(inner.right()));
        let (vl, vr) = (f32::from(view.left()), f32::from(view.right()));
        assert!(l >= vl + margin - 0.5 && r <= vr - margin + 0.5, "{what}: sideways in the view ({l}..{r} in {vl}..{vr}, margin {margin})");
    }
}

/// The room left between the bottom of `inner` and the bottom of `view`: the least scroll leaves about the margin, not a screenful.
fn room_below(inner: Bounds<Pixels>, view: Bounds<Pixels>) -> f32 {
    f32::from(view.bottom()) - f32::from(inner.bottom())
}

/// A document opened in a 390 x 844 window with a finger, no keyboard; the caret put in the first paragraph whose caret is under where the keyboard will be (the
/// band y 520..700: the keyboard (300) and its toolbar (44) take 844 - 344 = 500 and up; the window's bottom bars are not there with the keyboard).
fn docx_with_caret_in_the_hidden_band() -> Scenario {
    let mut s = touch_window("test/editing/docx-report.docx", 390., 844.);
    for p in 0..60 {
        let (id, _) = s.paragraph(p);
        s.click_text(ibis_doc_model::FlowPos::new(id, 0));
        let y = f32::from(caret_rect(&mut s).bottom());
        if (520.0..700.0).contains(&y) {
            return s;
        }
    }
    panic!("no paragraph of the test document has its caret in the band the keyboard will cover");
}

// ----- the document -----

/// KS1 (the request): the caret is in the lower part of the window, the keyboard comes up and covers it: the page is scrolled so the caret is in the part that is left,
/// by the least (the room below it is the margin and not a screenful).
fn ks1_docx_hidden_caret() {
    let mut s = docx_with_caret_in_the_hidden_band();
    keyboard(&mut s, PHONE_KEYBOARD);
    let (v, c) = (view(&mut s), caret_rect(&mut s));
    assert_in(c, v, MARGIN, false, "the caret");
    assert!(room_below(c, v) <= MOST_ROOM, "the page was scrolled by the least, not to the top: {} points below the caret", room_below(c, v));
}

/// KS2: the caret is in the upper part (above where the keyboard covers): the page does not move (the scroll offset is the same, exactly).
fn ks2_docx_visible_caret() {
    let mut s = touch_window("test/editing/docx-report.docx", 390., 844.);
    let (id, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    let before = caret_at(&mut s);
    keyboard(&mut s, PHONE_KEYBOARD);
    assert_eq!(caret_at(&mut s), before, "the caret was in view: the page stays (the caret is drawn where it was)");
}

/// KS3: the page is at 200% (wider than the window); the caret is in view sideways but under the keyboard: only the vertical offset changes.
fn ks3_docx_no_sideways_move() {
    let mut s = docx_with_caret_in_the_hidden_band();
    s.update(|e, w, cx| e.set_zoom(Some(2.0), w, cx));
    s.settle();
    let (id, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    // (Scrolled down so that the caret is in the hidden band: the scenario's way of putting the page where the user left it.)
    let x_before = caret_at(&mut s).0;
    keyboard(&mut s, PHONE_KEYBOARD);
    let x_after = caret_at(&mut s).0;
    assert!((x_after - x_before).abs() < 0.5, "the caret was in view sideways: the page is not moved sideways ({x_before} -> {x_after})");
    let (v, c) = (view(&mut s), caret_rect(&mut s));
    assert_in(c, v, MARGIN, true, "the caret");
}

/// KS4: the caret is in a text box of the document (a shape being edited), under the keyboard: scrolled into view.
fn ks4_docx_text_box() {
    let mut s = docx_with_caret_in_the_hidden_band();
    s.menu("insert.text-box");
    s.settle();
    s.type_text("abc");
    keyboard(&mut s, PHONE_KEYBOARD);
    let (v, c) = (view(&mut s), caret_rect(&mut s));
    assert_in(c, v, MARGIN, false, "the caret in the text box");
}

/// KS5: with the keyboard up, the composing text grows to the right edge and past it (at 200%): after every step the right edge of the composing text's last character
/// is in the view, sideways and vertically (a wrapped line moves it down).
fn ks5_docx_marked_end() {
    let mut s = docx_with_caret_in_the_hidden_band();
    s.update(|e, w, cx| e.set_zoom(Some(2.0), w, cx));
    s.settle();
    keyboard(&mut s, PHONE_KEYBOARD);
    let mut composing = String::new();
    for step in 0..24 {
        composing.push_str("きょう");
        s.ime_compose(&composing);
        let m = marked(&mut s);
        let last = range_rect(&mut s, m.end - 1..m.end);
        let v = view(&mut s);
        assert_in(last, v, MARGIN, true, &format!("the last character of the composing text, step {step}"));
    }
}

/// KS6: a long composing text whose current clause (the thick underline) is in the middle: the whole clause is in the view, sideways and vertically; where the clause
/// is wider than the view its left edge is (the design's rule).
fn ks6_docx_thick_clause() {
    let mut s = docx_with_caret_in_the_hidden_band();
    s.update(|e, w, cx| e.set_zoom(Some(2.0), w, cx));
    s.settle();
    keyboard(&mut s, PHONE_KEYBOARD);
    let text = "きょうはとてもいいてんきですのでさんぽにいきましょう";
    s.ime_compose(text);
    let n = text.encode_utf16().count();
    let thick = n / 3..n / 3 + 6;
    let clauses = vec![
        gpui_kit::ImeClause { range: 0..thick.start, thick: false, line: gpui_kit::ImeLine::Solid },
        gpui_kit::ImeClause { range: thick.clone(), thick: true, line: gpui_kit::ImeLine::Solid },
        gpui_kit::ImeClause { range: thick.end..n, thick: false, line: gpui_kit::ImeLine::Solid },
    ];
    s.update(move |e, w, cx| crate::shell::format::with_format!(e, set_ime_clauses(e, clauses, w, cx)));
    s.settle();
    let m = marked(&mut s);
    let clause = range_rect(&mut s, m.start + thick.start..m.start + thick.end);
    let v = view(&mut s);
    assert_in(clause, v, MARGIN, true, "the clause being chosen");
}

// ----- the workbook -----

/// A workbook in a 390 x 844 window; the cell found in the hidden band (y 520..700 of the cells' centres) is tapped.
fn xlsx_with_a_cell_in_the_hidden_band() -> (Scenario, u32) {
    let mut s = touch_window("test/editing/xlsx-sales.xlsx", 390., 844.);
    for row in 0..80 {
        if let Some(c) = s.with(|e, _| e.cell_center(0, row)) {
            if (520.0..700.0).contains(&f32::from(c.y)) {
                // (The cell is made the current one directly: a mouse click in a window that has been touched is the page's finger and does not select it.)
                s.update(move |e, _, cx| {
                    e.set_cell(0, row);
                    cx.notify();
                });
                s.settle();
                assert_eq!(s.with(|e, _| e.current_cell()), Some((0, row)), "setup: the cell is the current one");
                return (s, row);
            }
        }
    }
    panic!("no row of the test workbook is in the band the keyboard will cover");
}

/// KS7 (the request): a cell low on the screen is tapped, the keyboard comes up: the current cell's rectangle is in the part that is left, clear of the headers,
/// scrolled by the least.
fn ks7_xlsx_cell_down() {
    let (mut s, row) = xlsx_with_a_cell_in_the_hidden_band();
    keyboard(&mut s, PHONE_KEYBOARD);
    let c = s.with(move |e, _| e.cell_center(0, row)).expect("the cell is on screen");
    let v = view(&mut s);
    // A cell is about 20 points high: its middle is that far plus the margin from the bottom.
    assert!(f32::from(c.y) + 10.0 <= f32::from(v.bottom()) - MARGIN + 0.5, "the current cell is in the view above the keyboard (centre {:?}, view {:?})", c, v);
    assert!(f32::from(v.bottom()) - (f32::from(c.y) + 10.0) <= MOST_ROOM, "by the least");
}

/// KS8: a cell at the right edge, half out of the window, is tapped (the keyboard comes up): the sheet is scrolled sideways so the whole cell is in view.
fn ks8_xlsx_cell_right() {
    let mut s = touch_window("test/editing/xlsx-sales.xlsx", 390., 844.);
    // The first column whose cell has its right edge beyond the window's but whose middle is inside it: half out.
    let mut found = None;
    for column in 0..40u32 {
        let pair = s.with(move |e, _| (e.cell_corner(column, 3), e.cell_center(column, 3)));
        let (Some(corner), Some(centre)) = pair else { continue };
        // (`cell_corner` is the cell's bottom right, `cell_center` its middle.)
        if f32::from(centre.x) < 385.0 && f32::from(corner.x) > 392.0 {
            found = Some(column);
            break;
        }
    }
    let column = found.expect("a column is half out at the right edge of the window");
    s.update(move |e, _, cx| {
        e.set_cell(column, 3);
        cx.notify();
    });
    s.settle();
    keyboard(&mut s, PHONE_KEYBOARD);
    let (corner, centre) = s.with(move |e, _| (e.cell_corner(column, 3).expect("the cell is on screen"), e.cell_center(column, 3).expect("the cell is on screen")));
    let v = view(&mut s);
    let left = f32::from(centre.x) * 2.0 - f32::from(corner.x);
    assert!(f32::from(corner.x) <= f32::from(v.right()) - MARGIN + 0.5, "the cell's right edge is in the view ({} in {v:?})", f32::from(corner.x));
    assert!(left >= f32::from(v.left()), "and its left edge ({left} in {v:?})");
}

/// KS9: a cell is being typed into and the text runs right (a long entry): the caret of the cell's editor is in the view sideways, step by step, with the keyboard up.
fn ks9_xlsx_cell_edit() {
    let (mut s, _) = xlsx_with_a_cell_in_the_hidden_band();
    keyboard(&mut s, PHONE_KEYBOARD);
    for step in 0..12 {
        s.type_text("0123456789");
        let c = caret_rect(&mut s);
        let v = view(&mut s);
        assert_in(c, v, MARGIN, true, &format!("the caret of the cell being typed into, step {step}"));
    }
}

/// KS10: a text box on the sheet (a shape with text, edited) low on the screen: scrolled into view with the keyboard up.
fn ks10_xlsx_text_box() {
    let (mut s, _) = xlsx_with_a_cell_in_the_hidden_band();
    s.update(|e, w, cx| {
        e.insert_sheet_textbox(w, cx);
    });
    s.settle();
    s.type_text("abc");
    keyboard(&mut s, PHONE_KEYBOARD);
    let (v, c) = (view(&mut s), caret_rect(&mut s));
    assert_in(c, v, MARGIN, false, "the caret in the text box on the sheet");
}

// ----- the presentation -----

/// A new presentation with a text box low on the slide being edited, `zoom` (None: the fit), the window `h` high; the page scrolled (when it can) so that the box's caret is in
/// the band the keyboard will cover.
fn pptx_with_a_text_box(zoom: Option<f32>, h: f32) -> Scenario {
    let mut s = Scenario::new_document(ibis_formats::Kind::Presentation);
    s.size(390., h);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    s.update(move |e, w, cx| e.set_zoom(zoom, w, cx));
    s.settle();
    s.update(|e, window, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(914_400, 3_200_000, 3_657_600, 914_400);
        e.execute(vec![ibis_doc_model::Command::InsertShape { slide: e.current_slide, shapes: vec![(0, ibis_doc_model::Shape::text_box(id, frame, "box"))] }], cx);
        e.begin_text_edit(id, false, false, window, cx);
    });
    s.settle();
    // Under the keyboard: scrolled so that the caret is low in the window where the page can be.
    let y = f32::from(caret_rect(&mut s).bottom());
    let want = (h - 344.0) + 120.0;
    s.update(move |e, _, cx| {
        let o = e.workspace_scroll.offset();
        let max = e.workspace_scroll.max_offset();
        let y_new = (f32::from(o.y) + (want - y)).clamp(-f32::from(max.y).max(0.0), 0.0);
        e.workspace_scroll.set_offset(point(o.x, px(y_new)));
        cx.notify();
    });
    s.settle();
    s
}

/// KS11: the slide is zoomed in (200%) and a text box low on it is being edited under the keyboard: the slide is scrolled so the caret is in view, sideways too.
fn ks11_pptx_zoomed() {
    let mut s = pptx_with_a_text_box(Some(2.0), 844.);
    keyboard(&mut s, PHONE_KEYBOARD);
    let (v, c) = (view(&mut s), caret_rect(&mut s));
    assert_in(c, v, MARGIN, true, "the caret in the zoomed slide's text box");
}

/// KS12: the slide fits the window (the default zoom): with the keyboard up the text of a box low on the slide is in view (the slide may be fitted again to the shorter body).
fn ks12_pptx_fit() {
    let mut s = pptx_with_a_text_box(None, 560.);
    keyboard(&mut s, PHONE_KEYBOARD);
    let (v, c) = (view(&mut s), caret_rect(&mut s));
    assert_in(c, v, MARGIN, false, "the caret in the slide's text box (the slide shrank or was scrolled)");
}

// ----- all three -----

/// KS13: the keyboard is closed (the toolbar's ⌄ or the OS): the page does not move back, the scroll offset is the same as with the keyboard up.
fn ks13_closing_keeps_the_scroll() {
    let mut s = docx_with_caret_in_the_hidden_band();
    keyboard(&mut s, PHONE_KEYBOARD);
    let up = caret_at(&mut s);
    keyboard(&mut s, SoftKeyboard::HIDDEN);
    assert_eq!(caret_at(&mut s), up, "the page stays where it is when the keyboard goes (the caret is drawn where it was)");
}

/// KS14: a floating keyboard (an iPad's, no edge covered) and a hardware keyboard take no room (height 0): nothing is scrolled.
fn ks14_no_room_taken() {
    for kind in [KeyboardKind::Floating, KeyboardKind::Hardware] {
        let mut s = docx_with_caret_in_the_hidden_band();
        let before = caret_at(&mut s);
        keyboard(&mut s, SoftKeyboard { height: 0., kind, duration: 0.25 });
        assert_eq!(caret_at(&mut s), before, "{kind:?}: no room taken, no scroll");
    }
}

/// KS15: the keyboard grows (the emoji keyboard is taller, 300 -> 360): the page is scrolled again only if the caret is now hidden; a caret still in view stays.
fn ks15_growing() {
    let mut s = docx_with_caret_in_the_hidden_band();
    keyboard(&mut s, PHONE_KEYBOARD);
    let before = caret_at(&mut s);
    keyboard(&mut s, SoftKeyboard { height: 360., ..PHONE_KEYBOARD });
    let (v, c) = (view(&mut s), caret_rect(&mut s));
    assert_in(c, v, MARGIN, false, "the caret after the keyboard grew");
    // (The least: the page moved by about the growth, at most.)
    let moved = (caret_at(&mut s).1 - before.1).abs();
    assert!(moved <= 60.0 + MARGIN, "scrolled by about the 60 points the keyboard grew, not more ({moved})");
}

/// KS16: an iPad-wide window (1024 x 768) with a docked keyboard (300): the same as KS1.
fn ks16_ipad() {
    let mut s = touch_window("test/editing/docx-report.docx", 1024., 768.);
    for p in 0..60 {
        let (id, _) = s.paragraph(p);
        s.click_text(ibis_doc_model::FlowPos::new(id, 0));
        let y = f32::from(caret_rect(&mut s).bottom());
        if (440.0..700.0).contains(&y) {
            break;
        }
    }
    keyboard(&mut s, SoftKeyboard { height: 300., kind: KeyboardKind::DockedWithHideKey, duration: 0.25 });
    let (v, c) = (view(&mut s), caret_rect(&mut s));
    assert_in(c, v, MARGIN, false, "the caret");
}

// ----- the formula bar (found on the simulator: the body kept scrolling down by itself) -----

fn tap_at(s: &mut Scenario, at: Point<Pixels>) {
    let t = |phase| PlatformInput::Touch(TouchEvent { id: TouchId(1), phase, position: at, predicted_position: None, force: None });
    s.dispatch(vec![t(TouchPhase::Started)]);
    s.advance_ms(60);
    s.dispatch(vec![t(TouchPhase::Ended)]);
    s.settle();
}

/// The formula bar is tapped (the cell's edit begins in it) and the keyboard comes up; then `n` frames pass.
fn tap_the_bar_and_wait(s: &mut Scenario, n: usize) {
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let bar = s.with(|e, _| e.sheet_state().and_then(|st| st.bar_bounds.get())).expect("the formula bar is drawn").center();
    tap_at(s, bar);
    keyboard(s, PHONE_KEYBOARD);
    for _ in 0..n {
        s.advance_ms(16);
        s.update(|_, _, cx| cx.notify());
        s.settle();
    }
}

fn scroll_offset(s: &mut Scenario) -> (f32, f32) {
    s.with(|e, _| {
        let o = e.workspace_scroll.offset();
        (f32::from(o.x), f32::from(o.y))
    })
}

/// KS17: the caret is in the formula bar (outside the body): the body is not scrolled to bring it in, however many frames pass. With the current cell in view, the scroll
/// is the same after 12 frames as after 1.
fn ks17_formula_bar_does_not_scroll() {
    let mut s = touch_window("test/editing/xlsx-sales.xlsx", 390., 844.);
    s.click_cell("A1");
    tap_the_bar_and_wait(&mut s, 1);
    assert!(s.with(|e, _| e.cell_edit().is_some()), "setup: the tap on the bar began the cell's edit");
    let first = scroll_offset(&mut s);
    for frame in 0..12 {
        s.advance_ms(16);
        s.update(|_, _, cx| cx.notify());
        s.settle();
        assert_eq!(scroll_offset(&mut s), first, "the body did not move by itself (frame {frame})");
    }
}

/// KS18: the current cell is low on the screen and the formula bar is what is typed into: the cell is brought into the part the keyboard leaves, once (the scroll does not go on).
fn ks18_formula_bar_shows_the_cell_once() {
    let (mut s, row) = xlsx_with_a_cell_in_the_hidden_band();
    tap_the_bar_and_wait(&mut s, 2);
    let settled = scroll_offset(&mut s);
    let c = s.with(move |e, _| e.cell_center(0, row)).expect("the cell is on screen");
    let v = view(&mut s);
    assert!(f32::from(c.y) + 10.0 <= f32::from(v.bottom()) - MARGIN + 0.5, "the current cell is in the view above the keyboard (centre {c:?}, view {v:?})");
    for frame in 0..10 {
        s.advance_ms(16);
        s.update(|_, _, cx| cx.notify());
        s.settle();
        assert_eq!(scroll_offset(&mut s), settled, "and the scroll does not go on (frame {frame})");
    }
}
