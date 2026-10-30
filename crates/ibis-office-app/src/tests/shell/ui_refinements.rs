// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{Scenario, button_probe as buttons};
use crate::docx::merge_panel::MergeDialog;
use gpui_kit::*;
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::table_pointer_reorder_cancel_and_commit", reorder),
        ("shell::pane_actions_flow_and_stick", pane_actions),
        ("shell::panel_dropdown_filter_enter", dropdown_enter),
        ("shell::panel_dialog_eight_way_resize", eight_way_resize),
        ("shell::tab_control_underline_and_hover", tab_underline),
    ]
}
fn reorder() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 800.);
    s.update(|e, w, cx| {
        e.open_merge(w, cx);
        e.set_merge_data(
            vec!["Name".into()],
            vec![
                vec!["Alice".into()],
                vec!["Bob".into()],
                vec!["Carol".into()],
            ],
            cx,
        );
        e.open_merge_task(MergeDialog::Recipients, cx);
    });
    let state = s.with(|e, _| {
        e.doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .row_reorder
            .clone()
    });
    let bounds = |id| state.borrow().row_bounds(id).unwrap();
    let first = bounds(0);
    let last = bounds(2);
    let at = point(first.origin.x + px(60.), first.center().y);
    let end = point(at.x, last.center().y + px(4.));
    s.hover((f32::from(at.x), f32::from(at.y)));
    assert_eq!(state.borrow().hovered(), Some(0));
    s.hover((f32::from(first.origin.x + px(16.)), f32::from(at.y)));
    assert_eq!(
        state.borrow().hovered(),
        None,
        "checkbox cell is not a row hover/drag surface"
    );
    let before = s.with(|e, _| {
        e.doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .records
            .clone()
    });
    s.dispatch(vec![
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            click_count: 1,
            ..Default::default()
        }),
        PlatformInput::MouseMove(MouseMoveEvent {
            position: end,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        }),
    ]);
    assert_eq!(state.borrow().dragging, Some(0));
    assert_eq!(state.borrow().order, vec![1, 2, 0]);
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .records
            .clone()),
        before,
        "live positions are a cancellable draft"
    );
    s.screenshot("row-lifted-during-drag");
    assert!(s.with(|e, _| {
        e.active_table_reorder
            .as_ref()
            .is_some_and(|a| std::rc::Rc::ptr_eq(a, &state))
    }));
    s.key("escape");
    assert_eq!(state.borrow().dragging, None);
    assert_eq!(state.borrow().order, vec![0, 1, 2]);
    assert!(
        s.with(|e, _| e.panel_dialog_is_modal()),
        "Escape cancels the drag, not the dialog"
    );
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent {
        button: MouseButton::Left,
        position: end,
        click_count: 1,
        ..Default::default()
    })]);
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .records
            .clone()),
        before
    );
    s.wait(180);
    let first = bounds(0);
    let last = bounds(2);
    s.drag(
        (
            f32::from(first.origin.x + px(60.)),
            f32::from(first.center().y),
        ),
        (
            f32::from(first.origin.x + px(60.)),
            f32::from(last.center().y + px(4.)),
        ),
    );
    s.wait(180);
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .records
            .iter()
            .map(|r| r[0].clone())
            .collect::<Vec<_>>()),
        vec!["Bob", "Carol", "Alice"]
    );
    s.screenshot("reordered-recipients");
}
fn pane_actions() {
    use crate::shell::chrome::{self, FORCED_SIDEBAR, SidebarPanel};
    fn panel(
        e: &mut crate::shell::editor::SlideEditor,
        _: &mut Window,
        _: &mut Context<crate::shell::editor::SlideEditor>,
    ) -> Option<SidebarPanel> {
        let count = if e.status.is_some() { 80 } else { 1 };
        let body = gpui_kit::component::v_flex()
            .children((0..count).map(|_| div().h(px(24.)).flex_none().child("Content")));
        Some(SidebarPanel::with_footer(
            "Adaptive actions…",
            body.into_any_element(),
            chrome::action_bar()
                .action(chrome::ActionDefinition::new("adaptive-close", "Close", chrome::ActionButtonKind::Normal)),
        ))
    }
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    FORCED_SIDEBAR.with(|p| {
        p.set(Some((
            panel,
            "adaptive-actions",
            crate::shell::panel_dialog::PanelMode::Sidebar,
        )))
    });
    s.update(|e, _, cx| {
        e.sidebar_open = true;
        e.status = None;
        cx.notify();
    });
    let compact = buttons::bounds("adaptive-close").unwrap();
    s.update(|e, _, cx| {
        e.status = Some("long".into());
        cx.notify();
    });
    let sticky = buttons::bounds("adaptive-close").unwrap();
    assert!(
        sticky.origin.y > compact.origin.y + px(100.),
        "short content flows; overflowing content keeps actions at the pane bottom"
    );
    s.wheel(
        (
            f32::from(sticky.origin.x),
            f32::from(sticky.origin.y - px(60.)),
        ),
        -400.,
    );
    assert_eq!(
        buttons::bounds("adaptive-close").unwrap().origin.y,
        sticky.origin.y
    );
    FORCED_SIDEBAR.with(|p| p.set(None));
}

fn dropdown_enter() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, w, cx| e.open_merge(w, cx));
    buttons::click(&mut s, "merge-kind");
    s.screenshot("small-panel-dropdown-without-search");
    s.key("down");s.key("down");s.key("down");s.key("enter");
    assert_eq!(s.with(|e, _| e.flow().unwrap().mail_merge_type.clone()).as_deref(), Some("catalog"));
    assert!(s.with(|e, _| e.toolbar_search_open.is_none()));
}

fn eight_way_resize() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 800.);
    s.update(|e,w,cx| e.open_page_setup_panel(w,cx));
    // Each real edge/corner gesture must preserve the opposite edges.
    for (name, x, y, dx, dy, west, north, east, south) in [
        ("n",0.5,0.,0.,-20.,false,true,false,false),
        ("ne",1.,0.,30.,-20.,false,true,true,false),
        ("e",1.,0.5,30.,0.,false,false,true,false),
        ("se",1.,1.,30.,20.,false,false,true,true),
        ("s",0.5,1.,0.,20.,false,false,false,true),
        ("sw",0.,1.,-30.,20.,true,false,false,true),
        ("w",0.,0.5,-30.,0.,true,false,false,false),
        ("nw",0.,0.,-30.,-20.,true,true,false,false),
    ] {
        s.update(|e,_,cx| {
            e.panel_dialog.bounds=Some(Bounds::new(point(px(300.),px(150.)),size(px(500.),px(400.))));
            e.panel_dialog.user_resized=true;e.panel_dialog.user_moved=true;cx.notify();
        });
        let before=s.with(|e,_|e.panel_dialog.bounds.unwrap());
        let at=point(before.origin.x+px(2.)+(before.size.width-px(4.))*x,
            before.origin.y+px(2.)+(before.size.height-px(4.))*y);
        let to=at+point(px(dx),px(dy));
        s.dispatch(vec![PlatformInput::MouseDown(MouseDownEvent{button:MouseButton::Left,position:at,click_count:1,..Default::default()}),
            PlatformInput::MouseMove(MouseMoveEvent{position:to,pressed_button:Some(MouseButton::Left),..Default::default()})]);
        let after=s.with(|e,_|e.panel_dialog.bounds.unwrap());
        assert!((after.origin.x-before.origin.x-px(if west{dx}else{0.})).abs()<px(1.),"{name} left");
        assert!((after.origin.y-before.origin.y-px(if north{dy}else{0.})).abs()<px(1.),"{name} top");
        assert!((after.size.width-before.size.width-px(if west{-dx}else if east{dx}else{0.})).abs()<px(1.),"{name} width: before={before:?}, after={after:?}");
        assert!((after.size.height-before.size.height-px(if north{-dy}else if south{dy}else{0.})).abs()<px(1.),"{name} height: before={before:?}, after={after:?}");
        // Release outside the dialog too; the gesture must never remain captured.
        s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent{button:MouseButton::Left,position:point(px(20.),px(20.)),click_count:1,..Default::default()})]);
        assert!(s.with(|e,_| e.panel_dialog.drag.is_none()),"{name} releases outside");
    }
    s.screenshot("eight-way-resized-dialog");
}

fn tab_underline() {
    let _probe=buttons::Probe::new();
    let mut s=Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200.,800.);
    let index=s.with(|e,_|e.slide().shapes.iter().position(|shape|matches!(shape.kind,ibis_doc_model::ShapeKind::Chart(_))).unwrap());
    let at=s.shape_position(index);s.click(at);s.menu("xlsx.chart-format");
    let bounds=|index| {
        let id=buttons::all().into_iter().find(|id|id.contains("chart-format-tab")&&id.ends_with(&format!(", {index})"))).unwrap();
        buttons::bounds(&id).unwrap()
    };
    let selected=bounds(0);let other=bounds(1);
    assert_eq!(selected.size.height,px(32.));
    s.hover((f32::from(selected.center().x),f32::from(selected.center().y)));
    let path=s.screenshot("tab-underlines-full-hover");
    let picture=image::open(path).unwrap().to_rgb8();let scale=picture.width() as f32/1200.;
    let band=|b:Bounds<Pixels>,blue:bool| {
        let x=(f32::from(b.center().x)*scale).round() as u32;
        let y=f32::from(b.origin.y+b.size.height)-4.;
        ((y*scale).ceil() as u32..((y+4.)*scale).floor() as u32).filter(|&y| {
            let p=picture.get_pixel(x,y).0;
            if blue { p[2] as i32-p[0] as i32>50 && p[2] as i32-p[1] as i32>25 }
            else { p[0]>=205&&p[0]<=235&&(p[2] as i32-p[0] as i32).abs()<12 }
        }).collect::<Vec<_>>()
    };
    let blue=band(selected,true);let gray=band(other,true);
    assert!((blue.len() as f32-4.*scale).abs()<=1.,"selected underline is 4px: {blue:?}");
    assert!((gray.len() as f32-1.*scale).abs()<=1.,"full-width baseline is blue and 1px: {gray:?}");
    assert_eq!(blue.last(),gray.last(),"underline bottom edges align");
    let x=((f32::from(selected.origin.x)+5.)*scale).round() as u32;
    let y=((f32::from(selected.origin.y+selected.size.height)-6.)*scale).round() as u32;
    assert!(picture.get_pixel(x,y).0.into_iter().all(|channel|(225..=245).contains(&channel)),"hover fills the tab down to its underline");
}
