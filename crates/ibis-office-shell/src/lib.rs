// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
#![recursion_limit = "256"]
#![cfg_attr(not(feature = "scenarios"), allow(dead_code))]
//! The editor window and what every format shares: the window's state and
//! layout (editor.rs), the chrome as the UI mock has it (chrome.rs: the
//! title row, menu bar, toolbar and sidebar, with what they hold registered
//! by the formats; chrome_defaults.rs), the status bar (status_bar.rs),
//! the bundled icons (icons.rs), the start view
//! and recent files, the workspace that shows a page of shapes
//! (workspace.rs), saving (save.rs), and the hand-over to each format's
//! editor (format.rs).

// The crates below the shell, under the names the shell's code uses (as in ibis-office-app).
pub(crate) use ibis_office_base::clock;
pub(crate) use ibis_office_render as render;

/// The version the app shows (version.txt, given by build.rs for the platform built for).
pub const VERSION: &str = env!("IBIS_VERSION");

pub use ibis_office_base::app_data;
pub mod actions;
pub mod external;
pub mod app_hooks;
#[cfg(headless_export)]
pub mod headless;
#[cfg(target_family = "wasm")]
pub mod web;
pub mod app;
#[cfg(not(target_family = "wasm"))]
pub mod read_file;
#[cfg(not(target_family = "wasm"))]
pub mod timing;
pub mod linux_desktop;
pub mod dib_picture;
pub mod native_shape_clipboard;
pub mod spelling;
pub mod grammar;
#[cfg(all(headless_export, target_os = "macos"))]
pub(crate) mod svg_pdf;
#[cfg(headless_export)]
pub(crate) mod svg_emf;
#[cfg(headless_export)]
pub mod pdf_picture;
pub mod caret;
pub mod document_memory;
pub mod selection_handles;
pub mod autocorrect;
pub mod autorecover;
pub mod editor;
pub mod embed;
pub mod file_actions;
pub mod file_watch;
pub(crate) mod view_state;
pub(crate) mod message_box;
pub(crate) mod extension_consent;
pub mod cmyk;
pub mod cmyk_profile;
pub mod access_key;
pub mod comments;
#[cfg(not(target_family = "wasm"))]
pub mod open_guard;
pub mod chrome;
pub mod links;
pub mod dialog;
pub mod color_palette;
pub mod color_editor;
pub(crate) mod color_picker_dialog;
pub(crate) mod theme_colors;
pub mod math_editor;
pub mod ink_panel;
pub mod ink;
pub mod chart_panel;
pub mod chart_data;
pub mod compress;
pub mod theme_panel;
pub mod shape_editing;
pub mod shape_items;
pub mod text_edit;
pub mod text_keys;
pub mod shape_state;
pub mod smartart;
pub mod interaction;
pub mod text_chains;
pub(crate) mod crop_panel;
pub(crate) mod frame_panel;
pub mod find;
pub mod spelling_panel;
pub mod header_footer;
pub(crate) mod selection_pane;
pub(crate) mod thesaurus_panel;
pub mod size_panel;
pub(crate) mod insets_panel;
pub mod effect_editor;
pub mod shape_format_pane;
pub(crate) mod glow_panel;
pub(crate) mod shadow_panel;
pub mod gradient_panel;
pub mod paragraph_panel;
pub mod paste_placement;
pub(crate) mod shape_clipboard;
pub(crate) mod alt_text_panel;
pub mod pointer_mode;
pub mod backup_save;
pub mod backup_copy;
pub mod xls_note;
pub mod legacy_password;
pub mod sheet_metrics;
pub mod edit_mode;
pub mod arrange_kind;
pub(crate) mod arrange_windows;
pub mod split_axis;
pub mod chart_insert;
pub mod thesaurus;
pub mod word_count;
pub mod chrome_defaults;
pub mod infobar;
pub mod format;
pub mod font_list;
pub mod font_setup;
pub mod font_setup_dialog;
pub mod replace_fonts;
#[cfg(not(target_family = "wasm"))]
pub mod rename_file;
pub mod crop_units;
pub mod crop_mode;
pub(crate) mod font_preview;
pub use ibis_office_base::i18n;
pub mod settings;
pub mod settings_dialog;
pub mod qat;
#[cfg(not(target_family = "wasm"))]
pub mod update;
pub mod about;
pub(crate) mod modify_password;
#[cfg(not(target_family = "wasm"))]
pub(crate) mod password_window;
pub mod icons;
pub mod palette;
pub mod menu_bar;
pub mod menu_context;
#[cfg(not(target_family = "wasm"))]
pub mod open_documents;
#[cfg(not(target_family = "wasm"))]
pub(crate) mod window_switcher;
pub mod commands;
pub mod menu_layout;
mod menu_layout_data;
pub mod menu_search_history;
pub mod menu_tiles;
pub(crate) mod menu_scrollbar;
pub(crate) mod input;
pub mod plain_path;
pub mod recent_files;
pub(crate) mod status_bar;
pub mod spell_panel;
pub mod symbols;
pub mod user_name;
pub mod save;
pub mod save_location;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
pub mod scenario;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/scenario_examples.rs"]
pub mod scenario_examples;
#[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
#[path = "tests/scenario_menus.rs"]
pub mod scenario_menus;
pub mod sidebars;
pub mod draw_menu;
pub(crate) mod bar_keys;
pub mod start_view;
pub(crate) mod workspace;

/// The old name of the status bar's module, which the formats' code may
/// still use (`ribbon::thousands`).
pub mod ribbon {
    pub use super::status_bar::thousands;
}
