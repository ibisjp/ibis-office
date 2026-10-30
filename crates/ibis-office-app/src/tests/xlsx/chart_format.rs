// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use crate::shell::scenario::{Scenario, button_probe as buttons};
use ibis_doc_model::{Chart, Color, Fill, Rgb, ShapeKind};
fn open() -> Scenario {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    let i = s
        .with(|e, _| {
            e.slide()
                .shapes
                .iter()
                .position(|s| matches!(s.kind, ShapeKind::Chart(_)))
        })
        .unwrap();
    let at = s.shape_position(i);
    s.click(at);
    s.menu("xlsx.chart-format");
    s
}
fn actual(s: &mut Scenario) -> Chart {
    s.with(|e, _| {
        e.slide().shapes.iter().find_map(|s| {
            if let ShapeKind::Chart(c) = &s.kind {
                Some((**c).clone())
            } else {
                None
            }
        })
    })
    .unwrap()
}
fn tab(s: &mut Scenario, id: &str, i: usize) {
    let key = buttons::all()
        .into_iter()
        .find(|k| k.contains(id) && k.ends_with(&format!(", {i})")))
        .unwrap_or_else(|| panic!("tab {id}/{i}: {:?}", buttons::all()));
    buttons::click(s, &key);
}
pub(crate) fn live() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    let before = actual(&mut s);
    assert_eq!(s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().tab), 0);
    tab(&mut s, "chart-format-tab", 1);
    tab(&mut s, "chart-format-tab", 2);
    tab(&mut s, "chart-format-tab", 0);
    let edit = buttons::all()
        .into_iter()
        .find(|k| k.contains("chart-series-edit") && k.ends_with(", 0)"))
        .unwrap();
    buttons::click(&mut s, &edit);
    s.update(|e, _, cx| {
        e.set_sheet_chart_series_fill(Fill::Solid(Color::opaque(Rgb(0x22aa44))), cx)
    });
    let changed=actual(&mut s);
    assert_ne!(changed,before,"formatting changes the document while the editor is open");
    assert!(!buttons::all().iter().any(|id|["chart-series-ok","chart-series-cancel","chart-series-remove"].contains(&id.as_str())));
    buttons::click(&mut s,"chart-series-close");
    assert_eq!(actual(&mut s),changed,"Close retains live changes");
    s.key("cmd-z");assert_eq!(actual(&mut s),before,"Undo reverts a live property change");
    s.update(|e, w, cx| {
        e.open_chart_series_editor(Some(0), w, cx);
        e.set_chart_ui_type(crate::pptx::chart_panel::ChartPreset::Line, w, cx);
    });
    tab(&mut s, "chart-series-tab", 1);
    s.update(|e, w, cx| {
        let p = e
            .wb()
            .chart_edit
            .as_ref()
            .unwrap()
            .series_editor
            .as_ref()
            .unwrap();
        let input = p.fill.clone();
        input.update(cx, |i, cx| {i.set_value("22AA44", w, cx);cx.emit(gpui_kit::component::input::InputEvent::Change);});
        // A name cell above the values names the series (the first-cell checkbox is on and the name is read-only): a typed name needs the checkbox off.
        assert!(p.header, "the series is named by the cell above its values");
        e.toggle_chart_series_header(false, w, cx);
        let p = e.wb().chart_edit.as_ref().unwrap().series_editor.as_ref().unwrap();
        let fields = [
            (p.name.clone(), "April revenue"),
            (p.stroke.clone(), "556677"),
            (p.line_width.clone(), "2.5"),
            (p.marker_size.clone(), "9"),
        ];
        for (input, value) in fields {
            input.update(cx, |i, cx| {i.set_value(value, w, cx);cx.emit(gpui_kit::component::input::InputEvent::Change);});
        }
        e.set_sheet_chart_marker_symbol(ibis_doc_model::chart::MarkerSymbol::Triangle, cx);
        e.set_sheet_chart_labels_position(Some(Some(ibis_doc_model::LabelPosition::Above)), cx);
        e.set_sheet_chart_trendline_type(Some(ibis_doc_model::chart::TrendlineType::Linear), cx);
        e.set_sheet_chart_trendline_disp_eq(true, cx);
        e.set_sheet_chart_error_bars_shown(true, cx);
    });
    let live=actual(&mut s);assert_series(&live);
    buttons::click(&mut s, "chart-series-close");
    assert!(
        s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().series_editor.is_none()),
        "Close closes editor"
    );
    let after = actual(&mut s);
    assert_series(&after);
    assert_eq!(
        selected_series(&after).fill,
        Fill::Solid(Color::opaque(Rgb(0x22aa44)))
    );
    let a = after
        .groups
        .iter()
        .flat_map(|g| &g.series)
        .filter(|s| s.name != "April revenue")
        .collect::<Vec<_>>();
    let b = before
        .groups
        .iter()
        .flat_map(|g| &g.series)
        .skip(1)
        .collect::<Vec<_>>();
    assert_eq!(a, b, "other series unchanged");
    s.key("cmd-z");assert_ne!(actual(&mut s),after,"live edits have Undo history");
    for _ in 0..32 {if actual(&mut s)==before {break;}s.key("cmd-z");}
    assert_eq!(actual(&mut s),before,"Undo restores all live property edits");
    for _ in 0..32 {if actual(&mut s)==after {break;}s.key("cmd-shift-z");}
    assert_eq!(actual(&mut s),after);
    s.save_as("chart-format-transaction.xlsx");
    let p = s.reopen();
    let saved = p.slides[0]
        .shapes
        .iter()
        .find_map(|s| {
            if let ShapeKind::Chart(c) = &s.kind {
                Some(c)
            } else {
                None
            }
        })
        .unwrap();
    assert_series(saved);
    // A line is painted by its stroke; the reader canonically discards an unused series fill.
    assert_eq!(selected_series(saved).fill, Fill::None);
}
fn selected_series(chart: &Chart) -> &ibis_doc_model::Series {
    chart
        .groups
        .iter()
        .flat_map(|g| &g.series)
        .find(|s| s.name == "April revenue")
        .expect("edited series")
}
fn assert_series(chart: &Chart) {
    let s = selected_series(chart);
    assert_eq!(s.name, "April revenue");
    let stroke = s.stroke.as_ref().expect("series stroke");
    assert_eq!(stroke.width, 31750);
    assert_eq!(stroke.color.rgb, Rgb(0x556677));
    let marker = s.marker.as_ref().expect("series marker");
    assert_eq!(marker.symbol, ibis_doc_model::chart::MarkerSymbol::Triangle);
    assert_eq!(marker.size, 9);
    assert_eq!(
        s.labels.as_ref().unwrap().position,
        Some(ibis_doc_model::LabelPosition::Above)
    );
    assert_eq!(
        s.trendline.as_ref().unwrap().kind,
        ibis_doc_model::chart::TrendlineType::Linear
    );
    assert!(s.trendline.as_ref().unwrap().disp_eq);
    assert!(s.error_bars.is_some());
}
pub(crate) fn screens() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    s.screenshot("chart-format-series");
    tab(&mut s, "chart-format-tab", 1);
    s.screenshot("chart-format-axes");
    tab(&mut s, "chart-format-tab", 2);
    s.screenshot("chart-format-overall");
    s.update(|e, w, cx| {
        e.open_chart_series_editor(Some(0), w, cx);
        e.set_chart_ui_type(crate::pptx::chart_panel::ChartPreset::Line, w, cx);
    });
    s.screenshot("chart-format-edit-data");
    tab(&mut s, "chart-series-tab", 1);
    s.screenshot("chart-format-edit-display");
    s.update(|e, _, cx| {
        let mut bounds = e.panel_dialog.bounds.unwrap();
        bounds.size.width = gpui_kit::px(320.);
        bounds.origin.x = gpui_kit::px(880.);
        e.panel_dialog.bounds = Some(bounds);
        e.panel_dialog.user_resized = true;
        cx.notify();
    });
    let narrow=s.with(|e,_|e.panel_dialog.bounds.unwrap());
    s.wheel((f32::from(narrow.origin.x+narrow.size.width/2.),f32::from(narrow.origin.y)+90.),-600.);
    s.screenshot("chart-format-edit-display-320");
    s.update(|e, _, cx| {
        let mut bounds = e.panel_dialog.bounds.unwrap();
        e.panel_dialog.scroll.set_offset(Default::default());
        bounds.size.width = gpui_kit::px(550.);
        bounds.origin.x = gpui_kit::px(650.);
        e.panel_dialog.bounds = Some(bounds);
        e.panel_dialog.user_resized = false;
        cx.notify();
    });

    tab(&mut s, "chart-series-tab", 2);
    s.screenshot("chart-format-edit-analysis");
    tab(&mut s, "chart-series-tab", 0);
    buttons::click(&mut s, "chart-series-values");
    s.screenshot("chart-format-range-pick");
    s.key("escape");
    s.update(|e, _, cx| {
        e.set_sheet_chart_trendline_type(
            Some(ibis_doc_model::chart::TrendlineType::Polynomial),
            cx,
        );
        e.set_sheet_chart_error_bars_shown(true, cx);
    });
    tab(&mut s, "chart-series-tab", 2);
    s.screenshot("chart-format-edit-analysis-expanded");
    s.update(|e, w, cx| {
        e.cancel_chart_series_editor(w, cx);
        e.wb_mut().chart_edit.as_mut().unwrap().tab = 1;
        e.wb_mut().chart_edit.as_mut().unwrap().axis_tab = 1;
    });
    s.screenshot("chart-format-left-axis");
    s.update(|e, _, cx| {
        let p = e.wb_mut().chart_edit.as_mut().unwrap();
        p.tab = 2;
        p.detail_open[2] = true;
        cx.notify();
    });
    s.screenshot("chart-format-overall-expanded");
}
pub(crate) fn reorder() {
    use gpui_kit::*;
    let _probe = buttons::Probe::new();
    let mut s = open();
    let before = actual(&mut s);
    let row = s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().series_reorder.clone());
    let start = row.borrow().row_bounds(0).unwrap();
    let end = row.borrow().row_bounds(2).unwrap();
    let from = point(
        start.origin.x + px(15.),
        start.origin.y + start.size.height / 2.,
    );
    let to = point(from.x, end.origin.y + end.size.height / 2. + px(1.));
    let down = PlatformInput::MouseDown(MouseDownEvent {
        button: MouseButton::Left,
        position: from,
        click_count: 1,
        ..Default::default()
    });
    let mov = PlatformInput::MouseMove(MouseMoveEvent {
        position: to,
        pressed_button: Some(MouseButton::Left),
        ..Default::default()
    });
    s.dispatch(vec![down.clone(), mov.clone()]);
    assert_eq!(row.borrow().order, vec![1, 2, 0]);
    s.key("escape");
    assert_eq!(actual(&mut s), before);
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent {
        button: MouseButton::Left,
        position: to,
        click_count: 1,
        ..Default::default()
    })]);
    s.dispatch(vec![
        down,
        mov,
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: to,
            click_count: 1,
            ..Default::default()
        }),
    ]);
    let names = |c: &Chart| {
        c.groups
            .iter()
            .flat_map(|g| &g.series)
            .map(|s| s.name.clone())
            .collect::<Vec<_>>()
    };
    let mut expected = names(&before);
    let first = expected.remove(0);
    expected.push(first);
    assert_eq!(names(&actual(&mut s)), expected);
    s.save_as("chart-series-reorder.xlsx");
    let saved = s.reopen();
    let chart = saved.slides[0]
        .shapes
        .iter()
        .find_map(|s| {
            if let ShapeKind::Chart(c) = &s.kind {
                Some(c)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(names(chart), expected);
    s.key("cmd-z");
    assert_eq!(actual(&mut s), before);
}

pub(crate) fn choices() {
    use crate::pptx::chart_panel::ChartPreset;
    use crate::xlsx::chart_type_compatibility::restriction;
    let _probe = buttons::Probe::new();
    let mut s = open();
    let before = actual(&mut s);
    assert!(restriction(&before, Some(0), ChartPreset::Line).is_none());
    assert_eq!(
        restriction(&before, Some(0), ChartPreset::Bar),
        Some("chart.ui.type-orientation")
    );
    assert_eq!(
        restriction(&before, Some(0), ChartPreset::Pie),
        Some("chart.ui.type-family")
    );
    let mut three = before.clone();
    three.groups[0].three_d = Some(Default::default());
    assert_eq!(
        restriction(&three, Some(0), ChartPreset::Line),
        Some("chart.ui.type-three-d")
    );
    let mut round = before.clone();
    round.groups[0].kind = ibis_doc_model::ChartType::Pie { first_angle: 0 };
    assert_eq!(
        restriction(&round, Some(0), ChartPreset::Doughnut),
        Some("chart.ui.type-round-mixed")
    );
    s.update(|e, w, cx| e.open_chart_series_editor(Some(0), w, cx));
    assert!(
        !ibis_doc_model::ChartType::Bar {
            horizontal: false,
            gap_width: 150,
            overlap: 0
        }
        .supports_markers()
    );
    assert!(ibis_doc_model::ChartType::Line.supports_markers());
    assert!(!ibis_doc_model::ChartType::Pie { first_angle: 0 }.supports_series_analysis());
    s.update(|e, _, cx| {
        e.set_sheet_chart_marker_symbol(ibis_doc_model::chart::MarkerSymbol::Triangle, cx)
    });
    assert!(
        s.with(|e, _| e.sheet_chart().unwrap().1.groups[0].series[0]
            .marker
            .is_none()),
        "CT_BarSer must not acquire c:marker"
    );
    let entries = s.with(|e, _| e.chart_type_menu_entries());
    let enabled = |key| {
        entries
            .iter()
            .find_map(|entry| {
                if let crate::shell::chrome::MenuEntry::Item(i) = entry {
                    (i.id == key).then_some(i.enabled())
                } else {
                    None
                }
            })
            .unwrap()
    };
    assert!(enabled("chart.type.line"));
    assert!(!enabled("chart.type.bar"));
    assert!(!enabled("chart.type.pie"));
    s.update(|e, w, cx| e.set_chart_ui_type(ChartPreset::Bar, w, cx));
    assert_eq!(
        s.with(|e, _| e.sheet_chart().unwrap().1.groups[0].kind),
        before.groups[0].kind,
        "a disallowed direct command must also preserve the draft"
    );
    s.update(|e, w, cx| e.cancel_chart_series_editor(w, cx));
    assert_eq!(actual(&mut s), before);
}

/// Pointer-driven single-cell name picking, axis persistence and confirmation ownership.
pub(crate) fn references_and_modal_priority() {
    use gpui_kit::*;
    let _probe=buttons::Probe::new();
    let mut s=open();
    s.update(|e,w,cx|e.open_chart_series_editor(Some(0),w,cx));
    let full=s.with(|e,_|e.panel_dialog.bounds.unwrap());
    let input=s.with(|e,_|e.wb().chart_edit.as_ref().unwrap().series_editor.as_ref().unwrap().name.clone());
    // The name cell above the values is the series' first-cell name: the checkbox is on and the field shows the name.
    assert!(s.with(|e,_|e.wb().chart_edit.as_ref().unwrap().series_editor.as_ref().unwrap().header),"existing series keeps its name cell as the first-cell name");
    assert!(!s.with(|_,cx|input.read(cx).value().is_empty()));
    // Turning the checkbox off lets the name be picked from a single cell by pointer.
    s.update(|e,w,cx|e.toggle_chart_series_header(false,w,cx));
    for drag in [false,true] {
        buttons::click(&mut s,"chart-series-name");
        assert_eq!(s.with(|e,_|e.panel_dialog.bounds.unwrap().size.width),px(320.));
        let from=s.with(|e,_|e.cell_center(1,2).unwrap());
        let to=s.with(|e,_|e.cell_center(3,3).unwrap());
        if drag { s.drag((from.x.into(),from.y.into()),(to.x.into(),to.y.into())); }
        else {s.click((from.x.into(),from.y.into()));}
        assert!(!s.with(|e,_|e.sheet_range_pick_active()),"one completed pointer gesture accepts the name cell");
        assert_eq!(s.with(|e,_|e.panel_dialog.bounds.unwrap().size.width),full.size.width);
        let value=s.with(|_,cx|input.read(cx).value().to_string());
        assert!(value.starts_with('=') && value.contains("$B$3"),"name formula: {value}");
        assert!(!value.contains("$D$4"),"drag keeps its starting cell");
    }
    s.update(|e,w,cx|e.apply_chart_series_editor(w,cx));
    assert!(!actual(&mut s).groups[0].series[0].name.starts_with('='));
    // Assign every current series to the secondary axis. The first must stay primary at commit.
    let count=actual(&mut s).groups.iter().map(|g|g.series.len()).sum::<usize>();
    for _ in 0..count {
        s.update(|e,w,cx|{
            let c=e.sheet_chart().unwrap().1;
            let primary=crate::xlsx::chart_edit::value_axis(c).map(|i|c.axes[i].id).unwrap();
            let i=c.groups.iter().flat_map(|g|g.series.iter().map(move |_|g.axes.contains(&primary))).position(|left|left).unwrap();
            e.open_chart_series_editor(Some(i),w,cx);
            e.wb_mut().chart_edit.as_mut().unwrap().series_editor.as_mut().unwrap().secondary=true;
            e.apply_chart_series_editor(w,cx);
        });
    }
    let chart=actual(&mut s);
    let primary=crate::xlsx::chart_edit::value_axis(&chart).unwrap();
    let secondary=crate::xlsx::chart_edit::secondary_value_axis(&chart).unwrap();
    assert!(chart.groups.iter().any(|g|!g.series.is_empty()&&g.axes.contains(&chart.axes[primary].id)));
    assert!(chart.groups[0].axes.contains(&chart.axes[primary].id),"first series is forced to primary when all would be secondary");
    assert!(!chart.axes[secondary].deleted);
    assert_eq!(chart.axes[secondary].crosses,Some(ibis_doc_model::AxisCrosses::Maximum));
    s.save_as("chart-reference-axis-policy.xlsx");
    let reopened=s.reopen();
    let saved=reopened.slides.iter().flat_map(|s|&s.shapes).find_map(|s|if let ShapeKind::Chart(c)=&s.kind{Some(c)}else{None}).unwrap();
    assert!(saved.axes.iter().any(|a|a.side==ibis_doc_model::chart::Side::Right&&!a.deleted));
    s.update(|e,w,cx|e.open_chart_series_editor(Some(0),w,cx));
    buttons::click(&mut s,"chart-series-name");
    s.update(|e,_,cx|e.ask_to_save(false,cx));
    assert_eq!(s.with(|e,_|e.interaction_layer()),crate::shell::panel_dialog::InteractionLayer::Save);
    let covered=s.with(|_,cx|input.read(cx).value().to_string());
    s.type_text("covered input must stay unchanged");
    assert_eq!(s.with(|_,cx|input.read(cx).value().to_string()),covered,"confirmation owns native text input too");
    s.screenshot("chart-save-confirm-over-range");
    buttons::click(&mut s,"save-confirm-cancel");
    assert!(!s.with(|e,_|e.save_confirm_dialog_open),"front confirmation button must receive the click");
    assert!(s.with(|e,_|e.sheet_range_pick_active()),"lower session must not consume confirmation input");
    s.key("escape");
    s.update(|e,_,cx|e.show_panel_message("range validation".into(),cx));
    buttons::click(&mut s,"panel-message-ok");
    assert!(s.with(|e,_|e.panel_message.is_none()));
}

pub(crate) fn drag_stays_in_table() {
    use gpui_kit::*;
    let _probe=buttons::Probe::new();let mut s=open();
    let state=s.with(|e,_|e.wb().chart_edit.as_ref().unwrap().series_reorder.clone());
    let first=state.borrow().row_bounds(0).unwrap();
    let last=state.borrow().row_bounds(2).unwrap();
    let from=point(first.origin.x+px(15.),first.origin.y+first.size.height/2.);
    s.dispatch(vec![PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position:from,click_count:1,..Default::default()})]);
    for y in [last.bottom()+px(200.),first.origin.y-px(200.)] {
        s.dispatch(vec![PlatformInput::MouseMove(MouseMoveEvent{position:point(from.x,y),pressed_button:Some(MouseButton::Left),..Default::default()})]);
        let lifted=state.borrow().row_bounds(0).unwrap();
        assert!(lifted.origin.y>=first.origin.y-px(0.5),"drag above table: {lifted:?}");
        assert!(lifted.bottom()<=last.bottom()+px(0.5),"drag below table: {lifted:?}");
    }
    s.key("escape");
}

pub(crate) fn section_reveal_and_confirm_buttons() {
    let _probe=buttons::Probe::new();
    let mut s=open();
    tab(&mut s,"chart-format-tab",2);
    s.update(|e,_,cx|{let scroll=&e.sidebar_scroll;scroll.set_offset(gpui_kit::point(gpui_kit::px(0.),-scroll.max_offset().y));cx.notify();});
    let state=s.with(|e,_|e.wb().chart_edit.as_ref().unwrap().detail_state[2].clone());
    let header=state.bounds.get().unwrap();
    let before=s.with(|e,_|e.sidebar_scroll.offset().y);
    s.click((f32::from(header.origin.x+gpui_kit::px(70.)),f32::from(header.origin.y+gpui_kit::px(17.))));
    assert!(s.with(|e,_|e.wb().chart_edit.as_ref().unwrap().detail_open[2]));
    s.wait(30);
    let during=s.with(|e,_|e.sidebar_scroll.offset().y);
    for _ in 0..12 {s.wait(20);}
    let after=s.with(|e,_|e.sidebar_scroll.offset().y);
    let section=state.bounds.get().unwrap();
    let viewport=s.with(|e,_|e.sidebar_scroll.bounds());
    assert!(after<before,"expansion must reveal the newly inserted content");
    assert!(during>after,"scrolling advances through intermediate positions");
    assert!(section.bottom()<=viewport.bottom()+gpui_kit::px(1.),"expanded contents are visible: {section:?} / {viewport:?}");
    // Invalid ranges are blocking messages, rather than easily missed pane text.
    s.update(|e,w,cx|{let input=e.wb().chart_edit.as_ref().unwrap().range.clone();input.update(cx,|i,cx|i.set_value("A1",w,cx));e.set_sheet_chart_data(false,w,cx);});
    assert!(s.with(|e,_|e.panel_message.is_some()));
    s.screenshot("chart-invalid-range-message");
    buttons::click(&mut s,"panel-message-ok");
    drop(s);
    for save in [false,true] {
        let mut q=open();
        q.save_as(if save {"confirm-save-copy.xlsx"}else{"confirm-discard-copy.xlsx"});
        q.update(|e,w,cx|e.open_chart_series_editor(Some(0),w,cx));
        buttons::click(&mut q,"chart-series-values");
        q.update(|e,_,cx|e.ask_to_save(false,cx));
        buttons::click(&mut q,if save {"save-confirm-save"}else{"save-confirm-discard"});
        assert!(!q.with(|e,_|e.save_confirm_dialog_open),"front confirmation action must finish");
        assert!(!q.with(|e,_|e.has_document),"front action closes the document, not the underlying settings");
    }
}

pub(crate) fn live_title_styles_and_layout() {
    use gpui_kit::Focusable;
    use crate::xlsx::chart_style::{CHART_STYLES,ChartStylePreset};
    let _probe=buttons::Probe::new();let mut s=open();tab(&mut s,"chart-format-tab",2);
    let title=s.with(|e,_|e.wb().chart_edit.as_ref().unwrap().title.clone());
    // No Enter/Blur: each value change is reflected while the same field keeps focus.
    for value in ["R","Re","Revenue"] {
        s.update(|_,w,cx|title.update(cx,|input,cx|{input.focus_handle(cx).focus(w,cx);input.set_value(value,w,cx);cx.emit(gpui_kit::component::input::InputEvent::Change);}));
        assert_eq!(actual(&mut s).title.unwrap().text,value);
    }
    s.update(|e,_,cx|e.set_sheet_chart_title_shown(false,cx));assert!(actual(&mut s).title.is_none());
    s.update(|_,w,cx|title.update(cx,|input,cx|input.set_value("hidden draft",w,cx)));
    assert!(actual(&mut s).title.is_none(),"editing an inactive title must not turn it back on");
    s.update(|e,_,cx|e.set_sheet_chart_title_shown(true,cx));assert_eq!(actual(&mut s).title.unwrap().text,"hidden draft");
    // The legend is reachable from Overall, but absent from Axes.
    assert!(buttons::all().iter().any(|id|id.contains("chart-legend")));
    tab(&mut s,"chart-format-tab",1);buttons::clear();s.settle();
    assert!(!buttons::all().iter().any(|id|id.contains("chart-legend")));
    tab(&mut s,"chart-format-tab",2);
    let colors_before=actual(&mut s).groups.iter().flat_map(|g|&g.series).map(|series|series.fill.clone()).collect::<Vec<_>>();
    let mut previous=None;
    for (style_index,(_key,preset)) in CHART_STYLES.into_iter().enumerate() {
        // Actual searchable dropdown events apply portable formatting.
        buttons::click(&mut s,"chart-style-dropdown");
        if style_index>0 {s.key("down");}s.key("enter");
        let chart=actual(&mut s);assert!(preset.matches(&chart));
        let shot=s.screenshot(&format!("live-style-{preset:?}"));
        let image=image::open(shot).unwrap().to_rgba8();
        if let Some(before)=previous {assert_ne!(image,before,"each style has a visible effect");}
        previous=Some(image);
        assert_eq!(chart.groups.iter().flat_map(|g|&g.series).map(|series|series.fill.clone()).collect::<Vec<_>>(),colors_before,"style and palette are independent");
    }
    let width=buttons::bounds("chart-palette-dropdown").unwrap().size.width;
    // 2026-10-29: the colour trigger fills its row's control cell (its right edge is the data range field's;
    // checked in shell::ui20261029_chart_pane_right_edge), so it is never narrower than a standard field.
    assert!(width>=crate::shell::chrome::FIELD_WIDTH,"the trigger is at least the standard field width");
    buttons::click(&mut s,"chart-palette-dropdown");s.screenshot("live-palette-compact-popup");
    let choice=buttons::all().into_iter().find(|id|id.contains("searchable-item-chart-palette-dropdown")&&id.ends_with(", 1)")).unwrap();
    buttons::click(&mut s,&choice);
    s.save_as("live-title-style-palette.xlsx");
    let reopened=s.reopen();let chart=reopened.slides.iter().flat_map(|slide|&slide.shapes).find_map(|shape|if let ShapeKind::Chart(c)=&shape.kind{Some(c)}else{None}).unwrap();
    assert!(ChartStylePreset::Dark.matches(chart));assert_eq!(chart.title.as_ref().unwrap().text,"hidden draft");
    assert_eq!(chart.title.as_ref().unwrap().style.color,Some(Color::opaque(Rgb(0xe8eaed))));
    assert_eq!(chart.legend.as_ref().unwrap().text.color,Some(Color::opaque(Rgb(0xe8eaed))));
    assert!(chart.axes.iter().all(|axis|axis.text.color==Some(Color::opaque(Rgb(0xe8eaed)))));
    // A series' trend dropdown changes the visible document before closing the editor.
    s.update(|e,w,cx|e.open_chart_series_editor(Some(0),w,cx));
    tab(&mut s,"chart-series-tab",2);buttons::click(&mut s,"chart-series-trend");
    s.key("down");s.key("enter");
    assert!(actual(&mut s).groups.iter().flat_map(|g|&g.series).any(|series|series.trendline.is_some()));
    s.screenshot("live-series-analysis-dropdown");
    buttons::click(&mut s,"chart-series-close");
}
