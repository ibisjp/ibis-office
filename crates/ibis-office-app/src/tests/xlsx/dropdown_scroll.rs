// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
use super::SheetSteps;
use crate::shell::{scenario::Scenario,editor::SlideEditor};
use gpui_kit::*;
fn drag(s:&mut Scenario,handle:ScrollHandle) {
    let b=handle.bounds();let max=f32::from(handle.max_offset().y);assert!(max>0.);
    let vh=f32::from(b.size.height);let thumb=(vh*(vh/(vh+max)).clamp(0.05,1.)).max(16.).min(vh);
    let start=point(b.right()-px(5.),b.top()+px(thumb/2.));
    s.dispatch(vec![PlatformInput::MouseMove(MouseMoveEvent{position:start,..Default::default()}),PlatformInput::MouseDown(MouseDownEvent{position:start,button:MouseButton::Left,click_count:1,..Default::default()})]);
    let p=start+point(px(0.),px(40.));
    s.dispatch(vec![PlatformInput::MouseMove(MouseMoveEvent{position:p,pressed_button:Some(MouseButton::Left),..Default::default()})]);
    let first=handle.offset().y;assert!(first<px(0.),"first drag survives repaint");
    let p=p+point(px(0.),px(40.));
    s.dispatch(vec![PlatformInput::MouseMove(MouseMoveEvent{position:p,pressed_button:Some(MouseButton::Left),..Default::default()})]);
    assert!(handle.offset().y<first,"drag continues after another repaint");
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent{position:p,button:MouseButton::Left,click_count:1,..Default::default()})]);
}
fn panel(_: &mut SlideEditor,_:&mut Window,_:&mut Context<SlideEditor>)->Option<crate::shell::chrome::SidebarPanel>{
    Some(crate::shell::chrome::SidebarPanel{title:"Scrollbar test".into(),body:gpui_kit::component::v_flex().children((0..100).map(|i|div().h(px(28.)).flex_none().child(format!("Row {i}")))).into_any_element()})
}
struct Cleanup;
impl Drop for Cleanup {fn drop(&mut self){crate::shell::chrome::FORCED_SIDEBAR.with(|s|s.set(None));}}
pub(super) fn run(){
    let calls=std::cell::RefCell::new(Vec::new());let labels:Vec<_>=(1..=150).map(|n|"x".repeat(n)).collect();
    let width=crate::shell::chrome::dropdown_list::content_width(&labels,|s|s.clone(),|s|{calls.borrow_mut().push(s.len());s.len() as f32*10.},90.);
    assert_eq!(width,600.);assert_eq!(calls.borrow().len(),100);assert_eq!(calls.borrow()[0],150);assert_eq!(calls.borrow()[99],51);
    let mut s=super::new_sheet();s.size(1200.,800.);s.click_cell("A1");
    s.update(|e,_,cx|e.set_list_validation(&(0..100).map(|i|format!("Item {i:03} - long label for width measurement")).collect::<Vec<_>>().join(","),"",cx));
    s.key("alt-down");
    let scroll=s.with(|e,_|e.wb().validation_choices.as_ref().unwrap().scroll.clone());
    let (outer,cell)=s.with(|e,_|(e.wb().validation_choices.as_ref().unwrap().bounds.get().unwrap(),e.validation_cell_bounds(0,0).unwrap()));
    assert!(outer.size.width>cell.size.width+px(21.));assert!(outer.size.width<=px(600.));
    drag(&mut s,scroll);s.key("escape");
    crate::shell::chrome::FORCED_SIDEBAR.with(|c|c.set(Some((panel,"scrollbar-test",crate::shell::panel_dialog::PanelMode::Sidebar))));let _cleanup=Cleanup;
    s.update(|e,_,cx|{e.sidebar_open=true;cx.notify();});
    let scroll=s.with(|e,_|e.sidebar_scroll.clone());drag(&mut s,scroll);
}
