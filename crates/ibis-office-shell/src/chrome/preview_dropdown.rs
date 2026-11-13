// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Owner-drawn items use the same Dropdown search, navigation, scrolling and placement.
use super::*;
#[derive(Clone)]
pub struct PreviewChoice {
 pub label:SharedString,
 pub content:Rc<dyn Fn()->AnyElement>,
 pub choose:Handler,
}
/// The size of a preview dropdown: the button and list width (default [`FIELD_WIDTH`]), the height of the closed button and the
/// height of each list item (both default [`BUTTON_HEIGHT`]).
#[derive(Clone,Copy)]
pub struct PreviewSize {pub width:Pixels,pub height:f32,pub row_height:f32}
impl PreviewSize {pub fn width(mut self,width:Pixels)->Self {self.width=width;self}}
/// What the closed button adds around its picture: the border and padding of the button, the gap and the ▾ icon.
pub(crate) const PREVIEW_BUTTON_CHROME: f32 = 44.;
impl PreviewSize {
    /// A dropdown just wide enough for a picture `content_width` wide (the width is derived from the picture, not guessed).
    pub fn fitting(content_width: f32) -> Self {Self{width:px(content_width+PREVIEW_BUTTON_CHROME),..Self::default()}}
}
impl Default for PreviewSize {fn default()->Self {Self{width:FIELD_WIDTH,height:BUTTON_HEIGHT,row_height:BUTTON_HEIGHT}}}
#[derive(IntoElement)]
pub struct PreviewDropdown(dropdown::Dropdown);
pub fn preview_dropdown(id:&'static str,choices:Vec<PreviewChoice>,selected:usize,cx:&Context<SlideEditor>)->PreviewDropdown{
 preview_dropdown_sized(id,choices,selected,PreviewSize::default(),cx)
}
pub fn preview_dropdown_sized(id:&'static str,choices:Vec<PreviewChoice>,selected:usize,size:PreviewSize,cx:&Context<SlideEditor>)->PreviewDropdown{
 let current=selected.min(choices.len().saturating_sub(1));
 let preview=choices.get(current).map(|c|(c.content)()).unwrap_or_else(||div().into_any_element());
 let label=choices.get(current).map(|c|c.label.clone()).unwrap_or_default();
 let entries=choices.into_iter().enumerate().map(|(i,c)| {
  let mut item=dropdown::DropdownItem::new(c.label).checked(i==current).on_select(c.choose);
  item.content=Some(dropdown::ItemContent::Custom(c.content));item.menu().into()
 }).collect();
 PreviewDropdown(dropdown::Dropdown::with_button(id,dropdown_button(id,preview).w(size.width).h(px(size.height)).min_h(px(size.height)).max_h(px(size.height)).tooltip(label),entries,true,0,&cx.weak_entity())
  .checkmarks(false).list_width(dropdown::ListWidth::Fixed(size.width)).button_width(size.width).row_height(size.row_height))
}
impl RenderOnce for PreviewDropdown {fn render(self,_:&mut Window,_:&mut App)->impl IntoElement {self.0}}
/// A dropdown of line widths (pt): each item, and the closed button, draws a horizontal line of that width (the preview) and
/// the label ("3.5 pt") at its right. The label's column is as wide as the widest label (every label is laid out, the others
/// invisible and zero high, so the layout engine measures it with the real font), and the line takes the rest of the button.
/// `widths` are `(width in pt, label, choose)`; `selected` is the index shown.
pub(crate) fn line_width_dropdown(id:&'static str,widths:Vec<(f32,SharedString,Handler)>,selected:usize,size:PreviewSize,cx:&Context<SlideEditor>)->PreviewDropdown{
 let labels:Rc<Vec<SharedString>>=Rc::new(widths.iter().map(|(_,l,_)|l.clone()).collect());
 let choices=widths.into_iter().map(|(pt,label,choose)|{
  let (text,labels)=(label.clone(),labels.clone());
  PreviewChoice{label,choose,content:Rc::new(move||{
   // 1 pt is 96/72 px.
   let thickness=px(pt*96./72.);
   let label_column=v_flex().flex_none().items_end().text_size(px(14.))
    .children(labels.iter().map(|l|div().h_0().overflow_hidden().whitespace_nowrap().opacity(0.).child(l.clone())))
    .child(div().whitespace_nowrap().child(text.clone()));
   h_flex().w_full().min_w_0().items_center().gap_2()
    .child(div().flex_1().min_w_0().flex().items_center().child(div().w_full().h(thickness).bg(rgb(TEXT))))
    .child(label_column)
    .into_any_element()
  })}
 }).collect();
 preview_dropdown_sized(id,choices,selected,size,cx)
}

/// The frame of a picture in a list item: it fills the item's width (the xlsx cell pattern and gradient pictures, the pptx pattern).
pub fn preview_frame() -> Div {
 div().relative().flex_1().h(px(16.)).rounded(px(2.)).border_1().border_color(gpui_kit::rgb(0xc7c7c7)).overflow_hidden()
}
/// The picture of a pattern: its 8 x 8 `tile` (a row per byte, the high bit the leftmost cell), a pixel to a cell, in `fg` on `bg`.
/// `None` (a pattern without a tile) draws `label` on white in its place. The xlsx cell patterns and the pptx fill patterns share it.
pub fn pattern_preview(tile:Option<[u8;8]>,fg:ibis_doc_model::Rgb,bg:ibis_doc_model::Rgb,label:SharedString)->AnyElement {
 let hsla=|c:ibis_doc_model::Rgb|-> Hsla {gpui_kit::rgb(c.0).into()};
 let frame=preview_frame().bg(hsla(bg));
 let Some(tile)=tile else {
  return frame.bg(gpui_kit::rgb(0xffffff)).flex().items_center().px(px(4.)).text_size(px(12.)).child(div().min_w_0().truncate().child(label)).into_any_element();
 };
 let color=hsla(fg);
 frame.child(canvas(|_,_,_|{},move|bounds,_,window,_|{
  let (w,h)=(f32::from(bounds.size.width).ceil() as usize,f32::from(bounds.size.height).ceil() as usize);
  for row in 0..h {
   let bits=tile[row%8];
   let mut x=0;
   while x<w {
    if bits&(0x80>>(x%8))==0 {x+=1;continue;}
    let start=x;
    while x<w && bits&(0x80>>(x%8))!=0 {x+=1;}
    window.paint_quad(gpui_kit::fill(Bounds::new(bounds.origin+point(px(start as f32),px(row as f32)),size(px((x-start) as f32),px(1.))),color));
   }
  }
 }).absolute().inset_0()).into_any_element()
}
