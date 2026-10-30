// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > 予測シート…: a new sheet with a timeline and the values on it, the forecast to an end
//! date with the bounds of its confidence interval, a table of statistics, and a line or column
//! chart of them (`ibis_doc_model::forecast_sheet`). Made in one step to undo.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::forecast_sheet::{self, Settings};
use ibis_doc_model::{CellEdit, CellRange, CellValue, Command, EmuRect, SheetCommand, StyleChange};
use ibis_pptx::NewChartKind;

use super::analysis_panel::{Resolved, range_text};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

pub(crate) struct ForecastPanel {
    subscriptions:Vec<Subscription>,
    /// The timeline's cells (a column; a heading above is taken as its name).
    pub(crate) timeline: Entity<InputState>,
    /// The values' cells.
    pub(crate) values: Entity<InputState>,
    /// The timeline value (a date) the forecast starts at (empty = default to last historical data point).
    pub(crate) start: Entity<InputState>,
    /// The timeline value (a date) the forecast ends at.
    pub(crate) end: Entity<InputState>,
    /// The confidence level in percent.
    pub(crate) confidence: Entity<InputState>,
    /// Seasonality manual mode (false = auto, true = manual).
    pub(crate) seasonality_manual: bool,
    /// Manual seasonality integer value input.
    pub(crate) seasonality: Entity<InputState>,
    /// Data completion: false = 1 (interpolation), true = 0 (zeros).
    pub(crate) data_completion_zero: bool,
    /// Aggregation: 1..=7 (1: average, 2: count, 3: counta, 4: max, 5: median, 6: min, 7: sum).
    pub(crate) aggregation: u32,
    /// A column chart, else a line chart.
    pub(crate) columns: bool,
    /// The table of statistics beside the data.
    pub(crate) statistics: bool,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

/// The timeline or values of one column of `r`: the heading (a text first cell) and the numbers.
fn column_of(r: &Resolved) -> (Option<String>, Vec<Option<f64>>) {
    let range = r.range;
    let (rows, column) = (range.first_row..=range.last_row, range.first_column);
    let heading = match r.sheet.cell(range.first_row, column).map(|c| &c.value) {
        Some(CellValue::Text(t)) => Some(t.clone()),
        _ => None,
    };
    let skip = usize::from(heading.is_some());
    let values = rows
        .skip(skip)
        .map(|row| match r.sheet.cell(row, column).map(|c| &c.value) {
            Some(CellValue::Number(n)) => Some(*n),
            _ => None,
        })
        .collect();
    (heading, values)
}

impl SlideEditor {
    pub(crate) fn open_forecast_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let range = self.selected_range().or_else(|| self.current_region());
        let two = range.filter(|r| r.last_column > r.first_column);
        let (timeline_text, values_text) = match two {
            Some(r) => (
                range_text(CellRange { last_column: r.first_column, ..r }),
                range_text(CellRange { first_column: r.first_column + 1, last_column: r.first_column + 1, ..r }),
            ),
            None => (String::new(), String::new()),
        };
        let make = |value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            let v = value.to_string();
            state.update(cx, |i, cx| i.set_value(v, window, cx));
            state
        };
        let mut panel = ForecastPanel {subscriptions:vec![],
            timeline: make(&timeline_text, window, cx),
            values: make(&values_text, window, cx),
            start: make("", window, cx),
            end: make("", window, cx),
            confidence: make("95", window, cx),
            seasonality_manual: false,
            seasonality: make("4", window, cx),
            data_completion_zero: false,
            aggregation: 1,
            columns: false,
            statistics: true,
            message: String::new(),
            ui: Default::default(),
        };
        panel.subscriptions.push(cx.subscribe_in(&panel.timeline,window,|e,input,event:&gpui_kit::component::input::InputEvent,w,cx|{if matches!(event,gpui_kit::component::input::InputEvent::Change){let value=input.read(cx).value().to_string();e.refresh_forecast_bounds_from_timeline(&value,w,cx);}}));
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
        state.panels.forecast = Some(panel);
        self.sidebar_open = true;
        // The start and end of the forecast: when the timeline is regular.
        self.refresh_forecast_bounds_from_timeline(&timeline_text, window, cx);
        cx.notify();
    }

    pub(crate) fn refresh_forecast_bounds_from_timeline(&mut self, timeline_text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !timeline_text.is_empty()
            && let Ok(r) = self.resolve_range(timeline_text)
        {
            let (_, t) = column_of(&r);
            let t: Vec<f64> = t.into_iter().flatten().collect();
            let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
            if let Ok(step) = forecast_sheet::step_of(&t, date1904) {
                let last = t[t.len() - 1];
                let end = match step {
                    forecast_sheet::Step::Amount(a) => last + 5.0 * a,
                    forecast_sheet::Step::Months(m) => forecast_sheet::after(last, forecast_sheet::Step::Months(m), 5, date1904),
                };
                let shown_end = if t.iter().all(|v| v.fract() == 0.0) && matches!(step, forecast_sheet::Step::Months(_)) {
                    let (y, m, d) = ibis_calc::eval::convert::date_from_serial(end as i64, date1904);
                    format!("{y}/{m}/{d}")
                } else {
                    ibis_calc::eval::number_to_text(end)
                };
                let shown_start = if t.iter().all(|v| v.fract() == 0.0) && matches!(step, forecast_sheet::Step::Months(_)) {
                    let (y, m, d) = ibis_calc::eval::convert::date_from_serial(last as i64, date1904);
                    format!("{y}/{m}/{d}")
                } else {
                    ibis_calc::eval::number_to_text(last)
                };
                if let Some(p) = self.wb_mut().panels.forecast.as_ref() {
                    let start_input = p.start.clone();
                    start_input.update(cx, |i, cx| i.set_value(shown_start, window, cx));
                    let end_input = p.end.clone();
                    end_input.update(cx, |i, cx| i.set_value(shown_end, window, cx));
                }
            }
        }
    }

    pub(crate) fn set_forecast_range_from_selection(&mut self, for_values: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_range() else { return };
        let val = range_text(r);
        let input = self.wb().panels.forecast.as_ref().map(|p| if for_values { p.values.clone() } else { p.timeline.clone() });
        if let Some(input) = input {
            let v = val.clone();
            input.update(cx, |i, cx| i.set_value(v, window, cx));
            if !for_values {
                self.refresh_forecast_bounds_from_timeline(&val, window, cx);
            }
            cx.notify();
        }
    }

    pub(crate) fn forecast_set_start(&mut self, val: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.forecast.as_ref() {
            let start = p.start.clone();
            let v = val.to_string();
            start.update(cx, |i, cx| i.set_value(v, window, cx));
        }
    }

    pub(crate) fn forecast_set_seasonality(&mut self, manual: bool, val: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.forecast.as_mut() {
            p.seasonality_manual = manual;
            let seasonality = p.seasonality.clone();
            let v = val.to_string();
            seasonality.update(cx, |i, cx| i.set_value(v, window, cx));
        }
    }

    pub(crate) fn forecast_set_data_completion_zero(&mut self, zero: bool, _cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.forecast.as_mut() {
            p.data_completion_zero = zero;
        }
    }

    pub(crate) fn forecast_set_aggregation(&mut self, agg: u32, _cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.forecast.as_mut() {
            p.aggregation = agg;
        }
    }

    fn forecast_message(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.forecast.as_mut() {
            p.message = message;
        }
        cx.notify();
    }

    pub(crate) fn apply_forecast_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        match self.make_forecast_sheet(window, cx) {
            Ok(done) | Err(done) => self.forecast_message(done, cx),
        }
    }

    pub(crate) fn make_forecast_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<String, String> {
        let p = self.sheet_state().and_then(|s| s.panels.forecast.as_ref()).ok_or_else(|| text("xlsx.analysis.message.no-data").to_string())?;
        let read = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let (timeline_text, values_text, start_text, end_text, confidence_text) = (
            read(&p.timeline),
            read(&p.values),
            read(&p.start),
            read(&p.end),
            read(&p.confidence),
        );
        let (columns, statistics) = (p.columns, p.statistics);
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let timeline_ref = self.resolve_range(&timeline_text)?;
        let values_ref = self.resolve_range(&values_text)?;
        let (timeline_name, timeline) = column_of(&timeline_ref);
        let (values_name, values) = column_of(&values_ref);
        if timeline.len() != values.len() {
            return Err(text("xlsx.forecast.message.sizes").to_string());
        }
        let (Some(timeline), Some(values)): (Option<Vec<f64>>, Option<Vec<f64>>) = (timeline.into_iter().collect(), values.into_iter().collect()) else {
            return Err(text("xlsx.forecast.message.not-numbers").to_string());
        };
        let end = if end_text.is_empty() {
            let step = forecast_sheet::step_of(&timeline, date1904).map_err(|e| text(e.message()).to_string())?;
            let last = timeline[timeline.len() - 1];
            match step {
                forecast_sheet::Step::Amount(a) => last + 5.0 * a,
                forecast_sheet::Step::Months(m) => forecast_sheet::after(last, forecast_sheet::Step::Months(m), 5, date1904),
            }
        } else {
            match ibis_calc::input::parse_input(&end_text, date1904) {
                ibis_calc::input::Typed::Value(CellValue::Number(n), _) => n,
                _ => return Err(text("xlsx.forecast.message.end").to_string()),
            }
        };
        let start = if start_text.is_empty() {
            None
        } else {
            match ibis_calc::input::parse_input(&start_text, date1904) {
                ibis_calc::input::Typed::Value(CellValue::Number(n), _) => Some(n),
                _ => return Err(text("xlsx.forecast.message.start").to_string()),
            }
        };
        let seasonality = if p.seasonality_manual {
            let s_text = read(&p.seasonality);
            let s_val = s_text.parse::<i64>().ok().filter(|v| *v >= 0).ok_or_else(|| text("xlsx.forecast.error.seasonality").to_string())?;
            Some(s_val)
        } else {
            None
        };
        let data_completion = if p.data_completion_zero { 0 } else { 1 };
        let aggregation = p.aggregation as i64;
        let confidence = confidence_text.parse::<f64>().ok().filter(|c| *c > 0.0 && *c < 100.0).map(|c| c / 100.0).ok_or_else(|| text("xlsx.forecast.error.confidence").to_string())?;
        let label = |id: &str| text(id).to_string();
        let names = (timeline_name.unwrap_or_else(|| label("xlsx.forecast.timeline")), values_name.unwrap_or_else(|| label("xlsx.forecast.values")));
        let built = forecast_sheet::build(
            &timeline,
            &values,
            (&names.0, &names.1),
            &label,
            &Settings {
                end,
                start,
                confidence,
                seasonality,
                data_completion,
                aggregation,
                statistics,
                date1904,
            },
        ).map_err(|e| text(e.message()).to_string())?;

        // The timeline's number format (a date stays a date) from the source's first cell.
        let source_format = timeline_ref.sheet.cell(timeline_ref.range.first_row + u32::from(timeline_ref.sheet.cell(timeline_ref.range.first_row, timeline_ref.range.first_column).is_some_and(|c| matches!(c.value, CellValue::Text(_)))), timeline_ref.range.first_column).and_then(|c| timeline_ref.sheet.styles.get(c.style as usize)).map(|s| s.number_format.clone());

        // The sheet: after the current one, one step to undo with its cells and chart.
        let Some(wb) = self.presentation().workbook.clone() else { return Err(text("xlsx.analysis.message.no-data").to_string()) };
        let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)).map_or(wb.sheets.len(), |i| i + 1);
        let base = text("xlsx.forecast.sheet-name").to_string();
        let mut n = 1;
        let name = loop {
            let name = if n == 1 { base.clone() } else { format!("{base} {n}") };
            if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                break name;
            }
            n += 1;
        };
        let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::AddSheet { index, name: name.clone(), slide, shape })]) {
            return Err(format("edit.refused", &[("error", &err)]));
        }
        let edits: Vec<CellEdit> = built.cells.iter().map(|(row, column, input)| CellEdit { row: *row, column: *column, input: input.clone() }).collect();
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
        if let Some(code) = source_format.filter(|c| c != "General") {
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![CellRange { first_row: 1, first_column: 0, last_row: built.last_row, last_column: 0 }], change: StyleChange::NumberFormat(code) }));
        }
        for (range, code) in &built.formats {
            if code != "General" {
                commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![*range], change: StyleChange::NumberFormat(code.clone()) }));
            }
        }
        if let Err(err) = self.document.amend_last(commands) {
            return Err(format("edit.refused", &[("error", &err)]));
        }
        self.select_slide(slide, window, cx);

        // Autofit columns for the forecast sheet
        let mut size_commands = Vec::new();
        if let Some((_, sheet)) = self.current_sheet() {
            let ts = window.text_system().clone();
            let cols_to_fit: Vec<u32> = if statistics {
                vec![0, 1, 2, 3, 4, 6, 7]
            } else {
                vec![0, 1, 2, 3, 4]
            };
            let mut measures: Vec<Option<(super::sheet_view::GlyphWidths, f32, f32, f32)>> = Vec::new();
            measures.resize_with(sheet.styles.len(), || None);
            for &col in &cols_to_fit {
                let mut widest = 0.0f32;
                for cell in sheet.cells.iter() {
                    if cell.column != col || cell.text.is_empty() {
                        continue;
                    }
                    if sheet.merge_at(cell.row, cell.column).is_some() {
                        continue;
                    }
                    let style = &sheet.styles[cell.style as usize];
                    let slot = &mut measures[cell.style as usize];
                    let (glyphs, excel_digit, drawn_digit, italic) = slot.get_or_insert_with(|| self.excel_measure(style, &ts));
                    let width = glyphs.width(&cell.text) * *excel_digit / *drawn_digit + *italic + style.indent as f32 * 3.0 * *excel_digit;
                    widest = widest.max(width);
                }
                if widest > 0.0 {
                    let size = super::headers::fitted_width(widest);
                    size_commands.push(Command::Sheet(SheetCommand::SetSize {
                        slide,
                        axis: ibis_doc_model::sheet_command::Axis::Columns,
                        first: col,
                        last: col,
                        size,
                    }));
                }
            }
        }
        if !size_commands.is_empty() {
            if let Err(err) = self.document.amend_last(size_commands) {
                log::warn!("autofit columns amend rejected: {err}");
            }
        }

        // The chart: the values, the forecast and its bounds over the timeline, beside the data.
        let quoted = ibis_pptx::quote_sheet(&name);
        let rows = |c: u32| ibis_pptx::format_ref(&quoted, CellRange { first_row: 1, first_column: c, last_row: built.last_row, last_column: c });
        let heading = |c: u32| ibis_pptx::format_ref(&quoted, CellRange { first_row: 0, first_column: c, last_row: 0, last_column: c });
        let refs: Vec<ibis_pptx::SeriesRefs> = (1..5).map(|c| ibis_pptx::SeriesRefs { name: Some(heading(c)), categories: Some(rows(0)), values: Some(rows(c)), ..Default::default() }).collect();
        let kind = if columns { NewChartKind::ClusteredColumn } else { NewChartKind::Line };
        let message = match self.source.clone() {
            Some(source) => {
                let current = self.presentation().clone();
                let (x, y) = match self.current_sheet() {
                    Some((shape, sheet)) => {
                        let left = if statistics { 9 } else { 6 };
                        (shape.frame.x + sheet.column_offsets().get(left).copied().unwrap_or(0), shape.frame.y + sheet.row_offsets().get(1).copied().unwrap_or(0))
                    }
                    None => (952_500, 952_500),
                };
                let id = self.document.ids().new_shape_id();
                match source.new_sheet_chart(&current, slide, id, EmuRect::new(x, y, 5_486_400, 3_200_400), kind, refs) {
                    Ok(chart) => {
                        let index = self.slide().shapes.len();
                        match self.document.amend_last(vec![Command::InsertShape { slide, index, shape: chart }]) {
                            Ok(()) => None,
                            Err(err) => Some(format("edit.refused", &[("error", &err)])),
                        }
                    }
                    Err(err) => Some(err.to_string()),
                }
            }
            None => None,
        };
        self.wb_mut().panels.forecast = None;
        cx.notify();
        Ok(message.unwrap_or_else(|| format("xlsx.forecast.message.done", &[("name", &name)])))
    }

    pub(crate) fn forecast_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.forecast.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let caption = |id: &str| div().text_sm().child(text(id));
        let toggle = |id: &'static str, on: bool, label: &'static str, set: Rc<dyn Fn(&mut ForecastPanel)>| {
            mark(
                id,
                crate::shell::chrome::radio_button(id, on, false, text(label), cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.forecast.as_mut() {
                            set(p);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();
        body = body
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(caption("xlsx.forecast.field.timeline")),
            )
            .child(mark("timeline", super::range_field::local_range_field(&p.timeline,"forecast_sheet-p-timeline",false,cx).into_any_element()));
        body = body
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(caption("xlsx.forecast.field.values")),
            )
            .child(mark("values", super::range_field::local_range_field(&p.values,"forecast_sheet-p-values",false,cx).into_any_element()));
        // The sheet is made by its 作成 button, not on Enter: the ▲▼ only change the boxes. 開始・終了 are timeline values (dates or
        // numbers) and have none.
        body = body.child(caption("xlsx.forecast.field.start")).child(mark("start", crate::shell::chrome::text_field(&p.start).full_width().into_any_element()));
        body = body.child(caption("xlsx.forecast.field.end")).child(mark("end", crate::shell::chrome::text_field(&p.end).full_width().into_any_element()));
        body = body.child(caption("xlsx.forecast.field.confidence")).child(mark("confidence", crate::shell::chrome::text_field(&p.confidence).unit("%",px(20.)).spinner("forecast-confidence-spin", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 99.0), cx, |_, _, _, _| {}).into_any_element()));

        // 季節性
        body = body.child(caption("xlsx.forecast.field.seasonality")).child(
            h_flex()
                .gap_1()
                .child(toggle("seasonality-auto", !p.seasonality_manual, "xlsx.forecast.seasonality-auto", Rc::new(|p| p.seasonality_manual = false)))
                .child(toggle("seasonality-manual", p.seasonality_manual, "xlsx.forecast.seasonality-manual", Rc::new(|p| p.seasonality_manual = true))),
        );
        if p.seasonality_manual {
            body = body.child(mark("seasonality", crate::shell::chrome::text_field(&p.seasonality).full_width().spinner("forecast-seasonality-spin", crate::shell::chrome::SpinnerSpec::new(1.0).min(0.0).integer_only(), cx, |_, _, _, _| {}).into_any_element()));
        }

        // 欠損ポイントの補完
        body = body.child(caption("xlsx.forecast.field.data-completion")).child(
            h_flex()
                .gap_1()
                .child(toggle("completion-interpolate", !p.data_completion_zero, "xlsx.forecast.completion-interpolate", Rc::new(|p| p.data_completion_zero = false)))
                .child(toggle("completion-zero", p.data_completion_zero, "xlsx.forecast.completion-zero", Rc::new(|p| p.data_completion_zero = true))),
        );

        // 重複データの集計
        let agg_id = match p.aggregation {
            2 => "xlsx.forecast.agg-count",
            3 => "xlsx.forecast.agg-counta",
            4 => "xlsx.forecast.agg-max",
            5 => "xlsx.forecast.agg-median",
            6 => "xlsx.forecast.agg-min",
            7 => "xlsx.forecast.agg-sum",
            _ => "xlsx.forecast.agg-average",
        };
        let choices=[(1,"xlsx.forecast.agg-average"),(2,"xlsx.forecast.agg-count"),(3,"xlsx.forecast.agg-counta"),(4,"xlsx.forecast.agg-max"),(5,"xlsx.forecast.agg-median"),(6,"xlsx.forecast.agg-min"),(7,"xlsx.forecast.agg-sum")].into_iter().map(|(agg,label)|crate::shell::chrome::MenuItem::new("forecast-aggregation-choice",text(label)).checked(agg==p.aggregation).on(move|this,_,cx|{if let Some(p)=this.wb_mut().panels.forecast.as_mut(){p.aggregation=agg;}cx.notify();}).into()).collect();
        body=body.child(crate::shell::chrome::dropdown_row(text("xlsx.forecast.field.aggregation"),mark("aggregation",div().w(crate::shell::chrome::FIELD_WIDTH).child(crate::shell::chrome::Dropdown::from_entries("forecast-agg",text(agg_id),choices,true,0,&cx.weak_entity()).into_any_element()).into_any_element())));

        body = body.child(caption("xlsx.forecast.field.chart")).child(
            h_flex()
                .gap_1()
                .child(toggle("chart-line", !p.columns, "xlsx.forecast.chart-line", Rc::new(|p| p.columns = false)))
                .child(toggle("chart-column", p.columns, "xlsx.forecast.chart-column", Rc::new(|p| p.columns = true))),
        );
        let on = p.statistics;
        body = body.child(mark(
            "statistics",
            crate::shell::chrome::checkbox_button("forecast-statistics", text("xlsx.forecast.field.statistics"), on, false, cx.listener(|this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.forecast.as_mut() {
                        p.statistics = !p.statistics;
                    }
                    cx.notify();
                }))
                .into_any_element(),
        ));
        if !p.message.is_empty() {
            body = body.children(crate::shell::chrome::panel_message(&p.message));
        }
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("forecast-apply", "", crate::shell::chrome::ActionButtonKind::Primary).label(text("xlsx.forecast.create")).on_click(cx.listener(|this, _, window, cx| this.apply_forecast_panel(window, cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("xlsx.forecast.title"), body.into_any_element(), panel_terminal_actions))
    }
}
