// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! オートフィル (the fill handle at the selection's corner, dragged), as
//! Excel fills: numbers go on as a series from two or more (a single number
//! is copied), dates by a day, text ending in a number counts on, the
//! lists Excel knows (曜日, 月, 四半期, Jan…, Mon…) continue, formulas are
//! copied with their relative references moved, and everything else is
//! copied in turn. Formats come along. ⌥ held on release copies instead
//! of making a series (Excel's Ctrl).

use ibis_doc_model::{CellInput, CellRange, CellStyle, CellValue, Command, PutCell, SheetCommand};

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The lists Excel continues (its built-in custom lists, Japanese and English).
const LISTS: &[&[&str]] = &[
    &["日", "月", "火", "水", "木", "金", "土"], // i18n-ok（連続データの照合に使う語（系列の名前）。画面には出さない。一字の曜日）
    &["日曜日", "月曜日", "火曜日", "水曜日", "木曜日", "金曜日", "土曜日"], // i18n-ok（連続データの照合に使う語（系列の名前）。画面には出さない。）
    &["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"], // i18n-ok（連続データの照合に使う語（系列の名前）。画面には出さない。月名）
    &["第1四半期", "第2四半期", "第3四半期", "第4四半期"], // i18n-ok（連続データの照合に使う語（系列の名前）。画面には出さない。）
    &["第１四半期", "第２四半期", "第３四半期", "第４四半期"], // i18n-ok（連続データの照合に使う語（系列の名前）。画面には出さない。全角数字の四半期）
    &["睦月", "如月", "弥生", "卯月", "皐月", "水無月", "文月", "葉月", "長月", "神無月", "霜月", "師走"], // i18n-ok（連続データの照合に使う語（系列の名前）。画面には出さない。和風の月名）
    &["子", "丑", "寅", "卯", "辰", "巳", "午", "未", "申", "酉", "戌", "亥"], // i18n-ok（連続データの照合に使う語（系列の名前）。画面には出さない。十二支）
    &["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"], // i18n-ok（連続データの照合に使う語（系列の名前）。画面には出さない。十干）
    &["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    &["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"],
    &["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    &["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
];

/// How a fill fills (Excel's オートフィル オプション).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FillMode {
    /// A series where the values make one (the default).
    Series,
    /// The cells copied in turn.
    Copy,
    /// Dates by days, weekdays, months or years (a series of numbers otherwise).
    Days,
    Weekdays,
    Months,
    Years,
    /// The formats only (the values stay).
    FormatsOnly,
    /// The values (as a series) without the formats.
    WithoutFormats,
}

/// A source cell of the fill.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Source {
    pub(crate) value: CellValue,
    pub(crate) formula: Option<String>,
    /// Shown as a date (its number format has a date).
    pub(crate) date: bool,
}

/// What one filled cell gets: the value or formula (with how many cells
/// along the fill its source is, to move a formula's references by), and
/// which source cell's format.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Filled {
    Value(CellValue),
    Formula { source: String, offset: i64 },
    Clear,
}

/// The values of `count` cells after `sources` along the fill (before
/// them, `backward`: index 0 is the one next to the sources). `copy`: copy
/// in turn instead of making a series. Returns each with its source index
/// (for the format).
#[allow(dead_code)]
pub(crate) fn fill(sources: &[Source], count: usize, backward: bool, copy: bool) -> Vec<(Filled, usize)> {
    fill_as(sources, count, backward, if copy { FillMode::Copy } else { FillMode::Series })
}

/// `fill` as `mode` says.
pub(crate) fn fill_as(sources: &[Source], count: usize, backward: bool, mode: FillMode) -> Vec<(Filled, usize)> {
    let copy = mode == FillMode::Copy;
    let n = sources.len();
    if n == 0 {
        return Vec::new();
    }
    // Position of each target along the sources' axis: n, n+1, … (or -1, -2, …).
    let position = |j: usize| -> i64 { if backward { -(j as i64) - 1 } else { (n + j) as i64 } };
    let source_of = |p: i64| -> usize { p.rem_euclid(n as i64) as usize };

    let numbers: Option<Vec<f64>> = sources.iter().map(|s| match (&s.value, &s.formula) {
        (CellValue::Number(v), None) => Some(*v),
        _ => None,
    }).collect();
    let mut out = Vec::with_capacity(count);
    for j in 0..count {
        let p = position(j);
        let s = source_of(p);
        let filled = if copy {
            copied(&sources[s], p - s as i64)
        } else if let (Some(values), true) = (&numbers, matches!(mode, FillMode::Weekdays | FillMode::Months | FillMode::Years)) {
            // Dates by weekdays, months or years from the last source (the step from the first two).
            let step = if n >= 2 { values[1] - values[0] } else { 1.0 };
            let step = if mode == FillMode::Weekdays { step.round().max(1.0) as i64 } else { (step / if mode == FillMode::Months { 30.0 } else { 365.0 }).round().max(1.0) as i64 };
            let base = if backward { values[0] } else { values[n - 1] };
            let k = if backward { -(j as i64) - 1 } else { j as i64 + 1 };
            Filled::Value(CellValue::Number(date_step(base, mode, step * k)))
        } else if let Some(values) = &numbers
            && (n >= 2 || sources[0].date || mode == FillMode::Days)
        {
            // A least-squares line through the numbers (Excel's trend); a single date steps a day.
            let v = if n == 1 { values[0] + p as f64 } else { trend(values, p as f64) };
            Filled::Value(CellValue::Number(round15(v)))
        } else if let Some(v) = listed(sources, p) {
            Filled::Value(CellValue::Text(v))
        } else if let Some(v) = counted(sources, p) {
            Filled::Value(CellValue::Text(v))
        } else {
            copied(&sources[s], p - s as i64)
        };
        out.push((filled, s));
    }
    out
}

/// `base` (a date serial) moved by `k` weekdays, months or years (a month's
/// day kept, or its last day if it has fewer).
fn date_step(base: f64, mode: FillMode, k: i64) -> f64 {
    use ibis_calc::eval::convert::{date_from_serial, days_in_month, serial_from_date, weekday};
    let serial = base.floor() as i64;
    let time = base - base.floor();
    let moved = match mode {
        FillMode::Weekdays => {
            let mut s = serial;
            let mut left = k.abs();
            let dir = k.signum();
            while left > 0 {
                s += dir;
                // weekday: 0 Sunday … 6 Saturday.
                let w = weekday(s);
                if w != 0 && w != 6 {
                    left -= 1;
                }
            }
            s
        }
        FillMode::Months | FillMode::Years => {
            let (y, m, d) = date_from_serial(serial, false);
            let months = if mode == FillMode::Years { 12 * k } else { k };
            let total = y * 12 + (m as i64 - 1) + months;
            let (ny, nm) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
            serial_from_date(ny, nm as i64, d.min(days_in_month(ny, nm)) as i64, false)
        }
        _ => serial + k,
    };
    moved as f64 + time
}

fn copied(source: &Source, offset: i64) -> Filled {
    match (&source.formula, &source.value) {
        (Some(f), _) => Filled::Formula { source: f.clone(), offset },
        (None, CellValue::Empty) => Filled::Clear,
        (None, v) => Filled::Value(v.clone()),
    }
}

/// The least-squares line through (0, v0), (1, v1), … at `x`.
fn trend(values: &[f64], x: f64) -> f64 {
    let n = values.len() as f64;
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = values.iter().sum::<f64>() / n;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, v) in values.iter().enumerate() {
        let dx = i as f64 - mean_x;
        sxy += dx * (v - mean_y);
        sxx += dx * dx;
    }
    let slope = if sxx == 0.0 { 0.0 } else { sxy / sxx };
    mean_y + slope * (x - mean_x)
}

/// To 15 significant digits (so 0.1 steps stay 0.3, not 0.30000000000000004).
fn round15(v: f64) -> f64 {
    if v == 0.0 || !v.is_finite() {
        return v;
    }
    format!("{v:.14e}").parse().unwrap_or(v)
}

/// Continue a list Excel knows, if every source is in the same one: by the
/// step between the first two (1 for one), wrapping around.
fn listed(sources: &[Source], p: i64) -> Option<String> {
    let texts: Vec<&str> = sources.iter().map(|s| match (&s.value, &s.formula) {
        (CellValue::Text(t), None) => Some(t.as_str()),
        _ => None,
    }).collect::<Option<Vec<_>>>()?;
    for list in LISTS {
        let at: Option<Vec<usize>> = texts.iter().map(|t| list.iter().position(|x| x.eq_ignore_ascii_case(t))).collect();
        let Some(at) = at else { continue };
        let len = list.len() as i64;
        let step = if at.len() >= 2 { (at[1] as i64 - at[0] as i64).rem_euclid(len) } else { 1 };
        let step = if step == 0 { 1 } else { step };
        let i = (at[0] as i64 + step * p).rem_euclid(len);
        return Some(list[i as usize].to_string());
    }
    None
}

/// Text ending in a number (`第1回`, `Item 1`, `A-01`): the number counts
/// on (by the step between the first two with the same text around it).
fn counted(sources: &[Source], p: i64) -> Option<String> {
    let parts: Vec<(String, i64, usize, String)> = sources.iter().map(|s| match (&s.value, &s.formula) {
        (CellValue::Text(t), None) => split_number(t),
        _ => None,
    }).collect::<Option<Vec<_>>>()?;
    let (head, first, width, tail) = &parts[0];
    if parts.iter().any(|(h, _, _, t)| h != head || t != tail) {
        return None;
    }
    let step = if parts.len() >= 2 { parts[1].1 - parts[0].1 } else { 1 };
    let v = first + step * p;
    let digits = if v < 0 { format!("{}", -v) } else { format!("{v:0width$}", width = *width) };
    Some(format!("{head}{digits}{tail}"))
}

/// The last run of ASCII digits in `t`: the text before, its value, its
/// width (for leading zeros) and the text after.
fn split_number(t: &str) -> Option<(String, i64, usize, String)> {
    let end = t.rfind(|c: char| c.is_ascii_digit())? + 1;
    let start = t[..end].char_indices().rev().find(|(_, c)| !c.is_ascii_digit()).map_or(0, |(i, c)| i + c.len_utf8());
    let value = t[start..end].parse().ok()?;
    let width = if t[start..end].starts_with('0') { end - start } else { 0 };
    Some((t[..start].to_string(), value, width, t[end..].to_string()))
}

/// The selection's border dragged to move (or, with Option, copy) it (#11).
#[derive(Clone, Copy, Debug)]
pub struct CellMove {
    pub(crate) source: CellRange,
    /// The cell under the pointer when pressed.
    pub(crate) grab: (u32, u32),
    /// Where the range would land.
    pub(crate) target: CellRange,
    pub(crate) copy: bool,
    pub(crate) moved: bool,
}

/// Where dragging the fill handle has reached, as a cell on one side of
/// `source`: the pointer is at (`x`, `y`) in the sheet (EMU), over the cell
/// `(column, row)`, `xs` and `ys` the column and row offsets. It goes right
/// once the pointer is at half of the next column's width from the
/// source's bottom-right corner, down at half of the next row's height;
/// where both hold, the line at 45 degrees from the corner decides (below
/// it down, above it right). Above or left of the source it goes up or left
/// by the cell. Short of all of these it stays in the source.
pub(crate) fn fill_target(source: CellRange, (column, row): (u32, u32), (x, y): (i64, i64), xs: &[i64], ys: &[i64], (default_column, default_row): (i64, i64)) -> (u32, u32) {
    // Past the allocated grid the lines are the sheet's default size (the view grows the grid as the drag goes on).
    let at = |offsets: &[i64], default: i64, i: u32| {
        let i = i as usize;
        if i < offsets.len() { offsets[i] } else { offsets.last().copied().unwrap_or(0) + (i - (offsets.len() - 1)) as i64 * default.max(1) }
    };
    let (corner_x, corner_y) = (at(xs, default_column, source.last_column + 1), at(ys, default_row, source.last_row + 1));
    let (next_w, next_h) = (at(xs, default_column, source.last_column + 2) - corner_x, at(ys, default_row, source.last_row + 2) - corner_y);
    let (dx, dy) = (x - corner_x, y - corner_y);
    if row < source.first_row {
        return (source.last_column, row);
    }
    if column < source.first_column {
        return (column, source.last_row);
    }
    // The middle of the next cell counts (a pixel's rounding is not held against it).
    let (right, down) = (dx >= next_w / 2 - next_w / 50, dy >= next_h / 2 - next_h / 50);
    match (right, down) {
        (true, true) if dy > dx => (source.last_column, row.max(source.last_row + 1)),
        (true, true) | (true, false) => (column.max(source.last_column + 1), source.last_row),
        (false, true) => (source.last_column, row.max(source.last_row + 1)),
        (false, false) => (source.last_column, source.last_row),
    }
}

/// Direction of the edge fill (編集 > フィル > 下方向・右方向・上方向・左方向へフィル).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FillDirection {
    Down,
    Right,
    Up,
    Left,
}

#[ibis_office_macros::editor_ext(XlsxFill)]
impl SlideEditor {
    /// オートフィル オプション: the last fill done again as `mode` (in place of it).
    pub(crate) fn refill(&mut self, mode: FillMode, cx: &mut Context<Self>) {
        let Some((source, column, row, _)) = self.sheet_state().and_then(|s| s.last_fill) else { return };
        self.document.undo();
        self.fill_to_as(source, column, row, mode, cx);
    }

    /// The filled range of the last fill, if it is still what is selected
    /// (where オートフィル オプション shows).
    pub(crate) fn fill_options_range(&self) -> Option<CellRange> {
        let (source, column, row, _) = self.sheet_state()?.last_fill?;
        let filled = if row > source.last_row || row < source.first_row {
            CellRange { first_row: source.first_row.min(row), last_row: source.last_row.max(row), ..source }
        } else {
            CellRange { first_column: source.first_column.min(column), last_column: source.last_column.max(column), ..source }
        };
        (self.selected_range() == Some(filled)).then_some(filled)
    }

    /// Open オートフィル オプション at `position`.
    pub(crate) fn open_fill_options(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        use crate::shell::chrome::{MenuEntry, MenuItem, fill_menu};
        let current = self.sheet_state().and_then(|s| s.last_fill).map(|f| f.3);
        let item = |id: &'static str, label: gpui::SharedString, mode: FillMode| -> MenuEntry {
            MenuItem::new(id, label)
                .checked(current == Some(mode))
                .on(move |this, _, cx| {
                    if let Some(s) = this.sheet_state_mut() {
                        s.list_popup = None;
                    }
                    this.refill(mode, cx);
                })
                .into()
        };
        let entries = std::rc::Rc::new(vec![
            item("xlsx.fill-copy", crate::shell::i18n::text("_xlsx.copy-cells"), FillMode::Copy),
            item("xlsx.fill-series", crate::shell::i18n::text("_xlsx.fill-series"), FillMode::Series),
            item("xlsx.fill-formats", crate::shell::i18n::text("_xlsx.fill-formatting-only"), FillMode::FormatsOnly),
            item("xlsx.fill-values", crate::shell::i18n::text("_xlsx.fill-without-formatting"), FillMode::WithoutFormats),
            MenuEntry::Separator,
            item("xlsx.fill-days", crate::shell::i18n::text("_xlsx.fill-days"), FillMode::Days),
            item("xlsx.fill-weekdays", crate::shell::i18n::text("_xlsx.fill-weekdays"), FillMode::Weekdays),
            item("xlsx.fill-months", crate::shell::i18n::text("_xlsx.fill-months"), FillMode::Months),
            item("xlsx.fill-years", crate::shell::i18n::text("_xlsx.fill-years"), FillMode::Years),
        ]);
        let editor = cx.entity().downgrade();
        let menu = gpui_kit::component::menu::PopupMenu::build(window, cx, move |menu, window, cx| fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(s) = this.sheet_state_mut() {
                s.list_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.wb_mut().list_popup = Some(super::validation::ListPopup { menu, position, anchor: Anchor::TopLeft, _dismiss: dismiss });
        cx.notify();
    }

    /// Press on the selection's border (#11): remember the range and the
    /// cell grabbed, to move it (or copy it with Option) when let go.
    pub fn begin_cell_move(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        self.commit_cell_edit(cx);
        self.focus_handle.focus(window, cx);
        let (Some(source), Some(grab)) = (self.selected_range(), self.cell_under(position)) else { return };
        if self.selected_ranges().len() > 1 {
            return;
        }
        self.wb_mut().cell_move = Some(CellMove { source, grab, target: source, copy: window.modifiers().alt, moved: false });
        cx.notify();
    }

    /// The pointer moved while the border is dragged: where the range would land.
    pub(crate) fn update_cell_move(&mut self, position: Point<Pixels>, alt: bool, cx: &mut Context<Self>) {
        let Some(cell) = self.cell_under(position) else { return };
        let Some(m) = self.sheet_state_mut().and_then(|s| s.cell_move.as_mut()) else { return };
        let (dc, dr) = (cell.0 as i64 - m.grab.0 as i64, cell.1 as i64 - m.grab.1 as i64);
        let shift = |a: u32, d: i64| (a as i64 + d).max(0) as u32;
        // Kept inside the grid: the range is not squeezed, the move is held.
        let (w, h) = (m.source.last_column - m.source.first_column, m.source.last_row - m.source.first_row);
        let first_column = shift(m.source.first_column, dc).min(16_383 - w);
        let first_row = shift(m.source.first_row, dr).min(1_048_575 - h);
        let target = CellRange { first_column, first_row, last_column: first_column + w, last_row: first_row + h };
        m.moved |= target != m.source;
        m.target = target;
        m.copy = alt;
        cx.notify();
    }

    /// The border let go: the range moves (one step; formulas that refer to
    /// it follow), or with Option a copy lands there. Without a move, nothing.
    /// Over cells that hold something, Excel asks first (`move_overwrite`).
    pub fn finish_cell_move(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(m) = self.sheet_state_mut().and_then(|s| s.cell_move.take()) else { return false };
        cx.notify();
        if !m.moved || m.target == m.source || self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return true;
        }
        let holds = self.current_sheet().is_some_and(|(_, sheet)| {
            (m.target.first_row..=m.target.last_row).any(|r| {
                (m.target.first_column..=m.target.last_column).any(|c| {
                    let in_source = (m.source.first_row..=m.source.last_row).contains(&r) && (m.source.first_column..=m.source.last_column).contains(&c);
                    !in_source && sheet.cell(r, c).is_some_and(|cell| cell.formula.is_some() || !cell.value.is_empty())
                })
            })
        });
        if holds {
            self.wb_mut().move_overwrite = Some(m);
            return true;
        }
        self.apply_cell_move(m, cx);
        true
    }

    /// The question's OK (true) or キャンセル (false), when the range was let go over cells with data.
    pub(crate) fn answer_move_overwrite(&mut self, ok: bool, cx: &mut Context<Self>) {
        let Some(m) = self.sheet_state_mut().and_then(|s| s.move_overwrite.take()) else { return };
        if ok {
            self.apply_cell_move(m, cx);
        }
        cx.notify();
    }

    fn apply_cell_move(&mut self, m: CellMove, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        if !m.copy {
            self.run_sheet(vec![Command::Sheet(SheetCommand::MoveCells { slide, from: m.source, row: m.target.first_row, column: m.target.first_column })], cx);
        } else {
            // A copy goes through the clipboard code, the system clipboard
            // and the internal copy put back as they were.
            let previous = cx.read_from_clipboard();
            let previous_clip = self.sheet_state().and_then(|s| s.clip.clone());
            self.set_selected_ranges(&[m.source]);
            self.copy_cells(false, cx);
            self.set_selected_ranges(&[m.target]);
            self.paste_cells(false, cx);
            if let Some(item) = previous {
                cx.write_to_clipboard(item);
            }
            if let Some(state) = self.sheet_state_mut() {
                state.clip = previous_clip;
            }
        }
        self.set_selected_ranges(&[m.target]);
        cx.notify();
    }

    /// Press on the fill handle: remember what is being filled from.
    pub fn begin_fill(&mut self) {
        let source = self.selected_range();
        if let Some(state) = self.sheet_state_mut() {
            state.fill_source = source;
        }
    }

    /// The fill handle let go: fill the range dragged out from the source.
    pub fn finish_fill(&mut self, copy: bool, cx: &mut Context<Self>) {
        let Some(source) = self.sheet_state_mut().and_then(|s| s.fill_source.take()) else { return };
        let Some((column, row)) = self.sheet_state().and_then(|s| s.cell_ends.get(&self.current_slide).copied()).or_else(|| self.current_cell()) else { return };
        self.fill_to(source, column, row, copy, cx);
    }

    /// Excel's double-click on the fill handle: fills down to the end of the
    /// adjacent data range (left column preferred, then right column, or
    /// table end if inside a table).
    pub(crate) fn double_click_fill_handle(&mut self, cx: &mut Context<Self>) -> bool {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return false;
        }
        let Some(source) = self.selected_range() else { return false };
        let Some((_, sheet)) = self.current_sheet() else { return false };

        let target_row = Self::find_fill_handle_target_row(sheet, source);
        let Some(target_row) = target_row else { return false };
        if target_row <= source.last_row {
            return false;
        }

        self.fill_to(source, source.last_column, target_row, false, cx);
        true
    }

    /// Determine target row for double-clicking the fill handle based on
    /// adjacent data columns or containing table.
    pub(crate) fn find_fill_handle_target_row(sheet: &ibis_doc_model::Sheet, source: CellRange) -> Option<u32> {
        let is_empty = |r: u32, c: u32| -> bool {
            match sheet.cell(r, c) {
                None => true,
                Some(cell) => cell.formula.is_none() && matches!(cell.value, CellValue::Empty),
            }
        };

        // 1. If inside an Excel table, target is the last data row of the table.
        for table in &sheet.tables {
            let data_first_row = table.range.first_row + table.header_rows;
            let data_last_row = if table.range.last_row >= table.totals_rows {
                table.range.last_row - table.totals_rows
            } else {
                table.range.last_row
            };
            if source.first_row >= data_first_row
                && source.last_row <= data_last_row
                && source.first_column >= table.range.first_column
                && source.last_column <= table.range.last_column
            {
                if data_last_row > source.last_row {
                    return Some(data_last_row);
                }
            }
        }

        // Helper to find contiguous non-empty cell run in column `col` starting alongside `source`
        let scan_column = |col: u32| -> Option<u32> {
            // Check if there is data in column `col` alongside the source selection
            let has_data_at_source = (source.first_row..=source.last_row).any(|r| !is_empty(r, col));
            if !has_data_at_source {
                return None;
            }
            // Check if the row directly below source has data in column `col`
            let next_row = source.last_row + 1;
            if next_row >= super::sheet_ui::MAX_ROWS || is_empty(next_row, col) {
                return None;
            }
            // Scan downwards while cells in `col` are not empty
            let mut r = next_row;
            while r < super::sheet_ui::MAX_ROWS && !is_empty(r, col) {
                r += 1;
            }
            let end_row = r - 1;
            if end_row > source.last_row {
                Some(end_row)
            } else {
                None
            }
        };

        // 2. Check left adjacent column first (preferred)
        if source.first_column > 0 {
            if let Some(target) = scan_column(source.first_column - 1) {
                return Some(target);
            }
        }

        // 3. Check right adjacent column
        if source.last_column + 1 < super::sheet_ui::MAX_COLUMNS {
            if let Some(target) = scan_column(source.last_column + 1) {
                return Some(target);
            }
        }

        None
    }

    /// Fill from `source` towards the cell (`column`, `row`): down, up,
    /// right or left, whichever it lies beyond (rows first, as Excel).
    pub(crate) fn fill_to(&mut self, source: CellRange, column: u32, row: u32, copy: bool, cx: &mut Context<Self>) {
        self.fill_to_as(source, column, row, if copy { FillMode::Copy } else { FillMode::Series }, cx);
    }

    /// ⌘D / ⌘R (編集 > フィル > 下方向へコピー・右方向へコピー・上方向・左方向): the edge row
    /// (column) of the selection copied over the rest, or, with one row
    /// (column) selected, the one adjacent copied into it. The
    /// selection stays as it was.
    pub(crate) fn fill_selection_edge(&mut self, direction: FillDirection, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(r) = self.selected_range() else { return };
        let (source, column, row) = match direction {
            FillDirection::Down => {
                if r.last_row > r.first_row {
                    (CellRange { last_row: r.first_row, ..r }, r.first_column, r.last_row)
                } else if r.first_row > 0 {
                    (CellRange { first_row: r.first_row - 1, last_row: r.first_row - 1, ..r }, r.first_column, r.last_row)
                } else {
                    return;
                }
            }
            FillDirection::Right => {
                if r.last_column > r.first_column {
                    (CellRange { last_column: r.first_column, ..r }, r.last_column, r.first_row)
                } else if r.first_column > 0 {
                    (CellRange { first_column: r.first_column - 1, last_column: r.first_column - 1, ..r }, r.last_column, r.first_row)
                } else {
                    return;
                }
            }
            FillDirection::Up => {
                if r.last_row > r.first_row {
                    (CellRange { first_row: r.last_row, ..r }, r.first_column, r.first_row)
                } else {
                    (CellRange { first_row: r.last_row + 1, last_row: r.last_row + 1, ..r }, r.first_column, r.first_row)
                }
            }
            FillDirection::Left => {
                if r.last_column > r.first_column {
                    (CellRange { first_column: r.last_column, ..r }, r.first_column, r.first_row)
                } else {
                    (CellRange { first_column: r.last_column + 1, last_column: r.last_column + 1, ..r }, r.first_column, r.first_row)
                }
            }
        };
        self.fill_to(source, column, row, true, cx);
        // Back to what was selected (the fill extended it over the source).
        let slide = self.current_slide;
        if let Some(state) = self.sheet_state_mut() {
            state.last_fill = None;
            state.cells.insert(slide, (r.first_column, r.first_row));
            state.cell_ends.insert(slide, (r.last_column, r.last_row));
        }
        cx.notify();
    }

    /// `fill_to` as `mode` says (オートフィル オプション).
    pub(crate) fn fill_to_as(&mut self, source: CellRange, column: u32, row: u32, mode: FillMode, cx: &mut Context<Self>) {
        // The cells filled (beyond the source) are not a pivot table's.
        let filled = if row > source.last_row {
            Some(CellRange { first_row: source.last_row + 1, last_row: row, ..source })
        } else if row < source.first_row {
            Some(CellRange { first_row: row, last_row: source.first_row - 1, ..source })
        } else if column > source.last_column {
            Some(CellRange { first_column: source.last_column + 1, last_column: column, ..source })
        } else if column < source.first_column {
            Some(CellRange { first_column: column, last_column: source.first_column - 1, ..source })
        } else {
            None
        };
        if let Some(filled) = filled
            && self.pivot_refuses_ranges(&[filled], cx)
        {
            return;
        }
        let Some((_, sheet)) = self.current_sheet() else { return };
        let date = |style: &CellStyle| ibis_calc::numfmt::is_date_format(&style.number_format);
        let source_at = |r: u32, c: u32| -> (Source, CellStyle) {
            match sheet.cell(r, c) {
                // A value a copied array formula spilled: the fill leaves it to the formula, which spills again (#176).
                Some(cell) if super::clipboard::is_spill_ghost_within(sheet, source, r, c) => (Source { value: CellValue::Empty, formula: None, date: false }, sheet.styles[cell.style as usize].clone()),
                Some(cell) => {
                    let style = sheet.styles[cell.style as usize].clone();
                    (Source { value: cell.value.clone(), formula: cell.formula.clone(), date: date(&style) }, style)
                }
                None => (Source { value: CellValue::Empty, formula: None, date: false }, CellStyle::default()),
            }
        };
        // (vertical, backward, count)
        let (vertical, backward, count) = if row > source.last_row {
            (true, false, row - source.last_row)
        } else if row < source.first_row {
            (true, true, source.first_row - row)
        } else if column > source.last_column {
            (false, false, column - source.last_column)
        } else if column < source.first_column {
            (false, true, source.first_column - column)
        } else {
            return;
        };
        let mut cells = Vec::new();
        let lanes: Vec<u32> = if vertical { (source.first_column..=source.last_column).collect() } else { (source.first_row..=source.last_row).collect() };
        for lane in lanes {
            let along: Vec<u32> = if vertical { (source.first_row..=source.last_row).collect() } else { (source.first_column..=source.last_column).collect() };
            let pairs: Vec<(Source, CellStyle)> = along.iter().map(|&a| if vertical { source_at(a, lane) } else { source_at(lane, a) }).collect();
            let sources: Vec<Source> = pairs.iter().map(|p| p.0.clone()).collect();
            let start = along[0];
            let n = along.len() as i64;
            let series = if mode == FillMode::FormatsOnly || mode == FillMode::WithoutFormats { FillMode::Series } else { mode };
            for (j, (filled, s)) in fill_as(&sources, count as usize, backward, series).into_iter().enumerate() {
                let at = if backward { start as i64 - 1 - j as i64 } else { start as i64 + n + j as i64 } as u32;
                let (r, c) = if vertical { (at, lane) } else { (lane, at) };
                let input = match filled {
                    Filled::Value(v) => CellInput::Value(v),
                    Filled::Clear => CellInput::Clear,
                    Filled::Formula { source, offset } => {
                        let (dr, dc) = if vertical { (offset, 0) } else { (0, offset) };
                        CellInput::Formula(ibis_calc::formula::shift(&source, dr, dc))
                    }
                };
                let input = if mode == FillMode::FormatsOnly {
                    // The formats only: the cell keeps what it holds.
                    match sheet.cell(r, c) {
                        Some(t) => match &t.formula {
                            Some(f) => CellInput::Formula(f.clone()),
                            None if t.value.is_empty() => CellInput::Clear,
                            None => CellInput::Value(t.value.clone()),
                        },
                        None => CellInput::Clear,
                    }
                } else {
                    input
                };
                let style = (mode != FillMode::WithoutFormats).then(|| pairs[s].1.clone());
                cells.push(PutCell { row: r, column: c, input, style });
            }
        }
        let slide = self.current_slide;
        if let Some(state) = self.sheet_state_mut() {
            state.last_fill = Some((source, column, row, mode));
        }
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::PutCells { slide, cells })]) {
            log::warn!("fill rejected: {err}");
            self.status = Some(crate::shell::i18n::text("_xlsx.you-can-t-fill-here").into());
        }
        // The filled range stays selected, with the source.
        let filled = if vertical {
            CellRange { first_row: source.first_row.min(row), last_row: source.last_row.max(row), ..source }
        } else {
            CellRange { first_column: source.first_column.min(column), last_column: source.last_column.max(column), ..source }
        };
        let (anchor_c, anchor_r) = self.current_cell().unwrap_or((source.first_column, source.first_row));
        let far = (if anchor_c == filled.first_column { filled.last_column } else { filled.first_column }, if anchor_r == filled.first_row { filled.last_row } else { filled.first_row });
        let slide = self.current_slide;
        if let Some(state) = self.sheet_state_mut() {
            state.cell_ends.insert(slide, far);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Filled, Source, fill};
    use ibis_doc_model::CellValue;

    fn n(v: f64) -> Source {
        Source { value: CellValue::Number(v), formula: None, date: false }
    }
    fn t(v: &str) -> Source {
        Source { value: CellValue::Text(v.into()), formula: None, date: false }
    }
    fn values(f: Vec<(Filled, usize)>) -> Vec<String> {
        f.into_iter()
            .map(|(x, _)| match x {
                Filled::Value(CellValue::Number(v)) => format!("{v}"),
                Filled::Value(CellValue::Text(s)) => s,
                Filled::Formula { source, offset } => format!("{source}@{offset}"),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn series() {
        assert_eq!(values(fill(&[n(1.0), n(2.0)], 3, false, false)), ["3", "4", "5"]);
        assert_eq!(values(fill(&[n(1.0), n(3.0)], 2, true, false)), ["-1", "-3"]);
        assert_eq!(values(fill(&[n(0.1), n(0.2)], 2, false, false)), ["0.3", "0.4"]);
        assert_eq!(values(fill(&[n(5.0)], 2, false, false)), ["5", "5"], "one number is copied");
        assert_eq!(values(fill(&[n(1.0), n(2.0)], 2, false, true)), ["1", "2"], "⌥ copies");
        let day = Source { value: CellValue::Number(46203.0), formula: None, date: true };
        assert_eq!(values(fill(&[day], 2, false, false)), ["46204", "46205"], "a date steps a day");
        // Excel's trend through 1, 2, 4.
        assert_eq!(values(fill(&[n(1.0), n(2.0), n(4.0)], 1, false, false)), ["5.33333333333333"]);
    }

    #[test]
    fn lists_and_text() {
        assert_eq!(values(fill(&[t("月")], 3, false, false)), ["火", "水", "木"]);
        assert_eq!(values(fill(&[t("土曜日")], 2, false, false)), ["日曜日", "月曜日"]);
        assert_eq!(values(fill(&[t("11月")], 2, false, false)), ["12月", "1月"]);
        assert_eq!(values(fill(&[t("第1四半期")], 4, false, false)), ["第2四半期", "第3四半期", "第4四半期", "第1四半期"]);
        assert_eq!(values(fill(&[t("Jan"), t("Mar")], 2, false, false)), ["May", "Jul"]);
        assert_eq!(values(fill(&[t("第1回")], 2, false, false)), ["第2回", "第3回"]);
        assert_eq!(values(fill(&[t("A-009")], 2, false, false)), ["A-010", "A-011"]);
        assert_eq!(values(fill(&[t("売上"), t("原価")], 3, false, false)), ["売上", "原価", "売上"], "other text in turn");
        let f = Source { value: CellValue::Number(3.0), formula: Some("B2+C2".into()), date: false };
        assert_eq!(values(fill(&[f], 2, false, false)), ["B2+C2@1", "B2+C2@2"]);
    }

    #[test]
    fn fill_handle_direction() {
        // Columns 100 wide, rows 20 high; the source is A1:A2 (corner at 100, 40).
        let xs: Vec<i64> = (0..10).map(|i| i * 100).collect();
        let ys: Vec<i64> = (0..30).map(|i| i * 20).collect();
        let source = ibis_doc_model::CellRange { first_row: 0, last_row: 1, first_column: 0, last_column: 0 };
        let at = |x: i64, y: i64| {
            let column = xs.partition_point(|o| *o <= x).saturating_sub(1) as u32;
            let row = ys.partition_point(|o| *o <= y).saturating_sub(1) as u32;
            super::fill_target(source, (column, row), (x, y), &xs, &ys, (100, 20))
        };
        // A few pixels right and down: still inside the source.
        assert_eq!(at(108, 46), (0, 1));
        // Past half of the next column, not of the next row: right.
        assert_eq!(at(170, 44), (1, 1));
        // Past half of the next row only: down.
        assert_eq!(at(104, 60), (0, 3));
        // Both past half: the 45 degree line decides.
        assert_eq!(at(160, 120), (0, 6), "below the line: down");
        assert_eq!(at(400, 70), (4, 1), "above the line: right");
        // Above a source lower down: up.
        let lower = ibis_doc_model::CellRange { first_row: 4, last_row: 5, first_column: 0, last_column: 0 };
        assert_eq!(super::fill_target(lower, (0, 1), (50, 30), &xs, &ys, (100, 20)), (0, 1));
    }
}
