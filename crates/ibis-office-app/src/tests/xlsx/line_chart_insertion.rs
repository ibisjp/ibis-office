// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A numeric first column and two value columns, selected through row 17.
//! IBIS_LINE_CHART_FILE can point at a private workbook for a local reproduction;
//! no source file is changed or copied into the public test data.
use crate::shell::{chrome::{MenuEntry, MenuId}, scenario::{Scenario, button_probe as buttons}};
use super::SheetSteps;
use gpui_kit::*;
use ibis_doc_model::{CellRange, ShapeKind};

fn workbook() -> Scenario {
    if let Ok(path)=std::env::var("IBIS_LINE_CHART_FILE") {return Scenario::open(&path);}
    let mut s=super::new_sheet();
    for (column,label) in [("A1","x"),("B1","y"),("C1","z")] {s.click_cell(column);s.enter(label,"enter");}
    for row in 2..=17 {
        for (column,value) in [("A",(row-1)as f64),("B",(row%7)as f64/7.),("C",(row%11)as f64/11.)] {
            s.click_cell(&format!("{column}{row}"));s.enter(&value.to_string(),"enter");
        }
    }
    s
}

fn open_chart_menu(s:&mut Scenario) {
    let (insert,down)=s.with(|e,_|{
        let menus=e.menus();let (index,(_,entries))=menus.menus.iter().enumerate().find(|(_, (id,_))|*id==MenuId::Insert).unwrap();
        let target=entries.iter().position(|v|matches!(v,MenuEntry::Submenu(sub)if sub.id=="insert.chart")).unwrap();
        (index,entries[..=target].iter().filter(|v|match v{MenuEntry::Item(item)=>item.enabled(),MenuEntry::Submenu(_)=>true,_=>false}).count())
    });
    // Open the actual menu; subsequent keyboard navigation opens the actual child
    // popup, and the final chart choice is a pointer click, not a handler call.
    s.hover((0.,0.));
    s.update(|e,w,cx|e.open_menu_at(insert,w,cx));
    for _ in 0..down{s.key("down");}s.key("right");
}

pub(crate) fn selected_numeric_columns() {
    let _probe=buttons::Probe::new();
    let mut s=workbook();s.size(1200.,720.);
    // The same range as the user's reproduction, selected by a real pointer drag.
    let (from,to)=s.with(|e,_|(e.cell_center(0,0).unwrap(),e.cell_center(2,16).unwrap()));
    s.dispatch(vec![PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position:from,click_count:1,..Default::default()}),PlatformInput::MouseMove(MouseMoveEvent{position:to,pressed_button:Some(MouseButton::Left),..Default::default()}),PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position:to,click_count:1,..Default::default()})]);
    assert_eq!(s.with(|e,_|e.selected_range()),Some(CellRange{first_row:0,last_row:16,first_column:0,last_column:2}));
    assert_eq!(s.menu_state("pptx.insert-chart.line"),Some((true,false)),"numeric columns must not disable line-chart insertion");
    let selected_before=s.with(|e,_|e.selected_range());
    let before=s.with(|e,_|e.slide().shapes.iter().filter(|s|matches!(s.kind,ShapeKind::Chart(_))).count());
    open_chart_menu(&mut s);
    let bounds=buttons::bounds("menu-item-pptx.insert-chart.line").unwrap_or_else(||panic!("line-chart row absent from actual child popup: {:?}",buttons::all()));
    if std::env::var_os("IBIS_LINE_CHART_SHOT").is_some(){s.screenshot("line-chart-menu-before-click");}
    eprintln!("line chart actual pointer click bounds: {bounds:?}");
    s.click((f32::from(bounds.center().x),f32::from(bounds.center().y)));
    let (count,index)=s.with(|e,_|{let indices=e.slide().shapes.iter().enumerate().filter(|(_,s)|matches!(s.kind,ShapeKind::Chart(_))).map(|(i,_)|i).collect::<Vec<_>>();(indices.len(),indices.last().copied())});
    assert_eq!(count,before+1,"the actual menu click must insert a chart");
    assert_eq!(s.with(|e,_|e.selected_range()),selected_before,"clicking the menu must not select worksheet cells underneath it");
    let index=index.unwrap();let center=s.shape_position(index);
    let (frame,workspace,offset,series)=s.with(|e,_|{let shape=&e.slide().shapes[index];let ShapeKind::Chart(chart)=&shape.kind else{unreachable!()};(shape.frame,e.workspace_bounds.unwrap(),e.workspace_scroll.offset(),chart.groups.iter().flat_map(|g|&g.series).map(|v|(v.name.clone(),v.values.len())).collect::<Vec<_>>())});
    eprintln!("inserted chart frame={frame:?} center={center:?} workspace={workspace:?} scroll={offset:?} series={series:?}");
    assert_eq!(series.len(),3);assert!(series.iter().all(|(_,n)|*n==16));
    if std::env::var_os("IBIS_LINE_CHART_SHOT").is_some(){s.screenshot("line-chart-numeric-columns");}

    // The overlay must still handle its own scrollbar, without consuming rows.
    open_chart_menu(&mut s);
    let track=buttons::bounds("submenu-scroll-track").expect("long submenu has a scrollbar");
    let at=point(track.center().x,track.bottom()-px(2.));
    buttons::clear();
    s.dispatch(vec![PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position:at,click_count:1,..Default::default()}),PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position:at,click_count:1,..Default::default()})]);
    assert!(buttons::bounds("menu-item-pptx.insert-chart.sunburst").is_some(),"scrollbar track brings the final chart choices into view");
    assert_eq!(s.with(|e,_|e.selected_range()),selected_before,"scrolling the menu must not select cells underneath");
    assert_eq!(s.with(|e,_|e.slide().shapes.iter().filter(|sh|matches!(sh.kind,ShapeKind::Chart(_))).count()),count,"scrollbar clicks must not activate a chart choice");
    s.click((f32::from(workspace.origin.x+px(5.)),f32::from(workspace.bottom()-px(30.))));
    assert!(s.with(|e,_|e.open_menu.is_none()),"outside click dismisses the menu");
    assert_eq!(s.with(|e,_|e.selected_range()),selected_before,"dismissing the menu must not click through to the worksheet");
}
