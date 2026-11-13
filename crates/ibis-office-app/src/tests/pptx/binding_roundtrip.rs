// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A panel's Apply without any edit leaves the document as it was (tasks/doc/design/ui-model-binding.md): the fields show the
//! model's values rounded, and `ShownNumber` keeps the exact values behind them (a glow radius of 64 008 EMU = 5.04 pt is shown 5).
use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Geometry, Insets, Shadow, Shape, ShapeKind, TextBody, TextStyle};
use crate::shell::scenario::Scenario;
use crate::pptx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("pptx::binding_unedited_apply_keeps_glow_shadow_insets",unedited_apply),
    ("pptx::binding_unedited_apply_keeps_slide_size",unedited_slide_size),
    ("pptx::binding_unedited_apply_keeps_cell_size",unedited_cell_size),
    ("xlsx::binding_unedited_apply_keeps_frame",unedited_sheet_frame),
    ("xlsx::binding_unedited_apply_keeps_row_height_and_column_width",unedited_row_col_size),
]}

/// Values the displayed digits cannot hold: 5.04 pt, 4.0001 pt, 12.3457°, 0.2539 cm.
const GLOW:i64=64_008;
const BLUR:i64=50_801;
const DISTANCE:i64=63_501;
const DIRECTION:i32=740_741;
const INSET:i64=91_441;
/// A rotation of 12.3457 degrees: the frame panel shows 12.3.
const ROTATION:i32=740_741;

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
        let mut shape=Shape::new(id,EmuRect::new(500_001,500_003,2_000_007,1_000_009),ShapeKind::Auto(auto));
        shape.rotation=ROTATION;
        e.insert_shape(shape,cx);
        e.set_selection(vec![id]);cx.notify();id
    });
    let before=s.with(|e,_|e.slide().shape(id).cloned()).expect("the shape");
    let undo_before=s.with(|e,_|e.document.undo_len());
    for name in ["glow","shadow","insets","frame"] {
        s.update(|e,w,cx|match name {
            "glow"=>{e.open_glow_panel(w,cx);e.apply_glow_panel(w,cx);}
            "shadow"=>{e.open_shadow_panel(w,cx);e.apply_shadow_panel(w,cx);}
            "frame"=>{e.open_frame_panel(w,cx);e.apply_frame_panel(w,cx);}
            _=>{e.open_text_insets_panel(w,cx);e.apply_text_insets_panel(w,cx);}
        });s.settle();
        let after=s.with(|e,_|e.slide().shape(id).cloned()).expect("the shape");
        assert_eq!(after,before,"{name}: an Apply without an edit changed the shape");
        assert_eq!(s.with(|e,_|e.document.undo_len()),undo_before,"{name}: an Apply without an edit made an Undo step");
    }
}

/// The slide size panel shows 25.4 x 19.05 cm for 9 144 007 x 6 858 011 EMU; an Apply without an edit keeps the exact size.
fn unedited_slide_size(){
    let mut s=Scenario::new_document(ibis_pptx::Kind::Presentation);
    let size=ibis_doc_model::EmuSize::new(9_144_007,6_858_011);
    s.update(|e,_,cx|{e.run(vec![ibis_doc_model::Command::SetSlideSize{size}],cx);});
    let undo_before=s.with(|e,_|e.document.undo_len());
    s.update(|e,w,cx|{e.open_slide_size_panel(w,cx);e.apply_slide_size_panel(w,cx);});s.settle();
    assert_eq!(s.with(|e,_|e.presentation().slide_size),size,"an Apply without an edit changed the slide size");
    assert_eq!(s.with(|e,_|e.document.undo_len()),undo_before,"an Apply without an edit made an Undo step");
}

/// The cell size panel shows the row height and the column width in cm with two digits; an Apply without an edit keeps the table
/// (the rows and columns of the benchmark table are not whole hundredths of a cm).
fn unedited_cell_size(){
    use ibis_doc_model::ShapeKind as Kind;
    let mut s=Scenario::open("test/benchmark/earnings-presentation.pptx");
    let (page,shape)=s.with(|e,_|e.presentation().slides.iter().enumerate().find_map(|(i,sl)|sl.shapes.iter().position(|sh|matches!(&sh.kind,Kind::Table(t) if !t.has_merges()&&t.rows.len()>=3&&t.columns.len()>=3)).map(|j|(i,j)))).expect("a table");
    s.page(page+1);
    let cell=super::cell_middle(&mut s,shape,1,1);
    s.double_click(cell);
    let before=s.with(|e,_|e.slide().shapes[shape].clone());
    let (row_emu,column_emu)=s.with(|_e,_|{let Kind::Table(t)=&before.kind else {unreachable!()};(t.row_heights(before.frame.height)[1],t.columns[1])});
    let undo_before=s.with(|e,_|e.document.undo_len());
    s.update(|e,w,cx|{e.open_cell_size_panel(w,cx);e.apply_cell_size_panel(w,cx);});s.settle();
    let after=s.with(|e,_|e.slide().shapes[shape].clone());
    assert_eq!(after.frame,before.frame,"an Apply without an edit changed the table's frame (row {row_emu}, column {column_emu} EMU)");
    assert_eq!(after.kind,before.kind,"an Apply without an edit changed the table");
    assert_eq!(s.with(|e,_|e.document.undo_len()),undo_before,"an Apply without an edit made an Undo step");
}

/// The size and position panel of a sheet object applies when a field is entered or left: that must not move the object by the
/// rounding of the cm shown (1 234 567 EMU is shown 3.43 cm, a rotation of 12.3457 degrees 12.3).
fn unedited_sheet_frame(){
    let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");
    let id=s.update(|e,_,cx|e.insert_sheet_shape("rect",cx));
    let slide=s.with(|e,_|e.current_slide);
    let frame=EmuRect::new(1_234_567,2_345_679,3_456_791,987_653);
    s.update(|e,_,cx|{
        e.run_sheet(vec![
            ibis_doc_model::Command::SetFrame{slide,shape:id,frame},
            ibis_doc_model::Command::SetTransform{slide,shape:id,rotation:ROTATION,flip_h:false,flip_v:false},
        ],cx);
    });
    let before=s.with(|e,_|e.slide().shape(id).cloned()).expect("the shape");
    assert_eq!((before.frame,before.rotation),(frame,ROTATION));
    let undo_before=s.with(|e,_|e.document.undo_len());
    s.update(|e,w,cx|{e.selection=Some(id);e.open_sheet_frame_panel(w,cx);e.apply_sheet_frame_panel(w,cx);});s.settle();
    let after=s.with(|e,_|e.slide().shape(id).cloned()).expect("the shape");
    assert_eq!((after.frame,after.rotation),(frame,ROTATION),"an Apply without an edit moved the object");
    assert_eq!(s.with(|e,_|e.document.undo_len()),undo_before,"an Apply without an edit made an Undo step");
}

/// The row height (pt) and the column width (characters) panel shows two digits; Enter without an edit keeps the exact size
/// (238 126 EMU is 18.7502 pt, shown 18.75).
fn unedited_row_col_size(){
    use ibis_doc_model::{Command,SheetCommand,sheet_command::Axis};
    let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");
    let slide=s.with(|e,_|e.current_slide);
    let (row,column)=(238_126_i64,712_345_i64);
    s.update(|e,_,_|{
        let _=e.document.execute(vec![Command::Sheet(SheetCommand::SetSize{slide,axis:Axis::Rows,first:0,last:0,size:row})]);
        let _=e.document.execute(vec![Command::Sheet(SheetCommand::SetSize{slide,axis:Axis::Columns,first:0,last:0,size:column})]);
    });
    let sizes=|s:&mut Scenario|s.with(|e,_|{let (_,sheet)=e.current_sheet().expect("a sheet");(sheet.rows[0] as i64,sheet.columns[0] as i64)});
    assert_eq!(sizes(&mut s),(row,column));
    s.update(|e,w,cx|{e.open_row_height_panel(w,cx);e.apply_row_col_size(cx);});s.settle();
    assert_eq!(sizes(&mut s),(row,column),"Enter in the row height field without an edit changed the height");
    s.update(|e,w,cx|{e.open_column_width_panel(w,cx);e.apply_row_col_size(cx);});s.settle();
    assert_eq!(sizes(&mut s),(row,column),"Enter in the column width field without an edit changed the width");
}
