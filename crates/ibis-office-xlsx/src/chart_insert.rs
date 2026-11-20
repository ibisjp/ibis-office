// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > グラフ in a workbook: a chart of the selected cells (or of the
//! table around the selected cell), its series in columns or rows as
//! Excel takes them (more rows than columns: in columns), placed at the
//! selected cell.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, CellValue, Command, EmuRect};
use ibis_formats::NewChartKind;

use crate::shell::chrome::{MenuEntry, MenuItem, PRIMARY_BUTTON_BG, SidebarPanel};
use crate::shell::editor::SlideEditor;
use crate::shell::palette::rgb;
use crate::ext::*;

/// State of the「おすすめグラフ」right-pane panel (#128).
#[derive(Clone, Debug, Default)]
pub struct RecommendedChartsPanel {
    pub(crate) selected: usize,
}

/// The kinds of chart 挿入 > グラフ offers on a sheet, in menu order: the
/// item's id, its icon and the kind (`None` for a separator).
pub(crate) fn chart_items() -> Vec<MenuEntry> {
    // 「おすすめグラフ...」 first, then every kind as a picture (a 4:3 chart of the selected cells over its name),
    // three to a row (K0032, `menu_tiles.rs`).
    let mut items = vec![
        MenuItem::new("xlsx.recommended-charts", crate::shell::i18n::text("xlsx.recommended-charts.menu")).icon("insert_chart").on(|this: &mut SlideEditor, window, cx| this.open_recommended_charts(window, cx)).into(),
        MenuEntry::Separator,
    ];
    items.extend(crate::shell::chart_insert::chart_items_with_thumbs(SlideEditor::insert_sheet_chart, Some(SlideEditor::chart_kind_thumbnail)));
    // The kinds of Excel 2016 (chartEx), a workbook's only: over the selected cells.
    items.extend(crate::shell::chart_insert::chart_ex_items_with_thumbs(SlideEditor::insert_sheet_chart, SlideEditor::chart_kind_thumbnail));
    items
}

/// A chartEx chart over one column: its heading (if text) names the series, the numbers below are its values.
fn ex_refs_of_column(current: &ibis_doc_model::Presentation, sheet: &str, range: CellRange) -> Option<Vec<ibis_formats::SeriesRefs>> {
    if range.last_column != range.first_column {
        return None;
    }
    let model = ibis_formats::sheet_of(current, sheet)?;
    let heading = matches!(model.cell(range.first_row, range.first_column).map(|c| &c.value), Some(ibis_doc_model::CellValue::Text(_)));
    let first = range.first_row + u32::from(heading);
    if first > range.last_row {
        return None;
    }
    let one = |r0: u32, r1: u32| ibis_formats::format_ref(sheet, CellRange { first_row: r0, last_row: r1, ..range });
    Some(vec![ibis_formats::SeriesRefs { name: heading.then(|| one(range.first_row, range.first_row)), categories: None, values: Some(one(first, range.last_row)), ..Default::default() }])
}

/// A hierarchy for a treemap or a sunburst: a column of text for each level, the last column the sizes, under a
/// heading row if the sizes' heading is text. The categories are all the columns before the last.
fn ex_refs_of_hierarchy(current: &ibis_doc_model::Presentation, sheet: &str, range: CellRange) -> Option<Vec<ibis_formats::SeriesRefs>> {
    if range.last_column <= range.first_column {
        return None;
    }
    let model = ibis_formats::sheet_of(current, sheet)?;
    let size_column = range.last_column;
    let heading = matches!(model.cell(range.first_row, size_column).map(|c| &c.value), Some(ibis_doc_model::CellValue::Text(_)));
    let first = range.first_row + u32::from(heading);
    if first > range.last_row {
        return None;
    }
    let sizes_are_numbers = (first..=range.last_row).all(|r| matches!(model.cell(r, size_column).map(|c| &c.value), Some(ibis_doc_model::CellValue::Number(_)) | None));
    if !sizes_are_numbers {
        return None;
    }
    let part = |r0: u32, c0: u32, r1: u32, c1: u32| ibis_formats::format_ref(sheet, CellRange { first_row: r0, first_column: c0, last_row: r1, last_column: c1 });
    Some(vec![ibis_formats::SeriesRefs {
        name: heading.then(|| part(range.first_row, size_column, range.first_row, size_column)),
        categories: Some(part(first, range.first_column, range.last_row, size_column - 1)),
        values: Some(part(first, size_column, range.last_row, size_column)),
        ..Default::default()
    }])
}

/// Fallback series references for any range that has at least one numeric cell (#137).
/// Handles single-column, single-row, headerless numeric ranges, and repeats columns when a chart kind
/// (such as Bubble or Stock) requires multiple value series.
fn fallback_series_refs(
    current: &ibis_doc_model::Presentation,
    sheet: &str,
    range: CellRange,
    kind: NewChartKind,
    by_columns: bool,
) -> Option<Vec<ibis_formats::SeriesRefs>> {
    let model = ibis_formats::sheet_of(current, sheet)?;
    let is_number = |r: u32, c: u32| matches!(model.cell(r, c).map(|x| &x.value), Some(CellValue::Number(_)));
    let has_any_number = (range.first_row..=range.last_row).any(|r| (range.first_column..=range.last_column).any(|c| is_number(r, c)));
    if !has_any_number {
        return None;
    }
    let (r0, c0, r1, c1) = (range.first_row, range.first_column, range.last_row, range.last_column);
    let one = |ra: u32, ca: u32, rb: u32, cb: u32| ibis_formats::format_ref(sheet, CellRange { first_row: ra, first_column: ca, last_row: rb, last_column: cb });

    if by_columns {
        let has_header = r1 > r0 && (c0..=c1).any(|c| !is_number(r0, c)) && (c0..=c1).any(|c| (r0 + 1..=r1).any(|r| is_number(r, c)));
        let data_r0 = if has_header { r0 + 1 } else { r0 };
        let has_cat_col = c1 > c0 && (data_r0..=r1).all(|r| !is_number(r, c0));
        let val_c0 = if has_cat_col { c0 + 1 } else { c0 };
        let cat_ref = has_cat_col.then(|| one(data_r0, c0, r1, c0));
        let mut num_cols: Vec<u32> = (val_c0..=c1).filter(|&c| (data_r0..=r1).any(|r| is_number(r, c))).collect();
        if num_cols.is_empty() {
            num_cols.push(val_c0);
        }
        let needed = match kind {
            NewChartKind::StockHLC => 3,
            NewChartKind::StockOHLC | NewChartKind::StockVHLC => 4,
            NewChartKind::StockVOHLC => 5,
            NewChartKind::ComboColumnLine | NewChartKind::ComboColumnLineSecondaryAxis | NewChartKind::ComboAreaColumn => 2,
            _ if kind.is_chart_ex() => 1,
            _ => num_cols.len(),
        };
        let mut out = Vec::with_capacity(needed.max(1));
        for k in 0..needed.max(1) {
            let c = num_cols[k % num_cols.len()];
            let val_ref = one(data_r0, c, r1, c);
            let size_ref = if matches!(kind, NewChartKind::Bubble) {
                let sc = num_cols.get(k + 1).copied().unwrap_or(c);
                Some(one(data_r0, sc, r1, sc))
            } else {
                None
            };
            out.push(ibis_formats::SeriesRefs {
                name: has_header.then(|| one(r0, c, r0, c)),
                categories: cat_ref.clone(),
                values: Some(val_ref),
                sizes: size_ref,
            });
        }
        Some(out)
    } else {
        let has_header = c1 > c0 && !is_number(r0, c0) && (c0 + 1..=c1).any(|c| is_number(r0, c));
        let data_c0 = if has_header { c0 + 1 } else { c0 };
        let val_ref = one(r0, data_c0, r0, c1);
        let needed = match kind {
            NewChartKind::StockHLC => 3,
            NewChartKind::StockOHLC | NewChartKind::StockVHLC => 4,
            NewChartKind::StockVOHLC => 5,
            NewChartKind::ComboColumnLine | NewChartKind::ComboColumnLineSecondaryAxis | NewChartKind::ComboAreaColumn => 2,
            _ => 1,
        };
        let mut out = Vec::with_capacity(needed);
        for _ in 0..needed {
            out.push(ibis_formats::SeriesRefs {
                name: has_header.then(|| one(r0, c0, r0, c0)),
                categories: None,
                values: Some(val_ref.clone()),
                sizes: matches!(kind, NewChartKind::Bubble).then(|| val_ref.clone()),
            });
        }
        Some(out)
    }
}

/// The most rows and columns a picture of a chart reads (`chart_preview_range`).
const PREVIEW_ROWS: u32 = 24;
const PREVIEW_COLUMNS: u32 = 12;

type PreviewKey = (String, u32, u64);
thread_local! {
    /// The charts of `chart_preview`, by kind, shape and cells.
    static PREVIEWS: std::cell::RefCell<std::collections::HashMap<PreviewKey, Option<std::rc::Rc<ibis_doc_model::Chart>>>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The cells a new chart of `kind` is read from, as the series it shows (as 挿入 > グラフ inserts it, and the
/// recommended-charts panel and the menu's pictures draw it).
fn refs_for_new_chart(current: &ibis_doc_model::Presentation, sheet: &str, range: CellRange, kind: NewChartKind) -> Vec<ibis_formats::SeriesRefs> {
    // Prefer the orientation supported by the cell types, then point count.
    let scatter = matches!(kind, NewChartKind::Scatter | NewChartKind::ScatterSmooth | NewChartKind::ScatterLines);
    // X-Y charts and stock charts read their own layout (X column, date, high, low, close), so Excel's rule applies: in columns unless the range is wider than tall.
    let by_columns = if scatter || matches!(kind, NewChartKind::Bubble | NewChartKind::StockHLC | NewChartKind::StockOHLC | NewChartKind::StockVHLC | NewChartKind::StockVOHLC) {
        range.last_row - range.first_row >= range.last_column - range.first_column
    } else {
        ibis_formats::preferred_series_columns(current,sheet,range)
    };
    // A scatter chart: the first column the X values, a heading row of text above numbers.
    let bubble = matches!(kind, NewChartKind::Bubble);
    let is_stock = matches!(kind, NewChartKind::StockHLC | NewChartKind::StockOHLC | NewChartKind::StockVHLC | NewChartKind::StockVOHLC);
    let refs = if matches!(kind, NewChartKind::Treemap | NewChartKind::Sunburst) {
        // A hierarchy: the columns of text are its levels, the last column the sizes.
        ex_refs_of_hierarchy(current, sheet, range)
    } else if kind.is_chart_ex() {
        // One series: the categories (not for a histogram) and the values; or one column of numbers.
        ibis_formats::refs_for_range(current, sheet, range, true).or_else(|| ex_refs_of_column(current, sheet, range))
    } else if bubble {
        ibis_formats::bubble_refs_for_range(current, sheet, range, by_columns)
            .or_else(|| ibis_formats::bubble_refs_for_range(current, sheet, range, !by_columns))
    } else if is_stock {
        ibis_formats::stock_refs_for_range(current, sheet, range, kind, by_columns)
            .or_else(|| ibis_formats::stock_refs_for_range(current, sheet, range, kind, !by_columns))
    } else {
        if scatter {
            ibis_formats::scatter_refs_for_range(current, sheet, range, by_columns)
                .or_else(|| ibis_formats::scatter_refs_for_range(current, sheet, range, !by_columns))
        } else {
            None
        }
        .or_else(|| ibis_formats::refs_for_range(current, sheet, range, by_columns))
        .or_else(|| ibis_formats::refs_for_range(current, sheet, range, !by_columns))
    }
    .or_else(|| fallback_series_refs(current, sheet, range, kind, by_columns));
    refs.unwrap_or_default()
}

#[ibis_office_macros::editor_ext(ChartInsert)]
impl SlideEditor {
    /// The cells a chart is made of: the selected range, or (a single cell
    /// selected) the table around it. If that range has no numeric cells,
    /// falls back to the sheet's used numeric region if any (#137).
    pub(crate) fn chart_source_range(&self) -> Option<CellRange> {
        let range = self.selected_range().or_else(|| {
            self.current_cell().map(|(c, r)| CellRange { first_row: r, first_column: c, last_row: r, last_column: c })
        })?;
        let candidate = if range.first_row == range.last_row && range.first_column == range.last_column {
            self.current_region().unwrap_or(range)
        } else {
            range
        };
        if let Some((_, sheet)) = self.current_sheet() {
            let has_num = (candidate.first_row..=candidate.last_row)
                .any(|r| (candidate.first_column..=candidate.last_column).any(|c| matches!(sheet.cell(r, c).map(|x| &x.value), Some(CellValue::Number(_)))));
            if !has_num {
                let mut min_r = u32::MAX;
                let mut max_r = 0u32;
                let mut min_c = u32::MAX;
                let mut max_c = 0u32;
                let mut found_num = false;
                for cell in &sheet.cells {
                    let (r, c) = (cell.row, cell.column);
                    if !matches!(cell.value, CellValue::Empty) {
                        min_r = min_r.min(r);
                        max_r = max_r.max(r);
                        min_c = min_c.min(c);
                        max_c = max_c.max(c);
                        if matches!(cell.value, CellValue::Number(_)) {
                            found_num = true;
                        }
                    }
                }
                if found_num && min_r <= max_r && min_c <= max_c {
                    return Some(CellRange { first_row: min_r, first_column: min_c, last_row: max_r, last_column: max_c });
                }
            }
        }
        Some(candidate)
    }

    /// The cells a picture of a chart (the menu's tiles, the recommended-charts panel) is made of: the start of what
    /// `chart_source_range` would give, at most `PREVIEW_ROWS` rows by `PREVIEW_COLUMNS` columns. A picture of a kind
    /// of chart needs a few points of the data, as Excel's thumbnails do, not a million rows; the chart that is
    /// inserted still reads all of them (#203).
    pub(crate) fn chart_preview_range(&self) -> CellRange {
        let origin = CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 };
        let Some(range) = self.selected_range().or_else(|| self.current_cell().map(|(c, r)| CellRange { first_row: r, first_column: c, last_row: r, last_column: c })) else { return origin };
        let Some((_, sheet)) = self.current_sheet() else { return range };
        let clip = |r: CellRange| CellRange {
            last_row: r.last_row.min(r.first_row.saturating_add(PREVIEW_ROWS - 1)),
            last_column: r.last_column.min(r.first_column.saturating_add(PREVIEW_COLUMNS - 1)),
            ..r
        };
        let occupied = |r: u32, c: u32| sheet.cell(r, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some());
        let mut candidate = clip(range);
        if range.first_row == range.last_row && range.first_column == range.last_column {
            // The table around the cell, grown inside the window of the start of it.
            let mut region = range;
            let (max_row, max_column) = (range.first_row.saturating_add(PREVIEW_ROWS - 1), range.first_column.saturating_add(PREVIEW_COLUMNS - 1));
            loop {
                let old = region;
                if old.first_row > 0 && old.last_row - (old.first_row - 1) < PREVIEW_ROWS && (old.first_column..=old.last_column).any(|c| occupied(old.first_row - 1, c)) { region.first_row -= 1; }
                if old.last_row < max_row && (old.first_column..=old.last_column).any(|c| occupied(old.last_row + 1, c)) { region.last_row += 1; }
                if old.first_column > 0 && old.last_column - (old.first_column - 1) < PREVIEW_COLUMNS && (old.first_row..=old.last_row).any(|r| occupied(r, old.first_column - 1)) { region.first_column -= 1; }
                if old.last_column < max_column && (old.first_row..=old.last_row).any(|r| occupied(r, old.last_column + 1)) { region.last_column += 1; }
                if old == region { break; }
            }
            candidate = region;
        }
        let has_num = (candidate.first_row..=candidate.last_row)
            .any(|r| (candidate.first_column..=candidate.last_column).any(|c| matches!(sheet.cell(r, c).map(|x| &x.value), Some(CellValue::Number(_)))));
        if !has_num {
            // The numbers nearest the top of the sheet, as `chart_source_range` falls back to the sheet's numeric region.
            let cells = sheet.cells_in_rows(0, PREVIEW_ROWS - 1).iter().filter(|c| c.column < PREVIEW_COLUMNS && !matches!(c.value, CellValue::Empty)).collect::<Vec<_>>();
            if cells.iter().any(|c| matches!(c.value, CellValue::Number(_))) {
                let first_row = cells.iter().map(|c| c.row).min().unwrap_or(0);
                let first_column = cells.iter().map(|c| c.column).min().unwrap_or(0);
                let last_row = cells.iter().map(|c| c.row).max().unwrap_or(0);
                let last_column = cells.iter().map(|c| c.column).max().unwrap_or(0);
                return CellRange { first_row, first_column, last_row, last_column };
            }
        }
        candidate
    }

    /// 挿入 > グラフ: a chart of `kind` over the selected cells, below them.
    pub(crate) fn insert_sheet_chart(&mut self, kind: NewChartKind, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let source = match self.source.clone() {
            Some(s) => s,
            None => {
                let Some(opened) = ibis_formats::blank_file(ibis_formats::Kind::Workbook).ok().and_then(|b| ibis_formats::open(b).ok()) else { return };
                let s = std::sync::Arc::new(opened.source);
                self.source = Some(s.clone());
                s
            }
        };
        let range = self.chart_source_range().unwrap_or(CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 });
        let slide = self.current_slide;
        let Some(name) = self.presentation().slide(slide).and_then(|s| s.name.clone()) else { return };
        let sheet = ibis_formats::quote_sheet(&name);
        let current = self.presentation().clone();
        let refs = refs_for_new_chart(&current, &sheet, range, kind);
        // Below the data, as wide as five columns of it can be.
        let (x, _) = self.sheet_insertion_origin();
        let y = self.current_sheet().map_or(952_500, |(shape, sheet)| shape.frame.y + sheet.row_offsets().get(range.last_row as usize + 1).copied().unwrap_or(0) + 190_500);
        // Not on a chart already there: a half inch down and right of it, as Excel cascades them.
        let (mut x, mut y) = (x, y);
        while self.slide().shapes.iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_)) && (s.frame.x - x).abs() < 100_000 && (s.frame.y - y).abs() < 100_000) {
            x += 457_200;
            y += 457_200;
        }
        let frame = EmuRect::new(x, y, 4_572_000, 2_743_200);
        let id = self.document.ids().new_shape_id();
        let mut shape = match source.new_sheet_chart(&current, slide, id, frame, kind, refs) {
            Ok(shape) => shape,
            Err(err) => {
                log::warn!("Cannot insert the chart: {err}");
                self.status = Some(crate::shell::i18n::format("_xlsx.this-operation-can-t-be-done-err", &[("err", &err.to_string())]).into());
                cx.notify();
                return;
            }
        };
        if let ibis_doc_model::ShapeKind::Chart(chart)=&mut shape.kind {
            chart.title.get_or_insert_with(||ibis_doc_model::ChartTitle{text:crate::shell::i18n::text("_xlsx.chart-title").to_string(),style:ibis_doc_model::ChartText{size:1400,..Default::default()},at:None});
            if !kind.is_chart_ex(){
                // Existing theme accent colors provide the document's colorful series cycle.
                let accents=(1..=6).map(|n|current.theme_colors.iter().find(|(key,_)|key==&format!("accent{n}")).map(|(_,c)|*c).unwrap_or(ibis_doc_model::Rgb(COLOR_PALETTES[0].1[n-1]))).collect::<Vec<_>>();
                for (i,series)in chart.groups.iter_mut().flat_map(|g|&mut g.series).enumerate(){let color=ibis_doc_model::Color::opaque(accents[i%accents.len()]);if !matches!(series.fill,ibis_doc_model::Fill::None){series.fill=ibis_doc_model::Fill::Solid(color);}if let Some(stroke)=series.stroke.as_mut(){stroke.color=color;}if let Some(marker)=series.marker.as_mut(){marker.fill=ibis_doc_model::Fill::Solid(color);}}
            }

        }
        let index = self.slide().shapes.len();
        self.run_sheet(vec![Command::InsertShape { slide, shapes: vec![(index, shape)] }], cx);
        self.selection = Some(id);
        let _ = window;
        cx.notify();
    }

    /// A picture of the chart `kind` would make of `range`, `width` pixels wide and `ratio` (height over width) as
    /// tall: a tile of the recommended-charts panel (5:3) and of the 挿入 > グラフ menu (4:3) draw it with this one
    /// part. The chart is the one inserting `kind` makes (`refs_for_new_chart`), drawn by the sheet's chart view.
    pub(crate) fn chart_preview_element(&self, kind: NewChartKind, range: CellRange, width: f32, ratio: f32) -> AnyElement {
        const WIDTH_EMU: i64 = 4_572_000;
        let frame = EmuRect::new(0, 0, WIDTH_EMU, (WIDTH_EMU as f32 * ratio).round() as i64);
        match self.chart_preview(kind, range, frame, ratio) {
            Some(chart) => crate::shell::chart_insert::chart_preview_view(self, &chart, width, ratio),
            None => div().size_full().bg(rgb(0xf5f5f5)).into_any_element(),
        }
    }

    /// The 4:3 picture of the chart `kind` in the 挿入 > グラフ menu. Nothing chosen, or a single cell: the same picture as the presentation's
    /// and the document's menus (the kind's own look over the fixed data, `shell::chart_insert::kind_thumbnail`). Cells chosen that draw it: the
    /// chart of those cells, without the title, legend and data table that the other screens' pictures leave out (K0032).
    pub(crate) fn chart_kind_thumbnail(&self, kind: NewChartKind, width: f32) -> AnyElement {
        let single = self.selected_range().is_none_or(|r| r.first_row == r.last_row && r.first_column == r.last_column);
        if !single {
            let range = self.chart_preview_range();
            // A kind the cells do not draw (a Pareto chart of text) is the fixed picture, not an empty one.
            if let Some(chart) = self.chart_preview(kind, range, EmuRect::new(0, 0, 4_572_000, 3_429_000), 3.0 / 4.0).filter(|c| c.groups.iter().any(|g| g.series.iter().any(|s| !s.values.is_empty()))) {
                return crate::shell::chart_insert::chart_preview_view(self, &crate::shell::chart_insert::chart_without_preview_chrome((*chart).clone()), width, 3.0 / 4.0);
            }
        }
        crate::shell::chart_insert::kind_thumbnail(self, "xlsx", kind, width, || crate::shell::chart_insert::blank_presentation_chart(kind))
    }

    /// The chart `kind` makes of `range` (kept while the cells, the kind and the shape do not change: the menu draws
    /// dozens of them on every frame).
    fn chart_preview(&self, kind: NewChartKind, range: CellRange, frame: EmuRect, ratio: f32) -> Option<std::rc::Rc<ibis_doc_model::Chart>> {
        let source = self.source.as_ref()?;
        let name = self.slide().name.clone().unwrap_or_default();
        let key = (format!("{kind:?}"), ratio.to_bits(), self.chart_data_fingerprint(&name, range));
        if let Some(found) = PREVIEWS.with(|cache| cache.borrow().get(&key).cloned()) {
            return found;
        }
        let sheet = ibis_formats::quote_sheet(&name);
        let refs = refs_for_new_chart(self.presentation(), &sheet, range, kind);
        let made = source.new_sheet_chart_preview(self.presentation(), self.current_slide, ibis_doc_model::ShapeId(u64::MAX), frame, kind, refs, range.last_row).ok();
        let chart = match made {
            Some(ibis_doc_model::Shape { kind: ibis_doc_model::ShapeKind::Chart(chart), .. }) => Some(std::rc::Rc::new(*chart)),
            _ => None,
        };
        PREVIEWS.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() > 256 {
                cache.clear();
            }
            cache.insert(key, chart.clone());
        });
        chart
    }

    /// A number that changes when the sheet's name, the range or a cell of the range (the first 512) does.
    fn chart_data_fingerprint(&self, sheet: &str, range: CellRange) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (sheet, range.first_row, range.last_row, range.first_column, range.last_column).hash(&mut hasher);
        if let Some((_, model)) = self.current_sheet() {
            let mut seen = 0;
            'cells: for row in range.first_row..=range.last_row {
                for column in range.first_column..=range.last_column {
                    match model.cell(row, column).map(|c| &c.value) {
                        Some(CellValue::Number(n)) => (1u8, n.to_bits()).hash(&mut hasher),
                        Some(CellValue::Text(t)) => (2u8, t).hash(&mut hasher),
                        Some(_) => 3u8.hash(&mut hasher),
                        None => 0u8.hash(&mut hasher),
                    }
                    seen += 1;
                    if seen >= 512 {
                        break 'cells;
                    }
                }
            }
        }
        hasher.finish()
    }

    /// Returns the list of recommended chart types `(NewChartKind, label_id, icon_name)` for the current selection (#128).
    pub(crate) fn recommended_chart_candidates(&self) -> Vec<(NewChartKind, &'static str, &'static str)> {
        let range = self.chart_preview_range();
        let rows = (range.last_row - range.first_row).max(1);
        let cols = (range.last_column - range.first_column).max(1);
        // A time axis (dates or times down the first column, or across the first row) makes the line chart the first
        // choice, as in Excel: the dates are the X values and every other column (row) a series over time.
        let time_axis = self.current_sheet().is_some_and(|(_, sheet)| {
            let name = self.slide().name.clone().unwrap_or_default();
            let by_columns = ibis_formats::preferred_series_columns(self.presentation(), &ibis_formats::quote_sheet(&name), range);
            ibis_formats::leading_line_is_time_axis(sheet, range, by_columns)
        });
        if !time_axis && (rows <= 1 || cols <= 1) {
            vec![
                (NewChartKind::ClusteredColumn, "pptx.insert-chart.column", "bar_chart"),
                (NewChartKind::Pie, "pptx.insert-chart.pie", "pie_chart"),
                (NewChartKind::ClusteredBar, "pptx.insert-chart.bar", "bar_chart"),
                (NewChartKind::Line, "pptx.insert-chart.line", "insert_chart"),
            ]
        } else if time_axis || (rows >= 6 && cols <= 3) {
            vec![
                (NewChartKind::Line, "pptx.insert-chart.line", "insert_chart"),
                (NewChartKind::ClusteredColumn, "pptx.insert-chart.column", "bar_chart"),
                (NewChartKind::Area, "pptx.insert-chart.area", "area_chart"),
                (NewChartKind::Scatter, "pptx.insert-chart.scatter", "polyline"),
            ]
        } else {
            vec![
                (NewChartKind::ClusteredColumn, "pptx.insert-chart.column", "bar_chart"),
                (NewChartKind::StackedColumn, "pptx.insert-chart.stacked-column", "bar_chart"),
                (NewChartKind::ClusteredBar, "pptx.insert-chart.bar", "bar_chart"),
                (NewChartKind::Line, "pptx.insert-chart.line", "insert_chart"),
                (NewChartKind::Pie, "pptx.insert-chart.pie", "pie_chart"),
            ]
        }
    }

    /// 挿入 > おすすめグラフ (xlsx.recommended-charts):
    /// Opens the「おすすめグラフ」panel in the right sidebar (#128).
    pub fn open_recommended_charts(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let state = self.wb_mut();
        state.close_settings();
        state.panels.recommended_charts = Some(RecommendedChartsPanel { selected: 0 });
        self.sidebar_open = true;
        cx.notify();
    }

    /// Inserts the currently selected chart from the「おすすめグラフ」panel (#128).
    pub(crate) fn insert_recommended_chart(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self
            .sheet_state()
            .and_then(|s| s.panels.recommended_charts.as_ref())
            .map(|p| p.selected)
            .unwrap_or(0);
        let candidates = self.recommended_chart_candidates();
        let kind = candidates
            .get(selected)
            .or_else(|| candidates.first())
            .map(|(k, _, _)| *k)
            .unwrap_or(NewChartKind::ClusteredColumn);
        self.wb_mut().panels.recommended_charts = None;
        self.insert_sheet_chart(kind, window, cx);
        self.status = Some(crate::shell::i18n::text("xlsx.recommended-chart-inserted").into());
        cx.notify();
    }

    /// Renders the「おすすめグラフ」panel in the right sidebar (#128).
    pub fn recommended_charts_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.recommended_charts.as_ref()?;
        let selected = p.selected;
        let candidates = self.recommended_chart_candidates();
        let range = self.chart_preview_range();
        let has_num = self.current_sheet().is_some_and(|(_, sheet)| {
            (range.first_row..=range.last_row)
                .any(|r| (range.first_column..=range.last_column).any(|c| matches!(sheet.cell(r, c).map(|x| &x.value), Some(CellValue::Number(_)))))
        });

        let mut body = v_flex().gap_2().px(px(16.)).py(px(10.)).p_0();
        if !has_num {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(crate::shell::i18n::text("xlsx.recommended-charts.empty")),
            );
        }
        let mut tiles=Vec::new();
        for (idx,(kind,label_id,_))in candidates.iter().enumerate(){
            let preview=self.chart_preview_element(*kind,range,220.,0.6);
            tiles.push(v_flex().id(SharedString::from(format!("rec-chart-{idx}"))).flex_1().min_w_0().gap_1().p_2().rounded(px(6.)).cursor_pointer().border_1().border_color(if idx==selected{rgb(PRIMARY_BUTTON_BG).into()}else{cx.theme().border}).hover(|d|d.bg(rgb(0xf3f4f6))).child(div().relative().w_full().h(px(132.)).overflow_hidden().child(preview)).child(div().text_size(px(12.)).child(crate::shell::i18n::text(label_id))).on_click(cx.listener(move |e,_,_,cx|{if let Some(p)=e.wb_mut().panels.recommended_charts.as_mut(){p.selected=idx;}cx.notify();})).into_any_element());
        }
        let mut iter=tiles.into_iter();while let Some(first)=iter.next(){let mut row=h_flex().w_full().gap_2().items_stretch().child(first);if let Some(second)=iter.next(){row=row.child(second);}else{row=row.child(div().flex_1());}body=body.child(row);}
        let footer = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("rec-chart-insert", "", crate::shell::chrome::ActionButtonKind::Primary)
                    .label(crate::shell::i18n::text("_shell.insert"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.insert_recommended_chart(window, cx);
                    })),
            )

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(
            crate::shell::i18n::text("xlsx.recommended-charts"),
            body.into_any_element(),
            footer,
        ))
    }
}

/// The color palettes offered for series coloring: the spreadsheet's names for the shared colors
/// (`shell::chart_panel::PALETTE_COLORS`).
pub(crate) const COLOR_PALETTES: [(&str, &[u32]); 10] = [
    ("xlsx.chart-palette-1", crate::shell::chart_panel::PALETTE_COLORS[0]),
    ("xlsx.chart-palette-2", crate::shell::chart_panel::PALETTE_COLORS[1]),
    ("xlsx.chart-palette-3", crate::shell::chart_panel::PALETTE_COLORS[2]),
    ("xlsx.chart-palette-4", crate::shell::chart_panel::PALETTE_COLORS[3]),
    ("_xlsx.monochrome-1", crate::shell::chart_panel::PALETTE_COLORS[4]),
    ("_xlsx.monochrome-2", crate::shell::chart_panel::PALETTE_COLORS[5]),
    ("_xlsx.monochrome-3", crate::shell::chart_panel::PALETTE_COLORS[6]),
    ("_xlsx.monochrome-4", crate::shell::chart_panel::PALETTE_COLORS[7]),
    ("_xlsx.monochrome-5", crate::shell::chart_panel::PALETTE_COLORS[8]),
    ("_xlsx.monochrome-6", crate::shell::chart_panel::PALETTE_COLORS[9]),
];
