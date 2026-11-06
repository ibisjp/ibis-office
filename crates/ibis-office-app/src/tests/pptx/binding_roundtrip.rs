// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A panel's Apply without any edit leaves the document as it was (tasks/doc/design/ui-model-binding.md): the fields show the
//! model's values rounded, and `ShownNumber` keeps the exact values behind them (a glow radius of 64 008 EMU = 5.04 pt is shown 5).
use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Geometry, Insets, Shadow, Shape, ShapeKind, TextBody, TextStyle};
use crate::shell::scenario::Scenario;

pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("pptx::binding_unedited_apply_keeps_glow_shadow_insets",unedited_apply),
]}

/// Values the displayed digits cannot hold: 5.04 pt, 4.0001 pt, 12.3457°, 0.2539 cm.
const GLOW:i64=64_008;
const BLUR:i64=50_801;
const DISTANCE:i64=63_501;
const DIRECTION:i32=740_741;
const INSET:i64=91_441;

fn unedited_apply(){
    let mut s=Scenario::new_document(ibis_pptx::Kind::Presentation);
    s.size(1200.,900.);
    let id=s.update(|e,_,cx|{
        let mut auto=AutoShape::new(Geometry::Rect,Fill::Solid(Color::BLACK),None);
        auto.glow=Some((Color::BLACK,GLOW));
        auto.shadow=Some(Shadow::simple(Color::BLACK,BLUR,DISTANCE,DIRECTION));
        let mut text=TextBody::plain("Binding",TextStyle::default());
        text.insets=Insets{left:INSET,top:INSET+1,right:INSET+2,bottom:INSET+3};
        auto.text=Some(text);
        let id=e.document.ids().new_shape_id();
        e.insert_shape(Shape::new(id,EmuRect::new(500_000,500_000,2_000_000,1_000_000),ShapeKind::Auto(auto)),cx);
        e.set_selection(vec![id]);cx.notify();id
    });
    let before=s.with(|e,_|e.slide().shape(id).cloned()).expect("the shape");
    let undo_before=s.with(|e,_|e.document.undo_len());
    for name in ["glow","shadow","insets"] {
        s.update(|e,w,cx|match name {
            "glow"=>{e.open_glow_panel(w,cx);e.apply_glow_panel(w,cx);}
            "shadow"=>{e.open_shadow_panel(w,cx);e.apply_shadow_panel(w,cx);}
            _=>{e.open_text_insets_panel(w,cx);e.apply_text_insets_panel(w,cx);}
        });s.settle();
        let after=s.with(|e,_|e.slide().shape(id).cloned()).expect("the shape");
        assert_eq!(after,before,"{name}: an Apply without an edit changed the shape");
        assert_eq!(s.with(|e,_|e.document.undo_len()),undo_before,"{name}: an Apply without an edit made an Undo step");
    }
}
