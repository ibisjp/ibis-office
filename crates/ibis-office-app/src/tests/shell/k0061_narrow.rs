// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the narrow window's layout (K0061): under 720 px on a touch device (K0077: not with a mouse) the menu bar and the toolbar are at the bottom, below the
//! document and the status bar; the toolbar's ^ is not shown; on a touch device the bars scroll sideways with a shadow at each end
//! that has more to show. The window is laid out again as it is resized.
use crate::shell::narrow_layout::more_to_scroll;
use crate::shell::scenario::{Scenario, button_probe as buttons};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0061_order_at_719_and_720", order_at_719_and_720),
        ("shell::k0061_narrow_has_no_hide_menus_button", no_hide_menus_button),
        ("shell::k0061_touch_bars_scroll_with_shadows", touch_bars_scroll),
        ("shell::k0061_mouse_bars_fold_as_before", mouse_bars_fold),
        ("shell::k0061_menu_rises_from_the_bottom_bar", menu_rises),
        ("shell::k0077_a_narrow_window_with_a_mouse_keeps_the_bars_on_top", mouse_keeps_top),
    ]
}

/// A window of `width` with a mouse (nothing has touched it).
fn document(width: f32) -> Scenario {
    let mut s = Scenario::new_document(ibis_formats::Kind::Document);
    s.size(width, 700.);
    s.settle();
    s
}

/// A window of `width` on a touch device (the first touch has been seen).
fn touch_document(width: f32) -> Scenario {
    let mut s = document(width);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    s
}

/// `(workspace bottom, workspace top, toolbar top, menu bar button top)` with the buttons measured.
fn rows(s: &mut Scenario) -> (f32, f32, f32, f32) {
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let (ws, tb) = s.with(|e, _| (e.workspace_bounds.unwrap(), e.toolbar_bounds.get()));
    let menu = buttons::bounds("menu-button-menu.file").or_else(|| buttons::bounds("menu-button-menu.edit")).unwrap_or_else(|| panic!("a menu of the bar: {:?}", buttons::all()));
    (f32::from(ws.bottom()), f32::from(ws.top()), f32::from(tb.top()), f32::from(menu.top()))
}

fn order_at_719_and_720() {
    let mut s = touch_document(720.);
    let (ws_bottom, ws_top, toolbar_top, menu_top) = rows(&mut s);
    assert!(menu_top < toolbar_top && toolbar_top < ws_top, "720: menu bar, toolbar, then the document ({menu_top} {toolbar_top} {ws_top})");
    assert!(ws_bottom > toolbar_top);
    s.size(719., 700.);
    s.settle();
    let (ws_bottom, ws_top, toolbar_top, menu_top) = rows(&mut s);
    assert!(ws_top < ws_bottom && ws_bottom <= menu_top + 1., "719: the document, then the menu bar ({ws_bottom} {menu_top})");
    assert!(menu_top < toolbar_top, "719: the toolbar is below the menu bar ({menu_top} {toolbar_top})");
    assert!(toolbar_top > 600., "719: the toolbar is at the window's bottom ({toolbar_top})");
    // Laid out again when widened.
    s.size(900., 700.);
    s.settle();
    let (_, ws_top, toolbar_top, menu_top) = rows(&mut s);
    assert!(menu_top < toolbar_top && toolbar_top < ws_top, "900: back to the wide order");
}

fn no_hide_menus_button() {
    for (width, shown) in [(720., true), (719., false)] {
        let mut s = touch_document(width);
        let _probe = buttons::Probe::new();
        s.update(|_, _, cx| cx.notify());
        s.settle();
        assert_eq!(buttons::all().iter().any(|id| id.contains("toggle-menus")), shown, "{width}: the toolbar's ^: {:?}", buttons::all());
    }
}

fn touch_bars_scroll() {
    let mut s = document(400.);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    let (menu_more, toolbar_more) = s.with(|e, _| (more_to_scroll(&e.menu_bar_scroll), more_to_scroll(&e.toolbar_scroll)));
    assert_eq!(toolbar_more, (false, true), "the toolbar has more to the right at its start");
    assert!(!menu_more.0, "the menu bar is at its start");
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    assert!(buttons::bounds("toolbar-more-button").is_none(), "no ︙ on a touch device");
    assert!(buttons::bounds("menu-button-overflow").is_none(), "no ▾ on a touch device");
    let tb = s.with(|e, _| e.toolbar_bounds.get());
    let at = (f32::from(tb.left()) + 100., f32::from(tb.top()) + 18.);
    s.wheel_xy(at, -200., 0.);
    s.settle();
    let (_, toolbar_more) = s.with(|e, _| (0, more_to_scroll(&e.toolbar_scroll)));
    assert!(toolbar_more.0, "scrolled: more to the left now");
    let x = s.with(|e, _| f32::from(e.toolbar_scroll.offset().x));
    assert!(x < 0., "the pill moved with its buttons ({x})");
    let tb2 = s.with(|e, _| e.toolbar_bounds.get());
    assert!(tb2.left() < tb.left(), "the pill's left end scrolled away with the buttons");
    s.screenshot("narrow-touch");
}

fn mouse_bars_fold() {
    let mut s = document(400.);
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    assert!(buttons::bounds("toolbar-more-button").is_some(), "the mouse keeps the ︙: {:?}", buttons::all());
    assert_eq!(s.with(|e, _| more_to_scroll(&e.toolbar_scroll)), (false, false), "nothing scrolls with a mouse");
}

fn menu_rises() {
    let mut s = touch_document(600.);
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let bar = buttons::bounds("menu-button-menu.file").expect("the file menu");
    buttons::click(&mut s, "menu-button-menu.file");
    s.settle();
    let popup = s.with(|e, _| e.open_menu.is_some());
    assert!(popup, "the menu is open");
    s.screenshot("narrow-menu-open");
    assert!(f32::from(bar.top()) > 300.);
}

/// K0077 (T1 to T3): a narrow window with a mouse keeps the wide order (menu bar, toolbar, document) and its ^; the same window on a
/// touch device has the bars below the document; and the menu hangs down from a bar on top.
fn mouse_keeps_top() {
    let mut s = document(400.);
    let (ws_bottom, ws_top, toolbar_top, menu_top) = rows(&mut s);
    assert!(menu_top < toolbar_top && toolbar_top < ws_top, "mouse, 400: the bars are on top ({menu_top} {toolbar_top} {ws_top})");
    assert!(ws_bottom > toolbar_top);
    let probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    assert!(buttons::all().iter().any(|id| id.contains("toggle-menus")), "mouse, 400: the toolbar's ^ is shown: {:?}", buttons::all());
    buttons::click(&mut s, "menu-button-menu.file");
    s.settle();
    assert!(s.with(|e, _| e.open_menu.is_some()), "the menu is open");
    drop(probe);
    // The first touch moves the bars to the bottom.
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.update(|e, _, cx| e.close_menu(cx));
    s.settle();
    let (ws_bottom, ws_top, toolbar_top, menu_top) = rows(&mut s);
    assert!(ws_top < ws_bottom && ws_bottom <= menu_top + 1. && menu_top < toolbar_top, "touch, 400: the bars are at the bottom ({ws_bottom} {menu_top} {toolbar_top})");
}
