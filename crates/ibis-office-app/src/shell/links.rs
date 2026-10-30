// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shared external-link dispatch and hyperlink context menus.
use super::{
    chrome::{MenuEntry, fill_menu},
    editor::SlideEditor,
};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::*;

#[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
thread_local! { pub(crate) static CAPTURE_URLS:std::cell::RefCell<Option<Vec<String>>>=const {std::cell::RefCell::new(None)}; }

pub(crate) fn open_external_url(url: &str, cx: &mut App) {
    #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
    if CAPTURE_URLS.with(|slot| {
        let mut s = slot.borrow_mut();
        if let Some(v) = s.as_mut() {
            v.push(url.to_string());
            true
        } else {
            false
        }
    }) {
        return;
    }
    cx.open_url(url);
}

pub(crate) struct LinkPopup {
    pub menu: Entity<PopupMenu>,
    pub position: Point<Pixels>,
    _dismiss: Subscription,
}
impl SlideEditor {
    pub(crate) fn open_hyperlink_context(
        &mut self,
        position: Point<Pixels>,
        entries: Vec<MenuEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = cx.weak_entity();
        let menu = PopupMenu::build(window, cx, move |menu, w, cx| {
            fill_menu(menu, &entries, &editor, w, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |e, menu, _: &DismissEvent, w, cx| {
            let restore=menu.read(cx).focus_handle(cx).contains_focused(w,cx);
            e.link_popup = None;
            if restore{e.focus_handle.focus(w, cx);}
            cx.notify();
        });
        use gpui_kit::Focusable;
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.link_popup = Some(LinkPopup {
            menu,
            position,
            _dismiss: dismiss,
        });
        cx.notify();
    }
    pub(crate) fn render_hyperlink_context(&self) -> Option<AnyElement> {
        self.link_popup.as_ref().map(|p| {
            deferred(
                anchored()
                    .position(p.position)
                    .snap_to_window_with_margin(px(8.))
                    .child(p.menu.clone()),
            )
            .with_priority(3)
            .into_any_element()
        })
    }
    /// Map a document hit once at the workspace boundary. Child render layers
    /// (selection marks, page canvases) must not swallow hyperlink gestures.
    pub(crate) fn doc_link_under_pointer(&self,p:Point<Pixels>)->Option<ibis_doc_model::FlowPos>{
        if !self.is_document()||self.panel_dialog_is_modal(){return None;}
        let ws=self.workspace_bounds?;if !ws.contains(&p){return None;}
        let top=self.doc_split.is_some_and(|ratio|p.y<ws.origin.y+(ws.size.height-px(6.))*ratio);
        let sizes=self.page_sizes();
        for(page,size)in sizes.iter().enumerate(){
            let(origin,k)=self.page_origin_pane(page,top)?;
            let bounds=Bounds::new(origin,gpui::size(px(size.width as f32*k/ibis_doc_model::EMU_PER_POINT as f32),px(size.height as f32*k/ibis_doc_model::EMU_PER_POINT as f32)));
            if !bounds.contains(&p){continue;}
            let(x,y)=self.page_point_pane(page,p,top)?;
            if self.floating_text_box_at(page,x,y).is_some(){return None;}
            if let Some(pos)=self.doc_link_position_at_point(page,x,y){return Some(pos);}
        }None
    }
    pub(crate) fn handle_document_link_pointer(&mut self,event:&MouseDownEvent,w:&mut Window,cx:&mut Context<Self>)->bool{
        let Some(pos)=self.doc_link_under_pointer(event.position)else{return false;};
        if event.button==MouseButton::Right{
            if let Some((a,b,_))=self.doc_link_range(pos){self.set_caret(a,false,cx);self.set_caret(b,true,cx);self.open_hyperlink_context(event.position,self.doc_hyperlink_menu(),w,cx);return true;}
        }else if event.button==MouseButton::Left&&event.click_count==1&&!event.modifiers.shift{
            self.focus_handle.focus(w,cx);return self.follow_link(pos,cx);
        }false
    }
    pub(crate) fn update_link_hover(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button.is_some() {
            return;
        }
        let next = match self.format.kind() {
            ibis_pptx::Kind::Document =>self.doc_link_under_pointer(event.position).is_some(),
            ibis_pptx::Kind::Workbook => self.cell_under(event.position).is_some_and(|(c, r)| {
                self.current_sheet().is_some_and(|(_, s)| {
                    s.hyperlinks.iter().any(|h| {
                        h.range.first_row <= r
                            && r <= h.range.last_row
                            && h.range.first_column <= c
                            && c <= h.range.last_column
                    })
                })
            }),
            ibis_pptx::Kind::Presentation => self
                .slide_layout()
                .is_some_and(|l| self.slide_link_at(l.to_emu(event.position)).is_some()),
        };
        if self.link_hover != next {
            self.link_hover = next;
            cx.notify();
        }
    }
}
