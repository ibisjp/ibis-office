// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pivot tables (ピボットテーブル) in the editor: the columns of a table
//! built on opening fitted to it, as Excel fits them when it builds one.

use std::sync::Arc;

use gpui_kit::{Context, TextSystem};
use crate::shell::palette::rgb;
use ibis_doc_model::{CellStyle, Command, EMU_PER_POINT, Presentation, Shape, ShapeId, ShapeKind, Sheet, SheetCommand, SlideId};

use crate::render::shape_view::Resources;
use crate::shell::chrome::{MenuEntry, MenuItem};
use crate::shell::editor::SlideEditor;

/// The room Excel leaves beside a fitted pivot column's widest text, and
/// for a pivot button (▼), in points (measured against Excel's widths of
/// XPVT-01 and 02: "第1四半期" in 62 pt, "行ラベル" with its button in 71).
const PADDING: f32 = 12.0;
const BUTTON: f32 = 15.0;

/// A text's width as Excel measures it for fitting, in points: the drawn
/// width scaled to Excel's digit, and its indent.
pub(crate) fn excel_width(text: &str, style: &CellStyle, default_size: f32, res: &Resources, ts: &TextSystem) -> f32 {
    let digit = (default_size * 0.55).round().max(1.0);
    let size = style.font_size as f32 / 100.0;
    let excel_digit = digit * size / default_size;
    let drawn_digit = super::sheet_view::text_width_pt("0", style, res, ts).max(0.1);
    let width = super::sheet_view::text_width_pt(text, style, res, ts) * excel_digit / drawn_digit;
    let italic = if style.italic { 2.0 * size / 12.0 } else { 0.0 };
    width + italic + style.indent as f32 * 3.0 * excel_digit
}

/// The columns of each pivot table built on opening (`refreshOnLoad`)
/// fitted to its cells, as Excel does after building one: the widest text
/// of the column within the table and its report filters (not the corner
/// caption), with room for a pivot button. The style's bold is not counted
/// (Excel measures the cells' own font).
pub(crate) fn fit_built_pivots(presentation: &mut Presentation, res: &Resources, ts: &TextSystem) {
    let default_size = presentation.workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
    for slide in &mut presentation.slides {
        for shape in &mut slide.shapes {
            let ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
            if !sheet.pivots.iter().any(|p| p.refresh_on_load) {
                continue;
            }
            let widths = fitted(sheet, default_size, res, ts);
            let sheet = Arc::make_mut(sheet);
            set_fitted_widths(sheet, widths);
        }
    }
}

/// Puts the widths a pivot table was fitted to in the sheet. They are written to the file (Excel saves them, with `customWidth`),
/// so the sheet marks them as the file's own and a drawing anchored to a cell stays where it was.
fn set_fitted_widths(sheet: &mut Sheet, widths: Vec<(u32, ibis_doc_model::Emu)>) {
    for (column, width) in widths {
        if let Some(w) = sheet.columns.get_mut(column as usize)
            && *w > 0
        {
            *w = width;
            sheet.custom_columns.insert(column);
        }
    }
}

fn fitted(sheet: &Sheet, default_size: f32, res: &Resources, ts: &TextSystem) -> Vec<(u32, ibis_doc_model::Emu)> {
    let built: Vec<usize> = (0..sheet.pivots.len()).filter(|&i| sheet.pivots[i].refresh_on_load).collect();
    fitted_for(sheet, &built, default_size, res, ts)
}

fn fitted_for(sheet: &Sheet, which: &[usize], default_size: f32, res: &Resources, ts: &TextSystem) -> Vec<(u32, ibis_doc_model::Emu)> {
    let mut out = Vec::new();
    for p in which.iter().filter_map(|&i| sheet.pivots.get(i)) {
        let loc = p.location;
        let pads: std::collections::HashMap<(u32, u32), f32> = p.label_pads().into_iter().map(|(r, c, pt)| ((r, c), pt)).collect();
        let pages = p.page_fields.len() as u32;
        let top = if pages > 0 { loc.first_row.saturating_sub(pages + 1) } else { loc.first_row };
        for column in loc.first_column..=loc.last_column {
            let mut widest: f32 = 0.0;
            for cell in sheet.cells.iter().filter(|c| c.column == column && (top..=loc.last_row).contains(&c.row) && !c.text.is_empty()) {
                if p.corner == Some((cell.row, cell.column)) {
                    continue;
                }
                let mut style = sheet.styles[cell.style as usize].clone();
                style.bold = false;
                let button = if p.buttons.contains(&(cell.row, cell.column)) { BUTTON } else { 0.0 };
                // A row label's own indent in place of its cell's.
                let pad = pads.get(&(cell.row, cell.column)).copied();
                if pad.is_some() {
                    style.indent = 0;
                }
                let button = button + pad.unwrap_or(0.0);
                widest = widest.max(excel_width(&cell.text, &style, default_size, res, ts) + button);
            }
            // Excel keeps the first label column of a table not in コンパクト
            // form (XPVT-03 and 04 keep column A's width).
            let compact = p.row_fields.first().is_none_or(|&f| p.fields.get(f.max(0) as usize).is_none_or(|f| f.compact && f.outline));
            if column == loc.first_column && !compact {
                continue;
            }
            if widest > 0.0 {
                // Whole points, as Excel's widths of whole pixels.
                let pt = (widest + PADDING).ceil();
                out.push((column, (pt * EMU_PER_POINT as f32) as ibis_doc_model::Emu));
            }
        }
    }
    out
}

impl SlideEditor {
    /// The pivot table the selected cell is in (or in its report filters).
    pub(crate) fn pivot_at_selection(&self) -> Option<usize> {
        let (_, sheet) = self.current_sheet()?;
        let (column, row) = self.current_cell()?;
        sheet.pivots.iter().position(|p| {
            let loc = p.location;
            let pages = p.page_fields.len() as u32;
            let top = if pages > 0 { loc.first_row.saturating_sub(pages + 1) } else { loc.first_row };
            (top..=loc.last_row).contains(&row) && (loc.first_column..=loc.last_column).contains(&column)
        })
    }

    /// Selected cell's PivotTable index, active data field index, summary function, and showDataAs mode.
    pub(crate) fn active_pivot_data_field(&self) -> Option<(usize, usize, String, Option<String>)> {
        let pivot_idx = self.pivot_at_selection()?;
        let (_, sheet) = self.current_sheet()?;
        let pivot = sheet.pivots.get(pivot_idx)?;
        if pivot.data_fields.is_empty() {
            return None;
        }
        let val_pos = if pivot.data_fields.len() > 1 {
            if let Some((col, _)) = self.current_cell() {
                let first_val_col = pivot.location.first_column + pivot.first_data_col;
                if col >= first_val_col {
                    ((col - first_val_col) as usize) % pivot.data_fields.len()
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };
        let df = pivot.data_fields.get(val_pos)?;
        Some((pivot_idx, val_pos, df.function.clone(), df.show_as.clone()))
    }

    /// Set the summary function (`sum`, `count`, `average`, `max`, `min`, `product`) for the
    /// active value field of the PivotTable at selection.
    pub(crate) fn set_pivot_summarize_by_at_selection(&mut self, function: &'static str, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((pivot_idx, val_pos, _, _)) = self.active_pivot_data_field() else {
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, pivot_idx);
        self.change_pivot(target, |p| p.set_function(val_pos, function), cx);
    }

    /// Set the calculation type (`showDataAs`: `None`, `Some("percentOfTotal")`, `Some("percentOfCol")`,
    /// `Some("percentOfRow")`) for the active value field of the PivotTable at selection.
    pub(crate) fn set_pivot_show_values_as_at_selection(&mut self, show_as: Option<&'static str>, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((pivot_idx, val_pos, _, _)) = self.active_pivot_data_field() else {
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, pivot_idx);
        let mode = show_as.map(str::to_string);
        self.change_pivot(target, |p| p.set_show_as(val_pos, mode), cx);
    }

    /// ピボットテーブルの更新 (`all`: すべて更新, every pivot table of the
    /// book): each built again from its source, in its style, its columns
    /// fitted as Excel fits them; one step to undo.
    pub(crate) fn refresh_pivots(&mut self, all: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let targets: Vec<(SlideId, usize)> = if all {
            self.presentation()
                .slides
                .iter()
                .flat_map(|s| {
                    let count = s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(sheet) = &sh.kind { Some(sheet.pivots.len()) } else { None }).unwrap_or(0);
                    (0..count).map(move |i| (s.id, i))
                })
                .collect()
        } else {
            match self.pivot_at_selection() {
                Some(i) => vec![(self.current_slide, i)],
                None => {
                    self.status = Some(crate::shell::i18n::text("_xlsx.select-a-cell-in-a-pivottable").into());
                    return cx.notify();
                }
            }
        };
        if targets.is_empty() {
            return;
        }
        // Built one after another on a copy (several on one sheet).
        let mut work = self.presentation().clone();
        let default_size = work.workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
        let ts = cx.text_system().clone();
        let mut changed: Vec<SlideId> = Vec::new();
        for (slide, index) in targets {
            let Some(mut sheet) = ibis_pptx::refreshed_pivot(&work, slide, index) else { continue };
            let before = work.slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None }));
            if before.is_some_and(|old| covers_others(&old, &sheet, index)) {
                return self.refuse_overlap(cx);
            }
            let fitted_widths = fitted_for(&sheet, &[index], default_size, &self.resources, &ts);
        set_fitted_widths(&mut sheet, fitted_widths);
            if let Some(s) = work.slide_mut(slide)
                && let Some(ShapeKind::Sheet(target)) = s.shapes.iter_mut().map(|sh| &mut sh.kind).find(|k| matches!(k, ShapeKind::Sheet(_)))
            {
                *target = Arc::new(sheet);
            }
            if !changed.contains(&slide) {
                changed.push(slide);
            }
        }
        let mut commands: Vec<Command> = changed
            .into_iter()
            .filter_map(|slide| {
                let sheet = work.slide(slide)?.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some((**s).clone()) } else { None })?;
                Some(Command::Sheet(SheetCommand::ReplaceSheet { slide, sheet: Box::new(sheet) }))
            })
            .collect();
        // Also sync any PivotCharts linked to refreshed pivots
        for slide in self.presentation().slides.iter() {
            let sheet = work.slide(slide.id).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(sheet) = &sh.kind { Some((**sheet).clone()) } else { None }));
            if let Some(sheet) = sheet {
                for pivot in &sheet.pivots {
                    commands.extend(self.sync_pivot_charts_commands(slide.id, &sheet, pivot));
                }
            }
        }
        self.status = match self.document.execute(commands) {
            Ok(_) => None,
            Err(err) => Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into()),
        };
        cx.notify();
    }

    /// ピボットテーブルのデータセルに対して詳細データを展開する（Show Details / Drill Down）。
    /// 成功した場合は新規展開シートに切り替えて true を返す。
    pub(crate) fn show_pivot_details(&mut self, column: u32, row: u32, cx: &mut Context<Self>) -> bool {
        self.commit_cell_edit(cx);
        let before_slides: Vec<SlideId> = self.presentation().slides.iter().map(|s| s.id).collect();
        let cmd = Command::Sheet(SheetCommand::ShowPivotDetails {
            slide: self.current_slide,
            column,
            row,
        });
        match self.document.execute(vec![cmd]) {
            Ok(_) => {
                let after_slides: Vec<SlideId> = self.presentation().slides.iter().map(|s| s.id).collect();
                if let Some(&new_slide) = after_slides.iter().find(|id| !before_slides.contains(id)) {
                    self.current_slide = new_slide;
                    self.set_cell(0, 0);
                }
                self.status = Some(crate::shell::i18n::text("_xlsx.expanded-the-detail-data").into());
                cx.notify();
                true
            }
            Err(_) => false,
        }
    }

    /// メニュー（データ > 詳細データの表示）からの実行。
    pub(crate) fn show_pivot_details_at_selection(&mut self, cx: &mut Context<Self>) {
        let Some((column, row)) = self.current_cell() else {
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-data-cell-in-the-pivottable").into());
            return cx.notify();
        };
        if !self.show_pivot_details(column, row, cx) {
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-data-cell-in-the-pivottable").into());
            cx.notify();
        }
    }
}

/// データ > ピボットテーブルの更新, すべて更新, スタイルオプション, レイアウト.
pub(crate) fn menu_items() -> Vec<MenuEntry> {
    use crate::shell::chrome::Submenu;
    vec![
        MenuItem::tr("xlsx.pivot-fields").on(|this, window, cx| this.open_pivot_fields(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-value-settings").on(|this, window, cx| this.open_pivot_value_settings(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-options").on(|this, window, cx| this.open_pivot_options(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-insert-timeline").icon("timeline").on(|this, window, cx| this.open_timeline_insert_panel(window, cx)).into(),
        Submenu::new("xlsx.pivot-clear-menu", "clear_all", text("xlsx.pivot-clear"), vec![
            MenuItem::new("xlsx.pivot-clear-all", text("xlsx.pivot-clear-all")).on(|this, _, cx| this.clear_pivot_all(cx)).into(),
            MenuItem::new("xlsx.pivot-clear-filters", text("xlsx.pivot-clear-filters")).on(|this, _, cx| this.clear_pivot_filters(cx)).into(),
        ]).into(),
        Submenu::new("xlsx.pivot-select-menu", "select_all", text("xlsx.pivot-select"), vec![
            MenuItem::new("xlsx.pivot-select-entire-table", text("xlsx.pivot-select-entire-table")).on(|this, _, cx| this.select_pivot_part(0, cx)).into(),
            MenuItem::new("xlsx.pivot-select-values", text("xlsx.pivot-select-values")).on(|this, _, cx| this.select_pivot_part(1, cx)).into(),
            MenuItem::new("xlsx.pivot-select-labels", text("xlsx.pivot-select-labels")).on(|this, _, cx| this.select_pivot_part(2, cx)).into(),
        ]).into(),
        MenuItem::tr("xlsx.pivot-change-data-source").on(|this, window, cx| this.open_pivot_change_source(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-drill-buttons").on(|this, _, cx| this.toggle_pivot_drill_buttons(cx)).into(),
        MenuItem::tr("xlsx.pivot-clear").on(|this, _, cx| this.clear_pivot_fields(cx)).into(),
        MenuItem::tr("xlsx.pivot-select").on(|this, _, cx| this.select_entire_pivot(cx)).into(),
        MenuItem::tr("xlsx.pivot-move").on(|this, window, cx| this.open_pivot_move(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-blank-rows").on(|this, _, cx| this.toggle_pivot_blank_rows(cx)).into(),
        MenuItem::tr("xlsx.pivot-row-headers").on(|this, _, cx| this.toggle_pivot_row_headers(cx)).into(),
        MenuItem::tr("xlsx.pivot-col-headers").on(|this, _, cx| this.toggle_pivot_col_headers(cx)).into(),
        MenuItem::new("xlsx.pivot-banded-rows", text("_xlsx.banded-rows")).on(|this, _, cx| this.toggle_pivot_banded_rows(cx)).into(),
        MenuItem::new("xlsx.pivot-banded-columns", text("_xlsx.banded-columns")).on(|this, _, cx| this.toggle_pivot_banded_columns(cx)).into(),
        MenuItem::tr("xlsx.pivot-styles").on(|this, _, cx| this.set_pivot_style("PivotStyleLight16", cx)).into(),
        Submenu::new("xlsx.pivot-styles-menu", "palette", text("xlsx.pivot-styles"), pivot_style_items()).into(),
        Submenu::new("xlsx.pivot-report-layout", "table_rows", text("_xlsx.pivot-layout-updated"), vec![
            MenuItem::new("xlsx.pivot-layout-compact", text("_xlsx.pivot-compact-layout")).on(|this, _, cx| this.set_pivot_report_layout(PivotReportLayout::Compact, cx)).into(),
            MenuItem::new("xlsx.pivot-layout-outline", text("_xlsx.pivot-outline-layout")).on(|this, _, cx| this.set_pivot_report_layout(PivotReportLayout::Outline, cx)).into(),
            MenuItem::new("xlsx.pivot-layout-tabular", text("_xlsx.pivot-tabular-layout")).on(|this, _, cx| this.set_pivot_report_layout(PivotReportLayout::Tabular, cx)).into(),
            MenuItem::new("xlsx.pivot-repeat-labels", text("_xlsx.repeat-all-item-labels")).on(|this, _, cx| this.toggle_pivot_repeat_item_labels(cx)).into(),
            MenuItem::new("xlsx.pivot-grand-rows", text("_xlsx.show-grand-total-row")).on(|this, _, cx| this.toggle_pivot_row_grand_totals(cx)).into(),
            MenuItem::new("xlsx.pivot-grand-cols", text("_xlsx.show-grand-total-column")).on(|this, _, cx| this.toggle_pivot_col_grand_totals(cx)).into(),
        ]).into(),
        Submenu::new("xlsx.pivot-subtotals-menu", "functions", text("_xlsx.subtotals"), vec![
            MenuItem::new("xlsx.pivot-subtotal-off", text("_xlsx.do-not-show-subtotals")).on(|this, _, cx| this.set_pivot_subtotals(0, cx)).into(),
            MenuItem::new("xlsx.pivot-subtotal-bottom", text("_xlsx.show-all-subtotals-at-bottom-of-group")).on(|this, _, cx| this.set_pivot_subtotals(1, cx)).into(),
            MenuItem::new("xlsx.pivot-subtotal-top", text("_xlsx.show-all-subtotals-at-top-of-group")).on(|this, _, cx| this.set_pivot_subtotals(2, cx)).into(),
        ]).into(),
        Submenu::new("xlsx.pivot-blank-rows-menu", "space_bar", text("xlsx.pivot-blank-rows"), vec![
            MenuItem::new("xlsx.pivot-insert-blank-line", text("xlsx.pivot-insert-blank-line")).on(|this, _, cx| this.set_pivot_blank_line(true, cx)).into(),
            MenuItem::new("xlsx.pivot-remove-blank-line", text("xlsx.pivot-remove-blank-line")).on(|this, _, cx| this.set_pivot_blank_line(false, cx)).into(),
        ]).into(),
        MenuItem::tr("xlsx.pivot-show-details").on(|this, _, cx| this.show_pivot_details_at_selection(cx)).into(),
        MenuItem::tr("xlsx.pivot-calculated-field").on(|this, window, cx| this.open_pivot_calc_field(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-calculated-item").on(|this, window, cx| this.open_pivot_calc_item(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-group").on(|this, window, cx| this.open_pivot_group(window, cx)).into(),
        MenuItem::tr("xlsx.pivot-ungroup").on(|this, _, cx| this.pivot_ungroup(cx)).into(),
        MenuItem::tr("xlsx.pivot-refresh").on(|this, _, cx| this.refresh_pivots(false, cx)).into(),
        MenuItem::tr("xlsx.pivot-refresh-all").on(|this, _, cx| this.refresh_pivots(true, cx)).into(),
    ]
}

pub(crate) const PIVOT_STYLES: [(&str, &str, &str); 13] = [
    ("xlsx.pivot-style.none", "", "_shell.none"),
    ("xlsx.pivot-style.light-1", "PivotStyleLight1", "_xlsx.light"),
    ("xlsx.pivot-style.light-2", "PivotStyleLight2", "_xlsx.light"),
    ("xlsx.pivot-style.light-8", "PivotStyleLight8", "_xlsx.light"),
    ("xlsx.pivot-style.light-9", "PivotStyleLight9", "_xlsx.light"),
    ("xlsx.pivot-style.light-16", "PivotStyleLight16", "_xlsx.light"),
    ("xlsx.pivot-style.medium-1", "PivotStyleMedium1", "_xlsx.medium-2"),
    ("xlsx.pivot-style.medium-2", "PivotStyleMedium2", "_xlsx.medium-2"),
    ("xlsx.pivot-style.medium-7", "PivotStyleMedium7", "_xlsx.medium-2"),
    ("xlsx.pivot-style.medium-9", "PivotStyleMedium9", "_xlsx.medium-2"),
    ("xlsx.pivot-style.medium-14", "PivotStyleMedium14", "_xlsx.medium-2"),
    ("xlsx.pivot-style.dark-1", "PivotStyleDark1", "_xlsx.dark"),
    ("xlsx.pivot-style.dark-3", "PivotStyleDark3", "_xlsx.dark"),
];

pub(crate) fn pivot_style_items() -> Vec<MenuEntry> {
    PIVOT_STYLES
        .iter()
        .map(|&(id, name, group_label)| {
            let label = if name.is_empty() {
                text(group_label)
            } else {
                let num = name.trim_start_matches(|c: char| !c.is_ascii_digit());
                SharedString::from(format!("{} {num}", text(group_label)))
            };
            MenuItem::new(id, label)
                .on(move |this, _, cx| this.set_pivot_style_named(name, cx))
                .into()
        })
        .collect()
}

use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{AppContext as _, Entity, FontWeight, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window, div, px};
use ibis_doc_model::{PivotAxis, PivotSource, PivotTable};

use crate::shell::chrome::SidebarPanel;
use crate::shell::i18n::text;

/// 挿入 > ピボットテーブル: where its data is and where it goes; then the
/// table's fields (ピボットテーブルのフィールド).
pub(crate) struct PivotPanel {
    pub(crate) create: Option<CreatePivot>,
    pub(crate) change_source: Option<ChangePivotSource>,
    pub(crate) move_pivot: Option<MovePivot>,
    /// The table whose fields are shown (its sheet and index).
    pub(crate) target: Option<(SlideId, usize)>,
    /// The value whose summary is being chosen.
    pub(crate) value: Option<usize>,
    /// The custom name input for the value field being edited.
    pub(crate) value_name: Option<Entity<InputState>>,
    /// Selected row/col field for field settings (subtotals, layout).
    pub(crate) field_selected: Option<(PivotAxis, usize)>,
    pub(crate) message: String,
    /// 空白セルに表示する値 and エラー値に表示する値.
    pub(crate) missing: Option<Entity<InputState>>,
    pub(crate) error: Option<Entity<InputState>>,
}

pub(crate) struct CreatePivot {
    pub(crate) source: Entity<InputState>,
    pub(crate) new_sheet: bool,
    pub(crate) at: Entity<InputState>,
}

pub(crate) struct ChangePivotSource {
    pub(crate) source: Entity<InputState>,
}

pub(crate) struct MovePivot {
    pub(crate) destination: Entity<InputState>,
}

const AREAS: [(PivotAxis, &str); 4] = [(PivotAxis::Page, "_xlsx.filters"), (PivotAxis::Column, "_xlsx.columns"), (PivotAxis::Row, "_xlsx.rows"), (PivotAxis::Values, "_xlsx.values")];

/// レポートのレイアウト
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PivotReportLayout {
    Compact,
    Outline,
    Tabular,
}

impl SlideEditor {
    fn sheet_name_of(&self, slide: SlideId) -> Option<String> {
        self.presentation().slide(slide).and_then(|s| s.name.clone())
    }

    fn slide_named(&self, name: &str) -> Option<SlideId> {
        self.presentation().slides.iter().find(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name))).map(|s| s.id)
    }

    fn show_pivot_panel(&mut self, panel: PivotPanel, cx: &mut Context<Self>) {
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.pivot = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// 挿入 > ピボットテーブル: the panel, its data the table around the
    /// selected cell (or the selection), on a new sheet.
    pub(crate) fn open_create_pivot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let range = self.selected_range().filter(|r| r.first_row != r.last_row || r.first_column != r.last_column).or_else(|| self.current_region());
        let sheet = self.sheet_name_of(self.current_slide).unwrap_or_default();
        let source = range.map(|r| format!("{}!{}", quote_sheet(&sheet), absolute(r))).unwrap_or_default();
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let at = self.current_cell().map(|(c, r)| format!("{}!{}", quote_sheet(&sheet), ibis_doc_model::cell_name(c, r))).unwrap_or_default();
        let create = CreatePivot { source: input(source, window, cx), new_sheet: true, at: input(at, window, cx) };
        self.show_pivot_panel(PivotPanel { create: Some(create), change_source: None, move_pivot: None, target: None, value: None, value_name: None, field_selected: None, message: String::new(), missing: None, error: None }, cx);
    }

    /// データ > ピボットテーブルのフィールド: the panel of the table the
    /// selected cell is in.
    pub(crate) fn open_pivot_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = Some((self.current_slide, index));
        let pivot = self.presentation().slide(self.current_slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None }));
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let missing = input(pivot.as_ref().and_then(|p| p.missing_caption.clone()).unwrap_or_default(), window, cx);
        let error = input(pivot.as_ref().and_then(|p| p.error_caption.clone()).unwrap_or_default(), window, cx);
        self.show_pivot_panel(PivotPanel { create: None, change_source: None, move_pivot: None, target, value: None, value_name: None, field_selected: None, message: String::new(), missing: Some(missing), error: Some(error) }, cx);
    }

    /// データ > 値フィールドの設定…: the panel of the table the selected cell is in,
    /// focused on the value field.
    pub(crate) fn open_pivot_value_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, index);
        let pivot = self.presentation().slide(self.current_slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None }));
        let Some(pivot) = pivot else { return };
        if pivot.data_fields.is_empty() {
            self.status = Some(text("_xlsx.there-are-no-value-fields").into());
            return cx.notify();
        }
        let val_pos = if pivot.data_fields.len() > 1 {
            if let Some((col, _)) = self.current_cell() {
                let first_val_col = pivot.location.first_column + pivot.first_data_col;
                if col >= first_val_col {
                    ((col - first_val_col) as usize) % pivot.data_fields.len()
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let missing = input(pivot.missing_caption.clone().unwrap_or_default(), window, cx);
        let error = input(pivot.error_caption.clone().unwrap_or_default(), window, cx);
        let val_name = input(pivot.data_fields.get(val_pos).map(|d| d.name.clone()).unwrap_or_default(), window, cx);
        self.show_pivot_panel(PivotPanel {
            create: None,
            change_source: None,
            move_pivot: None,
            target: Some(target),
            value: Some(val_pos),
            value_name: Some(val_name),
            field_selected: None,
            message: String::new(),
            missing: Some(missing),
            error: Some(error),
        }, cx);
    }

    /// データ > データ ソースの変更… (xlsx.pivot-change-data-source):
    /// Open the change pivot source panel for the selected pivot table.
    pub(crate) fn open_pivot_change_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, index);
        let pivot = self.presentation().slide(self.current_slide).and_then(|s| {
            s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None })
        });
        let Some(pivot) = pivot else { return };
        let current_sheet = self.sheet_name_of(self.current_slide).unwrap_or_default();
        let source_text = if let Some(name) = &pivot.source.name {
            name.clone()
        } else if let Some(range) = pivot.source.range {
            let sheet = pivot.source.sheet.as_deref().unwrap_or(&current_sheet);
            format!("{}!{}", quote_sheet(sheet), absolute(range))
        } else {
            String::new()
        };
        let input = {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(source_text, window, cx));
            state
        };
        self.show_pivot_panel(PivotPanel {
            create: None,
            change_source: Some(ChangePivotSource { source: input }),
            move_pivot: None,
            target: Some(target),
            value: None,
            value_name: None,
            field_selected: None,
            message: String::new(),
            missing: None,
            error: None,
        }, cx);
    }

    /// Apply the modified pivot table data source.
    pub(crate) fn apply_change_pivot_source(&mut self, cx: &mut Context<Self>) {
        let Some(cs) = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.change_source.as_ref()) else { return };
        let Some((slide, index)) = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.target) else { return };
        let source_text = cs.source.read(cx).value().to_string();
        let current = self.sheet_name_of(slide).unwrap_or_default();
        let parsed_ref = super::sheet_ui::parse_reference(&source_text);
        let (source_sheet, range, source_name) = match parsed_ref {
            Some(super::sheet_ui::Reference { sheet, target: super::sheet_ui::Target::Cells(range) }) => {
                (sheet.unwrap_or_else(|| current.clone()), range, None)
            }
            Some(super::sheet_ui::Reference { target: super::sheet_ui::Target::Name(_), .. }) | None if super::sheet_ui::is_name(source_text.trim()) => {
                let name = source_text.trim();
                let mut found_table = None;
                for s in &self.presentation().slides {
                    let sh_name = s.name.clone().unwrap_or_default();
                    if let Some(sh) = s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(sheet) = &sh.kind { Some(sheet.clone()) } else { None }) {
                        if let Some(tbl) = sh.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name)) {
                            let r = ibis_doc_model::CellRange {
                                first_row: tbl.range.first_row,
                                last_row: tbl.range.last_row.saturating_sub(tbl.totals_rows),
                                first_column: tbl.range.first_column,
                                last_column: tbl.range.last_column,
                            };
                            found_table = Some((sh_name, r, Some(tbl.name.clone())));
                            break;
                        }
                    }
                }
                let found_name = if found_table.is_none() {
                    self.presentation().workbook.as_ref().and_then(|wb| {
                        wb.names.iter().find(|n| n.name.eq_ignore_ascii_case(name)).and_then(|dn| {
                            let trimmed = dn.formula.trim().trim_start_matches('=');
                            let (sh_n, cell_part) = if let Some((s, c)) = trimmed.split_once('!') {
                                (s.trim().trim_matches('\'').to_string(), c)
                            } else {
                                (current.clone(), trimmed)
                            };
                            let r = super::sheet_ui::parse_reference(cell_part).and_then(|re| match re.target {
                                super::sheet_ui::Target::Cells(c) => Some(c),
                                _ => None,
                            })?;
                            Some((sh_n, r, Some(dn.name.clone())))
                        })
                    })
                } else {
                    None
                };

                if let Some(t) = found_table.or(found_name) {
                    t
                } else {
                    return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx);
                }
            }
            _ => return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx),
        };
        let Some(source_slide) = self.slide_named(&source_sheet) else {
            return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx);
        };
        let Some(data_sheet) = self.presentation().slide(source_slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None })) else { return };
        let data = ibis_doc_model::pivot::source_data(&data_sheet, range);
        if range.last_row <= range.first_row || data.headings.iter().any(|h| h.trim().is_empty()) {
            return self.pivot_message(text("_xlsx.select-a-range-of-two-or-more-rows-with-a-headin"), cx);
        }
        let Some(old_pivot) = self.presentation().slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None })) else { return };
        let mut new_pivot = old_pivot.clone();
        new_pivot.source = PivotSource { sheet: Some(source_sheet), range: Some(range), name: source_name };
        for h in &data.headings {
            if !new_pivot.fields.iter().any(|f| f.name.eq_ignore_ascii_case(h)) {
                new_pivot.fields.push(ibis_doc_model::PivotField { name: h.clone(), ..Default::default() });
            }
        }
        let Some(mut sheet) = ibis_pptx::rebuilt_pivot(self.presentation(), slide, index, Some(new_pivot)) else {
            return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx);
        };
        let default_size = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
        let ts = cx.text_system().clone();
        let fitted_widths = fitted_for(&sheet, &[index], default_size, &self.resources, &ts);
        set_fitted_widths(&mut sheet, fitted_widths);
        let mut commands = vec![Command::Sheet(SheetCommand::ReplaceSheet { slide, sheet: Box::new(sheet.clone()) })];
        if let Some(p) = sheet.pivots.get(index) {
            commands.extend(self.sync_pivot_charts_commands(slide, &sheet, p));
        }
        self.run_sheet(commands, cx);
        self.close_pivot_panel(cx);
        self.status = Some(crate::shell::i18n::text("xlsx.pivot-source-changed").into());
        cx.notify();
    }

    /// +/- ボタンの表示/非表示を切り替え (xlsx.pivot-drill-buttons)
    pub(crate) fn toggle_pivot_drill_buttons(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        let Some(mut sheet) = self.presentation().slide(slide).and_then(|s| {
            s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some((**s).clone()) } else { None })
        }) else { return };
        let Some(pivot) = sheet.pivots.get_mut(index) else { return };
        pivot.show_drill = !pivot.show_drill;
        let shown = pivot.show_drill;
        self.run_sheet(vec![Command::Sheet(SheetCommand::ReplaceSheet { slide, sheet: Box::new(sheet) })], cx);
        self.status = Some(if shown {
            crate::shell::i18n::text("xlsx.pivot-drill-buttons-shown").into()
        } else {
            crate::shell::i18n::text("xlsx.pivot-drill-buttons-hidden").into()
        });
        cx.notify();
    }

    /// 縞模様（行）の表示/非表示を切り替え (`xlsx.pivot-banded-rows`)
    pub(crate) fn toggle_pivot_banded_rows(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        let on = self
            .current_sheet()
            .and_then(|(_, s)| s.pivots.get(index))
            .and_then(|p| p.style.as_ref())
            .is_none_or(|st| !st.row_stripes);
        self.change_pivot(
            (slide, index),
            move |pivot| {
                let style = pivot.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo {
                    name: "PivotStyleLight16".into(),
                    row_headers: true,
                    col_headers: true,
                    ..Default::default()
                });
                style.row_stripes = on;
            },
            cx,
        );
        self.status = Some(if on { text("_xlsx.banded-rows-enabled").into() } else { text("_xlsx.banded-rows-disabled").into() });
        cx.notify();
    }

    /// 縞模様（列）の表示/非表示を切り替え (`xlsx.pivot-banded-columns`)
    pub(crate) fn toggle_pivot_banded_columns(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        let on = self
            .current_sheet()
            .and_then(|(_, s)| s.pivots.get(index))
            .and_then(|p| p.style.as_ref())
            .is_none_or(|st| !st.col_stripes);
        self.change_pivot(
            (slide, index),
            move |pivot| {
                let style = pivot.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo {
                    name: "PivotStyleLight16".into(),
                    row_headers: true,
                    col_headers: true,
                    ..Default::default()
                });
                style.col_stripes = on;
            },
            cx,
        );
        self.status = Some(if on { text("_xlsx.banded-columns-enabled").into() } else { text("_xlsx.banded-columns-disabled").into() });
        cx.notify();
    }

    /// ピボットテーブルのスタイルを設定 (`style_name`: "PivotStyleLight16" 等、空文字でクリア)
    pub(crate) fn set_pivot_style_named(&mut self, style_name: &'static str, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        let is_none = style_name.is_empty() || style_name == "None";
        self.change_pivot((slide, index), move |p| {
            if is_none {
                p.style = None;
            } else {
                let st = p.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo {
                    name: style_name.to_string(),
                    row_headers: true,
                    col_headers: true,
                    ..Default::default()
                });
                st.name = style_name.to_string();
            }
        }, cx);
        self.status = Some(crate::shell::i18n::text("xlsx.pivot-style-applied").into());
        cx.notify();
    }

    /// 選択中ピボットテーブルの現在のレポートレイアウトを取得
    pub(crate) fn pivot_report_layout_at_selection(&self) -> Option<PivotReportLayout> {
        let index = self.pivot_at_selection()?;
        let (_, sheet) = self.current_sheet()?;
        let pivot = sheet.pivots.get(index)?;
        Some(match layout_of(pivot) {
            ReportLayout::Compact => PivotReportLayout::Compact,
            ReportLayout::Outline => PivotReportLayout::Outline,
            ReportLayout::Tabular => PivotReportLayout::Tabular,
        })
    }

    /// レポートのレイアウト（コンパクト形式・アウトライン形式・表形式）を設定
    pub(crate) fn set_pivot_report_layout(&mut self, layout: PivotReportLayout, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        self.change_pivot(
            (slide, index),
            move |pivot| {
                pivot.compact = matches!(layout, PivotReportLayout::Compact);
                pivot.outline = !matches!(layout, PivotReportLayout::Tabular);
                for f in &mut pivot.fields {
                    f.compact = pivot.compact;
                    f.outline = pivot.outline;
                }
            },
            cx,
        );
        self.status = Some(text("_xlsx.pivot-layout-updated").into());
        cx.notify();
    }

    /// 選択中ピボットテーブルの現在の小計モードを取得（0: 非表示, 1: グループの下部, 2: グループの上部）
    pub(crate) fn pivot_subtotals_mode_at_selection(&self) -> Option<u8> {
        let index = self.pivot_at_selection()?;
        let (_, sheet) = self.current_sheet()?;
        let pivot = sheet.pivots.get(index)?;
        Some(match subtotals_of(pivot) {
            Subtotals::Off => 0,
            Subtotals::Bottom => 1,
            Subtotals::Top => 2,
        })
    }

    /// 小計の表示モード（0: 非表示, 1: グループの下部, 2: グループの上部）を設定
    pub(crate) fn set_pivot_subtotals(&mut self, mode: u8, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        self.change_pivot(
            (slide, index),
            move |pivot| {
                for f in &mut pivot.fields {
                    f.default_subtotal = mode != 0;
                    f.subtotal_top = mode == 2;
                }
            },
            cx,
        );
        self.status = Some(text("_xlsx.pivot-layout-updated").into());
        cx.notify();
    }

    /// 選択中ピボットテーブルの現在のスタイル名を取得（未設定は空文字列）
    pub(crate) fn pivot_style_name_at_selection(&self) -> Option<String> {
        let index = self.pivot_at_selection()?;
        let (_, sheet) = self.current_sheet()?;
        let pivot = sheet.pivots.get(index)?;
        Some(pivot.style.as_ref().map(|s| s.name.clone()).unwrap_or_default())
    }

    /// 行の総計の表示/非表示を切り替え
    pub(crate) fn toggle_pivot_row_grand_totals(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        self.change_pivot((slide, index), |p| p.row_grand_totals = !p.row_grand_totals, cx);
        self.status = Some(text("_xlsx.pivot-layout-updated").into());
        cx.notify();
    }

    /// 列の総計の表示/非表示を切り替え
    pub(crate) fn toggle_pivot_col_grand_totals(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        self.change_pivot((slide, index), |p| p.col_grand_totals = !p.col_grand_totals, cx);
        self.status = Some(text("_xlsx.pivot-layout-updated").into());
        cx.notify();
    }

    /// すべてのアイテムのラベルを繰り返し表示の切り替え
    pub(crate) fn toggle_pivot_repeat_item_labels(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        self.change_pivot(
            (slide, index),
            |p| {
                p.fill_down_labels = !p.fill_down_labels;
                for f in &mut p.fields {
                    f.fill_down_labels = None;
                }
            },
            cx,
        );
        self.status = Some(text("_xlsx.pivot-layout-updated").into());
        cx.notify();
    }

    /// ピボットテーブルの全フィールドをクリア（初期化）
    pub(crate) fn clear_pivot_all(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        self.change_pivot((slide, index), |p| {
            p.row_fields.clear();
            p.col_fields.clear();
            p.data_fields.clear();
            p.page_fields.clear();
            p.filters.clear();
            for f in &mut p.fields {
                f.axis = None;
                f.subtotal_top = true;
                f.default_subtotal = true;
            }
        }, cx);
        self.status = Some(text("xlsx.pivot-cleared").into());
        cx.notify();
    }

    /// ピボットテーブルのフィルターをクリア
    pub(crate) fn clear_pivot_filters(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        self.change_pivot((slide, index), |p| {
            p.filters.clear();
            for f in &mut p.fields {
                for item in &mut f.items {
                    item.hidden = false;
                }
            }
        }, cx);
        self.status = Some(text("xlsx.pivot-filters-cleared").into());
        cx.notify();
    }

    /// 各アイテムの後に空行を挿入（または削除）
    pub(crate) fn set_pivot_blank_line(&mut self, insert: bool, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        self.change_pivot((slide, index), |p| {
            for &rf in &p.row_fields {
                if rf >= 0 && (rf as usize) < p.fields.len() {
                    p.fields[rf as usize].insert_blank_row = insert;
                }
            }
        }, cx);
        cx.notify();
    }

    /// ピボットテーブルの要素を選択（0: 全体, 1: 値, 2: ラベル）
    pub(crate) fn select_pivot_part(&mut self, part: usize, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let (_, sheet) = self.current_sheet().unwrap();
        let p = &sheet.pivots[index];
        let (c1, r1, c2, r2) = match part {
            1 => {
                // 値エリア
                let c1 = p.location.first_column + p.first_data_col;
                let r1 = p.location.first_row + p.first_data_row;
                (c1, r1, p.location.last_column, p.location.last_row)
            }
            2 => {
                // ラベルエリア
                let c2 = (p.location.first_column + p.first_data_col).saturating_sub(1);
                (p.location.first_column, p.location.first_row, c2, p.location.last_row)
            }
            _ => {
                // 全体
                (p.location.first_column, p.location.first_row, p.location.last_column, p.location.last_row)
            }
        };
        let slide = self.current_slide;
        self.wb_mut().cells.insert(slide, (c1, r1));
        self.wb_mut().cell_ends.insert(slide, (c2, r2));
        cx.notify();
    }

    /// ピボットテーブルを別のセルへ移動
    pub(crate) fn move_pivot_to_corner(&mut self, col: u32, row: u32, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        self.change_pivot((slide, index), |p| {
            let width = p.location.last_column.saturating_sub(p.location.first_column);
            let height = p.location.last_row.saturating_sub(p.location.first_row);
            p.location = ibis_doc_model::CellRange {
                first_column: col,
                first_row: row,
                last_column: col + width,
                last_row: row + height,
            };
        }, cx);
        self.set_cell(col, row);
        self.status = Some(text("_xlsx.pivot-table-moved").into());
        cx.notify();
    }

    /// 行の見出しの表示/非表示を切り替え (`xlsx.pivot-row-headers`)
    pub(crate) fn toggle_pivot_row_headers(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        let on = self
            .current_sheet()
            .and_then(|(_, s)| s.pivots.get(index))
            .and_then(|p| p.style.as_ref())
            .is_some_and(|st| !st.row_headers);
        self.change_pivot(
            (slide, index),
            move |pivot| {
                let style = pivot.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo {
                    name: "PivotStyleLight16".into(),
                    row_headers: true,
                    col_headers: true,
                    ..Default::default()
                });
                style.row_headers = on;
            },
            cx,
        );
        self.status = Some(if on { text("_xlsx.row-headers-enabled").into() } else { text("_xlsx.row-headers-disabled").into() });
        cx.notify();
    }

    /// 列の見出しの表示/非表示を切り替え (`xlsx.pivot-col-headers`)
    pub(crate) fn toggle_pivot_col_headers(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        let on = self
            .current_sheet()
            .and_then(|(_, s)| s.pivots.get(index))
            .and_then(|p| p.style.as_ref())
            .is_some_and(|st| !st.col_headers);
        self.change_pivot(
            (slide, index),
            move |pivot| {
                let style = pivot.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo {
                    name: "PivotStyleLight16".into(),
                    row_headers: true,
                    col_headers: true,
                    ..Default::default()
                });
                style.col_headers = on;
            },
            cx,
        );
        self.status = Some(if on { text("_xlsx.col-headers-enabled").into() } else { text("_xlsx.col-headers-disabled").into() });
        cx.notify();
    }

    /// 各アイテムの後に空行を挿入/削除 (`xlsx.pivot-blank-rows`)
    pub(crate) fn toggle_pivot_blank_rows(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        let new_val = self
            .current_sheet()
            .and_then(|(_, s)| s.pivots.get(index))
            .is_none_or(|pivot| {
                !pivot
                    .fields
                    .iter()
                    .filter(|f| f.axis == Some(PivotAxis::Row))
                    .any(|f| f.insert_blank_row)
            });
        self.change_pivot(
            (slide, index),
            move |pivot| {
                for f in &mut pivot.fields {
                    if f.axis == Some(PivotAxis::Row) {
                        f.insert_blank_row = new_val;
                    }
                }
            },
            cx,
        );
        self.status = Some(if new_val { text("_xlsx.blank-rows-inserted").into() } else { text("_xlsx.blank-rows-removed").into() });
        cx.notify();
    }

    /// ピボットテーブルのフィールドを一括クリア (`xlsx.pivot-clear`)
    pub(crate) fn clear_pivot_fields(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, index);
        self.change_pivot(target, |pivot| {
            pivot.row_fields.clear();
            pivot.col_fields.clear();
            pivot.page_fields.clear();
            pivot.data_fields.clear();
            for f in &mut pivot.fields {
                f.axis = None;
                f.items.clear();
            }
        }, cx);
        self.status = Some(text("_xlsx.pivot-fields-cleared").into());
        cx.notify();
    }

    /// ピボットテーブル全体を選択 (`xlsx.pivot-select`)
    pub(crate) fn select_entire_pivot(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let loc = {
            let Some((_, sheet)) = self.current_sheet() else { return };
            let Some(pivot) = sheet.pivots.get(index) else { return };
            pivot.location
        };
        let slide = self.current_slide;
        let s = self.wb_mut();
        s.cells.insert(slide, (loc.first_column, loc.first_row));
        s.cell_ends.insert(slide, (loc.last_column, loc.last_row));
        self.status = Some(text("_xlsx.pivot-table-selected").into());
        cx.notify();
    }

    /// ピボットテーブルの移動パネルを開く (`xlsx.pivot-move`)
    pub(crate) fn open_pivot_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let sheet = self.sheet_name_of(self.current_slide).unwrap_or_default();
        let at = self.current_cell().map(|(c, r)| format!("{}!{}", quote_sheet(&sheet), ibis_doc_model::cell_name(c, r))).unwrap_or_default();
        let destination = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(at, window, cx);
            s
        });
        self.show_pivot_panel(PivotPanel {
            create: None,
            change_source: None,
            move_pivot: Some(MovePivot { destination }),
            target: Some((self.current_slide, index)),
            value: None,
            value_name: None,
            field_selected: None,
            message: String::new(),
            missing: None,
            error: None,
        }, cx);
    }

    /// ピボットテーブルの移動を適用
    pub(crate) fn apply_move_pivot(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let (slide, index, destination) = {
            let Some(p) = self.wb_mut().panels.pivot.as_ref() else { return };
            let Some(target) = p.target else { return };
            let Some(mv) = &p.move_pivot else { return };
            (target.0, target.1, mv.destination.read(cx).value().trim().to_string())
        };
        let parsed_ref = super::sheet_ui::parse_reference(&destination);
        let (col, row) = match parsed_ref {
            Some(super::sheet_ui::Reference { target: super::sheet_ui::Target::Cells(range), .. }) => {
                (range.first_column, range.first_row)
            }
            _ => {
                if let Some(c) = ibis_calc::formula::parser::parse_cell(&destination) {
                    (c.column, c.row)
                } else {
                    self.pivot_message(text("_xlsx.where-to-place-the-pivottable"), cx);
                    return;
                }
            }
        };

        self.change_pivot((slide, index), |pivot| {
            let width = pivot.location.last_column.saturating_sub(pivot.location.first_column);
            let height = pivot.location.last_row.saturating_sub(pivot.location.first_row);
            pivot.location.first_column = col;
            pivot.location.first_row = row;
            pivot.location.last_column = col + width;
            pivot.location.last_row = row + height;
        }, cx);
        self.set_cell(col, row);
        self.close_pivot_panel(cx);
        self.status = Some(text("_xlsx.pivot-table-moved").into());
        cx.notify();
    }

    /// ピボットテーブルのスタイルを設定 (`xlsx.pivot-styles`)
    pub(crate) fn set_pivot_style(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let slide = self.current_slide;
        let style_name = name.to_string();
        let is_none = style_name.is_empty() || style_name == "None";
        self.change_pivot(
            (slide, index),
            move |pivot| {
                if is_none {
                    pivot.style = None;
                } else {
                    let style = pivot.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo {
                        name: "PivotStyleLight16".into(),
                        row_headers: true,
                        col_headers: true,
                        ..Default::default()
                    });
                    style.name = style_name.clone();
                }
            },
            cx,
        );
        self.status = Some(crate::shell::i18n::format("_xlsx.pivot-style-applied", &[("style", &name)]).into());
        cx.notify();
    }

    /// Close the pivot sidebar panel.
    pub(crate) fn close_pivot_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.panels.pivot = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    fn current_sheet_of(&self, slide: SlideId) -> Option<Arc<Sheet>> {
        self.presentation().slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None }))
    }

    fn refuse_overlap(&mut self, cx: &mut Context<Self>) {
        let message = text("_xlsx.the-pivottable-would-overlap-other-cells-or-anot");
        self.status = Some(message.clone());
        self.pivot_message(message, cx);
    }

    fn pivot_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot.as_mut() {
            p.message = message.to_string();
        }
        cx.notify();
    }

    /// The panel's 作成: the table made where it says, with nothing placed,
    /// and its fields shown; one step to undo.
    pub(crate) fn create_pivot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.create.as_ref()) else { return };
        let (source_text, new_sheet, at_text) = (c.source.read(cx).value().to_string(), c.new_sheet, c.at.read(cx).value().to_string());
        let current = self.sheet_name_of(self.current_slide).unwrap_or_default();
        let parsed_ref = super::sheet_ui::parse_reference(&source_text);
        let (source_sheet, range, source_name) = match parsed_ref {
            Some(super::sheet_ui::Reference { sheet, target: super::sheet_ui::Target::Cells(range) }) => {
                (sheet.unwrap_or_else(|| current.clone()), range, None)
            }
            Some(super::sheet_ui::Reference { target: super::sheet_ui::Target::Name(_), .. }) | None if super::sheet_ui::is_name(source_text.trim()) => {
                let name = source_text.trim();
                let mut found_table = None;
                for slide in &self.presentation().slides {
                    let sh_name = slide.name.clone().unwrap_or_default();
                    if let Some(sh) = slide.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None }) {
                        if let Some(tbl) = sh.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name)) {
                            let r = ibis_doc_model::CellRange {
                                first_row: tbl.range.first_row,
                                last_row: tbl.range.last_row.saturating_sub(tbl.totals_rows),
                                first_column: tbl.range.first_column,
                                last_column: tbl.range.last_column,
                            };
                            found_table = Some((sh_name, r, Some(tbl.name.clone())));
                            break;
                        }
                    }
                }
                let found_name = if found_table.is_none() {
                    self.presentation().workbook.as_ref().and_then(|wb| {
                        wb.names.iter().find(|n| n.name.eq_ignore_ascii_case(name)).and_then(|dn| {
                            let trimmed = dn.formula.trim().trim_start_matches('=');
                            let (sh_n, cell_part) = if let Some((s, c)) = trimmed.split_once('!') {
                                (s.trim().trim_matches('\'').to_string(), c)
                            } else {
                                (current.clone(), trimmed)
                            };
                            let r = super::sheet_ui::parse_reference(cell_part).and_then(|re| match re.target {
                                super::sheet_ui::Target::Cells(c) => Some(c),
                                _ => None,
                            })?;
                            Some((sh_n, r, Some(dn.name.clone())))
                        })
                    })
                } else {
                    None
                };

                if let Some(t) = found_table.or(found_name) {
                    t
                } else {
                    return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx);
                }
            }
            _ => return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx),
        };
        let Some(source_slide) = self.slide_named(&source_sheet) else { return self.pivot_message(text("_xlsx.the-data-range-is-not-valid"), cx) };
        let Some(data_sheet) = self.presentation().slide(source_slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None })) else { return };
        let data = ibis_doc_model::pivot::source_data(&data_sheet, range);
        if range.last_row <= range.first_row || data.headings.iter().any(|h| h.trim().is_empty()) {
            return self.pivot_message(text("_xlsx.select-a-range-of-two-or-more-rows-with-a-headin"), cx);
        }
        let count: usize = self.presentation().slides.iter().filter_map(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.pivots.len()) } else { None })).sum();
        let name = format!("{}{}", text("_xlsx.pivottable"), count + 1);
        let source = PivotSource { sheet: Some(source_sheet), range: Some(range), name: source_name };
        let (slide, row, column) = if new_sheet {
            // A new sheet before the current one, the table at A3, as Excel puts it.
            let Some(wb) = self.presentation().workbook.clone() else { return };
            let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)).unwrap_or(0);
            let mut n = wb.sheets.len() + 1;
            let sheet_name = loop {
                let name = format!("Sheet{n}");
                if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                    break name;
                }
                n += 1;
            };
            let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::AddSheet { index, name: sheet_name, slide, shape })]) {
                return self.pivot_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
            }
            (slide, 2, 0)
        } else {
            let Some(super::sheet_ui::Reference { sheet, target: super::sheet_ui::Target::Cells(at) }) = super::sheet_ui::parse_reference(&at_text) else {
                return self.pivot_message(text("_xlsx.the-location-is-not-valid"), cx);
            };
            let Some(slide) = self.slide_named(sheet.as_deref().unwrap_or(&current)) else { return self.pivot_message(text("_xlsx.the-location-is-not-valid"), cx) };
            (slide, at.first_row, at.first_column)
        };
        let pivot = PivotTable::new(&name, source, &data, row, column);
        let index = self.presentation().slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.pivots.len()) } else { None })).unwrap_or(0);
        let Some(sheet) = ibis_pptx::rebuilt_pivot(self.presentation(), slide, index, Some(pivot)) else { return };
        if !new_sheet && self.current_sheet_of(slide).is_some_and(|old| covers_others(&old, &sheet, index) || old.cell(row, column).is_some_and(|c| !c.value.is_empty())) {
            return self.refuse_overlap(cx);
        }
        let command = vec![Command::Sheet(SheetCommand::ReplaceSheet { slide, sheet: Box::new(sheet) })];
        let result = if new_sheet { self.document.amend_last(command) } else { self.document.execute(command) };
        if let Err(err) = result {
            return self.pivot_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        if slide != self.current_slide {
            self.select_slide(slide, window, cx);
        }
        let s = self.wb_mut();
        s.cells.insert(slide, (column, row));
        s.cell_ends.remove(&slide);
        if let Some(p) = s.panels.pivot.as_mut() {
            p.create = None;
            p.target = Some((slide, index));
            p.message = String::new();
        }
        cx.notify();
    }

    /// 挿入 > おすすめピボットテーブル (xlsx.recommended-pivots):
    /// データ範囲を分析し、最適な行フィールドと値フィールドを設定したピボットテーブルを新規シートに配置する。
    pub(crate) fn open_recommended_pivots(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let range = self.selected_range().filter(|r| r.first_row != r.last_row || r.first_column != r.last_column).or_else(|| self.current_region());
        let Some(range) = range else {
            self.status = Some(crate::shell::i18n::text("xlsx.chart.no-data").into());
            cx.notify();
            return;
        };
        let Some((_, data_sheet)) = self.current_sheet() else { return };
        let data = ibis_doc_model::pivot::source_data(&data_sheet, range);
        if range.last_row <= range.first_row || data.headings.iter().any(|h| h.trim().is_empty()) {
            self.status = Some(text("_xlsx.select-a-range-of-two-or-more-rows-with-a-headin").into());
            cx.notify();
            return;
        }

        let source_sheet = self.sheet_name_of(self.current_slide).unwrap_or_default();
        let count: usize = self.presentation().slides.iter().filter_map(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.pivots.len()) } else { None })).sum();
        let name = format!("{}{}", text("_xlsx.pivottable"), count + 1);
        let source = PivotSource { sheet: Some(source_sheet), range: Some(range), name: None };

        // 新規シートを作成（A3に配置）
        let Some(wb) = self.presentation().workbook.clone() else { return };
        let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)).unwrap_or(0);
        let mut n = wb.sheets.len() + 1;
        let sheet_name = loop {
            let name = format!("Sheet{n}");
            if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                break name;
            }
            n += 1;
        };
        let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::AddSheet { index, name: sheet_name, slide, shape })]) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
            cx.notify();
            return;
        }

        let mut pivot = PivotTable::new(&name, source, &data, 2, 0);

        // データフィールドとカテゴリフィールドの判別
        let mut num_cols = Vec::new();
        let mut cat_cols = Vec::new();
        for (i, _) in data.headings.iter().enumerate() {
            let num_count = data.records.iter().filter(|r| matches!(r.get(i), Some(ibis_doc_model::CellValue::Number(_)))).count();
            if num_count > data.records.len() / 2 {
                num_cols.push(i);
            } else {
                cat_cols.push(i);
            }
        }

        let row_col = cat_cols.first().copied().unwrap_or(0);
        pivot.row_fields.push(row_col as i32);
        if let Some(f) = pivot.fields.get_mut(row_col) {
            f.axis = Some(PivotAxis::Row);
        }

        if let Some(&num_col) = num_cols.first() {
            let h = &data.headings[num_col];
            pivot.data_fields.push(ibis_doc_model::PivotDataField {
                name: crate::shell::i18n::format("xlsx.pivot.sum-of", &[("name", h)]),
                field: num_col as u32,
                function: "sum".to_string(),
                show_as: None,
                number_format: None,
            });
        } else {
            let target_col = if cat_cols.len() > 1 { cat_cols[1] } else { 0 };
            let h = &data.headings[target_col];
            pivot.data_fields.push(ibis_doc_model::PivotDataField {
                name: crate::shell::i18n::format("xlsx.pivot.count-of", &[("name", h)]),
                field: target_col as u32,
                function: "count".to_string(),
                show_as: None,
                number_format: None,
            });
        }

        let pivot_index = self.presentation().slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s.pivots.len()) } else { None })).unwrap_or(0);
        let Some(mut sheet) = ibis_pptx::rebuilt_pivot(self.presentation(), slide, pivot_index, Some(pivot)) else { return };

        let default_size = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
        let ts = cx.text_system().clone();
        let fitted_widths = fitted_for(&sheet, &[pivot_index], default_size, &self.resources, &ts);
        set_fitted_widths(&mut sheet, fitted_widths);

        let command = vec![Command::Sheet(SheetCommand::ReplaceSheet { slide, sheet: Box::new(sheet) })];
        if let Err(err) = self.document.amend_last(command) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
            cx.notify();
            return;
        }

        self.select_slide(slide, window, cx);
        self.set_cell(0, 2);
        self.status = Some(crate::shell::i18n::text("xlsx.recommended-pivot-inserted").into());
        cx.notify();
    }

    /// The table `target` changed by `change`, built again; one step to undo.
    pub(crate) fn change_pivot(&mut self, target: (SlideId, usize), change: impl FnOnce(&mut PivotTable), cx: &mut Context<Self>) {
        let (slide, index) = target;
        let Some(mut pivot) = self.presentation().slide(slide).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None })) else { return };
        if let Some(data) = ibis_pptx::pivot_data(self.presentation(), slide, index) {
            pivot.fill_shared(&data);
        }
        change(&mut pivot);
        let Some(mut sheet) = ibis_pptx::rebuilt_pivot(self.presentation(), slide, index, Some(pivot)) else { return };
        if self.current_sheet_of(slide).is_some_and(|old| covers_others(&old, &sheet, index)) {
            return self.refuse_overlap(cx);
        }
        if sheet.pivots.get(index).is_none_or(|p| p.auto_fit_columns) {
            let default_size = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref()).map_or(11.0, |s| s.font_size as f32 / 100.0);
            let ts = cx.text_system().clone();
            let fitted_widths = fitted_for(&sheet, &[index], default_size, &self.resources, &ts);
        set_fitted_widths(&mut sheet, fitted_widths);
        }
        let mut commands = vec![Command::Sheet(SheetCommand::ReplaceSheet { slide, sheet: Box::new(sheet.clone()) })];
        if let Some(p) = sheet.pivots.get(index) {
            commands.extend(self.sync_pivot_charts_commands(slide, &sheet, p));
        }
        self.status = match self.document.execute(commands) {
            Ok(_) => None,
            Err(err) => Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into()),
        };
        let message = self.status.clone().unwrap_or_default();
        self.pivot_message(message, cx);
    }

    /// Build `SetKind` commands to update all PivotCharts in the workbook linked to `pivot` on `pivot_slide`.
    fn sync_pivot_charts_commands(&self, pivot_slide: SlideId, sheet: &Sheet, pivot: &PivotTable) -> Vec<Command> {
        let Some(data) = ibis_pptx::pivot_source(self.presentation(), sheet, pivot) else {
            return Vec::new();
        };
        let built = pivot.build(&data);
        let pivot_sheet_name = self.sheet_name_of(pivot_slide);
        let mut commands = Vec::new();
        for s in &self.presentation().slides {
            for sh in &s.shapes {
                if let ShapeKind::Chart(c) = &sh.kind {
                    if ibis_doc_model::pivot::pivot_source_matches(c.pivot_source.as_deref(), pivot_sheet_name.as_deref(), &pivot.name) {
                        let mut updated_chart = (**c).clone();
                        ibis_doc_model::pivot::update_chart_from_pivot(&mut updated_chart, pivot, &built);
                        if &updated_chart != c.as_ref() {
                            commands.push(Command::SetKind {
                                slide: s.id,
                                shape: sh.id,
                                kind: Box::new(ShapeKind::Chart(Box::new(updated_chart))),
                            });
                        }
                    }
                }
            }
        }
        commands
    }

    /// Insert a PivotChart linked to the pivot table at `target` (slide, pivot_index).
    pub(crate) fn insert_pivot_chart(&mut self, target: (SlideId, usize), cx: &mut Context<Self>) -> Option<ShapeId> {
        let (slide, index) = target;
        let s = self.presentation().slide(slide)?;
        let pivot = s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(index).cloned() } else { None })?;
        let sheet = s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { Some(s) } else { None })?;
        let data = ibis_pptx::pivot_source(self.presentation(), sheet, &pivot)?;
        let built = pivot.build(&data);
        let chart = ibis_doc_model::pivot::chart_from_pivot(&pivot, &built);

        let (x, y) = self.sheet_insertion_origin();
        let frame = ibis_doc_model::EmuRect::new(x, y, 4_500_000, 3_000_000);
        let id = self.document.ids().new_shape_id();
        let shape_index = self.slide().shapes.len();
        let shape = Shape {
            id,
            frame,
            rotation: 0,
            flip_h: false,
            flip_v: false,
            kind: ShapeKind::Chart(Box::new(chart)),
            floating: None,
        };
        self.run_sheet(vec![Command::InsertShape { slide, index: shape_index, shape }], cx);
        self.selection = Some(id);
        self.status = Some(crate::shell::i18n::text("xlsx.pivot-chart-inserted").into());
        cx.notify();
        Some(id)
    }

    /// 挿入 > ピボットグラフ: create and insert a PivotChart linked to the selected or active pivot table.
    pub(crate) fn open_insert_pivot_chart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        if let Some(index) = self.pivot_at_selection() {
            self.insert_pivot_chart((self.current_slide, index), cx);
            return;
        }
        // If there are pivot tables on the current sheet, pick the first one
        let first_pivot = self.slide().shapes.iter().find_map(|sh| {
            if let ShapeKind::Sheet(s) = &sh.kind {
                if !s.pivots.is_empty() {
                    Some(0)
                } else {
                    None
                }
            } else {
                None
            }
        });
        if let Some(index) = first_pivot {
            self.insert_pivot_chart((self.current_slide, index), cx);
        } else {
            // No pivot table exists, prompt create pivot dialog
            self.open_create_pivot(window, cx);
        }
    }

    /// A field's check box: on, to the rows (a field of numbers to the
    /// values), as Excel does; off, out of the table.
    pub(crate) fn toggle_pivot_field(&mut self, target: (SlideId, usize), f: usize, cx: &mut Context<Self>) {
        self.change_pivot(target, |p| {
            if p.place_of(f).is_some() {
                p.place(f, None, 0);
            } else {
                let numbers = p.fields[f].shared.iter().filter(|v| !matches!(v, ibis_doc_model::CellValue::Empty)).all(|v| matches!(v, ibis_doc_model::CellValue::Number(_)));
                let area = if numbers { PivotAxis::Values } else { PivotAxis::Row };
                p.place(f, Some(area), usize::MAX);
            }
        }, cx);
    }

    pub(crate) fn pivot_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.sheet_state()?.panels.pivot.as_ref()?;
        self.render_pivot_panel(cx)
    }

    #[inline(never)]
    fn render_pivot_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.pivot.as_ref()?;
        let message = p.message.clone();
        if let Some(c) = &p.create {
            let new_sheet = c.new_sheet;
            let radio = |id: &'static str, on: bool, label: &str, value: bool| {
                crate::shell::chrome::radio_button(id, on, false, text(label), cx.listener(move |this, _, _, cx| {
                    if let Some(c) = this.wb_mut().panels.pivot.as_mut().and_then(|p| p.create.as_mut()) {
                        c.new_sheet = value;
                    }
                    cx.notify();
                }))
            };
            let body = v_flex()
                .gap_1()
                .px(px(18.))
                .py(px(8.))
                .p_0().child(text("_xlsx.table-or-range"))
                .child(super::range_field::local_range_field(&c.source,"pivot-c-source",false,cx))
                .child(text("_xlsx.where-to-place-the-pivottable"))
                .child(radio("pivot-new-sheet", new_sheet, "_xlsx.new-worksheet", true))
                .child(radio("pivot-existing", !new_sheet, "_xlsx.existing-worksheet", false))
                .child(super::range_field::local_range_field(&c.at,"pivot-c-at",true,cx))
                .child(div().text_sm().child(SharedString::from(message)))
;
            let footer=crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("pivot-create", "", crate::shell::chrome::ActionButtonKind::Primary).label(text("_xlsx.create")).on_click(cx.listener(|e,_,w,cx|e.create_pivot(w,cx)))).action(crate::shell::chrome::dialog_cancel_definition(cx));
            return Some(SidebarPanel::with_footer("_xlsx.create-pivottable",body.into_any_element(),footer));
        }
        if let Some(cs) = &p.change_source {
            let label = |s: &str| div().text_sm().child(s.to_string());
            let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
            body = body.child(label(&crate::shell::i18n::text("xlsx.pivot-source-range")));
            body = body.child(super::range_field::local_range_field(&cs.source,"pivot-change-source",false,cx));
            if !message.is_empty() {
                body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(message));
            }
            let footer = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("xlsx-pivot-change-src-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|this, _, _, cx| this.apply_change_pivot_source(cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("xlsx-pivot-change-src-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_pivot_panel(cx))))
            ;
            return Some(SidebarPanel::with_footer(crate::shell::i18n::text("xlsx.pivot-change-data-source"), body.into_any_element(), footer));
        }
        if let Some(mv) = &p.move_pivot {
            let label = |s: &str| div().text_sm().child(s.to_string());
            let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
            body = body.child(label(&text("_xlsx.where-to-place-the-pivottable")));
            body = body.child(super::range_field::local_range_field(&mv.destination,"pivot-mv-destination",true,cx));
            if !message.is_empty() {
                body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(message));
            }
            let footer = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("xlsx-pivot-move-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|this, _, window, cx| this.apply_move_pivot(window, cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("xlsx-pivot-move-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_pivot_panel(cx))))
            ;
            return Some(SidebarPanel::with_footer(crate::shell::i18n::text("xlsx.pivot-move"), body.into_any_element(), footer));
        }
        let target = p.target?;
        let selected_value = p.value;
        let selected_field = p.field_selected;
        let pivot = self.presentation().slide(target.0).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(target.1).cloned() } else { None }))?;
        let mut body = v_flex().gap_1().px(px(18.)).py(px(8.)).p_0().child(text("_xlsx.choose-fields-to-add-to-the-report"));
        for (i, f) in pivot.fields.iter().enumerate() {
            let on = pivot.place_of(i).is_some();
            body = body.child(
                crate::shell::chrome::checkbox_button(SharedString::from(format!("pivot-field-{i}")), f.name.clone(), on, false, cx.listener(move |this, _, _, cx| this.toggle_pivot_field(target, i, cx))),
            );
        }
        for (area, label) in AREAS {
            body = body.child(div().pt(px(6.)).child(text(label)));
            let entries: Vec<(usize, String)> = match area {
                PivotAxis::Row => pivot.row_fields.iter().filter(|&&f| f >= 0).map(|&f| (f as usize, pivot.fields[f as usize].name.clone())).collect(),
                PivotAxis::Column => pivot.col_fields.iter().map(|&f| if f < 0 { (usize::MAX, text("_xlsx.values-2").to_string()) } else { (f as usize, pivot.fields[f as usize].name.clone()) }).collect(),
                PivotAxis::Page => pivot.page_fields.iter().map(|p| (p.field as usize, pivot.fields[p.field as usize].name.clone())).collect(),
                PivotAxis::Values => pivot.data_fields.iter().map(|d| (d.field as usize, d.name.clone())).collect(),
            };
            for (pos, (f, name)) in entries.into_iter().enumerate() {
                if f == usize::MAX {
                    body = body.child(div().pl(px(8.)).text_sm().child(SharedString::from(name)));
                    continue;
                }
                let id = |s: &str| SharedString::from(format!("pivot-{label}-{pos}-{s}"));
                let mut row = h_flex().gap_1().flex_wrap().pl(px(8.)).child(div().text_sm().child(SharedString::from(name)));
                if area == PivotAxis::Values {
                    row = row.child(crate::shell::chrome::action_button(id("fn")).small().label(text("fix.pivot.settings")).on_click(cx.listener(move |this, _, window, cx| {
                        let current_name = if let Some(target) = this.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.target) {
                            this.presentation().slide(target.0).and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(target.1).and_then(|p| p.data_fields.get(pos)).map(|d| d.name.clone()) } else { None })).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        if let Some(p) = this.wb_mut().panels.pivot.as_mut() {
                            if p.value == Some(pos) {
                                p.value = None;
                                p.value_name = None;
                            } else {
                                p.value = Some(pos);
                                p.field_selected = None;
                                let state = cx.new(|cx| InputState::new(window, cx));
                                state.update(cx, |i, cx| i.set_value(current_name, window, cx));
                                p.value_name = Some(state);
                            }
                        }
                        cx.notify();
                    })));
                } else if area == PivotAxis::Row || area == PivotAxis::Column {
                    row = row.child(crate::shell::chrome::action_button(id("field-fn")).small().label(text("fix.pivot.settings")).on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.pivot.as_mut() {
                            p.value = None;
                            p.value_name = None;
                            p.field_selected = if p.field_selected == Some((area, pos)) { None } else { Some((area, pos)) };
                        }
                        cx.notify();
                    })));
                }
                row = row
                    .child(crate::shell::chrome::action_button(id("up")).small().label("▲").on_click(cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| p.place(f, Some(area), pos.saturating_sub(1)), cx))))
                    .child(crate::shell::chrome::action_button(id("down")).small().label("▼").on_click(cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| p.place(f, Some(area), pos + 1), cx))));
                for (other, other_label) in AREAS {
                    if other == area || (area == PivotAxis::Values) != (other == PivotAxis::Values) && area == PivotAxis::Values {
                        continue;
                    }
                    row = row.child(crate::shell::chrome::action_button(id(other_label)).small().label(format!("→{}", text(other_label))).on_click(cx.listener(move |this, _, _, cx| {
                        this.change_pivot(target, |p| {
                            if area == PivotAxis::Values {
                                p.place(f, None, 0);
                            }
                            p.place(f, Some(other), usize::MAX)
                        }, cx)
                    })));
                }
                row = row.child(crate::shell::chrome::destructive_button(id("remove"),cx).small().label("✕").on_click(cx.listener(move |this, _, _, cx| {
                    this.change_pivot(target, |p| {
                        if area == PivotAxis::Values {
                            p.data_fields.retain(|d| d.field != f as u32);
                            p.place(f, p.place_of(f).map(|(a, _)| a), usize::MAX);
                        } else {
                            p.place(f, None, 0);
                        }
                    }, cx)
                })));
                body = body.child(row);
                if area == PivotAxis::Values && selected_value == Some(pos) {
                    let current_df = pivot.data_fields.get(pos);
                    let current_fn = current_df.map(|d| d.function.as_str()).unwrap_or("sum");
                    let current_fmt = current_df.and_then(|d| d.number_format.as_deref()).unwrap_or("General");
                    let current_show = current_df.and_then(|d| d.show_as.as_deref()).unwrap_or("normal");

                    let mut settings = v_flex().gap_1().pl(px(16.)).pr(px(8.)).py(px(4.));

                    // 1. 名前 (指定した名前)
                    settings = settings.child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.name-custom-name")));
                    if let Some(input_state) = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.value_name.as_ref()) {
                        let name_row = h_flex().gap_1().child(crate::shell::chrome::text_field(input_state).full_width()).child(
                            crate::shell::chrome::action_button(id("apply-name")).small().label(text("_shell.apply")).on_click(cx.listener(move |this, _, _, cx| {
                                let name = this.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(|p| p.value_name.as_ref()).map(|st| st.read(cx).value().to_string()).unwrap_or_default();
                                this.change_pivot(target, |p| p.set_data_field_name(pos, &name), cx);
                            }))
                        );
                        settings = settings.child(name_row);
                    }

                    // 2. 計算の種類 (集計方法)
                    settings = settings.child(div().pt(px(2.)).text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.summarize-values-by")));
                    let mut fn_choices = h_flex().gap_1().flex_wrap();
                    for (function, label) in ibis_doc_model::pivot::FUNCTIONS {
                        let is_active = current_fn == function;
                        fn_choices = fn_choices.child(
                            crate::shell::chrome::radio_button(id(function), is_active, false, text(label), cx.listener(move |this, _, window, cx| {
                                    this.change_pivot(target, |p| p.set_function(pos, function), cx);
                                    let new_name = this.presentation().slide(target.0)
                                        .and_then(|s| s.shapes.iter().find_map(|sh| if let ShapeKind::Sheet(s) = &sh.kind { s.pivots.get(target.1) } else { None }))
                                        .and_then(|p| p.data_fields.get(pos).map(|df| df.name.clone()));
                                    if let Some(name) = new_name {
                                        if let Some(inp) = this.wb_mut().panels.pivot.as_mut().and_then(|p| p.value_name.as_ref()) {
                                            inp.update(cx, |i, cx| i.set_value(name, window, cx));
                                        }
                                    }
                                }))
                        );
                    }
                    settings = settings.child(fn_choices);

                    // 3. 表示形式
                    settings = settings.child(div().pt(px(2.)).text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.show-values-as")));
                    const PIVOT_NUM_FORMATS: &[(&str, Option<&str>)] = &[
                        ("_xlsx.general", None),
                        ("_xlsx.number", Some("#,##0")),
                        ("_xlsx.number-2-decimals", Some("#,##0.00")),
                        ("_xlsx.currency", Some("\"¥\"#,##0;\"¥\"\\-#,##0")),
                        ("_xlsx.percentage", Some("0.0%")),
                        ("_xlsx.percentage-2-decimals", Some("0.00%")),
                    ];
                    let mut fmt_choices = h_flex().gap_1().flex_wrap();
                    for (label, fmt_code) in PIVOT_NUM_FORMATS {
                        let is_active = match fmt_code {
                            None => current_fmt == "General" || current_df.and_then(|d| d.number_format.as_deref()).is_none(),
                            Some(c) => current_fmt == *c,
                        };
                        let code_owned = fmt_code.map(str::to_string);
                        fmt_choices = fmt_choices.child(
                            crate::shell::chrome::radio_button(id(&format!("fmt-{}", label)), is_active, false, text(label), cx.listener(move |this, _, _, cx| {
                                    let code = code_owned.clone();
                                    this.change_pivot(target, |p| p.set_number_format(pos, code), cx);
                                }))
                        );
                    }
                    settings = settings.child(fmt_choices);

                    // 4. 計算の種類 (値の表示方法 / 比率)
                    settings = settings.child(div().pt(px(2.)).text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.calculation-type-show-values-as")));
                    const SHOW_AS_OPTIONS: &[(&str, Option<&str>)] = &[
                        ("_xlsx.no-calculation", None),
                        ("_xlsx.of-grand-total", Some("percentOfTotal")),
                        ("_xlsx.of-column-total", Some("percentOfCol")),
                        ("_xlsx.of-row-total", Some("percentOfRow")),
                    ];
                    let mut show_choices = h_flex().gap_1().flex_wrap();
                    for (label, show_mode) in SHOW_AS_OPTIONS {
                        let is_active = match show_mode {
                            None => current_show == "normal" || current_df.and_then(|d| d.show_as.as_deref()).is_none(),
                            Some(m) => current_show == *m,
                        };
                        let mode_owned = show_mode.map(str::to_string);
                        show_choices = show_choices.child(
                            crate::shell::chrome::radio_button(id(&format!("show-{}", label)), is_active, false, text(label), cx.listener(move |this, _, _, cx| {
                                    let mode = mode_owned.clone();
                                    this.change_pivot(target, |p| p.set_show_as(pos, mode), cx);
                                }))
                        );
                    }
                    settings = settings.child(show_choices);

                    body = body.child(settings);
                }
                if (area == PivotAxis::Row || area == PivotAxis::Column) && selected_field == Some((area, pos)) {
                    if let Some(target_field) = pivot.fields.get(f) {
                        let mut field_settings = v_flex().gap_1().pl(px(16.)).pr(px(8.)).py(px(4.));

                        // 1. 小計設定
                        field_settings = field_settings.child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.subtotals")));
                        let subtotal_on = target_field.default_subtotal;
                        let subtotal_top = target_field.subtotal_top;
                        field_settings = field_settings.child(
                            crate::shell::chrome::checkbox_button(id("toggle-subtotal"), text("_xlsx.show-subtotals"), subtotal_on, false, cx.listener(move |this, _, _, cx| {
                                    this.change_pivot(target, |p| {
                                        if let Some(fld) = p.fields.get_mut(f) {
                                            fld.default_subtotal = !fld.default_subtotal;
                                        }
                                    }, cx);
                                }))
                        );
                        if subtotal_on {
                            field_settings = field_settings.child(
                                crate::shell::chrome::checkbox_button(id("toggle-subtotal-top"), text("_xlsx.show-subtotals-at-top-of-group"), subtotal_top, false, cx.listener(move |this, _, _, cx| {
                                        this.change_pivot(target, |p| {
                                            if let Some(fld) = p.fields.get_mut(f) {
                                                fld.subtotal_top = !fld.subtotal_top;
                                            }
                                        }, cx);
                                    }))
                            );
                        }

                        // 2. レイアウト形式
                        field_settings = field_settings.child(div().pt(px(2.)).text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.layout")));
                        let is_compact = target_field.compact;
                        let is_outline = target_field.outline;
                        let layout_kind = if is_compact {
                            ReportLayout::Compact
                        } else if is_outline {
                            ReportLayout::Outline
                        } else {
                            ReportLayout::Tabular
                        };

                        let mut layout_choices = h_flex().gap_1().flex_wrap();
                        for (lid, llabel, lkind) in [
                            ("compact", "_xlsx.compact-form", ReportLayout::Compact),
                            ("outline", "_xlsx.outline-form", ReportLayout::Outline),
                            ("tabular", "_xlsx.tabular-form", ReportLayout::Tabular),
                        ] {
                            let active = layout_kind == lkind;
                            layout_choices = layout_choices.child(
                                crate::shell::chrome::radio_button(id(lid), active, false, text(llabel), cx.listener(move |this, _, _, cx| {
                                        this.change_pivot(target, |p| {
                                            if let Some(fld) = p.fields.get_mut(f) {
                                                fld.compact = lkind == ReportLayout::Compact;
                                                fld.outline = lkind != ReportLayout::Tabular;
                                            }
                                        }, cx);
                                    }))
                            );
                        }
                        field_settings = field_settings.child(layout_choices);
                        let repeat_labels = target_field.fill_down_labels.unwrap_or(pivot.fill_down_labels);
                        field_settings = field_settings.child(
                            crate::shell::chrome::checkbox_button(id("toggle-repeat-labels"), text("_xlsx.repeat-item-labels"), repeat_labels, false, cx.listener(move |this, _, _, cx| {
                                    this.change_pivot(target, |p| {
                                        let table_default = p.fill_down_labels;
                                        if let Some(fld) = p.fields.get_mut(f) {
                                            let next = !fld.fill_down_labels.unwrap_or(table_default);
                                            fld.fill_down_labels = Some(next);
                                        }
                                    }, cx);
                                }))
                        );
                        let blank_row = target_field.insert_blank_row;
                        field_settings = field_settings.child(
                            crate::shell::chrome::checkbox_button(id("toggle-blank-row"), text("xlsx.pivot-blank-rows"), blank_row, false, cx.listener(move |this, _, _, cx| {
                                    this.change_pivot(target, |p| {
                                        if let Some(fld) = p.fields.get_mut(f) {
                                            fld.insert_blank_row = !fld.insert_blank_row;
                                        }
                                    }, cx);
                                }))
                        );

                        body = body.child(field_settings);
                    }
                }
            }
        }
        body = body.child(self.pivot_design_section(&pivot, target, cx));
        body = body.child(div().text_sm().child(SharedString::from(message)));
        Some(SidebarPanel{title:crate::shell::i18n::text("xlsx.pivot-fields"),body:body.into_any_element()})
    }
}

/// レポートのレイアウト.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReportLayout {
    Compact,
    Outline,
    Tabular,
}

fn layout_of(p: &PivotTable) -> ReportLayout {
    let first = p.row_fields.first().and_then(|&f| p.fields.get(f.max(0) as usize));
    match first {
        Some(f) if f.compact && f.outline => ReportLayout::Compact,
        Some(f) if f.outline => ReportLayout::Outline,
        Some(_) => ReportLayout::Tabular,
        None if p.compact && p.outline => ReportLayout::Compact,
        None if p.outline => ReportLayout::Outline,
        None => ReportLayout::Tabular,
    }
}

/// The subtotals the row fields show: none, at the bottom of each group, or at its top.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Subtotals {
    Off,
    Bottom,
    Top,
}

fn subtotals_of(p: &PivotTable) -> Subtotals {
    let first = p.row_fields.first().and_then(|&f| p.fields.get(f.max(0) as usize));
    match first {
        Some(f) if !f.default_subtotal => Subtotals::Off,
        Some(f) if f.subtotal_top => Subtotals::Top,
        _ => Subtotals::Bottom,
    }
}

impl SlideEditor {
    /// デザイン: レポートのレイアウト, 小計, 総計, 項目ラベルの繰り返し, 空行の挿入, スタイル オプション, スタイル, and the captions of empty and error values.
    fn pivot_design_section(&mut self, pivot: &PivotTable, target: (SlideId, usize), cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let layout = layout_of(pivot);
        let subtotals = subtotals_of(pivot);
        let blank_rows = pivot.fields.iter().filter(|f| f.axis == Some(PivotAxis::Row)).any(|f| f.insert_blank_row);
        let mut body = v_flex().gap_1().pt(px(8.)).child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(text("_shell.layout")));
        let mut layouts = crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow);
        for (id, label, kind) in [("pivot-layout-compact", "_xlsx.show-in-compact-form", ReportLayout::Compact), ("pivot-layout-outline", "_xlsx.show-in-outline-form", ReportLayout::Outline), ("pivot-layout-tabular", "_xlsx.show-in-tabular-form", ReportLayout::Tabular)] {
            layouts = layouts.child(crate::shell::chrome::radio_button(id, layout == kind, false, text(label), cx.listener(move |this, _, _, cx| {
                this.change_pivot(target, |p| {
                    p.compact = kind == ReportLayout::Compact;
                    p.outline = kind != ReportLayout::Tabular;
                    for f in &mut p.fields {
                        f.compact = p.compact;
                        f.outline = p.outline;
                    }
                }, cx)
            })));
        }
        body = body.child(layouts);
        let mut subs = crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow);
        for (id, label, kind) in [("pivot-subtotal-off", "_xlsx.do-not-show-subtotals", Subtotals::Off), ("pivot-subtotal-bottom", "_xlsx.show-all-subtotals-at-bottom-of-group", Subtotals::Bottom), ("pivot-subtotal-top", "_xlsx.show-all-subtotals-at-top-of-group", Subtotals::Top)] {
            subs = subs.child(crate::shell::chrome::radio_button(id, subtotals == kind, false, text(label), cx.listener(move |this, _, _, cx| {
                this.change_pivot(target, |p| {
                    for f in &mut p.fields {
                        f.default_subtotal = kind != Subtotals::Off;
                        f.subtotal_top = kind == Subtotals::Top;
                    }
                }, cx)
            })));
        }
        body = body.child(subs);
        body = body
            .child(crate::shell::chrome::checkbox_button("pivot-grand-rows", text("_xlsx.show-grand-total-row"), pivot.row_grand_totals, false, cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| p.row_grand_totals = !p.row_grand_totals, cx))))
            .child(crate::shell::chrome::checkbox_button("pivot-grand-cols", text("_xlsx.show-grand-total-column"), pivot.col_grand_totals, false, cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| p.col_grand_totals = !p.col_grand_totals, cx))))
            .child(crate::shell::chrome::checkbox_button("pivot-fill-down", text("_xlsx.repeat-all-item-labels"), pivot.fill_down_labels, false, cx.listener(move |this, _, _, cx| this.change_pivot(target, |p| {
                p.fill_down_labels = !p.fill_down_labels;
                for f in &mut p.fields {
                    f.fill_down_labels = None;
                }
            }, cx))))
            .child(crate::shell::chrome::checkbox_button("pivot-blank-rows", text("xlsx.pivot-blank-rows"), blank_rows, false, cx.listener(move |this, _, _, cx| this.change_pivot(target, move |p| {
                for f in &mut p.fields {
                    if f.axis == Some(PivotAxis::Row) {
                        f.insert_blank_row = !blank_rows;
                    }
                }
            }, cx))));
        let (row_headers, col_headers, banded_rows, banded_cols, cur_style) = match pivot.style.as_ref() {
            Some(st) => (st.row_headers, st.col_headers, st.row_stripes, st.col_stripes, st.name.as_str()),
            None => (true, true, false, false, ""),
        };
        let mut style_opts = h_flex().gap_1().flex_wrap();
        style_opts = style_opts
            .child(crate::shell::chrome::checkbox_button("pivot-row-headers", text("xlsx.pivot-row-headers"), row_headers, false, cx.listener(move |this, _, _, cx| this.change_pivot(target, move |p| {
                let st = p.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo { name: "PivotStyleLight16".into(), row_headers: true, col_headers: true, ..Default::default() });
                st.row_headers = !row_headers;
            }, cx))))
            .child(crate::shell::chrome::checkbox_button("pivot-col-headers", text("xlsx.pivot-col-headers"), col_headers, false, cx.listener(move |this, _, _, cx| this.change_pivot(target, move |p| {
                let st = p.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo { name: "PivotStyleLight16".into(), row_headers: true, col_headers: true, ..Default::default() });
                st.col_headers = !col_headers;
            }, cx))))
            .child(crate::shell::chrome::checkbox_button("pivot-banded-rows", text("_xlsx.banded-rows"), banded_rows, false, cx.listener(move |this, _, _, cx| this.change_pivot(target, move |p| {
                let st = p.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo { name: "PivotStyleLight16".into(), row_headers: true, col_headers: true, ..Default::default() });
                st.row_stripes = !banded_rows;
            }, cx))))
            .child(crate::shell::chrome::checkbox_button("pivot-banded-columns", text("_xlsx.banded-columns"), banded_cols, false, cx.listener(move |this, _, _, cx| this.change_pivot(target, move |p| {
                let st = p.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo { name: "PivotStyleLight16".into(), row_headers: true, col_headers: true, ..Default::default() });
                st.col_stripes = !banded_cols;
            }, cx))));
        body = body
            .child(div().pt(px(2.)).font_weight(gpui_kit::FontWeight::SEMIBOLD).child(text("_xlsx.table-style-options")))
            .child(style_opts);
        let mut style_buttons = crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow);
        for &(id, style_name, group_label) in &PIVOT_STYLES {
            let btn_id = SharedString::from(id.replace("xlsx.pivot-style.", "pivot-style-"));
            let label = if style_name.is_empty() {
                text(group_label)
            } else {
                let num = style_name.trim_start_matches(|c: char| !c.is_ascii_digit());
                SharedString::from(format!("{} {num}", text(group_label)))
            };
            let active = cur_style == style_name;
            style_buttons = style_buttons.child(crate::shell::chrome::radio_button(btn_id, active, false, label, cx.listener(move |this, _, _, cx| {
                this.change_pivot(target, move |p| {
                    if style_name.is_empty() {
                        p.style = None;
                    } else {
                        let st = p.style.get_or_insert_with(|| ibis_doc_model::pivot::PivotStyleInfo { name: style_name.to_string(), row_headers: true, col_headers: true, ..Default::default() });
                        st.name = style_name.to_string();
                    }
                }, cx);
            })));
        }
        body = body
            .child(div().pt(px(2.)).font_weight(gpui_kit::FontWeight::SEMIBOLD).child(text("xlsx.pivot-styles")))
            .child(style_buttons);
        let inputs = self.sheet_state().and_then(|s| s.panels.pivot.as_ref()).map(|p| (p.missing.clone(), p.error.clone()));
        if let Some((Some(missing), Some(error))) = inputs {
            body = body
                .child(text("_xlsx.for-empty-cells-show"))
                .child(crate::shell::chrome::text_field(&missing).full_width())
                .child(text("_xlsx.for-error-values-show"))
                .child(crate::shell::chrome::text_field(&error).full_width())
                .child(crate::shell::chrome::action_button("pivot-captions").small().label(text("_xlsx.apply-captions")).on_click(cx.listener(move |this, _, _, cx| {
                    let read = |this: &Self, pick: fn(&PivotPanel) -> Option<Entity<InputState>>, cx: &Context<Self>| this.sheet_state().and_then(|s| s.panels.pivot.as_ref()).and_then(pick).map(|e| e.read(cx).value().to_string()).unwrap_or_default();
                    let (m, e) = (read(this, |p| p.missing.clone(), cx), read(this, |p| p.error.clone(), cx));
                    this.change_pivot(target, move |p| {
                        p.missing_caption = (!m.is_empty()).then_some(m);
                        p.error_caption = (!e.is_empty()).then_some(e);
                    }, cx)
                })));
        }
        let show_drill = pivot.show_drill;
        let mut actions = h_flex().gap_1().flex_wrap().pt(px(4.));
        actions = actions
            .child(crate::shell::chrome::checkbox_button("pivot-panel-drill-buttons", text("xlsx.pivot-drill-buttons"), show_drill, false, cx.listener(move |this, _, _, cx| {
                this.change_pivot(target, move |p| p.show_drill = !show_drill, cx);
            })))
            .child(crate::shell::chrome::action_button("pivot-panel-change-source").small().label(text("xlsx.pivot-change-data-source")).on_click(cx.listener(|this, _, window, cx| this.open_pivot_change_source(window, cx))))
            .child(crate::shell::chrome::action_button("pivot-panel-move").small().label(text("xlsx.pivot-move")).on_click(cx.listener(|this, _, window, cx| this.open_pivot_move(window, cx))))
            .child(crate::shell::chrome::destructive_button("pivot-panel-clear-filters",cx).small().label(text("xlsx.pivot-clear-filters")).on_click(cx.listener(|this, _, _, cx| this.clear_pivot_filters(cx))))
            .child(crate::shell::chrome::destructive_button("pivot-panel-clear-all",cx).small().label(text("xlsx.pivot-clear-all")).on_click(cx.listener(|this, _, _, cx| this.clear_pivot_all(cx))))
            .child(crate::shell::chrome::action_button("pivot-panel-insert-chart").small().label(text("xlsx.pivot-chart-insert")).on_click(cx.listener(move |this, _, _, cx| {
                this.insert_pivot_chart(target, cx);
            })))
            .child(crate::shell::chrome::action_button("pivot-panel-insert-slicer").small().label(text("xlsx.slicer-insert")).on_click(cx.listener(|this, _, window, cx| this.open_slicer_insert_panel(window, cx))))
            .child(crate::shell::chrome::action_button("pivot-panel-insert-timeline").small().label(text("xlsx.timeline-insert")).on_click(cx.listener(|this, _, window, cx| this.open_timeline_insert_panel(window, cx))));
        body = body.child(actions);
        body.into_any_element()
    }
}

/// The cells a pivot table covers: its place and its report filters'.
fn pivot_area(p: &ibis_doc_model::PivotTable) -> Vec<ibis_doc_model::CellRange> {
    let loc = p.location;
    let pages = p.page_fields.len() as u32;
    let mut out = vec![loc];
    if pages > 0 {
        out.push(ibis_doc_model::CellRange { first_row: loc.first_row.saturating_sub(pages + 1), last_row: loc.first_row.saturating_sub(1), first_column: loc.first_column, last_column: loc.first_column + 1 });
    }
    out
}

/// Whether table `index` as built on `new` would cover cells of `old`
/// with something in them that are not its own (Excel will not write a
/// pivot table over them).
fn covers_others(old: &Sheet, new: &Sheet, index: usize) -> bool {
    let Some(p) = new.pivots.get(index) else { return false };
    let inside = |areas: &[ibis_doc_model::CellRange], r: u32, c: u32| areas.iter().any(|a| (a.first_row..=a.last_row).contains(&r) && (a.first_column..=a.last_column).contains(&c));
    let area = pivot_area(p);
    let own = old.pivots.get(index).map(pivot_area).unwrap_or_default();
    let others: Vec<ibis_doc_model::CellRange> = old.pivots.iter().enumerate().filter(|(i, _)| *i != index).flat_map(|(_, q)| pivot_area(q)).collect();
    if others.iter().any(|o| area.iter().any(|a| a.first_row <= o.last_row && o.first_row <= a.last_row && a.first_column <= o.last_column && o.first_column <= a.last_column)) {
        return true;
    }
    old.cells.iter().any(|c| (!c.value.is_empty() || c.formula.is_some()) && inside(&area, c.row, c.column) && !inside(&own, c.row, c.column))
}

/// A sheet's name as a reference writes it (quoted when it must be).
fn quote_sheet(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        name.to_string()
    } else {
        format!("'{}'", name.replace('\'', "''"))
    }
}

/// `$A$1:$G$41`.
fn absolute(r: ibis_doc_model::CellRange) -> String {
    let cell = |c: u32, row: u32| {
        let name = ibis_doc_model::cell_name(c, row);
        let split = name.find(|ch: char| ch.is_ascii_digit()).unwrap_or(name.len());
        format!("${}${}", &name[..split], &name[split..])
    };
    format!("{}:{}", cell(r.first_column, r.first_row), cell(r.last_column, r.last_row))
}

/// A pivot button's (▼) panel: a field's items to show (ticked) and its
/// order, or a report filter's one item.
pub(crate) struct PivotPopup {
    pub(crate) target: (SlideId, usize),
    /// The fields it is for (行ラベル: the row fields), and the one shown.
    pub(crate) fields: Vec<usize>,
    pub(crate) field: usize,
    /// A report filter: its place among them.
    pub(crate) page: Option<usize>,
    pub(crate) position: gpui_kit::Point<gpui_kit::Pixels>,
    /// Each item of the field (by its place in its items): ticked to show,
    /// or (a report filter) the one chosen (`None`: すべて).
    pub(crate) ticked: Vec<bool>,
    pub(crate) chosen: Option<usize>,
    /// 検索.
    pub(crate) search: Entity<InputState>,
    /// ラベル フィルター ▸ / 値フィルター ▸ opened.
    pub(crate) section: Option<PopupSection>,
    /// The filter being asked for (its value, or the トップテン's).
    pub(crate) dialog: Option<PopupDialog>,
}

/// A part of the panel opened under its heading, as the オートフィルター's.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopupSection {
    Label,
    Value,
    Sort,
}

/// ラベル フィルター / 値フィルター / トップテン: what is asked.
pub(crate) struct PopupDialog {
    pub(crate) label: bool,
    /// The filter's type (`captionEqual`, …, `count`).
    pub(crate) kind: &'static str,
    /// The value filtered by.
    pub(crate) measure: usize,
    pub(crate) values: [Entity<InputState>; 2],
    /// トップテン: the top, or the bottom; 項目・パーセント・合計.
    pub(crate) top: bool,
    pub(crate) top_kind: &'static str,
    pub(crate) error: Option<SharedString>,
}

/// The label filters of the panel, as Excel lists them, and the value filters.
const LABEL_FILTERS: &[(&str, &str)] = &[
    ("_xlsx.equal-to-2", "captionEqual"),
    ("_xlsx.not-equal-to-2", "captionNotEqual"),
    ("_xlsx.begins-with-2", "captionBeginsWith"),
    ("_xlsx.does-not-begin-with-2", "captionNotBeginsWith"),
    ("_xlsx.ends-with-2", "captionEndsWith"),
    ("_xlsx.does-not-end-with-2", "captionNotEndsWith"),
    ("_xlsx.contains-2", "captionContains"),
    ("_xlsx.does-not-contain-2", "captionNotContains"),
    ("_xlsx.greater-than-3", "captionGreaterThan"),
    ("_xlsx.greater-than-or-equal-to-3", "captionGreaterThanOrEqual"),
    ("_xlsx.less-than-3", "captionLessThan"),
    ("_xlsx.less-than-or-equal-to-3", "captionLessThanOrEqual"),
    ("_xlsx.between-2", "captionBetween"),
    ("_xlsx.not-between-2", "captionNotBetween"),
];
const VALUE_FILTERS: &[(&str, &str)] = &[
    ("_xlsx.equal-to-2", "valueEqual"),
    ("_xlsx.not-equal-to-2", "valueNotEqual"),
    ("_xlsx.greater-than-3", "valueGreaterThan"),
    ("_xlsx.greater-than-or-equal-to-3", "valueGreaterThanOrEqual"),
    ("_xlsx.less-than-3", "valueLessThan"),
    ("_xlsx.less-than-or-equal-to-3", "valueLessThanOrEqual"),
    ("_xlsx.between-2", "valueBetween"),
    ("_xlsx.not-between-2", "valueNotBetween"),
    ("_xlsx.top-10", "count"),
];

/// A field's items as listed: their place among its items and their text.
fn item_labels(p: &PivotTable, f: usize) -> Vec<(usize, String)> {
    let Some(field) = p.fields.get(f) else { return Vec::new() };
    field
        .items
        .iter()
        .enumerate()
        .filter(|(_, it)| it.kind.is_none())
        .filter_map(|(i, it)| {
            let v = field.shared.get(it.x? as usize)?;
            let t = ibis_doc_model::sheet_command::display_text(v, &Default::default(), false);
            Some((i, if t.is_empty() { crate::shell::i18n::text("_xlsx.blank-2").to_string() } else { t }))
        })
        .collect()
}

impl SlideEditor {
    pub(crate) fn pivot_def(&self, target: (SlideId, usize)) -> Option<PivotTable> {
        self.current_sheet_of(target.0).and_then(|s| s.pivots.get(target.1).cloned())
    }

    /// A pivot button pressed: its panel, for `button`.
    pub(crate) fn open_pivot_popup(&mut self, target: (SlideId, usize), button: ibis_doc_model::pivot::PivotButton, position: gpui_kit::Point<gpui_kit::Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        use ibis_doc_model::pivot::PivotButton as B;
        self.commit_cell_edit(cx);
        let Some(p) = self.pivot_def(target) else { return };
        let real = |list: &[i32]| list.iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect::<Vec<_>>();
        let (fields, page) = match button {
            B::Rows => (real(&p.row_fields), None),
            B::Columns => (real(&p.col_fields), None),
            B::Field(f) => (vec![f], None),
            B::Page(i) => (p.page_fields.get(i).map(|pf| vec![pf.field as usize]).unwrap_or_default(), Some(i)),
        };
        let Some(&field) = fields.first() else { return };
        let search = cx.new(|cx| InputState::new(window, cx));
        let mut popup = PivotPopup { target, fields, field, page, position, ticked: Vec::new(), chosen: None, search, section: None, dialog: None };
        self.fill_popup(&p, &mut popup);
        self.wb_mut().panels.pivot_popup = Some(popup);
        cx.notify();
    }

    fn fill_popup(&self, p: &PivotTable, popup: &mut PivotPopup) {
        let field = &p.fields[popup.field];
        popup.ticked = field.items.iter().map(|it| !it.hidden).collect();
        popup.chosen = popup.page.and_then(|i| p.page_fields.get(i)).and_then(|pf| pf.item).map(|i| i as usize);
    }

    /// Another field of 行ラベル's (列ラベル's) panel shown.
    pub(crate) fn pivot_popup_field(&mut self, field: usize, cx: &mut Context<Self>) {
        let Some(target) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| p.target) else { return };
        let Some(p) = self.pivot_def(target) else { return };
        let mut popup = self.wb_mut().panels.pivot_popup.take().expect("open");
        popup.field = field;
        self.fill_popup(&p, &mut popup);
        self.wb_mut().panels.pivot_popup = Some(popup);
        cx.notify();
    }

    pub(crate) fn close_pivot_popup(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.panels.pivot_popup = None;
        }
        cx.notify();
    }

    /// Tick or untick item `i` (`None`: すべて選択).
    pub(crate) fn pivot_popup_tick(&mut self, i: Option<usize>, on: bool, cx: &mut Context<Self>) {
        // With something searched for, すべて選択 is of the items found.
        let shown = self.sheet_state().and_then(|st| st.panels.pivot_popup.as_ref()).and_then(|popup| {
            let query = popup.search.read(cx).value().trim().to_lowercase();
            let p = self.pivot_def(popup.target)?;
            (!query.is_empty()).then(|| item_labels(&p, popup.field).into_iter().filter(|(_, l)| l.to_lowercase().contains(&query)).map(|(i, _)| i).collect::<Vec<_>>())
        });
        if let Some(p) = self.wb_mut().panels.pivot_popup.as_mut() {
            match i {
                Some(i) => {
                    if let Some(t) = p.ticked.get_mut(i) {
                        *t = on;
                    }
                }
                None => match shown {
                    Some(shown) => shown.iter().for_each(|&i| {
                        if let Some(t) = p.ticked.get_mut(i) {
                            *t = on;
                        }
                    }),
                    None => p.ticked.iter_mut().for_each(|t| *t = on),
                },
            }
        }
        cx.notify();
    }

    /// A report filter's item chosen (`None`: すべて).
    pub(crate) fn pivot_popup_choose(&mut self, i: Option<usize>, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot_popup.as_mut() {
            p.chosen = i;
        }
        cx.notify();
    }

    /// 昇順・降順: the field's items sorted by label (the table built again).
    pub(crate) fn pivot_popup_sort(&mut self, ascending: bool, cx: &mut Context<Self>) {
        let Some((target, f)) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| (p.target, p.field)) else { return };
        self.close_pivot_popup(cx);
        self.change_pivot(target, move |p| p.sort_field_by_label(f, ascending), cx);
    }

    /// 値で並べ替え: the field's items sorted by data field `df_idx` (the table built again).
    pub(crate) fn pivot_popup_sort_by_value(&mut self, df_idx: usize, ascending: bool, cx: &mut Context<Self>) {
        let Some((target, f)) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| (p.target, p.field)) else { return };
        self.close_pivot_popup(cx);
        self.change_pivot(target, move |p| p.sort_field_by_value(f, df_idx, ascending), cx);
    }

    /// OK: the items ticked shown (at least one), or the report filter's
    /// item kept.
    pub(crate) fn pivot_popup_ok(&mut self, cx: &mut Context<Self>) {
        let Some(popup) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()) else { return };
        let (target, f, page, chosen, ticked) = (popup.target, popup.field, popup.page, popup.chosen, popup.ticked.clone());
        let Some(p) = self.pivot_def(target) else { return };
        if page.is_none() && !p.fields[f].items.iter().zip(&ticked).any(|(it, &t)| it.kind.is_none() && t) {
            return;
        }
        self.close_pivot_popup(cx);
        self.change_pivot(target, move |p| match page {
            Some(i) => {
                if let Some(pf) = p.page_fields.get_mut(i) {
                    pf.item = chosen.map(|c| c as u32);
                }
            }
            None => {
                for (it, t) in p.fields[f].items.iter_mut().zip(ticked) {
                    if it.kind.is_none() {
                        it.hidden = !t;
                    }
                }
            }
        }, cx);
    }

    pub(crate) fn pivot_popup_section(&mut self, section: PopupSection, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot_popup.as_mut() {
            p.section = if p.section == Some(section) { None } else { Some(section) };
        }
        cx.notify();
    }

    /// ラベル フィルター / 値フィルター の 1 つを選んだ: its value asked for (what the field has now in it).
    pub(crate) fn pivot_popup_ask(&mut self, label: bool, kind: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, field)) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| (p.target, p.field)) else { return };
        let Some(p) = self.pivot_def(target) else { return };
        let current = p.filters.iter().find(|f| f.field as usize == field && f.is_label() == label).cloned();
        let same = current.as_ref().filter(|f| f.kind == kind || (kind == "count" && matches!(f.kind.as_str(), "count" | "percent" | "sum")));
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let values = [input(same.map(|f| f.value.clone()).unwrap_or_else(|| if kind == "count" { "10".into() } else { String::new() }), window, cx), input(same.and_then(|f| f.value2.clone()).unwrap_or_default(), window, cx)];
        let top_kind = match same.map(|f| f.kind.as_str()) {
            Some("percent") => "percent",
            Some("sum") => "sum",
            _ => "count",
        };
        let dialog = PopupDialog { label, kind, measure: same.map_or(0, |f| f.measure as usize), values, top: same.is_none_or(|f| f.top), top_kind, error: None };
        if let Some(popup) = self.wb_mut().panels.pivot_popup.as_mut() {
            popup.dialog = Some(dialog);
        }
        cx.notify();
    }

    pub(crate) fn close_pivot_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.pivot_popup.as_mut() {
            p.dialog = None;
        }
        cx.notify();
    }

    pub(crate) fn with_pivot_dialog(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut PopupDialog)) {
        if let Some(d) = self.wb_mut().panels.pivot_popup.as_mut().and_then(|p| p.dialog.as_mut()) {
            f(d);
        }
        cx.notify();
    }

    /// OK of the ラベル フィルター / 値フィルター / トップテン dialog: the filter set on the field (the table built again).
    pub(crate) fn pivot_popup_dialog_ok(&mut self, cx: &mut Context<Self>) {
        let Some(popup) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()) else { return };
        let Some(d) = popup.dialog.as_ref() else { return };
        let (target, field) = (popup.target, popup.field as u32);
        let (label, mut kind, measure, top) = (d.label, d.kind, d.measure as u32, d.top);
        let (a, b) = (d.values[0].read(cx).value().trim().to_string(), d.values[1].read(cx).value().trim().to_string());
        if kind == "count" {
            kind = d.top_kind;
        }
        let between = kind.ends_with("Between") || kind.ends_with("NotBetween");
        let numeric = !label || matches!(kind, "count" | "percent" | "sum");
        let bad = |t: &str| t.is_empty() || (numeric && t.parse::<f64>().is_err());
        let error = if bad(&a) || (between && bad(&b)) {
            Some(text(if numeric { "_xlsx.enter-a-number" } else { "_xlsx.enter-a-value" }))
        } else if matches!(kind, "count" | "percent" | "sum") && a.parse::<f64>().is_ok_and(|n| n <= 0.0) {
            Some(text("_xlsx.enter-a-number-greater-than-0"))
        } else {
            None
        };
        if let Some(e) = error {
            return self.with_pivot_dialog(cx, |d| d.error = Some(e));
        }
        self.close_pivot_popup(cx);
        let filter = ibis_doc_model::PivotFilter { field, kind: kind.to_string(), measure, value: a, value2: between.then_some(b), top };
        self.change_pivot(target, move |p| {
            p.filters.retain(|f| !(f.field == field && f.is_label() == label));
            p.filters.push(filter);
        }, cx);
    }

    /// "…" からフィルターをクリア: the field's filters and hidden items gone.
    pub(crate) fn pivot_popup_clear(&mut self, cx: &mut Context<Self>) {
        let Some((target, f)) = self.sheet_state().and_then(|s| s.panels.pivot_popup.as_ref()).map(|p| (p.target, p.field)) else { return };
        self.close_pivot_popup(cx);
        self.change_pivot(target, move |p| {
            p.filters.retain(|x| x.field as usize != f);
            for it in &mut p.fields[f].items {
                it.hidden = false;
            }
        }, cx);
    }

    /// The pivot buttons of the current sheet, over their cells' right ends.
    pub(crate) fn pivot_buttons(&self, cx: &mut Context<Self>) -> Vec<gpui_kit::AnyElement> {
        use gpui_kit::{InteractiveElement as _, MouseButton, MouseDownEvent, rgb};
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let Some(g) = self.sheet_state().map(|s| s.header_geometry.borrow().clone()) else { return Vec::new() };
        let (ox, oy) = g.origin;
        let slide = self.current_slide;
        let mut out = Vec::new();
        for (index, p) in sheet.pivots.iter().enumerate() {
            for (row, column, button) in p.button_places() {
                let Some(&(_, top, bottom)) = g.rows.iter().find(|r| r.0 == row) else { continue };
                let Some(&(_, _, right)) = g.columns.iter().find(|c| c.0 == column) else { continue };
                let side = (bottom - top).clamp(12.0, 18.0);
                let filtered = match button {
                    ibis_doc_model::pivot::PivotButton::Page(i) => p.page_fields.get(i).is_some_and(|pf| pf.item.is_some()),
                    _ => {
                        let fields: Vec<usize> = match button {
                            ibis_doc_model::pivot::PivotButton::Rows => p.row_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect(),
                            ibis_doc_model::pivot::PivotButton::Columns => p.col_fields.iter().filter(|&&f| f >= 0).map(|&f| f as usize).collect(),
                            ibis_doc_model::pivot::PivotButton::Field(f) => vec![f],
                            ibis_doc_model::pivot::PivotButton::Page(_) => Vec::new(),
                        };
                        fields.iter().any(|&f| p.fields.get(f).is_some_and(|fl| fl.items.iter().any(|i| i.hidden)) || p.filters.iter().any(|x| x.field as usize == f))
                    }
                };
                out.push(
                    div()
                        .id(SharedString::from(format!("pivot-button-{index}-{row}-{column}")))
                        .absolute()
                        .left(px(right - ox - side - 2.0))
                        .top(px(bottom - oy - side - 2.0))
                        .size(px(side))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgb(0xA6A6A6))
                        .text_size(px(side * 0.55))
                        .text_color(if filtered { rgb(0x217346) } else { rgb(0x404040) })
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_pivot_popup((slide, index), button, e.position, window, cx);
                        }))
                        .child(if filtered { "▼●" } else { "▼" })
                        .into_any_element(),
                );
            }
        }
        out
    }

    /// ラベル フィルター / 値フィルター / トップテン: the value asked for, as the オートフィルターの 指定の値… dialog.
    fn render_pivot_filter_dialog(&self, p: &PivotTable, field: usize, d: &PopupDialog, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        use gpui_kit::component::ActiveTheme as _;
        let theme = cx.theme().clone();
        let name = p.fields[field].name.clone();
        let title = if d.label { "_xlsx.label-filters" } else { "_xlsx.value-filters" };
        let top = d.kind == "count";
        let mut body = v_flex().gap_2().child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(SharedString::from(format!("{} ({name})", text(title)))));
        if !d.label && p.data_fields.len() > 1 {
            let mut choose = crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow);
            for (i, df) in p.data_fields.iter().enumerate() {
                choose = choose.child(crate::shell::chrome::radio_button(SharedString::from(format!("pivot-dialog-measure-{i}")), d.measure == i, false, df.name.clone(), cx.listener(move |this, _, _, cx| this.with_pivot_dialog(cx, |d| d.measure = i))));
            }
            body = body.child(div().child(text("_xlsx.values"))).child(choose);
        }
        if top {
            let kinds: [(&'static str, &str); 3] = [("count", "pptx.pivot.category"), ("percent", "_xlsx.percent-2"), ("sum", "_xlsx.sum")];
            let mut unit = crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow);
            for (kind, label) in kinds {
                unit = unit.child(crate::shell::chrome::radio_button(SharedString::from(format!("pivot-dialog-{kind}")), d.top_kind == kind, false, text(label), cx.listener(move |this, _, _, cx| this.with_pivot_dialog(cx, |d| d.top_kind = kind))));
            }
            body = body
                .child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow).child(crate::shell::chrome::radio_button("pivot-dialog-top", d.top, false, text("_xlsx.top"), cx.listener(|this, _, _, cx| this.with_pivot_dialog(cx, |d| d.top = true)))).child(crate::shell::chrome::radio_button("pivot-dialog-bottom", !(d.top), false, text("_xlsx.bottom"), cx.listener(|this, _, _, cx| this.with_pivot_dialog(cx, |d| d.top = false)))))
                .child(match d.top_kind {
                    // トップテン: how many items (1–500) or what percent (1–100) to show, read when OK is pressed. A sum is any value.
                    "count" | "percent" => crate::shell::chrome::text_field(&d.values[0])
                        .full_width()
                        .spinner("pivot-dialog-top-value", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, if d.top_kind == "count" { 500.0 } else { 100.0 }).integer_only(), cx, |_, _, _, _| {}),
                    _ => crate::shell::chrome::text_field(&d.values[0]).full_width(),
                })
                .child(unit);
        } else {
            let label = LABEL_FILTERS.iter().chain(VALUE_FILTERS).find(|(_, k)| *k == d.kind).map_or("", |(l, _)| l.trim_end_matches('…'));
            body = body.child(div().child(text(label))).child(crate::shell::chrome::text_field(&d.values[0]).full_width());
            if d.kind.ends_with("Between") {
                body = body.child(div().child(text("_xlsx.and"))).child(crate::shell::chrome::text_field(&d.values[1]).full_width());
            }
            if d.label {
                body = body.child(div().text_color(theme.muted_foreground).child(text("_xlsx.use-to-represent-any-single-character-use-to-rep")));
            }
        }
        body.when_some(d.error.clone(), |b, e| b.child(div().text_color(theme.danger).child(e)))
            .child(
                crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("pivot-dialog-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|this, _, _, cx| this.pivot_popup_dialog_ok(cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("pivot-dialog-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_pivot_dialog(cx)))))),
            )
            .into_any_element()
    }

    /// The open pivot button's panel.
    pub(crate) fn render_pivot_popup(&self, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        use gpui_kit::component::ActiveTheme as _;
        use crate::shell::chrome::Checkbox;
        use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _, anchored, deferred};
        let popup = self.sheet_state()?.panels.pivot_popup.as_ref()?;
        let p = self.pivot_def(popup.target)?;
        let theme = cx.theme().clone();
        let items = item_labels(&p, popup.field);
        let mut body = v_flex().gap_1();
        if popup.fields.len() > 1 {
            let mut choose = h_flex().gap_1().flex_wrap().child(text("_xlsx.select-field"));
            for &f in &popup.fields {
                let on = f == popup.field;
                choose = choose.child(crate::shell::chrome::radio_button(SharedString::from(format!("pivot-popup-field-{f}")), on, false, p.fields[f].name.clone(), cx.listener(move |this, _, _, cx| this.pivot_popup_field(f, cx))));
            }
            body = body.child(choose);
        }
        if let Some(_) = popup.page {
            let chosen = popup.chosen;
            body = body.child(crate::shell::chrome::radio_button("pivot-popup-all", chosen.is_none(), false, text("_xlsx.all-2"), cx.listener(|this, _, _, cx| this.pivot_popup_choose(None, cx))));
            for (i, label) in items {
                body = body.child(crate::shell::chrome::radio_button(SharedString::from(format!("pivot-popup-item-{i}")), chosen == Some(i), false, label, cx.listener(move |this, _, _, cx| this.pivot_popup_choose(Some(i), cx))));
            }
        } else if popup.dialog.is_none() {
            // As the オートフィルター's panel: sorting, clearing, the label and value filters, the search box and the items.
            let name = p.fields[popup.field].name.clone();
            let filtered = p.filters.iter().any(|f| f.field as usize == popup.field) || p.fields[popup.field].items.iter().any(|it| it.hidden);
            let arrow = |open: bool| if open { " ▾" } else { " ▸" };
            let mut menu = v_flex()
                .child(self.panel_row("pivot-popup-asc", "_xlsx.sort-ascending", true, Rc::new(|this, _, cx| this.pivot_popup_sort(true, cx)), cx))
                .child(self.panel_row("pivot-popup-desc", "_xlsx.sort-descending", true, Rc::new(|this, _, cx| this.pivot_popup_sort(false, cx)), cx));
            if !p.data_fields.is_empty() {
                menu = menu.child(self.panel_row("pivot-popup-sort-value", format!("{}{}", text("_xlsx.sort-by-value"), arrow(popup.section == Some(PopupSection::Sort))), true, Rc::new(|this, _, cx| this.pivot_popup_section(PopupSection::Sort, cx)), cx));
                if popup.section == Some(PopupSection::Sort) {
                    let mut sort_list = v_flex().pl_3();
                    for (df_idx, df) in p.data_fields.iter().enumerate() {
                        let df_name = &df.name;
                        let asc_label = crate::shell::i18n::format("_xlsx.df-name-ascending", &[("df_name", &df_name)]);
                        let desc_label = crate::shell::i18n::format("_xlsx.df-name-descending", &[("df_name", &df_name)]);
                        sort_list = sort_list
                            .child(self.panel_row(format!("pivot-popup-sort-val-asc-{df_idx}"), asc_label, true, Rc::new(move |this, _, cx| this.pivot_popup_sort_by_value(df_idx, true, cx)), cx))
                            .child(self.panel_row(format!("pivot-popup-sort-val-desc-{df_idx}"), desc_label, true, Rc::new(move |this, _, cx| this.pivot_popup_sort_by_value(df_idx, false, cx)), cx));
                    }
                    menu = menu.child(sort_list);
                }
            }
            menu = menu
                .child(div().h(px(1.)).my_1().bg(theme.border))
                .child(self.panel_row("pivot-popup-clear", crate::shell::i18n::format("_xlsx.clear-filter-from-name", &[("name", &name)]), filtered, Rc::new(|this, _, cx| this.pivot_popup_clear(cx)), cx))
                .child(self.panel_row("pivot-popup-label-filters", format!("{}{}", text("_xlsx.label-filters"), arrow(popup.section == Some(PopupSection::Label))), true, Rc::new(|this, _, cx| this.pivot_popup_section(PopupSection::Label, cx)), cx));
            if popup.section == Some(PopupSection::Label) {
                let mut list = v_flex().pl_3();
                for &(label, kind) in LABEL_FILTERS {
                    list = list.child(self.panel_row(format!("pivot-popup-{kind}"), label, true, Rc::new(move |this, window, cx| this.pivot_popup_ask(true, kind, window, cx)), cx));
                }
                menu = menu.child(list);
            }
            menu = menu.child(self.panel_row("pivot-popup-value-filters", format!("{}{}", text("_xlsx.value-filters"), arrow(popup.section == Some(PopupSection::Value))), !p.data_fields.is_empty(), Rc::new(|this, _, cx| this.pivot_popup_section(PopupSection::Value, cx)), cx));
            if popup.section == Some(PopupSection::Value) && !p.data_fields.is_empty() {
                let mut list = v_flex().pl_3();
                for &(label, kind) in VALUE_FILTERS {
                    list = list.child(self.panel_row(format!("pivot-popup-{kind}"), label, true, Rc::new(move |this, window, cx| this.pivot_popup_ask(false, kind, window, cx)), cx));
                }
                menu = menu.child(list);
            }
            let query = popup.search.read(cx).value().trim().to_lowercase();
            let searching = !query.is_empty();
            let shown: Vec<(usize, String)> = items.iter().filter(|(_, l)| !searching || l.to_lowercase().contains(&query)).cloned().collect();
            let all = shown.iter().all(|(i, _)| popup.ticked.get(*i).copied().unwrap_or(true));
            let mut list = v_flex().child(Checkbox::new("pivot-popup-all").label(text(if searching { "_xlsx.select-all-search-results" } else { "_xlsx.select-all" })).checked(all).on_click(cx.listener(|this, on: &bool, _, cx| this.pivot_popup_tick(None, *on, cx))));
            for (i, label) in shown {
                let on = popup.ticked.get(i).copied().unwrap_or(true);
                list = list.child(Checkbox::new(SharedString::from(format!("pivot-popup-item-{i}"))).label(label).checked(on).on_click(cx.listener(move |this, on: &bool, _, cx| this.pivot_popup_tick(Some(i), *on, cx))));
            }
            body = body
                .child(menu)
                .child(div().h(px(1.)).my_1().bg(theme.border))
                .child(crate::shell::chrome::text_field(&popup.search).full_width())
                .child(div().id("pivot-popup-items").h(px(200.)).overflow_y_scroll().border_1().border_color(theme.border).rounded_sm().child(list));
        }
        if let Some(d) = popup.dialog.as_ref() {
            body = body.child(self.render_pivot_filter_dialog(&p, popup.field, d, cx));
        } else {
            body = body.child(
                crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("pivot-popup-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|this, _, _, cx| this.pivot_popup_ok(cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("pivot-popup-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_pivot_popup(cx)))))),
            );
        }
        let panel = v_flex()
            .id("pivot-popup")
            .w(px(if popup.dialog.is_some() { 360. } else { 280. }))
            .max_h(px(560.))
            .overflow_y_scroll()
            .p_2()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_lg()
            .text_size(px(12.))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_pivot_popup(cx)))
            .child(body);
        Some(deferred(anchored().position(popup.position).snap_to_window_with_margin(px(8.)).child(panel)).with_priority(2).into_any_element())
    }
}
