// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-04 プレゼンテーション画面（PPTX）のダイアログの手直し（#06 #09 #24 #32 #37 #42 #44）。
use crate::shell::chrome::{BUTTON_HEIGHT, FIELD_WIDTH};
use crate::shell::scenario::{button_probe, Scenario};
use gpui_kit::{Bounds, Pixels};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::dialog_polish_qat_reset_label", qat_reset_label),
        ("pptx::dialog_polish_photo_album_dropdowns", photo_album_dropdowns),
        ("pptx::dialog_polish_print_layout_dropdown_and_segments", print_layout_dropdown_and_segments),
        ("pptx::dialog_polish_header_footer_action_order", header_footer_action_order),
        ("pptx::dialog_polish_link_tooltip_wording", link_tooltip_wording),
        ("pptx::dialog_polish_paragraph_label_and_dropdown_width", paragraph_label_and_dropdown_width),
        ("pptx::dialog_polish_theme_editor_label_width", theme_editor_label_width),
    ]
}

fn bounds_of(s: &mut Scenario, id: &str) -> Bounds<Pixels> {
    let _probe = button_probe::Probe::new();
    s.update(|_, _, cx| cx.notify());
    button_probe::bounds(id).unwrap_or_else(|| panic!("missing {id}: {:?}", button_probe::all()))
}
/// Presses the button or list item `id` (drawn first, so the probe knows where it is).
fn press(s: &mut Scenario, id: &str) {
    let _probe = button_probe::Probe::new();
    s.update(|_, _, cx| cx.notify());
    button_probe::click(s, id);
    s.settle();
}
fn rows_of(s: &mut Scenario, prefix: &str) -> Vec<(String, Bounds<Pixels>)> {
    let _probe = button_probe::Probe::new();
    s.update(|_, _, cx| cx.notify());
    let mut rows: Vec<_> = button_probe::all().into_iter().filter(|id| id.contains(prefix)).filter_map(|id| button_probe::bounds(&id).map(|b| (id, b))).collect();
    rows.sort_by(|a, b| a.1.origin.y.partial_cmp(&b.1.origin.y).unwrap());
    rows
}
/// With `IBIS_POLISH_SHOTS=<directory>` the window is saved there as `<name>.png` (for the report's pictures).
fn shot(s: &mut Scenario, name: &str) {
    let Ok(dir) = std::env::var("IBIS_POLISH_SHOTS") else { return };
    let (w, raw) = s.picture();
    let h = raw.len() as u32 / (w * 4);
    image::RgbaImage::from_raw(w, h, raw).expect("rgba image").save(std::path::Path::new(&dir).join(format!("{name}.png"))).expect("save");
}
fn f(v: Pixels) -> f32 { f32::from(v) }
/// What the probe leaves out of a dropdown button: its border (1 px each side) and its horizontal padding (8 px each side).
const BUTTON_INSET_X: f32 = 18.;
const BUTTON_INSET_Y: f32 = 2.;
fn near(a: f32, b: f32) { assert!((a - b).abs() < 0.6, "{a} != {b}"); }

/// #06: the QAT reset button reads 既定にリセット (not 規程).
fn qat_reset_label() {
    let label = crate::shell::i18n::text("pptx.customize-ribbon.qat-reset").to_string();
    assert!(!label.contains("QAT") && !label.contains("規程"), "{label}"); // i18n-ok（確認する文言そのもの）
    assert!(label == "既定にリセット" || label == "Reset to Default", "{label}"); // i18n-ok（確認する文言そのもの）
}

/// #09: the album layout and frame dropdowns are 1.5 times the usual width; the frame list's items are 1.5 times as high, the button is not.
fn photo_album_dropdowns() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, w, cx| e.open_photo_album_panel(w, cx));
    let layout = bounds_of(&mut s, "pptx-album-layout");
    near(f(layout.size.width) + BUTTON_INSET_X, f(FIELD_WIDTH) * 1.5);
    let frame = bounds_of(&mut s, "pptx-album-frame");
    near(f(frame.size.width) + BUTTON_INSET_X, f(FIELD_WIDTH) * 1.5);
    near(f(frame.size.height) + BUTTON_INSET_Y, BUTTON_HEIGHT);
    near(f(layout.origin.x), f(frame.origin.x));
    press(&mut s, "pptx-album-frame");
    let rows = rows_of(&mut s, "searchable-item-pptx-album-frame");
    assert_eq!(rows.len(), 7, "{rows:?}");
    shot(&mut s, "pptx-polish-photo-album-frame-list");
    for (_, b) in &rows { near(f(b.size.height), BUTTON_HEIGHT * 1.5); }
}

/// #24: the print layout is a dropdown (button and items 2.5 times as high, each with a 4:3 picture), the landscape check is above it, and the
/// colour is a segment control.
fn print_layout_dropdown_and_segments() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("file.print");
    let landscape = bounds_of(&mut s, "pptx-print-landscape");
    let layout = bounds_of(&mut s, "pptx-print-layout");
    near(f(layout.size.height) + BUTTON_INSET_Y, BUTTON_HEIGHT * 2.5);
    assert!(f(landscape.origin.y) + f(landscape.size.height) <= f(layout.origin.y) + 0.5, "the landscape check is before (above) the dropdown");
    let (a, b, c) = (bounds_of(&mut s, "pptx-print-color"), bounds_of(&mut s, "pptx-print-gray"), bounds_of(&mut s, "pptx-print-bw"));
    near(f(a.origin.y), f(b.origin.y)); near(f(b.origin.y), f(c.origin.y));
    assert!(a.origin.x < b.origin.x && b.origin.x < c.origin.x, "one segment control");
    assert!(f(c.origin.y) > f(layout.origin.y), "the colour is under the layout");
    press(&mut s, "pptx-print-layout");
    let rows = rows_of(&mut s, "searchable-item-pptx-print-layout");
    assert_eq!(rows.len(), 9, "{rows:?}");
    shot(&mut s, "pptx-polish-print-layout-list");
    for (_, b) in &rows { near(f(b.size.height), BUTTON_HEIGHT * 2.5); }
    // Choosing 配布資料 (2) (the list is over the dialog, so the row is pressed where it is drawn) and the colour segment.
    let b = rows[4].1;
    s.click((f(b.origin.x + b.size.width / 2.), f(b.origin.y + b.size.height / 2.)));
    s.settle();
    assert_eq!(s.with(|e, _| e.pptx().unwrap().print_panel.as_ref().map(|p| p.settings.handout)), Some(Some(2)));
    press(&mut s, "pptx-print-bw");
    assert_eq!(s.with(|e, _| e.pptx().unwrap().print_panel.as_ref().map(|p| p.settings.color)), Some(crate::print_panel::PrintColorKind::BlackWhite));
}

/// #37: すべてに適用, 適用, キャンセル.
fn header_footer_action_order() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, w, cx| e.open_header_footer(w, cx));
    let (all, apply, cancel) = (bounds_of(&mut s, "pptx-hf-all"), bounds_of(&mut s, "pptx-hf-apply"), bounds_of(&mut s, "pptx-hf-cancel"));
    assert!(all.origin.x < apply.origin.x && apply.origin.x < cancel.origin.x);
}

/// #42: ヒント（ScreenTip） is ツールチップ.
fn link_tooltip_wording() {
    for id in ["pptx.links.screentip", "pptx.links.screentip-text"] {
        let t = crate::shell::i18n::text(id).to_string();
        assert!(!t.contains("ヒント"), "{id}: {t}"); // i18n-ok（確認する文言そのもの）
    }
}

/// #32: the label column and the font-alignment dropdown are 1.5 times the usual width.
fn paragraph_label_and_dropdown_width() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.select_shape(0);
    s.update(|e, w, cx| e.open_paragraph_panel(w, cx));
    let dd = bounds_of(&mut s, "pptx-paragraph-font-align");
    near(f(dd.size.width) + BUTTON_INSET_X, f(FIELD_WIDTH) * 1.5);
    let dialog = s.with(|e, _| e.dialog.bounds).expect("dialog bounds");
    assert!(f(dd.origin.x) - f(dialog.origin.x) >= f(FIELD_WIDTH) * 1.5, "label column at least 1.5 times");
}

/// #44: the custom colour editor's label column is 1.6 times the usual width.
fn theme_editor_label_width() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, w, cx| { e.open_theme_panel(w, cx); e.theme_colors_ui.editing = true; cx.notify(); });
    let field = bounds_of(&mut s, "theme-color-0-swatch");
    let dialog = s.with(|e, _| e.dialog.bounds).expect("dialog bounds");
    assert!(f(field.origin.x) - f(dialog.origin.x) >= f(FIELD_WIDTH) * 1.6, "label column at least 1.6 times");
}
