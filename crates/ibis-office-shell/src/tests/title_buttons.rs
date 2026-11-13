// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the title row's Draw button (menu bar redesign, stage 8, `tasks/doc/menu-redesign`): the button is the leftmost of the
//! row's buttons on the three screens, its menu is one definition with what a screen cannot do grey and its reason shown, it hangs from the
//! button by the one rule of the drop-down buttons (the right edges line up when the list does not fit), Alt+D opens it on Windows and
//! Linux, the menu bar has no Draw, and a narrow window leaves the Draw button out first and then the comments button.
use gpui_kit::*;
use ibis_pptx::Kind;

use super::{button_probe as buttons, Scenario};
use crate::access_key;
use crate::chrome::{dropdown_list, MenuEntry, MenuId};
use crate::menu_bar::DRAW_IX;
use crate::menu_layout::TitleMenu;

pub fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::title_draw_button_exists", draw_button_exists),
        ("shell::title_draw_menu_shared", draw_menu_shared),
        ("shell::title_draw_menu_position", draw_menu_position),
        ("shell::title_draw_alt_d", draw_alt_d),
        ("shell::title_draw_button_opens_the_ink_panel", draw_button_opens_ink_panel),
        ("shell::title_buttons_narrow", buttons_narrow),
        ("shell::menubar_has_no_draw", menubar_has_no_draw),
        ("shell::sidebars_toolbar_buttons", toolbar_buttons),
        ("shell::title_comments_button_toggles", comments_button_toggles),
        ("shell::title_design_mode_close_button", design_mode_close_button),
    ]
}

const FILES: [(Kind, &str, &str); 3] = [
    (Kind::Document, "docx", "test/editing/docx-report.docx"),
    (Kind::Workbook, "xlsx", "test/editing/xlsx-compatibility-v1.xlsx"),
    (Kind::Presentation, "pptx", "test/benchmark/earnings-presentation.pptx"),
];

fn ids(entries: &[MenuEntry], out: &mut Vec<&'static str>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(i) => out.push(i.id),
            MenuEntry::Submenu(s) => {
                out.push(s.id);
                ids(&s.items, out);
            }
            _ => {}
        }
    }
}

fn item(entries: &[MenuEntry], id: &str) -> Option<crate::chrome::MenuItem> {
    for entry in entries {
        match entry {
            MenuEntry::Item(i) if i.id == id => return Some(i.clone()),
            MenuEntry::Submenu(s) => {
                if let Some(found) = item(&s.items, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// The Draw button is in the title row of each screen, left of the buttons that were there (show comments, the sidebar; the title row has no add-comment button: ⌥⌘M and 挿入 > コメント do it).
fn draw_button_exists() {
    let _probe = buttons::Probe::new();
    for (_, name, file) in FILES {
        let mut s = Scenario::open(file);
        s.size(1400., 900.);
        s.settle();
        s.screenshot(&format!("title-row-{name}"));
        let draw = buttons::bounds("title-draw").unwrap_or_else(|| panic!("{name}: no Draw button in the title row"));
        let order: Vec<_> = ["show-comments", "sidebar"].iter().map(|id| buttons::bounds(id).unwrap_or_else(|| panic!("{name}: no {id} button"))).collect();
        assert!(order.windows(2).all(|w| w[0].left() < w[1].left()), "{name}: the buttons keep their order: {order:?}");
        assert!(draw.right() <= order[0].left() + px(1.), "{name}: Draw is left of show-comments: {draw:?} {:?}", order[0]);
        assert!(draw.bottom() <= px(40.), "{name}: Draw is in the title row: {draw:?}");
    }
}

/// One menu for the three screens: the same lines, the drawing canvas only on the document screen, and what needs ink to be selected
/// is grey with its reason while nothing is selected.
fn draw_menu_shared() {
    let mut all: Vec<(Kind, Vec<&'static str>)> = Vec::new();
    for (kind, name, file) in FILES {
        let mut s = Scenario::open(file);
        let entries = s.with(|e, _| e.title_menu_entries(TitleMenu::Draw));
        let mut found = Vec::new();
        ids(&entries, &mut found);
        all.push((kind, found));
        let canvas = item(&entries, "draw.canvas").unwrap_or_else(|| panic!("{name}: no drawing canvas row"));
        assert_eq!(canvas.enabled(), false, "{name}: the canvas is not made yet (document screen) or not for this screen");
        if kind != Kind::Document {
            assert_eq!(canvas.hint.as_deref(), Some(crate::i18n::text("draw.reason.document-only").as_ref()), "{name}: the canvas says it is for the document screen");
        }
        let to_shape = item(&entries, "draw.to-shape").unwrap();
        assert!(!to_shape.enabled(), "{name}: converting needs ink to be selected");
        assert!(to_shape.hint.is_some(), "{name}: a grey row says why");
        let panel = item(&entries, "draw.ink-panel").unwrap();
        assert!(panel.enabled(), "{name}: the Ink panel opens from the menu");
    }
    assert!(all.windows(2).all(|w| w[0].1 == w[1].1), "the three screens have the same Draw menu: {all:?}");
}

/// The menu hangs from the button by `below_button`: its right edge is on the button's right edge (the list does not fit to the
/// right of the button), and it is inside the window. The rule itself, for a button that has room.
fn draw_menu_position() {
    let viewport = size(px(1000.), px(700.));
    let near_left = Bounds::new(point(px(100.), px(4.)), size(px(32.), px(32.)));
    let at_right = Bounds::new(point(px(900.), px(4.)), size(px(32.), px(32.)));
    assert_eq!(dropdown_list::below_button(near_left, px(280.), viewport), (near_left.bottom_left(), Anchor::TopLeft), "a list that fits hangs from the left edge");
    assert_eq!(dropdown_list::below_button(at_right, px(280.), viewport), (at_right.bottom_right(), Anchor::TopRight), "a list that does not fit hangs from the right edge");
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(FILES[0].2);
    s.size(1000., 700.);
    s.settle();
    let button = buttons::bounds("title-draw").expect("the Draw button");
    // The button opens the Ink panel (K0004); the Draw menu hangs from it when it is opened by its access key.
    s.update(|e, window, cx| e.open_menu_at(DRAW_IX, window, cx));
    assert_eq!(s.with(|e, _| e.open_menu.as_ref().map(|o| o.index())), Some(DRAW_IX), "the Draw menu is open");
    s.screenshot("draw-menu-open");
    let row = buttons::bounds("menu-item-draw.ink-panel").expect("a row of the menu is drawn");
    assert!(row.right() <= button.right() + px(1.), "the menu's right edge is at the button's: {row:?} {button:?}");
    assert!(row.left() >= px(0.) && row.right() <= px(1000.), "the menu is inside the window: {row:?}");
    assert!(row.top() >= button.bottom() - px(2.), "the menu is under the button: {row:?} {button:?}");
}

/// The Draw button opens the screen's Ink panel, on the three screens (the user, K0004, 2026-11-12), not a menu.
fn draw_button_opens_ink_panel() {
    let _probe = buttons::Probe::new();
    for (_, name, file) in FILES {
        let mut s = Scenario::open(file);
        s.size(1400., 800.);
        s.settle();
        buttons::click(&mut s, "title-draw");
        s.settle();
        assert!(s.with(|e, _| e.open_menu.is_none()), "{name}: no menu opens");
        assert!(s.with(|e, _| e.sidebar_open), "{name}: the Ink panel is open in the sidebar");
        s.screenshot(&format!("title-draw-ink-panel-{name}"));
        let ink_open = |s: &mut Scenario| s.with(|e, _| crate::format::with_format!(e, ink_panel_open(e)));
        assert!(ink_open(&mut s), "{name}: the Ink panel is open");
        // A second press closes it (the button is filled while it is open).
        buttons::click(&mut s, "title-draw");
        s.settle();
        assert!(!ink_open(&mut s), "{name}: a second press closes the Ink panel");
        buttons::click(&mut s, "title-draw");
        s.settle();
        assert!(ink_open(&mut s), "{name}: a third press opens it again");
    }
}

fn draw_alt_d() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(FILES[0].2);
    {
        let _off = access_key::override_supported(false);
        s.key("alt-d");
        assert!(s.with(|e, _| e.open_menu.is_none()), "a Mac has no Alt access keys: Alt+D does nothing");
    }
    let _on = access_key::override_supported(true);
    s.key("alt-d");
    assert_eq!(s.with(|e, _| e.open_menu.as_ref().map(|o| o.index())), Some(DRAW_IX), "Alt+D opens the Draw menu");
    s.key("escape");
    assert!(s.with(|e, _| e.open_menu.is_none()), "Esc closes it");
}

/// In a narrow window the file name gives way first (it truncates), then the Draw button goes, then the comments button.
fn buttons_narrow() {
    for (_, name, file) in FILES {
        let mut s = Scenario::open(file);
        let flags: Vec<(f32, (bool, bool))> = (300..=1600).step_by(8).map(|w| (w as f32, s.with(|e, _| e.title_buttons_hidden(w as f32)))).collect();
        assert_eq!(flags.last().unwrap().1, (false, false), "{name}: a wide window shows both");
        assert_eq!(flags.first().unwrap().1, (true, true), "{name}: a very narrow window shows neither");
        assert!(flags.iter().all(|(_, (draw, comments))| !*comments || *draw), "{name}: the comments button never goes before the Draw button: {flags:?}");
        assert!(flags.iter().any(|(_, f)| *f == (true, false)), "{name}: there is a width where only Draw is left out: {flags:?}");
        // Wider means no more is hidden.
        assert!(flags.windows(2).all(|w| (w[1].1 .0 <= w[0].1 .0) && (w[1].1 .1 <= w[0].1 .1)), "{name}: widening never hides one: {flags:?}");
    }
    // In the window: at a width where Draw is left out the button is not drawn.
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(FILES[0].2);
    let narrow = (300..=1600).step_by(8).map(|w| w as f32).find(|w| s.with(|e, _| e.title_buttons_hidden(*w)) == (true, false)).unwrap();
    s.size(narrow, 700.);
    s.settle();
    assert!(buttons::bounds("title-draw").is_none(), "Draw is left out at {narrow}");
    assert!(buttons::bounds("show-comments").is_some(), "the comments button stays at {narrow}");
}

fn menubar_has_no_draw() {
    let counts = [(Kind::Document, 11), (Kind::Workbook, 10), (Kind::Presentation, 11)];
    for ((kind, name, file), (_, count)) in FILES.into_iter().zip(counts) {
        let mut s = Scenario::open(file);
        let menus: Vec<MenuId> = s.with(|e, _| e.menu_bar_menus().menus.iter().map(|(id, _)| *id).collect());
        assert_eq!(menus.len(), count, "{name} ({kind:?}): the bar has {count} menus and none is Draw: {menus:?}");
        assert!(s.with(|e, _| e.bar_items().iter().all(|(m, _)| m.key() != "menu.draw")));
    }
}

/// The sidebars that stand for a button of the title row name it, and the row has it.
fn toolbar_buttons() {
    let _probe = buttons::Probe::new();
    for (kind, name, file) in FILES {
        let mut s = Scenario::open(file);
        s.size(1400., 900.);
        s.settle();
        for def in crate::sidebars::of(kind).iter().filter(|d| d.listed()) {
            if let Some(id) = def.toolbar.filter(|id| ["show-comments", "outline", "notes-pane", "sidebar"].contains(id)) {
                // A button the screen does not have (the outline is the document's, the notes pane the slides') is not asked for.
                let applies = match id {
                    "outline" => kind == Kind::Document,
                    "notes-pane" => kind == Kind::Presentation,
                    _ => true,
                };
                if applies {
                    assert!(buttons::bounds(id).is_some(), "{name}: the sidebar {} stands for the title row's {id}, which is not drawn", def.id);
                }
            }
        }
    }
}

/// The title row has no add-comment button on the three screens; the one comments button shows and hides the comments' panel, and the
/// state it draws from (filled while shown, the tooltip of what a click does) is the panel's.
fn comments_button_toggles() {
    let _probe = buttons::Probe::new();
    for (_, name, file) in FILES {
        let mut s = Scenario::open(file);
        s.size(1400., 900.);
        s.settle();
        assert!(buttons::bounds("add-comment").is_none(), "{name}: the title row has no add-comment button");
        let shown = |s: &mut Scenario| s.with(|e, _| crate::format::with_format!(e, show_comments(e)).map(|(on, _)| on));
        assert_eq!(shown(&mut s), Some(false), "{name}: the comments' panel is not shown at first");
        buttons::click(&mut s, "show-comments");
        s.settle();
        assert_eq!(shown(&mut s), Some(true), "{name}: a click on the button shows the panel");
        buttons::click(&mut s, "show-comments");
        s.settle();
        assert_eq!(shown(&mut s), Some(false), "{name}: a second click hides it");
    }
}

/// A workbook in design mode has the red 「デザイン モードを閉じる」 pill in the title row (as the slide master has its own); a click
/// leaves the mode, and the menu's line is checked while the mode lasts.
fn design_mode_close_button() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(FILES[1].2);
    s.size(1400., 900.);
    s.settle();
    assert!(buttons::bounds("xlsx-design-mode-close").is_none(), "no pill outside design mode");
    assert_eq!(s.menu_state("xlsx.design-mode").map(|(_, checked)| checked), Some(false));
    s.menu("xlsx.design-mode");
    s.settle();
    assert_eq!(s.menu_state("xlsx.design-mode").map(|(_, checked)| checked), Some(true), "the line is checked in design mode");
    let pill = buttons::bounds("xlsx-design-mode-close").expect("the pill is in the title row in design mode");
    assert!(pill.bottom() <= px(44.), "the pill is in the title row: {pill:?}");
    s.screenshot("title-row-design-mode");
    buttons::click(&mut s, "xlsx-design-mode-close");
    buttons::clear(); // the recorded places are the last frame's: draw again without the pill
    s.settle();
    assert!(buttons::bounds("xlsx-design-mode-close").is_none(), "the pill goes when the mode is left");
    assert_eq!(s.menu_state("xlsx.design-mode").map(|(_, checked)| checked), Some(false), "and the line is unchecked");
}
