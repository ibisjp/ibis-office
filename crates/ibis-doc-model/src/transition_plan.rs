// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 画面切り替えの効果を、スライドショーで描くための「計画」に直す（画面のライブラリに依存しない）。
//! 新しいスライドが、古いスライドの上でどう現れるかを、進み具合 `q`（0 から 1）で表す。長方形の窓（ワイプ・スプリット・
//! ブラインド・チェッカーボード・ランダムストライプ・図形…）は `animation_preset::reveal_rects`（`p:animEffect` の
//! `filter` と同じ形）を使う。長方形や平行移動で表せない効果（3D のもの、ホイール、ディゾルブなど）はフェードで描く。

use crate::animation_preset::{Rect, reveal_rects, scatter};

#[derive(Clone, Debug, PartialEq)]
pub enum TransitionPlan {
    /// The new slide fades in over the old (through black: the old goes black, then the new comes out of it).
    Fade { through_black: bool },
    /// The slides move (fractions of the slide's width and height): the new one from `from` to its place, the old from its place
    /// to `old_to`; the new one over the old, or under it (the old moves away).
    Slide { from: (f64, f64), old_to: (f64, f64), new_on_top: bool },
    /// The new slide is seen through these rectangles of the slide (fractions).
    Windows(Vec<Rect>),
    /// The new slide is seen through these rectangles at the given opacities (a soft edge: ワイプ).
    SoftWindows(Vec<(Rect, f64)>),
}

/// The easing of transition `effect` at `t` (0 to 1 of its time), where PowerPoint's differs from the common ease-in-out
/// (`None`: the caller's own). Measured on PowerPoint for Windows (Microsoft 365 16.0.20527; the slideshow exported to video
/// with `Presentation.CreateVideo`, 30 frames a second, 0.5, 1 and 2 second transitions; the same curve at every duration):
/// アンカバー (`pull`) and カバー are `cubic-bezier(0.72, 0, 0.36, 1)` (rms error 0.003 of the slide's width), プッシュ is
/// `cubic-bezier(0.44, 0, 0.56, 1)` (0.003), and ワイプ runs at a constant speed.
pub fn progress(effect: &str, t: f64) -> Option<f64> {
    let t = t.clamp(0.0, 1.0);
    match effect {
        "pull" | "reveal" | "cover" => Some(bezier_ease(0.72, 0.36, t)),
        "push" => Some(bezier_ease(0.44, 0.56, t)),
        "wipe" => Some(t),
        _ => None,
    }
}

/// `cubic-bezier(x1, 0, x2, 1)` at time `u` (0 to 1).
pub fn bezier_ease(x1: f64, x2: f64, u: f64) -> f64 {
    let x = |s: f64| 3.0 * (1.0 - s) * (1.0 - s) * s * x1 + 3.0 * (1.0 - s) * s * s * x2 + s * s * s;
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..48 {
        let mid = (lo + hi) / 2.0;
        if x(mid) < u {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let s = (lo + hi) / 2.0;
    3.0 * (1.0 - s) * s * s + s * s * s
}

/// How many pieces a wipe is drawn in (each at one opacity; a soft edge needs more than rectangles give).
const WIPE_STRIPS: usize = 48;
/// Checkerboard: the squares of the slide (measured on PowerPoint for Windows: 7 across and 5 down, on 16:9 and 4:3 slides, for
/// both options), and the share of the transition one square takes to turn over (about 5 frames of the 60 of two seconds).
const CHECKER_COLUMNS: usize = 7;
const CHECKER_ROWS: usize = 5;
const CHECKER_TURN: f64 = 5.0 / 60.0;
/// Blinds: the slats (measured the same on both slide shapes: 14 horizontal slats, 18 vertical ones).
const BLINDS_HORIZONTAL: usize = 14;
const BLINDS_VERTICAL: usize = 18;

/// ワイプ: the new slide's opacity at distance `s` (0 to 1, from the edge the wipe starts at) when the wipe has run `t` (0 to 1,
/// at a constant speed): a half cosine as wide as the slide, whose middle goes from a slide's half width before the start edge
/// to a half beyond the far one (measured on PowerPoint for Windows: width 1288 px on a 1280 px slide, 42 px a frame, 30 frames
/// a second, 2 seconds).
pub fn wipe_opacity(t: f64, s: f64) -> f64 {
    let u = (2.0 * t - s).clamp(0.0, 1.0);
    (1.0 - (std::f64::consts::PI * u).cos()) / 2.0
}

fn wipe_strips(from: char, t: f64) -> Vec<(Rect, f64)> {
    let mut out = Vec::new();
    for i in 0..WIPE_STRIPS {
        let (a, z) = (i as f64 / WIPE_STRIPS as f64, (i + 1) as f64 / WIPE_STRIPS as f64);
        let mid = (a + z) / 2.0;
        // `a`..`z` along the slide's width or height; `s` is the distance from the start edge.
        let (rect, s): (Rect, f64) = match from {
            'r' => ([a, 0.0, z, 1.0], 1.0 - mid),
            'l' => ([a, 0.0, z, 1.0], mid),
            'b' => ([0.0, a, 1.0, z], 1.0 - mid),
            _ => ([0.0, a, 1.0, z], mid),
        };
        let alpha = wipe_opacity(t, s);
        if alpha > 0.004 {
            out.push((rect, alpha));
        }
    }
    out
}

fn checker_windows(across: bool, q: f64) -> Vec<Rect> {
    let mut out = Vec::new();
    for row in 0..CHECKER_ROWS {
        for col in 0..CHECKER_COLUMNS {
            let i = if across { col * CHECKER_ROWS + row } else { row * CHECKER_COLUMNS + col };
            // Each square turns over at a time of its own; the new face is seen from the half of the turn on.
            let start = scatter(i) * (1.0 - CHECKER_TURN);
            if q >= start + CHECKER_TURN / 2.0 {
                out.push([col as f64 / CHECKER_COLUMNS as f64, row as f64 / CHECKER_ROWS as f64, (col + 1) as f64 / CHECKER_COLUMNS as f64, (row + 1) as f64 / CHECKER_ROWS as f64]);
            }
        }
    }
    out
}

fn blinds_windows(horizontal: bool, q: f64) -> Vec<Rect> {
    let n = if horizontal { BLINDS_HORIZONTAL } else { BLINDS_VERTICAL };
    (0..n)
        .filter_map(|i| {
            let (a, z) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let r: Rect = if horizontal { [0.0, a, 1.0, a + (z - a) * q] } else { [a, 0.0, a + (z - a) * q, 1.0] };
            (q > 0.0).then_some(r)
        })
        .collect()
}

/// Which way the content moves for option `token` (`dir`: `l` is leftward, `d` downward, `ld` left and down): PowerPoint's
/// `dir` is the direction of movement (a push to the left brings the new slide in from the right; measured on PowerPoint for
/// Mac, 2026-10-31, `target/issue88-89-90/ppt/cap`).
fn motion(token: &str) -> (f64, f64) {
    match token {
        "l" => (-1.0, 0.0),
        "r" => (1.0, 0.0),
        "u" => (0.0, -1.0),
        "d" => (0.0, 1.0),
        "ld" => (-1.0, 1.0),
        "lu" => (-1.0, -1.0),
        "rd" => (1.0, 1.0),
        "ru" => (1.0, -1.0),
        _ => (-1.0, 0.0),
    }
}

/// How effect `effect` with option `token` shows the new slide at `q` (0 to 1) of its time, on a slide of `aspect` (width over height).
pub fn plan(effect: &str, token: Option<&str>, q: f64, aspect: f64) -> TransitionPlan {
    let token = token.unwrap_or("");
    let windows = |filter: &str| match reveal_rects(filter, true, q, aspect) {
        Some(rects) => TransitionPlan::Windows(rects),
        None => TransitionPlan::Fade { through_black: false },
    };
    match effect {
        "fade" => TransitionPlan::Fade { through_black: token == "thruBlk" },
        "push" | "pan" | "conveyor" | "gallery" | "ferris" => {
            let m = motion(token);
            TransitionPlan::Slide { from: (-m.0, -m.1), old_to: m, new_on_top: true }
        }
        "cover" => {
            let m = motion(token);
            TransitionPlan::Slide { from: (-m.0, -m.1), old_to: (0.0, 0.0), new_on_top: true }
        }
        "pull" | "reveal" => TransitionPlan::Slide { from: (0.0, 0.0), old_to: motion(token), new_on_top: false },
        // ワイプ: the new slide is wiped in the way `dir` says, from the opposite edge (a wipe to the left starts at the right).
        "wipe" => TransitionPlan::SoftWindows(wipe_strips(
            match token {
                "r" => 'l',
                "u" => 'b',
                "d" => 't',
                _ => 'r',
            },
            q,
        )),
        "split" => windows(match token {
            "in" => "barn(inVertical)",
            "horzIn" => "barn(inHorizontal)",
            "horzOut" => "barn(outHorizontal)",
            _ => "barn(outVertical)",
        }),
        "doors" | "window" => windows(if token == "horz" { "barn(outHorizontal)" } else { "barn(outVertical)" }),
        "blinds" => TransitionPlan::Windows(blinds_windows(token != "vert", q)),
        "checker" => TransitionPlan::Windows(checker_windows(token != "vert", q)),
        "randomBar" => windows(if token == "vert" { "randombar(vertical)" } else { "randombar(horizontal)" }),
        "comb" => windows(if token == "vert" { "comb(vertical)" } else { "comb(horizontal)" }),
        "circle" => windows(if token == "in" { "circle(in)" } else { "circle(out)" }),
        "wheel" => match token.parse::<usize>() {
            Ok(n) if n <= 2 => windows(&format!("wheel({n})")),
            // Four or eight spokes need so many windows that the slide is drawn that many times: faded.
            _ => TransitionPlan::Fade { through_black: false },
        },
        "wedge" => windows("wedge"),
        "diamond" => windows("diamond(out)"),
        "plus" => windows("plus(out)"),
        "strips" => windows(match token {
            "ld" => "strips(downLeft)",
            "rd" => "strips(downRight)",
            "ru" => "strips(upRight)",
            _ => "strips(upLeft)",
        }),
        _ => TransitionPlan::Fade { through_black: false },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_moves_both_slides_and_cover_only_the_new_one() {
        // `dir` is where the content moves: a push to the left brings the new slide in from the right.
        assert_eq!(plan("push", Some("l"), 0.5, 1.0), TransitionPlan::Slide { from: (1.0, 0.0), old_to: (-1.0, 0.0), new_on_top: true });
        assert_eq!(plan("cover", Some("d"), 0.5, 1.0), TransitionPlan::Slide { from: (0.0, -1.0), old_to: (0.0, 0.0), new_on_top: true });
        // Uncover: the new slide stays; the old goes.
        assert!(matches!(plan("pull", Some("r"), 0.5, 1.0), TransitionPlan::Slide { from: (0.0, 0.0), new_on_top: false, .. }));
    }

    #[test]
    fn wipes_and_blinds_are_windows_that_grow() {
        // A wipe to the left starts at the right edge.
        let TransitionPlan::SoftWindows(w) = plan("wipe", Some("l"), 0.5, 1.0) else { panic!() };
        // At half time the edge's middle is at the middle of the slide: the right is new, the left old, the middle half way.
        assert!(w.first().is_none_or(|(_, a)| *a < 0.1), "the left stays old");
        assert!(w.last().unwrap().1 > 0.99, "the right is new");
        let TransitionPlan::Windows(w) = plan("blinds", Some("horz"), 1.0, 1.0) else { panic!() };
        assert_eq!(w.len(), 14);
        let TransitionPlan::Windows(w) = plan("blinds", Some("vert"), 1.0, 1.0) else { panic!() };
        assert_eq!(w.len(), 18);
        let TransitionPlan::Windows(w) = plan("checker", None, 1.0, 1.0) else { panic!() };
        assert_eq!(w.len(), 35);
        assert!(matches!(plan("vortex", None, 0.5, 1.0), TransitionPlan::Fade { through_black: false }));
        assert!(matches!(plan("fade", Some("thruBlk"), 0.5, 1.0), TransitionPlan::Fade { through_black: true }));
    }

    #[test]
    fn easing_matches_the_measured_curves() {
        assert!((bezier_ease(0.72, 0.36, 0.0)).abs() < 1e-9 && (bezier_ease(0.72, 0.36, 1.0) - 1.0).abs() < 1e-9);
        // Uncover at half time is a little under half way (measured 0.41 on PowerPoint, 1 second).
        let half = progress("pull", 0.5).unwrap();
        assert!((0.38..0.45).contains(&half), "{half}");
        assert!((progress("push", 0.5).unwrap() - 0.5).abs() < 0.02);
        assert_eq!(progress("wipe", 0.3), Some(0.3));
        assert_eq!(progress("fade", 0.3), None);
    }

    #[test]
    fn a_wipe_edge_is_a_cosine_as_wide_as_the_slide() {
        assert_eq!(wipe_opacity(0.0, 0.0), 0.0);
        assert_eq!(wipe_opacity(1.0, 1.0), 1.0);
        // The middle of the edge is the start edge's distance 2t - 0.5 away.
        assert!((wipe_opacity(0.5, 0.5) - 0.5).abs() < 1e-9);
        assert!((wipe_opacity(0.25, 0.0) - 0.5).abs() < 1e-9 && (wipe_opacity(0.75, 1.0) - 0.5).abs() < 1e-9);
    }
}
