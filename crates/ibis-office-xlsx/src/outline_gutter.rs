// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! アウトラインの欄 (GitHub issue #291): the rows (columns) that are grouped (データ > グループ化) show a bracket line at the left (top) of the
//! headers, a − button at the row (column) after each group that hides it (a + button when it is hidden), and the level buttons 1, 2, … at the
//! corner that show the groups down to that level. The sizes are Excel's, measured (`ibis_office_shell::sheet_metrics::outline_button_size`).

use std::collections::BTreeMap;

use gpui_kit::*;
use ibis_doc_model::sheet::Sheet;
use ibis_doc_model::sheet_command::{Axis, outline_summary_line};
use ibis_doc_model::{Command, SheetCommand};

use crate::ext::*;
use crate::shell::editor::SlideEditor;

// Excel's: the lines (171, 171, 171) and the buttons' edges (130, 135, 144).
const LINE: u32 = 0xABABAB;
const BOX_BORDER: u32 = 0x828790;
const BOX_BACKGROUND: u32 = 0xE4E4E4;
const BUTTON_TEXT: u32 = 0x404040;

/// What a press in the outline's area does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutlineAction {
    /// Hide (`hide`) or show the rows (columns) `first..=last` of a group.
    Toggle { axis: Axis, first: u32, last: u32, hide: bool },
    /// The level button numbered `level` (1, 2, …): the groups above that level are shown, the rows of `level` and deeper are hidden.
    Level { axis: Axis, level: u8 },
}

/// A button as it was painted: its rectangle in window pixels (x, y, width, height).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutlineButton {
    pub(crate) rect: (f32, f32, f32, f32),
    pub(crate) action: OutlineAction,
}

/// The runs of consecutive rows (columns) whose level is `level` or deeper, as `(first, last)`.
pub(crate) fn runs(levels: &BTreeMap<u32, u8>, level: u8) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (&line, &l) in levels {
        if l < level {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.1 + 1 == line => last.1 = line,
            _ => out.push((line, line)),
        }
    }
    out
}

/// Whether line `i` is hidden (its size is 0).
fn hidden(sizes: &[i64], i: u32) -> bool {
    sizes.get(i as usize) == Some(&0)
}

/// The command a press does: the model works out what is hidden, shown and marked collapsed (`SheetCommand::CollapseGroup`, `ShowOutlineLevel`).
pub(crate) fn command_for(slide: ibis_doc_model::SlideId, action: OutlineAction) -> Command {
    Command::Sheet(match action {
        OutlineAction::Toggle { axis, first, last, hide } => SheetCommand::CollapseGroup { sheet: slide, axis, first, last, collapse: hide },
        OutlineAction::Level { axis, level } => SheetCommand::ShowOutlineLevel { sheet: slide, axis, level },
    })
}

/// The most levels either axis has.
fn depth(levels: &BTreeMap<u32, u8>) -> u8 {
    levels.values().copied().max().unwrap_or(0)
}

/// Paint the outline of both axes as Excel draws it (measured at 96 DPI, `tasks/doc/requests/2026-11-24-291-outline-measure-result.md`): `rows` and
/// `columns` are where the headers were painted (index, start, end in window pixels), `o` the top left of the headers, `(gw, gh)` the size of the
/// outline's strips and `(head_w, head_h)` the size of the headers with them, `b` the side of a button. Returns the buttons painted.
///
/// Rows: the level buttons side by side in the band of the column headers (above them when no column is grouped), one line 2 px wide for each group
/// in the column of its level (`b` apart) from 1 px below the group's top, with a 6 px hook there, down to 1 px above the next row, where the − (or +)
/// button of the group is. Columns: the same turned about: the level buttons one under the other above the row numbers, lines in the row of their
/// level, the buttons at the middle of the column after the group; a dot at each detail line. With the totals above (left), the button is in the row (column)
/// before the group and the line runs from below it to the group's far end, with the hook there (`Sheet::outline_summary_above`, `outline_summary_left`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint(
    window: &mut Window,
    cx: &mut App,
    sheet: &Sheet,
    o: Point<Pixels>,
    (gw, gh): (f32, f32),
    (head_w, head_h): (f32, f32),
    rows: &[(u32, f32, f32)],
    columns: &[(u32, f32, f32)],
    label: &dyn Fn(&mut Window, &mut App, String, Rgba, Pixels, Pixels, Pixels, Pixels),
) -> Vec<OutlineButton> {
    let mut buttons = Vec::new();
    let b = crate::shell::sheet_metrics::outline_button_size(sheet);
    let (line, border, background, text) = (rgb(LINE), rgb(BOX_BORDER), rgb(BOX_BACKGROUND), rgb(BUTTON_TEXT));
    let (dr, dc) = (depth(&sheet.outline_rows), depth(&sheet.outline_columns));
    let (ox, oy) = (f32::from(o.x), f32::from(o.y));
    let quad = |window: &mut Window, x: f32, y: f32, w: f32, h: f32, color: Rgba| window.paint_quad(fill(Bounds::new(point(px(x), px(y)), size(px(w), px(h))), color));
    // A button: a square with a border, and what is in it.
    let button = |window: &mut Window, cx: &mut App, x: f32, y: f32, glyph: &str| {
        quad(window, x, y, b, b, border);
        quad(window, x + 1., y + 1., b - 2., b - 2., background);
        match glyph {
            "-" | "+" => {
                let (bar, thick) = ((b / 2.).round(), (b / 12.).round().max(1.));
                let (cx0, cy0) = (x + (b / 2.).floor(), y + (b / 2.).floor());
                quad(window, cx0 - (bar / 2.).floor(), cy0, bar, thick, text);
                if glyph == "+" {
                    quad(window, cx0, cy0 - (bar / 2.).floor(), thick, bar, text);
                }
            }
            number => label(window, cx, number.to_string(), text, px(x), px(y), px(b), px(b)),
        }
    };
    // The level buttons: 1 to the deepest level + 1.
    for level in 1..=dr.saturating_add(1) {
        if dr == 0 {
            break;
        }
        let (x, y) = (ox + 2.0 + b * f32::from(level - 1), if dc > 0 { oy + gh + 1.0 } else { oy });
        button(window, cx, x, y, &level.to_string());
        buttons.push(OutlineButton { rect: (x, y, b, b), action: OutlineAction::Level { axis: Axis::Rows, level } });
    }
    for level in 1..=dc.saturating_add(1) {
        if dc == 0 {
            break;
        }
        let (x, y) = (ox + gw + crate::shell::sheet_metrics::outline_extra(sheet).0, oy + b * f32::from(level - 1));
        button(window, cx, x, y, &level.to_string());
        buttons.push(OutlineButton { rect: (x, y, b, b), action: OutlineAction::Level { axis: Axis::Columns, level } });
    }
    // Where a line of level `level` lies across its strip: the button's start and the line's, in the strip.
    let line_at = |level: u8| (2.0 + b * f32::from(level - 1), 2.0 + b * f32::from(level - 1) + (b / 2.).floor() - 3.0);
    let hook = 6.0;
    // The groups of the rows.
    let strip_top = oy + head_h;
    let strip_bottom = rows.iter().map(|r| r.2).fold(strip_top, f32::max);
    // What scrolls under the headers is cut off at the strips' edges, in the picture and for the presses (#291 W1).
    let clip = |rect: (f32, f32, f32, f32), (min_x, min_y): (f32, f32)| -> Option<(f32, f32, f32, f32)> {
        let (x, y) = (rect.0.max(min_x), rect.1.max(min_y));
        let (w, h) = (rect.0 + rect.2 - x, rect.1 + rect.3 - y);
        (w > 0.0 && h > 0.0).then_some((x, y, w, h))
    };
    let mask = |x: f32, y: f32| Some(ContentMask { bounds: Bounds::new(point(px(x), px(y)), size(px(100_000.), px(100_000.))) });
    // ... and it does not run over the level buttons (which are taller than our column headers).
    let level_bottom = oy + if dc > 0 { gh + 1.0 } else { 0.0 } + b;
    let row_clip = (ox, if dr > 0 { strip_top.max(level_bottom) } else { strip_top });
    window.with_content_mask(mask(row_clip.0, row_clip.1), |window| {
    for level in 1..=dr {
        let (button_x, line_x) = line_at(level);
        let (button_x, line_x) = (ox + button_x, ox + line_x);
        for (first, last) in runs(&sheet.outline_rows, level) {
            let span = |i: u32| rows.iter().find(|r| r.0 == i).map(|r| (r.1, r.2));
            let summary_line = outline_summary_line(first, last, sheet.outline_summary_above);
            let is_hidden = summary_line.is_some_and(|l| sheet.collapsed_rows.contains(&l)) || (sheet.rows.len() > last as usize && (first..=last).all(|i| hidden(&sheet.rows, i)));
            let (last_seen, first_seen) = (rows.last().map(|r| r.0), rows.first().map(|r| r.0));
            if last_seen.is_some_and(|r| first > r + u32::from(sheet.outline_summary_above)) || first_seen.is_some_and(|r| last + u32::from(!sheet.outline_summary_above) < r) {
                continue;
            }
            let summary = summary_line.and_then(span).filter(|s| s.1 > s.0);
            if !is_hidden {
                if sheet.outline_summary_above {
                    // Totals above (Excel's: the line runs from 2 px below the button to the last row's lower edge, the hook at that end).
                    let top = summary.map_or(strip_top, |s| s.0 + b + 2.0).max(strip_top);
                    let bottom = span(last).map_or(strip_bottom, |s| s.1 + 1.0);
                    if bottom > top {
                        quad(window, line_x, top, 2.0, bottom - top, line);
                        if let Some(s) = span(last) {
                            quad(window, line_x, s.1 - 1.0, hook, 2.0, line);
                        }
                    }
                } else {
                    let top = span(first).map_or(strip_top, |s| s.0 + 1.0).max(strip_top);
                    let bottom = summary.map_or(strip_bottom, |s| s.0 - 1.0);
                    if bottom > top {
                        quad(window, line_x, top, 2.0, bottom - top, line);
                        if span(first).is_some() {
                            quad(window, line_x, top, hook, 2.0, line);
                        }
                    }
                }
            }
            if let Some((s_top, _)) = summary {
                button(window, cx, button_x, s_top, if is_hidden { "+" } else { "-" });
                if let Some(rect) = clip((button_x, s_top, b, b), row_clip) {
                    buttons.push(OutlineButton { rect, action: OutlineAction::Toggle { axis: Axis::Rows, first, last, hide: !is_hidden } });
                }
            }
        }
    }
    });
    // The groups of the columns, likewise.
    let strip_left = ox + head_w;
    let strip_right = columns.iter().map(|c| c.2).fold(strip_left, f32::max);
    let column_clip = (if dc > 0 { strip_left.max(ox + gw + crate::shell::sheet_metrics::outline_extra(sheet).0 + b) } else { strip_left }, oy);
    window.with_content_mask(mask(column_clip.0, column_clip.1), |window| {
    for level in 1..=dc {
        let (button_y, line_y) = line_at(level);
        let (button_y, line_y) = (oy + button_y, oy + line_y);
        for (first, last) in runs(&sheet.outline_columns, level) {
            let span = |i: u32| columns.iter().find(|c| c.0 == i).map(|c| (c.1, c.2));
            let summary_line = outline_summary_line(first, last, sheet.outline_summary_left);
            let is_hidden = summary_line.is_some_and(|l| sheet.collapsed_columns.contains(&l)) || (sheet.columns.len() > last as usize && (first..=last).all(|i| hidden(&sheet.columns, i)));
            let (last_seen, first_seen) = (columns.last().map(|c| c.0), columns.first().map(|c| c.0));
            if last_seen.is_some_and(|c| first > c + u32::from(sheet.outline_summary_left)) || first_seen.is_some_and(|c| last + u32::from(!sheet.outline_summary_left) < c) {
                continue;
            }
            let summary = summary_line.and_then(span).filter(|s| s.1 > s.0);
            if !is_hidden {
                if sheet.outline_summary_left {
                    let left = summary.map_or(strip_left, |s| (s.0 + s.1) / 2. - (b / 2.).floor() + b + 2.0).max(strip_left);
                    let right = span(last).map_or(strip_right, |s| s.1 + 1.0);
                    if right > left {
                        quad(window, left, line_y, right - left, 2.0, line);
                        if let Some(s) = span(last) {
                            quad(window, s.1 - 1.0, line_y, 2.0, hook, line);
                        }
                    }
                } else {
                    let left = span(first).map_or(strip_left, |s| s.0 + 1.0).max(strip_left);
                    let right = summary.map_or(strip_right, |s| ((s.0 + s.1) / 2. - (b / 2.).floor()) - 1.0);
                    if right > left {
                        quad(window, left, line_y, right - left, 2.0, line);
                        if span(first).is_some() {
                            quad(window, left, line_y, 2.0, hook, line);
                        }
                    }
                }
            }
            if let Some((s_left, s_right)) = summary {
                let x = (s_left + s_right) / 2. - (b / 2.).floor();
                button(window, cx, x, button_y, if is_hidden { "+" } else { "-" });
                if let Some(rect) = clip((x, button_y, b, b), column_clip) {
                    buttons.push(OutlineButton { rect, action: OutlineAction::Toggle { axis: Axis::Columns, first, last, hide: !is_hidden } });
                }
            }
        }
    }
    });
    // The line along the edge of the frozen panes goes on through the outline's strips (Excel's does; measured on Windows, #291 W7).
    if let Some((frozen_columns, frozen_rows)) = sheet.frozen {
        let frozen = rgb(crate::shell::sheet_metrics::FROZEN_LINE);
        if let Some(&(_, _, bottom)) = rows.iter().find(|r| frozen_rows > 0 && r.0 == frozen_rows - 1).filter(|_| gw > 0.0) {
            quad(window, ox, bottom - 1.0, gw, 1.0, frozen);
        }
        if let Some(&(_, _, right)) = columns.iter().find(|c| frozen_columns > 0 && c.0 == frozen_columns - 1).filter(|_| gh > 0.0) {
            quad(window, right - 1.0, oy, 1.0, gh, frozen);
        }
    }
    // A dot (2 x 2 px) at each detail row (column), in the column (row) of the level after its own.
    for &(row, top, bottom) in rows.iter().filter(|r| r.2 > r.1) {
        if let Some(&level) = sheet.outline_rows.get(&row) {
            let (_, line_x) = line_at(level + 1);
            quad(window, ox + line_x, (top + bottom) / 2. - 1.0, 2.0, 2.0, line);
        }
    }
    for &(column, left, right) in columns.iter().filter(|c| c.2 > c.1) {
        if let Some(&level) = sheet.outline_columns.get(&column) {
            let (_, line_y) = line_at(level + 1);
            quad(window, (left + right) / 2. - 1.0, oy + line_y, 2.0, 2.0, line);
        }
    }
    buttons
}

#[ibis_office_macros::editor_ext(OutlineGutter)]
impl SlideEditor {
    /// A press in the outline's area: the button under the pointer does what it says (one undo step).
    pub(crate) fn on_outline_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let buttons = self.sheet_state().map(|s| s.header_geometry.borrow().outline_buttons.clone()).unwrap_or_default();
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        let Some(button) = buttons.iter().find(|b| x >= b.rect.0 && x < b.rect.0 + b.rect.2 && y >= b.rect.1 && y < b.rect.1 + b.rect.3) else { return };
        let axis = match button.action {
            OutlineAction::Toggle { axis, .. } | OutlineAction::Level { axis, .. } => axis,
        };
        if self.outline_refused_by_protection(axis, false) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        self.run_sheet(vec![command_for(slide, button.action)], cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_doc_model::SlideId;

    fn sheet_with(rows: &[(u32, u8)], hidden_rows: &[u32]) -> Sheet {
        let mut sheet = Sheet { rows: vec![240_000; 12], ..Sheet::default() };
        for &(r, l) in rows {
            sheet.outline_rows.insert(r, l);
        }
        for &r in hidden_rows {
            sheet.rows[r as usize] = 0;
        }
        sheet
    }

    #[core::prelude::v1::test]
    fn a_run_is_the_consecutive_rows_of_that_level_or_deeper() {
        let levels: BTreeMap<u32, u8> = [(1, 1), (2, 2), (3, 2), (4, 1), (7, 1)].into();
        assert_eq!(runs(&levels, 1), vec![(1, 4), (7, 7)]);
        assert_eq!(runs(&levels, 2), vec![(2, 3)]);
        assert!(runs(&levels, 3).is_empty());
    }

    #[core::prelude::v1::test]
    fn a_press_is_one_model_command() {
        let toggle = command_for(SlideId(1), OutlineAction::Toggle { axis: Axis::Rows, first: 3, last: 6, hide: true });
        assert!(matches!(toggle, Command::Sheet(SheetCommand::CollapseGroup { first: 3, last: 6, collapse: true, .. })));
        assert!(matches!(command_for(SlideId(1), OutlineAction::Level { axis: Axis::Columns, level: 2 }), Command::Sheet(SheetCommand::ShowOutlineLevel { level: 2, .. })));
    }

    /// Excel's sizes (96 DPI, `2026-11-24-291-outline-measure-result.md`): the button by zoom, the strips of two levels of rows and columns.
    #[core::prelude::v1::test]
    fn the_strips_and_buttons_have_excels_sizes() {
        use crate::shell::sheet_metrics::{outline_button_size, outline_height, outline_width};
        let zoomed = |zoom: Option<u16>| Sheet { zoom, ..sheet_with(&[(1, 1), (2, 2)], &[]) };
        // The 100 % size times the zoom, rounded, at most 42 px; Excel's own sizes (`2026-11-26-291-outline-zoom-result.md`) are within 2 px of these.
        for (zoom, size) in [(25, 6.0), (50, 12.0), (75, 17.0), (100, 23.0), (125, 29.0), (150, 35.0), (175, 40.0), (200, 42.0), (300, 42.0), (400, 42.0)] {
            assert_eq!(outline_button_size(&zoomed(Some(zoom))), size, "{zoom} %");
        }
        assert_eq!(outline_button_size(&zoomed(None)), 23.0);
        // Rows of two levels: 3 buttons and 5 px wide; one band of buttons above the column headers.
        let rows = zoomed(None);
        // Our column headers are 20 px high, so the band of Excel's 25 px (23 + 2) lacks 5 px.
        assert_eq!((outline_width(&rows), outline_height(&rows)), (74.0, 5.0));
        // Columns only: no strip at the left (Excel's level buttons are above the row numbers); 3 buttons + 2 px above.
        let mut columns = zoomed(None);
        columns.outline_rows.clear();
        columns.outline_columns.extend([(2, 1), (3, 2)]);
        assert_eq!((outline_width(&columns), outline_height(&columns)), (0.0, 72.0));
        // At 200 % Excel's strip above the headers is 129 px (42 * 3 + 3), as at every zoom (3 buttons + 3 px); the buttons are 10 px right (Excel 9).
        columns.zoom = Some(200);
        assert_eq!((outline_width(&columns), outline_height(&columns), crate::shell::sheet_metrics::outline_extra(&columns).0), (0.0, 129.0, 10.0));
        // Both axes of two levels: 74 wide, 72 high (Excel's, 3 buttons + 3 px).
        let mut both = zoomed(None);
        both.outline_columns.extend([(2, 1), (3, 2)]);
        assert_eq!((outline_width(&both), outline_height(&both)), (74.0, 72.0));
        assert_eq!(outline_width(&Sheet::default()), 0.0);
        // X14 of the #291 design (`tasks/doc/test-design/issue-291.md`): the width of the row bar of two levels at each measured zoom (Excel, `2026-11-24-291-outline-measure-result.md`
        // 2): the button times the three level buttons plus 5 px.
        for (zoom, width) in [(50, 41.0), (75, 56.0), (100, 74.0), (125, 92.0), (150, 110.0), (175, 125.0), (200, 131.0), (300, 131.0), (400, 131.0)] {
            assert_eq!(outline_width(&zoomed(Some(zoom))), width, "{zoom} %");
        }
        // The strip above the columns' headers is always 3 buttons + 3 px (Excel's, at every zoom measured from 25 to 400 %).
        for zoom in [50, 100, 150, 200, 400] {
            let mut both = zoomed(Some(zoom));
            both.outline_columns.extend([(2, 1), (3, 2)]);
            assert_eq!(outline_height(&both), outline_button_size(&both) * 3.0 + 3.0, "{zoom} %");
        }
    }
}
