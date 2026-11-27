// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The host layer (shell/host.rs, shell/host_editor.rs) with a `MemoryHost`: a document the host gave is saved back to it, a failure
//! of the host leads where the design says (5.7), a read-only document is saved as another file, and the host is told what happens.
//! The browser's `JsHost` is the same trait; this runs on the desktop build with no browser.
use std::rc::Rc;

use crate::shell::host::{self, DocumentSource, HostCapabilities, HostEvent, HostFailure, HostOptions, HostReason, MemoryHost, SaveTrigger};
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    let mut all: Vec<(&'static str, fn())> = vec![
        ("shell::host_load_edit_save", load_edit_save),
        ("shell::host_save_failures", save_failures),
        ("shell::host_read_only_saves_as_another_file", read_only),
        ("shell::host_replacing_the_document_tells_the_host", replacing),
        ("shell::host_without_functions_changes_nothing", no_host),
        ("shell::host_dirty_is_told_in_the_browser_too", dirty_on_wasm),
        ("shell::host_menu_close_goes_the_way_the_cross_does", menu_close),
        ("shell::host_menu_close_with_edits_asks_first", menu_close_with_edits),
        ("shell::host_escape_closes_when_clean_and_not_with_edits", escape_closes_when_clean),
        ("shell::bundled_icons_of_the_review_menu", bundled_review_icons),
        ("shell::host_without_a_start_window_has_no_start_window_menu_item", no_start_window_menu_item),
        ("shell::host_recent_files_are_added_on_load_and_listed", recent_files_added_and_listed),
        ("shell::host_csv_and_tsv_are_saved_as_text", text_saves),
        ("shell::host_save_as_writes_the_format_of_the_name", text_save_as),
        ("shell::host_save_as_of_a_package_stays_xlsx", xlsx_save_as),
        ("shell::host_text_file_saves_as_xlsx_by_name", text_to_xlsx),
        ("shell::host_opening_text_tells_nothing_on_save", text_open_no_note),
        ("shell::host_old_binary_file_is_saved_as_a_new_name", legacy_saves_as),
    ];
    all.extend(file_window());
    all.extend(rename_and_bars());
    all.extend(options_and_links());
    all
}

/// A host with one writable folder holding a document, and a scenario with that document open (read from the host).
fn start() -> (Scenario, MemoryHost, String) {
    let host = MemoryHost::new();
    let folder = host.add_folder(None, "Sales", true);
    let bytes = ibis_formats::blank_file(ibis_formats::Kind::Document).expect("a blank document");
    let file = host.add_file(Some(&folder), "report.docx", &bytes);
    let folder_id = folder.clone();
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    host.set_current(Some(&file));
    let mut s = Scenario::start_view();
    let version = host.version_of(&file).expect("version");
    s.update(|e, w, cx| e.host_load("report.docx".into(), bytes, version, Some(folder_id), false, None, true, w, cx).expect("the host's document opens"));
    s.quit_with_last_window();
    (s, host, file)
}

/// B-4: the browser version has no table of open documents (`sync_open_documents` is desktop only), but it has hosts: `host_sync_dirty` is
/// called from `render_window` itself (not from `sync_open_documents`).
fn dirty_on_wasm() {
    let source = include_str!("../../../../ibis-office-shell/src/editor.rs");
    let at = source.find("self.host_sync_dirty();").expect("the window drawing tells the host of the dirty mark");
    let function = source[..at].rfind("fn ").map(|i| &source[i..at]).unwrap_or("");
    assert!(function.starts_with("fn render_window"), "it is called in render_window, not in sync_open_documents: {}", &function[..function.len().min(40)]);
}

/// K0037: 閉じる in the File menu in a host asks about unsaved changes and tells the host to close (as the × does), and does not close the file (a white page).
fn menu_close() {
    let (mut s, host, _file) = start();
    // Without edits: the host is told at once, the document stays.
    s.update(|e, w, cx| e.close_file(&crate::shell::actions::CloseFile, w, cx));
    s.settle();
    assert!(host.events().contains(&HostEvent::CloseRequested), "the host is told to close: {:?}", host.events());
    assert!(s.with(|e, _| e.has_document), "the document is still shown");
    host::uninstall();
}

/// K0037: with `ui.startWindow` off the File menu has no スタートウィンドウ line (and no separator left first); with it on (the default) the desktop menu has it.
fn no_start_window_menu_item() {
    let (mut s, _host, _file) = start();
    assert!(s.menu_state("file.start-window").is_some(), "the default: the line is there");
    drop(s);
    host::uninstall();
    let host = MemoryHost::new();
    let options = HostOptions { start_window: false, ..HostOptions::default() };
    host::install(Rc::new(host.clone()), HostCapabilities::all(), options);
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    assert!(s.menu_state("file.start-window").is_none(), "no start window: no line");
    assert!(s.menu_state("file.save").is_some(), "the rest of the menu is there");
    drop(s);
    host::uninstall();
}

/// K0037: a file the host loads is put first in the host's recent files (name and the address `getFileLocation` gave), and the list the host gives back is what the menu shows.
fn recent_files_added_and_listed() {
    let (mut s, host, _file) = start();
    s.settle();
    let listed = host::recent_files();
    assert_eq!(listed.len(), 1, "the loaded file is in the list: {listed:?}");
    assert_eq!(listed[0].name, "report.docx");
    assert!(listed[0].url.starts_with("memory://file/"), "its address is the host's location: {}", listed[0].url);
    drop(s);
    host::uninstall();
    assert!(host::recent_files().is_empty(), "no host, no list");
}

/// K0037 (ibisWorks's report): the Review menu's accessibility, read-aloud and find-in-page icons are embedded (the Web version asked the server for them: 404).
fn bundled_review_icons() {
    for name in ["accessibility", "record_voice_over", "find_in_page"] {
        assert!(crate::shell::icons::icon_svg_bytes(name).is_some(), "the icon {name} is in icons/material");
    }
}

/// K0037: in a host with a close button, Esc that nothing else took closes the editor when there is nothing unsaved (the host is told, as the × does), and does nothing with
/// unsaved changes.
fn escape_closes_when_clean() {
    let (mut s, host, _file) = start();
    crate::shell::embed::set_embed_options(true, false, false, true, true);
    s.key("escape");
    s.settle();
    assert!(host.events().contains(&HostEvent::CloseRequested), "clean: the host is told to close: {:?}", host.events());
    drop(s);
    host::uninstall();
    let (mut s, host, _file) = start();
    crate::shell::embed::set_embed_options(true, false, false, true, true);
    type_something(&mut s);
    s.key("escape");
    s.settle();
    assert!(!host.events().contains(&HostEvent::CloseRequested), "with edits Esc does nothing");
    assert!(!s.with(|e, _| e.save_confirm_dialog_open), "and asks nothing");
    crate::shell::embed::set_embed_options(false, false, false, true, true);
    host::uninstall();
}

/// With edits: the question first, the host not told before it is answered.
fn menu_close_with_edits() {
    let (mut s, host, _file) = start();
    type_something(&mut s);
    s.update(|e, w, cx| e.close_file(&crate::shell::actions::CloseFile, w, cx));
    s.settle();
    assert!(!host.events().contains(&HostEvent::CloseRequested), "not told before the question is answered");
    assert!(s.with(|e, _| e.save_confirm_dialog_open), "the question is asked");
    host::uninstall();
}

fn type_something(s: &mut Scenario) {
    let (id, _) = crate::docx::scenarios::DocSteps::paragraph(s, 0);
    crate::docx::scenarios::DocSteps::click_text(s, ibis_doc_model::FlowPos::new(id, 0));
    s.type_text("edited");
    assert!(s.with(|e, _| e.is_dirty()));
}

fn load_edit_save() {
    let (mut s, host, file) = start();
    assert!(s.with(|e, _| e.is_host_bound() && !e.is_dirty() && !e.is_read_only()));
    assert!(matches!(host.events().last(), Some(HostEvent::Loaded { name, kind: "docx" }) if name == "report.docx"));
    type_something(&mut s);
    s.settle();
    assert!(host.events().contains(&HostEvent::Dirty(true)), "the host is told of the unsaved changes");
    let before = host.bytes_of(&file).expect("bytes");
    let version = host.version_of(&file).expect("version");
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    s.settle();
    assert_ne!(host.bytes_of(&file).expect("bytes"), before, "the edited document went to saveFile");
    assert_ne!(host.version_of(&file).expect("version"), version);
    assert!(s.with(|e, _| !e.is_dirty() && e.host.binding.as_ref().is_some_and(|b| Some(b.version.clone()) == host.version_of(&file))), "clean, with the version the host gave back");
    assert!(host.events().contains(&HostEvent::Saved(SaveTrigger::User)));
    assert!(host.events().contains(&HostEvent::Dirty(false)));
    // The snapshot is what a save writes.
    let snapshot = s.update(|e, w, cx| e.host_bytes(w, cx)).expect("snapshot");
    assert_eq!(Some(snapshot), host.bytes_of(&file));
    host::uninstall();
}

fn save_failures() {
    // Another person saved first: a message box with the host's words, the changes stay, the file window (save as) opens.
    {
    let (mut s, host, file) = start();
    type_something(&mut s);
    host.touch(&file);
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    s.settle();
    assert!(host.calls().contains(&"saveFile".to_string()));
    assert!(s.with(|e, _| e.alert.as_ref().is_some_and(|(_, m)| m.as_ref() == "somebody saved it first") && e.is_dirty()), "told in a message box with the host's words, the changes kept");
    assert!(s.with(|e, _| e.host.dialog.as_ref().is_some_and(|d| d.mode() == crate::shell::host_file_dialog::Mode::SaveAs)), "save as is offered next");
    host::uninstall();
    }

    // The host's own words are shown as they are (and the file window opens: save as is offered); a lost connection is also in the status bar, keeps the changes and opens no window.
    let (mut s, host, _file) = start();
    type_something(&mut s);
    host.fail_next("saveFile", HostFailure::new(HostReason::Locked, "Taro is editing it on PC-1."));
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    s.settle();
    assert!(s.with(|e, _| e.alert.as_ref().is_some_and(|(_, m)| m.as_ref() == "Taro is editing it on PC-1.")));
    s.update(|e, _, cx| e.dismiss_alert(cx));
    s.update(|e, w, cx| e.close_dialog(w, cx));
    host.fail_next("saveFile", HostFailure::new(HostReason::Network, ""));
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    s.settle();
    assert!(s.with(|e, _| e.alert.is_some() && e.is_dirty() && e.host.dialog.is_none()), "network: no save as, the changes stay");
    assert!(s.with(|e, _| e.status.as_ref().is_some_and(|m| m.as_ref() == crate::shell::i18n::text("shell.host.error-network-status").as_ref())));
    host::uninstall();
}

fn read_only() {
    let (mut s, host, file) = start();
    s.update(|e, w, cx| e.host_set_read_only(true, Some("locked by Taro".into()), w, cx));
    assert!(s.with(|e, _| e.is_read_only()));
    let before = host.bytes_of(&file);
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    s.settle();
    assert_eq!(host.bytes_of(&file), before, "a read-only document is not written back");
    assert!(!host.calls().contains(&"saveFile".to_string()));
    assert!(s.with(|e, _| e.host.dialog.as_ref().is_some_and(|d| d.mode() == crate::shell::host_file_dialog::Mode::SaveAs)), "ⓘ it is saved as another file");
    s.update(|e, w, cx| e.host_set_read_only(false, None, w, cx));
    assert!(s.with(|e, _| !e.is_read_only()));
    host::uninstall();
}

fn replacing() {
    let (mut s, host, _file) = start();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Workbook, w, cx));
    s.settle();
    assert!(s.with(|e, _| !e.is_host_bound()), "a new file is not the host's");
    assert!(host.events().contains(&HostEvent::DocumentChanged(DocumentSource::New)));
    host::uninstall();
}

fn no_host() {
    host::uninstall();
    assert!(!host::is_connected());
    assert_eq!(host::capabilities(), HostCapabilities::default());
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    s.settle();
    assert!(s.with(|e, _| !e.is_host_bound() && e.host.dialog.is_none()), "the editor's own way, as before");
}

// ---- the file window ----

pub(crate) fn file_window() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::host_file_window_open_lists_and_pages", window_open),
        ("shell::host_file_window_starts_in_the_files_folder_and_opens_the_tree_to_it", window_start_folder),
        ("shell::host_file_window_drops_a_stale_folder", window_stale),
        ("shell::host_file_window_save_checks_name_permission_and_overwrite", window_save),
        ("shell::host_file_window_failures", window_failures),
        ("shell::host_file_window_tree_arrow_opens_and_the_row_chooses", window_tree_targets),
        ("shell::host_file_window_keeps_its_size_and_splitter_and_fits_the_window", window_size_and_split),
        ("shell::host_file_window_roots_stand_side_by_side_and_have_their_icons", window_roots),
        ("shell::host_file_window_show_the_rest_asks_the_slow_continuation_only_when_pressed", window_deferred),
        ("shell::host_file_window_show_the_rest_that_brings_nothing_says_so", window_deferred_nothing),
        ("shell::host_file_window_same_name_on_a_page_not_yet_read_is_found_with_and_without_the_check", window_same_name_unread),
    ]
}

/// A host of `Sales` (writable) > `2025` (writable) and `Archive` (read-only), with documents and some other files in them.
fn tree_host() -> (MemoryHost, String, String, String) {
    crate::shell::host_file_dialog::forget_last_folder();
    let host = MemoryHost::new();
    let sales = host.add_folder(None, "Sales", true);
    let year = host.add_folder(Some(&sales), "2025", true);
    let archive = host.add_folder(None, "Archive", false);
    let doc = ibis_formats::blank_file(ibis_formats::Kind::Document).expect("a blank document");
    host.add_file(Some(&sales), "a.docx", &doc);
    host.add_file(Some(&sales), "notes.txt", b"text");
    host.add_file(Some(&year), "b.docx", &doc);
    host.add_file(Some(&archive), "old.docx", &doc);
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    (host, sales, year, archive)
}

fn window_open() {
    let (host, sales, _year, _archive) = tree_host();
    // 2250 documents in one folder: the window takes 1000 at a time.
    let big = host.add_folder(None, "Big", true);
    let doc = ibis_formats::blank_file(ibis_formats::Kind::Document).expect("a blank document");
    for i in 0..2250 {
        host.add_file(Some(&big), &format!("f{i:04}.docx"), &doc);
    }
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    // Opening asks nothing yet of the folders below the roots (only the roots; the first root is shown): the roots stand side by side, none opened (K0041).
    let (names, tree) = s.with(|e, _| e.host.dialog.as_ref().map(|d| (d.list_names(), d.tree_names())).expect("the window is open"));
    assert_eq!(names, ["2025", "a.docx"], "the first root's contents (the roots keep the order the host gave: Sales, Archive, Big)");
    assert_eq!(tree, [(0, "Sales".to_string()), (1, "2025".to_string()), (0, "Archive".to_string()), (0, "Big".to_string())], "the roots, side by side; the first one shown is opened");
    assert!(s.with(|e, _| e.host.dialog.as_ref().is_some_and(|d| d.current_folder().is_some())));
    assert!(s.with(|e, _| e.host.dialog.as_ref().is_some_and(|d| d.mode() == crate::shell::host_file_dialog::Mode::Open && !d.is_loading())));
    // A folder is chosen: its files, only the ones the editor opens (notes.txt is left out).
    s.update(|e, w, cx| e.host_fd_select_folder(Some(sales.clone()), w, cx));
    s.settle();
    let names = s.with(|e, _| e.host.dialog.as_ref().map(|d| d.list_names()).expect("open"));
    assert_eq!(names, ["2025", "a.docx"]);
    // The big folder: the first page is 1000 (folders first, then files); the next page comes on request, and the last has the rest.
    s.update(|e, w, cx| e.host_fd_select_folder(Some(big.clone()), w, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| d.list_names().len())), Some(1000));
    s.update(|e, w, cx| e.host_fd_more_list(w, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| d.list_names().len())), Some(2000));
    s.update(|e, w, cx| e.host_fd_more_list(w, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| d.list_names().len())), Some(2250));
    // Choosing a file and 開く: the host is asked to open it (it loads the document itself), and the window closes.
    s.update(|e, w, cx| e.host_fd_select_folder(Some(sales), w, cx));
    s.settle();
    s.update(|e, w, cx| e.host_fd_pick_file("a.docx", w, cx));
    s.update(|e, w, cx| e.host_fd_confirm(w, cx));
    s.settle();
    let wanted = host.find_file(Some(&host.find_folder("Sales")), "a.docx").expect("a.docx");
    assert_eq!(host.opened(), vec![wanted]);
    assert!(s.with(|e, _| e.host.dialog.is_none()));
    host::uninstall();
}

fn window_start_folder() {
    let (host, sales, year, _archive) = tree_host();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    // The document of a file in 2025: the window starts there, and the tree is opened down to it (only the folders on the way are asked).
    s.update(|e, _, _| e.host.binding = Some(crate::shell::host_editor::HostBinding { folder_id: Some(year.clone()), version: "v1".into() }));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    assert!(s.with(|e, _| e.host.dialog.as_ref().is_some_and(|d| d.current_folder() == Some(year.as_str()))));
    let tree = s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_names()).expect("open"));
    assert_eq!(tree, [(0, "Sales".to_string()), (1, "2025".to_string()), (0, "Archive".to_string())], "the roots (in the host's order), Sales opened to 2025");
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| d.list_names())), Some(vec!["b.docx".to_string()]));
    let _ = (host, sales);
    host::uninstall();
}

/// K0045: the file window's size and the splitter between the tree and the list are kept for the next time (saving or opening alike); what
/// was kept larger than the window is shrunk to the window (the kept value stays), and an open window follows the window getting smaller.
fn window_size_and_split() {
    use crate::shell::host_file_dialog::{fit_split, SPLIT_KEY, SPLIT_MIN};
    let keys = ["dialog.host-file.width", "dialog.host-file.height", SPLIT_KEY];
    for k in keys { crate::shell::settings::remove(k); }
    let (host, _sales, _year, _archive) = tree_host();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    let first = s.with(|e, _| e.dialog.bounds.expect("the window's place"));
    let split0 = s.with(|e, _| e.host.dialog.as_ref().expect("open").split());
    assert_eq!(split0, 240., "no choice yet: the default");
    // The bottom right corner is dragged in: the window is smaller.
    let grip = (f32::from(first.origin.x + first.size.width) - 3., f32::from(first.origin.y + first.size.height) - 3.);
    s.drag(grip, (grip.0 - 100., grip.1 - 60.));
    let small = s.with(|e, _| e.dialog.bounds.expect("bounds"));
    assert!((f32::from(first.size.width - small.size.width) - 100.).abs() < 1. && (f32::from(first.size.height - small.size.height) - 60.).abs() < 1., "resized: {first:?} -> {small:?}");
    // The splitter is dragged 60 px to the right (its middle is 4 px into its 9 px; the dialog's padding and border are 13 px).
    let bar = (f32::from(small.origin.x) + 1. + 12. + split0 + 4., f32::from(small.origin.y) + f32::from(small.size.height) / 2.);
    s.drag(bar, (bar.0 + 60., bar.1));
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().expect("open").split()), 300.);
    // Closed and opened again (as the opening window this time): the same size and splitter.
    s.update(|e, w, cx| e.close_dialog(w, cx));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    let again = s.with(|e, _| e.dialog.bounds.expect("bounds"));
    assert_eq!((again.size.width, again.size.height), (small.size.width, small.size.height), "the size is kept");
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().expect("open").split()), 300., "the splitter is kept");
    s.update(|e, w, cx| e.close_dialog(w, cx));
    // A kept size larger than the window is shrunk to the window's view; the kept value itself is left as it was.
    crate::shell::settings::set("dialog.host-file.width", "5000");
    crate::shell::settings::set("dialog.host-file.height", "5000");
    crate::shell::settings::set(SPLIT_KEY, "4000");
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    let viewport = s.update(|_, w, _| w.viewport_size());
    let big = s.with(|e, _| e.dialog.bounds.expect("bounds"));
    assert_eq!((big.size.width, big.size.height), (viewport.width, viewport.height), "no larger than the window's view");
    assert!(big.origin.x >= gpui_kit::px(0.) && big.right() <= viewport.width && big.bottom() <= viewport.height);
    let split = s.with(|e, _| e.host.dialog.as_ref().expect("open").split());
    assert_eq!(split, 4000., "the kept value is not rewritten by a fit");
    assert!(fit_split(split, f32::from(big.size.width)) <= f32::from(big.size.width) - 240. - 26. - 9., "the list keeps its room");
    assert!(fit_split(0., 900.) == SPLIT_MIN);
    // The window gets narrower and lower with the dialog open: the dialog follows it.
    s.size(640., 420.);
    let narrow = s.update(|_, w, _| w.viewport_size());
    s.settle();
    let followed = s.with(|e, _| e.dialog.bounds.expect("bounds"));
    assert!(followed.size.width <= narrow.width && followed.size.height <= narrow.height && followed.right() <= narrow.width && followed.bottom() <= narrow.height, "{followed:?} in {narrow:?}");
    assert_eq!(crate::shell::settings::get("dialog.host-file.width").as_deref(), Some("5000"), "the kept size is not overwritten");
    s.update(|e, w, cx| e.close_dialog(w, cx));
    for k in keys { crate::shell::settings::remove(k); }
    let _ = host;
    host::uninstall();
}

/// K0046: the tree's arrow only opens or closes the node (the current folder stays); the rest of the row only makes the folder the current one
/// (the node is not opened).
fn window_tree_targets() {
    let (host, sales, _year, archive) = tree_host();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    let tree = |s: &mut Scenario| s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_names()).expect("open").len());
    let current = |s: &mut Scenario| s.with(|e, _| e.host.dialog.as_ref().and_then(|d| d.current_folder().map(str::to_string)));
    // (The window starts in the folder last used in this run, so the tree's size is compared with itself, not with a fixed number.)
    let base = tree(&mut s);
    // The row of Archive: it is the current folder; the tree stays as it was.
    s.update(|e, w, cx| e.host_fd_select_current(Some(archive.clone()), w, cx));
    s.settle();
    assert_eq!(current(&mut s), Some(archive.clone()));
    assert_eq!(tree(&mut s), base, "choosing the row does not open or close anything");
    // The row of Sales (a node with a folder below, not opened by it if it was closed): the current folder changes.
    let sales_open = s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_names().iter().any(|(_, n)| n == "2025")).expect("open"));
    s.update(|e, w, cx| e.host_fd_select_current(Some(sales.clone()), w, cx));
    s.settle();
    assert_eq!(current(&mut s), Some(sales.clone()));
    let after_row = s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_names().iter().any(|(_, n)| n == "2025")).expect("open"));
    assert_eq!(after_row, sales_open, "the row does not open Sales");
    // The arrow of Sales: it opens or closes the node; the current folder is the same.
    s.update(|e, w, cx| e.host_fd_toggle_node(Some(sales.clone()), w, cx));
    s.settle();
    let toggled = s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_names().iter().any(|(_, n)| n == "2025")).expect("open"));
    assert_eq!(toggled, !sales_open, "the arrow opens or closes the node");
    assert_eq!(current(&mut s), Some(sales.clone()), "the arrow does not change the current folder");
    let _ = host;
    host::uninstall();
}

fn window_stale() {
    let (host, sales, _year, archive) = tree_host();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    // Two folders chosen one after the other before the host answers: only the last one's page is shown.
    s.update(|e, w, cx| {
        e.host_fd_select_folder(Some(sales.clone()), w, cx);
        e.host_fd_select_folder(Some(archive.clone()), w, cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.host.dialog.as_ref().is_some_and(|d| d.current_folder() == Some(archive.as_str()))));
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| d.list_names())), Some(vec!["old.docx".to_string()]));
    // The window closed while a request was out: its answer is dropped (nothing breaks, nothing opens).
    s.update(|e, w, cx| {
        e.host_fd_select_folder(Some(sales.clone()), w, cx);
        e.close_dialog(w, cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.host.dialog.is_none()));
    let _ = host;
    host::uninstall();
}

fn window_save() {
    let (host, sales, _year, archive) = tree_host();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.save_as(&crate::shell::actions::SaveAs, w, cx));
    s.settle();
    assert!(s.with(|e, _| e.host.dialog.as_ref().is_some_and(|d| d.mode() == crate::shell::host_file_dialog::Mode::SaveAs)));
    // A folder that cannot be written: nothing is sent.
    s.update(|e, w, cx| e.host_fd_select_folder(Some(archive), w, cx));
    s.settle();
    s.update(|e, w, cx| e.host_fd_set_name("mine", w, cx));
    s.update(|e, w, cx| e.host_fd_confirm(w, cx));
    s.settle();
    assert!(!host.calls().contains(&"saveFileAs".to_string()), "no permission: nothing is sent");
    // A name with a forbidden character: nothing is sent.
    s.update(|e, w, cx| e.host_fd_select_folder(Some(sales.clone()), w, cx));
    s.settle();
    s.update(|e, w, cx| e.host_fd_set_name("a/b", w, cx));
    s.update(|e, w, cx| e.host_fd_confirm(w, cx));
    s.settle();
    assert!(!host.calls().contains(&"saveFileAs".to_string()));
    // A name of a file there: asked first; the answer OK writes over it (the id goes as `overwriteFileId`).
    s.update(|e, w, cx| e.host_fd_set_name("a", w, cx));
    s.update(|e, w, cx| e.host_fd_confirm(w, cx));
    s.settle();
    assert!(s.with(|e, _| e.ask.is_some()), "a.docx exists: the question");
    assert!(!host.calls().contains(&"saveFileAs".to_string()));
    s.update(|e, _, cx| {
        e.ask = None;
        cx.notify();
    });
    // A new name: saved, the document is tied to the new file, the window closes.
    s.update(|e, w, cx| e.host_fd_set_name("fresh.docx", w, cx));
    s.update(|e, w, cx| e.host_fd_confirm(w, cx));
    s.settle();
    let fresh = host.find_file(Some(&sales), "fresh.docx").expect("the new file");
    assert!(host.bytes_of(&fresh).is_some_and(|b| !b.is_empty()));
    assert!(s.with(|e, _| e.host.dialog.is_none() && e.is_host_bound() && e.file_name.as_ref().is_some_and(|n| n.as_ref() == "fresh.docx")));
    assert_eq!(s.with(|e, _| e.host.binding.as_ref().and_then(|b| b.folder_id.clone())), Some(sales));
    host::uninstall();
}

fn window_failures() {
    let (host, _sales, _year, _archive) = tree_host();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    host.fail_next("listFolder", HostFailure::new(HostReason::Network, "no server"));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    // The page of the top failed: told in the window, which stays; the host's other answers go on.
    assert!(s.with(|e, _| e.host.dialog.is_some()));
    host::uninstall();
}


/// K0041: a host's roots (storage, joined places, personal) stand side by side at the top of the tree in the host's order; the first one is
/// shown when nothing was chosen; each has the icon of its kind; a root that cannot be written is chosen but not saved in.
fn window_roots() {
    use crate::shell::host::RootKind;
    crate::shell::host_file_dialog::forget_last_folder();
    let host = MemoryHost::new();
    let storage = host.add_root("Storage", RootKind::Shared, true);
    let joined = host.add_root("Joined", RootKind::Joined, false);
    let mine = host.add_root("Mine", RootKind::Personal, true);
    let doc = ibis_formats::blank_file(ibis_formats::Kind::Document).expect("a blank document");
    host.add_folder(Some(&storage), "Sales", true);
    host.add_file(Some(&storage), "top.docx", &doc);
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.save_as(&crate::shell::actions::SaveAs, w, cx));
    s.settle();
    let tree = s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_names()).expect("open"));
    assert_eq!(tree, [(0, "Storage".to_string()), (1, "Sales".to_string()), (0, "Joined".to_string()), (0, "Mine".to_string())], "side by side, the host's order, no folder above them; the first one shown is opened");
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().and_then(|d| d.current_folder().map(str::to_string))), Some(storage.clone()), "the first root is shown");
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| d.list_names())), Some(vec!["Sales".to_string(), "top.docx".to_string()]));
    assert_eq!(RootKind::Joined.icon(), "group");
    assert_eq!(RootKind::parse("a kind this editor does not know"), RootKind::Other, "an unknown kind is other");
    // A root that cannot be written: shown, chosen, and not saved in (nothing is sent).
    s.update(|e, w, cx| e.host_fd_select_folder(Some(joined.clone()), w, cx));
    s.settle();
    s.update(|e, w, cx| e.host_fd_set_name("mine", w, cx));
    s.update(|e, w, cx| e.host_fd_confirm(w, cx));
    s.settle();
    assert!(!host.calls().contains(&"saveFileAs".to_string()) && !host.calls().contains(&"checkSaveTarget".to_string()), "no permission: nothing is sent");
    // Backspace at a root goes nowhere (the roots have no parent).
    s.key("backspace");
    s.settle();
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().and_then(|d| d.current_folder().map(str::to_string))), Some(joined));
    let _ = mine;
    host::uninstall();
}

/// A host of a storage and a joined root whose list is quick for the first folder and slow for the others (`deferredCursor`).
fn deferred_host() -> (MemoryHost, String) {
    use crate::shell::host::RootKind;
    crate::shell::host_file_dialog::forget_last_folder();
    let host = MemoryHost::new();
    host.add_root("Storage", RootKind::Shared, true);
    let joined = host.add_root("Joined", RootKind::Joined, false);
    host.add_folder(Some(&joined), "own", true);
    host.add_late_folder(&joined, "other1", true);
    host.add_late_folder(&joined, "other2", true);
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    (host, joined)
}

/// K0041: the first list of the joined root is the quick part (the user's own folder); 続きの表示 stands at the end of the tree's node and of the right list and
/// asks nothing by itself; a press asks with the host's `deferredCursor` and adds the rest (a folder in both is one).
fn window_deferred() {
    let (host, joined) = deferred_host();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    s.update(|e, w, cx| e.host_fd_select_folder(Some(joined.clone()), w, cx));
    s.settle();
    let labels = |s: &mut Scenario| s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_labels()).expect("open"));
    let lists = host.calls().iter().filter(|c| *c == "listFolder").count();
    assert_eq!(labels(&mut s), [(0, "Storage".to_string()), (0, "Joined".to_string()), (1, "own".to_string()), (1, "[deferred]".to_string())], "the quick part, then the more row");
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| (d.list_names(), d.list_tail()))), Some((vec!["own".to_string()], Some(true))), "the list too");
    s.settle();
    assert_eq!(host.calls().iter().filter(|c| *c == "listFolder").count(), lists, "nothing is asked until it is pressed");
    // Pressed in the tree: all of them (own once), the row gone.
    s.update(|e, w, cx| e.host_fd_deferred_children(Some(joined.clone()), w, cx));
    s.settle();
    assert_eq!(labels(&mut s), [(0, "Storage".to_string()), (0, "Joined".to_string()), (1, "own".to_string()), (1, "other1".to_string()), (1, "other2".to_string())]);
    // The right list: its own press.
    s.update(|e, w, cx| e.host_fd_deferred_list(w, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| (d.list_names(), d.list_tail()))), Some((vec!["own".to_string(), "other1".to_string(), "other2".to_string()], None)), "the rest comes after what was there, once each");
    host::uninstall();
}

/// K0041: when 続きの表示 brought nothing that was not there, the status line says there were no others.
fn window_deferred_nothing() {
    use crate::shell::host::RootKind;
    crate::shell::host_file_dialog::forget_last_folder();
    let host = MemoryHost::new();
    host.add_root("Storage", RootKind::Shared, true);
    let joined = host.add_root("Joined", RootKind::Joined, false);
    host.add_folder(Some(&joined), "own", true);
    host.defer_nothing(&joined);
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    s.update(|e, w, cx| e.host_fd_select_folder(Some(joined.clone()), w, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_labels().last().map(|l| l.1.clone()))), Some(Some("[deferred]".to_string())));
    s.update(|e, w, cx| e.host_fd_deferred_children(Some(joined.clone()), w, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.status.as_ref().map(|m| m.to_string())), Some(crate::shell::i18n::text("shell.host-file.deferred-empty").to_string()));
    assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_labels().last().map(|l| l.1.clone()))), Some(Some("own".to_string())), "the row is gone");
    host::uninstall();
}

/// K0041: the same name as a file on a page the window has not read: with the host's `checkSaveTarget` the host says so; without it the rest of the pages
/// are read first. Either way the user is asked, and a name nobody has is saved.
fn window_same_name_unread() {
    for with_check in [true, false] {
        crate::shell::host_file_dialog::forget_last_folder();
        let host = MemoryHost::new();
        let big = host.add_folder(None, "Big", true);
        let doc = ibis_formats::blank_file(ibis_formats::Kind::Document).expect("a blank document");
        for i in 0..2250 {
            host.add_file(Some(&big), &format!("f{i:04}.docx"), &doc);
        }
        let caps = HostCapabilities { check_save_target: with_check, ..HostCapabilities::all() };
        host::install(Rc::new(host.clone()), caps, HostOptions::default());
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
        s.update(|e, w, cx| e.save_as(&crate::shell::actions::SaveAs, w, cx));
        s.settle();
        assert_eq!(s.with(|e, _| e.host.dialog.as_ref().map(|d| d.list_names().len())), Some(1000), "one page is read");
        let lists = host.calls().iter().filter(|c| *c == "listFolder").count();
        s.update(|e, w, cx| e.host_fd_set_name("f2249", w, cx));
        s.update(|e, w, cx| e.host_fd_confirm(w, cx));
        s.settle();
        assert!(s.with(|e, _| e.ask.is_some()), "f2249.docx is on the third page: asked (with_check {with_check})");
        assert!(!host.calls().contains(&"saveFileAs".to_string()));
        assert_eq!(host.calls().contains(&"checkSaveTarget".to_string()), with_check);
        assert_eq!(host.calls().iter().filter(|c| *c == "listFolder").count() > lists, !with_check, "the pages are read only where the host cannot check");
        s.update(|e, _, cx| {
            e.ask = None;
            cx.notify();
        });
        s.update(|e, w, cx| e.host_fd_set_name("brand-new", w, cx));
        s.update(|e, w, cx| e.host_fd_confirm(w, cx));
        s.settle();
        assert!(host.find_file(Some(&big), "brand-new.docx").is_some(), "a name nobody has is saved (with_check {with_check})");
        host::uninstall();
    }
}

// ---- the file name's menu and the information bar ----

pub(crate) fn rename_and_bars() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::host_rename_and_get_path", rename_path),
        ("shell::host_info_bars", info_bars),
    ]
}

fn rename_path() {
    let (mut s, host, file) = start();
    let folder = host.find_folder("Sales");
    host.add_file(Some(&folder), "taken.docx", b"x");
    // パスの取得: the host's place goes to the clipboard.
    s.update(|e, _, cx| e.host_get_file_location(cx));
    s.settle();
    let copied = s.with_app(|app| app.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(copied, Some(format!("memory://file/{file}")));
    // ファイル名を変更: a name that is taken is refused with the host's words, and the dialog stays for another name.
    s.update(|e, w, cx| e.open_rename_file_dialog(w, cx));
    s.update(|e, w, cx| {
        let input = e.rename_file.as_ref().expect("the dialog").input.clone();
        input.update(cx, |input, cx| input.set_value("taken", w, cx));
        e.apply_rename_file(w, cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.rename_file.is_some() && e.alert.is_some()), "refused: the dialog stays, the host's words are shown");
    assert_eq!(host.name_of(&file), Some("report.docx".to_string()));
    s.update(|e, _, cx| e.dismiss_alert(cx));
    // A free name: renamed (the extension stays), the window follows, with the version the host gave.
    s.update(|e, w, cx| {
        let input = e.rename_file.as_ref().expect("the dialog").input.clone();
        input.update(cx, |input, cx| input.set_value("renamed", w, cx));
        e.apply_rename_file(w, cx);
    });
    s.settle();
    assert_eq!(host.name_of(&file), Some("renamed.docx".to_string()));
    assert!(s.with(|e, _| e.rename_file.is_none() && e.file_name.as_ref().is_some_and(|n| n.as_ref() == "renamed.docx")));
    assert_eq!(s.with(|e, _| e.host.binding.as_ref().map(|b| b.version.clone())), host.version_of(&file));
    host::uninstall();
}

fn info_bars() {
    use crate::shell::infobar::InfoBarKind;
    let (mut s, _host, _file) = start();
    assert_eq!(s.with(|e, _| e.infobar_kind()), None);
    s.update(|e, w, cx| e.host_set_read_only(true, Some("Taro has it open.".into()), w, cx));
    assert_eq!(s.with(|e, _| e.infobar_kind()), Some(InfoBarKind::HostReadOnly));
    // A notice of the host comes before the read-only bar, and goes when the host clears it.
    s.update(|e, _, cx| e.host_notify(Some(crate::shell::host_editor::HostNotice { level: crate::shell::host_editor::HostNoticeLevel::Warn, message: "Cannot reach the server.".into(), id: None }), cx));
    assert_eq!(s.with(|e, _| e.infobar_kind()), Some(InfoBarKind::HostNotice));
    s.update(|e, _, cx| e.host_notify(None, cx));
    assert_eq!(s.with(|e, _| e.infobar_kind()), Some(InfoBarKind::HostReadOnly));
    s.update(|e, w, cx| e.host_set_read_only(false, None, w, cx));
    assert_eq!(s.with(|e, _| e.infobar_kind()), None);
    host::uninstall();
}

// ---- the options of the host: links, the source, the user ----

pub(crate) fn options_and_links() -> Vec<(&'static str, fn())> {
    vec![("shell::host_source_url_and_open_url", source_and_links), ("shell::host_logo_opens_the_hosts_start_url", logo_opens_start_url), ("shell::host_start_view_opens_through_the_host", start_view_open), ("shell::host_user_name_is_the_author", user_is_author), ("shell::host_features_theme_print_export_fonts_watermark", features), ("shell::host_file_window_keys", window_keys), ("shell::host_print_and_pdf_export_flow", print_flow), ("shell::host_pdf_export_says_what_is_missing", export_missing_glyphs)]
}

fn source_and_links() {
    let host = MemoryHost::new();
    // A host that gave a sourceUrl: Help > Source Code opens it through the host's openUrl (nothing is started on this machine).
    let options = HostOptions { source_url: Some("https://example.com/src/v1-ibisworks.1".into()), ..HostOptions::default() };
    host::install(Rc::new(host.clone()), HostCapabilities::all(), options);
    let mut s = Scenario::start_view();
    let _ = crate::shell::external::take_launched();
    s.menu("help.source-code");
    s.settle();
    assert_eq!(host.opened_urls(), vec!["https://example.com/src/v1-ibisworks.1".to_string()]);
    assert!(crate::shell::external::take_launched().is_empty(), "the host opens it, not this machine");
    // Without a sourceUrl: the project's page, still through the host.
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    s.menu("help.source-code");
    s.settle();
    assert_eq!(host.opened_urls().last().map(String::as_str), Some(crate::shell::about::PROJECT_URL));
    // A host without openUrl, or with openLinks off: no link opens anywhere.
    let before = host.opened_urls().len();
    host::install(Rc::new(host.clone()), HostCapabilities { open_url: false, ..HostCapabilities::all() }, HostOptions::default());
    s.menu("help.source-code");
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions { open_links: false, ..HostOptions::default() });
    s.menu("help.source-code");
    s.settle();
    assert_eq!(host.opened_urls().len(), before);
    assert!(crate::shell::external::take_launched().is_empty());
    host::uninstall();
    // No host: as before.
    s.menu("help.source-code");
    assert_eq!(crate::shell::external::take_launched(), vec![format!("url {}", crate::shell::about::PROJECT_URL)]);
}

fn logo_opens_start_url() {
    // K0037: with a host the logo opens the host's startUrl through the host's openUrl; without a startUrl it does nothing.
    let host = MemoryHost::new();
    let options = HostOptions { start_url: Some("https://example.com/office-start".into()), ..HostOptions::default() };
    host::install(Rc::new(host.clone()), HostCapabilities::all(), options);
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.size(1200., 800.);
    let _ = crate::shell::external::take_launched();
    {
        let _probe = crate::shell::scenario::button_probe::Probe::new();
        s.update(|_, _, cx| cx.notify());
        crate::shell::scenario::button_probe::click(&mut s, "editor-app-logo");
        s.settle();
    }
    assert_eq!(host.opened_urls(), vec!["https://example.com/office-start".to_string()]);
    assert!(crate::shell::external::take_launched().is_empty(), "the host opens it, not this machine");
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    {
        let _probe = crate::shell::scenario::button_probe::Probe::new();
        s.update(|_, _, cx| cx.notify());
        s.settle();
        // No startUrl: the logo is a picture, not a button (no highlight under the pointer, nothing to click).
        assert!(!crate::shell::scenario::button_probe::all().iter().any(|id| id == "editor-app-logo"), "no button for the logo");
    }
    assert_eq!(host.opened_urls().len(), 1, "no startUrl: the logo does nothing");
    assert!(crate::shell::external::take_launched().is_empty());
    host::uninstall();
}

fn start_view_open() {
    // K0037: a host page with no document (the host's start view): 開く is the host's file window, not the local picker.
    let host = MemoryHost::new();
    host.add_folder(None, "Sales", true);
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    let tree = s.with(|e, _| e.host.dialog.as_ref().map(|d| d.tree_names()).expect("the host's file window is open on the start view"));
    assert_eq!(tree, [(0, "Sales".to_string())], "the host's root, and it is the one shown");
    host::uninstall();
}

fn user_is_author() {
    host::uninstall();
    let system = crate::shell::user_name::user_name();
    let host = MemoryHost::new();
    host::install(Rc::new(host), HostCapabilities::all(), HostOptions { user_name: Some("Hanako Host".into()), ..HostOptions::default() });
    // The comments' and tracked changes' author, and the initials, are the host's user.
    assert_eq!(crate::shell::user_name::user_name(), "Hanako Host");
    assert_eq!(crate::shell::comments::comment_author().author, "Hanako Host");
    assert_eq!(crate::shell::comments::comment_author().initials, "HH");
    // A name the person set in the options of the document wins over it.
    crate::shell::user_name::set_custom_user_info(Some("Other Name".into()), None);
    assert_eq!(crate::shell::user_name::user_name(), "Other Name");
    crate::shell::user_name::set_custom_user_info(None, None);
    host::uninstall();
    assert_eq!(crate::shell::user_name::user_name(), system, "no host: the system's name as before");
}

fn features() {
    use crate::shell::palette;
    let host = MemoryHost::new();
    let mut s = Scenario::start_view();
    // theme: the host's light or dark is the appearance whatever the system says, until the host is gone.
    palette::keep_appearance(|| {
        host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions { dark: Some(true), ..HostOptions::default() });
        s.with_app(|app| palette::follow(gpui_kit::WindowAppearance::Light, None, app));
        assert!(palette::is_dark(), "theme 'dark'");
        host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions { dark: Some(false), ..HostOptions::default() });
        s.with_app(|app| palette::follow(gpui_kit::WindowAppearance::Dark, None, app));
        assert!(!palette::is_dark(), "theme 'light'");
        host::uninstall();
        s.with_app(|app| palette::follow(gpui_kit::WindowAppearance::Dark, None, app));
        assert!(palette::is_dark(), "theme 'auto' (or no host) follows the system");
        s.with_app(|app| palette::follow(gpui_kit::WindowAppearance::Light, None, app));
    });
    // print: with no printPdf (features.print off) the item is disabled and the command does nothing; with it, enabled.
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    host::install(Rc::new(host.clone()), HostCapabilities { print_pdf: false, ..HostCapabilities::all() }, HostOptions::default());
    assert_eq!(s.menu_state("file.print").map(|(enabled, _)| enabled), Some(false));
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    assert_eq!(s.menu_state("file.print").map(|(enabled, _)| enabled), Some(true));
    // export: a list allows those formats only; false allows none; nothing leaves by a download either.
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions { export: Some(vec!["html".into()]), ..HostOptions::default() });
    assert_eq!(s.menu_state("file.export.pdf").map(|(enabled, _)| enabled), Some(false));
    assert!(host::export_allowed("HTML") && !host::export_allowed("pdf"));
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions { export: Some(Vec::new()), ..HostOptions::default() });
    assert!(s.update(|e, w, cx| e.browser_export("a.html".into(), "text/html", b"x".to_vec(), w, cx)), "swallowed: no file window, no download");
    assert!(s.with(|e, _| e.host.dialog.is_none()));
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions { export: None, ..HostOptions::default() });
    assert_eq!(s.menu_state("file.export.pdf").map(|(enabled, _)| enabled), Some(true));
    // fontDownload: off means no font setup.
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions { font_download: false, ..HostOptions::default() });
    assert!(!crate::shell::embed::font_prompt());
    // watermark: drawn over the document (the window renders with it).
    let mark = host::Watermark { text: "CONFIDENTIAL".into(), opacity: host::Watermark::DEFAULT_OPACITY, include_in_print: true };
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions { watermark: Some(mark.clone()), ..HostOptions::default() });
    s.settle();
    assert_eq!(host::watermark(), Some(mark));
    host::uninstall();
    assert_eq!(host::watermark(), None);
}

/// The file window's keys (2.6): up and down in the list, Backspace to the parent, Tab from pane to pane, left and right in the tree.
fn window_keys() {
    let (host, sales, year, _archive) = tree_host();
    let doc = ibis_formats::blank_file(ibis_formats::Kind::Document).expect("a blank document");
    host.add_file(Some(&sales), "c.docx", &doc);
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.open_file(&crate::shell::actions::Open, w, cx));
    s.settle();
    s.update(|e, w, cx| e.host_fd_select_folder(Some(sales.clone()), w, cx));
    s.settle();
    let chosen = |s: &mut Scenario| s.with(|e, _| e.host.dialog.as_ref().and_then(|d| d.selected_name()));
    assert_eq!(chosen(&mut s), None);
    // Down: the first file (the folder row is not a file), then the next, and it stops at the last.
    s.key("down");
    assert_eq!(chosen(&mut s), Some("a.docx".to_string()));
    s.key("down");
    assert_eq!(chosen(&mut s), Some("c.docx".to_string()));
    s.key("down");
    assert_eq!(chosen(&mut s), Some("c.docx".to_string()));
    s.key("up");
    assert_eq!(chosen(&mut s), Some("a.docx".to_string()));
    // Tab takes the keyboard from the search field to the tree; Backspace is the parent folder, down moves the folder shown down the
    // tree (into the one that is open), left closes the open folder and then goes to the parent.
    s.update(|e, w, cx| e.host_fd_select_folder(Some(year.clone()), w, cx));
    s.settle();
    let current = |s: &mut Scenario| s.with(|e, _| e.host.dialog.as_ref().and_then(|d| d.current_folder().map(str::to_string)));
    s.key("tab");
    s.key("backspace");
    s.settle();
    assert_eq!(current(&mut s), Some(sales.clone()), "Backspace: the parent");
    s.key("down");
    s.settle();
    assert_eq!(current(&mut s), Some(year.clone()), "down in the tree: the folder under Sales");
    s.key("left");
    s.settle();
    assert_eq!(current(&mut s), Some(year.clone()), "left closes the open folder first");
    s.key("left");
    s.settle();
    assert_eq!(current(&mut s), Some(sales.clone()), "then goes to the parent");
    s.key("up");
    s.settle();
    assert_eq!(current(&mut s), Some(sales.clone()), "up at the first root of the tree: there is nothing above it");
    s.key("escape");
    host::uninstall();
}

/// 印刷 and 書き出し > PDF in the browser (`shell/web_pdf.rs`), with and without a host. The PDF being made is put in by the test (the
/// real one is made in the page, on wasm). With a host: printBegin, then the PDF, then printPdf (printCancel when it could not be made);
/// the export goes to the file window unless `features.export` says no.
fn print_flow() {
    use crate::shell::web_pdf::{Purpose, WebPdf};
    let pdf = |bytes: Vec<u8>| -> crate::shell::web_pdf::PdfFuture { Box::pin(async move { Ok(WebPdf { bytes, missing_glyphs: 0, pictures_skipped: false }) }) };
    let broken = || -> crate::shell::web_pdf::PdfFuture { Box::pin(async { Err("no pdf".to_string()) }) };
    let host = MemoryHost::new();
    host.add_folder(None, "Sales", true);
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.update(|e, w, cx| e.web_pdf_deliver(Purpose::Print, "report".into(), pdf(vec![1, 2, 3]), w, cx));
    s.settle();
    assert_eq!(host.calls(), ["printBegin", "printPdf"]);
    assert_eq!(host.printed(), vec![("report.pdf".to_string(), vec![1, 2, 3])]);
    // A PDF that cannot be made: the host's tab is closed, nothing is shown to print.
    s.update(|e, w, cx| e.web_pdf_deliver(Purpose::Print, "report".into(), broken(), w, cx));
    s.settle();
    assert_eq!(host.calls()[2..], ["printBegin", "printCancel"]);
    assert_eq!(host.printed().len(), 1);
    // No printPdf (features.print off): the command does nothing at all.
    host::install(Rc::new(host.clone()), HostCapabilities { print_pdf: false, ..HostCapabilities::all() }, HostOptions::default());
    s.update(|e, w, cx| e.print(w, cx));
    s.settle();
    assert_eq!(host.calls().len(), 4);
    // No host: no call to any host, and nothing is lost (the print of the desktop's own path is what runs here).
    host::uninstall();
    s.update(|e, w, cx| e.web_pdf_deliver(Purpose::Print, "report".into(), pdf(vec![7]), w, cx));
    s.settle();
    assert_eq!(host.calls().len(), 4, "a print without a host does not reach a host");
    // 書き出し > PDF with a host: the file window for the PDF; with export off, only the words.
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    s.update(|e, w, cx| e.web_pdf_deliver(Purpose::Export, "report".into(), pdf(vec![9]), w, cx));
    s.settle();
    assert!(s.with(|e, _| e.host.dialog.as_ref().is_some_and(|d| d.mode() == crate::shell::host_file_dialog::Mode::Export)));
    s.key("escape");
    s.settle();
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions { export: Some(Vec::new()), ..HostOptions::default() });
    s.update(|e, w, cx| e.web_pdf_deliver(Purpose::Export, "report".into(), pdf(vec![9]), w, cx));
    s.settle();
    assert!(s.with(|e, _| e.host.dialog.is_none()), "export is off: no file window");
    host::uninstall();
}

/// B-7: an export to the host's storage of a PDF that lacks characters says so at the end (not only 書き出しました), as a print does.
fn export_missing_glyphs() {
    use crate::shell::web_pdf::{Purpose, WebPdf};
    let host = MemoryHost::new();
    let sales = host.add_folder(None, "Sales", true);
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    let made = |missing_glyphs: usize| -> crate::shell::web_pdf::PdfFuture { Box::pin(async move { Ok(WebPdf { bytes: vec![9], missing_glyphs, pictures_skipped: false }) }) };
    for (missing, name) in [(3usize, "lacking.pdf"), (0, "whole.pdf")] {
        s.update(|e, w, cx| e.web_pdf_deliver(Purpose::Export, "report".into(), made(missing), w, cx));
        s.settle();
        s.update(|e, w, cx| e.host_fd_select_folder(Some(sales.clone()), w, cx));
        s.settle();
        s.update(|e, w, cx| e.host_fd_set_name(name, w, cx));
        s.update(|e, w, cx| e.host_fd_confirm(w, cx));
        s.settle();
        assert!(host.calls().contains(&"saveFileAs".to_string()), "the PDF was written");
        let said = s.status_texts().join(" / ");
        if missing > 0 {
            assert!(said.contains(&crate::shell::i18n::format("shell.web-pdf.missing-glyphs", &[("count", &missing)])), "the lack is told: {said}");
        } else {
            assert!(said.contains(crate::shell::i18n::text("shell.host.exported").as_ref()), "{said}");
        }
    }
    host::uninstall();
}

/// A scenario with `name` (bytes `data`) given by a host as its bound file, in the folder `Sales`.
fn start_with(name: &str, data: &[u8]) -> (Scenario, MemoryHost, String, String) {
    crate::shell::host_file_dialog::forget_last_folder();
    let host = MemoryHost::new();
    let folder = host.add_folder(None, "Sales", true);
    let file = host.add_file(Some(&folder), name, data);
    host::install(Rc::new(host.clone()), HostCapabilities::all(), HostOptions::default());
    host.set_current(Some(&file));
    let mut s = Scenario::start_view();
    let version = host.version_of(&file).expect("version");
    let (n, d, f) = (name.to_string(), data.to_vec(), folder.clone());
    s.update(|e, w, cx| e.host_load(n, d, version, Some(f), false, None, true, w, cx).expect("the host's document opens"));
    s.quit_with_last_window();
    (s, host, folder, file)
}

/// K0062: a workbook read from .csv / .tsv goes back to `saveFile` as text (UTF-8 BOM, CRLF), never as the xlsx package.
fn text_saves() {
    for (name, data, text) in [("data.csv", "a,b\r\n1,2\r\n", "a,b\r\n1,2"), ("data.tsv", "a\tb\r\n1\t2\r\n", "a\tb\r\n1\t2"), ("data.tab", "a\tb\r\n1\t2\r\n", "a\tb\r\n1\t2")] {
        let (mut s, host, _folder, file) = start_with(name, data.as_bytes());
        s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
        s.settle();
        assert!(host.calls().contains(&"saveFile".to_string()), "{name}: saveFile");
        let saved = host.bytes_of(&file).expect("bytes");
        assert!(saved.starts_with(&[0xEF, 0xBB, 0xBF]) && !saved.starts_with(b"PK"), "{name}: BOM, not a ZIP");
        let body = String::from_utf8(saved[3..].to_vec()).expect("utf-8");
        assert!(body.starts_with(text), "{name}: {body:?}");
        assert!(body.ends_with("\r\n") && !body.replace("\r\n", "").contains('\n'), "{name}: CRLF only");
        host::uninstall();
    }
}

/// K0062: 名前を付けて保存 writes the format the name's extension says (a text file stays text; a workbook is xlsx).
fn text_save_as() {
    let (mut s, host, folder, _file) = start_with("data.csv", "a,b\r\n1,2\r\n".as_bytes());
    for (typed, saved_name, text) in [("copy", "copy.csv", true), ("copy2.tsv", "copy2.tsv", true)] {
        s.update(|e, w, cx| e.save_as(&crate::shell::actions::SaveAs, w, cx));
        s.settle();
        s.update(|e, w, cx| e.host_fd_select_folder(Some(folder.clone()), w, cx));
        s.settle();
        s.update(|e, w, cx| e.host_fd_set_name(typed, w, cx));
        s.update(|e, w, cx| e.host_fd_confirm(w, cx));
        s.settle();
        let id = host.find_file(Some(&folder), saved_name).unwrap_or_else(|| panic!("{saved_name} was saved"));
        let bytes = host.bytes_of(&id).expect("bytes");
        assert_eq!(!bytes.starts_with(b"PK"), text, "{saved_name}");
        if saved_name.ends_with(".tsv") {
            assert!(String::from_utf8_lossy(&bytes).contains("a\tb"), "tab-separated");
        } else {
            assert!(String::from_utf8_lossy(&bytes).contains("a,b"));
        }
    }
    host::uninstall();
}

/// K0062: a workbook of a package is still saved as xlsx by name.
fn xlsx_save_as() {
    let bytes = ibis_formats::blank_file(ibis_formats::Kind::Workbook).expect("a blank workbook");
    let (mut s, host, folder, _file) = start_with("book.xlsx", &bytes);
    s.update(|e, w, cx| e.save_as(&crate::shell::actions::SaveAs, w, cx));
    s.settle();
    s.update(|e, w, cx| e.host_fd_set_name("other", w, cx));
    s.update(|e, w, cx| e.host_fd_confirm(w, cx));
    s.settle();
    let id = host.find_file(Some(&folder), "other.xlsx").expect("saved as xlsx");
    assert!(host.bytes_of(&id).expect("bytes").starts_with(b"PK"));
    host::uninstall();
}

/// K0062: a file read from an old binary format (.xls) is not written back over its name: 名前を付けて保存 asks for the new name (.xlsx).
fn legacy_saves_as() {
    let blank = ibis_formats::blank_file(ibis_formats::Kind::Workbook).expect("a blank workbook");
    let xls = ibis_formats::write_xls(&ibis_formats::open(blank).expect("opens").presentation).expect("an old binary workbook");
    assert!(ibis_formats::is_legacy_binary(&xls));
    let (mut s, host, folder, _file) = start_with("old.xls", &xls);
    assert!(s.with(|e, _| e.opened_from_legacy), "read as an old binary file");
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    s.settle();
    assert!(!host.calls().contains(&"saveFile".to_string()), "no ZIP over the .xls");
    assert!(s.with(|e, _| e.host.dialog.as_ref().is_some_and(|d| d.mode() == crate::shell::host_file_dialog::Mode::SaveAs)), "save as opens");
    s.update(|e, w, cx| e.host_fd_confirm(w, cx));
    s.settle();
    let id = host.find_file(Some(&folder), "old.xlsx").expect("offered as old.xlsx");
    assert!(host.bytes_of(&id).expect("bytes").starts_with(b"PK"));
    assert!(s.with(|e, _| !e.opened_from_legacy), "the bar is gone");
    host::uninstall();
}

/// K0062 Q1-Q3, Q7: a workbook read from a text file is saved as xlsx when the name typed ends in .xlsx (as Excel's 名前を付けて保存 does for a
/// csv it opened); after that it is a workbook (its 上書き保存 is the package) and nothing says that only the values were kept.
fn text_to_xlsx() {
    for (name, data) in [("data.csv", "a,b\r\n1,2\r\n"), ("data.tsv", "a\tb\r\n1\t2\r\n"), ("data.tab", "a\tb\r\n1\t2\r\n")] {
        let (mut s, host, folder, file) = start_with(name, data.as_bytes());
        s.update(|e, w, cx| e.save_as(&crate::shell::actions::SaveAs, w, cx));
        s.settle();
        s.update(|e, w, cx| e.host_fd_select_folder(Some(folder.clone()), w, cx));
        s.settle();
        s.update(|e, w, cx| e.host_fd_set_name("book.xlsx", w, cx));
        s.update(|e, w, cx| e.host_fd_confirm(w, cx));
        s.settle();
        let id = host.find_file(Some(&folder), "book.xlsx").unwrap_or_else(|| panic!("{name}: book.xlsx was saved"));
        let bytes = host.bytes_of(&id).expect("bytes");
        assert!(bytes.starts_with(b"PK"), "{name}: a package");
        let reread = ibis_formats::open(bytes).expect("the saved xlsx opens");
        let sheet = reread.presentation.slides.iter().flat_map(|slide| slide.shapes.iter()).find_map(|shape| match &shape.kind {
            ibis_doc_model::ShapeKind::Sheet(sheet) => Some(sheet.clone()),
            _ => None,
        });
        assert!(sheet.is_some(), "{name}: the same sheet is in it");
        assert!(!s.status_texts().join(" / ").contains(crate::shell::i18n::text("shell.host.text-save-values-only").as_ref()), "{name}: no values-only note for an xlsx");
        s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
        s.settle();
        assert!(host.bytes_of(&id).expect("bytes").starts_with(b"PK"), "{name}: saved again, still the package");
        // The file the document was read from is untouched.
        assert_eq!(host.bytes_of(&file).expect("bytes"), data.as_bytes(), "{name}: the original");
        host::uninstall();
    }
}

/// K0062 Q5: only opening a text file (plain values: numbers, a leading zero, quotes, a date-like text) and saving it tells nothing.
fn text_open_no_note() {
    let data = "name,qty,when\r\n\"x,y\",001,2026-01-02\r\nb,2.5,\"a \"\"q\"\"\"\r\n";
    let (mut s, host, _folder, file) = start_with("plain.csv", data.as_bytes());
    s.update(|e, w, cx| e.save(&crate::shell::actions::Save, w, cx));
    s.settle();
    assert!(host.calls().contains(&"saveFile".to_string()));
    assert!(host.bytes_of(&file).expect("bytes").starts_with(&[0xEF, 0xBB, 0xBF]));
    let said = s.status_texts().join(" / ");
    assert!(!said.contains(crate::shell::i18n::text("shell.host.text-save-values-only").as_ref()), "no note for plain values: {said}");
    host::uninstall();
}
