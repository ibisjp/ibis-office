// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! User workflows for the shared ink panel and the separate spreadsheet editors.
use super::{Scenario,button_probe as buttons};
use gpui_kit::*;
use gpui_kit::component::input::{InputEvent,InputState};
use crate::xlsx::scenarios::SheetSteps;
use crate::xlsx::ink::PenKind;

pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("shell::solver_constraint_editor_commit_cancel_close",solver_constraint),
    ("shell::watch_editor_commit_cancel_close",watch_editor),
    ("shell::table_name_live_change_and_undo",table_name),
    ("shell::csv_path_enter_preserves_first_record",csv_path),
    ("shell::shared_ink_docx_pointer_controls",ink_docx),
    ("shell::shared_ink_xlsx_pointer_controls",ink_xlsx),
    ("shell::shared_ink_pptx_pointer_controls",ink_pptx),
]}
fn replace(s:&mut Scenario,input:Entity<InputState>,value:&str){
    s.update(|_,w,cx|input.update(cx,|i,cx|{i.set_value(value,w,cx);cx.emit(InputEvent::Change);}));
}
fn tuple_button(s:&mut Scenario,name:&str,index:usize){
    let id=buttons::all().into_iter().find(|id|id.contains(&format!("\"{name}\""))&&id.ends_with(&format!(", {index})"))).unwrap_or_else(||panic!("missing {name}[{index}]: {:?}",buttons::all()));
    buttons::click(s,&id);
}
fn close_x(s:&mut Scenario){
    let bounds=s.with(|e,_|e.panel_dialog.bounds.expect("dialog frame"));
    s.click((f32::from(bounds.right()-px(26.)),f32::from(bounds.origin.y+px(22.))));
}
fn solver_values(s:&mut Scenario,lhs:&str,rhs:&str){
    let(a,b)=s.with(|e,_|{let p=e.wb().panels.solver.as_ref().unwrap();(p.c_lhs.clone(),p.c_rhs.clone())});
    replace(s,a,lhs);replace(s,b,rhs);
}
fn solver_constraint(){
    let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");s.size(1200.,900.);
    s.update(|e,w,cx|e.open_solver_panel(w,cx));
    let initial=s.with(|e,_|e.wb().panels.solver.as_ref().unwrap().constraints.clone());
    buttons::click(&mut s,"solver-add");
    assert_eq!(s.with(|e,_|e.panel_dialog.key),Some("solver_constraint_editor"));
    solver_values(&mut s,"A1","10");buttons::click(&mut s,"solver-constraint-cancel");
    assert_eq!(s.with(|e,_|e.wb().panels.solver.as_ref().unwrap().constraints.clone()),initial);
    buttons::click(&mut s,"solver-add");solver_values(&mut s,"A1","10");buttons::click(&mut s,"solver-constraint-ok");
    let index=s.with(|e,_|{let p=e.wb().panels.solver.as_ref().unwrap();assert!(p.editor.is_none());assert_eq!(p.constraints.len(),initial.len()+1);p.constraints.len()-1});
    let saved=s.with(|e,_|e.wb().panels.solver.as_ref().unwrap().constraints.clone());
    tuple_button(&mut s,"solver-constraint-edit",index);solver_values(&mut s,"B2","20");close_x(&mut s);
    assert_eq!(s.with(|e,_|e.wb().panels.solver.as_ref().unwrap().constraints.clone()),saved,"title X discards a constraint draft");
    tuple_button(&mut s,"solver-constraint-edit",index);solver_values(&mut s,"B2","20");buttons::click(&mut s,"solver-constraint-ok");
    s.with(|e,_|{let p=e.wb().panels.solver.as_ref().unwrap();assert_eq!(p.constraints[index].0,"B2");assert_eq!(p.constraints[index].2,"20");assert!(p.editor.is_none());});
    tuple_button(&mut s,"solver-constraint-delete",index);
    assert_eq!(s.with(|e,_|e.wb().panels.solver.as_ref().unwrap().constraints.clone()),initial);
}
fn watch_editor(){
    let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");s.size(1200.,900.);
    s.click_cell("A2");s.update(|e,w,cx|e.open_watch_window(w,cx));buttons::click(&mut s,"watch-add");
    let(id,original)=s.with(|e,_|{let entries=&e.wb().panels.watches;assert!(!entries.is_empty());(entries[0].id,entries[0].clone())});
    tuple_button(&mut s,"watch-edit",id as usize);
    let input=s.with(|e,_|e.wb().panels.watch_window.as_ref().unwrap().editor.clone().unwrap());replace(&mut s,input,"B3");buttons::click(&mut s,"watch-edit-cancel");
    assert_eq!(s.with(|e,_|e.wb().panels.watches.iter().find(|e|e.id==id).unwrap().clone()),original);
    tuple_button(&mut s,"watch-edit",id as usize);let input=s.with(|e,_|e.wb().panels.watch_window.as_ref().unwrap().editor.clone().unwrap());replace(&mut s,input,"B3");close_x(&mut s);
    assert_eq!(s.with(|e,_|e.wb().panels.watches.iter().find(|e|e.id==id).unwrap().clone()),original);
    tuple_button(&mut s,"watch-edit",id as usize);let input=s.with(|e,_|e.wb().panels.watch_window.as_ref().unwrap().editor.clone().unwrap());replace(&mut s,input,"B3");buttons::click(&mut s,"watch-edit-ok");
    assert_eq!(s.with(|e,_|e.wb().panels.watches.iter().find(|e|e.id==id).unwrap().cell_ref.clone()),"B3");
    assert!(s.with(|e,_|e.wb().panels.watch_window.as_ref().unwrap().editor.is_none()));
    tuple_button(&mut s,"watch-delete",id as usize);assert!(s.with(|e,_|e.wb().panels.watches.iter().all(|e|e.id!=id)));
}
fn table_name(){
    let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");s.size(1200.,900.);
    // Put an independent fixture below every existing cell and merge in the sample workbook.
    let first_row=s.with(|e,_|{let sheet=e.current_sheet().unwrap().1;sheet.cells.iter().map(|c|c.row).chain(sheet.merges.iter().map(|r|r.last_row)).max().unwrap_or(0)+2});
    s.update(|e,_,cx|{
        use ibis_doc_model::{CellEdit,CellInput,CellValue,Command,SheetCommand};
        let mut edits=Vec::new();
        for (r,values) in [["Product","Quantity","Total"],["Alpha","2","10"],["Beta","3","15"],["Gamma","4","20"]].into_iter().enumerate(){
            for(c,value)in values.into_iter().enumerate(){edits.push(CellEdit{row:first_row+r as u32,column:c as u32,input:CellInput::Value(if r==0||c==0{CellValue::Text(value.into())}else{CellValue::Number(value.parse().unwrap())})});}
        }
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetCells{slide:e.current_slide,edits})],cx);
        e.set_cell(0,first_row);
    });
    let created=s.update(|e,_,cx|e.make_table(ibis_doc_model::CellRange{first_row,last_row:first_row+3,first_column:0,last_column:2},true,cx));
    assert!(created.is_ok(),"independent non-merged table fixture: {created:?}");
    s.update(|e,w,cx|e.open_table_design(w,cx));
    let(index,input)=s.with(|e,_|{let p=e.wb().panels.table_design.as_ref().unwrap();(p.index,p.name.clone())});
    s.update(|_,w,cx|input.update(cx,|i,cx|i.focus(w,cx)));s.key("cmd-a");s.type_text("X");
    assert_eq!(s.with(|e,_|e.current_sheet().unwrap().1.tables[index].name.clone()),"X","one character change is already committed");
    s.type_text("y");assert_eq!(s.with(|e,_|e.current_sheet().unwrap().1.tables[index].name.clone()),"Xy");
    assert!(!buttons::all().iter().any(|id|id=="table-rename"));
    s.update(|e,w,cx|e.focus_handle.focus(w,cx));s.key("cmd-z");
    assert_eq!(s.with(|e,_|e.current_sheet().unwrap().1.tables[index].name.clone()),"X","Undo reverses the last live name change");
}
fn csv_path(){
    let _probe=buttons::Probe::new();
    let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");s.size(1200.,1000.);s.update(|e,w,cx|e.show_import_text_panel(w,cx));
    let path=crate::shell::scenario::repo_root().join("target/scenario/csv-path-input.csv");std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let data=b"first-record,001\nsecond-record,002\nthird-record,003\nfourth-record,004\nfifth-record,005\nsixth-record,006\n";std::fs::write(&path,data).unwrap();
    let input=s.with(|e,_|e.wb().panels.import_text.as_ref().expect("CSV dialog open before typing path").file_path.clone());
    s.update(|_,w,cx|input.update(cx,|i,cx|{i.set_value(path.to_string_lossy().into_owned(),w,cx);i.focus(w,cx);}));s.key("enter");
    assert!(s.with(|e,_|e.wb().panels.import_text.is_some()),"Enter in the file path loads the file without dismissing the import dialog");
    s.with(|e,_|{let p=e.wb().panels.import_text.as_ref().unwrap();assert_eq!(p.data.as_deref(),Some(data.as_slice()));assert_eq!(p.name.as_deref(),Some("csv-path-input.csv"));});
    assert!(buttons::bounds("import-text-pick").is_some(),"standard path Browse button");
    assert!(buttons::bounds("import-text-encoding").is_some());assert!(buttons::bounds("import-text-delimiter").is_some());
    buttons::click(&mut s,"import-text-encoding");
    assert!(s.with(|e,_|e.wb().panels.import_text.is_some()),"opening the encoding popup preserves its owner");
    assert_eq!(s.with(|e,_|e.toolbar_search_open),Some("import-text-encoding"),"common encoding popup is open");
    s.key("escape");
    assert!(s.with(|e,_|e.wb().panels.import_text.is_some()),"Escape closes the encoding popup, not its import dialog");
    assert!(s.with(|e,_|e.toolbar_search_open.is_none()),"Escape dismissed only the nested popup");
    s.with(|e,_|{let p=e.wb().panels.import_text.as_ref().unwrap();let parsed=ibis_pptx::csv::import(p.data.as_ref().unwrap(),&ibis_pptx::csv::ImportOptions::default()).unwrap();assert_eq!(parsed.rows.len(),6);assert_eq!(parsed.rows[0],vec!["first-record","001"]);});
}
fn ink_docx(){ink(0);}fn ink_xlsx(){ink(1);}fn ink_pptx(){ink(2);}
fn ink(format:usize){
    let _probe=buttons::Probe::new();let path=["test/editing/docx-report.docx","test/editing/xlsx-sales.xlsx","test/editing/pptx-plan.pptx"][format];
    let mut s=Scenario::open(path);s.size(1200.,1000.);s.update(|e,w,cx|match format{0=>e.open_docx_ink_panel(cx),1=>e.open_ink_panel(w,cx),_=>e.open_pptx_ink_panel(w,cx)});
    let tool=if format==0{"docx-ink-highlighter"}else{"ink-highlighter"};buttons::click(&mut s,tool);
    assert_eq!(s.with(|e,_|match format{0=>e.doc_state().unwrap().docx_ink.as_ref().unwrap().pen,1=>e.wb().panels.ink.as_ref().unwrap().pen,_=>e.pptx().unwrap().ink.as_ref().unwrap().pen}),PenKind::Highlighter);
    let(width_id,index,expected)=match format{0=>("docx-ink-width",0,crate::xlsx::ink::WIDTHS[0]),1=>("ink-width",0,crate::xlsx::ink::WIDTHS[0]),_=>("ink-width",0,crate::pptx::ink::PPTX_INK_WIDTHS[0])};if format==0{tuple_button(&mut s,width_id,index);}else{buttons::click(&mut s,"ink-width-0");}
    let actual=s.with(|e,_|match format{0=>e.doc_state().unwrap().docx_ink.as_ref().unwrap().width,1=>e.wb().panels.ink.as_ref().unwrap().width,_=>e.pptx().unwrap().ink.as_ref().unwrap().width});assert_eq!(actual,expected);
    buttons::click(&mut s,if format==0{"docx-ink-pen"}else{"ink-pen"});
    assert_eq!(s.with(|e,_|match format{0=>e.doc_state().unwrap().docx_ink.as_ref().unwrap().pen,1=>e.wb().panels.ink.as_ref().unwrap().pen,_=>e.pptx().unwrap().ink.as_ref().unwrap().pen}),PenKind::Pen);
    assert!(!s.with(|e,_|e.panel_dialog.active),"ink remains a pane for every format");
}
