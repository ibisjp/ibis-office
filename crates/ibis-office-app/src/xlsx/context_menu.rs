// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The right button on a cell: Excel for Mac's menu (切り取り, コピー,
//! 貼り付け, 形式を選択して貼り付け, 挿入, 削除, 数式と値のクリア, フィルター,
//! 並べ替え, メモ, セルの書式設定, ハイパーリンク, 名前の定義). The cell
//! pressed is selected unless it is within the selection (the menu then
//! acts on the selection). The pop-up is the header menu's (`MenuItem`,
//! `fill_menu`).
use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use gpui_kit::*;
use ibis_doc_model::ColumnFilter;

impl SlideEditor {
    /// The right button on the sheet.
    pub(crate) fn on_sheet_right_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.commit_edit(window, cx);
        self.focus_handle.focus(window, cx);
        self.selection = None;
        let Some((column, row)) = self.cell_under(event.position) else { return };
        self.commit_cell_edit(cx);
        let within = self.selected_ranges().iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column));
        let on_link=self.current_sheet().is_some_and(|(_,s)|s.hyperlinks.iter().any(|h| (h.range.first_row..=h.range.last_row).contains(&row)&&(h.range.first_column..=h.range.last_column).contains(&column)));
        if !within || on_link {
            self.set_cell(column, row);
        }
        let entries = self.cell_menu();
        self.open_popup_menu(entries, event.position, window, cx);
    }

    /// The menu's entries, in Excel for Mac's order.
    pub(crate) fn cell_menu(&self) -> Vec<MenuEntry> {
        let menus = self.menus();
        let has_thread = self.current_cell().zip(self.current_sheet()).is_some_and(|((c, r), (_, sheet))| sheet.thread_at(r, c).is_some());
        let has_note = self.current_cell().zip(self.current_sheet()).is_some_and(|((c, r), (_, sheet))| sheet.comment_at(r, c).is_some());
        let has_cell_image = self.current_cell().zip(self.current_sheet()).is_some_and(|((c, r), (_, sheet))| sheet.cell_images.iter().any(|i| i.column == c && i.row == r));
        let mut out: Vec<MenuEntry> = vec![
            MenuItem::new("xlsx.cell-cut", "_xlsx.cut").icon("content_cut").shortcut("⌘X").on(|this, _, cx| this.copy_cells(true, cx)).into(),
            MenuItem::new("xlsx.cell-copy", "_shell.copy").icon("content_copy").shortcut("⌘C").on(|this, _, cx| this.copy_cells(false, cx)).into(),
            MenuItem::new("xlsx.cell-paste", "_xlsx.paste").icon("content_paste").shortcut("⌘V").on(|this, _, cx| this.paste_cells(false, cx)).into(),
        ];
        if has_cell_image {
            out.push(MenuItem::new("xlsx.cell-place-over-cells", "_xlsx.place-over-cells").icon("filter_none").on(|this, _, cx| this.place_current_cell_picture_over_cells(cx)).into());
        }
        out.extend(menus.find_entry("xlsx.paste-special"));
        out.push(MenuEntry::Separator);
        out.push(MenuItem::new("xlsx.cell-insert", "cells.insert-menu").icon("add").shortcut("⌘+").on(|this, _, cx| this.smart_insert(cx)).into());
        out.push(MenuItem::new("xlsx.cell-delete", "cells.delete-menu").icon("delete").shortcut("⌘-").on(|this, _, cx| this.smart_delete(cx)).into());
        out.push(MenuItem::new("xlsx.cell-clear", "_xlsx.clear-contents").on(|this, _, cx| this.clear_selected_cells(cx)).into());
        out.push(MenuEntry::Separator);
        let has_active_filter = self.current_sheet().is_some_and(|(_, sheet)| {
            sheet.auto_filter.is_some() || sheet.tables.iter().any(|t| t.filter.is_some())
        });
        let has_cell_icon = self
            .current_cell()
            .zip(self.current_sheet())
            .is_some_and(|((c, r), (_, sheet))| ibis_doc_model::sheet_command::cell_icon(sheet, r, c).is_some());
        out.push(
            Submenu::new(
                "xlsx.cell-filter",
                "filter_alt",
                "_xlsx.filters",
                vec![
                    MenuItem::new("xlsx.cell-filter-toggle", "_xlsx.create-or-remove-filter").on(|this, _, cx| this.toggle_auto_filter(cx)).into(),
                    MenuItem::new("xlsx.cell-filter-clear", "xlsx.clear-filter").disabled(!has_active_filter).on(|this, _, cx| this.clear_auto_filter(cx)).into(),
                    MenuItem::new("xlsx.cell-filter-reapply", "xlsx.reapply-filter").disabled(!has_active_filter).on(|this, _, cx| this.reapply_auto_filter(cx)).into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-filter-value", "_xlsx.filter-by-selected-cell-s-value").on(|this, _, cx| this.filter_by_selected(cx, |text, _| ColumnFilter::Values(vec![text.to_string()]))).into(),
                    MenuItem::new("xlsx.cell-filter-fill", "_xlsx.filter-by-selected-cell-s-color").on(|this, _, cx| this.filter_by_selected(cx, |_, style| ColumnFilter::Color { cell: true, color: style.fill })).into(),
                    MenuItem::new("xlsx.cell-filter-font", "_xlsx.filter-by-selected-cell-s-font-color").on(|this, _, cx| this.filter_by_selected(cx, |_, style| ColumnFilter::Color { cell: false, color: Some(style.color) })).into(),
                    MenuItem::new("xlsx.cell-filter-icon", "xlsx.filter-by-selected-icon").disabled(!has_cell_icon).on(|this, _, cx| this.filter_by_selected_icon(cx)).into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-filter-advanced", "xlsx.advanced-filter").on(|this, window, cx| this.open_advanced_filter_panel(window, cx)).into(),
                ],
            )
            .into(),
        );
        out.push(
            Submenu::new(
                "xlsx.cell-sort",
                "sort",
                "_xlsx.sort-2",
                vec![
                    MenuItem::new("xlsx.cell-sort-ascending", "_xlsx.ascending").on(|this, _, cx| this.sort_by_selected_column(true, cx)).into(),
                    MenuItem::new("xlsx.cell-sort-descending", "_xlsx.descending").on(|this, _, cx| this.sort_by_selected_column(false, cx)).into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-sort-fill", "xlsx.sort-by-selected-color").on(|this, _, cx| this.sort_by_selected_cell_color(cx)).into(),
                    MenuItem::new("xlsx.cell-sort-font", "xlsx.sort-by-selected-font-color").on(|this, _, cx| this.sort_by_selected_font_color(cx)).into(),
                    MenuItem::new("xlsx.cell-sort-icon", "xlsx.sort-by-selected-icon").disabled(!has_cell_icon).on(|this, _, cx| this.sort_by_selected_icon(cx)).into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-sort-custom", "xlsx.sort-manage").on(|this, window, cx| this.open_sort_panel(window, cx)).into(),
                ],
            )
            .into(),
        );
        if let Some(table_idx) = self.table_at_selection() {
            let cur_col = self.current_cell().map(|(c, _)| c).unwrap_or(0);
            let (
                has_header,
                has_totals,
                has_filter,
                banded_rows,
                banded_cols,
                first_col,
                last_col,
                current_style,
                current_total_fn,
            ) = self
                .current_sheet()
                .and_then(|(_, s)| s.tables.get(table_idx))
                .map(|t| {
                    let st = t.style.as_ref();
                    let col_idx = cur_col.saturating_sub(t.range.first_column) as usize;
                    let total_fn = t.totals.get(col_idx).and_then(|tt| tt.function.clone());
                    (
                        t.header_rows > 0,
                        t.totals_rows > 0,
                        t.filter.is_some(),
                        st.is_some_and(|s| s.row_stripes),
                        st.is_some_and(|s| s.column_stripes),
                        st.is_some_and(|s| s.first_column),
                        st.is_some_and(|s| s.last_column),
                        st.map(|s| s.name.clone()).unwrap_or_default(),
                        total_fn,
                    )
                })
                .unwrap_or((true, false, false, false, false, false, false, String::new(), None));

            let select_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-table-select-all", "xlsx.table-select-all")
                    .on(|this, window, cx| {
                        this.select_table_part(0, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-select-data", "xlsx.table-select-data")
                    .on(|this, window, cx| {
                        this.select_table_part(1, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-select-col", "xlsx.table-select-col")
                    .on(|this, window, cx| {
                        this.select_table_part(2, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-select-col-data", "xlsx.table-select-col-data")
                    .on(|this, window, cx| {
                        this.select_table_part(3, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-select-row", "xlsx.table-select-row")
                    .on(|this, window, cx| {
                        this.select_table_part(4, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];

            let mut total_fn_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-table-total-none", "_shell.none")
                    .checked(has_totals && current_total_fn.is_none())
                    .on(|this, window, cx| {
                        this.set_table_total_at_selection(None, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];
            for &(f, _, label) in &super::table_design::TOTALS {
                let id = match f {
                    "average" => "xlsx.cell-table-total-average",
                    "count" => "xlsx.cell-table-total-count",
                    "countNums" => "xlsx.cell-table-total-count-nums",
                    "max" => "xlsx.cell-table-total-max",
                    "min" => "xlsx.cell-table-total-min",
                    "sum" => "xlsx.cell-table-total-sum",
                    "stdDev" => "xlsx.cell-table-total-stddev",
                    "var" => "xlsx.cell-table-total-var",
                    _ => "xlsx.cell-table-total-custom",
                };
                total_fn_items.push(
                    MenuItem::new(id, label)
                        .checked(has_totals && current_total_fn.as_deref() == Some(f))
                        .on(move |this, window, cx| {
                            this.set_table_total_at_selection(Some(f), cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                );
            }

            let style_opt_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-table-header-row", "docx.table-design.header-row")
                    .checked(has_header)
                    .on(|this, window, cx| {
                        this.toggle_table_header_row_at_selection(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-totals-opt", "docx.table-design.total-row")
                    .checked(has_totals)
                    .on(|this, window, cx| {
                        this.toggle_table_totals_row_at_selection(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-banded-rows", "docx.table-design.banded-rows")
                    .checked(banded_rows)
                    .on(|this, window, cx| {
                        this.toggle_table_banded_rows_at_selection(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-first-column", "docx.table-design.first-col")
                    .checked(first_col)
                    .on(|this, window, cx| {
                        this.toggle_table_first_column_at_selection(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-last-column", "docx.table-design.last-col")
                    .checked(last_col)
                    .on(|this, window, cx| {
                        this.toggle_table_last_column_at_selection(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-banded-columns", "docx.table-design.banded-cols")
                    .checked(banded_cols)
                    .on(|this, window, cx| {
                        this.toggle_table_banded_columns_at_selection(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-table-filter-button", "_xlsx.filter-button")
                    .checked(has_filter)
                    .on(|this, window, cx| {
                        this.toggle_table_filter_button_at_selection(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];

            let mut style_items: Vec<MenuEntry> = [
                ("xlsx.cell-table-style-light-1", "TableStyleLight1"),
                ("xlsx.cell-table-style-light-2", "TableStyleLight2"),
                ("xlsx.cell-table-style-light-9", "TableStyleLight9"),
                ("xlsx.cell-table-style-medium-2", "TableStyleMedium2"),
                ("xlsx.cell-table-style-medium-4", "TableStyleMedium4"),
                ("xlsx.cell-table-style-medium-7", "TableStyleMedium7"),
                ("xlsx.cell-table-style-medium-9", "TableStyleMedium9"),
                ("xlsx.cell-table-style-dark-1", "TableStyleDark1"),
                ("xlsx.cell-table-style-dark-2", "TableStyleDark2"),
            ]
            .into_iter()
            .map(|(id, name)| {
                MenuItem::new(id, name)
                    .checked(current_style == name)
                    .on(move |this, window, cx| {
                        this.format_as_table_with_style(name, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into()
            })
            .collect();
            style_items.push(MenuEntry::Separator);
            style_items.push(
                MenuItem::new("xlsx.cell-table-style-clear", "xlsx.clear-all")
                    .checked(current_style.is_empty())
                    .on(|this, window, cx| {
                        this.clear_table_style_at_selection(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            );

            out.push(
                Submenu::new(
                    "xlsx.cell-table",
                    "table",
                    "_xlsx.table",
                    vec![
                        Submenu::new(
                            "xlsx.cell-table-select",
                            "select_all",
                            "xlsx.table-select",
                            select_items,
                        )
                        .into(),
                        MenuEntry::Separator,
                        MenuItem::new("xlsx.cell-table-insert-row-above", "xlsx.table-insert-row-above")
                            .on(|this, window, cx| {
                                this.insert_table_rows_above(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-table-insert-row-below", "xlsx.table-insert-row-below")
                            .on(|this, window, cx| {
                                this.insert_table_rows_below(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-table-insert-col-left", "xlsx.table-insert-col-left")
                            .on(|this, window, cx| {
                                this.insert_table_columns_left(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-table-insert-col-right", "xlsx.table-insert-col-right")
                            .on(|this, window, cx| {
                                this.insert_table_columns_right(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-table-delete-row", "xlsx.table-delete-row")
                            .on(|this, window, cx| {
                                this.delete_table_rows(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-table-delete-col", "xlsx.table-delete-col")
                            .on(|this, window, cx| {
                                this.delete_table_columns(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuEntry::Separator,
                        MenuItem::new("xlsx.cell-table-totals", "docx.table-design.total-row")
                            .checked(has_totals)
                            .on(|this, window, cx| {
                                this.toggle_table_totals_row_at_selection(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        Submenu::new(
                            "xlsx.cell-table-total-fn",
                            "functions",
                            "_xlsx.total-row-functions",
                            total_fn_items,
                        )
                        .into(),
                        Submenu::new(
                            "xlsx.cell-table-style-options",
                            "tune",
                            "_xlsx.table-style-options",
                            style_opt_items,
                        )
                        .into(),
                        Submenu::new(
                            "xlsx.cell-table-styles",
                            "palette",
                            "_xlsx.table-styles",
                            style_items,
                        )
                        .into(),
                        MenuEntry::Separator,
                        MenuItem::new("xlsx.cell-table-to-range", "_xlsx.convert-to-range")
                            .on(|this, window, cx| {
                                this.table_to_range(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-table-resize", "_xlsx.table-resize")
                            .on(|this, window, cx| this.resize_table_dialog(window, cx))
                            .into(),
                        MenuItem::new("xlsx.cell-table-design", "_xlsx.table-design-2")
                            .icon("table")
                            .on(|this, window, cx| this.open_table_design(window, cx))
                            .into(),
                        MenuItem::new("xlsx.cell-table-dedupe", "_xlsx.remove-duplicates")
                            .on(|this, window, cx| this.open_dedupe_panel(window, cx))
                            .into(),
                        MenuItem::new("xlsx.cell-table-pivot", "xlsx.pivot-insert")
                            .icon("pivot_table_chart")
                            .on(|this, window, cx| this.open_create_pivot(window, cx))
                            .into(),
                        MenuItem::new("xlsx.cell-table-slicer", "xlsx.slicer-insert")
                            .icon("filter_alt")
                            .on(|this, window, cx| this.open_slicer_insert_panel(window, cx))
                            .into(),
                        MenuItem::new("xlsx.cell-table-timeline", "xlsx.timeline-insert")
                            .icon("view_timeline")
                            .on(|this, window, cx| this.open_timeline_insert_panel(window, cx))
                            .into(),
                    ],
                )
                .into(),
            );
        }
        if let Some(pivot_idx) = self.pivot_at_selection() {
            use super::pivot::PivotReportLayout;
            let current_layout = self.pivot_report_layout_at_selection();
            let current_subtotals = self.pivot_subtotals_mode_at_selection();
            let current_pivot_style = self.pivot_style_name_at_selection().unwrap_or_default();
            let (
                row_grand,
                col_grand,
                repeat_labels,
                blank_rows,
                row_headers,
                col_headers,
                banded_rows,
                banded_cols,
                show_drill,
            ) = self
                .current_sheet()
                .and_then(|(_, s)| s.pivots.get(pivot_idx))
                .map(|p| {
                    let st = p.style.as_ref();
                    let has_blank = p
                        .fields
                        .iter()
                        .filter(|f| f.axis == Some(ibis_doc_model::PivotAxis::Row))
                        .any(|f| f.insert_blank_row);
                    (
                        p.row_grand_totals,
                        p.col_grand_totals,
                        p.fill_down_labels,
                        has_blank,
                        st.is_none_or(|s| s.row_headers),
                        st.is_none_or(|s| s.col_headers),
                        st.is_some_and(|s| s.row_stripes),
                        st.is_some_and(|s| s.col_stripes),
                        p.show_drill,
                    )
                })
                .unwrap_or((true, true, false, false, true, true, false, false, true));

            let mut pivot_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-pivot-refresh", "xlsx.pivot-refresh")
                    .icon("refresh")
                    .on(|this, window, cx| {
                        this.refresh_pivots(false, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-show-details", "xlsx.pivot-show-details")
                    .on(|this, window, cx| {
                        this.show_pivot_details_at_selection(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-drill-buttons", "xlsx.pivot-drill-buttons")
                    .checked(show_drill)
                    .on(|this, window, cx| {
                        this.toggle_pivot_drill_buttons(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];
            if let Some((_, _, current_fn, current_show)) = self.active_pivot_data_field() {
                let fn_items: Vec<MenuEntry> = [
                    ("xlsx.cell-pivot-fn-sum", "sum", "_xlsx.sum"),
                    ("xlsx.cell-pivot-fn-count", "count", "_xlsx.count"),
                    ("xlsx.cell-pivot-fn-average", "average", "_xlsx.average"),
                    ("xlsx.cell-pivot-fn-max", "max", "_xlsx.max-2"),
                    ("xlsx.cell-pivot-fn-min", "min", "_xlsx.min"),
                    ("xlsx.cell-pivot-fn-product", "product", "_xlsx.product"),
                ]
                .into_iter()
                .map(|(id, func, label)| {
                    MenuItem::new(id, label)
                        .checked(current_fn == func)
                        .on(move |this, window, cx| {
                            this.set_pivot_summarize_by_at_selection(func, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into()
                })
                .collect();
                pivot_items.push(Submenu::new("xlsx.cell-pivot-summarize-by", "functions", "_xlsx.function", fn_items).into());

                let show_items: Vec<MenuEntry> = [
                    ("xlsx.cell-pivot-show-normal", None, "_xlsx.no-calculation"),
                    ("xlsx.cell-pivot-show-pct-total", Some("percentOfTotal"), "_xlsx.of-grand-total"),
                    ("xlsx.cell-pivot-show-pct-col", Some("percentOfCol"), "_xlsx.of-column-total"),
                    ("xlsx.cell-pivot-show-pct-row", Some("percentOfRow"), "_xlsx.of-row-total"),
                ]
                .into_iter()
                .map(|(id, show_as, label)| {
                    let is_active = match show_as {
                        None => current_show.is_none() || current_show.as_deref() == Some("normal"),
                        Some(mode) => current_show.as_deref() == Some(mode),
                    };
                    MenuItem::new(id, label)
                        .checked(is_active)
                        .on(move |this, window, cx| {
                            this.set_pivot_show_values_as_at_selection(show_as, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into()
                })
                .collect();
                pivot_items.push(
                    Submenu::new(
                        "xlsx.cell-pivot-show-values-as",
                        "percent",
                        "_xlsx.calculation-type-show-values-as",
                        show_items,
                    )
                    .into(),
                );
            }

            let layout_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-pivot-layout-compact", "_xlsx.show-in-compact-form")
                    .checked(current_layout == Some(PivotReportLayout::Compact))
                    .on(|this, window, cx| {
                        this.set_pivot_report_layout(PivotReportLayout::Compact, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-layout-outline", "_xlsx.show-in-outline-form")
                    .checked(current_layout == Some(PivotReportLayout::Outline))
                    .on(|this, window, cx| {
                        this.set_pivot_report_layout(PivotReportLayout::Outline, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-layout-tabular", "_xlsx.show-in-tabular-form")
                    .checked(current_layout == Some(PivotReportLayout::Tabular))
                    .on(|this, window, cx| {
                        this.set_pivot_report_layout(PivotReportLayout::Tabular, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuEntry::Separator,
                MenuItem::new("xlsx.cell-pivot-subtotal-off", "_xlsx.do-not-show-subtotals")
                    .checked(current_subtotals == Some(0))
                    .on(|this, window, cx| {
                        this.set_pivot_subtotals(0, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-subtotal-bottom", "_xlsx.show-all-subtotals-at-bottom-of-group")
                    .checked(current_subtotals == Some(1))
                    .on(|this, window, cx| {
                        this.set_pivot_subtotals(1, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-subtotal-top", "_xlsx.show-all-subtotals-at-top-of-group")
                    .checked(current_subtotals == Some(2))
                    .on(|this, window, cx| {
                        this.set_pivot_subtotals(2, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuEntry::Separator,
                MenuItem::new("xlsx.cell-pivot-repeat-labels", "_xlsx.repeat-all-item-labels")
                    .checked(repeat_labels)
                    .on(|this, window, cx| {
                        this.toggle_pivot_repeat_item_labels(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-blank-rows", "xlsx.pivot-blank-rows")
                    .checked(blank_rows)
                    .on(|this, window, cx| {
                        this.toggle_pivot_blank_rows(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuEntry::Separator,
                MenuItem::new("xlsx.cell-pivot-grand-rows", "_xlsx.show-grand-total-row")
                    .checked(row_grand)
                    .on(|this, window, cx| {
                        this.toggle_pivot_row_grand_totals(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-grand-cols", "_xlsx.show-grand-total-column")
                    .checked(col_grand)
                    .on(|this, window, cx| {
                        this.toggle_pivot_col_grand_totals(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];

            let pivot_style_opt_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-pivot-row-headers", "xlsx.pivot-row-headers")
                    .checked(row_headers)
                    .on(|this, window, cx| {
                        this.toggle_pivot_row_headers(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-col-headers", "xlsx.pivot-col-headers")
                    .checked(col_headers)
                    .on(|this, window, cx| {
                        this.toggle_pivot_col_headers(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-banded-rows", "xlsx.pivot-banded-rows")
                    .checked(banded_rows)
                    .on(|this, window, cx| {
                        this.toggle_pivot_banded_rows(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-banded-columns", "xlsx.pivot-banded-columns")
                    .checked(banded_cols)
                    .on(|this, window, cx| {
                        this.toggle_pivot_banded_columns(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];

            const CELL_PIVOT_STYLES: [(&str, &str, &str); 13] = [
                ("xlsx.cell-pivot-style-none", "", "_shell.none"),
                ("xlsx.cell-pivot-style-light-1", "PivotStyleLight1", "_xlsx.light"),
                ("xlsx.cell-pivot-style-light-2", "PivotStyleLight2", "_xlsx.light"),
                ("xlsx.cell-pivot-style-light-8", "PivotStyleLight8", "_xlsx.light"),
                ("xlsx.cell-pivot-style-light-9", "PivotStyleLight9", "_xlsx.light"),
                ("xlsx.cell-pivot-style-light-16", "PivotStyleLight16", "_xlsx.light"),
                ("xlsx.cell-pivot-style-medium-1", "PivotStyleMedium1", "_xlsx.medium-2"),
                ("xlsx.cell-pivot-style-medium-2", "PivotStyleMedium2", "_xlsx.medium-2"),
                ("xlsx.cell-pivot-style-medium-7", "PivotStyleMedium7", "_xlsx.medium-2"),
                ("xlsx.cell-pivot-style-medium-9", "PivotStyleMedium9", "_xlsx.medium-2"),
                ("xlsx.cell-pivot-style-medium-14", "PivotStyleMedium14", "_xlsx.medium-2"),
                ("xlsx.cell-pivot-style-dark-1", "PivotStyleDark1", "_xlsx.dark"),
                ("xlsx.cell-pivot-style-dark-3", "PivotStyleDark3", "_xlsx.dark"),
            ];
            let pivot_style_preset_items: Vec<MenuEntry> = CELL_PIVOT_STYLES
                .iter()
                .map(|&(item_id, style_name, group_label)| {
                    let label = if style_name.is_empty() {
                        crate::shell::i18n::text(group_label)
                    } else {
                        let num = style_name.trim_start_matches(|c: char| !c.is_ascii_digit());
                        gpui_kit::SharedString::from(format!("{} {num}", crate::shell::i18n::text(group_label)))
                    };
                    MenuItem::new(item_id, label)
                        .checked(current_pivot_style == style_name)
                        .on(move |this, window, cx| {
                            this.set_pivot_style_named(style_name, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into()
                })
                .collect();

            let pivot_select_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-pivot-select-entire", "xlsx.pivot-select-entire-table")
                    .on(|this, window, cx| {
                        this.select_pivot_part(0, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-select-values", "xlsx.pivot-select-values")
                    .on(|this, window, cx| {
                        this.select_pivot_part(1, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-select-labels", "xlsx.pivot-select-labels")
                    .on(|this, window, cx| {
                        this.select_pivot_part(2, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];

            let pivot_clear_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-pivot-clear-all", "xlsx.pivot-clear-all")
                    .on(|this, window, cx| {
                        this.clear_pivot_all(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-clear-filters", "xlsx.pivot-clear-filters")
                    .on(|this, window, cx| {
                        this.clear_pivot_filters(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];

            pivot_items.extend([
                MenuEntry::Separator,
                Submenu::new("xlsx.cell-pivot-layout", "view_quilt", "_shell.layout", layout_items).into(),
                Submenu::new(
                    "xlsx.cell-pivot-style-options",
                    "tune",
                    "_xlsx.table-style-options",
                    pivot_style_opt_items,
                )
                .into(),
                Submenu::new(
                    "xlsx.cell-pivot-styles",
                    "palette",
                    "xlsx.pivot-styles",
                    pivot_style_preset_items,
                )
                .into(),
                Submenu::new(
                    "xlsx.cell-pivot-select",
                    "select_all",
                    "xlsx.pivot-select",
                    pivot_select_items,
                )
                .into(),
                Submenu::new(
                    "xlsx.cell-pivot-clear",
                    "clear_all",
                    "xlsx.pivot-clear",
                    pivot_clear_items,
                )
                .into(),
                MenuEntry::Separator,
                MenuItem::new("xlsx.cell-pivot-group", "xlsx.pivot-group")
                    .on(|this, window, cx| this.open_pivot_group(window, cx))
                    .into(),
                MenuItem::new("xlsx.cell-pivot-ungroup", "xlsx.pivot-ungroup")
                    .on(|this, window, cx| {
                        this.pivot_ungroup(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-pivot-calculated-field", "xlsx.pivot-calculated-field")
                    .icon("functions")
                    .on(|this, window, cx| this.open_pivot_calc_field(window, cx))
                    .into(),
                MenuItem::new("xlsx.cell-pivot-calculated-item", "xlsx.pivot-calculated-item")
                    .icon("functions")
                    .on(|this, window, cx| this.open_pivot_calc_item(window, cx))
                    .into(),
                MenuEntry::Separator,
                MenuItem::new("xlsx.cell-pivot-move", "xlsx.pivot-move")
                    .icon("open_with")
                    .on(|this, window, cx| this.open_pivot_move(window, cx))
                    .into(),
                MenuItem::new("xlsx.cell-pivot-change-source", "xlsx.pivot-change-data-source")
                    .icon("dataset")
                    .on(|this, window, cx| this.open_pivot_change_source(window, cx))
                    .into(),
                MenuItem::new("xlsx.cell-pivot-chart", "xlsx.pivot-chart-insert")
                    .icon("insert_chart")
                    .on(|this, window, cx| this.open_insert_pivot_chart(window, cx))
                    .into(),
                MenuItem::new("xlsx.cell-pivot-slicer", "xlsx.slicer-insert")
                    .icon("filter_alt")
                    .on(|this, window, cx| this.open_slicer_insert_panel(window, cx))
                    .into(),
                MenuItem::new("xlsx.cell-pivot-timeline", "xlsx.timeline-insert")
                    .icon("timeline")
                    .on(|this, window, cx| this.open_timeline_insert_panel(window, cx))
                    .into(),
                MenuEntry::Separator,
                MenuItem::new("xlsx.cell-pivot-value-settings", "xlsx.pivot-value-settings")
                    .on(|this, window, cx| this.open_pivot_value_settings(window, cx))
                    .into(),
                MenuItem::new("xlsx.cell-pivot-options", "xlsx.pivot-options")
                    .on(|this, window, cx| this.open_pivot_options(window, cx))
                    .into(),
                MenuItem::new("xlsx.cell-pivot-fields", "xlsx.pivot-fields")
                    .icon("table_chart")
                    .on(|this, window, cx| this.open_pivot_fields(window, cx))
                    .into(),
            ]);
            out.push(Submenu::new("xlsx.cell-pivot", "pivot_table_chart", "_xlsx.pivottable", pivot_items).into());
        }
        if let Some((_, sp_group)) = self.sparkline_group_at_selection() {
            use super::sparkline_dialog::{SPARKLINE_STYLES, SparklinePointKind as SPK};
            use ibis_doc_model::SparklineKind as SK;
            let active_preset = SPARKLINE_STYLES.iter().position(|p| {
                sp_group.series == Some(p.series)
                    && sp_group.negative == Some(p.negative)
                    && sp_group.high_color == Some(p.high)
                    && sp_group.low_color == Some(p.low)
            });
            const CELL_SPARKLINE_STYLE_IDS: [(&str, &str, usize); 6] = [
                ("xlsx.cell-sparkline-style-1", "xlsx.sparkline-style-1", 0),
                ("xlsx.cell-sparkline-style-2", "xlsx.sparkline-style-2", 1),
                ("xlsx.cell-sparkline-style-3", "xlsx.sparkline-style-3", 2),
                ("xlsx.cell-sparkline-style-4", "xlsx.sparkline-style-4", 3),
                ("xlsx.cell-sparkline-style-5", "xlsx.sparkline-style-5", 4),
                ("xlsx.cell-sparkline-style-6", "xlsx.sparkline-style-6", 5),
            ];
            let style_items: Vec<MenuEntry> = CELL_SPARKLINE_STYLE_IDS
                .into_iter()
                .map(|(id, label, idx)| {
                    MenuItem::new(id, label)
                        .checked(active_preset == Some(idx))
                        .on(move |this, window, cx| {
                            this.set_selected_sparklines_style(idx, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into()
                })
                .collect();

            const CELL_SPARKLINE_COLORS: [(&str, &str, u32); 8] = [
                ("xlsx.cell-sparkline-color-blue", "xlsx.sparkline-color-blue", 0x376092),
                ("xlsx.cell-sparkline-color-red", "xlsx.sparkline-color-red", 0xC00000),
                ("xlsx.cell-sparkline-color-green", "xlsx.sparkline-color-green", 0x70AD47),
                ("xlsx.cell-sparkline-color-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                ("xlsx.cell-sparkline-color-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                ("xlsx.cell-sparkline-color-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                ("xlsx.cell-sparkline-color-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                ("xlsx.cell-sparkline-color-black", "xlsx.sparkline-color-black", 0x000000),
            ];
            let series_color_items: Vec<MenuEntry> = super::toolbar::palette_menu_with_ids(
                self,
                None,
                &CELL_SPARKLINE_COLORS,
                false,
                std::rc::Rc::new(|this, rgb, window, cx| {
                    this.set_selected_sparklines_series_color(rgb, cx);
                    this.focus_handle.focus(window, cx);
                }),
            );
            // The line's colour is marked in the palette.
            let series_color_items = match sp_group.series {
                Some(color) => crate::shell::color_palette::mark_current(series_color_items, color),
                None => series_color_items,
            };

            let point_items: Vec<MenuEntry> = [
                ("xlsx.cell-sparkline-high-point", "xlsx.sparkline-high-point", SPK::High, sp_group.high),
                ("xlsx.cell-sparkline-low-point", "xlsx.sparkline-low-point", SPK::Low, sp_group.low),
                ("xlsx.cell-sparkline-negative-points", "xlsx.sparkline-negative-points", SPK::Negative, sp_group.negative_points),
                ("xlsx.cell-sparkline-first-point", "xlsx.sparkline-first-point", SPK::First, sp_group.first),
                ("xlsx.cell-sparkline-last-point", "xlsx.sparkline-last-point", SPK::Last, sp_group.last),
                ("xlsx.cell-sparkline-markers", "xlsx.sparkline-markers", SPK::Markers, sp_group.markers),
            ]
            .into_iter()
            .map(|(id, label, pt, active)| {
                MenuItem::new(id, label)
                    .checked(active)
                    .on(move |this, window, cx| {
                        this.toggle_selected_sparklines_point(pt, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into()
            })
            .collect();

            const CELL_MC_MENUS: [(
                &str,
                &str,
                SPK,
                [(&str, &str, u32); 8],
            ); 6] = [
                (
                    "xlsx.cell-sparkline-mc-negative",
                    "xlsx.sparkline-negative-points",
                    SPK::Negative,
                    [
                        ("xlsx.cell-sparkline-mc-negative-blue", "xlsx.sparkline-color-blue", 0x376092),
                        ("xlsx.cell-sparkline-mc-negative-red", "xlsx.sparkline-color-red", 0xC00000),
                        ("xlsx.cell-sparkline-mc-negative-green", "xlsx.sparkline-color-green", 0x70AD47),
                        ("xlsx.cell-sparkline-mc-negative-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                        ("xlsx.cell-sparkline-mc-negative-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                        ("xlsx.cell-sparkline-mc-negative-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                        ("xlsx.cell-sparkline-mc-negative-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                        ("xlsx.cell-sparkline-mc-negative-black", "xlsx.sparkline-color-black", 0x000000),
                    ],
                ),
                (
                    "xlsx.cell-sparkline-mc-markers",
                    "xlsx.sparkline-markers",
                    SPK::Markers,
                    [
                        ("xlsx.cell-sparkline-mc-markers-blue", "xlsx.sparkline-color-blue", 0x376092),
                        ("xlsx.cell-sparkline-mc-markers-red", "xlsx.sparkline-color-red", 0xC00000),
                        ("xlsx.cell-sparkline-mc-markers-green", "xlsx.sparkline-color-green", 0x70AD47),
                        ("xlsx.cell-sparkline-mc-markers-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                        ("xlsx.cell-sparkline-mc-markers-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                        ("xlsx.cell-sparkline-mc-markers-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                        ("xlsx.cell-sparkline-mc-markers-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                        ("xlsx.cell-sparkline-mc-markers-black", "xlsx.sparkline-color-black", 0x000000),
                    ],
                ),
                (
                    "xlsx.cell-sparkline-mc-high",
                    "xlsx.sparkline-high-point",
                    SPK::High,
                    [
                        ("xlsx.cell-sparkline-mc-high-blue", "xlsx.sparkline-color-blue", 0x376092),
                        ("xlsx.cell-sparkline-mc-high-red", "xlsx.sparkline-color-red", 0xC00000),
                        ("xlsx.cell-sparkline-mc-high-green", "xlsx.sparkline-color-green", 0x70AD47),
                        ("xlsx.cell-sparkline-mc-high-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                        ("xlsx.cell-sparkline-mc-high-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                        ("xlsx.cell-sparkline-mc-high-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                        ("xlsx.cell-sparkline-mc-high-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                        ("xlsx.cell-sparkline-mc-high-black", "xlsx.sparkline-color-black", 0x000000),
                    ],
                ),
                (
                    "xlsx.cell-sparkline-mc-low",
                    "xlsx.sparkline-low-point",
                    SPK::Low,
                    [
                        ("xlsx.cell-sparkline-mc-low-blue", "xlsx.sparkline-color-blue", 0x376092),
                        ("xlsx.cell-sparkline-mc-low-red", "xlsx.sparkline-color-red", 0xC00000),
                        ("xlsx.cell-sparkline-mc-low-green", "xlsx.sparkline-color-green", 0x70AD47),
                        ("xlsx.cell-sparkline-mc-low-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                        ("xlsx.cell-sparkline-mc-low-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                        ("xlsx.cell-sparkline-mc-low-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                        ("xlsx.cell-sparkline-mc-low-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                        ("xlsx.cell-sparkline-mc-low-black", "xlsx.sparkline-color-black", 0x000000),
                    ],
                ),
                (
                    "xlsx.cell-sparkline-mc-first",
                    "xlsx.sparkline-first-point",
                    SPK::First,
                    [
                        ("xlsx.cell-sparkline-mc-first-blue", "xlsx.sparkline-color-blue", 0x376092),
                        ("xlsx.cell-sparkline-mc-first-red", "xlsx.sparkline-color-red", 0xC00000),
                        ("xlsx.cell-sparkline-mc-first-green", "xlsx.sparkline-color-green", 0x70AD47),
                        ("xlsx.cell-sparkline-mc-first-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                        ("xlsx.cell-sparkline-mc-first-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                        ("xlsx.cell-sparkline-mc-first-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                        ("xlsx.cell-sparkline-mc-first-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                        ("xlsx.cell-sparkline-mc-first-black", "xlsx.sparkline-color-black", 0x000000),
                    ],
                ),
                (
                    "xlsx.cell-sparkline-mc-last",
                    "xlsx.sparkline-last-point",
                    SPK::Last,
                    [
                        ("xlsx.cell-sparkline-mc-last-blue", "xlsx.sparkline-color-blue", 0x376092),
                        ("xlsx.cell-sparkline-mc-last-red", "xlsx.sparkline-color-red", 0xC00000),
                        ("xlsx.cell-sparkline-mc-last-green", "xlsx.sparkline-color-green", 0x70AD47),
                        ("xlsx.cell-sparkline-mc-last-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                        ("xlsx.cell-sparkline-mc-last-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                        ("xlsx.cell-sparkline-mc-last-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                        ("xlsx.cell-sparkline-mc-last-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                        ("xlsx.cell-sparkline-mc-last-black", "xlsx.sparkline-color-black", 0x000000),
                    ],
                ),
            ];
            let marker_color_items: Vec<MenuEntry> = CELL_MC_MENUS
                .into_iter()
                .map(|(sub_id, label_key, pt, palette)| {
                    let items: Vec<MenuEntry> = super::toolbar::palette_menu_with_ids(
                        self,
                        None,
                        &palette,
                        false,
                        std::rc::Rc::new(move |this, rgb, window, cx| {
                            this.set_selected_sparklines_point_color(pt, rgb, cx);
                            this.focus_handle.focus(window, cx);
                        }),
                    );
                    // The colour of this kind of point is marked in the palette.
                    let current = match pt {
                        SPK::High => sp_group.high_color,
                        SPK::Low => sp_group.low_color,
                        SPK::Negative => sp_group.negative,
                        SPK::First => sp_group.first_color,
                        SPK::Last => sp_group.last_color,
                        SPK::Markers => sp_group.markers_color,
                    };
                    let items = match current {
                        Some(color) => crate::shell::color_palette::mark_current(items, color),
                        None => items,
                    };
                    Submenu::new(sub_id, "palette", label_key, items).into()
                })
                .collect();

            let weight_items: Vec<MenuEntry> = [
                ("xlsx.cell-sparkline-weight-75", "0.75 pt", 75u32),
                ("xlsx.cell-sparkline-weight-100", "1.0 pt", 100u32),
                ("xlsx.cell-sparkline-weight-150", "1.5 pt", 150u32),
                ("xlsx.cell-sparkline-weight-225", "2.25 pt", 225u32),
            ]
            .into_iter()
            .map(|(id, label, w)| {
                MenuItem::new(id, label)
                    .checked(sp_group.line_weight == w)
                    .on(move |this, window, cx| {
                        this.set_selected_sparklines_line_weight(w, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into()
            })
            .collect();

            let empty_items: Vec<MenuEntry> = [
                ("xlsx.cell-sparkline-empty-gap", "_xlsx.gaps", ibis_doc_model::SparklineEmpty::Gap),
                ("xlsx.cell-sparkline-empty-zero", "_xlsx.zeros", ibis_doc_model::SparklineEmpty::Zero),
                ("xlsx.cell-sparkline-empty-span", "_xlsx.connect-data-points-with-line", ibis_doc_model::SparklineEmpty::Span),
            ]
            .into_iter()
            .map(|(id, label, empty_mode)| {
                MenuItem::new(id, label)
                    .checked(sp_group.empty == empty_mode)
                    .on(move |this, window, cx| {
                        this.set_selected_sparklines_empty(empty_mode, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into()
            })
            .collect();

            let hidden_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-sparkline-display-hidden-on", "xlsx.sparkline-display-hidden-on")
                    .checked(sp_group.display_hidden)
                    .on(|this, window, cx| {
                        this.set_selected_sparklines_display_hidden(true, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-sparkline-display-hidden-off", "xlsx.sparkline-display-hidden-off")
                    .checked(!sp_group.display_hidden)
                    .on(|this, window, cx| {
                        this.set_selected_sparklines_display_hidden(false, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];

            let sparkline_items: Vec<MenuEntry> = vec![
                MenuItem::new("xlsx.cell-sparkline-edit", "_xlsx.edit-sparkline")
                    .icon("edit")
                    .on(|this, window, cx| this.open_sparkline_panel(None, window, cx))
                    .into(),
                MenuEntry::Separator,
                MenuItem::new("xlsx.cell-sparkline-line", "xlsx.sparkline-line")
                    .checked(sp_group.kind == SK::Line)
                    .on(|this, window, cx| {
                        this.set_selected_sparklines_kind(SK::Line, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-sparkline-column", "xlsx.sparkline-column")
                    .checked(sp_group.kind == SK::Column)
                    .on(|this, window, cx| {
                        this.set_selected_sparklines_kind(SK::Column, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-sparkline-winloss", "xlsx.sparkline-winloss")
                    .checked(sp_group.kind == SK::WinLoss)
                    .on(|this, window, cx| {
                        this.set_selected_sparklines_kind(SK::WinLoss, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuEntry::Separator,
                Submenu::new("xlsx.cell-sparkline-styles", "palette", "xlsx.sparkline-styles", style_items).into(),
                Submenu::new("xlsx.cell-sparkline-colors", "format_color_text", "xlsx.sparkline-colors", series_color_items).into(),
                Submenu::new("xlsx.cell-sparkline-points", "scatter_plot", "xlsx.sparkline-points", point_items).into(),
                Submenu::new("xlsx.cell-sparkline-marker-colors", "palette", "xlsx.sparkline-marker-colors", marker_color_items).into(),
                Submenu::new("xlsx.cell-sparkline-weight", "line_weight", "xlsx.sparkline-weight", weight_items).into(),
                Submenu::new("xlsx.cell-sparkline-empty", "space_bar", "xlsx.sparkline-empty-cells", empty_items).into(),
                Submenu::new("xlsx.cell-sparkline-display-hidden", "visibility", "xlsx.sparkline-display-hidden", hidden_items).into(),
                Submenu::new(
                    "xlsx.cell-sparkline-axis",
                    "timeline",
                    "xlsx.sparkline-date-axis-menu",
                    vec![
                        MenuItem::new("xlsx.cell-sparkline-date-axis-on", "xlsx.sparkline-date-axis-on")
                            .checked(sp_group.date_axis)
                            .on(|this, window, cx| {
                                this.set_selected_sparklines_date_axis(true, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-sparkline-date-axis-off", "xlsx.sparkline-date-axis-off")
                            .checked(!sp_group.date_axis)
                            .on(|this, window, cx| {
                                this.set_selected_sparklines_date_axis(false, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuEntry::Separator,
                        MenuItem::new("xlsx.cell-sparkline-show-axis", "xlsx.sparkline-show-axis")
                            .checked(sp_group.display_x_axis)
                            .on(|this, window, cx| {
                                this.toggle_selected_sparklines_display_x_axis(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-sparkline-right-to-left", "xlsx.sparkline-right-to-left")
                            .checked(sp_group.right_to_left)
                            .on(|this, window, cx| {
                                this.toggle_selected_sparklines_right_to_left(cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuEntry::Separator,
                        MenuItem::new("xlsx.cell-sparkline-min-individual", "xlsx.sparkline-min-individual")
                            .checked(matches!(sp_group.min_axis, ibis_doc_model::SparklineAxis::Individual))
                            .on(|this, window, cx| {
                                this.set_selected_sparklines_min_axis(ibis_doc_model::SparklineAxis::Individual, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-sparkline-min-group", "xlsx.sparkline-min-group")
                            .checked(matches!(sp_group.min_axis, ibis_doc_model::SparklineAxis::Group))
                            .on(|this, window, cx| {
                                this.set_selected_sparklines_min_axis(ibis_doc_model::SparklineAxis::Group, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-sparkline-min-custom", "xlsx.sparkline-min-custom")
                            .checked(matches!(sp_group.min_axis, ibis_doc_model::SparklineAxis::Custom(_)))
                            .on(|this, window, cx| {
                                this.open_sparkline_panel(None, window, cx);
                            })
                            .into(),
                        MenuEntry::Separator,
                        MenuItem::new("xlsx.cell-sparkline-max-individual", "xlsx.sparkline-max-individual")
                            .checked(matches!(sp_group.max_axis, ibis_doc_model::SparklineAxis::Individual))
                            .on(|this, window, cx| {
                                this.set_selected_sparklines_max_axis(ibis_doc_model::SparklineAxis::Individual, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-sparkline-max-group", "xlsx.sparkline-max-group")
                            .checked(matches!(sp_group.max_axis, ibis_doc_model::SparklineAxis::Group))
                            .on(|this, window, cx| {
                                this.set_selected_sparklines_max_axis(ibis_doc_model::SparklineAxis::Group, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.cell-sparkline-max-custom", "xlsx.sparkline-max-custom")
                            .checked(matches!(sp_group.max_axis, ibis_doc_model::SparklineAxis::Custom(_)))
                            .on(|this, window, cx| {
                                this.open_sparkline_panel(None, window, cx);
                            })
                            .into(),
                    ],
                )
                .into(),
                MenuEntry::Separator,
                MenuItem::new("xlsx.cell-sparkline-group", "xlsx.group")
                    .on(|this, window, cx| {
                        this.group_selected_sparklines(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-sparkline-ungroup", "xlsx.ungroup")
                    .on(|this, window, cx| {
                        this.ungroup_selected_sparklines(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuEntry::Separator,
                MenuItem::new("xlsx.cell-sparkline-clear-selected", "xlsx.sparkline-clear-selected")
                    .icon("delete")
                    .on(|this, window, cx| {
                        this.clear_sparklines_action(false, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
                MenuItem::new("xlsx.cell-sparkline-clear-group", "xlsx.sparkline-clear-group")
                    .on(|this, window, cx| {
                        this.clear_sparklines_action(true, cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            ];
            out.push(Submenu::new("xlsx.cell-sparkline", "show_chart", "xlsx.sparklines", sparkline_items).into());
        }
        out.push(MenuEntry::Separator);
        // ドロップダウン リストから選択…: a list rule on the cell or candidates from the same column.
        if self.can_pick_from_list() {
            let shortcut_str = if crate::is_mac() { "⌥↓" } else { "Alt+↓" };
            out.push(
                MenuItem::new("xlsx.cell-pick-list", "_xlsx.pick-from-drop-down-list")
                    .icon("arrow_drop_down_circle")
                    .shortcut(shortcut_str)
                    .on(|this, window, cx| {
                        this.pick_from_drop_down_list(window, cx);
                    })
                    .into(),
            );
        }
        // スレッド形式のコメント (Threaded Comments: 新しいコメント / コメントに返信…・コメントの削除).
        if has_thread {
            out.push(
                MenuItem::new("xlsx.cell-comment-reply", "xlsx.comment-reply")
                    .icon("comment")
                    .shortcut("⌥⌘M")
                    .on(|this, window, cx| this.open_threaded_comment_panel(window, cx))
                    .into(),
            );
            out.push(
                MenuItem::new("xlsx.cell-comment-delete", "xlsx.comment-delete")
                    .icon("delete")
                    .on(|this, window, cx| {
                        if let Some((c, r)) = this.current_cell() {
                            this.delete_threaded_comment(r, c, cx);
                        }
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            );
        } else {
            out.push(
                MenuItem::new("xlsx.cell-comment-new", "xlsx.comment-new")
                    .icon("add_comment")
                    .shortcut("⌥⌘M")
                    .on(|this, window, cx| this.open_threaded_comment_panel(window, cx))
                    .into(),
            );
        }
        // メモ (Notes: 新しいメモ / メモの編集・メモの削除).
        out.push(
            MenuItem::tr(if has_note { "xlsx.note-edit" } else { "xlsx.note-new" })
                .icon("note_add")
                .shortcut("⇧F2")
                .on(|this, window, cx| this.open_note_panel(window, cx))
                .into(),
        );
        if has_note {
            out.push(
                MenuItem::tr("xlsx.note-delete")
                    .icon("delete")
                    .on(|this, window, cx| {
                        this.delete_notes(cx);
                        this.focus_handle.focus(window, cx);
                    })
                    .into(),
            );
        }
        out.push(MenuItem::new("xlsx.cell-format", "_xlsx.format-cells").icon("tune").on(|this, window, cx| this.open_custom_format(window, cx)).into());
        let has_link = self.current_link().is_some();
        if has_link {
            let mut links=Vec::new();
            links.push(
                MenuItem::new("xlsx.cell-link-open", crate::shell::i18n::text("hyperlink.open"))
                    .icon("open_in_new")
                    .on(|this, window, cx| {
                        this.follow_cell_link(window, cx);
                    })
                    .into(),
            );
            links.push(
                MenuItem::new("xlsx.cell-link-edit", crate::shell::i18n::text("hyperlink.edit"))
                    .icon("edit")
                    .shortcut("⌘K")
                    .on(|this, window, cx| {
                        this.open_link_panel(window, cx);
                    })
                    .into(),
            );
            links.push(
                MenuItem::new("xlsx.cell-link-remove", crate::shell::i18n::text("hyperlink.remove"))
                    .icon("link_off")
                    .on(|this, _, cx| {
                        this.remove_cell_link(cx);
                    })
                    .into(),
            );
            out.push(Submenu::tr("hyperlink.menu","link",links).into());
        } else {
            out.push(MenuItem::new("xlsx.cell-link", "_xlsx.hyperlink").icon("link").shortcut("⌘K").on(|this, window, cx| this.open_link_panel(window, cx)).into());
        }
        out.push(MenuItem::new("xlsx.cell-define-name", "_xlsx.define-name").on(|this, window, cx| this.open_names(window, cx)).into());
        out.push(
            Submenu::new(
                "xlsx.cell-create-names",
                "bookmark_add",
                "xlsx.create-from-selection",
                vec![
                    MenuItem::new("xlsx.cell-create-names-top", "xlsx.create-names-top-row")
                        .on(|this, window, cx| {
                            this.create_names_from_selection(true, false, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-create-names-left", "xlsx.create-names-left-col")
                        .on(|this, window, cx| {
                            this.create_names_from_selection(false, true, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-create-names-both", "xlsx.create-names-both")
                        .on(|this, window, cx| {
                            this.create_names_from_selection(true, true, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                ],
            )
            .into(),
        );
        let sheet_names: Vec<String> = self
            .presentation()
            .workbook
            .as_ref()
            .map(|w| w.sheets.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default();
        let defined_names: Vec<(String, Option<usize>)> = self
            .presentation()
            .workbook
            .as_ref()
            .map(|w| {
                w.names
                    .iter()
                    .filter(|n| !n.hidden && !n.name.starts_with("_xlnm."))
                    .take(8)
                    .map(|n| (n.name.clone(), n.sheet))
                    .collect()
            })
            .unwrap_or_default();
        if !defined_names.is_empty() {
            const USE_NAME_IDS: [&str; 8] = [
                "xlsx.cell-use-name-0",
                "xlsx.cell-use-name-1",
                "xlsx.cell-use-name-2",
                "xlsx.cell-use-name-3",
                "xlsx.cell-use-name-4",
                "xlsx.cell-use-name-5",
                "xlsx.cell-use-name-6",
                "xlsx.cell-use-name-7",
            ];
            const GOTO_NAME_IDS: [&str; 8] = [
                "xlsx.cell-goto-name-0",
                "xlsx.cell-goto-name-1",
                "xlsx.cell-goto-name-2",
                "xlsx.cell-goto-name-3",
                "xlsx.cell-goto-name-4",
                "xlsx.cell-goto-name-5",
                "xlsx.cell-goto-name-6",
                "xlsx.cell-goto-name-7",
            ];
            let mut use_items: Vec<MenuEntry> = Vec::new();
            let mut goto_items: Vec<MenuEntry> = Vec::new();
            for (idx, (name, scope)) in defined_names.into_iter().enumerate() {
                let label = match scope.and_then(|s| sheet_names.get(s)) {
                    Some(sh_name) => gpui_kit::SharedString::from(format!("{name} [{sh_name}]")),
                    None => gpui_kit::SharedString::from(name.clone()),
                };
                let use_name = name.clone();
                use_items.push(
                    MenuItem::new(USE_NAME_IDS[idx], label.clone())
                        .on(move |this, window, cx| {
                            this.use_name_in_formula(&use_name, window, cx);
                        })
                        .into(),
                );
                let goto_ref = match scope.and_then(|s| sheet_names.get(s)) {
                    Some(sh_name) => format!("{}!{name}", ibis_calc::formula::quote_sheet(sh_name)),
                    None => name,
                };
                goto_items.push(
                    MenuItem::new(GOTO_NAME_IDS[idx], label)
                        .on(move |this, window, cx| {
                            this.go_to_reference(&goto_ref, window, cx);
                        })
                        .into(),
                );
            }
            out.push(Submenu::new("xlsx.cell-use-in-formula", "functions", "xlsx.use-in-formula", use_items).into());
            out.push(Submenu::new("xlsx.cell-go-to-name", "find_replace", "_xlsx.go-to", goto_items).into());
        }
        out.extend(menus.find_entry("xlsx.conditional-menu"));
        let circles_shown = self.sheet_state().is_some_and(|s| s.validation_circles);
        out.push(
            Submenu::new(
                "xlsx.cell-validation",
                "fact_check",
                "_xlsx.data-validation",
                vec![
                    MenuItem::new("xlsx.cell-validation-open", "_xlsx.data-validation")
                        .icon("fact_check")
                        .on(|this, window, cx| this.open_validation_panel(window, cx))
                        .into(),
                    MenuItem::new("xlsx.cell-validation-clear", "_xlsx.clear-all")
                        .icon("delete")
                        .on(|this, _window, cx| this.clear_validation(cx))
                        .into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-validation-select-same", "xlsx.validation.select-same")
                        .on(|this, window, cx| {
                            this.select_validation_cells(true, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-validation-select-all", "xlsx.validation.select-all")
                        .on(|this, window, cx| {
                            this.select_validation_cells(false, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-validation-circle", "xlsx.validation.circle-invalid")
                        .checked(circles_shown)
                        .on(|this, window, cx| {
                            this.circle_invalid_data(cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-validation-clear-circles", "xlsx.validation.clear-circles")
                        .disabled(!circles_shown)
                        .on(|this, window, cx| {
                            this.clear_validation_circles(cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-validation-next-invalid", "xlsx.validation.next-invalid")
                        .on(|this, window, cx| {
                            this.jump_to_next_invalid_cell(cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                ],
            )
            .into(),
        );
        let has_arrows = !self.trace_arrows().is_empty();
        out.push(
            Submenu::new(
                "xlsx.cell-audit",
                "account_tree",
                "xlsx.formula-auditing",
                vec![
                    MenuItem::new("xlsx.cell-trace-precedents", "xlsx.trace-precedents")
                        .icon("arrow_forward")
                        .on(|this, window, cx| {
                            this.trace_precedents(cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-trace-dependents", "xlsx.trace-dependents")
                        .icon("arrow_back")
                        .on(|this, window, cx| {
                            this.trace_dependents(cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-remove-arrows", "xlsx.remove-all-arrows")
                        .icon("clear")
                        .disabled(!has_arrows)
                        .on(|this, window, cx| {
                            this.remove_all_arrows(cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-evaluate-formula", "xlsx.evaluate-formula")
                        .icon("functions")
                        .on(|this, window, cx| this.open_evaluate_formula(window, cx))
                        .into(),
                    MenuItem::new("xlsx.cell-add-watch", "_xlsx.add-watch")
                        .icon("visibility")
                        .on(|this, window, cx| {
                            this.add_watch_cell();
                            if this.sheet_state().is_none_or(|s| s.panels.watch_window.is_none()) {
                                this.open_watch_window(window, cx);
                            } else {
                                cx.notify();
                            }
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-watch-window", "xlsx.watch-window")
                        .icon("visibility")
                        .on(|this, window, cx| this.open_watch_window(window, cx))
                        .into(),
                ],
            )
            .into(),
        );
        use super::goto_special::Special as GS;
        out.push(
            Submenu::new(
                "xlsx.cell-select-special",
                "select_all",
                "xlsx.cell-select-special",
                vec![
                    MenuItem::new("xlsx.cell-select-formulas", "xlsx.select-formulas")
                        .on(|this, window, cx| {
                            this.select_special_kind(GS::Formulas, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-select-constants", "xlsx.select-constants")
                        .on(|this, window, cx| {
                            this.select_special_kind(GS::Constants, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-select-blanks", "xlsx.select-blanks")
                        .on(|this, window, cx| {
                            this.select_special_kind(GS::Blanks, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-select-visible", "xlsx.select-visible-cells")
                        .on(|this, window, cx| {
                            this.select_special_kind(GS::Visible, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-select-current-region", "xlsx.select-current-region")
                        .on(|this, window, cx| {
                            this.select_special_kind(GS::CurrentRegion, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-select-notes", "xlsx.select-notes")
                        .on(|this, window, cx| {
                            this.select_special_kind(GS::Comments, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-select-conditional", "xlsx.select-conditional-formats")
                        .on(|this, window, cx| {
                            this.select_special_kind(GS::Conditional, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-select-validation", "xlsx.select-data-validation")
                        .on(|this, window, cx| {
                            this.select_special_kind(GS::Validation, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-goto-special-dialog", "xlsx.goto-special")
                        .icon("find_replace")
                        .on(|this, _window, cx| this.open_goto_special_panel(cx))
                        .into(),
                ],
            )
            .into(),
        );
        out.push(
            Submenu::new(
                "xlsx.cell-data-tools",
                "table_chart",
                "xlsx.data-tools",
                vec![
                    MenuItem::new("xlsx.cell-text-to-columns", "xlsx.text-to-columns")
                        .icon("view_column")
                        .on(|this, window, cx| this.open_text_columns_panel(window, cx))
                        .into(),
                    MenuItem::new("xlsx.cell-flash-fill", "xlsx.flash-fill")
                        .icon("auto_fix_high")
                        .shortcut("⌃E")
                        .on(|this, window, cx| {
                            this.flash_fill_column(cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                    MenuItem::new("xlsx.cell-remove-duplicates", "xlsx.remove-duplicates")
                        .icon("delete")
                        .on(|this, window, cx| this.open_dedupe_panel(window, cx))
                        .into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-consolidate", "xlsx.consolidate")
                        .icon("table_chart")
                        .on(|this, window, cx| this.open_consolidate_panel(window, cx))
                        .into(),
                    MenuItem::new("xlsx.cell-subtotal", "xlsx.subtotal")
                        .icon("functions")
                        .on(|this, window, cx| this.open_subtotal_panel(window, cx))
                        .into(),
                ],
            )
            .into(),
        );
        out.push(
            Submenu::new(
                "xlsx.cell-what-if",
                "functions",
                "xlsx.what-if-analysis",
                vec![
                    MenuItem::new("xlsx.cell-scenario-manager", "xlsx.scenario-manager")
                        .on(|this, window, cx| this.open_scenario_manager(window, cx))
                        .into(),
                    MenuItem::new("xlsx.cell-goal-seek", "xlsx.goal-seek")
                        .on(|this, window, cx| this.open_goal_seek(window, cx))
                        .into(),
                    MenuItem::new("xlsx.cell-data-table", "xlsx.data-table")
                        .on(|this, window, cx| this.open_data_table(window, cx))
                        .into(),
                    MenuEntry::Separator,
                    MenuItem::new("xlsx.cell-forecast-sheet", "xlsx.forecast-sheet")
                        .icon("show_chart")
                        .on(|this, window, cx| this.open_forecast_panel(window, cx))
                        .into(),
                    MenuItem::new("xlsx.cell-solver", "xlsx.solver")
                        .icon("functions")
                        .on(|this, window, cx| this.open_solver_panel(window, cx))
                        .into(),
                    MenuItem::new("xlsx.cell-data-analysis", "xlsx.data-analysis")
                        .icon("functions")
                        .on(|this, window, cx| this.open_analysis_panel(window, cx))
                        .into(),
                ],
            )
            .into(),
        );
        out
    }

    /// The right button on the tab of sheet `id`: the sheet is shown, and
    /// its menu opens (Excel for Mac's: 挿入, 削除, 名前の変更, 移動または
    /// コピー, シートの保護, タブの色, 非表示, 再表示).
    pub(crate) fn on_tab_right_down(&mut self, id: ibis_doc_model::SlideId, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.current_slide != id {
            self.select_slide(id, window, cx);
        }
        let entries = self.tab_menu();
        self.open_popup_menu(entries, event.position, window, cx);
    }

    /// The sheet tab's menu.
    pub(crate) fn tab_menu(&self) -> Vec<MenuEntry> {
        let menus = self.menus();
        let is_protected = self.current_sheet().is_some_and(|(_, s)| s.protection.is_some());
        let grid_lines = self.current_sheet().is_some_and(|(_, s)| s.grid_lines);
        let headings = self.current_sheet().map_or(true, |(_, s)| s.headings);
        let mut out: Vec<MenuEntry> = Vec::new();
        out.extend(menus.find_entry("xlsx.insert-sheet"));
        out.extend(menus.find_entry("xlsx.delete-sheet"));
        out.extend(menus.find_entry("xlsx.rename-sheet"));
        out.extend(menus.find_entry("xlsx.duplicate-sheet"));
        out.extend(menus.find_entry("xlsx.move-sheet-left"));
        out.extend(menus.find_entry("xlsx.move-sheet-right"));
        out.extend(menus.find_entry("xlsx.move-sheet-first"));
        out.extend(menus.find_entry("xlsx.move-sheet-last"));
        out.push(MenuEntry::Separator);
        out.extend(menus.find_entry("xlsx.view-code"));
        out.push(
            MenuItem::new("xlsx.tab-protect", "_xlsx.protect-sheet")
                .icon("lock")
                .checked(is_protected)
                .on(|this, window, cx| this.open_protect_panel(window, cx))
                .into(),
        );
        out.extend(menus.find_entry("xlsx.tab-colors"));
        out.push(MenuEntry::Separator);
        out.extend(menus.find_entry("xlsx.hide-sheet"));
        out.extend(
            menus
                .find_entry("xlsx.unhide-sheets")
                .or_else(|| menus.find_entry("xlsx.unhide-sheet")),
        );
        out.push(MenuEntry::Separator);
        out.push(
            MenuItem::new("xlsx.tab-gridlines", "xlsx.gridlines")
                .checked(grid_lines)
                .on(|this, window, cx| {
                    let Some((_, sheet)) = this.current_sheet() else { return };
                    let (slide, shown) = (this.current_slide, !sheet.grid_lines);
                    this.run_sheet(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetGridLines { slide, shown })], cx);
                    this.focus_handle.focus(window, cx);
                })
                .into(),
        );
        out.push(
            MenuItem::new("xlsx.tab-headings", "xlsx.headings")
                .checked(headings)
                .on(|this, window, cx| {
                    let Some((_, sheet)) = this.current_sheet() else { return };
                    let (slide, shown) = (this.current_slide, !sheet.headings);
                    this.run_sheet(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetHeadings { slide, shown })], cx);
                    this.focus_handle.focus(window, cx);
                })
                .into(),
        );
        out
    }

    /// The right button on a shape (a chart, picture or drawing) over the
    /// sheet: it is selected (unless it is among those selected) and its menu
    /// opens: 切り取り, コピー, 貼り付け, 削除, 順序, グループ化, 代替テキスト,
    /// サイズとプロパティ.
    pub(crate) fn on_sheet_shape_right_down(&mut self, shape: ibis_doc_model::ShapeId, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.commit_edit(window, cx);
        self.focus_handle.focus(window, cx);
        if !self.selected_ids().contains(&shape) {
            self.set_selection(vec![shape]);
        }
        let entries = self.shape_menu();
        self.open_popup_menu(entries, event.position, window, cx);
        cx.notify();
    }

    /// The menu of a shape over the sheet.
    pub(crate) fn shape_menu(&self) -> Vec<MenuEntry> {
        let menus = self.menus();
        let mut out: Vec<MenuEntry> = Vec::new();
        for id in ["edit.cut", "edit.copy", "edit.paste"] {
            out.extend(menus.find_entry(id));
        }
        out.extend(menus.find_entry("edit.delete"));
        out.push(MenuEntry::Separator);

        // A SmartArt diagram: its shapes, layout and colours (a slide's menu, #XLSX-OBJECT-017); its text pane is a slide's.
        if self.can_modify_smartart_items() {
            out.extend(self.smartart_shape_menu_entries().into_iter().filter(|e| !matches!(e, MenuEntry::Item(i) if i.id == "pptx.smartart.text-pane")));
            out.push(MenuEntry::Separator);
        }

        if let Some(shape_id) = self.selected_sheet_shape() {
            if let Some(shape) = self.slide().shapes.iter().find(|s| s.id == shape_id) {
                if matches!(shape.kind, ibis_doc_model::ShapeKind::Picture(_)) {
                    let is_in_cell = self.current_sheet().is_some_and(|(_, sheet)| {
                        let xs = sheet.column_offsets();
                        let ys = sheet.row_offsets();
                        let cx_pos = shape.frame.x + shape.frame.width / 2;
                        let cy_pos = shape.frame.y + shape.frame.height / 2;
                        sheet.cell_images.iter().any(|img| {
                            let col = img.column as usize;
                            let r = img.row as usize;
                            col + 1 < xs.len() && r + 1 < ys.len()
                                && (xs[col]..=xs[col + 1]).contains(&cx_pos)
                                && (ys[r]..=ys[r + 1]).contains(&cy_pos)
                        })
                    });
                    if is_in_cell {
                        out.push(MenuItem::new("xlsx.place-over-cells", "_xlsx.place-over-cells").icon("filter_none").on(move |this, _, cx| {
                            this.place_picture_over_cells(shape_id, cx);
                        }).into());
                    } else {
                        out.push(MenuItem::new("xlsx.place-in-cell", "_xlsx.place-in-cell").icon("table_rows").on(move |this, _, cx| {
                            this.place_picture_in_cell(shape_id, cx);
                        }).into());
                    }
                    out.push(MenuEntry::Separator);
                    out.extend(menus.find_entry("xlsx.picture.remove-background"));
                    out.extend(menus.find_entry("xlsx.picture.color"));
                    out.extend(menus.find_entry("xlsx.picture.correction"));
                    out.extend(menus.find_entry("xlsx.picture.reset"));
                    out.push(MenuEntry::Separator);
                } else if matches!(shape.kind, ibis_doc_model::ShapeKind::Chart(_)) {
                    for id in [
                        "xlsx.chart-format",
                        "xlsx.chart-type",
                        "xlsx.chart-switch-row-column",
                        "xlsx.chart-styles",
                        "xlsx.chart-palettes",
                        "xlsx.chart-quick-layout",
                        "xlsx.chart-data-table",
                        "xlsx.chart-trendline",
                        "xlsx.chart-error-bars",
                        "xlsx.move-chart",
                        "xlsx.chart.save-template",
                        "xlsx.chart.apply-template",
                    ] {
                        out.extend(menus.find_entry(id));
                    }
                    out.push(MenuEntry::Separator);
                } else if let ibis_doc_model::ShapeKind::Slicer(slicer) = &shape.kind {
                    let all_selected = slicer.items.iter().all(|i| i.selected);
                    let slicer_clone = (**slicer).clone();
                    let label = crate::shell::i18n::format(
                        "_xlsx.clear-filter-from-name",
                        &[("name", &slicer.caption)],
                    );
                    out.push(
                        MenuItem::new("xlsx.slicer-clear-filter", label)
                            .icon("filter_alt_off")
                            .disabled(all_selected)
                            .on(move |this, _, cx| {
                                let all = vec![true; slicer_clone.items.len()];
                                this.select_slicer_items(&slicer_clone, &all, cx);
                            })
                            .into(),
                    );
                    out.push(
                        MenuItem::new("xlsx.slicer-sort-asc", "_xlsx.sort-ascending")
                            .icon("arrow_upward")
                            .checked(!slicer.descending)
                            .on(move |this, window, cx| {
                                this.update_slicer(shape_id, |s| s.descending = false, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                    );
                    out.push(
                        MenuItem::new("xlsx.slicer-sort-desc", "_xlsx.sort-descending")
                            .icon("arrow_downward")
                            .checked(slicer.descending)
                            .on(move |this, window, cx| {
                                this.update_slicer(shape_id, |s| s.descending = true, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                    );
                    let cur_show_caption = slicer.show_caption;
                    out.push(
                        MenuItem::new("xlsx.slicer-show-caption", "xlsx.slicer-settings.show-caption")
                            .checked(cur_show_caption)
                            .on(move |this, window, cx| {
                                this.update_slicer(shape_id, |s| s.show_caption = !s.show_caption, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                    );
                    let cur_cols = slicer.columns.max(1);
                    let col_items: Vec<MenuEntry> = [
                        (1u32, "xlsx.slicer-columns-1", "1"),
                        (2u32, "xlsx.slicer-columns-2", "2"),
                        (3u32, "xlsx.slicer-columns-3", "3"),
                        (4u32, "xlsx.slicer-columns-4", "4"),
                        (5u32, "xlsx.slicer-columns-5", "5"),
                    ]
                    .into_iter()
                    .map(|(cols, item_id, label)| {
                        MenuItem::new(item_id, label)
                            .checked(cur_cols == cols)
                            .on(move |this, window, cx| {
                                this.update_slicer(shape_id, move |s| s.columns = cols, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into()
                    })
                    .collect();
                    out.push(
                        Submenu::new(
                            "xlsx.slicer-columns",
                            "view_column",
                            "xlsx.slicer-settings.columns",
                            col_items,
                        )
                        .into(),
                    );
                    let cur_style = if slicer.style.is_empty() {
                        "SlicerStyleLight1"
                    } else {
                        slicer.style.as_str()
                    };
                    let style_items: Vec<MenuEntry> = [
                        ("xlsx.slicer-style-SlicerStyleLight1", "SlicerStyleLight1", "Light 1"),
                        ("xlsx.slicer-style-SlicerStyleLight2", "SlicerStyleLight2", "Light 2"),
                        ("xlsx.slicer-style-SlicerStyleLight3", "SlicerStyleLight3", "Light 3"),
                        ("xlsx.slicer-style-SlicerStyleLight4", "SlicerStyleLight4", "Light 4"),
                        ("xlsx.slicer-style-SlicerStyleLight5", "SlicerStyleLight5", "Light 5"),
                        ("xlsx.slicer-style-SlicerStyleLight6", "SlicerStyleLight6", "Light 6"),
                        ("xlsx.slicer-style-SlicerStyleDark1", "SlicerStyleDark1", "Dark 1"),
                        ("xlsx.slicer-style-SlicerStyleDark2", "SlicerStyleDark2", "Dark 2"),
                        ("xlsx.slicer-style-SlicerStyleDark3", "SlicerStyleDark3", "Dark 3"),
                        ("xlsx.slicer-style-SlicerStyleDark4", "SlicerStyleDark4", "Dark 4"),
                        ("xlsx.slicer-style-SlicerStyleDark5", "SlicerStyleDark5", "Dark 5"),
                        ("xlsx.slicer-style-SlicerStyleDark6", "SlicerStyleDark6", "Dark 6"),
                        ("xlsx.slicer-style-SlicerStyleOther1", "SlicerStyleOther1", "Other 1"),
                        ("xlsx.slicer-style-SlicerStyleOther2", "SlicerStyleOther2", "Other 2"),
                    ]
                    .into_iter()
                    .map(|(item_id, style_id, style_label)| {
                        MenuItem::new(item_id, style_label)
                            .checked(cur_style == style_id)
                            .on(move |this, window, cx| {
                                this.update_slicer(
                                    shape_id,
                                    move |s| s.style = style_id.to_string(),
                                    cx,
                                );
                                this.focus_handle.focus(window, cx);
                            })
                            .into()
                    })
                    .collect();
                    out.push(
                        Submenu::new(
                            "xlsx.slicer-styles",
                            "palette",
                            "xlsx.slicer-settings.style",
                            style_items,
                        )
                        .into(),
                    );
                    let candidate_pivots = self.candidate_slicer_pivots(slicer);
                    if !candidate_pivots.is_empty() {
                        const SLICER_CONN_IDS: [&str; 8] = [
                            "xlsx.slicer-conn-0",
                            "xlsx.slicer-conn-1",
                            "xlsx.slicer-conn-2",
                            "xlsx.slicer-conn-3",
                            "xlsx.slicer-conn-4",
                            "xlsx.slicer-conn-5",
                            "xlsx.slicer-conn-6",
                            "xlsx.slicer-conn-7",
                        ];
                        let conn_items: Vec<MenuEntry> = candidate_pivots
                            .into_iter()
                            .enumerate()
                            .map(|(idx, (p_name, is_conn))| {
                                let p_name_click = p_name.clone();
                                let item_id = SLICER_CONN_IDS.get(idx).copied().unwrap_or("xlsx.slicer-conn-n");
                                MenuItem::new(item_id, gpui_kit::SharedString::from(p_name))
                                    .checked(is_conn)
                                    .on(move |this, window, cx| {
                                        let p_name_click = p_name_click.clone();
                                        this.update_slicer(
                                            shape_id,
                                            move |s| {
                                                if let ibis_doc_model::slicer::SlicerSource::Pivot { pivots, .. } =
                                                    &mut s.source
                                                {
                                                    if let Some(pos) = pivots.iter().position(|n| *n == p_name_click) {
                                                        if pivots.len() > 1 {
                                                            pivots.remove(pos);
                                                        }
                                                    } else {
                                                        pivots.push(p_name_click);
                                                    }
                                                }
                                            },
                                            cx,
                                        );
                                        this.focus_handle.focus(window, cx);
                                    })
                                    .into()
                            })
                            .collect();
                        out.push(
                            Submenu::new(
                                "xlsx.slicer-connections",
                                "link",
                                "xlsx.report-connections",
                                conn_items,
                            )
                            .into(),
                        );
                    }
                    out.extend(menus.find_entry("xlsx.slicer-settings"));
                    out.push(MenuEntry::Separator);
                } else if let ibis_doc_model::ShapeKind::Timeline(timeline) = &shape.kind {
                    let is_cleared = timeline.selection.is_none();
                    let timeline_clone = (**timeline).clone();
                    let label = crate::shell::i18n::format(
                        "_xlsx.clear-filter-from-name",
                        &[("name", &timeline.caption)],
                    );
                    out.push(
                        MenuItem::new("xlsx.timeline-clear-filter", label)
                            .icon("filter_alt_off")
                            .disabled(is_cleared)
                            .on(move |this, _, cx| {
                                let mut next = timeline_clone.clone();
                                next.selection = None;
                                this.apply_timeline(this.current_slide, shape_id, next, cx);
                            })
                            .into(),
                    );
                    let current_level = timeline.selection_level;
                    let mut level_items: Vec<MenuEntry> = Vec::new();
                    for (lvl, id) in [
                        (0u8, "xlsx.timeline.level.years"),
                        (1u8, "xlsx.timeline.level.quarters"),
                        (2u8, "xlsx.timeline.level.months"),
                        (3u8, "xlsx.timeline.level.days"),
                    ] {
                        let t_clone = (**timeline).clone();
                        level_items.push(
                            MenuItem::tr(id)
                                .checked(current_level == lvl)
                                .on(move |this, _, cx| {
                                    let mut next = t_clone.clone();
                                    next.level = lvl;
                                    next.selection_level = lvl;
                                    this.apply_timeline(this.current_slide, shape_id, next, cx);
                                })
                                .into(),
                        );
                    }
                    out.push(
                        Submenu::new(
                            "xlsx.timeline-level",
                            "timeline",
                            crate::shell::i18n::text("_xlsx.units"),
                            level_items,
                        )
                        .into(),
                    );
                    let t_hdr = (**timeline).clone();
                    let t_sel = (**timeline).clone();
                    let t_lvl = (**timeline).clone();
                    let t_scr = (**timeline).clone();
                    let show_items: Vec<MenuEntry> = vec![
                        MenuItem::new("xlsx.timeline-show-header", "xlsx.timeline.show-header")
                            .checked(timeline.show_header)
                            .on(move |this, window, cx| {
                                let mut next = t_hdr.clone();
                                next.show_header = !next.show_header;
                                this.apply_timeline(this.current_slide, shape_id, next, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new(
                            "xlsx.timeline-show-selection-label",
                            "xlsx.timeline.show-selection-label",
                        )
                        .checked(timeline.show_selection_label)
                        .on(move |this, window, cx| {
                            let mut next = t_sel.clone();
                            next.show_selection_label = !next.show_selection_label;
                            this.apply_timeline(this.current_slide, shape_id, next, cx);
                            this.focus_handle.focus(window, cx);
                        })
                        .into(),
                        MenuItem::new("xlsx.timeline-show-time-level", "xlsx.timeline.show-time-level")
                            .checked(timeline.show_time_level)
                            .on(move |this, window, cx| {
                                let mut next = t_lvl.clone();
                                next.show_time_level = !next.show_time_level;
                                this.apply_timeline(this.current_slide, shape_id, next, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                        MenuItem::new("xlsx.timeline-show-scroll-bar", "xlsx.timeline.show-scroll-bar")
                            .checked(timeline.show_scroll_bar)
                            .on(move |this, window, cx| {
                                let mut next = t_scr.clone();
                                next.show_scroll_bar = !next.show_scroll_bar;
                                this.apply_timeline(this.current_slide, shape_id, next, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into(),
                    ];
                    out.push(
                        Submenu::new(
                            "xlsx.timeline-show",
                            "visibility",
                            "_xlsx.show",
                            show_items,
                        )
                        .into(),
                    );
                    let all_pivots: Vec<String> = self
                        .presentation()
                        .slides
                        .iter()
                        .filter_map(ibis_doc_model::sheet_calc::sheet_of_slide)
                        .flat_map(|s| s.pivots.iter().map(|p| p.name.clone()))
                        .collect();
                    if !all_pivots.is_empty() {
                        const TIMELINE_CONN_IDS: [&str; 8] = [
                            "xlsx.timeline-conn-0",
                            "xlsx.timeline-conn-1",
                            "xlsx.timeline-conn-2",
                            "xlsx.timeline-conn-3",
                            "xlsx.timeline-conn-4",
                            "xlsx.timeline-conn-5",
                            "xlsx.timeline-conn-6",
                            "xlsx.timeline-conn-7",
                        ];
                        let conn_items: Vec<MenuEntry> = all_pivots
                            .into_iter()
                            .enumerate()
                            .map(|(idx, p_name)| {
                                let is_conn = timeline.pivots.contains(&p_name);
                                let t_conn = (**timeline).clone();
                                let p_name_click = p_name.clone();
                                let item_id = TIMELINE_CONN_IDS.get(idx).copied().unwrap_or("xlsx.timeline-conn-n");
                                MenuItem::new(item_id, gpui_kit::SharedString::from(p_name))
                                    .checked(is_conn)
                                    .on(move |this, window, cx| {
                                        let mut next = t_conn.clone();
                                        if let Some(pos) = next.pivots.iter().position(|n| *n == p_name_click) {
                                            if next.pivots.len() > 1 {
                                                next.pivots.remove(pos);
                                            }
                                        } else {
                                            next.pivots.push(p_name_click.clone());
                                        }
                                        this.apply_timeline(this.current_slide, shape_id, next, cx);
                                        this.focus_handle.focus(window, cx);
                                    })
                                    .into()
                            })
                            .collect();
                        out.push(
                            Submenu::new(
                                "xlsx.timeline-connections",
                                "link",
                                "xlsx.report-connections",
                                conn_items,
                            )
                            .into(),
                        );
                    }
                    let cur_style = if timeline.style.is_empty() {
                        "TimeSlicerStyleLight1"
                    } else {
                        timeline.style.as_str()
                    };
                    let style_items: Vec<MenuEntry> = [
                        ("xlsx.timeline-style-TimeSlicerStyleLight1", "TimeSlicerStyleLight1", "Light 1"),
                        ("xlsx.timeline-style-TimeSlicerStyleLight2", "TimeSlicerStyleLight2", "Light 2"),
                        ("xlsx.timeline-style-TimeSlicerStyleLight3", "TimeSlicerStyleLight3", "Light 3"),
                        ("xlsx.timeline-style-TimeSlicerStyleLight4", "TimeSlicerStyleLight4", "Light 4"),
                        ("xlsx.timeline-style-TimeSlicerStyleLight5", "TimeSlicerStyleLight5", "Light 5"),
                        ("xlsx.timeline-style-TimeSlicerStyleLight6", "TimeSlicerStyleLight6", "Light 6"),
                        ("xlsx.timeline-style-TimeSlicerStyleDark1", "TimeSlicerStyleDark1", "Dark 1"),
                        ("xlsx.timeline-style-TimeSlicerStyleDark2", "TimeSlicerStyleDark2", "Dark 2"),
                        ("xlsx.timeline-style-TimeSlicerStyleDark3", "TimeSlicerStyleDark3", "Dark 3"),
                        ("xlsx.timeline-style-TimeSlicerStyleDark4", "TimeSlicerStyleDark4", "Dark 4"),
                        ("xlsx.timeline-style-TimeSlicerStyleDark5", "TimeSlicerStyleDark5", "Dark 5"),
                        ("xlsx.timeline-style-TimeSlicerStyleDark6", "TimeSlicerStyleDark6", "Dark 6"),
                    ]
                    .into_iter()
                    .map(|(item_id, style_id, style_label)| {
                        let t_style = (**timeline).clone();
                        MenuItem::new(item_id, style_label)
                            .checked(cur_style == style_id)
                            .on(move |this, window, cx| {
                                let mut next = t_style.clone();
                                next.style = style_id.to_string();
                                this.apply_timeline(this.current_slide, shape_id, next, cx);
                                this.focus_handle.focus(window, cx);
                            })
                            .into()
                    })
                    .collect();
                    out.push(
                        Submenu::new(
                            "xlsx.timeline-styles",
                            "palette",
                            "xlsx.slicer-settings.style",
                            style_items,
                        )
                        .into(),
                    );
                    out.push(MenuEntry::Separator);
                } else if matches!(shape.kind, ibis_doc_model::ShapeKind::Control(_)) {
                    for id in [
                        "xlsx.control-properties",
                        "xlsx.design-mode",
                        "xlsx.view-code",
                    ] {
                        out.extend(menus.find_entry(id));
                    }
                    out.push(MenuEntry::Separator);
                } else if matches!(
                    shape.kind,
                    ibis_doc_model::ShapeKind::Auto(_) | ibis_doc_model::ShapeKind::TextBox(_)
                ) {
                    for id in [
                        "xlsx.change-shape",
                        "xlsx.shape-textlink",
                        "xlsx.shape-fill",
                        "xlsx.shape-stroke",
                        "xlsx.shape-effect",
                        "xlsx.text-format",
                        "xlsx.text-align",
                    ] {
                        out.extend(menus.find_entry(id));
                    }
                    out.push(MenuEntry::Separator);
                }
            }
        }

        for id in ["xlsx.order", "xlsx.group-shapes", "xlsx.alt-text", "xlsx.frame-settings"] {
            out.extend(menus.find_entry(id));
        }
        out
    }

    /// フィルター > 選択したセルの値（色）でフィルター: the table (or the sheet's
    /// filter, made if there is none) shows the rows whose cell in this
    /// column shows what the selected cell shows.
    pub(crate) fn filter_by_selected(&mut self, cx: &mut Context<Self>, make: impl Fn(&str, &ibis_doc_model::CellStyle) -> ColumnFilter) {
        let Some((column, row)) = self.current_cell() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let text = sheet.cell(row, column).map(|c| c.text.clone()).unwrap_or_default();
        let style = sheet.cell(row, column).and_then(|c| sheet.styles.get(c.style as usize)).cloned().unwrap_or_default();
        let filter = make(&text, &style);
        let in_table = sheet.tables.iter().position(|t| t.filter.is_some() && (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column));
        let target = match (in_table, &sheet.auto_filter) {
            (Some(i), _) => Some(super::filter::FilterTarget::Table(i)),
            (None, Some(f)) if (f.range.first_column..=f.range.last_column).contains(&column) => Some(super::filter::FilterTarget::Sheet),
            _ => None,
        };
        let target = match target {
            Some(t) => t,
            None => {
                // No filter over this cell yet: made on the table around it, as フィルター does.
                self.toggle_auto_filter(cx);
                let Some((_, sheet)) = self.current_sheet() else { return };
                match sheet.tables.iter().position(|t| t.filter.is_some() && (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column)) {
                    Some(i) => super::filter::FilterTarget::Table(i),
                    None => super::filter::FilterTarget::Sheet,
                }
            }
        };
        self.filter_column_in(target, column, Some(filter), cx);
    }

    pub(crate) fn filter_by_selected_icon(&mut self, cx: &mut Context<Self>) {
        let Some((column, row)) = self.current_cell() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some((icon_set, icon_id)) = ibis_doc_model::sheet_command::cell_icon(sheet, row, column) else {
            return;
        };
        self.filter_by_selected(cx, move |_, _| ColumnFilter::Icon {
            icon_set: icon_set.clone(),
            icon_id,
        });
    }
}
