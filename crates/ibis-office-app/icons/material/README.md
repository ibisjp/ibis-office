# Material Symbols（同梱のアイコン）

ibisOffice のタイトル行・メニュー・ツールバー・サイドバーのアイコン。ibisOffice のソースではなく、別ライセンスの同梱物（著作権は Google LLC）。

- 出典：Google 公式のリポジトリ https://github.com/google/material-design-icons のコミット `bd8cb85bd4bad964fe6918f79665bb40c3a8efef`（2026-09-25）
- 取り込んだもの：`symbols/web/<名前>/materialsymbolsoutlined/<名前>_20px.svg`（Outlined、太さ 400、グレード 0、光学サイズ 20）。塗りつぶし版は同じ場所の `<名前>_fill1_20px.svg`
- ライセンス：Apache License 2.0（同じフォルダの `LICENSE` は、同じコミットのリポジトリの `LICENSE`）

## 手を加えた点

- ファイル名だけを変えた：`<名前>_20px.svg` → `<名前>.svg`、`<名前>_fill1_20px.svg` → `<名前>_fill.svg`。
- 図形（SVG の中身）は変えていない。色は描画のときに文字色で塗る（ファイルは変えない）。

## 足すとき

同じコミット（または新しいコミット。そのときはこの README の SHA を直す）から同じ形で取り、この一覧に足す。アプリへの埋め込みは `build.rs` がこのフォルダの全 SVG を自動で行う。使い方は `src/shell/icons.rs`（`Material("名前")`）。

## 一覧（207 ファイル）

Outlined（197）：accessibility_new、add、add_circle、add_comment、align_horizontal_center、align_horizontal_left、align_horizontal_right、align_vertical_bottom、align_vertical_center、align_vertical_top、animation、arrow_downward、arrow_drop_down、arrow_forward、arrow_left、arrow_right、arrow_right_alt、arrow_selector_tool、arrow_upward、aspect_ratio、auto_awesome、bar_chart、border_all、border_bottom、border_clear、border_color、border_horizontal、border_inner、border_left、border_outer、border_right、border_style、border_top、border_vertical、cell_merge、center_focus_strong、chat_bubble、check、checklist、cleaning_services、close、co_present、comment、content_copy、content_cut、content_paste、content_paste_go、control_point_duplicate、conversion_path、crop、crop_landscape、crop_square、dashboard_customize、dataset、decimal_decrease、decimal_increase、delete、delete_sweep、description、dock_to_bottom、dock_to_left、dock_to_right、download、drag_indicator、draw、edit、emoji_symbols、ev_shadow、expand_less、filter_alt、filter_list、filter_none、find_replace、fit_screen、flip、flip_to_back、flip_to_front、folder_open、format_align_center、format_align_justify、format_align_left、format_align_right、format_bold、format_clear、format_color_reset、format_color_text、format_image_left、format_indent_decrease、format_indent_increase、format_italic、format_line_spacing、format_list_bulleted、format_list_numbered、format_paint、format_strikethrough、format_text_clip、format_text_overflow、format_text_wrap、format_underlined、fullscreen、functions、gesture、grid_on、grid_view、group_work、help、horizontal_distribute、horizontal_rule、image、info、ink_highlighter、insert_chart、insert_page_break、ios_share、keyboard、keyboard_arrow_down、keyboard_arrow_up、label、language、line_end_arrow_notch、line_start、line_style、line_weight、link、lock、menu、menu_book、more_vert、movie、music_note、new_releases、note_add、notes、open_with、palette、pause、percent、photo_camera、picture_as_pdf、play_arrow、play_circle、polyline、print、rate_review、rectangle、redo、remove、reset_image、rotate_90_degrees_ccw、rotate_90_degrees_cw、rule、save、save_as、schedule、search、select_all、send、settings、shapes、slideshow、skip_previous、sort、spellcheck、splitscreen、swap_horiz、table、table_chart、tag、text_fields、text_format、text_rotate_up、text_rotate_vertical、text_rotation_angledown、text_rotation_angleup、text_rotation_down、text_rotation_none、toc、tune、turn_right、undo、upload_file、vertical_align_bottom、vertical_align_center、vertical_align_top、vertical_distribute、vertical_split、view_column、view_quilt、visibility、visibility_off、volume_off、volume_up、wallpaper、workspaces、wrap_text、zoom_in、zoom_out

塗りつぶし（10）：description_fill、dock_to_bottom_fill、dock_to_left_fill、dock_to_right_fill、format_color_fill、pause_fill、play_arrow_fill、skip_previous_fill、slideshow_fill、table_chart_fill
