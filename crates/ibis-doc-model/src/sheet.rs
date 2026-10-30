// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Worksheets: a grid of cells with values already formatted for display.
//!
//! A worksheet is read into one [`Sheet`] shape covering the used cells;
//! its charts and pictures are ordinary shapes on the same slide.

use std::sync::Arc;

pub use ibis_calc::{CellError, CellValue};

use crate::drawing::{Color, Fill, GradientPath, GradientStop, Rgb};
use crate::flow::VerticalPosition;
use crate::geometry::Emu;
use crate::model::SlideId;

/// Horizontal alignment of a cell's text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CellAlign {
    /// Numbers to the right, text to the left, booleans and errors centred.
    #[default]
    General,
    Left,
    Center,
    Right,
    Fill,
    Justify,
    /// Centred across the empty cells to its right that share the style.
    CenterAcross,
    /// Spread over the cell's width, the characters evenly apart
    /// (`distributed`, Japanese 均等割り付け).
    Distributed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CellVerticalAlign {
    Top,
    Center,
    #[default]
    Bottom,
    Justify,
    Distributed,
}

/// How a border line is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BorderLine {
    #[default]
    Thin,
    Hair,
    Medium,
    Thick,
    Double,
    Dotted,
    Dashed,
    DashDot,
    DashDotDot,
    MediumDashed,
    MediumDashDot,
    MediumDashDotDot,
    SlantDashDot,
}

impl BorderLine {
    /// All thirteen non-empty SpreadsheetML border styles.
    pub const ALL: [Self; 13] = [Self::Thin, Self::Hair, Self::Medium, Self::Thick, Self::Double,
        Self::Dotted, Self::Dashed, Self::DashDot, Self::DashDotDot, Self::MediumDashed,
        Self::MediumDashDot, Self::MediumDashDotDot, Self::SlantDashDot];

    pub fn ooxml(self) -> &'static str {
        match self {
            Self::Thin => "thin", Self::Hair => "hair", Self::Medium => "medium", Self::Thick => "thick",
            Self::Double => "double", Self::Dotted => "dotted", Self::Dashed => "dashed",
            Self::DashDot => "dashDot", Self::DashDotDot => "dashDotDot", Self::MediumDashed => "mediumDashed",
            Self::MediumDashDot => "mediumDashDot", Self::MediumDashDotDot => "mediumDashDotDot", Self::SlantDashDot => "slantDashDot",
        }
    }

    pub fn from_ooxml(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.ooxml() == value)
    }

    /// The line's width in pixels at 100 %.
    pub fn width(self) -> f32 {
        match self {
            BorderLine::Hair => 0.5,
            BorderLine::Thin | BorderLine::Dotted | BorderLine::Dashed | BorderLine::DashDot | BorderLine::DashDotDot => 1.0,
            BorderLine::Medium | BorderLine::MediumDashed | BorderLine::MediumDashDot | BorderLine::MediumDashDotDot | BorderLine::SlantDashDot => 2.0,
            BorderLine::Thick | BorderLine::Double => 3.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Border {
    pub line: BorderLine,
    pub color: Rgb,
}

/// SpreadsheetML font underline variants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CellUnderline {
    #[default]
    None,
    Single,
    Double,
    SingleAccounting,
    DoubleAccounting,
}
impl CellUnderline {
    pub const ALL: [Self; 5] = [Self::None, Self::Single, Self::Double, Self::SingleAccounting, Self::DoubleAccounting];
    pub fn enabled(self) -> bool { self != Self::None }
    pub fn ooxml(self) -> &'static str {
        match self { Self::None => "none", Self::Single => "single", Self::Double => "double", Self::SingleAccounting => "singleAccounting", Self::DoubleAccounting => "doubleAccounting" }
    }
    pub fn from_ooxml(value: &str) -> Self {
        match value { "none" => Self::None, "double" => Self::Double, "singleAccounting" => Self::SingleAccounting, "doubleAccounting" => Self::DoubleAccounting, _ => Self::Single }
    }
}

/// The format of a cell.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CellStyle {
    /// Typeface; `None` is the workbook's default.
    pub font: Option<String>,
    /// Hundredths of a point.
    pub font_size: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: CellUnderline,
    pub strike: bool,
    pub color: Rgb,
    /// The theme colour `color` is (`theme` and `tint` of the font's colour):
    /// saved as such, and `color` is that colour of the theme.
    pub color_theme: Option<ThemeColor>,
    pub fill: Option<Rgb>,
    /// The theme colour a solid `fill` is.
    pub fill_theme: Option<ThemeColor>,
    /// A gradient fill (`gradientFill`).
    pub gradient: Option<CellGradient>,
    pub left: Option<Border>,
    pub right: Option<Border>,
    pub top: Option<Border>,
    pub bottom: Option<Border>,
    /// Diagonal lines: bottom-left to top-right, and top-left to bottom-right.
    pub diagonal_up: Option<Border>,
    pub diagonal_down: Option<Border>,
    pub align: CellAlign,
    pub vertical: CellVerticalAlign,
    /// Whether the cell's alignment names its vertical position (Excel sets
    /// the lines of an explicitly bottom-aligned cell on other measures than
    /// those of a cell left at the default).
    pub vertical_set: bool,
    pub wrap: bool,
    /// Indent level (each about three characters wide).
    pub indent: u32,
    /// Shrink the text to fit the cell's width.
    pub shrink: bool,
    /// Text rotation: 0–180 degrees, or 255 for vertical text.
    pub rotation: u8,
    /// The number format code (`General`, `#,##0`, `yyyy/m/d`).
    pub number_format: String,
    /// The workbook's cell format (`cellXfs/xf`, by index) this style was
    /// read from, or made from by changing some of it. Saving writes a
    /// style that is still that format as it, and one changed from it as a
    /// new format based on it (keeping what the model does not hold, such
    /// as theme colours and protection).
    pub xf: Option<u32>,
    /// Superscript, subscript, or normal baseline.
    pub vert_align: VerticalPosition,
    /// A two-colour pattern fill (`patternFill`): preset name, fgColor, bgColor.
    pub pattern: Option<(String, Rgb, Rgb)>,
    /// Not locked (`protection/@locked="0"`): can be changed while the
    /// sheet is protected (cells are locked by default).
    pub unlocked: bool,
    /// The formula hidden while the sheet is protected (`protection/@hidden`).
    pub formula_hidden: bool,
}

impl Default for CellStyle {
    /// Excel's default: 11 pt, black, general alignment at the bottom.
    fn default() -> Self {
        Self {
            font: None,
            font_size: 1100,
            bold: false,
            italic: false,
            underline: CellUnderline::None,
            strike: false,
            color: Rgb::BLACK,
            color_theme: None,
            fill: None,
            fill_theme: None,
            gradient: None,
            left: None,
            right: None,
            top: None,
            bottom: None,
            diagonal_up: None,
            diagonal_down: None,
            align: CellAlign::General,
            vertical: CellVerticalAlign::Bottom,
            vertical_set: false,
            wrap: false,
            indent: 0,
            shrink: false,
            rotation: 0,
            number_format: "General".into(),
            xf: None,
            vert_align: VerticalPosition::Baseline,
            pattern: None,
            unlocked: false,
            formula_hidden: false,
        }
    }
}

impl CellStyle {
    /// Update effective fill and font colors according to theme color references.
    pub fn retheme(&mut self, colors: &[(String, Rgb)]) {
        if let Some(ft) = self.fill_theme {
            if let Some(rgb) = ft.resolve(colors) {
                self.fill = Some(rgb);
            }
        }
        if let Some(ct) = self.color_theme {
            if let Some(rgb) = ct.resolve(colors) {
                self.color = rgb;
            }
        }
    }
}

/// What a cell holds, as far as display goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellKind {
    #[default]
    Empty,
    Text,
    Number,
    Boolean,
    Error,
}

impl CellKind {
    /// How a value displays.
    pub fn of(value: &CellValue) -> Self {
        match value {
            CellValue::Empty => Self::Empty,
            CellValue::Number(_) => Self::Number,
            CellValue::Text(_) => Self::Text,
            CellValue::Bool(_) => Self::Boolean,
            CellValue::Error(_) => Self::Error,
        }
    }
}

/// A colour of the workbook's theme: its place in SpreadsheetML's order
/// (0 lt1, 1 dk1, 2 lt2, 3 dk2, 4–9 accent1–6) and the tint (Excel's
/// lighter/darker, in millionths: 800000 is 80 % lighter, −250000 25 %
/// darker).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThemeColor {
    pub index: u8,
    pub tint: i32,
}

impl ThemeColor {
    pub const NAMES: [&'static str; 10] = ["lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6"];

    pub fn resolve(&self, colors: &[(String, Rgb)]) -> Option<Rgb> {
        let name = Self::NAMES.get(self.index as usize)?;
        let base = colors.iter().find(|(n, _)| n == name).map(|(_, c)| *c)?;
        Some(base.tinted(self.tint))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetCell {
    pub row: u32,
    pub column: u32,
    /// The value: typed in, or calculated by the formula (as Excel last
    /// calculated it, until it is calculated again).
    pub value: CellValue,
    /// The value as displayed (number formats applied).
    pub text: String,
    pub kind: CellKind,
    /// The formula, without the leading `=` (shared formulas already moved
    /// to this cell).
    pub formula: Option<String>,
    /// Index into [`Sheet::styles`].
    pub style: u32,
    /// The reading of the text (`rPh`, the cell's furigana), which sorting
    /// goes by in place of the characters.
    pub phonetic: Option<std::sync::Arc<str>>,
    /// The text's parts in their own fonts (rich text: `<r>` runs of the
    /// string); their texts together are the value's.
    pub runs: Option<std::sync::Arc<Vec<CellRun>>>,
}

/// A part of a cell's text in its own font. A property left `None` is the
/// cell's.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellRun {
    pub text: String,
    pub font: Option<String>,
    /// Hundredths of a point.
    pub size: Option<u32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<CellUnderline>,
    pub strike: Option<bool>,
    pub color: Option<Rgb>,
}

/// Compatibility versions currently selectable for workbook calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalculationCompatibilityVersion {
    V1,
    V2,
}

impl CalculationCompatibilityVersion {
    pub const ALL: [Self; 2] = [Self::V1, Self::V2];

    pub const fn number(self) -> u32 {
        match self { Self::V1 => 1, Self::V2 => 2 }
    }
}

/// The workbook calculation mode (ECMA-376 Part 1 §18.2.3 calcPr/@calcMode).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CalcMode {
    /// Calculations are performed whenever cell values change (default).
    #[default]
    Auto,
    /// Calculations are performed automatically, except data tables.
    AutoNoTable,
    /// Calculations are performed only when explicitly requested (e.g. F9).
    Manual,
}

impl CalcMode {
    pub const ALL: [Self; 3] = [Self::Auto, Self::AutoNoTable, Self::Manual];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::AutoNoTable => "autoNoTable",
            Self::Manual => "manual",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "autoNoTable" => Some(Self::AutoNoTable),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

/// A floating-point number compared by bits, allowing `Eq` derivations.
#[derive(Clone, Copy, Debug, Default)]
pub struct SheetFloat(pub f64);

impl PartialEq for SheetFloat {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for SheetFloat {}

impl From<f64> for SheetFloat {
    fn from(f: f64) -> Self {
        Self(f)
    }
}

impl From<SheetFloat> for f64 {
    fn from(f: SheetFloat) -> Self {
        f.0
    }
}

/// Precomputed formula dependency index for recalculation, shared across snapshots.
#[derive(Clone, Debug, Default)]
pub struct CalcIndex(pub std::sync::Arc<std::sync::Mutex<Option<ibis_calc::recalc::DependencyIndex>>>, pub std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, Option<std::sync::Arc<ibis_calc::formula::Expr>>>>>);

impl CalcIndex {
    /// Invalidate the cached dependency index. The formulas it had parsed
    /// are kept for the next one (a row inserted changes few of the formulas'
    /// texts: parsing them all again took seconds in a big workbook).
    pub fn invalidate(&self) {
        if let Ok(mut guard) = self.0.lock() {
            if let Some(mut old) = guard.take()
                && let Ok(mut kept) = self.1.lock()
            {
                *kept = old.take_cache();
            }
        }
    }

    /// Build the index for `book` from its formula cells, with the formulas parsed before.
    pub fn build(&self, book: &dyn ibis_calc::eval::Book, formulas: &[(usize, u32, u32)]) -> ibis_calc::recalc::DependencyIndex {
        let previous = self.1.lock().map(|mut k| std::mem::take(&mut *k)).unwrap_or_default();
        ibis_calc::recalc::DependencyIndex::build_with_cache(book, formulas, previous)
    }
}

impl PartialEq for CalcIndex {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for CalcIndex {}

/// A workbook's own things, beside its worksheets (which are the slides):
/// every sheet in order, including hidden ones and chart sheets, and the
/// defined names. Formulas refer to sheets by these names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workbook {
    pub sheets: Vec<WorkbookSheet>,
    pub names: Vec<DefinedName>,
    /// Dates count from 1904-01-01 (`workbookPr/@date1904`) instead of 1900.
    pub date1904: bool,
    /// Explicit workbook calculation compatibility version; absent means legacy 1.
    /// Unknown future values are retained, not rewritten to a known version.
    pub compatibility_version: Option<u32>,
    /// The format of a cell with none of its own (`cellXfs` 0), which new
    /// cells get.
    pub default_style: Option<CellStyle>,
    /// Structural changes made since the workbook was read (rows and
    /// columns inserted and deleted), in order: saving makes them in the
    /// file (`sheet_ops.rs`).
    pub structure: Vec<crate::sheet_ops::StructureEdit>,
    /// The theme's colours by name (`dk1`, `accent1`, …), for formats made
    /// in the app from the theme (a new table's style).
    pub theme_colors: Vec<(String, crate::drawing::Rgb)>,
    /// The theme's fonts (`majorFont` and `minorFont`).
    pub theme_fonts: Option<crate::text::ThemeFonts>,
    /// User-defined custom cell styles (`<cellStyles>` and `<cellStyleXfs>`).
    pub custom_cell_styles: Vec<CustomCellStyle>,
    /// Calculation mode (`calcPr/@calcMode`): auto (default), autoNoTable, or manual.
    pub calc_mode: CalcMode,
    /// Recalculate before saving (`calcPr/@calcOnSave`, default true).
    pub calc_on_save: bool,
    /// Calculation precision (`calcPr/@fullPrecision`, default true).
    /// If false, calculations and entries use displayed precision.
    pub full_precision: bool,
    /// Iterative calculation enabled (`calcPr/@iterate`, default false).
    pub iterate: bool,
    /// Maximum iterations for cyclic formulas (`calcPr/@maxIterations`, default 100).
    pub max_iterations: u32,
    /// Maximum change between iterations (`calcPr/@maxChange`, default 0.001).
    pub max_change: SheetFloat,
    /// Workbook protection (`<workbookProtection>`).
    pub protection: Option<WorkbookProtection>,
    /// Document properties (`docProps/core.xml`, `docProps/app.xml`).
    pub properties: crate::flow::DocProperties,
    /// Cached dependency index for recalculation.
    pub calc_index: CalcIndex,
    /// ユーザー設定のビュー (`<customWorkbookViews>`).
    pub custom_views: Vec<CustomWorkbookView>,
    /// ウィンドウの非表示状態 (`<bookViews><workbookView visibility="hidden"/>`).
    pub window_hidden: bool,
    /// The external workbooks the formulas refer to (`externalReferences`),
    /// in order: `[1]` in a file's formula is the first.
    pub external_books: Vec<ExternalBook>,
    /// The queries of the クエリ エディター (`xl/ibisQueries.txt`) and the parameters they use.
    pub queries: Vec<crate::query::Query>,
    pub query_parameters: Vec<crate::query::Parameter>,
}

/// An external workbook (`externalLink/externalBook`): its file's name as
/// Excel shows it in a formula (`[売上.xlsx]Sheet1!A1`), and the cells of
/// its sheets the file caches (`sheetDataSet`), which the formulas read
/// when the workbook is not open. A link that is not a workbook (DDE, OLE)
/// has no name and no sheets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalBook {
    pub name: String,
    /// Where the file is, as the link names it (`externalLinkPath`: a path
    /// or `file:///…`), for updating the cached cells from it.
    pub path: String,
    pub sheets: Vec<ExternalSheet>,
    /// The link was broken (リンクの解除): no formula refers to it, and
    /// saving takes it out of the file.
    pub broken: bool,
}

/// A sheet of an external workbook, with its cached cells by (row, column).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExternalSheet {
    pub name: String,
    pub cells: std::collections::BTreeMap<(u32, u32), CellValue>,
}

impl Workbook {
    /// Whether any hidden sheet stored in the workbook contains an Excel table (`<table>`).
    pub fn has_tables(&self) -> bool {
        self.sheets.iter().any(|ws| ws.hidden.as_ref().map_or(false, |s| !s.tables.is_empty()))
    }
}

impl Default for Workbook {
    fn default() -> Self {
        Self {
            sheets: Vec::new(),
            names: Vec::new(),
            date1904: false,
            compatibility_version: None,
            default_style: None,
            structure: Vec::new(),
            theme_colors: Vec::new(),
            theme_fonts: None,
            custom_cell_styles: Vec::new(),
            calc_mode: CalcMode::Auto,
            calc_on_save: true,
            full_precision: true,
            iterate: false,
            max_iterations: 100,
            max_change: SheetFloat(0.001),
            protection: None,
            properties: crate::flow::DocProperties::default(),
            calc_index: CalcIndex::default(),
            custom_views: Vec::new(),
            window_hidden: false,
            external_books: Vec::new(),
            queries: Vec::new(),
            query_parameters: Vec::new(),
        }
    }
}

/// A user-defined cell style (`<cellStyles>` and `<cellStyleXfs>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomCellStyle {
    pub name: String,
    pub format: CellStyle,
    pub include_number_format: bool,
    pub include_alignment: bool,
    pub include_font: bool,
    pub include_border: bool,
    pub include_fill: bool,
    pub include_protection: bool,
}

impl CustomCellStyle {
    /// Convert the custom cell style's active components into a list of style changes.
    pub fn to_style_changes(&self) -> Vec<crate::sheet_command::StyleChange> {
        use crate::sheet_command::{Edge, StyleChange};
        let mut changes = Vec::new();
        if self.include_number_format {
            changes.push(StyleChange::NumberFormat(self.format.number_format.clone()));
        }
        if self.include_alignment {
            changes.push(StyleChange::Align(self.format.align));
            changes.push(StyleChange::Vertical(self.format.vertical));
            changes.push(StyleChange::Wrap(self.format.wrap));
            changes.push(StyleChange::Indent(self.format.indent));
            changes.push(StyleChange::Shrink(self.format.shrink));
            changes.push(StyleChange::Rotation(self.format.rotation));
        }
        if self.include_font {
            changes.push(StyleChange::Font(self.format.font.clone()));
            changes.push(StyleChange::FontSize(self.format.font_size));
            changes.push(StyleChange::Bold(self.format.bold));
            changes.push(StyleChange::Italic(self.format.italic));
            changes.push(StyleChange::UnderlineKind(self.format.underline));
            changes.push(StyleChange::Strike(self.format.strike));
            changes.push(StyleChange::Color(self.format.color));
            if let Some(theme) = self.format.color_theme {
                changes.push(StyleChange::ColorTheme(self.format.color, theme));
            }
        }
        if self.include_border {
            changes.push(StyleChange::Border(Edge::Left, self.format.left));
            changes.push(StyleChange::Border(Edge::Right, self.format.right));
            changes.push(StyleChange::Border(Edge::Top, self.format.top));
            changes.push(StyleChange::Border(Edge::Bottom, self.format.bottom));
        }
        if self.include_fill {
            changes.push(StyleChange::Fill(self.format.fill));
            if let Some(theme) = self.format.fill_theme {
                if let Some(c) = self.format.fill {
                    changes.push(StyleChange::FillTheme(c, theme));
                }
            }
            if let Some(p) = self.format.pattern.clone() {
                changes.push(StyleChange::Pattern(Some(p)));
            }
            if let Some(g) = self.format.gradient.clone() {
                changes.push(StyleChange::Gradient(Some(g)));
            }
        }
        if self.include_protection {
            changes.push(StyleChange::Locked(!self.format.unlocked));
            changes.push(StyleChange::FormulaHidden(self.format.formula_hidden));
        }
        changes
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SheetState {
    #[default]
    Visible,
    Hidden,
    VeryHidden,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkbookSheet {
    pub name: String,
    pub state: SheetState,
    /// The slide a shown worksheet is on.
    pub slide: Option<SlideId>,
    /// The cells of a hidden worksheet (for formulas that refer to it).
    pub hidden: Option<Arc<Sheet>>,
    /// Not a worksheet (a chart sheet, a dialog or macro sheet).
    pub other: bool,
}

/// A defined name (`definedName`): a name for a range, a constant or a formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinedName {
    pub name: String,
    /// The sheet (index in [`Workbook::sheets`]) it belongs to, if it is local.
    pub sheet: Option<usize>,
    /// What it stands for, without `=`.
    pub formula: String,
    pub hidden: bool,
}

impl Workbook {
    /// The index of the sheet named `name` (case-insensitive, as Excel).
    pub fn sheet_index(&self, name: &str) -> Option<usize> {
        self.sheets.iter().position(|s| s.name.to_lowercase() == name.to_lowercase())
    }
}

/// A cell reference such as `B12` for a 0-based column and row.
pub fn cell_name(column: u32, row: u32) -> String {
    let mut letters = String::new();
    let mut c = column + 1;
    while c > 0 {
        let rem = (c - 1) % 26;
        letters.insert(0, (b'A' + rem as u8) as char);
        c = (c - 1) / 26;
    }
    format!("{letters}{}", row + 1)
}

/// A merged range, inclusive, 0-based.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellRange {
    pub first_row: u32,
    pub first_column: u32,
    pub last_row: u32,
    pub last_column: u32,
}

fn next_sheet_content_id() -> usize {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// A sheet's cells, shared between copies of the sheet until one of them changes a cell: a copy of a sheet (the one the screen's layout holds, the one undo keeps) costs nothing while its cells are the same, and changing a column's width does not copy a million rows. Reads as a `Vec`; changing it copies the cells first if another copy holds them.
#[derive(Clone, Debug)]
pub struct Cells(std::sync::Arc<Vec<SheetCell>>, usize);

impl Default for Cells {
    fn default() -> Self {
        Self(std::sync::Arc::default(), next_sheet_content_id())
    }
}

impl PartialEq for Cells {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for Cells {}

impl std::ops::Deref for Cells {
    type Target = Vec<SheetCell>;
    fn deref(&self) -> &Vec<SheetCell> {
        &self.0
    }
}

impl std::ops::DerefMut for Cells {
    fn deref_mut(&mut self) -> &mut Vec<SheetCell> {
        self.1 = next_sheet_content_id();
        std::sync::Arc::make_mut(&mut self.0)
    }
}

impl Cells {
    /// The cells as a `Vec` (copied if another copy of the sheet holds them).
    pub fn into_vec(self) -> Vec<SheetCell> {
        std::sync::Arc::try_unwrap(self.0).unwrap_or_else(|shared| (*shared).clone())
    }

    /// Stable identifier for this sheet's cell/layout content across view-only zoom changes.
    pub fn content_id(&self) -> usize {
        self.1
    }

    /// Assign a fresh content identifier when the sheet's visual content or layout changes.
    pub fn bump_content_id(&mut self) {
        self.1 = next_sheet_content_id();
    }
}

impl From<Vec<SheetCell>> for Cells {
    fn from(cells: Vec<SheetCell>) -> Self {
        Cells(std::sync::Arc::new(cells), next_sheet_content_id())
    }
}

impl<'a> IntoIterator for &'a Cells {
    type Item = &'a SheetCell;
    type IntoIter = std::slice::Iter<'a, SheetCell>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut Cells {
    type Item = &'a mut SheetCell;
    type IntoIter = std::slice::IterMut<'a, SheetCell>;
    fn into_iter(self) -> Self::IntoIter {
        self.1 = next_sheet_content_id();
        std::sync::Arc::make_mut(&mut self.0).iter_mut()
    }
}

/// Workbook-level context cached on each [`Sheet`] so single-sheet consumers
/// (such as conditional formatting evaluation) can resolve defined names,
/// cross-sheet references, the sheet's own index/name, and the 1904 date system.
#[derive(Debug, Clone, Default)]
pub struct SheetBookContext {
    pub sheet_name: String,
    pub sheet_index: usize,
    pub defined_names: std::sync::Arc<[DefinedName]>,
    pub date1904: bool,
    pub sheets: std::sync::Arc<[(String, Cells)]>,
}

impl SheetBookContext {
    /// Whether two contexts carry identical workbook state and sheet cell content IDs.
    pub fn exact_eq(&self, other: &Self) -> bool {
        self.sheet_name == other.sheet_name
            && self.sheet_index == other.sheet_index
            && self.date1904 == other.date1904
            && self.defined_names == other.defined_names
            && self.sheets.len() == other.sheets.len()
            && self
                .sheets
                .iter()
                .zip(other.sheets.iter())
                .all(|((na, ca), (nb, cb))| na == nb && ca.content_id() == cb.content_id())
    }
}

impl PartialEq for SheetBookContext {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for SheetBookContext {}

/// A value the app works out when it opens a file and that is not part of the file: two sheets are equal whatever it holds
/// (reading a file again gives the sheet without it), and it is not saved. Compare its contents through `.0`.
#[derive(Clone, Debug, Default)]
pub struct Unsaved<T>(pub T);

impl<T> PartialEq for Unsaved<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T> Eq for Unsaved<T> {}

impl<T> std::ops::Deref for Unsaved<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Unsaved<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sheet {
    /// Column widths, in EMU (0 for a hidden column).
    pub columns: Vec<Emu>,
    /// Row heights, in EMU (0 for a hidden row).
    pub rows: Vec<Emu>,
    /// Rows whose height the file fixes (`customHeight`). `rows` keeps the stored height; printing lays
    /// these rows out on whole points (Excel truncates a fractional stored height there).
    pub custom_rows: std::collections::BTreeSet<u32>,
    /// Columns whose width the user (or the file: `customWidth="1"`) fixed. A column outside this set
    /// widens by itself when a number wider than it is entered, and its width is not saved (as in Excel).
    pub custom_columns: std::collections::BTreeSet<u32>,
    /// Cells with a value or a visible format, in row-major order.
    pub cells: Cells,
    pub styles: Vec<CellStyle>,
    pub merges: Vec<CellRange>,
    /// Whether to draw the grid lines between cells.
    pub grid_lines: bool,
    /// Whether to draw the row numbers and column letters (`showRowColHeaders`).
    pub headings: bool,
    /// The range printed (`_xlnm.Print_Area`), if one is set.
    pub print_area: Option<CellRange>,
    /// The colour of the sheet's tab (`sheetPr/tabColor`).
    pub tab_color: Option<Rgb>,
    /// Frozen panes: the columns and rows (counts from the top left) that
    /// stay in view while the rest scrolls.
    pub frozen: Option<(u32, u32)>,
    /// Split panes: the columns and rows (counts from the top left) where
    /// the window is split into scrollable panes (`pane/@state="split"`).
    pub split: Option<(u32, u32)>,
    /// シートの保護 (`sheetProtection`), when the sheet is protected.
    pub protection: Option<SheetProtection>,
    /// 範囲の編集の許可 (`protectedRanges`), allowing specific ranges to be
    /// edited even when the sheet is protected.
    pub protected_ranges: Vec<ProtectedRange>,
    /// Shown in page break preview (`sheetView/@view`), where the area
    /// outside the print area is greyed.
    pub page_break_preview: bool,
    /// Shown in page layout view (`sheetView/@view="pageLayout"`): the
    /// pages' paper, margins and numbers drawn over the cells.
    pub page_layout: bool,
    /// Whether to show the ruler in page layout view (`sheetView/@showRuler`).
    pub show_ruler: bool,
    /// The zoom the sheet is shown at in percent (`sheetView/@zoomScale`),
    /// if not 100%.
    pub zoom: Option<u16>,
    /// How the sheet is printed.
    pub print: SheetPrint,
    /// The height of a row and the width of a column with no size of
    /// their own (for rows and columns inserted).
    pub default_row: Emu,
    pub default_column: Emu,
    /// The rows (0-based) that have no height of their own (`customHeight`) and no wrapping text: Excel measures them by the
    /// fonts in them, whatever height the file stored (`ibis-office-app`'s `xlsx::row_fit` does, where the fonts can be read).
    /// Not saved; empty for a sheet made here.
    pub measured_rows: Unsaved<Vec<u32>>,
    /// The height each of the `measured_rows` was given when the sheet was opened (what the fonts in it measure, which is not
    /// what the file stored). A row still at that height is not a height the person chose: saving does not write it.
    pub fitted_rows: Unsaved<std::collections::BTreeMap<u32, Emu>>,
    /// How many rows the file prints (0: all of `rows`). Rows past it hold only a height (`<row ht customHeight/>` with no cell):
    /// Excel keeps them but prints no page for them, so printing lays the sheet out without them. Not saved.
    pub printed_rows: Unsaved<u32>,
    /// The font of each `<col>` of the file (first column, last column, typeface, size in hundredths of a point; `None` and 0 when
    /// the column has no style of its own, so it is the Normal font): a cell no one wrote takes its column's style, and Excel
    /// measures a row by all the cells of it, the unwritten ones too. Not saved.
    pub column_fonts: Unsaved<Vec<(u32, u32, Option<String>, u32)>>,
    /// The font of the style of each row that has one of its own (`<row s customFormat="1">`; `None` and 0 for the Normal style):
    /// the cells of the row no one wrote take it, not their column's. Not saved.
    pub row_fonts: Unsaved<std::collections::BTreeMap<u32, (Option<String>, u32)>>,
    /// The fonts (typeface, size in hundredths of a point) of the cells written in each row of the file, empty ones too, but not the
    /// ones in merged ranges. Excel measures a row by them and by the cells no one wrote. Not saved.
    pub written_fonts: Unsaved<std::collections::BTreeMap<u32, Vec<(Option<String>, u32)>>>,
    /// The height of each hidden row and the width of each hidden column
    /// (their size in `rows` and `columns` is 0), to show them at again.
    pub hidden_rows: std::collections::BTreeMap<u32, Emu>,
    pub hidden_columns: std::collections::BTreeMap<u32, Emu>,
    /// The dynamic array formulas (Excel 365's: an array result spills
    /// into the cells beside it), by their cell (row, column), with the
    /// area their result takes (just the cell when it is one value). When
    /// the cell shows `#SPILL!`, that area was blocked and holds nothing
    /// of it; otherwise its other cells hold the result's values.
    pub spills: std::collections::BTreeMap<(u32, u32), CellRange>,
    /// Conditional formats (`conditionalFormatting`), in the file's order.
    pub conditional: Vec<ConditionalFormat>,
    /// Rules only Excel 2010 has (`x14:cfRule` in the sheet's `extLst` that no rule of 2007 names: the icon sets
    /// 3Stars, 3Triangles and 5Boxes): drawn like the others, never written back (the file keeps them as they are).
    pub conditional_x14: Vec<ConditionalFormat>,
    /// Data validation (`dataValidations`), in the file's order.
    pub validations: Vec<Validation>,
    /// Hyperlinks (`hyperlinks`), in the file's order.
    pub hyperlinks: Vec<Hyperlink>,
    /// Comments on cells (`comments`), in the file's order.
    pub comments: Vec<SheetComment>,
    /// Outline (group) levels of rows and columns (`outlineLevel`, 1–7).
    pub outline_rows: std::collections::BTreeMap<u32, u8>,
    pub outline_columns: std::collections::BTreeMap<u32, u8>,
    /// The filter over a table (`autoFilter`), if one is set.
    pub auto_filter: Option<AutoFilter>,
    /// Rows excluded when the filter was applied; edits do not reapply it.
    pub filtered_rows: std::collections::BTreeSet<u32>,
    /// The tables (`tableParts`) on the sheet, for their structured
    /// references (`Table1[Column]`).
    pub tables: Vec<SheetTable>,
    /// Sparklines (`x14:sparklineGroups` in the sheet's `extLst`).
    pub sparklines: Vec<SparklineGroup>,
    /// Threaded comments (Excel 365's コメント, beside the notes in
    /// `comments`), as read: shown, not yet edited.
    pub threads: Vec<ThreadedComment>,
    /// Pivot tables (ピボットテーブル) on the sheet.
    pub pivots: Vec<crate::pivot::PivotTable>,
    /// What-If data tables (`{=TABLE(…)}`): the cells they fill hold values only.
    pub data_tables: Vec<DataTable>,
    /// データ統合 (`<dataConsolidate>`).
    pub data_consolidate: Option<DataConsolidate>,
    /// What-If 分析のシナリオ (`<scenarios>`).
    pub scenarios: Option<SheetScenarios>,
    /// ユーザー設定のシート ビュー (`<customSheetViews>`).
    pub custom_views: Vec<CustomSheetView>,
    /// ふりがなの表示 (`showPhonetic`).
    pub show_phonetic: bool,
    /// ふりがなの設定 (`<phoneticPr>`).
    pub phonetic_pr: Option<PhoneticProperties>,
    /// エラー チェックで無視するエラー (`ignoredErrors`).
    pub ignored_errors: Vec<crate::error_check::IgnoredError>,
    /// The cells that hold a picture (セル内の画像, Excel 365's 「セルに配置」,
    /// `vm` of the cell → `rdrichvalue.xml`): where it is, and which of the
    /// workbook's rich value relationships (`richValueRel.xml`) is its file.
    /// The picture is drawn as a shape over the cell.
    pub cell_images: Vec<CellImage>,
    /// The pictures IMAGE shows from an address, by cell (row, column): what the calculation of the formula asked for
    /// (the address read as any reference is, other sheets, names and other workbooks too). Not in the file.
    pub web_images: std::collections::BTreeMap<(u32, u32), ibis_calc::eval::ImageSpec>,
    /// 数式の表示 (`sheetView/@showFormulas`).
    pub show_formulas: bool,
    /// ゼロ値の表示 (`sheetView/@showZeros`, 既定 true).
    pub show_zeros: bool,
    /// インクの非表示.
    pub hide_ink: bool,
    /// Cached workbook context for single-sheet evaluation (conditional formatting, etc.).
    pub book_context: SheetBookContext,
}

impl Default for Sheet {
    fn default() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            cells: Cells::default(),
            styles: Vec::new(),
            merges: Vec::new(),
            grid_lines: false,
            headings: false,
            print_area: None,
            tab_color: None,
            frozen: None,
            split: None,
            protection: None,
            protected_ranges: Vec::new(),
            page_break_preview: false,
            page_layout: false,
            show_ruler: false,
            zoom: None,
            print: SheetPrint::default(),
            default_row: 0,
            default_column: 0,
            measured_rows: Unsaved::default(),
            fitted_rows: Unsaved::default(),
            printed_rows: Unsaved::default(),
            column_fonts: Unsaved::default(),
            row_fonts: Unsaved::default(),
            written_fonts: Unsaved::default(),
            hidden_rows: std::collections::BTreeMap::new(),
            custom_rows: std::collections::BTreeSet::new(),
            custom_columns: std::collections::BTreeSet::new(),
            hidden_columns: std::collections::BTreeMap::new(),
            spills: std::collections::BTreeMap::new(),
            conditional: Vec::new(),
            conditional_x14: Vec::new(),
            validations: Vec::new(),
            hyperlinks: Vec::new(),
            comments: Vec::new(),
            outline_rows: std::collections::BTreeMap::new(),
            outline_columns: std::collections::BTreeMap::new(),
            auto_filter: None,
            filtered_rows: std::collections::BTreeSet::new(),
            tables: Vec::new(),
            sparklines: Vec::new(),
            threads: Vec::new(),
            pivots: Vec::new(),
            data_tables: Vec::new(),
            data_consolidate: None,
            scenarios: None,
            custom_views: Vec::new(),
            show_phonetic: false,
            phonetic_pr: None,
            ignored_errors: Vec::new(),
            cell_images: Vec::new(),
            web_images: std::collections::BTreeMap::new(),
            show_formulas: false,
            show_zeros: true,
            hide_ink: false,
            book_context: SheetBookContext::default(),
        }
    }
}

/// A picture in a cell (see [`Sheet::cell_images`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellImage {
    pub row: u32,
    pub column: u32,
    /// The index among `richValueRel.xml`'s `rel`s.
    pub index: u32,
}

/// ふりがなの種類 (`type`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhoneticType {
    HalfwidthKatakana,
    #[default]
    FullwidthKatakana,
    Hiragana,
    NoConversion,
}

impl PhoneticType {
    pub fn to_ooxml(self) -> &'static str {
        match self {
            Self::HalfwidthKatakana => "halfwidthKatakana",
            Self::FullwidthKatakana => "fullwidthKatakana",
            Self::Hiragana => "Hiragana",
            Self::NoConversion => "noConversion",
        }
    }

    pub fn from_ooxml(s: &str) -> Option<Self> {
        match s {
            "halfwidthKatakana" => Some(Self::HalfwidthKatakana),
            "fullwidthKatakana" => Some(Self::FullwidthKatakana),
            "Hiragana" | "hiragana" => Some(Self::Hiragana),
            "noConversion" => Some(Self::NoConversion),
            _ => None,
        }
    }
}

/// ふりがなの配置 (`alignment`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhoneticAlignment {
    #[default]
    Left,
    Center,
    Distributed,
    NoControl,
}

impl PhoneticAlignment {
    pub fn to_ooxml(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Distributed => "distributed",
            Self::NoControl => "noControl",
        }
    }

    pub fn from_ooxml(s: &str) -> Option<Self> {
        match s {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "distributed" => Some(Self::Distributed),
            "noControl" => Some(Self::NoControl),
            _ => None,
        }
    }
}

/// ふりがなのプロパティ (`<phoneticPr>`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhoneticProperties {
    pub font_id: u32,
    pub phonetic_type: PhoneticType,
    pub alignment: PhoneticAlignment,
}

/// ユーザー設定のブック ビュー (`<customWorkbookView>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomWorkbookView {
    /// ビュー名 (`name`).
    pub name: String,
    /// 一意の識別子 (`guid`).
    pub guid: String,
    /// 印刷設定を含める (`includePrintSettings`, 既定 true).
    pub include_print_settings: bool,
    /// 非表示の行・列・フィルターの設定を含める (`includeHiddenRowCol`, 既定 true).
    pub include_hidden: bool,
    /// ビュー作成時のアクティブ シート インデックス (0-based, OOXML では `activeSheetId`).
    pub active_sheet_index: usize,
}

/// ユーザー設定のシート ビュー (`<customSheetView>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomSheetView {
    /// 紐づく CustomWorkbookView の GUID.
    pub guid: String,
    /// ビュー名 (`name`).
    pub name: String,
    /// ズーム倍率 (`scale`).
    pub zoom: Option<u16>,
    /// 枠線（グリッド線）の表示 (`showGridLines`).
    pub grid_lines: bool,
    /// 行番号・列記号（見出し）の表示 (`showRowCol`).
    pub headings: bool,
    /// ルーラーの表示 (`showRuler`).
    pub show_ruler: bool,
    /// 改ページ プレビュー (`view="pageBreakPreview"`).
    pub page_break_preview: bool,
    /// ウィンドウ枠の固定 (`<pane>`).
    pub frozen: Option<(u32, u32)>,
    /// ウィンドウの分割 (`<pane state="split">`).
    pub split: Option<(u32, u32)>,
    /// アクティブ セル (`<selection activeCell="...">`).
    pub active_cell: Option<(u32, u32)>,
    /// 選択範囲 (`<selection sqref="...">`).
    pub selection_range: Option<CellRange>,
    /// 印刷設定 (`includePrintSettings` が true の場合にスナップショット保存).
    pub print: Option<SheetPrint>,
    /// 非表示の行 (`includeHidden` が true の場合にスナップショット保存: row -> height).
    pub hidden_rows: Option<std::collections::BTreeMap<u32, Emu>>,
    /// 非表示の列 (`includeHidden` が true の場合にスナップショット保存: col -> width).
    pub hidden_columns: Option<std::collections::BTreeMap<u32, Emu>>,
    /// オートフィルター (`includeHidden` が true の場合にスナップショット保存).
    pub auto_filter: Option<Option<AutoFilter>>,
    /// フィルター除外行 (`includeHidden` が true の場合にスナップショット保存).
    pub filtered_rows: Option<std::collections::BTreeSet<u32>>,
    /// 数式の表示 (`showFormulas`).
    pub show_formulas: bool,
    /// ゼロ値の表示 (`showZeros`, 既定 true).
    pub show_zeros: bool,
}

impl Default for CustomSheetView {
    fn default() -> Self {
        Self {
            guid: String::new(),
            name: String::new(),
            zoom: None,
            grid_lines: false,
            headings: false,
            show_ruler: false,
            page_break_preview: false,
            frozen: None,
            split: None,
            active_cell: None,
            selection_range: None,
            print: None,
            hidden_rows: None,
            hidden_columns: None,
            auto_filter: None,
            filtered_rows: None,
            show_formulas: false,
            show_zeros: true,
        }
    }
}

/// データ テーブル (`<f t="dataTable">`): `range` is the block of results; the
/// values it substitutes are in the row above and the column left of it,
/// and the formulas in the row above (inputs down a column), the column
/// left of it (inputs along a row) or the corner (two inputs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataTable {
    pub range: CellRange,
    /// 行の入力セル (the values across the top go in it), (row, column).
    pub row_input: Option<(u32, u32)>,
    /// 列の入力セル (the values down the left go in it).
    pub column_input: Option<(u32, u32)>,
}

/// A consolidate function (`dataConsolidate/@function`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConsolidateFunction {
    #[default]
    Sum,
    Count,
    Average,
    Max,
    Min,
    Product,
    CountNums,
    StdDev,
    StdDevp,
    Var,
    Varp,
}

impl ConsolidateFunction {
    pub const ALL: [Self; 11] = [
        Self::Sum,
        Self::Count,
        Self::Average,
        Self::Max,
        Self::Min,
        Self::Product,
        Self::CountNums,
        Self::StdDev,
        Self::StdDevp,
        Self::Var,
        Self::Varp,
    ];

    pub fn ooxml(self) -> &'static str {
        match self {
            Self::Sum => "sum",
            Self::Count => "count",
            Self::Average => "average",
            Self::Max => "max",
            Self::Min => "min",
            Self::Product => "product",
            Self::CountNums => "countNums",
            Self::StdDev => "stdDev",
            Self::StdDevp => "stdDevp",
            Self::Var => "var",
            Self::Varp => "varp",
        }
    }

    pub fn from_ooxml(s: &str) -> Option<Self> {
        match s {
            "sum" => Some(Self::Sum),
            "count" => Some(Self::Count),
            "average" => Some(Self::Average),
            "max" => Some(Self::Max),
            "min" => Some(Self::Min),
            "product" => Some(Self::Product),
            "countNums" => Some(Self::CountNums),
            "stdDev" => Some(Self::StdDev),
            "stdDevp" => Some(Self::StdDevp),
            "var" => Some(Self::Var),
            "varp" => Some(Self::Varp),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Sum => "合計",
            Self::Count => "個数",
            Self::Average => "平均",
            Self::Max => "最大値",
            Self::Min => "最小値",
            Self::Product => "積",
            Self::CountNums => "数値の個数",
            Self::StdDev => "標本標準偏差",
            Self::StdDevp => "標準偏差",
            Self::Var => "標本分散",
            Self::Varp => "分散",
        }
    }
}

/// データ統合 (`<dataConsolidate>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataConsolidate {
    pub function: ConsolidateFunction,
    pub left_labels: bool,
    pub top_labels: bool,
    pub link: bool,
    pub data_refs: Vec<String>,
}

/// Parameters for inserting subtotals into a sheet (`データ > 小計`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtotalParams {
    /// The entire data range including the top header row.
    pub range: CellRange,
    /// The 0-based column index to group by (change in value triggers subtotal).
    pub group_by_column: u32,
    /// The function code for SUBTOTAL:
    /// 1: AVERAGE, 2: COUNT, 3: COUNTA, 4: MAX, 5: MIN, 6: PRODUCT,
    /// 7: STDEV, 8: STDEVP, 9: SUM, 10: VAR, 11: VARP
    pub function_code: u32,
    /// The columns (0-based) where SUBTOTAL formulas should be placed.
    pub subtotal_columns: Vec<u32>,
    /// Whether to remove existing subtotals before applying.
    pub replace_existing: bool,
    /// Whether to insert summary row below data (true: default in Excel) or above (false).
    pub summary_below: bool,
    /// Optional: whether to insert page breaks between groups.
    pub page_breaks: bool,
}

impl Default for SubtotalParams {
    fn default() -> Self {
        Self {
            range: CellRange::default(),
            group_by_column: 0,
            function_code: 9, // SUM
            subtotal_columns: Vec::new(),
            replace_existing: true,
            summary_below: true,
            page_breaks: false,
        }
    }
}

/// What-If 分析のシナリオ集 (`<scenarios>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetScenarios {
    pub current: Option<u32>,
    pub show: Option<u32>,
    pub sqref: Option<String>,
    pub scenarios: Vec<SheetScenario>,
}

/// What-If 分析のシナリオ (`<scenario>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetScenario {
    pub name: String,
    pub comment: Option<String>,
    pub user: Option<String>,
    pub locked: bool,
    pub hidden: bool,
    pub input_cells: Vec<ScenarioInputCell>,
}

/// シナリオの変化させるセル (`<inputCells>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScenarioInputCell {
    /// セル参照（例: "B1"）
    pub r: String,
    /// シナリオにおけるセルの値（文字列形式）
    pub val: String,
    /// 表示形式 ID
    pub num_fmt_id: Option<u32>,
    pub deleted: bool,
}

/// The kind of a sparkline group (`x14:sparklineGroup/@type`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SparklineKind {
    #[default]
    Line,
    Column,
    /// Win/loss (`stacked`).
    WinLoss,
}

/// How a sparkline shows an empty cell (`displayEmptyCellsAs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SparklineEmpty {
    #[default]
    Gap,
    Zero,
    /// The line joins the points either side.
    Span,
}

/// Where a sparkline group's vertical axis ends (`minAxisType`,
/// `maxAxisType`): each sparkline's own data, the group's, or a value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SparklineAxis {
    #[default]
    Individual,
    Group,
    /// `manualMin` / `manualMax`, as written.
    Custom(String),
}

/// One sparkline: the cell it is drawn in and its data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sparkline {
    pub row: u32,
    pub column: u32,
    /// The data's sheet when the reference names one.
    pub sheet: Option<String>,
    pub data: CellRange,
}

/// A group of sparklines sharing their settings (`x14:sparklineGroup`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SparklineGroup {
    pub kind: SparklineKind,
    /// `x14:colorSeries`; without it Excel draws a line black and no
    /// columns or win/loss bars at all.
    pub series: Option<Rgb>,
    /// The colours below likewise: one the group leaves out is not drawn
    /// (Excel for Mac's PDFs of xlsx-XSPK: no markers, no axis).
    pub negative: Option<Rgb>,
    pub axis: Option<Rgb>,
    pub markers_color: Option<Rgb>,
    pub first_color: Option<Rgb>,
    pub last_color: Option<Rgb>,
    pub high_color: Option<Rgb>,
    pub low_color: Option<Rgb>,
    /// The line's width, hundredths of a point (`lineWeight`, 0.75 pt).
    pub line_weight: u32,
    pub markers: bool,
    pub high: bool,
    pub low: bool,
    pub first: bool,
    pub last: bool,
    pub negative_points: bool,
    pub display_x_axis: bool,
    pub empty: SparklineEmpty,
    pub min_axis: SparklineAxis,
    pub max_axis: SparklineAxis,
    pub right_to_left: bool,
    /// `displayHidden="1"`: Plot data in hidden rows and columns (default: false).
    pub display_hidden: bool,
    /// `dateAxis="1"`: Plot data along a date axis (default: false).
    pub date_axis: bool,
    /// Formula for date axis data range (`<xm:f>`), if dateAxis is true.
    pub date_range: Option<String>,
    pub sparklines: Vec<Sparkline>,
}

impl Default for SparklineGroup {
    fn default() -> Self {
        Self {
            kind: SparklineKind::Line,
            series: Some(Rgb(0x376092)),
            negative: Some(Rgb(0xD00000)),
            axis: Some(Rgb(0x000000)),
            markers_color: Some(Rgb(0xD00000)),
            first_color: Some(Rgb(0xD00000)),
            last_color: Some(Rgb(0xD00000)),
            high_color: Some(Rgb(0x0070C0)),
            low_color: Some(Rgb(0xC00000)),
            line_weight: 75,
            markers: false,
            high: false,
            low: false,
            first: false,
            last: false,
            negative_points: false,
            display_x_axis: false,
            empty: SparklineEmpty::Gap,
            min_axis: SparklineAxis::Individual,
            max_axis: SparklineAxis::Individual,
            right_to_left: false,
            display_hidden: false,
            date_axis: false,
            date_range: None,
            sparklines: Vec::new(),
        }
    }
}

/// A table on a sheet (`xl/tables/table*.xml`): its name, its range with
/// the header and totals rows, and its columns' names in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetTable {
    /// The name formulas use (`displayName`).
    pub name: String,
    pub range: CellRange,
    pub header_rows: u32,
    pub totals_rows: u32,
    pub columns: Vec<String>,
    /// Its table style and options (`tableStyleInfo`), if it names one.
    pub style: Option<TableStyleInfo>,
    /// Its filter (`autoFilter`: the ▼ of its headings and what they keep);
    /// `None`: no buttons. The range is the table's without its totals.
    pub filter: Option<AutoFilter>,
    /// Each column's 集計行 (`tableColumn/@totalsRowFunction`,
    /// `@totalsRowLabel`), in the columns' order; empty when none is known.
    pub totals: Vec<TableTotal>,
}

/// A column's cell of a table's 集計行: its function (`sum`, `average`,
/// `count`, `countNums`, `max`, `min`, `stdDev`, `var`, `custom`) or its
/// label.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableTotal {
    pub function: Option<String>,
    pub label: Option<String>,
}

/// A table's style (`tableStyleInfo`): the style's name and which of its
/// parts show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableStyleInfo {
    pub name: String,
    pub first_column: bool,
    pub last_column: bool,
    pub row_stripes: bool,
    pub column_stripes: bool,
}

/// A cell's comment or note.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetComment {
    pub row: u32,
    pub column: u32,
    pub author: String,
    pub text: String,
}

/// A threaded comment (`threadedComment`): the first post on a cell and
/// its replies, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadedComment {
    /// The first post's `id`, which the replies' `parentId` name.
    pub id: String,
    pub row: u32,
    pub column: u32,
    pub posts: Vec<ThreadPost>,
    /// Resolved (`done`).
    pub done: bool,
}

/// One post of a thread: who (the person's display name), when (`dT`, as
/// written), and the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadPost {
    pub author: String,
    pub date: String,
    pub text: String,
}

/// A filter (オートフィルター): the range (its first row the headings)
/// and, by column, what is kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoFilter {
    pub range: CellRange,
    pub columns: Vec<(u32, ColumnFilter)>,
}

/// What a filtered column keeps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnFilter {
    /// The rows whose cell shows one of these texts; `""` keeps the blank
    /// cells (`filters/@blank`).
    Values(Vec<String>),
    /// Values and dates by year, month, day, … (`filters` with
    /// `dateGroupItem`s), as Excel's ▼ list of a date column writes them.
    Dates { values: Vec<String>, groups: Vec<DateGroup> },
    /// Above (or below) the column's average (`dynamicFilter`).
    AboveAverage(bool),
    /// One or two comparisons (`customFilters`), both kept (`and`) or
    /// either: an operator (`equal`, `greaterThan`, …) and a value, a
    /// number or text with the wildcards `*`, `?` and `~`.
    Custom { and: bool, conditions: Vec<(String, String)> },
    /// The highest or lowest items, or percent of them (`top10`); the
    /// count as written.
    Top { top: bool, percent: bool, value: String },
    /// A period of dates (`dynamicFilter/@type`: `today`, `thisMonth`,
    /// `lastYear`, `Q1`, `M1`, …).
    Period(String),
    /// The cells filled with a colour, or with text of a colour
    /// (`colorFilter`); `None`: no fill, or the automatic text colour.
    Color { cell: bool, color: Option<Rgb> },
    /// An icon of a conditional formatting icon set (`iconFilter`).
    Icon { icon_set: String, icon_id: u32 },
}

/// A date (or part of one) a date filter keeps (`dateGroupItem`): every
/// date in the year, month, … down to `grouping`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DateGroup {
    /// `year`, `month`, `day`, `hour`, `minute` or `second`.
    pub grouping: String,
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

/// A cell's (or range's) hyperlink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hyperlink {
    pub range: CellRange,
    /// A web address or file (the relationship's target).
    pub target: Option<String>,
    /// A place in the workbook (`Sheet1!A1`, a name).
    pub location: Option<String>,
    pub tooltip: Option<String>,
}

/// A data validation rule (`dataValidation`) over its ranges.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Validation {
    pub ranges: Vec<CellRange>,
    /// `list`, `whole`, `decimal`, `date`, `time`, `textLength`, `custom` (`none`: any value, with a message).
    pub kind: String,
    /// `between`, `notBetween`, `equal`, … (not for `list` and `custom`).
    pub operator: String,
    /// The formulas without `=`: a list's items (`"A,B,C"` in quotes, or a
    /// range), the bounds, or the custom formula.
    pub formula1: Option<String>,
    pub formula2: Option<String>,
    pub allow_blank: bool,
    /// A list shows its drop-down (Excel's `showDropDown="1"` hides it).
    pub dropdown: bool,
    /// The message shown when a cell is selected, if any (title, text).
    pub prompt: Option<(String, String)>,
    /// The message for a value it refuses, if shown (style `stop`,
    /// `warning`, `information`; title; text).
    pub error: Option<(String, String, String)>,
}

/// A conditional format: its ranges and rules.
#[derive(Clone, Debug, Default)]
pub struct ConditionalFormat {
    /// Import identity within a worksheet, retained on updates and cleared on additions.
    /// It links opaque XML to the same rule group; it is not workbook content.
    pub source_index: Option<usize>,
    pub ranges: Vec<CellRange>,
    pub rules: Vec<CfRule>,
}

impl PartialEq for ConditionalFormat {
    fn eq(&self, other: &Self) -> bool { self.ranges == other.ranges && self.rules == other.rules }
}
impl Eq for ConditionalFormat {}

/// A rule of a conditional format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CfRule {
    pub kind: CfKind,
    /// Lower is first.
    pub priority: u32,
    /// `stopIfTrue`: no rule after it applies where it does.
    pub stop: bool,
    /// The format it gives (`dxf`).
    pub style: Option<CfStyle>,
}

/// What a rule tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CfKind {
    /// `cellIs`: the value against one or two formulas (`lessThan`, `between`, …).
    CellIs { operator: String, formulas: Vec<String> },
    /// A formula true for the cell (`expression`, and the text, blank,
    /// error and date rules, whose formula Excel writes): relative to the
    /// first range's top-left cell.
    Expression(String),
    /// `timePeriod`: date-based conditional formatting (yesterday, today, tomorrow, etc.).
    TimePeriod { period: String, formula: String },
    Top10 { rank: u32, percent: bool, bottom: bool },
    AboveAverage { above: bool, equal: bool, std_dev: u32 },
    Unique,
    Duplicate,
    /// Colours at two or three points.
    ColorScale(Vec<(CfValue, crate::drawing::Rgb)>),
    DataBar {
        min: CfValue,
        max: CfValue,
        color: crate::drawing::Rgb,
        gradient: bool,
        negative_color: Option<crate::drawing::Rgb>,
        axis_color: Option<crate::drawing::Rgb>,
        axis_position: Option<DataBarAxisPosition>,
        min_length: Option<u32>,
        max_length: Option<u32>,
        /// The bar's border colour (`x14:borderColor`; `border="1"`): `None` for no border.
        border_color: Option<crate::drawing::Rgb>,
        /// The border of a negative bar (`x14:negativeBorderColor`); the positive one's when `None`.
        negative_border_color: Option<crate::drawing::Rgb>,
        /// Which way the bars grow (`x14:dataBar/@direction`).
        direction: DataBarDirection,
    },
    IconSet {
        set: String,
        values: Vec<CfValue>,
        reverse: bool,
        show_value: bool,
        custom: bool,
        icons: Vec<Option<CfIcon>>,
    },
    /// A rule not drawn yet.
    Other,
}

/// An individual icon specification for a conditional formatting icon set (`x14:cfIcon`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CfIcon {
    /// The icon set name (e.g. "3TrafficLights1", or "NoIcon" for no icon).
    pub set: String,
    /// Icon index within the set (0-based, or -1 for NoIcon).
    pub id: i32,
}

impl CfIcon {
    /// An icon slot that draws nothing (NoIcon).
    pub fn no_icon() -> Self {
        Self { set: "NoIcon".into(), id: -1 }
    }

    /// An icon slot with a specific set and index.
    pub fn new(set: impl Into<String>, id: i32) -> Self {
        Self { set: set.into(), id }
    }

    /// Whether this icon slot represents "no icon".
    pub fn is_no_icon(&self) -> bool {
        self.id < 0 || self.set == "NoIcon"
    }
}

/// The position of the vertical axis in a conditional formatting data bar (`x14:axisPosition`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DataBarAxisPosition {
    /// Placed automatically based on positive and negative values.
    #[default]
    Automatic,
    /// Placed at the midpoint (50%) of the cell.
    Midpoint,
    /// No axis displayed (bars drawn from left to right regardless).
    None,
}

/// Which way a data bar grows (`x14:dataBar/@direction`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DataBarDirection {
    /// With the text (left to right in the languages here).
    #[default]
    Context,
    LeftToRight,
    RightToLeft,
}

impl CfKind {
    /// Create a standard data bar rule with default settings.
    pub fn data_bar(min: CfValue, max: CfValue, color: crate::drawing::Rgb, gradient: bool) -> Self {
        Self::DataBar {
            min,
            max,
            color,
            gradient,
            negative_color: None,
            axis_color: None,
            axis_position: None,
            min_length: None,
            max_length: None,
            border_color: None,
            negative_border_color: None,
            direction: DataBarDirection::Context,
        }
    }

    /// Create a standard icon set rule with default settings.
    pub fn icon_set(set: impl Into<String>, values: Vec<CfValue>, reverse: bool, show_value: bool) -> Self {
        Self::IconSet {
            set: set.into(),
            values,
            reverse,
            show_value,
            custom: false,
            icons: Vec::new(),
        }
    }
}

/// A point of a colour scale, data bar or icon set (`cfvo`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CfValue {
    /// `min`, `max`, `num`, `percent`, `percentile`, `formula`, `autoMin`, `autoMax`.
    pub kind: String,
    pub value: Option<String>,
    /// `gte` (the default) or `gt` for icon sets.
    pub gte: bool,
}

/// A conditional format's format (`dxf`): what it sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CfStyle {
    pub color: Option<crate::drawing::Rgb>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<CellUnderline>,
    pub strike: Option<bool>,
    pub fill: Option<crate::drawing::Rgb>,
    pub gradient: Option<CellGradient>,
    pub pattern: Option<(String, crate::drawing::Rgb, crate::drawing::Rgb)>,
    pub vert_align: Option<VerticalPosition>,
    /// Left, right, top, bottom.
    pub borders: [Option<Border>; 4],
    pub number_format: Option<String>,
}

/// Header and footer settings for printing a sheet (`<headerFooter>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetHeaderFooter {
    /// Different header/footer on the first page (`differentFirst="1"`).
    pub different_first: bool,
    /// Different headers/footers for odd and even pages (`differentOddEven="1"`).
    pub different_odd_even: bool,
    /// Scale header and footer with the document (`scaleWithDoc="1"`, default true).
    pub scale_with_doc: bool,
    /// Align header and footer margins with page margins (`alignWithMargins="1"`, default true).
    pub align_with_margins: bool,
    pub odd_header: Option<String>,
    pub odd_footer: Option<String>,
    pub even_header: Option<String>,
    pub even_footer: Option<String>,
    pub first_header: Option<String>,
    pub first_footer: Option<String>,
}

impl SheetHeaderFooter {
    pub fn is_empty(&self) -> bool {
        self.odd_header.is_none()
            && self.odd_footer.is_none()
            && self.even_header.is_none()
            && self.even_footer.is_none()
            && self.first_header.is_none()
            && self.first_footer.is_none()
            && !self.different_first
            && !self.different_odd_even
            && self.scale_with_doc
            && self.align_with_margins
    }
}

impl Default for SheetHeaderFooter {
    fn default() -> Self {
        Self {
            different_first: false,
            different_odd_even: false,
            scale_with_doc: true,
            align_with_margins: true,
            odd_header: None,
            odd_footer: None,
            even_header: None,
            even_footer: None,
            first_header: None,
            first_footer: None,
        }
    }
}

/// How a sheet is printed: its paper, margins, scale and page breaks
/// (`pageSetup`, `pageMargins`, `printOptions`, `headerFooter`, `rowBreaks`, `colBreaks`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetPrint {
    /// The paper's width and height, upright.
    pub paper: (Emu, Emu),
    pub landscape: bool,
    /// The scale in percent.
    pub scale: u32,
    /// Shrunk to fit this many pages across and down (0: as many as it
    /// takes), instead of `scale`.
    pub fit_to: Option<(u32, u32)>,
    /// Left, right, top and bottom margins.
    pub margins: [Emu; 4],
    /// Header and footer margins.
    pub header_margin: Emu,
    pub footer_margin: Emu,
    /// Centred across and down the page.
    pub centered: (bool, bool),
    /// Print grid lines (`gridLines="1"`).
    pub grid_lines: bool,
    /// Print row and column headings (`headings="1"`).
    pub headings: bool,
    /// Print in black and white (`blackAndWhite="1"`).
    pub black_and_white: bool,
    /// Draft quality printing (`draft="1"`).
    pub draft: bool,
    /// How cell errors are printed: "displayed", "blank", "dash", "NA".
    pub cell_errors: Option<String>,
    /// Starting page number for the sheet (`firstPageNumber="N"`).
    pub first_page_number: Option<u32>,
    /// Whether to use `first_page_number` (`useFirstPageNumber="1"`).
    pub use_first_page_number: bool,
    /// Pages go across first, then down (`pageOrder="overThenDown"`);
    /// Excel's default is down, then across.
    pub over_then_down: bool,
    /// Headers and footers.
    pub header_footer: SheetHeaderFooter,
    /// The rows and columns manual page breaks come before (0-based).
    pub row_breaks: Vec<u32>,
    pub column_breaks: Vec<u32>,
    /// The file names Letter (or no paper), is not fitted to pages, and the printer is set for A4: Excel for Windows breaks
    /// the pages where Letter's would and draws them on the A4 paper at 0.95 of the scale (rounded down to a whole percent), in
    /// the middle of the printable height, from the left margin. Read from the file; not saved.
    pub letter_on_a4: bool,
}

impl SheetPrint {
    /// A4.
    pub const A4: (Emu, Emu) = (210 * 36_000, 297 * 36_000);
    /// はがき (100 x 148 mm).
    pub const HAGAKI: (Emu, Emu) = (100 * 36_000, 148 * 36_000);
    /// 往復はがき (200 x 148 mm).
    pub const OFUKU_HAGAKI: (Emu, Emu) = (200 * 36_000, 148 * 36_000);

    /// The paper's width and height as printed (turned when landscape).
    pub fn page_size(&self) -> (Emu, Emu) {
        if self.landscape { (self.paper.1, self.paper.0) } else { self.paper }
    }
}

impl Default for SheetPrint {
    /// Excel's defaults: A4 upright at 100%, with the "Normal" margins
    /// (0.7 inch at the sides, 0.75 inch at the top and bottom, 0.3 inch for header/footer).
    fn default() -> Self {
        let inch = crate::geometry::EMU_PER_INCH;
        Self {
            paper: Self::A4,
            landscape: false,
            scale: 100,
            fit_to: None,
            margins: [inch * 7 / 10, inch * 7 / 10, inch * 3 / 4, inch * 3 / 4],
            header_margin: inch * 3 / 10,
            footer_margin: inch * 3 / 10,
            centered: (false, false),
            grid_lines: false,
            headings: false,
            black_and_white: false,
            draft: false,
            cell_errors: None,
            first_page_number: None,
            use_first_page_number: false,
            over_then_down: false,
            header_footer: SheetHeaderFooter::default(),
            row_breaks: Vec::new(),
            column_breaks: Vec::new(),
            letter_on_a4: false,
        }
    }
}

impl Sheet {
    /// The formula of the cell at (`row`, `column`) as Excel's bar and an edit
    /// show it: a this-row reference of the table the cell is in is `[@Qty]`.
    pub fn formula_shown(&self, row: u32, column: u32, formula: &str) -> String {
        let table = self.tables.iter().find(|t| t.range.first_row <= row && row <= t.range.last_row && t.range.first_column <= column && column <= t.range.last_column).map(|t| t.name.as_str());
        ibis_calc::formula::this_row_to_display(formula, table)
    }

    /// The cell at a row and column, if it holds a value or a format.
    pub fn cell(&self, row: u32, column: u32) -> Option<&SheetCell> {
        self.cells.binary_search_by_key(&(row, column), |c| (c.row, c.column)).ok().map(|i| &self.cells[i])
    }

    /// The comment at a row and column, if one exists.
    pub fn comment_at(&self, row: u32, column: u32) -> Option<&SheetComment> {
        self.comments.iter().find(|c| c.row == row && c.column == column)
    }

    /// The threaded comment at a row and column, if one exists.
    pub fn thread_at(&self, row: u32, column: u32) -> Option<&ThreadedComment> {
        self.threads.iter().find(|t| t.row == row && t.column == column)
    }

    /// The mutable threaded comment at a row and column, if one exists.
    pub fn thread_at_mut(&mut self, row: u32, column: u32) -> Option<&mut ThreadedComment> {
        self.threads.iter_mut().find(|t| t.row == row && t.column == column)
    }

    /// The merged range whose area holds this cell.
    pub fn merge_at(&self, row: u32, column: u32) -> Option<CellRange> {
        self.merges
            .iter()
            .find(|m| (m.first_row..=m.last_row).contains(&row) && (m.first_column..=m.last_column).contains(&column))
            .copied()
    }

    /// The column and row at a point (EMU from the sheet's top-left).
    pub fn cell_at(&self, x: Emu, y: Emu) -> Option<(u32, u32)> {
        if x < 0 || y < 0 {
            return None;
        }
        let col_offsets = self.column_offsets();
        let last_x = *col_offsets.last().unwrap_or(&0);
        let col = if x < last_x {
            col_offsets.partition_point(|o| *o <= x).saturating_sub(1) as u32
        } else {
            let def = self.default_column.max(1);
            let extra = (x - last_x) / def;
            ((col_offsets.len().saturating_sub(1)) as u64 + extra as u64).min(16_383) as u32
        };

        let row_offsets = self.row_offsets();
        let last_y = *row_offsets.last().unwrap_or(&0);
        let row = if y < last_y {
            row_offsets.partition_point(|o| *o <= y).saturating_sub(1) as u32
        } else {
            let def = self.default_row.max(1);
            let extra = (y - last_y) / def;
            ((row_offsets.len().saturating_sub(1)) as u64 + extra as u64).min(1_048_575) as u32
        };
        Some((col, row))
    }

    /// Offsets of the columns' left edges, with the total width at the end.
    pub fn column_offsets(&self) -> Vec<Emu> {
        offsets(&self.columns)
    }

    /// Offsets of the rows' top edges, with the total height at the end.
    pub fn row_offsets(&self) -> Vec<Emu> {
        offsets(&self.rows)
    }

    /// Whether the row is hidden (height is 0 or recorded in hidden_rows).
    pub fn is_row_hidden(&self, row: u32) -> bool {
        self.rows.get(row as usize) == Some(&0) || self.hidden_rows.contains_key(&row)
    }

    /// Whether the column is hidden (width is 0 or recorded in hidden_columns).
    pub fn is_col_hidden(&self, col: u32) -> bool {
        self.columns.get(col as usize) == Some(&0) || self.hidden_columns.contains_key(&col)
    }

    /// The EMU bounds of cell `(column, row)`.
    pub fn cell_bounds_emu(&self, column: u32, row: u32) -> crate::geometry::EmuRect {
        let xs = self.column_offsets();
        let ys = self.row_offsets();
        let col = column as usize;
        let r = row as usize;
        let x_at = |idx: usize| -> Emu {
            if idx < xs.len() {
                xs[idx]
            } else {
                let last = xs.last().copied().unwrap_or(0);
                last + (idx - xs.len().saturating_sub(1)) as i64 * self.default_column
            }
        };
        let y_at = |idx: usize| -> Emu {
            if idx < ys.len() {
                ys[idx]
            } else {
                let last = ys.last().copied().unwrap_or(0);
                last + (idx - ys.len().saturating_sub(1)) as i64 * self.default_row
            }
        };
        let x0 = x_at(col);
        let x1 = x_at(col + 1);
        let y0 = y_at(r);
        let y1 = y_at(r + 1);
        crate::geometry::EmuRect::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1))
    }

    /// The picture in cell `(column, row)`, if any.
    pub fn cell_image_at(&self, row: u32, column: u32) -> Option<&CellImage> {
        self.cell_images.iter().find(|i| i.row == row && i.column == column)
    }
}

fn offsets(sizes: &[Emu]) -> Vec<Emu> {
    let mut out = Vec::with_capacity(sizes.len() + 1);
    let mut at = 0;
    out.push(0);
    for size in sizes {
        at += size;
        out.push(at);
    }
    out
}

/// A gradient fill of a cell (`gradientFill`).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CellGradient {
    Linear { degree: u16, stops: Vec<(u16, Rgb)> },
    Path { path: GradientPath, focus: [u32; 4], stops: Vec<(u16, Rgb)> },
}

impl CellGradient {
    /// Convert to drawing [`Fill`] for gradient rendering.
    pub fn to_fill(&self) -> Fill {
        match self {
            Self::Linear { degree, stops } => {
                let angle = *degree as i32 * 60_000;
                let stops = stops.iter().map(|(pos, col)| GradientStop {
                    position: *pos as u32 * 100,
                    color: Color::opaque(*col),
                }).collect();
                Fill::Linear { angle, scaled: false, stops }
            }
            Self::Path { path, focus, stops } => {
                let focus = focus.map(|v| v as i32);
                let stops = stops.iter().map(|(pos, col)| GradientStop {
                    position: *pos as u32 * 100,
                    color: Color::opaque(*col),
                }).collect();
                Fill::Path { path: *path, focus, stops }
            }
        }
    }
}

/// A workbook's protection (`workbookProtection`): whether structure and/or
/// windows are locked, and password hash if protected with a password.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkbookProtection {
    /// `workbookAlgorithmName` (`SHA-512`), `workbookHashValue` and `workbookSaltValue` (Base64)
    /// and `workbookSpinCount`, as Excel writes them.
    pub hash: Option<PasswordHash>,
    /// Legacy 16-bit hash (`workbookPassword`, hex).
    pub legacy: Option<String>,
    /// Whether the workbook structure (sheets added, deleted, renamed,
    /// hidden, moved) is locked (`lockStructure`).
    pub lock_structure: bool,
    /// Whether the workbook windows are locked (`lockWindows`).
    pub lock_windows: bool,
    /// Whether revision tracking is locked (`lockRevision`).
    pub lock_revision: bool,
}

/// A sheet's protection (`sheetProtection`): its password, if any, as a
/// hash (never the password itself), and what may still be done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SheetProtection {
    /// `algorithmName` (`SHA-512`), `hashValue` and `saltValue` (Base64)
    /// and `spinCount`, as Excel writes them.
    pub hash: Option<PasswordHash>,
    /// The old 16-bit hash (`password`, 4 hex digits) of older files.
    pub legacy: Option<String>,
    /// What may be done while protected: each is Excel's attribute (which
    /// says that it is *not* allowed when 1, except the two selections).
    pub allowed: ProtectionAllowed,
}

/// A protected range on a worksheet (`protectedRange` in `protectedRanges`).
/// Allows specific users or anyone with the password to edit the range even
/// when the sheet is protected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProtectedRange {
    /// Title / name of the range (e.g. "Range1").
    pub name: String,
    /// The cell ranges covered.
    pub ranges: Vec<CellRange>,
    /// Password hash (SHA-512, etc.), if password-protected.
    pub hash: Option<PasswordHash>,
    /// Legacy 16-bit password hash, if present.
    pub legacy: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PasswordHash {
    pub algorithm: String,
    pub hash: String,
    pub salt: String,
    pub spin_count: u32,
}

/// The operations allowed on a protected sheet (Excel's check boxes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtectionAllowed {
    pub select_locked: bool,
    pub select_unlocked: bool,
    pub format_cells: bool,
    pub format_columns: bool,
    pub format_rows: bool,
    pub insert_columns: bool,
    pub insert_rows: bool,
    pub insert_hyperlinks: bool,
    pub delete_columns: bool,
    pub delete_rows: bool,
    pub sort: bool,
    pub auto_filter: bool,
    pub pivot_tables: bool,
    pub objects: bool,
    pub scenarios: bool,
}

impl Default for ProtectionAllowed {
    /// Excel's default: selecting cells only.
    fn default() -> Self {
        Self {
            select_locked: true,
            select_unlocked: true,
            format_cells: false,
            format_columns: false,
            format_rows: false,
            insert_columns: false,
            insert_rows: false,
            insert_hyperlinks: false,
            delete_columns: false,
            delete_rows: false,
            sort: false,
            auto_filter: false,
            pivot_tables: false,
            objects: false,
            scenarios: false,
        }
    }
}

#[cfg(test)]
mod size_tests {
    /// A million-row CSV is 21 million cells: every byte of a cell is 21 MB of memory.
    #[test]
    fn a_cell_stays_small() {
        assert!(std::mem::size_of::<super::SheetCell>() <= 112, "SheetCell is {} bytes", std::mem::size_of::<super::SheetCell>());
    }
}
