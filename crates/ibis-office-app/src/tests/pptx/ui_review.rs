// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Behavioral checks for the live settings and shared table migration.
use gpui_kit::*;
use gpui_kit::component::{input::InputEvent, slider::SliderEvent};
use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Geometry, Shape, ShapeId, ShapeKind};
use crate::shell::scenario::{Scenario, button_probe as buttons};

pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("pptx::ui_review_sidebar_live_undo_save_focus",sidebar_live),
    ("pptx::ui_review_3d_live_numeric_slider",threed_live),
    ("pptx::ui_review_selection_table_drag_cancel_undo",selection_drag),
    ("pptx::ui_review_reading_table_drag_cancel_undo",reading_drag),
    ("pptx::ui_review_smartart_table_drag_undo",smartart_drag),
    ("pptx::ui_review_slide_dimensions_keep_exact_emu",slide_dimensions),
]}
fn fixture()->(Scenario,Vec<ShapeId>){
    fixture_from(Scenario::new_document(ibis_pptx::Kind::Presentation))
}
fn fixture_from(mut s:Scenario)->(Scenario,Vec<ShapeId>){
    s.size(1200.,900.);
    let ids=s.update(|e,_,cx|{
        let mut ids=Vec::new();
        for i in 0..3 {
            let id=e.document.ids().new_shape_id();
            let shape=Shape::new(id,EmuRect::new(500_000+i*1_000_000,500_000,800_000,600_000),ShapeKind::Auto(AutoShape::new(Geometry::Rect,Fill::Solid(Color::BLACK),None)));
            e.insert_shape(shape,cx);ids.push(id);
        }
        e.sidebar_open=true;cx.notify();ids
    });(s,ids)
}
fn sidebar_live(){
    // Saving an existing package exercises the supported save path; a new unsourced
    // presentation cannot currently be saved by this driver.
    let(mut s,ids)=fixture_from(Scenario::open("test/editing/pptx-plan.pptx"));let id=ids[2];
    let field=s.with(|e,_|e.pptx().unwrap().sidebar_settings.as_ref().unwrap().fields[0].clone());
    let entity=field.entity_id();let original=s.with(|e,_|e.slide().shape(id).unwrap().frame.width);
    s.update(|_,w,cx|field.update(cx,|f,cx|{f.focus(w,cx);f.set_value("12.5",w,cx);cx.emit(InputEvent::Change);}));
    assert_eq!(s.with(|e,_|e.slide().shape(id).unwrap().frame.width),4_500_000);
    assert_eq!(s.with(|e,_|e.pptx().unwrap().sidebar_settings.as_ref().unwrap().fields[0].entity_id()),entity,"live updates retain the input entity");
    assert!(s.update(|_,w,cx|field.read(cx).focus_handle(cx).is_focused(w)),"live updates retain input focus");
    s.update(|e,w,cx|e.focus_handle.focus(w,cx));s.key("cmd-z");
    assert_eq!(s.with(|e,_|e.slide().shape(id).unwrap().frame.width),original,"one Undo restores the live edit");
    assert!((s.with(|_,cx|field.read(cx).value().trim().parse::<f64>().unwrap()) - original as f64/360_000.).abs() < 0.0001);
    s.update(|e,w,cx|{let f=e.pptx().unwrap().sidebar_settings.as_ref().unwrap().fields[5].clone();f.update(cx,|f,cx|{f.set_value("336699",w,cx);cx.emit(InputEvent::Change);});});
    assert!(s.with(|e,_|matches!(&e.slide().shape(id).unwrap().kind,ShapeKind::Auto(a)if a.fill==Fill::Solid(Color::opaque(ibis_doc_model::Rgb(0x336699))))));
    let saved_count=s.with(|e,_|e.slide().shapes.len());
    s.update(|e,_,cx|{let slide=e.current_slide;e.run(vec![ibis_doc_model::Command::SetShapeName {slide,shape:id,name:"UI live sidebar sample".into()}],cx);});
    s.save_as("live-sidebar.pptx");let loaded=s.reopen();
    let (saved_slide,saved_id)=loaded.slides.iter().find_map(|slide|slide.shape_names.iter().find(|(_,name)|name.as_str()=="UI live sidebar sample").map(|(id,_)|(slide,*id))).expect("the added sample survives save/reopen");
    assert_eq!(saved_slide.shapes.len(),saved_count,"pre-existing shapes survive the settings edit");
    assert!(matches!(&saved_slide.shape(saved_id).unwrap().kind,ShapeKind::Auto(a)if a.fill==Fill::Solid(Color::opaque(ibis_doc_model::Rgb(0x336699)))));
    s.update(|e,_,cx|{e.set_selection(Vec::new());cx.notify();});
    let slide_field=s.with(|e,_|e.pptx().unwrap().sidebar_settings.as_ref().unwrap().fields[0].clone());
    assert_ne!(slide_field.entity_id(),entity,"selection changes rebuild the settings model");
    s.update(|_,w,cx|slide_field.update(cx,|f,cx|{f.set_value("30",w,cx);cx.emit(InputEvent::Change);}));
    assert_eq!(s.with(|e,_|e.presentation().slide_size.width),10_800_000);
}
fn threed_live(){
    assert!(crate::shell::icons::icon_svg_bytes("arrow_left").is_some(), "pan arrow must have bundled SVG data");
    assert!(crate::shell::icons::icon_svg_bytes("menu").is_some(), "row handles must have bundled SVG data");
    let(mut s,ids)=fixture();let id=ids[2];s.update(|e,w,cx|e.open_3d_model_panel(w,cx));
    let fields=s.with(|e,_|e.pptx().unwrap().threed_panel.as_ref().unwrap().rotation.clone());
    s.update(|_,w,cx|fields[0].update(cx,|f,cx|{f.set_value("45",w,cx);cx.emit(InputEvent::Change);}));
    assert_eq!(s.with(|e,_|e.selected_3d_model_rotation_deg().0),45);
    let sliders=s.with(|e,_|e.pptx().unwrap().threed_panel.as_ref().unwrap().sliders.clone());
    s.update(|_,w,cx|sliders[0].update(cx,|slider,cx|{slider.set_value(150.,w,cx);cx.emit(SliderEvent::Change(150_f32.into()));}));
    assert_eq!(s.with(|e,_|e.selected_3d_model_pan_and_zoom().2),150);
    s.update(|_,w,cx|sliders[1].update(cx,|slider,cx|{slider.set_value(4.,w,cx);cx.emit(SliderEvent::Change(4_f32.into()));}));
    assert_eq!(s.with(|e,_|e.slide().shape(id).unwrap().frame.width),1_440_000);
}
fn selection_drag(){table_drag(false)}
fn reading_drag(){table_drag(true)}
fn table_drag(reading:bool){
    let _probe=buttons::Probe::new();let(mut s,_)=fixture();
    s.update(|e,_,cx|{if reading {e.open_reading_order_pane(cx);}else{e.toggle_selection_pane(cx);}});
    let(state,order)=s.with(|e,_|{
        let p=e.pptx().unwrap();let state=if reading {p.reading_order_pane.as_ref().unwrap().reorder.clone()}else{p.selection_pane.as_ref().unwrap().reorder.clone()};
        (state,e.slide().shapes.iter().map(|s|s.id).collect::<Vec<_>>())
    });
    let from_id=*state.borrow().order.first().unwrap();let to_id=*state.borrow().order.last().unwrap();
    let first=state.borrow().row_bounds(from_id).unwrap();let last=state.borrow().row_bounds(to_id).unwrap();
    let from=point(first.origin.x+px(70.),first.center().y);let to=point(from.x,last.center().y+px(3.));
    s.dispatch(vec![PlatformInput::MouseDown(MouseDownEvent {button:MouseButton::Left,position:from,click_count:1,..Default::default()}),PlatformInput::MouseMove(MouseMoveEvent {position:to,pressed_button:Some(MouseButton::Left),..Default::default()})]);
    assert_eq!(state.borrow().dragging,Some(from_id));
    assert_eq!(s.with(|e,_|e.slide().shapes.iter().map(|s|s.id).collect::<Vec<_>>()),order,"drag remains cancellable until release");
    s.key("escape");s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent {button:MouseButton::Left,position:to,click_count:1,..Default::default()})]);
    assert!(state.borrow().dragging.is_none());
    assert_eq!(s.with(|e,_|e.slide().shapes.iter().map(|s|s.id).collect::<Vec<_>>()),order);
    s.drag((f32::from(from.x),f32::from(from.y)),(f32::from(to.x),f32::from(to.y)));
    assert_ne!(s.with(|e,_|e.slide().shapes.iter().map(|s|s.id).collect::<Vec<_>>()),order);
    s.update(|e,w,cx|e.focus_handle.focus(w,cx));s.key("cmd-z");
    assert_eq!(s.with(|e,_|e.slide().shapes.iter().map(|s|s.id).collect::<Vec<_>>()),order,"one Undo restores the entire reorder");
    if !reading {
        buttons::click(&mut s,"sel-all-eye");assert!(s.with(|e,_|e.slide().shapes.iter().all(|shape|e.slide().hidden_shapes.contains(&shape.id))));
        buttons::click(&mut s,"sel-all-eye");assert!(s.with(|e,_|e.slide().hidden_shapes.is_empty()));
    }
}

fn smartart_drag(){
    let(mut s,_)=fixture();
    s.update(|e,_,cx|{
        e.insert_smartart_with_items(crate::pptx::SmartArtKind::BasicBlockList, &["One","Two","Three"], None, cx);
        e.toggle_smartart_text_pane(cx);
    });
    let before=s.with(|e,_|e.slide().clone());
    let state=s.with(|e,_|e.pptx().unwrap().smartart_reorder.clone());
    let first=state.borrow().row_bounds(0).unwrap();let last=state.borrow().row_bounds(2).unwrap();
    let from=(f32::from(first.right()-px(12.)),f32::from(first.center().y));
    let to=(from.0,f32::from(last.center().y+px(3.)));
    s.drag(from,to);
    let items=s.with(|e,_|e.selected_smartart_data().unwrap().4);
    assert_eq!(items,vec!["Two","Three","One"]);
    s.update(|e,w,cx|e.focus_handle.focus(w,cx));s.key("cmd-z");
    assert_eq!(s.with(|e,_|e.slide().clone()),before,"one Undo restores all SmartArt nodes and their order");
}

fn slide_dimensions(){
    let mut s=Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e,_,cx|{e.run(vec![ibis_doc_model::Command::SetSlideSize {size:ibis_doc_model::EmuSize::new(12_192_007,6_858_011)}],cx);});
    let exact=s.with(|e,_|e.presentation().slide_size);
    s.update(|e,w,cx|e.open_slide_size_panel(w,cx));
    s.update(|e,w,cx|e.apply_slide_size_panel(w,cx));
    assert_eq!(s.with(|e,_|e.presentation().slide_size),exact,"unchanged display fields retain exact imported EMU");
    s.update(|e,w,cx|{e.open_slide_size_panel(w,cx);let width=e.pptx().unwrap().size_panel.as_ref().unwrap().width.clone();width.update(cx,|f,cx|f.set_value("42.5",w,cx));e.apply_slide_size_panel(w,cx);});
    let changed=s.with(|e,_|e.presentation().slide_size);
    assert_eq!(changed.width,15_300_000);
    assert_eq!(changed.height,exact.height,"editing width leaves the unedited height exact");
}
