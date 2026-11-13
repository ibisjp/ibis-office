// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-06, issue 86: the calls that play, pause and stop a video or an audio are rows of the animation pane (as
//! PowerPoint lists them), the slide show plays them with one click or Space for each group, and the animation pane's
//! rows slide aside for a dragged row (the table's live reorder, as the chart series list does).
use crate::shell::scenario::Scenario;
use ibis_doc_model::{AnimationClass as C, AnimationLook as L, MediaCommand};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue86_media_rows_in_the_pane_and_played_by_space_in_the_show", media_rows),
        ("pptx::issue86_animation_pane_rows_slide_aside_while_dragging", rows_slide),
    ]
}

const FILE: &str = "test/github-issue/62/layout_test.pptx";

fn present() -> bool {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(FILE).exists()
}

fn playing(s: &mut Scenario) -> Vec<bool> {
    s.with(|e, _| e.pptx().unwrap().clips.iter().map(|c| c.player.is_playing()).collect())
}

fn media_rows() {
    if !present() {
        return;
    }
    let mut s = Scenario::open(FILE);
    s.size(1280., 720.);
    // P2: PowerPoint lists 再生 (by click) and 一時停止 (by a click on the video).
    s.page(2);
    s.update(|e, _, cx| e.toggle_animation_pane(cx));
    s.settle();
    let rows: Vec<(C, L)> = s.with(|e, _| e.slide().animations.iter().map(|a| (a.class, a.look)).collect());
    assert_eq!(rows, vec![(C::Media, L::Media(MediaCommand::Play)), (C::Media, L::Media(MediaCommand::Pause))]);
    s.screenshot("p2-animation-pane");
    // The show: the first Space plays the video (the slide stays), the second goes on to the next slide.
    let slide = s.with(|e, _| e.current_slide);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.current_slide), slide);
    assert!(playing(&mut s).is_empty(), "nothing plays before the first Space");
    s.key("space");
    s.settle();
    assert_eq!(s.with(|e, _| e.current_slide), slide, "the first Space plays the video and stays");
    assert_eq!(playing(&mut s), vec![true], "the video plays");
    s.screenshot("p2-show-playing");
    s.key("space");
    s.settle();
    assert_ne!(s.with(|e, _| e.current_slide), slide, "the second Space goes to the next slide");
    s.update(|e, w, cx| e.end_slideshow(w, cx));

    // P3: the text effects, then 再生 of the audio, as the last row.
    s.page(3);
    s.settle();
    let classes: Vec<C> = s.with(|e, _| e.slide().animations.iter().map(|a| a.class).collect());
    assert_eq!(classes.last(), Some(&C::Media), "{classes:?}");
    s.screenshot("p3-animation-pane");
    let groups = s.with(|e, _| crate::slideshow::animation_groups(&e.slide().animations).len());
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    for _ in 0..groups - 1 {
        s.key("space");
        s.settle();
        assert!(playing(&mut s).is_empty(), "the text effects come first");
    }
    s.key("space");
    s.settle();
    assert_eq!(playing(&mut s), vec![true], "the audio plays with the last Space");
    s.update(|e, w, cx| e.end_slideshow(w, cx));
}

fn rows_slide() {
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
    let (x, y0, y2) = (f32::from(b(0).origin.x) + 120., f32::from(b(0).origin.y) + 10., f32::from(b(2).origin.y) + 30.);
    let h = f32::from(b(0).size.height);
    // The press on the first row and a drag down over the last, the button still down: the other rows stand one row higher
    // (the dragged row's place is taken) and the dragged row is where it would drop.
    let at = |x: f32, y: f32| gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y));
    s.dispatch(vec![
        gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent { position: at(x, y0), ..Default::default() }),
        gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent { button: gpui_kit::MouseButton::Left, position: at(x, y0), click_count: 1, ..Default::default() }),
        gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent { position: at(x, y0 + 8.), pressed_button: Some(gpui_kit::MouseButton::Left), ..Default::default() }),
        gpui_kit::PlatformInput::MouseMove(gpui_kit::MouseMoveEvent { position: at(x, y2), pressed_button: Some(gpui_kit::MouseButton::Left), ..Default::default() }),
    ]);
    // Past the 160 ms move.
    std::thread::sleep(std::time::Duration::from_millis(400));
    s.settle();
    let shifts: Vec<Option<(i32, i32, usize)>> = (0..3).map(|i| dnd.live_shift("anim-table", i)).collect();
    assert_eq!(shifts.iter().map(|s| s.map(|(_, to, _)| to)).collect::<Vec<_>>(), vec![Some(2), Some(-1), Some(-1)], "{shifts:?}");
    s.screenshot("pane-rows-while-dragging");
    let _ = h;
    s.dispatch(vec![gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent { button: gpui_kit::MouseButton::Left, position: at(x, y2), click_count: 1, ..Default::default() })]);
    s.settle();
    s.screenshot("pane-rows-after-drop");
    let order: Vec<usize> = s.with(|e, _| e.slide().animations.iter().map(|a| e.slide().shape_index(a.shape).unwrap()).collect());
    assert_eq!(order, vec![0, 1, 0], "the first effect went last");
}
