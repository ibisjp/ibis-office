// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Owner-drawn items use the same Dropdown search, navigation, scrolling and placement.
use super::*;
#[derive(Clone)]
pub(crate) struct PreviewChoice {
 pub label:SharedString,
 pub content:Rc<dyn Fn()->AnyElement>,
 pub choose:Handler,
}
#[derive(IntoElement)]
pub(crate) struct PreviewDropdown(dropdown::Dropdown);
pub(crate) fn preview_dropdown(id:&'static str,choices:Vec<PreviewChoice>,selected:usize,cx:&Context<SlideEditor>)->PreviewDropdown{
 let current=selected.min(choices.len().saturating_sub(1));
 let preview=choices.get(current).map(|c|(c.content)()).unwrap_or_else(||div().into_any_element());
 let label=choices.get(current).map(|c|c.label.clone()).unwrap_or_default();
 let entries=choices.into_iter().enumerate().map(|(i,c)| {
  let mut item=dropdown::DropdownItem::new(c.label).checked(i==current).on_select(c.choose);
  item.content=Some(dropdown::ItemContent::Custom(c.content));item.menu().into()
 }).collect();
 PreviewDropdown(dropdown::Dropdown::with_button(id,dropdown_button(id,preview).w(FIELD_WIDTH).tooltip(label),entries,true,0,&cx.weak_entity())
  .checkmarks(false).list_width(dropdown::ListWidth::Fixed(FIELD_WIDTH)).button_width(FIELD_WIDTH))
}
impl RenderOnce for PreviewDropdown {fn render(self,_:&mut Window,_:&mut App)->impl IntoElement {self.0}}
