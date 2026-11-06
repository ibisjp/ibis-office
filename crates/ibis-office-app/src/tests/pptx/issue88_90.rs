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
    s.toolbar_menu("transition-options", "transition.dir.up");
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
    use crate::pptx::slideshow::{ShapeMotion, motion_at};
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
    let grow = |name: &str| AnimBehavior { kind: BehaviorKind::Anim { attr: name.into(), keys: vec![(0, "0".into()), (100_000, format!("#{name}"))], formula: None, discrete: false }, begin: 0, dur: 500 };
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
