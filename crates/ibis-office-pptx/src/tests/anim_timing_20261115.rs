// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-15: how long an effect runs and what it does as the time goes (ECMA-376 19.5.3, `p:cTn` `repeatCount`, `autoRev`, `endSync`):
//! a repeated effect plays the same motion again (it is not stretched), an effect that runs back plays back and ends where it began, and an
//! effect repeating until the next click or the end of the slide goes on. Frames are read as pixels (the red box's width and height).
use crate::shell::scenario::Scenario;
use crate::ext::*;
use ibis_doc_model::{AnimationLook, AnimationRepeat, Color, Command, Fill, Rgb};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::anim_timing_repeat_plays_the_motion_again_not_stretched", repeat_loops),
        ("pptx::anim_timing_auto_reverse_runs_back_and_ends_where_it_began", auto_reverse_runs_back),
        ("pptx::anim_timing_repeat_until_next_click_goes_on", repeat_indefinitely),
        ("pptx::anim_timing_repeat_2_5_stops_the_last_pass_half_way", repeat_half_a_time),
        ("pptx::anim_timing_clock_and_timeline_arithmetic", clock_arithmetic),
        ("pptx::transition_plain_fade_mixes_the_two_slides_as_wholes", fade_is_a_plain_mix),
        ("pptx::anim_dim_colours_the_shape_at_the_next_click_and_round_trips", dim_at_next_click),
        ("pptx::transition_random_shows_a_drawn_effect_of_the_gallery_not_a_fade", random_transition),
    ]
}

/// The first slide's title as a red box on a white slide with a スピン of `by` (1/60000 degree) for 1 s, repeated `repeat` times and running
/// back when `rev`; the size (width, height) of the red part `ms` into the effect.
fn red_size(by: i32, repeat: AnimationRepeat, rev: bool, ms: f32) -> (u32, u32) {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(960., 540.);
    let (slide, shape) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[0].shapes[0].id));
    s.update(|e, _, cx| {
        e.run(
            vec![
                Command::SetBackground { slide, fill: Fill::Solid(Color::opaque(Rgb::from_rgb(255, 255, 255))) },
                Command::SetFill { slide, shape, fill: Fill::Solid(Color::opaque(Rgb::from_rgb(255, 0, 0))) },
            ],
            cx,
        )
    });
    s.select_shape(0);
    s.toolbar_menu("motion", "add.pptx.animation.em.spin");
    s.update(|e, _, cx| {
        let mut all = e.slide().animations.clone();
        let a = &mut all[0];
        a.look = AnimationLook::Spin(by);
        a.duration = 1000;
        a.delay = 0;
        a.repeat_count = repeat;
        a.auto_reverse = rev;
        a.accel = 0;
        a.decel = 0;
        e.run(vec![Command::SetAnimations { slide, animations: all }], cx);
    });
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    s.key("space");
    s.update(|e, _, cx| {
        if let Some(show) = e.slideshow_mut() {
            show.anim_at = Some(ms);
        }
        cx.notify();
    });
    s.settle();
    let path = s.screenshot(&format!("anim-timing-{by}-{rev}-{ms:.0}"));
    let img = image::open(path).unwrap().to_rgba8();
    let (mut left, mut right, mut top, mut bottom) = (i32::MAX, 0, i32::MAX, 0);
    for y in 0..img.height() as i32 {
        for x in 0..img.width() as i32 {
            let p = img.get_pixel(x as u32, y as u32);
            if p[0] > 200 && p[0] as i32 - p[1] as i32 > 25 && (p[1] as i32 - p[2] as i32).abs() < 12 {
                (left, right, top, bottom) = (left.min(x), right.max(x), top.min(y), bottom.max(y));
            }
        }
    }
    assert!(right > left && bottom > top, "no red box at {ms} ms");
    ((right - left) as u32, (bottom - top) as u32)
}

fn tall(size: (u32, u32)) -> bool {
    size.1 > size.0
}

/// A turn of 360 degrees in 1 s, twice: a quarter of each pass is 90 degrees (the wide box stands up), half of it 180 (flat again). Stretched over
/// 2 s, 1250 ms would be 225 degrees, which is not tall.
fn repeat_loops() {
    let full = 21_600_000;
    let two = AnimationRepeat::Count(2);
    assert!(tall(red_size(full, two, false, 250.0)), "90 degrees into the first pass");
    assert!(!tall(red_size(full, two, false, 500.0)), "180 degrees into the first pass");
    assert!(tall(red_size(full, two, false, 1250.0)), "the second pass turns again from the start: 90 degrees");
    assert!(!tall(red_size(full, two, false, 2500.0)), "ended after two passes");
}

/// A quarter turn in 1 s that runs back: 90 degrees at 1 s, back to 0 at 2 s (a wide box again, which a forward-only effect would not be).
fn auto_reverse_runs_back() {
    let quarter = 5_400_000;
    assert!(tall(red_size(quarter, AnimationRepeat::None, true, 1000.0)), "the turn is done at the end of the first half");
    assert!(!tall(red_size(quarter, AnimationRepeat::None, true, 1990.0)), "back where it began at the end of the second half");
    assert!(!tall(red_size(quarter, AnimationRepeat::None, true, 4000.0)), "ended where it began");
    assert!(tall(red_size(quarter, AnimationRepeat::None, false, 4000.0)), "(without running back it stays turned)");
}

/// 2.5 passes of a 360 degree turn: ended, the box is left at 180 degrees (flat), where 3 whole passes would leave 360 (also flat) and
/// 2 whole passes the same; a 90 degree quarter would tell, so a turn of 180 degrees in 1 s, 2.5 times: left at 90 (tall).
fn repeat_half_a_time() {
    let half_turn = 10_800_000;
    let two_and_a_half = AnimationRepeat::Thousandths(2500);
    assert!(!tall(red_size(half_turn, two_and_a_half, false, 2250.0)), "a quarter into the third pass: 45 degrees");
    assert!(tall(red_size(half_turn, two_and_a_half, false, 2500.0)), "stopped half way: 90 degrees");
    assert!(tall(red_size(half_turn, two_and_a_half, false, 6000.0)), "and left there");
}

/// 1 s turns without end: 5250 ms is 90 degrees into the sixth pass, for the next click and for the end of the slide alike.
fn repeat_indefinitely() {
    let full = 21_600_000;
    for repeat in [AnimationRepeat::UntilNextClick, AnimationRepeat::UntilEndOfSlide] {
        assert!(tall(red_size(full, repeat, false, 5250.0)), "{repeat:?} goes on turning");
        assert!(!tall(red_size(full, repeat, false, 5500.0)), "{repeat:?} 180 degrees into the sixth pass");
    }
}

fn clock_arithmetic() {
    use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass, AnimationStart, AnimationTrigger, ShapeId};
    let anim = |repeat, rev| Animation {
        shape: ShapeId(1),
        class: AnimationClass::Emphasis,
        start: AnimationStart::OnClick,
        look: AnimationLook::Spin(21_600_000),
        delay: 0,
        duration: 1000,
        preset: (8, 0),
        repeat_count: repeat,
        auto_reverse: rev,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: AnimationAfterEffect::Hold,
        trigger: AnimationTrigger::None,
        paragraph: None,
        chars: None,
        text_unit: None,
        path: Vec::new(),
        chart_element: None,
        chart_build: None,
        dgm_element: None,
        dgm_build: None,
        behaviors: Vec::new(),
    };
    let near = |a: (f32, bool), b: (f32, bool)| (a.0 - b.0).abs() < 1e-4 && a.1 == b.1;
    let a = anim(AnimationRepeat::Count(3), false);
    assert_eq!((a.pass_ms(), a.active_ms(), a.span_ms()), (1000, Some(3000), 3000));
    assert!(near(a.clock(0.0), (0.0, false)) && near(a.clock(250.0), (0.25, false)) && near(a.clock(1250.0), (0.25, false)));
    assert!(near(a.clock(2999.0), (0.999, false)) && near(a.clock(3000.0), (1.0, true)) && near(a.clock(9000.0), (1.0, true)));
    let r = anim(AnimationRepeat::Count(2), true);
    assert_eq!((r.pass_ms(), r.active_ms()), (2000, Some(4000)));
    assert!(near(r.clock(500.0), (0.5, false)) && near(r.clock(1500.0), (0.5, false)) && near(r.clock(2250.0), (0.25, false)) && near(r.clock(4000.0), (0.0, true)));
    assert_eq!(r.settled_progress(), 0.0);
    // 2.5 times: the last pass stops half way (180 degrees of the turn), and that is where it is left.
    let h = anim(AnimationRepeat::Thousandths(2500), false);
    assert_eq!((h.active_ms(), h.span_ms()), (Some(2500), 2500));
    assert!(near(h.clock(2250.0), (0.25, false)) && near(h.clock(2500.0), (0.5, true)) && near(h.clock(7000.0), (0.5, true)));
    assert!((h.settled_progress() - 0.5).abs() < 1e-4);
    let u = anim(AnimationRepeat::UntilEndOfSlide, false);
    assert_eq!((u.active_ms(), u.span_ms()), (None, 1000));
    assert!(near(u.clock(100_250.0), (0.25, false)));
}

/// ランダム: half way through, the picture is the one of an effect of the gallery (the one `random_effect` draws for the change's number),
/// not the plain fade it used to be.
fn random_transition() {
    use ibis_doc_model::{Color, Command, Fill, Rgb, Transition};
    let frame = |transition: Transition| {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        s.size(960., 540.);
        let (first, second) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[1].id));
        s.update(|e, _, cx| {
            e.run(
                vec![
                    Command::SetBackground { slide: first, fill: Fill::Solid(Color::opaque(Rgb::from_rgb(255, 0, 0))) },
                    Command::SetBackground { slide: second, fill: Fill::Solid(Color::opaque(Rgb::from_rgb(0, 0, 255))) },
                    Command::SetTransition { slide: second, transition: Some(transition.clone()) },
                ],
                cx,
            )
        });
        s.page(1);
        s.update(|e, w, cx| e.start_slideshow(w, cx));
        s.settle();
        // (Set to a time before the move on, so that the draw is made from the change's number, not the clock.)
        s.update(|e, _, _| {
            if let Some(show) = e.slideshow_mut() {
                show.transition_at = Some(1000.0);
            }
        });
        s.key("space");
        s.settle();
        let mut changes = 0;
        s.update(|e, _, _| changes = e.slideshow_mut().map_or(0, |show| show.changes));
        let path = s.screenshot("random");
        (image::open(path).unwrap().to_rgba8().into_raw(), changes as u64)
    };
    let (random, changes) = frame(Transition { duration: 2000, ..Transition::new("random") });
    let (fade, _) = frame(Transition { duration: 2000, ..Transition::new("fade") });
    // The show draws with the number of the change that brought the slide in as the seed (as it does when set to a time).
    let (effect, direction) = ibis_doc_model::transition_plan::random_effect(changes);
    let (same, _) = frame(Transition { duration: 2000, direction: direction.map(str::to_string), ..Transition::new(effect) });
    assert!(random == same, "the frame is not that of {effect} {direction:?} (seed {changes})");
    let drawn = [changes];
    let all_fades = drawn.iter().all(|&seed| ibis_doc_model::transition_plan::random_effect(seed).0 == "fade");
    assert!(all_fades || random != fade, "not the plain fade");
}

/// 2026-11-15 (PowerPoint's video, 0.0149 squared error at half time before, 0.0005 after): a plain fade at half time is the plain mean of the
/// two pictures. The new slide's own fill over its paper must not let the old slide through each in turn (opacity on each element).
fn fade_is_a_plain_mix() {
    use ibis_doc_model::{Color, Command, Fill, Rgb, Transition};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(960., 540.);
    let (first, second) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[1].id));
    let (shape1, shape2) = s.with(|e, _| (e.presentation().slides[0].shapes[0].id, e.presentation().slides[1].shapes[0].id));
    let solid = |r, g, b| Fill::Solid(Color::opaque(Rgb::from_rgb(r, g, b)));
    s.update(|e, _, cx| {
        e.run(
            vec![
                Command::SetBackground { slide: first, fill: solid(255, 0, 0) },
                Command::SetFill { slide: first, shape: shape1, fill: solid(255, 0, 0) },
                Command::SetBackground { slide: second, fill: solid(0, 0, 255) },
                Command::SetFill { slide: second, shape: shape2, fill: solid(0, 255, 0) },
                Command::SetTransition { slide: second, transition: Some(Transition { duration: 2000, ..Transition::new("fade") }) },
            ],
            cx,
        )
    });
    s.page(1);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    s.update(|e, _, _| {
        if let Some(show) = e.slideshow_mut() {
            show.transition_at = Some(1000.0);
        }
    });
    s.key("space");
    s.settle();
    let img = image::open(s.screenshot("fade-mix")).unwrap().to_rgba8();
    // Inside the new slide's green shape the mix of red and green is (127, 127, 0); with the paper's blue let through it would be (63, 127, 63).
    let green: Vec<_> = img.pixels().filter(|p| p[1] > 100 && p[1] < 150 && p[0] > 90 && p[0] < 170 && p[2] < 100).collect();
    assert!(green.len() > 200, "no mixed shape found: {}", green.len());
    assert!(green.iter().all(|p| p[2] < 20), "the paper shows through the shape: {:?}", green[0]);
}

/// 淡色表示 (`p:animClr` of `ppt_c`, `masterRel="nextClick"`, `afterEffect="1"`, the colour in `p:to` `a:srgbClr`, as PowerPoint writes it): the
/// shape keeps its colours after its own effect and takes the dimming colour when the next click's effect begins; the colour is saved and read back.
fn dim_at_next_click() {
    use ibis_doc_model::{AnimationAfterEffect, Color, Command, Fill, Rgb};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(960., 540.);
    let (slide, first, second) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[0].shapes[0].id, e.presentation().slides[0].shapes[1].id));
    let solid = |r, g, b| Fill::Solid(Color::opaque(Rgb::from_rgb(r, g, b)));
    s.update(|e, _, cx| {
        e.run(
            vec![
                Command::SetBackground { slide, fill: solid(255, 255, 255) },
                Command::SetFill { slide, shape: first, fill: solid(255, 0, 0) },
                Command::SetFill { slide, shape: second, fill: solid(0, 160, 0) },
            ],
            cx,
        )
    });
    s.select_shape(0);
    s.toolbar_menu("motion", "pptx.animation.in.appear");
    s.select_shape(1);
    s.toolbar_menu("motion", "pptx.animation.in.appear");
    let blue = Rgb::from_rgb(0, 0, 255);
    s.update(|e, _, cx| {
        let mut all = e.slide().animations.clone();
        for a in all.iter_mut() {
            a.after_effect = if a.shape == first { AnimationAfterEffect::Dim(blue) } else { AnimationAfterEffect::Hold };
        }
        e.run(vec![Command::SetAnimations { slide, animations: all }], cx);
    });
    // Saved as PowerPoint writes it, and read back with the colour.
    s.save_as("pptx-anim-dim.pptx");
    let saved = s.reopen();
    let dimmed = saved.slides[0].animations.iter().find(|a| a.shape == first).expect("the first shape's effect");
    assert_eq!(dimmed.after_effect, AnimationAfterEffect::Dim(blue));
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    let count = |s: &mut Scenario, name: &str| {
        let img = image::open(s.screenshot(name)).unwrap().to_rgba8();
        let red = img.pixels().filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60).count();
        let blue = img.pixels().filter(|p| p[2] > 200 && p[0] < 60 && p[1] < 60).count();
        (red, blue)
    };
    s.key("space");
    s.settle();
    let (red, blue_px) = count(&mut s, "dim-1");
    assert!(red > 500 && blue_px == 0, "after its own effect the shape keeps its colour: {red} {blue_px}");
    s.key("space");
    s.settle();
    let (red, blue_px) = count(&mut s, "dim-2");
    assert!(red == 0 && blue_px > 500, "at the next click it takes the dimming colour: {red} {blue_px}");
}
