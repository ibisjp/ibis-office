// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The shared slider (value at its left, unit, a typed number, the primary track), the 3D model panel, the ink effects list and the fill panels.
use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Geometry, Shape, ShapeId, ShapeKind};
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::ext::*;

pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("pptx::slider_3d_panel_value_unit_typed_number",threed_slider),
    ("pptx::slider_ink_effects_dropdown",ink_effects),
    ("pptx::slider_fill_panels",fill_panels),
]}
fn fixture()->(Scenario,Vec<ShapeId>){
    let mut s=Scenario::new_document(ibis_pptx::Kind::Presentation);
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
fn center(id:&str)->(f32,f32){
    let b=buttons::bounds(id).unwrap_or_else(||panic!("{id} not drawn: {:?}",buttons::all()));
    (f32::from(b.origin.x+b.size.width/2.),f32::from(b.origin.y+b.size.height/2.))
}
fn threed_slider(){
    let _probe=buttons::Probe::new();
    let (mut s,ids)=fixture();
    s.update(|e,_,cx|{e.set_selection(vec![ids[2]]);cx.notify();});
    s.update(|e,w,cx|e.open_3d_model_panel(w,cx));s.settle();
    s.screenshot("3d-panel");
    // The value is at the slider's left, with its unit; the zoom of a new model is 100 %.
    let value=buttons::bounds("3d-zoom-value").expect("the zoom value is drawn");
    assert!(f32::from(value.size.width)>=f32::from(crate::shell::chrome::slider::VALUE_WIDTH)-1.);
    // Four pan arrows, each its own icon.
    assert!(crate::shell::icons::icon_svg_bytes("arrow_back").is_some());
    // A click on the number turns it into a field; a typed number sets the slider (clamped to 50..300) and sends the change.
    s.click(center("3d-zoom-value"));s.settle();
    assert!(buttons::bounds("3d-zoom-field").is_some(),"the number became a field");
    s.screenshot("3d-panel-editing");
    s.type_text("200");s.key("enter");s.settle();
    assert_eq!(s.with(|e,_|e.selected_3d_model_pan_and_zoom().2),200);
    s.click(center("3d-zoom-value"));s.settle();
    s.type_text("9999");s.key("enter");s.settle();
    assert_eq!(s.with(|e,_|e.selected_3d_model_pan_and_zoom().2),300,"clamped to the slider's end");
    s.click(center("3d-width-value"));s.settle();
    s.type_text("3.5");s.key("enter");s.settle();
    assert_eq!(s.with(|e,_|e.slide().shape(ids[2]).unwrap().frame.width),1_260_000,"3.5 cm");
    s.screenshot("3d-panel-after");
}
fn ink_effects(){
    let _probe=buttons::Probe::new();
    let (mut s,_)=fixture();
    s.update(|e,w,cx|e.open_pptx_ink_panel(w,cx));s.settle();
    s.update(|e,_,cx|e.select_pptx_ink_pen(crate::shell::ink::PenKind::Pen,cx));s.settle();
    s.screenshot("ink-panel");
    // The list of effects is a drop-down; choosing an item sets the effect.
    s.click(center("ink-effect"));s.settle();
    s.screenshot("ink-effects-open");
    s.key("down");s.key("enter");s.settle();
    s.screenshot("ink-effects-chosen");
}
fn fill_panels(){
    let _probe=buttons::Probe::new();
    let (mut s,ids)=fixture();
    s.update(|e,_,cx|{e.set_selection(vec![ids[0]]);cx.notify();});
    s.update(|e,w,cx|e.open_gradient_panel(false,w,cx));s.settle();s.screenshot("fill-linear");
    s.update(|e,w,cx|e.open_path_gradient_panel(false,w,cx));s.settle();s.screenshot("fill-path");
    s.click(center("pptx-gradient-rect"));s.settle();
    assert_eq!(s.with(|e,_|e.shape_edit.gradient_panel.as_ref().unwrap().path),Some(ibis_doc_model::GradientPath::Rect),"the segment control chose the square");
    s.screenshot("fill-path-rect");
    s.update(|e,w,cx|e.open_pattern_panel(false,w,cx));s.settle();s.screenshot("fill-pattern");
}
