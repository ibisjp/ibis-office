// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The テーブルのデザイン panel's styles as 4:3 previews in two columns.

use crate::shell::scenario::{Scenario, button_probe as buttons};
use ibis_doc_model::sheet_calc::sheet_of_slide;

use super::SheetSteps;

/// Scroll the panel until the style's preview is inside the window.
fn reveal(s: &mut Scenario, id: &str) {
    for _ in 0..40 {
        let b = buttons::all().into_iter().find(|k| k.contains(id)).and_then(|k| buttons::bounds(&k)).unwrap_or_else(|| panic!("{id} not drawn"));
        if f32::from(b.origin.y + b.size.height) < 960. && f32::from(b.origin.y) > 300. {
            return;
        }
        s.wheel((1250., 600.), -150.);
        s.settle();
    }
    panic!("{id} not reachable");
}

fn style_name(s: &mut Scenario) -> String {
    s.with(|e, _| sheet_of_slide(e.slide()).unwrap().tables[0].style.clone().map(|x| x.name).unwrap_or_default())
}

pub(crate) fn previews_two_columns_click_and_frame() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1400., 1000.);
    s.menu("xlsx.insert-sheet");
    for (row, values) in [["item", "qty", "amount"], ["a", "1", "100"], ["b", "5", "250"], ["c", "3", "60"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("A2");
    s.key("cmd-t");
    s.key("enter");
    s.menu("xlsx.table-design");
    s.settle();
    let id = |name: &str| format!("table-style-{name}");
    let find = |name: &str| buttons::all().into_iter().find(|k| k.contains(&id(name))).unwrap_or_else(|| panic!("{name} not drawn"));
    // 4:3, two columns: the first two share a row, the third is below the first.
    let a = buttons::bounds(&find("TableStyleLight1")).unwrap();
    let b = buttons::bounds(&find("TableStyleLight2")).unwrap();
    let c = buttons::bounds(&find("TableStyleLight3")).unwrap();
    let ratio = f32::from(a.size.width) / f32::from(a.size.height);
    assert!((ratio - 4. / 3.).abs() < 0.02, "4:3, got {ratio}");
    assert_eq!(a.origin.y, b.origin.y, "first row holds two");
    assert!(b.origin.x > a.origin.x, "second column to the right");
    assert_eq!(c.origin.x, a.origin.x, "third is in the first column");
    assert!(c.origin.y > a.origin.y, "third is on the second row");
    let before = style_name(&mut s);
    assert_eq!(before, "TableStyleMedium2");
    reveal(&mut s, &id("TableStyleMedium2"));
    s.screenshot("table-style-previews");
    // The selected one is framed: Medium2 is drawn on; click another and the style changes.
    reveal(&mut s, &id("TableStyleMedium7"));
    buttons::click(&mut s, &find("TableStyleMedium7"));
    s.settle();
    assert_eq!(style_name(&mut s), "TableStyleMedium7");
    s.screenshot("table-style-previews-selected");
}
