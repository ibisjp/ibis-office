// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > グラフ in a document: an inline chart (DrawingML `c:chart` in a `w:drawing`) of a kind
//! the menu offers (the same kinds and sample data as a presentation's, `ibis_pptx::NewChartKind`),
//! at the caret, 6 × 3.5 inches as Word makes it. It is drawn as the charts a file has are.
use gpui_kit::*;
use ibis_doc_model::{Chart, ChartType, Fill, Stroke, EmuSize, FlowCommand, FlowPos, FlowRunKind, Shape, ShapeId, ShapeKind, SlideId};
use ibis_pptx::NewChartKind;

use crate::pptx::chart_panel::{ChartPanel, ChartPreset, chart_with_preset, ChartPanelActions, chart_panel_sidebar};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// Word's size for a new chart: 6 × 3.5 inches.
const CHART_SIZE: EmuSize = EmuSize::new(5_486_400, 3_200_400);

impl SlideEditor {
    /// 挿入 > グラフ: a chart of `kind` at the caret (the selection replaced), selected.
    pub(crate) fn doc_insert_chart(&mut self, kind: NewChartKind, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else { return };
        let Some((_, caret)) = self.text_selection() else { return };
        let Some(flow) = self.flow().cloned() else { return };
        let id = ShapeId(flow.new_shape_id());
        let current = self.presentation().clone();
        let shape = match source.new_doc_chart(&current, caret, id, CHART_SIZE, kind) {
            Ok(shape) => shape,
            Err(err) => {
                log::warn!("Cannot insert chart: {err}");
                self.set_status(err.to_string(), cx);
                return;
            }
        };
        debug_assert!(matches!(shape.kind, ShapeKind::Chart(_)));
        if self.run_flow(vec![FlowCommand::InsertChart { at: caret, shape: Box::new(Shape { floating: None, ..shape }) }], cx) {
            self.set_caret(FlowPos::new(caret.para, caret.offset), false, cx);
            self.set_caret(FlowPos::new(caret.para, caret.offset + 1), true, cx);
        }
    }

    /// The chart selected (the one position of its paragraph the selection covers): where it is, its shape
    /// id and the chart.
    pub(crate) fn selected_doc_chart(&self) -> Option<(FlowPos, ShapeId, Chart)> {
        let (a, b) = self.ordered_selection()?;
        if a.para != b.para || b.offset != a.offset + 1 {
            return None;
        }
        let p = ibis_doc_model::flow_command::paragraph(self.flow()?, a.para)?;
        let mut at = 0;
        for run in &p.runs {
            if at == a.offset && let FlowRunKind::Shape(shape) = &run.kind && let ShapeKind::Chart(chart) = &shape.kind {
                return Some((a, shape.id, (**chart).clone()));
            }
            at += run.len();
        }
        None
    }

    /// Whether the selected chart can be edited here: not a chart of a new kind (chartEx), whose data
    /// is edited in its workbook.
    pub(crate) fn can_edit_doc_chart(&self) -> bool {
        self.selected_doc_chart().is_some_and(|(_, _, chart)| chart.ex.is_none())
    }

    /// グラフの種類の変更: the selected chart made `preset` (one step of undo).
    pub(crate) fn set_doc_chart_type(&mut self, preset: ChartPreset, cx: &mut Context<Self>) {
        let Some((at, _, old)) = self.selected_doc_chart().filter(|(_, _, c)| c.ex.is_none()) else { return };
        match chart_with_preset(old.clone(), preset) {
            Ok(mut chart) => {
                // What the file reads back as: a bar's or area's fill is a line's colour, and back.
                let line = |c: &Chart| c.groups.first().is_some_and(|g| matches!(g.kind, ChartType::Line));
                match (line(&old), line(&chart)) {
                    (false, true) => {
                        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                            if let Fill::Solid(color) = series.fill {
                                series.stroke = Some(Stroke::solid(color, 28_575));
                            }
                            series.fill = Fill::None;
                        }
                    }
                    (true, false) => {
                        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                            if let Some(stroke) = series.stroke.take() {
                                series.fill = Fill::Solid(stroke.color);
                            }
                        }
                    }
                    _ => {}
                }
                self.run_flow(vec![FlowCommand::SetChart { at, chart: Box::new(chart) }], cx);
            }
            Err(key) => self.set_status(crate::shell::i18n::text(key).to_string(), cx),
        }
    }

    /// The 書式 menu's グラフの種類 for the selected chart.
    pub(crate) fn doc_chart_type_entry(&self) -> crate::shell::chrome::MenuEntry {
        use crate::shell::chrome::{MenuItem, Submenu};
        let label = crate::shell::i18n::text("pptx.chrome.change-chart-type");
        if !self.can_edit_doc_chart() {
            return MenuItem::new("docx.chart.type-none", label).icon("bar_chart").disabled(true).into();
        }
        let chart = self.selected_doc_chart().map(|(_, _, c)| c);
        let items = crate::pptx::chrome::chart_type_items_for(chart.as_ref(), Self::set_doc_chart_type);
        Submenu::new("docx.chart.type", "bar_chart", label, items).into()
    }

    /// データの編集: the panel for the selected chart.
    pub(crate) fn open_doc_chart_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if !self.can_edit_doc_chart() {
            return;
        }
        let Some((at, id, chart)) = self.selected_doc_chart() else { return };
        let panel = ChartPanel::new(&chart, (SlideId(0), id), window, cx);
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = super::help_panels::HelpPanels::default();
            s.helper.chart = Some((panel, at));
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_doc_chart_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.helper.chart = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// 適用: the chart as typed in the panel, in one step of undo.
    pub(crate) fn apply_doc_chart_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((panel, at)) = self.doc_state().and_then(|s| s.helper.chart.as_ref()) else { return };
        let at = *at;
        let Some((_, _, chart)) = self.selected_doc_chart().filter(|(a, ..)| *a == at) else { return };
        match panel.apply_to(chart.clone(), cx) {
            Ok(new) => {
                if new != chart {
                    self.run_flow(vec![FlowCommand::SetChart { at, chart: Box::new(new) }], cx);
                }
                cx.notify();
            }
            Err(error) => {
                if let Some((p, _)) = self.doc_state_mut().and_then(|s| s.helper.chart.as_mut()) {
                    p.set_error(error);
                }
                cx.notify();
            }
        }
    }

    pub(crate) fn doc_chart_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (p, _) = self.doc_state()?.helper.chart.as_ref()?;
        let actions = ChartPanelActions {
            reversed: |e, cx| {
                if let Some((p, _)) = e.doc_state_mut().and_then(|s| s.helper.chart.as_mut()) {
                    p.toggle_reversed();
                    cx.notify();
                }
            },
            log: |e, cx| {
                if let Some((p, _)) = e.doc_state_mut().and_then(|s| s.helper.chart.as_mut()) {
                    p.toggle_log_scale();
                    cx.notify();
                }
            },
            apply: Self::apply_doc_chart_panel,
            cancel: Self::close_doc_chart_panel,
        };
        Some(chart_panel_sidebar(p, actions, cx))
    }
}
