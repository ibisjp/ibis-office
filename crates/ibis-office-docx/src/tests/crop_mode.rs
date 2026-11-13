// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-04: the crop of a picture on the page of the document screen (the same crop mode as the presentation and
//! workbook screens, `shell/crop_mode.rs`): an inline picture and an anchored one, kept, undone, given up, saved
//! (`a:srcRect`) and read back; and the numeric crop in the picture panel with a unit and the size left.
use crate::shell::crop_units::CropUnit;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Block, Crop, FlowPos, FlowRunKind, PictureWrapType, ShapeKind};
use crate::ext::*;

const REPORT: &str = "test/editing/docx-report.docx";

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::crop_mode_inline_picture_on_the_page", inline_picture),
        ("docx::crop_mode_anchored_picture_on_the_page", anchored_picture),
        ("docx::crop_mode_picture_panel_unit_and_size", panel_unit_and_size),
    ]
}

/// What the picture at `at` shows: its crop, its size (EMU), and the offset from its anchor when it is anchored.
#[derive(Clone, Debug, PartialEq)]
struct Seen {
    crop: Crop,
    size: (i64, i64),
    offset: Option<(i64, i64)>,
}

fn seen_in(doc: &ibis_doc_model::FlowDocument, para: u32) -> Option<Seen> {
    let Some(Block::Paragraph(p)) = doc.body.iter().find(|b| matches!(b, Block::Paragraph(p) if p.id == para)) else { return None };
    p.runs.iter().find_map(|r| match &r.kind {
        FlowRunKind::Picture { size, look, .. } => Some(Seen { crop: look.crop, size: (size.width, size.height), offset: None }),
        FlowRunKind::Shape(shape) => match &shape.kind {
            ShapeKind::Picture(pic) => Some(Seen { crop: pic.crop, size: (shape.frame.width, shape.frame.height), offset: shape.floating.as_ref().map(|f| (f.offset_x, f.offset_y)) }),
            _ => None,
        },
        _ => None,
    })
}

fn seen(s: &mut Scenario, para: u32) -> Seen {
    s.with(|e, _| seen_in(e.presentation().flow.as_deref().expect("a document"), para)).expect("a picture in the paragraph")
}

/// A paragraph of the report with text to put the picture in, and the report's own PNG.
fn picture_in_the_report(s: &mut Scenario) -> (u32, Vec<u8>) {
    let data: Vec<u8> = s.with(|e, _| e.presentation().media.iter().find(|m| m.format == ibis_doc_model::MediaFormat::Png).map(|m| m.data.to_vec())).expect("a PNG in the report");
    let id = s.with(|e, _| {
        let doc = e.presentation().flow.as_deref().expect("a document");
        doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).filter(|p| p.text().chars().count() >= 6).nth(6).map(|p| p.id)
    }).expect("a text paragraph");
    s.update(|e, _, cx| e.set_caret(FlowPos::new(id, 2), false, cx));
    s.update(|e, _, cx| e.doc_insert_picture(data.clone(), ibis_doc_model::MediaFormat::Png, cx));
    (id, data)
}

/// The window position of a place (EMU of the page) of the page the picture is on.
fn on_screen(s: &mut Scenario, page: usize, x: i64, y: i64) -> (f32, f32) {
    let (origin, k) = s.with(|e, _| e.page_origin(page)).expect("the page is placed");
    (f32::from(origin.x) + x as f32 / 12_700. * k, f32::from(origin.y) + y as f32 / 12_700. * k)
}

/// Enter crop mode on the picture at `at`, drag the left edge's handle a tenth of the picture's width to the right
/// and return the crop that gives.
fn drag_left_edge(s: &mut Scenario, at: FlowPos) -> Crop {
    s.update(|e, _, cx| e.set_caret(at, false, cx));
    s.update(|e, _, cx| {
        let lines = e.flow_lines();
        let (page, _, y, h) = e.caret_box(&lines, at).expect("laid out");
        e.scroll_into_view(page, y, h, cx);
    });
    s.menu("docx.picture.crop-mode");
    assert!(s.with(|e, _| e.is_crop_mode()), "the crop mode of the picture is on");
    s.settle();
    let page = s.with(|e, _| e.floating_text_box_frame_of(at)).expect("the picture is laid out").0;
    let kept = s.with(|e, _| e.crop_mode_kept()).expect("crop mode");
    let from = on_screen(s, page, kept.x, kept.y + kept.height / 2);
    let to = on_screen(s, page, kept.x + kept.width / 10, kept.y + kept.height / 2);
    s.drag(from, to);
    s.with(|e, _| e.crop_mode_crop()).expect("crop mode")
}

fn inline_picture() {
    let mut s = Scenario::open(REPORT);
    let (id, _) = picture_in_the_report(&mut s);
    let at = FlowPos::new(id, 2);
    let original = seen(&mut s, id);
    assert_eq!(original.crop, Crop::default());
    let crop = drag_left_edge(&mut s, at);
    assert!((crop.left - 10_000).abs() <= 600 && crop.top == 0 && crop.right == 0 && crop.bottom == 0, "{crop:?}");
    s.screenshot("docx-crop-on-the-page");
    // Enter keeps it: the crop and the size of the kept part (the height stays), one step to undo.
    s.key("enter");
    assert!(!s.with(|e, _| e.is_crop_mode()));
    let kept = seen(&mut s, id);
    assert_eq!(kept.crop, crop);
    assert_eq!(kept.size.1, original.size.1);
    let expected_w = (original.size.0 as f64 * (1. - f64::from(crop.left) / 100_000.)).round() as i64;
    assert!((kept.size.0 - expected_w).abs() <= 2, "{} vs {expected_w}", kept.size.0);
    // Saved as `a:srcRect` and read back.
    s.save_as("crop-mode-inline.docx");
    let saved = s.reopen();
    assert_eq!(seen_in(saved.flow.as_deref().expect("a document"), id).map(|v| v.crop), Some(crop));
    s.menu("edit.undo");
    assert_eq!(seen(&mut s, id), original);
    // Cropped again from the cropped picture: its whole picture is recovered from the frame and the crop; Escape gives up.
    s.menu("edit.redo");
    let second = drag_left_edge(&mut s, at);
    assert!(second.left > crop.left, "{second:?}");
    s.key("escape");
    assert!(!s.with(|e, _| e.is_crop_mode()));
    assert_eq!(seen(&mut s, id), kept);
    // A press outside the crop frame keeps the crop.
    let again = drag_left_edge(&mut s, at);
    let page = s.with(|e, _| e.floating_text_box_frame_of(at)).expect("laid out").0;
    let kept_now = s.with(|e, _| e.crop_mode_kept()).expect("crop mode");
    let outside = on_screen(&mut s, page, kept_now.x, kept_now.y + kept_now.height + 12 * 12_700);
    s.click(outside);
    assert!(!s.with(|e, _| e.is_crop_mode()));
    assert_eq!(seen(&mut s, id).crop, again);
}

fn anchored_picture() {
    let mut s = Scenario::open(REPORT);
    let (id, _) = picture_in_the_report(&mut s);
    let at = FlowPos::new(id, 2);
    s.update(|e, _, cx| e.set_picture_wrap(PictureWrapType::InFrontOfText, cx));
    s.update(|e, _, cx| e.set_caret(at, false, cx));
    let original = seen(&mut s, id);
    let offset = original.offset.expect("anchored");
    let crop = drag_left_edge(&mut s, at);
    assert!(crop.left > 0, "{crop:?}");
    s.key("enter");
    let kept = seen(&mut s, id);
    assert_eq!(kept.crop, crop);
    // The left edge moved by the part cut away; the right edge and the top stay.
    let cut = (original.size.0 as f64 * f64::from(crop.left) / 100_000.).round() as i64;
    let offset_now = kept.offset.expect("still anchored");
    assert!((offset_now.0 - (offset.0 + cut)).abs() <= 2, "{offset_now:?} from {offset:?} cut {cut}");
    assert_eq!(offset_now.1, offset.1);
    assert!(((kept.size.0 + cut) - original.size.0).abs() <= 2);
    s.save_as("crop-mode-anchored.docx");
    let saved = s.reopen();
    assert_eq!(seen_in(saved.flow.as_deref().expect("a document"), id).map(|v| v.crop), Some(crop));
    s.menu("edit.undo");
    assert_eq!(seen(&mut s, id), original);
}

fn panel_unit_and_size() {
    let mut s = Scenario::open(REPORT);
    let (id, _) = picture_in_the_report(&mut s);
    s.update(|e, _, cx| e.set_caret(FlowPos::new(id, 2), false, cx));
    s.update(|e, w, cx| e.open_picture_edit(w, cx));
    let (fields, unit) = s.with(|e, _| {
        let edit = e.doc_state().unwrap().picture_edit.as_ref().expect("the panel");
        (edit.crop.clone(), edit.crop_unit)
    });
    assert_eq!(unit, CropUnit::Percent);
    // 20 % off the top: applied as it is typed (percent first).
    s.update(|_, w, cx| fields[0].update(cx, |f, cx| f.set_value("20", w, cx)));
    s.update(|e, _, cx| e.apply_picture_crop(cx));
    assert_eq!(seen(&mut s, id).crop, Crop { top: 20_000, ..Default::default() });
    // The same crop in pixels is a fifth of the picture's height; typing pixels cuts that much.
    s.update(|e, w, cx| e.set_picture_crop_unit(CropUnit::Pixels, w, cx));
    let (px_text, source) = s.with(|e, cx| {
        let edit = e.doc_state().unwrap().picture_edit.as_ref().unwrap();
        (edit.crop[0].read(cx).value().to_string(), e.picture_at_caret().map(|(_, media, size)| e.presentation().media(media).and_then(|m| crate::editing::picture_pixels(&m.data)).map_or(size.height as f64 / 9525., |(_, h)| f64::from(h))).unwrap())
    });
    let typed: f64 = px_text.parse().expect("a number");
    assert!((typed - source * 0.2).abs() < 0.1, "{typed} vs {}", source * 0.2);
    s.update(|_, w, cx| fields[1].update(cx, |f, cx| f.set_value(format!("{}", (source * 0.1).round()), w, cx)));
    s.update(|e, _, cx| e.apply_picture_crop(cx));
    let crop = seen(&mut s, id).crop;
    assert_eq!(crop.top, 20_000, "{crop:?}");
    assert!((crop.bottom - 10_000).abs() <= (100_000. / source) as i32 + 1, "{crop:?}");
    s.screenshot("docx-crop-panel-px");
    // Going back to percent shows the same crop again.
    s.update(|e, w, cx| e.set_picture_crop_unit(CropUnit::Percent, w, cx));
    let top = s.with(|e, cx| e.doc_state().unwrap().picture_edit.as_ref().unwrap().crop[0].read(cx).value().to_string());
    assert_eq!(top, "20");
}
