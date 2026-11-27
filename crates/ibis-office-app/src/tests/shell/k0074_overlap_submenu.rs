// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0074: a press outside the front-most of overlapping menus only closes that one menu (design: `tasks/doc/test-design/K0074.md`; the
//! numbers M1... are that document's). A cascade that fits neither right nor left of its parent is put inside the window and covers
//! part of the parent (`explicit_only`, the fork's `submenu_placement.rs`); a press outside the front menu but inside the parent then
//! reached the parent's line. The user's decision (2026-11-25): only the front-most menu closes, the lower menu stays open, and the
//! press goes nowhere (no line, no element under the pointer).
//!
//! Every test is in `all()` (K0074 is fixed) (the ones that already pass say so in the design and may move to `all()` at once).
//! The tests read the menus through the fork's read-only accessors `PopupMenu::painted_bounds`, `painted_rows`, `shown_submenu`.
use super::Scenario;
use crate::shell::chrome::{FORCED_SIDEBAR, SidebarPanel};
use crate::shell::editor::SlideEditor;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::*;
use crate::docx::scenarios::DocSteps;
use ibis_office_docx::ext::*;
use ibis_office_xlsx::ext::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0074_m1_mouse_press_on_a_lower_line_only_closes_the_front_menu", m1_mouse_on_lower_line),
        ("shell::k0074_m2_mouse_press_on_the_body_only_closes_the_front_menu", m2_mouse_on_body),
        ("shell::k0074_t1_tap_on_a_lower_line_only_closes_the_front_menu", t1_tap_on_lower_line),
        ("shell::k0074_t2_tap_on_the_body_only_closes_the_front_menu", t2_tap_on_body),
        ("shell::k0074_m3_without_overlap_the_whole_chain_closes_as_before", m3_no_overlap),
        ("shell::k0074_m4_esc_closes_one_level_at_a_time", m4_esc),
        ("shell::k0074_m5_a_press_inside_the_front_menu_still_runs_its_line", m5_inside_front),
        ("shell::k0074_l1_right_click_menu_of_the_document", l1_right_click_menu),
        ("shell::k0074_l2_folded_menu_bar_of_the_sheet", l2_folded_bar),
    ]
}

const FILE: &str = "test/editing/xlsx-sales.xlsx";

struct State {
    popup: Option<Entity<PopupMenu>>,
    row_a: Rc<Cell<u32>>,
    row_b: Rc<Cell<u32>>,
    leaf: Rc<Cell<u32>>,
    dismissed: Rc<Cell<u32>>,
    _sub: Option<Subscription>,
}
thread_local! {static STATE: RefCell<Option<State>> = const { RefCell::new(None) };}

fn panel(_: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    let viewport = window.viewport_size();
    let popup = STATE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let st = slot.as_mut().unwrap();
        if st.popup.is_none() {
            let (a, b, leaf) = (st.row_a.clone(), st.row_b.clone(), st.leaf.clone());
            let popup = PopupMenu::build(window, cx, move |menu, w, cx| {
                let (a, b, leaf) = (a.clone(), b.clone(), leaf.clone());
                menu.min_w(px(200.)).scrollable(false)
                    .item(PopupMenuItem::new("Row A").on_click(move |_, _, _| a.set(a.get() + 1)))
                    .item(PopupMenuItem::new("Row B").on_click(move |_, _, _| b.set(b.get() + 1)))
                    .submenu("More", w, cx, move |m, _, _| {
                        let leaf = leaf.clone();
                        m.min_w(px(200.)).item(PopupMenuItem::new("Leaf").on_click(move |_, _, _| leaf.set(leaf.get() + 1)))
                    })
            });
            let d = st.dismissed.clone();
            st._sub = Some(cx.subscribe(&popup, move |_, _, _: &DismissEvent, _| d.set(d.get() + 1)));
            st.popup = Some(popup);
        }
        st.popup.as_ref().unwrap().clone()
    });
    // Low in the window, so that the body (the sheet's cells) is free above it.
    let at = point(px(10.), viewport.height - px(170.));
    Some(SidebarPanel {
        title: "K0074".into(),
        body: div().child(deferred(anchored().position(at).snap_to_window().child(div().relative().child(popup))).with_priority(2)).into_any_element(),
    })
}

struct Harness {
    // (Declared before `s`: the menu must be dropped before the scenario's window, or the entity is reported as leaked.)
    popup: Entity<PopupMenu>,
    s: Scenario,
    row_a: Rc<Cell<u32>>,
    row_b: Rc<Cell<u32>>,
    leaf: Rc<Cell<u32>>,
    dismissed: Rc<Cell<u32>>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        FORCED_SIDEBAR.with(|c| c.set(None));
        STATE.with(|slot| slot.borrow_mut().take());
    }
}

impl Harness {
    /// A sheet of `width` x 600 with the test popup (Row A, Row B, More > Leaf) painted low-left, the submenu opened by a click on More.
    fn open(width: f32) -> Self {
        let mut s = Scenario::open(FILE);
        s.size(width, 600.);
        let st = State { popup: None, row_a: Rc::default(), row_b: Rc::default(), leaf: Rc::default(), dismissed: Rc::default(), _sub: None };
        let (row_a, row_b, leaf, dismissed) = (st.row_a.clone(), st.row_b.clone(), st.leaf.clone(), st.dismissed.clone());
        STATE.with(|slot| *slot.borrow_mut() = Some(st));
        FORCED_SIDEBAR.with(|c| c.set(Some((panel, "k0074", crate::shell::dialog::DialogMode::Sidebar))));
        s.update(|e, _, cx| {
            e.sidebar_open = true;
            cx.notify();
        });
        s.settle();
        let popup = STATE.with(|slot| slot.borrow().as_ref().unwrap().popup.clone().expect("the test popup is built"));
        let mut h = Harness { s, popup, row_a, row_b, leaf, dismissed };
        let (rows, _) = h.rows(&h.popup.clone());
        let more = rows.iter().find(|(_, sub)| *sub).expect("More is a submenu row").0.center();
        h.s.click((more.x.into(), more.y.into()));
        h.s.wait(350);
        h
    }
    fn rows(&mut self, menu: &Entity<PopupMenu>) -> (Vec<(Bounds<Pixels>, bool)>, Bounds<Pixels>) {
        let menu = menu.clone();
        self.s.with_app(|cx| menu.read(cx).painted_rows()).pipe(|r| {
            let b = self.s.with_app(|cx| menu.read(cx).painted_bounds());
            (r, b)
        })
    }
    fn front(&mut self) -> Option<Entity<PopupMenu>> {
        let p = self.popup.clone();
        self.s.with_app(|cx| p.read(cx).shown_submenu())
    }
    /// (parent, child) bounds once the submenu is shown; panics if the setup did not open it.
    fn geometry(&mut self) -> (Bounds<Pixels>, Bounds<Pixels>) {
        let child = self.front().expect("setup: the submenu is shown (a click on More opens it)");
        let c = self.s.with_app(|cx| child.read(cx).painted_bounds());
        let p = self.popup.clone();
        let pb = self.s.with_app(|cx| p.read(cx).painted_bounds());
        (pb, c)
    }
    /// A point inside the parent and outside the child, on a line that is not More (`None` if the child covers the parent's lines).
    fn lower_line_point(&mut self) -> Option<(f32, f32)> {
        let (parent, child) = self.geometry();
        let p = self.popup.clone();
        let (rows, _) = self.rows(&p);
        for (b, sub) in rows {
            if sub {
                continue;
            }
            for x in [parent.left() + px(6.), parent.right() - px(6.)] {
                let y = b.center().y;
                let pt = point(x, y);
                if parent.contains(&pt) && !child.contains(&pt) && b.contains(&pt) {
                    return Some((x.into(), y.into()));
                }
            }
        }
        None
    }
    fn body_point(&mut self) -> (f32, f32) {
        let c = self.s.with(|e, _| e.cell_center(1, 2)).expect("a cell is on screen");
        (c.x.into(), c.y.into())
    }
}

trait Pipe: Sized {
    fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R {
        f(self)
    }
}
impl<T> Pipe for T {}

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

fn tap(s: &mut Scenario, (x, y): (f32, f32)) {
    let at = point(px(x), px(y));
    let ev = |phase| PlatformInput::Touch(TouchEvent { id: TouchId(1), phase, position: at, predicted_position: None, force: None });
    s.dispatch(vec![ev(TouchPhase::Started)]);
    s.dispatch(vec![ev(TouchPhase::Ended)]);
}

/// The widths where the submenu (200 wide) covers part of the parent (200 wide): neither side fits.
const WIDTHS: [f32; 3] = [360., 400., 430.];

/// After a press outside the front menu: only the front menu closed.
fn assert_only_front_closed(h: &mut Harness, what: &str) {
    assert_eq!((h.row_a.get(), h.row_b.get(), h.leaf.get()), (0, 0, 0), "{what}: no line ran (Row A, Row B, Leaf)");
    assert!(h.front().is_none(), "{what}: the front submenu is closed");
    assert_eq!(h.dismissed.get(), 0, "{what}: the lower menu stays open (times it was dismissed)");
}

fn m1_mouse_on_lower_line() {
    let mut any = false;
    for w in WIDTHS {
        let mut h = Harness::open(w);
        let (parent, child) = h.geometry();
        if !parent.intersects(&child) {
            continue;
        }
        let Some(pt) = h.lower_line_point() else { continue };
        any = true;
        let before = h.s.selected();
        h.s.click(pt);
        h.s.wait(100);
        assert_only_front_closed(&mut h, &format!("width {w}"));
        assert_eq!(h.s.selected(), before, "width {w}: the selection did not move");
    }
    assert!(any, "setup: some width shows a submenu that covers part of the parent with a free strip of a lower line");
}

fn m2_mouse_on_body() {
    let mut any = false;
    for w in WIDTHS {
        let mut h = Harness::open(w);
        let (parent, child) = h.geometry();
        if !parent.intersects(&child) {
            continue;
        }
        any = true;
        let before = h.s.selected();
        let pt = h.body_point();
        h.s.click(pt);
        h.s.wait(100);
        assert_only_front_closed(&mut h, &format!("width {w}"));
        assert_eq!(h.s.selected(), before, "width {w}: the click did not reach the cell under it");
    }
    assert!(any, "setup: some width overlaps");
}

fn t1_tap_on_lower_line() {
    let mut any = false;
    for w in WIDTHS {
        let mut h = Harness::open(w);
        let (parent, child) = h.geometry();
        if !parent.intersects(&child) {
            continue;
        }
        let Some(pt) = h.lower_line_point() else { continue };
        any = true;
        tap(&mut h.s, pt);
        h.s.wait(100);
        assert_only_front_closed(&mut h, &format!("tap, width {w}"));
    }
    assert!(any, "setup: some width overlaps with a free strip");
}

fn t2_tap_on_body() {
    let mut any = false;
    for w in WIDTHS {
        let mut h = Harness::open(w);
        let (parent, child) = h.geometry();
        if !parent.intersects(&child) {
            continue;
        }
        any = true;
        let before = h.s.selected();
        let pt = h.body_point();
        tap(&mut h.s, pt);
        h.s.wait(100);
        assert_only_front_closed(&mut h, &format!("tap, width {w}"));
        assert_eq!(h.s.selected(), before, "tap, width {w}: the tap did not select the cell under it");
    }
    assert!(any, "setup: some width overlaps");
}

/// Today's behaviour that must not change (the user's words: "重ならないときに今の動きが変わらない"): a wide window, the submenu beside its parent, a press on the
/// body closes the whole chain and the cell does not get it.
fn m3_no_overlap() {
    let mut h = Harness::open(1000.);
    let (parent, child) = h.geometry();
    assert!(!parent.intersects(&child), "setup: no overlap at 1000 wide");
    let before = h.s.selected();
    let pt = h.body_point();
    h.s.click(pt);
    h.s.wait(100);
    assert!(h.front().is_none(), "the submenu is closed");
    assert_eq!(h.dismissed.get(), 1, "the whole chain is dismissed");
    assert_eq!((h.row_a.get(), h.row_b.get(), h.leaf.get()), (0, 0, 0));
    assert_eq!(h.s.selected(), before, "the cell did not get the press");
}

/// Esc closes one level at a time (the user's decision, 2026-11-25): the first Esc closes the front submenu only, the second the parent.
/// No line runs and the document gets nothing.
fn m4_esc() {
    let mut h = Harness::open(400.);
    let (parent, child) = h.geometry();
    assert!(parent.intersects(&child), "setup: overlap at 400");
    let before = h.s.selected();
    h.s.key("escape");
    h.s.wait(100);
    assert!(h.front().is_none(), "the first Esc closes the front submenu");
    assert_eq!(h.dismissed.get(), 0, "the first Esc leaves the parent open");
    h.s.key("escape");
    h.s.wait(100);
    assert_eq!(h.dismissed.get(), 1, "the second Esc closes the parent");
    assert_eq!((h.row_a.get(), h.row_b.get(), h.leaf.get()), (0, 0, 0), "no line ran");
    assert_eq!(h.s.selected(), before);
}

/// Inside the front menu a press still runs its line and closes the menus (no change).
fn m5_inside_front() {
    let mut h = Harness::open(400.);
    let child = h.front().expect("setup");
    let (rows, _) = h.rows(&child);
    let at = rows[0].0.center();
    h.s.click((at.x.into(), at.y.into()));
    h.s.wait(100);
    assert_eq!(h.leaf.get(), 1, "the Leaf line ran once");
    assert_eq!((h.row_a.get(), h.row_b.get()), (0, 0));
}

/// The real context menu of the document (right click in the text): open its first submenu by a click and press outside it. `link_popup` is the menu.
fn l1_right_click_menu() {
    let mut any = false;
    for w in [420., 520., 640.] {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        s.size(w, 700.);
        let (para, _) = s.paragraph(2);
        s.click_text(ibis_doc_model::FlowPos::new(para, 0));
        let at = text_screen_pos(&mut s, ibis_doc_model::FlowPos::new(para, 1));
        s.right_click(at);
        let Some(menu) = s.with(|e, _| e.link_popup.as_ref().map(|p| p.menu.clone())) else { panic!("setup: the right click opens the menu") };
        let rows = s.with_app(|cx| menu.read(cx).painted_rows());
        let more = rows.iter().find(|(_, sub)| *sub).expect("setup: the menu has a submenu").0.center();
        s.click((more.x.into(), more.y.into()));
        s.wait(350);
        let Some(child) = s.with_app(|cx| menu.read(cx).shown_submenu()) else { continue };
        let (pb, cb) = s.with_app(|cx| (menu.read(cx).painted_bounds(), child.read(cx).painted_bounds()));
        if !pb.intersects(&cb) {
            continue;
        }
        any = true;
        let sel = s.with(|e, _| e.text_selection());
        // On the body: outside both menus (the middle of the left margin strip of the page).
        s.click((6., 400.));
        s.wait(100);
        let menu2 = menu.clone();
        assert!(s.with_app(|cx| menu2.read(cx).shown_submenu()).is_none(), "width {w}: the front submenu is closed");
        assert!(s.with(|e, _| e.link_popup.is_some()), "width {w}: the right click menu stays open");
        assert_eq!(s.with(|e, _| e.text_selection()), sel, "width {w}: the press did not move the caret");
    }
    assert!(any, "setup: some width overlaps");
}

/// The sheet's menu bar folded into the ▾ at a narrow width (K0061): the ▾ opens the folded menus as submenus; press outside the front one.
fn l2_folded_bar() {
    let _on = crate::shell::access_key::override_supported(true);
    let mut any = false;
    for w in [360., 420., 500.] {
        let mut s = Scenario::open(FILE);
        s.size(w, 700.);
        s.key("alt-e");
        let Some(menu) = s.with(|e, _| e.open_menu.as_ref().map(|o| o.popup_entity().clone())) else { panic!("setup: Alt+E opens a menu (the ▾ when folded)") };
        let rows = s.with_app(|cx| menu.read(cx).painted_rows());
        let Some((row, _)) = rows.iter().find(|(_, sub)| *sub) else { continue };
        s.click((row.center().x.into(), row.center().y.into()));
        s.wait(350);
        let Some(child) = s.with_app(|cx| menu.read(cx).shown_submenu()) else { continue };
        let (pb, cb) = s.with_app(|cx| (menu.read(cx).painted_bounds(), child.read(cx).painted_bounds()));
        if !pb.intersects(&cb) {
            continue;
        }
        any = true;
        let before = s.selected();
        // A cell outside both menus (the menus are 280 wide and, in a narrow window, cover the first columns: a press on them is a press on a line).
        let c = (0..8u32)
            .flat_map(|column| (0..40u32).map(move |row| (column, row)))
            .filter_map(|(column, row)| s.with(|e, _| e.cell_center(column, row)))
            .find(|c| !pb.contains(c) && !cb.contains(c) && f32::from(c.x) < w && f32::from(c.y) < 690.0)
            .expect("a cell outside both menus is on screen");
        s.click((c.x.into(), c.y.into()));
        s.wait(100);
        assert!(s.with_app(|cx| menu.read(cx).shown_submenu()).is_none(), "width {w}: the front submenu is closed");
        assert!(s.with(|e, _| e.open_menu.is_some()), "width {w}: the lower menu stays open");
        assert_eq!(s.selected(), before, "width {w}: the cell did not get the press");
    }
    assert!(any, "setup: some width overlaps");
}
