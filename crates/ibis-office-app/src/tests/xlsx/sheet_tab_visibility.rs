// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use crate::shell::scenario::Scenario;
use gpui_kit::{Bounds, Pixels, point, px, PlatformInput, MouseMoveEvent, MouseDownEvent, MouseUpEvent, MouseButton};
use ibis_doc_model::{Command, SheetCommand, SlideId};

fn geometry(s: &mut Scenario, index: usize) -> (Bounds<Pixels>, Bounds<Pixels>, f32) {
    s.with(|e, _| {
        let scroll = &e.wb().tab_scroll;
        (scroll.bounds(), scroll.bounds_for_item(index).expect("tab laid out"), f32::from(scroll.offset().x))
    })
}
fn click_visible_part(s: &mut Scenario, index: usize, id: SlideId) {
    let (view, tab, offset) = geometry(s, index);
    let left = f32::from(view.left()).max(f32::from(tab.left()) + offset);
    let right = f32::from(view.right()).min(f32::from(tab.right()) + offset);
    assert!(right > left, "part of target tab is visible");
    s.click(((left + right) / 2., f32::from(view.center().y)));
    s.settle();
    assert_eq!(s.with(|e, _| e.current_slide), id);
    assert!(s.with(|e, _| e.wb().tab_reveal.is_none()));
}
pub(super) fn selected_tab_reveals_neighbour() {
    let mut s = super::new_sheet();
    s.size(900., 720.);
    let ids = s.update(|e, w, cx| {
        for _ in 0..23 { e.insert_sheet(w, cx); }
        let ids: Vec<_> = e.presentation().slides.iter().map(|s| s.id).collect();
        let commands = ids.iter().enumerate().map(|(i, id)| Command::Sheet(SheetCommand::RenameSheet { slide: *id, name: format!("Sheet_{i:02}_data") })).collect();
        e.run_sheet(commands, cx);
        e.wb().tab_scroll.set_offset(point(px(0.), px(0.)));
        ids
    });
    let index = s.with(|e, _| {
        let scroll = &e.wb().tab_scroll;
        (0..ids.len()).find(|&i| {
            let b = scroll.bounds_for_item(i).unwrap();
            b.left() < scroll.bounds().right() && b.right() > scroll.bounds().right()
        }).expect("partly clipped right tab")
    });
    click_visible_part(&mut s, index, ids[index]);
    let (view, tab, offset) = geometry(&mut s, index);
    assert!((f32::from(view.right() - tab.right()) - offset - 28.).abs() < 1., "next tab is visible by the add-button width");
    assert!(tab.left() + px(offset) >= view.left());

    let index = 4;
    s.update(|e, _, cx| {
        let scroll = &e.wb().tab_scroll;
        let tab = scroll.bounds_for_item(index).unwrap();
        scroll.set_offset(point(scroll.bounds().left() - tab.left() - px(35.), px(0.)));
        cx.notify();
    });
    click_visible_part(&mut s, index, ids[index]);
    let (view, tab, offset) = geometry(&mut s, index);
    assert!((f32::from(tab.left() - view.left()) + offset - 28.).abs() < 1.);

    s.update(|e, _, cx| { e.wb().tab_scroll.set_offset(point(px(0.), px(0.))); cx.notify(); });
    click_visible_part(&mut s, 0, ids[0]);
    assert_eq!(geometry(&mut s, 0).2, 0., "a fully visible tab stays put");

    s.size(420., 720.);
    let index = 10;
    s.update(|e, _, cx| e.run_sheet(vec![Command::Sheet(SheetCommand::RenameSheet { slide: ids[index], name: "あ".repeat(31) })], cx)); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    s.update(|e, _, cx| {
        let scroll = &e.wb().tab_scroll;
        let tab = scroll.bounds_for_item(index).unwrap();
        scroll.set_offset(point(scroll.bounds().right() - tab.left() - px(20.), px(0.)));
        cx.notify();
    });
    click_visible_part(&mut s, index, ids[index]);
    let (view, tab, offset) = geometry(&mut s, index);
    assert!(tab.size.width > view.size.width);
    assert!((f32::from(tab.left() - view.left()) + offset).abs() < 1., "long tab starts at viewport left");

    // Insertion uses the same post-layout reveal, with the new child present.
    s.click((f32::from(view.right()) + 15., f32::from(view.center().y)));
    s.settle();
    let count = s.with(|e, _| e.presentation().slides.len());
    assert_eq!(count, ids.len() + 1);
    let selected = s.with(|e, _| e.presentation().slide_index(e.current_slide).unwrap());
    let (view, tab, offset) = geometry(&mut s, selected);
    assert!(tab.left() + px(offset) >= view.left() && tab.right() + px(offset) <= view.right());
}

pub(super) fn double_click_requires_same_sheet() {
    let mut s = super::new_sheet();
    s.size(900., 720.);
    let ids = s.update(|e, w, cx| {
        for _ in 0..23 { e.insert_sheet(w, cx); }
        let ids: Vec<_> = e.presentation().slides.iter().map(|s| s.id).collect();
        let commands = ids.iter().enumerate().map(|(i, id)| Command::Sheet(SheetCommand::RenameSheet { slide: *id, name: format!("Sheet_{i:02}_data") })).collect();
        e.run_sheet(commands, cx);
        ids
    });
    for right in [true, false] {
        let index = 10;
        s.update(|e, _, cx| {
            let scroll = &e.wb().tab_scroll;
            let tab = scroll.bounds_for_item(index).unwrap();
            let offset = if right { scroll.bounds().right() + px(12.) - tab.right() } else { scroll.bounds().left() - px(12.) - tab.left() };
            scroll.set_offset(point(offset, px(0.)));
            cx.notify();
        });
        let (view, _, _) = geometry(&mut s, index);
        let position = point(if right { view.right() - px(4.) } else { view.left() + px(4.) }, view.center().y);
        s.click((f32::from(position.x), f32::from(position.y)));
        assert_eq!(s.with(|e, _| e.current_slide), ids[index]);
        // The first click scrolls a neighbour underneath exactly the same pointer.
        // Dispatch the OS's second click, including its click_count=2.
        s.dispatch(vec![
            PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),
            PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 2, ..Default::default() }),
            PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, click_count: 2, ..Default::default() }),
        ]);
        assert_eq!(s.with(|e, _| e.current_slide), ids[if right { index + 1 } else { index - 1 }]);
        assert_eq!(s.with(|e, _| e.wb().jump.renaming), None, "two different tabs must not begin renaming");
    }
    s.update(|e, _, cx| { e.wb().tab_scroll.set_offset(point(px(0.), px(0.))); cx.notify(); });
    let (view, tab, offset) = geometry(&mut s, 0);
    s.double_click((f32::from(tab.center().x) + offset, f32::from(view.center().y)));
    assert_eq!(s.with(|e, _| e.wb().jump.renaming), Some(ids[0]), "ordinary double click still renames the same tab");
}
