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
    /// カット through black: the old slide stays for the first half of the transition's length, is cut away to a black screen for the
    /// second half, and then the new slide is cut in (PowerPoint for Windows, 2 seconds filmed at 30 frames a second: the old slide to
    /// 1.0 s, black for the frames 183 to 212 of the video = 1.0 s, no fade at either end; a different length has not been filmed, so
    /// the half is a share of it, which the earlier "black for the whole length" at 1 second also fits).
    BlackScreen,
    /// The slides move (fractions of the slide's width and height): the new one from `from` to its place, the old from its place
    /// to `old_to`; the new one over the old, or under it (the old moves away).
    Slide { from: (f64, f64), old_to: (f64, f64), new_on_top: bool },
    /// The new slide is seen through these rectangles of the slide (fractions).
    Windows(Vec<Rect>),
    /// The new slide is seen through these rectangles at the given opacities (a soft edge: ワイプ).
    SoftWindows(Vec<(Rect, f64)>),
    /// The slides are mixed with a flat backdrop (grey level `backdrop`, 0 black to 1 white) in vertical strips, each strip with its own
    /// weights of the old and the new slide (リビール: the fade through grey or black runs at a different time in each strip).
    /// `scale` is the size of the old and of the new slide about the centre (1 for a slide that keeps its size).
    StripMix { strips: Vec<StripMix>, backdrop: f64, scale: (f64, f64) },
    /// The slides are drawn at a size about the centre: each as `(scale, opacity)`, the new one over the old, or under it.
    /// A flat stand-in for the effects that move the camera (ズーム, フライスルー, ワープ).
    Scaled { old: Option<(f64, f64)>, new: Option<(f64, f64)>, new_on_top: bool },
    /// ニュース速報: the new slide, whole, grows out of the middle of the old one (which stays as it is) at `scale` of its size about the
    /// centre, turned `turn` degrees clockwise about the centre, at this `opacity`.
    Spin { scale: f64, turn: f64, opacity: f64 },
    /// ワープ: the camera moves through the two slides, which stand one behind the other: each slide is drawn whole at `scale` about the
    /// centre (more than 1: the middle of it fills the frame) at its `opacity`; the one drawn later is over the earlier one. With
    /// `additive` the new slide's rectangle shows `old * (1 - k) + new * k` and the rest of the frame `old * (1 - k)` over black (the
    /// new slide is added to the dimmed old one, not laid over it); without, the old one is laid over the new at its opacity. The frame
    /// behind them is grey at the level `backdrop` (0 black to 1 white; フライスルー shows a backdrop that fades from white to black).
    Dolly { old: (f64, f64), new: (f64, f64), new_on_top: bool, additive: bool, backdrop: f64 },
    /// ズーム: the old slide (`old`) or the new one (`new`) at this scale about the centre, whole (opacity 1) with a soft edge
    /// (`soft_scaled_pieces`), the other slide under it; the new one over the old, or under it.
    SoftScaled { old: Option<f64>, new: Option<f64>, new_on_top: bool },
    /// The slide turns over about its vertical middle: the old one narrows to a strip of this share of its width (0 when gone),
    /// then the new one widens (反転; a flat stand-in for the turn).
    Flip { old: f64, new: f64 },
    /// An effect drawn in 3D: the slides are cards placed in perspective (the viewer renders them from textures); `motion` is the
    /// way the content moves, as for a push. Where the viewer cannot, `flat_plan` is its flat stand-in.
    Turn3d { kind: Turn3dKind, motion: (f64, f64) },
}

/// One vertical strip of `TransitionPlan::StripMix`: its rectangle (fractions of the slide) and the weights of the old and the new slide
/// (the rest is the backdrop).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripMix {
    pub rect: Rect,
    pub old: f64,
    pub new: f64,
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
    /// クラッシュ: the old slide is a sheet (a mesh with the slide as its texture) crumpled into a ball from the four corners, which then falls out at the bottom; the new slide is behind.
    Crush,
    /// プレステージ: the old slide is a cloth (a mesh of nodes joined by springs, with friction) held and pulled up near the middle of its top edge; the new slide is behind (see `turn3d_cloth`).
    Prestige,
    /// フラクチャ: the old slide is cracked into wedges from a point near its middle, which open and fall away; the new slide is behind (see `turn3d_fracture`).
    Fracture,
    /// さざなみ: the slide is a surface with concentric waves in Z (shaded) spreading from the centre or a corner; the new slide shows through them.
    Ripple,
    /// くし形: bands of the two slides pushing each other, alternately in opposite ways (the motion is (1, 0) for the horizontal bands, (0, 1) for the vertical ones).
    Comb,
    /// ブラインド: slats turning over about their own middle lines (the motion is (1, 0) for the horizontal slats, (0, 1) for the vertical ones).
    Blinds,
    /// チェッカー: squares turning over, each in its own time (the motion is (1, 0) for the across option, (0, 1) for down).
    Checker,
    /// ハニカム: the slide is a plane of hexagons that each fade out in its own time while the plane flies past the camera, and the new slide's hexagons
    /// each come back in their own time on a plane that settles from far and turned (see `turn3d_tiles::honeycomb`).
    Honeycomb,
    /// 細断: the slide is cut into strips (`particle` false: 24 of unequal widths, paired by a horizontal cut) or small squares (`particle`), which sink away
    /// each at its own speed while the whole turns, and the new slide's pieces come back from the right (see `turn3d_tiles::shred`).
    Shred { particle: bool },
    /// 渦巻き: the tiles of the slides thrown toward the viewer behind a sweeping front, the new slide's coming back the same way run backwards (see `turn3d_tiles::vortex`).
    Vortex,
    /// きらめき: the slide is a lattice of hexagons (`diamond` false) or diamonds; the old ones turn about their vertical axes and fall, black shows, and the new slide's
    /// come down from far above turning and pile up in their places (the motion is the side it starts at, as for the vortex; see `turn3d_tiles::glitter`).
    Glitter { diamond: bool },
    /// 扉 and 窓: the old slide's halves swing back and the new slide grows from the middle (the motion is (1, 0) for the halves above and below, (0, 1) for those at the sides; `window` on grey).
    Doors { window: bool },
}

/// The easing of transition `effect` at `t` (0 to 1 of its time), where PowerPoint's differs from the common ease-in-out
/// (`None`: the caller's own). Measured on PowerPoint for Windows (Microsoft 365 16.0.20527; the slideshow exported to video
/// with `Presentation.CreateVideo`, 30 frames a second, 0.5, 1 and 2 second transitions; the same curve at every duration):
/// アンカバー (`pull`) and カバー are `cubic-bezier(0.72, 0, 0.36, 1)` (rms error 0.003 of the slide's width), プッシュ is
/// `cubic-bezier(0.44, 0, 0.56, 1)` (0.003), and ワイプ runs at a constant speed.
pub fn progress(effect: &str, t: f64) -> Option<f64> {
    progress_with(effect, None, t)
}

/// `cubic-bezier(x1, y1, x2, 1)` of the frames `s` to `s + len` (of the 60 of a 2 second transition at 30 frames a second) at `t`.
fn bezier_frames(x1: f64, y1: f64, x2: f64, s: f64, len: f64, t: f64) -> f64 {
    let u = ((t * 60.0 - s) / len).clamp(0.0, 1.0);
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
    let m = (lo + hi) / 2.0;
    3.0 * (1.0 - m) * (1.0 - m) * m * y1 + 3.0 * (1.0 - m) * m * m + m * m * m
}

/// `cubic-bezier(x1, y1, x2, y2)` of the frames `s` to `s + len` (of the 60 of a 2 second transition at 30 frames a second) at `t`.
fn bezier_frames_xy(x1: f64, y1: f64, x2: f64, y2: f64, s: f64, len: f64, t: f64) -> f64 {
    bezier_xy(x1, y1, x2, y2, (t * 60.0 - s) / len)
}

/// フライスルー's progress `kappa` (0 the old slide whole, 1 the new slide whole) at `t` (0 to 1 of its time). Measured on PowerPoint
/// for Windows (`CreateVideo`, 30 frames a second, 2 seconds, 720p; slides 91 to 94 of `audit-trans-flat-16x9-dur2000`): `kappa` was fitted
/// to the picture at each frame (`flythrough_look`), then one curve to the frames of in and out together (a frame of offset each): in,
/// `cubic-bezier(0.456, 0, 0.533, 1)` over 61.2 frames from -0.6 (rms 0.0035 of `kappa`); in-bounce, `cubic-bezier(0.664, 0, 0.905, 1.269)`
/// over 52.8 frames from -1.5 (rms 0.008): `kappa` rises past 1 (the camera goes a little past the place where the new slide is whole,
/// 1.03 at frame 50) and comes back. Out is in run the other way, `1 - kappa` (the picture of out at `t` is that of in at `1 - kappa`
/// with the slides' places swapped: `kappa` of out + `kappa` of in = 1 to 0.02 at every frame).
fn flythrough_progress(token: Option<&str>, t: f64) -> f64 {
    let token = token.unwrap_or("");
    let kappa = if token.ends_with("bounce") { bezier_frames_xy(0.664, 0.0, 0.905, 1.269, -1.5, 52.8, t) } else { bezier_frames_xy(0.456, 0.0, 0.533, 1.0, -0.6, 61.2, t) };
    if token.is_empty() || token.starts_with("in") { kappa } else { 1.0 - kappa }
}

/// フライスルー: the pictures of the camera's travel `kappa` (see `flythrough_progress`; in the direction "in" of the old slide to the new
/// one; "out" is the same picture with the slides' places swapped: the new slide is the near one, and `kappa` is `flythrough_progress`'s
/// `1 - kappa`, which starts at 1). The camera
/// moves through two slides standing one behind the other, the near one `1` (focal length) from the camera and the far one `1 + FLYTHROUGH_TRAVEL`:
/// it travels `x = FLYTHROUGH_TRAVEL * kappa`, so the near slide is drawn at `1 / (1 - x)` (it flies past the camera at `x = 1`, and
/// fades out at the opacity `1 - x`) and the far one at `1 / (1 + FLYTHROUGH_TRAVEL - x)` (half size at the start, whole at the end).
/// The far one's opacity is `kappa^2`, laid in the rectangle it covers under the near one at the opacity that leaves the weights
/// `near : far : backdrop` = `(1 - x) : kappa^2 : (the rest)` there (so the near slide keeps its weight over the far one: laying it over
/// it would take `1 - x` off the far one again), and the backdrop is grey at the level `1 - kappa` (white at the start, black at the end).
/// Returns `(near, far, backdrop)`: each slide's `(scale, opacity)` and the backdrop's level.
fn flythrough_look(kappa: f64) -> ((f64, f64), (f64, f64), f64) {
    let x = FLYTHROUGH_TRAVEL * kappa;
    let near_opacity = (1.0 - x).clamp(0.0, 1.0);
    let near_scale = if near_opacity > 0.0 { 1.0 / (1.0 - x) } else { 1.0 };
    let far_scale = 1.0 / (1.0 + FLYTHROUGH_TRAVEL - x);
    let far_weight = kappa.clamp(0.0, 1.0).powi(2);
    let far_opacity = if near_opacity < 1.0 { (far_weight / (1.0 - near_opacity)).min(1.0) } else { 0.0 };
    ((near_scale, near_opacity), (far_scale, far_opacity), (1.0 - kappa).clamp(0.0, 1.0))
}

/// How far the camera travels in フライスルー, in units of the distance (the focal length) from the camera to the near slide (see `flythrough_look`).
const FLYTHROUGH_TRAVEL: f64 = 1.2;

/// The turn of ニュース速報's new slide at its start, clockwise, which it unwinds at a constant speed (see `plan`).
const NEWSFLASH_TURN_DEGREES: f64 = 400.0;

/// `progress` where the effect's option (`dir`) changes the curve: カバー and アンカバー run on other curves for the diagonals than for
/// the four straight directions. Measured on PowerPoint for Windows (Microsoft 365, `CreateVideo` 30 frames a second, a 2 second
/// transition, 160 x 90 frames; the slide's place fitted at every frame, one curve `cubic-bezier(x1, y1, x2, 1)` of `len` frames from
/// frame `s` (counted from where the movement is first seen) shared by the four directions of a kind, a frame of offset each):
/// カバー straight 46.2 frames from 0.5, (0.614, 0.117, 0.326), rms error 0.007 of the slide's width; カバー diagonal 53.9 frames from
/// -0.9, (0.402, 0.589, 0.228), 0.004; アンカバー diagonal 29.5 frames from 1.4, (0.653, 0, 0.295), 0.003. (アンカバー straight is the
/// curve below.)
pub fn progress_with(effect: &str, token: Option<&str>, t: f64) -> Option<f64> {
    let t = t.clamp(0.0, 1.0);
    let diagonal = token.is_some_and(|d| d.len() == 2);
    match effect {
        "cover" if diagonal => return Some(bezier_frames(0.402, 0.589, 0.228, -0.9, 53.9, t)),
        "cover" => return Some(bezier_frames(0.614, 0.117, 0.326, 0.5, 46.2, t)),
        "pull" if diagonal => return Some(bezier_frames(0.653, 0.0, 0.295, 1.4, 29.5, t)),
        _ => {}
    }
    match effect {
        "pull" | "cover" => Some(bezier_ease(0.72, 0.36, t)),
        // リビール is the fade through grey or black, linear in time (the curve is inside `reveal_mix`).
        "reveal" | "flash" => Some(t),
        "push" => Some(bezier_ease(0.44, 0.56, t)),
        // 立方体 (`cube`) and 反転 (`flip`): fitted to the angle of the model of `turn3d.rs` over each frame of the video (the
        // transition begins about 11 frames in, `cubic-bezier(x1, 0, x2, 1)` of its 60; rms error 0.002 and 0.009 of the angle).
        "cube" | "rotate" => Some(bezier_ease(0.326, 0.69, t)),
        "box" | "orbit" => Some(bezier_ease(0.346, 0.65, t)),
        // コンベヤー runs at a constant speed in the table of `turn3d.rs`; ハニカム's planes move by the frame (`turn3d_tiles::honeycomb`).
        "conveyor" | "gallery" | "switch" | "ferris" | "fallOver" | "pageCurlSingle" | "peelOff" | "curtains" | "wind" | "pageCurlDouble" | "drape" | "airplane" | "origami" | "crush" | "fracture" | "ripple" | "honeycomb" | "vortex" | "comb" | "blinds" | "checker" | "doors" | "window" => Some(t),
        "flip" => Some(bezier_ease(0.593, 0.358, t)),
        // ワープ: the camera's dolly (see `plan`).
        "warp" => Some(bezier_ease(0.44, 0.56, t)),
        "wipe" | "split" | "strips" | "dissolve" => Some(t),
        // プレステージ: the cloth's own time (see `turn3d_cloth`).
        "prestige" => Some(t),
        // フライスルー: the camera's travel (see `flythrough_progress`).
        "flythrough" => Some(flythrough_progress(token, t)),
        // The shapes' bands travel at a constant speed (measured on PowerPoint for Windows, 1 and 2 seconds); the fade's own
        // curve is `fade_look`'s, which is applied to each half of a fade through black.
        "fade" | "circle" | "diamond" | "plus" | "wedge" | "wheel" | "wheelReverse" | "zoom" | "newsflash" => Some(t),
        _ => None,
    }
}

/// `cubic-bezier(x1, y1, x2, y2)` at time `u` (clamped to 0 to 1).
pub fn bezier_xy(x1: f64, y1: f64, x2: f64, y2: f64, u: f64) -> f64 {
    let u = u.clamp(0.0, 1.0);
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
    3.0 * (1.0 - s) * (1.0 - s) * s * y1 + 3.0 * (1.0 - s) * s * s * y2 + s * s * s
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
        // ズーム: measured on PowerPoint for Windows (`CreateVideo`, 30 frames a second, durations 0.5 to 3 s, 720p): "in" (`dir="in"`)
        // shrinks the old slide into the middle, at a constant speed, in the first half of the time; "out" (no `dir`) grows the new
        // slide out of the middle, at a constant speed, in the second half (nothing moves in the other half). In "out" the new slide
        // itself keeps its size (the user's F2 of 2026-11-23, PowerPoint's recording): only the soft-edged rectangular hole it is seen
        // through grows (`soft_masked_pieces`).
        "zoom" if token == "in" => {
            let s = (2.0 * q).min(1.0);
            TransitionPlan::SoftScaled { old: Some(1.0 - s), new: None, new_on_top: false }
        }
        "zoom" => {
            let s = (2.0 * q - 1.0).max(0.0);
            TransitionPlan::SoftScaled { old: None, new: Some(s), new_on_top: true }
        }
        // ワープ: measured on PowerPoint for Windows (`CreateVideo`, 30 frames a second, 2 seconds, 720p; slides 95 and 96 of
        // `audit-trans-flat-16x9-dur2000`). At each frame the two slides' scales (about the middle), opacities and the compositing were
        // fitted to the picture (the squared error of 320 x 180 pixels, 0.001 to 0.006). The camera is dollied by `k` (0 to 1) of the
        // distance between the slides, which stand one focal length `F` apart, so a slide at distance `d` from the camera is drawn at
        // `F / d`:
        //   in  (the camera moves toward the old slide and through it to the new): the old slide at `1 / (1 - k)` (it flies past the
        //       camera) over the new one at `1 / (2 - k)` (it grows from half size to whole and stays opaque behind it): the old slide is in front at
        //       the opacity `1 - k`, so the picture is `old (1 - k) + new k` where the new slide shows (the same as the pictures added on black);
        //       the fitted scales are those of this model to 0.5 % (the new one `1 / (2 - k)` at `k` 0.24, 0.46, 0.75: 0.568, 0.648,
        //       0.798, fitted 0.567, 0.649, 0.799), and the pictures' weights were fitted free: 0.76 and 0.235 at `k` 0.24, 0.54 and
        //       0.455 at 0.46, 0.25 and 0.742 at 0.75 (the same frames with one layer laid over the other left a 4 to 10 times
        //       larger error);
        //   out (the camera backs off): the old slide at `1 / (1 + k)` laid over the new one at `1 / k` (the new one is whole, in front
        //       of the camera at first), the old one at the opacity `1 - k` (fitted 0.805, 0.445, 0.144 at `k` 0.195, 0.556, 0.85: the
        //       model 0.805, 0.444, 0.15), the scales to 1 % (the new one at `k` 0.261, 0.555, 0.812: 3.83, 1.80, 1.23, fitted 3.78,
        //       1.80, 1.23).
        // `k` runs on `cubic-bezier(0.44, 0, 0.56, 1)` of the 60 frames (rms 0.004 in both; (0.5, 0, 0.5, 1) over 64 frames fits as well).
        "warp" if token == "in" => {
            let k = q.min(0.999);
            TransitionPlan::Dolly { old: (1.0 / (1.0 - k), 1.0 - k), new: (1.0 / (2.0 - k), 1.0), new_on_top: false, additive: false, backdrop: 0.0 }
        }
        "warp" => {
            let k = q.clamp(0.001, 1.0);
            TransitionPlan::Dolly { old: (1.0 / (1.0 + k), 1.0 - k), new: (1.0 / k, 1.0), new_on_top: false, additive: false, backdrop: 0.0 }
        }
        // フライスルー: measured on PowerPoint for Windows (see `flythrough_progress` and `flythrough_look`). In, the old slide is the near
        // one; out, the new slide is the near one (drawn over the old one, which is the far one) and `q` is run the other way.
        "flythrough" if token.is_empty() || token.starts_with("in") => {
            let (near, far, backdrop) = flythrough_look(q);
            TransitionPlan::Dolly { old: near, new: far, new_on_top: false, additive: false, backdrop }
        }
        "flythrough" => {
            let (near, far, backdrop) = flythrough_look(q);
            TransitionPlan::Dolly { old: far, new: near, new_on_top: true, additive: false, backdrop }
        }
        // ニュース速報: measured on PowerPoint for Windows (`CreateVideo`, 30 frames a second, 2 seconds, 720p; slide 103 of
        // `audit-trans-flat-16x9-dur2000`): the old slide stays whole under the new one. The new slide's outline (a rectangle of the
        // slide's shape) was fitted to the frames where it shows (1 - IoU of the changed pixels; the 180 degree ambiguity of a rectangle
        // settled by where the white disc of the slide lies); over the 60.5 frames of the movement, with u the share of the time: the
        // centre stays at the middle of the frame (mean 320.5, 180.3 of 640 x 360, standard deviation under 1 pixel), the width grows
        // linearly from 0 (13814.0) to the frame's width (13874.6) (the fit's residue 1.2 pixels), and the turn is a constant speed,
        // -0.1152 radian a frame (residue 0.011 radian): 0.1152 x 60.5 = 6.98 radian = 400 degrees, ending upright. The opacity of the
        // slide (read at the white disc over the old slide's black) is `1 - (1 - u)^3` (the exponent fitted 2.9 to 3.1 at u = 0.17 to 0.71).
        "newsflash" => TransitionPlan::Spin { scale: q, turn: NEWSFLASH_TURN_DEGREES * (1.0 - q), opacity: 1.0 - (1.0 - q).powi(3) },
        "flash" => TransitionPlan::StripMix { strips: flash_mix(q), backdrop: 1.0, scale: (1.0, 1.0) },
        "flip" => TransitionPlan::Flip { old: (1.0 - 2.0 * q).max(0.0), new: (2.0 * q - 1.0).max(0.0) },
        // The cards of a cube, a box, a rotating slide, an orbit and a switch move as the push does.
        "cube" | "box" | "rotate" | "orbit" | "switch" | "ferris" => slide(motion(token), true),
        // ripple: rings from the middle (the corners are drawn the same).
        "ripple" => windows("circle(out)"),
        "comb" => windows(if token == "vert" { "comb(vertical)" } else { "comb(horizontal)" }),
        "blinds" => TransitionPlan::Windows(blinds_windows(token != "vert", q)),
        "checker" => TransitionPlan::Windows(checker_windows(token != "vert", q)),
        "doors" | "window" => windows(if token == "horz" { "barn(outHorizontal)" } else { "barn(outVertical)" }),
        "vortex" | "honeycomb" | "glitter" => windows("dissolve"),
        // 2013's effects: the old slide leaves, curtains opening, strips falling, a wind of strips, the page lifted off.
        "curtains" => windows("barn(outVertical)"),
        "drape" => windows(if inv { "randombar(vertical)" } else { "blinds(vertical)" }),
        "wind" | "fallOver" => TransitionPlan::SoftWindows(wipe_strips(if inv { 'l' } else { 'r' }, q)),
        "peelOff" | "pageCurlDouble" | "pageCurlSingle" | "airplane" | "origami" => TransitionPlan::Slide { from: (0.0, 0.0), old_to: if inv { (1.0, 0.0) } else { (-1.0, 0.0) }, new_on_top: false },
        "fracture" | "crush" => windows("dissolve"),
        _ => return None,
    })
}

/// Where the waves of さざなみ start for option `token` (`dir`): the middle, or the corner opposite to the way they spread (`ru`, up and right, starts at the bottom left; measured as the catalogue says).
fn ripple_origin(token: &str) -> (f64, f64) {
    match token {
        "lu" => (1.0, 1.0),
        "ru" => (-1.0, 1.0),
        "ld" => (1.0, -1.0),
        "rd" => (-1.0, -1.0),
        _ => (0.0, 0.0),
    }
}

/// How many pieces a wipe is drawn in (each at one opacity; a soft edge needs more than rectangles give).
const WIPE_STRIPS: usize = 48;
/// Checkerboard: the squares of the slide (measured on PowerPoint for Windows: 7 across and 5 down, on 16:9 and 4:3 slides, for
/// both options), and the share of the transition one square takes to turn over (about 5 frames of the 60 of two seconds).
const CHECKER_COLUMNS: usize = 7;
const CHECKER_ROWS: usize = 5;
const CHECKER_TURN: f64 = 5.0 / 60.0;
/// Blinds: the slats (measured the same on both slide shapes: 14 horizontal slats, 18 vertical ones).
pub const BLINDS_HORIZONTAL: usize = 14;
pub const BLINDS_VERTICAL: usize = 18;

/// ワイプ: the new slide's opacity at distance `s` (0 to 1, from the edge the wipe starts at) when the wipe has run `t` (0 to 1,
/// at a constant speed): a half cosine as wide as the slide, whose middle goes from a slide's half width before the start edge
/// to a half beyond the far one (measured on PowerPoint for Windows: width 1288 px on a 1280 px slide, 42 px a frame, 30 frames
/// a second, 2 seconds).
pub fn wipe_opacity(t: f64, s: f64) -> f64 {
    let u = (2.0 * t - s).clamp(0.0, 1.0);
    (1.0 - (std::f64::consts::PI * u).cos()) / 2.0
}

/// The shapes whose edge between the old slide and the new one is a soft band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoftShape {
    Circle,
    Diamond,
    Plus,
    Wedge,
    Wheel,
    /// The animations' ボックス (`phi = max(|u|, |v|)`) and スプリット (`|u|` splitting across, `|v|` splitting down): the same soft edge as the shapes above.
    Box,
    SplitAcross,
    SplitDown,
}

/// Rows and columns the soft shapes are sampled at, and the opacity levels they are drawn in (each level one set of windows).
pub const SOFT_ROWS: usize = 72;
pub const SOFT_COLUMNS: usize = 144;
pub const SOFT_LEVELS: usize = 16;

/// The shape's own distance `phi` of a point of the slide (`x`, `y` as fractions), the distance `big` at which the shape has
/// covered the slide, and the band's width.
/// Measured on PowerPoint for Windows (1280 x 720, 30 frames a second, 1 and 2 seconds; the new slide's share of each pixel
/// from the frames before and after, rms error 0.002 to 0.02 of it): the new slide's share is `1 - B((phi - c) / w + 1/2)` with B
/// the half cosine step `(1 - cos(pi u)) / 2`, and `c` runs at a constant speed from `-w/2` to `big + w/2`, so that the band
/// has just left the slide at the end.
/// * Circle: `phi` the distance from the middle in pixels (a circle on a 16:9 slide), `big` the distance to a corner, `w = big / 3`.
/// * Diamond: `phi = |u| + |v|` with `u`, `v` the distances from the middle as fractions of the slide's half width and half height
///   (up to 2), `w = big / 3`. Plus: `phi = min(|u|, |v|)` (up to 1), `w = big / 3`.
/// * Wedge: `phi` the angle from the top in those same (u, v) units, either way round (up to 180 degrees), `w = big / 3`.
/// * Wheel: the angle clockwise from the top in (u, v) units, taken modulo a spoke's sector (`360 / spokes`), `w = 0.12` of the
///   sector: the sector's leading edge is soft and where the sweep started is hard.
fn soft_phi(shape: SoftShape, spokes: usize, reverse: bool, x: f64, y: f64, aspect: f64, band: Option<f64>) -> (f64, f64, f64) {
    let (u, v) = (2.0 * x - 1.0, 2.0 * y - 1.0);
    let angle = |a: f64| a.rem_euclid(360.0);
    // `band`: the width of the soft edge as a share of `big` where it is not the transition's (a third of it).
    let width = |big: f64| band.map_or(big / 3.0, |b| b * big);
    // Entering from the outside (`reverse`): the distance to where the shape has covered the slide.
    let entering = |phi: f64, big: f64| if reverse { big - phi } else { phi };
    match shape {
        SoftShape::Circle => {
            let big = 0.5 * aspect.hypot(1.0);
            let phi = ((x - 0.5) * aspect).hypot(y - 0.5);
            (entering(phi, big), big, width(big))
        }
        SoftShape::Diamond => (entering(u.abs() + v.abs(), 2.0), 2.0, width(2.0)),
        SoftShape::Plus => (entering(u.abs().min(v.abs()), 1.0), 1.0, width(1.0)),
        SoftShape::Box => (entering(u.abs().max(v.abs()), 1.0), 1.0, width(1.0)),
        SoftShape::SplitAcross => (entering(u.abs(), 1.0), 1.0, width(1.0)),
        SoftShape::SplitDown => (entering(v.abs(), 1.0), 1.0, width(1.0)),
        SoftShape::Wedge => {
            let a = angle(u.atan2(-v).to_degrees());
            (entering(if a > 180.0 { 360.0 - a } else { a }, 180.0), 180.0, width(180.0))
        }
        SoftShape::Wheel => {
            let sector = 360.0 / spokes.max(1) as f64;
            let a = angle(if reverse { -u.atan2(-v).to_degrees() } else { u.atan2(-v).to_degrees() });
            (a.rem_euclid(sector), sector, 0.12 * sector)
        }
    }
}

/// ランダム ストライプ: the slide cut into bars of 1.5 pt (2 px of the 1280 x 720 video: 640 across a 16:9 slide, as many down as make them square), each coming
/// in at a time of its own, `0.15 + 0.25 (u1 + u2)` of the transition (a triangle from 0.15 to 0.65, mean 0.4). Measured on PowerPoint for Windows
/// (`CreateVideo`, 1280 x 720, 30 frames a second, 2 seconds, the vertical option): every bar switches once and for good (no fade, 0 of 1280 columns back);
/// two columns at a time (the pairs from an even column agree in 80 % of them, the others in 37 %); the share switched, frame by frame over the whole picture,
/// fits the distribution function of `c + w (u1 + u2)` with rms 0.008 (w 0.253, mean 0.397; a normal one 0.011, an even spread 0.011), whatever the start taken. The animations' bars are a pixel and come in evenly
/// (`animation_preset`): another effect.
fn random_bars(vertical: bool, q: f64, aspect: f64) -> Vec<Rect> {
    const ACROSS: f64 = 640.0;
    let n = if vertical { ACROSS } else { ACROSS / aspect.max(0.1) }.round().clamp(1.0, 4096.0) as usize;
    let seed = if vertical { 0 } else { 1 << 20 };
    let shown = |i: usize| q >= 0.15 + 0.25 * (crate::animation_preset::scatter(seed + 2 * i) + crate::animation_preset::scatter(seed + 2 * i + 1));
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..=n {
        match (i < n && shown(i), start) {
            (true, None) => start = Some(i),
            (false, Some(a)) => {
                let (a, z) = (a as f64 / n as f64, i as f64 / n as f64);
                out.push(if vertical { [a, 0.0, z, 1.0] } else { [0.0, a, 1.0, z] });
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// The new slide's share (0 to 1) at point (`x`, `y`) of a slide of `aspect` when a soft shape has run `q` of its time.
pub fn soft_shape_alpha(shape: SoftShape, spokes: usize, reverse: bool, q: f64, x: f64, y: f64, aspect: f64) -> f64 {
    soft_shape_alpha_band(shape, spokes, reverse, q, x, y, aspect, None)
}

/// `soft_shape_alpha` with the soft edge's width given as a share of `big` (`None`: a third of it, a slide transition's). The same function draws the
/// animations' circle, diamond, plus and wedge, whose edge is narrower (`animation_preset::SOFT_BAND`).
#[allow(clippy::too_many_arguments)]
pub fn soft_shape_alpha_band(shape: SoftShape, spokes: usize, reverse: bool, q: f64, x: f64, y: f64, aspect: f64, band: Option<f64>) -> f64 {
    let (phi, big, w) = soft_phi(shape, spokes, reverse, x, y, aspect, band);
    let c = -w / 2.0 + (big + w) * q;
    let u = ((phi - c) / w + 0.5).clamp(0.0, 1.0);
    1.0 - (1.0 - (std::f64::consts::PI * u).cos()) / 2.0
}

/// A soft shape as windows over the old slide, each at an opacity: the windows of level `k` (of `SOFT_LEVELS`) cover where the new
/// slide's share is at least `(k - 1/2) / levels`, at opacity `1 / (levels - k + 1)`, so the levels drawn one over another build the
/// share up to `k / levels`.
pub fn soft_shape(shape: SoftShape, spokes: usize, reverse: bool, q: f64, aspect: f64) -> Vec<(Rect, f64)> {
    // The wheel's spokes meet at the middle, where one cell sees every angle (a square block there, and steps along the edges): its
    // grid is twice as fine, and its band has twice the levels.
    let (rows, columns, levels) = if shape == SoftShape::Wheel { (SOFT_ROWS * 2, SOFT_COLUMNS * 2, SOFT_LEVELS * 2) } else { (SOFT_ROWS, SOFT_COLUMNS, SOFT_LEVELS) };
    let share: Vec<Vec<f64>> = (0..rows)
        .map(|r| (0..columns).map(|c| soft_shape_alpha(shape, spokes, reverse, q, (c as f64 + 0.5) / columns as f64, (r as f64 + 0.5) / rows as f64, aspect)).collect())
        .collect();
    share_windows_of_levels(&share, levels)
}

/// A grid of shares (0 to 1, `share[row][column]`) as windows over the old picture, each at an opacity, the levels drawn one over
/// another (see `soft_shape`); the windows are in fractions of the grid.
pub fn share_windows(share: &[Vec<f64>]) -> Vec<(Rect, f64)> {
    share_windows_of_levels(share, SOFT_LEVELS)
}

/// `share_windows` with `n` levels of opacity (more levels draw a smoother band, in more windows).
pub fn share_windows_of_levels(share: &[Vec<f64>], n: usize) -> Vec<(Rect, f64)> {
    let (rows, columns) = (share.len(), share.first().map_or(0, |r| r.len()));
    let mut out = Vec::new();
    for k in 1..=n {
        let threshold = (k as f64 - 0.5) / n as f64;
        let opacity = 1.0 / (n - k + 1) as f64;
        // Runs of columns per row, then rows with the same run joined.
        let mut open: Vec<(usize, usize, usize)> = Vec::new(); // (first column, past-the-end column, first row)
        let flush = |open: &mut Vec<(usize, usize, usize)>, keep: &[(usize, usize)], row: usize, out: &mut Vec<(Rect, f64)>| {
            open.retain(|&(a, z, r0)| {
                if keep.contains(&(a, z)) {
                    true
                } else {
                    out.push(([a as f64 / columns as f64, r0 as f64 / rows as f64, z as f64 / columns as f64, row as f64 / rows as f64], opacity));
                    false
                }
            });
        };
        for r in 0..=rows {
            let mut runs: Vec<(usize, usize)> = Vec::new();
            if r < rows {
                let mut start: Option<usize> = None;
                for c in 0..=columns {
                    let on = c < columns && share[r][c] >= threshold;
                    match (on, start) {
                        (true, None) => start = Some(c),
                        (false, Some(a)) => {
                            runs.push((a, c));
                            start = None;
                        }
                        _ => {}
                    }
                }
            }
            flush(&mut open, &runs, r, &mut out);
            for run in runs {
                if !open.iter().any(|&(a, z, _)| (a, z) == run) {
                    open.push((run.0, run.1, r));
                }
            }
        }
    }
    out
}

/// A fade at `t` (0 to 1 of its time, linear): `(through the new slide, dim)`. A plain fade shows the new slide at the opacity
/// `ease(t)`. Through black, the first half takes the old slide to black and the second brings the new one out of it, each half
/// with the same ease (measured on PowerPoint for Windows, the mean of the picture over the frames, 0.5, 1 and 2 seconds: each
/// half is `cubic-bezier(0.35, 0, 0.65, 1)` of its time, rms error 0.5 of 255; black is only an instant, not a held screen).
/// Returns `(new slide is the one shown, its opacity)`; for a plain fade the old slide stays under it.
pub fn fade_look(through_black: bool, t: f64) -> (bool, f64) {
    let t = t.clamp(0.0, 1.0);
    if !through_black {
        return (true, bezier_ease(0.35, 0.65, t));
    }
    if t < 0.5 {
        (false, 1.0 - bezier_ease(0.35, 0.65, t * 2.0))
    } else {
        (true, bezier_ease(0.35, 0.65, t * 2.0 - 1.0))
    }
}

/// リビール (`p14:reveal`) at `t` (0 to 1 of its time, linear). Measured on PowerPoint for Windows (Microsoft 365, `CreateVideo`
/// 30 frames a second, a 2 second transition = 60 frames, over the 160 x 90 frames of the slide): the old slide fades to the backdrop and
/// the new one comes out of it, each as `cubic-bezier(0.35, 0, 0.65, 1)` (as the fade's halves are), but the moment depends on where the
/// column is, `x'` (0 to 1: from the side the fade starts at): the old slide fades during `a0 + s * x'` for `len` frames of 60, and the
/// new one during `60 - a0 - s * x' - len` for `len` frames, so the old one is gone from the side it starts at first and the new one is
/// there first at the other. Fitted to every pixel of 66 frames of the four variants (rms error 0.033 of 255 for the fade through
/// grey, 0.034 for black, of which the part not due to the model is the picture's own motion); the ease cannot be told from
/// `(0.25, 0.75)` to `(0.5, 0.5)` by the fit.
/// Through black (`blk-l`, `blk-r`) the backdrop is black and the old slide fades from the left (`l`) first; the grey one (`l`, `r`)
/// fades from the right (`l`) first, over a backdrop that goes from black to white (`l`) or white to black (`r`) linearly during
/// 5/6 of the time around the middle (0.5 at the middle, 1.476 a time unit).
pub fn reveal_mix(through_black: bool, left: bool, t: f64) -> (Vec<StripMix>, f64) {
    const STRIPS: usize = 64;
    let k = t.clamp(0.0, 1.0) * 60.0;
    // (a0, s, len) in frames of 60 and the direction of the sweep: the old slide leaves the left first for black-left and grey-right.
    let (a0, s, len, from_left) = if through_black { (0.2, 6.4, 25.4, left) } else { (0.8, 5.75, 22.9, !left) };
    let out = (0..STRIPS)
        .map(|i| {
            let x = (i as f64 + 0.5) / STRIPS as f64;
            let xp = if from_left { x } else { 1.0 - x };
            let old_start = a0 + s * xp;
            let new_start = 60.0 - old_start - len;
            let new = bezier_ease(0.35, 0.65, (k - new_start) / len);
            let old = if k > new_start { 0.0 } else { 1.0 - bezier_ease(0.35, 0.65, (k - old_start) / len) };
            StripMix { rect: [i as f64 / STRIPS as f64, 0.0, (i + 1) as f64 / STRIPS as f64, 1.0], old, new }
        })
        .collect();
    let backdrop = if through_black { 0.0 } else { (0.5 + 1.476 * (t.clamp(0.0, 1.0) - 0.5) * if left { 1.0 } else { -1.0 }).clamp(0.0, 1.0) };
    (out, backdrop)
}

/// How much larger than whole リビール draws the slides, `(old, new)`, at `t`: the old slide grows from its size to `1 + REVEAL_ZOOM` at a
/// constant rate through the first half, then the new slide is drawn from `1 + REVEAL_ZOOM` back to its size through the second half (the
/// peak is at the middle, where only the backdrop shows). Measured on PowerPoint for Windows (`CreateVideo`, 30 frames a second, a 2
/// second transition, `reveal/l` of `audit-trans-flat-16x9-dur2000`): the scale of the whole picture about its middle, found at each frame
/// as the one that makes the gradient picture best correlate with the old slide (frames 29 to 53: 1.002 to 1.034, the least squares line
/// 0.00139 a frame through 0, which is 0.0417 at the middle frame 58) and with the new slide (frames 62 to 88: 1.033 to 1.0, the line
/// reaches 0 at frame 88.4 = 0.0398 at the middle); the frames of the middle (54 to 61) are the backdrop and not measurable. A slide of the
/// other direction or colour is taken to be the same.
pub const REVEAL_ZOOM: f64 = 0.0408;

pub fn reveal_scale(t: f64) -> (f64, f64) {
    let t = t.clamp(0.0, 1.0);
    (1.0 + REVEAL_ZOOM * (2.0 * t).min(1.0), 1.0 + REVEAL_ZOOM * (2.0 - 2.0 * t).min(1.0))
}

/// The new slide's weight in フラッシュ (`p14:flash`) for each of three parts of the slide, at frame `k` of 60: points `(frame, weight)`
/// joined by straight lines. Measured on PowerPoint for Windows (Microsoft 365, `CreateVideo` 30 frames a second, a 2 second transition
/// = 60 frames, 160 x 90 frames of the slide, the weight of the new picture in the white-to-new mix, 66 frames of it): the whole slide
/// is white from about frame 8 on and the new slide comes out of the white at a different time in each part, left to right: the
/// left half (x < 0.5) from frame 12.5 at a steady rate, the next 0.3 (to x = 0.8) from 11 at a slower rate until frame 35 and a quicker one after, the last 0.2 from 23 likewise; all are whole at frame 54.5
/// (rms error of the lines against the measured weights 0.01 of 255; PowerPoint's own curve is not a single formula; the points are
/// the weights at every third frame). Only the 2 second transition was filmed: the frames are taken as shares of the duration.
const FLASH_NEW: [&[(f64, f64)]; 3] = [
    &[(12.5, 0.0), (14.0, 0.09), (17.0, 0.14), (20.0, 0.20), (26.0, 0.34), (32.0, 0.46), (38.0, 0.57), (44.0, 0.69), (50.0, 0.84), (53.0, 0.96), (54.5, 1.0)],
    &[(11.0, 0.0), (14.0, 0.03), (20.0, 0.07), (26.0, 0.13), (32.0, 0.21), (35.0, 0.25), (38.0, 0.35), (41.0, 0.45), (44.0, 0.58), (47.0, 0.71), (50.0, 0.83), (53.0, 0.96), (54.5, 1.0)],
    &[(23.0, 0.0), (26.0, 0.02), (29.0, 0.05), (32.0, 0.08), (35.0, 0.11), (38.0, 0.23), (41.0, 0.35), (44.0, 0.50), (47.0, 0.65), (50.0, 0.80), (53.0, 0.95), (54.5, 1.0)],
];

fn polyline(points: &[(f64, f64)], k: f64) -> f64 {
    match points.iter().position(|&(f, _)| f > k) {
        None => points.last().map_or(0.0, |p| p.1),
        Some(0) => points[0].1,
        Some(i) => {
            let ((f0, w0), (f1, w1)) = (points[i - 1], points[i]);
            w0 + (w1 - w0) * (k - f0) / (f1 - f0)
        }
    }
}

/// フラッシュ at `t` (0 to 1 of its time, linear): the old slide goes white, straight, between frames 2.3 and 8 of 60 (measured with
/// the new slide's weights above), then the new slide comes out of the white (`FLASH_NEW`), in 20 strips.
pub fn flash_mix(t: f64) -> Vec<StripMix> {
    const STRIPS: usize = 20;
    let k = t.clamp(0.0, 1.0) * 60.0;
    let old = (1.0 - (k - 2.3) / 5.7).clamp(0.0, 1.0);
    (0..STRIPS)
        .map(|i| {
            let x = (i as f64 + 0.5) / STRIPS as f64;
            let part = if x < 0.5 { 0 } else if x < 0.8 { 1 } else { 2 };
            StripMix { rect: [i as f64 / STRIPS as f64, 0.0, (i + 1) as f64 / STRIPS as f64, 1.0], old, new: polyline(FLASH_NEW[part], k) }
        })
        .collect()
}

/// スプリット at `t`: a wipe (`wipe_opacity`) that runs from the middle to the two edges (`inward` false) or from the edges to the middle,
/// with strips across the width (`horizontal` false: the line splits left from right) or the height. Measured on PowerPoint for Windows
/// (2 seconds, 30 frames a second, 160 x 90 frames, the new slide's share at 40 places of the line, all four options): the same cosine
/// of `u = 2 t - s` as the wipe, `s` the distance from where it starts (0 to 1), `u` 0.127 ahead (3.8 frames), rms error 0.008 of 255.
fn split_strips(horizontal: bool, inward: bool, t: f64) -> Vec<(Rect, f64)> {
    let mut out = Vec::new();
    for i in 0..WIPE_STRIPS {
        let (a, z) = (i as f64 / WIPE_STRIPS as f64, (i + 1) as f64 / WIPE_STRIPS as f64);
        let d = ((a + z) / 2.0 - 0.5).abs() * 2.0;
        let rect: Rect = if horizontal { [0.0, a, 1.0, z] } else { [a, 0.0, z, 1.0] };
        let alpha = wipe_opacity(t + 0.0635, if inward { 1.0 - d } else { d });
        if alpha > 0.004 {
            out.push((rect, alpha));
        }
    }
    out
}

/// The width of the soft edge of ズーム's slide as a share of the frame's width and of its height (47 pixels of 1280 and 26 of 720).
pub const ZOOM_EDGE: f64 = 47.0 / 1280.0;

/// One piece of a slide drawn with a soft edge: where it goes on the screen (fractions of the screen), which part of the slide's picture
/// it shows (fractions of the slide, 0 to 1) and its opacity.
pub type SoftPiece = (Rect, Rect, f64);

/// The pieces of a slide at the scale `k` about the middle of the screen, whose edge fades by a half cosine of the width `ZOOM_EDGE` of the
/// frame centred on the edge (a slide's edge, half inside it and half outside): measured on PowerPoint for Windows (`CreateVideo`, 720p,
/// 2 s, the old slide shrinking, the new one growing: fitted at five scales, 47 pixels of 1280 wide, in and out, rms of the share 0.02,
/// the centre of the band within 4 pixels of the slide's edge). The part of the band outside the slide shows the slide's edge pixels
/// (the pieces' picture parts are held to the slide).
pub fn soft_scaled_pieces(k: f64) -> Vec<SoftPiece> {
    soft_pieces(k, false)
}

/// ズーム: the slide (the old one shrinking in "in", the new one growing in "out") is not scaled, it stays whole and fixed, and is seen through a rectangular hole about the middle of the
/// screen (the share `k` of the screen) whose edge fades by the same half cosine (PowerPoint's recording: the next slide's size does not
/// change, only the hole grows). Pieces as `soft_scaled_pieces`, but each shows the part of the slide it lies over.
pub fn soft_masked_pieces(k: f64) -> Vec<SoftPiece> {
    soft_pieces(k, true)
}

fn soft_pieces(k: f64, fixed: bool) -> Vec<SoftPiece> {
    const STEPS: usize = 12;
    if k <= 0.0 {
        return Vec::new();
    }
    // The ramp: 0 a half band outside the edge, 1 a half band inside, a half cosine between.
    let ramp = |d: f64, band: f64| {
        let u = (d / band + 0.5).clamp(0.0, 1.0);
        (1.0 - (std::f64::consts::PI * u).cos()) / 2.0
    };
    // The breakpoints along an axis: each edge's band in STEPS parts, and the interval between them (alpha is 1 there), over 0 to 1.
    let axis = |band: f64| -> Vec<f64> {
        let (lo, hi) = (0.5 - k / 2.0, 0.5 + k / 2.0);
        let mut v: Vec<f64> = vec![0.0, 1.0];
        for edge in [lo, hi] {
            for i in 0..=STEPS {
                v.push(edge - band / 2.0 + band * i as f64 / STEPS as f64);
            }
        }
        v.retain(|x| (0.0..=1.0).contains(x));
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        v
    };
    let alpha_at = |x: f64, band: f64| ramp(x - (0.5 - k / 2.0), band) * ramp((0.5 + k / 2.0) - x, band);
    let uv = |a: f64, b: f64| {
        // The part of the slide a span of the screen shows, held to the slide: a span outside it shows the edge line.
        if fixed {
            return (a, b);
        }
        let (u0, u1) = (((a - (0.5 - k / 2.0)) / k).clamp(0.0, 1.0), ((b - (0.5 - k / 2.0)) / k).clamp(0.0, 1.0));
        if u1 - u0 < 1e-4 { (u0.min(1.0 - 1e-4), u0.min(1.0 - 1e-4) + 1e-4) } else { (u0, u1) }
    };
    let (xs, ys) = (axis(ZOOM_EDGE), axis(ZOOM_EDGE));
    let mut out = Vec::new();
    for yw in ys.windows(2) {
        for xw in xs.windows(2) {
            let alpha = alpha_at((xw[0] + xw[1]) / 2.0, ZOOM_EDGE) * alpha_at((yw[0] + yw[1]) / 2.0, ZOOM_EDGE);
            if alpha < 0.002 {
                continue;
            }
            let (u, v) = (uv(xw[0], xw[1]), uv(yw[0], yw[1]));
            out.push(([xw[0], yw[0], xw[1], yw[1]], [u.0, v.0, u.1, v.1], alpha));
        }
    }
    out
}

/// ストリップ at `t` (0 to 1, linear): a soft wipe that sweeps the slide along its diagonal from the corner `(x0, y0)` (fractions of
/// the width and height) to the opposite one, drawn as a grid of tiles each at its own opacity. Measured on PowerPoint for Windows
/// (2 seconds, 30 frames a second, 160 x 90 frames, the new slide's share in 10 x 10 pixel blocks, all four directions, each fitted with
/// its direction free and coming out at 45 degrees of the slide's own fractions, 135 and 225 degrees): the cosine of
/// `u = (k + 6.2 - 27.6 s) / 30.1`, `k` the frame (of 60), `s` (0 to 1) the sum of the distances from the corner over 2; rms error 0.004.
fn diagonal_wipe(x0: f64, y0: f64, t: f64) -> Vec<(Rect, f64)> {
    const COLS: usize = 64;
    const ROWS: usize = 36;
    let k = t.clamp(0.0, 1.0) * 60.0;
    let mut out = Vec::new();
    for j in 0..ROWS {
        for i in 0..COLS {
            let rect: Rect = [i as f64 / COLS as f64, j as f64 / ROWS as f64, (i + 1) as f64 / COLS as f64, (j + 1) as f64 / ROWS as f64];
            let (cx, cy) = ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
            let s = ((cx - x0).abs() + (cy - y0).abs()) / 2.0;
            let u = ((k + 6.2 - 27.6 * s) / 30.1).clamp(0.0, 1.0);
            let alpha = (1.0 - (std::f64::consts::PI * u).cos()) / 2.0;
            if alpha > 0.004 {
                out.push((rect, alpha));
            }
        }
    }
    out
}

/// When the share of the tiles of ディゾルブ that have appeared reaches each value, at frame `k` of 60: `(frame, share)`. Measured on
/// PowerPoint for Windows (2 seconds, 30 frames a second, the 1280 x 720 video averaged to the 54 x 42 tiles; the share of tiles
/// whose new-slide weight is above a half at every fifth frame; 2193 tiles).
const DISSOLVE_SHARE: [(f64, f64); 13] = [(3.0, 0.0), (5.0, 0.014), (10.0, 0.067), (15.0, 0.131), (20.0, 0.236), (25.0, 0.358), (30.0, 0.507), (35.0, 0.634), (40.0, 0.745), (45.0, 0.829), (50.0, 0.917), (55.0, 0.965), (57.0, 1.0)];

/// ディゾルブ at `t` (0 to 1, linear): the slide is 54 x 42 tiles (the grid in the video: columns every 23.7 pixels of 1280 and rows
/// every 17.1 of 720), each of which changes to the new slide at a time of its own, in about 4 frames (the shortest fade of a tile in the
/// video), the times spread as `DISSOLVE_SHARE`. The order is a fixed shuffle (PowerPoint's is random and not the same twice).
fn dissolve_tiles(t: f64) -> Vec<(Rect, f64)> {
    const COLS: usize = 54;
    const ROWS: usize = 42;
    const FADE: f64 = 4.0;
    let k = t.clamp(0.0, 1.0) * 60.0;
    let mut out = Vec::new();
    for j in 0..ROWS {
        for i in 0..COLS {
            // The tile's frame: the point where the share of tiles is its draw (`scatter`), by the measured shares.
            let u = scatter(j * COLS + i) * 0.999 + 0.0005;
            let at = DISSOLVE_SHARE.windows(2).find(|w| u <= w[1].1).map_or(57.0, |w| w[0].0 + (w[1].0 - w[0].0) * (u - w[0].1) / (w[1].1 - w[0].1));
            let alpha = ((k - at) / FADE + 0.5).clamp(0.0, 1.0);
            if alpha > 0.004 {
                out.push(([i as f64 / COLS as f64, j as f64 / ROWS as f64, (i + 1) as f64 / COLS as f64, (j + 1) as f64 / ROWS as f64], alpha));
            }
        }
    }
    out
}

/// How much of the old slide's tiles are still shown, `frames` (of the 124 of the transition) after the sweep's front has passed them. Fitted so that the whole
/// picture's non-black neutral share (the old slide's tiles in the video of a white slide to four colours, 1080p, 62.5 frames a second; dim tiles count) follows
/// the video's frame by frame: the share at frame `k` is the mean over the slide's width of `VORTEX_REMAIN` at `k - 2.7 - 27.4 x` (the front's sweep), the knots
/// (every 3 frames from the front) found by least squares with a second-difference penalty and made not to rise (the rms miss of the fit is 0.008 of the picture;
/// the table that stood here, the share of white pixels pooled over 24 columns with the front's place subtracted, missed it by 0.156 rms: at frame 30 it gave 0.33
/// of the picture, the video's 0.58, so ibisOffice drew half the tiles PowerPoint does).
const VORTEX_REMAIN: [(f64, f64); 17] = [(-3.0, 1.0), (0.0, 0.662), (9.0, 0.662), (12.0, 0.623), (15.0, 0.554), (18.0, 0.525), (21.0, 0.487), (24.0, 0.399), (27.0, 0.367), (36.0, 0.367), (39.0, 0.215), (42.0, 0.16), (45.0, 0.02), (48.0, 0.006), (60.0, 0.0), (61.0, 0.0), (62.0, 0.0)];

/// How much of the new slide's tiles are shown, `frames` before they settle (the new slide's tiles come back after the front has passed, run backwards: `frames` is
/// 124 less the frame less the front's frame). Fitted as `VORTEX_REMAIN` is, to what is left of PowerPoint's whole-picture coverage after the old slide's share
/// (the 720p video of the flat deck, `vortex/l`, every 100 ms: 0.168 at 1.1 s, 0.277 at 1.3 s, 0.615 at 1.5 s, 0.769 at 1.7 s, 0.946 at 2.0 s; rms miss 0.007): the new
/// slide does not come back as the old one went (the share is not its mirror: 0.72 to 0.82 at the front, then falling to 0.1 over 30 frames), so the middle of the
/// transition has far fewer tiles of it than the mirror gave (0.26 of the picture at 1.1 s against PowerPoint's 0.17).
const VORTEX_REMAIN_NEW: [(f64, f64); 14] = [(-15.0, 1.0), (-12.0, 0.937), (-9.0, 0.798), (-6.0, 0.76), (9.0, 0.76), (12.0, 0.743), (15.0, 0.666), (18.0, 0.557), (21.0, 0.39), (24.0, 0.252), (27.0, 0.148), (30.0, 0.1), (45.0, 0.0), (60.0, 0.0)];

fn vortex_remain_new(frames: f64) -> f64 {
    if frames <= VORTEX_REMAIN_NEW[0].0 {
        return 1.0;
    }
    VORTEX_REMAIN_NEW.windows(2).find(|w| frames <= w[1].0).map_or(0.0, |w| w[0].1 + (w[1].1 - w[0].1) * (frames - w[0].0) / (w[1].0 - w[0].0))
}

fn vortex_remain(frames: f64) -> f64 {
    if frames <= VORTEX_REMAIN[0].0 {
        return 1.0;
    }
    VORTEX_REMAIN.windows(2).find(|w| frames <= w[1].0).map_or(0.0, |w| w[0].1 + (w[1].1 - w[0].1) * (frames - w[0].0) / (w[1].0 - w[0].0))
}

/// 渦巻き (`p14:vortex`): the slide is 128 x 100 tiles (15 x 10.8 pixels of 1920 x 1080: the black pieces' widths are 15 in 41 of 46 whole ones, the
/// heights 11 or 10). The old slide's tiles are thrown away behind a front that sweeps from the side the option names (`l` starts at the right) and
/// the new slide's tiles come back the same way run backwards, over black; the middle of the transition is black. Measured on PowerPoint for
/// Windows: the 1080p60 video (white to four colours: the old slide's white share per column, and the new slide's colour share, fitted over 24
/// columns): the front (the share dropping through 0.7) crosses the slide in 27.4 of 125 frames, from frame 2.7 (`front(s)`, `s` 0 to 1 from the
/// side it starts at), linear in the column with a largest miss of 0.9 frames; the old slide's remaining share `VORTEX_REMAIN` behind it; and
/// the new slide's present share at frame `k` of the column is the old's at frame `124 - k`: the sums of the two crossing frames over the 24 columns
/// are 502 within 1.5 frames (so the effect is its own reverse, with the same column). The 720p30 audit video of the four options agrees on the
/// way (`l` starts at the right, `r` at the left, `u` at the bottom, `d` at the top) and on the time the front takes across (13 to 14 of 30 frames
/// a second). Against that video (its own slides, 60 frames, the four options) the squared error of the 160 x 90 blurred frames at five moments is 0.0181 (`l`), 0.0141 (`r`), 0.0136 (`u`), 0.0141 (`d`), where the dissolve stood in it was 0.116 to 0.130. The tiles' own motion in 3D (they grow as they come toward the viewer) is not drawn; which tile goes when is a fixed shuffle.
pub const VORTEX_COLS: usize = 128;
pub const VORTEX_ROWS: usize = 100;
/// The frames the old tile still shows after the front (see `VORTEX_REMAIN`) is 45; a tile's `age` below is in these frames.
pub const VORTEX_LIFE: f64 = 45.0;

/// One tile of 渦巻き at some moment: where it is in the grid and, when its old (or new) face is shown, how many frames after the front it is (for the new face: how
/// many frames before it settles).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VortexTile {
    pub col: usize,
    pub row: usize,
    pub old: Option<f64>,
    pub new: Option<f64>,
}

/// The tiles of 渦巻き at `q` that show their old or new face (see `vortex_tiles`).
pub fn vortex_pieces(token: &str, q: f64) -> Vec<VortexTile> {
    // The transition is 125 frames of the 1080p video (2 s at 62.5 frames a second, from frame 189 to 313 as the pixels change); the two crossing frames of a column add up to 124.
    const FRAMES: f64 = 125.0;
    const REVERSE: f64 = 124.0;
    let k = q.clamp(0.0, 1.0) * FRAMES;
    let mut out = Vec::new();
    for j in 0..VORTEX_ROWS {
        for i in 0..VORTEX_COLS {
            let (x, y) = ((i as f64 + 0.5) / VORTEX_COLS as f64, (j as f64 + 0.5) / VORTEX_ROWS as f64);
            let sweep = match token {
                "r" => x,
                "u" => 1.0 - y,
                "d" => y,
                _ => 1.0 - x,
            };
            let front = 2.7 + 27.4 * sweep;
            let n = j * VORTEX_COLS + i;
            let old_age = k - front;
            let new_age = REVERSE - k - front;
            let old = (crate::animation_preset::scatter(2 * n) < vortex_remain(old_age)).then_some(old_age);
            let new = (q >= 1.0 || crate::animation_preset::scatter(2 * n + 1) < vortex_remain_new(new_age)).then_some(new_age);
            if old.is_some() || new.is_some() {
                out.push(VortexTile { col: i, row: j, old, new });
            }
        }
    }
    out
}

fn vortex_tiles(token: &str, q: f64) -> Vec<StripMix> {
    vortex_pieces(token, q)
        .into_iter()
        .map(|t| StripMix {
            rect: [t.col as f64 / VORTEX_COLS as f64, t.row as f64 / VORTEX_ROWS as f64, (t.col + 1) as f64 / VORTEX_COLS as f64, (t.row + 1) as f64 / VORTEX_ROWS as f64],
            old: t.old.is_some() as u8 as f64,
            new: t.new.is_some() as u8 as f64,
        })
        .collect()
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
    match effect {
        "fade" => TransitionPlan::Fade { through_black: token == "thruBlk" },
        "cut" if token == "thruBlk" && q < 0.5 => TransitionPlan::Windows(vec![]),
        "cut" if token == "thruBlk" => TransitionPlan::BlackScreen,
        "cut" => TransitionPlan::Windows(vec![[0.0, 0.0, 1.0, 1.0]]),
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
        // さざなみ: `motion` is the waves' origin (x, y: -1 left or top, 0 the middle, 1 right or bottom); `dir` is the way the waves spread, so the diagonal ones start at the opposite corner.
        "ripple" => TransitionPlan::Turn3d { kind: Turn3dKind::Ripple, motion: ripple_origin(token) },
        "crush" => TransitionPlan::Turn3d { kind: Turn3dKind::Crush, motion: (0.0, 0.0) },
        "fracture" => TransitionPlan::Turn3d { kind: Turn3dKind::Fracture, motion: (0.0, 0.0) },
        "prestige" => TransitionPlan::Turn3d { kind: Turn3dKind::Prestige, motion: (0.0, 0.0) },
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
        "reveal" => {
            let (strips, backdrop) = reveal_mix(token.starts_with("blk-"), token.ends_with('l') || token.is_empty(), q);
            TransitionPlan::StripMix { strips, backdrop, scale: reveal_scale(q) }
        }
        "pull" => TransitionPlan::Slide { from: (0.0, 0.0), old_to: motion(token), new_on_top: false },
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
        "split" => TransitionPlan::SoftWindows(split_strips(token.starts_with("horz"), token == "in" || token == "horzIn", q)),
        "doors" | "window" => TransitionPlan::Turn3d { kind: Turn3dKind::Doors { window: effect == "window" }, motion: if token == "horz" { (1.0, 0.0) } else { (0.0, 1.0) } },
        "blinds" => TransitionPlan::Turn3d { kind: Turn3dKind::Blinds, motion: if token == "vert" { (0.0, 1.0) } else { (1.0, 0.0) } },
        "checker" => TransitionPlan::Turn3d { kind: Turn3dKind::Checker, motion: if token == "vert" { (0.0, 1.0) } else { (1.0, 0.0) } },
        "randomBar" => TransitionPlan::Windows(random_bars(token == "vert", q, aspect)),
        "comb" => TransitionPlan::Turn3d { kind: Turn3dKind::Comb, motion: if token == "vert" { (0.0, 1.0) } else { (1.0, 0.0) } },
        "circle" => TransitionPlan::SoftWindows(soft_shape(SoftShape::Circle, 0, token == "in", q, aspect)),
        "wheel" | "wheelReverse" => {
            let spokes = token.parse::<usize>().ok().filter(|n| (1..=16).contains(n)).unwrap_or(4);
            // PowerPoint's ホイール (逆回転) (`p14:wheelReverse`) draws one fan for 2, 3 and 8 spokes as it does for 1; only 4 gets four
            // (its video with every option, 2 seconds: the file keeps `spokes`, the picture has one fan; the squared error against it
            // was 0.077-0.096 with the spokes drawn, 1 and 4 already 0.001).
            let spokes = if effect == "wheelReverse" && spokes != 4 { 1 } else { spokes };
            TransitionPlan::SoftWindows(soft_shape(SoftShape::Wheel, spokes, effect == "wheelReverse", q, aspect))
        }
        // Dissolve, and the tile effects whose tiles (hexagons, diamonds, strips) are drawn in 3D or as shapes the rectangles do not
        // make: the new slide appears in squares in turn (an approximation of PowerPoint's, which shows the tiles' own shapes).
        "dissolve" => TransitionPlan::SoftWindows(dissolve_tiles(q)),
        // The side the sweep starts at, as the motion: `l` (from the right) (-1, 0), `r` (1, 0), `u` (0, -1), `d` (0, 1).
        "vortex" => TransitionPlan::Turn3d { kind: Turn3dKind::Vortex, motion: match token { "r" => (1.0, 0.0), "u" => (0.0, -1.0), "d" => (0.0, 1.0), _ => (-1.0, 0.0) } },
        "honeycomb" => TransitionPlan::Turn3d { kind: Turn3dKind::Honeycomb, motion: (0.0, 0.0) },
        // The option is `hex-<side>` or `<side>` (diamonds); the side (`l` when the file leaves it out) is where the effect starts, as the motion.
        "glitter" => TransitionPlan::Turn3d {
            kind: Turn3dKind::Glitter { diamond: !token.starts_with("hex") },
            motion: match token.chars().last() { Some('r') => (1.0, 0.0), Some('u') => (0.0, -1.0), Some('d') => (0.0, 1.0), _ => (-1.0, 0.0) },
        },
        "shred" => TransitionPlan::Turn3d { kind: Turn3dKind::Shred { particle: matches!(token, "rect-in" | "rect-out" | "rectangle" | "particle") }, motion: (0.0, 0.0) },
        "wedge" => TransitionPlan::SoftWindows(soft_shape(SoftShape::Wedge, 0, false, q, aspect)),
        "diamond" => TransitionPlan::SoftWindows(soft_shape(SoftShape::Diamond, 0, false, q, aspect)),
        "plus" => TransitionPlan::SoftWindows(soft_shape(SoftShape::Plus, 0, false, q, aspect)),
        // ストリップ: the corner the sweep starts at is where the content comes from (`ld`, moving left and down, starts at the top right).
        "strips" => TransitionPlan::SoftWindows(match token {
            "ld" => diagonal_wipe(1.0, 0.0, q),
            "rd" => diagonal_wipe(0.0, 0.0, q),
            "ru" => diagonal_wipe(0.0, 1.0, q),
            _ => diagonal_wipe(1.0, 1.0, q),
        }),
        "cube" => TransitionPlan::Turn3d { kind: Turn3dKind::Cube, motion: motion(token) },
        "flip" => TransitionPlan::Turn3d { kind: Turn3dKind::Flip, motion: motion(token) },
        "box" => TransitionPlan::Turn3d { kind: Turn3dKind::Box, motion: motion(token) },
        "rotate" => TransitionPlan::Turn3d { kind: Turn3dKind::Rotate, motion: motion(token) },
        "orbit" => TransitionPlan::Turn3d { kind: Turn3dKind::Orbit, motion: motion(token) },
        other => flat_plan(other, token, q, aspect).unwrap_or(TransitionPlan::Fade { through_black: false }),
    }
}

/// The effect and option (`Transition::direction`) that ランダム (`p:random`) shows for `seed`: ECMA-376 19.5.64 only says it picks a
/// transition at random each time, so it picks from the base schema's (`p:`) gallery, as PowerPoint 2007's random did, any option of
/// it, and never カット or another ランダム. (Which effects PowerPoint's own random draws from is to be measured on Windows.)
pub fn random_effect(seed: u64) -> (&'static str, Option<&'static str>) {
    use crate::transition_catalog::{EFFECTS, TransitionNs};
    // splitmix64: two draws from the seed.
    let mut state = seed;
    let mut draw = move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let pool: Vec<_> = EFFECTS.iter().filter(|e| e.ns == TransitionNs::P && !matches!(e.id, "cut" | "random")).collect();
    let effect = pool[(draw() % pool.len() as u64) as usize];
    let option = (!effect.options.is_empty()).then(|| effect.options[(draw() % effect.options.len() as u64) as usize].token);
    (effect.id, option)
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
        // Blinds and checker are drawn in 3D; their flat stand-ins are windows.
        assert!(matches!(plan("blinds", Some("horz"), 1.0, 1.0), TransitionPlan::Turn3d { kind: Turn3dKind::Blinds, .. }));
        let Some(TransitionPlan::Windows(w)) = flat_plan("blinds", "horz", 1.0, 1.0) else { panic!() };
        assert_eq!(w.len(), 14);
        let Some(TransitionPlan::Windows(w)) = flat_plan("blinds", "vert", 1.0, 1.0) else { panic!() };
        assert_eq!(w.len(), 18);
        let Some(TransitionPlan::Windows(w)) = flat_plan("checker", "", 1.0, 1.0) else { panic!() };
        assert_eq!(w.len(), 35);
        // The 3D effects are drawn as the flat motion nearest to them, none as a fade.
        for e in ["zoom", "flash", "flip", "cube", "box", "rotate", "orbit", "switch", "ripple", "vortex", "flythrough", "warp", "newsflash", "curtains", "drape", "wind", "fallOver", "peelOff", "pageCurlDouble", "pageCurlSingle", "airplane", "origami", "prestige", "fracture", "crush"] {
            assert!(!matches!(plan(e, None, 0.5, 1.0), TransitionPlan::Fade { .. }), "{e}");
        }
        // ズーム (video of PowerPoint for Windows): "in" shrinks the old slide in the first half, "out" grows the new one in the second.
        assert_eq!(plan("zoom", Some("in"), 0.25, 1.0), TransitionPlan::SoftScaled { old: Some(0.5), new: None, new_on_top: false });
        assert_eq!(plan("zoom", Some("in"), 0.75, 1.0), TransitionPlan::SoftScaled { old: Some(0.0), new: None, new_on_top: false });
        assert_eq!(plan("zoom", Some("out"), 0.25, 1.0), TransitionPlan::SoftScaled { old: None, new: Some(0.0), new_on_top: true });
        assert_eq!(plan("zoom", Some("out"), 0.75, 1.0), TransitionPlan::SoftScaled { old: None, new: Some(0.5), new_on_top: true });
        // ワープ (video of PowerPoint for Windows): in, the old slide flies past the camera at 1 / (1 - k), the new one grows from half size, added;
        // out, the old one shrinks to half under its own fading, over the new one coming down from large.
        let TransitionPlan::Dolly { old, new, new_on_top, additive, .. } = plan("warp", Some("in"), 0.5, 1.78) else { panic!() };
        assert!((old.0 - 2.0).abs() < 1e-12 && (old.1 - 0.5).abs() < 1e-12 && (new.0 - 1.0 / 1.5).abs() < 1e-12 && new.1 == 1.0 && !new_on_top && !additive);
        let TransitionPlan::Dolly { old, new, new_on_top, additive, .. } = plan("warp", None, 0.5, 1.78) else { panic!() };
        assert!((old.0 - 1.0 / 1.5).abs() < 1e-12 && (old.1 - 0.5).abs() < 1e-12 && (new.0 - 2.0).abs() < 1e-12 && new.1 == 1.0 && !new_on_top && !additive);
        assert!((progress_with("warp", None, 0.5).unwrap() - 0.5).abs() < 1e-6);
        // フライスルー (video of PowerPoint for Windows): in, the old slide is the near one (flies past the camera at 1 / (1 - x), fading as 1 - x), the
        // new one is the far one (half size to whole), the backdrop goes from white to black; out is in run the other way with the slides swapped.
        let TransitionPlan::Dolly { old, new, new_on_top, backdrop, .. } = plan("flythrough", Some("in"), 0.0, 1.78) else { panic!() };
        assert!(old == (1.0, 1.0) && new.1 == 0.0 && (new.0 - 1.0 / 2.2).abs() < 1e-12 && !new_on_top && backdrop == 1.0);
        let TransitionPlan::Dolly { old, new, backdrop, .. } = plan("flythrough", Some("in"), 0.5, 1.78) else { panic!() };
        assert!((old.0 - 1.0 / 0.4).abs() < 1e-12 && (old.1 - 0.4).abs() < 1e-12 && (new.0 - 1.0 / 1.6).abs() < 1e-12 && (backdrop - 0.5).abs() < 1e-12);
        // At 0.5 the weights are near 0.4, far 0.25 (kappa^2), backdrop 0.35 (the far one laid at 0.25 / 0.6 under the near one).
        assert!((new.1 - 0.25 / 0.6).abs() < 1e-12);
        let TransitionPlan::Dolly { new, new_on_top, backdrop, .. } = plan("flythrough", Some("in"), 1.0, 1.78) else { panic!() };
        assert!((new.0 - 1.0).abs() < 1e-12 && new.1 == 1.0 && !new_on_top && backdrop == 0.0);
        let TransitionPlan::Dolly { old, new, new_on_top, backdrop, .. } = plan("flythrough", Some("out"), 1.0, 1.78) else { panic!() };
        assert!((old.0 - 1.0).abs() < 1e-12 && old.1 == 1.0 && new.1 == 0.0 && new_on_top && backdrop == 0.0);
        // The bounce goes past the place of the new slide and comes back; out is in run the other way.
        let top = (0..=60).map(|f| progress_with("flythrough", Some("in-bounce"), f as f64 / 60.0).unwrap()).fold(0.0, f64::max);
        assert!(top > 1.02 && top < 1.06, "{top}");
        assert!((progress_with("flythrough", Some("in"), 0.4).unwrap() + progress_with("flythrough", Some("out"), 0.4).unwrap() - 1.0).abs() < 1e-12);
        // ニュース速報 (video of PowerPoint for Windows): the new slide grows linearly from the middle, unwinding a turn of 400 degrees at a
        // constant speed, its opacity `1 - (1 - u)^3`; the old slide is not drawn by the plan (it stands whole).
        assert_eq!(progress_with("newsflash", None, 0.3), Some(0.3));
        let TransitionPlan::Spin { scale, turn, opacity } = plan("newsflash", None, 0.25, 1.78) else { panic!() };
        assert!((scale - 0.25).abs() < 1e-12 && (turn - 300.0).abs() < 1e-9 && (opacity - (1.0 - 0.75f64.powi(3))).abs() < 1e-12);
        // 立方体 from the right (`dir="l"`): the new slide comes in from the right and pushes the old one out to the left.
        assert_eq!(plan("cube", Some("l"), 0.5, 1.0), TransitionPlan::Turn3d { kind: Turn3dKind::Cube, motion: (-1.0, 0.0) });
        assert_eq!(flat_stand_in("cube", Some("l"), 0.5, 1.0), Some(TransitionPlan::Slide { from: (1.0, 0.0), old_to: (-1.0, 0.0), new_on_top: true }));
        // Honeycomb is a plane of hexagons drawn in 3D; its flat stand-in is windows, not a fade (dissolve is tiles at their own opacities).
        assert!(matches!(plan("honeycomb", None, 0.5, 1.0), TransitionPlan::Turn3d { kind: Turn3dKind::Honeycomb, .. }));
        assert!(matches!(flat_stand_in("honeycomb", None, 0.5, 1.0), Some(TransitionPlan::Windows(_))));
        // The circle, diamond, plus, wedge and wheels have a soft band, not a hard edge.
        for (e, t) in [("circle", None), ("diamond", None), ("plus", None), ("wedge", None), ("wheel", Some("1")), ("wheel", Some("3")), ("wheel", None), ("wheel", Some("8"))] {
            assert!(matches!(plan(e, t, 0.5, 16.0 / 9.0), TransitionPlan::SoftWindows(_)), "{e} {t:?}");
        }
        assert!(matches!(plan("fade", Some("thruBlk"), 0.5, 1.0), TransitionPlan::Fade { through_black: true }));
    }

    #[test]
    fn the_fitted_3d_effects_are_cards_and_sheets_with_a_flat_stand_in() {
        for (e, kind) in [("switch", Turn3dKind::Switch), ("ferris", Turn3dKind::Ferris), ("fallOver", Turn3dKind::FallOver), ("pageCurlSingle", Turn3dKind::PageCurlSingle), ("peelOff", Turn3dKind::PeelOff), ("curtains", Turn3dKind::Curtains), ("wind", Turn3dKind::Wind), ("pageCurlDouble", Turn3dKind::PageCurlDouble), ("drape", Turn3dKind::Drape), ("airplane", Turn3dKind::Airplane), ("origami", Turn3dKind::Origami), ("crush", Turn3dKind::Crush), ("ripple", Turn3dKind::Ripple)] {
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
    fn cover_and_diagonal_uncover_run_on_their_own_curves() {
        // 2 s = 60 frames: the diagonal uncover is done by frame 31 (measured 29.5 frames from 1.4), the straight cover by frame 47.
        assert!((progress_with("pull", Some("ld"), 0.55).unwrap() - 1.0).abs() < 1e-9);
        assert!(progress_with("pull", Some("l"), 0.55).unwrap() < 0.8);
        assert!((progress_with("cover", Some("l"), 0.8).unwrap() - 1.0).abs() < 1e-9);
        // The diagonal cover has covered about half (0.50 measured at frame 12 of the diagonal pair) at 0.2 of its time.
        let half = progress_with("cover", Some("ld"), 0.2).unwrap();
        assert!((0.4..0.6).contains(&half), "{half}");
    }

    #[test]
    fn easing_matches_the_measured_curves() {
        assert!((bezier_ease(0.72, 0.36, 0.0)).abs() < 1e-9 && (bezier_ease(0.72, 0.36, 1.0) - 1.0).abs() < 1e-9);
        // Uncover at half time is a little under half way (measured 0.41 on PowerPoint, 1 second).
        let half = progress("pull", 0.5).unwrap();
        assert!((0.38..0.45).contains(&half), "{half}");
        assert!((progress("push", 0.5).unwrap() - 0.5).abs() < 0.02);
        assert_eq!(progress("wipe", 0.3), Some(0.3));
        assert_eq!(progress("fade", 0.3), Some(0.3));
    }

    #[test]
    fn prestige_is_a_cloth_on_its_own_linear_time() {
        assert_eq!(progress("prestige", 0.3), Some(0.3));
        assert!(matches!(plan("prestige", None, 0.5, 1.78), TransitionPlan::Turn3d { kind: Turn3dKind::Prestige, .. }));
    }

    #[test]
    fn a_wipe_edge_is_a_cosine_as_wide_as_the_slide() {
        assert_eq!(wipe_opacity(0.0, 0.0), 0.0);
        assert_eq!(wipe_opacity(1.0, 1.0), 1.0);
        // The middle of the edge is the start edge's distance 2t - 0.5 away.
        assert!((wipe_opacity(0.5, 0.5) - 0.5).abs() < 1e-9);
        assert!((wipe_opacity(0.25, 0.0) - 0.5).abs() < 1e-9 && (wipe_opacity(0.75, 1.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn split_is_a_soft_wipe_from_the_middle_or_from_the_edges() {
        let alpha_at = |token: &str, x: f64| match plan("split", Some(token), 0.5, 1.78) {
            TransitionPlan::SoftWindows(w) => w.iter().find(|(r, _)| r[0] <= x && x < r[2]).map_or(0.0, |w| w.1),
            other => panic!("{other:?}"),
        };
        // Half way, the middle is the new slide, the edge about a seventh of it (measured: 0.15); the other way round for the inward one.
        assert!(alpha_at("out", 0.5) > 0.99 && alpha_at("out", 0.01) < 0.25, "{} {}", alpha_at("out", 0.5), alpha_at("out", 0.01));
        assert!(alpha_at("in", 0.01) > 0.99 && alpha_at("in", 0.5) < 0.25);
        match plan("split", Some("horzOut"), 0.5, 1.78) {
            TransitionPlan::SoftWindows(w) => assert!(w.iter().all(|(r, _)| r[0] == 0.0 && r[2] == 1.0), "strips across the height"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn strips_sweep_from_the_corner_the_content_comes_from() {
        let alpha_at = |token: &str, x: f64, y: f64| match plan("strips", Some(token), 0.4, 1.78) {
            TransitionPlan::SoftWindows(w) => w.iter().find(|(r, _)| r[0] <= x && x < r[2] && r[1] <= y && y < r[3]).map_or(0.0, |w| w.1),
            other => panic!("{other:?}"),
        };
        assert!(alpha_at("ld", 0.95, 0.05) > alpha_at("ld", 0.05, 0.95) + 0.5);
        assert!(alpha_at("rd", 0.05, 0.05) > alpha_at("rd", 0.95, 0.95) + 0.5);
        assert!(alpha_at("ru", 0.05, 0.95) > alpha_at("ru", 0.95, 0.05) + 0.5);
        assert!(alpha_at("lu", 0.95, 0.95) > alpha_at("lu", 0.05, 0.05) + 0.5);
    }

    /// リビール: the old slide grows to the middle of the time and the new one shrinks to whole from it.
    #[test]
    fn reveal_zooms_the_old_slide_up_and_the_new_one_down() {
        assert_eq!(reveal_scale(0.0), (1.0, 1.0 + REVEAL_ZOOM));
        assert!((reveal_scale(0.25).0 - (1.0 + REVEAL_ZOOM / 2.0)).abs() < 1e-12);
        assert!((reveal_scale(0.5).0 - (1.0 + REVEAL_ZOOM)).abs() < 1e-12 && (reveal_scale(0.5).1 - (1.0 + REVEAL_ZOOM)).abs() < 1e-12);
        assert!((reveal_scale(0.75).1 - (1.0 + REVEAL_ZOOM / 2.0)).abs() < 1e-12);
        assert_eq!(reveal_scale(1.0), (1.0 + REVEAL_ZOOM, 1.0));
    }

    /// ズーム out: the new slide keeps its size, each piece of the hole shows the part of the slide it lies over.
    #[test]
    fn the_hole_of_zoom_out_shows_the_slide_where_it_lies() {
        let pieces = soft_masked_pieces(0.5);
        assert!(pieces.iter().all(|(r, uv, _)| (r[0] - uv[0]).abs() < 1e-12 && (r[1] - uv[1]).abs() < 1e-12 && (r[2] - uv[2]).abs() < 1e-12 && (r[3] - uv[3]).abs() < 1e-12));
        let alphas: Vec<f64> = soft_scaled_pieces(0.5).iter().map(|p| p.2).collect();
        assert_eq!(alphas, pieces.iter().map(|p| p.2).collect::<Vec<_>>());
    }

    /// ズーム's slide has a soft edge: whole inside, half at its edge, nothing past a half band outside, the picture held to the slide.
    #[test]
    fn a_zoomed_slide_has_a_soft_edge_centred_on_its_edge() {
        let pieces = soft_scaled_pieces(0.5);
        let alpha_at = |x: f64, y: f64| pieces.iter().find(|(r, _, _)| r[0] <= x && x < r[2] && r[1] <= y && y < r[3]).map_or(0.0, |p| p.2);
        assert!(alpha_at(0.5, 0.5) > 0.999, "inside");
        // The left edge is at 0.25: a half of the band's cosine at the edge, little a half band outside, whole a half band inside.
        let e = ZOOM_EDGE;
        assert!((alpha_at(0.25 + 0.01 * e, 0.5) - 0.5).abs() < 0.1, "{}", alpha_at(0.25 + 0.01 * e, 0.5));
        assert!(alpha_at(0.25 - 0.6 * e, 0.5) == 0.0);
        assert!(alpha_at(0.25 + 0.6 * e, 0.5) > 0.99);
        assert!(pieces.iter().all(|(_, uv, a)| *a > 0.0 && *a <= 1.0 && uv.iter().all(|v| (0.0..=1.0).contains(v)) && uv[2] > uv[0] && uv[3] > uv[1]));
        // Nothing of a slide that is not there; the whole screen's worth when it fills it (its edge on the border).
        assert!(soft_scaled_pieces(0.0).is_empty());
        assert!(soft_scaled_pieces(1.0).len() > 4);
    }

    #[test]
    fn dissolve_tiles_appear_by_the_measured_share() {
        let share = |t: f64| match plan("dissolve", None, t, 1.78) {
            TransitionPlan::SoftWindows(w) => w.iter().filter(|(_, a)| *a > 0.5).count() as f64 / (54.0 * 42.0),
            other => panic!("{other:?}"),
        };
        for (t, want) in [(10.0 / 60.0, 0.067), (30.0 / 60.0, 0.507), (50.0 / 60.0, 0.917)] {
            assert!((share(t) - want).abs() < 0.03, "{t}: {} against {want}", share(t));
        }
        assert!(share(0.0) < 0.001 && share(1.0) > 0.999);
    }

    #[test]
    fn wheel_reverse_with_2_3_or_8_spokes_is_drawn_as_one_fan() {
        for n in ["2", "3", "8"] {
            assert_eq!(plan("wheelReverse", Some(n), 0.4, 1.78), plan("wheelReverse", Some("1"), 0.4, 1.78), "{n}");
        }
        assert_ne!(plan("wheelReverse", Some("4"), 0.4, 1.78), plan("wheelReverse", Some("1"), 0.4, 1.78));
        assert_ne!(plan("wheel", Some("2"), 0.4, 1.78), plan("wheel", Some("1"), 0.4, 1.78));
    }

    #[test]
    fn cut_through_black_is_a_black_screen_for_its_length() {
        assert_eq!(plan("cut", Some("thruBlk"), 0.5, 1.0), TransitionPlan::BlackScreen);
        assert_ne!(plan("cut", None, 0.5, 1.0), TransitionPlan::BlackScreen);
        // The first half shows the old slide (no window of the new one).
        assert_eq!(plan("cut", Some("thruBlk"), 0.4, 1.0), TransitionPlan::Windows(vec![]));
    }

    #[test]
    fn fade_through_black_goes_to_black_and_comes_back_by_halves() {
        // The old slide goes out over the first half (opacity 1 to 0), the new one in over the second (0 to 1).
        assert_eq!(fade_look(true, 0.0), (false, 1.0));
        let (new, a) = fade_look(true, 0.5 - 1e-9);
        assert!(!new && a < 0.001);
        let (new, a) = fade_look(true, 0.5);
        assert!(new && a < 0.001);
        assert_eq!(fade_look(true, 1.0), (true, 1.0));
        // A quarter through (half way down) the old slide is at half its brightness; measured the same on PowerPoint.
        assert!((fade_look(true, 0.25).1 - 0.5).abs() < 0.01);
        // The picture has a black frame: sampling the 60 frames of 2 seconds, the darkest is below 2% of full.
        let darkest = (0..=60).map(|f| fade_look(true, f as f64 / 60.0).1).fold(1.0, f64::min);
        assert!(darkest < 0.02, "{darkest}");
        assert!((fade_look(false, 0.5).1 - 0.5).abs() < 0.01);
    }

    /// ランダム ストライプ of a slide transition: 640 bars of 2 px across a 16:9 slide, each switching once between 0.15 and 0.65 of the time, the
    /// share switched by then following a triangle's distribution (its median at 0.4).
    #[test]
    fn transition_random_bars_switch_in_a_triangle_of_times() {
        let share = |vertical: bool, q: f64| match plan("randomBar", Some(if vertical { "vert" } else { "horz" }), q, 16.0 / 9.0) {
            TransitionPlan::Windows(w) => w.iter().map(|r| (r[2] - r[0]) * (r[3] - r[1])).sum::<f64>(),
            other => panic!("{other:?}"),
        };
        for vertical in [true, false] {
            assert!(share(vertical, 0.14) < 1e-9 && share(vertical, 0.66) > 1.0 - 1e-9);
            assert!((share(vertical, 0.4) - 0.5).abs() < 0.06 && (share(vertical, 0.275) - 0.125).abs() < 0.06 && (share(vertical, 0.525) - 0.875).abs() < 0.06);
            // Never back: more of the slide is the new one the later it is.
            assert!(share(vertical, 0.35) <= share(vertical, 0.36) && share(vertical, 0.44) <= share(vertical, 0.45));
        }
    }

    #[test]
    fn soft_shapes_have_an_alpha_band_not_a_hard_edge() {
        let a = 16.0 / 9.0;
        for (shape, spokes) in [(SoftShape::Circle, 0), (SoftShape::Diamond, 0), (SoftShape::Plus, 0), (SoftShape::Wedge, 0), (SoftShape::Wheel, 1), (SoftShape::Wheel, 4)] {
            // At the start nothing is new, at the end all is.
            let share = |q: f64| -> Vec<f64> { (0..40).flat_map(|r| (0..70).map(move |c| (c, r))).map(|(c, r)| soft_shape_alpha(shape, spokes, false, q, (c as f64 + 0.5) / 70.0, (r as f64 + 0.5) / 40.0, a)).collect() };
            assert!(share(0.0).iter().all(|&v| v < 1e-9), "{shape:?} start");
            assert!(share(1.0).iter().all(|&v| v > 1.0 - 1e-9), "{shape:?} end");
            // Half way: a good part of the slide is strictly between old and new (a band), more than a hard edge's single sample row.
            let mid = share(0.5);
            let between = mid.iter().filter(|&&v| v > 0.05 && v < 0.95).count();
            assert!(between as f64 > 0.03 * mid.len() as f64, "{shape:?} {between}");
        }
        // The circle's band is a third of the corner distance wide (10% to 90% of a half cosine is 0.59 of it).
        let big = 0.5 * (a as f64).hypot(1.0);
        let q = 0.5;
        let c = -big / 6.0 + (big + big / 3.0) * q;
        let at = |d: f64| soft_shape_alpha(SoftShape::Circle, 0, false, q, 0.5 + (c + d) / a, 0.5, a);
        assert!((at(0.0) - 0.5).abs() < 1e-6);
        assert!(at(-big / 6.0) > 0.999 && at(big / 6.0) < 0.001);
        // The windows reproduce the band: the layers at several opacities give the share to within one level.
        let w = soft_shape(SoftShape::Circle, 0, false, q, a);
        assert!(w.iter().any(|(_, o)| *o > 0.1 && *o < 0.99), "partial opacities");
        assert!(w.len() < 700, "{}", w.len());
    }

    #[test]
    fn random_picks_a_real_effect_with_one_of_its_options() {
        use std::collections::HashSet;
        let mut seen = HashSet::new();
        for seed in 0..400u64 {
            let (id, token) = random_effect(seed);
            let effect = crate::transition_catalog::effect_of(id).expect("an effect of the gallery");
            assert!(!matches!(id, "cut" | "random"), "{id}");
            assert_eq!(token.is_none(), effect.options.is_empty(), "{id}");
            assert!(token.is_none_or(|t| effect.options.iter().any(|o| o.token == t)), "{id} {token:?}");
            // The picked one is played by a plan of its own, not the stand-in fade of an effect the plan does not know.
            let _ = plan(id, token, 0.5, 16.0 / 9.0);
            assert_eq!(random_effect(seed), (id, token), "the same seed picks the same");
            seen.insert(id);
        }
        assert!(seen.len() >= 12, "the draw reaches many effects: {seen:?}");
    }

    #[test]
    fn vortex_throws_the_old_tiles_from_the_side_the_option_names_and_brings_the_new_back_over_black() {
        let share = |token: &str, q: f64, x_lo: f64, x_hi: f64| {
            let strips = vortex_tiles(token, q);
            let in_band = |m: &&StripMix| (x_lo..x_hi).contains(&((m.rect[0] + m.rect[2]) / 2.0));
            let n = 128.0 * 100.0 * (x_hi - x_lo);
            (strips.iter().filter(in_band).filter(|m| m.old > 0.0).count() as f64 / n, strips.iter().filter(in_band).filter(|m| m.new > 0.0).count() as f64 / n)
        };
        // At the start the old slide is whole and nothing of the new one is there; at the end the other way round; in the middle little is left of either (about a tenth each: the picture is mostly black).
        let (o0, n0) = share("l", 0.0, 0.0, 1.0);
        let (o1, n1) = share("l", 1.0, 0.0, 1.0);
        let (om, nm) = share("l", 0.5, 0.0, 1.0);
        assert!(o0 > 0.99 && n0 < 0.01 && o1 < 0.01 && n1 > 0.99 && om < 0.2 && nm < 0.2 && om + nm < 0.3, "{o0} {n0} {o1} {n1} {om} {nm}");
        // `l` starts at the right: a fifth of the way in (the front has passed the right fifth, not the left one) the right is thinner.
        let (right, _) = share("l", 0.12, 0.8, 1.0);
        let (left, _) = share("l", 0.12, 0.0, 0.2);
        assert!(right < 0.7 && left > 0.97, "{right} {left}");
        // `r` is the mirror image.
        let (right_r, _) = share("r", 0.12, 0.8, 1.0);
        assert!(right_r > 0.97);
    }
}
