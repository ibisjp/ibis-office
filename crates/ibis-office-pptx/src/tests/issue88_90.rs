// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! issues #88 (画面切り替えの効果と効果のオプション), #89 (開始・終了の効果), #90 (ズームの滑らかさ).
use crate::shell::scenario::Scenario;
use crate::ext::*;
use ibis_doc_model::animation_preset::build_filter_preset;
use ibis_doc_model::{Animation, AnimationClass, AnimationLook, ShapeId};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue88_every_gallery_effect_is_offered_with_its_options_and_saved", transition_gallery_and_options),
        ("pptx::issue89_catalogue_effects_are_added_with_options_and_saved", catalogue_effects),
        ("pptx::issue90_zoom_is_drawn_for_each_moment_not_in_steps", zoom_is_continuous),
        ("pptx::issue89_colour_and_font_effects_restyle_the_shape_and_light_speed_shears_it", restyled),
        ("pptx::issue89_shimmer_keeps_the_shape_shown", shimmer),
        ("pptx::issue89_shape_transitions_fade_between_the_slides_with_a_soft_band", shape_transitions_are_soft),
    ]
}

fn transition_gallery_and_options() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    // くしゃくしゃ and 紙飛行機 (PowerPoint 2013's `p15:prstTrans`) are in the gallery now.
    for effect in ibis_doc_model::transition_catalog::EFFECTS {
        s.toolbar_menu("transition", effect.key);
        assert_eq!(s.with(|e, _| e.slide().transition.as_ref().map(|t| t.effect.clone())).as_deref(), Some(effect.id));
        // 効果のオプション: each option of the effect is in the options menu and sets what the model keeps.
        for option in effect.options {
            s.toolbar_menu("transition-options", option.key);
            let want = Some(option.token).filter(|t| !t.is_empty()).map(str::to_string);
            assert_eq!(s.with(|e, _| e.slide().transition.as_ref().and_then(|t| t.direction.clone())), want, "{} {}", effect.id, option.token);
        }
    }
    // Saved and read back: the last of the gallery with its last option.
    s.toolbar_menu("transition", "transition.crush");
    s.save_as("test-issue88-crush.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].transition.as_ref().map(|t| t.effect.as_str()), Some("crush"));
    // 変形 and the others keep working: the menu still offers the old ones.
    s.toolbar_menu("transition", "transition.push");
    s.toolbar_menu("transition-options", "transition.dir.from-bottom");
    assert_eq!(s.with(|e, _| e.slide().transition.as_ref().and_then(|t| t.direction.clone())).as_deref(), Some("u"));
}

fn catalogue_effects() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.select_shape(0);
    // 開始: ブラインド (a catalogue effect, with a filter), then its option 垂直.
    s.toolbar_menu("motion", "pptx.animation.preset.blinds");
    let a = s.with(|e, _| e.slide().animations.first().cloned()).expect("an effect");
    assert_eq!((a.class, a.look, a.preset), (AnimationClass::Entrance, AnimationLook::Preset, (3, 10)));
    s.toolbar_menu("anim-options", "pptx.animation.preset.blinds.vertical");
    let a = s.with(|e, _| e.slide().animations.first().cloned()).expect("an effect");
    assert_eq!(a.preset, (3, 5));
    assert_eq!(a.behaviors, build_filter_preset(AnimationClass::Entrance, 3, 5, a.duration).unwrap());
    s.save_as("test-issue89-blinds.pptx");
    let saved = s.reopen();
    let back = &saved.slides[0].animations[0];
    assert_eq!((back.look, back.preset, &back.behaviors), (AnimationLook::Preset, (3, 5), &a.behaviors));
}

/// ズーム: the size at each moment of the effect grows by the effect's own time, in steps of at most a hundredth of it (not the
/// 24 steps it was drawn in), from nothing to the shape's own size.
fn zoom_is_continuous() {
    use crate::slideshow::{ShapeMotion, motion_at};
    use ibis_doc_model::animation_preset::{AnimBehavior, BehaviorKind};
    let anim = |look, behaviors| Animation {
        shape: ShapeId(1),
        class: AnimationClass::Entrance,
        start: ibis_doc_model::AnimationStart::OnClick,
        look,
        delay: 0,
        duration: 500,
        preset: (23, 16),
        repeat_count: ibis_doc_model::AnimationRepeat::None,
        auto_reverse: false,
        rewind: false,
        accel: 0,
        decel: 0,
        after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
        trigger: ibis_doc_model::AnimationTrigger::None,
        paragraph: None,
        chars: None,
        text_unit: None,
        path: Vec::new(),
        chart_element: None,
        chart_build: None,
        dgm_element: None,
        dgm_build: None,
        behaviors,
    };
    let grow = |name: &str| AnimBehavior { kind: BehaviorKind::Anim { attr: name.into(), keys: vec![(0, "0".into()), (100_000, format!("#{name}"))], formula: None, discrete: false }, begin: 0, dur: 500, time: Default::default() };
    let from = ShapeMotion::at_rest(0.0);
    for a in [anim(AnimationLook::Zoom, Vec::new()), anim(AnimationLook::Preset, vec![grow("ppt_w"), grow("ppt_h")])] {
        let mut last = 0.0f32;
        for k in 0..=100 {
            let p = k as f32 / 100.0;
            let m = motion_at(&a, from, p, (100.0, 100.0, 200.0, 100.0), (960.0, 540.0));
            // Linear in time, as the effect says (no easing): the size is the share of the time gone.
            assert!((m.grow.0 - p).abs() < 1e-4 && (m.grow.1 - p).abs() < 1e-4, "{:?} at {p}: {:?}", a.look, m.grow);
            assert!(m.grow.0 + 1e-6 >= last && m.grow.0 - last <= 0.0101, "a jump at {p}");
            last = m.grow.0;
        }
        assert!((last - 1.0).abs() < 1e-4);
    }
}

/// An emphasis that changes colours or fonts, and ライト スピード, change how the shape is drawn: its fill goes toward the theme's accent
/// 2, the text takes the bold and the italic, and the shape leans.
fn restyled() {
    use crate::slideshow::{ShapeMotion, set_theme_colors, styled_shape};
    use ibis_doc_model::animation_preset::{PptBox, sample};
    use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Rgb, Shape, ShapeKind, TextBody, TextStyle};
    let body = TextBody::plain("Abc", TextStyle::default());
    let shape = Shape::new(ShapeId(1), EmuRect::new(0, 0, 1000, 500), ShapeKind::Auto(AutoShape::text_box(Fill::Solid(Color::opaque(Rgb::from_rgb(0, 100, 200))), body)));
    set_theme_colors(&[("accent2".to_string(), Rgb::from_rgb(200, 100, 0))]);
    let b = PptBox { x: 0.5, y: 0.5, w: 0.1, h: 0.1 };
    let at = |class, id, sub, ms| {
        let a = ibis_formats::animation_catalog::make(ShapeId(1), class, id, sub).unwrap();
        styled_shape(&shape, ShapeMotion::at_rest(1.0), &[sample(&a.behaviors, ms, b)])
    };
    // 塗りつぶしの色, half way.
    let (s, shear) = at(AnimationClass::Emphasis, 1, 2, 1000.0);
    let ShapeKind::Auto(auto) = &s.kind else { panic!() };
    assert_eq!(auto.fill, Fill::Solid(Color::opaque(Rgb::from_rgb(100, 100, 100))));
    assert_eq!(shear, 0.0);
    // フォント スタイル: bold, no italic, no underline.
    let (s, _) = at(AnimationClass::Emphasis, 5, 1, 100.0);
    let body = s.text_body().unwrap();
    assert!(body.style.bold && !body.style.italic);
    assert!(body.paragraphs[0].runs.iter().all(|r| r.style.bold == Some(true) && !r.style.underline));
    // ライト スピード at the top of its lean.
    let (_, shear) = at(AnimationClass::Entrance, 34, 0, 800.0);
    assert!((shear + 1.0).abs() < 1e-6, "{shear}");
}

/// シマー (強調 36) moves, turns and squeezes the shape and brings it back: it is shown the whole time.
fn shimmer() {
    use crate::slideshow::{ShapeMotion, motion_at};
    let a = ibis_formats::animation_catalog::make(ShapeId(1), AnimationClass::Emphasis, 36, 0).unwrap();
    for k in 0..=20 {
        let p = k as f32 / 20.0;
        let m = motion_at(&a, ShapeMotion::at_rest(1.0), p, (100.0, 100.0, 400.0, 100.0), (960.0, 540.0));
        assert!(m.opacity > 0.99, "hidden at {p}: {m:?}");
        assert!(m.dx.abs() < 200.0 && m.dy.abs() < 200.0, "far away at {p}: {m:?}");
    }
}

/// 図形（円・ひし形・プラス・くさび形）: half way through the show's transition the screen is the old slide (red), the new one (blue) and,
/// between them, a band of both (the new slide's share between 0.1 and 0.9), not a hard edge. The band's share of the picture and the
/// places of the old and new slide follow the measured shape (`transition_plan::soft_shape_alpha`).
fn shape_transitions_are_soft() {
    use ibis_doc_model::{Color, Command, Fill, Rgb, Transition};
    for effect in ["circle", "diamond", "plus", "wedge"] {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        s.size(960., 540.);
        let (first, second) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[1].id));
        let red = Fill::Solid(Color::opaque(Rgb::from_rgb(255, 0, 0)));
        let blue = Fill::Solid(Color::opaque(Rgb::from_rgb(0, 0, 255)));
        s.update(|e, _, cx| {
            e.run(
                vec![
                    Command::SetBackground { slide: first, fill: red },
                    Command::SetBackground { slide: second, fill: blue },
                    Command::SetTransition { slide: second, transition: Some(Transition { duration: 2000, ..Transition::new(effect) }) },
                ],
                cx,
            )
        });
        s.page(1);
        s.update(|e, w, cx| e.start_slideshow(w, cx));
        s.settle();
        s.key("space");
        let shot = |ms: f32, s: &mut Scenario| {
            s.update(|e, _, cx| {
                if let Some(show) = e.slideshow_mut() {
                    show.transition_at = Some(ms);
                }
                cx.notify();
            });
            s.settle();
            let path = s.screenshot(&format!("{effect}-{ms:.0}"));
            image::open(path).unwrap().to_rgba8()
        };
        let mid = shot(1000.0, &mut s);
        // The slides' own colours (pure red and blue; text and the show's bar are dark or grey), and what lies between them.
        let is_old = |p: &image::Rgba<u8>| p[0] > 240 && p[1] < 15 && p[2] < 15;
        let is_new = |p: &image::Rgba<u8>| p[2] > 240 && p[0] < 15 && p[1] < 15;
        let coloured = |p: &image::Rgba<u8>| p[0] as u32 + p[2] as u32 > 240;
        let (mut blended, mut old, mut new, mut counted) = (0usize, 0usize, 0usize, 0usize);
        for p in mid.pixels().filter(|p| coloured(p)) {
            counted += 1;
            if is_old(p) {
                old += 1;
            } else if is_new(p) {
                new += 1;
            } else {
                blended += 1;
            }
        }
        // The band is a mix of the two slides' colours (red and blue), not lightened by the slide's paper under its fill.
        let clean = mid.pixels().filter(|p| coloured(p) && !is_old(p) && !is_new(p) && p[1] < 16).count();
        assert!(clean as f64 > 0.9 * blended as f64, "{effect}: {clean} of {blended} band pixels are a plain mix of red and blue");
        let f = |n: usize| n as f64 / counted.max(1) as f64;
        // The shapes' band is a third of the way across (`w = big / 3`), so a good part of the picture is blended, and the old and new slide
        // are both seen.
        assert!(f(blended) > 0.15, "{effect}: only {:.3} of the picture is in the band (old {:.3}, new {:.3})", f(blended), f(old), f(new));
        assert!(f(old) > 0.05 && f(new) > 0.05, "{effect}: old {:.3}, new {:.3}", f(old), f(new));
        // At the start the old slide, and at the end the new one, whole.
        let start = shot(0.0, &mut s);
        let blue_share = start.pixels().filter(|p| is_new(p)).count() as f64 / start.pixels().filter(|p| coloured(p)).count().max(1) as f64;
        assert!(blue_share < 0.01, "{effect}: the new slide shows at the start ({blue_share})");
    }
}
