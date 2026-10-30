// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Real nested popup placement, including the relative offsets of the component's submenu wrapper.
use crate::shell::{
    chrome::{self, FORCED_SIDEBAR, SidebarPanel},
    editor::SlideEditor,
    scenario::Scenario,
};
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
struct State {
    popup: Option<Entity<PopupMenu>>,
    root: Rc<Cell<Option<Bounds<Pixels>>>>,
    leaf: Rc<Cell<Option<Bounds<Pixels>>>>,
}
thread_local! {static STATE:RefCell<Option<State>>=const{RefCell::new(None)};}
fn panel(
    _: &mut SlideEditor,
    window: &mut Window,
    cx: &mut Context<SlideEditor>,
) -> Option<SidebarPanel> {
    let owner = cx.weak_entity();
    let viewport = window.viewport_size();
    let (popup,root)=STATE.with(|slot|{
        let mut slot=slot.borrow_mut();let state=slot.as_mut().unwrap();
        if state.popup.is_none(){
            let leaf=state.leaf.clone();
            state.popup=Some(PopupMenu::build(window,cx,move|menu,w,cx|{
                menu.min_w(px(crate::shell::menu_bar::fitted_menu_width(280.,viewport.width.into()))).scrollable(false)
                    .submenu("Submenu",w,cx,move|menu,w,cx|{
                        let entries=vec![chrome::MenuItem::new("long-viewport-item","An extremely wide menu item label that must be clipped and never overflow its popup or the screen".repeat(4)).into()];
                        let menu=chrome::fill_menu(menu,&entries,&owner,w,cx);
                        let record=leaf.clone();
                        menu.item(PopupMenuItem::element(move|_,_|{
                            let record=record.clone();
                            // The bounds come from a canvas filling the marker, as PopupMenu measures its own rows
                            // (item_bounds): `on_prepaint` on the row read 22.5px below where the canvas and the
                            // row bounds show the marker (569 vs 591.5 at 900x600), outside what is painted.
                            div().w_full().h(px(20.)).relative().child("Marker").child(canvas(|_,_,_|(),move|b,_,_,_|record.set(Some(b))).absolute().inset_0())
                        }))
                    })
            }));
        }
        (state.popup.as_ref().unwrap().clone(),state.root.clone())
    });
    let body = deferred(
        anchored()
            .position(point(viewport.width - px(10.), viewport.height - px(15.)))
            .snap_to_window()
            .child(
                div().relative().child(popup).child(
                    canvas(|_, _, _| (), move |b, _, _, _| root.set(Some(b)))
                        .absolute()
                        .inset_0(),
                ),
            ),
    )
    .with_priority(2);
    Some(SidebarPanel {
        title: "Menu viewport verification".into(),
        body: div().child(body).into_any_element(),
    })
}
pub(crate) fn run() {
    for (width, height) in [(900., 600.), (320., 240.), (260., 180.)] {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        s.size(width, height);
        let root = Rc::new(Cell::new(None));
        let leaf = Rc::new(Cell::new(None));
        STATE.with(|slot| {
            *slot.borrow_mut() = Some(State {
                popup: None,
                root: root.clone(),
                leaf: leaf.clone(),
            })
        });
        FORCED_SIDEBAR.with(|slot| {
            slot.set(Some((
                panel,
                "menu-viewport",
                crate::shell::panel_dialog::PanelMode::Sidebar,
            )))
        });
        s.update(|e, _, cx| {
            e.sidebar_open = true;
            cx.notify();
        });
        let parent = root.get().expect("root menu rendered");
        s.hover((
            (parent.origin.x + parent.size.width / 2.).into(),
            (parent.origin.y + parent.size.height / 2.).into(),
        ));
        s.wait(350);
        if width < 600. {
            // A cascade that fits neither right nor left of its parent does not cover the parent on hover
            // (fork gpui-component 27eafbc, `explicit_only`); a click on the row opens it.
            assert!(leaf.get().is_none(), "hover alone must not cover the parent at {width}x{height}");
            s.click((
                (parent.origin.x + parent.size.width / 2.).into(),
                (parent.origin.y + parent.size.height / 2.).into(),
            ));
            s.wait(350);
        }
        let child = leaf.get().unwrap_or_else(|| panic!("hover opens the real submenu at {width}x{height}"));
        assert!(
            child.origin.x >= px(0.) && child.origin.y >= px(0.),
            "negative popup: {child:?}"
        );
        assert!(
            child.right() <= px(width) && child.bottom() <= px(height),
            "submenu overflows {width}x{height}: {child:?}"
        );
        assert!(child.size.width <= px(chrome::MENU_ITEM_MAX_WIDTH));
        assert_eq!(
            chrome::menu_width_for_entries(&[
                chrome::MenuItem::new("long", "W".repeat(1000)).into()
            ]),
            chrome::MENU_ITEM_MAX_WIDTH
        );
        s.screenshot(&format!("submenu-viewport-{width}"));
        FORCED_SIDEBAR.with(|slot| slot.set(None));
        STATE.with(|slot| {
            slot.borrow_mut().take();
        });
        drop(s);
    }
}
