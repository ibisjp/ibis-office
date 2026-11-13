// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! issues #88 (画面切り替えの効果と効果のオプション), #89 (開始・終了の効果), #90 (ズームの滑らかさ).
use crate::shell::scenario::Scenario;
use ibis_doc_model::animation_preset::build_filter_preset;
use ibis_doc_model::{Animation, AnimationClass, AnimationLook, ShapeId};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue88_every_gallery_effect_is_offered_with_its_options_and_saved", transition_gallery_and_options),
        ("pptx::issue89_catalogue_effects_are_added_with_options_and_saved", catalogue_effects),
        ("pptx::issue90_zoom_is_drawn_for_each_moment_not_in_steps", zoom_is_continuous),
        ("pptx::issue89_colour_and_font_effects_restyle_the_shape_and_light_speed_shears_it", restyled),
        ("pptx::issue89_shimmer_keeps_the_shape_shown", shimmer),
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
    s.toolbar_menu("motion", "pptx.anim.preset.blinds");
    let a = s.with(|e, _| e.slide().animations.first().cloned()).expect("an effect");
    assert_eq!((a.class, a.look, a.preset), (AnimationClass::Entrance, AnimationLook::Preset, (3, 10)));
    s.toolbar_menu("anim-options", "pptx.anim.preset.blinds.vertical");
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
        let a = ibis_pptx::animation_catalog::make(ShapeId(1), class, id, sub).unwrap();
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
    let a = ibis_pptx::animation_catalog::make(ShapeId(1), AnimationClass::Emphasis, 36, 0).unwrap();
    for k in 0..=20 {
        let p = k as f32 / 20.0;
        let m = motion_at(&a, ShapeMotion::at_rest(1.0), p, (100.0, 100.0, 400.0, 100.0), (960.0, 540.0));
        assert!(m.opacity > 0.99, "hidden at {p}: {m:?}");
        assert!(m.dx.abs() < 200.0 && m.dy.abs() < 200.0, "far away at {p}: {m:?}");
    }
}
