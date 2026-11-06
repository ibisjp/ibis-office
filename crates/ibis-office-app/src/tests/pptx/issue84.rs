// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! issue #84: the guides (ガイド線) of the view and of a master or layout page, and managing the layouts in the slide master view.
use crate::shell::scenario::Scenario;
use ibis_doc_model::SlideGuide;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue84_guides_are_seeded_added_dragged_removed_and_undone", guides_view),
        ("pptx::issue84_shapes_dragged_snap_to_the_guides", guides_snap),
        ("pptx::issue84_master_page_guides_are_kept_when_the_view_is_left", guides_in_master_view),
        ("pptx::issue84_layouts_are_renamed_copied_deleted_and_applied_in_the_master_view", layouts_in_master_view),
    ]
}

fn guide_point(s: &mut Scenario, g: &SlideGuide) -> (f32, f32) {
    s.with(|e, _| {
        let layout = e.slide_layout().expect("laid out");
        let size = e.presentation().size_of(e.slide());
        // On the guide, a quarter of the way along it (not where the middle guides cross).
        let (x, y) = if g.horizontal { (size.width / 4, g.emu()) } else { (g.emu(), size.height / 4) };
        (f32::from(layout.origin.x + layout.to_px(x)), f32::from(layout.origin.y + layout.to_px(y)))
    })
}

fn guides_view() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    assert!(s.with(|e, _| e.presentation().guides.is_empty()));
    // Shown with none in the file: PowerPoint's two through the middle.
    s.menu("pptx.guides");
    let guides = s.with(|e, _| e.presentation().guides.clone());
    assert_eq!(guides.len(), 2, "{guides:?}");
    let size = s.with(|e, _| e.presentation().size_of(e.slide()));
    assert!(guides.iter().any(|g| g.horizontal && g.emu() == size.height / 2));
    assert!(guides.iter().any(|g| !g.horizontal && g.emu() == size.width / 2));
    // 縦のガイドを追加: a third, not on the middle one.
    s.menu("pptx.guides.add-vertical");
    let guides = s.with(|e, _| e.presentation().guides.clone());
    assert_eq!(guides.len(), 3);
    let added = guides[2].clone();
    assert!(!added.horizontal && added.user_drawn && added.emu() != size.width / 2, "{added:?}");
    // Dragged 100 px to the right with the pointer.
    let from = guide_point(&mut s, &added);
    let scale = s.with(|e, _| e.slide_layout().unwrap().scale);
    let p0 = gpui_kit::point(gpui_kit::px(from.0), gpui_kit::px(from.1));
    assert_eq!(s.with(|e, _| e.guide_at(p0)), Some(2), "the added guide is under {from:?}");
    s.drag(from, (from.0 + 100.0, from.1));
    let moved = s.with(|e, _| e.presentation().guides[2].clone());
    let expected = SlideGuide::pos_from_emu(added.emu() + (100.0 / scale as f64) as i64);
    assert!((moved.pos - expected).abs() <= 1, "{} vs {expected}", moved.pos);
    assert_eq!(s.with(|e, _| e.presentation().guides.len()), 3);
    // Out of the slide: removed.
    let from = guide_point(&mut s, &moved);
    let origin_x = s.with(|e, _| f32::from(e.slide_layout().unwrap().origin.x));
    s.drag(from, (origin_x - 40.0, from.1));
    assert_eq!(s.with(|e, _| e.presentation().guides.len()), 2, "taken off the slide");
    // Undone, then the lot taken away.
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.presentation().guides.len()), 3);
    s.menu("pptx.guides.remove-all");
    assert!(s.with(|e, _| e.presentation().guides.is_empty()));
}

fn guides_snap() {
    use ibis_doc_model::{Command, EmuRect, Shape};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slide = s.with(|e, _| e.slide().id);
    // A vertical guide at 4,000,000 EMU (in units of 1/8 pt).
    let pos = SlideGuide::pos_from_emu(4_000_000);
    let guide = SlideGuide { id: 1, horizontal: false, pos, color: None, user_drawn: true };
    let target = guide.emu();
    s.update(|e, _, cx| e.run(vec![Command::SetGuides { slide: None, guides: vec![guide.clone()] }], cx));
    let id = s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        e.insert_shape(Shape::text_box(id, EmuRect::new(1_000_000, 1_000_000, 1_000_000, 1_000_000), "snap"), cx);
        id
    });
    s.update(|e, _, _| e.set_selection(vec![id]));
    let layout = s.with(|e, _| e.slide_layout().unwrap());
    let start = gpui_kit::point(gpui_kit::px(100.0), gpui_kit::px(100.0));
    // The right edge (x = 2,000,000 + dx) 3 px short of the guide: with the guides hidden, no snap.
    let dx = (target - 2_000_000) - (3.0 / layout.scale as f64) as i64;
    let drag = |s: &mut Scenario| {
        s.update(|e, _, _| {
            let current = gpui_kit::point(gpui_kit::px(100.0 + dx as f32 * layout.scale), gpui_kit::px(100.0));
            e.set_drag(Some(crate::pptx::slide_view::Drag::Move { start, current, origins: vec![(id, EmuRect::new(1_000_000, 1_000_000, 1_000_000, 1_000_000))] }));
        });
        s.with(|e, _| e.preview(e.slide().shape(id).unwrap()).0)
    };
    let hidden = drag(&mut s);
    assert!((hidden.right() - target).abs() > 1000, "no snap with the guides hidden: {hidden:?}");
    s.update(|e, _, cx| {
        e.set_drag(None);
        cx.notify();
    });
    s.menu("pptx.guides");
    assert!(s.with(|e, _| e.is_pptx_guides_enabled()));
    let shown = drag(&mut s);
    assert_eq!(shown.right(), target, "the edge snaps to the guide: {shown:?}");
    let _ = slide;
}

fn guides_in_master_view() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.master-view");
    assert!(s.with(|e, _| e.is_master_view()));
    assert!(s.with(|e, _| e.slide().guides.is_empty()));
    // Guides made on the page shown (the master's own), shown over it.
    s.update(|e, _, cx| e.toggle_pptx_guides(cx));
    assert_eq!(s.with(|e, _| e.slide().guides.len()), 2);
    s.update(|e, _, cx| e.add_guide(true, cx));
    let made = s.with(|e, _| e.slide().guides.clone());
    assert_eq!(made.len(), 3);
    // Another page and back: the guides stay with their page.
    let pages = s.with(|e, _| e.master_view_page_count());
    s.update(|e, w, cx| e.switch_master_page(pages - 1, w, cx));
    assert!(s.with(|e, _| e.slide().guides.is_empty()), "the other page has none");
    s.update(|e, w, cx| e.switch_master_page(1, w, cx));
    s.update(|e, w, cx| e.switch_master_page(pages - 1, w, cx));
    s.update(|e, w, cx| e.switch_master_page(0, w, cx));
    s.update(|e, w, cx| e.switch_master_page(1, w, cx));
    let first = s.with(|e, _| e.slide().guides.clone());
    // (The page shown first is the layout the slide follows.)
    s.update(|e, w, cx| e.close_master_view(w, cx));
    assert!(!s.with(|e, _| e.is_master_view()));
    s.menu("pptx.master-view");
    let again = s.with(|e, _| e.slide().guides.clone());
    assert_eq!(again.len(), first.len());
    s.update(|e, w, cx| e.close_master_view(w, cx));
    let _ = made;
}

fn layouts_in_master_view() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.master-view");
    let before = s.with(|e, _| e.master_view_page_names());
    assert!(before.len() > 2, "{before:?}");
    // The master (page 0) is not a layout: no copy.
    s.update(|e, w, cx| e.switch_master_page(0, w, cx));
    s.update(|e, w, cx| e.duplicate_master_layout(w, cx));
    assert_eq!(s.with(|e, _| e.master_view_page_count()), before.len());
    // 名前の変更 of a layout.
    s.update(|e, w, cx| e.switch_master_page(2, w, cx));
    s.update(|e, w, cx| {
        e.set_master_page_name_input("新しい名前", w, cx); // i18n-ok: test data
        e.rename_master_page(w, cx);
    });
    let renamed = s.with(|e, _| e.master_view_page_names());
    assert_eq!(renamed[2], "新しい名前", "{renamed:?}"); // i18n-ok: test data
    assert_eq!(renamed.len(), before.len());
    // レイアウトの複製: a copy right after, shown.
    s.update(|e, w, cx| e.duplicate_master_layout(w, cx));
    let copied = s.with(|e, _| e.master_view_page_names());
    assert_eq!(copied.len(), before.len() + 1, "{copied:?}");
    assert_eq!(copied[2], "新しい名前"); // i18n-ok: test data
    assert_ne!(copied[3], "新しい名前"); // i18n-ok: test data
    assert_eq!(s.with(|e, _| e.master_view_name()), copied[3], "the copy is shown");
    // レイアウトの削除: the copy goes.
    s.update(|e, w, cx| e.delete_master_layout(w, cx));
    assert_eq!(s.with(|e, _| e.master_view_page_names()), renamed);
    // スライドに適用して閉じる: the slide follows the layout shown.
    s.update(|e, w, cx| e.switch_master_page(3, w, cx));
    let wanted = renamed[3].clone();
    s.update(|e, w, cx| e.apply_master_page_and_close(w, cx));
    assert!(!s.with(|e, _| e.is_master_view()));
    let followed = s.with(|e, _| e.slide().layout.and_then(|l| e.presentation().layout(l)).map(|l| l.name.clone()));
    assert_eq!(followed.as_deref(), Some(wanted.as_str()));
    // The renamed layout is in the presentation, and a layout a slide follows cannot be deleted from the view.
    assert!(s.with(|e, _| e.presentation().layouts.iter().any(|l| l.name == "新しい名前"))); // i18n-ok: test data
    s.menu("pptx.master-view");
    let names = s.with(|e, _| e.master_view_page_names());
    let at = names.iter().position(|n| *n == wanted).unwrap();
    s.update(|e, w, cx| e.switch_master_page(at, w, cx));
    s.update(|e, w, cx| e.delete_master_layout(w, cx));
    assert_eq!(s.with(|e, _| e.master_view_page_names()), names, "the layout in use stays");
    s.update(|e, w, cx| e.close_master_view(w, cx));
}
