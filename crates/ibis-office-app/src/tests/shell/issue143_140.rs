// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issues #143 (the folder the first save of a new file opens in, and the last folder saved to) and #140 (a
//! document's right-click menu; 行番号 that could not be turned off).
use super::Scenario;
use crate::shell::chrome::MenuEntry;
use crate::shell::save_location;
use ibis_doc_model::{Block, FlowPos, LineNumberRestart};
use crate::docx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue143_last_save_folder_is_remembered", last_save_folder),
        ("shell::issue140_docx_right_click_menu", right_click_menu),
        ("shell::issue140_docx_line_numbers_off_in_every_section_and_round_trip", line_numbers),
        ("shell::issue140_docx_count_by_zero_or_missing_is_no_line_numbers", count_by_zero),
    ]
}

fn find<'a>(entries: &'a [MenuEntry], id: &str) -> Option<&'a MenuEntry> {
    for e in entries {
        match e {
            MenuEntry::Item(i) if i.id == id => return Some(e),
            MenuEntry::Submenu(s) => {
                if s.id == id {
                    return Some(e);
                }
                if let Some(found) = find(&s.items, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

fn enabled(entries: &[MenuEntry], id: &str) -> bool {
    match find(entries, id) {
        Some(MenuEntry::Item(i)) => i.enabled(),
        Some(_) => true,
        None => panic!("no menu line {id}"),
    }
}

fn text_screen_pos(s: &mut Scenario, pos: FlowPos) -> (f32, f32) {
    let (page, x, y, h) = s.update(|e, _, cx| {
        let lines = e.flow_lines();
        let (page, x, y, h) = e.caret_box(&lines, pos).expect("the position is laid out");
        e.scroll_into_view(page, y, h, cx);
        (page, x, y, h)
    });
    let (origin, k) = s.with(|e, _| e.page_origin(page)).expect("the page is placed");
    (f32::from(origin.x) + x * k + 0.5, f32::from(origin.y) + (y + h / 2.0) * k)
}

/// #143: a new file's save dialog opens in the folder saved to last; one that is gone is not used (the documents folder
/// of the OS is); a file that has a folder opens 名前を付けて保存 in it.
fn last_save_folder() {
    // Linux: XDG_DOCUMENTS_DIR of user-dirs.dirs, with $HOME filled in; the home folder itself means "none".
    let home = std::path::Path::new("/home/u");
    let dirs = "# comment\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_DOCUMENTS_DIR=\"$HOME/Docs\"\n";
    assert_eq!(save_location::xdg_user_dir(dirs, "XDG_DOCUMENTS_DIR", home), Some(std::path::PathBuf::from("/home/u/Docs")));
    assert_eq!(save_location::xdg_user_dir("XDG_DOCUMENTS_DIR=\"/data/docs\"", "XDG_DOCUMENTS_DIR", home), Some(std::path::PathBuf::from("/data/docs")));
    assert_eq!(save_location::xdg_user_dir("XDG_DOCUMENTS_DIR=\"$HOME/\"", "XDG_DOCUMENTS_DIR", home), None);
    assert_eq!(save_location::xdg_user_dir(dirs, "XDG_MUSIC_DIR", home), None);
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let base = crate::shell::scenario::repo_root().join("target/scenario/issue143-folders");
    let _ = std::fs::remove_dir_all(&base);
    let (first, second) = (base.join("first"), base.join("second"));
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    // The fallbacks, as a pure function: remembered, else documents, else home, else the current folder.
    let docs = base.join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    let gone = base.join("gone");
    assert_eq!(save_location::resolve_default_folder(first.to_str(), Some(docs.clone()), Some(base.clone())), first);
    assert_eq!(save_location::resolve_default_folder(gone.to_str(), Some(docs.clone()), Some(base.clone())), docs, "a remembered folder that is gone falls back to the documents folder");
    assert_eq!(save_location::resolve_default_folder(None, Some(gone.clone()), Some(base.clone())), base, "no documents folder: the home folder");
    assert_eq!(save_location::resolve_default_folder(None, None, None), std::path::PathBuf::from("."));
    // Nothing remembered (what a new install has): the OS's documents folder, or the home folder when it has none.
    crate::shell::settings::set(save_location::LAST_FOLDER_KEY, "");
    let expected = save_location::documents_dir().filter(|d| d.is_dir()).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap();
    assert_eq!(save_location::default_save_folder(), expected, "the first save opens in the documents folder");
    // A save through the dialog remembers its folder (the setting is kept in the settings file) ...
    save_location::remember_save_folder(&first.join("a.docx"));
    assert_eq!(crate::shell::settings::get(save_location::LAST_FOLDER_KEY).as_deref(), first.to_str());
    assert_eq!(save_location::default_save_folder(), first);
    save_location::remember_save_folder(&second.join("b.docx"));
    assert_eq!(save_location::default_save_folder(), second, "the next new file opens in the last folder saved to");
    // ... and a file with no folder yet opens there.
    let with_path = s.with(|e, _| e.save_dialog_folder());
    assert_eq!(with_path, s.with(|e, _| e.path.clone()).unwrap().parent().unwrap(), "an opened file's Save As starts in its own folder");
    s.update(|e, _, _| e.set_file_path(None));
    assert_eq!(s.with(|e, _| e.new_file_save_folder()), second);
    // The remembered folder is gone: the documents folder again.
    std::fs::remove_dir_all(&second).unwrap();
    assert_eq!(save_location::default_save_folder(), expected, "a folder that is gone falls back to the default");
    let _ = std::fs::remove_dir_all(&base);
    crate::shell::settings::set(save_location::LAST_FOLDER_KEY, "");
}

/// #140: a right click in the text opens Word's menu (cut, copy, paste, the paste options, font, paragraph, bullets,
/// link, comment), with the table's lines in a table; the caret goes to the click unless it is in the selection.
fn right_click_menu() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let (para, text) = s.with(|e, _| {
        e.flow().unwrap().body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some((p.id, p.text())).filter(|(_, t)| t.chars().count() > 4) } else { None }).unwrap()
    });
    let _ = text;
    s.update(|e, _, cx| e.set_caret(FlowPos::new(para, 1), false, cx));
    let at = text_screen_pos(&mut s, FlowPos::new(para, 2));
    s.right_click(at);
    assert!(s.with(|e, _| e.link_popup.is_some()), "a right click in the text opens the menu");
    let entries = s.with(|e, _| e.doc_body_popup_entries());
    for id in ["edit.cut", "edit.copy", "edit.paste", "docx.paste-special", "docx.context.font", "docx.context.paragraph", "docx.bullet.toggle", "docx.list.toggle", "insert.link", "insert.comment", "edit.select-all"] {
        assert!(find(&entries, id).is_some(), "the menu has {id}");
    }
    assert!(find(&entries, "docx.table.row-above").is_none(), "no table lines outside a table");
    // The caret went to the click (nothing was selected).
    assert_eq!(s.with(|e, _| e.text_selection()), Some((FlowPos::new(para, 2), FlowPos::new(para, 2))));
    assert!(!enabled(&entries, "edit.copy") || !s.with(|e, _| e.has_selection()), "no selection: nothing to copy");
    // A selection: a right click in it keeps it, and cut and copy can run.
    s.update(|e, _, cx| {
        e.set_caret(FlowPos::new(para, 0), false, cx);
        e.set_caret(FlowPos::new(para, 4), true, cx);
    });
    s.update(|e, _, cx| {
        e.link_popup = None;
        cx.notify();
    });
    let inside = text_screen_pos(&mut s, FlowPos::new(para, 2));
    s.right_click(inside);
    assert_eq!(s.with(|e, _| e.ordered_selection()), Some((FlowPos::new(para, 0), FlowPos::new(para, 4))), "a right click inside the selection keeps it");
    let entries = s.with(|e, _| e.doc_body_popup_entries());
    assert!(enabled(&entries, "edit.cut") && enabled(&entries, "edit.copy"), "a selection can be cut and copied");
    // In a table: the table's lines.
    let cell = s.with(|e, _| {
        e.flow().unwrap().body.iter().find_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).and_then(|t| t.rows[0].cells[0].blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None })).unwrap()
    });
    s.update(|e, _, cx| {
        e.link_popup = None;
        cx.notify();
    });
    let in_cell = text_screen_pos(&mut s, FlowPos::new(cell, 0));
    s.right_click(in_cell);
    assert!(s.with(|e, _| e.link_popup.is_some()));
    let entries = s.with(|e, _| e.doc_body_popup_entries());
    assert!(find(&entries, "docx.table.row-above").is_some() && find(&entries, "docx.table.delete-column").is_some(), "a table's lines in a table");
    // Running a line of the menu runs its command (select all).
    let run = match find(&entries, "edit.select-all") {
        Some(MenuEntry::Item(i)) => i.handler.clone().unwrap(),
        _ => panic!("select all"),
    };
    s.update(|e, w, cx| run(e, w, cx));
    assert_ne!(s.with(|e, _| e.ordered_selection().map(|(a, b)| a == b)), Some(true), "select all selected something");
}

/// #140: 行番号 > なし turns the numbering off in every section (it was only the caret's), and the options (the
/// interval, the distance from the text, the restart) are written to the file and read back.
fn line_numbers() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let sections = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.flow().unwrap();
            let state = ibis_doc_model::flow_command::section_state(doc, &ibis_doc_model::flow_command::section_paragraphs(doc));
            let mut all = vec![state.page.line_numbers];
            all.extend(state.breaks.iter().filter_map(|(_, _, setup)| setup.as_ref().map(|s| s.page.line_numbers)));
            all
        })
    };
    // Two sections, numbered both.
    s.key("cmd-down");
    s.menu("docx.section-next");
    s.menu("docx.line-numbers.continuous");
    let numbered = sections(&mut s);
    assert!(numbered.len() >= 2, "the document has two sections now: {numbered:?}");
    assert!(numbered.iter().all(|n| n.is_some()), "every section is numbered: {numbered:?}");
    // The options.
    s.menu("docx.line-numbers.every-5");
    s.menu("docx.line-numbers.distance-10");
    s.menu("docx.line-numbers.restart-page");
    let page = sections(&mut s);
    assert!(page.iter().all(|n| n.is_some_and(|n| n.count_by == 5 && n.distance == 10 * 36_000 && n.restart == LineNumberRestart::NewPage)), "{page:?}");
    s.save_as("issue140-line-numbers.docx");
    let back = s.reopen();
    let ln = back.flow.as_ref().unwrap().page.line_numbers.expect("line numbers read back");
    assert_eq!((ln.count_by, ln.restart), (5, LineNumberRestart::NewPage), "countBy and restart are saved and read back");
    assert!((ln.distance - 10 * 36_000).abs() <= 635, "the distance is saved in whole twips (1 twip = 635 EMU): {}", ln.distance);
    // Turned off from the other section: off in every one.
    s.key("cmd-home");
    s.menu("docx.line-numbers.none");
    let off = sections(&mut s);
    assert!(off.iter().all(|n| n.is_none()), "numbering is off in every section: {off:?}");
    assert_eq!(s.menu_state("docx.line-numbers.every-5"), Some((false, false)), "the options are off while numbering is");
    s.save_as("issue140-line-numbers-off.docx");
    assert!(s.reopen().flow.as_ref().unwrap().page.line_numbers.is_none(), "no lnNumType is left in the saved file");
}

/// #140 (the reporter's file): `w:lnNumType w:countBy="0"` (and a missing countBy) is no line numbering in Word, which
/// shows none and reports LineNumbering.Active = 0; ibisOffice used to turn the 0 into 1 and numbered every line.
fn count_by_zero() {
    for (file, numbered) in [
        ("test/editing/docx-linenumbers-countby0.docx", false),
        ("test/editing/docx-linenumbers-countby-omitted.docx", false),
        ("test/editing/docx-linenumbers-countby2.docx", true),
    ] {
        let mut s = Scenario::open(file);
        let page = s.with(|e, _| e.flow().unwrap().page.line_numbers);
        assert_eq!(page.is_some(), numbered, "{file}: {page:?}");
        if let Some(n) = page {
            assert_eq!(n.count_by, 2, "{file}");
        }
    }
}
