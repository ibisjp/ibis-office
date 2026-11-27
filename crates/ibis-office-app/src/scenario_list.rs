// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Every scenario (`tests/scenarios.rs` runs them through the shell's driver, `shell/scenario.rs`): the shell's own, the ones
//! here (they open the formats' files or call the formats), and each format's (`FormatEditor::scenarios`). The scenarios'
//! modules below name the driver as their parent (`super::Scenario`, `super::button_probe`, `super::repo_root`).

#![allow(dead_code)] // The driver's API, for the scenarios.

pub(crate) use crate::shell::scenario::*;

#[path = "tests/shell/dialog_apply.rs"]
mod dialog_apply;
#[path = "tests/shell/host.rs"]
mod host_scenarios;
#[path = "tests/shell/font_setup.rs"]
mod font_setup_scenarios;
#[path = "tests/shell/shared_equation_editor.rs"]
mod shared_equation_editor;
#[path = "tests/shell/hyperlinks.rs"]
mod hyperlink_scenarios;
#[path = "tests/shell/sidebars.rs"]
mod sidebars_scenarios;
#[path = "tests/shell/dialog.rs"]
mod dialog_scenarios;
#[path = "tests/shell/merge_workflow.rs"]
mod merge_workflow_scenarios;
#[path = "tests/shell/ui_refinements.rs"]
mod ui_refinements;
#[path = "tests/shell/layout_refinements.rs"]
mod layout_refinements;
#[path = "tests/shell/xlsx_ink_followup.rs"]
mod xlsx_ink_followup;
#[path = "tests/shell/message_box_primary.rs"]
mod message_box_primary;
#[path = "tests/shell/ui_followup.rs"]
mod ui_followup;
#[path = "tests/shell/issues_33_plus.rs"]
mod issues_33_plus;
#[path = "tests/shell/issues59_62.rs"]
mod issues59_62;
#[path = "tests/shell/issue138_ime_target.rs"]
mod issue138_ime_target;
#[path = "tests/shell/issue202_ime_cell_move.rs"]
mod issue202_ime_cell_move;
#[path = "tests/shell/issue121_ime.rs"]
mod issue121_ime;
#[path = "tests/shell/k0054_open_replace.rs"]
mod k0054_open_replace;
#[path = "tests/shell/issue182_reconvert.rs"]
mod issue182_reconvert;
#[path = "tests/shell/issue201_close_inactive.rs"]
mod issue201_close_inactive;
#[path = "tests/shell/plane16_font.rs"]
mod plane16_font;
#[path = "tests/shell/issue143_140.rs"]
mod issue143_140;
#[path = "tests/shell/access_key.rs"]
mod access_key_scenarios;
#[path = "tests/shell/issue235_pdf.rs"]
mod issue235_pdf;
#[path = "tests/shell/issue235_viewer.rs"]
mod issue235_viewer;
#[path = "tests/shell/issue245_caret.rs"]
mod issue245_caret;
#[path = "tests/shell/menu_layout.rs"]
mod menu_layout_scenarios;
#[path = "tests/shell/menus_20261109.rs"]
mod menus_20261109_scenarios;
#[path = "tests/shell/menus_20261112.rs"]
mod menus_20261112_scenarios;
#[path = "tests/shell/menus_20261114.rs"]
mod menus_20261114_scenarios;
#[path = "tests/shell/k0061_narrow.rs"]
mod k0061_narrow_scenarios;
#[path = "tests/shell/k0067_back_to_start.rs"]
mod k0067_back_to_start_scenarios;
#[path = "tests/shell/k0064_keyboard_toolbar.rs"]
mod k0064_keyboard_toolbar_scenarios;
#[path = "tests/shell/touch_gesture.rs"]
mod touch_gesture_scenarios;
#[path = "tests/shell/k0034_rest.rs"]
mod k0034_rest_scenarios;
#[path = "tests/shell/toolbar_scenes.rs"]
mod toolbar_scenes_scenarios;
#[path = "tests/shell/k0039_workbook.rs"]
mod k0039_workbook_scenarios;
#[path = "tests/shell/menu_every_item.rs"]
mod menu_every_item_scenarios;
#[path = "tests/shell/menu_contexts.rs"]
mod menu_contexts_scenarios;
#[path = "tests/shell/access_key_menus.rs"]
mod access_key_menus_scenarios;
#[path = "tests/shell/bar_keys.rs"]
mod bar_keys_scenarios;
#[path = "tests/shell/ios_edit_marks.rs"]
mod ios_edit_marks;
#[path = "tests/shell/k0066_emoji.rs"]
mod k0066_emoji;
#[path = "tests/shell/issue229_law_save.rs"]
mod issue229_law_save;
#[path = "tests/shell/k0065_touch_scroll.rs"]
mod k0065_touch_scroll;
#[path = "tests/shell/k0070_access_keys.rs"]
mod k0070_access_keys;
#[path = "tests/shell/issue234_backspace_pages.rs"]
mod issue234_backspace_pages;
#[path = "tests/shell/issue245_caret_contrast.rs"]
mod issue245_caret_contrast;
#[path = "tests/shell/issue295_word_count.rs"]
mod issue295_word_count;
#[path = "tests/shell/k0074_overlap_submenu.rs"]
mod k0074_overlap_submenu;
#[path = "tests/shell/k0073_long_press.rs"]
mod k0073_long_press;
#[path = "tests/shell/k0089_formula_bar_bottom.rs"]
mod k0089_formula_bar_bottom;
#[path = "tests/shell/k0081_narrow_title_icon.rs"]
mod k0081_narrow_title_icon;
#[path = "tests/shell/k0080_narrow_statusbar.rs"]
mod k0080_narrow_statusbar;
#[path = "tests/shell/k0087_mobile_menus.rs"]
mod k0087_mobile_menus;
#[path = "tests/shell/k0092_kbd_hide_commits_ime.rs"]
mod kbd_hide_commits_ime;
#[path = "tests/shell/k_kbd_close.rs"]
mod k_kbd_close;
#[path = "tests/shell/k_keyboard_scroll.rs"]
mod k_keyboard_scroll;
#[path = "tests/shell/k_android_enter.rs"]
mod k_android_enter;
#[path = "tests/shell/k_formula_bar_kbd.rs"]
mod k_formula_bar_kbd;
#[path = "tests/shell/k0088_title_name_tap.rs"]
mod k0088_title_name_tap;
#[path = "tests/shell/k0076_mobile_menu.rs"]
mod k0076_mobile_menu;
#[path = "tests/shell/k0078_menu_scroll_finger.rs"]
mod k0078_menu_scroll_finger;
#[path = "tests/shell/k0084_splitter_touch.rs"]
mod k0084_splitter_touch;
#[path = "tests/shell/menu_dump.rs"]
mod menu_dump_scenarios;
#[cfg(not(target_family = "wasm"))]
#[path = "tests/shell/window_menu.rs"]
mod window_menu_scenarios;
#[path = "tests/shell/ui_20261028.rs"]
mod ui_20261028;
#[path = "tests/shell/ui_20261029_align.rs"]
mod ui_20261029_align;
#[path = "tests/shell/editing_20261028.rs"]
mod editing_20261028;
#[path = "tests/shell/k0027_k0029_shape_pane.rs"]
mod k0027_k0029_shape_pane;
#[path = "tests/shell/pii_check.rs"]
mod pii_check;
#[path = "tests/shell/dicom_dialog.rs"]
mod dicom_dialog;
#[path = "tests/shell/followup_20261028.rs"]
mod followup_20261028;
#[path = "tests/shell/issues64_65.rs"]
mod issues64_65;
#[path = "tests/shell/issues83.rs"]
mod issues83;
#[path = "tests/shell/settings_dropdown_mouse.rs"]
mod settings_dropdown_mouse;
#[path = "tests/shell/issue108_paste.rs"]
mod issue108_paste;
#[path = "tests/shell/issue105_editor.rs"]
mod issue105_editor;
#[path = "tests/shell/find_shortcuts.rs"]
mod find_shortcuts;
#[path = "tests/shell/native_jtd.rs"]
mod native_jtd_scenarios;
#[path = "tests/shell/file_watch.rs"]
mod file_watch_scenarios;
#[path = "tests/shell/title_file_menu.rs"]
mod title_file_menu;
#[path = "tests/shell/dropdown.rs"]
mod dropdown;
#[path = "tests/shell/panel_catalog.rs"]
mod panel_catalog;
#[path = "tests/shell/common_parts.rs"]
mod common_parts;
#[path = "tests/shell/comments_common.rs"]
mod comments_common;
#[path = "tests/shell/scenario_i18n.rs"]
mod scenario_i18n;
#[path = "tests/shell/scenario_fix.rs"]
pub(crate) mod scenario_fix;
#[path = "tests/shell/scenario_open_save.rs"]
mod scenario_open_save;
#[path = "tests/shell/issue232_signature.rs"]
mod issue232_signature;
#[path = "tests/shell/xlsx_no_styles_save.rs"]
mod xlsx_no_styles_save;
#[path = "tests/shell/scenario_menu_search.rs"]
mod scenario_menu_search;

/// Every scenario, as (name, function).
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    let mut all = crate::shell::scenario_examples::all();
    all.extend(crate::native_quit_scenarios::all());
    all.extend(native_jtd_scenarios::all());
    all.extend(crate::offscreen_3d_scenarios::all());
    all.extend(dialog_scenarios::all());
    all.push(("shell::submenu_viewport_fit",crate::shell::scenario::menu_viewport::run));
    all.extend(hyperlink_scenarios::all());
    all.extend(dialog_apply::all());
    all.extend(font_setup_scenarios::all());
    all.extend(ui_refinements::all());
    all.extend(issue229_law_save::all());
    all.extend(layout_refinements::all());
    all.extend(ui_followup::all());
    all.extend(issues_33_plus::all());
    all.extend(issues59_62::all());
    all.extend(issue138_ime_target::all());
    all.extend(issue202_ime_cell_move::all());
    all.extend(issue121_ime::all());
    all.extend(k0054_open_replace::all());
    all.extend(issue182_reconvert::all());
    all.extend(issue201_close_inactive::all());
    all.extend(host_scenarios::all());
    all.extend(plane16_font::all());
    all.extend(issue143_140::all());
    all.extend(access_key_scenarios::all());
    all.extend(issue235_pdf::all());
    all.extend(issue235_viewer::all());
    all.extend(issue245_caret::all());
    all.extend(k0074_overlap_submenu::all());
    all.extend(issue245_caret_contrast::all());
    all.extend(ui_20261028::all());
    all.extend(ui_20261029_align::all());
    all.extend(editing_20261028::all());
    all.extend(k0027_k0029_shape_pane::all());
    all.extend(pii_check::all());
    all.extend(dicom_dialog::all());
    all.extend(followup_20261028::all());
    all.extend(issues64_65::all());
    all.extend(issues83::all());
    all.extend(settings_dropdown_mouse::all());
    all.extend(issue108_paste::all());
    all.extend(issue105_editor::all());
    all.extend(find_shortcuts::all());
    all.extend(file_watch_scenarios::all());
    all.extend(title_file_menu::all());
    all.extend(comments_common::all());
    all.push(("shell::dropdown_item_attributes",dropdown::run));
    all.push(("shell::toolbar_style_common_dropdown",dropdown::toolbar_style));
    all.extend(xlsx_ink_followup::all());
    all.extend(message_box_primary::all());
    all.extend(crate::shell::scenario::popup_guard_scenarios::all());
    all.extend(crate::shell::scenario::settings_dialog_scenarios::all());
    all.extend(crate::shell::scenario::toolbar_overflow_scenarios::all());
    all.extend(crate::shell::scenario::start_menu_scenarios::all());
    all.extend(toolbar_scenes_scenarios::all());
    all.extend(k0039_workbook_scenarios::all());
    all.extend(crate::shell::scenario::segment_control_ui::all());
    all.extend(common_parts::all());
    all.extend(crate::shell::scenario::input_components_rendering::all());
    all.extend(merge_workflow_scenarios::all());
    all.extend(scenario_i18n::all());
    all.extend(scenario_fix::all());
    all.extend(scenario_open_save::all());
    all.extend(issue232_signature::all());
    all.extend(xlsx_no_styles_save::all());
    all.extend(crate::shell::scenario_menus::all());
    all.extend(scenario_menu_search::all());
    all.extend(menu_layout_scenarios::all());
    all.extend(menus_20261109_scenarios::all());
    all.extend(menus_20261112_scenarios::all());
    all.extend(menus_20261114_scenarios::all());
    all.extend(k0061_narrow_scenarios::all());
    all.extend(k0067_back_to_start_scenarios::all());
    all.extend(k0089_formula_bar_bottom::all());
    all.extend(k0081_narrow_title_icon::all());
    all.extend(k0080_narrow_statusbar::all());
    all.extend(k0087_mobile_menus::all());
    all.extend(kbd_hide_commits_ime::all());
    all.extend(k_kbd_close::all());
    all.extend(k_keyboard_scroll::all());
    all.extend(k_android_enter::all());
    all.extend(k_formula_bar_kbd::all());
    all.extend(k0088_title_name_tap::all());
    all.extend(k0076_mobile_menu::all());
    all.extend(k0078_menu_scroll_finger::all());
    all.extend(k0084_splitter_touch::all());
    all.extend(k0064_keyboard_toolbar_scenarios::all());
    all.extend(k0065_touch_scroll::all());
    all.extend(ios_edit_marks::all());
    all.extend(k0066_emoji::all());
    all.extend(touch_gesture_scenarios::all());
    all.extend(k0034_rest_scenarios::all());
    all.extend(menu_every_item_scenarios::all());
    all.extend(menu_contexts_scenarios::all());
    all.extend(sidebars_scenarios::all());
    all.extend(issue295_word_count::all());
    all.extend(issue234_backspace_pages::all());
    all.extend(crate::shell::scenario::title_buttons_scenarios::all());
    all.extend(bar_keys_scenarios::all());
    all.extend(access_key_menus_scenarios::all());
    all.extend(k0070_access_keys::all());
    all.extend(menu_dump_scenarios::all());
    #[cfg(not(target_family = "wasm"))]
    all.extend(window_menu_scenarios::all());
    all.extend(crate::pptx_scenarios::all());
    for (_, driver) in crate::shell::format::drivers() {
        all.extend(driver.scenarios());
    }
    // The document's layout checks that export a page stay in the app (tasks/doc/crate-split-plan.md, 15.2).
    all.extend(shared_equation_editor::all());
    #[cfg(not(windows))]
    all.extend(crate::docx_layout_tests::all());
    #[cfg(not(windows))]
    all.extend(crate::docx_layout_tests_grid::all());
    #[cfg(target_os = "macos")]
    all.extend(crate::xlsx_corpus_report::all());
    #[cfg(target_os = "macos")]
    all.extend(crate::xlsx_issue189::all());
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    {
        all.extend(crate::export_print::export_print_tests::all());
    }
    all
}

/// Unfixed bug scenarios (corpus_report), skipped in normal `cargo test` and
/// run with `--ignored` or `--include-ignored`.
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    let mut out: Vec<(&'static str, fn())> = vec![("shell::dump_all_right_panels", panel_catalog::dump_all_right_panels)];
    out.extend(ui_followup::ignored());
    out.extend(issue234_backspace_pages::ignored());
    out.extend(menu_every_item_scenarios::ignored());
    out.extend(issue295_word_count::ignored());
    out.extend(k0073_long_press::ignored());
    out.extend(k0089_formula_bar_bottom::ignored());
    out.extend(k0081_narrow_title_icon::ignored());
    out.extend(k0080_narrow_statusbar::ignored());
    out.extend(k0087_mobile_menus::ignored());
    out.extend(kbd_hide_commits_ime::ignored());
    out.extend(k_kbd_close::ignored());
    out.extend(k_keyboard_scroll::ignored());
    out.extend(k_android_enter::ignored());
    out.extend(k_formula_bar_kbd::ignored());
    out.extend(k0088_title_name_tap::ignored());
    out.extend(k0076_mobile_menu::ignored());
    out.extend(k0078_menu_scroll_finger::ignored());
    out.extend(k0084_splitter_touch::ignored());
    out.extend(crate::pptx_scenarios::ignored());
    for (_, driver) in crate::shell::format::drivers() {
        out.extend(driver.ignored_scenarios());
    }
    #[cfg(not(windows))]
    out.extend(crate::docx_layout_tests::ignored());
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    out.extend(crate::export_print::export_print_tests::ignored());
    #[cfg(not(windows))]
    out.extend(crate::docx_layout_tests_grid::ignored());
    #[cfg(target_os = "macos")]
    out.extend(crate::xlsx_corpus_report::ignored());
    out
}

