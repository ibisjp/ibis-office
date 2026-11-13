// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! PowerPoint's gallery of animation effects (開始・強調・終了・アニメーションの軌跡), each as the `p:par` PowerPoint itself wrote
//! for it (`data/animation-presets.tsv`, made by `scripts/gen-animation-presets.py` from a file PowerPoint 16.0.20527 saved, one
//! effect to a slide). An effect added from the menu is that `p:par` read as a file's would be, so what it plays and what it
//! saves are what PowerPoint's own effect does.

use std::sync::OnceLock;

use ibis_doc_model::{Animation, AnimationClass, ShapeId};
use roxmltree::Document;

const DATA: &str = include_str!("../data/animation-presets.tsv");

/// An effect of the gallery: its `presetID` in its class, the i18n id of its name and its options (`presetSubtype`, i18n id).
#[derive(Debug)]
pub struct CatalogEffect {
    pub class: AnimationClass,
    pub id: u32,
    pub key: &'static str,
    pub options: Vec<(u32, &'static str)>,
}

fn class_code(class: AnimationClass) -> Option<&'static str> {
    match class {
        AnimationClass::Entrance => Some("entr"),
        AnimationClass::Emphasis => Some("emph"),
        AnimationClass::Exit => Some("exit"),
        AnimationClass::Path => Some("path"),
        AnimationClass::Media => None,
    }
}

fn class_of(code: &str) -> Option<AnimationClass> {
    match code {
        "entr" => Some(AnimationClass::Entrance),
        "emph" => Some(AnimationClass::Emphasis),
        "exit" => Some(AnimationClass::Exit),
        "path" => Some(AnimationClass::Path),
        _ => None,
    }
}

/// Every effect of the gallery, in the order PowerPoint lists them.
pub fn effects() -> &'static [CatalogEffect] {
    static ALL: OnceLock<Vec<CatalogEffect>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all: Vec<CatalogEffect> = Vec::new();
        for line in DATA.lines() {
            let mut cols = line.splitn(4, '\t');
            let (Some(code), Some(id), Some(sub)) = (cols.next(), cols.next(), cols.next()) else { continue };
            let (Some(class), Ok(id_n), Ok(sub_n)) = (class_of(code), id.parse::<u32>(), sub.parse::<u32>()) else { continue };
            // The i18n ids live as long as the program.
            let key: &'static str = Box::leak(format!("pptx.anim.cat.{code}.{id}").into_boxed_str());
            let position = match all.iter().position(|e| e.class == class && e.id == id_n) {
                Some(p) => p,
                None => {
                    all.push(CatalogEffect { class, id: id_n, key, options: Vec::new() });
                    all.len() - 1
                }
            };
            all[position].options.push((sub_n, Box::leak(format!("{key}.{sub}").into_boxed_str())));
        }
        // An effect with one option and no option name has none to choose.
        all
    })
}

/// The effects of `class`.
pub fn effects_of(class: AnimationClass) -> impl Iterator<Item = &'static CatalogEffect> {
    effects().iter().filter(move |e| e.class == class)
}

/// The effect `id` of `class`.
pub fn effect(class: AnimationClass, id: u32) -> Option<&'static CatalogEffect> {
    effects().iter().find(|e| e.class == class && e.id == id)
}

/// Effect `id` of `class` with option `subtype` on `shape`, as PowerPoint writes it (the first option for one the effect does not
/// have); `None` for an effect not in the gallery.
pub fn make(shape: ShapeId, class: AnimationClass, id: u32, subtype: u32) -> Option<Animation> {
    let code = class_code(class)?;
    let (mut exact, mut first) = (None, None);
    for line in DATA.lines() {
        let mut cols = line.splitn(4, '\t');
        let (Some(c), Some(i), Some(s), Some(xml)) = (cols.next(), cols.next(), cols.next(), cols.next()) else { continue };
        if c != code || i.parse() != Ok(id) {
            continue;
        }
        first.get_or_insert(xml);
        if s.parse() == Ok(subtype) {
            exact = Some(xml);
            break;
        }
    }
    let xml = exact.or(first)?;
    let text = format!(r#"<r xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">{xml}</r>"#);
    let doc = Document::parse(&text).ok()?;
    let node = doc.descendants().find(|n| n.is_element() && n.tag_name().name() == "cTn" && n.attribute("presetClass").is_some())?;
    crate::animation::read_effect(node, 0, shape, ibis_doc_model::AnimationTrigger::None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_doc_model::AnimationLook;

    /// The gallery has what PowerPoint's has (by effect, options not counted): 53 開始 and 終了 effects each, 33 強調, 64 軌跡; but
    /// for the 3D ones PowerPoint's own list has no name for (1 開始, 3 強調).
    #[test]
    fn the_gallery_has_every_effect() {
        let count = |c| effects_of(c).count();
        assert_eq!((count(AnimationClass::Entrance), count(AnimationClass::Exit)), (52, 52));
        assert_eq!(count(AnimationClass::Emphasis), 30);
        assert_eq!(count(AnimationClass::Path), 64);
    }

    /// Each effect is made as the file's own: its class and preset, and (but for Appear and the ones the model names) its behaviours.
    #[test]
    fn every_effect_is_made_with_its_behaviours() {
        for e in effects() {
            for &(sub, _) in &e.options {
                let a = make(ShapeId(1), e.class, e.id, sub).unwrap_or_else(|| panic!("{:?} {} {}", e.class, e.id, sub));
                assert_eq!((a.class, a.preset), (e.class, (e.id, sub)));
                if a.look == AnimationLook::Preset {
                    assert!(!a.behaviors.is_empty(), "{:?} {} {}", e.class, e.id, sub);
                }
            }
        }
    }

    /// Every behaviour of every effect of the gallery changes what `sample` says (alone, half way through): none is left unevaluated
    /// (#89). What the player cannot show is a decision, listed here: `fill.type` says the fill is solid (the colours it goes with set
    /// that), `style.fontFamily` names the typeface the text has.
    #[test]
    fn every_behaviour_of_every_effect_is_evaluated() {
        use ibis_doc_model::BehaviorKind;
        use ibis_doc_model::animation_preset::{PptBox, PresetState, sample};
        let b = PptBox { x: 0.5, y: 0.5, w: 0.2, h: 0.2 };
        let mut left = std::collections::BTreeSet::new();
        let mut total = 0;
        for e in effects() {
            for &(sub, _) in &e.options {
                let a = make(ShapeId(1), e.class, e.id, sub).unwrap();
                for bh in &a.behaviors {
                    total += 1;
                    let mut one = bh.clone();
                    one.begin = 0;
                    one.time.remove = false;
                    if sample(std::slice::from_ref(&one), f64::from(bh.dur) / 2.0, b) == PresetState::default() {
                        let what = match &bh.kind {
                            BehaviorKind::Set { attr, .. } => format!("set {attr}"),
                            BehaviorKind::Anim { attr, .. } => format!("anim {attr}"),
                            BehaviorKind::Effect { filter, .. } => format!("filter {filter}"),
                            BehaviorKind::Opaque(raw) => format!("opaque {}", raw.chars().take(30).collect::<String>()),
                            other => format!("{other:?}"),
                        };
                        left.insert(format!("{what} ({:?} {})", e.class, e.id));
                    }
                }
            }
        }
        assert!(total > 500, "{total}");
        let left: Vec<String> = left.into_iter().filter(|w| !w.starts_with("set fill.type")).collect();
        assert!(left.is_empty(), "not evaluated: {left:#?}");
    }

    /// ライト スピード leans the shape (`xshear` goes to -1 and back, from 600 ms on, 200 ms each way): measured on PowerPoint for Windows
    /// (Microsoft 365 16.0.20527, `CreateVideo`, 30 frames a second): the lean starts at 0.6 s, is a full height of the shape at the
    /// top edge's lean about 0.8 s, and is gone at 1.0 s.
    #[test]
    fn light_speed_leans_the_shape_at_the_measured_times() {
        use ibis_doc_model::animation_preset::{PptBox, sample};
        let a = make(ShapeId(1), AnimationClass::Entrance, 34, 0).unwrap();
        let b = PptBox { x: 0.5, y: 0.5, w: 0.5, h: 0.5 };
        let shear = |ms: f64| sample(&a.behaviors, ms, b).shear;
        assert_eq!(shear(500.0), None);
        assert!((shear(800.0).unwrap() + 1.0).abs() < 1e-9, "{:?}", shear(800.0));
        assert!(shear(700.0).unwrap() < -0.3 && shear(700.0).unwrap() > -1.0);
        assert!(shear(1010.0).is_none_or(|v| v.abs() < 1e-9), "{:?}", shear(1010.0));
    }

    /// 塗りつぶしの色 goes linearly from the shape's own colour to the theme's accent 2 in its 2 s (measured on PowerPoint: the
    /// fill's red at 0.25 s steps was 0.10, 0.23, 0.34, 0.48, 0.61, 0.74, 0.85, 0.98 of the way).
    #[test]
    fn change_fill_color_goes_to_the_theme_colour() {
        use ibis_doc_model::animation_preset::{PptBox, sample};
        use ibis_doc_model::{Rgb, animation_color::ColorSlot};
        let a = make(ShapeId(1), AnimationClass::Emphasis, 1, 2).unwrap();
        let b = PptBox { x: 0.5, y: 0.5, w: 0.5, h: 0.5 };
        let state = sample(&a.behaviors, 1000.0, b);
        assert_eq!(state.fill_on, Some(true));
        let c = &state.colors[0];
        assert_eq!(c.slot, ColorSlot::Fill);
        let theme = |n: &str| (n == "accent2").then_some(Rgb::from_rgb(200, 100, 0));
        assert_eq!(c.resolve(Rgb::from_rgb(0, 100, 200), &theme), Rgb::from_rgb(100, 100, 100));
    }

    /// 塗りつぶしの色 against PowerPoint's frames (Windows, Microsoft 365 16.0.20527, `CreateVideo`, 30 frames a second): the fill's red
    /// channel every 0.25 s from the start was 0, 0.095, 0.225, 0.342, 0.476, 0.606, 0.74, 0.848, 0.978 of the way (the first frames of
    /// a video lag the show by about 0.07 s), so the model's straight line over 2 s is within 0.04 of each.
    #[test]
    fn change_fill_color_runs_as_powerpoints_frames_do() {
        use ibis_doc_model::animation_preset::{PptBox, sample};
        use ibis_doc_model::Rgb;
        let a = make(ShapeId(1), AnimationClass::Emphasis, 1, 2).unwrap();
        let b = PptBox { x: 0.5, y: 0.5, w: 0.5, h: 0.5 };
        let theme = |n: &str| (n == "accent2").then_some(Rgb::from_rgb(237, 125, 49));
        let measured = [0.0, 0.095, 0.225, 0.342, 0.476, 0.606, 0.74, 0.848, 0.978];
        for (k, want) in measured.iter().enumerate() {
            let state = sample(&a.behaviors, k as f64 * 250.0, b);
            let red = state.colors.first().map_or(0, |c| c.resolve(Rgb::from_rgb(0, 112, 192), &theme).r());
            let got = f64::from(red) / 237.0;
            assert!((got - want).abs() < 0.04, "{k}: {got} against {want}");
        }
    }

    /// 暗く (by hsl: saturation -12.5%, lightness -25.1%): PowerPoint's frames went from a blue of lightness 0.386 to 0.127 (-0.259;
    /// the video's colours are a little off the file's), the model's is -0.251.
    #[test]
    fn darken_takes_a_quarter_of_the_lightness() {
        use ibis_doc_model::animation_preset::{PptBox, sample};
        use ibis_doc_model::Rgb;
        let a = make(ShapeId(1), AnimationClass::Emphasis, 24, 0).unwrap();
        let b = PptBox { x: 0.5, y: 0.5, w: 0.5, h: 0.5 };
        let light = |c: Rgb| (f64::from(c.r().max(c.g()).max(c.b())) + f64::from(c.r().min(c.g()).min(c.b()))) / 510.0;
        let base = Rgb::from_rgb(0, 112, 192);
        let state = sample(&a.behaviors, 500.0, b);
        let fill = state.colors.iter().find(|c| c.slot == ibis_doc_model::animation_color::ColorSlot::Fill).expect("a fill colour");
        let dark = fill.resolve(base, &|_| None);
        assert!((light(base) - light(dark) - 0.259).abs() < 0.02, "{} {}", light(base), light(dark));
    }

    /// フォントの色: the text keeps its colour for the 2 s and takes the new one at the end (PowerPoint's frames, 30 a second: black
    /// to 1.9 s, the accent colour from 2.0 s).
    #[test]
    fn change_font_color_changes_at_the_end() {
        use ibis_doc_model::animation_preset::{PptBox, sample};
        use ibis_doc_model::Rgb;
        let a = make(ShapeId(1), AnimationClass::Emphasis, 3, 2).unwrap();
        let b = PptBox { x: 0.5, y: 0.5, w: 0.5, h: 0.5 };
        let theme = |n: &str| (n == "accent2").then_some(Rgb::from_rgb(237, 125, 49));
        let at = |ms: f64| sample(&a.behaviors, ms, b).colors.first().map(|c| c.resolve(Rgb::BLACK, &theme));
        assert_eq!(at(1900.0), Some(Rgb::BLACK));
        assert_eq!(at(2000.0), Some(Rgb::from_rgb(237, 125, 49)));
    }

    /// 中心で回転 (exit 43) moves the shape 0.31 down in 0.6 s and then a behaviour that says `ppt_y` (where it is now) holds it there, as
    /// PowerPoint's frames do (the shape is 0.31 of the slide lower at 0.65 s), and クレジット (28) moves it from above to below.
    #[test]
    fn exits_hold_where_they_took_the_shape_and_credits_move_it() {
        use ibis_doc_model::animation_preset::{PptBox, sample};
        let b = PptBox { x: 0.5, y: 0.4, w: 0.5, h: 0.2 };
        let a = make(ShapeId(1), AnimationClass::Exit, 43, 0).unwrap();
        let y = sample(&a.behaviors, 700.0, b).y.unwrap();
        assert!((y - 0.71).abs() < 1e-6, "{y}");
        let a = make(ShapeId(1), AnimationClass::Exit, 28, 0).unwrap();
        assert_eq!(a.look, AnimationLook::Preset);
        assert!(sample(&a.behaviors, 1.0, b).y.unwrap() < 0.0);
    }
}

