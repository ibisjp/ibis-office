// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The menu at the file name in the title bar (shell/rename_file.rs), in the document, spreadsheet and presentation screens: the
//! ▾ right of the name, ファイル名をコピー / ファイルパスをコピー / ファイル名を変更… (renames the file on disk at once), and
//! every error of the rename told in the message box. Pictures: `target/scenario/shell__title_file_menu_*/`.
use std::path::{Path, PathBuf};

use super::{Scenario, button_probe as buttons};
use crate::docx::scenarios::DocSteps;
use crate::shell::dialog::InteractionLayer;
use crate::shell::rename_file::RenameError;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use ibis_pptx::Kind;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::title_file_menu_docx", docx),
        ("shell::title_file_menu_xlsx", xlsx),
        ("shell::title_file_menu_pptx", pptx),
        ("shell::title_file_menu_errors_docx", errors_docx),
        ("shell::title_file_menu_errors_xlsx", errors_xlsx),
        ("shell::title_file_menu_errors_pptx", errors_pptx),
        ("shell::title_file_menu_no_file_no_button", no_file_no_button),
    ]
}

fn docx() {
    flow(Kind::Document, false);
}
fn xlsx() {
    flow(Kind::Workbook, false);
}
/// The presentation's rename is confirmed with Enter (the other two with the button).
fn pptx() {
    flow(Kind::Presentation, true);
}
fn errors_docx() {
    errors(Kind::Document);
}
fn errors_xlsx() {
    errors(Kind::Workbook);
}
fn errors_pptx() {
    errors(Kind::Presentation);
}

fn sample(kind: Kind) -> &'static str {
    match kind {
        Kind::Document => "test/editing/docx-report.docx",
        Kind::Workbook => "test/editing/xlsx-sales.xlsx",
        Kind::Presentation => "test/editing/pptx-comments-modern.pptx",
    }
}

fn path_of(s: &mut Scenario) -> PathBuf {
    s.with(|e, _| e.path.clone()).expect("the file has a path")
}

fn clipboard(s: &mut Scenario) -> String {
    s.with(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()).expect("text on the clipboard"))
}

/// Leave the unsaved change in the document (the rename must not lose it).
fn edit(s: &mut Scenario, kind: Kind) {
    match kind {
        Kind::Document => {
            let (id, _) = s.paragraph(0);
            s.click_text(ibis_doc_model::FlowPos::new(id, 0));
            s.type_text("unsaved");
        }
        Kind::Workbook => {
            s.click_cell("A1");
            s.type_text("unsaved");
            s.key("enter");
        }
        Kind::Presentation => s.update(|e, _, cx| {
            let id = e.document.ids().new_shape_id();
            let frame = ibis_doc_model::EmuRect::new(0, 0, 914_400, 914_400);
            e.execute(vec![ibis_doc_model::Command::InsertShape { slide: e.current_slide, index: 0, shape: ibis_doc_model::Shape::text_box(id, frame, "unsaved") }], cx);
        }),
    }
    assert!(s.with(|e, _| e.is_dirty()));
}

fn center(b: Bounds<Pixels>) -> (f32, f32) {
    (f32::from(b.center().x), f32::from(b.center().y))
}

/// Press the ▾ and the menu's item `id`.
fn pick(s: &mut Scenario, id: &str) {
    buttons::click(s, "title-file-menu");
    buttons::click(s, &format!("menu-item-shell.file-name-menu.{id}"));
}

/// Type `name` as the dialog's name (the field is replaced).
fn set_name(s: &mut Scenario, name: &str) {
    let name = name.to_string();
    s.update(move |e, window, cx| {
        let input = e.rename_file.as_ref().expect("the rename dialog is open").input.clone();
        input.update(cx, |i, cx| i.set_value(name, window, cx));
    });
}

fn dialog_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.rename_file.is_some() && e.dialog.active && e.dialog.key == Some("rename_file_panel"))
}

fn alert_message(s: &mut Scenario) -> Option<String> {
    s.with(|e, _| e.alert.as_ref().filter(|_| e.interaction_layer() == InteractionLayer::Alert).map(|(_, m)| m.to_string()))
}

fn dismiss_alert(s: &mut Scenario) {
    // Straight at the button: the box is over the dialog, which `buttons::click` would scroll.
    let ok = buttons::bounds("alert-ok").expect("the message box has OK");
    s.click(center(ok));
    assert!(alert_message(s).is_none(), "OK closes the message box");
}

/// The menu, the two copies, the rename (an unsaved document is renamed and keeps its changes) and a save to the new name.
fn flow(kind: Kind, confirm_with_enter: bool) {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(sample(kind));
    let old = path_of(&mut s);
    let old_name = old.file_name().unwrap().to_string_lossy().into_owned();
    let ext = format!(".{}", old.extension().unwrap().to_string_lossy());
    // The scenario folder survives between runs: a file renamed by the last run would make this rename "exist already".
    let _ = std::fs::remove_file(old.with_file_name(format!("renamed file{ext}")));
    s.screenshot("1-title-with-button");

    // The ▾ is right of the name, in the title row.
    let caret = buttons::bounds("title-file-menu").expect("the ▾ is drawn when the file has a path");
    assert!(caret.bottom() <= px(40.), "the ▾ is in the title row: {caret:?}");
    // The menu hangs from the button's lower left corner (dropdown_list::below_button).
    buttons::click(&mut s, "title-file-menu");
    let first = buttons::bounds("menu-item-shell.file-name-menu.copy-name").expect("ファイル名をコピー is in the menu"); // i18n-ok (Japanese text under test or a name table that matches labels)
    assert!(buttons::bounds("menu-item-shell.file-name-menu.copy-path").is_some());
    assert!(buttons::bounds("menu-item-shell.file-name-menu.rename").is_some());
    assert!(first.origin.y >= caret.bottom() - px(2.), "the menu is under the button: {first:?} {caret:?}");
    assert!((f32::from(first.origin.x) - f32::from(caret.origin.x)).abs() <= 32., "the menu's left edge is at the button's (the item's text starts right of the icon column): {first:?} {caret:?}");
    s.screenshot("2-menu");
    buttons::click(&mut s, "menu-item-shell.file-name-menu.copy-name");

    // ファイル名をコピー: the name with its extension. ファイルパスをコピー: the full path.
    assert_eq!(clipboard(&mut s), old_name);
    pick_again(&mut s, "copy-path");
    assert_eq!(clipboard(&mut s), crate::shell::plain_path::plain(&old).to_string_lossy());

    // ファイル名を変更…: the dialog has the name without its extension.
    edit(&mut s, kind);
    pick_again(&mut s, "rename");
    assert!(dialog_open(&mut s), "the rename dialog is open");
    let (shown, kept) = s.with(|e, cx| {
        let state = e.rename_file.as_ref().unwrap();
        (state.input.read(cx).value().to_string(), state.extension.clone())
    });
    assert_eq!(shown, old.file_stem().unwrap().to_string_lossy());
    assert_eq!(kept, ext, "the extension is kept apart, not editable");
    s.screenshot("3-dialog");
    set_name(&mut s, "renamed file");
    s.screenshot("4-dialog-typed");
    if confirm_with_enter {
        s.key("enter");
    } else {
        buttons::click(&mut s, "rename-file-apply");
    }
    let alert = alert_message(&mut s);
    assert!(!dialog_open(&mut s), "the dialog closes (alert: {alert:?})");
    assert!(alert_message(&mut s).is_none(), "no error: {:?}", alert_message(&mut s));

    // The file on disk is renamed at once, in the same folder, with the extension.
    let new = old.with_file_name(format!("renamed file{ext}"));
    assert!(!old.exists() && new.is_file(), "renamed on disk: {} -> {}", old.display(), new.display());
    assert_eq!(path_of(&mut s), new, "the path saving writes to");
    assert_eq!(s.with(|e, _| e.file_name.clone()).map(|n| n.to_string()), Some(format!("renamed file{ext}")));
    assert_eq!(s.with(|e, _| e.display_title_formatted()), format!("renamed file{ext}"));
    // The unsaved change is still there, and the document is still unsaved.
    assert!(s.with(|e, _| e.is_dirty()), "the rename leaves the document's changes");
    // Recent files and the watching for other apps' changes follow the new name.
    let recent = s.with(|e, _| e.recent_files.clone());
    let canonical_new = std::fs::canonicalize(&new).unwrap();
    assert!(recent.iter().any(|p| *p == canonical_new || *p == new), "the new path is recent: {recent:?}");
    let canonical_old = std::fs::canonicalize(old.parent().unwrap()).map(|d| d.join(old.file_name().unwrap())).unwrap_or(old.clone());
    assert!(!recent.iter().any(|p| *p == old || *p == canonical_old), "the old path is not: {recent:?}");
    assert_eq!(s.with(|e, _| e.watch.known.clone()), crate::shell::file_watch::DiskStamp::stat(&new), "the file's stamp is the new file's");
    s.update(|e, _, cx| e.check_external_update(cx));
    assert!(s.with(|e, _| !e.watch.dialog), "the rename is not another app's change");
    s.screenshot("5-renamed");

    // The title shows the new name (the picture), and the menu now copies it.
    pick_again(&mut s, "copy-name");
    assert_eq!(clipboard(&mut s), format!("renamed file{ext}"));

    // 保存 writes the new name (the old name is not made again), with the change.
    let before = std::fs::metadata(&new).unwrap().len();
    s.update(|e, window, cx| e.save(&crate::Save, window, cx));
    assert!(!old.exists(), "saving does not write the old name");
    assert!(new.is_file());
    assert!(!s.with(|e, _| e.is_dirty()), "saved");
    let _ = before;
    s.screenshot("6-saved");
}

/// The ▾, then the item `id`, when the previous menu has been used already.
fn pick_again(s: &mut Scenario, id: &str) {
    pick(s, id);
}

/// Every failure of the rename is told in the message box; the file stays, and so does the dialog.
fn errors(kind: Kind) {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(sample(kind));
    let old = path_of(&mut s);
    let ext = format!(".{}", old.extension().unwrap().to_string_lossy());
    let folder = old.parent().unwrap().to_path_buf();
    let original = std::fs::read(&old).unwrap();
    let still = |s: &mut Scenario, what: &str| {
        assert!(old.is_file(), "{what}: the file is where it was");
        assert_eq!(path_of(s), old, "{what}: the window's path is unchanged");
        assert!(dialog_open(s), "{what}: the dialog stays open for another name");
    };
    // Try `name` in the dialog; the message box has `expected`.
    let try_name = |s: &mut Scenario, name: &str, expected: RenameError, shot: &str| {
        set_name(s, name);
        buttons::click(s, "rename-file-apply");
        let message = alert_message(s).unwrap_or_else(|| panic!("{name:?}: no message box"));
        assert_eq!(message, expected.message(), "{name:?}");
        assert!(!message.is_empty());
        if !shot.is_empty() {
            s.screenshot(shot);
        }
        dismiss_alert(s);
        still(s, name);
    };
    pick(&mut s, "rename");
    assert!(dialog_open(&mut s));

    // 空の名前, 使えない文字, 末尾のピリオド・空白.
    try_name(&mut s, "", RenameError::Empty, "1-error-empty");
    try_name(&mut s, "   ", RenameError::Empty, "");
    try_name(&mut s, "a/b", RenameError::BadChar('/'), "2-error-char");
    try_name(&mut s, "a:b", RenameError::BadChar(':'), "");
    try_name(&mut s, "a?b", RenameError::BadChar('?'), "");
    try_name(&mut s, "name.", RenameError::BadEnd, "");
    try_name(&mut s, &"x".repeat(300), RenameError::TooLong, "");

    // 同名のファイルが既にある: 上書きしない.
    let other = folder.join(format!("already-there{ext}"));
    std::fs::write(&other, b"another file").unwrap();
    try_name(&mut s, "already-there", RenameError::Exists(format!("already-there{ext}")), "3-error-exists");
    assert_eq!(std::fs::read(&other).unwrap(), b"another file", "the other file is not overwritten");
    assert_eq!(std::fs::read(&old).unwrap(), original, "the file is untouched");
    // Typing the extension too finds the same file.
    try_name(&mut s, &format!("already-there{ext}"), RenameError::Exists(format!("already-there{ext}")), "");

    // 読み取り専用のフォルダー (permission): not for the superuser, who can rename anywhere.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let writable = std::fs::metadata(&folder).unwrap().permissions();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
        let can_still = std::fs::write(folder.join(".probe"), b"").is_ok();
        let _ = std::fs::remove_file(folder.join(".probe"));
        if !can_still {
            set_name(&mut s, "no-permission");
            buttons::click(&mut s, "rename-file-apply");
            std::fs::set_permissions(&folder, writable.clone()).unwrap();
            let message = alert_message(&mut s).expect("a message box for the folder that cannot be written");
            assert_eq!(message, RenameError::Permission(old.clone()).message());
            s.screenshot("4-error-permission");
            dismiss_alert(&mut s);
            still(&mut s, "permission");
        } else {
            std::fs::set_permissions(&folder, writable).unwrap();
        }
    }

    // ファイルが無い (moved or deleted by another app).
    std::fs::remove_file(&other).unwrap();
    let moved = folder.join(format!("moved-away{ext}"));
    std::fs::rename(&old, &moved).unwrap();
    set_name(&mut s, "after-moved");
    buttons::click(&mut s, "rename-file-apply");
    let message = alert_message(&mut s).expect("a message box for the missing file");
    assert_eq!(message, RenameError::Missing(old.clone()).message());
    s.screenshot("5-error-missing");
    dismiss_alert(&mut s);
    assert!(dialog_open(&mut s) && !folder.join(format!("after-moved{ext}")).exists());
    assert_eq!(path_of(&mut s), old, "the window keeps its path when nothing was renamed");

    // The file back: its own name closes the dialog with nothing done and no message.
    std::fs::rename(&moved, &old).unwrap();
    set_name(&mut s, &old.file_stem().unwrap().to_string_lossy());
    buttons::click(&mut s, "rename-file-apply");
    assert!(alert_message(&mut s).is_none() && !dialog_open(&mut s), "the same name is no error and closes the dialog");
    assert!(old.is_file());

    // Cancel leaves it all as it was.
    pick_again(&mut s, "rename");
    set_name(&mut s, "never-used");
    buttons::click(&mut s, "dialog-cancel");
    assert!(!dialog_open(&mut s) && old.is_file() && !folder.join(format!("never-used{ext}")).exists());

    // Only the case differs (the same file on a case-insensitive file system, a plain rename elsewhere): no clash.
    pick_again(&mut s, "rename");
    let upper = old.file_stem().unwrap().to_string_lossy().to_uppercase();
    set_name(&mut s, &upper);
    buttons::click(&mut s, "rename-file-apply");
    assert!(alert_message(&mut s).is_none(), "a change of case is a rename: {:?}", alert_message(&mut s));
    let names: Vec<String> = std::fs::read_dir(&folder).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert!(names.contains(&format!("{upper}{ext}")), "the new case is on disk: {names:?}");
    assert_eq!(std::fs::read(path_of(&mut s)).unwrap(), original, "the same content");
    let _: &Path = &folder;
}

/// A new document has no file: no ▾ (nothing to copy or rename). The three screens.
fn no_file_no_button() {
    for kind in [Kind::Document, Kind::Workbook, Kind::Presentation] {
        let _probe = buttons::Probe::new();
        let mut s = Scenario::new_document(kind);
        assert!(buttons::bounds("title-file-menu").is_none(), "{kind:?}: no ▾ for a document with no file");
        s.screenshot("new-document");
    }
}
