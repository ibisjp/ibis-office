// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Calculating a formula.
//!
//! A formula is evaluated for one cell of a [`Book`] (a workbook as far as
//! calculation goes). References are resolved through the book; a cell
//! that is to be calculated again (during a recalculation, see
//! `recalc.rs`) is calculated first, through the [`Cells`] the context is
//! given.
//!
//! Formulas are evaluated as Excel evaluates a formula saved without the
//! dynamic array flag: where a function wants one value and gets a range,
//! the range is intersected with the formula's row or column (implicit
//! intersection); an array (a constant, or a function's result) makes the
//! function calculate for each element, and the cell shows the top-left
//! element.

pub mod convert;
pub mod step;
pub mod structured;

use std::rc::Rc;

use crate::formula::{AreaRef, BinaryOp, Expr, MAX_COLUMN, MAX_ROW, SheetRef};
use crate::value::{CellError, CellValue};
pub use convert::{number_to_text, parse_number_text};
pub use step::*;

/// A workbook as the calculation sees it.
/// What `CELL` asks of a cell's format and its column.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CellInfo {
    /// The number format code (`General`, `#,##0.00`).
    pub number_format: String,
    /// Whether the cell is locked (`protection/@locked`; the default).
    pub locked: bool,
    /// The horizontal alignment: `'` left (also General text), `"` right, `^` centre, `\` fill, or none.
    pub prefix: Option<char>,
    /// The width of the cell's column in characters.
    pub column_width: f64,
}

/// A table, as a structured reference sees it: rows and columns are the
/// book's (from 0).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableInfo {
    pub sheet: usize,
    pub first_row: u32,
    pub first_column: u32,
    pub last_row: u32,
    pub last_column: u32,
    pub header_rows: u32,
    pub totals_rows: u32,
    pub columns: Vec<String>,
}

pub trait Book {
    fn sheet_count(&self) -> usize;
    fn sheet_name(&self, sheet: usize) -> &str;
    fn sheet_index(&self, name: &str) -> Option<usize> {
        // (No copies for names in ASCII: every reference of every formula asks.)
        (0..self.sheet_count()).find(|&i| {
            let n = self.sheet_name(i);
            if n.is_ascii() && name.is_ascii() { n.eq_ignore_ascii_case(name) } else { n.to_lowercase() == name.to_lowercase() }
        })
    }
    /// The index a sheet of an external workbook (its name as in a formula's
    /// `[Book.xlsx]Sheet1!`) has in this book, where its cached cells are read
    /// from; past `sheet_count`. `None`: not known.
    fn external_sheet(&self, _book: &str, _sheet: &str) -> Option<usize> {
        None
    }
    /// The range of sheet indices for a 3D reference across an external workbook's sheets
    /// (`[Book.xlsx]Sheet1:Sheet3!A1`).
    fn external_sheets(&self, book: &str, first: &str, last: &str) -> Option<(usize, usize)> {
        let f = self.external_sheet(book, first)?;
        let l = self.external_sheet(book, last)?;
        Some((f.min(l), f.max(l)))
    }
    /// The format of a cell and the width of its column, for `CELL`; `None` where the book does not know.
    fn cell_info(&self, _sheet: usize, _row: u32, _column: u32) -> Option<CellInfo> {
        None
    }
    /// The value a cell holds now.
    fn value(&self, sheet: usize, row: u32, column: u32) -> CellValue;
    /// The formula of a cell, without `=`, as the user sees it.
    fn formula(&self, sheet: usize, row: u32, column: u32) -> Option<&str>;
    /// Call `f` for each cell that exists (has a value or a formula) in the
    /// rectangle, in rows then columns.
    fn for_each_cell(&self, sheet: usize, area: (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue));
    /// The rows and columns in use (the last used row and column, plus one).
    fn extent(&self, sheet: usize) -> (u32, u32);
    /// A defined name's formula: the sheet's own name first, then the
    /// workbook's.
    fn defined_name(&self, name: &str, sheet: Option<usize>) -> Option<String>;
    /// The table of that name (case-insensitive), for structured references.
    fn table(&self, _name: &str) -> Option<TableInfo> {
        None
    }
    /// The table a cell lies in, for a structured reference without a
    /// table's name (`[@Column]`).
    fn table_at(&self, _sheet: usize, _row: u32, _column: u32) -> Option<TableInfo> {
        None
    }
    fn date1904(&self) -> bool {
        false
    }
    /// Whether iterative calculation is enabled for cyclic formulas (default false).
    fn iterate(&self) -> bool {
        false
    }
    /// The maximum number of iterations allowed when calculating cyclic formulas (default 100).
    fn max_iterations(&self) -> u32 {
        100
    }
    /// The maximum change allowed between iterations when calculating cyclic formulas (default 0.001).
    fn max_change(&self) -> f64 {
        0.001
    }
    /// Workbook calculation compatibility version. Older books default to 1.
    fn compatibility_version(&self) -> u32 {
        1
    }
    /// Whether a row is hidden (by hand or by a filter), for SUBTOTAL and
    /// AGGREGATE.
    fn row_hidden(&self, _sheet: usize, _row: u32) -> bool {
        false
    }
    /// Whether a row was excluded by an active filter, separately from manual hiding.
    fn row_filtered(&self, _sheet: usize, _row: u32) -> bool {
        false
    }
    /// Today as a serial number, and now as one with the time, for TODAY
    /// and NOW. `None` where the clock is not known.
    fn now(&self) -> Option<f64> {
        convert::now_serial(self.date1904())
    }
    /// Whether a cell's formula is a dynamic array formula (Excel 365's:
    /// an array result spills into the cells beside it).
    fn dynamic(&self, _sheet: usize, _row: u32, _column: u32) -> bool {
        false
    }
    /// The area a dynamic array formula's result fills now (first row,
    /// first column, last row, last column), if it spills; for `A1#`.
    fn spill_area(&self, _sheet: usize, _row: u32, _column: u32) -> Option<(u32, u32, u32, u32)> {
        None
    }
    /// The phonetic guide (ふりがな, `rPh`) a cell holds, if any.
    fn phonetic(&self, _sheet: usize, _row: u32, _column: u32) -> Option<&str> {
        None
    }
    /// Retrieve data from a pivot table on `sheet` covering `(row, column)`,
    /// for GETPIVOTDATA. Returns `None` if the cell is not in a pivot table
    /// or the requested field/item does not exist (resulting in `#REF!`).
    fn pivot_data(
        &self,
        _sheet: usize,
        _row: u32,
        _column: u32,
        _data_field: &str,
        _fields: &[(&str, &str)],
    ) -> Option<CellValue> {
        None
    }
}

/// Where cell values come from during a calculation: the book's, or,
/// for a cell being calculated again, its new value.
pub trait Cells {
    /// The value of a cell to be calculated again (calculating it now if
    /// need be), or `None` to use the book's.
    fn fresh(&self, book: &dyn Book, sheet: usize, row: u32, column: u32) -> Option<CellValue>;
    /// The area a dynamic array formula fills, if spilling.
    fn spill_area(&self, _sheet: usize, _row: u32, _column: u32) -> Option<(u32, u32, u32, u32)> {
        None
    }
    /// Cells present in the calculation results (formulas or spilled dynamic arrays).
    fn for_each_cell(&self, _sheet: usize, _area: (u32, u32, u32, u32), _f: &mut dyn FnMut(u32, u32, &CellValue)) {}
    /// A range's array made earlier in this calculation, if its values
    /// have not changed since (lets many formulas share one lookup table).
    fn cached_array(&self, _area: &Area) -> Option<Rc<Array>> {
        None
    }
    /// Remember a range's array, if all its cells are final.
    fn store_array(&self, _area: &Area, _array: &Rc<Array>) {}
}

/// Uses the book's values as they are.
pub struct Stored;

impl Cells for Stored {
    fn fresh(&self, _: &dyn Book, _: usize, _: u32, _: u32) -> Option<CellValue> {
        None
    }
}

/// A rectangle of cells on a sheet (or on each sheet of a 3D range).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Area {
    pub sheet: usize,
    /// The last sheet of a 3D range (`sheet` for one sheet).
    pub last_sheet: usize,
    pub first_row: u32,
    pub first_column: u32,
    pub last_row: u32,
    pub last_column: u32,
}

impl Area {
    pub fn cell(sheet: usize, row: u32, column: u32) -> Self {
        Self { sheet, last_sheet: sheet, first_row: row, first_column: column, last_row: row, last_column: column }
    }
    pub fn rows(&self) -> u32 {
        self.last_row - self.first_row + 1
    }
    pub fn columns(&self) -> u32 {
        self.last_column - self.first_column + 1
    }
    pub fn is_cell(&self) -> bool {
        self.first_row == self.last_row && self.first_column == self.last_column && self.sheet == self.last_sheet
    }
    pub fn contains(&self, sheet: usize, row: u32, column: u32) -> bool {
        (self.sheet..=self.last_sheet).contains(&sheet) && (self.first_row..=self.last_row).contains(&row) && (self.first_column..=self.last_column).contains(&column)
    }
}

/// A two-dimensional array of values (none of them arrays or references).
#[derive(Clone, Debug, PartialEq)]
pub struct Array {
    pub rows: usize,
    pub columns: usize,
    pub data: Vec<Value>,
}

impl Array {
    pub fn new(rows: usize, columns: usize, mut data: Vec<Value>) -> Self {
        debug_assert_eq!(rows * columns, data.len());
        data.resize(rows * columns, Value::Error(CellError::NA));
        Self { rows, columns, data }
    }
    pub fn filled(rows: usize, columns: usize, v: Value) -> Self {
        Self { rows, columns, data: vec![v; rows * columns] }
    }
    pub fn get(&self, row: usize, column: usize) -> &Value {
        &self.data[row * self.columns + column]
    }
    /// The element at (row, column) as Excel broadcasts an array to a
    /// bigger size: a single row or column repeats, and beyond the array
    /// is `#N/A`.
    pub fn broadcast(&self, row: usize, column: usize) -> Value {
        let r = if self.rows == 1 { 0 } else { row };
        let c = if self.columns == 1 { 0 } else { column };
        if r < self.rows && c < self.columns { self.get(r, c).clone() } else { Value::Error(CellError::NA) }
    }
}

/// A function made by LAMBDA.
#[derive(Debug, PartialEq)]
pub struct Lambda {
    pub params: Vec<String>,
    /// Trailing parameters explicitly declared with square brackets.
    pub optional: usize,
    pub body: Expr,
    /// The names bound where it was made (LET), captured.
    pub captured: Vec<(String, Value)>,
    /// A function named alone (`BYROW(A1:C3, SUM)`, `_xleta.SUM` in a file):
    /// called with the values it is given, however many.
    pub eta: Option<String>,
}

/// A value during calculation.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    Bool(bool),
    Error(CellError),
    Empty,
    /// An explicit missing argument of a LAMBDA, retained through local bindings.
    Omitted,
    Array(Rc<Array>),
    /// One or more areas (more than one from a union).
    Ref(Rc<Vec<Area>>),
    Lambda(Rc<Lambda>),
}

impl From<CellValue> for Value {
    fn from(v: CellValue) -> Self {
        match v {
            CellValue::Empty => Value::Empty,
            CellValue::Number(n) => Value::Number(n),
            CellValue::Text(t) => Value::Text(t),
            CellValue::Bool(b) => Value::Bool(b),
            CellValue::Error(e) => Value::Error(e),
        }
    }
}

impl From<&CellValue> for Value {
    fn from(v: &CellValue) -> Self {
        v.clone().into()
    }
}

impl From<CellError> for Value {
    fn from(e: CellError) -> Self {
        Value::Error(e)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        num(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Text(s)
    }
}

impl From<Result<f64, CellError>> for Value {
    fn from(r: Result<f64, CellError>) -> Self {
        match r {
            Ok(n) => num(n),
            Err(e) => Value::Error(e),
        }
    }
}

impl From<Result<Value, CellError>> for Value {
    fn from(r: Result<Value, CellError>) -> Self {
        r.unwrap_or_else(Value::Error)
    }
}

/// A number, or `#NUM!` for one that is not finite.
pub fn num(n: f64) -> Value {
    if n.is_finite() { Value::Number(if n == 0.0 { 0.0 } else { n }) } else { Value::Error(CellError::Num) }
}

impl Value {
    pub fn is_error(&self) -> bool {
        matches!(self, Value::Error(_))
    }

    /// A scalar as a cell value (arrays and references are not cell values).
    pub fn to_cell(&self) -> CellValue {
        match self {
            Value::Number(n) => CellValue::Number(*n),
            Value::Text(t) => CellValue::Text(t.clone()),
            Value::Bool(b) => CellValue::Bool(*b),
            Value::Error(e) => CellValue::Error(*e),
            Value::Empty | Value::Omitted => CellValue::Empty,
            Value::Array(_) | Value::Ref(_) => CellValue::Error(CellError::Value),
            Value::Lambda(_) => CellValue::Error(CellError::Calc),
        }
    }
}

pub type Res<T> = Result<T, CellError>;

/// A scalar as a number, where a function wants one: text that reads as a
/// number (`"5"`, `"1,000"`, a date) is one, TRUE is 1, empty is 0.
pub fn to_number(v: &Value, date1904: bool) -> Res<f64> {
    match v {
        Value::Number(n) => Ok(*n),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::Empty | Value::Omitted => Ok(0.0),
        Value::Text(t) => parse_number_text(t, date1904).ok_or(CellError::Value),
        Value::Error(e) => Err(*e),
        Value::Array(a) => to_number(a.data.first().unwrap_or(&Value::Empty), date1904),
        Value::Ref(_) | Value::Lambda(_) => Err(CellError::Value),
    }
}

pub fn to_text(v: &Value) -> Res<String> {
    match v {
        Value::Number(n) => Ok(number_to_text(*n)),
        Value::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.into()),
        Value::Empty | Value::Omitted => Ok(String::new()),
        Value::Text(t) => Ok(t.clone()),
        Value::Error(e) => Err(*e),
        Value::Array(a) => to_text(a.data.first().unwrap_or(&Value::Empty)),
        Value::Ref(_) | Value::Lambda(_) => Err(CellError::Value),
    }
}

pub fn to_bool(v: &Value) -> Res<bool> {
    match v {
        Value::Number(n) => Ok(*n != 0.0),
        Value::Bool(b) => Ok(*b),
        Value::Empty | Value::Omitted => Ok(false),
        Value::Text(t) if t.eq_ignore_ascii_case("TRUE") => Ok(true),
        Value::Text(t) if t.eq_ignore_ascii_case("FALSE") => Ok(false),
        Value::Text(_) => Err(CellError::Value),
        Value::Error(e) => Err(*e),
        Value::Array(a) => to_bool(a.data.first().unwrap_or(&Value::Empty)),
        Value::Ref(_) | Value::Lambda(_) => Err(CellError::Value),
    }
}

/// How Excel orders values when comparing: numbers, then text, then
/// logical values; text without regard to case. Empty is 0 or "" as the
/// other side is.
pub fn compare(a: &Value, b: &Value) -> std::cmp::Ordering {
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Number(_) => 0,
            Value::Text(_) => 1,
            Value::Bool(_) => 2,
            _ => 3,
        }
    }
    let empty_like = |e: &Value, other: &Value| -> Value {
        if !matches!(e, Value::Empty | Value::Omitted) {
            return e.clone();
        }
        match other {
            Value::Text(_) => Value::Text(String::new()),
            // In Excel, empty cells compared against Booleans (or numbers/empty) behave as 0.0,
            // NOT as false. Thus `A1=FALSE` is false (since 0 != FALSE), and `A1<FALSE` is true.
            _ => Value::Number(0.0),
        }
    };
    let (a, b) = (empty_like(a, b), empty_like(b, a));
    match (&a, &b) {
        (Value::Number(x), Value::Number(y)) => compare_numbers(*x, *y),
        (Value::Text(x), Value::Text(y)) => compare_text(x, y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => rank(&a).cmp(&rank(&b)),
    }
}

/// Numbers compared as Excel compares them: 15 significant digits of precision.
pub fn compare_numbers(x: f64, y: f64) -> std::cmp::Ordering {
    if x == y {
        return std::cmp::Ordering::Equal;
    }
    if !x.is_finite() || !y.is_finite() {
        return x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
    }
    let diff = (x - y).abs();
    let scale = x.abs().max(y.abs());
    if scale > 0.0 && diff > 1e-14 * scale {
        return x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
    }
    let rx = convert::round_significant(x, 15);
    let ry = convert::round_significant(y, 15);
    rx.partial_cmp(&ry).unwrap_or(std::cmp::Ordering::Equal)
}

/// Text compared as Excel compares it: without regard to case.
pub fn compare_text(a: &str, b: &str) -> std::cmp::Ordering {
    crate::collation::compare(a, b)
}

/// The value of a formula for a cell: what it shows, as a cell value.
pub fn evaluate_cell(book: &dyn Book, cells: &dyn Cells, sheet: usize, row: u32, column: u32, expr: &Expr) -> CellValue {
    let mut ctx = Ctx::new(book, cells, sheet, row, column);
    let v = ctx.eval(expr);
    ctx.cell_result(v)
}

/// The picture IMAGE(source, [alt_text], [sizing], [height], [width]) asks for: fetched and drawn in the cell by
/// the application (the calculation does not reach the network).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageSpec {
    pub source: String,
    pub alt_text: Option<String>,
    /// 0 fit the cell keeping the proportions, 1 fill the cell, 2 the picture's own size, 3 the height and width given.
    pub sizing: u8,
    /// For sizing 3, in pixels.
    pub height: u32,
    pub width: u32,
}

/// The picture a formula's IMAGE asks for in the cell (`None`: it does not call IMAGE, or it is an error).
pub fn evaluate_image(book: &dyn Book, cells: &dyn Cells, sheet: usize, row: u32, column: u32, expr: &Expr) -> Option<ImageSpec> {
    let mut ctx = Ctx::new(book, cells, sheet, row, column);
    let v = ctx.eval(expr);
    if matches!(v, Value::Error(_)) {
        return None;
    }
    ctx.image
}

/// The context of one formula's evaluation.
pub struct Ctx<'a> {
    pub book: &'a dyn Book,
    pub cells: &'a dyn Cells,
    /// The formula's cell.
    pub sheet: usize,
    pub row: u32,
    pub column: u32,
    /// Names bound by LET and LAMBDA, innermost last.
    pub locals: Vec<(String, Value)>,
    /// Set when a function the engine does not have yet was called: the
    /// result is not to be trusted (the recalculation then keeps the value
    /// Excel last calculated).
    pub unsupported: bool,
    /// What IMAGE was asked to show (the last call), for the picture to be fetched and drawn in the cell.
    pub image: Option<ImageSpec>,
    /// Whether evaluation is within an array argument context.
    pub in_array_context: bool,
    /// Whether this is the root operation of the cell's formula.
    pub is_root: bool,
    /// Calls deep, against runaway recursion (LAMBDA).
    depth: u32,
}

impl<'a> Ctx<'a> {
    pub fn new(book: &'a dyn Book, cells: &'a dyn Cells, sheet: usize, row: u32, column: u32) -> Self {
        Self { book, cells, sheet, row, column, locals: Vec::new(), unsupported: false, image: None, in_array_context: false, is_root: true, depth: 0 }
    }

    pub fn date1904(&self) -> bool {
        self.book.date1904()
    }

    /// The area a dynamic array formula fills, checking calculated cells then the book.
    pub fn spill_area(&self, sheet: usize, row: u32, column: u32) -> Option<(u32, u32, u32, u32)> {
        if let Some(area) = self.cells.spill_area(sheet, row, column) {
            return Some(area);
        }
        let _ = self.cells.fresh(self.book, sheet, row, column);
        self.cells.spill_area(sheet, row, column).or_else(|| self.book.spill_area(sheet, row, column))
    }

    /// The value of a cell, calculated again first if it is to be.
    pub fn cell_value(&self, sheet: usize, row: u32, column: u32) -> Value {
        match self.cells.fresh(self.book, sheet, row, column) {
            Some(v) => v.into(),
            None => self.book.value(sheet, row, column).into(),
        }
    }

    /// Call `f` for each existing cell of `area` with its value.
    pub fn for_each(&self, area: &Area, f: &mut dyn FnMut(usize, u32, u32, Value)) {
        for sheet in area.sheet..=area.last_sheet {
            // Whole columns reach only as far as the sheet is used.
            let (rows, columns) = self.book.extent(sheet);
            let last_row = if area.last_row == crate::formula::MAX_ROW { area.last_row.min(rows.saturating_sub(1)) } else { area.last_row };
            let last_column = if area.last_column == crate::formula::MAX_COLUMN { area.last_column.min(columns.saturating_sub(1)) } else { area.last_column };
            if (rows == 0 || columns == 0 || area.first_row > last_row || area.first_column > last_column)
                && (area.last_row == crate::formula::MAX_ROW && area.last_column == crate::formula::MAX_COLUMN)
            {
                continue;
            }
            let mut found: std::collections::BTreeSet<(u32, u32)> = std::collections::BTreeSet::new();
            self.book.for_each_cell(sheet, (area.first_row, area.first_column, last_row, last_column), &mut |r, c, _| {
                found.insert((r, c));
            });
            self.cells.for_each_cell(sheet, (area.first_row, area.first_column, last_row, last_column), &mut |r, c, _| {
                found.insert((r, c));
            });
            for (r, c) in found {
                let v = self.cell_value(sheet, r, c);
                f(sheet, r, c, v);
            }
        }
    }

    /// The final value of a formula, for its cell.
    pub fn cell_result(&mut self, v: Value) -> CellValue {
        match v {
            Value::Ref(areas) => {
                let v = if self.book.dynamic(self.sheet,self.row,self.column) {
                    match self.ref_to_array(&areas) {
                        Value::Array(a) => a.data.first().cloned().unwrap_or(Value::Empty),
                        other => other,
                    }
                } else { self.deref_implicit(&areas) };
                match v {
                    Value::Empty | Value::Omitted => CellValue::Number(0.0),
                    other => other.to_cell(),
                }
            }
            Value::Array(a) => match a.data.first() {
                Some(Value::Empty | Value::Omitted) | None => CellValue::Number(0.0),
                Some(v) => v.to_cell(),
            },
            Value::Empty | Value::Omitted => CellValue::Number(0.0),
            Value::Lambda(_) => CellValue::Error(CellError::Calc),
            other => other.to_cell(),
        }
    }

    /// The cells a dynamic array formula's result fills, if it is an array
    /// of more than one value (a range is its cells' values): rows,
    /// columns and the values, row by row (an empty one shows 0).
    pub fn spill_result(&mut self, v: &Value) -> Option<(usize, usize, Vec<CellValue>)> {
        let a = match v {
            Value::Array(a) => a.clone(),
            Value::Ref(r) => match self.ref_to_array(r) {
                Value::Array(a) => a,
                _ => return None,
            },
            _ => return None,
        };
        if a.data.len() <= 1 {
            return None;
        }
        let values = a
            .data
            .iter()
            .map(|x| match x {
                Value::Empty | Value::Omitted => CellValue::Number(0.0),
                Value::Ref(_) | Value::Array(_) | Value::Lambda(_) => CellValue::Error(CellError::Value),
                other => other.to_cell(),
            })
            .collect();
        Some((a.rows, a.columns, values))
    }

    /// A reference's value where one value is wanted: a single cell's
    /// value, or the cell of a range in the formula's row (for a column)
    /// or column (for a row).
    pub fn deref_implicit(&self, areas: &[Area]) -> Value {
        let [area] = areas else { return Value::Error(CellError::Value) };
        if area.sheet != area.last_sheet {
            return Value::Error(CellError::Value);
        }
        if area.is_cell() {
            return self.cell_value(area.sheet, area.first_row, area.first_column);
        }
        let row = if area.rows() == 1 {
            Some(area.first_row)
        } else if (area.first_row..=area.last_row).contains(&self.row) {
            Some(self.row)
        } else {
            None
        };
        let column = if area.columns() == 1 {
            Some(area.first_column)
        } else if (area.first_column..=area.last_column).contains(&self.column) {
            Some(self.column)
        } else {
            None
        };
        match (row, column) {
            (Some(r), Some(c)) => self.cell_value(area.sheet, r, c),
            _ => Value::Error(CellError::Value),
        }
    }

    /// A reference's cells as an array.
    pub fn ref_to_array(&self, areas: &[Area]) -> Value {
        let [area] = areas else { return Value::Error(CellError::Value) };
        if area.sheet != area.last_sheet {
            return Value::Error(CellError::Value);
        }
        // Whole columns: only as far as the sheet is used.
        let (used_rows, used_columns) = self.book.extent(area.sheet);
        let last_row = if area.last_row == MAX_ROW { area.last_row.min(used_rows.max(area.first_row + 1) - 1) } else { area.last_row };
        let last_column = if area.last_column == MAX_COLUMN { area.last_column.min(used_columns.max(area.first_column + 1) - 1) } else { area.last_column };
        let rows = (last_row - area.first_row + 1) as usize;
        let columns = (last_column - area.first_column + 1) as usize;
        if rows.saturating_mul(columns) > 50_000_000 {
            return Value::Error(CellError::Num);
        }
        if let Some(a) = self.cells.cached_array(area) {
            return Value::Array(a);
        }
        let mut data = vec![Value::Empty; rows * columns];
        let clipped = Area { last_row, last_column, ..*area };
        self.for_each(&clipped, &mut |_, r, c, v| {
            data[(r - area.first_row) as usize * columns + (c - area.first_column) as usize] = v;
        });
        let array = Rc::new(Array::new(rows, columns, data));
        if rows * columns >= 64 {
            self.cells.store_array(area, &array);
        }
        Value::Array(array)
    }

    // ----- Evaluating --------------------------------------------------

    /// Evaluate an expression; references stay references.
    pub fn eval(&mut self, e: &Expr) -> Value {
        let was_root = self.is_root;
        self.is_root = false;
        match e {
            Expr::Number(n) => Value::Number(*n),
            Expr::Text(s) => Value::Text(s.clone()),
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Error(e) => Value::Error(*e),
            Expr::Missing => Value::Empty,
            Expr::Array(rows) => {
                let columns = rows.first().map_or(0, |r| r.len());
                let data = rows.iter().flatten().map(|x| self.eval(x)).collect();
                Value::Array(Rc::new(Array::new(rows.len(), columns, data)))
            }
            Expr::Ref(r) => self.resolve_ref(r),
            Expr::Name { sheet, name } => self.resolve_name(sheet.as_ref(), name),
            Expr::Structured { table, spec } => self.resolve_structured(table, spec),
            Expr::Call { name, args } => self.call(name, args),
            Expr::Invoke { callee, args } => {
                let f = self.eval(callee);
                self.invoke(&f, args)
            }
            Expr::Neg(x) => {
                self.is_root = was_root;
                let v = self.eval_scalar_or_array(x);
                self.map1(v, |ctx, v| match to_number(&v, ctx.date1904()) {
                    Ok(n) => num(-n),
                    Err(e) => Value::Error(e),
                })
            }
            Expr::Plus(x) => {
                self.is_root = was_root;
                self.eval(x)
            }
            Expr::Percent(x) => {
                let v = self.eval_scalar_or_array(x);
                self.map1(v, |ctx, v| match to_number(&v, ctx.date1904()) {
                    Ok(n) => num(n / 100.0),
                    Err(e) => Value::Error(e),
                })
            }
            Expr::Binary(op, a, b) => {
                let a = self.eval_scalar_or_array(a);
                let b = self.eval_scalar_or_array(b);
                let op = *op;
                self.map2(a, b, move |ctx, x, y| binary_ext(op, &x, &y, ctx.date1904(), was_root))
            }
            Expr::Range(a, b) => {
                let (a, b) = (self.eval(a), self.eval(b));
                match (a, b) {
                    (Value::Ref(x), Value::Ref(y)) if x.len() == 1 && y.len() == 1 && x[0].sheet == y[0].sheet => {
                        let (x, y) = (x[0], y[0]);
                        Value::Ref(Rc::new(vec![Area {
                            sheet: x.sheet,
                            last_sheet: x.last_sheet.max(y.last_sheet),
                            first_row: x.first_row.min(y.first_row),
                            first_column: x.first_column.min(y.first_column),
                            last_row: x.last_row.max(y.last_row),
                            last_column: x.last_column.max(y.last_column),
                        }]))
                    }
                    (Value::Error(e), _) | (_, Value::Error(e)) => Value::Error(e),
                    _ => Value::Error(CellError::Value),
                }
            }
            Expr::Intersect(a, b) => {
                let (a, b) = (self.eval(a), self.eval(b));
                match (a, b) {
                    (Value::Ref(x), Value::Ref(y)) => {
                        let mut out = Vec::new();
                        for p in x.iter() {
                            for q in y.iter() {
                                if p.sheet != q.sheet {
                                    continue;
                                }
                                let r0 = p.first_row.max(q.first_row);
                                let r1 = p.last_row.min(q.last_row);
                                let c0 = p.first_column.max(q.first_column);
                                let c1 = p.last_column.min(q.last_column);
                                if r0 <= r1 && c0 <= c1 {
                                    out.push(Area { sheet: p.sheet, last_sheet: p.sheet, first_row: r0, first_column: c0, last_row: r1, last_column: c1 });
                                }
                            }
                        }
                        if out.is_empty() { Value::Error(CellError::Null) } else { Value::Ref(Rc::new(out)) }
                    }
                    (Value::Error(e), _) | (_, Value::Error(e)) => Value::Error(e),
                    _ => Value::Error(CellError::Value),
                }
            }
            Expr::Union(items) => {
                let mut out = Vec::new();
                for item in items {
                    match self.eval(item) {
                        Value::Ref(r) => out.extend(r.iter().copied()),
                        Value::Error(e) => return Value::Error(e),
                        _ => return Value::Error(CellError::Value),
                    }
                }
                Value::Ref(Rc::new(out))
            }
            // `A1#`: the area the formula in A1 fills.
            Expr::Spill(inner) => match self.eval(inner) {
                Value::Ref(r) if r.len() == 1 && r[0].first_row == r[0].last_row && r[0].first_column == r[0].last_column => {
                    let a = r[0];
                    if let Value::Error(e) = self.cell_value(a.sheet, a.first_row, a.first_column) {
                        return Value::Error(e);
                    }
                    match self.spill_area(a.sheet, a.first_row, a.first_column) {
                        Some((r0, c0, r1, c1)) => Value::Ref(Rc::new(vec![Area { sheet: a.sheet, last_sheet: a.sheet, first_row: r0, first_column: c0, last_row: r1, last_column: c1 }])),
                        None => Value::Error(CellError::Ref),
                    }
                }
                Value::Error(e) => Value::Error(e),
                _ => Value::Error(CellError::Ref),
            },
            Expr::Implicit(x) => match self.eval(x) {
                Value::Ref(r) => self.deref_implicit(&r),
                Value::Array(a) => a.data.first().cloned().unwrap_or(Value::Empty),
                v => v,
            },
        }
    }

    /// Evaluate a value or array. Dynamic formulas retain referenced arrays;
    /// legacy formulas intersect references. Explicit @ intersects in `eval`.
    pub fn eval_scalar_or_array(&mut self, e: &Expr) -> Value {
        match self.eval(e) {
            Value::Ref(r) if self.in_array_context || self.book.dynamic(self.sheet, self.row, self.column) => self.ref_to_array(&r),
            Value::Ref(r) => self.deref_implicit(&r),
            Value::Omitted => Value::Empty,
            v => v,
        }
    }

    /// Evaluate where an array is wanted: a reference becomes the array
    /// of its cells.
    pub fn eval_array(&mut self, e: &Expr) -> Value {
        let prev = self.in_array_context;
        self.in_array_context = true;
        let v = match self.eval(e) {
            Value::Ref(r) => self.ref_to_array(&r),
            Value::Omitted => Value::Empty,
            v => v,
        };
        self.in_array_context = prev;
        v
    }

    /// Evaluate where exactly one value is wanted (an array gives its first
    /// element).
    pub fn eval_scalar(&mut self, e: &Expr) -> Value {
        match self.eval_scalar_or_array(e) {
            Value::Array(a) => a.data.first().cloned().unwrap_or(Value::Empty),
            v => v,
        }
    }

    /// Apply `f` to a value, or to each element of an array.
    pub fn map1(&mut self, v: Value, mut f: impl FnMut(&mut Self, Value) -> Value) -> Value {
        match v {
            Value::Array(a) => {
                let data = a.data.iter().map(|x| f(self, x.clone())).collect();
                Value::Array(Rc::new(Array::new(a.rows, a.columns, data)))
            }
            v => f(self, v),
        }
    }

    /// Apply `f` to two values, element by element for arrays (broadcast).
    pub fn map2(&mut self, a: Value, b: Value, mut f: impl FnMut(&mut Self, Value, Value) -> Value) -> Value {
        match (&a, &b) {
            (Value::Array(_), _) | (_, Value::Array(_)) => {
                let as_array = |v: &Value| match v {
                    Value::Array(x) => x.clone(),
                    other => Rc::new(Array::new(1, 1, vec![other.clone()])),
                };
                let (x, y) = (as_array(&a), as_array(&b));
                let rows = x.rows.max(y.rows);
                let columns = x.columns.max(y.columns);
                let mut data = Vec::with_capacity(rows * columns);
                for r in 0..rows {
                    for c in 0..columns {
                        let (p, q) = (x.broadcast(r, c), y.broadcast(r, c));
                        data.push(f(self, p, q));
                    }
                }
                Value::Array(Rc::new(Array::new(rows, columns, data)))
            }
            _ => f(self, a, b),
        }
    }

    /// Apply `f` to the values of several arguments, element by element
    /// where any is an array (each argument evaluated as a value).
    pub fn map_args(&mut self, args: &[Expr], mut f: impl FnMut(&mut Self, &[Value]) -> Value) -> Value {
        let values: Vec<Value> = args.iter().map(|a| self.eval_scalar_or_array(a)).collect();
        self.map_values(values, &mut f)
    }

    pub fn map_values(&mut self, values: Vec<Value>, f: &mut dyn FnMut(&mut Self, &[Value]) -> Value) -> Value {
        let (mut rows, mut columns) = (0, 0);
        for v in &values {
            if let Value::Array(a) = v {
                rows = rows.max(a.rows);
                columns = columns.max(a.columns);
            }
        }
        if rows == 0 {
            return f(self, &values);
        }
        let mut data = Vec::with_capacity(rows * columns);
        let mut scratch = values.clone();
        for r in 0..rows {
            for c in 0..columns {
                for (i, v) in values.iter().enumerate() {
                    scratch[i] = match v {
                        Value::Array(a) => a.broadcast(r, c),
                        other => other.clone(),
                    };
                }
                data.push(f(self, &scratch));
            }
        }
        Value::Array(Rc::new(Array::new(rows, columns, data)))
    }

    fn resolve_sheet(&self, sheet: Option<&SheetRef>) -> Result<(usize, usize), CellError> {
        match sheet {
            None => Ok((self.sheet, self.sheet)),
            Some(s) if s.book.is_some() => {
                let book = s.book.as_deref().unwrap_or("");
                let (first, last) = match &s.last {
                    Some(l) if !l.eq_ignore_ascii_case(&s.first) => {
                        self.book.external_sheets(book, &s.first, l).ok_or(CellError::Ref)?
                    }
                    _ => {
                        let idx = self.book.external_sheet(book, &s.first).ok_or(CellError::Ref)?;
                        (idx, idx)
                    }
                };
                Ok((first, last))
            }
            Some(s) => {
                let first = self.book.sheet_index(&s.first).ok_or(CellError::Ref)?;
                let last = match &s.last {
                    Some(l) => self.book.sheet_index(l).ok_or(CellError::Ref)?,
                    None => first,
                };
                Ok((first.min(last), first.max(last)))
            }
        }
    }

    pub fn resolve_ref(&self, r: &AreaRef) -> Value {
        match self.resolve_sheet(r.sheet.as_ref()) {
            Ok((sheet, last_sheet)) => Value::Ref(Rc::new(vec![Area {
                sheet,
                last_sheet,
                first_row: r.start.row,
                first_column: r.start.column,
                last_row: r.end.row,
                last_column: r.end.column,
            }])),
            Err(e) => Value::Error(e),
        }
    }

    fn resolve_structured(&self, table: &str, spec: &str) -> Value {
        let info = if table.is_empty() { self.book.table_at(self.sheet, self.row, self.column) } else { self.book.table(table) };
        let Some(info) = info else {
            return Value::Error(if table.is_empty() { CellError::Ref } else { CellError::Name });
        };
        match structured::area(&info, spec, self.row) {
            Ok((r0, c0, r1, c1)) => Value::Ref(Rc::new(vec![Area { sheet: info.sheet, last_sheet: info.sheet, first_row: r0, first_column: c0, last_row: r1, last_column: c1 }])),
            Err(e) => Value::Error(e),
        }
    }

    fn resolve_name(&mut self, sheet: Option<&SheetRef>, name: &str) -> Value {
        if sheet.is_none()
            && let Some((_, v)) = self.locals.iter().rev().find(|(n, _)| n.eq_ignore_ascii_case(name))
        {
            return v.clone();
        }
        let scope = match sheet {
            Some(s) => match self.book.sheet_index(&s.first) {
                Some(i) => Some(i),
                None => return Value::Error(CellError::Ref),
            },
            None => Some(self.sheet),
        };
        let Some(formula) = self.book.defined_name(name, scope) else {
            // A function's name alone is that function, to give to BYROW, GROUPBY and the like.
            let bare = name.trim_start_matches("_xleta.").to_uppercase();
            if sheet.is_none() && crate::functions::lookup(&bare).is_some() {
                return Value::Lambda(Rc::new(Lambda { params: Vec::new(), optional: 0, body: Expr::Missing, captured: Vec::new(), eta: Some(bare) }));
            }
            return Value::Error(CellError::Name);
        };
        if self.depth > 64 {
            return Value::Error(CellError::Num);
        }
        let Ok(expr) = crate::formula::parse(formula.trim_start_matches('=')) else {
            return Value::Error(CellError::Name);
        };
        self.depth += 1;
        // A name's relative references are relative to A1.
        let v = self.eval(&expr);
        self.depth -= 1;
        v
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> Value {
        // A name bound to a LAMBDA by LET.
        if let Some((_, v)) = self.locals.iter().rev().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            let f = v.clone();
            return self.invoke(&f, args);
        }
        if let Some(info) = crate::functions::table::function_info(name) {
            if args.len() < info.min_args as usize || args.len() > info.max_args.max(info.min_args) as usize && info.max_args < 255 {
                return Value::Error(CellError::Value);
            }
        }
        match crate::functions::lookup(name) {
            Some(f) => {
                // The Analysis ToolPak's functions take no logical values.
                if crate::functions::refuses_logical(name) {
                    // (ACCRINT's last argument is a logical value.)
                    let logical_last = name == "ACCRINT";
                    for (i, a) in args.iter().enumerate() {
                        if logical_last && i == 7 {
                            continue;
                        }
                        if let Value::Bool(_) = self.eval_scalar_or_array(a) {
                            return Value::Error(CellError::Value);
                        }
                    }
                }
                self.depth += 1;
                let v = if self.depth > 200 { Value::Error(CellError::Num) } else { f(self, args) };
                self.depth -= 1;
                v
            }
            None => {
                // A defined name holding a LAMBDA, or a function we lack.
                if let Some(formula) = self.book.defined_name(name, Some(self.sheet))
                    && let Ok(expr) = crate::formula::parse(formula.trim_start_matches('='))
                {
                    let f = self.eval(&expr);
                    return self.invoke(&f, args);
                }
                if crate::functions::table::function_info(name).is_some() {
                    self.unsupported = true;
                }
                Value::Error(CellError::Name)
            }
        }
    }

    /// Call a LAMBDA with arguments.
    pub fn invoke(&mut self, f: &Value, args: &[Expr]) -> Value {
        let values: Vec<Value> = args.iter().map(|a| if matches!(a, Expr::Missing) { Value::Omitted } else { self.eval(a) }).collect();
        self.invoke_values(f, values)
    }

    pub fn invoke_values(&mut self, f: &Value, values: Vec<Value>) -> Value {
        let Value::Lambda(l) = f else {
            return match f {
                Value::Error(e) => Value::Error(*e),
                _ => Value::Error(CellError::Value),
            };
        };
        if let Some(name) = &l.eta {
            let names: Vec<String> = (0..values.len()).map(|i| format!("_eta{i}")).collect();
            let saved = std::mem::take(&mut self.locals);
            self.locals = names.iter().cloned().zip(values).collect();
            let args: Vec<Expr> = names.into_iter().map(|n| Expr::Name { sheet: None, name: n }).collect();
            self.depth += 1;
            let v = self.call(name, &args);
            self.depth -= 1;
            self.locals = saved;
            return v;
        }
        if values.len() > l.params.len() || values.len() < l.params.len() - l.optional {
            return Value::Error(CellError::Value);
        }
        if self.depth > 200 {
            return Value::Error(CellError::Num);
        }
        let saved = std::mem::replace(&mut self.locals, l.captured.clone());
        for (i, p) in l.params.iter().enumerate() {
            self.locals.push((p.clone(), values.get(i).cloned().unwrap_or(Value::Omitted)));
        }
        self.depth += 1;
        let v = self.eval(&l.body);
        self.depth -= 1;
        self.locals = saved;
        v
    }
}

/// A binary operator on two scalars.
pub fn binary(op: BinaryOp, a: &Value, b: &Value, date1904: bool) -> Value {
    binary_ext(op, a, b, date1904, false)
}

/// A binary operator on two scalars, with `is_root` indicating whether this is the formula's root operation.
pub fn binary_ext(op: BinaryOp, a: &Value, b: &Value, date1904: bool, is_root: bool) -> Value {
    use BinaryOp::*;
    match op {
        Add | Sub | Mul | Div | Pow => {
            let x = match to_number(a, date1904) {
                Ok(x) => x,
                Err(e) => return Value::Error(e),
            };
            let y = match to_number(b, date1904) {
                Ok(y) => y,
                Err(e) => return Value::Error(e),
            };
            match op {
                Add => {
                    let sum = x + y;
                    if is_root && sum != 0.0 && sum.abs() <= 1e-15 * x.abs().max(y.abs()) {
                        num(0.0)
                    } else {
                        num(sum)
                    }
                }
                Sub => {
                    let diff = x - y;
                    if is_root && diff != 0.0 && diff.abs() <= 1e-15 * x.abs().max(y.abs()) {
                        num(0.0)
                    } else {
                        num(diff)
                    }
                }
                Mul => num(x * y),
                Div => {
                    if y == 0.0 {
                        Value::Error(CellError::Div0)
                    } else {
                        num(x / y)
                    }
                }
                Pow => power(x, y),
                _ => unreachable!(),
            }
        }
        Concat => match (to_text(a), to_text(b)) {
            (Ok(x), Ok(y)) => Value::Text(x + &y),
            (Err(e), _) | (_, Err(e)) => Value::Error(e),
        },
        Eq | Ne | Lt | Le | Gt | Ge => {
            if let Value::Error(e) = a {
                return Value::Error(*e);
            }
            if let Value::Error(e) = b {
                return Value::Error(*e);
            }
            let ord = compare(a, b);
            use std::cmp::Ordering::*;
            Value::Bool(match op {
                Eq => ord == Equal,
                Ne => ord != Equal,
                Lt => ord == Less,
                Le => ord != Greater,
                Gt => ord == Greater,
                Ge => ord != Less,
                _ => unreachable!(),
            })
        }
    }
}

/// `x ^ y` as Excel: 0^0 and 0^negative are errors, a negative base with
/// a fractional exponent is `#NUM!` unless the root is odd.
pub fn power(x: f64, y: f64) -> Value {
    if x == 0.0 && y == 0.0 {
        return Value::Error(CellError::Num);
    }
    if x == 0.0 && y < 0.0 {
        return Value::Error(CellError::Div0);
    }
    if x == 0.0 {
        return num(0.0);
    }
    if x < 0.0 && y.fract() != 0.0 {
        // An odd root of a negative number: (-8)^(1/3) = -2.
        let inv = 1.0 / y;
        if (inv.round() - inv).abs() < 1e-10 && (inv.round() as i64) % 2 != 0 {
            let p = power(-x, y);
            return match p {
                Value::Number(n) => num(-n),
                _ => p,
            };
        }
        return Value::Error(CellError::Num);
    }
    if x < 0.0 {
        let sign = if (y as i64) % 2 != 0 { -1.0 } else { 1.0 };
        let p = power(-x, y);
        return match p {
            Value::Number(n) => num(sign * n),
            _ => p,
        };
    }
    if y.fract() == 0.0 && y.abs() <= 10_000.0 {
        num(pow_int(x, y as i64))
    } else {
        num((y * x.ln()).exp())
    }
}

fn pow_int(mut base: f64, mut exp: i64) -> f64 {
    if exp == 0 {
        return 1.0;
    }
    if exp < 0 {
        return 1.0 / pow_int(base, -exp);
    }
    let mut res = 1.0;
    while exp > 0 {
        if exp & 1 == 1 {
            res *= base;
        }
        base *= base;
        exp >>= 1;
    }
    res
}

