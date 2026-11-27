// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0087 (iOS and Android; design: `tasks/doc/test-design/K0087.md`; the numbers MB1... are that document's): the start window's ウィンドウ > しまう and
//! the ヘルプ > 更新の確認 are not shown; ヘルプ > ソースコード and the About window's 「Powered by ibisOffice」 and its github.com address open the
//! project's page in the outside browser; ファイル > 戻る (K0076) has the ＜ icon. A computer is as it was. The mobile OS is
//! `mobile_os::ForcedMobileOs` (K0076); what the app asks the system to open is read from the gate every start of an outside app passes
//! (`external::take_launched`: in a test run nothing is started and the gate keeps `url <address>`).
use crate::shell::about::PROJECT_URL;
use crate::shell::chrome::MenuEntry;
use crate::shell::external::take_launched;
use crate::shell::mobile_os::ForcedMobileOs;
use crate::shell::scenario::{Scenario, button_probe as buttons};
use gpui_kit::*;
use ibis_formats::Kind;

const KINDS: [Kind; 3] = [Kind::Document, Kind::Workbook, Kind::Presentation];

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0087_d1_a_computer_keeps_minimize_check_for_updates_and_the_close_icon", d1_computer),
        ("shell::k0087_mb1_the_start_windows_window_menu_has_no_minimize", mb1_no_minimize),
        ("shell::k0087_mb2_no_check_for_updates_in_any_help_menu", mb2_no_update_check),
        ("shell::k0087_mb3_source_code_opens_the_projects_page_outside", mb3_source_code),
        ("shell::k0087_mb4_a_tap_on_the_about_links_opens_the_projects_page_outside", mb4_about_links),
        ("shell::k0087_mb5_the_back_line_has_the_arrow_icon", mb5_back_icon),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

fn ids(entries: &[MenuEntry], out: &mut Vec<(&'static str, Option<&'static str>)>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(i) => out.push((i.id, i.icon)),
            MenuEntry::Submenu(sub) => ids(&sub.items, out),
            _ => {}
        }
    }
}

/// Every line (id, icon) of the start window's menus.
fn start_lines(s: &mut Scenario) -> Vec<(&'static str, Option<&'static str>)> {
    s.with(|e, _| {
        let mut out = Vec::new();
        for (_, entries) in e.start_menus() {
            ids(&entries, &mut out);
        }
        out
    })
}

/// Every line (id, icon) of a screen's menus, the menu bar's and the menus' (the keys and the search use those).
fn screen_lines(s: &mut Scenario) -> Vec<(&'static str, Option<&'static str>)> {
    s.with(|e, _| {
        let mut out = Vec::new();
        for (_, entries) in &e.menus().menus {
            ids(entries, &mut out);
        }
        for (_, entries) in &e.menu_bar_menus().menus {
            ids(entries, &mut out);
        }
        out
    })
}

fn document(kind: Kind) -> Scenario {
    let mut s = Scenario::new_document(kind);
    s.size(1000., 700.);
    s.settle();
    s
}

/// MB1 (K0087 1): the start window has no ウィンドウ menu (so no しまう, `window.minimize`). A screen's own ウィンドウ menu is not part of this.
fn mb1_no_minimize() {
    let _mobile = ForcedMobileOs::new();
    let mut s = Scenario::start_view();
    let lines = start_lines(&mut s);
    assert!(lines.iter().any(|(id, _)| *id == "file.open"), "setup: the start menus are made");
    assert!(!lines.iter().any(|(id, _)| *id == "window.minimize"), "no しまう on a mobile OS: {lines:?}");
    // The decision (the user): the whole ウィンドウ menu goes from the start window's menus (only one window can be open at a time yet).
    let has_window_menu = s.with(|e, _| e.start_menus().iter().any(|(id, _)| *id == crate::shell::chrome::MenuId::Window));
    assert!(!has_window_menu, "the start window has no ウィンドウ menu on a mobile OS");
}

/// MB2 (K0087 2): no 更新の確認 (`help.check-update`) in the start window's ヘルプ menu, nor in a document's, workbook's or presentation's, nor found by
/// the menu search. (The update's log, `help.update-log`, is the same family: design 1.)
fn mb2_no_update_check() {
    let _mobile = ForcedMobileOs::new();
    let mut s = Scenario::start_view();
    assert!(!start_lines(&mut s).iter().any(|(id, _)| *id == "help.check-update"), "the start window's ヘルプ has no 更新の確認");
    drop(s); // one Scenario is alive at a time
    for kind in KINDS {
        let mut s = document(kind);
        assert!(!screen_lines(&mut s).iter().any(|(id, _)| *id == "help.check-update"), "{kind:?}: no 更新の確認 in the menus");
        let searched = s.with(|e, _| e.command_search_items().into_iter().any(|i| i.id == "help.check-update"));
        assert!(!searched, "{kind:?}: and the menu search does not find it");
        // The decision: the update log stays (only the check goes).
        assert!(screen_lines(&mut s).iter().any(|(id, _)| *id == "help.update-log"), "{kind:?}: 更新履歴 stays");
    }
}

/// MB3 (K0087 3; passes now where the gate works): ヘルプ > ソースコード (the start window's and a screen's) asks the system to open the project's page.
fn mb3_source_code() {
    let _mobile = ForcedMobileOs::new();
    let _ = take_launched();
    let mut s = Scenario::start_view();
    let handler = s.with(|e, _| e.start_menus().into_iter().flat_map(|(_, v)| v).find_map(|x| if let MenuEntry::Item(i) = x { (i.id == "help.source-code").then(|| i.handler.clone()).flatten() } else { None }));
    let handler = handler.expect("the start window's ヘルプ has ソースコード");
    s.update(|e, w, cx| handler(e, w, cx));
    assert_eq!(take_launched(), [format!("url {PROJECT_URL}")], "the start window's line opens the project's page");
    drop(s); // one Scenario is alive at a time
    let mut s = document(Kind::Document);
    s.menu("help.source-code");
    assert_eq!(take_launched(), [format!("url {PROJECT_URL}")], "a screen's line opens it too");
}

/// MB4 (K0087 4; passes now where the gate works): a finger's tap on 「Powered by ibisOffice」 and on the github.com address of the About window asks the
/// system to open the project's page (each once).
fn mb4_about_links() {
    let _mobile = ForcedMobileOs::new();
    let _probe = buttons::Probe::new();
    let mut s = document(Kind::Document);
    s.update(|e, _, cx| e.show_about(cx));
    s.settle();
    for id in ["about-powered", "about-url"] {
        let _ = take_launched();
        buttons::clear();
        s.update(|_, _, cx| cx.notify());
        s.settle();
        let b = buttons::bounds(id).unwrap_or_else(|| panic!("setup: {id} is drawn: {:?}", buttons::all()));
        let c = b.center();
        s.dispatch(vec![PlatformInput::Touch(TouchEvent { id: TouchId(1), phase: TouchPhase::Started, position: c, predicted_position: None, force: None })]);
        s.advance_ms(60);
        s.dispatch(vec![PlatformInput::Touch(TouchEvent { id: TouchId(1), phase: TouchPhase::Ended, position: c, predicted_position: None, force: None })]);
        s.settle();
        assert_eq!(take_launched(), [format!("url {PROJECT_URL}")], "a tap on {id} opens the project's page");
    }
}

/// MB5 (K0087 5): ファイル > 戻る (`file.close` of K0076) has the ＜ icon (the title row's, `chevron_left`) in the menus' and the menu bar's lines, on
/// the three screens.
fn mb5_back_icon() {
    let _mobile = ForcedMobileOs::new();
    for kind in KINDS {
        let mut s = document(kind);
        let found: Vec<_> = screen_lines(&mut s).into_iter().filter(|(id, _)| *id == "file.close").collect();
        assert!(!found.is_empty(), "{kind:?}: setup");
        assert!(found.iter().all(|(_, icon)| *icon == Some("chevron_left")), "{kind:?}: the 戻る line has the ＜ icon: {found:?}");
    }
}

/// D1 (unchanged; passes now): on a computer the start window's ウィンドウ has しまう, every ヘルプ has 更新の確認 (the browser has none: not run here),
/// and ファイル > 閉じる keeps its × icon.
fn d1_computer() {
    let mut s = Scenario::start_view();
    let lines = start_lines(&mut s);
    assert!(lines.iter().any(|(id, _)| *id == "window.minimize"), "しまう: {lines:?}");
    assert!(lines.iter().any(|(id, _)| *id == "help.check-update"), "更新の確認: {lines:?}");
    drop(s); // one Scenario is alive at a time
    for kind in KINDS {
        let mut s = document(kind);
        let lines = screen_lines(&mut s);
        assert!(lines.iter().any(|(id, _)| *id == "help.check-update"), "{kind:?}: 更新の確認");
        assert!(lines.iter().filter(|(id, _)| *id == "file.close").all(|(_, icon)| *icon == Some("close")), "{kind:?}: 閉じる keeps its icon");
    }
}
