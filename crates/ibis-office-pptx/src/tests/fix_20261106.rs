// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-06: fixes found by eye before the release: the placeholder's buttons (the toolbar's size), cropping a picture
//! from the menu or the panel (the part that stays is not stretched), turning a picture with its handle, and the format
//! pane's changes to a picture. Pictures are in `target/scenario/<name>/`.
use crate::shell::scenario::Scenario;
use crate::scenarios::SlideSteps;
use ibis_doc_model::{Crop, EmuPoint, ShapeId, ShapeKind};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::fix_20261106_placeholder_buttons_are_the_toolbars_size", placeholder_buttons),
        ("pptx::fix_20261106_menu_crop_keeps_the_part_that_stays", menu_crop),
        ("pptx::fix_20261106_picture_turns_with_its_handle", picture_turns),
        ("pptx::fix_20261106_format_pane_changes_a_picture", pane_changes_picture),
    ]
}

/// The plan's slide 3 with a 120 x 60 picture dropped on it (selected).
fn with_picture() -> (Scenario, ShapeId) {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(3);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/scenario-drop");
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("fix-20261106.png");
    let mut out = Vec::new();
    // Left half red, right half blue: a stretch shows as a changed boundary.
    let image = image::RgbaImage::from_fn(120, 60, |x, y| if x < 60 { image::Rgba([220, 40, 40, 255]) } else if y < 30 { image::Rgba([40, 40, 220, 255]) } else { image::Rgba([40, 180, 220, 255]) });
    image::DynamicImage::ImageRgba8(image).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    std::fs::write(&png, out).unwrap();
    let target = s.at(EmuPoint::new(6_000_000, 3_000_000));
    s.drop_files(target, vec![png]);
    let id = s.with(|e, _| e.selected_shapes().first().map(|sh| (matches!(sh.kind, ShapeKind::Picture(_)), sh.id))).expect("selected");
    assert!(id.0, "the picture is selected");
    (s, id.1)
}

fn placeholder_buttons() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 900.);
    // A new slide in the layout with a content placeholder: its four buttons (table, chart, picture, video).
    let layout = s.with(|e, _| e.presentation().layouts.iter().find(|l| l.name.contains("コンテンツ") || l.name.contains("Content")).or(e.presentation().layouts.get(1)).or(e.presentation().layouts.first()).map(|l| l.id)).expect("a layout"); // i18n-ok（試験・ファイルの中の日本語のデータ）
    s.update(move |e, w, cx| e.add_slide_with_layout(layout, w, cx));
    s.screenshot("placeholder-buttons");
}

fn menu_crop() {
    use crate::shell::crop_mode::full_extent;
    let (mut s, id) = with_picture();
    let state = |s: &mut Scenario| s.with(|e, _| { let sh = e.slide().shape(id).unwrap(); let ShapeKind::Picture(p) = &sh.kind else { panic!() }; (sh.frame, p.crop) });
    let (frame0, crop0) = state(&mut s);
    let whole = full_extent(frame0, crop0).unwrap();
    s.screenshot("crop-before");
    for crop in [Crop { left: 20_000, ..Default::default() }, Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 }, Crop { right: 30_000, bottom: 25_000, ..Default::default() }] {
        s.update(move |e, _, cx| e.crop_selected_pictures(crop, cx));
        let (frame, c) = state(&mut s);
        assert_eq!(c, crop);
        let now = full_extent(frame, c).unwrap();
        let near = |a: i64, b: i64| (a - b).abs() <= 2;
        assert!(near(now.x, whole.x) && near(now.y, whole.y) && near(now.width, whole.width) && near(now.height, whole.height), "the whole picture stays: {whole:?} -> {now:?} ({crop:?}, frame {frame:?})");
    }
    s.screenshot("crop-after");
    // Reset gives the whole picture back at its place.
    s.update(|e, _, cx| e.crop_selected_pictures(Crop::default(), cx));
    let (frame, _) = state(&mut s);
    assert_eq!(frame, frame0);
    // An aspect ratio, from the menu's own entry.
    s.update(|e, _, cx| e.crop_selected_pictures_aspect(1, 1, cx));
    let (frame, c) = state(&mut s);
    let now = full_extent(frame, c).unwrap();
    assert!((now.width - whole.width).abs() <= 2 && (now.height - whole.height).abs() <= 2 && (frame.width - frame.height).abs() <= 2, "{frame:?} {c:?}");
    s.screenshot("crop-aspect");
}

fn picture_turns() {
    let (mut s, id) = with_picture();
    s.screenshot("turn-before");
    let handle = |s: &mut Scenario| {
        let (f, r) = s.with(|e, _| { let sh = e.slide().shape(id).unwrap(); (sh.frame, sh.rotation) });
        let scale = s.with(|e, _| e.slide_layout().unwrap().scale) as f64;
        let p = crate::interaction::rotation_handle_point(f, r, crate::interaction::ROTATE_HANDLE_PX as f64 / scale);
        s.at(EmuPoint::new(p.x.round() as i64, p.y.round() as i64))
    };
    let from = handle(&mut s);
    s.drag(from, (from.0 + 80.0, from.1 + 40.0));
    let r = s.with(|e, _| e.slide().shape(id).unwrap().rotation);
    s.wait(300);
    s.screenshot("turn-after");
    assert_ne!(r, 0, "the picture turned");
    // It is drawn turned: the picture put in after the file was opened has its data to wrap in SVG (as a file's picture has).
    let drawn = s.with(|e, _| { let ShapeKind::Picture(p) = &e.slide().shape(id).unwrap().kind else { panic!() }; e.resources.source(p.media).is_some() });
    assert!(drawn, "the inserted picture can be drawn turned");
}

fn pane_changes_picture() {
    use gpui_kit::component::input::InputEvent;
    let (mut s, id) = with_picture();
    s.size(1200., 900.);
    s.update(|e, _, cx| { e.sidebar_open = true; cx.notify(); });
    s.screenshot("pane-before");
    let set = |s: &mut Scenario, index: usize, value: &str| {
        let field = s.with(|e, _| e.pptx().unwrap().sidebar_settings.as_ref().expect("the settings").fields[index].clone());
        let value = value.to_string();
        s.update(move |_, w, cx| field.update(cx, |f, cx| { f.set_value(value, w, cx); cx.emit(InputEvent::Change); }));
    };
    let before = s.with(|e, _| e.slide().shape(id).unwrap().frame);
    set(&mut s, 0, "9");
    let after = s.with(|e, _| e.slide().shape(id).unwrap().frame);
    assert_eq!(after.width, 3_240_000, "the width typed in the pane: {before:?} -> {after:?}");
    set(&mut s, 2, "30");
    assert_eq!(s.with(|e, _| e.slide().shape(id).unwrap().rotation), 1_800_000, "the rotation typed in the pane");
    set(&mut s, 3, "5");
    assert_eq!(s.with(|e, _| e.slide().shape(id).unwrap().frame.x), 1_800_000, "x typed in the pane");
    s.screenshot("pane-after");
}
