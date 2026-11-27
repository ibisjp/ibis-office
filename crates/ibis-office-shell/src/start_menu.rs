// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The menus of the start window (K0052): ファイル, ウィンドウ and ヘルプ.
//!
//! On Windows and Linux the start window draws them in the menu bar below its title row (the same bar, menus and rows as a document
//! window's: `menu_bar.rs`). On macOS the system's menu bar has them (`app::set_app_menus`), the same commands by the same actions.
//! ⌘D, ⌘E and ⌘P (new document, spreadsheet, presentation) are the start window's own keys: they are bound in `START_CONTEXT`, which only
//! the start window has, so ⌘P is still the print key of a document window.

use ibis_formats::Kind;

use crate::chrome::{MenuEntry, MenuId};
use crate::chrome_defaults::{item, recent_entries, SEP};
use crate::editor::SlideEditor;

/// The key context only a window with no file open (the start window) has, besides `editor::CONTEXT`.
pub const START_CONTEXT: &str = "StartView";
/// The contexts of the start window: the editor's and `START_CONTEXT` (a test checks that they are those).
pub(crate) const START_WINDOW_CONTEXTS: &str = "SlideEditor StartView";

thread_local! {
    /// A scenario on a Mac asks for the bar the other systems draw (`force_bar`).
    static FORCED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the start window draws its menus in a menu bar of its own: Windows and Linux (macOS has the system's; the browser has no start menus).
pub(crate) fn shows_bar() -> bool {
    FORCED.with(std::cell::Cell::get) || cfg!(not(any(target_os = "macos", target_family = "wasm")))
}

/// For the scenarios: draw the bar on this system too.
#[allow(dead_code)]
pub(crate) fn force_bar(on: bool) {
    FORCED.with(|f| f.set(on));
}

/// What the start window's keys and menu lines of ファイル > 新しい… make: the kind, the id of the line (its words) and its key.
pub(crate) fn new_lines() -> [(Kind, &'static str, &'static str); 3] {
    [(Kind::Document, "file.new-document", "⌘D"), (Kind::Workbook, "file.new-workbook", "⌘E"), (Kind::Presentation, "file.new-presentation", "⌘P")]
}

impl SlideEditor {
    /// The menus of the start window's menu bar, in their order. (Windows and Linux; macOS's are the system menu bar's.)
    pub fn start_menus(&self) -> Vec<(MenuId, Vec<MenuEntry>)> {
        let mut file: Vec<MenuEntry> = new_lines()
            .into_iter()
            .map(|(kind, id, key)| item(id, crate::format::info(kind).icon).shortcut(key).on(move |this, window, cx| this.new_file(kind, window, cx)).into())
            .collect();
        file.push(item("file.open", "folder_open").shortcut("⌘O").on(|this, window, cx| this.open_file(&crate::actions::Open, window, cx)).into());
        file.push(crate::chrome_defaults::sub("file.recent", "schedule", recent_entries(self)));
        file.push(SEP);
        // iOS and Android (K0076): closing the window ends the app, so neither this line nor 終了 is shown.
        if !crate::mobile_os::is_mobile_os() {
            file.push(item("file.close-window", "close").shortcut("⌘W").on(|this, window, cx| this.close_file(&crate::actions::CloseFile, window, cx)).into());
        }
        #[cfg(not(target_family = "wasm"))]
        if !crate::mobile_os::is_mobile_os() {
            file.push(SEP);
            file.push(item("file.quit", "logout").shortcut("⌘Q").on(|_, _, cx| crate::app::quit_asking_to_save(cx)).into());
        }

        let mut window: Vec<MenuEntry> = vec![item("window.minimize", "remove").on(|_, window, _| window.minimize_window()).into()];
        window.push(SEP);
        window.extend(self.open_window_menu_items());

        let mut help: Vec<MenuEntry> = Vec::new();
        #[cfg(not(target_family = "wasm"))]
        {
            // No update check on iOS and Android (K0087): the stores update the app.
            if !crate::mobile_os::is_mobile_os() {
                help.push(item("help.check-update", "download").on(|_, _, cx| crate::update::check_now(cx)).into());
            }
            help.push(item("help.source-code", "link").on(|_, _, cx| crate::external::open_url(&crate::host::source_url(), cx)).into());
        }
        // The About screen is the app's (the start window has the window it opens in).
        help.push(item("help.about", "info").on(|this, _, cx| this.show_about(cx)).into());

        // iOS and Android (K0087): one window is the app's for now, so there is no ウィンドウ menu to put away or list in.
        if crate::mobile_os::is_mobile_os() {
            return vec![(MenuId::File, file), (MenuId::Help, help)];
        }
        vec![(MenuId::File, file), (MenuId::Window, window), (MenuId::Help, help)]
    }
}
