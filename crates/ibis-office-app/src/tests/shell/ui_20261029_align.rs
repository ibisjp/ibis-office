// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Right-edge alignment of the chart pane's controls and the menu bar's item shapes (2026-10-29).
use super::{Scenario, button_probe as buttons};
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;

fn edges(id: &str) -> (f32, f32) {
    let b = buttons::bounds(id).unwrap_or_else(|| panic!("{id} was not drawn; drawn: {:?}", buttons::all()));
    (f32::from(b.origin.x + b.size.width), f32::from(b.size.width))
}
fn chart_pane() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    for (cell, value) in [("A1", "x"), ("B1", "y"), ("A2", "1"), ("B2", "2")] {
        s.click_cell(cell);
        s.enter(value, "enter");
    }
    s.update(|e, w, cx| e.go_to_reference("A1:B2", w, cx));
    s.menu("pptx.insert-chart.line");
    s.update(|e, w, cx| e.open_sheet_chart_panel(w, cx));
    let tab = format!("{:?}", ElementId::from(("chart-format-tab", 2usize)));
    buttons::click(&mut s, &tab);
    s.update(|_, _, cx| cx.notify());
    s.settle();
    s.screenshot("chart-pane-edges");
    let (range, _) = edges("chart-global-range-field");
    // Pixels: scan each control's vertical centre from the pane's right side inwards for its border.
    let (width, rgba) = s.picture();
    let logical = std::rc::Rc::new(std::cell::Cell::new(1.0f32));
    let seen = logical.clone();
    s.update(move |_, w, _| seen.set(f32::from(w.viewport_size().width)));
    let scale = width as f32 / logical.get();
    let right_pixel = |id: &str| {
        let b = buttons::bounds(id).unwrap();
        let y = (f32::from(b.center().y) * scale) as usize;
        let bg = |x: usize| { let i = (y * width as usize + x) * 4; [rgba[i], rgba[i + 1], rgba[i + 2]] };
        let from = ((range + 6.) * scale) as usize;
        let reference = bg(from);
        (from - (12. * scale) as usize..from).rev().find(|x| bg(*x).iter().zip(reference).any(|(a, b)| a.abs_diff(b) > 6)).map(|x| x as f32 / scale)
    };
    let (pr, pc) = (right_pixel("chart-global-range-field"), right_pixel("chart-palette-dropdown"));
    let (pr, pc) = (pr.expect("range border"), pc.expect("colors border"));
    assert!((pr - pc).abs() <= 1.0, "chart colors right edge {pc} == data range right edge {pr} (pixels)");
}
/// The logo and a menu name are hovered in turn: the same height, the same top, and both are `menu_bar_button`s
/// (one definition of the corner radius, the padding and the hover colour).
fn menu_bar_hover() {
    use crate::shell::menu_bar::MENU_BAR_BUTTON_HEIGHT;
    for kind in [ibis_pptx::Kind::Document, ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let _probe = buttons::Probe::new();
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        s.update(|_, _, cx| cx.notify());
        s.settle();
        let logo = buttons::bounds("editor-app-logo").unwrap_or_else(|| panic!("{kind:?}: no logo; drawn: {:?}", buttons::all()));
        let file = buttons::bounds("menu-button-menu.file").unwrap_or_else(|| panic!("{kind:?}: no File; drawn: {:?}", buttons::all()));
        s.hover((f32::from(logo.center().x), f32::from(logo.center().y)));
        s.screenshot(&format!("menu-hover-logo-{kind:?}"));
        s.hover((f32::from(file.center().x), f32::from(file.center().y)));
        s.screenshot(&format!("menu-hover-file-{kind:?}"));
        assert_eq!(f32::from(logo.size.height), MENU_BAR_BUTTON_HEIGHT, "{kind:?} logo hover height");
        assert_eq!(logo.size.height, file.size.height, "{kind:?} logo and File hover heights");
        assert_eq!(logo.origin.y, file.origin.y, "{kind:?} logo and File hover tops");
    }
}
/// The format's icon in the title bar is only a mark: it is not a button (the start window opens from the logo).
fn title_icon_is_not_a_button() {
    for kind in [ibis_pptx::Kind::Document, ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let _probe = buttons::Probe::new();
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        s.update(|_, _, cx| cx.notify());
        s.settle();
        assert!(buttons::bounds("start-window").is_none(), "{kind:?}: the title bar icon is not a button; drawn: {:?}", buttons::all());
        assert!(buttons::bounds("editor-app-logo").is_some(), "{kind:?}: the logo is still a button");
    }
}
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::ui20261029_chart_pane_right_edge", chart_pane), ("shell::ui20261029_menu_bar_hover", menu_bar_hover), ("shell::ui20261029_title_icon_is_not_a_button", title_icon_is_not_a_button)]
}
