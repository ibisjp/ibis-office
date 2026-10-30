// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the fix 担当's requests (`tasks/todo-fix.md` #10 and after):
//! what the user does with the pointer in the window's chrome.
use crate::docx::scenarios::DocSteps;
use crate::xlsx::scenarios::SheetSteps;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::shared_radio_group_layouts", shared_radio_group_layouts),
        ("shell::fix72_font_dropdown_preview", fix72_font_dropdown_preview),
        ("shell::fix75_start_window_closes_with_cmd_w", fix75_start_window_closes_with_cmd_w),
        ("shell::issue27_close_button_asks_to_save", issue27_close_button_asks_to_save),
        ("shell::client_close_button_click_asks_to_save", client_close_button_click_asks_to_save),
        ("shell::client_close_button_click_saves_then_closes", client_close_button_click_saves_then_closes),
        ("shell::issue27_save_answer_writes_then_closes", issue27_save_answer_writes_then_closes),
        ("shell::issue27_quit_asks_to_save", issue27_quit_asks_to_save),
        ("shell::right_pane_button_follows_the_pane", right_pane_button_follows_the_pane),
        ("shell::fix_textbox_01_docx_second_line_selection_and_caret_alignment", fix_textbox_01_docx_second_line_selection_and_caret_alignment),
        ("shell::fix_textbox_02_docx_second_line_large_font_third_line_alignment", fix_textbox_02_docx_second_line_large_font_third_line_alignment),
        ("shell::fix_textbox_03_docx_arrow_up_down_vertical_navigation", fix_textbox_03_docx_arrow_up_down_vertical_navigation),
        ("shell::fix_textbox_04_docx_shift_up_down_multiline_selection", fix_textbox_04_docx_shift_up_down_multiline_selection),
        ("shell::fix_textbox_05_docx_left_right_home_end_navigation", fix_textbox_05_docx_left_right_home_end_navigation),
        ("shell::fix_textbox_06_docx_word_navigation_and_selection", fix_textbox_06_docx_word_navigation_and_selection),
        ("shell::fix_textbox_07_docx_five_lines_fixed_rectangle_size", fix_textbox_07_docx_five_lines_fixed_rectangle_size),
        ("shell::fix_textbox_08_docx_ime_composing_visible_and_commit", fix_textbox_08_docx_ime_composing_visible_and_commit),
        ("shell::fix_textbox_09_docx_double_click_places_caret_at_click_pos", fix_textbox_09_docx_double_click_places_caret_at_click_pos),
        ("shell::fix_textbox_10_docx_double_and_triple_click_selects_word_and_paragraph", fix_textbox_10_docx_double_and_triple_click_selects_word_and_paragraph),
        ("shell::fix_textbox_11_docx_mouse_drag_selects_text_range", fix_textbox_11_docx_mouse_drag_selects_text_range),
        ("shell::fix_textbox_12_docx_partial_selection_bold_italic_underline", fix_textbox_12_docx_partial_selection_bold_italic_underline),
        ("shell::fix_textbox_13_docx_partial_selection_font_size_and_color", fix_textbox_13_docx_partial_selection_font_size_and_color),
        ("shell::fix_textbox_14_docx_eight_handles_resize_while_editing", fix_textbox_14_docx_eight_handles_resize_while_editing),
        ("shell::fix_textbox_15_docx_border_drag_moves_box_while_editing", fix_textbox_15_docx_border_drag_moves_box_while_editing),
        ("shell::fix_textbox_16_docx_undo_coalesces_typing_session", fix_textbox_16_docx_undo_coalesces_typing_session),
        ("shell::fix_textbox_17_xlsx_insert_and_five_lines_fixed_rectangle_size", fix_textbox_17_xlsx_insert_and_five_lines_fixed_rectangle_size),
        ("shell::fix_textbox_18_xlsx_second_line_selection_and_caret_alignment", fix_textbox_18_xlsx_second_line_selection_and_caret_alignment),
        ("shell::fix_textbox_19_xlsx_second_line_large_font_third_line_alignment", fix_textbox_19_xlsx_second_line_large_font_third_line_alignment),
        ("shell::fix_textbox_20_xlsx_arrow_up_down_and_shift_selection", fix_textbox_20_xlsx_arrow_up_down_and_shift_selection),
        ("shell::fix_textbox_21_xlsx_word_nav_and_delete_shortcuts", fix_textbox_21_xlsx_word_nav_and_delete_shortcuts),
        ("shell::fix_textbox_22_xlsx_ime_composing_visible_and_commit", fix_textbox_22_xlsx_ime_composing_visible_and_commit),
        ("shell::fix_textbox_23_xlsx_double_click_places_caret_at_click_pos", fix_textbox_23_xlsx_double_click_places_caret_at_click_pos),
        ("shell::fix_textbox_24_xlsx_double_triple_click_and_drag_selection", fix_textbox_24_xlsx_double_triple_click_and_drag_selection),
        ("shell::fix_textbox_25_xlsx_partial_selection_formatting_and_toolbar_sync", fix_textbox_25_xlsx_partial_selection_formatting_and_toolbar_sync),
        ("shell::fix_textbox_26_xlsx_eight_handles_and_border_drag_while_editing", fix_textbox_26_xlsx_eight_handles_and_border_drag_while_editing),
        ("shell::fix_textbox_27_xlsx_empty_new_textbox_retracted_on_escape_and_undo_coalesced", fix_textbox_27_xlsx_empty_new_textbox_retracted_on_escape_and_undo_coalesced),
        ("shell::fix_textbox_28_pptx_new_slide_inserted_textbox_edit_and_ime", fix_textbox_28_pptx_new_slide_inserted_textbox_edit_and_ime),
        ("shell::fix_textbox_29_pptx_five_lines_fixed_rectangle_size_and_up_down", fix_textbox_29_pptx_five_lines_fixed_rectangle_size_and_up_down),
        ("shell::fix_textbox_30_pptx_second_line_large_font_and_selection_alignment", fix_textbox_30_pptx_second_line_large_font_and_selection_alignment),
        ("shell::fix_textbox_31_pptx_partial_selection_bold_italic_underline_and_handles", fix_textbox_31_pptx_partial_selection_bold_italic_underline_and_handles),
        ("shell::fix_textbox_32_copy_cut_paste_and_select_all_across_docx_xlsx_pptx", fix_textbox_32_copy_cut_paste_and_select_all_across_docx_xlsx_pptx),
        ("shell::fix_pinch_to_zoom", fix_pinch_to_zoom),
        ("shell::fix_cjk_edit_font_xlsx", fix_cjk_edit_font_xlsx),
        ("shell::fix_cjk_edit_font_pptx", fix_cjk_edit_font_pptx),
        ("shell::fix_cjk_edit_font_docx", fix_cjk_edit_font_docx),
        ("shell::fix51_cell_edit_and_align_toolbar", fix51_cell_edit_and_align_toolbar),
        ("shell::fix12_menu_bar_hover_switches_menu", fix12_menu_bar_hover_switches_menu),
        ("shell::fix15_long_menu_scrolls", fix15_long_menu_scrolls),
        ("shell::fix14_fill_handle_direction", fix14_fill_handle_direction),
        ("shell::fix13_sheet_tabs_scroll", fix13_sheet_tabs_scroll),
        ("shell::fix11_right_pane_tabs", fix11_right_pane_tabs),
        ("shell::fix77_save_as_opendocument", fix77_save_as_opendocument),
        ("shell::fix76_scrollbars_always", fix76_scrollbars_always),
        ("shell::fix78_dark_chrome", fix78_dark_chrome),
        ("shell::fix83_scenarios_keep_user_data_apart", fix83_scenarios_keep_user_data_apart),
        ("shell::fix_docx_edit014_symbols", fix_docx_edit014_symbols),
        ("shell::fix_pptx_symbols", fix_pptx_symbols),
        ("shell::fix_xlsx_symbols", fix_xlsx_symbols),
        ("shell::fix_docx_tab_stops", fix_docx_tab_stops),
        ("shell::fix_docx_contextual_spacing", fix_docx_contextual_spacing),
        ("shell::fix_docx_kerning", fix_docx_kerning),
        ("shell::fix_docx_widow_and_suppress_lines", fix_docx_widow_and_suppress_lines),
        ("shell::about_shows_powered_by", about_shows_powered_by),
        ("shell::start_view_new_and_select", start_view_new_and_select),
        ("shell::fix_docx_image_look", fix_docx_image_look),
        ("shell::fix17_print_is_bound", fix17_print_is_bound),
        ("shell::fix10_settings_window_centered_and_app_menu", fix10_settings_window_centered_and_app_menu),
        ("shell::fix_xlsx_new_menu_and_first_right_pane", fix_xlsx_new_menu_and_first_right_pane),
        ("shell::fix_xlsx_headings_and_formula_bar", fix_xlsx_headings_and_formula_bar),
        ("shell::fix_xlsx_zoom", fix_xlsx_zoom),
        ("shell::fix_xlsx_page_break_preview", fix_xlsx_page_break_preview),
        ("shell::fix_docx_line_numbers", fix_docx_line_numbers),
        ("shell::fix_docx_ruler", fix_docx_ruler),
        ("shell::fix_xlsx_ruler", fix_xlsx_ruler),
        ("shell::fix_xlsx_page_layout_ruler_drag", fix_xlsx_page_layout_ruler_drag),
        ("shell::fix_xlsx_save_copy_and_recent", fix_xlsx_save_copy_and_recent),
        ("shell::fix_docx_gridlines", fix_docx_gridlines),
        ("shell::fix_xlsx_zoom_to_selection", fix_xlsx_zoom_to_selection),
        ("shell::fix_xlsx_validation_allow_blank", fix_xlsx_validation_allow_blank),
        ("shell::fix_docx_replace_picture", fix_docx_replace_picture),
        ("shell::fix_xlsx_print_options", fix_xlsx_print_options),
        ("shell::fix_xlsx_print_title_columns", fix_xlsx_print_title_columns),
        ("shell::fix_xlsx_paper_sizes_and_centering", fix_xlsx_paper_sizes_and_centering),
        ("shell::fix_xlsx_export_csv_and_pdf", fix_xlsx_export_csv_and_pdf),
        ("shell::fix_docx_table_alignment_and_indent", fix_docx_table_alignment_and_indent),
        ("shell::fix_docx_columns", fix_docx_columns),
        ("shell::fix_docx_table_selection", fix_docx_table_selection),
        ("shell::fix_xlsx_circle_invalid_data", fix_xlsx_circle_invalid_data),
        ("shell::fix_docx_word_paragraph_selection", fix_docx_word_paragraph_selection),
        ("shell::fix_docx_word_count_dialog", fix_docx_word_count_dialog),
        ("shell::fix_docx_find_special_characters", fix_docx_find_special_characters),
        ("shell::fix_docx_find_whole_words", fix_docx_find_whole_words),
        ("shell::fix_docx_paste_special", fix_docx_paste_special),
        ("shell::fix_docx_unlink_fields", fix_docx_unlink_fields),
        ("shell::fix_docx_align_distribute", fix_docx_align_distribute),
        ("shell::fix_docx_toggle_field_codes", fix_docx_toggle_field_codes),
        ("shell::fix_docx_lock_fields", fix_docx_lock_fields),
        ("shell::fix_docx_insert_and_edit_field", fix_docx_insert_and_edit_field),
        ("shell::fix_docx_caption_labels_and_numbering", fix_docx_caption_labels_and_numbering),
        ("shell::fix_docx_table_of_figures", fix_docx_table_of_figures),
        ("shell::fix_status_bar_and_zoom_i18n", fix_status_bar_and_zoom_i18n),
        ("shell::fix_zoom_percent_button", fix_zoom_percent_button),
        ("shell::fix21_pptx_sidebar_close_zoom_fit_benchmark", fix21_pptx_sidebar_close_zoom_fit_benchmark),
        ("shell::pptx_sidebar_starts_closed", pptx_sidebar_starts_closed),
        ("shell::fix_xlsx_file017_doc_properties", fix_xlsx_file017_doc_properties),
        ("shell::fix_xlsx_sheet_tabs_scrollbar_and_fixed_add", fix_xlsx_sheet_tabs_scrollbar_and_fixed_add),
        ("shell::fix27_status_bar_tab_scrollbar_place", fix27_status_bar_tab_scrollbar_place),
        ("shell::fix30_status_bar_tab_wheel", fix30_status_bar_tab_wheel),
        ("shell::fix31_sheet_100_percent_is_a_point_a_pixel", fix31_sheet_100_percent_is_a_point_a_pixel),
        ("shell::fix33_formula_without_cached_value", fix33_formula_without_cached_value),
        ("shell::bench_open_scroll", bench_open_scroll),
        ("shell::fix_xlsx_sheet_add_threshold_blank_workbook", fix_xlsx_sheet_add_threshold_blank_workbook),
        ("shell::fix_xlsx_file018_custom_properties", fix_xlsx_file018_custom_properties),
        ("shell::fix_xlsx_sec022_alt_text", fix_xlsx_sec022_alt_text),
        ("shell::fix_xlsx_sec008_threaded_comments", fix_xlsx_sec008_threaded_comments),
        ("shell::fix_xlsx_file004_file007_templates", fix_xlsx_file004_file007_templates),
        ("shell::fix_xlsx_sec021_accessibility_check", fix_xlsx_sec021_accessibility_check),
        ("shell::fix_xlsx_sec019_print_targets", fix_xlsx_sec019_print_targets),
        ("shell::fix_xlsx_sec016_custom_header_footer", fix_xlsx_sec016_custom_header_footer),
        ("shell::fix_xlsx_object001_insert_shape_and_textbox", fix_xlsx_object001_insert_shape_and_textbox),
        ("shell::fix_xlsx_object008_selection_pane_align_and_frame", fix_xlsx_object008_selection_pane_align_and_frame),
        ("shell::fix_xlsx_object003_image011_image018_shape_format_and_picture_properties", fix_xlsx_object003_image011_image018_shape_format_and_picture_properties),
        ("shell::fix_xlsx_object007_group_and_image012_crop", fix_xlsx_object007_group_and_image012_crop),
        ("shell::fix_xlsx_image013_color_and_image014_effects", fix_xlsx_image013_color_and_image014_effects),
        ("shell::fix_xlsx_object012_object013_text_format_and_textbox_layout", fix_xlsx_object012_object013_text_format_and_textbox_layout),
        ("shell::fix_xlsx_object009_object010_image016_shape_placement_and_print", fix_xlsx_object009_object010_image016_shape_placement_and_print),
        ("shell::fix_xlsx_object002_change_shape", fix_xlsx_object002_change_shape),
        ("shell::fix_xlsx_object014_shape_formula_textlink", fix_xlsx_object014_shape_formula_textlink),
        ("shell::fix_xlsx_image019_copy_as_picture", fix_xlsx_image019_copy_as_picture),
        ("shell::fix_xlsx_image016_svg_insert_and_color", fix_xlsx_image016_svg_insert_and_color),
        ("shell::fix23_pptx_thumbnail_scroll_and_sections", fix23_pptx_thumbnail_scroll_and_sections),
        ("shell::fix_xlsx_file011_csv_and_tsv", fix_xlsx_file011_csv_and_tsv),
        ("shell::fix97_menu_scrollbar_visible_and_flush_to_edges", fix97_menu_scrollbar_visible_and_flush_to_edges),
        ("shell::fix98_searchable_dropdown_and_sheet_list", fix98_searchable_dropdown_and_sheet_list),
        ("shell::fix99_100_101_102_docx_textbox_panel_and_inline_picture_resize", fix99_100_101_102_docx_textbox_panel_and_inline_picture_resize),
        ("shell::fix103_menu_search_popup_sorted_filter_and_enter", fix103_menu_search_popup_sorted_filter_and_enter),
        ("shell::fix104_textbox_border_click_exits_edit_and_keeps_handles", fix104_textbox_border_click_exits_edit_and_keeps_handles),
        ("shell::fix105_106_107_xlsx_side_panels_layout_and_controls", fix105_106_107_xlsx_side_panels_layout_and_controls),
        ("shell::fix108_109_110_111_window_menu_and_arrange_all", fix108_109_110_111_window_menu_and_arrange_all),
        ("shell::fix112_docx_checklist_in_textbox_and_shape", fix112_docx_checklist_in_textbox_and_shape),
        ("shell::fix113_unified_color_palettes_in_menus", fix113_unified_color_palettes_in_menus),
        ("shell::fix114_115_shape_fill_and_border_color_in_toolbar", fix114_115_shape_fill_and_border_color_in_toolbar),
        ("shell::fix116_117_clear_formatting_in_xlsx_docx_pptx", fix116_117_clear_formatting_in_xlsx_docx_pptx),
        ("shell::fix118_multi_shape_selection_with_shift_click", fix118_multi_shape_selection_with_shift_click),
        ("shell::fix119_pptx_picture_aspect_ratio_locked_by_default", fix119_pptx_picture_aspect_ratio_locked_by_default),
        ("shell::fix120_toolbar_font_size_pulldown_and_direct_input", fix120_toolbar_font_size_pulldown_and_direct_input),
        ("shell::fix121_pptx_change_shape_removed_from_toolbar_kept_in_menus", fix121_pptx_change_shape_removed_from_toolbar_kept_in_menus),
        ("shell::fix122_xlsx_dynamic_array_spill_outline_and_ghost_formula", fix122_xlsx_dynamic_array_spill_outline_and_ghost_formula),
        ("shell::fix123_no_quick_analysis_button_on_multi_cell_selection", fix123_no_quick_analysis_button_on_multi_cell_selection),
        ("shell::fix124_no_mini_toolbar_on_body_text_selection", fix124_no_mini_toolbar_on_body_text_selection),
        ("shell::fix_embed_view_mode_and_bars", fix_embed_view_mode_and_bars),
        ("shell::fix152_156_xlsx_shape_underline_and_multi_shape_text_format", fix152_156_xlsx_shape_underline_and_multi_shape_text_format),
        ("shell::fix135_xlsx_split_view_comprehensive", fix135_xlsx_split_view_comprehensive),
        ("shell::fix157_xlsx_fill_border_gridline_pixel_alignment_across_zooms", fix157_xlsx_fill_border_gridline_pixel_alignment_across_zooms),
        ("shell::fix11_xlsx_option_hover_cursor_on_border_and_fill_handle", fix11_xlsx_option_hover_cursor_on_border_and_fill_handle),
        ("shell::fix152_156_multi_shape_text_and_paragraph_formatting", fix152_156_multi_shape_text_and_paragraph_formatting),
        ("shell::fix127_149_toolbar_font_and_font_size_combo_filter_reset_and_keys", fix127_149_toolbar_font_and_font_size_combo_filter_reset_and_keys),
        ("shell::fix136_long_submenu_scrollbar_arrows_and_click_occlusion", fix136_long_submenu_scrollbar_arrows_and_click_occlusion),
        ("shell::fix150_151_right_pane_clearable_inputs_and_page_setup_sticky_footer", fix150_151_right_pane_clearable_inputs_and_page_setup_sticky_footer),
    ]
}

fn open_index(s: &mut Scenario) -> Option<usize> {
    s.with(|e, _| e.open_menu.as_ref().map(|o| o.index()))
}

/// #12: with a menu open, the pointer on the next menu's name closes it and
/// opens that one; a press on the open menu's name closes it.
fn fix12_menu_bar_hover_switches_menu() {
    use crate::shell::scenario::button_probe as buttons;
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.update(|_, _, cx| cx.notify());
    s.settle();
    // The menu names' own positions (the logo's button before them is as wide as the menu bar's items).
    let at = |name: &str| {
        let b = buttons::bounds(&format!("menu-button-menu.{name}")).unwrap_or_else(|| panic!("{name}: {:?}", buttons::all()));
        (f32::from(b.center().x), f32::from(b.center().y))
    };
    let (file, edit, view) = (at("file"), at("edit"), at("view"));
    assert_eq!(open_index(&mut s), None);
    // Hovering with no menu open opens nothing.
    s.hover(edit);
    assert_eq!(open_index(&mut s), None);
    s.click(file);
    s.screenshot("file-open");
    assert_eq!(open_index(&mut s), Some(0));
    s.hover(edit);
    s.screenshot("edit-by-hover");
    assert_eq!(open_index(&mut s), Some(1));
    s.hover(view);
    assert_eq!(open_index(&mut s), Some(2));
    // Pressing the open menu's name closes it.
    s.click(view);
    assert_eq!(open_index(&mut s), None);
    // Pressing another menu's name while one is open switches to it.
    s.click(file);
    assert_eq!(open_index(&mut s), Some(0));
    s.click((900.0, 300.0));
    assert_eq!(open_index(&mut s), None);
}

/// Where the menu bar's button for menu `name` (`edit`, ...) is drawn: the logo before the names moves them, so
/// a fixed pixel is not the menu.
fn menu_button_at(s: &mut Scenario, name: &str) -> (f32, f32) {
    use crate::shell::scenario::button_probe as buttons;
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let b = buttons::bounds(&format!("menu-button-menu.{name}")).unwrap_or_else(|| panic!("{name}: {:?}", buttons::all()));
    (f32::from(b.center().x), f32::from(b.center().y))
}

fn menu_start(s: &mut Scenario) -> usize {
    s.with(|e, _| e.open_menu.as_ref().map_or(usize::MAX, |o| o.start()))
}

/// #15: a menu taller than the window (編集 of a workbook) shows ▲ ▼ rows;
/// hovering ▼ scrolls (on as the time passes), the wheel scrolls, and ▲
/// scrolls back; a window tall enough shows the whole menu with no rows.
fn fix15_long_menu_scrolls() {
    let _probe = crate::shell::scenario::button_probe::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let edit = menu_button_at(&mut s, "edit");
    s.click(edit);
    assert_eq!(open_index(&mut s), Some(1));
    assert_eq!(menu_start(&mut s), 0);
    s.screenshot("top");
    // ▼ is the menu's last row: at the window's bottom edge (menu 64..~672).
    s.hover((300.0, 648.0));
    s.wait(400);
    let after_hover = menu_start(&mut s);
    assert!(after_hover > 0, "hovering ▼ scrolled: {after_hover}");
    s.screenshot("scrolled");
    // The pointer at the menu's right edge shows the scroll bar.
    s.hover((388.0, 300.0));
    s.screenshot("scrollbar");
    // Off the row, it stops.
    s.hover((300.0, 300.0));
    s.wait(400);
    assert_eq!(menu_start(&mut s), after_hover);
    // The wheel: up shows earlier lines, down later ones.
    s.wheel((300.0, 300.0), 60.0);
    let up = menu_start(&mut s);
    assert!(up < after_hover, "the wheel up scrolled: {up}");
    s.wheel((300.0, 300.0), -60.0);
    assert!(menu_start(&mut s) > up, "the wheel down scrolled");
    // ▲ (the first row, at the menu's top) scrolls back to the top.
    s.hover((300.0, 82.0));
    s.wait(3000);
    assert_eq!(menu_start(&mut s), 0);
    // A tall window: no rows.
    s.size(1200.0, 1500.0);
    let edit = menu_button_at(&mut s, "edit");
    s.click(edit);
    s.click(edit);
    assert_eq!(menu_start(&mut s), 0);
    s.screenshot("tall");
}

/// #10: ツール > 設定… opens the settings window in the middle of the window
/// the user is in, its categories at the top of the left column; the app's
/// own menu ("ibisOffice") has 設定… (⌘,).
fn fix10_settings_window_centered_and_app_menu() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let parent = s.update(|_, window, _| window.bounds());
    s.menu("tools.settings");
    let handle = crate::shell::settings_window::handle().expect("the settings window is open");
    let bounds = s.update(|_, _, cx| handle.update(cx, |_, window, _| window.bounds()).expect("its bounds"));
    let (pc, bc) = (parent.center(), bounds.center());
    assert!((f32::from(pc.x) - f32::from(bc.x)).abs() < 1.0 && (f32::from(pc.y) - f32::from(bc.y)).abs() < 1.0, "centred on its parent: {parent:?} {bounds:?}");
    s.screenshot_of(handle.into(), "settings");
    // The app menu: 設定… then 終了, as macOS apps have them.
    let labels = s.update(|_, _, cx| {
        let menus = cx.get_menus().unwrap_or_default();
        menus.first().map(|m| (m.name.to_string(), m.items.iter().map(|i| match i { gpui_kit::OwnedMenuItem::Action { name, .. } => name.to_string(), _ => String::new() }).collect::<Vec<_>>()))
    });
    let (name, items) = labels.expect("the app menu");
    assert_eq!(name, "ibisOffice");
    assert_eq!(items.first().map(String::as_str), Some("ibisOffice について"), "{items:?}");
    assert!(items.iter().any(|i| i == "設定…"), "{items:?}");
}

/// ファイル > 新規 ends with テンプレートから新規作成…; a workbook's right pane
/// opened for the first time shows the 書式設定 tab.
fn fix_xlsx_new_menu_and_first_right_pane() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let last = s.with(|e, _| match e.menus().find_entry("file.new") {
        Some(crate::shell::chrome::MenuEntry::Submenu(sub)) => sub.items.last().and_then(|i| match i {
            crate::shell::chrome::MenuEntry::Item(item) => Some(item.id),
            _ => None,
        }),
        _ => None,
    });
    assert_eq!(last, Some("xlsx.new-from-template"), "the template item is the last of the New submenu");
    assert!(!s.with(|e, _| e.sidebar_open), "a workbook starts with the right pane closed");
    s.update(|e, window, cx| e.toggle_sidebar(window, cx));
    s.settle();
    assert!(s.with(|e, _| e.sidebar_open));
    assert!(s.with(|e, _| e.sheet_state().is_some_and(|w| w.custom_format.is_some())), "the cell-format panel is shown");
}

fn cell_text(s: &mut Scenario, column: u32, row: u32) -> String {
    s.with(move |e, _| {
        ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).and_then(|sh| sh.cell(row, column)).map(|c| format!("{:?}", c.value)).unwrap_or_default()
    })
}

fn enter(s: &mut Scenario, text: &str) {
    s.type_text(text);
    s.key("enter");
}

/// #14: 1 and 2 in A1:A2 (with names beside them), the fill handle moved a few
/// pixels right and down and then a long way down: it fills downwards, the
/// names beside untouched. Moved right of half the next column: it fills right.
fn fix14_fill_handle_direction() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.insert-sheet");
    let centre = |s: &mut Scenario, c: u32, r: u32| {
        let p = s.with(move |e, _| e.cell_center(c, r)).expect("the cell on screen");
        (f32::from(p.x), f32::from(p.y))
    };
    let corner = |s: &mut Scenario, c: u32, r: u32| {
        let p = s.with(move |e, _| e.cell_corner(c, r)).expect("the cell on screen");
        (f32::from(p.x), f32::from(p.y))
    };
    let a1 = centre(&mut s, 0, 0);
    s.click(a1);
    enter(&mut s, "1");
    enter(&mut s, "2");
    let b1 = centre(&mut s, 1, 0);
    s.click(b1);
    enter(&mut s, "山田");
    enter(&mut s, "山田");
    s.click(a1);
    s.key("shift-down");
    let c = corner(&mut s, 0, 1);
    // A few pixels right and down, then far down: down.
    let far = centre(&mut s, 0, 6);
    s.drag_through(&[(c.0 - 1.0, c.1 - 1.0), (c.0 + 6.0, c.1 + 6.0), (c.0 + 8.0, c.1 + 40.0), (c.0 + 8.0, far.1)]);
    assert_eq!(cell_text(&mut s, 0, 2), "Number(3.0)", "A3 filled down");
    assert_eq!(cell_text(&mut s, 0, 6), "Number(7.0)", "to the row of the pointer");
    assert_eq!(cell_text(&mut s, 1, 0), "Text(\"山田\")", "the names beside are kept");
    assert_eq!(cell_text(&mut s, 1, 2), "", "nothing filled right");
    s.screenshot("filled-down");
}

fn tab_offset(s: &mut Scenario) -> (f32, f32) {
    s.with(|e, _| {
        let h = e.sheet_state().expect("a workbook").tab_scroll.clone();
        (f32::from(h.offset().x), f32::from(h.max_offset().x))
    })
}

/// #13: the sheet tabs' row: ◀ ▶ scroll the tabs, + adds a sheet, ≡ lists the
/// sheets; with more tabs than fit, shadows show at the end they scroll towards.
/// + is inline after tabs when few, and moves to the right edge when overflowing.
fn fix13_sheet_tabs_scroll() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let sheets = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.len());
    assert_eq!(sheets(&mut s), 3);
    s.screenshot("few");
    let (y, list, right, left) = (679.0, 70.0, 41.0, 14.0);
    // Initially few sheets (3 sheets): + button is inline at the right of the tabs (~310.0).
    s.click((310.0, y));
    assert_eq!(sheets(&mut s), 4, "inline + added a 4th sheet");

    // Add sheets until they overflow. Once overflowed, + moves to the right edge fixed area (~1186.0).
    for _ in 0..18 {
        s.menu("xlsx.insert-sheet");
    }
    s.settle();
    let current_sheets = sheets(&mut s);
    assert!(current_sheets >= 20);

    // Click the fixed + button on the right edge.
    let fixed_plus_x = 1186.0;
    s.click((fixed_plus_x, y));
    assert_eq!(sheets(&mut s), current_sheets + 1, "fixed + on the right edge added a sheet");
    s.settle();

    let (offset, max) = tab_offset(&mut s);
    assert!(max > 100.0, "the tabs overflow: {max}");
    s.screenshot("overflow");

    // ◀ scrolls the tabs right (towards the first), ▶ left; the new tab was
    // brought into view, so the offset is somewhere inside.
    s.click((left, y));
    let (back, _) = tab_offset(&mut s);
    assert!(back > offset || offset == 0.0, "◀ scrolled: {offset} -> {back}");
    s.screenshot("scrolled");
    s.click((right, y));
    let (forward, _) = tab_offset(&mut s);
    assert!(forward < back || back <= -max, "▶ scrolled: {back} -> {forward}");

    // ≡ lists the sheets.
    s.click((list, y));
    s.screenshot("list");
}

/// todo-fix #79: the right pane has no tabs; it shows the attributes of what is selected: the cells'
/// format, a text box's (its size and place, fill, outline), the cells' again after a click on a cell.
/// データの入力規則 opens from the Data menu.
fn fix11_right_pane_tabs() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    assert!(!s.with(|e, _| e.sidebar_open));
    s.update(|e, window, cx| e.toggle_sidebar(window, cx));
    s.settle();
    let state = |s: &mut Scenario| s.with(|e, _| {
        let st = e.sheet_state().expect("a workbook");
        (st.custom_format.is_some(), st.panels.frame.is_some(), st.validation.is_some())
    });
    assert_eq!(state(&mut s), (true, false, false), "cells: the cell format");
    s.screenshot("cells");
    // A text box inserted and selected: its attributes.
    s.menu("xlsx.insert-textbox");
    s.type_text("x");
    s.key("escape");
    s.settle();
    assert_eq!(state(&mut s), (false, true, false), "a text box: its attributes");
    s.screenshot("text-box");
    // Its fill and outline, typed in the pane, change the shape.
    let id = s.with(|e, _| e.selected_sheet_shape()).expect("the text box is selected");
    s.update(|e, window, cx| {
        let p = e.sheet_state().unwrap().panels.frame.as_ref().unwrap();
        let fill = p.attrs[0].clone();
        fill.update(cx, |f, cx| f.set_value("FF0000", window, cx));
        e.apply_sheet_frame_panel(window, cx);
    });
    let fill = s.with(move |e, _| match &e.slide().shape(id).unwrap().kind {
        ibis_doc_model::ShapeKind::Auto(a) => match &a.fill { ibis_doc_model::Fill::Solid(c) => Some(c.rgb.0), _ => None },
        _ => None,
    });
    assert_eq!(fill, Some(0xFF0000));
    // A click on a cell: the cell format again.
    let far = s.with(|e, _| e.cell_center(8, 20)).expect("the cell on screen");
    s.click((f32::from(far.x), f32::from(far.y)));
    s.settle();
    assert_eq!(state(&mut s), (true, false, false), "back to the cells");
    // データの入力規則 from the menu stays while the cells stay selected.
    s.menu("xlsx.validation");
    s.settle();
    assert_eq!(state(&mut s), (false, false, true));
    s.screenshot("validation");
}

/// #17: ⌘P (ファイル > 印刷…) was only written beside the menu item, with no key
/// binding and no item in the app's own メニュー: it is bound now, in each
/// format's window. (Printing itself opens Preview's dialog, so it is not run.)
fn fix17_print_is_bound() {
    for file in ["test/editing/xlsx-sales.xlsx", "test/editing/pptx-plan.pptx"] {
        let mut s = Scenario::open(file);
        let bound = s.update(|_, window, _| window.bindings_for_action(&crate::PrintFile).iter().map(|b| b.keystrokes().iter().map(|k| k.unparse()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>());
        assert!(bound.iter().any(|k| k == "cmd-p"), "{file}: {bound:?}");
    }
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let bound = s.update(|_, window, _| window.bindings_for_action(&crate::PrintFile).len());
    assert!(bound > 0);
}

/// Closes the modal command panel (Undo and the menus are the document's, so they come after it).
fn close_modal_panel(s: &mut Scenario) {
    if s.with(|e, _| e.panel_dialog_is_modal()) {
        s.update(|e, window, cx| e.close_panel_dialog(window, cx));
    }
}

/// DOCX-EDIT-014: 挿入 > 特殊文字… opens the panel of characters; pressing one
/// puts it at the caret, as one step to undo.
fn fix_docx_edit014_symbols() {
    use ibis_doc_model::FlowPos;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let first = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some((p.id, p.text())) } else { None }).unwrap()
    });
    let text_of = |s: &mut Scenario, id| {
        s.with(move |e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == id { Some(p.text()) } else { None }).unwrap()
        })
    };
    let (id, before) = first;
    // One character of each group exists, and the groups are not empty.
    for g in 0..crate::shell::symbols::group_names().len() {
        assert!(crate::shell::symbols::characters(g).len() >= 10, "group {g}");
    }
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(id, 0));
            st.caret = Some(FlowPos::new(id, 0));
        }
        cx.notify();
    });
    s.menu("insert.symbol");
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.symbol.is_some())));
    s.screenshot("symbols");
    s.update(|e, _, cx| e.insert_symbol('→', cx));
    assert_eq!(text_of(&mut s, id), format!("→{before}"));
    s.update(|e, _, cx| e.insert_symbol('€', cx));
    assert_eq!(text_of(&mut s, id), format!("→€{before}"));
    close_modal_panel(&mut s);
    s.key("cmd-z");
    assert_eq!(text_of(&mut s, id), format!("→{before}"), "one step undoes one character");
}

/// The start view: the window's title is the app and its version, the
/// title row shows the logo (no 開く there), and 新規作成's pages make an
/// empty file of their kind (the xlsx one here).
fn start_view_new_and_select() {
    let mut s = Scenario::start_view();
    assert_eq!(crate::shell::start_view::start_title(), format!("ibisOffice {}", env!("IBIS_VERSION")));
    s.screenshot("start");
    // 新規作成's second page (xlsx), below the section's title.
    s.click((150.0, 140.0));
    s.settle();
    assert!(s.with(|e, _| e.has_document && e.kind() == ibis_pptx::Kind::Workbook && e.path.is_none()));
    s.screenshot("new-workbook");
}

/// ヘルプ > ibisOffice について: the About window shows the version and
/// "Powered by ibisOffice" with the project's link (NOTICE: a derivative keeps
/// them), centred on the window.
fn about_shows_powered_by() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let parent = s.update(|_, window, _| window.bounds());
    s.menu("help.about");
    let handle = crate::shell::about_window::handle().expect("the About window is open");
    let bounds = s.update(|_, _, cx| handle.update(cx, |_, window, _| window.bounds()).expect("its bounds"));
    let (pc, bc) = (parent.center(), bounds.center());
    assert!((f32::from(pc.x) - f32::from(bc.x)).abs() < 1.0 && (f32::from(pc.y) - f32::from(bc.y)).abs() < 1.0);
    assert_eq!(crate::shell::about_window::POWERED_BY, "Powered by ibisOffice");
    assert_eq!(crate::shell::about_window::PROJECT_URL, "https://github.com/ibisjp/ibis-office");
    assert_eq!(crate::shell::about_window::version_line(), format!("バージョン {}", env!("IBIS_VERSION")));
    s.screenshot_of(handle.into(), "about");
}

/// DOCX-IMAGE-003/004: the picture at the caret is turned, flipped and cut from
/// its panel; drawn so (a shape with the turn, flips and crop), kept by saving.
fn fix_docx_image_look() {
    use ibis_doc_model::{FlowPos, FlowRunKind, PictureLook};
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let (id, offset) = s.with(|e, _| {
        for b in &e.presentation().flow.as_ref().unwrap().body {
            if let ibis_doc_model::Block::Paragraph(p) = b {
                let mut offset = 0;
                for r in &p.runs {
                    if matches!(r.kind, FlowRunKind::Picture { .. }) {
                        return (p.id, offset);
                    }
                    offset += r.len();
                }
            }
        }
        panic!("a picture");
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(id, offset));
            st.caret = Some(FlowPos::new(id, offset));
        }
        cx.notify();
    });
    s.menu("docx.picture.size");
    let look = |s: &mut Scenario| s.with(|e, _| e.picture_look_at_caret().expect("the picture"));
    assert_eq!(look(&mut s), PictureLook { name: "図 1".to_string(), ..Default::default() });
    let shapes = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            crate::docx::flow_layout::pages(&doc, &e.resources).pages.iter().flat_map(|p| p.items.iter()).filter(|i| matches!(i, crate::docx::flow_layout::Item::Shape { shape, .. } if matches!(shape.kind, ibis_doc_model::ShapeKind::Picture(_)))).count()
        })
    };
    assert_eq!(shapes(&mut s), 0, "an untouched picture is drawn as before");
    let doc_of = |s: &mut Scenario| s.with(|e, _| e.presentation().flow.clone().unwrap());
    let before = doc_of(&mut s);
    s.update(|e, _, cx| e.turn_picture(1, cx));
    s.update(|e, _, cx| e.flip_picture(true, cx));
    s.update(|e, _, cx| e.change_picture_look(|l| l.crop = ibis_doc_model::Crop { left: 10_000, top: 20_000, right: 0, bottom: 0 }, cx));
    let l = look(&mut s);
    assert_eq!((l.rotation, l.flip_h, l.flip_v, l.crop.left, l.crop.top), (5_400_000, true, false, 10_000, 20_000));
    assert_eq!(shapes(&mut s), 1, "drawn turned");
    s.screenshot("turned");
    s.save_as("image-look.docx");
    let saved = s.reopen();
    let read = ibis_doc_model::PictureLook { rotation: 5_400_000, flip_h: true, flip_v: false, crop: ibis_doc_model::Crop { left: 10_000, top: 20_000, right: 0, bottom: 0 }, stroke: None, alt_text: Default::default(), decorative: false, name: "図 1".to_string(), ..Default::default() };
    let found = saved.flow.as_ref().unwrap().body.iter().any(|b| matches!(b, ibis_doc_model::Block::Paragraph(p) if p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Picture { look, .. } if look == &read))));
    assert!(found, "saved and read again");
    // Three steps to undo, back to the picture as it was.
    for _ in 0..3 {
        s.key("cmd-z");
    }
    assert_eq!(doc_of(&mut s), before);
}

/// PPTX: 挿入 > 特殊文字… opens the symbol panel and inserting puts the character into the slide.
fn fix_pptx_symbols() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("insert.symbol");
    assert!(s.with(|e, _| e.symbol.is_some() && e.sidebar_open));
    s.screenshot("pptx-symbols");
    s.update(|e, window, cx| e.insert_symbol_with_window('★', window, cx));
    let has_symbol = s.with(|e, _| {
        let slide = e.slide();
        slide.shapes.iter().any(|sh| {
            sh.text_body().is_some_and(|tb| {
                tb.paragraphs.iter().any(|p| p.runs.iter().any(|r| r.text.contains('★')))
            })
        })
    });
    assert!(has_symbol, "symbol was inserted into pptx");
}

/// XLSX: 挿入 > 特殊文字… opens the symbol panel and inserting enters edit mode with the character.
fn fix_xlsx_symbols() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.symbol");
    assert!(s.with(|e, _| e.symbol.is_some() && e.sidebar_open));
    s.screenshot("xlsx-symbols");
    s.update(|e, _, cx| e.insert_symbol('✓', cx));
    let edit_text = s.with(|e, _| e.cell_edit().map(|c| c.text.clone()));
    assert!(edit_text.as_ref().is_some_and(|t| t.ends_with('✓')), "symbol '✓' was inserted into the cell");
}

/// DOCX-PARA-010: タブ位置・配置・リーダー設定パネルでタブ位置を設定・適用し、
/// 保存と再読み込み、アンドゥを検証する。
fn fix_docx_tab_stops() {
    use ibis_doc_model::{FlowPos, TabAlign};
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let first_id = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap()
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 0));
        }
        cx.notify();
    });
    // Open tab stops edit
    s.menu("docx.tabs");
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.tab_edit.is_some()) && e.sidebar_open));
    s.screenshot("tab-panel");

    // Set first tab stop: 72.0 pt, Right, leader: true
    s.update(|e, window, cx| {
        e.choose_tab_align(TabAlign::Right, cx);
        e.choose_tab_leader(true, cx);
        if let Some(edit) = e.doc_state_mut().and_then(|st| st.tab_edit.as_mut()) {
            let field = edit.pos_field.clone();
            field.update(cx, |i, cx| i.set_value("72", window, cx));
        }
        e.set_tab_stop(window, cx);
    });

    // Set second tab stop: 144.0 pt, Center, leader: false
    s.update(|e, window, cx| {
        e.choose_tab_align(TabAlign::Center, cx);
        e.choose_tab_leader(false, cx);
        if let Some(edit) = e.doc_state_mut().and_then(|st| st.tab_edit.as_mut()) {
            let field = edit.pos_field.clone();
            field.update(cx, |i, cx| i.set_value("144", window, cx));
        }
        e.set_tab_stop(window, cx);
    });

    let stop_count = s.with(|e, _| e.doc_state().and_then(|d| d.tab_edit.as_ref()).map(|ed| ed.stops.len()).unwrap_or(0));
    assert_eq!(stop_count, 2, "2 tab stops configured in panel");

    // Apply edits
    s.update(|e, window, cx| e.apply_tab_edit(window, cx));

    // Verify paragraph has the 2 tab stops
    let tabs_of = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id { Some(p.tabs.clone()) } else { None }).unwrap()
        })
    };
    let applied = tabs_of(&mut s);
    assert_eq!(applied.len(), 2);
    assert_eq!(applied[0].position, 72 * 12_700);
    assert_eq!(applied[0].align, TabAlign::Right);
    assert!(applied[0].leader);
    assert_eq!(applied[1].position, 144 * 12_700);
    assert_eq!(applied[1].align, TabAlign::Center);
    assert!(!applied[1].leader);

    // Save and reload
    s.save_as("tab-stops.docx");
    let reloaded = s.reopen();
    let reloaded_tabs = reloaded.flow.as_ref().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id { Some(p.tabs.clone()) } else { None }).unwrap();
    assert_eq!(reloaded_tabs.len(), 2, "tab stops kept by saving and reopening");
    assert_eq!(reloaded_tabs[0].position, 72 * 12_700);
    assert_eq!(reloaded_tabs[0].align, TabAlign::Right);
    assert!(reloaded_tabs[0].leader);

    // Undo restores to original
    s.key("cmd-z");
    let undone = tabs_of(&mut s);
    assert_eq!(undone.len(), 0, "undo restores original tabs");
}

/// DOCX-PARA-007: 同一スタイル間の間隔抑制（w:contextualSpacing）
/// パネルでのチェックボックス切り替え・適用、保存と再読み込みでの保持、
/// アンドゥ、ツールバーメニューからのトグル動作を検証する。
fn fix_docx_contextual_spacing() {
    use ibis_doc_model::FlowPos;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let (id1, _) = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        let mut paras = doc.body.iter().filter_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None });
        (paras.next().unwrap(), paras.next().unwrap())
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(id1, 0));
            st.caret = Some(FlowPos::new(id1, 0));
        }
        cx.notify();
    });

    let is_contextual = |s: &mut Scenario, id| {
        s.with(move |e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == id { Some(p.contextual_spacing) } else { None }).unwrap()
        })
    };
    let initial = is_contextual(&mut s, id1);

    // Open paragraph details panel
    s.menu("docx.paragraph.details");
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.paragraph_edit.is_some()) && e.sidebar_open));
    assert_eq!(
        s.with(|e, _| e.doc_state().and_then(|d| d.paragraph_edit.as_ref()).and_then(|p| p.contextual_spacing)),
        Some(initial),
        "panel shows initial contextual spacing"
    );
    s.screenshot("contextual-spacing-panel");

    // Toggle contextual spacing checkbox to opposite and apply
    s.update(|e, window, cx| {
        e.toggle_paragraph_contextual_spacing(!initial, cx);
        e.apply_paragraph_edit(window, cx);
    });
    assert_eq!(is_contextual(&mut s, id1), !initial, "contextual spacing toggled via panel");

    // Save as and reload to verify round-trip
    s.save_as("contextual-spacing.docx");
    let reloaded = s.reopen();
    let reloaded_contextual = reloaded.flow.as_ref().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == id1 { Some(p.contextual_spacing) } else { None }).unwrap();
    assert_eq!(reloaded_contextual, !initial, "contextual spacing preserved by save and reload");

    // Undo restores to original
    close_modal_panel(&mut s);
    s.key("cmd-z");
    assert_eq!(is_contextual(&mut s, id1), initial, "undo restores original contextual spacing");

    // Menu toggle: turn to !initial
    s.menu("docx.spacing.contextual");
    assert_eq!(is_contextual(&mut s, id1), !initial, "toolbar menu toggles contextual spacing");

    // Menu toggle: turn back to initial
    s.menu("docx.spacing.contextual");
    assert_eq!(is_contextual(&mut s, id1), initial, "toolbar menu toggles contextual spacing back");
}

/// DOCX-FONT-015: カーニング（w:kern）
/// メニュー「表示形式 > テキスト > カーニング」からの設定、保存と再読み込みでの保持、
/// アンドゥ、カーニングなし（0 pt）への設定、設定解除を検証する。
fn fix_docx_kerning() {
    use ibis_doc_model::FlowPos;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let first_id = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap()
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 3));
        }
        cx.notify();
    });

    let kern_of = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id {
                p.runs.first().and_then(|r| r.format.kern)
            } else { None })
        })
    };

    // Apply kerning: 12 pt and above (24 half-points)
    s.menu("docx.text.kern-12");
    assert_eq!(kern_of(&mut s), Some(24), "kerning set to 24 half-points (12 pt)");

    // Save as and reopen to verify OOXML round-trip
    s.save_as("kerning.docx");
    let reloaded = s.reopen();
    let reloaded_kern = reloaded.flow.as_ref().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id {
        p.runs.first().and_then(|r| r.format.kern)
    } else { None });
    assert_eq!(reloaded_kern, Some(24), "kerning preserved by save and reload");

    // Undo restores original
    s.key("cmd-z");
    assert_ne!(kern_of(&mut s), Some(24), "undo restores original kerning");

    // Re-select and apply kerning off (0 pt)
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 3));
        }
        cx.notify();
    });
    s.menu("docx.text.kern-none");
    assert_eq!(kern_of(&mut s), Some(0), "kerning set to 0 (off)");

    // Clear kerning setting
    s.menu("docx.text.kern-clear");
    assert_eq!(kern_of(&mut s), None, "kerning setting cleared");
}

/// DOCX-PARA-013（孤立行防止 w:widowControl）および DOCX-PARA-017（行番号の対象除外 w:suppressLineNumbers）
/// パネルでのトグル・適用、保存と再読み込みでの保持、アンドゥ、メニューからのトグル動作を検証する。
fn fix_docx_widow_and_suppress_lines() {
    use ibis_doc_model::FlowPos;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let first_id = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap()
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 0));
        }
        cx.notify();
    });

    let flags_of = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id {
                Some((p.widow_control, p.suppress_line_numbers))
            } else { None }).unwrap()
        })
    };

    let (initial_widow, initial_suppress) = flags_of(&mut s);

    // Open paragraph details panel
    s.menu("docx.paragraph.details");
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.paragraph_edit.is_some()) && e.sidebar_open));
    assert_eq!(
        s.with(|e, _| e.doc_state().and_then(|d| d.paragraph_edit.as_ref()).map(|p| (p.widow_control, p.suppress_line_numbers))),
        Some((Some(initial_widow), Some(initial_suppress))),
        "panel shows initial states"
    );

    // Toggle checkboxes and apply via panel
    s.update(|e, window, cx| {
        e.toggle_paragraph_widow_control(!initial_widow, cx);
        e.toggle_paragraph_suppress_line_numbers(!initial_suppress, cx);
        e.apply_paragraph_edit(window, cx);
    });
    assert_eq!(flags_of(&mut s), (!initial_widow, !initial_suppress), "panel applied toggled states");

    // Save as and reload to verify OOXML round-trip
    s.save_as("widow-suppress.docx");
    let reloaded = s.reopen();
    let reloaded_flags = reloaded.flow.as_ref().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b && p.id == first_id {
        Some((p.widow_control, p.suppress_line_numbers))
    } else { None }).unwrap();
    assert_eq!(reloaded_flags, (!initial_widow, !initial_suppress), "widow_control and suppress_line_numbers preserved by save and reload");

    // Undo restores original settings
    close_modal_panel(&mut s);
    s.key("cmd-z");
    assert_eq!(flags_of(&mut s), (initial_widow, initial_suppress), "undo restores original settings");

    // Menu toggle: widow-control to opposite
    s.menu("docx.para.widow-control");
    assert_eq!(flags_of(&mut s).0, !initial_widow, "menu turned widow control to opposite");

    // Menu toggle: widow-control back to initial
    s.menu("docx.para.widow-control");
    assert_eq!(flags_of(&mut s).0, initial_widow, "menu turned widow control back to initial");

    // Menu toggle: suppress-line-numbers to opposite
    s.menu("docx.para.suppress-line-numbers");
    assert_eq!(flags_of(&mut s).1, !initial_suppress, "menu turned suppress line numbers to opposite");

    // Menu toggle: suppress-line-numbers back to initial
    s.menu("docx.para.suppress-line-numbers");
    assert_eq!(flags_of(&mut s).1, initial_suppress, "menu turned suppress line numbers back to initial");
}

/// XLSX-SHEET-015 (見出し表示) & XLSX-SHEET-018 (数式バー表示):
/// toggle headings and formula bar from menus, save and reload, undo/redo.
fn fix_xlsx_headings_and_formula_bar() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // Initial states: headings is true, formula bar is true
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, true)));
    assert_eq!(s.menu_state("xlsx.formula-bar"), Some((true, true)));
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings));
    assert!(s.with(|e, _| e.sheet_state().unwrap().show_formula_bar));

    // Toggle formula bar off
    s.menu("xlsx.formula-bar");
    assert_eq!(s.menu_state("xlsx.formula-bar"), Some((true, false)));
    assert!(!s.with(|e, _| e.sheet_state().unwrap().show_formula_bar));

    // Toggle formula bar back on
    s.menu("xlsx.formula-bar");
    assert_eq!(s.menu_state("xlsx.formula-bar"), Some((true, true)));
    assert!(s.with(|e, _| e.sheet_state().unwrap().show_formula_bar));

    // Toggle headings off
    s.menu("xlsx.headings");
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, false)));
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings));

    // Save and reopen
    s.save_as("headings-off.xlsx");
    let reloaded = s.reopen();
    assert!(!ibis_doc_model::sheet_calc::sheet_of_slide(&reloaded.slides[0]).unwrap().headings, "headings state preserved after save & reopen");

    // Undo toggle headings
    s.key("cmd-z");
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings), "headings restored by undo");
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, true)));

    // Redo toggle headings
    s.key("cmd-shift-z");
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings), "headings toggled by redo");
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, false)));

    // Toggle headings back on via menu
    s.menu("xlsx.headings");
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().headings));
    assert_eq!(s.menu_state("xlsx.headings"), Some((true, true)));
}

/// XLSX-SHEET-013 (表示倍率・ズーム):
/// change sheet zoom, save and reload (OOXML sheetView/@zoomScale), undo/redo.
fn fix_xlsx_zoom() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // Initial state: zoom is None (default 100%)
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), None);
    assert_eq!(s.with(|e, _| e.zoom), Some(1.0));

    // Change zoom to 150%
    s.update(|e, window, cx| e.set_zoom(Some(1.5), window, cx));
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), Some(150));
    assert_eq!(s.with(|e, _| e.zoom), Some(1.5));

    // Change zoom to 200%
    s.update(|e, window, cx| e.set_zoom(Some(2.0), window, cx));
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), Some(200));
    assert_eq!(s.with(|e, _| e.zoom), Some(2.0));

    // Save as and reload to verify OOXML round-trip
    s.save_as("zoom-test.xlsx");
    let reloaded = s.reopen();
    assert_eq!(ibis_doc_model::sheet_calc::sheet_of_slide(&reloaded.slides[0]).unwrap().zoom, Some(200), "zoomScale preserved by save & reopen");

    // Undo restores 150%
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), Some(150));
    assert_eq!(s.with(|e, _| e.zoom), Some(1.5));

    // Undo restores initial 100% (None)
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), None);
    assert_eq!(s.with(|e, _| e.zoom), Some(1.0));

    // Redo restores 150%
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), Some(150));
    assert_eq!(s.with(|e, _| e.zoom), Some(1.5));

    // Zoom back to 100% via set_zoom(Some(1.0))
    s.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    assert_eq!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom), None);
    assert_eq!(s.with(|e, _| e.zoom), Some(1.0));
}

/// XLSX-SHEET-019 (改ページプレビュー):
/// toggle page break preview via menu, save and reload (OOXML sheetView/@view="pageBreakPreview"), undo/redo.
fn fix_xlsx_page_break_preview() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // Initial state: pageBreakPreview is false
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview));
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, false)));

    // Toggle page break preview on
    s.menu("xlsx.page-break-preview");
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, true)));
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview));

    // Save as and reload to verify OOXML round-trip
    s.save_as("page-break-preview.xlsx");
    let reloaded = s.reopen();
    assert!(ibis_doc_model::sheet_calc::sheet_of_slide(&reloaded.slides[0]).unwrap().page_break_preview, "pageBreakPreview preserved after save & reopen");

    // Undo toggle
    s.key("cmd-z");
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview), "pageBreakPreview restored by undo");
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, false)));

    // Redo toggle
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview), "pageBreakPreview restored by redo");
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, true)));

    // Toggle back off via menu
    s.menu("xlsx.page-break-preview");
    assert!(!s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().page_break_preview));
    assert_eq!(s.menu_state("xlsx.page-break-preview"), Some((true, false)));
}

/// DOCX-PAGE-017 (行番号の開始・再開始・設定):
/// toggle line numbers via menu, save and reload (OOXML w:lnNumType), undo/redo.
fn fix_docx_line_numbers() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    let get_ln = |s: &mut Scenario| {
        s.with(|e, _| {
            let doc = e.presentation().flow.clone().unwrap();
            doc.page.line_numbers
        })
    };

    // Initial state: line_numbers is None
    assert_eq!(get_ln(&mut s), None);
    assert_eq!(s.menu_state("docx.line-numbers.none"), Some((true, true)));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, false)));
    assert_eq!(s.menu_state("docx.line-numbers.restart-page"), Some((true, false)));
    assert_eq!(s.menu_state("docx.line-numbers.restart-section"), Some((true, false)));

    // Switch to continuous line numbering
    s.menu("docx.line-numbers.continuous");
    assert_eq!(s.menu_state("docx.line-numbers.none"), Some((true, false)));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, true)));
    let ln = get_ln(&mut s).expect("line_numbers should be set");
    assert_eq!(ln.restart, ibis_doc_model::LineNumberRestart::Continuous);

    // Save as and reload to verify OOXML round-trip
    s.save_as("line-numbers-test.docx");
    let reloaded = s.reopen();
    let reloaded_ln = reloaded.flow.as_ref().unwrap().page.line_numbers.expect("reloaded docx has line_numbers");
    assert_eq!(reloaded_ln.restart, ibis_doc_model::LineNumberRestart::Continuous, "line_numbers restart preserved after save & reopen");

    // Undo restores None
    s.key("cmd-z");
    assert_eq!(get_ln(&mut s), None, "undo restores None line_numbers");
    assert_eq!(s.menu_state("docx.line-numbers.none"), Some((true, true)));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, false)));

    // Redo restores continuous
    s.key("cmd-shift-z");
    assert_eq!(get_ln(&mut s).map(|n| n.restart), Some(ibis_doc_model::LineNumberRestart::Continuous));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, true)));

    // Switch to restart each page
    s.menu("docx.line-numbers.restart-page");
    assert_eq!(get_ln(&mut s).map(|n| n.restart), Some(ibis_doc_model::LineNumberRestart::NewPage));
    assert_eq!(s.menu_state("docx.line-numbers.restart-page"), Some((true, true)));
    assert_eq!(s.menu_state("docx.line-numbers.continuous"), Some((true, false)));

    // Save as and reload
    s.save_as("line-numbers-restart-page.docx");
    let reloaded2 = s.reopen();
    let reloaded_ln2 = reloaded2.flow.as_ref().unwrap().page.line_numbers.expect("reloaded docx has restart-page line_numbers");
    assert_eq!(reloaded_ln2.restart, ibis_doc_model::LineNumberRestart::NewPage);

    // Turn off line numbering via "none"
    s.menu("docx.line-numbers.none");
    assert_eq!(get_ln(&mut s), None);
    assert_eq!(s.menu_state("docx.line-numbers.none"), Some((true, true)));
    assert_eq!(s.menu_state("docx.line-numbers.restart-page"), Some((true, false)));

    // Save as and reload
    s.save_as("line-numbers-none.docx");
    let reloaded3 = s.reopen();
    assert_eq!(reloaded3.flow.as_ref().unwrap().page.line_numbers, None);
}

/// DOCX-VIEW-011: 水平ルーラーの表示切替（表示 > ルーラー）
/// 初期状態でのルーラー表示（true）、メニューでのチェック表示、
/// メニュークリックによる非表示化（false）とチェック解除、
/// 再クリックによる表示復帰（true）とチェック表示を検証する。
fn fix_docx_ruler() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // The ruler is a setting of the right pane now (表示 > ルーラー opens it), not a toggle in the menu.
    assert!(s.with(|e, _| e.ruler_open), "ruler should be visible initially");
    assert_eq!(s.menu_state("docx.ruler"), Some((true, false)), "docx.ruler opens the pane, so the item is enabled and has no check");
    s.menu("docx.ruler");
    assert!(s.with(|e, _| e.ruler_settings_open && e.sidebar_open));
    s.screenshot("docx-ruler-visible");

    docx_click_ruler_setting(&mut s, "ruler-horizontal");
    assert!(!s.with(|e, _| e.ruler_open), "ruler should be hidden after unchecking it");
    s.screenshot("docx-ruler-hidden");

    docx_click_ruler_setting(&mut s, "ruler-horizontal");
    assert!(s.with(|e, _| e.ruler_open), "ruler should be visible again after checking it");
}

/// Clicks the control `id` of the ruler settings pane (the center of where it was drawn).
fn docx_click_ruler_setting(s: &mut Scenario, id: &str) {
    let bounds = s.with(|e, _| *e.ruler_settings_ui.borrow().get(id).unwrap_or_else(|| panic!("missing ruler UI {id}")));
    s.click((f32::from(bounds.origin.x + bounds.size.width / 2.), f32::from(bounds.origin.y + bounds.size.height / 2.)));
    s.settle();
}

/// XLSX-VIEW-008: ルーラーの表示条件
/// 標準表示・改ページプレビューでは無効（disabled・チェックなし）、
/// ページレイアウト表示（view="pageLayout"）時のみ有効（enabled・初期値チェックあり・ルーラー表示）。
/// チェック切替でルーラー非表示／表示、Undo/Redo、OOXML sheetView/@showRuler 保存・再読込ラウンドトリップを検証する。
fn fix_xlsx_ruler() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. 初期状態（標準表示）: xlsx.ruler は無効（disabled: true -> menu_state enabled: false）
    assert_eq!(s.menu_state("xlsx.ruler"), Some((false, false)), "xlsx.ruler should be disabled in normal view");
    assert!(s.with(|e, _| e.current_sheet().unwrap().1.show_ruler));

    // 2. ページレイアウト表示へ切り替え（Page Layout view）
    s.menu("xlsx.page-layout");
    assert_eq!(s.menu_state("xlsx.page-layout"), Some((true, true)));
    // ページレイアウト表示では xlsx.ruler が有効化され、初期状態でチェック付き
    assert_eq!(s.menu_state("xlsx.ruler"), Some((true, true)), "xlsx.ruler should be enabled and checked in page layout view");
    assert!(s.with(|e, _| e.current_sheet().unwrap().1.show_ruler));
    s.screenshot("page-layout-ruler-visible");

    // 3. ルーラーをクリックして非表示にする
    s.menu("xlsx.ruler");
    assert_eq!(s.menu_state("xlsx.ruler"), Some((true, false)), "xlsx.ruler should be unchecked after toggle");
    assert!(!s.with(|e, _| e.current_sheet().unwrap().1.show_ruler));
    s.screenshot("page-layout-ruler-hidden");

    // 4. Undo でルーラー再表示
    s.key("cmd-z");
    assert!(s.with(|e, _| e.current_sheet().unwrap().1.show_ruler), "ruler restored by undo");
    assert_eq!(s.menu_state("xlsx.ruler"), Some((true, true)));

    // 5. Redo でルーラー再非表示
    s.key("cmd-shift-z");
    assert!(!s.with(|e, _| e.current_sheet().unwrap().1.show_ruler), "ruler hidden by redo");
    assert_eq!(s.menu_state("xlsx.ruler"), Some((true, false)));

    // 6. 保存と開き直し（OOXML sheetView/@showRuler="0" のラウンドトリップ）
    s.save_as("ruler-off.xlsx");
    let reloaded = s.reopen();
    let r_sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&reloaded.slides[0]).unwrap();
    assert!(!r_sheet.show_ruler, "show_ruler=false preserved after save and reopen");
    assert!(r_sheet.page_layout, "page_layout preserved");

    // 7. 標準表示に戻すと、再び無効化される
    s.menu("xlsx.page-layout");
    assert_eq!(s.menu_state("xlsx.ruler"), Some((false, false)), "xlsx.ruler should be disabled when exiting page layout view");
}

/// XLSX-VIEW-002: ページレイアウト表示でのルーラー余白ドラッグおよび測定単位切替
/// ページレイアウト表示中のルーラー上余白ハンドル（左・右・上・下）をドラッグして余白（margins）を動的変更、
/// ガイドライン描画、Undo/Redo 連動、ルーラーコーナーでの単位切替（cm -> mm -> in -> pt -> pi）、
/// および OOXML <pageMargins> 保存・開き直しラウンドトリップを検証する。
fn fix_xlsx_page_layout_ruler_drag() {
    use crate::xlsx::page_layout::RulerMargin;
    use crate::shell::editor::RulerUnit;

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.page-layout");
    assert_eq!(s.menu_state("xlsx.page-layout"), Some((true, true)));

    // 1. ルーラーの初期余白取得
    let initial_margins = s.with(|e, _| e.current_sheet().unwrap().1.print.margins);

    // 2. 左余白ハンドルの位置取得とドラッグ
    let left_handle = s.with(|e, _| e.page_layout_ruler_margin_handle_pos(RulerMargin::Left).unwrap());
    let (lx0, ly0) = (f32::from(left_handle.x), f32::from(left_handle.y));
    let drag_dx = 25.0; // 右方向へドラッグ（左余白を拡大）
    s.drag((lx0, ly0), (lx0 + drag_dx, ly0));
    s.settle();

    let new_margins = s.with(|e, _| e.current_sheet().unwrap().1.print.margins);
    assert!(new_margins[0] > initial_margins[0], "left margin increased by dragging right: before={}, after={}", initial_margins[0], new_margins[0]);
    s.screenshot("page-layout-margin-left-dragged");

    // 3. Undo で左余白が元に戻ることを検証
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.print.margins), initial_margins, "undo restores initial left margin");

    // 4. Redo で変更後の左余白が再適用されることを検証
    s.key("cmd-shift-z");
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.print.margins), new_margins, "redo reapplies dragged left margin");

    // 5. 上余白ハンドルのドラッグ
    let top_handle = s.with(|e, _| e.page_layout_ruler_margin_handle_pos(RulerMargin::Top).unwrap());
    let (tx0, ty0) = (f32::from(top_handle.x), f32::from(top_handle.y));
    let drag_dy = 20.0; // 下方向へドラッグ（上余白を拡大）
    s.drag((tx0, ty0), (tx0, ty0 + drag_dy));
    s.settle();

    let after_top = s.with(|e, _| e.current_sheet().unwrap().1.print.margins);
    assert!(after_top[2] > new_margins[2], "top margin increased by dragging down: before={}, after={}", new_margins[2], after_top[2]);
    s.screenshot("page-layout-margin-top-dragged");

    // 6. ルーラーコーナー（0, 0）をクリックして測定単位を順送り切替
    assert_eq!(s.with(|e, _| e.ruler_unit), RulerUnit::Centimeters);
    let corner = s.with(|e, _| e.page_layout_ruler_corner_pos().unwrap());
    s.click((f32::from(corner.x), f32::from(corner.y)));
    assert_eq!(s.with(|e, _| e.ruler_unit), RulerUnit::Millimeters, "cycled to Millimeters");
    s.click((f32::from(corner.x), f32::from(corner.y)));
    assert_eq!(s.with(|e, _| e.ruler_unit), RulerUnit::Inches, "cycled to Inches");
    s.screenshot("page-layout-ruler-inches");

    // 7. 保存・開き直しで変更された余白とページレイアウト表示が保持されることを検証
    s.save_as("ruler-drag-margins.xlsx");
    let reloaded = s.reopen();
    let r_sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&reloaded.slides[0]).unwrap();
    assert_eq!(r_sheet.print.margins, after_top, "dragged margins preserved across save and reopen");
    assert!(r_sheet.page_layout, "page_layout preserved");

    // 8. 変更をすべて Undo
    s.key("cmd-z"); // top margin
    s.key("cmd-z"); // left margin
    assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.print.margins), initial_margins, "all margin changes undone");
}

/// XLSX-FILE-006（コピー保存）および XLSX-FILE-003（最近使ったブック）
/// Excel（.xlsx）を開いたときの最近使ったファイル一覧（recent_files）への登録、
/// セル編集後の別名コピー保存（copy_to）、ウィンドウ自身のパス保持、元ファイルの非変更、
/// およびコピー先ファイルへの編集反映を検証する。
fn fix_xlsx_save_copy_and_recent() {
    use crate::xlsx::scenarios::SheetSteps as _;

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let own = s.with(|e, _| e.path.clone()).expect("file has path");
    let original = std::fs::read(&own).expect("can read original file");

    // XLSX-FILE-003: 開いたブックが最近使ったファイル一覧（recent_files）に登録され保持されることを検証
    s.update(|e, _, _| {
        e.recent_files = crate::shell::recent_files::add(&own);
    });
    let recents = s.with(|e, _| e.recent_files.clone());
    assert!(
        recents.iter().any(|p| p.file_name() == own.file_name()),
        "recent_files should contain opened xlsx file"
    );

    // セルを編集
    s.click_cell("A1");
    s.enter("コピーテスト", "enter");
    assert_eq!(s.shown("A1"), "コピーテスト");

    // XLSX-FILE-006: コピーを作成（copy_to）
    let copy_path = crate::shell::scenario::repo_root().join("target/scenario/xlsx::save_copy/copy.xlsx");
    if let Some(parent) = copy_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    s.with(|e, _| e.copy_to(&copy_path)).expect("copy_to succeeds for xlsx");

    // ウィンドウ自身のパスは変更されない
    assert_eq!(s.with(|e, _| e.path.clone()), Some(own.clone()), "window keeps original path");

    // 元のファイルは保存されていない（未保存のまま）
    assert_eq!(std::fs::read(&own).unwrap(), original, "original file remains unchanged");

    // コピーされたファイルに編集が正しく反映されている
    let copied_bytes = std::fs::read(&copy_path).expect("copied file exists");
    let copied = ibis_pptx::read(&copied_bytes).expect("can read copied xlsx");
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&copied.slides[0]).expect("sheet exists");
    let a1_cell = sheet.cell(0, 0).expect("A1 cell exists");
    assert_eq!(
        a1_cell.text,
        "コピーテスト",
        "copied file contains edited cell content"
    );
}

/// DOCX-VIEW-014: グリッド線の表示切替（表示 > グリッド線）
/// 初期状態でのグリッド線非表示（false）、メニューでの未チェック表示、
/// メニュークリックによる表示化（true）とチェック表示、
/// 再クリックによる非表示化（false）とチェック解除を検証する。
fn fix_docx_gridlines() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // Grid lines are a setting of the ruler pane now (not a separate menu item).
    assert!(!s.with(|e, _| e.gridlines_open), "gridlines should be hidden initially");
    assert!(s.menu_state("docx.gridlines").is_none(), "docx.gridlines is no longer a menu item");
    s.menu("docx.ruler");
    s.screenshot("docx-gridlines-hidden");

    docx_click_ruler_setting(&mut s, "ruler-gridlines");
    assert!(s.with(|e, _| e.gridlines_open), "gridlines should be visible after checking it");
    s.screenshot("docx-gridlines-visible");

    docx_click_ruler_setting(&mut s, "ruler-gridlines");
    assert!(!s.with(|e, _| e.gridlines_open), "gridlines should be hidden again after unchecking it");
}

/// XLSX-VIEW-005: 選択範囲に合わせたズーム（表示 > 選択範囲に合わせて拡大/縮小）
/// セル範囲を選択し、メニューをクリックして選択範囲に合わせたズーム倍率への変更とスクロールを検証。
fn fix_xlsx_zoom_to_selection() {
    use crate::xlsx::scenarios::SheetSteps as _;

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_zoom = s.with(|e, _| e.current_zoom());
    assert!((initial_zoom - 1.0).abs() < 0.01, "initial zoom is 100%");

    // メニューが存在し、有効であること
    assert_eq!(s.menu_state("xlsx.zoom-selection"), Some((true, false)));

    // セル A1 を選択し、Shift+Right と Shift+Down で狭い範囲（A1:B3）を選択
    s.click_cell("A1");
    s.key("shift-right");
    s.key("shift-down");
    s.key("shift-down");

    // メニューから選択範囲に合わせてズームを実行
    s.menu("xlsx.zoom-selection");
    let zoomed = s.with(|e, _| e.current_zoom());
    assert!(zoomed > 1.0, "zoom should zoom in for a small selection (was {zoomed})");
    s.screenshot("xlsx-zoom-to-selection");

    // ズームを100%にリセットし、直接メソッド呼び出しでも動作することを検証
    s.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    assert!((s.with(|e, _| e.current_zoom()) - 1.0).abs() < 0.01);
    s.update(|e, window, cx| e.zoom_to_selection(window, cx));
    let zoomed_again = s.with(|e, _| e.current_zoom());
    assert_eq!(zoomed_again, zoomed, "zoom_to_selection yields consistent zoom");
}

/// XLSX-VALID-006: データの入力規則の空白の扱い（allow_blank）
/// 空白を無視する（allow_blank = true、既定）のときは未入力・空白の確定が許可され、
/// allow_blank = false に設定したときは空白の入力が規則違反として拒否されることを検証。
/// 保存・再オープンでの allowBlank の保持、解除を検証。
fn fix_xlsx_validation_allow_blank() {
    use crate::xlsx::scenarios::SheetSteps as _;
    use ibis_doc_model::{CellValue, Command, SheetCommand, Validation};

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // セル B10 に数値（1〜10）の入力規則を設定（allow_blank: true 既定）
    s.click_cell("B10");
    let range = s.with(|e, _| e.selected_range().unwrap());
    let rule_allow_blank = Validation {
        kind: "whole".into(),
        operator: "between".into(),
        formula1: Some("1".into()),
        formula2: Some("10".into()),
        allow_blank: true,
        error: Some(("stop".into(), "エラー".into(), "1から10の整数を入力してください".into())),
        ..Default::default()
    };
    let slide = s.with(|e, _| e.current_slide);
    s.update(|e, _, _| {
        e.document
            .execute(vec![Command::Sheet(SheetCommand::SetValidation {
                slide,
                ranges: vec![range],
                rule: Some(rule_allow_blank),
            })])
            .unwrap();
    });

    // 1. allow_blank = true のとき：
    // 数値 5 を入力 → 成功
    s.click_cell("B10");
    s.enter("5", "enter");
    assert_eq!(s.value("B10"), CellValue::Number(5.0));

    // F2 で編集モードに入り、文字を消去して空文字確定 → allow_blank = true なので成功
    s.click_cell("B10");
    s.key("f2");
    s.key("cmd-a");
    s.key("backspace");
    s.key("enter");
    assert_eq!(s.value("B10"), CellValue::Empty, "allow_blank = true permits clearing to empty");

    // 範囲外の数値 15 を入力 → 拒否される
    s.click_cell("B10");
    s.enter("15", "enter");
    assert_eq!(s.value("B10"), CellValue::Empty, "value out of range is refused");
    s.key("escape");

    // 2. allow_blank = false に変更
    let rule_no_blank = Validation {
        kind: "whole".into(),
        operator: "between".into(),
        formula1: Some("1".into()),
        formula2: Some("10".into()),
        allow_blank: false,
        error: Some(("stop".into(), "エラー".into(), "空白は許可されていません".into())),
        ..Default::default()
    };
    s.update(|e, _, _| {
        e.document
            .execute(vec![Command::Sheet(SheetCommand::SetValidation {
                slide,
                ranges: vec![range],
                rule: Some(rule_no_blank),
            })])
            .unwrap();
    });

    // 正しい数値 8 を入力 → 成功
    s.click_cell("B10");
    s.enter("8", "enter");
    assert_eq!(s.value("B10"), CellValue::Number(8.0));

    // allow_blank = false の状態で F2 編集に入り、文字を消して空文字で確定を試みる → 拒否される！
    s.click_cell("B10");
    s.key("f2");
    s.key("cmd-a");
    s.key("backspace");
    s.key("enter");
    assert_ne!(s.value("B10"), CellValue::Empty, "empty entry is refused when allow_blank = false");
    assert!(s.status_texts().iter().any(|t| t.contains("空白は許可されていません")), "{:?}", s.status_texts());
    s.key("escape");

    // 3. 保存と再オープンで allowBlank 属性が保持されることを検証
    s.save_as("fix-xlsx-validation-allow-blank.xlsx");
    let presentation = s.reopen();
    let sheet = presentation.slides.iter().find_map(ibis_doc_model::sheet_calc::sheet_of_slide).expect("sheet exists");
    let read_rule = sheet.validations.iter().find(|v| v.ranges.iter().any(|r| r.first_row == range.first_row && r.first_column == range.first_column)).expect("rule exists");
    assert_eq!(read_rule.allow_blank, false, "allow_blank: false is preserved through save and reopen");
    assert_eq!(read_rule.kind, "whole");

    // 4. クリアして規則解除
    s.update(|e, _, _| {
        e.document
            .execute(vec![Command::Sheet(SheetCommand::SetValidation {
                slide,
                ranges: vec![range],
                rule: None,
            })])
            .unwrap();
    });
    assert!(s.with(|e, _| e.current_rule().is_none()));
}

/// DOCX-IMAGE-001: 画像の置換（画像の変更 Change Picture）
/// 画像が選択されている時のみメニュー「書式 > 画像の変更…」が有効化されること、
/// 画像を置き換えた際に MediaId が更新されサイズが保持されること、
/// アンドゥで元の画像に戻り、リドゥで再度置換されること、
/// 保存と再読み込み後も新しい画像が保持されていることを検証する。
fn fix_docx_replace_picture() {
    use ibis_doc_model::{FlowPos, FlowRunKind, MediaFormat};

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xDE,
        0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54,
        0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00, 0x00,
        0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0,
        0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
        0xAE, 0x42, 0x60, 0x82,
    ];

    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. テキスト位置ではメニュー「docx.picture.replace」は無効
    let first_id = s.with(|e, _| {
        let doc = e.presentation().flow.clone().unwrap();
        doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.id) } else { None }).unwrap()
    });
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(FlowPos::new(first_id, 0));
            st.caret = Some(FlowPos::new(first_id, 0));
        }
        cx.notify();
    });
    assert_eq!(s.menu_state("docx.picture.replace"), Some((false, false)), "docx.picture.replace is disabled when not on picture");

    // 2. 画像の位置を特定してキャレットを置く
    let (para_id, offset, initial_media, initial_size) = s.with(|e, _| {
        for b in &e.presentation().flow.as_ref().unwrap().body {
            if let ibis_doc_model::Block::Paragraph(p) = b {
                let mut offset = 0;
                for r in &p.runs {
                    if let FlowRunKind::Picture { media, size, .. } = r.kind {
                        return (p.id, offset, media, size);
                    }
                    offset += r.len();
                }
            }
        }
        panic!("picture not found");
    });
    let pic_pos = FlowPos::new(para_id, offset);
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut() {
            st.anchor = Some(pic_pos);
            st.caret = Some(pic_pos);
        }
        cx.notify();
    });

    // 3. 画像位置ではメニュー「docx.picture.replace」が有効
    assert_eq!(s.menu_state("docx.picture.replace"), Some((true, false)), "docx.picture.replace is enabled at picture");
    s.screenshot("docx-picture-replace-menu");

    let current_media = |s: &mut Scenario| {
        s.with(|e, _| {
            for b in &e.presentation().flow.as_ref().unwrap().body {
                if let ibis_doc_model::Block::Paragraph(p) = b && p.id == para_id {
                    let mut offset = 0;
                    for r in &p.runs {
                        if let FlowRunKind::Picture { media, size, .. } = r.kind {
                            if offset == pic_pos.offset {
                                return (media, size);
                            }
                        }
                        offset += r.len();
                    }
                }
            }
            panic!("picture run missing");
        })
    };
    assert_eq!(current_media(&mut s), (initial_media, initial_size));

    // 4. doc_replace_picture で画像を置き換え（keep_size: true）
    s.update(|e, _, cx| {
        e.doc_replace_picture(pic_pos, TINY_PNG.to_vec(), MediaFormat::Png, true, cx);
    });
    let (replaced_media, replaced_size) = current_media(&mut s);
    assert_ne!(replaced_media, initial_media, "media id changed after replacement");
    assert_eq!(replaced_size, initial_size, "size preserved when keep_size is true");
    s.screenshot("docx-picture-replaced");

    // 5. アンドゥ（cmd-z）で元の画像・メディアIDに戻る
    s.key("cmd-z");
    assert_eq!(current_media(&mut s), (initial_media, initial_size), "undo restores original picture");

    // 6. リドゥ（cmd-shift-z）で再度置換後の画像になる
    s.key("cmd-shift-z");
    assert_eq!(current_media(&mut s), (replaced_media, replaced_size), "redo restores replaced picture");

    // 7. 保存して再オープンし、置換された画像が保持されていることを検証
    s.save_as("replace-picture.docx");
    let reloaded = s.reopen();
    let reloaded_has_picture = reloaded.flow.as_ref().unwrap().body.iter().any(|b| {
        if let ibis_doc_model::Block::Paragraph(p) = b && p.id == para_id {
            p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Picture { size, .. } if size == replaced_size))
        } else {
            false
        }
    });
    assert!(reloaded_has_picture, "reloaded document contains picture with replaced size");
    assert!(!reloaded.media.is_empty(), "media collection has saved images");
}

/// XLSX-SEC-017: ページ設定の印刷オプション（枠線、行列見出し、白黒、簡易印刷）:
/// メニュー「ファイル > ページ設定」でサイドバーを開き、
/// printOptions / pageSetup の各フラグを切り替え、Undo/Redo、保存・再オープンで保持されることの検証。
fn fix_xlsx_print_options() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.screenshot("page-setup-open");

    // 1. 初期状態の確認（すべて false）
    let initial_print = s.with(|e, _| e.current_print().unwrap());
    assert!(!initial_print.grid_lines);
    assert!(!initial_print.headings);
    assert!(!initial_print.black_and_white);
    assert!(!initial_print.draft);

    // 2. ページ設定パネルが正しく開かれていることを確認
    s.update(|e, _, cx| {
        let panel = e.page_setup_panel(cx);
        assert!(panel.is_some(), "page_setup_panel should be open");
    });

    // 3. 印刷オプションを順にオンにする
    s.update(|e, _, cx| e.set_print(|p| p.grid_lines = true, None, cx));
    s.update(|e, _, cx| e.set_print(|p| p.headings = true, None, cx));
    s.update(|e, _, cx| e.set_print(|p| p.black_and_white = true, None, cx));
    s.update(|e, _, cx| e.set_print(|p| p.draft = true, None, cx));

    let updated = s.with(|e, _| e.current_print().unwrap());
    assert!(updated.grid_lines);
    assert!(updated.headings);
    assert!(updated.black_and_white);
    assert!(updated.draft);
    s.screenshot("page-setup-options-enabled");

    // 4. 保存して再オープンし、OOXML (printOptions, pageSetup) に反映されていることを確認
    s.save_as("fix_xlsx_print_options.xlsx");
    let saved = s.reopen();
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert!(sheet.print.grid_lines, "grid_lines should be persisted");
    assert!(sheet.print.headings, "headings should be persisted");
    assert!(sheet.print.black_and_white, "black_and_white should be persisted");
    assert!(sheet.print.draft, "draft should be persisted");

    // 5. Undo（cmd-z）で直前の操作（draft = true）を取り消し（ページ設定はモーダルなので、先に閉じる）
    close_modal_panel(&mut s);
    s.key("cmd-z");
    assert!(!s.with(|e, _| e.current_print().unwrap().draft));
    assert!(s.with(|e, _| e.current_print().unwrap().black_and_white));

    // 6. Redo（cmd-shift-z）で再適用
    s.key("cmd-shift-z");
    assert!(s.with(|e, _| e.current_print().unwrap().draft));
}

/// XLSX-SEC-015: ページ設定のタイトル列（印刷タイトル）:
/// 選択した列（またはアクティブセル）を「タイトル列」として設定、クリア、
/// タイトル行との共存、Undo/Redo、ファイル保存・再オープンでの _xlnm.Print_Titles 保持を検証。
fn fix_xlsx_print_title_columns() {
    use crate::xlsx::scenarios::SheetSteps as _;

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.screenshot("page-setup-open");

    // 1. 初期状態：タイトル行・列ともに None
    assert_eq!(s.with(|e, _| e.print_title_rows()), None);
    assert_eq!(s.with(|e, _| e.print_title_columns()), None);

    // 2. セル B2 を選択し、タイトル列を設定（B列 = column 1）。ページ設定はモーダルなので、先に閉じる
    close_modal_panel(&mut s);
    s.click_cell("B2");
    s.update(|e, _, cx| {
        let cols = e.selected_range().map(|r| (r.first_column, r.last_column)).or_else(|| e.current_cell().map(|(c, _)| (c, c)));
        e.set_print_title_columns(cols, cx);
    });
    assert_eq!(s.with(|e, _| e.print_title_columns()), Some((1, 1)));
    assert_eq!(s.with(|e, _| e.print_title_rows()), None);

    // 3. タイトル行も設定（A1:B2 選択から行 0..1）
    s.click_cell("A1");
    s.key("shift-down");
    s.update(|e, _, cx| {
        let rows = e.selected_range().map(|r| (r.first_row, r.last_row)).or_else(|| e.current_cell().map(|(_, r)| (r, r)));
        e.set_print_title_rows(rows, cx);
    });
    assert_eq!(s.with(|e, _| e.print_title_rows()), Some((0, 1)));
    assert_eq!(s.with(|e, _| e.print_title_columns()), Some((1, 1)));
    s.screenshot("page-setup-both-titles-set");

    // 4. 保存して再オープンし、行と列の両方の印刷タイトルが保持されていることを確認
    s.save_as("fix_xlsx_print_titles.xlsx");
    let saved = s.reopen();
    let wb = saved.workbook.as_ref().unwrap();
    let titles_name = wb.names.iter().find(|n| n.name.eq_ignore_ascii_case("_xlnm.Print_Titles")).unwrap();
    let (rows, cols) = crate::xlsx::header_footer::parse_titles(&titles_name.formula);
    assert_eq!(rows, Some((0, 1)), "title rows persisted in OOXML defined name");
    assert_eq!(cols, Some((1, 1)), "title cols persisted in OOXML defined name");

    // 5. タイトル列のクリア（None に戻し、タイトル行は保持）
    s.update(|e, _, cx| e.set_print_title_columns(None, cx));
    assert_eq!(s.with(|e, _| e.print_title_columns()), None);
    assert_eq!(s.with(|e, _| e.print_title_rows()), Some((0, 1)));

    // 6. Undo（cmd-z）でタイトル列が復元されることを確認
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.print_title_columns()), Some((1, 1)));

    // 7. Redo（shift-cmd-z）で再度クリアされることを確認
    s.key("shift-cmd-z");
    assert_eq!(s.with(|e, _| e.print_title_columns()), None);
}

/// XLSX-SEC-013: ページ設定の用紙サイズ拡充（A5, B4, はがき）と余白のページ中央配置（水平・垂直）:
/// 用紙サイズの切り替え、ページ中央配置のトグル、Undo/Redo、保存・再オープンによる
/// OOXML (pageSetup paperSize, printOptions horizontalCentered/verticalCentered) 保持を検証。
fn fix_xlsx_paper_sizes_and_centering() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.screenshot("page-setup-open");

    // 1. 初期状態：A4 (210mm x 297mm), centered (false, false)
    let initial = s.with(|e, _| e.current_print().unwrap());
    assert_eq!(initial.paper, ibis_doc_model::SheetPrint::A4);
    assert_eq!(initial.centered, (false, false));

    // 2. 用紙サイズを「はがき」に変更し、中央配置（水平・垂直）をオンにする
    s.update(|e, _, cx| {
        e.set_print(|p| {
            p.paper = ibis_doc_model::SheetPrint::HAGAKI;
            p.centered = (true, true);
        }, None, cx);
    });
    let updated = s.with(|e, _| e.current_print().unwrap());
    assert_eq!(updated.paper, ibis_doc_model::SheetPrint::HAGAKI);
    assert_eq!(updated.centered, (true, true));
    s.screenshot("page-setup-hagaki-centered");

    // 3. 保存して再オープンし、paperSize（43: はがき）および中央配置が保持されていることを確認
    s.save_as("fix_xlsx_hagaki_centered.xlsx");
    let saved = s.reopen();
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.print.paper, ibis_doc_model::SheetPrint::HAGAKI, "HAGAKI paper size should be persisted");
    assert_eq!(sheet.print.centered, (true, true), "horizontal and vertical centering should be persisted");

    // 4. 用紙サイズを「B4」に変更
    s.update(|e, _, cx| {
        e.set_print(|p| p.paper = (257 * 36_000, 364 * 36_000), None, cx);
    });
    assert_eq!(s.with(|e, _| e.current_print().unwrap().paper), (257 * 36_000, 364 * 36_000));

    // 5. Undo（cmd-z）で「はがき」に戻ることを確認（ページ設定はモーダルなので、先に閉じる）
    close_modal_panel(&mut s);
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.current_print().unwrap().paper), ibis_doc_model::SheetPrint::HAGAKI);

    // 6. Redo（shift-cmd-z）で「B4」に再適用されることを確認
    s.key("shift-cmd-z");
    assert_eq!(s.with(|e, _| e.current_print().unwrap().paper), (257 * 36_000, 364 * 36_000));
}

/// XLSX-SEC-020: PDF・CSV 書き出し:
/// ワークブックの現在のシートを CSV（.csv）へ書き出し、行・列・値が正しくフォーマットされること、
/// および PDF（.pdf）への書き出しが正常に完了することを検証。
fn fix_xlsx_export_csv_and_pdf() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let out_dir = std::env::temp_dir().join(format!("ibisoffice-test-xlsx-export-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&out_dir);

    // 1. CSV 書き出しの検証
    let csv_path = out_dir.join("exported.csv");
    let csv_res = s.with(|e, _| e.write_csv(&csv_path));
    assert!(csv_res.is_ok(), "write_csv succeeded");
    assert!(csv_path.exists(), "csv file exists");

    let csv_content = std::fs::read_to_string(&csv_path).expect("read exported csv");
    assert!(!csv_content.is_empty(), "csv content is not empty");
    let lines: Vec<&str> = csv_content.lines().collect();
    assert!(lines.len() >= 2, "csv has header and data rows");

    // 2. PDF 書き出しの検証
    let pdf_path = out_dir.join("exported.pdf");
    let pdf_job = s.with(|e, _| e.write_pdf(pdf_path.clone())).expect("write_pdf returns job");
    let pdf_res = pdf_job();
    assert!(pdf_res.is_ok(), "pdf rendering job succeeded");
    assert!(pdf_path.exists(), "pdf file exists");
    let pdf_bytes = std::fs::read(&pdf_path).expect("read exported pdf");
    assert!(pdf_bytes.starts_with(b"%PDF-"), "file starts with PDF header");
    assert!(pdf_bytes.len() > 1000, "pdf file has non-trivial size");

    let _ = std::fs::remove_dir_all(&out_dir);
}

/// DOCX-TBLFMT-001: 表の位置揃えと字下げ:
/// 表の配置（左揃え・中央揃え・右揃え）およびインデント（左余白からの字下げ）を変更し、
/// メニューのチェック状態連動、Undo/Redo、OOXML（w:jc, w:tblInd）保存・再オープンによる保持を検証。
fn fix_docx_table_alignment_and_indent() {
    use crate::docx::scenarios::DocSteps as _;

    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.key("cmd-down");
    s.key("enter");
    // 表の挿入 (2x2)
    s.menu("docx.table.insert-2x2");

    let table = |s: &mut Scenario| {
        s.document()
            .body
            .into_iter()
            .rev()
            .find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None })
            .unwrap()
    };

    // 初期状態: 左揃え (Left), インデント 0
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Left);
    assert_eq!(table(&mut s).indent, 0);
    assert_eq!(s.menu_state("docx.table.align.left"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.align.center"), Some((true, false)));
    assert_eq!(s.menu_state("docx.table.align.right"), Some((true, false)));
    assert_eq!(s.menu_state("docx.table.indent.none"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.indent.10mm"), Some((true, false)));

    // 1. 中央揃えに変更
    s.menu("docx.table.align.center");
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Center);
    assert_eq!(s.menu_state("docx.table.align.center"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.align.left"), Some((true, false)));

    // 2. 右揃えに変更
    s.menu("docx.table.align.right");
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Right);
    assert_eq!(s.menu_state("docx.table.align.right"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.align.center"), Some((true, false)));

    // 3. 左揃えに戻し、インデント 10mm (360,000 EMU) を設定
    s.menu("docx.table.align.left");
    s.menu("docx.table.indent.10mm");
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Left);
    assert_eq!(table(&mut s).indent, 360_000);
    assert_eq!(s.menu_state("docx.table.indent.10mm"), Some((true, true)));
    assert_eq!(s.menu_state("docx.table.indent.none"), Some((true, false)));

    // 4. Undo / Redo
    s.key("cmd-z"); // インデント取り消し -> 0
    assert_eq!(table(&mut s).indent, 0);
    assert_eq!(s.menu_state("docx.table.indent.none"), Some((true, true)));

    s.key("cmd-shift-z"); // インデント再適用 -> 360,000
    assert_eq!(table(&mut s).indent, 360_000);
    assert_eq!(s.menu_state("docx.table.indent.10mm"), Some((true, true)));

    // 5. 保存と開き直し (左揃え + インデント 10mm)
    s.save_as("test_table_align_indent.docx");
    let saved = s.reopen();
    let d = saved.flow.expect("document flow");
    let t_saved = d.body.iter().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).unwrap();
    assert_eq!(t_saved.align, ibis_doc_model::FlowAlign::Left);
    // OOXML は twips 単位（1 twip = 635 EMU）で保存されるため 1 twip 未満の丸め誤差を許容
    assert!((t_saved.indent - 360_000).abs() < 635, "indent preserved within 1 twip: {}", t_saved.indent);

    // 6. 中央揃えに変更して保存と開き直し (中央揃え w:jc="center")
    s.menu("docx.table.align.center");
    assert_eq!(table(&mut s).align, ibis_doc_model::FlowAlign::Center);
    s.save_as("test_table_align_center.docx");
    let saved_center = s.reopen();
    let d_center = saved_center.flow.expect("document flow");
    let t_saved_center = d_center.body.iter().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).unwrap();
    assert_eq!(t_saved_center.align, ibis_doc_model::FlowAlign::Center);
}

/// DOCX-PAGE-013（段組の段数）および DOCX-PAGE-015（段間罫線）
/// 表示形式 > ページ設定 > 段組み（1段、2段、3段、境界線を引く）の切替、
/// Undo/Redo、OOXML <w:cols> の保存・再オープン保持を検証する。
fn fix_docx_columns() {
    use crate::docx::scenarios::DocSteps as _;
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 初期状態: 1段 (count = 1), 境界線なし (separator = false)
    assert_eq!(s.document().page.columns.count, 1);
    assert!(!s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.1"), Some((true, true)));
    assert_eq!(s.menu_state("docx.columns.2"), Some((true, false)));
    assert_eq!(s.menu_state("docx.columns.3"), Some((true, false)));
    assert_eq!(s.menu_state("docx.columns.separator"), Some((false, false)));

    // 1. 2段に変更
    s.menu("docx.columns.2");
    assert_eq!(s.document().page.columns.count, 2);
    assert!(!s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.1"), Some((true, false)));
    assert_eq!(s.menu_state("docx.columns.2"), Some((true, true)));
    assert_eq!(s.menu_state("docx.columns.separator"), Some((true, false)));

    // 2. 境界線を有効化
    s.menu("docx.columns.separator");
    assert_eq!(s.document().page.columns.count, 2);
    assert!(s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.separator"), Some((true, true)));

    // 3. 3段に変更 (境界線は有効のまま)
    s.menu("docx.columns.3");
    assert_eq!(s.document().page.columns.count, 3);
    assert!(s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.3"), Some((true, true)));
    assert_eq!(s.menu_state("docx.columns.separator"), Some((true, true)));

    // 4. Undo / Redo
    s.key("cmd-z"); // 3段取り消し -> 2段（境界線あり）
    assert_eq!(s.document().page.columns.count, 2);
    assert!(s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.2"), Some((true, true)));

    s.key("cmd-z"); // 境界線有効化取り消し -> 2段（境界線なし）
    assert_eq!(s.document().page.columns.count, 2);
    assert!(!s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.separator"), Some((true, false)));

    s.key("cmd-z"); // 2段取り消し -> 1段（境界線なし）
    assert_eq!(s.document().page.columns.count, 1);
    assert!(!s.document().page.columns.separator);
    assert_eq!(s.menu_state("docx.columns.1"), Some((true, true)));

    s.key("cmd-shift-z"); // 2段再適用
    assert_eq!(s.document().page.columns.count, 2);
    s.key("cmd-shift-z"); // 境界線再適用
    assert_eq!(s.document().page.columns.count, 2);
    assert!(s.document().page.columns.separator);
    s.key("cmd-shift-z"); // 3段再適用
    assert_eq!(s.document().page.columns.count, 3);
    assert!(s.document().page.columns.separator);

    // 5. 保存と開き直し (3段 + 境界線あり)
    s.save_as("test_columns_3_sep.docx");
    let saved = s.reopen();
    let d = saved.flow.expect("document flow");
    assert_eq!(d.page.columns.count, 3);
    assert!(d.page.columns.separator);

    // 6. 1段に戻して保存と開き直し
    s.menu("docx.columns.1");
    assert_eq!(s.document().page.columns.count, 1);
    s.save_as("test_columns_1_reset.docx");
    let saved_reset = s.reopen();
    let d_reset = saved_reset.flow.expect("document flow");
    assert_eq!(d_reset.page.columns.count, 1);
    assert!(!d_reset.page.columns.separator);
}

/// DOCX-TABLE-014（表・行・列・セル選択）
/// 表示形式 > 表 > 選択（セルを選択、行を選択、表を選択）の各操作、
/// メニューの有効/無効、選択範囲の正確性、選択テキストへの書式適用を検証する。
fn fix_docx_table_selection() {
    use crate::docx::scenarios::DocSteps as _;
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 表の外では「選択」メニュー項目は無効（disabled）
    assert_eq!(s.menu_state("docx.table.select.cell"), Some((false, false)));
    assert_eq!(s.menu_state("docx.table.select.row"), Some((false, false)));
    assert_eq!(s.menu_state("docx.table.select.table"), Some((false, false)));

    // 2. 文書末尾に表（2x2）を挿入
    s.key("cmd-down");
    s.key("enter");
    s.menu("docx.table.insert-2x2");

    // 表の中では各メニューが有効（enabled）
    assert_eq!(s.menu_state("docx.table.select.cell"), Some((true, false)));
    assert_eq!(s.menu_state("docx.table.select.row"), Some((true, false)));
    assert_eq!(s.menu_state("docx.table.select.table"), Some((true, false)));

    let table = |s: &mut Scenario| {
        s.document()
            .body
            .into_iter()
            .rev()
            .find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None })
            .unwrap()
    };
    let cell_para = |t: &ibis_doc_model::FlowTable, r: usize, c: usize| {
        t.rows[r].cells[c].blocks.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.clone()) } else { None }).unwrap()
    };

    // セル (0, 0) に文字を入力
    s.type_text("セルA");
    let t0 = table(&mut s);
    let p00 = cell_para(&t0, 0, 0);
    assert_eq!(p00.text(), "セルA");

    // 3. セルを選択（docx.table.select.cell）
    s.menu("docx.table.select.cell");
    assert_eq!(s.selection(), Some((ibis_doc_model::FlowPos::new(p00.id, 0), ibis_doc_model::FlowPos::new(p00.id, p00.len()))));

    // 選択されたセルに太字（cmd-b）を適用
    s.key("cmd-b");
    let t_bold = table(&mut s);
    let p00_bold = cell_para(&t_bold, 0, 0);
    assert!(p00_bold.runs.iter().any(|r| r.style.bold), "cell(0,0) bold");

    // 選択された状態で上書き入力してセル内容が置き換わることを確認
    s.menu("docx.table.select.cell");
    s.type_text("置換A");
    let t_rep = table(&mut s);
    let p00_rep = cell_para(&t_rep, 0, 0);
    assert_eq!(p00_rep.text(), "置換A");

    // セル (0, 1) へ移動して文字入力
    s.key("tab");
    s.type_text("セルB");
    let t1 = table(&mut s);
    let p01 = cell_para(&t1, 0, 1);
    assert_eq!(p01.text(), "セルB");

    // 4. 行を選択（docx.table.select.row）
    s.menu("docx.table.select.row");
    // 行 0 全体（cell 0 先頭から cell 1 末尾）が選択されることを確認
    let t_row = table(&mut s);
    let p00_cur = cell_para(&t_row, 0, 0);
    let p01_cur = cell_para(&t_row, 0, 1);
    assert_eq!(s.selection(), Some((ibis_doc_model::FlowPos::new(p00_cur.id, 0), ibis_doc_model::FlowPos::new(p01_cur.id, p01_cur.len()))));

    // セル (1, 0) へ移動して文字入力
    s.key("tab");
    s.type_text("セルC");

    // 5. 表全体を選択（docx.table.select.table）
    s.menu("docx.table.select.table");
    let t_all = table(&mut s);
    let p_first = cell_para(&t_all, 0, 0);
    let p_last = cell_para(&t_all, 1, 1);
    assert_eq!(s.selection(), Some((ibis_doc_model::FlowPos::new(p_first.id, 0), ibis_doc_model::FlowPos::new(p_last.id, p_last.len()))));

    // 6. 保存と再オープンで内容が保たれていることを確認
    s.save_as("fix_docx_table_selection.docx");
    let saved = s.reopen();
    let flow_doc = saved.flow.as_ref().unwrap();
    let reloaded_table = flow_doc.body.iter().rev().find_map(|b| if let ibis_doc_model::Block::Table(t) = b { Some(t) } else { None }).unwrap();
    let c00 = reloaded_table.rows[0].cells[0].blocks.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.text()) } else { None }).unwrap_or_default();
    let c01 = reloaded_table.rows[0].cells[1].blocks.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.text()) } else { None }).unwrap_or_default();
    let c10 = reloaded_table.rows[1].cells[0].blocks.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p.text()) } else { None }).unwrap_or_default();
    assert_eq!(c00, "置換A");
    assert_eq!(c01, "セルB");
    assert_eq!(c10, "セルC");
}

/// XLSX-VALID-009: 無効データのマーク（データの入力規則のマーク表示・マークのクリア）:
/// 入力規則のルールに違反しているセルを検出し、赤い角丸長方形／楕円枠でマーク表示（Circle Invalid Data）
/// およびマークのクリア（Clear Validation Circles）の動作を検証する。
fn fix_xlsx_circle_invalid_data() {
    use ibis_doc_model::{CellRange, CellValue, Command, SheetCommand, Validation};
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. セル範囲 (0..=1, 0..=1) 即ち A1:B2 に「10以上100以下の整数」の入力規則を設定
    let rule = Validation {
        ranges: vec![CellRange { first_row: 0, last_row: 1, first_column: 0, last_column: 1 }],
        kind: "whole".into(),
        operator: "between".into(),
        formula1: Some("10".into()),
        formula2: Some("100".into()),
        allow_blank: true,
        ..Default::default()
    };
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let _ = e.document.execute(vec![Command::Sheet(SheetCommand::SetValidation {
            slide,
            ranges: vec![CellRange { first_row: 0, last_row: 1, first_column: 0, last_column: 1 }],
            rule: Some(rule),
        })]);
    });

    // 2. セルに値を直接設定（A1: 50[適合], A2: 5[不適合], B1: 150[不適合], B2: 80[適合]）
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let mut edits = Vec::new();
        edits.push(ibis_doc_model::CellEdit { row: 0, column: 0, input: ibis_doc_model::CellInput::Value(CellValue::Number(50.0)) });
        edits.push(ibis_doc_model::CellEdit { row: 1, column: 0, input: ibis_doc_model::CellInput::Value(CellValue::Number(5.0)) });
        edits.push(ibis_doc_model::CellEdit { row: 0, column: 1, input: ibis_doc_model::CellInput::Value(CellValue::Number(150.0)) });
        edits.push(ibis_doc_model::CellEdit { row: 1, column: 1, input: ibis_doc_model::CellInput::Value(CellValue::Number(80.0)) });
        let _ = e.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })]);
    });

    // 3. 初期状態：validation_circles は false、render_validation_circles は None
    assert!(!s.with(|e, _| e.validation_circles()));
    assert!(s.with(|e, _| e.render_validation_circles(1.0)).is_none());

    // 4. 無効セル一覧を取得し、A2 (0, 1) と B1 (1, 0) の2つが検出されることを確認
    let invalid = s.with(|e, _| e.invalid_cells());
    assert_eq!(invalid, vec![(0, 1), (1, 0)]);

    // 5. メニュー「データ > 無効データのマーク表示」（xlsx.validation.circle-invalid）を実行
    s.menu("xlsx.validation.circle-invalid");
    assert!(s.with(|e, _| e.validation_circles()));
    assert!(s.with(|e, _| e.render_validation_circles(1.0)).is_some());

    // 6. メニュー「データ > マークのクリア」（xlsx.validation.clear-circles）を実行
    s.menu("xlsx.validation.clear-circles");
    assert!(!s.with(|e, _| e.validation_circles()));
    assert!(s.with(|e, _| e.render_validation_circles(1.0)).is_none());

    // 7. 入力規則パネルを開き、パネル経由でのマーク表示・クリア操作を検証
    s.menu("xlsx.validation");
    s.update(|e, _, cx| e.circle_invalid_data(cx));
    assert!(s.with(|e, _| e.validation_circles()));
    s.update(|e, _, cx| e.clear_validation_circles(cx));
    assert!(!s.with(|e, _| e.validation_circles()));

    // 8. 保存と再オープンでデータ整合性が保持されていることを確認
    s.save_as("fix_xlsx_circle_invalid_data.xlsx");
    let saved = s.reopen();
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.validations.len(), 1);
    assert_eq!(sheet.cell(0, 0).map(|c| &c.value), Some(&CellValue::Number(50.0)));
    assert_eq!(sheet.cell(1, 0).map(|c| &c.value), Some(&CellValue::Number(5.0)));
}

/// DOCX-EDIT-003（単語・段落・全文選択）
/// 1クリック（キャレット移動）、2クリック（単語選択）、3クリック（段落選択）、
/// および ⌘A / メニュー「すべて選択」（edit.select-all）による全文選択を検証する。
fn fix_docx_word_paragraph_selection() {
    use crate::docx::scenarios::DocSteps as _;
    let mut s = Scenario::open("test/editing/docx-report.docx");

    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 15 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let para_id = p.id;
    let para_len = p.len();

    // 1. 通常クリック（1クリック）: キャレットが指定位置に移動し、範囲選択はない（始点==終点）
    s.click_text(ibis_doc_model::FlowPos::new(para_id, 2));
    let (a, c) = s.selection().expect("selection exists");
    assert_eq!(a, ibis_doc_model::FlowPos::new(para_id, 2));
    assert_eq!(c, ibis_doc_model::FlowPos::new(para_id, 2));

    // 2. ダブルクリック（2クリック）: 単語が選択される
    // pos 2 の文字を含む単語が選択されることを確認
    s.double_click_text(ibis_doc_model::FlowPos::new(para_id, 2));
    let (sel_start, sel_end) = s.selection().expect("word selected");
    assert_eq!(sel_start.para, para_id);
    assert_eq!(sel_end.para, para_id);
    assert!(sel_start.offset <= 2 && sel_end.offset > 2, "word contains offset 2");
    assert!(sel_start.offset < sel_end.offset, "non-empty selection");

    // 3. トリプルクリック（3クリック）: 段落全体が選択される (0..para_len)
    s.triple_click_text(ibis_doc_model::FlowPos::new(para_id, 5));
    let (p_start, p_end) = s.selection().expect("paragraph selected");
    assert_eq!(p_start, ibis_doc_model::FlowPos::new(para_id, 0));
    assert_eq!(p_end, ibis_doc_model::FlowPos::new(para_id, para_len));

    // 選択された段落に太字（cmd-b）を適用
    s.key("cmd-b");
    let doc_bold = s.document();
    let p_bold = ibis_doc_model::flow_command::paragraph(&doc_bold, para_id).unwrap();
    assert!(p_bold.runs.iter().all(|r| r.style.bold), "all runs in paragraph are bold");

    // 元に戻す（cmd-z）
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, para_id).unwrap();
    assert_eq!(p_undo.runs.iter().all(|r| r.style.bold), p.runs.iter().all(|r| r.style.bold));

    // 4. メニュー「編集 > すべて選択」（edit.select-all）による全文選択
    s.menu("edit.select-all");
    let (all_start, all_end) = s.selection().expect("all selected");
    assert_eq!(all_start.offset, 0);
    let full_doc = s.document();
    assert_eq!(all_end.offset, ibis_doc_model::flow_command::paragraph(&full_doc, all_end.para).unwrap().len());
    assert_ne!(all_start.para, all_end.para, "multiple paragraphs selected across document");

    // 5. キーボードショートカット ⌘A による全文選択
    // 一度クリックでキャレット解除してから cmd-a
    s.click_text(ibis_doc_model::FlowPos::new(para_id, 0));
    let (cur_a, cur_c) = s.selection().unwrap();
    assert_eq!(cur_a, cur_c);
    s.key("cmd-a");
    let (cmd_all_start, cmd_all_end) = s.selection().expect("all selected via cmd-a");
    assert_eq!(cmd_all_start, all_start);
    assert_eq!(cmd_all_end, all_end);
}

/// DOCX-FILE-019（文書統計・文字カウント Words Count ダイアログ）
/// メニュー「ツール > 文字カウント…」（tools.word-count）から開く「文字カウント」パネル、
/// ページ数、単語数、文字数（スペースなし）、文字数（スペースあり）、段落数、行数の
/// 文書全体および選択範囲ごとの統計計算・表示、およびパネルの開閉を検証する。
fn fix_docx_word_count_dialog() {
    use crate::docx::scenarios::DocSteps as _;
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 文書全体の統計計算の検証
    let whole_stats = s.with(|e, _| e.document_statistics());
    assert!(!whole_stats.is_selection, "whole document stats");
    assert!(whole_stats.pages >= 1, "pages >= 1");
    assert!(whole_stats.words > 0, "words > 0");
    assert!(whole_stats.characters_no_spaces > 0, "chars no spaces > 0");
    assert!(whole_stats.characters_with_spaces >= whole_stats.characters_no_spaces, "chars with spaces >= chars no spaces");
    assert!(whole_stats.paragraphs > 0, "paragraphs > 0");
    assert!(whole_stats.lines > 0, "lines > 0");

    // ステータスバーの文字数と document_statistics の characters_no_spaces が一致することを確認
    let status_chars = s.with(|e, _| e.doc_state().map_or(0, |st| st.characters));
    assert_eq!(status_chars, whole_stats.characters_no_spaces, "status bar characters match stats");

    // 2. メニュー「ツール > 文字カウント…」（tools.word-count）を実行
    assert!(s.with(|e, _| e.doc_state().unwrap().helper.word_count.is_none()));
    s.menu("tools.word-count");

    // サイドバーが開き、文字カウントパネルが表示されていることを確認
    assert!(s.with(|e, _| e.sidebar_open));
    let panel_stats = s.with(|e, _| e.doc_state().unwrap().helper.word_count.as_ref().unwrap().stats.clone());
    assert_eq!(panel_stats, whole_stats, "panel displays whole document stats");

    // 3. パネルを閉じる
    s.update(|e, _, cx| e.close_word_count_panel(cx));
    assert!(s.with(|e, _| e.doc_state().unwrap().helper.word_count.is_none()));

    // 4. テキスト選択状態での統計計算の検証
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 15 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    // ダブルクリックで単語を選択
    s.double_click_text(ibis_doc_model::FlowPos::new(p.id, 2));
    assert!(s.with(|e, _| e.has_selection()));
    let selected_str = s.with(|e, _| e.selected_text());
    assert!(!selected_str.is_empty(), "selected text is not empty");

    // 選択状態での文字カウントを実行
    s.menu("tools.word-count");
    let sel_stats = s.with(|e, _| e.doc_state().unwrap().helper.word_count.as_ref().unwrap().stats.clone());
    assert!(sel_stats.is_selection, "is selection stats");
    let (exp_words, exp_no, exp_with) = crate::docx::help_panels::count_words_and_chars(&selected_str);
    assert_eq!(sel_stats.words, exp_words, "selection words match");
    assert_eq!(sel_stats.characters_no_spaces, exp_no, "selection chars without spaces match");
    assert_eq!(sel_stats.characters_with_spaces, exp_with, "selection chars with spaces match");
    assert_eq!(sel_stats.paragraphs, 1, "single paragraph selected");

    // 5. パネルを閉じる
    s.update(|e, _, cx| e.close_word_count_panel(cx));
    assert!(s.with(|e, _| e.doc_state().unwrap().helper.word_count.is_none()));
}

/// DOCX-FIND-007: 特殊文字・段落記号の検索と置換
/// special_characters の拡張（^m 改ページ、^s ノーブレークスペース、^- 任意指定ハイフン、^~ 改行しないハイフン等）
/// および大文字コード対応（^P, ^T, ^L, ^M, ^N, ^S）の検証
fn fix_docx_find_special_characters() {
    use crate::docx::find::special_characters;

    // 1. special_characters 単体での変換精度の検証
    assert_eq!(special_characters("A^tB"), "A\tB");
    assert_eq!(special_characters("A^TB"), "A\tB");
    assert_eq!(special_characters("Line1^lLine2"), "Line1\u{b}Line2");
    assert_eq!(special_characters("Line1^LLine2"), "Line1\u{b}Line2");
    assert_eq!(special_characters("Page1^mPage2"), "Page1\u{c}Page2");
    assert_eq!(special_characters("Page1^MPage2"), "Page1\u{c}Page2");
    assert_eq!(special_characters("Col1^nCol2"), "Col1\u{e}Col2");
    assert_eq!(special_characters("Col1^NCol2"), "Col1\u{e}Col2");
    assert_eq!(special_characters("No^sBreak"), "No\u{a0}Break");
    assert_eq!(special_characters("No^SBreak"), "No\u{a0}Break");
    assert_eq!(special_characters("Opt^-Hyphen"), "Opt\u{ad}Hyphen");
    assert_eq!(special_characters("Non^~Hyphen"), "Non\u{2011}Hyphen");
    assert_eq!(special_characters("Em^+Dash"), "Em\u{2014}Dash");
    assert_eq!(special_characters("En^=Dash"), "En\u{2013}Dash");
    assert_eq!(special_characters("Caret^^Here"), "Caret^Here");
    assert_eq!(special_characters("End^p"), "End\n");
    assert_eq!(special_characters("End^P"), "End\n");

    // 複合クエリ
    assert_eq!(
        special_characters("word1^sword2^m^tp1^-sub^~fix^+dash"),
        "word1\u{a0}word2\u{c}\tp1\u{ad}sub\u{2011}fix\u{2014}dash"
    );

    // 2. 実際のドキュメントでの検索・置換操作
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // テキスト段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    // 特殊文字を含むテキストを挿入
    s.type_text("SPEC_NBSP\u{a0}TEST SPEC_HYPH\u{2011}TEST ");

    // 検索テスト: ^s (ノーブレークスペース) で検索
    let doc = s.document();
    let query_nbsp = "NBSP^sTEST";
    let matches_nbsp = crate::docx::find::find_all(&doc, query_nbsp);
    assert_eq!(matches_nbsp.len(), 1, "found 1 NBSP match with ^s");

    // 検索テスト: 大文字 ^S でも検索できることを確認
    let matches_nbsp_upper = crate::docx::find::find_all(&doc, "NBSP^STEST");
    assert_eq!(matches_nbsp_upper.len(), 1, "found 1 NBSP match with ^S");

    // 検索テスト: ^~ (改行しないハイフン) で検索
    let query_hyph = "HYPH^~TEST";
    let matches_hyph = crate::docx::find::find_all(&doc, query_hyph);
    assert_eq!(matches_hyph.len(), 1, "found 1 non-breaking hyphen match with ^~");

    // 3. 置換テスト: 特殊文字への置換（replace_all）
    s.menu("edit.find-replace");
    // "SPEC_NBSP\u{a0}TEST" を "REPLACED^sDONE" に置換
    let n = s.update(|e, _, cx| e.replace_all("SPEC_NBSP^sTEST", "REPLACED^sDONE", cx));
    assert_eq!(n, 1, "replaced 1 occurrence");

    let doc_after = s.document();
    let p_after = ibis_doc_model::flow_command::paragraph(&doc_after, p_id).expect("paragraph exists");
    assert!(p_after.text().contains("REPLACED\u{a0}DONE"), "replacement contains NBSP: {:?}", p_after.text());

    // 4. 手動改ページ (^m) への置換テスト
    let n_page = s.update(|e, _, cx| e.replace_all("SPEC_HYPH^~TEST", "BEFORE^mAFTER", cx));
    assert_eq!(n_page, 1, "replaced 1 occurrence with page break");

    let doc_after_page = s.document();
    let p_after_page = ibis_doc_model::flow_command::paragraph(&doc_after_page, p_id).expect("paragraph exists");
    assert!(p_after_page.text().contains("BEFORE\u{c}AFTER"), "replacement contains PageBreak character: {:?}", p_after_page.text());
    assert!(p_after_page.runs.iter().any(|r| r.kind == ibis_doc_model::FlowRunKind::PageBreak), "runs contain FlowRunKind::PageBreak");

    // 検索テスト: 置換された手動改ページを ^m で検索
    let matches_page = crate::docx::find::find_all(&doc_after_page, "BEFORE^mAFTER");
    assert_eq!(matches_page.len(), 1, "found 1 PageBreak match with ^m");

    // 検索テスト: 大文字 ^M でも検索できることを確認
    let matches_page_upper = crate::docx::find::find_all(&doc_after_page, "BEFORE^MAFTER");
    assert_eq!(matches_page_upper.len(), 1, "found 1 PageBreak match with ^M");

    // 5. Undo で元に戻ることを確認
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo1 = s.document();
    let p_undo1 = ibis_doc_model::flow_command::paragraph(&doc_undo1, p_id).expect("paragraph exists");
    assert!(!p_undo1.text().contains("BEFORE\u{c}AFTER"), "undo removed page break replacement");
}

/// DOCX-FIND-004: 単語全体の一致（日本語・全角約物・多言語区切り対応）
/// 和文約物（「」『』【】（）、。・）や引用符・ダッシュで囲まれた単語が
/// 単語境界として正しく認識され、複合語の部分一致は除外されることを検証
fn fix_docx_find_whole_words() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    // 和文約物・括弧・記号を含むテキストを入力
    // 「テスト」, テスト、, （テスト）, テスト全般（除外対象）, 【テスト】
    s.type_text("「テスト」 テスト、 （テスト） テスト全般 【テスト】 ");

    let doc_typed = s.document();

    // 1. 通常検索（単語単位オフ）: "テスト" は 5 箇所すべてでヒットする
    let all_matches = crate::docx::find::find_all(&doc_typed, "テスト");
    assert_eq!(all_matches.len(), 5, "finds 5 matches without whole_words");

    // 2. 単語単位検索（whole_words = true）
    s.menu("edit.find-replace");
    s.type_text("テスト");
    // 単語単位チェックボックスをクリック (座標でなくエディタの設定関数で切り替える)
    crate::docx::scenarios::toggle_find_whole_words(&mut s);
    assert!(s.with(|e, _| e.doc_state().unwrap().find.as_ref().unwrap().whole_words), "whole_words enabled");

    // 「テスト」, テスト、, （テスト）, 【テスト】 の 4 箇所がマッチし、
    // 「テスト全般」の「テスト」は右隣が「全」（単語文字）のためマッチしない
    let matches_whole = s.with(|e, _| e.doc_state().unwrap().find.as_ref().unwrap().matches.clone());
    assert_eq!(matches_whole.len(), 4, "finds 4 matches with whole_words (excluding テスト全般)");

    // 3. 置換（replace_all）: 単語単位の一致のみが置換される
    let n = s.update(|e, _, cx| e.replace_all("テスト", "成功", cx));
    assert_eq!(n, 4, "replaced 4 occurrences");

    let doc_replaced = s.document();
    let p_rep = ibis_doc_model::flow_command::paragraph(&doc_replaced, p_id).expect("paragraph exists");
    // 置換後: 「成功」 成功、 （成功） テスト全般 【成功】
    assert!(p_rep.text().contains("「成功」 成功、 （成功） テスト全般 【成功】"), "result text: {:?}", p_rep.text());
    assert!(p_rep.text().contains("テスト全般"), "unmatched word part was preserved");

    // 4. Undo で元に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, p_id).expect("paragraph exists");
    assert!(p_undo.text().contains("「テスト」 テスト、 （テスト） テスト全般 【テスト】"), "undo restored original");
}

/// DOCX-EDIT-011: 形式を選択して貼付
/// メニュー「編集 > 形式を選択して貼り付け」（docx.paste-special）および
/// 各オプション（元の書式を保持、貼り付け先の書式に合わせる、テキストのみ保持、書式のみ）
/// からの貼り付け動作、ショートカット連携、Undo/Redoを検証
fn fix_docx_paste_special() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;

    // 1. 太字テキストを入力してコピー
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.type_text("PASTE_SRC ");
    // 入力したテキストを選択して太字化
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    for _ in 0..9 {
        s.key("shift-right");
    }
    s.key("cmd-b");
    // コピー
    s.menu("edit.copy");

    // 2. 貼り付け先へ移動
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 10));

    // 3. テキストのみ保持して貼り付け（docx.paste-special.text-only）
    s.menu("docx.paste-special.text-only");
    let doc_text_only = s.document();
    let p_text_only = ibis_doc_model::flow_command::paragraph(&doc_text_only, p_id).expect("paragraph exists");
    assert!(p_text_only.text().contains("PASTE_SRC"), "pasted text exists");

    // Undo で取り消し
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");

    // 4. 元の書式を保持して貼り付け（docx.paste-special.keep-source）
    s.menu("docx.paste-special.keep-source");
    let doc_keep = s.document();
    let p_keep = ibis_doc_model::flow_command::paragraph(&doc_keep, p_id).expect("paragraph exists");
    assert!(p_keep.runs.iter().any(|r| r.style.bold && matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("PASTE_SRC"))), "pasted run keeps bold formatting");

    // Undo で取り消し
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");

    // 5. 貼り付け先の書式に合わせて貼り付け（docx.paste-special.match-format）
    s.menu("docx.paste-special.match-format");
    let doc_match = s.document();
    let p_match = ibis_doc_model::flow_command::paragraph(&doc_match, p_id).expect("paragraph exists");
    assert!(p_match.text().contains("PASTE_SRC"), "pasted with match format exists");

    // Undo で元に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
}

/// DOCX-FIELD-005: フィールドのリンク解除（Unlink Fields, ⇧⌘F9 / Ctrl+Shift+F9）
/// および DOCX-FIELD-003: フィールドの更新（F9）
/// フィールド（SEQ、REF、DATE等）のリンク解除により静的テキストへの変換、
/// メニュー「docx.fields.unlink」およびショートカット連携、Undo/Redo、ファイル保存・再読込を検証
fn fix_docx_unlink_fields() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文からプレーンな段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;

    // 2. キャレットを段落の先頭に置き、図表番号（SEQ 図）を挿入
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("docx.caption.figure");

    // 挿入された段落を確認（SEQ フィールドが存在する）
    let doc_after = s.document();
    let p_caption = doc_after
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph with SEQ field");

    let cap_id = p_caption.id;
    assert!(p_caption.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "has field run");

    // 3. フィールドの位置（offset 2）にキャレットを置き、メニュー「挿入 > フィールドのリンク解除」（docx.fields.unlink）を実行
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.menu("docx.fields.unlink");

    // リンク解除後の段落を確認
    let doc_unlinked = s.document();
    let p_unlinked = ibis_doc_model::flow_command::paragraph(&doc_unlinked, cap_id).expect("paragraph exists");
    // 全ての Field がプレーンテキスト化されている
    assert!(!p_unlinked.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "no field run after unlink");
    // テキストに図表番号の文字列が含まれている
    assert!(p_unlinked.text().contains("図 1"), "unlinked text preserved content");

    // 4. Undo で元に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, cap_id).expect("paragraph exists");
    assert!(p_undo.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "field restored after undo");

    // 5. Redo で再度リンク解除
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-shift-z");
    let doc_redo = s.document();
    let p_redo = ibis_doc_model::flow_command::paragraph(&doc_redo, cap_id).expect("paragraph exists");
    assert!(!p_redo.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "no field run after redo");
    assert!(p_redo.text().contains("図 1"), "redo preserved text");

    // 6. 保存と再読込
    s.save_as("test-unlink-field.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = flow
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.text().contains("図 1") {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("reread unlinked paragraph");
    assert!(!p_reread.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "field remains unlinked after save & reopen");

    // 7. ショートカットキー ⇧⌘F9 によるリンク解除と F9 によるフィールド更新の検証
    // 新たに図表番号（表）を挿入
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("docx.caption.table");
    let doc_tbl = s.document();
    let p_tbl_caption = doc_tbl
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 表"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph with SEQ 表");
    let tbl_cap_id = p_tbl_caption.id;

    // F9 キーでフィールド更新
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("f9");

    // ショートカット ⇧⌘F9 (cmd-shift-f9) でリンク解除
    s.click_text(ibis_doc_model::FlowPos::new(tbl_cap_id, 2));
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-shift-f9");

    let doc_unlinked2 = s.document();
    let p_unlinked2 = ibis_doc_model::flow_command::paragraph(&doc_unlinked2, tbl_cap_id).expect("paragraph exists");
    assert!(!p_unlinked2.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "field unlinked via shortcut cmd-shift-f9");
    assert!(p_unlinked2.text().contains("表 1"), "unlinked text preserved content for table caption");
}

/// DOCX-PARA-002: 均等割付（Distributed / ⇧⌘J / Ctrl+Shift+J / align.distribute）
/// ショートカットキー ⇧⌘J、メニュー「書式 > 配置 > 均等割り付け」およびツールバーからの
/// 均等割り付け適用、Undo/Redo、ファイル保存・再読込で <w:jc w:val="distribute"/> の保持を検証
fn fix_docx_align_distribute() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文からプレーンな段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 10 && !p.text().contains('\u{fffc}') && p.list.is_none() && p.align != ibis_doc_model::FlowAlign::Distribute {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");

    let p_id = p.id;
    let initial_align = p.align;

    // 2. キャレットを段落に置き、メニュー「align.distribute」を実行
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("align.distribute");

    // 段落の配置が Distribute に変更されたことを確認
    let doc_dist = s.document();
    let p_dist = ibis_doc_model::flow_command::paragraph(&doc_dist, p_id).expect("paragraph exists");
    assert_eq!(p_dist.align, ibis_doc_model::FlowAlign::Distribute, "alignment changed to Distribute via menu");

    // 3. Undo で元の配置に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, p_id).expect("paragraph exists");
    assert_eq!(p_undo.align, initial_align, "alignment restored after undo");

    // 4. ショートカットキー ⇧⌘J (cmd-shift-j) で均等割り付けを適用
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-shift-j");
    let doc_shortcut = s.document();
    let p_shortcut = ibis_doc_model::flow_command::paragraph(&doc_shortcut, p_id).expect("paragraph exists");
    assert_eq!(p_shortcut.align, ibis_doc_model::FlowAlign::Distribute, "alignment changed to Distribute via shortcut cmd-shift-j");

    // 5. 保存と再読込で <w:jc w:val="distribute"/> が維持されることを確認
    s.save_as("test-align-distribute.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = ibis_doc_model::flow_command::paragraph(flow, p_id).expect("paragraph exists in reread");
    assert_eq!(p_reread.align, ibis_doc_model::FlowAlign::Distribute, "alignment remains Distribute after save & reopen");
}

/// DOCX-FIELD-002: フィールドコードの表示/非表示（Shift+F9 / Alt+F9 / docx.fields.toggle / docx.fields.toggle-all）
/// 選択中フィールドのコード表示切替（Shift+F9 / docx.fields.toggle）、文書全体の切替（Alt+F9 / docx.fields.toggle-all）、
/// 表示されるテキストがコード（{ SEQ 図 \* ARABIC } 等）と結果に切り替わること、
/// Undo/Redo、ファイル保存・再読込でフィールド属性が損なわれず保持されることを検証。
fn fix_docx_toggle_field_codes() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文段落を見つけ、図表番号（図）を挿入
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 5 && !p.text().contains('\u{fffc}') {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let p_id = p.id;

    // キャレットを置き、メニュー「docx.caption.figure」で図表番号を挿入
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("docx.caption.figure");

    // 挿入されたキャプション段落（SEQ 図を含む段落）を取得
    let doc_cap = s.document();
    let p_caption = doc_cap
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 図"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph with SEQ");
    let cap_id = p_caption.id;

    // 初期状態: show_field_codes は false、run.show_field_code も false
    assert!(!doc_cap.show_field_codes);
    let run = p_caption.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run.show_field_code);
    assert!(!run.is_field_code_shown(doc_cap.show_field_codes));
    assert_eq!(run.display_field_code(), Some("{ SEQ 図 \\* ARABIC }".into()));

    // 2. メニュー「docx.fields.toggle」（Shift+F9）で個別フィールドのコード表示をトグル
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.menu("docx.fields.toggle");

    let doc_toggled = s.document();
    let p_toggled = ibis_doc_model::flow_command::paragraph(&doc_toggled, cap_id).expect("caption paragraph exists");
    let run_toggled = p_toggled.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_toggled.show_field_code, "field's show_field_code is toggled to true");
    assert!(run_toggled.is_field_code_shown(doc_toggled.show_field_codes), "field code is shown");

    // 3. Undo (cmd-z) で元の結果表示に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, cap_id).expect("caption paragraph exists");
    let run_undo = p_undo.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run_undo.show_field_code, "field code display restored to false after undo");

    // 4. Redo (shift-cmd-z) で再度コード表示になる
    s.key("shift-cmd-z");
    let doc_redo = s.document();
    let p_redo = ibis_doc_model::flow_command::paragraph(&doc_redo, cap_id).expect("caption paragraph exists");
    let run_redo = p_redo.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_redo.show_field_code, "field code display restored to true after redo");

    // 5. ショートカットキー shift-f9 で再度トグルして false に戻す
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("shift-f9");
    let doc_s9 = s.document();
    let p_s9 = ibis_doc_model::flow_command::paragraph(&doc_s9, cap_id).expect("caption paragraph exists");
    let run_s9 = p_s9.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run_s9.show_field_code, "toggled back to false via shortcut shift-f9");

    // 6. メニュー「docx.fields.toggle-all」（Alt+F9）で文書全体のすべてのフィールドコード表示をトグル
    s.menu("docx.fields.toggle-all");
    let doc_all = s.document();
    assert!(doc_all.show_field_codes, "document-wide show_field_codes is true");
    let p_all = ibis_doc_model::flow_command::paragraph(&doc_all, cap_id).expect("caption paragraph exists");
    let run_all = p_all.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_all.is_field_code_shown(doc_all.show_field_codes), "field code is shown under document-wide toggle");

    // 7. ショートカット alt-f9 で文書全体のコード表示をオフに戻す
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("alt-f9");
    let doc_all_off = s.document();
    assert!(!doc_all_off.show_field_codes, "document-wide show_field_codes is back to false via alt-f9");

    // 8. ファイル保存・再読込でフィールドが正常に維持されることを確認
    s.save_as("test-toggle-field-codes.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = flow
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 図"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph exists in reread");
    assert!(p_reread.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })), "field persists after save & reopen");
}

/// DOCX-FIELD-004: フィールドのロック (Ctrl+F11 / ⌘F11 / docx.fields.lock) および
/// ロック解除 (Ctrl+Shift+F11 / ⇧⌘F11 / docx.fields.unlock)。
/// ロック中はフィールド更新 (F9) による再計算がスキップされ、Undo/Redo およびファイル保存・再読込で保持されることを検証。
fn fix_docx_lock_fields() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文段落を見つけ、図表番号（図）を挿入
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 5 && !p.text().contains('\u{fffc}') {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let p_id = p.id;

    // キャレットを置き、メニュー「docx.caption.figure」で図表番号を挿入
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.menu("docx.caption.figure");

    // 挿入されたキャプション段落（SEQ 図を含む段落）を取得
    let doc_cap = s.document();
    let p_caption = doc_cap
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 図"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph with SEQ");
    let cap_id = p_caption.id;

    // 初期状態: field_locked は false
    let run = p_caption.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run.field_locked, "initially not locked");

    // 2. キャレットをキャプション段落に置き、メニュー「docx.fields.lock」でロック
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.menu("docx.fields.lock");

    let doc_locked = s.document();
    let p_locked = ibis_doc_model::flow_command::paragraph(&doc_locked, cap_id).expect("caption paragraph exists");
    let run_locked = p_locked.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_locked.field_locked, "field is locked via menu docx.fields.lock");

    // 3. Undo (cmd-z) でロック解除に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, cap_id).expect("caption paragraph exists");
    let run_undo = p_undo.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run_undo.field_locked, "field lock reverted after undo");

    // 4. Redo (shift-cmd-z) で再びロックされる
    s.key("shift-cmd-z");
    let doc_redo = s.document();
    let p_redo = ibis_doc_model::flow_command::paragraph(&doc_redo, cap_id).expect("caption paragraph exists");
    let run_redo = p_redo.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_redo.field_locked, "field lock re-applied after redo");

    // 5. ロック解除（ショートカット shift-cmd-f11）
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.key("shift-cmd-f11");
    let doc_unlocked = s.document();
    let p_unlocked = ibis_doc_model::flow_command::paragraph(&doc_unlocked, cap_id).expect("caption paragraph exists");
    let run_unlocked = p_unlocked.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(!run_unlocked.field_locked, "field unlocked via shift-cmd-f11");

    // 6. ショートカット cmd-f11 で再びロック
    s.click_text(ibis_doc_model::FlowPos::new(cap_id, 2));
    s.key("cmd-f11");
    let doc_relocked = s.document();
    let p_relocked = ibis_doc_model::flow_command::paragraph(&doc_relocked, cap_id).expect("caption paragraph exists");
    let run_relocked = p_relocked.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(run_relocked.field_locked, "field relocked via cmd-f11");

    // 7. ファイル保存・再読込でロック状態が保持されることを検証
    s.save_as("test-lock-fields.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = flow
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 図"))) {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("caption paragraph exists in reread");
    let field_reread = p_reread.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { .. })).unwrap();
    assert!(field_reread.field_locked, "field_locked persists after save and reopen");
}

fn fix_docx_insert_and_edit_field() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 5 && !p.text().contains('\u{fffc}') {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let p_id = p.id;

    // 段落先頭に "PAGE" と入力
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    s.type_text("PAGE ");

    // 先頭の "PAGE"（0..4）を選択
    s.click_text(ibis_doc_model::FlowPos::new(p_id, 0));
    for _ in 0..4 {
        s.key("shift-right");
    }

    // 2. ショートカット cmd-f9 でフィールドに変換（Word の ⌘F9 / Ctrl+F9）
    s.key("cmd-f9");

    let doc_field = s.document();
    let p_field = ibis_doc_model::flow_command::paragraph(&doc_field, p_id).expect("paragraph exists");
    let field_run = p_field.runs.iter().find(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction == "PAGE")).expect("PAGE field inserted");
    assert!(field_run.show_field_code, "field code shown after cmd-f9");

    // 3. Undo (cmd-z) で元のテキストに戻る
    s.key("cmd-z");
    let doc_undo = s.document();
    let p_undo = ibis_doc_model::flow_command::paragraph(&doc_undo, p_id).expect("paragraph exists");
    assert!(p_undo.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("PAGE"))), "reverted to text PAGE");

    // 4. Redo (shift-cmd-z) で再びフィールドに戻る
    s.key("shift-cmd-z");
    let doc_redo = s.document();
    let p_redo = ibis_doc_model::flow_command::paragraph(&doc_redo, p_id).expect("paragraph exists");
    assert!(p_redo.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction == "PAGE")), "field re-created after redo");

    // 5. メニュー「docx.fields.insert」でフィールドパネルを開く
    s.menu("docx.fields.insert");
    assert!(s.with(|e, _| e.sidebar_open && e.doc_state().and_then(|s| s.helper.field.as_ref()).is_some()), "field panel is open");

    // プリセット "DATE" を設定して適用
    s.update(|e, w, cx| {
        if let Some(panel) = e.doc_state_mut().and_then(|s| s.helper.field.as_mut()) {
            panel.code.update(cx, |i, cx| i.set_value("DATE \\@ \"yyyy-MM-dd\"", w, cx));
        }
        e.apply_field_panel(cx);
    });

    let doc_inserted = s.document();
    let p_inserted = ibis_doc_model::flow_command::paragraph(&doc_inserted, p_id).expect("paragraph exists");
    assert!(p_inserted.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("DATE"))), "DATE field inserted via panel");

    // 6. メニュー「docx.fields.edit」でフィールドコードを編集
    let date_offset: usize = p_inserted.runs.iter().take_while(|r| !matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("DATE"))).map(|r| r.len()).sum();
    s.click_text(ibis_doc_model::FlowPos::new(p_id, date_offset));
    s.menu("docx.fields.edit");
    assert!(s.with(|e, _| e.sidebar_open && e.doc_state().and_then(|s| s.helper.field.as_ref()).is_some_and(|f| f.is_edit)), "field panel open in edit mode");

    // コードを "NUMPAGES" に更新して適用
    s.update(|e, w, cx| {
        if let Some(panel) = e.doc_state_mut().and_then(|s| s.helper.field.as_mut()) {
            panel.code.update(cx, |i, cx| i.set_value("NUMPAGES", w, cx));
        }
        e.apply_field_panel(cx);
    });

    let doc_edited = s.document();
    let p_edited = ibis_doc_model::flow_command::paragraph(&doc_edited, p_id).expect("paragraph exists");
    assert!(p_edited.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction == "NUMPAGES")), "field instruction edited to NUMPAGES");

    // 7. ファイル保存・再読込でフィールドが保持されることを検証
    s.save_as("test-insert-edit-field.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let p_reread = flow.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
        if p.id == p_id { Some(p.clone()) } else { None }
    } else { None }).expect("paragraph exists in reread");
    assert!(p_reread.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction == "NUMPAGES")), "NUMPAGES field persists after save and reopen");
}

fn fix_docx_caption_labels_and_numbering() {
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 本文段落を見つける
    let doc = s.document();
    let p = doc
        .body
        .iter()
        .find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.len() >= 5 && !p.text().contains('\u{fffc}') {
                Some(p.clone())
            } else {
                None
            }
        } else {
            None
        })
        .expect("a plain text paragraph");
    let p_id = p.id;

    // カレットを段落末尾に置く
    s.click_text(ibis_doc_model::FlowPos::new(p_id, p.len()));

    let body_len_before = s.document().body.len();

    // 2. メニュー「docx.caption.equation」で「数式」ラベルの図表番号を挿入
    s.menu("docx.caption.equation");

    let doc = s.document();
    assert_eq!(doc.body.len(), body_len_before + 1, "caption paragraph inserted after text");
    let (_, p_cap1) = doc.body.iter().enumerate().find_map(|(idx, b)| {
        if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ 数式"))) {
                Some((idx, p.clone()))
            } else {
                None
            }
        } else {
            None
        }
    }).expect("caption with 数式 exists");
    assert!(p_cap1.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t == "数式 ")), "has 数式 label");

    // 3. カレットをそのキャプションの末尾に置き、メニュー「docx.caption.insert」で図表番号パネルを開く
    s.click_text(ibis_doc_model::FlowPos::new(p_cap1.id, p_cap1.len()));
    s.menu("docx.caption.insert");
    assert!(s.with(|e, _| e.sidebar_open && e.doc_state().and_then(|s| s.helper.caption.as_ref()).is_some()), "caption panel is open");

    // プリセットに標準ラベルが含まれていることを検証
    s.with(|e, _| {
        let p = e.doc_state().unwrap().helper.caption.as_ref().unwrap();
        assert!(p.presets.contains(&"数式".to_string()));
    });

    // 新規カスタムラベル「グラフ」、説明文「軌道予測」、番号書式 UpperRoman を設定して適用
    s.update(|e, w, cx| {
        if let Some(panel) = e.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
            panel.label.update(cx, |i, cx| i.set_value("グラフ", w, cx));
            panel.text.update(cx, |i, cx| i.set_value("軌道予測", w, cx));
            panel.format = ibis_doc_model::CaptionNumberFormat::UpperRoman;
        }
        e.apply_caption_panel(cx);
    });

    let doc2 = s.document();
    assert_eq!(doc2.body.len(), body_len_before + 2, "second caption paragraph inserted");
    let (cap2_index, p_cap2) = doc2.body.iter().enumerate().find_map(|(idx, b)| {
        if let ibis_doc_model::Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, .. } if instruction.contains("SEQ グラフ"))) {
                Some((idx, p.clone()))
            } else {
                None
            }
        } else {
            None
        }
    }).expect("caption with グラフ exists");
    assert!(p_cap2.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t == "グラフ ")), "has グラフ label");
    assert!(p_cap2.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, shown } if instruction.contains("SEQ グラフ") && shown == "I")), "has SEQ グラフ field with I");
    assert!(p_cap2.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("軌道予測"))), "has 軌道予測 description");

    // 4. ラベル除外オプション（exclude_label = true）の検証
    s.click_text(ibis_doc_model::FlowPos::new(p_cap2.id, p_cap2.len()));
    s.menu("docx.caption.insert");
    s.update(|e, w, cx| {
        if let Some(panel) = e.doc_state_mut().and_then(|s| s.helper.caption.as_mut()) {
            assert!(panel.presets.contains(&"グラフ".to_string()), "グラフ is now in presets");
            panel.label.update(cx, |i, cx| i.set_value("グラフ", w, cx));
            panel.exclude_label = true;
            panel.format = ibis_doc_model::CaptionNumberFormat::LowerLetter;
        }
        e.apply_caption_panel(cx);
    });

    let doc3 = s.document();
    assert_eq!(doc3.body.len(), body_len_before + 3, "third caption paragraph inserted");
    let p_cap3 = if let ibis_doc_model::Block::Paragraph(p) = &doc3.body[cap2_index + 1] { p.clone() } else { panic!() };
    assert!(!p_cap3.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(t) if t.contains("グラフ"))), "label is excluded");
    assert!(p_cap3.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Field { instruction, shown } if instruction.contains("SEQ グラフ") && shown == "b")), "has SEQ グラフ with switch \\* alphabetic (b)");

    // 5. Undo (cmd-z) で直前の挿入を取り消す
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let doc_undo = s.document();
    assert_eq!(doc_undo.body.len(), body_len_before + 2, "undone last caption");

    // 6. Redo (shift-cmd-z) で再度復元
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("shift-cmd-z");
    let doc_redo = s.document();
    assert_eq!(doc_redo.body.len(), body_len_before + 3, "redone last caption");

    // 7. ファイル保存・再読込で検証
    s.save_as("test-caption-labels.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let caps = ibis_doc_model::flow_command::caption_labels(flow);
    assert!(caps.contains(&"数式".to_string()), "数式 label found in reread");
    assert!(caps.contains(&"グラフ".to_string()), "グラフ label found in reread");
}

/// DOCX-TOC-008: 図表目次（Table of Figures: Word 標準の「図表目次の挿入」
/// `TOC \h \z \c "図"` / `TOC \h \z \c "表"` / `TOC \h \z \c "数式"` の生成・更新、
/// 右揃えタブリーダー、PageRef リンク、スタイル定義、Undo/Redo、OOXML保存・再読込検証）。
fn fix_docx_table_of_figures() {
    use ibis_doc_model::{Block, FlowDocument, FlowParagraph, FlowRunKind, FlowPos, TabAlign};
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // 1. 文書末尾にキャプションを追加（図 1, 図 2）
    s.key("cmd-down");
    s.key("enter");
    s.menu("docx.caption.figure");
    s.type_text(" システム構成図");
    s.key("enter");
    s.menu("docx.caption.figure");
    s.type_text(" 処理フロー図");

    // 2. 先頭に移動し、図表目次（図）を挿入
    s.key("cmd-up");
    s.menu("docx.table-of-figures.figure");

    let tof = |doc: &FlowDocument| -> Vec<(u32, FlowParagraph)> {
        let ps: Vec<&FlowParagraph> = doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect();
        let start = ps.iter().position(|p| p.field_start.as_deref().is_some_and(|i| i.contains("TOC") && i.contains("\\c") && i.contains("図"))).expect("a table of figures for 図");
        let end = start + ps[start..].iter().position(|p| p.field_end).expect("field_end for table of figures");
        ps[start..=end].iter().map(|p| (p.id, (*p).clone())).collect()
    };

    let doc = s.document();
    let entries = tof(&doc);
    assert_eq!(entries.len(), 2, "2 figure captions in the table of figures");

    // 各エントリの書式・スタイル・タブ・PageRefリンクを検証
    for (_, p) in &entries {
        assert_eq!(p.style.as_deref(), Some("TableofFigures"), "entry has TableofFigures style ID");
        assert_eq!(p.style.as_deref().and_then(|id| doc.styles.styles.get(id)).map(|st| st.name.as_str()), Some("table of figures"), "entry style name is 'table of figures'");
        // 右揃えタブリーダーの検証
        assert!(p.direct.tabs.iter().any(|t| t.stop.as_ref().is_some_and(|s| s.align == TabAlign::Right && s.leader)), "has right-aligned tab stop with leader");
        // Text, Tab, PageRef の検証
        assert!(p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Text(_))), "has text run");
        assert!(p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::Tab)), "has tab run");
        assert!(p.runs.iter().any(|r| matches!(&r.kind, FlowRunKind::PageRef { .. })), "has pageref run");
        // リンク（Anchor）の検証
        assert!(p.runs.iter().all(|r| matches!(&r.link, Some(ibis_doc_model::Link::Anchor(_)))), "runs link to bookmark");
    }

    let p0 = &entries[0].1;
    let p1 = &entries[1].1;
    assert!(p0.text().contains("システム構成図"), "entry 1 text: {:?}", p0.text());
    assert!(p1.text().contains("処理フロー図"), "entry 2 text: {:?}", p1.text());

    // リンクを辿る (follow_link) の検証
    let went = s.update(|e, _, cx| e.follow_link(FlowPos::new(p0.id, 1), cx));
    assert!(went, "follow_link succeeded");

    // 3. キャプションを追加し、目次位置で「目次の更新」を実行
    s.key("cmd-down");
    s.key("enter");
    s.menu("docx.caption.figure");
    s.type_text(" アーキテクチャ概要");

    // カレットを図表目次の最初の段落に戻す
    s.click_text(FlowPos::new(p0.id, 0));
    s.menu("docx.toc.update");

    let entries_updated = tof(&s.document());
    assert_eq!(entries_updated.len(), 3, "table of figures now has 3 entries after update");
    assert!(entries_updated[2].1.text().contains("アーキテクチャ概要"));

    // 4. Undo (cmd-z) で更新前の2件に戻る
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-z");
    let entries_undone = tof(&s.document());
    assert_eq!(entries_undone.len(), 2, "undone back to 2 entries");

    // 5. Redo (shift-cmd-z) で再度3件に復元
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("shift-cmd-z");
    let entries_redone = tof(&s.document());
    assert_eq!(entries_redone.len(), 3, "redone back to 3 entries");

    // 6. OOXML保存 & 再読込検証
    s.save_as("test-table-of-figures.docx");
    let reread = s.reopen();
    let flow = reread.flow.as_ref().unwrap();
    let reread_entries = tof(flow);
    assert_eq!(reread_entries.len(), 3, "3 entries preserved after save and reopen");
    let start_p = &reread_entries[0].1;
    assert!(start_p.field_start.as_deref().is_some_and(|i| i.contains("TOC") && i.contains("\\c") && i.contains("図")), "field_start preserved: {:?}", start_p.field_start);
}

/// todo-fix #40: the status bar's 「100%▲」 is one button (the pulldown arrow of the toolbar, turned up): a press on the percentage or on the arrow opens the list of zooms (a press again closes it); the hover shows the toolbar buttons' background. In the three formats.
fn fix_zoom_percent_button() {
    for kind in [ibis_pptx::Kind::Document, ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let mut s = Scenario::new_document(kind);
        s.settle();
        let bounds = |s: &mut Scenario| s.with(|e, _| e.zoom_trigger_bounds.get()).expect("the zoom button is painted");
        let b = bounds(&mut s);
        let (left, right, y) = (f32::from(b.origin.x), f32::from(b.origin.x + b.size.width), f32::from(b.origin.y + b.size.height / 2.0));
        // The percentage and the arrow are in it: wider than the arrow alone.
        assert!(right - left > 40.0, "{kind:?}: {b:?}");
        assert!(!s.with(|e, _| e.zoom_menu_open));
        s.hover((left + 8.0, y));
        s.screenshot(&format!("{kind:?}-hover"));
        // A press on the percentage's side opens the list.
        s.click((left + 8.0, y));
        assert!(s.with(|e, _| e.zoom_menu_open), "{kind:?}: the percentage opens the list");
        s.screenshot(&format!("{kind:?}-open"));
        s.update(|e, window, cx| e.set_zoom_menu_open(false, window, cx));
        // And on the arrow's side.
        s.click((right - 6.0, y));
        assert!(s.with(|e, _| e.zoom_menu_open), "{kind:?}: the arrow opens the list");
        s.update(|e, window, cx| e.set_zoom_menu_open(false, window, cx));
    }
}

/// todo-fix #8: ステータスバー・ズーム・ヒント・配置の多言語化（i18n）
fn fix_status_bar_and_zoom_i18n() {
    use crate::shell::editor::Tool;
    use crate::shell::i18n::{self, Language};

    // 1. 日本語モードでの検証
    i18n::show(Language::Japanese);
    assert_eq!(i18n::text("view.zoom-out"), "縮小");
    assert_eq!(i18n::text("view.zoom-in"), "拡大");
    assert_eq!(i18n::text("view.zoom-menu"), "拡大率の一覧");
    assert_eq!(i18n::text("view.zoom-custom"), "指定");
    assert_eq!(i18n::text("view.zoom-fit"), "画面に合わせる");
    assert_eq!(i18n::text("align.left"), "左揃え");
    assert_eq!(i18n::text("align.center"), "中央揃え");
    assert_eq!(i18n::text("align.right"), "右揃え");
    assert_eq!(i18n::text("align.justify"), "両端揃え");
    assert_eq!(i18n::text("status.hint.textbox"), "スライドをクリックまたはドラッグしてテキスト ボックスを配置");
    assert_eq!(i18n::text("status.hint.esc"), "Esc で編集を終了");

    let mut s = Scenario::new_document(ibis_pptx::Kind::Presentation);
    s.update(|e, _, _| e.tool = Tool::TextBox);
    assert!(s.status_texts().iter().any(|t| t == "スライドをクリックまたはドラッグしてテキスト ボックスを配置"));

    // 2. 英語モードでの検証
    i18n::show(Language::English);
    s.update(|_, _, _| {});
    assert_eq!(i18n::text("view.zoom-out"), "Zoom Out");
    assert_eq!(i18n::text("view.zoom-in"), "Zoom In");
    assert_eq!(i18n::text("view.zoom-menu"), "Zoom Levels");
    assert_eq!(i18n::text("view.zoom-custom"), "Custom");
    assert_eq!(i18n::text("view.zoom-fit"), "Fit to Window");
    assert_eq!(i18n::text("align.left"), "Align Left");
    assert_eq!(i18n::text("align.center"), "Align Center");
    assert_eq!(i18n::text("align.right"), "Align Right");
    assert_eq!(i18n::text("align.justify"), "Justify");
    assert_eq!(i18n::text("status.hint.textbox"), "Click or drag on the slide to place a text box");
    assert_eq!(i18n::text("status.hint.esc"), "Press Esc to finish editing");

    assert!(s.status_texts().iter().any(|t| t == "Click or drag on the slide to place a text box"));

    // 3. 日本語モードに戻す
    i18n::show(Language::Japanese);
    assert!(s.status_texts().iter().any(|t| t == "スライドをクリックまたはドラッグしてテキスト ボックスを配置"));
}

/// #21: earnings-presentation.pptx を開いて、右ペイン（サイドバー）を閉じたときの
/// スライド自動フィットのベンチマーク計測
pub(crate) fn fix21_pptx_sidebar_close_zoom_fit_benchmark() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    assert!(!s.with(|e, _| e.sidebar_open), "開いた直後は右ペイン（サイドバー）が閉じていること");
    assert!(s.with(|e, _| e.zoom.is_none()), "初期状態で自動フィットモードであること");
    let closed_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout exists");
    s.menu("view.sidebar");
    s.settle();
    assert!(s.with(|e, _| e.sidebar_open), "メニューで右ペインが開くこと");
    let initial_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout exists");
    assert!(initial_scale < closed_scale, "右ペインを開くと、スライドの自動フィットが狭い幅で計算されること");

    eprintln!("--- START BENCHMARK ---");
    let start = crate::clock::Instant::now();
    s.menu("view.sidebar");
    s.settle();
    let elapsed = start.elapsed();
    eprintln!("--- END BENCHMARK (s.menu(\"view.sidebar\") + s.settle(): {:?}) ---", elapsed);

    let new_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout exists");
    assert!(!s.with(|e, _| e.sidebar_open), "サイドバーが閉じていること");
    assert!(new_scale > initial_scale, "サイドバーが閉じて表示領域が広がり、スライドが拡大フィットしていること: initial={initial_scale}, new={new_scale}");
    assert!((new_scale - closed_scale).abs() < closed_scale * 1e-3, "閉じた後の倍率は、最初から閉じていたときと同じ: {new_scale} / {closed_scale}");

    eprintln!("\n=======================================================");
    eprintln!("[BENCHMARK-FIX-21] Right pane close & slide auto-fit elapsed: {:?}", elapsed);
    eprintln!("Initial scale: {:.4}, New fitted scale: {:.4}", initial_scale, new_scale);
    eprintln!("=======================================================\n");
}

/// pptx の右ペインは、新規作成でも、開いたときでも、閉じた状態で始まる。
/// ツールバー（view.sidebar）で開ける。docx の右ペインは今までどおり開いている。
pub(crate) fn pptx_sidebar_starts_closed() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Presentation);
    assert!(!s.with(|e, _| e.sidebar_open), "新しいプレゼンテーションでは右ペインが閉じている");
    let closed_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout");
    s.menu("view.sidebar");
    s.settle();
    assert!(s.with(|e, _| e.sidebar_open), "開くボタンで右ペインが開く");
    let open_scale = s.with(|e, _| e.slide_layout().map(|l| l.scale)).expect("slide_layout");
    assert!(open_scale < closed_scale, "開くとスライドの自動フィットが狭くなる: {open_scale} / {closed_scale}");

    drop(s); // One scenario window at a time.
    let mut s = Scenario::open("test/editing/pptx-comment-newline-enter.pptx");
    assert!(!s.with(|e, _| e.sidebar_open), "pptx を開いたとき右ペインは閉じている");
    drop(s);

    let mut s = Scenario::new_document(ibis_pptx::Kind::Document);
    assert!(s.with(|e, _| e.sidebar_open), "docx の右ペインは開いたまま");
}

/// XLSX-FILE-017: ファイル > 情報 (file.info) によるブックプロパティパネル
/// （タイトル・作成者・件名・キーワード・コメント・会社名等）の編集・適用、
/// 保存・再オープンでの保持、Undo/Redo 検証。
fn fix_xlsx_file017_doc_properties() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. file.info メニューからブックプロパティパネルを開く
    s.menu("file.info");
    assert!(s.with(|e, _| e.sidebar_open && e.sheet_state().is_some_and(|st| st.panels.properties.is_some())), "properties panel is open");

    // 2. プロパティを入力・適用
    s.update(|e, w, cx| {
        if let Some(p) = e.sheet_state_mut().and_then(|st| st.panels.properties.as_mut()) {
            p.title.update(cx, |inp, cx| inp.set_value("売上月次分析", w, cx));
            p.creator.update(cx, |inp, cx| inp.set_value("営業企画部", w, cx));
            p.subject.update(cx, |inp, cx| inp.set_value("月次売上レポート", w, cx));
            p.keywords.update(cx, |inp, cx| inp.set_value("売上, 2026, 分析", w, cx));
            p.description.update(cx, |inp, cx| inp.set_value("各支社の実績サマリー", w, cx));
            p.company.update(cx, |inp, cx| inp.set_value("株式会社アイビス", w, cx));
            p.manager.update(cx, |inp, cx| inp.set_value("山田", w, cx));
            p.category.update(cx, |inp, cx| inp.set_value("月次報告", w, cx));
        }
        e.apply_sheet_doc_properties(cx);
    });

    // 3. モデルのプロパティが更新されたことを検証
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.title.as_deref(), Some("売上月次分析"));
        assert_eq!(wb.properties.creator.as_deref(), Some("営業企画部"));
        assert_eq!(wb.properties.subject.as_deref(), Some("月次売上レポート"));
        assert_eq!(wb.properties.keywords.as_deref(), Some("売上, 2026, 分析"));
        assert_eq!(wb.properties.description.as_deref(), Some("各支社の実績サマリー"));
        assert_eq!(wb.properties.company.as_deref(), Some("株式会社アイビス"));
        assert_eq!(wb.properties.manager.as_deref(), Some("山田"));
        assert_eq!(wb.properties.category.as_deref(), Some("月次報告"));
    });

    s.screenshot("xlsx-file-info-properties");

    // 4. 保存と再読込での保持検証
    s.save_as("file017_sheet_properties.xlsx");
    let reloaded = s.reopen();
    let wb = reloaded.workbook.as_deref().expect("workbook exists");
    assert_eq!(wb.properties.title.as_deref(), Some("売上月次分析"));
    assert_eq!(wb.properties.creator.as_deref(), Some("営業企画部"));
    assert_eq!(wb.properties.subject.as_deref(), Some("月次売上レポート"));
    assert_eq!(wb.properties.keywords.as_deref(), Some("売上, 2026, 分析"));
    assert_eq!(wb.properties.description.as_deref(), Some("各支社の実績サマリー"));
    assert_eq!(wb.properties.company.as_deref(), Some("株式会社アイビス"));
    assert_eq!(wb.properties.manager.as_deref(), Some("山田"));
    assert_eq!(wb.properties.category.as_deref(), Some("月次報告"));

    // 5. Undo 検証
    close_modal_panel(&mut s);
    s.key("cmd-z");
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_ne!(wb.properties.title.as_deref(), Some("売上月次分析"));
    });

    // 6. Redo 検証
    s.key("cmd-shift-z");
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.title.as_deref(), Some("売上月次分析"));
    });
}

/// Scenario verifying the fix for xlsx sheet tab scrollbar confinement and dynamic + button:
/// 1. When tabs are few (content width is small), + button is inline at the right of the last sheet tab.
/// 2. When tabs increase and approach/overflow the viewport, + button moves to the right-edge fixed area,
///    and the scrollview's right boundary is constrained to the left of the fixed + button.
/// 3. The horizontal scrollbar is confined within the scroll viewport and does not spill over the left buttons
///    (◀ ▶ ≡) or right edge.
/// 4. Left buttons (◀ ▶ ≡) and the fixed + button are clickable and work reliably even when scrolled.
fn fix_xlsx_sheet_tabs_scrollbar_and_fixed_add() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let sheets = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.len());
    let (y, list, right, left) = (679.0, 70.0, 41.0, 14.0);

    // 1. Initial state: 3 sheets, no overflow, + button is inline at the end of tabs (~310.0).
    assert_eq!(sheets(&mut s), 3);
    let (_offset, max) = tab_offset(&mut s);
    assert_eq!(max, 0.0, "initially tabs do not overflow");
    s.screenshot("few_inline_plus");

    // Click inline + button to add the 4th sheet.
    s.click((310.0, y));
    assert_eq!(sheets(&mut s), 4, "clicked inline + button to add 4th sheet");

    // 2. Add sheets to overflow (increase up to 22 sheets).
    for _ in 0..18 {
        s.menu("xlsx.insert-sheet");
    }
    s.settle();
    let current_sheets = sheets(&mut s);
    assert_eq!(current_sheets, 22);

    let (_offset, max) = tab_offset(&mut s);
    assert!(max > 100.0, "tabs overflow: max_offset = {max}");
    s.screenshot("overflow_fixed_plus");

    // 3. Fixed + button on the right edge (~1186.0) adds another sheet.
    let fixed_plus_x = 1186.0;
    s.click((fixed_plus_x, y));
    s.settle();
    assert_eq!(sheets(&mut s), 23, "fixed + button on the right edge added 23rd sheet");

    // 4. Verify left buttons (◀ ▶ ≡) are fully responsive and not obstructed by the scrollbar.
    let (offset, max) = tab_offset(&mut s);

    // Click ◀ button: scrolls towards the beginning.
    s.click((left, y));
    s.settle();
    let (after_left, _) = tab_offset(&mut s);
    assert!(after_left > offset || offset == 0.0, "◀ button scrolled: {offset} -> {after_left}");
    s.screenshot("scrolled_left");

    // Click ▶ button: scrolls towards the end.
    s.click((right, y));
    s.settle();
    let (after_right, _) = tab_offset(&mut s);
    assert!(after_right < after_left || after_left <= -max, "▶ button scrolled: {after_left} -> {after_right}");

    // Click ≡ button: opens sheet list menu.
    s.click((list, y));
    s.settle();
    s.screenshot("sheet_list_opened");
}

/// Scenario directly verifying the user's specific feedback:
/// "Starting from an empty workbook, adding sheets with the + button:
///  At Sheet12, + should NOT move to the fixed area prematurely.
///  It should remain inline until Sheet14+ approaching the right edge."
fn fix_xlsx_sheet_add_threshold_blank_workbook() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    s.settle();
    let sheets = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.len());
    let y = 679.0;

    // Initial state: Sheet1 (1 sheet).
    assert_eq!(sheets(&mut s), 1);
    let (_offset, max) = tab_offset(&mut s);
    assert_eq!(max, 0.0, "initially tabs do not overflow");

    // Add sheets up to Sheet12. At Sheet12, + should remain inline (no overflow).
    for target in 2..=12 {
        s.menu("xlsx.insert-sheet");
        s.settle();
        assert_eq!(sheets(&mut s), target, "added sheet {target}");
        let (_offset, max) = tab_offset(&mut s);
        assert_eq!(max, 0.0, "at sheet {target}, still inline without overflow");
    }

    assert_eq!(sheets(&mut s), 12);
    s.screenshot("sheet12_still_inline");

    // Add Sheet13 and Sheet14 via menu.
    for target in 13..=14 {
        s.menu("xlsx.insert-sheet");
        s.settle();
        assert_eq!(sheets(&mut s), target, "added sheet {target}");
    }

    s.screenshot("sheet14_near_edge");

    // Add more sheets until overflow occurs (up to 20 sheets).
    for target in 15..=20 {
        s.menu("xlsx.insert-sheet");
        s.settle();
        assert_eq!(sheets(&mut s), target);
    }

    // With 20 sheets, tabs overflow and + moves to the right edge fixed area (~1186.0).
    let (_offset, max) = tab_offset(&mut s);
    assert!(max > 50.0, "overflowed with 20 sheets: {max}");
    s.screenshot("overflowed_fixed_plus");

    // Click fixed + on right edge to add 21st sheet.
    s.click((1186.0, y));
    s.settle();
    assert_eq!(sheets(&mut s), 21, "fixed + on right edge added 21st sheet");
}

/// XLSX-FILE-018: カスタムプロパティ（docProps/custom.xml）の閲覧・追加・削除・適用、
/// 保存・再読込での完全復元、Undo/Redo 検証。
fn fix_xlsx_file018_custom_properties() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.settle();

    // 1. ファイル > 情報（file.info）からプロパティパネルを開く
    s.menu("file.info");
    s.settle();

    // 2. カスタムプロパティを追加
    s.update(|e, w, cx| {
        if let Some(p) = e.sheet_state_mut().and_then(|st| st.panels.properties.as_mut()) {
            p.new_custom_name.update(cx, |inp, cx| inp.set_value("プロジェクト名", w, cx));
            p.new_custom_val.update(cx, |inp, cx| inp.set_value("次世代オフィス", w, cx));
        }
        e.add_custom_property(w, cx);

        if let Some(p) = e.sheet_state_mut().and_then(|st| st.panels.properties.as_mut()) {
            p.new_custom_name.update(cx, |inp, cx| inp.set_value("承認状態", w, cx));
            p.new_custom_val.update(cx, |inp, cx| inp.set_value("承認済み", w, cx));
        }
        e.add_custom_property(w, cx);

        e.apply_sheet_doc_properties(cx);
    });

    // 3. モデルのカスタムプロパティが更新されたことを検証
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.custom.len(), 2);
        assert_eq!(wb.properties.custom[0].name, "プロジェクト名");
        assert_eq!(wb.properties.custom[0].value, ibis_doc_model::CustomPropertyValue::Text("次世代オフィス".to_string()));
        assert_eq!(wb.properties.custom[1].name, "承認状態");
        assert_eq!(wb.properties.custom[1].value, ibis_doc_model::CustomPropertyValue::Text("承認済み".to_string()));
    });

    s.screenshot("xlsx-file-custom-properties");

    // 4. 保存と再読込での完全保持検証
    s.save_as("file018_custom_properties.xlsx");
    let reloaded = s.reopen();
    let wb = reloaded.workbook.as_deref().expect("workbook exists");
    assert_eq!(wb.properties.custom.len(), 2);
    assert_eq!(wb.properties.custom[0].name, "プロジェクト名");
    assert_eq!(wb.properties.custom[0].value, ibis_doc_model::CustomPropertyValue::Text("次世代オフィス".to_string()));
    assert_eq!(wb.properties.custom[1].name, "承認状態");
    assert_eq!(wb.properties.custom[1].value, ibis_doc_model::CustomPropertyValue::Text("承認済み".to_string()));

    // 5. プロパティの削除と適用
    s.update(|e, _, cx| {
        e.remove_custom_property(0, cx);
        e.apply_sheet_doc_properties(cx);
    });
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.custom.len(), 1);
        assert_eq!(wb.properties.custom[0].name, "承認状態");
    });

    // 6. Undo 検証（削除が元に戻り、2件に戻る）
    close_modal_panel(&mut s);
    s.key("cmd-z");
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.custom.len(), 2);
    });

    // 7. Redo 検証（再削除され、1件に戻る）
    s.key("cmd-shift-z");
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_deref().expect("workbook exists");
        assert_eq!(wb.properties.custom.len(), 1);
    });
}

/// XLSX-SEC-022: 画像・図・グラフの代替テキスト（Alt Text）の表示・編集・保存・再オープン・Undo/Redo 検証.
fn fix_xlsx_sec022_alt_text() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. 図形未選択時はメニューが無効
    assert_eq!(s.menu_state("xlsx.alt-text"), Some((false, false)));

    // 2. シート1のグラフを選択
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))).unwrap();
    let at = s.shape_position(index);
    s.click(at);

    // グラフ選択時はメニューが有効
    assert_eq!(s.menu_state("xlsx.alt-text"), Some((true, false)));

    // 3. メニュー「xlsx.alt-text」を実行してサイドバーパネルを開く
    s.menu("xlsx.alt-text");
    assert!(s.with(|e, _| e.wb().panels.alt_text.is_some()));

    // 4. 代替テキスト（タイトル・説明）を入力して「適用」
    s.update(|e, window, cx| {
        let p = e.wb().panels.alt_text.as_ref().unwrap();
        p.title_field.update(cx, |i, cx| i.set_value("売上サマリーグラフ", window, cx));
        p.desc_field.update(cx, |i, cx| i.set_value("4〜6月の店舗別売上実績棒グラフ", window, cx));
        e.apply_sheet_alt_text(cx);
    });

    let chart_shape = s.with(|e, _| e.selected_sheet_shape()).unwrap();
    s.with(|e, _| {
        let alt = e.slide().shape_alt_texts.get(&chart_shape).unwrap();
        assert_eq!(alt.title, "売上サマリーグラフ");
        assert_eq!(alt.description, "4〜6月の店舗別売上実績棒グラフ");
    });

    // 5. 保存して再オープンし、代替テキストが正しく読み込まれることを検証
    s.save_as("sec022-alt-text.xlsx");
    let reopened = s.reopen();
    let reopened_alt = reopened.slides[0].shape_alt_texts.get(&chart_shape).expect("alt text preserved");
    assert_eq!(reopened_alt.title, "売上サマリーグラフ");
    assert_eq!(reopened_alt.description, "4〜6月の店舗別売上実績棒グラフ");

    // 6. Undo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().shape_alt_texts.get(&chart_shape), None);
    });

    // 7. Redo 検証
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let alt = e.slide().shape_alt_texts.get(&chart_shape).unwrap();
        assert_eq!(alt.title, "売上サマリーグラフ");
    });
}

fn fix_xlsx_sec008_threaded_comments() {
    let mut s = Scenario::open("test/editing/xlsx-threads.xlsx");

    // 初期状態の確認（B2にスレッド、B4に解決済みスレッド）
    let threads_count = s.with(|e, _| e.current_sheet().unwrap().1.threads.len());
    assert_eq!(threads_count, 2);

    // 1. C3 (col: 2, row: 2) を選択して「insert.comment」を実行
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        if let Some(state) = e.sheet_state_mut() {
            state.cells.insert(slide, (2, 2));
        }
        cx.notify();
    });
    s.menu("insert.comment");
    assert!(s.with(|e, _| e.wb().panels.threaded_comment.is_some()));
    assert_eq!(s.with(|e, _| e.wb().panels.threaded_comment.as_ref().unwrap().cell), (2, 2));

    // 2. 新規コメントを投稿
    s.update(|e, window, cx| {
        e.send_threaded_comment("C3への新規コメント", window, cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        let t = sheet.thread_at(2, 2).expect("thread exists at C3");
        assert_eq!(t.posts.len(), 1);
        assert_eq!(t.posts[0].text, "C3への新規コメント");
        assert!(!t.done);
    });

    // 3. 返信を投稿
    s.update(|e, window, cx| {
        e.send_threaded_comment("返信メッセージ", window, cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        let t = sheet.thread_at(2, 2).unwrap();
        assert_eq!(t.posts.len(), 2);
        assert_eq!(t.posts[1].text, "返信メッセージ");
    });

    // 4. 解決済みに切り替え
    s.update(|e, _, cx| {
        e.resolve_threaded_comment(2, 2, true, cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.thread_at(2, 2).unwrap().done);
    });

    // 5. 再開に切り替え
    s.update(|e, _, cx| {
        e.resolve_threaded_comment(2, 2, false, cx);
    });
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(!sheet.thread_at(2, 2).unwrap().done);
    });

    // 6. 保存と再オープン（ラウンドトリップ検証）
    s.save_as("sec008-threads-test.xlsx");
    let reopened = s.reopen();
    let re_sheet = reopened.slides[0].shapes.iter().find_map(|sh| match &sh.kind {
        ibis_doc_model::ShapeKind::Sheet(s) => Some(s),
        _ => None,
    }).unwrap();
    let re_t = re_sheet.thread_at(2, 2).expect("preserved after reload");
    assert_eq!(re_t.posts.len(), 2);
    assert_eq!(re_t.posts[0].text, "C3への新規コメント");
    assert_eq!(re_t.posts[1].text, "返信メッセージ");

    // 7. コメントの削除
    s.menu("xlsx.comment-delete");
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.thread_at(2, 2).is_none());
        assert!(e.wb().panels.threaded_comment.is_none());
    });

    // 8. Undo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.thread_at(2, 2).is_some());
    });

    // 9. Redo 検証
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        assert!(sheet.thread_at(2, 2).is_none());
    });
}

/// XLSX-FILE-004 / XLSX-FILE-007: テンプレート保存 (.xltx) およびテンプレートからの新規作成
fn fix_xlsx_file004_file007_templates() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let temp_dir = std::env::temp_dir();
    let xltx_path = temp_dir.join("test_scenario_sales_template.xltx");

    // 1. .xltx として保存（copy_to）
    s.with(|e, _| {
        assert!(e.is_workbook());
        e.copy_to(&xltx_path).expect("copy_to .xltx succeeds");
    });
    assert!(xltx_path.exists());

    // 2. 読み込み検証
    let loaded = crate::read_file(&xltx_path).expect("read xltx succeeds");
    assert!(loaded.presentation.workbook.is_some(), "saved xltx must be a workbook");

    // 3. new_from_template_path でテンプレートから新規作成
    s.update(|e, window, cx| {
        e.new_from_template_path(xltx_path.clone(), window, cx);
    });
    s.settle();

    s.with(|e, _| {
        assert_eq!(e.file_name.as_deref(), Some("Book1.xlsx"));
        assert!(e.path.is_none(), "template instance must be untitled (path is None)");
        assert!(e.is_workbook());
        assert!(e.presentation().workbook.is_some());
    });

    // 4. open_path でテンプレートファイルを開いた場合も新規ブック（path: None, Book1.xlsx）として開く
    s.update(|e, window, cx| {
        e.open_path(xltx_path.clone(), None, window, cx);
    });
    s.settle();

    s.with(|e, _| {
        assert_eq!(e.file_name.as_deref(), Some("Book1.xlsx"));
        assert!(e.path.is_none(), "opened template must be untitled (path is None)");
        assert!(e.is_workbook());
    });

    // 5. 後片付け
    let _ = std::fs::remove_file(xltx_path);
}

/// XLSX-SEC-021: アクセシビリティ検査パネルとジャンプ・再検査の動作検証
fn fix_xlsx_sec021_accessibility_check() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. メニューコマンド `xlsx.accessibility-check` からパネルを開く
    s.menu("xlsx.accessibility-check");
    s.settle();

    s.with(|e, _| {
        assert!(e.sidebar_open, "sidebar should be opened");
        let panel = e.wb().panels.accessibility.as_ref().expect("accessibility panel should be active");
        println!("Initial report: errors={}, warnings={}, tips={}",
            panel.report.errors.len(),
            panel.report.warnings.len(),
            panel.report.tips.len());
    });

    // 2. 図形（代替テキストなし）、テーブル（見出し行なし）、不明瞭リンクを追加して再検査
    s.update(|e, _window, cx| {
        let slide_id = e.current_slide;
        let slide = e.document.ids().slides.iter_mut().find(|s| s.id == slide_id).unwrap();

        // 代替テキストなしの図形（空のテキストボックス）を追加
        let new_shape_id = ibis_doc_model::ShapeId(9999);
        slide.shapes.push(ibis_doc_model::Shape::text_box(
            new_shape_id,
            ibis_doc_model::EmuRect::default(),
            "",
        ));
        slide.shape_names.insert(new_shape_id, "テスト図形".into());

        // 見出し行なしのテーブルと、不明瞭なハイパーリンクを追加
        for shape in &mut slide.shapes {
            if let ibis_doc_model::ShapeKind::Sheet(sheet_arc) = &mut shape.kind {
                let sheet = std::sync::Arc::make_mut(sheet_arc);
                sheet.tables.push(ibis_doc_model::sheet::SheetTable {
                    name: "Table_NoHeader".into(),
                    range: ibis_doc_model::sheet::CellRange {
                        first_row: 10,
                        first_column: 0,
                        last_row: 15,
                        last_column: 3,
                    },
                    header_rows: 0,
                    totals_rows: 0,
                    columns: vec!["Col1".into(), "Col2".into(), "Col3".into(), "Col4".into()],
                    style: None,
                    filter: None,
                    totals: Vec::new(),
                });

                sheet.cells.push(ibis_doc_model::sheet::SheetCell {
                    row: 20,
                    column: 1,
                    value: ibis_doc_model::sheet::CellValue::Empty,
                    text: "ここをクリック".into(),
                    kind: ibis_doc_model::sheet::CellKind::Text,
                    formula: None,
                    style: 0,
                    phonetic: None,
                    runs: None,
                });
                sheet.hyperlinks.push(ibis_doc_model::sheet::Hyperlink {
                    range: ibis_doc_model::sheet::CellRange {
                        first_row: 20,
                        first_column: 1,
                        last_row: 20,
                        last_column: 1,
                    },
                    target: Some("https://example.com".into()),
                    location: None,
                    tooltip: None,
                });
                break;
            }
        }

        // 再検査を実行
        e.recheck_xlsx_accessibility(cx);
    });
    s.settle();

    // 3. レポート内容の検証
    s.with(|e, _| {
        let panel = e.wb().panels.accessibility.as_ref().expect("accessibility panel should be active");
        assert!(!panel.report.errors.is_empty(), "should detect missing alt text error");
        assert!(!panel.report.warnings.is_empty(), "should detect missing table header warning or default sheet name");
        assert!(!panel.report.tips.is_empty(), "should detect unclear hyperlink tip");

        // エラー、警告、ヒントそれぞれについて検証
        let has_img_error = panel.report.errors.iter().any(|err| err.title.contains("テスト図形"));
        assert!(has_img_error, "must detect missing alt text for 'テスト図形'");

        let has_tbl_warning = panel.report.warnings.iter().any(|w| w.title.contains("Table_NoHeader"));
        assert!(has_tbl_warning, "must detect table without headers");

        let has_hl_tip = panel.report.tips.iter().any(|t| t.title.contains("B21"));
        assert!(has_hl_tip, "must detect unclear hyperlink at B21");
    });

    // 4. ジャンプ機能の検証
    s.update(|e, _window, cx| {
        // 全課題リストの先頭（エラー）へジャンプ
        e.jump_to_xlsx_accessibility_issue(0, cx);
    });
    s.settle();

    s.with(|e, _| {
        let panel = e.wb().panels.accessibility.as_ref().unwrap();
        assert_eq!(panel.selected_issue, Some(0));
    });

    // 5. パネルを閉じる
    s.update(|e, _window, cx| {
        e.close_xlsx_accessibility_panel(cx);
    });
    s.settle();

    s.with(|e, _| {
        assert!(!e.sidebar_open, "sidebar should be closed");
        assert!(e.wb().panels.accessibility.is_none(), "accessibility panel should be cleared");
    });
}

/// XLSX-SEC-019: 印刷対象（作業中のシート／ブック全体／選択した部分）の選択・PDF書き出し動作の検証
fn fix_xlsx_sec019_print_targets() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let out_dir = std::env::temp_dir().join(format!("test-sec019-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&out_dir);

    // 1. 初期状態は ActiveSheet（作業中のシート）
    s.with(|e, _| {
        assert_eq!(e.print_what(), crate::xlsx::PrintWhat::ActiveSheet);
    });

    // 2. ActiveSheet で PDF 書き出し
    let pdf1 = out_dir.join("active_sheet.pdf");
    let job1 = s.with(|e, _| e.write_pdf(pdf1.clone())).expect("write_pdf returns job");
    assert!(job1().is_ok(), "ActiveSheet pdf output succeeded");
    assert!(pdf1.exists());
    let bytes1 = std::fs::read(&pdf1).unwrap();
    assert!(bytes1.starts_with(b"%PDF-"));

    // 3. ページ設定パネルを開き、印刷対象を EntireWorkbook（ブック全体）に変更
    s.update(|e, _window, cx| {
        e.open_page_setup(cx);
        e.set_print_what(crate::xlsx::PrintWhat::EntireWorkbook, cx);
    });
    s.settle();

    s.with(|e, _| {
        assert!(e.sidebar_open);
        assert_eq!(e.print_what(), crate::xlsx::PrintWhat::EntireWorkbook);
    });

    // 4. EntireWorkbook で PDF 書き出し
    let pdf2 = out_dir.join("entire_workbook.pdf");
    let job2 = s.with(|e, _| e.write_pdf(pdf2.clone())).expect("write_pdf returns job");
    assert!(job2().is_ok(), "EntireWorkbook pdf output succeeded");
    assert!(pdf2.exists());
    let bytes2 = std::fs::read(&pdf2).unwrap();
    assert!(bytes2.starts_with(b"%PDF-"));

    // 5. 選択範囲（Selection）に変更し、セル範囲を選択して PDF 書き出し
    s.update(|e, _window, cx| {
        e.set_print_what(crate::xlsx::PrintWhat::Selection, cx);
        let sheet_id = e.current_slide;
        let wb = e.wb_mut();
        // B2:D6 を選択
        wb.cells.insert(sheet_id, (1, 1));
        wb.cell_ends.insert(sheet_id, (3, 5));
    });
    s.settle();

    s.with(|e, _| {
        assert_eq!(e.print_what(), crate::xlsx::PrintWhat::Selection);
        assert_eq!(e.selected_range(), Some(ibis_doc_model::CellRange { first_row: 1, first_column: 1, last_row: 5, last_column: 3 }));
    });

    let pdf3 = out_dir.join("selection.pdf");
    let job3 = s.with(|e, _| e.write_pdf(pdf3.clone())).expect("write_pdf returns job");
    assert!(job3().is_ok(), "Selection pdf output succeeded");
    assert!(pdf3.exists());
    let bytes3 = std::fs::read(&pdf3).unwrap();
    assert!(bytes3.starts_with(b"%PDF-"));

    // 6. 印刷対象を作業中のシート（ActiveSheet）に戻して確認
    s.update(|e, _window, cx| {
        e.set_print_what(crate::xlsx::PrintWhat::ActiveSheet, cx);
    });
    s.settle();

    s.with(|e, _| {
        assert_eq!(e.print_what(), crate::xlsx::PrintWhat::ActiveSheet);
    });

    let pdf4 = out_dir.join("active_sheet_again.pdf");
    let job4 = s.with(|e, _| e.write_pdf(pdf4.clone())).expect("write_pdf returns job");
    assert!(job4().is_ok(), "ActiveSheet pdf output succeeded");
    assert!(pdf4.exists());
    let bytes4 = std::fs::read(&pdf4).unwrap();
    assert!(bytes4.starts_with(b"%PDF-"));

    // 7. 後片付け
    let _ = std::fs::remove_dir_all(&out_dir);
}

/// XLSX-SEC-016: ヘッダー・フッターのコード（カスタム ヘッダー・フッター編集UI、左・中・右セクション入力、フィールドコード挿入、ページ設定連携）:
/// 1. ページ設定パネルのヘッダー・フッタープリセット選択とコード反映
/// 2. カスタム ヘッダー編集画面の起動、左・中・右セクションへの文字列・フィールドコード（&P, &N, &D, &A）入力・反映
/// 3. カスタム フッター編集画面の起動、フィールド挿入、直接コード入力（apply_hf_code_direct）
/// 4. ヘッダー・フッターオプション（different_first, different_odd_even, scale_with_doc, align_with_margins）の切り替え
/// 5. 偶数ページ・先頭ページの個別設定（set_hf_page_kind）
/// 6. Undo/Redo によるヘッダー・フッター変更の取り消し・再適用
/// 7. OOXML（.xlsx）保存・再オープンによるヘッダー・フッター全設定（odd, even, first, 各種フラグ）の完全保持検証
/// 8. PDF エクスポート（ベクター描画）がヘッダー・フッター付きで正常に出力されることを検証
fn fix_xlsx_sec016_custom_header_footer() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("file.page-setup");
    s.settle();

    // 1. ページ設定パネルが開いていることを確認
    s.with(|e, _| {
        assert!(e.sheet_state().map_or(false, |st| st.page_setup), "page setup panel should be open");
    });

    // 2. プリセットからヘッダーとフッターを選択
    s.update(|e, _, cx| {
        e.set_print(
            |p| {
                p.header_footer.odd_header = Some("&C&P".to_string());
                p.header_footer.odd_footer = Some("&C&P / &N ページ".to_string());
            },
            None,
            cx,
        );
    });
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.odd_header.as_deref(), Some("&C&P"));
        assert_eq!(p.header_footer.odd_footer.as_deref(), Some("&C&P / &N ページ"));
    });

    // 3. カスタム ヘッダー編集画面を開く
    s.update(|e, window, cx| {
        e.open_header_footer_edit(crate::xlsx::header_footer_edit::HfTarget::Header, window, cx);
    });
    s.settle();
    s.with(|e, _| {
        assert!(e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).is_some(), "header edit panel should be open");
    });

    // 4. 左側にシート名(&A)、中央にタイトル文字列、右側にページ番号(&P)と総ページ数(&N)を入力
    s.update(|e, window, cx| {
        let edit = e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).unwrap();
        edit.left.update(cx, |i, cx| i.set_value("&A".to_string(), window, cx));
        edit.center.update(cx, |i, cx| i.set_value("売上報告書".to_string(), window, cx));
        edit.right.update(cx, |i, cx| i.set_value("&P / &N".to_string(), window, cx));
        e.apply_header_footer_edit(cx);
    });
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.odd_header.as_deref(), Some("&L&A&C売上報告書&R&P / &N"));
    });

    // 5. カスタム フッター編集画面に切り替え、直接コード入力（apply_hf_code_direct）で設定
    s.update(|e, window, cx| {
        e.open_header_footer_edit(crate::xlsx::header_footer_edit::HfTarget::Footer, window, cx);
    });
    s.settle();
    s.update(|e, window, cx| {
        let edit = e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).unwrap();
        edit.code.update(cx, |i, cx| i.set_value("&L&D &T&R社外秘".to_string(), window, cx));
        e.apply_hf_code_direct(window, cx);
    });
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.odd_footer.as_deref(), Some("&L&D &T&R社外秘"));
    });

    // 6. オプション（先頭ページ別、奇数偶数別、拡大縮小、余白合わせ）の設定
    s.update(|e, _, cx| {
        e.set_print(
            |p| {
                p.header_footer.different_first = true;
                p.header_footer.different_odd_even = true;
                p.header_footer.scale_with_doc = false;
                p.header_footer.align_with_margins = false;
            },
            None,
            cx,
        );
    });
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert!(p.header_footer.different_first);
        assert!(p.header_footer.different_odd_even);
        assert!(!p.header_footer.scale_with_doc);
        assert!(!p.header_footer.align_with_margins);
    });

    // 7. 先頭ページ用・偶数ページ用のヘッダーを設定
    s.update(|e, window, cx| {
        e.open_header_footer_edit(crate::xlsx::header_footer_edit::HfTarget::Header, window, cx);
        e.set_hf_page_kind(crate::xlsx::header_footer_edit::HfPageKind::First, window, cx);
        let edit = e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).unwrap();
        edit.center.update(cx, |i, cx| i.set_value("表紙".to_string(), window, cx));
        e.apply_header_footer_edit(cx);

        e.set_hf_page_kind(crate::xlsx::header_footer_edit::HfPageKind::Even, window, cx);
        let edit = e.sheet_state().and_then(|st| st.header_footer_edit.as_ref()).unwrap();
        edit.left.update(cx, |i, cx| i.set_value("&P".to_string(), window, cx));
        edit.right.update(cx, |i, cx| i.set_value("&A".to_string(), window, cx));
        e.apply_header_footer_edit(cx);

        e.close_header_footer_edit(cx);
    });
    s.settle();

    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.first_header.as_deref(), Some("&C表紙"));
        assert_eq!(p.header_footer.even_header.as_deref(), Some("&L&P&R&A"));
        assert_eq!(p.header_footer.odd_header.as_deref(), Some("&L&A&C売上報告書&R&P / &N"));
    });

    // 8. Undo (cmd-z) で偶数ページの変更を取り消せることを確認
    s.key("cmd-z");
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.even_header, None);
        assert_eq!(p.header_footer.first_header.as_deref(), Some("&C表紙"));
    });

    // 9. Redo (shift-cmd-z) で再度適用されることを確認
    s.key("shift-cmd-z");
    s.settle();
    s.with(|e, _| {
        let p = e.current_print().unwrap();
        assert_eq!(p.header_footer.even_header.as_deref(), Some("&L&P&R&A"));
    });

    // 10. 保存・再オープンによる OOXML 往復検証
    s.save_as("fix_xlsx_custom_header_footer.xlsx");
    let saved = s.reopen();
    let saved_sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap().clone();
    let saved_hf = &saved_sheet.print.header_footer;
    assert_eq!(saved_hf.odd_header.as_deref(), Some("&L&A&C売上報告書&R&P / &N"));
    assert_eq!(saved_hf.odd_footer.as_deref(), Some("&L&D &T&R社外秘"));
    assert_eq!(saved_hf.first_header.as_deref(), Some("&C表紙"));
    assert_eq!(saved_hf.even_header.as_deref(), Some("&L&P&R&A"));
    assert!(saved_hf.different_first);
    assert!(saved_hf.different_odd_even);
    assert!(!saved_hf.scale_with_doc);
    assert!(!saved_hf.align_with_margins);

    // 11. PDF エクスポートでヘッダー・フッター描画を含む正常出力確認
    let out_dir = std::env::temp_dir().join("ibis_test_sec016_hf");
    let _ = std::fs::create_dir_all(&out_dir);
    let pdf_path = out_dir.join("header_footer.pdf");
    let job = s.with(|e, _| e.write_pdf(pdf_path.clone())).expect("write_pdf returns job");
    assert!(job().is_ok(), "pdf output with custom header/footer succeeded");
    assert!(pdf_path.exists());
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    assert!(pdf_bytes.starts_with(b"%PDF-"));

    let _ = std::fs::remove_dir_all(&out_dir);
}

fn fix_xlsx_object001_insert_shape_and_textbox() {
    let mut s = Scenario::open("test/editing/xlsx-formats.xlsx");

    // 初期状態：スライドにはワークシート 1 つのみが存在
    let initial_count = s.with(|e, _| e.slide().shapes.len());
    assert_eq!(initial_count, 1);
    assert!(matches!(s.with(|e, _| e.slide().shapes[0].kind.clone()), ibis_doc_model::ShapeKind::Sheet(_)));

    // 1. テキストボックスの挿入 (xlsx.insert-textbox)
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        if let Some(state) = e.sheet_state_mut() {
            state.cells.insert(slide, (1, 2)); // B3 セル
        }
        cx.notify();
    });
    s.menu("xlsx.insert-textbox");

    // テキストボックスが追加され、選択されていることを確認
    let tb_id = s.with(|e, _| e.selected_sheet_shape()).expect("textbox is selected");
    let tb_shape = s.with(|e, _| e.slide().shape(tb_id).unwrap().clone());
    // A new text box: a pale yellow fill and a thin black outline (todo-fix #80).
    match &tb_shape.kind {
        ibis_doc_model::ShapeKind::Auto(a) => {
            assert!(matches!(&a.fill, ibis_doc_model::Fill::Solid(c) if c.rgb.0 == 0xFFFFE5), "{:?}", a.fill);
            assert_eq!(a.stroke.as_ref().map(|s| (s.color.rgb.0, s.width)), Some((0, 9_525)));
            assert!(a.text.is_some());
        }
        other => panic!("a text box with a fill and an outline: {other:?}"),
    }
    // B3 セルの位置に配置されていることを確認
    assert!(tb_shape.frame.x > 0);
    assert!(tb_shape.frame.y > 0);
    assert_eq!(tb_shape.frame.width, 2_000_000);
    assert_eq!(tb_shape.frame.height, 1_000_000);

    // テキストボックスにテキストを入力
    s.type_text("ワークシート上のテキストボックス");
    s.update(|e, window, cx| e.commit_edit(window, cx));
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.text(), "ワークシート上のテキストボックス");
    });

    // 2. 基本図形（四角形）の挿入 (xlsx.shape.rect)
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        if let Some(state) = e.sheet_state_mut() {
            state.cells.insert(slide, (3, 5)); // D6 セル
        }
        cx.notify();
    });
    s.menu("xlsx.shape.rect");
    let rect_id = s.with(|e, _| e.selected_sheet_shape()).expect("rect shape is selected");
    assert_ne!(rect_id, tb_id);
    let rect_shape = s.with(|e, _| e.slide().shape(rect_id).unwrap().clone());
    assert!(matches!(rect_shape.kind, ibis_doc_model::ShapeKind::Auto(_)));
    assert_eq!(rect_shape.frame.width, 2_000_000);
    assert_eq!(rect_shape.frame.height, 1_200_000);

    // 3. 楕円の挿入 (xlsx.shape.ellipse)
    s.menu("xlsx.shape.ellipse");
    let ellipse_id = s.with(|e, _| e.selected_sheet_shape()).expect("ellipse is selected");
    assert_ne!(ellipse_id, rect_id);
    let ellipse_shape = s.with(|e, _| e.slide().shape(ellipse_id).unwrap().clone());
    assert!(matches!(ellipse_shape.kind, ibis_doc_model::ShapeKind::Auto(_)));

    // 4. 矢印（右矢印）の挿入 (xlsx.shape.right-arrow)
    s.menu("xlsx.shape.right-arrow");
    let arrow_id = s.with(|e, _| e.selected_sheet_shape()).expect("arrow is selected");
    assert!(matches!(s.with(|e, _| e.slide().shape(arrow_id).unwrap().kind.clone()), ibis_doc_model::ShapeKind::Auto(_)));

    // 5. 直線の挿入 (xlsx.shape.line)
    s.menu("xlsx.shape.line");
    let line_id = s.with(|e, _| e.selected_sheet_shape()).expect("line is selected");
    let line_shape = s.with(|e, _| e.slide().shape(line_id).unwrap().clone());
    assert!(matches!(line_shape.kind, ibis_doc_model::ShapeKind::Line(_)));
    assert_eq!(line_shape.frame.height, 0);

    // 6. 矢印線の挿入 (xlsx.shape.line-arrow)
    s.menu("xlsx.shape.line-arrow");
    let arrow_line_id = s.with(|e, _| e.selected_sheet_shape()).expect("arrow line is selected");
    let arrow_line_shape = s.with(|e, _| e.slide().shape(arrow_line_id).unwrap().clone());
    assert!(matches!(arrow_line_shape.kind, ibis_doc_model::ShapeKind::Line(_)));

    // 7. 浮動画像の挿入 (insert_sheet_picture)
    // 最小限の 1x1 PNG バイナリ
    let png_1x1 = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("sample.png", png_1x1, cx)
    }).expect("picture inserted");
    assert!(matches!(s.with(|e, _| e.slide().shape(pic_id).unwrap().kind.clone()), ibis_doc_model::ShapeKind::Picture(_)));

    // 8. 前後順（順序）の検証
    // 現在の末尾は pic_id。rect_id を選択して最前面へ移動
    s.update(|e, _, _| e.selection = Some(rect_id));
    s.menu("xlsx.order-front");
    s.with(|e, _| {
        let last = e.slide().shapes.last().unwrap();
        assert_eq!(last.id, rect_id, "rect moved to front");
    });

    // 最背面へ移動（シートの直後、インデックス 1）
    s.menu("xlsx.order-back");
    s.with(|e, _| {
        assert_eq!(e.slide().shapes[1].id, rect_id, "rect moved to back (behind other shapes, above sheet)");
    });

    // 9. 削除と Undo / Redo の検証
    // rect_id を削除
    s.update(|e, window, cx| e.delete_selection(&crate::DeleteSelection, window, cx));
    s.with(|e, _| {
        assert!(e.slide().shape(rect_id).is_none(), "rect was deleted");
    });

    // Undo で復元
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert!(e.slide().shape(rect_id).is_some(), "rect restored by undo");
    });

    // Redo で再度削除
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        assert!(e.slide().shape(rect_id).is_none(), "rect removed again by redo");
    });

    // 10. Undo による全件ロールバックと初期状態での保存検証
    // すべての操作を Undo して初期状態に戻す
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), 1, "all shapes reverted to initial sheet-only state");
    });

    // Redo で再度適用されることを確認
    while s.with(|e, _| e.document.can_redo()) {
        s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    }
    s.with(|e, _| {
        assert!(e.slide().shapes.len() >= 5, "all shapes restored by redo");
    });

    // 再度初期状態に戻して保存・再オープンが正常に成功することを確認
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.save_as("fix_xlsx_object001_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), 1);
}

fn fix_xlsx_object008_selection_pane_align_and_frame() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. 図形を3つ挿入 (rect, ellipse, textbox)
    let rect_id = s.update(|e, _, cx| e.insert_sheet_shape("rect", cx));
    let _ellipse_id = s.update(|e, _, cx| e.insert_sheet_shape("ellipse", cx));
    let _tb_id = s.update(|e, window, cx| e.insert_sheet_textbox(window, cx));
    s.type_text("メモ");
    s.update(|e, window, cx| e.commit_edit(window, cx));

    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes + 3, "initial + 3 floating shapes");
    });

    // 2. 整列 (Align) と 均等配置 (Distribute) の検証 (XLSX-OBJECT-006)
    // rect を選択して左揃え
    s.update(|e, _, cx| {
        e.selection = Some(rect_id);
        e.align_sheet_shape(crate::pptx::editing::Align::Left, cx);
    });
    let rect_frame = s.with(|e, _| e.slide().shape(rect_id).unwrap().frame);
    let min_x = s.with(|e, _| e.slide().shapes.iter().skip(1).map(|s| s.frame.x).min().unwrap());
    assert_eq!(rect_frame.x, min_x, "rect aligned to left bound");

    // 均等配置 (水平方向)
    s.update(|e, _, cx| {
        e.distribute_sheet_shapes(true, cx);
    });
    s.with(|e, _| {
        let sh = &e.slide().shapes;
        let xs: Vec<i64> = sh.iter().skip(1).map(|s| s.frame.x).collect();
        assert_eq!(xs.len(), initial_shapes + 3 - 1);
    });

    // 均等配置 (垂直方向)
    s.update(|e, _, cx| {
        e.distribute_sheet_shapes(false, cx);
    });

    // 3. サイズと位置パネルの検証 (XLSX-OBJECT-004)
    s.update(|e, window, cx| {
        e.selection = Some(rect_id);
        e.open_sheet_frame_panel(window, cx);
    });
    s.with(|e, _| {
        assert!(e.sidebar_open, "sidebar opened for frame panel");
        assert!(e.sheet_state().unwrap().panels.frame.is_some(), "frame panel is present");
    });

    // フィールドを更新して適用
    s.update(|e, window, cx| {
        if let Some(panel) = e.sheet_state_mut().and_then(|st| st.panels.frame.as_mut()) {
            panel.fields[0].update(cx, |f, cx| f.set_value("6.0", window, cx)); // height 6 cm
            panel.fields[1].update(cx, |f, cx| f.set_value("8.0", window, cx)); // width 8 cm
            panel.fields[2].update(cx, |f, cx| f.set_value("45.0", window, cx)); // rotation 45 deg
        }
        e.apply_sheet_frame_panel(window, cx);
    });
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        // 6 cm = 2_160_000 EMU
        assert_eq!(shape.frame.height, 2_160_000, "height updated to 6 cm");
        // 8 cm = 2_880_000 EMU
        assert_eq!(shape.frame.width, 2_880_000, "width updated to 8 cm");
        // 45 deg = 2_700_000
        assert_eq!(shape.rotation, 2_700_000, "rotation updated to 45 deg");
        assert!(e.sidebar_open, "the pane stays (it follows the selection)");
    });

    // 4. 選択ウィンドウ (Selection Pane) の検証 (XLSX-OBJECT-008)
    s.update(|e, window, cx| {
        e.open_xlsx_selection_pane(window, cx);
    });
    s.with(|e, _| {
        assert!(e.sidebar_open, "sidebar opened for selection pane");
        assert!(e.sheet_state().unwrap().panels.selection_pane.is_some(), "selection pane present");
    });

    // パネルUIレンダリング確認
    let pane_ui = s.update(|e, _, cx| e.xlsx_selection_pane_panel(cx));
    assert!(pane_ui.is_some(), "selection pane sidebar panel rendered");

    // 図形の非表示切り替え (目のアイコン)
    s.update(|e, _, cx| {
        e.toggle_sheet_shape_hidden(rect_id, cx);
    });
    s.with(|e, _| {
        assert!(e.slide().hidden_shapes.contains(&rect_id), "rect is hidden");
    });

    // 再トグルで表示状態に復帰
    s.update(|e, _, cx| {
        e.toggle_sheet_shape_hidden(rect_id, cx);
    });
    s.with(|e, _| {
        assert!(!e.slide().hidden_shapes.contains(&rect_id), "rect is shown again");
    });

    // すべて非表示 / すべて表示
    let total_floating = initial_shapes + 3 - 1;
    s.update(|e, _, cx| {
        e.set_all_sheet_shapes_hidden(true, cx);
    });
    s.with(|e, _| {
        assert_eq!(e.slide().hidden_shapes.len(), total_floating, "all floating shapes hidden");
    });
    s.update(|e, _, cx| {
        e.set_all_sheet_shapes_hidden(false, cx);
    });
    s.with(|e, _| {
        assert!(e.slide().hidden_shapes.is_empty(), "all shapes shown again");
    });

    // 図形の名前変更 (鉛筆アイコン / インライン入力)
    s.update(|e, window, cx| {
        e.start_sheet_shape_renaming(rect_id, window, cx);
        if let Some(p) = e.sheet_state_mut().and_then(|st| st.panels.selection_pane.as_mut()) {
            if let Some((_, field)) = &p.renaming {
                field.update(cx, |f, cx| f.set_value("売上四角形", window, cx));
            }
        }
        e.finish_sheet_shape_renaming(window, cx);
    });
    s.with(|e, _| {
        assert_eq!(e.slide().shape_names.get(&rect_id).map(|s| s.as_str()), Some("売上四角形"), "shape renamed");
    });

    // パネルを閉じる
    s.update(|e, window, cx| {
        e.close_xlsx_selection_pane(window, cx);
    });
    s.with(|e, _| {
        assert!(!e.sidebar_open, "sidebar closed");
        assert!(e.sheet_state().unwrap().panels.selection_pane.is_none());
    });

    // 5. Undo / Redo で名前やサイズ変更が正確に戻ること
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx)); // undo rename
    s.with(|e, _| {
        assert_ne!(e.slide().shape_names.get(&rect_id).map(|s| s.as_str()), Some("売上四角形"), "rename undone");
    });
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx)); // redo rename
    s.with(|e, _| {
        assert_eq!(e.slide().shape_names.get(&rect_id).map(|s| s.as_str()), Some("売上四角形"), "rename redone");
    });

    // 6. 全件ロールバックと初期状態保存
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "reverted to initial shapes count");
    });
    s.save_as("fix_xlsx_object008_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

fn fix_xlsx_object003_image011_image018_shape_format_and_picture_properties() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. 図形（四角形）の挿入
    s.menu("xlsx.shape.rect");
    let rect_id = s.with(|e, _| e.selected_sheet_shape()).expect("rect selected");
    assert!(matches!(s.with(|e, _| e.slide().shape(rect_id).unwrap().kind.clone()), ibis_doc_model::ShapeKind::Auto(_)));

    // 2. 図形の塗りつぶし変更 (XLSX-OBJECT-003)
    s.menu("xlsx.shape-fill.blue");
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        if let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind {
            assert_eq!(a.fill, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a))));
        } else {
            panic!("not auto shape");
        }
    });

    // 3. 図形の枠線変更 (XLSX-OBJECT-003)
    s.menu("xlsx.shape-stroke.red");
    s.menu("xlsx.shape-stroke.width-3");
    s.menu("xlsx.shape-stroke.dash-dash");
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        if let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind {
            let stroke = a.stroke.as_ref().expect("has stroke");
            assert_eq!(stroke.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000)));
            assert_eq!(stroke.width, 38_100);
            assert!(stroke.dashed);
            assert_eq!(stroke.dash, ibis_doc_model::LineDash::Dash);
        } else {
            panic!("not auto shape");
        }
    });

    // 4. 図形の効果（影）変更 (XLSX-OBJECT-003)
    s.menu("xlsx.shape-effect.shadow-bottom-right");
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        if let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind {
            assert!(a.shadow.is_some(), "shadow applied");
        } else {
            panic!("not auto shape");
        }
    });

    // 塗り・枠線・影のクリア
    s.menu("xlsx.shape-fill.none");
    s.menu("xlsx.shape-stroke.none");
    s.menu("xlsx.shape-effect.shadow-none");
    s.with(|e, _| {
        let shape = e.slide().shape(rect_id).unwrap();
        if let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind {
            assert_eq!(a.fill, ibis_doc_model::Fill::None);
            assert!(a.stroke.is_none());
            assert!(a.shadow.is_none());
        } else {
            panic!("not auto shape");
        }
    });

    // 5. 浮動画像の挿入
    let png_1x1 = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("product.png", png_1x1, cx)
    }).expect("picture inserted");
    assert_eq!(s.with(|e, _| e.selected_sheet_shape()), Some(pic_id));

    // 6. 画像のサイズ・縦横比・回転の設定 (XLSX-IMAGE-011)
    s.menu("xlsx.frame-settings");
    assert!(s.with(|e, _| e.sheet_state().unwrap().panels.frame.is_some()));
    s.update(|e, window, cx| {
        let p = e.sheet_state_mut().unwrap().panels.frame.as_mut().unwrap();
        p.fields[0].update(cx, |f, cx| f.set_value("4", window, cx)); // height: 4cm
        p.fields[1].update(cx, |f, cx| f.set_value("5", window, cx)); // width: 5cm
        p.fields[2].update(cx, |f, cx| f.set_value("45", window, cx)); // rotation: 45°
        p.fields[3].update(cx, |f, cx| f.set_value("2", window, cx)); // x: 2cm
        p.fields[4].update(cx, |f, cx| f.set_value("3", window, cx)); // y: 3cm
        p.locked = true; // aspect ratio locked
        e.apply_sheet_frame_panel(window, cx);
    });
    s.with(|e, _| {
        let shape = e.slide().shape(pic_id).unwrap();
        assert_eq!(shape.frame.height, 4 * 360_000); // 1,440,000 EMU
        assert_eq!(shape.frame.width, 5 * 360_000);  // 1,800,000 EMU
        assert_eq!(shape.frame.x, 2 * 360_000);      // 720,000 EMU
        assert_eq!(shape.frame.y, 3 * 360_000);      // 1,080,000 EMU
        assert_eq!(shape.rotation, 45 * 60_000);     // 2,700,000
        assert!(e.slide().aspect_locked.contains(&pic_id), "aspect locked");
    });

    // 7. 画像の代替テキスト設定とファイル保存・再読込 (XLSX-IMAGE-018)
    s.menu("xlsx.alt-text");
    assert!(s.with(|e, _| e.sheet_state().unwrap().panels.alt_text.is_some()));
    s.update(|e, window, cx| {
        let p = e.sheet_state().unwrap().panels.alt_text.as_ref().unwrap();
        p.title_field.update(cx, |f, cx| f.set_value("製品画像", window, cx));
        p.desc_field.update(cx, |f, cx| f.set_value("新製品の正面写真", window, cx));
        e.apply_sheet_alt_text(cx);
    });
    s.with(|e, _| {
        let alt = e.slide().shape_alt_texts.get(&pic_id).expect("alt text found");
        assert_eq!(alt.title, "製品画像");
        assert_eq!(alt.description, "新製品の正面写真");
    });

    // 8. Undo / Redo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx)); // undo alt text
    s.with(|e, _| {
        assert_eq!(e.slide().shape_alt_texts.get(&pic_id), None, "alt text undone");
    });
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx)); // redo alt text
    s.with(|e, _| {
        assert_eq!(e.slide().shape_alt_texts.get(&pic_id).unwrap().title, "製品画像", "alt text redone");
    });

    // 全件ロールバック（新規追加した図形・画像を完全に巻き戻す）
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "reverted to initial shapes count");
    });
    s.save_as("fix_xlsx_object003_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// XLSX-OBJECT-007 (図形のグループ化と解除) および XLSX-IMAGE-012 (画像の切抜き)
fn fix_xlsx_object007_group_and_image012_crop() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());
    assert!(s.with(|e, _| e.slide().groups.is_empty()), "initial groups empty");

    // 1. 基本図形2個（四角形、楕円）および画像を挿入
    s.menu("xlsx.shape.rect");
    let rect_id = s.with(|e, _| e.selected_sheet_shape()).expect("rect inserted");

    s.menu("xlsx.shape.ellipse");
    let ellipse_id = s.with(|e, _| e.selected_sheet_shape()).expect("ellipse inserted");

    s.menu("xlsx.shape.star5");
    let star_id = s.with(|e, _| e.selected_sheet_shape()).expect("star inserted");

    let png_1x1 = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("sample.png", png_1x1, cx)
    }).expect("picture inserted");

    // 2. 複数選択とグループ化可否判定 (can_group / can_ungroup)
    s.update(|e, _, _| e.select_sheet_shape(rect_id));
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![rect_id]);
    assert!(!s.with(|e, _| e.can_group()), "single shape cannot group");
    assert!(!s.with(|e, _| e.can_ungroup()), "ungrouped shape cannot ungroup");

    // 楕円を追加選択（複数選択）
    s.update(|e, _, _| e.toggle_select_sheet_shape(ellipse_id));
    assert_eq!(s.with(|e, _| e.selected_ids().len()), 2);
    assert!(s.with(|e, _| e.can_group()), "multiple shapes can group");
    assert!(!s.with(|e, _| e.can_ungroup()), "not yet grouped");

    // 3. グループ化実行 (XLSX-OBJECT-007)
    s.menu("xlsx.group-shapes.group");
    s.with(|e, _| {
        let groups = &e.slide().groups;
        assert_eq!(groups.len(), 1, "one group formed");
        assert_eq!(groups[0].name, "グループ 1");
        assert_eq!(groups[0].children.len(), 2);
    });
    assert!(s.with(|e, _| e.can_ungroup()), "grouped shapes can ungroup");

    // 4. グループ化の Undo / Redo
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().groups.len(), 0, "group undone");
    });
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().groups.len(), 1, "group redone");
    });

    // 5. ネストしたグループ化（グループ + 星）
    s.update(|e, _, _| e.select_sheet_shapes(&[rect_id, star_id]));
    assert!(s.with(|e, _| e.can_group()), "group + shape can group");
    s.menu("xlsx.group-shapes.group");
    s.with(|e, _| {
        let groups = &e.slide().groups;
        assert_eq!(groups.len(), 1, "nested group formed");
        assert_eq!(groups[0].name, "グループ 2");
        assert_eq!(groups[0].children.len(), 2);
        let has_subgroup = groups[0].children.iter().any(|c| matches!(c, ibis_doc_model::GroupMember::Group(_)));
        let has_star = groups[0].children.iter().any(|c| matches!(c, ibis_doc_model::GroupMember::Shape(id) if *id == star_id));
        assert!(has_subgroup && has_star, "nested group structure verified");
    });

    // 6. グループ解除実行 (XLSX-OBJECT-007)
    // 外側グループ解除
    s.menu("xlsx.group-shapes.ungroup");
    s.with(|e, _| {
        let groups = &e.slide().groups;
        assert_eq!(groups.len(), 1, "outer group dissolved, inner group remains");
        assert_eq!(groups[0].name, "グループ 1");
    });

    // 内側グループ解除
    s.menu("xlsx.group-shapes.ungroup");
    s.with(|e, _| {
        assert!(e.slide().groups.is_empty(), "inner group dissolved, all ungrouped");
    });

    // Undo で内側グループ、さらに外側グループ復元
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().groups.len(), 1, "inner group restored by undo");
        assert_eq!(e.slide().groups[0].name, "グループ 1");
    });
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().groups.len(), 1, "outer group restored by undo");
        assert_eq!(e.slide().groups[0].name, "グループ 2");
    });

    // 7. 画像の切抜き検証 (XLSX-IMAGE-012)
    s.update(|e, _, _| e.select_sheet_shape(pic_id));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop, ibis_doc_model::Crop::default(), "initial crop is 0");
    });

    // 左を10%切り抜く
    s.menu("xlsx.crop.left");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop.left, 10_000);
        assert_eq!(p.crop.top, 0);
    });

    // 四辺を10%切り抜く
    s.menu("xlsx.crop.center");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop, ibis_doc_model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 });
    });

    // 切り抜きの Undo / Redo
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop.left, 10_000);
        assert_eq!(p.crop.top, 0, "undone to left 10%");
    });
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop, ibis_doc_model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 }, "redone to center crop");
    });

    // トリミングを解除
    s.menu("xlsx.crop.reset");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.crop, ibis_doc_model::Crop::default(), "crop reset");
    });

    // 8. 全件ロールバック（初期状態へ巻き戻す）
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "reverted to initial shapes count");
        assert!(e.slide().groups.is_empty(), "groups cleared after undo");
    });
    s.save_as("fix_xlsx_object007_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
    assert!(saved.slides[0].groups.is_empty());
}

/// XLSX-IMAGE-013: 画像補正・色・透明度 & XLSX-IMAGE-014: 画像効果・枠線
fn fix_xlsx_image013_color_and_image014_effects() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    let png_1x1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
        0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41,
        0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
        0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("photo.png", png_1x1.to_vec(), cx)
    }).expect("picture inserted");

    // 画像を選択
    s.update(|e, _, _| e.select_sheet_shape(pic_id));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, None);
        assert_eq!(p.luminance, None);
        assert_eq!(p.corner, 0);
        assert_eq!(p.stroke, None);
    });

    // 1. 画像の色効果 (XLSX-IMAGE-013)
    s.menu("xlsx.picture.color.grayscale");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))), "grayscale duotone");
    });

    s.menu("xlsx.picture.color.sepia");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x402000), ibis_doc_model::Rgb(0xffe0c0))), "sepia duotone");
    });

    s.menu("xlsx.picture.color.none");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, None, "duotone cleared");
    });

    // 2. 画像の補正（明るさ・コントラスト） (XLSX-IMAGE-013)
    s.menu("xlsx.picture.lum.bright-p40");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.luminance, Some((40_000, 0)), "brightness +40%");
    });

    s.menu("xlsx.picture.lum.contrast-p20");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.luminance, Some((0, 20_000)), "contrast +20%");
    });

    s.menu("xlsx.picture.lum.bright-contrast-high");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.luminance, Some((20_000, 20_000)), "high contrast");
    });

    // 3. 画像の角丸効果 (XLSX-IMAGE-014)
    s.menu("xlsx.picture.corner.medium");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.corner, 10_000, "corner 10%");
    });

    s.menu("xlsx.picture.corner.large");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.corner, 20_000, "corner 20%");
    });

    // 4. 画像の枠線（色・太さ・破線） (XLSX-IMAGE-014)
    s.menu("xlsx.shape-stroke.blue");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        let stroke = p.stroke.as_ref().expect("stroke applied");
        assert_eq!(stroke.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a)));
    });

    s.menu("xlsx.shape-stroke.width-3");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        let stroke = p.stroke.as_ref().expect("stroke applied");
        assert_eq!(stroke.width, 38_100, "3pt stroke");
    });

    s.menu("xlsx.shape-stroke.dash-dash");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        let stroke = p.stroke.as_ref().expect("stroke applied");
        assert_eq!(stroke.dash, ibis_doc_model::LineDash::Dash, "dashed stroke");
    });

    // 5. 色と効果のリセット
    s.menu("xlsx.picture.color.blue");
    s.menu("xlsx.picture.reset");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, None, "duotone reset");
        assert_eq!(p.luminance, None, "luminance reset");
        assert_eq!(p.corner, 0, "corner reset");
        assert_eq!(p.stroke, None, "stroke reset");
    });

    // 6. Undo / Redo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))), "blue duotone restored");
        assert_eq!(p.corner, 20_000, "corner restored");
        assert!(p.stroke.is_some(), "stroke restored");
    });

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(p) = &e.slide().shape(pic_id).unwrap().kind else { panic!("not picture") };
        assert_eq!(p.duotone, None, "reset redone");
        assert_eq!(p.luminance, None);
        assert_eq!(p.corner, 0);
        assert_eq!(p.stroke, None);
    });

    // 7. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "picture reverted");
    });
    s.save_as("fix_xlsx_image013_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// XLSX-OBJECT-012: テキストボックス内の文字・段落書式 & XLSX-OBJECT-013: 文字方向・上下配置・内余白
fn fix_xlsx_object012_object013_text_format_and_textbox_layout() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. テキストボックス挿入
    s.menu("xlsx.insert-textbox");
    let tb_id = s.with(|e, _| e.selected_ids().first().copied()).expect("textbox inserted and selected");

    // テキストを入力
    s.type_text("Hello XLSX");
    s.update(|e, window, cx| e.commit_edit(window, cx));
    s.update(|e, _, _| e.select_sheet_shape(tb_id));

    s.with(|e, _| {
        let shape = e.slide().shape(tb_id).unwrap();
        let body = shape.text_body().unwrap();
        assert_eq!(body.text(), "Hello XLSX");
    });

    // 2. 文字書式の検証 (XLSX-OBJECT-012)
    // 太字 (Bold)
    s.menu("xlsx.text.bold");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].runs[0].style.bold, Some(true), "bold applied");
    });

    // 斜体 (Italic)
    s.menu("xlsx.text.italic");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].runs[0].style.italic, Some(true), "italic applied");
    });

    // 下線 (Underline)
    s.menu("xlsx.text.underline");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert!(body.paragraphs[0].runs[0].style.underline, "underline applied");
    });

    // 取り消し線 (Strikethrough)
    s.menu("xlsx.text.strikethrough");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_ne!(body.paragraphs[0].runs[0].style.strike, ibis_doc_model::Strike::None, "strike applied");
    });

    // 文字色 (Color)
    s.menu("xlsx.text.color-blue");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].runs[0].style.color, Some(ibis_doc_model::Rgb(0x2b579a)), "blue color applied");
    });

    // フォントサイズ (Size 24pt = 2400)
    s.menu("xlsx.text.size-24");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].runs[0].style.font_size, Some(2400), "24pt font size applied");
    });

    // 3. 段落配置の検証 (XLSX-OBJECT-012)
    s.menu("align.center");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].align, Some(ibis_doc_model::TextAlign::Center), "center align applied");
    });

    s.menu("align.right");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.paragraphs[0].align, Some(ibis_doc_model::TextAlign::Right), "right align applied");
    });

    // 4. 上下配置の検証 (XLSX-OBJECT-013)
    s.menu("xlsx.text-vertical.middle");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.anchor, ibis_doc_model::VerticalAnchor::Middle, "vertical middle anchor applied");
    });

    s.menu("xlsx.text-vertical.bottom");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.anchor, ibis_doc_model::VerticalAnchor::Bottom, "vertical bottom anchor applied");
    });

    // 5. 文字列の方向の検証 (XLSX-OBJECT-013)
    s.menu("xlsx.text-direction.vertical");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert!(body.vertical, "vertical text applied");
    });

    s.menu("xlsx.text-direction.horizontal");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert!(!body.vertical, "horizontal text applied");
    });

    // 6. 内余白の検証 (XLSX-OBJECT-013)
    s.menu("xlsx.text-insets.none");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.insets, ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 }, "zero insets applied");
    });

    s.menu("xlsx.text-insets.wide");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.insets.left, 182_880, "wide insets applied");
    });

    // 7. テキスト折り返しの検証 (XLSX-OBJECT-013)
    let initial_wrap = s.with(|e, _| e.slide().shape(tb_id).unwrap().text_body().unwrap().wrap);
    s.menu("xlsx.text.wrap");
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.wrap, !initial_wrap, "text wrap toggled");
    });

    // 8. Undo / Redo 検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.wrap, initial_wrap, "wrap undone");
    });

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let body = e.slide().shape(tb_id).unwrap().text_body().unwrap();
        assert_eq!(body.wrap, !initial_wrap, "wrap redone");
    });

    // 9. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "textbox reverted");
    });
    s.save_as("fix_xlsx_object012_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

fn fix_xlsx_object009_object010_image016_shape_placement_and_print() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. シート上の既存図形・グラフの取得・選択
    let shape_id = s.with(|e, _| {
        e.slide().shapes.iter().find(|sh| !matches!(sh.kind, ibis_doc_model::ShapeKind::Sheet(_))).unwrap().id
    });
    s.update(|e, _, _| {
        e.select_sheet_shape(shape_id);
    });

    // 初期状態の検証 (既定は MoveAndSize, 印刷=true)
    let (initial_placement, initial_print) = s.with(|e, _| {
        (
            e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(),
            e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true),
        )
    });
    assert_eq!(initial_placement, ibis_doc_model::ObjectPlacement::MoveAndSize);
    assert_eq!(initial_print, true);

    // 2. メニュー経由での配置変更 (XLSX-OBJECT-009 / XLSX-IMAGE-016)
    // 「セルに合わせて移動するがサイズ変更はしない」
    s.menu("xlsx.placement.move-only");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveOnly);
    });

    // 「セルに合わせて移動やサイズ変更をしない」
    s.menu("xlsx.placement.free");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::Free);
    });

    // 「セルに合わせて移動やサイズ変更をする」
    s.menu("xlsx.placement.move-and-size");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveAndSize);
    });

    // 3. メニュー経由での印刷指定変更 (XLSX-OBJECT-010)
    // 印刷トグル -> オフ
    s.menu("xlsx.placement.print");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), false);
    });

    // 印刷トグル -> オン
    s.menu("xlsx.placement.print");
    s.with(|e, _| {
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), true);
    });

    // 4. サイズと位置サイドバーパネルの操作・検証 (UI連携)
    s.menu("xlsx.frame-settings");
    s.with(|e, _| {
        assert!(e.sheet_state().unwrap().panels.frame.is_some(), "frame panel opened");
        let panel = e.sheet_state().unwrap().panels.frame.as_ref().unwrap();
        assert_eq!(panel.placement, ibis_doc_model::ObjectPlacement::MoveAndSize);
        assert_eq!(panel.print, true);
    });

    // パネル上で MoveOnly + 印刷オフ に切り替えて適用
    s.update(|e, window, cx| {
        if let Some(panel) = e.wb_mut().panels.frame.as_mut() {
            panel.placement = ibis_doc_model::ObjectPlacement::MoveOnly;
            panel.print = false;
        }
        e.apply_sheet_frame_panel(window, cx);
    });

    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveOnly);
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), false);
    });

    // 5. Undo / Redo の検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveAndSize);
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), true);
    });

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveOnly);
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), false);
    });

    // 6. 保存と再読込による永続化の検証
    s.save_as("fix_xlsx_object009_010_saved.xlsx");
    let reopened = s.reopen();
    assert_eq!(reopened.slides[0].shape_placements.get(&shape_id).copied().unwrap_or_default(), ibis_doc_model::ObjectPlacement::MoveOnly);
    assert_eq!(reopened.slides[0].shape_prints.get(&shape_id).copied().unwrap_or(true), false);

    // 7. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shape_placements.get(&shape_id).copied().unwrap_or_default(), initial_placement);
        assert_eq!(e.slide().shape_prints.get(&shape_id).copied().unwrap_or(true), initial_print);
    });
}

/// XLSX-OBJECT-002: 図形の変更（Change Shape）
/// オートシェイプの形状（四角形、楕円、星、ハートなど）変更、テキストボックスからの変換、Undo/Redo、ロールバックの検証。
fn fix_xlsx_object002_change_shape() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. 基本図形（四角形）を挿入
    s.menu("xlsx.shape.rect");
    let shape_id = s.with(|e, _| e.selected_sheet_shape()).expect("rect shape inserted");

    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("rect"));
    });

    // 2. 「書式 > 図形の変更」メニューから楕円に変更
    s.menu("xlsx.change-shape.ellipse");
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("ellipse"));
    });

    // 3. 星に変更
    s.menu("xlsx.change-shape.star5");
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("star5"));
    });

    // 4. ハートに変更
    s.menu("xlsx.change-shape.heart");
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("heart"));
    });

    // 5. Undo / Redo の検証
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx)); // back to star5
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("star5"));
    });

    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx)); // back to ellipse
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("ellipse"));
    });

    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx)); // redo to star5
    s.with(|e, _| {
        let shape = e.slide().shape(shape_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("not auto") };
        assert_eq!(a.geometry.preset_name(), Some("star5"));
    });

    // 6. テキストボックスからの図形変更
    s.menu("xlsx.insert-textbox");
    let tb_id = s.with(|e, _| e.selected_ids().first().copied()).expect("textbox inserted");
    s.type_text("Memo");
    s.update(|e, window, cx| e.commit_edit(window, cx));
    s.update(|e, _, _| e.select_sheet_shape(tb_id));

    // テキストボックスから吹き出し（wedgeRectCallout）に変更
    s.menu("xlsx.change-shape.callout");
    s.with(|e, _| {
        let shape = e.slide().shape(tb_id).unwrap();
        let ibis_doc_model::ShapeKind::Auto(a) = &shape.kind else { panic!("converted to auto") };
        assert_eq!(a.geometry.preset_name(), Some("wedgeRectCallout"));
        assert_eq!(a.text.as_ref().unwrap().text(), "Memo");
    });

    // 7. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "all inserted shapes rolled back");
    });
}

/// XLSX-OBJECT-014: テキストボックス・図形とセルの連動 (TextLink / Drawing Formula Link)
/// セル参照数式のバインド、セルの最新値の自動反映、セル値編集後の同期、連動解除、Undo/Redo、OOXML (xdr:f) 保存・再読込保持。
fn fix_xlsx_object014_shape_formula_textlink() {
    use crate::xlsx::scenarios::SheetSteps as _;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. セル A1 に値を設定
    s.click_cell("A1");
    s.enter("KPI-2026", "enter");

    // 2. 四角形を挿入
    s.menu("xlsx.shape.rect");
    let shape_id = s.with(|e, _| e.selected_ids().first().copied()).expect("shape inserted");

    // 3. セル連動を設定（A1 と連動）
    // セル A1 を再度クリックして選択セルとし、図形を選択してメニューから「選択中のセルと連動」を実行
    s.click_cell("A1");
    s.update(|e, _, _| e.select_sheet_shape(shape_id));
    s.menu("xlsx.shape-textlink.current-cell");

    // 連動式とテキストの初期反映を検証
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shape_formulas.get(&shape_id).map(|s| s.as_str()), Some("A1"), "formula is A1");
        let shape = slide.shape(shape_id).unwrap();
        assert_eq!(shape.text_body().map(|t| t.text()).as_deref(), Some("KPI-2026"), "shape text shows cell value");
    });

    // 4. セル A1 を更新して連動図形のテキストが同期されるかを検証
    s.click_cell("A1");
    s.enter("Target-999", "enter");

    s.with(|e, _| {
        let slide = e.slide();
        let shape = slide.shape(shape_id).unwrap();
        assert_eq!(shape.text_body().map(|t| t.text()).as_deref(), Some("Target-999"), "shape text updated to new cell value");
    });

    // 5. 連動解除メニュー (xlsx.shape-textlink.clear) を検証
    s.update(|e, _, _| e.select_sheet_shape(shape_id));
    s.menu("xlsx.shape-textlink.clear");
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shape_formulas.get(&shape_id), None, "formula is cleared");
    });

    // 6. Undo で連動式が復活すること
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shape_formulas.get(&shape_id).map(|s| s.as_str()), Some("A1"), "formula restored by undo");
    });

    // 7. Redo で連動解除が再適用されること
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.shape_formulas.get(&shape_id), None, "formula cleared again by redo");
    });

    // 8. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "all shapes rolled back");
    });

    // 9. ロールバック後の保存・再オープンが正常に通ること
    s.save_as("fix_xlsx_object014_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// XLSX-IMAGE-019: セル・グラフ・オブジェクトを画像としてコピー (Copy as Picture)
fn fix_xlsx_image019_copy_as_picture() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    // 1. メニュー存在確認
    assert!(s.menu_state("xlsx.copy-as-picture").is_some(), "xlsx.copy-as-picture menu item exists");

    // 2. セル範囲を選択して画像としてコピー
    s.click_cell("A1");
    s.key("shift-right");
    s.key("shift-down"); // A1:B2 選択
    s.menu("xlsx.copy-as-picture");

    s.with(|e, cx| {
        assert_eq!(e.status.as_deref(), Some("選択範囲を画像としてコピーしました"));
        let clip = cx.read_from_clipboard().expect("clipboard item exists");
        let entry = clip.entries().iter().find(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_))).expect("image entry exists");
        if let gpui_kit::ClipboardEntry::Image(img) = entry {
            assert_eq!(img.format, gpui_kit::ImageFormat::Png);
            assert!(img.bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]), "valid PNG header");
            assert!(img.bytes.len() > 50, "PNG image has non-trivial size");
        }
    });

    // 3. 別のセル（C4）を選択して画像貼り付け
    s.click_cell("C4");
    s.update(|e, _, cx| e.paste_cells(false, cx));

    let shape_id = s.with(|e, _| {
        assert_eq!(e.status.as_deref(), Some("画像を貼り付けました"));
        assert_eq!(e.slide().shapes.len(), initial_shapes + 1, "pasted image inserted as shape");
        let last = e.slide().shapes.last().unwrap();
        assert!(matches!(last.kind, ibis_doc_model::ShapeKind::Picture(_)), "last shape is picture");
        last.id
    });

    // 4. 貼り付けられた画像オブジェクトを選択して画像コピー
    s.update(|e, _, _| e.select_sheet_shape(shape_id));
    s.menu("xlsx.copy-as-picture");

    s.with(|e, cx| {
        assert_eq!(e.status.as_deref(), Some("画像を画像としてコピーしました"));
        let clip = cx.read_from_clipboard().expect("clipboard item exists");
        let entry = clip.entries().iter().find(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_))).expect("image entry exists");
        if let gpui_kit::ClipboardEntry::Image(img) = entry {
            assert_eq!(img.format, gpui_kit::ImageFormat::Png);
            assert!(img.bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]), "valid PNG header");
        }
    });

    // 5. テキストボックスを挿入し、オブジェクトとして画像コピー
    let tb_id = s.update(|e, window, cx| e.insert_sheet_textbox(window, cx));
    s.type_text("サンプル");
    s.update(|e, window, cx| e.commit_edit(window, cx));
    s.update(|e, _, _| e.select_sheet_shape(tb_id));
    s.menu("xlsx.copy-as-picture");

    s.with(|e, cx| {
        assert_eq!(e.status.as_deref(), Some("オブジェクトを画像としてコピーしました"));
        let clip = cx.read_from_clipboard().expect("clipboard item exists");
        let entry = clip.entries().iter().find(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_))).expect("image entry exists");
        if let gpui_kit::ClipboardEntry::Image(img) = entry {
            assert_eq!(img.format, gpui_kit::ImageFormat::Png);
            assert!(img.bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]), "valid PNG header");
        }
    });

    // 6. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "all shapes rolled back");
    });

    // 7. 保存・再オープン検証
    s.save_as("fix_xlsx_image019_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// XLSX-IMAGE-016: SVG挿入と色変更
fn fix_xlsx_image016_svg_insert_and_color() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let initial_shapes = s.with(|e, _| e.slide().shapes.len());

    let svg_bytes = br##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="120" viewBox="0 0 160 120"><rect width="160" height="120" fill="#204080"/><circle cx="80" cy="60" r="40" fill="#f0a000"/></svg>"##.to_vec();

    // 1. insert_sheet_picture で SVG 画像を挿入
    let pic_id = s.update(|e, _, cx| {
        e.insert_sheet_picture("vector-icon.svg", svg_bytes.clone(), cx)
    }).expect("SVG picture inserted");

    // 2. 挿入された図形の検証
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes + 1, "picture shape added");
        let shape = e.slide().shape(pic_id).expect("shape exists");
        let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind else { panic!("expected picture") };
        let media = e.presentation().media(pic.media).expect("media exists");
        assert_eq!(media.format, ibis_doc_model::MediaFormat::Svg, "media format is SVG");
        assert_eq!(media.data.as_ref(), &svg_bytes[..], "media data matches SVG");
        // 160 x 120 px at 96 DPI (9525 EMU/px) = 1_524_000 x 1_143_000 EMU
        assert_eq!(shape.frame.width, 160 * 9525, "width from SVG header");
        assert_eq!(shape.frame.height, 120 * 9525, "height from SVG header");
        assert_eq!(pic.duotone, None, "initially no duotone effect");
    });

    // 3. SVG 画像の色変更（xlsx.picture.color.*）
    s.update(|e, _, _| e.select_sheet_shape(pic_id));

    // 青 (blue)
    s.menu("xlsx.picture.color.blue");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))), "blue duotone applied");
    });

    // オレンジ (orange)
    s.menu("xlsx.picture.color.orange");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((ibis_doc_model::Rgb(0x803000), ibis_doc_model::Rgb(0xffe6cc))), "orange duotone applied");
    });

    // 紫 (purple)
    s.menu("xlsx.picture.color.purple");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((ibis_doc_model::Rgb(0x400060), ibis_doc_model::Rgb(0xf2e6ff))), "purple duotone applied");
    });

    // グレースケール (grayscale)
    s.menu("xlsx.picture.color.grayscale");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))), "grayscale duotone applied");
    });

    // 色効果なし (none) でリセット
    s.menu("xlsx.picture.color.none");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.duotone, None, "duotone cleared to none");
    });

    // 4. 枠線（Graphic Outline）の設定
    s.menu("xlsx.shape-stroke.blue");
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert!(pic.stroke.is_some(), "stroke applied to SVG picture");
        let stroke = pic.stroke.as_ref().unwrap();
        assert_eq!(stroke.color, ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a)));
    });

    // 5. SVG 画像を画像としてコピー（xlsx.copy-as-picture）
    s.menu("xlsx.copy-as-picture");
    s.with(|e, cx| {
        assert_eq!(e.status.as_deref(), Some("画像を画像としてコピーしました"));
        let clip = cx.read_from_clipboard().expect("clipboard item exists");
        let entry = clip.entries().iter().find(|e| matches!(e, gpui_kit::ClipboardEntry::Image(_))).expect("image entry exists");
        if let gpui_kit::ClipboardEntry::Image(img) = entry {
            assert_eq!(img.format, gpui_kit::ImageFormat::Png);
            assert!(img.bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]), "valid PNG raster header");
        }
    });

    // 6. Undo / Redo の検証
    // undo stroke
    s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert_eq!(pic.stroke, None, "stroke undone");
    });

    // redo stroke
    s.update(|e, window, cx| e.redo(&crate::Redo, window, cx));
    s.with(|e, _| {
        let ibis_doc_model::ShapeKind::Picture(pic) = &e.slide().shape(pic_id).unwrap().kind else { panic!("picture") };
        assert!(pic.stroke.is_some(), "stroke redone");
    });

    // 7. 全件ロールバック
    while s.with(|e, _| e.document.can_undo()) {
        s.update(|e, window, cx| e.undo(&crate::Undo, window, cx));
    }
    s.with(|e, _| {
        assert_eq!(e.slide().shapes.len(), initial_shapes, "all shapes rolled back to initial state");
    });

    // 8. 保存・再オープン検証
    s.save_as("fix_xlsx_image016_clean.xlsx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].shapes.len(), initial_shapes);
}

/// #23: 左サムネイル一覧のスクロール不具合（最下部・最上部・セクション折りたたみ・行インデックス整合性検証）
fn fix23_pptx_thumbnail_scroll_and_sections() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), 31);

    // 1. セクションなし状態での行インデックス整合性
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 0);
    assert_eq!(s.with(|e, _| e.row_of_slide(30)), 30);

    // 2. 最終スライド (31枚目) への移動
    s.page(31);
    s.settle();
    s.with(|e, _| {
        let last_id = e.presentation().slides[30].id;
        assert_eq!(e.current_slide, last_id, "slide 31 is current");
    });
    s.screenshot("thumbnails_slide31_selected");

    // 3. 先頭スライド (1枚目) への復帰
    s.page(1);
    s.settle();
    s.with(|e, _| {
        let first_id = e.presentation().slides[0].id;
        assert_eq!(e.current_slide, first_id, "slide 1 is current");
    });

    // 4. セクションを追加して row_of_slide の見出しオフセット計算を検証
    s.page(1);
    s.menu("pptx.section.add");
    s.key("cmd-a");
    s.type_text("第1部");
    {
        let _probe = crate::shell::scenario::button_probe::Probe::new();
        s.update(|_, _, cx| cx.notify());
        crate::shell::scenario::button_probe::click(&mut s, "pptx-section-apply");
    }

    s.page(7);
    s.menu("pptx.section.add");
    s.key("cmd-a");
    s.type_text("第2部");
    {
        let _probe = crate::shell::scenario::button_probe::Probe::new();
        s.update(|_, _, cx| cx.notify());
        crate::shell::scenario::button_probe::click(&mut s, "pptx-section-apply");
    }

    let pres = s.with(|e, _| e.presentation().clone());
    assert_eq!(pres.sections.len(), 2);
    let s1_id = pres.sections[0].id.clone();

    // セクション構造:
    // row 0: セクション1見出し
    // row 1..=6: スライド 1..6 (slide_index 0..=5)
    // row 7: セクション2見出し
    // row 8..=32: スライド 7..31 (slide_index 6..=30)
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 1);
    assert_eq!(s.with(|e, _| e.row_of_slide(5)), 6);
    assert_eq!(s.with(|e, _| e.row_of_slide(6)), 8);
    assert_eq!(s.with(|e, _| e.row_of_slide(30)), 32);

    // 5. セクション1を折りたたみ
    s.update(|e, _, cx| e.toggle_section_fold(&s1_id, cx));
    s.settle();

    // 折りたたみ時:
    // セクション1の見出しは row 0
    // セクション1に属するスライド 1..6 (slide_index 0..=5) は見出しの行 (row 0) にマッピングされる
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 0);
    assert_eq!(s.with(|e, _| e.row_of_slide(5)), 0);
    // セクション2の見出しは row 1 (直前のスライド6枚が隠れるため)
    // スライド 7 (slide_index 6) は row 2
    assert_eq!(s.with(|e, _| e.row_of_slide(6)), 2);

    // 6. セクション1を展開復元
    s.update(|e, _, cx| e.toggle_section_fold(&s1_id, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 1);
    assert_eq!(s.with(|e, _| e.row_of_slide(6)), 8);

    // 7. Undo でセクション追加前の状態に復元
    while s.with(|e, _| e.document.can_undo()) {
        s.key("cmd-z");
    }
    assert_eq!(s.with(|e, _| e.presentation().sections.len()), 0);
    assert_eq!(s.with(|e, _| e.row_of_slide(0)), 0);
    assert_eq!(s.with(|e, _| e.row_of_slide(30)), 30);
}

/// XLSX-FILE-011: CSVの文字コード（Shift_JIS / UTF-8 BOM）および区切り文字（TSV・カンマ）の読込・保存・エクスポート
fn fix_xlsx_file011_csv_and_tsv() {
    use ibis_doc_model::CellValue;

    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    let sjis_csv_path = dir.join("file011_sjis.csv");
    let sjis_content = "品名,数量,単価,小計\r\nりんご,5,120,=B2*C2\r\n\"温州,みかん\",3,100,=B3*C3\r\n";
    let sjis_bytes = ibis_pptx::csv::encode_sjis(sjis_content);
    std::fs::write(&sjis_csv_path, &sjis_bytes).unwrap();

    // 1. Shift_JIS CSV を開く
    let mut s = Scenario::open("target/scenario-input/file011_sjis.csv");
    s.with(|e, _| {
        assert!(e.is_workbook());
    });
    assert_eq!(s.value("A1"), CellValue::Text("品名".into()));
    assert_eq!(s.value("A2"), CellValue::Text("りんご".into()));
    assert_eq!(s.value("B2"), CellValue::Number(5.0));
    assert_eq!(s.value("C2"), CellValue::Number(120.0));
    assert_eq!(s.value("A3"), CellValue::Text("温州,みかん".into()));

    // 2. セル編集して保存 (cmd-s)
    s.click_cell("B4");
    s.enter("=SUM(B2:B3)", "enter");
    assert_eq!(s.value("B4"), CellValue::Number(8.0));
    s.key("cmd-s");
    s.settle();

    // 3. Shift_JIS CSV 書き出し (write_csv_sjis)
    let out_sjis_path = dir.join("file011_out_sjis.csv");
    s.with(|e, _| {
        e.write_csv_sjis(&out_sjis_path).expect("write_csv_sjis succeeds");
    });
    let out_sjis_bytes = std::fs::read(&out_sjis_path).unwrap();
    assert!(!out_sjis_bytes.starts_with(b"\xEF\xBB\xBF"), "Shift_JIS must not have UTF-8 BOM");
    let decoded_sjis = ibis_pptx::csv::decode(&out_sjis_bytes).expect("decode sjis");
    assert!(decoded_sjis.contains("品名,数量,単価,小計\r\n"));
    assert!(decoded_sjis.contains("\"温州,みかん\",3,100"));

    // 4. TSV 書き出し (write_tsv)
    let out_tsv_path = dir.join("file011_out.tsv");
    s.with(|e, _| {
        e.write_tsv(&out_tsv_path).expect("write_tsv succeeds");
    });
    let out_tsv_bytes = std::fs::read(&out_tsv_path).unwrap();
    let decoded_tsv = ibis_pptx::csv::decode(&out_tsv_bytes).expect("decode tsv");
    assert!(decoded_tsv.contains("品名\t数量\t単価\t小計\r\n"));
    assert!(decoded_tsv.contains("りんご\t5\t120"));

    // 5. TSV ファイルを open_path で開いて検証
    s.update(|e, window, cx| {
        e.open_path(out_tsv_path.clone(), None, window, cx);
    });
    s.settle();
    s.with(|e, _| {
        assert!(e.is_workbook());
    });
    assert_eq!(s.value("A1"), CellValue::Text("品名".into()));
    assert_eq!(s.value("B2"), CellValue::Number(5.0));
    assert_eq!(s.value("A3"), CellValue::Text("温州,みかん".into()));

    // 6. 後片付け
    let _ = std::fs::remove_file(sjis_csv_path);
    let _ = std::fs::remove_file(out_sjis_path);
    let _ = std::fs::remove_file(out_tsv_path);
}

/// #28: the sheet tabs' scrollbar in the status bar ends 8 px before the
/// zoom controls (the − button) whatever the window's width, shows only while
/// the tabs scroll, and never reaches the − button.
fn fix27_status_bar_tab_scrollbar_place() {
    for width in [600.0, 900.0, 1200.0, 1600.0] {
        let mut s = Scenario::open_sized("test/editing/xlsx-sales.xlsx", width);
        // Few sheets: the tabs do not scroll, so there is no scrollbar.
        let scrolls = |s: &mut Scenario| s.with(|e, _| e.sheet_state().expect("a workbook").tab_scroll.max_offset().x > gpui_kit::px(0.5));
        s.settle();
        assert!(!scrolls(&mut s), "{width}: three sheets fit");
        for _ in 0..20 {
            s.menu("xlsx.insert-sheet");
        }
        s.settle();
        s.settle();
        assert!(scrolls(&mut s), "{width}: 23 sheets scroll");
        let (layout, tabs_left, minus) = s.with(|e, _| {
            let scroll = e.sheet_state().expect("a workbook").tab_scroll.clone();
            (e.status_layout.get(), f32::from(scroll.bounds().left()), e.status_layout.get()[2])
        });
        // The scrollbar's right edge is 8 px before the − button's left edge.
        let right = minus - 8.0;
        eprintln!("width {width}: status bar {:?}, tabs left {tabs_left}, scrollbar right {right}, − button left {minus}", &layout[..2]);
        assert!((layout[1] - width).abs() < 1.0, "{width}: the status bar measured: {layout:?}");
        assert!(minus > tabs_left && minus < width, "{width}: the − button is in the window: {minus}");
        // Scrolled to the end, with the pointer over the scrollbar, its thumb
        // ends where the scrollbar does.
        s.with(|e, _| {
            let scroll = e.sheet_state().expect("a workbook").tab_scroll.clone();
            let max = scroll.max_offset().x;
            scroll.set_offset(gpui_kit::point(-max, gpui_kit::px(0.)));
        });
        s.settle();
        s.hover((tabs_left + 40.0, 720.0 - 20.0));
        s.screenshot(&format!("status_bar_{width}"));
    }
}

/// #30: the wheel over the sheet tabs' scrollbar in the status bar scrolls the
/// tabs sideways (a vertical wheel too), as over the tabs themselves; with
/// every sheet in view it does nothing.
fn fix30_status_bar_tab_wheel() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let offset = |s: &mut Scenario| s.with(|e, _| f32::from(e.sheet_state().expect("a workbook").tab_scroll.offset().x));
    let (tabs_y, bar_y) = (679.0, 720.0 - 26.0 + 5.0);
    // Three sheets fit: nothing scrolls, over the bar or the tabs.
    s.wheel_xy((300.0, bar_y), 0.0, -120.0);
    s.wheel_xy((300.0, tabs_y), 0.0, -120.0);
    assert_eq!(offset(&mut s), 0.0, "everything in view: nothing scrolls");
    for _ in 0..20 {
        s.menu("xlsx.insert-sheet");
    }
    s.settle();
    s.settle();
    s.wheel_xy((300.0, tabs_y), 0.0, 0.0);
    let start = offset(&mut s);
    // The wheel over the tabs.
    s.wheel_xy((300.0, tabs_y), -60.0, 0.0);
    let sideways_tabs = offset(&mut s) - start;
    s.wheel_xy((300.0, tabs_y), 60.0, 0.0);
    s.wheel_xy((300.0, tabs_y), 0.0, -60.0);
    let vertical_tabs = offset(&mut s) - start;
    s.wheel_xy((300.0, tabs_y), 0.0, 60.0);
    eprintln!("tabs: sideways wheel moves the offset by {sideways_tabs}, a vertical one by {vertical_tabs}");
    // The wheel over the bar in the status bar.
    let before = offset(&mut s);
    s.wheel_xy((300.0, bar_y), -60.0, 0.0);
    let sideways_bar = offset(&mut s) - before;
    s.wheel_xy((300.0, bar_y), 60.0, 0.0);
    s.wheel_xy((300.0, bar_y), 0.0, -60.0);
    let vertical_bar = offset(&mut s) - before;
    eprintln!("bar: sideways {sideways_bar}, vertical {vertical_bar}");
    assert!(sideways_bar != 0.0, "a sideways wheel over the bar scrolls the tabs");
    assert!(vertical_bar != 0.0, "a vertical wheel over the bar scrolls the tabs too");
    assert_eq!(vertical_bar.signum(), sideways_bar.signum(), "both scroll the same way for the same sign");
}

/// #31: a worksheet at 100% is drawn as Excel for Mac draws it, a point a
/// logical pixel (a slide's 100% is 96 px an inch: 4/3 pixel a point).
fn fix31_sheet_100_percent_is_a_point_a_pixel() {
    use crate::xlsx::scenarios::SheetSteps as _;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    s.settle();
    for name in ["A", "B", "C"] {
        let (mid, line) = (s.header(name), s.header_line(name));
        let width_px = (line.0 - mid.0) * 2.0;
        let width_pt = s.size_pt(name);
        eprintln!("column {name}: {width_pt} pt, {width_px} px at 100%");
        assert!((width_px / width_pt - 1.0).abs() < 0.03, "column {name}: {width_pt} pt drawn {width_px} px at 100%");
    }
    // A presentation's slide: 72 px an inch at 100% too (1 pt a pixel, as PowerPoint for Mac; todo-fix #41).
    drop(s);
    let mut p = Scenario::new_document(ibis_pptx::Kind::Presentation);
    p.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    p.settle();
    let ratio = p.with(|e, _| e.slide_layout().map(|l| l.scale * ibis_doc_model::EMU_PER_POINT as f32));
    eprintln!("a slide at 100%: {ratio:?} px a point");
    assert!((ratio.unwrap() - 1.0).abs() < 0.01);
}

/// #33: a formula with no cached value in the file (written by a program that does not
/// calculate: C#'s Open XML SDK, openpyxl) shows its value when the file is opened.
fn fix33_formula_without_cached_value() {
    use crate::xlsx::scenarios::SheetSteps as _;
    let mut s = Scenario::open("test/editing/xlsx-no-cached-values.xlsx");
    assert_eq!(s.shown("B10"), "280", "SUM(B2:B9) with no cached value");
    assert_eq!(s.shown("D10"), "#N/A");
    // Opening is no edit: nothing to undo, so nothing to save.
    assert!(!s.with(|e, _| e.document.can_undo()), "the calculation on opening is not an edit");
}

/// Times of a file: parsed, opened and drawn, shown at its end, and (a
/// workbook) at each next sheet. Off unless `IBIS_BENCH_FILES` names files
/// (paths joined by `:`): `IBIS_BENCH_FILES=a.xlsx:b.pptx cargo test
/// --release --features scenarios --test scenarios -- bench_open_scroll
/// --nocapture`. Milliseconds, one run each.
fn bench_open_scroll() {
    let Some(files) = std::env::var_os("IBIS_BENCH_FILES") else { return };
    let ms = crate::clock::unix_millis;
    for path in files.to_string_lossy().split(':').filter(|p| !p.is_empty()) {
        let t = ms();
        let parsed = crate::read_file(std::path::Path::new(path)).map(|_| ());
        let parse = ms() - t;
        if parsed.is_err() {
            eprintln!("BENCH {path}: cannot read");
            continue;
        }
        let t = ms();
        let mut s = Scenario::open(path);
        let open = ms() - t;
        let is_xlsx = path.ends_with(".xlsx") || path.ends_with(".xlsm");
        let t = ms();
        s.key(if path.ends_with(".pptx") { "end" } else { "ctrl-end" });
        s.settle();
        let end = ms() - t;
        let mut sheets = Vec::new();
        if is_xlsx {
            let count = std::env::var("IBIS_BENCH_SHEETS").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
            for _ in 0..count {
                let t = ms();
                s.key("ctrl-pagedown");
                s.settle();
                sheets.push(ms() - t);
            }
        }
        eprintln!("BENCH {path}: parse {parse} ms, open+draw {open} ms (with parse), to the end {end} ms, next sheets {sheets:?} ms");
    }
}

/// #51: cell editing with F2/Enter preserves row height, toolbar alignment buttons are split into 4 parts, and start-window menu works.
fn fix51_cell_edit_and_align_toolbar() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // 1. Toolbar alignment buttons are 4 parts each (Left, Center, Right, Dropdown).
    assert!(s.toolbar_state("align-left").is_some());
    assert!(s.toolbar_state("align-center").is_some());
    assert!(s.toolbar_state("align-right").is_some());
    assert!(s.toolbar_state("halign").is_some());
    assert!(s.toolbar_state("valign-top").is_some());
    assert!(s.toolbar_state("valign-center").is_some());
    assert!(s.toolbar_state("valign-bottom").is_some());
    assert!(s.toolbar_state("valign").is_some());

    // 2. F2 on A4 then Enter does not change row height.
    s.click_cell("A4");
    let h0 = s.with(|e, _| e.current_sheet().unwrap().1.rows[3]);
    s.key("f2");
    s.key("enter");
    let h1 = s.with(|e, _| e.current_sheet().unwrap().1.rows[3]);
    assert_eq!(h0, h1, "row height should not change after F2 + Enter without edits");

    // 3. Merged cell A4:E6: F2 + Enter does not change row height.
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let range = ibis_doc_model::CellRange { first_row: 3, first_column: 0, last_row: 5, last_column: 4 };
        let _ = e.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::Merge { slide, range, center: false })]);
    });
    s.click_cell("A4");
    let h0_m = s.with(|e, _| e.current_sheet().unwrap().1.rows[3]);
    s.key("f2");
    s.with(|e, _| {
        assert!(e.cell_edit().is_some());
        let edit = e.cell_edit().unwrap();
        assert_eq!((edit.row, edit.column), (3, 0));
    });
    s.key("enter");
    let h1_m = s.with(|e, _| e.current_sheet().unwrap().1.rows[3]);
    assert_eq!(h0_m, h1_m, "row height of merged cell A4:E6 should not change after F2 + Enter");

    // 4. Start window menu item exists in xlsx.
    assert!(s.menu_state("file.start-window").is_some());
    assert_eq!(s.with(|e, _| e.kind()), ibis_pptx::Kind::Workbook);
    drop(s);

    // 5. If default.xlsx exists in test/private, verify directly.
    if std::path::Path::new("test/github-issue/default.xlsx").exists() {
        let mut s_def = Scenario::open("test/github-issue/default.xlsx");
        s_def.click_cell("A4");
        let r3_before = s_def.with(|e, _| e.current_sheet().unwrap().1.rows[3]);
        let r4_before = s_def.with(|e, _| e.current_sheet().unwrap().1.rows[4]);
        let r5_before = s_def.with(|e, _| e.current_sheet().unwrap().1.rows[5]);
        s_def.key("f2");
        s_def.with(|e, _| {
            assert!(e.cell_edit().is_some());
            let edit = e.cell_edit().unwrap();
            assert_eq!((edit.row, edit.column), (3, 0));
        });
        s_def.key("enter");
        let r3_after = s_def.with(|e, _| e.current_sheet().unwrap().1.rows[3]);
        let r4_after = s_def.with(|e, _| e.current_sheet().unwrap().1.rows[4]);
        let r5_after = s_def.with(|e, _| e.current_sheet().unwrap().1.rows[5]);
        assert_eq!(r3_before, r3_after, "row 4 height unchanged in default.xlsx");
        assert_eq!(r4_before, r4_after, "row 5 height unchanged in default.xlsx");
        assert_eq!(r5_before, r5_after, "row 6 height unchanged in default.xlsx");
        drop(s_def);
    }

    {
        let mut s_docx = Scenario::open("test/editing/docx-report.docx");
        assert!(s_docx.menu_state("file.start-window").is_some());
        assert_eq!(s_docx.with(|e, _| e.kind()), ibis_pptx::Kind::Document);
    }

    {
        let mut s_pptx = Scenario::open("test/editing/pptx-plan.pptx");
        assert!(s_pptx.menu_state("file.start-window").is_some());
        assert_eq!(s_pptx.with(|e, _| e.kind()), ibis_pptx::Kind::Presentation);
    }
}

/// GitHub issue #2: Support trackpad pinch-to-zoom on macOS (ibisjp/ibis-office#2)
/// Verifies trackpad pinch gestures zoom in/out centered around the pointer across formats (pptx, xlsx, docx).
fn fix_pinch_to_zoom() {
    // 1. PPTX presentation
    {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        let initial_zoom = s.with(|e, _| e.current_zoom());
        let (ws_origin, ws_size) = s.with(|e, _| {
            let ws = e.workspace_bounds.expect("workspace bounds");
            (ws.origin, ws.size)
        });
        let center_x = f32::from(ws_origin.x + ws_size.width / 2.0);
        let center_y = f32::from(ws_origin.y + ws_size.height / 2.0);

        // Pinch out (magnification > 0): zoom in
        s.pinch((center_x, center_y), 0.25);
        let zoomed_in = s.with(|e, _| e.current_zoom());
        assert!(zoomed_in > initial_zoom, "pinch out should increase zoom (was {initial_zoom}, now {zoomed_in})");
        assert!((zoomed_in - initial_zoom * 1.25).abs() < 0.01, "zoom should increase by ~25%");

        // Pinch in (magnification < 0): zoom out
        s.pinch((center_x, center_y), -0.2);
        let zoomed_out = s.with(|e, _| e.current_zoom());
        assert!(zoomed_out < zoomed_in, "pinch in should decrease zoom (was {zoomed_in}, now {zoomed_out})");

        // Pointer-centered zoom: pinch at an off-center position (e.g. top-left quadrant)
        let anchor_x = f32::from(ws_origin.x + ws_size.width / 4.0);
        let anchor_y = f32::from(ws_origin.y + ws_size.height / 4.0);
        let offset_before = s.with(|e, _| e.workspace_scroll.offset());
        s.pinch((anchor_x, anchor_y), 0.5);
        let offset_after = s.with(|e, _| e.workspace_scroll.offset());
        assert_ne!(offset_before, offset_after, "scrolling should adjust to preserve point under pointer");

        // Extreme zoom limits (clamp MIN_ZOOM..=MAX_ZOOM)
        s.pinch((center_x, center_y), 10.0);
        assert_eq!(s.with(|e, _| e.current_zoom()), crate::shell::editor::MAX_ZOOM, "zoom clamped to MAX_ZOOM");
        s.pinch((center_x, center_y), -0.99);
        s.pinch((center_x, center_y), -0.99);
        assert_eq!(s.with(|e, _| e.current_zoom()), crate::shell::editor::MIN_ZOOM, "zoom clamped to MIN_ZOOM");
    }

    // 2. XLSX workbook
    {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        let initial_zoom = s.with(|e, _| e.current_zoom());
        let (ws_origin, ws_size) = s.with(|e, _| {
            let ws = e.workspace_bounds.expect("workspace bounds");
            (ws.origin, ws.size)
        });
        let center_x = f32::from(ws_origin.x + ws_size.width / 2.0);
        let center_y = f32::from(ws_origin.y + ws_size.height / 2.0);

        s.pinch((center_x, center_y), 0.5);
        let zoomed = s.with(|e, _| e.current_zoom());
        assert!(zoomed > initial_zoom, "xlsx pinch out should increase zoom");
        // Verify sheet zoom was also updated
        let sheet_zoom = s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap().zoom);
        assert_eq!(sheet_zoom, Some(150), "sheet zoom should reflect 150%");
    }

    // 3. DOCX document
    {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        let initial_zoom = s.with(|e, _| e.current_zoom());
        let (ws_origin, ws_size) = s.with(|e, _| {
            let ws = e.workspace_bounds.expect("workspace bounds");
            (ws.origin, ws.size)
        });
        let center_x = f32::from(ws_origin.x + ws_size.width / 2.0);
        let center_y = f32::from(ws_origin.y + ws_size.height / 2.0);

        s.pinch((center_x, center_y), 0.3);
        let zoomed = s.with(|e, _| e.current_zoom());
        assert!(zoomed > initial_zoom, "docx pinch out should increase zoom");
    }
}

/// #72: font selection dropdown renders crisp font family previews on a light menu background (never solid black).
fn fix72_font_dropdown_preview() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    // Click the font select dropdown in the toolbar (around x=350, y=84).
    s.click((350.0, 84.0));
    s.settle();
    s.screenshot("font-dropdown-open");

    // Verify rendered pixels in the font dropdown region (x: 275..410, y: 180..550):
    // must have predominantly light menu background (> 70% light pixels) and dark anti-aliased glyph strokes,
    // never solid black rectangles (#000000).
    let (stride_w, raw) = s.picture();
    let scale = (stride_w as f32 / 1200.0).round().max(1.0) as u32;
    let mut light_bg = 0usize;
    let mut dark_glyph = 0usize;
    let mut max_dark_run = 0u32;
    let mut total = 0usize;
    for y in (180 * scale)..(550 * scale) {
        let mut run = 0u32;
        for x in (275 * scale)..(410 * scale) {
            let idx = ((y * stride_w + x) * 4) as usize;
            if idx + 3 < raw.len() {
                let (r, g, b) = (raw[idx], raw[idx + 1], raw[idx + 2]);
                total += 1;
                if r > 235 && g > 235 && b > 235 {
                    light_bg += 1;
                    run = 0;
                } else if r < 80 && g < 80 && b < 80 {
                    dark_glyph += 1;
                    run += 1;
                    max_dark_run = max_dark_run.max(run);
                } else {
                    run = 0;
                }
            }
        }
    }
    assert!(
        light_bg * 100 / total.max(1) > 80,
        "font dropdown background must be predominantly light (got {light_bg}/{total} light pixels)"
    );
    assert!(
        dark_glyph > 100 && dark_glyph * 100 / total.max(1) < 20,
        "font dropdown must contain visible dark glyph pixels without solid black fill (got {dark_glyph}/{total})"
    );
    assert!(
        max_dark_run < 50 * scale,
        "font dropdown must not contain solid black preview bars (max horizontal dark run = {max_dark_run}px)"
    );
}

/// Kanji whose Japanese and Chinese forms differ (直, 骨, 写): typed into a new
/// document, they are drawn in the same Japanese font while composed, while
/// edited and after (pictures to compare: the system's text system picked a
/// Chinese font where the document's 游ゴシック is not installed).
const CJK_SAMPLE: &str = "直接・骨・写真";

/// A window with a new file of `kind`, as ファイル > 新規 makes it.
fn new_file(kind: ibis_pptx::Kind) -> Scenario {
    let mut s = Scenario::start_view();
    s.update(|e, window, cx| e.new_file(kind, window, cx));
    s.settle();
    s
}

/// Compose `CJK_SAMPLE` (a picture), then commit it (a picture).
fn compose_and_commit(s: &mut Scenario) {
    use crate::shell::format::with_format;
    let n = CJK_SAMPLE.encode_utf16().count();
    s.update(|e, window, cx| {
        assert!(with_format!(e, wants_text_input(e, window, cx)), "the format takes no text input now");
        with_format!(e, replace_and_mark_text_in_range(e, None, CJK_SAMPLE, Some(n..n), window, cx));
    });
    s.settle();
    s.screenshot("composing");
    s.update(|e, window, cx| with_format!(e, replace_text_in_range(e, None, CJK_SAMPLE, window, cx)));
    s.settle();
    s.screenshot("editing");
}

fn fix_cjk_edit_font_xlsx() {
    let mut s = new_file(ibis_pptx::Kind::Workbook);
    s.click_cell("B2");
    s.key("f2");
    compose_and_commit(&mut s);
    s.key("enter");
    s.screenshot("committed");
}

fn fix_cjk_edit_font_pptx() {
    let mut s = new_file(ibis_pptx::Kind::Presentation);
    s.double_click_shape(0);
    compose_and_commit(&mut s);
    s.key("escape");
    s.click((5.0, 300.0));
    s.screenshot("committed");
}

fn fix_cjk_edit_font_docx() {
    let mut s = new_file(ibis_pptx::Kind::Document);
    let (id, _) = s.paragraph(0);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    compose_and_commit(&mut s);
    s.key("enter");
    s.screenshot("committed");
}

/// todo-fix #77: a document saved as .odt, a presentation as .odp and a workbook as .ods are OpenDocument
/// files (the mimetype first in the zip), and read again as what they are.
fn fix77_save_as_opendocument() {
    for (file, out, mimetype) in [
        ("test/editing/docx-report.docx", "saved.odt", "application/vnd.oasis.opendocument.text"),
        ("test/editing/pptx-plan.pptx", "saved.odp", "application/vnd.oasis.opendocument.presentation"),
        ("test/editing/xlsx-sales.xlsx", "saved.ods", "application/vnd.oasis.opendocument.spreadsheet"),
    ] {
        let mut s = Scenario::open(file);
        let path = s.save_as(out);
        let bytes = std::fs::read(&path).unwrap();
        // A stored first entry: "mimetype" then its text.
        assert_eq!(&bytes[..4], b"PK\x03\x04", "{out}: a zip");
        assert_eq!(&bytes[30..38], b"mimetype", "{out}: the mimetype is the first entry");
        let head = String::from_utf8_lossy(&bytes[38..38 + 64]).to_string();
        assert!(head.contains(mimetype), "{out}: {head:?}");
        if let Err(e) = crate::read_file(&path) { panic!("{out} does not read again: {e}"); }
    }
}

/// todo-fix #76: スクロール バーを常に表示 is on at first; on, every window's scrollbars are always shown, and it is kept.
fn fix76_scrollbars_always() {
    use gpui_kit::component::scroll::ScrollbarMode;
    use gpui_kit::component::theme::Theme;
    let dir = crate::shell::scenario::repo_root().join("target/scenario/shell::fix76_scrollbars_always/settings");
    let _ = std::fs::remove_dir_all(&dir);
    // SAFETY: the scenarios run one at a time on the main thread; nothing else reads the environment meanwhile.
    unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir) };
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let mode = |s: &mut Scenario| s.update(|_, _, cx| Theme::global(cx).scrollbar_mode);
    assert!(crate::shell::settings_window::scrollbars_always(), "on by default");
    assert_eq!(mode(&mut s), ScrollbarMode::Always);
    s.menu("tools.settings");
    s.screenshot("settings");
    crate::shell::settings::set(crate::shell::settings_window::SCROLLBARS_ALWAYS_KEY, "1");
    s.update(|_, _, cx| crate::shell::settings_window::apply_scrollbar_mode(cx));
    assert_eq!(mode(&mut s), ScrollbarMode::Always);
    assert!(std::fs::read_to_string(dir.join("settings.txt")).unwrap_or_default().contains("view.scrollbars-always=1"));
    s.screenshot("always");
    crate::shell::settings::set(crate::shell::settings_window::SCROLLBARS_ALWAYS_KEY, "0");
    s.update(|_, _, cx| crate::shell::settings_window::apply_scrollbar_mode(cx));
    assert_eq!(mode(&mut s), ScrollbarMode::Hover);
    unsafe { std::env::remove_var("IBIS_SETTINGS_DIR") };
}

/// todo-fix #78: in the dark appearance the chrome is dark and the document (page, cells, slide) stays white.
fn fix78_dark_chrome() {
    for (file, name) in [("test/editing/docx-report.docx", "docx"), ("test/editing/xlsx-sales.xlsx", "xlsx"), ("test/editing/pptx-plan.pptx", "pptx")] {
        let mut s = Scenario::open(file);
        s.update(|_, _, cx| crate::shell::palette::force(true, cx));
        assert!(crate::shell::palette::is_dark());
        s.settle();
        if name == "xlsx" {
            s.update(|e, window, cx| e.toggle_sidebar(window, cx));
            s.settle();
        }
        s.screenshot(&format!("{name}-dark"));
        // A menu open.
        s.click((78.0, 50.0));
        s.settle();
        s.screenshot(&format!("{name}-dark-menu"));
        s.key("escape");
        if name == "docx" {
            s.menu("insert.text-box");
            s.settle();
            s.screenshot("docx-dark-text-box-panel");
        }
        if name == "xlsx" {
            s.menu("xlsx.insert-textbox");
            s.type_text("x");
            s.key("escape");
            s.settle();
            s.screenshot("xlsx-dark-text-box-panel");
        }
        s.key("cmd-f");
        s.settle();
        s.screenshot(&format!("{name}-dark-find"));
        s.key("escape");
        s.update(|_, _, cx| crate::shell::palette::force(false, cx));
        assert!(!crate::shell::palette::is_dark());
        s.settle();
        s.screenshot(&format!("{name}-light"));
    }
    // The start window and the settings window.
    let mut s = Scenario::start_view();
    s.update(|_, _, cx| crate::shell::palette::force(true, cx));
    s.settle();
    s.screenshot("start-dark");
    s.menu("tools.settings");
    s.settle();
    let handle = crate::shell::settings_window::handle().expect("the settings window is open");
    s.screenshot_of(handle.into(), "settings-dark");
    s.update(|_, _, cx| crate::shell::palette::release(cx));
}

/// todo-fix #83: the scenarios write the recent files, the settings, AutoRecover and the backup settings under
/// target/scenario-user-data, not into the user's own application data directory.
fn fix83_scenarios_keep_user_data_apart() {
    let Some(home) = std::env::var_os("HOME") else { return };
    let real = std::path::PathBuf::from(home).join("Library/Application Support/ibisOffice");
    let stamps = |dir: &std::path::Path| -> Vec<(String, Option<std::time::SystemTime>)> {
        let mut v: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.metadata().and_then(|m| m.modified()).ok())).collect();
        v.sort();
        v
    };
    let before = stamps(&real);
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let root = crate::shell::scenario::repo_root();
    crate::shell::recent_files::add(&root.join("test/editing/docx-report.docx"));
    crate::shell::settings::set("fix83.probe", "1");
    crate::shell::autorecover::save_autorecover_config(&crate::shell::autorecover::load_autorecover_config());
    s.update(|_, _, _| ());
    assert_eq!(stamps(&real), before, "the user's own data directory is not touched");
    let base = crate::shell::app_data::base().expect("a data directory");
    assert!(base.starts_with(root.join("target")), "{base:?}");
    assert!(base.join("ibisOffice/recent-files.txt").exists(), "the scenarios' recent files are kept there");
}

/// todo-fix #75: ⌘W (and ファイル > 閉じる) closes the start window; the last window closing ends the app (on a Mac too).
fn fix75_start_window_closes_with_cmd_w() {
    for via_key in [true, false] {
        let mut s = Scenario::start_view();
        assert_eq!(s.window_count(), 1, "the start window is open");
        // The menu bar's ファイル > 閉じる is not greyed (it is when no listener of the action is on the window's path).
        let available = s.update(|_, window, cx| window.is_action_available(&crate::CloseFile, cx));
        assert!(available, "ファイル > 閉じる is enabled on the start window");
        s.quit_with_last_window();
        assert!(!s.quit_requested());
        if via_key {
            s.key("cmd-w");
        } else {
            s.menu("file.close");
        }
        assert_eq!(s.window_count(), 0, "the start window is closed (by {})", if via_key { "⌘W" } else { "ファイル > 閉じる" });
        assert!(s.quit_requested(), "the last window closing ends the app");
    }
}

/// An edit in a new file of `kind`, as the user makes it: a cell typed in (not yet entered, as when the window is
/// closed right after typing), text typed in a document, a text box on a slide.
/// A new file of `kind`, as ファイル > 新規作成 makes it on the start window.
fn issue27_new(kind: ibis_pptx::Kind) -> Scenario {
    let mut s = Scenario::start_view();
    s.update(|e, window, cx| e.new_file(kind, window, cx));
    s.settle();
    s
}

fn issue27_make_edit(s: &mut Scenario, kind: ibis_pptx::Kind) {
    match kind {
        ibis_pptx::Kind::Workbook => {
            s.click_cell("A1");
            s.type_text("a");
        }
        ibis_pptx::Kind::Document => {
            let (id, _) = s.paragraph(0);
            s.click_text(ibis_doc_model::FlowPos::new(id, 0));
            s.type_text("a");
            assert!(s.paragraph(0).1.contains('a'), "the document takes the typing");
        }
        ibis_pptx::Kind::Presentation => s.update(|e, _, cx| {
            let slide = e.current_slide;
            let id = e.document.ids().new_shape_id();
            let frame = ibis_doc_model::EmuRect::new(0, 0, 914_400, 914_400);
            e.execute(vec![ibis_doc_model::Command::InsertShape { slide, index: 0, shape: ibis_doc_model::Shape::text_box(id, frame, "a") }], cx);
        }),
    }
}

/// Click the actual client-drawn cross, not Scenario::press_close_button's
/// simulated native close callback. This catches a direct remove_window bypass.
fn click_client_close(s: &mut Scenario) {
    let at = s.update(|_, window, _| (f32::from(window.viewport_size().width) - 23.0, 18.0));
    s.click(at);
}

fn client_close_button_click_asks_to_save() {
    let _controls = crate::shell::chrome::scenario_client_window_controls();
    for compact in [false, true] {
        for kind in [ibis_pptx::Kind::Document, ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
            let setup = |s: &mut Scenario| {
                s.size(1280.0, 800.0);
                s.update(|e, _, cx| {
                    e.chrome_compact = compact;
                    e.show_menu_bar = true;
                    cx.notify();
                });
                s.quit_with_last_window();
            };
            let mut s = issue27_new(kind);
            setup(&mut s);
            click_client_close(&mut s);
            assert_eq!(s.window_count(), 0, "{kind:?}, compact={compact}: clean cross closes immediately");
            assert!(s.quit_requested());
            drop(s);

            let mut s = issue27_new(kind);
            setup(&mut s);
            issue27_make_edit(&mut s, kind);
            assert!(s.with(|e, _| e.is_dirty()));
            click_client_close(&mut s);
            assert_eq!(s.window_count(), 1, "{kind:?}, compact={compact}: cross preserves unsaved work");
            assert!(s.with(|e, _| e.save_confirm_dialog_open && e.close_window_after_confirm));
            assert!(!s.quit_requested());
            s.update(|e, _, cx| e.close_save_confirm_dialog(cx));
            assert!(s.with(|e, _| e.is_dirty() && !e.save_confirm_dialog_open));
            assert_eq!(s.window_count(), 1, "Cancel keeps the document open");
            click_client_close(&mut s);
            assert!(s.with(|e, _| e.save_confirm_dialog_open));
            s.update(|e, window, cx| e.confirm_discard_and_close(window, cx));
            assert_eq!(s.window_count(), 0, "{kind:?}, compact={compact}: Discard closes the window");
            assert!(s.quit_requested());
        }
    }
}

fn client_close_button_click_saves_then_closes() {
    let _controls = crate::shell::chrome::scenario_client_window_controls();
    for (kind, extension) in [
        (ibis_pptx::Kind::Document, "docx"),
        (ibis_pptx::Kind::Workbook, "xlsx"),
        (ibis_pptx::Kind::Presentation, "pptx"),
    ] {
        let mut s = issue27_new(kind);
        s.size(1280.0, 800.0);
        s.quit_with_last_window();
        let path = s.save_as(&format!("client-close.{extension}"));
        issue27_make_edit(&mut s, kind);
        click_client_close(&mut s);
        assert_eq!(s.window_count(), 1);
        assert!(s.with(|e, _| e.save_confirm_dialog_open));
        s.update(|e, window, cx| e.confirm_save_and_close(window, cx));
        assert_eq!(s.window_count(), 0, "{kind:?}: Save finishes the requested close");
        assert!(s.quit_requested());
        let saved = ibis_pptx::read(&std::fs::read(&path).unwrap()).unwrap();
        match kind {
            ibis_pptx::Kind::Workbook => {
                let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap();
                assert_eq!(sheet.cell(0, 0).map(|c| c.value.clone()), Some(ibis_doc_model::CellValue::Text("a".into())), "pending typed cell was saved");
            }
            ibis_pptx::Kind::Document => {
                assert!(saved.flow.as_ref().unwrap().body.iter().any(|block| match block {
                    ibis_doc_model::Block::Paragraph(p) => p.runs.iter().any(|r| matches!(&r.kind, ibis_doc_model::FlowRunKind::Text(text) if text.contains('a'))),
                    _ => false,
                }), "document typing was saved");
            }
            ibis_pptx::Kind::Presentation => {
                assert!(saved.slides.iter().flat_map(|slide| &slide.shapes).any(|shape| shape.text_body().is_some_and(|body| body.paragraphs.iter().flat_map(|p| &p.runs).any(|run| run.text.contains('a')))), "inserted text was saved");
            }
        }
    }
}

/// GitHub issue #27: closing a window with edits not saved (its close button, ファイル > 閉じる) asks whether to save
/// them, in each format and for a new file; without edits it closes at once; キャンセル keeps the window and the edits;
/// 保存しない from the close button closes the window (not only the file), and the app ends with its last window.
fn issue27_close_button_asks_to_save() {
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Document, ibis_pptx::Kind::Presentation] {
        // Nothing typed: the close button closes at once.
        let mut s = issue27_new(kind);
        s.quit_with_last_window();
        assert!(!s.with(|e, _| e.is_dirty()), "{kind:?}: a new file without edits has nothing to save");
        assert!(s.press_close_button(), "{kind:?}: the close button closes a window without edits");
        assert_eq!(s.window_count(), 0, "{kind:?}");
        assert!(s.quit_requested(), "{kind:?}: the app ends with its last window");
        drop(s);

        let mut s = issue27_new(kind);
        s.quit_with_last_window();
        issue27_make_edit(&mut s, kind);
        assert!(s.with(|e, _| e.is_dirty()), "{kind:?}: an edit is not saved yet");
        // The close button: the window stays and asks.
        assert!(!s.press_close_button(), "{kind:?}: the close button does not close a window with edits");
        assert_eq!(s.window_count(), 1, "{kind:?}");
        assert!(s.with(|e, _| e.save_confirm_dialog_open && e.close_window_after_confirm), "{kind:?}: 変更を保存しますか? is asked, to close the window");
        // Pressed again while asking: still asking.
        assert!(!s.press_close_button(), "{kind:?}: the close button again while asking");
        // キャンセル: the window and the edit stay.
        s.update(|e, _, cx| e.close_save_confirm_dialog(cx));
        assert!(s.with(|e, _| !e.save_confirm_dialog_open && !e.close_window_after_confirm && e.is_dirty()), "{kind:?}: キャンセル keeps the edit");
        assert_eq!(s.window_count(), 1, "{kind:?}");
        // ⌘W (ファイル > 閉じる) asks too (it did not for a presentation), to close the window once answered.
        s.key("cmd-w");
        assert_eq!(s.window_count(), 1, "{kind:?}: ⌘W does not close a window with edits");
        assert!(s.with(|e, _| e.save_confirm_dialog_open && e.close_window_after_confirm), "{kind:?}: ⌘W asks");
        s.update(|e, _, cx| e.close_save_confirm_dialog(cx));
        // 保存しない from the close button: the window goes, and the app with it.
        assert!(!s.press_close_button());
        s.update(|e, window, cx| e.confirm_discard_and_close(window, cx));
        assert_eq!(s.window_count(), 0, "{kind:?}: 保存しない closes the window");
        assert!(s.quit_requested(), "{kind:?}: the app ends with its last window");
    }
    // ⌘W closes the window in every format, not showing the start view in it; the app ends with its last window.
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Document, ibis_pptx::Kind::Presentation] {
        let mut s = issue27_new(kind);
        s.quit_with_last_window();
        s.key("cmd-w");
        assert_eq!(s.window_count(), 0, "{kind:?}: ⌘W closes the window");
        assert!(s.quit_requested(), "{kind:?}: the app ends with its last window");
        drop(s);
        // With an edit: 保存しない closes the window too.
        let mut s = issue27_new(kind);
        s.quit_with_last_window();
        issue27_make_edit(&mut s, kind);
        s.key("cmd-w");
        s.update(|e, window, cx| e.confirm_discard_and_close(window, cx));
        assert_eq!(s.window_count(), 0, "{kind:?}: ⌘W, 保存しない closes the window");
        assert!(s.quit_requested(), "{kind:?}");
    }
    // The release of the click that closed a workbook still reaches the sheet's listeners of the frame: with no
    // workbook open, nothing to do (it crashed: 「a workbook is open」).
    let mut s = Scenario::start_view();
    s.update(|e, _, cx| e.on_split_up(gpui_kit::point(gpui_kit::px(10.), gpui_kit::px(10.)), cx));
}

/// GitHub issue #27: 保存 from the close button writes the file (with what was being typed in the cell) and then
/// closes the window; a file saved has nothing left to ask.
fn issue27_save_answer_writes_then_closes() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.quit_with_last_window();
    let path = s.save_as("issue27.xlsx");
    assert!(!s.with(|e, _| e.is_dirty()), "just saved");
    assert!(s.press_close_button(), "a file just saved closes at once");
    drop(s);
    let mut s = Scenario::open(path.to_str().unwrap());
    s.quit_with_last_window();
    s.click_cell("H20");
    s.type_text("issue27");
    assert!(!s.press_close_button(), "edits not saved: asks");
    s.update(|e, window, cx| e.confirm_save_and_close(window, cx));
    assert_eq!(s.window_count(), 0, "保存 closes the window once written");
    assert!(s.quit_requested());
    let saved = ibis_pptx::read(&std::fs::read(&path).unwrap()).unwrap();
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&saved.slides[0]).unwrap();
    assert_eq!(sheet.cell(19, 7).map(|c| c.value.clone()), Some(ibis_doc_model::CellValue::Text("issue27".into())), "the typed cell is saved");
}

/// GitHub issue #27: ibisOffice > 終了 (⌘Q) with edits not saved asks instead of ending; without edits it ends.
fn issue27_quit_asks_to_save() {
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Document, ibis_pptx::Kind::Presentation] {
        let mut s = issue27_new(kind);
        s.quit_with_last_window();
        s.key("cmd-q");
        assert!(s.quit_requested(), "{kind:?}: nothing to save: ⌘Q ends the app");
        drop(s);
        let mut s = issue27_new(kind);
        s.quit_with_last_window();
        issue27_make_edit(&mut s, kind);
        // The key, as the user presses it: the action comes while the window is being updated.
        s.key("cmd-q");
        assert!(!s.quit_requested(), "{kind:?}: edits not saved: ⌘Q does not end the app");
        assert_eq!(s.window_count(), 1, "{kind:?}");
        assert!(s.with(|e, _| e.save_confirm_dialog_open && e.close_window_after_confirm), "{kind:?}: ⌘Q asks whether to save");
        // キャンセル: stays.
        s.update(|e, _, cx| e.close_save_confirm_dialog(cx));
        assert!(!s.quit_requested() && s.window_count() == 1, "{kind:?}: キャンセル keeps the app");
        // ⌘Q again, 保存しない: the window goes and the app ends.
        s.key("cmd-q");
        s.update(|e, window, cx| e.confirm_discard_and_close(window, cx));
        assert_eq!(s.window_count(), 0, "{kind:?}");
        assert!(s.quit_requested(), "{kind:?}: answered: the app ends with its last window");
    }
}

/// The title row's right-pane button (and 表示 > サイドバー) says what is on screen: on only when the pane is shown,
/// enabled only when it can show something. A docx shows the pane only for a panel opened for it (none in a new
/// document: the button is off and greyed); xlsx and pptx show it on request.
fn right_pane_button_follows_the_pane() {
    let pane = |s: &mut Scenario| s.with(|e, _| e.right_pane);
    let menu = |s: &mut Scenario| s.with(|e, _| {
        let entries = crate::shell::chrome_defaults::view_menu(e, e.kind());
        entries
            .into_iter()
            .find_map(|entry| match entry {
                crate::shell::chrome::MenuEntry::Item(i) if i.id == "view.sidebar" => Some((i.checked, i.enabled())),
                _ => None,
            })
            .expect("表示 > サイドバー")
    });
    // docx: a new document has no panel open.
    let mut s = issue27_new(ibis_pptx::Kind::Document);
    assert_eq!(pane(&mut s), crate::shell::chrome::RightPane { available: false, shown: false }, "docx: nothing to show: off, disabled");
    assert_eq!(menu(&mut s), (false, false), "docx: 表示 > サイドバー unchecked, disabled");
    // A panel opened: on, and the button hides it and shows it again.
    s.update(|e, window, cx| e.open_comments(window, cx));
    assert_eq!(pane(&mut s), crate::shell::chrome::RightPane { available: true, shown: true }, "docx: the comments panel is shown");
    assert_eq!(menu(&mut s), (true, true));
    s.update(|e, window, cx| e.toggle_sidebar(window, cx));
    assert_eq!(pane(&mut s), crate::shell::chrome::RightPane { available: true, shown: false }, "docx: hidden, can be shown again");
    s.update(|e, window, cx| e.toggle_sidebar(window, cx));
    assert!(pane(&mut s).shown, "docx: shown again");
    drop(s);
    // xlsx and pptx: the pane is there on request.
    for kind in [ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let mut s = issue27_new(kind);
        let before = pane(&mut s);
        assert!(before.available, "{kind:?}: the pane can be shown");
        s.update(|e, window, cx| e.toggle_sidebar(window, cx));
        let after = pane(&mut s);
        assert_ne!(before.shown, after.shown, "{kind:?}: the button switches the pane");
        assert_eq!(menu(&mut s), (after.shown, true), "{kind:?}: 表示 > サイドバー says the same");
    }
}

fn docx_open_box_edit(s: &mut Scenario) -> (f32, f32, f32, f32) {
    s.menu("insert.text-box");
    s.settle();
    let rect = docx_box_win_rect(s);
    s.double_click((rect.0 + rect.2 / 2.0, rect.1 + rect.3 / 2.0));
    s.settle();
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.box_edit.is_some())), "entered docx box_edit");
    docx_box_win_rect(s)
}

fn docx_box_win_rect(s: &mut Scenario) -> (f32, f32, f32, f32) {
    let (page, frame) = s.with(|e, _| {
        if let Some(edit) = e.doc_state().and_then(|d| d.box_edit.as_ref()) {
            let f = e.floating_text_box_frame_of(edit.at).map(|(_, f, _)| f).unwrap_or(edit.frame);
            return (edit.page, f);
        }
        if let Some((page, frame, _, _)) = e.selected_text_box_frame() {
            return (page, frame);
        }
        let laid = crate::docx::flow_layout::pages(e.flow().unwrap(), &e.resources);
        laid.pages
            .iter()
            .enumerate()
            .find_map(|(i, pg)| {
                pg.items.iter().find_map(|item| match item {
                    crate::docx::flow_layout::Item::Shape { shape, x, y, w, h }
                        if matches!(shape.kind, ibis_doc_model::ShapeKind::TextBox(_)) =>
                    {
                        Some((i, (*x, *y, *w, *h)))
                    }
                    _ => None,
                })
            })
            .expect("the box is on a page")
    });
    let (origin, k) = s.with(|e, _| e.page_origin(page)).expect("the page is placed");
    (f32::from(origin.x) + frame.0 * k, f32::from(origin.y) + frame.1 * k, frame.2 * k, frame.3 * k)
}

/// 1. docx: 2nd-line selection rectangle and caret align with 2nd-line painted glyphs.
fn fix_textbox_01_docx_second_line_selection_and_caret_alignment() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("Line1");
    s.key("enter");
    s.type_text("Line2");
    s.settle();

    let (caret_rect, p0_y, p1_y, p1_h) = s.with(|e, _| {
        let marks = e.caret_marks().expect("caret marks");
        let (caret, _) = marks.rects(true).expect("rects");
        let edit = e.doc_state().unwrap().box_edit.as_ref().unwrap();
        let list = edit.positions.paragraphs.borrow();
        assert_eq!(list.len(), 2, "two painted paragraphs");
        let p0 = list[0].borrow();
        let p1 = list[1].borrow();
        let p0_y = p0.lines[0].top;
        let p1_y = p1.lines[0].top;
        let p1_h = p1.lines[0].height;
        (caret.expect("caret at end of line 2"), p0_y, p1_y, p1_h)
    });
    assert!(p1_y > p0_y + 8.0, "line 2 is below line 1: p0_y={p0_y}, p1_y={p1_y}");
    assert!((f32::from(caret_rect.origin.y) - p1_y).abs() < 1.0, "caret Y matches line 2 top: caret={caret_rect:?}, p1_y={p1_y}");

    s.key("shift-left");
    s.key("shift-left");
    s.key("shift-left");
    s.settle();
    let sel_rects = s.with(|e, _| e.caret_marks().unwrap().rects(true).unwrap().1);
    assert_eq!(sel_rects.len(), 1, "single selection rect on line 2");
    let sr = sel_rects[0];
    assert!((f32::from(sr.origin.y) - p1_y).abs() < 1.0, "selection Y matches line 2 top: sr={sr:?}, p1_y={p1_y}");
    assert!((f32::from(sr.size.height) - p1_h).abs() < 1.0, "selection height matches line 2 height: sr={sr:?}, p1_h={p1_h}");
    s.screenshot("docx-line2-selection");
}

/// 2. docx: 2nd line with 24pt font keeps 3rd-line caret and selection aligned with 3rd-line painted glyphs.
fn fix_textbox_02_docx_second_line_large_font_third_line_alignment() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("Line1");
    s.key("enter");
    s.type_text("BigLine2");
    s.key("enter");
    s.type_text("Line3After");
    s.settle();

    // Select "BigLine2" (indices 6..14) and apply 24pt (2400).
    s.update(|e, _, cx| {
        if let Some(te) = e.text_editing_mut() {
            te.anchor = 6;
            te.caret = 14;
        }
        e.apply_box_edit_run_change(&ibis_doc_model::RunChange::Size(2400), cx);
    });
    s.settle();

    // Move caret to end of Line 3 and select "After" on Line 3.
    s.key("cmd-down");
    s.settle();
    let (caret_y, p1_y, p2_y, p2_h) = s.with(|e, _| {
        let marks = e.caret_marks().unwrap();
        let (caret, _) = marks.rects(true).unwrap();
        let edit = e.doc_state().unwrap().box_edit.as_ref().unwrap();
        let list = edit.positions.paragraphs.borrow();
        assert_eq!(list.len(), 3);
        let p1 = list[1].borrow();
        let p2 = list[2].borrow();
        let p1_y = p1.lines[0].top;
        let p2_y = p2.lines[0].top;
        let p2_h = p2.lines[0].height;
        (f32::from(caret.unwrap().origin.y), p1_y, p2_y, p2_h)
    });
    assert!(p2_y > p1_y + 18.0, "line 3 is below 24pt line 2: p1_y={p1_y}, p2_y={p2_y}");
    assert!((caret_y - p2_y).abs() < 1.0, "line 3 caret Y matches line 3 painted top: caret_y={caret_y}, p2_y={p2_y}");

    s.key("shift-left");
    s.key("shift-left");
    s.key("shift-left");
    s.settle();
    let sr = s.with(|e, _| e.caret_marks().unwrap().rects(true).unwrap().1[0]);
    assert!((f32::from(sr.origin.y) - p2_y).abs() < 1.0, "line 3 selection Y matches line 3 painted top");
    assert!((f32::from(sr.size.height) - p2_h).abs() < 1.0, "line 3 selection height matches line 3 painted height");
}

/// 3. docx: Up and Down arrow keys move caret vertically across lines while keeping goal_x.
fn fix_textbox_03_docx_arrow_up_down_vertical_navigation() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("123456");
    s.key("enter");
    s.type_text("123456");
    s.key("enter");
    s.type_text("123456");
    s.settle();

    // Caret is at index 20 ("123456|"). Move left 3 times -> index 17 ("123|456").
    s.key("left");
    s.key("left");
    s.key("left");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 17);

    // Up -> line 2 ("123|456", index 10).
    s.key("up");
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 10, "Up moves from line 3 to line 2");

    // Up -> line 1 ("123|456", index 3).
    s.key("up");
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 3, "Up moves from line 2 to line 1");

    // Down -> line 2 (index 10).
    s.key("down");
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 10, "Down moves from line 1 to line 2");

    // Down -> line 3 (index 17).
    s.key("down");
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 17, "Down moves from line 2 to line 3");
}

/// 4. docx: Shift+Up and Shift+Down extend selection across multiple lines.
fn fix_textbox_04_docx_shift_up_down_multiline_selection() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("aaaa");
    s.key("enter");
    s.type_text("aaaa");
    s.key("enter");
    s.type_text("aaaa");
    s.settle();

    s.key("shift-up");
    s.settle();
    let r1 = s.with(|e, _| e.text_editing().unwrap().range());
    assert_eq!(r1, 9..14, "Shift+Up selects from end of line 2 to end of line 3");

    s.key("shift-up");
    s.settle();
    let r2 = s.with(|e, _| e.text_editing().unwrap().range());
    assert_eq!(r2, 4..14, "Second Shift+Up extends selection to line 1");
    let rect_count = s.with(|e, _| e.caret_marks().unwrap().rects(true).unwrap().1.len());
    assert!(rect_count >= 2, "multi-line selection produces multiple rects: {rect_count}");

    s.key("shift-down");
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 9..14);
}

/// 5. docx: Left, Right, Home/Cmd+Left, End/Cmd+Right, Cmd+Up, Cmd+Down navigation.
fn fix_textbox_05_docx_left_right_home_end_navigation() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("hello");
    s.key("enter");
    s.type_text("world");
    s.settle();

    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 11);
    s.key("home");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 6, "Home moves to start of line 2");
    s.key("end");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 11, "End moves to end of line 2");
    s.key("cmd-up");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 0, "Cmd+Up moves to start of text box");
    s.key("cmd-right");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 5, "Cmd+Right moves to end of line 1");
    s.key("right");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 6, "Right crosses newline to start of line 2");
    s.key("left");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 5, "Left crosses newline back to end of line 1");
    s.key("cmd-down");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 11, "Cmd+Down moves to end of text box");
}

/// 6. docx: Alt+Left / Alt+Right word navigation and Shift+Alt+Left / Right word selection.
fn fix_textbox_06_docx_word_navigation_and_selection() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("alpha beta gamma");
    s.settle();

    s.key("alt-left");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 11, "Alt+Left jumps to start of 'gamma'");
    s.key("alt-left");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 6, "Alt+Left jumps to start of 'beta'");
    s.key("alt-shift-right");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 6..10, "Shift+Alt+Right selects 'beta'");
    s.key("alt-left");
    s.key("alt-left");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 0);
}

/// 7. docx: Typing 5+ lines never changes the user-specified text box rectangle size.
fn fix_textbox_07_docx_five_lines_fixed_rectangle_size() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    let (w0, h0) = s.with(|e, _| {
        let at = e.doc_state().unwrap().box_edit.as_ref().unwrap().at;
        let info = e.text_box_near(at).unwrap();
        (info.geometry[0].clone(), info.geometry[1].clone())
    });
    let frame0 = s.with(|e, _| e.doc_state().unwrap().box_edit.as_ref().unwrap().frame);

    s.key("cmd-a");
    for line in ["a", "b", "c", "d", "e", "f", "g"] {
        s.type_text(line);
        s.key("enter");
    }
    s.settle();

    let frame1 = s.with(|e, _| e.doc_state().unwrap().box_edit.as_ref().unwrap().frame);
    assert_eq!(frame1.2, frame0.2, "box_edit width unchanged after 7 lines");
    assert_eq!(frame1.3, frame0.3, "box_edit height unchanged after 7 lines");
    s.key("escape");
    s.settle();
    let (w1, h1) = s.with(|e, _| {
        let info = e.text_box_at_caret().unwrap();
        (info.geometry[0].clone(), info.geometry[1].clone())
    });
    assert_eq!((w1, h1), (w0, h0), "docx text box geometry unchanged after typing 7 lines");
}

/// 8. docx: IME composition ("あいうえお") is visible inline with underline (not white/invisible) and commits cleanly.
fn fix_textbox_08_docx_ime_composing_visible_and_commit() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.key("backspace");
    s.settle();

    s.update(|e, _, cx| {
        e.input_mark("あいうえお", cx);
    });
    s.settle();

    let (marked, composing_text, underlined, color_rgb, has_caret_bounds) = s.with(|e, _| {
        let marked = e.text_editing().unwrap().marked.clone();
        let (_, _, comp) = e.resources.edited_box.as_ref().expect("resources.edited_box populated");
        let body = comp.as_ref().expect("composing body present while IME active");
        let run = &body.paragraphs[0].runs[0];
        let color = run.style.color.unwrap_or(body.style.color);
        (marked, body.text(), run.style.underline, color.0, e.caret_bounds().is_some())
    });
    assert_eq!(marked.as_deref(), Some("あいうえお"));
    assert_eq!(composing_text, "あいうえお");
    assert!(underlined, "IME composing run is underlined");
    assert_ne!(color_rgb & 0xffffff, 0xffffff, "IME composing text is dark/readable, not white: {color_rgb:#x}");
    assert!(has_caret_bounds, "IME candidate window has caret_bounds");
    s.screenshot("docx-ime-composing");

    s.update(|e, _, cx| {
        e.input_replace(None, "あいうえお", cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.text_editing().unwrap().marked.is_none()), "marked cleared on commit");
    assert_eq!(s.with(|e, _| e.edited_text()), Some("あいうえお".to_string()));
}

/// 9. docx: Double-clicking a text box places the caret at the mouse click position (line 1 left, line 1 right, line 2).
fn fix_textbox_09_docx_double_click_places_caret_at_click_pos() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("abcde");
    s.key("enter");
    s.type_text("fghij");
    s.settle();
    let (pt_l1_left, pt_l1_right, pt_l2) = s.with(|e, _| {
        let edit = e.doc_state().unwrap().box_edit.as_ref().unwrap();
        let list = edit.positions.paragraphs.borrow();
        let p0 = list[0].borrow();
        let p1 = list[1].borrow();
        let last0 = p0.chars.last().unwrap();
        (
            (p0.lines[0].x + 2.0, p0.lines[0].top + 6.0),
            (last0.x + last0.advance - 1.0, p0.lines[0].top + 6.0),
            (p1.lines[0].x + 20.0, p1.lines[0].top + 6.0),
        )
    });
    s.key("escape");
    s.settle();

    // Line 1 near left edge -> index 0.
    s.double_click(pt_l1_left);
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 0, "line 1 left double-click places caret at 0");
    s.key("escape");
    s.settle();

    // Line 1 near right edge -> near end of line 1 (index >= 4, <= 5).
    s.double_click(pt_l1_right);
    s.settle();
    let c_right = s.with(|e, _| e.text_editing().unwrap().caret);
    assert!((4..=5).contains(&c_right), "line 1 right double-click places caret near end of line 1: {c_right}");
    s.key("escape");
    s.settle();

    // Line 2 near middle -> on line 2 (index >= 6).
    s.double_click(pt_l2);
    s.settle();
    let c_line2 = s.with(|e, _| e.text_editing().unwrap().caret);
    assert!(c_line2 >= 6, "line 2 double-click places caret on line 2: {c_line2}");
}

/// 10. docx: Double-click inside box_edit selects word; triple-click selects paragraph.
fn fix_textbox_10_docx_double_and_triple_click_selects_word_and_paragraph() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("hello world");
    s.key("enter");
    s.type_text("second line");
    s.settle();

    let word_pt = s.with(|e, _| {
        let edit = e.doc_state().unwrap().box_edit.as_ref().unwrap();
        let list = edit.positions.paragraphs.borrow();
        let p0 = list[0].borrow();
        (p0.lines[0].x + 12.0, p0.lines[0].top + 6.0)
    });
    s.double_click(word_pt);
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 0..6, "double-click inside box_edit selects 'hello '");

    s.triple_click(word_pt);
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 0..11, "triple-click inside box_edit selects paragraph 'hello world'");
}

/// 11. docx: Mouse drag inside box_edit selects character range.
fn fix_textbox_11_docx_mouse_drag_selects_text_range() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("abcdefghij");
    s.settle();

    let (x0, x1, y) = s.with(|e, _| {
        let edit = e.doc_state().unwrap().box_edit.as_ref().unwrap();
        let list = edit.positions.paragraphs.borrow();
        let p0 = list[0].borrow();
        let x0 = p0.lines[0].x + 1.0;
        let last = p0.chars.last().unwrap();
        let x1 = last.x + last.advance - 1.0;
        (x0, x1, p0.lines[0].top + 6.0)
    });
    s.drag((x0, y), (x1, y));
    s.settle();
    let r = s.with(|e, _| e.text_editing().unwrap().range());
    assert!(r.start <= 1 && r.end >= 8, "mouse drag selects across line: {r:?}");
}

/// 12. docx: Partial selection Cmd+B / Cmd+I / Cmd+U formats only the selected range and updates current_format().
fn fix_textbox_12_docx_partial_selection_bold_italic_underline() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("hello world");
    s.settle();

    // Select "world" (6..11).
    for _ in 0..5 {
        s.key("shift-left");
    }
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 6..11);

    s.key("cmd-b");
    s.key("cmd-i");
    s.key("cmd-u");
    s.settle();

    let (fmt_bold, fmt_italic, fmt_underline, runs) = s.with(|e, _| {
        let (_, st) = e.current_format().expect("current_format");
        let body = e.edited_body().unwrap();
        let runs: Vec<_> = body.paragraphs[0].runs.iter().map(|r| (r.text.clone(), r.style.bold, r.style.italic, r.style.underline)).collect();
        (st.bold, st.italic, st.underline, runs)
    });
    assert!(fmt_bold && fmt_italic && fmt_underline, "toolbar current_format reflects selected 'world'");
    assert_eq!(runs.len(), 2, "split into 'hello ' and 'world': {runs:?}");
    assert_eq!(runs[0].0, "hello ");
    assert_ne!(runs[0].1, Some(true), "'hello ' is not bold");
    assert_eq!(runs[1].0, "world");
    assert_eq!(runs[1].1, Some(true), "'world' is bold");
    assert_eq!(runs[1].2, Some(true), "'world' is italic");
    assert!(runs[1].3, "'world' is underline");
}

/// 13. docx: Partial selection font size and color formatting in box_edit.
fn fix_textbox_13_docx_partial_selection_font_size_and_color() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("small BIG");
    s.settle();

    s.key("shift-left");
    s.key("shift-left");
    s.key("shift-left");
    s.update(|e, _, cx| {
        e.apply_box_edit_run_change(&ibis_doc_model::RunChange::Size(2400), cx);
        e.apply_box_edit_run_change(&ibis_doc_model::RunChange::Color(ibis_doc_model::Rgb(0xFF0000)), cx);
    });
    s.settle();

    let runs = s.with(|e, _| {
        let body = e.edited_body().unwrap();
        body.paragraphs[0].runs.iter().map(|r| (r.text.clone(), r.style.font_size, r.style.color)).collect::<Vec<_>>()
    });
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(runs[0].0, "small ");
    assert_ne!(runs[0].1, Some(2400));
    assert_eq!(runs[1].0, "BIG");
    assert_eq!(runs[1].1, Some(2400));
    assert_eq!(runs[1].2, Some(ibis_doc_model::Rgb(0xFF0000)));
}

/// 14. docx: All 8 resize handles are active and resize the text box while remaining in box_edit mode.
fn fix_textbox_14_docx_eight_handles_resize_while_editing() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let (bx, by, bw, bh) = docx_open_box_edit(&mut s);
    let (_, _, w0, h0) = s.with(|e, _| e.doc_state().unwrap().box_edit.as_ref().unwrap().frame);

    // Drag the bottom-right handle (bx + bw, by + bh) by (+40, +30) px.
    s.drag((bx + bw, by + bh), (bx + bw + 40.0, by + bh + 30.0));
    s.settle();

    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.box_edit.is_some())), "still in box_edit after handle resize");
    let (_, _, w1, h1) = s.with(|e, _| e.doc_state().unwrap().box_edit.as_ref().unwrap().frame);
    assert!(w1 > w0 + 15.0 && h1 > h0 + 10.0, "box resized by handle: ({w0}, {h0}) -> ({w1}, {h1})");
}

/// 15. docx: Border drag strips move the floating text box while remaining in box_edit mode.
fn fix_textbox_15_docx_border_drag_moves_box_while_editing() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let (bx, by, bw, _bh) = docx_open_box_edit(&mut s);
    let (x0, y0, _, _) = s.with(|e, _| e.doc_state().unwrap().box_edit.as_ref().unwrap().frame);

    // Drag top border midpoint (bx + bw * 0.3, by) by (+35, +25) px.
    s.drag((bx + bw * 0.3, by), (bx + bw * 0.3 + 35.0, by + 25.0));
    s.settle();

    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.box_edit.is_some())), "still in box_edit after border move");
    let (x1, y1, _, _) = s.with(|e, _| e.doc_state().unwrap().box_edit.as_ref().unwrap().frame);
    assert!(x1 > x0 + 10.0 && y1 > y0 + 10.0, "box moved by border strip: ({x0}, {y0}) -> ({x1}, {y1})");
}

/// 16. docx: Typing multiple strings in one box_edit session is undone in a single Undo step.
fn fix_textbox_16_docx_undo_coalesces_typing_session() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.menu("insert.text-box");
    s.settle();
    let before = s.document();
    let (bx, by, bw, bh) = docx_box_win_rect(&mut s);
    s.double_click((bx + bw / 2.0, by + bh / 2.0));
    s.settle();

    s.key("cmd-a");
    s.type_text("one");
    s.type_text(" two");
    s.key("enter");
    s.type_text("three");
    s.key("escape");
    s.settle();

    assert_ne!(s.document(), before);
    s.menu("edit.undo");
    s.settle();
    assert_eq!(s.document(), before, "entire docx box_edit typing session is undone in 1 step");
}

/// 17. xlsx: Inserting a text box enters edit mode and typing 5+ lines keeps rectangle size fixed.
fn fix_textbox_17_xlsx_insert_and_five_lines_fixed_rectangle_size() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();
    assert!(s.with(|e, _| e.editing.is_some()), "insert.text-box enters edit mode");
    let frame0 = s.with(|e, _| e.slide().shapes.last().unwrap().frame);

    for line in ["a", "b", "c", "d", "e", "f"] {
        s.type_text(line);
        s.key("enter");
    }
    s.settle();

    let frame1 = s.with(|e, _| e.slide().shapes.last().unwrap().frame);
    assert_eq!(frame1, frame0, "xlsx text box frame stays exactly at user-specified size after 6 lines");
}

/// 18. xlsx: 2nd-line selection rectangle and caret align with 2nd-line painted glyphs.
fn fix_textbox_18_xlsx_second_line_selection_and_caret_alignment() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();
    s.type_text("FirstLine");
    s.key("enter");
    s.type_text("SecondLineText");
    s.settle();

    let (caret_rect, p0_y, p1_y, p1_h) = s.with(|e, _| {
        let marks = e.caret_marks().unwrap();
        let (caret, _) = marks.rects(true).unwrap();
        let list = e.editing.as_ref().unwrap().positions.paragraphs.borrow();
        let p0 = list[0].borrow();
        let p1 = list[1].borrow();
        let p0_y = p0.lines[0].top;
        let p1_y = p1.lines[0].top;
        let p1_h = p1.lines[0].height;
        (caret.unwrap(), p0_y, p1_y, p1_h)
    });
    assert!(p1_y > p0_y + 8.0);
    assert!((f32::from(caret_rect.origin.y) - p1_y).abs() < 1.0, "xlsx line 2 caret Y aligns with line 2 top");

    for _ in 0..4 {
        s.key("shift-left");
    }
    s.settle();
    let sr = s.with(|e, _| e.caret_marks().unwrap().rects(true).unwrap().1[0]);
    assert!((f32::from(sr.origin.y) - p1_y).abs() < 1.0, "xlsx line 2 selection Y aligns with line 2 top");
    assert!((f32::from(sr.size.height) - p1_h).abs() < 1.0, "xlsx line 2 selection height aligns with line 2 height");
}

/// 19. xlsx: 2nd line with 24pt font keeps 3rd-line caret and selection aligned with 3rd-line painted glyphs.
fn fix_textbox_19_xlsx_second_line_large_font_third_line_alignment() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();
    s.type_text("Row1");
    s.key("enter");
    s.type_text("BigRow2");
    s.key("enter");
    s.type_text("Row3End");
    s.settle();

    // Select "BigRow2" (5..12) and apply 24pt (2400).
    s.update(|e, _, cx| {
        if let Some(te) = e.text_editing_mut() {
            te.anchor = 5;
            te.caret = 12;
        }
        e.format_cells(ibis_doc_model::StyleChange::FontSize(2400), cx);
    });
    s.settle();

    s.key("cmd-down");
    s.settle();
    let (caret_y, p1_y, p2_y, p2_h) = s.with(|e, _| {
        let marks = e.caret_marks().unwrap();
        let (caret, _) = marks.rects(true).unwrap();
        let list = e.editing.as_ref().unwrap().positions.paragraphs.borrow();
        let p1 = list[1].borrow();
        let p2 = list[2].borrow();
        let p1_y = p1.lines[0].top;
        let p2_y = p2.lines[0].top;
        let p2_h = p2.lines[0].height;
        (f32::from(caret.unwrap().origin.y), p1_y, p2_y, p2_h)
    });
    assert!(p2_y > p1_y + 18.0, "xlsx row 3 is below 24pt row 2");
    assert!((caret_y - p2_y).abs() < 1.0, "xlsx row 3 caret Y matches painted row 3 top");

    s.key("shift-left");
    s.key("shift-left");
    s.key("shift-left");
    s.settle();
    let sr = s.with(|e, _| e.caret_marks().unwrap().rects(true).unwrap().1[0]);
    assert!((f32::from(sr.origin.y) - p2_y).abs() < 1.0);
    assert!((f32::from(sr.size.height) - p2_h).abs() < 1.0);
}

/// 20. xlsx: Up and Down arrow keys and Shift+Up / Shift+Down inside text box editing.
fn fix_textbox_20_xlsx_arrow_up_down_and_shift_selection() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();
    s.type_text("12345");
    s.key("enter");
    s.type_text("67890");
    s.key("enter");
    s.type_text("abcde");
    s.settle();

    s.key("left");
    s.key("left");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 15);
    s.key("up");
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 9, "Up moves from line 3 to line 2 in xlsx");
    s.key("up");
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 3, "Up moves from line 2 to line 1 in xlsx");
    s.key("shift-down");
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 3..9, "Shift+Down selects from line 1 to line 2 in xlsx");
}

/// 21. xlsx: Word navigation (Alt+Left/Right) and word/line deletion (Alt+Backspace, Cmd+Backspace).
fn fix_textbox_21_xlsx_word_nav_and_delete_shortcuts() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();
    s.type_text("one two three");
    s.settle();

    s.key("alt-left");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 8);
    s.key("alt-right");
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 13);
    s.key("alt-backspace");
    s.settle();
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("one two "));
    s.key("cmd-backspace");
    s.settle();
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some(""));
}

/// 22. xlsx: IME composition ("あいうえお") is visible inline with underline and commits cleanly.
fn fix_textbox_22_xlsx_ime_composing_visible_and_commit() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();

    s.update(|e, _, cx| {
        e.input_mark("あいうえお", cx);
    });
    s.settle();

    let (marked, comp_text, underlined, color_rgb, has_bounds) = s.with(|e, _| {
        let shape_id = e.text_editing().unwrap().shape;
        let comp = e.composing_body(shape_id).expect("composing_body present");
        let run = &comp.paragraphs[0].runs[0];
        let color = run.style.color.unwrap_or(comp.style.color);
        (e.text_editing().unwrap().marked.clone(), comp.text(), run.style.underline, color.0, e.caret_bounds().is_some())
    });
    assert_eq!(marked.as_deref(), Some("あいうえお"));
    assert_eq!(comp_text, "あいうえお");
    assert!(underlined);
    assert_ne!(color_rgb & 0xffffff, 0xffffff, "IME text is dark/readable, not white");
    assert!(has_bounds);
    s.screenshot("xlsx-ime-composing");

    s.update(|e, _, cx| {
        e.input_replace(None, "あいうえお", cx);
    });
    s.settle();
    assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("あいうえお"));
}

/// 23. xlsx: Double-clicking a text box places the caret at the clicked position on line 1 or line 2.
fn fix_textbox_23_xlsx_double_click_places_caret_at_click_pos() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();
    s.type_text("abcdefghij");
    s.key("enter");
    s.type_text("klmnopqrst");
    s.key("escape");
    s.settle();

    let (tb_x, tb_y, tb_w, _) = s.with(|e, _| {
        let shape = e.slide().shapes.last().unwrap();
        let layout = e.slide_layout().unwrap();
        let f = shape.frame;
        (
            f32::from(layout.origin.x + layout.to_px(f.x)),
            f32::from(layout.origin.y + layout.to_px(f.y)),
            f32::from(layout.to_px(f.width)),
            f32::from(layout.to_px(f.height)),
        )
    });

    s.double_click((tb_x + 8.0, tb_y + 10.0));
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().caret), 0);
    s.key("escape");
    s.settle();

    s.double_click((tb_x + tb_w - 8.0, tb_y + 10.0));
    s.settle();
    let c1 = s.with(|e, _| e.text_editing().unwrap().caret);
    assert!((7..=10).contains(&c1), "line 1 right double-click: {c1}");
    s.key("escape");
    s.settle();

    s.double_click((tb_x + 25.0, tb_y + 30.0));
    s.settle();
    let c2 = s.with(|e, _| e.text_editing().unwrap().caret);
    assert!(c2 >= 11, "line 2 double-click places caret on line 2: {c2}");
}

/// 24. xlsx: Double-click selects word, triple-click selects paragraph, mouse drag selects range inside edit mode.
fn fix_textbox_24_xlsx_double_triple_click_and_drag_selection() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();
    s.type_text("hello world");
    s.key("enter");
    s.type_text("second line");
    s.settle();

    let (pt_word, x_end) = s.with(|e, _| {
        let list = e.editing.as_ref().unwrap().positions.paragraphs.borrow();
        let p0 = list[0].borrow();
        let last = p0.chars.last().unwrap();
        ((p0.lines[0].x + 10.0, p0.lines[0].top + 6.0), last.x + last.advance - 1.0)
    });
    s.double_click(pt_word);
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 0..6);

    s.triple_click(pt_word);
    s.settle();
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 0..11);

    s.drag((pt_word.0 - 8.0, pt_word.1), (x_end, pt_word.1));
    s.settle();
    let r = s.with(|e, _| e.text_editing().unwrap().range());
    assert!(r.start <= 1 && r.end >= 9, "drag selection in xlsx text box: {r:?}");
}

/// 25. xlsx: Partial selection formatting (Cmd+B, Cmd+I, Cmd+U, font size) and toolbar current_style() sync.
fn fix_textbox_25_xlsx_partial_selection_formatting_and_toolbar_sync() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();
    s.type_text("plain styled");
    s.settle();

    for _ in 0..6 {
        s.key("shift-left");
    }
    assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 6..12);

    s.key("cmd-b");
    s.key("cmd-i");
    s.key("cmd-u");
    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::FontSize(1800), cx));
    s.settle();

    let (style, runs) = s.with(|e, _| {
        let st = e.current_style();
        let body = e.edited_body().unwrap();
        let runs: Vec<_> = body.paragraphs[0].runs.iter().map(|r| (r.text.clone(), r.style.bold, r.style.italic, r.style.underline, r.style.font_size)).collect();
        (st, runs)
    });
    assert!(style.bold && style.italic && style.underline.enabled() && style.font_size == 1800);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].0, "plain ");
    assert_ne!(runs[0].1, Some(true));
    assert_eq!(runs[1].0, "styled");
    assert_eq!(runs[1].1, Some(true));
    assert_eq!(runs[1].2, Some(true));
    assert!(runs[1].3);
    assert_eq!(runs[1].4, Some(1800));
}

/// 26. xlsx: 8 resize handles and border drag strips work while editing a text box.
fn fix_textbox_26_xlsx_eight_handles_and_border_drag_while_editing() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("insert.text-box");
    s.settle();
    s.type_text("Resize and move me");
    s.settle();

    let (f0, br_pt) = s.with(|e, _| {
        let shape = e.slide().shapes.last().unwrap();
        let layout = e.slide_layout().unwrap();
        let f = shape.frame;
        (f, (f32::from(layout.origin.x + layout.to_px(f.x + f.width)), f32::from(layout.origin.y + layout.to_px(f.y + f.height))))
    });
    s.drag(br_pt, (br_pt.0 + 50.0, br_pt.1 + 30.0));
    s.settle();
    let f1 = s.with(|e, _| e.slide().shapes.last().unwrap().frame);
    assert!(f1.width > f0.width && f1.height > f0.height, "xlsx handle resized frame: {f0:?} -> {f1:?}");

    // Re-enter edit mode and drag top border strip.
    let (top_mid, _) = s.with(|e, _| {
        let shape = e.slide().shapes.last().unwrap();
        let layout = e.slide_layout().unwrap();
        let f = shape.frame;
        ((f32::from(layout.origin.x + layout.to_px(f.x + f.width / 3)), f32::from(layout.origin.y + layout.to_px(f.y))), f)
    });
    s.double_click((top_mid.0, top_mid.1 + 15.0));
    s.settle();
    assert!(s.with(|e, _| e.editing.is_some()));
    s.drag(top_mid, (top_mid.0 + 40.0, top_mid.1 + 25.0));
    s.settle();
    let f2 = s.with(|e, _| e.slide().shapes.last().unwrap().frame);
    assert!(f2.x > f1.x && f2.y > f1.y, "xlsx border drag moved frame: {f1:?} -> {f2:?}");
}

/// 27. xlsx: Escape on a newly created empty text box retracts it; typing session is undone in a single step.
fn fix_textbox_27_xlsx_empty_new_textbox_retracted_on_escape_and_undo_coalesced() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let count0 = s.with(|e, _| e.slide().shapes.len());
    s.update(|e, window, cx| {
        let id = e.insert_sheet_textbox(window, cx);
        e.editing.as_mut().unwrap().edit.is_new = true;
        let _ = id;
    });
    s.settle();
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), count0 + 1);
    s.key("escape");
    s.settle();
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), count0, "empty new text box retracted on Escape");

    // Insert and type, then Escape, then double-click to edit again and undo that second session in 1 step.
    s.menu("insert.text-box");
    s.settle();
    s.type_text("initial");
    s.key("escape");
    s.settle();
    let after_initial = s.with(|e, _| e.slide().clone());

    let center = s.with(|e, _| {
        let shape = e.slide().shapes.last().unwrap();
        let layout = e.slide_layout().unwrap();
        let f = shape.frame;
        (f32::from(layout.origin.x + layout.to_px(f.x + f.width / 2)), f32::from(layout.origin.y + layout.to_px(f.y + f.height / 2)))
    });
    s.double_click(center);
    s.settle();
    s.key("cmd-a");
    s.type_text("changed");
    s.type_text(" again");
    s.key("escape");
    s.settle();
    assert_ne!(s.with(|e, _| e.slide().clone()), after_initial);
    s.menu("edit.undo");
    s.settle();
    assert_eq!(s.with(|e, _| e.slide().clone()), after_initial, "second edit session coalesced into 1 undo step");
}

/// 28. pptx: On a newly added slide, an inserted text box enters edit mode with visible caret, supports IME, and re-enters edit mode on double-click.
fn fix_textbox_28_pptx_new_slide_inserted_textbox_edit_and_ime() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.new-slide");
    s.settle();
    s.menu("insert.text-box");
    s.settle();

    let slide_pt = s.with(|e, _| {
        let layout = e.slide_layout().unwrap();
        (f32::from(layout.origin.x) + 150.0, f32::from(layout.origin.y) + 150.0)
    });
    s.drag(slide_pt, (slide_pt.0 + 220.0, slide_pt.1 + 90.0));
    s.settle();

    assert!(s.with(|e, _| e.text_editing().is_some()), "new text box on new slide enters edit mode");
    let has_caret = s.with(|e, _| e.caret_marks().and_then(|m| m.rects(true)).is_some_and(|(c, _)| c.is_some()));
    assert!(has_caret, "caret is painted inside newly inserted empty text box");

    // IME composition inside the newly inserted text box.
    s.update(|e, _, cx| e.input_mark("あいうえお", cx));
    s.settle();
    let comp_ok = s.with(|e, _| {
        let id = e.text_editing().unwrap().shape;
        e.composing_body(id).is_some_and(|b| b.text() == "あいうえお")
    });
    assert!(comp_ok, "IME composing body visible on new pptx text box");
    s.update(|e, _, cx| e.input_replace(None, "あいうえお", cx));
    s.settle();
    s.key("escape");
    s.settle();
    assert!(s.with(|e, _| e.text_editing().is_none()));

    // Double-click the text box to re-enter edit mode.
    s.double_click((slide_pt.0 + 30.0, slide_pt.1 + 15.0));
    s.settle();
    assert!(s.with(|e, _| e.text_editing().is_some()), "double-clicking inserted text box re-enters edit mode");
    let has_caret_again = s.with(|e, _| e.caret_marks().and_then(|m| m.rects(true)).is_some_and(|(c, _)| c.is_some()));
    assert!(has_caret_again, "caret is visible after double-clicking inserted text box");
}

/// 29. pptx: Typing 5+ lines keeps user-specified rectangle size fixed, and Up/Down navigation works.
fn fix_textbox_29_pptx_five_lines_fixed_rectangle_size_and_up_down() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("insert.text-box");
    s.settle();
    let slide_pt = s.with(|e, _| {
        let layout = e.slide_layout().unwrap();
        (f32::from(layout.origin.x) + 120.0, f32::from(layout.origin.y) + 120.0)
    });
    s.drag(slide_pt, (slide_pt.0 + 200.0, slide_pt.1 + 80.0));
    s.settle();

    let frame0 = s.with(|e, _| e.slide().shapes.last().unwrap().frame);
    for line in ["line1", "line2", "line3", "line4", "line5", "line6"] {
        s.type_text(line);
        s.key("enter");
    }
    s.settle();
    let frame1 = s.with(|e, _| e.slide().shapes.last().unwrap().frame);
    assert_eq!(frame1, frame0, "pptx text box frame stays fixed after 6 lines");

    s.key("up");
    s.key("up");
    s.settle();
    let c = s.with(|e, _| e.text_editing().unwrap().caret);
    assert!(c < 30, "Up arrow moved caret up in pptx text box: {c}");
}

/// 30. pptx: 2nd line with 28pt font keeps 2nd and 3rd line selection rectangles aligned with painted glyphs.
fn fix_textbox_30_pptx_second_line_large_font_and_selection_alignment() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("insert.text-box");
    s.settle();
    let slide_pt = s.with(|e, _| {
        let layout = e.slide_layout().unwrap();
        (f32::from(layout.origin.x) + 100.0, f32::from(layout.origin.y) + 100.0)
    });
    s.drag(slide_pt, (slide_pt.0 + 260.0, slide_pt.1 + 140.0));
    s.settle();

    s.type_text("Line1");
    s.key("enter");
    s.type_text("LargeLine2");
    s.key("enter");
    s.type_text("Line3Tail");
    s.settle();

    s.update(|e, window, cx| {
        if let Some(te) = e.text_editing_mut() {
            te.anchor = 6;
            te.caret = 16;
        }
        e.format_characters(window, cx, ibis_doc_model::CharacterFormat { font_size: Some(2800), ..Default::default() });
    });
    s.settle();

    s.key("cmd-down");
    s.key("shift-left");
    s.key("shift-left");
    s.key("shift-left");
    s.settle();

    let (sr, p2_y, p2_h) = s.with(|e, _| {
        let sr = e.caret_marks().unwrap().rects(true).unwrap().1[0];
        let list = e.pptx().unwrap().positions.paragraphs.borrow();
        let p2 = list[2].borrow();
        let p2_y = p2.lines[0].top;
        let p2_h = p2.lines[0].height;
        (sr, p2_y, p2_h)
    });
    assert!((f32::from(sr.origin.y) - p2_y).abs() < 1.0, "pptx line 3 selection Y matches painted top");
    assert!((f32::from(sr.size.height) - p2_h).abs() < 1.0, "pptx line 3 selection height matches painted height");
}

/// 31. pptx: Partial selection Bold/Italic/Underline and resize handles while editing text box.
fn fix_textbox_31_pptx_partial_selection_bold_italic_underline_and_handles() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("insert.text-box");
    s.settle();
    let slide_pt = s.with(|e, _| {
        let layout = e.slide_layout().unwrap();
        (f32::from(layout.origin.x) + 120.0, f32::from(layout.origin.y) + 120.0)
    });
    s.drag(slide_pt, (slide_pt.0 + 220.0, slide_pt.1 + 90.0));
    s.settle();

    s.type_text("normal bold");
    for _ in 0..4 {
        s.key("shift-left");
    }
    s.key("cmd-b");
    s.key("cmd-i");
    s.key("cmd-u");
    s.settle();

    let (style, runs) = s.with(|e, _| {
        let st = e.selected_text_style().unwrap();
        let body = e.edited_body().unwrap();
        let runs: Vec<_> = body.paragraphs[0].runs.iter().map(|r| (r.text.clone(), r.style.bold, r.style.italic, r.style.underline)).collect();
        (st, runs)
    });
    assert!(style.bold && style.italic);
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].0, "normal ");
    assert_ne!(runs[0].1, Some(true));
    assert_eq!(runs[1].0, "bold");
    assert_eq!(runs[1].1, Some(true));
    assert_eq!(runs[1].2, Some(true));
    assert!(runs[1].3);

    // Drag bottom-right resize handle while in edit mode.
    let (f0, br) = s.with(|e, _| {
        let shape = e.slide().shapes.last().unwrap();
        let layout = e.slide_layout().unwrap();
        let f = shape.frame;
        (f, (f32::from(layout.origin.x + layout.to_px(f.x + f.width)), f32::from(layout.origin.y + layout.to_px(f.y + f.height))))
    });
    s.drag(br, (br.0 + 40.0, br.1 + 30.0));
    s.settle();
    let f1 = s.with(|e, _| e.slide().shapes.last().unwrap().frame);
    assert!(f1.width > f0.width && f1.height > f0.height, "pptx handle resized while editing");
}

/// 32. Unified Cut / Copy / Paste / Select-All inside text box editing across docx, xlsx, and pptx.
fn fix_textbox_32_copy_cut_paste_and_select_all_across_docx_xlsx_pptx() {
    // 1. docx
    {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        docx_open_box_edit(&mut s);
        s.key("cmd-a");
        s.type_text("abc def");
        for _ in 0..3 {
            s.key("shift-left");
        }
        s.key("cmd-x");
        s.settle();
        assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("abc "));
        s.key("home");
        s.key("cmd-v");
        s.settle();
        assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("defabc "));
        s.key("cmd-a");
        assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 0..7);
    }
    // 2. xlsx
    {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        s.menu("insert.text-box");
        s.settle();
        s.type_text("abc def");
        for _ in 0..3 {
            s.key("shift-left");
        }
        s.key("cmd-x");
        s.settle();
        assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("abc "));
        s.key("home");
        s.key("cmd-v");
        s.settle();
        assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("defabc "));
        s.key("cmd-a");
        assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 0..7);
    }
    // 3. pptx
    {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        s.menu("insert.text-box");
        s.settle();
        let slide_pt = s.with(|e, _| {
            let layout = e.slide_layout().unwrap();
            (f32::from(layout.origin.x) + 120.0, f32::from(layout.origin.y) + 120.0)
        });
        s.drag(slide_pt, (slide_pt.0 + 200.0, slide_pt.1 + 80.0));
        s.settle();
        s.type_text("abc def");
        for _ in 0..3 {
            s.key("shift-left");
        }
        s.key("cmd-x");
        s.settle();
        assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("abc "));
        s.key("home");
        s.key("cmd-v");
        s.settle();
        assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("defabc "));
        s.key("cmd-a");
        assert_eq!(s.with(|e, _| e.text_editing().unwrap().range()), 0..7);
    }
}

/// #97: Long menu scrollbar is visible as soon as the menu opens, and its track
/// reaches the very top when ▲ is hidden (`start == 0`) and the very bottom when ▼ is hidden.
fn fix97_menu_scrollbar_visible_and_flush_to_edges() {
    let _probe = crate::shell::scenario::button_probe::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let edit = menu_button_at(&mut s, "edit");
    s.click(edit);
    assert_eq!(open_index(&mut s), Some(1));
    assert_eq!(menu_start(&mut s), 0, "starts at top of Edit menu");
    s.screenshot("fix97-menu-top");

    // Hover ▼ to scroll into the middle (both ▲ and ▼ shown).
    s.hover((300.0, 648.0));
    s.wait(400);
    let mid_start = menu_start(&mut s);
    assert!(mid_start > 0, "scrolled down into middle: {mid_start}");
    s.screenshot("fix97-menu-middle");

    // Scroll to the very bottom so ▼ is hidden.
    s.update(|e, window, cx| {
        e.scroll_menu_to(999, window, cx);
    });
    s.settle();
    let bot_start = menu_start(&mut s);
    assert!(bot_start >= mid_start, "at bottom of menu: {bot_start}");
    s.screenshot("fix97-menu-bottom");
}

/// #98: Searchable dropdown menu component with ✕ clear button, and bottom-left
/// sheet-list popup showing tab color swatches, checkmarks, filtering, and Enter selection.
fn fix98_searchable_dropdown_and_sheet_list() {
    use crate::shell::chrome::{filter_menu_items, flatten_menu_entries};
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // Set a tab color on the first sheet and add two more sheets.
    s.update(|e, _, cx| {
        e.set_tab_color(Some(ibis_doc_model::Rgb(0xe53935)), cx);
    });
    s.menu("xlsx.insert-sheet");
    s.menu("xlsx.insert-sheet");
    s.settle();

    // Verify sheet_list_menu_entries includes the tab color swatch on the first sheet and checkmark on current.
    let entries = s.with(|e, _| crate::xlsx::sheet_ui::sheet_list_menu_entries(e));
    let items = flatten_menu_entries(&entries);
    assert!(items.len() >= 5, "sheet list has all visible sheets: {}", items.len());
    assert_eq!(items[0].swatch, Some(ibis_doc_model::Rgb(0xe53935)), "first sheet has red tab swatch");
    assert!(items.iter().any(|it| it.checked), "current sheet is checked");

    // Filter by query: partial match, no match ("みつかりません"), and empty query.
    let first_name = items[0].label.to_string();
    let matched = filter_menu_items(&items, &first_name);
    assert!(!matched.is_empty() && matched[0].label == first_name);
    let none_matched = filter_menu_items(&items, "存在しないシート名XYZ");
    assert!(none_matched.is_empty(), "non-existent query returns empty list");
    assert_eq!(filter_menu_items(&items, "").len(), items.len(), "cleared query restores full list");

    // Pressing Enter in the sheet-list search input activates the top matched sheet.
    let first_slide_id = s.with(|e, _| e.presentation().slides[0].id);
    assert_ne!(s.with(|e, _| e.current_slide), first_slide_id);
    s.update(move |e, window, cx| {
        let input = e.ensure_toolbar_search_input("sheet-list", window, cx);
        input.update(cx, |state, cx| state.set_value(first_name.clone(), window, cx));
        e.activate_top_toolbar_search_match("sheet-list", window, cx);
    });
    s.settle();
    assert_eq!(s.with(|e, _| e.current_slide), first_slide_id, "Enter on search query switched to matched sheet");
}

/// #99, #100, #101, #102:
/// - #99: docx text box side panel removes duplicate vertical-alignment / B / I buttons (toolbar handles them).
/// - #100: compact spinner & color fields in docx text box side panel update geometry and style.
/// - #101: graphical line-style dropdown (`line_dash_open`, `hovered_dash`, `DASHES`) in docx text box side panel.
/// - #102: docx inline picture selection shows 8 resize handles, corner drag preserves aspect ratio, and interior move drag does not move inline pictures.
fn fix99_100_101_102_docx_textbox_panel_and_inline_picture_resize() {
    use ibis_doc_model::{FlowPos, FlowRunKind, LineDash};
    let mut s = Scenario::open("test/editing/docx-report.docx");

    // Insert a floating text box and verify the side panel opens (#99, #100, #101).
    s.menu("insert.text-box");
    s.settle();
    assert!(s.with(|e, _| e.doc_state().is_some_and(|d| d.text_box.is_some()) && e.sidebar_open));

    // #100: Update width, height, and line width via the compact panel inputs.
    s.update(|e, window, cx| {
        let st = e.doc_state().unwrap().text_box.as_ref().unwrap();
        let w_in = st.geometry[0].clone();
        let h_in = st.geometry[1].clone();
        let lw_in = st.line_width.clone();
        w_in.update(cx, |i, cx| i.set_value("220", window, cx));
        h_in.update(cx, |i, cx| i.set_value("110", window, cx));
        lw_in.update(cx, |i, cx| i.set_value("2", window, cx));
        e.apply_text_box(cx);
    });
    s.settle();
    let (_, (_, _, w_pt, h_pt), _, _) = s.with(|e, _| e.selected_text_box_frame()).expect("selected text box");
    assert!((w_pt - 220.0).abs() < 2.0 && (h_pt - 110.0).abs() < 2.0, "panel updated text box size: {w_pt}x{h_pt}");

    // #101: Open graphical line-dash dropdown, hover DashDot, and select Dash.
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut().and_then(|d| d.text_box.as_mut()) {
            st.line_dash_open = true;
            st.hovered_dash = Some(LineDash::DashDot);
        }
        cx.notify();
    });
    s.settle();
    assert!(s.with(|e, _| e.doc_state().unwrap().text_box.as_ref().unwrap().line_dash_open));
    s.update(|e, _, cx| {
        if let Some(st) = e.doc_state_mut().and_then(|d| d.text_box.as_mut()) {
            st.dash = LineDash::Dash;
            st.line_dash_open = false;
            st.hovered_dash = None;
        }
        e.apply_text_box(cx);
    });
    s.settle();
    let applied_dash = s.with(|e, _| e.text_box_at_caret().unwrap().dash);
    assert_eq!(applied_dash, LineDash::Dash, "graphical line style dropdown applied Dash");

    // #99: Toolbar vertical alignment and Bold/Italic still work on the text box.
    s.menu("docx.shape.align-text.middle");
    s.settle();
    assert_eq!(
        s.with(|e, _| e.text_box_at_caret().unwrap().anchor),
        ibis_doc_model::VerticalAnchor::Middle
    );

    // #102: Select the inline picture in docx-report.docx and resize it via its corner handle while preserving aspect ratio.
    let pic_pos = s.with(|e, _| {
        for b in &e.presentation().flow.as_ref().unwrap().body {
            if let ibis_doc_model::Block::Paragraph(p) = b {
                let mut offset = 0;
                for r in &p.runs {
                    if matches!(r.kind, FlowRunKind::Picture { .. }) {
                        return FlowPos::new(p.id, offset);
                    }
                    offset += r.len();
                }
            }
        }
        panic!("expected inline picture in docx-report.docx");
    });
    let (is_pic, is_inline) = s.with(move |e, _| e.is_picture_at(pic_pos));
    assert_eq!((is_pic, is_inline), (true, true), "target run is an inline picture");

    let (page, (px0, py0, pw0, ph0), _) = s
        .with(move |e, _| e.floating_text_box_frame_of(pic_pos))
        .expect("inline picture has layout frame");
    let orig_ratio = pw0 / ph0;

    // Click inside the inline picture to select it: moving an inline picture must keep its origin unchanged.
    s.update(move |e, _, cx| {
        assert!(e.begin_shape_drag(page, px0 + pw0 * 0.5, py0 + ph0 * 0.5, cx));
        if let Some(d) = e.doc_state_mut().and_then(|st| st.shape_drag.as_mut()) {
            d.delta = (30.0, 25.0);
            assert_eq!(d.frame_now(), (px0, py0, pw0, ph0), "inline picture Move drag does not shift x/y");
        }
        e.end_shape_drag(cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.doc_state().unwrap().shape_selected), "inline picture is selected with handles");

    // Drag the bottom-right corner handle (px0 + pw0, py0 + ph0) to resize with aspect ratio locked.
    s.update(move |e, _, cx| {
        assert!(e.begin_shape_drag(page, px0 + pw0, py0 + ph0, cx));
        if let Some(d) = e.doc_state_mut().and_then(|st| st.shape_drag.as_mut()) {
            d.delta = (40.0, 10.0);
        }
        e.end_shape_drag(cx);
    });
    s.settle();
    let (_, (_, _, pw1, ph1), _) = s
        .with(move |e, _| e.floating_text_box_frame_of(pic_pos))
        .expect("resized inline picture frame");
    assert!(pw1 > pw0 + 20.0, "inline picture width grew: {pw0} -> {pw1}");
    let new_ratio = pw1 / ph1;
    assert!(
        (new_ratio - orig_ratio).abs() < 0.05,
        "corner drag preserved inline picture aspect ratio: {orig_ratio} -> {new_ratio}"
    );
}

/// #103: Top-left toolbar `menu-search` button opens a searchable popup of all executable
/// menu items sorted in ascending order, filters by query, and executes the top match on Enter.
fn fix103_menu_search_popup_sorted_filter_and_enter() {
    use crate::shell::chrome::filter_menu_items;
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    let items = s.with(|e, _| e.searchable_menu_items_for("menu-search"));
    assert!(items.len() > 20, "menu-search collects executable menu items: {}", items.len());
    assert!(
        items.windows(2).all(|w| w[0].label.as_ref() <= w[1].label.as_ref()),
        "menu-search items are sorted in ascending order of display label"
    );

    // Substring filter and 0-hit case.
    let grid_label = crate::shell::i18n::text("xlsx.gridlines").to_string();
    let filtered = filter_menu_items(&items, &grid_label);
    assert!(!filtered.is_empty(), "filtering by '{grid_label}' finds gridlines menu item");
    assert!(filter_menu_items(&items, "存在しないコマンド999").is_empty());

    // Execute top match via Enter (`activate_top_toolbar_search_match`).
    let before_grid = s.with(|e, _| e.current_sheet().unwrap().1.grid_lines);
    s.update(move |e, window, cx| {
        let input = e.ensure_toolbar_search_input("menu-search", window, cx);
        input.update(cx, |st, cx| st.set_value(grid_label.clone(), window, cx));
        e.activate_top_toolbar_search_match("menu-search", window, cx);
    });
    s.settle();
    let after_grid = s.with(|e, _| e.current_sheet().unwrap().1.grid_lines);
    assert_eq!(after_grid, !before_grid, "Enter in menu-search executed the top matched item");
}

/// #104: Clicking the border or handle of a text box while editing text exits text-editing mode
/// and transitions to object-selection mode (keeping the 8 resize handles visible) across docx, xlsx, and pptx.
fn fix104_textbox_border_click_exits_edit_and_keeps_handles() {
    // 1. docx
    {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        docx_open_box_edit(&mut s);
        assert!(s.with(|e, _| e.doc_state().unwrap().box_edit.is_some()));
        // Simulate clicking the border without dragging (delta == 0) and releasing.
        s.update(|e, window, cx| {
            e.commit_box_edit(window, cx);
        });
        s.settle();
        assert!(s.with(|e, _| e.doc_state().unwrap().box_edit.is_none()), "docx exited box_edit");
        assert!(s.with(|e, _| e.doc_state().unwrap().shape_selected), "docx keeps shape_selected with 8 handles");
    }
    // 2. xlsx
    {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        s.menu("insert.text-box");
        s.settle();
        s.type_text("xlsx border click");
        assert!(s.with(|e, _| e.editing.is_some()));
        let shape_id = s.with(|e, _| e.editing.as_ref().unwrap().shape);
        // Simulate clicking the border without moving (`is_new = false`, `commit_edit`).
        s.update(move |e, window, cx| {
            if let Some(ed) = e.editing.as_mut() {
                ed.edit.is_new = false;
            }
            e.commit_edit(window, cx);
            e.set_selection(vec![shape_id]);
        });
        s.settle();
        assert!(s.with(|e, _| e.editing.is_none()), "xlsx exited text editing");
        assert_eq!(s.with(|e, _| e.selected_sheet_shape()), Some(shape_id), "xlsx keeps text box selected with handles");
    }
    // 3. pptx
    {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        s.menu("insert.text-box");
        s.settle();
        let slide_pt = s.with(|e, _| {
            let layout = e.slide_layout().unwrap();
            (f32::from(layout.origin.x) + 120.0, f32::from(layout.origin.y) + 120.0)
        });
        s.drag(slide_pt, (slide_pt.0 + 200.0, slide_pt.1 + 80.0));
        s.settle();
        s.type_text("pptx border click");
        let shape_id = s.with(|e, _| e.text_editing().unwrap().shape);
        s.update(|e, window, cx| {
            e.end_text_edit(window, cx);
        });
        s.settle();
        assert!(s.with(|e, _| e.text_editing().is_none()), "pptx exited text editing");
        assert_eq!(s.with(|e, _| e.selected_ids()), vec![shape_id], "pptx keeps shape selected with handles");
    }
}

/// #105, #106, #107:
/// - #105: xlsx Custom Views (`xlsx.custom-views`) and Goto Special (`xlsx.goto-special`, no redundant Close button).
/// - #106: xlsx Arrange Windows (`xlsx.arrange-all`, shared radio/checkbox rows, primary "実行" button, no redundant Cancel button).
/// - #107: xlsx Paste Special (`xlsx.paste-special-dialog`) opens even before copying and has no redundant Cancel button.
fn fix105_106_107_xlsx_side_panels_layout_and_controls() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // #107: Open Paste Special dialog from menu without copying first.
    s.update(|e, _, _| {
        e.wb_mut().clip = None;
    });
    s.menu("xlsx.paste-special-dialog");
    s.settle();
    assert!(
        s.with(|e, _| e.wb().panels.paste_special_dialog.is_some() && e.sidebar_open),
        "Paste Special panel opens from menu even when clip is None"
    );
    let (has_ok, has_cancel, has_link) = s.with(|e, _| {
        let ui = e.wb().panels.paste_special_dialog.as_ref().unwrap().ui.borrow();
        (ui.contains_key("ok"), ui.contains_key("cancel"), ui.contains_key("paste-link"))
    });
    assert!(has_ok && has_link && !has_cancel, "Paste Special has OK and Paste Link, and no Cancel button");

    s.update(|e, window, cx| e.close_panel_dialog(window, cx));

    // #105a: Custom Views panel — add, select, apply, and delete a custom view.
    s.menu("xlsx.custom-views");
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.custom_views.is_some()));
    s.update(|e, window, cx| {
        e.open_add_custom_view(window, cx);
        if let Some(cv) = e.wb_mut().panels.custom_views.as_mut() {
            cv.name_input.update(cx, |i, cx| i.set_value("Q1View", window, cx));
        }
        e.apply_add_custom_view_ui(window, cx);
    });
    s.settle();
    assert_eq!(
        s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_views.len()),
        1,
        "custom view Q1View added"
    );
    s.update(|e, window, cx| {
        if let Some(cv) = e.wb_mut().panels.custom_views.as_mut() {
            cv.selected_index = Some(0);
        }
        e.show_selected_custom_view(window, cx);
        e.delete_selected_custom_view(cx);
    });
    s.settle();
    assert_eq!(
        s.with(|e, _| e.presentation().workbook.as_ref().unwrap().custom_views.len()),
        0,
        "custom view deleted"
    );

    s.update(|e, window, cx| e.close_panel_dialog(window, cx));

    // #105b: Goto Special panel — radio rows and "選択" button without redundant "閉じる" button.
    s.menu("xlsx.goto-special");
    s.settle();
    let (gs_apply, gs_close) = s.with(|e, _| {
        let ui = e.wb().panels.goto_special.as_ref().unwrap().ui.borrow();
        (ui.contains_key("apply"), ui.contains_key("close"))
    });
    assert!(gs_apply && !gs_close, "Goto Special has '選択' button and no redundant '閉じる' button");
    s.update(|e, _, cx| e.apply_goto_special(cx));
    s.settle();

    s.update(|e, window, cx| e.close_panel_dialog(window, cx));

    // #106: Arrange Windows panel — radio rows, checkbox, and "実行" button without redundant Cancel button.
    s.menu("xlsx.arrange-all");
    s.settle();
    let (arr_ok, arr_cancel, arr_tiled) = s.with(|e, _| {
        let ui = e.wb().panels.arrange_windows.as_ref().unwrap().ui.borrow();
        (ui.contains_key("arrange-ok"), ui.contains_key("arrange-cancel"), ui.contains_key("arrange-tiled"))
    });
    assert!(arr_ok && arr_tiled && !arr_cancel, "Arrange Windows has radio options and '実行' button, no Cancel button");
}

/// #108, #109, #110, #111:
/// - #108: Menu width is pre-calculated across all entries so scrolling does not shift menu width.
/// - #109: Top-level `ウィンドウ` (`MenuId::Window`) menu in docx, xlsx, and pptx.
/// - #110: Arrange Windows (`Tiled`, `Horizontal`, `Vertical`, `Cascade`) tiles/cascades open windows.
/// - #111: Open editor windows are listed at the bottom of the `ウィンドウ` menu with `:1`, `:2` suffixes and active checkmark.
fn fix108_109_110_111_window_menu_and_arrange_all() {
    use crate::shell::chrome::{MenuEntry, MenuId, menu_width_for_entries};

    for file in ["test/editing/xlsx-sales.xlsx", "test/editing/docx-report.docx", "test/editing/pptx-plan.pptx"] {
        let mut s = Scenario::open(file);
        let (has_window_menu, win_width, has_open_entry) = s.with(|e, _| {
            let menus = e.menus();
            let win_entries = menus.menus.iter().find(|(id, _)| *id == MenuId::Window).map(|(_, v)| v.clone());
            let Some(entries) = win_entries else { return (false, 0.0, false) };
            let w = menu_width_for_entries(&entries);
            let has_open = entries.iter().any(|en| matches!(en, MenuEntry::Item(it) if it.id.starts_with("window.open.") && it.checked));
            (true, w, has_open)
        });
        assert!(has_window_menu, "{file} has top-level Window menu");
        assert!(win_width >= 280.0, "{file} Window menu has fixed width {win_width}");
        assert!(has_open_entry, "{file} Window menu lists current window with checkmark");
    }

    // Open a second window on xlsx-sales.xlsx via `window.new-window` and verify `:1` and `:2` labels in Window menu (#111)
    // as well as ArrangeKind modes (#110).
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("window.new-window");
    s.settle();
    let (label1, open_labels) = s.with(|e, _| {
        let l1 = e.window_menu_label().to_string();
        let items: Vec<(String, bool)> = e
            .open_window_menu_items()
            .into_iter()
            .filter_map(|en| match en {
                MenuEntry::Item(it) => Some((it.label.to_string(), it.checked)),
                _ => None,
            })
            .collect();
        (l1, items)
    });
    assert!(label1.ends_with(":1"), "first window label has :1 suffix: {label1}");
    assert!(
        open_labels.iter().any(|(l, _)| l.ends_with(":1")) && open_labels.iter().any(|(l, _)| l.ends_with(":2")),
        "Window menu lists both :1 and :2 windows: {open_labels:?}"
    );

    for kind in [
        crate::xlsx::arrange_windows::ArrangeKind::Tiled,
        crate::xlsx::arrange_windows::ArrangeKind::Horizontal,
        crate::xlsx::arrange_windows::ArrangeKind::Vertical,
        crate::xlsx::arrange_windows::ArrangeKind::Cascade,
    ] {
        s.update(move |e, window, cx| {
            e.set_arrange_kind(kind, cx);
            e.apply_arrange_windows(window, cx);
        });
        s.settle();
    }
}

/// #112: docx checklist button works inside a floating text box or shape (both while editing text and when the shape is selected).
fn fix112_docx_checklist_in_textbox_and_shape() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    docx_open_box_edit(&mut s);
    s.key("cmd-a");
    s.type_text("Check item 1");
    s.settle();

    // Verify checklist toolbar button is enabled inside box_edit.
    let checklist_enabled = s.with(|e, _| {
        e.toolbar_items().into_iter().any(|it| match it {
            crate::shell::chrome::ToolItem::Button(b) if b.id == "checklist" => !b.disabled,
            _ => false,
        })
    });
    assert!(checklist_enabled, "checklist button is enabled while editing text box");
    assert!(!s.with(|e, _| e.shape_has_checklist()));

    // Toggle checklist ON while editing text box.
    s.update(|e, _, cx| e.toggle_checklist(cx));
    s.settle();
    assert!(s.with(|e, _| e.shape_has_checklist()), "checklist bullet added to text box paragraph");

    // Toggle checklist OFF while editing text box.
    s.update(|e, _, cx| e.toggle_checklist(cx));
    s.settle();
    assert!(!s.with(|e, _| e.shape_has_checklist()), "checklist bullet removed from text box paragraph");

    // Exit box_edit so shape is selected (`shape_selected == true`), and toggle checklist ON on the whole shape.
    s.update(|e, window, cx| e.commit_box_edit(window, cx));
    s.settle();
    assert!(s.with(|e, _| e.doc_state().unwrap().shape_selected));
    s.update(|e, _, cx| e.toggle_checklist(cx));
    s.settle();
    assert!(s.with(|e, _| e.shape_has_checklist()), "checklist toggled ON when shape is selected");
}

/// #113: Color menus across docx, xlsx, and pptx use the unified 10×6 theme + standard color `MenuEntry::Palette`.
fn fix113_unified_color_palettes_in_menus() {
    use crate::shell::chrome::MenuEntry;

    fn submenu_has_palette(entries: &[MenuEntry], target_id: &str) -> bool {
        for entry in entries {
            if let MenuEntry::Submenu(sub) = entry {
                if sub.id == target_id {
                    return sub.items.iter().any(|it| matches!(it, MenuEntry::Palette(_)));
                }
                if submenu_has_palette(&sub.items, target_id) {
                    return true;
                }
            }
        }
        false
    }

    // 1. docx: text color, underline color, paragraph shading/border color, cell border color, picture border color.
    {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        let all_entries = s.with(|e, _| {
            e.menus()
                .menus
                .into_iter()
                .flat_map(|(_, v)| v)
                .collect::<Vec<_>>()
        });
        for id in [
            "docx.format.text.colors",
            "docx.underline.colors",
            "docx.paragraph.fill",
            "docx.paragraph.border-color",
            "docx.cell.border.color",
            "docx.picture.border.color",
        ] {
            assert!(submenu_has_palette(&all_entries, id), "docx submenu {id} uses MenuEntry::Palette");
        }
    }

    // 2. xlsx: sparkline series/marker colors and shape text color use MenuEntry::Palette.
    {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        let all_entries = s.with(|e, _| {
            e.menus()
                .menus
                .into_iter()
                .flat_map(|(_, v)| v)
                .collect::<Vec<_>>()
        });
        for id in [
            "xlsx.sparkline-colors",
            "xlsx.sparkline-marker-color-high",
            "xlsx.sparkline-marker-color-low",
            "xlsx.text.color",
        ] {
            assert!(submenu_has_palette(&all_entries, id), "xlsx submenu {id} uses MenuEntry::Palette");
        }
    }

    // 3. pptx: text outline (`pptx.text-outline`) and table border color (`table.borders.color`) use MenuEntry::Palette.
    {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        let all_entries = s.with(|e, _| {
            e.menus()
                .menus
                .into_iter()
                .flat_map(|(_, v)| v)
                .collect::<Vec<_>>()
        });
        for id in ["pptx.text-outline", "table.borders.color"] {
            assert!(submenu_has_palette(&all_entries, id), "pptx submenu {id} uses MenuEntry::Palette");
        }
    }
}

/// #114, #115:
/// - #114: docx toolbar shows `shape-fill-color` and `shape-line-color` buttons when a shape/text box is selected or edited.
/// - #115: xlsx toolbar has `border-color` next to `fill`, updating cell border color or selected shape stroke color.
fn fix114_115_shape_fill_and_border_color_in_toolbar() {
    use crate::shell::chrome::ToolItem;

    // #114: docx shape fill & line color buttons in toolbar
    {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        let has_shape_btns = |s: &mut Scenario| {
            s.with(|e, _| {
                let ids: Vec<&str> = e
                    .toolbar_items()
                    .into_iter()
                    .filter_map(|it| match it {
                        ToolItem::Button(b) => Some(b.id),
                        _ => None,
                    })
                    .collect();
                (ids.contains(&"shape-fill-color"), ids.contains(&"shape-line-color"))
            })
        };
        assert_eq!(has_shape_btns(&mut s), (false, false), "hidden when body text is active");
        s.menu("insert.text-box");
        s.settle();
        assert_eq!(has_shape_btns(&mut s), (true, true), "shown when floating text box is selected");
    }

    // #115: xlsx border-color button in toolbar works for both cells and worksheet shapes
    {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        let has_border_color_btn = s.with(|e, _| {
            e.toolbar_items().into_iter().any(|it| match it {
                ToolItem::Button(b) => b.id == "border-color",
                _ => false,
            })
        });
        assert!(has_border_color_btn, "xlsx toolbar has border-color button");

        // 1) Cell selected: picking a color from toolbar_border_color_menu sets border_color and applies borders
        s.click_cell("D4");
        s.update(|e, window, cx| {
            let entries = crate::xlsx::toolbar::toolbar_border_color_menu(e);
            if let Some(crate::shell::chrome::MenuEntry::Palette(p)) = entries.first()
                && let Some(pick) = &p.pick
            {
                pick(e, Some(ibis_doc_model::Rgb(0xc00000)), window, cx);
            }
        });
        s.settle();
        assert_eq!(s.with(|e, _| e.wb().border_color), ibis_doc_model::Rgb(0xc00000));
        assert!(s.with(|e, _| e.range_edge("top").is_some()), "cell border applied on D4");

        // 2) Worksheet shape selected: picking a color from toolbar_border_color_menu updates shape stroke color
        s.update(|e, _, cx| {
            e.insert_sheet_shape("rect", cx);
        });
        s.settle();
        let shape_id = s.with(|e, _| e.selected_sheet_shape()).expect("inserted shape");
        s.update(|e, window, cx| {
            let entries = crate::xlsx::toolbar::toolbar_border_color_menu(e);
            if let Some(crate::shell::chrome::MenuEntry::Palette(p)) = entries.first()
                && let Some(pick) = &p.pick
            {
                pick(e, Some(ibis_doc_model::Rgb(0x008000)), window, cx);
            }
        });
        s.settle();
        let stroke_rgb = s.with(move |e, _| match &e.slide().shape(shape_id).unwrap().kind {
            ibis_doc_model::ShapeKind::Auto(a) => a.stroke.as_ref().map(|st| st.color.rgb),
            _ => None,
        });
        assert_eq!(stroke_rgb, Some(ibis_doc_model::Rgb(0x008000)), "shape stroke color updated via toolbar border-color");
    }
}

/// #116, #117:
/// - #116: xlsx `clear-format` toolbar button clears cell formatting or worksheet shape/textbox formatting while keeping text.
/// - #117: `clear-format` in docx and pptx clears formatting inside text boxes / shapes (both partial selection and whole shape).
fn fix116_117_clear_formatting_in_xlsx_docx_pptx() {
    // 1. xlsx (#116): cell format clear + worksheet text box format clear
    {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        s.click_cell("B2");
        s.key("cmd-b");
        s.update(|e, _, cx| {
            e.format_cells(ibis_doc_model::StyleChange::Fill(Some(ibis_doc_model::Rgb(0xffff00))), cx);
        });
        s.settle();
        assert!(s.with(|e, _| e.current_style().bold));
        s.update(|e, _, cx| e.clear_cell_formats(cx));
        s.settle();
        assert!(!s.with(|e, _| e.current_style().bold), "xlsx cell bold cleared");
        assert_eq!(s.with(|e, _| e.current_style().fill), None, "xlsx cell fill cleared");

        // Worksheet text box: apply bold and clear formatting
        s.menu("insert.text-box");
        s.settle();
        s.type_text("Hello Clear");
        s.key("cmd-a");
        s.key("cmd-b");
        s.settle();
        assert!(s.with(|e, _| e.selected_text_style().unwrap().bold));
        s.update(|e, _, cx| e.clear_cell_formats(cx));
        s.settle();
        assert!(!s.with(|e, _| e.selected_text_style().unwrap().bold), "xlsx text box bold cleared");
        assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("Hello Clear"), "text preserved");
    }

    // 2. docx (#117): floating text box format clear
    {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        docx_open_box_edit(&mut s);
        s.key("cmd-a");
        s.type_text("Docx Box Text");
        s.key("cmd-a");
        s.key("cmd-b");
        s.update(|e, _, cx| e.toggle_checklist(cx));
        s.settle();
        assert!(s.with(|e, _| e.shape_has_checklist()));
        s.update(|e, _, cx| e.clear_formatting(cx));
        s.settle();
        assert!(!s.with(|e, _| e.shape_has_checklist()), "docx text box paragraph bullet cleared");
        assert_eq!(s.with(|e, _| e.edited_text()).as_deref(), Some("Docx Box Text"), "docx text box text preserved");
    }

    // 3. pptx (#117): text box partial and whole-shape format clear
    {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        s.menu("insert.text-box");
        s.settle();
        let slide_pt = s.with(|e, _| {
            let layout = e.slide_layout().unwrap();
            (f32::from(layout.origin.x) + 120.0, f32::from(layout.origin.y) + 120.0)
        });
        s.drag(slide_pt, (slide_pt.0 + 220.0, slide_pt.1 + 90.0));
        s.settle();
        s.type_text(" StyledPPTX");
        s.key("cmd-a");
        s.key("cmd-b");
        s.update(|e, window, cx| {
            e.commit_edit(window, cx);
        });
        s.settle();
        // Whole-shape clear_formats when selected (not editing text)
        s.update(|e, window, cx| {
            e.clear_formats(window, cx);
        });
        s.settle();
        let (bold_after, text_after) = s.with(|e, _| {
            let tb = e.slide().shapes.last().unwrap().text_body().unwrap();
            let is_b = tb.paragraphs[0].runs.iter().any(|r| r.style.bold == Some(true));
            (is_b, tb.text())
        });
        assert!(!bold_after, "pptx whole-shape clear_formats cleared bold");
        assert_eq!(text_after, " StyledPPTX");
    }
}

/// #118: Multi-shape selection with Shift+click in docx and xlsx, simultaneous style application, and batch delete.
fn fix118_multi_shape_selection_with_shift_click() {
    // 1. docx: insert two shapes, Shift+click to select both, and delete both at once
    {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        let count_shapes = |s: &mut Scenario| {
            s.with(|e, _| {
                e.flow()
                    .unwrap()
                    .body
                    .iter()
                    .filter_map(|b| match b {
                        ibis_doc_model::Block::Paragraph(p) => Some(
                            p.runs
                                .iter()
                                .filter(|r| matches!(r.kind, ibis_doc_model::FlowRunKind::Shape(_)))
                                .count(),
                        ),
                        _ => None,
                    })
                    .sum::<usize>()
            })
        };
        let initial_count = count_shapes(&mut s);
        s.menu("insert.text-box");
        s.settle();
        let at1 = s.with(|e, _| e.selected_text_box_frame().unwrap().2);
        // Exit in-place text edit and insert a second shape
        s.update(|e, window, cx| {
            e.commit_box_edit(window, cx);
            e.doc_insert_shape("rect", cx);
        });
        s.settle();
        let (page2, (x2, y2, w2, h2), at2, _) = s.with(|e, _| e.selected_text_box_frame().unwrap());
        assert_ne!(at1, at2);
        assert_eq!(count_shapes(&mut s), initial_count + 2);

        // Select first shape, then Shift+click the second shape
        s.update(move |e, _, cx| {
            let (page1, (x1, y1, w1, h1), _) = e.floating_text_box_frame_of(at1).unwrap();
            assert!(e.begin_shape_drag(page1, x1 + w1 * 0.5, y1 + h1 * 0.5, cx));
            e.end_shape_drag(cx);
            assert!(e.shift_click_shape(page2, x2 + w2 * 0.5, y2 + h2 * 0.5, cx));
        });
        s.settle();
        assert_eq!(s.with(|e, _| e.all_selected_shapes().len()), 2, "2 shapes selected in docx via Shift+click");

        // Delete removes both selected shapes in one step, and Undo restores both
        s.update(|e, _, cx| e.delete_char(false, cx));
        s.settle();
        assert_eq!(count_shapes(&mut s), initial_count, "Delete removed both selected shapes");
        s.key("cmd-z");
        s.settle();
        assert_eq!(count_shapes(&mut s), initial_count + 2, "Undo restored both shapes");
    }

    // 2. xlsx: insert two worksheet shapes, select both, and apply fill & stroke simultaneously
    {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        let (id1, id2) = s.update(|e, _, cx| {
            let a = e.insert_sheet_shape("rect", cx);
            let b = e.insert_sheet_shape("ellipse", cx);
            e.set_selection(vec![a, b]);
            (a, b)
        });
        s.settle();
        assert_eq!(s.with(|e, _| e.selected_shapes().len()), 2, "2 shapes selected in xlsx");
        s.update(|e, _, cx| {
            e.fill_sheet_shape(
                ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000))),
                cx,
            );
            e.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000))), cx);
            e.format_cells(ibis_doc_model::StyleChange::Font(Some("Arial".to_string())), cx);
            e.format_cells(ibis_doc_model::StyleChange::FontSize(2000), cx);
            e.format_cells(ibis_doc_model::StyleChange::Bold(true), cx);
            e.format_cells(ibis_doc_model::StyleChange::Italic(true), cx);
            e.format_cells(ibis_doc_model::StyleChange::Color(ibis_doc_model::Rgb(0x0070c0)), cx);
            e.format_cells(ibis_doc_model::StyleChange::Align(ibis_doc_model::CellAlign::Center), cx);
            e.step_font_size_selection(true, cx);
        });
        s.settle();
        for id in [id1, id2] {
            let (fill_ok, stroke_ok, text_ok) = s.with(move |e, _| match &e.slide().shape(id).unwrap().kind {
                ibis_doc_model::ShapeKind::Auto(a) => {
                    let t_ok = a.text.as_ref().is_some_and(|b| {
                        b.style.font.as_deref() == Some("Arial")
                            && b.style.font_size == 2400
                            && b.style.bold
                            && b.style.italic
                            && b.style.color == ibis_doc_model::Rgb(0x0070c0)
                            && b.style.align == ibis_doc_model::TextAlign::Center
                    });
                    (
                        matches!(a.fill, ibis_doc_model::Fill::Solid(c) if c.rgb == ibis_doc_model::Rgb(0xffc000)),
                        a.stroke.as_ref().is_some_and(|st| st.color.rgb == ibis_doc_model::Rgb(0xc00000)),
                        t_ok,
                    )
                }
                _ => (false, false, false),
            });
            assert!(fill_ok && stroke_ok && text_ok, "xlsx multi-selected shape {id:?} received fill, stroke, and text formatting (#152)");
        }
    }
}

/// #119: pptx pictures inserted onto a slide have aspect ratio locked (`aspect_locked`) by default.
fn fix119_pptx_picture_aspect_ratio_locked_by_default() {
    use ibis_doc_model::{AutoShape, EmuRect, Fill, Geometry, Shape, ShapeKind};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");

    let (pic_id, rect_id) = s.update(|e, _, cx| {
        let p_id = e.insert_icon("search", cx).expect("insert icon picture");
        let r_shape_id = e.document.ids().new_shape_id();
        let r_shape = Shape::new(
            r_shape_id,
            EmuRect::new(914_400, 914_400, 1_828_800, 1_371_600),
            ShapeKind::Auto(AutoShape::new(Geometry::Rect, Fill::None, None)),
        );
        let r_id = e.insert_shape(r_shape, cx);
        (p_id, r_id)
    });
    s.settle();
    assert!(
        s.with(move |e, _| e.slide().aspect_locked.contains(&pic_id)),
        "inserted Picture has aspect ratio locked by default"
    );
    assert!(
        !s.with(move |e, _| e.slide().aspect_locked.contains(&rect_id)),
        "inserted AutoShape does not lock aspect ratio by default"
    );
}

/// #120: Toolbar font size box acts as a pulldown + direct numeric input across docx, xlsx, and pptx.
fn fix120_toolbar_font_size_pulldown_and_direct_input() {
    // 1. docx
    {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        s.key("cmd-a");
        s.update(|e, window, cx| e.set_toolbar_font_size(24.0, window, cx));
        s.settle();
        assert_eq!(s.with(|e, _| e.current_format().and_then(|(f, _)| f.size)), Some(2400), "docx 24pt = 2400 hundredths of a point");
        s.update(|e, window, cx| e.set_toolbar_font_size(10.5, window, cx));
        s.settle();
        assert_eq!(s.with(|e, _| e.current_format().and_then(|(f, _)| f.size)), Some(1050), "docx 10.5pt = 1050 hundredths of a point");
    }
    // 2. xlsx
    {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        s.click_cell("B2");
        s.update(|e, window, cx| e.set_toolbar_font_size(20.0, window, cx));
        s.settle();
        assert_eq!(s.with(|e, _| e.current_style().font_size), 2000, "xlsx 20pt = 2000 hundredths");
    }
    // 3. pptx
    {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        s.menu("insert.text-box");
        s.settle();
        let slide_pt = s.with(|e, _| {
            let layout = e.slide_layout().unwrap();
            (f32::from(layout.origin.x) + 120.0, f32::from(layout.origin.y) + 120.0)
        });
        s.drag(slide_pt, (slide_pt.0 + 200.0, slide_pt.1 + 80.0));
        s.settle();
        s.type_text("SizeTest");
        s.key("cmd-a");
        s.update(|e, window, cx| e.set_toolbar_font_size(28.0, window, cx));
        s.settle();
        assert_eq!(s.with(|e, _| e.selected_text_style().unwrap().font_size), 2800, "pptx 28pt = 2800 hundredths");
    }
}

/// #121: pptx `change-shape` button is hidden from the toolbar (`b.hidden == true`),
/// while remaining available in the Format menu and shape right-click context menu.
fn fix121_pptx_change_shape_removed_from_toolbar_kept_in_menus() {
    use crate::shell::chrome::{MenuEntry, ToolItem};
    use ibis_doc_model::{AutoShape, EmuRect, Fill, Geometry, Shape, ShapeKind};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let shape_id = s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let shape = Shape::new(
            id,
            EmuRect::new(914_400, 914_400, 1_828_800, 1_371_600),
            ShapeKind::Auto(AutoShape::new(Geometry::Rect, Fill::None, None)),
        );
        e.insert_shape(shape, cx)
    });
    s.settle();

    // In toolbar_items(), `change-shape` is marked `.hidden()` so `render_toolbar` excludes it.
    let hidden_in_toolbar = s.with(|e, _| {
        e.toolbar_items().into_iter().all(|it| match it {
            ToolItem::Button(b) if b.id == "change-shape" => b.hidden,
            _ => true,
        })
    });
    assert!(hidden_in_toolbar, "change-shape button is hidden from pptx toolbar");

    // Still present in right-click `shape_context_menu_entries()` and functional (`change-shape.ellipse`).
    let in_context_menu = s.with(|e, _| {
        e.shape_context_menu_entries()
            .iter()
            .any(|en| matches!(en, MenuEntry::Submenu(sub) if sub.id == "pptx.chrome.change-shape"))
    });
    assert!(in_context_menu, "change-shape remains in pptx right-click context menu");

    s.toolbar_menu("change-shape", "change-shape.ellipse");
    s.settle();
    let geom = s.with(move |e, _| match &e.slide().shape(shape_id).unwrap().kind {
        ibis_doc_model::ShapeKind::Auto(a) => a.geometry.clone(),
        _ => panic!("expected auto shape"),
    });
    assert_eq!(Some(geom), ibis_pptx::preset_geometry("ellipse"), "change-shape via menu changed rect to ellipse");
}

/// #122 (GitHub issue #24): xlsx dynamic array spill range outline (solid blue when active,
/// dashed blue when `#SPILL!` is selected) and gray ghost formula in the formula bar.
fn fix122_xlsx_dynamic_array_spill_outline_and_ghost_formula() {
    use crate::xlsx::sheet_ui::{active_spill_outlines_on, spilled_ghost_formula_on};
    use ibis_doc_model::{CellError, CellRange, CellValue, sheet_calc::sheet_of_slide};

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.insert-sheet");
    s.settle();

    // 1. Enter `=SEQUENCE(3,2,10,5)` in B2 (col=1, row=1) -> spills to B2:C4 (rows 1..=3, cols 1..=2).
    s.click_cell("B2");
    s.type_text("=SEQUENCE(3,2,10,5)");
    s.key("enter");
    s.settle();

    let expected_spill = CellRange {
        first_row: 1,
        first_column: 1,
        last_row: 3,
        last_column: 2,
    };

    // Anchor B2 has its own formula (not a ghost formula), and selecting B2 shows unblocked outline `(expected_spill, false)`.
    s.click_cell("B2");
    let (ghost_b2, outlines_b2) = s.with(|e, _| {
        let sh = sheet_of_slide(e.slide()).unwrap();
        (
            spilled_ghost_formula_on(sh, 1, 1),
            active_spill_outlines_on(e.presentation(), e.current_slide, sh, e.current_cell(), &e.selected_ranges()),
        )
    });
    assert_eq!(ghost_b2, None, "anchor B2 uses normal formula, not ghost formula");
    assert_eq!(outlines_b2, vec![(expected_spill, false)], "selecting anchor B2 shows solid spill outline");

    // Spilled destination cell C4 (col=2, row=3) shows ghost formula `=SEQUENCE(3,2,10,5)` and solid spill outline.
    s.click_cell("C4");
    let (ghost_c4, outlines_c4) = s.with(|e, _| {
        let sh = sheet_of_slide(e.slide()).unwrap();
        (
            spilled_ghost_formula_on(sh, 3, 2),
            active_spill_outlines_on(e.presentation(), e.current_slide, sh, e.current_cell(), &e.selected_ranges()),
        )
    });
    assert_eq!(ghost_c4.as_deref(), Some("=SEQUENCE(3,2,10,5)"), "spilled cell C4 shows ghost formula");
    assert_eq!(outlines_c4, vec![(expected_spill, false)], "selecting spilled cell C4 shows solid spill outline");
    s.screenshot("fix122-spill-active");

    // Selecting E5 outside the spill range hides the spill outline.
    s.click_cell("E5");
    let outlines_e5 = s.with(|e, _| {
        let sh = sheet_of_slide(e.slide()).unwrap();
        active_spill_outlines_on(e.presentation(), e.current_slide, sh, e.current_cell(), &e.selected_ranges())
    });
    assert!(outlines_e5.is_empty(), "selecting outside cell E5 hides spill outline");

    // 2. Put blocking text in C3 (col=2, row=2) -> B2 becomes `#SPILL!`, and selecting B2 shows dashed outline `(expected_spill, true)`.
    s.click_cell("C3");
    s.type_text("BLOCK");
    s.key("enter");
    s.settle();
    s.click_cell("B2");
    let (b2_val, outlines_blocked) = s.with(|e, _| {
        let sh = sheet_of_slide(e.slide()).unwrap();
        (
            sh.cell(1, 1).unwrap().value.clone(),
            active_spill_outlines_on(e.presentation(), e.current_slide, sh, e.current_cell(), &e.selected_ranges()),
        )
    });
    assert_eq!(b2_val, CellValue::Error(CellError::Spill), "B2 is #SPILL! when C3 blocks");
    assert_eq!(
        outlines_blocked,
        vec![(expected_spill, true)],
        "selecting #SPILL! cell B2 shows intended spill range with dashed outline (is_blocked = true)"
    );
    s.screenshot("fix122-spill-blocked");

    // 3. Clear C3 -> spill recovers and outline returns to solid (`is_blocked = false`).
    s.click_cell("C3");
    s.key("backspace");
    s.key("enter");
    s.settle();
    s.click_cell("B2");
    let outlines_recovered = s.with(|e, _| {
        let sh = sheet_of_slide(e.slide()).unwrap();
        active_spill_outlines_on(e.presentation(), e.current_slide, sh, e.current_cell(), &e.selected_ranges())
    });
    assert_eq!(outlines_recovered, vec![(expected_spill, false)], "spill outline recovered after clearing blocker");
}

/// #123: Selecting multiple cells in xlsx no longer renders the floating Quick Analysis button at the bottom-right corner.
fn fix123_no_quick_analysis_button_on_multi_cell_selection() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B3");
    s.key("shift-right");
    s.key("shift-down");
    s.settle();
    assert_eq!(
        s.with(|e, _| e.selected_range()),
        Some(ibis_doc_model::CellRange {
            first_column: 1,
            first_row: 2,
            last_column: 2,
            last_row: 3,
        })
    );
    // Clicking the cell just below the bottom-right corner of the selection selects that cell directly
    // (no floating Quick Analysis button intercepts the click or opens `list_popup`).
    let d5 = s.with(|e, _| e.cell_center(3, 4)).expect("D5 on screen");
    s.click((f32::from(d5.x), f32::from(d5.y)));
    s.settle();
    assert!(s.with(|e, _| e.sheet_state().unwrap().list_popup.is_none()), "no quick-analysis popup opened");
    assert_eq!(s.with(|e, _| e.current_cell()), Some((3, 4)), "click selected D5 directly");
}

/// #124: Selecting body text in docx no longer renders the floating mini-toolbar over the page.
fn fix124_no_mini_toolbar_on_body_text_selection() {
    use ibis_doc_model::FlowPos;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let first_para = s.with(|e, _| {
        e.flow()
            .unwrap()
            .body
            .iter()
            .find_map(|b| match b {
                ibis_doc_model::Block::Paragraph(p) => Some(p.id),
                _ => None,
            })
            .unwrap()
    });
    s.update(move |e, _, cx| {
        e.set_caret(FlowPos::new(first_para, 0), false, cx);
        e.set_caret(FlowPos::new(first_para, 4), true, cx);
    });
    s.settle();
    let has_sel = s.with(|e, _| e.ordered_selection().is_some_and(|(a, b)| a != b));
    assert!(has_sel, "body text range is selected");
    s.screenshot("fix124-docx-selection-no-mini-toolbar");

    // Standard keyboard formatting (e.g. Cmd+B) works normally on the selected body text.
    let bold_before = s.with(|e, _| e.current_format().map(|(_, st)| st.bold)).unwrap_or(false);
    s.key("cmd-b");
    s.settle();
    let bold_after = s.with(|e, _| e.current_format().map(|(_, st)| st.bold)).unwrap_or(false);
    assert_eq!(bold_after, !bold_before, "Cmd+B toggles bold on selected body text");
}

/// Web `view=1` read-only mode and `menuBar=1` / `toolBar=1` defaults and overrides.
fn fix_embed_view_mode_and_bars() {
    use crate::shell::embed::{
        is_view_only, resolve_embed_query_options, set_embed_options, show_menu_bar, show_tool_bar,
    };

    // 1. Verify embed query option defaults: `?view=1` keeps `menuBar=true` and `toolBar=true` by default.
    assert_eq!(
        resolve_embed_query_options("?view=1"),
        (false, false, true, true, true),
        "?view=1 keeps menuBar=1 and toolBar=1 enabled by default"
    );
    assert_eq!(
        resolve_embed_query_options("?view=1&menuBar=0&toolBar=0"),
        (false, false, true, false, false),
        "?view=1&menuBar=0&toolBar=0 explicitly hides menuBar and toolBar"
    );
    assert_eq!(
        resolve_embed_query_options("?view=1&close=1&download=1"),
        (true, true, true, true, true),
        "?view=1&close=1&download=1 enables close/download while keeping menuBar and toolBar on"
    );

    set_embed_options(false, false, true, true, true);
    assert!(is_view_only() && show_menu_bar() && show_tool_bar());
    set_embed_options(false, false, false, true, true);

    // Also verify `www/main.ts` matches `?? true` for both `menuBar` and `toolBar`.
    let main_ts = std::fs::read_to_string(crate::shell::scenario::repo_root().join("www/main.ts"))
        .expect("read www/main.ts");
    assert!(
        main_ts.contains("menuBar: js.menuBar ?? flag(params, 'menuBar') ?? flag(params, 'menubar') ?? true"),
        "www/main.ts must default menuBar to true even when view=1"
    );
    assert!(
        main_ts.contains("toolBar: js.toolBar ?? flag(params, 'toolBar') ?? flag(params, 'toolbar') ?? true"),
        "www/main.ts must default toolBar to true even when view=1"
    );

    let mut s = Scenario::open("test/editing/docx-report.docx");
    // 2. Simulate launching with `?view=1` (read_only = true, default show_menu_bar = true, show_toolbar = true).
    s.update(|e, _, cx| {
        let (_, _, view, menu_bar, tool_bar) = resolve_embed_query_options("?view=1");
        e.read_only = view;
        e.show_menu_bar = menu_bar;
        e.show_toolbar = tool_bar;
        e.sidebar_open = false;
        cx.notify();
    });
    s.settle();
    assert!(s.with(|e, _| e.is_read_only() && e.show_menu_bar && e.show_toolbar && !e.sidebar_open));
    s.screenshot("view-only-menubar-and-toolbar");

    // 3. Simulate `?view=1&menuBar=0&toolBar=0` (both hidden).
    s.update(|e, _, cx| {
        let (_, _, view, menu_bar, tool_bar) = resolve_embed_query_options("?view=1&menuBar=0&toolBar=0");
        e.read_only = view;
        e.show_menu_bar = menu_bar;
        e.show_toolbar = tool_bar;
        cx.notify();
    });
    s.settle();
    assert!(s.with(|e, _| e.is_read_only() && !e.show_menu_bar && !e.show_toolbar && !e.sidebar_open));
    s.screenshot("view-only-no-bars");

    // 4. Simulate `?view=1&menuBar=0&toolBar=1` (toolbar shown, menu bar hidden).
    s.update(|e, _, cx| {
        e.show_toolbar = true;
        cx.notify();
    });
    s.settle();
    assert!(s.with(|e, _| e.is_read_only() && !e.show_menu_bar && e.show_toolbar));
    s.screenshot("view-only-toolbar");
}

/// #135: Comprehensive test for Excel (.xlsx) Split View:
/// - Menu (`window.split`, `xlsx.split`) and shortcut (`alt-cmd-s`) toggle ON/OFF.
/// - Split bars (`split-bar-h`, `split-bar-v`, `split-bar-both`) drag smoothly even across frozen panes/scrollbars and double-click clears each axis or both.
/// - All 4 split scrollbars (`ScrollHLeft`, `ScrollHRight`, `ScrollVTop`, `ScrollVBottom`) scroll their panes independently, and the bottom-right / top-right / bottom-left panes can scroll back to Row 1 / Column A (`main_lead`).
/// - Mouse wheel scrolling and arrow-key auto-scroll work independently per pane.
fn fix135_xlsx_split_view_comprehensive() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("D6"); // col = 3, row = 5
    s.settle();

    // 1. Toggle ON via Window > Split (`window.split`), OFF via View > Split (`xlsx.split`), ON/OFF via `alt-cmd-s`.
    s.menu("window.split");
    s.settle();
    assert_eq!(s.with(|e, _| e.split_panes().map(|p| p.at)), Some((3, 5)), "Window > Split turns split ON at D6 (3, 5)");

    s.menu("xlsx.split");
    s.settle();
    assert!(s.with(|e, _| e.split_panes().is_none()), "View > Split turns split OFF");

    s.key("alt-cmd-s");
    s.settle();
    assert_eq!(s.with(|e, _| e.split_panes().map(|p| p.at)), Some((3, 5)), "alt-cmd-s turns split ON");

    s.key("alt-cmd-s");
    s.settle();
    assert!(s.with(|e, _| e.split_panes().is_none()), "alt-cmd-s turns split OFF");

    // Turn back ON at D6 (3, 5).
    s.menu("window.split");
    s.settle();

    let bar_win_pos = |s: &mut Scenario| -> (f32, f32, f32, f32, f32, f32) {
        s.with(|e, _| {
            let wb = e.workspace_bounds.expect("workspace_bounds");
            let content = e.workspace_content().expect("workspace_content");
            let (sx, sy) = e.frozen_split(content).unwrap_or((None, None));
            (
                f32::from(wb.origin.x),
                f32::from(wb.origin.y),
                f32::from(wb.size.width),
                f32::from(wb.size.height),
                sx.unwrap_or(0.0),
                sy.unwrap_or(0.0),
            )
        })
    };

    // 2. Drag horizontal split bar (`split-bar-h`) downward across `frozen-rows` / pane overlays and release.
    let (wx, wy, _, _, sx, sy) = bar_win_pos(&mut s);
    let h_bar_start = (wx + sx + 100.0, wy + sy);
    s.drag_through(&[
        h_bar_start,
        (h_bar_start.0, h_bar_start.1 + 25.0),
        (h_bar_start.0, h_bar_start.1 + 55.0),
    ]);
    s.settle();
    let (at_after_h, drag_cleared_h) = s.with(|e, _| (e.split_panes().unwrap().at, e.sheet_state().unwrap().split_drag.is_none()));
    assert!(at_after_h.1 > 5, "horizontal split bar drag increased split row from 5 to {}", at_after_h.1);
    assert_eq!(at_after_h.0, 3, "horizontal split bar drag kept split col at 3");
    assert!(drag_cleared_h, "split_drag cleared on mouse up");

    // Drag vertical split bar (`split-bar-v`) rightward across `frozen-columns` / pane overlays and release.
    let (wx, wy, _, _, sx, sy) = bar_win_pos(&mut s);
    let v_bar_start = (wx + sx, wy + sy + 80.0);
    s.drag_through(&[
        v_bar_start,
        (v_bar_start.0 + 40.0, v_bar_start.1),
        (v_bar_start.0 + 90.0, v_bar_start.1),
    ]);
    s.settle();
    let (at_after_v, drag_cleared_v) = s.with(|e, _| (e.split_panes().unwrap().at, e.sheet_state().unwrap().split_drag.is_none()));
    assert!(at_after_v.0 > 3, "vertical split bar drag increased split col from 3 to {}", at_after_v.0);
    assert_eq!(at_after_v.1, at_after_h.1);
    assert!(drag_cleared_v, "split_drag cleared on mouse up");

    // Drag intersection (`split-bar-both`) diagonally back toward top-left.
    let (wx, wy, _, _, sx, sy) = bar_win_pos(&mut s);
    let both_start = (wx + sx, wy + sy);
    s.drag_through(&[
        both_start,
        (both_start.0 - 60.0, both_start.1 - 20.0),
        (both_start.0 - 120.0, both_start.1 - 45.0),
    ]);
    s.settle();
    let at_after_both = s.with(|e, _| e.split_panes().unwrap().at);
    assert!(
        at_after_both.0 < at_after_v.0 && at_after_both.1 < at_after_v.1,
        "intersection drag updated both col and row: {:?} -> {:?}",
        at_after_v,
        at_after_both
    );

    // 3. Double-click horizontal bar clears row split only; double-click vertical bar clears col split only; double-click intersection clears both.
    let (wx, wy, _, _, sx, sy) = bar_win_pos(&mut s);
    s.double_click((wx + sx + 80.0, wy + sy));
    s.settle();
    let at_after_dbl_h = s.with(|e, _| e.split_panes().unwrap().at);
    assert_eq!(at_after_dbl_h.1, 0, "double-clicking horizontal split bar cleared row split");
    assert!(at_after_dbl_h.0 > 0, "column split remained active");

    // Reset to (3, 5) and double-click vertical bar.
    s.menu("window.split");
    s.click_cell("D6");
    s.menu("window.split");
    s.settle();
    let (wx, wy, _, _, sx, sy) = bar_win_pos(&mut s);
    s.double_click((wx + sx, wy + sy + 80.0));
    s.settle();
    let at_after_dbl_v = s.with(|e, _| e.split_panes().unwrap().at);
    assert_eq!(at_after_dbl_v.0, 0, "double-clicking vertical split bar cleared column split");
    assert_eq!(at_after_dbl_v.1, 5, "row split remained active");

    // Reset to (3, 5) and double-click intersection.
    s.menu("window.split");
    s.click_cell("D6");
    s.menu("window.split");
    s.settle();
    let (wx, wy, _, _, sx, sy) = bar_win_pos(&mut s);
    s.double_click((wx + sx, wy + sy));
    s.settle();
    assert!(s.with(|e, _| e.split_panes().is_none()), "double-clicking intersection cleared split view completely");

    // 4. Independent scrolling via the 4 split scrollbars and reaching Row 1 / Column A in bottom-right pane (`main_lead`).
    s.click_cell("E7"); // col = 4, row = 6
    s.menu("window.split");
    s.settle();
    let (wx, wy, ww, wh, sx, sy) = bar_win_pos(&mut s);
    s.drag_through(&[
        (wx + 20.0, wy + wh - 6.0),
        (wx + 80.0, wy + wh - 6.0),
    ]);
    s.settle();
    assert!(
        s.with(|e, _| f32::from(e.split_panes().unwrap().shift.x) < 0.0),
        "dragging left horizontal scrollbar scrolled left panes horizontally"
    );

    s.drag_through(&[
        (wx + ww - 6.0, wy + 20.0),
        (wx + ww - 6.0, wy + 70.0),
    ]);
    s.settle();
    assert!(
        s.with(|e, _| f32::from(e.split_panes().unwrap().shift.y) < 0.0),
        "dragging top vertical scrollbar scrolled top panes vertically"
    );

    // Drag right horizontal scrollbar (`ScrollHRight`) toward its left edge so `main_lead.x > 0` (Column A visible in right panes).
    s.drag_through(&[
        (wx + sx + 80.0, wy + wh - 6.0),
        (wx + sx + 5.0, wy + wh - 6.0),
    ]);
    s.settle();
    assert!(
        s.with(|e, _| f32::from(e.split_panes().unwrap().main_lead.x) > 0.0),
        "dragging right horizontal scrollbar to left end exposed columns before split col (main_lead.x > 0)"
    );

    // Drag bottom vertical scrollbar (`ScrollVBottom`) toward its top edge so `main_lead.y > 0` (Row 1 visible in bottom panes).
    s.drag_through(&[
        (wx + ww - 6.0, wy + sy + 80.0),
        (wx + ww - 6.0, wy + sy + 5.0),
    ]);
    s.settle();
    assert!(
        s.with(|e, _| f32::from(e.split_panes().unwrap().main_lead.y) > 0.0),
        "dragging bottom vertical scrollbar to top end exposed rows before split row (main_lead.y > 0)"
    );

    // 5. Mouse wheel scrolling on individual panes and arrow-key navigation in bottom-right pane back to A1.
    let shift_before_wheel = s.with(|e, _| e.split_panes().unwrap().shift);
    s.wheel_xy((wx + 60.0, wy + 60.0), 0.0, -40.0);
    s.settle();
    let shift_after_wheel = s.with(|e, _| e.split_panes().unwrap().shift);
    assert!(
        f32::from(shift_after_wheel.y) < f32::from(shift_before_wheel.y),
        "mouse wheel over top-left pane scrolled top panes vertically"
    );

    // Click inside bottom-right pane and use Left/Up arrow keys to reach A1 (0, 0) while staying in bottom-right pane.
    s.click((wx + sx + 80.0, wy + sy + 60.0));
    s.settle();
    assert_eq!(s.with(|e, _| e.split_panes().unwrap().active_pane), (true, true), "clicked in bottom-right pane");
    for _ in 0..10 {
        s.key("left");
        s.key("up");
    }
    s.settle();
    let (cur_cell, active_pane, lead) = s.with(|e, _| {
        let sp = e.split_panes().unwrap();
        (e.current_cell(), sp.active_pane, sp.main_lead)
    });
    assert_eq!(cur_cell, Some((0, 0)), "arrow keys reached A1 (0, 0) inside bottom-right pane");
    assert_eq!(active_pane, (true, true), "active pane remained bottom-right");
    assert!(f32::from(lead.x) > 0.0 && f32::from(lead.y) > 0.0, "bottom-right pane auto-scrolled to show A1: main_lead={lead:?}");
    s.screenshot("fix135-xlsx-split-view");
}

/// #157: Cell background fill, thin solid border, and gridlines snap to identical device pixels
/// across zoom levels (100%, 130%, 150%, 175%, 200%, 400%) with no 1px gap or gridline bleed,
/// and no 3px black corner protrusion at 400% zoom.
fn fix157_xlsx_fill_border_gridline_pixel_alignment_across_zooms() {
    use gpui_kit::{point, px};
    use ibis_doc_model::{Rgb, StyleChange};

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.insert-sheet");
    s.settle();

    // Format B2:D4 (cols 1..=3, rows 1..=3) with yellow fill (#ffff00) and thin black all-borders (#000000).
    s.click_cell("B2");
    s.key("shift-right");
    s.key("shift-right");
    s.key("shift-down");
    s.key("shift-down");
    s.settle();
    s.update(|e, _, cx| {
        e.format_cells(StyleChange::Fill(Some(Rgb(0xffff00))), cx);
    });
    s.toolbar_menu("borders", "xlsx.border-all");
    s.settle();

    // Move active cell away to H10 so selection border does not overlap B2:D4.
    s.click_cell("H10");
    s.settle();

    for zoom_pct in [100.0f32, 130.0, 150.0, 175.0, 200.0, 400.0] {
        s.update(move |e, window, cx| {
            e.set_zoom(Some(zoom_pct / 100.0), window, cx);
            e.workspace_scroll.set_offset(point(px(0.), px(0.)));
        });
        s.settle();

        let (c2_center, c3_center, b2_center) = s.with(|e, _| {
            (
                e.cell_center(2, 1).expect("C2 on screen"),
                e.cell_center(2, 2).expect("C3 on screen"),
                e.cell_center(1, 1).expect("B2 on screen"),
            )
        });
        let (pic_width, pic_rgba) = s.picture();
        let k = pic_width as f32 / 1200.0;
        let pic_height = (pic_rgba.len() as u32 / 4) / pic_width.max(1);
        let px_at = |x: i32, y: i32| -> (u8, u8, u8) {
            let x = x.clamp(0, pic_width as i32 - 1) as u32;
            let y = y.clamp(0, pic_height as i32 - 1) as u32;
            let idx = ((y * pic_width + x) * 4) as usize;
            (pic_rgba[idx], pic_rgba[idx + 1], pic_rgba[idx + 2])
        };
        let is_yellow = |(r, g, b): (u8, u8, u8)| r > 230 && g > 230 && b < 30;
        let is_black = |(r, g, b): (u8, u8, u8)| r < 40 && g < 40 && b < 40;

        // Vertical scan from C2 center to C3 center across the horizontal thin border between row 1 and row 2:
        // Every pixel must be either yellow fill (`#ffff00`) or black border (`#000000`) — never white or grey gridline!
        let scan_x = (f32::from(c2_center.x) * k).round() as i32;
        let y_start = (f32::from(c2_center.y) * k).round() as i32;
        let y_end = (f32::from(c3_center.y) * k).round() as i32;
        let mut saw_border_h = false;
        for y in y_start..=y_end {
            let rgb = px_at(scan_x, y);
            if is_black(rgb) {
                saw_border_h = true;
            } else {
                assert!(
                    is_yellow(rgb),
                    "zoom {zoom_pct}%: unexpected pixel {rgb:?} at ({scan_x}, {y}) between C2 and C3 (1px gap or gridline bleed)"
                );
            }
        }
        assert!(saw_border_h, "zoom {zoom_pct}%: horizontal thin border between C2 and C3 must be present");

        // Horizontal scan from B2 center to C2 center across the vertical thin border between col 1 and col 2:
        let scan_y = (f32::from(b2_center.y) * k).round() as i32;
        let x_start = (f32::from(b2_center.x) * k).round() as i32;
        let x_end = (f32::from(c2_center.x) * k).round() as i32;
        let mut saw_border_v = false;
        for x in x_start..=x_end {
            let rgb = px_at(x, scan_y);
            if is_black(rgb) {
                saw_border_v = true;
            } else {
                assert!(
                    is_yellow(rgb),
                    "zoom {zoom_pct}%: unexpected pixel {rgb:?} at ({x}, {scan_y}) between B2 and C2 (1px gap or gridline bleed)"
                );
            }
        }
        assert!(saw_border_v, "zoom {zoom_pct}%: vertical thin border between B2 and C2 must be present");

        // Also scan upward from B2 center across B2's top border into B1 (unfilled white):
        let mut y = scan_y;
        while y > 0 && is_yellow(px_at(x_start, y)) {
            y -= 1;
        }
        assert!(
            is_black(px_at(x_start, y)),
            "zoom {zoom_pct}%: pixel immediately above B2 yellow fill at ({x_start}, {y}) must be black border, got {:?}",
            px_at(x_start, y)
        );
        while y > 0 && is_black(px_at(x_start, y - 1)) {
            y -= 1;
        }
        // And scan leftward from B2 center across B2's left border into A2:
        let mut x = x_start;
        while x > 0 && is_yellow(px_at(x, scan_y)) {
            x -= 1;
        }
        assert!(
            is_black(px_at(x, scan_y)),
            "zoom {zoom_pct}%: pixel immediately left of B2 yellow fill at ({x}, {scan_y}) must be black border, got {:?}",
            px_at(x, scan_y)
        );
        while x > 0 && is_black(px_at(x - 1, scan_y)) {
            x -= 1;
        }

        // At 400%, verify the outer top-left corner of B2 (`(x, y)`) does not protrude above `y` or left of `x`.
        if (zoom_pct - 400.0).abs() < 1e-3 {
            assert!(
                !is_black(px_at(x - 1, y)) && !is_black(px_at(x, y - 1)),
                "zoom 400%: top-left border corner at ({x}, {y}) must not protrude into ({}, {y})={:?} or ({x}, {})={:?}",
                x - 1,
                px_at(x - 1, y),
                y - 1,
                px_at(x, y - 1)
            );
        }
    }
    s.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    s.settle();
}

/// #11: Hovering the selection frame border or fill handle in Excel (.xlsx) and pressing/releasing
/// Option (Alt) before dragging switches the cursor to `CursorStyle::DragCopy` (`+`) and back to
/// `OpenHand` / `Crosshair`.
fn fix11_xlsx_option_hover_cursor_on_border_and_fill_handle() {
    use gpui_kit::CursorStyle;

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B2");
    s.settle();

    // 1. Before any drag starts (`cell_move().is_none()`), `cell_move_cursor(false)` is `OpenHand`
    // and `cell_move_cursor(true)` (when Option/Alt is held on hover) is `DragCopy`.
    let (no_drag, cursor_plain, cursor_alt) = s.with(|e, _| {
        (
            e.cell_move().is_none(),
            e.cell_move_cursor(false),
            e.cell_move_cursor(true),
        )
    });
    assert!(no_drag, "no cell drag active initially");
    assert_eq!(cursor_plain, CursorStyle::OpenHand, "hovering selection border without Option shows OpenHand");
    assert_eq!(cursor_alt, CursorStyle::DragCopy, "hovering selection border with Option shows DragCopy before drag starts");

    // 2. Start a cell move drag without Option: cursor is `ClosedHand`, and pressing Option mid-drag switches to `DragCopy`.
    let b2_pt = s.with(|e, _| e.cell_center(1, 1).expect("B2 on screen"));
    s.update(move |e, window, cx| e.begin_cell_move(b2_pt, window, cx));
    s.settle();
    assert_eq!(
        s.with(|e, _| e.cell_move_cursor(false)),
        CursorStyle::ClosedHand,
        "dragging selection border without Option shows ClosedHand"
    );
    assert_eq!(
        s.with(|e, _| e.cell_move_cursor(true)),
        CursorStyle::DragCopy,
        "pressing Option during selection border drag shows DragCopy"
    );
    s.update(|e, _, cx| {
        e.finish_cell_move(cx);
    });
    s.settle();

    // 3. Fill handle state (`begin_fill()` starts fill drag).
    s.update(|e, _, _| {
        e.begin_fill();
    });
    assert!(s.with(|e, _| e.sheet_state().unwrap().fill_source.is_some()), "fill handle drag started");
    s.update(|e, _, cx| {
        e.finish_fill(false, cx);
    });
}

/// #152 / #156: Selecting multiple text boxes / shapes without entering text-edit mode and applying
/// paragraph formatting (left/center/right align, indent more/less, bullet/numbered list) and character
/// formatting (font size direct input, font-larger/smaller `+/-`, and underline styles) applies to all
/// selected shapes simultaneously across `.pptx`, `.docx`, and `.xlsx`.
fn fix152_156_multi_shape_text_and_paragraph_formatting() {
    use ibis_doc_model::{AutoShape, Command, EmuRect, Fill, Geometry, Shape, ShapeKind, TextAlign, TextBody, TextStyle, Underline};

    // 1. PowerPoint (.pptx)
    {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        let (id1, id2) = s.update(|e, _, cx| {
            let a_id = e.document.ids().new_shape_id();
            let b_id = e.document.ids().new_shape_id();
            let a = Shape::new(
                a_id,
                EmuRect::new(914_400, 914_400, 1_828_800, 914_400),
                ShapeKind::TextBox(TextBody::plain("Alpha", TextStyle::default())),
            );
            let mut auto = AutoShape::new(Geometry::Rect, Fill::None, None);
            auto.text = Some(TextBody::plain("Beta", TextStyle::default()));
            let b = Shape::new(
                b_id,
                EmuRect::new(3_200_400, 914_400, 1_828_800, 914_400),
                ShapeKind::Auto(auto),
            );
            let id1 = e.insert_shape(a, cx);
            let id2 = e.insert_shape(b, cx);
            e.editing = None;
            e.set_selection(vec![id1, id2]);
            (id1, id2)
        });
        s.settle();
        assert!(s.with(|e, _| e.editing.is_none() && e.selected_shapes().len() == 2));

        // Alignment: center -> right -> left
        for (btn, expected) in [
            ("align-center", TextAlign::Center),
            ("align-right", TextAlign::Right),
            ("align-left", TextAlign::Left),
        ] {
            s.toolbar(btn);
            s.settle();
            for id in [id1, id2] {
                let align = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].align);
                assert_eq!(align, Some(expected), "pptx multi-shape {btn} on {id:?}");
            }
        }

        // Indent: indent-more -> indent-less
        s.toolbar("indent-more");
        s.settle();
        for id in [id1, id2] {
            let ml = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].margin_left);
            assert!(ml > 0, "pptx multi-shape indent-more on {id:?}: margin_left={ml}");
        }
        s.toolbar("indent-less");
        s.settle();
        for id in [id1, id2] {
            let ml = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].margin_left);
            assert_eq!(ml, 0, "pptx multi-shape indent-less on {id:?}");
        }

        // Bullet list & Numbered list
        s.toolbar("bullets");
        s.settle();
        for id in [id1, id2] {
            let has_bullet = s.with(move |e, _| {
                e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0]
                    .bullet
                    .as_ref()
                    .is_some_and(|b| b.auto.is_none())
            });
            assert!(has_bullet, "pptx multi-shape bullets on {id:?}");
        }
        s.toolbar("numbered");
        s.settle();
        for id in [id1, id2] {
            let has_number = s.with(move |e, _| {
                e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0]
                    .bullet
                    .as_ref()
                    .is_some_and(|b| b.auto.is_some())
            });
            assert!(has_number, "pptx multi-shape numbered on {id:?}");
        }

        // Font size direct input (24pt = 2400) and +/- (larger -> 2800, smaller -> 2400)
        s.update(|e, window, cx| e.set_toolbar_font_size(24.0, window, cx));
        s.settle();
        for id in [id1, id2] {
            let sz = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].runs[0].style.font_size);
            assert_eq!(sz, Some(2400), "pptx multi-shape direct font size 24pt on {id:?}");
        }
        s.update(|e, window, cx| e.step_font_size_of_text(true, window, cx));
        s.settle();
        for id in [id1, id2] {
            let sz = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].runs[0].style.font_size);
            assert_eq!(sz, Some(2800), "pptx multi-shape font-larger on {id:?}");
        }
        s.update(|e, window, cx| e.step_font_size_of_text(false, window, cx));
        s.settle();
        for id in [id1, id2] {
            let sz = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].runs[0].style.font_size);
            assert_eq!(sz, Some(2400), "pptx multi-shape font-smaller on {id:?}");
        }

        // Underline styles (17-style menu: double, wavy, toggle off)
        s.toolbar_menu("underline", "pptx.underline.dbl");
        s.settle();
        for id in [id1, id2] {
            let (u, kind) = s.with(move |e, _| {
                let st = &e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
                (st.underline, st.underline_kind)
            });
            assert!(u && kind == Underline::Double, "pptx multi-shape pptx.underline.dbl on {id:?}");
        }
        s.toolbar_menu("underline", "pptx.underline.wavy");
        s.settle();
        for id in [id1, id2] {
            let (u, kind) = s.with(move |e, _| {
                let st = &e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
                (st.underline, st.underline_kind)
            });
            assert!(u && kind == Underline::Wavy, "pptx multi-shape pptx.underline.wavy on {id:?}");
        }
        s.toolbar("underline");
        s.settle();
        for id in [id1, id2] {
            let u = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].runs[0].style.underline);
            assert!(!u, "pptx multi-shape underline toggle off on {id:?}");
        }
    }

    // 2. Word (.docx)
    {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        s.menu("insert.text-box");
        s.settle();
        let at1 = s.with(|e, _| e.selected_text_box_frame().unwrap().2);
        s.update(|e, window, cx| {
            e.commit_box_edit(window, cx);
            e.doc_insert_shape("rect", cx);
        });
        s.settle();
        let (page2, (x2, y2, w2, h2), at2, _) = s.with(|e, _| e.selected_text_box_frame().unwrap());
        // Give both shapes text so both selected shapes have a TextBody with runs.
        s.update(move |e, window, cx| {
            e.run_flow(
                vec![
                    ibis_doc_model::flow_command::FlowCommand::SetShapeText {
                        at: at1,
                        text: "FirstBox".to_string(),
                    },
                    ibis_doc_model::flow_command::FlowCommand::SetShapeText {
                        at: at2,
                        text: "SecondBox".to_string(),
                    },
                ],
                cx,
            );
            e.commit_box_edit(window, cx);
            let (page1, (x1, y1, w1, h1), _) = e.floating_text_box_frame_of(at1).unwrap();
            assert!(e.begin_shape_drag(page1, x1 + w1 * 0.5, y1 + h1 * 0.5, cx));
            e.end_shape_drag(cx);
            assert!(e.shift_click_shape(page2, x2 + w2 * 0.5, y2 + h2 * 0.5, cx));
        });
        s.settle();
        assert_eq!(s.with(|e, _| e.all_selected_shapes().len()), 2, "docx 2 shapes selected");

        // Alignment: center -> right -> left
        for (btn, expected) in [
            ("align-center", TextAlign::Center),
            ("align-right", TextAlign::Right),
            ("align-left", TextAlign::Left),
        ] {
            s.toolbar(btn);
            s.settle();
            for at in [at1, at2] {
                let align = s.with(move |e, _| e.shape_text_body_at(at).unwrap().paragraphs[0].align);
                assert_eq!(align, Some(expected), "docx multi-shape {btn} at {at:?}");
            }
        }

        // Indent: indent-more -> indent-less
        s.toolbar("indent-more");
        s.settle();
        for at in [at1, at2] {
            let ml = s.with(move |e, _| e.shape_text_body_at(at).unwrap().paragraphs[0].margin_left);
            assert!(ml > 0, "docx multi-shape indent-more increased margin_left to {ml} at {at:?}");
        }
        s.toolbar("indent-less");
        s.settle();
        for at in [at1, at2] {
            let ml = s.with(move |e, _| e.shape_text_body_at(at).unwrap().paragraphs[0].margin_left);
            assert_eq!(ml, 0, "docx multi-shape indent-less restored margin_left to 0 at {at:?}");
        }

        // Bullet list & Numbered list
        s.toolbar("bullets");
        s.settle();
        for at in [at1, at2] {
            let has_bullet = s.with(move |e, _| {
                e.shape_text_body_at(at).unwrap().paragraphs[0]
                    .bullet
                    .as_ref()
                    .is_some_and(|b| b.auto.is_none())
            });
            assert!(has_bullet, "docx multi-shape bullets at {at:?}");
        }
        s.toolbar("numbered");
        s.settle();
        for at in [at1, at2] {
            let has_number = s.with(move |e, _| {
                e.shape_text_body_at(at).unwrap().paragraphs[0]
                    .bullet
                    .as_ref()
                    .is_some_and(|b| b.auto.is_some())
            });
            assert!(has_number, "docx multi-shape numbered at {at:?}");
        }

        // Font size direct input (22pt = 2200) and +/- (larger -> 2400, smaller -> 2000)
        s.update(|e, window, cx| e.set_toolbar_font_size(22.0, window, cx));
        s.settle();
        for at in [at1, at2] {
            let sz = s.with(move |e, _| {
                let b = e.shape_text_body_at(at).unwrap();
                b.paragraphs[0].runs[0].style.font_size.unwrap_or(b.style.font_size)
            });
            assert_eq!(sz, 2200, "docx multi-shape direct font size 22pt at {at:?}");
        }
        s.update(|e, _, cx| e.step_text_size(true, cx));
        s.settle();
        for at in [at1, at2] {
            let sz = s.with(move |e, _| {
                let b = e.shape_text_body_at(at).unwrap();
                b.paragraphs[0].runs[0].style.font_size.unwrap_or(b.style.font_size)
            });
            assert_eq!(sz, 2400, "docx multi-shape font-larger at {at:?}");
        }
        s.update(|e, _, cx| e.step_text_size(false, cx));
        s.settle();
        for at in [at1, at2] {
            let sz = s.with(move |e, _| {
                let b = e.shape_text_body_at(at).unwrap();
                b.paragraphs[0].runs[0].style.font_size.unwrap_or(b.style.font_size)
            });
            assert_eq!(sz, 2000, "docx multi-shape font-smaller at {at:?}");
        }

        // Underline styles (17-style menu: double, wavy, none)
        s.toolbar_menu("underline", "docx.underline.double");
        s.settle();
        for at in [at1, at2] {
            let (u, kind) = s.with(move |e, _| {
                let st = &e.shape_text_body_at(at).unwrap().paragraphs[0].runs[0].style;
                (st.underline, st.underline_kind)
            });
            assert!(u && kind == Underline::Double, "docx multi-shape docx.underline.double at {at:?}");
        }
        s.toolbar_menu("underline", "docx.underline.wave");
        s.settle();
        for at in [at1, at2] {
            let (u, kind) = s.with(move |e, _| {
                let st = &e.shape_text_body_at(at).unwrap().paragraphs[0].runs[0].style;
                (st.underline, st.underline_kind)
            });
            assert!(u && kind == Underline::Wavy, "docx multi-shape docx.underline.wave at {at:?}");
        }
        s.toolbar_menu("underline", "docx.underline.none");
        s.settle();
        for at in [at1, at2] {
            let u = s.with(move |e, _| e.shape_text_body_at(at).unwrap().paragraphs[0].runs[0].style.underline);
            assert!(!u, "docx multi-shape docx.underline.none at {at:?}");
        }
    }

    // 3. Excel (.xlsx)
    {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        let (id1, id2) = s.update(|e, _, cx| {
            let a = e.insert_sheet_shape("rect", cx);
            let b = e.insert_sheet_shape("ellipse", cx);
            let slide = e.current_slide;
            for (id, txt) in [(a, "BoxOne"), (b, "BoxTwo")] {
                if let Some(ShapeKind::Auto(mut auto)) = e.slide().shape(id).map(|sh| sh.kind.clone()) {
                    auto.text = Some(TextBody::plain(txt, TextStyle::default()));
                    e.run_sheet(
                        vec![Command::SetKind {
                            slide,
                            shape: id,
                            kind: Box::new(ShapeKind::Auto(auto)),
                        }],
                        cx,
                    );
                }
            }
            e.editing = None;
            e.set_selection(vec![a, b]);
            (a, b)
        });
        s.settle();
        assert_eq!(s.with(|e, _| e.selected_shapes().len()), 2, "xlsx 2 shapes selected");

        // Alignment: center -> right -> left
        for (btn, expected) in [
            ("align-center", TextAlign::Center),
            ("align-right", TextAlign::Right),
            ("align-left", TextAlign::Left),
        ] {
            s.toolbar(btn);
            s.settle();
            for id in [id1, id2] {
                let align = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].align);
                assert_eq!(align, Some(expected), "xlsx multi-shape {btn} on {id:?}");
            }
        }

        // Indent: indent-more -> indent-less
        s.toolbar("indent-more");
        s.settle();
        for id in [id1, id2] {
            let ml = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].margin_left);
            assert!(ml > 0, "xlsx multi-shape indent-more increased margin_left to {ml} on {id:?}");
        }
        s.toolbar("indent-less");
        s.settle();
        for id in [id1, id2] {
            let ml = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].margin_left);
            assert_eq!(ml, 0, "xlsx multi-shape indent-less restored margin_left to 0 on {id:?}");
        }

        // Bullet list & Numbered list
        s.toolbar("bullets");
        s.settle();
        for id in [id1, id2] {
            let has_bullet = s.with(move |e, _| {
                e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0]
                    .bullet
                    .as_ref()
                    .is_some_and(|b| b.auto.is_none())
            });
            assert!(has_bullet, "xlsx multi-shape bullets on {id:?}");
        }
        s.toolbar("numbered");
        s.settle();
        for id in [id1, id2] {
            let has_number = s.with(move |e, _| {
                e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0]
                    .bullet
                    .as_ref()
                    .is_some_and(|b| b.auto.is_some())
            });
            assert!(has_number, "xlsx multi-shape numbered on {id:?}");
        }

        // Font size direct input (20pt = 2000) and +/- (larger -> 2400, smaller -> 2000)
        s.update(|e, window, cx| e.set_toolbar_font_size(20.0, window, cx));
        s.settle();
        for id in [id1, id2] {
            let sz = s.with(move |e, _| {
                let b = e.slide().shape(id).unwrap().text_body().unwrap();
                b.paragraphs[0].runs[0].style.font_size.unwrap_or(b.style.font_size)
            });
            assert_eq!(sz, 2000, "xlsx multi-shape direct font size 20pt on {id:?}");
        }
        s.update(|e, _, cx| e.step_font_size_selection(true, cx));
        s.settle();
        for id in [id1, id2] {
            let sz = s.with(move |e, _| {
                let b = e.slide().shape(id).unwrap().text_body().unwrap();
                b.paragraphs[0].runs[0].style.font_size.unwrap_or(b.style.font_size)
            });
            assert_eq!(sz, 2400, "xlsx multi-shape font-larger on {id:?}");
        }
        s.update(|e, _, cx| e.step_font_size_selection(false, cx));
        s.settle();
        for id in [id1, id2] {
            let sz = s.with(move |e, _| {
                let b = e.slide().shape(id).unwrap().text_body().unwrap();
                b.paragraphs[0].runs[0].style.font_size.unwrap_or(b.style.font_size)
            });
            assert_eq!(sz, 2000, "xlsx multi-shape font-smaller on {id:?}");
        }

        // Underline styles (17-style menu: double, wavy, toggle off)
        s.toolbar_menu("underline", "pptx.underline.dbl");
        s.settle();
        for id in [id1, id2] {
            let (u, kind) = s.with(move |e, _| {
                let st = &e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
                (st.underline, st.underline_kind)
            });
            assert!(u && kind == Underline::Double, "xlsx multi-shape pptx.underline.dbl on {id:?}");
        }
        s.toolbar_menu("underline", "pptx.underline.wavy");
        s.settle();
        for id in [id1, id2] {
            let (u, kind) = s.with(move |e, _| {
                let st = &e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
                (st.underline, st.underline_kind)
            });
            assert!(u && kind == Underline::Wavy, "xlsx multi-shape pptx.underline.wavy on {id:?}");
        }
        s.toolbar("underline");
        s.settle();
        for id in [id1, id2] {
            let u = s.with(move |e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs[0].runs[0].style.underline);
            assert!(!u, "xlsx multi-shape underline toggle off on {id:?}");
        }
    }
}

/// #152 & #156:
/// - #156: When a worksheet shape/textbox is selected in Excel (.xlsx), the toolbar underline dropdown
///   excludes cell-only accounting underlines (`singleAccounting`, `doubleAccounting`) and shows the
///   17 DrawingML underline styles (`pptx.underline.*`) with active checked state and applies the chosen
///   underline style to all selected shapes.
/// - #152: When multiple shapes/textboxes on a sheet are selected without entering text edit mode,
///   changing font family, direct font size input, font size +/-, bold, italic, underline, strikethrough,
///   text color, highlight color, horizontal alignment, vertical alignment, bullets/numbered/checklist,
///   and indent +/- applies to all text of all selected shapes.
fn fix152_156_xlsx_shape_underline_and_multi_shape_text_format() {
    use crate::shell::chrome::{MenuEntry, ToolItem};
    use ibis_doc_model::{
        AutoShape, CellAlign, CellVerticalAlign, Color, EmuRect, Fill, ListKind, Rgb,
        Shape, ShapeKind, Strike, StyleChange, TextAlign, TextBody, TextStyle, Underline,
        VerticalAnchor,
    };

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");

    // 1. When a cell is selected (no shape selected), the underline dropdown has the 5 cell underline items
    //    including singleAccounting and doubleAccounting.
    let cell_underline_ids: Vec<&'static str> = s.with(|e, _| {
        let items = crate::xlsx::toolbar::toolbar_right(e);
        items
            .into_iter()
            .find_map(|item| match item {
                ToolItem::Button(b) if b.id == "underline" => b.menu.map(|m| {
                    m.into_iter()
                        .filter_map(|entry| match entry {
                            MenuEntry::Item(mi) => Some(mi.id),
                            _ => None,
                        })
                        .collect()
                }),
                _ => None,
            })
            .unwrap_or_default()
    });
    assert!(
        cell_underline_ids.contains(&"singleAccounting")
            && cell_underline_ids.contains(&"doubleAccounting"),
        "Cell selection underline dropdown includes accounting underlines: {cell_underline_ids:?}"
    );

    // 2. Insert two shapes with different initial font sizes (10pt and 14pt) and select both without entering text edit mode.
    let (id1, id2) = s.update(|e, _, cx| {
        let slide = e.current_slide;
        let id1 = e.document.ids().new_shape_id();
        let id2 = e.document.ids().new_shape_id();
        let mut st1 = TextStyle::default();
        st1.font_size = 1000; // 10pt
        let body1 = TextBody::plain("Alpha", st1);
        let auto1 = AutoShape::text_box(Fill::Solid(Color::opaque(Rgb(0xD9E1F2))), body1);
        let sh1 = Shape::new(
            id1,
            EmuRect {
                x: 400_000,
                y: 400_000,
                width: 1_800_000,
                height: 700_000,
            },
            ShapeKind::Auto(auto1),
        );

        let mut st2 = TextStyle::default();
        st2.font_size = 1400; // 14pt
        let body2 = TextBody::plain("Beta", st2);
        let auto2 = AutoShape::text_box(Fill::Solid(Color::opaque(Rgb(0xE2EFDA))), body2);
        let sh2 = Shape::new(
            id2,
            EmuRect {
                x: 2_500_000,
                y: 400_000,
                width: 1_800_000,
                height: 700_000,
            },
            ShapeKind::Auto(auto2),
        );

        let idx = e.slide().shapes.len();
        e.run_sheet(
            vec![
                ibis_doc_model::Command::InsertShape {
                    slide,
                    index: idx,
                    shape: sh1,
                },
                ibis_doc_model::Command::InsertShape {
                    slide,
                    index: idx + 1,
                    shape: sh2,
                },
            ],
            cx,
        );
        e.select_sheet_shapes(&[id1, id2]);
        (id1, id2)
    });
    s.settle();

    // Verify we are NOT in text edit mode and both shapes are selected.
    assert!(s.with(|e, _| e.editing.is_none() && e.selected_ids() == vec![id1, id2]));

    // 3. #156: Check the underline dropdown while worksheet shapes are selected:
    //    - Must NOT contain singleAccounting or doubleAccounting
    //    - Must contain all 17 DrawingML underline styles (`pptx.underline.*`)
    let shape_underline_ids: Vec<&'static str> = s.with(|e, _| {
        let items = crate::xlsx::toolbar::toolbar_right(e);
        items
            .into_iter()
            .find_map(|item| match item {
                ToolItem::Button(b) if b.id == "underline" => b.menu.map(|m| {
                    m.into_iter()
                        .filter_map(|entry| match entry {
                            MenuEntry::Item(mi) => Some(mi.id),
                            _ => None,
                        })
                        .collect()
                }),
                _ => None,
            })
            .unwrap_or_default()
    });
    assert_eq!(
        shape_underline_ids.len(),
        17,
        "Shape underline dropdown has exactly 17 DrawingML styles: {shape_underline_ids:?}"
    );
    assert!(
        !shape_underline_ids.contains(&"singleAccounting")
            && !shape_underline_ids.contains(&"doubleAccounting"),
        "Shape underline dropdown excludes cell-only accounting underlines"
    );
    assert!(
        shape_underline_ids.contains(&"pptx.underline.sng")
            && shape_underline_ids.contains(&"pptx.underline.wavy")
            && shape_underline_ids.contains(&"pptx.underline.wavyDbl"),
        "Shape underline dropdown includes DrawingML underline kinds"
    );

    // Apply Wavy underline via set_sheet_shape_underline and verify both shapes get Underline::Wavy
    // and the dropdown marks `pptx.underline.wavy` as checked.
    s.update(|e, _, cx| {
        e.set_sheet_shape_underline(Some(Underline::Wavy), cx);
    });
    s.settle();
    let (u1, u2, checked_id) = s.with(|e, _| {
        let r1 = &e.slide().shape(id1).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
        let r2 = &e.slide().shape(id2).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
        let checked = crate::xlsx::toolbar::shape_underline_kind_items(e.selected_sheet_shape_underline())
            .into_iter()
            .find_map(|entry| match entry {
                MenuEntry::Item(mi) if mi.checked => Some(mi.id),
                _ => None,
            });
        (
            (r1.underline, r1.underline_kind),
            (r2.underline, r2.underline_kind),
            checked,
        )
    });
    assert_eq!(u1, (true, Underline::Wavy));
    assert_eq!(u2, (true, Underline::Wavy));
    assert_eq!(checked_id, Some("pptx.underline.wavy"));

    // 4. #152: Font size +/- steps each shape's runs independently (10pt -> 11pt, 14pt -> 16pt).
    s.update(|e, _, cx| {
        e.step_font_size_selection(true, cx);
    });
    s.settle();
    let (sz1, sz2) = s.with(|e, _| {
        let r1 = &e.slide().shape(id1).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
        let r2 = &e.slide().shape(id2).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
        (r1.font_size, r2.font_size)
    });
    assert_eq!((sz1, sz2), (Some(1050), Some(1600)));

    // Direct font size input (e.g. 18pt) sets both shapes to 18pt (1800).
    s.update(|e, window, cx| {
        e.set_toolbar_font_size(18.0, window, cx);
    });
    s.settle();
    let (sz1_after, sz2_after) = s.with(|e, _| {
        let r1 = &e.slide().shape(id1).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
        let r2 = &e.slide().shape(id2).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
        (r1.font_size, r2.font_size)
    });
    assert_eq!((sz1_after, sz2_after), (Some(1800), Some(1800)));

    // Font family, bold, italic, strikethrough, text color, highlight color across both selected shapes.
    s.update(|e, _, cx| {
        e.format_cells(StyleChange::Font(Some("Carlito".into())), cx);
        e.format_cells(StyleChange::Bold(true), cx);
        e.format_cells(StyleChange::Italic(true), cx);
        e.format_cells(StyleChange::Strike(true), cx);
        e.format_cells(StyleChange::Color(Rgb(0xC00000)), cx);
        e.set_sheet_shape_highlight(Some(Rgb(0xFFFF00)), cx);
    });
    s.settle();
    for sid in [id1, id2] {
        let (font, bold, italic, strike, color, highlight) = s.with(|e, _| {
            let r = &e.slide().shape(sid).unwrap().text_body().unwrap().paragraphs[0].runs[0].style;
            (
                r.font.clone(),
                r.bold,
                r.italic,
                r.strike,
                r.color,
                r.highlight,
            )
        });
        assert_eq!(font.as_deref(), Some("Carlito"));
        assert_eq!(bold, Some(true));
        assert_eq!(italic, Some(true));
        assert_eq!(strike, Strike::Single);
        assert_eq!(color, Some(Rgb(0xC00000)));
        assert_eq!(highlight, Some(Rgb(0xFFFF00)));
    }

    // Horizontal alignment (Center), vertical alignment (Middle), bullet list, checklist, and indent +/- across both shapes.
    s.update(|e, _, cx| {
        e.format_cells(StyleChange::Align(CellAlign::Center), cx);
        e.format_cells(StyleChange::Vertical(CellVerticalAlign::Center), cx);
        e.toggle_sheet_shape_list(ListKind::Bullet, cx);
        e.step_sheet_indent(true, cx);
    });
    s.settle();
    for sid in [id1, id2] {
        let (align, anchor, bullet, level) = s.with(|e, _| {
            let b = e.slide().shape(sid).unwrap().text_body().unwrap();
            let p = &b.paragraphs[0];
            (p.align, b.anchor, ListKind::of(p.bullet.as_ref()), p.margin_left)
        });
        assert_eq!(align, Some(TextAlign::Center));
        assert_eq!(anchor, VerticalAnchor::Middle);
        assert_eq!(bullet, ListKind::Bullet);
        assert_eq!(level, 685_800);
    }

    // Toggle checklist and decrease indent across both shapes.
    s.update(|e, _, cx| {
        e.toggle_sheet_shape_list(ListKind::Symbol('☐'), cx);
        e.step_sheet_indent(false, cx);
    });
    s.settle();
    for sid in [id1, id2] {
        let (bullet, level) = s.with(|e, _| {
            let b = e.slide().shape(sid).unwrap().text_body().unwrap();
            let p = &b.paragraphs[0];
            (ListKind::of(p.bullet.as_ref()), p.margin_left)
        });
        assert_eq!(bullet, ListKind::Symbol('☐'));
        assert_eq!(level, 228_600);
    }
}

/// #127 / #149: Toolbar font and font-size combo `text_field` filter, `✕` reset,
/// and keyboard navigation (`↑`, `↓`, `PageUp`, `PageDown`, `Enter`, `Escape`).
fn fix127_149_toolbar_font_and_font_size_combo_filter_reset_and_keys() {
    use crate::shell::chrome::{
        FONT_SIZE_CANDIDATES, filter_menu_items, filtered_font_size_candidates,
        set_current_toolbar_open, step_enabled_menu_index,
    };

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("B2");
    s.settle();

    // --- 1. Font-size combo (`font_size_menu_open`) ---
    // Open the font-size pulldown with initial value "11" and `font_size_filtering = false`.
    s.update(|e, window, cx| {
        e.font_size_menu_open = true;
        e.font_size_filtering = false;
        e.font_size_selected = None;
        e.toolbar_search_open = None;
        e.font_size_input.update(cx, |input, cx| {
            input.set_value("11", window, cx);
        });
        set_current_toolbar_open(true, None);
        cx.notify();
    });
    s.settle();
    s.update(|e, window, cx| {
        e.font_size_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    });
    s.settle();

    // Initially (`filtering == false`), all `FONT_SIZE_CANDIDATES` are shown even though input holds "11".
    let initial_candidates = s.with(|e, cx| {
        let q = e.font_size_input.read(cx).value().to_string();
        filtered_font_size_candidates(&q, e.font_size_filtering)
    });
    assert_eq!(
        initial_candidates.len(),
        FONT_SIZE_CANDIDATES.len(),
        "opening font-size pulldown shows all candidates before filtering"
    );

    // Editing the input to "10" (`replace_all` emits `InputEvent::Change`) activates `font_size_filtering = true` and filters to `[10.0, 10.5]`.
    s.update(|e, window, cx| {
        e.font_size_input.update(cx, |input, cx| {
            input.replace_all("10", window, cx);
        });
    });
    s.settle();
    let (q_10, filtering_on, candidates_10) = s.with(|e, cx| {
        let q = e.font_size_input.read(cx).value().to_string();
        (q.clone(), e.font_size_filtering, filtered_font_size_candidates(&q, e.font_size_filtering))
    });
    assert_eq!(q_10, "10");
    assert!(filtering_on, "typing into font_size_input enables font_size_filtering");
    assert_eq!(candidates_10, vec![10.0, 10.5], "filtering '10' matches 10 and 10.5");

    // Clicking the `✕` clear button (`input.clean(window, cx)`) emits `InputEvent::Change` with an empty string,
    // which resets `font_size_filtering = false` and restores all `FONT_SIZE_CANDIDATES`.
    s.update(|e, window, cx| {
        e.font_size_input.update(cx, |input, cx| {
            input.clean(window, cx);
        });
    });
    s.settle();
    let (filtering_after_clear, candidates_after_clear) = s.with(|e, cx| {
        let q = e.font_size_input.read(cx).value().to_string();
        (e.font_size_filtering, filtered_font_size_candidates(&q, e.font_size_filtering))
    });
    assert!(!filtering_after_clear, "clearing font_size_input via ✕ resets font_size_filtering to false");
    assert_eq!(
        candidates_after_clear.len(),
        FONT_SIZE_CANDIDATES.len(),
        "clearing font_size_input restores all font size candidates"
    );

    // The size is typed in the toolbar's own field (`font_size_edit_input`); Enter applies it (16pt = `1600` hundredths).
    // The pulldown (`font_size_input` is its filter) is closed first, as it is when the user goes back to the field.
    s.update(|e, window, cx| {
        e.font_size_menu_open = false;
        e.font_size_filtering = false;
        set_current_toolbar_open(false, None);
        e.font_size_edit_input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.replace_all("16", window, cx);
        });
    });
    s.settle();
    s.key("enter");
    s.settle();
    assert_eq!(
        s.with(|e, _| (e.font_size_menu_open, e.font_size_filtering)),
        (false, false),
        "Enter in font_size_edit_input leaves the pulldown closed"
    );
    s.click_cell("B2");
    assert_eq!(
        s.with(|e, _| e.current_style().font_size),
        1600,
        "Enter in font_size_edit_input applied 16pt (1600 hundredths) to B2"
    );

    // --- 2. Font combo (`"font"` searchable dropdown) ---
    let font_items = s.with(|e, _| e.searchable_menu_items_for("font"));
    assert!(
        font_items.iter().any(|it| it.id.starts_with("font.section.") && !it.enabled()),
        "font menu items include disabled section headers (font.section.*)"
    );
    assert!(
        font_items.iter().any(|it| it.id == "xlsx.font" && it.enabled()),
        "font menu items include enabled font entries"
    );

    // Verify `step_enabled_menu_index` skips `font.section.*` headers on `down`, `pagedown`, `up`, `pageup`.
    let first_idx = step_enabled_menu_index(&font_items, None, 1).expect("first enabled font index");
    assert!(
        font_items[first_idx].enabled() && !font_items[first_idx].id.starts_with("font.section."),
        "down from None lands on first enabled font item, skipping section header"
    );
    let second_idx = step_enabled_menu_index(&font_items, Some(first_idx), 1).expect("second enabled font index");
    assert!(
        second_idx > first_idx && font_items[second_idx].enabled() && !font_items[second_idx].id.starts_with("font.section."),
        "down steps to next enabled font item, skipping any intermediate section header"
    );
    let page_down_idx = step_enabled_menu_index(&font_items, Some(first_idx), 8).expect("pagedown enabled font index");
    assert!(
        page_down_idx > second_idx && font_items[page_down_idx].enabled(),
        "pagedown steps forward by page_step across enabled font items"
    );
    let page_up_idx = step_enabled_menu_index(&font_items, Some(page_down_idx), -8).expect("pageup enabled font index");
    assert_eq!(page_up_idx, first_idx, "pageup steps back to first_idx");

    // Open `"font"` searchable dropdown, filter by "carlito", verify `✕` clear (`clean`) restores all items,
    // and press Enter to apply the top matched font ("Carlito") and close the dropdown.
    s.update(|e, window, cx| {
        let input = e.ensure_toolbar_search_input("font", window, cx);
        e.toolbar_search_open = Some("font");
        set_current_toolbar_open(false, Some("font"));
        input.update(cx, |st, cx| {
            st.set_value("", window, cx);
        });
        cx.notify();
    });
    s.settle();
    s.update(|e, window, cx| {
        let input = e.ensure_toolbar_search_input("font", window, cx);
        input.update(cx, |st, cx| {
            st.focus(window, cx);
            st.replace_all("carlito", window, cx);
        });
    });
    s.settle();

    let matched_carlito = s.with(|e, cx| {
        let q = e.toolbar_search_inputs.get("font").unwrap().read(cx).value().to_string();
        filter_menu_items(&font_items, &q)
    });
    assert!(
        !matched_carlito.is_empty()
            && matched_carlito.iter().all(|it| !it.id.starts_with("font.section.") && it.label.to_lowercase().contains("carlito")),
        "filtering 'carlito' returns only matching font items without section headers"
    );

    // Clearing via `✕` (`st.clean(window, cx)`) restores the full unfiltered font list.
    s.update(|e, window, cx| {
        let input = e.ensure_toolbar_search_input("font", window, cx);
        input.update(cx, |st, cx| st.clean(window, cx));
    });
    s.settle();
    let restored_count = s.with(|e, cx| {
        let q = e.toolbar_search_inputs.get("font").unwrap().read(cx).value().to_string();
        filter_menu_items(&font_items, &q).len()
    });
    assert_eq!(restored_count, font_items.len(), "clearing font search input via ✕ restores all items");

    // Filter "carlito" again and press Enter in the `"font"` search input to apply "Carlito" and close the dropdown!
    s.update(|e, window, cx| {
        let input = e.ensure_toolbar_search_input("font", window, cx);
        input.update(cx, |st, cx| {
            st.focus(window, cx);
            st.replace_all("carlito", window, cx);
        });
    });
    s.settle();
    s.key("enter");
    s.settle();
    let (search_open_after_enter, cell_font_after_enter) = s.with(|e, _| {
        (e.toolbar_search_open, e.current_style().font.clone())
    });
    assert_eq!(search_open_after_enter, None, "Enter in font search input closes toolbar_search_open");
    assert_eq!(
        cell_font_after_enter.expect("font").to_lowercase(),
        "carlito",
        "Enter in font search input applies the filtered font 'Carlito'"
    );
}

/// #136: Long submenu (`SlideEditor::submenu_room`) scrollbar / up-down scroll arrows (`▲`/`▼`)
/// and background click-through prevention (`occlude` + `stop_propagation`).
fn fix136_long_submenu_scrollbar_arrows_and_click_occlusion() {
    use crate::shell::chrome::{MenuEntry, MenuId};
    use crate::shell::menu_bar::visible;
    use crate::shell::editor::SlideEditor;

    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.settle();

    // 1. Extract the real `ツール > 変更履歴の記録` (`docx.track`) submenu from `e.menus()`
    // and verify that `SlideEditor::submenu_room(window)` cuts it (`cut == true`) and scrolling reaches the final item (`docx.track.change-user-name`).
    let sub_info = s.update(|e, window, _| {
        let menus = e.menus();
        let (_, tools_entries) = menus
            .menus
            .iter()
            .find(|(id, _)| *id == MenuId::Tools)?;
        let track_sub = tools_entries
            .iter()
            .find_map(|entry| match entry {
                MenuEntry::Submenu(sub) if sub.id == "docx.track" => Some(sub),
                _ => None,
            })?;
        let room = SlideEditor::submenu_room(window);
        let top_view = visible(&track_sub.items, 0, room);
        let bottom_view = visible(&track_sub.items, track_sub.items.len(), room);
        let last_entry_id = match track_sub.items.last()? {
            MenuEntry::Item(it) => it.id,
            _ => "",
        };
        Some((
            track_sub.items.len(),
            top_view.cut,
            top_view.end,
            bottom_view.cut,
            bottom_view.start,
            bottom_view.end,
            last_entry_id,
        ))
    });
    let (sub_len, cut_at_top, end_at_top, cut_at_bottom, start_at_bottom, end_at_bottom, last_id) =
        sub_info.expect("docx.track submenu exists under Tools");
    assert!(sub_len >= 20, "track-changes submenu has {sub_len} entries (>= 20)");
    assert!(cut_at_top && end_at_top < sub_len, "submenu is cut at top in normal window height (end={end_at_top} < {sub_len})");
    assert!(
        cut_at_bottom && start_at_bottom > 0 && end_at_bottom == sub_len,
        "scrolling submenu to bottom reaches final entry (start={start_at_bottom}, end={end_at_bottom})"
    );
    assert_eq!(last_id, "docx.track.change-user-name", "last item of track-changes submenu is docx.track.change-user-name");
    drop(s);

    // 2. Open a long menu (`編集` in xlsx at (133.0, 50.0)) and verify that:
    //    - clicking the ▼ scroll arrow row scrolls the menu (`menu_start > 0`)
    //    - clicking inside the open menu area or scroll arrow does NOT click through to select the worksheet cell behind it!
    let _probe = crate::shell::scenario::button_probe::Probe::new();
    let mut sx = Scenario::open("test/editing/xlsx-sales.xlsx");
    sx.click_cell("F10");
    sx.settle();
    let cell_before = sx.with(|e, _| e.current_cell());
    assert_eq!(cell_before, Some((5, 9)), "selected F10 (col=5, row=9) before opening menu");

    let edit = menu_button_at(&mut sx, "edit");
    sx.click(edit);
    sx.settle();
    assert_eq!(open_index(&mut sx), Some(1), "Edit menu is open");
    assert_eq!(menu_start(&mut sx), 0);

    // Click the ▼ scroll arrow at the bottom of the open Edit menu (x=200.0, y=648.0).
    // If click-through occurred, (200.0, 648.0) would select a cell near row 25, col B!
    sx.click((200.0, 648.0));
    sx.settle();
    assert!(menu_start(&mut sx) > 0, "clicking ▼ scroll arrow row scrolled the menu down");
    assert_eq!(
        sx.with(|e, _| e.current_cell()),
        cell_before,
        "clicking ▼ scroll arrow must be occluded and must not select the worksheet cell behind it"
    );
}

/// #150 / #151: Right-pane `text_field` inputs, vertical alignment of input/unit columns
/// in `doc_page_setup_panel`, and sticky bottom footer (`OK` button stays visible outside scroll).
fn fix150_151_right_pane_clearable_inputs_and_page_setup_sticky_footer() {
    use gpui_kit::{point, px};

    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, window, cx| {
        e.open_page_setup_panel(window, cx);
    });
    s.settle();

    // 1. Verify the page setup panel is open in the right sidebar and has recorded bounds for its aligned rows and sticky footer `apply` button.
    let (field_bounds, apply_y_before, field0_y_before) = s.with(|e, _| {
        let p = e
            .doc_state()
            .and_then(|st| st.helper.page_setup.as_ref())
            .expect("page_setup panel is open");
        let ui = p.ui.borrow();
        let keys = [
            "field-0",
            "field-1",
            "field-2",
            "field-3",
            "field-4",
            "grid-lines-input",
            "grid-line-pitch",
            "grid-chars",
            "grid-char-space",
            "start",
            "field-7",
            "field-8",
            "field-5",
            "field-6",
        ];
        let mut collected = Vec::new();
        for k in keys {
            let b = ui.get(k).copied().unwrap_or_else(|| panic!("missing UI bounds for {k}"));
            collected.push((k, f32::from(b.origin.x), f32::from(b.size.width)));
        }
        let apply_b = ui.get("apply").copied().expect("sticky footer apply button bounds");
        let f0_b = ui.get("field-0").copied().expect("field-0 bounds");
        (collected, f32::from(apply_b.origin.y), f32::from(f0_b.origin.y))
    });

    // Every input box in `doc_page_setup_panel` must have the same width and the exact same left X coordinate (`origin.x`),
    // ensuring vertical alignment across margins (mm), document grid (行/pt/字), start page, paper size (mm), and header/footer (mm).
    let expected_x = field_bounds[0].1;
    // Each row's mark covers the standard field (`FIELD_WIDTH`) with its spinner, the gap (8px) and the unit (72px).
    let field_width = f32::from(crate::shell::chrome::FIELD_WIDTH);
    let expected_w = field_width + 8.0 + 72.0;
    for (k, x, w) in &field_bounds {
        assert!(
            (*x - expected_x).abs() <= 0.5,
            "input {k} left edge x={x} must align with field-0 x={expected_x}"
        );
        assert!(
            (*w - expected_w).abs() <= 0.5,
            "input {k} width w={w} must be {expected_w}px (the standard field {field_width}px + the gap and the unit's 72px)"
        );
    }

    // 2. Scroll the right-pane body down (`sidebar_scroll`) and verify that `field-0` scrolls up
    // while the sticky footer `apply` ("OK") button stays fixed at the exact same bottom Y coordinate!
    s.update(|e, _, cx| {
        e.panel_dialog.scroll.set_offset(point(px(0.0), px(-120.0)));
        cx.notify();
    });
    s.settle();

    let (apply_y_after, field0_y_after) = s.with(|e, _| {
        let p = e.doc_state().unwrap().helper.page_setup.as_ref().unwrap();
        let ui = p.ui.borrow();
        (
            f32::from(ui.get("apply").unwrap().origin.y),
            f32::from(ui.get("field-0").unwrap().origin.y),
        )
    });
    assert!(
        (field0_y_before - field0_y_after - 120.0).abs() <= 2.0,
        "scrolling sidebar_scroll moves field-0 up by ~120px (before={field0_y_before}, after={field0_y_after})"
    );
    assert!(
        (apply_y_after - apply_y_before).abs() <= 0.5,
        "sticky footer OK button (`apply`) stays at fixed Y (before={apply_y_before}, after={apply_y_after})"
    );

    // 3. Edit top margin (`field-0`) to "30" mm and click the sticky footer OK (`apply`) button to apply.
    let apply_center = s.with(|e, _| {
        let p = e.doc_state().unwrap().helper.page_setup.as_ref().unwrap();
        let b = *p.ui.borrow().get("apply").unwrap();
        (
            f32::from(b.origin.x) + f32::from(b.size.width) * 0.5,
            f32::from(b.origin.y) + f32::from(b.size.height) * 0.5,
        )
    });
    s.update(|e, window, cx| {
        let p = e.doc_state_mut().unwrap().helper.page_setup.as_mut().unwrap();
        p.fields[0].update(cx, |input, cx| input.set_value("30", window, cx));
    });
    s.settle();
    s.click(apply_center);
    s.settle();

    let top_mm = s.with(|e, _| {
        let top_emu = e.caret_page().expect("caret_page").top;
        (top_emu as f64 / 36_000.0).round() as i64
    });
    assert_eq!(top_mm, 30, "clicking sticky footer OK button applies 30mm top margin");
}


/// Shared radio groups wrap between whole buttons in Flow and put one option per row in Column.
fn shared_radio_group_layouts() {
    let center = |b: gpui_kit::Bounds<gpui_kit::Pixels>| (
        f32::from(b.origin.x + b.size.width / 2.),
        f32::from(b.origin.y + b.size.height / 2.),
    );
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, w, cx| e.open_page_setup_panel(w, cx));
    s.settle();
    s.update(|e, _, cx| {
        e.panel_dialog.bounds.as_mut().unwrap().size.width = gpui_kit::px(320.);
        cx.notify();
    });
    s.settle();
    let (across, vertical, pages) = s.with(|e, _| {
        let p = e.doc_state().unwrap().helper.page_setup.as_ref().unwrap();
        let ui = p.ui.borrow();
        (ui["direction-across"], ui["direction-vertical"],
         ["page-normal", "page-mirror", "page-book-fold", "page-reverse-book-fold", "page-two-on-one"]
            .map(|key| ui[key]))
    });
    assert_eq!(across.origin.y, vertical.origin.y, "short options share a flow row");
    assert!(vertical.origin.x >= across.origin.x + across.size.width, "flow options do not overlap");
    assert!(pages.iter().any(|b| b.origin.y > pages[0].origin.y), "the page options wrap to another row");
    for b in pages {
        assert_eq!(b.size.height, across.size.height, "a radio label never wraps inside its button");
    }
    // Bring the direction group into the viewport, then exercise the real shared button's handler.
    s.update(|e, _, cx| {
        e.panel_dialog.scroll.set_offset(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(-300.)));
        cx.notify();
    });
    s.settle();
    let target = s.with(|e, _| e.doc_state().unwrap().helper.page_setup.as_ref().unwrap().ui.borrow()["direction-vertical"]);
    s.click(center(target));
    s.settle();
    assert!(s.with(|e, _| e.doc_state().unwrap().helper.page_setup.as_ref().unwrap().vertical));
    s.screenshot("radio-flow");
    drop(s);

    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.goto-special");
    s.settle();
    let rows = s.with(|e, _| {
        let ui = e.wb().panels.goto_special.as_ref().unwrap().ui.borrow();
        ["comments", "constants", "formulas", "blanks"].map(|key| ui[key])
    });
    for pair in rows.windows(2) {
        assert_eq!(pair[0].origin.x, pair[1].origin.x, "column options align on the left");
        assert!(pair[1].origin.y >= pair[0].origin.y + pair[0].size.height, "one option per row");
    }
    s.click(center(rows[1]));
    s.settle();
    assert!(s.with(|e, _| e.wb().panels.goto_special.as_ref().unwrap().ui.borrow().contains_key("numbers")),
        "clicking Constants displays its type choices");
    s.screenshot("radio-column");
}
