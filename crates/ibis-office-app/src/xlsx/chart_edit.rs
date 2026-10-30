// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! グラフの書式 in the sidebar, for the chart selected on a sheet: the
//! title (shown or not, its text), the legend's place (or none), and the
//! value axis's minimum, maximum and major unit (each automatic or a
//! number) and its major gridlines. Each change is one step to undo;
//! saving writes it into the chart's part (`ibis-pptx` save/xlsx.rs).

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::chart::{
    AxisKind, ChartDataTable, ChartText, ChartTitle, DataLabels, ErrorBarType, ErrorBarValueType,
    LabelPosition, Legend, LegendPosition, Marker, MarkerSymbol, Number, Side,
    TrendlineType,
};
use ibis_doc_model::{CellRange, Chart, Color, Command, DispBlanksAs, Fill, Rgb, ShapeId, ShapeKind, Stroke};

use crate::shell::chrome::SidebarPanel;
use crate::shell::chrome::segment_control::{SegmentItem,Mode};
use crate::shell::chrome::SegmentControlRow;
use crate::shell::editor::SlideEditor;

/// The fields of the panel, for the chart they were filled from.
pub(crate) struct ChartPanel {
    pub(crate) shape: ShapeId,
    pub(crate) tab: usize,
    pub(crate) rendering_sidebar: bool,
    pub(crate) axis_tab: usize,
    pub(crate) palette_selected: Option<usize>,
    pub(crate) detail_open: [bool;3],
    pub(crate) detail_state: [std::rc::Rc<crate::shell::chrome::collapsible_section::SectionState>;3],
    pub(crate) series_editor: Option<super::chart_series::SeriesEditor>,
    pub(crate) series_reorder: std::rc::Rc<std::cell::RefCell<crate::shell::chrome::data_table::RowReorder>>,
    pub(crate) title: Entity<InputState>,
    pub(crate) background: Entity<InputState>,
    pub(crate) cat_title: Entity<InputState>,
    pub(crate) val_title: Entity<InputState>,
    pub(crate) sec_val_title: Entity<InputState>,
    pub(crate) ser_title: Entity<InputState>,
    pub(crate) domain_min: Entity<InputState>,
    pub(crate) domain_max: Entity<InputState>,
    pub(crate) domain_unit: Entity<InputState>,
    pub(crate) min: Entity<InputState>,
    pub(crate) max: Entity<InputState>,
    pub(crate) unit: Entity<InputState>,
    pub(crate) gap_width: Entity<InputState>,
    pub(crate) overlap: Entity<InputState>,
    pub(crate) of_pie_split_pos: Entity<InputState>,
    pub(crate) of_pie_second_size: Entity<InputState>,
    /// 3-D 回転: X 回転、Y 回転、奥行き (%)
    pub(crate) three_d_rot_x: Entity<InputState>,
    pub(crate) three_d_rot_y: Entity<InputState>,
    pub(crate) three_d_depth: Entity<InputState>,
    /// データの範囲 (`月次売上!$A$3:$D$8`) the series take their data from.
    pub(crate) range: Entity<InputState>,
    /// A histogram's bins (新しいグラフ): the width, the number, the underflow and the overflow bins.
    pub(crate) ex_bin_width: Entity<InputState>,
    pub(crate) ex_bin_count: Entity<InputState>,
    pub(crate) ex_underflow: Entity<InputState>,
    pub(crate) ex_overflow: Entity<InputState>,
    pub(crate) ex_gap_width: Entity<InputState>,
    pub(crate) error: Option<SharedString>,
}

/// The legend places offered, as the panel names them.
pub(crate) const LEGEND_PLACES: [(&str, Option<LegendPosition>); 5] =
    [("_shell.right", Some(LegendPosition::Right)), ("_shell.top", Some(LegendPosition::Top)), ("_shell.bottom", Some(LegendPosition::Bottom)), ("_shell.left", Some(LegendPosition::Left)), ("_shell.none", None)];

/// The data label positions offered, as the panel names them.
pub(crate) const LABEL_POSITIONS: [(&str, Option<Option<LabelPosition>>); 5] = [
    ("_shell.none", None),
    ("_shell.center", Some(Some(LabelPosition::Center))),
    ("_xlsx.inside-base", Some(Some(LabelPosition::InsideBase))),
    ("_xlsx.inside-end", Some(Some(LabelPosition::InsideEnd))),
    ("_xlsx.outside-end", Some(Some(LabelPosition::OutsideEnd))),
];

/// The display units offered for value axes.
pub(crate) const DISPLAY_UNITS: [(&str, Option<f64>); 5] = [
    ("_shell.none", None),
    ("pptx.chart.axis.disp-hundreds", Some(1e2)),
    ("pptx.chart.axis.disp-thousands", Some(1e3)),
    ("_xlsx.ten-thousands", Some(1e4)),
    ("pptx.chart.axis.disp-millions", Some(1e6)),
];

/// The trendline types offered.
pub(crate) const TRENDLINE_TYPES: [(&str, Option<TrendlineType>); 7] = [
    ("_shell.none", None),
    ("pptx.chart.trendline.linear", Some(TrendlineType::Linear)),
    ("pptx.chart.trendline.exp", Some(TrendlineType::Exponential)),
    ("pptx.chart.trendline.log", Some(TrendlineType::Logarithmic)),
    ("pptx.chart.trendline.poly", Some(TrendlineType::Polynomial)),
    ("pptx.chart.trendline.power", Some(TrendlineType::Power)),
    ("pptx.chart.trendline.moving-avg", Some(TrendlineType::MovingAverage)),
];

/// The error bar directions / types offered.
pub(crate) const ERROR_BAR_TYPES: [(&str, ErrorBarType); 3] = [
    ("pptx.chart.error-bars.both", ErrorBarType::Both),
    ("_xlsx.plus", ErrorBarType::Plus),
    ("_xlsx.minus", ErrorBarType::Minus),
];

/// The error bar calculation value types offered.
pub(crate) const ERROR_BAR_VAL_TYPES: [(&str, ErrorBarValueType); 4] = [
    ("docx.common.exactly", ErrorBarValueType::FixedValue),
    ("fix.chart-edit.percent", ErrorBarValueType::Percentage),
    ("pptx.chart.error-bars.std-dev", ErrorBarValueType::StandardDeviation),
    ("pptx.chart.error-bars.std-err", ErrorBarValueType::StandardError),
];

/// How blank cells are plotted, as the panel names them.
pub(crate) const BLANK_CELL_OPTIONS: [(&str, Option<DispBlanksAs>); 3] = [
    ("_xlsx.gaps", Some(DispBlanksAs::Gap)),
    ("_xlsx.zero", Some(DispBlanksAs::Zero)),
    ("_xlsx.connect-data-points-with-line", Some(DispBlanksAs::Span)),
];

/// The marker symbols offered, as the panel names them.
pub(crate) const MARKER_SYMBOLS: [(&str, MarkerSymbol); 5] = [
    ("_shell.none", MarkerSymbol::None),
    ("pptx.common.circle", MarkerSymbol::Circle),
    ("pptx.chrome.square", MarkerSymbol::Square),
    ("_shell.diamond", MarkerSymbol::Diamond),
    ("_shell.triangle", MarkerSymbol::Triangle),
];

/// The radar chart styles offered, as the panel names them.
pub(crate) const RADAR_STYLES: [(&str, Option<ibis_doc_model::RadarStyle>); 3] = [
    ("_xlsx.radar", Some(ibis_doc_model::RadarStyle::Standard)),
    ("_xlsx.radar-with-markers", Some(ibis_doc_model::RadarStyle::Marker)),
    ("pptx.common.fill", Some(ibis_doc_model::RadarStyle::Filled)),
];

/// The ofPie chart kinds offered (pie of pie or bar of pie).
pub(crate) const OF_PIE_KINDS: [(&str, Option<ibis_doc_model::OfPieKind>); 2] = [
    ("fix.chart-edit.pie-pie", Some(ibis_doc_model::OfPieKind::Pie)),
    ("fix.chart-edit.bar-pie", Some(ibis_doc_model::OfPieKind::Bar)),
];

/// The ofPie chart split types offered (position, value, percent).
pub(crate) const OF_PIE_SPLITS: [(&str, Option<ibis_doc_model::OfPieSplit>); 3] = [
    ("_shell.position", Some(ibis_doc_model::OfPieSplit::Position)),
    ("chart.labels.show-value", Some(ibis_doc_model::OfPieSplit::Value)),
    ("fix.chart-edit.percent", Some(ibis_doc_model::OfPieSplit::Percent)),
];

/// The line widths offered for series strokes.
pub(crate) const LINE_WIDTHS: [(&str, Option<i64>); 4] = [
    ("_shell.none", None),
    ("_xlsx.thin-2", Some(12_700)),  // 1 pt
    ("_xlsx.standard", Some(28_575)), // 2.25 pt
    ("_xlsx.thick-2", Some(38_100)),  // 3 pt
];

/// The category axis type options (text or date).
pub(crate) const AXIS_TYPE_OPTIONS: [(&str, bool); 2] = [
    ("_xlsx.text-axis", false),
    ("xlsx.sparkline-date-axis-menu", true),
];

/// The surface chart subtype options: (label, is_3d, wireframe).
pub(crate) const SURFACE_TYPES: [(&str, bool, bool); 4] = [
    ("fix.chart-edit.3-d-surface", true, false),
    ("fix.chart-edit.3-d-wireframe", true, true),
    ("fix.chart-edit.contour", false, false),
    ("fix.chart-edit.wireframe-contour", false, true),
];

/// The fill colors offered for series fill.
pub(crate) const FILL_COLORS: [(&str, Option<Rgb>); 7] = [
    ("_shell.blue", Some(Rgb(0x4472C4))),
    ("xlsx.sparkline-color-orange", Some(Rgb(0xED7D31))),
    ("xlsx.sparkline-color-gray", Some(Rgb(0xA5A5A5))),
    ("_shell.yellow", Some(Rgb(0xFFC000))),
    ("_shell.green", Some(Rgb(0x70AD47))),
    ("_shell.red", Some(Rgb(0xC00000))),
    ("_shell.none", None),
];

/// The stroke colors offered for series outline.
pub(crate) const STROKE_COLORS: [(&str, Option<Rgb>); 7] = [
    ("_shell.blue", Some(Rgb(0x4472C4))),
    ("xlsx.sparkline-color-orange", Some(Rgb(0xED7D31))),
    ("xlsx.sparkline-color-gray", Some(Rgb(0xA5A5A5))),
    ("_shell.green", Some(Rgb(0x70AD47))),
    ("_shell.red", Some(Rgb(0xC00000))),
    ("_shell.black", Some(Rgb(0x000000))),
    ("_shell.none", None),
];

pub(crate) use super::chart_style::{CHART_STYLES,ChartStylePreset};

/// The color palettes offered for series coloring.
pub(crate) const COLOR_PALETTES: [(&str, &[u32]); 10] = [
    ("xlsx.chart-palette-1", &[0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47]),
    ("xlsx.chart-palette-2", &[0x2F5597, 0xC55A11, 0x7B7B7B, 0xBF8F00, 0x1F4E79, 0x385723]),
    ("xlsx.chart-palette-3", &[0x41719C, 0x548235, 0x7030A0, 0xC55A11, 0xBF8F00, 0xC00000]),
    ("xlsx.chart-palette-4", &[0x1F4E79, 0x833C0C, 0x595959, 0x7F6000, 0x385723, 0x3B3838]),
    ("_xlsx.monochrome-1", &[0x2F5597, 0x4472C4, 0x5B9BD5, 0x8EA9DB, 0xB4C6E7, 0xD9E1F2]),
    ("_xlsx.monochrome-2", &[0xC55A11, 0xED7D31, 0xF4B084, 0xF8CBAD, 0xFCE4D6, 0xFFF2CC]),
    ("_xlsx.monochrome-3", &[0x595959, 0x7B7B7B, 0xA5A5A5, 0xBFBFBF, 0xD9D9D9, 0xF2F2F2]),
    ("_xlsx.monochrome-4", &[0xBF8F00, 0xFFC000, 0xFFD966, 0xFFE699, 0xFFF2CC, 0xF7BA0B]),
    ("_xlsx.monochrome-5", &[0x385723, 0x548235, 0x70AD47, 0xA9D08E, 0xC6E0B4, 0xE2EFDA]),
    ("_xlsx.monochrome-6", &[0xC00000, 0xED1C24, 0xF15A24, 0xF7931E, 0xF9A03F, 0xFBB040]),
];

/// The primary domain axis: category/date, or a scatter/bubble's horizontal value axis.
pub(crate) fn category_axis(chart:&Chart)->Option<usize>{
    let referenced=|i:usize|chart.groups.iter().any(|g|g.axes.contains(&chart.axes[i].id));
    chart.axes.iter().enumerate().find(|(i,a)|!a.deleted&&a.kind==AxisKind::Category&&referenced(*i)).map(|(i,_)|i)
        .or_else(||chart.axes.iter().position(|a|!a.deleted&&a.kind==AxisKind::Category))
        .or_else(||chart.axes.iter().enumerate().find(|(i,a)|!a.deleted&&a.kind==AxisKind::Value&&matches!(a.side,Side::Bottom|Side::Top)&&referenced(*i)).map(|(i,_)|i))
}
/// The primary range axis connected to the primary domain axis. Both scatter axes are Value.
pub(crate) fn value_axis(chart:&Chart)->Option<usize>{
    let domain=category_axis(chart);
    let domain_id=domain.map(|i|chart.axes[i].id);
    let horizontal_values=domain.is_some_and(|i|matches!(chart.axes[i].side,Side::Left|Side::Right));
    let orientation=|a:&ibis_doc_model::chart::Axis|if horizontal_values{matches!(a.side,Side::Bottom|Side::Top)}else{matches!(a.side,Side::Left|Side::Right)};
    let connected=|a:&ibis_doc_model::chart::Axis|domain_id.is_some_and(|id|a.cross_axis_id==Some(id)||chart.groups.iter().any(|g|g.axes.contains(&id)&&g.axes.contains(&a.id)));
    chart.axes.iter().enumerate().filter(|(i,a)|Some(*i)!=domain&&!a.deleted&&a.kind==AxisKind::Value)
        .max_by_key(|(_,a)| (connected(a),orientation(a),matches!(a.side,Side::Left|Side::Bottom)))
        .map(|(i,_)|i)
}
/// A second range axis, excluding the scatter/bubble's primary X/domain axis.
pub(crate) fn secondary_value_axis(chart:&Chart)->Option<usize>{
    let primary=value_axis(chart)?;let domain=category_axis(chart);
    let vertical=matches!(chart.axes[primary].side,Side::Left|Side::Right);
    chart.axes.iter().enumerate().find(|(i,a)|*i!=primary&&Some(*i)!=domain&&!a.deleted&&a.kind==AxisKind::Value&&matches!(a.side,Side::Left|Side::Right)==vertical).map(|(i,_)|i)
}

/// The chart's series axis (for 3D charts), by index.
pub(crate) fn series_axis(chart: &Chart) -> Option<usize> {
    chart.axes.iter().position(|a| a.kind == AxisKind::Series && !a.deleted)
}

fn number_field(n: Option<Number>) -> String {
    n.map_or(String::new(), |v| ibis_calc::number_to_xml(v.0))
}

/// A field's number: `None` when empty (automatic), `Err` when not a number.
fn parse_field(text: &str) -> Result<Option<Number>, ()> {
    let t = text.trim().replace(',', "");
    if t.is_empty() || t == crate::shell::i18n::text("_shell.auto") || t == crate::shell::i18n::text("_shell.auto").as_ref() {
        return Ok(None);
    }
    t.parse::<f64>().map(|v| Some(Number(v))).map_err(|_| ())
}

impl SlideEditor {
    /// The chart selected on the current sheet.
    pub(crate) fn sheet_chart(&self) -> Option<(ShapeId, &Chart)> {
        if let Some(p) = self.sheet_state().and_then(|s| s.chart_edit.as_ref()) {
            if let Some(edit) = &p.series_editor { if !p.rendering_sidebar {return Some((p.shape, &edit.chart));} }
        }
        if self.sheet_range_pick_active() {
            let id = self.wb().chart_edit.as_ref()?.shape;
            let shape = self.presentation().slides.iter().flat_map(|slide| &slide.shapes).find(|shape| shape.id == id)?;
            if let ShapeKind::Chart(chart) = &shape.kind { return Some((id, chart)); }
        }
        let id = self.selection?;
        let shape = self.slide().shapes.iter().find(|s| s.id == id)?;
        match &shape.kind {
            ShapeKind::Chart(c) => Some((id, &**c)),
            _ => None,
        }
    }

    /// 表示形式 > グラフの書式…: the panel for the selected chart.
    pub(crate) fn open_sheet_chart_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, chart)) = self.sheet_chart() else {
            self.wb_mut().chart_edit=None;
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-chart").into());
            return cx.notify();
        };
        let title = chart.title.as_ref().map_or(String::new(), |t| t.text.clone());
        let background = match chart.background {Fill::Solid(c)=>format!("{:06X}",c.rgb.0),_=>String::new()};
        let cat_axis = category_axis(chart).map(|i| &chart.axes[i]);
        let val_axis = value_axis(chart).map(|i| &chart.axes[i]);
        let cat_title = cat_axis.and_then(|a| a.title.as_ref()).map_or(String::new(), |t| t.text.clone());
        let val_title = val_axis.and_then(|a| a.title.as_ref()).map_or(String::new(), |t| t.text.clone());
        let sec_val_axis = secondary_value_axis(chart).map(|i| &chart.axes[i]);
        let ser_axis = series_axis(chart).map(|i| &chart.axes[i]);
        let sec_val_title = sec_val_axis.and_then(|a| a.title.as_ref()).map_or(String::new(), |t| t.text.clone());
        let ser_title = ser_axis.and_then(|a| a.title.as_ref()).map_or(String::new(), |t| t.text.clone());
        let (min, max, unit) = (number_field(val_axis.and_then(|a| a.min)), number_field(val_axis.and_then(|a| a.max)), number_field(val_axis.and_then(|a| a.major_unit)));
        let (gap_width, overlap) = chart.groups.iter().find_map(|g| match g.kind {
            ibis_doc_model::ChartType::Bar { gap_width, overlap, .. } => Some((gap_width.to_string(), overlap.to_string())),
            _ => None,
        }).unwrap_or_default();
        let (of_pie_split_pos, of_pie_second_size) = chart.groups.iter().find_map(|g| {
            if matches!(g.kind, ibis_doc_model::ChartType::OfPie { .. }) {
                let pos = g.of_pie.as_ref().and_then(|op| op.split_position).map_or(String::new(), |p| ibis_calc::number_to_xml(p.0));
                let size = g.of_pie.as_ref().and_then(|op| op.second_size).map_or(String::new(), |s| s.to_string());
                Some((pos, size))
            } else {
                None
            }
        }).unwrap_or_default();
        let (three_d_rot_x, three_d_rot_y, three_d_depth) = chart.groups.iter().find_map(|g| g.three_d).map(|v| (v.rot_x.to_string(), v.rot_y.to_string(), v.depth_percent.to_string())).unwrap_or_default();
        let field = |text: String, cx: &mut Context<Self>, window: &mut Window| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_shell.auto")));
            input.update(cx, |i, cx| i.set_value(text, window, cx));
            input
        };
        let range = self.source.as_deref().and_then(|s| s.sheet_chart_refs(id, chart)).and_then(|refs| ibis_pptx::refs_range(&refs)).map(|(sheet, range, _)| ibis_pptx::format_ref(&sheet, range)).unwrap_or_default();
        let range_input = cx.new(|cx| InputState::new(window, cx));
        range_input.update(cx, |i, cx| i.set_value(range, window, cx));
        let ex = chart.ex_source.as_ref().map(|s| s.0.options()).unwrap_or_default();
        let ex_width = match ex.bin_by { ibis_doc_model::BinBy::Width(w) => ibis_calc::number_to_xml(w.0), _ => String::new() };
        let ex_count = match ex.bin_by { ibis_doc_model::BinBy::Count(c) => c.to_string(), _ => String::new() };
        let ex_under = ex.underflow.map_or(String::new(), |n| ibis_calc::number_to_xml(n.0));
        let ex_over = ex.overflow.map_or(String::new(), |n| ibis_calc::number_to_xml(n.0));
        let ex_gap = ex.gap_width.map_or(String::new(), |g| g.to_string());
        let domain_scale=(number_field(cat_axis.and_then(|a|a.min)),number_field(cat_axis.and_then(|a|a.max)),number_field(cat_axis.and_then(|a|a.major_unit)));
        let panel = ChartPanel {
            shape: id,
            palette_selected: None, rendering_sidebar: false, tab: self.document_memory.get::<usize>("xlsx.chart-format.tab").unwrap_or(0).min(2), axis_tab: 0, detail_open: [false;3], detail_state: Default::default(), series_editor: None, series_reorder: Default::default(),
            ex_bin_width: field(ex_width, cx, window),
            ex_bin_count: field(ex_count, cx, window),
            ex_underflow: field(ex_under, cx, window),
            ex_overflow: field(ex_over, cx, window),
            ex_gap_width: field(ex_gap, cx, window),
            title: field(title, cx, window),
            background: field(background, cx, window),
            cat_title: field(cat_title, cx, window),
            val_title: field(val_title, cx, window),
            sec_val_title: field(sec_val_title, cx, window),
            ser_title: field(ser_title, cx, window),
            domain_min:field(domain_scale.0,cx,window),
            domain_max:field(domain_scale.1,cx,window),
            domain_unit:field(domain_scale.2,cx,window),
            min: field(min, cx, window),
            max: field(max, cx, window),
            unit: field(unit, cx, window),
            gap_width: field(gap_width, cx, window),
            overlap: field(overlap, cx, window),
            of_pie_split_pos: field(of_pie_split_pos, cx, window),
            of_pie_second_size: field(of_pie_second_size, cx, window),
            three_d_rot_x: field(three_d_rot_x, cx, window),
            three_d_rot_y: field(three_d_rot_y, cx, window),
            three_d_depth: field(three_d_depth, cx, window),
            range: range_input,
            error: None,
        };
        for input in [&panel.domain_min,&panel.domain_max,&panel.domain_unit,&panel.title,&panel.cat_title,&panel.val_title,&panel.sec_val_title,&panel.ser_title,&panel.min,&panel.max,&panel.unit,&panel.gap_width,&panel.overlap,&panel.of_pie_split_pos,&panel.of_pie_second_size,&panel.three_d_rot_x,&panel.three_d_rot_y,&panel.three_d_depth] {
            cx.subscribe_in(input, window, |this,_,event:&gpui_kit::component::input::InputEvent,_,cx| {
                if matches!(event,gpui_kit::component::input::InputEvent::PressEnter{..}|gpui_kit::component::input::InputEvent::Blur)
                    && this.wb().chart_edit.as_ref().is_some_and(|p|p.series_editor.is_none()) {this.apply_sheet_chart_panel(cx);}
            }).detach();
        }
        cx.subscribe_in(&panel.title,window,|this,_,event:&gpui_kit::component::input::InputEvent,_,cx| {
            if matches!(event,gpui_kit::component::input::InputEvent::Change) {
                let text=this.wb().chart_edit.as_ref().map(|p|p.title.read(cx).value().to_string()).unwrap_or_default();
                this.change_sheet_chart(|c|{if let Some(title)=c.title.as_mut(){title.text=text;}},cx);
            }
        }).detach();
        cx.subscribe_in(&panel.background,window,|this,input,event:&gpui_kit::component::input::InputEvent,_,cx| {
            if !matches!(event,gpui_kit::component::input::InputEvent::Change|gpui_kit::component::input::InputEvent::PressEnter{..}) {return;}
            let value=input.read(cx).value().trim().trim_start_matches('#').to_string();
            let fill=if value.is_empty(){Fill::None}else{
                let Some(color)=u32::from_str_radix(&value,16).ok().filter(|_|value.len()==6) else{return;};
                Fill::Solid(Color {rgb:Rgb(color),alpha:255})
            };
            this.change_sheet_chart(|c|c.background=fill,cx);
        }).detach();
        cx.subscribe_in(&panel.range,window,|this,_,event:&gpui_kit::component::input::InputEvent,w,cx| {
            if matches!(event,gpui_kit::component::input::InputEvent::PressEnter{..}|gpui_kit::component::input::InputEvent::Blur)
                && !this.sheet_range_pick_active() && this.wb().chart_edit.as_ref().is_some_and(|p|p.series_editor.is_none()) {this.set_sheet_chart_data(false,w,cx);}
        }).detach();
        self.begin_sheet_settings();
        let state = self.wb_mut();
        state.chart_edit = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Change the selected chart by `f` (one step to undo).
    pub(crate) fn change_sheet_chart(&mut self, f: impl FnOnce(&mut Chart), cx: &mut Context<Self>) {
        if let Some(edit) = self.sheet_state_mut().and_then(|s| s.chart_edit.as_mut()).and_then(|p| p.series_editor.as_mut()) {
            let before=edit.chart.clone();f(&mut edit.chart);super::chart_style::inherit_new_text(&before,&mut edit.chart); self.commit_chart_series_model(cx); cx.notify(); return;
        }
        let Some((shape, chart)) = self.sheet_chart() else { return };
        let before = chart.clone();
        let mut chart = before.clone();
        f(&mut chart);
        super::chart_style::inherit_new_text(&before,&mut chart);
        if chart == before { return; }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(chart))) }]) {
            log::warn!("chart edit rejected: {err}");
        }
        cx.notify();
    }

    /// A chartEx chart's settings changed by `edit` (one step to undo): the chart made again from its cells with them.
    pub(crate) fn set_chart_ex_options(&mut self, edit: impl FnOnce(&mut ibis_doc_model::ChartExOptions), cx: &mut Context<Self>) {
        let Some((shape, chart)) = self.sheet_chart().map(|(id, c)| (id, c.clone())) else { return };
        let Some(source) = chart.ex_source.clone() else { return };
        let mut options = source.0.options();
        edit(&mut options);
        let Some(changed) = source.0.with_options(&options) else { return };
        let mut next = chart;
        next.ex_source = Some(ibis_doc_model::ExSourceRef(changed.0, source.1));
        let next = ibis_doc_model::chart_refresh::rebuilt_ex(self.presentation(), &next);
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(next))) }]) {
            log::warn!("chart edit rejected: {err}");
        }
        cx.notify();
    }

    /// ウォーターフォール: point `index` set as a total, or no longer one.
    pub(crate) fn toggle_chart_ex_total(&mut self, index: usize, cx: &mut Context<Self>) {
        self.set_chart_ex_options(
            |o| match o.subtotals.iter().position(|i| *i == index) {
                Some(at) => {
                    o.subtotals.remove(at);
                }
                None => o.subtotals.push(index),
            },
            cx,
        );
    }

    /// ヒストグラム: the bins of the fields (a width, else a number, else automatic; the under- and overflow values).
    pub(crate) fn apply_chart_ex_bins(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb().chart_edit.as_ref() else { return };
        let read = |input: &Entity<InputState>| input.read(cx).value().trim().to_string();
        let (width, count, under, over) = (read(&p.ex_bin_width), read(&p.ex_bin_count), read(&p.ex_underflow), read(&p.ex_overflow));
        let number = |t: &str| t.parse::<f64>().ok().filter(|v| v.is_finite());
        let bin_by = match (number(&width).filter(|w| *w > 0.0), count.parse::<u32>().ok().filter(|c| *c >= 1)) {
            (Some(w), _) => ibis_doc_model::BinBy::Width(Number(w)),
            (None, Some(c)) => ibis_doc_model::BinBy::Count(c),
            _ => ibis_doc_model::BinBy::Auto,
        };
        let (underflow, overflow) = (number(&under).map(Number), number(&over).map(Number));
        self.set_chart_ex_options(
            move |o| {
                o.bin_by = bin_by;
                o.underflow = underflow;
                o.overflow = overflow;
            },
            cx,
        );
    }

    /// ChartEx: apply gap width (%) setting.
    pub(crate) fn apply_chart_ex_gap_width(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb().chart_edit.as_ref() else { return };
        let text = p.ex_gap_width.read(cx).value().trim().to_string();
        let gap = text.parse::<u32>().ok();
        self.set_chart_ex_options(
            move |o| {
                o.gap_width = gap;
            },
            cx,
        );
    }

    /// データの範囲 / 行/列の切り替え: the selected chart's series over the range
    /// in the field (when `switch` is false) or over the range it has with its
    /// series turned from columns to rows or back (`switch`).
    pub(crate) fn set_sheet_chart_data(&mut self, switch: bool, window: &mut Window, cx: &mut Context<Self>) {
        let fail = |this: &mut Self, text: &str, cx: &mut Context<Self>| {
            this.show_panel_message(crate::shell::i18n::text(text),cx);
        };
        let Some((shape, chart)) = self.sheet_chart().map(|(id, c)| (id, c.clone())) else { return };
        let Some(source) = self.source.clone() else { return };
        let Some(current) = source.sheet_chart_refs(shape, &chart).and_then(|refs| ibis_pptx::refs_range(&refs)) else {
            return fail(self, "_xlsx.the-data-range-of-this-chart-cannot-be-changed", cx);
        };
        let text = self.wb().chart_edit.as_ref().map(|p| p.range.read(cx).value().trim().trim_start_matches('=').to_string()).unwrap_or_default();
        let (sheet, range) = if switch && text.is_empty() {
            (current.0.clone(), current.1)
        } else {
            let sheet_name = self.slide().name.clone().unwrap_or_default();
            let typed = if text.contains('!') { text.clone() } else { format!("{}!{text}", ibis_pptx::quote_sheet(&sheet_name)) };
            let Some((sheet, range)) = ibis_pptx::parse_ref(&typed) else {
                return fail(self, "_xlsx.cannot-read-the-data-range-example-sheet1-a-3-d", cx);
            };
            (sheet, range)
        };
        let changed = (sheet.as_str(), range) != (current.0.as_str(), current.1);
        // A new range: series in columns when it has no more columns than rows (as Excel does); else the chart's own.
        let is_xy_or_stock = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Stock { .. } | ibis_doc_model::ChartType::Scatter { .. } | ibis_doc_model::ChartType::Bubble { .. }));
        let by_columns = if switch { !current.2 } else if changed {
            // X-Y and stock charts read their own layout: in columns unless the range is wider than tall.
            if is_xy_or_stock { range.last_row - range.first_row >= range.last_column - range.first_column } else { ibis_pptx::preferred_series_columns(self.presentation(),&sheet,range) }
        } else { current.2 };
        let is_scatter = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Scatter { .. }));
        let is_bubble = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Bubble { .. }));
        let stock_kind = if chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Stock { .. })) {
            let total_series: usize = chart.groups.iter().map(|g| g.series.len()).sum();
            let has_bar = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Bar { .. }));
            if has_bar {
                if total_series <= 4 { Some(ibis_pptx::NewChartKind::StockVHLC) } else { Some(ibis_pptx::NewChartKind::StockVOHLC) }
            } else if total_series <= 3 {
                Some(ibis_pptx::NewChartKind::StockHLC)
            } else {
                Some(ibis_pptx::NewChartKind::StockOHLC)
            }
        } else {
            None
        };
        let get_refs = |by_cols: bool| -> Option<Vec<ibis_pptx::SeriesRefs>> {
            if let Some(kind) = stock_kind {
                ibis_pptx::stock_refs_for_range(self.presentation(), &sheet, range, kind, by_cols)
            } else if is_bubble {
                ibis_pptx::bubble_refs_for_range(self.presentation(), &sheet, range, by_cols)
            } else if is_scatter {
                ibis_pptx::scatter_refs_for_range(self.presentation(), &sheet, range, by_cols)
            } else {
                ibis_pptx::refs_for_range(self.presentation(), &sheet, range, by_cols)
            }
        };
        let refs = if switch {
            get_refs(by_columns)
        } else {
            get_refs(by_columns).or_else(|| if changed { get_refs(!by_columns) } else { None })
        };
        let Some(refs) = refs else {
            return fail(self, "_xlsx.use-a-range-of-at-least-2-rows-and-2-columns-who", cx);
        };
        // A new layout or range: a series no name cell names takes its default name again, not the one of the series it replaces.
        let mut renamed = chart.clone();
        if switch || changed {
            for (series, r) in renamed.groups.iter_mut().flat_map(|g| g.series.iter_mut()).zip(&refs) {
                if r.name.is_none() { series.name.clear(); }
            }
        }
        let target = ibis_pptx::sheet_chart_from_data(&renamed, self.presentation(), &refs);
        if target == chart {
            return cx.notify();
        }
        source.set_chart_data(shape, target.clone(), ibis_pptx::ChartData { workbook: Vec::new(), series: refs });
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(target))) }]) {
            log::warn!("chart data edit rejected: {err}");
        }
        let input = self.wb().chart_edit.as_ref().map(|p| p.range.clone());
        if let Some(input) = input {
            let shown = ibis_pptx::format_ref(&sheet, range);
            input.update(cx, |i, cx| i.set_value(shown, window, cx));
        }
        if let Some(p) = self.wb_mut().chart_edit.as_mut() {
            p.error = None;
        }
        cx.notify();
    }

    /// 行/列の切り替え: 選択中グラフのデータ系列を行と列で入れ替える.
    pub(crate) fn switch_sheet_chart_row_column(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_sheet_chart_data(true, window, cx);
    }

    /// 系列の並べ替え: 指定インデックスの系列を 1 つ上（up=true）または下（up=false）に移動する.
    pub(crate) fn move_sheet_chart_series(&mut self, series_idx: usize, up: bool, _window: &mut Window, cx: &mut Context<Self>) {
        let fail = |this: &mut Self, text: &str, cx: &mut Context<Self>| {
            if let Some(p) = this.wb_mut().chart_edit.as_mut() {
                p.error = Some(crate::shell::i18n::text(text));
            }
            cx.notify();
        };
        let Some((shape, mut chart)) = self.sheet_chart().map(|(id, c)| (id, c.clone())) else { return };
        let Some(source) = self.source.clone() else { return };
        let Some(mut refs) = source.sheet_chart_refs(shape, &chart) else {
            return fail(self, "xlsx.chart-series-reorder-error", cx);
        };
        let total_series: usize = chart.groups.iter().map(|g| g.series.len()).sum();
        if series_idx >= total_series || series_idx >= refs.len() {
            return;
        }
        let target_idx = if up {
            if series_idx == 0 {
                return;
            }
            series_idx - 1
        } else {
            if series_idx + 1 >= total_series || series_idx + 1 >= refs.len() {
                return;
            }
            series_idx + 1
        };

        refs.swap(series_idx, target_idx);

        let group_lens: Vec<usize> = chart.groups.iter().map(|g| g.series.len()).collect();
        let mut all_series: Vec<ibis_doc_model::Series> = chart.groups.iter_mut().flat_map(|g| g.series.drain(..)).collect();
        all_series.swap(series_idx, target_idx);
        let mut iter = all_series.into_iter();
        for (g, len) in chart.groups.iter_mut().zip(group_lens) {
            g.series.extend(iter.by_ref().take(len));
        }

        let target = ibis_pptx::sheet_chart_from_data(&chart, self.presentation(), &refs);
        source.set_chart_data(shape, target.clone(), ibis_pptx::ChartData { workbook: Vec::new(), series: refs });
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(target))) }]) {
            log::warn!("chart series move rejected: {err}");
        }
        if let Some(p) = self.wb_mut().chart_edit.as_mut() {
            p.error = None;
        }
        cx.notify();
    }

    /// 系列の削除: 指定インデックスの系列を削除する.
    pub(crate) fn delete_sheet_chart_series(&mut self, series_idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        let fail = |this: &mut Self, text: &str, cx: &mut Context<Self>| {
            if let Some(p) = this.wb_mut().chart_edit.as_mut() {
                p.error = Some(crate::shell::i18n::text(text));
            }
            cx.notify();
        };
        let Some((shape, mut chart)) = self.sheet_chart().map(|(id, c)| (id, c.clone())) else { return };
        let total_series: usize = chart.groups.iter().map(|g| g.series.len()).sum();
        if total_series <= 1 {
            return fail(self, "xlsx.chart-series-last-error", cx);
        }
        let Some(source) = self.source.clone() else { return };
        let Some(mut refs) = source.sheet_chart_refs(shape, &chart) else {
            return fail(self, "xlsx.chart-series-delete-error", cx);
        };
        if series_idx >= refs.len() || series_idx >= total_series {
            return;
        }

        refs.remove(series_idx);

        let mut curr = 0;
        for g in &mut chart.groups {
            if series_idx < curr + g.series.len() {
                g.series.remove(series_idx - curr);
                break;
            }
            curr += g.series.len();
        }

        chart.groups.retain(|g|!g.series.is_empty());
        let target = ibis_pptx::sheet_chart_from_data(&chart, self.presentation(), &refs);
        source.set_chart_data(shape, target.clone(), ibis_pptx::ChartData { workbook: Vec::new(), series: refs.clone() });
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(target))) }]) {
            log::warn!("chart series delete rejected: {err}");
        }
        if let Some(input) = self.wb().chart_edit.as_ref().map(|p| p.range.clone()) {
            if let Some((sheet, range, _)) = ibis_pptx::refs_range(&refs) {
                let shown = ibis_pptx::format_ref(&sheet, range);
                input.update(cx, |i, cx| i.set_value(shown, window, cx));
            }
        }
        if let Some(p) = self.wb_mut().chart_edit.as_mut() {
            p.error = None;
        }
        cx.notify();
    }

    /// 系列の追加: 次の列/行のデータを系列として追加する.
    pub(crate) fn add_sheet_chart_series(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fail = |this: &mut Self, text: &str, cx: &mut Context<Self>| {
            if let Some(p) = this.wb_mut().chart_edit.as_mut() {
                p.error = Some(crate::shell::i18n::text(text));
            }
            cx.notify();
        };
        let Some((shape, chart)) = self.sheet_chart().map(|(id, c)| (id, c.clone())) else { return };
        let Some(source) = self.source.clone() else { return };
        let Some(mut refs) = source.sheet_chart_refs(shape, &chart) else {
            return fail(self, "xlsx.chart-series-add-error", cx);
        };
        let is_bubble = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Bubble { .. }));
        let new_ref = if let Some((sheet, range, by_columns)) = ibis_pptx::refs_range(&refs) {
            let one = |r0: u32, c0: u32, r1: u32, c1: u32| ibis_pptx::format_ref(&sheet, CellRange { first_row: r0, first_column: c0, last_row: r1, last_column: c1 });
            let has_header = refs.first().and_then(|r| r.name.as_ref()).is_some();
            if is_bubble {
                if by_columns {
                    let next_col = range.last_column + 1;
                    let size_col = range.last_column + 2;
                    let (data_r0, name) = if has_header {
                        (range.first_row + 1, Some(one(range.first_row, next_col, range.first_row, next_col)))
                    } else {
                        (range.first_row, None)
                    };
                    ibis_pptx::SeriesRefs {
                        name,
                        categories: refs.first().and_then(|r| r.categories.clone()),
                        values: Some(one(data_r0, next_col, range.last_row, next_col)),
                        sizes: Some(one(data_r0, size_col, range.last_row, size_col)),
                    }
                } else {
                    let next_row = range.last_row + 1;
                    let size_row = range.last_row + 2;
                    let (data_c0, name) = if has_header {
                        (range.first_column + 1, Some(one(next_row, range.first_column, next_row, range.first_column)))
                    } else {
                        (range.first_column, None)
                    };
                    ibis_pptx::SeriesRefs {
                        name,
                        categories: refs.first().and_then(|r| r.categories.clone()),
                        values: Some(one(next_row, data_c0, next_row, range.last_column)),
                        sizes: Some(one(size_row, data_c0, size_row, range.last_column)),
                    }
                }
            } else if by_columns {
                let next_col = range.last_column + 1;
                let (data_r0, name) = if has_header {
                    (range.first_row + 1, Some(one(range.first_row, next_col, range.first_row, next_col)))
                } else {
                    (range.first_row, None)
                };
                ibis_pptx::SeriesRefs {
                    name,
                    categories: refs.first().and_then(|r| r.categories.clone()),
                    values: Some(one(data_r0, next_col, range.last_row, next_col)),
                    ..Default::default()
                }
            } else {
                let next_row = range.last_row + 1;
                let (data_c0, name) = if has_header {
                    (range.first_column + 1, Some(one(next_row, range.first_column, next_row, range.first_column)))
                } else {
                    (range.first_column, None)
                };
                ibis_pptx::SeriesRefs {
                    name,
                    categories: refs.first().and_then(|r| r.categories.clone()),
                    values: Some(one(next_row, data_c0, next_row, range.last_column)),
                    ..Default::default()
                }
            }
        } else if let Some(last) = refs.last() {
            last.clone()
        } else {
            return fail(self, "xlsx.chart-series-add-error", cx);
        };
        refs.push(new_ref);
        let target = ibis_pptx::sheet_chart_from_data(&chart, self.presentation(), &refs);
        source.set_chart_data(shape, target.clone(), ibis_pptx::ChartData { workbook: Vec::new(), series: refs.clone() });
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(target))) }]) {
            log::warn!("chart series add rejected: {err}");
        }
        if let Some(input) = self.wb().chart_edit.as_ref().map(|p| p.range.clone()) {
            if let Some((sheet, range, _)) = ibis_pptx::refs_range(&refs) {
                let shown = ibis_pptx::format_ref(&sheet, range);
                input.update(cx, |i, cx| i.set_value(shown, window, cx));
            }
        }
        if let Some(p) = self.wb_mut().chart_edit.as_mut() {
            p.error = None;
        }
        cx.notify();
    }

    /// The title shown (with the field's text, or グラフ タイトル) or not.
    pub(crate) fn set_sheet_chart_title_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        let text = self.wb().chart_edit.as_ref().map(|p| p.title.read(cx).value().to_string()).unwrap_or_default();
        self.change_sheet_chart(
            |c| {
                c.title = shown.then(|| ChartTitle {
                    text: if text.trim().is_empty() { crate::shell::i18n::text("_xlsx.chart-title").into() } else { text.clone() },
                    style: c.title.as_ref().map_or(ChartText { size: 1400, ..ChartText::default() }, |t| t.style.clone()),
                    at: None,
                })
            },
            cx,
        );
    }

    /// クイック レイアウト: Excel のグラフのクイック レイアウト 1..=10 を適用する.
    pub(crate) fn set_sheet_chart_quick_layout(&mut self, layout: u32, cx: &mut Context<Self>) {
        if !(1..=10).contains(&layout) {
            return;
        }
        self.change_sheet_chart(
            |c| {
                let default_title_text = crate::shell::i18n::text("_xlsx.chart-title").to_string();
                let default_axis_text = crate::shell::i18n::text("_xlsx.axis-title").to_string();

                // All 10 quick layouts show the Chart Title at top.
                c.title = Some(ChartTitle {
                    text: c.title.as_ref().map(|t| t.text.clone()).filter(|s| !s.trim().is_empty()).unwrap_or(default_title_text),
                    style: c.title.as_ref().map_or(ChartText { size: 1400, ..ChartText::default() }, |t| t.style.clone()),
                    at: None,
                });

                // Legend configuration:
                // Layout 1, 3, 6, 8, 9, 10: Right
                // Layout 2, 7: Top
                // Layout 4, 5: None
                let legend_pos = match layout {
                    1 | 3 | 6 | 8 | 9 | 10 => Some(LegendPosition::Right),
                    2 | 7 => Some(LegendPosition::Top),
                    4 | 5 => None,
                    _ => None,
                };
                c.legend = legend_pos.map(|pos| {
                    let side = match pos {
                        LegendPosition::Bottom => Side::Bottom,
                        LegendPosition::Left => Side::Left,
                        LegendPosition::Top => Side::Top,
                        LegendPosition::Right | LegendPosition::TopRight => Side::Right,
                    };
                    match c.legend.take() {
                        Some(l) => Legend { source_position: Some(pos), side, layout: None, ..l },
                        None => Legend {
                            source_position: Some(pos),
                            side,
                            overlay: Some(false),
                            layout: None,
                            text: ChartText { size: 900, ..ChartText::default() },
                            hidden: Vec::new(),
                        },
                    }
                });

                // Category & Value Axis Titles:
                // Layout 3, 7, 9: Both Category and Value Axis Titles
                // Layout 5: Value Axis Title only
                // Other layouts: None
                let (has_cat_title, has_val_title) = match layout {
                    3 | 7 | 9 => (true, true),
                    5 => (false, true),
                    _ => (false, false),
                };

                if let Some(i) = category_axis(c) {
                    let a = &mut c.axes[i];
                    a.title = if has_cat_title {
                        Some(ChartTitle {
                            text: a.title.as_ref().map(|t| t.text.clone()).filter(|s| !s.trim().is_empty()).unwrap_or_else(|| default_axis_text.clone()),
                            style: a.title.as_ref().map_or(ChartText { size: 1000, ..a.text.clone() }, |t| t.style.clone()),
                            at: None,
                        })
                    } else {
                        None
                    };
                }

                if let Some(i) = value_axis(c) {
                    let a = &mut c.axes[i];
                    a.title = if has_val_title {
                        Some(ChartTitle {
                            text: a.title.as_ref().map(|t| t.text.clone()).filter(|s| !s.trim().is_empty()).unwrap_or_else(|| default_axis_text.clone()),
                            style: a.title.as_ref().map_or(ChartText { size: 1000, ..a.text.clone() }, |t| t.style.clone()),
                            at: None,
                        })
                    } else {
                        None
                    };
                }

                // Gridlines:
                // Layout 2: None
                // Layout 1, 3..=10: ON
                let gridlines_on = layout != 2;
                if let Some(i) = value_axis(c) {
                    let a = &mut c.axes[i];
                    a.gridlines = if gridlines_on {
                        a.gridlines.clone().or_else(|| Some(Stroke::solid(Color::opaque(Rgb(0xD9D9D9)), Stroke::DEFAULT_WIDTH)))
                    } else {
                        None
                    };
                }

                // Data Labels:
                // Layout 2: OutsideEnd
                // Layout 6: Center
                // Others: None
                let label_pos = match layout {
                    2 => Some(LabelPosition::OutsideEnd),
                    6 => Some(LabelPosition::Center),
                    _ => None,
                };
                for series in c.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                    series.labels = match (label_pos, series.labels.take()) {
                        (Some(pos), Some(l)) => Some(DataLabels { position: Some(pos), show_value: true, ..l }),
                        (Some(pos), None) => Some(DataLabels { show_value: true, position: Some(pos), ..Default::default() }),
                        (None, _) => None,
                    };
                }

                // Data Table:
                // Layout 4: ON, show_keys: true
                // Layout 5: ON, show_keys: false
                // Others: None
                c.data_table = match layout {
                    4 => Some(ChartDataTable {
                        show_keys: true,
                        ..c.data_table.unwrap_or_default()
                    }),
                    5 => Some(ChartDataTable {
                        show_keys: false,
                        ..c.data_table.unwrap_or_default()
                    }),
                    _ => None,
                };

                // Trendline:
                // Layout 8: Linear (disp_eq: false)
                // Layout 9: Linear (disp_eq: true)
                // Others: None
                let trendline_cfg = match layout {
                    8 => Some((TrendlineType::Linear, false)),
                    9 => Some((TrendlineType::Linear, true)),
                    _ => None,
                };
                for series in c.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                    series.trendline = trendline_cfg.map(|(kind, disp_eq)| {
                        let mut tl = series.trendline.clone().unwrap_or_default();
                        tl.kind = kind;
                        tl.disp_eq = disp_eq;
                        tl
                    });
                }

                // Error Bars:
                // Layout 10: ON (default)
                // Others: None
                let error_bars_on = layout == 10;
                for series in c.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                    series.error_bars = if error_bars_on {
                        Some(series.error_bars.clone().unwrap_or_default())
                    } else {
                        None
                    };
                }
            },
            cx,
        );
    }

    /// The legend at `place` (`None`: no legend).
    pub(crate) fn set_sheet_chart_legend(&mut self, place: Option<LegendPosition>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                c.legend = place.map(|p| {
                    let side = match p {
                        LegendPosition::Bottom => Side::Bottom,
                        LegendPosition::Left => Side::Left,
                        LegendPosition::Top => Side::Top,
                        LegendPosition::Right | LegendPosition::TopRight => Side::Right,
                    };
                    match c.legend.take() {
                        Some(l) => Legend { source_position: Some(p), side, layout: None, ..l },
                        None => Legend { source_position: Some(p), side, overlay: Some(false), layout: None, text: ChartText { size: 900, ..ChartText::default() }, hidden: Vec::new() },
                    }
                })
            },
            cx,
        );
    }

    /// データラベル: 位置や有無を設定.
    pub(crate) fn set_sheet_chart_labels_position(&mut self, position: Option<Option<LabelPosition>>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                    series.labels = match (position, series.labels.take()) {
                        (None, _) => None,
                        (Some(pos), Some(l)) => Some(ibis_doc_model::DataLabels { position: pos, ..l }),
                        (Some(pos), None) => Some(ibis_doc_model::DataLabels { show_value: true, position: pos, ..Default::default() }),
                    };
                }
            },
            cx,
        );
    }

    /// 横（項目）軸のタイトルを表示（または非表示）.
    pub(crate) fn set_sheet_chart_cat_axis_title_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        let text = self.wb().chart_edit.as_ref().map(|p| p.cat_title.read(cx).value().to_string()).unwrap_or_default();
        self.change_sheet_chart(
            |c| {
                if let Some(i) = category_axis(c) {
                    let a = &mut c.axes[i];
                    a.title = shown.then(|| ChartTitle {
                        text: if text.trim().is_empty() { crate::shell::i18n::text("_xlsx.axis-title").into() } else { text.clone() },
                        style: a.title.as_ref().map_or(ChartText { size: 1000, ..a.text.clone() }, |t| t.style.clone()),
                        at: None,
                    });
                }
            },
            cx,
        );
    }

    /// 縦（値）軸のタイトルを表示（または非表示）.
    pub(crate) fn set_sheet_chart_val_axis_title_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        let text = self.wb().chart_edit.as_ref().map(|p| p.val_title.read(cx).value().to_string()).unwrap_or_default();
        self.change_sheet_chart(
            |c| {
                if let Some(i) = value_axis(c) {
                    let a = &mut c.axes[i];
                    a.title = shown.then(|| ChartTitle {
                        text: if text.trim().is_empty() { crate::shell::i18n::text("_xlsx.axis-title").into() } else { text.clone() },
                        style: a.title.as_ref().map_or(ChartText { size: 1000, ..a.text.clone() }, |t| t.style.clone()),
                        at: None,
                    });
                }
            },
            cx,
        );
    }

    /// The value axis's major gridlines on or off.
    pub(crate) fn set_sheet_chart_gridlines(&mut self, on: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = value_axis(c) {
                    let a = &mut c.axes[i];
                    a.gridlines = if on { a.gridlines.clone().or(Some(Stroke::solid(Color::opaque(Rgb(0xD9D9D9)), Stroke::DEFAULT_WIDTH))) } else { None };
                }
            },
            cx,
        );
    }

    /// 横（項目）軸の主目盛線の表示・非表示を設定.
    pub(crate) fn set_sheet_chart_cat_gridlines(&mut self, on: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = category_axis(c) {
                    let a = &mut c.axes[i];
                    a.gridlines = if on { a.gridlines.clone().or(Some(Stroke::solid(Color::opaque(Rgb(0xD9D9D9)), Stroke::DEFAULT_WIDTH))) } else { None };
                }
            },
            cx,
        );
    }

    /// 縦（値）軸の補助目盛線の表示・非表示を設定.
    pub(crate) fn set_sheet_chart_val_minor_gridlines(&mut self, on: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = value_axis(c) {
                    let a = &mut c.axes[i];
                    a.minor_gridlines = if on { a.minor_gridlines.clone().or(Some(Stroke::solid(Color::opaque(Rgb(0xEBEBEB)), Stroke::DEFAULT_WIDTH * 3 / 4))) } else { None };
                }
            },
            cx,
        );
    }

    /// 縦（値）軸の表示単位を設定.
    pub(crate) fn set_sheet_chart_display_unit(&mut self, unit: Option<Number>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = value_axis(c) {
                    c.axes[i].display_unit = unit;
                }
            },
            cx,
        );
    }

    /// 横（項目）軸の反転（逆順）を設定.
    pub(crate) fn set_sheet_chart_cat_axis_reversed(&mut self, reversed: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = category_axis(c) {
                    c.axes[i].reversed = reversed;
                }
            },
            cx,
        );
    }

    /// 横（項目）軸の種類（テキスト軸または日付軸）を設定.
    pub(crate) fn set_sheet_chart_cat_axis_date(&mut self, is_date: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = category_axis(c) {
                    c.axes[i].date = is_date;
                    c.axes[i].source_kind = if is_date {
                        ibis_doc_model::chart::AxisSourceKind::Date
                    } else {
                        ibis_doc_model::chart::AxisSourceKind::Category
                    };
                }
            },
            cx,
        );
    }

    /// 縦（値）軸の反転（逆順）を設定.
    pub(crate) fn set_sheet_chart_val_axis_reversed(&mut self, reversed: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = value_axis(c) {
                    c.axes[i].reversed = reversed;
                }
            },
            cx,
        );
    }

    /// 縦（値）軸の対数目盛（底）を設定 (None: 線形目盛, Some(Number(10.0)): 対数目盛).
    pub(crate) fn set_sheet_chart_val_axis_log_scale(&mut self, log_base: Option<Number>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = value_axis(c) {
                    c.axes[i].log_base = log_base;
                }
            },
            cx,
        );
    }

    /// 第2（値）軸のタイトルを表示（または非表示）.
    pub(crate) fn set_sheet_chart_sec_val_axis_title_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        let text = self.wb().chart_edit.as_ref().map(|p| p.sec_val_title.read(cx).value().to_string()).unwrap_or_default();
        self.change_sheet_chart(
            |c| {
                if let Some(i) = secondary_value_axis(c) {
                    let a = &mut c.axes[i];
                    a.title = shown.then(|| ChartTitle {
                        text: if text.trim().is_empty() { crate::shell::i18n::text("_xlsx.axis-title").into() } else { text.clone() },
                        style: a.title.as_ref().map_or(ChartText { size: 1000, ..a.text.clone() }, |t| t.style.clone()),
                        at: None,
                    });
                }
            },
            cx,
        );
    }

    /// 第2（値）軸の主目盛線の表示・非表示を設定.
    pub(crate) fn set_sheet_chart_sec_val_axis_gridlines(&mut self, on: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = secondary_value_axis(c) {
                    let a = &mut c.axes[i];
                    a.gridlines = if on { a.gridlines.clone().or(Some(Stroke::solid(Color::opaque(Rgb(0xD9D9D9)), Stroke::DEFAULT_WIDTH))) } else { None };
                }
            },
            cx,
        );
    }

    /// 第2（値）軸の補助目盛線の表示・非表示を設定.
    pub(crate) fn set_sheet_chart_sec_val_axis_minor_gridlines(&mut self, on: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = secondary_value_axis(c) {
                    let a = &mut c.axes[i];
                    a.minor_gridlines = if on { a.minor_gridlines.clone().or(Some(Stroke::solid(Color::opaque(Rgb(0xEBEBEB)), Stroke::DEFAULT_WIDTH * 3 / 4))) } else { None };
                }
            },
            cx,
        );
    }

    /// 第2（値）軸の表示単位を設定.
    pub(crate) fn set_sheet_chart_sec_val_axis_display_unit(&mut self, unit: Option<Number>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = secondary_value_axis(c) {
                    c.axes[i].display_unit = unit;
                }
            },
            cx,
        );
    }

    /// 第2（値）軸の反転（逆順）を設定.
    pub(crate) fn set_sheet_chart_sec_val_axis_reversed(&mut self, reversed: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = secondary_value_axis(c) {
                    c.axes[i].reversed = reversed;
                }
            },
            cx,
        );
    }

    /// 第2（値）軸の対数目盛（底）を設定.
    pub(crate) fn set_sheet_chart_sec_val_axis_log_scale(&mut self, log_base: Option<Number>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = secondary_value_axis(c) {
                    c.axes[i].log_base = log_base;
                }
            },
            cx,
        );
    }

    /// 系列軸のタイトルを表示（または非表示）.
    pub(crate) fn set_sheet_chart_ser_axis_title_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        let text = self.wb().chart_edit.as_ref().map(|p| p.ser_title.read(cx).value().to_string()).unwrap_or_default();
        self.change_sheet_chart(
            |c| {
                if let Some(i) = series_axis(c) {
                    let a = &mut c.axes[i];
                    a.title = shown.then(|| ChartTitle {
                        text: if text.trim().is_empty() { crate::shell::i18n::text("_xlsx.axis-title").into() } else { text.clone() },
                        style: a.title.as_ref().map_or(ChartText { size: 1000, ..a.text.clone() }, |t| t.style.clone()),
                        at: None,
                    });
                }
            },
            cx,
        );
    }

    /// 系列軸の目盛線の表示・非表示を設定.
    pub(crate) fn set_sheet_chart_ser_axis_gridlines(&mut self, on: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = series_axis(c) {
                    let a = &mut c.axes[i];
                    a.gridlines = if on { a.gridlines.clone().or(Some(Stroke::solid(Color::opaque(Rgb(0xD9D9D9)), Stroke::DEFAULT_WIDTH))) } else { None };
                }
            },
            cx,
        );
    }

    /// 系列軸の反転（逆順）を設定.
    pub(crate) fn set_sheet_chart_ser_axis_reversed(&mut self, reversed: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                if let Some(i) = series_axis(c) {
                    c.axes[i].reversed = reversed;
                }
            },
            cx,
        );
    }

    /// 空白セルのプロット方法（dispBlanksAs）を設定.
    pub(crate) fn set_sheet_chart_disp_blanks_as(&mut self, disp: Option<DispBlanksAs>, cx: &mut Context<Self>) {
        self.change_sheet_chart(|c| c.disp_blanks_as = disp, cx);
    }

    /// 非表示セルをプロットするか（plotVisOnly）を設定.
    pub(crate) fn set_sheet_chart_plot_vis_only(&mut self, plot_vis_only: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(|c| c.plot_vis_only = plot_vis_only, cx);
    }

    /// データテーブル（dTable）の表示/非表示を設定.
    pub(crate) fn set_sheet_chart_data_table_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(|c| {
            if shown {
                if c.data_table.is_none() {
                    c.data_table = Some(ibis_doc_model::ChartDataTable::default());
                }
            } else {
                c.data_table = None;
            }
        }, cx);
    }

    /// データテーブルの凡例キー表示を設定.
    pub(crate) fn set_sheet_chart_data_table_show_keys(&mut self, show_keys: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(|c| {
            if let Some(t) = c.data_table.as_mut() {
                t.show_keys = show_keys;
            }
        }, cx);
    }

    /// データテーブルの水平罫線表示を設定.
    pub(crate) fn set_sheet_chart_data_table_show_horz_border(&mut self, show: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(|c| {
            if let Some(t) = c.data_table.as_mut() {
                t.show_horz_border = show;
            }
        }, cx);
    }

    /// データテーブルの垂直罫線表示を設定.
    pub(crate) fn set_sheet_chart_data_table_show_vert_border(&mut self, show: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(|c| {
            if let Some(t) = c.data_table.as_mut() {
                t.show_vert_border = show;
            }
        }, cx);
    }

    /// データテーブルの外枠罫線表示を設定.
    pub(crate) fn set_sheet_chart_data_table_show_outline(&mut self, show: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(|c| {
            if let Some(t) = c.data_table.as_mut() {
                t.show_outline = show;
            }
        }, cx);
    }

    /// 近似曲線の種類を設定（全系列）.
    pub(crate) fn set_sheet_chart_trendline_type(&mut self, kind: Option<TrendlineType>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().filter(|g|g.kind.supports_series_analysis()).flat_map(|g| g.series.iter_mut()) {
                    match kind {
                        None => series.trendline = None,
                        Some(k) => {
                            let mut tl = series.trendline.clone().unwrap_or_default();
                            tl.kind = k;
                            series.trendline = Some(tl);
                        }
                    }
                }
            },
            cx,
        );
    }

    /// 近似曲線の数式表示を設定.
    pub(crate) fn set_sheet_chart_trendline_disp_eq(&mut self, disp_eq: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().filter(|g|g.kind.supports_series_analysis()).flat_map(|g| g.series.iter_mut()) {
                    if let Some(tl) = series.trendline.as_mut() {
                        tl.disp_eq = disp_eq;
                    }
                }
            },
            cx,
        );
    }

    /// 近似曲線のR-2乗値表示を設定.
    pub(crate) fn set_sheet_chart_trendline_disp_r_sqr(&mut self, disp_r_sqr: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().filter(|g|g.kind.supports_series_analysis()).flat_map(|g| g.series.iter_mut()) {
                    if let Some(tl) = series.trendline.as_mut() {
                        tl.disp_r_sqr = disp_r_sqr;
                    }
                }
            },
            cx,
        );
    }

    /// 近似曲線の有無をトグル（メニュー用）.
    pub(crate) fn toggle_sheet_chart_trendline(&mut self, cx: &mut Context<Self>) {
        let has_any = self.sheet_chart().map_or(false, |(_, c)| {
            c.groups.iter().flat_map(|g| &g.series).any(|s| s.trendline.is_some())
        });
        self.set_sheet_chart_trendline_type(if has_any { None } else { Some(TrendlineType::Linear) }, cx);
    }

    /// 誤差範囲の表示/非表示を設定（全系列）.
    pub(crate) fn set_sheet_chart_error_bars_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().filter(|g|g.kind.supports_series_analysis()).flat_map(|g| g.series.iter_mut()) {
                    if shown {
                        if series.error_bars.is_none() {
                            series.error_bars = Some(ibis_doc_model::ErrorBars::default());
                        }
                    } else {
                        series.error_bars = None;
                    }
                }
            },
            cx,
        );
    }

    /// 誤差範囲の表示方向（Both / Plus / Minus）を設定.
    pub(crate) fn set_sheet_chart_error_bar_type(&mut self, bar_type: ErrorBarType, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().filter(|g|g.kind.supports_series_analysis()).flat_map(|g| g.series.iter_mut()) {
                    let mut eb = series.error_bars.clone().unwrap_or_default();
                    eb.bar_type = bar_type;
                    series.error_bars = Some(eb);
                }
            },
            cx,
        );
    }

    /// 誤差範囲の計算方法（Fixed / Percentage / StdDev / StdErr）を設定.
    pub(crate) fn set_sheet_chart_error_bar_val_type(&mut self, val_type: ErrorBarValueType, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().filter(|g|g.kind.supports_series_analysis()).flat_map(|g| g.series.iter_mut()) {
                    let mut eb = series.error_bars.clone().unwrap_or_default();
                    eb.val_type = val_type;
                    series.error_bars = Some(eb);
                }
            },
            cx,
        );
    }

    /// 誤差範囲の末端キャップ表示を設定.
    pub(crate) fn set_sheet_chart_error_bar_end_cap(&mut self, end_cap: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().filter(|g|g.kind.supports_series_analysis()).flat_map(|g| g.series.iter_mut()) {
                    if let Some(eb) = series.error_bars.as_mut() {
                        eb.no_end_cap = !end_cap;
                    }
                }
            },
            cx,
        );
    }

    /// 誤差範囲の有無をトグル（メニュー用）.
    pub(crate) fn toggle_sheet_chart_error_bars(&mut self, cx: &mut Context<Self>) {
        let has_any = self.sheet_chart().map_or(false, |(_, c)| {
            c.groups.iter().flat_map(|g| &g.series).any(|s| s.error_bars.is_some())
        });
        self.set_sheet_chart_error_bars_shown(!has_any, cx);
    }

    /// マーカーの記号を設定（全系列）.
    pub(crate) fn set_sheet_chart_marker_symbol(&mut self, symbol: MarkerSymbol, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().filter(|g|g.kind.supports_markers()).flat_map(|g| g.series.iter_mut()) {
                    if symbol == MarkerSymbol::None {
                        series.marker = None;
                    } else if let Some(m) = series.marker.as_mut() {
                        m.symbol = symbol;
                        if m.size == 0 {
                            m.size = 5;
                        }
                    } else {
                        let fill = match &series.fill {
                            Fill::Solid(color) => Fill::Solid(*color),
                            _ => Fill::Solid(Color::opaque(Rgb(0x4472C4))),
                        };
                        series.marker = Some(Marker {
                            source: None,
                            symbol,
                            size: 5,
                            fill,
                        });
                    }
                }
            },
            cx,
        );
    }

    /// 系列の線の太さを設定（全系列、None の場合は線なし）.
    pub(crate) fn set_sheet_chart_line_stroke(&mut self, width: Option<i64>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                    match width {
                        None => {
                            series.stroke = None;
                        }
                        Some(w) => {
                            if let Some(s) = series.stroke.as_mut() {
                                s.width = w;
                            } else {
                                let color = match &series.fill {
                                    Fill::Solid(color) => *color,
                                    _ => Color::opaque(Rgb(0x4472C4)),
                                };
                                series.stroke = Some(Stroke::solid(color, w));
                            }
                        }
                    }
                }
            },
            cx,
        );
    }

    /// 系列の塗りつぶし色を設定（全系列）.
    pub(crate) fn set_sheet_chart_series_fill(&mut self, fill: Fill, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                    series.fill = fill.clone();
                }
            },
            cx,
        );
    }

    /// 系列の枠線色を設定（全系列、None の場合は枠線なし）.
    pub(crate) fn set_sheet_chart_series_stroke_color(&mut self, rgb: Option<Rgb>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for series in c.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                    match rgb {
                        None => {
                            series.stroke = None;
                        }
                        Some(color) => {
                            let width = series.stroke.as_ref().map(|s| s.width).unwrap_or(28_575);
                            series.stroke = Some(Stroke::solid(Color::opaque(color), width));
                        }
                    }
                }
            },
            cx,
        );
    }

    pub(crate) fn set_sheet_chart_visual_style(&mut self,style:ChartStylePreset,cx:&mut Context<Self>) {
        self.change_sheet_chart(|chart|style.apply(chart),cx);
    }

    /// グラフの配色パレットを全系列に適用.
    pub(crate) fn set_sheet_chart_color_palette(&mut self, palette_index: usize, cx: &mut Context<Self>) {
        let Some((_, colors)) = COLOR_PALETTES.get(palette_index) else { return };
        let colors = *colors;
        self.change_sheet_chart(
            |c| {
                let mut i = 0;
                for series in c.groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
                    let hex = colors[i % colors.len()];
                    let color = Color::opaque(Rgb(hex));
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
            },
            cx,
        );
        if let Some(p)=self.wb_mut().chart_edit.as_mut(){p.palette_selected=Some(palette_index);}
    }

    /// グラフの種類の変更: the selected chart's one group made `preset` (one step to undo).
    pub(crate) fn set_sheet_chart_type(&mut self, preset: crate::pptx::chart_panel::ChartPreset, cx: &mut Context<Self>) {
        let Some((_, chart)) = self.sheet_chart() else { return };
        match crate::pptx::chart_panel::chart_with_preset(chart.clone(), preset) {
            Ok(new_chart) => {
                self.change_sheet_chart(|c| *c = new_chart, cx);
            }
            Err(key) => {
                self.set_status(crate::shell::i18n::text(key).to_string(), cx);
            }
        }
    }

    /// 軸の交点（自動または最大値等）を設定.
    #[allow(dead_code)]
    pub(crate) fn set_sheet_chart_axis_crosses(&mut self, is_value_axis: bool, crosses: Option<ibis_doc_model::chart::AxisCrosses>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                let idx = if is_value_axis { value_axis(c) } else { category_axis(c) };
                if let Some(i) = idx {
                    c.axes[i].crosses = crosses;
                    c.axes[i].crosses_at = None;
                }
            },
            cx,
        );
    }

    /// 棒グラフの要素の間隔（%）と系列の重なり（%）を設定.
    pub(crate) fn set_sheet_chart_bar_options(&mut self, gap_width: Option<u32>, overlap: Option<i32>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in c.groups.iter_mut() {
                    if let ibis_doc_model::ChartType::Bar { gap_width: g, overlap: o, .. } = &mut group.kind {
                        if let Some(val) = gap_width {
                            *g = val;
                        }
                        if let Some(val) = overlap {
                            *o = val;
                        }
                    }
                }
            },
            cx,
        );
    }

    /// レーダー チャートのスタイルを設定（標準 / マーカー / 塗りつぶし）.
    pub(crate) fn set_sheet_chart_radar_style(&mut self, style: Option<ibis_doc_model::RadarStyle>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in &mut c.groups {
                    if let ibis_doc_model::ChartType::Radar { style: ref mut s } = group.kind {
                        *s = style;
                    }
                }
            },
            cx,
        );
    }

    /// 補助円・補助棒グラフの種類を設定（補助円 / 補助棒）.
    pub(crate) fn set_sheet_chart_of_pie_kind(&mut self, kind: Option<ibis_doc_model::OfPieKind>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in &mut c.groups {
                    if let ibis_doc_model::ChartType::OfPie { kind: ref mut k } = group.kind {
                        *k = kind;
                    }
                }
            },
            cx,
        );
    }

    /// 補助円・補助棒グラフの分割の基準を設定（位置 / 値 / ％）.
    pub(crate) fn set_sheet_chart_of_pie_split(&mut self, split: Option<ibis_doc_model::OfPieSplit>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in &mut c.groups {
                    if let ibis_doc_model::ChartType::OfPie { .. } = group.kind {
                        let op = group.of_pie.get_or_insert_with(ibis_doc_model::OfPieSettings::default);
                        op.split = split;
                    }
                }
            },
            cx,
        );
    }

    /// 補助円・補助棒グラフの分割位置/値を設定.
    pub(crate) fn set_sheet_chart_of_pie_split_position(&mut self, pos: Option<ibis_doc_model::Number>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in &mut c.groups {
                    if let ibis_doc_model::ChartType::OfPie { .. } = group.kind {
                        let op = group.of_pie.get_or_insert_with(ibis_doc_model::OfPieSettings::default);
                        op.split_position = pos;
                    }
                }
            },
            cx,
        );
    }

    /// 補助円・補助棒グラフの第2プロットのサイズ（%）を設定.
    pub(crate) fn set_sheet_chart_of_pie_second_size(&mut self, size: Option<u16>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in &mut c.groups {
                    if let ibis_doc_model::ChartType::OfPie { .. } = group.kind {
                        let op = group.of_pie.get_or_insert_with(ibis_doc_model::OfPieSettings::default);
                        op.second_size = size;
                    }
                }
            },
            cx,
        );
    }

    /// 株価チャートの高値安値線の表示/非表示を設定.
    pub(crate) fn set_sheet_chart_stock_hi_low_lines(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in &mut c.groups {
                    if let ibis_doc_model::ChartType::Stock { hi_low_lines, .. } = &mut group.kind {
                        *hi_low_lines = enabled;
                    }
                }
            },
            cx,
        );
    }

    /// 株価チャートの陽線・陰線の表示/非表示を設定.
    pub(crate) fn set_sheet_chart_stock_up_down_bars(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in &mut c.groups {
                    if let ibis_doc_model::ChartType::Stock { up_down_bars, .. } = &mut group.kind {
                        *up_down_bars = enabled;
                    }
                }
            },
            cx,
        );
    }

    /// 株価チャートの降下線の表示/非表示を設定.
    pub(crate) fn set_sheet_chart_stock_drop_lines(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in &mut c.groups {
                    if let ibis_doc_model::ChartType::Stock { drop_lines, .. } = &mut group.kind {
                        *drop_lines = enabled;
                    }
                }
            },
            cx,
        );
    }

    /// 表面チャートの種別（3D/等高線、ワイヤーフレーム）を設定.
    pub(crate) fn set_sheet_chart_surface_type(&mut self, is_3d: bool, wireframe: bool, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for group in &mut c.groups {
                    if let ibis_doc_model::ChartType::Surface { is_3d: s_3d, wireframe: s_wf } = &mut group.kind {
                        *s_3d = is_3d;
                        *s_wf = wireframe;
                    }
                }
            },
            cx,
        );
    }

    /// 3-D 回転の設定 (X軸回転、Y軸回転、直角の軸、奥行き %)
    pub(crate) fn set_sheet_chart_3d_rotation(&mut self, rot_x: Option<i32>, rot_y: Option<i32>, right_angle_axes: Option<bool>, depth_percent: Option<u32>, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for g in &mut c.groups {
                    if let Some(v) = &mut g.three_d {
                        if let Some(rx) = rot_x { v.rot_x = rx; }
                        if let Some(ry) = rot_y { v.rot_y = ry; }
                        if let Some(raa) = right_angle_axes { v.right_angle_axes = raa; }
                        if let Some(dp) = depth_percent { v.depth_percent = dp.clamp(20, 2000); }
                    }
                }
            },
            cx,
        );
    }

    /// 3-D 回転を既定値にリセット
    pub(crate) fn reset_sheet_chart_3d_rotation(&mut self, cx: &mut Context<Self>) {
        self.change_sheet_chart(
            |c| {
                for g in &mut c.groups {
                    if g.three_d.is_some() {
                        let is_pie = matches!(g.kind, ibis_doc_model::ChartType::Pie { .. });
                        g.three_d = Some(if is_pie {
                            ibis_doc_model::View3D { rot_x: 30, rot_y: 0, right_angle_axes: false, depth_percent: 100, height_percent: None, series_in_depth: true, gap_depth: 150 }
                        } else {
                            ibis_doc_model::View3D::default()
                        });
                    }
                }
            },
            cx,
        );
    }

    /// 適用: the title's text and the axis's scale from the fields.
    pub(crate) fn apply_sheet_chart_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb().chart_edit.as_ref() else { return };
        let title = p.title.read(cx).value().to_string();
        let cat_title = p.cat_title.read(cx).value().to_string();
        let val_title = p.val_title.read(cx).value().to_string();
        let sec_val_title = p.sec_val_title.read(cx).value().to_string();
        let ser_title = p.ser_title.read(cx).value().to_string();
        let domain_fields=[p.domain_min.read(cx).value().to_string(),p.domain_max.read(cx).value().to_string(),p.domain_unit.read(cx).value().to_string()];
        let domain_scale=domain_fields.iter().map(|s|parse_field(s)).collect::<Result<Vec<_>,_>>();
        if domain_scale.as_ref().is_err()||domain_scale.as_ref().ok().is_some_and(|v|v[0].zip(v[1]).is_some_and(|(a,b)|a.0>=b.0)||v[2].is_some_and(|n|n.0<=0.)){
            if let Some(p)=self.wb_mut().chart_edit.as_mut(){p.error=Some(crate::shell::i18n::text("_xlsx.enter-numbers-for-the-minimum-maximum-and-major"));}cx.notify();return;
        }
        let domain_scale=domain_scale.unwrap();
        let fields = [p.min.read(cx).value().to_string(), p.max.read(cx).value().to_string(), p.unit.read(cx).value().to_string()];
        let parsed: Result<Vec<Option<Number>>, ()> = fields.iter().map(|f| parse_field(f)).collect();
        let gap_str = p.gap_width.read(cx).value().trim().to_string();
        let overlap_str = p.overlap.read(cx).value().trim().to_string();
        let parsed_gap: Result<Option<u32>, ()> = if gap_str.is_empty() {
            Ok(None)
        } else {
            gap_str.parse::<u32>().map_err(|_| ()).and_then(|v| if v <= 500 { Ok(Some(v)) } else { Err(()) })
        };
        let parsed_overlap: Result<Option<i32>, ()> = if overlap_str.is_empty() {
            Ok(None)
        } else {
            overlap_str.parse::<i32>().map_err(|_| ()).and_then(|v| if (-100..=100).contains(&v) { Ok(Some(v)) } else { Err(()) })
        };
        let of_pie_pos_str = p.of_pie_split_pos.read(cx).value().trim().to_string();
        let parsed_of_pie_pos = parse_field(&of_pie_pos_str);
        let of_pie_size_str = p.of_pie_second_size.read(cx).value().trim().to_string();
        let parsed_of_pie_size: Result<Option<u16>, ()> = if of_pie_size_str.is_empty() {
            Ok(None)
        } else {
            of_pie_size_str.parse::<u16>().map_err(|_| ()).and_then(|v| if (5..=200).contains(&v) { Ok(Some(v)) } else { Err(()) })
        };
        let rot_x_str = p.three_d_rot_x.read(cx).value().trim().to_string();
        let rot_y_str = p.three_d_rot_y.read(cx).value().trim().to_string();
        let depth_str = p.three_d_depth.read(cx).value().trim().to_string();
        let parsed_rot_x: Option<i32> = rot_x_str.parse::<i32>().ok().map(|v| v.clamp(-90, 90));
        let parsed_rot_y: Option<i32> = rot_y_str.parse::<i32>().ok().map(|v| v.clamp(0, 360));
        let parsed_depth: Option<u32> = depth_str.parse::<u32>().ok().map(|v| v.clamp(20, 2000));
        let error = match (&parsed, parsed_gap, parsed_overlap, &parsed_of_pie_pos, parsed_of_pie_size) {
            (Err(()), _, _, _, _) => Some(crate::shell::i18n::text("_xlsx.enter-numbers-for-the-minimum-maximum-and-major")),
            (Ok(v), _, _, _, _) if v[0].zip(v[1]).is_some_and(|(a, b)| a.0 >= b.0) => Some(crate::shell::i18n::text("_xlsx.the-minimum-must-be-less-than-the-maximum")),
            (Ok(v), _, _, _, _) if v[2].is_some_and(|u| u.0 <= 0.0) => Some(crate::shell::i18n::text("_xlsx.the-major-unit-must-be-greater-than-0")),
            (_, Err(()), _, _, _) => Some(crate::shell::i18n::text("_xlsx.enter-a-whole-number-from-0-to-500-for-the-eleme")),
            (_, _, Err(()), _, _) => Some(crate::shell::i18n::text("_xlsx.enter-an-integer-from-100-to-100-for-series-over")),
            (_, _, _, Err(()), _) => Some(crate::shell::i18n::text("fix.chart-edit.enter-number-split-position-value")),
            (_, _, _, _, Err(())) => Some(crate::shell::i18n::text("fix.chart-edit.enter-integer-between-5-200")),
            _ => None,
        };
        if let Some(e) = error {
            if let Some(p) = self.wb_mut().chart_edit.as_mut() {
                p.error = Some(e.into());
            }
            return cx.notify();
        }
        let v = parsed.unwrap_or_default();
        let new_gap = parsed_gap.unwrap_or_default();
        let new_overlap = parsed_overlap.unwrap_or_default();
        self.change_sheet_chart(
            |c| {
                if let Some(t) = c.title.as_mut()
                    && !title.trim().is_empty()
                {
                    t.text = title.clone();
                }
                if let Some(i) = category_axis(c) {
                    let a = &mut c.axes[i];
                    if a.kind==AxisKind::Value{(a.min,a.max,a.major_unit)=(domain_scale[0],domain_scale[1],domain_scale[2]);}
                    if let Some(t) = a.title.as_mut()
                        && !cat_title.trim().is_empty()
                    {
                        t.text = cat_title.clone();
                    }
                }
                if let Some(i) = value_axis(c) {
                    let a = &mut c.axes[i];
                    if let Some(t) = a.title.as_mut()
                        && !val_title.trim().is_empty()
                    {
                        t.text = val_title.clone();
                    }
                    (a.min, a.max, a.major_unit) = (v[0], v[1], v[2]);
                }
                if let Some(i) = secondary_value_axis(c) {
                    let a = &mut c.axes[i];
                    if let Some(t) = a.title.as_mut()
                        && !sec_val_title.trim().is_empty()
                    {
                        t.text = sec_val_title.clone();
                    }
                }
                if let Some(i) = series_axis(c) {
                    let a = &mut c.axes[i];
                    if let Some(t) = a.title.as_mut()
                        && !ser_title.trim().is_empty()
                    {
                        t.text = ser_title.clone();
                    }
                }
                if new_gap.is_some() || new_overlap.is_some() {
                    for group in c.groups.iter_mut() {
                        if let ibis_doc_model::ChartType::Bar { gap_width, overlap, .. } = &mut group.kind {
                            if let Some(g) = new_gap { *gap_width = g; }
                            if let Some(o) = new_overlap { *overlap = o; }
                        }
                    }
                }
                if let Ok(pos) = parsed_of_pie_pos {
                    for group in c.groups.iter_mut() {
                        if let ibis_doc_model::ChartType::OfPie { .. } = group.kind {
                            let op = group.of_pie.get_or_insert_with(ibis_doc_model::OfPieSettings::default);
                            op.split_position = pos;
                        }
                    }
                }
                if let Ok(sz) = parsed_of_pie_size {
                    for group in c.groups.iter_mut() {
                        if let ibis_doc_model::ChartType::OfPie { .. } = group.kind {
                            let op = group.of_pie.get_or_insert_with(ibis_doc_model::OfPieSettings::default);
                            op.second_size = sz;
                        }
                    }
                }
                for g in &mut c.groups {
                    if let Some(v) = &mut g.three_d {
                        if let Some(rx) = parsed_rot_x { v.rot_x = rx; }
                        if let Some(ry) = parsed_rot_y { v.rot_y = ry; }
                        if let Some(dp) = parsed_depth { v.depth_percent = dp; }
                    }
                }
            },
            cx,
        );
        if let Some(p) = self.wb_mut().chart_edit.as_mut() {
            p.error = None;
        }
    }

    pub(crate) fn sheet_chart_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel>{
        self.sheet_state()?.chart_edit.as_ref()?;
        self.wb_mut().chart_edit.as_mut()?.rendering_sidebar=true;
        let panel=self.render_sheet_chart_panel(cx);
        if let Some(p)=self.wb_mut().chart_edit.as_mut(){p.rendering_sidebar=false;}
        panel
    }

    #[inline(never)]
    fn render_sheet_chart_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.chart_edit.as_ref()?;
        let theme = cx.theme().clone();
        let Some((id, chart)) = self.sheet_chart() else {
            return Some(SidebarPanel { title: crate::shell::i18n::text("_xlsx.format-chart").into(), body: div().px(px(18.)).py(px(8.)).child(crate::shell::i18n::text("_xlsx.select-a-chart")).into_any_element() });
        };
        if id != p.shape {
            return Some(SidebarPanel { title: crate::shell::i18n::text("_xlsx.format-chart").into(), body: div().px(px(18.)).py(px(8.)).child(crate::shell::i18n::text("_xlsx.a-different-chart-is-selected-open-format-chart")).into_any_element() });
        }
        let titled = chart.title.is_some();
        let legend = chart.legend.as_ref().map(|l| l.source_position.unwrap_or(LegendPosition::Right));
        let has_data_table = chart.data_table.is_some();
        let dt = chart.data_table.unwrap_or_default();
        let dt_keys = dt.show_keys;
        let dt_horz = dt.show_horz_border;
        let dt_vert = dt.show_vert_border;
        let dt_outline = dt.show_outline;
        let cat_axis = category_axis(chart);
        let cat_titled = cat_axis.is_some_and(|i| chart.axes[i].title.is_some());
        let val_axis = value_axis(chart);
        let val_titled = val_axis.is_some_and(|i| chart.axes[i].title.is_some());
        let cat_reversed = cat_axis.is_some_and(|i| chart.axes[i].reversed);
        let cat_date = cat_axis.is_some_and(|i| chart.axes[i].date);
        let val_reversed = val_axis.is_some_and(|i| chart.axes[i].reversed);
        let cat_gridlines = cat_axis.is_some_and(|i| chart.axes[i].gridlines.is_some());
        let gridlines = val_axis.is_some_and(|i| chart.axes[i].gridlines.is_some());
        let val_minor_gridlines = val_axis.is_some_and(|i| chart.axes[i].minor_gridlines.is_some());
        let val_log_scale = val_axis.is_some_and(|i| chart.axes[i].log_base.is_some());
        let sec_val_axis = secondary_value_axis(chart);
        let sec_val_titled = sec_val_axis.is_some_and(|i| chart.axes[i].title.is_some());
        let sec_val_reversed = sec_val_axis.is_some_and(|i| chart.axes[i].reversed);
        let sec_val_gridlines = sec_val_axis.is_some_and(|i| chart.axes[i].gridlines.is_some());
        let sec_val_minor_gridlines = sec_val_axis.is_some_and(|i| chart.axes[i].minor_gridlines.is_some());
        let sec_val_log_scale = sec_val_axis.is_some_and(|i| chart.axes[i].log_base.is_some());
        let ser_axis = series_axis(chart);
        let ser_titled = ser_axis.is_some_and(|i| chart.axes[i].title.is_some());
        let ser_reversed = ser_axis.is_some_and(|i| chart.axes[i].reversed);
        let ser_gridlines = ser_axis.is_some_and(|i| chart.axes[i].gridlines.is_some());
        let has_bar = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Bar { .. }));
        let has_stock = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Stock { .. }));
        let (stock_hi_low, stock_up_down, stock_drop) = chart.groups.iter().find_map(|g| match g.kind {
            ibis_doc_model::ChartType::Stock { hi_low_lines, up_down_bars, drop_lines } => Some((hi_low_lines, up_down_bars, drop_lines)),
            _ => None,
        }).unwrap_or((false, false, false));
        let has_3d = chart.groups.iter().any(|g| g.three_d.is_some());
        let current_v3d = chart.groups.iter().find_map(|g| g.three_d);
        let right_angle_axes = current_v3d.map_or(false, |v| v.right_angle_axes);
        let has_of_pie = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::OfPie { .. }));
        let current_of_pie_kind = chart.groups.iter().find_map(|g| match g.kind {
            ibis_doc_model::ChartType::OfPie { kind } => Some(kind.unwrap_or(ibis_doc_model::OfPieKind::Pie)),
            _ => None,
        });
        let current_of_pie_split = chart.groups.iter().find_map(|g| match g.kind {
            ibis_doc_model::ChartType::OfPie { .. } => Some(g.of_pie.as_ref().and_then(|op| op.split).unwrap_or(ibis_doc_model::OfPieSplit::Position)),
            _ => None,
        });
        let mut places = SegmentControlRow::new(Mode::Proportional{width:px(264.)});
        for (i, (label, place)) in LEGEND_PLACES.iter().enumerate() {
            let place = *place;
            let on = legend == place || (place.is_none() && legend.is_none());
            places = places.child(SegmentItem::label(("chart-legend", i), crate::shell::i18n::text(label), on).tooltip(crate::shell::i18n::text(label)).on_click(cx.listener(move |this, _, _, cx| this.set_sheet_chart_legend(place, cx))));
        }
        let display_unit = val_axis.and_then(|i| chart.axes[i].display_unit.map(|u| u.0));
        let mut disp_unit_buttons = SegmentControlRow::new(Mode::Proportional{width:px(264.)});
        for (i, (label, unit)) in DISPLAY_UNITS.iter().enumerate() {
            let unit = *unit;
            let on = match (unit, display_unit) {
                (None, None) => true,
                (Some(u), Some(du)) => (u - du).abs() < 1e-6,
                _ => false,
            };
            disp_unit_buttons = disp_unit_buttons.child(
                SegmentItem::label(("chart-disp-unit", i), crate::shell::i18n::text(label), on).tooltip(crate::shell::i18n::text(label))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_sheet_chart_display_unit(unit.map(Number), cx))),
            );
        }
        let sec_display_unit = sec_val_axis.and_then(|i| chart.axes[i].display_unit.map(|u| u.0));
        let mut sec_disp_unit_buttons = SegmentControlRow::new(Mode::Proportional{width:px(264.)});
        for (i, (label, unit)) in DISPLAY_UNITS.iter().enumerate() {
            let unit = *unit;
            let on = match (unit, sec_display_unit) {
                (None, None) => true,
                (Some(u), Some(du)) => (u - du).abs() < 1e-6,
                _ => false,
            };
            sec_disp_unit_buttons = sec_disp_unit_buttons.child(
                SegmentItem::label(("chart-sec-disp-unit", i), crate::shell::i18n::text(label), on).tooltip(crate::shell::i18n::text(label))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_sheet_chart_sec_val_axis_display_unit(unit.map(Number), cx))),
            );
        }
        let current_blanks = chart.disp_blanks_as.unwrap_or(DispBlanksAs::Gap);
        let mut blank_buttons = SegmentControlRow::new(Mode::Proportional {width:px(264.)});
        for (i, (label, opt)) in BLANK_CELL_OPTIONS.iter().enumerate() {
            let opt = *opt;
            let on = opt == Some(current_blanks);
            blank_buttons = blank_buttons.child(
                SegmentItem::label(("chart-blanks", i),crate::shell::i18n::text(label),on)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_sheet_chart_disp_blanks_as(opt, cx))),
            );
        }
        let has_radar = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Radar { .. }));
        let current_radar_style = chart.groups.iter().find_map(|g| match g.kind {
            ibis_doc_model::ChartType::Radar { style } => Some(style.unwrap_or(ibis_doc_model::RadarStyle::Standard)),
            _ => None,
        });
        let mut radar_style_buttons = crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic);
        for (i, (label, style)) in RADAR_STYLES.iter().enumerate() {
            let style = *style;
            let on = current_radar_style == style;
            radar_style_buttons = radar_style_buttons.child(
                crate::shell::chrome::segment_control::SegmentItem::label(("chart-radar-style", i),crate::shell::i18n::text(label),on).disabled(false).on_click(cx.listener(move |this, _, _, cx| this.set_sheet_chart_radar_style(style, cx))),
            );
        }
        let has_surface = chart.groups.iter().any(|g| matches!(g.kind, ibis_doc_model::ChartType::Surface { .. }));
        let current_surface = chart.groups.iter().find_map(|g| match g.kind {
            ibis_doc_model::ChartType::Surface { is_3d, wireframe } => Some((is_3d, wireframe)),
            _ => None,
        });
        let mut surface_type_buttons = crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow);
        for (i, (label, is_3d, wireframe)) in SURFACE_TYPES.iter().enumerate() {
            let (is_3d, wireframe) = (*is_3d, *wireframe);
            let on = current_surface == Some((is_3d, wireframe));
            surface_type_buttons = surface_type_buttons.child(
                crate::shell::chrome::radio_button(("chart-surface-type", i),on,false,crate::shell::i18n::text(label),cx.listener(move |this, _, _, cx| this.set_sheet_chart_surface_type(is_3d, wireframe, cx))),
            );
        }
        let mut of_pie_kind_buttons = crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic);
        for (i, (label, kind)) in OF_PIE_KINDS.iter().enumerate() {
            let kind = *kind;
            let on = kind == current_of_pie_kind;
            of_pie_kind_buttons = of_pie_kind_buttons.child(
                crate::shell::chrome::segment_control::SegmentItem::label(("chart-of-pie-kind", i),crate::shell::i18n::text(label),on).disabled(false).on_click(cx.listener(move |this, _, _, cx| this.set_sheet_chart_of_pie_kind(kind, cx))),
            );
        }
        let mut of_pie_split_buttons = crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic);
        for (i, (label, split)) in OF_PIE_SPLITS.iter().enumerate() {
            let split = *split;
            let on = split == current_of_pie_split;
            of_pie_split_buttons = of_pie_split_buttons.child(
                crate::shell::chrome::segment_control::SegmentItem::label(("chart-of-pie-split", i),crate::shell::i18n::text(label),on).on_click(cx.listener(move |this, _, _, cx| this.set_sheet_chart_of_pie_split(split, cx))),
            );
        }
        let mut axis_type_buttons = crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic);
        for (i, (label, is_date)) in AXIS_TYPE_OPTIONS.iter().enumerate() {
            let is_date = *is_date;
            let on = cat_date == is_date;
            axis_type_buttons = axis_type_buttons.child(
                crate::shell::chrome::segment_control::SegmentItem::label(("chart-cat-axis-type", i),crate::shell::i18n::text(label),on).disabled(false).on_click(cx.listener(move |this, _, _, cx| this.set_sheet_chart_cat_axis_date(is_date, cx))),
            );
        }
        let series_list = self.chart_series_table(cx);
        let add_series_btn = crate::shell::chrome::primary_button("chart-series-add")
            .label(crate::shell::i18n::text("chart.ui.add"))
            .on_click(cx.listener(|this, _, window, cx| this.open_chart_series_editor(None, window, cx)));
        let show_hidden = !chart.plot_vis_only;
        let labeled = |key: &str, input: crate::shell::chrome::TextField| v_flex().gap_1().child(div().text_color(crate::shell::palette::rgb(0x5f6368)).child(crate::shell::i18n::text(key))).child(input);
        // A chartEx chart's own settings: the points that are totals (a waterfall), the bins (a histogram).
        let ex_section = chart.ex_source.as_ref().map(|source| {
            let xml = source.0.xml();
            let o = source.0.options();
            let mut section = v_flex().gap_2();
            if xml.contains("layoutId=\"waterfall\"") {
                section = section.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.chart-ex.totals")));
                let names: Vec<String> = chart.groups.first().and_then(|g| g.series.first()).map(|s| s.categories.clone()).unwrap_or_default();
                for (i, name) in names.iter().enumerate().take(60) {
                    let on = o.subtotals.contains(&i);
                    section = section.child(Checkbox::new(("chart-ex-total", i)).label(name.clone()).checked(on).on_click(cx.listener(move |this, _: &bool, _, cx| this.toggle_chart_ex_total(i, cx))));
                }
            }
            if xml.contains("layoutId=\"clusteredColumn\"") && !is_pareto(&xml) {
                section = section
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.chart-ex.bins")))
                    .child(labeled("xlsx.chart-ex.bin-width", crate::shell::chrome::text_field(&p.ex_bin_width).full_width()))
                    .child(labeled("xlsx.chart-ex.bin-count", chart_spin(&p.ex_bin_count, "chart-ex-bin-count", crate::shell::chrome::SpinnerSpec::new(1.0).min(1.0).integer_only(), cx)))
                    .child(labeled("xlsx.chart-ex.underflow", crate::shell::chrome::text_field(&p.ex_underflow).full_width()))
                    .child(labeled("xlsx.chart-ex.overflow", crate::shell::chrome::text_field(&p.ex_overflow).full_width()))
                    .child(div().text_color(crate::shell::palette::rgb(0x5f6368)).child(crate::shell::i18n::text("xlsx.chart-ex.bins-hint")))
                    .child(Checkbox::new("chart-ex-closed-right").label(crate::shell::i18n::text("xlsx.chart-ex.closed-right")).checked(o.closed_right).on_click(cx.listener(|this, on: &bool, _, cx| {
                        let on = *on;
                        this.set_chart_ex_options(move |o| o.closed_right = on, cx)
                    })))
                    .child(crate::shell::chrome::primary_button("chart-ex-bins-apply").small().label(crate::shell::i18n::text("xlsx.chart-ex.apply")).on_click(cx.listener(|this, _, _, cx| this.apply_chart_ex_bins(cx))));
            }
            if xml.contains("layoutId=\"boxWhisker\"") {
                section = section
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.chart-ex.box-whisker")))
                    .child(Checkbox::new("chart-ex-box-mean-marker").label(crate::shell::i18n::text("xlsx.chart-ex.box-mean-marker")).checked(o.box_mean_marker).on_click(cx.listener(|this, on: &bool, _, cx| {
                        let on = *on;
                        this.set_chart_ex_options(move |o| o.box_mean_marker = on, cx)
                    })))
                    .child(Checkbox::new("chart-ex-box-mean-line").label(crate::shell::i18n::text("xlsx.chart-ex.box-mean-line")).checked(o.box_mean_line).on_click(cx.listener(|this, on: &bool, _, cx| {
                        let on = *on;
                        this.set_chart_ex_options(move |o| o.box_mean_line = on, cx)
                    })))
                    .child(Checkbox::new("chart-ex-box-outliers").label(crate::shell::i18n::text("xlsx.chart-ex.box-outliers")).checked(o.box_outliers).on_click(cx.listener(|this, on: &bool, _, cx| {
                        let on = *on;
                        this.set_chart_ex_options(move |o| o.box_outliers = on, cx)
                    })))
                    .child(Checkbox::new("chart-ex-box-inclusive").label(crate::shell::i18n::text("xlsx.chart-ex.box-inclusive")).checked(o.box_inclusive).on_click(cx.listener(|this, on: &bool, _, cx| {
                        let on = *on;
                        this.set_chart_ex_options(move |o| o.box_inclusive = on, cx)
                    })));
            }
            if xml.contains("layoutId=\"treemap\"") {
                section = section
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.chart-ex.treemap")))
                    .child(Checkbox::new("chart-ex-treemap-overlapping").label(crate::shell::i18n::text("xlsx.chart-ex.treemap-overlapping")).checked(o.treemap_overlapping).on_click(cx.listener(|this, on: &bool, _, cx| {
                        let on = *on;
                        this.set_chart_ex_options(move |o| o.treemap_overlapping = on, cx)
                    })));
            }
            if is_funnel(&xml) {
                section = section
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.chart-ex.funnel")))
                    .child(labeled("xlsx.chart-ex.gap-width", chart_spin(&p.ex_gap_width, "chart-ex-funnel-gap", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 500.0).integer_only(), cx)))
                    .child(crate::shell::chrome::primary_button("chart-ex-funnel-apply").small().label(crate::shell::i18n::text("xlsx.chart-ex.apply-gap")).on_click(cx.listener(|this, _, _, cx| this.apply_chart_ex_gap_width(cx))));
            }
            if is_pareto(&xml) {
                section = section
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.chart-ex.pareto")))
                    .child(labeled("xlsx.chart-ex.gap-width", chart_spin(&p.ex_gap_width, "chart-ex-pareto-gap", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 500.0).integer_only(), cx)))
                    .child(crate::shell::chrome::primary_button("chart-ex-pareto-apply").small().label(crate::shell::i18n::text("xlsx.chart-ex.apply-gap")).on_click(cx.listener(|this, _, _, cx| this.apply_chart_ex_gap_width(cx))));
            }
            section
        });
        let field = |label: &str, input: crate::shell::chrome::TextField| crate::shell::chrome::text_field_row(label.to_string(), input);
        let command_font=self.resources.font(&ibis_doc_model::TextStyle::default());
        let command_width=["_xlsx.apply-range","_xlsx.switch-row-column","_xlsx.move-chart","_xlsx.save-as-template","_xlsx.apply-template"].iter().map(|key|self.resources.text_width(&crate::shell::i18n::text(key),&command_font,14.)+32.).fold(0.,f32::max);
        let kind_settings=v_flex().gap_2().when_some(ex_section,|b,section|b.child(section))
            .when(has_bar, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.series-options")))
                    .child(field(&crate::shell::i18n::text("_xlsx.gap-width"), chart_spin(&p.gap_width, "chart-gap-width", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 500.0).integer_only(), cx)))
                    .child(field(&crate::shell::i18n::text("_xlsx.series-overlap"), chart_spin(&p.overlap, "chart-overlap", crate::shell::chrome::SpinnerSpec::new(1.0).range(-100.0, 100.0).integer_only(), cx)))
            })
            .when(has_radar, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.radar-style")))
                    .child(radar_style_buttons)
            })
            .when(has_of_pie, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("fix.chart-edit.pie-chart-type")))
                    .child(of_pie_kind_buttons)
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("fix.chart-edit.split-series")))
                    .child(of_pie_split_buttons)
                    .child(field(&crate::shell::i18n::text("fix.chart-edit.split-position-value"), crate::shell::chrome::text_field(&p.of_pie_split_pos).full_width()))
                    .child(field(&crate::shell::i18n::text("fix.chart-edit.second-plot-size"), chart_spin(&p.of_pie_second_size, "chart-of-pie-size", crate::shell::chrome::SpinnerSpec::new(1.0).range(5.0, 200.0).integer_only(), cx)))
            })
            .when(has_stock, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("fix.chart-edit.stock-chart-options")))
                    .child(Checkbox::new("chart-stock-hi-low").label(crate::shell::i18n::text("fix.chart-edit.high-low-lines")).checked(stock_hi_low).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_stock_hi_low_lines(*on, cx))))
                    .child(Checkbox::new("chart-stock-up-down").label(crate::shell::i18n::text("fix.chart-edit.up-down-bars")).checked(stock_up_down).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_stock_up_down_bars(*on, cx))))
                    .child(Checkbox::new("chart-stock-drop").label(crate::shell::i18n::text("fix.chart-edit.drop-lines")).checked(stock_drop).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_stock_drop_lines(*on, cx))))
            })
            .when(has_surface, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("fix.chart-edit.surface-type")))
                    .child(surface_type_buttons)
            })
            .when(has_3d, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.chart-3d-rotation")))
                    .child(field(&crate::shell::i18n::text("xlsx.chart-3d-x-rotation"), chart_spin(&p.three_d_rot_x, "chart-3d-rot-x", crate::shell::chrome::SpinnerSpec::new(1.0).range(-90.0, 90.0).integer_only(), cx)))
                    .child(field(&crate::shell::i18n::text("xlsx.chart-3d-y-rotation"), chart_spin(&p.three_d_rot_y, "chart-3d-rot-y", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 360.0).integer_only(), cx)))
                    .child(Checkbox::new("chart-3d-right-angle-axes").label(crate::shell::i18n::text("xlsx.chart-3d-right-angle-axes")).checked(right_angle_axes).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_3d_rotation(None, None, Some(*on), None, cx))))
                    .child(field(&crate::shell::i18n::text("xlsx.chart-3d-depth"), chart_spin(&p.three_d_depth, "chart-3d-depth", crate::shell::chrome::SpinnerSpec::new(1.0).range(20.0, 2000.0).integer_only(), cx)))
                    .child(
                        crate::shell::chrome::action_button("chart-3d-reset-defaults")
                            .small()
                            .label(crate::shell::i18n::text("xlsx.chart-3d-reset-defaults"))
                            .on_click(cx.listener(|this, _, _, cx| this.reset_sheet_chart_3d_rotation(cx)))
                    )
            })
;
        let data_settings=v_flex().gap_2()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.data-table")))
            .child(Checkbox::new("chart-data-table-shown").label(crate::shell::i18n::text("_xlsx.show-data-table")).checked(has_data_table).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_data_table_shown(*on, cx))))
            .when(has_data_table, |b| {
                b.child(Checkbox::new("chart-data-table-keys").label(crate::shell::i18n::text("_xlsx.show-legend-keys")).checked(dt_keys).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_data_table_show_keys(*on, cx))))
                    .child(Checkbox::new("chart-data-table-horz").label(crate::shell::i18n::text("_xlsx.horizontal-border")).checked(dt_horz).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_data_table_show_horz_border(*on, cx))))
                    .child(Checkbox::new("chart-data-table-vert").label(crate::shell::i18n::text("_xlsx.vertical-border")).checked(dt_vert).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_data_table_show_vert_border(*on, cx))))
                    .child(Checkbox::new("chart-data-table-outline").label(crate::shell::i18n::text("_xlsx.outside-borders")).checked(dt_outline).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_data_table_show_outline(*on, cx))))
            })
            .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.hidden-and-empty-cells")))
            .child(blank_buttons.label(crate::shell::i18n::text("_xlsx.show-empty-cells-as")))
            .child(Checkbox::new("chart-show-hidden").label(crate::shell::i18n::text("_xlsx.show-data-in-hidden-rows-and-columns")).checked(show_hidden).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_plot_vis_only(!*on, cx))))
;
        let template_commands=v_flex().gap_2()
            .child(crate::shell::chrome::action_button("chart-move").w(px(command_width)).max_w_full().label(crate::shell::i18n::text("_xlsx.move-chart")).on_click(cx.listener(|e,_,w,cx|e.open_move_chart_panel(w,cx))))
            .child(crate::shell::chrome::action_button("chart-save-template").w(px(command_width)).max_w_full().label(crate::shell::i18n::text("_xlsx.save-as-template")).on_click(cx.listener(|e,_,w,cx|e.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Save,w,cx))))
            .child(crate::shell::chrome::action_button("chart-apply-template").w(px(command_width)).max_w_full().label(crate::shell::i18n::text("_xlsx.apply-template")).on_click(cx.listener(|e,_,w,cx|e.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Apply,w,cx))))
            .when_some(chart.pivot_source.clone(), |b, p_name| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.pivot-chart")))
                    .child(
                        h_flex().items_center().gap_2()
                            .child(div().text_color(crate::shell::palette::rgb(0x5f6368)).child(crate::shell::i18n::text("xlsx.pivot-chart-source")))
                            .child(div().font_weight(FontWeight::MEDIUM).child(p_name))
                    )
                    .child(
                        crate::shell::chrome::action_button("pivot-chart-open-fields")
                            .small()
                            .label(crate::shell::i18n::text("xlsx.pivot-chart-open-fields"))
                            .on_click(cx.listener(|this, _, window, cx| this.open_pivot_fields(window, cx)))
                    )
            })
;
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(self.chart_panel_tabs(cx))
            .when(p.tab == 2, |b| b
            .child(div().child(crate::shell::i18n::text("chart.ui.type")))
            .child(self.chart_type_dropdown(cx))
            .child(div().child(crate::shell::i18n::text("_xlsx.data-range")))
            .child(super::range_field::range_field(&p.range,"chart-global-range",cx).full_width())
            .child(crate::shell::chrome::action_button("chart-switch").w(px(command_width)).max_w_full().label(crate::shell::i18n::text("_xlsx.switch-row-column")).on_click(cx.listener(|this,_,w,cx|this.set_sheet_chart_data(true,w,cx))))
            )
            .when(p.tab == 0, |b| b.child(h_flex().justify_between().items_center().child(crate::shell::i18n::text("xlsx.chart-series")).child(add_series_btn)).child(series_list))
            .when(p.tab == 2, |b| b
            .child(div().child(crate::shell::i18n::text("_shell.chart-styles")))
            .child(self.chart_style_dropdown(cx))
            .child(crate::shell::chrome::dropdown_row(crate::shell::i18n::text("_xlsx.chart-colors"),self.chart_palette_dropdown(cx)))
            .child(Checkbox::new("chart-title-shown").label(crate::shell::i18n::text("chart.ui.show-title")).checked(titled).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_title_shown(*on, cx))))
            .child(div().pl(px(14.)).child(crate::shell::chrome::text_field(&p.title).full_width().disabled(!titled)))
            .child(places.label(crate::shell::i18n::text("_shell.legend")))
            .child(crate::shell::chrome::color_text_field_row(crate::shell::i18n::text("_shell.background"),crate::shell::chrome::panel_color_field("chart-background",&p.background,cx)))
            )
            .when(p.tab == 1, |b| b
            .child(self.chart_axis_tabs(cx))
            .when(cat_axis.is_some() && p.axis_tab == 0, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text(if cat_axis.is_some_and(|i|chart.axes[i].kind==AxisKind::Value){"chart.ui.horizontal-value-axis"}else if cat_axis.is_some_and(|i|matches!(chart.axes[i].side,Side::Left|Side::Right)){"chart.ui.vertical-categories"}else{"_xlsx.horizontal-category-axis"})))
                    .child(Checkbox::new("chart-cat-title-shown").label(crate::shell::i18n::text("_xlsx.show-axis-title")).checked(cat_titled).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_cat_axis_title_shown(*on, cx))))
                    .child(crate::shell::chrome::text_field(&p.cat_title).full_width())
                    .when(cat_axis.is_some_and(|i|chart.axes[i].kind==AxisKind::Category),|b|b.child(div().text_color(crate::shell::palette::rgb(0x5f6368)).child(crate::shell::i18n::text("_xlsx.axis-type"))).child(axis_type_buttons))
                    .when(cat_axis.is_some_and(|i|chart.axes[i].kind==AxisKind::Value),|b|b.child(field(&crate::shell::i18n::text("xlsx.chart-axis-min"),chart_spin(&p.domain_min,"chart-domain-min",crate::shell::chrome::SpinnerSpec::new(1.),cx))).child(field(&crate::shell::i18n::text("xlsx.chart-axis-max"),chart_spin(&p.domain_max,"chart-domain-max",crate::shell::chrome::SpinnerSpec::new(1.),cx))).child(field(&crate::shell::i18n::text("_xlsx.major-unit"),chart_spin(&p.domain_unit,"chart-domain-unit",crate::shell::chrome::SpinnerSpec::new(1.).min(0.),cx))))
                    .child(Checkbox::new("chart-cat-gridlines").label(crate::shell::i18n::text("_xlsx.gridlines")).checked(cat_gridlines).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_cat_gridlines(*on, cx))))
                    .child(Checkbox::new("chart-cat-reversed").label(crate::shell::i18n::text("_xlsx.categories-in-reverse-order")).checked(cat_reversed).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_cat_axis_reversed(*on, cx))))
            })
            .when(val_axis.is_some() && p.axis_tab == 1, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text(super::chart_series::value_axis_label(chart,false))))
                    .child(Checkbox::new("chart-val-title-shown").label(crate::shell::i18n::text("_xlsx.show-axis-title")).checked(val_titled).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_val_axis_title_shown(*on, cx))))
                    .child(crate::shell::chrome::text_field(&p.val_title).full_width())
                    .child(field(&crate::shell::i18n::text("xlsx.chart-axis-min"), crate::shell::chrome::text_field(&p.min).full_width()))
                    .child(field(&crate::shell::i18n::text("xlsx.chart-axis-max"), crate::shell::chrome::text_field(&p.max).full_width()))
                    .child(field(&crate::shell::i18n::text("_xlsx.major-unit"), crate::shell::chrome::text_field(&p.unit).full_width()))
                    .child(disp_unit_buttons.label(crate::shell::i18n::text("_xlsx.display-units")))
                    .child(Checkbox::new("chart-gridlines").label(crate::shell::i18n::text("_xlsx.gridlines")).checked(gridlines).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_gridlines(*on, cx))))
                    .child(Checkbox::new("chart-val-minor-gridlines").label(crate::shell::i18n::text("_xlsx.minor-gridlines")).checked(val_minor_gridlines).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_val_minor_gridlines(*on, cx))))
                    .child(Checkbox::new("chart-val-reversed").label(crate::shell::i18n::text("_xlsx.categories-in-reverse-order")).checked(val_reversed).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_val_axis_reversed(*on, cx))))
                    .child(Checkbox::new("chart-val-log-scale").label(crate::shell::i18n::text("_xlsx.logarithmic-scale")).checked(val_log_scale).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_val_axis_log_scale(on.then_some(Number(10.0)), cx))))
            })
            .when(sec_val_axis.is_some() && p.axis_tab == 2, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text(super::chart_series::value_axis_label(chart,true))))
                    .child(Checkbox::new("chart-sec-val-title-shown").label(crate::shell::i18n::text("_xlsx.show-axis-title")).checked(sec_val_titled).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_sec_val_axis_title_shown(*on, cx))))
                    .child(crate::shell::chrome::text_field(&p.sec_val_title).full_width())
                    .child(sec_disp_unit_buttons.label(crate::shell::i18n::text("_xlsx.display-units")))
                    .child(Checkbox::new("chart-sec-gridlines").label(crate::shell::i18n::text("_xlsx.gridlines")).checked(sec_val_gridlines).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_sec_val_axis_gridlines(*on, cx))))
                    .child(Checkbox::new("chart-sec-val-minor-gridlines").label(crate::shell::i18n::text("_xlsx.minor-gridlines")).checked(sec_val_minor_gridlines).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_sec_val_axis_minor_gridlines(*on, cx))))
                    .child(Checkbox::new("chart-sec-val-reversed").label(crate::shell::i18n::text("_xlsx.categories-in-reverse-order")).checked(sec_val_reversed).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_sec_val_axis_reversed(*on, cx))))
                    .child(Checkbox::new("chart-sec-val-log-scale").label(crate::shell::i18n::text("_xlsx.logarithmic-scale")).checked(sec_val_log_scale).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_sec_val_axis_log_scale(on.then_some(Number(10.0)), cx))))
            })
            .when(ser_axis.is_some() && p.axis_tab == 3, |b| {
                b.child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.series-axis")))
                    .child(Checkbox::new("chart-ser-title-shown").label(crate::shell::i18n::text("_xlsx.show-axis-title")).checked(ser_titled).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_ser_axis_title_shown(*on, cx))))
                    .child(crate::shell::chrome::text_field(&p.ser_title).full_width())
                    .child(Checkbox::new("chart-ser-gridlines").label(crate::shell::i18n::text("_xlsx.gridlines")).checked(ser_gridlines).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_ser_axis_gridlines(*on, cx))))
                    .child(Checkbox::new("chart-ser-reversed").label(crate::shell::i18n::text("_xlsx.categories-in-reverse-order")).checked(ser_reversed).on_click(cx.listener(|this, on: &bool, _, cx| this.set_sheet_chart_ser_axis_reversed(*on, cx))))
            })
            )
            .when(p.tab == 2, |b| b
            .child(self.chart_detail_section(0,"chart.ui.kind-settings",kind_settings.into_any_element(),cx))
            .child(self.chart_detail_section(1,"chart.ui.visible-data",data_settings.into_any_element(),cx))
            .child(self.chart_detail_section(2,"chart.ui.template-placement",template_commands.into_any_element(),cx))
            )
            .when_some(p.error.clone(), |b, e| b.child(div().text_color(theme.danger).child(e)))
            ;
        Some(SidebarPanel {title:crate::shell::i18n::text("_xlsx.format-chart"),body:body.into_any_element()})
    }
}

fn is_funnel(raw: &str) -> bool {
    raw.contains("layoutId=\"funnel\"")
}

fn is_pareto(raw: &str) -> bool {
    raw.contains("layoutId=\"pareto\"") || raw.contains("layoutId=\"paretoLine\"") || raw.contains("paretoLine")
}

/// A number field of the chart panel with ▲▼. The panel applies its fields with its buttons (適用), not on Enter, so a
/// step only changes the field.
fn chart_spin(input: &Entity<InputState>, id: &'static str, spec: crate::shell::chrome::SpinnerSpec, cx: &Context<SlideEditor>) -> crate::shell::chrome::TextField {
    crate::shell::chrome::text_field(input).full_width().spinner(id, spec, cx, |e, _, _, cx| e.apply_sheet_chart_panel(cx))
}
