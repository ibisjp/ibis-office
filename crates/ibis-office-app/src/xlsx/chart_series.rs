// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Chart tabs and a transactional, single-series editor.
use super::chart_edit::*;
use crate::shell::{
    chrome::{self, SidebarPanel},
    editor::SlideEditor,
    palette::rgb,
};
use chrome::segment_control::{Mode, SegmentItem};
use chrome::SegmentControlRow;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::chart::{AxisKind, DataLabels, Marker, MarkerSymbol};
use ibis_doc_model::{Chart, Color, Command, Fill, Number, Rgb, ShapeKind, Stroke};

pub(crate) struct SeriesEditor {
    pub chart: Chart,
    pub index: Option<usize>,
    owner_slide: ibis_doc_model::SlideId,
    pub tab: usize,
    pub name: Entity<InputState>,
    pub categories: Entity<InputState>,
    pub values: Entity<InputState>,
    pub fill: Entity<InputState>,
    pub stroke: Entity<InputState>,
    pub line_width: Entity<InputState>,
    pub marker_size: Entity<InputState>,
    pub order: Entity<InputState>,
    pub period: Entity<InputState>,
    pub error_value: Entity<InputState>,
    pub secondary: bool,
    refs: ibis_pptx::SeriesRefs,
    pub(crate) header: bool,
    full_refs: ibis_pptx::SeriesRefs,
    manual_name: String,
    pub error: Option<SharedString>,
}
fn label(key: &str) -> Div {
    div().child(crate::shell::i18n::text(key))
}
fn row(key: &str, control: impl IntoElement) -> Div {
    chrome::control_row(crate::shell::i18n::text(key), control)
}

impl SlideEditor {
    pub(crate) fn toggle_chart_series_header(&mut self,on:bool,window:&mut Window,cx:&mut Context<Self>){
        let Some(edit)=self.wb_mut().chart_edit.as_mut().and_then(|p|p.series_editor.as_mut())else{return;};
        if edit.header==on{return;}
        if !edit.header {
            edit.manual_name=edit.name.read(cx).value().to_string();
            edit.full_refs.values=Some(edit.values.read(cx).value().to_string());
            edit.full_refs.categories=Some(edit.categories.read(cx).value().to_string());
        }
        let mut refs=edit.full_refs.clone();
        if on {
            let Some((sheet,mut values))=refs.values.as_deref().and_then(ibis_pptx::parse_ref)else{return;};
            let first=ibis_doc_model::CellRange{last_row:values.first_row,last_column:values.first_column,..values};
            refs.name=Some(ibis_pptx::format_ref(&sheet,first));
            if values.first_column==values.last_column && values.first_row<values.last_row{values.first_row+=1;}
            else if values.first_row==values.last_row && values.first_column<values.last_column{values.first_column+=1;}else{return;}
            refs.values=Some(ibis_pptx::format_ref(&sheet,values));
            if let Some((sheet,mut cat))=refs.categories.as_deref().and_then(ibis_pptx::parse_ref){
                if cat.first_column==cat.last_column && cat.first_row<cat.last_row{cat.first_row+=1;}else if cat.first_row==cat.last_row && cat.first_column<cat.last_column{cat.first_column+=1;}
                refs.categories=Some(ibis_pptx::format_ref(&sheet,cat));
            }
        }else{refs.name=None;}
        edit.header=on;edit.refs=refs.clone();
        let name_field=edit.name.clone();let manual_name=edit.manual_name.clone();
        let displayed=if on{refs.name.as_deref().and_then(ibis_pptx::parse_ref).and_then(|(sheet,r)|ibis_pptx::sheet_of(self.presentation(),&sheet).and_then(|s|s.cell(r.first_row,r.first_column)).map(|c|c.text.clone())).unwrap_or_default()}else{manual_name};
        let edit=self.wb().chart_edit.as_ref().unwrap().series_editor.as_ref().unwrap();
        let fields=[(edit.values.clone(),refs.values.unwrap_or_default()),(edit.categories.clone(),refs.categories.unwrap_or_default()),(name_field,displayed)];
        for (field,value) in fields{field.update(cx,|i,cx|i.set_value(value,window,cx));}
        self.commit_chart_series_model(cx);cx.notify();
    }
    pub(crate) fn set_chart_ui_type(
        &mut self,
        preset: crate::pptx::chart_panel::ChartPreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((chart, index)) = self.series_type_context() {
            if let Some(reason) = super::chart_type_compatibility::restriction(chart, index, preset)
            {
                let message = crate::shell::i18n::text(reason);
                if let Some(edit) = self
                    .wb_mut()
                    .chart_edit
                    .as_mut()
                    .and_then(|p| p.series_editor.as_mut())
                {
                    edit.error = Some(message);
                }
                cx.notify();
                return;
            }
            if chart
                .groups
                .iter()
                .flat_map(|g| g.series.iter().map(move |_| g))
                .nth(index.unwrap_or(0))
                .is_some_and(|g| {
                    crate::pptx::chart_panel::ChartPreset::of(g.kind, g.grouping) == Some(preset)
                })
            {
                return;
            }
        }
        if let Some(edit) = self
            .wb()
            .chart_edit
            .as_ref()
            .and_then(|p| p.series_editor.as_ref())
        {
            let read = |i: &Entity<InputState>| i.read(cx).value().trim().to_string();
            let fill = read(&edit.fill);
            let stroke = read(&edit.stroke);
            let width = read(&edit.line_width).parse::<f64>().ok();
            let parse = |s: &str| {
                u32::from_str_radix(s.trim_start_matches('#'), 16)
                    .ok()
                    .map(Rgb)
            };
            let fill = parse(&fill);
            let stroke = parse(&stroke);
            if let Some(edit) = self
                .wb_mut()
                .chart_edit
                .as_mut()
                .and_then(|p| p.series_editor.as_mut())
            {
                let series = &mut edit.chart.groups[0].series[0];
                series.fill = fill.map_or(Fill::None, |r| Fill::Solid(Color::opaque(r)));
                if let Some(width) = width {
                    series.stroke = stroke
                        .filter(|_| width > 0.)
                        .map(|r| Stroke::solid(Color::opaque(r), (width * 12700.).round() as i64));
                }
            }
        }
        self.set_sheet_chart_type(preset, cx);
        if let Some(edit) = self
            .wb()
            .chart_edit
            .as_ref()
            .and_then(|p| p.series_editor.as_ref())
        {
            let s = &edit.chart.groups[0].series[0];
            let fields = [
                (
                    edit.fill.clone(),
                    match &s.fill {
                        Fill::Solid(c) => format!("#{:06X}", c.rgb.0),
                        _ => String::new(),
                    },
                ),
                (
                    edit.stroke.clone(),
                    s.stroke
                        .as_ref()
                        .map(|s| format!("#{:06X}", s.color.rgb.0))
                        .unwrap_or_default(),
                ),
                (
                    edit.line_width.clone(),
                    s.stroke.as_ref().map_or("0".into(), |s| {
                        ibis_calc::number_to_xml(s.width as f64 / 12700.)
                    }),
                ),
            ];
            for (input, value) in fields {
                input.update(cx, |i, cx| i.set_value(value, window, cx));
            }
        }
    }
    fn series_type_context(&self) -> Option<(&Chart, Option<usize>)> {
        let panel = self.wb().chart_edit.as_ref()?;
        let edit = panel.series_editor.as_ref()?;
        let actual = self
            .presentation()
            .slides
            .iter()
            .flat_map(|s| &s.shapes)
            .find(|s| s.id == panel.shape)
            .and_then(|s| {
                if let ShapeKind::Chart(c) = &s.kind {
                    Some(c.as_ref())
                } else {
                    None
                }
            })?;
        Some((actual, edit.index))
    }
    pub(crate) fn chart_type_menu_entries(&self) -> Vec<chrome::MenuEntry> {
        crate::pptx::chart_panel::ChartPreset::ALL
            .iter()
            .map(|(p, key)| {
                let preset = *p;
                let reason = self.series_type_context().and_then(|(chart, index)| {
                    super::chart_type_compatibility::restriction(chart, index, preset)
                });
                let name = crate::shell::i18n::text(key);
                let label = reason.map_or_else(
                    || name.to_string(),
                    |reason| format!("{} ({})", name, crate::shell::i18n::text(reason)),
                );
                chrome::MenuItem::new(key, label)
                    .disabled(reason.is_some())
                    .on(move |e, w, cx| e.set_chart_ui_type(preset, w, cx))
                    .into()
            })
            .collect()
    }
    pub(crate) fn can_delete_chart_series(&self, index: usize) -> bool {
        let Some(p) = self.wb().chart_edit.as_ref() else {
            return false;
        };
        let Some(chart) = self
            .presentation()
            .slides
            .iter()
            .flat_map(|s| &s.shapes)
            .find(|s| s.id == p.shape)
            .and_then(|s| {
                if let ShapeKind::Chart(c) = &s.kind {
                    Some(c.as_ref())
                } else {
                    None
                }
            })
        else {
            return false;
        };
        let total = chart.groups.iter().map(|g| g.series.len()).sum::<usize>();
        total > 1
            && index < total
            && self
                .source
                .as_ref()
                .and_then(|s| s.sheet_chart_refs(p.shape, chart))
                .is_some_and(|refs| index < refs.len())
    }
    pub(crate) fn chart_type_dropdown(&self, cx: &Context<Self>) -> impl IntoElement {
        let chart = self.sheet_chart().unwrap().1;
        let current = if chart.groups.len() == 1 {
            chart_kind_label(&chart.groups[0])
        } else {
            "chart.ui.combination"
        };
        let entries = self.chart_type_menu_entries();
        chrome::Dropdown::from_entries("chart-type-dropdown",crate::shell::i18n::text(current),entries,false,0,&cx.weak_entity()).into_any_element()
    }
    pub(crate) fn chart_detail_section(
        &self,
        index: usize,
        key: &'static str,
        body: AnyElement,
        cx: &Context<Self>,
    ) -> Div {
        let panel = self.wb().chart_edit.as_ref().unwrap();
        let open = panel.detail_open[index];
        div().mx(px(-18.)).child(chrome::collapsible_section(("chart-detail", index), crate::shell::i18n::text(key), open,
            div().px(px(18.)).child(body).into_any_element(), panel.detail_state[index].clone(), self.sidebar_scroll.clone(),
            cx.listener(move |e, _, _, cx| {
                e.wb_mut().chart_edit.as_mut().unwrap().detail_open[index] = !open;cx.notify();
            })))
    }

    pub(crate) fn chart_panel_tabs(&self, cx: &Context<Self>) -> Div {
        chrome::tab_control(
            [
                "xlsx.chart-series",
                "chart.ui.axes",
                "chart.ui.overall",
            ]
            .map(crate::shell::i18n::text),
            self.wb().chart_edit.as_ref().unwrap().tab,
            "chart-format-tab",
            |e, i, cx| {
                e.wb_mut().chart_edit.as_mut().unwrap().tab = i;
                e.document_memory.set("xlsx.chart-format.tab",i);
                cx.notify()
            },
            cx,
        )
    }
    pub(crate) fn chart_axis_tabs(&self, cx: &Context<Self>) -> Div {
        let p = self.wb().chart_edit.as_ref().unwrap();
        let chart = self.sheet_chart().unwrap().1;
        let domain_label = category_axis(chart)
            .map(|i| {
                if matches!(
                    chart.axes[i].side,
                    ibis_doc_model::chart::Side::Left | ibis_doc_model::chart::Side::Right
                ) {
                    "chart.ui.vertical-categories"
                } else {
                    "chart.ui.horizontal-axis"
                }
            })
            .unwrap_or("chart.ui.horizontal-axis");
        let mut items = vec![(0, domain_label), (1, value_axis_label(chart, false))];
        if secondary_value_axis(chart).is_some() {
            items.push((2, value_axis_label(chart, true)));
        }
        if series_axis(chart).is_some() {
            items.push((3, "_xlsx.series-axis"));
        }
        let mut segments = SegmentControlRow::new(Mode::Proportional { width: px(264.) });
        for (i, key) in items {
            segments = segments.child(
                SegmentItem::label(
                    ("chart-axis-tab", i),
                    crate::shell::i18n::text(key),
                    p.axis_tab == i,
                )
                .tooltip(crate::shell::i18n::text(key))
                .on_click(cx.listener(move |e, _, _, cx| {
                    e.wb_mut().chart_edit.as_mut().unwrap().axis_tab = i;
                    cx.notify()
                })),
            );
        }
        let axis = match p.axis_tab {
            0 => category_axis(chart),
            1 => value_axis(chart),
            2 => secondary_value_axis(chart),
            _ => series_axis(chart),
        };
        let names = axis
            .map(|i| {
                chart
                    .groups
                    .iter()
                    .filter(|g| g.axes.contains(&chart.axes[i].id))
                    .flat_map(|g| &g.series)
                    .map(|s| s.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let names = if p.axis_tab == 0 {
            chart
                .groups
                .iter()
                .flat_map(|g| &g.series)
                .next()
                .map(|s| {
                    if category_axis(chart).is_some_and(|i| chart.axes[i].kind == AxisKind::Value) {
                        s.x_values
                            .iter()
                            .take(8)
                            .map(|v| v.map_or(String::new(), |n| ibis_calc::number_to_xml(n.0)))
                            .collect::<Vec<_>>()
                            .join(", ")
                    } else {
                        s.categories
                            .iter()
                            .take(8)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                })
                .unwrap_or_default()
        } else {
            names
        };
        let key = match p.axis_tab {
            0 => category_axis(chart)
                .map(|i| {
                    if chart.axes[i].kind == AxisKind::Value {
                        "chart.ui.horizontal-values"
                    } else if matches!(
                        chart.axes[i].side,
                        ibis_doc_model::chart::Side::Left | ibis_doc_model::chart::Side::Right
                    ) {
                        "chart.ui.vertical-categories-label"
                    } else {
                        "chart.ui.horizontal-categories"
                    }
                })
                .unwrap_or("chart.ui.horizontal-categories"),
            1 => value_axis_label(chart, false),
            2 => value_axis_label(chart, true),
            _ => "_xlsx.series-axis",
        };
        v_flex()
            .gap_2()
            .child(segments.label(crate::shell::i18n::text("chart.ui.edit-axis")))
            .child(format!("{}: {}", crate::shell::i18n::text(key), names))
    }
    pub(crate) fn chart_series_table(&self, cx: &Context<Self>) -> impl IntoElement {
        use chrome::data_table::{DataTable, row_controls};
        let p = self.wb().chart_edit.as_ref().unwrap();
        let chart = self.sheet_chart().unwrap().1;
        let total = chart.groups.iter().map(|g| g.series.len()).sum::<usize>();
        let rows = chart
            .groups
            .iter()
            .flat_map(|g| g.series.iter().map(move |s| (g, s)))
            .enumerate()
            .map(|(i, (group, s))| {
                let edit = chrome::action_button(("chart-series-edit", i))
                    .small()
                    .w(px(24.))
                    .px_0()
                    .icon(crate::shell::icons::Material("edit"))
                    .tooltip(crate::shell::i18n::text("chart.ui.edit"))
                    .on_click(
                        cx.listener(move |e, _, w, cx| e.open_chart_series_editor(Some(i), w, cx)),
                    );
                let delete = chrome::action_button(("chart-series-delete", i))
                    .small()
                    .w(px(24.))
                    .px_0()
                    .icon(crate::shell::icons::Material("delete"))
                    .disabled(total <= 1)
                    .on_click(
                        cx.listener(move |e, _, w, cx| e.delete_sheet_chart_series(i, w, cx)),
                    );
                let name = if s.name.is_empty() {
                    format!(
                        "{} {}",
                        crate::shell::i18n::text("xlsx.chart-series"),
                        i + 1
                    )
                } else {
                    s.name.clone()
                };
                let kind = match group.kind {
                    ibis_doc_model::ChartType::Bar {
                        horizontal: true, ..
                    } => "bar_chart",
                    ibis_doc_model::ChartType::Bar { .. } => "bar_chart",
                    ibis_doc_model::ChartType::Line => "show_chart",
                    ibis_doc_model::ChartType::Pie { .. }
                    | ibis_doc_model::ChartType::Doughnut { .. } => "pie_chart",
                    ibis_doc_model::ChartType::Scatter { .. }
                    | ibis_doc_model::ChartType::Bubble { .. } => "scatter_plot",
                    _ => "insert_chart",
                };
                let kind_label = chart_kind_label(group);
                let secondary = secondary_value_axis(chart)
                    .is_some_and(|a| group.axes.contains(&chart.axes[a].id));
                let axis = value_axis_label(chart, secondary);
                let swatch = match &s.fill {
                    Fill::Solid(c) => c.rgb.0,
                    _ => s.stroke.as_ref().map_or(0xdddddd, |s| s.color.rgb.0),
                };
                let r = h_flex()
                    .h_full()
                    .px_2()
                    .gap(px(2.))
                    .items_center()
                    .child(
                        div()
                            .size(px(9.))
                            .flex_none()
                            .rounded(px(2.))
                            .bg(rgb(swatch)),
                    )
                    .child(
                        div()
                            .id(("chart-series-name", i))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(name)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |e, event: &MouseDownEvent, w, cx| {
                                    if event.click_count == 2 {
                                        e.open_chart_series_editor(Some(i), w, cx);
                                        cx.stop_propagation()
                                    }
                                }),
                            ),
                    )
                    .child(
                        chrome::action_button(("chart-series-type", i))
                            .ghost()
                            .small()
                            .px_0()
                            .w(px(18.))
                            .icon(crate::shell::icons::Material(kind))
                            .tooltip(crate::shell::i18n::text(kind_label)),
                    )
                    .child(
                        chrome::action_button(("chart-series-axis", i))
                            .ghost()
                            .small()
                            .w(px(18.))
                            .px_0()
                            .label(crate::shell::i18n::text(match axis {
                                "chart.ui.right-value-axis" => "chart.ui.right-short",
                                "chart.ui.top-value-axis" => "chart.ui.top-short",
                                "chart.ui.bottom-value-axis" => "chart.ui.bottom-short",
                                _ => "chart.ui.left-short",
                            }))
                            .tooltip(crate::shell::i18n::text(axis)),
                    )
                    .child(row_controls(p.series_reorder.clone(), i, 1, edit))
                    .child(row_controls(p.series_reorder.clone(), i, 2, delete))
                    .child(chrome::icon("menu", 18., rgb(0x5f6368).into()));
                (i, r.into_any_element(), false)
            })
            .collect();
        DataTable::new(
            "chart-series-table",
            Some(
                h_flex()
                    .h_full()
                    .items_center()
                    .px_2()
                    .child(crate::shell::i18n::text("xlsx.chart-series"))
                    .into_any_element(),
            ),
            rows,
            cx,
        )
        .reorderable(p.series_reorder.clone(), |e, from, to, w, cx| {
            e.reorder_chart_series(from, to, w, cx)
        })
    }
    fn reorder_chart_series(
        &mut self,
        from: usize,
        to: usize,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from == to {
            return;
        }
        let Some((shape, mut chart)) = self.sheet_chart().map(|(s, c)| (s, c.clone())) else {
            return;
        };
        let Some(source) = self.source.clone() else {
            return;
        };
        let Some(mut refs) = source.sheet_chart_refs(shape, &chart) else {
            return;
        };
        if from >= refs.len() || to >= refs.len() {
            return;
        }
        // Keep each series' chart kind and axis assignment when it crosses a combination group.
        let mut groups = chart
            .groups
            .iter()
            .flat_map(|g| {
                g.series.iter().map(|series| {
                    let mut one = g.clone();
                    one.series = vec![series.clone()];
                    one
                })
            })
            .collect::<Vec<_>>();
        let moving = groups.remove(from);
        groups.insert(to, moving);
        let moving = refs.remove(from);
        refs.insert(to, moving);
        let mut compact: Vec<ibis_doc_model::ChartGroup> = Vec::new();
        for g in groups {
            if let Some(last) = compact.last_mut() {
                let mut a = last.clone();
                a.series.clear();
                let mut b = g.clone();
                b.series.clear();
                if a == b {
                    last.series.extend(g.series);
                    continue;
                }
            }
            compact.push(g);
        }
        chart.groups = compact;
        chart.cell_refs = refs
            .iter()
            .map(|r| ibis_doc_model::SeriesCells {
                name: r.name.clone(),
                categories: r.categories.clone(),
                values: r.values.clone(),
                sizes: r.sizes.clone(),
            })
            .collect();
        source.set_chart_data(
            shape,
            chart.clone(),
            ibis_pptx::ChartData {
                workbook: vec![],
                series: refs,
            },
        );
        self.document
            .execute(vec![Command::SetKind {
                slide: self.current_slide,
                shape,
                kind: Box::new(ShapeKind::Chart(Box::new(chart))),
            }])
            .ok();
        let _ = w;
        cx.notify();
    }
    pub(crate) fn open_chart_series_editor(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((shape, chart)) = self.sheet_chart().map(|(s, c)| (s, c.clone())) else {
            return;
        };
        let flat = chart
            .groups
            .iter()
            .flat_map(|g| &g.series)
            .collect::<Vec<_>>();
        let Some(series) = flat.get(index.unwrap_or(0)).cloned().cloned() else {
            return;
        };
        let group = chart
            .groups
            .iter()
            .find(|g| {
                g.series
                    .iter()
                    .any(|s| std::ptr::eq(s, flat[index.unwrap_or(0)]))
            })
            .cloned()
            .unwrap();
        let refs = self
            .source
            .as_ref()
            .and_then(|s| s.sheet_chart_refs(shape, &chart))
            .and_then(|r| r.get(index.unwrap_or(0)).cloned())
            .unwrap_or_default();
        let mut draft = chart.clone();
        draft.groups = vec![group];
        draft.groups[0].series = vec![series.clone()];
        let field = |value: String, cx: &mut Context<Self>, window: &mut Window| {
            let i = cx.new(|cx| InputState::new(window, cx));
            i.update(cx, |i, cx| i.set_value(value, window, cx));
            i
        };
        let color = |fill: &Fill| match fill {
            Fill::Solid(c) => format!("#{:06X}", c.rgb.0),
            _ => String::new(),
        };
        let mut full_refs=refs.clone();
        let header=refs.name.as_ref().and_then(|n|ibis_pptx::parse_ref(n)).zip(refs.values.as_ref().and_then(|v|ibis_pptx::parse_ref(v))).is_some_and(|((ns,n),(vs,v))|ns==vs && n.first_row==n.last_row && n.first_column==n.last_column && ((v.first_column==v.last_column && n.first_column==v.first_column && n.first_row.checked_add(1)==Some(v.first_row)) || (v.first_row==v.last_row && n.first_row==v.first_row && n.first_column.checked_add(1)==Some(v.first_column))));
        if header {
            for r in [&mut full_refs.values,&mut full_refs.categories]{if let Some((sheet,mut range))=r.as_deref().and_then(ibis_pptx::parse_ref){if range.first_column==range.last_column && range.first_row>0{range.first_row-=1;}else if range.first_row==range.last_row && range.first_column>0{range.first_column-=1;}*r=Some(ibis_pptx::format_ref(&sheet,range));}}
        }
        let editor = SeriesEditor {
            header, full_refs, manual_name:series.name.clone(),
            owner_slide: self.current_slide,
            refs: refs.clone(),
            chart: draft,
            index,
            tab: 0,
            name: field(
                if index.is_none() {
                    String::new()
                } else {
                    if header {series.name.clone()} else {refs.name.as_ref().map(|r|format!("={r}")).unwrap_or_else(||series.name.clone())}
                },
                cx,
                window,
            ),
            categories: field(refs.categories.unwrap_or_default(), cx, window),
            values: field(refs.values.unwrap_or_default(), cx, window),
            fill: field(
                if matches!(series.fill, Fill::None) {
                    series
                        .marker
                        .as_ref()
                        .map(|m| color(&m.fill))
                        .unwrap_or_default()
                } else {
                    color(&series.fill)
                },
                cx,
                window,
            ),
            stroke: field(
                series
                    .stroke
                    .as_ref()
                    .map(|s| format!("#{:06X}", s.color.rgb.0))
                    .unwrap_or_default(),
                cx,
                window,
            ),
            line_width: field(
                series
                    .stroke
                    .as_ref()
                    .map(|s| ibis_calc::number_to_xml(s.width as f64 / 12700.))
                    .unwrap_or_else(|| "0".into()),
                cx,
                window,
            ),
            marker_size: field(
                series.marker.as_ref().map_or(5, |m| m.size).to_string(),
                cx,
                window,
            ),
            order: field(
                series.trendline.as_ref().map_or(2, |t| t.order).to_string(),
                cx,
                window,
            ),
            period: field(
                series
                    .trendline
                    .as_ref()
                    .map_or(2, |t| t.period)
                    .to_string(),
                cx,
                window,
            ),
            error_value: field(
                series
                    .error_bars
                    .as_ref()
                    .and_then(|e| e.value)
                    .map_or("1".into(), |n| ibis_calc::number_to_xml(n.0)),
                cx,
                window,
            ),
            secondary: secondary_value_axis(&chart)
                .is_some_and(|i| draft_axis_contains(&chart, index.unwrap_or(0), chart.axes[i].id)),
            error: None,
        };
        for input in [&editor.categories, &editor.values] {
            cx.subscribe_in(
                input,
                window,
                |this, _, event: &gpui_kit::component::input::InputEvent, w, cx| {
                    if matches!(
                        event,
                        gpui_kit::component::input::InputEvent::PressEnter { .. }
                            | gpui_kit::component::input::InputEvent::Blur
                    ) && !this.sheet_range_pick_active()
                    {
                        this.validate_chart_series_ranges(w, cx);
                    }
                },
            )
            .detach();
        }
        for input in [&editor.name,&editor.categories,&editor.values,&editor.fill,&editor.stroke,&editor.line_width,&editor.marker_size,&editor.order,&editor.period,&editor.error_value] {
            cx.subscribe_in(input,window,|this,_,event:&gpui_kit::component::input::InputEvent,_,cx| {
                if matches!(event,gpui_kit::component::input::InputEvent::Change) {this.commit_chart_series_editor(cx);}
            }).detach();
        }
        self.wb_mut().chart_edit.as_mut().unwrap().series_editor = Some(editor);
        self.panel_dialog.bounds = None;
        self.panel_dialog.key = None;
        cx.notify();
    }
    pub(crate) fn cancel_chart_series_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sheet_range_pick_active() {
            self.finish_sheet_range_pick(true, window, cx);
            return;
        }

        if let Some(p) = self.wb_mut().chart_edit.as_mut() {
            p.series_editor = None;
        }
        self.panel_dialog.key = None;
        self.panel_dialog.bounds = None;
        self.panel_dialog.active = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }
    fn validate_chart_series_ranges(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self
            .wb()
            .chart_edit
            .as_ref()
            .and_then(|p| p.series_editor.as_ref())
        else {
            return;
        };
        let fields = [edit.categories.clone(), edit.values.clone()];
        let normalized = fields
            .iter()
            .map(|i| {
                super::sheet_ui::parse_reference(i.read(cx).value().trim())
                    .and_then(|r| self.resolve_reference(&r))
                    .and_then(|(id, r)| {
                        self.presentation()
                            .slides
                            .iter()
                            .find(|s| s.id == id)
                            .map(|s| {
                                ibis_pptx::format_ref(
                                    &ibis_pptx::quote_sheet(s.name.as_deref().unwrap_or_default()),
                                    r,
                                )
                            })
                    })
            })
            .collect::<Vec<_>>();
        let valid = normalized.iter().all(Option::is_some);
        if valid {
            for (input, text) in fields.iter().zip(normalized) {
                let text = text.unwrap();
                if input.read(cx).value().as_ref() != text {
                    input.update(cx, |i, cx| i.set_value(text, window, cx));
                }
            }
        }
        if let Some(edit) = self
            .wb_mut()
            .chart_edit
            .as_mut()
            .and_then(|p| p.series_editor.as_mut())
        {
            edit.error = (!valid).then(|| crate::shell::i18n::text("chart.ui.invalid-series"));
        }
        cx.notify();
    }
    pub(crate) fn apply_chart_series_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sheet_range_pick_active() {self.finish_sheet_range_pick(false,window,cx);return;}
        self.commit_chart_series_editor(cx);
        self.cancel_chart_series_editor(window,cx);
    }
    pub(crate) fn commit_chart_series_editor(&mut self, cx: &mut Context<Self>) {
        self.commit_chart_series_edit(true,cx);
    }
    pub(crate) fn commit_chart_series_model(&mut self, cx: &mut Context<Self>) {
        self.commit_chart_series_edit(false,cx);
    }
    fn commit_chart_series_edit(&mut self, from_inputs: bool, cx: &mut Context<Self>) {
        if self.sheet_range_pick_active() {return;}
        let Some(p) = self.wb().chart_edit.as_ref() else {
            return;
        };
        let Some(edit) = p.series_editor.as_ref() else {
            return;
        };
        let shape = p.shape;
        let owner_slide = edit.owner_slide;
        let index = edit.index;
        let mut draft = edit.chart.clone();
        let read = |i: &Entity<InputState>| i.read(cx).value().trim().to_string();
        let normalize = |text: String| {
            super::sheet_ui::parse_reference(text.trim_start_matches('='))
                .and_then(|r| self.resolve_reference(&r))
                .and_then(|(id, r)| {
                    self.presentation()
                        .slides
                        .iter()
                        .find(|s| s.id == id)
                        .map(|s| {
                            ibis_pptx::format_ref(
                                &ibis_pptx::quote_sheet(s.name.as_deref().unwrap_or_default()),
                                r,
                            )
                        })
                })
        };
        let refs = ibis_pptx::SeriesRefs {
            categories: normalize(read(&edit.categories)),
            values: normalize(read(&edit.values)),
            sizes: edit.refs.sizes.clone(),
            name: if edit.header { edit.refs.name.clone() } else if read(&edit.name).starts_with('=') {
                normalize(read(&edit.name))
            } else { None },
        };
        let valid = |r: &Option<String>| {
            r.as_ref()
                .is_some_and(|r| ibis_pptx::parse_ref(r).is_some())
        };
        let width = read(&edit.line_width)
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n >= 0.);
        let marker = read(&edit.marker_size)
            .parse::<u32>()
            .ok()
            .filter(|n| (2..=72).contains(n));
        if (refs.categories.is_some() && !valid(&refs.categories)) || !valid(&refs.values) || (from_inputs && (width.is_none() || marker.is_none()))
            || (read(&edit.name).starts_with('=') && refs.name.as_ref().and_then(|r|ibis_pptx::parse_ref(r)).is_none_or(|(_,r)|r.first_row!=r.last_row || r.first_column!=r.last_column)) {
            self.wb_mut()
                .chart_edit
                .as_mut()
                .unwrap()
                .series_editor
                .as_mut()
                .unwrap()
                .error = Some(crate::shell::i18n::text("chart.ui.invalid-series"));
            cx.notify();
            return;
        }
        let secondary = edit.secondary;
        let name = read(&edit.name);
        let fill = read(&edit.fill);
        let stroke = read(&edit.stroke);
        let s = &mut draft.groups[0].series[0];
        s.name = name;
        let parse_color = |text: &str| {
            u32::from_str_radix(text.trim_start_matches('#'), 16)
                .ok()
                .filter(|_| text.trim_start_matches('#').len() == 6)
                .map(Rgb)
        };
        if from_inputs {
            // Partial hexadecimal/numeric entry stays in the input until it is valid.
            if [&fill,&stroke].iter().any(|v|!v.is_empty() && parse_color(v).is_none()) {return;}
        s.fill = parse_color(&fill).map_or(Fill::None, |r| Fill::Solid(Color::opaque(r)));
        s.stroke = if width.unwrap() == 0. {
            None
        } else {
            parse_color(&stroke)
                .map(|r| Stroke::solid(Color::opaque(r), (width.unwrap() * 12700.).round() as i64))
        };
        if let Some(m) = s.marker.as_mut() {
            m.size = marker.unwrap();
            if let Some(color) = parse_color(&fill) {
                m.fill = Fill::Solid(Color::opaque(color));
            }
        }
        if let Some(t) = s.trendline.as_mut() {
            t.order = read(&edit.order).parse::<u8>().unwrap_or(2).clamp(2, 6);
            t.period = read(&edit.period).parse::<u32>().unwrap_or(2).max(2);
        }
        if let Some(e) = s.error_bars.as_mut() {
            e.value = read(&edit.error_value)
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite() && *n >= 0.)
                .map(Number);
        }
        }
        let Some(source) = self.source.clone() else {
            return;
        };
        let Some(actual) = self
            .presentation()
            .slides
            .iter()
            .find(|s| s.id == owner_slide)
            .and_then(|slide| slide.shapes.iter().find(|s| s.id == shape))
            .and_then(|s| {
                if let ShapeKind::Chart(c) = &s.kind {
                    Some((**c).clone())
                } else {
                    None
                }
            })
        else {
            return;
        };
        let Some(mut allrefs) = source.sheet_chart_refs(shape, &actual) else {
            return;
        };
        let before = actual.clone();
        let mut target = actual;
        let mut at = 0;
        let mut destination = None;
        for (gi, g) in target.groups.iter_mut().enumerate() {
            if let Some(index) = index {
                if index >= at && index < at + g.series.len() {
                    g.series.remove(index - at);
                    destination = Some((gi, index - at));
                    break;
                }
            }
            at += g.series.len();
        }
        let newseries = draft.groups[0].series[0].clone();
        let kind = draft.groups[0].kind.clone();
        let grouping = draft.groups[0].grouping;
        let mut axes = draft.groups[0].axes.clone();
        if secondary {
            let axis_id = if let Some(i) = secondary_value_axis(&target) {
                Some(target.axes[i].id)
            } else if let Some(i) = value_axis(&target) {
                let mut secondary = target.axes[i].clone();
                secondary.id = target.axes.iter().map(|a| a.id).max().unwrap_or(0) + 1;
                secondary.side = if matches!(
                    secondary.side,
                    ibis_doc_model::chart::Side::Bottom | ibis_doc_model::chart::Side::Top
                ) {
                    ibis_doc_model::chart::Side::Top
                } else {
                    ibis_doc_model::chart::Side::Right
                };
                secondary.source_id = Some(secondary.id);
                secondary.deleted = false;
                secondary.crosses = Some(ibis_doc_model::AxisCrosses::Maximum);
                secondary.crosses_at = None;
                let id = secondary.id;
                target.axes.push(secondary);
                Some(id)
            } else {
                None
            };
            if let Some(id) = axis_id {
                replace_range_axis(&target, &mut axes, id);
            }
        }
        if !secondary {
            if let Some(i) = value_axis(&target) {
                replace_range_axis(&target, &mut axes, target.axes[i].id);
            }
        }
        let (gi, si) = destination.unwrap_or((0, target.groups[0].series.len()));
        let same = target.groups[gi].kind == kind
            && target.groups[gi].grouping == grouping
            && target.groups[gi].axes == axes;
        if same {
            target.groups[gi].series.insert(si, newseries);
        } else {
            let mut group = draft.groups[0].clone();
            group.axes = axes;
            target.groups.push(group);
        }
        if let Some(i) = index {
            allrefs.remove(i);
        }
        let insert = target
            .groups
            .iter()
            .take(gi)
            .map(|g| g.series.len())
            .sum::<usize>()
            + si;
        let append = !same;
        if append {
            allrefs.push(refs);
        } else {
            allrefs.insert(insert.min(allrefs.len()), refs);
        }
        target.groups.retain(|g| !g.series.is_empty());
        ensure_primary_series_axis(&mut target);
        let mut rebuilt = ibis_pptx::sheet_chart_from_data(&target, self.presentation(), &allrefs);
        // A literal series name is a draft value, not a workbook name formula.
        let flatindex = if append {
            allrefs.len() - 1
        } else {
            insert.min(allrefs.len() - 1)
        };
        if allrefs[flatindex].name.is_none() {
        if let Some(s) = rebuilt
            .groups
            .iter_mut()
            .flat_map(|g| &mut g.series)
            .nth(flatindex)
        {
            s.name = draft.groups[0].series[0].name.clone();
        }
        }
        source.set_chart_data(shape,rebuilt.clone(),ibis_pptx::ChartData {workbook:vec![],series:allrefs.clone()});
        if rebuilt != before {
            self.document.execute(vec![Command::SetKind {slide:owner_slide,shape,kind:Box::new(ShapeKind::Chart(Box::new(rebuilt.clone())))}]).ok();
        }
        // A type/axis change may move this series to a new group. Keep the live editor bound to it.
        let mut draft = rebuilt.clone();
        let selected = rebuilt.groups.iter().flat_map(|g|g.series.iter().map(move |s|(g,s))).nth(flatindex);
        if let Some((group,series))=selected {
            let mut group=group.clone();group.series=vec![series.clone()];draft.groups=vec![group];
        }
        let secondary=secondary_value_axis(&rebuilt).is_some_and(|i|draft_axis_contains(&rebuilt,flatindex,rebuilt.axes[i].id));
        if let Some(edit)=self.wb_mut().chart_edit.as_mut().and_then(|p|p.series_editor.as_mut()) {
            edit.index=Some(flatindex);edit.chart=draft;edit.refs=allrefs[flatindex].clone();edit.secondary=secondary;edit.error=None;
        }
        cx.notify();
    }

    pub(crate) fn chart_series_editor_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<SidebarPanel> {
        let edit = self.wb().chart_edit.as_ref()?.series_editor.as_ref()?;
        let s = &edit.chart.groups[0].series[0];
        let body_width =
            (self.panel_dialog.bounds.map_or(px(520.), |b| b.size.width) - px(32.)).max(px(124.));
        let mut body = v_flex()
            .gap_3()
            .px_4()
            .py_2()
            .p_0().when(!self.sheet_range_pick_active(), |b| {
                b.child(chrome::tab_control(
                    ["chart.ui.data", "chart.ui.display", "chart.ui.analysis"]
                        .map(crate::shell::i18n::text),
                    edit.tab,
                    "chart-series-tab",
                    |e, i, cx| {
                        e.wb_mut()
                            .chart_edit
                            .as_mut()
                            .unwrap()
                            .series_editor
                            .as_mut()
                            .unwrap()
                            .tab = i;
                        cx.notify()
                    },
                    cx,
                ))
            });
        if self.sheet_range_pick_active() {
            if self.is_sheet_range_pick_target(&edit.name) {
                body = body.child(chrome::text_field_row(crate::shell::i18n::text("chart.ui.name"), super::range_field::range_field(&edit.name, "chart-series-name", cx).full_width()));
            } else if self.is_sheet_range_pick_target(&edit.categories) {
                body = body.child(label("chart.ui.categories")).child(
                    super::range_field::range_field(
                        &edit.categories,
                        "chart-series-categories",
                        cx,
                    )
                    .full_width(),
                );
            } else {
                body = body.child(label("chart.ui.values")).child(
                    super::range_field::range_field(&edit.values, "chart-series-values", cx)
                        .full_width(),
                );
            }
        } else if edit.tab == 0 {
            let axes = SegmentControlRow::new(Mode::Intrinsic)
                .child(
                    SegmentItem::label(
                        "chart-series-primary",
                        crate::shell::i18n::text(value_axis_label(&edit.chart, false)),
                        !edit.secondary,
                    )
                    .on_click(cx.listener(|e, _, _, cx| {
                        e.wb_mut()
                            .chart_edit
                            .as_mut()
                            .unwrap()
                            .series_editor
                            .as_mut()
                            .unwrap()
                            .secondary = false;
                        e.commit_chart_series_model(cx);
                        cx.notify()
                    })),
                )
                .child(
                    SegmentItem::label(
                        "chart-series-secondary",
                        crate::shell::i18n::text(value_axis_label(&edit.chart, true)),
                        edit.secondary,
                    )
                    .disabled(edit.chart.groups[0].three_d.is_some())
                    .tooltip(crate::shell::i18n::text(
                        if edit.chart.groups[0].three_d.is_some() {
                            "chart.ui.type-three-d"
                        } else {
                            "chart.ui.right-value-axis"
                        },
                    ))
                    .on_click(cx.listener(|e, _, _, cx| {
                        e.wb_mut()
                            .chart_edit
                            .as_mut()
                            .unwrap()
                            .series_editor
                            .as_mut()
                            .unwrap()
                            .secondary = true;
                        e.commit_chart_series_model(cx);
                        cx.notify()
                    })),
                );
            body = body
                .child(chrome::Checkbox::new("chart-series-header").label(crate::shell::i18n::text("chart.ui.first-cell-name")).checked(edit.header).on_click(cx.listener(|e,on:&bool,w,cx|e.toggle_chart_series_header(*on,w,cx))))
                .child(chrome::text_field_row(
                    crate::shell::i18n::text("chart.ui.name"),
                    super::range_field::range_field(&edit.name, "chart-series-name", cx).full_width().disabled(edit.header),
                ))
                .child(label("chart.ui.categories"))
                .child(
                    super::range_field::range_field(
                        &edit.categories,
                        "chart-series-categories",
                        cx,
                    )
                    .full_width(),
                )
                .child(label("chart.ui.values"))
                .child(
                    super::range_field::range_field(&edit.values, "chart-series-values", cx)
                        .full_width(),
                )
                .child(chrome::dropdown_row(crate::shell::i18n::text("chart.ui.type"), self.chart_type_dropdown(cx)))
                .when(!edit.chart.groups[0].axes.is_empty(), |b| {
                    b.child(axes.label(crate::shell::i18n::text("chart.ui.use-axis")).available_width(body_width))
                });
        } else if edit.tab == 1 {
            let marker_allowed = edit.chart.groups[0].kind.supports_markers();
            let marker_color =
                u32::from_str_radix(edit.fill.read(cx).value().trim_start_matches('#'), 16)
                    .ok()
                    .or_else(|| {
                        s.marker.as_ref().and_then(|m| {
                            if let Fill::Solid(c) = m.fill {
                                Some(c.rgb.0)
                            } else {
                                None
                            }
                        })
                    })
                    .or_else(|| s.stroke.as_ref().map(|s| s.color.rgb.0))
                    .unwrap_or(0x444746);

            let marker = s.marker.as_ref().map_or(MarkerSymbol::None, |m| m.symbol);
            let mut marker_items = Vec::new();
            for (i, (key, sym)) in MARKER_SYMBOLS.iter().enumerate() {
                let sym = *sym;
                let glyph = match sym {
                    MarkerSymbol::None => "—",
                    MarkerSymbol::Circle => "●",
                    MarkerSymbol::Square => "■",
                    MarkerSymbol::Diamond => "◆",
                    _ => "▲",
                };
                marker_items.push(
                    SegmentItem::icon(
                        ("chart-series-marker", i),
                        move || {
                            div()
                                .text_color(rgb(if sym == MarkerSymbol::None {
                                    0x444746
                                } else {
                                    marker_color
                                }))
                                .child(glyph)
                                .into_any_element()
                        },
                        marker == sym,
                    )
                    .disabled(!marker_allowed)
                    .tooltip(crate::shell::i18n::text(key))
                    .on_click(
                        cx.listener(move |e, _, _, cx| e.set_sheet_chart_marker_symbol(sym, cx)),
                    ),
                );
            }
            let width = chrome::text_field(&edit.line_width)
                .unit("pt", px(22.))
                .spinner(
                    "chart-series-line-width",
                    chrome::SpinnerSpec::new(0.25).range(0., 100.),
                    cx,
                    |e, _, _, cx| e.commit_chart_series_editor(cx),
                );
            let size = chrome::text_field(&edit.marker_size)
                .disabled(!marker_allowed)
                .unit("pt", px(22.))
                .spinner(
                    "chart-series-marker-size",
                    chrome::SpinnerSpec::new(1.).range(2., 72.).integer_only(),
                    cx,
                    |e, _, _, cx| e.commit_chart_series_editor(cx),
                );
            let mut label_items = Vec::new();
            for (i, (key, pos)) in LABEL_POSITIONS.iter().enumerate() {
                let pos = *pos;
                let current = s.labels.as_ref().map(|l| l.position);
                label_items.push(
                    SegmentItem::label(
                        ("chart-series-label-position", i),
                        crate::shell::i18n::text(key),
                        current == pos,
                    )
                    .on_click(
                        cx.listener(move |e, _, _, cx| e.set_sheet_chart_labels_position(pos, cx)),
                    ),
                );
            }
            body = body
                .child(row(
                    "pptx.common.fill",
                    chrome::panel_color_field("chart-series-fill", &edit.fill, cx),
                ))
                .child(chrome::color_field_row(
                    crate::shell::i18n::text("_shell.border-color"),
                    chrome::panel_color_field("chart-series-stroke", &edit.stroke, cx),
                ))
                .child(row("pptx.chrome.border-weight", width))
                .child(chrome::control_row::LabeledSegments {
                    label: crate::shell::i18n::text("_xlsx.marker"),
                    items: marker_items,
                    available: body_width,
                })
                .child(row("chart.ui.marker-size", size))
                .when(
                    matches!(
                        edit.chart.groups[0].kind,
                        ibis_doc_model::ChartType::Scatter { .. }
                    ),
                    |b| {
                        b.child(
                            chrome::Checkbox::new("chart-series-smooth")
                                .label(crate::shell::i18n::text("chart.ui.smooth"))
                                .checked(matches!(
                                    edit.chart.groups[0].kind,
                                    ibis_doc_model::ChartType::Scatter { smooth: true }
                                ))
                                .on_click(cx.listener(|e, on: &bool, _, cx| {
                                    let on = *on;
                                    e.change_sheet_chart(
                                        move |c| {
                                            if let ibis_doc_model::ChartType::Scatter { smooth } =
                                                &mut c.groups[0].kind
                                            {
                                                *smooth = on;
                                            }
                                        },
                                        cx,
                                    )
                                })),
                        )
                    },
                )
                .child(chrome::control_row::LabeledSegments {
                    label: crate::shell::i18n::text("_xlsx.data-labels"),
                    items: label_items,
                    available: body_width,
                });
        } else if !edit.chart.groups[0].kind.supports_series_analysis() {
            body = body.child(label("chart.ui.analysis-unsupported"));
        } else {
            let selected=s.trendline.as_ref().map(|t|t.kind);
            let entries=TRENDLINE_TYPES.iter().map(|(key,kind)| {
                let kind=*kind;
                chrome::MenuItem::tr(key).checked(kind==selected).on(move |e,_,cx|e.set_sheet_chart_trendline_type(kind,cx)).into()
            }).collect();
            let current=TRENDLINE_TYPES.iter().find(|(_,kind)|*kind==selected).unwrap().0;
            let dropdown=chrome::Dropdown::from_entries("chart-series-trend",crate::shell::i18n::text(current),entries,false,0,&cx.weak_entity()).into_any_element();
            body=body.child(chrome::dropdown_row(crate::shell::i18n::text("pptx.chrome.trendline"),div().w(chrome::FIELD_WIDTH).child(dropdown)));
            if let Some(t) = &s.trendline {
                if t.kind == ibis_doc_model::chart::TrendlineType::Polynomial {
                    body = body.child(row(
                        "chart.ui.order",
                        chrome::text_field(&edit.order).spinner(
                            "chart-series-order",
                            chrome::SpinnerSpec::new(1.).range(2., 6.).integer_only(),
                            cx,
                            |e, _, _, cx| e.commit_chart_series_editor(cx),
                        ),
                    ));
                }
                if t.kind == ibis_doc_model::chart::TrendlineType::MovingAverage {
                    body = body.child(row(
                        "chart.ui.period",
                        chrome::text_field(&edit.period).spinner(
                            "chart-series-period",
                            chrome::SpinnerSpec::new(1.).min(2.).integer_only(),
                            cx,
                            |e, _, _, cx| e.commit_chart_series_editor(cx),
                        ),
                    ));
                }
                body = body
                    .child(
                        chrome::Checkbox::new("chart-series-equation")
                            .label(crate::shell::i18n::text("_xlsx.display-equation-on-chart"))
                            .checked(t.disp_eq)
                            .on_click(cx.listener(|e, on: &bool, _, cx| {
                                e.set_sheet_chart_trendline_disp_eq(*on, cx)
                            })),
                    )
                    .child(
                        chrome::Checkbox::new("chart-series-r-squared")
                            .label(crate::shell::i18n::text(
                                "_xlsx.display-r-squared-value-on-chart",
                            ))
                            .checked(t.disp_r_sqr)
                            .on_click(cx.listener(|e, on: &bool, _, cx| {
                                e.set_sheet_chart_trendline_disp_r_sqr(*on, cx)
                            })),
                    );
            }
            body = body.child(
                chrome::Checkbox::new("chart-series-errors")
                    .label(crate::shell::i18n::text("_xlsx.show-error-bars"))
                    .checked(s.error_bars.is_some())
                    .on_click(cx.listener(|e, on: &bool, _, cx| {
                        e.set_sheet_chart_error_bars_shown(*on, cx)
                    })),
            );
            if let Some(error) = &s.error_bars {
                let mut types = SegmentControlRow::new(Mode::Proportional {
                    width: (body_width - px(132.)).max(px(124.)),
                });
                for (i, (key, t)) in ERROR_BAR_TYPES.iter().enumerate() {
                    let t = *t;
                    types = types.child(
                        SegmentItem::label(
                            ("chart-series-error-direction", i),
                            crate::shell::i18n::text(key),
                            t == error.bar_type,
                        )
                        .on_click(
                            cx.listener(move |e, _, _, cx| e.set_sheet_chart_error_bar_type(t, cx)),
                        ),
                    );
                }
                let mut calc = SegmentControlRow::new(Mode::Proportional {
                    width: (body_width - px(132.)).max(px(124.)),
                });
                for (i, (key, t)) in ERROR_BAR_VAL_TYPES.iter().enumerate() {
                    let t = *t;
                    calc = calc.child(
                        SegmentItem::label(
                            ("chart-series-error-calculation", i),
                            crate::shell::i18n::text(key),
                            t == error.val_type,
                        )
                        .on_click(cx.listener(move |e, _, _, cx| {
                            e.set_sheet_chart_error_bar_val_type(t, cx)
                        })),
                    );
                }
                body = body
                    .child(types.label(crate::shell::i18n::text("chart.ui.error-direction")))
                    .child(calc.label(crate::shell::i18n::text("_xlsx.error-amount")))
                    .child(row(
                        "chart.ui.error-value",
                        chrome::text_field(&edit.error_value).spinner(
                            "chart-series-error-value",
                            chrome::SpinnerSpec::new(1.).min(0.),
                            cx,
                            |e, _, _, cx| e.commit_chart_series_editor(cx),
                        ),
                    ))
                    .child(
                        chrome::Checkbox::new("chart-series-error-end")
                            .label(crate::shell::i18n::text("_xlsx.end-cap"))
                            .checked(!error.no_end_cap)
                            .on_click(cx.listener(|e, on: &bool, _, cx| {
                                e.set_sheet_chart_error_bar_end_cap(*on, cx)
                            })),
                    );
            }
        }
        body = body.when_some(edit.error.clone(), |b, error| {
            b.child(div().text_color(cx.theme().danger).child(error))
        });
        let picking=self.sheet_range_pick_active();
        let actions=chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("chart-series-close", "", crate::shell::chrome::ActionButtonKind::Normal)
            .when(picking,|d|d.no_submit())
            .label(crate::shell::i18n::text("_shell.close"))
            .on_click(cx.listener(|e,_,w,cx|e.cancel_chart_series_editor(w,cx))));
        Some(SidebarPanel::with_footer(
            crate::shell::i18n::text("chart.ui.edit-series"),
            body.into_any_element(),
            actions,
        ))
    }
}

fn draft_axis_contains(chart: &Chart, index: usize, id: u32) -> bool {
    let mut at = 0;
    for g in &chart.groups {
        if index < at + g.series.len() {
            return g.axes.contains(&id);
        }
        at += g.series.len();
    }
    false
}

fn chart_kind_label(group: &ibis_doc_model::ChartGroup) -> &'static str {
    crate::pptx::chart_panel::ChartPreset::ALL
        .iter()
        .find(|(preset, _)| {
            let (kind, grouping) = preset.parts();
            let same = match (kind, group.kind) {
                (
                    ibis_doc_model::ChartType::Bar { horizontal: a, .. },
                    ibis_doc_model::ChartType::Bar { horizontal: b, .. },
                ) => a == b,
                (
                    ibis_doc_model::ChartType::Scatter { smooth: a },
                    ibis_doc_model::ChartType::Scatter { smooth: b },
                ) => a == b,
                (a, b) => std::mem::discriminant(&a) == std::mem::discriminant(&b),
            };
            same && group.grouping == grouping
        })
        .map(|(_, key)| *key)
        .unwrap_or(match group.kind {
            ibis_doc_model::ChartType::Radar { .. } => "_xlsx.radar",
            ibis_doc_model::ChartType::Stock { .. } => "fix.chart-edit.stock-chart-options",
            ibis_doc_model::ChartType::Surface { .. } => "fix.chart-edit.surface-type",
            ibis_doc_model::ChartType::OfPie { .. } => "fix.chart-edit.pie-chart-type",
            _ => "chart.ui.type",
        })
}

pub(crate) fn value_axis_label(chart: &Chart, secondary: bool) -> &'static str {
    let axis = if secondary {
        secondary_value_axis(chart)
    } else {
        value_axis(chart)
    };
    let side = axis.map(|i| chart.axes[i].side).or_else(|| {
        value_axis(chart).map(|i| match chart.axes[i].side {
            ibis_doc_model::chart::Side::Left => ibis_doc_model::chart::Side::Right,
            ibis_doc_model::chart::Side::Bottom => ibis_doc_model::chart::Side::Top,
            s => s,
        })
    });
    match side {
        Some(ibis_doc_model::chart::Side::Bottom) => "chart.ui.bottom-value-axis",
        Some(ibis_doc_model::chart::Side::Top) => "chart.ui.top-value-axis",
        Some(ibis_doc_model::chart::Side::Right) => "chart.ui.right-value-axis",
        _ => "chart.ui.left-value-axis",
    }
}
fn replace_range_axis(chart: &Chart, axes: &mut [u32], id: u32) {
    let vertical = value_axis(chart).is_some_and(|i| {
        matches!(
            chart.axes[i].side,
            ibis_doc_model::chart::Side::Left | ibis_doc_model::chart::Side::Right
        )
    });
    if let Some(slot) = axes.iter_mut().find(|id| {
        chart.axes.iter().any(|a| {
            a.id == **id
                && a.kind == AxisKind::Value
                && matches!(
                    a.side,
                    ibis_doc_model::chart::Side::Left | ibis_doc_model::chart::Side::Right
                ) == vertical
        })
    }) {
        *slot = id;
    }
}

/// Preserve at least one primary series, splitting a secondary group without losing data order.
fn ensure_primary_series_axis(chart: &mut Chart) {
    let Some(primary) = value_axis(chart).map(|i|chart.axes[i].id) else {return};
    if chart.groups.iter().any(|g|!g.series.is_empty() && g.axes.contains(&primary)) {return;}
    let Some(gi) = chart.groups.iter().position(|g|!g.series.is_empty()) else {return;};
    if chart.groups[gi].series.len()==1 {
        let mut axes=chart.groups[gi].axes.clone();
        replace_range_axis(chart,&mut axes,primary);chart.groups[gi].axes=axes;
    } else {
        let mut first=chart.groups[gi].clone();
        first.series=vec![chart.groups[gi].series.remove(0)];
        replace_range_axis(chart,&mut first.axes,primary);
        chart.groups.insert(gi,first);
    }
}
