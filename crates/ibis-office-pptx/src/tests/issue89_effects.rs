// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! issue #89: every effect of PowerPoint's gallery, played as the behaviours of the file PowerPoint wrote say
//! (`test/github-issue/89/shape-animation-check.pptx`, 291 slides, one effect each; not published: the scenarios return without it).
use crate::slideshow::{ShapeMotion, motion_at, preset_reveal};
use crate::shell::scenario::Scenario;
use ibis_doc_model::{AnimationClass, AnimationLook};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue89_every_effect_of_the_check_file_changes_what_is_drawn", every_effect_is_drawn),
        ("pptx::issue89_frames_of_the_check_file_at_set_times", frames),
        ("pptx::issue89_looks_of_the_effects_of_a_file", looks),
        ("pptx::issue89_gallery_effects_are_in_the_menu_and_kept_through_a_save", gallery_in_menu),
    ]
}

const FILE: &str = "test/github-issue/89/shape-animation-check.pptx";

fn present() -> bool {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(FILE).exists()
}

/// What an effect leaves drawn at `p` of its time: opacity, offset, turn, size, and the area of what a filter shows.
fn signature(a: &ibis_doc_model::Animation, p: f32) -> [i64; 8] {
    let frame = (300.0, 200.0, 360.0, 200.0);
    let slide = (960.0, 540.0);
    let from = ShapeMotion::at_rest(if a.class == AnimationClass::Entrance { 0.0 } else { 1.0 });
    let m = motion_at(a, from, p, frame, slide);
    let area = preset_reveal(a, p, frame).map_or(1.0, |r| r.iter().map(|r| (r[2] - r[0]) * (r[3] - r[1])).sum::<f64>());
    let q = |v: f32| (v * 100.0).round() as i64;
    [q(m.opacity), q(m.dx), q(m.dy), q(m.turn), q(m.grow.0), q(m.grow.1), (area * 100.0).round() as i64, 0]
}

/// An entrance, exit or emphasis effect that is not Appear/Disappear draws something different along its way; one that does not is
/// an effect the player plays as nothing (listed in the failure). Effects the player does not draw yet are named in `UNDRAWN`.
fn every_effect_is_drawn() {
    if !present() {
        return;
    }
    let data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(FILE)).unwrap();
    let p = ibis_pptx::read(&data).unwrap();
    let mut nothing = Vec::new();
    for (i, slide) in p.slides.iter().enumerate() {
        for a in &slide.animations {
            if a.look != AnimationLook::Preset || a.class == AnimationClass::Path {
                continue;
            }
            let steps: Vec<[i64; 8]> = (0..=19).map(|k| signature(a, k as f32 / 20.0 * 0.999)).collect();
            if steps.iter().all(|s| *s == steps[0]) && a.preset.0 != 1 {
                nothing.push(format!("slide {} {:?} preset {}/{}", i + 1, a.class, a.preset.0, a.preset.1));
            }
        }
    }
    println!("effects drawn as nothing: {}", nothing.len());
    for n in &nothing {
        println!("  {n}");
    }
    assert!(nothing.len() <= UNDRAWN, "{} effects are drawn as nothing: {nothing:?}", nothing.len());
}

/// The effects that change nothing the player draws (a colour, a font, a 3D turn or a skew only): see the report of #89.
const UNDRAWN: usize = 28;

/// A picture of the show at set times after the click for the slides in `ANIM89_PAGES` (comma separated, 1-based; `all` for the
/// whole file), as `target/scenario/<test name>/<nn>-p<slide>-<ms>.png`.
fn frames() {
    // `ANIM89_FILE` names another file (a path under the repository, or an absolute one).
    let file = std::env::var("ANIM89_FILE").unwrap_or_else(|_| FILE.to_string());
    if file == FILE && !present() {
        return;
    }
    let pages = match std::env::var("ANIM89_PAGES") {
        Ok(p) => p,
        Err(_) if std::env::var("ANIM89_TIMES").is_ok() => String::new(),
        Err(_) => return,
    };
    let mut s = Scenario::open(&file);
    s.size(960., 540.);
    let count = s.with(|e, _| e.presentation().slides.len());
    let pages: Vec<usize> = if pages == "all" { (1..=count).collect() } else { pages.split(',').filter_map(|p| p.trim().parse().ok()).collect() };
    let times: Vec<f32> = std::env::var("ANIM89_MS").map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect()).unwrap_or_else(|_| vec![0.0, 150.0, 300.0, 450.0, 600.0, 900.0, 3000.0]);
    // `ANIM89_TIMES` names a file of lines `<page> <ms>,<ms>…`: the pages to play and the times of each (instead of `ANIM89_PAGES` and
    // `ANIM89_MS`).
    let per_page: Option<Vec<(usize, Vec<f32>)>> = std::env::var("ANIM89_TIMES").ok().map(|f| {
        std::fs::read_to_string(f)
            .unwrap()
            .lines()
            .filter_map(|l| {
                let (p, t) = l.split_once(' ')?;
                Some((p.parse().ok()?, t.split(',').filter_map(|t| t.trim().parse().ok()).collect()))
            })
            .collect()
    });
    let plan: Vec<(usize, Vec<f32>)> = per_page.unwrap_or_else(|| pages.into_iter().map(|p| (p, times.clone())).collect());
    for (page, times) in plan {
        // A page of 1000 and more is the slide of that number less 1000 (counted from 1) played from the start of the show as it is (its
        // effects begin by themselves); any other page is the one before the slide shown, which the space key then moves on to.
        let from_start = page >= 1000;
        s.page(if from_start { page - 1000 } else { page });
        s.update(|e, w, cx| e.start_slideshow(w, cx));
        s.settle();
        if !from_start {
            s.key("space");
        }
        let mut shots = Vec::new();
        for &ms in &times {
            s.update(|e, _, cx| {
                if let Some(show) = e.slideshow_mut() {
                    show.anim_at = Some(ms);
                }
                cx.notify();
            });
            s.settle();
            shots.push(s.screenshot(&format!("p{page}-{ms:.0}")));
        }
        // One row of the frames, a third of their size: the effect at a glance.
        let tiles: Vec<image::RgbaImage> = shots.iter().map(|p| image::imageops::resize(&image::open(p).unwrap().to_rgba8(), 320, 180, image::imageops::FilterType::Triangle)).collect();
        let mut sheet = image::RgbaImage::from_pixel(tiles.len() as u32 * 324, 184, image::Rgba([90, 90, 90, 255]));
        for (i, t) in tiles.iter().enumerate() {
            image::imageops::replace(&mut sheet, t, i as i64 * 324 + 2, 2);
        }
        sheet.save(s.screenshot_path(&format!("sheet-p{page}"))).unwrap();
        s.update(|e, w, cx| e.end_slideshow(w, cx));
    }
}

/// An effect of PowerPoint's gallery that the menu had no item for (開始 and 強調 each) is put on a shape from その他の効果, has its
/// behaviours, and is saved and read back as it was.
fn gallery_in_menu() {
    for (class, key_class) in [(AnimationClass::Entrance, "entr"), (AnimationClass::Emphasis, "emph")] {
        let effect = ibis_pptx::animation_catalog::effects_of(class)
            .filter(|e| !(class == AnimationClass::Entrance && ibis_doc_model::animation_preset::preset_def(e.id).is_some()))
            .find(|e| ibis_pptx::animation_catalog::make(ibis_doc_model::ShapeId(0), class, e.id, e.options[0].0).is_some_and(|a| a.look == AnimationLook::Preset))
            .expect("a gallery effect");
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        s.select_shape(0);
        s.toolbar_menu("motion", &format!("pptx.anim.cat.{key_class}.{}", effect.id));
        let a = s.with(|e, _| e.slide().animations.first().cloned()).expect("an effect");
        assert_eq!((a.class, a.look, a.preset.0), (class, AnimationLook::Preset, effect.id));
        assert!(!a.behaviors.is_empty());
        s.save_as(&format!("test-issue89-gallery-{key_class}.pptx"));
        let back = s.reopen().slides[0].animations[0].clone();
        assert_eq!((back.look, back.preset, &back.behaviors), (AnimationLook::Preset, a.preset, &a.behaviors));
    }
}

/// `ANIM89_LOOKS=<out.tsv>` (with `ANIM89_FILE`): one line for each effect of the file, `slide class preset subtype look behaviours`,
/// to see which effects the player draws as a fade or by hand (not by their behaviours).
fn looks() {
    let Ok(out) = std::env::var("ANIM89_LOOKS") else { return };
    let file = std::env::var("ANIM89_FILE").unwrap_or_else(|_| FILE.to_string());
    let data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(&file)).unwrap();
    let p = ibis_pptx::read(&data).unwrap();
    let mut lines = Vec::new();
    for (i, slide) in p.slides.iter().enumerate() {
        for a in &slide.animations {
            lines.push(format!("{}\t{:?}\t{}\t{}\t{:?}\t{}", i + 1, a.class, a.preset.0, a.preset.1, a.look, a.behaviors.len()));
        }
    }
    std::fs::write(out, lines.join("\n")).unwrap();
}
