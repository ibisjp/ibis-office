// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The cells of a pivot table are not typed over (measured on Excel 16.0 build 20527, see
//! tasks/doc/reports/2026-11-03-pivot-insert-panel-protect.md):
//! - the names (a field's, an item's, a value's, 行ラベル / 列ラベル, 総計) are renamed by typing over them;
//! - a value, or a blank cell inside the table, refuses an entry (「ピボットテーブルで現在選択されている部分は変更できません。」);
//! - deleting, pasting, filling, merging, cutting, or inserting / deleting rows and columns through the table is
//!   refused (「ピボットテーブルに影響するため…」); the formats (number format, fill, border, font) are free.

use ibis_doc_model::{CellRange, CellValue, PivotTable, SlideId};

use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

use gpui_kit::Context;
use crate::ext::*;

/// What typing over a label renames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Rename {
    RowHeader,
    ColHeader,
    Values,
    Grand,
    DataField(usize),
    Field(usize),
    Item { field: usize, key: String },
}

/// What a cell of a pivot table is, for an entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PivotPart {
    /// Refuses an entry.
    Locked,
    Rename(Rename),
    /// A report filter's item (the page field at this place): typing an item of the field keeps that one.
    PageItem(usize),
}

const SENTINEL: char = '\u{1}';
const SEPARATOR: char = '\u{2}';

fn contains(r: &CellRange, row: u32, column: u32) -> bool {
    (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column)
}

fn intersects(a: &CellRange, b: &CellRange) -> bool {
    a.first_row <= b.last_row && b.first_row <= a.last_row && a.first_column <= b.last_column && b.first_column <= a.last_column
}

/// The cells of the report filters (a row each, two columns), without the blank row under them.
fn page_block(p: &PivotTable) -> Option<CellRange> {
    let pages = p.page_fields.len() as u32;
    (pages > 0).then(|| {
        let first = p.location.first_row.saturating_sub(pages + 1);
        CellRange { first_row: first, last_row: first + pages - 1, first_column: p.location.first_column, last_column: p.location.first_column + 1 }
    })
}

/// The whole of a table: its place and its report filters with the blank row under them.
fn area(p: &PivotTable) -> CellRange {
    let pages = p.page_fields.len() as u32;
    let top = if pages > 0 { p.location.first_row.saturating_sub(pages + 1) } else { p.location.first_row };
    CellRange { first_row: top, ..p.location }
}

fn cell_text(v: &CellValue) -> String {
    match v {
        CellValue::Text(t) => t.clone(),
        other => ibis_doc_model::pivot_item_key(other),
    }
}

/// The table with a recognisable name on every part that can be named, so a built cell tells what it shows.
fn marked(p: &PivotTable) -> PivotTable {
    let mut m = p.clone();
    m.row_header_caption = Some(format!("{SENTINEL}row"));
    m.col_header_caption = Some(format!("{SENTINEL}col"));
    m.data_caption = Some(format!("{SENTINEL}val"));
    m.grand_total_caption = Some(format!("{SENTINEL}grand"));
    for (d, df) in m.data_fields.iter_mut().enumerate() {
        df.name = format!("{SENTINEL}d{d}");
    }
    m.aliases = Default::default();
    for (f, field) in p.fields.iter().enumerate() {
        m.aliases.fields.push((f as u32, format!("{SENTINEL}f{f}")));
        for v in &field.shared {
            let key = ibis_doc_model::pivot_item_key(v);
            m.aliases.items.push((f as u32, key.clone(), format!("{SENTINEL}i{f}{SEPARATOR}{key}")));
        }
    }
    m
}

fn display_of_item(p: &PivotTable, f: usize, v: &CellValue) -> String {
    let key = ibis_doc_model::pivot_item_key(v);
    p.aliases.item(f, &key).map_or(key, str::to_string)
}

#[ibis_office_macros::editor_ext(PivotProtect)]
impl SlideEditor {
    fn pivots_of(&self, slide: SlideId) -> Vec<PivotTable> {
        self.presentation().slide(slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide).map(|s| s.pivots.clone()).unwrap_or_default()
    }

    /// What the cell at (`row`, `column`) is, and the table (its index) when it is in one.
    pub(crate) fn pivot_part_at(&self, slide: SlideId, row: u32, column: u32) -> Option<(usize, PivotPart)> {
        let pivots = self.pivots_of(slide);
        let index = pivots.iter().position(|p| contains(&p.location, row, column) || page_block(p).is_some_and(|b| contains(&b, row, column)))?;
        let mut pivot = pivots[index].clone();
        if let Some(data) = ibis_pptx::pivot_data(self.presentation(), slide, index) {
            pivot.fill_shared(&data);
            let built = marked(&pivot).build(&data);
            let Some(cell) = built.cells.iter().find(|c| c.row == row && c.column == column) else {
                return Some((index, PivotPart::Locked));
            };
            use ibis_doc_model::pivot::PivotRole;
            let text = cell_text(&cell.value);
            let part = match cell.role {
                PivotRole::Value => PivotPart::Locked,
                PivotRole::PageValue => page_block(&pivot).map_or(PivotPart::Locked, |b| PivotPart::PageItem((row - b.first_row) as usize)),
                _ => match text.strip_prefix(SENTINEL) {
                    Some("row") => PivotPart::Rename(Rename::RowHeader),
                    Some("col") => PivotPart::Rename(Rename::ColHeader),
                    Some("val") => PivotPart::Rename(Rename::Values),
                    Some("grand") => PivotPart::Rename(Rename::Grand),
                    Some(rest) if rest.starts_with('d') => rest[1..].parse().map_or(PivotPart::Locked, |d| PivotPart::Rename(Rename::DataField(d))),
                    Some(rest) if rest.starts_with('f') => rest[1..].parse().map_or(PivotPart::Locked, |f| PivotPart::Rename(Rename::Field(f))),
                    Some(rest) if rest.starts_with('i') && !text.ends_with(" 集計") => match rest[1..].split_once(SEPARATOR) { // i18n-ok（Excel の日本語のメッセージ・小計の見出しとの照合）
                        Some((f, key)) => f.parse().map_or(PivotPart::Locked, |field| PivotPart::Rename(Rename::Item { field, key: key.to_string() })),
                        None => PivotPart::Locked,
                    },
                    _ => PivotPart::Locked,
                },
            };
            return Some((index, part));
        }
        Some((index, PivotPart::Locked))
    }

    fn pivot_notice(&mut self, id: &str, cx: &mut Context<Self>) {
        self.status = Some(text(id));
        cx.notify();
    }

    /// Whether an entry in the current cell is refused (a value, a blank cell of the table): then says so.
    pub(crate) fn pivot_entry_refused(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((column, row)) = self.current_cell() else { return false };
        if self.pivot_part_at(self.current_slide, row, column).is_some_and(|(_, part)| part == PivotPart::Locked) {
            self.pivot_notice("xlsx.pivot.cannot-change-part", cx);
            return true;
        }
        false
    }

    /// Whether changing the cells of `ranges` (delete, paste, fill, merge, cut) would change a pivot table: then says so.
    pub(crate) fn pivot_refuses_ranges(&mut self, ranges: &[CellRange], cx: &mut Context<Self>) -> bool {
        let hit = self.pivots_of(self.current_slide).iter().any(|p| ranges.iter().any(|r| intersects(r, &p.location) || page_block(p).is_some_and(|b| intersects(r, &b))));
        if hit {
            self.pivot_notice("xlsx.pivot.affects-pivot", cx);
        }
        hit
    }

    /// Whether inserting rows (columns) at `at`, or deleting `first..=last`, goes through a pivot table: then says so.
    /// `at` is the line the new ones come before (`None`: deleting).
    pub(crate) fn pivot_refuses_lines(&mut self, rows: bool, first: u32, last: u32, at: Option<u32>, cx: &mut Context<Self>) -> bool {
        let hit = self.pivots_of(self.current_slide).iter().any(|p| {
            let a = area(p);
            let (lo, hi) = if rows { (a.first_row, a.last_row) } else { (a.first_column, a.last_column) };
            match at {
                Some(at) => lo < at && at <= hi,
                None => first <= hi && lo <= last,
            }
        });
        if hit {
            self.pivot_notice("xlsx.pivot.affects-pivot", cx);
        }
        hit
    }

    /// The entry finishing in a cell of a pivot table: renames what a label is, refuses the rest. `true`: handled.
    pub(crate) fn commit_pivot_entry(&mut self, slide: SlideId, row: u32, column: u32, entry: &str, cx: &mut Context<Self>) -> bool {
        let Some((index, part)) = self.pivot_part_at(slide, row, column) else { return false };
        match part {
            PivotPart::Locked => self.pivot_notice("xlsx.pivot.cannot-change-part", cx),
            PivotPart::Rename(rename) => {
                if let Err(message) = self.rename_pivot_part(slide, index, rename, entry, cx) {
                    self.status = Some(message);
                    cx.notify();
                }
            }
            PivotPart::PageItem(i) => {
                if let Err(message) = self.set_pivot_page_item(slide, index, i, entry, cx) {
                    self.status = Some(message);
                    cx.notify();
                }
            }
        }
        true
    }

    fn rename_pivot_part(&mut self, slide: SlideId, index: usize, rename: Rename, name: &str, cx: &mut Context<Self>) -> Result<(), gpui_kit::SharedString> {
        let pivot = self.pivots_of(slide).get(index).cloned().ok_or_else(|| text("xlsx.pivot.cannot-change-part"))?;
        if name.starts_with('=') {
            return Err(text("xlsx.pivot.name-formula"));
        }
        if name.is_empty() {
            return Err(text("xlsx.pivot.name-null"));
        }
        let field_names: Vec<String> = pivot.fields.iter().enumerate().map(|(f, field)| pivot.aliases.field(f).map_or_else(|| field.name.clone(), str::to_string)).collect();
        let value_names: Vec<&str> = pivot.data_fields.iter().map(|d| d.name.as_str()).collect();
        let taken = |except_field: Option<usize>, except_value: Option<usize>| {
            field_names.iter().enumerate().any(|(f, n)| Some(f) != except_field && n == name) || value_names.iter().enumerate().any(|(d, n)| Some(d) != except_value && *n == name)
        };
        let target = (slide, index);
        match rename {
            Rename::RowHeader => self.change_pivot(target, |p| p.row_header_caption = Some(name.to_string()), cx),
            Rename::ColHeader => self.change_pivot(target, |p| p.col_header_caption = Some(name.to_string()), cx),
            Rename::Values => self.change_pivot(target, |p| p.data_caption = Some(name.to_string()), cx),
            Rename::Grand => self.change_pivot(target, |p| p.grand_total_caption = Some(name.to_string()), cx),
            Rename::DataField(d) => {
                if taken(None, Some(d)) {
                    return Err(text("xlsx.pivot.name-exists"));
                }
                self.change_pivot(target, |p| p.set_data_field_name(d, name), cx);
            }
            Rename::Field(f) => {
                if taken(Some(f), None) {
                    return Err(text("xlsx.pivot.name-exists"));
                }
                let original = pivot.fields.get(f).map(|x| x.name.clone()).unwrap_or_default();
                self.change_pivot(target, |p| p.aliases.set_field(f, name, &original), cx);
            }
            Rename::Item { field, key } => {
                let Some(fld) = pivot.fields.get(field) else { return Err(text("xlsx.pivot.cannot-change-part")) };
                let current = pivot.aliases.item(field, &key).unwrap_or(&key).to_string();
                if current == name {
                    return Ok(());
                }
                let sibling = fld.shared.iter().any(|v| ibis_doc_model::pivot_item_key(v) != key && display_of_item(&pivot, field, v) == name);
                if sibling {
                    return Err(text("xlsx.pivot.name-exists"));
                }
                self.change_pivot(target, |p| p.aliases.set_item(field, &key, name), cx);
            }
        }
        Ok(())
    }

    fn set_pivot_page_item(&mut self, slide: SlideId, index: usize, page: usize, entry: &str, cx: &mut Context<Self>) -> Result<(), gpui_kit::SharedString> {
        let pivot = self.pivots_of(slide).get(index).cloned().ok_or_else(|| text("xlsx.pivot.cannot-change-part"))?;
        let Some(pf) = pivot.page_fields.get(page) else { return Err(text("xlsx.pivot.cannot-change-part")) };
        if entry.starts_with('=') {
            return Err(text("xlsx.pivot.name-formula"));
        }
        let field = pf.field as usize;
        let Some(fld) = pivot.fields.get(field) else { return Err(text("xlsx.pivot.cannot-change-part")) };
        let found = fld.items.iter().position(|it| it.kind.is_none() && it.x.and_then(|x| fld.shared.get(x as usize)).is_some_and(|v| display_of_item(&pivot, field, v) == entry));
        match found {
            Some(k) => {
                self.change_pivot((slide, index), |p| {
                    if let Some(pf) = p.page_fields.get_mut(page) {
                        pf.item = Some(k as u32);
                    }
                }, cx);
                Ok(())
            }
            None => Err(format("xlsx.pivot.not-an-item", &[("item", &entry)]).into()),
        }
    }
}
