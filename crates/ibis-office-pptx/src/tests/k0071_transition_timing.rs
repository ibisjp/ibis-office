// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0071: the times of a slide's transition and its animations (design: `tasks/doc/test-design/K0071.md`; the numbers P1... are that document's).
//! A reviewer's video of 0.8.9 on Windows (YouTube, Q0nNx3pDl_M) found: the animations began with the transition instead of after it, a slide with
//! no transition effect but a leftover `p14:dur` waited that long before moving on, and page curl and origami ended earlier than PowerPoint's.
//! The tests were `#[ignore]`d until the fix (K0071); now they are all regular scenarios (`ignored()` here lists them for `scenarios::all`); the ones that already passed were
//! regression tests, the others were fixed (P1-P3).
//!
//! The scenarios' clock is simulated (`Scenario::wait`); the animations' own clock reads real time, so a test that depends on it lets both pass (`elapse`).
use crate::shell::scenario::Scenario;
use crate::ext::*;
use ibis_doc_model::{Animation, AnimationStart, Color, Command, Fill, Rgb, Transition};

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::k0071_p1_an_after_previous_effect_begins_when_the_transition_has_ended", p1_after_previous_after_transition),
        ("pptx::k0071_p2_a_with_previous_first_effect_also_begins_when_the_transition_has_ended", p2_with_previous_after_transition),
        ("pptx::k0071_p3_a_slide_with_no_effect_and_a_leftover_dur_moves_on_after_advtm_alone", p3_no_effect_ignores_dur),
        ("pptx::k0071_p4_a_slide_with_an_effect_moves_on_advtm_after_the_transition_has_ended", p4_effect_then_advtm),
        ("pptx::k0071_p5_the_effects_of_a_group_are_timed_by_their_starts_and_delays", p5_group_times),
        ("pptx::k0071_p6_page_curl_has_a_curling_surface_and_a_shadow_not_a_slide_shrinking", p6_page_curl_shadow),
        ("pptx::k0071_p7_origami_is_still_moving_at_1_7_s_of_a_2_s_transition", p7_origami_end),
    ]
}

/// An entrance effect (fade in) of `shape`.
fn fade_in(shape: ibis_doc_model::ShapeId, start: AnimationStart, delay: u32, duration: u32) -> Animation {
    use ibis_doc_model::{AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationTrigger};
    Animation {
        shape,
        class: AnimationClass::Entrance,
        start,
        look: AnimationLook::Fade,
        delay,
        duration,
        preset: (10, 0),
        repeat_count: AnimationRepeat::None,
        auto_reverse: false,
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
    }
}

fn solid(r: u8, g: u8, b: u8) -> Fill {
    Fill::Solid(Color::opaque(Rgb::from_rgb(r, g, b)))
}

/// Both clocks: the simulated one (timers) and real time (the animations' clock).
fn elapse(s: &mut Scenario, ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
    s.wait(ms);
}

/// How red the pictured slide is (0 white, 1 full red), over the whole picture: the red box fading in or out is measured as the drop of the green
/// channel from the white paper.
fn redness(path: std::path::PathBuf) -> f64 {
    let img = image::open(path).unwrap().to_rgba8();
    let total: u64 = img.pixels().filter(|p| p[0] > 200).map(|p| (255 - p[1]) as u64).sum();
    total as f64
}

/// Slide 2 of the sample as a white slide with its first shape a red box with one entrance fade (`start`, no delay, 1 s) and a transition `effect`
/// of 1.5 s; the show started on slide 1 and moved to slide 2. The picture is taken `ms` after that.
fn picture_after_the_move(start: AnimationStart, ms: u64) -> f64 {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(960., 540.);
    let (first, second, shape) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[1].id, e.presentation().slides[1].shapes[0].id));
    s.update(|e, _, cx| {
        e.run(
            vec![
                Command::SetBackground { slide: first, fill: solid(255, 255, 255) },
                Command::SetBackground { slide: second, fill: solid(255, 255, 255) },
                Command::SetFill { slide: second, shape, fill: solid(255, 0, 0) },
                Command::SetTransition { slide: second, transition: Some(Transition { duration: 1500, ..Transition::new("push") }) },
            ],
            cx,
        )
    });
    s.update(|e, _, cx| e.run(vec![Command::SetAnimations { slide: second, animations: vec![fade_in(shape, start, 0, 1000)] }], cx));
    s.page(1);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    s.key("space");
    elapse(&mut s, ms);
    redness(s.screenshot(&format!("k0071-{ms}")))
}

/// P1: the slide comes in by a 1.5 s transition and its first effect (fade in, 直前の動作の後, 1 s) begins when the transition has ended
/// (PowerPoint's order, measured in the video: 2.04-2.22 s apart with a 2 s transition). 250 ms after the transition: the fade is a quarter through; it
/// is not yet complete, which it would be if it had begun with the transition.
fn p1_after_previous_after_transition() {
    let full = picture_after_the_move(AnimationStart::AfterPrevious, 3500);
    assert!(full > 0.0, "the red box shows at the end");
    let early = picture_after_the_move(AnimationStart::AfterPrevious, 1750);
    let share = early / full;
    assert!(share > 0.02 && share < 0.8, "250 ms after the transition the fade has just begun: {share:.2} of the finished box");
}

/// P2: the same for an effect started 直前の動作と同時 (the first of the slide).
fn p2_with_previous_after_transition() {
    let full = picture_after_the_move(AnimationStart::WithPrevious, 3500);
    let early = picture_after_the_move(AnimationStart::WithPrevious, 1750);
    let share = early / full;
    assert!(share > 0.02 && share < 0.8, "250 ms after the transition the fade has just begun: {share:.2} of the finished box");
}

/// The page the show is on (0-based) of a show started on slide 1 of `file`, `ms` of the scenario's clock after the start.
fn page_after(file: &str, ms: &[u64]) -> Vec<Option<usize>> {
    page_after_with(file, false, ms)
}

/// `page_after`; with `quick_first` slide 1 is made to move on at once and take no time (so that slide 2's own times can be read alone).
fn page_after_with(file: &str, quick_first: bool, ms: &[u64]) -> Vec<Option<usize>> {
    let mut s = Scenario::open(file);
    s.size(960., 540.);
    if quick_first {
        let first = s.with(|e, _| e.presentation().slides[0].id);
        s.update(|e, _, cx| e.run(vec![Command::SetTransition { slide: first, transition: Some(Transition { effect: String::new(), duration: 0, after: Some(0), on_click: false, ..Transition::new("") }) }], cx));
    }
    s.update(|e, _, cx| {
        let mut properties = e.presentation().show_properties.clone();
        properties.use_timings = true;
        e.run(vec![Command::SetShowProperties { properties }], cx);
    });
    s.page(1);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    let mut out = Vec::new();
    let mut done = 0;
    for &at in ms {
        s.wait(at - done);
        done = at;
        out.push(s.with(|e, _| e.presentation().slide_index(e.current_slide)));
    }
    out
}

/// P3: the file of the video's kind: slide 1 has no effect, `advTm="500"` and a leftover `p14:dur="2000"` (PowerPoint writes it even for none). The show
/// moves on after 0.5 s (a transition of no effect takes no time), not after 2.5 s.
fn p3_no_effect_ignores_dur() {
    let pages = page_after("test/editing/k0071-no-effect-with-dur.pptx", &[400, 700]);
    assert_eq!(pages[0], Some(0), "not before the advance time");
    assert_eq!(pages[1], Some(1), "moved on 0.7 s after the start: the leftover p14:dur is not waited for");
}

/// P4: slide 2 of the same file has a fade of 1 s and `advTm="500"`: it moves on 0.5 s after its transition has ended (1.5 s after it came in; slide 1 is
/// made to move on at once so that only slide 2's times are read), as PowerPoint counts it from the end of the transition (measured: the 2026-11-15 video result).
fn p4_effect_then_advtm() {
    let pages = page_after_with("test/editing/k0071-no-effect-with-dur.pptx", true, &[100, 1400, 1700]);
    assert_eq!(pages[0], Some(1), "on slide 2 (slide 1 moved on at once)");
    assert_eq!(pages[1], Some(1), "still on slide 2 before the transition (1 s) and the advance time (0.5 s) are over");
    assert_eq!(pages[2], Some(2), "moved on to slide 3 after them (1.5 s)");
}

/// P5: the group of effects begins when the slide is on (the group's own times, not the transition's): 直前の動作の後 follows the effect before
/// it, 直前の動作と同時 begins with it, delays add. (The times in the video's deck: 8 effects within 0.12 s of PowerPoint's once the transition was
/// taken away.)
fn p5_group_times() {
    use ibis_doc_model::{AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationTrigger, ShapeId};
    let anim = |start, delay, duration| Animation {
        shape: ShapeId(1),
        class: AnimationClass::Entrance,
        start,
        look: AnimationLook::Fade,
        delay,
        duration,
        preset: (10, 0),
        repeat_count: AnimationRepeat::None,
        auto_reverse: false,
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
    let animations = [
        anim(AnimationStart::AfterPrevious, 0, 1000),
        anim(AnimationStart::WithPrevious, 250, 500),
        anim(AnimationStart::AfterPrevious, 100, 400),
        anim(AnimationStart::OnClick, 0, 300),
    ];
    let groups = crate::slideshow::animation_groups(&animations);
    assert_eq!(groups.len(), 2, "the click starts a second group");
    assert!(groups[0].auto && !groups[1].auto);
    assert_eq!(groups[0].effects, [(0, 0, 1000), (1, 250, 750), (2, 1100, 1500)], "begin and end after the group starts");
    assert_eq!(groups[1].effects, [(3, 0, 300)]);
}

/// The picture of the show `ms` into a transition `effect` of `duration` ms from a white slide to a white slide (the slides' own text is taken
/// away), set to that time as `--export ui transition-at:` does.
fn white_transition_at(effect: &str, duration: u32, ms: f32) -> image::RgbaImage {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(960., 540.);
    let (first, second) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[1].id));
    let shapes = s.with(|e, _| (e.presentation().slides[0].shapes.iter().map(|x| x.id).collect::<Vec<_>>(), e.presentation().slides[1].shapes.iter().map(|x| x.id).collect::<Vec<_>>()));
    s.update(|e, _, cx| {
        e.run(
            vec![
                Command::DeleteShape { slide: first, shapes: shapes.0.clone(), with_attached: true },
                Command::DeleteShape { slide: second, shapes: shapes.1.clone(), with_attached: true },
                Command::SetBackground { slide: first, fill: solid(255, 255, 255) },
                Command::SetBackground { slide: second, fill: solid(255, 255, 255) },
                Command::SetTransition { slide: second, transition: Some(Transition { duration, ..Transition::new(effect) }) },
            ],
            cx,
        )
    });
    s.page(1);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    s.update(|e, _, _| {
        if let Some(show) = e.slideshow_mut() {
            show.transition_at = Some(ms);
        }
    });
    s.key("space");
    s.settle();
    image::open(s.screenshot(&format!("k0071-{effect}-{ms:.0}"))).unwrap().to_rgba8()
}

/// P6: page curl (ページカール) of a white slide over a white slide: PowerPoint rolls the old slide's page up (a curling surface, shaded) and casts a
/// shadow on the new slide, so the picture at half time has grey that is neither slide; a page that only shrinks sideways leaves nothing but white
/// (what 0.8.9 drew, per the video). The numbers of the shading are for the Windows measurement (W3) to set; here only that it exists.
fn p6_page_curl_shadow() {
    for effect in ["pageCurlDouble", "pageCurlSingle"] {
        let mid = white_transition_at(effect, 2000, 1000.0);
        let shaded = mid.pixels().filter(|p| p[3] > 0 && p[0] < 235 && (p[0] as i32 - p[1] as i32).abs() < 12 && (p[1] as i32 - p[2] as i32).abs() < 12).count();
        let share = shaded as f64 / (mid.width() * mid.height()) as f64;
        assert!(share > 0.01, "{effect}: only {share:.4} of the picture is shaded at half time: no curling surface and shadow");
    }
}

/// P7: origami of a 2 s transition, an old slide red and a new one blue: PowerPoint's motion is 1.88 s long in the video, ibisOffice's 1.50 s, so
/// at 1.7 s the old slide's paper is still on the screen (not yet the plain new slide).
fn p7_origami_end() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(960., 540.);
    let (first, second) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[1].id));
    let shapes = s.with(|e, _| (e.presentation().slides[0].shapes.iter().map(|x| x.id).collect::<Vec<_>>(), e.presentation().slides[1].shapes.iter().map(|x| x.id).collect::<Vec<_>>()));
    s.update(|e, _, cx| {
        e.run(
            vec![
                Command::DeleteShape { slide: first, shapes: shapes.0.clone(), with_attached: true },
                Command::DeleteShape { slide: second, shapes: shapes.1.clone(), with_attached: true },
                Command::SetBackground { slide: first, fill: solid(255, 0, 0) },
                Command::SetBackground { slide: second, fill: solid(0, 0, 255) },
                Command::SetTransition { slide: second, transition: Some(Transition { duration: 2000, ..Transition::new("origami") }) },
            ],
            cx,
        )
    });
    s.page(1);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    s.key("space");
    let not_blue = |ms: f32, s: &mut Scenario| {
        s.update(|e, _, cx| {
            if let Some(show) = e.slideshow_mut() {
                show.transition_at = Some(ms);
            }
            cx.notify();
        });
        s.settle();
        let img = image::open(s.screenshot(&format!("k0071-origami-{ms:.0}"))).unwrap().to_rgba8();
        let coloured = img.pixels().filter(|p| p[0] as u32 + p[2] as u32 > 240).count().max(1);
        let blue = img.pixels().filter(|p| p[2] > 240 && p[0] < 15 && p[1] < 15).count();
        1.0 - blue as f64 / coloured as f64
    };
    assert!(not_blue(1700.0, &mut s) > 0.005, "origami has already ended at 1.7 s: PowerPoint's motion lasts to about 1.88 s");
    assert!(not_blue(2000.0, &mut s) < 0.005, "the new slide is whole at the end");
}
