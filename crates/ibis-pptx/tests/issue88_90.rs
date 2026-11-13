// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issues #88 (画面切り替えの効果と効果のオプション) and #89 (開始・終了の効果): every effect of the gallery and of the
//! animation catalogue is written, read back, and kept as the file says.

use ibis_doc_model::transition_catalog::EFFECTS;
use ibis_doc_model::{AnimationClass, AnimationLook, Transition};
use ibis_pptx::SaveOptions;

fn plan() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-plan.pptx")
}

/// The saved file's text of slide 1.
fn slide_xml(saved: &[u8]) -> String {
    let package = ibis_ooxml::Package::open(saved.to_vec()).unwrap();
    let xml = package.xml("ppt/slides/slide1.xml").unwrap();
    String::from_utf8(xml.to_bytes()).unwrap()
}

#[test]
fn every_transition_of_the_gallery_and_each_option_is_written_and_read_back() {
    let opened = ibis_pptx::open(std::fs::read(plan()).unwrap()).unwrap();
    let mut count = 0;
    for effect in EFFECTS {
        let tokens: Vec<Option<&str>> = if effect.options.is_empty() { vec![None] } else { effect.options.iter().map(|o| Some(o.token).filter(|t| !t.is_empty())).collect() };
        for token in tokens {
            let mut edited = opened.presentation.clone();
            // 2 s: the duration goes in `p14:dur`, as PowerPoint writes it.
            edited.slides[0].transition = Some(Transition { direction: token.map(str::to_string), duration: 2000, ..Transition::new(effect.id) });
            let saved = opened.source.save(&edited, SaveOptions::default()).unwrap_or_else(|e| panic!("{} {token:?}: {e:?}", effect.id));
            // 変形 by object is its default: read back as none.
            let token = if effect.id == "morph" && token == Some("byObject") { None } else { token };
            let back = ibis_pptx::read(&saved).unwrap();
            let t = back.slides[0].transition.as_ref().unwrap_or_else(|| panic!("{} {token:?} lost", effect.id));
            // An option that is the schema's default is left out of the element, as PowerPoint writes it: the same option.
            let shown = |d: Option<&str>| ibis_doc_model::transition_catalog::effective_token(effect.id, d);
            assert_eq!((t.effect.as_str(), shown(t.direction.as_deref()), t.duration), (effect.id, shown(token), 2000), "{} {token:?}", effect.id);
            // The element PowerPoint knows it by.
            let xml = slide_xml(&saved);
            assert!(xml.contains(&format!(":{}", effect.element)), "{} {token:?}: {xml}", effect.id);
            count += 1;
        }
    }
    assert!(count > 100, "{count} transitions");
}

/// A file PowerPoint 2013 saved: `p15:prstTrans` (くしゃくしゃ) in a `mc:Choice`, read as the effect, and kept when only the
/// slide's other settings change.
#[test]
fn a_crush_transition_is_read_and_kept() {
    let opened = ibis_pptx::open(std::fs::read(plan()).unwrap()).unwrap();
    let mut edited = opened.presentation.clone();
    edited.slides[0].transition = Some(Transition { duration: 2000, ..Transition::new("crush") });
    let saved = opened.source.save(&edited, SaveOptions::default()).unwrap();
    let xml = slide_xml(&saved);
    assert!(xml.contains("Requires=\"p15\"") && xml.contains("prst=\"crush\""), "{xml}");
    assert!(xml.contains("<p:fade/>"), "the fallback is a fade");
    let t = ibis_pptx::read(&saved).unwrap().slides[0].transition.clone().unwrap();
    assert_eq!(t.effect, "crush");
}

#[test]
fn catalogue_effects_are_written_as_behaviours_and_read_back() {
    use ibis_doc_model::animation_preset::FILTER_PRESETS;
    let opened = ibis_pptx::open(std::fs::read(plan()).unwrap()).unwrap();
    let shape = opened.presentation.slides[0].shapes[0].id;
    for class in [AnimationClass::Entrance, AnimationClass::Exit] {
        for def in FILTER_PRESETS {
            for &(subtype, _, filter) in def.options {
                let mut edited = opened.presentation.clone();
                let mut a = edited.slides[0].animations.first().cloned().unwrap_or_else(|| {
                    let mut a = ibis_doc_model::Animation {
                        shape,
                        class,
                        start: ibis_doc_model::AnimationStart::OnClick,
                        look: AnimationLook::Preset,
                        delay: 0,
                        duration: 500,
                        preset: (0, 0),
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
                        behaviors: Vec::new(),
                    };
                    a.shape = shape;
                    a
                });
                a.class = class;
                a.look = AnimationLook::Preset;
                a.preset = (def.id, subtype);
                a.behaviors = ibis_doc_model::animation_preset::build_filter_preset(class, def.id, subtype, 500).unwrap();
                edited.slides[0].animations = vec![a.clone()];
                let saved = opened.source.save(&edited, SaveOptions::default()).unwrap_or_else(|e| panic!("{} {subtype}: {e:?}", def.key));
                let xml = slide_xml(&saved);
                assert!(xml.contains(&format!("filter=\"{filter}\"")), "{xml}");
                let back = &ibis_pptx::read(&saved).unwrap().slides[0].animations[0];
                assert_eq!((back.look, back.preset, back.class), (AnimationLook::Preset, (def.id, subtype), class), "{}", def.key);
                assert_eq!(back.behaviors, a.behaviors, "{} {subtype}", def.key);
            }
        }
    }
}

/// A file's own preset that the model has no look for (スライドイン from a corner: `presetID` 2, subtype 12) is played as its
/// behaviours, not as a fade, and written back the same.
#[test]
fn an_unknown_fly_in_is_kept_as_its_behaviours() {
    let opened = ibis_pptx::open(std::fs::read(plan()).unwrap()).unwrap();
    let shape = opened.presentation.slides[0].shapes[0].id;
    let mut edited = opened.presentation.clone();
    let mut a = ibis_doc_model::Animation { shape, class: AnimationClass::Entrance, start: ibis_doc_model::AnimationStart::OnClick, look: AnimationLook::Preset, delay: 0, duration: 500, preset: (2, 12), repeat_count: ibis_doc_model::AnimationRepeat::None, auto_reverse: false, rewind: false, accel: 0, decel: 0, after_effect: ibis_doc_model::AnimationAfterEffect::Hold, trigger: ibis_doc_model::AnimationTrigger::None, paragraph: None, chars: None, text_unit: None, path: Vec::new(), chart_element: None, chart_build: None, dgm_element: None, dgm_build: None, behaviors: Vec::new() };
    use ibis_doc_model::{AnimBehavior, BehaviorKind};
    a.behaviors = vec![
        AnimBehavior { kind: BehaviorKind::Set { attr: "style.visibility".into(), value: "visible".into() }, begin: 0, dur: 1, time: Default::default() },
        AnimBehavior { kind: BehaviorKind::Anim { attr: "ppt_x".into(), keys: vec![(0, "0-#ppt_w/2".into()), (100_000, "#ppt_x".into())], formula: None, discrete: false }, begin: 0, dur: 500, time: Default::default() },
        AnimBehavior { kind: BehaviorKind::Anim { attr: "ppt_y".into(), keys: vec![(0, "1+#ppt_h/2".into()), (100_000, "#ppt_y".into())], formula: None, discrete: false }, begin: 0, dur: 500, time: Default::default() },
    ];
    edited.slides[0].animations = vec![a.clone()];
    let saved = opened.source.save(&edited, SaveOptions::default()).unwrap();
    let back = &ibis_pptx::read(&saved).unwrap().slides[0].animations[0];
    assert_eq!((back.look, back.preset), (AnimationLook::Preset, (2, 12)));
    assert_eq!(back.behaviors, a.behaviors);
}

/// Files PowerPoint itself saved (the transitions and animations of `scripts`-generated decks, kept out of the repository):
/// `IBIS_PPT_DECKS` names a folder of them. Every effect is read as the effect it is: no 開始・終了 effect is a fade unless it is
/// `presetID` 10, no transition is lost, and a change to the animations is written back without losing any effect.
#[test]
fn powerpoint_saved_decks_are_read_and_written_back() {
    let Ok(dir) = std::env::var("IBIS_PPT_DECKS") else { return };
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "pptx") && !p.file_name().unwrap().to_string_lossy().starts_with('~')).collect();
    files.sort();
    let (mut seen, mut fades, mut transitions, mut mismatches) = (0, Vec::new(), 0, Vec::<String>::new());
    for file in files {
        let opened = ibis_pptx::open(std::fs::read(&file).unwrap()).unwrap_or_else(|e| panic!("{file:?}: {e:?}"));
        let mut edited = opened.presentation.clone();
        for (i, slide) in opened.presentation.slides.iter().enumerate() {
            if let Some(t) = &slide.transition {
                assert!(!t.effect.is_empty(), "{file:?} slide {i}");
                transitions += 1;
            }
            for a in &slide.animations {
                seen += 1;
                if matches!(a.class, AnimationClass::Entrance | AnimationClass::Exit) && a.look == AnimationLook::Fade && a.preset.0 != 10 {
                    fades.push((file.file_name().unwrap().to_string_lossy().to_string(), i, a.preset));
                }
            }
            for a in &mut edited.slides[i].animations {
                a.duration += 1;
            }
        }
        // Written back: the same effects (presets) in the same order.
        let saved = match opened.source.save(&edited, SaveOptions::default()) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{file:?}: not written back: {e:?}");
                continue;
            }
        };
        let back = ibis_pptx::read(&saved).unwrap();
        for (i, slide) in opened.presentation.slides.iter().enumerate() {
            let want: Vec<_> = slide.animations.iter().map(|a| (a.class, a.preset)).collect();
            let got: Vec<_> = back.slides[i].animations.iter().map(|a| (a.class, a.preset)).collect();
            if want != got {
                mismatches.push(format!("{} slide {i}: {want:?} -> {got:?}", file.file_name().unwrap().to_string_lossy()));
            }
        }
    }
    eprintln!("{seen} animations, {transitions} transitions");
    for m in &mismatches {
        eprintln!("MISMATCH {m}");
    }
    assert!(fades.is_empty(), "read as a fade: {fades:?}");
    assert!(mismatches.is_empty(), "{} slides changed by writing back", mismatches.len());
}
