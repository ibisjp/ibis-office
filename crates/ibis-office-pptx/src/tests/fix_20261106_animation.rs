// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-06, before the release: the animation pane and 効果のオプション.
//! The effect dropdown of the dialog, the sidebar button showing the animation pane, the dragged row of the pane carried in
//! front with the series list's shadow, the paragraphs' rows under their text box's row, and a new clip's rows at once.
use crate::shell::scenario::Scenario;
use ibis_doc_model::{AnimationClass as C, AnimationLook as L, MediaCommand};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::fix_20261106_options_dialog_changes_the_effect", effect_dropdown),
        ("pptx::fix_20261106_sidebar_button_shows_the_animation_pane", sidebar_button),
        ("pptx::fix_20261106_dragged_pane_row_is_carried_in_front", dragged_row),
        ("pptx::fix_20261106_paragraph_rows_under_the_text_box_row", paragraph_rows),
        ("pptx::fix_20261106_paragraph_rows_move_among_themselves", paragraph_rows_drag),
        ("pptx::fix_20261106_row_is_lifted_at_the_press_and_follows_the_pointer", lifted_at_the_press),
        ("pptx::fix_20261106_row_passes_the_next_one_once_without_shaking", passes_once),
        ("pptx::fix_20261106_text_box_row_is_dragged_with_its_paragraphs", text_box_row_drag),
        ("pptx::fix_20261106_every_drop_keeps_all_the_effects_synthetic", every_drop_synthetic),
        ("pptx::fix_20261106_first_row_dropped_on_the_third_loses_no_effect", first_row_on_third_real_file),
        ("pptx::fix_20261106_drawn_paragraph_row_follows_the_pointer_in_the_real_file", drawn_child_follows),
        ("pptx::fix_20261106_two_text_box_rows_open_and_fold_on_their_own", two_text_boxes_on_their_own),
        ("pptx::fix_20261106_paragraph_row_stays_inside_its_text_box_range", child_stays_in_range),
        ("pptx::fix_20261106_text_boxes_fold_while_a_text_box_row_is_dragged", folds_while_dragged),
        ("pptx::fix_20261106_inserted_clip_has_media_rows_at_once", clip_rows),
    ]
}

fn at(x: f32, y: f32) -> gpui_kit::Point<gpui_kit::Pixels> {
    gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y))
}

fn effect_dropdown() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 900.);
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    s.toolbar_menu("motion", "pptx.anim.pane");
    s.update(|e, _, cx| e.open_animation_options_panel(0, cx));
    s.settle();
    s.screenshot("dialog-effect-dropdown");
    s.update(|e, _, cx| e.set_panel_effect(L::Zoom, None, cx));
    s.settle();
    s.screenshot("dialog-after-zoom");
    s.update(|e, _, cx| e.apply_animation_options_panel(cx));
    let (class, look) = s.with(|e, _| (e.slide().animations[0].class, e.slide().animations[0].look));
    assert_eq!((class, look), (C::Entrance, L::Zoom), "the effect was changed in the dialog");
    // A catalogue effect: its preset and behaviours go with it.
    s.update(|e, _, cx| e.open_animation_options_panel(0, cx));
    let id = ibis_doc_model::animation_preset::FILTER_PRESETS[0].id;
    s.update(|e, _, cx| e.set_panel_effect(L::Preset, Some(id), cx));
    s.update(|e, _, cx| e.apply_animation_options_panel(cx));
    let a = s.with(|e, _| e.slide().animations[0].clone());
    assert_eq!((a.look, a.preset.0), (L::Preset, id));
    assert!(!a.behaviors.is_empty());
}

fn sidebar_button() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 900.);
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    assert!(!s.with(|e, _| e.pptx().unwrap().animation_pane));
    s.update(|e, w, cx| e.toggle_sidebar(w, cx));
    s.settle();
    assert!(s.with(|e, _| e.pptx().unwrap().animation_pane), "the title bar's button shows the animation pane");
    s.screenshot("sidebar-button-animation-pane");
}

/// The fraction of the pixels of a strip (`rows` of `a` against `rows` of `b`, the same columns) that differ by more than a
/// shade (the lifted row's hover tint is one).
fn strip_difference(a: &image::RgbaImage, b: &image::RgbaImage, columns: std::ops::Range<u32>, a_top: u32, b_top: u32, height: u32) -> f32 {
    let mut differing = 0;
    let mut all = 0;
    for dy in 0..height {
        for x in columns.clone() {
            let (p, q) = (a.get_pixel(x, a_top + dy).0, b.get_pixel(x, b_top + dy).0);
            all += 1;
            if (0..3).any(|c| (p[c] as i32 - q[c] as i32).abs() > 24) {
                differing += 1;
            }
        }
    }
    differing as f32 / all as f32
}

fn dragged_row() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1280., 720.);
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    s.toolbar_menu("motion", "add.pptx.anim.em.spin");
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    s.toolbar_menu("motion", "pptx.anim.pane");
    s.settle();
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap();
    let before = image::open(s.screenshot("pane-before-the-drag")).unwrap().to_rgba8();
    let scale = before.width() as f32 / 1280.;
    let height = f32::from(b(1).origin.y - b(0).origin.y);
    // The middle row is taken down by a bit more than half a row: it passes no row yet and lies over the last one.
    let (x, y0) = (f32::from(b(1).origin.x) + 120., f32::from(b(1).origin.y) + 10.);
    let y1 = y0 + height * 0.6;
    s.dispatch(vec![
        gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent { position: at(x, y0), ..Default::default() }),
        gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent { button: gpui_kit::MouseButton::Left, position: at(x, y0), click_count: 1, ..Default::default() }),
        gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent { position: at(x, y0 + 8.), pressed_button: Some(gpui_kit::MouseButton::Left), ..Default::default() }),
        gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent { position: at(x, y1), pressed_button: Some(gpui_kit::MouseButton::Left), ..Default::default() }),
    ]);
    std::thread::sleep(std::time::Duration::from_millis(400));
    s.settle();
    // The row is carried by the pointer (its offset from its own place follows it); no label hangs on the pointer.
    let lift = dnd.lift_offset("anim-table", 1).expect("the middle row is carried");
    assert!((height * 0.5..height * 0.7).contains(&lift), "{lift} of {height}");
    assert!(dnd.ghost("anim-table").is_some(), "the drag state still knows its label, the table does not draw it");
    let during = image::open(s.screenshot("dragged-row-in-front")).unwrap().to_rgba8();
    let px = |v: f32| (v * scale).round() as u32;
    let (left, right) = (px(f32::from(b(1).origin.x) + 8.), px(f32::from(b(1).origin.x) + f32::from(b(1).size.width) - 30.));
    let (own_top, carried_top) = (f32::from(b(1).origin.y), f32::from(b(1).origin.y) + lift);
    // The shadow: in the empty place the row left, just above the carried row (outside the row), the white is darkened; farther
    // up it is white again.
    let mean = |y: u32| -> f32 { (left..right).map(|x| { let p = during.get_pixel(x, y).0; (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3. }).sum::<f32>() / (right - left) as f32 };
    let near = mean(px(carried_top - 3.));
    let far = mean(px(carried_top - 16.));
    assert!(near < far - 2. && far > 252., "no shadow above the carried row: {near} next to it, {far} farther up");
    // In front: where the carried row lies over the last row, the pixels are the carried row's (as it stood before the drag),
    // not the last row's.
    let strip = (height - lift + 4., lift - 8.);
    let rows = px(strip.1.min(height - strip.0 - 4.));
    let carried = strip_difference(&during, &before, left..right, px(carried_top + strip.0), px(own_top + strip.0), rows);
    let last_row = strip_difference(&during, &before, left..right, px(carried_top + strip.0), px(own_top + height + (strip.0 + lift - height)), rows);
    assert!(carried < 0.03, "the strip over the last row is not the carried row's: {carried}");
    assert!(last_row > 0.03, "the check cannot tell the carried row from the last one: {last_row}");
    s.dispatch(vec![gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent { button: gpui_kit::MouseButton::Left, position: at(x, y1), click_count: 1, ..Default::default() })]);
    s.settle();
}

/// The rows of a text box's paragraphs are dragged among themselves; another row passes all of them, never between them.
fn paragraph_rows_drag() {
    use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationStart, AnimationTrigger, Command};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1280., 720.);
    s.page(2);
    let (slide, shape, other) = s.with(|e, _| {
        let sl = e.slide();
        let sh = sl.shapes.iter().find(|sh| sh.text_body().is_some_and(|t| t.paragraphs.len() >= 3)).expect("a shape with paragraphs");
        let other = sl.shapes.iter().find(|o| o.id != sh.id).expect("another shape").id;
        (sl.id, sh.id, other)
    });
    let effect = |shape, paragraph: Option<u32>| Animation {
        shape,
        class: AnimationClass::Entrance,
        start: AnimationStart::OnClick,
        look: AnimationLook::Appear,
        delay: 0,
        duration: 1,
        preset: (1, 0),
        repeat_count: AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: AnimationAfterEffect::Hold,
        trigger: AnimationTrigger::None,
        paragraph: paragraph.map(|i| (i, i)), chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None,
        dgm_element: None, dgm_build: None, behaviors: Vec::new(),
    };
    // Row 0: another shape; rows 1..=3: the paragraphs 0, 1, 2 of the text box; row 4: the other shape again.
    let animations = vec![effect(other, None), effect(shape, Some(0)), effect(shape, Some(1)), effect(shape, Some(2)), effect(other, None)];
    s.update(|e, _, cx| e.run(vec![Command::SetAnimations { slide, animations: animations.clone() }], cx));
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    s.update(|e, _, cx| e.toggle_animation_expanded((shape, 0, 0), cx));
    s.settle();
    let paragraphs_of = |s: &mut Scenario| -> Vec<Option<u32>> { s.with(|e, _| e.slide().animations.iter().map(|a| a.paragraph.map(|p| p.0)).collect()) };
    // The drops, as the table reports them. A paragraph's row among its own.
    s.update(|e, _, cx| e.animation_row_dropped(crate::shell::chrome::row_dnd::RowDropEvent { from_table: "anim-table", from: 1, target_table: "anim-table", target: 3, place: crate::shell::chrome::row_dnd::DropPlace::After }, cx));
    assert_eq!(paragraphs_of(&mut s), vec![None, Some(1), Some(2), Some(0), None]);
    // A paragraph's row dropped on another shape's row: nothing.
    s.update(|e, _, cx| e.animation_row_dropped(crate::shell::chrome::row_dnd::RowDropEvent { from_table: "anim-table", from: 1, target_table: "anim-table", target: 4, place: crate::shell::chrome::row_dnd::DropPlace::Before }, cx));
    assert_eq!(paragraphs_of(&mut s), vec![None, Some(1), Some(2), Some(0), None]);
    // Another shape's row dropped between paragraphs goes past all of them.
    s.update(|e, _, cx| e.animation_row_dropped(crate::shell::chrome::row_dnd::RowDropEvent { from_table: "anim-table", from: 0, target_table: "anim-table", target: 2, place: crate::shell::chrome::row_dnd::DropPlace::After }, cx));
    // (The paragraphs 1, 2, 0 are still one text box's row: its paragraphs may stand in any order, 2026-11-07.)
    assert_eq!(paragraphs_of(&mut s), vec![Some(1), Some(2), Some(0), None, None]);
    // The pane's rows: the paragraphs' rows can be dragged (they end in the grip), the text box's row cannot.
    s.screenshot("paragraph-rows-draggable");
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    assert!(dnd.row_bounds("anim-table", 2).is_some(), "the paragraphs' rows take drops");
}

fn paragraph_rows() {
    let file = "test/github-issue/62/layout_test.pptx";
    if !std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(file).exists() {
        return;
    }
    let mut s = Scenario::open(file);
    s.size(1280., 720.);
    s.page(3);
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    s.settle();
    s.screenshot("paragraph-rows-folded");
    let key = s.with(|e, _| e.slide().animations.iter().position(|a| a.paragraph.is_some()).map(|i| crate::animation_ui::box_keys(&e.slide().animations)[i]));
    let Some(key) = key else { return };
    s.update(|e, _, cx| e.toggle_animation_expanded(key, cx));
    s.settle();
    s.screenshot("paragraph-rows-expanded");
}

fn clip_rows() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let (video, audio) = (root.join("clip-test.mp4"), root.join("clip-tone.wav"));
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1280., 720.);
    s.page(2);
    let (vdata, adata) = (std::fs::read(&video).unwrap(), std::fs::read(&audio).unwrap());
    let before = s.with(|e, _| e.slide().animations.len());
    s.update(|e, _, cx| e.insert_clip(&video, vdata.clone(), false, cx)).unwrap();
    let rows = |s: &mut Scenario| -> Vec<(C, L)> { s.with(|e, _| e.slide().animations.iter().skip(before).map(|a| (a.class, a.look)).collect()) };
    assert_eq!(rows(&mut s), vec![(C::Media, L::Media(MediaCommand::Pause))], "a video (PowerPoint 16): only 一時停止, by a click on the video"); // i18n-ok（試験・ファイルの中の日本語のデータ）
    s.update(|e, _, cx| e.insert_clip(&audio, adata.clone(), true, cx)).unwrap();
    assert_eq!(rows(&mut s).len(), 2, "an audio: 再生 by a click on it"); // i18n-ok（試験・ファイルの中の日本語のデータ）
    assert!(s.with(|e, _| e.slide().animations.iter().skip(before).all(|a| a.trigger == ibis_doc_model::AnimationTrigger::OnShapeClick(a.shape))), "no row of the main sequence");
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    s.settle();
    s.screenshot("inserted-clip-rows");
    // Saved and opened again: the same rows.
    s.save_as("clip-rows.pptx");
    let saved = s.reopen();
    let again: Vec<(C, L)> = saved.slides[1].animations.iter().skip(before).map(|a| (a.class, a.look)).collect();
    assert_eq!(again.len(), 2, "{again:?}");
    assert_eq!(again, vec![(C::Media, L::Media(MediaCommand::Pause)), (C::Media, L::Media(MediaCommand::Play))]);
}

/// One event, then the frame drawn.
pub(crate) fn step(s: &mut Scenario, event: gpui_kit::PlatformInput) {
    s.dispatch(vec![event]);
    std::thread::sleep(std::time::Duration::from_millis(250));
    s.settle();
}

pub(crate) fn mouse_move(x: f32, y: f32, pressed: bool) -> gpui_kit::PlatformInput {
    gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent { position: at(x, y), pressed_button: pressed.then_some(gpui_kit::MouseButton::Left), ..Default::default() })
}

pub(crate) fn mouse_down(x: f32, y: f32) -> gpui_kit::PlatformInput {
    gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent { button: gpui_kit::MouseButton::Left, position: at(x, y), click_count: 1, ..Default::default() })
}

pub(crate) fn mouse_up(x: f32, y: f32) -> gpui_kit::PlatformInput {
    gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent { button: gpui_kit::MouseButton::Left, position: at(x, y), click_count: 1, ..Default::default() })
}

/// The pane with three effects (rows 0, 1 of the first shape, row 2 of the second).
fn three_rows() -> Scenario {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1280., 720.);
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    s.toolbar_menu("motion", "add.pptx.anim.em.spin");
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    s.toolbar_menu("motion", "pptx.anim.pane");
    s.settle();
    s
}

/// (a) The row is lifted when the button goes down on it, before the drag begins, and put back when the button goes up without
/// a drag; (c) it follows the pointer by the distance moved from the press, also within the distance a drag begins at.
fn lifted_at_the_press() {
    let mut s = three_rows();
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap();
    let before = image::open(s.screenshot("pane-before-the-press")).unwrap().to_rgba8();
    let scale = before.width() as f32 / 1280.;
    let (x, y) = (f32::from(b(2).origin.x) + 120., f32::from(b(2).origin.y) + 20.);
    let own_top = f32::from(b(2).origin.y);
    assert_eq!(dnd.lift_offset("anim-table", 2), None);
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    assert_eq!(dnd.lift_offset("anim-table", 2), Some(0.), "lifted at the press");
    assert!(dnd.dragging().is_none(), "no drag yet");
    let pressed = image::open(s.screenshot("row-lifted-at-the-press")).unwrap().to_rgba8();
    let px = |v: f32| (v * scale).round() as u32;
    let (left, right) = (px(f32::from(b(2).origin.x) + 8.), px(f32::from(b(2).origin.x) + f32::from(b(2).size.width) - 30.));
    let mean = |img: &image::RgbaImage, y: u32| -> f32 { (left..right).map(|x| { let p = img.get_pixel(x, y).0; (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3. }).sum::<f32>() / (right - left) as f32 };
    // The shadow lies above the row (a row of the pane, white there), outside the lifted row.
    let (shadowed, plain) = (mean(&pressed, px(own_top - 4.)), mean(&before, px(own_top - 4.)));
    assert!(shadowed < plain - 1., "no shadow when the button goes down: {shadowed} against {plain}");
    // The row follows the pointer from the press: by 1, 2 and 3 px upward (inside the distance a drag begins at), then more.
    let mut seen = Vec::new();
    for dy in [1., 2., 3., 3.5] {
        step(&mut s, mouse_move(x, y - dy, true));
        let lift = dnd.lift_offset("anim-table", 2).expect("still held");
        assert!((lift + dy).abs() < 0.5, "the row is at {lift} after the pointer moved {dy} up");
        seen.push(lift);
    }
    assert!(dnd.dragging().is_none(), "still no drag after 3.5 px");
    let moved = image::open(s.screenshot("row-follows-below-the-threshold")).unwrap().to_rgba8();
    let (a, c) = (mean(&moved, px(own_top - 3.5 - 4.)), mean(&before, px(own_top - 3.5 - 4.)));
    assert!(a < c - 1., "the shadow did not move with the row: {a} against {c} ({seen:?})");
    // Further: the drag begins, and the row keeps following.
    for dy in [6., 10., 14.] {
        step(&mut s, mouse_move(x, y - dy, true));
        let lift = dnd.lift_offset("anim-table", 2).expect("held");
        assert!((lift + dy).abs() < 0.5, "the row is at {lift} after the pointer moved {dy} up");
    }
    assert!(dnd.dragging().is_some());
    // Released where it was pressed: put back, nothing changed.
    let order: Vec<_> = s.with(|e, _| e.slide().animations.iter().map(|a| a.look).collect());
    step(&mut s, mouse_move(x, y, true));
    step(&mut s, mouse_up(x, y));
    assert_eq!(dnd.lift_offset("anim-table", 2), None, "put back");
    assert_eq!(s.with(|e, _| e.slide().animations.iter().map(|a| a.look).collect::<Vec<_>>()), order);
    // A press and release without a move is a click: it selects the row's shape.
    s.click_shape(0);
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    step(&mut s, mouse_up(x, y));
    assert_eq!(dnd.lift_offset("anim-table", 2), None, "put back after the click");
    assert!(s.with(|e, _| e.shape_edit.selection == vec![e.slide().animations[2].shape]), "the click selected the row's shape");
}

/// (d) The third row dragged up by a row and a little more passes the one above it once, and the rows do not shake: the number
/// of the changes of its place is counted at every event.
fn passes_once() {
    let mut s = three_rows();
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap();
    let height = f32::from(b(2).origin.y - b(1).origin.y);
    let (x, y) = (f32::from(b(2).origin.x) + 120., f32::from(b(2).origin.y) + 20.);
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    let mut places = vec![];
    let mut last_lift = 0.;
    let mut dy = 0.;
    // Up in 2 px steps to a row and a third, then back down to half a row up and up again: the place changes once.
    let path: Vec<f32> = (1..).map(|n| n as f32 * 2.).take_while(|d| *d <= height * 1.3).chain((0..).map(|n| height * 1.3 - n as f32 * 2.).take_while(|d| *d >= height * 0.9)).chain((1..).map(|n| height * 0.9 + n as f32 * 2.).take_while(|d| *d <= height * 1.3)).collect();
    for d in path {
        dy = d;
        step(&mut s, mouse_move(x, y - dy, true));
        let lift = dnd.lift_offset("anim-table", 2).expect("held");
        // The row follows the pointer exactly (the table's top is far above, nothing clamps it).
        assert!((lift + dy).abs() < 0.5, "the row is at {lift}, the pointer {dy} up");
        last_lift = lift;
        places.push(dnd.hover().map(|(_, at, place)| (at, place)));
    }
    let changes = places.windows(2).filter(|w| w[0] != w[1]).count();
    assert_eq!(changes, 1, "the place changed {changes} times: {places:?}");
    assert_eq!(places.last().unwrap().map(|p| p.0), Some(1), "{last_lift} {dy}");
    s.screenshot("third-row-passed-the-second");
    step(&mut s, mouse_up(x, y - dy));
    let looks: Vec<_> = s.with(|e, _| e.slide().animations.iter().map(|a| a.look).collect());
    assert_eq!(looks.len(), 3);
}

/// The pane with five effects: another shape's, the three paragraphs of a text box (its row open), another shape's.
fn text_box_rows(open: bool) -> (Scenario, ibis_doc_model::ShapeId) {
    use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationStart, AnimationTrigger, Command};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1280., 720.);
    s.page(2);
    let (slide, shape, other) = s.with(|e, _| {
        let sl = e.slide();
        let sh = sl.shapes.iter().find(|sh| sh.text_body().is_some_and(|t| t.paragraphs.len() >= 3)).expect("a shape with paragraphs");
        (sl.id, sh.id, sl.shapes.iter().find(|o| o.id != sh.id).expect("another shape").id)
    });
    let effect = |shape, paragraph: Option<u32>| Animation {
        shape, class: AnimationClass::Entrance, start: AnimationStart::OnClick, look: AnimationLook::Appear, delay: 0, duration: 1, preset: (1, 0),
        repeat_count: AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: AnimationAfterEffect::Hold,
        trigger: AnimationTrigger::None, paragraph: paragraph.map(|i| (i, i)), chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None, dgm_element: None, dgm_build: None, behaviors: Vec::new(),
    };
    let animations = vec![effect(other, None), effect(shape, Some(0)), effect(shape, Some(1)), effect(shape, Some(2)), effect(other, None)];
    s.update(|e, _, cx| e.run(vec![Command::SetAnimations { slide, animations: animations.clone() }], cx));
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    if open {
        s.update(|e, _, cx| e.toggle_animation_expanded((shape, 0, 0), cx));
    }
    s.settle();
    (s, shape)
}

/// (b) A text box's row is dragged with its paragraphs' rows (folded while it is carried) among the other rows.
fn text_box_row_drag() {
    let (mut s, _shape) = text_box_rows(true);
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap();
    let height = f32::from(b(1).origin.y - b(0).origin.y);
    s.screenshot("text-box-row-before-the-drag");
    // The text box's row is display row 1. Press, a small move, then down by a row and a third (its paragraphs' rows fold).
    let (x, y) = (f32::from(b(1).origin.x) + 120., f32::from(b(1).origin.y) + 20.);
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    assert_eq!(dnd.lift_offset("anim-table", 1), Some(0.), "the text box's row is lifted at the press");
    step(&mut s, mouse_move(x, y + 2., true));
    step(&mut s, mouse_move(x, y + 8., true));
    assert!(dnd.dragging().is_some());
    let mut places = vec![];
    for n in 1..=((height * 1.3 / 2.) as usize) {
        step(&mut s, mouse_move(x, y + 8. + n as f32 * 2., true));
        places.push(dnd.hover().map(|(_, at, place)| (at, place)));
    }
    let changes = places.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(changes <= 1, "{places:?}");
    s.screenshot("text-box-row-carried");
    step(&mut s, mouse_up(x, y + 8. + height * 1.3));
    let order: Vec<Option<u32>> = s.with(|e, _| e.slide().animations.iter().map(|a| a.paragraph.map(|p| p.0)).collect());
    assert_eq!(order, vec![None, None, Some(0), Some(1), Some(2)], "the text box's effects went together after the other row");
}

/// Every open text box folds when a text box's row is pressed to be dragged, and opens again when the button goes up or the drag
/// is given up with Escape; one that was folded stays folded.
fn folds_while_dragged() {
    let (mut s, shape) = text_box_rows(true);
    let expanded = |s: &mut Scenario| s.with(|e, _| e.pptx().unwrap().animation_expanded.contains(&(shape, 0, 0)));
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap();
    let (x, y) = (f32::from(b(1).origin.x) + 120., f32::from(b(1).origin.y) + 20.);
    assert!(expanded(&mut s));
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    assert!(!expanded(&mut s), "the text box folds at the press");
    step(&mut s, mouse_move(x, y + 12., true));
    assert!(dnd.dragging().is_some() && !expanded(&mut s));
    // The held row stays under the pointer after the rows moved.
    let lift = dnd.lift_offset("anim-table", 1).expect("held");
    assert!((lift - 12.).abs() < 1.5, "{lift}");
    step(&mut s, mouse_up(x, y + 12.));
    assert!(expanded(&mut s), "open again after the button goes up");
    // Given up with Escape: open again too.
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    step(&mut s, mouse_move(x, y + 12., true));
    assert!(!expanded(&mut s));
    s.key("escape");
    s.settle();
    assert!(dnd.dragging().is_none());
    assert!(expanded(&mut s), "open again after Escape");
    // One that was folded is not opened.
    drop(s);
    let (mut s, shape) = text_box_rows(false);
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap();
    let (x, y) = (f32::from(b(1).origin.x) + 120., f32::from(b(1).origin.y) + 20.);
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    step(&mut s, mouse_move(x, y + 12., true));
    step(&mut s, mouse_up(x, y + 12.));
    assert!(!s.with(|e, _| e.pptx().unwrap().animation_expanded.contains(&(shape, 0, 0))), "a folded text box stays folded");
}

/// A paragraph's row dragged past the first or the last of its text box's rows is not carried farther than that row's place, however
/// far the pointer goes (it is not drawn over the other rows).
fn child_stays_in_range() {
    let (mut s, _) = text_box_rows(true);
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    // Display rows: 0 another shape, 1 the text box, 2..=4 its paragraphs, 5 another shape.
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap();
    let (first, last) = (f32::from(b(2).origin.y), f32::from(b(4).origin.y));
    let (x, y) = (f32::from(b(3).origin.x) + 120., f32::from(b(3).origin.y) + 20.);
    let own = f32::from(b(3).origin.y);
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    // Up, in frames, far past the first paragraph's row and the text box's row.
    for dy in [10., 40., 80., 140., 220.] {
        step(&mut s, mouse_move(x, y - dy, true));
        let top = own + dnd.lift_offset("anim-table", 3).expect("held");
        assert!(top >= first - 0.5, "carried to {top} above the first paragraph's row at {first} (pointer {dy} up)");
        if dy >= 80. {
            assert!((top - first).abs() < 0.5, "held at the top end: {top} against {first}");
        }
    }
    // Down, far past the last one and the other shape's row.
    for dy in [10., 40., 80., 140., 220.] {
        step(&mut s, mouse_move(x, y + dy, true));
        let top = own + dnd.lift_offset("anim-table", 3).expect("held");
        assert!(top <= last + 0.5, "carried to {top} below the last paragraph's row at {last} (pointer {dy} down)");
        if dy >= 80. {
            assert!((top - last).abs() < 0.5, "held at the bottom end: {top} against {last}");
        }
    }
    s.screenshot("paragraph-row-held-at-the-bottom-end");
    step(&mut s, mouse_up(x, y + 220.));
}

/// What every drop the pane can make must keep: the same effects in another order, and the effects of one text box's paragraphs in
/// one block. Every row the pane lets be dragged (a plain row, a paragraph's row, a text box's row) on every row, before and after.
fn check_every_drop(animations: &[ibis_doc_model::Animation], what: &str) -> usize {
    use crate::shell::chrome::row_dnd::DropPlace;
    let key = |a: &ibis_doc_model::Animation| format!("{a:?}");
    let mut sorted: Vec<String> = animations.iter().map(key).collect();
    sorted.sort();
    let n = animations.len();
    // The runs, as the pane groups them (two or more paragraph effects of a shape in a row, none triggered).
    let runs: Vec<(usize, usize)> = {
        let mut v = Vec::new();
        let mut i = 0;
        while i < n {
            let (f, l) = crate::animation_ui::paragraph_run(animations, i);
            if l > f {
                v.push((f, l));
            }
            i = l + 1;
        }
        v
    };
    let mut drops = 0;
    for from in 0..n {
        let mut ids = vec![from as u64];
        if runs.iter().any(|(f, l)| *f == from && l > f) {
            ids.push(from as u64 | crate::animation_ui::GROUP_DRAG);
        }
        for id in ids {
            for target in 0..n {
                for place in [DropPlace::Before, DropPlace::After] {
                    let Some(after) = crate::animation_ui::reordered_effects(animations, id, target, place) else { continue };
                    drops += 1;
                    let case = format!("{what}: row {from} (id {id:#x}) dropped {place:?} row {target}");
                    assert_eq!(after.len(), n, "{case}: {} effects of {n} are left", after.len());
                    let mut now: Vec<String> = after.iter().map(key).collect();
                    now.sort();
                    assert_eq!(now, sorted, "{case}: not the same effects");
                    assert_ne!(after, animations, "{case}: a drop that changes nothing is reported as a change");
                    // Each run still lies in one block (its effects side by side); a text box's row takes them along in their order.
                    for (f, l) in &runs {
                        let mut places: Vec<usize> = (*f..=*l).map(|k| after.iter().position(|a| key(a) == key(&animations[k])).expect("a run's effect")).collect();
                        let ordered = places.windows(2).all(|w| w[0] < w[1]);
                        places.sort();
                        assert_eq!(places.last().unwrap() - places[0] + 1, l - f + 1, "{case}: the text box's effects are not in one block");
                        if id & crate::animation_ui::GROUP_DRAG != 0 {
                            assert!(ordered, "{case}: the text box's effects changed their order");
                        }
                    }
                }
            }
        }
    }
    drops
}

pub(crate) fn effect_of(shape: ibis_doc_model::ShapeId, paragraph: Option<u32>, delay: u32) -> ibis_doc_model::Animation {
    use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationStart, AnimationTrigger};
    Animation {
        shape, class: AnimationClass::Entrance, start: AnimationStart::OnClick, look: AnimationLook::Appear, delay, duration: 1, preset: (1, 0),
        repeat_count: AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: AnimationAfterEffect::Hold,
        trigger: AnimationTrigger::None, paragraph: paragraph.map(|i| (i, i)), chars: None, text_unit: None, path: Vec::new(),
        chart_element: None, chart_build: None, dgm_element: None, dgm_build: None, behaviors: Vec::new(),
    }
}

/// Synthetic lists mixing plain rows, text boxes' rows and their paragraphs' rows: no drop loses or makes an effect.
fn every_drop_synthetic() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let ids: Vec<ibis_doc_model::ShapeId> = s.with(|e, _| e.presentation().slides.iter().flat_map(|sl| sl.shapes.iter().map(|sh| sh.id)).collect());
    let mut distinct: Vec<ibis_doc_model::ShapeId> = Vec::new();
    for id in ids {
        if !distinct.contains(&id) {
            distinct.push(id);
        }
    }
    assert!(distinct.len() >= 3, "{distinct:?}");
    let (a, b, c) = (distinct[0], distinct[1], distinct[2]);
    let mut d = 0;
    let mut n = |shape, paragraph: Option<u32>| { d += 1; effect_of(shape, paragraph, d) };
    let lists = vec![
        vec![n(a, None), n(b, Some(0)), n(b, Some(1)), n(b, Some(2)), n(c, None)],
        vec![n(b, Some(0)), n(b, Some(1)), n(a, None), n(c, Some(0)), n(c, Some(1)), n(c, Some(2))],
        vec![n(a, None), n(b, Some(0)), n(b, Some(1)), n(c, Some(0)), n(c, Some(1))],
        vec![n(a, None), n(b, None), n(c, None)],
        vec![n(a, Some(0)), n(a, Some(1)), n(a, Some(2)), n(b, Some(0)), n(b, Some(1))],
        vec![n(a, None), n(a, Some(0)), n(a, Some(1)), n(b, None), n(a, None)],
    ];
    // The same paragraphs twice, a media row between: two text boxes' rows. The first dropped on the third row leaves three rows
    // (the paragraphs 0, 1 again after the paragraphs 1 do not join them into one row of four).
    let twice = vec![n(a, Some(0)), n(a, Some(1)), n(c, None), n(a, Some(0)), n(a, Some(1))];
    let moved = crate::animation_ui::reordered_effects(&twice, 0 | crate::animation_ui::GROUP_DRAG, 3, crate::shell::chrome::row_dnd::DropPlace::After).expect("a move");
    let rows_of = |v: &[ibis_doc_model::Animation]| { let (mut i, mut r) = (0, 0); while i < v.len() { i = crate::animation_ui::paragraph_run(v, i).1 + 1; r += 1; } r };
    assert_eq!((rows_of(&twice), rows_of(&moved)), (3, 3), "the text boxes' rows after the move");
    let keys = crate::animation_ui::box_keys(&twice);
    assert_ne!(keys[0], keys[3], "the two rows have their own keys");
    assert_eq!((keys[0], keys[1]), (keys[1], keys[0]));
    let mut lists = lists;
    lists.push(twice);
    let mut total = 0;
    for (i, list) in lists.iter().enumerate() {
        total += check_every_drop(list, &format!("list {i}"));
    }
    assert!(total > 100, "{total}");
}

/// The user's file: a slide's pane with the first row dropped on the third keeps every effect.
fn first_row_on_third_real_file() {
    let file = "test/github-issue/62/layout_test2.pptx";
    if !std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(file).exists() {
        return;
    }
    let mut s = Scenario::open(file);
    s.size(1280., 720.);
    s.page(3);
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    s.settle();
    let before: Vec<ibis_doc_model::Animation> = s.with(|e, _| e.slide().animations.clone());
    check_every_drop(&before, "layout_test2 page 3");
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let rows = (0..8).take_while(|i| dnd.row_bounds("anim-table", *i).is_some()).count();
    assert!(rows >= 3, "the pane lists {rows} rows of {} effects", before.len());
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap();
    let (x, y0, y1) = (f32::from(b(0).origin.x) + 120., f32::from(b(0).origin.y) + 20., f32::from(b(2).origin.y) + 30.);
    step(&mut s, mouse_move(x, y0, false));
    step(&mut s, mouse_down(x, y0));
    let mut y = y0;
    while y < y1 {
        y = (y + 4.).min(y1);
        step(&mut s, mouse_move(x, y, true));
    }
    step(&mut s, mouse_up(x, y1));
    let after: Vec<ibis_doc_model::Animation> = s.with(|e, _| e.slide().animations.clone());
    let key = |a: &ibis_doc_model::Animation| format!("{a:?}");
    let (mut kb, mut ka): (Vec<String>, Vec<String>) = (before.iter().map(key).collect(), after.iter().map(key).collect());
    kb.sort();
    ka.sort();
    let summary = |v: &[ibis_doc_model::Animation]| v.iter().map(|a| format!("{}:{:?}:{:?}:{:?}", a.shape.0, a.paragraph.map(|p| p.0), a.class, a.start)).collect::<Vec<_>>().join(" | ");
    assert_eq!(ka, kb, "{} effects before, {} after the drop", before.len(), after.len());
    let _ = summary(&after);
    let shown = (0..8).take_while(|i| dnd.row_bounds("anim-table", *i).is_some()).count();
    assert_eq!(shown, rows, "the pane lists {shown} rows after the drop, {rows} before: [{}] became [{}]", summary(&before), summary(&after));
}

/// The vertical place (device pixels, the top of the row) where the picture of a row (`crop`: its pixels as they stood when it
/// was at rest) lies best in `frame`, searching `from..=to`, and the fraction of the row's pixels that match there.
pub(crate) fn find_row(frame: &image::RgbaImage, crop: &image::RgbaImage, left: u32, from: i32, to: i32) -> (i32, f32) {
    // The place where the squared differences of the pixels are smallest (a row is mostly white, so the count of equal pixels
    // cannot tell a place from the ones a few pixels off).
    let mut best = (from, u64::MAX, 0.);
    for top in from..=to {
        if top < 0 || top as u32 + crop.height() > frame.height() {
            continue;
        }
        let (mut sum, mut same, mut all) = (0u64, 0u32, 0u32);
        for y in 0..crop.height() {
            for x in (0..crop.width()).step_by(2) {
                let (p, q) = (frame.get_pixel(left + x, top as u32 + y).0, crop.get_pixel(x, y).0);
                let d: i32 = (0..3).map(|c| (p[c] as i32 - q[c] as i32).abs()).max().unwrap();
                sum += (d * d) as u64;
                all += 1;
                if d <= 24 {
                    same += 1;
                }
            }
        }
        if sum < best.1 {
            best = (top, sum, same as f32 / all as f32);
        }
    }
    (best.0, best.2)
}

/// What is drawn, not the state: a paragraph's row of an open text box, dragged up and down in frames, is where the pointer holds
/// it (the pixels of the row are found in each frame), inside the range of its text box's paragraphs.
fn drawn_child_follows() {
    let file = "test/github-issue/62/layout_test2.pptx";
    if !std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(file).exists() {
        return;
    }
    let mut s = Scenario::open(file);
    s.size(1280., 720.);
    s.page(3);
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    // The first text box with paragraph effects, open.
    let (_shape, first, last, display_before) = s.with(|e, _| {
        let animations = &e.slide().animations;
        let mut i = 0;
        let mut display = 0;
        while i < animations.len() {
            let (f, l) = crate::animation_ui::paragraph_run(animations, i);
            if l > f {
                return (animations[f].shape, f, l, display);
            }
            display += 1;
            i = l + 1;
        }
        panic!("no text box with paragraph effects on page 3");
    });
    let key = s.with(|e, _| crate::animation_ui::box_keys(&e.slide().animations)[first]);
    s.update(|e, _, cx| e.toggle_animation_expanded(key, cx));
    s.settle();
    let children = last - first + 1;
    // The display rows: the plain ones before, the text box's row, then its paragraphs' rows.
    let (top_row, bottom_row) = (display_before + 1, display_before + children);
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let b = |i: usize| dnd.row_bounds("anim-table", i).unwrap_or_else(|| panic!("row {i} is not in the pane"));
    for row in [top_row, bottom_row] {
    let rest = image::open(s.screenshot(&format!("child-{row}-at-rest"))).unwrap().to_rgba8();
    let scale = rest.width() as f32 / 1280.;
    let height = f32::from(b(row).size.height);
    let (own, range_top, range_bottom) = (f32::from(b(row).origin.y), f32::from(b(top_row).origin.y), f32::from(b(bottom_row).origin.y));
    let (x, y) = (f32::from(b(row).origin.x) + 120., own + 20.);
    let px = |v: f32| (v * scale).round() as i32;
    let left = px(f32::from(b(row).origin.x) + 8.) as u32;
    let width = px(f32::from(b(row).size.width) - 40.) as u32;
    let crop = image::imageops::crop_imm(&rest, left, px(own) as u32 + 2, width, (px(height) - 4) as u32).to_image();
    step(&mut s, mouse_move(x, y, false));
    step(&mut s, mouse_down(x, y));
    let mut saved = vec![];
    let moves: Vec<f32> = vec![-4., -20., -height, -height * 3., 4., 20., height, height * 3., 0.];
    for (n, d) in moves.iter().enumerate() {
        step(&mut s, mouse_move(x, y + d, true));
        let frame = image::open(s.screenshot(&format!("child-{row}-frame-{n}-pointer-{}px", *d as i32))).unwrap().to_rgba8();
        let expected = (own + d).clamp(range_top, range_bottom);
        let (found, fraction) = find_row(&frame, &crop, left, px(expected) - px(height) * 4, px(expected) + px(height) * 4);
        let found_top = (found - 2) as f32 / scale;
        saved.push((n, d, expected, found_top, fraction, dnd.lift_offset("anim-table", row), dnd.hover()));
        assert!(fraction > 0.85, "frame {n}: the row is not drawn anywhere (best {fraction}) {saved:?}");
        assert!((found_top - expected).abs() < 1.5, "frame {n}: the row is drawn at {found_top}, the pointer holds it at {expected} (range {range_top}..{range_bottom}) {saved:?}");
    }
    step(&mut s, mouse_up(x, y));
    }
}

/// Two text boxes' rows of one shape (its paragraphs 0-1, an effect of another shape between, its paragraphs 2-3) are open or folded
/// each on its own, and the folding for a drag opens again only the ones that were open.
fn two_text_boxes_on_their_own() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1280., 720.);
    s.page(2);
    let (slide, shape, other) = s.with(|e, _| {
        let sl = e.slide();
        let sh = sl.shapes.iter().find(|sh| sh.text_body().is_some_and(|t| t.paragraphs.len() >= 4)).or_else(|| sl.shapes.iter().find(|sh| sh.text_body().is_some_and(|t| t.paragraphs.len() >= 3))).expect("a shape with paragraphs");
        (sl.id, sh.id, sl.shapes.iter().find(|o| o.id != sh.id).expect("another shape").id)
    });
    let animations = vec![effect_of(shape, Some(0), 1), effect_of(shape, Some(1), 2), effect_of(other, None, 3), effect_of(shape, Some(2), 4), effect_of(shape, Some(3), 5)];
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetAnimations { slide, animations: animations.clone() }], cx));
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    s.settle();
    let (one, two) = ((shape, 0u32, 0u32), (shape, 2u32, 0u32));
    let open = |s: &mut Scenario| -> (bool, bool) { s.with(|e, _| { let x = &e.pptx().unwrap().animation_expanded; (x.contains(&one), x.contains(&two)) }) };
    let rows = |s: &mut Scenario| -> usize {
        let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
        (0..12).take_while(|i| dnd.row_bounds("anim-table", *i).is_some()).count()
    };
    assert_eq!((open(&mut s), rows(&mut s)), ((false, false), 3), "both folded: two text box rows and the other shape's");
    s.update(|e, _, cx| e.toggle_animation_expanded(one, cx));
    s.settle();
    assert_eq!((open(&mut s), rows(&mut s)), ((true, false), 5), "the first opens, the second stays folded");
    s.screenshot("first-text-box-open");
    s.update(|e, _, cx| e.toggle_animation_expanded(one, cx));
    s.update(|e, _, cx| e.toggle_animation_expanded(two, cx));
    s.settle();
    assert_eq!((open(&mut s), rows(&mut s)), ((false, true), 5), "only the second opens");
    s.update(|e, _, cx| e.toggle_animation_expanded(one, cx));
    s.settle();
    assert_eq!((open(&mut s), rows(&mut s)), ((true, true), 7), "both open");
    s.screenshot("both-text-boxes-open");
    s.update(|e, _, cx| e.toggle_animation_expanded(two, cx));
    s.settle();
    assert_eq!((open(&mut s), rows(&mut s)), ((true, false), 5), "the second folds, the first stays open");
    // One shape's paragraphs 0 and 1 twice, a row of another shape between: the first row opens without the second.
    let twice = vec![effect_of(shape, Some(0), 1), effect_of(shape, Some(1), 2), effect_of(other, None, 3), effect_of(shape, Some(0), 4), effect_of(shape, Some(1), 5)];
    s.update(|e, _, cx| e.toggle_animation_expanded(one, cx));
    s.update(|e, _, cx| e.toggle_animation_expanded(two, cx));
    s.update(|e, _, cx| e.toggle_animation_expanded(two, cx));
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetAnimations { slide, animations: twice.clone() }], cx));
    s.settle();
    let (first, second) = ((shape, 0u32, 0u32), (shape, 0u32, 1u32));
    let open_of = |s: &mut Scenario| -> (bool, bool) { s.with(|e, _| { let x = &e.pptx().unwrap().animation_expanded; (x.contains(&first), x.contains(&second)) }) };
    assert_eq!((open_of(&mut s), rows(&mut s)), ((false, false), 3));
    s.update(|e, _, cx| e.toggle_animation_expanded(first, cx));
    s.settle();
    assert_eq!((open_of(&mut s), rows(&mut s)), ((true, false), 5), "the first row of two like it opens alone");
    s.update(|e, _, cx| e.toggle_animation_expanded(second, cx));
    s.settle();
    assert_eq!((open_of(&mut s), rows(&mut s)), ((true, true), 7));
}
