// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0001: the セットアップ dialog (the fonts that can be downloaded). The list and the font files come from a small server on
//! 127.0.0.1 (`font_setup::test_support`), the font is Carlito (the OFL font already in the repository) under other names; nothing
//! goes to the internet. The settings and the font folder are the scenarios' own (`target/scenario-user-data`).
use crate::shell::font_setup::test_support::{Route, Server};
use crate::shell::font_setup::{self, FontEntry};
use crate::shell::scenario::{button_probe, repo_root, Scenario};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::font_setup_help_menu_opens_the_dialog_skipped_last_and_off", help_menu_lists),
        ("shell::font_setup_header_checkbox_toggles_all_and_a_row_toggles_one", header_toggle),
        ("shell::font_setup_skip_records_the_rows_and_asks_no_more", skip_records),
        ("shell::font_setup_install_downloads_the_checked_fonts_only", install_downloads),
        ("shell::font_setup_a_failed_download_is_told_and_nothing_stops", failed_download),
        ("shell::font_setup_cancel_stops_a_running_download", cancel_download),
        ("shell::font_setup_an_unreadable_list_is_told_in_the_dialog", list_unreadable),
        ("shell::font_setup_startup_check_asks_once_for_a_version", startup_check),
    ]
}

const FILES: [&str; 3] = ["k1-alpha.ttf", "k1-beta.ttf", "k1-gamma.ttf"];

fn carlito() -> Vec<u8> {
    std::fs::read(repo_root().join("crates/ibis-office-render/fonts/carlito/Carlito-Regular.ttf")).expect("Carlito")
}

/// Two servers: one has the three font files (the third is a 404 when `gamma_missing`) and one has the list that names the first's addresses
/// (a server cannot name its own port before it is bound). The server returned has the list; the other is held until `release`.
fn serve_with_list(gamma_missing: bool) -> Server {
    let font = carlito();
    let mut routes: Vec<(String, Route)> = vec![];
    for file in FILES {
        let id = file.trim_end_matches(".ttf");
        if !(gamma_missing && id.ends_with("gamma")) {
            routes.push((format!("/fonts/{id}/{file}"), Route::Body(font.clone())));
            routes.push((format!("/fonts/{id}/OFL.txt"), Route::Body(b"OFL".to_vec())));
        }
    }
    let files = Server::start(routes.iter().map(|(p, r)| (p.as_str(), r.clone())).collect());
    let mut list = String::from("# test list\n");
    for file in FILES {
        let id = file.trim_end_matches(".ttf");
        list.push_str(&format!(
            "{{\"id\":\"{id}\",\"name_en\":\"{id} en\",\"name_ja\":\"{id} ja\",\"file\":\"{file}\",\"size\":{},\"url\":\"{}\",\"license\":\"SIL OFL 1.1\",\"license_url\":\"{}\"}}\n",
            font.len(),
            files.url(&format!("/fonts/{id}/{file}")),
            files.url(&format!("/fonts/{id}/OFL.txt")),
        ));
    }
    let lister = Server::start(vec![("/font-list.txt", Route::Body(list.into_bytes()))]);
    // The two servers live as long as the returned one: keep the file server in a thread-local holder.
    HOLD.with(|h| h.borrow_mut().push(files));
    lister
}

thread_local! {
    static HOLD: std::cell::RefCell<Vec<Server>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn release() {
    HOLD.with(|h| h.borrow_mut().clear());
}

fn use_list(server: &Server) {
    // Scenarios run one at a time on the main thread: nothing else reads the variable meanwhile.
    unsafe { std::env::set_var("IBIS_FONT_LIST_URL", server.url("/font-list.txt")) };
}

fn forget() {
    unsafe { std::env::remove_var("IBIS_FONT_LIST_URL") };
    for file in FILES {
        font_setup::set_skipped(file, false);
        let id = file.trim_end_matches(".ttf");
        let folder = FontEntry { id: id.into(), name_en: String::new(), name_ja: String::new(), file: file.into(), size: 0, url: String::new(), license: String::new(), license_url: String::new(), sha256: None };
        if let Some(dir) = font_setup::font_folder(&folder) {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
    crate::shell::settings::remove(font_setup::CHECKED_KEY);
    release();
}

fn installed(file: &str) -> bool {
    let id = file.trim_end_matches(".ttf");
    ibis_office_base::app_data::fonts_dir().is_some_and(|d| d.join(id).join(file).exists())
}

/// Wait for what the dialog runs in the background (the list being read, the downloads): the scenario's clock is simulated, so
/// it is advanced, and the real threads of the transfers get real time.
fn wait_idle(s: &mut Scenario) {
    for _ in 0..400 {
        let (_, running, loading) = s.with(|e, _| e.font_setup_status());
        if !running && !loading {
            return;
        }
        s.wait(150);
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    panic!("the dialog still works after a minute");
}

fn press(s: &mut Scenario, id: &str) {
    let _probe = button_probe::Probe::new();
    s.update(|_, _, cx| cx.notify());
    button_probe::click(s, id);
    s.settle();
}

fn open_from_the_menu(s: &mut Scenario) {
    s.menu("help.setup");
    wait_idle(s);
}

fn help_menu_lists() {
    forget();
    let server = serve_with_list(false);
    use_list(&server);
    // One font was skipped before, one is installed.
    font_setup::set_skipped(FILES[0], true);
    let mut s = Scenario::start_view();
    assert_eq!(s.menu_state("help.setup").map(|(enabled, _)| enabled), Some(true), "the Help menu has Setup, and it is enabled");
    open_from_the_menu(&mut s);
    assert!(s.with(|e, _| e.font_setup_open()), "the menu item opens the dialog");
    // The skipped font is last and off; the others are on, in the list's order.
    let rows = s.with(|e, _| e.font_setup_rows());
    assert_eq!(rows, vec![(FILES[1].into(), true, false), (FILES[2].into(), true, false), (FILES[0].into(), false, true)], "{rows:?}");
    s.screenshot("font-setup");
    crate::shell::i18n::show(crate::shell::i18n::Language::English);
    s.update(|_, _, cx| cx.notify());
    s.screenshot("font-setup-en");
    crate::shell::i18n::show(crate::shell::i18n::Language::Japanese);
    drop(s);
    forget();
}

fn header_toggle() {
    forget();
    let server = serve_with_list(false);
    use_list(&server);
    font_setup::set_skipped(FILES[0], true);
    let mut s = Scenario::start_view();
    open_from_the_menu(&mut s);
    let checked = |s: &mut Scenario| s.with(|e, _| e.font_setup_rows()).into_iter().map(|r| r.1).collect::<Vec<_>>();
    assert_eq!(checked(&mut s), vec![true, true, false], "the skipped font is off by default");
    // The heading's check box: mixed -> all on -> all off -> all on.
    s.update(|e, _, cx| e.font_setup_click_check(None, cx));
    assert_eq!(checked(&mut s), vec![true, true, true]);
    s.update(|e, _, cx| e.font_setup_click_check(None, cx));
    assert_eq!(checked(&mut s), vec![false, false, false]);
    s.update(|e, _, cx| e.font_setup_click_check(None, cx));
    assert_eq!(checked(&mut s), vec![true, true, true]);
    // A row's own check box turns that font only.
    s.update(|e, _, cx| e.font_setup_click_check(Some(FILES[1]), cx));
    assert_eq!(checked(&mut s), vec![false, true, true]);
    drop(s);
    forget();
}

fn skip_records() {
    forget();
    let server = serve_with_list(false);
    use_list(&server);
    let mut s = Scenario::start_view();
    open_from_the_menu(&mut s);
    // One row is off: 「インストールしない」 records the rows the table has now, whatever their boxes say.
    s.update(|e, _, cx| e.font_setup_click_check(Some(FILES[1]), cx));
    press(&mut s, "font-setup-skip");
    assert!(!s.with(|e, _| e.font_setup_open()), "the dialog closes");
    for file in FILES {
        assert!(font_setup::is_skipped(file), "{file} is recorded as skipped");
        assert!(!installed(file), "nothing was downloaded");
    }
    // Opened again later (ヘルプ > セットアップ): the same rows, all off.
    open_from_the_menu(&mut s);
    let rows = s.with(|e, _| e.font_setup_rows());
    assert_eq!(rows.iter().map(|r| (r.1, r.2)).collect::<Vec<_>>(), vec![(false, true); 3], "{rows:?}");
    drop(s);
    forget();
}

fn install_downloads() {
    forget();
    let server = serve_with_list(false);
    use_list(&server);
    font_setup::set_skipped(FILES[0], true);
    let mut s = Scenario::start_view();
    open_from_the_menu(&mut s);
    // beta and gamma are on, alpha (skipped before) is off: only the two are downloaded.
    press(&mut s, "font-setup-install");
    wait_idle(&mut s);
    assert!(installed(FILES[1]) && installed(FILES[2]), "the checked fonts are installed");
    assert!(!installed(FILES[0]), "the unchecked one is not");
    let dir = ibis_office_base::app_data::fonts_dir().unwrap();
    assert_eq!(std::fs::read(dir.join("k1-beta").join("OFL.txt")).unwrap(), b"OFL", "the licence file is beside the font");
    let found = ibis_office_base::app_data::downloaded_font_files();
    assert!(found.iter().any(|p| p.ends_with("k1-beta/k1-beta.ttf")), "the render crate would load it at the next start: {found:?}");
    // And it is given to the text system at once (no restart): the font names grew (the user's question, 2026-11-12).
    assert!(crate::render::shape_view::fonts_generation() > 0, "the fonts were added while the app runs");
    assert!(crate::render::shape_view::system_font_names().is_some_and(|n| n.contains("Carlito")), "the downloaded font's family is known now");
    // The installed fonts are gone from the table; the one left out is now recorded as skipped (the dialog does not ask again).
    let (message, running, _) = s.with(|e, _| e.font_setup_status());
    assert!(!running && message.is_some(), "the dialog says it is done: {message:?}");
    assert!(font_setup::is_skipped(FILES[0]), "the font that was left unchecked is skipped from now");
    assert!(!font_setup::is_skipped(FILES[1]));
    s.screenshot("font-setup-done");
    drop(s);
    forget();
}

fn failed_download() {
    forget();
    let server = serve_with_list(true);
    use_list(&server);
    let mut s = Scenario::start_view();
    open_from_the_menu(&mut s);
    press(&mut s, "font-setup-install");
    wait_idle(&mut s);
    // gamma's file is a 404: the other two are installed, the dialog stays open with the failed one and says so.
    assert!(installed(FILES[0]) && installed(FILES[1]) && !installed(FILES[2]));
    assert!(s.with(|e, _| e.font_setup_open()), "the dialog stays open");
    let rows = s.with(|e, _| e.font_setup_rows());
    assert_eq!(rows.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), vec![FILES[2]], "the failed font is still offered: {rows:?}");
    let (message, _, _) = s.with(|e, _| e.font_setup_status());
    assert!(message.as_deref().is_some_and(|m| m.contains("k1-gamma")), "the failure names the font: {message:?}");
    assert!(!font_setup::is_skipped(FILES[2]), "a failure is not a skip: it is asked about again");
    let id = FILES[2].trim_end_matches(".ttf");
    let leftovers = std::fs::read_dir(ibis_office_base::app_data::fonts_dir().unwrap().join(id)).map(|d| d.flatten().filter(|f| f.file_name().to_string_lossy().ends_with(".part")).count()).unwrap_or(0);
    assert_eq!(leftovers, 0, "no unfinished file");
    s.screenshot("font-setup-failed");
    drop(s);
    forget();
}

fn cancel_download() {
    forget();
    // A font that stalls after 1 KB.
    let stall = Server::start(vec![("/stall.ttf", Route::Stalls { total: 1 << 22 }), ("/OFL.txt", Route::Body(b"OFL".to_vec()))]);
    let list = format!(
        "{{\"id\":\"k1-alpha\",\"name_en\":\"A\",\"file\":\"k1-alpha.ttf\",\"size\":{},\"url\":\"{}\",\"license_url\":\"{}\"}}\n",
        1 << 22,
        stall.url("/stall.ttf"),
        stall.url("/OFL.txt")
    );
    let lister = Server::start(vec![("/font-list.txt", Route::Body(list.into_bytes()))]);
    use_list(&lister);
    let mut s = Scenario::start_view();
    open_from_the_menu(&mut s);
    press(&mut s, "font-setup-install");
    // It runs: the action bar has 取り消し.
    for _ in 0..40 {
        s.wait(150);
        std::thread::sleep(std::time::Duration::from_millis(20));
        if s.with(|e, _| e.font_setup_status()).1 {
            break;
        }
    }
    assert!(s.with(|e, _| e.font_setup_status()).1, "a download is running");
    press(&mut s, "font-setup-cancel");
    wait_idle(&mut s);
    assert!(!installed(FILES[0]), "nothing is installed after a cancel");
    let part = ibis_office_base::app_data::fonts_dir().unwrap().join("k1-alpha").join("k1-alpha.ttf.part");
    assert!(!part.exists(), "the unfinished file is removed");
    assert!(s.with(|e, _| e.font_setup_open()), "the dialog stays");
    assert_eq!(s.with(|e, _| e.font_setup_rows()).len(), 1, "the font is still offered");
    assert!(!font_setup::is_skipped(FILES[0]));
    drop(s);
    drop((stall, lister));
    forget();
}

fn list_unreadable() {
    forget();
    // Nothing listens at this address.
    let gone = Server::start(vec![]);
    let url = gone.url("/font-list.txt");
    drop(gone);
    std::thread::sleep(std::time::Duration::from_millis(250));
    unsafe { std::env::set_var("IBIS_FONT_LIST_URL", url) };
    let mut s = Scenario::start_view();
    open_from_the_menu(&mut s);
    assert!(s.with(|e, _| e.font_setup_open()), "the dialog opens and says what happened");
    let (message, running, loading) = s.with(|e, _| e.font_setup_status());
    assert!(!running && !loading);
    assert!(message.is_some(), "the line under the table tells the list could not be read");
    assert!(s.with(|e, _| e.font_setup_rows()).is_empty());
    s.screenshot("font-setup-list-failed");
    press(&mut s, "font-setup-close");
    assert!(!s.with(|e, _| e.font_setup_open()));
    drop(s);
    forget();
}

fn startup_check() {
    forget();
    let server = serve_with_list(false);
    use_list(&server);
    let mut s = Scenario::start_view();
    // A version that has asked already: nothing opens.
    crate::shell::settings::set(font_setup::CHECKED_KEY, crate::shell::VERSION);
    s.with_app(|cx| crate::shell::font_setup_dialog::start(cx));
    for _ in 0..10 {
        s.wait(500);
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    assert!(!s.with(|e, _| e.font_setup_open()), "no question when this version has asked");
    // The first start of a version (the migration forgot it): the dialog opens by itself, and the version is recorded.
    crate::shell::settings::remove(font_setup::CHECKED_KEY);
    s.with_app(|cx| crate::shell::font_setup_dialog::start(cx));
    for _ in 0..40 {
        s.wait(500);
        std::thread::sleep(std::time::Duration::from_millis(15));
        if s.with(|e, _| e.font_setup_open()) {
            break;
        }
    }
    assert!(s.with(|e, _| e.font_setup_open()), "the startup check opens the dialog for a font that is neither installed nor skipped");
    assert_eq!(crate::shell::settings::get(font_setup::CHECKED_KEY).as_deref(), Some(crate::shell::VERSION));
    // 「インストールしない」: every font is skipped, so the next version's check has nothing new to ask about.
    press(&mut s, "font-setup-skip");
    let entries = font_setup::fetch_list().expect("the list");
    assert!(!font_setup::has_new(&entries, |e| font_setup::is_installed(e), |e| font_setup::is_skipped(&e.file)), "nothing new is left to ask about");
    crate::shell::settings::remove(font_setup::CHECKED_KEY);
    s.with_app(|cx| crate::shell::font_setup_dialog::start(cx));
    for _ in 0..10 {
        s.wait(500);
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    assert!(!s.with(|e, _| e.font_setup_open()), "all skipped: the check reads the list and asks nothing");
    drop(s);
    forget();
}
