// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! issue #84: the guides (ガイド線) of the view and of a master or layout page, and managing the layouts in the slide master view.
use crate::shell::scenario::Scenario;
use crate::shell::i18n::{self, Language};
use ibis_doc_model::SlideGuide;
use crate::ext::*;

/// The window as shown in Japanese and in English (pictures of the report, `-ja` and `-en`).
fn shots(s: &mut Scenario, name: &str) {
    for (language, tag) in [(Language::Japanese, "ja"), (Language::English, "en")] {
        i18n::show(language);
        s.update(|_, window, _| window.refresh());
        s.settle();
        s.screenshot(&format!("{name}-{tag}"));
    }
    i18n::show(Language::Japanese);
}

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue84_guides_are_seeded_added_dragged_removed_and_undone", guides_view),
        ("pptx::issue84_shapes_dragged_snap_to_the_guides", guides_snap),
        ("pptx::issue84_the_guides_of_the_view_are_saved_in_the_view_properties_and_read_again", guides_saved),
        ("pptx::issue84_master_page_guides_are_kept_when_the_view_is_left", guides_in_master_view),
        ("pptx::issue84_master_and_layout_guides_show_on_the_slide_and_the_shapes_snap_to_them", guides_inherited),
        ("pptx::issue84_a_guide_dragged_shows_its_distance_from_the_middle_and_alt_goes_finer", guides_drag_label),
        ("pptx::issue84_a_dragged_guide_goes_by_a_tenth_of_a_centimetre_and_with_alt_by_a_hundredth", a_dragged_guide_goes_by_a_tenth_of_a_centimetre_and_with_alt_by_a_hundredth),
        ("pptx::issue84_the_label_is_the_distance_from_the_middle_in_the_rulers_unit", the_label_is_the_distance_from_the_middle_in_the_rulers_unit),
        ("pptx::issue84_a_resized_edge_snaps_to_a_guide_near_it", a_resized_edge_snaps_to_a_guide_near_it),
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
    // (On the 0.1 cm grid from the middle of the slide.)
    let expected = SlideGuide::pos_from_emu(crate::guides::quantize_from_middle(added.emu() + (100.0 / scale as f64) as i64, size.width / 2, 36_000.0));
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
    // The right edge (x = 2,000,000 + dx) 1 px short of the guide: with the guides hidden, no snap.
    let dx = (target - 2_000_000) - (1.0 / layout.scale as f64) as i64;
    let drag = |s: &mut Scenario| {
        s.update(|e, _, _| {
            let current = gpui_kit::point(gpui_kit::px(100.0 + dx as f32 * layout.scale), gpui_kit::px(100.0));
            e.set_drag(Some(crate::slide_view::Drag::Move { start, current, origins: vec![(id, EmuRect::new(1_000_000, 1_000_000, 1_000_000, 1_000_000))] }));
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

/// A copy of the plan deck with guides on its master (a vertical at 3,000,000 EMU and a horizontal at 2,000,000) and
/// on its first layout (a vertical at 5,000,000), as PowerPoint writes them; the path (from the repository's root).
fn deck_with_master_guides() -> (String, Vec<SlideGuide>, Vec<SlideGuide>) {
    use ibis_doc_model::{Command, Document};
    use ibis_pptx::{master_page, SaveOptions};
    let root = crate::shell::scenario::repo_root();
    let mut data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).expect("the plan deck");
    let master_guides = vec![
        SlideGuide { id: 1, horizontal: false, pos: SlideGuide::pos_from_emu(3_000_000), color: None, user_drawn: true },
        SlideGuide { id: 2, horizontal: true, pos: SlideGuide::pos_from_emu(2_000_000), color: None, user_drawn: true },
    ];
    let layout_guides = vec![SlideGuide { id: 1, horizontal: false, pos: SlideGuide::pos_from_emu(5_000_000), color: None, user_drawn: true }];
    let pages = master_page::pages(&data).unwrap();
    for (page, guides) in [(pages.iter().find(|p| p.master).unwrap().clone(), master_guides.clone()), (pages.iter().find(|p| !p.master).unwrap().clone(), layout_guides.clone())] {
        let opened = ibis_pptx::open(master_page::build(&data, &page.part).unwrap()).unwrap();
        let mut document = Document::new(opened.presentation.clone());
        document.execute(vec![Command::SetGuides { slide: Some(opened.presentation.slides[0].id), guides }]).unwrap();
        let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
        data = master_page::apply(&data, &edited, &page.part).unwrap();
    }
    let path = "target/issue84-guides.pptx";
    std::fs::write(root.join(path), &data).unwrap();
    (path.to_string(), master_guides, layout_guides)
}

fn guides_inherited() {
    use ibis_doc_model::{Command, EmuRect, Shape};
    let (path, master_guides, _) = deck_with_master_guides();
    let mut s = Scenario::open(&path);
    // The layouts of the file carry the guides; every one has the master's.
    let layouts = s.with(|e, _| e.presentation().layouts.clone());
    assert!(layouts.iter().all(|l| l.master_guides == master_guides), "{:?}", layouts.iter().map(|l| &l.master_guides).collect::<Vec<_>>());
    assert_eq!(layouts.iter().filter(|l| l.guides.len() == 1).count(), 1);
    // The slide shows its layout's guides and its master's, not owning them.
    let shown = s.with(|e, _| e.inherited_guides());
    let layout_of_slide = s.with(|e, _| e.slide().layout);
    let own = layouts.iter().find(|l| Some(l.id) == layout_of_slide).unwrap().guides.len();
    assert_eq!(shown.len(), master_guides.len() + own, "{shown:?}");
    assert!(s.with(|e, _| e.guide_list().is_empty()), "the slide's own list stays empty");
    // Shown with the guides on (PowerPoint's two middle guides are made, the inherited ones come over them).
    s.menu("pptx.guides");
    assert!(s.with(|e, _| e.is_pptx_guides_enabled()));
    let (xs, ys) = s.with(|e, _| e.guide_snap_targets());
    assert!(xs.contains(&master_guides[0].emu()) && ys.contains(&master_guides[1].emu()), "{xs:?} {ys:?}");
    // A shape dragged near the master's vertical guide snaps to it.
    let id = s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        e.insert_shape(Shape::text_box(id, EmuRect::new(1_000_000, 1_000_000, 1_000_000, 1_000_000), "snap"), cx);
        id
    });
    s.update(|e, _, _| e.set_selection(vec![id]));
    let layout = s.with(|e, _| e.slide_layout().unwrap());
    let start = gpui_kit::point(gpui_kit::px(100.0), gpui_kit::px(100.0));
    let target = master_guides[0].emu();
    let dx = (target - 2_000_000) - (1.0 / layout.scale as f64) as i64;
    s.update(|e, _, _| {
        let current = gpui_kit::point(gpui_kit::px(100.0 + dx as f32 * layout.scale), gpui_kit::px(100.0));
        e.set_drag(Some(crate::slide_view::Drag::Move { start, current, origins: vec![(id, EmuRect::new(1_000_000, 1_000_000, 1_000_000, 1_000_000))] }));
    });
    let moved = s.with(|e, _| e.preview(e.slide().shape(id).unwrap()).0);
    assert_eq!(moved.right(), target, "the edge snaps to the master's guide: {moved:?}");
    s.update(|e, _, cx| {
        e.set_drag(None);
        cx.notify();
    });
    // The right edge dragged (resized) near the layout's guide or the master's snaps too.
    s.update(|e, _, _| {
        let origin = EmuRect::new(1_000_000, 1_000_000, 1_000_000, 1_000_000);
        let current = gpui_kit::point(gpui_kit::px(100.0 + (target - 2_000_000 - (1.0 / layout.scale as f64) as i64) as f32 * layout.scale), gpui_kit::px(100.0));
        e.set_drag(Some(crate::slide_view::Drag::Resize { shape: id, handle: ibis_doc_model::ResizeHandle::Right, start, current, origin, rotation: 0, keep_aspect_on_corners: false }));
    });
    let resized = s.with(|e, _| e.preview(e.slide().shape(id).unwrap()).0);
    assert_eq!(resized.right(), target, "the edge being resized snaps to the guide: {resized:?}");
    s.update(|e, _, cx| {
        e.set_drag(None);
        cx.notify();
    });
    // 画面の画像 (the slide with the guides of its master).
    shots(&mut s, "slide-with-master-guides");
    // In the master view: a layout page shows its master's guides (the master page, only its own).
    s.menu("pptx.master-view");
    let pages = s.with(|e, _| e.master_view_page_count());
    s.update(|e, w, cx| e.switch_master_page(0, w, cx));
    assert!(s.with(|e, _| e.inherited_guides().is_empty()));
    assert_eq!(s.with(|e, _| e.slide().guides.len()), master_guides.len());
    s.update(|e, w, cx| e.switch_master_page(pages - 1, w, cx));
    assert_eq!(s.with(|e, _| e.inherited_guides()), master_guides);
    s.update(|e, w, cx| e.close_master_view(w, cx));
    let _ = Command::SetGuides { slide: None, guides: Vec::new() };
}

fn guides_drag_label() {
    use crate::slide_view::Drag;
    use crate::shell::editor::RulerUnit;
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.guides");
    let size = s.with(|e, _| e.presentation().size_of(e.slide()));
    let layout = s.with(|e, _| e.slide_layout().unwrap());
    // The vertical guide of the middle, dragged to 9.04 cm right of the middle (the points a quarter down it: not where the two cross).
    let (index, guide) = s.with(|e, _| e.guide_list().into_iter().enumerate().find(|(_, g)| !g.horizontal).unwrap());
    let at = |cm: f64| {
        let x = size.width / 2 + (cm * 360_000.0) as i64;
        gpui_kit::point(layout.origin.x + layout.to_px(x), layout.origin.y + layout.to_px(size.height / 4))
    };
    let from = at(0.0);
    for (alt, label) in [(false, "9.00"), (true, "9.04")] {
        s.update(|e, _, cx| {
            e.set_drag(Some(Drag::Guide { index, start: from, current: at(9.04), alt }));
            cx.notify();
        });
        let (text, icon) = s.with(|e, _| e.guide_drag_label()).expect("a label while a guide is dragged");
        assert_eq!((text.as_str(), icon), (label, "arrow_right"), "alt {alt}");
        // (Dragged to the left: the same number, the other way.)
        s.update(|e, _, _| e.set_drag(Some(Drag::Guide { index, start: from, current: at(-9.04), alt })));
        assert_eq!(s.with(|e, _| e.guide_drag_label()), Some((label.to_string(), "arrow_left")));
    }
    // The unit follows the ruler's: inches, two decimals.
    s.update(|e, _, cx| e.set_ruler_unit(RulerUnit::Inches, cx));
    s.update(|e, _, _| e.set_drag(Some(Drag::Guide { index, start: from, current: at(2.54), alt: true })));
    assert_eq!(s.with(|e, _| e.guide_drag_label()).map(|l| l.0), Some("1.00".to_string()));
    s.update(|e, _, cx| {
        e.set_drag(None);
        e.set_ruler_unit(RulerUnit::Centimeters, cx);
    });
    assert!(s.with(|e, _| e.guide_drag_label()).is_none());
    // With the pointer: the guide dragged and dropped goes by 0.1 cm, with Alt by 0.01 cm; the number shows on the way.
    let from_px = (f32::from(from.x), f32::from(from.y));
    let target = (f32::from(at(9.04).x), f32::from(at(9.04).y));
    s.drag(from_px, target);
    let pos = s.with(|e, _| e.presentation().guides.iter().find(|g| g.id == guide.id).unwrap().emu());
    assert!((pos - (size.width / 2 + 3_240_000)).abs() <= 800, "0.1 cm steps: {pos}");
    // With Alt held (pressed, moved and released by hand to see the flag on the way): the finer step.
    use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};
    let alt = gpui_kit::Modifiers { alt: true, ..Default::default() };
    let (down, up) = (at(9.0), at(9.04));
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: down, modifiers: alt, ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: down, click_count: 1, modifiers: alt, ..Default::default() }),
        PlatformInput::MouseMove(MouseMoveEvent { position: up, pressed_button: Some(MouseButton::Left), modifiers: alt, ..Default::default() }),
    ]);
    assert!(s.with(|e, _| matches!(e.shape_edit.drag.as_ref(), Some(Drag::Guide { alt: true, .. }))), "Alt held is kept by the drag of the guide");
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: up, click_count: 1, modifiers: alt, ..Default::default() })]);
    let pos = s.with(|e, _| e.presentation().guides.iter().find(|g| g.id == guide.id).unwrap().emu());
    assert!((pos - (size.width / 2 + (9.04 * 360_000.0) as i64)).abs() <= (1.5 / layout.scale as f64) as i64, "0.01 cm steps (to the pixel the pointer is on): {pos}");
    // Not on the 0.1 cm grid: no snap with Alt.
    assert_ne!((pos - size.width / 2) % 36_000, 0, "{pos}");
    // 画面の画像 (the number beside the pointer while dragging).
    s.update(|e, _, cx| {
        e.set_drag(Some(Drag::Guide { index, start: from, current: at(9.04), alt: false }));
        cx.notify();
    });
    shots(&mut s, "guide-drag-label");
    s.update(|e, _, _| e.set_drag(None));
}

fn a_dragged_guide_goes_by_a_tenth_of_a_centimetre_and_with_alt_by_a_hundredth() {
    use crate::guides::{guide_step_emu, quantize_from_middle};
    use crate::shell::editor::RulerUnit;
    let half = 6_096_000;
    let step = guide_step_emu(RulerUnit::Centimeters, false);
    assert_eq!(step, 36_000.0);
    // 9.04 cm from the middle: 9.0 cm; with Alt, as it is.
    let coord = half + 3_254_400;
    assert_eq!(quantize_from_middle(coord, half, step), half + 3_240_000);
    assert_eq!(quantize_from_middle(coord, half, guide_step_emu(RulerUnit::Centimeters, true)), half + 3_254_400);
    // The middle itself is on the grid (the middle of a slide of an odd width too).
    assert_eq!(quantize_from_middle(half + 10_000, half, step), half);
    assert_eq!(quantize_from_middle(half - 20_000, half, step), half - 36_000);
}

fn the_label_is_the_distance_from_the_middle_in_the_rulers_unit() {
    use crate::guides::guide_label;
    use crate::shell::editor::RulerUnit;
    use ibis_doc_model::EmuSize;
    let size = EmuSize { width: 12_192_000, height: 6_858_000 };
    let (text, icon) = guide_label(6_096_000 + 3_240_000, size, false, RulerUnit::Centimeters);
    assert_eq!((text.as_str(), icon), ("9.00", "arrow_right"));
    assert_eq!(guide_label(6_096_000 - 3_240_000, size, false, RulerUnit::Centimeters), ("9.00".to_string(), "arrow_left"));
    assert_eq!(guide_label(3_429_000 + 360_000, size, true, RulerUnit::Centimeters), ("1.00".to_string(), "arrow_drop_down"));
    assert_eq!(guide_label(3_429_000 - 914_400, size, true, RulerUnit::Inches), ("1.00".to_string(), "arrow_drop_up"));
    assert_eq!(guide_label(6_096_000, size, false, RulerUnit::Centimeters).0, "0.00");
}

fn a_resized_edge_snaps_to_a_guide_near_it() {
    use crate::guides::snap_resize_to_guides;
    use ibis_doc_model::{EmuRect, ResizeHandle};
    let origin = EmuRect::new(1_000_000, 1_000_000, 1_000_000, 1_000_000);
    // The right edge (2,000,000 + 990,000) 10,000 short of a guide at 3,000,000 goes onto it; the other axis stays.
    assert_eq!(snap_resize_to_guides(origin, ResizeHandle::Right, 990_000.0, 5.0, &[3_000_000], &[1_000_000], 20_000), (1_000_000.0, 5.0));
    // The left edge snaps by its own side; a handle that does not move an edge leaves that axis alone.
    assert_eq!(snap_resize_to_guides(origin, ResizeHandle::TopLeft, -498_000.0, 1_995.0, &[500_000], &[3_000_000], 5_000), (-500_000.0, 1_995.0));
    assert_eq!(snap_resize_to_guides(origin, ResizeHandle::Top, 990_000.0, 0.0, &[3_000_000], &[], 20_000).0, 990_000.0);
    // Out of reach: no snap.
    assert_eq!(snap_resize_to_guides(origin, ResizeHandle::Right, 900_000.0, 0.0, &[3_000_000], &[], 20_000).0, 900_000.0);
}

fn guides_saved() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.guides");
    s.menu("pptx.guides.add-vertical");
    s.menu("pptx.guides.add-horizontal");
    // A guide dragged on the slide (the added vertical one, a quarter down it) to the 0.1 cm grid.
    let guides = s.with(|e, _| e.presentation().guides.clone());
    assert_eq!(guides.len(), 4);
    let from = guide_point(&mut s, &guides[2]);
    s.drag(from, (from.0 + 50.0, from.1));
    let made = s.with(|e, _| e.presentation().guides.clone());
    assert_ne!(made[2].pos, guides[2].pos);
    s.key("cmd-s");
    // Read again: the same places (the view properties keep no colour: the others are as they were).
    let again = s.reopen().guides;
    assert_eq!(again.iter().map(|g| (g.horizontal, g.pos)).collect::<Vec<_>>(), made.iter().map(|g| (g.horizontal, g.pos)).collect::<Vec<_>>());
}
