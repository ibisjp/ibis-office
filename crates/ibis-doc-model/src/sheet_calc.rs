// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A workbook's model as the calculation engine sees it (`ibis_calc`'s
//! [`Book`]), and recalculating after cells change.

use std::cell::OnceCell;

use ibis_calc::eval::Book;
use ibis_calc::recalc::{CellId, Recalculated};

use crate::model::{Presentation, ShapeKind, SlideId};
use crate::sheet::{CellValue, DefinedName, Sheet};

/// The sheets of a workbook, in the workbook's order, with their cells.
pub struct BookView<'a> {
    sheets: Vec<BookSheet<'a>>,
    /// How many of `sheets` are the book's own.
    real_sheets: usize,
    names: &'a [DefinedName],
    date1904: bool,
    compatibility_version: u32,
    iterate: bool,
    max_iterations: u32,
    max_change: f64,
    calc_index: Option<crate::sheet::CalcIndex>,
}

struct BookSheet<'a> {
    name: String,
    slide: Option<SlideId>,
    sheet: Option<&'a Sheet>,
    extent: OnceCell<(u32, u32)>,
    /// A sheet of an external workbook: its cached cells.
    external: Option<(&'a str, &'a crate::sheet::ExternalSheet)>,
}

/// The worksheet shape of a slide.
pub fn sheet_of_slide(slide: &crate::model::Slide) -> Option<&std::sync::Arc<Sheet>> {
    slide.shapes.iter().find_map(|s| match &s.kind {
        ShapeKind::Sheet(sheet) => Some(sheet),
        _ => None,
    })
}

/// Defined names of `presentation`'s workbook.
pub(crate) fn collect_defined_names(presentation: &Presentation) -> Vec<DefinedName> {
    presentation.workbook.as_deref().map(|wb| wb.names.clone()).unwrap_or_default()
}

impl<'a> BookView<'a> {
    pub fn new(presentation: &'a Presentation) -> Self {
        let mut sheets = Vec::new();
        match presentation.workbook.as_deref() {
            Some(wb) => {
                for s in &wb.sheets {
                    let sheet = match s.slide {
                        Some(id) => presentation.slide(id).and_then(sheet_of_slide).map(|a| &**a),
                        None => s.hidden.as_deref(),
                    };
                    sheets.push(BookSheet { name: s.name.clone(), slide: s.slide, sheet, extent: OnceCell::new(), external: None });
                }
            }
            None => {
                for (i, slide) in presentation.slides.iter().enumerate() {
                    if let Some(sheet) = sheet_of_slide(slide) {
                        let name = slide.name.clone().unwrap_or_else(|| format!("Sheet{}", i + 1));
                        sheets.push(BookSheet { name, slide: Some(slide.id), sheet: Some(sheet), extent: OnceCell::new(), external: None });
                    }
                }
            }
        }
        // The external workbooks' cached sheets come after the sheets of the
        // book (`sheet_count` leaves them out); formulas read them by index.
        let real_sheets = sheets.len();
        if let Some(wb) = presentation.workbook.as_deref() {
            for book in &wb.external_books {
                for sheet in &book.sheets {
                    sheets.push(BookSheet { name: sheet.name.clone(), slide: None, sheet: None, extent: OnceCell::new(), external: Some((book.name.as_str(), sheet)) });
                }
            }
        }
        let (names, date1904, iterate, max_iterations, max_change) = match presentation.workbook.as_deref() {
            Some(wb) => (wb.names.as_slice(), wb.date1904, wb.iterate, wb.max_iterations, wb.max_change.0),
            None => (&[][..], false, false, 100, 0.001),
        };
        let compatibility_version = presentation.workbook.as_deref().and_then(|w| w.compatibility_version).unwrap_or(1);
        let calc_index = presentation.workbook.as_deref().map(|wb| wb.calc_index.clone());
        Self { sheets, real_sheets, names, date1904, compatibility_version, iterate, max_iterations, max_change, calc_index }
    }

    /// The index of the sheet on `slide`.
    pub fn index_of_slide(&self, slide: SlideId) -> Option<usize> {
        self.sheets.iter().position(|s| s.slide == Some(slide))
    }

    pub fn slide_of(&self, index: usize) -> Option<SlideId> {
        self.sheets.get(index).and_then(|s| s.slide)
    }

    /// Every formula cell of the workbook.
    pub fn formula_cells(&self) -> Vec<CellId> {
        let mut out = Vec::new();
        for (i, s) in self.sheets.iter().enumerate() {
            if let Some(sheet) = s.sheet {
                out.extend(sheet.cells.iter().filter(|c| c.formula.is_some()).map(|c| (i, c.row, c.column)));
            }
        }
        out
    }

    /// The cells whose formula calls IMAGE (calculated when the book is opened: the picture they ask for is a result of
    /// the calculation, not kept in the file).
    pub fn image_formula_cells(&self) -> Vec<CellId> {
        let mut out = Vec::new();
        for (i, s) in self.sheets.iter().enumerate() {
            if let Some(sheet) = s.sheet {
                out.extend(sheet.cells.iter().filter(|c| c.formula.as_deref().is_some_and(|f| f.as_bytes().windows(6).any(|w| w.eq_ignore_ascii_case(b"IMAGE(")))).map(|c| (i, c.row, c.column)));
            }
        }
        out
    }

    /// The cells whose formula calls the function `name` (upper case, without the parenthesis).
    pub fn formula_cells_calling(&self, name: &str) -> Vec<CellId> {
        let call = format!("{name}(");
        let mut out = Vec::new();
        for (i, s) in self.sheets.iter().enumerate() {
            if let Some(sheet) = s.sheet {
                out.extend(sheet.cells.iter().filter(|c| c.formula.as_deref().is_some_and(|f| f.as_bytes().windows(call.len()).any(|w| w.eq_ignore_ascii_case(call.as_bytes())))).map(|c| (i, c.row, c.column)));
            }
        }
        out
    }

    /// The cells whose formula calls a volatile function (Excel calculates them again whenever a book is opened).
    /// Read from the formula text, not from the dependency index: it is not built for a book without such cells.
    pub fn volatile_formula_cells(&self) -> Vec<CellId> {
        const NAMES: [&[u8]; 9] = [b"TODAY(", b"NOW(", b"RAND(", b"RANDBETWEEN(", b"RANDARRAY(", b"OFFSET(", b"INDIRECT(", b"CELL(", b"INFO("];
        // A book whose file is not known (bytes opened without a path) keeps what CELL and INFO were cached as: with no
        // path, CELL("filename") is "" and the common sheet-name formula built on it would turn into #VALUE!.
        let known_file = ibis_calc::eval::convert::workbook_file().is_some();
        let mut out = Vec::new();
        for (i, s) in self.sheets.iter().enumerate() {
            if let Some(sheet) = s.sheet {
                out.extend(
                    sheet
                        .cells
                        .iter()
                        .filter(|c| {
                            c.formula.as_deref().is_some_and(|f| {
                                NAMES.iter().filter(|n| known_file || !(n.starts_with(b"CELL") || n.starts_with(b"INFO"))).any(|n| f.as_bytes().windows(n.len()).any(|w| w.eq_ignore_ascii_case(n)))
                            })
                        })
                        .map(|c| (i, c.row, c.column)),
                );
            }
        }
        out
    }

    /// The formula cells with no value: a file written by a program that does
    /// not calculate (C#'s Open XML SDK, openpyxl) has none cached.
    pub fn formula_cells_without_value(&self) -> Vec<CellId> {
        let mut out = Vec::new();
        for (i, s) in self.sheets.iter().enumerate() {
            if let Some(sheet) = s.sheet {
                out.extend(sheet.cells.iter().filter(|c| c.formula.is_some() && c.value.is_empty()).map(|c| (i, c.row, c.column)));
            }
        }
        out
    }

    /// Calculate exactly `cells` (see `DependencyIndex::calculate_only`).
    pub fn calculate_only(&self, cells: &[CellId]) -> Vec<Recalculated> {
        if let Some(calc_index) = &self.calc_index {
            let mut guard = calc_index.0.lock().unwrap();
            if guard.is_none() {
                *guard = Some(calc_index.build(self, &self.formula_cells()));
            }
            guard.as_ref().unwrap().calculate_only(self, cells)
        } else {
            ibis_calc::recalc::DependencyIndex::build(self, &self.formula_cells()).calculate_only(self, cells)
        }
    }

    /// Calculate again what depends on `changed` (cells of this book).
    pub fn recalculate(&self, changed: &[CellId]) -> Vec<Recalculated> {
        if let Some(calc_index) = &self.calc_index {
            let mut guard = calc_index.0.lock().unwrap();
            if guard.is_none() {
                *guard = Some(calc_index.build(self, &self.formula_cells()));
            }
            guard.as_ref().unwrap().recalculate(self, changed)
        } else {
            ibis_calc::recalc::recalculate(self, changed, &self.formula_cells())
        }
    }
}

impl BookView<'_> {
    fn table_info(&self, sheet: usize, t: &crate::sheet::SheetTable) -> ibis_calc::eval::TableInfo {
        ibis_calc::eval::TableInfo {
            sheet,
            first_row: t.range.first_row,
            first_column: t.range.first_column,
            last_row: t.range.last_row,
            last_column: t.range.last_column,
            header_rows: t.header_rows,
            totals_rows: t.totals_rows,
            columns: t.columns.clone(),
        }
    }
}

impl Book for BookView<'_> {
    fn sheet_count(&self) -> usize {
        self.real_sheets
    }

    fn external_sheet(&self, book: &str, sheet: &str) -> Option<usize> {
        let raw = if let Some(p) = book.find('[') && let Some(e) = book[p + 1..].find(']') {
            &book[p + 1..p + 1 + e]
        } else {
            book
        };
        (self.real_sheets..self.sheets.len()).find(|&i| {
            self.sheets[i].external.is_some_and(|(b, s)| {
                (b.eq_ignore_ascii_case(book) || b.eq_ignore_ascii_case(raw) || book.ends_with(b) || b.ends_with(book)) && s.name.eq_ignore_ascii_case(sheet)
            })
        })
    }

    fn external_sheets(&self, book: &str, first: &str, last: &str) -> Option<(usize, usize)> {
        let f = self.external_sheet(book, first)?;
        let l = self.external_sheet(book, last)?;
        let min = f.min(l);
        let max = f.max(l);
        let raw = if let Some(p) = book.find('[') && let Some(e) = book[p + 1..].find(']') {
            &book[p + 1..p + 1 + e]
        } else {
            book
        };
        for i in min..=max {
            if !self.sheets.get(i).and_then(|s| s.external).is_some_and(|(b, _)| b.eq_ignore_ascii_case(book) || b.eq_ignore_ascii_case(raw) || book.ends_with(b) || b.ends_with(book)) {
                return None;
            }
        }
        Some((min, max))
    }

    fn table(&self, name: &str) -> Option<ibis_calc::eval::TableInfo> {
        self.sheets.iter().enumerate().find_map(|(i, s)| {
            let t = s.sheet?.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name))?;
            Some(self.table_info(i, t))
        })
    }

    fn table_at(&self, sheet: usize, row: u32, column: u32) -> Option<ibis_calc::eval::TableInfo> {
        let s = self.sheets.get(sheet)?.sheet?;
        let t = s.tables.iter().find(|t| {
            let r = &t.range;
            row >= r.first_row && row <= r.last_row && column >= r.first_column && column <= r.last_column
        })?;
        Some(self.table_info(sheet, t))
    }

    fn sheet_name(&self, sheet: usize) -> &str {
        self.sheets.get(sheet).map_or("", |s| s.name.as_str())
    }

    fn value(&self, sheet: usize, row: u32, column: u32) -> CellValue {
        if let Some((_, e)) = self.sheets.get(sheet).and_then(|s| s.external) {
            return e.cells.get(&(row, column)).cloned().unwrap_or_default();
        }
        self.sheets.get(sheet).and_then(|s| s.sheet).and_then(|s| s.cell(row, column)).map(|c| c.value.clone()).unwrap_or_default()
    }

    fn formula(&self, sheet: usize, row: u32, column: u32) -> Option<&str> {
        self.sheets.get(sheet)?.sheet?.cell(row, column)?.formula.as_deref()
    }

    fn cell_info(&self, sheet: usize, row: u32, column: u32) -> Option<ibis_calc::eval::CellInfo> {
        let sheet = self.sheets.get(sheet)?.sheet?;
        let style = sheet.cell(row, column).and_then(|c| sheet.styles.get(c.style as usize)).cloned().unwrap_or_default();
        let prefix = match style.align {
            crate::sheet::CellAlign::Right => Some('"'),
            crate::sheet::CellAlign::Center | crate::sheet::CellAlign::CenterAcross => Some('^'),
            crate::sheet::CellAlign::Fill => Some('\\'),
            _ => Some('\''),
        };
        // A column's width in characters of the default font (the reader gives a column of n characters about 6 n points).
        let width_emu = sheet.columns.get(column as usize).copied().filter(|w| *w > 0).unwrap_or(sheet.default_column);
        Some(ibis_calc::eval::CellInfo { number_format: style.number_format.clone(), locked: !style.unlocked, prefix, column_width: width_emu as f64 / 12_700.0 / 6.0 })
    }

    fn for_each_cell(&self, sheet: usize, area: (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue)) {
        if let Some((_, e)) = self.sheets.get(sheet).and_then(|s| s.external) {
            let (r0, c0, r1, c1) = area;
            for (&(r, c), v) in e.cells.range((r0, 0)..=(r1, u32::MAX)) {
                if c >= c0 && c <= c1 && !v.is_empty() {
                    f(r, c, v);
                }
            }
            return;
        }
        let Some(sheet) = self.sheets.get(sheet).and_then(|s| s.sheet) else { return };
        let (r0, c0, r1, c1) = area;
        let start = sheet.cells.partition_point(|c| (c.row, c.column) < (r0, c0));
        let mut i = start;
        while i < sheet.cells.len() {
            let cell = &sheet.cells[i];
            if cell.row > r1 {
                break;
            }
            if cell.column < c0 {
                // Jump to this row's first column in the area.
                i = sheet.cells.partition_point(|c| (c.row, c.column) < (cell.row, c0));
                continue;
            }
            if cell.column > c1 {
                i = sheet.cells.partition_point(|c| (c.row, c.column) < (cell.row + 1, c0));
                continue;
            }
            if cell.formula.is_some() || !cell.value.is_empty() {
                f(cell.row, cell.column, &cell.value);
            }
            i += 1;
        }
    }

    fn extent(&self, sheet: usize) -> (u32, u32) {
        let Some(s) = self.sheets.get(sheet) else { return (0, 0) };
        *s.extent.get_or_init(|| match s.sheet {
            None if s.external.is_some() => s.external.map_or((0, 0), |(_, e)| e.cells.iter().filter(|(_, v)| !v.is_empty()).fold((0, 0), |(r, c), (&(row, col), _)| (r.max(row + 1), c.max(col + 1)))),
            Some(sheet) => sheet
                .cells
                .iter()
                .filter(|c| c.formula.is_some() || !c.value.is_empty())
                .fold((0, 0), |(r, c), cell| (r.max(cell.row + 1), c.max(cell.column + 1))),
            None => (0, 0),
        })
    }

    fn defined_name(&self, name: &str, sheet: Option<usize>) -> Option<String> {
        let matches = |d: &&DefinedName| d.name.eq_ignore_ascii_case(name);
        if let Some(s) = sheet
            && let Some(d) = self.names.iter().filter(matches).find(|d| d.sheet == Some(s))
        {
            return Some(d.formula.clone());
        }
        self.names.iter().filter(matches).find(|d| d.sheet.is_none()).map(|d| d.formula.clone())
    }

    fn date1904(&self) -> bool {
        self.date1904
    }

    fn iterate(&self) -> bool {
        self.iterate
    }

    fn max_iterations(&self) -> u32 {
        self.max_iterations
    }

    fn max_change(&self) -> f64 {
        self.max_change
    }

    fn compatibility_version(&self) -> u32 {
        self.compatibility_version
    }

    fn row_hidden(&self, sheet: usize, row: u32) -> bool {
        self.sheets.get(sheet).and_then(|s| s.sheet).is_some_and(|s| s.rows.get(row as usize) == Some(&0))
    }

    fn row_filtered(&self, sheet: usize, row: u32) -> bool {
        self.sheets.get(sheet).and_then(|s| s.sheet).is_some_and(|s| s.filtered_rows.contains(&row))
    }

    fn dynamic(&self, sheet: usize, row: u32, column: u32) -> bool {
        self.sheets.get(sheet).and_then(|s| s.sheet).is_some_and(|s| s.spills.contains_key(&(row, column)))
    }

    fn spill_area(&self, sheet: usize, row: u32, column: u32) -> Option<(u32, u32, u32, u32)> {
        let s = self.sheets.get(sheet)?.sheet?;
        let r = s.spills.get(&(row, column))?;
        let blocked = s.cell(row, column).is_some_and(|c| c.value == CellValue::Error(crate::sheet::CellError::Spill));
        (!blocked).then_some((r.first_row, r.first_column, r.last_row, r.last_column))
    }

    fn phonetic(&self, sheet: usize, row: u32, column: u32) -> Option<&str> {
        self.sheets.get(sheet)?.sheet?.cell(row, column)?.phonetic.as_deref()
    }

    fn pivot_data(
        &self,
        sheet: usize,
        row: u32,
        column: u32,
        data_field: &str,
        fields: &[(&str, &str)],
    ) -> Option<CellValue> {
        let sheet_obj = self.sheets.get(sheet)?.sheet?;
        for pivot in &sheet_obj.pivots {
            if pivot.contains_cell(row, column) {
                return pivot.get_pivot_data(sheet_obj, data_field, fields);
            }
        }
        None
    }
}

/// Ensure the dependency index for recalculation is built.
pub fn ensure_calc_index(presentation: &Presentation) {
    if let Some(wb) = presentation.workbook.as_deref() {
        {
            let guard = wb.calc_index.0.lock().unwrap();
            if guard.is_some() {
                return;
            }
        }
        let view = BookView::new(presentation);
        let formulas = view.formula_cells();
        let index = ibis_calc::recalc::DependencyIndex::build(&view, &formulas);
        let mut guard = wb.calc_index.0.lock().unwrap();
        if guard.is_none() {
            *guard = Some(index);
        }
    }
}

/// Precompute the dependency index on a background thread so the first cell edit
/// does not have to wait for building the index.
pub fn warmup_calc_index_ahead(presentation: Presentation) {
    #[cfg(not(target_family = "wasm"))]
    {
        if presentation.workbook.is_none() {
            return;
        }
        std::thread::Builder::new()
            .name("calc-index-warmup".into())
            .spawn(move || {
                ensure_calc_index(&presentation);
            })
            .ok();
    }
    #[cfg(target_family = "wasm")]
    let _ = presentation;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::{Sheet, SheetCell, Workbook, WorkbookSheet};
    use crate::model::{Presentation, Shape, ShapeKind, Slide, SlideId};
    use std::sync::Arc;

    fn make_cell(row: u32, column: u32, value: CellValue, formula: Option<&str>) -> SheetCell {
        SheetCell {
            row,
            column,
            kind: crate::sheet::CellKind::of(&value),
            value,
            text: String::new(),
            formula: formula.map(str::to_string),
            style: 0,
            phonetic: None,
            runs: None,
        }
    }

    #[test]
    fn test_warmup_calc_index() {
        let mut sheet = Sheet::default();
        sheet.cells.push(make_cell(0, 0, CellValue::Number(10.0), None));
        sheet.cells.push(make_cell(0, 1, CellValue::Number(20.0), Some("A1*2")));

        let slide_id = SlideId(1);
        let shape = Shape::new(
            crate::model::ShapeId(1),
            crate::geometry::EmuRect::default(),
            ShapeKind::Sheet(Arc::new(sheet)),
        );
        let mut slide = Slide::new(slide_id);
        slide.name = Some("Sheet1".to_string());
        slide.shapes.push(shape);

        let mut wb = Workbook::default();
        wb.sheets.push(WorkbookSheet {
            name: "Sheet1".to_string(),
            slide: Some(slide_id),
            hidden: None,
            state: crate::sheet::SheetState::Visible,
            other: false,
        });

        let mut presentation = Presentation::empty(crate::geometry::EmuSize::WIDESCREEN);
        presentation.slides.push(slide);
        presentation.workbook = Some(Arc::new(wb));

        let wb_ref = presentation.workbook.as_ref().unwrap();
        assert!(wb_ref.calc_index.0.lock().unwrap().is_none());

        ensure_calc_index(&presentation);
        assert!(wb_ref.calc_index.0.lock().unwrap().is_some());

        let view = BookView::new(&presentation);
        let formulas = view.formula_cells();
        assert_eq!(formulas.len(), 1);
        let results = view.recalculate(&[(0, 0, 0)]);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].cell, (0, 0, 1));
        assert_eq!(results[0].value, CellValue::Number(20.0));
    }
}
