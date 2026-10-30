// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
use super::{Scenario,button_probe as buttons};
use crate::shell::{chrome,editor::SlideEditor};
use gpui_kit::*;
thread_local!{static CHECKS:std::cell::Cell<bool>=const{std::cell::Cell::new(true)};static VALUE:std::cell::Cell<usize>=const{std::cell::Cell::new(1)};}
fn panel(_: &mut SlideEditor,_:&mut Window,cx:&mut Context<SlideEditor>)->Option<chrome::SidebarPanel>{
    let selected=VALUE.with(|s|s.get());
    let items=["Alpha","Beta","Gamma - a longer item that determines the content width"].into_iter().enumerate().map(|(i,label)| {
        let mut item=chrome::DropdownItem::new(label).checked(i==selected).on_select(std::rc::Rc::new(move|_,_,cx|{VALUE.with(|v|v.set(i));cx.notify();}));
        if i==0 {item.icon=Some("insert_chart");}item
    }).collect();
    Some(chrome::SidebarPanel{title:"Dropdown attributes".into(),body:chrome::Dropdown::new("attribute-dropdown","Choose",items,&cx.weak_entity()).checkmarks(CHECKS.with(|s|s.get())).into_any_element()})
}
struct Cleanup;
impl Drop for Cleanup {fn drop(&mut self){chrome::FORCED_SIDEBAR.with(|s|s.set(None));}}
pub(crate) fn run(){
    CHECKS.with(|s|s.set(true));VALUE.with(|s|s.set(1));let _probe=buttons::Probe::new();
    let mut s=Scenario::open("test/editing/docx-report.docx");s.size(1200.,800.);
    chrome::FORCED_SIDEBAR.with(|c|c.set(Some((panel,"dropdown-attributes",crate::shell::panel_dialog::PanelMode::Sidebar))));let _cleanup=Cleanup;
    s.update(|e,_,cx|{e.sidebar_open=true;cx.notify();});
    buttons::click(&mut s,"attribute-dropdown");
    let row=buttons::all().into_iter().find(|id|id.contains("searchable-item-attribute-dropdown")&&id.ends_with(", 1)")).unwrap();
    let b=buttons::bounds(&row).unwrap();let with=s.screenshot("checkmark-on");
    let width=chrome::DROPDOWN_LAYOUT.with(|m|m.borrow()["attribute-dropdown"].0);assert!(width>124. && width<=600.);
    s.key("escape");CHECKS.with(|s|s.set(false));s.update(|_,_,cx|cx.notify());buttons::click(&mut s,"attribute-dropdown");
    let without=s.screenshot("checkmark-off");
    let a=image::open(with).unwrap().to_rgba8();let z=image::open(without).unwrap().to_rgba8();let scale=a.width() as f32/1200.;
    let crop=|i:&image::RgbaImage|image::imageops::crop_imm(i,(f32::from(b.left())*scale)as u32,(f32::from(b.top())*scale)as u32,(22.*scale)as u32,(f32::from(b.size.height)*scale)as u32).to_image();
    assert_ne!(crop(&a),crop(&z),"the checkbox attribute changes the check glyph, without changing item data");
    assert_eq!(VALUE.with(|s|s.get()),1);s.key("down");s.key("enter");assert_eq!(VALUE.with(|s|s.get()),2);
    assert!(s.with(|e,_|e.toolbar_search_open.is_none()));
}

pub(crate) fn toolbar_style(){
    use crate::docx::scenarios::DocSteps;
    let _probe=buttons::Probe::new();let mut s=Scenario::open("test/editing/docx-report.docx");s.size(1200.,800.);
    let (paragraph,_)=s.paragraph(0);s.click_text(ibis_doc_model::FlowPos::new(paragraph,0));
    let id=buttons::all().into_iter().find(|id|id.starts_with("style-")&&id[6..].parse::<usize>().is_ok()).unwrap();
    let button=buttons::bounds(&id).unwrap();let path=s.screenshot("toolbar-style-common-button");
    let image=image::open(path).unwrap().to_rgba8();let scale=image.width() as f32/1200.;
    // Sample the clear top padding, avoiding fractional border antialiasing and label glyphs.
    let from=((f32::from(button.left())+10.)*scale) as u32;
    let to=((f32::from(button.right())-20.)*scale) as u32;
    let y=((f32::from(button.top())+4.)*scale) as u32;
    let white=(from..to).filter(|&x|image.get_pixel(x,y).0==[255,255,255,255]).count();
    assert!(white as f32 >= (to-from) as f32*0.8,"the style dropdown uses the common white button: {button:?}");
    buttons::click(&mut s,&id);
    let selected=s.with(|e,_|e.toolbar_search_popup_items.as_ref().unwrap().1.iter().position(|i|i.checked).unwrap());
    let row=buttons::all().into_iter().find(|id|id.contains("searchable-item-style")&&id.ends_with(&format!(", {selected})"))).unwrap();
    let b=buttons::bounds(&row).unwrap();let path=s.screenshot("toolbar-style-selected-check");let image=image::open(path).unwrap().to_rgba8();
    let pixel=image.get_pixel(((f32::from(b.left())+3.)*scale)as u32,(f32::from(b.center().y)*scale)as u32);
    let color=chrome::SELECTED_ROW_BG;assert_eq!(pixel.0,[(color>>16)as u8,(color>>8)as u8,color as u8,255]);
    s.key("escape");
}
