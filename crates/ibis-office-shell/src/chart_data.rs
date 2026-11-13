// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データの編集: the selected chart's data in the workbook's grid, embedded in
//! the chart pane (the spreadsheet screen's editor, grid only), the ranges its
//! series take their names, categories and values from framed on the
//! sheet. What is changed there goes back to the chart as it is made:
//! its series (as many as the table or the cells around the data hold),
//! their caches and the workbook kept in the file (`Source::set_chart_data`).
//! A chart with no workbook gets one made from its cached data (in memory, `Source::chart_data`), so that
//! every chart's data is edited in the same grid, in a presentation and in a document.

use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, ShapeId, ShapeKind, SlideId};
use ibis_pptx::SeriesRefs;

use crate::editor::SlideEditor;
use crate::format::with_format;

/// The colours of the frames, as PowerPoint's data sheet: the series'
/// names, the categories, the values.
const NAME_COLOR: u32 = 0xC0_50_4D;
const CATEGORY_COLOR: u32 = 0x70_30_A0;
const VALUE_COLOR: u32 = 0x44_72_C4;

/// A chart's data grid, embedded in its presentation's chart pane (the spreadsheet screen's editor, grid only).
pub struct DataWindow {
    pub view: Entity<SlideEditor>,
    pub(crate) slide: SlideId,
    pub shape: ShapeId,
    /// The ranges, as last given to the chart.
    pub refs: Vec<SeriesRefs>,
    /// The workbook's sheets as last given to the chart (to see a change).
    seen: Vec<usize>,
    _watch: Subscription,
}

/// The sheets of a workbook model, by their place in memory (a changed
/// sheet is a new one).
fn sheets_of(p: &ibis_doc_model::Presentation) -> Vec<usize> {
    p.slides
        .iter()
        .flat_map(|s| s.shapes.iter())
        .filter_map(|sh| match &sh.kind {
            ShapeKind::Sheet(sheet) => Some(std::sync::Arc::as_ptr(sheet) as usize),
            _ => None,
        })
        .collect()
}

impl SlideEditor {
    /// The data grid of the chart being edited: where it is kept is the screen's to say (`FormatEditor::chart_data_window`,
    /// `chart_data_slot`).
    pub fn chart_data_window(&self) -> Option<&DataWindow> {
        with_format!(self, chart_data_window(self))
    }

    /// The keyboard is in the data grid of the chart pane: its own editor takes the key (`guard_panel_actions` leaves it alone). Esc
    /// is the grid's only while a cell is being typed in (it gives that up); otherwise it closes the dialog.
    pub(crate) fn chart_grid_takes_keys(&self, escape: bool, window: &Window, cx: &App) -> bool {
        let Some(open) = self.chart_data_window() else { return false };
        let grid = open.view.read(cx);
        grid.focus_handle.contains_focused(window, cx) && (!escape || with_format!(grid, own_edit_open(grid)))
    }

    fn chart_data_window_mut(&mut self) -> Option<&mut DataWindow> {
        with_format!(self, chart_data_slot(self)).and_then(|slot| slot.as_mut())
    }

    fn set_chart_data_window(&mut self, window: Option<DataWindow>) {
        if let Some(slot) = with_format!(self, chart_data_slot(self)) {
            *slot = window;
        }
    }

    /// データの編集: the selected chart's pane, with its data in the workbook's grid.
    pub fn open_chart_data(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_chart_panel(window, cx);
    }

    /// The workbook editor holding the data of chart `shape` (grid only, to put in the pane), made once and kept
    /// while the pane is open. None, with the reason in the status, when the chart's data cannot be opened.
    pub fn chart_data_grid(&mut self, slide: SlideId, shape: ShapeId, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<SlideEditor>> {
        if let Some(open) = self.chart_data_window()
            && (open.slide, open.shape) == (slide, shape)
        {
            return Some(open.view.clone());
        }
        let source = self.source.clone()?;
        let data = match with_format!(self, read_chart_data(self, &source, slide, shape)) {
            Ok(d) => d,
            Err(err) => {
                self.set_status(format!("{}: {err}", crate::i18n::text("pptx.chart-data.cannot-open-chart-data")), cx);
                return None;
            }
        };
        let opened = match ibis_pptx::open(data.workbook.clone()) {
            Ok(o) => o,
            Err(err) => {
                self.set_status(format!("{}: {err}", crate::i18n::text("pptx.chart-data.cannot-open-chart-data")), cx);
                return None;
            }
        };
        let seen = sheets_of(&opened.presentation);
        let refs = data.series.clone();
        let name: SharedString = crate::i18n::text("pptx.chart-data.chart-data").into();
        let ranges = refs.clone();
        let view = cx.new(|cx| {
            let mut editor = SlideEditor::new(ibis_doc_model::Presentation::new(), window, cx);
            editor.grid_only = true;
            editor.load(opened.presentation, name, window, cx);
            editor.source = Some(std::sync::Arc::new(opened.source));
            editor.chart_ranges = Some(ranges);
            editor
        });
        let watch = cx.observe(&view, |this, view, cx| this.sync_chart_data(&view, cx));
        self.set_chart_data_window(Some(DataWindow { view: view.clone(), slide, shape, refs, seen, _watch: watch }));
        Some(view)
    }

    /// The data grid changed: if its workbook did, the chart made anew
    /// from it.
    fn sync_chart_data(&mut self, view: &Entity<SlideEditor>, cx: &mut Context<Self>) {
        let Some(open) = self.chart_data_window() else { return };
        let (slide, shape, old_refs, seen) = (open.slide, open.shape, open.refs.clone(), open.seen.clone());
        let (book, workbook_source) = {
            let v = view.read(cx);
            (v.presentation().clone(), v.source.clone())
        };
        let now = sheets_of(&book);
        if now == seen {
            return;
        }
        if let Some(open) = self.chart_data_window_mut() {
            open.seen = now;
        }
        let Some(workbook_source) = workbook_source else { return };
        let bytes = match workbook_source.save(&book, ibis_pptx::SaveOptions::default()) {
            Ok(b) => b,
            Err(err) => return log::warn!("Cannot save the chart data workbook: {err}"),
        };
        let refs = ibis_pptx::grown_refs(&book, &old_refs);
        if refs != old_refs {
            let shown = refs.clone();
            view.update(cx, |v, cx| {
                v.chart_ranges = Some(shown);
                cx.notify();
            });
        }
        if let Some(open) = self.chart_data_window_mut() {
            open.refs = refs.clone();
        }
        with_format!(self, apply_chart_data(self, slide, shape, &book, ibis_pptx::ChartData { workbook: bytes, series: refs }, cx));
    }

    /// Chart `shape`, a shape of page `slide`, made from `data` (`book` its workbook's model): its
    /// names and values put in when only they changed; else the chart as
    /// the file reads it with its series rewritten. (A chart in a document's text: `FormatEditor::apply_chart_data`.)
    pub(crate) fn apply_shape_chart_data(&mut self, slide: SlideId, shape: ShapeId, book: &ibis_doc_model::Presentation, data: ibis_pptx::ChartData, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else { return };
        let Some(ShapeKind::Chart(chart)) = self.presentation().slide(slide).and_then(|s| s.shape(shape)).map(|s| s.kind.clone()) else { return };
        // A chartEx chart is drawn from its part as written: read again from the file written with the data.
        if chart.ex.is_some() {
            let current = self.presentation().clone();
            match source.chart_ex_with_data(&current, slide, shape, data) {
                Ok(read) if self.presentation().slide(slide).and_then(|s| s.shape(shape)).is_some_and(|s| s.kind != read.kind) => self.run(vec![Command::SetKind { slide, shape, kind: Box::new(read.kind) }], cx),
                Ok(_) => {}
                Err(err) => self.set_status(format!("{}: {err}", crate::i18n::text("pptx.chart-data.cannot-apply-chart-data")), cx),
            }
            return;
        }
        let target = ibis_pptx::chart_from_data(&chart, book, &data.series);
        source.set_chart_data(shape, target.clone(), data.clone());
        let same_shape = |a: &ibis_doc_model::Chart, b: &ibis_doc_model::Chart| {
            a.ex.is_none()
                && a.groups.len() == b.groups.len()
                && a.groups.iter().zip(&b.groups).all(|(x, y)| x.series.len() == y.series.len() && x.series.iter().zip(&y.series).all(|(s, t)| s.categories == t.categories && s.values.len() == t.values.len()))
        };
        let kind = if same_shape(&chart, &target) {
            ShapeKind::Chart(Box::new(target))
        } else {
            let mut changed = self.presentation().clone();
            if let Some(t) = changed.slides.iter_mut().find(|s| s.id == slide).and_then(|s| s.shapes.iter_mut().find(|x| x.id == shape)) {
                t.kind = ShapeKind::Chart(Box::new(target));
            }
            match source.as_read(&changed, slide, shape) {
                Ok(read) => {
                    if let ShapeKind::Chart(c) = &read.kind {
                        source.set_chart_data(shape, (**c).clone(), data);
                    }
                    read.kind
                }
                Err(err) => {
                    self.set_status(format!("{}: {err}", crate::i18n::text("pptx.chart-data.cannot-apply-chart-data")), cx);
                    return;
                }
            }
        };
        if self.presentation().slide(slide).and_then(|s| s.shape(shape)).is_some_and(|s| s.kind != kind) {
            self.run(vec![Command::SetKind { slide, shape, kind: Box::new(kind) }], cx);
        }
    }

    /// The frames of a chart's ranges on the sheet shown (データの編集's
    /// window): the series' names, the categories and the values.
    pub fn render_chart_ranges(&self, scale: f32) -> Option<Div> {
        let refs = self.chart_ranges.as_ref()?;
        let (shape, sheet) = self.current_sheet()?;
        let name = self.slide().name.clone()?;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at = |offsets: &[i64], i: u32| offsets[(i as usize).min(offsets.len() - 1)];
        let here = |f: &Option<String>| -> Option<CellRange> {
            let (s, range) = ibis_pptx::parse_ref(f.as_deref()?)?;
            let s = s.strip_prefix('\'').and_then(|x| x.strip_suffix('\'')).map_or_else(|| s.clone(), |x| x.replace("''", "'"));
            (s == name).then_some(range)
        };
        let union = |ranges: Vec<CellRange>| {
            ranges.into_iter().reduce(|a, b| CellRange {
                first_row: a.first_row.min(b.first_row),
                first_column: a.first_column.min(b.first_column),
                last_row: a.last_row.max(b.last_row),
                last_column: a.last_column.max(b.last_column),
            })
        };
        let names = union(refs.iter().filter_map(|r| here(&r.name)).collect());
        let categories = refs.iter().find_map(|r| here(&r.categories));
        let values = union(refs.iter().filter_map(|r| here(&r.values)).collect());
        let frame = |range: CellRange, color: u32| {
            let x0 = (shape.frame.x + at(&xs, range.first_column)) as f32 * scale;
            let x1 = (shape.frame.x + at(&xs, range.last_column + 1)) as f32 * scale;
            let y0 = (shape.frame.y + at(&ys, range.first_row)) as f32 * scale;
            let y1 = (shape.frame.y + at(&ys, range.last_row + 1)) as f32 * scale;
            div().absolute().left(px(x0 - 1.0)).top(px(y0 - 1.0)).w(px(x1 - x0 + 2.0)).h(px(y1 - y0 + 2.0)).border_2().border_color(rgb(color))
        };
        let frames: Vec<Div> = [(names, NAME_COLOR), (categories, CATEGORY_COLOR), (values, VALUE_COLOR)].into_iter().filter_map(|(r, c)| r.map(|r| frame(r, c))).collect();
        (!frames.is_empty()).then(|| div().absolute().left(px(0.)).top(px(0.)).children(frames))
    }
}
