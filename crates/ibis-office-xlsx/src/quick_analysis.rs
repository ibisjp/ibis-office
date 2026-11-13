// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! クイック分析: a button just past the corner of a selected range of cells with data; its menu is
//! Excel's five tabs, 書式設定・グラフ・合計・テーブル・スパークライン, each with the commands the ribbon has
//! for them, applied to the selected range.

use gpui_kit::*;
use ibis_doc_model::{CellRange, SparklineKind};

use crate::shell::chrome::{MenuEntry, MenuItem, Submenu, fill_menu};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(QuickAnalysis)]
impl SlideEditor {
    /// The selected range, when クイック分析 is offered for it: more than one cell, one range, some of it with data,
    /// and no cell being typed in.
    #[allow(dead_code)]
    pub(crate) fn quick_analysis_range(&self) -> Option<CellRange> {
        if self.cell_edit().is_some() || self.selected_ranges().len() != 1 {
            return None;
        }
        let range = self.selected_range()?;
        if (range.first_row, range.first_column) == (range.last_row, range.last_column) {
            return None;
        }
        let (_, sheet) = self.current_sheet()?;
        // A whole row or column selected would be a million cells to look through: only the cells the sheet has.
        let has_data = sheet.cells.iter().any(|c| {
            (range.first_row..=range.last_row).contains(&c.row) && (range.first_column..=range.last_column).contains(&c.column) && !c.value.is_empty()
        });
        has_data.then_some(range)
    }

    /// Open the クイック分析 menu at `position`.
    #[allow(dead_code)]
    pub(crate) fn open_quick_analysis(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let t = |id: &str| crate::shell::i18n::text(id);
        let total = |id: &'static str, label: &str, function: &'static str| -> MenuEntry {
            MenuItem::new(id, t(label)).on(move |this, _, cx| this.auto_function(function, cx)).into()
        };
        let spark = |id: &'static str, kind: SparklineKind| -> MenuEntry {
            MenuItem::tr(id).on(move |this, window, cx| this.open_sparkline_panel(Some(kind), window, cx)).into()
        };
        let formatting = match super::conditional_quick::menu() {
            MenuEntry::Submenu(s) => s.items,
            other => vec![other],
        };
        let entries = std::rc::Rc::new(vec![
            Submenu::tr("xlsx.qa.formatting", "rule", formatting).into(),
            Submenu::tr("xlsx.qa.charts", "bar_chart", super::chart_insert::chart_items()).into(),
            Submenu::tr(
                "xlsx.qa.totals",
                "functions",
                vec![
                    total("xlsx.qa.sum", "_xlsx.sum", "SUM"),
                    total("xlsx.qa.average", "_xlsx.average", "AVERAGE"),
                    total("xlsx.qa.count", "_xlsx.count-numbers", "COUNT"),
                    total("xlsx.qa.max", "_xlsx.max", "MAX"),
                    total("xlsx.qa.min", "_xlsx.min-2", "MIN"),
                ],
            )
            .into(),
            Submenu::tr(
                "xlsx.qa.tables",
                "table_chart",
                vec![
                    MenuItem::tr("xlsx.insert-table").on(|this, window, cx| this.open_table_dialog(window, cx)).into(),
                    MenuItem::tr("xlsx.pivot-insert").on(|this, window, cx| this.open_create_pivot(window, cx)).into(),
                ],
            )
            .into(),
            Submenu::tr(
                "xlsx.qa.sparklines",
                "polyline",
                vec![
                    spark("xlsx.sparkline-line", SparklineKind::Line),
                    spark("xlsx.sparkline-column", SparklineKind::Column),
                    spark("xlsx.sparkline-winloss", SparklineKind::WinLoss),
                ],
            )
            .into(),
        ]);
        let editor = cx.entity().downgrade();
        let menu = gpui_kit::component::menu::PopupMenu::build(window, cx, move |menu, window, cx| fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(s) = this.sheet_state_mut() {
                s.list_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.wb_mut().list_popup = Some(super::validation::ListPopup { menu, position, anchor: Anchor::TopLeft, _dismiss: dismiss });
        cx.notify();
    }
}
