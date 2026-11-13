// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The 3D transitions drawn with GPUI's offscreen textures (`Window::paint_offscreen`, `paint_textured_quad`): 立方体 (cube) and
//! 反転 (flip). Each slide is painted to a texture and drawn as a card placed in 3D by a 4x4 transform.
//!
//! The geometry is fitted to PowerPoint for Windows (Microsoft 365 16.0.20527): a slide show of 4-colour test slides with
//! each effect exported to video (`Presentation.CreateVideo`, 1280 x 720, 30 frames a second, 2 s transitions; the
//! frames of `target/fbo3d/trans3d.mp4`), then, for each frame, the two cards rendered by the model below and compared with
//! the frame (blurred squared difference), the angle and, for 立方体, the camera's distance fitted per frame.
//!
//! * 立方体 (`cube`): the two slides are faces of a cube whose side is the slide's size along the turn (the pivot is half that
//!   behind the slide's plane, `c`); the cube turns about its vertical (or horizontal) axis and is seen from outside by a
//!   camera of focal length `F` (the scale of the plane z = 0 at rest) whose distance from the pivot's plane is
//!   `F (1 + K sin 2a)` (`a` the angle turned: the camera backs off a little in the middle of the turn). Fitted to each
//!   direction's frames (26 of the 78 around each transition, the angle free for every frame): left and right `F` = 900 and 905
//!   px of a 640 px slide (so about the slide's diagonal over the width, sqrt 2), `K` = 0.169 and 0.169, the pivot `c` = -322.6
//!   (-W/2); up `F` = 1399, `K` = 0.089, `c` = -182.8 (-H/2); down the same with the other sign. The picture's error, with
//!   the constants below: the mean squared difference of the blurred pictures 0.025, 0.025, 0.027 and 0.020 per pixel for left,
//!   right, up and down (the colours and the edges are 0 to 1; a wrong model, for example the camera inside the cube, 0.3).
//! * 反転 (`flip`): the slide turns about its own vertical middle (`c` = 0) in perspective `F` = 660 px (a 640 px slide); the
//!   new slide is the card's back.
//! * Shading: a face turned `a` from the viewer is darker by `1 - 0.24 smoothstep(0.4, 1.5, |a|)` (see `DARKEST`).
//! * Time: the angle follows the transition's `cubic-bezier` (see `transition_plan::progress`): 立方体
//!   `(0.326, 0, 0.691, 1)` (rms 0.0023 of the angle), 反転 `(0.593, 0, 0.358, 1)` (0.0087).

use gpui_kit::{MeshVertex, Transform3D};
use std::sync::Arc;

/// 立方体's camera for a turn about the vertical axis: the focal length is a multiple of the slide's HEIGHT that depends on the slide's
/// shape (fitted on 16:9 slides: 905 px of 360 = 2.51 H, and on 4:3 slides 1323 px of 480 = 2.76 H; the same width, 640 px) and
/// the share it backs off by (0.169 and 0.122), each interpolated by the aspect ratio. Four 4:3 and 16:9 videos, 26 frames each:
/// 4:3 mean error 0.029 (blurred squared difference, 0 to 1), 16:9 0.025.
pub(crate) fn cube_horizontal_camera(w: f32, h: f32) -> (f32, f32) {
    let aspect = w / h;
    let focal = (2.51 - 0.562 * (aspect - 1.778)).clamp(2.0, 3.2) * h;
    let back_off = (0.169 + 0.1056 * (aspect - 1.778)).clamp(0.05, 0.25);
    (focal, back_off)
}
/// The same for a turn about the horizontal axis: the focal length over the slide's WIDTH (fitted 1399 px of 640 on 16:9 and 1444 on
/// 4:3, so it does not follow the height: 2.22) and the share it backs off by (0.089 and 0.087: 0.088).
pub(crate) const CUBE_FOCAL_VERTICAL: f32 = 2.22;
pub(crate) const CUBE_BACK_OFF_VERTICAL: f32 = 0.088;
/// 箱 (`box`, the cube seen from inside; the pivot is in front of the slide's plane): horizontal turns, over the slide's width:
/// the focal length (fitted 613, 608 and, with the white background of 軌道, 489 of 640 px: 550), the camera's back off (0.637,
/// 0.641 and 0.627: 0.635) and the pivot's distance in front of the plane (371 of 640 px, 0.58).
pub(crate) const BOX_FOCAL_HORIZONTAL: f32 = 0.86;
pub(crate) const BOX_BACK_OFF_HORIZONTAL: f32 = 0.635;
pub(crate) const BOX_PIVOT_HORIZONTAL: f32 = 0.58;
/// The same for turns about the horizontal axis: the focal length over the slide's width (1023 and 981 of 640: 1000), the back off (0.241,
/// 0.238) and the pivot over the slide's height (202 and 201 of 360 px: 0.56).
pub(crate) const BOX_FOCAL_VERTICAL: f32 = 1.5625;
pub(crate) const BOX_BACK_OFF_VERTICAL: f32 = 0.24;
pub(crate) const BOX_PIVOT_VERTICAL: f32 = 0.56;
/// 反転's focal length over the slide's width (fitted 660 of 640 px).
pub(crate) const FLIP_FOCAL: f32 = 1.031;
/// The brightness of a face turned `a` from the viewer: `1 - DARKEST smoothstep(0.4, 1.5, |a|)`. Measured on the video frames of
/// 反転 (the brightness of the slides' white, 0.99 at 0.47 rad, 0.94 at 0.89, 0.85 at 1.07, 0.79 at 1.27, 0.77 at 1.47: 27
/// frames) and of 立方体 (the colours of the faces are not darkened up to 1 rad; the nearly edge-on face is 0.74).
pub(crate) const DARKEST: f32 = 0.24;

/// The four effects of the cube family (PowerPoint's `p14:prism`): the cube seen from outside (立方体) or from inside (箱), with
/// the slides as they are or with their contents alone turning over the slide's own background (回転 and 軌道).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Prism {
    Cube,
    Box,
    Rotate,
    Orbit,
}

impl Prism {
    /// Seen from inside the cube.
    pub(crate) fn inside(self) -> bool {
        matches!(self, Prism::Box | Prism::Orbit)
    }

    /// Only the slides' shapes turn; their background stays.
    pub(crate) fn content_only(self) -> bool {
        matches!(self, Prism::Rotate | Prism::Orbit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Face {
    Old,
    New,
}

/// The card of one slide: the transform placing the slide (at its own size, top left at 0, 0, on the plane z = 0), whether
/// the face turned away is left out, and its brightness.
#[derive(Clone, Debug)]
pub(crate) struct Card {
    pub face: Face,
    pub transform: Transform3D,
    pub cull_back: bool,
    pub shade: f32,
    /// A sheet that is not flat (a curled or bent page): its vertices (x, y, z in the slide's own pixels, the slide's top left at 0, 0)
    /// and texture points, and the triangles; the transform places it as it places a flat card. `None`: the flat card.
    pub mesh: Option<Mesh>,
    /// The share of white laid over the card (the paper's back is paler than its front: see `BACK_WHITE`); 0 for none.
    pub white: f32,
}

/// A triangle mesh of a sheet that is not flat: the vertices (shared by the several draws of one sheet, each with the triangles it
/// shows) and the triangles' corners.
#[derive(Clone, Debug)]
pub(crate) struct Mesh {
    pub vertices: Arc<Vec<MeshVertex>>,
    pub indices: Vec<u32>,
}

impl Card {
    /// A flat card.
    pub(crate) fn flat(face: Face, transform: Transform3D, cull_back: bool, shade: f32) -> Self {
        Self { face, transform, cull_back, shade, mesh: None, white: 0.0 }
    }
}

/// The way a turn is made: about the vertical axis (the slide moves sideways) or the horizontal one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    Vertical,
    Horizontal,
}

/// The axis and the sign of the turn for the option's motion (`motion`: the way the content moves, as `transition_plan`),
/// as fitted: left `-1` and right `+1` about the vertical axis, up `+1` and down `-1` about the horizontal one.
fn axis_of(motion: (f64, f64)) -> (Axis, f32) {
    if motion.0.abs() >= motion.1.abs() && motion.0 != 0.0 {
        (Axis::Vertical, if motion.0 < 0.0 { -1.0 } else { 1.0 })
    } else if motion.1 != 0.0 {
        (Axis::Horizontal, if motion.1 < 0.0 { 1.0 } else { -1.0 })
    } else {
        (Axis::Vertical, -1.0)
    }
}

fn turn(axis: Axis, radians: f32) -> Transform3D {
    match axis {
        Axis::Vertical => Transform3D::rotate_y(radians),
        Axis::Horizontal => Transform3D::rotate_x(radians),
    }
}

fn shade(angle: f32) -> f32 {
    let t = ((angle.abs() - 0.4) / (1.5 - 0.4)).clamp(0.0, 1.0);
    1.0 - DARKEST * t * t * (3.0 - 2.0 * t)
}

/// A card turned `angle` about the axis through (`w` / 2, `h` / 2, `pivot_z`) seen by a camera of focal length `focal` at
/// `eye` from the plane z = 0 (`eye` = `focal` at rest).
fn card(face: Face, axis: Axis, sign: f32, angle: f32, pivot_z: f32, focal: f32, eye: f32, w: f32, h: f32, cull_back: bool) -> Card {
    Card::flat(
        face,
        Transform3D::translate(-w / 2.0, -h / 2.0, 0.0)
            .then(Transform3D::translate(0.0, 0.0, -pivot_z))
            .then(turn(axis, sign * angle))
            .then(Transform3D::translate(0.0, 0.0, pivot_z))
            .then(Transform3D::translate(0.0, 0.0, focal - eye))
            .then(Transform3D::perspective(focal))
            .then(Transform3D::translate(w / 2.0, h / 2.0, 0.0)),
        cull_back,
        shade(angle),
    )
}

/// The cube family for the angle turned `angle` (0 to a quarter turn) on a slide of `w` x `h` logical pixels: the old slide and the new
/// one.
pub(crate) fn prism_at(kind: Prism, motion: (f64, f64), angle: f32, w: f32, h: f32) -> [Card; 2] {
    let (axis, mut sign) = axis_of(motion);
    let (side, focal, back_off, pivot_z) = match (kind.inside(), axis) {
        (false, Axis::Vertical) => {
            let (focal, back_off) = cube_horizontal_camera(w, h);
            (w, focal, back_off, -w / 2.0)
        }
        (false, Axis::Horizontal) => (h, CUBE_FOCAL_VERTICAL * w, CUBE_BACK_OFF_VERTICAL, -h / 2.0),
        (true, Axis::Vertical) => (w, BOX_FOCAL_HORIZONTAL * w, BOX_BACK_OFF_HORIZONTAL, BOX_PIVOT_HORIZONTAL * w),
        (true, Axis::Horizontal) => (h, BOX_FOCAL_VERTICAL * w, BOX_BACK_OFF_VERTICAL, BOX_PIVOT_VERTICAL * h),
    };
    let _ = side;
    // Seen from inside the turn is the other way.
    if kind.inside() {
        sign = -sign;
    }
    let eye = focal * (1.0 + back_off * (2.0 * angle).sin());
    [
        card(Face::Old, axis, sign, angle, pivot_z, focal, eye, w, h, false),
        card(Face::New, axis, sign, angle - std::f32::consts::FRAC_PI_2, pivot_z, focal, eye, w, h, false),
    ]
}

/// The cube family at `q` (0 to 1 of the turn).
pub(crate) fn prism(kind: Prism, motion: (f64, f64), q: f32, w: f32, h: f32) -> [Card; 2] {
    prism_at(kind, motion, q.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2, w, h)
}

/// 立方体 for the angle turned `angle`.
#[cfg(test)]
pub(crate) fn cube_at(motion: (f64, f64), angle: f32, w: f32, h: f32) -> [Card; 2] {
    prism_at(Prism::Cube, motion, angle, w, h)
}

/// 立方体 at `q` (0 to 1 of the turn).
#[cfg(test)]
pub(crate) fn cube(motion: (f64, f64), q: f32, w: f32, h: f32) -> [Card; 2] {
    prism(Prism::Cube, motion, q, w, h)
}

/// 反転 for the angle turned `angle` (0 to a half turn): the old slide turns to edge-on at a quarter, then the new one (on its
/// back) turns to face.
pub(crate) fn flip_at(motion: (f64, f64), angle: f32, w: f32, h: f32) -> [Card; 2] {
    let (axis, sign) = axis_of(motion);
    let focal = FLIP_FOCAL * w;
    [
        card(Face::Old, axis, sign, angle, 0.0, focal, focal, w, h, true),
        card(Face::New, axis, sign, angle - std::f32::consts::PI, 0.0, focal, focal, w, h, true),
    ]
}

/// 反転 at `q` (0 to 1 of the turn).
pub(crate) fn flip(motion: (f64, f64), q: f32, w: f32, h: f32) -> [Card; 2] {
    flip_at(motion, q.clamp(0.0, 1.0) * std::f32::consts::PI, w, h)
}

/// コンベヤー (`conveyor`): the cards of the two slides ride a belt. Not a rotation of one rigid body but a path of each card: the old
/// card backs off and turns a little (-0.38 rad about the vertical axis), both slide to the side, and the new one comes forward and
/// turns back. Fitted per frame (the card's shift along x and z in px of a 640 px slide and its turn about the vertical axis, the
/// camera's focal length fixed at 1000 px, NOT fitted) on the frames of the left variant, 76 frames in steps of 2 (the 24 frames of
/// each card that show it; the per-frame mean squared difference of the pictures 0.0003 to 0.006 on a white background, with
/// five frames of the old card's leaving and of the new card's arrival unreliable and left out); the right variant is its mirror
/// (the shift and the turn change sign). The table is in video frames (a transition begins at frame 12 and lasts 60 frames); the
/// time is linear (`transition_plan::progress`).
const CONVEYOR_FOCAL: f32 = 1000.0 / 640.0;
/// (frame, shift x, shift z, turn)
const CONVEYOR_OLD: &[(f32, f32, f32, f32)] = &[
    (12.0, 0.0, 0.0, 0.0),
    (14.0, 0.0, -21.0, -0.08),
    (16.0, 0.3, -35.0, -0.12),
    (20.0, 4.7, -58.0, -0.20),
    (24.0, 10.5, -85.0, -0.27),
    (28.0, 17.8, -106.0, -0.32),
    (32.0, 14.0, -107.0, -0.38),
    (34.0, -12.0, -127.0, -0.38),
    (36.0, -62.0, -143.0, -0.37),
    (38.0, -142.0, -190.0, -0.38),
    (40.0, -241.0, -241.0, -0.32),
    (42.0, -357.0, -298.0, -0.36),
    (44.0, -450.0, -330.0, -0.36),
    (46.0, -600.0, -360.0, -0.36),
    (50.0, -900.0, -400.0, -0.36),
];
const CONVEYOR_NEW: &[(f32, f32, f32, f32)] = &[
    (38.0, 700.0, 0.0, -0.1),
    (40.0, 434.0, 25.0, -0.25),
    (42.0, 318.0, -1.0, -0.25),
    (44.0, 212.0, -36.0, -0.32),
    (46.0, 125.0, -74.0, -0.37),
    (48.0, 68.0, -101.0, -0.37),
    (50.0, 36.0, -99.5, -0.37),
    (52.0, 24.0, -107.0, -0.38),
    (54.0, 19.0, -98.0, -0.36),
    (56.0, 12.0, -91.0, -0.32),
    (58.0, 9.7, -81.5, -0.28),
    (60.0, 7.5, -62.0, -0.26),
    (62.0, 5.5, -59.0, -0.22),
    (64.0, 3.0, -49.0, -0.17),
    (66.0, 2.7, -38.0, -0.12),
    (68.0, 0.6, -25.0, -0.09),
    (70.0, 0.5, -11.5, -0.04),
    (72.0, 0.0, 0.0, 0.0),
];

/// The track's shift and turn at the video frame `frame`: `None` outside the table at the side where the card is out of the picture.
fn track_at(track: &[(f32, f32, f32, f32)], frame: f32) -> Option<(f32, f32, f32)> {
    let (first, last) = (track.first()?, track.last()?);
    if frame < first.0 || frame > last.0 {
        return None;
    }
    let i = track.windows(2).position(|w| frame <= w[1].0).unwrap_or(track.len() - 2);
    let (a, b) = (track[i], track[i + 1]);
    let u = if b.0 > a.0 { (frame - a.0) / (b.0 - a.0) } else { 0.0 };
    Some((a.1 + (b.1 - a.1) * u, a.2 + (b.2 - a.2) * u, a.3 + (b.3 - a.3) * u))
}

/// コンベヤー at `t` (0 to 1, linear) for the content moving by `motion` (left: the table as it is; right: its mirror): the cards that
/// are in the picture (the old card before it leaves, the new card from when it arrives).
pub(crate) fn conveyor(motion: (f64, f64), t: f32, w: f32, h: f32) -> Vec<Card> {
    let mirror = if motion.0 > 0.0 { -1.0 } else { 1.0 };
    let frame = 12.0 + t.clamp(0.0, 1.0) * 60.0;
    let focal = CONVEYOR_FOCAL * w;
    let mut out = Vec::new();
    for (face, track) in [(Face::Old, CONVEYOR_OLD), (Face::New, CONVEYOR_NEW)] {
        let Some((tx, tz, turn)) = track_at(track, frame) else { continue };
        let (tx, turn) = (tx * w / 640.0 * mirror, turn * mirror);
        out.push(Card::flat(
            face,
            Transform3D::translate(-w / 2.0, -h / 2.0, 0.0)
                .then(Transform3D::rotate_y(turn))
                .then(Transform3D::translate(tx, 0.0, tz * w / 640.0))
                .then(Transform3D::perspective(focal))
                .then(Transform3D::translate(w / 2.0, h / 2.0, 0.0)),
            false,
            shade(turn),
        ));
    }
    out
}

/// ギャラリー (`gallery`): the two slides are two cards side by side in one strip (no gap) that backs off, tilts, scrolls by one slide
/// and comes back. Fitted per frame for the left variant (the strip's scroll in px of a 640 px slide, its turn about the vertical
/// axis, its depth and its height shift; the camera's focal length 800 px: 600, 700, 800, 1000 and 1400 gave the totals 0.568,
/// 0.559, 0.551, 0.579 and 0.623 over 26 frames, and a gap between the cards of 40 and 60 px was worse: 0.78 and 2.66), the mean
/// squared difference of the pictures per frame 0.0004 to 0.07 (0.021 on average; the flat stand-in 0.28). Time is linear (video frames
/// 12 to 72); the right variant is the mirror. The floor's reflection under the cards is not drawn.
const GALLERY_FOCAL: f32 = 800.0 / 640.0;
/// (frame, scroll, turn, shift z, shift y)
const GALLERY: &[(f32, f32, f32, f32, f32)] = &[
    (12.0, 0.0, 0.0, -9.0, 0.0),
    (15.0, -1.9, -0.06, -34.0, 0.75),
    (18.0, -4.4, -0.13, -63.0, 2.3),
    (21.0, -6.7, -0.21, -84.0, 2.6),
    (24.0, -14.8, -0.26, -110.0, 2.9),
    (27.0, -22.8, -0.32, -138.0, 2.9),
    (30.0, -33.7, -0.38, -162.0, 4.5),
    (33.0, -14.0, -0.38, -164.0, 4.5),
    (36.0, 60.6, -0.37, -165.0, 3.9),
    (39.0, 187.0, -0.38, -167.0, 4.4),
    (42.0, 302.0, -0.35, -165.0, 4.0),
    (45.0, 449.0, -0.31, -169.0, 3.9),
    (48.0, 549.0, -0.38, -172.6, 3.2),
    (51.0, 600.0, -0.38, -172.0, 3.3),
    (54.0, 609.0, -0.36, -158.0, 3.2),
    (57.0, 619.0, -0.30, -133.0, 2.3),
    (60.0, 628.0, -0.24, -101.0, 2.4),
    (63.0, 634.0, -0.18, -77.0, 2.1),
    (66.0, 637.0, -0.11, -49.0, 1.1),
    (69.0, 638.0, -0.03, -24.0, 0.9),
    (72.0, 640.0, 0.0, 0.0, 0.0),
];

/// ギャラリー at `t` (0 to 1, linear): both cards of the strip.
pub(crate) fn gallery(motion: (f64, f64), t: f32, w: f32, h: f32) -> Vec<Card> {
    let mirror = if motion.0 > 0.0 { -1.0 } else { 1.0 };
    let frame = 12.0 + t.clamp(0.0, 1.0) * 60.0;
    let i = GALLERY.windows(2).position(|p| frame <= p[1].0).unwrap_or(GALLERY.len() - 2);
    let (a, b) = (GALLERY[i], GALLERY[i + 1]);
    let u = if b.0 > a.0 { ((frame - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 0.0 };
    let lerp = |x: f32, y: f32| x + (y - x) * u;
    let (scroll, turn, depth, shift) = (lerp(a.1, b.1), lerp(a.2, b.2), lerp(a.3, b.3), lerp(a.4, b.4));
    let k = w / 640.0;
    let focal = GALLERY_FOCAL * w;
    let (scroll, turn) = (scroll * k * mirror, turn * mirror);
    [(Face::Old, 0.0), (Face::New, mirror * w)]
        .into_iter()
        .map(|(face, offset)| {
            Card::flat(
                face,
                Transform3D::translate(offset - scroll - w / 2.0, -h / 2.0, 0.0)
                    .then(Transform3D::rotate_y(turn))
                    .then(Transform3D::translate(0.0, shift * k, depth * k))
                    .then(Transform3D::perspective(focal))
                    .then(Transform3D::translate(w / 2.0, h / 2.0, 0.0)),
                false,
                shade(turn),
            )
        })
        .collect()
}

/// A card's rigid pose: the shift (x, y, z, px of a 640 px slide; z toward the viewer is positive) of the card's middle and the turns
/// about its middle: about x, about y, about z (radians; applied in the order y, x, z, as `turn3d_tracks` documents).
pub(crate) type Pose = [f32; 6];

/// The effects drawn as rigid cards, each at a pose of its own per video frame: スイッチ (`switch`), 観覧車 (`ferris`), 倒れる (`fallOver`;
/// the old slide falls, the new one stays), 風 (`wind`; the old slide is blown off, the new one stays), ドレープ (`drape`; the new slide falls
/// onto the old one, which stays).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rigid {
    Switch,
    Ferris,
    FallOver,
    Wind,
    Drape,
}

impl Rigid {
    /// The slide's background behind the cards in PowerPoint's video: black for the switch, white for the others.
    pub(crate) fn dark_background(self) -> bool {
        self == Rigid::Switch
    }
}

/// The poses of a card at the video frame `frame`, linearly between the table's rows; `None` before the first row and after the last (the
/// card is not in the picture).
fn pose_at(track: &[(f32, Pose)], frame: f32) -> Option<Pose> {
    let (first, last) = (track.first()?, track.last()?);
    if frame < first.0 || frame > last.0 {
        return None;
    }
    let i = track.windows(2).position(|p| frame <= p[1].0).unwrap_or(track.len().saturating_sub(2));
    let (a, b) = (track.get(i)?, track.get(i + 1).unwrap_or(&track[i]));
    let u = if b.0 > a.0 { ((frame - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 0.0 };
    let mut out = [0.0; 6];
    for (k, o) in out.iter_mut().enumerate() {
        *o = a.1[k] + (b.1[k] - a.1[k]) * u;
    }
    Some(out)
}

/// A card at `pose` seen by the camera of focal length `RIGID_FOCAL` x width; `mirror` -1 for the table's mirror image.
fn pose_card(face: Face, pose: Pose, mirror: f32, w: f32, h: f32) -> Card {
    let k = w / 640.0;
    Card::flat(
        face,
        Transform3D::translate(-w / 2.0, -h / 2.0, 0.0)
            .then(Transform3D::rotate_y(pose[4] * mirror))
            .then(Transform3D::rotate_x(pose[3]))
            .then(Transform3D::rotate_z(pose[5] * mirror))
            .then(Transform3D::translate(pose[0] * mirror * k, pose[1] * k, pose[2] * k))
            .then(Transform3D::perspective(RIGID_FOCAL * w))
            .then(Transform3D::translate(w / 2.0, h / 2.0, 0.0)),
        false,
        1.0,
    )
}

/// The camera's focal length of the rigid effects' tables over the slide's width (fitted 800 of 640 px; 1200 gave nearly the same errors).
const RIGID_FOCAL: f32 = 800.0 / 640.0;

/// The rigid effects at `t` (0 to 1, linear in the video's frames 12 to 72) for the content moving by `motion` (the tables are the
/// right (`motion.0 > 0`) option's; the left one is their mirror): the cards in the picture, the farther one first.
pub(crate) fn rigid(kind: Rigid, motion: (f64, f64), t: f32, w: f32, h: f32) -> Vec<Card> {
    use super::turn3d_tracks as tracks;
    let mirror = if motion.0 < 0.0 { -1.0 } else { 1.0 };
    let frame = 12.0 + t.clamp(0.0, 1.0) * 60.0;
    let (old, new): (&[(f32, Pose)], &[(f32, Pose)]) = match kind {
        Rigid::Switch => (tracks::SWITCH_OLD, tracks::SWITCH_NEW),
        Rigid::Ferris => (tracks::FERRIS_OLD, tracks::FERRIS_NEW),
        Rigid::FallOver => (tracks::FALL_OVER_OLD, &[]),
        Rigid::Wind => (tracks::WIND_OLD, &[]),
        Rigid::Drape => (&[], tracks::DRAPE_NEW),
    };
    let mut cards: Vec<(f32, Card)> = Vec::new();
    for (face, track) in [(Face::Old, old), (Face::New, new)] {
        let pose = if track.is_empty() { Some([0.0; 6]) } else { pose_at(track, frame) };
        if let Some(pose) = pose {
            cards.push((pose[2], pose_card(face, pose, mirror, w, h)));
        }
    }
    // The nearer card is drawn last; the falling slide is drawn over the new one.
    if matches!(kind, Rigid::FallOver | Rigid::Wind) {
        cards.sort_by_key(|(_, c)| c.face == Face::Old);
    } else if kind == Rigid::Drape {
        cards.sort_by_key(|(_, c)| c.face == Face::New);
    } else {
        cards.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    cards.into_iter().map(|(_, c)| c).collect()
}

/// ページめくり (単一, `pageCurlSingle`): the old slide slides off while its leading edge lifts toward the viewer as a bent sheet whose height is
/// `lift x (u / width) ^ power` (u along the slide from the edge that stays) and whose length is kept (a bent sheet's shadow is shorter). The
/// rows are (frame, shift, lift, power) in px of a 640 px slide, fitted per frame; the new slide stays under it.
pub(crate) fn page_curl_single(motion: (f64, f64), t: f32, w: f32, h: f32) -> Vec<Card> {
    use super::turn3d_tracks::PAGE_CURL_SINGLE;
    let mirror = motion.0 < 0.0;
    let frame = 12.0 + t.clamp(0.0, 1.0) * 60.0;
    let mut cards = vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    // Before the first row the slide is at rest (the first row is nearly so); after the last it is gone.
    let frame = frame.max(PAGE_CURL_SINGLE[0].0);
    let Some(i) = PAGE_CURL_SINGLE.windows(2).position(|p| frame <= p[1].0) else { return cards };
    let (a, b) = (PAGE_CURL_SINGLE[i], PAGE_CURL_SINGLE[i + 1]);
    let u = if b.0 > a.0 { ((frame - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 0.0 };
    let (shift, lift, power) = (a.1 + (b.1 - a.1) * u, a.2 + (b.2 - a.2) * u, a.3 + (b.3 - a.3) * u);
    if shift >= 639.0 {
        return cards;
    }
    let k = w / 640.0;
    const COLUMNS: usize = 48;
    // The profile along the page: height by u and the x by its arc length.
    let mut profile = Vec::with_capacity(COLUMNS);
    let (mut x, mut prev) = (0.0_f32, (0.0_f32, 0.0_f32));
    for c in 0..COLUMNS {
        let u = c as f32 / (COLUMNS - 1) as f32;
        let z = lift * u.powf(power);
        let px = u * 640.0;
        if c > 0 {
            x += ((px - prev.0).powi(2) + (z - prev.1).powi(2)).sqrt();
        }
        prev = (px, z);
        profile.push((u, x, z));
    }
    let mut vertices = Vec::with_capacity(COLUMNS * 2);
    for &(u, x, z) in &profile {
        // The mirror image: the lift grows toward the left edge.
        let ux = if mirror { 1.0 - u } else { u };
        let px = if mirror { 640.0 - (x + shift) } else { x + shift };
        for v in [0.0, 1.0] {
            vertices.push(MeshVertex::new([(px - 320.0) * k, v * h - h / 2.0, z * k], [ux, v]));
        }
    }
    let mut indices = Vec::new();
    for c in 0..COLUMNS - 1 {
        let (tl, bl, tr, br) = ((c * 2) as u32, (c * 2 + 1) as u32, (c * 2 + 2) as u32, (c * 2 + 3) as u32);
        if mirror {
            indices.extend([tr, tl, br, tl, bl, br]);
        } else {
            indices.extend([tl, tr, bl, tr, br, bl]);
        }
    }
    cards.push(Card {
        face: Face::Old,
        transform: Transform3D::perspective(RIGID_FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0)),
        cull_back: false,
        shade: 1.0,
        mesh: Some(Mesh { vertices: Arc::new(vertices), indices }),
        white: 0.0,
    });
    cards
}

/// The camera of the rolled sheet over the slide's width (fitted 1000 of 640 px: 500, 2000 and 6000 gave larger errors).
const ROLL_FOCAL: f32 = 1000.0 / 640.0;
/// How much darker the part of a rolled sheet that rises from the page is, at its steepest: `1 - ROLL_DARK (1 - |cos phi|)`, `phi` the angle
/// round the roll (fitted with the radius and the roll's place per frame on peelOff: 0.6 gave 0.0141, 0.3 0.0142 and none 0.0147).
const ROLL_DARK: f32 = 0.6;
/// How much of white lies over the sheet's back: none (the back shows the picture as it is: 0 gave 0.0141, 0.3 0.0177 and 0.6 higher).
pub(crate) const BACK_WHITE: f32 = 0.0;

/// A point of a page rolled over on a cylinder: the page's point (u, v) (px of a 640 px slide) for a roll whose axis line is the line at
/// the distance `c` along the direction `theta` (the side beyond it, positive, is the rolled one), of radius `r`. Returns the
/// position (x, y, z; z toward the viewer) and the angle round the roll `phi` (0 where the page is flat, up to pi at the top, then
/// the page goes back over itself at the height 2 r).
pub(crate) fn roll_point(c: f32, r: f32, theta: f32, u: f32, v: f32) -> ([f32; 3], f32) {
    let (n, t) = ((theta.cos(), theta.sin()), (-theta.sin(), theta.cos()));
    let s = u * n.0 + v * n.1 - c;
    let a = u * t.0 + v * t.1;
    let phi = (s.max(0.0) / r).max(0.0);
    let (along, z) = if s <= 0.0 {
        (s, 0.0)
    } else if phi <= std::f32::consts::PI {
        (r * phi.sin(), r * (1.0 - phi.cos()))
    } else {
        (-(s - std::f32::consts::PI * r), 2.0 * r)
    };
    ([(c + along) * n.0 + a * t.0, (c + along) * n.1 + a * t.1, z], phi)
}

/// The sheet of the old slide rolled as `roll_point` says, as cards drawn from the flat part to the top of the roll: the flat part, the
/// rising part in four bands of its own darkness, and the part that is back over the roll (the sheet's back, mirrored by the roll itself).
/// The new slide lies flat under it (it is the first card). `mirror` for the roll in the mirror image of the picture.
pub(crate) fn rolled_sheet(c: f32, r: f32, theta: f32, mirror: bool, w: f32, h: f32) -> Vec<Card> {
    const NX: usize = 96;
    let k = w / 640.0;
    let hh = h / k;
    let ny = ((NX as f32 * hh / 640.0).round() as usize).max(2);
    let model_u = |u: f32| if mirror { 640.0 - u } else { u };
    let mut vertices = Vec::with_capacity((NX + 1) * (ny + 1));
    for j in 0..=ny {
        for i in 0..=NX {
            let (u, v) = (640.0 * i as f32 / NX as f32, hh * j as f32 / ny as f32);
            let ([x, y, z], _) = roll_point(c, r, theta, model_u(u), v);
            let x = if mirror { 640.0 - x } else { x };
            vertices.push(MeshVertex::new([(x - 320.0) * k, (y - hh / 2.0) * k, z * k], [u / 640.0, v / hh]));
        }
    }
    const BANDS: usize = 7;
    let mut indices: Vec<Vec<u32>> = vec![Vec::new(); BANDS];
    let id = |i: usize, j: usize| (j * (NX + 1) + i) as u32;
    // The band a point of the sheet (model units) is in.
    let band_at = |u: f32, v: f32| -> usize {
        let (_, phi) = roll_point(c, r, theta, model_u(u), v);
        let s = model_u(u) * theta.cos() + v * theta.sin() - c;
        let half = std::f32::consts::FRAC_PI_2;
        if s <= 0.0 {
            0
        } else if phi <= half {
            1 + ((phi / half * 4.0) as usize).min(3)
        } else if phi <= std::f32::consts::PI {
            5
        } else {
            6
        }
    };
    // A cell the border between two bands runs through is cut into SPLIT x SPLIT parts, each in the band of its middle, so the border
    // is as fine as the parts and not a staircase of whole cells (PowerPoint's roll is smooth).
    const SPLIT: usize = 8;
    for j in 0..ny {
        for i in 0..NX {
            let (u0, u1) = (640.0 * i as f32 / NX as f32, 640.0 * (i + 1) as f32 / NX as f32);
            let (v0, v1) = (hh * j as f32 / ny as f32, hh * (j + 1) as f32 / ny as f32);
            let band = band_at((u0 + u1) / 2.0, (v0 + v1) / 2.0);
            let mixed = [(u0, v0), (u1, v0), (u0, v1), (u1, v1)].iter().any(|&(u, v)| band_at(u, v) != band);
            if !mixed {
                let (tl, tr, bl, br) = (id(i, j), id(i + 1, j), id(i, j + 1), id(i + 1, j + 1));
                indices[band].extend([tl, tr, bl, tr, br, bl]);
                continue;
            }
            let first = vertices.len() as u32;
            for sj in 0..=SPLIT {
                for si in 0..=SPLIT {
                    let (u, v) = (u0 + (u1 - u0) * si as f32 / SPLIT as f32, v0 + (v1 - v0) * sj as f32 / SPLIT as f32);
                    let ([x, y, z], _) = roll_point(c, r, theta, model_u(u), v);
                    let x = if mirror { 640.0 - x } else { x };
                    vertices.push(MeshVertex::new([(x - 320.0) * k, (y - hh / 2.0) * k, z * k], [u / 640.0, v / hh]));
                }
            }
            let sub = |si: usize, sj: usize| first + (sj * (SPLIT + 1) + si) as u32;
            for sj in 0..SPLIT {
                for si in 0..SPLIT {
                    let (u, v) = (u0 + (u1 - u0) * (si as f32 + 0.5) / SPLIT as f32, v0 + (v1 - v0) * (sj as f32 + 0.5) / SPLIT as f32);
                    let (tl, tr, bl, br) = (sub(si, sj), sub(si + 1, sj), sub(si, sj + 1), sub(si + 1, sj + 1));
                    indices[band_at(u, v)].extend([tl, tr, bl, tr, br, bl]);
                }
            }
        }
    }
    let vertices = Arc::new(vertices);
    let mut cards = vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    for (band, tris) in indices.into_iter().enumerate() {
        if tris.is_empty() {
            continue;
        }
        let shade = if (1..=4).contains(&band) {
            let phi = (band as f32 - 0.5) / 4.0 * std::f32::consts::FRAC_PI_2;
            1.0 - ROLL_DARK * (1.0 - phi.cos().abs())
        } else {
            1.0
        };
        cards.push(Card {
            face: Face::Old,
            transform: Transform3D::perspective(ROLL_FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0)),
            cull_back: false,
            shade,
            mesh: Some(Mesh { vertices: vertices.clone(), indices: tris }),
            white: if band >= 5 { BACK_WHITE } else { 0.0 },
        });
    }
    cards
}

/// ピールオフ (`peelOff`): the old slide is peeled off by a roll along a slanted line that goes across the slide. The rows of `PEEL_OFF` are
/// (video frame, the roll's place `c`, its radius `r`), the direction `PEEL_OFF_THETA` is the same throughout (fitted per frame;
/// the angle came out 2.35 to 2.43 when free).
pub(crate) fn peel_off(motion: (f64, f64), t: f32, w: f32, h: f32) -> Vec<Card> {
    use super::turn3d_tracks::{PEEL_OFF, PEEL_OFF_THETA};
    let mirror = motion.0 < 0.0;
    let frame = 12.0 + t.clamp(0.0, 1.0) * 60.0;
    let (first, last) = (PEEL_OFF[0], PEEL_OFF[PEEL_OFF.len() - 1]);
    if frame < first.0 {
        return rolled_sheet(2000.0, 30.0, PEEL_OFF_THETA, mirror, w, h);
    }
    if frame > last.0 {
        return vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    }
    let i = PEEL_OFF.windows(2).position(|p| frame <= p[1].0).unwrap_or(PEEL_OFF.len() - 2);
    let (a, b) = (PEEL_OFF[i], PEEL_OFF[i + 1]);
    let u = if b.0 > a.0 { ((frame - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 0.0 };
    rolled_sheet(a.1 + (b.1 - a.1) * u, a.2 + (b.2 - a.2) * u, PEEL_OFF_THETA, mirror, w, h)
}

/// The row of a table of (video frame, numbers) at `frame`, linearly between the rows (the first or last row outside them).
fn row_at<const N: usize>(table: &[(f32, [f32; N])], frame: f32) -> [f32; N] {
    let (first, last) = (table[0], table[table.len() - 1]);
    if frame <= first.0 {
        return first.1;
    }
    if frame >= last.0 {
        return last.1;
    }
    let i = table.windows(2).position(|p| frame <= p[1].0).unwrap_or(table.len() - 2);
    let (a, b) = (table[i], table[i + 1]);
    let u = if b.0 > a.0 { ((frame - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 0.0 };
    let mut out = [0.0; N];
    for (k, o) in out.iter_mut().enumerate() {
        *o = a.1[k] + (b.1[k] - a.1[k]) * u;
    }
    out
}

/// A half of a slide (`left`, or the right) as a flat sheet of the face `face`, at rest in the slide's plane.
fn half_sheet(face: Face, left: bool, w: f32, h: f32) -> Card {
    let (x0, x1) = if left { (0.0, 0.5) } else { (0.5, 1.0) };
    let vertices = vec![
        MeshVertex::new([(x0 - 0.5) * w, -h / 2.0, 0.0], [x0, 0.0]),
        MeshVertex::new([(x1 - 0.5) * w, -h / 2.0, 0.0], [x1, 0.0]),
        MeshVertex::new([(x0 - 0.5) * w, h / 2.0, 0.0], [x0, 1.0]),
        MeshVertex::new([(x1 - 0.5) * w, h / 2.0, 0.0], [x1, 1.0]),
    ];
    Card {
        face,
        transform: Transform3D::perspective(ROLL_FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0)),
        cull_back: false,
        shade: 1.0,
        mesh: Some(Mesh { vertices: Arc::new(vertices), indices: vec![0, 1, 2, 1, 3, 2] }),
        white: 0.0,
    }
}

/// ページめくり（両側） (`pageCurlDouble`): the old slide's right half is a leaf of a book hinged at the slide's vertical middle. It lifts, bends
/// and turns over to the left, its front the old slide's right half and its back the new slide's left half, and lands on the left half; the
/// new slide's right half shows where it was. The leaf's shape is the angle of its tangent in the plane of the turn (0 pointing right, pi
/// pointing left, up toward the viewer) at five points along it, `PAGE_CURL_DOUBLE` rows (frame, angles at 0, 80, 160, 240 and 320 px
/// of the leaf's length in px of a 640 px slide). Fitted per frame by minimising the blurred squared difference of the picture of this
/// model (camera 1000 px) to the video's frame (see `turn3d_tracks`). The table is the left option's (the file without `invX`); the other is
/// its mirror.
pub(crate) fn page_curl_double(motion: (f64, f64), t: f32, w: f32, h: f32) -> Vec<Card> {
    use super::turn3d_tracks::PAGE_CURL_DOUBLE;
    let mirror = motion.0 > 0.0;
    let frame = 12.0 + t.clamp(0.0, 1.0) * 60.0;
    if frame >= PAGE_CURL_DOUBLE[PAGE_CURL_DOUBLE.len() - 1].0 {
        return vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    }
    let angles = row_at(PAGE_CURL_DOUBLE, frame);
    let k = w / 640.0;
    let hh = h / k;
    const COLUMNS: usize = 64;
    let ds = 320.0 / COLUMNS as f32;
    let angle_at = |s: f32| {
        let x = (s / 80.0).clamp(0.0, 3.999);
        let i = x as usize;
        angles[i] + (angles[i + 1] - angles[i]) * (x - i as f32)
    };
    let mut front = Vec::with_capacity((COLUMNS + 1) * 2);
    let mut back = Vec::with_capacity((COLUMNS + 1) * 2);
    let (mut x, mut z) = (320.0_f32, 0.0_f32);
    let mut points = Vec::with_capacity(COLUMNS + 1);
    points.push((x, z));
    let mut faces_front = Vec::with_capacity(COLUMNS);
    for j in 0..COLUMNS {
        let a = angle_at((j as f32 + 0.5) * ds);
        x += a.cos() * ds;
        z += a.sin() * ds;
        points.push((x, z));
        faces_front.push(a.cos() > 0.0);
    }
    for (j, &(px, pz)) in points.iter().enumerate() {
        let s = j as f32 * ds;
        // The picture's own coordinates: the mirror image has the leaf on the other side.
        let place = |mx: f32| if mirror { 640.0 - mx } else { mx };
        for v in [0.0_f32, 1.0] {
            let position = [(place(px) - 320.0) * k, (v * hh - hh / 2.0) * k, pz * k];
            front.push(MeshVertex::new(position, [place(320.0 + s) / 640.0, v]));
            back.push(MeshVertex::new(position, [place(320.0 - s) / 640.0, v]));
        }
    }
    let (mut front_indices, mut back_indices) = (Vec::new(), Vec::new());
    for (j, &is_front) in faces_front.iter().enumerate() {
        let (tl, bl, tr, br) = ((j * 2) as u32, (j * 2 + 1) as u32, (j * 2 + 2) as u32, (j * 2 + 3) as u32);
        let list = if is_front { &mut front_indices } else { &mut back_indices };
        list.extend([tl, tr, bl, tr, br, bl]);
    }
    // The slide's halves that stay: the old one's far half, the new one's near half (the table's left option; mirrored for the other).
    let mut cards = vec![half_sheet(Face::Old, !mirror, w, h), half_sheet(Face::New, mirror, w, h)];
    let transform = Transform3D::perspective(ROLL_FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    for (face, vertices, indices) in [(Face::Old, front, front_indices), (Face::New, back, back_indices)] {
        if !indices.is_empty() {
            cards.push(Card { face, transform, cull_back: false, shade: 1.0, mesh: Some(Mesh { vertices: Arc::new(vertices), indices }), white: 0.0 });
        }
    }
    cards
}

/// A 3 x 3 rotation (the turns about x, y, z applied in the order y, x, z, as `Pose` documents).
fn euler(rx: f32, ry: f32, rz: f32) -> [[f32; 3]; 3] {
    let (cx, sx, cy, sy, cz, sz) = (rx.cos(), rx.sin(), ry.cos(), ry.sin(), rz.cos(), rz.sin());
    let rxm = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let rym = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rzm = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    let mul = |a: [[f32; 3]; 3], b: [[f32; 3]; 3]| {
        let mut m = [[0.0_f32; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
            }
        }
        m
    };
    mul(rzm, mul(rxm, rym))
}

fn apply(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// 紙飛行機 (`airplane`): the old slide is cut along two creases from the middle of its top edge A to the bottom edge at B and C (`AIRPLANE_CREASE`
/// px from the corners of a 640 px slide) into a middle triangle ABC and two wings. The wings slide out sideways, flat, and leave the
/// picture; the triangle, bent along its middle line (A to the middle M of the bottom edge) into the plane's two faces, moves and turns as
/// a rigid body (the folded wings' white edges are not drawn) and flies off to the upper right. `AIRPLANE` rows give the pose per video
/// frame (see `turn3d_tracks`); the table is the file without `invX`, the other is its mirror image. The new slide lies behind. The
/// paper's back is white, and a face's brightness is `1 - 0.4 (1 - |n_z|)`.
pub(crate) fn airplane(motion: (f64, f64), t: f32, w: f32, h: f32) -> Vec<Card> {
    use super::turn3d_tracks::AIRPLANE;
    let mirror = motion.0 > 0.0;
    let frame = 12.0 + t.clamp(0.0, 1.0) * 60.0;
    let mut cards = vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    if frame >= AIRPLANE[AIRPLANE.len() - 1].0 {
        return cards;
    }
    let [tx, ty, tz, rx, ry, rz, phi, side] = row_at(AIRPLANE, frame);
    let k = w / 640.0;
    let hh = h / k;
    let (a, b, c, m) = ([320.0_f32, 0.0, 0.0], [AIRPLANE_CREASE, hh, 0.0], [640.0 - AIRPLANE_CREASE, hh, 0.0], [320.0_f32, hh, 0.0]);
    let (tl, tr, bl, br) = ([0.0_f32, 0.0, 0.0], [640.0_f32, 0.0, 0.0], [0.0_f32, hh, 0.0], [640.0_f32, hh, 0.0]);
    let rotation = euler(rx, ry, rz);
    let center = [320.0_f32, hh / 2.0, 0.0];
    // Rotation about the middle line (the vertical line through A and M) by `angle`.
    let fold = |v: [f32; 3], angle: f32| {
        let (s, co) = (angle.sin(), angle.cos());
        let (x, z) = (v[0] - 320.0, v[2]);
        [320.0 + co * x + s * z, v[1], -s * x + co * z]
    };
    let put = |v: [f32; 3]| {
        let r = apply(&rotation, [v[0] - center[0], v[1] - center[1], v[2] - center[2]]);
        [r[0] + center[0] + tx, r[1] + center[1] + ty, r[2] + center[2] + tz]
    };
    // (the corners at rest, the way each is placed)
    let mut faces: Vec<([[f32; 3]; 3], [[f32; 3]; 3])> = Vec::new();
    let slid = |v: [f32; 3], d: f32| [v[0] + d, v[1], v[2]];
    if side < 600.0 {
        for tri in [[tl, a, b], [tl, b, bl]] {
            faces.push((tri, tri.map(|v| slid(v, -side))));
        }
        for tri in [[a, tr, c], [tr, br, c]] {
            faces.push((tri, tri.map(|v| slid(v, side))));
        }
    }
    faces.push(([a, b, m], [a, b, m].map(|v| put(fold(v, phi / 2.0)))));
    faces.push(([a, m, c], [a, m, c].map(|v| put(fold(v, -phi / 2.0)))));
    const FOCAL: f32 = 1000.0;
    let transform = Transform3D::perspective(ROLL_FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    // Far to near.
    let mut drawn: Vec<(f32, Card)> = Vec::new();
    for (rest, placed) in faces {
        let screen = |v: [f32; 3]| {
            let s = FOCAL / (FOCAL - v[2]);
            [(v[0] - 320.0) * s, (v[1] - hh / 2.0) * s]
        };
        if placed.iter().any(|v| v[2] > FOCAL - 40.0) {
            continue;
        }
        let area = |p: [[f32; 2]; 3]| (p[1][1] - p[2][1]) * (p[0][0] - p[2][0]) + (p[2][0] - p[1][0]) * (p[0][1] - p[2][1]);
        let rest_area = area(rest.map(|v| [v[0], v[1]]));
        let placed_area = area(placed.map(screen));
        let front = rest_area * placed_area > 0.0;
        let normal = {
            let (u, v) = ([placed[1][0] - placed[0][0], placed[1][1] - placed[0][1], placed[1][2] - placed[0][2]], [placed[2][0] - placed[0][0], placed[2][1] - placed[0][1], placed[2][2] - placed[0][2]]);
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            n[2].abs() / (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6)
        };
        let vertices: Vec<MeshVertex> = rest
            .iter()
            .zip(placed.iter())
            .map(|(r, p)| {
                let place = |x: f32| if mirror { 640.0 - x } else { x };
                MeshVertex::new([(place(p[0]) - 320.0) * k, (p[1] - hh / 2.0) * k, p[2] * k], [place(r[0]) / 640.0, r[1] / hh])
            })
            .collect();
        let depth = placed.iter().map(|v| v[2]).sum::<f32>() / 3.0;
        drawn.push((
            depth,
            Card {
                face: Face::Old,
                transform,
                cull_back: false,
                shade: 1.0 - AIRPLANE_SHADE * (1.0 - normal),
                mesh: Some(Mesh { vertices: Arc::new(vertices), indices: vec![0, 1, 2] }),
                white: if front { 0.0 } else { 1.0 },
            },
        ));
    }
    drawn.sort_by(|p, q| p.0.total_cmp(&q.0));
    cards.extend(drawn.into_iter().map(|(_, c)| c));
    cards
}

/// 折り紙 (`origami`): the old slide folds into a paper crane that flies off to the upper right. The slide is cut into the crane's faces
/// (`CRANE`, from PowerPoint's video): each corner goes from its place on the slide to its place on the crane by `lam` (a straight line), the
/// crane being turned, scaled and moved by an affine map (`ORIGAMI_POSE`, fitted per video frame); the paper's back faces are white. Seen from
/// the front, without perspective. Linear in time; the table is the file without `invX`, the other is its mirror image (the texture too).
pub(crate) fn origami(motion: (f64, f64), t: f32, w: f32, h: f32) -> Vec<Card> {
    use super::turn3d_tracks::{CRANE, ORIGAMI_POSE};
    let mirror = motion.0 > 0.0;
    let frame = 12.0 + t.clamp(0.0, 1.0) * 60.0;
    let mut cards = vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    if frame >= ORIGAMI_POSE[ORIGAMI_POSE.len() - 1].0 {
        return cards;
    }
    // Before the first row the slide is whole and flat.
    if frame < ORIGAMI_POSE[0].0 {
        cards.push(pose_card(Face::Old, [0.0; 6], 1.0, w, h));
        return cards;
    }
    let [lam, scale, turn, shift_x, shift_y, shear, stretch] = row_at(ORIGAMI_POSE, frame);
    let k = w / 640.0;
    let hh = h / k;
    let ky = hh / 360.0;
    let (cos, sin) = (turn.cos(), turn.sin());
    // A point of the 640 x 360 picture of the model, as the affine map of the crane puts it (in px of a 640 px slide).
    let place = |x: f32, y: f32| -> (f32, f32) {
        let (px, py) = (x - 320.0, y - 180.0);
        let (px, py) = (px + shear * py, stretch * py);
        (320.0 + scale * (cos * px - sin * py) + shift_x, 180.0 + scale * (sin * px + cos * py) + shift_y)
    };
    let transform = Transform3D::perspective(ROLL_FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    let flip = |x: f32| if mirror { 640.0 - x } else { x };
    // The colour faces first, the white backs over them.
    for white in [false, true] {
        for (n, face) in CRANE.iter().enumerate().filter(|(_, f)| (f.color == 4) == white) {
            if white && lam < 0.3 {
                continue;
            }
            let count = face.poly.len() as f32;
            let (cx, cy) = (face.poly.iter().map(|p| p[0]).sum::<f32>() / count, face.poly.iter().map(|p| p[1]).sum::<f32>() / count);
            let (bx0, bx1) = face.poly.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
            let (by0, by1) = face.poly.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
            let mut vertices = Vec::with_capacity(face.poly.len());
            for p in face.poly {
                let (bird_x, bird_y) = (p[0], p[1]);
                let (flat_x, flat_y, u, v) = if white {
                    // A back grows from its middle.
                    (cx + lam * (bird_x - cx), cy + lam * (bird_y - cy), 0.0, 0.0)
                } else {
                    let [u0, v0, u1, v1] = face.rect;
                    let u = u0 + (p[0] - bx0) / (bx1 - bx0).max(1e-6) * (u1 - u0);
                    let v = v0 + (p[1] - by0) / (by1 - by0).max(1e-6) * (v1 - v0);
                    (u * 640.0 * (1.0 - lam) + bird_x * lam, v * 360.0 * (1.0 - lam) + bird_y * lam, u, v)
                };
                let (x, y) = place(flat_x, flat_y);
                let u = if mirror { 1.0 - u } else { u };
                vertices.push(MeshVertex::new([(flip(x) - 320.0) * k, (y * ky - hh / 2.0) * k, 0.01 * n as f32], [u, v]));
            }
            let indices: Vec<u32> = face.tris.iter().flat_map(|t| t.iter().map(|&i| i as u32)).collect();
            cards.push(Card {
                face: Face::Old,
                transform,
                cull_back: false,
                shade: if white { 0.93 } else { 1.0 },
                mesh: Some(Mesh { vertices: Arc::new(vertices), indices }),
                white: if white { 1.0 } else { 0.0 },
            });
        }
    }
    cards
}

/// 紙飛行機's creases end at x = 80 and 560 on the bottom edge of a 640 px slide (measured on the video's first frames of the motion: the line
/// from the top edge's middle slopes 0.69 px across per px down, 0.125 of the width in).
const AIRPLANE_CREASE: f32 = 80.0;
/// How much darker a face turned edge-on is than one facing the viewer (0.2, 0.4 and 0.5 gave 0.0102, 0.0096 and 0.0098 over frames 3524 to 3562).
const AIRPLANE_SHADE: f32 = 0.4;

/// カーテン (`curtains`): the old slide's two halves are pleated cloth that gathers at the sides, leaning as it swings, with the new slide
/// behind. `CURTAINS` rows are (frame, [inner edge at the top, inner edge at the bottom, outer edge at the top, outer edge at the bottom])
/// of the left half in px of a 640 px slide; the right half is its mirror. The halves are the old slide's halves squeezed across (the
/// pleats' shading is not drawn).
pub(crate) fn curtains(t: f32, w: f32, h: f32) -> Vec<Card> {
    use super::turn3d_tracks::CURTAINS;
    let frame = 12.0 + t.clamp(0.0, 1.0) * 60.0;
    let mut cards = vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    if frame >= CURTAINS[CURTAINS.len() - 1].0 {
        return cards;
    }
    let [lt, lb, ot, ob] = if frame < CURTAINS[0].0 { [320.0, 320.0, 0.0, 0.0] } else { row_at(CURTAINS, frame) };
    let k = w / 640.0;
    const N: usize = 8;
    let transform = Transform3D::perspective(ROLL_FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    for right in [false, true] {
        let mut vertices = Vec::with_capacity((N + 1) * (N + 1));
        for j in 0..=N {
            let v = j as f32 / N as f32;
            let (xin, xo) = (lt + (lb - lt) * v, ot + (ob - ot) * v);
            for i in 0..=N {
                let u = i as f32 / N as f32;
                let x = xo + (xin - xo) * u;
                let (px, tu) = if right { (640.0 - x, 1.0 - u * 0.5) } else { (x, u * 0.5) };
                vertices.push(MeshVertex::new([(px - 320.0) * k, (v - 0.5) * h, 0.0], [tu, v]));
            }
        }
        let mut indices = Vec::new();
        for j in 0..N {
            for i in 0..N {
                let id = |a: usize, b: usize| (b * (N + 1) + a) as u32;
                let (tl, tr, bl, br) = (id(i, j), id(i + 1, j), id(i, j + 1), id(i + 1, j + 1));
                indices.extend([tl, tr, bl, tr, br, bl]);
            }
        }
        cards.push(Card { face: Face::Old, transform, cull_back: false, shade: 1.0, mesh: Some(Mesh { vertices: Arc::new(vertices), indices }), white: 0.0 });
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corner(card: &Card, x: f32, y: f32) -> Option<(f32, f32)> {
        card.transform.project(x, y, 0.0)
    }

    #[test]
    fn at_rest_the_old_slide_is_where_it_is_and_the_new_one_is_at_the_side() {
        let [old, new] = cube((-1.0, 0.0), 0.0, 640.0, 360.0);
        let p = corner(&old, 100.0, 50.0).unwrap();
        assert!((p.0 - 100.0).abs() < 0.01 && (p.1 - 50.0).abs() < 0.01, "{p:?}");
        // The new slide to the right, turned a quarter: its left edge at the old one's right edge.
        let left_edge = corner(&new, 0.0, 180.0).unwrap();
        assert!((left_edge.0 - 640.0).abs() < 0.5, "{left_edge:?}");
        let [old, new] = cube((-1.0, 0.0), 1.0, 640.0, 360.0);
        let p = corner(&new, 100.0, 50.0).unwrap();
        assert!((p.0 - 100.0).abs() < 0.01 && (p.1 - 50.0).abs() < 0.01, "{p:?}");
        let right_edge = corner(&old, 640.0, 180.0).unwrap();
        assert!(right_edge.0.abs() < 0.5, "{right_edge:?}");
    }

    #[test]
    fn the_corner_of_the_cube_is_the_nearest_line_in_the_middle_of_the_turn() {
        let [old, new] = cube((-1.0, 0.0), 0.5, 640.0, 360.0);
        // The shared corner: the old slide's right edge, the new one's left edge, at the same place.
        let a = corner(&old, 640.0, 0.0).unwrap();
        let b = corner(&new, 0.0, 0.0).unwrap();
        assert!((a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5, "{a:?} {b:?}");
        // Nearer means taller on the screen: the edge at the corner is taller than at the far side of the old slide.
        let tall = |c: &Card, x: f32| corner(c, x, 360.0).unwrap().1 - corner(c, x, 0.0).unwrap().1;
        assert!(tall(&old, 640.0) > tall(&old, 0.0));
        assert!(tall(&new, 0.0) > tall(&new, 640.0));
        // The corner is in the middle of the picture.
        assert!((a.0 - 320.0).abs() < 5.0, "{a:?}");
    }

    #[test]
    fn the_cube_matches_a_measured_frame() {
        // PowerPoint's 立方体 (left) at the angle where the old red rectangle's right edge (slide x 300) reached the picture's
        // quarter: fitted frame 28 of 78, angle 0.51 rad; the same rectangle's left edge (slide x 20) was at x = 0.4 of 640 px.
        let [old, _] = cube_at((-1.0, 0.0), 0.51, 640.0, 360.0);
        let (l, r) = (corner(&old, 20.0, 20.0).unwrap(), corner(&old, 300.0, 20.0).unwrap());
        assert!(r.0 > l.0 && r.0 - l.0 < 280.0, "{l:?} {r:?}");
    }

    #[test]
    fn the_box_has_its_corner_far_and_the_cube_near() {
        let tall = |c: &Card, x: f32| corner(c, x, 360.0).unwrap().1 - corner(c, x, 0.0).unwrap().1;
        let [old, _] = prism(Prism::Box, (-1.0, 0.0), 0.5, 640.0, 360.0);
        assert!(tall(&old, 640.0) < tall(&old, 0.0), "a box's corner is the farthest line");
        let [old, _] = prism(Prism::Cube, (-1.0, 0.0), 0.5, 640.0, 360.0);
        assert!(tall(&old, 640.0) > tall(&old, 0.0));
        // At rest each is the slide in its place.
        for kind in [Prism::Box, Prism::Orbit, Prism::Rotate] {
            let [old, _] = prism(kind, (-1.0, 0.0), 0.0, 640.0, 360.0);
            let p = corner(&old, 100.0, 50.0).unwrap();
            assert!((p.0 - 100.0).abs() < 0.01 && (p.1 - 50.0).abs() < 0.01, "{kind:?} {p:?}");
        }
    }

    #[test]
    fn conveyor_rests_at_both_ends_and_carries_both_cards_in_between() {
        let start = conveyor((-1.0, 0.0), 0.0, 640.0, 360.0);
        assert_eq!(start.len(), 1);
        assert_eq!(start[0].face, Face::Old);
        let p = start[0].transform.project(100.0, 50.0, 0.0).unwrap();
        assert!((p.0 - 100.0).abs() < 0.01 && (p.1 - 50.0).abs() < 0.01, "{p:?}");
        let end = conveyor((-1.0, 0.0), 1.0, 640.0, 360.0);
        assert_eq!(end.len(), 1);
        assert_eq!(end[0].face, Face::New);
        let middle = conveyor((-1.0, 0.0), 0.5, 640.0, 360.0);
        assert_eq!(middle.len(), 2);
        // The mirror: the old card goes to the right for the other option.
        let left = conveyor((-1.0, 0.0), 0.55, 640.0, 360.0)[0].transform.project(320.0, 180.0, 0.0).unwrap().0;
        let right = conveyor((1.0, 0.0), 0.55, 640.0, 360.0)[0].transform.project(320.0, 180.0, 0.0).unwrap().0;
        assert!(left < 320.0 && right > 320.0, "{left} {right}");
    }

    #[test]
    fn gallery_rests_at_both_ends_and_scrolls_the_strip() {
        let start = gallery((-1.0, 0.0), 0.0, 640.0, 360.0);
        let p = start[0].transform.project(100.0, 50.0, 0.0).unwrap();
        assert!((p.0 - 100.0).abs() < 8.0, "{p:?}");
        let end = gallery((-1.0, 0.0), 1.0, 640.0, 360.0);
        let q = end[1].transform.project(100.0, 50.0, 0.0).unwrap();
        assert!((q.0 - 100.0).abs() < 0.5 && (q.1 - 50.0).abs() < 0.5, "{q:?}");
        // In the middle the strip is backed off and turned: the old card is left of the middle, the new one right of it.
        let mid = gallery((-1.0, 0.0), 0.5, 640.0, 360.0);
        let (old, new) = (mid[0].transform.project(640.0, 180.0, 0.0).unwrap().0, mid[1].transform.project(0.0, 180.0, 0.0).unwrap().0);
        assert!((old - new).abs() < 2.0, "{old} {new}");
    }

    #[test]
    fn the_rigid_effects_rest_at_both_ends() {
        let at = |c: &Card, x: f32, y: f32| c.transform.project(x, y, 0.0).unwrap();
        for kind in [Rigid::Switch, Rigid::Ferris, Rigid::FallOver, Rigid::Wind, Rigid::Drape] {
            let start = rigid(kind, (1.0, 0.0), 0.0, 640.0, 360.0);
            let old = start.iter().find(|c| c.face == Face::Old).expect("the old card at the start");
            let p = at(old, 100.0, 50.0);
            assert!((p.0 - 100.0).abs() < 8.0 && (p.1 - 50.0).abs() < 4.0, "{kind:?} {p:?}");
            let end = rigid(kind, (1.0, 0.0), 1.0, 640.0, 360.0);
            let new = end.iter().find(|c| c.face == Face::New).expect("the new card at the end");
            let q = at(new, 100.0, 50.0);
            assert!((q.0 - 100.0).abs() < 8.0 && (q.1 - 50.0).abs() < 4.0, "{kind:?} {q:?}");
        }
    }

    #[test]
    fn the_left_option_is_the_mirror_of_the_right() {
        let mid = |m: (f64, f64)| rigid(Rigid::Switch, m, 0.5, 640.0, 360.0).iter().map(|c| c.transform.project(320.0, 180.0, 0.0).unwrap().0).collect::<Vec<_>>();
        let (r, l) = (mid((1.0, 0.0)), mid((-1.0, 0.0)));
        for (a, b) in r.iter().zip(&l) {
            assert!((a + b - 640.0).abs() < 0.5, "{a} {b}");
        }
    }

    #[test]
    fn the_page_curl_lifts_the_leading_edge_and_leaves() {
        let start = page_curl_single((1.0, 0.0), 0.0, 640.0, 360.0);
        assert_eq!(start.len(), 2, "the new slide under the old one");
        let mid = page_curl_single((1.0, 0.0), 0.5, 640.0, 360.0);
        let sheet = mid.iter().find(|c| c.mesh.is_some()).expect("the sheet");
        let mesh = sheet.mesh.as_ref().unwrap();
        let height = |v: &MeshVertex| v.position[2];
        assert!(mesh.vertices.iter().map(height).fold(0.0_f32, f32::max) > 50.0, "the edge is lifted");
        assert!(mesh.vertices[0].position[2] < 1.0, "the far edge stays on the table");
        let end = page_curl_single((1.0, 0.0), 1.0, 640.0, 360.0);
        assert!(end.iter().all(|c| c.mesh.is_none()), "the old slide is gone");
    }

    #[test]
    fn a_rolled_page_is_flat_before_the_roll_and_rises_round_it() {
        let (r, c) = (50.0_f32, 300.0_f32);
        // The roll's direction along x: the page beyond x = 300 is rolled.
        let (p, _) = roll_point(c, r, 0.0, 200.0, 40.0);
        assert_eq!(p, [200.0, 40.0, 0.0]);
        let (top, phi) = roll_point(c, r, 0.0, c + std::f32::consts::FRAC_PI_2 * r, 40.0);
        assert!((top[2] - r).abs() < 1e-3 && (top[0] - (c + r)).abs() < 1e-3 && (phi - std::f32::consts::FRAC_PI_2).abs() < 1e-5, "{top:?}");
        let (back, _) = roll_point(c, r, 0.0, c + std::f32::consts::PI * r + 30.0, 40.0);
        assert!((back[2] - 2.0 * r).abs() < 1e-3 && (back[0] - (c - 30.0)).abs() < 1e-3, "{back:?}");
    }

    #[test]
    fn the_rolled_sheet_has_the_flat_part_first_and_the_back_last() {
        let cards = rolled_sheet(200.0, 40.0, 0.0, false, 640.0, 360.0);
        assert_eq!(cards[0].face, Face::New);
        assert!(cards.len() >= 4, "{}", cards.len());
        let heights: Vec<f32> = cards[1..].iter().map(|c| c.mesh.as_ref().unwrap().indices.iter().map(|&i| c.mesh.as_ref().unwrap().vertices[i as usize].position[2]).sum::<f32>() / c.mesh.as_ref().unwrap().indices.len() as f32).collect();
        assert!(heights.windows(2).all(|w| w[0] <= w[1] + 1e-3), "{heights:?}");
        // At rest (the roll beyond the slide) the sheet is all flat.
        let rest = rolled_sheet(2000.0, 40.0, 0.0, false, 640.0, 360.0);
        assert_eq!(rest.len(), 2);
    }

    #[test]
    fn flip_shows_one_face_at_a_time() {
        for (q, old_front, new_front) in [(0.25, true, false), (0.75, false, true)] {
            let [old, new] = flip((-1.0, 0.0), q, 640.0, 360.0);
            let winding = |c: &Card| {
                let p0 = corner(c, 0.0, 0.0).unwrap();
                let p1 = corner(c, 640.0, 0.0).unwrap();
                let p2 = corner(c, 0.0, 360.0).unwrap();
                (p1.0 - p0.0) * (p2.1 - p0.1) - (p1.1 - p0.1) * (p2.0 - p0.0) > 0.0
            };
            assert_eq!(winding(&old), old_front, "old at {q}");
            assert_eq!(winding(&new), new_front, "new at {q}");
        }
    }

    #[test]
    fn faces_are_darker_the_more_they_turn() {
        let [a, b] = cube((-1.0, 0.0), 0.5, 640.0, 360.0);
        assert!((a.shade - b.shade).abs() < 1e-5 && a.shade <= 1.0 && a.shade > 0.7);
        let [edge_on, _] = cube_at((-1.0, 0.0), 1.55, 640.0, 360.0);
        assert!((edge_on.shade - 0.76).abs() < 0.01);
        let [at_rest, _] = cube((-1.0, 0.0), 0.0, 640.0, 360.0);
        assert!((at_rest.shade - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vertical_options_turn_about_the_horizontal_axis() {
        let [old, _] = cube((0.0, -1.0), 0.3, 640.0, 360.0);
        let (l, r) = (corner(&old, 0.0, 180.0).unwrap(), corner(&old, 640.0, 180.0).unwrap());
        assert!((l.1 - r.1).abs() < 1.0, "{l:?} {r:?}");
        let (t, b) = (corner(&old, 320.0, 0.0).unwrap(), corner(&old, 320.0, 360.0).unwrap());
        assert!((b.1 - t.1 - 360.0).abs() > 1.0, "{t:?} {b:?}");
    }

    #[test]
    fn curtains_gather_at_the_sides_and_leave_the_new_slide() {
        let reach = |c: &Card, right: bool| {
            let xs = c.mesh.as_ref().unwrap().vertices.iter().map(|v| v.position[0] + 320.0);
            if right { xs.fold(f32::MAX, f32::min) } else { xs.fold(f32::MIN, f32::max) }
        };
        let start = curtains(0.0, 640.0, 360.0);
        assert_eq!(start.len(), 3, "the new slide under the two halves");
        assert_eq!(start[0].face, Face::New);
        assert!((reach(&start[1], false) - 320.0).abs() < 0.5 && (reach(&start[2], true) - 320.0).abs() < 0.5);
        // In the middle the halves are squeezed toward the sides and are mirror images.
        let mid = curtains(0.5, 640.0, 360.0);
        let (l, r) = (reach(&mid[1], false), reach(&mid[2], true));
        assert!(l < 260.0 && (l + r - 640.0).abs() < 0.5, "{l} {r}");
        assert_eq!(curtains(1.0, 640.0, 360.0).len(), 1, "gone at the end");
        // The mesh's texture points stay on the old slide's own half.
        let left = start[1].mesh.as_ref().unwrap().vertices.iter().map(|v| v.uv[0]).fold(0.0_f32, f32::max);
        assert!((left - 0.5).abs() < 1e-5);
    }

    #[test]
    fn the_double_curl_turns_a_leaf_over_to_the_other_half() {
        let start = page_curl_double((-1.0, 0.0), 0.0, 640.0, 360.0);
        // The old slide's two halves are at rest and the leaf lies flat on the right half.
        assert!(start.iter().all(|c| c.mesh.as_ref().unwrap().vertices.iter().all(|v| v.position[2].abs() < 1e-3)), "flat at the start");
        let mid = page_curl_double((-1.0, 0.0), 0.45, 640.0, 360.0);
        let lift = mid.iter().flat_map(|c| c.mesh.as_ref().unwrap().vertices.iter()).map(|v| v.position[2]).fold(0.0_f32, f32::max);
        assert!(lift > 40.0, "the leaf is lifted: {lift}");
        // The mirror image has the leaf on the other side of the hinge (at 0.2 of the time it is still mostly on its own side).
        let mean_x = |m: (f64, f64)| {
            let cards = page_curl_double(m, 0.2, 640.0, 360.0);
            let xs: Vec<f32> = cards.iter().skip(2).flat_map(|c| c.mesh.as_ref().unwrap().vertices.iter()).map(|v| v.position[0]).collect();
            xs.iter().sum::<f32>() / xs.len() as f32
        };
        let (l, r) = (mean_x((-1.0, 0.0)), mean_x((1.0, 0.0)));
        assert!((l + r).abs() < 0.5 && l.abs() > 20.0, "{l} {r}");
        let end = page_curl_double((-1.0, 0.0), 1.0, 640.0, 360.0);
        assert_eq!(end.len(), 1);
        assert_eq!(end[0].face, Face::New);
    }

    #[test]
    fn the_airplane_slides_its_wings_out_and_folds_the_middle_into_a_plane() {
        // At the start the old slide is whole and flat (the new slide behind it, six faces: two wings and two halves of the middle).
        let start = airplane((-1.0, 0.0), 0.0, 640.0, 360.0);
        assert_eq!(start[0].face, Face::New);
        assert!(start[1..].iter().all(|c| c.face == Face::Old && c.white == 0.0), "the front of the paper");
        assert!(start[1..].iter().all(|c| c.mesh.as_ref().unwrap().vertices.iter().all(|v| v.position[2].abs() < 1e-3)), "flat");
        // The wings slide out: at 0.3 of the time the leftmost corner of the old slide is outside the picture; in the mirror the rightmost.
        let reach = |m: (f64, f64), sign: f32| {
            airplane(m, 0.3, 640.0, 360.0)[1..].iter().flat_map(|c| c.mesh.as_ref().unwrap().vertices.iter()).map(|v| v.position[0] * sign).fold(f32::MIN, f32::max)
        };
        assert!(reach((-1.0, 0.0), -1.0) > 320.0 + 40.0 && reach((1.0, 0.0), 1.0) > 320.0 + 40.0);
        // The plane is tilted (no longer flat), and its tilted faces are darker.
        let mid = airplane((-1.0, 0.0), 0.6, 640.0, 360.0);
        assert!(mid[1..].iter().any(|c| c.shade < 0.95));
        assert!(mid[1..].iter().flat_map(|c| c.mesh.as_ref().unwrap().vertices.iter()).any(|v| v.position[2].abs() > 5.0));
        let end = airplane((-1.0, 0.0), 1.0, 640.0, 360.0);
        assert_eq!(end.len(), 1);
    }

    #[test]
    fn the_origami_slide_folds_into_a_crane_of_faces_and_leaves() {
        // Flat and whole at the start; the crane's faces (the colour ones and the white backs) after the folding.
        let start = origami((-1.0, 0.0), 0.0, 640.0, 360.0);
        assert_eq!(start.len(), 2);
        let crane = origami((-1.0, 0.0), 0.3, 640.0, 360.0);
        assert!(crane.len() > 5, "{} cards", crane.len());
        assert!(crane[1..].iter().any(|c| c.white == 1.0), "the paper's white back shows");
        assert!(crane[1..].iter().all(|c| c.mesh.as_ref().is_some_and(|m| m.indices.len() % 3 == 0 && m.indices.iter().all(|&i| (i as usize) < m.vertices.len()))));
        // The mirror image is on the other side.
        let x_of = |m: (f64, f64)| {
            let cards = origami(m, 0.3, 640.0, 360.0);
            let xs: Vec<f32> = cards[1..].iter().flat_map(|c| c.mesh.as_ref().unwrap().vertices.iter()).map(|v| v.position[0]).collect();
            xs.iter().sum::<f32>() / xs.len() as f32
        };
        assert!((x_of((-1.0, 0.0)) + x_of((1.0, 0.0))).abs() < 0.5);
        assert_eq!(origami((-1.0, 0.0), 1.0, 640.0, 360.0).len(), 1, "gone at the end");
    }
}
