// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The editor window and what every format shares: the window's state and
//! layout (editor.rs), the chrome as the UI mock has it (chrome.rs: the
//! title row, menu bar, toolbar and sidebar, with what they hold registered
//! by the formats; chrome_defaults.rs), the status bar (status_bar.rs),
//! the bundled icons (icons.rs), the start view
//! and recent files, the workspace that shows a page of shapes
//! (workspace.rs), saving (save.rs), and the hand-over to each format's
//! editor (format.rs).

pub(crate) mod app_data;
pub(crate) mod caret;
pub(crate) mod document_memory;
pub(crate) mod paste_placement;
pub(crate) mod selection_handles;
pub(crate) mod autocorrect;
pub(crate) mod autorecover;
pub(crate) mod editor;
pub mod embed;
pub(crate) mod file_actions;
pub(crate) mod chrome;
pub(crate) mod links;
pub(crate) mod panel_dialog;
pub(crate) mod color_palette;
pub(crate) mod color_editor;
pub(crate) mod color_picker_dialog;
pub(crate) mod theme_colors;
pub(crate) mod math_editor;
pub(crate) mod ink_panel;
pub(crate) mod chrome_defaults;
pub(crate) mod format;
pub(crate) mod font_list;
pub(crate) mod font_preview;
pub(crate) mod i18n;
pub(crate) mod settings;
pub(crate) mod settings_window;
pub(crate) mod qat;
#[cfg(not(target_family = "wasm"))]
pub(crate) mod update;
pub(crate) mod about_window;
#[cfg(not(target_family = "wasm"))]
pub(crate) mod password_window;
pub(crate) mod icons;
pub(crate) mod palette;
pub(crate) mod menu_bar;
pub(crate) mod input;
pub(crate) mod plain_path;
pub(crate) mod recent_files;
pub(crate) mod status_bar;
pub(crate) mod spell_panel;
pub(crate) mod symbols;
pub(crate) mod user_name;
pub(crate) mod save;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
pub(crate) mod scenario;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "../tests/shell/scenario_examples.rs"]
pub(crate) mod scenario_examples;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "../tests/shell/scenario_i18n.rs"]
pub(crate) mod scenario_i18n;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "../tests/shell/scenario_menus.rs"]
pub(crate) mod scenario_menus;
pub(crate) mod menu_bridge;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "../tests/shell/scenario_fix.rs"]
pub(crate) mod scenario_fix;
pub(crate) mod start_view;
pub(crate) mod workspace;

/// The old name of the status bar's module, which the formats' code may
/// still use (`ribbon::thousands`).
pub(crate) mod ribbon {
    pub(crate) use super::status_bar::thousands;
}
