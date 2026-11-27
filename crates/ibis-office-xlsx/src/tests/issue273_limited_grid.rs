// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #273: 限られたグリッド表示. Off (the default) the sheet scrolls over the whole grid as before; on, it is the used
//! range and a margin, it grows when the selection nears its edge, and the setting is kept for the next start.
use crate::ext::*;
use crate::shell::scenario::Scenario;
use crate::shell::settings_dialog::{LIMITED_GRID_KEY, limited_grid, note_limited_grid_changed};

fn book() -> Scenario {
    let mut s = super::new_sheet();
    s.size(900., 700.);
    s
}

fn content(s: &mut Scenario) -> (f32, f32) {
    s.with(|e, _| {
        let c = e.workspace_content().expect("laid out");
        (f32::from(c.size.width), f32::from(c.size.height))
    })
}

/// A saved setting, as the next start reads it.
fn saved(on: Option<bool>) {
    match on {
        Some(on) => crate::shell::settings::set(LIMITED_GRID_KEY, if on { "1" } else { "0" }),
        None => crate::shell::settings::remove(LIMITED_GRID_KEY),
    }
    note_limited_grid_changed();
}

pub(crate) fn off_by_default_keeps_the_whole_grid() {
    saved(None);
    assert!(!limited_grid());
    let mut s = book();
    let (w, h) = content(&mut s);
    // 1,000 rows and 50 columns past the cells, as before.
    assert!(h > 900. * 15.0 && w > 40. * 40.0, "the whole grid: {w} x {h}");
}

pub(crate) fn on_limits_the_grid_to_the_used_range_and_a_margin() {
    saved(Some(false));
    let mut s = book();
    let (w_off, h_off) = content(&mut s);
    let (columns, rows) = s.with(|e, _| {
        let (_, sheet) = e.current_sheet().unwrap();
        crate::limited_grid::extent(sheet, e.selected_range())
    });
    assert!(rows >= crate::limited_grid::MIN_ROWS && rows < 200, "rows {rows}");
    assert!(columns >= crate::limited_grid::MIN_COLUMNS && columns < 100, "columns {columns}");
    s.update(|_, _, cx| crate::shell::settings_dialog::set_limited_grid(true, cx));
    let (w_on, h_on) = content(&mut s);
    assert!(h_on < h_off / 5. && w_on < w_off / 3., "limited: {w_on} x {h_on}, whole: {w_off} x {h_off}");
    s.update(|_, _, cx| crate::shell::settings_dialog::set_limited_grid(false, cx));
    assert_eq!(content(&mut s), (w_off, h_off), "off again is the whole grid again");
    saved(None);
}

pub(crate) fn the_grid_grows_when_the_selection_goes_to_its_edge() {
    saved(Some(true));
    let mut s = book();
    let (_, h) = content(&mut s);
    s.update(|e, _, cx| e.jump_to_cell(300, 2, cx));
    let (_, grown) = content(&mut s);
    assert!(grown > h * 3., "{h} -> {grown}");
    s.update(|e, _, cx| e.jump_to_cell(0, 0, cx));
    let (_, back) = content(&mut s);
    assert_eq!(back, h, "the grid shrinks back to the used range and its margin when the selection returns");
    saved(None);
}

pub(crate) fn the_setting_is_kept_for_the_next_start() {
    saved(None);
    assert!(!limited_grid());
    let mut s = book();
    s.update(|_, _, cx| crate::shell::settings_dialog::set_limited_grid(true, cx));
    assert_eq!(crate::shell::settings::get(LIMITED_GRID_KEY).as_deref(), Some("1"));
    // The next start has nothing cached: it reads the file.
    note_limited_grid_changed();
    assert!(limited_grid());
    saved(None);
}
