// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::*;
use crate::{Document,Validation};
fn workbook()->(Document,SlideId){
 let mut p=Presentation::empty(EmuSize::WIDESCREEN);let id=p.new_slide_id();let mut slide=Slide::new(id);slide.name=Some("Data".into());
 slide.shapes.push(Shape{id:p.new_shape_id(),frame:EmuRect::new(0,0,1000,1000),rotation:0,flip_h:false,flip_v:false,kind:ShapeKind::Sheet(Arc::new(Sheet::default())),floating:None});p.slides.push(slide);(Document::new(p),id)
}
fn rectangle(r0:u32,c0:u32,r1:u32,c1:u32)->CellRange{CellRange{first_row:r0,first_column:c0,last_row:r1,last_column:c1}}
fn validations(d:&Document,id:SlideId)->Vec<Validation>{crate::sheet_calc::sheet_of_slide(d.presentation().slide(id).unwrap()).unwrap().validations.clone()}
#[test]
fn clearing_the_middle_preserves_surrounding_rules_and_undo(){
 let(mut d,id)=workbook();let area=rectangle(0,0,2,2);let center=rectangle(1,1,1,1);
 let rule=Validation{kind:"custom".into(),formula1:Some("A1>0".into()),..Default::default()};
 d.execute(vec![Command::Sheet(SheetCommand::SetValidation{slide:id,ranges:vec![area],rule:Some(rule)})]).unwrap();let before=validations(&d,id);
 d.execute(vec![Command::Sheet(SheetCommand::SetValidation{slide:id,ranges:vec![center],rule:None})]).unwrap();let rules=validations(&d,id);
 for row in 0..=2{for column in 0..=2{let count=rules.iter().flat_map(|r|&r.ranges).filter(|r|r.first_row<=row&&row<=r.last_row&&r.first_column<=column&&column<=r.last_column).count();assert_eq!(count,usize::from((row,column)!=(1,1)));}}
 assert!(d.undo());assert_eq!(validations(&d,id),before);
}
#[test]
fn trimming_the_anchor_rebases_relative_validation_formulas(){
 let(mut d,id)=workbook();d.execute(vec![Command::Sheet(SheetCommand::SetValidation{slide:id,ranges:vec![rectangle(0,0,3,0)],rule:Some(Validation{kind:"custom".into(),formula1:Some("MOD(A1,2)=0".into()),..Default::default()})})]).unwrap();
 d.execute(vec![Command::Sheet(SheetCommand::SetValidation{slide:id,ranges:vec![rectangle(0,0,0,0)],rule:Some(Validation{kind:"whole".into(),..Default::default()})})]).unwrap();let rules=validations(&d,id);assert_eq!(rules.len(),2);assert_eq!(rules[0].ranges,vec![rectangle(1,0,3,0)]);assert_eq!(rules[0].formula1.as_deref(),Some("MOD(A2,2)=0"));
 assert!(d.undo());assert_eq!(validations(&d,id)[0].formula1.as_deref(),Some("MOD(A1,2)=0"));
}
