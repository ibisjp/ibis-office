// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0076: the menus on iOS and Android (design: `tasks/doc/test-design/K0076.md`; the numbers M1... and D1... are that document's).
//! (1) no 終了 (the app is not quit from a menu: iOS froze), (2) ファイル > 閉じる reads 「戻る」 and goes back to the start view as the title
//! row's ＜ does (K0067), (3) no shortcut is drawn in a menu line (the keys still work, with a keyboard). A computer and the browser keep
//! their menus as they are (`all()`: these pass before the fix and must keep passing).
//!
//! A scenario on a Mac or a PC takes the mobile OS with `mobile_os::ForcedMobileOs` (`cfg(target_os = "ios")` cannot be had here). That the
//! phone's own OS does the same (the real build, the real touch, the real freeze) is for the simulators: the design's section 6. No scenario
//! here may be taken as proof of those.
use crate::docx::ext::*;
use crate::shell::chrome::{MenuEntry, MenuId, Menus};
use crate::shell::i18n::{self, Language};
use crate::shell::mobile_os::ForcedMobileOs;
use crate::shell::scenario::Scenario;
use ibis_formats::Kind;

const KINDS: [Kind; 3] = [Kind::Document, Kind::Workbook, Kind::Presentation];

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0076_d1_a_computer_keeps_exit_in_the_file_menu", d1_computer_keeps_exit),
        ("shell::k0076_d2_a_computer_keeps_close_its_words_its_key_and_the_window_closing", d2_computer_keeps_close),
        ("shell::k0076_d3_a_finger_on_a_computer_does_not_make_it_a_phone", d3_finger_on_a_computer),
        ("shell::k0076_d4_a_computer_keeps_the_shortcuts_in_its_menus", d4_computer_keeps_shortcuts),
        ("shell::k0076_d5_a_computer_quits_with_the_key_as_before", d5_computer_quits_with_the_key),
        ("shell::k0076_d6_the_start_window_of_a_computer_keeps_exit", d6_computer_start_window_keeps_exit),
        ("shell::k0076_m1_no_exit_line_in_any_menu_of_the_three_screens", m1_no_exit),
        ("shell::k0076_m2_no_exit_line_in_the_start_view_menus", m2_no_exit_in_start_menus),
        ("shell::k0076_m3_the_quit_key_does_not_end_the_app", m3_quit_key),
        ("shell::k0076_m4_close_reads_back_in_both_languages", m4_close_reads_back),
        ("shell::k0076_m5_back_goes_to_the_start_view_in_the_same_window", m5_back_goes_to_start),
        ("shell::k0076_m6_back_asks_before_losing_edits_and_goes_on_with_the_answer", m6_back_asks),
        ("shell::k0076_m7_the_close_key_does_the_same_as_the_line", m7_close_key),
        ("shell::k0076_m8_no_menu_line_shows_a_shortcut", m8_no_shortcut_shown),
        ("shell::k0076_m9_the_keys_still_work_without_being_shown", m9_keys_still_work),
        ("shell::k0076_m10_every_drawing_of_a_shortcut_goes_through_one_function", m10_one_gate),
        ("shell::k0076_m11_back_is_the_same_as_the_title_rows_arrow", m11_same_as_the_arrow),
        ("shell::k0076_m12_the_menu_search_finds_back", m12_search_finds_back),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

fn document(kind: Kind) -> Scenario {
    if kind == Kind::Document {
        // As `issue234_backspace_pages.rs` makes one: a blank document loaded directly has no paragraph to put the caret in.
        let mut s = Scenario::start_view();
        s.menu("file.new.document");
        s.key("cmd-8");
        s.size(1000., 700.);
        s.settle();
        let first = s.with(|e, _| e.flow().and_then(|f| f.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None })));
        if let Some(first) = first {
            s.update(|e, _, cx| e.set_caret(ibis_doc_model::FlowPos::new(first, 0), false, cx));
        }
        return s;
    }
    let mut s = Scenario::new_document(kind);
    s.size(1000., 700.);
    s.settle();
    s
}

/// One undoable edit of the screen's own: typed text in a document, a cell entered in a workbook, a new slide in a presentation (typing alone
/// edits nothing in a presentation with no shape selected, and a workbook's cell is not committed until Enter).
fn edit(s: &mut Scenario, kind: Kind) {
    match kind {
        Kind::Document => s.type_text("abc"),
        Kind::Workbook => {
            s.type_text("abc");
            s.key("enter");
        }
        _ => {
            s.menu("pptx.new-slide");
            s.settle();
        }
    }
}

/// Every line (id, label, shortcut) of `menus`, submenus included.
fn lines(entries: &[MenuEntry], out: &mut Vec<(&'static str, String, Option<&'static str>)>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(i) => out.push((i.id, i.label.to_string(), i.shortcut)),
            MenuEntry::Submenu(sub) => lines(&sub.items, out),
            _ => {}
        }
    }
}

fn all_lines(menus: &Menus) -> Vec<(&'static str, String, Option<&'static str>)> {
    let mut out = Vec::new();
    for (_, entries) in &menus.menus {
        lines(entries, &mut out);
    }
    out
}

/// The lines of the three places a screen's menu lines come from: the menus (keys and search), the menu bar's, the start view's.
fn lines_of_the_editor(s: &mut Scenario) -> Vec<(&'static str, String, Option<&'static str>)> {
    s.with(|e, _| {
        let mut out = all_lines(&e.menus());
        out.extend(all_lines(&e.menu_bar_menus()));
        out
    })
}

fn start_lines(s: &mut Scenario) -> Vec<(&'static str, String, Option<&'static str>)> {
    s.with(|e, _| {
        let mut out = Vec::new();
        for (_, entries) in e.start_menus() {
            lines(&entries, &mut out);
        }
        out
    })
}

// ---- iOS / Android (before the fix these fail) ----

/// M1 (K0076 1): 終了 is in no menu of a document, workbook or presentation (the menus, the menu bar's, the menu search's lines).
fn m1_no_exit() {
    let _mobile = ForcedMobileOs::new();
    for kind in KINDS {
        let mut s = document(kind);
        let lines = lines_of_the_editor(&mut s);
        assert!(!lines.iter().any(|(id, ..)| *id == "file.quit"), "{kind:?}: a mobile OS has no 終了 line");
        let searched = s.with(|e, _| e.command_search_items().into_iter().any(|i| i.id == "file.quit"));
        assert!(!searched, "{kind:?}: and the menu search does not find it");
        assert!(lines.iter().any(|(id, ..)| *id == "file.close"), "{kind:?}: the line before it (the back line) is still there");
    }
}

/// M2 (K0076 1): the start view's File menu has no 終了 either (iOS and Android are not `macos`/`wasm`, so they draw that bar).
fn m2_no_exit_in_start_menus() {
    let _mobile = ForcedMobileOs::new();
    let mut s = Scenario::start_view();
    let lines = start_lines(&mut s);
    assert!(lines.iter().any(|(id, ..)| *id == "file.open"), "setup: the start menus are made");
    assert!(!lines.iter().any(|(id, ..)| *id == "file.quit"), "the start view's File menu has no 終了: {lines:?}");
    assert!(!lines.iter().any(|(id, ..)| *id == "file.close-window"), "nor ［ウィンドウを閉じる］ (closing the window ends the app): {lines:?}");
}

/// M3 (K0076 1): the key that quits (⌘Q / Ctrl+Q on a keyboard of an iPad or a tablet) does not end the app either, in a window with no edits
/// and in a window with edits not saved (nothing is asked, nothing ends): the freeze came from ending the app.
fn m3_quit_key() {
    let _mobile = ForcedMobileOs::new();
    for kind in KINDS {
        let mut s = document(kind);
        s.quit_with_last_window();
        s.key("cmd-q");
        assert!(!s.quit_requested(), "{kind:?}: ⌘Q does not end the app");
        assert_eq!(s.window_count(), 1, "{kind:?}: the window stays");
        assert!(s.with(|e, _| e.has_document && !e.save_confirm_dialog_open), "{kind:?}: the file stays open and nothing is asked");
    }
}

/// M4 (K0076 2): ファイル > 閉じる is 「戻る」 (English: Back) in every place its line is drawn from, in both languages; it is the one line (the
/// id stays `file.close`: the commands' names do not change).
fn m4_close_reads_back() {
    let _mobile = ForcedMobileOs::new();
    for (language, back, was) in [(Language::Japanese, "戻る", "閉じる"), (Language::English, "Back", "Close")] {
        for kind in KINDS {
            let mut s = document(kind);
            i18n::show(language); // after the scenario is made: it shows Japanese when it starts
            let found: Vec<String> = lines_of_the_editor(&mut s).into_iter().filter(|(id, ..)| *id == "file.close").map(|(_, label, _)| label).collect();
            assert!(!found.is_empty() && found.iter().all(|l| l == back), "{kind:?} {language:?}: the line reads {back} (not {was}): {found:?}");
        }
    }
    i18n::show(Language::Japanese);
}

/// M5 (K0076 2): the line, with nothing unsaved, shows the start view in the same window: the file is closed, the window stays (not
/// removed: it is the only window of the app), nothing is asked, and the app is not told to quit.
fn m5_back_goes_to_start() {
    let _mobile = ForcedMobileOs::new();
    for kind in KINDS {
        let mut s = document(kind);
        s.quit_with_last_window();
        s.menu("file.close");
        s.settle();
        assert!(!s.with(|e, _| e.has_document), "{kind:?}: the start view is shown");
        assert_eq!(s.window_count(), 1, "{kind:?}: in the same window");
        assert!(!s.with(|e, _| e.save_confirm_dialog_open), "{kind:?}: nothing was asked");
        assert!(!s.quit_requested(), "{kind:?}: the app is not ended");
    }
}

/// M6 (K0076 2): with edits not saved the line asks first (保存 / 保存しない / キャンセル): キャンセル keeps the file and its edits;
/// 保存しない goes to the start view in the same window (the window does not go: `close_window_after_confirm` is false).
fn m6_back_asks() {
    let _mobile = ForcedMobileOs::new();
    for kind in KINDS {
        let mut s = document(kind);
        s.quit_with_last_window();
        edit(&mut s, kind);
        assert!(s.with(|e, _| e.is_dirty()), "{kind:?}: setup: an edit");
        s.menu("file.close");
        s.settle();
        assert!(s.with(|e, _| e.has_document && e.save_confirm_dialog_open), "{kind:?}: it asks, the file stays until it is answered");
        assert!(!s.with(|e, _| e.close_window_after_confirm), "{kind:?}: the answer ends in the start view, not in the window going");
        s.update(|e, _, cx| e.close_save_confirm_dialog(cx));
        assert!(s.with(|e, _| e.has_document && e.is_dirty() && !e.save_confirm_dialog_open), "{kind:?}: キャンセル keeps the file and the edit");
        s.menu("file.close");
        s.update(|e, window, cx| e.confirm_discard_and_close(window, cx));
        s.settle();
        assert!(!s.with(|e, _| e.has_document), "{kind:?}: 保存しない: the start view");
        assert_eq!(s.window_count(), 1, "{kind:?}: in the same window");
        assert!(!s.quit_requested(), "{kind:?}");
    }
}

/// M7 (K0076 3 and 2): ⌘W / Ctrl+W (a keyboard) does what the line does, though no key is shown.
fn m7_close_key() {
    let _mobile = ForcedMobileOs::new();
    for kind in KINDS {
        let mut s = document(kind);
        s.key("cmd-w");
        assert!(!s.with(|e, _| e.has_document), "{kind:?}: ⌘W shows the start view");
        assert_eq!(s.window_count(), 1, "{kind:?}: in the same window");
    }
}

/// M8 (K0076 3): no line of any menu shows a shortcut (`chrome::shown_shortcut`, which every menu line's drawing and measuring uses: M10),
/// in the menus, the menu bar's and the start view's; and there are lines that have one (the test is not empty).
fn m8_no_shortcut_shown() {
    let _mobile = ForcedMobileOs::new();
    for kind in KINDS {
        let mut s = document(kind);
        let mut all = lines_of_the_editor(&mut s);
        all.extend(start_lines(&mut s));
        let with_key: Vec<_> = all.iter().filter_map(|(id, _, key)| key.map(|k| (*id, k))).collect();
        assert!(with_key.len() >= 10, "{kind:?}: setup: the lines have shortcuts to hide: {}", with_key.len());
        let shown: Vec<_> = with_key.iter().filter(|(_, key)| crate::shell::chrome::shown_shortcut(key).is_some()).collect();
        assert!(shown.is_empty(), "{kind:?}: shortcuts shown on a mobile OS: {shown:?}");
    }
}

/// M9 (K0076 3): the keys are found through the menus' `shortcut`s (`run_menu_shortcut`), so hiding the text must not take them away: the
/// lines keep their shortcuts, and ⌘Z undoes, ⌘Shift+Z redoes, ⌘S saves (a file, as the line does).
fn m9_keys_still_work() {
    let desktop: Vec<usize> = KINDS
        .iter()
        .map(|&kind| {
            let mut s = document(kind);
            lines_of_the_editor(&mut s).iter().filter(|(id, _, key)| key.is_some() && *id != "file.quit").count()
        })
        .collect();
    let _mobile = ForcedMobileOs::new();
    for (n, kind) in KINDS.into_iter().enumerate() {
        let mut s = document(kind);
        let with_key = lines_of_the_editor(&mut s).iter().filter(|(id, _, key)| key.is_some() && *id != "file.quit").count();
        assert_eq!(with_key, desktop[n], "{kind:?}: the lines keep the shortcuts the keys are found by");
        edit(&mut s, kind);
        assert!(s.with(|e, _| e.document.can_undo()), "{kind:?}: setup");
        s.key("cmd-z");
        assert!(s.with(|e, _| e.document.can_redo()), "{kind:?}: ⌘Z undoes");
        s.key("cmd-shift-z");
        assert!(s.with(|e, _| !e.document.can_redo()), "{kind:?}: ⇧⌘Z redoes");
        s.key("cmd-s");
        s.settle();
        assert!(s.with(|e, _| !e.is_dirty() || e.save_confirm_dialog_open || e.has_document), "{kind:?}: ⌘S reaches the save");
    }
}

/// M10 (K0076 3): the places that draw or measure a menu line's shortcut all go through `chrome::shown_shortcut`; none calls
/// `format_shortcut_for_platform` itself (its definition, `shown_shortcut`, and the button tooltip's one, which is no menu line, are the three).
/// A source check, as `clock_rule.rs` is: the scenarios cannot read the drawn text.
fn m10_one_gate() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ibis-office-shell/src");
    let mut total = 0;
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).expect("the shell's source") {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "tests") {
                    stack.push(path);
                }
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                total += text.matches("format_shortcut_for_platform(").count();
            }
        }
    }
    assert!(total <= 3, "{total} uses of format_shortcut_for_platform: a menu line's shortcut is drawn through shown_shortcut");
}

/// M11 (K0076 2): the line and the title row's ＜ (K0067) leave the same state, with and without unsaved edits.
fn m11_same_as_the_arrow() {
    let _mobile = ForcedMobileOs::new();
    for dirty in [false, true] {
        let mut states = Vec::new();
        for by_line in [true, false] {
            let mut s = document(Kind::Workbook);
            if dirty {
                edit(&mut s, Kind::Workbook);
            }
            if by_line {
                s.menu("file.close");
            } else {
                s.update(|e, window, cx| e.back_to_start_on(true, window, cx));
            }
            s.settle();
            states.push(s.with(|e, _| (e.has_document, e.save_confirm_dialog_open, e.close_window_after_confirm, e.is_dirty())));
            assert_eq!(s.window_count(), 1);
        }
        assert_eq!(states[0], states[1], "dirty={dirty}: the line and the ＜ end the same");
    }
}

/// M12 (asked by main with K0076; design question 3): the menu search finds the line by what it shows, 「戻る」 (English: Back), and no longer
/// by 「閉じる」 on a mobile OS.
fn m12_search_finds_back() {
    let _mobile = ForcedMobileOs::new();
    for kind in KINDS {
        let mut s = document(kind);
        let labels: Vec<String> = s.with(|e, _| e.command_search_items().into_iter().filter(|i| i.id == "file.close").map(|i| i.label.to_string()).collect());
        assert!(!labels.is_empty() && labels.iter().all(|l| l == "戻る"), "{kind:?}: the search lists the line as 戻る: {labels:?}");
    }
}

// ---- computers (and the browser): unchanged; these pass now ----

/// D1: a computer's File menu keeps 終了 as its last line (on every OS but the browser, which has no app to quit).
fn d1_computer_keeps_exit() {
    for kind in KINDS {
        let mut s = document(kind);
        let lines = lines_of_the_editor(&mut s);
        assert!(lines.iter().any(|(id, ..)| *id == "file.quit"), "{kind:?}: 終了 is in the File menu");
        let file: Vec<&str> = s.with(|e, _| e.menus().menus.iter().find(|(m, _)| *m == MenuId::File).map(|(_, v)| v.iter().filter_map(|x| if let MenuEntry::Item(i) = x { Some(i.id) } else { None }).collect::<Vec<_>>()).unwrap_or_default()).into_iter().collect();
        // The lines only one screen has (the document's 「未保存のファイルの回復」) may come after it; it is after 閉じる.
        let at = |id: &str| file.iter().position(|x| *x == id);
        assert!(at("file.quit") > at("file.close") && at("file.close").is_some(), "{kind:?}: it follows 閉じる: {file:?}");
    }
}

/// D2: on a computer ファイル > 閉じる still reads 「閉じる」 / Close, still has ⌘W (Ctrl+W), and still closes the window.
fn d2_computer_keeps_close() {
    for (language, close) in [(Language::Japanese, "閉じる"), (Language::English, "Close")] {
        let mut s = document(Kind::Document);
        i18n::show(language); // after the scenario is made: it shows Japanese when it starts
        let found: Vec<(String, Option<&str>)> = lines_of_the_editor(&mut s).into_iter().filter(|(id, ..)| *id == "file.close").map(|(_, label, key)| (label, key)).collect();
        assert!(!found.is_empty() && found.iter().all(|(l, k)| l == close && *k == Some("⌘W")), "{language:?}: {found:?}");
    }
    i18n::show(Language::Japanese);
    let mut s = document(Kind::Workbook);
    s.menu("file.close");
    s.settle();
    assert_eq!(s.window_count(), 0, "the window closes (not the start view in it)");
}

/// D3: a finger on a computer's window (K0067's `touch_seen`) shows the ＜ but does not turn the menus into a phone's.
fn d3_finger_on_a_computer() {
    let mut s = document(Kind::Document);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    let lines = lines_of_the_editor(&mut s);
    assert!(lines.iter().any(|(id, ..)| *id == "file.quit"), "still has 終了");
    assert!(lines.iter().any(|(id, label, key)| *id == "file.close" && label == "閉じる" && key.is_some()), "still 閉じる with its key");
    assert!(crate::shell::chrome::shown_shortcut("⌘W").is_some());
    s.menu("file.close");
    s.settle();
    assert_eq!(s.window_count(), 0, "still closes the window");
}

/// D4: a computer shows its shortcuts (as `Ctrl+W` or `⌘W` for its OS).
fn d4_computer_keeps_shortcuts() {
    let shown = crate::shell::chrome::shown_shortcut("⌘W").expect("a computer shows the key");
    assert_eq!(shown, if cfg!(target_os = "macos") { "⌘W" } else { "Ctrl+W" });
    let mut s = document(Kind::Document);
    let with_key = lines_of_the_editor(&mut s).iter().filter(|(_, _, key)| key.is_some()).count();
    assert!(with_key >= 10, "{with_key}");
}

/// D5: ⌘Q on a computer ends the app (with nothing unsaved) as before.
fn d5_computer_quits_with_the_key() {
    let mut s = document(Kind::Document);
    s.quit_with_last_window();
    s.key("cmd-q");
    assert!(s.quit_requested(), "⌘Q ends the app");
}

/// D6: the start window of a computer keeps 終了 (Windows and Linux's bar; the browser has none).
fn d6_computer_start_window_keeps_exit() {
    let mut s = Scenario::start_view();
    let lines = start_lines(&mut s);
    assert!(lines.iter().any(|(id, ..)| *id == "file.quit"), "{lines:?}");
}
