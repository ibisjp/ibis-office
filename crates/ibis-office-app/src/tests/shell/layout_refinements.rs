// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{Scenario, button_probe as buttons};
use crate::docx::scenarios::DocSteps;
use gpui_kit::*;
use gpui_kit::component::input::InputEvent;
pub(crate) fn all()->Vec<(&'static str,fn())>{vec![
    ("shell::dropdown_search_threshold_and_width",dropdown_threshold),
    ("shell::autocorrect_dictionary_fixed_header",dictionary),
    ("shell::print_copies_spinner_and_full_content",print_content),
]}
fn shot(s:&mut Scenario,name:&str){
    let path=s.screenshot(name);
    let bounds=s.with(|e,_|e.panel_dialog.bounds.unwrap());
    let image=image::open(&path).unwrap().to_rgba8();let scale=image.width() as f32/1200.;
    let crop=image::imageops::crop_imm(&image,(f32::from(bounds.origin.x)*scale).round() as u32,(f32::from(bounds.origin.y)*scale).round() as u32,(f32::from(bounds.size.width)*scale).round() as u32,(f32::from(bounds.size.height)*scale).round() as u32).to_image();
    let root=crate::shell::scenario::repo_root().join("target/ui-layout-refinements");std::fs::create_dir_all(&root).unwrap();
    crop.save(root.join(format!("{name}.png"))).unwrap();
    std::fs::copy(path,root.join(format!("{name}-window.png"))).unwrap();
}
fn dictionary(){
    let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/docx-report.docx");s.size(1200.,800.);
    s.update(|e,w,cx|{e.open_doc_autocorrect_panel(w,cx);e.doc_state_mut().unwrap().autocorrect.as_mut().unwrap().working.table=(0..100).map(|i|(format!("from-{i:03}"),format!("replacement-{i:03}"))).collect();cx.notify();});
    shot(&mut s,"autocorrect-settings");
    let tab=buttons::all().into_iter().find(|id|id.contains("autocorrect-tabs")&&id.ends_with(", 1)")).unwrap();buttons::click(&mut s,&tab);
    assert!(s.with(|e,_|e.doc_state().unwrap().autocorrect.as_ref().unwrap().dictionary_tab));
    let header=s.with(|e,_|e.doc_state().unwrap().autocorrect.as_ref().unwrap().ui.borrow()["dictionary-header"]);
    shot(&mut s,"autocorrect-dictionary");
    s.wheel((f32::from(header.center().x),f32::from(header.bottom()+px(50.))),-10000.);
    let (after,offset,outer,last)=s.with(|e,_|{let p=e.doc_state().unwrap().autocorrect.as_ref().unwrap();(p.ui.borrow()["dictionary-header"],p.table_scroll.offset().y,e.panel_dialog.scroll.offset().y,p.ui.borrow()["entry-99"])});
    assert_eq!(header,after,"table header stays fixed");assert!(offset<px(0.));assert_eq!(outer,px(0.),"dialog content does not scroll");
    let dialog=s.with(|e,_|e.panel_dialog.bounds.unwrap());assert!(last.bottom()<dialog.bottom()-px(40.),"last row is above action bar");
    shot(&mut s,"autocorrect-dictionary-bottom");
    s.update(|e,_,cx|{let p=e.doc_state_mut().unwrap().autocorrect.as_mut().unwrap();p.working.table.clear();cx.notify();});
    buttons::click(&mut s,"autocorrect-add");assert!(s.with(|e,_|e.doc_state().unwrap().autocorrect.as_ref().unwrap().entry_open));
}
fn print_content(){
    let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/docx-report.docx");s.size(1200.,800.);
    s.update(|e,w,cx|e.open_doc_print_panel(w,cx));
    s.update(|e,w,cx|{let input=e.doc_state().unwrap().print_panel.as_ref().unwrap().copies_input.clone();input.update(cx,|i,cx|{i.set_value("3",w,cx);cx.emit(InputEvent::Change);});});
    assert_eq!(s.with(|e,_|e.docx_print_settings().copies),3);
    shot(&mut s,"print-settings-top");let b=s.with(|e,_|e.panel_dialog.bounds.unwrap());s.wheel((f32::from(b.center().x),f32::from(b.center().y)),-10000.);
    assert!(s.with(|e,_|e.panel_dialog.scroll.offset().y)<px(0.));shot(&mut s,"print-settings-bottom");
    s.size(1200.,1600.);s.update(|e,_,cx|{e.panel_dialog.bounds=None;e.panel_dialog.scroll.set_offset(Point::default());cx.notify();});shot(&mut s,"print-settings-all");
}
thread_local! {
    static DROPDOWN_INPUT:std::cell::RefCell<Option<Entity<gpui_kit::component::input::InputState>>>=const{std::cell::RefCell::new(None)};
    static CHOICES:std::cell::Cell<usize>=const{std::cell::Cell::new(20)};
}
fn dropdown_panel(_: &mut crate::shell::editor::SlideEditor,_:&mut Window,cx:&mut Context<crate::shell::editor::SlideEditor>)->Option<crate::shell::chrome::SidebarPanel>{
    let field=DROPDOWN_INPUT.with(|f|f.borrow().as_ref().unwrap().clone());
    let choices=(0..CHOICES.with(|n|n.get())).map(|i|format!("Choice {i:02}")).collect();
    Some(crate::shell::chrome::SidebarPanel{title:"Dropdown fixture".into(),body:crate::shell::chrome::input_dropdown("threshold-dropdown",&field,choices,cx)})
}
struct DropdownCleanup;
impl Drop for DropdownCleanup {fn drop(&mut self){crate::shell::chrome::FORCED_SIDEBAR.with(|c|c.set(None));DROPDOWN_INPUT.with(|f|{f.borrow_mut().take();});}}
fn dropdown_threshold(){
    let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/docx-report.docx");s.size(1200.,800.);
    s.update(|_,w,cx|DROPDOWN_INPUT.with(|f|*f.borrow_mut()=Some(cx.new(|cx|gpui_kit::component::input::InputState::new(w,cx).default_value("Choice 00")))));
    crate::shell::chrome::FORCED_SIDEBAR.with(|c|c.set(Some((dropdown_panel,"threshold-fixture",crate::shell::panel_dialog::PanelMode::Sidebar))));
    let _cleanup=DropdownCleanup;
    for count in [20,21]{
        CHOICES.with(|n|n.set(count));s.update(|e,_,cx|{e.sidebar_open=true;cx.notify();});buttons::click(&mut s,"threshold-dropdown");
        let (width,search)=crate::shell::chrome::DROPDOWN_LAYOUT.with(|m|m.borrow()["threshold-dropdown"]);
        assert_eq!(search,count>20);assert!(width>=f32::from(crate::shell::chrome::FIELD_WIDTH) && width<=600.,"default popup sizes to content with a button-width minimum, actual={width}");
        s.screenshot(&format!("dropdown-{count}-items"));
        if count==20 {let before=s.document();s.type_text("ignored");assert!(s.document()==before,"typing in an open list must not edit the document");s.key("down");s.key("enter");assert_eq!(s.with(|_,cx|DROPDOWN_INPUT.with(|f|f.borrow().as_ref().unwrap().read(cx).value().to_string())),"Choice 01");}
        else{s.type_text("Choice 20");s.key("enter");assert_eq!(s.with(|_,cx|DROPDOWN_INPUT.with(|f|f.borrow().as_ref().unwrap().read(cx).value().to_string())),"Choice 20");}
    }
    crate::shell::chrome::FORCED_SIDEBAR.with(|c|c.set(None));DROPDOWN_INPUT.with(|f|{f.borrow_mut().take();});
}
