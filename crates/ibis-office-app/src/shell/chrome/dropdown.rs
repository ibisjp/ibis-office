// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! One button/list lifecycle. Specialized dropdowns supply item renderers, not popovers.
use super::*;

#[derive(Clone)]
pub(crate) enum ItemContent {
    Font(SharedString),
    Palette(Vec<u32>),
    Custom(Rc<dyn Fn()->AnyElement>),
}
#[derive(Clone,Default)]
pub(crate) struct DropdownItem {
    pub label: SharedString,
    pub icon: Option<&'static str>,
    pub checked: bool,
    pub disabled: bool,
    pub on_select: Option<Handler>,
    pub on_app_select: Option<Rc<dyn Fn(&mut Window,&mut App)>>,
    pub content: Option<ItemContent>,
}
impl DropdownItem {
    pub fn new(label:impl Into<SharedString>)->Self { Self {label:label.into(),icon:None,checked:false,disabled:false,on_select:None,on_app_select:None,content:None} }
    pub fn icon(mut self,icon:&'static str)->Self {self.icon=Some(icon);self}
    pub fn checked(mut self,value:bool)->Self {self.checked=value;self}
    pub fn on_select(mut self,handler:Handler)->Self {self.on_select=Some(handler);self}
    pub fn on_app_select(mut self,handler:impl Fn(&mut Window,&mut App)+'static)->Self {self.on_app_select=Some(Rc::new(handler));self}
    pub fn menu(self)->MenuItem {
        let mut item=MenuItem::new("dropdown.item",self.label);
        item.icon=self.icon;item.checked=self.checked;item.disabled=self.disabled;item.handler=self.on_select;item.dropdown_content=self.content;item
    }
}
#[derive(Clone,Copy)]
pub(crate) enum ListWidth { Content, Button, Fixed(Pixels) }
#[derive(Clone,Copy)]
pub(crate) struct Options {pub checkmarks:bool,pub width:ListWidth,pub offset_x:Pixels,pub search:bool,pub hover_reveal:Option<&'static str>}
impl Default for Options {fn default()->Self {Self{checkmarks:true,width:ListWidth::Content,offset_x:px(0.),search:true,hover_reveal:None}}}

#[derive(IntoElement)]
pub(crate) struct Dropdown {
    id:&'static str, button:Button, entries:Vec<MenuEntry>,single_select:bool,index:usize,
    editor:Option<WeakEntity<SlideEditor>>,options:Options,button_width:Option<Pixels>,application_items:Vec<DropdownItem>,fill:bool,
}
impl Dropdown {
    pub fn new(id:&'static str,label:impl Into<SharedString>,items:Vec<DropdownItem>,editor:&WeakEntity<SlideEditor>)->Self {
        Self::with_button(id,dropdown_button(id,label.into()).w(FIELD_WIDTH),items.into_iter().map(|i|i.menu().into()).collect(),true,0,editor).button_width(FIELD_WIDTH)
    }
    pub fn from_entries(id:&'static str,label:impl Into<SharedString>,entries:Vec<MenuEntry>,single_select:bool,index:usize,editor:&WeakEntity<SlideEditor>)->Self {
        let button_id=if index==0 {id.to_owned()}else{format!("{id}-{index}")};
        Self::with_button(id,dropdown_button(SharedString::from(button_id),label.into()),entries,single_select,index,editor).button_width(FIELD_WIDTH)
    }
    /// Adapts menu-model item arrays and compound controls to the same dropdown lifecycle.
    pub fn with_button(id:&'static str,button:Button,entries:Vec<MenuEntry>,single_select:bool,index:usize,editor:&WeakEntity<SlideEditor>)->Self {
        Self{id,button,entries,single_select,index,editor:Some(editor.clone()),options:Options::default(),button_width:None,application_items:Vec::new(),fill:false}
    }
    pub fn application(id:&'static str,label:impl Into<SharedString>,items:Vec<DropdownItem>)->Self {
        Self{id,button:dropdown_button(id,label.into()).w(FIELD_WIDTH),entries:Vec::new(),single_select:true,index:0,
            editor:None,options:Options::default(),button_width:Some(FIELD_WIDTH),application_items:items,fill:false}
    }
    pub fn checkmarks(mut self,show:bool)->Self {self.options.checkmarks=show;self}
    pub fn reveal_on_hover(mut self,group:&'static str)->Self{self.options.hover_reveal=Some(group);self}
    pub fn searchable(mut self,search:bool)->Self {self.options.search=search;self}
    pub fn offset_x(mut self,offset:Pixels)->Self {self.options.offset_x=offset;self}
    pub fn list_width(mut self,width:ListWidth)->Self {self.options.width=width;self}
    pub fn button_width(mut self,width:Pixels)->Self {self.button=self.button.w(width);self.button_width=Some(width);self}
    pub fn instance(mut self,index:usize)->Self {self.index=index;self}
    /// The button fills its parent (the control cell of a [`dropdown_row`]) so its right edge is the row's, like a `full_width` text field.
    pub fn fill_width(mut self)->Self {self.button=self.button.w_full();self.fill=true;self}
}
impl RenderOnce for Dropdown {
    fn render(self,_:&mut Window,_:&mut App)->impl IntoElement {
        gpui_kit::component::form_controls::other(self.button.is_disabled(),false);
        if let Some(editor)=self.editor {
            div().when_some(self.button_width,|d,w|d.w(w).flex_none()).when(self.fill,|d|d.w_full()).child(super::render_dropdown(self.id,self.button,self.entries,self.single_select,self.index,&editor,self.options)).into_any_element()
        } else {
            // Standalone windows have no document editor; their callbacks operate on the app.
            // Item construction, sizing and button/popup ownership still live in this component.
            let items=self.application_items;let options=self.options;let minimum=self.button_width.unwrap_or(FIELD_WIDTH);
            div().w(minimum).flex_none().child(self.button.dropdown_menu(move|menu,w,_| {
                let width=match options.width {ListWidth::Button=>f32::from(minimum),ListWidth::Fixed(w)=>f32::from(w),ListWidth::Content=>dropdown_list::content_width(&items,|i|i.label.to_string(),|i|dropdown_list::label_width(w,&i.label,13.)+68.,f32::from(minimum))}.min(dropdown_list::MAX_WIDTH);
                items.iter().enumerate().fold(menu.min_w(px(width)).max_w(px(width)),|menu,(index,item)| {
                    let callback=item.on_app_select.clone();let checked=item.checked;let disabled=item.disabled;let icon_name=item.icon;let label=item.label.clone();
                    let row=gpui_kit::component::menu::PopupMenuItem::element(move|_,_| {
                        dropdown_list::option(("application-dropdown-row",index),checked&&options.checkmarks,false,!disabled)
                            .child(div().w(px(20.)).flex_none().children(dropdown_list::checkmark(checked,options.checkmarks,rgb(TEXT).into()))
                                .when(!checked||!options.checkmarks,|d|d.when_some(icon_name,|d,i|d.child(icon(i,16.,rgb(TEXT).into())))))
                            .child(div().flex_1().truncate().child(label.clone()))
                    }).disabled(disabled).on_click(move|_,w,cx|{if let Some(callback)=&callback{callback(w,cx);}});
                    menu.item(row)
                })
            })).into_any_element()
        }
    }
}

pub(crate) fn is_font_item_id(id:&str)->bool {
    matches!(id,"xlsx.font"|"pptx.font"|"docx.font"|"font.recent.item"|"docx-font-latin-list"|"docx-font-east-asian-list"|"docx-font-complex-script-list"|"docx-style-font-list") || id.starts_with("panel-font-")
}
pub(crate) fn font_item(family:&str,label:SharedString,color:Hsla)->AnyElement {
    if let Some(preview)=crate::shell::font_preview::get_or_render_preview(family,&label) {
        let fallback=label.clone();
        div().flex_1().truncate().child(img(preview.image).w(px(preview.width)).h(px(preview.height))
            .with_loading(move||div().text_size(px(13.)).text_color(color).child(fallback.clone()).into_any_element())).into_any_element()
    }else{div().flex_1().truncate().text_size(px(13.)).text_color(color).child(label).into_any_element()}
}
pub(crate) fn color_palette_item(colors:&[u32])->AnyElement {
    let width=colors.len() as f32*16.+colors.len().saturating_sub(1) as f32*3.;
    h_flex().gap(px(3.)).w(px(width)).max_w_full().overflow_hidden().children(colors.iter().map(|c|div().size(px(16.)).flex_none().rounded(px(3.)).bg(rgb(*c)))).into_any_element()
}
#[derive(IntoElement)]
pub(crate) struct FontDropdown(Dropdown);
impl FontDropdown {
    pub fn new(id:&'static str,current:&str,entries:Vec<MenuEntry>,index:usize,editor:&WeakEntity<SlideEditor>)->Self {
        let items=flatten_menu_entries(&entries).into_iter().map(|mut i| {
            if !i.id.starts_with("font.section."){i.dropdown_content=Some(ItemContent::Font(i.label.clone()));} i.into()
        }).collect();
        Self(Dropdown::with_button(id,dropdown_button(id,crate::shell::i18n::font_label(current)).w(FONT_DROPDOWN_WIDTH),items,true,index,editor).button_width(FONT_DROPDOWN_WIDTH))
    }
    pub fn button(mut self,button:Button)->Self {self.0.button=button;self.0.button_width=None;self}
}
impl RenderOnce for FontDropdown {fn render(self,_:&mut Window,_:&mut App)->impl IntoElement {self.0}}
#[derive(Clone)]
pub(crate) struct ColorPaletteItem {pub label:SharedString,pub colors:Vec<u32>,pub on_select:Handler}
impl ColorPaletteItem {
    pub fn single(label:SharedString,color:u32,on_select:Handler)->Self {Self{label,colors:vec![color],on_select}}
}
#[derive(IntoElement)]
pub(crate) struct ColorPaletteDropdown {body:PaletteBody}
enum PaletteBody { List(Dropdown), Grid {id:&'static str,button:Button,entries:Rc<Vec<MenuEntry>>,editor:WeakEntity<SlideEditor>} }
impl ColorPaletteDropdown {
    pub fn new(id:&'static str,items:Vec<ColorPaletteItem>,selected:usize,editor:&WeakEntity<SlideEditor>)->Self {
        let current=selected.min(items.len().saturating_sub(1));
        let preview=items.get(current).map(|i|color_palette_item(&i.colors)).unwrap_or_else(||div().into_any_element());
        let label=items.get(current).map(|i|i.label.clone()).unwrap_or_default();
        let entries=items.into_iter().enumerate().map(|(index,i)| {
            let mut item=DropdownItem::new(i.label).checked(index==current).on_select(i.on_select);
            item.content=Some(ItemContent::Palette(i.colors));item.menu().into()
        }).collect();
        Self{body:PaletteBody::List(Dropdown::with_button(id,dropdown_button(id,preview).w(FIELD_WIDTH).tooltip(label),entries,true,0,editor).checkmarks(false).list_width(ListWidth::Fixed(FIELD_WIDTH)).button_width(FIELD_WIDTH))}
    }
    pub fn from_palette(id:&'static str,button:Button,entries:Vec<MenuEntry>,editor:&WeakEntity<SlideEditor>)->Self {
        Self{body:PaletteBody::Grid{id,button,entries:Rc::new(entries),editor:editor.clone()}}
    }
    pub fn button_width(mut self,width:Pixels)->Self {if let PaletteBody::List(list)=self.body{self.body=PaletteBody::List(list.button_width(width).list_width(ListWidth::Fixed(width)));}self}
    pub fn checkmarks(mut self,show:bool)->Self {if let PaletteBody::List(list)=self.body{self.body=PaletteBody::List(list.checkmarks(show));}self}
    /// See [`Dropdown::fill_width`].
    pub fn fill_width(mut self)->Self {if let PaletteBody::List(list)=self.body{self.body=PaletteBody::List(list.fill_width());}self}
}
impl RenderOnce for ColorPaletteDropdown {
    fn render(self,_:&mut Window,_:&mut App)->impl IntoElement {
        match self.body {
            PaletteBody::List(list)=>list.into_any_element(),
            PaletteBody::Grid{id,button,entries,editor}=>div().id(id).child(button.dropdown_menu(move|m,w,cx|fill_menu(m,&entries,&editor,w,cx))).into_any_element(),
        }
    }
}
