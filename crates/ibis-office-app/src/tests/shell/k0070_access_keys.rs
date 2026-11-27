// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0070: the main menu's access keys on Windows (design: `tasks/doc/test-design/K0070.md`; the numbers T1... are that document's).
//! A user saw `F` underlined on the File menu and Alt+F did nothing (0.8.9 for Windows). These are the requirements, written before the
//! fix. Run before the fix, T2, T4 and T6 failed (T3's last expectation was wrong and is corrected); all are in `all()` now.
//!
//! The scenarios send the keys as GPUI gets them from Windows (`key_char` given, the modifiers as separate events), not only the
//! `"alt-f"` shorthand that leaves `key_char` empty. They do not go through the Windows message layer: that is checked on a real Windows
//! PC (`tasks/doc/requests/2026-11-25-k0070-windows-access-keys.md`), and no scenario here may be taken as proof of it.
use super::Scenario;
use crate::shell::access_key;
use crate::shell::chrome::MenuId;
use crate::shell::i18n::{self, Language};
use crate::shell::menu_bar::{BarMenu, OVERFLOW_IX};
use gpui_kit::{Keystroke, Modifiers};
use crate::docx::scenarios::DocSteps;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0070_t1_alt_f_opens_the_file_menu_on_every_screen_and_language", t1_alt_f_opens_file),
        ("shell::k0070_t2_alt_f_opens_the_file_menu_of_the_start_window", t2_start_window),
        ("shell::k0070_t3_alt_f_while_editing_keeps_the_text_and_esc_returns_to_it", t3_while_editing),
        ("shell::k0070_t4_alt_pressed_and_released_alone_enters_the_menu_bar", t4_alt_alone),
        ("shell::k0070_t5_alt_then_letter_does_not_also_enter_bar_mode_on_release", t5_no_bar_mode_after_combo),
        ("shell::k0070_t6_open_menu_arrows_and_esc", t6_open_menu_navigation),
        ("shell::k0070_t7_underline_follows_alt", t7_underline_follows_alt),
        ("shell::k0070_t8_alt_f_is_not_taken_while_a_dialog_has_the_keys", t8_dialog_has_the_keys),
        ("shell::k0070_t9_alt_f_in_a_narrow_window", t9_narrow_window),
        ("shell::k0070_t10_alt_f_off_where_the_os_has_no_access_keys", t10_off_on_mac),
        ("shell::k0070_t11_recent_files_are_numbered_for_the_keyboard", t11_recent_files_numbered),
        ("shell::k0070_t12_an_app_shortcut_with_alt_then_alt_released_does_not_enter_bar_mode", t12_shortcut_then_alt_release),
        ("shell::k0070_t13_esc_in_a_submenu_closes_one_level", t13_esc_closes_one_level),
    ]
}

const FILES: [(&str, &str); 3] = [
    ("docx", "test/editing/docx-report.docx"),
    ("xlsx", "test/editing/xlsx-compatibility-v1.xlsx"),
    ("pptx", "test/benchmark/earnings-presentation.pptx"),
];

const LANGUAGES: [(&str, Language); 2] = [("ja", Language::Japanese), ("en", Language::English)];

fn alt() -> Modifiers {
    Modifiers { alt: true, ..Default::default() }
}

/// Alt+`letter`, as Windows delivers it: the modifier, then the key with its character.
fn alt_letter(s: &mut Scenario, letter: &str) {
    s.modifiers_changed(alt());
    s.key_stroke(Keystroke { modifiers: alt(), key: letter.into(), key_char: Some(letter.into()) });
}

fn alt_release(s: &mut Scenario) {
    s.modifiers_changed(Modifiers::default());
}

fn open_index(s: &mut Scenario) -> Option<usize> {
    s.with(|e, _| e.open_menu.as_ref().map(|o| o.index()))
}

fn file_index(s: &mut Scenario) -> usize {
    s.with(|e, _| e.bar_items().iter().position(|(m, _)| *m == BarMenu::Static(MenuId::File)).expect("the File menu is in the bar"))
}

/// T1: Alt+F opens File on the three screens, in both languages, with `key_char` given or not.
fn t1_alt_f_opens_file() {
    let _on = access_key::override_supported(true);
    for (lang_name, lang) in LANGUAGES {
        i18n::show(lang);
        for (name, file) in FILES {
            for with_char in [true, false] {
                let mut s = Scenario::open(file);
                s.size(1600., 900.);
                let file_ix = file_index(&mut s);
                if with_char {
                    alt_letter(&mut s, "f");
                } else {
                    s.key("alt-f");
                }
                assert_eq!(open_index(&mut s), Some(file_ix), "{name} {lang_name} (key_char {with_char}): Alt+F opens the File menu");
            }
        }
    }
    i18n::show(Language::Japanese);
}

/// T2: the start window (no file open) has a menu bar with File; Alt+F opens it.
fn t2_start_window() {
    let _on = access_key::override_supported(true);
    for (lang_name, lang) in LANGUAGES {
        i18n::show(lang);
        let mut s = Scenario::start_view();
        let file_ix = s.with(|e, _| e.bar_items().iter().position(|(m, _)| *m == BarMenu::Static(MenuId::File)));
        let file_ix = file_ix.unwrap_or_else(|| panic!("{lang_name}: the start window's bar has File"));
        alt_letter(&mut s, "f");
        assert_eq!(open_index(&mut s), Some(file_ix), "{lang_name}: Alt+F opens File in the start window");
    }
    i18n::show(Language::Japanese);
}

/// T3: with the caret in the text, Alt+F opens the menu (no `f` is typed); Esc closes it and typing goes on in the document.
fn t3_while_editing() {
    let _on = access_key::override_supported(true);
    let mut s = Scenario::open(FILES[0].1);
    s.size(1600., 900.);
    let (id, _) = s.paragraph(2);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    s.type_text("ab");
    let before = s.paragraph(2).1;
    let file_ix = file_index(&mut s);
    alt_letter(&mut s, "f");
    assert_eq!(open_index(&mut s), Some(file_ix), "Alt+F opens File while the caret is in the text");
    assert_eq!(s.paragraph(2).1, before, "no `f` is typed into the text");
    alt_release(&mut s);
    s.key("escape");
    assert!(open_index(&mut s).is_none(), "Esc closes the menu");
    s.type_text("c");
    assert_eq!(s.paragraph(2).1, format!("abc{}", &before[2..]), "typing goes on at the caret in the document after Esc");
}

/// T4: Alt pressed and released with no other key puts the focus on the menu bar (Windows: "ALT toggles in and out of menu bar mode",
/// https://learn.microsoft.com/en-us/windows/win32/menurc/about-menus); Alt again, or Esc, leaves it.
fn t4_alt_alone() {
    let _on = access_key::override_supported(true);
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        s.size(1600., 900.);
        s.modifiers_changed(alt());
        alt_release(&mut s);
        assert!(s.with(|e, _| e.bar_keys.focus.is_some()), "{name}: Alt alone puts the focus on the bar");
        s.modifiers_changed(alt());
        alt_release(&mut s);
        assert!(s.with(|e, _| e.bar_keys.focus.is_none()), "{name}: Alt again leaves the bar");
        s.modifiers_changed(alt());
        alt_release(&mut s);
        s.key("escape");
        assert!(s.with(|e, _| e.bar_keys.focus.is_none()), "{name}: Esc leaves the bar");
    }
}

/// T5: Alt+F opens the menu; releasing Alt afterwards must not toggle the bar mode (the menu stays as it is, no focus appears).
fn t5_no_bar_mode_after_combo() {
    let _on = access_key::override_supported(true);
    let mut s = Scenario::open(FILES[0].1);
    s.size(1600., 900.);
    let file_ix = file_index(&mut s);
    alt_letter(&mut s, "f");
    alt_release(&mut s);
    assert_eq!(open_index(&mut s), Some(file_ix), "the menu stays open after Alt is released");
    s.key("escape");
    alt_letter(&mut s, "z"); // a letter no menu has
    alt_release(&mut s);
    assert!(s.with(|e, _| e.bar_keys.focus.is_none()), "Alt+letter then release is not 'Alt alone'");
}

/// T6: with File open by Alt+F, → opens the next menu and ← comes back, ↓ and ↑ stay in the menu, Esc closes it (one Esc: the menu) and the
/// next Esc does nothing to the bar.
fn t6_open_menu_navigation() {
    let _on = access_key::override_supported(true);
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        s.size(1600., 900.);
        let file_ix = file_index(&mut s);
        alt_letter(&mut s, "f");
        alt_release(&mut s);
        assert_eq!(open_index(&mut s), Some(file_ix), "{name}: Alt+F opens File");
        s.key("right");
        let next = open_index(&mut s);
        assert!(next.is_some() && next != Some(file_ix), "{name}: → opens the next menu ({next:?})");
        s.key("left");
        assert_eq!(open_index(&mut s), Some(file_ix), "{name}: ← comes back to File");
        s.key("down");
        s.key("up");
        assert_eq!(open_index(&mut s), Some(file_ix), "{name}: ↓ and ↑ stay in the menu");
        s.key("escape");
        assert!(open_index(&mut s).is_none(), "{name}: Esc closes the menu");
    }
}

/// T7: the access letters of the bar are underlined while Alt is held and not after it is released (Windows hides them until Alt is pressed,
/// https://learn.microsoft.com/en-us/windows/win32/uxguide/inter-keyboard), and with the bar focused (F10 or Alt alone).
fn t7_underline_follows_alt() {
    let _on = access_key::override_supported(true);
    let mut s = Scenario::open(FILES[0].1);
    s.size(1600., 900.);
    assert!(!s.with(|e, _| e.bar_keys.alt_held), "no underline before Alt");
    s.modifiers_changed(alt());
    assert!(s.with(|e, _| e.bar_keys.alt_held), "Alt held: underlined");
    alt_release(&mut s);
    s.key("escape");
    assert!(!s.with(|e, _| e.bar_keys.alt_held), "Alt released: not underlined");
}

/// T8: while a message box or dialog has the keys, Alt+F does not open a menu behind it.
fn t8_dialog_has_the_keys() {
    let _on = access_key::override_supported(true);
    let mut s = Scenario::open(FILES[0].1);
    s.size(1600., 900.);
    s.update(|e, _, cx| e.show_alert("Title".into(), "Message".into(), cx));
    s.settle();
    alt_letter(&mut s, "f");
    assert!(open_index(&mut s).is_none(), "Alt+F opens no menu behind a message box");
    assert!(s.with(|e, _| e.alert.is_some()), "the message box is still there");
}

/// T9: in a window narrower than 720 px (K0061: the bars at the bottom, some menus folded into the ▾) Alt+F still opens File (through the ▾ when it
/// is folded).
fn t9_narrow_window() {
    let _on = access_key::override_supported(true);
    for width in [719., 500., 300.] {
        for (name, file) in FILES {
            let mut s = Scenario::open(file);
            s.size(width, 800.);
            s.settle();
            let file_ix = file_index(&mut s);
            alt_letter(&mut s, "f");
            let open = open_index(&mut s);
            assert!(open == Some(file_ix) || open == Some(OVERFLOW_IX), "{name} at {width}: Alt+F opens File (or the ▾ that holds it): {open:?}");
        }
    }
}

/// T10: where the OS has no access keys (macOS, the Web) Alt (Option) +F opens no menu and the bar does not change.
fn t10_off_on_mac() {
    let _off = access_key::override_supported(false);
    let mut s = Scenario::open(FILES[0].1);
    s.size(1600., 900.);
    alt_letter(&mut s, "f");
    alt_release(&mut s);
    assert!(open_index(&mut s).is_none(), "no menu opens");
    assert!(s.with(|e, _| e.bar_keys.focus.is_none() && !e.bar_keys.alt_held), "no bar mode and no underline");
}

/// T11 (the part that was missing, R10): the recent files of ファイル > 最近使ったファイル have the numbers 1, 2, 3, ... as their access keys, one
/// each, and the lines that are not files have no number.
fn t11_recent_files_numbered() {
    use crate::shell::chrome::{assign_access_keys, MenuEntry};
    let _on = access_key::override_supported(true);
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        s.update(|e, _, _| e.recent_files = ["docx-report.docx", "xlsx-sales.xlsx", "pptx-plan.pptx", "autospace-letters.docx"].iter().map(|f| std::path::PathBuf::from("test/editing").join(f)).collect());
        s.settle();
        let keys: Vec<Option<char>> = s.with(|e, _| {
            let file_menu = e.bar_items().into_iter().find(|(m, _)| *m == BarMenu::Static(MenuId::File)).expect("the File menu").1;
            let mut recent: Vec<MenuEntry> = file_menu
                .into_iter()
                .find_map(|entry| match entry {
                    MenuEntry::Submenu(sub) if sub.items.iter().any(|i| matches!(i, MenuEntry::Item(item) if item.id == "file.recent.item")) => Some(sub.items),
                    _ => None,
                })
                .expect("the recent files are a submenu of File");
            assign_access_keys(&mut recent);
            // The numbers of the lines that are recent files; any other line has no digit.
            let mut files = Vec::new();
            for e in &recent {
                let MenuEntry::Item(item) = e else { continue };
                let key = item.access.as_ref().and_then(|a| a.key);
                if item.id == "file.recent.item" {
                    files.push(key);
                } else {
                    assert!(!key.is_some_and(|k| k.is_ascii_digit()), "a line that is no file has a number: {}", item.id);
                }
            }
            files
        });
        assert_eq!(keys, vec![Some('1'), Some('2'), Some('3'), Some('4')], "{name}: the recent files are numbered");
    }
}

/// T12 (F1 of the Windows check): a key the app's own shortcut takes (a sheet's Alt+→, a document's Ctrl+Alt+F) with Alt held, and then Alt let go, is not "Alt alone": the
/// bar does not take the focus (the next key, an arrow or Enter, is the sheet's).
fn t12_shortcut_then_alt_release() {
    let _on = access_key::override_supported(true);
    let mut s = Scenario::open(FILES[1].1);
    s.size(1600., 900.);
    let sheet_before = s.with(|e, _| e.current_slide);
    s.modifiers_changed(alt());
    s.key_stroke(Keystroke { modifiers: alt(), key: "right".into(), key_char: None });
    let sheet_after = s.with(|e, _| e.current_slide);
    assert_ne!(sheet_before, sheet_after, "Alt+→ is the sheet's own shortcut (it went to the next sheet)");
    alt_release(&mut s);
    assert!(s.with(|e, _| e.bar_keys.focus.is_none()), "a shortcut with Alt, then Alt let go: the bar does not take the focus");
    // Alt alone, with no key, still does.
    s.modifiers_changed(alt());
    alt_release(&mut s);
    assert!(s.with(|e, _| e.bar_keys.focus.is_some()), "Alt pressed and let go alone still puts the focus on the bar");
}

/// T13 (F3): with a submenu open (File, then N for 新規作成), Esc closes the submenu only: the File menu is still open, and the next Esc closes it. (Windows' menus; the
/// macOS system menus close together.)
fn t13_esc_closes_one_level() {
    let _on = access_key::override_supported(true);
    let mut s = Scenario::open(FILES[0].1);
    s.size(1600., 900.);
    let file_ix = file_index(&mut s);
    alt_letter(&mut s, "f");
    alt_release(&mut s);
    assert_eq!(open_index(&mut s), Some(file_ix), "Alt+F opens File");
    s.type_text("n");
    assert_eq!(open_index(&mut s), Some(file_ix), "N opens the submenu (File stays open)");
    s.key("escape");
    assert_eq!(open_index(&mut s), Some(file_ix), "Esc in the submenu closes the submenu only: File is still open");
    s.key("escape");
    assert!(open_index(&mut s).is_none(), "the next Esc closes the menu");
}
