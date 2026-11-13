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
thread_local! { pub static CAPTURE_URLS:std::cell::RefCell<Option<Vec<String>>>=const {std::cell::RefCell::new(None)}; }

pub fn open_external_url(url: &str, cx: &mut App) {
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
    crate::external::open_url(url, cx);
}

pub struct LinkPopup {
    pub menu: Entity<PopupMenu>,
    pub position: Point<Pixels>,
    _dismiss: Subscription,
}
impl SlideEditor {
    pub fn open_hyperlink_context(
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
    pub fn update_link_hover(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button.is_some() {
            return;
        }
        let next = crate::format::with_format!(self, link_under_pointer(self, event.position));
        if self.link_hover != next {
            self.link_hover = next;
            cx.notify();
        }
    }
}
