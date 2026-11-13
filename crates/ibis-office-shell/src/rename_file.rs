// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The menu at the file name in the title bar (the document, spreadsheet and presentation screens alike): a small ▾ right of the
//! name opens ファイル名をコピー / ファイルパスをコピー / ファイル名を変更….
//!
//! 変更 renames the file ON DISK at once (`std::fs::rename`, inside its folder; the extension stays, and is not editable). It never
//! overwrites a file. A failure is told in the common message box (`show_alert`) and the dialog stays, so the name can be corrected.
//! A rename leaves the window's content alone (an unsaved document stays unsaved, with its changes); the window title, the recent
//! files, the watching for other apps' changes and the path the next save writes to all follow the new name.
//!
//! The browser version has no file path and nothing to rename, so it has no ▾ (the whole module is for the other platforms).

use std::path::{Path, PathBuf};

use gpui_kit::component::input::InputState;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{Disableable as _, Icon, Sizable as _};
use gpui_kit::*;

use crate::chrome::{ActionButtonKind, ActionDefinition, MenuEntry, MenuItem, SidebarPanel};
use crate::editor::SlideEditor;
use crate::palette::rgb;
use crate::i18n::{format, text};
use crate::icons::Material;

/// A file name's longest length in bytes (what the common file systems allow).
const NAME_MAX_BYTES: usize = 255;

/// The dialog's state: the name without its extension, as it is being edited, and the extension that stays.
pub struct RenameFileState {
    pub input: Entity<InputState>,
    /// With its dot (`.docx`), as the file has it; empty for a file with none.
    pub extension: String,
}

/// Why a file cannot be renamed.
#[derive(Debug, PartialEq, Eq)]
pub enum RenameError {
    Empty,
    BadChar(char),
    BadEnd,
    Reserved(String),
    TooLong,
    Missing(PathBuf),
    Exists(String),
    Permission(PathBuf),
    ReadOnly(PathBuf),
    /// Another app has the file open (Windows' sharing violation).
    #[cfg_attr(not(windows), allow(dead_code))]
    InUse(PathBuf),
    Other(String),
}

impl RenameError {
    /// The message of the message box, in the UI's language.
    pub fn message(&self) -> String {
        match self {
            Self::Empty => text("shell.rename-file.error-empty").to_string(),
            Self::BadChar(c) => format("shell.rename-file.error-char", &[("char", &c)]),
            Self::BadEnd => text("shell.rename-file.error-end").to_string(),
            Self::Reserved(name) => format("shell.rename-file.error-reserved", &[("name", name)]),
            Self::TooLong => text("shell.rename-file.error-long").to_string(),
            Self::Missing(path) => format("shell.rename-file.error-missing", &[("path", &path.display())]),
            Self::Exists(name) => format("shell.rename-file.error-exists", &[("name", name)]),
            Self::Permission(path) => format("shell.rename-file.error-permission", &[("path", &path.display())]),
            Self::ReadOnly(path) => format("shell.rename-file.error-readonly", &[("path", &path.display())]),
            Self::InUse(path) => format("shell.rename-file.error-in-use", &[("path", &path.display())]),
            Self::Other(error) => format("shell.rename-file.error-other", &[("error", error)]),
        }
    }
}

/// Whether `name` (without the extension) is one the system reserves (Windows only: CON, NUL, COM1…).
fn is_reserved(name: &str) -> bool {
    if !cfg!(target_os = "windows") {
        return false;
    }
    let stem = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && stem.as_bytes()[3].is_ascii_digit() && stem.as_bytes()[3] != b'0')
}

/// The name `typed` (without the extension; the extension is stripped if it was typed too) checked for a file whose extension is `ext`
/// (with its dot): the new name as a path next to `old`, `None` when it is the file's own name.
pub(crate) fn rename_target(old: &Path, typed: &str, ext: &str) -> Result<Option<PathBuf>, RenameError> {
    let mut stem = typed.trim();
    if !ext.is_empty() && stem.len() > ext.len() && stem.is_char_boundary(stem.len() - ext.len()) && stem[stem.len() - ext.len()..].eq_ignore_ascii_case(ext) {
        stem = stem[..stem.len() - ext.len()].trim_end();
    }
    if stem.is_empty() {
        return Err(RenameError::Empty);
    }
    if let Some(c) = stem.chars().find(|c| c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')) {
        return Err(RenameError::BadChar(c));
    }
    if stem.ends_with('.') || stem.ends_with(' ') {
        return Err(RenameError::BadEnd);
    }
    if is_reserved(stem) {
        return Err(RenameError::Reserved(stem.to_string()));
    }
    let name = format!("{stem}{ext}");
    if name.len() > NAME_MAX_BYTES {
        return Err(RenameError::TooLong);
    }
    let new = old.with_file_name(&name);
    if new == old {
        return Ok(None);
    }
    if std::fs::symlink_metadata(old).is_err() {
        return Err(RenameError::Missing(old.to_path_buf()));
    }
    // A name that differs only in case is the same file on a case-insensitive file system: it is a rename, not a clash.
    if std::fs::symlink_metadata(&new).is_ok() {
        let same = matches!((std::fs::canonicalize(old), std::fs::canonicalize(&new)), (Ok(a), Ok(b)) if a == b);
        if !same {
            return Err(RenameError::Exists(name));
        }
    }
    Ok(Some(new))
}

/// The error of `std::fs::rename` as the message box tells it.
fn io_error(old: &Path, error: &std::io::Error) -> RenameError {
    use std::io::ErrorKind;
    #[cfg(unix)]
    const EROFS: i32 = 30;
    #[cfg(windows)]
    const ERROR_SHARING_VIOLATION: i32 = 32;
    match error.kind() {
        ErrorKind::NotFound => return RenameError::Missing(old.to_path_buf()),
        ErrorKind::AlreadyExists => return RenameError::Exists(String::new()),
        _ => {}
    }
    #[cfg(unix)]
    if error.raw_os_error() == Some(EROFS) {
        return RenameError::ReadOnly(old.to_path_buf());
    }
    #[cfg(windows)]
    if error.raw_os_error() == Some(ERROR_SHARING_VIOLATION) {
        return RenameError::InUse(old.to_path_buf());
    }
    if error.kind() == ErrorKind::PermissionDenied {
        return RenameError::Permission(old.to_path_buf());
    }
    RenameError::Other(error.to_string())
}

/// Rename `old` to the name `typed` (see [`rename_target`]) on disk. `Ok(None)` when the name is the file's own (nothing done).
pub(crate) fn rename_on_disk(old: &Path, typed: &str, ext: &str) -> Result<Option<PathBuf>, RenameError> {
    let Some(new) = rename_target(old, typed, ext)? else { return Ok(None) };
    match std::fs::rename(old, &new) {
        Ok(()) => Ok(Some(new)),
        Err(error) => Err(match io_error(old, &error) {
            RenameError::Exists(_) => RenameError::Exists(new.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            other => other,
        }),
    }
}

impl SlideEditor {
    /// The ▾ right of the file name in the title row, with its menu; `None` for a document that has no file yet (nothing to copy or
    /// rename). `disabled` while a modal dialog is open.
    pub(crate) fn file_name_menu_button(&self, editor: &WeakEntity<SlideEditor>) -> Option<AnyElement> {
        self.path.as_ref()?;
        let items: Vec<MenuEntry> = vec![
            MenuItem::tr("shell.file-name-menu.copy-name").icon("content_copy").on(|e, _, cx| e.copy_file_name(cx)).into(),
            MenuItem::tr("shell.file-name-menu.copy-path").icon("link").on(|e, _, cx| e.copy_file_path(cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("shell.file-name-menu.rename").icon("edit").on(|e, window, cx| e.open_rename_file_dialog(window, cx)).into(),
        ];
        let editor = editor.clone();
        let id = "title-file-menu";
        let button = Button::new(id)
            .ghost()
            .xsmall()
            .h(px(24.))
            .w(px(22.))
            .rounded(px(4.))
            .icon(Icon::new(Material("arrow_drop_down")))
            .tooltip(text("shell.file-name-menu.tooltip"))
            .disabled(self.dialog_is_modal());
        #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
        let button = crate::scenario::button_probe::observe(button, id.into());
        Some(
            button
                .dropdown_menu(move |menu, window, cx| crate::chrome::fill_menu(menu.min_w(px(200.)), &items, &editor, window, cx))
                .into_any_element(),
        )
    }

    /// ファイル名をコピー: the name with its extension.
    pub(crate) fn copy_file_name(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(name));
        self.set_status(text("shell.file-name-menu.name-copied").to_string(), cx);
    }

    /// ファイルパスをコピー: the full path.
    pub(crate) fn copy_file_path(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.as_ref().map(|p| crate::plain_path::plain(p).to_string_lossy().into_owned()) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(path));
        self.set_status(text("shell.file-name-menu.path-copied").to_string(), cx);
    }

    /// ファイル名を変更…: the dialog with the name (without its extension) to edit.
    pub(crate) fn open_rename_file_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else { return };
        self.end_text_edit(window, cx);
        if self.dialog.active {
            self.close_dialog(window, cx);
        }
        let stem = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let extension = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(stem));
        input.update(cx, |state, cx| state.focus(window, cx));
        self.rename_file = Some(RenameFileState { input, extension });
        self.sidebar_open = true;
        cx.notify();
    }

    /// 名前を変更 of the dialog: rename the file on disk, then make the window follow it. An error is told in the message box.
    pub(crate) fn apply_rename_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((typed, ext)) = self.rename_file.as_ref().map(|s| (s.input.read(cx).value().to_string(), s.extension.clone())) else { return };
        let Some(old) = self.path.clone() else { return };
        match rename_on_disk(&old, &typed, &ext) {
            Ok(None) => self.close_dialog(window, cx),
            Ok(Some(new)) => {
                self.close_dialog(window, cx);
                self.adopt_renamed_path(&old, &new, window, cx);
                self.set_status(text("shell.file-name-menu.renamed").to_string(), cx);
            }
            Err(error) => self.show_alert(text("shell.rename-file.error-title"), error.message().into(), cx),
        }
    }

    /// The file at `old` is now at `new` (same folder): the path saving writes to, the name and title, the recent files, the watching for
    /// other apps' changes. The windows that show the same workbook or document (a second window) follow too.
    fn adopt_renamed_path(&mut self, old: &Path, new: &Path, window: &mut Window, cx: &mut Context<Self>) {
        // The list keeps the path as `add` stored it (the absolute one).
        crate::recent_files::remove(old);
        if let Ok(absolute) = crate::plain_path::canonical(old) {
            crate::recent_files::remove(&absolute);
        }
        self.recent_files = crate::recent_files::add(new);
        self.follow_renamed_path(new, window, cx);
        let me = cx.entity_id();
        let peers = if self.format_info().several_windows.is_some() { Self::find_other_matching_windows(self.file_id, me, cx) } else { Vec::new() };
        for (handle, peer) in peers {
            let new = new.to_path_buf();
            let _ = handle.update(cx, |_, window, app| peer.update(app, |peer, cx| peer.follow_renamed_path(&new, window, cx)));
        }
    }

    fn follow_renamed_path(&mut self, new: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let name: SharedString = new.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into();
        self.file_name = Some(name);
        // Updates the document's file, the stamp the watching compares with (a rename keeps the file's time and size) and the folder watched.
        self.set_file_path(Some(new.to_path_buf()));
        self.watch.asked = None;
        window.set_window_title(&format!("{} - ibisOffice", self.display_title_formatted()));
        cx.notify();
    }

    /// The dialog's panel.
    pub(crate) fn rename_file_panel_if_open(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.rename_file.as_ref()?;
        let input = state.input.clone();
        let extension = state.extension.clone();
        let mut body = crate::chrome::panel_form().child(
            crate::chrome::text_field_row(text("shell.rename-file.name"), crate::chrome::transient_text_field(&input).full_width()),
        );
        if !extension.is_empty() {
            body = body.child(div().text_color(rgb(crate::chrome::TEXT_SECONDARY)).child(format("shell.rename-file.extension-note", &[("extension", &extension)])));
        }
        let actions = crate::chrome::action_bar()
            .action(ActionDefinition::new("rename-file-apply", text("shell.rename-file.apply"), ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_rename_file(w, cx))))
            .action(crate::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("shell.rename-file.title"), body.into_any_element(), actions))
    }
}
