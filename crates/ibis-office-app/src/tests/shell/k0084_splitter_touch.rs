// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0084: a finger can take hold of a splitter (a pane's border) and move it: the border between the presentation's slide list and the slide, the
//! one between the slide and its notes, the right pane's left edge (its width), and the settings window's vertical splitter. The mouse could;
//! it is as it was (design: `tasks/doc/test-design/K0084.md`; the numbers SP1... are that document's).
//!
//! The dividers are found by where they are drawn (probes of `pane-divider`, `notes-divider`, `sidebar-width-handle`, `settings-splitter`), so that the
//! tests do not depend on how the fix catches the finger. The fingers are raw `TouchEvent`s with the mock clock moving between them (K0065's way); the
//! platforms' own touch paths are for the simulators (the design's section 5).
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::docx::ext::*;
use gpui_kit::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0084_g1_the_mouse_drags_every_splitter_one_for_one", g1_mouse_drag),
        ("shell::k0084_g2_the_mouse_grabs_a_splitter_only_on_its_own_width", g2_mouse_hit_width),
        ("shell::k0084_sp1_a_finger_moves_every_splitter_one_for_one", sp1_finger_drag),
        ("shell::k0084_sp2_a_splitter_is_caught_within_22_points_of_its_line_and_not_beyond_30", sp2_hit_width),
        ("shell::k0084_sp3_a_tap_on_a_splitter_changes_nothing_and_leaves_no_drag_behind", sp3_tap),
        ("shell::k0084_sp4_a_second_finger_takes_the_drag_back", sp4_second_finger),
        ("shell::k0084_sp5_the_drag_reverses_and_stays_in_the_window", sp5_reverse_and_limits),
        ("shell::k0084_sp6_the_finger_that_moves_a_splitter_scrolls_and_selects_nothing", sp6_nothing_else_moves),
        ("shell::k0084_sp7_the_settings_width_is_kept_when_the_finger_lifts", sp7_settings_kept),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}


const STEP_MS: u64 = 16;

#[derive(Clone, Copy, Debug)]
enum Splitter {
    /// The presentation's slide list | slide border (vertical line, moves along x).
    PaneList,
    /// The presentation's slide | notes border (horizontal line, moves along y).
    Notes,
    /// The right pane's left edge, on a spreadsheet (a find pane opens it), moves along x (left widens).
    SidebarSheet,
    /// The same on a document.
    SidebarDoc,
    /// The settings window's categories | detail splitter.
    Settings,
}

const ALL: [Splitter; 5] = [Splitter::PaneList, Splitter::Notes, Splitter::SidebarSheet, Splitter::SidebarDoc, Splitter::Settings];

impl Splitter {
    fn id(self) -> &'static str {
        match self {
            Splitter::PaneList => "pane-divider",
            Splitter::Notes => "notes-divider",
            Splitter::SidebarSheet | Splitter::SidebarDoc => "sidebar-width-handle",
            Splitter::Settings => "settings-splitter",
        }
    }
    fn vertical_line(self) -> bool {
        !matches!(self, Splitter::Notes)
    }
}

fn pt(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

fn touch(id: u64, phase: TouchPhase, p: (f32, f32)) -> PlatformInput {
    PlatformInput::Touch(TouchEvent { id: TouchId(id), phase, position: pt(p.0, p.1), predicted_position: None, force: None })
}

/// A window with the splitter on show, the touch on (`touch_on`) or a mouse only.
fn setup(which: Splitter, touch_on: bool) -> Scenario {
    let mut s = match which {
        Splitter::PaneList | Splitter::Notes => Scenario::open("test/editing/pptx-plan.pptx"),
        Splitter::SidebarSheet | Splitter::Settings => Scenario::open("test/editing/xlsx-sales.xlsx"),
        Splitter::SidebarDoc => Scenario::open("test/editing/docx-report.docx"),
    };
    s.size(1200., 800.);
    s.update(move |e, _, cx| {
        e.touch_seen = touch_on;
        cx.notify();
    });
    s.settle();
    match which {
        Splitter::SidebarSheet | Splitter::SidebarDoc => {
            s.update(|e, window, cx| e.open_find(window, cx));
        }
        Splitter::Settings => {
            s.menu("tools.settings");
        }
        _ => {}
    }
    s.settle();
    s
}

/// Where the splitter is drawn: (center x, center y, width, height).
fn bounds(s: &mut Scenario, which: Splitter) -> (f32, f32, f32, f32) {
    buttons::clear();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let b = buttons::bounds(which.id()).unwrap_or_else(|| panic!("setup: {which:?} is drawn ({}): {:?}", which.id(), buttons::all()));
    (f32::from(b.center().x), f32::from(b.center().y), f32::from(b.size.width), f32::from(b.size.height))
}

/// The splitter's place along the axis it moves on.
fn place(s: &mut Scenario, which: Splitter) -> f32 {
    let b = bounds(s, which);
    if which.vertical_line() { b.0 } else { b.1 }
}

/// A point `across` away from the line's center along the axis it moves on, in the middle of its length.
fn at(s: &mut Scenario, which: Splitter, across: f32) -> (f32, f32) {
    let b = bounds(s, which);
    if which.vertical_line() { (b.0 + across, b.1) } else { (b.0, b.1 + across) }
}

fn along(which: Splitter, from: (f32, f32), d: f32) -> (f32, f32) {
    if which.vertical_line() { (from.0 + d, from.1) } else { (from.0, from.1 + d) }
}

fn run(s: &mut Scenario, ms: u64) {
    for _ in 0..(ms / STEP_MS) {
        s.advance_ms(STEP_MS);
    }
    s.settle();
}

/// One finger down at `from`, moved by `d` along the axis in `ms`, and lifted (`lift`).
fn finger_drag(s: &mut Scenario, which: Splitter, from: (f32, f32), d: f32, ms: u64, lift: bool) {
    s.dispatch(vec![touch(1, TouchPhase::Started, from)]);
    let steps = (ms / STEP_MS).max(1);
    for i in 1..=steps {
        s.dispatch(vec![touch(1, TouchPhase::Moved, along(which, from, d * i as f32 / steps as f32))]);
        s.advance_ms(STEP_MS);
    }
    if lift {
        s.dispatch(vec![touch(1, TouchPhase::Ended, along(which, from, d))]);
    }
    s.settle();
}

/// The settings of the scenario in a folder of its own (so that the settings window's width is not kept in the user's file); undone when dropped.
struct Fresh;
impl Fresh {
    fn new(name: &str) -> Self {
        let dir = crate::shell::scenario::scenario_dir(name).join("settings");
        let _ = std::fs::remove_dir_all(&dir);
        // SAFETY: the scenarios run one at a time on the main thread; nothing else reads the environment meanwhile.
        unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir) };
        Fresh
    }
}
impl Drop for Fresh {
    fn drop(&mut self) {
        // SAFETY: as in `new`.
        unsafe { std::env::remove_var("IBIS_SETTINGS_DIR") };
    }
}

/// SP1 (the request): a finger put on the line, moved 60 points along the axis and lifted, moves the splitter 60 points one for one (the right pane's edge
/// goes the way the finger goes), on each of the five.
fn sp1_finger_drag() {
    let _fresh = Fresh::new("shell::k0084_sp1");
    let _probe = buttons::Probe::new();
    for which in ALL {
        let mut s = setup(which, true);
        let before = place(&mut s, which);
        let from = at(&mut s, which, 0.0);
        // A direction the splitter has room for: the notes divider goes up (the pane grows), the others to where they have room (the right pane's edge to the left).
        let d = match which {
            Splitter::Notes | Splitter::SidebarSheet | Splitter::SidebarDoc => -60.0,
            _ => 60.0,
        };
        finger_drag(&mut s, which, from, d, 600, true);
        run(&mut s, 300);
        let after = place(&mut s, which);
        assert!((after - before - d).abs() < 2.0, "{which:?}: the splitter followed the finger one for one ({before} -> {after} for {d})");
    }
}

/// SP2: a finger down up to 22 points from the line (a 44-point catch, the size of a finger: iOS asks 44 pt, Android 48 dp) catches it, and one 30
/// points away does not (the catch is not the whole pane): the splitter does not move then.
fn sp2_hit_width() {
    let _probe = buttons::Probe::new();
    for which in ALL {
        for (across, caught) in [(-18.0, true), (18.0, true), (-30.0, false), (30.0, false)] {
            // (The settings keep the width the last case left: each case starts from the default one, or two of them would drive it into its limit.)
            let _fresh = Fresh::new("shell::k0084_sp2");
            let mut s = setup(which, true);
            let before = place(&mut s, which);
            let from = at(&mut s, which, across);
            let d = if matches!(which, Splitter::Notes | Splitter::SidebarSheet | Splitter::SidebarDoc) { -40.0 } else { 40.0 };
            finger_drag(&mut s, which, from, d, 500, true);
            run(&mut s, 300);
            let moved = place(&mut s, which) - before;
            if caught {
                assert!((moved - d).abs() < 2.0, "{which:?}: a finger {across} pt from the line caught it ({moved} for {d})");
            } else {
                assert!(moved.abs() < 1.0, "{which:?}: a finger {across} pt from the line did not catch it ({moved})");
            }
        }
    }
}

/// SP3: a tap on the line changes nothing, and no drag is left behind (a mouse move afterwards, with no button, moves nothing).
fn sp3_tap() {
    let _fresh = Fresh::new("shell::k0084_sp3");
    let _probe = buttons::Probe::new();
    for which in ALL {
        let mut s = setup(which, true);
        let before = place(&mut s, which);
        let p = at(&mut s, which, 0.0);
        s.dispatch(vec![touch(1, TouchPhase::Started, p)]);
        s.advance_ms(60);
        s.dispatch(vec![touch(1, TouchPhase::Ended, p)]);
        run(&mut s, 300);
        let far = along(which, p, 80.0);
        s.hover(far);
        run(&mut s, 100);
        assert!((place(&mut s, which) - before).abs() < 1.0, "{which:?}: nothing moved after a tap and a mouse move");
    }
}

/// SP4 (K0065, #209): a second finger down while the first holds the splitter takes the drag back: the splitter returns to where it was.
fn sp4_second_finger() {
    let _fresh = Fresh::new("shell::k0084_sp4");
    let _probe = buttons::Probe::new();
    for which in ALL {
        let mut s = setup(which, true);
        let before = place(&mut s, which);
        let from = at(&mut s, which, 0.0);
        let d = if matches!(which, Splitter::Notes | Splitter::SidebarSheet | Splitter::SidebarDoc) { -50.0 } else { 50.0 };
        finger_drag(&mut s, which, from, d, 400, false);
        s.dispatch(vec![touch(2, TouchPhase::Started, (from.0 + 150.0, from.1 + 150.0))]);
        run(&mut s, 100);
        s.dispatch(vec![touch(1, TouchPhase::Ended, along(which, from, d)), touch(2, TouchPhase::Ended, (from.0 + 150.0, from.1 + 150.0))]);
        run(&mut s, 400);
        assert!((place(&mut s, which) - before).abs() < 1.5, "{which:?}: back where it was ({before} -> {})", place(&mut s, which));
    }
}

/// SP5: moved 60 and back 60 with the finger held, the splitter is where it began; moved 3000 points either way it stays inside the window (it stops at
/// the limits the mouse's drag has).
fn sp5_reverse_and_limits() {
    let _fresh = Fresh::new("shell::k0084_sp5");
    let _probe = buttons::Probe::new();
    for which in ALL {
        let mut s = setup(which, true);
        let before = place(&mut s, which);
        let from = at(&mut s, which, 0.0);
        s.dispatch(vec![touch(1, TouchPhase::Started, from)]);
        for i in 1..=20 {
            s.dispatch(vec![touch(1, TouchPhase::Moved, along(which, from, 60.0 * i as f32 / 20.0))]);
            s.advance_ms(STEP_MS);
        }
        for i in (0..20).rev() {
            s.dispatch(vec![touch(1, TouchPhase::Moved, along(which, from, 60.0 * i as f32 / 20.0))]);
            s.advance_ms(STEP_MS);
        }
        s.dispatch(vec![touch(1, TouchPhase::Ended, from)]);
        run(&mut s, 300);
        assert!((place(&mut s, which) - before).abs() < 2.0, "{which:?}: back and forth ends where it began");
        let (w, h) = (1200.0, 800.0);
        // (The narrowest first: the widest list (480) leaves less than twice its width beside it in a 1200 window, so K0083 takes the list away and its divider with it.)
        for d in [-3000.0, 3000.0] {
            let from = at(&mut s, which, 0.0);
            finger_drag(&mut s, which, from, d, 800, true);
            run(&mut s, 300);
            buttons::clear();
            s.update(|_, _, cx| cx.notify());
            s.settle();
            match buttons::bounds(which.id()) {
                Some(b) => {
                    let now = if which.vertical_line() { f32::from(b.center().x) } else { f32::from(b.center().y) };
                    assert!(now > 0.0 && now < if which.vertical_line() { w } else { h }, "{which:?}: {d}: it stays inside the window ({now})");
                }
                None => assert!(matches!(which, Splitter::PaneList | Splitter::Notes) && d > 0.0, "{which:?}: {d}: the splitter is drawn"),
            }
        }
    }
}

/// SP6: the finger that moves a splitter scrolls nothing and selects nothing: the document's, the slide's and the list's scroll positions and the
/// selection are as they were (the pan under the finger is not a scroll of the pane it began on).
fn sp6_nothing_else_moves() {
    let _probe = buttons::Probe::new();
    for which in [Splitter::PaneList, Splitter::SidebarDoc] {
        let mut s = setup(which, true);
        let before = s.with(|e, _| (f32::from(e.workspace_scroll.offset().x), f32::from(e.workspace_scroll.offset().y), e.text_selection(), e.current_slide));
        let from = at(&mut s, which, 0.0);
        finger_drag(&mut s, which, from, 50.0, 500, true);
        run(&mut s, 1500);
        let after = s.with(|e, _| (f32::from(e.workspace_scroll.offset().x), f32::from(e.workspace_scroll.offset().y), e.text_selection(), e.current_slide));
        assert_eq!(before, after, "{which:?}: nothing else moved");
    }
}

/// SP7: the settings window's width is kept in the settings file when the finger lifts, as it is when the mouse is let go (and the window opened again has it).
fn sp7_settings_kept() {
    let _fresh = Fresh::new("shell::k0084_sp7");
    let _probe = buttons::Probe::new();
    let mut s = setup(Splitter::Settings, true);
    let from = at(&mut s, Splitter::Settings, 0.0);
    finger_drag(&mut s, Splitter::Settings, from, 60.0, 600, true);
    run(&mut s, 300);
    let text = std::fs::read_to_string(crate::shell::scenario::scenario_dir("shell::k0084_sp7").join("settings/settings.txt")).unwrap_or_default();
    assert!(text.contains("settings.nav-width="), "the width is kept in the settings file: {text:?}");
}

// ---- the mouse, as it was (these pass now) ----

/// G1: the mouse drags each splitter one for one.
fn g1_mouse_drag() {
    let _fresh = Fresh::new("shell::k0084_g1");
    let _probe = buttons::Probe::new();
    for which in ALL {
        let mut s = setup(which, false);
        let before = place(&mut s, which);
        let from = at(&mut s, which, 0.0);
        let d = if matches!(which, Splitter::Notes | Splitter::SidebarSheet | Splitter::SidebarDoc) { -60.0 } else { 60.0 };
        s.drag(from, along(which, from, d));
        s.settle();
        assert!((place(&mut s, which) - before - d).abs() < 2.0, "{which:?}: the mouse moved it by {d}");
    }
}

/// G2: the mouse's catch is the line's own width (4, 4, 6, 9 points), not a finger's 44: a press 12 points away takes nothing (the growth of the catch is
/// for a touch device only).
fn g2_mouse_hit_width() {
    let _fresh = Fresh::new("shell::k0084_g2");
    let _probe = buttons::Probe::new();
    for which in ALL {
        let mut s = setup(which, false);
        let before = place(&mut s, which);
        let from = at(&mut s, which, 12.0);
        let d = if matches!(which, Splitter::Notes | Splitter::SidebarSheet | Splitter::SidebarDoc) { -60.0 } else { 60.0 };
        s.drag(from, along(which, from, d));
        s.settle();
        assert!((place(&mut s, which) - before).abs() < 1.0, "{which:?}: a mouse press 12 points from the line caught nothing");
    }
}
