// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0081: in a narrow window of a touch device (`narrow_layout::bars_at_bottom`: under 720 px and on a touch device) the title bar has the ＜ but not the
//! format's icon (blue, green, orange: K0067's second button) at its right. A wide window and a narrow one with a mouse only are as they were (design:
//! `tasks/doc/test-design/K0081.md`; the numbers NT1... are that document's). The buttons are found by their ids `back-to-start` (the ＜) and
//! `back-to-start-icon` (the icon), as K0067's scenarios find them.
use crate::shell::scenario::{Scenario, button_probe as buttons};
use ibis_formats::Kind;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0081_nt2_a_wide_window_and_a_mouse_keep_the_title_buttons_as_they_were", nt2_unchanged),
        ("shell::k0081_nt3_the_arrow_still_goes_back_in_a_narrow_touch_window", nt3_arrow_works),
        ("shell::k0081_nt1_a_narrow_touch_window_has_the_arrow_and_no_format_icon", nt1_hidden),
        ("shell::k0081_nt4_the_icon_goes_and_comes_back_with_the_window_and_the_finger", nt4_follows),
        ("shell::k0081_nt5_the_arrow_keeps_its_place_and_the_title_row_does_not_fold_for_the_icon", nt5_row),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}


const KINDS: [Kind; 3] = [Kind::Document, Kind::Workbook, Kind::Presentation];

fn screen(kind: Kind, width: f32, touch: bool) -> Scenario {
    let mut s = Scenario::new_document(kind);
    s.size(width, 700.);
    s.update(move |e, _, cx| {
        e.touch_seen = touch;
        cx.notify();
    });
    s.settle();
    s
}

/// The `back-to-start…` buttons drawn now (ids).
fn back_buttons(s: &mut Scenario) -> Vec<String> {
    buttons::clear();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let mut ids: Vec<String> = buttons::all().into_iter().filter(|id| id.starts_with("back-to-start")).collect();
    ids.sort();
    ids
}

/// NT1 (the request): under 720 px on a touch device (719 and 400) the ＜ is there and the format's icon is not, on the document, the workbook and the
/// presentation.
fn nt1_hidden() {
    let _probe = buttons::Probe::new();
    for kind in KINDS {
        for width in [719.0, 400.0] {
            let mut s = screen(kind, width, true);
            assert_eq!(back_buttons(&mut s), ["back-to-start"], "{kind:?} at {width}: the ＜ and no icon");
        }
    }
}

/// NT2 (unchanged; passes now): at 720 and wider with a finger the ＜ and the icon are both there; under 720 with a mouse only there are neither (K0067:
/// a computer has the ＜ once a finger has touched it); a wide window with a mouse has neither either.
fn nt2_unchanged() {
    let _probe = buttons::Probe::new();
    for kind in KINDS {
        for (width, touch, expected) in [
            (720.0, true, vec!["back-to-start", "back-to-start-icon"]),
            (1200.0, true, vec!["back-to-start", "back-to-start-icon"]),
            (719.0, false, vec![]),
            (400.0, false, vec![]),
            (1200.0, false, vec![]),
        ] {
            let mut s = screen(kind, width, touch);
            assert_eq!(back_buttons(&mut s), expected, "{kind:?} at {width} (touch {touch})");
        }
    }
}

/// NT3 (passes now): the ＜ of a narrow touch window works as K0067 made it (on a computer: the start window comes to the front and the file stays
/// open, so the app has two windows).
fn nt3_arrow_works() {
    let _probe = buttons::Probe::new();
    for kind in KINDS {
        let mut s = screen(kind, 719.0, true);
        assert!(back_buttons(&mut s).contains(&"back-to-start".to_string()), "{kind:?}: setup: the ＜");
        buttons::click(&mut s, "back-to-start");
        s.settle();
        assert_eq!(s.window_count(), 2, "{kind:?}: the start window opened");
        assert!(s.with(|e, _| e.has_document), "{kind:?}: and the file stays open in its window");
    }
}

/// NT4: the icon follows the window and the finger: a touch window 1200 → 700 → 1200 has it, has not, has it; a mouse-only window of 700 has neither
/// the ＜ nor the icon, and the first touch gives the ＜ only.
fn nt4_follows() {
    let _probe = buttons::Probe::new();
    let mut s = screen(Kind::Document, 1200.0, true);
    assert_eq!(back_buttons(&mut s), ["back-to-start", "back-to-start-icon"], "1200");
    s.size(700.0, 700.0);
    s.settle();
    assert_eq!(back_buttons(&mut s), ["back-to-start"], "700 with a finger: the ＜ only");
    s.size(1200.0, 700.0);
    s.settle();
    assert_eq!(back_buttons(&mut s), ["back-to-start", "back-to-start-icon"], "back at 1200");
    drop(s); // two Scenarios cannot be alive together
    let mut s = screen(Kind::Document, 700.0, false);
    assert!(back_buttons(&mut s).is_empty(), "700 with a mouse: neither");
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    assert_eq!(back_buttons(&mut s), ["back-to-start"], "the first touch: the ＜ only");
}

/// NT5: the ＜ stays where it was (its left edge the same with and without the icon: it is the first button), and the space the icon had is not left
/// empty: the next button of the title row (the undo button) is at least 24 points nearer the ＜ than it is at 720, where the icon is. The title bar's
/// room (how many buttons fit before the row folds) counts one button fewer (`chrome.rs`: `left = 1 + shows_back_to_start + ...`).
fn nt5_row() {
    let _probe = buttons::Probe::new();
    let mut narrow = screen(Kind::Document, 719.0, true);
    let _ = back_buttons(&mut narrow);
    let arrow_narrow = buttons::bounds("back-to-start").expect("the ＜");
    let undo_narrow = buttons::bounds("title-undo").or_else(|| buttons::bounds("undo")).expect("the undo button is in the title row");
    drop(narrow); // two Scenarios cannot be alive together; the bounds are kept
    let mut wide = screen(Kind::Document, 720.0, true);
    let _ = back_buttons(&mut wide);
    let arrow_wide = buttons::bounds("back-to-start").expect("the ＜");
    let undo_wide = buttons::bounds("title-undo").or_else(|| buttons::bounds("undo")).expect("the undo button is in the title row");
    assert!((f32::from(arrow_narrow.left()) - f32::from(arrow_wide.left())).abs() < 1.0, "the ＜ keeps its place");
    let gap = |a: Bounds<Pixels>, u: Bounds<Pixels>| f32::from(u.left()) - f32::from(a.right());
    assert!(gap(arrow_wide, undo_wide) - gap(arrow_narrow, undo_narrow) >= 24.0, "the icon's room is not left empty ({} vs {})", gap(arrow_wide, undo_wide), gap(arrow_narrow, undo_narrow));
}

use gpui_kit::{Bounds, Pixels};
