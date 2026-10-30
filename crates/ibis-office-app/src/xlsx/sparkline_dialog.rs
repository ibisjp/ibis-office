// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スパークライン（折れ線・縦棒・勝敗）の作成、編集、グループ化・解除、消去。
//! Sparklines panel (`SheetCommand::AddSparklineGroup`, `EditSparklineGroup`,
//! `DeleteSparklineGroup`, `ClearSparklines`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, Rgb, SheetCommand, SparklineAxis, SparklineEmpty, SparklineGroup, SparklineKind, cell_name};

use super::sheet_ui::{Reference, Target, parse_reference};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 表示する点（頂点・低点・負の点・始点・終点・マーカー）の種類
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SparklinePointKind {
    High,
    Low,
    Negative,
    First,
    Last,
    Markers,
}

/// スパークラインの組み込みスタイル定義
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SparklineStylePreset {
    pub(crate) name: &'static str,
    pub(crate) series: Rgb,
    pub(crate) negative: Rgb,
    pub(crate) axis: Rgb,
    pub(crate) markers: Rgb,
    pub(crate) first: Rgb,
    pub(crate) last: Rgb,
    pub(crate) high: Rgb,
    pub(crate) low: Rgb,
}

pub(crate) const SPARKLINE_STYLES: [SparklineStylePreset; 6] = [
    SparklineStylePreset {
        name: "xlsx.sparkline-style-1",
        series: Rgb(0x376092),
        negative: Rgb(0xD00000),
        axis: Rgb(0x000000),
        markers: Rgb(0xD00000),
        first: Rgb(0xD00000),
        last: Rgb(0xD00000),
        high: Rgb(0x0070C0),
        low: Rgb(0xC00000),
    },
    SparklineStylePreset {
        name: "xlsx.sparkline-style-2",
        series: Rgb(0xC0504D),
        negative: Rgb(0x9BBB59),
        axis: Rgb(0x000000),
        markers: Rgb(0x4F81BD),
        first: Rgb(0x4F81BD),
        last: Rgb(0x4F81BD),
        high: Rgb(0x8064A2),
        low: Rgb(0x9BBB59),
    },
    SparklineStylePreset {
        name: "xlsx.sparkline-style-3",
        series: Rgb(0x9BBB59),
        negative: Rgb(0xC0504D),
        axis: Rgb(0x000000),
        markers: Rgb(0x4F81BD),
        first: Rgb(0x4F81BD),
        last: Rgb(0x4F81BD),
        high: Rgb(0x8064A2),
        low: Rgb(0xC0504D),
    },
    SparklineStylePreset {
        name: "xlsx.sparkline-style-4",
        series: Rgb(0x8064A2),
        negative: Rgb(0xC0504D),
        axis: Rgb(0x000000),
        markers: Rgb(0x4F81BD),
        first: Rgb(0x4F81BD),
        last: Rgb(0x4F81BD),
        high: Rgb(0x9BBB59),
        low: Rgb(0xC0504D),
    },
    SparklineStylePreset {
        name: "xlsx.sparkline-style-5",
        series: Rgb(0x4BACC6),
        negative: Rgb(0xC0504D),
        axis: Rgb(0x000000),
        markers: Rgb(0xF79646),
        first: Rgb(0xF79646),
        last: Rgb(0xF79646),
        high: Rgb(0x8064A2),
        low: Rgb(0xC0504D),
    },
    SparklineStylePreset {
        name: "xlsx.sparkline-style-6",
        series: Rgb(0x000000),
        negative: Rgb(0xC00000),
        axis: Rgb(0x000000),
        markers: Rgb(0x595959),
        first: Rgb(0x595959),
        last: Rgb(0x595959),
        high: Rgb(0x0070C0),
        low: Rgb(0xC00000),
    },
];

pub(crate) const SPARKLINE_COLORS: [(&str, Rgb); 8] = [
    ("_shell.blue", Rgb(0x376092)),
    ("_shell.red", Rgb(0xC00000)),
    ("_shell.green", Rgb(0x70AD47)),
    ("_shell.purple", Rgb(0x7030A0)),
    ("_xlsx.aqua", Rgb(0x4BACC6)),
    ("xlsx.sparkline-color-orange", Rgb(0xED7D31)),
    ("xlsx.sparkline-color-gray", Rgb(0x7F7F7F)),
    ("_shell.black", Rgb(0x000000)),
];

pub(crate) const SPARKLINE_WEIGHTS: [(&str, u32); 4] = [
    ("0.75 pt", 75),
    ("1.0 pt", 100),
    ("1.5 pt", 150),
    ("2.25 pt", 225),
];

pub(crate) const SPARKLINE_EMPTY_OPTIONS: [(&str, SparklineEmpty); 3] = [
    ("_xlsx.gaps", SparklineEmpty::Gap),
    ("_xlsx.zeros", SparklineEmpty::Zero),
    ("_xlsx.connect-data-points-with-line", SparklineEmpty::Span),
];

pub(crate) struct SparklinePanel {
    /// 既存グループの編集中の場合、そのインデックス
    pub(crate) group_index: Option<usize>,
    /// 折れ線、縦棒、勝敗
    pub(crate) kind: SparklineKind,
    /// データ範囲の入力欄（例: A1:C3 または Sheet1!A1:C3）
    pub(crate) data_range_input: Entity<InputState>,
    /// 配置先範囲の入力欄（例: D1:D3）
    pub(crate) location_range_input: Entity<InputState>,
    /// マーカー
    pub(crate) markers: bool,
    /// 頂点（高値）
    pub(crate) high: bool,
    /// 最小値（安値）
    pub(crate) low: bool,
    /// 先頭
    pub(crate) first: bool,
    /// 末尾
    pub(crate) last: bool,
    /// 負のポイント
    pub(crate) negative_points: bool,
    /// 軸の表示
    pub(crate) display_x_axis: bool,
    /// 線の太さ（ポイント * 100）
    pub(crate) line_weight: u32,
    /// 系列色（線色または棒色）
    pub(crate) series: Option<Rgb>,
    pub(crate) negative: Option<Rgb>,
    pub(crate) axis: Option<Rgb>,
    pub(crate) markers_color: Option<Rgb>,
    pub(crate) first_color: Option<Rgb>,
    pub(crate) last_color: Option<Rgb>,
    pub(crate) high_color: Option<Rgb>,
    pub(crate) low_color: Option<Rgb>,
    /// 非表示行列のデータ表示（displayHidden）
    pub(crate) display_hidden: bool,
    /// 右から左の描画（rightToLeft）
    pub(crate) right_to_left: bool,
    /// 日付軸（dateAxis）
    pub(crate) date_axis: bool,
    /// 日付範囲入力（<xm:f>）
    pub(crate) date_range_input: Entity<InputState>,
    /// 縦軸の最小値モード（0: Individual, 1: Group, 2: Custom）
    pub(crate) min_axis_kind: u8,
    /// 縦軸の最小値カスタム入力欄（manualMin）
    pub(crate) min_custom_input: Entity<InputState>,
    /// 縦軸の最大値モード（0: Individual, 1: Group, 2: Custom）
    pub(crate) max_axis_kind: u8,
    /// 縦軸の最大値カスタム入力欄（manualMax）
    pub(crate) max_custom_input: Entity<InputState>,
    /// 空白セルの表示方法（displayEmptyCellsAs）
    pub(crate) empty: SparklineEmpty,
    /// マーカー色編集対象のポイント種類
    pub(crate) selected_point_kind: SparklinePointKind,
    /// 選択中のスタイルプリセット番号
    pub(crate) style_preset: Option<usize>,
    /// エラー・案内メッセージ
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    pub(crate) fn open_sparkline_panel(&mut self, default_kind: Option<SparklineKind>, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let current_cell = self.current_cell();
        let current_sparkline = self.current_sheet().and_then(|(_, sheet)| {
            let (col, row) = current_cell?;
            sheet.sparklines.iter().enumerate().find_map(|(idx, g)| {
                if g.sparklines.iter().any(|s| s.row == row && s.column == col) {
                    Some((idx, g.clone()))
                } else {
                    None
                }
            })
        });

        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };

        let panel = if let Some((idx, group)) = current_sparkline {
            let data_str = if let Some(first) = group.sparklines.first() {
                let r = first.data;
                let range_str = if r.first_row == r.last_row && r.first_column == r.last_column {
                    cell_name(r.first_column, r.first_row)
                } else {
                    format!("{}:{}", cell_name(r.first_column, r.first_row), cell_name(r.last_column, r.last_row))
                };
                match &first.sheet {
                    Some(sh) => format!("{sh}!{range_str}"),
                    None => range_str,
                }
            } else {
                String::new()
            };
            let loc_str = if group.sparklines.len() == 1 {
                let s = &group.sparklines[0];
                cell_name(s.column, s.row)
            } else if let (Some(first), Some(last)) = (group.sparklines.first(), group.sparklines.last()) {
                format!("{}:{}", cell_name(first.column, first.row), cell_name(last.column, last.row))
            } else {
                String::new()
            };

            let style_preset = SPARKLINE_STYLES.iter().position(|p| group.series == Some(p.series));
            let (min_axis_kind, min_val_str) = match &group.min_axis {
                SparklineAxis::Individual => (0, String::new()),
                SparklineAxis::Group => (1, String::new()),
                SparklineAxis::Custom(v) => (2, v.clone()),
            };
            let (max_axis_kind, max_val_str) = match &group.max_axis {
                SparklineAxis::Individual => (0, String::new()),
                SparklineAxis::Group => (1, String::new()),
                SparklineAxis::Custom(v) => (2, v.clone()),
            };
            SparklinePanel {
                group_index: Some(idx),
                kind: default_kind.unwrap_or(group.kind),
                data_range_input: input(data_str, window, cx),
                location_range_input: input(loc_str, window, cx),
                markers: group.markers,
                high: group.high,
                low: group.low,
                first: group.first,
                last: group.last,
                negative_points: group.negative_points,
                display_x_axis: group.display_x_axis,
                line_weight: group.line_weight,
                series: group.series,
                negative: group.negative,
                axis: group.axis,
                markers_color: group.markers_color,
                first_color: group.first_color,
                last_color: group.last_color,
                high_color: group.high_color,
                low_color: group.low_color,
                display_hidden: group.display_hidden,
                right_to_left: group.right_to_left,
                date_axis: group.date_axis,
                date_range_input: input(group.date_range.clone().unwrap_or_default(), window, cx),
                min_axis_kind,
                min_custom_input: input(min_val_str, window, cx),
                max_axis_kind,
                max_custom_input: input(max_val_str, window, cx),
                empty: group.empty,
                selected_point_kind: SparklinePointKind::High,
                style_preset,
                message: String::new(),
                ui: Default::default(),
            }
        } else {
            let sel = self.selected_range();
            let (data_str, loc_str) = match sel {
                Some(r) if r.first_row == r.last_row && r.first_column == r.last_column => {
                    (String::new(), cell_name(r.first_column, r.first_row))
                }
                Some(r) => {
                    let range_str = format!("{}:{}", cell_name(r.first_column, r.first_row), cell_name(r.last_column, r.last_row));
                    (range_str, String::new())
                }
                None => (String::new(), String::new()),
            };

            let default_preset = &SPARKLINE_STYLES[0];
            SparklinePanel {
                group_index: None,
                kind: default_kind.unwrap_or(SparklineKind::Line),
                data_range_input: input(data_str, window, cx),
                location_range_input: input(loc_str, window, cx),
                markers: false,
                high: false,
                low: false,
                first: false,
                last: false,
                negative_points: false,
                display_x_axis: false,
                line_weight: 75,
                series: Some(default_preset.series),
                negative: Some(default_preset.negative),
                axis: Some(default_preset.axis),
                markers_color: Some(default_preset.markers),
                first_color: Some(default_preset.first),
                last_color: Some(default_preset.last),
                high_color: Some(default_preset.high),
                low_color: Some(default_preset.low),
                display_hidden: false,
                right_to_left: false,
                date_axis: false,
                date_range_input: input(String::new(), window, cx),
                min_axis_kind: 0,
                min_custom_input: input(String::new(), window, cx),
                max_axis_kind: 0,
                max_custom_input: input(String::new(), window, cx),
                empty: SparklineEmpty::Gap,
                selected_point_kind: SparklinePointKind::High,
                style_preset: Some(0),
                message: String::new(),
                ui: Default::default(),
            }
        };

        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.sort_edit = None;
        state.close_settings();
        state.panels.sparkline = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn sparkline_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sparkline.as_mut() {
            p.message = message.to_string();
        }
        cx.notify();
    }

    pub(crate) fn set_sparkline_kind(&mut self, kind: SparklineKind, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sparkline.as_mut() {
            p.kind = kind;
        }
        cx.notify();
    }

    pub(crate) fn set_sparkline_style_preset(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sparkline.as_mut() {
            if let Some(preset) = SPARKLINE_STYLES.get(idx) {
                p.style_preset = Some(idx);
                p.series = Some(preset.series);
                p.negative = Some(preset.negative);
                p.axis = Some(preset.axis);
                p.markers_color = Some(preset.markers);
                p.first_color = Some(preset.first);
                p.last_color = Some(preset.last);
                p.high_color = Some(preset.high);
                p.low_color = Some(preset.low);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_sparkline_series_color(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sparkline.as_mut() {
            p.series = color;
            p.style_preset = None;
        }
        cx.notify();
    }

    pub(crate) fn set_sparkline_point_color(&mut self, point: SparklinePointKind, color: Option<Rgb>, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sparkline.as_mut() {
            match point {
                SparklinePointKind::High => {
                    p.high_color = color;
                    if color.is_some() { p.high = true; }
                }
                SparklinePointKind::Low => {
                    p.low_color = color;
                    if color.is_some() { p.low = true; }
                }
                SparklinePointKind::Negative => {
                    p.negative = color;
                    if color.is_some() { p.negative_points = true; }
                }
                SparklinePointKind::First => {
                    p.first_color = color;
                    if color.is_some() { p.first = true; }
                }
                SparklinePointKind::Last => {
                    p.last_color = color;
                    if color.is_some() { p.last = true; }
                }
                SparklinePointKind::Markers => {
                    p.markers_color = color;
                    if color.is_some() { p.markers = true; }
                }
            }
            p.style_preset = None;
        }
        cx.notify();
    }

    pub(crate) fn set_sparkline_line_weight(&mut self, weight: u32, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sparkline.as_mut() {
            p.line_weight = weight;
        }
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn set_sparkline_display_hidden(&mut self, display_hidden: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sparkline.as_mut() {
            p.display_hidden = display_hidden;
        }
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn set_sparkline_date_axis(&mut self, date_axis: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sparkline.as_mut() {
            p.date_axis = date_axis;
        }
        cx.notify();
    }

    pub(crate) fn set_sparkline_empty(&mut self, empty: SparklineEmpty, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sparkline.as_mut() {
            p.empty = empty;
        }
        cx.notify();
    }

    pub(crate) fn sparkline_group_at_selection(&self) -> Option<(usize, SparklineGroup)> {
        let ranges = self.selected_ranges();
        let (_, sheet) = self.current_sheet()?;
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| {
                ranges.iter().any(|r| {
                    s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column
                })
            }) {
                return Some((idx, g.clone()));
            }
        }
        None
    }

    pub(crate) fn set_selected_sparklines_kind(&mut self, kind: SparklineKind, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| {
                ranges.iter().any(|r| {
                    s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column
                })
            }) {
                let mut edited = g.clone();
                edited.kind = kind;
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_selected_sparklines_style(&mut self, style_idx: usize, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(preset) = SPARKLINE_STYLES.get(style_idx) else { return };
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.series = Some(preset.series);
                edited.negative = Some(preset.negative);
                edited.axis = Some(preset.axis);
                edited.markers_color = Some(preset.markers);
                edited.first_color = Some(preset.first);
                edited.last_color = Some(preset.last);
                edited.high_color = Some(preset.high);
                edited.low_color = Some(preset.low);
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_selected_sparklines_series_color(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.series = color;
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_selected_sparklines_display_hidden(&mut self, display_hidden: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.display_hidden = display_hidden;
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_selected_sparklines_date_axis(&mut self, date_axis: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.date_axis = date_axis;
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_selected_sparklines_empty(&mut self, empty: SparklineEmpty, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.empty = empty;
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_selected_sparklines_line_weight(&mut self, weight: u32, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.line_weight = weight;
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn toggle_selected_sparklines_point(&mut self, point: SparklinePointKind, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                match point {
                    SparklinePointKind::High => edited.high = !edited.high,
                    SparklinePointKind::Low => edited.low = !edited.low,
                    SparklinePointKind::Negative => edited.negative_points = !edited.negative_points,
                    SparklinePointKind::First => edited.first = !edited.first,
                    SparklinePointKind::Last => edited.last = !edited.last,
                    SparklinePointKind::Markers => edited.markers = !edited.markers,
                }
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_selected_sparklines_point_color(&mut self, point: SparklinePointKind, color: Option<Rgb>, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                match point {
                    SparklinePointKind::High => {
                        edited.high_color = color;
                        if color.is_some() { edited.high = true; }
                    }
                    SparklinePointKind::Low => {
                        edited.low_color = color;
                        if color.is_some() { edited.low = true; }
                    }
                    SparklinePointKind::Negative => {
                        edited.negative = color;
                        if color.is_some() { edited.negative_points = true; }
                    }
                    SparklinePointKind::First => {
                        edited.first_color = color;
                        if color.is_some() { edited.first = true; }
                    }
                    SparklinePointKind::Last => {
                        edited.last_color = color;
                        if color.is_some() { edited.last = true; }
                    }
                    SparklinePointKind::Markers => {
                        edited.markers_color = color;
                        if color.is_some() { edited.markers = true; }
                    }
                }
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn toggle_selected_sparklines_display_x_axis(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.display_x_axis = !edited.display_x_axis;
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn toggle_selected_sparklines_right_to_left(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.right_to_left = !edited.right_to_left;
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_selected_sparklines_min_axis(&mut self, min_axis: SparklineAxis, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.min_axis = min_axis.clone();
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn set_selected_sparklines_max_axis(&mut self, max_axis: SparklineAxis, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                let mut edited = g.clone();
                edited.max_axis = max_axis.clone();
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            cx.notify();
        }
    }

    pub(crate) fn resolve_sparkline_data_range(&self, raw: &str) -> Option<(Option<String>, ibis_doc_model::CellRange)> {
        let clean = raw.trim().trim_start_matches('=').trim();
        if clean.is_empty() {
            return None;
        }
        let parsed = parse_reference(clean)?;
        if let Some((slide_id, range)) = self.resolve_reference(&parsed) {
            let sheet_name = if slide_id == self.current_slide {
                None
            } else {
                self.presentation().slides.iter().find(|s| s.id == slide_id).and_then(|s| s.name.clone())
            };
            return Some((sheet_name, range));
        }
        match parsed {
            Reference { sheet, target: Target::Cells(range) } => Some((sheet, range)),
            _ => None,
        }
    }

    pub(crate) fn resolve_sparkline_location_range(&self, raw: &str) -> Option<ibis_doc_model::CellRange> {
        let clean = raw.trim().trim_start_matches('=').trim();
        if clean.is_empty() {
            return None;
        }
        let parsed = parse_reference(clean)?;
        if let Some((slide_id, range)) = self.resolve_reference(&parsed) {
            if slide_id == self.current_slide {
                return Some(range);
            }
            return None;
        }
        match parsed {
            Reference { sheet: None, target: Target::Cells(range) } => Some(range),
            _ => None,
        }
    }

    pub(crate) fn set_sparkline_range_from_selection(&mut self, field: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_range() else { return };
        let val = if r.first_row == r.last_row && r.first_column == r.last_column {
            cell_name(r.first_column, r.first_row)
        } else {
            format!("{}:{}", cell_name(r.first_column, r.first_row), cell_name(r.last_column, r.last_row))
        };
        let input = self.wb().panels.sparkline.as_ref().and_then(|p| match field {
            "data" => Some(p.data_range_input.clone()),
            "location" => Some(p.location_range_input.clone()),
            "date" => Some(p.date_range_input.clone()),
            _ => None,
        });
        if let Some(input) = input {
            input.update(cx, |i, cx| i.set_value(val, window, cx));
            cx.notify();
        }
    }

    pub(crate) fn apply_sparkline(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.sparkline.as_ref()) else { return };
        let data_text = p.data_range_input.read(cx).value().trim().to_string();
        let loc_text = p.location_range_input.read(cx).value().trim().to_string();

        if data_text.is_empty() || loc_text.is_empty() {
            return self.sparkline_message(text("_xlsx.enter-a-data-range-and-a-location-range"), cx);
        }

        let Some((data_sheet, data_range)) = self.resolve_sparkline_data_range(&data_text) else {
            return self.sparkline_message(text("_xlsx.enter-a-valid-data-range"), cx);
        };
        let Some(loc_range) = self.resolve_sparkline_location_range(&loc_text) else {
            return self.sparkline_message(text("_xlsx.enter-a-valid-location-range"), cx);
        };

        let sparklines = ibis_doc_model::sheet_command::make_sparklines(data_sheet, data_range, loc_range);
        if sparklines.is_empty() {
            return self.sparkline_message(text("_xlsx.the-data-range-and-the-destination-range-don-t-h"), cx);
        }

        let date_text = p.date_range_input.read(cx).value().trim().to_string();
        let date_range = if p.date_axis && !date_text.is_empty() {
            let normalized = if let Some((sh, r)) = self.resolve_sparkline_data_range(&date_text) {
                let r_str = if r.first_row == r.last_row && r.first_column == r.last_column {
                    cell_name(r.first_column, r.first_row)
                } else {
                    format!("{}:{}", cell_name(r.first_column, r.first_row), cell_name(r.last_column, r.last_row))
                };
                match sh {
                    Some(s_name) => format!("{s_name}!{r_str}"),
                    None => r_str,
                }
            } else {
                date_text
            };
            Some(normalized)
        } else {
            None
        };

        let min_custom_text = p.min_custom_input.read(cx).value().trim().to_string();
        let min_axis = match p.min_axis_kind {
            1 => SparklineAxis::Group,
            2 => SparklineAxis::Custom(if min_custom_text.is_empty() { "0".to_string() } else { min_custom_text }),
            _ => SparklineAxis::Individual,
        };
        let max_custom_text = p.max_custom_input.read(cx).value().trim().to_string();
        let max_axis = match p.max_axis_kind {
            1 => SparklineAxis::Group,
            2 => SparklineAxis::Custom(if max_custom_text.is_empty() { "0".to_string() } else { max_custom_text }),
            _ => SparklineAxis::Individual,
        };

        let slide = self.current_slide;
        if let Some(idx) = p.group_index {
            let current_g = self.current_sheet().and_then(|(_, s)| s.sparklines.get(idx)).cloned();
            let mut group = current_g.unwrap_or_default();
            group.kind = p.kind;
            group.line_weight = p.line_weight;
            group.markers = p.markers;
            group.high = p.high;
            group.low = p.low;
            group.first = p.first;
            group.last = p.last;
            group.negative_points = p.negative_points;
            group.display_x_axis = p.display_x_axis;
            group.series = p.series;
            group.negative = p.negative;
            group.axis = p.axis;
            group.markers_color = p.markers_color;
            group.first_color = p.first_color;
            group.last_color = p.last_color;
            group.high_color = p.high_color;
            group.low_color = p.low_color;
            group.display_hidden = p.display_hidden;
            group.right_to_left = p.right_to_left;
            group.date_axis = p.date_axis;
            group.date_range = date_range;
            group.min_axis = min_axis;
            group.max_axis = max_axis;
            group.empty = p.empty;
            group.sparklines = sparklines;

            self.run_sheet(vec![Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group })], cx);
        } else {
            let group = SparklineGroup {
                kind: p.kind,
                line_weight: p.line_weight,
                markers: p.markers,
                high: p.high,
                low: p.low,
                first: p.first,
                last: p.last,
                negative_points: p.negative_points,
                display_x_axis: p.display_x_axis,
                series: p.series,
                negative: p.negative,
                axis: p.axis,
                markers_color: p.markers_color,
                first_color: p.first_color,
                last_color: p.last_color,
                high_color: p.high_color,
                low_color: p.low_color,
                display_hidden: p.display_hidden,
                right_to_left: p.right_to_left,
                date_axis: p.date_axis,
                date_range,
                min_axis,
                max_axis,
                empty: p.empty,
                sparklines,
            };
            self.run_sheet(vec![Command::Sheet(SheetCommand::AddSparklineGroup { slide, group })], cx);
        }

        self.wb_mut().panels.sparkline = None;
        cx.notify();
    }

    pub(crate) fn clear_sparklines_action(&mut self, whole_group: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        if ranges.is_empty() {
            return;
        }
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::ClearSparklines { slide, ranges, whole_group })], cx);
        if let Some(s) = self.sheet_state_mut() {
            s.panels.sparkline = None;
        }
        cx.notify();
    }

    pub(crate) fn ungroup_selected_sparklines(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut commands = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            let mut keep = Vec::new();
            let mut split = Vec::new();
            for s in &g.sparklines {
                if ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column) {
                    split.push(s.clone());
                } else {
                    keep.push(s.clone());
                }
            }
            if !keep.is_empty() && !split.is_empty() {
                let mut edited = g.clone();
                edited.sparklines = keep;
                commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: idx, group: edited }));
                let mut new_g = g.clone();
                new_g.sparklines = split;
                commands.push(Command::Sheet(SheetCommand::AddSparklineGroup { slide, group: new_g }));
            }
        }
        if !commands.is_empty() {
            self.run_sheet(commands, cx);
            if let Some(s) = self.sheet_state_mut() {
                s.panels.sparkline = None;
            }
            cx.notify();
        }
    }

    pub(crate) fn group_selected_sparklines(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut matching_indices: Vec<usize> = Vec::new();
        for (idx, g) in sheet.sparklines.iter().enumerate() {
            if g.sparklines.iter().any(|s| ranges.iter().any(|r| s.row >= r.first_row && s.row <= r.last_row && s.column >= r.first_column && s.column <= r.last_column)) {
                matching_indices.push(idx);
            }
        }
        if matching_indices.len() > 1 {
            let base_idx = matching_indices[0];
            let mut merged = sheet.sparklines[base_idx].clone();
            for &idx in &matching_indices[1..] {
                merged.sparklines.extend(sheet.sparklines[idx].sparklines.clone());
            }
            let mut commands = Vec::new();
            for &idx in matching_indices[1..].iter().rev() {
                commands.push(Command::Sheet(SheetCommand::DeleteSparklineGroup { slide, index: idx }));
            }
            commands.push(Command::Sheet(SheetCommand::EditSparklineGroup { slide, index: base_idx, group: merged }));
            self.run_sheet(commands, cx);
            if let Some(s) = self.sheet_state_mut() {
                s.panels.sparkline = None;
            }
            cx.notify();
        }
    }

    pub(crate) fn sparkline_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.sparkline.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(
                    canvas(|_, _, _| {}, move |bounds, _, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
        };

        let current_kind = p.kind;
        let is_edit = p.group_index.is_some();
        let markers = p.markers;
        let high = p.high;
        let low = p.low;
        let first = p.first;
        let last = p.last;
        let negative_points = p.negative_points;
        let display_x_axis = p.display_x_axis;
        let current_style = p.style_preset;
        let current_series = p.series;
        let current_weight = p.line_weight;
        let current_empty = p.empty;
        let selected_point_kind = p.selected_point_kind;
        let current_point_color = match selected_point_kind {
            SparklinePointKind::High => p.high_color,
            SparklinePointKind::Low => p.low_color,
            SparklinePointKind::Negative => p.negative,
            SparklinePointKind::First => p.first_color,
            SparklinePointKind::Last => p.last_color,
            SparklinePointKind::Markers => p.markers_color,
        };
        let display_hidden = p.display_hidden;
        let right_to_left = p.right_to_left;
        let date_axis = p.date_axis;
        let min_axis_kind = p.min_axis_kind;
        let max_axis_kind = p.max_axis_kind;
        let min_custom_input = p.min_custom_input.clone();
        let max_custom_input = p.max_custom_input.clone();

        let kind_button = |kind: SparklineKind, label: SharedString, id: &'static str, cx: &mut Context<Self>| {
            let ui=p.ui.clone();
            crate::shell::chrome::segment_control::SegmentItem::label(id,label,current_kind==kind)
                .on_bounds(move |b|{ui.borrow_mut().insert(id.to_string(),b);})
                .on_click(cx.listener(move |e,_,_,cx|e.set_sparkline_kind(kind,cx)))
        };

        let style_entries=SPARKLINE_STYLES.iter().enumerate().map(|(i,p)|crate::shell::chrome::MenuItem::new("xlsx.sparkline.style-choice",text(p.name)).checked(current_style==Some(i)).on(move|e,_,cx|e.set_sparkline_style_preset(i,cx)).into()).collect();
        let style_dropdown=crate::shell::chrome::Dropdown::from_entries("sparkline-style-dropdown",current_style.map(|i|text(SPARKLINE_STYLES[i].name)).unwrap_or_else(||text("_shell.custom")),style_entries,true,0,&cx.weak_entity()).into_any_element();
        let color_choices=SPARKLINE_COLORS.iter().map(|(name,color)|{let color=*color;crate::shell::chrome::dropdown::ColorPaletteItem::single(text(name),color.0,std::rc::Rc::new(move |e,_,cx|e.set_sparkline_series_color(Some(color),cx)))}).collect();
        let color_index=SPARKLINE_COLORS.iter().position(|(_,c)|Some(*c)==current_series).unwrap_or(0);
        let color_dropdown=crate::shell::chrome::ColorPaletteDropdown::new("sparkline-line-color",color_choices,color_index,&cx.weak_entity());
        let point_target_buttons: Vec<AnyElement> = [
            ("xlsx.sparkline.point-target.high", "_xlsx.high-point", SparklinePointKind::High),
            ("xlsx.sparkline.point-target.low", "xlsx.sparkline.low-point", SparklinePointKind::Low),
            ("xlsx.sparkline.point-target.negative", "_xlsx.negative-points", SparklinePointKind::Negative),
            ("xlsx.sparkline.point-target.first", "_xlsx.first-point", SparklinePointKind::First),
            ("xlsx.sparkline.point-target.last", "_xlsx.last-point", SparklinePointKind::Last),
            ("xlsx.sparkline.point-target.markers", "_xlsx.marker", SparklinePointKind::Markers),
        ]
        .into_iter()
        .map(|(id, label, pt)| {
            let is_sel = selected_point_kind == pt;
            mark(
                id,
                crate::shell::chrome::radio_button(id,is_sel,false,text(label),cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() {
                            p.selected_point_kind = pt;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        })
        .collect();

        let point_color_choices=SPARKLINE_COLORS.iter().map(|(name,color)|{let color=*color;crate::shell::chrome::dropdown::ColorPaletteItem::single(text(name),color.0,std::rc::Rc::new(move |e,_,cx|e.set_sparkline_point_color(selected_point_kind,Some(color),cx)))}).collect();
        let point_color_index=SPARKLINE_COLORS.iter().position(|(_,c)|Some(*c)==current_point_color).unwrap_or(0);
        let point_color_dropdown=crate::shell::chrome::ColorPaletteDropdown::new("sparkline-point-color",point_color_choices,point_color_index,&cx.weak_entity());


        let weight_buttons=SPARKLINE_WEIGHTS.iter().enumerate().map(|(i,(label,w))| {
            let id=format!("xlsx.sparkline.weight.{}",i+1);let ui=p.ui.clone();let weight=*w;
            crate::shell::chrome::segment_control::SegmentItem::label(id.clone(),*label,current_weight==weight)
                .on_bounds(move |b|{ui.borrow_mut().insert(id.clone(),b);})
                .on_click(cx.listener(move |e,_,_,cx|e.set_sparkline_line_weight(weight,cx)))
        }).collect::<Vec<_>>();
        let empty_buttons: Vec<AnyElement> = SPARKLINE_EMPTY_OPTIONS
            .iter()
            .enumerate()
            .map(|(_, (label, empty_val))| {
                let is_sel = current_empty == *empty_val;
                let id = match empty_val {
                    SparklineEmpty::Gap => "xlsx.sparkline.empty.gap",
                    SparklineEmpty::Zero => "xlsx.sparkline.empty.zero",
                    SparklineEmpty::Span => "xlsx.sparkline.empty.span",
                };
                let empty_mode = *empty_val;
                mark(
                    id,
                    crate::shell::chrome::radio_button(id,is_sel,false,text(label),cx.listener(move |this, _, _, cx| {
                            this.set_sparkline_empty(empty_mode, cx);
                        }))
                        .into_any_element(),
                )
            })
            .collect();

        let footer = crate::shell::chrome::action_bar()
                    .action(mark_action("xlsx.sparkline.apply", crate::shell::chrome::ActionDefinition::new("xlsx.sparkline.apply", "", crate::shell::chrome::ActionButtonKind::Primary).label(if is_edit { text("_shell.apply") } else { text("_xlsx.create") }).on_click(cx.listener(|this, _, _, cx| {
                        this.apply_sparkline(cx);
                    }))))
                    .action(mark_action("xlsx.sparkline.cancel", crate::shell::chrome::ActionDefinition::new("xlsx.sparkline.cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| {
                        this.wb_mut().panels.sparkline = None;
                        cx.notify();
                    }))));
        let body = v_flex()
            .gap_2()
            .px(px(16.))
            .py(px(8.))
            .p_0().child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(gpui::FontWeight::BOLD).child(if is_edit { text("_xlsx.edit-sparkline") } else { text("_xlsx.create-sparklines") }))
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(text("fix.series.type")))
                    .child(
                        crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                            .child(kind_button(SparklineKind::Line, crate::shell::i18n::text("pptx.insert-chart.line"), "xlsx.sparkline.kind.line", cx))
                            .child(kind_button(SparklineKind::Column, crate::shell::i18n::text("_xlsx.column-2"), "xlsx.sparkline.kind.column", cx))
                            .child(kind_button(SparklineKind::WinLoss, crate::shell::i18n::text("_xlsx.win-loss"), "xlsx.sparkline.kind.winloss", cx))
                    )
            )
            .child(crate::shell::chrome::dropdown_row(text("_shell.styles"),style_dropdown))
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(text("_xlsx.line-color")))
                    .child(color_dropdown)
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(text("xlsx.sparkline-marker-colors")))
                    .child(h_flex().gap_1().flex_wrap().children(point_target_buttons))
                    .child(point_color_dropdown)
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(text("pptx.chrome.line-weight")))
                    .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic).children(weight_buttons))
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().child(text("_xlsx.data-range"))),
                    )
                    .child(mark("xlsx.sparkline.data-range", super::range_field::local_range_field(&p.data_range_input,"sparkline_dialog-p-data_range_input",false,cx).into_any_element()))
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().child(text("_xlsx.location-range"))),
                    )
                    .child(mark("xlsx.sparkline.location-range", super::range_field::local_range_field(&p.location_range_input,"sparkline_dialog-p-location_range_input",false,cx).into_any_element()))
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(gpui::FontWeight::BOLD).child(text("_shell.view")))
                    .child(mark("xlsx.sparkline.high", Checkbox::new("sparkline-high").label(text("_xlsx.high-point")).checked(high).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.high = *on; }
                        cx.notify();
                    })).into_any_element()))
                    .child(mark("xlsx.sparkline.low", Checkbox::new("sparkline-low").label(text("_shell.at-least")).checked(low).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.low = *on; }
                        cx.notify();
                    })).into_any_element()))
                    .child(mark("xlsx.sparkline.negative", Checkbox::new("sparkline-negative").label(text("_xlsx.negative-points")).checked(negative_points).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.negative_points = *on; }
                        cx.notify();
                    })).into_any_element()))
                    .child(mark("xlsx.sparkline.first", Checkbox::new("sparkline-first").label(text("_xlsx.first-point")).checked(first).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.first = *on; }
                        cx.notify();
                    })).into_any_element()))
                    .child(mark("xlsx.sparkline.last", Checkbox::new("sparkline-last").label(text("_xlsx.last-point")).checked(last).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.last = *on; }
                        cx.notify();
                    })).into_any_element()))
                    .child(mark("xlsx.sparkline.markers", Checkbox::new("sparkline-markers").label(text("_xlsx.marker")).checked(markers).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.markers = *on; }
                        cx.notify();
                    })).into_any_element()))
                    .child(mark("xlsx.sparkline.axis", Checkbox::new("sparkline-axis").label(text("_xlsx.show-axis")).checked(display_x_axis).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.display_x_axis = *on; }
                        cx.notify();
                    })).into_any_element()))
                    .child(mark("xlsx.sparkline.right-to-left", Checkbox::new("sparkline-right-to-left").label(text("xlsx.sparkline-right-to-left")).checked(right_to_left).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.right_to_left = *on; }
                        cx.notify();
                    })).into_any_element()))
                    .child(mark("xlsx.sparkline.display-hidden", Checkbox::new("sparkline-display-hidden").label(text("_xlsx.show-data-in-hidden-rows-and-columns")).checked(display_hidden).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.display_hidden = *on; }
                        cx.notify();
                    })).into_any_element()))
                    .child(mark("xlsx.sparkline.date-axis", Checkbox::new("sparkline-date-axis").label(text("_xlsx.date-axis")).checked(date_axis).on_click(cx.listener(|this, on: &bool, _, cx| {
                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.date_axis = *on; }
                        cx.notify();
                    })).into_any_element()))
            )
            .when(date_axis, |b| {
                b.child(
                    v_flex()
                        .gap_1()
                        .child(
                            h_flex()
                                .items_center()
                                .justify_between()
                                .child(div().text_sm().child(text("_xlsx.date-range"))),
                        )
                        .child(mark("xlsx.sparkline.date-range", super::range_field::local_range_field(&p.date_range_input,"sparkline_dialog-p-date_range_input",false,cx).into_any_element()))
                )
            })
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(text("_xlsx.vertical-axis-min")))
                    .child(
                        crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                            .child(mark(
                                "xlsx.sparkline.min.individual",
                                crate::shell::chrome::radio_button("xlsx.sparkline.min.individual", min_axis_kind == 0, false, text("_xlsx.automatic-for-each"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.min_axis_kind = 0; }
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                            ))
                            .child(mark(
                                "xlsx.sparkline.min.group",
                                crate::shell::chrome::radio_button("xlsx.sparkline.min.group", min_axis_kind == 1, false, text("_xlsx.same-for-all"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.min_axis_kind = 1; }
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                            ))
                            .child(mark(
                                "xlsx.sparkline.min.custom",
                                crate::shell::chrome::radio_button("xlsx.sparkline.min.custom", min_axis_kind == 2, false, text("_xlsx.custom-value"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.min_axis_kind = 2; }
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                            )),
                    )
                    .when(min_axis_kind == 2, |b| {
                        b.child(mark("xlsx.sparkline.min-value", crate::shell::chrome::text_field(&min_custom_input).full_width().into_any_element()))
                    }),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(text("_xlsx.vertical-axis-max")))
                    .child(
                        crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                            .child(mark(
                                "xlsx.sparkline.max.individual",
                                crate::shell::chrome::radio_button("xlsx.sparkline.max.individual", max_axis_kind == 0, false, text("_xlsx.automatic-for-each"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.max_axis_kind = 0; }
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                            ))
                            .child(mark(
                                "xlsx.sparkline.max.group",
                                crate::shell::chrome::radio_button("xlsx.sparkline.max.group", max_axis_kind == 1, false, text("_xlsx.same-for-all"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.max_axis_kind = 1; }
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                            ))
                            .child(mark(
                                "xlsx.sparkline.max.custom",
                                crate::shell::chrome::radio_button("xlsx.sparkline.max.custom", max_axis_kind == 2, false, text("_xlsx.custom-value"), cx.listener(|this, _, _, cx| {
                                        if let Some(p) = this.wb_mut().panels.sparkline.as_mut() { p.max_axis_kind = 2; }
                                        cx.notify();
                                    }))
                                    .into_any_element(),
                            )),
                    )
                    .when(max_axis_kind == 2, |b| {
                        b.child(mark("xlsx.sparkline.max-value", crate::shell::chrome::text_field(&max_custom_input).full_width().into_any_element()))
                    }),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(text("_xlsx.show-empty-cells-as")))
                    .child(h_flex().gap_1().flex_wrap().children(empty_buttons))
            )
            .when(!p.message.is_empty(), |b| {
                let msg = p.message.clone();
                b.child(div().text_sm().text_color(gpui::red()).child(msg))
            })

            .when(is_edit, |b| {
                b.child(
                    v_flex()
                        .gap_1()
                        .pt_2()
                        .child(mark("xlsx.sparkline.group", crate::shell::chrome::action_button("xlsx.sparkline.group").small().label(text("xlsx.group")).on_click(cx.listener(|this, _, _, cx| {
                            this.group_selected_sparklines(cx);
                        })).into_any_element()))
                        .child(mark("xlsx.sparkline.ungroup", crate::shell::chrome::destructive_button("xlsx.sparkline.ungroup",cx).small().label(text("_shell.ungroup")).on_click(cx.listener(|this, _, _, cx| {
                            this.ungroup_selected_sparklines(cx);
                        })).into_any_element()))
                        .child(mark("xlsx.sparkline.clear-selected", crate::shell::chrome::destructive_button("xlsx.sparkline.clear-selected",cx).small().label(text("_xlsx.clear-selected-sparklines")).on_click(cx.listener(|this, _, _, cx| {
                            this.clear_sparklines_action(false, cx);
                        })).into_any_element()))
                        .child(mark("xlsx.sparkline.clear-group", crate::shell::chrome::destructive_button("xlsx.sparkline.clear-group",cx).small().label(text("_xlsx.clear-group")).on_click(cx.listener(|this, _, _, cx| {
                            this.clear_sparklines_action(true, cx);
                        })).into_any_element()))
                )
            });

        Some(SidebarPanel::with_footer(text("_xlsx.sparklines"),body.into_any_element(),footer))
    }
}
