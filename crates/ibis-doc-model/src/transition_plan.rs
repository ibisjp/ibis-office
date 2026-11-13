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
    /// The old slide goes white, then the new one comes out of it (フラッシュ).
    FadeWhite,
    /// The slides are drawn at a size about the centre: each as `(scale, opacity)`, the new one over the old, or under it.
    /// A flat stand-in for the effects that move the camera (ズーム, フライスルー, ワープ).
    Scaled { old: Option<(f64, f64)>, new: Option<(f64, f64)>, new_on_top: bool },
    /// The slide turns over about its vertical middle: the old one narrows to a strip of this share of its width (0 when gone),
    /// then the new one widens (反転; a flat stand-in for the turn).
    Flip { old: f64, new: f64 },
    /// An effect drawn in 3D: the slides are cards placed in perspective (the viewer renders them from textures); `motion` is the
    /// way the content moves, as for a push. Where the viewer cannot, `flat_plan` is its flat stand-in.
    Turn3d { kind: Turn3dKind, motion: (f64, f64) },
}

/// The 3D effects the viewer draws as cards in perspective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn3dKind {
    /// 立方体: the slides are two faces of a cube.
    Cube,
    /// 反転: the slide turns over about its middle.
    Flip,
    /// 箱: the cube seen from inside.
    Box,
    /// 回転: the cube with only the slides' shapes turning over their own background.
    Rotate,
    /// 軌道: the box with only the slides' shapes turning over their own background.
    Orbit,
    /// コンベヤー: the cards of the slides ride a belt (the shapes alone, over the slide's background; linear time).
    Conveyor,
    /// ギャラリー: the two slides in a strip that backs off, tilts, scrolls and comes back (black background; linear time).
    Gallery,
    /// スイッチ: two cards on a rail that turn toward each other and pass.
    Switch,
    /// 観覧車: the old card sinks toward the viewer and out, the new one comes down from far at the top left.
    Ferris,
    /// 倒れる: the old slide falls forward over the new one.
    FallOver,
    /// ページめくり（単一）: the old slide leaves bent, its leading edge lifted toward the viewer.
    PageCurlSingle,
    /// ピールオフ: the old slide is peeled off by a roll along a slanted line.
    PeelOff,
    /// カーテン: the old slide's halves gather at the sides, pleated, and the new slide is behind.
    Curtains,
    /// 風: the old slide is blown off as a card.
    Wind,
    /// ページめくり（両側）: the old slide's half on the far side turns over like a leaf of a book, bent, and lands on the other half showing the new slide.
    PageCurlDouble,
    /// ドレープ: the new slide falls onto the old one as a card.
    Drape,
    /// 紙飛行機: the old slide is folded into a paper plane that flies off.
    Airplane,
    /// 折り紙: the old slide folds into a paper crane that flies off (drawn as one card sent back and tilted).
    Origami,
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
        // 立方体 (`cube`) and 反転 (`flip`): fitted to the angle of the model of `turn3d.rs` over each frame of the video (the
        // transition begins about 11 frames in, `cubic-bezier(x1, 0, x2, 1)` of its 60; rms error 0.002 and 0.009 of the angle).
        "cube" | "rotate" => Some(bezier_ease(0.326, 0.69, t)),
        "box" | "orbit" => Some(bezier_ease(0.346, 0.65, t)),
        // コンベヤー runs at a constant speed in the table of `turn3d.rs`.
        "conveyor" | "gallery" | "switch" | "ferris" | "fallOver" | "pageCurlSingle" | "peelOff" | "curtains" | "wind" | "pageCurlDouble" | "drape" | "airplane" | "origami" => Some(t),
        "flip" => Some(bezier_ease(0.593, 0.358, t)),
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

/// The effects PowerPoint draws in 3D (the slide as a card turned, folded or swung in perspective) are drawn as the flat motion
/// that is closest to them: the plan of `effect` when it is one of these, `None` otherwise. The picture is not PowerPoint's
/// (the slides are not turned in perspective: GPUI elements cannot be), the direction and the way the slides leave are.
fn flat_plan(effect: &str, token: &str, q: f64, aspect: f64) -> Option<TransitionPlan> {
    let slide = |m: (f64, f64), pushed: bool| TransitionPlan::Slide { from: (-m.0, -m.1), old_to: if pushed { m } else { (0.0, 0.0) }, new_on_top: true };
    let windows = |filter: &str| match reveal_rects(filter, true, q, aspect) {
        Some(rects) => TransitionPlan::Windows(rects),
        None => TransitionPlan::Fade { through_black: false },
    };
    // The files' `invX` (right) is the option "From Right" of the 2013 effects, the default "From Left": the slide leaves toward the other side.
    let inv = token == "invX";
    Some(match effect {
        // ズーム: out, the old slide shrinks into the middle over the new one; in, the new one grows out of the middle.
        "zoom" if token == "in" => TransitionPlan::Scaled { old: None, new: Some((q, q)), new_on_top: true },
        "zoom" => TransitionPlan::Scaled { old: Some((1.0 - q, 1.0 - q)), new: None, new_on_top: false },
        // フライスルー and ワープ move the camera through the slide: in, toward the new one; out, away from the old one.
        "flythrough" | "warp" if (effect == "warp" && token == "in") || (effect == "flythrough" && (token.is_empty() || token.starts_with("in"))) => TransitionPlan::Scaled { old: Some((1.0 + q, 1.0 - q)), new: Some((q, q)), new_on_top: true },
        "flythrough" | "warp" => TransitionPlan::Scaled { old: Some((1.0 - q, 1.0 - q)), new: Some((1.0 + (1.0 - q), q)), new_on_top: false },
        "newsflash" => TransitionPlan::Scaled { old: Some((1.0 - q, 1.0 - q)), new: Some((q, q)), new_on_top: true },
        "flash" => TransitionPlan::FadeWhite,
        "flip" => TransitionPlan::Flip { old: (1.0 - 2.0 * q).max(0.0), new: (2.0 * q - 1.0).max(0.0) },
        // The cards of a cube, a box, a rotating slide, an orbit and a switch move as the push does.
        "cube" | "box" | "rotate" | "orbit" | "switch" | "ferris" => slide(motion(token), true),
        // ripple: rings from the middle (the corners are drawn the same).
        "ripple" => windows("circle(out)"),
        "vortex" => windows("dissolve"),
        // 2013's effects: the old slide leaves, curtains opening, strips falling, a wind of strips, the page lifted off.
        "curtains" => windows("barn(outVertical)"),
        "drape" => windows(if inv { "randombar(vertical)" } else { "blinds(vertical)" }),
        "wind" | "fallOver" => TransitionPlan::SoftWindows(wipe_strips(if inv { 'l' } else { 'r' }, q)),
        "peelOff" | "pageCurlDouble" | "pageCurlSingle" | "airplane" | "origami" => TransitionPlan::Slide { from: (0.0, 0.0), old_to: if inv { (1.0, 0.0) } else { (-1.0, 0.0) }, new_on_top: false },
        "prestige" | "fracture" | "crush" => windows("dissolve"),
        _ => return None,
    })
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
        "conveyor" => TransitionPlan::Turn3d { kind: Turn3dKind::Conveyor, motion: motion(token) },
        "gallery" => TransitionPlan::Turn3d { kind: Turn3dKind::Gallery, motion: motion(token) },
        "switch" => TransitionPlan::Turn3d { kind: Turn3dKind::Switch, motion: motion(token) },
        "ferris" => TransitionPlan::Turn3d { kind: Turn3dKind::Ferris, motion: motion(token) },
        // The 2013 effects' option is the file's `invX` (from the right), the default from the left.
        "fallOver" => TransitionPlan::Turn3d { kind: Turn3dKind::FallOver, motion: if token == "invX" { (1.0, 0.0) } else { (-1.0, 0.0) } },
        "peelOff" => TransitionPlan::Turn3d { kind: Turn3dKind::PeelOff, motion: if token == "invX" { (1.0, 0.0) } else { (-1.0, 0.0) } },
        "pageCurlSingle" => TransitionPlan::Turn3d { kind: Turn3dKind::PageCurlSingle, motion: if token == "invX" { (1.0, 0.0) } else { (-1.0, 0.0) } },
        "pageCurlDouble" => TransitionPlan::Turn3d { kind: Turn3dKind::PageCurlDouble, motion: if token == "invX" { (1.0, 0.0) } else { (-1.0, 0.0) } },
        "drape" => TransitionPlan::Turn3d { kind: Turn3dKind::Drape, motion: if token == "invX" { (1.0, 0.0) } else { (-1.0, 0.0) } },
        "airplane" => TransitionPlan::Turn3d { kind: Turn3dKind::Airplane, motion: if token == "invX" { (1.0, 0.0) } else { (-1.0, 0.0) } },
        "origami" => TransitionPlan::Turn3d { kind: Turn3dKind::Origami, motion: if token == "invX" { (1.0, 0.0) } else { (-1.0, 0.0) } },
        "curtains" => TransitionPlan::Turn3d { kind: Turn3dKind::Curtains, motion: (-1.0, 0.0) },
        "wind" => TransitionPlan::Turn3d { kind: Turn3dKind::Wind, motion: if token == "invX" { (1.0, 0.0) } else { (-1.0, 0.0) } },
        "push" | "pan" => {
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
        // The sectors are cut from one grid of cells whatever the number of spokes, so four and eight cost as much as one.
        "wheel" | "wheelReverse" => windows(&format!("wheel({})", token.parse::<usize>().ok().filter(|n| (1..=16).contains(n)).unwrap_or(4))),
        // Dissolve, and the tile effects whose tiles (hexagons, diamonds, strips) are drawn in 3D or as shapes the rectangles do not
        // make: the new slide appears in squares in turn (an approximation of PowerPoint's, which shows the tiles' own shapes).
        "dissolve" | "honeycomb" | "glitter" | "shred" => windows("dissolve"),
        "wedge" => windows("wedge"),
        "diamond" => windows("diamond(out)"),
        "plus" => windows("plus(out)"),
        "strips" => windows(match token {
            "ld" => "strips(downLeft)",
            "rd" => "strips(downRight)",
            "ru" => "strips(upRight)",
            _ => "strips(upLeft)",
        }),
        "cube" => TransitionPlan::Turn3d { kind: Turn3dKind::Cube, motion: motion(token) },
        "flip" => TransitionPlan::Turn3d { kind: Turn3dKind::Flip, motion: motion(token) },
        "box" => TransitionPlan::Turn3d { kind: Turn3dKind::Box, motion: motion(token) },
        "rotate" => TransitionPlan::Turn3d { kind: Turn3dKind::Rotate, motion: motion(token) },
        "orbit" => TransitionPlan::Turn3d { kind: Turn3dKind::Orbit, motion: motion(token) },
        other => flat_plan(other, token, q, aspect).unwrap_or(TransitionPlan::Fade { through_black: false }),
    }
}

/// The flat stand-in of a 3D effect, for a viewer that cannot draw it in 3D.
pub fn flat_stand_in(effect: &str, token: Option<&str>, q: f64, aspect: f64) -> Option<TransitionPlan> {
    flat_plan(effect, token.unwrap_or(""), q, aspect)
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
        // The 3D effects are drawn as the flat motion nearest to them, none as a fade.
        for e in ["zoom", "flash", "flip", "cube", "box", "rotate", "orbit", "switch", "ripple", "vortex", "flythrough", "warp", "newsflash", "curtains", "drape", "wind", "fallOver", "peelOff", "pageCurlDouble", "pageCurlSingle", "airplane", "origami", "prestige", "fracture", "crush"] {
            assert!(!matches!(plan(e, None, 0.5, 1.0), TransitionPlan::Fade { .. }), "{e}");
        }
        // 立方体 from the right (`dir="l"`): the new slide comes in from the right and pushes the old one out to the left.
        assert_eq!(plan("cube", Some("l"), 0.5, 1.0), TransitionPlan::Turn3d { kind: Turn3dKind::Cube, motion: (-1.0, 0.0) });
        assert_eq!(flat_stand_in("cube", Some("l"), 0.5, 1.0), Some(TransitionPlan::Slide { from: (1.0, 0.0), old_to: (-1.0, 0.0), new_on_top: true }));
        // Wheels of 3, 4 (the default) and 8 spokes, and dissolve, are windows, not a fade.
        for (e, t) in [("wheel", Some("3")), ("wheel", None), ("wheel", Some("8")), ("dissolve", None), ("honeycomb", None)] {
            assert!(matches!(plan(e, t, 0.5, 1.0), TransitionPlan::Windows(_)), "{e} {t:?}");
        }
        assert!(matches!(plan("fade", Some("thruBlk"), 0.5, 1.0), TransitionPlan::Fade { through_black: true }));
    }

    #[test]
    fn the_fitted_3d_effects_are_cards_and_sheets_with_a_flat_stand_in() {
        for (e, kind) in [("switch", Turn3dKind::Switch), ("ferris", Turn3dKind::Ferris), ("fallOver", Turn3dKind::FallOver), ("pageCurlSingle", Turn3dKind::PageCurlSingle), ("peelOff", Turn3dKind::PeelOff), ("curtains", Turn3dKind::Curtains), ("wind", Turn3dKind::Wind), ("pageCurlDouble", Turn3dKind::PageCurlDouble), ("drape", Turn3dKind::Drape), ("airplane", Turn3dKind::Airplane), ("origami", Turn3dKind::Origami)] {
            assert!(matches!(plan(e, Some("r"), 0.5, 1.0), TransitionPlan::Turn3d { kind: k, .. } if k == kind), "{e}");
            assert!(flat_stand_in(e, Some("r"), 0.5, 1.0).is_some(), "{e}");
            // Linear in time (the tables are in video frames).
            assert_eq!(progress(e, 0.3), Some(0.3), "{e}");
        }
        // The file's `invX` is the option that moves the content to the right.
        assert!(matches!(plan("peelOff", Some("invX"), 0.5, 1.0), TransitionPlan::Turn3d { motion: (1.0, 0.0), .. }));
        assert!(matches!(plan("peelOff", None, 0.5, 1.0), TransitionPlan::Turn3d { motion: (-1.0, 0.0), .. }));
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
