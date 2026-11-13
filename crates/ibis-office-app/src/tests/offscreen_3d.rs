// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GPUI's offscreen passes and textured quads (`Window::paint_offscreen`, `paint_textured_quad`), drawn headless:
//! a red card painted to a texture is drawn on a black window through 3D transforms.
use gpui_kit::{Bounds, MeshVertex, OffscreenId, Transform3D, canvas, div, offscreen, point, px, rgb, size};
use gpui_kit::{AppContext, IntoElement, ParentElement, Styled};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("gpui::offscreen_identity_draws_the_card_in_place", identity_draws_the_card_in_place),
        ("gpui::offscreen_turn_with_perspective_narrows_it", a_turn_about_the_vertical_middle_with_perspective_narrows_it),
        ("gpui::offscreen_back_face_is_left_out_when_culled", the_back_face_is_left_out_when_culled),
        ("gpui::offscreen_opacity_mixes_with_the_window", opacity_mixes_with_the_window),
        ("gpui::offscreen_mesh_draws_the_card_in_place", mesh_draws_the_card_in_place),
        ("gpui::offscreen_mesh_folds_and_culls", mesh_folds_and_culls),
    ]
}

const CARD: Bounds<gpui_kit::Pixels> = Bounds { origin: point(px(40.), px(40.)), size: size(px(200.), px(100.)) };

/// The picture (window 400 x 300 logical pixels, black) with the card drawn through `transform`, as (width, height, RGBA).
fn draw(transform: Transform3D, cull_back: bool, opacity: f32) -> (u32, u32, Vec<u8>) {
    draw_with(transform, cull_back, opacity, None)
}

/// The same, the card drawn as the mesh `(vertices, indices)` when there is one.
fn draw_with(transform: Transform3D, cull_back: bool, opacity: f32, mesh: Option<(Vec<MeshVertex>, Vec<u32>)>) -> (u32, u32, Vec<u8>) {
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let window = cx
        .open_window(size(px(400.), px(300.)), move |_, app| {
            app.new(|_| Scene { transform, cull_back, opacity, mesh })
        })
        .unwrap();
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, _| window.refresh()).ok();
    }
    let image = cx.capture_screenshot(window.into()).unwrap();
    (image.width(), image.height(), image.into_raw())
}

struct Scene {
    transform: Transform3D,
    cull_back: bool,
    opacity: f32,
    mesh: Option<(Vec<MeshVertex>, Vec<u32>)>,
}

impl gpui_kit::Render for Scene {
    fn render(&mut self, _: &mut gpui_kit::Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let (transform, cull_back, opacity, mesh) = (self.transform, self.cull_back, self.opacity, self.mesh.clone());
        div()
            .relative()
            .size_full()
            .bg(rgb(0x000000))
            .child(
                div()
                    .absolute()
                    .left(CARD.origin.x)
                    .top(CARD.origin.y)
                    .child(offscreen(OffscreenId(7), div().w(CARD.size.width).h(CARD.size.height).bg(rgb(0xff0000)))),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, _| match &mesh {
                        Some((vertices, indices)) => window.paint_textured_mesh(OffscreenId(7), vertices, indices, transform, opacity, 1.0, cull_back),
                        None => window.paint_textured_quad(OffscreenId(7), CARD, transform, opacity, 1.0, [0., 0., 1., 1.], cull_back),
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

/// The columns (in logical pixels) the red spans on the row `y`, as (first, last), if any.
fn red_span(picture: &(u32, u32, Vec<u8>), y: f32) -> Option<(f32, f32)> {
    let scale = picture.0 as f32 / 400.0;
    let row = (y * scale) as usize;
    let at = |x: usize| {
        let i = (row * picture.0 as usize + x) * 4;
        picture.2[i] > 128 && picture.2[i + 1] < 64
    };
    let xs: Vec<usize> = (0..picture.0 as usize).filter(|x| at(*x)).collect();
    Some((*xs.first()? as f32 / scale, (*xs.last()? + 1) as f32 / scale))
}

fn identity_draws_the_card_in_place() {
    let picture = draw(Transform3D::identity(), false, 1.0);
    let (a, b) = red_span(&picture, 90.0).expect("the card is drawn");
    assert!((a - 40.0).abs() <= 1.0 && (b - 240.0).abs() <= 1.0, "{a} {b}");
    assert!(red_span(&picture, 20.0).is_none());
    assert!(red_span(&picture, 160.0).is_none());
}

fn a_turn_about_the_vertical_middle_with_perspective_narrows_it() {
    // About the card's middle (140, 90): 60 degrees, the eye 600 away.
    let t = Transform3D::rotate_y_degrees(60.).then(Transform3D::perspective(600.)).about(140., 90.);
    let picture = draw(t, false, 1.0);
    let (a, b) = red_span(&picture, 90.0).expect("the card is drawn");
    // Compare the CPU projection of the same edges.
    let left = t.project(40., 90., 0.).unwrap().0;
    let right = t.project(240., 90., 0.).unwrap().0;
    assert!((a - left).abs() <= 1.5 && (b - right).abs() <= 1.5, "drawn {a}..{b}, expected {left}..{right}");
    assert!(b - a < 150.0, "narrower than the card");
    // The near edge is taller than the far one.
    let near = (0..300).filter(|y| red_span(&picture, *y as f32).is_some_and(|(s, _)| s < 140.0 + (a - 140.0) * 0.5)).count();
    let far = (0..300).filter(|y| red_span(&picture, *y as f32).is_some_and(|(_, e)| e > 140.0 + (b - 140.0) * 0.5)).count();
    assert!(near > far, "{near} {far}");
}

fn the_back_face_is_left_out_when_culled() {
    let t = Transform3D::rotate_y_degrees(180.).about(140., 90.);
    assert!(red_span(&draw(t, false, 1.0), 90.0).is_some());
    assert!(red_span(&draw(t, true, 1.0), 90.0).is_none());
    // The front face is kept.
    assert!(red_span(&draw(Transform3D::identity(), true, 1.0), 90.0).is_some());
}

fn opacity_mixes_with_the_window() {
    let picture = draw(Transform3D::identity(), false, 0.5);
    let scale = picture.0 as f32 / 400.0;
    let i = ((90.0 * scale) as usize * picture.0 as usize + (140.0 * scale) as usize) * 4;
    assert!((i32::from(picture.2[i]) - 128).abs() <= 3, "{}", picture.2[i]);
}

/// The card (40, 40, 200 x 100 logical pixels) as a grid mesh of `n` x 1 cells, its vertices `bend` px nearer the viewer at the middle
/// column (a fold), the triangles clockwise on the screen when `front`.
fn card_mesh(n: usize, bend: f32, front: bool) -> (Vec<MeshVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    for column in 0..=n {
        let u = column as f32 / n as f32;
        let z = bend * (1.0 - (2.0 * u - 1.0).abs());
        vertices.push(MeshVertex::new([40.0 + 200.0 * u, 40.0, z], [u, 0.0]));
        vertices.push(MeshVertex::new([40.0 + 200.0 * u, 140.0, z], [u, 1.0]));
    }
    let mut indices = Vec::new();
    for c in 0..n as u32 {
        let (tl, bl, tr, br) = (2 * c, 2 * c + 1, 2 * c + 2, 2 * c + 3);
        // Clockwise on the screen: top left, top right, bottom left.
        let triangles = [[tl, tr, bl], [tr, br, bl]];
        for t in triangles {
            if front {
                indices.extend_from_slice(&t);
            } else {
                indices.extend_from_slice(&[t[0], t[2], t[1]]);
            }
        }
    }
    (vertices, indices)
}

fn mesh_draws_the_card_in_place() {
    let picture = draw_with(Transform3D::identity(), false, 1.0, Some(card_mesh(4, 0.0, true)));
    let (a, b) = red_span(&picture, 90.0).expect("the mesh is drawn");
    assert!((a - 40.0).abs() <= 1.0 && (b - 240.0).abs() <= 1.0, "{a} {b}");
    assert!(red_span(&picture, 20.0).is_none() && red_span(&picture, 160.0).is_none());
}

fn mesh_folds_and_culls() {
    // The middle of the card toward the viewer by 100 px with the eye 600 away: the edges stay, the middle is drawn larger.
    let t = Transform3D::perspective(600.).about(140., 90.);
    let folded = draw_with(t, false, 1.0, Some(card_mesh(4, 100.0, true)));
    let flat = draw_with(t, false, 1.0, Some(card_mesh(4, 0.0, true)));
    let height = |picture: &(u32, u32, Vec<u8>), x: f32| {
        let scale = picture.0 as f32 / 400.0;
        let column = (x * scale) as usize;
        (0..picture.1 as usize).filter(|y| {
            let i = (y * picture.0 as usize + column) * 4;
            picture.2[i] > 128 && picture.2[i + 1] < 64
        }).count() as f32 / scale
    };
    assert!(height(&folded, 140.0) > height(&flat, 140.0) + 10.0, "{} {}", height(&folded, 140.0), height(&flat, 140.0));
    // The back faces: with culling, triangles turned the other way are not drawn, the front ones are.
    assert!(red_span(&draw_with(Transform3D::identity(), true, 1.0, Some(card_mesh(4, 0.0, false))), 90.0).is_none());
    assert!(red_span(&draw_with(Transform3D::identity(), true, 1.0, Some(card_mesh(4, 0.0, true))), 90.0).is_some());
}
