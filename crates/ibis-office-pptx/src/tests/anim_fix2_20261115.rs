// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-15 (anim-fix-2): screen transitions fitted to PowerPoint's video (`scripts/transition-fit/audit_compare.py`); frames are read as pixels.
use crate::shell::scenario::Scenario;
use crate::slideshow::SlideshowExt;
use ibis_doc_model::{Color, Command, Fill, Rgb, Transition};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::transition_flash_goes_white_then_the_new_slide_comes_out_of_it_left_part_first", flash_phases),
        ("pptx::transition_reveal_fades_through_grey_or_black_with_a_phase_across_the_slide", reveal_phases),
    ]
}

/// A red slide changing to a blue one with `transition`, the frame `ms` into it, as (width, height, pixels).
fn frame(transition: Transition, ms: f32) -> (u32, u32, Vec<u8>) {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(960., 540.);
    let (first, second) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[1].id));
    s.update(|e, _, cx| {
        e.run(
            vec![
                Command::SetBackground { slide: first, fill: Fill::Solid(Color::opaque(Rgb::from_rgb(255, 0, 0))) },
                Command::SetBackground { slide: second, fill: Fill::Solid(Color::opaque(Rgb::from_rgb(0, 0, 255))) },
                Command::SetTransition { slide: second, transition: Some(transition) },
            ],
            cx,
        )
    });
    s.page(1);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    s.key("space");
    s.update(|e, _, cx| {
        if let Some(show) = e.slideshow_mut() {
            show.transition_at = Some(ms);
        }
        cx.notify();
    });
    s.settle();
    let img = image::open(s.screenshot("frame")).unwrap().to_rgba8();
    (img.width(), img.height(), img.into_raw())
}

fn at(f: &(u32, u32, Vec<u8>), fx: f32, fy: f32) -> [u8; 3] {
    let (x, y) = ((f.0 as f32 * fx) as usize, (f.1 as f32 * fy) as usize);
    let i = (y * f.0 as usize + x) * 4;
    [f.2[i], f.2[i + 1], f.2[i + 2]]
}

/// リビール (PowerPoint, 2 s: the old slide fades to grey (a backdrop going from black to white) or to black, and the new one comes out of
/// it, at a moment that depends on where in the slide the column is; measured 0.057 to 0.0008 of the squared error, `transition_plan::reveal_mix`).
/// At half time every column is the backdrop (mid grey; black through black); a quarter in, the left and the right of the slide are at
/// different stages of the fade.
fn reveal_phases() {
    let reveal = |d: &str| Transition { duration: 2000, direction: Some(d.to_string()), ..Transition::new("reveal") };
    let mid = frame(reveal("l"), 1000.0);
    for (fx, fy) in [(0.1, 0.3), (0.5, 0.6), (0.9, 0.3)] {
        let p = at(&mid, fx, fy);
        assert!(p.iter().all(|&c| (115..=140).contains(&c)), "reveal/l at half time is not mid grey at ({fx}, {fy}): {p:?}");
    }
    let black = frame(reveal("blk-l"), 1000.0);
    for (fx, fy) in [(0.1, 0.3), (0.9, 0.3)] {
        let p = at(&black, fx, fy);
        assert!(p.iter().all(|&c| c < 12), "reveal/blk-l at half time is not black at ({fx}, {fy}): {p:?}");
    }
    // A quarter in (0.5 s), through black to the left: the left is darker than the right (the old slide leaves the left first).
    let q = frame(reveal("blk-l"), 500.0);
    let (left, right) = (at(&q, 0.1, 0.3)[0], at(&q, 0.9, 0.3)[0]);
    assert!(right > left + 25, "reveal/blk-l a quarter in: the right ({right}) is not brighter than the left ({left})");
    // Through grey to the left, the old slide leaves the right first: the opposite.
    let g = frame(reveal("l"), 500.0);
    let (left, right) = (at(&g, 0.1, 0.3), at(&g, 0.9, 0.3));
    assert!(left[0] > right[0] + 25, "reveal/l a quarter in: {left:?} {right:?}");
}

/// フラッシュ (PowerPoint, 2 s; 0.0724 to 0.0082 of the squared error, `transition_plan::flash_mix`): the slide is white by 0.3 s (frame 9),
/// and the new slide comes out of the white first in the left half (46 percent at 1 s), then the middle part (about 18), then the right fifth (about 7).
fn flash_phases() {
    let flash = || Transition { duration: 2000, ..Transition::new("flash") };
    let white = frame(flash(), 300.0);
    for (fx, fy) in [(0.1, 0.3), (0.6, 0.6), (0.9, 0.3)] {
        let p = at(&white, fx, fy);
        assert!(p.iter().all(|&c| c > 245), "flash at 0.3 s is not white at ({fx}, {fy}): {p:?}");
    }
    let mid = frame(flash(), 1000.0);
    // Over white, a blue slide of weight w is (255 (1 - w), 255 (1 - w), 255).
    let share = |fx: f32| 1.0 - f32::from(at(&mid, fx, 0.6)[0]) / 255.0;
    let (a, b, c) = (share(0.2), share(0.65), share(0.9));
    assert!((a - 0.46).abs() < 0.06 && (b - 0.17).abs() < 0.06 && c < 0.12, "flash at 1 s: the new slide's share is {a:.2} {b:.2} {c:.2}");
}
