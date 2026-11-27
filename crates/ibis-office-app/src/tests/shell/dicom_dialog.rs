// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #246: 校閲 > 医療画像（DICOM）の匿名化と挿入… (`insert.medical-image`) on the document and presentation screens: the menu line opens the dialog for the
//! chosen file, rectangles are added (by the buttons, the numbers and the pointer), 挿入 paints them into the picture's pixels and puts it into the file as one step
//! that one undo takes back.

use super::Scenario;
use crate::docx::scenarios::DocSteps;
use crate::shell::chrome::MenuEntry;
use ibis_doc_model::FlowPos;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::dicom_dialog_is_a_review_line_on_the_document_and_presentation_screens", is_in_review_menus),
        ("shell::dicom_dialog_blacks_out_the_pixels_of_a_slide_picture_and_undoes", presentation_insert),
        ("shell::dicom_dialog_blacks_out_the_pixels_of_a_document_picture_and_undoes", document_insert),
        ("shell::dicom_dialog_pointer_adds_moves_and_drops_rectangles", pointer_rects),
        ("shell::dicom_dialog_real_pointer_maps_screen_to_picture_pixels", real_pointer),
        ("shell::dicom_dialog_warns_of_burned_in_annotation_and_refuses_other_files", warning_and_refusal),
    ]
}

const W: u32 = 64;
const H: u32 = 48;
const GRAY: u8 = 200;

fn element(out: &mut Vec<u8>, group: u16, element: u16, vr: &[u8; 2], value: &[u8]) {
    let mut value = value.to_vec();
    if value.len() % 2 == 1 {
        value.push(if vr == b"UI" { 0 } else { b' ' });
    }
    out.extend_from_slice(&group.to_le_bytes());
    out.extend_from_slice(&element.to_le_bytes());
    out.extend_from_slice(vr);
    if matches!(vr, b"OB" | b"OW") {
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    } else {
        out.extend_from_slice(&(value.len() as u16).to_le_bytes());
    }
    out.extend_from_slice(&value);
}

/// A DICOM Part 10 file of a 64 x 48 8-bit gray picture of one value, with a patient name and ID to be taken out (explicit VR little endian).
fn dicom(burned_in: bool) -> Vec<u8> {
    dicom_sized(W, H, burned_in)
}

fn dicom_sized(w: u32, h: u32, burned_in: bool) -> Vec<u8> {
    let mut meta = Vec::new();
    element(&mut meta, 2, 2, b"UI", b"1.2.840.10008.5.1.4.1.1.7");
    element(&mut meta, 2, 3, b"UI", b"1.2.3.4.5.6.7.8.9");
    element(&mut meta, 2, 0x10, b"UI", b"1.2.840.10008.1.2.1");
    let mut out = vec![0u8; 128];
    out.extend_from_slice(b"DICM");
    element(&mut out, 2, 0, b"UL", &(meta.len() as u32).to_le_bytes());
    out.extend_from_slice(&meta);
    element(&mut out, 8, 0x60, b"CS", b"OT");
    element(&mut out, 0x10, 0x10, b"PN", b"YAMADA^TARO");
    element(&mut out, 0x10, 0x20, b"LO", b"PAT-999999");
    element(&mut out, 0x28, 2, b"US", &1u16.to_le_bytes());
    element(&mut out, 0x28, 4, b"CS", b"MONOCHROME2");
    element(&mut out, 0x28, 0x10, b"US", &(h as u16).to_le_bytes());
    element(&mut out, 0x28, 0x11, b"US", &(w as u16).to_le_bytes());
    element(&mut out, 0x28, 0x100, b"US", &8u16.to_le_bytes());
    if burned_in {
        element(&mut out, 0x28, 0x301, b"CS", b"YES");
    }
    element(&mut out, 0x7fe0, 0x10, b"OB", &vec![GRAY; (w * h) as usize]);
    out
}

fn has_item(entries: &[MenuEntry], id: &str) -> bool {
    entries.iter().any(|e| match e {
        MenuEntry::Item(i) => i.id == id,
        MenuEntry::Submenu(s) => has_item(&s.items, id),
        _ => false,
    })
}

/// The pixels (RGB) of the newest picture file in the model that is `W x H`.
fn made_picture(s: &mut Scenario) -> image::RgbaImage {
    let png = s
        .with(|e, _| e.presentation().media.iter().rev().filter(|m| m.format == ibis_doc_model::MediaFormat::Png).map(|m| m.data.to_vec()).find(|d| ibis_formats::dicom::png_size(d) == Some((W, H))))
        .expect("the made picture is in the file");
    image::load_from_memory(&png).expect("a PNG").to_rgba8()
}

fn is_in_review_menus() {
    for file in ["test/editing/docx-report.docx", "test/benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open(file);
        let found = s.with(|e, _| e.menu_bar_menus().menus.iter().any(|(_, entries)| has_item(entries, "insert.medical-image")));
        assert!(found, "{file}: 挿入 > 画像 has the medical image line");
        let label = s.with(|_, _| crate::shell::i18n::text("insert.medical-image").to_string());
        assert!(label.contains("DICOM") && label.ends_with('…'), "{label}");
    }
}

/// Add a rectangle by the button, give it numbers, and add a second by the pointer; the burned-in file warns.
fn fill_in_the_dialog(s: &mut Scenario) {
    scenario_file(true);
    s.menu("insert.medical-image");
    assert!(s.with(|e, _| e.dicom_dialog.is_some()), "the menu line opens the dialog for the chosen file");
    assert!(s.with(|e, _| e.dicom_dialog.as_ref().unwrap().burned_in), "BurnedInAnnotation = YES is seen");
    s.update(|e, w, cx| e.dicom_add_rect(w, cx));
    // The numbers of the new rectangle (the top tenth of the picture): 8, 8, 16 x 16.
    s.update(|e, _, cx| {
        for (which, v) in [8, 8, 16, 16].into_iter().enumerate() {
            e.dicom_set_field(which, Some(v), cx);
        }
    });
    s.screenshot("dicom-dialog");
}

fn scenario_file(burned_in: bool) {
    crate::shell::dicom_dialog::scenario_file::set("case.dcm", dicom(burned_in));
}

fn check_pixels(img: &image::RgbaImage) {
    for (x, y) in [(8, 8), (23, 23), (15, 15)] {
        assert_eq!(img.get_pixel(x, y).0, [0, 0, 0, 255], "({x},{y}) is inside the area: black");
    }
    for (x, y) in [(7, 8), (24, 8), (8, 7), (8, 24), (0, 0), (63, 47), (40, 30)] {
        assert_eq!(img.get_pixel(x, y).0, [GRAY, GRAY, GRAY, 255], "({x},{y}) is outside: the picture's own value");
    }
}

fn presentation_insert() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    fill_in_the_dialog(&mut s);
    let shapes_before = s.with(|e, _| e.slide().shapes.len());
    let undo_before = s.with(|e, _| e.document.undo_len());
    let media_before = s.with(|e, _| e.presentation().media.len());
    s.update(|e, w, cx| e.apply_dicom_dialog(w, cx));
    assert!(s.with(|e, _| e.dicom_dialog.is_none()), "the dialog is closed");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), shapes_before + 1, "the picture is on the slide");
    assert_eq!(s.with(|e, _| e.document.undo_len()), undo_before + 1, "one undo step");
    assert_eq!(s.with(|e, _| e.presentation().media.len()), media_before + 1, "only the made picture was added: the file's own pixels are in none");
    check_pixels(&made_picture(&mut s));
    s.update(|e, _, _| { e.document.undo(); });
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), shapes_before, "one undo takes it back");
}

fn document_insert() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let (id, _) = s.paragraph(0);
    s.click_text(FlowPos::new(id, 0));
    fill_in_the_dialog(&mut s);
    let pictures = |s: &mut Scenario| s.with(|e, _| e.presentation().flow.as_ref().unwrap().body.iter().filter_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p) } else { None }).map(|p| p.runs.iter().filter(|r| matches!(r.kind, ibis_doc_model::FlowRunKind::Picture { .. })).count()).sum::<usize>());
    let before = pictures(&mut s);
    let undo_before = s.with(|e, _| e.document.undo_len());
    s.update(|e, w, cx| e.apply_dicom_dialog(w, cx));
    assert_eq!(pictures(&mut s), before + 1, "the picture is in the text");
    assert_eq!(s.with(|e, _| e.document.undo_len()), undo_before + 1, "one undo step");
    check_pixels(&made_picture(&mut s));
    s.update(|e, _, _| { e.document.undo(); });
    assert_eq!(pictures(&mut s), before, "one undo takes it back");
}

/// A drag over empty picture makes a rectangle (a click does not), a drag over a rectangle moves it, 選んだ範囲を消す deletes it.
fn pointer_rects() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    scenario_file(false);
    s.menu("insert.medical-image");
    assert!(!s.with(|e, _| e.dicom_dialog.as_ref().unwrap().burned_in), "no warning for a file without the mark");
    s.update(|e, w, cx| {
        e.dicom_press_px(20., 30., w, cx);
        e.dicom_drag_px(40., 40., w, cx);
        e.dicom_release(w, cx);
    });
    let rects = s.with(|e, _| e.dicom_dialog.as_ref().unwrap().rects.clone());
    assert_eq!(rects.len(), 1);
    assert_eq!((rects[0].x, rects[0].y, rects[0].width, rects[0].height), (20, 30, 20, 10));
    // A bare click is no rectangle.
    s.update(|e, w, cx| {
        e.dicom_press_px(2., 2., w, cx);
        e.dicom_release(w, cx);
    });
    assert_eq!(s.with(|e, _| e.dicom_dialog.as_ref().unwrap().rects.len()), 1);
    // Dragging inside the rectangle carries it (kept in the picture).
    s.update(|e, w, cx| {
        e.dicom_press_px(25., 35., w, cx);
        e.dicom_drag_px(100., 100., w, cx);
        e.dicom_release(w, cx);
    });
    let r = s.with(|e, _| e.dicom_dialog.as_ref().unwrap().rects[0]);
    assert_eq!((r.x + r.width, r.y + r.height), (W, H), "carried to the corner and no further");
    s.update(|e, w, cx| e.dicom_delete_rect(w, cx));
    assert!(s.with(|e, _| e.dicom_dialog.as_ref().unwrap().rects.is_empty()));
    s.update(|e, w, cx| e.close_dialog(w, cx));
    assert!(s.with(|e, _| e.dicom_dialog.is_none()), "cancel drops the dialog's state");
}

fn warning_and_refusal() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    crate::shell::dicom_dialog::scenario_file::set("notes.txt", b"not a dicom file".to_vec());
    s.menu("insert.medical-image");
    assert!(s.with(|e, _| e.dicom_dialog.is_none() && e.alert.is_some()), "a file that is not DICOM is told so in a message box");
}

/// Drag with the real mouse events over the painted picture and check the screen point -> picture pixel conversion and the number fields,
/// for a picture of another aspect than the room (4:3 and tall), and after the window is made narrower and wider.
fn real_pointer() {
    for (w, h) in [(64u32, 48u32), (48, 96), (200, 40)] {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        crate::shell::dicom_dialog::scenario_file::set("case.dcm", dicom_sized(w, h, false));
        s.menu("insert.medical-image");
        for window_size in [None, Some((900., 700.)), Some((1400., 900.))] {
            if let Some((ww, hh)) = window_size {
                s.size(ww, hh);
            }
            s.update(|e, win, cx| {
                while e.dicom_dialog.as_ref().is_some_and(|d| !d.rects.is_empty()) {
                    e.dicom_delete_rect(win, cx);
                    // delete works on the chosen one; choose the next
                    e.dicom_select_rect(0, win, cx);
                }
            });
            let b = s.with(|e, _| e.dicom_picture_bounds()).expect("painted");
            let (ox, oy, bw, bh) = (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height));
            let (sx, sy) = (bw / w as f32, bh / h as f32);
            assert!((sx - sy).abs() < 0.01, "{w}x{h}: the picture keeps its aspect ({sx} vs {sy})");
            // From pixel (w/4, h/4) to (3w/4, h/2).
            let from = (ox + (w as f32 / 4.) * sx, oy + (h as f32 / 4.) * sy);
            let to = (ox + (3. * w as f32 / 4.) * sx, oy + (h as f32 / 2.) * sy);
            s.drag(from, to);
            let rects = s.with(|e, _| e.dicom_dialog.as_ref().unwrap().rects.clone());
            assert_eq!(rects.len(), 1, "{w}x{h}: one rectangle");
            let r = rects[0];
            let near = |a: u32, b: f32| (a as f32 - b).abs() <= 1.01;
            assert!(near(r.x, w as f32 / 4.) && near(r.y, h as f32 / 4.) && near(r.x + r.width, 3. * w as f32 / 4.) && near(r.y + r.height, h as f32 / 2.), "{w}x{h} {window_size:?}: {r:?}");
            let texts = s.with(|e, cx| e.dicom_field_texts(cx));
            assert_eq!(texts, vec![r.x.to_string(), r.y.to_string(), r.width.to_string(), r.height.to_string()], "the fields follow the drag");
            // Move it by dragging from its middle by (+3, +2) pixels of the picture.
            let mid = (ox + (r.x as f32 + r.width as f32 / 2.) * sx, oy + (r.y as f32 + r.height as f32 / 2.) * sy);
            s.drag(mid, (mid.0 + 3. * sx, mid.1 + 2. * sy));
            let m = s.with(|e, _| e.dicom_dialog.as_ref().unwrap().rects[0]);
            assert!((m.x as i64 - r.x as i64 - 3).abs() <= 1 && (m.y as i64 - r.y as i64 - 2).abs() <= 1, "{w}x{h}: moved {r:?} -> {m:?}");
            assert_eq!((m.width, m.height), (r.width, r.height), "moving keeps the size");
            let texts = s.with(|e, cx| e.dicom_field_texts(cx));
            assert_eq!(texts[0], m.x.to_string());
            assert_eq!(texts[1], m.y.to_string());
            // Typing into the fields moves the rectangle.
            for (which, v) in [(0usize, 2u32), (1, 3), (2, 5), (3, 6)] {
                s.update(|e, win, cx| e.dicom_focus_field(which, win, cx));
                s.key("cmd-a");
                s.type_text(&v.to_string());
            }
            let t = s.with(|e, _| e.dicom_dialog.as_ref().unwrap().rects[0]);
            assert_eq!((t.x, t.y, t.width, t.height), (2, 3, 5.min(w - 2), 6.min(h - 3)), "{w}x{h}: typed numbers reach the rectangle");
        }
    }
}
