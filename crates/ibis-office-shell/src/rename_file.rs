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
//! The browser version has no file path: its ▾ is there only when a host gave `renameFile` or `getFileLocation` (`host.rs`), and the
//! rename is the host's (`HostRename` below; the checks of the name are the same for both). 「パスの取得」 asks the host for the place.

use std::path::PathBuf;
#[cfg(not(target_family = "wasm"))]
use std::path::Path;

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
/// (with its dot): the whole new name. The checks of a name are the same for the desktop (the file system) and the host.
pub(crate) fn check_name(typed: &str, ext: &str) -> Result<String, RenameError> {
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
    Ok(name)
}

/// [`check_name`], and the new name as a path next to `old`: `None` when it is the file's own name.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn rename_target(old: &Path, typed: &str, ext: &str) -> Result<Option<PathBuf>, RenameError> {
    let name = check_name(typed, ext)?;
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
#[cfg(not(target_family = "wasm"))]
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
#[cfg(not(target_family = "wasm"))]
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

/// Whether the ▾ has to do with a host: it is connected and gave a function the menu uses, and the document is its own.
fn host_menu() -> bool {
    let caps = crate::host::capabilities();
    crate::host::is_connected() && (caps.rename_file || caps.get_file_location)
}

impl SlideEditor {
    /// The ▾ right of the file name in the title row, with its menu; `None` for a document that has no file yet (nothing to copy or
    /// rename). With a host the same menu works through the host's functions: an item the host cannot do (it gave no function, or the
    /// document is not its own) is there, disabled, as the menus' rule is. `disabled` while a modal dialog is open.
    pub(crate) fn file_name_menu_button(&self, editor: &WeakEntity<SlideEditor>) -> Option<AnyElement> {
        let on_host = host_menu();
        #[cfg(not(target_family = "wasm"))]
        let has_path = self.path.is_some();
        #[cfg(target_family = "wasm")]
        let has_path = false;
        if !has_path && !on_host {
            return None;
        }
        let caps = crate::host::capabilities();
        let bound = self.is_host_bound();
        let items: Vec<MenuEntry> = if on_host && (bound || !has_path) {
            vec![
                MenuItem::tr("shell.file-name-menu.copy-name").icon("content_copy").on(|e, _, cx| e.copy_file_name(cx)).into(),
                MenuItem::tr("shell.file-name-menu.get-path").icon("link").disabled(!(caps.get_file_location && bound)).on(|e, _, cx| e.host_get_file_location(cx)).into(),
                MenuEntry::Separator,
                MenuItem::tr("shell.file-name-menu.rename").icon("edit").disabled(!(caps.rename_file && bound) || self.is_read_only()).on(|e, window, cx| e.open_rename_file_dialog(window, cx)).into(),
            ]
        } else {
            vec![
                MenuItem::tr("shell.file-name-menu.copy-name").icon("content_copy").on(|e, _, cx| e.copy_file_name(cx)).into(),
                MenuItem::tr("shell.file-name-menu.copy-path").icon("link").on(|e, _, cx| e.copy_file_path(cx)).into(),
                MenuEntry::Separator,
                MenuItem::tr("shell.file-name-menu.rename").icon("edit").on(|e, window, cx| e.open_rename_file_dialog(window, cx)).into(),
            ]
        };
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
        #[cfg(not(target_family = "wasm"))]
        let from_path = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        #[cfg(target_family = "wasm")]
        let from_path: Option<String> = None;
        let Some(name) = from_path.or_else(|| self.file_name.as_ref().map(|n| n.to_string())) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(name));
        self.set_status(text("shell.file-name-menu.name-copied").to_string(), cx);
    }

    /// ファイルパスをコピー: the full path (the desktop; the host's is 「パスの取得」, `host_get_file_location`).
    pub(crate) fn copy_file_path(&mut self, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        {
            let Some(path) = self.path.as_ref().map(|p| crate::plain_path::plain(p).to_string_lossy().into_owned()) else { return };
            cx.write_to_clipboard(ClipboardItem::new_string(path));
            self.set_status(text("shell.file-name-menu.path-copied").to_string(), cx);
        }
        #[cfg(target_family = "wasm")]
        let _ = cx;
    }

    /// パスの取得: the host's `getFileLocation` (the place it decided: a URL, ...) goes to the clipboard.
    pub fn host_get_file_location(&mut self, cx: &mut Context<Self>) {
        let Some(bridge) = crate::host::bridge_with(|c| c.get_file_location) else { return };
        cx.spawn(async move |this, cx| {
            let result = bridge.get_file_location().await;
            this.update(cx, |this, cx| match result {
                Ok(url) => {
                    // The browser can refuse the clipboard (no permission): that is told in the status bar, not left as an unhandled rejection.
                    #[cfg(not(target_family = "wasm"))]
                    {
                        cx.write_to_clipboard(ClipboardItem::new_string(url));
                        this.set_status(text("shell.file-name-menu.path-copied").to_string(), cx);
                    }
                    #[cfg(target_family = "wasm")]
                    cx.spawn(async move |this, cx| {
                        let copied = crate::web::copy_text(&url).await;
                        this.update(cx, |this, cx| this.set_status(text(if copied { "shell.file-name-menu.path-copied" } else { "shell.file-name-menu.path-copy-failed" }).to_string(), cx)).ok();
                    })
                    .detach();
                }
                Err(failure) => {
                    let message = failure.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| text("shell.host-file.error-path").to_string());
                    this.show_alert(text("shell.host-file.error-path-title"), message.into(), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// ファイル名を変更…: the dialog with the name (without its extension) to edit.
    pub fn open_rename_file_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        #[cfg(not(target_family = "wasm"))]
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        #[cfg(target_family = "wasm")]
        let name: Option<String> = None;
        // A host's document is renamed by the host: its name is the window's (the desktop's path is for the file system's).
        let Some(name) = (if self.is_host_bound() { self.file_name.as_ref().map(|n| n.to_string()) } else { name.or_else(|| self.file_name.as_ref().map(|n| n.to_string())) }) else { return };
        self.end_text_edit(window, cx);
        if self.dialog.active {
            self.close_dialog(window, cx);
        }
        let (stem, extension) = match name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
            _ => (name.clone(), String::new()),
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(stem));
        // The name opens all selected, so typing replaces it (K0040).
        input.update(cx, |state, cx| {
            state.focus(window, cx);
            state.select_all(window, cx);
        });
        self.rename_file = Some(RenameFileState { input, extension });
        self.sidebar_open = true;
        cx.notify();
    }

    /// 名前を変更 of the dialog: with a host, its `renameFile`; else rename the file on disk, then make the window follow it. An error is
    /// told in the message box and the dialog stays, so the name can be corrected.
    pub fn apply_rename_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((typed, ext)) = self.rename_file.as_ref().map(|s| (s.input.read(cx).value().to_string(), s.extension.clone())) else { return };
        if self.is_host_bound() {
            return self.apply_rename_on_host(typed, ext, window, cx);
        }
        #[cfg(not(target_family = "wasm"))]
        {
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
    }

    /// 名前を変更 with a host: the host renames (it holds the lock and the permission); the document keeps its edits, and the window
    /// takes the name and version the host gives back. A refusal is told in the message box with the host's words.
    fn apply_rename_on_host(&mut self, typed: String, ext: String, window: &mut Window, cx: &mut Context<Self>) {
        let name = match check_name(&typed, &ext) {
            Ok(name) => name,
            Err(error) => return self.show_alert(text("shell.rename-file.error-title"), error.message().into(), cx),
        };
        if self.file_name.as_ref().is_some_and(|n| n.as_ref() == name) {
            return self.close_dialog(window, cx);
        }
        let Some(bridge) = crate::host::bridge_with(|c| c.rename_file) else { return };
        let Some(version) = self.host.binding.as_ref().map(|b| b.version.clone()) else { return };
        cx.spawn_in(window, async move |this, cx| {
            let result = bridge.rename_file(name, version).await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(renamed) => {
                    if let Some(binding) = &mut this.host.binding {
                        binding.version = renamed.version;
                    }
                    this.file_name = Some(renamed.name.clone().into());
                    window.set_window_title(&format!("{} - ibisOffice", renamed.name));
                    this.close_dialog(window, cx);
                    this.set_status(text("shell.file-name-menu.renamed").to_string(), cx);
                }
                Err(failure) => {
                    let message = failure.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| text("shell.host-file.error-rename").to_string());
                    this.show_alert(text("shell.rename-file.error-title"), message.into(), cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// The file at `old` is now at `new` (same folder): the path saving writes to, the name and title, the recent files, the watching for
    /// other apps' changes. The windows that show the same workbook or document (a second window) follow too.
    #[cfg(not(target_family = "wasm"))]
    fn adopt_renamed_path(&mut self, old: &Path, new: &Path, window: &mut Window, cx: &mut Context<Self>) {
        // The list keeps the path as `add` stored it (the absolute one).
        crate::recent_files::remove(old);
        if let Ok(absolute) = crate::plain_path::canonical(old) {
            crate::recent_files::remove(&absolute);
        }
        self.recent_files = crate::recent_files::add(new);
        crate::app::set_app_menus(cx);
        self.follow_renamed_path(new, window, cx);
        let me = cx.entity_id();
        let peers = if self.format_info().several_windows.is_some() { Self::find_other_matching_windows(self.file_id, me, cx) } else { Vec::new() };
        for (handle, peer) in peers {
            let new = new.to_path_buf();
            let _ = handle.update(cx, |_, window, app| peer.update(app, |peer, cx| peer.follow_renamed_path(&new, window, cx)));
        }
    }

    #[cfg(not(target_family = "wasm"))]
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
