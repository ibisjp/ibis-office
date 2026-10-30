// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! グラフのデータの編集: the selected chart's data in a grid (its
//! categories down, its series across, their names on top) and its title,
//! typed in and applied as one change (see `save/pptx_chart.rs` for what
//! saving writes).

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputState,InputEvent};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{
    AxisKind, Chart, ChartType, Color, Command, ErrorBarType, ErrorBarValueType, ErrorBars, Fill, Grouping, LabelPosition, Number,
    ShapeId, ShapeKind, SlideId, Stroke, TrendlineType, chart::{Marker, MarkerSymbol},
};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The panel's fields.
pub(crate) struct ChartPanel {
    target: (SlideId, ShapeId),
    pub(crate) title: Option<Entity<InputState>>,
    /// The series' names, in the chart's order (all groups).
    pub(crate) names: Vec<Entity<InputState>>,
    /// The categories (the first series' that has them).
    pub(crate) categories: Vec<Entity<InputState>>,
    /// Each series' values.
    pub(crate) values: Vec<Vec<Entity<InputState>>>,
    /// Value axis scaling & units.
    pub(crate) val_min: Option<Entity<InputState>>,
    pub(crate) val_max: Option<Entity<InputState>>,
    pub(crate) val_major: Option<Entity<InputState>>,
    pub(crate) val_reversed: bool,
    pub(crate) val_log_scale: bool,
    error: Option<SharedString>,
    subscriptions:Vec<Subscription>,
}

fn chart_field_subscription(input:&Entity<InputState>,w:&mut Window,cx:&mut Context<SlideEditor>)->Subscription{
 cx.subscribe_in(input,w,|e,_,ev:&InputEvent,w,cx|if matches!(ev,InputEvent::Change|InputEvent::PressEnter{..}){e.apply_embedded_chart_data(w,cx);})
}
impl SlideEditor {
 fn apply_embedded_chart_data(&mut self,w:&mut Window,cx:&mut Context<Self>){if self.is_document(){self.apply_doc_chart_panel(w,cx);}else{self.apply_chart_panel(w,cx);}}
 fn extend_embedded_chart_data(&mut self,series:bool,w:&mut Window,cx:&mut Context<Self>){
  let p=if self.is_document(){self.doc_state_mut().and_then(|s|s.helper.chart.as_mut().map(|(p,_)|p))}else{self.pptx_mut().and_then(|s|s.chart_panel.as_mut())};let Some(p)=p else{return;};
  let mut added=Vec::new();let mut field=|value:String|{let input=cx.new(|cx|InputState::new(w,cx).default_value(value));added.push(input.clone());input};
  if series {p.names.push(field(format!("{} {}",crate::shell::i18n::text("xlsx.chart-series"),p.names.len()+1)));let n=p.values.iter().map(Vec::len).max().unwrap_or(p.categories.len()).max(1);p.values.push((0..n).map(|_|field(String::new())).collect());}
  else {p.categories.push(field((p.categories.len()+1).to_string()));for values in &mut p.values{values.push(field(String::new()));}}
  for input in added {p.subscriptions.push(chart_field_subscription(&input,w,cx));}
  self.apply_embedded_chart_data(w,cx);cx.notify();
 }
}
impl ChartPanel {
    /// The fields for `chart` (`target`: where it is, for the editor that holds the panel).
    pub(crate) fn new(chart: &Chart, target: (SlideId, ShapeId), window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let mut field = |text: String| cx.new(|cx| InputState::new(window, cx).default_value(text));
        let title = chart.title.as_ref().map(|t| field(t.text.clone()));
        let series: Vec<_> = chart.groups.iter().flat_map(|g| g.series.iter()).collect();
        let names = series.iter().map(|s| field(s.name.clone())).collect();
        let categories = series.iter().find(|s| !s.categories.is_empty()).map(|s| s.categories.iter().map(|c| field(c.clone())).collect()).unwrap_or_default();
        let values = series.iter().map(|s| s.values.iter().map(|v| field(shown(*v))).collect()).collect();
        let val_axis = chart.axes.iter().find(|a| a.kind == AxisKind::Value);
        let val_min = val_axis.map(|a| field(shown(a.min)));
        let val_max = val_axis.map(|a| field(shown(a.max)));
        let val_major = val_axis.map(|a| field(shown(a.major_unit)));
        let val_reversed = val_axis.is_some_and(|a| a.reversed);
        let val_log_scale = val_axis.is_some_and(|a| a.log_base.is_some());
        let mut panel=ChartPanel { target, title, names, categories, values, val_min, val_max, val_major, val_reversed, val_log_scale, error: None,subscriptions:vec![] };
        let fields=panel.title.iter().chain(panel.names.iter()).chain(panel.categories.iter()).chain(panel.values.iter().flatten()).chain(panel.val_min.iter()).chain(panel.val_max.iter()).chain(panel.val_major.iter()).cloned().collect::<Vec<_>>();
        for input in fields{panel.subscriptions.push(chart_field_subscription(&input,window,cx));}panel
    }

    /// `chart` with what was typed in the fields; the message when a value is not a number.
    pub(crate) fn apply_to(&self, mut chart: Chart, cx: &App) -> Result<Chart, SharedString> {
        let panel = self;
        let read = |e: &Entity<InputState>| e.read(cx).value().to_string();
        if let (Some(t), Some(field)) = (chart.title.as_mut(), &panel.title) {
            t.text = read(field);
        }
        let old_categories: Option<Vec<String>> = chart.groups.iter().flat_map(|g| g.series.iter()).find(|s| !s.categories.is_empty()).map(|s| s.categories.clone());
        let categories: Vec<String> = panel.categories.iter().map(read).collect();
        let existing=chart.groups.iter().map(|g|g.series.len()).sum::<usize>();
        if panel.names.len()>existing {
            let template=chart.groups.iter().flat_map(|g|&g.series).last().cloned();
            if let (Some(template),Some(group))=(template,chart.groups.last_mut()) {for _ in existing..panel.names.len(){group.series.push(template.clone());}}
        }
        let mut bad = false;
        let mut index = 0;
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if let Some(name) = panel.names.get(index) {
                series.name = read(name);
            }
            // The same categories are every series'.
            if old_categories.as_ref().is_some_and(|c| *c == series.categories) {
                series.categories = categories.clone();
            }
            if let Some(fields) = panel.values.get(index) {
                series.values.resize(fields.len(),None);
                for (v, field) in series.values.iter_mut().zip(fields) {
                    match typed(&read(field)) {
                        Ok(x) => *v = x,
                        Err(()) => bad = true,
                    }
                }
            }
            index += 1;
        }
        let mut min_val = None;
        let mut max_val = None;
        let mut major_val = None;
        if let Some(f) = &panel.val_min {
            match typed(&read(f)) {
                Ok(v) => min_val = v,
                Err(()) => bad = true,
            }
        }
        if let Some(f) = &panel.val_max {
            match typed(&read(f)) {
                Ok(v) => max_val = v,
                Err(()) => bad = true,
            }
        }
        if let Some(f) = &panel.val_major {
            match typed(&read(f)) {
                Ok(v) => major_val = v,
                Err(()) => bad = true,
            }
        }
        if bad {
            return Err(crate::shell::i18n::text("pptx.chart-panel.values-must-numeric-blank-auto"));
        }
        if let Some(v_axis) = chart.axes.iter_mut().find(|a| a.kind == AxisKind::Value) {
            v_axis.min = min_val;
            v_axis.max = max_val;
            v_axis.major_unit = major_val;
            v_axis.reversed = panel.val_reversed;
            v_axis.log_base = if panel.val_log_scale { Some(Number(10.0)) } else { None };
        }
        Ok(chart)
    }
}

/// A value as the grid shows it.
fn shown(v: Option<Number>) -> String {
    v.map(|Number(x)| format!("{x}")).unwrap_or_default()
}

/// A typed value: empty is no value; else a number (`1,234.5` too).
fn typed(text: &str) -> Result<Option<Number>, ()> {
    let t = text.trim().replace(',', "");
    if t.is_empty() {
        return Ok(None);
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite()).map(|v| Some(Number(v))).ok_or(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChartLabelFlag {
    Value,
    Category,
    SeriesName,
    Percent,
}

impl SlideEditor {
    /// The selected chart, and where it is.
    pub(crate) fn selected_chart(&self) -> Option<(SlideId, ShapeId, Chart)> {
        let shape = self.selected_shapes().into_iter().next()?;
        match &shape.kind {
            ShapeKind::Chart(c) => Some((self.current_slide, shape.id, (**c).clone())),
            _ => None,
        }
    }

    /// 挿入 > グラフ: a clustered column chart of PowerPoint's sample data
    /// in the middle of the slide (its data then edited in the panel).
    pub(crate) fn insert_slide_chart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.insert_slide_chart_kind(ibis_pptx::NewChartKind::ClusteredColumn, window, cx);
    }

    /// 挿入 > グラフ: insert chart of specified kind.
    pub(crate) fn insert_slide_chart_kind(&mut self, kind: ibis_pptx::NewChartKind, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let Some(source) = self.source.clone() else { return };
        // As PowerPoint: in the slide's empty content placeholder (which it
        // takes the place of), else in the middle of the slide.
        let placeholder = self.empty_content_placeholder_for("chart");
        let replaced = placeholder.map(|(old_id, _)| old_id);
        let frame = match placeholder {
            Some((_, ph_frame)) => ph_frame,
            None => {
                let size = self.presentation().size_of(self.slide());
                let (width, height) = (size.width * 3 / 4, size.height * 3 / 4);
                ibis_doc_model::EmuRect::new((size.width - width) / 2, (size.height - height) / 2, width, height)
            }
        };
        let id = self.document.ids().new_shape_id();
        let slide = self.current_slide;
        let current = self.presentation().clone();
        let shape = match source.new_chart_kind(&current, slide, id, frame, kind) {
            Ok(shape) => shape,
            Err(err) => {
                log::warn!("Cannot insert chart: {err}");
                return;
            }
        };
        let mut commands = Vec::new();
        if let Some(old) = replaced {
            commands.push(Command::RemoveShapes { slide, shapes: vec![old] });
        }
        let index = self.slide().shapes.len() - usize::from(replaced.is_some());
        commands.push(Command::InsertShape { slide, index, shape });
        self.run(commands, cx);
        self.set_selection(vec![id]);
        // A chart of a new kind (chartEx) has its data edited in its workbook (データの編集), not in the panel.
        if !kind.is_chart_ex() {
            self.open_chart_panel(window, cx);
        }
    }

    /// データの編集: the panel for the selected chart.
    pub(crate) fn open_chart_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let Some((slide, shape, chart)) = self.selected_chart() else { return };
        // A chart of a new kind (chartEx): its data is edited in its workbook (データの編集).
        if chart.ex.is_some() {
            return self.open_chart_data(window, cx);
        }
        let panel = ChartPanel::new(&chart, (slide, shape), window, cx);
        if let Some(s) = self.pptx_mut() {
            s.timing_panel = None; s.section_panel = None; s.gradient_panel = None; s.shadow_panel = None; s.glow_panel = None; s.paragraph_panel = None; s.find = None; s.size_panel = None; s.insets_panel = None; s.crop_panel = None; s.frame_panel = None; s.comments = None; s.header_footer = None;
            s.chart_panel = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn apply_chart_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.chart_panel.as_ref()) else { return };
        let (slide, shape) = panel.target;
        let Some(ShapeKind::Chart(chart)) = self.presentation().slide(slide).and_then(|s| s.shape(shape)).map(|s| s.kind.clone()) else { return };
        let chart = match panel.apply_to(*chart, cx) {
            Ok(chart) => chart,
            Err(error) => {
                if let Some(p) = self.pptx_mut().and_then(|s| s.chart_panel.as_mut()) {
                    p.error = Some(error);
                }
                return cx.notify();
            }
        };
        let current = self.presentation().slide(slide).and_then(|s| s.shape(shape)).map(|s| s.kind.clone());
        if current != Some(ShapeKind::Chart(Box::new(chart.clone()))) {
            self.run(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(chart))) }], cx);
        }
        cx.notify();
    }

    /// データラベル: 位置や有無を設定。
    /// `position == None` なら非表示（None）。
    /// `position == Some(pos)` なら表示（`pos` は `Option<LabelPosition>` で、`None` は自動位置、`Some(p)` は指定位置）。
    pub(crate) fn set_chart_labels_position(&mut self, position: Option<Option<LabelPosition>>, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            series.labels = match (position, series.labels.take()) {
                (None, _) => None,
                (Some(pos), Some(l)) => Some(ibis_doc_model::DataLabels { position: pos, ..l }),
                (Some(pos), None) => Some(ibis_doc_model::DataLabels { show_value: true, position: pos, ..Default::default() }),
            };
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// データラベル: the selected chart's series' values shown as labels, or
    /// no labels. Read back through the file, as a new legend is.
    #[allow(dead_code)]
    pub(crate) fn set_chart_labels(&mut self, show: bool, cx: &mut Context<Self>) {
        if show {
            self.set_chart_labels_position(Some(None), cx);
        } else {
            self.set_chart_labels_position(None, cx);
        }
    }

    /// データラベルの表示項目（値、項目名、系列名、パーセント）をトグル。
    pub(crate) fn toggle_chart_label_flag(&mut self, flag: ChartLabelFlag, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            let mut labels = series.labels.take().unwrap_or_else(|| ibis_doc_model::DataLabels { ..Default::default() });
            match flag {
                ChartLabelFlag::Value => labels.show_value = !labels.show_value,
                ChartLabelFlag::Category => labels.show_category = !labels.show_category,
                ChartLabelFlag::SeriesName => labels.show_series_name = !labels.show_series_name,
                ChartLabelFlag::Percent => labels.show_percent = !labels.show_percent,
            }
            series.labels = Some(labels);
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// グラフの種類の変更: the selected chart's one group made `preset`
    /// (the file's reading of it, as a new legend is).
    pub(crate) fn set_chart_type(&mut self, preset: ChartPreset, cx: &mut Context<Self>) {
        let Some((slide, shape, chart)) = self.selected_chart() else { return };
        match chart_with_preset(chart, preset) {
            Ok(chart) => self.set_chart_through_file(slide, shape, chart, cx),
            Err(key) => self.set_status(crate::shell::i18n::text(key).to_string(), cx),
        }
    }

    /// 最初の扇形の角度: the selected chart's first slice angle (Pie or Doughnut).
    pub(crate) fn set_chart_first_slice_angle(&mut self, angle: u32, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let [group] = chart.groups.as_mut_slice() else { return };
        match &mut group.kind {
            ChartType::Pie { first_angle } | ChartType::Doughnut { first_angle, .. } => {
                *first_angle = angle % 360;
            }
            _ => return,
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// ドーナツの穴の大きさ: the selected chart's doughnut hole size (Doughnut only, 1..90).
    pub(crate) fn set_chart_doughnut_hole_size(&mut self, hole_size: u32, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let [group] = chart.groups.as_mut_slice() else { return };
        match &mut group.kind {
            ChartType::Doughnut { hole, .. } => {
                *hole = hole_size.clamp(1, 90);
            }
            _ => return,
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// バブルの大きさ（倍率 %）: the selected chart's bubble scale (Bubble only, 1..300).
    pub(crate) fn set_chart_bubble_scale(&mut self, scale: u32, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let [group] = chart.groups.as_mut_slice() else { return };
        match &mut group.kind {
            ChartType::Bubble { scale: s } => {
                *s = scale.clamp(1, 300);
            }
            _ => return,
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 散布図の平滑線: the selected chart's scatter smooth curves (Scatter only).
    pub(crate) fn set_chart_scatter_smooth(&mut self, smooth: bool, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let [group] = chart.groups.as_mut_slice() else { return };
        match &mut group.kind {
            ChartType::Scatter { smooth: s } => {
                *s = smooth;
            }
            _ => return,
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// Chart `shape` set to `chart` as the file reads it back.
    fn set_chart_through_file(&mut self, slide: SlideId, shape: ShapeId, chart: Chart, cx: &mut Context<Self>) {
        let kind = ShapeKind::Chart(Box::new(chart));
        let kind = match &self.source {
            Some(source) => {
                let mut changed = self.presentation().clone();
                if let Some(target) = changed.slides.iter_mut().find(|s| s.id == slide).and_then(|s| s.shapes.iter_mut().find(|x| x.id == shape)) {
                    target.kind = kind.clone();
                }
                match source.as_read(&changed, slide, shape) {
                    Ok(read) => read.kind,
                    Err(err) => {
                        log::warn!("Cannot change chart: {err}");
                        return;
                    }
                }
            }
            None => kind,
        };
        if self.presentation().slide(slide).and_then(|s| s.shape(shape)).is_some_and(|s| s.kind != kind) {
            self.run(vec![Command::SetKind { slide, shape, kind: Box::new(kind) }], cx);
        }
    }

    /// chartEx のグラフ: the chart's `ex_format` changed by `change` (read back through the file, as the new title is).
    fn change_chart_ex_format(&mut self, change: impl FnOnce(&mut ibis_doc_model::ExFormat), cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let Some(format) = chart.ex_format.as_mut() else { return };
        change(format);
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// データ ラベル (chartEx): none (`None`) or what they show and where.
    pub(crate) fn set_chart_ex_labels(&mut self, labels: Option<ibis_doc_model::ExLabels>, cx: &mut Context<Self>) {
        self.change_chart_ex_format(|f| f.labels = labels, cx);
    }

    /// 値軸の最小・最大・主目盛 (chartEx): each `Some(value)` sets it (`value` `None`: automatic).
    pub(crate) fn set_chart_ex_axis(&mut self, min: Option<Option<Number>>, max: Option<Option<Number>>, major: Option<Option<Number>>, cx: &mut Context<Self>) {
        self.change_chart_ex_format(
            |f| {
                if let Some(v) = min {
                    f.axis_min = v;
                }
                if let Some(v) = max {
                    f.axis_max = v;
                }
                if let Some(v) = major {
                    f.axis_major = v;
                }
            },
            cx,
        );
    }

    /// 系列の塗りつぶし (chartEx): the fill of series `index` (a waterfall's increase, decrease or total), `None` the chart's own.
    pub(crate) fn set_chart_ex_fill(&mut self, index: usize, fill: Option<ibis_doc_model::Rgb>, cx: &mut Context<Self>) {
        self.change_chart_ex_format(
            |f| {
                if f.fills.len() <= index {
                    f.fills.resize(index + 1, None);
                }
                f.fills[index] = fill;
            },
            cx,
        );
    }

    /// グラフタイトル: the selected chart's title shown or hidden. The new
    /// chart is the file's reading of it (its text takes the chart's).
    pub(crate) fn set_chart_title(&mut self, show: bool, cx: &mut Context<Self>) {
        use ibis_doc_model::{ChartText, ChartTitle};
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let original = self.source.as_ref().and_then(|s| s.baseline().slide(slide)?.shape(shape).cloned()).and_then(|s| match s.kind {
            ShapeKind::Chart(c) => c.title,
            _ => None,
        });
        chart.title = match (show, chart.title.take().or(original)) {
            (false, _) => None,
            (true, Some(t)) => Some(ChartTitle { at: None, ..t }),
            (true, None) => {
                let base_style = chart.axes.first().map(|a| a.text.clone()).unwrap_or_default();
                Some(ChartTitle {
                    text: crate::shell::i18n::text("_xlsx.chart-title").to_string(),
                    style: ChartText { size: 1400, ..base_style },
                    at: None,
                })
            }
        };
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 凡例: the selected chart's legend hidden (`None`) or on a side. The
    /// new chart is the file's reading of it (its text takes the chart's).
    pub(crate) fn set_chart_legend(&mut self, position: Option<ibis_doc_model::LegendPosition>, cx: &mut Context<Self>) {
        use ibis_doc_model::{Legend, LegendPosition as P, Side};
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let side = |p: P| match p {
            P::Bottom => Side::Bottom,
            P::Left => Side::Left,
            P::Top | P::TopRight => Side::Top,
            P::Right => Side::Right,
        };
        // Shown again: the file's own legend, if it had one (its look kept).
        let original = self.source.as_ref().and_then(|s| s.baseline().slide(slide)?.shape(shape).cloned()).and_then(|s| match s.kind {
            ShapeKind::Chart(c) => c.legend,
            _ => None,
        });
        chart.legend = match (position, chart.legend.take().or(original)) {
            (None, _) => None,
            (Some(p), Some(l)) => Some(Legend { source_position: Some(p), side: side(p), layout: None, ..l }),
            (Some(p), None) => Some(Legend { source_position: Some(p), side: side(p), overlay: Some(false), layout: None, text: Default::default(), hidden: Vec::new() }),
        };
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    pub(crate) fn close_chart_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.chart_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn chart_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.chart_panel.as_ref()?;
        let actions = ChartPanelActions { reversed: Self::toggle_chart_panel_val_reversed, log: Self::toggle_chart_panel_val_log_scale, apply: Self::apply_chart_panel, cancel: Self::close_chart_panel };
        Some(chart_panel_sidebar(p, actions, cx))
    }
}

/// What the panel's buttons do (the editor holds the panel: a slide's, a document's).
#[derive(Clone, Copy)]
pub(crate) struct ChartPanelActions {
    pub(crate) reversed: fn(&mut SlideEditor, &mut Context<SlideEditor>),
    pub(crate) log: fn(&mut SlideEditor, &mut Context<SlideEditor>),
    pub(crate) apply: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>),
    pub(crate) cancel: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>),
}

impl ChartPanel {
    pub(crate) fn set_error(&mut self, error: SharedString) {
        self.error = Some(error);
    }

    pub(crate) fn toggle_reversed(&mut self) {
        self.val_reversed = !self.val_reversed;
    }

    pub(crate) fn toggle_log_scale(&mut self) {
        self.val_log_scale = !self.val_log_scale;
    }
}

/// データの編集's panel: the grid of the chart's data, its title and value axis.
pub(crate) fn chart_panel_sidebar(p: &ChartPanel, actions: ChartPanelActions, cx: &mut Context<SlideEditor>) -> SidebarPanel {
    {
        let cell = |e: &Entity<InputState>| div().w(px(84.)).flex_none().child(crate::shell::chrome::text_field(e).full_width());
        let label = |t: &str| div().w(px(84.)).flex_none().text_sm().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text(t));
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        body=body.child(h_flex().gap_2().child(crate::shell::chrome::action_button("chart-data-add-row").label(crate::shell::i18n::text("chart.data.add-row")).on_click(cx.listener(|e,_,w,cx|e.extend_embedded_chart_data(false,w,cx))))
          .child(crate::shell::chrome::action_button("chart-data-add-series").label(crate::shell::i18n::text("chart.data.add-series")).on_click(cx.listener(|e,_,w,cx|e.extend_embedded_chart_data(true,w,cx)))));

        if let Some(title) = &p.title {
            body = body.child(div().text_sm().child(crate::shell::i18n::text("_shell.title"))).child(crate::shell::chrome::text_field(title).full_width());
        }
        // Names across the top, then a row per category.
        let mut grid = v_flex().gap_1();
        let mut head = h_flex().gap_1().child(label("pptx.pivot.category"));
        for name in &p.names {
            head = head.child(cell(name));
        }
        grid = grid.child(head);
        let rows = p.values.iter().map(Vec::len).max().unwrap_or(0);
        for r in 0..rows {
            let mut row = h_flex().gap_1();
            row = match p.categories.get(r) {
                Some(c) => row.child(cell(c)),
                None => row.child(label(&format!("{}", r + 1))),
            };
            for series in &p.values {
                row = match series.get(r) {
                    Some(v) => row.child(cell(v)),
                    None => row.child(div().w(px(84.)).flex_none()),
                };
            }
            grid = grid.child(row);
        }
        body = body.child(div().text_sm().child(crate::shell::i18n::text("pptx.chart-panel.series-names-categories-values"))).child(div().id("pptx-chart-grid").overflow_x_scroll().child(grid));
        if p.val_min.is_some() || p.val_max.is_some() || p.val_major.is_some() {
            body = body.child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("pptx.chart.axis-settings")));
            let mut axis_fields = h_flex().gap_2();
            if let Some(min_in) = &p.val_min {
                axis_fields = axis_fields.child(v_flex().gap_1().child(label("pptx.chart.axis.min")).child(cell(min_in)));
            }
            if let Some(max_in) = &p.val_max {
                axis_fields = axis_fields.child(v_flex().gap_1().child(label("pptx.chart.axis.max")).child(cell(max_in)));
            }
            if let Some(major_in) = &p.val_major {
                axis_fields = axis_fields.child(v_flex().gap_1().child(label("pptx.chart.axis.major")).child(cell(major_in)));
            }
            body = body.child(axis_fields);

            let rev_btn = crate::shell::chrome::checkbox_button("pptx-chart-val-reversed", crate::shell::i18n::text("pptx.chart.axis.reversed"), p.val_reversed, false, cx.listener(move |this, _, w, cx| {(actions.reversed)(this,cx);(actions.apply)(this,w,cx);}));
            let log_btn = crate::shell::chrome::checkbox_button("pptx-chart-val-log", crate::shell::i18n::text("pptx.chart.axis.log-scale"), p.val_log_scale, false, cx.listener(move |this, _, w, cx| {(actions.log)(this,cx);(actions.apply)(this,w,cx);}));
            body = body.child(h_flex().gap_2().child(rev_btn).child(log_btn));
        }
        if let Some(error) = &p.error {
            body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error.clone()));
        }
        SidebarPanel{title:crate::shell::i18n::text("pptx.chart-data.chart-data"),body:body.into_any_element()}

    }
}

impl SlideEditor {
    pub(crate) fn toggle_chart_panel_val_reversed(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.chart_panel.as_mut()) {
            p.toggle_reversed();
            cx.notify();
        }
    }

    pub(crate) fn toggle_chart_panel_val_log_scale(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.chart_panel.as_mut()) {
            p.toggle_log_scale();
            cx.notify();
        }
    }

    /// 軸を取得して更新
    fn update_chart_axis(&mut self, is_value: bool, cx: &mut Context<Self>, f: impl FnOnce(&mut ibis_doc_model::Axis)) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let kind = if is_value { ibis_doc_model::AxisKind::Value } else { ibis_doc_model::AxisKind::Category };
        if let Some(axis) = chart.axes.iter_mut().find(|a| a.kind == kind) {
            f(axis);
            self.set_chart_through_file(slide, shape, chart, cx);
        }
    }

    /// 軸の最小値を設定 (None は自動)
    pub(crate) fn set_chart_axis_min(&mut self, is_value: bool, min: Option<Number>, cx: &mut Context<Self>) {
        self.update_chart_axis(is_value, cx, |a| a.min = min);
    }

    /// 軸の最大値を設定 (None は自動)
    pub(crate) fn set_chart_axis_max(&mut self, is_value: bool, max: Option<Number>, cx: &mut Context<Self>) {
        self.update_chart_axis(is_value, cx, |a| a.max = max);
    }

    /// 軸の主目盛単位を設定 (None は自動)
    pub(crate) fn set_chart_axis_major_unit(&mut self, is_value: bool, unit: Option<Number>, cx: &mut Context<Self>) {
        self.update_chart_axis(is_value, cx, |a| a.major_unit = unit);
    }

    /// 軸の補助目盛単位を設定 (None は自動)
    #[allow(dead_code)]
    pub(crate) fn set_chart_axis_minor_unit(&mut self, is_value: bool, unit: Option<Number>, cx: &mut Context<Self>) {
        self.update_chart_axis(is_value, cx, |a| a.minor_unit = unit);
    }

    /// 軸の表示単位を設定 (None は通常表示)
    pub(crate) fn set_chart_axis_display_unit(&mut self, is_value: bool, unit: Option<Number>, cx: &mut Context<Self>) {
        self.update_chart_axis(is_value, cx, |a| a.display_unit = unit);
    }

    /// 軸の対数目盛を設定 (Some(Number(10.0)) で対数目盛、None で線形)
    pub(crate) fn set_chart_axis_log_scale(&mut self, is_value: bool, log_base: Option<Number>, cx: &mut Context<Self>) {
        self.update_chart_axis(is_value, cx, |a| a.log_base = log_base);
    }

    /// 軸の対数目盛をトグル切り替え
    pub(crate) fn toggle_chart_axis_log_scale(&mut self, is_value: bool, cx: &mut Context<Self>) {
        let Some((_, _, chart)) = self.selected_chart() else { return };
        let kind = if is_value { ibis_doc_model::AxisKind::Value } else { ibis_doc_model::AxisKind::Category };
        let is_log = chart.axes.iter().find(|a| a.kind == kind).is_some_and(|a| a.log_base.is_some());
        let new_base = if is_log { None } else { Some(Number(10.0)) };
        self.set_chart_axis_log_scale(is_value, new_base, cx);
    }

    /// 軸の反転を設定 (true で反転)
    pub(crate) fn set_chart_axis_reversed(&mut self, is_value: bool, reversed: bool, cx: &mut Context<Self>) {
        self.update_chart_axis(is_value, cx, |a| a.reversed = reversed);
    }

    /// 軸の反転をトグル切り替え
    pub(crate) fn toggle_chart_axis_reversed(&mut self, is_value: bool, cx: &mut Context<Self>) {
        let Some((_, _, chart)) = self.selected_chart() else { return };
        let kind = if is_value { ibis_doc_model::AxisKind::Value } else { ibis_doc_model::AxisKind::Category };
        let is_rev = chart.axes.iter().find(|a| a.kind == kind).is_some_and(|a| a.reversed);
        self.set_chart_axis_reversed(is_value, !is_rev, cx);
    }

    /// 近似曲線の種類を設定 (None は近似曲線なし、Some はその種類に設定)
    pub(crate) fn set_chart_trendline_type(&mut self, kind: Option<TrendlineType>, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            match kind {
                None => series.trendline = None,
                Some(k) => {
                    let mut tl = series.trendline.clone().unwrap_or_default();
                    tl.kind = k;
                    series.trendline = Some(tl);
                }
            }
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 近似曲線の数式表示を設定
    pub(crate) fn set_chart_trendline_disp_eq(&mut self, disp_eq: bool, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if let Some(tl) = series.trendline.as_mut() {
                tl.disp_eq = disp_eq;
            }
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 近似曲線のR-2乗値表示を設定
    pub(crate) fn set_chart_trendline_disp_r_sqr(&mut self, disp_r_sqr: bool, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if let Some(tl) = series.trendline.as_mut() {
                tl.disp_r_sqr = disp_r_sqr;
            }
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 近似曲線の有無をトグル切り替え（なし <-> 線形）
    #[allow(dead_code)]
    pub(crate) fn toggle_chart_trendline(&mut self, cx: &mut Context<Self>) {
        let Some((_, _, chart)) = self.selected_chart() else { return };
        let has_any = chart.groups.iter().flat_map(|g| &g.series).any(|s| s.trendline.is_some());
        self.set_chart_trendline_type(if has_any { None } else { Some(TrendlineType::Linear) }, cx);
    }

    /// 誤差範囲の表示/非表示を設定 (全系列)
    pub(crate) fn set_chart_error_bars_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if shown {
                if series.error_bars.is_none() {
                    series.error_bars = Some(ErrorBars::default());
                }
            } else {
                series.error_bars = None;
            }
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 誤差範囲の表示方向（Both / Plus / Minus）を設定
    pub(crate) fn set_chart_error_bar_type(&mut self, bar_type: ErrorBarType, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            let mut eb = series.error_bars.clone().unwrap_or_default();
            eb.bar_type = bar_type;
            series.error_bars = Some(eb);
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 誤差範囲の計算方法（Fixed / Percentage / StdDev / StdErr）を設定
    pub(crate) fn set_chart_error_bar_val_type(&mut self, val_type: ErrorBarValueType, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            let mut eb = series.error_bars.clone().unwrap_or_default();
            eb.val_type = val_type;
            series.error_bars = Some(eb);
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 誤差範囲の末端キャップ表示を設定
    pub(crate) fn set_chart_error_bar_end_cap(&mut self, end_cap: bool, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if let Some(eb) = series.error_bars.as_mut() {
                eb.no_end_cap = !end_cap;
            }
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 誤差範囲の有無をトグル切り替え
    #[allow(dead_code)]
    pub(crate) fn toggle_chart_error_bars(&mut self, cx: &mut Context<Self>) {
        let Some((_, _, chart)) = self.selected_chart() else { return };
        let has_any = chart.groups.iter().flat_map(|g| &g.series).any(|s| s.error_bars.is_some());
        self.set_chart_error_bars_shown(!has_any, cx);
    }

    /// 系列の塗りつぶしを設定 (series_idx が None の場合は全系列)
    pub(crate) fn set_chart_series_fill(&mut self, series_idx: Option<usize>, fill: Fill, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let mut cur = 0;
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if series_idx.is_none() || series_idx == Some(cur) {
                series.fill = fill.clone();
            }
            cur += 1;
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 系列の枠線（stroke）を設定 (series_idx が None の場合は全系列)
    pub(crate) fn set_chart_series_stroke(&mut self, series_idx: Option<usize>, stroke: Option<Stroke>, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let mut cur = 0;
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if series_idx.is_none() || series_idx == Some(cur) {
                series.stroke = stroke.clone();
            }
            cur += 1;
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 系列の枠線の太さを設定 (series_idx が None の場合は全系列)
    pub(crate) fn set_chart_series_stroke_width(&mut self, series_idx: Option<usize>, width: i64, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let mut cur = 0;
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if series_idx.is_none() || series_idx == Some(cur) {
                if let Some(s) = series.stroke.as_mut() {
                    s.width = width;
                } else {
                    series.stroke = Some(Stroke::solid(Color::BLACK, width));
                }
            }
            cur += 1;
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// 系列のマーカー記号を設定 (series_idx が None の場合は全系列)
    pub(crate) fn set_chart_series_marker_symbol(&mut self, series_idx: Option<usize>, symbol: MarkerSymbol, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let mut cur = 0;
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if series_idx.is_none() || series_idx == Some(cur) {
                if symbol == MarkerSymbol::None {
                    series.marker = None;
                } else if let Some(m) = series.marker.as_mut() {
                    m.symbol = symbol;
                } else {
                    series.marker = Some(Marker {
                        source: None,
                        symbol,
                        size: 7,
                        fill: Fill::Solid(Color::opaque(ibis_doc_model::Rgb(0x4472c4))),
                    });
                }
            }
            cur += 1;
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// プロット領域の背景塗りつぶしを設定
    pub(crate) fn set_chart_plot_background(&mut self, fill: Fill, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        chart.plot_background = fill;
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// グラフのスタイル（c:style val="N"）を設定
    pub(crate) fn set_chart_style(&mut self, style_id: Option<u32>, cx: &mut Context<Self>) {
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        chart.style = style_id;
        self.set_chart_through_file(slide, shape, chart, cx);
    }

    /// グラフの配色パレットを全系列に適用
    pub(crate) fn set_chart_color_palette(&mut self, palette_index: usize, cx: &mut Context<Self>) {
        let Some((_, colors)) = COLOR_PALETTES.get(palette_index) else { return };
        let colors = *colors;
        let Some((slide, shape, mut chart)) = self.selected_chart() else { return };
        let mut i = 0;
        for series in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            let hex = colors[i % colors.len()];
            let color = Color::opaque(ibis_doc_model::Rgb(hex));
            series.fill = Fill::Solid(color);
            if let Some(s) = series.stroke.as_mut() {
                s.color = color;
            } else {
                series.stroke = Some(Stroke::solid(color, 28_575));
            }
            if let Some(m) = series.marker.as_mut() {
                m.fill = Fill::Solid(color);
            }
            i += 1;
        }
        self.set_chart_through_file(slide, shape, chart, cx);
    }
}

/// The chart visual styles offered (c:style).
pub(crate) const CHART_STYLES: [(&str, Option<u32>); 9] = [
    ("pptx.chart.style.none", None),
    ("pptx.chart.style.1", Some(1)),
    ("pptx.chart.style.2", Some(2)),
    ("pptx.chart.style.3", Some(3)),
    ("pptx.chart.style.4", Some(4)),
    ("pptx.chart.style.5", Some(5)),
    ("pptx.chart.style.6", Some(6)),
    ("pptx.chart.style.7", Some(7)),
    ("pptx.chart.style.8", Some(8)),
];

/// The color palettes offered for series coloring.
pub(crate) const COLOR_PALETTES: [(&str, &[u32]); 10] = [
    ("pptx.chart.palette.1", &[0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47]),
    ("pptx.chart.palette.2", &[0x2F5597, 0xC55A11, 0x7B7B7B, 0xBF8F00, 0x1F4E79, 0x385723]),
    ("pptx.chart.palette.3", &[0x41719C, 0x548235, 0x7030A0, 0xC55A11, 0xBF8F00, 0xC00000]),
    ("pptx.chart.palette.4", &[0x1F4E79, 0x833C0C, 0x595959, 0x7F6000, 0x385723, 0x3B3838]),
    ("pptx.chart.palette.5", &[0x2F5597, 0x4472C4, 0x5B9BD5, 0x8EA9DB, 0xB4C6E7, 0xD9E1F2]),
    ("pptx.chart.palette.6", &[0xC55A11, 0xED7D31, 0xF4B084, 0xF8CBAD, 0xFCE4D6, 0xFFF2CC]),
    ("pptx.chart.palette.7", &[0x595959, 0x7B7B7B, 0xA5A5A5, 0xBFBFBF, 0xD9D9D9, 0xF2F2F2]),
    ("pptx.chart.palette.8", &[0xBF8F00, 0xFFC000, 0xFFD966, 0xFFE699, 0xFFF2CC, 0xF7BA0B]),
    ("pptx.chart.palette.9", &[0x385723, 0x548235, 0x70AD47, 0xA9D08E, 0xC6E0B4, 0xE2EFDA]),
    ("pptx.chart.palette.10", &[0xC00000, 0xED1C24, 0xF15A24, 0xF7931E, 0xF9A03F, 0xFBB040]),
];

/// The chart types 変更 offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChartPreset {
    Column,
    ColumnStacked,
    ColumnPercent,
    Bar,
    BarStacked,
    BarPercent,
    Line,
    Area,
    AreaStacked,
    Pie,
    Doughnut,
    Scatter,
    ScatterSmooth,
    Bubble,
}

impl ChartPreset {
    pub(crate) const ALL: [(ChartPreset, &'static str); 14] = [
        (ChartPreset::Column, "chart.type.column"),
        (ChartPreset::ColumnStacked, "chart.type.column-stacked"),
        (ChartPreset::ColumnPercent, "chart.type.column-percent"),
        (ChartPreset::Bar, "chart.type.bar"),
        (ChartPreset::BarStacked, "chart.type.bar-stacked"),
        (ChartPreset::BarPercent, "chart.type.bar-percent"),
        (ChartPreset::Line, "chart.type.line"),
        (ChartPreset::Area, "chart.type.area"),
        (ChartPreset::AreaStacked, "chart.type.area-stacked"),
        (ChartPreset::Pie, "chart.type.pie"),
        (ChartPreset::Doughnut, "chart.type.doughnut"),
        (ChartPreset::Scatter, "chart.type.scatter"),
        (ChartPreset::ScatterSmooth, "chart.type.scatter-smooth"),
        (ChartPreset::Bubble, "chart.type.bubble"),
    ];

    /// The kind and grouping, with PowerPoint's gap and overlap for them.
    pub(crate) fn parts(self) -> (ChartType, Grouping) {
        use ChartPreset::*;
        let bar = |horizontal: bool, stacked: bool| ChartType::Bar { horizontal, gap_width: if stacked { 150 } else { 219 }, overlap: if stacked { 100 } else { -27 } };
        match self {
            Column => (bar(false, false), Grouping::Standard),
            ColumnStacked => (bar(false, true), Grouping::Stacked),
            ColumnPercent => (bar(false, true), Grouping::PercentStacked),
            Bar => (bar(true, false), Grouping::Standard),
            BarStacked => (bar(true, true), Grouping::Stacked),
            BarPercent => (bar(true, true), Grouping::PercentStacked),
            Line => (ChartType::Line, Grouping::Standard),
            Area => (ChartType::Area, Grouping::Standard),
            AreaStacked => (ChartType::Area, Grouping::Stacked),
            Pie => (ChartType::Pie { first_angle: 0 }, Grouping::Standard),
            Doughnut => (ChartType::Doughnut { first_angle: 0, hole: 75 }, Grouping::Standard),
            Scatter => (ChartType::Scatter { smooth: false }, Grouping::Standard),
            ScatterSmooth => (ChartType::Scatter { smooth: true }, Grouping::Standard),
            Bubble => (ChartType::Bubble { scale: 100 }, Grouping::Standard),
        }
    }

    /// The preset a group is.
    pub(crate) fn of(kind: ChartType, grouping: Grouping) -> Option<ChartPreset> {
        Self::ALL.iter().map(|(p, _)| *p).find(|p| {
            let (k, g) = p.parts();
            g == grouping && match (k, kind) {
                (ChartType::Bar { horizontal: a, .. }, ChartType::Bar { horizontal: b, .. }) => a == b,
                (ChartType::Pie { .. }, ChartType::Pie { .. }) => true,
                (ChartType::Doughnut { .. }, ChartType::Doughnut { .. }) => true,
                (ChartType::Scatter { smooth: a }, ChartType::Scatter { smooth: b }) => a == b,
                (ChartType::Bubble { .. }, ChartType::Bubble { .. }) => true,
                (a, b) => a == b,
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{shown, typed};
    use ibis_doc_model::Number;

    #[test]
    fn values_are_typed_as_numbers() {
        assert_eq!(typed(" 1,234.5 "), Ok(Some(Number(1234.5))));
        assert_eq!(typed(""), Ok(None));
        assert_eq!(typed("abc"), Err(()));
        assert_eq!(shown(Some(Number(12.0))), "12");
    }
}

/// `chart` with its one group made `preset`: an error is the key of the message why it cannot be
/// (a combo chart; a kind of chart that has no such presets; a change between axis, pie and scatter charts).
pub(crate) fn chart_with_preset(mut chart: Chart, preset: ChartPreset) -> Result<Chart, &'static str> {
    let [group] = chart.groups.as_mut_slice() else { return Err("pptx.chart-panel.type-combo-chart-cannot-changed") };
    let is_axis_bearing = matches!(group.kind, ChartType::Bar { .. } | ChartType::Line | ChartType::Area);
    let is_pie_family = matches!(group.kind, ChartType::Pie { .. } | ChartType::Doughnut { .. });
    let is_scatter_family = matches!(group.kind, ChartType::Scatter { .. } | ChartType::Bubble { .. });
    if !is_axis_bearing && !is_pie_family && !is_scatter_family {
        return Err("pptx.chart-panel.chart-type-cannot-changed");
    }
    let (kind, grouping) = preset.parts();
    let target_is_pie_family = matches!(kind, ChartType::Pie { .. } | ChartType::Doughnut { .. });
    let target_is_scatter_family = matches!(kind, ChartType::Scatter { .. } | ChartType::Bubble { .. });
    let target_is_axis_bearing = matches!(kind, ChartType::Bar { .. } | ChartType::Line | ChartType::Area);
    if (is_axis_bearing && !target_is_axis_bearing) || (is_pie_family && !target_is_pie_family) || (is_scatter_family && !target_is_scatter_family) {
        return Err("pptx.chart-panel.cannot-convert-between-charts");
    }
    let kind = match (group.kind, kind) {
        (ChartType::Pie { first_angle }, ChartType::Doughnut { hole, .. }) => ChartType::Doughnut { first_angle, hole },
        (ChartType::Doughnut { first_angle, .. }, ChartType::Pie { .. }) => ChartType::Pie { first_angle },
        (ChartType::Bubble { scale }, ChartType::Bubble { .. }) => ChartType::Bubble { scale },
        _ => kind,
    };
    (group.kind, group.grouping) = (kind, grouping);
    Ok(chart)
}
